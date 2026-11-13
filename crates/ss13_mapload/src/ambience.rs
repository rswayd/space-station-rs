//! Area ambience (tg `SSambience`, `/area/proc/play_ambience`, `code/_globalvars/lists/ambience.dm`)
//! for [`crate::DmArea`]: every area's `ambientsounds` as `/area/Initialize` leaves them (its own
//! list, else `GLOB.ambience_assoc[ambience_index]`), the buzz loop, its volume and the cooldowns.
//! The ambience lists come from the dumped globals ([`AMBIENCE_ASSOC`]).

use std::collections::BTreeSet;

use serde_json::Value;

use crate::convert::DmTypes;
use crate::fidelity::{ext, is_under, num};
use crate::value::Field;

/// tg `GLOB.ambience_assoc`: `AMBIENCE_*` key -> the global list it names
/// (`ss13_dm_extract::DUMPED_GLOBALS`).
pub const AMBIENCE_ASSOC: [(&str, &str); 15] = [
    ("away", "away_ambience"),
    ("creepy", "creepy_ambience"),
    ("danger", "danger_ambience"),
    ("engi", "engi_ambience"),
    ("generic", "generic_ambience"),
    ("holy", "holy_ambience"),
    ("maint", "maint_ambience"),
    ("med", "medical_ambience"),
    ("viro", "virology_ambience"),
    ("mining", "mining_ambience"),
    ("icemoon", "icemoon_ambience"),
    ("reebe", "reebe_ambience"),
    ("ruins", "ruins_ambience"),
    ("space", "space_ambience"),
    ("spooky", "spooky_ambience"),
];

/// `/area/station/maintenance`'s `minecraft_cave_noises`: a rare sound in the dark.
pub const MAINT_CAVE_NOISES: [&str; 10] = [
    "sound/machines/airlock/airlock.ogg",
    "sound/effects/snap.ogg",
    "sound/effects/footstep/clownstep1.ogg",
    "sound/effects/footstep/clownstep2.ogg",
    "sound/items/tools/welder.ogg",
    "sound/items/tools/welder2.ogg",
    "sound/items/tools/crowbar.ogg",
    "sound/items/deconstruct.ogg",
    "sound/ambience/misc/source_holehit3.ogg",
    "sound/ambience/misc/cavesound3.ogg",
];

const AREA: &str = "/area";
const COMPONENT: &str = "dm_area";

fn files(v: &Value) -> Vec<String> {
    v.get("list")
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice)
        .iter()
        .filter_map(|f| match f {
            Value::String(s) => Some(s.clone()),
            f => f.get("resource").and_then(Value::as_str).map(str::to_owned),
        })
        .collect()
}

/// `GLOB.ambience_assoc[key]`; empty for an unknown key or a dump without the globals.
pub fn ambience_list(types: &DmTypes, key: &str) -> Vec<String> {
    AMBIENCE_ASSOC
        .iter()
        .find(|(k, _)| *k == key)
        .and_then(|(_, global)| types.globals.get(*global))
        .map(files)
        .unwrap_or_default()
}

/// `ambientsounds` of the area type after `/area/Initialize`: its own list (an empty list stays
/// empty: a DM list is true), else the list of its `ambience_index`.
pub fn ambientsounds(types: &DmTypes, path: &str) -> Vec<String> {
    match types.resolved(path, "ambientsounds") {
        Some(v @ Value::Object(_)) => files(v),
        _ => {
            let key = types
                .resolved(path, "ambience_index")
                .and_then(Value::as_str)
                .unwrap_or("generic");
            ambience_list(types, key)
        }
    }
}

fn resource(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.clone(),
        Some(v) => v
            .get("resource")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        None => String::new(),
    }
}

fn ron_str(s: &str) -> String {
    ron::to_string(s).expect("strings always serialize")
}

/// The area's ambience fields: the root carries every var resolved, subtypes what they declare,
/// and `ambientsounds` where it differs from the parent's.
pub fn type_fields(types: &DmTypes, path: &str) -> Vec<Field> {
    let mut out = Vec::new();
    let Some(ty) = types.types.get(path) else {
        return out;
    };
    if !is_under(path, AREA) {
        return out;
    }
    let root = path == AREA;
    let var = |name: &'static str| {
        if root {
            types.resolved(path, name)
        } else {
            ty.declared_vars.get(name)
        }
    };
    if let Some(v) = var("ambient_buzz") {
        out.push(ext(COMPONENT, "ambient_buzz", ron_str(&resource(Some(v)))));
    }
    if let Some(n) = var("ambient_buzz_vol").and_then(|v| num(Some(v))) {
        out.push(ext(
            COMPONENT,
            "ambient_buzz_vol",
            format!("{:?}", n as f32),
        ));
    }
    // Deciseconds in DM, seconds here.
    for name in ["min_ambience_cooldown", "max_ambience_cooldown"] {
        if let Some(n) = var(name).and_then(|v| num(Some(v))) {
            out.push(ext(COMPONENT, name, format!("{:?}", (n / 10.0) as f32)));
        }
    }
    if let Some(n) = var("forced_ambience").and_then(|v| num(Some(v))) {
        out.push(ext(COMPONENT, "forced_ambience", (n != 0.0).to_string()));
    }
    if let Some(n) = var("sound_environment").and_then(|v| num(Some(v))) {
        out.push(ext(COMPONENT, "sound_environment", (n as i32).to_string()));
    }
    let own = ambientsounds(types, path);
    let parent = ty
        .parent
        .as_deref()
        .filter(|p| is_under(p, AREA))
        .map(|p| ambientsounds(types, p));
    if root || parent.as_ref() != Some(&own) {
        let ron = ron::to_string(&own).expect("string lists always serialize");
        out.push(ext(COMPONENT, "ambientsounds", ron));
    }
    out
}

/// Every ambience file the server can name: the ambience lists, every area type's own
/// `ambientsounds` and `ambient_buzz`, the maintenance cave noises.
pub fn referenced(types: &DmTypes) -> BTreeSet<String> {
    let mut out: BTreeSet<String> = AMBIENCE_ASSOC
        .iter()
        .flat_map(|(key, _)| ambience_list(types, key))
        .collect();
    out.extend(MAINT_CAVE_NOISES.map(str::to_owned));
    for (path, ty) in types.types.iter().filter(|(p, _)| is_under(p, AREA)) {
        if let Some(v) = ty.declared_vars.get("ambientsounds") {
            out.extend(files(v));
        }
        let buzz = resource(types.resolved(path, "ambient_buzz"));
        if !buzz.is_empty() {
            out.insert(buzz);
        }
    }
    out.retain(|f| f.ends_with(".ogg"));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::convert::DmTypes;

    const DUMP: &str = r#"{
      "schema": 3,
      "types": {
        "/datum": { "parent": null },
        "/atom": { "parent": "/datum" },
        "/area": { "parent": "/atom", "declared_vars": {
            "ambience_index": "generic", "ambientsounds": null,
            "ambient_buzz": { "resource": "sound/ambience/general/shipambience.ogg" },
            "ambient_buzz_vol": 35, "min_ambience_cooldown": 40, "max_ambience_cooldown": 100,
            "forced_ambience": 0, "sound_environment": -1 } },
        "/area/station": { "parent": "/area" },
        "/area/station/maintenance": { "parent": "/area/station", "declared_vars": {
            "ambience_index": "maint", "forced_ambience": 1,
            "ambient_buzz": { "resource": "sound/ambience/maintenance/source_corridor2.ogg" } } },
        "/area/station/command": { "parent": "/area/station", "declared_vars": {
            "ambientsounds": { "list": [ { "resource": "sound/ambience/misc/signal.ogg" } ] } } },
        "/area/station/command/bridge": { "parent": "/area/station/command", "declared_vars": {
            "ambience_index": "maint" } },
        "/area/space": { "parent": "/area", "declared_vars": { "ambient_buzz": null,
            "ambientsounds": { "list": [] } } }
      },
      "globals": {
        "generic_ambience": { "list": [ { "resource": "sound/ambience/general/ambigen1.ogg" } ] },
        "maint_ambience": { "list": [ { "resource": "sound/ambience/maintenance/ambimaint1.ogg" } ] }
      }
    }"#;

    fn types() -> DmTypes {
        DmTypes::parse(DUMP.as_bytes()).unwrap()
    }

    #[test]
    fn ambientsounds_follow_area_initialize() {
        let types = types();
        assert_eq!(
            ambientsounds(&types, "/area/station"),
            ["sound/ambience/general/ambigen1.ogg"]
        );
        assert_eq!(
            ambientsounds(&types, "/area/station/maintenance"),
            ["sound/ambience/maintenance/ambimaint1.ogg"]
        );
        // An inherited own list wins over the subtype's `ambience_index`.
        assert_eq!(
            ambientsounds(&types, "/area/station/command/bridge"),
            ["sound/ambience/misc/signal.ogg"]
        );
        assert!(ambientsounds(&types, "/area/space").is_empty());
    }

    #[test]
    fn fields_and_referenced_files() {
        let types = types();
        let keys = |path| -> Vec<&'static str> {
            type_fields(&types, path)
                .into_iter()
                .filter_map(|f| match f {
                    Field::Ext { key, .. } => Some(key),
                    _ => None,
                })
                .collect()
        };
        assert_eq!(
            keys("/area"),
            [
                "ambient_buzz",
                "ambient_buzz_vol",
                "min_ambience_cooldown",
                "max_ambience_cooldown",
                "forced_ambience",
                "sound_environment",
                "ambientsounds",
            ]
        );
        assert!(keys("/area/station").is_empty());
        assert_eq!(
            keys("/area/station/maintenance"),
            ["ambient_buzz", "forced_ambience", "ambientsounds"]
        );
        assert!(keys("/area/station/command/bridge").is_empty());
        let files = referenced(&types);
        for f in [
            "sound/ambience/general/ambigen1.ogg",
            "sound/ambience/maintenance/ambimaint1.ogg",
            "sound/ambience/misc/signal.ogg",
            "sound/ambience/general/shipambience.ogg",
            "sound/ambience/maintenance/source_corridor2.ogg",
            "sound/ambience/misc/cavesound3.ogg",
        ] {
            assert!(files.contains(f), "{f}");
        }
    }
}
