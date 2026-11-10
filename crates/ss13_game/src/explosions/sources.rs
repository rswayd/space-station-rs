//! What blows up (server):
//!
//! - Fuel tanks (`/obj/structure/reagent_dispensers/fueltank`, `boom()`): an explosion sized by
//!   the welding fuel in them, from an explosion, a fire, a brute or burn projectile or a lit
//!   welder; an unlit welder refills from them instead (`welder_act`).
//! - Gas tanks (`/obj/item/tank`, `handle_tolerances`, `atom_break`, `atom_destruction`): over
//!   `TANK_LEAK_PRESSURE` a tank takes damage every `SSobj` tick; at half integrity it springs
//!   a leak (a quarter of its gas out each tick), at none it fragments above
//!   `TANK_FRAGMENT_PRESSURE` (`dyn_explosion` of `volume * (pressure - fragment) / scale`,
//!   flash 1.5, capped) and lets the rest of its gas out.
//! - C-4 and X-4 (`/obj/item/grenade/c4`): use in hand sets the timer (10 s or more), a click
//!   on an adjacent atom plants it after 3 s; when the timer runs out the target takes a heavy
//!   blast (mobs do not) and the charge explodes with its `boom_sizes`; X-4 aims its blast
//!   (120 degrees, away from the planter) when stuck to something dense. A charge destroyed by a
//!   blast before it is planted goes off where it lies.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::{DmAtom, DmItem, DmObj, IconIndex};
use ss13_net::chat::server::SendChat;
use ss13_shared::appearance::Overlay;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_shared::{Appearance, Dir, TilePos};
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, ask_world};
use ss13_world::Density;

use super::{Explosion, Severity};
use crate::construction::Job;
use crate::internals::TankAir;
use crate::inventory::CarriedBy;
use crate::obj_damage::{clients_in_range, is_under};
use crate::tools::{Actor, DoAfter, Welder};

pub const FUELTANK: &str = "/obj/structure/reagent_dispensers/fueltank";
const FUEL: &str = "/datum/reagent/fuel";
const C4: &str = "/obj/item/grenade/c4";
const X4: &str = "/obj/item/grenade/c4/x4";
/// `TANK_LEAK_PRESSURE`, `TANK_RUPTURE_PRESSURE`, `TANK_FRAGMENT_PRESSURE`,
/// `TANK_FRAGMENT_SCALE` (kPa), `TANK_MELT_TEMPERATURE` (K).
pub const TANK_LEAK_PRESSURE: f64 = 30.0 * ONE_ATMOSPHERE;
pub const TANK_RUPTURE_PRESSURE: f64 = 35.0 * ONE_ATMOSPHERE;
pub const TANK_FRAGMENT_PRESSURE: f64 = 40.0 * ONE_ATMOSPHERE;
pub const TANK_FRAGMENT_SCALE: f64 = 84.0 * ONE_ATMOSPHERE;
const TANK_MELT_TEMPERATURE: f64 = 1_000_000.0;
const ONE_ATMOSPHERE: f64 = 101.325;
/// `/obj/item/tank` `integrity_failure`.
const TANK_INTEGRITY_FAILURE: f32 = 0.5;
const SPRAY: &str = "sound/effects/spray.ogg";

fn say_around(world: &mut World, pos: TilePos, span: Span, text: String) {
    let clients = clients_in_range(world, pos, crate::health::DEFAULT_MESSAGE_RANGE, &[]);
    if clients.is_empty() {
        return;
    }
    world.write_message(SendChat {
        clients,
        message: ChatMessage {
            channel: ChatChannel::Combat,
            segments: vec![ChatSegment::new(span, text)],
        },
    });
}

fn capitalized_name(world: &World, e: Entity) -> String {
    crate::cooking::capitalize(&crate::chem::name_of(world, e))
}

// --- Fuel tanks ---------------------------------------------------------------------------

pub fn is_fueltank(world: &World, e: Entity) -> bool {
    world
        .get::<DmObj>(e)
        .is_some_and(|o| is_under(&o.path, FUELTANK))
}

/// `/obj/structure/reagent_dispensers/fueltank/ex_act`: it blows up. False for anything else.
pub fn fueltank_ex_act(world: &mut World, e: Entity) -> bool {
    if !is_fueltank(world, e) {
        return false;
    }
    boom(world, e);
    true
}

/// tg `boom()`'s explosion for `fuel` units of welding fuel (`None`: under 25, no blast):
/// `(devastation, heavy, light, flame)`.
pub fn fuel_blast(fuel: f32) -> Option<(i32, i32, i32, i32)> {
    Some(match fuel {
        f if f < 25.0 => return None,
        f if f <= 150.0 => (0, 0, 1, 2),
        f if f <= 300.0 => (0, 0, 2, 3),
        f if f <= 750.0 => (0, 1, 3, 5),
        f if f <= 1500.0 => (0, 1, 4, 6),
        _ => (1, 2, 6, 8),
    })
}

/// `/obj/structure/reagent_dispensers/proc/boom()`: the fuel explodes, the tank is gone.
// shortcut: no `chem_splash` of the other reagents (they are lost with the tank).
pub fn boom(world: &mut World, tank: Entity) {
    let Some(&pos) = world.get::<TilePos>(tank) else {
        return;
    };
    let fuel = match (
        world.get_resource::<crate::chem::Chem>(),
        world.get::<crate::chem::Reagents>(tank),
    ) {
        (Some(chem), Some(r)) => chem.0.id(FUEL).map_or(0.0, |id| r.0.amount(id)),
        _ => 0.0,
    };
    let name = capitalized_name(world, tank);
    world.despawn(tank);
    crate::construction::tile_changed(world, pos);
    match fuel_blast(fuel) {
        Some((dev, heavy, light, flame)) => {
            say_around(world, pos, Span::Danger, format!("{name} взрывается!"));
            info!("{name} at {pos:?} exploded with {fuel} units of fuel");
            super::explosion(
                world,
                pos,
                Explosion::new(dev, heavy, light).flame(flame).cause(&name),
            );
        }
        None => say_around(world, pos, Span::Danger, format!("{name} разрывается!")),
    }
}

/// `fueltank/welder_act`: an unlit welder refills from the tank, a lit one blows it up.
pub fn fueltank_welder_act(world: &mut World, actor: Actor, target: Entity) -> bool {
    let Some(item) = actor.item else {
        return false;
    };
    if !is_fueltank(world, target) {
        return false;
    }
    let Some(welder) = world.get::<Welder>(item).cloned() else {
        return false;
    };
    let fuel_id = world
        .get_resource::<crate::chem::Chem>()
        .and_then(|c| c.0.id(FUEL));
    let in_tank = match (fuel_id, world.get::<crate::chem::Reagents>(target)) {
        (Some(id), Some(r)) => r.0.amount(id),
        _ => 0.0,
    };
    let tell = |world: &mut World, span, text: String| {
        crate::tools::tell(world, actor.client, span, text);
    };
    let welder_name = crate::chem::name_of(world, item);
    if !welder.on {
        if welder.fuel >= welder.max_fuel {
            tell(
                world,
                Span::Warning,
                format!("Ваш {welder_name} уже полон!"),
            );
            return true;
        }
        let amount = (welder.max_fuel - welder.fuel).min(in_tank);
        if let (Some(id), Some(mut r)) = (fuel_id, world.get_mut::<crate::chem::Reagents>(target)) {
            r.0.remove_reagent(id, amount);
        }
        if let Some(mut w) = world.get_mut::<Welder>(item) {
            w.fuel += amount;
        }
        crate::tools::refresh_welder(world, item);
        tell(
            world,
            Span::Notice,
            format!("Вы заправляете {welder_name}."),
        );
        if let Some(&pos) = world.get::<TilePos>(target) {
            crate::sound::playsound(
                world,
                crate::sound::PlaySound::new(pos, "sound/effects/refill.ogg", 50.0, true),
            );
        }
        return true;
    }
    if in_tank <= 0.0 {
        let name = crate::chem::name_of(world, target);
        tell(
            world,
            Span::Warning,
            format!("{name}: топливо закончилось!"),
        );
        return true;
    }
    if let Some(&pos) = world.get::<TilePos>(target) {
        let user = capitalized_name(world, actor.mob);
        say_around(
            world,
            pos,
            Span::Danger,
            format!("{user} с треском проваливает попытку заправить {welder_name}!"),
        );
    }
    info!("{:?} detonated a fuel tank with a lit welder", actor.mob);
    boom(world, target);
    true
}

// --- Gas tanks ----------------------------------------------------------------------------

/// The `integrity_failure` of an item that breaks before it is destroyed (gas tanks).
pub fn item_integrity_failure(world: &World, item: Entity) -> Option<f32> {
    world
        .get::<TankAir>(item)
        .is_some()
        .then_some(TANK_INTEGRITY_FAILURE)
}

/// A gas tank that sprang a leak (tg `leaking`).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Leaking;

/// What a gas tank's air went through, for the doppler array (tg `reaction_info` filled by
/// `/datum/component/atmos_reaction_recorder` and `explosion_info`).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TankResults {
    /// The reactions that ran in it (`ss13_atmos::reactions::reacted_ids` mask). tg's recorder
    /// never resets (its reset signals are never sent), so neither does this.
    pub reactions: u32,
    /// `TANK_MERGE_OVERPRESSURE`: gas merged into it left it over `TANK_FRAGMENT_PRESSURE`.
    pub overpressure: bool,
}

/// Adds the reactions of `mask` to the tank's record.
pub fn record_reactions(world: &mut World, tank: Entity, mask: u32) {
    if mask == 0 {
        return;
    }
    if let Some(mut r) = world.get_mut::<TankResults>(tank) {
        r.reactions |= mask;
        return;
    }
    if let Ok(mut e) = world.get_entity_mut(tank) {
        e.insert(TankResults {
            reactions: mask,
            overpressure: false,
        });
    }
}

/// `merging_information` (`COMSIG_GASMIX_MERGED`): called after gas was merged into the tank.
pub fn note_merge(world: &mut World, tank: Entity) {
    let over = world
        .get::<TankAir>(tank)
        .is_some_and(|t| t.pressure() > TANK_FRAGMENT_PRESSURE);
    if !over {
        return;
    }
    let mut results = world.get::<TankResults>(tank).copied().unwrap_or_default();
    results.overpressure = true;
    if let Ok(mut e) = world.get_entity_mut(tank) {
        e.insert(results);
    }
}

/// Where an item's gas goes: its tile, or its carrier's.
fn item_turf(world: &World, item: Entity) -> Option<TilePos> {
    let mut e = item;
    for _ in 0..8 {
        if let Some(&pos) = world.get::<TilePos>(e) {
            return Some(pos);
        }
        e = world
            .get::<CarriedBy>(e)
            .map(|c| c.0)
            .or_else(|| world.get::<crate::storage::StoredIn>(e).map(|s| s.0))?;
    }
    None
}

/// `/obj/item/tank/atom_break`: it springs a leak.
pub fn item_break(world: &mut World, item: Entity) {
    if world.get::<TankAir>(item).is_none() || world.get::<Leaking>(item).is_some() {
        return;
    }
    world.entity_mut(item).insert(Leaking);
    if let Some(pos) = item_turf(world, item) {
        let name = capitalized_name(world, item);
        say_around(world, pos, Span::Warning, format!("{name} даёт течь!"));
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, SPRAY, 10.0, true).extrarange(-3.0),
        );
    }
}

/// `/obj/item/tank/atom_destruction` and `atom_deconstruct`: fragmentation over
/// `TANK_FRAGMENT_PRESSURE` (one more reaction first), then the gas out onto the tile.
pub fn item_destruction(world: &mut World, item: Entity) {
    let Some(mut tank) = world.get::<TankAir>(item).cloned() else {
        return;
    };
    let pos = item_turf(world, item);
    // A tank compressor holding the tank absorbs its explosion and takes its gas
    // (`explosion_handle`, `assume_air`, `tank_destruction`).
    let compressor = crate::science::ordnance_machines::compressor_of(world, item);
    if tank.pressure() > TANK_FRAGMENT_PRESSURE {
        let volume = tank.volume;
        let mut reacted = 0;
        tank.air.react_recording(
            ss13_atmos::reactions::Holder::Other,
            volume,
            &mut ss13_atmos::reactions::Exposures::default(),
            &mut reacted,
        );
        record_reactions(world, item, reacted);
        let power =
            (tank.volume * (tank.pressure() - TANK_FRAGMENT_PRESSURE)) / TANK_FRAGMENT_SCALE;
        // `COMSIG_ATOM_INTERNAL_EXPLOSION`: a refinery cancels it and keeps no gas
        // (`assume_air` returns null).
        if crate::science::anomaly_refinery::internal_explosion(world, item, power as f32) {
            if let Some(mut t) = world.get_mut::<TankAir>(item) {
                t.air = ss13_atmos::GasMixture::default();
            }
            return;
        }
        if let Some(c) = compressor {
            crate::science::ordnance_machines::absorb_explosion(world, c);
        } else if let Some(pos) = pos {
            info!("a gas tank at {pos:?} exploded with a power of {power}");
            // `dyn_explosion(src, power, flash_range = 1.5)` with the tank as the cause.
            let (r, range) = super::wave::dyn_ranges(power as f32);
            let ex = Explosion {
                devastation: r.devastation,
                heavy: r.heavy,
                light: r.light,
                flame: Some(0.0),
                flash: Some(1.5 * range),
                source: Some(item),
                ..Explosion::new(0, 0, 0).cause("gas tank")
            };
            if power != 0.0 && power.is_finite() {
                super::explosion(world, pos, ex);
            }
        }
    }
    if let Some(c) = compressor {
        let pressure = tank.pressure();
        crate::science::ordnance_machines::assume_air(world, c, &tank.air);
        crate::science::ordnance_machines::tank_destroyed(world, c, item, pressure);
    } else if let Some(pos) = pos {
        if let Some(mut atmos) = world.get_resource_mut::<ss13_atmos::Atmos>() {
            atmos.merge_air(pos, &tank.air);
        }
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, SPRAY, 10.0, true).extrarange(-3.0),
        );
    }
    if let Some(mut t) = world.get_mut::<TankAir>(item) {
        t.air = ss13_atmos::GasMixture::default();
    }
}

/// tg `/obj/item/tank/process` -> `handle_tolerances` and the leak, every `SSobj` tick of
/// `secs`.
// Cost: one pass over the gas tanks (hundreds) every second, a pressure each.
pub fn tank_tolerances(world: &mut World, secs: f32) {
    let tanks: Vec<(Entity, f64, f64)> = world
        .query::<(Entity, &TankAir)>()
        .iter(world)
        .map(|(e, t)| (e, t.pressure(), t.air.temperature))
        .collect();
    for (tank, pressure, temperature) in tanks {
        let max = world
            .get::<DmItem>(tank)
            .map(|d| d.max_integrity)
            .filter(|m| *m > 0.0)
            .unwrap_or(200.0);
        if temperature >= TANK_MELT_TEMPERATURE {
            // `take_damage(..., BURN, FIRE)`.
            // shortcut: the `fire` armor of tanks is not modelled (80): full damage.
            let ratio = (temperature - TANK_MELT_TEMPERATURE) / temperature;
            super::ex_act::damage_item(world, tank, max * ratio as f32 * secs);
            if world.get_entity(tank).is_err() {
                continue;
            }
        }
        if pressure >= TANK_LEAK_PRESSURE {
            let ratio =
                (pressure - TANK_LEAK_PRESSURE) / (TANK_RUPTURE_PRESSURE - TANK_LEAK_PRESSURE);
            let armor = world
                .get::<ss13_mapload::DmArmor>(tank)
                .map_or(0.0, |a| a.bomb);
            let damage = super::ex_act::through_armor(max * ratio as f32 * secs, armor);
            super::ex_act::damage_item(world, tank, damage);
        }
        if world.get::<Leaking>(tank).is_none() || world.get_entity(tank).is_err() {
            continue;
        }
        // `location.assume_air(leaked_gas)`: a tank compressor keeps it in its buffer.
        if let Some(c) = crate::science::ordnance_machines::compressor_of(world, tank) {
            let leaked = world
                .get_mut::<TankAir>(tank)
                .map(|mut t| t.air.remove_ratio(0.25));
            if let Some(leaked) = leaked {
                crate::science::ordnance_machines::assume_air(world, c, &leaked);
            }
            continue;
        }
        let Some(pos) = item_turf(world, tank) else {
            continue;
        };
        let leaked = world
            .get_mut::<TankAir>(tank)
            .map(|mut t| t.air.remove_ratio(0.25));
        if let (Some(leaked), Some(mut atmos)) =
            (leaked, world.get_resource_mut::<ss13_atmos::Atmos>())
        {
            atmos.merge_air(pos, &leaked);
        }
    }
}

// --- C-4 ----------------------------------------------------------------------------------

/// A C-4 or X-4 charge (`/obj/item/grenade/c4`).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Charge {
    /// `det_time`, seconds.
    pub det_time: f64,
    /// `boom_sizes`: devastation, heavy, light.
    pub boom_sizes: [i32; 3],
    pub directional: bool,
    /// `aim_dir`.
    pub aim_dir: Dir,
    /// Planted: on what, and the game time it goes off.
    pub planted: Option<(Entity, f64)>,
}

/// `minimum_timer`, `maximum_timer`.
const C4_MIN_TIMER: f64 = 10.0;
const C4_MAX_TIMER: f64 = 60000.0;
/// `directional_arc`.
const X4_ARC: f32 = 120.0;
/// `plant_c4`'s `do_after`, deciseconds.
const PLANT_DS: f32 = 30.0;

/// Gives a charge its state the first time it is used.
fn charge(world: &mut World, item: Entity) -> Option<Charge> {
    if let Some(c) = world.get::<Charge>(item) {
        return Some(*c);
    }
    let path = world.get::<DmItem>(item)?.path.clone();
    if !is_under(&path, C4) {
        return None;
    }
    let x4 = is_under(&path, X4);
    // The spider charge's `boom_sizes = list(4, 8, 12)`.
    let spider = is_under(&path, crate::antag::raiders::ninja::SPIDER_CHARGE);
    let c = Charge {
        det_time: C4_MIN_TIMER,
        boom_sizes: if x4 {
            [0, 5, 7]
        } else if spider {
            [4, 8, 12]
        } else {
            [0, 0, 3]
        },
        directional: x4,
        aim_dir: Dir::North,
        planted: None,
    };
    world.entity_mut(item).insert(c);
    Some(c)
}

/// The timer prompt of a charge.
#[derive(Component, Clone, Copy, Debug)]
pub struct ChargePrompt {
    pub charge: Entity,
}

/// `c4/attack_self`: "Please set the timer".
pub fn c4_attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    let Some(c) = charge(world, item) else {
        return false;
    };
    if c.planted.is_some() {
        return true;
    }
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            mob,
            "C4 Timer",
            "Please set the timer",
            PromptKind::Number {
                default: C4_MIN_TIMER,
                min: C4_MIN_TIMER,
                max: C4_MAX_TIMER,
                round: true,
            },
        ),
    );
    world
        .entity_mut(prompt)
        .insert(ChargePrompt { charge: item });
    true
}

/// The timer answer, checked now: the charge still in the user's hands.
pub(super) fn c4_prompt_answers(
    world: &mut World,
    mut cursor: Local<MessageCursor<PromptAnswered>>,
) {
    let answers: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, actor, answer) in answers {
        let Some(ChargePrompt { charge }) = world.get::<ChargePrompt>(prompt).copied() else {
            continue;
        };
        let Some(PromptAnswer::Number(n)) = answer else {
            continue;
        };
        let held = world.get::<CarriedBy>(charge).is_some_and(|c| c.0 == actor);
        if !held || n <= 0.0 {
            continue;
        }
        let Some(mut c) = world.get_mut::<Charge>(charge) else {
            continue;
        };
        if c.planted.is_some() {
            continue;
        }
        c.det_time = n.clamp(C4_MIN_TIMER, C4_MAX_TIMER);
        let secs = c.det_time;
        crate::tools::tell(
            world,
            client,
            Span::Notice,
            format!("Timer set for {secs} seconds."),
        );
    }
}

/// `c4/interact_with_atom` -> `plant_c4`: 3 s on the target, then the charge sticks to it.
pub fn c4_interact(world: &mut World, actor: Actor, target: Entity) -> bool {
    let Some(item) = actor.item else {
        return false;
    };
    let Some(c) = charge(world, item) else {
        return false;
    };
    // "Here lies C4 ghosts. We hardly knew ye"
    if world.get::<crate::ghost::Ghost>(target).is_some() || target == item {
        return false;
    }
    // `c4/ninja/plant_c4`: only a ninja, only in the objective's area.
    if crate::antag::raiders::ninja::is_spider_charge(world, item)
        && !crate::antag::raiders::ninja::spider_plant_check(world, actor, item, target)
    {
        return true;
    }
    let (Some(&from), Some(&to)) = (
        world.get::<TilePos>(actor.mob),
        world.get::<TilePos>(target),
    ) else {
        return false;
    };
    let aim = crate::pull::dir_between(from, to).unwrap_or(Dir::North);
    if let Some(mut c) = world.get_mut::<Charge>(item) {
        c.aim_dir = aim;
    }
    let secs = c.det_time;
    crate::tools::tell(
        world,
        actor.client,
        Span::Notice,
        format!("You start planting the charge. The timer is set to {secs}..."),
    );
    crate::tools::do_after(
        world,
        actor,
        target,
        PLANT_DS,
        0,
        Job::Explosive(ExplosiveJob::Plant),
    );
    true
}

/// A timed step of the explosives (`Job::Explosive`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExplosiveJob {
    Plant,
}

/// The end of [`ExplosiveJob::Plant`]: the charge leaves the hand, sticks to the target and
/// starts counting.
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: ExplosiveJob) {
    let ExplosiveJob::Plant = job;
    let Some(item) = action.item else {
        return;
    };
    let held = world.get::<CarriedBy>(item).is_some_and(|c| c.0 == mob);
    if !held || world.get_entity(action.target).is_err() {
        return;
    }
    let Some(c) = world.get::<Charge>(item).copied() else {
        return;
    };
    // `temporarilyRemoveItemFromInventory`, `moveToNullspace`.
    world.entity_mut(item).remove::<CarriedBy>();
    let now = world.resource::<Time>().elapsed_secs_f64();
    let at = now + c.det_time;
    if let Some(mut c) = world.get_mut::<Charge>(item) {
        c.planted = Some((action.target, at));
    }
    set_overlay(world, item, action.target, true);
    crate::antag::raiders::ninja::spider_planted(world, item, mob);
    let target_name = crate::chem::name_of(world, action.target);
    info!(
        "{mob:?} planted a charge on {target_name} at {:?} with a {} second fuse",
        action.target_pos, c.det_time
    );
    crate::tools::tell(
        world,
        action.client,
        Span::Notice,
        format!(
            "You plant the bomb. Timer counting down from {}.",
            c.det_time
        ),
    );
}

/// `plastic_overlay` (`"[inhand_icon_state]2"` of the charge's icon) on the target.
fn set_overlay(world: &mut World, item: Entity, target: Entity, on: bool) {
    let Some(atom) = world.get::<DmAtom>(item) else {
        return;
    };
    let state = format!("{}2", inhand_state(world, item, atom));
    let Some(id) = world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(&atom.icon, &state))
    else {
        return;
    };
    let Some(mut look) = world.get_mut::<Appearance>(target) else {
        return;
    };
    if on {
        look.overlays.push(Overlay::new(id));
    } else if let Some(i) = look.overlays.iter().position(|o| o.icon_state == id) {
        look.overlays.remove(i);
    }
}

fn inhand_state<'a>(world: &'a World, item: Entity, atom: &'a DmAtom) -> &'a str {
    world
        .get::<DmItem>(item)
        .map(|d| d.inhand_icon_state.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(atom.icon_state.as_str())
}

/// Planted charges whose timer ran out: `c4/detonate`.
// Cost: per tick, one pass over the planted charges (a handful).
pub(super) fn detonate_charges(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let due: Vec<(Entity, Charge)> = world
        .query::<(Entity, &Charge)>()
        .iter(world)
        .filter(|(_, c)| c.planted.is_some_and(|(_, at)| at <= now))
        .map(|(e, c)| (e, *c))
        .collect();
    for (item, c) in due {
        detonate(world, item, c);
    }
}

/// `/obj/item/grenade/atom_deconstruct(FALSE)` of a charge (destroyed by a blast): it goes off
/// where it is. False for anything that is not a charge.
pub fn c4_destroyed(world: &mut World, item: Entity) -> bool {
    let Some(c) = charge(world, item) else {
        return false;
    };
    detonate(world, item, c);
    true
}

fn detonate(world: &mut World, item: Entity, c: Charge) {
    let Some((target, _)) = c.planted else {
        let [dev, heavy, light] = c.boom_sizes;
        if let Some(pos) = item_turf(world, item) {
            super::explosion(world, pos, Explosion::new(dev, heavy, light).cause("C-4"));
        }
        if world.get_entity(item).is_ok() {
            world.despawn(item);
        }
        return;
    };
    // `c4/ninja/detonate`: out of its area it fizzles into a fresh charge.
    let spider = world
        .get::<crate::antag::raiders::ninja::SpiderCharge>(item)
        .copied();
    if spider.is_some()
        && world.get_entity(target).is_ok()
        && !crate::antag::raiders::ninja::spider_detonation(world, item, target)
    {
        set_overlay(world, item, target, false);
        world.despawn(item);
        return;
    }
    let mut location = None;
    let mut dense = false;
    if world.get_entity(target).is_ok() {
        // `get_turf(target)`: a carried target goes off where it is carried.
        location = item_turf(world, target);
        dense = world.get::<Density>(target).is_some();
        set_overlay(world, item, target, false);
        if world.get::<crate::health::Health>(target).is_none() {
            // `EX_ACT(target, EXPLODE_HEAVY, target)`.
            if world.get::<ss13_mapload::Turf>(target).is_some() {
                if let Some(pos) = location {
                    super::ex_act::ex_act_turf(world, pos, Severity::Heavy);
                }
            } else {
                for inside in super::ex_act::contents_of(world, target) {
                    super::ex_act::ex_act_movable(world, inside, Severity::Heavy);
                }
                super::ex_act::ex_act_movable(world, target, Severity::Heavy);
            }
        }
    }
    let [dev, heavy, light] = c.boom_sizes;
    if let Some(pos) = location {
        let mut ex = Explosion::new(dev, heavy, light).cause("C-4");
        if c.directional && dense {
            ex = ex.directional(dir_angle(c.aim_dir), X4_ARC);
        }
        super::explosion(world, pos, ex);
        if let Some(spider) = spider {
            crate::antag::raiders::ninja::spider_exploded(world, spider);
        }
    }
    if world.get_entity(item).is_ok() {
        world.despawn(item);
    }
}

/// tg `dir2angle`: degrees clockwise from north.
pub fn dir_angle(dir: Dir) -> f32 {
    match dir {
        Dir::North => 0.0,
        Dir::NorthEast => 45.0,
        Dir::East => 90.0,
        Dir::SouthEast => 135.0,
        Dir::South => 180.0,
        Dir::SouthWest => 225.0,
        Dir::West => 270.0,
        Dir::NorthWest => 315.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuel_tank_blasts_by_fuel() {
        assert_eq!(fuel_blast(20.0), None);
        assert_eq!(fuel_blast(100.0), Some((0, 0, 1, 2)));
        assert_eq!(fuel_blast(1000.0), Some((0, 1, 4, 6)));
        assert_eq!(fuel_blast(5000.0), Some((1, 2, 6, 8)));
    }

    #[test]
    fn tank_fragment_power() {
        // A 70 L tank at 60 atm: 70 * 20 atm / 84 atm = 16.67 power: (0, 2, 5).
        let pressure = 60.0 * ONE_ATMOSPHERE;
        let power = (70.0 * (pressure - TANK_FRAGMENT_PRESSURE)) / TANK_FRAGMENT_SCALE;
        assert!((power - 16.666).abs() < 0.01);
        let (r, _) = super::super::wave::dyn_ranges(power as f32);
        assert_eq!((r.devastation, r.heavy, r.light), (1.0, 2.0, 5.0));
    }
}
