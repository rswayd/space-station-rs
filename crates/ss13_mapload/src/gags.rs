//! GAGS types point at their rendered icons (`ss13_assets::gags`, rendered by `ss13-import`):
//! tg `update_greyscale` replaces `icon` (and an item's `worn_icon`, `lefthand_file`,
//! `righthand_file`) with the config rendered in `greyscale_colors`, and `Initialize` then sets
//! `icon_state = post_init_icon_state` (the source's `icon_state` names the map preview).
//! A type without a config keeps its `icon`; `load::map_icon_preview` covers those.
// shortcut: runtime recoloring (`set_greyscale`: spray paint, material skins, the greyscale menu)
// and map var edits of `greyscale_colors` (two carp plushies) keep the type's colors; render
// such keys on demand when something recolors at runtime.

use ss13_assets::gags::{CONFIG_VARS, colors, config_path, icon_key};

use crate::convert::DmTypes;
use crate::fidelity::{ext, is_under};
use crate::item::ITEM_ROOT;
use crate::value::Field;

/// Every type with GAGS gets its fields (not only those whose key differs from the parent's):
/// a declared `icon` or `icon_state` below would otherwise win over the inherited ones.
pub fn type_fields(types: &DmTypes, path: &str) -> Vec<Field> {
    let mut out = Vec::new();
    let Some(colors) = types.resolved(path, "greyscale_colors").and_then(colors) else {
        return out;
    };
    let item = is_under(path, ITEM_ROOT);
    for (var, target) in CONFIG_VARS {
        let Some(config) = types.resolved(path, var).and_then(config_path) else {
            continue;
        };
        let key = icon_key(config, colors);
        match target {
            "icon" => {
                out.push(Field::Icon(key));
                if let Some(state) = types
                    .resolved(path, "post_init_icon_state")
                    .and_then(serde_json::Value::as_str)
                    .filter(|s| !s.is_empty())
                {
                    out.push(Field::IconState(state.to_owned()));
                }
            }
            "worn_icon" if item => out.push(ext("dm_item", "worn_icon", ron_str(&key))),
            "lefthand_file" if item => out.push(ext("dm_item", "lefthand_file", ron_str(&key))),
            "righthand_file" if item => out.push(ext("dm_item", "righthand_file", ron_str(&key))),
            _ => {}
        }
    }
    out
}

fn ron_str(s: &str) -> String {
    ron::to_string(s).expect("strings always serialize")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gags_items_point_at_rendered_icons() {
        let types = DmTypes::parse(
            br##"{"schema": 3, "types": {
            "/obj/item/clothing/under/color": {"parent": null, "declared_vars": {
                "icon": {"resource": "icons/map_icons/clothing/under/color.dmi"},
                "icon_state": "/obj/item/clothing/under/color",
                "post_init_icon_state": "jumpsuit",
                "greyscale_config": {"path": "/datum/greyscale_config/jumpsuit"},
                "greyscale_config_worn": {"path": "/datum/greyscale_config/jumpsuit/worn"},
                "greyscale_colors": "#3f3f3f"}},
            "/obj/item/clothing/under/color/grey": {"parent": "/obj/item/clothing/under/color",
                "declared_vars": {"greyscale_colors": "#b3b3b3"}},
            "/obj/item/clothing/under/plain": {"parent": "/obj/item/clothing/under/color",
                "declared_vars": {"greyscale_colors": null}}
        }}"##,
        )
        .unwrap();
        let fields = type_fields(&types, "/obj/item/clothing/under/color/grey");
        assert_eq!(
            fields,
            [
                Field::Icon("gags:/datum/greyscale_config/jumpsuit#b3b3b3".into()),
                Field::IconState("jumpsuit".into()),
                ext(
                    "dm_item",
                    "worn_icon",
                    "\"gags:/datum/greyscale_config/jumpsuit/worn#b3b3b3\""
                ),
            ]
        );
        assert!(type_fields(&types, "/obj/item/clothing/under/plain").is_empty());
    }
}
