//! Clothing and gas tank vars as prototype data: what breathing, pressure and temperature read
//! from worn items (`code/modules/clothing/clothing.dm`, `code/game/objects/items.dm`) and what
//! internals read from tanks (`code/game/objects/items/tanks/tanks.dm`).

use bevy::prelude::*;

use crate::convert::DmTypes;
use crate::fidelity::{ext, is_under, num};
use crate::value::Field;

pub const CLOTHING_ROOT: &str = "/obj/item/clothing";
pub const TANK_ROOT: &str = "/obj/item/tank";

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

const CLOTHING_VARS: [&str; 9] = [
    "flash_protect",
    "tint",
    "siemens_coefficient",
    "clothing_flags",
    "body_parts_covered",
    "heat_protection",
    "cold_protection",
    "min_cold_protection_temperature",
    "max_heat_protection_temperature",
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
            let ron =
                if var.ends_with("temperature") || var == "siemens_coefficient" || var == "tint" {
                    format!("{:?}", n as f32)
                } else if var == "flash_protect" {
                    (n as i32).to_string()
                } else {
                    (n.max(0.0) as u32).to_string()
                };
            out.push(ext("dm_clothing", var, ron));
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
        let regular = fields(&types, "/obj/item/clothing/glasses/regular");
        assert_eq!(
            regular,
            [
                ("vision_correction".into(), "true".into()),
                ("reagent_scanner".into(), "false".into())
            ]
        );
        let blindfold = fields(&types, "/obj/item/clothing/glasses/blindfold");
        assert_eq!(blindfold, [("tint".into(), "1e31".into())]);
        let suit = fields(&types, "/obj/item/clothing/suit/space");
        assert!(suit.contains(&("clothing_flags".into(), "18".into())));
        assert!(suit.contains(&("min_cold_protection_temperature".into(), "72.0".into())));
        assert!(suit.contains(&("allowed".into(), "[\"/obj/item/tank/internals\"]".into())));
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
