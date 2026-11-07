//! Ruin puzzle pieces as prototype data (`code/game/objects/items/puzzle_pieces.dm`): keycards,
//! puzzle doors and the puzzle panels that open them. Mappers pair them by var edits
//! (`puzzle_id` on keycards and doors, `id` on panels), so every var below takes map edits.

use bevy::prelude::*;

use crate::convert::DmTypes;
use crate::fidelity::{ext, is_under};
use crate::value::{Field, Skip, Val};

pub const KEYCARD_ROOT: &str = "/obj/item/keycard";
pub const PUZZLE_DOOR_ROOT: &str = "/obj/machinery/door/puzzle";
pub const KEYCARD_DOOR_ROOT: &str = "/obj/machinery/door/puzzle/keycard";
pub const PUZZLE_MACHINE_ROOT: &str = "/obj/machinery/puzzle";
const ROOTS: [&str; 3] = [KEYCARD_ROOT, PUZZLE_DOOR_ROOT, PUZZLE_MACHINE_ROOT];
const COMPONENT: &str = "dm_puzzle";

/// The puzzle vars of a keycard, puzzle door or puzzle panel. A var the type does not declare
/// keeps BYOND's `null`/`0`.
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmPuzzle {
    pub path: String,
    /// Keycards and doors: `puzzle_id` (empty: `null`).
    pub puzzle_id: String,
    /// Panels: `id`, the puzzle they complete.
    pub id: String,
    /// Doors: `open_message`, `uses_queuelinks`.
    pub open_message: String,
    pub uses_queuelinks: bool,
    /// Panels: `single_use`, `base_icon_state`.
    pub single_use: bool,
    pub base_icon_state: String,
}

#[derive(Clone, Copy)]
enum Kind {
    Text,
    Flag,
}

const VARS: [(&str, Kind); 6] = [
    ("puzzle_id", Kind::Text),
    ("id", Kind::Text),
    ("open_message", Kind::Text),
    ("uses_queuelinks", Kind::Flag),
    ("single_use", Kind::Flag),
    ("base_icon_state", Kind::Text),
];

fn is_puzzle(path: &str) -> bool {
    ROOTS.iter().any(|root| is_under(path, root))
}

fn ron_of(kind: Kind, value: Val) -> Result<String, Skip> {
    match kind {
        Kind::Text => {
            let text = crate::door::link_id(value)?;
            Ok(ron::to_string(&text).expect("strings always serialize"))
        }
        Kind::Flag => match value {
            Val::Num(n) => Ok((n != 0.0).to_string()),
            Val::Null => Ok("false".into()),
            _ => Err(Skip::Unsupported),
        },
    }
}

/// Converter fields: every puzzle type carries the vars its chain declares.
pub fn type_fields(types: &DmTypes, path: &str) -> Vec<Field> {
    if !is_puzzle(path) || !types.types.contains_key(path) {
        return Vec::new();
    }
    let path_ron = ron::to_string(path).expect("strings always serialize");
    std::iter::once(ext(COMPONENT, "path", path_ron))
        .chain(VARS.iter().filter_map(|&(var, kind)| {
            let value = types.resolved(path, var)?;
            let ron = ron_of(kind, Val::from_json(value)).ok()?;
            Some(ext(COMPONENT, var, ron))
        }))
        .collect()
}

/// `.dmm` edits of the puzzle vars.
pub fn ext_field(path: &str, var: &str, value: Val) -> Result<Field, Skip> {
    let Some(&(key, kind)) = VARS.iter().find(|(v, _)| *v == var) else {
        return Err(Skip::UnknownVar);
    };
    if !is_puzzle(path) {
        return Err(Skip::UnknownVar);
    }
    Ok(ext(COMPONENT, key, ron_of(kind, value)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keycards_and_doors_pair_by_map_edits() {
        assert_eq!(
            ext_field("/obj/item/keycard", "puzzle_id", Val::Str("vault")).ok(),
            Some(ext(COMPONENT, "puzzle_id", "\"vault\""))
        );
        assert_eq!(
            ext_field(KEYCARD_DOOR_ROOT, "puzzle_id", Val::Null).ok(),
            Some(ext(COMPONENT, "puzzle_id", "\"\""))
        );
        assert_eq!(
            ext_field("/obj/machinery/puzzle/keycardpad", "id", Val::Num(3.0)).ok(),
            Some(ext(COMPONENT, "id", "\"3\""))
        );
        assert!(ext_field("/obj/item/card/id", "puzzle_id", Val::Str("x")).is_err());
        assert!(ext_field(KEYCARD_ROOT, "name", Val::Str("x")).is_err());
    }

    #[test]
    fn types_carry_their_declared_vars() {
        let types = DmTypes::parse(
            br#"{"schema": 3, "types": {
                "/atom": { "parent": null },
                "/obj": { "parent": "/atom" },
                "/obj/item": { "parent": "/obj" },
                "/obj/item/keycard": { "parent": "/obj/item",
                    "declared_vars": { "puzzle_id": null } },
                "/obj/item/keycard/yellow": { "parent": "/obj/item/keycard",
                    "declared_vars": { "puzzle_id": "yellow" } }
            }}"#,
        )
        .expect("parses");
        let fields = type_fields(&types, "/obj/item/keycard/yellow");
        assert!(fields.contains(&ext(COMPONENT, "puzzle_id", "\"yellow\"")));
        assert!(fields.contains(&ext(COMPONENT, "path", "\"/obj/item/keycard/yellow\"")));
        assert!(type_fields(&types, "/obj/item").is_empty());
    }
}
