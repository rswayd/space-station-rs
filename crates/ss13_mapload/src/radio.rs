//! DM radios as prototype data: the vars of `/obj/item/radio` and its subtypes (headsets,
//! intercoms, station bounced radios) that talking and receiving read
//! (`code/game/objects/items/devices/radio/`). The channels of the encryption keys a type starts
//! with (`keyslot`, a headset's `keyslot2`) are resolved here, at conversion time, as tg's
//! `recalculateChannels()` merges them. Only radio types get [`DmRadio`].
//!
//! Map var edits of `frequency`, `broadcasting`, `listening`, `freerange`, `on`,
//! `canhear_range` and `subspace_transmission` are supported; `keyslot` edits are not.

use bevy::prelude::*;
use serde_json::Value;

use crate::convert::DmTypes;
use crate::fidelity::{ext, is_under, num, truthy};
use crate::value::{Field, Skip, Val};

pub const RADIO_ROOT: &str = "/obj/item/radio";
pub const HEADSET_ROOT: &str = "/obj/item/radio/headset";
pub const INTERCOM_ROOT: &str = "/obj/item/radio/intercom";

const COMPONENT: &str = "dm_radio";

/// One entry of tg's `channels` list: a channel name and whether the radio listens to it
/// (`FREQ_LISTENING`; a key may grant a channel switched off, like the captain's).
#[derive(Reflect, Default, Clone, Debug, PartialEq)]
pub struct RadioChannelVar {
    pub name: String,
    pub listening: bool,
}

/// The radio vars of a DM type (type defaults + map var edits), prototype data like
/// [`DmAtom`](crate::DmAtom). The game builds its live radio from it and the type's
/// `Initialize()`.
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmRadio {
    pub path: String,
    pub on: bool,
    pub frequency: f64,
    pub broadcasting: bool,
    pub listening: bool,
    pub canhear_range: f64,
    pub subspace_transmission: bool,
    pub freerange: bool,
    /// `special_channels` of the radio and its keys (`RADIO_SPECIAL_*`).
    pub special_channels: u8,
    /// The keys' channels in tg order (`keyslot` first, then `keyslot2`).
    pub channels: Vec<RadioChannelVar>,
}

#[derive(Clone, Copy)]
enum Kind {
    Num,
    Flag,
}

const VARS: [(&str, Kind); 7] = [
    ("on", Kind::Flag),
    ("frequency", Kind::Num),
    ("broadcasting", Kind::Flag),
    ("listening", Kind::Flag),
    ("canhear_range", Kind::Num),
    ("subspace_transmission", Kind::Flag),
    ("freerange", Kind::Flag),
];

/// Converter fields of `path`: every radio type carries its resolved vars and channels, so a
/// subtype's keys never depend on the parent's patch.
pub fn type_fields(types: &DmTypes, path: &str) -> Vec<Field> {
    let mut out = Vec::new();
    if !is_under(path, RADIO_ROOT) || !types.types.contains_key(path) {
        return out;
    }
    out.push(ext(COMPONENT, "path", str_ron(path)));
    for (var, kind) in VARS {
        if let Some(ron) = types.resolved(path, var).and_then(|v| json_ron(kind, v)) {
            out.push(ext(COMPONENT, var, ron));
        }
    }
    let (special, channels) = keys(types, path);
    out.push(ext(COMPONENT, "special_channels", special.to_string()));
    let channels: Vec<String> = channels
        .iter()
        .map(|c| format!("(name: {}, listening: {})", str_ron(&c.name), c.listening))
        .collect();
    out.push(ext(
        COMPONENT,
        "channels",
        format!("[{}]", channels.join(", ")),
    ));
    out
}

/// tg `recalculateChannels()`: the keys' channels, the first key's entry winning for a channel
/// both grant, and `special_channels` of the radio and its keys.
fn keys(types: &DmTypes, path: &str) -> (u8, Vec<RadioChannelVar>) {
    let mut special = num(types.resolved(path, "special_channels")).unwrap_or(0.0) as u8;
    let mut channels: Vec<RadioChannelVar> = Vec::new();
    let slots: &[&str] = if is_under(path, HEADSET_ROOT) {
        &["keyslot", "keyslot2"]
    } else {
        &["keyslot"]
    };
    for slot in slots {
        let Some(key) = types
            .resolved(path, slot)
            .and_then(|v| v.get("path"))
            .and_then(Value::as_str)
        else {
            continue;
        };
        special |= num(types.resolved(key, "special_channels")).unwrap_or(0.0) as u8;
        let list = types
            .resolved(key, "channels")
            .and_then(|v| v.get("list"))
            .and_then(Value::as_array);
        for item in list.into_iter().flatten() {
            let (name, flag) = match item.get("assoc").and_then(Value::as_array) {
                Some(pair) if pair.len() == 2 => (pair[0].as_str(), truthy(Some(&pair[1]))),
                _ => (item.as_str(), false),
            };
            if let Some(name) = name
                && !channels.iter().any(|c| c.name == name)
            {
                channels.push(RadioChannelVar {
                    name: name.to_owned(),
                    listening: flag,
                });
            }
        }
    }
    (special, channels)
}

fn json_ron(kind: Kind, value: &Value) -> Option<String> {
    match kind {
        Kind::Num => num(Some(value)).map(|n| format!("{n:?}")),
        Kind::Flag => Some(truthy(Some(value)).to_string()),
    }
}

/// `.dmm` var edits of radio vars.
pub fn ext_field(path: &str, var: &str, value: Val) -> Result<Field, Skip> {
    let Some(&(key, kind)) = VARS.iter().find(|(v, _)| *v == var) else {
        return Err(Skip::UnknownVar);
    };
    if !is_under(path, RADIO_ROOT) {
        return Err(Skip::UnknownVar);
    }
    let ron = match (kind, value) {
        (Kind::Num, Val::Num(n)) => format!("{n:?}"),
        (Kind::Num, Val::Null) => "0.0".to_owned(),
        (Kind::Flag, Val::Num(n)) => (n != 0.0).to_string(),
        (Kind::Flag, Val::Null) => "false".to_owned(),
        _ => return Err(Skip::Unsupported),
    };
    Ok(ext(COMPONENT, key, ron))
}

fn str_ron(s: &str) -> String {
    ron::to_string(s).expect("strings always serialize")
}

#[cfg(test)]
mod tests {
    use super::*;

    const TYPES: &str = r##"{
      "schema": 3,
      "types": {
        "/atom": { "parent": null },
        "/obj": { "parent": "/atom" },
        "/obj/item": { "parent": "/obj" },
        "/obj/item/encryptionkey": { "parent": "/obj/item", "declared_vars": {
            "channels": { "list": [] }, "special_channels": 0 } },
        "/obj/item/encryptionkey/headset_eng": { "parent": "/obj/item/encryptionkey",
            "declared_vars": { "channels": { "list": [{ "assoc": ["Инженерия", 1] }] } } },
        "/obj/item/encryptionkey/headset_com": { "parent": "/obj/item/encryptionkey",
            "declared_vars": { "channels": { "list": [{ "assoc": ["Командование", 1] },
                { "assoc": ["Инженерия", 0] }] } } },
        "/obj/item/encryptionkey/syndicate": { "parent": "/obj/item/encryptionkey",
            "declared_vars": { "channels": { "list": [{ "assoc": ["Синдикат", 1] }] },
                "special_channels": 1 } },
        "/obj/item/radio": { "parent": "/obj/item", "declared_vars": { "on": 1,
            "frequency": 1459, "broadcasting": 0, "listening": 1, "canhear_range": 3,
            "subspace_transmission": 0, "freerange": 0, "keyslot": null, "special_channels": 0 } },
        "/obj/item/radio/headset": { "parent": "/obj/item/radio", "declared_vars": {
            "canhear_range": 0, "subspace_transmission": 1, "keyslot2": null } },
        "/obj/item/radio/headset/combo": { "parent": "/obj/item/radio/headset", "declared_vars": {
            "keyslot": { "path": "/obj/item/encryptionkey/headset_eng" },
            "keyslot2": { "path": "/obj/item/encryptionkey/headset_com" } } },
        "/obj/item/radio/headset/syndicate": { "parent": "/obj/item/radio/headset",
            "declared_vars": { "keyslot": { "path": "/obj/item/encryptionkey/syndicate" } } },
        "/obj/item/radio/intercom": { "parent": "/obj/item/radio", "declared_vars": {
            "canhear_range": 2 } }
      }
    }"##;

    fn get<'a>(fields: &'a [Field], key: &str) -> Option<&'a str> {
        fields.iter().find_map(|f| match f {
            Field::Ext { key: k, ron, .. } if *k == key => Some(ron.as_str()),
            _ => None,
        })
    }

    #[test]
    fn keys_merge_like_recalculate_channels() {
        let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
        let combo = type_fields(&types, "/obj/item/radio/headset/combo");
        // The first key's engineering entry (listening) wins over the second key's.
        assert_eq!(
            get(&combo, "channels"),
            Some(
                r#"[(name: "Инженерия", listening: true), (name: "Командование", listening: true)]"#
            )
        );
        assert_eq!(get(&combo, "canhear_range"), Some("0.0"));
        assert_eq!(get(&combo, "subspace_transmission"), Some("true"));
        assert_eq!(get(&combo, "frequency"), Some("1459.0"));
        let syndie = type_fields(&types, "/obj/item/radio/headset/syndicate");
        assert_eq!(get(&syndie, "special_channels"), Some("1"));
        let intercom = type_fields(&types, "/obj/item/radio/intercom");
        assert_eq!(get(&intercom, "channels"), Some("[]"));
        assert_eq!(get(&intercom, "canhear_range"), Some("2.0"));
        assert!(type_fields(&types, "/obj/item/encryptionkey/headset_eng").is_empty());
    }

    #[test]
    fn map_edits() {
        let p = "/obj/item/radio/intercom/directional/north";
        assert_eq!(
            ext_field(p, "frequency", Val::Num(1447.0)),
            Ok(ext(COMPONENT, "frequency", "1447.0"))
        );
        assert_eq!(
            ext_field(p, "broadcasting", Val::Num(1.0)),
            Ok(ext(COMPONENT, "broadcasting", "true"))
        );
        assert_eq!(
            ext_field("/obj/machinery/door", "frequency", Val::Num(1.0)),
            Err(Skip::UnknownVar)
        );
        assert_eq!(
            ext_field(p, "listening", Val::Other),
            Err(Skip::Unsupported)
        );
    }
}
