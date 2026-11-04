//! Bar signs as prototype data: tg `/obj/machinery/barsign` (`code/game/machinery/barsigns.dm`)
//! with every `/datum/barsign` (and BandaStation's `ss220_icon`,
//! `modular_bandastation/barsigns`) as the [`DmBarsign`] component.

use bevy::prelude::*;
use serde_json::Value;

use crate::convert::DmTypes;
use crate::fidelity::{ext, is_under, num};
use crate::value::Field;

pub const BARSIGN_ROOT: &str = "/obj/machinery/barsign";
pub const BARSIGN_COMPONENT: &str = "dm_barsign";
const SIGN_ROOT: &str = "/datum/barsign";

/// One `/datum/barsign`.
#[derive(Reflect, Default, Clone, Debug, PartialEq)]
pub struct BarSign {
    pub name: String,
    /// `desc`, else `It displays "name".` (`New()`).
    pub desc: String,
    /// `ss220_icon`; empty: the machine's own icon.
    pub icon: String,
    pub icon_state: String,
    /// Not in the pick list (`hidden`, the `hiddensigns` subtypes).
    pub hidden: bool,
}

/// A bar sign type (prototype data).
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmBarsign {
    /// `req_access`.
    pub req_access: Vec<String>,
    /// `subtypesof(/datum/barsign)` in path order.
    pub signs: Vec<BarSign>,
}

fn text(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Object(o)) => o
            .get("resource")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        _ => String::new(),
    }
}

/// Every bar sign datum.
pub fn signs(types: &DmTypes) -> Vec<BarSign> {
    let mut paths: Vec<&String> = types
        .types
        .keys()
        .filter(|p| is_under(p, SIGN_ROOT) && p.as_str() != SIGN_ROOT)
        .collect();
    paths.sort();
    paths
        .into_iter()
        .map(|p| {
            let get = |var: &str| types.resolved(p, var);
            let name = text(get("name"));
            let mut desc = text(get("desc"));
            if desc.is_empty() {
                desc = format!("It displays \"{name}\".");
            }
            BarSign {
                desc,
                icon: text(get("ss220_icon")),
                icon_state: text(get("icon_state")),
                hidden: num(get("hidden")).is_some_and(|v| v != 0.0),
                name,
            }
        })
        // Abstract parents (`hiddensigns`) have no name.
        .filter(|b| !b.name.is_empty())
        .collect()
}

/// Converter fields of a bar sign type.
pub fn type_fields(types: &DmTypes, path: &str) -> Vec<Field> {
    if !is_under(path, BARSIGN_ROOT) || !types.types.contains_key(path) {
        return Vec::new();
    }
    let s = |t: &str| ron::to_string(t).expect("strings serialize");
    let access: Vec<String> = match types.resolved(path, "req_access") {
        Some(Value::Object(o)) => o
            .get("list")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(s)
            .collect(),
        _ => Vec::new(),
    };
    let signs: Vec<String> = signs(types)
        .iter()
        .map(|b| {
            format!(
                "(name: {}, desc: {}, icon: {}, icon_state: {}, hidden: {})",
                s(&b.name),
                s(&b.desc),
                s(&b.icon),
                s(&b.icon_state),
                b.hidden
            )
        })
        .collect();
    vec![
        ext(
            BARSIGN_COMPONENT,
            "req_access",
            format!("[{}]", access.join(", ")),
        ),
        ext(
            BARSIGN_COMPONENT,
            "signs",
            format!("[{}]", signs.join(", ")),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signs_and_access() {
        let types = DmTypes::parse(
            br##"{ "schema": 3, "types": {
            "/datum": { "parent": null },
            "/datum/barsign": { "parent": "/datum", "declared_vars": { "name": null,
                "icon_state": null, "desc": null, "hidden": 0, "ss220_icon": null } },
            "/datum/barsign/hiddensigns": { "parent": "/datum/barsign",
                "declared_vars": { "hidden": 1 } },
            "/datum/barsign/hiddensigns/signoff": { "parent": "/datum/barsign/hiddensigns",
                "declared_vars": { "name": "Off", "icon_state": "empty" } },
            "/datum/barsign/thebark": { "parent": "/datum/barsign", "declared_vars": {
                "name": "The Bark", "icon_state": "thebark" } },
            "/datum/barsign/warpsurf": { "parent": "/datum/barsign", "declared_vars": {
                "name": "SS220 Warp Surf", "icon_state": "warpsurf",
                "ss220_icon": { "resource": "modular_bandastation/barsigns/icons/barsigns.dmi" } } },
            "/atom": { "parent": "/datum" },
            "/obj": { "parent": "/atom" },
            "/obj/machinery": { "parent": "/obj" },
            "/obj/machinery/barsign": { "parent": "/obj/machinery", "declared_vars": {
                "req_access": { "list": ["bar"] } } }
            } }"##,
        )
        .unwrap();
        let all = signs(&types);
        let names: Vec<&str> = all.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["Off", "The Bark", "SS220 Warp Surf"]);
        assert!(all[0].hidden && !all[1].hidden);
        assert_eq!(all[1].desc, "It displays \"The Bark\".");
        assert!(all[2].icon.ends_with("barsigns.dmi"));
        let fields = format!("{:?}", type_fields(&types, "/obj/machinery/barsign"));
        assert!(fields.contains("[\\\"bar\\\"]"), "{fields}");
    }
}
