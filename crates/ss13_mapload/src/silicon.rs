//! Silicon data from the DM dump.
//!
//! - [`DmCamera`] on `/obj/machinery/camera` types (`code/game/machinery/camera/camera.dm`):
//!   `c_tag`, `network`, `view_range`, `camera_enabled`, with map edits of `c_tag` and `network`
//!   (a list, or one string as some maps write it).
//! - [`DmCameraConsole`] on `/obj/machinery/computer/security` types: their `network`.
//! - [`SILICON_FILE`]: every `/datum/ai_laws` lawset (`code/datums/ai_laws/`, BandaStation's
//!   `modular_bandastation/ai_laws`) and the cyborg models `pick_model()` offers
//!   (`/obj/item/robot_model/*`: name, sprite, `basic_modules`, `emag_modules`,
//!   `radio_channels`) and the cyborg upgrade boards (`/obj/item/borg/upgrade/*`, their item
//!   lists and mounting rules) and the law rack modules (`/obj/item/ai_module/*`: their
//!   `laws`, `law_id`). The game (`ss13_game::silicon`) reads it at startup.

use std::collections::BTreeSet;
use std::path::Path;

use bevy::prelude::{Component, Reflect};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ss13_assets::dmm::DmValue;

use crate::convert::DmTypes;
use crate::fidelity::{ext, is_under, num, truthy};
use crate::value::{Field, Skip};

/// `<assets>/silicon.ron`.
pub const SILICON_FILE: &str = "silicon.ron";
pub const CAMERA_ROOT: &str = "/obj/machinery/camera";
pub const CAMERA_CONSOLE_ROOT: &str = "/obj/machinery/computer/security";
pub const LAWS_ROOT: &str = "/datum/ai_laws";
pub const MODEL_ROOT: &str = "/obj/item/robot_model";
pub const AI_TYPE: &str = "/mob/living/silicon/ai";
pub const ROBOT_TYPE: &str = "/mob/living/silicon/robot";
pub const AI_EYE_TYPE: &str = "/mob/eye/camera/ai";
/// `/mob/living/silicon/robot::cell`.
pub const ROBOT_CELL: &str = "/obj/item/stock_parts/power_store/cell/high";
/// `/mob/living/silicon/robot/model/syndicate::cell`.
pub const ROBOT_SYNDICATE_CELL: &str = "/obj/item/stock_parts/power_store/cell/hyper";
const CAMERA: &str = "dm_camera";
const CONSOLE: &str = "dm_camera_console";
/// `/obj/machinery/ai_slipper`: the AI's foam dispenser.
pub const AI_SLIPPER_ROOT: &str = "/obj/machinery/ai_slipper";
const SLIPPER: &str = "dm_ai_slipper";

/// Spawned at runtime by camera construction (`ss13_game::defenses::cameras`): the frame a
/// camera assembly puts on a wall, the assembly a camera comes apart into, its cable.
pub const CAMERA_RUNTIME_TYPES: [&str; 3] = [
    "/obj/machinery/camera/autoname/deconstructed",
    "/obj/item/wallframe/camera",
    "/obj/item/stack/cable_coil",
];

/// Spawned at runtime by the silicon jobs: the mobs and the borg's cell; by carding, shunting
/// and AI core construction (`ss13_game::silicon::ai_core`): the core, its board and what it
/// comes apart into.
pub const SILICON_TYPES: [&str; 10] = [
    AI_TYPE,
    ROBOT_TYPE,
    AI_EYE_TYPE,
    ROBOT_CELL,
    ROBOT_SYNDICATE_CELL,
    "/obj/structure/ai_core",
    "/obj/structure/ai_core/latejoin_inactive",
    "/obj/item/circuitboard/aicore",
    "/obj/item/stack/sheet/rglass",
    "/obj/item/stack/sheet/plasteel",
];

/// `/obj/item/borg/upgrade`.
pub const UPGRADE_ROOT: &str = "/obj/item/borg/upgrade";
/// `/obj/item/ai_module`: what goes into a law rack.
pub const AI_MODULE_ROOT: &str = "/obj/item/ai_module";

/// tg `/mob/living/silicon/robot/proc/pick_model()`: the models offered, in its order, with
/// BandaStation's `DISABLE_SECBORG` (on in `config/game_options.txt`) leaving security out.
pub const PICK_MODELS: [&str; 6] = [
    "/obj/item/robot_model/engineering",
    "/obj/item/robot_model/medical",
    "/obj/item/robot_model/miner",
    "/obj/item/robot_model/janitor",
    "/obj/item/robot_model/service",
    "/obj/item/robot_model/peacekeeper",
];

/// The models `pick_model()` does not offer, which other paths reach: security (its config flag
/// is on, see [`PICK_MODELS`]; a server without it adds this one to the list), the clown (its
/// `transform` board), the syndicate models (the operatives' beacons).
pub const OTHER_MODELS: [&str; 5] = [
    "/obj/item/robot_model/security",
    "/obj/item/robot_model/clown",
    "/obj/item/robot_model/syndicate",
    "/obj/item/robot_model/syndicate_medical",
    "/obj/item/robot_model/saboteur",
];

/// Camera vars of a type (defaults + map edits).
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmCamera {
    pub path: String,
    pub c_tag: String,
    pub network: Vec<String>,
    pub view_range: f64,
    pub camera_enabled: bool,
}

/// `uses` of a foam dispenser (`code/game/machinery/ai_slipper.dm`), with map edits.
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmAiSlipper {
    pub uses: f64,
}

/// `network` of a camera console.
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmCameraConsole {
    pub network: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Lawset {
    pub path: String,
    pub id: String,
    pub name: String,
    pub zeroth: Option<String>,
    pub zeroth_borg: Option<String>,
    pub inherent: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RobotModel {
    pub path: String,
    pub name: String,
    pub cyborg_base_icon: String,
    /// Module item types, in `basic_modules` order (repeats kept: two omnitools).
    pub basic_modules: Vec<String>,
    /// tg `emag_modules`: added while the cyborg is emagged.
    #[serde(default)]
    pub emag_modules: Vec<String>,
    pub radio_channels: Vec<String>,
    /// One of [`OTHER_MODELS`]: not in the choice list.
    #[serde(default)]
    pub hidden: bool,
    /// tg `borg_skins` (BandaStation adds its own): the looks offered when picking the model.
    #[serde(default)]
    pub skins: Vec<BorgSkin>,
}

/// An entry of `borg_skins`: its name, `SKIN_ICON_STATE` and `SKIN_ICON` (the dmi; none: the
/// cyborg's `icons/mob/silicon/robots.dmi`).
// shortcut: no `SKIN_PIXEL_X/Y`, `SKIN_LIGHT_KEY`, `SKIN_HAT_OFFSET` or `SKIN_TRAITS`: the
// port has no cyborg lights, hats or model traits.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BorgSkin {
    pub name: String,
    pub icon_state: Option<String>,
    pub icon: Option<String>,
}

/// `borg_skins`: `list("Name" = list(SKIN_ICON_STATE = "...", SKIN_ICON = 'x.dmi', ...))`.
fn borg_skins(v: Option<&Value>) -> Vec<BorgSkin> {
    let Some(items) = v.and_then(|v| v.get("list")).and_then(Value::as_array) else {
        return Vec::new();
    };
    let pair = |item: &Value| -> Option<(Value, Value)> {
        match item.get("assoc")?.as_array()?.as_slice() {
            [k, v] => Some((k.clone(), v.clone())),
            _ => None,
        }
    };
    items
        .iter()
        .filter_map(|item| {
            let (name, details) = pair(item)?;
            let mut skin = BorgSkin {
                name: name.as_str()?.to_owned(),
                ..Default::default()
            };
            let details = details.get("list").and_then(Value::as_array);
            for (key, value) in details.into_iter().flatten().filter_map(pair) {
                match key.as_str() {
                    Some("skin_icon_state") => skin.icon_state = value.as_str().map(str::to_owned),
                    Some("skin_icon") => {
                        skin.icon = value
                            .get("resource")
                            .and_then(Value::as_str)
                            .map(str::to_owned);
                    }
                    _ => {}
                }
            }
            Some(skin)
        })
        .collect()
}

/// A cyborg upgrade board (tg `/obj/item/borg/upgrade` vars).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BorgUpgrade {
    pub path: String,
    /// tg `locked`: refused until unlocked (the syndicate boards).
    pub locked: bool,
    pub require_model: bool,
    /// tg `model_type`: the models with a mounting point; empty: any.
    pub model_type: Vec<String>,
    pub one_use: bool,
    pub allow_duplicates: bool,
    /// tg `items_to_add`, `items_to_remove`: module types.
    pub items_to_add: Vec<String>,
    pub items_to_remove: Vec<String>,
    /// `/obj/item/borg/upgrade/transform` `new_model`.
    pub new_model: Option<String>,
}

/// A law rack module (tg `/obj/item/ai_module` vars).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AiModuleDef {
    pub path: String,
    /// tg `laws` (the keys; none of the typed lists start ioned).
    pub laws: Vec<String>,
    /// `/obj/item/ai_module/law/core/full` `law_id`: its laws are that lawset's `inherent`.
    pub law_id: Option<String>,
}

/// One employer of `strings/antagonist_flavor/malfunction_flavor.json`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MalfFlavor {
    #[serde(default)]
    pub allies: String,
    #[serde(default)]
    pub goal: String,
    #[serde(default)]
    pub introduction: String,
    #[serde(default)]
    pub zeroth_law: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SiliconData {
    pub lawsets: Vec<Lawset>,
    /// [`PICK_MODELS`] and [`OTHER_MODELS`] that exist.
    pub models: Vec<RobotModel>,
    /// Every `/obj/item/borg/upgrade` subtype, by path.
    #[serde(default)]
    pub upgrades: Vec<BorgUpgrade>,
    /// Every `/obj/item/ai_module` subtype, by path.
    #[serde(default)]
    pub modules: Vec<AiModuleDef>,
    /// `MALFUNCTION_FLAVOR_FILE` by employer, sorted.
    #[serde(default)]
    pub malf_flavor: Vec<(String, MalfFlavor)>,
}

impl SiliconData {
    pub fn read(path: &Path) -> anyhow::Result<Self> {
        crate::read_ron(path)
    }

    pub fn to_ron(&self) -> String {
        let mut out = String::from("// Generated by `ss13-mapload convert`. Do not edit.\n");
        out.push_str(
            &ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())
                .expect("silicon data serializes"),
        );
        out.push('\n');
        out
    }

    /// Spawned at runtime: the models and their modules, the upgrades' modules.
    pub fn spawned_types(&self) -> BTreeSet<&str> {
        let models = self.models.iter().flat_map(|m| {
            std::iter::once(m.path.as_str())
                .chain(m.basic_modules.iter().map(String::as_str))
                .chain(m.emag_modules.iter().map(String::as_str))
        });
        let upgrades = self
            .upgrades
            .iter()
            .flat_map(|u| &u.items_to_add)
            .map(String::as_str);
        let modules = self.modules.iter().map(|m| m.path.as_str());
        models.chain(upgrades).chain(modules).collect()
    }

    pub fn module(&self, path: &str) -> Option<&AiModuleDef> {
        self.modules.iter().find(|m| m.path == path)
    }

    pub fn upgrade(&self, path: &str) -> Option<&BorgUpgrade> {
        self.upgrades.iter().find(|u| u.path == path)
    }

    pub fn lawset(&self, id: &str) -> Option<&Lawset> {
        self.lawsets.iter().find(|l| l.id == id)
    }
}

/// Reads the malfunctioning AI's flavor from the BYOND checkout's `strings/`; the problem, if
/// the file is missing or broken.
pub fn read_malf_flavor(source: &Path, data: &mut SiliconData) -> Option<String> {
    let path = source.join("strings/antagonist_flavor/malfunction_flavor.json");
    match std::fs::read(&path)
        .map_err(anyhow::Error::from)
        .and_then(|b| {
            Ok(serde_json::from_slice::<
                std::collections::BTreeMap<String, MalfFlavor>,
            >(&b)?)
        }) {
        Ok(map) => {
            data.malf_flavor = map.into_iter().collect();
            None
        }
        Err(e) => Some(format!("{}: {e}", path.display())),
    }
}

fn text(v: Option<&Value>) -> Option<String> {
    v.and_then(Value::as_str).map(str::to_owned)
}

/// A dumped DM list of strings (`{"list": [...]}`) or one string; paths as `{"path": ...}`.
fn list(v: Option<&Value>) -> Vec<String> {
    match v {
        Some(Value::String(s)) => vec![s.clone()],
        Some(Value::Object(o)) => o
            .get("list")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|item| match item {
                Value::String(s) => Some(s.clone()),
                Value::Object(o) => o.get("path").and_then(Value::as_str).map(str::to_owned),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

pub fn resolve(types: &DmTypes) -> SiliconData {
    let mut lawsets: Vec<Lawset> = types
        .types
        .keys()
        .filter(|p| is_under(p, LAWS_ROOT) && p.as_str() != LAWS_ROOT)
        .map(|path| Lawset {
            path: path.clone(),
            id: text(types.resolved(path, "id")).unwrap_or_default(),
            name: text(types.resolved(path, "name")).unwrap_or_default(),
            zeroth: text(types.resolved(path, "zeroth")),
            zeroth_borg: text(types.resolved(path, "zeroth_borg")),
            inherent: list(types.resolved(path, "inherent")),
        })
        .collect();
    lawsets.sort_by(|a, b| a.path.cmp(&b.path));
    let models = PICK_MODELS
        .iter()
        .map(|&p| (p, false))
        .chain(OTHER_MODELS.iter().map(|&p| (p, true)))
        .filter(|(p, _)| types.types.contains_key(*p))
        .map(|(path, hidden)| RobotModel {
            path: path.to_owned(),
            hidden,
            name: text(types.resolved(path, "name")).unwrap_or_default(),
            cyborg_base_icon: text(types.resolved(path, "cyborg_base_icon")).unwrap_or_default(),
            basic_modules: existing(types, list(types.resolved(path, "basic_modules"))),
            emag_modules: existing(types, list(types.resolved(path, "emag_modules"))),
            radio_channels: list(types.resolved(path, "radio_channels")),
            skins: borg_skins(types.resolved(path, "borg_skins")),
        })
        .collect();
    let mut upgrades: Vec<BorgUpgrade> = types
        .types
        .keys()
        .filter(|p| is_under(p, UPGRADE_ROOT))
        .map(|path| BorgUpgrade {
            path: path.clone(),
            locked: truthy(types.resolved(path, "locked")),
            require_model: truthy(types.resolved(path, "require_model")),
            model_type: list(types.resolved(path, "model_type")),
            one_use: truthy(types.resolved(path, "one_use")),
            allow_duplicates: truthy(types.resolved(path, "allow_duplicates")),
            items_to_add: existing(types, list(types.resolved(path, "items_to_add"))),
            items_to_remove: list(types.resolved(path, "items_to_remove")),
            new_model: match types.resolved(path, "new_model") {
                Some(Value::String(s)) => Some(s.clone()),
                Some(Value::Object(o)) => o.get("path").and_then(Value::as_str).map(str::to_owned),
                _ => None,
            },
        })
        .collect();
    upgrades.sort_by(|a, b| a.path.cmp(&b.path));
    let mut modules: Vec<AiModuleDef> = types
        .types
        .keys()
        .filter(|p| is_under(p, AI_MODULE_ROOT) && p.as_str() != AI_MODULE_ROOT)
        .map(|path| AiModuleDef {
            path: path.clone(),
            laws: list(types.resolved(path, "laws")),
            law_id: text(types.resolved(path, "law_id")),
        })
        .collect();
    modules.sort_by(|a, b| a.path.cmp(&b.path));
    SiliconData {
        lawsets,
        models,
        upgrades,
        modules,
        malf_flavor: Vec::new(),
    }
}

/// The types of `paths` the dump has.
fn existing(types: &DmTypes, paths: Vec<String>) -> Vec<String> {
    paths
        .into_iter()
        .filter(|m| types.types.contains_key(m))
        .collect()
}

fn str_ron(s: &str) -> String {
    ron::to_string(s).expect("strings always serialize")
}

fn list_ron(items: &[String]) -> String {
    ron::to_string(items).expect("string lists always serialize")
}

/// Converter fields of `path`: every camera and console type carries its resolved vars.
pub fn type_fields(types: &DmTypes, path: &str) -> Vec<Field> {
    let mut out = Vec::new();
    if !types.types.contains_key(path) {
        return out;
    }
    if is_under(path, CAMERA_ROOT) {
        out.push(ext(CAMERA, "path", str_ron(path)));
        let c_tag = text(types.resolved(path, "c_tag")).unwrap_or_default();
        out.push(ext(CAMERA, "c_tag", str_ron(&c_tag)));
        out.push(ext(
            CAMERA,
            "network",
            list_ron(&list(types.resolved(path, "network"))),
        ));
        let range = num(types.resolved(path, "view_range")).unwrap_or(7.0);
        out.push(ext(CAMERA, "view_range", format!("{range:?}")));
        out.push(ext(
            CAMERA,
            "camera_enabled",
            truthy(types.resolved(path, "camera_enabled")).to_string(),
        ));
    } else if is_under(path, CAMERA_CONSOLE_ROOT) {
        out.push(ext(
            CONSOLE,
            "network",
            list_ron(&list(types.resolved(path, "network"))),
        ));
    } else if is_under(path, AI_SLIPPER_ROOT) {
        let uses = num(types.resolved(path, "uses")).unwrap_or(20.0);
        out.push(ext(SLIPPER, "uses", format!("{uses:?}")));
    }
    out
}

/// `.dmm` edits of `c_tag`, `network` and `view_range` on cameras, `network` on consoles,
/// `uses` on foam dispensers.
pub fn field_dmm(path: &str, var: &str, value: &DmValue) -> Option<Result<Field, Skip>> {
    if is_under(path, AI_SLIPPER_ROOT) {
        return match (var, value) {
            ("uses", DmValue::Number(n)) => Some(Ok(ext(SLIPPER, "uses", format!("{n:?}")))),
            ("uses", _) => Some(Err(Skip::Unsupported)),
            _ => None,
        };
    }
    let camera = is_under(path, CAMERA_ROOT);
    let component = if camera {
        CAMERA
    } else if is_under(path, CAMERA_CONSOLE_ROOT) {
        CONSOLE
    } else {
        return None;
    };
    Some(match (var, value) {
        ("network", DmValue::Null) => Ok(ext(component, "network", "[]")),
        ("network", DmValue::Str(s)) => {
            Ok(ext(component, "network", list_ron(std::slice::from_ref(s))))
        }
        ("network", DmValue::List(items)) => items
            .iter()
            .map(|(k, v)| match (k, v) {
                (DmValue::Str(s), None) => Some(s.clone()),
                _ => None,
            })
            .collect::<Option<Vec<String>>>()
            .map(|l| ext(component, "network", list_ron(&l)))
            .ok_or(Skip::Unsupported),
        ("network", _) => Err(Skip::Unsupported),
        ("c_tag", DmValue::Str(s)) if camera => Ok(ext(CAMERA, "c_tag", str_ron(s))),
        ("c_tag", DmValue::Null) if camera => Ok(ext(CAMERA, "c_tag", "\"\"")),
        ("c_tag", _) if camera => Err(Skip::Unsupported),
        ("view_range", DmValue::Number(n)) if camera => {
            Ok(ext(CAMERA, "view_range", format!("{n:?}")))
        }
        ("view_range", _) if camera => Err(Skip::Unsupported),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const TYPES: &str = r##"{
      "schema": 3,
      "types": {
        "/datum": { "parent": null },
        "/datum/ai_laws": { "parent": "/datum", "declared_vars": { "id": "default",
            "name": "Unknown Laws", "inherent": { "list": [] }, "zeroth": null } },
        "/datum/ai_laws/crewsimovpp": { "parent": "/datum/ai_laws", "declared_vars": {
            "id": "crewsimovpp", "name": "Крюзимов++", "inherent": { "list": ["A", "B"] } } },
        "/atom": { "parent": "/datum" },
        "/obj": { "parent": "/atom" },
        "/obj/item": { "parent": "/obj" },
        "/obj/item/pen": { "parent": "/obj/item" },
        "/obj/item/robot_model": { "parent": "/obj/item", "declared_vars": { "name": "Default",
            "cyborg_base_icon": "robot", "basic_modules": { "list": [] } } },
        "/obj/item/robot_model/service": { "parent": "/obj/item/robot_model", "declared_vars": {
            "name": "Service", "cyborg_base_icon": "service_m",
            "basic_modules": { "list": [{ "path": "/obj/item/pen" }, { "path": "/obj/item/nope" }] },
            "borg_skins": { "list": [
                { "assoc": ["Bro", { "list": [{ "assoc": ["skin_icon_state", "brobot"] }] }] },
                { "assoc": ["Waitress", { "list": [
                    { "assoc": ["skin_icon_state", "service_f"] },
                    { "assoc": ["skin_icon", { "resource": "modular_bandastation/borgs/icons/robots.dmi" }] },
                    { "assoc": ["skin_hat_offset", 1e31] }] }] }] },
            "radio_channels": { "list": ["Сервис"] } } },
        "/obj/item/robot_model/saboteur": { "parent": "/obj/item/robot_model", "declared_vars": {
            "name": "Syndicate Saboteur", "cyborg_base_icon": "synd_engi",
            "basic_modules": { "list": [{ "path": "/obj/item/pen" }] } } },
        "/obj/item/borg": { "parent": "/obj/item" },
        "/obj/item/borg/upgrade": { "parent": "/obj/item/borg", "declared_vars": {
            "locked": 0, "require_model": 0, "model_type": null, "one_use": 0,
            "items_to_add": null, "items_to_remove": null } },
        "/obj/item/borg/upgrade/amop": { "parent": "/obj/item/borg/upgrade", "declared_vars": {
            "require_model": 1, "model_type": { "list": [{ "path": "/obj/item/robot_model/service" }] },
            "items_to_add": { "list": [{ "path": "/obj/item/pen" }] },
            "items_to_remove": { "list": [{ "path": "/obj/item/mop" }] } } },
        "/obj/item/ai_module": { "parent": "/obj/item" },
        "/obj/item/ai_module/law": { "parent": "/obj/item/ai_module", "declared_vars": {
            "laws": { "list": [] } } },
        "/obj/item/ai_module/law/supplied/quarantine": { "parent": "/obj/item/ai_module/law",
            "declared_vars": { "laws": { "list": ["Q"] } } },
        "/obj/item/ai_module/law/core/full/crew": { "parent": "/obj/item/ai_module/law",
            "declared_vars": { "law_id": "crewsimovpp" } },
        "/obj/machinery": { "parent": "/obj" },
        "/obj/machinery/camera": { "parent": "/obj/machinery", "declared_vars": {
            "c_tag": null, "network": { "list": ["ss13"] }, "view_range": 7, "camera_enabled": 1 } },
        "/obj/machinery/camera/preset": { "parent": "/obj/machinery/camera" },
        "/obj/machinery/camera/preset/ordnance": { "parent": "/obj/machinery/camera/preset",
            "declared_vars": { "c_tag": "Bomb Testing Site", "network": { "list": ["rd", "ordnance"] } } }
      }
    }"##;

    fn get<'a>(fields: &'a [Field], key: &str) -> Option<&'a str> {
        fields.iter().find_map(|f| match f {
            Field::Ext { key: k, ron, .. } if *k == key => Some(ron.as_str()),
            _ => None,
        })
    }

    #[test]
    fn lawsets_models_and_cameras_resolve() {
        let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
        let data = resolve(&types);
        let crew = data.lawset("crewsimovpp").unwrap();
        assert_eq!(crew.inherent, ["A", "B"]);
        assert_eq!(crew.name, "Крюзимов++");
        assert_eq!(data.models.len(), 2);
        assert_eq!(data.models[0].basic_modules, ["/obj/item/pen"]);
        assert_eq!(data.models[0].radio_channels, ["Сервис"]);
        assert!(!data.models[0].hidden);
        let skins = &data.models[0].skins;
        assert_eq!(skins.len(), 2);
        assert_eq!(skins[0].icon_state.as_deref(), Some("brobot"));
        assert_eq!(skins[0].icon, None);
        assert_eq!(skins[1].name, "Waitress");
        assert_eq!(
            skins[1].icon.as_deref(),
            Some("modular_bandastation/borgs/icons/robots.dmi")
        );
        assert!(data.models[1].skins.is_empty());
        assert!(data.models[1].hidden, "{}", data.models[1].path);
        assert_eq!(data.upgrades.len(), 2);
        let amop = data.upgrade("/obj/item/borg/upgrade/amop").unwrap();
        assert!(amop.require_model && !amop.one_use && !amop.locked);
        assert_eq!(amop.model_type, ["/obj/item/robot_model/service"]);
        assert_eq!(amop.items_to_add, ["/obj/item/pen"]);
        assert_eq!(amop.items_to_remove, ["/obj/item/mop"]);
        assert!(data.spawned_types().contains("/obj/item/pen"));
        assert_eq!(data.modules.len(), 3);
        let q = data
            .module("/obj/item/ai_module/law/supplied/quarantine")
            .unwrap();
        assert_eq!(q.laws, ["Q"]);
        assert_eq!(q.law_id, None);
        let crew = data
            .module("/obj/item/ai_module/law/core/full/crew")
            .unwrap();
        assert_eq!(crew.law_id.as_deref(), Some("crewsimovpp"));
        assert!(
            data.spawned_types()
                .contains("/obj/item/ai_module/law/core/full/crew")
        );
        let cam = type_fields(&types, "/obj/machinery/camera/preset/ordnance");
        assert_eq!(get(&cam, "network"), Some(r#"["rd","ordnance"]"#));
        assert_eq!(get(&cam, "c_tag"), Some(r#""Bomb Testing Site""#));
        assert_eq!(get(&cam, "view_range"), Some("7.0"));
    }

    #[test]
    fn map_edits() {
        let p = "/obj/machinery/camera/directional/north";
        let list = DmValue::List(vec![
            (DmValue::Str("ss13".into()), None),
            (DmValue::Str("rd".into()), None),
        ]);
        assert_eq!(
            field_dmm(p, "network", &list),
            Some(Ok(ext(CAMERA, "network", r#"["ss13","rd"]"#)))
        );
        assert_eq!(
            field_dmm(p, "network", &DmValue::Str("tcommsat".into())),
            Some(Ok(ext(CAMERA, "network", r#"["tcommsat"]"#)))
        );
        assert_eq!(
            field_dmm(p, "c_tag", &DmValue::Str("Bar".into())),
            Some(Ok(ext(CAMERA, "c_tag", r#""Bar""#)))
        );
        assert_eq!(field_dmm("/obj/item/pen", "c_tag", &DmValue::Null), None);
        assert_eq!(field_dmm(p, "dir", &DmValue::Number(1.0)), None);
        assert_eq!(
            field_dmm(AI_SLIPPER_ROOT, "uses", &DmValue::Number(10.0)),
            Some(Ok(ext(SLIPPER, "uses", "10.0")))
        );
        assert_eq!(
            field_dmm(AI_SLIPPER_ROOT, "dir", &DmValue::Number(1.0)),
            None
        );
    }
}
