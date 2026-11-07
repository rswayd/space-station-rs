//! Which `.dmi` files the maps draw, for atlas page order: the client loads pages on demand, so
//! icons drawn together should share pages.
//!
//! A placed object draws its `icon` (var edit, else the type's from `ss13_dm_extract`'s
//! `dm_types.json`), the overlay files its type declares ([`OVERLAY_VARS`]) and, for a spawner,
//! what its spawn list holds. States changed at runtime (smoothing, doors, machines) stay in the
//! same `.dmi`, which is why the unit is a file.
//!
//! Files used on more maps come first: a client plays one station map, and what every station
//! shares is what it draws everywhere.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use rayon::prelude::*;
use serde::Deserialize;
use serde_json::Value;
use thiserror::Error;

use crate::dmm::{DmValue, DmmMap, NO_PREFAB};

/// `.dmi` vars drawn together with the object's `icon` (door lights and panels, window edges).
pub const OVERLAY_VARS: [&str; 2] = ["overlays_file", "edge_overlay_file"];
/// List vars of `/obj/effect/spawner` types whose paths are placed at load (see
/// `ss13_mapload::spawner`).
pub const SPAWN_VARS: [&str; 3] = ["spawn_list", "loot", "items"];
/// Drawn by every client whatever the map: the own mob's body (`ss13_game::human`).
pub const ALWAYS_DRAWN: [&str; 1] = ["icons/mob/human/"];
/// Spawners spawning spawners are followed this deep.
const SPAWN_DEPTH: u32 = 3;

#[derive(Debug, Error)]
pub enum UsageError {
    #[error("{path}: {msg}")]
    Read { path: PathBuf, msg: String },
}

/// How much the maps use one `.dmi`; greater is more used.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Usage {
    /// Map groups (directories under the maps root, e.g. one station) that draw it.
    pub maps: u32,
    /// Objects drawing it over all maps.
    pub placed: u64,
}

/// `.dmi` path (relative to the source root) -> usage.
pub type DmiUsage = HashMap<String, Usage>;

/// The part of `dm_types.json` that decides what a placed type draws.
#[derive(Deserialize)]
pub struct DmTypes {
    types: HashMap<String, Type>,
}

#[derive(Deserialize)]
struct Type {
    parent: Option<String>,
    #[serde(default)]
    vars: Vars,
    #[serde(default)]
    declared_vars: HashMap<String, Value>,
}

#[derive(Deserialize, Default)]
struct Vars {
    #[serde(default)]
    icon: Option<Value>,
}

/// `'file.dmi'` (`{"resource": ...}`) or, rarely, a plain string naming the file.
fn dmi(value: &Value) -> Option<&str> {
    let file = value
        .get("resource")
        .and_then(Value::as_str)
        .or(value.as_str())?;
    file.ends_with(".dmi").then_some(file)
}

/// Type paths in a declared list (`{"list": [{"path": ...} | {"assoc": [{"path": ...}, w]}]}`),
/// nested lists included.
fn paths(value: &Value, out: &mut Vec<String>) {
    let Some(items) = value.get("list").and_then(Value::as_array) else {
        return;
    };
    for item in items {
        let key = item.get("assoc").and_then(|a| a.get(0)).unwrap_or(item);
        match key.get("path").and_then(Value::as_str) {
            Some(path) => out.push(path.to_owned()),
            None => paths(key, out),
        }
    }
}

impl DmTypes {
    pub fn parse(json: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(json)
    }

    /// The first declaration of `var` up the type chain.
    fn declared(&self, path: &str, var: &str) -> Option<&Value> {
        let mut t = self.types.get(path)?;
        loop {
            if let Some(v) = t.declared_vars.get(var) {
                return Some(v);
            }
            t = self.types.get(t.parent.as_deref()?)?;
        }
    }

    /// The GAGS icon key replacing `target` (`"icon"`, `"worn_icon"`, ...) on `path`, if any
    /// ([`crate::gags::CONFIG_VARS`]).
    pub fn gags_key(&self, path: &str, target: &str) -> Option<String> {
        let (var, _) = crate::gags::CONFIG_VARS
            .iter()
            .find(|(_, t)| *t == target)?;
        let colors = self
            .declared(path, "greyscale_colors")
            .and_then(crate::gags::colors)?;
        let config = self
            .declared(path, var)
            .and_then(crate::gags::config_path)?;
        Some(crate::gags::icon_key(config, colors))
    }

    /// Every greyscale config type with its JSON and `.dmi` (inherited ones included), and every
    /// icon key some type uses: what [`crate::import::import_dmi`] renders.
    pub fn gags(&self, source: &Path) -> crate::gags::Gags {
        let file = |path: &str, var: &str| {
            self.declared(path, var)?
                .get("resource")
                .and_then(Value::as_str)
        };
        let configs: Vec<(&str, &str, &str)> = self
            .types
            .keys()
            .filter(|p| p.starts_with("/datum/greyscale_config/"))
            .filter_map(|p| Some((p.as_str(), file(p, "json_config")?, file(p, "icon_file")?)))
            .collect();
        let keys = crate::gags::used_keys(self.types.keys().map(String::as_str), |p, v| {
            self.declared(p, v)
        });
        crate::gags::Gags {
            library: crate::gags::Library::new(source, configs),
            keys,
        }
    }

    /// `.dmi` files an object of `path` draws; `icon` is its var edit, if any. A GAGS type draws
    /// its rendered icon (and worn icon) instead of its `icon`.
    pub fn drawn(&self, path: &str, icon: Option<&str>, out: &mut Vec<String>) {
        self.drawn_at(path, icon, SPAWN_DEPTH, out);
    }

    fn drawn_at(&self, path: &str, icon: Option<&str>, depth: u32, out: &mut Vec<String>) {
        let Some(t) = self.types.get(path) else {
            return;
        };
        if let Some(key) = self.gags_key(path, "icon") {
            out.push(key);
            out.extend(self.gags_key(path, "worn_icon"));
        } else if let Some(file) = icon.or_else(|| t.vars.icon.as_ref().and_then(dmi)) {
            out.push(file.to_owned());
        }
        for var in OVERLAY_VARS {
            if let Some(file) = self.declared(path, var).and_then(dmi) {
                out.push(file.to_owned());
            }
        }
        if depth == 0 || !path.starts_with("/obj/effect/spawner/") {
            return;
        }
        let mut spawned = Vec::new();
        for var in SPAWN_VARS {
            if let Some(list) = self.declared(path, var) {
                paths(list, &mut spawned);
            }
        }
        for p in spawned {
            self.drawn_at(&p, None, depth - 1, out);
        }
    }
}

/// Usage per `.dmi` over `maps`; a map's group is its first directory below `root`.
pub fn map_usage(root: &Path, maps: &[PathBuf], types: &DmTypes) -> Result<DmiUsage, UsageError> {
    let per_map: Vec<HashMap<String, u64>> = maps
        .par_iter()
        .map(|path| {
            let map = DmmMap::from_file(path).map_err(|e| UsageError::Read {
                path: path.clone(),
                msg: e.to_string(),
            })?;
            Ok(count(&map, types))
        })
        .collect::<Result<_, UsageError>>()?;
    let mut total = DmiUsage::new();
    let mut groups: HashMap<&str, HashSet<PathBuf>> = HashMap::new();
    for (path, usage) in maps.iter().zip(&per_map) {
        let group = path
            .strip_prefix(root)
            .ok()
            .and_then(|p| p.components().next())
            .map_or_else(|| path.clone(), |c| PathBuf::from(c.as_os_str()));
        for (dmi, &n) in usage {
            total.entry(dmi.clone()).or_default().placed += n;
            groups.entry(dmi).or_default().insert(group.clone());
        }
    }
    for (dmi, g) in groups {
        total.get_mut(dmi).expect("counted above").maps = g.len() as u32;
    }
    Ok(total)
}

fn count(map: &DmmMap, types: &DmTypes) -> HashMap<String, u64> {
    let mut cells = vec![0u64; map.prefabs.len()];
    for &p in map.grid.iter().filter(|&&p| p != NO_PREFAB) {
        cells[p as usize] += 1;
    }
    let mut usage = HashMap::new();
    let mut drawn = Vec::new();
    for (prefab, &n) in map.prefabs.iter().zip(&cells).filter(|(_, n)| **n > 0) {
        for object in &prefab.objects {
            let icon = match object.var("icon") {
                Some(DmValue::Resource(r)) => Some(r.as_str()),
                _ => None,
            };
            drawn.clear();
            types.drawn(&object.path, icon, &mut drawn);
            drawn.sort_unstable();
            drawn.dedup();
            for file in &drawn {
                *usage.entry(file.clone()).or_default() += n;
            }
        }
    }
    usage
}

/// [`ALWAYS_DRAWN`] files first, then most used, files no map uses last; ties in path order.
pub fn order_by_usage(names: &mut [String], usage: &DmiUsage) {
    let key = |name: &String| {
        let always = ALWAYS_DRAWN.iter().any(|p| name.starts_with(p));
        (always, usage.get(name).copied())
    };
    names.sort_by(|a, b| key(b).cmp(&key(a)).then_with(|| a.cmp(b)));
}

/// All files with extension `ext` under `dir`, sorted.
pub fn find_files(dir: &Path, ext: &str) -> std::io::Result<Vec<PathBuf>> {
    fn walk(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let path = entry?.path();
            if path.is_dir() {
                walk(&path, ext, out)?;
            } else if path.extension().is_some_and(|e| e == ext) {
                out.push(path);
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(dir, ext, &mut out)?;
    out.sort();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TYPES: &str = r#"{"schema": 3, "types": {
        "/turf/floor": {"parent": null, "vars": {"icon": {"resource": "floors.dmi"}}},
        "/obj/lamp": {"parent": null, "vars": {"icon": {"resource": "lamp.dmi"}}},
        "/obj/door": {"parent": null, "vars": {"icon": {"resource": "door.dmi"}},
            "declared_vars": {"overlays_file": {"resource": "door_lights.dmi"},
                              "worn_icon": {"resource": "worn.dmi"}}},
        "/obj/door/red": {"parent": "/obj/door", "vars": {"icon": "red.dmi"}},
        "/obj/effect/spawner/w": {"parent": null, "vars": {"icon": {"resource": "spawner.dmi"}},
            "declared_vars": {"spawn_list": {"list": [{"path": "/obj/lamp"},
                {"assoc": [{"path": "/obj/door/red"}, 1]}]}}},
        "/area/a": {"parent": null, "vars": {"icon": null}}
    }}"#;

    fn drawn(types: &DmTypes, path: &str) -> Vec<String> {
        let mut out = Vec::new();
        types.drawn(path, None, &mut out);
        out
    }

    #[test]
    fn drawn_files_follow_icons_overlays_and_spawn_lists() {
        let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
        assert_eq!(
            drawn(&types, "/obj/door/red"),
            ["red.dmi", "door_lights.dmi"]
        );
        assert_eq!(
            drawn(&types, "/obj/effect/spawner/w"),
            ["spawner.dmi", "lamp.dmi", "red.dmi", "door_lights.dmi"]
        );
        assert!(drawn(&types, "/area/a").is_empty());
        assert!(drawn(&types, "/unknown").is_empty());
    }

    #[test]
    fn counts_cells_per_map_group_and_honors_icon_var_edits() {
        let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
        let dir = std::env::temp_dir().join(format!("ss13_usage_{}", std::process::id()));
        let maps = [dir.join("Meta/meta.dmm"), dir.join("Tram/a/b.dmm")];
        let text = r#""a" = (/obj/lamp,/turf/floor,/area/a)
"b" = (/obj/lamp{icon = 'other.dmi'},/turf/floor,/area/a)
"c" = (/turf/floor,/area/a)

(1,1,1) = {"
aab
"}
"#;
        for m in &maps {
            std::fs::create_dir_all(m.parent().unwrap()).unwrap();
            std::fs::write(m, text).unwrap();
        }
        let usage = map_usage(&dir, &maps, &types).unwrap();
        std::fs::remove_dir_all(&dir).ok();
        let u = |maps, placed| Usage { maps, placed };
        assert_eq!(usage["floors.dmi"], u(2, 6));
        assert_eq!(usage["lamp.dmi"], u(2, 4));
        assert_eq!(usage["other.dmi"], u(2, 2));
        assert_eq!(usage.len(), 3);

        let mut usage = usage;
        usage.insert("b.dmi".into(), u(1, 100));
        let mut names = [
            "b.dmi",
            "lamp.dmi",
            "a.dmi",
            "floors.dmi",
            "icons/mob/human/human.dmi",
            "other.dmi",
        ]
        .map(String::from);
        order_by_usage(&mut names, &usage);
        assert_eq!(
            names,
            [
                "icons/mob/human/human.dmi",
                "floors.dmi",
                "lamp.dmi",
                "other.dmi",
                "b.dmi",
                "a.dmi"
            ]
        );
    }

    /// BandaStation's own configs (`modular_bandastation/objects/code/greyscale/config_types/
    /// greyscale_clothes.dm`) are collected like tg's: the `/worn` subtype inherits the parent's
    /// `json_config` and draws from its own `icon_file`, and a fork item using both gets its
    /// rendered icon and worn icon.
    #[test]
    fn fork_greyscale_configs_render_with_inherited_json() {
        let types = DmTypes::parse(
            br##"{"schema": 3, "types": {
            "/datum/greyscale_config": {"parent": null},
            "/datum/greyscale_config/carnival_formal": {"parent": "/datum/greyscale_config",
                "declared_vars": {
                "icon_file": {"resource": "modular_bandastation/objects/icons/obj/clothing/under/carnival.dmi"},
                "json_config": {"resource": "code/datums/greyscale/json_configs/bandastation/under_carnival_formal.json"}}},
            "/datum/greyscale_config/carnival_formal/worn": {
                "parent": "/datum/greyscale_config/carnival_formal", "declared_vars": {
                "icon_file": {"resource": "modular_bandastation/objects/icons/mob/clothing/under/carnival.dmi"}}},
            "/obj/item/clothing/under/carnival/formal": {"parent": null, "declared_vars": {
                "greyscale_config": {"path": "/datum/greyscale_config/carnival_formal"},
                "greyscale_config_worn": {"path": "/datum/greyscale_config/carnival_formal/worn"},
                "greyscale_colors": "#242424#dddddd"}}
        }}"##,
        )
        .unwrap();
        let path = "/obj/item/clothing/under/carnival/formal";
        let icon = "gags:/datum/greyscale_config/carnival_formal#242424#dddddd";
        let worn = "gags:/datum/greyscale_config/carnival_formal/worn#242424#dddddd";
        assert_eq!(drawn(&types, path), [icon, worn]);
        let gags = types.gags(Path::new("/nonexistent"));
        assert_eq!(gags.library.len(), 2);
        assert!(gags.keys.contains(icon) && gags.keys.contains(worn));
        assert_eq!(
            gags.library.icon_file(worn),
            Some("modular_bandastation/objects/icons/mob/clothing/under/carnival.dmi")
        );
    }
}
