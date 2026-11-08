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
//!
//! A shuttle docking at the external airlock (`crate::shuttle`: tg `afterShuttleMove`) tells the
//! pump its pressure: cycling out aims for it instead of the vacuum, and with the chamber and
//! the shuttle both at the internal pressure both sides open (`safe_dock`). Leaving closes the
//! external side (`undock`).
// shortcut: a shuttle-mounted pump does not count CentCom's `poddoor/shuttledock` as an airlock
// to dock with; port it when a shuttle with a pump docks there. No pipe underlay drawing, no
// unwrenching or rebuilding.

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
use ss13_mapload::{Areas, DoorKind};

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
/// `on_dock_request`'s `stoplag(1.1 SECONDS)` behind a door animation.
const DOCK_QUEUE: f64 = 1.1;
/// The airlocks a shuttle's airlock counts as docked with (`orange(2, src)`).
const DOCK_RANGE: i32 = 2;

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
    /// `docked_side_pressure`: the docked vessel's, overriding `external_target`.
    pub docked_side_pressure: Option<f64>,
    dock_check_at: Option<f64>,
    animating_until: f64,
    pending: Option<(f64, Pending)>,
    adjacent: Vec<TilePos>,
    check_turfs_at: f64,
    was_powered: bool,
}

/// `post_machine_initialize` -> `set_links` of every mapped pump.
pub fn init_airlock_pumps(world: &mut World) {
    let mut q = world.query_filtered::<(Entity, &DmAtmosDevice, &AtmosDevice, &TilePos), With<ss13_world::Mapped>>();
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
            docked_side_pressure: None,
            dock_check_at: None,
            animating_until: 0.0,
            pending: None,
            adjacent: Vec::new(),
            check_turfs_at: 0.0,
            was_powered: true,
        };
        set_links(world, e, &mut pump, pos, dir);
        world.entity_mut(e).insert(pump);
        dock_at_round_start(world, e, pos);
    }
}

/// tg `isshuttleturf`: the turf stands on a shuttle's skipover.
fn on_shuttle(world: &World, pos: TilePos) -> bool {
    crate::construction::turf_at(world, pos).is_some_and(|t| {
        crate::construction::baseturfs_of(world, t)
            .iter()
            .any(|b| b == ss13_mapload::dock::SHUTTLE_SKIPOVER)
    })
}

/// The shuttles docked before this pass ran (`Startup`), so no `after_shuttle_move` reached the
/// pump (tg `post_machine_initialize`): an airlock across the shuttle's edge means docked.
fn dock_at_round_start(world: &mut World, pump: Entity, pos: TilePos) {
    let Some(p) = world.get::<AirlockPump>(pump).cloned() else {
        return;
    };
    let Some(front) = pump_dir(world, pump) else {
        return;
    };
    let mounted = on_shuttle(world, pos);
    for &airlock in &p.external {
        let Some(&at) = world.get::<TilePos>(airlock) else {
            continue;
        };
        let across = airlocks_near(world, at).into_iter().find(|&(other, tile)| {
            !p.external.contains(&other)
                && !p.internal.contains(&other)
                && on_shuttle(world, tile) != mounted
        });
        if let Some((_, tile)) = across {
            let pressure = pressure_at(world, tile.step(front.reverse()));
            on_dock_request(world, pump, pressure);
            return;
        }
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
    let mut internal = adjacent_airlocks(world, find_density(world, pos, front), sides);
    let mut external = adjacent_airlocks(world, find_density(world, pos, front.reverse()), sides);
    // "support for awkwardly shaped airlocks that cycle on a group ID" (`crate::airlock_cycle`).
    if internal.is_empty() {
        internal = cycle_group(world, &external);
    }
    if external.is_empty() {
        external = cycle_group(world, &internal);
    }
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

/// The `close_others` of `side`'s airlocks that are not on `side` themselves.
fn cycle_group(world: &World, side: &[Entity]) -> Vec<Entity> {
    let mut out = Vec::new();
    for door in side.iter().filter_map(|&a| world.get::<Door>(a)) {
        for &other in &door.close_others {
            if !side.contains(&other) && !out.contains(&other) {
                out.push(other);
            }
        }
    }
    out
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
        if let Some(docked) = p.docked_side_pressure
            && !beyond_matches(world, pump, airlock, docked)
        {
            deny(world, airlock);
            return;
        }
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
        p.cycle_target = p.docked_side_pressure.unwrap_or(p.external_target);
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
    if p.docked_side_pressure.is_some() {
        let mut pm = world.get_mut::<AirlockPump>(pump).expect("checked");
        pm.on = false;
        pm.timeout_at = None;
        if safe_dock(world, pump, unbolt_only) {
            return true;
        }
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

/// `safe_dock`'s pressure check: the docked vessel is neither above the internal target nor
/// too far below it to open both sides.
fn docking_safe(internal_target: f64, docked: f64) -> bool {
    let delta = internal_target - docked;
    delta + PRESSURE_ERROR >= 0.0 && delta <= PRESSURE_ERROR * 10.0
}

fn pressure_at(world: &World, pos: TilePos) -> f64 {
    world
        .resource::<Atmos>()
        .air(pos)
        .map_or(0.0, |a| a.pressure().max(0.0))
}

fn pump_dir(world: &World, pump: Entity) -> Option<Dir> {
    Dir::from_bits(world.get::<AtmosDevice>(pump)?.dir)
}

/// The tile past an external airlock has the pressure declared on docking and is no space.
fn beyond_matches(world: &World, pump: Entity, airlock: Entity, docked: f64) -> bool {
    let (Some(front), Some(&pos)) = (pump_dir(world, pump), world.get::<TilePos>(airlock)) else {
        return false;
    };
    let tile = pos.step(front.reverse());
    !crate::construction::is_space(world, tile)
        && (docked - pressure_at(world, tile)).abs() <= PRESSURE_ERROR * 10.0
}

/// `safe_dock`: with the chamber and the vessel at the internal pressure, both sides open
/// (external airlocks only onto the declared pressure). False leaves everything as it was.
fn safe_dock(world: &mut World, pump: Entity, unbolt_only: bool) -> bool {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let Some(p) = world.get::<AirlockPump>(pump).cloned() else {
        return false;
    };
    let Some(docked) = p.docked_side_pressure else {
        return false;
    };
    let Some(&pos) = world.get::<TilePos>(pump) else {
        return false;
    };
    if !docking_safe(p.internal_target, docked)
        || world.get::<Unpowered>(pump).is_some()
        || p.internal_target - pressure_at(world, pos) > PRESSURE_ERROR
    {
        return false;
    }
    let external = p
        .external
        .iter()
        .copied()
        .filter(|&a| beyond_matches(world, pump, a, docked))
        .collect::<Vec<_>>();
    for &a in external.iter().chain(&p.internal) {
        unbolt(world, a);
        if p.open_on_cycle && !unbolt_only {
            secure(world, a, true);
        }
    }
    let mut pm = world.get_mut::<AirlockPump>(pump).expect("checked");
    pm.animating_until = now + ANIMATION;
    pm.pending = Some((
        now + ANIMATION,
        Pending::Say {
            airlock: pump,
            text: "Docking complete.",
        },
    ));
    true
}

/// `on_dock_request`: a vessel with `pressure` docked at the external side.
pub fn on_dock_request(world: &mut World, pump: Entity, pressure: f64) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let powered = world.get::<Unpowered>(pump).is_none();
    let Some(mut p) = world.get_mut::<AirlockPump>(pump) else {
        return;
    };
    if p.docked_side_pressure.is_some() {
        return;
    }
    p.docked_side_pressure = Some(pressure);
    if !powered || !p.set_up {
        return;
    }
    if now < p.animating_until {
        p.dock_check_at = Some(now + DOCK_QUEUE);
        say(world, pump, "Docking request queued.");
        return;
    }
    docking_step(world, pump);
}

/// The rest of `on_dock_request` once no door is moving.
fn docking_step(world: &mut World, pump: Entity) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let Some(p) = world.get::<AirlockPump>(pump).cloned() else {
        return;
    };
    if p.docked_side_pressure.is_none() {
        return;
    }
    if now < p.animating_until {
        say(
            world,
            pump,
            "ERROR: D11. Please re-initiate docking sequence.",
        );
        return;
    }
    if p.on {
        // "You can't go there, there is a shuttle now."
        if !p.releasing {
            stop_cycle(
                world,
                pump,
                Some("Cycling sequence overriden by docking sequence."),
                true,
            );
            let mut pm = world.get_mut::<AirlockPump>(pump).expect("checked");
            pm.dock_check_at = Some(pm.animating_until + ANIMATION / 10.0);
        }
        return;
    }
    let Some(&pos) = world.get::<TilePos>(pump) else {
        return;
    };
    if p.internal_target - pressure_at(world, pos) <= PRESSURE_ERROR {
        safe_dock(world, pump, false);
    } else {
        if let Some(&a) = p.internal.first() {
            say(world, a, "Docking sequence initiated");
        }
        start_cycle(world, pump, true, None);
    }
}

/// `undock`: the vessel left; the external side closes and bolts.
pub fn undock(world: &mut World, pump: Entity) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let powered = world.get::<Unpowered>(pump).is_none();
    let Some(mut p) = world.get_mut::<AirlockPump>(pump) else {
        return;
    };
    if p.docked_side_pressure.take().is_none() {
        return;
    }
    p.dock_check_at = None;
    if !powered {
        return;
    }
    p.animating_until = now + ANIMATION;
    let external = p.external.clone();
    for a in external {
        secure(world, a, false);
    }
    say(world, pump, "Docking connection terminated.");
}

/// The airlocks standing on `tiles`.
pub fn airlocks_on(world: &World, tiles: &[TilePos]) -> Vec<Entity> {
    tiles
        .iter()
        .flat_map(|&t| world.resource::<SpatialIndex>().on_tile(t).to_vec())
        .filter(|&e| {
            world
                .get::<Door>(e)
                .is_some_and(|d| d.kind == DoorKind::Airlock)
        })
        .collect()
}

/// The airlocks within [`DOCK_RANGE`] of `pos`, `pos` included.
fn airlocks_near(world: &World, pos: TilePos) -> Vec<(Entity, TilePos)> {
    let mut out = Vec::new();
    for y in -DOCK_RANGE..=DOCK_RANGE {
        for x in -DOCK_RANGE..=DOCK_RANGE {
            let tile = TilePos::new(pos.x + x, pos.y + y, pos.z);
            if world.resource::<SpatialIndex>().in_bounds(tile) {
                out.extend(airlocks_on(world, &[tile]).into_iter().map(|e| (e, tile)));
            }
        }
    }
    out
}

fn cycle_pump(world: &World, airlock: Entity) -> Option<Entity> {
    world.get::<Door>(airlock)?.cycle_pump
}

/// tg airlock `close(FALSE, TRUE)`: the usual checks, but whatever stands in the way is crushed.
fn close_crushing(world: &mut World, airlock: Entity) {
    with_door(world, airlock, |door, appearance, ctx| {
        let safe = std::mem::replace(&mut door.safe, false);
        let moved = ctx.close(door, appearance, false);
        door.safe = safe;
        ((), moved)
    });
}

/// tg airlock `beforeShuttleMove`: around each moving airlock (itself included) the pumps undock
/// and the airlocks without one shut.
pub fn before_shuttle_move(world: &mut World, airlocks: &[Entity]) {
    for &airlock in airlocks {
        let Some(&pos) = world.get::<TilePos>(airlock) else {
            continue;
        };
        for (other, _) in airlocks_near(world, pos) {
            match cycle_pump(world, other) {
                Some(pump) => undock(world, pump),
                None => close_crushing(world, other),
            }
        }
    }
}

/// tg airlock `afterShuttleMove`: an airlock of another area within reach means docked; each
/// side's pump learns the air past the other side's airlock.
pub fn after_shuttle_move(world: &mut World, airlocks: &[Entity]) {
    for &airlock in airlocks {
        let Some(&pos) = world.get::<TilePos>(airlock) else {
            continue;
        };
        let area = world.resource::<Areas>().area_at(pos);
        let others = airlocks_near(world, pos)
            .into_iter()
            .filter(|&(other, at)| {
                other != airlock && world.resource::<Areas>().area_at(at) != area
            })
            .collect::<Vec<_>>();
        for &(other, _) in &others {
            if let Some(pump) = cycle_pump(world, other)
                && let Some(front) = pump_dir(world, pump)
            {
                let pressure = pressure_at(world, pos.step(front.reverse()));
                on_dock_request(world, pump, pressure);
            }
        }
        if let (Some(pump), Some(&(_, at))) = (cycle_pump(world, airlock), others.last())
            && let Some(front) = pump_dir(world, pump)
        {
            let pressure = pressure_at(world, at.step(front.reverse()));
            on_dock_request(world, pump, pressure);
        }
    }
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
        if p.dock_check_at.is_some_and(|at| now >= at) {
            world
                .get_mut::<AirlockPump>(e)
                .expect("listed")
                .dock_check_at = None;
            docking_step(world, e);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docking_is_safe_only_near_the_internal_pressure() {
        assert!(docking_safe(ONE_ATMOSPHERE, ONE_ATMOSPHERE));
        assert!(docking_safe(
            ONE_ATMOSPHERE,
            ONE_ATMOSPHERE + PRESSURE_ERROR
        ));
        // A vessel above the chamber, or too far below it, keeps the doors shut.
        assert!(!docking_safe(
            ONE_ATMOSPHERE,
            ONE_ATMOSPHERE + 2.0 * PRESSURE_ERROR
        ));
        assert!(!docking_safe(ONE_ATMOSPHERE, 0.0));
        assert!(!docking_safe(
            ONE_ATMOSPHERE,
            ONE_ATMOSPHERE - 11.0 * PRESSURE_ERROR
        ));
    }
}
