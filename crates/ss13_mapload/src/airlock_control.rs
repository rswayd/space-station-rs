//! Airlock chamber controls as prototype data: tg's access consoles and buttons
//! (`/obj/machinery/door_buttons`, `code/game/machinery/embedded_controller/access_controller.dm`),
//! the embedded airlock consoles (`/obj/machinery/airlock_controller`, `airlock_controller.dm`)
//! and their sensors (`/obj/machinery/airlock_sensor`, `airlock_control.dm`). Only those
//! families get [`DmAirlockControl`]; the game (`ss13_game::airlock_controllers`) links them by
//! their tags once.
//!
//! Every var below may also be a map edit.

use bevy::prelude::*;
use serde_json::Value;
use ss13_assets::dmm::DmValue;

use crate::convert::DmTypes;
use crate::door::link_id;
use crate::fidelity::{ext, is_under, truthy};
use crate::value::{Field, Skip, Val};

pub const DOOR_BUTTONS_ROOT: &str = "/obj/machinery/door_buttons";
pub const ACCESS_BUTTON: &str = "/obj/machinery/door_buttons/access_button";
pub const ACCESS_CONSOLE: &str = "/obj/machinery/door_buttons/airlock_controller";
pub const AIRLOCK_CONTROLLER_ROOT: &str = "/obj/machinery/airlock_controller";
pub const AIRLOCK_SENSOR_ROOT: &str = "/obj/machinery/airlock_sensor";

const COMPONENT: &str = "dm_airlock_control";
const ROOTS: [&str; 3] = [
    DOOR_BUTTONS_ROOT,
    AIRLOCK_CONTROLLER_ROOT,
    AIRLOCK_SENSOR_ROOT,
];

/// The vars of an airlock chamber control (type defaults + map edits).
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmAirlockControl {
    /// The DM type: every type here sets its own, so instances keep their type's.
    pub path: String,
    /// Door buttons: `idSelf`, the console a button belongs to.
    pub id_self: String,
    /// Access buttons: `idDoor`, the airlock (`id_tag`) the button works.
    pub id_door: String,
    /// Access consoles: `idInterior`, `idExterior` (airlock `id_tag`s).
    pub id_interior: String,
    pub id_exterior: String,
    pub req_access: Vec<String>,
    pub req_one_access: Vec<String>,
    /// Airlock consoles and sensors: `id_tag`.
    pub id_tag: String,
    /// Sensors: `master_tag`, the console a press cycles.
    pub master_tag: String,
    /// Airlock consoles: the tags of their doors, pump and sensor.
    pub exterior_door_tag: String,
    pub interior_door_tag: String,
    pub airpump_tag: String,
    pub sensor_tag: String,
    /// Airlock consoles: `sanitize_external` (pump the chamber down to 1% before leaving).
    pub sanitize_external: bool,
}

#[derive(Clone, Copy)]
enum Kind {
    Id,
    List,
    Flag,
}

/// DM var, [`DmAirlockControl`] field, kind.
const VARS: [(&str, &str, Kind); 13] = [
    ("idSelf", "id_self", Kind::Id),
    ("idDoor", "id_door", Kind::Id),
    ("idInterior", "id_interior", Kind::Id),
    ("idExterior", "id_exterior", Kind::Id),
    ("req_access", "req_access", Kind::List),
    ("req_one_access", "req_one_access", Kind::List),
    ("id_tag", "id_tag", Kind::Id),
    ("master_tag", "master_tag", Kind::Id),
    ("exterior_door_tag", "exterior_door_tag", Kind::Id),
    ("interior_door_tag", "interior_door_tag", Kind::Id),
    ("airpump_tag", "airpump_tag", Kind::Id),
    ("sensor_tag", "sensor_tag", Kind::Id),
    ("sanitize_external", "sanitize_external", Kind::Flag),
];

pub fn is_airlock_control(path: &str) -> bool {
    ROOTS.iter().any(|r| is_under(path, r))
}

fn str_ron(s: &str) -> String {
    ron::to_string(s).expect("strings always serialize")
}

fn list_ron(items: &[String]) -> String {
    ron::to_string(items).expect("string lists always serialize")
}

fn json_ron(kind: Kind, v: &Value) -> Option<String> {
    Some(match kind {
        Kind::Id => str_ron(&link_id(Val::from_json(v)).ok()?),
        Kind::List => match v {
            Value::Null => list_ron(&[]),
            Value::Object(o) => list_ron(
                &o.get("list")?
                    .as_array()?
                    .iter()
                    .map(|i| i.as_str().map(str::to_owned))
                    .collect::<Option<Vec<_>>>()?,
            ),
            _ => return None,
        },
        Kind::Flag => truthy(Some(v)).to_string(),
    })
}

/// Converter fields of `path`: every control type carries its resolved vars.
pub fn type_fields(types: &DmTypes, path: &str) -> Vec<Field> {
    let mut out = Vec::new();
    if !is_airlock_control(path) || !types.types.contains_key(path) {
        return out;
    }
    out.push(ext(COMPONENT, "path", str_ron(path)));
    for (var, key, kind) in VARS {
        if let Some(ron) = types.resolved(path, var).and_then(|v| json_ron(kind, v)) {
            out.push(ext(COMPONENT, key, ron));
        }
    }
    out
}

/// `.dmm` edits of the [`VARS`] on airlock chamber controls.
pub fn field_dmm(path: &str, var: &str, value: &DmValue) -> Option<Result<Field, Skip>> {
    if !is_airlock_control(path) {
        return None;
    }
    let &(_, key, kind) = VARS.iter().find(|(v, ..)| *v == var)?;
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
        "/obj": { "parent": "/atom", "declared_vars": { "id_tag": null } },
        "/obj/machinery": { "parent": "/obj", "declared_vars": { "req_access": null } },
        "/obj/machinery/airlock_controller": { "parent": "/obj/machinery", "declared_vars": {
            "exterior_door_tag": null, "sanitize_external": null } },
        "/obj/machinery/airlock_controller/incinerator_atmos": {
            "parent": "/obj/machinery/airlock_controller", "declared_vars": {
            "exterior_door_tag": "incinerator_airlock_exterior", "id_tag": "inc_ctrl",
            "sanitize_external": 1 } }
      }
    }"##;

    fn get<'a>(fields: &'a [Field], key: &str) -> Option<&'a str> {
        fields.iter().find_map(|f| match f {
            Field::Ext { key: k, ron, .. } if *k == key => Some(ron.as_str()),
            _ => None,
        })
    }

    #[test]
    fn controller_vars_resolve_and_map_edits() {
        let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
        let c = type_fields(
            &types,
            "/obj/machinery/airlock_controller/incinerator_atmos",
        );
        assert_eq!(
            get(&c, "exterior_door_tag"),
            Some(r#""incinerator_airlock_exterior""#)
        );
        assert_eq!(get(&c, "id_tag"), Some(r#""inc_ctrl""#));
        assert_eq!(get(&c, "sanitize_external"), Some("true"));
        assert!(type_fields(&types, "/obj/machinery").is_empty());
        let list = DmValue::List(vec![(DmValue::Str("virology".into()), None)]);
        assert_eq!(
            field_dmm(ACCESS_CONSOLE, "req_access", &list)
                .unwrap()
                .unwrap(),
            ext(COMPONENT, "req_access", r#"["virology"]"#)
        );
        assert_eq!(
            field_dmm(ACCESS_BUTTON, "idDoor", &DmValue::Str("xeno_ext".into()))
                .unwrap()
                .unwrap(),
            ext(COMPONENT, "id_door", r#""xeno_ext""#)
        );
        assert_eq!(
            field_dmm(ACCESS_BUTTON, "pixel_x", &DmValue::Number(1.0)),
            None
        );
        assert_eq!(field_dmm("/obj/item/pen", "idDoor", &DmValue::Null), None);
    }
}
