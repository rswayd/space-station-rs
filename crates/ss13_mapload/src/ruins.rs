//! Random ruins of the mining level, after tg `SSmapping.setup_ruins` -> `seedRuins`
//! (`code/modules/mapping/ruins.dm`) with the `/datum/map_template/ruin/lavaland` templates
//! (`code/datums/ruins/lavaland.dm`): templates are picked by weight within the
//! `lavaland_budget` (always-placed ones first) and pasted onto the parsed mining map before it
//! spawns, centred on a random tile whose whole footprint lies in
//! `/area/lavaland/surface/outdoors/unexplored` and no earlier ruin.
//!
//! - `/turf/template_noop` and `/area/template_noop` cells keep the cave's turf and area; the
//!   ruin's own movables join what is there, minus `clear_below`'s spawners, simple animals and
//!   flora.
//! - A template only places when the converted prototypes cover all its types (the loader fails
//!   on an unknown type): run `ss13-mapload convert` with `_maps/RandomRuins/LavaRuins`.
// shortcut: no ore vent ruins (`mineral_budget`, `/datum/map_template/ruin/lavaland/vent`: no
// ore vents), no AnywhereRuins (golem ship, fountain hall, shoe factory), no
// `lavaruinblacklist.txt` or `lavaland_budget` config (tg default 60), no themed ruins, no
// `always_spawn_with` / `never_spawn_with` (no lavaland template sets them), no `ruin` landmarks
// or `active_ruins`, no terrain generation inside ruin areas (`biome_replace` turfs stay basalt).

use std::collections::{HashMap, HashSet};
use std::path::Path;

use ss13_assets::dmm::{DmmMap, DmmObject, NO_PREFAB, Prefab};

use crate::fidelity::is_under;
use crate::spawner::Rng;

/// `/datum/map_template/ruin/lavaland` `prefix`.
pub const LAVA_RUINS_DIR: &str = "_maps/RandomRuins/LavaRuins";
/// `CONFIG_GET(number/lavaland_budget)` default.
pub const LAVALAND_BUDGET: i64 = 60;
/// `setup_ruins`: the lavaland `mineral_budget`.
const MINERAL_BUDGET: i64 = 15;
/// `seedRuins` whitelist on lavaland.
pub const RUIN_AREA: &str = "/area/lavaland/surface/outdoors/unexplored";
/// `PLACEMENT_TRIES`.
const PLACEMENT_TRIES: u32 = 100;
/// `TRANSITIONEDGE + SPACERUIN_MAP_EDGE_PAD`.
const EDGE: i64 = 8 + 15;
const TURF_NOOP: &str = "/turf/template_noop";
const AREA_NOOP: &str = "/area/template_noop";
/// `load_reserved`'s `clear_below_typecache`.
const CLEAR_BELOW: [&str; 3] = [
    "/obj/structure/spawner",
    "/mob/living/simple_animal",
    "/obj/structure/flora",
];

/// The vars of one `/datum/map_template/ruin/lavaland` subtype.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RuinDef {
    pub id: &'static str,
    /// `suffix`: the file in [`LAVA_RUINS_DIR`].
    pub suffix: &'static str,
    pub cost: i64,
    pub always_place: bool,
    pub allow_duplicates: bool,
    pub unpickable: bool,
}

const fn ruin(id: &'static str, suffix: &'static str, cost: i64, dup: bool) -> RuinDef {
    RuinDef {
        id,
        suffix,
        cost,
        always_place: false,
        allow_duplicates: dup,
        unpickable: false,
    }
}

const fn always(mut def: RuinDef) -> RuinDef {
    def.always_place = true;
    def
}

/// The lavaland templates with a file in `LavaRuins` (every `placement_weight` is 1).
pub const LAVALAND_RUINS: &[RuinDef] = &[
    ruin("biodome-beach", "lavaland_biodome_beach.dmm", 5, false),
    ruin(
        "biodome-winter",
        "lavaland_surface_biodome_winter.dmm",
        5,
        false,
    ),
    ruin(
        "biodome-clown",
        "lavaland_biodome_clown_planet.dmm",
        5,
        false,
    ),
    ruin("lizgas2", "lavaland_surface_gas.dmm", 0, false),
    ruin("wishgranter-cube", "lavaland_surface_cube.dmm", 10, false),
    ruin("seed-vault", "lavaland_surface_seed_vault.dmm", 10, false),
    ruin("ash-walker", "lavaland_surface_ash_walker1.dmm", 20, false),
    ruin(
        "lava-base",
        "lavaland_surface_syndicate_base1.dmm",
        20,
        false,
    ),
    ruin("gaia", "lavaland_surface_gaia.dmm", 5, false),
    ruin("envy", "lavaland_surface_envy.dmm", 10, false),
    ruin("gluttony", "lavaland_surface_gluttony.dmm", 10, false),
    ruin("greed", "lavaland_surface_greed.dmm", 10, false),
    ruin("pride", "lavaland_surface_pride.dmm", 10, false),
    // BandaStation makes sloth unpickable.
    RuinDef {
        unpickable: true,
        ..ruin("sloth", "lavaland_surface_sloth.dmm", 0, false)
    },
    ruin("ratvar", "lavaland_surface_dead_ratvar.dmm", 0, false),
    always(ruin(
        "hierophant",
        "lavaland_surface_hierophant.dmm",
        0,
        false,
    )),
    // The three blood-drunk variants share their id; `random` (always placed) picks one.
    always(ruin(
        "blooddrunk",
        "lavaland_surface_blooddrunk1.dmm",
        0,
        false,
    )),
    ruin("blooddrunk", "lavaland_surface_blooddrunk2.dmm", 0, false),
    ruin("blooddrunk", "lavaland_surface_blooddrunk3.dmm", 0, false),
    ruin("ufo-crash", "lavaland_surface_ufo_crash.dmm", 5, true),
    ruin("xeno-nest", "lavaland_surface_xeno_nest.dmm", 20, true),
    ruin(
        "survivalcapsule",
        "lavaland_surface_survivalpod.dmm",
        5,
        true,
    ),
    ruin("pizza", "lavaland_surface_pizzaparty.dmm", 5, false),
    ruin("cultaltar", "lavaland_surface_cultaltar.dmm", 10, false),
    ruin("hermitcave", "lavaland_surface_hermit.dmm", 10, false),
    ruin("ripley", "lavaland_surface_random_ripley.dmm", 5, false),
    ruin("dark_wizards", "lavaland_surface_wizard.dmm", 5, true),
    ruin("strong_stone", "lavaland_strong_rock.dmm", 2, false),
    ruin("puzzle", "lavaland_surface_puzzle.dmm", 5, true),
    always(ruin("tumor", "lavaland_surface_elite_tumor.dmm", 5, true)),
    ruin(
        "Graveyard",
        "lavaland_surface_elephant_graveyard.dmm",
        10,
        false,
    ),
    ruin(
        "bileworm_nest",
        "lavaland_surface_bileworm_nest.dmm",
        5,
        false,
    ),
    ruin(
        "lava_phonebooth",
        "lavaland_surface_phonebooth.dmm",
        5,
        false,
    ),
    ruin("battle_site", "lavaland_battle_site.dmm", 3, true),
    ruin(
        "watcher-grave",
        "lavaland_surface_watcher_grave.dmm",
        5,
        false,
    ),
    ruin("mook_village", "lavaland_surface_mookvillage.dmm", 5, false),
    ruin(
        "shuttle_wreckage",
        "lavaland_surface_shuttle_wreckage.dmm",
        0,
        false,
    ),
    ruin("crashsite", "lavaland_surface_crashsite.dmm", 0, false),
];

/// A loaded template.
pub struct Template {
    pub def: RuinDef,
    pub map: DmmMap,
}

/// A placed ruin: its id and bottom-left corner (1-based map coordinates).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Placed {
    pub id: &'static str,
    pub suffix: &'static str,
    pub corner: (u32, u32, u32),
}

/// Reads the [`LAVALAND_RUINS`] files under `source` whose every type `known` accepts. The
/// blood-drunk variant of the always-placed `random` template is picked with `seed`.
pub fn read_lavaland(source: &Path, seed: u64, known: impl Fn(&str) -> bool) -> Vec<Template> {
    let mut rng = Rng::new(seed ^ 0x5255_494E);
    let drunk = format!("lavaland_surface_blooddrunk{}.dmm", rng.range(1, 3));
    LAVALAND_RUINS
        .iter()
        .filter_map(|def| {
            let mut def = *def;
            // `blood_drunk_miner/random/New()`: `pick()` of the three files.
            if def.id == "blooddrunk" {
                def.always_place = def.suffix == drunk;
            }
            let path = source.join(LAVA_RUINS_DIR).join(def.suffix);
            let map = DmmMap::from_file(&path).ok()?;
            let unknown = map
                .prefabs
                .iter()
                .flat_map(|p| &p.objects)
                .map(|o| o.path.as_str())
                .filter(|p| *p != TURF_NOOP && *p != AREA_NOOP)
                .find(|p| !known(p));
            if let Some(path) = unknown {
                bevy::log::warn!("ruin {}: no prototype for {path}, not placed", def.suffix);
                return None;
            }
            Some(Template { def, map })
        })
        .collect()
}

/// tg `seedRuins(z_levels, budget, whitelist, ...)` on every z-level of `map`.
pub fn seed(map: &mut DmmMap, templates: &[Template], budget: i64, rng: &mut Rng) -> Vec<Placed> {
    let mut budget = budget;
    let mineral_budget = MINERAL_BUDGET;
    let mut reserved: HashSet<usize> = HashSet::new();
    let mut forced: Vec<usize> = Vec::new();
    let mut available: Vec<usize> = Vec::new();
    for (i, t) in templates.iter().enumerate() {
        if t.def.cost > budget {
            continue;
        }
        if t.def.always_place {
            forced.push(i);
        }
        if !t.def.unpickable {
            available.push(i);
        }
    }
    let mut placed = Vec::new();
    while ((budget > 0 || mineral_budget > 0) && !available.is_empty()) || !forced.is_empty() {
        let pick = if forced.is_empty() {
            // `pick_weight` with every weight 1.
            available[rng.range(0, available.len() as i64 - 1) as usize]
        } else {
            forced.remove(0)
        };
        let t = &templates[pick];
        let spot = (0..PLACEMENT_TRIES).find_map(|_| {
            let z = rng.range(1, i64::from(map.size.2)) as u32;
            try_to_place(&*map, &t.map, z, &reserved, &mut *rng)
        });
        match spot {
            None => {
                available.retain(|&i| templates[i].def.id != t.def.id);
                bevy::log::info!("Failed to place {} ruin!", t.def.id);
            }
            Some((x, y, z)) => {
                budget -= t.def.cost;
                if !t.def.allow_duplicates {
                    available.retain(|&i| templates[i].def.id != t.def.id);
                }
                let (w, h, _) = t.map.size;
                for ry in 0..h {
                    for rx in 0..w {
                        if let Some(at) = cell(map, x + rx, y + ry, z) {
                            reserved.insert(at);
                        }
                    }
                }
                paste(map, &t.map, (x, y, z));
                placed.push(Placed {
                    id: t.def.id,
                    suffix: t.def.suffix,
                    corner: (x, y, z),
                });
            }
        }
        available.retain(|&i| templates[i].def.cost <= budget);
    }
    placed
}

/// 0-based grid index of 1-based `(x, y, z)`.
fn cell(map: &DmmMap, x: u32, y: u32, z: u32) -> Option<usize> {
    let (sx, sy, sz) = map.size;
    (x >= 1 && y >= 1 && z >= 1 && x <= sx && y <= sy && z <= sz).then(|| {
        ((z as usize - 1) * sy as usize + (y as usize - 1)) * sx as usize + (x as usize - 1)
    })
}

fn area_at(map: &DmmMap, at: usize) -> Option<&str> {
    let p = map.grid[at];
    (p != NO_PREFAB)
        .then(|| &map.prefabs[p as usize])?
        .objects
        .iter()
        .find(|o| is_under(&o.path, "/area"))
        .map(|o| o.path.as_str())
}

/// `try_to_place`: a random centre at least `TRANSITIONEDGE + SPACERUIN_MAP_EDGE_PAD` plus half
/// the ruin from the edges whose footprint is all whitelisted and unreserved. The bottom-left
/// corner.
fn try_to_place(
    map: &DmmMap,
    ruin: &DmmMap,
    z: u32,
    reserved: &HashSet<usize>,
    rng: &mut Rng,
) -> Option<(u32, u32, u32)> {
    let (w, h, _) = ruin.size;
    let (sx, sy, _) = map.size;
    let wb = EDGE + i64::from(w / 2);
    let hb = EDGE + i64::from(h / 2);
    if i64::from(sx) - wb < wb || i64::from(sy) - hb < hb {
        return None;
    }
    let cx = rng.range(wb, i64::from(sx) - wb);
    let cy = rng.range(hb, i64::from(sy) - hb);
    let x0 = (cx - i64::from(w / 2)) as u32;
    let y0 = (cy - i64::from(h / 2)) as u32;
    for ry in 0..h {
        for rx in 0..w {
            let at = cell(map, x0 + rx, y0 + ry, z)?;
            if reserved.contains(&at) || !area_at(map, at).is_some_and(|a| is_under(a, RUIN_AREA)) {
                return None;
            }
        }
    }
    Some((x0, y0, z))
}

fn split(objects: &[DmmObject]) -> (Vec<&DmmObject>, Option<&DmmObject>, Option<&DmmObject>) {
    let mut movables = Vec::new();
    let (mut turf, mut area) = (None, None);
    for o in objects {
        if is_under(&o.path, "/area") {
            area = Some(o);
        } else if is_under(&o.path, "/turf") {
            turf = Some(o);
        } else {
            movables.push(o);
        }
    }
    (movables, turf, area)
}

/// `load_reserved(central_turf, clear_below = TRUE)` of the first z-level of `ruin` with its
/// bottom-left corner on `corner`.
pub fn paste(map: &mut DmmMap, ruin: &DmmMap, corner: (u32, u32, u32)) {
    let (x0, y0, z) = corner;
    let (w, h, _) = ruin.size;
    let mut merged: HashMap<(u32, u32), u32> = HashMap::new();
    for ry in 1..=h {
        for rx in 1..=w {
            let Some(rp) = ruin.index(rx, ry, 1) else {
                continue;
            };
            let Some(at) = cell(map, x0 + rx - 1, y0 + ry - 1, z) else {
                continue;
            };
            let tp = map.grid[at];
            let i = *merged.entry((rp, tp)).or_insert_with(|| {
                let below: &[DmmObject] = if tp == NO_PREFAB {
                    &[]
                } else {
                    &map.prefabs[tp as usize].objects
                };
                let (old_movables, old_turf, old_area) = split(below);
                let (new_movables, new_turf, new_area) = split(&ruin.prefabs[rp as usize].objects);
                let mut objects: Vec<DmmObject> = old_movables
                    .into_iter()
                    .filter(|o| !CLEAR_BELOW.iter().any(|root| is_under(&o.path, root)))
                    .chain(new_movables)
                    .cloned()
                    .collect();
                let turf = new_turf.filter(|t| t.path != TURF_NOOP).or(old_turf);
                let area = new_area.filter(|a| a.path != AREA_NOOP).or(old_area);
                objects.extend(turf.cloned());
                objects.extend(area.cloned());
                let key = format!(
                    "{}+{}",
                    ruin.prefabs[rp as usize].key,
                    if tp == NO_PREFAB {
                        String::new()
                    } else {
                        map.prefabs[tp as usize].key.clone()
                    }
                );
                map.prefabs.push(Prefab { key, objects });
                (map.prefabs.len() - 1) as u32
            });
            map.grid[at] = i;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: &str = "/area/lavaland/surface/outdoors/unexplored";

    fn level(w: usize, h: usize) -> DmmMap {
        let row = "a".repeat(w);
        let rows: Vec<String> = (0..h).map(|_| row.clone()).collect();
        let text = format!(
            "\"a\" = (/obj/structure/flora/rock,/turf/open/misc/asteroid/basalt/lava_land_surface,{AREA})\n\n(1,1,1) = {{\"\n{}\n\"}}\n",
            rows.join("\n")
        );
        DmmMap::parse(&text).unwrap()
    }

    fn ruin_map() -> DmmMap {
        DmmMap::parse(
            "\
\"n\" = (/turf/template_noop,/area/template_noop)
\"w\" = (/obj/item/pickaxe,/turf/closed/wall,/area/ruin/unpowered)

(1,1,1) = {\"
nwn
www
\"}
",
        )
        .unwrap()
    }

    fn template(id: &'static str, cost: i64, always_place: bool, dup: bool) -> Template {
        Template {
            def: RuinDef {
                always_place,
                ..ruin(id, "test.dmm", cost, dup)
            },
            map: ruin_map(),
        }
    }

    #[test]
    fn paste_keeps_noop_cells_and_clears_flora() {
        let mut map = level(10, 10);
        paste(&mut map, &ruin_map(), (4, 4, 1));
        // Ruin row 2 (top) is map y 5; the noop corner keeps the cave's turf and area, but its
        // rock (flora) is cleared below the ruin.
        let corner = map.get(4, 5, 1).unwrap();
        let paths: Vec<&str> = corner.objects.iter().map(|o| o.path.as_str()).collect();
        assert_eq!(
            paths,
            ["/turf/open/misc/asteroid/basalt/lava_land_surface", AREA]
        );
        // A wall cell: the flora is cleared, the ruin's item, turf and area take its place.
        let wall = map.get(5, 5, 1).unwrap();
        let paths: Vec<&str> = wall.objects.iter().map(|o| o.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                "/obj/item/pickaxe",
                "/turf/closed/wall",
                "/area/ruin/unpowered"
            ]
        );
        assert_eq!(
            map.get(4, 4, 1).unwrap().objects[1].path,
            "/turf/closed/wall"
        );
        // Outside the footprint nothing changes.
        assert_eq!(map.get(7, 4, 1).unwrap().objects.len(), 3);
        assert_eq!(
            map.get(7, 4, 1).unwrap().objects[1].path,
            "/turf/open/misc/asteroid/basalt/lava_land_surface"
        );
    }

    #[test]
    fn seeding_spends_the_budget_and_keeps_ruins_apart() {
        let mut map = level(120, 120);
        let templates = [
            template("forced", 5, true, false),
            template("cheap", 10, false, true),
            template("unique", 10, false, false),
            template("pricey", 100, false, true),
        ];
        let placed = seed(&mut map, &templates, 60, &mut Rng::new(3));
        assert_eq!(placed.iter().filter(|p| p.id == "forced").count(), 1);
        assert!(placed.iter().filter(|p| p.id == "unique").count() <= 1);
        assert!(!placed.iter().any(|p| p.id == "pricey"));
        let spent: i64 = placed
            .iter()
            .map(|p| {
                templates
                    .iter()
                    .find(|t| t.def.id == p.id)
                    .unwrap()
                    .def
                    .cost
            })
            .sum();
        assert!((50..=60).contains(&spent), "{spent}");
        // No two footprints (3x2) overlap, and every one is off the map edge padding.
        for (i, a) in placed.iter().enumerate() {
            assert!(a.corner.0 >= 22 && a.corner.0 + 2 <= 120 - 22, "{a:?}");
            for b in &placed[i + 1..] {
                let apart = a.corner.0 + 3 <= b.corner.0
                    || b.corner.0 + 3 <= a.corner.0
                    || a.corner.1 + 2 <= b.corner.1
                    || b.corner.1 + 2 <= a.corner.1;
                assert!(apart, "{a:?} {b:?}");
            }
        }
    }

    #[test]
    fn ruins_only_land_in_the_unexplored_wastes() {
        let mut map = level(60, 60);
        // Turn the whole level into the mining base's area: nothing fits.
        for p in &mut map.prefabs {
            p.objects.last_mut().unwrap().path = "/area/mine/production".into();
        }
        let placed = seed(
            &mut map,
            &[template("forced", 0, true, false)],
            60,
            &mut Rng::new(1),
        );
        assert!(placed.is_empty());
    }

    #[test]
    fn the_ruin_table_matches_tg() {
        let hiero = LAVALAND_RUINS
            .iter()
            .find(|r| r.id == "hierophant")
            .unwrap();
        assert!(hiero.always_place && !hiero.allow_duplicates);
        let tumor = LAVALAND_RUINS.iter().find(|r| r.id == "tumor").unwrap();
        assert!(tumor.always_place && tumor.allow_duplicates && tumor.cost == 5);
        assert!(
            LAVALAND_RUINS
                .iter()
                .find(|r| r.id == "sloth")
                .unwrap()
                .unpickable
        );
        assert_eq!(
            LAVALAND_RUINS
                .iter()
                .filter(|r| r.id == "blooddrunk")
                .count(),
            3
        );
    }
}
