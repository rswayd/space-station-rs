//! Chemistry data from `dm_types.json`: every tg `/datum/reagent` and `/datum/chemical_reaction`
//! as `ss13_reagents` [`ChemData`] (`ss13-mapload convert` writes [`ss13_reagents::CHEM_FILE`]),
//! and the reagent container vars (`/obj/item/reagent_containers`) as the [`DmReagents`]
//! prototype component.

use std::collections::BTreeSet;

use bevy::prelude::*;
use serde_json::Value;
use ss13_reagents::{ChemData, GlassStyleDef, ReactionDef, ReagentDef};

use crate::convert::DmTypes;
use crate::fidelity::{ext, is_under, num};
use crate::value::Field;

pub const REAGENT_ROOT: &str = "/datum/reagent";
pub const REACTION_ROOT: &str = "/datum/chemical_reaction";
/// Randomized recipes come from a per-server JSON and are rerolled on conflicts; not converted.
pub const RANDOMIZED_REACTIONS: &str = "/datum/chemical_reaction/randomized";
/// Its `pre_reaction_other_checks` always passes.
const FOOD_REACTIONS: &str = "/datum/chemical_reaction/food";
const GLASS_STYLE_ROOT: &str = "/datum/glass_style";
/// `/datum/glass_style/has_foodtype/soup/New()`: no name or description means the soup's.
const SOUP_STYLE: &str = "/datum/glass_style/has_foodtype/soup";
pub const CONTAINER_ROOT: &str = "/obj/item/reagent_containers";
pub const CONTAINER_COMPONENT: &str = "dm_reagents";

/// `REAGENT_UNAFFECTED_BY_METABOLISM`, which `/datum/reagent/consumable/New()` adds.
const REAGENT_UNAFFECTED_BY_METABOLISM: u32 = 1 << 10;

/// One `list(/datum/reagent/x = amount)` entry.
#[derive(Reflect, Default, Clone, Debug, PartialEq)]
pub struct ReagentAmount {
    pub path: String,
    pub amount: f32,
}

/// `/obj/item/reagent_containers` vars (every type carries all of them resolved).
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmReagents {
    /// `volume`: the holder's `maximum_volume`.
    pub volume: f32,
    pub amount_per_transfer_from_this: f32,
    pub possible_transfer_amounts: Vec<f32>,
    pub has_variable_transfer_amount: bool,
    /// Holder flags (`OPENCONTAINER`, `INJECTABLE`, ...).
    pub initial_reagent_flags: u32,
    /// What it spawns with, in order.
    pub list_reagents: Vec<ReagentAmount>,
    /// `list_reagents_purity`; 0: each reagent's default.
    pub list_reagents_purity: f32,
    /// `fill_icon_thresholds` (percent); empty: no fill overlay.
    pub fill_icon_thresholds: Vec<f32>,
    /// Fill state prefix; empty: the icon state.
    pub fill_icon_state: String,
    pub fill_icon: String,
    /// `/obj/item/reagent_containers/cup::gulp_size`.
    pub gulp_size: f32,
    /// `/obj/item/reagent_containers/blood::blood_type`: the `/datum/blood_type` path its blood
    /// is (empty: none).
    pub blood_type: String,
}

/// `list(path = n, ...)` of a dumped value; `None` when it is not such a list.
pub(crate) fn amounts(value: Option<&Value>) -> Option<Vec<(String, f32)>> {
    let items = match value {
        None | Some(Value::Null) => return Some(Vec::new()),
        Some(Value::Object(o)) => o.get("list")?.as_array()?,
        _ => return None,
    };
    let mut out: Vec<(String, f32)> = Vec::new();
    for item in items {
        let (key, amount) = match item.get("assoc").and_then(Value::as_array) {
            Some(pair) if pair.len() == 2 => (&pair[0], num(Some(&pair[1]))?),
            _ => (item, 0.0),
        };
        let path = key.get("path")?.as_str()?.to_owned();
        let amount = amount as f32;
        match out.iter_mut().find(|(p, _)| *p == path) {
            Some(entry) => entry.1 = amount,
            None => out.push((path, amount)),
        }
    }
    Some(out)
}

fn numbers(value: Option<&Value>) -> Vec<f32> {
    match value {
        Some(Value::Object(o)) => o
            .get("list")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|v| num(Some(v)).map(|n| n as f32))
                    .collect()
            })
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

pub(crate) fn string(value: Option<&Value>) -> String {
    match value {
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

pub(crate) fn path_value(value: Option<&Value>) -> Option<String> {
    value?.get("path")?.as_str().map(str::to_owned)
}

/// Whether `path` or an ancestor below `root` overrides one of `procs`.
fn overrides(types: &DmTypes, path: &str, root: &str, procs: &[&str]) -> bool {
    types.chain(path).take_while(|p| *p != root).any(|p| {
        types.types[p]
            .procs
            .iter()
            .any(|q| procs.contains(&q.as_str()))
    })
}

/// Every reagent (`valid_subtypesof(/datum/reagent)`: abstract types left out) and every
/// reaction with required reagents, in path order. Reactions with their own
/// `pre_reaction_other_checks` and the randomized ones are left out; [`Skipped`] counts them.
pub fn chem_data(types: &DmTypes) -> (ChemData, Skipped) {
    let paths: BTreeSet<&str> = types.types.keys().map(String::as_str).collect();
    let mut data = ChemData::default();
    let mut skipped = Skipped::default();
    for &path in paths
        .iter()
        .filter(|p| is_under(p, REAGENT_ROOT) && **p != REAGENT_ROOT)
    {
        let get = |var: &str| types.resolved(path, var);
        if path_value(get("abstract_type")).as_deref() == Some(path) {
            continue;
        }
        let n = |var: &str| num(get(var)).unwrap_or(0.0) as f32;
        let mut chemical_flags = n("chemical_flags") as u32;
        if is_under(path, "/datum/reagent/consumable") {
            chemical_flags |= REAGENT_UNAFFECTED_BY_METABOLISM;
        }
        data.reagents.push(ReagentDef {
            path: path.to_owned(),
            name: string(get("name")),
            description: string(get("description")),
            color: string(get("color")),
            taste_description: string(get("taste_description")),
            taste_mult: n("taste_mult"),
            metabolization_rate: n("metabolization_rate"),
            overdose_threshold: n("overdose_threshold"),
            ph: n("ph"),
            specific_heat: n("specific_heat"),
            chemical_flags,
            purity: n("purity"),
            creation_purity: n("creation_purity"),
            inverse_chem: path_value(get("inverse_chem")),
            inverse_chem_val: n("inverse_chem_val"),
            purge_multiplier: n("purge_multiplier"),
            toxpwr: n("toxpwr"),
            health_required: n("health_required"),
            liver_tolerance_multiplier: n("liver_tolerance_multiplier"),
            boozepwr: n("boozepwr"),
            brute_heal: n("brute_heal"),
            nutriment_factor: n("nutriment_factor"),
        });
    }
    for &path in paths
        .iter()
        .filter(|p| is_under(p, REACTION_ROOT) && **p != REACTION_ROOT)
    {
        if is_under(path, RANDOMIZED_REACTIONS) {
            skipped.randomized += 1;
            continue;
        }
        let get = |var: &str| types.resolved(path, var);
        let (Some(required_reagents), Some(results), Some(required_catalysts)) = (
            amounts(get("required_reagents")),
            amounts(get("results")),
            amounts(get("required_catalysts")),
        ) else {
            skipped.unreadable.push(path.to_owned());
            continue;
        };
        if required_reagents.is_empty() {
            continue;
        }
        // `/datum/chemical_reaction/food`'s check only records the purity for its food.
        let check = types
            .chain(path)
            .take_while(|p| *p != REACTION_ROOT)
            .find(|p| {
                types.types[*p]
                    .procs
                    .iter()
                    .any(|q| q == "pre_reaction_other_checks")
            });
        if check.is_some_and(|c| c != FOOD_REACTIONS) {
            skipped.own_checks.push(path.to_owned());
            continue;
        }
        let n = |var: &str| num(get(var)).unwrap_or(0.0) as f32;
        let truthy = |var: &str| n(var) != 0.0;
        data.reactions.push(ReactionDef {
            path: path.to_owned(),
            required_reagents,
            results,
            required_catalysts,
            required_container: path_value(get("required_container")),
            required_container_accepts_subtypes: truthy("required_container_accepts_subtypes"),
            mob_react: truthy("mob_react"),
            mix_message: string(get("mix_message")),
            is_cold_recipe: truthy("is_cold_recipe"),
            required_temp: n("required_temp"),
            optimal_temp: n("optimal_temp"),
            overheat_temp: n("overheat_temp"),
            optimal_ph_min: n("optimal_ph_min"),
            optimal_ph_max: n("optimal_ph_max"),
            determin_ph_range: n("determin_ph_range"),
            temp_exponent_factor: n("temp_exponent_factor"),
            ph_exponent_factor: n("ph_exponent_factor"),
            thermic_constant: n("thermic_constant"),
            h_ion_release: n("H_ion_release"),
            rate_up_lim: n("rate_up_lim"),
            purity_min: n("purity_min"),
            reaction_flags: n("reaction_flags") as u32,
            special: overrides(
                types,
                path,
                REACTION_ROOT,
                &[
                    "on_reaction",
                    "reaction_step",
                    "reaction_finish",
                    "overheated",
                    "overly_impure",
                ],
            ),
        });
    }
    data.glass_styles = glass_styles(types, &data.reagents);
    (data, skipped)
}

/// `create_glass_styles()`: every style type with both required types set.
fn glass_styles(types: &DmTypes, reagents: &[ReagentDef]) -> Vec<GlassStyleDef> {
    let mut out = Vec::new();
    let paths: BTreeSet<&String> = types.types.keys().collect();
    for path in paths {
        if !is_under(path, GLASS_STYLE_ROOT) {
            continue;
        }
        let get = |var: &str| types.resolved(path, var);
        let (Some(container), Some(drink)) = (
            path_value(get("required_container_type")),
            path_value(get("required_drink_type")),
        ) else {
            continue;
        };
        let mut style = GlassStyleDef {
            name: string(get("name")),
            desc: string(get("desc")),
            icon: string(get("icon")),
            icon_state: string(get("icon_state")),
            container,
            drink,
        };
        if is_under(path, SOUP_STYLE)
            && let Some(r) = reagents.iter().find(|r| r.path == style.drink)
        {
            if style.name.is_empty() {
                style.name = r.name.clone();
            }
            if style.desc.is_empty() {
                style.desc = r.description.clone();
            }
        }
        out.push(style);
    }
    out
}

/// Reactions [`chem_data`] did not convert.
#[derive(Default, Debug)]
pub struct Skipped {
    pub randomized: usize,
    /// Their own `pre_reaction_other_checks` (conditions in code).
    pub own_checks: Vec<String>,
    /// A reagent list that is not constant.
    pub unreadable: Vec<String>,
}

/// Converter fields of a reagent container type: the whole [`DmReagents`].
pub fn type_fields(types: &DmTypes, path: &str) -> Vec<Field> {
    if !is_under(path, CONTAINER_ROOT) || !types.types.contains_key(path) {
        return Vec::new();
    }
    let get = |var: &str| types.resolved(path, var);
    let n = |var: &str| num(get(var)).unwrap_or(0.0) as f32;
    let s = |text: &str| ron::to_string(text).expect("strings serialize");
    let floats = |list: &[f32]| {
        let items: Vec<String> = list.iter().map(|v| format!("{v:?}")).collect();
        format!("[{}]", items.join(", "))
    };
    let list: Vec<String> = amounts(get("list_reagents"))
        .unwrap_or_default()
        .iter()
        .map(|(p, a)| format!("(path: {}, amount: {a:?})", s(p)))
        .collect();
    let c = CONTAINER_COMPONENT;
    vec![
        ext(c, "volume", format!("{:?}", n("volume"))),
        ext(
            c,
            "amount_per_transfer_from_this",
            format!("{:?}", n("amount_per_transfer_from_this")),
        ),
        ext(
            c,
            "possible_transfer_amounts",
            floats(&numbers(get("possible_transfer_amounts"))),
        ),
        ext(
            c,
            "has_variable_transfer_amount",
            (n("has_variable_transfer_amount") != 0.0).to_string(),
        ),
        ext(
            c,
            "initial_reagent_flags",
            (n("initial_reagent_flags").max(0.0) as u32).to_string(),
        ),
        ext(c, "list_reagents", format!("[{}]", list.join(", "))),
        ext(
            c,
            "list_reagents_purity",
            format!("{:?}", n("list_reagents_purity")),
        ),
        ext(
            c,
            "fill_icon_thresholds",
            floats(&numbers(get("fill_icon_thresholds"))),
        ),
        ext(c, "fill_icon_state", s(&string(get("fill_icon_state")))),
        ext(c, "fill_icon", s(&string(get("fill_icon")))),
        ext(c, "gulp_size", format!("{:?}", n("gulp_size"))),
        ext(c, "blood_type", s(&type_path(get("blood_type")))),
    ]
}

/// A typepath var's value (`{"path": ...}`); empty when unset.
fn type_path(value: Option<&Value>) -> String {
    value
        .and_then(|v| v.get("path"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TYPES: &str = r##"{
      "schema": 3,
      "types": {
        "/datum": { "parent": null },
        "/datum/reagent": { "parent": "/datum", "declared_vars": {
            "abstract_type": { "path": "/datum/reagent" }, "name": "", "description": "",
            "color": "#000000", "taste_description": "metaphorical salt", "taste_mult": 1,
            "metabolization_rate": 0.2, "overdose_threshold": 0, "ph": 7, "specific_heat": 200,
            "chemical_flags": 0, "purity": 1, "creation_purity": 1,
            "inverse_chem": { "path": "/datum/reagent/inverse" }, "inverse_chem_val": 0.25,
            "purge_multiplier": 1 } },
        "/datum/reagent/water": { "parent": "/datum/reagent", "declared_vars": {
            "name": "Water", "color": "#AAAAAA77", "chemical_flags": 144 } },
        "/datum/reagent/consumable": { "parent": "/datum/reagent", "declared_vars": {
            "abstract_type": { "path": "/datum/reagent/consumable" }, "inverse_chem": null } },
        "/datum/reagent/consumable/sugar": { "parent": "/datum/reagent/consumable",
            "declared_vars": { "name": "Sugar" } },
        "/datum/chemical_reaction": { "parent": "/datum", "declared_vars": {
            "results": { "list": [] }, "required_reagents": { "list": [] },
            "required_catalysts": { "list": [] }, "mob_react": 1,
            "mix_message": "The solution begins to bubble.", "required_temp": 100,
            "H_ion_release": 0.01, "reaction_flags": 0 },
            "procs": ["on_reaction", "pre_reaction_other_checks"] },
        "/datum/chemical_reaction/water": { "parent": "/datum/chemical_reaction",
            "declared_vars": {
            "results": { "list": [{ "assoc": [{ "path": "/datum/reagent/water" }, 2] }] },
            "required_reagents": { "list": [
                { "assoc": [{ "path": "/datum/reagent/sugar" }, 1] },
                { "assoc": [{ "path": "/datum/reagent/water" }, 1] }] },
            "reaction_flags": 2 } },
        "/datum/chemical_reaction/checked": { "parent": "/datum/chemical_reaction/water",
            "procs": ["pre_reaction_other_checks", "on_reaction"] },
        "/datum/chemical_reaction/randomized": { "parent": "/datum/chemical_reaction" },
        "/obj": { "parent": "/datum" },
        "/obj/item": { "parent": "/obj" },
        "/obj/item/reagent_containers": { "parent": "/obj/item", "declared_vars": {
            "amount_per_transfer_from_this": 5, "has_variable_transfer_amount": 1,
            "possible_transfer_amounts": { "list": [5, 10, 15] }, "volume": 30,
            "initial_reagent_flags": 0, "list_reagents": null, "list_reagents_purity": null,
            "fill_icon_thresholds": null, "fill_icon_state": null,
            "fill_icon": { "resource": "icons/obj/medical/reagent_fillings.dmi" } } },
        "/obj/item/reagent_containers/cup": { "parent": "/obj/item/reagent_containers",
            "declared_vars": { "initial_reagent_flags": 60, "gulp_size": 5, "volume": 50 } },
        "/obj/item/reagent_containers/cup/beaker": { "parent": "/obj/item/reagent_containers/cup",
            "declared_vars": { "fill_icon_thresholds": { "list": [0, 1, 20] } } },
        "/obj/item/reagent_containers/cup/beaker/water": {
            "parent": "/obj/item/reagent_containers/cup/beaker", "declared_vars": {
            "list_reagents": { "list": [{ "assoc": [{ "path": "/datum/reagent/water" }, 50] }] } } }
      }
    }"##;

    #[test]
    fn reagents_and_reactions_resolve_through_inheritance() {
        let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
        let (data, skipped) = chem_data(&types);
        let paths: Vec<&str> = data.reagents.iter().map(|r| r.path.as_str()).collect();
        assert_eq!(
            paths,
            ["/datum/reagent/consumable/sugar", "/datum/reagent/water"],
            "abstract types are left out"
        );
        let water = &data.reagents[1];
        assert_eq!(
            (water.name.as_str(), water.color.as_str()),
            ("Water", "#AAAAAA77")
        );
        assert_eq!(water.metabolization_rate, 0.2);
        assert_eq!(
            water.inverse_chem.as_deref(),
            Some("/datum/reagent/inverse")
        );
        let sugar = &data.reagents[0];
        assert_eq!(sugar.chemical_flags, REAGENT_UNAFFECTED_BY_METABOLISM);
        assert_eq!(sugar.inverse_chem, None);
        assert_eq!(data.reactions.len(), 1);
        let r = &data.reactions[0];
        assert_eq!(
            r.required_reagents[0],
            ("/datum/reagent/sugar".to_owned(), 1.0)
        );
        assert_eq!(r.results, [("/datum/reagent/water".to_owned(), 2.0)]);
        assert_eq!(
            (r.reaction_flags, r.h_ion_release, r.special),
            (2, 0.01, false)
        );
        assert_eq!(skipped.own_checks, ["/datum/chemical_reaction/checked"]);
        assert_eq!(skipped.randomized, 1);
    }

    #[test]
    fn container_fields() {
        let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
        let ron = crate::value::patch_ron(&type_fields(
            &types,
            "/obj/item/reagent_containers/cup/beaker/water",
        ));
        assert!(ron.contains("volume: 50.0"), "{ron}");
        assert!(ron.contains("initial_reagent_flags: 60"), "{ron}");
        assert!(
            ron.contains(r#"list_reagents: [(path: "/datum/reagent/water", amount: 50.0)]"#),
            "{ron}"
        );
        assert!(
            ron.contains("fill_icon_thresholds: [0.0, 1.0, 20.0]"),
            "{ron}"
        );
        assert!(
            ron.contains("possible_transfer_amounts: [5.0, 10.0, 15.0]"),
            "{ron}"
        );
        assert!(crate::value::patch_ron(&type_fields(&types, "/obj/item")) == "{}");
    }
}
