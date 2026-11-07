//! MODsuit data from the DM dump, written by `ss13-mapload convert` to [`MODSUIT_FILE`]: the
//! themes (`/datum/mod_theme/*`, `code/modules/mod/mod_theme.dm`, with their armor and skin
//! variants), the control units (`/obj/item/mod/control` and its `pre_equipped` subtypes,
//! `mod_types.dm`: theme, skin, core, cell, modules, default pins, access) and the modules
//! (`/obj/item/mod/module/*`, `code/modules/mod/modules/`: type, complexity, power costs,
//! cooldown, incompatibilities, required slots, storage limits, light). The game
//! (`ss13_game::modsuit`) reads it at startup.

use std::collections::BTreeSet;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::convert::DmTypes;
use crate::guns::DmArmor;
use crate::research::{list, path, paths, plain, subtypes, text, texts};

/// `<assets>/modsuits.ron`.
pub const MODSUIT_FILE: &str = "modsuits.ron";
pub const THEME_ROOT: &str = "/datum/mod_theme";
pub const CONTROL_ROOT: &str = "/obj/item/mod/control";
pub const MODULE_ROOT: &str = "/obj/item/mod/module";
pub const CORE_ROOT: &str = "/obj/item/mod/core";
/// The control unit's sounds: refusals, the part hiss, activation, installs.
pub const SOUNDS: [&str; 5] = [
    "sound/machines/scanner/scanbuzz.ogg",
    "sound/vehicles/mecha/mechmove03.ogg",
    "sound/machines/synth/synth_yes.ogg",
    "sound/machines/synth/synth_no.ogg",
    "sound/machines/click.ogg",
];
/// The parts `set_up_parts` creates (`variants[skin]` keys).
pub const PART_TYPES: [&str; 4] = [
    "/obj/item/clothing/head/mod",
    "/obj/item/clothing/suit/mod",
    "/obj/item/clothing/gloves/mod",
    "/obj/item/clothing/shoes/mod",
];

/// One part of a skin (`variants[skin][part type]`).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct PartDef {
    pub path: String,
    /// `UNSEALED_CLOTHING`, `SEALED_CLOTHING` (`clothing_flags`).
    pub unsealed_clothing: u32,
    pub sealed_clothing: u32,
    /// `UNSEALED_INVISIBILITY`, `SEALED_INVISIBILITY` (`flags_inv`).
    pub unsealed_inv: u32,
    pub sealed_inv: u32,
    /// `UNSEALED_COVER`, `SEALED_COVER` (`flags_cover`).
    pub unsealed_cover: u32,
    pub sealed_cover: u32,
    pub unsealed_message: String,
    pub sealed_message: String,
}

/// A `variants` entry.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct SkinDef {
    pub name: String,
    pub parts: Vec<PartDef>,
    /// `MOD_ICON_OVERRIDE`, `MOD_WORN_ICON_OVERRIDE`; empty: the default MOD files.
    pub icon: String,
    pub worn_icon: String,
}

/// A `/datum/mod_theme` type.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct ThemeDef {
    pub path: String,
    pub name: String,
    pub desc: String,
    pub extended_desc: String,
    pub default_skin: String,
    pub slot_flags: u32,
    pub armor: DmArmor,
    pub resistance_flags: u32,
    pub max_heat_protection_temperature: f32,
    pub min_cold_protection_temperature: f32,
    pub siemens_coefficient: f32,
    pub complexity_max: i32,
    /// Watts (`charge_drain` per second).
    pub charge_drain: f64,
    /// Deciseconds of slowdown with every part on.
    pub slowdown_deployed: f32,
    /// Deciseconds per part.
    pub activation_step_time: f32,
    pub ui_theme: String,
    pub inbuilt_modules: Vec<String>,
    pub allowed_suit_storage: Vec<String>,
    pub skins: Vec<SkinDef>,
    /// BandaStation `activation_sound`.
    pub activation_sound: String,
}

/// An `/obj/item/mod/control` type.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct SuitDef {
    pub path: String,
    pub theme: String,
    /// `applied_skin`; empty: the theme's default.
    pub skin: String,
    /// `applied_core`; empty: no core (a plain control unit).
    pub core: String,
    /// `applied_cell` (standard cores).
    pub cell: String,
    pub modules: Vec<String>,
    pub default_pins: Vec<String>,
    pub req_access: Vec<String>,
}

/// `/datum/storage` limits of a storage module's `storage_type`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub struct ModStorageDef {
    pub max_slots: u32,
    pub max_specific_storage: u8,
    pub max_total_storage: u32,
}

/// An `/obj/item/mod/module` type.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct ModuleDef {
    pub path: String,
    pub name: String,
    pub desc: String,
    /// `MODULE_PASSIVE` 0, `MODULE_USABLE` 1, `MODULE_TOGGLE` 2, `MODULE_ACTIVE` 3.
    pub module_type: u8,
    pub complexity: i32,
    /// Watts while idle / active, joules per use.
    pub idle_power_cost: f64,
    pub active_power_cost: f64,
    pub use_energy_cost: f64,
    /// Deciseconds.
    pub cooldown_time: f32,
    pub incompatible: Vec<String>,
    /// `required_slots`: each entry needs one of its `ITEM_SLOT_*` bits among the parts.
    pub required_slots: Vec<u32>,
    pub removable: bool,
    pub allow_flags: u32,
    pub tgui_id: String,
    pub storage: Option<ModStorageDef>,
    /// Flashlights: `light_range`, `light_power`, `light_color`, `min_range`, `max_range`.
    pub light: Option<(f32, f32, String, f32, f32)>,
    /// Magboots: `slowdown_active`.
    pub slowdown_active: f32,
    pub active_traits: Vec<String>,
    /// Worn overlays (`generate_worn_overlay`): `overlay_state_inactive`, `overlay_state_active`,
    /// `overlay_state_use` in `overlay_icon_file`; `mask_worn_overlay` ones are masked by the part.
    #[serde(default)]
    pub overlay_inactive: String,
    #[serde(default)]
    pub overlay_active: String,
    #[serde(default)]
    pub overlay_use: String,
    #[serde(default)]
    pub overlay_icon: String,
    #[serde(default)]
    pub mask_worn_overlay: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct ModData {
    pub themes: Vec<ThemeDef>,
    pub suits: Vec<SuitDef>,
    pub modules: Vec<ModuleDef>,
    /// Every `/obj/item/mod/core` type.
    pub cores: Vec<String>,
}

impl ModData {
    pub fn read(path: &Path) -> anyhow::Result<Self> {
        crate::read_ron(path)
    }

    pub fn to_ron(&self) -> String {
        let mut out = String::from("// Generated by `ss13-mapload convert`. Do not edit.\n");
        out.push_str(
            &ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())
                .expect("modsuit data serializes"),
        );
        out.push('\n');
        out
    }

    /// The types the game spawns: control units, their parts, modules, cores and cells.
    pub fn prototype_types(&self) -> BTreeSet<&str> {
        let mut out: BTreeSet<&str> = PART_TYPES.into_iter().collect();
        for s in &self.suits {
            out.insert(s.path.as_str());
            out.insert(s.core.as_str());
            out.insert(s.cell.as_str());
        }
        out.extend(self.modules.iter().map(|m| m.path.as_str()));
        out.extend(self.cores.iter().map(String::as_str));
        out.retain(|p| !p.is_empty());
        out
    }

    /// The sounds the game names: [`SOUNDS`] and the themes' activation sounds.
    pub fn sounds(&self) -> BTreeSet<&str> {
        self.themes
            .iter()
            .map(|t| t.activation_sound.as_str())
            .filter(|s| s.ends_with(".ogg"))
            .chain(SOUNDS)
            .collect()
    }
}

/// A number, also from a constant expression the extractor kept as text (`{"expr": "40"}`).
fn number(v: Option<&Value>) -> f64 {
    match v {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0),
        Some(Value::Object(o)) => o
            .get("expr")
            .and_then(Value::as_str)
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0.0),
        _ => 0.0,
    }
}

fn resource(v: Option<&Value>) -> String {
    v.and_then(|v| v.get("resource"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn type_path(v: Option<&Value>) -> String {
    path(v)
        .or_else(|| {
            v?.get("expr")?
                .as_str()
                .filter(|s| s.starts_with('/'))
                .map(str::to_owned)
        })
        .unwrap_or_default()
}

fn name(types: &DmTypes, p: &str) -> String {
    plain(&text(types.resolved(p, "name"))).replace("\\\"", "\"")
}

fn part(path: String, entries: Option<&Value>) -> PartDef {
    let mut out = PartDef {
        path,
        ..Default::default()
    };
    for (key, value) in list(entries) {
        let n = number(value) as u32;
        let s = text(value);
        match key.as_str().unwrap_or_default() {
            "unsealed_clothing" => out.unsealed_clothing = n,
            "sealed_clothing" => out.sealed_clothing = n,
            "unsealed_invisibility" => out.unsealed_inv = n,
            "sealed_invisibility" => out.sealed_inv = n,
            "unsealed_cover" => out.unsealed_cover = n,
            "sealed_cover" => out.sealed_cover = n,
            "unsealed_message" => out.unsealed_message = s,
            "sealed_message" => out.sealed_message = s,
            _ => {}
        }
    }
    out
}

fn skins(types: &DmTypes, p: &str) -> Vec<SkinDef> {
    list(types.resolved(p, "variants"))
        .into_iter()
        .filter_map(|(key, value)| {
            let mut skin = SkinDef {
                name: key.as_str()?.to_owned(),
                ..Default::default()
            };
            for (k, v) in list(value) {
                if let Some(part_path) = path(Some(k)) {
                    skin.parts.push(part(part_path, v));
                    continue;
                }
                match k.as_str().unwrap_or_default() {
                    "mod_icon_override" => skin.icon = resource(v),
                    "mod_worn_icon_override" => skin.worn_icon = resource(v),
                    _ => {}
                }
            }
            Some(skin)
        })
        .collect()
}

fn theme(types: &DmTypes, p: &str) -> ThemeDef {
    let n = |var| number(types.resolved(p, var));
    let t = |var| text(types.resolved(p, var));
    ThemeDef {
        path: p.to_owned(),
        name: name(types, p),
        desc: t("desc"),
        extended_desc: t("extended_desc"),
        default_skin: t("default_skin"),
        slot_flags: n("slot_flags") as u32,
        armor: crate::guns::armor(types, p),
        resistance_flags: n("resistance_flags") as u32,
        max_heat_protection_temperature: n("max_heat_protection_temperature") as f32,
        min_cold_protection_temperature: n("min_cold_protection_temperature") as f32,
        siemens_coefficient: n("siemens_coefficient") as f32,
        complexity_max: n("complexity_max") as i32,
        charge_drain: n("charge_drain"),
        slowdown_deployed: n("slowdown_deployed") as f32,
        activation_step_time: n("activation_step_time") as f32,
        ui_theme: t("ui_theme"),
        inbuilt_modules: paths(types.resolved(p, "inbuilt_modules")),
        allowed_suit_storage: paths(types.resolved(p, "allowed_suit_storage")),
        skins: skins(types, p),
        activation_sound: resource(types.resolved(p, "activation_sound")),
    }
}

fn suit(types: &DmTypes, p: &str) -> SuitDef {
    let theme = match type_path(types.resolved(p, "theme")) {
        t if t.is_empty() => format!("{THEME_ROOT}/standard"),
        t => t,
    };
    SuitDef {
        path: p.to_owned(),
        theme,
        skin: text(types.resolved(p, "applied_skin")),
        core: type_path(types.resolved(p, "applied_core")),
        cell: type_path(types.resolved(p, "applied_cell")),
        modules: paths(types.resolved(p, "applied_modules")),
        default_pins: paths(types.resolved(p, "default_pins")),
        req_access: texts(types.resolved(p, "req_access")),
    }
}

fn module(types: &DmTypes, p: &str) -> ModuleDef {
    let n = |var| number(types.resolved(p, var));
    let storage = Some(type_path(types.resolved(p, "storage_type")))
        .filter(|s| !s.is_empty())
        .map(|s| ModStorageDef {
            max_slots: number(types.resolved(&s, "max_slots")) as u32,
            max_specific_storage: number(types.resolved(&s, "max_specific_storage")) as u8,
            max_total_storage: number(types.resolved(&s, "max_total_storage")) as u32,
        });
    let light = types.resolved(p, "min_range").is_some().then(|| {
        (
            n("light_range") as f32,
            n("light_power") as f32,
            text(types.resolved(p, "light_color")),
            n("min_range") as f32,
            n("max_range") as f32,
        )
    });
    ModuleDef {
        path: p.to_owned(),
        name: name(types, p),
        desc: text(types.resolved(p, "desc")),
        module_type: n("module_type") as u8,
        complexity: n("complexity") as i32,
        idle_power_cost: n("idle_power_cost"),
        active_power_cost: n("active_power_cost"),
        use_energy_cost: n("use_energy_cost"),
        cooldown_time: n("cooldown_time") as f32,
        incompatible: paths(types.resolved(p, "incompatible_modules")),
        required_slots: list(types.resolved(p, "required_slots"))
            .into_iter()
            .map(|(k, _)| number(Some(k)) as u32)
            .collect(),
        // `removable = TRUE` on `/obj/item/mod/module`.
        removable: types.resolved(p, "removable").is_none() || n("removable") != 0.0,
        allow_flags: n("allow_flags") as u32,
        tgui_id: text(types.resolved(p, "tgui_id")),
        storage,
        light,
        slowdown_active: n("slowdown_active") as f32,
        active_traits: texts(types.resolved(p, "active_traits")),
        overlay_inactive: text(types.resolved(p, "overlay_state_inactive")),
        overlay_active: text(types.resolved(p, "overlay_state_active")),
        overlay_use: text(types.resolved(p, "overlay_state_use")),
        overlay_icon: resource(types.resolved(p, "overlay_icon_file")),
        mask_worn_overlay: n("mask_worn_overlay") != 0.0,
    }
}

pub fn resolve(types: &DmTypes) -> ModData {
    let themes = subtypes(types, THEME_ROOT)
        .into_iter()
        .map(|p| theme(types, p))
        .collect();
    let suits = std::iter::once(CONTROL_ROOT)
        .chain(subtypes(types, CONTROL_ROOT))
        .filter(|p| types.types.contains_key(*p))
        .map(|p| suit(types, p))
        .collect();
    let modules = subtypes(types, MODULE_ROOT)
        .into_iter()
        .map(|p| module(types, p))
        .collect();
    let cores = subtypes(types, CORE_ROOT)
        .into_iter()
        .map(str::to_owned)
        .collect();
    ModData {
        themes,
        suits,
        modules,
        cores,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn types(json: &str) -> DmTypes {
        DmTypes::parse(json.as_bytes()).unwrap()
    }

    #[test]
    fn themes_suits_and_modules_resolve() {
        let types = types(
            r#"{"schema": 3, "types": {
            "/datum/armor": {"parent": null, "vars": {}, "declared_vars": {}},
            "/datum/armor/mod_theme_engineering": {"parent": "/datum/armor", "vars": {},
                "declared_vars": {"melee": {"expr": "10"}, "fire": {"expr": "100"}}},
            "/datum/mod_theme": {"parent": null, "vars": {}, "declared_vars": {
                "abstract_type": {"path": "/datum/mod_theme"}, "complexity_max": 15,
                "charge_drain": 50, "slowdown_deployed": 0.75, "activation_step_time": 10,
                "slot_flags": 1024,
                "activation_sound": {"resource": "sound/vehicles/mecha/nominal.ogg"}}},
            "/datum/mod_theme/engineering": {"parent": "/datum/mod_theme", "vars": {},
                "declared_vars": {"name": "engineering", "default_skin": "engineering",
                "armor_type": {"path": "/datum/armor/mod_theme_engineering"},
                "slowdown_deployed": 1,
                "variants": {"list": [{"assoc": ["engineering", {"list": [
                    {"assoc": [{"path": "/obj/item/clothing/head/mod"}, {"list": [
                        {"assoc": ["sealed_clothing", 2066]},
                        {"assoc": ["sealed_message", "hisses closed"]}]}]}]}]}]}}},
            "/datum/storage": {"parent": null, "vars": {}, "declared_vars": {
                "max_slots": 7, "max_specific_storage": 3, "max_total_storage": 14}},
            "/datum/storage/mod_storage": {"parent": "/datum/storage", "vars": {},
                "declared_vars": {"max_total_storage": 15}},
            "/obj/item/mod/control": {"parent": null, "vars": {}, "declared_vars": {}},
            "/obj/item/mod/control/pre_equipped": {"parent": "/obj/item/mod/control",
                "vars": {}, "declared_vars": {
                "applied_core": {"path": "/obj/item/mod/core/standard"},
                "applied_cell": {"path": "/obj/item/stock_parts/power_store/cell/super"}}},
            "/obj/item/mod/control/pre_equipped/engineering": {
                "parent": "/obj/item/mod/control/pre_equipped", "vars": {}, "declared_vars": {
                "theme": {"path": "/datum/mod_theme/engineering"},
                "applied_modules": {"list": [{"path": "/obj/item/mod/module/storage"}]},
                "req_access": {"list": ["engineering"]}}},
            "/obj/item/mod/module": {"parent": null, "vars": {}, "declared_vars": {
                "abstract_type": {"path": "/obj/item/mod/module"}, "removable": 1}},
            "/obj/item/mod/module/storage": {"parent": "/obj/item/mod/module", "vars": {},
                "declared_vars": {"name": "\\improper MOD storage", "complexity": 1,
                "required_slots": {"list": [1024]},
                "storage_type": {"path": "/datum/storage/mod_storage"}}}
            }}"#,
        );
        let data = resolve(&types);
        let theme = &data.themes[0];
        assert_eq!(theme.path, "/datum/mod_theme/engineering");
        assert_eq!(theme.armor.fire, 100.0);
        assert_eq!(theme.slowdown_deployed, 1.0);
        assert_eq!(theme.complexity_max, 15);
        assert_eq!(theme.skins[0].parts[0].sealed_clothing, 2066);
        assert_eq!(theme.skins[0].parts[0].sealed_message, "hisses closed");
        let control = data.suits.iter().find(|s| s.path == CONTROL_ROOT).unwrap();
        assert_eq!(control.theme, "/datum/mod_theme/standard");
        assert!(control.core.is_empty());
        let eng = data
            .suits
            .iter()
            .find(|s| s.path.ends_with("engineering"))
            .unwrap();
        assert_eq!(eng.core, "/obj/item/mod/core/standard");
        assert_eq!(eng.req_access, ["engineering"]);
        let storage = &data.modules[0];
        assert_eq!(storage.name, "MOD storage");
        assert_eq!(storage.required_slots, [1024]);
        assert!(storage.removable);
        assert_eq!(
            storage.storage,
            Some(ModStorageDef {
                max_slots: 7,
                max_specific_storage: 3,
                max_total_storage: 15
            })
        );
        assert!(
            data.prototype_types()
                .contains("/obj/item/clothing/head/mod")
        );
    }
}
