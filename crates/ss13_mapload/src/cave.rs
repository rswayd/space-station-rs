//! Cave terrain of the mining level, after tg `/datum/map_generator/cave_generator`
//! (`code/datums/mapgen/CaveGenerator.dm`) with the lavaland turfs
//! (`/datum/map_generator/cave_generator/lavaland`): the map's `/turf/open/genturf`
//! placeholders become open basalt or volcanic rock by a cellular automaton, before the map
//! spawns (so no turf is spawned twice).
// shortcut: BandaStation generates with rust-g's `cave_system_generator` (BSP rooms, corridors,
// loops, ruin footprints) and biomes (shale, red rock); here a plain automaton with its
// `noise_percent`, `ca_steps`, `birth_limit`, `survival_limit` and `edges_are_alive` carves the
// caves, with one open and one closed turf. No lava rivers (`spawn_rivers`), ruins, flora,
// features (geysers, ore vents) or mobs (`populate_terrain`).

use std::collections::HashMap;

use ss13_assets::dmm::{DmmMap, DmmObject, NO_PREFAB};

use crate::fidelity::is_under;
use crate::spawner::Rng;

/// tg `/turf/open/genturf`.
pub const GENTURF: &str = "/turf/open/genturf";
/// `/datum/map_generator/cave_generator/lavaland` `weighted_open_turf_types`.
pub const LAVALAND_OPEN: &str = "/turf/open/misc/asteroid/basalt/lava_land_surface";
/// `/datum/map_generator/cave_generator/lavaland` `weighted_closed_turf_types`.
pub const LAVALAND_CLOSED: &str = "/turf/closed/mineral/random/volcanic";

/// `/datum/map_generator/cave_generator` vars.
const NOISE_PERCENT: u64 = 51;
const CA_STEPS: usize = 8;
const BIRTH_LIMIT: usize = 6;
const SURVIVAL_LIMIT: usize = 4;

/// Closed (alive) cells of a `w` x `h` automaton: `noise_percent` closed at random, then
/// `ca_steps` generations (a closed cell survives with `survival_limit` closed neighbours, an
/// open one closes with `birth_limit`); off-grid neighbours count as closed.
pub fn automaton(w: usize, h: usize, rng: &mut Rng) -> Vec<bool> {
    let mut cells: Vec<bool> = (0..w * h)
        .map(|_| rng.next_u64() % 100 < NOISE_PERCENT)
        .collect();
    let mut next = cells.clone();
    for _ in 0..CA_STEPS {
        for y in 0..h {
            for x in 0..w {
                let mut closed = 0;
                for dy in -1i64..=1 {
                    for dx in -1i64..=1 {
                        if dx == 0 && dy == 0 {
                            continue;
                        }
                        let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                        let off = nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64;
                        if off || cells[ny as usize * w + nx as usize] {
                            closed += 1;
                        }
                    }
                }
                let i = y * w + x;
                next[i] = if cells[i] {
                    closed >= SURVIVAL_LIMIT
                } else {
                    closed >= BIRTH_LIMIT
                };
            }
        }
        std::mem::swap(&mut cells, &mut next);
    }
    cells
}

/// Replaces every `genturf` tile of `map` with `open` or `closed` by [`automaton`] (one per
/// z-level, from `seed`). Returns the number of tiles generated.
pub fn generate(map: &mut DmmMap, seed: u64, open: &str, closed: &str) -> usize {
    let (sx, sy, sz) = map.size;
    let (w, h) = (sx as usize, sy as usize);
    let gen_prefab: Vec<bool> = map
        .prefabs
        .iter()
        .map(|p| p.objects.iter().any(|o| is_under(&o.path, GENTURF)))
        .collect();
    if !gen_prefab.contains(&true) {
        return 0;
    }
    // (genturf prefab, closed) -> the prefab with the generated turf in its place.
    let mut variants: HashMap<(u32, bool), u32> = HashMap::new();
    let mut rng = Rng::new(seed ^ 0x4341_5645);
    let mut generated = 0;
    for z in 0..sz as usize {
        let cells = automaton(w, h, &mut rng);
        for (i, &is_closed) in cells.iter().enumerate() {
            let at = z * w * h + i;
            let p = map.grid[at];
            if p == NO_PREFAB || !gen_prefab[p as usize] {
                continue;
            }
            let variant = *variants.entry((p, is_closed)).or_insert_with(|| {
                let mut prefab = map.prefabs[p as usize].clone();
                prefab.key = format!("{}#{}", prefab.key, u8::from(is_closed));
                for o in prefab
                    .objects
                    .iter_mut()
                    .filter(|o| is_under(&o.path, GENTURF))
                {
                    *o = DmmObject {
                        path: if is_closed { closed } else { open }.to_owned(),
                        vars: Vec::new(),
                    };
                }
                map.prefabs.push(prefab);
                (map.prefabs.len() - 1) as u32
            });
            map.grid[at] = variant;
            generated += 1;
        }
    }
    generated
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automaton_is_seeded_and_carves_caves() {
        let a = automaton(64, 48, &mut Rng::new(7));
        assert_eq!(a, automaton(64, 48, &mut Rng::new(7)));
        let closed = a.iter().filter(|&&c| c).count();
        // Neither solid rock nor an open plain.
        assert!(
            closed > 64 * 48 / 10 && closed < 64 * 48 * 9 / 10,
            "{closed}"
        );
        // The edges count as rock, so the border rows stay mostly closed.
        let edge = (0..64).filter(|&x| a[x]).count();
        assert!(edge > 32, "{edge}");
    }

    #[test]
    fn genturf_tiles_become_open_or_closed() {
        let text = "\
\"a\" = (/turf/open/genturf,/area/lavaland/surface/outdoors/unexplored)
\"b\" = (/turf/open/floor/plating,/area/mine/lounge)

(1,1,1) = {\"
aaaaaaaa
aaaaaaaa
aaaabbaa
aaaaaaaa
\"}
";
        let mut map = DmmMap::parse(text).unwrap();
        let n = generate(&mut map, 1, LAVALAND_OPEN, LAVALAND_CLOSED);
        assert_eq!(n, 30);
        for y in 1..=4 {
            for x in 1..=8 {
                let turf = &map.get(x, y, 1).unwrap().objects[0].path;
                assert_ne!(turf, GENTURF);
                if (y, x) == (2, 5) || (y, x) == (2, 6) {
                    assert_eq!(turf, "/turf/open/floor/plating");
                } else {
                    assert!(turf == LAVALAND_OPEN || turf == LAVALAND_CLOSED, "{turf}");
                }
                let area = &map.get(x, y, 1).unwrap().objects[1].path;
                assert!(area.starts_with("/area/"));
            }
        }
        // At most two variants of the one genturf prefab.
        assert!(map.prefabs.len() <= 4);
    }
}
