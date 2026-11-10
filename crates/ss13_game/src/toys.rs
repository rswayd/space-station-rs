//! Toys (server), after tg `code/game/objects/items/toys.dm` and `latexballoon.dm`: toy weapons
//! and balloons. Their harmlessness (no force, no throw force) is their DM vars.
//!
//! - The cap gun ([`CapGun`], 7 caps) fires at anything out of reach: a bang and a message, or
//!   `*click*` when empty; a box of caps ([`CapAmmo`], 7) reloads it up to 7.
//! - The toy sword ([`ToySword`]) flicks out and in (`/datum/component/transforming`:
//!   `e_sword_on_<colour>`, `saberon`/`saberoff`); a screwdriver turns its colour dial (red,
//!   blue, green, purple), a multitool makes it rainbow; two toy swords make the double-bladed
//!   toy (`/obj/item/dualsaber/toy`, wielded by `crate::weapons::two_handed`).
//! - The Mini-Meteor lands with a meteor's thud and shakes the screens within 10 tiles; emagged
//!   it explodes (light 1).
//! - Snap pops burst (sparks, ash, a snap) when thrown and not caught, in fire, or stepped on
//!   by a running human or a silicon.
//! - Water balloons ([`WaterBalloon`], 10 u) fill from a tank or a cup; thrown and not caught,
//!   or hit by something sharp, they burst over a tile (`crate::effects::splash::splash_tile`).
//! - Balloons take a random colour (`balloon_<colour>`, `_storage` off the floor); snowballs
//!   are thrown at far clicks and fall apart when they hit; latex gloves blow up from a gas
//!   tank (3 L), a second time or a sharp or hot thing bursts them, their air into the room.
// shortcut: the katana's alternative stab and the foam blade, dagger and beach ball are only
// their vars; no balloon animals (`TRAIT_BALLOON_SUTRA`), no monkeys popping balloons with
// foam darts, no syndicate balloon mood, no coloured balloon names; the double toy sword does
// not keep the rainbow; the phoenix snap pop leaves plain ash; latex balloons do not burst in
// hot air, by bullets or explosions; a toy sword's `w_class` does not change.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmAtom, DmItem, DmObj, DmTool};
use ss13_net::camera::server::SendCameraEffect;
use ss13_reagents::{Holder, Owner};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_shared::protocol::CameraEffect;

use crate::chem::{Reagents, map_pos, name_of, tell_mob, visible};
use crate::click::{RangedClick, ResolveClicks};
use crate::combat::CombatRng;
use crate::inventory::{CarriedBy, Inventory};
use crate::sound::{PlaySound, playsound};

pub const CAP_GUN: &str = "/obj/item/toy/gun";
pub const CAP_AMMO: &str = "/obj/item/toy/ammo/gun";
pub const TOY_SWORD: &str = "/obj/item/toy/sword";
pub const TOY_DUALSABER: &str = "/obj/item/dualsaber/toy";
const MINIMETEOR: &str = "/obj/item/toy/minimeteor";
const SNAPPOP: &str = "/obj/item/toy/snappop";
const WATER_BALLOON: &str = "/obj/item/toy/waterballoon";
const BALLOON: &str = "/obj/item/toy/balloon";
const SNOWBALL: &str = "/obj/item/toy/snowball";
const LATEX_BALLOON: &str = "/obj/item/latexballoon";
const DISPENSER_ROOT: &str = "/obj/structure/reagent_dispensers";
const CUP_ROOT: &str = "/obj/item/reagent_containers/cup";
const TANK_ROOT: &str = "/obj/item/tank";
const ASH: &str = "/obj/effect/decal/cleanable/ash";
/// The cap gun's and the cap box's 7.
const CAPS: u32 = 7;
/// `BALLOON_COLORS`.
const BALLOON_COLORS: [&str; 6] = ["red", "blue", "green", "yellow", "orange", "purple"];
/// Balloons with their own sprite (`random_color = FALSE`).
const PLAIN_BALLOONS: [&str; 4] = [
    "/obj/item/toy/balloon/corgi",
    "/obj/item/toy/balloon/heart",
    "/obj/item/toy/balloon/syndicate",
    "/obj/item/toy/balloon/arrest",
];
/// The toy sword's colour dial (`screwdriver_act`).
const SWORD_COLORS: [&str; 4] = ["red", "blue", "green", "purple"];
/// The water balloon's holder.
const WATER_BALLOON_VOLUME: f32 = 10.0;
/// `T0C + 100`: what lights a latex balloon (`FIRE_MINIMUM_TEMPERATURE_TO_EXIST`).
const BURST_HEAT: f32 = crate::medical::surgery::FIRE_MINIMUM_TEMPERATURE_TO_EXIST;
/// `remove_air_volume(3)`.
const LATEX_BREATH: f64 = 3.0;
/// The Mini-Meteor's `urange(10)` and `shake_camera(M, 3, 1)`.
const METEOR_SHAKE_RANGE: u32 = 10;

/// `/obj/item/toy/gun::bullets`.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct CapGun {
    pub bullets: u32,
}

/// `/obj/item/toy/ammo/gun::amount_left`.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct CapAmmo {
    pub left: u32,
}

/// `/obj/item/toy/sword`: `TRAIT_TRANSFORM_ACTIVE`, `saber_color`, `hacked`.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct ToySword {
    pub active: bool,
    pub color: String,
    pub hacked: bool,
}

/// `obj_flags & EMAGGED` of a Mini-Meteor.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HeavyMeteor;

/// A water balloon (its reagents are its [`Reagents`]).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WaterBalloon;

/// A balloon's `current_color` and whether it was last drawn on the floor.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct Balloon {
    pub color: Option<String>,
    on_floor: Option<bool>,
}

/// `/obj/item/latexballoon`: `state`, `air_contents`.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct LatexBalloon {
    pub state: LatexState,
    air: Option<ss13_atmos::GasMixture>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LatexState {
    Deflated,
    Inflated,
    Popped,
}

pub struct ToysPlugin;

impl Plugin for ToysPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        app.add_click_handler(ClickPhase::AttackSelf, 3319, |w, c| {
            toggle_sword(w, c.mob, c.target)
        });
        app.add_click_handler(ClickPhase::OtherHand, 479, |w, c| {
            c.held.is_some_and(|h| item_on_item(w, c.mob, h, c.target))
        });
        app.add_click_handler(ClickPhase::HeldOnTarget, 3649, |w, c| {
            c.held
                .is_some_and(|h| held_on_target(w, c.mob, h, c.target))
        });
        app.add_observer(add_state).add_systems(
            Update,
            (
                ranged_clicks.after(ResolveClicks),
                snappop_steps.after(ss13_world::WorldSystems::Movement),
                balloon_looks,
            ),
        );
    }
}

fn path_of(world: &World, e: Entity) -> String {
    world
        .get::<DmItem>(e)
        .map(|i| i.path.clone())
        .or_else(|| world.get::<DmObj>(e).map(|o| o.path.clone()))
        .unwrap_or_default()
}

fn sound_at(world: &mut World, e: Entity, file: &str, vol: f32, vary: bool) {
    if let Some(pos) = map_pos(world, e) {
        playsound(world, PlaySound::new(pos, file, vol, vary));
    }
}

fn say_near(world: &mut World, pos: TilePos, span: Span, text: &str) {
    let clients = crate::chem::clients_near(world, pos, None);
    if !clients.is_empty() {
        world.write_message(ss13_net::chat::server::SendChat {
            clients,
            message: crate::chem::line(span, text),
        });
    }
}

fn add_state(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    let Ok(item) = items.get(add.entity) else {
        return;
    };
    let e = add.entity;
    let path = item.path.clone();
    if is_under(&path, CAP_GUN) {
        commands
            .entity(e)
            .try_insert_if_new(CapGun { bullets: CAPS });
    } else if is_under(&path, CAP_AMMO) {
        commands.entity(e).try_insert_if_new(CapAmmo { left: CAPS });
    } else if is_under(&path, TOY_SWORD) {
        commands.entity(e).try_insert_if_new(ToySword {
            active: false,
            color: "blue".to_owned(),
            hacked: false,
        });
    } else if is_under(&path, WATER_BALLOON) {
        let holder = Holder::new(
            WATER_BALLOON_VOLUME,
            0,
            Owner {
                path,
                living: false,
            },
        );
        commands
            .entity(e)
            .try_insert_if_new((WaterBalloon, Reagents(holder)));
    } else if is_under(&path, BALLOON) {
        commands.queue(move |world: &mut World| init_balloon(world, e));
    } else if is_under(&path, LATEX_BALLOON) {
        commands.entity(e).try_insert_if_new(LatexBalloon {
            state: LatexState::Deflated,
            air: None,
        });
    }
}

// --- Cap gun ----------------------------------------------------------------------------------

/// `/obj/item/toy/gun/item_interaction`: caps into the gun.
fn reload(world: &mut World, mob: Entity, ammo: Entity, gun: Entity) -> bool {
    let (Some(gun_state), Some(box_state)) = (
        world.get::<CapGun>(gun).copied(),
        world.get::<CapAmmo>(ammo).copied(),
    ) else {
        return false;
    };
    if gun_state.bullets >= CAPS {
        tell_mob(world, mob, Span::Warning, "Он уже полностью заряжен!");
        return true;
    }
    if box_state.left == 0 {
        tell_mob(world, mob, Span::Warning, "Пистонов больше нет!");
        return true;
    }
    let (bullets, left, loaded) = reloaded(gun_state.bullets, box_state.left);
    if let Some(mut g) = world.get_mut::<CapGun>(gun) {
        g.bullets = bullets;
    }
    if let Some(mut a) = world.get_mut::<CapAmmo>(ammo) {
        a.left = left;
    }
    crate::obj_damage::set_icon_state(world, ammo, &format!("357OLD-{left}"));
    tell_mob(
        world,
        mob,
        Span::Notice,
        &format!("Вы заряжаете пистоны: {loaded} шт."),
    );
    true
}

/// The gun's and the box's caps after a reload, and how many went in.
pub fn reloaded(bullets: u32, left: u32) -> (u32, u32, u32) {
    let loaded = (CAPS - bullets.min(CAPS)).min(left);
    (bullets + loaded, left - loaded, loaded)
}

/// `ranged_interact_with_atom`: a cap fired at what was clicked.
fn fire_cap_gun(world: &mut World, mob: Entity, gun: Entity, target: Entity) {
    let Some(state) = world.get::<CapGun>(gun).copied() else {
        return;
    };
    if state.bullets == 0 {
        tell_mob(world, mob, Span::Warning, "*щёлк*");
        sound_at(
            world,
            gun,
            "sound/items/weapons/gun/revolver/dry_fire.ogg",
            30.0,
            true,
        );
        return;
    }
    sound_at(
        world,
        mob,
        "sound/items/weapons/gun/revolver/shot.ogg",
        100.0,
        true,
    );
    if let Some(mut g) = world.get_mut::<CapGun>(gun) {
        g.bullets -= 1;
    }
    let (user, name, what) = (
        name_of(world, mob),
        name_of(world, gun),
        name_of(world, target),
    );
    visible(
        world,
        mob,
        &format!("Вы стреляете из {name} в {what}!"),
        &format!("{user} стреляет из {name} в {what}!"),
        Span::Danger,
    );
}

// --- Toy swords -------------------------------------------------------------------------------

fn refresh_sword(world: &mut World, sword: Entity) {
    let Some(state) = world.get::<ToySword>(sword).cloned() else {
        return;
    };
    let part = if state.active {
        format!("_on_{}", state.color)
    } else {
        String::new()
    };
    crate::obj_damage::set_icon_state(world, sword, &format!("e_sword{part}"));
    if let Some(mut dm) = world.get_mut::<DmItem>(sword) {
        dm.inhand_icon_state = format!("e_sword{part}");
    }
    if let Some(holder) = world.get::<CarriedBy>(sword).map(|c| c.0)
        && let Some(mut inventory) = world.get_mut::<Inventory>(holder)
    {
        inventory.set_changed();
    }
}

/// `transforming/on_attack_self` -> `on_transform`.
fn toggle_sword(world: &mut World, mob: Entity, sword: Entity) -> bool {
    let Some(active) = world.get_mut::<ToySword>(sword).map(|mut s| {
        s.active = !s.active;
        s.active
    }) else {
        return false;
    };
    let text = if active {
        "клинок выдвинут"
    } else {
        "клинок убран"
    };
    crate::balloon::balloon_alert(world, mob, sword, text);
    let sound = if active {
        "sound/items/weapons/saberon.ogg"
    } else {
        "sound/items/weapons/saberoff.ogg"
    };
    sound_at(world, sword, sound, 20.0, true);
    refresh_sword(world, sword);
    true
}

/// The next colour of the dial; `None` for the rainbow one.
pub fn next_sword_color(color: &str) -> Option<&'static str> {
    let i = SWORD_COLORS.iter().position(|&c| c == color)?;
    Some(SWORD_COLORS[(i + 1) % SWORD_COLORS.len()])
}

/// `screwdriver_act`, `multitool_act` and two swords joined (`item_interaction`).
fn sword_tool(world: &mut World, mob: Entity, held: Entity, sword: Entity) -> bool {
    let Some(state) = world.get::<ToySword>(sword).cloned() else {
        return false;
    };
    if world.get::<ToySword>(held).is_some() {
        if crate::inventory::no_drop(world, held) || crate::inventory::no_drop(world, sword) {
            tell_mob(
                world,
                mob,
                Span::Warning,
                "Меч прилип к руке, его не присоединить!",
            );
            return true;
        }
        let Some(pos) = map_pos(world, mob) else {
            return true;
        };
        tell_mob(
            world,
            mob,
            Span::Notice,
            "Вы соединяете концы двух пластиковых мечей и получаете двуклинковую игрушку! \
             Вы фальшиво-круты.",
        );
        world.despawn(held);
        world.despawn(sword);
        if let Some(saber) = crate::construction::spawn_obj(world, TOY_DUALSABER, pos)
            && let Some(inventory) = world.get::<Inventory>(mob).cloned()
        {
            crate::storage::put_in_hands(world, mob, &inventory, saber);
        }
        return true;
    }
    let behaviour = world
        .get::<DmTool>(held)
        .map(|t| t.behaviour.clone())
        .unwrap_or_default();
    match behaviour.as_str() {
        "screwdriver" => {
            if let Some(next) = next_sword_color(&state.color) {
                if let Some(mut s) = world.get_mut::<ToySword>(sword) {
                    s.color = next.to_owned();
                }
                crate::balloon::balloon_alert(world, mob, sword, &format!("цвет: {next}"));
                refresh_sword(world, sword);
            }
            true
        }
        "multitool" => {
            if state.hacked {
                tell_mob(world, mob, Span::Warning, "Он и так шикарен!");
            } else {
                if let Some(mut s) = world.get_mut::<ToySword>(sword) {
                    s.hacked = true;
                    s.color = "rainbow".to_owned();
                }
                tell_mob(world, mob, Span::Warning, "RNBW_ENGAGE");
                refresh_sword(world, sword);
            }
            true
        }
        _ => false,
    }
}

// --- Mini-Meteor, snap pops, snowballs --------------------------------------------------------

/// `/obj/item/toy/minimeteor/throw_impact`.
fn meteor_impact(world: &mut World, meteor: Entity) {
    let Some(pos) = map_pos(world, meteor) else {
        return;
    };
    playsound(
        world,
        PlaySound::new(pos, "sound/effects/meteorimpact.ogg", 40.0, true),
    );
    if world.contains_resource::<Messages<SendCameraEffect>>() {
        for client in crate::obj_damage::clients_in_range(world, pos, METEOR_SHAKE_RANGE, &[]) {
            world.write_message(SendCameraEffect {
                client,
                effect: CameraEffect::Shake {
                    duration_ds: 3.0,
                    strength: 1.0,
                },
            });
        }
    }
    if world.get::<HeavyMeteor>(meteor).is_some() {
        crate::explosions::explosion(world, pos, crate::explosions::Explosion::new(0, 0, 1));
    }
}

/// `pop_burst(n, c)`: sparks, ash, a snap, gone.
fn pop_burst(world: &mut World, pop: Entity, sparks: u32, cardinal: bool) {
    let Some(pos) = map_pos(world, pop) else {
        world.despawn(pop);
        return;
    };
    crate::effects::particles::do_sparks(world, sparks, cardinal, pos);
    crate::construction::spawn_obj(world, ASH, pos);
    let name = name_of(world, pop);
    say_near(world, pos, Span::Warning, &format!("{name} взрывается!"));
    playsound(
        world,
        PlaySound::new(pos, "sound/effects/snap.ogg", 50.0, true),
    );
    world.despawn(pop);
}

/// `fire_act` of a snap pop. True when `thing` is one.
pub fn fire_act(world: &mut World, thing: Entity) -> bool {
    if is_under(&path_of(world, thing), SNAPPOP) {
        pop_burst(world, thing, 3, true);
        return true;
    }
    false
}

/// `on_entered` of snap pops: a running human or a silicon steps on them.
fn snappop_steps(world: &mut World, mut cursor: Local<MessageCursor<ss13_world::Moved>>) {
    let moved: Vec<ss13_world::Moved> = cursor
        .read(world.resource::<Messages<ss13_world::Moved>>())
        .copied()
        .collect();
    for m in moved {
        let Ok(mover) = world.get_entity(m.entity) else {
            continue;
        };
        let silicon = mover.contains::<crate::silicon::Silicon>();
        let human = mover.contains::<Inventory>() && mover.contains::<crate::health::Health>();
        let running = mover.get::<crate::janitor::Walking>().is_none_or(|w| !w.0);
        if !(silicon || (human && running)) {
            continue;
        }
        let pops: Vec<Entity> = world
            .resource::<ss13_world::SpatialIndex>()
            .on_tile(m.to)
            .iter()
            .copied()
            .filter(|&e| is_under(&path_of(world, e), SNAPPOP))
            .collect();
        for pop in pops {
            tell_mob(world, m.entity, Span::Danger, "Вы наступаете на хлопушку!");
            pop_burst(world, pop, 2, false);
        }
    }
}

/// Far clicks: the cap gun fires, a snowball is thrown (`ranged_interact_with_atom`).
fn ranged_clicks(world: &mut World, mut cursor: Local<MessageCursor<RangedClick>>) {
    let clicks: Vec<RangedClick> = cursor
        .read(world.resource::<Messages<RangedClick>>())
        .copied()
        .collect();
    for click in clicks {
        let Some(held) = world
            .get::<Inventory>(click.mob)
            .and_then(Inventory::active_item)
        else {
            continue;
        };
        if world.get::<CapGun>(held).is_some() {
            fire_cap_gun(world, click.mob, held, click.target);
        } else if is_under(&path_of(world, held), SNOWBALL) {
            // `user.throw_item(interacting_with)`: through the throw click.
            world
                .entity_mut(click.mob)
                .insert(crate::throw::ThrowMode(true));
            world.write_message(crate::click::ThrowClick {
                client: click.client,
                mob: click.mob,
                target: click.target,
                target_pos: click.target_pos,
            });
        }
    }
}

/// `throw_impact` of the toys, plates and pizza boxes, `hit` what it hit (`None`: it landed).
/// The thrown thing was not caught. True when it is gone.
pub fn thrown(world: &mut World, item: Entity, hit: Option<Entity>) -> bool {
    let path = path_of(world, item);
    if world.get::<WaterBalloon>(item).is_some() {
        let at = hit.and_then(|h| world.get::<TilePos>(h).copied());
        return balloon_burst(world, item, at);
    }
    if is_under(&path, SNAPPOP) {
        pop_burst(world, item, 3, true);
        return true;
    }
    if is_under(&path, SNOWBALL) {
        sound_at(world, item, "sound/effects/pop.ogg", 20.0, true);
        world.despawn(item);
        return true;
    }
    if is_under(&path, MINIMETEOR) {
        meteor_impact(world, item);
        return world.get_entity(item).is_err();
    }
    if crate::kitchen_ware::plate_thrown(world, item) {
        return true;
    }
    crate::kitchen_ware::pizzabox_thrown(world, item);
    false
}

// --- Water balloons ---------------------------------------------------------------------------

/// `balloon_burst(AT)`: a filled balloon bursts over the turf of what it hit (or its own).
fn balloon_burst(world: &mut World, balloon: Entity, at: Option<TilePos>) -> bool {
    let Some(holder) = world.get::<Reagents>(balloon).map(|r| r.0.clone()) else {
        return false;
    };
    if holder.total_volume < 1.0 {
        return false;
    }
    let Some(pos) = at.or_else(|| map_pos(world, balloon)) else {
        return false;
    };
    let name = name_of(world, balloon);
    say_near(world, pos, Span::Danger, &format!("{name} лопается!"));
    crate::effects::splash::splash_tile(world, &holder, pos);
    world.despawn(balloon);
    true
}

fn refresh_water_balloon(world: &mut World, balloon: Entity) {
    let full = world
        .get::<Reagents>(balloon)
        .is_some_and(|r| r.0.total_volume >= 1.0);
    let (state, inhand) = if full {
        ("waterballoon", "balloon")
    } else {
        ("balloon_red-e", "balloon-empty")
    };
    crate::obj_damage::set_icon_state(world, balloon, state);
    if let Some(mut dm) = world.get_mut::<DmItem>(balloon) {
        dm.inhand_icon_state = inhand.to_owned();
    }
    if let Some(holder) = world.get::<CarriedBy>(balloon).map(|c| c.0)
        && let Some(mut inventory) = world.get_mut::<Inventory>(holder)
    {
        inventory.set_changed();
    }
}

/// A water balloon filled from a tank (`interact_with_atom`) or a cup (`item_interaction`).
fn fill_balloon(world: &mut World, mob: Entity, balloon: Entity, source: Entity) -> bool {
    let (balloon_name, source_name) = (name_of(world, balloon), name_of(world, source));
    let source_volume = world
        .get::<Reagents>(source)
        .map_or(0.0, |r| r.0.total_volume);
    if source_volume <= 0.0 {
        tell_mob(world, mob, Span::Warning, &format!("{source_name} пуст."));
        return true;
    }
    if world
        .get::<Reagents>(balloon)
        .is_some_and(|r| r.0.total_volume >= WATER_BALLOON_VOLUME)
    {
        tell_mob(world, mob, Span::Warning, &format!("{balloon_name} полон."));
        return true;
    }
    crate::chem::transfer(world, source, balloon, WATER_BALLOON_VOLUME);
    tell_mob(
        world,
        mob,
        Span::Notice,
        &format!("Вы наполняете {balloon_name} содержимым {source_name}."),
    );
    refresh_water_balloon(world, balloon);
    true
}

// --- Balloons ---------------------------------------------------------------------------------

fn init_balloon(world: &mut World, e: Entity) {
    if world.get_entity(e).is_err() || world.get::<Balloon>(e).is_some() {
        return;
    }
    let path = path_of(world, e);
    let color = (!PLAIN_BALLOONS.iter().any(|root| is_under(&path, root))).then(|| {
        let i = world
            .resource_mut::<CombatRng>()
            .range(0, BALLOON_COLORS.len() as u32 - 1) as usize;
        BALLOON_COLORS[i].to_owned()
    });
    world.entity_mut(e).insert(Balloon {
        color,
        on_floor: None,
    });
}

/// `update_icon_state` of balloons after `Moved`: `[icon_state][_colour][_storage]`.
// Cost: one pass over the balloons (a few per map) per tick.
fn balloon_looks(world: &mut World) {
    let stale: Vec<(Entity, bool)> = world
        .query::<(Entity, &Balloon, Has<TilePos>)>()
        .iter(world)
        .filter(|(_, b, floor)| b.on_floor != Some(*floor))
        .map(|(e, _, floor)| (e, floor))
        .collect();
    for (e, floor) in stale {
        let Some(b) = world.get::<Balloon>(e).cloned() else {
            continue;
        };
        // `initial(icon_state)`.
        let initial = world
            .get::<DmAtom>(e)
            .map(|a| a.icon_state.clone())
            .unwrap_or_default();
        let base = match &b.color {
            Some(color) => format!("{initial}_{color}"),
            None => initial,
        };
        let state = if floor {
            base.clone()
        } else {
            format!("{base}_storage")
        };
        crate::obj_damage::set_icon_state(world, e, &state);
        if let Some(mut dm) = world.get_mut::<DmItem>(e) {
            dm.inhand_icon_state = base;
        }
        if let Some(mut b) = world.get_mut::<Balloon>(e) {
            b.on_floor = Some(floor);
        }
    }
}

// --- Latex balloons ---------------------------------------------------------------------------

fn set_latex(world: &mut World, glove: Entity, state: LatexState) {
    if let Some(mut l) = world.get_mut::<LatexBalloon>(glove) {
        l.state = state;
    }
    match state {
        LatexState::Inflated => {
            crate::obj_damage::set_icon_state(world, glove, "latexballoon_blow");
            if let Some(mut dm) = world.get_mut::<DmItem>(glove) {
                dm.inhand_icon_state = "latexballoon".to_owned();
                dm.lefthand_file = "icons/mob/inhands/items_lefthand.dmi".to_owned();
                dm.righthand_file = "icons/mob/inhands/items_righthand.dmi".to_owned();
            }
        }
        LatexState::Popped => {
            crate::obj_damage::set_icon_state(world, glove, "latexballoon_bursted");
            if let Some(mut dm) = world.get_mut::<DmItem>(glove) {
                dm.inhand_icon_state = "greyscale_gloves".to_owned();
                dm.lefthand_file = "icons/mob/inhands/clothing/gloves_righthand.dmi".to_owned();
                dm.righthand_file = "icons/mob/inhands/clothing/gloves_lefthand.dmi".to_owned();
            }
        }
        LatexState::Deflated => {}
    }
    if let Some(holder) = world.get::<CarriedBy>(glove).map(|c| c.0)
        && let Some(mut inventory) = world.get_mut::<Inventory>(holder)
    {
        inventory.set_changed();
    }
}

/// `blow(tank)`: 3 L from the tank; a blown one bursts instead.
fn blow(world: &mut World, mob: Entity, glove: Entity, tank: Entity) -> bool {
    let Some(state) = world.get::<LatexBalloon>(glove).map(|l| l.state) else {
        return false;
    };
    if state == LatexState::Popped {
        return true;
    }
    let air = world
        .get_mut::<crate::internals::TankAir>(tank)
        .and_then(|mut t| t.remove_air_volume(LATEX_BREATH));
    let Some(air) = air else {
        crate::balloon::balloon_alert(world, mob, glove, "баллон пуст!");
        return true;
    };
    if let Some(mut l) = world.get_mut::<LatexBalloon>(glove) {
        l.air = Some(air);
    }
    if state == LatexState::Inflated {
        latex_burst(world, glove);
        return true;
    }
    sound_at(
        world,
        glove,
        "sound/items/modsuit/inflate_bloon.ogg",
        50.0,
        true,
    );
    crate::balloon::balloon_alert(world, mob, glove, "вы надуваете шарик!");
    set_latex(world, glove, LatexState::Inflated);
    true
}

/// `burst`: a blown balloon pops and lets its air out.
fn latex_burst(world: &mut World, glove: Entity) {
    let Some(l) = world.get::<LatexBalloon>(glove).cloned() else {
        return;
    };
    if l.state != LatexState::Inflated {
        return;
    }
    let Some(air) = l.air else {
        return;
    };
    set_latex(world, glove, LatexState::Popped);
    if let Some(mut l) = world.get_mut::<LatexBalloon>(glove) {
        l.air = None;
    }
    sound_at(world, glove, "sound/items/balloon_pop.ogg", 75.0, true);
    if let Some(pos) = map_pos(world, glove)
        && let Some(mut atmos) = world.get_resource_mut::<ss13_atmos::Atmos>()
    {
        atmos.merge_air(pos, &air);
    }
}

// --- Clicks -----------------------------------------------------------------------------------

/// `held` used on the item `target` (in the other hand or in the world).
fn item_on_item(world: &mut World, mob: Entity, held: Entity, target: Entity) -> bool {
    let target_path = path_of(world, target);
    if world.get::<CapGun>(target).is_some() && world.get::<CapAmmo>(held).is_some() {
        return reload(world, mob, held, target);
    }
    if world.get::<ToySword>(target).is_some() && sword_tool(world, mob, held, target) {
        return true;
    }
    if is_under(&target_path, MINIMETEOR) && crate::emag::is_emag(world, held) {
        if world.get::<HeavyMeteor>(target).is_none() {
            tell_mob(
                world,
                mob,
                Span::Warning,
                "Вы закорачиваете электронику внутри. «Метеор» вдруг кажется гораздо тяжелее...?",
            );
            world.entity_mut(target).insert(HeavyMeteor);
        }
        return true;
    }
    if world.get::<WaterBalloon>(target).is_some() {
        if is_under(&path_of(world, held), CUP_ROOT) && world.get::<Reagents>(held).is_some() {
            return fill_balloon(world, mob, target, held);
        }
        let tool = crate::medical::surgery::ToolInfo::read(world, Some(held));
        if tool.sharpness != 0 {
            let at = map_pos(world, target);
            balloon_burst(world, target, at);
            return true;
        }
    }
    if world.get::<WaterBalloon>(held).is_some() && is_under(&target_path, DISPENSER_ROOT) {
        return fill_balloon(world, mob, held, target);
    }
    if world.get::<LatexBalloon>(target).is_some() {
        if is_under(&path_of(world, held), TANK_ROOT)
            && world.get::<crate::internals::TankAir>(held).is_some()
        {
            return blow(world, mob, target, held);
        }
        let tool = crate::medical::surgery::ToolInfo::read(world, Some(held));
        if tool.sharpness != 0 || tool.temperature >= BURST_HEAT {
            latex_burst(world, target);
            return true;
        }
    }
    false
}

/// `held` on a world target: [`item_on_item`]; a water balloon never hits a mob (`attack`).
fn held_on_target(world: &mut World, mob: Entity, held: Entity, target: Entity) -> bool {
    if world.get::<crate::health::Health>(target).is_some() {
        return world.get::<WaterBalloon>(held).is_some();
    }
    item_on_item(world, mob, held, target)
}

pub fn examine_lines(world: &World, item: Entity, lines: &mut Vec<(Span, String)>) {
    if let Some(gun) = world.get::<CapGun>(item) {
        lines.push((Span::Info, format!("Осталось пистонов: {}.", gun.bullets)));
    }
    if let Some(ammo) = world.get::<CapAmmo>(item) {
        lines.push((Span::Info, format!("Осталось пистонов: {}.", ammo.left)));
    }
    if let Some(sword) = world.get::<ToySword>(item) {
        lines.push((
            Span::Notice,
            format!(
                "Регулятор подсветки установлен на {}; его можно повернуть <b>отвёрткой</b>.",
                sword.color
            ),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cap_reload_fills_up_to_seven() {
        assert_eq!(reloaded(0, 7), (7, 0, 7));
        assert_eq!(reloaded(5, 7), (7, 5, 2));
        assert_eq!(reloaded(2, 3), (5, 0, 3));
        assert_eq!(reloaded(7, 4), (7, 4, 0));
    }

    #[test]
    fn sword_dial_cycles_and_rainbow_stays() {
        assert_eq!(next_sword_color("red"), Some("blue"));
        assert_eq!(next_sword_color("purple"), Some("red"));
        assert_eq!(next_sword_color("rainbow"), None);
    }
}
