//! The megafauna of the mining level, after tg `/datum/biome/populate_turfs`
//! (`code/datums/mapgen/biomes/_biome.dm`) with the lavaland basalt biome
//! (`biomes/lavaland.dm`) and `GLOB.megafauna_spawn_list`: on every open basalt turf of the
//! megafauna areas (`/area/lavaland/surface/outdoors/unexplored/danger`,
//! `MEGAFAUNA_SPAWN_ALLOWED`), in map order, flora takes 2.5% and features 0.25% first, then
//! 6% spawn fauna, of which `SPAWN_MEGAFAUNA` is 2 in 179 (`fauna_types`); a boss is then
//! picked by weight (bubblegum 6, colossus 2, drake 4) and spawns unless another boss is within
//! 7 tiles (`megafauna_exclusion_radius`). Bubblegum is unique: its first pick takes it off the
//! list.
// shortcut: bubblegum is not ported: its pick spawns nothing (the colossus' and drake's odds
// stay tg's). One biome (basalt) for the whole level, as the cave generator makes one open turf;
// the rolls are this module's own, not shared with the flora, features and fauna of the
// basic mobs' population, whose exclusion radii megafauna ignore in tg too.

use bevy::prelude::*;
use ss13_mapload::construct::DmTurfBuild;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::spawner::Rng;
use ss13_mapload::{Areas, DmPower, MapSeed, Turf};
use ss13_shared::TilePos;

use super::{COLOSSUS, DRAKE};

/// tg `/turf/open/misc/asteroid/basalt/lava_land_surface` (the biome's `open_turf_type`).
pub const OPEN_TURF: &str = ss13_mapload::cave::LAVALAND_OPEN;
/// The lavaland area with `MEGAFAUNA_SPAWN_ALLOWED`.
pub const DANGER_AREA: &str = "/area/lavaland/surface/outdoors/unexplored/danger";
const BUBBLEGUM: &str = "/mob/living/simple_animal/hostile/megafauna/bubblegum";
/// `/datum/biome/lavaland/basalt`: `flora_density`, `feature_density`, `fauna_density`
/// (percent), the `SPAWN_MEGAFAUNA` weight and the sum of `fauna_types` weights.
const FLORA_DENSITY: f32 = 2.5;
const FEATURE_DENSITY: f32 = 0.25;
const FAUNA_DENSITY: f32 = 6.0;
const MEGAFAUNA_WEIGHT: i64 = 2;
const FAUNA_WEIGHTS: i64 = 179;
/// `megafauna_exclusion_radius`.
const EXCLUSION: u32 = 7;
/// `GLOB.megafauna_spawn_list`.
const SPAWN_LIST: [(&str, i64); 3] = [(BUBBLEGUM, 6), (COLOSSUS, 2), (DRAKE, 4)];

/// The bosses for the candidate `tiles` (in map order).
pub fn plan(tiles: &[TilePos], rng: &mut Rng) -> Vec<(TilePos, &'static str)> {
    let mut list: Vec<(&'static str, i64)> = SPAWN_LIST.to_vec();
    let mut placed: Vec<TilePos> = Vec::new();
    let mut out = Vec::new();
    for &tile in tiles {
        if rng.prob(FLORA_DENSITY) || rng.prob(FEATURE_DENSITY) || !rng.prob(FAUNA_DENSITY) {
            continue;
        }
        if rng.range(1, FAUNA_WEIGHTS) > MEGAFAUNA_WEIGHT {
            continue;
        }
        let total: i64 = list.iter().map(|e| e.1).sum();
        if total == 0 {
            continue;
        }
        // `pick_weight`.
        let mut roll = rng.range(1, total);
        let Some(&(path, _)) = list.iter().find(|e| {
            if roll <= e.1 {
                return true;
            }
            roll -= e.1;
            false
        }) else {
            continue;
        };
        if placed
            .iter()
            .any(|p| p.chebyshev(tile).is_some_and(|d| d <= EXCLUSION))
        {
            continue;
        }
        if path == BUBBLEGUM {
            list.retain(|e| e.0 != BUBBLEGUM);
            continue;
        }
        placed.push(tile);
        out.push((tile, path));
    }
    out
}

/// After the map load: the bosses on the mining level, if one loaded.
// Cost: once at startup, one pass over the turfs (65k on a 255x255 level).
pub(super) fn place(world: &mut World) {
    let Some(z) = ss13_mapload::mining_z(world) else {
        return;
    };
    let seed = world.get_resource::<MapSeed>().map_or(0, |s| s.0);
    let open: Vec<TilePos> = world
        .query_filtered::<(&TilePos, &DmTurfBuild), With<Turf>>()
        .iter(world)
        .filter(|(p, t)| p.z == z && is_under(&t.path, OPEN_TURF))
        .map(|(p, _)| *p)
        .collect();
    let areas = world.get_resource::<Areas>();
    let mut tiles: Vec<TilePos> = open
        .into_iter()
        .filter(|&pos| {
            areas
                .and_then(|a| a.area_at(pos))
                .and_then(|area| world.get::<DmPower>(area))
                .is_some_and(|p| is_under(&p.path, DANGER_AREA))
        })
        .collect();
    // `block()` order: x fastest, then y.
    tiles.sort_by_key(|p| (p.y, p.x));
    let mut rng = Rng::new(seed ^ 0x4D45_4741);
    let bosses = plan(&tiles, &mut rng);
    for &(pos, path) in &bosses {
        super::super::spawn::spawn_mob(world, path, pos);
    }
    info!("mining level z{z}: {} megafauna placed", bosses.len());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bosses_are_rare_apart_and_never_bubblegum() {
        let tiles: Vec<TilePos> = (1..=200)
            .flat_map(|y| (1..=200).map(move |x| TilePos::new(x, y, 3)))
            .collect();
        let mut rng = Rng::new(7);
        let bosses = plan(&tiles, &mut rng);
        // 40000 tiles x ~0.065%: tens of picks, a third of them the unique bubblegum's.
        assert!((3..40).contains(&bosses.len()), "{} bosses", bosses.len());
        assert!(bosses.iter().all(|(_, p)| *p == COLOSSUS || *p == DRAKE));
        for (i, (a, _)) in bosses.iter().enumerate() {
            for (b, _) in &bosses[i + 1..] {
                assert!(a.chebyshev(*b).unwrap() > EXCLUSION, "{a:?} {b:?}");
            }
        }
        // Seeded: the same plan twice.
        assert_eq!(plan(&tiles, &mut Rng::new(7)), bosses);
        assert!(plan(&[], &mut Rng::new(7)).is_empty());
    }
}
