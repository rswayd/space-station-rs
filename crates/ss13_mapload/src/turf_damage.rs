//! Broken and burnt floors: tg `/turf/open` `broken`, `burnt`, `damaged_dmi`, the
//! `broken_states()` / `burnt_states()` procs and the `burn_tile()` overrides
//! (`code/game/turfs/open/_open.dm`, `floor.dm` and the floor files).
//!
//! The state lists are procs, so [`STATES`] is their bodies read by a person (with the few that
//! compute a name from the type's vars); [`BURN_RULES`] the `burn_tile()` overrides.
//! `source_matches_tables` re-reads the reference so an upstream change shows up in tests.

use bevy::prelude::*;

use crate::convert::DmTypes;
use crate::fidelity::{ext, is_under, truthy};
use crate::value::Field;

const COMPONENT: &str = "dm_turf_damage";

/// What a proc body returns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum States {
    List(&'static [&'static str]),
    /// `/turf/open/misc/asteroid`: `initial(dug) ? icon_state : "[base_icon_state]_dug"`.
    AsteroidDug,
    /// `/turf/open/floor/mineral`: `"[initial(icon_state)]_dam"` (none without an icon state).
    MineralDam,
    /// `/turf/open/floor/grass`: `"[initial(icon_state)]_damaged"`.
    GrassDamaged,
    /// `/turf/open/misc/grass`: `"[base_broken_icon_state]-255"` (burnt:
    /// `base_burnt_icon_state`); the smoothing junction variant is not followed.
    MiscGrass,
}

const FLOOR_DAMAGED: &[&str] = &["damaged1", "damaged2", "damaged3", "damaged4", "damaged5"];

/// `(type, burnt_states (else broken_states), body)`: every override in the reference; the
/// later of two definitions (modular_bandastation's) wins, as in the compiled game.
pub const STATES: &[(&str, bool, States)] = &[
    ("/turf/open", false, States::List(&[])),
    ("/turf/open", true, States::List(&[])),
    ("/turf/open/floor", false, States::List(FLOOR_DAMAGED)),
    ("/turf/open/floor", true, States::List(&[])),
    ("/turf/open/floor/iron", false, States::List(FLOOR_DAMAGED)),
    (
        "/turf/open/floor/iron",
        true,
        States::List(&["floorscorched1", "floorscorched2"]),
    ),
    (
        "/turf/open/floor/plating",
        false,
        States::List(&["damaged1", "damaged2", "damaged4"]),
    ),
    (
        "/turf/open/floor/plating",
        true,
        States::List(&["floorscorched1", "floorscorched2"]),
    ),
    (
        "/turf/open/floor/tram",
        false,
        States::List(&["tram_platform-damaged1", "tram_platform-damaged2"]),
    ),
    (
        "/turf/open/floor/tram/tram_platform",
        true,
        States::List(&["tram_platform-scorched1", "tram_platform-scorched2"]),
    ),
    (
        "/turf/open/floor/tram/plate",
        false,
        States::List(&["tram_plate-damaged1", "tram_plate-damaged2"]),
    ),
    (
        "/turf/open/floor/tram/plate",
        true,
        States::List(&["tram_plate-scorched1", "tram_plate-scorched2"]),
    ),
    (
        "/turf/open/floor/tram/plate/energized",
        false,
        States::List(&["energized_plate_damaged"]),
    ),
    (
        "/turf/open/floor/tram/plate/energized",
        true,
        States::List(&["energized_plate_damaged"]),
    ),
    (
        "/turf/open/misc/sandy_dirt",
        false,
        States::List(&["sand_damaged"]),
    ),
    ("/turf/open/misc/asteroid", false, States::AsteroidDug),
    (
        "/turf/open/misc/asteroid/dug",
        false,
        States::List(&["asteroid_dug"]),
    ),
    (
        "/turf/open/misc/asteroid/snow",
        true,
        States::List(&["snow_dug"]),
    ),
    ("/turf/open/misc/grass", false, States::MiscGrass),
    ("/turf/open/misc/grass", true, States::MiscGrass),
    ("/turf/open/misc/snow", false, States::List(&["snow_dug"])),
    ("/turf/open/floor/mineral", false, States::MineralDam),
    (
        "/turf/open/floor/mineral/plasma/tiled",
        false,
        States::List(FLOOR_DAMAGED),
    ),
    (
        "/turf/open/floor/mineral/gold/tiled",
        false,
        States::List(FLOOR_DAMAGED),
    ),
    (
        "/turf/open/floor/mineral/gold/sun",
        false,
        States::List(FLOOR_DAMAGED),
    ),
    (
        "/turf/open/floor/mineral/silver/tiled",
        false,
        States::List(FLOOR_DAMAGED),
    ),
    (
        "/turf/open/floor/mineral/silver/moon",
        false,
        States::List(FLOOR_DAMAGED),
    ),
    (
        "/turf/open/floor/mineral/titanium",
        false,
        States::List(FLOOR_DAMAGED),
    ),
    (
        "/turf/open/floor/mineral/titanium/tiled",
        false,
        States::List(FLOOR_DAMAGED),
    ),
    (
        "/turf/open/floor/mineral/plastitanium",
        false,
        States::List(FLOOR_DAMAGED),
    ),
    (
        "/turf/open/floor/mineral/bananium/tiled",
        false,
        States::List(FLOOR_DAMAGED),
    ),
    (
        "/turf/open/floor/mineral/diamond/tiled",
        false,
        States::List(FLOOR_DAMAGED),
    ),
    (
        "/turf/open/floor/mineral/uranium/tiled",
        false,
        States::List(FLOOR_DAMAGED),
    ),
    (
        "/turf/open/floor/mineral/bluespace",
        false,
        States::List(FLOOR_DAMAGED),
    ),
    (
        "/turf/open/floor/mineral/telecrystal",
        false,
        States::List(FLOOR_DAMAGED),
    ),
    (
        "/turf/open/floor/mineral/adamantine",
        false,
        States::List(FLOOR_DAMAGED),
    ),
    (
        "/turf/open/floor/mineral/metal_hydrogen",
        false,
        States::List(FLOOR_DAMAGED),
    ),
    (
        "/turf/open/floor/mineral/runite",
        false,
        States::List(FLOOR_DAMAGED),
    ),
    (
        "/turf/open/floor/mineral/mythril",
        false,
        States::List(FLOOR_DAMAGED),
    ),
    (
        "/turf/open/floor/concrete",
        false,
        States::List(&["concdam_1", "concdam_2", "concdam_3", "concdam_4"]),
    ),
    (
        "/turf/open/floor/noslip",
        false,
        States::List(&["noslip-damaged1", "noslip-damaged2", "noslip-damaged3"]),
    ),
    (
        "/turf/open/floor/noslip",
        true,
        States::List(&["noslip-scorched1", "noslip-scorched2"]),
    ),
    (
        "/turf/open/floor/plastic",
        false,
        States::List(&["plastic-damaged1", "plastic-damaged2"]),
    ),
    (
        "/turf/open/floor/eighties",
        false,
        States::List(&["eighties_damaged"]),
    ),
    (
        "/turf/open/floor/eighties/red",
        false,
        States::List(&["eightiesred_damaged"]),
    ),
    (
        "/turf/open/floor/cult",
        false,
        States::List(&[
            "cultdamage",
            "cultdamage2",
            "cultdamage3",
            "cultdamage4",
            "cultdamage5",
            "cultdamage6",
            "cultdamage7",
        ]),
    ),
    (
        "/turf/open/floor/wood",
        false,
        States::List(&[
            "wood-broken",
            "wood-broken2",
            "wood-broken3",
            "wood-broken4",
            "wood-broken5",
            "wood-broken6",
            "wood-broken7",
        ]),
    ),
    (
        "/turf/open/floor/wood/tile",
        false,
        States::List(&["wood_tile-broken", "wood_tile-broken2", "wood_tile-broken3"]),
    ),
    (
        "/turf/open/floor/wood/parquet",
        false,
        States::List(&[
            "wood_parquet-broken",
            "wood_parquet-broken2",
            "wood_parquet-broken3",
            "wood_parquet-broken4",
            "wood_parquet-broken5",
            "wood_parquet-broken6",
            "wood_parquet-broken7",
        ]),
    ),
    // BandaStation's `aesthetics/floors` redefines it after tg's `wood_large-broken*`.
    (
        "/turf/open/floor/wood/large",
        false,
        States::List(&[
            "wood_fancy-broken",
            "wood_fancy-broken2",
            "wood_fancy-broken3",
        ]),
    ),
    (
        "/turf/open/floor/bamboo",
        false,
        States::List(&["bamboodamaged"]),
    ),
    (
        "/turf/open/floor/bamboo/tatami",
        false,
        States::List(&["tatami-damaged"]),
    ),
    ("/turf/open/floor/grass", false, States::GrassDamaged),
    (
        "/turf/open/floor/fake_snow",
        false,
        States::List(&["snow_dug"]),
    ),
    (
        "/turf/open/floor/fakespace",
        false,
        States::List(&["damaged"]),
    ),
    (
        "/turf/open/floor/light",
        false,
        States::List(&["light_broken"]),
    ),
    (
        "/turf/open/floor/glass",
        false,
        States::List(&["glass-damaged1", "glass-damaged2", "glass-damaged3"]),
    ),
];

/// What `burn_tile()` does to a turf.
#[derive(Reflect, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TurfBurn {
    /// `/turf/open`: `burnt = TRUE` and the burnt overlay, once, when it has a `damaged_dmi`.
    #[default]
    Overlay,
    /// Overrides that return at once (holograms, ash, ice, rock, abductor and engine floors,
    /// foam and abductor plating).
    Never,
    /// `/turf/open/floor/carpet`: `burnt` makes it plating (`make_plating`).
    Plating,
}

/// `(type, rule)`: the `burn_tile()` overrides.
pub const BURN_RULES: &[(&str, TurfBurn)] = &[
    ("/turf/open", TurfBurn::Overlay),
    ("/turf/open/floor/holofloor", TurfBurn::Never),
    ("/turf/open/misc/ashplanet", TurfBurn::Never),
    ("/turf/open/misc/ice", TurfBurn::Never),
    ("/turf/open/misc/asteroid", TurfBurn::Never),
    // Snow melts: `burnt`, no slowdown, its `snow_dug` look.
    // shortcut: the slowdown is not dropped (turf slowdown is not ported).
    ("/turf/open/misc/asteroid/snow", TurfBurn::Overlay),
    ("/turf/open/misc/asteroid/snow/ice", TurfBurn::Never),
    ("/turf/open/floor/mineral/abductor", TurfBurn::Never),
    ("/turf/open/floor/engine", TurfBurn::Never),
    ("/turf/open/floor/carpet", TurfBurn::Plating),
    ("/turf/open/floor/plating/foam", TurfBurn::Never),
    ("/turf/open/floor/plating/abductor2", TurfBurn::Never),
];

/// A turf type's damage looks (prototype data).
#[derive(Component, Reflect, Clone, Debug, Default, PartialEq)]
pub struct DmTurfDamage {
    /// `damaged_dmi`; empty for `null` (the turf shows no damage).
    pub damaged_dmi: String,
    pub broken_states: Vec<String>,
    pub burnt_states: Vec<String>,
    pub burn: TurfBurn,
    /// Mapped broken or burnt (`broken = TRUE` on the type).
    pub broken: bool,
    pub burnt: bool,
}

impl DmTurfDamage {
    /// The states a burn picks from: `burnt_states()`, else `broken_states()`.
    pub fn burn_choices(&self) -> &[String] {
        if self.burnt_states.is_empty() {
            &self.broken_states
        } else {
            &self.burnt_states
        }
    }
}

/// The nearest override of `path`'s chain in `table`.
fn nearest<'a, T>(path: &str, table: &'a [(&str, T)]) -> Option<&'a T> {
    table
        .iter()
        .filter(|(owner, _)| is_under(path, owner))
        .max_by_key(|(owner, _)| owner.len())
        .map(|(_, v)| v)
}

fn states(types: &DmTypes, path: &str, burnt: bool) -> Vec<String> {
    let body = STATES
        .iter()
        .filter(|(owner, b, _)| *b == burnt && is_under(path, owner))
        .max_by_key(|(owner, ..)| owner.len())
        .map(|(_, _, s)| *s);
    let var = |name| {
        types
            .resolved(path, name)
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned()
    };
    match body {
        None => Vec::new(),
        Some(States::List(list)) => list.iter().map(|s| (*s).to_owned()).collect(),
        Some(States::AsteroidDug) => {
            if truthy(types.resolved(path, "dug")) {
                vec![var("icon_state")]
            } else {
                vec![format!("{}_dug", var("base_icon_state"))]
            }
        }
        Some(States::MineralDam) => {
            let state = var("icon_state");
            if state.is_empty() {
                Vec::new()
            } else {
                vec![format!("{state}_dam")]
            }
        }
        Some(States::GrassDamaged) => vec![format!("{}_damaged", var("icon_state"))],
        Some(States::MiscGrass) => {
            let base = if burnt {
                "base_burnt_icon_state"
            } else {
                "base_broken_icon_state"
            };
            vec![format!("{}-255", var(base))]
        }
    }
}

/// The damage looks of `path` as tg computes them.
pub fn damage(types: &DmTypes, path: &str) -> DmTurfDamage {
    let damaged_dmi = match types.resolved(path, "damaged_dmi") {
        Some(serde_json::Value::Object(o)) => o
            .get("resource")
            .and_then(|r| r.as_str())
            .unwrap_or_default()
            .to_owned(),
        Some(serde_json::Value::String(s)) => s.clone(),
        _ => String::new(),
    };
    DmTurfDamage {
        damaged_dmi,
        broken_states: states(types, path, false),
        burnt_states: states(types, path, true),
        burn: nearest(path, BURN_RULES).copied().unwrap_or_default(),
        broken: truthy(types.resolved(path, "broken")),
        burnt: truthy(types.resolved(path, "burnt")),
    }
}

/// Converter fields: open turfs whose looks differ from their parent's carry all of them.
pub fn type_fields(types: &DmTypes, path: &str) -> Vec<Field> {
    if !is_under(path, "/turf/open") {
        return Vec::new();
    }
    let ours = damage(types, path);
    let parent = types
        .types
        .get(path)
        .and_then(|t| t.parent.as_deref())
        .filter(|p| is_under(p, "/turf/open"));
    if parent.is_some_and(|p| damage(types, p) == ours) {
        return Vec::new();
    }
    let list = |v: &Vec<String>| ron::to_string(v).expect("strings serialize");
    vec![
        ext(
            COMPONENT,
            "damaged_dmi",
            ron::to_string(&ours.damaged_dmi).expect("strings serialize"),
        ),
        ext(COMPONENT, "broken_states", list(&ours.broken_states)),
        ext(COMPONENT, "burnt_states", list(&ours.burnt_states)),
        ext(COMPONENT, "burn", format!("{:?}", ours.burn)),
        ext(COMPONENT, "broken", ours.broken.to_string()),
        ext(COMPONENT, "burnt", ours.burnt.to_string()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every `broken_states()`, `burnt_states()` and `burn_tile()` override in the reference
    /// is in the tables (the later definition of a type wins).
    #[test]
    fn source_matches_tables() {
        // `env!` is relative to the workspace (`.cargo/config.toml`).
        let root = std::env::var("SS13_SOURCE").map_or_else(
            |_| {
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../..")
                    .join(env!("SS13_SOURCE"))
            },
            std::path::PathBuf::from,
        );
        if !root.join("code").is_dir() {
            eprintln!("no reference source, skipped");
            return;
        }
        let mut found: std::collections::BTreeMap<(String, String), ()> = Default::default();
        let mut stack = vec![root.join("code"), root.join("modular_bandastation")];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).unwrap().flatten() {
                let p = entry.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().is_some_and(|e| e == "dm") {
                    let text = std::fs::read_to_string(&p).unwrap_or_default();
                    for line in text.lines() {
                        for proc in ["broken_states", "burnt_states", "burn_tile"] {
                            let suffix = format!("/{proc}()");
                            if let Some(owner) = line.strip_suffix(&suffix)
                                && owner.starts_with("/turf")
                                && !owner.contains("/proc")
                            {
                                found.insert((owner.to_owned(), proc.to_owned()), ());
                            }
                        }
                    }
                }
            }
        }
        let mut ours: std::collections::BTreeSet<(String, String)> = STATES
            .iter()
            .map(|(t, burnt, _)| {
                let proc = if *burnt {
                    "burnt_states"
                } else {
                    "broken_states"
                };
                ((*t).to_owned(), proc.to_owned())
            })
            .collect();
        ours.extend(
            BURN_RULES
                .iter()
                .map(|(t, _)| ((*t).to_owned(), "burn_tile".to_owned())),
        );
        // The base definitions are `/turf/open/proc/...` and `/turf/proc/burn_tile`.
        ours.remove(&("/turf/open".to_owned(), "broken_states".to_owned()));
        ours.remove(&("/turf/open".to_owned(), "burnt_states".to_owned()));
        let found: std::collections::BTreeSet<_> = found.into_keys().collect();
        assert_eq!(
            found.difference(&ours).collect::<Vec<_>>(),
            Vec::<&(String, String)>::new(),
            "overrides in the reference missing from the tables"
        );
        assert_eq!(
            ours.difference(&found).collect::<Vec<_>>(),
            Vec::<&(String, String)>::new(),
            "table entries the reference no longer has"
        );
    }

    #[test]
    fn nearest_override_wins() {
        let types = crate::convert::DmTypes::parse(
            br#"{"schema": 3, "types": {
            "/turf": {"parent": null},
            "/turf/open": {"parent": "/turf"},
            "/turf/open/floor": {"parent": "/turf/open",
                "declared_vars": {"damaged_dmi": {"resource": "icons/turf/damaged.dmi"}}},
            "/turf/open/floor/iron": {"parent": "/turf/open/floor"},
            "/turf/open/floor/iron/dark": {"parent": "/turf/open/floor/iron"},
            "/turf/open/floor/carpet": {"parent": "/turf/open/floor"},
            "/turf/open/floor/mineral": {"parent": "/turf/open/floor"},
            "/turf/open/floor/mineral/gold": {"parent": "/turf/open/floor/mineral",
                "declared_vars": {"icon_state": "gold"}}
            }}"#,
        )
        .unwrap();
        let iron = damage(&types, "/turf/open/floor/iron/dark");
        assert_eq!(iron.damaged_dmi, "icons/turf/damaged.dmi");
        assert_eq!(iron.burn_choices(), ["floorscorched1", "floorscorched2"]);
        // A plain floor has no burnt states: a burn picks a broken one.
        assert_eq!(damage(&types, "/turf/open/floor").burn_choices().len(), 5);
        assert_eq!(
            damage(&types, "/turf/open/floor/carpet").burn,
            TurfBurn::Plating
        );
        assert_eq!(
            damage(&types, "/turf/open/floor/mineral/gold").broken_states,
            ["gold_dam"]
        );
        assert!(type_fields(&types, "/turf/open/floor/iron/dark").is_empty());
        assert_eq!(type_fields(&types, "/turf/open/floor/iron").len(), 6);
    }
}
