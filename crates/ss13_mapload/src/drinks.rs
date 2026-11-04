//! Drinking glasses as prototype data: tg `isGlass` of `/obj/item/reagent_containers/cup/glass`
//! (`reagent_containers/cups/drinks.dm`) as the [`DmGlass`] component: a thrown glass that is not
//! caught smashes (`ss13_game::bar`).

use bevy::prelude::*;

use crate::convert::DmTypes;
use crate::fidelity::{ext, is_under, num};
use crate::value::Field;

pub const GLASS_ROOT: &str = "/obj/item/reagent_containers/cup/glass";
pub const GLASS_COMPONENT: &str = "dm_glass";

/// A drink container type (prototype data); every type under [`GLASS_ROOT`] has it.
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmGlass {
    /// `isGlass`: it smashes when thrown.
    pub is_glass: bool,
}

/// Converter fields of a drink container type.
pub fn type_fields(types: &DmTypes, path: &str) -> Vec<Field> {
    if !is_under(path, GLASS_ROOT) || !types.types.contains_key(path) {
        return Vec::new();
    }
    let is_glass = num(types.resolved(path, "isGlass")).is_some_and(|v| v != 0.0);
    vec![ext(GLASS_COMPONENT, "is_glass", is_glass.to_string())]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subtypes_can_opt_out() {
        let types = DmTypes::parse(
            br##"{ "schema": 3, "types": {
            "/datum": { "parent": null },
            "/atom": { "parent": "/datum" },
            "/obj": { "parent": "/atom" },
            "/obj/item": { "parent": "/obj" },
            "/obj/item/reagent_containers": { "parent": "/obj/item" },
            "/obj/item/reagent_containers/cup": { "parent": "/obj/item/reagent_containers",
                "declared_vars": { "isGlass": 0 } },
            "/obj/item/reagent_containers/cup/glass": { "parent": "/obj/item/reagent_containers/cup",
                "declared_vars": { "isGlass": 1 } },
            "/obj/item/reagent_containers/cup/glass/coffee": {
                "parent": "/obj/item/reagent_containers/cup/glass", "declared_vars": { "isGlass": 0 } },
            "/obj/item/reagent_containers/cup/glass/drinkingglass": {
                "parent": "/obj/item/reagent_containers/cup/glass" }
            } }"##,
        )
        .unwrap();
        let glass = |p: &str| format!("{:?}", type_fields(&types, p));
        assert!(glass("/obj/item/reagent_containers/cup/glass/drinkingglass").contains("true"));
        assert!(glass("/obj/item/reagent_containers/cup/glass/coffee").contains("false"));
        assert!(type_fields(&types, "/obj/item/reagent_containers/cup").is_empty());
    }
}
