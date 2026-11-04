//! Chem dispensers as prototype data: tg `/obj/machinery/chem_dispenser` (`reagents/chemistry/
//! machinery/chem_dispenser.dm`) with the reagent lists its `Initialize` procs settle on, as the
//! [`DmChemDispenser`] component.
//!
//! The lists are `var/static/list` values (`ss13-dm-extract` evaluates them): an empty
//! `dispensable_reagents`/`upgrade_reagents`/`emagged_reagents` takes the type family's static
//! default (`default_*`; soda: `drinks_*`, booze: `beer_*`), as each `Initialize` does.

use bevy::prelude::*;
use serde_json::Value;

use crate::convert::DmTypes;
use crate::fidelity::{ext, is_under, num};
use crate::value::Field;

pub const DISPENSER_ROOT: &str = "/obj/machinery/chem_dispenser";
pub const DISPENSER_COMPONENT: &str = "dm_chem_dispenser";
const DRINKS: &str = "/obj/machinery/chem_dispenser/drinks";
const BEER: &str = "/obj/machinery/chem_dispenser/drinks/beer";
/// `modular_bandastation/objects/code/items/consumables/drinks.dm`: the booze dispenser's
/// `Initialize` adds the fork's liquors.
const BANDASTATION_BEER: [&str; 3] = [
    "/datum/reagent/consumable/ethanol/sambuka",
    "/datum/reagent/consumable/ethanol/jagermeister",
    "/datum/reagent/consumable/ethanol/bluecuracao",
];
/// `EMAGGED` (`obj_flags`).
const EMAGGED: u32 = 1 << 0;

/// What a chem dispenser type dispenses and how (prototype data).
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmChemDispenser {
    /// Reagent types, in the type's order (the game sorts them by name).
    pub dispensable: Vec<String>,
    /// Unlocked by a servo above tier 3.
    pub upgrade: Vec<String>,
    /// Unlocked by an emag.
    pub emagged: Vec<String>,
    /// The starting `amount`.
    pub amount: f32,
    /// Joules per unit (`power_cost`).
    pub power_cost: f32,
    /// Watts (`recharge_amount`).
    pub recharge_amount: f32,
    pub dispensed_temperature: f32,
    pub base_reagent_purity: f32,
    /// Starts emagged (`obj_flags & EMAGGED`): `fullupgrade`'s `dispensable_reagents |=
    /// emagged_reagents`.
    pub emagged_at_start: bool,
}

/// A dumped list of type paths; `None` for `null`.
fn path_list(value: Option<&Value>) -> Option<Vec<String>> {
    let items = value?.get("list")?.as_array()?;
    Some(
        items
            .iter()
            .filter_map(|i| i.get("path")?.as_str().map(str::to_owned))
            .collect(),
    )
}

/// The dispenser model of `path`; `None` for other types.
pub fn dispenser(types: &DmTypes, path: &str) -> Option<DmChemDispenser> {
    if !is_under(path, DISPENSER_ROOT) || !types.types.contains_key(path) {
        return None;
    }
    let get = |var: &str| types.resolved(path, var);
    let n = |var: &str, default: f32| num(get(var)).map_or(default, |v| v as f32);
    let family = if is_under(path, BEER) {
        "beer"
    } else if is_under(path, DRINKS) {
        "drinks"
    } else {
        "default"
    };
    // `drinks` keeps `drink_emagged_reagents`, not `drinks_emagged_reagents`.
    let static_list = |kind: &str| {
        let name = match (family, kind) {
            ("drinks", "emagged") => "drink_emagged_reagents".to_owned(),
            _ => format!("{family}_{kind}_reagents"),
        };
        path_list(get(&name)).unwrap_or_default()
    };
    // `null`: none; an empty list: the family's default; else the type's own.
    let list = |var: &str, kind: &str| match path_list(get(var)) {
        None => Vec::new(),
        Some(own) if own.is_empty() => static_list(kind),
        Some(own) => own,
    };
    let mut dispensable = list("dispensable_reagents", "dispensable");
    let mut emagged = list("emagged_reagents", "emagged");
    // `beer/Initialize` replaces both lists whatever they hold.
    if family == "beer" {
        dispensable = static_list("dispensable");
        emagged = static_list("emagged");
        for extra in BANDASTATION_BEER {
            if !dispensable.iter().any(|p| p == extra) {
                dispensable.push(extra.to_owned());
            }
        }
    }
    let emagged_at_start = num(get("obj_flags")).is_some_and(|f| f as u32 & EMAGGED != 0);
    Some(DmChemDispenser {
        dispensable,
        upgrade: list("upgrade_reagents", "upgrade"),
        emagged,
        amount: n("amount", 30.0),
        power_cost: n("power_cost", 100.0),
        recharge_amount: n("recharge_amount", 300.0),
        dispensed_temperature: n("dispensed_temperature", 300.0),
        base_reagent_purity: n("base_reagent_purity", 1.0),
        emagged_at_start,
    })
}

/// Converter fields of a dispenser type: the whole [`DmChemDispenser`].
pub fn type_fields(types: &DmTypes, path: &str) -> Vec<Field> {
    let Some(d) = dispenser(types, path) else {
        return Vec::new();
    };
    let c = DISPENSER_COMPONENT;
    let paths = |list: &[String]| {
        let items: Vec<String> = list
            .iter()
            .map(|p| ron::to_string(p).expect("strings serialize"))
            .collect();
        format!("[{}]", items.join(", "))
    };
    vec![
        ext(c, "dispensable", paths(&d.dispensable)),
        ext(c, "upgrade", paths(&d.upgrade)),
        ext(c, "emagged", paths(&d.emagged)),
        ext(c, "amount", format!("{:?}", d.amount)),
        ext(c, "power_cost", format!("{:?}", d.power_cost)),
        ext(c, "recharge_amount", format!("{:?}", d.recharge_amount)),
        ext(
            c,
            "dispensed_temperature",
            format!("{:?}", d.dispensed_temperature),
        ),
        ext(
            c,
            "base_reagent_purity",
            format!("{:?}", d.base_reagent_purity),
        ),
        ext(c, "emagged_at_start", d.emagged_at_start.to_string()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    const TYPES: &str = r##"{
      "schema": 3,
      "types": {
        "/datum": { "parent": null },
        "/atom": { "parent": "/datum" },
        "/obj": { "parent": "/atom", "declared_vars": { "obj_flags": 2 } },
        "/obj/machinery": { "parent": "/obj" },
        "/obj/machinery/chem_dispenser": { "parent": "/obj/machinery", "declared_vars": {
            "amount": 30, "power_cost": 100, "recharge_amount": 300,
            "dispensed_temperature": 300, "base_reagent_purity": 1,
            "dispensable_reagents": { "list": [] }, "upgrade_reagents": { "list": [] },
            "emagged_reagents": { "list": [] },
            "default_dispensable_reagents": { "list": [{ "path": "/datum/reagent/water" }] },
            "default_upgrade_reagents": { "list": [{ "path": "/datum/reagent/ash" }] },
            "default_emagged_reagents": { "list": [{ "path": "/datum/reagent/toxin" }] } } },
        "/obj/machinery/chem_dispenser/drinks": { "parent": "/obj/machinery/chem_dispenser",
            "declared_vars": { "amount": 10, "upgrade_reagents": null,
            "drinks_dispensable_reagents": { "list": [{ "path": "/datum/reagent/consumable/space_cola" }] },
            "drink_emagged_reagents": { "list": [{ "path": "/datum/reagent/toxin/mindbreaker" }] } } },
        "/obj/machinery/chem_dispenser/drinks/fullupgrade": {
            "parent": "/obj/machinery/chem_dispenser/drinks", "declared_vars": { "obj_flags": 3 } },
        "/obj/machinery/chem_dispenser/drinks/beer": { "parent": "/obj/machinery/chem_dispenser/drinks",
            "declared_vars": { "upgrade_reagents": null,
            "beer_dispensable_reagents": { "list": [{ "path": "/datum/reagent/consumable/ethanol/beer" }] },
            "beer_emagged_reagents": { "list": [{ "path": "/datum/reagent/iron" }] } } }
      }
    }"##;

    #[test]
    fn empty_lists_take_the_family_defaults() {
        let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
        let chem = dispenser(&types, DISPENSER_ROOT).unwrap();
        assert_eq!(chem.dispensable, ["/datum/reagent/water"]);
        assert_eq!(chem.upgrade, ["/datum/reagent/ash"]);
        assert_eq!(chem.emagged, ["/datum/reagent/toxin"]);
        let soda = dispenser(&types, DRINKS).unwrap();
        assert_eq!(soda.dispensable, ["/datum/reagent/consumable/space_cola"]);
        assert!(soda.upgrade.is_empty(), "null upgrade list");
        assert_eq!(soda.emagged, ["/datum/reagent/toxin/mindbreaker"]);
        assert_eq!(soda.amount, 10.0);
        let full = dispenser(&types, "/obj/machinery/chem_dispenser/drinks/fullupgrade").unwrap();
        assert!(full.emagged_at_start);
        let beer = dispenser(&types, BEER).unwrap();
        assert_eq!(beer.dispensable[0], "/datum/reagent/consumable/ethanol/beer");
        assert_eq!(&beer.dispensable[1..], BANDASTATION_BEER);
        assert_eq!(beer.emagged, ["/datum/reagent/iron"]);
    }
}
