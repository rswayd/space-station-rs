//! DM power data as prototype data: the vars tg power code reads (`code/modules/power/`,
//! `code/game/machinery/_machinery.dm`, `code/game/area/areas.dm`) on machines, cables, areas
//! and the APC mapping helpers. Only those families get [`DmPower`]; the game builds its live
//! power state from it once.
//!
//! Values tg derives in procs from other types are resolved here, at conversion time, from
//! `dm_types.json`: the APC cell (`cell_type` -> `maxcharge`/`chargerate`), the SMES capacity
//! (its circuit board's batteries) and the cells the APC helpers install.

use bevy::prelude::*;

use crate::convert::DmTypes;
use crate::fidelity::{ext, is_under, num, truthy};
use crate::value::{Field, Skip, Val};

const COMPONENT: &str = "dm_power";

pub const MACHINE_ROOT: &str = "/obj/machinery";
pub const CABLE_ROOT: &str = "/obj/structure/cable";
pub const AREA_ROOT: &str = "/area";
pub const APC_ROOT: &str = "/obj/machinery/power/apc";
pub const SMES_ROOT: &str = "/obj/machinery/power/smes";
pub const TERMINAL_ROOT: &str = "/obj/machinery/power/terminal";
pub const RTG_ROOT: &str = "/obj/machinery/power/rtg";
pub const LIGHT_ROOT: &str = "/obj/machinery/light";
/// tg `/obj/effect/mapping_helpers/apc`: payloads applied to the APC on their tile.
pub const APC_HELPER_ROOT: &str = "/obj/effect/mapping_helpers/apc";
pub const APC_HELPER_CELL_5K: &str = "/obj/effect/mapping_helpers/apc/cell_5k";
pub const APC_HELPER_CELL_10K: &str = "/obj/effect/mapping_helpers/apc/cell_10k";
pub const APC_HELPER_NO_CHARGE: &str = "/obj/effect/mapping_helpers/apc/no_charge";
pub const APC_HELPER_FULL_CHARGE: &str = "/obj/effect/mapping_helpers/apc/full_charge";
/// `unlock()`, `give_syndicate_access()`, `give_away_general_access()` (`apc_main.dm`).
pub const APC_HELPER_UNLOCKED: &str = "/obj/effect/mapping_helpers/apc/unlocked";
pub const APC_HELPER_SYNDICATE_ACCESS: &str = "/obj/effect/mapping_helpers/apc/syndicate_access";
pub const APC_HELPER_AWAY_GENERAL_ACCESS: &str =
    "/obj/effect/mapping_helpers/apc/away_general_access";
/// Turns the lights of its area off with the light switch (`area.lightswitch = FALSE`).
pub const LIGHTS_OFF_HELPER: &str = "/obj/effect/mapping_helpers/turn_off_lights_with_lightswitch";
// shortcut: no `cut_AI_wire` APC helper (it cuts a wire at map load); add it with map-time wires.

/// The cells `install_cell_5k()` / `install_cell_10k()` create (`apc_main.dm`).
const HELPER_CELLS: [(&str, &str); 2] = [
    (
        APC_HELPER_CELL_5K,
        "/obj/item/stock_parts/power_store/battery/upgraded",
    ),
    (
        APC_HELPER_CELL_10K,
        "/obj/item/stock_parts/power_store/battery/high",
    ),
];
/// `req_components` key of the batteries a SMES board holds.
const BATTERY: &str = "/obj/item/stock_parts/power_store/battery";

/// The power vars of a DM type (field = DM var name unless noted). Prototype data like
/// [`DmAtom`](crate::DmAtom); every family root declares its own defaults, so `Default` is only
/// what absent fields read as.
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmPower {
    /// The DM type: every power type sets its own, so instances keep their type's.
    pub path: String,
    /// Machines: `NO_POWER_USE` 0, `IDLE_POWER_USE` 1, `ACTIVE_POWER_USE` 2.
    pub use_power: f64,
    /// Watts.
    pub idle_power_usage: f64,
    pub active_power_usage: f64,
    /// `AREA_USAGE_EQUIP` 1, `AREA_USAGE_LIGHT` 2, `AREA_USAGE_ENVIRON` 3.
    pub power_channel: f64,
    /// Cables and power machines: `CABLE_LAYER_*` bits.
    pub cable_layer: f64,
    /// APC: initial cell charge, percent.
    pub start_charge: f64,
    /// APC channels: `APC_CHANNEL_OFF` 0 .. `APC_CHANNEL_AUTO_ON` 3.
    pub lighting: f64,
    pub equipment: f64,
    pub environ: f64,
    pub operating: bool,
    pub chargemode: bool,
    /// APC: area type it serves instead of its own (mapper var).
    pub areastring: String,
    /// APC (from `cell_type`) and APC cell helpers: the cell's `maxcharge`, joules.
    pub cell_maxcharge: f64,
    /// The cell's `chargerate`, watts.
    pub cell_chargerate: f64,
    /// SMES: initial charge and capacity (sum of its board's batteries), joules.
    pub charge: f64,
    pub capacity: f64,
    /// SMES, watts.
    pub input_level: f64,
    pub output_level: f64,
    pub input_attempt: bool,
    pub output_attempt: bool,
    /// RTG: constant output, watts.
    pub power_gen: f64,
    /// Areas.
    pub requires_power: bool,
    pub always_unpowered: bool,
    pub power_equip: bool,
    pub power_light: bool,
    pub power_environ: bool,
    pub lightswitch: bool,
    /// Areas: `area_flags` (`VALID_TERRITORY`, `CULT_PERMITTED`, ...).
    pub area_flags: f64,
    /// Light fixtures (brightness and bulb power are in [`DmLight`](crate::DmLight)).
    pub base_state: String,
    /// The `.dmi` of the fixture's lit overlays.
    pub overlay_icon: String,
    /// Watts per unit of `brightness * bulb_power`.
    pub power_consumption_rate: f64,
    pub no_low_power: bool,
    pub start_with_cell: bool,
}

#[derive(Clone, Copy)]
enum Kind {
    Num,
    Flag,
    Text,
}

/// Power vars, their [`DmPower`] field and the family that has them.
const VARS: [(&str, Kind, &str); 30] = [
    ("use_power", Kind::Num, MACHINE_ROOT),
    ("idle_power_usage", Kind::Num, MACHINE_ROOT),
    ("active_power_usage", Kind::Num, MACHINE_ROOT),
    ("power_channel", Kind::Num, MACHINE_ROOT),
    ("cable_layer", Kind::Num, "/obj/machinery/power"),
    ("start_charge", Kind::Num, APC_ROOT),
    ("lighting", Kind::Num, APC_ROOT),
    ("equipment", Kind::Num, APC_ROOT),
    ("environ", Kind::Num, APC_ROOT),
    ("operating", Kind::Flag, APC_ROOT),
    ("chargemode", Kind::Flag, APC_ROOT),
    ("areastring", Kind::Text, APC_ROOT),
    ("charge", Kind::Num, SMES_ROOT),
    ("input_level", Kind::Num, SMES_ROOT),
    ("output_level", Kind::Num, SMES_ROOT),
    ("input_attempt", Kind::Flag, SMES_ROOT),
    ("output_attempt", Kind::Flag, SMES_ROOT),
    ("power_gen", Kind::Num, RTG_ROOT),
    ("requires_power", Kind::Flag, AREA_ROOT),
    ("always_unpowered", Kind::Flag, AREA_ROOT),
    ("power_equip", Kind::Flag, AREA_ROOT),
    ("power_light", Kind::Flag, AREA_ROOT),
    ("power_environ", Kind::Flag, AREA_ROOT),
    ("lightswitch", Kind::Flag, AREA_ROOT),
    ("area_flags", Kind::Num, AREA_ROOT),
    ("base_state", Kind::Text, LIGHT_ROOT),
    ("overlay_icon", Kind::Text, LIGHT_ROOT),
    ("power_consumption_rate", Kind::Num, LIGHT_ROOT),
    ("no_low_power", Kind::Flag, LIGHT_ROOT),
    ("start_with_cell", Kind::Flag, LIGHT_ROOT),
];

/// Types that get [`DmPower`]: the families above, the cable family and the helpers.
pub fn is_power_type(path: &str) -> bool {
    [
        MACHINE_ROOT,
        CABLE_ROOT,
        AREA_ROOT,
        APC_HELPER_ROOT,
        LIGHTS_OFF_HELPER,
    ]
    .iter()
    .any(|root| is_under(path, root))
}

fn family_has(path: &str, var: &str, family: &str) -> bool {
    // Cables have `cable_layer` too, under another root than power machines.
    is_under(path, family) || (var == "cable_layer" && is_under(path, CABLE_ROOT))
}

/// Converter fields of `path`: its type path, the power vars it declares and the values tg
/// derives from other types. Family roots also get what they inherit from above the root.
pub fn type_fields(types: &DmTypes, path: &str) -> Vec<Field> {
    let mut out = Vec::new();
    let Some(ty) = types.types.get(path) else {
        return out;
    };
    if !is_power_type(path) {
        return out;
    }
    out.push(ext(COMPONENT, "path", str_ron(path)));
    let roots = [MACHINE_ROOT, CABLE_ROOT, AREA_ROOT];
    for (var, kind, family) in VARS {
        if !family_has(path, var, family) {
            continue;
        }
        let value = if roots.contains(&path) || path == family {
            types.resolved(path, var)
        } else {
            ty.declared_vars.get(var)
        };
        if let Some(Some(ron)) = value.map(|v| json_ron(kind, v)) {
            out.push(ext(COMPONENT, var, ron));
        }
    }
    let cell = if path == APC_ROOT
        || (is_under(path, APC_ROOT) && ty.declared_vars.contains_key("cell_type"))
    {
        types.resolved(path, "cell_type").and_then(type_path)
    } else {
        HELPER_CELLS
            .iter()
            .find(|(helper, _)| is_under(path, helper))
            .map(|(_, cell)| *cell)
    };
    if let Some(cell) = cell {
        let get = |var| num(types.resolved(cell, var)).unwrap_or(0.0);
        out.push(ext(COMPONENT, "cell_maxcharge", num_ron(get("maxcharge"))));
        out.push(ext(
            COMPONENT,
            "cell_chargerate",
            num_ron(get("chargerate")),
        ));
    }
    if path == SMES_ROOT || (is_under(path, SMES_ROOT) && ty.declared_vars.contains_key("circuit"))
    {
        let capacity = types
            .resolved(path, "circuit")
            .and_then(type_path)
            .map_or(0.0, |board| smes_capacity(types, board));
        out.push(ext(COMPONENT, "capacity", num_ron(capacity)));
    }
    out
}

/// tg `RefreshParts()`: the sum of the board's batteries' `maxcharge`, which are its
/// `def_components` substitute (or the generic battery) times the `req_components` count.
fn smes_capacity(types: &DmTypes, board: &str) -> f64 {
    let pairs = |var| assoc_pairs(types.resolved(board, var));
    let count = pairs("req_components")
        .into_iter()
        .find(|(k, _)| type_path(k) == Some(BATTERY))
        .and_then(|(_, v)| num(Some(v)))
        .unwrap_or(0.0);
    let defs = pairs("def_components");
    let battery = defs
        .into_iter()
        .find(|(k, _)| type_path(k) == Some(BATTERY))
        .and_then(|(_, v)| type_path(v))
        .unwrap_or(BATTERY);
    count * num(types.resolved(battery, "maxcharge")).unwrap_or(0.0)
}

/// `{"list": [{"assoc": [k, v]}, ...]}` -> `(k, v)` pairs.
fn assoc_pairs(value: Option<&serde_json::Value>) -> Vec<(&serde_json::Value, &serde_json::Value)> {
    let Some(items) = value
        .and_then(|v| v.get("list"))
        .and_then(serde_json::Value::as_array)
    else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| match item.get("assoc")?.as_array()?.as_slice() {
            [k, v] => Some((k, v)),
            _ => None,
        })
        .collect()
}

fn type_path(v: &serde_json::Value) -> Option<&str> {
    v.get("path").and_then(serde_json::Value::as_str)
}

fn json_ron(kind: Kind, value: &serde_json::Value) -> Option<String> {
    match kind {
        Kind::Num => num(Some(value)).map(num_ron),
        Kind::Flag => Some(truthy(Some(value)).to_string()),
        Kind::Text => match value {
            serde_json::Value::Null => Some(str_ron("")),
            serde_json::Value::String(s) => Some(str_ron(s)),
            serde_json::Value::Object(o) => o.get("resource")?.as_str().map(str_ron),
            _ => None,
        },
    }
}

/// `.dmm` var edits of power vars.
pub fn ext_field(path: &str, var: &str, value: Val) -> Result<Field, Skip> {
    let Some(&(key, kind, family)) = VARS.iter().find(|(v, ..)| *v == var) else {
        return Err(Skip::UnknownVar);
    };
    if !family_has(path, var, family) {
        return Err(Skip::UnknownVar);
    }
    let ron = match (kind, value) {
        (Kind::Num, Val::Num(n)) => num_ron(n),
        (Kind::Num, Val::Null) => num_ron(0.0),
        (Kind::Flag, Val::Num(n)) => (n != 0.0).to_string(),
        (Kind::Flag, Val::Null) => "false".to_owned(),
        (Kind::Text, Val::Str(s)) => str_ron(s),
        (Kind::Text, Val::Null) => str_ron(""),
        _ => return Err(Skip::Unsupported),
    };
    Ok(ext(COMPONENT, key, ron))
}

fn num_ron(n: f64) -> String {
    format!("{n:?}")
}

fn str_ron(s: &str) -> String {
    ron::to_string(s).expect("strings always serialize")
}

#[cfg(test)]
mod tests {
    use super::*;

    const TYPES: &str = r##"{
      "schema": 3,
      "types": {
        "/atom": { "parent": null },
        "/obj": { "parent": "/atom" },
        "/obj/item": { "parent": "/obj" },
        "/obj/item/stock_parts/power_store": { "parent": "/obj/item", "declared_vars": {
            "maxcharge": 10000, "chargerate": 500 } },
        "/obj/item/stock_parts/power_store/battery": { "parent": "/obj/item/stock_parts/power_store",
            "declared_vars": { "maxcharge": 1000000, "chargerate": 1000000 } },
        "/obj/item/stock_parts/power_store/battery/upgraded": { "parent": "/obj/item/stock_parts/power_store/battery",
            "declared_vars": { "maxcharge": 2500000, "chargerate": 500000 } },
        "/obj/item/stock_parts/power_store/battery/high": { "parent": "/obj/item/stock_parts/power_store/battery",
            "declared_vars": { "maxcharge": 10000000, "chargerate": 750000 } },
        "/obj/item/stock_parts/power_store/battery/high/empty": { "parent": "/obj/item/stock_parts/power_store/battery/high" },
        "/obj/item/circuitboard": { "parent": "/obj/item" },
        "/obj/item/circuitboard/machine/smes": { "parent": "/obj/item/circuitboard", "declared_vars": {
            "def_components": {"list": [{"assoc": [{"path": "/obj/item/stock_parts/power_store/battery"}, {"path": "/obj/item/stock_parts/power_store/battery/high/empty"}]}]},
            "req_components": {"list": [{"assoc": [{"path": "/obj/item/stack/cable_coil"}, 5]}, {"assoc": [{"path": "/obj/item/stock_parts/power_store/battery"}, 5]}]} } },
        "/obj/machinery": { "parent": "/obj", "declared_vars": {
            "use_power": 1, "idle_power_usage": 100, "active_power_usage": 1000, "power_channel": 1 } },
        "/obj/machinery/power": { "parent": "/obj/machinery", "declared_vars": {
            "use_power": 0, "idle_power_usage": 0, "active_power_usage": 0, "cable_layer": 2 } },
        "/obj/machinery/power/apc": { "parent": "/obj/machinery/power", "declared_vars": {
            "cell_type": {"path": "/obj/item/stock_parts/power_store/battery/upgraded"},
            "start_charge": 90, "lighting": 3, "equipment": 3, "environ": 3, "operating": 1,
            "chargemode": 1, "areastring": null } },
        "/obj/machinery/power/apc/auto_name": { "parent": "/obj/machinery/power/apc" },
        "/obj/machinery/power/smes": { "parent": "/obj/machinery/power", "declared_vars": {
            "circuit": {"path": "/obj/item/circuitboard/machine/smes"}, "charge": 0,
            "input_level": 50000, "output_level": 50000, "input_attempt": 1, "output_attempt": 1 } },
        "/obj/machinery/power/smes/engineering": { "parent": "/obj/machinery/power/smes",
            "declared_vars": { "charge": 50000000, "output_level": 90000 } },
        "/obj/machinery/light": { "parent": "/obj/machinery", "declared_vars": {
            "use_power": 2, "power_channel": 2, "base_state": "tube", "power_consumption_rate": 20,
            "overlay_icon": { "resource": "icons/lo.dmi" }, "no_low_power": 0,
            "start_with_cell": 1 } },
        "/obj/structure": { "parent": "/obj" },
        "/obj/structure/cable": { "parent": "/obj/structure", "declared_vars": { "cable_layer": 2 } },
        "/obj/structure/cable/layer1": { "parent": "/obj/structure/cable", "declared_vars": { "cable_layer": 1 } },
        "/obj/structure/table": { "parent": "/obj/structure" },
        "/obj/effect": { "parent": "/obj" },
        "/obj/effect/mapping_helpers": { "parent": "/obj/effect" },
        "/obj/effect/mapping_helpers/apc": { "parent": "/obj/effect/mapping_helpers" },
        "/obj/effect/mapping_helpers/apc/cell_10k": { "parent": "/obj/effect/mapping_helpers/apc" },
        "/area": { "parent": "/atom", "declared_vars": { "requires_power": 1, "always_unpowered": 0,
            "power_equip": 1, "power_light": 1, "power_environ": 1, "lightswitch": 1 } },
        "/area/space": { "parent": "/area", "declared_vars": { "always_unpowered": 1,
            "power_equip": 0, "power_light": 0, "power_environ": 0 } }
      }
    }"##;

    fn ron(types: &DmTypes, path: &str) -> String {
        crate::value::patch_ron(&type_fields(types, path))
    }

    #[test]
    fn power_vars_and_derived_values() {
        let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
        assert_eq!(ron(&types, "/obj/structure/table"), "{}");
        assert_eq!(ron(&types, "/obj/item/circuitboard"), "{}");
        assert!(ron(&types, "/obj/machinery/light").contains(r#"overlay_icon: "icons/lo.dmi""#));
        assert_eq!(
            ron(&types, "/obj/structure/cable/layer1"),
            r#"{"dm_power": (path: "/obj/structure/cable/layer1", cable_layer: 1.0)}"#
        );
        assert_eq!(
            ron(&types, "/obj/machinery/power/apc"),
            r#"{"dm_power": (path: "/obj/machinery/power/apc", start_charge: 90.0, lighting: 3.0, equipment: 3.0, environ: 3.0, operating: true, chargemode: true, areastring: "", cell_maxcharge: 2500000.0, cell_chargerate: 500000.0)}"#
        );
        assert_eq!(
            ron(&types, "/obj/machinery/power/apc/auto_name"),
            r#"{"dm_power": (path: "/obj/machinery/power/apc/auto_name")}"#
        );
        // 5 batteries of the board's `def_components` substitute (10 MJ each).
        assert!(ron(&types, "/obj/machinery/power/smes").contains("capacity: 50000000.0"));
        assert_eq!(
            ron(&types, "/obj/machinery/power/smes/engineering"),
            r#"{"dm_power": (path: "/obj/machinery/power/smes/engineering", charge: 50000000.0, output_level: 90000.0)}"#
        );
        assert!(
            ron(&types, "/obj/effect/mapping_helpers/apc/cell_10k")
                .contains("cell_maxcharge: 10000000.0, cell_chargerate: 750000.0")
        );
        assert_eq!(
            ron(&types, "/area/space"),
            r#"{"dm_power": (path: "/area/space", always_unpowered: true, power_equip: false, power_light: false, power_environ: false)}"#
        );
        assert_eq!(
            ext_field(
                "/obj/machinery/power/apc",
                "areastring",
                Val::Str("/area/x")
            ),
            Ok(ext(COMPONENT, "areastring", "\"/area/x\""))
        );
        assert_eq!(
            ext_field("/obj/structure/cable", "cable_layer", Val::Num(4.0)),
            Ok(ext(COMPONENT, "cable_layer", "4.0"))
        );
        assert_eq!(
            ext_field("/obj/structure/table", "cable_layer", Val::Num(4.0)),
            Err(Skip::UnknownVar)
        );
        assert_eq!(
            ext_field("/obj/machinery/light", "charge", Val::Num(1.0)),
            Err(Skip::UnknownVar)
        );
    }
}
