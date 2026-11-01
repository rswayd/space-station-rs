//! Carp Migration (`code/modules/events/carp_migration.dm`): a carp on every
//! `/obj/effect/landmark/carpspawn` (one in twenty a megacarp), each level's school sent along
//! the same path: a safe station tile, then the level's edge beyond it (tg
//! `pick_carp_migration_points`, `migrate_to`).

use bevy::prelude::*;
use ss13_shared::{Dir, TilePos};

use super::controller::{EventRng, EventState, RoundEvent};
use crate::mobs::ai::{AiController, Key, RUN_WHILE_UNWATCHED, Value};

pub const NAME: &str = "Carp Migration";
const CARP: &str = "/mob/living/basic/carp";
const MEGACARP: &str = "/mob/living/basic/carp/mega";

pub fn make(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(CarpMigration)
}

struct CarpMigration;

impl RoundEvent for CarpMigration {
    fn setup(&mut self, world: &mut World, ev: &mut EventState) {
        ev.announce_when = 3;
        ev.start_when = world.resource_mut::<EventRng>().0.range(40, 60) as u32;
    }

    fn announce(&mut self, world: &mut World, _: &mut EventState, _: bool) {
        let text = format!(
            "Неизвестные биологические объекты были обнаружены вблизи {}, будьте наготове.",
            super::station_name(world)
        );
        super::priority_announce(world, "Неопознанные формы жизни", &text);
    }

    fn start(&mut self, world: &mut World, _: &mut EventState) {
        spawn_school(world);
    }
}

/// tg `/datum/round_event/carp_migration/start`. Returns the carp.
pub fn spawn_school(world: &mut World) -> Vec<Entity> {
    let spawns = world.resource::<crate::server::SpawnPoints>().carp.clone();
    let mut paths: Vec<(u16, Vec<TilePos>)> = Vec::new();
    let mut fish = Vec::new();
    for pos in spawns {
        let mega = !world.resource_mut::<EventRng>().0.prob(95.0);
        let Some(carp) =
            crate::mobs::spawn::spawn_mob(world, if mega { MEGACARP } else { CARP }, pos)
        else {
            continue;
        };
        fish.push(carp);
        let path = match paths.iter().find(|(z, _)| *z == pos.z) {
            Some((_, path)) => path.clone(),
            None => {
                let path = migration_points(world, pos.z);
                paths.push((pos.z, path.clone()));
                path
            }
        };
        if path.is_empty() {
            continue;
        }
        if let Some(mut controller) = world.get_mut::<AiController>(carp) {
            // tg `migrate_to`: `RUN_WHILE_UNWATCHED`, then the path.
            controller.traits |= RUN_WHILE_UNWATCHED;
            controller.bb.set(Key::MigrationPath, Value::Tiles(path));
        }
    }
    fish
}

/// tg `pick_carp_migration_points`: a safe station tile and the level's edge from it in a
/// random direction.
// shortcut: the safe station tile is a random crew spawn point (tg
// `get_safe_random_station_turf` samples station areas for breathable, unblocked floors).
fn migration_points(world: &mut World, z: u16) -> Vec<TilePos> {
    let candidates: Vec<TilePos> = world
        .resource::<crate::server::SpawnPoints>()
        .tiles
        .iter()
        .copied()
        .filter(|t| t.z == z)
        .collect();
    let Some(&station) = world.resource_mut::<EventRng>().pick(&candidates) else {
        return Vec::new();
    };
    let dir = *world
        .resource_mut::<EventRng>()
        .pick(&Dir::DMI_ORDER)
        .expect("eight directions");
    let index = world.resource::<ss13_world::SpatialIndex>();
    let mut edge = station;
    while index.in_bounds(edge.step(dir)) {
        edge = edge.step(dir);
    }
    vec![station, edge]
}
