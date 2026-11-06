//! Step triggers, bump teleporters and permanent portals as prototype data
//! (`code/game/objects/effects/step_triggers.dm`, `bump_teleporter.dm`, `portals.dm`): the vars
//! their behavior reads, type defaults plus `.dmm` edits. Mappers link these by var edits
//! (`id`/`id_target` on CentCom's bump teleporters, `teleport_y_offset` on looping hallways,
//! `id` on portal pairs and their `/obj/effect/landmark/portal_exit` exits), so every var below
//! takes map edits.

use bevy::prelude::*;

use crate::convert::DmTypes;
use crate::fidelity::{ext, is_under};
use crate::value::{Field, Skip, Val};

pub const STEP_TRIGGER_ROOT: &str = "/obj/effect/step_trigger";
pub const BUMP_TELEPORTER_ROOT: &str = "/obj/effect/bump_teleporter";
pub const PERMANENT_PORTAL_ROOT: &str = "/obj/effect/portal/permanent";
pub const PORTAL_EXIT_ROOT: &str = "/obj/effect/landmark/portal_exit";
/// Mines only need the type path here (`ss13_game::effects::mines`).
pub const MINE_ROOT: &str = "/obj/effect/mine";
const ROOTS: [&str; 5] = [
    STEP_TRIGGER_ROOT,
    BUMP_TELEPORTER_ROOT,
    PERMANENT_PORTAL_ROOT,
    PORTAL_EXIT_ROOT,
    MINE_ROOT,
];
const COMPONENT: &str = "dm_trigger";

/// The trigger vars of a step trigger, bump teleporter, permanent portal or portal exit. Field
/// names are the DM var names; a var the type does not declare keeps BYOND's `null`/`0`.
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmTrigger {
    /// The type path (`/obj/effect` atoms carry no [`crate::DmObj`]).
    pub path: String,
    /// Bump teleporters, permanent portals, portal exits: `id` (numbers as DM prints them).
    pub id: String,
    /// Bump teleporters: the `id` of the teleporter they send to.
    pub id_target: String,
    pub affect_ghosts: bool,
    /// Stops throwers.
    pub stopper: bool,
    pub mobs_only: bool,
    /// `message`: the line, once (`once`) or every time.
    pub message: String,
    pub once: bool,
    /// `teleporter` (absolute, `0`: no teleport) and `teleporter/random` (the lower corner).
    pub teleport_x: f32,
    pub teleport_y: f32,
    pub teleport_z: f32,
    /// `teleporter/offset`: relative; `teleporter/random`: the upper corner.
    pub teleport_x_offset: f32,
    pub teleport_y_offset: f32,
    pub teleport_z_offset: f32,
    /// `teleport_fancy`: the destination on the trigger's own level.
    pub locationx: f32,
    pub locationy: f32,
    /// `teleport_fancy`: uses left (`0`: infinite).
    pub uses: f32,
    pub entersparks: bool,
    pub exitsparks: bool,
    pub entersmoke: bool,
    pub exitsmoke: bool,
    /// `sound_effect`: a file.
    pub sound: String,
    pub volume: f32,
    pub freq_vary: bool,
    pub extra_range: f32,
    pub happens_once: bool,
    pub triggerer_only: bool,
    /// `thrower`: DM direction bits, tiles (`0`: until a stopper), immobilized while flung,
    /// faces the flight, only teleporters stop it.
    pub direction: f32,
    pub tiles: f32,
    pub immobilize: bool,
    pub facedir: bool,
    pub nostop: bool,
}

#[derive(Clone, Copy)]
enum Kind {
    Text,
    Num,
    Flag,
}

/// `(var, kind)`; the component key is the var name.
const VARS: [(&str, Kind); 31] = [
    ("id", Kind::Text),
    ("id_target", Kind::Text),
    ("affect_ghosts", Kind::Flag),
    ("stopper", Kind::Flag),
    ("mobs_only", Kind::Flag),
    ("message", Kind::Text),
    ("once", Kind::Flag),
    ("teleport_x", Kind::Num),
    ("teleport_y", Kind::Num),
    ("teleport_z", Kind::Num),
    ("teleport_x_offset", Kind::Num),
    ("teleport_y_offset", Kind::Num),
    ("teleport_z_offset", Kind::Num),
    ("locationx", Kind::Num),
    ("locationy", Kind::Num),
    ("uses", Kind::Num),
    ("entersparks", Kind::Flag),
    ("exitsparks", Kind::Flag),
    ("entersmoke", Kind::Flag),
    ("exitsmoke", Kind::Flag),
    ("sound", Kind::Text),
    ("volume", Kind::Num),
    ("freq_vary", Kind::Flag),
    ("extra_range", Kind::Num),
    ("happens_once", Kind::Flag),
    ("triggerer_only", Kind::Flag),
    ("direction", Kind::Num),
    ("tiles", Kind::Num),
    ("immobilize", Kind::Flag),
    ("facedir", Kind::Flag),
    ("nostop", Kind::Flag),
];

fn is_trigger(path: &str) -> bool {
    ROOTS.iter().any(|root| is_under(path, root))
}

/// One var's RON value.
fn ron_of(kind: Kind, value: Val) -> Result<String, Skip> {
    match kind {
        Kind::Text => {
            let text = match value {
                Val::Resource(s) => s.to_owned(),
                other => crate::door::link_id(other)?,
            };
            Ok(ron::to_string(&text).expect("strings always serialize"))
        }
        Kind::Num => match value {
            Val::Num(n) => Ok(format!("{:?}", n as f32)),
            Val::Null => Ok("0.0".into()),
            _ => Err(Skip::Unsupported),
        },
        Kind::Flag => match value {
            Val::Num(n) => Ok((n != 0.0).to_string()),
            Val::Null => Ok("false".into()),
            Val::Str(s) => Ok((!s.is_empty()).to_string()),
            _ => Err(Skip::Unsupported),
        },
    }
}

/// Converter fields: every trigger type carries the vars its chain declares.
pub fn type_fields(types: &DmTypes, path: &str) -> Vec<Field> {
    if !is_trigger(path) || !types.types.contains_key(path) {
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

/// `.dmm` edits of the trigger vars.
pub fn ext_field(path: &str, var: &str, value: Val) -> Result<Field, Skip> {
    let Some(&(key, kind)) = VARS.iter().find(|(v, _)| *v == var) else {
        return Err(Skip::UnknownVar);
    };
    if !is_trigger(path) {
        return Err(Skip::UnknownVar);
    }
    Ok(ext(COMPONENT, key, ron_of(kind, value)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_edits_become_fields() {
        assert_eq!(
            ext_field(BUMP_TELEPORTER_ROOT, "id_target", Val::Str("Synd25")).ok(),
            Some(ext(COMPONENT, "id_target", "\"Synd25\""))
        );
        assert_eq!(
            ext_field(PERMANENT_PORTAL_ROOT, "id", Val::Num(3.0)).ok(),
            Some(ext(COMPONENT, "id", "\"3\""))
        );
        assert_eq!(
            ext_field(
                "/obj/effect/step_trigger/teleporter/offset",
                "teleport_y_offset",
                Val::Num(-30.0)
            )
            .ok(),
            Some(ext(COMPONENT, "teleport_y_offset", "-30.0"))
        );
        assert_eq!(
            ext_field("/obj/effect/step_trigger/message", "once", Val::Num(0.0)).ok(),
            Some(ext(COMPONENT, "once", "false"))
        );
        assert!(ext_field("/obj/machinery/door", "id", Val::Str("x")).is_err());
        assert!(ext_field(BUMP_TELEPORTER_ROOT, "name", Val::Str("x")).is_err());
    }

    #[test]
    fn types_carry_their_declared_vars() {
        let types = DmTypes::parse(
            br#"{"schema": 3, "types": {
                "/atom": { "parent": null },
                "/obj": { "parent": "/atom" },
                "/obj/effect": { "parent": "/obj" },
                "/obj/effect/step_trigger": { "parent": "/obj/effect",
                    "declared_vars": { "affect_ghosts": 0, "stopper": 1, "mobs_only": 0 } },
                "/obj/effect/step_trigger/sound_effect": { "parent": "/obj/effect/step_trigger",
                    "declared_vars": { "sound": null, "volume": 100, "happens_once": 0 } },
                "/obj/effect/step_trigger/sound_effect/lavaland_cult_altar": {
                    "parent": "/obj/effect/step_trigger/sound_effect",
                    "declared_vars": { "happens_once": 1,
                        "sound": { "resource": "sound/effects/hallucinations/i_see_you1.ogg" } } }
            }}"#,
        )
        .expect("parses");
        let fields = type_fields(
            &types,
            "/obj/effect/step_trigger/sound_effect/lavaland_cult_altar",
        );
        for expected in [
            ext(
                COMPONENT,
                "path",
                "\"/obj/effect/step_trigger/sound_effect/lavaland_cult_altar\"",
            ),
            ext(COMPONENT, "stopper", "true"),
            ext(COMPONENT, "happens_once", "true"),
            ext(COMPONENT, "volume", "100.0"),
            ext(
                COMPONENT,
                "sound",
                "\"sound/effects/hallucinations/i_see_you1.ogg\"",
            ),
        ] {
            assert!(fields.contains(&expected), "{expected:?} in {fields:?}");
        }
        assert!(type_fields(&types, "/obj/effect").is_empty());
    }
}
