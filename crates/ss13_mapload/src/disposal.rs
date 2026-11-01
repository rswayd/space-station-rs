//! Disposals as prototype data (`code/modules/recycling/disposal/`): the sort tags of mail
//! sorting junctions. A junction's own `sortType` (a number, or numbers joined by `;` like
//! access strings, `pipe_sorting.dm` `Initialize`) and the `sort_type` of a mail sorting
//! mapping helper (`code/modules/mapping/mail_sorting_helpers.dm`, which adds it to the
//! junction on its tile in `LateInitialize`) both become [`DmMailSort`]. Map var edits of
//! `sortType` on junctions are supported.

use bevy::prelude::*;
use serde_json::Value;

use crate::convert::DmTypes;
use crate::fidelity::{ext, is_under};
use crate::value::{Field, Skip, Val};

pub const MAIL_SORTING_ROOT: &str = "/obj/structure/disposalpipe/sorting/mail";
pub const MAIL_HELPER_ROOT: &str = "/obj/effect/mapping_helpers/mail_sorting";
const COMPONENT: &str = "dm_mail_sort";

/// Parcels made at runtime by the package wrapper (`/obj/item/stack/package_wrap`).
pub const DISPOSAL_RUNTIME_TYPES: [&str; 1] = ["/obj/item/delivery/small"];

/// Sort tags (`GLOB.TAGGERLOCATIONS` indexes) of a mail junction or a mail sorting helper.
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmMailSort {
    pub sort_types: Vec<u8>,
}

/// `sortType` as `Initialize` reads it: a number, or `;`-separated numbers (`text2num`, zeros
/// and garbage dropped, duplicates kept once).
pub fn parse_sort_type(value: Val) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    let mut add = |n: f64| {
        if n != 0.0 && (1.0..=255.0).contains(&n) && !out.contains(&(n as u8)) {
            out.push(n as u8);
        }
    };
    match value {
        Val::Num(n) => add(n),
        Val::Str(s) => {
            for part in s.split(';') {
                if let Ok(n) = part.trim().parse::<f64>() {
                    add(n);
                }
            }
        }
        _ => {}
    }
    out
}

fn list_ron(tags: &[u8]) -> String {
    format!(
        "[{}]",
        tags.iter()
            .map(u8::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    )
}

/// Converter fields: every mail junction and mail sorting helper type carries its tags.
pub fn type_fields(types: &DmTypes, path: &str) -> Vec<Field> {
    let var = if is_under(path, MAIL_SORTING_ROOT) {
        "sortType"
    } else if is_under(path, MAIL_HELPER_ROOT) {
        "sort_type"
    } else {
        return Vec::new();
    };
    if !types.types.contains_key(path) {
        return Vec::new();
    }
    let tags = match types.resolved(path, var) {
        Some(Value::Number(n)) => parse_sort_type(n.as_f64().map_or(Val::Other, Val::Num)),
        Some(Value::String(s)) => parse_sort_type(Val::Str(s)),
        _ => Vec::new(),
    };
    vec![ext(COMPONENT, "sort_types", list_ron(&tags))]
}

/// `.dmm` edits of `sortType` on mail junctions.
pub fn ext_field(path: &str, var: &str, value: Val) -> Result<Field, Skip> {
    if var != "sortType" || !is_under(path, MAIL_SORTING_ROOT) {
        return Err(Skip::UnknownVar);
    }
    match value {
        Val::Num(_) | Val::Str(_) | Val::Null => Ok(ext(
            COMPONENT,
            "sort_types",
            list_ron(&parse_sort_type(value)),
        )),
        _ => Err(Skip::Unsupported),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sort_type_strings_split_like_initialize() {
        assert_eq!(parse_sort_type(Val::Num(9.0)), vec![9]);
        assert_eq!(parse_sort_type(Val::Num(0.0)), Vec::<u8>::new());
        assert_eq!(parse_sort_type(Val::Str("2;3;x;0;3")), vec![2, 3]);
        assert_eq!(parse_sort_type(Val::Null), Vec::<u8>::new());
    }

    #[test]
    fn helpers_and_junctions_carry_their_tags() {
        let types = DmTypes::parse(
            br#"{"schema": 3, "types": {
                "/atom": { "parent": null },
                "/obj": { "parent": "/atom" },
                "/obj/effect": { "parent": "/obj" },
                "/obj/effect/mapping_helpers": { "parent": "/obj/effect" },
                "/obj/effect/mapping_helpers/mail_sorting": {
                    "parent": "/obj/effect/mapping_helpers", "declared_vars": { "sort_type": 0 } },
                "/obj/effect/mapping_helpers/mail_sorting/medbay": {
                    "parent": "/obj/effect/mapping_helpers/mail_sorting",
                    "declared_vars": { "sort_type": 9 } },
                "/obj/structure": { "parent": "/obj" },
                "/obj/structure/disposalpipe": { "parent": "/obj/structure" },
                "/obj/structure/disposalpipe/sorting": { "parent": "/obj/structure/disposalpipe" },
                "/obj/structure/disposalpipe/sorting/mail": {
                    "parent": "/obj/structure/disposalpipe/sorting",
                    "declared_vars": { "sortType": "2;3" } }
            }}"#,
        )
        .unwrap();
        let ron = |path| match &type_fields(&types, path)[..] {
            [Field::Ext { ron, .. }] => ron.clone(),
            other => panic!("{other:?}"),
        };
        assert_eq!(
            ron("/obj/effect/mapping_helpers/mail_sorting/medbay"),
            "[9]"
        );
        assert_eq!(ron("/obj/structure/disposalpipe/sorting/mail"), "[2, 3]");
        assert!(type_fields(&types, "/obj/structure/disposalpipe/sorting").is_empty());
        assert!(matches!(
            ext_field(MAIL_SORTING_ROOT, "sortType", Val::Num(5.0)),
            Ok(Field::Ext { ref ron, .. }) if ron == "[5]"
        ));
        assert!(
            ext_field(
                "/obj/structure/disposalpipe/segment",
                "sortType",
                Val::Num(5.0)
            )
            .is_err()
        );
    }
}
