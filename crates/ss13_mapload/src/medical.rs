//! Medical stack vars as prototype data (`/obj/item/stack/medical`,
//! `code/game/objects/items/stacks/medical.dm`): what healing reads from the item.
//!
//! Every type under the root carries the component with every var resolved, so a prototype never
//! depends on which ancestors were converted.

use bevy::prelude::*;

use crate::convert::DmTypes;
use crate::fidelity::{ext, is_under, num};
use crate::value::Field;

pub const MEDICAL_ROOT: &str = "/obj/item/stack/medical";
pub const MEDICAL_COMPONENT: &str = "dm_medical";

/// `/obj/item/stack/medical` vars.
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmMedical {
    /// Brute and burn healed per application.
    pub heal_brute: f32,
    pub heal_burn: f32,
    /// Bleeding reduced per application on cut wounds.
    pub stop_bleeding: f32,
    /// Deciseconds to apply to yourself / to someone else (0: at once).
    pub self_delay: f32,
    pub other_delay: f32,
    /// Keeps applying while there is more and the patient is hurt.
    pub repeating: bool,
    pub works_on_dead: bool,
    /// `apply_verb` ("обрабатываем", "зашиваем", ...).
    pub apply_verb: String,
    /// `/obj/item/stack/medical/wrap::always_applicable`: wraps limbs without wounds.
    pub always_applicable: bool,
    /// Burn wound sanitization and flesh regeneration per application.
    pub sanitization: f32,
    pub flesh_regeneration: f32,
    /// Wraps: blood/pus soaked per second, capacity before it falls off, the splint and burn
    /// cleanliness multipliers (0: none).
    pub absorption_rate: f32,
    pub absorption_capacity: f32,
    pub splint_factor: f32,
    pub burn_cleanliness_bonus: f32,
}

const NUM_VARS: [&str; 11] = [
    "heal_brute",
    "heal_burn",
    "stop_bleeding",
    "self_delay",
    "other_delay",
    "sanitization",
    "flesh_regeneration",
    "absorption_rate",
    "absorption_capacity",
    "splint_factor",
    "burn_cleanliness_bonus",
];

pub const ITEM_ROOT: &str = "/obj/item";
pub const WOUNDING_COMPONENT: &str = "dm_wounding";
pub const DEFIB_ROOT: &str = "/obj/item/defibrillator";
pub const DEFIB_COMPONENT: &str = "dm_defib";
/// `SURGICAL_TOOL` (`item_flags`).
pub const SURGICAL_TOOL: u32 = 1 << 10;

/// Types spawned at runtime by medicine: defibrillator paddles (`paddle_type`).
pub const MEDICAL_RUNTIME_TYPES: [&str; 4] = [
    // Dissection's notes (`crate::medical::surgery::autopsy` in `ss13_game`).
    "/obj/item/research_notes",
    "/obj/item/shockpaddles",
    "/obj/item/shockpaddles/syndicate",
    "/obj/item/shockpaddles/syndicate/nanotrasen",
];

/// What an item's hits do to wounds and surgery (`/obj/item` `sharpness`, `wound_bonus`,
/// `exposed_wound_bonus`, `item_flags & SURGICAL_TOOL`). Only on items where one of them is set
/// (or was set on the parent), with every var resolved.
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmWounding {
    /// `SHARP_EDGED` 1, `SHARP_POINTY` 2.
    pub sharpness: u32,
    pub wound_bonus: f32,
    pub exposed_wound_bonus: f32,
    pub surgical_tool: bool,
}

/// `/obj/item/defibrillator` vars: `combat`, `cooldown_duration` (deciseconds), `paddle_type`.
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmDefib {
    pub combat: bool,
    pub cooldown_duration: f32,
    pub paddle_type: String,
}

fn wounding(types: &DmTypes, path: &str) -> (u32, f32, f32, bool) {
    let n = |var| num(types.resolved(path, var)).unwrap_or(0.0);
    (
        n("sharpness").max(0.0) as u32,
        n("wound_bonus") as f32,
        n("exposed_wound_bonus") as f32,
        (n("item_flags").max(0.0) as u32) & SURGICAL_TOOL != 0,
    )
}

fn wounding_fields(types: &DmTypes, path: &str) -> Vec<Field> {
    let mut out = Vec::new();
    if !is_under(path, ITEM_ROOT) || !types.types.contains_key(path) {
        return out;
    }
    let none = (0, 0.0, 0.0, false);
    let own = wounding(types, path);
    let parent = types.types[path]
        .parent
        .as_deref()
        .filter(|p| is_under(p, ITEM_ROOT))
        .map_or(none, |p| wounding(types, p));
    if own == none && parent == none {
        return out;
    }
    let c = WOUNDING_COMPONENT;
    out.push(ext(c, "sharpness", own.0.to_string()));
    out.push(ext(c, "wound_bonus", format!("{:?}", own.1)));
    out.push(ext(c, "exposed_wound_bonus", format!("{:?}", own.2)));
    out.push(ext(c, "surgical_tool", own.3.to_string()));
    out
}

fn defib_fields(types: &DmTypes, path: &str) -> Vec<Field> {
    let mut out = Vec::new();
    if !is_under(path, DEFIB_ROOT) || !types.types.contains_key(path) {
        return out;
    }
    let get = |var| types.resolved(path, var);
    let paddle = match get("paddle_type") {
        Some(serde_json::Value::Object(o)) => o
            .get("path")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        _ => String::new(),
    };
    let c = DEFIB_COMPONENT;
    out.push(ext(
        c,
        "combat",
        (num(get("combat")).unwrap_or(0.0) != 0.0).to_string(),
    ));
    out.push(ext(
        c,
        "cooldown_duration",
        format!("{:?}", num(get("cooldown_duration")).unwrap_or(0.0) as f32),
    ));
    out.push(ext(
        c,
        "paddle_type",
        ron::to_string(&paddle).expect("strings always serialize"),
    ));
    out
}
const BOOL_VARS: [&str; 3] = ["repeating", "works_on_dead", "always_applicable"];

/// Converter fields of `path`.
pub fn type_fields(types: &DmTypes, path: &str) -> Vec<Field> {
    let mut out = wounding_fields(types, path);
    out.extend(defib_fields(types, path));
    if !is_under(path, MEDICAL_ROOT) || !types.types.contains_key(path) {
        return out;
    }
    for var in NUM_VARS {
        if let Some(n) = num(types.resolved(path, var)) {
            out.push(ext(MEDICAL_COMPONENT, var, format!("{:?}", n as f32)));
        }
    }
    for var in BOOL_VARS {
        if let Some(n) = num(types.resolved(path, var)) {
            out.push(ext(MEDICAL_COMPONENT, var, (n != 0.0).to_string()));
        }
    }
    if let Some(serde_json::Value::String(verb)) = types.resolved(path, "apply_verb") {
        out.push(ext(
            MEDICAL_COMPONENT,
            "apply_verb",
            ron::to_string(verb).expect("strings always serialize"),
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const TYPES: &str = r##"{
      "schema": 3,
      "types": {
        "/obj": { "parent": null },
        "/obj/item": { "parent": "/obj", "declared_vars": { "sharpness": 0, "item_flags": 0 } },
        "/obj/item/scalpel": { "parent": "/obj/item", "declared_vars": { "sharpness": 1,
            "wound_bonus": 10, "exposed_wound_bonus": 15, "item_flags": 1024 } },
        "/obj/item/scalpel/blunt": { "parent": "/obj/item/scalpel", "declared_vars": {
            "sharpness": 0, "wound_bonus": 0, "exposed_wound_bonus": 0, "item_flags": 0 } },
        "/obj/item/defibrillator": { "parent": "/obj/item", "declared_vars": { "combat": 0,
            "cooldown_duration": 50, "paddle_type": { "path": "/obj/item/shockpaddles" } } },
        "/obj/item/defibrillator/combat": { "parent": "/obj/item/defibrillator",
            "declared_vars": { "combat": 1, "cooldown_duration": 25 } },
        "/obj/item/stack": { "parent": "/obj/item" },
        "/obj/item/stack/medical": { "parent": "/obj/item/stack", "declared_vars": {
            "heal_brute": null, "heal_burn": null, "stop_bleeding": null, "self_delay": 50,
            "other_delay": 0, "repeating": 0, "works_on_dead": 0,
            "apply_verb": "обрабатываем" } },
        "/obj/item/stack/medical/suture": { "parent": "/obj/item/stack/medical",
            "declared_vars": { "heal_brute": 10, "stop_bleeding": 0.5, "self_delay": 30,
            "other_delay": 10, "repeating": 1, "apply_verb": "зашиваем" } },
        "/obj/item/stack/medical/wrap": { "parent": "/obj/item/stack/medical",
            "declared_vars": { "always_applicable": 0, "works_on_dead": 1 } },
        "/obj/item/stack/medical/wrap/gauze": { "parent": "/obj/item/stack/medical/wrap",
            "declared_vars": { "always_applicable": 1, "other_delay": 20 } }
      }
    }"##;

    #[test]
    fn medical_vars_resolve_through_inheritance() {
        let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
        let ron = |path| crate::value::patch_ron(&type_fields(&types, path));
        assert_eq!(ron("/obj/item/stack"), "{}");
        assert_eq!(
            ron("/obj/item/stack/medical/suture"),
            r#"{"dm_medical": (heal_brute: 10.0, heal_burn: 0.0, stop_bleeding: 0.5, self_delay: 30.0, other_delay: 10.0, sanitization: 0.0, flesh_regeneration: 0.0, absorption_rate: 0.0, absorption_capacity: 0.0, splint_factor: 0.0, burn_cleanliness_bonus: 0.0, repeating: true, works_on_dead: false, always_applicable: false, apply_verb: "зашиваем")}"#
        );
        let gauze = ron("/obj/item/stack/medical/wrap/gauze");
        assert!(
            gauze.contains("self_delay: 50.0, other_delay: 20.0"),
            "{gauze}"
        );
        assert!(gauze.contains("works_on_dead: true, always_applicable: true"));
        assert_eq!(
            ron("/obj/item/scalpel"),
            r#"{"dm_wounding": (sharpness: 1, wound_bonus: 10.0, exposed_wound_bonus: 15.0, surgical_tool: true)}"#
        );
        // A subtype that clears everything still overrides its parent's component.
        assert!(ron("/obj/item/scalpel/blunt").contains("sharpness: 0"));
        assert_eq!(ron("/obj/item/stack"), "{}");
        assert_eq!(
            ron("/obj/item/defibrillator/combat"),
            r#"{"dm_defib": (combat: true, cooldown_duration: 25.0, paddle_type: "/obj/item/shockpaddles")}"#
        );
    }
}
