//! Trams and elevators as load data (tg `code/modules/transport/`, `__DEFINES/transport.dm`).
//!
//! [`DmTransport`] carries the transport vars of the linear modules (`/obj/structure/transport`),
//! their landmarks (transport ids, tram nav and platform beacons), the tram controls, call
//! buttons and cabinets, the elevator panels and the doors a transport drives (lift blast doors,
//! elevator windoors, tram airlocks, and any door a map links with `transport_linked_id`). Type
//! defaults and `.dmm` edits; the game (`ss13_game::transport`) builds the transports from it.

use bevy::prelude::*;
use serde_json::Value;
use ss13_assets::dmm::DmValue;

use crate::convert::DmTypes;
use crate::door::link_id;
use crate::fidelity::{ext, is_under, num, truthy};
use crate::value::{Field, Skip, Val};

pub const MODULE_ROOT: &str = "/obj/structure/transport";
pub const LANDMARK_ROOT: &str = "/obj/effect/landmark/transport";
pub const DOOR_ROOT: &str = "/obj/machinery/door";
const COMPONENT: &str = "dm_transport";
const ROOTS: [&str; 9] = [
    MODULE_ROOT,
    LANDMARK_ROOT,
    "/obj/machinery/computer/tram_controls",
    "/obj/machinery/elevator_control_panel",
    "/obj/machinery/button/transport",
    "/obj/machinery/transport",
    "/obj/machinery/door/poddoor/lift",
    "/obj/machinery/door/window/elevator",
    "/obj/machinery/door/airlock/tram",
];

/// The transport vars of a DM type (type defaults + map edits). Prototype data like
/// [`DmAtom`](crate::DmAtom). Absent vars keep DM's `null`/`FALSE`.
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmTransport {
    pub path: String,
    /// Modules: `transport_id` (`TRANSPORT_TYPE_ELEVATOR`, `TRANSPORT_TYPE_TRAM`), what they
    /// merge with.
    pub transport_id: String,
    /// Id landmarks, tram beacons, tram controls and call buttons: `specific_transport_id`.
    pub specific_transport_id: String,
    /// Tram platform beacons: `platform_code` (a number, kept as its text).
    pub platform_code: String,
    /// Elevator panels (and floor indicators): `linked_elevator_id`.
    pub linked_elevator_id: String,
    /// Doors: `transport_linked_id`, `elevator_mode`.
    pub transport_linked_id: String,
    pub elevator_mode: bool,
    /// Tram machinery (`/obj/machinery/transport`): `configured_transport_id`.
    pub configured_transport_id: String,
    /// Modules: `elevator_vertical_speed` (deciseconds), `violent_landing`,
    /// `warns_on_down_movement`, `radial_travel`.
    pub elevator_vertical_speed: f32,
    pub violent_landing: bool,
    pub warns_on_down_movement: bool,
    pub radial_travel: bool,
    /// Elevator panels: `req_access`.
    pub req_access: Vec<String>,
    /// Elevator panels: `preset_destination_names`, `(z as text, name)`.
    pub preset_destination_names: Vec<(String, String)>,
}

#[derive(Clone, Copy)]
enum Kind {
    Id,
    Text,
    Flag,
    Num,
    List,
    Names,
}

const VARS: [(&str, Kind); 13] = [
    ("transport_id", Kind::Text),
    ("specific_transport_id", Kind::Id),
    ("platform_code", Kind::Id),
    ("linked_elevator_id", Kind::Id),
    ("transport_linked_id", Kind::Id),
    ("elevator_mode", Kind::Flag),
    ("configured_transport_id", Kind::Id),
    ("elevator_vertical_speed", Kind::Num),
    ("violent_landing", Kind::Flag),
    ("warns_on_down_movement", Kind::Flag),
    ("radial_travel", Kind::Flag),
    ("req_access", Kind::List),
    ("preset_destination_names", Kind::Names),
];

pub fn is_transport(path: &str) -> bool {
    ROOTS.iter().any(|r| is_under(path, r))
}

fn str_ron(s: &str) -> String {
    ron::to_string(s).expect("strings always serialize")
}

fn json_ron(kind: Kind, v: &Value) -> Option<String> {
    Some(match kind {
        Kind::Id => str_ron(&link_id(Val::from_json(v)).ok()?),
        Kind::Text => str_ron(v.as_str().unwrap_or_default()),
        Kind::Flag => truthy(Some(v)).to_string(),
        Kind::Num => format!("{:?}", num(Some(v))? as f32),
        Kind::List => {
            let items: Vec<String> = match v {
                Value::Null => Vec::new(),
                Value::Object(o) => o
                    .get("list")?
                    .as_array()?
                    .iter()
                    .map(|i| i.as_str().map(str::to_owned))
                    .collect::<Option<_>>()?,
                _ => return None,
            };
            ron::to_string(&items).ok()?
        }
        // Type defaults name no floors (the maps edit them in).
        Kind::Names => "[]".to_owned(),
    })
}

/// Converter fields of `path`: every transport type carries its resolved vars.
pub fn type_fields(types: &DmTypes, path: &str) -> Vec<Field> {
    let mut out = Vec::new();
    if !is_transport(path) || !types.types.contains_key(path) {
        return out;
    }
    out.push(ext(COMPONENT, "path", str_ron(path)));
    for (var, kind) in VARS {
        if let Some(ron) = types.resolved(path, var).and_then(|v| json_ron(kind, v)) {
            out.push(ext(COMPONENT, var, ron));
        }
    }
    out
}

/// `.dmm` edits of the transport vars (and `transport_linked_id`/`elevator_mode` on any door).
pub fn field_dmm(path: &str, var: &str, value: &DmValue) -> Option<Result<Field, Skip>> {
    let door_var = matches!(var, "transport_linked_id" | "elevator_mode");
    if !is_transport(path) && !(door_var && is_under(path, DOOR_ROOT)) {
        return None;
    }
    let &(key, kind) = VARS.iter().find(|(v, _)| *v == var)?;
    // Doors keep their own `req_access` (`crate::door`).
    if matches!(kind, Kind::List) && is_under(path, DOOR_ROOT) {
        return None;
    }
    let ron = match (kind, value) {
        (Kind::Id, v) => link_id(Val::from_dmm(v)).map(|s| str_ron(&s)),
        (Kind::Text, DmValue::Str(s)) => Ok(str_ron(s)),
        (Kind::Text, DmValue::Null) => Ok(str_ron("")),
        (Kind::Flag, DmValue::Number(n)) => Ok((*n != 0.0).to_string()),
        (Kind::Flag, DmValue::Null) => Ok("false".to_owned()),
        (Kind::Num, DmValue::Number(n)) => Ok(format!("{:?}", *n as f32)),
        (Kind::List, DmValue::Null) => Ok("[]".to_owned()),
        (Kind::List, DmValue::List(items)) => items
            .iter()
            .map(|(k, v)| match (k, v) {
                (DmValue::Str(s), None) => Some(s.clone()),
                _ => None,
            })
            .collect::<Option<Vec<String>>>()
            .and_then(|l| ron::to_string(&l).ok())
            .ok_or(Skip::Unsupported),
        (Kind::Names, DmValue::Null) => Ok("[]".to_owned()),
        (Kind::Names, DmValue::List(items)) => items
            .iter()
            .map(|(k, v)| match (k, v) {
                (DmValue::Str(z), Some(DmValue::Str(name))) => Some((
                    z.clone(),
                    name.split_whitespace().collect::<Vec<_>>().join(" "),
                )),
                _ => None,
            })
            .collect::<Option<Vec<(String, String)>>>()
            .and_then(|l| ron::to_string(&l).ok())
            .ok_or(Skip::Unsupported),
        _ => Err(Skip::Unsupported),
    };
    Some(ron.map(|r| ext(COMPONENT, key, r)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_vars_and_map_edits() {
        let types = DmTypes::parse(
            br#"{ "schema": 3, "types": {
                "/atom": { "parent": null },
                "/obj": { "parent": "/atom" },
                "/obj/structure": { "parent": "/obj" },
                "/obj/structure/transport": { "parent": "/obj/structure" },
                "/obj/structure/transport/linear": { "parent": "/obj/structure/transport",
                    "declared_vars": { "transport_id": "icts_elev", "elevator_vertical_speed": 20,
                    "violent_landing": 1, "radial_travel": 1 } },
                "/obj/structure/transport/linear/public": {
                    "parent": "/obj/structure/transport/linear",
                    "declared_vars": { "violent_landing": 0, "radial_travel": 0 } }
            } }"#,
        )
        .unwrap();
        let fields = type_fields(&types, "/obj/structure/transport/linear/public");
        assert!(fields.contains(&ext(COMPONENT, "transport_id", "\"icts_elev\"")));
        assert!(fields.contains(&ext(COMPONENT, "elevator_vertical_speed", "20.0")));
        assert!(fields.contains(&ext(COMPONENT, "violent_landing", "false")));
        assert!(type_fields(&types, "/obj/structure").is_empty());

        let door = "/obj/machinery/door/window/left";
        assert_eq!(
            field_dmm(door, "transport_linked_id", &DmValue::Str("lift".into())),
            Some(Ok(ext(COMPONENT, "transport_linked_id", "\"lift\"")))
        );
        assert_eq!(field_dmm(door, "req_access", &DmValue::Null), None);
        let panel = "/obj/machinery/elevator_control_panel/directional/west";
        let names = DmValue::List(vec![(
            DmValue::Str("2".into()),
            Some(DmValue::Str("Lower      Deck".into())),
        )]);
        assert_eq!(
            field_dmm(panel, "preset_destination_names", &names),
            Some(Ok(ext(
                COMPONENT,
                "preset_destination_names",
                "[(\"2\",\"Lower Deck\")]"
            )))
        );
        let beacon = "/obj/effect/landmark/transport/nav_beacon/tram/platform";
        assert_eq!(
            field_dmm(beacon, "platform_code", &DmValue::Number(2.0)),
            Some(Ok(ext(COMPONENT, "platform_code", "\"2\"")))
        );
        assert_eq!(field_dmm("/obj/item", "transport_id", &DmValue::Null), None);
    }
}
