//! Fishing as prototype data (tg `code/modules/fishing`): a fish type's vars (`/obj/item/fish`,
//! `fish/_fish.dm`) as [`DmFish`], a rod's (`/obj/item/fishing_rod`, `fishing_rod.dm`) as
//! [`DmFishingRod`]; the fish sources (`/datum/fish_source/*`, `sources/`) the game rolls catches
//! from, the turfs that are fishing spots and what the game spawns from them.
//!
//! Fish sources are datums, not prototypes, so their tables are written out here.
// shortcut: hand-copied `fish_table`s of the ported sources (river, ocean, beach, Tiziran sea,
// hot spring, ice, aquarium, the fishing portal's aquarium and Syndicate dimensions) without the
// rewards that are not items (message bottles, mystery boxes, frog spawners, chasm detritus);
// export `/datum/fish_source` from the dump like `research.ron` when more sources are ported.

use bevy::prelude::*;
use serde_json::Value;

use crate::convert::DmTypes;
use crate::fidelity::{ext, is_under};
use crate::value::Field;

pub const FISH_ROOT: &str = "/obj/item/fish";
pub const ROD_ROOT: &str = "/obj/item/fishing_rod";
pub const FISH_COMPONENT: &str = "dm_fish";
pub const ROD_COMPONENT: &str = "dm_fishing_rod";
/// `FISHING_DUD`.
pub const DUD: &str = "dud";
/// `/datum/fish_source/aquarium`: its table is the fish inside the aquarium.
pub const AQUARIUM_SOURCE: &str = "/datum/fish_source/aquarium";
/// `/datum/fish_source/portal` and `/portal/syndicate` (the fish-porter's settings).
pub const PORTAL_SOURCE: &str = "/datum/fish_source/portal";
pub const SYNDICATE_PORTAL_SOURCE: &str = "/datum/fish_source/portal/syndicate";
/// `/datum/fish_source` `duds`.
const DEFAULT_DUDS: &[&str] = &["it was nothing", "the hook is empty"];
/// `FISHING_DEFAULT_DIFFICULTY`, `FISHING_EASY_DIFFICULTY`.
pub const DEFAULT_DIFFICULTY: f32 = 15.0;
pub const EASY_DIFFICULTY: f32 = 10.0;
/// The regen of a limited reward that sets none (`/datum/fish_source/New`: 30 minutes).
const DEFAULT_REGEN_SECS: f64 = 1800.0;

/// One fish type (prototype data). Temperatures in kelvin, `feeding_frequency` in seconds.
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmFish {
    pub average_size: f32,
    pub average_weight: f32,
    pub fishing_difficulty_modifier: f32,
    /// `fish_traits`: `/datum/fish_trait/...` paths.
    pub fish_traits: Vec<String>,
    /// `required_fluid_type`: `"Freshwater"`, `"Saltwater"`, `"Sulfuric Water"`, `"Air"`,
    /// `"Anadromous"`, `"Any Fluid"`.
    pub required_fluid_type: String,
    pub required_temperature_min: f32,
    pub required_temperature_max: f32,
    pub feeding_frequency: f32,
    /// `fish_movement_type`: a `/datum/fish_movement` path.
    pub fish_movement_type: String,
    pub stable_population: u32,
    /// Empty: no dead sprite.
    pub icon_state_dead: String,
    /// `death_text` with `%SRC`.
    pub death_text: String,
}

/// One fishing rod type (prototype data).
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmFishingRod {
    pub cast_range: f32,
    pub difficulty_modifier: f32,
    pub completion_speed_mult: f32,
    pub bait_speed_mult: f32,
    pub deceleration_mult: f32,
    pub bounciness_mult: f32,
    pub gravity_mult: f32,
    pub bait_height_mult: f32,
    /// `line` / `hook`: spawned with one.
    pub has_line: bool,
    pub has_hook: bool,
}

fn ron<T: serde::Serialize>(v: &T) -> String {
    ::ron::to_string(v).expect("plain values serialize")
}

fn number(v: Option<&Value>, default: f64) -> f64 {
    v.and_then(Value::as_f64).unwrap_or(default)
}

fn text(v: Option<&Value>) -> String {
    v.and_then(Value::as_str).unwrap_or_default().to_owned()
}

/// A `{"path": ...}` value.
fn path_of(v: Option<&Value>) -> Option<String> {
    v?.get("path")?.as_str().map(str::to_owned)
}

fn paths(v: Option<&Value>) -> Vec<String> {
    v.and_then(|v| v.get("list"))
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(|i| path_of(Some(i))).collect())
        .unwrap_or_default()
}

/// The fish type `path`; `None` for other types.
pub fn fish(types: &DmTypes, path: &str) -> Option<DmFish> {
    if !is_under(path, FISH_ROOT) || !types.types.contains_key(path) {
        return None;
    }
    let get = |var: &str| types.resolved(path, var);
    Some(DmFish {
        average_size: number(get("average_size"), 50.0) as f32,
        average_weight: number(get("average_weight"), 1000.0) as f32,
        fishing_difficulty_modifier: number(get("fishing_difficulty_modifier"), 0.0) as f32,
        fish_traits: paths(get("fish_traits")),
        required_fluid_type: match text(get("required_fluid_type")) {
            t if t.is_empty() => "Freshwater".to_owned(),
            t => t,
        },
        required_temperature_min: number(get("required_temperature_min"), 273.15) as f32,
        required_temperature_max: number(get("required_temperature_max"), 373.15) as f32,
        // Deciseconds in DM.
        feeding_frequency: (number(get("feeding_frequency"), 3000.0) / 10.0) as f32,
        fish_movement_type: path_of(get("fish_movement_type"))
            .unwrap_or_else(|| "/datum/fish_movement".to_owned()),
        stable_population: number(get("stable_population"), 1.0).max(1.0) as u32,
        icon_state_dead: text(get("icon_state_dead")),
        death_text: match text(get("death_text")) {
            t if t.is_empty() => "%SRC dies.".to_owned(),
            t => t,
        },
    })
}

/// The rod type `path`; `None` for other types.
pub fn rod(types: &DmTypes, path: &str) -> Option<DmFishingRod> {
    if !is_under(path, ROD_ROOT) || !types.types.contains_key(path) {
        return None;
    }
    let get = |var: &str| types.resolved(path, var);
    let mult = |var: &str| number(get(var), 1.0) as f32;
    Some(DmFishingRod {
        cast_range: number(get("cast_range"), 3.0) as f32,
        difficulty_modifier: number(get("difficulty_modifier"), 0.0) as f32,
        completion_speed_mult: mult("completion_speed_mult"),
        bait_speed_mult: mult("bait_speed_mult"),
        deceleration_mult: mult("deceleration_mult"),
        bounciness_mult: mult("bounciness_mult"),
        gravity_mult: mult("gravity_mult"),
        bait_height_mult: mult("bait_height_mult"),
        has_line: path_of(get("line")).is_some(),
        has_hook: path_of(get("hook")).is_some(),
    })
}

/// Converter fields of a fish or rod type: the whole component.
pub fn type_fields(types: &DmTypes, path: &str) -> Vec<Field> {
    if let Some(f) = fish(types, path) {
        let c = FISH_COMPONENT;
        return vec![
            ext(c, "average_size", ron(&f.average_size)),
            ext(c, "average_weight", ron(&f.average_weight)),
            ext(
                c,
                "fishing_difficulty_modifier",
                ron(&f.fishing_difficulty_modifier),
            ),
            ext(c, "fish_traits", ron(&f.fish_traits)),
            ext(c, "required_fluid_type", ron(&f.required_fluid_type)),
            ext(
                c,
                "required_temperature_min",
                ron(&f.required_temperature_min),
            ),
            ext(
                c,
                "required_temperature_max",
                ron(&f.required_temperature_max),
            ),
            ext(c, "feeding_frequency", ron(&f.feeding_frequency)),
            ext(c, "fish_movement_type", ron(&f.fish_movement_type)),
            ext(c, "stable_population", ron(&f.stable_population)),
            ext(c, "icon_state_dead", ron(&f.icon_state_dead)),
            ext(c, "death_text", ron(&f.death_text)),
        ];
    }
    if let Some(r) = rod(types, path) {
        let c = ROD_COMPONENT;
        return vec![
            ext(c, "cast_range", ron(&r.cast_range)),
            ext(c, "difficulty_modifier", ron(&r.difficulty_modifier)),
            ext(c, "completion_speed_mult", ron(&r.completion_speed_mult)),
            ext(c, "bait_speed_mult", ron(&r.bait_speed_mult)),
            ext(c, "deceleration_mult", ron(&r.deceleration_mult)),
            ext(c, "bounciness_mult", ron(&r.bounciness_mult)),
            ext(c, "gravity_mult", ron(&r.gravity_mult)),
            ext(c, "bait_height_mult", ron(&r.bait_height_mult)),
            ext(c, "has_line", ron(&r.has_line)),
            ext(c, "has_hook", ron(&r.has_hook)),
        ];
    }
    Vec::new()
}

/// One `/datum/fish_source` subtype.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FishSourceDef {
    pub path: &'static str,
    /// `fishing_difficulty` (added to the rod's, the fish's and the fisher's).
    pub fishing_difficulty: f32,
    /// `duds`: the line shown when nothing bites.
    pub duds: &'static [&'static str],
    /// `fish_table`: reward ([`DUD`] or a type) and relative weight.
    pub table: &'static [(&'static str, u32)],
    /// `fish_counts` with `fish_count_regen`: limited rewards, their count and the seconds one
    /// takes to come back.
    pub counts: &'static [(&'static str, u32, f64)],
    /// `background` of the minigame.
    pub background: &'static str,
}

const RIVER_TABLE: &[(&str, u32)] = &[
    (DUD, 4),
    ("/obj/item/fish/goldfish", 5),
    ("/obj/item/fish/guppy", 5),
    ("/obj/item/fish/plasmatetra", 4),
    ("/obj/item/fish/perch", 4),
    ("/obj/item/fish/angelfish", 4),
    ("/obj/item/fish/catfish", 4),
    ("/obj/item/fish/slimefish", 2),
    ("/obj/item/fish/sockeye_salmon", 1),
    ("/obj/item/fish/arctic_char", 1),
    ("/obj/item/fish/pike", 1),
    ("/obj/item/fish/goldfish/three_eyes", 1),
];

const OCEAN_TABLE: &[(&str, u32)] = &[
    (DUD, 10),
    ("/obj/item/coin/gold", 6),
    ("/obj/item/fish/clownfish", 11),
    ("/obj/item/fish/pufferfish", 11),
    ("/obj/item/fish/cardinal", 11),
    ("/obj/item/fish/greenchromis", 11),
    ("/obj/item/fish/squid", 11),
    ("/obj/item/fish/stingray", 8),
    ("/obj/item/fish/plaice", 8),
    ("/obj/item/fish/monkfish", 5),
    ("/obj/item/fish/lanternfish", 7),
    ("/obj/item/fish/zipzap", 7),
    ("/obj/item/fish/clownfish/lube", 5),
    ("/obj/item/fish/swordfish", 5),
];

const OCEAN_COUNTS: &[(&str, u32, f64)] = &[
    ("/obj/item/fish/clownfish/lube", 2, 180.0),
    ("/obj/item/fish/swordfish", 2, 300.0),
];

pub const FISH_SOURCES: &[FishSourceDef] = &[
    FishSourceDef {
        path: "/datum/fish_source/river",
        fishing_difficulty: DEFAULT_DIFFICULTY + 15.0,
        duds: DEFAULT_DUDS,
        table: RIVER_TABLE,
        counts: &[("/obj/item/fish/pike", 3, 240.0)],
        background: "background_default",
    },
    FishSourceDef {
        path: "/datum/fish_source/ocean",
        fishing_difficulty: DEFAULT_DIFFICULTY + 15.0,
        duds: DEFAULT_DUDS,
        table: OCEAN_TABLE,
        counts: OCEAN_COUNTS,
        background: "background_default",
    },
    FishSourceDef {
        path: "/datum/fish_source/ocean/beach",
        fishing_difficulty: DEFAULT_DIFFICULTY + 15.0,
        duds: DEFAULT_DUDS,
        table: OCEAN_TABLE,
        counts: OCEAN_COUNTS,
        background: "background_default",
    },
    FishSourceDef {
        path: "/datum/fish_source/ice_fishing",
        fishing_difficulty: DEFAULT_DIFFICULTY + 30.0,
        duds: DEFAULT_DUDS,
        table: &[
            (DUD, 4),
            ("/obj/item/fish/arctic_char", 5),
            ("/obj/item/fish/sockeye_salmon", 5),
            ("/obj/item/fish/chasm_crab/ice", 2),
            ("/obj/item/fish/boned", 1),
        ],
        counts: &[],
        background: "background_ice",
    },
    FishSourceDef {
        path: "/datum/fish_source/hot_spring",
        fishing_difficulty: DEFAULT_DIFFICULTY + 30.0,
        duds: DEFAULT_DUDS,
        table: &[
            (DUD, 20),
            ("/obj/item/fish/bumpy", 10),
            ("/obj/item/fish/sacabambaspis", 10),
            ("/obj/item/fishing_rod/telescopic/master", 1),
        ],
        counts: &[
            ("/obj/item/fish/sacabambaspis", 5, 240.0),
            (
                "/obj/item/fishing_rod/telescopic/master",
                2,
                DEFAULT_REGEN_SECS,
            ),
        ],
        background: "background_default",
    },
    FishSourceDef {
        path: "/datum/fish_source/tizira",
        fishing_difficulty: DEFAULT_DIFFICULTY + 20.0,
        duds: DEFAULT_DUDS,
        table: &[
            (DUD, 10),
            ("/obj/item/fish/needlefish", 5),
            ("/obj/item/fish/armorfish", 5),
            ("/obj/item/fish/gunner_jellyfish", 4),
            ("/obj/item/fish/moonfish/dwarf", 2),
            ("/obj/item/fish/moonfish", 2),
        ],
        counts: &[],
        background: "background_default",
    },
    FishSourceDef {
        path: AQUARIUM_SOURCE,
        fishing_difficulty: EASY_DIFFICULTY + 5.0,
        duds: DEFAULT_DUDS,
        table: &[(DUD, 10)],
        counts: &[],
        background: "background_default",
    },
    FishSourceDef {
        path: PORTAL_SOURCE,
        fishing_difficulty: DEFAULT_DIFFICULTY,
        duds: DEFAULT_DUDS,
        table: &[
            (DUD, 7),
            ("/obj/item/fish/goldfish", 10),
            ("/obj/item/fish/guppy", 10),
            ("/obj/item/fish/angelfish", 10),
            ("/obj/item/fish/perch", 5),
            ("/obj/item/fish/goldfish/three_eyes", 3),
            ("/obj/item/fish/plasmatetra", 3),
        ],
        counts: &[],
        background: "background_default",
    },
    FishSourceDef {
        path: SYNDICATE_PORTAL_SOURCE,
        fishing_difficulty: DEFAULT_DIFFICULTY + 25.0,
        duds: DEFAULT_DUDS,
        table: &[
            (DUD, 5),
            ("/obj/item/fish/donkfish", 5),
            ("/obj/item/fish/emulsijack", 5),
            ("/obj/item/fish/jumpercable", 5),
            ("/obj/item/fish/chainsawfish", 2),
            ("/obj/item/fish/pike/armored", 2),
        ],
        counts: &[
            ("/obj/item/fish/chainsawfish", 1, 420.0),
            ("/obj/item/fish/pike/armored", 1, 420.0),
        ],
        background: "background_lavaland",
    },
];

pub fn fish_source(path: &str) -> Option<&'static FishSourceDef> {
    FISH_SOURCES.iter().find(|s| s.path == path)
}

/// Turf types and their `fishing_datum` (`code/game/turfs/open/water.dm`), most specific first;
/// `None` is `fishing_datum = null`.
// shortcut: no lava (`fish_source_type`, lavaland), sand, chasm, ice (`ice.dm`), holodeck or
// cursed spring spots.
pub const TURF_SPOTS: &[(&str, Option<&str>)] = &[
    ("/turf/open/water/no_fishing", None),
    ("/turf/open/water/alternative/no_fishing", None),
    ("/turf/open/water/alternative/muddy/no_fishing", None),
    ("/turf/open/water/hot_spring/cursed", None),
    (
        "/turf/open/water/beach/tizira",
        Some("/datum/fish_source/tizira"),
    ),
    (
        "/turf/open/water/beach",
        Some("/datum/fish_source/ocean/beach"),
    ),
    (
        "/turf/open/water/deep_beach",
        Some("/datum/fish_source/ocean"),
    ),
    (
        "/turf/open/water/lavaland_atmos/basalt",
        Some("/datum/fish_source/ocean"),
    ),
    (
        "/turf/open/water/hot_spring",
        Some("/datum/fish_source/hot_spring"),
    ),
    ("/turf/open/water", Some("/datum/fish_source/river")),
];

/// The fish source of a turf type, if it is a fishing spot.
pub fn turf_source(path: &str) -> Option<&'static FishSourceDef> {
    TURF_SPOTS
        .iter()
        .find(|(root, _)| is_under(path, root))
        .and_then(|(_, source)| fish_source((*source)?))
}

/// `/obj/structure/aquarium/prefilled` `Initialize`: its fish.
pub const PREFILLED_FISH: [&str; 3] = [
    "/obj/item/fish/goldfish",
    "/obj/item/fish/angelfish",
    "/obj/item/fish/guppy",
];

/// What fishing spawns: every reward of the sources and the prefilled aquarium's fish.
pub fn runtime_types() -> impl Iterator<Item = &'static str> {
    FISH_SOURCES
        .iter()
        .flat_map(|s| s.table.iter().map(|&(p, _)| p))
        .filter(|&p| p != DUD)
        .chain(PREFILLED_FISH)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TYPES: &str = r##"{ "schema": 3, "types": {
        "/datum": { "parent": null },
        "/atom": { "parent": "/datum" },
        "/obj": { "parent": "/atom" },
        "/obj/item": { "parent": "/obj" },
        "/obj/item/fish": { "parent": "/obj/item", "declared_vars": {
            "average_size": 50, "average_weight": 1000, "fishing_difficulty_modifier": 0,
            "fish_traits": { "list": [] }, "required_fluid_type": "Freshwater",
            "required_temperature_min": 273.15, "required_temperature_max": 373.15,
            "feeding_frequency": 3000, "fish_movement_type": { "path": "/datum/fish_movement" },
            "stable_population": 1, "icon_state_dead": null, "death_text": "%SRC dies." } },
        "/obj/item/fish/pike": { "parent": "/obj/item/fish", "declared_vars": {
            "average_size": 100, "fishing_difficulty_modifier": 10,
            "fish_traits": { "list": [{ "path": "/datum/fish_trait/carnivore" }] },
            "fish_movement_type": { "path": "/datum/fish_movement/plunger" } } },
        "/obj/item/fishing_rod": { "parent": "/obj/item", "declared_vars": {
            "cast_range": 3, "difficulty_modifier": 0, "completion_speed_mult": 1,
            "line": { "path": "/obj/item/fishing_line" }, "hook": { "path": "/obj/item/fishing_hook" } } },
        "/obj/item/fishing_rod/unslotted": { "parent": "/obj/item/fishing_rod", "declared_vars": {
            "line": null, "hook": null } }
    } }"##;

    #[test]
    fn fish_and_rods_resolve_their_vars() {
        let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
        let pike = fish(&types, "/obj/item/fish/pike").unwrap();
        assert_eq!(pike.average_size, 100.0);
        assert_eq!(pike.average_weight, 1000.0);
        assert_eq!(pike.fishing_difficulty_modifier, 10.0);
        assert_eq!(pike.fish_traits, ["/datum/fish_trait/carnivore"]);
        assert_eq!(pike.fish_movement_type, "/datum/fish_movement/plunger");
        assert_eq!(pike.feeding_frequency, 300.0);
        assert_eq!(pike.required_fluid_type, "Freshwater");
        let rod_def = rod(&types, ROD_ROOT).unwrap();
        assert!(rod_def.has_line && rod_def.has_hook);
        assert_eq!(rod_def.cast_range, 3.0);
        assert_eq!(rod_def.gravity_mult, 1.0);
        let bare = rod(&types, "/obj/item/fishing_rod/unslotted").unwrap();
        assert!(!bare.has_line && !bare.has_hook);
        assert!(type_fields(&types, "/obj/item").is_empty());
        assert_eq!(type_fields(&types, "/obj/item/fish/pike").len(), 12);
    }

    #[test]
    fn water_turfs_map_to_their_sources() {
        let river = turf_source("/turf/open/water/no_planet_atmos/deep").unwrap();
        assert_eq!(river.path, "/datum/fish_source/river");
        assert_eq!(
            turf_source("/turf/open/water/beach/tizira").unwrap().path,
            "/datum/fish_source/tizira"
        );
        assert!(turf_source("/turf/open/water/no_fishing").is_none());
        assert!(turf_source("/turf/open/floor").is_none());
        // Every limited reward is in its table.
        for source in FISH_SOURCES {
            for (path, ..) in source.counts {
                assert!(source.table.iter().any(|(p, _)| p == path), "{path}");
            }
        }
        assert!(runtime_types().all(|p| p != DUD));
    }
}
