//! Atmos vars as prototype data: a turf's air (`initial_gas_mix`, `blocks_air`,
//! `planetary_atmos`, `temperature`, `code/modules/atmospherics/environmental/LINDA_*.dm`)
//! and an object's `can_atmos_pass`. The loader turns them into `ss13_atmos` components once
//! per template.

use std::sync::Arc;

use bevy::prelude::*;
use ss13_atmos::{
    AtmosBlocker, AtmosPass, FireStage, GASES, GasOverlayIcons, HotspotIcons, Thermal, TileKind,
    TurfAir,
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
    /// Superconduction: `thermal_conductivity`, `heat_capacity` (`INFINITY` is 1e31) and the
    /// turf's own `temperature` (inherited, unlike [`Self::temperature`]).
    pub thermal_conductivity: f64,
    pub heat_capacity: f64,
    pub turf_temperature: f64,
}

impl Default for DmTurfAtmos {
    fn default() -> Self {
        Self {
            initial_gas_mix: String::new(),
            blocks_air: false,
            planetary_atmos: false,
            can_atmos_pass: ATMOS_PASS_NO,
            temperature: None,
            thermal_conductivity: Thermal::TURF.conductivity,
            heat_capacity: Thermal::TURF.heat_capacity,
            turf_temperature: Thermal::TURF.temperature,
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
            thermal: Thermal {
                conductivity: self.thermal_conductivity,
                heat_capacity: self.heat_capacity,
                temperature: self.turf_temperature,
            },
        }
    }
}

/// An area's vars (tg `/area`): `outdoors` (weather reaches it, barometers work in it, no room
/// beauty), the mood bonus of being there (`mood_bonus`, `mood_message`, `mood_trait`: the
/// trait it needs, empty for anyone) and its ambience ([`crate::ambience`]).
#[derive(Component, Reflect, Clone, Debug, PartialEq)]
pub struct DmArea {
    pub outdoors: bool,
    pub mood_bonus: f32,
    pub mood_message: String,
    pub mood_trait: String,
    /// `ambientsounds` after `/area/Initialize` (own list, else the `ambience_index` list).
    pub ambientsounds: Vec<String>,
    /// A newcomer may get an ambience sound before the cooldown ends (5% a second).
    pub forced_ambience: bool,
    /// The loop on the buzz channel; empty for none.
    pub ambient_buzz: String,
    pub ambient_buzz_vol: f32,
    /// Seconds (DM: deciseconds).
    pub min_ambience_cooldown: f32,
    pub max_ambience_cooldown: f32,
    /// `SOUND_ENVIRONMENT_*` (-1: none).
    pub sound_environment: i32,
}

/// tg's `/area` values; the files come from the converter.
impl Default for DmArea {
    fn default() -> Self {
        Self {
            outdoors: false,
            mood_bonus: 0.0,
            mood_message: String::new(),
            mood_trait: String::new(),
            ambientsounds: Vec::new(),
            forced_ambience: false,
            ambient_buzz: String::new(),
            ambient_buzz_vol: 35.0,
            min_ambience_cooldown: 4.0,
            max_ambience_cooldown: 10.0,
            sound_environment: -1,
        }
    }
}

/// `beauty` of a cleanable decal type (`/datum/element/beauty` in its `Initialize`): dirt and
/// messes make a room uglier.
#[derive(Component, Reflect, Clone, Copy, Debug, Default, PartialEq)]
pub struct DmBeauty {
    pub beauty: f32,
}

/// On object types that declare `can_atmos_pass`.
#[derive(Component, Reflect, Clone, Copy, Debug, PartialEq)]
pub struct DmAtmosPass {
    pub can_atmos_pass: i8,
    /// tg `block_superconductivity()`, a proc: [`blocks_heat`] evaluates it per type.
    pub block_superconductivity: bool,
}

impl Default for DmAtmosPass {
    fn default() -> Self {
        Self {
            can_atmos_pass: ATMOS_PASS_YES,
            block_superconductivity: false,
        }
    }
}

/// tg `block_superconductivity()` of a closed `path`: doors that are opaque or `heat_proof`
/// (not plasma airlocks), reinforced plasma windows, and the structures that always return
/// TRUE (`atmos_field_gen.dm`, `holosign.dm`, `_blob.dm`, `turbine.dm`, `aliens.dm`).
pub fn blocks_heat(types: &DmTypes, path: &str) -> bool {
    const ALWAYS: [&str; 6] = [
        "/obj/structure/window/reinforced/plasma",
        "/obj/structure/holosign/barrier/atmos",
        "/obj/effect/atmos_shield",
        "/obj/structure/blob",
        "/obj/machinery/power/turbine",
        "/obj/structure/alien/resin/wall",
    ];
    if ALWAYS.iter().any(|root| is_under(path, root)) {
        return true;
    }
    if !is_under(path, "/obj/machinery/door")
        || is_under(path, "/obj/machinery/door/airlock/plasma")
    {
        return false;
    }
    let set = |var| num(types.resolved(path, var)).is_some_and(|n| n != 0.0);
    set("opacity") || set("heat_proof")
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

const TURF_VARS: [&str; 6] = [
    "initial_gas_mix",
    "blocks_air",
    "planetary_atmos",
    "can_atmos_pass",
    "thermal_conductivity",
    "heat_capacity",
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
        let declared = if path == TURF {
            types.resolved(path, "temperature")
        } else {
            ty.declared_vars.get("temperature")
        };
        if let Some(t) = declared.and_then(serde_json::Value::as_f64) {
            out.push(ext(TURF_COMPONENT, "turf_temperature", format!("{t:?}")));
        }
        let parent = ty.parent.as_deref().filter(|p| is_under(p, TURF));
        let own = temperature_override(types, path);
        if own != parent.and_then(|p| temperature_override(types, p)) {
            out.push(ext(TURF_COMPONENT, "temperature", option_ron(own)));
        }
    } else if is_under(path, "/area") {
        if let Some(v) = ty.declared_vars.get("outdoors") {
            let on = num(Some(v)).is_some_and(|n| n != 0.0);
            out.push(ext("dm_area", "outdoors", on.to_string()));
        }
        if let Some(n) = num(ty.declared_vars.get("mood_bonus")) {
            out.push(ext("dm_area", "mood_bonus", format!("{:?}", n as f32)));
        }
        for var in ["mood_message", "mood_trait"] {
            if let Some(v) = ty.declared_vars.get(var) {
                let text = v.as_str().unwrap_or_default();
                let ron = ron::to_string(text).expect("strings always serialize");
                out.push(ext("dm_area", var, ron));
            }
        }
    } else if is_under(path, "/obj") {
        if is_under(path, crate::CLEANABLE_ROOT)
            && let Some(n) = num(ty.declared_vars.get("beauty"))
        {
            out.push(ext("dm_beauty", "beauty", format!("{:?}", n as f32)));
        }
        if let Some(v) = ty.declared_vars.get("can_atmos_pass")
            && let Some(n) = num(Some(v))
        {
            out.push(ext(PASS_COMPONENT, "can_atmos_pass", (n as i8).to_string()));
        }
        let heat = blocks_heat(types, path);
        let parent = ty.parent.as_deref();
        if heat != parent.is_some_and(|p| blocks_heat(types, p)) {
            out.push(ext(
                PASS_COMPONENT,
                "block_superconductivity",
                heat.to_string(),
            ));
        }
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
        "thermal_conductivity" => format!("{:?}", json_num(value)?),
        "heat_capacity" => format!("{:?}", json_num(value)?),
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
            // shortcut: an edit sets the air's and not the solid's (`turf_temperature`) one;
            // a field carries one var, and edited temperatures are rare on station turfs.
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
        "/turf/open/floor": { "parent": "/turf/open",
            "declared_vars": { "thermal_conductivity": 0.02, "heat_capacity": 20000 } },
        "/turf/open/floor/airless": { "parent": "/turf/open/floor",
            "declared_vars": { "initial_gas_mix": "TEMP=2.7" } },
        "/turf/closed": { "parent": "/turf", "declared_vars": { "blocks_air": 1 } },
        "/obj": { "parent": "/atom" },
        "/obj/machinery/door": { "parent": "/obj", "declared_vars": { "can_atmos_pass": -2,
            "opacity": 1, "heat_proof": 0 } },
        "/obj/machinery/door/airlock": { "parent": "/obj/machinery/door" },
        "/obj/machinery/door/airlock/glass": { "parent": "/obj/machinery/door/airlock",
            "declared_vars": { "opacity": 0 } },
        "/obj/machinery/door/airlock/plasma": { "parent": "/obj/machinery/door/airlock" },
        "/obj/machinery/door/firedoor": { "parent": "/obj/machinery/door",
            "declared_vars": { "opacity": 0, "heat_proof": 1 } },
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
                e("turf_temperature", "293.15"),
            ]
        );
        assert_eq!(
            f("/turf/open"),
            [e("planetary_atmos", "false"), e("can_atmos_pass", "-1")]
        );
        assert_eq!(
            f("/turf/open/space"),
            [e("turf_temperature", "2.7"), e("temperature", "Some(2.7)")]
        );
        assert_eq!(
            f("/turf/open/floor"),
            [
                e("thermal_conductivity", "0.02"),
                e("heat_capacity", "20000.0")
            ]
        );
        assert_eq!(f("/turf/open/space/basic"), [e("temperature", "None")]);
        assert_eq!(
            f("/turf/open/floor/airless"),
            [e("initial_gas_mix", "\"TEMP=2.7\"")]
        );
        assert_eq!(
            f("/obj/machinery/door"),
            [
                ext(PASS_COMPONENT, "can_atmos_pass", "-2"),
                ext(PASS_COMPONENT, "block_superconductivity", "true"),
            ]
        );
        // Opaque doors stop heat, glass ones do not, plasma airlocks never, firelocks are
        // `heat_proof`; a subtype that agrees with its parent carries nothing.
        assert_eq!(f("/obj/machinery/door/airlock"), []);
        let heat = |p| blocks_heat(&types, p);
        assert!(!heat("/obj/machinery/door/airlock/glass"));
        assert!(!heat("/obj/machinery/door/airlock/plasma"));
        assert!(heat("/obj/machinery/door/firedoor"));
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
