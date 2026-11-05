//! Service items as prototype data: a paper's starting text (`/obj/item/paper`
//! `default_raw_text`, `code/modules/paperwork/paper.dm`), a book's contents (`/obj/item/book`
//! `starting_title`/`starting_author`/`starting_content`, `code/modules/library/book.dm`) and a
//! die's sides (`/obj/item/dice` `sides`, `code/game/objects/items/dice.dm`), as the
//! [`DmService`] component. Map var edits of those vars apply too.

use bevy::prelude::*;
use serde_json::Value;

use crate::convert::DmTypes;
use crate::fidelity::{ext, is_under};
use crate::value::{Field, Skip, Val};

pub const SERVICE_COMPONENT: &str = "dm_service";
pub const PAPER_ROOT: &str = "/obj/item/paper";
pub const BOOK_ROOT: &str = "/obj/item/book";
pub const DICE_ROOT: &str = "/obj/item/dice";

/// The null rod's forms the chaplain picks from (`init_nullrod_variants`: the
/// `chaplain_spawnable` `/obj/item/nullrod` types, in `typesof` order).
// shortcut: the non-rod forms (carp plushie, divine bow, vorpal scythe shard, holy skateboard,
// Hanzo katana sheath, energy swords) are left out until their mechanics are ported.
pub const NULLROD_VARIANTS: [&str; 31] = [
    "/obj/item/nullrod",
    "/obj/item/nullrod/armblade",
    "/obj/item/nullrod/armblade/tentacle",
    "/obj/item/nullrod/bostaff",
    "/obj/item/nullrod/chainsaw",
    "/obj/item/nullrod/claymore",
    "/obj/item/nullrod/claymore/chainsaw_sword",
    "/obj/item/nullrod/claymore/darkblade",
    "/obj/item/nullrod/claymore/glowing",
    "/obj/item/nullrod/claymore/heretic",
    "/obj/item/nullrod/claymore/multiverse",
    "/obj/item/nullrod/claymore/spellblade",
    "/obj/item/nullrod/claymore/talking",
    "/obj/item/nullrod/clown",
    "/obj/item/nullrod/egyptian",
    "/obj/item/nullrod/fedora",
    "/obj/item/nullrod/godhand",
    "/obj/item/nullrod/hammer",
    "/obj/item/nullrod/hypertool",
    "/obj/item/nullrod/nullblade",
    "/obj/item/nullrod/pitchfork",
    "/obj/item/nullrod/pride_hammer",
    "/obj/item/nullrod/scythe",
    "/obj/item/nullrod/scythe/talking",
    "/obj/item/nullrod/sord",
    "/obj/item/nullrod/spear",
    "/obj/item/nullrod/staff",
    "/obj/item/nullrod/staff/blue",
    "/obj/item/nullrod/tribal_knife",
    "/obj/item/nullrod/vibro",
    "/obj/item/nullrod/whip",
];

/// The armaments beacon's kits (`typesof(/obj/item/storage/box/holy)`).
pub const HOLY_ARMOR_KITS: [&str; 9] = [
    "/obj/item/storage/box/holy",
    "/obj/item/storage/box/holy/adept",
    "/obj/item/storage/box/holy/clock",
    "/obj/item/storage/box/holy/divine_archer",
    "/obj/item/storage/box/holy/follower",
    "/obj/item/storage/box/holy/heretic",
    "/obj/item/storage/box/holy/sentinel",
    "/obj/item/storage/box/holy/student",
    "/obj/item/storage/box/holy/witchhunter",
];

/// The machine god's blessings (`/datum/religion_rites/machine_blessing`).
pub const MACHINE_BLESSINGS: [&str; 13] = [
    "/obj/item/organ/cyberimp/arm/toolkit/combat",
    "/obj/item/organ/cyberimp/arm/toolkit/surgery",
    "/obj/item/organ/cyberimp/arm/toolkit/toolset",
    "/obj/item/organ/cyberimp/eyes/hud/diagnostic",
    "/obj/item/organ/cyberimp/eyes/hud/medical",
    "/obj/item/organ/eyes/robotic/glow",
    "/obj/item/organ/eyes/robotic/shield",
    "/obj/item/organ/cyberimp/chest/reviver",
    "/obj/item/organ/cyberimp/chest/thrusters",
    "/obj/item/organ/cyberimp/brain/anti_drop",
    "/obj/item/organ/cyberimp/brain/connector",
    "/obj/item/organ/cyberimp/brain/anti_stun",
    "/obj/item/organ/cyberimp/mouth/breathing_tube",
];

/// What religion makes at runtime (`ss13_game::service` religion): the chaplain's bible, the
/// altar a bible unpacks into, the armaments beacon, the immortal candles and the greedy
/// god's vendor.
pub const RELIGION_RUNTIME_TYPES: [&str; 6] = [
    "/obj/item/book/bible/booze",
    "/obj/structure/altar/of_gods",
    "/obj/item/choice_beacon/holy",
    "/obj/item/flashlight/flare/candle/infinite",
    "/obj/machinery/vending/custom/greed",
    "/obj/item/nullrod",
];

/// One paper, book or die type (prototype data).
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmService {
    /// `default_raw_text` of a paper, `starting_content` of a book (raw markdown/HTML).
    pub text: String,
    /// `starting_title` of a book.
    pub title: String,
    /// `starting_author` of a book.
    pub author: String,
    /// `sides` of a die.
    pub sides: u32,
}

/// (root type, var, [`DmService`] field).
const VARS: [(&str, &str, &str); 5] = [
    (PAPER_ROOT, "default_raw_text", "text"),
    (BOOK_ROOT, "starting_content", "text"),
    (BOOK_ROOT, "starting_title", "title"),
    (BOOK_ROOT, "starting_author", "author"),
    (DICE_ROOT, "sides", "sides"),
];

fn str_ron(s: &str) -> String {
    ron::to_string(s).expect("strings always serialize")
}

/// A dumped value as the field's RON; strings that are expressions (`"[station_name()]"`)
/// become empty.
fn json_ron(key: &str, value: Option<&Value>) -> String {
    if key == "sides" {
        let n = value.and_then(Value::as_f64).unwrap_or(6.0);
        return (n.max(1.0) as u32).to_string();
    }
    str_ron(value.and_then(Value::as_str).unwrap_or_default())
}

/// Converter fields of `path`: the vars it declares; the roots also get what they inherit.
pub fn type_fields(types: &DmTypes, path: &str) -> Vec<Field> {
    let Some(ty) = types.types.get(path) else {
        return Vec::new();
    };
    VARS.iter()
        .filter(|(root, ..)| is_under(path, root))
        .filter_map(|&(root, var, key)| {
            let value = if path == root {
                types.resolved(path, var)
            } else {
                Some(ty.declared_vars.get(var)?)
            };
            Some(ext(SERVICE_COMPONENT, key, json_ron(key, value)))
        })
        .collect()
}

/// `.dmm` var edits of those vars.
pub fn ext_field(path: &str, var: &str, value: Val) -> Result<Field, Skip> {
    let Some(&(_, _, key)) = VARS
        .iter()
        .find(|(root, v, _)| *v == var && is_under(path, root))
    else {
        return Err(Skip::UnknownVar);
    };
    let ron = match (key, value) {
        ("sides", Val::Num(n)) => (n.max(1.0) as u32).to_string(),
        ("sides", _) => return Err(Skip::Unsupported),
        (_, Val::Null) => str_ron(""),
        (_, Val::Str(s)) => str_ron(s),
        _ => return Err(Skip::Unsupported),
    };
    Ok(ext(SERVICE_COMPONENT, key, ron))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TYPES: &str = r##"{ "schema": 3, "types": {
        "/datum": { "parent": null },
        "/atom": { "parent": "/datum" },
        "/obj": { "parent": "/atom" },
        "/obj/item": { "parent": "/obj" },
        "/obj/item/paper": { "parent": "/obj/item", "declared_vars": { "default_raw_text": null } },
        "/obj/item/paper/fluff/sop": { "parent": "/obj/item/paper",
            "declared_vars": { "default_raw_text": "Rule one" } },
        "/obj/item/paper/fluff/station": { "parent": "/obj/item/paper",
            "declared_vars": { "default_raw_text": { "expr": "\"[station_name()]\"" } } },
        "/obj/item/dice": { "parent": "/obj/item", "declared_vars": { "sides": 6 } },
        "/obj/item/dice/d20": { "parent": "/obj/item/dice", "declared_vars": { "sides": 20 } }
    } }"##;

    fn ron_of(fields: &[Field], key: &str) -> Option<String> {
        fields.iter().find_map(|f| match f {
            Field::Ext { key: k, ron, .. } if *k == key => Some(ron.clone()),
            _ => None,
        })
    }

    #[test]
    fn roots_carry_the_component_and_subtypes_their_own_vars() {
        let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
        let root = type_fields(&types, "/obj/item/paper");
        assert_eq!(ron_of(&root, "text").as_deref(), Some("\"\""));
        let sop = type_fields(&types, "/obj/item/paper/fluff/sop");
        assert_eq!(ron_of(&sop, "text").as_deref(), Some("\"Rule one\""));
        let station = type_fields(&types, "/obj/item/paper/fluff/station");
        assert_eq!(ron_of(&station, "text").as_deref(), Some("\"\""));
        let d20 = type_fields(&types, "/obj/item/dice/d20");
        assert_eq!(ron_of(&d20, "sides").as_deref(), Some("20"));
        assert!(type_fields(&types, "/obj/item").is_empty());
    }

    #[test]
    fn var_edits_set_text_and_sides() {
        let f = ext_field("/obj/item/paper", "default_raw_text", Val::Str("hi")).unwrap();
        assert_eq!(ron_of(&[f], "text").as_deref(), Some("\"hi\""));
        let f = ext_field("/obj/item/dice/d20", "sides", Val::Num(4.0)).unwrap();
        assert_eq!(ron_of(&[f], "sides").as_deref(), Some("4"));
        assert!(ext_field("/obj/item/book", "sides", Val::Num(4.0)).is_err());
    }
}
