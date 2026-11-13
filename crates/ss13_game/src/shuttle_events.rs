//! Emergency shuttle flight events (server), after tg `code/modules/shuttle/shuttle_events/`
//! (`_shuttle_events.dm`, `turbulence.dm`, `meteors.dm`) and the emergency port's
//! `setup_shuttle_events`, `start_up_event` and `process_events`.
//!
//! - When the round's emergency shuttle is set up each event is rolled by its
//!   `event_probability` ([`EVENTS`]). At the launch an event activates after its
//!   `activation_fraction` of the flight, then runs once a second until the shuttle docks.
//! - Turbulence: a minor announcement on activation, then every 20–50 s the camera of everyone
//!   in the escape shuttle shakes (a quarter as hard when buckled) and 2 s later everyone not
//!   buckled is paralyzed for 3 s.
//! - Meteor spawners (dust, safe, meaty): each second they may throw meteors from the transit
//!   level's edge ahead of the shuttle (tg `preferred_direction` north, so they come from the
//!   north and fly south), from above the shuttle (hitting it) or beside it (missing it).
// shortcut: no carp, player-controlled, human, item or black hole events (no hyperspace drift
// for spawned things, no migrating carp), no template event lists (the humpback's carp), no
// ghost notification of the selected events (`announce_shuttle_events`), turbulence does not
// ignore stun immunity. The transit level keeps the port's small margin, so the spawn line is
// clamped to its top edge instead of 16 tiles out.

use bevy::ecs::message::Messages;
use bevy::prelude::*;
use ss13_mapload::spawner::Rng;
use ss13_mapload::{Areas, MapSeed};
use ss13_net::camera::server::SendCameraEffect;
use ss13_shared::TilePos;
use ss13_shared::protocol::CameraEffect;
use ss13_world::SpatialIndex;

use crate::health::Health;
use crate::medical::surgery::Buckled;
use crate::obj_damage::is_under;
use crate::server::PlayerMob;
use crate::shuttle::{EmergencyShuttle, MobileDock, ShuttleMode, ordered_turfs};
use crate::status_effects::Incap;

/// tg `SHUTTLE_TRANSIT_BORDER`.
const TRANSIT_BORDER: i32 = 16;
const ESCAPE_AREA: &str = "/area/shuttle/escape";
/// `SSshuttle` `wait`: events process once a second.
const PROCESS_SECS: f64 = 1.0;
/// Turbulence: `minimum_interval`, `maximum_interval`, `warning_interval` and the first wait
/// after the announcement (`rand(5 SECONDS, 20 SECONDS)`).
const TURBULENCE_MIN: i64 = 20;
const TURBULENCE_MAX: i64 = 50;
const TURBULENCE_WARNING: f64 = 2.0;
const TURBULENCE_FIRST: (i64, i64) = (5, 20);
const TURBULENCE_PARALYZE: f32 = 3.0;

/// What an event does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventKind {
    Turbulence,
    /// `/datum/shuttle_event/simple_spawner/meteor`.
    Meteors,
}

/// A `/datum/shuttle_event` subtype.
#[derive(Clone, Copy, Debug)]
pub struct EventDef {
    pub name: &'static str,
    /// `event_probability`, percent.
    pub probability: f32,
    pub activation_fraction: f64,
    pub kind: EventKind,
    /// `spawning_flags`: `SHUTTLE_EVENT_HIT_SHUTTLE`, `SHUTTLE_EVENT_MISS_SHUTTLE`.
    pub hit: bool,
    pub miss: bool,
    /// `spawn_probability_per_process`, percent.
    pub spawn_probability: f32,
    pub spawns_per_spawn: u32,
    /// `hit_the_shuttle_chance` (percent) of the `get_spawn_turf` overrides; `None` picks from
    /// both lists alike.
    pub hit_chance: Option<f32>,
    /// `spawning_list`: meteor paths and weights.
    pub spawning: &'static [(&'static str, i64)],
}

const NONE: EventDef = EventDef {
    name: "",
    probability: 0.0,
    activation_fraction: 0.0,
    kind: EventKind::Meteors,
    hit: true,
    miss: true,
    spawn_probability: 0.0,
    spawns_per_spawn: 1,
    hit_chance: None,
    spawning: &[],
};

/// The events rolled for every emergency shuttle.
pub const EVENTS: [EventDef; 4] = [
    EventDef {
        name: "Turbulence",
        probability: 5.0,
        activation_fraction: 0.1,
        kind: EventKind::Turbulence,
        ..NONE
    },
    EventDef {
        name: "Dust Meteors! (Mostly Safe)",
        probability: 2.0,
        activation_fraction: 0.1,
        spawn_probability: 100.0,
        spawns_per_spawn: 5,
        hit_chance: Some(1.0),
        spawning: &[
            ("/obj/effect/meteor/dust", 1),
            ("/obj/effect/meteor/sand", 1),
        ],
        ..NONE
    },
    EventDef {
        name: "Various Meteors! (Safe)",
        probability: 5.0,
        activation_fraction: 0.1,
        hit: false,
        spawn_probability: 100.0,
        spawns_per_spawn: 6,
        spawning: &[
            ("/obj/effect/meteor/medium", 10),
            ("/obj/effect/meteor/big", 5),
            ("/obj/effect/meteor/flaming", 3),
            ("/obj/effect/meteor/cluster", 1),
            ("/obj/effect/meteor/irradiated", 3),
            ("/obj/effect/meteor/bluespace", 2),
        ],
        ..NONE
    },
    EventDef {
        name: "Meaty Meteors! (Mostly Safe)",
        probability: 0.1,
        activation_fraction: 0.1,
        spawn_probability: 100.0,
        spawns_per_spawn: 3,
        hit_chance: Some(2.0),
        spawning: &[("/obj/effect/meteor/meaty", 1)],
        ..NONE
    },
];

/// A running `/datum/shuttle_event`.
#[derive(Clone, Debug)]
pub struct ShuttleEvent {
    pub def: &'static EventDef,
    /// `activate_at`; `None` before the launch.
    pub activate_at: Option<f64>,
    pub active: bool,
    /// Turbulence: `turbulence_cooldown` and the pending `knock_down`.
    next_shake: f64,
    knock_at: Option<f64>,
    /// `spawning_turfs_hit`, `spawning_turfs_miss`.
    hit: Vec<TilePos>,
    miss: Vec<TilePos>,
}

impl ShuttleEvent {
    pub fn new(def: &'static EventDef) -> Self {
        Self {
            def,
            activate_at: None,
            active: false,
            next_shake: 0.0,
            knock_at: None,
            hit: Vec::new(),
            miss: Vec::new(),
        }
    }
}

/// The emergency shuttle's `event_list`.
#[derive(Resource)]
pub struct ShuttleEvents {
    pub events: Vec<ShuttleEvent>,
    next_process: f64,
    rng: Rng,
}

impl ShuttleEvents {
    pub fn new(seed: u64) -> Self {
        Self {
            events: Vec::new(),
            next_process: 0.0,
            rng: Rng::new(seed),
        }
    }

    /// `add_shuttle_event`.
    pub fn add(&mut self, def: &'static EventDef) {
        self.events.push(ShuttleEvent::new(def));
    }
}

/// `setup_shuttle_events`: every event by its probability.
pub(crate) fn setup(world: &mut World) {
    let seed = world.get_resource::<MapSeed>().map_or(0, |s| s.0) ^ 0x5345_5645;
    let mut events = ShuttleEvents::new(seed);
    for def in &EVENTS {
        if def.probability > 0.0 && events.rng.prob(def.probability) {
            events.add(def);
        }
    }
    if !events.events.is_empty() {
        let names: Vec<&str> = events.events.iter().map(|e| e.def.name).collect();
        info!("the emergency shuttle has selected: {}", names.join(", "));
    }
    world.insert_resource(events);
}

/// tg `start_up_event(evacuation_duration)` for every event, right after the shuttle entered
/// transit: activation times and the spawners' tiles.
pub(crate) fn start_up(world: &mut World, now: f64, evacuation_secs: f64) {
    let (hit, miss) = spawning_turfs(world);
    let Some(mut events) = world.get_resource_mut::<ShuttleEvents>() else {
        return;
    };
    for event in &mut events.events {
        event.activate_at = Some(now + evacuation_secs * event.def.activation_fraction);
        if event.def.kind == EventKind::Meteors {
            event.hit = if event.def.hit { hit.clone() } else { vec![] };
            event.miss = if event.def.miss { miss.clone() } else { vec![] };
        }
    }
    events.next_process = now;
}

/// `generate_spawning_turfs(return_coords(), spawning_flags, NORTH)` on the transit level: the
/// row ahead of the shuttle over its width (hit) and beside it (miss).
fn spawning_turfs(world: &World) -> (Vec<TilePos>, Vec<TilePos>) {
    let Some(shuttle) = world.get_resource::<EmergencyShuttle>() else {
        return (vec![], vec![]);
    };
    let (Some(mobile), Some(&origin)) = (
        world.get::<MobileDock>(shuttle.port),
        world.get::<TilePos>(shuttle.port),
    ) else {
        return (vec![], vec![]);
    };
    let turfs = ordered_turfs(origin, mobile.dir, mobile.size);
    let Some((w, h)) = world.resource::<SpatialIndex>().level_size(origin.z) else {
        return (vec![], vec![]);
    };
    let (min_x, max_x) = turfs
        .iter()
        .fold((i32::MAX, i32::MIN), |(a, b), p| (a.min(p.x), b.max(p.x)));
    let max_y = turfs.iter().map(|p| p.y).max().unwrap_or(1);
    let y = (max_y + TRANSIT_BORDER).min(h as i32);
    let at = |x: i32| (x >= 1 && x <= w as i32).then(|| TilePos::new(x, y, origin.z));
    let hit = (min_x..=max_x).filter_map(at).collect();
    let miss = (1..=TRANSIT_BORDER)
        .flat_map(|i| [at(min_x - i), at(max_x + i)])
        .flatten()
        .collect();
    (hit, miss)
}

/// `process_events` while the emergency shuttle flies, once a second.
pub(crate) fn process(world: &mut World, now: f64) {
    if !world.contains_resource::<ShuttleEvents>() {
        return;
    }
    {
        let events = world.resource::<ShuttleEvents>();
        if events.events.is_empty() || now < events.next_process {
            return;
        }
    }
    world.resource_mut::<ShuttleEvents>().next_process = now + PROCESS_SECS;
    let count = world.resource::<ShuttleEvents>().events.len();
    let mut cleared = Vec::new();
    for i in 0..count {
        if !process_event(world, i, now) {
            cleared.push(i);
        }
    }
    let mut events = world.resource_mut::<ShuttleEvents>();
    for i in cleared.into_iter().rev() {
        events.events.remove(i);
    }
}

/// One `event_process`; false for `SHUTTLE_EVENT_CLEAR`.
fn process_event(world: &mut World, i: usize, now: f64) -> bool {
    let mut events = world.resource_mut::<ShuttleEvents>();
    let event = &mut events.events[i];
    if !event.active {
        if event.activate_at.is_none_or(|at| now < at) {
            return true;
        }
        event.active = true;
        let kind = event.def.kind;
        if kind == EventKind::Turbulence {
            let (lo, hi) = TURBULENCE_FIRST;
            let wait = events.rng.range(lo, hi) as f64;
            events.events[i].next_shake = now + wait;
            crate::security_level::minor_announce(
                world,
                "Эвакуационный шаттл",
                "Обратите внимание, что мы входим в зону подпрастранственной турбулентности. Для \
                 вашей же безопасности, пожалуйста, пристегните ремни безопасности и оставайтесь \
                 на своих местах, пока шаттл полностью не остановится.",
                true,
            );
        }
        return true;
    }
    let kind = event.def.kind;
    match kind {
        EventKind::Turbulence => turbulence(world, i, now),
        EventKind::Meteors => spawn_meteors(world, i),
    }
}

/// Turbulence's `event_process`: the shake when due, the knock-down 2 s after it.
fn turbulence(world: &mut World, i: usize, now: f64) -> bool {
    let mut events = world.resource_mut::<ShuttleEvents>();
    if events.events[i].knock_at.is_some_and(|at| now >= at) {
        events.events[i].knock_at = None;
        knock_down(world);
        events = world.resource_mut::<ShuttleEvents>();
    }
    if now < events.events[i].next_shake {
        return true;
    }
    let wait = events.rng.range(TURBULENCE_MIN, TURBULENCE_MAX) as f64;
    events.events[i].next_shake = now + wait;
    events.events[i].knock_at = Some(now + TURBULENCE_WARNING);
    shake(world);
    true
}

/// The living mobs in the escape shuttle's areas (`mobs_in_area_type`).
fn mobs_aboard(world: &mut World) -> Vec<Entity> {
    let mut q = world.query_filtered::<(Entity, &TilePos), With<Health>>();
    let mobs: Vec<(Entity, TilePos)> = q.iter(world).map(|(e, p)| (e, *p)).collect();
    let areas = world.resource::<Areas>();
    mobs.into_iter()
        .filter(|(_, pos)| {
            areas
                .area_at(*pos)
                .and_then(|a| world.get::<ss13_mapload::DmPower>(a))
                .is_some_and(|p| is_under(&p.path, ESCAPE_AREA))
        })
        .map(|(e, _)| e)
        .collect()
}

/// `shake`: every player aboard feels it, a quarter as hard when buckled.
fn shake(world: &mut World) {
    if !world.contains_resource::<Messages<SendCameraEffect>>() {
        return;
    }
    for mob in mobs_aboard(world) {
        let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) else {
            continue;
        };
        let strength = if world.get::<Buckled>(mob).is_some() {
            0.25
        } else {
            1.0
        };
        world.write_message(SendCameraEffect {
            client,
            effect: CameraEffect::Shake {
                duration_ds: 30.0,
                strength,
            },
        });
    }
}

/// `knock_down`: whoever is aboard and not buckled is paralyzed, unless the shuttle docked.
fn knock_down(world: &mut World) {
    if world
        .get_resource::<EmergencyShuttle>()
        .is_none_or(|s| s.state.mode != ShuttleMode::Escape)
    {
        return;
    }
    for mob in mobs_aboard(world) {
        if world.get::<Buckled>(mob).is_none() {
            crate::status_effects::apply(world, mob, Incap::Paralyze, TURBULENCE_PARALYZE);
        }
    }
}

/// `simple_spawner/event_process` with the meteor `spawn_movable`: meteors from the spawn tiles
/// towards the edge behind the shuttle.
fn spawn_meteors(world: &mut World, i: usize) -> bool {
    let mut events = world.resource_mut::<ShuttleEvents>();
    let def = events.events[i].def;
    if def.spawning.is_empty() || !events.rng.prob(def.spawn_probability) {
        return true;
    }
    let mut throws = Vec::new();
    for _ in 0..def.spawns_per_spawn {
        let event = &events.events[i];
        let (hit, miss) = (event.hit.clone(), event.miss.clone());
        let rng = &mut events.rng;
        let from_hit = match def.hit_chance {
            Some(chance) => rng.prob(chance),
            None => rng.range(0, (hit.len() + miss.len()) as i64 - 1) < hit.len() as i64,
        };
        let pool = if from_hit && !hit.is_empty() || miss.is_empty() {
            &hit
        } else {
            &miss
        };
        if pool.is_empty() {
            continue;
        }
        let start = pool[rng.range(0, pool.len() as i64 - 1) as usize];
        let Some(path) = crate::events::controller::pick_weight(def.spawning, rng) else {
            continue;
        };
        throws.push((start, path));
    }
    for (start, path) in throws {
        let Some(kind) = crate::events::meteors::kind_by_path(path) else {
            continue;
        };
        // `get_edge_target_turf(spawn_turf, SOUTH)`: opposite the flight.
        let goal = TilePos::new(start.x, 1, start.z);
        crate::events::meteors::spawn_meteor_at(world, kind, start, goal);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_match_tg() {
        let names: Vec<&str> = EVENTS.iter().map(|e| e.name).collect();
        assert_eq!(names.len(), 4);
        let safe = &EVENTS[2];
        assert!(!safe.hit && safe.miss, "the safe meteors never hit");
        assert!(
            EVENTS
                .iter()
                .flat_map(|e| e.spawning)
                .all(|(p, _)| crate::events::meteors::kind_by_path(p).is_some()),
            "every meteor path is a ported meteor"
        );
    }
}
