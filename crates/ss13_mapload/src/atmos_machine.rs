//! Atmos machinery as prototype data: the vars tg pipes, atmos components, portable machines
//! and air alarms read (`code/modules/atmospherics/machinery/`), and the air alarm mapping
//! helpers. Every type of those families carries all its resolved vars, so a subtype never
//! depends on its parent's patch; the game builds pipe networks and machines from it once.
//!
//! Map var edits of the scalar vars below are supported, and of `filter_type`/`filter_types`
//! and `req_access`/`req_one_access` (lists). Gas lists are stored as tg gas ids (`n2`).

use bevy::prelude::*;
use serde_json::Value;
use ss13_assets::dmm::DmValue;

use crate::convert::DmTypes;
use crate::fidelity::{ext, is_under, num, truthy};
use crate::value::{Field, Skip, Val};

const COMPONENT: &str = "dm_atmos_device";

pub const ATMOS_ROOT: &str = "/obj/machinery/atmospherics";
pub const PORTABLE_ROOT: &str = "/obj/machinery/portable_atmospherics";
pub const AIR_ALARM_ROOT: &str = "/obj/machinery/airalarm";
pub const AIR_ALARM_HELPER_ROOT: &str = "/obj/effect/mapping_helpers/airalarm";

/// The atmos vars of a DM type (type defaults + map var edits); fields read 0/false/empty
/// where the type has no such var.
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmAtmosDevice {
    pub path: String,
    pub piping_layer: f64,
    /// `#RRGGBB`.
    pub pipe_color: String,
    pub pipe_flags: f64,
    pub device_type: f64,
    pub initialize_directions: f64,
    pub hide: bool,
    pub on: bool,
    pub welded: bool,
    /// Pipes, portables, tanks.
    pub volume: f64,
    pub target_pressure: f64,
    pub transfer_rate: f64,
    pub pump_direction: f64,
    pub pressure_checks: f64,
    pub external_pressure_bound: f64,
    pub internal_pressure_bound: f64,
    /// Scrubbers' `filter_types`, filters' `filter_type`, portable scrubbers' `scrubbing`.
    pub filter_types: Vec<String>,
    /// Vent scrubbers: `ATMOS_DIRECTION_SCRUBBING` 1 / siphoning 0.
    pub scrubbing: f64,
    pub widenet: bool,
    pub volume_rate: f64,
    pub node1_concentration: f64,
    pub node2_concentration: f64,
    pub flipped: bool,
    /// Valves: `"m"` manual, `"d"` digital.
    pub valve_type: String,
    pub target_temperature: f64,
    pub base_heating: f64,
    pub base_cooling: f64,
    /// Canisters: the gas `create_gas` fills (tg id), how full and the pressure scale.
    pub gas_type: String,
    pub filled: f64,
    pub maximum_pressure: f64,
    pub release_pressure: f64,
    pub valve_open: bool,
    pub initial_gas_mix: String,
    /// Portable pumps: `PUMP_IN` true / `PUMP_OUT` false.
    pub direction: bool,
    pub overpressure_m: f64,
    /// Air alarms.
    pub req_access: Vec<String>,
    pub req_one_access: Vec<String>,
    pub locked: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Num,
    Flag,
    Text,
    /// A gas path, stored as its tg id.
    Gas,
}

/// Scalar vars: DM name, kind, field name.
const VARS: [(&str, Kind, &str); 34] = [
    ("piping_layer", Kind::Num, "piping_layer"),
    ("pipe_color", Kind::Text, "pipe_color"),
    ("pipe_flags", Kind::Num, "pipe_flags"),
    ("device_type", Kind::Num, "device_type"),
    ("initialize_directions", Kind::Num, "initialize_directions"),
    ("hide", Kind::Flag, "hide"),
    ("on", Kind::Flag, "on"),
    ("welded", Kind::Flag, "welded"),
    ("volume", Kind::Num, "volume"),
    ("target_pressure", Kind::Num, "target_pressure"),
    ("transfer_rate", Kind::Num, "transfer_rate"),
    ("pump_direction", Kind::Num, "pump_direction"),
    ("pressure_checks", Kind::Num, "pressure_checks"),
    (
        "external_pressure_bound",
        Kind::Num,
        "external_pressure_bound",
    ),
    (
        "internal_pressure_bound",
        Kind::Num,
        "internal_pressure_bound",
    ),
    ("widenet", Kind::Flag, "widenet"),
    ("volume_rate", Kind::Num, "volume_rate"),
    ("node1_concentration", Kind::Num, "node1_concentration"),
    ("node2_concentration", Kind::Num, "node2_concentration"),
    ("flipped", Kind::Flag, "flipped"),
    ("valve_type", Kind::Text, "valve_type"),
    ("target_temperature", Kind::Num, "target_temperature"),
    ("base_heating", Kind::Num, "base_heating"),
    ("base_cooling", Kind::Num, "base_cooling"),
    ("gas_type", Kind::Gas, "gas_type"),
    ("filled", Kind::Num, "filled"),
    ("maximum_pressure", Kind::Num, "maximum_pressure"),
    ("release_pressure", Kind::Num, "release_pressure"),
    ("valve_open", Kind::Flag, "valve_open"),
    ("initial_gas_mix", Kind::Text, "initial_gas_mix"),
    ("direction", Kind::Flag, "direction"),
    ("overpressure_m", Kind::Num, "overpressure_m"),
    ("locked", Kind::Flag, "locked"),
    // A vent scrubber's `scrubbing` is a number; a portable scrubber's is its gas list.
    ("scrubbing", Kind::Num, "scrubbing"),
];

/// List vars: gas lists, then access lists.
const GAS_LISTS: [&str; 3] = ["filter_types", "filter_type", "scrubbing"];
const ACCESS_LISTS: [&str; 2] = ["req_access", "req_one_access"];

pub fn is_atmos_type(path: &str) -> bool {
    [
        ATMOS_ROOT,
        PORTABLE_ROOT,
        AIR_ALARM_ROOT,
        AIR_ALARM_HELPER_ROOT,
    ]
    .iter()
    .any(|root| is_under(path, root))
}

/// Converter fields of `path`: its path and every resolved var above.
pub fn type_fields(types: &DmTypes, path: &str) -> Vec<Field> {
    let mut out = Vec::new();
    if !is_atmos_type(path) || !types.types.contains_key(path) {
        return out;
    }
    out.push(ext(COMPONENT, "path", ron_str(path)));
    for (var, kind, key) in VARS {
        let Some(value) = types.resolved(path, var) else {
            continue;
        };
        if value.get("list").is_some() {
            continue;
        }
        if let Some(ron) = json_ron(kind, value) {
            out.push(ext(COMPONENT, key, ron));
        }
    }
    for var in GAS_LISTS {
        if let Some(list) = types.resolved(path, var).and_then(|v| v.get("list")) {
            out.push(ext(COMPONENT, "filter_types", ron_list(&json_gases(list))));
        }
    }
    for var in ACCESS_LISTS {
        if let Some(value) = types.resolved(path, var) {
            let items: Vec<String> = value
                .get("list")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect();
            out.push(ext(COMPONENT, access_key(var), ron_list(&items)));
        }
    }
    out
}

fn access_key(var: &str) -> &'static str {
    if var == "req_access" {
        "req_access"
    } else {
        "req_one_access"
    }
}

/// `{"list": [{"path": "/datum/gas/x"}, "n2", ...]}` -> tg gas ids.
fn json_gases(list: &Value) -> Vec<String> {
    list.as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.get("path").and_then(Value::as_str).or_else(|| v.as_str()))
        .filter_map(gas_id)
        .collect()
}

fn gas_id(path: &str) -> Option<String> {
    ss13_atmos::GasId::parse(path).map(|g| g.info().id.to_owned())
}

fn json_ron(kind: Kind, value: &Value) -> Option<String> {
    match kind {
        Kind::Num => num(Some(value)).map(num_ron),
        Kind::Flag => Some(truthy(Some(value)).to_string()),
        Kind::Text => match value {
            Value::Null => Some(ron_str("")),
            Value::String(s) => Some(ron_str(s)),
            _ => None,
        },
        Kind::Gas => match value {
            Value::Null => Some(ron_str("")),
            _ => value
                .get("path")
                .and_then(Value::as_str)
                .and_then(gas_id)
                .map(|g| ron_str(&g)),
        },
    }
}

/// `.dmm` var edits of the scalar vars.
pub fn ext_field(path: &str, var: &str, value: Val) -> Result<Field, Skip> {
    if !is_atmos_type(path) {
        return Err(Skip::UnknownVar);
    }
    let Some(&(_, kind, key)) = VARS.iter().find(|(v, ..)| *v == var) else {
        return Err(Skip::UnknownVar);
    };
    let ron = match (kind, value) {
        (Kind::Num, Val::Num(n)) => num_ron(n),
        (Kind::Num, Val::Null) => num_ron(0.0),
        (Kind::Flag, Val::Num(n)) => (n != 0.0).to_string(),
        (Kind::Flag, Val::Null) => "false".to_owned(),
        (Kind::Text, Val::Str(s)) => ron_str(s),
        (Kind::Text | Kind::Gas, Val::Null) => ron_str(""),
        _ => return Err(Skip::Unsupported),
    };
    Ok(ext(COMPONENT, key, ron))
}

/// `.dmm` var edits of the list vars; `None` for any other var.
pub fn field_dmm(path: &str, var: &str, value: &DmValue) -> Option<Result<Field, Skip>> {
    if !is_atmos_type(path) {
        return None;
    }
    let items = |value: &DmValue| -> Option<Vec<String>> {
        match value {
            DmValue::Null => Some(Vec::new()),
            DmValue::List(items) => items
                .iter()
                .map(|(k, v)| match (k, v) {
                    (DmValue::Str(s), None) => Some(s.clone()),
                    (DmValue::Path(p), None) => Some(p.clone()),
                    _ => None,
                })
                .collect(),
            _ => None,
        }
    };
    if GAS_LISTS.contains(&var) && matches!(value, DmValue::List(_) | DmValue::Null) {
        return Some(
            items(value)
                .map(|l| {
                    let gases: Vec<String> = l.iter().filter_map(|g| gas_id(g)).collect();
                    ext(COMPONENT, "filter_types", ron_list(&gases))
                })
                .ok_or(Skip::Unsupported),
        );
    }
    if ACCESS_LISTS.contains(&var) {
        return Some(
            items(value)
                .map(|l| ext(COMPONENT, access_key(var), ron_list(&l)))
                .ok_or(Skip::Unsupported),
        );
    }
    None
}

fn num_ron(n: f64) -> String {
    format!("{n:?}")
}

fn ron_str(s: &str) -> String {
    ron::to_string(s).expect("strings always serialize")
}

fn ron_list(l: &[String]) -> String {
    ron::to_string(l).expect("string lists always serialize")
}

#[cfg(test)]
mod tests {
    use super::*;

    const TYPES: &str = r##"{
      "schema": 3,
      "types": {
        "/atom": { "parent": null },
        "/obj": { "parent": "/atom" },
        "/obj/machinery": { "parent": "/obj" },
        "/obj/machinery/atmospherics": { "parent": "/obj/machinery", "declared_vars": {
            "piping_layer": 3, "pipe_color": "#EEEEEE", "pipe_flags": 0, "device_type": 0,
            "initialize_directions": 15, "hide": 1, "on": 0 } },
        "/obj/machinery/atmospherics/components": { "parent": "/obj/machinery/atmospherics",
            "declared_vars": { "hide": 0, "welded": 0 } },
        "/obj/machinery/atmospherics/components/unary": { "parent": "/obj/machinery/atmospherics/components",
            "declared_vars": { "device_type": 1, "initialize_directions": 2, "pipe_flags": 2 } },
        "/obj/machinery/atmospherics/components/unary/vent_scrubber": { "parent": "/obj/machinery/atmospherics/components/unary",
            "declared_vars": { "scrubbing": 1, "volume_rate": 200, "widenet": 0,
            "filter_types": {"list": [{"path": "/datum/gas/carbon_dioxide"}]} } },
        "/obj/machinery/portable_atmospherics": { "parent": "/obj/machinery", "declared_vars": { "volume": 0 } },
        "/obj/machinery/portable_atmospherics/canister": { "parent": "/obj/machinery/portable_atmospherics",
            "declared_vars": { "volume": 2000, "filled": 0.5, "maximum_pressure": 9119.25, "gas_type": null,
            "release_pressure": 101.325, "valve_open": 0 } },
        "/obj/machinery/portable_atmospherics/canister/nitrogen": { "parent": "/obj/machinery/portable_atmospherics/canister",
            "declared_vars": { "gas_type": {"path": "/datum/gas/nitrogen"} } },
        "/obj/machinery/airalarm": { "parent": "/obj/machinery", "declared_vars": {
            "req_access": {"list": ["atmos_equip"]}, "locked": 1 } }
      }
    }"##;

    fn ron(types: &DmTypes, path: &str) -> String {
        crate::value::patch_ron(&type_fields(types, path))
    }

    #[test]
    fn atmos_vars_and_gas_lists() {
        let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
        let scrubber = ron(
            &types,
            "/obj/machinery/atmospherics/components/unary/vent_scrubber",
        );
        assert!(scrubber.contains("device_type: 1.0"), "{scrubber}");
        assert!(scrubber.contains("filter_types: [\"co2\"]"), "{scrubber}");
        assert!(scrubber.contains("scrubbing: 1.0"), "{scrubber}");
        assert!(scrubber.contains("pipe_color: \"#EEEEEE\""), "{scrubber}");
        let can = ron(
            &types,
            "/obj/machinery/portable_atmospherics/canister/nitrogen",
        );
        assert!(can.contains("gas_type: \"n2\""), "{can}");
        assert!(can.contains("volume: 2000.0"), "{can}");
        let alarm = ron(&types, "/obj/machinery/airalarm");
        assert!(alarm.contains("req_access: [\"atmos_equip\"]"), "{alarm}");
        assert!(alarm.contains("locked: true"), "{alarm}");
        assert_eq!(
            ext_field(
                "/obj/machinery/atmospherics/components/unary",
                "target_pressure",
                Val::Num(300.0)
            ),
            Ok(ext(COMPONENT, "target_pressure", "300.0"))
        );
        let list = DmValue::List(vec![(DmValue::Path("/datum/gas/nitrogen".into()), None)]);
        assert_eq!(
            field_dmm(
                "/obj/machinery/atmospherics/components/trinary",
                "filter_type",
                &list
            ),
            Some(Ok(ext(COMPONENT, "filter_types", "[\"n2\"]")))
        );
        assert_eq!(
            field_dmm("/obj/structure/table", "filter_type", &list),
            None
        );
    }
}
