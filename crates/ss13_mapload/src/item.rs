//! DM items as prototype data: the vars of `/obj/item` and its subtypes that picking up, holding
//! and wearing read (`code/game/objects/items.dm`). Only item types get [`DmItem`], so "is an
//! item" is "has `DmItem`", decided by the DM type tree.

use bevy::prelude::*;

use crate::convert::DmTypes;
use crate::fidelity::{ext, is_under, num};
use crate::value::{Field, Skip, Val};

pub const ITEM_ROOT: &str = "/obj/item";
/// `GetID()` returns the card itself for this type and its subtypes (`cards_ids.dm`).
pub const ID_CARD_ROOT: &str = "/obj/item/card/id";
/// `INTERACT_ITEM_ATTACK_HAND_PICKUP` (`code/__DEFINES/interaction_flags.dm`).
pub const INTERACT_ITEM_ATTACK_HAND_PICKUP: u32 = 1;

const COMPONENT: &str = "dm_item";

/// The item vars of a DM item type (type defaults + map var edits). Prototype data like
/// [`DmAtom`](crate::DmAtom); empty strings are DM `null`.
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmItem {
    /// The DM type: every item type sets its own, so instances keep their type's.
    pub path: String,
    pub anchored: bool,
    pub interaction_flags_item: u32,
    /// `ITEM_SLOT_*` bits the item can be worn in.
    pub slot_flags: u32,
    /// Empty: in-hand sprites use `icon_state`.
    pub inhand_icon_state: String,
    pub lefthand_file: String,
    pub righthand_file: String,
    /// Empty: the slot's default file.
    pub worn_icon: String,
    /// Empty: worn sprites use `icon_state`.
    pub worn_icon_state: String,
    /// `access` of ID cards (static: tg fills most cards from their trim at runtime).
    pub access: Vec<String>,
    /// Melee damage (`force`, inherited from `/obj`).
    pub force: f32,
    /// `damtype`: `"brute"`, `"fire"`, `"tox"`, `"oxy"`, `"stamina"` (inherited from `/obj`).
    pub damtype: String,
    /// Click cooldown after a melee attack, deciseconds (`attack_speed`, `CLICK_CD_MELEE`).
    pub attack_speed: f32,
    /// Damage when it hits something thrown (`throwforce`, inherited from `/atom/movable`).
    pub throwforce: f32,
    /// `throw_speed` (`/atom/movable` 2) and `throw_range` in tiles (`/atom/movable` 7).
    pub throw_speed: f32,
    pub throw_range: f32,
    /// `hitsound`: a file or an SFX key; empty: `null` (`Initialize` then picks one by
    /// `damtype`, see `ss13_game::sound::hitsound`).
    pub hitsound: String,
}

impl DmItem {
    pub fn is_id_card(&self) -> bool {
        is_under(&self.path, ID_CARD_ROOT)
    }

    /// tg `attack_hand` -> `attempt_pickup` preconditions that are static.
    pub fn pickable(&self) -> bool {
        !self.anchored && self.interaction_flags_item & INTERACT_ITEM_ATTACK_HAND_PICKUP != 0
    }
}

/// Item vars and their [`DmItem`] field names.
const VARS: [(&str, &str); 16] = [
    ("anchored", "anchored"),
    ("interaction_flags_item", "interaction_flags_item"),
    ("slot_flags", "slot_flags"),
    ("inhand_icon_state", "inhand_icon_state"),
    ("lefthand_file", "lefthand_file"),
    ("righthand_file", "righthand_file"),
    ("worn_icon", "worn_icon"),
    ("worn_icon_state", "worn_icon_state"),
    ("access", "access"),
    ("force", "force"),
    ("damtype", "damtype"),
    ("attack_speed", "attack_speed"),
    ("throwforce", "throwforce"),
    ("throw_speed", "throw_speed"),
    ("throw_range", "throw_range"),
    ("hitsound", "hitsound"),
];

/// Converter fields of `path`: its type path, and the item vars it declares. The root
/// `/obj/item` also gets the values it inherits (`anchored` from `/atom/movable`).
pub fn type_fields(types: &DmTypes, path: &str) -> Vec<Field> {
    let mut out = Vec::new();
    let Some(ty) = types.types.get(path) else {
        return out;
    };
    if !is_under(path, ITEM_ROOT) {
        return out;
    }
    out.push(ext(COMPONENT, "path", str_ron(path)));
    for (var, key) in VARS {
        let value = if path == ITEM_ROOT {
            types.resolved(path, var)
        } else {
            ty.declared_vars.get(var)
        };
        if let Some(Ok(ron)) = value.map(|v| json_ron(var, path, v)) {
            out.push(ext(COMPONENT, key, ron));
        }
    }
    out
}

/// `.dmm` var edits of item vars (lists are not supported in var edits).
pub fn ext_field(path: &str, var: &str, value: Val) -> Result<Field, Skip> {
    let Some(&(_, key)) = VARS.iter().find(|(v, _)| *v == var) else {
        return Err(Skip::UnknownVar);
    };
    if !is_under(path, ITEM_ROOT) || var == "access" {
        return Err(Skip::UnknownVar);
    }
    let ron = match value {
        Val::Null => scalar_ron(var, 0.0, ""),
        Val::Num(n) => scalar_ron(var, n, ""),
        Val::Str(s) | Val::Resource(s) => scalar_ron(var, 0.0, s),
        Val::Other => None,
    };
    ron.map(|r| ext(COMPONENT, key, r)).ok_or(Skip::Unsupported)
}

fn json_ron(var: &str, path: &str, value: &serde_json::Value) -> Result<String, Skip> {
    if var == "access" {
        if !is_under(path, ID_CARD_ROOT) {
            return Err(Skip::UnknownVar);
        }
        let items: Option<Vec<&str>> = match value {
            serde_json::Value::Null => Some(Vec::new()),
            serde_json::Value::Object(o) => match o.get("list") {
                Some(serde_json::Value::Array(items)) => {
                    items.iter().map(serde_json::Value::as_str).collect()
                }
                _ => None,
            },
            _ => None,
        };
        let items = items.ok_or(Skip::Unsupported)?;
        return Ok(ron::to_string(&items).expect("string lists always serialize"));
    }
    let text = match value {
        serde_json::Value::String(s) => s.as_str(),
        serde_json::Value::Object(o) => o
            .get("resource")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default(),
        _ => "",
    };
    let n = num(Some(value)).unwrap_or(0.0);
    let numeric = !matches!(
        var,
        "inhand_icon_state"
            | "lefthand_file"
            | "righthand_file"
            | "worn_icon"
            | "worn_icon_state"
            | "damtype"
            | "hitsound"
    );
    if numeric && num(Some(value)).is_none() {
        return Err(Skip::Unsupported);
    }
    scalar_ron(var, n, text).ok_or(Skip::Unsupported)
}

fn scalar_ron(var: &str, n: f64, text: &str) -> Option<String> {
    Some(match var {
        "anchored" => (n != 0.0).to_string(),
        "interaction_flags_item" | "slot_flags" => (n.max(0.0) as u32).to_string(),
        "inhand_icon_state" | "lefthand_file" | "righthand_file" | "worn_icon"
        | "worn_icon_state" | "damtype" | "hitsound" => str_ron(text),
        "force" | "attack_speed" | "throwforce" | "throw_speed" | "throw_range" => {
            format!("{:?}", n as f32)
        }
        _ => return None,
    })
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
        "/atom/movable": { "parent": "/atom", "declared_vars": { "anchored": 0 } },
        "/obj": { "parent": "/atom/movable", "declared_vars": { "force": 0, "damtype": "brute" } },
        "/obj/item": { "parent": "/obj", "declared_vars": { "attack_speed": 8,
            "interaction_flags_item": 1, "slot_flags": 0, "inhand_icon_state": null,
            "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
            "worn_icon": null, "worn_icon_state": null } },
        "/obj/item/card": { "parent": "/obj/item", "declared_vars": {
            "inhand_icon_state": "card-id" } },
        "/obj/item/card/id": { "parent": "/obj/item/card", "declared_vars": {
            "access": { "list": ["brig", "maint_tunnels"] }, "slot_flags": 256,
            "worn_icon_state": "nothing" } },
        "/obj/item/toolbox": { "parent": "/obj/item", "declared_vars": {
            "force": 13, "damtype": "burn" } },
        "/obj/item/bolted": { "parent": "/obj/item", "declared_vars": {
            "anchored": 1, "access": { "list": ["x"] } } },
        "/obj/structure": { "parent": "/obj" }
      }
    }"##;

    #[test]
    fn item_vars_follow_dm_inheritance() {
        let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
        let ron = |path| {
            let fields = type_fields(&types, path);
            crate::value::patch_ron(&fields)
        };
        assert_eq!(ron("/obj/structure"), "{}");
        assert_eq!(
            ron("/obj/item"),
            r#"{"dm_item": (path: "/obj/item", anchored: false, interaction_flags_item: 1, slot_flags: 0, inhand_icon_state: "", lefthand_file: "l.dmi", righthand_file: "r.dmi", worn_icon: "", worn_icon_state: "", force: 0.0, damtype: "brute", attack_speed: 8.0)}"#
        );
        assert_eq!(
            ron("/obj/item/card/id"),
            r#"{"dm_item": (path: "/obj/item/card/id", slot_flags: 256, worn_icon_state: "nothing", access: ["brig","maint_tunnels"])}"#
        );
        assert_eq!(
            ron("/obj/item/toolbox"),
            r#"{"dm_item": (path: "/obj/item/toolbox", force: 13.0, damtype: "burn")}"#
        );
        // `access` means something else outside ID cards.
        assert_eq!(
            ron("/obj/item/bolted"),
            r#"{"dm_item": (path: "/obj/item/bolted", anchored: true)}"#
        );
        assert_eq!(
            ext_field("/obj/item/card", "anchored", Val::Num(1.0)),
            Ok(ext(COMPONENT, "anchored", "true"))
        );
        assert_eq!(
            ext_field("/obj/structure/table", "anchored", Val::Num(1.0)),
            Err(Skip::UnknownVar)
        );
        assert_eq!(
            ext_field("/obj/item/card/id", "access", Val::Other),
            Err(Skip::UnknownVar)
        );
        let id = DmItem {
            path: "/obj/item/card/id/advanced".into(),
            interaction_flags_item: 1,
            ..default()
        };
        assert!(id.is_id_card() && id.pickable());
        assert!(
            !DmItem {
                anchored: true,
                ..id
            }
            .pickable()
        );
    }
}
