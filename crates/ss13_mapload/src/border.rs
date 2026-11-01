//! tg border objects (`flags_1 & ON_BORDER_1`: directional windows, windoors, railings,
//! directional firelocks) as prototype data. The loader turns [`DmBorder`] and the atom's `dir`
//! into `ss13_world::Border`.

use bevy::prelude::*;
use ss13_world::Border;

use crate::convert::DmTypes;
use crate::fidelity::{ext, num};
use crate::value::Field;

/// `code/__DEFINES/_flags.dm`.
pub const ON_BORDER_1: u32 = 1 << 1;
/// Railings test `border_dir & dir` instead of `==` (`railings.dm`), so diagonal ones block two
/// edges; their subtypes (platforms) inherit it.
pub const RAILING_ROOT: &str = "/obj/structure/railing";

const COMPONENT: &str = "dm_border";

/// Only types that are, or descend from, a border type carry it.
#[derive(Component, Reflect, Default, Clone, Copy, Debug, PartialEq)]
pub struct DmBorder {
    pub on_border: bool,
    /// Blocks every edge in its `dir` bits (railings), not only a cardinal `dir`.
    pub any_bit: bool,
}

impl DmBorder {
    /// The live border of an atom facing `dir` (BYOND bits); `None` when not a border object.
    pub fn border(self, dir: u8) -> Option<Border> {
        let dir = ss13_shared::Dir::from_bits(dir).unwrap_or_default();
        self.on_border.then(|| Border::facing(dir, self.any_bit))
    }
}

/// Converter fields of `path`: `on_border` where its declared `flags_1` changes the border bit
/// against its parent (so types that never are border objects get no component), `any_bit` on
/// the railing root.
// shortcut: `flags_1` var edits in `.dmm` files are ignored; no shipped map edits the border bit.
pub fn type_fields(types: &DmTypes, path: &str) -> Vec<Field> {
    let mut out = Vec::new();
    let Some(ty) = types.types.get(path) else {
        return out;
    };
    let border = |v: Option<&serde_json::Value>| num(v).map(|n| n as u32 & ON_BORDER_1 != 0);
    if let Some(own) = ty
        .declared_vars
        .get("flags_1")
        .and_then(|v| border(Some(v)))
    {
        let inherited = ty
            .parent
            .as_deref()
            .and_then(|p| border(types.resolved(p, "flags_1")))
            .unwrap_or(false);
        if own != inherited {
            out.push(ext(COMPONENT, "on_border", own.to_string()));
        }
    }
    if path == RAILING_ROOT {
        out.push(ext(COMPONENT, "any_bit", "true"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn border_bit_changes_only() {
        let types = DmTypes::parse(
            br#"{"schema": 3, "types": {
            "/atom": {"parent": null, "declared_vars": {"flags_1": 0}},
            "/obj": {"parent": "/atom"},
            "/obj/structure": {"parent": "/obj", "declared_vars": {"flags_1": 0}},
            "/obj/structure/window": {"parent": "/obj/structure", "declared_vars": {"flags_1": 2}},
            "/obj/structure/window/tinted": {"parent": "/obj/structure/window",
                "declared_vars": {"flags_1": 3}},
            "/obj/structure/window/fulltile": {"parent": "/obj/structure/window",
                "declared_vars": {"flags_1": 8}},
            "/obj/structure/railing": {"parent": "/obj/structure", "declared_vars": {"flags_1": 2}}
        }}"#,
        )
        .unwrap();
        let on = |v: bool| ext(COMPONENT, "on_border", v.to_string());
        assert!(type_fields(&types, "/atom").is_empty());
        assert!(type_fields(&types, "/obj/structure").is_empty());
        assert_eq!(type_fields(&types, "/obj/structure/window"), [on(true)]);
        assert!(type_fields(&types, "/obj/structure/window/tinted").is_empty());
        assert_eq!(
            type_fields(&types, "/obj/structure/window/fulltile"),
            [on(false)]
        );
        assert_eq!(
            type_fields(&types, RAILING_ROOT),
            [on(true), ext(COMPONENT, "any_bit", "true")]
        );
        let pane = DmBorder {
            on_border: true,
            any_bit: false,
        };
        assert_eq!(pane.border(1), Some(Border { edges: 1 }));
        assert_eq!(pane.border(5), Some(Border { edges: 0 }));
        assert_eq!(DmBorder::default().border(1), None);
    }
}
