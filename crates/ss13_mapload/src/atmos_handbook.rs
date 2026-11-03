//! The atmos handbook of the gas analyzer window: tg `atmos_handbooks_init()`
//! (`code/__HELPERS/atmospherics.dm`) from the dumped `/datum/gas` vars, the
//! `/datum/gas_reaction` vars and their `init_factors()` lists (`reaction_factors`).

use std::path::Path;

use serde_json::Value;
use ss13_tgui::interfaces::gas_analyzer::{
    AtmosHandbook, HandbookFactor, HandbookGas, HandbookReaction,
};

use crate::convert::DmTypes;
use crate::fidelity::{is_under, num};

/// `<assets>/atmos_handbook.ron`.
pub const HANDBOOK_FILE: &str = "atmos_handbook.ron";
const GAS_ROOT: &str = "/datum/gas";
const REACTION_ROOT: &str = "/datum/gas_reaction";
/// `GAS_EXPORTABLE`.
const GAS_EXPORTABLE: f64 = 1.0;
/// `MONEY_SYMBOL`, `MONEY_NAME` (`code/__DEFINES/economy.dm`).
const MONEY: &str = "¢";

fn text(types: &DmTypes, path: &str, var: &str) -> String {
    types
        .resolved(path, var)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

/// `atmos_handbooks_init`.
pub fn resolve(types: &DmTypes) -> AtmosHandbook {
    let mut gases: Vec<(String, HandbookGas)> = types
        .types
        .keys()
        .filter(|p| is_under(p, GAS_ROOT) && p.as_str() != GAS_ROOT)
        .filter(|p| !text(types, p, "id").is_empty())
        .map(|p| {
            let flags = num(types.resolved(p, "cargo_flags")).unwrap_or(0.0) as u32;
            let export = if flags & GAS_EXPORTABLE as u32 != 0 {
                num(types.resolved(p, "base_value")).unwrap_or(0.0)
            } else {
                0.0
            };
            let gas = HandbookGas {
                id: text(types, p, "id"),
                name: text(types, p, "name"),
                description: text(types, p, "desc"),
                specific_heat: num(types.resolved(p, "specific_heat")).unwrap_or(0.0) as f32,
                export_value: export as f32,
                reactions: Vec::new(),
            };
            (p.clone(), gas)
        })
        .collect();
    let mut reactions = Vec::new();
    for path in types.types.keys().filter(|p| is_under(p, REACTION_ROOT)) {
        // `valid_subtypesof`: no root, no `abstract_type`s.
        let abstract_type = types
            .resolved(path, "abstract_type")
            .and_then(|v| v.get("path"))
            .and_then(Value::as_str);
        if path == REACTION_ROOT || abstract_type == Some(path.as_str()) {
            continue;
        }
        let (id, name) = (text(types, path, "id"), text(types, path, "name"));
        let mut factors = Vec::new();
        let items = types
            .reaction_factors
            .get(path)
            .and_then(|v| v.get("list"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for item in items {
            let Some([key, desc]) = item
                .get("assoc")
                .and_then(Value::as_array)
                .and_then(|a| <&[Value; 2]>::try_from(a.as_slice()).ok())
            else {
                continue;
            };
            let desc = desc.as_str().unwrap_or_default().to_owned();
            let gas = key
                .get("path")
                .and_then(Value::as_str)
                .and_then(|p| gases.iter_mut().find(|(gp, _)| gp == p));
            let factor = match gas {
                Some((_, gas)) => {
                    gas.reactions.push((id.clone(), name.clone()));
                    HandbookFactor {
                        factor_id: Some(gas.id.clone()),
                        factor_type: "gas".into(),
                        factor_name: gas.name.clone(),
                        desc,
                        tooltip: None,
                    }
                }
                None => {
                    let name = key.as_str().unwrap_or_default().to_owned();
                    HandbookFactor {
                        tooltip: misc_tooltip(&name),
                        factor_id: None,
                        factor_type: "misc".into(),
                        factor_name: name,
                        desc,
                    }
                }
            };
            factors.push(factor);
        }
        reactions.push(HandbookReaction {
            id,
            name,
            description: text(types, path, "desc"),
            factors,
        });
    }
    AtmosHandbook {
        gas_info: gases.into_iter().map(|(_, g)| g).collect(),
        reaction_info: reactions,
        money_symbol: MONEY.into(),
        money_name: MONEY.into(),
    }
}

/// The tooltips `atmos_handbooks_init` gives the non-gas factors.
fn misc_tooltip(factor: &str) -> Option<String> {
    Some(match factor {
        "Temperature" | "Pressure" => format!(
            "Reaction is influenced by the {} of the place where the reaction is occurring.",
            factor.to_lowercase()
        ),
        "Energy" => "Energy released by the reaction. May or may not result in linear \
            temperature change depending on a slew of other factors."
            .into(),
        "Radiation" => "This reaction emits hazardous radiation - take precautions.".into(),
        "Location" => {
            "This reaction has special behaviour when occurring in specific locations.".into()
        }
        "Hot Ice" => "Hot ice are solidified stacks of plasma. Ignition of one will result in \
            a raging fire."
            .into(),
        _ => return None,
    })
}

pub fn to_ron(handbook: &AtmosHandbook) -> String {
    let mut out = String::from("// Generated by `ss13-mapload convert`. Do not edit.\n");
    out.push_str(
        &ron::ser::to_string_pretty(handbook, ron::ser::PrettyConfig::default())
            .expect("the handbook serializes"),
    );
    out.push('\n');
    out
}

/// The handbook from `<assets>/atmos_handbook.ron`; empty when missing.
pub fn load(assets: &Path) -> AtmosHandbook {
    let path = assets.join(HANDBOOK_FILE);
    ss13_data::read_to_string(&path)
        .ok()
        .and_then(|text| ron::from_str(&text).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factors_link_gases_and_reactions() {
        let types = DmTypes::parse(
            br#"{"schema": 3, "types": {
            "/datum": {"parent": null},
            "/datum/gas": {"parent": "/datum"},
            "/datum/gas/oxygen": {"parent": "/datum/gas", "declared_vars": {"id": "o2",
                "name": "Oxygen", "desc": "Breathe it.", "specific_heat": 20,
                "cargo_flags": 1, "base_value": 0.2}},
            "/datum/gas_reaction": {"parent": "/datum"},
            "/datum/gas_reaction/standard": {"parent": "/datum/gas_reaction",
                "declared_vars": {"abstract_type": {"path": "/datum/gas_reaction/standard"}}},
            "/datum/gas_reaction/standard/plasmafire": {"parent": "/datum/gas_reaction/standard",
                "declared_vars": {"id": "plasmafire", "name": "Plasma Combustion", "desc": "Burns."}}
            }, "reaction_factors": {"/datum/gas_reaction/standard/plasmafire": {"list": [
                {"assoc": [{"path": "/datum/gas/oxygen"}, "Consumed."]},
                {"assoc": ["Temperature", "Hotter is faster."]}]}}}"#,
        )
        .unwrap();
        let book = resolve(&types);
        assert_eq!(book.reaction_info.len(), 1, "the abstract root is skipped");
        let fire = &book.reaction_info[0];
        assert_eq!(fire.factors[0].factor_id.as_deref(), Some("o2"));
        assert!(
            fire.factors[1]
                .tooltip
                .as_deref()
                .unwrap()
                .contains("temperature")
        );
        let o2 = &book.gas_info[0];
        assert_eq!(
            o2.reactions,
            [("plasmafire".into(), "Plasma Combustion".into())]
        );
        assert_eq!(o2.export_value, 0.2);
    }
}
