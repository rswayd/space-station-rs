//! Smokables as prototype data: the vars of tg `/obj/item/cigarette` (cigarettes, cigars, pipes,
//! rollies; `code/game/objects/items/cigarettes.dm`) as the [`DmSmokable`] component
//! (`ss13_game::smoking` lights and smokes them).

use bevy::prelude::*;

use crate::convert::DmTypes;
use crate::fidelity::{ext, is_under, num, truthy};
use crate::reagents::{ReagentAmount, amounts, string};
use crate::value::Field;

pub const CIGARETTE_ROOT: &str = "/obj/item/cigarette";
pub const SMOKABLE_COMPONENT: &str = "dm_smokable";

/// A smokable type (prototype data); every type under [`CIGARETTE_ROOT`] has it.
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmSmokable {
    /// `smoketime`, seconds (0: a pipe with nothing in it).
    pub smoketime: f32,
    /// `dragtime`, seconds.
    pub dragtime: f32,
    /// `chem_volume`: the holder's size.
    pub chem_volume: f32,
    /// `list_reagents`, in order.
    pub list_reagents: Vec<ReagentAmount>,
    pub smoke_all: bool,
    pub lung_harm: f32,
    /// `heat` while lit.
    pub heat: f32,
    /// `type_butt`: what is left when it burns out (empty: nothing).
    pub type_butt: String,
    pub icon_on: String,
    pub icon_off: String,
    pub inhand_icon_on: String,
    pub inhand_icon_off: String,
    pub starts_lit: bool,
}

/// Converter fields of a smokable type.
pub fn type_fields(types: &DmTypes, path: &str) -> Vec<Field> {
    if !is_under(path, CIGARETTE_ROOT) || !types.types.contains_key(path) {
        return Vec::new();
    }
    let get = |var: &str| types.resolved(path, var);
    let n = |var: &str| num(get(var)).unwrap_or(0.0) as f32;
    let s = |text: &str| ron::to_string(text).expect("strings serialize");
    let list: Vec<String> = amounts(get("list_reagents"))
        .unwrap_or_default()
        .iter()
        .map(|(p, a)| format!("(path: {}, amount: {a:?})", s(p)))
        .collect();
    let c = SMOKABLE_COMPONENT;
    vec![
        // DM times are deciseconds.
        ext(c, "smoketime", format!("{:?}", n("smoketime") / 10.0)),
        ext(c, "dragtime", format!("{:?}", n("dragtime") / 10.0)),
        ext(c, "chem_volume", format!("{:?}", n("chem_volume"))),
        ext(c, "list_reagents", format!("[{}]", list.join(", "))),
        ext(c, "smoke_all", truthy(get("smoke_all")).to_string()),
        ext(c, "lung_harm", format!("{:?}", n("lung_harm"))),
        ext(c, "heat", format!("{:?}", n("heat"))),
        ext(c, "type_butt", s(&string(get("type_butt")))),
        ext(c, "icon_on", s(&string(get("icon_on")))),
        ext(c, "icon_off", s(&string(get("icon_off")))),
        ext(c, "inhand_icon_on", s(&string(get("inhand_icon_on")))),
        ext(c, "inhand_icon_off", s(&string(get("inhand_icon_off")))),
        ext(c, "starts_lit", truthy(get("starts_lit")).to_string()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cigarette_vars() {
        let types = DmTypes::parse(
            br##"{ "schema": 3, "types": {
            "/datum": { "parent": null },
            "/atom": { "parent": "/datum" },
            "/obj": { "parent": "/atom" },
            "/obj/item": { "parent": "/obj", "declared_vars": { "heat": 0 } },
            "/obj/item/cigarette": { "parent": "/obj/item", "declared_vars": {
                "heat": 1000, "smoketime": 3600, "dragtime": 100, "chem_volume": 30,
                "list_reagents": { "list": [ { "assoc": [
                    { "path": "/datum/reagent/drug/nicotine" }, 15 ] } ] },
                "smoke_all": 0, "lung_harm": 1, "type_butt": { "path": "/obj/item/cigbutt" },
                "icon_on": "cigon", "icon_off": "cigoff", "inhand_icon_on": "cigon",
                "inhand_icon_off": "cigoff", "starts_lit": 0 } },
            "/obj/item/cigarette/pipe": { "parent": "/obj/item/cigarette", "declared_vars": {
                "smoketime": 0, "list_reagents": null, "inhand_icon_on": null } }
            } }"##,
        )
        .unwrap();
        let cig = format!("{:?}", type_fields(&types, "/obj/item/cigarette"));
        assert!(cig.contains("360.0"), "{cig}");
        assert!(cig.contains("/datum/reagent/drug/nicotine"), "{cig}");
        assert!(cig.contains("/obj/item/cigbutt"), "{cig}");
        let pipe = format!("{:?}", type_fields(&types, "/obj/item/cigarette/pipe"));
        assert!(pipe.contains("list_reagents\", ron: \"[]\""), "{pipe}");
        assert!(type_fields(&types, "/obj/item").is_empty());
    }
}
