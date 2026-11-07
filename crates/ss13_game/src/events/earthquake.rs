//! Chasmic Earthquake (`code/modules/events/earthquake.dm`), planetary maps only: a fault line
//! of turfs around a station epicenter (a rectangle between two opposite fracture points,
//! filtered to within 2 tiles of a rough line through them) rumbles for 25 event ticks: every
//! 10 the station's players feel it (camera shake, distant rumble); at tick 23 the living on
//! the fault are knocked down (7 s) and paralyzed (5 s); at 24 the fault takes a light blast
//! (90%), the rock under it is drilled out and metal creaks; at the end the fault is bombed
//! (devastating, 10% heavy) and its edge (3 tiles off) lightly or (25%) heavily, through the
//! explosion queue (`SSexplosions.*turf`) without an explosion, so the turfs break down to the
//! level below.
// shortcut: no turf `Shake` animation, no ghost notification, no achievement, no admin epicenter;
// the epicenter is a random station tile (tg: a `generic_event_spawn` landmark, not loaded) and
// asteroid turfs are not scraped away besides the blast.

use bevy::ecs::message::Messages;
use bevy::prelude::*;
use ss13_net::camera::server::SendCameraEffect;
use ss13_net::sound::server::SendSound;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_shared::protocol::{CameraEffect, SoundEvent};

use super::controller::{EventRng, EventState, RoundEvent, station_levels};
use crate::explosions::{Severity, queue_turfs};
use crate::server::PlayerMob;

pub const NAME: &str = "Chasmic Earthquake";

const END_WHEN: u32 = 25;
const DISTANT_RUMBLES: [&str; 4] = [
    "sound/ambience/earth_rumble/earth_rumble_distant1.ogg",
    "sound/ambience/earth_rumble/earth_rumble_distant2.ogg",
    "sound/ambience/earth_rumble/earth_rumble_distant3.ogg",
    "sound/ambience/earth_rumble/earth_rumble_distant4.ogg",
];
const RUMBLE: &str = "sound/ambience/earth_rumble/earth_rumble.ogg";
const METAL_CREAK: &str = "sound/misc/metal_creak.ogg";
const EXPLOSION_FAR: &str = "sound/effects/explosion/explosionfar.ogg";

/// `map_flags = EVENT_PLANETARY_ONLY`: the first station map's json says `"planetary"`
/// (tg `SSmapping.is_planetary()`).
pub fn is_planetary(world: &mut World) -> bool {
    let Some(plugin) = world.get_resource::<ss13_mapload::MapLoadPlugin>() else {
        return false;
    };
    let Some(map) = plugin.maps.first() else {
        return false;
    };
    let stem = std::path::Path::new(map)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(map);
    ss13_mapload::dock::map_json(&plugin.source, stem)
        .and_then(|json| json.get("planetary").cloned())
        .is_some_and(|v| v.as_bool().unwrap_or_else(|| v.as_i64().unwrap_or(0) != 0))
}

pub fn make(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(Earthquake::default())
}

/// `/datum/round_event/earthquake`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Earthquake {
    pub epicenter: Option<TilePos>,
    /// `turfs_to_shred`.
    pub fault: Vec<TilePos>,
    /// `underbelly`: the turfs below the fault.
    pub underbelly: Vec<TilePos>,
    pub edges: Vec<TilePos>,
}

/// `TURF_MIDPOINT(a, b)` (`locate` drops the half).
fn midpoint(a: TilePos, b: TilePos) -> TilePos {
    TilePos::new((a.x + b.x).div_euclid(2), (a.y + b.y).div_euclid(2), a.z)
}

/// `get_dist`.
fn dist(a: TilePos, b: TilePos) -> i32 {
    (a.x - b.x).abs().max((a.y - b.y).abs())
}

/// The fault and its edges between the fracture points `high` (north-east of `epicenter`)
/// and `low` (south-west): the tiles of their rectangle within 2 tiles of the rough line, and
/// those exactly 3 off.
pub fn fault_line(epicenter: TilePos, high: TilePos, low: TilePos) -> (Vec<TilePos>, Vec<TilePos>) {
    let high_mid = midpoint(high, epicenter);
    let low_mid = midpoint(low, epicenter);
    let line = [
        high,
        low,
        high_mid,
        low_mid,
        midpoint(high, high_mid),
        midpoint(low, low_mid),
        midpoint(high_mid, epicenter),
        midpoint(low_mid, epicenter),
    ];
    let (mut fault, mut edges) = (Vec::new(), Vec::new());
    for y in low.y.min(high.y)..=low.y.max(high.y) {
        for x in low.x.min(high.x)..=low.x.max(high.x) {
            let tile = TilePos::new(x, y, epicenter.z);
            let nearest = line
                .iter()
                .fold(dist(tile, epicenter), |d, &p| d.min(dist(tile, p)));
            match nearest {
                0..=2 => fault.push(tile),
                3 => edges.push(tile),
                _ => {}
            }
        }
    }
    (fault, edges)
}

impl RoundEvent for Earthquake {
    fn setup(&mut self, world: &mut World, ev: &mut EventState) {
        ev.start_when = 1;
        ev.announce_when = 3;
        ev.end_when = END_WHEN;
        ev.announce_chance = 25.0;
        let tiles = super::station_tiles(world);
        let Some(size) = tiles
            .first()
            .and_then(|&(_, p)| world.resource::<ss13_world::SpatialIndex>().level_size(p.z))
        else {
            ev.killed = true;
            return;
        };
        let rng = &mut world.resource_mut::<EventRng>().0;
        let (_, spot) = tiles[rng.range(0, tiles.len() as i64 - 1) as usize];
        let clamp = |x: i64, y: i64| {
            TilePos::new(
                x.clamp(1, size.0 as i64) as i32,
                y.clamp(1, size.1 as i64) as i32,
                spot.z,
            )
        };
        let epicenter = clamp(
            spot.x as i64 + rng.range(-10, 10),
            spot.y as i64 + rng.range(-10, 10),
        );
        let high = clamp(
            epicenter.x as i64 + rng.range(4, 10),
            epicenter.y as i64 + rng.range(4, 8),
        );
        let low = clamp(
            epicenter.x as i64 - rng.range(4, 10),
            epicenter.y as i64 - rng.range(4, 8),
        );
        let (fault, edges) = fault_line(epicenter, high, low);
        self.underbelly = world
            .get_resource::<crate::multiz::ZLevels>()
            .map(|levels| {
                fault
                    .iter()
                    .filter_map(|&t| levels.step(t, false))
                    .collect()
            })
            .unwrap_or_default();
        self.epicenter = Some(epicenter);
        self.fault = fault;
        self.edges = edges;
        info!(
            "{NAME}: epicenter {epicenter:?}, {} turfs",
            self.fault.len()
        );
    }

    fn announce(&mut self, world: &mut World, _: &mut EventState, _fake: bool) {
        super::priority_announce(
            world,
            "Отчёт о сейсмической активности",
            "Системы планетарного мониторинга указывают на разрушительное сейсмическое событие \
             в ближайшем будущем.",
        );
    }

    fn tick(&mut self, world: &mut World, ev: &mut EventState) {
        let active = ev.active_for;
        if active.is_multiple_of(10) {
            for client in station_clients(world) {
                let file = {
                    let rng = &mut world.resource_mut::<EventRng>().0;
                    DISTANT_RUMBLES[rng.range(0, 3) as usize]
                };
                shake_camera(world, client, 10.0, 1.0 + (active % 10) as f32);
                play_local(world, client, file, 75.0);
            }
        }
        if active == END_WHEN - 2 {
            for &tile in &self.fault {
                for mob in living_on(world, tile) {
                    crate::status_effects::knockdown(world, mob, 7.0, 0.0);
                    crate::status_effects::apply(
                        world,
                        mob,
                        crate::status_effects::Incap::Paralyze,
                        5.0,
                    );
                    tell(
                        world,
                        mob,
                        "Под вами яростно дрожит земля, сбивая вас с ног!",
                    );
                }
            }
            for &tile in &self.underbelly {
                for mob in living_on(world, tile) {
                    tell(world, mob, "Интересно, а что это за грохот?");
                }
            }
        }
        if active == END_WHEN - 1 {
            let light: Vec<TilePos> = {
                let rng = &mut world.resource_mut::<EventRng>().0;
                self.fault
                    .iter()
                    .copied()
                    .filter(|_| rng.prob(90.0))
                    .collect()
            };
            queue_turfs(world, &light, Severity::Light);
            for &tile in &self.underbelly {
                crate::mining::rocks::gets_drilled(world, tile, None);
            }
            if let Some(epicenter) = self.epicenter {
                crate::sound::playsound(
                    world,
                    crate::sound::PlaySound::new(epicenter, METAL_CREAK, 125.0, true),
                );
            }
        }
    }

    fn end(&mut self, world: &mut World, _: &mut EventState) {
        let Some(epicenter) = self.epicenter else {
            return;
        };
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(epicenter, RUMBLE, 125.0, false),
        );
        // tg skips everyone not on a level that is both station and mining (`||`); the intent
        // is the station's players.
        for client in station_clients(world) {
            shake_camera(world, client, 20.0, 4.0);
            play_local(world, client, EXPLOSION_FAR, 75.0);
        }
        let (mut heavy, mut devastate, mut edge_heavy, mut edge_light) =
            (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        {
            let rng = &mut world.resource_mut::<EventRng>().0;
            for &tile in &self.fault {
                if rng.prob(10.0) {
                    heavy.push(tile);
                } else {
                    devastate.push(tile);
                }
            }
            for &tile in &self.edges {
                if rng.prob(25.0) {
                    edge_heavy.push(tile);
                } else {
                    edge_light.push(tile);
                }
            }
        }
        heavy.extend(edge_heavy);
        queue_turfs(world, &heavy, Severity::Heavy);
        queue_turfs(world, &devastate, Severity::Devastate);
        queue_turfs(world, &edge_light, Severity::Light);
    }
}

/// The clients of the players on station levels.
fn station_clients(world: &mut World) -> Vec<Entity> {
    let levels = station_levels(world);
    let mut q = world.query::<(&PlayerMob, &TilePos)>();
    let mut clients: Vec<Entity> = q
        .iter(world)
        .filter(|(_, p)| levels.contains(&p.z))
        .map(|(m, _)| m.client)
        .collect();
    clients.sort();
    clients
}

/// The living mobs on `pos`.
fn living_on(world: &World, pos: TilePos) -> Vec<Entity> {
    world
        .resource::<ss13_world::SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .filter(|&e| {
            world.get::<crate::health::Health>(e).is_some()
                && world.get::<crate::health::Stat>(e).is_some()
        })
        .collect()
}

/// `shake_camera(mob, duration, strength)`; `duration_ds` in deciseconds.
fn shake_camera(world: &mut World, client: Entity, duration_ds: f32, strength: f32) {
    if world.contains_resource::<Messages<SendCameraEffect>>() {
        world.write_message(SendCameraEffect {
            client,
            effect: CameraEffect::Shake {
                duration_ds,
                strength,
            },
        });
    }
}

/// `playsound_local(mob, file, volume)`: at the listener.
fn play_local(world: &mut World, client: Entity, file: &str, volume: f32) {
    if world.contains_resource::<Messages<SendSound>>() {
        world.write_message(SendSound {
            client,
            sound: SoundEvent {
                file: file.to_owned(),
                volume,
                speed: 1.0,
                offset: Some([0, 0]),
            },
        });
    }
}

fn tell(world: &mut World, mob: Entity, text: &str) {
    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
        crate::tools::tell(world, client, Span::Warning, text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fault_follows_the_line_between_the_fracture_points() {
        let epicenter = TilePos::new(50, 50, 1);
        let (fault, edges) =
            fault_line(epicenter, TilePos::new(58, 56, 1), TilePos::new(42, 44, 1));
        assert!(fault.contains(&epicenter));
        assert!(fault.contains(&TilePos::new(58, 56, 1)));
        assert!(fault.contains(&TilePos::new(42, 44, 1)));
        // The far corners of the rectangle are off the line.
        assert!(!fault.contains(&TilePos::new(58, 44, 1)));
        assert!(!edges.contains(&TilePos::new(58, 44, 1)));
        for tile in &edges {
            assert!(!fault.contains(tile));
        }
        assert!(!edges.is_empty());
        assert!(fault.len() < 17 * 13);
    }

    #[test]
    fn the_end_bombs_the_fault_and_its_edges() {
        let mut world = World::new();
        world.insert_resource(EventRng(ss13_mapload::spawner::Rng::new(5)));
        world.init_resource::<crate::explosions::ExplosionQueue>();
        world.init_resource::<Messages<crate::sound::PlaySound>>();
        let fault: Vec<TilePos> = (1..=20).map(|x| TilePos::new(x, 5, 1)).collect();
        let edges: Vec<TilePos> = (1..=20).map(|x| TilePos::new(x, 8, 1)).collect();
        let mut quake = Earthquake {
            epicenter: Some(TilePos::new(10, 5, 1)),
            fault: fault.clone(),
            underbelly: Vec::new(),
            edges: edges.clone(),
        };
        quake.end(&mut world, &mut EventState::new(0));
        let queue = world.resource::<crate::explosions::ExplosionQueue>();
        for tile in fault {
            assert!(matches!(
                queue.queued(tile),
                Some(Severity::Devastate | Severity::Heavy)
            ));
        }
        for tile in edges {
            assert!(matches!(
                queue.queued(tile),
                Some(Severity::Light | Severity::Heavy)
            ));
        }
    }
}
