//! Breakable structures (server), after tg `code/game/atom/atom_defense.dm` (`take_damage`,
//! `run_atom_armor`), `code/game/objects/obj_defense.dm` (`atom_destruction` -> `deconstruct`)
//! and `code/_onclick/item_attack.dm` (`/obj/attackby` -> `attack_atom` -> `/obj/attacked_by`).
//!
//! Windows, grilles and girders carry [`Integrity`]: an item hit takes its `force` as melee
//! damage through `damage_deflection` and the `melee` armor rating; `integrity_failure` breaks
//! a grille (it drops a rod and lets things through), 0 destroys the structure into its debris
//! (a window into shards, rods for reinforced ones, a grille into rods, a girder into a rod or a
//! sheet). A destroyed window takes its `AtmosBlocker` with it, so its tile opens to the air.
//!
//! Every structure and machine also gets [`Anchored`] (tg `anchored`), which pulling reads; a
//! dense loose one is [`Pushable`] (`sync_pushable`).
// shortcut: only windows, grilles, girders, frames, gas machines, vendors, turrets and cameras
// take damage; other
// structures and machines (tables, closets, doors) wait for their own
// `atom_break`/`atom_deconstruct`.

use bevy::prelude::*;
use ss13_atmos::{AtmosBlocker, AtmosPass};
use ss13_mapload::{DmAtom, DmItem, DmName, DmObj, IconIndex};
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_shared::{Appearance, TilePos};
use ss13_world::{Density, Mover, Pushable, SpatialIndex};

use crate::combat::{CombatRng, NextMove};
use crate::health::{COMBAT_MESSAGE_RANGE, DamageType};
use crate::server::PlayerMob;
use crate::tools::Actor;

/// `INDESTRUCTIBLE` (`resistance_flags`).
pub const INDESTRUCTIBLE: u32 = 1 << 6;
/// tg `/obj/structure/alien` (weeds, resin, eggs).
const ALIEN_STRUCTURE: &str = "/obj/structure/alien";
/// `CAN_BE_HIT` (`obj_flags`).
pub const CAN_BE_HIT: u32 = 1 << 1;
/// `CONDUCTS_ELECTRICITY` (`obj_flags`).
pub const CONDUCTS_ELECTRICITY: u32 = 1 << 12;
/// `DAMAGE_PRECISION`.
pub const DAMAGE_PRECISION: f32 = 0.1;
/// `WINDOW_OUT_OF_FRAME`, `WINDOW_IN_FRAME`, `WINDOW_SCREWED_TO_FRAME`, `RWINDOW_SECURE`.
pub const WINDOW_OUT_OF_FRAME: i32 = 0;
pub const WINDOW_IN_FRAME: i32 = 1;
pub const WINDOW_SCREWED_TO_FRAME: i32 = 2;
pub const RWINDOW_SECURE: i32 = 8;
/// `GIRDER_NORMAL`, `GIRDER_REINF_STRUTS`, `GIRDER_REINF`, `GIRDER_DISPLACED`.
pub const GIRDER_NORMAL: i32 = 0;
pub const GIRDER_REINF_STRUTS: i32 = 1;
pub const GIRDER_REINF: i32 = 2;
pub const GIRDER_DISPLACED: i32 = 3;

pub const WINDOW: &str = "/obj/structure/window";
pub const GRILLE: &str = "/obj/structure/grille";
pub const GIRDER: &str = "/obj/structure/girder";
const RODS: &str = "/obj/item/stack/rods";
/// Gas containers ([`crate::atmos_machinery::damage`]).
const CANISTER: &str = "/obj/machinery/portable_atmospherics/canister";
const STATIONARY_TANK: &str = "/obj/machinery/atmospherics/components/tank";
/// `/obj/structure/frame`: machine and computer frames (`crate::machine_frame`).
const FRAME: &str = "/obj/structure/frame";
const VENT_PUMP: &str = "/obj/machinery/atmospherics/components/unary/vent_pump";
const SHARD: &str = "/obj/item/shard";
const VENDING: &str = "/obj/machinery/vending";

/// `/a/b` is under `/a` and `/a/b`, not under `/a/bc`.
pub fn is_under(path: &str, root: &str) -> bool {
    path.strip_prefix(root)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

/// tg `anchored` of a structure or machine, live.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Anchored(pub bool);

/// Whether `e` is anchored; entities without the component are not.
pub fn anchored(world: &World, e: Entity) -> bool {
    world.get::<Anchored>(e).is_some_and(|a| a.0)
}

/// tg `atom_integrity` and `broken` of a breakable structure.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Integrity {
    pub current: f32,
    pub broken: bool,
}

/// The live `state` var of girders and windows (`GIRDER_*`, `WINDOW_*`).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObjState(pub i32);

/// What happened to a structure that took damage.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    /// After armor (0: deflected).
    pub damage: f32,
    pub broke: bool,
    pub destroyed: bool,
}

/// tg `run_atom_armor` for `MELEE` damage of `damtype`.
pub fn armor_damage(obj: &DmObj, amount: f32, damtype: DamageType) -> f32 {
    if amount < obj.damage_deflection {
        return 0.0;
    }
    if !matches!(damtype, DamageType::Brute | DamageType::Burn) {
        return 0.0;
    }
    let armor = obj.armor_melee.clamp(0.0, 100.0);
    round_to(amount * (100.0 - armor) * 0.01, DAMAGE_PRECISION)
}

/// DM `round(x, step)`.
fn round_to(x: f32, step: f32) -> f32 {
    (x / step).round() * step
}

/// tg `take_damage` bookkeeping on the numbers alone (no effects): the hit, and the new
/// integrity in `integrity`.
pub fn damage_numbers(
    obj: &DmObj,
    integrity: &mut Integrity,
    amount: f32,
    damtype: DamageType,
) -> Hit {
    let none = Hit {
        damage: 0.0,
        broke: false,
        destroyed: false,
    };
    if obj.resistance_flags & INDESTRUCTIBLE != 0 || integrity.current <= 0.0 {
        return none;
    }
    let damage = armor_damage(obj, amount, damtype);
    if damage < DAMAGE_PRECISION {
        return none;
    }
    let previous = integrity.current;
    integrity.current = (previous - damage).max(0.0);
    let failure = obj.integrity_failure * obj.max_integrity;
    Hit {
        damage,
        broke: obj.integrity_failure > 0.0 && previous > failure && integrity.current <= failure,
        destroyed: integrity.current <= 0.0 && previous > 0.0,
    }
}

pub struct ObjDamageServerPlugin;

impl Plugin for ObjDamageServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(add_obj_state).add_systems(
            PostUpdate,
            sync_pushable.before(ss13_net::server::NetServerSystems::Send),
        );
    }
}

/// Keeps [`Pushable`] on the dense structures and machines that are not anchored, do not walk
/// and give way to the default force (tg `PushAM` refuses `anchored` and
/// `force < move_resist * MOVE_FORCE_PUSH_RATIO`), whoever changes the anchoring or the density;
/// the client's movement prediction reads it.
fn sync_pushable(
    changed: Query<Entity, Or<(Changed<Anchored>, Added<Density>, Added<Mover>)>>,
    mut lost_density: RemovedComponents<Density>,
    mut lost_mover: RemovedComponents<Mover>,
    objs: Query<(
        &Anchored,
        Option<&DmObj>,
        Has<Density>,
        Has<Mover>,
        Has<Pushable>,
    )>,
    mut commands: Commands,
) {
    let touched = changed
        .iter()
        .chain(lost_density.read())
        .chain(lost_mover.read());
    for e in touched {
        let Ok((anchored, obj, dense, mover, pushable)) = objs.get(e) else {
            continue;
        };
        let light = obj.is_none_or(|o| {
            crate::fire::MOVE_FORCE_DEFAULT
                >= f64::from(o.move_resist) * crate::fire::MOVE_FORCE_PUSH_RATIO
        });
        let loose = dense && !anchored.0 && !mover && light;
        if loose && !pushable {
            commands.entity(e).try_insert(Pushable);
        } else if !loose && pushable {
            commands.entity(e).try_remove::<Pushable>();
        }
    }
}

/// tg `Initialize` of structures and machines: `anchored`, full integrity, and a window's state
/// from its anchoring (`/obj/structure/window/Initialize`).
fn add_obj_state(add: On<Add<DmObj>>, objs: Query<&DmObj>, mut commands: Commands) {
    let Ok(obj) = objs.get(add.entity) else {
        return;
    };
    let mut entity = commands.entity(add.entity);
    // Spawners that set these themselves (`replace_girder`, glass on a grille) win.
    entity.insert_if_new(Anchored(obj.anchored));
    let breakable = [
        WINDOW,
        GRILLE,
        GIRDER,
        CANISTER,
        STATIONARY_TANK,
        VENT_PUMP,
        FRAME,
        VENDING,
        crate::holosign::HOLOSIGN,
        crate::flora::FLORA,
        crate::defenses::barricades::BARRICADE,
    ]
    .iter()
    .any(|root| is_under(&obj.path, root))
        || crate::decor::breakable(&obj.path)
        || crate::tank_storage::breakable(&obj.path)
        || crate::displaycase::breakable(&obj.path)
        || crate::spawners::breakable(&obj.path)
        || crate::antag::ashwalker::breakable(&obj.path)
        || crate::defenses::server::breakable(&obj.path)
        || is_under(&obj.path, crate::defenses::cameras::CAMERA_ROOT);
    if !breakable {
        return;
    }
    // `/obj/structure/grille/broken/Initialize`: `take_damage(max_integrity * 0.6)` without a
    // damage flag, so armor does not apply.
    entity.insert_if_new(Integrity {
        current: if obj.broken {
            obj.max_integrity * 0.4
        } else {
            obj.max_integrity
        },
        broken: obj.broken,
    });
    let state = match (is_under(&obj.path, WINDOW), obj.reinf, obj.anchored) {
        (true, true, true) => RWINDOW_SECURE,
        (true, false, true) => WINDOW_SCREWED_TO_FRAME,
        _ => obj.state,
    };
    entity.insert_if_new(ObjState(state));
}

/// tg `/obj/attackby` -> `attack_atom` -> `/obj/attacked_by` of the actor's held item on
/// `target`. False when `target` cannot be hit here (the click goes on to other handlers).
pub fn attackby(world: &mut World, actor: Actor, target: Entity) -> bool {
    let Some(item) = actor.item else {
        return false;
    };
    let Some(obj) = world.get::<DmObj>(target).cloned() else {
        return false;
    };
    if obj.obj_flags & CAN_BE_HIT == 0 || world.get::<Integrity>(target).is_none() {
        return false;
    }
    let Some(dm) = world.get::<DmItem>(item).cloned() else {
        return false;
    };
    // `/obj/structure/grille/item_interaction`: a conductive item on a live grille.
    if is_under(&obj.path, GRILLE)
        && world
            .get::<DmObj>(item)
            .is_some_and(|o| o.obj_flags & CONDUCTS_ELECTRICITY != 0)
        && crate::construction::grille_shock(world, target, actor.mob, 70.0)
    {
        return true;
    }
    // `attack_atom`: the item's click cooldown, even for a forceless item.
    let now = world.resource::<Time>().elapsed_secs_f64();
    if let Some(mut next) = world.get_mut::<NextMove>(actor.mob) {
        next.0 = now + f64::from(dm.attack_speed) / 10.0;
    }
    // BandaStation `stamina_cost_per_hit` on `COMSIG_ITEM_ATTACK_ATOM` (the sledgehammers).
    match crate::weapons::stamina_cost::charge(world, actor.mob, item, false) {
        None => {
            crate::tools::tell(world, actor.client, Span::Warning, "нет сил!");
            return true;
        }
        Some(cost) if cost > 0.0 => crate::status_effects::adjust_stamina(world, actor.mob, cost),
        Some(_) => {}
    }
    if dm.force <= 0.0 {
        return true;
    }
    let damtype = DamageType::from_dm(&dm.damtype).unwrap_or(DamageType::Brute);
    let name = |e: Entity| {
        world
            .get::<DmName>(e)
            .map(|n| strip(&n.name).to_owned())
            .or_else(|| world.get::<Name>(e).map(|n| n.as_str().to_owned()))
            .unwrap_or_else(|| "что-то".to_owned())
    };
    let (user, thing, weapon) = (name(actor.mob), name(target), name(item));
    // shortcut: `demolition_mod` and `armour_penetration` are 1 and 0 for every item.
    let hit = take_damage(world, target, dm.force, damtype);
    // `/obj/machinery/vending/attacked_by`: a hit that did damage may tip it or shake goods out.
    if hit.damage > 0.0 && !hit.destroyed {
        crate::vending::tilt::attacked(world, actor.mob, target);
    }
    let tail = if hit.damage > 0.0 {
        ".".to_owned()
    } else {
        ", не оставляя следов!".to_owned()
    };
    // shortcut: tg picks one of the item's `attack_verb_continuous`; "атакует" is its default.
    let pos = world.get::<TilePos>(actor.mob).copied();
    let others = pos.map_or_else(Vec::new, |pos| {
        clients_in_range(world, pos, COMBAT_MESSAGE_RANGE, &[actor.mob])
    });
    let message = |text: String| ChatMessage {
        channel: ChatChannel::Combat,
        segments: vec![ChatSegment::new(Span::Danger, text)],
    };
    world.write_message(ss13_net::chat::server::SendChat {
        clients: others,
        message: message(format!("{user} атакует {thing} с помощью {weapon}{tail}")),
    });
    world.write_message(ss13_net::chat::server::SendChat {
        clients: vec![actor.client],
        message: message(format!("Вы атакуете {thing} с помощью {weapon}{tail}")),
    });
    // `/obj/item/fireaxe/afterattack`: wielded, it destroys windows and grilles in one hit.
    if !hit.destroyed
        && obj.resistance_flags & INDESTRUCTIBLE == 0
        && world.get_entity(target).is_ok()
        && crate::weapons::fireaxe_destroys(world, item, &obj.path)
    {
        deconstruct(world, target, false);
    }
    true
}

/// BYOND `\proper` / `\improper`.
pub fn strip(name: &str) -> &str {
    name.trim_start_matches("\\proper ")
        .trim_start_matches("\\improper ")
}

/// tg `take_damage` with its effects: `atom_break` at `integrity_failure`, `atom_destruction`
/// at 0.
pub fn take_damage(world: &mut World, target: Entity, amount: f32, damtype: DamageType) -> Hit {
    // Blob structures have their own resistances (`crate::antag::blob::structures`).
    use crate::antag::blob::strains::Flag;
    if let Some(hit) =
        crate::antag::blob::structures::damage(world, target, amount, damtype, Flag::Melee)
    {
        return hit;
    }
    let Some(obj) = world.get::<DmObj>(target).cloned() else {
        return Hit {
            damage: 0.0,
            broke: false,
            destroyed: false,
        };
    };
    // `/obj/structure/spider/run_atom_armor` for `MELEE`.
    let amount = if is_under(&obj.path, crate::effects::webs::SPIDER_STRUCTURE) {
        amount * crate::effects::webs::melee_damage_mult(damtype == DamageType::Burn)
    } else if is_under(&obj.path, ALIEN_STRUCTURE) {
        // `/obj/structure/alien/run_atom_armor` for `MELEE`.
        amount
            * if damtype == DamageType::Burn {
                2.0
            } else {
                0.25
            }
    } else {
        amount
    };
    apply_damage(world, target, &obj, amount, damtype)
}

fn apply_damage(
    world: &mut World,
    target: Entity,
    obj: &DmObj,
    amount: f32,
    damtype: DamageType,
) -> Hit {
    // A modular shield field passes its damage to its generator (`take_damage` override).
    let brute_or_burn = matches!(damtype, DamageType::Brute | DamageType::Burn);
    if crate::eng_machines::modular_shield::absorb(world, target, amount, brute_or_burn) {
        return Hit {
            damage: 0.0,
            broke: false,
            destroyed: false,
        };
    }
    let Some(mut integrity) = world.get::<Integrity>(target).copied() else {
        return Hit {
            damage: 0.0,
            broke: false,
            destroyed: false,
        };
    };
    // `camera/run_atom_armor`: a broken camera takes the damage as it is.
    let bare;
    let obj = if integrity.broken
        && world
            .get::<crate::silicon::cameras::Camera>(target)
            .is_some()
    {
        let mut o = obj.clone();
        o.armor_melee = 0.0;
        o.damage_deflection = 0.0;
        bare = o;
        &bare
    } else {
        obj
    };
    let hit = damage_numbers(obj, &mut integrity, amount, damtype);
    if hit.damage <= 0.0 {
        return hit;
    }
    world.entity_mut(target).insert(integrity);
    if hit.broke {
        atom_break(world, target, obj);
    }
    if hit.destroyed {
        deconstruct(world, target, false);
    } else {
        // `porta_turret/take_damage`: sparks and anger.
        crate::defenses::server::damaged(world, target);
        // `clowncar/take_damage`: lube foam.
        crate::cars::damaged(world, target);
        refresh_look(world, target);
    }
    hit
}

/// tg `take_damage(amount)` with no damage flag (`run_atom_armor` skips armor and deflection):
/// brute, with the same break and destruction effects.
pub fn take_damage_flagless(world: &mut World, target: Entity, amount: f32) -> Hit {
    let Some(mut obj) = world.get::<DmObj>(target).cloned() else {
        return Hit {
            damage: 0.0,
            broke: false,
            destroyed: false,
        };
    };
    obj.armor_melee = 0.0;
    obj.damage_deflection = 0.0;
    apply_damage(world, target, &obj, amount, DamageType::Brute)
}

/// tg `take_damage` with an armour rating other than the melee one (a projectile's flag).
pub fn take_damage_armor(
    world: &mut World,
    target: Entity,
    amount: f32,
    damtype: DamageType,
    armor: f32,
) -> Hit {
    let Some(mut obj) = world.get::<DmObj>(target).cloned() else {
        return Hit {
            damage: 0.0,
            broke: false,
            destroyed: false,
        };
    };
    obj.armor_melee = armor;
    apply_damage(world, target, &obj, amount, damtype)
}

/// tg `take_damage(amount, BRUTE, BOMB)`: through the `bomb` armor, no deflection
/// (`/obj/ex_act`).
pub fn take_bomb_damage(world: &mut World, target: Entity, amount: f32) -> Hit {
    use crate::antag::blob::strains::Flag;
    if let Some(hit) =
        crate::antag::blob::structures::damage(world, target, amount, DamageType::Brute, Flag::Bomb)
    {
        return hit;
    }
    let Some(mut obj) = world.get::<DmObj>(target).cloned() else {
        return Hit {
            damage: 0.0,
            broke: false,
            destroyed: false,
        };
    };
    obj.armor_melee = obj.armor_bomb;
    obj.damage_deflection = 0.0;
    apply_damage(world, target, &obj, amount, DamageType::Brute)
}

/// tg `atom_break`: a grille falls apart (`/obj/structure/grille/atom_break`).
fn atom_break(world: &mut World, target: Entity, obj: &DmObj) {
    if crate::atmos_machinery::damage::atom_break(world, target) {
        return;
    }
    if is_under(&obj.path, VENDING) {
        crate::vending::atom_break(world, target);
        return;
    }
    if crate::decor::atom_break(world, target)
        || crate::displaycase::atom_break(world, target)
        || crate::defenses::server::atom_break(world, target)
        || crate::defenses::cameras::atom_break(world, target)
    {
        return;
    }
    if !is_under(&obj.path, GRILLE) {
        return;
    }
    let Some(mut integrity) = world.get_mut::<Integrity>(target) else {
        return;
    };
    if integrity.broken {
        return;
    }
    integrity.broken = true;
    integrity.current = 20.0;
    world.entity_mut(target).remove::<Density>();
    if let Some(&pos) = world.get::<TilePos>(target) {
        let rods = if obj.rods_type.is_empty() {
            RODS
        } else {
            &obj.rods_type
        };
        crate::construction::spawn_stack(world, rods, pos, 1);
    }
}

/// A grille's damage states (`/obj/structure/grille/update_icon_state`).
fn refresh_look(world: &mut World, target: Entity) {
    let (Some(obj), Some(integrity)) = (
        world.get::<DmObj>(target).cloned(),
        world.get::<Integrity>(target).copied(),
    ) else {
        return;
    };
    if !is_under(&obj.path, GRILLE) {
        return;
    }
    let base = "grille";
    let state = if integrity.broken {
        format!("broken{base}")
    } else if obj.max_integrity > 0.0 && integrity.current / obj.max_integrity <= 0.5 {
        let n = world
            .get_resource_mut::<CombatRng>()
            .map_or(0, |mut rng| rng.range(0, 3));
        format!("{base}50_{n}")
    } else {
        base.to_owned()
    };
    set_icon_state(world, target, &state);
}

/// Sets the drawn state of `target` to `state` of its own `.dmi`, if the state exists.
pub fn set_icon_state(world: &mut World, target: Entity, state: &str) {
    let Some(icon) = world.get::<DmAtom>(target).map(|a| a.icon.clone()) else {
        return;
    };
    let Some(id) = world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(&icon, state))
    else {
        return;
    };
    if let Some(mut appearance) = world.get_mut::<Appearance>(target)
        && appearance.icon_state != id
    {
        appearance.icon_state = id;
    }
}

/// tg `deconstruct(disassembled)`: the structure's debris (`atom_deconstruct`), then it is
/// gone and its neighbours resmooth.
pub fn deconstruct(world: &mut World, target: Entity, disassembled: bool) {
    let (Some(obj), Some(&pos)) = (
        world.get::<DmObj>(target).cloned(),
        world.get::<TilePos>(target),
    ) else {
        return;
    };
    let broken = world.get::<Integrity>(target).is_some_and(|i| i.broken);
    let machine = crate::machine_frame::buildable_machine(world, target).is_some();
    if crate::atmos_machinery::damage::deconstruct(world, target, disassembled) {
        // Its gas and parts are out; a tank's group keeps the shared air.
    } else if is_under(&obj.path, WINDOW) {
        if !disassembled {
            // `spawn_debris`: glass shards, rods of the reinforcement.
            let shards = if obj.fulltile { 2 } else { 1 };
            for _ in 0..shards {
                crate::construction::spawn_obj(world, SHARD, pos);
            }
            if obj.reinf {
                crate::construction::spawn_stack(world, RODS, pos, shards);
            }
        }
    } else if is_under(&obj.path, GRILLE) {
        let amount = if broken { 1 } else { obj.rods_amount.max(1) };
        let rods = if obj.rods_type.is_empty() {
            RODS
        } else {
            &obj.rods_type
        };
        crate::construction::spawn_stack(world, rods, pos, amount);
    } else if is_under(&obj.path, GIRDER) {
        if disassembled {
            crate::construction::spawn_stack(world, &obj.stack_type, pos, obj.stack_amount);
        } else {
            // `pick(/obj/item/stack/rods, stack_type)`, one unit.
            let rod = world
                .get_resource_mut::<CombatRng>()
                .is_none_or(|mut rng| rng.chance(50.0));
            let remains = if rod { RODS } else { obj.stack_type.as_str() };
            crate::construction::spawn_stack(world, remains, pos, 1);
        }
    } else if is_under(&obj.path, FRAME) {
        crate::machine_frame::frame_deconstruct(world, target, pos, disassembled);
    } else if is_under(&obj.path, crate::explosions::ex_act::LATTICE) {
        crate::explosions::ex_act::lattice_debris(world, &obj.path, pos);
    } else if crate::furniture::deconstruct(world, target, &obj, pos)
        || crate::plumbing::deconstruct(world, target, &obj, pos)
        || crate::decor::deconstruct(world, target, &obj, pos, disassembled)
        || crate::tank_storage::deconstruct(world, target, &obj, pos)
        || crate::displaycase::deconstruct(world, target, &obj, pos, disassembled)
        || crate::flora::deconstruct(world, target, &obj)
        || crate::defenses::barricades::deconstruct(world, &obj, pos)
        || crate::chem_machines::separator::deconstruct(world, target, &obj, pos)
        || crate::antag::ashwalker::deconstruct(world, &obj, pos)
        || (!disassembled && crate::defenses::cameras::destroyed(world, target, pos))
        || crate::cars::destroyed(world, target, pos)
    {
        // A chair's, bed's, bathroom fixture's, decor's or flora's debris is out.
    }
    // A vendor's stock into its canister and its returned items out, before its parts leave.
    crate::vending::on_deconstruction(world, target, pos, disassembled);
    if machine {
        crate::machine_frame::on_deconstruction(world, target, pos, disassembled);
    }
    crate::power::shields::on_destroy(world, target);
    crate::eng_machines::atmos_shield::on_destroy(world, target);
    crate::eng_machines::modular_shield::on_destroy(world, target);
    crate::traps::deconstruct(world, target);
    world.despawn(target);
    crate::construction::tile_changed(world, pos);
}

/// tg `set_anchored`; windows block air only while anchored (`can_atmos_pass`).
pub fn set_anchored(world: &mut World, target: Entity, anchored: bool) {
    world.entity_mut(target).insert(Anchored(anchored));
    let window = world
        .get::<DmObj>(target)
        .is_some_and(|o| is_under(&o.path, WINDOW));
    if window {
        if anchored {
            world
                .entity_mut(target)
                .insert(AtmosBlocker(AtmosPass::Proc));
        } else {
            world.entity_mut(target).remove::<AtmosBlocker>();
        }
    }
}

/// Clients of player mobs within `range` of `pos` that perceive it (an AI eye only where its
/// cameras see: `ss13_chat::server::MaskedSenses`), except `exclude`: tg `visible_message` and
/// `audible_message` hearers.
// shortcut: range only, no line of sight.
pub(crate) fn clients_in_range(
    world: &World,
    pos: TilePos,
    range: u32,
    exclude: &[Entity],
) -> Vec<Entity> {
    let Some(index) = world.get_resource::<SpatialIndex>() else {
        return Vec::new();
    };
    index
        .in_radius(pos, range)
        .filter(|e| !exclude.contains(e))
        .filter(|&e| ss13_chat::server::perceives_in(world, e, pos))
        .filter_map(|e| world.get::<PlayerMob>(e).map(|p| p.client))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window() -> DmObj {
        DmObj {
            path: "/obj/structure/window/fulltile".into(),
            max_integrity: 100.0,
            armor_melee: 50.0,
            obj_flags: CAN_BE_HIT,
            fulltile: true,
            ..default()
        }
    }

    #[test]
    fn melee_armor_and_deflection() {
        let w = window();
        // A toolbox (13 brute) against melee 50: 6.5.
        assert_eq!(armor_damage(&w, 13.0, DamageType::Brute), 6.5);
        assert_eq!(armor_damage(&w, 15.0, DamageType::Burn), 7.5);
        assert_eq!(armor_damage(&w, 15.0, DamageType::Tox), 0.0);
        // A reinforced window deflects anything under 11.
        let r = DmObj {
            damage_deflection: 11.0,
            armor_melee: 80.0,
            ..w.clone()
        };
        assert_eq!(armor_damage(&r, 10.0, DamageType::Brute), 0.0);
        assert!((armor_damage(&r, 13.0, DamageType::Brute) - 2.6).abs() < 1e-5);
    }

    #[test]
    fn a_fulltile_window_takes_sixteen_toolbox_hits() {
        let w = window();
        let mut integrity = Integrity {
            current: w.max_integrity,
            broken: false,
        };
        let mut hits = 0;
        loop {
            hits += 1;
            let hit = damage_numbers(&w, &mut integrity, 13.0, DamageType::Brute);
            assert!(!hit.broke, "windows have no integrity_failure");
            if hit.destroyed {
                break;
            }
        }
        // ceil(100 / 6.5)
        assert_eq!(hits, 16);
        let after = damage_numbers(&w, &mut integrity, 13.0, DamageType::Brute);
        assert_eq!(after.damage, 0.0, "nothing hits a destroyed window again");
    }

    #[test]
    fn a_grille_breaks_at_forty_percent() {
        let g = DmObj {
            path: "/obj/structure/grille".into(),
            max_integrity: 50.0,
            integrity_failure: 0.4,
            armor_melee: 50.0,
            ..default()
        };
        let mut integrity = Integrity {
            current: 50.0,
            broken: false,
        };
        // 10 force -> 5 per hit: 45, 40, 35, 30, 25, 20 (<= 20 breaks).
        let hits: Vec<bool> = (0..6)
            .map(|_| damage_numbers(&g, &mut integrity, 10.0, DamageType::Brute).broke)
            .collect();
        assert_eq!(hits, [false, false, false, false, false, true]);
        let indestructible = DmObj {
            resistance_flags: INDESTRUCTIBLE,
            ..g
        };
        let mut full = Integrity {
            current: 50.0,
            broken: false,
        };
        let hit = damage_numbers(&indestructible, &mut full, 100.0, DamageType::Brute);
        assert_eq!((hit.damage, full.current), (0.0, 50.0));
    }

    #[test]
    fn pushable_follows_anchoring_and_density() {
        let mut app = App::new();
        app.add_systems(Update, sync_pushable);
        let e = app.world_mut().spawn((Anchored(false), Density)).id();
        let anchored = app.world_mut().spawn((Anchored(true), Density)).id();
        let obj = |move_resist| DmObj {
            move_resist,
            ..default()
        };
        // An unbolted door (2999) is beyond the default force; a default object (1000) is not.
        let heavy = app
            .world_mut()
            .spawn((Anchored(false), Density, obj(2999.0)))
            .id();
        let normal = app
            .world_mut()
            .spawn((Anchored(false), Density, obj(1000.0)))
            .id();
        app.update();
        let pushable = |app: &App, e| app.world().get::<Pushable>(e).is_some();
        assert!(pushable(&app, e));
        assert!(!pushable(&app, anchored));
        assert!(!pushable(&app, heavy), "too heavy");
        assert!(pushable(&app, normal));
        app.world_mut().entity_mut(e).insert(Anchored(true));
        app.update();
        assert!(!pushable(&app, e), "wrenched down");
        app.world_mut().entity_mut(e).insert(Anchored(false));
        app.update();
        assert!(pushable(&app, e), "unwrenched");
        app.world_mut().entity_mut(e).remove::<Density>();
        app.update();
        assert!(!pushable(&app, e), "no longer dense");
        app.world_mut()
            .entity_mut(e)
            .insert((Density, Mover::new(0.0)));
        app.update();
        assert!(!pushable(&app, e), "walks by itself");
    }
}
