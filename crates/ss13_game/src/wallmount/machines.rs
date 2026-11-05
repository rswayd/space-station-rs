//! The machines buttons drive by `id`: mass drivers (tg `mass_driver.dm` `drive`), floor
//! igniters and wall sparkers (`igniter.dm` `toggle`, `process`, `ignite`).
//!
//! - A mass driver, powered, throws what is on its tile (unanchored things, living mobs,
//!   mechs; at most 19, the 20th makes it screech) towards the map edge it faces,
//!   `drive_range * power` tiles at `power` speed, for `power_per_obj` joules each and once.
//! - An igniter toggles on a button or a hand; while on, every `SSmachines` tick (2 s) it uses
//!   its active power and exposes its tile to 1000 K (`hotspot_expose(1000, 500, TRUE)`), so
//!   gas there burns; no power or no open turf turns it off.
//! - A sparker, powered and 5 s after its last spark, uses its active power, sparks and
//!   exposes its tile to 1000 K (`hotspot_expose(1000, 2500, TRUE)`).
// shortcut: no wires,
// panel, deconstruction, direction wrench, multitool id, sparker screwdriver disable; a
// thrown mob flies without impact damage (`crate::throw` hits with items only); the mass
// driver's parts are the tier 1 servo of a mapped one (`RefreshParts`: `drive_range + 10`).

use bevy::prelude::*;
use ss13_atmos::Atmos;
use ss13_mapload::{DmAtom, DmItem, DmObj, DmPower, DmWallMount};
use ss13_shared::chat::Span;
use ss13_shared::{Dir, TilePos};
use ss13_world::SpatialIndex;

use super::{Look, refresh_look};
use crate::power::{MachineTicks, Unpowered, machine_use_energy};

/// `/obj/machinery/igniter/process`, `/obj/machinery/sparker/ignite`: `hotspot_expose` heat and
/// volumes.
const IGNITE_HEAT: f64 = 1000.0;
const IGNITER_VOLUME: f64 = 500.0;
const SPARKER_VOLUME: f64 = 2500.0;
/// `last_spark + 50` deciseconds.
const SPARK_COOLDOWN: f64 = 5.0;
/// `O_limit >= 20`.
const DRIVE_LIMIT: u32 = 20;
/// `RefreshParts` of a mapped driver: one tier 1 servo.
const SERVO_RANGE: f32 = 10.0;

/// `/obj/machinery/mass_driver`.
#[derive(Component, Clone, Debug)]
pub struct MassDriver {
    pub id: Option<String>,
    pub power: f32,
    pub drive_range: f32,
    pub power_per_obj: f32,
}

/// `/obj/machinery/igniter`.
#[derive(Component, Clone, Debug)]
pub struct Igniter {
    pub id: Option<String>,
    pub on: bool,
}

/// An igniter that is on (`begin_processing`).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct IgniterLit;

/// `/obj/machinery/sparker`.
#[derive(Component, Clone, Debug)]
pub struct Sparker {
    pub id: Option<String>,
    pub last_spark: f64,
}

fn now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

pub(super) fn add_mass_driver(world: &mut World, e: Entity, dm: &DmWallMount) {
    let id = super::controls::id_of(dm);
    super::controls::link(world, |l| &mut l.mass_drivers, &id, e);
    world.entity_mut(e).insert(MassDriver {
        id,
        power: dm.power,
        drive_range: dm.drive_range + SERVO_RANGE,
        power_per_obj: dm.power_per_obj,
    });
}

pub(super) fn add_igniter(world: &mut World, e: Entity, dm: &DmWallMount) {
    let id = super::controls::id_of(dm);
    super::controls::link(world, |l| &mut l.igniters, &id, e);
    let mut entity = world.entity_mut(e);
    entity.insert(Igniter { id, on: dm.on });
    if dm.on {
        entity.insert(IgniterLit);
    }
}

pub(super) fn add_sparker(world: &mut World, e: Entity, dm: &DmWallMount) {
    let id = super::controls::id_of(dm);
    super::controls::link(world, |l| &mut l.sparkers, &id, e);
    world.entity_mut(e).insert(Sparker {
        id,
        last_spark: f64::NEG_INFINITY,
    });
}

fn is_open_turf(world: &World, pos: TilePos) -> bool {
    crate::construction::turf_at(world, pos)
        .and_then(|t| world.get::<ss13_mapload::DmTurfBuild>(t))
        .is_some_and(|b| ss13_mapload::fidelity::is_under(&b.path, "/turf/open"))
}

/// tg `use_energy(active_power_usage, force = FALSE)` succeeded.
// shortcut: takes what the area gives (`force = TRUE`) and fails only on nothing at all.
fn use_active_power(world: &mut World, machine: Entity) -> bool {
    let joules = world
        .get::<DmPower>(machine)
        .map_or(0.0, |p| p.active_power_usage);
    joules <= 0.0 || machine_use_energy(world, machine, joules, false) > 0.0
}

/// `flick("[base_icon_state]1")` of a mass driver, `flick("[base_icon_state]-spark")` of a
/// sparker.
fn flick(world: &mut World, e: Entity, suffix: &str) {
    let Some(base) = world
        .get::<DmWallMount>(e)
        .map(|d| d.base_icon_state.clone())
    else {
        return;
    };
    crate::animation::flick(world, e, &format!("{base}{suffix}"));
}

/// tg `mass_driver/drive()`.
pub fn drive(world: &mut World, driver: Entity) {
    let (Some(state), Some(&pos)) = (
        world.get::<MassDriver>(driver).cloned(),
        world.get::<TilePos>(driver),
    ) else {
        return;
    };
    if world.get::<Unpowered>(driver).is_some() {
        return;
    }
    machine_use_energy(world, driver, f64::from(state.power_per_obj), false);
    let dir = world
        .get::<DmAtom>(driver)
        .and_then(|a| Dir::from_bits(a.dir))
        .unwrap_or(Dir::South);
    let range = (state.drive_range * state.power).max(0.0) as u32;
    // `get_edge_target_turf`: far enough that the range ends the flight first.
    let (dx, dy) = dir.offset();
    let reach = range as i32 + 1;
    let target = TilePos::new(pos.x + dx * reach, pos.y + dy * reach, pos.z);
    let on_tile: Vec<Entity> = world.resource::<SpatialIndex>().on_tile(pos).to_vec();
    let now_ds = now(world) * 10.0;
    let mut count = 0;
    for thing in on_tile {
        if thing == driver || !drivable(world, thing) {
            continue;
        }
        count += 1;
        if count >= DRIVE_LIMIT {
            let name = name_of(world, driver);
            let chat = crate::pull::visible_message(
                world,
                pos,
                ss13_shared::chat::ChatChannel::Local,
                (
                    Span::Notice,
                    format!(
                        "{name} lets out a screech, it doesn't seem to be able to handle the load."
                    ),
                ),
                &[],
            );
            for message in chat {
                world.write_message(message);
            }
            break;
        }
        machine_use_energy(world, driver, f64::from(state.power_per_obj), false);
        if let Some(path) = crate::throw::Trajectory::new(pos, target, range, state.power, now_ds) {
            world.entity_mut(thing).insert(crate::throw::Thrown {
                path,
                thrower: None,
                target: None,
                last_dir: None,
            });
        }
    }
    flick(world, driver, "1");
}

/// `!O.anchored || ismecha(O)`, without non-living mobs: items, living mobs, mechs, unanchored
/// structures and machines.
fn drivable(world: &World, e: Entity) -> bool {
    if world.get::<crate::ghost::Ghost>(e).is_some() {
        return false;
    }
    world.get::<DmItem>(e).is_some()
        || world.get::<crate::health::Health>(e).is_some()
        || world.get::<crate::mobs::BasicMob>(e).is_some()
        || world.get::<crate::mecha::Mech>(e).is_some()
        || (world.get::<DmObj>(e).is_some()
            && !world
                .get::<crate::obj_damage::Anchored>(e)
                .is_some_and(|a| a.0))
}

fn name_of(world: &World, e: Entity) -> String {
    world
        .get::<ss13_mapload::DmName>(e)
        .map_or_else(String::new, |n| {
            crate::vending::plain_name(&n.name).to_owned()
        })
}

/// tg `igniter/toggle()`.
pub fn toggle_igniter(world: &mut World, igniter: Entity) {
    let Some(mut state) = world.get_mut::<Igniter>(igniter) else {
        return;
    };
    state.on = !state.on;
    let on = state.on;
    if on {
        world.entity_mut(igniter).insert(IgniterLit);
    } else {
        world.entity_mut(igniter).remove::<IgniterLit>();
    }
    refresh_look(world, igniter);
}

/// tg `sparker/ignite()`. True when it sparked.
pub fn ignite(world: &mut World, sparker: Entity) -> bool {
    let (Some(state), Some(&pos)) = (
        world.get::<Sparker>(sparker).cloned(),
        world.get::<TilePos>(sparker),
    ) else {
        return false;
    };
    let t = now(world);
    if world.get::<Unpowered>(sparker).is_some()
        || t < state.last_spark + SPARK_COOLDOWN
        || !is_open_turf(world, pos)
        || !use_active_power(world, sparker)
    {
        return false;
    }
    flick(world, sparker, "-spark");
    // `spark_system.start()`: two cardinal sparks (with their sound).
    crate::effects::particles::do_sparks(world, 2, true, pos);
    if let Some(mut s) = world.get_mut::<Sparker>(sparker) {
        s.last_spark = t;
    }
    if let Some(mut atmos) = world.get_resource_mut::<Atmos>() {
        atmos.hotspot_expose(pos, IGNITE_HEAT, SPARKER_VOLUME, true);
    }
    true
}

/// tg `igniter/attack_hand`: a powered igniter toggles. True when `target` is an igniter.
pub(super) fn interact(world: &mut World, target: Entity) -> bool {
    if world.get::<Igniter>(target).is_none() {
        return false;
    }
    if world.get::<Unpowered>(target).is_none() {
        toggle_igniter(world, target);
    }
    true
}

/// tg `igniter/process()` of the lit igniters, once per `SSmachines` tick.
pub(super) fn process_igniters(world: &mut World, mut last: Local<Option<u64>>) {
    let ticks = world.resource::<MachineTicks>().0;
    let from = last.unwrap_or(ticks);
    *last = Some(ticks);
    for _ in from..ticks {
        let mut query = world.query_filtered::<(Entity, &TilePos), With<IgniterLit>>();
        let lit: Vec<(Entity, TilePos)> = query.iter(world).map(|(e, &p)| (e, p)).collect();
        for (e, pos) in lit {
            let works = is_open_turf(world, pos)
                && world.get::<Unpowered>(e).is_none()
                && use_active_power(world, e);
            if !works {
                if let Some(mut i) = world.get_mut::<Igniter>(e) {
                    i.on = false;
                }
                world.entity_mut(e).remove::<IgniterLit>();
                refresh_look(world, e);
                continue;
            }
            if let Some(mut atmos) = world.get_resource_mut::<Atmos>() {
                atmos.hotspot_expose(pos, IGNITE_HEAT, IGNITER_VOLUME, true);
            }
        }
    }
}

/// tg `update_icon_state` of the three.
pub(super) fn look(world: &World, e: Entity, powered: bool) -> Option<Look> {
    let base = world.get::<DmWallMount>(e)?.base_icon_state.as_str();
    let icon_state = if let Some(igniter) = world.get::<Igniter>(e) {
        format!("{base}{}", u8::from(igniter.on))
    } else if world.get::<Sparker>(e).is_some() {
        if powered {
            base.to_owned()
        } else {
            format!("{base}-p")
        }
    } else if world.get::<MassDriver>(e).is_some() {
        base.to_owned()
    } else {
        return None;
    };
    Some(Look {
        icon_state,
        overlays: Vec::new(),
        light: None,
    })
}
