//! Circuit boards and the machines built from them as prototype data
//! (`code/game/objects/items/circuitboards/circuitboard.dm`, `code/game/machinery/_machinery.dm`):
//! a board's `build_path` and `req_components`, the parts a map machine starts with
//! (`apply_default_parts`: `def_components` substitutions) and a machine's `circuit`.
//!
//! `/datum/stock_part` keys are resolved here to their `physical_object_type` (the tier 1 item;
//! better tiers are its subtypes, so tg's `istype` check is a path prefix check).

use std::collections::BTreeSet;

use bevy::prelude::*;
use serde_json::Value;

use crate::convert::DmTypes;
use crate::fidelity::{ext, is_under, truthy};
use crate::research::{list, path};
use crate::value::Field;

pub const BOARD_ROOT: &str = "/obj/item/circuitboard";
pub const COMPUTER_BOARD_ROOT: &str = "/obj/item/circuitboard/computer";
pub const MACHINE_BOARD_ROOT: &str = "/obj/item/circuitboard/machine";
const MACHINE_ROOT: &str = "/obj/machinery";
const STOCK_PART_DATUM: &str = "/datum/stock_part";
pub const STACK_ROOT: &str = "/obj/item/stack";
pub const MACHINE_FRAME: &str = "/obj/structure/frame/machine";
pub const COMPUTER_FRAME: &str = "/obj/structure/frame/computer";
pub const CABLE_COIL: &str = "/obj/item/stack/cable_coil";

/// What frames are built from and leave behind, so `ss13-mapload convert` emits them.
pub const FRAME_RUNTIME_TYPES: [&str; 6] = [
    MACHINE_FRAME,
    COMPUTER_FRAME,
    CABLE_COIL,
    "/obj/item/stack/sheet/iron",
    "/obj/item/stack/sheet/glass",
    "/obj/item/shard",
];

const BOARD_COMPONENT: &str = "dm_circuit";
const MACHINE_COMPONENT: &str = "dm_machine";

/// One `req_components` entry: an item type (stacks: the stack type) and how many.
#[derive(Reflect, Default, Clone, Debug, PartialEq)]
pub struct CircuitPart {
    pub path: String,
    pub amount: u32,
}

/// A circuit board's vars, on `/obj/item/circuitboard` and its subtypes.
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmCircuit {
    /// The machine it builds (empty: none).
    pub build_path: String,
    /// `/obj/item/circuitboard/computer`: built in a computer frame.
    pub computer: bool,
    /// Machine boards: the frame must be anchored (`needs_anchored`).
    pub needs_anchored: bool,
    /// What a machine frame needs before the screwdriver completes it, in declared order.
    pub req_components: Vec<CircuitPart>,
    /// The non-stack parts a machine built by `Initialize` holds (`flatten_component_list`
    /// with `def_components`); stacks come back from `req_components` on deconstruction.
    pub default_parts: Vec<CircuitPart>,
}

/// A machine's `circuit`, on `/obj/machinery` types that have one.
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmMachine {
    pub circuit: String,
}

fn str_ron(s: &str) -> String {
    ron::to_string(s).expect("strings serialize")
}

/// A `req_components` key as the item a frame takes: a stock part datum's tier 1 object.
fn physical(types: &DmTypes, key: &str) -> Option<String> {
    if is_under(key, STOCK_PART_DATUM) {
        return path(types.resolved(key, "physical_object_type"));
    }
    is_under(key, "/obj/item").then(|| key.to_owned())
}

fn amount(v: Option<&Value>) -> u32 {
    v.and_then(Value::as_f64).unwrap_or(0.0).max(0.0) as u32
}

/// A board's `req_components` and default parts.
fn board_parts(types: &DmTypes, board: &str) -> (Vec<CircuitPart>, Vec<CircuitPart>) {
    let substitutes: Vec<(String, String)> = list(types.resolved(board, "def_components"))
        .into_iter()
        .filter_map(|(k, v)| Some((path(Some(k))?, path(v)?)))
        .collect();
    let mut req = Vec::new();
    let mut defaults = Vec::new();
    for (key, n) in list(types.resolved(board, "req_components")) {
        let (Some(key), n) = (path(Some(key)), amount(n)) else {
            continue;
        };
        if n == 0 {
            continue;
        }
        let Some(item) = physical(types, &key) else {
            continue;
        };
        if !is_under(&item, STACK_ROOT) {
            let default = substitutes
                .iter()
                .find(|(k, _)| *k == key)
                .and_then(|(_, sub)| physical(types, sub))
                .unwrap_or_else(|| item.clone());
            defaults.push(CircuitPart {
                path: default,
                amount: n,
            });
        }
        req.push(CircuitPart {
            path: item,
            amount: n,
        });
    }
    (req, defaults)
}

fn parts_ron(parts: &[CircuitPart]) -> String {
    let entries: Vec<String> = parts
        .iter()
        .map(|p| format!("(path: {}, amount: {})", str_ron(&p.path), p.amount))
        .collect();
    format!("[{}]", entries.join(", "))
}

/// Converter fields: boards carry their resolved [`DmCircuit`], machines their circuit.
pub fn type_fields(types: &DmTypes, path_: &str) -> Vec<Field> {
    let mut out = Vec::new();
    if !types.types.contains_key(path_) {
        return out;
    }
    if is_under(path_, BOARD_ROOT) {
        let build = path(types.resolved(path_, "build_path")).unwrap_or_default();
        let (req, defaults) = board_parts(types, path_);
        out.push(ext(BOARD_COMPONENT, "build_path", str_ron(&build)));
        out.push(ext(
            BOARD_COMPONENT,
            "computer",
            is_under(path_, COMPUTER_BOARD_ROOT).to_string(),
        ));
        out.push(ext(
            BOARD_COMPONENT,
            "needs_anchored",
            truthy(types.resolved(path_, "needs_anchored")).to_string(),
        ));
        out.push(ext(BOARD_COMPONENT, "req_components", parts_ron(&req)));
        out.push(ext(BOARD_COMPONENT, "default_parts", parts_ron(&defaults)));
    } else if is_under(path_, MACHINE_ROOT)
        && let Some(circuit) = path(types.resolved(path_, "circuit"))
    {
        out.push(ext(MACHINE_COMPONENT, "circuit", str_ron(&circuit)));
    }
    out
}

/// The boards of every machine among `needed`, and the parts of those boards and of the
/// boards among `needed` (printed by the circuit imprinter): deconstruction spawns them.
pub fn machine_closure<'a>(
    types: &DmTypes,
    needed: impl IntoIterator<Item = &'a str>,
) -> BTreeSet<String> {
    let mut boards = BTreeSet::new();
    for p in needed {
        if is_under(p, BOARD_ROOT) {
            boards.insert(p.to_owned());
        } else if is_under(p, MACHINE_ROOT)
            && let Some(circuit) = path(types.resolved(p, "circuit"))
        {
            boards.insert(circuit);
        }
    }
    let mut out = BTreeSet::new();
    for board in &boards {
        let (req, defaults) = board_parts(types, board);
        out.extend(req.into_iter().chain(defaults).map(|p| p.path));
    }
    out.extend(boards);
    out.retain(|p| types.types.contains_key(p));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const TYPES: &str = r#"{ "schema": 3, "types": {
        "/atom": { "vars": {} },
        "/datum/stock_part": { "parent": "/datum" },
        "/datum/stock_part/servo": { "parent": "/datum/stock_part", "declared_vars": {
            "physical_object_type": { "path": "/obj/item/stock_parts/servo" } } },
        "/obj/item/stock_parts/servo": { "parent": "/obj/item" },
        "/obj/item/stock_parts/power_store/cell": { "parent": "/obj/item" },
        "/obj/item/stock_parts/power_store/cell/high": { "parent": "/obj/item/stock_parts/power_store/cell" },
        "/obj/item/stack/sheet/glass": { "parent": "/obj/item/stack" },
        "/obj/item/circuitboard": { "parent": "/obj/item" },
        "/obj/item/circuitboard/machine": { "parent": "/obj/item/circuitboard",
            "declared_vars": { "needs_anchored": 1 } },
        "/obj/item/circuitboard/machine/station": { "parent": "/obj/item/circuitboard/machine",
            "declared_vars": {
                "build_path": { "path": "/obj/machinery/station" },
                "needs_anchored": 0,
                "req_components": { "list": [
                    { "assoc": [{ "path": "/datum/stock_part/servo" }, 2] },
                    { "assoc": [{ "path": "/obj/item/stock_parts/power_store/cell" }, 1] },
                    { "assoc": [{ "path": "/obj/item/stack/sheet/glass" }, 3] } ] },
                "def_components": { "list": [
                    { "assoc": [{ "path": "/obj/item/stock_parts/power_store/cell" },
                                { "path": "/obj/item/stock_parts/power_store/cell/high" }] } ] } } },
        "/obj/machinery": { "parent": "/obj" },
        "/obj/machinery/station": { "parent": "/obj/machinery", "declared_vars": {
            "circuit": { "path": "/obj/item/circuitboard/machine/station" } } }
    } }"#;

    #[test]
    fn boards_resolve_parts() {
        let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
        let (req, defaults) = board_parts(&types, "/obj/item/circuitboard/machine/station");
        let pairs = |v: &[CircuitPart]| -> Vec<(String, u32)> {
            v.iter().map(|p| (p.path.clone(), p.amount)).collect()
        };
        assert_eq!(
            pairs(&req),
            [
                ("/obj/item/stock_parts/servo".to_owned(), 2),
                ("/obj/item/stock_parts/power_store/cell".to_owned(), 1),
                ("/obj/item/stack/sheet/glass".to_owned(), 3),
            ]
        );
        assert_eq!(
            pairs(&defaults),
            [
                ("/obj/item/stock_parts/servo".to_owned(), 2),
                ("/obj/item/stock_parts/power_store/cell/high".to_owned(), 1),
            ],
            "def_components substitute, stacks are not parts"
        );
        let closure = machine_closure(&types, ["/obj/machinery/station"]);
        assert!(closure.contains("/obj/item/circuitboard/machine/station"));
        assert!(closure.contains("/obj/item/stock_parts/power_store/cell/high"));
        assert!(closure.contains("/obj/item/stack/sheet/glass"));
    }
}
