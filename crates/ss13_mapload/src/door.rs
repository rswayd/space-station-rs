//! DM doors as prototype data: the vars of `/obj/machinery/door` and its subtypes that door
//! behavior reads, plus the airlock mapping helpers that bolt or weld the airlock on their tile.
//!
//! Only door types get [`DmDoor`] (`locked`, `req_access`, ... mean other things on lockers and
//! machines), so callers check [`is_door`] before converting a var with [`field`].

use std::fmt::Write as _;

use bevy::prelude::*;
use ss13_assets::dmm::DmValue;

use crate::convert::DmTypes;
use crate::value::{Skip, Val, field as atom_field};

pub const DOOR_ROOT: &str = "/obj/machinery/door";

/// Kind roots: the converter sets `kind` on these types and their subtypes inherit it.
pub const KIND_ROOTS: [(&str, DoorKind); 5] = [
    (DOOR_ROOT, DoorKind::Basic),
    ("/obj/machinery/door/airlock", DoorKind::Airlock),
    ("/obj/machinery/door/window", DoorKind::Windoor),
    ("/obj/machinery/door/firedoor", DoorKind::Firedoor),
    ("/obj/machinery/door/poddoor", DoorKind::Poddoor),
];

/// tg `code/modules/mapping/mapping_helpers.dm` and `access_helpers.dm`: airlock helpers apply
/// their `payload` to the airlock on their tile at init and delete themselves.
pub const HELPER_LOCKED: &str = "/obj/effect/mapping_helpers/airlock/locked";
pub const HELPER_WELDED: &str = "/obj/effect/mapping_helpers/airlock/welded";
/// `unres` and `unres/delayed`: the helper's `dir` side of the airlock opens without access.
pub const HELPER_UNRES: &str = "/obj/effect/mapping_helpers/airlock/unres";
pub const HELPER_UNRES_DELAYED: &str = "/obj/effect/mapping_helpers/airlock/unres/delayed";
/// Access helpers: `get_access()` of the chain (dumped per type by `ss13_dm_extract`) goes to
/// `req_one_access` (`any`) or `req_access` (`all`).
pub const HELPER_ACCESS_ANY: &str = "/obj/effect/mapping_helpers/airlock/access/any";
pub const HELPER_ACCESS_ALL: &str = "/obj/effect/mapping_helpers/airlock/access/all";
/// `red_alert_access`: the airlock lifts its access at red alert and above.
pub const HELPER_RED_ALERT_ACCESS: &str = "/obj/effect/mapping_helpers/airlock/red_alert_access";
// shortcut: `cyclelink_helper`(_multi) (cycling airlock pairs), `abandoned` (random wall/weld/
// bolt per round) and the rarer airlock helpers are not applied; their entities stay inert. Add
// each with the mechanic it configures.

/// Which tg door family a type belongs to; decides timings, icon state names and who may open it.
#[derive(Reflect, Default, Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DoorKind {
    /// `/obj/machinery/door` itself and subtypes outside the other families (morgue, ...).
    #[default]
    Basic,
    Airlock,
    /// `/obj/machinery/door/window`: a border object in tg.
    Windoor,
    /// Firelocks: closed by fire alarms, never by hand.
    Firedoor,
    /// Blast doors and shutters: closed by buttons, never by hand.
    Poddoor,
}

/// The door vars of a DM door type (type defaults + map var edits). Prototype data like
/// [`DmAtom`](crate::DmAtom): the game builds its live door state from it once.
/// Absent vars keep BYOND's `null`/`FALSE`, so `Default` matches DM.
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmDoor {
    pub kind: DoorKind,
    /// `base_icon_state`, or `base_state` on windoors (the same role under another name).
    pub base_icon_state: String,
    /// `closingLayer`.
    pub closing_layer: f32,
    pub autoclose: bool,
    /// Airlocks: autoclose after 8 s instead of 1.5 s.
    pub normalspeed: bool,
    /// Bolted.
    pub locked: bool,
    pub welded: bool,
    pub glass: bool,
    /// `FALSE` keeps a closed door transparent (windoors).
    pub visible: bool,
    /// Refuses to close on dense things in the doorway.
    pub safe: bool,
    pub can_open_with_hands: bool,
    pub airlock_material: String,
    pub overlays_file: String,
    pub req_access: Vec<String>,
    pub req_one_access: Vec<String>,
    /// Two tiles wide (`/obj/machinery/door/airlock/multi_tile`), with a filler on the second.
    pub multi_tile: bool,
    /// Airlocks: tg `aiControlDisabled` (`AI_WIRE_*`).
    pub ai_control_disabled: i8,
    /// Airlocks: `doorOpen`, `doorClose`, `doorDeni`, `boltUp`, `boltDown` (empty: silent).
    pub door_open: String,
    pub door_close: String,
    pub door_deni: String,
    pub bolt_up: String,
    pub bolt_down: String,
    /// Blast doors and shutters: `animation_sound`.
    pub animation_sound: String,
    /// Poddoors (and brig windoors): `id`, the channel buttons open them by; empty is `null`.
    /// A number is kept as its text (DM `"[id]"`).
    pub id: String,
    /// Airlocks: `id_tag`, the channel airlock buttons (`assembly/control/airlock`) match.
    pub id_tag: String,
}

/// An airlock mapping helper's payload.
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmAirlockHelper {
    pub locked: bool,
    pub welded: bool,
    /// The helper's `dir` becomes an unrestricted side (`unres_sides ^= dir`).
    pub unres: bool,
    /// `unres/delayed`: tg opens it only after a `do_after`.
    pub unres_delayed: bool,
    /// `access/any`: appended to `req_one_access` (only while `req_access` is null).
    pub access_any: Vec<String>,
    /// `access/all`: appended to `req_access` (only while `req_one_access` is null).
    pub access_all: Vec<String>,
    /// `red_alert_access`.
    pub red_alert_access: bool,
}

/// One door var with its converted value.
#[derive(Clone, Debug, PartialEq)]
pub enum DoorField {
    Kind(DoorKind),
    BaseIconState(String),
    ClosingLayer(f32),
    Autoclose(bool),
    Normalspeed(bool),
    Locked(bool),
    Welded(bool),
    Glass(bool),
    Visible(bool),
    Safe(bool),
    CanOpenWithHands(bool),
    AirlockMaterial(String),
    OverlaysFile(String),
    ReqAccess(Vec<String>),
    ReqOneAccess(Vec<String>),
    MultiTile(bool),
    AiControlDisabled(i8),
    DoorOpen(String),
    DoorClose(String),
    DoorDeni(String),
    BoltUp(String),
    BoltDown(String),
    AnimationSound(String),
    Id(String),
    IdTag(String),
}

fn under(path: &str, root: &str) -> bool {
    path.strip_prefix(root)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

pub fn is_door(path: &str) -> bool {
    under(path, DOOR_ROOT)
}

/// The kind a type sets itself (only the kind roots do).
pub fn own_kind(path: &str) -> Option<DoorKind> {
    KIND_ROOTS
        .iter()
        .find(|(root, _)| *root == path)
        .map(|&(_, kind)| kind)
}

/// The helper payload of `path`, for the applied helper types and their subtypes; access
/// helpers concatenate the `get_access` additions of their chain, root first (each override
/// calls `..()`).
pub fn helper(types: &DmTypes, path: &str) -> Option<DmAirlockHelper> {
    let access = || {
        let mut chain: Vec<&str> = types.chain(path).collect();
        chain.reverse();
        chain
            .into_iter()
            .flat_map(|p| types.types[p].get_access.iter().cloned())
            .collect::<Vec<_>>()
    };
    Some(if under(path, HELPER_LOCKED) {
        DmAirlockHelper {
            locked: true,
            ..default()
        }
    } else if under(path, HELPER_WELDED) {
        DmAirlockHelper {
            welded: true,
            ..default()
        }
    } else if under(path, HELPER_UNRES) {
        DmAirlockHelper {
            unres: true,
            unres_delayed: under(path, HELPER_UNRES_DELAYED),
            ..default()
        }
    } else if under(path, HELPER_ACCESS_ANY) {
        DmAirlockHelper {
            access_any: access(),
            ..default()
        }
    } else if under(path, HELPER_ACCESS_ALL) {
        DmAirlockHelper {
            access_all: access(),
            ..default()
        }
    } else if under(path, HELPER_RED_ALERT_ACCESS) {
        DmAirlockHelper {
            red_alert_access: true,
            ..default()
        }
    } else {
        return None;
    })
}

/// A DM list of strings; `None` for anything else (`null` is the empty list).
fn str_list<'a>(items: impl IntoIterator<Item = Option<&'a str>>) -> Option<Vec<String>> {
    items.into_iter().map(|s| s.map(str::to_owned)).collect()
}

/// `dm_types.json` value of a door var; `None` when `var` is not a door var.
pub fn field_json(var: &str, value: &serde_json::Value) -> Option<Result<DoorField, Skip>> {
    let list = || match value {
        serde_json::Value::Null => Some(Vec::new()),
        serde_json::Value::Object(o) => match o.get("list") {
            Some(serde_json::Value::Array(items)) => {
                str_list(items.iter().map(serde_json::Value::as_str))
            }
            _ => None,
        },
        _ => None,
    };
    field(var, Val::from_json(value), list)
}

/// `.dmm` var edit of a door var; `None` when `var` is not a door var.
pub fn field_dmm(var: &str, value: &DmValue) -> Option<Result<DoorField, Skip>> {
    let list = || match value {
        DmValue::Null => Some(Vec::new()),
        DmValue::List(items) => str_list(items.iter().map(|(k, v)| match (k, v) {
            (DmValue::Str(s), None) => Some(s.as_str()),
            _ => None,
        })),
        _ => None,
    };
    field(var, Val::from_dmm(value), list)
}

fn field(
    var: &str,
    value: Val,
    list: impl FnOnce() -> Option<Vec<String>>,
) -> Option<Result<DoorField, Skip>> {
    let flag = |v: Val| match v {
        Val::Num(n) => Ok(n != 0.0),
        Val::Null => Ok(false),
        _ => Err(Skip::Unsupported),
    };
    let text = |v: Val<'_>| match v {
        Val::Str(s) | Val::Resource(s) => Ok(s.to_owned()),
        Val::Null => Ok(String::new()),
        _ => Err(Skip::Unsupported),
    };
    let list = || list().ok_or(Skip::Unsupported);
    Some(match var {
        "base_icon_state" | "base_state" => text(value).map(DoorField::BaseIconState),
        // Same rounding as `layer`.
        "closingLayer" => match atom_field("layer", value) {
            Ok(crate::value::Field::Layer(l)) => Ok(DoorField::ClosingLayer(l)),
            _ => Err(Skip::Unsupported),
        },
        "autoclose" => flag(value).map(DoorField::Autoclose),
        "normalspeed" => flag(value).map(DoorField::Normalspeed),
        "aiControlDisabled" => match value {
            Val::Num(n) => Ok(DoorField::AiControlDisabled(n.clamp(-1.0, 2.0) as i8)),
            Val::Null => Ok(DoorField::AiControlDisabled(0)),
            _ => Err(Skip::Unsupported),
        },
        "locked" => flag(value).map(DoorField::Locked),
        "welded" => flag(value).map(DoorField::Welded),
        "glass" => flag(value).map(DoorField::Glass),
        "visible" => flag(value).map(DoorField::Visible),
        "safe" => flag(value).map(DoorField::Safe),
        "can_open_with_hands" => flag(value).map(DoorField::CanOpenWithHands),
        "airlock_material" => text(value).map(DoorField::AirlockMaterial),
        "overlays_file" => text(value).map(DoorField::OverlaysFile),
        "req_access" => list().map(DoorField::ReqAccess),
        "req_one_access" => list().map(DoorField::ReqOneAccess),
        "multi_tile" => flag(value).map(DoorField::MultiTile),
        "doorOpen" => text(value).map(DoorField::DoorOpen),
        "doorClose" => text(value).map(DoorField::DoorClose),
        "doorDeni" => text(value).map(DoorField::DoorDeni),
        "boltUp" => text(value).map(DoorField::BoltUp),
        "boltDown" => text(value).map(DoorField::BoltDown),
        "animation_sound" => text(value).map(DoorField::AnimationSound),
        "id" => link_id(value).map(DoorField::Id),
        "id_tag" => link_id(value).map(DoorField::IdTag),
        _ => return None,
    })
}

/// A DM `id`/`id_tag` value as text: strings as they are, numbers as DM prints them
/// (`"[id]"`), `null` empty.
pub fn link_id(value: Val) -> Result<String, Skip> {
    match value {
        Val::Str(s) => Ok(s.to_owned()),
        Val::Num(n) => Ok(if n.fract() == 0.0 && n.abs() < 1e15 {
            format!("{}", n as i64)
        } else {
            format!("{n}")
        }),
        Val::Null => Ok(String::new()),
        _ => Err(Skip::Unsupported),
    }
}

/// `"dm_door": (...)` for a prototype patch; `None` without fields. Later fields win.
pub fn patch_entry(fields: &[DoorField]) -> Option<String> {
    let mut out = String::new();
    for (i, f) in fields.iter().enumerate() {
        let same = |g: &DoorField| std::mem::discriminant(g) == std::mem::discriminant(f);
        if fields[i + 1..].iter().any(same) {
            continue;
        }
        let ron_str = |s: &str| ron::to_string(s).expect("strings always serialize");
        let ron_list = |l: &[String]| ron::to_string(l).expect("string lists always serialize");
        let (key, value) = match f {
            DoorField::Kind(k) => ("kind", format!("{k:?}")),
            DoorField::BaseIconState(s) => ("base_icon_state", ron_str(s)),
            DoorField::ClosingLayer(v) => ("closing_layer", format!("{v:?}")),
            DoorField::Autoclose(v) => ("autoclose", v.to_string()),
            DoorField::Normalspeed(v) => ("normalspeed", v.to_string()),
            DoorField::Locked(v) => ("locked", v.to_string()),
            DoorField::Welded(v) => ("welded", v.to_string()),
            DoorField::Glass(v) => ("glass", v.to_string()),
            DoorField::Visible(v) => ("visible", v.to_string()),
            DoorField::Safe(v) => ("safe", v.to_string()),
            DoorField::CanOpenWithHands(v) => ("can_open_with_hands", v.to_string()),
            DoorField::AirlockMaterial(s) => ("airlock_material", ron_str(s)),
            DoorField::OverlaysFile(s) => ("overlays_file", ron_str(s)),
            DoorField::ReqAccess(l) => ("req_access", ron_list(l)),
            DoorField::ReqOneAccess(l) => ("req_one_access", ron_list(l)),
            DoorField::MultiTile(v) => ("multi_tile", v.to_string()),
            DoorField::AiControlDisabled(v) => ("ai_control_disabled", v.to_string()),
            DoorField::DoorOpen(s) => ("door_open", ron_str(s)),
            DoorField::DoorClose(s) => ("door_close", ron_str(s)),
            DoorField::DoorDeni(s) => ("door_deni", ron_str(s)),
            DoorField::BoltUp(s) => ("bolt_up", ron_str(s)),
            DoorField::BoltDown(s) => ("bolt_down", ron_str(s)),
            DoorField::AnimationSound(s) => ("animation_sound", ron_str(s)),
            DoorField::Id(s) => ("id", ron_str(s)),
            DoorField::IdTag(s) => ("id_tag", ron_str(s)),
        };
        if !out.is_empty() {
            out.push_str(", ");
        }
        let _ = write!(out, "{key}: {value}");
    }
    (!out.is_empty()).then(|| format!("\"dm_door\": ({out})"))
}

/// `"dm_airlock_helper": (...)` for a helper type: every field, so no merge with the parent's.
pub fn helper_entry(h: &DmAirlockHelper) -> String {
    let list = |l: &[String]| ron::to_string(l).expect("string lists always serialize");
    format!(
        "\"dm_airlock_helper\": (locked: {}, welded: {}, unres: {}, unres_delayed: {}, \
         access_any: {}, access_all: {}, red_alert_access: {})",
        h.locked,
        h.welded,
        h.unres,
        h.unres_delayed,
        list(&h.access_any),
        list(&h.access_all),
        h.red_alert_access
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn door_vars_only_and_lists() {
        let json: serde_json::Value =
            serde_json::from_str(r#"{"list": ["brig", "security"]}"#).unwrap();
        assert_eq!(
            field_json("req_access", &json),
            Some(Ok(DoorField::ReqAccess(vec![
                "brig".into(),
                "security".into()
            ])))
        );
        assert_eq!(field_json("icon", &json), None);
        let mixed: serde_json::Value = serde_json::from_str(r#"{"list": [1]}"#).unwrap();
        assert_eq!(
            field_json("req_access", &mixed),
            Some(Err(Skip::Unsupported))
        );
        assert_eq!(
            field_dmm("locked", &DmValue::Number(1.0)),
            Some(Ok(DoorField::Locked(true)))
        );
        assert_eq!(
            field_dmm("req_one_access", &DmValue::Null),
            Some(Ok(DoorField::ReqOneAccess(vec![])))
        );
        assert_eq!(
            patch_entry(&[
                DoorField::Locked(false),
                DoorField::Kind(DoorKind::Airlock),
                DoorField::Locked(true),
                DoorField::ReqAccess(vec!["a\"b".into()]),
            ])
            .unwrap(),
            r#""dm_door": (kind: Airlock, locked: true, req_access: ["a\"b"])"#
        );
        assert_eq!(patch_entry(&[]), None);
        assert_eq!(
            field_dmm("id", &DmValue::Number(3.0)),
            Some(Ok(DoorField::Id("3".into())))
        );
        assert_eq!(
            field_dmm("id_tag", &DmValue::Str("lock".into())),
            Some(Ok(DoorField::IdTag("lock".into())))
        );
        assert!(is_door("/obj/machinery/door/airlock/glass"));
        assert!(!is_door("/obj/machinery/door_buttons"));
    }

    #[test]
    fn helper_payloads_any_all_and_chain() {
        let types = DmTypes::parse(
            br#"{"schema": 3, "types": {
            "/obj": {"parent": null},
            "/obj/effect": {"parent": "/obj"},
            "/obj/effect/mapping_helpers": {"parent": "/obj/effect"},
            "/obj/effect/mapping_helpers/airlock": {"parent": "/obj/effect/mapping_helpers"},
            "/obj/effect/mapping_helpers/airlock/locked": {"parent": "/obj/effect/mapping_helpers/airlock"},
            "/obj/effect/mapping_helpers/airlock/locked/sub": {"parent": "/obj/effect/mapping_helpers/airlock/locked"},
            "/obj/effect/mapping_helpers/airlock/unres": {"parent": "/obj/effect/mapping_helpers/airlock"},
            "/obj/effect/mapping_helpers/airlock/unres/delayed": {"parent": "/obj/effect/mapping_helpers/airlock/unres"},
            "/obj/effect/mapping_helpers/airlock/cyclelink_helper": {"parent": "/obj/effect/mapping_helpers/airlock"},
            "/obj/effect/mapping_helpers/airlock/red_alert_access": {"parent": "/obj/effect/mapping_helpers/airlock"},
            "/obj/effect/mapping_helpers/airlock/access": {"parent": "/obj/effect/mapping_helpers/airlock"},
            "/obj/effect/mapping_helpers/airlock/access/any": {"parent": "/obj/effect/mapping_helpers/airlock/access"},
            "/obj/effect/mapping_helpers/airlock/access/any/command": {"parent": "/obj/effect/mapping_helpers/airlock/access/any"},
            "/obj/effect/mapping_helpers/airlock/access/any/command/maintenance": {
                "parent": "/obj/effect/mapping_helpers/airlock/access/any/command",
                "get_access": ["command", "maint_tunnels"]},
            "/obj/effect/mapping_helpers/airlock/access/any/command/maintenance/extra": {
                "parent": "/obj/effect/mapping_helpers/airlock/access/any/command/maintenance",
                "get_access": ["eva"]},
            "/obj/effect/mapping_helpers/airlock/access/all": {"parent": "/obj/effect/mapping_helpers/airlock/access"},
            "/obj/effect/mapping_helpers/airlock/access/all/security": {"parent": "/obj/effect/mapping_helpers/airlock/access/all"},
            "/obj/effect/mapping_helpers/airlock/access/all/security/brig": {
                "parent": "/obj/effect/mapping_helpers/airlock/access/all/security",
                "get_access": ["brig"]}
            }}"#,
        )
        .unwrap();
        let h = |p: &str| helper(&types, &format!("/obj/effect/mapping_helpers/airlock/{p}"));
        assert!(h("locked/sub").unwrap().locked);
        assert_eq!(
            h("unres/delayed"),
            Some(DmAirlockHelper {
                unres: true,
                unres_delayed: true,
                ..default()
            })
        );
        assert_eq!(h("cyclelink_helper"), None);
        assert!(h("red_alert_access").unwrap().red_alert_access);
        assert_eq!(
            h("access/any/command/maintenance/extra")
                .unwrap()
                .access_any,
            ["command", "maint_tunnels", "eva"]
        );
        let brig = h("access/all/security/brig").unwrap();
        assert_eq!(
            (brig.access_all, brig.access_any),
            (vec!["brig".into()], vec![])
        );
        assert_eq!(
            helper_entry(&h("access/all/security/brig").unwrap()),
            r#""dm_airlock_helper": (locked: false, welded: false, unres: false, unres_delayed: false, access_any: [], access_all: ["brig"], red_alert_access: false)"#
        );
    }
}
