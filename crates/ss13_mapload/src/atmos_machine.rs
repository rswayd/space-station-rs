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
/// `/obj/machinery/meter`: its `target_layer` lands in `piping_layer`.
pub const METER_ROOT: &str = "/obj/machinery/meter";
/// Chamber sensors and the consoles that read them (`atmos_computers/`).
pub const AIR_SENSOR_ROOT: &str = "/obj/machinery/air_sensor";
pub const ATMOS_CONTROL_ROOT: &str = "/obj/machinery/computer/atmos_control";

/// `/obj/structure/tank_frame`: what cutting open a stationary tank leaves, spawned at runtime.
pub const TANK_FRAME: &str = "/obj/structure/tank_frame";

/// `/obj/item/pipe` and `/obj/item/pipe_meter`: what wrenching a pipe, device or meter off
/// leaves, spawned at runtime.
pub const PIPE_FITTING: &str = "/obj/item/pipe";
pub const PIPE_METER_ITEM: &str = "/obj/item/pipe_meter";

/// `/obj/machinery/pipedispenser`: the atmos pipe dispenser (its `disposal` and `transit_tube`
/// subtypes are other machines).
pub const PIPE_DISPENSER: &str = "/obj/machinery/pipedispenser";

/// One `/datum/pipe_info` of `GLOB.atmos_pipe_recipes`: its name, the machine it makes (`None`:
/// the meter) and `all_layers` (the RPD can lay it on several layers at once).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PipeRecipe {
    pub name: &'static str,
    pub path: Option<&'static str>,
    pub all_layers: bool,
    /// `desc` (`<br>` between lines; empty for none).
    pub desc: &'static str,
}

const fn recipe(name: &'static str, path: &'static str, all_layers: bool) -> PipeRecipe {
    PipeRecipe {
        name,
        path: Some(path),
        all_layers,
        desc: "",
    }
}

const fn described(
    name: &'static str,
    path: &'static str,
    all_layers: bool,
    desc: &'static str,
) -> PipeRecipe {
    PipeRecipe {
        name,
        path: Some(path),
        all_layers,
        desc,
    }
}

/// `GLOB.atmos_pipe_recipes` (`_globalvars/lists/pipe_recipes.dm`), in its order: the pipe
/// dispenser's and the RPD's atmos categories.
pub const ATMOS_PIPE_RECIPES: [(&str, &[PipeRecipe]); 4] = [
    (
        "Pipes",
        &[
            recipe("Pipe", "/obj/machinery/atmospherics/pipe/smart", true),
            recipe(
                "Layer Adapter",
                "/obj/machinery/atmospherics/pipe/layer_manifold",
                true,
            ),
            recipe(
                "Color Adapter",
                "/obj/machinery/atmospherics/pipe/color_adapter",
                true,
            ),
            described(
                "Bridge Pipe",
                "/obj/machinery/atmospherics/pipe/bridge_pipe",
                true,
                "Can be used to cross pipelines of the same layer and color without accidentally joining them together.<br>Redundant for pipelines on different layers or of different colors.",
            ),
            described(
                "Multi-Deck Adapter",
                "/obj/machinery/atmospherics/pipe/multiz",
                false,
                "Directly connects pipelines vertically across station levels.",
            ),
        ],
    ),
    (
        "Binary",
        &[
            described(
                "Manual Valve",
                "/obj/machinery/atmospherics/components/binary/valve",
                true,
                "Blocks the flow of gas through the pipeline.<br>Cannot be operated by silicons. Does not require power.",
            ),
            described(
                "Digital Valve",
                "/obj/machinery/atmospherics/components/binary/valve/digital",
                true,
                "Blocks the flow of gas through the pipeline.<br>Can be operated by silicons. Does not require power, despite being digital.",
            ),
            described(
                "Pressure Pump",
                "/obj/machinery/atmospherics/components/binary/pump",
                true,
                "Moves gas from one pipeline to another.<br>Transfers a variable amount of gas until the output network reaches the configured pressure.",
            ),
            described(
                "Volume Pump",
                "/obj/machinery/atmospherics/components/binary/volume_pump",
                true,
                "Moves gas from one pipeline to another.<br>Transfers a fixed volume of gas based on the configured volume.<br>Can be overclocked to increase volume moved, though potentially causing leaks.",
            ),
            described(
                "Passive Gate",
                "/obj/machinery/atmospherics/components/binary/passive_gate",
                true,
                "Blocks the flow of gas through the pipeline while the output pipeline has higher pressure than either the configured pressure or the input pipeline's pressure.<br>Does not allow backwards flow, and does not require power.",
            ),
            described(
                "Pressure Gate",
                "/obj/machinery/atmospherics/components/binary/pressure_valve",
                true,
                "Blocks the flow of gas through the pipeline while the input pipeline's pressure is below the configured pressure.<br>Does not allow backwards flow, and does not require power.",
            ),
            described(
                "Temperature Gate",
                "/obj/machinery/atmospherics/components/binary/temperature_gate",
                true,
                "Blocks the flow of gas through the pipeline while the input pipeline's temperature is below the configured temperature.<br>Does not allow backwards flow, and can be multitooled to instead check for exceeding the configured temperature.",
            ),
            described(
                "Temperature Pump",
                "/obj/machinery/atmospherics/components/binary/temperature_pump",
                true,
                "Moves heat from one pipeline to another, cooling the input pipeline and heating the output pipeline. No gas is transferred.",
            ),
        ],
    ),
    (
        "Devices",
        &[
            recipe(
                "Gas Filter",
                "/obj/machinery/atmospherics/components/trinary/filter",
                true,
            ),
            recipe(
                "Gas Mixer",
                "/obj/machinery/atmospherics/components/trinary/mixer",
                true,
            ),
            recipe(
                "Connector",
                "/obj/machinery/atmospherics/components/unary/portables_connector",
                true,
            ),
            recipe(
                "Injector",
                "/obj/machinery/atmospherics/components/unary/outlet_injector",
                true,
            ),
            recipe(
                "Scrubber",
                "/obj/machinery/atmospherics/components/unary/vent_scrubber",
                true,
            ),
            described(
                "Pump Vent",
                "/obj/machinery/atmospherics/components/unary/vent_pump",
                true,
                "A vent with an inbuilt pressure pump.<br>Forces gas from the connected pipeline into the exterior atmosphere, or pulls gas from the exterior atmosphere into the connected pipeline, depending on air alarm configuration.<br>Can be overclocked.",
            ),
            described(
                "Passive Vent",
                "/obj/machinery/atmospherics/components/unary/passive_vent",
                true,
                "Combines exterior atmosphere with the connected pipeline.<br>Does not require power.",
            ),
            PipeRecipe {
                name: "Meter",
                path: None,
                all_layers: true,
                desc: "",
            },
        ],
    ),
    (
        "Heat Exchange",
        &[
            recipe(
                "Pipe",
                "/obj/machinery/atmospherics/pipe/heat_exchanging/simple",
                false,
            ),
            recipe(
                "Manifold",
                "/obj/machinery/atmospherics/pipe/heat_exchanging/manifold",
                false,
            ),
            recipe(
                "4-Way Manifold",
                "/obj/machinery/atmospherics/pipe/heat_exchanging/manifold4w",
                false,
            ),
            recipe(
                "Junction",
                "/obj/machinery/atmospherics/pipe/heat_exchanging/junction",
                false,
            ),
            described(
                "Heat Exchanger",
                "/obj/machinery/atmospherics/components/unary/heat_exchanger",
                false,
                "When directly facing another heat exchanger, balances the temperature of the gases within each connected pipeline.",
            ),
        ],
    ),
];

/// `GLOB.gas_id_to_canister` in `sort_list` order: a canister's `relabel` choices and the type
/// each copies. Read at runtime, so `convert` keeps their prototypes.
pub const CANISTER_LABELS: [(&str, &str); 22] = [
    ("air", "/obj/machinery/portable_atmospherics/canister/air"),
    (
        "antinoblium",
        "/obj/machinery/portable_atmospherics/canister/antinoblium",
    ),
    ("bz", "/obj/machinery/portable_atmospherics/canister/bz"),
    ("caution", "/obj/machinery/portable_atmospherics/canister"),
    (
        "co2",
        "/obj/machinery/portable_atmospherics/canister/carbon_dioxide",
    ),
    (
        "freon",
        "/obj/machinery/portable_atmospherics/canister/freon",
    ),
    (
        "halon",
        "/obj/machinery/portable_atmospherics/canister/halon",
    ),
    (
        "healium",
        "/obj/machinery/portable_atmospherics/canister/healium",
    ),
    (
        "helium",
        "/obj/machinery/portable_atmospherics/canister/helium",
    ),
    (
        "hydrogen",
        "/obj/machinery/portable_atmospherics/canister/hydrogen",
    ),
    (
        "hypernoblium",
        "/obj/machinery/portable_atmospherics/canister/nob",
    ),
    (
        "miasma",
        "/obj/machinery/portable_atmospherics/canister/miasma",
    ),
    (
        "n2",
        "/obj/machinery/portable_atmospherics/canister/nitrogen",
    ),
    (
        "n2o",
        "/obj/machinery/portable_atmospherics/canister/nitrous_oxide",
    ),
    (
        "nitrium",
        "/obj/machinery/portable_atmospherics/canister/nitrium",
    ),
    ("o2", "/obj/machinery/portable_atmospherics/canister/oxygen"),
    (
        "plasma",
        "/obj/machinery/portable_atmospherics/canister/plasma",
    ),
    (
        "pluoxium",
        "/obj/machinery/portable_atmospherics/canister/pluoxium",
    ),
    (
        "proto_nitrate",
        "/obj/machinery/portable_atmospherics/canister/proto_nitrate",
    ),
    (
        "tritium",
        "/obj/machinery/portable_atmospherics/canister/tritium",
    ),
    (
        "water_vapor",
        "/obj/machinery/portable_atmospherics/canister/water_vapor",
    ),
    (
        "zauker",
        "/obj/machinery/portable_atmospherics/canister/zauker",
    ),
];

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
    /// `binary/temperature_pump`: percent of the heat delta moved per fire.
    pub heat_transfer_rate: f64,
    /// `binary/temperature_gate`: let gas through when hotter, not colder.
    pub inverted: bool,
    /// `binary/dp_vent_pump`: the input port's floor and the output port's ceiling.
    pub input_pressure_min: f64,
    pub output_pressure_max: f64,
    /// `/obj/machinery/atmospherics/miner`: the gas (tg id, empty for none), its temperature,
    /// moles per second, the limits it stops at, its light's color, whether it runs.
    pub spawn_id: String,
    /// Air sensors and monitored meters: the chamber they read (`chamber_id`).
    pub chamber_id: String,
    /// `atmos_control` consoles: `atmos_chambers` (id, name), `control`, `reconnecting`.
    pub atmos_chambers: Vec<(String, String)>,
    pub control: bool,
    pub reconnecting: bool,
    /// `GLOB.station_gas_chambers` (id, name), for reconnecting consoles.
    pub station_chambers: Vec<(String, String)>,
    /// `components/tank`: the pressure it takes damage over, and how full it starts.
    pub max_pressure: f64,
    pub starting_pressure_percent: f64,
    pub spawn_temp: f64,
    pub spawn_mol: f64,
    pub max_ext_kpa: f64,
    pub max_ext_mol: f64,
    pub overlay_color: String,
    pub active: bool,
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
    /// Wrenching it off leaves a fitting (`can_unwrench`); the fitting's type
    /// (`construction_type`) and its icon state in `pipe_item.dmi` (`pipe_state`).
    pub can_unwrench: bool,
    pub construction_type: String,
    pub pipe_state: String,
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Num,
    Flag,
    Text,
    /// A gas path, stored as its tg id.
    Gas,
    /// A type path.
    Path,
}

/// Scalar vars: DM name, kind, field name.
const VARS: [(&str, Kind, &str); 54] = [
    ("piping_layer", Kind::Num, "piping_layer"),
    ("target_layer", Kind::Num, "piping_layer"),
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
    ("heat_transfer_rate", Kind::Num, "heat_transfer_rate"),
    ("inverted", Kind::Flag, "inverted"),
    ("input_pressure_min", Kind::Num, "input_pressure_min"),
    ("spawn_id", Kind::Gas, "spawn_id"),
    ("chamber_id", Kind::Text, "chamber_id"),
    ("control", Kind::Flag, "control"),
    ("reconnecting", Kind::Flag, "reconnecting"),
    ("max_pressure", Kind::Num, "max_pressure"),
    (
        "starting_pressure_percent",
        Kind::Num,
        "starting_pressure_percent",
    ),
    ("spawn_temp", Kind::Num, "spawn_temp"),
    ("spawn_mol", Kind::Num, "spawn_mol"),
    ("max_ext_kpa", Kind::Num, "max_ext_kpa"),
    ("max_ext_mol", Kind::Num, "max_ext_mol"),
    ("overlay_color", Kind::Text, "overlay_color"),
    ("active", Kind::Flag, "active"),
    ("output_pressure_max", Kind::Num, "output_pressure_max"),
    // A vent scrubber's `scrubbing` is a number; a portable scrubber's is its gas list.
    ("scrubbing", Kind::Num, "scrubbing"),
    ("can_unwrench", Kind::Flag, "can_unwrench"),
    ("construction_type", Kind::Path, "construction_type"),
    ("pipe_state", Kind::Text, "pipe_state"),
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
        METER_ROOT,
        AIR_SENSOR_ROOT,
        ATMOS_CONTROL_ROOT,
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
    if path == ATMOS_CONTROL_ROOT
        && let Some(list) = types
            .globals
            .get("station_gas_chambers")
            .and_then(|v| v.get("list"))
            .and_then(Value::as_array)
    {
        out.push(ext(
            COMPONENT,
            "station_chambers",
            ron::to_string(&assoc_pairs(list)).expect("strings serialize"),
        ));
    }
    if let Some(list) = types
        .resolved(path, "atmos_chambers")
        .and_then(|v| v.get("list"))
        .and_then(Value::as_array)
    {
        let pairs = assoc_pairs(list);
        out.push(ext(
            COMPONENT,
            "atmos_chambers",
            ron::to_string(&pairs).expect("strings serialize"),
        ));
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

/// `{ "assoc": [key, value] }` text pairs; a later key replaces an earlier one, as in DM.
fn assoc_pairs(list: &[Value]) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for v in list {
        let Some([k, v]) = v
            .get("assoc")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .and_then(|a| <&[Value; 2]>::try_from(a).ok())
        else {
            continue;
        };
        let (Some(k), Some(v)) = (k.as_str(), v.as_str()) else {
            continue;
        };
        match out.iter_mut().find(|(key, _)| key == k) {
            Some(entry) => entry.1 = v.to_owned(),
            None => out.push((k.to_owned(), v.to_owned())),
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
        Kind::Path => match value {
            Value::Null => Some(ron_str("")),
            _ => value.get("path").and_then(Value::as_str).map(ron_str),
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
        (Kind::Text | Kind::Gas | Kind::Path, Val::Null) => ron_str(""),
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
