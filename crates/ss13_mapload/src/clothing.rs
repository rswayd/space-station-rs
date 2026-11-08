//! Clothing and gas tank vars as prototype data: what breathing, pressure and temperature read
//! from worn items (`code/modules/clothing/clothing.dm`, `code/game/objects/items.dm`) and what
//! internals read from tanks (`code/game/objects/items/tanks/tanks.dm`).

use bevy::prelude::*;

use crate::convert::DmTypes;
use crate::fidelity::{ext, is_under, num, truthy};
use crate::value::Field;

pub const CLOTHING_ROOT: &str = "/obj/item/clothing";
pub const TANK_ROOT: &str = "/obj/item/tank";
const GLASSES_ROOT: &str = "/obj/item/clothing/glasses";

/// `/obj/item/clothing` vars (the protection ones are declared on `/obj/item`).
#[derive(Component, Reflect, Clone, Debug, PartialEq)]
pub struct DmClothing {
    /// `STOPSPRESSUREDAMAGE`, `MASKINTERNALS`, `HEADINTERNALS`, ... (`obj_flags.dm`).
    pub clothing_flags: u32,
    /// Body part bits (`CHEST`, `HEAD`, ...).
    pub body_parts_covered: u32,
    pub heat_protection: u32,
    pub cold_protection: u32,
    /// Kelvin; 0: no protection.
    pub min_cold_protection_temperature: f32,
    pub max_heat_protection_temperature: f32,
    /// `allowed`: types that fit the suit storage slot of this suit.
    pub allowed: Vec<String>,
    /// `flash_protect` (`FLASH_PROTECTION_*`; negative is sensitive).
    pub flash_protect: i32,
    /// `siemens_coefficient`: how much of a shock gets through (insulated gloves 0).
    pub siemens_coefficient: f32,
    /// `tint`: how much the item darkens the wearer's view (`TINT_*`; a blindfold is infinite).
    pub tint: f32,
    /// `clothing_traits` has `TRAIT_NEARSIGHTED_CORRECTED` (prescription glasses).
    pub vision_correction: bool,
    /// `clothing_traits` has `TRAIT_REAGENT_SCANNER` (science and chemical glasses).
    pub reagent_scanner: bool,
    /// `flags_inv` (`HIDE*`, `code/__DEFINES/inventory.dm`): what the item hides on its wearer.
    pub flags_inv: u32,
    /// `flags_cover` (`*COVERSEYES`, `*COVERSMOUTH`, `PEPPERPROOF`).
    pub flags_cover: u32,
    /// `transparent_protection` (`HIDE*` bits): slots covered without being hidden (the strip
    /// menu cannot reach them).
    pub transparent_protection: u32,
    /// `toggle_message`, `alt_toggle_message`: `adjust_visor`'s messages (empty: the defaults).
    pub toggle_message: String,
    pub alt_toggle_message: String,
    /// `visor_flags`, `visor_flags_inv`, `visor_flags_cover`: what `adjust_visor` toggles.
    pub visor_flags: u32,
    pub visor_flags_inv: u32,
    pub visor_flags_cover: u32,
    /// `limb_integrity`: zone damage before a covered zone shreds (0: never).
    pub limb_integrity: f32,
    /// `actions_types` (item action buttons, e.g. `/datum/action/item_action/adjust`).
    pub actions: Vec<String>,
    /// Masks: BandaStation `can_be_adjusted`, breath masks `adjustable`.
    pub adjustable: bool,
    /// Jumpsuits: `has_sensor` (`BROKEN_SENSORS` -1 .. `LOCKED_SENSORS` 2), `random_sensor`,
    /// `sensor_mode` (`SENSOR_OFF` .. `SENSOR_COORDS`), `can_adjust`, `alt_covers_chest`.
    pub has_sensor: i32,
    pub random_sensor: bool,
    pub sensor_mode: i32,
    pub can_adjust: bool,
    pub alt_covers_chest: bool,
    /// Hooded suits: `hoodtype` (empty: none) and `hood_up_affix`.
    pub hood_type: String,
    pub hood_up_affix: String,
    /// `/obj/item/clothing/suit/toggle`: `toggle_noun` (empty: not a toggle suit).
    pub toggle_noun: String,
    /// Accessories: `attachment_slot` (body part bits, 0: any) and `above_suit`.
    pub attachment_slot: u32,
    pub above_suit: bool,
    /// `clothing_traits`: trait ids (`"noslip_water"`, ...) the wearer has while this is worn in
    /// one of its `slot_flags` slots. Runtime state (`attach_clothing_traits`).
    pub clothing_traits: Vec<String>,
    /// `/obj/item/clothing/glasses` `color_cutoffs` (`None`: no night vision), 0..=100 per
    /// channel.
    pub color_cutoffs: Option<[f32; 3]>,
    /// `/obj/item/clothing/glasses` `lighting_cutoff`.
    pub lighting_cutoff: Option<f32>,
}

impl Default for DmClothing {
    /// `/obj/item` defaults (`siemens_coefficient = 1`).
    fn default() -> Self {
        Self {
            clothing_flags: 0,
            body_parts_covered: 0,
            heat_protection: 0,
            cold_protection: 0,
            min_cold_protection_temperature: 0.0,
            max_heat_protection_temperature: 0.0,
            allowed: Vec::new(),
            flash_protect: 0,
            siemens_coefficient: 1.0,
            tint: 0.0,
            vision_correction: false,
            reagent_scanner: false,
            flags_inv: 0,
            flags_cover: 0,
            transparent_protection: 0,
            toggle_message: String::new(),
            alt_toggle_message: String::new(),
            visor_flags: 0,
            visor_flags_inv: 0,
            visor_flags_cover: 0,
            limb_integrity: 0.0,
            actions: Vec::new(),
            adjustable: true,
            has_sensor: 0,
            random_sensor: false,
            sensor_mode: 0,
            can_adjust: false,
            alt_covers_chest: false,
            hood_type: String::new(),
            hood_up_affix: String::new(),
            toggle_noun: String::new(),
            attachment_slot: 0,
            above_suit: false,
            clothing_traits: Vec::new(),
            color_cutoffs: None,
            lighting_cutoff: None,
        }
    }
}

/// `/obj/item/tank` vars.
#[derive(Component, Reflect, Clone, Debug, PartialEq)]
pub struct DmTank {
    /// Litres.
    pub volume: f32,
    /// kPa the tank releases into a breath.
    pub distribute_pressure: f32,
}

impl Default for DmTank {
    fn default() -> Self {
        Self {
            volume: 70.0,
            distribute_pressure: 101.325,
        }
    }
}

const CLOTHING_VARS: [&str; 16] = [
    "flash_protect",
    "tint",
    "siemens_coefficient",
    "clothing_flags",
    "body_parts_covered",
    "heat_protection",
    "cold_protection",
    "min_cold_protection_temperature",
    "max_heat_protection_temperature",
    "flags_inv",
    "flags_cover",
    "transparent_protection",
    "visor_flags",
    "visor_flags_inv",
    "visor_flags_cover",
    "limb_integrity",
];

/// How a var of a clothing subfamily converts.
#[derive(Clone, Copy)]
enum Kind {
    Int,
    Bits,
    Bool,
    Text,
    Path,
}

/// Vars declared on a subfamily root: (root, var, field, kind). The root carries the resolved
/// value, its subtypes what they declare.
const FAMILY_VARS: [(&str, &str, &str, Kind); 14] = [
    (
        CLOTHING_ROOT,
        "toggle_message",
        "toggle_message",
        Kind::Text,
    ),
    (
        CLOTHING_ROOT,
        "alt_toggle_message",
        "alt_toggle_message",
        Kind::Text,
    ),
    (
        "/obj/item/clothing/mask",
        "can_be_adjusted",
        "adjustable",
        Kind::Bool,
    ),
    (
        "/obj/item/clothing/mask/breath",
        "adjustable",
        "adjustable",
        Kind::Bool,
    ),
    (
        "/obj/item/clothing/under",
        "has_sensor",
        "has_sensor",
        Kind::Int,
    ),
    (
        "/obj/item/clothing/under",
        "random_sensor",
        "random_sensor",
        Kind::Bool,
    ),
    (
        "/obj/item/clothing/under",
        "sensor_mode",
        "sensor_mode",
        Kind::Int,
    ),
    (
        "/obj/item/clothing/under",
        "can_adjust",
        "can_adjust",
        Kind::Bool,
    ),
    (
        "/obj/item/clothing/under",
        "alt_covers_chest",
        "alt_covers_chest",
        Kind::Bool,
    ),
    (
        "/obj/item/clothing/suit/hooded",
        "hoodtype",
        "hood_type",
        Kind::Path,
    ),
    (
        "/obj/item/clothing/suit/hooded",
        "hood_up_affix",
        "hood_up_affix",
        Kind::Text,
    ),
    (
        "/obj/item/clothing/suit/toggle",
        "toggle_noun",
        "toggle_noun",
        Kind::Text,
    ),
    (
        "/obj/item/clothing/accessory",
        "attachment_slot",
        "attachment_slot",
        Kind::Bits,
    ),
    (
        "/obj/item/clothing/accessory",
        "above_suit",
        "above_suit",
        Kind::Bool,
    ),
];

/// `TRAIT_NEARSIGHTED_CORRECTED`.
const NEARSIGHTED_CORRECTED: &str = "fixes_nearsighted";
/// `TRAIT_REAGENT_SCANNER`.
const REAGENT_SCANNER: &str = "reagent_scanner";

const TANK_VARS: [&str; 2] = ["volume", "distribute_pressure"];

/// Converter fields of `path`: the roots carry every var resolved, subtypes what they declare.
pub fn type_fields(types: &DmTypes, path: &str) -> Vec<Field> {
    let mut out = Vec::new();
    let Some(ty) = types.types.get(path) else {
        return out;
    };
    let value = |root: &str, var: &str| {
        if path == root {
            types.resolved(path, var)
        } else {
            ty.declared_vars.get(var)
        }
    };
    if is_under(path, CLOTHING_ROOT) {
        for var in CLOTHING_VARS {
            let Some(n) = value(CLOTHING_ROOT, var).and_then(|v| num(Some(v))) else {
                continue;
            };
            let ron = if var.ends_with("temperature")
                || matches!(var, "siemens_coefficient" | "tint" | "limb_integrity")
            {
                format!("{:?}", n as f32)
            } else if var == "flash_protect" {
                (n as i32).to_string()
            } else {
                (n.max(0.0) as u32).to_string()
            };
            out.push(ext("dm_clothing", var, ron));
        }
        if let Some(actions) = value(CLOTHING_ROOT, "actions_types").and_then(path_list) {
            out.push(ext(
                "dm_clothing",
                "actions",
                ron::to_string(&actions).expect("string lists always serialize"),
            ));
        }
        for (root, var, field, kind) in FAMILY_VARS {
            if !is_under(path, root) {
                continue;
            }
            let Some(v) = (if path == root {
                types.resolved(path, var)
            } else {
                ty.declared_vars.get(var)
            }) else {
                continue;
            };
            let ron = match kind {
                Kind::Int => num(Some(v)).map(|n| (n as i32).to_string()),
                Kind::Bits => num(Some(v)).map(|n| (n.max(0.0) as u32).to_string()),
                Kind::Bool => Some(truthy(Some(v)).to_string()),
                Kind::Text => Some(
                    ron::to_string(v.as_str().unwrap_or_default())
                        .expect("strings always serialize"),
                ),
                Kind::Path => Some(
                    ron::to_string(v.get("path").and_then(|p| p.as_str()).unwrap_or_default())
                        .expect("strings always serialize"),
                ),
            };
            if let Some(ron) = ron {
                out.push(ext("dm_clothing", field, ron));
            }
        }
        if let Some(traits) = value(CLOTHING_ROOT, "clothing_traits") {
            let has = |name: &str| {
                traits
                    .get("list")
                    .and_then(|l| l.as_array())
                    .is_some_and(|l| l.iter().any(|t| t.as_str() == Some(name)))
            };
            out.push(ext(
                "dm_clothing",
                "vision_correction",
                has(NEARSIGHTED_CORRECTED).to_string(),
            ));
            out.push(ext(
                "dm_clothing",
                "reagent_scanner",
                has(REAGENT_SCANNER).to_string(),
            ));
            let list: Vec<&str> = traits
                .get("list")
                .and_then(|l| l.as_array())
                .map(|l| l.iter().filter_map(|t| t.as_str()).collect())
                .unwrap_or_default();
            out.push(ext(
                "dm_clothing",
                "clothing_traits",
                ron::to_string(&list).expect("string lists always serialize"),
            ));
        }
        if is_under(path, GLASSES_ROOT) {
            if let Some(cutoffs) = value(GLASSES_ROOT, "color_cutoffs") {
                let list: Option<[f32; 3]> = cutoffs
                    .get("list")
                    .and_then(|l| l.as_array())
                    .and_then(|l| {
                        l.iter()
                            .map(|c| num(Some(c)).map(|n| n as f32))
                            .collect::<Option<Vec<_>>>()
                    })
                    .and_then(|l| l.try_into().ok());
                out.push(ext(
                    "dm_clothing",
                    "color_cutoffs",
                    ron::to_string(&list).expect("cutoffs always serialize"),
                ));
            }
            if let Some(cutoff) = value(GLASSES_ROOT, "lighting_cutoff") {
                let n: Option<f32> = num(Some(cutoff)).map(|n| n as f32);
                out.push(ext(
                    "dm_clothing",
                    "lighting_cutoff",
                    ron::to_string(&n).expect("cutoffs always serialize"),
                ));
            }
        }
        if let Some(allowed) = value(CLOTHING_ROOT, "allowed").and_then(path_list) {
            out.push(ext(
                "dm_clothing",
                "allowed",
                ron::to_string(&allowed).expect("string lists always serialize"),
            ));
        }
    }
    if is_under(path, TANK_ROOT) {
        for var in TANK_VARS {
            if let Some(n) = value(TANK_ROOT, var).and_then(|v| num(Some(v))) {
                out.push(ext("dm_tank", var, format!("{:?}", n as f32)));
            }
        }
    }
    out
}

/// A dumped list of type paths (`{"list": [{"path": ..}]}`); `null` is empty.
fn path_list(value: &serde_json::Value) -> Option<Vec<String>> {
    match value {
        serde_json::Value::Null => Some(Vec::new()),
        serde_json::Value::Object(o) => o.get("list")?.as_array().map(|items| {
            items
                .iter()
                .filter_map(|i| i.get("path")?.as_str().map(str::to_owned))
                .collect()
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TYPES: &str = r##"{
      "schema": 3,
      "types": {
        "/obj": { "parent": null },
        "/obj/item": { "parent": "/obj", "declared_vars": { "body_parts_covered": 0,
            "siemens_coefficient": 1,
            "heat_protection": 0, "cold_protection": 0, "min_cold_protection_temperature": 0,
            "max_heat_protection_temperature": 0, "allowed": null } },
        "/obj/item/clothing": { "parent": "/obj/item", "declared_vars": { "clothing_flags": 0,
            "flash_protect": 0, "tint": 0, "clothing_traits": null } },
        "/obj/item/clothing/glasses/sunglasses": { "parent": "/obj/item/clothing",
            "declared_vars": { "flash_protect": 1 } },
        "/obj/item/clothing/glasses/regular": { "parent": "/obj/item/clothing",
            "declared_vars": { "clothing_traits": { "list": ["fixes_nearsighted"] } } },
        "/obj/item/clothing/glasses/blindfold": { "parent": "/obj/item/clothing",
            "declared_vars": { "tint": 1e+31 } },
        "/obj/item/clothing/suit/space": { "parent": "/obj/item/clothing", "declared_vars": {
            "body_parts_covered": 2043, "clothing_flags": 18, "min_cold_protection_temperature": 72,
            "allowed": { "list": [{ "path": "/obj/item/tank/internals" }] } } },
        "/obj/item/clothing/gloves/color/yellow": { "parent": "/obj/item/clothing",
            "declared_vars": { "siemens_coefficient": 0 } },
        "/obj/item/clothing/under": { "parent": "/obj/item/clothing", "declared_vars": {
            "has_sensor": 1, "random_sensor": 1, "sensor_mode": 0, "can_adjust": 1,
            "limb_integrity": 30 } },
        "/obj/item/clothing/under/rank/prisoner": { "parent": "/obj/item/clothing/under",
            "declared_vars": { "has_sensor": 2, "random_sensor": 0, "sensor_mode": 3 } },
        "/obj/item/clothing/suit/hooded": { "parent": "/obj/item/clothing", "declared_vars": {
            "hoodtype": { "path": "/obj/item/clothing/head/hooded/winterhood" },
            "hood_up_affix": "_t" } },
        "/obj/item/clothing/mask/gas": { "parent": "/obj/item/clothing", "declared_vars": {
            "flags_inv": 1248, "actions_types": { "list": [
                { "path": "/datum/action/item_action/adjust" }] } } },
        "/obj/item/tank": { "parent": "/obj/item", "declared_vars": { "volume": 70,
            "distribute_pressure": 101.325 } },
        "/obj/item/tank/internals/emergency_oxygen": { "parent": "/obj/item/tank",
            "declared_vars": { "volume": 3, "distribute_pressure": 16 } }
      }
    }"##;

    fn fields(types: &DmTypes, path: &str) -> Vec<(String, String)> {
        type_fields(types, path)
            .into_iter()
            .map(|f| match f {
                Field::Ext { key, ron, .. } => (key.to_owned(), ron),
                other => panic!("{other:?}"),
            })
            .collect()
    }

    #[test]
    fn roots_resolve_and_subtypes_declare() {
        let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
        let root = fields(&types, CLOTHING_ROOT);
        assert!(root.contains(&("body_parts_covered".into(), "0".into())));
        assert!(root.contains(&("allowed".into(), "[]".into())));
        assert!(root.contains(&("flash_protect".into(), "0".into())));
        assert!(root.contains(&("siemens_coefficient".into(), "1.0".into())));
        let gloves = fields(&types, "/obj/item/clothing/gloves/color/yellow");
        assert_eq!(gloves, [("siemens_coefficient".into(), "0.0".into())]);
        let glasses = fields(&types, "/obj/item/clothing/glasses/sunglasses");
        assert_eq!(glasses, [("flash_protect".into(), "1".into())]);
        assert!(root.contains(&("tint".into(), "0.0".into())));
        assert!(root.contains(&("vision_correction".into(), "false".into())));
        assert!(root.contains(&("clothing_traits".into(), "[]".into())));
        let regular = fields(&types, "/obj/item/clothing/glasses/regular");
        assert_eq!(
            regular,
            [
                ("vision_correction".into(), "true".into()),
                ("reagent_scanner".into(), "false".into()),
                ("clothing_traits".into(), "[\"fixes_nearsighted\"]".into())
            ]
        );
        let blindfold = fields(&types, "/obj/item/clothing/glasses/blindfold");
        assert_eq!(blindfold, [("tint".into(), "1e31".into())]);
        let suit = fields(&types, "/obj/item/clothing/suit/space");
        assert!(suit.contains(&("clothing_flags".into(), "18".into())));
        assert!(suit.contains(&("min_cold_protection_temperature".into(), "72.0".into())));
        assert!(suit.contains(&("allowed".into(), "[\"/obj/item/tank/internals\"]".into())));
        let under = fields(&types, "/obj/item/clothing/under");
        assert!(under.contains(&("limb_integrity".into(), "30.0".into())));
        assert!(under.contains(&("has_sensor".into(), "1".into())));
        assert!(under.contains(&("random_sensor".into(), "true".into())));
        assert!(under.contains(&("can_adjust".into(), "true".into())));
        let prisoner = fields(&types, "/obj/item/clothing/under/rank/prisoner");
        assert_eq!(
            prisoner,
            [
                ("has_sensor".into(), "2".into()),
                ("random_sensor".into(), "false".into()),
                ("sensor_mode".into(), "3".into())
            ]
        );
        let hooded = fields(&types, "/obj/item/clothing/suit/hooded");
        assert!(hooded.contains(&(
            "hood_type".into(),
            "\"/obj/item/clothing/head/hooded/winterhood\"".into()
        )));
        assert!(hooded.contains(&("hood_up_affix".into(), "\"_t\"".into())));
        let gas = fields(&types, "/obj/item/clothing/mask/gas");
        assert_eq!(
            gas,
            [
                ("flags_inv".into(), "1248".into()),
                (
                    "actions".into(),
                    "[\"/datum/action/item_action/adjust\"]".into()
                )
            ]
        );
        let tank = fields(&types, "/obj/item/tank/internals/emergency_oxygen");
        assert_eq!(
            tank,
            [
                ("volume".into(), "3.0".into()),
                ("distribute_pressure".into(), "16.0".into())
            ]
        );
    }
}
