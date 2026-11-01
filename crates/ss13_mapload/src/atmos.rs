//! Atmos vars as prototype data: a turf's air (`initial_gas_mix`, `blocks_air`,
//! `planetary_atmos`, `temperature`, `code/modules/atmospherics/environmental/LINDA_*.dm`)
//! and an object's `can_atmos_pass`. The loader turns them into `ss13_atmos` components once
//! per template.

use std::sync::Arc;

use bevy::prelude::*;
use ss13_atmos::{
    AtmosBlocker, AtmosPass, FireStage, GASES, GasOverlayIcons, HotspotIcons, TileKind, TurfAir,
};

use crate::convert::DmTypes;
use crate::fidelity::{ext, is_under, num};
use crate::value::{Field, Skip, Val};

const TURF: &str = "/turf";
const SPACE: &str = "/turf/open/space";
const OPEN: &str = "/turf/open";
const TURF_COMPONENT: &str = "dm_turf_atmos";
const PASS_COMPONENT: &str = "dm_atmos_pass";

/// `code/__DEFINES/atmospherics/atmos_core.dm`.
pub const ATMOS_PASS_YES: i8 = 1;
pub const ATMOS_PASS_NO: i8 = 0;
pub const ATMOS_PASS_PROC: i8 = -1;
pub const ATMOS_PASS_DENSITY: i8 = -2;

/// On every turf type.
#[derive(Component, Reflect, Clone, Debug, PartialEq)]
pub struct DmTurfAtmos {
    pub initial_gas_mix: String,
    pub blocks_air: bool,
    pub planetary_atmos: bool,
    /// `can_atmos_pass`: `/turf` is NO, `/turf/open` asks its proc (open unless `blocks_air`).
    pub can_atmos_pass: i8,
    /// tg `create_gas_mixture` uses the turf's `temperature` only when the type's own value
    /// differs from its parent type's, or a map edit sets it: not inherited by subtypes.
    pub temperature: Option<f32>,
}

impl Default for DmTurfAtmos {
    fn default() -> Self {
        Self {
            initial_gas_mix: String::new(),
            blocks_air: false,
            planetary_atmos: false,
            can_atmos_pass: ATMOS_PASS_NO,
            temperature: None,
        }
    }
}

impl DmTurfAtmos {
    /// The live component of a turf of type `path`.
    pub fn turf_air(&self, path: &str) -> TurfAir {
        let kind =
            if self.blocks_air || self.can_atmos_pass == ATMOS_PASS_NO || !is_under(path, OPEN) {
                TileKind::Blocked
            } else if is_under(path, SPACE) {
                TileKind::Space
            } else {
                TileKind::Open
            };
        TurfAir {
            kind,
            initial_gas_mix: Arc::from(self.initial_gas_mix.as_str()),
            temperature: self.temperature.map(f64::from),
            planetary: self.planetary_atmos,
        }
    }
}

/// On object types that declare `can_atmos_pass`.
#[derive(Component, Reflect, Clone, Copy, Debug, PartialEq)]
pub struct DmAtmosPass {
    pub can_atmos_pass: i8,
}

impl Default for DmAtmosPass {
    fn default() -> Self {
        Self {
            can_atmos_pass: ATMOS_PASS_YES,
        }
    }
}

impl DmAtmosPass {
    pub fn blocker(self) -> Option<AtmosBlocker> {
        match self.can_atmos_pass {
            ATMOS_PASS_NO => Some(AtmosBlocker(AtmosPass::No)),
            ATMOS_PASS_DENSITY => Some(AtmosBlocker(AtmosPass::Density)),
            ATMOS_PASS_PROC => Some(AtmosBlocker(AtmosPass::Proc)),
            _ => None,
        }
    }
}

/// tg `/obj/effect/overlay/gas` states in `icons/effects/atmospherics.dmi`, by gas.
pub fn overlay_icons(icons: &crate::IconIndex) -> GasOverlayIcons {
    let mut out = GasOverlayIcons::default();
    for (slot, gas) in out.0.iter_mut().zip(&GASES) {
        *slot = gas
            .overlay
            .and_then(|state| icons.get("icons/effects/atmospherics.dmi", state));
    }
    out
}

/// tg `/obj/effect/hotspot` stage states and BandaStation's overlay of each stage.
pub fn hotspot_icons(icons: &crate::IconIndex) -> HotspotIcons {
    let mut out = HotspotIcons::default();
    for stage in [FireStage::Light, FireStage::Medium, FireStage::Heavy] {
        let state = stage.icon_state();
        out.stages[stage as usize] = icons.get("icons/effects/fire.dmi", state);
        out.glow[stage as usize] =
            icons.get("modular_bandastation/aesthetics/fire/icons/fire.dmi", state);
    }
    out
}

const TURF_VARS: [&str; 4] = [
    "initial_gas_mix",
    "blocks_air",
    "planetary_atmos",
    "can_atmos_pass",
];

/// Converter fields of `path`: the turf root carries every turf var resolved, subtypes what
/// they declare; `temperature` where the type's own override differs from its parent's;
/// `can_atmos_pass` of objects where they declare it.
pub fn type_fields(types: &DmTypes, path: &str) -> Vec<Field> {
    let mut out = Vec::new();
    let Some(ty) = types.types.get(path) else {
        return out;
    };
    if is_under(path, TURF) {
        for var in TURF_VARS {
            let value = if path == TURF {
                types.resolved(path, var)
            } else {
                ty.declared_vars.get(var)
            };
            if let Some(Ok(f)) = value.map(|v| turf_field(var, Val::from_json(v))) {
                out.push(f);
            }
        }
        let parent = ty.parent.as_deref().filter(|p| is_under(p, TURF));
        let own = temperature_override(types, path);
        if own != parent.and_then(|p| temperature_override(types, p)) {
            out.push(ext(TURF_COMPONENT, "temperature", option_ron(own)));
        }
    } else if is_under(path, "/obj")
        && let Some(v) = ty.declared_vars.get("can_atmos_pass")
        && let Some(n) = num(Some(v))
    {
        out.push(ext(PASS_COMPONENT, "can_atmos_pass", (n as i8).to_string()));
    }
    out
}

/// The type's `temperature` when it differs from its parent type's.
fn temperature_override(types: &DmTypes, path: &str) -> Option<f32> {
    let own = num(types.resolved(path, "temperature"))?;
    let parent = types.types.get(path)?.parent.as_deref();
    let inherited = parent.and_then(|p| num(types.resolved(p, "temperature")));
    (Some(own) != inherited && parent.is_some_and(|p| is_under(p, TURF))).then_some(own as f32)
}

fn option_ron(v: Option<f32>) -> String {
    v.map_or_else(|| "None".to_owned(), |t| format!("Some({t:?})"))
}

fn turf_field(var: &str, value: Val) -> Result<Field, Skip> {
    let json_num = |v: Val| match v {
        Val::Num(n) => Ok(n),
        Val::Null => Ok(0.0),
        _ => Err(Skip::Unsupported),
    };
    let ron = match var {
        "initial_gas_mix" => match value {
            Val::Str(s) => ron::to_string(s).expect("strings always serialize"),
            Val::Null => "\"\"".to_owned(),
            _ => return Err(Skip::Unsupported),
        },
        "blocks_air" | "planetary_atmos" => (json_num(value)? != 0.0).to_string(),
        "can_atmos_pass" => (json_num(value)? as i8).to_string(),
        _ => return Err(Skip::UnknownVar),
    };
    Ok(ext(TURF_COMPONENT, static_var(var), ron))
}

fn static_var(var: &str) -> &'static str {
    TURF_VARS
        .into_iter()
        .find(|v| *v == var)
        .unwrap_or("temperature")
}

/// `.dmm` var edits: turf air vars (and `temperature`, which then always applies) on turfs,
/// `can_atmos_pass` on objects.
pub fn ext_field(path: &str, var: &str, value: Val) -> Result<Field, Skip> {
    if is_under(path, TURF) {
        if var == "temperature" {
            let Val::Num(t) = value else {
                return Err(Skip::Unsupported);
            };
            return Ok(ext(
                TURF_COMPONENT,
                "temperature",
                option_ron(Some(t as f32)),
            ));
        }
        return turf_field(var, value);
    }
    if var == "can_atmos_pass" && is_under(path, "/obj") {
        let Val::Num(n) = value else {
            return Err(Skip::Unsupported);
        };
        return Ok(ext(PASS_COMPONENT, "can_atmos_pass", (n as i8).to_string()));
    }
    Err(Skip::UnknownVar)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TYPES: &str = r##"{
      "schema": 3,
      "types": {
        "/atom": { "parent": null, "declared_vars": { "can_atmos_pass": 1 } },
        "/turf": { "parent": "/atom", "declared_vars": { "blocks_air": 0, "can_atmos_pass": 0,
            "initial_gas_mix": "o2=22;n2=82;TEMP=293.15", "temperature": 293.15 } },
        "/turf/open": { "parent": "/turf", "declared_vars": { "can_atmos_pass": -1,
            "planetary_atmos": 0 } },
        "/turf/open/space": { "parent": "/turf/open", "declared_vars": { "temperature": 2.7 } },
        "/turf/open/space/basic": { "parent": "/turf/open/space" },
        "/turf/open/floor": { "parent": "/turf/open" },
        "/turf/open/floor/airless": { "parent": "/turf/open/floor",
            "declared_vars": { "initial_gas_mix": "TEMP=2.7" } },
        "/turf/closed": { "parent": "/turf", "declared_vars": { "blocks_air": 1 } },
        "/obj": { "parent": "/atom" },
        "/obj/machinery/door": { "parent": "/obj", "declared_vars": { "can_atmos_pass": -2 } },
        "/obj/structure/window": { "parent": "/obj", "declared_vars": { "can_atmos_pass": -1 } }
      }
    }"##;

    #[test]
    fn fields() {
        let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
        let f = |p| type_fields(&types, p);
        let e = |k, v: &str| ext(TURF_COMPONENT, k, v);
        assert_eq!(
            f("/turf"),
            [
                e("initial_gas_mix", "\"o2=22;n2=82;TEMP=293.15\""),
                e("blocks_air", "false"),
                e("can_atmos_pass", "0"),
            ]
        );
        assert_eq!(
            f("/turf/open"),
            [e("planetary_atmos", "false"), e("can_atmos_pass", "-1")]
        );
        assert_eq!(f("/turf/open/space"), [e("temperature", "Some(2.7)")]);
        assert_eq!(f("/turf/open/space/basic"), [e("temperature", "None")]);
        assert_eq!(
            f("/turf/open/floor/airless"),
            [e("initial_gas_mix", "\"TEMP=2.7\"")]
        );
        assert_eq!(
            f("/obj/machinery/door"),
            [ext(PASS_COMPONENT, "can_atmos_pass", "-2")]
        );
        assert!(f("/obj").is_empty());
        let space = DmTurfAtmos {
            can_atmos_pass: ATMOS_PASS_PROC,
            ..default()
        };
        assert_eq!(
            space.turf_air("/turf/open/space/basic").kind,
            TileKind::Space
        );
        assert_eq!(space.turf_air("/turf/open/floor").kind, TileKind::Open);
        let wall = DmTurfAtmos {
            blocks_air: true,
            ..space.clone()
        };
        assert_eq!(wall.turf_air("/turf/closed/wall").kind, TileKind::Blocked);
        assert_eq!(
            ext_field("/turf/open/floor", "initial_gas_mix", Val::Str("TEMP=2.7")),
            Ok(e("initial_gas_mix", "\"TEMP=2.7\""))
        );
    }
}
