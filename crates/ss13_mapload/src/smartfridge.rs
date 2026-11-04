//! Smart fridges as prototype data: tg `/obj/machinery/smartfridge` (`food_and_drinks/
//! machinery/smartfridge.dm`): what it starts with (`initial_contents`) and how it draws its
//! contents, as the [`DmSmartFridge`] component.

use std::collections::BTreeSet;

use bevy::prelude::*;

use crate::convert::DmTypes;
use crate::fidelity::{ext, is_under, num};
use crate::reagents::amounts;
use crate::value::Field;

pub const FRIDGE_ROOT: &str = "/obj/machinery/smartfridge";
pub const FRIDGE_COMPONENT: &str = "dm_smartfridge";

/// One smart fridge type (prototype data).
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmSmartFridge {
    /// `initial_contents`: type and count (a missing count is 1).
    pub initial_contents: Vec<(String, u32)>,
    /// `contents_overlay_icon`.
    pub overlay: String,
    pub visible_contents: bool,
    pub has_emissive: bool,
    /// `max_n_of_items` per matter bin tier.
    pub max_items: u32,
}

/// The fridge model of `path`; `None` for other types.
pub fn fridge(types: &DmTypes, path: &str) -> Option<DmSmartFridge> {
    if !is_under(path, FRIDGE_ROOT) || !types.types.contains_key(path) {
        return None;
    }
    let get = |var: &str| types.resolved(path, var);
    let flag = |var: &str| num(get(var)).is_some_and(|v| v != 0.0);
    Some(DmSmartFridge {
        initial_contents: amounts(get("initial_contents"))
            .unwrap_or_default()
            .into_iter()
            .map(|(p, n)| (p, if n <= 0.0 { 1 } else { n as u32 }))
            .collect(),
        overlay: get("contents_overlay_icon")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned(),
        visible_contents: flag("visible_contents"),
        has_emissive: flag("has_emissive"),
        max_items: num(get("max_n_of_items")).map_or(1500, |v| v as u32),
    })
}

/// What fridges on the map start with.
pub fn fridge_closure<'a>(
    types: &DmTypes,
    needed: impl IntoIterator<Item = &'a str>,
) -> BTreeSet<String> {
    needed
        .into_iter()
        .filter_map(|p| fridge(types, p))
        .flat_map(|f| f.initial_contents.into_iter().map(|(p, _)| p))
        .filter(|p| types.types.contains_key(p))
        .collect()
}

/// Converter fields of a fridge type: the whole [`DmSmartFridge`].
pub fn type_fields(types: &DmTypes, path: &str) -> Vec<Field> {
    let Some(f) = fridge(types, path) else {
        return Vec::new();
    };
    let s = |text: &str| ron::to_string(text).expect("strings serialize");
    let c = FRIDGE_COMPONENT;
    let contents: Vec<String> = f
        .initial_contents
        .iter()
        .map(|(p, n)| format!("({}, {n})", s(p)))
        .collect();
    vec![
        ext(c, "initial_contents", format!("[{}]", contents.join(", "))),
        ext(c, "overlay", s(&f.overlay)),
        ext(c, "visible_contents", f.visible_contents.to_string()),
        ext(c, "has_emissive", f.has_emissive.to_string()),
        ext(c, "max_items", f.max_items.to_string()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_contents_default_to_one() {
        let types = DmTypes::parse(
            br##"{ "schema": 3, "types": {
            "/datum": { "parent": null },
            "/atom": { "parent": "/datum" },
            "/obj": { "parent": "/atom" },
            "/obj/item": { "parent": "/obj" },
            "/obj/item/pill": { "parent": "/obj/item" },
            "/obj/item/bottle": { "parent": "/obj/item" },
            "/obj/machinery": { "parent": "/obj" },
            "/obj/machinery/smartfridge": { "parent": "/obj/machinery", "declared_vars": {
                "contents_overlay_icon": "plant", "visible_contents": 1, "has_emissive": 1,
                "max_n_of_items": 1500, "initial_contents": null } },
            "/obj/machinery/smartfridge/chem": { "parent": "/obj/machinery/smartfridge",
                "declared_vars": { "contents_overlay_icon": "chem", "initial_contents": {
                "list": [{ "assoc": [{ "path": "/obj/item/pill" }, 12] },
                         { "path": "/obj/item/bottle" }] } } }
            } }"##,
        )
        .unwrap();
        let chem = fridge(&types, "/obj/machinery/smartfridge/chem").unwrap();
        assert_eq!(
            chem.initial_contents,
            [
                ("/obj/item/pill".into(), 12),
                ("/obj/item/bottle".into(), 1)
            ]
        );
        assert_eq!(chem.overlay, "chem");
        assert!(chem.visible_contents && chem.has_emissive);
        let closure = fridge_closure(&types, ["/obj/machinery/smartfridge/chem"]);
        assert_eq!(closure.len(), 2);
    }
}
