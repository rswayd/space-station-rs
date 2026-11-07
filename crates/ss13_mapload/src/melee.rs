//! Melee weapon vars as prototype data: blocking (`block_chance`, `block_sound` of
//! `/obj/item`), the two-handed forces the weapons pass to `/datum/component/two_handed`
//! (`force_unwielded`, `force_wielded`, `two_hand_force`), the transforming energy weapons'
//! `active_*` vars (`code/game/objects/items/weaponry/melee/energy.dm`) and the shields' bash
//! and break vars (`code/game/objects/items/weaponry/shields.dm`). Every type carrying any of
//! them gets its resolved values; other types get nothing.

use bevy::prelude::*;
use serde_json::Value;

use crate::convert::DmTypes;
use crate::fidelity::{ext, is_under, num, truthy};
use crate::value::Field;

pub const MELEE_COMPONENT: &str = "dm_melee";
pub const SHIELD_ROOT: &str = "/obj/item/shield";
pub const ENERGY_ROOT: &str = "/obj/item/melee/energy";

#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmMelee {
    pub path: String,
    pub block_chance: f32,
    /// Empty: no sound.
    pub block_sound: String,
    /// `BASE_OR_INITIAL_ICON_STATE`: `base_icon_state`, else `icon_state`.
    pub base_icon_state: String,
    /// Declared by the fireaxes, spears and sledgehammers; 0 elsewhere.
    pub force_unwielded: f32,
    pub force_wielded: f32,
    /// The double-bladed energy sword's and the electrostaff's wielded force.
    pub two_hand_force: f32,
    /// `/obj/item/melee/energy` while on.
    pub active_force: f32,
    pub active_throwforce: f32,
    pub active_w_class: u8,
    pub active_hitsound: String,
    /// `/obj/item/shield`.
    pub is_bashable: bool,
    pub shield_bash_sound: String,
    pub breakable_by_damage: bool,
    pub shield_break_sound: String,
    pub shield_break_leftover: String,
    pub max_integrity: f32,
}

fn text(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Object(o)) => o
            .get("resource")
            .or_else(|| o.get("path"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        _ => String::new(),
    }
}

fn str_ron(s: &str) -> String {
    ron::to_string(s).expect("strings always serialize")
}

/// Converter fields of `path`.
pub fn type_fields(types: &DmTypes, path: &str) -> Vec<Field> {
    let mut out = Vec::new();
    if !types.types.contains_key(path) || !is_under(path, "/obj/item") {
        return out;
    }
    let get = |var: &str| types.resolved(path, var);
    let n = |var: &str| num(get(var)).unwrap_or(0.0) as f32;
    let shield = is_under(path, SHIELD_ROOT);
    let energy = is_under(path, ENERGY_ROOT);
    let relevant = n("block_chance") > 0.0
        || n("force_wielded") > 0.0
        || n("two_hand_force") > 0.0
        || shield
        || energy;
    if !relevant {
        return out;
    }
    let c = MELEE_COMPONENT;
    out.push(ext(c, "path", str_ron(path)));
    for var in [
        "block_chance",
        "force_unwielded",
        "force_wielded",
        "two_hand_force",
    ] {
        out.push(ext(c, var, format!("{:?}", n(var))));
    }
    out.push(ext(c, "block_sound", str_ron(&text(get("block_sound")))));
    let base = text(get("base_icon_state"));
    let base = if base.is_empty() {
        text(get("icon_state"))
    } else {
        base
    };
    out.push(ext(c, "base_icon_state", str_ron(&base)));
    if energy {
        out.push(ext(c, "active_force", format!("{:?}", n("active_force"))));
        out.push(ext(
            c,
            "active_throwforce",
            format!("{:?}", n("active_throwforce")),
        ));
        out.push(ext(
            c,
            "active_w_class",
            (n("active_w_class") as u8).to_string(),
        ));
        out.push(ext(
            c,
            "active_hitsound",
            str_ron(&text(get("active_hitsound"))),
        ));
    }
    if shield {
        out.push(ext(
            c,
            "is_bashable",
            truthy(get("is_bashable")).to_string(),
        ));
        out.push(ext(
            c,
            "breakable_by_damage",
            truthy(get("breakable_by_damage")).to_string(),
        ));
        for var in [
            "shield_bash_sound",
            "shield_break_sound",
            "shield_break_leftover",
        ] {
            let key: &'static str = var;
            out.push(ext(c, key, str_ron(&text(get(var)))));
        }
        out.push(ext(c, "max_integrity", format!("{:?}", n("max_integrity"))));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const TYPES: &str = r##"{
      "schema": 3,
      "types": {
        "/obj": { "parent": null, "declared_vars": { "max_integrity": 500 } },
        "/obj/item": { "parent": "/obj", "declared_vars": { "block_chance": 0,
            "block_sound": null, "icon_state": "" } },
        "/obj/item/fireaxe": { "parent": "/obj/item", "declared_vars": {
            "icon_state": "fireaxe0", "base_icon_state": "fireaxe",
            "force_unwielded": 5, "force_wielded": 24 } },
        "/obj/item/fireaxe/boardingaxe": { "parent": "/obj/item/fireaxe",
            "declared_vars": { "force_wielded": 30 } },
        "/obj/item/shield": { "parent": "/obj/item", "declared_vars": { "block_chance": 50,
            "block_sound": { "resource": "sound/items/weapons/block_shield.ogg" },
            "is_bashable": 1, "breakable_by_damage": 1,
            "shield_bash_sound": { "resource": "sound/effects/shieldbash.ogg" },
            "shield_break_sound": { "resource": "sound/effects/bang.ogg" },
            "shield_break_leftover": { "path": "/obj/item/stack/sheet/mineral/wood" } } },
        "/obj/item/shield/buckler": { "parent": "/obj/item/shield",
            "declared_vars": { "block_chance": 30, "max_integrity": 55 } },
        "/obj/item/pen": { "parent": "/obj/item" }
      }
    }"##;

    fn ron(types: &DmTypes, path: &str) -> String {
        crate::value::patch_ron(&type_fields(types, path))
    }

    #[test]
    fn melee_vars_resolve_per_type() {
        let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
        let axe = ron(&types, "/obj/item/fireaxe/boardingaxe");
        for part in [
            "force_unwielded: 5.0",
            "force_wielded: 30.0",
            "base_icon_state: \"fireaxe\"",
        ] {
            assert!(axe.contains(part), "{part} in {axe}");
        }
        let buckler = ron(&types, "/obj/item/shield/buckler");
        for part in [
            "block_chance: 30.0",
            "max_integrity: 55.0",
            "is_bashable: true",
            "block_sound: \"sound/items/weapons/block_shield.ogg\"",
            "shield_break_leftover: \"/obj/item/stack/sheet/mineral/wood\"",
        ] {
            assert!(buckler.contains(part), "{part} in {buckler}");
        }
        assert!(type_fields(&types, "/obj/item/pen").is_empty());
    }
}
