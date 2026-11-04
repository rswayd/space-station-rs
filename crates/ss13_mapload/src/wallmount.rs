//! Wall-mounted machines and cabinets as prototype data: the vars of tg's fire alarms
//! (`code/game/machinery/firealarm.dm`), buttons (`buttons.dm`, the `assembly/control` they
//! hold: `code/modules/assembly/doorcontrol.dm`), light switches (`lightswitch.dm`), mounted
//! flashers (`flasher.dm`), extinguisher cabinets (`code/game/objects/structures/extinguisher.dm`)
//! and fire axe cabinets (`fireaxe.dm`), and the machines buttons drive on the floor and walls:
//! mass drivers (`mass_driver.dm`), igniters and sparkers (`igniter.dm`). Only those families get
//! [`DmWallMount`]; the game (`ss13_game::wallmount`) builds their live state from it once.
//!
//! Map var edits of `id`, `req_access`, `req_one_access`, `normaldoorcontrol`,
//! `specialfunctions`, `sync_doors`, `skin`, `flash_range`, `locked`, `on` and `power` are
//! supported.

use bevy::prelude::*;
use serde_json::Value;
use ss13_assets::dmm::DmValue;

use crate::convert::DmTypes;
use crate::door::link_id;
use crate::fidelity::{ext, is_under, num, truthy};
use crate::value::{Field, Skip, Val};

pub const FIREALARM_ROOT: &str = "/obj/machinery/firealarm";
pub const BUTTON_ROOT: &str = "/obj/machinery/button";
pub const DOOR_BUTTON_ROOT: &str = "/obj/machinery/button/door";
pub const LIGHT_SWITCH_ROOT: &str = "/obj/machinery/light_switch";
pub const FLASHER_ROOT: &str = "/obj/machinery/flasher";
/// `/obj/machinery/flasher/portable`: a floor tripod with its own proximity logic, not a
/// wall mount. It carries its own path (it would inherit the flasher's), and the game skips it.
pub const PORTABLE_FLASHER: &str = "/obj/machinery/flasher/portable";
pub const EXTINGUISHER_CABINET_ROOT: &str = "/obj/structure/extinguisher_cabinet";
pub const FIREAXE_CABINET_ROOT: &str = "/obj/structure/fireaxecabinet";
pub const MASS_DRIVER_ROOT: &str = "/obj/machinery/mass_driver";
pub const IGNITER_ROOT: &str = "/obj/machinery/igniter";
pub const SPARKER_ROOT: &str = "/obj/machinery/sparker";
/// What the cabinets create in `Initialize(mapload)` (`new /obj/item/extinguisher(src)`, a fire
/// axe cabinet's default `item_path`).
pub const WALLMOUNT_RUNTIME_TYPES: [&str; 2] = ["/obj/item/extinguisher", "/obj/item/fireaxe"];

const COMPONENT: &str = "dm_wallmount";
const ROOTS: [&str; 9] = [
    FIREALARM_ROOT,
    BUTTON_ROOT,
    LIGHT_SWITCH_ROOT,
    FLASHER_ROOT,
    EXTINGUISHER_CABINET_ROOT,
    FIREAXE_CABINET_ROOT,
    MASS_DRIVER_ROOT,
    IGNITER_ROOT,
    SPARKER_ROOT,
];

/// The vars of a wall mount (type defaults + map edits). Prototype data like
/// [`DmAtom`](crate::DmAtom).
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmWallMount {
    /// The DM type: every wall-mount type sets its own, so instances keep their type's.
    pub path: String,
    /// Buttons, flashers, mass drivers, igniters and sparkers: `id`, the channel they control or
    /// listen on (DM `"[id]"`; empty is `null`).
    pub id: String,
    /// Buttons: the access their airlock electronics hold.
    pub req_access: Vec<String>,
    pub req_one_access: Vec<String>,
    /// Door buttons: an airlock controller (`TRUE`) or a blast door controller.
    pub normaldoorcontrol: bool,
    /// Door buttons with an airlock controller: `OPEN | IDSCAN | BOLTS | SHOCK | SAFE` bits.
    pub specialfunctions: u8,
    /// Door buttons with a blast door controller: every door follows the first one.
    pub sync_doors: bool,
    /// Buttons: the assembly `Initialize(mapload)` puts in (`device_type`); door buttons pick
    /// their own from `normaldoorcontrol`.
    pub device_type: String,
    pub base_icon_state: String,
    /// Buttons: the front panel skin suffix (`""`, `"-warning"`).
    pub skin: String,
    /// Flashers: `flash_range` tiles and `strength` (deciseconds of paralysis).
    pub flash_range: f32,
    pub strength: f32,
    /// Fire axe cabinets: `item_path`, `item_overlay`, `populate_contents`, `locked`.
    pub item_path: String,
    pub item_overlay: String,
    pub populate_contents: bool,
    pub locked: bool,
    /// Light switches: `light_on_range` (the glow while the lights are off).
    pub light_on_range: f32,
    /// Igniters: lit at start (`/obj/machinery/igniter/on`).
    pub on: bool,
    /// Mass drivers: `power` (throw speed, range multiplier), `drive_range` before its parts,
    /// `power_per_obj` joules.
    pub power: f32,
    pub drive_range: f32,
    pub power_per_obj: f32,
}

#[derive(Clone, Copy)]
enum Kind {
    Id,
    List,
    Flag,
    Num,
    /// A small integer (`specialfunctions` bits).
    Bits,
    Text,
    Path,
}

/// Wall-mount vars, their [`DmWallMount`] field (same name) and kind.
const VARS: [(&str, Kind); 19] = [
    ("id", Kind::Id),
    ("req_access", Kind::List),
    ("req_one_access", Kind::List),
    ("normaldoorcontrol", Kind::Flag),
    ("specialfunctions", Kind::Bits),
    ("sync_doors", Kind::Flag),
    ("device_type", Kind::Path),
    ("base_icon_state", Kind::Text),
    ("skin", Kind::Text),
    ("flash_range", Kind::Num),
    ("strength", Kind::Num),
    ("item_path", Kind::Path),
    ("item_overlay", Kind::Text),
    ("populate_contents", Kind::Flag),
    ("locked", Kind::Flag),
    ("on", Kind::Flag),
    ("power", Kind::Num),
    ("drive_range", Kind::Num),
    ("power_per_obj", Kind::Num),
];
/// Vars a map may edit (the rest are type data).
const MAP_VARS: [&str; 11] = [
    "id",
    "req_access",
    "req_one_access",
    "normaldoorcontrol",
    "specialfunctions",
    "sync_doors",
    "skin",
    "flash_range",
    "locked",
    "on",
    "power",
];

pub fn is_wallmount(path: &str) -> bool {
    ROOTS.iter().any(|r| is_under(path, r))
}

fn str_ron(s: &str) -> String {
    ron::to_string(s).expect("strings always serialize")
}

fn list_ron(items: &[String]) -> String {
    ron::to_string(items).expect("string lists always serialize")
}

/// A dumped DM list of strings; `null` is empty, anything else unsupported.
fn json_list(v: &Value) -> Option<Vec<String>> {
    match v {
        Value::Null => Some(Vec::new()),
        Value::Object(o) => o
            .get("list")?
            .as_array()?
            .iter()
            .map(|i| i.as_str().map(str::to_owned))
            .collect(),
        _ => None,
    }
}

fn json_ron(kind: Kind, v: &Value) -> Option<String> {
    Some(match kind {
        Kind::Id => str_ron(&link_id(Val::from_json(v)).ok()?),
        Kind::List => list_ron(&json_list(v)?),
        Kind::Flag => truthy(Some(v)).to_string(),
        Kind::Num => format!("{:?}", num(Some(v))? as f32),
        Kind::Bits => (num(Some(v))?.clamp(0.0, 255.0) as u8).to_string(),
        Kind::Text => str_ron(v.as_str().unwrap_or_default()),
        Kind::Path => str_ron(v.get("path").and_then(Value::as_str).unwrap_or_default()),
    })
}

/// Converter fields of `path`: every wall-mount type carries its resolved vars.
pub fn type_fields(types: &DmTypes, path: &str) -> Vec<Field> {
    let mut out = Vec::new();
    if !is_wallmount(path) || !types.types.contains_key(path) {
        return out;
    }
    out.push(ext(COMPONENT, "path", str_ron(path)));
    for (var, kind) in VARS {
        if let Some(ron) = types.resolved(path, var).and_then(|v| json_ron(kind, v)) {
            out.push(ext(COMPONENT, var, ron));
        }
    }
    if let Some(range) = types
        .resolved(path, "light_on_range")
        .and_then(|v| num(Some(v)))
    {
        out.push(ext(
            COMPONENT,
            "light_on_range",
            format!("{:?}", range as f32),
        ));
    }
    out
}

/// `.dmm` edits of the [`MAP_VARS`] on wall mounts.
pub fn field_dmm(path: &str, var: &str, value: &DmValue) -> Option<Result<Field, Skip>> {
    if !is_wallmount(path) || !MAP_VARS.contains(&var) {
        return None;
    }
    let &(key, kind) = VARS.iter().find(|(v, _)| *v == var)?;
    let ron = match (kind, value) {
        (Kind::Id, v) => link_id(Val::from_dmm(v)).map(|s| str_ron(&s)),
        (Kind::List, DmValue::Null) => Ok("[]".to_owned()),
        (Kind::List, DmValue::List(items)) => items
            .iter()
            .map(|(k, v)| match (k, v) {
                (DmValue::Str(s), None) => Some(s.clone()),
                _ => None,
            })
            .collect::<Option<Vec<String>>>()
            .map(|l| list_ron(&l))
            .ok_or(Skip::Unsupported),
        (Kind::Flag, DmValue::Number(n)) => Ok((*n != 0.0).to_string()),
        (Kind::Flag, DmValue::Null) => Ok("false".to_owned()),
        (Kind::Num, DmValue::Number(n)) => Ok(format!("{:?}", *n as f32)),
        (Kind::Bits, DmValue::Number(n)) => Ok((n.clamp(0.0, 255.0) as u8).to_string()),
        (Kind::Text, DmValue::Str(s)) => Ok(str_ron(s)),
        (Kind::Text, DmValue::Null) => Ok("\"\"".to_owned()),
        _ => Err(Skip::Unsupported),
    };
    Some(ron.map(|r| ext(COMPONENT, key, r)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TYPES: &str = r##"{
      "schema": 3,
      "types": {
        "/atom": { "parent": null },
        "/obj": { "parent": "/atom" },
        "/obj/machinery": { "parent": "/obj", "declared_vars": { "req_access": null } },
        "/obj/machinery/button": { "parent": "/obj/machinery", "declared_vars": {
            "id": null, "skin": "", "base_icon_state": "button", "device_type": null } },
        "/obj/machinery/button/door": { "parent": "/obj/machinery/button", "declared_vars": {
            "normaldoorcontrol": 0, "specialfunctions": 1, "sync_doors": 1 } },
        "/obj/machinery/button/door/incinerator_vent_ordmix": { "parent": "/obj/machinery/button/door",
            "declared_vars": { "id": "ordmix_vent", "req_access": { "list": ["ordnance"] } } },
        "/obj/machinery/button/crematorium": { "parent": "/obj/machinery/button",
            "declared_vars": { "id": 1, "device_type": { "path": "/obj/item/assembly/control/crematorium" } } },
        "/obj/machinery/flasher": { "parent": "/obj/machinery", "declared_vars": {
            "id": null, "flash_range": 2, "strength": 50 } },
        "/obj/machinery/flasher/portable": { "parent": "/obj/machinery/flasher" },
        "/obj/machinery/mass_driver": { "parent": "/obj/machinery", "declared_vars": {
            "id": 1, "power": 1, "drive_range": 10, "power_per_obj": 1000 } },
        "/obj/machinery/igniter": { "parent": "/obj/machinery", "declared_vars": {
            "id": null, "on": 0 } },
        "/obj/machinery/igniter/on": { "parent": "/obj/machinery/igniter", "declared_vars": {
            "on": 1 } }
      }
    }"##;

    fn get<'a>(fields: &'a [Field], key: &str) -> Option<&'a str> {
        fields.iter().find_map(|f| match f {
            Field::Ext { key: k, ron, .. } if *k == key => Some(ron.as_str()),
            _ => None,
        })
    }

    #[test]
    fn button_and_flasher_vars_resolve() {
        let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
        let vent = type_fields(&types, "/obj/machinery/button/door/incinerator_vent_ordmix");
        assert_eq!(get(&vent, "id"), Some(r#""ordmix_vent""#));
        assert_eq!(get(&vent, "req_access"), Some(r#"["ordnance"]"#));
        assert_eq!(get(&vent, "specialfunctions"), Some("1"));
        assert_eq!(get(&vent, "sync_doors"), Some("true"));
        let crem = type_fields(&types, "/obj/machinery/button/crematorium");
        assert_eq!(get(&crem, "id"), Some(r#""1""#), "a number id is its text");
        assert_eq!(
            get(&crem, "device_type"),
            Some(r#""/obj/item/assembly/control/crematorium""#)
        );
        let flasher = type_fields(&types, "/obj/machinery/flasher");
        assert_eq!(get(&flasher, "strength"), Some("50.0"));
        assert_eq!(
            get(
                &type_fields(&types, "/obj/machinery/flasher/portable"),
                "path"
            ),
            Some(r#""/obj/machinery/flasher/portable""#)
        );
        let driver = type_fields(&types, "/obj/machinery/mass_driver");
        assert_eq!(get(&driver, "id"), Some(r#""1""#));
        assert_eq!(get(&driver, "drive_range"), Some("10.0"));
        assert_eq!(get(&driver, "power_per_obj"), Some("1000.0"));
        let igniter = type_fields(&types, "/obj/machinery/igniter/on");
        assert_eq!(get(&igniter, "on"), Some("true"));
        assert_eq!(
            field_dmm("/obj/machinery/mass_driver", "power", &DmValue::Number(5.0)),
            Some(Ok(ext(COMPONENT, "power", "5.0")))
        );
    }

    #[test]
    fn map_edits() {
        let p = "/obj/machinery/button/door/directional/west";
        assert_eq!(
            field_dmm(p, "id", &DmValue::Str("rdrnd".into())),
            Some(Ok(ext(COMPONENT, "id", r#""rdrnd""#)))
        );
        let list = DmValue::List(vec![(DmValue::Str("brig".into()), None)]);
        assert_eq!(
            field_dmm(p, "req_access", &list),
            Some(Ok(ext(COMPONENT, "req_access", r#"["brig"]"#)))
        );
        assert_eq!(
            field_dmm(p, "normaldoorcontrol", &DmValue::Number(1.0)),
            Some(Ok(ext(COMPONENT, "normaldoorcontrol", "true")))
        );
        assert_eq!(field_dmm(p, "pixel_y", &DmValue::Number(1.0)), None);
        assert_eq!(field_dmm("/obj/item/pen", "id", &DmValue::Null), None);
    }
}
