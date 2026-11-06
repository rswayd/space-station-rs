//! The mining level's fauna at round start, after tg `/datum/biome/proc/populate_turfs`
//! (`code/datums/mapgen/biomes/_biome.dm`) with `/datum/biome/lavaland/basalt`: each generated
//! open turf of an area with `MOB_SPAWN_ALLOWED` (`/area/lavaland/surface/outdoors/unexplored`
//! and its `danger` part) gets a mob at `fauna_density` 6%, picked by weight from
//! `fauna_types`, unless another mob is within `mob_exclusion_radius` (9). The random spawners
//! pick their rare kinds (`/obj/effect/spawner/random/lavaland_mob/*`).
// shortcut: one biome (the caves of `ss13_mapload::cave` are all basalt, no shale or red rock);
// the megafauna, tendril and raptor picks spawn nothing here (megafauna and tendrils belong to
// their own ports, raptors are not ported), so their share of the density stays empty; no flora
// and feature rolls before the mob roll (`flora_density`, `feature_density`), no
// `mining_mob_respawner`.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::spawner::Rng;
use ss13_mapload::{Areas, DmPower, MapSeed};
use ss13_shared::TilePos;
use ss13_world::SpatialIndex;

/// `area_flags_mapping & MOB_SPAWN_ALLOWED` on the lavaland surface.
const SPAWN_AREA: &str = "/area/lavaland/surface/outdoors/unexplored";
/// The open turfs the cave generator makes (`open_turf_type` of the lavaland biomes).
const OPEN_TURF: &str = "/turf/open/misc/asteroid";
/// `fauna_density`, `mob_exclusion_radius` of `/datum/biome/lavaland`.
pub const FAUNA_DENSITY: f64 = 6.0;
pub const MOB_EXCLUSION_RADIUS: u32 = 9;

/// A `fauna_types` entry: a mob, a random spawner's weighted kinds, or nothing here.
#[derive(Clone, Copy, Debug)]
pub enum Fauna {
    Mob(&'static str),
    Spawner(&'static [(&'static str, u32)]),
    /// `SPAWN_MEGAFAUNA`, tendrils and raptors: not spawned by this port.
    Elsewhere,
}

/// `/obj/effect/spawner/random/lavaland_mob/goliath`.
const GOLIATHS: &[(&str, u32)] = &[
    ("/mob/living/basic/mining/goliath", 99),
    ("/mob/living/basic/mining/goliath/ancient/immortal", 1),
];
/// `/obj/effect/spawner/random/lavaland_mob/legion`.
const LEGIONS: &[(&str, u32)] = &[
    ("/mob/living/basic/mining/legion", 19),
    ("/mob/living/basic/mining/legion/dwarf", 1),
];
/// `/obj/effect/spawner/random/lavaland_mob/watcher`.
const WATCHERS: &[(&str, u32)] = &[
    ("/mob/living/basic/mining/watcher", 80),
    ("/mob/living/basic/mining/watcher/magmawing", 15),
    ("/mob/living/basic/mining/watcher/icewing", 5),
];

/// `/datum/biome/lavaland/basalt` `fauna_types`.
pub const BASALT_FAUNA: [(Fauna, u32); 9] = [
    (Fauna::Elsewhere, 2),
    (Fauna::Spawner(GOLIATHS), 50),
    (Fauna::Spawner(LEGIONS), 40),
    (Fauna::Spawner(WATCHERS), 20),
    (Fauna::Mob("/mob/living/basic/mining/bileworm"), 10),
    (Fauna::Mob("/mob/living/basic/mining/lobstrosity/lava"), 20),
    (Fauna::Elsewhere, 15),
    (Fauna::Mob("/mob/living/basic/mining/goldgrub"), 15),
    (Fauna::Elsewhere, 7),
];

fn pick_weight<T: Copy>(table: &[(T, u32)], rng: &mut Rng) -> T {
    let total: u32 = table.iter().map(|e| e.1).sum();
    let mut roll = rng.range(1, i64::from(total)) as u32;
    for &(item, weight) in table {
        if roll <= weight {
            return item;
        }
        roll -= weight;
    }
    table[table.len() - 1].0
}

/// `populate_turfs` over `tiles` (in map order): where each mob goes.
pub fn plan(tiles: &[TilePos], rng: &mut Rng) -> Vec<(TilePos, &'static str)> {
    let mut spawned: Vec<(TilePos, &'static str)> = Vec::new();
    for &tile in tiles {
        // `prob(fauna_density)`.
        if (rng.next_u64() % 10_000) as f64 / 100.0 >= FAUNA_DENSITY {
            continue;
        }
        let path = match pick_weight(&BASALT_FAUNA, rng) {
            Fauna::Mob(path) => path,
            Fauna::Spawner(kinds) => pick_weight(kinds, rng),
            Fauna::Elsewhere => continue,
        };
        let crowded = spawned
            .iter()
            .any(|(p, _)| p.chebyshev(tile).is_some_and(|d| d <= MOB_EXCLUSION_RADIUS));
        if !crowded {
            spawned.push((tile, path));
        }
    }
    spawned
}

/// The generated open turfs of the spawn areas on level `z`, in map order (rows bottom up).
fn spawn_tiles(world: &World, z: u16) -> Vec<TilePos> {
    let areas = world.resource::<Areas>();
    let allowed: Vec<Entity> = areas
        .entities()
        .iter()
        .copied()
        .filter(|&a| {
            world
                .get::<DmPower>(a)
                .is_some_and(|p| is_under(&p.path, SPAWN_AREA))
        })
        .collect();
    if allowed.is_empty() {
        return Vec::new();
    }
    let index = world.resource::<SpatialIndex>();
    let Some((w, h)) = index.level_size(z) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for y in 1..=h as i32 {
        for x in 1..=w as i32 {
            let pos = TilePos::new(x, y, z);
            if !areas.area_at(pos).is_some_and(|a| allowed.contains(&a))
                || !index.dense_on_tile(pos).is_empty()
            {
                continue;
            }
            let open = crate::construction::turf_at(world, pos)
                .and_then(|t| crate::construction::path_of(world, t))
                .is_some_and(|p| is_under(&p, OPEN_TURF));
            if open {
                out.push(pos);
            }
        }
    }
    out
}

/// Round start: the fauna of the mining level ([`ss13_mapload::mining_z`]).
// Cost: once at startup, one pass over the level's tiles (65k on Lavaland) and an exclusion
// check per roll against the mobs placed so far (about a hundred).
pub(super) fn populate(world: &mut World) {
    let Some(z) = ss13_mapload::mining_z(world) else {
        return;
    };
    let seed = world.get_resource::<MapSeed>().map_or(0, |s| s.0);
    let tiles = spawn_tiles(world, z);
    let mut rng = Rng::new(seed ^ 0x4641_554e_4121);
    let picks = plan(&tiles, &mut rng);
    let mut spawned = 0;
    for (pos, path) in picks {
        if crate::mobs::spawn::spawn_mob(world, path, pos).is_some() {
            spawned += 1;
        }
    }
    info!(
        "lavaland: {spawned} fauna on z{z} ({} open tiles)",
        tiles.len()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fauna_keeps_its_distance_and_density() {
        let tiles: Vec<TilePos> = (1..=200)
            .flat_map(|y| (1..=200).map(move |x| TilePos::new(x, y, 3)))
            .collect();
        let picks = plan(&tiles, &mut Rng::new(42));
        assert_eq!(picks, plan(&tiles, &mut Rng::new(42)), "seeded");
        assert!(picks.len() > 50, "{}", picks.len());
        for (i, (a, _)) in picks.iter().enumerate() {
            for (b, _) in &picks[i + 1..] {
                assert!(a.chebyshev(*b).unwrap() > MOB_EXCLUSION_RADIUS);
            }
        }
        // Goliaths and legions are the most common.
        let goliaths = picks
            .iter()
            .filter(|(_, p)| p.starts_with("/mob/living/basic/mining/goliath"))
            .count();
        assert!(goliaths * 5 > picks.len(), "{goliaths} of {}", picks.len());
        assert!(
            picks
                .iter()
                .all(|(_, p)| p.starts_with("/mob/living/basic/mining/"))
        );
    }

    #[test]
    fn spawners_pick_their_kinds_by_weight() {
        let mut rng = Rng::new(1);
        let mut immortal = 0;
        for _ in 0..1000 {
            if pick_weight(GOLIATHS, &mut rng).ends_with("immortal") {
                immortal += 1;
            }
        }
        assert!(immortal < 40, "{immortal}");
    }
}
