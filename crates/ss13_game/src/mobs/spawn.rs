//! Spawning basic mobs: [`spawn_mob`] (tg `new path(loc)`) and the round-start vermin of tg
//! `SSminor_mapping.trigger_migration` (`mice_roundstart`, 10): mice on exposed station wires
//! whose air a mouse survives.

use bevy::prelude::*;
use ss13_atmos::{Atmos, GasId};
use ss13_mapload::{DmPower, HiddenUnderTile, MapSeed};
use ss13_shared::TilePos;
use ss13_world::SpatialIndex;

/// tg `mice_roundstart` default.
pub const MICE_ROUNDSTART: usize = 10;
/// `NPC_DEFAULT_MIN_TEMP`, `NPC_DEFAULT_MAX_TEMP`.
const NPC_MIN_TEMP: f64 = 250.0;
const NPC_MAX_TEMP: f64 = 350.0;
const CABLE_ROOT: &str = "/obj/structure/cable";
const MULTIZ_CABLE: &str = "/obj/structure/cable/multilayer/multiz";
const MOUSE: &str = "/mob/living/basic/mouse";

/// A `path` basic mob on `pos`, initialized like a mapped one (`crate::mobs` attaches its
/// definition), replicated. `None` without its prototype.
pub fn spawn_mob(world: &mut World, path: &str, pos: TilePos) -> Option<Entity> {
    let mob = crate::construction::spawn_obj(world, path, pos)?;
    world.flush();
    Some(mob)
}

/// tg `find_exposed_wires`: tiles of station levels with a cable not under a floor tile and
/// nothing dense, in a seeded shuffle.
// shortcut: "plating" is a visible cable's tile (turf types are not kept on entities); a cable
// on a catwalk counts too.
pub fn exposed_wires(world: &mut World, seed: u64) -> Vec<TilePos> {
    let levels = crate::events::controller::station_levels(world);
    let mut q = world.query_filtered::<(&DmPower, &TilePos), Without<HiddenUnderTile>>();
    let mut tiles: Vec<TilePos> = q
        .iter(world)
        .filter(|(p, pos)| {
            levels.contains(&pos.z)
                && ss13_mapload::fidelity::is_under(&p.path, CABLE_ROOT)
                && !ss13_mapload::fidelity::is_under(&p.path, MULTIZ_CABLE)
        })
        .map(|(_, &pos)| pos)
        .collect();
    tiles.sort_by_key(|p| (p.z, p.y, p.x));
    tiles.dedup();
    let index = world.resource::<SpatialIndex>();
    tiles.retain(|&p| index.dense_on_tile(p).is_empty());
    let mut rng = ss13_mapload::spawner::Rng::new(seed ^ 0x6d69_6365);
    for i in (1..tiles.len()).rev() {
        let j = rng.range(0, i as i64) as usize;
        tiles.swap(i, j);
    }
    tiles
}

/// tg `valid_mouse_turf`: at least 5 moles of oxygen, between 250 and 350 K.
pub fn valid_mouse_turf(world: &World, pos: TilePos) -> bool {
    let Some(air) = world.get_resource::<Atmos>().and_then(|a| a.air(pos)) else {
        return false;
    };
    air.get(GasId::O2) >= 5.0 && air.temperature < NPC_MAX_TEMP && air.temperature > NPC_MIN_TEMP
}

/// tg `trigger_migration(CONFIG_GET(number/mice_roundstart))` at round start.
// shortcut: only mice from `vermin_chances` (snails, stoats and regal rats are not ported).
pub(super) fn roundstart_vermin(world: &mut World) {
    if world
        .get_resource::<ss13_mapload::MapLoadPlugin>()
        .is_none()
        || world.get_resource::<Atmos>().is_none()
    {
        return;
    }
    let seed = world.get_resource::<MapSeed>().map_or(0, |s| s.0);
    let mut spawned = 0;
    for pos in exposed_wires(world, seed) {
        if spawned >= MICE_ROUNDSTART {
            break;
        }
        if !valid_mouse_turf(world, pos) {
            continue;
        }
        if spawn_mob(world, MOUSE, pos).is_some() {
            spawned += 1;
        }
    }
    if spawned > 0 {
        info!("mobs: {spawned} mice on exposed wires");
    }
}
