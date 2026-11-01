//! Construction and integrity vars as prototype data: tools (`tool_behaviour`), stacks
//! (`/obj/item/stack`), turf construction (`baseturfs`, `floor_tile`, wall sheets and girders) and
//! structure/machine integrity, anchoring and construction state (`code/game/atom/atom_defense.dm`,
//! `code/game/objects/structures/{girders,window,grille}.dm`, `code/game/turfs/closed/walls.dm`).
//!
//! Every type of a family carries its component with all fields resolved (inherited values
//! included), so a prototype never depends on which ancestors were converted.

use bevy::prelude::*;

use crate::convert::DmTypes;
use crate::fidelity::{ext, is_under};
use crate::value::{Field, Skip, Val};

pub const TOOL_COMPONENT: &str = "dm_tool";
pub const STACK_COMPONENT: &str = "dm_stack";
pub const TURF_COMPONENT: &str = "dm_turf_build";
pub const OBJ_COMPONENT: &str = "dm_obj";

/// Types the construction code spawns or turns turfs into, so `ss13-mapload convert` emits them
/// even when no converted map uses them. Fixtures convert without them; the game then skips
/// what has no prototype.
pub const CONSTRUCTION_TYPES: [&str; 12] = [
    "/turf/open/floor/plating",
    "/turf/open/floor/iron",
    "/turf/closed/wall",
    "/obj/structure/girder",
    "/obj/structure/girder/displaced",
    "/obj/structure/grille",
    "/obj/structure/window/fulltile",
    "/obj/item/stack/sheet/iron",
    "/obj/item/stack/sheet/glass",
    "/obj/item/stack/rods",
    "/obj/item/stack/tile/iron/base",
    "/obj/item/shard",
];

/// `tool_behaviour` and its speed (`/obj/item`). Only on items whose `tool_behaviour` is set.
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmTool {
    /// `TOOL_*` text: `"wrench"`, `"screwdriver"`, `"crowbar"`, `"welder"`, `"cutters"`,
    /// `"multitool"`, ...
    pub behaviour: String,
    /// `toolspeed`: multiplies every `use_tool` delay.
    pub speed: f32,
    /// Welders: `max_fuel` (they start full, `starting_fuel`).
    pub max_fuel: f32,
    /// `heat` (welders 3800).
    pub heat: f32,
}

/// `/obj/item/stack` vars.
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmStack {
    /// The amount a new stack of this type has (map edits override it).
    pub amount: u32,
    pub max_amount: u32,
    /// Stacks merge only with stacks of the same `merge_type` (empty: the type itself).
    pub merge_type: String,
    pub singular_name: String,
    /// Tiles: the turf they place (`/obj/item/stack/tile::turf_type`).
    pub turf_type: String,
}

/// Turf construction vars (`/turf`).
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmTurfBuild {
    /// The DM type.
    pub path: String,
    /// `baseturfs` (a single path; lists are not supported and leave it empty).
    pub baseturf: String,
    /// `/turf/open/floor::floor_tile`: what prying the tile drops.
    pub floor_tile: String,
    pub overfloor_placed: bool,
    /// Walls: `sheet_type`, `sheet_amount`, `girder_type`, `slicing_duration` (deciseconds).
    pub sheet_type: String,
    pub sheet_amount: u32,
    pub girder_type: String,
    pub slicing_duration: f32,
}

/// Integrity, anchoring and construction vars of `/obj/structure`, `/obj/machinery` and
/// `/obj/vehicle`.
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmObj {
    pub path: String,
    pub anchored: bool,
    pub max_integrity: f32,
    /// Fraction of `max_integrity` at which `atom_break` runs (0: never).
    pub integrity_failure: f32,
    /// Melee damage below this is ignored.
    pub damage_deflection: f32,
    /// `armor_type`'s `melee` rating, percent.
    pub armor_melee: f32,
    pub obj_flags: u32,
    pub resistance_flags: u32,
    /// Girders and windows: `state` (`GIRDER_*`, `WINDOW_*`).
    pub state: i32,
    /// Girders: `stack_type`, `stack_amount`, `can_displace`.
    pub stack_type: String,
    pub stack_amount: u32,
    pub can_displace: bool,
    /// Windows: `reinf`, `fulltile`, `glass_type`, `glass_amount`, `decon_speed` (deciseconds).
    pub reinf: bool,
    pub fulltile: bool,
    pub glass_type: String,
    pub glass_amount: u32,
    pub decon_speed: f32,
    /// Grilles: `rods_type`, `rods_amount`; `broken` (pre-broken map grilles).
    pub rods_type: String,
    pub rods_amount: u32,
    pub broken: bool,
}

#[derive(Clone, Copy)]
enum Kind {
    Text,
    Path,
    Num,
    Count,
    Int,
    Bool,
}

const TOOL_VARS: [(&str, &str, Kind); 4] = [
    ("tool_behaviour", "behaviour", Kind::Text),
    ("toolspeed", "speed", Kind::Num),
    ("max_fuel", "max_fuel", Kind::Num),
    ("heat", "heat", Kind::Num),
];

const STACK_VARS: [(&str, &str, Kind); 5] = [
    ("amount", "amount", Kind::Count),
    ("max_amount", "max_amount", Kind::Count),
    ("merge_type", "merge_type", Kind::Path),
    ("singular_name", "singular_name", Kind::Text),
    ("turf_type", "turf_type", Kind::Path),
];

const TURF_VARS: [(&str, &str, Kind); 7] = [
    ("baseturfs", "baseturf", Kind::Path),
    ("floor_tile", "floor_tile", Kind::Path),
    ("overfloor_placed", "overfloor_placed", Kind::Bool),
    ("sheet_type", "sheet_type", Kind::Path),
    ("sheet_amount", "sheet_amount", Kind::Count),
    ("girder_type", "girder_type", Kind::Path),
    ("slicing_duration", "slicing_duration", Kind::Num),
];

const OBJ_VARS: [(&str, &str, Kind); 19] = [
    ("anchored", "anchored", Kind::Bool),
    ("max_integrity", "max_integrity", Kind::Num),
    ("integrity_failure", "integrity_failure", Kind::Num),
    ("damage_deflection", "damage_deflection", Kind::Num),
    ("obj_flags", "obj_flags", Kind::Count),
    ("resistance_flags", "resistance_flags", Kind::Count),
    ("state", "state", Kind::Int),
    ("stack_type", "stack_type", Kind::Path),
    ("stack_amount", "stack_amount", Kind::Count),
    ("can_displace", "can_displace", Kind::Bool),
    ("reinf", "reinf", Kind::Bool),
    ("fulltile", "fulltile", Kind::Bool),
    ("glass_type", "glass_type", Kind::Path),
    ("glass_amount", "glass_amount", Kind::Count),
    ("decon_speed", "decon_speed", Kind::Num),
    ("rods_type", "rods_type", Kind::Path),
    ("rods_amount", "rods_amount", Kind::Count),
    ("broken", "broken", Kind::Bool),
    ("armor_type", "armor_melee", Kind::Num),
];

/// A dumped value as a number: plain numbers and constant expressions the extractor kept as
/// text (`{"expr": "50"}`); null is 0.
fn number(v: Option<&serde_json::Value>) -> Option<f64> {
    match v {
        None | Some(serde_json::Value::Null) => Some(0.0),
        Some(serde_json::Value::Number(n)) => n.as_f64(),
        Some(serde_json::Value::Object(o)) => o.get("expr")?.as_str()?.trim().parse().ok(),
        Some(_) => None,
    }
}

/// A dumped type path (`{"path": ...}` or a constant `{"expr": "/a/b"}`); null is empty.
fn type_path(v: Option<&serde_json::Value>) -> Option<String> {
    match v {
        None | Some(serde_json::Value::Null) => Some(String::new()),
        Some(serde_json::Value::Object(o)) => {
            let p = o.get("path").or_else(|| o.get("expr"))?.as_str()?;
            p.starts_with('/').then(|| p.to_owned())
        }
        Some(_) => None,
    }
}

fn ron_str(s: &str) -> String {
    ron::to_string(s).expect("strings always serialize")
}

fn value_ron(kind: Kind, v: Option<&serde_json::Value>) -> Option<String> {
    Some(match kind {
        Kind::Text => match v {
            Some(serde_json::Value::String(s)) => ron_str(s),
            None | Some(serde_json::Value::Null) => ron_str(""),
            _ => return None,
        },
        Kind::Path => ron_str(&type_path(v)?),
        Kind::Num => format!("{:?}", number(v)? as f32),
        Kind::Count => (number(v)?.max(0.0) as u32).to_string(),
        Kind::Int => (number(v)? as i32).to_string(),
        Kind::Bool => (number(v)? != 0.0).to_string(),
    })
}

fn fields(
    types: &DmTypes,
    path: &str,
    component: &'static str,
    vars: &[(&'static str, &'static str, Kind)],
    out: &mut Vec<Field>,
) {
    for &(var, key, kind) in vars {
        let value = if var == "armor_type" {
            // The rating of the armor datum, not the path.
            let armor = type_path(types.resolved(path, var)).unwrap_or_default();
            let melee = number(types.resolved(&armor, "melee")).unwrap_or(0.0);
            Some(format!("{:?}", melee as f32))
        } else {
            value_ron(kind, types.resolved(path, var))
        };
        if let Some(ron) = value {
            out.push(ext(component, key, ron));
        }
    }
}

/// Converter fields of `path`.
pub fn type_fields(types: &DmTypes, path: &str) -> Vec<Field> {
    let mut out = Vec::new();
    if !types.types.contains_key(path) {
        return out;
    }
    if is_under(path, "/obj/item") {
        let behaviour = types.resolved(path, "tool_behaviour");
        if behaviour.is_some_and(|b| b.is_string()) {
            fields(types, path, TOOL_COMPONENT, &TOOL_VARS, &mut out);
        }
        if is_under(path, "/obj/item/stack") {
            fields(types, path, STACK_COMPONENT, &STACK_VARS, &mut out);
        }
    } else if is_under(path, "/turf") {
        out.push(ext(TURF_COMPONENT, "path", ron_str(path)));
        fields(types, path, TURF_COMPONENT, &TURF_VARS, &mut out);
    } else if is_under(path, "/obj/structure")
        || is_under(path, "/obj/machinery")
        || is_under(path, "/obj/vehicle")
    {
        out.push(ext(OBJ_COMPONENT, "path", ron_str(path)));
        fields(types, path, OBJ_COMPONENT, &OBJ_VARS, &mut out);
    }
    out
}

/// `.dmm` var edits: a stack's `amount`.
pub fn ext_field(path: &str, var: &str, value: Val) -> Result<Field, Skip> {
    if var != "amount" || !is_under(path, "/obj/item/stack") {
        return Err(Skip::UnknownVar);
    }
    match value {
        Val::Num(n) => Ok(ext(
            STACK_COMPONENT,
            "amount",
            (n.max(0.0) as u32).to_string(),
        )),
        _ => Err(Skip::Unsupported),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TYPES: &str = r##"{
      "schema": 3,
      "types": {
        "/datum": { "parent": null },
        "/datum/armor": { "parent": "/datum", "declared_vars": { "melee": { "expr": "0" } } },
        "/datum/armor/structure_window": { "parent": "/datum/armor", "declared_vars": {
            "melee": { "expr": "50" } } },
        "/atom": { "parent": "/datum", "declared_vars": { "max_integrity": 500,
            "armor_type": { "expr": "/datum/armor" }, "integrity_failure": 0,
            "damage_deflection": 0, "resistance_flags": 0, "baseturfs": null } },
        "/obj": { "parent": "/atom", "declared_vars": { "obj_flags": 2, "anchored": 0 } },
        "/obj/item": { "parent": "/obj", "declared_vars": { "tool_behaviour": null,
            "toolspeed": 1, "heat": 0 } },
        "/obj/item/wrench": { "parent": "/obj/item", "declared_vars": {
            "tool_behaviour": "wrench" } },
        "/obj/item/weldingtool": { "parent": "/obj/item", "declared_vars": {
            "tool_behaviour": "welder", "max_fuel": 20, "heat": 3800 } },
        "/obj/item/stack": { "parent": "/obj/item", "declared_vars": { "amount": 1,
            "max_amount": 50, "merge_type": null } },
        "/obj/item/stack/rods": { "parent": "/obj/item/stack", "declared_vars": {
            "merge_type": { "path": "/obj/item/stack/rods" }, "singular_name": "iron rod" } },
        "/obj/structure": { "parent": "/obj", "declared_vars": { "anchored": 1,
            "max_integrity": 300 } },
        "/obj/structure/window": { "parent": "/obj/structure", "declared_vars": {
            "armor_type": { "expr": "/datum/armor/structure_window" }, "max_integrity": 50,
            "state": 0, "reinf": 0, "glass_type": { "path": "/obj/item/stack/sheet/glass" },
            "glass_amount": 1, "decon_speed": 30 } },
        "/turf": { "parent": "/atom", "declared_vars": {
            "baseturfs": { "path": "/turf/baseturf_bottom" } } },
        "/turf/open": { "parent": "/turf" },
        "/turf/open/floor": { "parent": "/turf/open", "declared_vars": {
            "baseturfs": { "path": "/turf/open/floor/plating" }, "overfloor_placed": 1 } }
      }
    }"##;

    #[test]
    fn construction_vars_resolve_through_inheritance() {
        let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
        let ron = |path| crate::value::patch_ron(&type_fields(&types, path));
        assert_eq!(ron("/obj/item"), "{}", "no tool, no stack");
        assert_eq!(
            ron("/obj/item/weldingtool"),
            r#"{"dm_tool": (behaviour: "welder", speed: 1.0, max_fuel: 20.0, heat: 3800.0)}"#
        );
        assert!(ron("/obj/item/stack/rods").contains(
            r#""dm_stack": (amount: 1, max_amount: 50, merge_type: "/obj/item/stack/rods", singular_name: "iron rod", turf_type: "")"#
        ));
        let window = ron("/obj/structure/window");
        assert!(window.contains("max_integrity: 50.0"), "{window}");
        assert!(window.contains("armor_melee: 50.0"), "{window}");
        assert!(window.contains("anchored: true"), "{window}");
        assert!(window.contains(r#"glass_type: "/obj/item/stack/sheet/glass""#));
        let floor = ron("/turf/open/floor");
        assert!(
            floor.contains(r#"baseturf: "/turf/open/floor/plating""#),
            "{floor}"
        );
        assert!(floor.contains("overfloor_placed: true"));
        assert_eq!(
            ext_field("/obj/item/stack/rods", "amount", Val::Num(20.0)),
            Ok(ext(STACK_COMPONENT, "amount", "20"))
        );
        assert_eq!(
            ext_field("/obj/structure/window", "amount", Val::Num(20.0)),
            Err(Skip::UnknownVar)
        );
    }
}
