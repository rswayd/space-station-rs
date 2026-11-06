//! Docking ports and shuttle templates as load data (tg `code/modules/shuttle/shuttle.dm`,
//! `stationary_port/`, `/datum/map_template/shuttle`, `_maps/map_jsons/*.json`).
//!
//! [`DmDockingPort`] carries the vars of `/obj/docking_port` and its subtypes (type defaults and
//! `.dmm` edits of `shuttle_id`, `json_key`, `width`, `height`, `dwidth`, `dheight`; `dir` and
//! `name` come through [`crate::DmAtom`] and [`crate::DmName`]). [`ShuttleTemplates`] lists the
//! shuttle `.dmm` files the game wants loaded with the station: [`load_maps`](crate::load_maps)
//! puts each on its own z-level after the station's, as tg loads a template into a reservation
//! before docking it.

use std::path::{Path, PathBuf};

use bevy::prelude::*;

use crate::convert::DmTypes;
use crate::fidelity::{ext, is_under, num};
use crate::value::{Field, Skip, Val};

pub const DOCKING_PORT_ROOT: &str = "/obj/docking_port";
pub const MOBILE_PORT_ROOT: &str = "/obj/docking_port/mobile";
pub const STATIONARY_PORT_ROOT: &str = "/obj/docking_port/stationary";
/// tg `/turf/baseturf_skipover/shuttle`: the baseturf a shuttle template's turfs get under their
/// own layers (`/datum/map_template/shuttle/dispatch`); a shuttle move takes what is above it.
pub const SHUTTLE_SKIPOVER: &str = "/turf/baseturf_skipover/shuttle";
/// Every `/turf/baseturf_skipover` is skipped by `ScrapeAway`.
pub const SKIPOVER_ROOT: &str = "/turf/baseturf_skipover";

const COMPONENT: &str = "dm_docking_port";

/// The vars of a docking port (tg `/obj/docking_port`). Prototype data like
/// [`DmAtom`](crate::DmAtom); the game registers the ports from it.
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmDockingPort {
    pub path: String,
    /// Empty = `null`.
    pub shuttle_id: String,
    /// A stationary port's `json_key`: the map json's shuttle loaded on it at round start.
    pub json_key: String,
    pub width: i32,
    pub height: i32,
    pub dwidth: i32,
    pub dheight: i32,
}

impl DmDockingPort {
    pub fn mobile(&self) -> bool {
        is_under(&self.path, MOBILE_PORT_ROOT)
    }
}

#[derive(Clone, Copy)]
enum Kind {
    Text,
    Num,
}

const VARS: [(&str, Kind); 6] = [
    ("shuttle_id", Kind::Text),
    ("json_key", Kind::Text),
    ("width", Kind::Num),
    ("height", Kind::Num),
    ("dwidth", Kind::Num),
    ("dheight", Kind::Num),
];

/// Converter fields of `path`: every docking port type carries all its resolved vars.
pub fn type_fields(types: &DmTypes, path: &str) -> Vec<Field> {
    let mut out = Vec::new();
    if !is_under(path, DOCKING_PORT_ROOT) || !types.types.contains_key(path) {
        return out;
    }
    out.push(ext(COMPONENT, "path", str_ron(path)));
    for (var, kind) in VARS {
        let value = types.resolved(path, var);
        let ron = match kind {
            Kind::Text => Some(str_ron(value.and_then(|v| v.as_str()).unwrap_or(""))),
            Kind::Num => num(value).map(|n| (n as i32).to_string()),
        };
        if let Some(ron) = ron {
            out.push(ext(COMPONENT, var, ron));
        }
    }
    out
}

/// `.dmm` var edits of docking port vars.
pub fn ext_field(path: &str, var: &str, value: Val) -> Result<Field, Skip> {
    let Some(&(key, kind)) = VARS.iter().find(|(v, _)| *v == var) else {
        return Err(Skip::UnknownVar);
    };
    if !is_under(path, DOCKING_PORT_ROOT) {
        return Err(Skip::UnknownVar);
    }
    let ron = match (kind, value) {
        (Kind::Text, Val::Str(s)) => str_ron(s),
        (Kind::Text, Val::Null) => str_ron(""),
        (Kind::Num, Val::Num(n)) => (n as i32).to_string(),
        (Kind::Num, Val::Null) => "0".to_owned(),
        _ => return Err(Skip::Unsupported),
    };
    Ok(ext(COMPONENT, key, ron))
}

fn str_ron(s: &str) -> String {
    ron::to_string(s).expect("strings always serialize")
}

/// Shuttle template `.dmm` files to load with the station, each onto its own new z-level after
/// the station maps (in this order). Insert before the map loads (`PreStartup`).
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct ShuttleTemplates(pub Vec<PathBuf>);

/// tg `SSmapping.current_map.shuttles[key]` (`_maps/map_jsons/*.json`, `"shuttles"`): the
/// template id (`emergency_meta`) of the map named `map` (its `map_name` or `map_file` stem).
/// `None` when no json describes the map or it names no such shuttle.
pub fn map_shuttle(source: &Path, map: &str, key: &str) -> Option<String> {
    map_json(source, map)?
        .get("shuttles")?
        .get(key)?
        .as_str()
        .map(str::to_owned)
}

/// The map json (`_maps/map_jsons/*.json`, tg `/datum/map_config`) whose `map_name` or
/// `map_file` stem is `map`.
pub fn map_json(source: &Path, map: &str) -> Option<serde_json::Value> {
    let dir = source.join("_maps/map_jsons");
    let mut entries: Vec<PathBuf> = ss13_data::read_dir(&dir)
        .ok()?
        .into_iter()
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    entries.sort();
    entries.into_iter().find_map(|path| {
        let json: serde_json::Value = serde_json::from_slice(&ss13_data::read(&path).ok()?).ok()?;
        let named = json.get("map_name").and_then(|v| v.as_str()) == Some(map)
            || json
                .get("map_file")
                .and_then(|v| v.as_str())
                .is_some_and(|f| f.trim_end_matches(".dmm") == map);
        named.then_some(json)
    })
}

/// tg `/datum/map_template/shuttle` `mappath`: `[prefix][port_id]_[suffix].dmm`; the prefix is
/// `_maps/shuttles/<port_id>/` (emergency, labour), or the folder of the port id's first word
/// (`mining_common` templates live in `_maps/shuttles/mining/`).
pub fn shuttle_template_path(source: &Path, key: &str, id: &str) -> PathBuf {
    let first = key.split('_').next().unwrap_or(key);
    [key, first]
        .into_iter()
        .map(|dir| source.join(format!("_maps/shuttles/{dir}/{id}.dmm")))
        .find(|p| ss13_data::exists(p))
        .unwrap_or_else(|| source.join(format!("_maps/shuttles/{id}.dmm")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vars_and_edits() {
        let types = DmTypes::parse(
            br#"{ "schema": 3, "types": {
                "/atom": { "parent": null },
                "/obj": { "parent": "/atom" },
                "/obj/docking_port": { "parent": "/obj", "declared_vars": { "shuttle_id": null,
                    "width": 0, "height": 0, "dwidth": 0, "dheight": 0 } },
                "/obj/docking_port/stationary": { "parent": "/obj/docking_port",
                    "declared_vars": { "json_key": null } },
                "/obj/docking_port/mobile/emergency": { "parent": "/obj/docking_port",
                    "declared_vars": { "shuttle_id": "emergency" } }
            } }"#,
        )
        .unwrap();
        let fields = type_fields(&types, "/obj/docking_port/mobile/emergency");
        assert!(fields.contains(&ext(COMPONENT, "shuttle_id", "\"emergency\"")));
        assert!(fields.contains(&ext(COMPONENT, "width", "0")));
        assert!(type_fields(&types, "/obj").is_empty());
        let p = "/obj/docking_port/stationary";
        assert_eq!(
            ext_field(p, "dwidth", Val::Num(9.0)),
            Ok(ext(COMPONENT, "dwidth", "9"))
        );
        assert_eq!(
            ext_field(p, "shuttle_id", Val::Str("emergency_home")),
            Ok(ext(COMPONENT, "shuttle_id", "\"emergency_home\""))
        );
        assert_eq!(
            ext_field("/obj/item", "width", Val::Num(1.0)),
            Err(Skip::UnknownVar)
        );
    }

    #[test]
    fn metastation_emergency_template() {
        let source = Path::new(env!("SS13_SOURCE"));
        if !source.exists() {
            return;
        }
        let id = map_shuttle(source, "MetaStation", "emergency").unwrap();
        assert_eq!(id, "emergency_meta");
        assert!(shuttle_template_path(source, "emergency", &id).exists());
        assert_eq!(map_shuttle(source, "NoSuchStation", "emergency"), None);
        assert!(
            shuttle_template_path(source, "mining_common", "mining_common_meta")
                .ends_with("_maps/shuttles/mining/mining_common_meta.dmm")
        );
        assert!(shuttle_template_path(source, "labour", "labour_delta").exists());
    }
}
