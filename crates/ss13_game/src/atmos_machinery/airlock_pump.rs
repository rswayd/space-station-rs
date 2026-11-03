//! External airlock pumps (server): tg `/obj/machinery/atmospherics/components/unary/
//! airlock_pump` (`unary_devices/airlock_pump.dm`).
//!
//! At round start a pump links the airlocks in front of it (toward its pipes: internal) and
//! behind it (external), each side the first airlock or airtight tile within 2 tiles plus the
//! airlocks lined up beside it, closes and bolts the external ones and opens and bolts the
//! internal ones. Opening one of its bolted airlocks asks the pump to cycle: all airlocks close
//! and bolt, then for up to 10 s it fills its tile and the tiles around from the distro port
//! (toward the internal side's target) or siphons them into the waste port (toward the external
//! side's), and finally unbolts and opens the side that was asked for.
// shortcut: no shuttle docking (`on_dock_request`, `safe_dock`, `undock`), no `close_others`
// group fallback, no pipe underlay drawing, no unwrenching or rebuilding.

use bevy::prelude::*;
use ss13_atmos::Atmos;
use ss13_atmos::gas::{CELL_VOLUME, ONE_ATMOSPHERE, R_IDEAL_GAS_EQUATION, quantize};
use ss13_atmos::pipenet::Pipenet;
use ss13_mapload::DmAtmosDevice;
use ss13_shared::{Dir, TilePos};
use ss13_world::SpatialIndex;

use super::AtmosDevice;
use crate::doors::{Door, with_door};
use crate::obj_damage::is_under;
use crate::power::Unpowered;
use ss13_mapload::DoorKind;

pub const AIRLOCK_PUMP: &str = "/obj/machinery/atmospherics/components/unary/airlock_pump";
const UNBOLT_ONLY: &str = "/obj/machinery/atmospherics/components/unary/airlock_pump/unbolt_only";
const SILENT: &str = "/obj/machinery/atmospherics/components/unary/airlock_pump/silent";
const LAVALAND: &str = "/obj/machinery/atmospherics/components/unary/airlock_pump/lavaland";
/// `airlock_pump_distance_limit`, `airlock_group_distance_limit`.
const PUMP_DISTANCE: u32 = 2;
const GROUP_DISTANCE: u32 = 2;
/// `allowed_pressure_error`, `min_distro_pressure`, `volume_rate`, `cycle_timeout`.
const PRESSURE_ERROR: f64 = ONE_ATMOSPHERE / 100.0;
const MIN_DISTRO_PRESSURE: f64 = ONE_ATMOSPHERE / 10.0;
const VOLUME_RATE: f64 = 2000.0;
const CYCLE_TIMEOUT: f64 = 10.0;
/// The door animations it waits for (`stoplag(1 SECONDS)`).
const ANIMATION: f64 = 1.0;

/// What the pump does once the airlocks finish moving.
#[derive(Clone, Debug, PartialEq)]
enum Pending {
    /// `start_cycle` after closing everything.
    Run {
        releasing: bool,
        source: Option<Entity>,
    },
    /// `stop_cycle`'s message after the opening.
    Say { airlock: Entity, text: &'static str },
}

#[derive(Component, Clone, Debug, PartialEq)]
pub struct AirlockPump {
    pub on: bool,
    /// `pump_direction`: releasing fills from distro, else siphons to waste.
    pub releasing: bool,
    pub internal_target: f64,
    pub external_target: f64,
    pub cycle_target: f64,
    pub internal: Vec<Entity>,
    pub external: Vec<Entity>,
    pub set_up: bool,
    pub open_on_cycle: bool,
    pub audible: bool,
    pub timeout_at: Option<f64>,
    animating_until: f64,
    pending: Option<(f64, Pending)>,
    adjacent: Vec<TilePos>,
    check_turfs_at: f64,
    was_powered: bool,
}

/// `post_machine_initialize` -> `set_links` of every mapped pump.
pub fn init_airlock_pumps(world: &mut World) {
    let mut q = world.query::<(Entity, &DmAtmosDevice, &AtmosDevice, &TilePos)>();
    let found: Vec<(Entity, String, u8, TilePos)> = q
        .iter(world)
        .filter(|(_, dm, ..)| is_under(&dm.path, AIRLOCK_PUMP))
        .map(|(e, dm, d, &p)| (e, dm.path.clone(), d.dir, p))
        .collect();
    for (e, path, dir, pos) in found {
        let mut pump = AirlockPump {
            on: false,
            releasing: false,
            internal_target: ONE_ATMOSPHERE,
            external_target: if is_under(&path, LAVALAND) { 50.0 } else { 0.0 },
            cycle_target: 0.0,
            internal: Vec::new(),
            external: Vec::new(),
            set_up: false,
            open_on_cycle: !is_under(&path, UNBOLT_ONLY),
            audible: !is_under(&path, SILENT),
            timeout_at: None,
            animating_until: 0.0,
            pending: None,
            adjacent: Vec::new(),
            check_turfs_at: 0.0,
            was_powered: true,
        };
        set_links(world, e, &mut pump, pos, dir);
        world.entity_mut(e).insert(pump);
    }
}

fn airlock_at(world: &World, pos: TilePos) -> Option<Entity> {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .find(|&e| {
            world
                .get::<Door>(e)
                .is_some_and(|d| d.kind == DoorKind::Airlock)
        })
}

/// `is_valid_airlock`: not linked to another pump yet (mapped pumps take any airlock type).
fn valid(world: &World, airlock: Option<Entity>) -> Option<Entity> {
    airlock.filter(|&a| world.get::<Door>(a).is_some_and(|d| d.cycle_pump.is_none()))
}

/// `find_density`: the first airlock or airtight tile within reach.
fn find_density(world: &World, origin: TilePos, dir: Dir) -> Option<TilePos> {
    let atmos = world.get_resource::<Atmos>();
    let mut pos = origin;
    for _ in 0..PUMP_DISTANCE.max(1) {
        pos = pos.step(dir);
        if valid(world, airlock_at(world, pos)).is_some() {
            return Some(pos);
        }
        // `!next_turf.can_atmos_pass`: a wall (no air of its own).
        if atmos.is_some_and(|a| a.air(pos).is_none()) {
            return Some(pos);
        }
    }
    None
}

/// `get_adjacent_airlocks`: the origin's airlock and those lined up beside it.
fn adjacent_airlocks(world: &World, origin: Option<TilePos>, sides: [Dir; 2]) -> Vec<Entity> {
    let Some(origin) = origin else {
        return Vec::new();
    };
    let mut out: Vec<Entity> = valid(world, airlock_at(world, origin))
        .into_iter()
        .collect();
    for dir in sides {
        let mut pos = origin;
        for _ in 0..GROUP_DISTANCE {
            pos = pos.step(dir);
            match valid(world, airlock_at(world, pos)) {
                Some(a) if !out.contains(&a) => out.push(a),
                Some(_) => {}
                None => break,
            }
        }
    }
    out
}

fn set_links(world: &mut World, e: Entity, pump: &mut AirlockPump, pos: TilePos, dir: u8) {
    let Some(front) = Dir::from_bits(dir) else {
        return;
    };
    let sides = if matches!(front, Dir::North | Dir::South) {
        [Dir::East, Dir::West]
    } else {
        [Dir::North, Dir::South]
    };
    let internal = adjacent_airlocks(world, find_density(world, pos, front), sides);
    let external = adjacent_airlocks(world, find_density(world, pos, front.reverse()), sides);
    if internal.is_empty() || external.is_empty() {
        warn!("airlock pump at {pos:?} found no airlocks to cycle with");
        return;
    }
    for &a in internal.iter().chain(&external) {
        if let Some(mut d) = world.get_mut::<Door>(a) {
            d.cycle_pump = Some(e);
        }
    }
    for &a in &external {
        secure(world, a, false);
    }
    if pump.open_on_cycle {
        for &a in &internal {
            secure(world, a, true);
        }
    }
    pump.internal = internal;
    pump.external = external;
    pump.set_up = true;
}

/// `secure_open` / `secure_close`: unbolt, move (forced), bolt again.
fn secure(world: &mut World, airlock: Entity, open: bool) {
    with_door(world, airlock, |door, appearance, ctx| {
        door.locked = false;
        let moved = if open {
            ctx.open(door, appearance, true)
        } else {
            ctx.close(door, appearance, true)
        };
        door.locked = true;
        door.apply_look(appearance);
        ((), moved)
    });
}

fn unbolt(world: &mut World, airlock: Entity) {
    crate::wires::set_bolts(world, airlock, false);
}

fn deny(world: &mut World, airlock: Entity) {
    with_door(world, airlock, |door, appearance, ctx| {
        ((), ctx.deny(door, appearance))
    });
}

/// An airlock's `say()`: its name and the text to everyone near.
fn say(world: &mut World, speaker: Entity, text: &str) {
    super::machine_say(world, speaker, text);
}

/// Bolted airlocks someone tried to open (`airlock_act`).
pub fn cycle_requests(world: &mut World) {
    let mut q = world.query::<(Entity, &mut Door)>();
    let mut asked = Vec::new();
    for (e, mut door) in q.iter_mut(world) {
        if door.cycle_requested {
            door.cycle_requested = false;
            if let Some(pump) = door.cycle_pump {
                asked.push((pump, e));
            }
        }
    }
    for (pump, airlock) in asked {
        airlock_act(world, pump, airlock);
    }
}

/// `airlock_act`: a busy pump denies; else the side decides which way to cycle.
fn airlock_act(world: &mut World, pump: Entity, airlock: Entity) {
    let Some(p) = world.get::<AirlockPump>(pump).cloned() else {
        return;
    };
    if p.on {
        deny(world, airlock);
        return;
    }
    if !p.set_up {
        say(world, airlock, "Airlock pair not found.");
        return;
    }
    if p.external.contains(&airlock) {
        start_cycle(world, pump, false, Some(airlock));
    } else if p.internal.contains(&airlock) {
        start_cycle(world, pump, true, Some(airlock));
    }
}

/// `start_cycle`: everything closes and bolts; the cycle itself starts a second later.
fn start_cycle(world: &mut World, pump: Entity, releasing: bool, source: Option<Entity>) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let powered = world.get::<Unpowered>(pump).is_none();
    let Some(p) = world.get::<AirlockPump>(pump).cloned() else {
        return;
    };
    if p.on || !p.set_up || now < p.animating_until || !powered || p.pending.is_some() {
        return;
    }
    for &a in p.internal.iter().chain(&p.external) {
        secure(world, a, false);
    }
    let mut p = world.get_mut::<AirlockPump>(pump).expect("checked");
    p.releasing = releasing;
    p.animating_until = now + ANIMATION;
    p.pending = Some((now + ANIMATION, Pending::Run { releasing, source }));
}

/// The rest of `start_cycle` after the closing animation.
fn run_cycle(world: &mut World, pump: Entity, releasing: bool, source: Option<Entity>) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let Some(&pos) = world.get::<TilePos>(pump) else {
        return;
    };
    let tile = world
        .resource::<Atmos>()
        .air(pos)
        .map_or(0.0, |a| a.pressure().max(0.0));
    let distro = world
        .get::<AtmosDevice>(pump)
        .and_then(|d| {
            world
                .resource::<Pipenet>()
                .air(d.id, 0)
                .map(|a| a.pressure())
        })
        .unwrap_or(0.0);
    let Some(mut p) = world.get_mut::<AirlockPump>(pump) else {
        return;
    };
    p.on = true;
    p.timeout_at = Some(now + CYCLE_TIMEOUT);
    let (internal, external, audible) = (p.internal.clone(), p.external.clone(), p.audible);
    if releasing {
        p.cycle_target = p.internal_target;
        if p.cycle_target - tile <= PRESSURE_ERROR {
            stop_cycle(world, pump, Some("Pressure nominal, cycle skipped."), false);
            return;
        }
        if distro < MIN_DISTRO_PRESSURE {
            stop_cycle(
                world,
                pump,
                Some("Low pipe pressure, cycle skipped. Proceed with caution."),
                true,
            );
            return;
        }
        if audible && let Some(a) = source.or_else(|| internal.first().copied()) {
            say(world, a, "Pressurizing airlock.");
        }
    } else {
        p.cycle_target = p.external_target;
        if tile - p.cycle_target <= PRESSURE_ERROR {
            stop_cycle(world, pump, Some("Pressure nominal, cycle skipped."), false);
            return;
        }
        if audible && let Some(a) = source.or_else(|| external.first().copied()) {
            say(world, a, "Decompressing airlock.");
        }
    }
}

/// `stop_cycle`: the side the cycle was for unbolts (and opens unless `unbolt_only`); the
/// message comes a second later from its first airlock.
fn stop_cycle(
    world: &mut World,
    pump: Entity,
    message: Option<&'static str>,
    unbolt_only: bool,
) -> bool {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let Some(p) = world.get::<AirlockPump>(pump).cloned() else {
        return false;
    };
    if !p.on {
        return false;
    }
    let unlocked = if p.releasing { p.internal } else { p.external };
    for &a in &unlocked {
        unbolt(world, a);
        if p.open_on_cycle && !unbolt_only {
            secure(world, a, true);
        }
    }
    let mut pm = world.get_mut::<AirlockPump>(pump).expect("checked");
    pm.on = false;
    pm.timeout_at = None;
    pm.animating_until = now + ANIMATION;
    pm.pending = match (message, unlocked.first()) {
        (Some(text), Some(&airlock)) if p.audible => {
            Some((now + ANIMATION, Pending::Say { airlock, text }))
        }
        _ => None,
    };
    true
}

/// Pending steps, the timeout and power changes, every frame.
pub fn pump_timers(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut q = world.query::<(Entity, &AirlockPump, Has<Unpowered>)>();
    let pumps: Vec<(Entity, AirlockPump, bool)> =
        q.iter(world).map(|(e, p, u)| (e, p.clone(), !u)).collect();
    for (e, mut p, powered) in pumps {
        if p.set_up && unlink_gone(world, e, &mut p) {
            continue;
        }
        if let Some((_, step)) = p.pending.clone().filter(|(at, _)| now >= *at) {
            world.get_mut::<AirlockPump>(e).expect("listed").pending = None;
            match step {
                Pending::Run { releasing, source } => run_cycle(world, e, releasing, source),
                Pending::Say { airlock, text } => say(world, airlock, text),
            }
        }
        // `emergency_stop`.
        if p.on && p.timeout_at.is_some_and(|t| now >= t) && now >= p.animating_until {
            stop_cycle(world, e, Some("Cycling timed out, bolts unlocked."), true);
        }
        // `on_set_is_operational`.
        if p.was_powered != powered {
            world.get_mut::<AirlockPump>(e).expect("listed").was_powered = powered;
            if powered {
                for &a in &p.external {
                    secure(world, a, false);
                }
                if p.open_on_cycle {
                    for &a in &p.internal {
                        secure(world, a, true);
                    }
                }
            } else {
                for &a in p.internal.iter().chain(&p.external) {
                    unbolt(world, a);
                }
                let mut pm = world.get_mut::<AirlockPump>(e).expect("listed");
                pm.on = false;
                pm.timeout_at = None;
            }
        }
    }
}

/// `unlink_airlock` (`COMSIG_QDELETING` of a linked airlock): a gone airlock leaves its list,
/// and a side left empty breaks the whole setup (`break_all_links`). True when it broke.
fn unlink_gone(world: &mut World, pump: Entity, p: &mut AirlockPump) -> bool {
    let alive = |world: &World, a: &Entity| world.get::<Door>(*a).is_some();
    if p.internal
        .iter()
        .chain(&p.external)
        .all(|a| alive(world, a))
    {
        return false;
    }
    p.internal.retain(|a| alive(world, a));
    p.external.retain(|a| alive(world, a));
    let broke = p.internal.is_empty() || p.external.is_empty();
    if broke {
        for &a in p.internal.iter().chain(&p.external) {
            unbolt(world, a);
            if let Some(mut d) = world.get_mut::<Door>(a) {
                d.cycle_pump = None;
            }
        }
        p.internal.clear();
        p.external.clear();
        p.set_up = false;
        p.on = false;
        p.timeout_at = None;
    }
    let mut pm = world.get_mut::<AirlockPump>(pump).expect("listed");
    pm.internal = p.internal.clone();
    pm.external = p.external.clone();
    pm.set_up = p.set_up;
    pm.on = p.on;
    pm.timeout_at = p.timeout_at;
    broke
}

/// `process_atmos` of a cycling pump: fill from distro or siphon to waste, its tile and the
/// tiles air reaches from it, until the target pressure.
pub fn process_airlock_pumps(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut q = world.query::<(Entity, &AirlockPump, &AtmosDevice, &TilePos)>();
    let pumps: Vec<(Entity, AirlockPump, AtmosDevice, TilePos)> = q
        .iter(world)
        .filter(|(_, p, ..)| p.on)
        .map(|(e, p, d, &pos)| (e, p.clone(), *d, pos))
        .collect();
    for (e, p, device, pos) in pumps {
        let Some(tile) = world.resource::<Atmos>().air(pos).map(|a| a.pressure()) else {
            continue;
        };
        // `check_turfs` every 2 s.
        let adjacent = if now >= p.check_turfs_at {
            let adj = world.resource::<Atmos>().adjacent_turfs(pos, true);
            let mut pm = world.get_mut::<AirlockPump>(e).expect("listed");
            pm.adjacent = adj.clone();
            pm.check_turfs_at = now + 2.0;
            adj
        } else {
            p.adjacent.clone()
        };
        let tiles: Vec<TilePos> = std::iter::once(pos).chain(adjacent).collect();
        if p.releasing {
            let delta = p.cycle_target - tile;
            if delta <= PRESSURE_ERROR
                && stop_cycle(world, e, Some("Pressurization complete."), false)
            {
                continue;
            }
            world.resource_scope(|world, mut net: Mut<Pipenet>| {
                let mut atmos = world.resource_mut::<Atmos>();
                let Some(distro) = net.air(device.id, 0).cloned() else {
                    return;
                };
                let split = quantize(distro.total_moles() / tiles.len() as f64);
                for &t in &tiles {
                    let wanted =
                        VOLUME_RATE * delta / (distro.temperature() * R_IDEAL_GAS_EQUATION);
                    let moles = split.min(wanted);
                    let Some(removed) = net.air_mut(device.id, 0).and_then(|a| a.remove(moles))
                    else {
                        break;
                    };
                    atmos.merge_air(t, &removed);
                }
                net.mark_parents(device.id);
            });
        } else {
            let delta = tile - p.cycle_target;
            if delta <= PRESSURE_ERROR
                && stop_cycle(world, e, Some("Decompression complete."), false)
            {
                continue;
            }
            world.resource_scope(|world, mut net: Mut<Pipenet>| {
                let mut atmos = world.resource_mut::<Atmos>();
                for &t in &tiles {
                    let Some(moles) = atmos
                        .air(t)
                        .map(|a| a.total_moles() * (VOLUME_RATE / CELL_VOLUME))
                    else {
                        continue;
                    };
                    if let Some(removed) = atmos.remove_air(t, moles)
                        && let Some(waste) = net.air_mut(device.id, 1)
                    {
                        waste.merge(&removed);
                    }
                }
                net.mark_parents(device.id);
            });
        }
    }
}
