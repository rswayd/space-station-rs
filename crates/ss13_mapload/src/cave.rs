//! Cave terrain of the mining level, after tg `/datum/map_generator/cave_generator`
//! (`code/datums/mapgen/CaveGenerator.dm`) with the lavaland turfs
//! (`/datum/map_generator/cave_generator/lavaland`): the map's `/turf/open/genturf`
//! placeholders become open basalt or volcanic rock by a cellular automaton, before the map
//! spawns (so no turf is spawned twice).
//!
//! Lava rivers ([`spawn_rivers`], tg `setup_rivers` -> `spawn_rivers` in
//! `code/game/turfs/open/river.dm`) then snake between waypoints through the unexplored wastes,
//! widening into the rock (`Spread`); ruins are pasted after them ([`crate::ruins`]).
// shortcut: BandaStation generates with rust-g's `cave_system_generator` (BSP rooms, corridors,
// loops, ruin footprints) and biomes (shale, red rock); here a plain automaton with its
// `noise_percent`, `ca_steps`, `birth_limit`, `survival_limit` and `edges_are_alive` carves the
// caves, with one open and one closed turf. No flora, features (geysers, ore vents) or mobs
// (`populate_terrain`).

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

/// `/turf/open/lava/smooth/lava_land_surface`: lavaland's rivers.
pub const LAVALAND_RIVER: &str = "/turf/open/lava/smooth/lava_land_surface";
/// `spawn_rivers` whitelist on lavaland.
pub const RIVER_AREA: &str = "/area/lavaland/surface/outdoors/unexplored";
/// `setup_rivers`: three nodes on lavaland, "+1 from the mapped in waypoint".
pub const LAVALAND_RIVER_NODES: usize = 3;
const RIVER_WAYPOINT: &str = "/obj/effect/landmark/river_waypoint";
/// `RANDOM_LOWER_X`, `RANDOM_LOWER_Y`, `RANDOM_UPPER_X`, `RANDOM_UPPER_Y`.
const RIVER_BOX: (u32, u32, u32, u32) = (50, 50, 200, 200);
const MINERAL: &str = "/turf/closed/mineral";
const CLOSED: &str = "/turf/closed";
const INDESTRUCTIBLE_FLOOR: &str = "/turf/open/indestructible";
/// Steps of one river before it gives up (tg walks until it arrives).
const MAX_RIVER_STEPS: usize = 4096;
/// `GLOB.alldirs` counterclockwise from east: BYOND `turn(dir, 45)` is the next one.
const DIRS: [(i64, i64); 8] = [
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
    (0, -1),
    (1, -1),
];

/// Turf changes on a parsed map before it spawns: a changed cell gets a copy of its prefab with
/// the new turf (one copy per prefab and turf), like `ChangeTurf(..., CHANGETURF_SKIP)` at
/// mapload.
pub struct TurfEdits<'a> {
    pub map: &'a mut DmmMap,
    variants: HashMap<(u32, String), u32>,
}

impl<'a> TurfEdits<'a> {
    pub fn new(map: &'a mut DmmMap) -> Self {
        Self {
            map,
            variants: HashMap::new(),
        }
    }

    /// Grid index of 1-based `(x, y, z)`.
    pub fn cell(&self, x: i64, y: i64, z: u32) -> Option<usize> {
        let (sx, sy, sz) = self.map.size;
        (x >= 1 && y >= 1 && z >= 1 && x <= i64::from(sx) && y <= i64::from(sy) && z <= sz).then(
            || ((z as usize - 1) * sy as usize + (y as usize - 1)) * sx as usize + (x as usize - 1),
        )
    }

    fn object(&self, at: usize, root: &str) -> Option<&str> {
        let p = self.map.grid[at];
        if p == NO_PREFAB {
            return None;
        }
        self.map.prefabs[p as usize]
            .objects
            .iter()
            .rev()
            .find(|o| is_under(&o.path, root))
            .map(|o| o.path.as_str())
    }

    pub fn turf(&self, at: usize) -> Option<&str> {
        self.object(at, "/turf")
    }

    pub fn area(&self, at: usize) -> Option<&str> {
        self.object(at, "/area")
    }

    /// The cell's turf becomes a plain `path` turf (its var edits go with the old one).
    pub fn set_turf(&mut self, at: usize, path: &str) {
        let p = self.map.grid[at];
        if p == NO_PREFAB || self.turf(at) == Some(path) {
            return;
        }
        let map = &mut *self.map;
        let variant = *self
            .variants
            .entry((p, path.to_owned()))
            .or_insert_with(|| {
                let mut prefab = map.prefabs[p as usize].clone();
                prefab.key = format!("{}#{path}", prefab.key);
                if let Some(o) = prefab
                    .objects
                    .iter_mut()
                    .rev()
                    .find(|o| is_under(&o.path, "/turf"))
                {
                    *o = DmmObject {
                        path: path.to_owned(),
                        vars: Vec::new(),
                    };
                }
                map.prefabs.push(prefab);
                (map.prefabs.len() - 1) as u32
            });
        self.map.grid[at] = variant;
    }
}

/// BYOND `get_dir`.
fn dir_to(from: (i64, i64), to: (i64, i64)) -> usize {
    let d = ((to.0 - from.0).signum(), (to.1 - from.1).signum());
    DIRS.iter().position(|&x| x == d).unwrap_or(0)
}

/// tg `spawn_rivers(z, nodes, turf_type, whitelist_area)` on every z-level of `map`: `nodes`
/// waypoints on random whitelisted tiles of the middle box join the mapped ones, and from each
/// waypoint a river of `turf` walks towards another (straight, or detouring 45 degrees for a
/// while), skipping over tiles outside the whitelist, every step spreading (`Spread(25, 11)`).
/// The mapped waypoint landmarks are removed. Returns the number of river tiles.
pub fn spawn_rivers(map: &mut DmmMap, seed: u64, nodes: usize, turf: &str, area: &str) -> usize {
    let mut rng = Rng::new(seed ^ 0x0052_4956_4552);
    let (sx, sy, sz) = map.size;
    let mut changed = 0;
    // The mapped waypoints, then the landmarks come off their prefabs.
    let mut mapped: Vec<Vec<(i64, i64)>> = vec![Vec::new(); sz as usize];
    let has_waypoint: Vec<bool> = map
        .prefabs
        .iter()
        .map(|p| p.objects.iter().any(|o| o.path == RIVER_WAYPOINT))
        .collect();
    for z in 1..=sz {
        for y in 1..=sy {
            for x in 1..=sx {
                if map.index(x, y, z).is_some_and(|p| has_waypoint[p as usize]) {
                    mapped[z as usize - 1].push((i64::from(x), i64::from(y)));
                }
            }
        }
    }
    for p in &mut map.prefabs {
        p.objects.retain(|o| o.path != RIVER_WAYPOINT);
    }
    let mut edits = TurfEdits::new(map);
    for z in 1..=sz {
        let whitelisted = |edits: &TurfEdits, x: i64, y: i64| {
            edits
                .cell(x, y, z)
                .and_then(|at| edits.area(at))
                .is_some_and(|a| is_under(a, area))
        };
        let (x0, y0, x1, y1) = RIVER_BOX;
        // `CORNER_BLOCK(corner, width, height)`: `min` to `max - 1`.
        let candidates: Vec<(i64, i64)> = (y0..y1)
            .flat_map(|y| (x0..x1).map(move |x| (i64::from(x), i64::from(y))))
            .filter(|&(x, y)| whitelisted(&edits, x, y))
            .collect();
        let mut waypoints = std::mem::take(&mut mapped[z as usize - 1]);
        if candidates.is_empty() && waypoints.is_empty() {
            continue;
        }
        for _ in 0..nodes {
            if candidates.is_empty() {
                break;
            }
            waypoints.push(candidates[rng.range(0, candidates.len() as i64 - 1) as usize]);
        }
        let mut connected = vec![false; waypoints.len()];
        for (i, &start) in waypoints.iter().enumerate() {
            if connected[i] {
                continue;
            }
            connected[i] = true;
            let mut cur = start;
            if let Some(at) = edits.cell(cur.0, cur.1, z) {
                edits.set_turf(at, turf);
                changed += 1;
            }
            let others: Vec<usize> = (0..waypoints.len()).filter(|&j| j != i).collect();
            if others.is_empty() {
                break;
            }
            let target = waypoints[others[rng.range(0, others.len() as i64 - 1) as usize]];
            let mut detouring = false;
            let mut dir = dir_to(cur, target);
            let step = |p: (i64, i64), d: usize| {
                (
                    (p.0 + DIRS[d].0).clamp(1, i64::from(sx)),
                    (p.1 + DIRS[d].1).clamp(1, i64::from(sy)),
                )
            };
            let mut steps = 0;
            while cur != target && steps < MAX_RIVER_STEPS {
                steps += 1;
                if detouring {
                    if rng.prob(20.0) {
                        detouring = false;
                        dir = dir_to(cur, target);
                    }
                } else if rng.prob(20.0) {
                    detouring = true;
                    dir = if rng.prob(50.0) {
                        (dir + 1) % 8
                    } else {
                        (dir + 7) % 8
                    };
                } else {
                    dir = dir_to(cur, target);
                }
                cur = step(cur, dir);
                if !whitelisted(&edits, cur.0, cur.1) {
                    detouring = false;
                    dir = dir_to(cur, target);
                    cur = step(cur, dir);
                    continue;
                }
                if let Some(at) = edits.cell(cur.0, cur.1, z) {
                    edits.set_turf(at, turf);
                    changed += 1;
                    changed += spread(&mut edits, cur, z, 25.0, 11.0, turf, area, &mut rng);
                }
            }
        }
    }
    changed
}

/// tg `/turf/proc/Spread(probability, prob_loss, whitelisted_area)` of the river turf at `at`:
/// the neighbours that are not dense (rock is fine), indestructible or outside the whitelist and
/// not of the rock's floor type change: cardinal ones always (spreading on by `probability`),
/// diagonal ones by `probability` (spreading on), else diagonal rock is dug to its floor.
// shortcut: every rock's `turf_type` is the lavaland basalt ([`LAVALAND_OPEN`]).
#[allow(clippy::too_many_arguments)]
fn spread(
    edits: &mut TurfEdits,
    at: (i64, i64),
    z: u32,
    probability: f32,
    prob_loss: f32,
    turf: &str,
    area: &str,
    rng: &mut Rng,
) -> usize {
    if probability <= 0.0 {
        return 0;
    }
    let mut logged: Option<&str> = None;
    let mut cardinal = Vec::new();
    let mut diagonal = Vec::new();
    // `RANGE_TURFS(1, src) - src`: x fastest, from the bottom row.
    for dy in -1..=1 {
        for dx in -1..=1 {
            if dx == 0 && dy == 0 {
                continue;
            }
            let p = (at.0 + dx, at.1 + dy);
            let Some(cell) = edits.cell(p.0, p.1, z) else {
                continue;
            };
            let path = edits.turf(cell).unwrap_or("");
            let mineral = is_under(path, MINERAL);
            if (is_under(path, CLOSED) && !mineral) || is_under(path, INDESTRUCTIBLE_FLOOR) {
                continue;
            }
            if !edits.area(cell).is_some_and(|a| is_under(a, area)) {
                continue;
            }
            if logged.is_none() && mineral {
                logged = Some(LAVALAND_OPEN);
            }
            if dx == 0 || dy == 0 {
                cardinal.push((p, cell));
            } else {
                diagonal.push((p, cell));
            }
        }
    }
    let of_logged = |edits: &TurfEdits, cell: usize| {
        logged.is_some_and(|l| edits.turf(cell).is_some_and(|t| is_under(t, l)))
    };
    let mut changed = 0;
    for (p, cell) in cardinal {
        if of_logged(&*edits, cell) {
            continue;
        }
        edits.set_turf(cell, turf);
        changed += 1;
        if rng.prob(probability) {
            changed += spread(
                edits,
                p,
                z,
                probability - prob_loss,
                prob_loss,
                turf,
                area,
                rng,
            );
        }
    }
    for (p, cell) in diagonal {
        if !of_logged(&*edits, cell) && rng.prob(probability) {
            edits.set_turf(cell, turf);
            changed += 1;
            changed += spread(
                edits,
                p,
                z,
                probability - prob_loss,
                prob_loss,
                turf,
                area,
                rng,
            );
        } else if edits.turf(cell).is_some_and(|t| is_under(t, MINERAL)) {
            edits.set_turf(cell, LAVALAND_OPEN);
        }
    }
    changed
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

    fn wastes(w: usize, h: usize, rock_every: usize) -> DmmMap {
        let mut rows = Vec::new();
        for y in 0..h {
            let row: String = (0..w)
                .map(|x| if (x + y) % rock_every == 0 { 'r' } else { 'o' })
                .collect();
            rows.push(row);
        }
        let text = format!(
            "\"o\" = (/turf/open/misc/asteroid/basalt/lava_land_surface,{RIVER_AREA})\n\
             \"r\" = (/turf/closed/mineral/random/volcanic,{RIVER_AREA})\n\
             \"b\" = (/obj/effect/landmark/river_waypoint,/turf/open/misc/asteroid/basalt/lava_land_surface,{RIVER_AREA})\n\n\
             (1,1,1) = {{\"\n{}\n\"}}\n",
            rows.join("\n")
        );
        DmmMap::parse(&text).unwrap()
    }

    #[test]
    fn rivers_join_waypoints_through_the_wastes() {
        let mut map = wastes(250, 250, 3);
        let n = spawn_rivers(&mut map, 5, 3, LAVALAND_RIVER, RIVER_AREA);
        let lava = (1..=250u32)
            .flat_map(|y| (1..=250u32).map(move |x| (x, y)))
            .filter(|&(x, y)| {
                map.get(x, y, 1)
                    .unwrap()
                    .objects
                    .iter()
                    .any(|o| o.path == LAVALAND_RIVER)
            })
            .count();
        assert!(n >= lava && lava > 30, "{n} changes, {lava} lava tiles");
        // Seeded.
        let mut again = wastes(250, 250, 3);
        spawn_rivers(&mut again, 5, 3, LAVALAND_RIVER, RIVER_AREA);
        assert_eq!(map, again);
    }

    #[test]
    fn rivers_stay_out_of_other_areas() {
        let mut map = wastes(250, 250, 4);
        for p in &mut map.prefabs {
            p.objects.last_mut().unwrap().path = "/area/mine/production".into();
        }
        assert_eq!(spawn_rivers(&mut map, 1, 3, LAVALAND_RIVER, RIVER_AREA), 0);
    }

    #[test]
    fn the_mapped_waypoint_is_a_river_end_and_goes_away() {
        let text = format!(
            "\"o\" = (/turf/open/misc/asteroid/basalt/lava_land_surface,{RIVER_AREA})\n\
             \"b\" = (/obj/effect/landmark/river_waypoint,/turf/open/misc/asteroid/basalt/lava_land_surface,{RIVER_AREA})\n\n\
             (1,1,1) = {{\"\nboooo\nooooo\nooooo\nooooo\noooob\n\"}}\n"
        );
        let mut map = DmmMap::parse(&text).unwrap();
        // No room for random nodes in the middle box: the two mapped waypoints join.
        spawn_rivers(&mut map, 2, 3, LAVALAND_RIVER, RIVER_AREA);
        let turf = |x, y| {
            map.get(x, y, 1)
                .unwrap()
                .objects
                .iter()
                .find(|o| o.path.starts_with("/turf"))
                .map(|o| o.path.clone())
        };
        assert_eq!(turf(1, 5).as_deref(), Some(LAVALAND_RIVER));
        assert_eq!(turf(5, 1).as_deref(), Some(LAVALAND_RIVER));
        assert!(
            map.prefabs
                .iter()
                .all(|p| p.objects.iter().all(|o| o.path != RIVER_WAYPOINT))
        );
    }
}
