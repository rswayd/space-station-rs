//! Basic mob data from the DM dump (`/mob/living/basic`, `code/modules/mob/living/basic/`),
//! written by `ss13-mapload convert` to [`MOBS_FILE`]: health, physiology, melee, verbs (with
//! BandaStation's `ru_verbs.toml` translations), environment tolerances, icons, factions and
//! the AI controller type. Every basic mob prototype also carries [`DmMob`] (its own path), so
//! the game finds its definition wherever the mob is spawned (map load or at runtime).

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use anyhow::Context as _;
use bevy::prelude::{Component, Reflect};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::convert::DmTypes;
use crate::fidelity::{ext, is_under};
use crate::research::{list, path as dm_path, plain, text};
use crate::value::Field;

/// `<assets>/mobs.ron`.
pub const MOBS_FILE: &str = "mobs.ron";
pub const BASIC_ROOT: &str = "/mob/living/basic";
const COMPONENT: &str = "dm_mob";

/// Types the game spawns by itself: the carp of the migration event (tg `carp_type`,
/// `boss_type`), round-start vermin (tg `SSminor_mapping`), and the dead mouse a mouse leaves.
pub const MOB_RUNTIME_TYPES: [&str; 5] = [
    "/mob/living/basic/carp",
    "/mob/living/basic/carp/mega",
    "/mob/living/basic/mouse",
    "/mob/living/basic/cockroach",
    "/obj/item/food/deadmouse",
];

/// The DM type of a basic mob (prototype data): the key into [`MobsData::mobs`].
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmMob {
    pub path: String,
}

/// `habitable_atmos` (mol), 0 = no limit (tg `atmos_requirements`).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub struct HabitableAtmos {
    pub min_oxy: f32,
    pub max_oxy: f32,
    pub min_plas: f32,
    pub max_plas: f32,
    pub min_co2: f32,
    pub max_co2: f32,
    pub min_n2: f32,
    pub max_n2: f32,
}

/// One `/mob/living/basic` type, vars resolved through its parents.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct MobDef {
    pub path: String,
    pub name: String,
    /// `male`, `female`, `neuter` or `plural` (tg randomizes plural ones).
    pub gender: String,
    pub max_health: f32,
    pub health: f32,
    /// `physiology` damage multipliers: brute, burn, toxin, oxygen, stamina.
    pub physiology: [f32; 5],
    pub melee_damage_lower: f32,
    pub melee_damage_upper: f32,
    /// `brute`, `burn`, ...
    pub melee_damage_type: String,
    /// Deciseconds.
    pub melee_attack_cooldown: f32,
    pub obj_damage: f32,
    /// Russian (`ru_attack_verb`) where BandaStation translates them, else as in DM.
    pub attack_verb_continuous: String,
    pub attack_verb_simple: String,
    pub friendly_verb_continuous: String,
    pub friendly_verb_simple: String,
    pub response_help_continuous: String,
    pub response_help_simple: String,
    pub response_disarm_continuous: String,
    pub response_disarm_simple: String,
    pub response_harm_continuous: String,
    pub response_harm_simple: String,
    pub attack_sound: String,
    pub attacked_sound: String,
    pub death_sound: String,
    pub death_message: String,
    /// tg `speed`: deciseconds added per tile (`simplemob_varspeed`).
    pub speed: f32,
    pub basic_mob_flags: u32,
    pub habitable_atmos: Option<HabitableAtmos>,
    pub unsuitable_atmos_damage: f32,
    pub minimum_survivable_temperature: f32,
    pub maximum_survivable_temperature: f32,
    pub unsuitable_cold_damage: f32,
    pub unsuitable_heat_damage: f32,
    pub icon: String,
    pub icon_living: String,
    pub icon_dead: String,
    pub density: bool,
    pub mob_size: u8,
    /// Faction names (`FACTION_*` values).
    pub faction: Vec<String>,
    /// `ai_controller` type, empty without one.
    pub ai_controller: String,
    /// `speak_emote`, translated (`ru_say_verb`).
    pub speak_emote: Vec<String>,
    /// BandaStation mice: `body_icon_state` + `_` + color (`animal_variety`).
    pub body_icon_state: String,
    pub body_color: String,
    pub possible_body_colors: Vec<String>,
    /// Carp: `cowardly` (no `ai_flee_while_injured`).
    pub cowardly: bool,
    /// Mice: `cable_zap_prob`.
    pub cable_zap_prob: f32,
    /// `AddElement(/datum/element/simple_flying)` in its `Initialize` (or an ancestor's): it
    /// flies (`movement_type |= FLYING`) and does not fall through open space.
    #[serde(default)]
    pub flying: bool,
    /// `/mob/living/basic/bot` vars, for bots only.
    #[serde(default)]
    pub bot: Option<BotDef>,
}

/// The vars of a `/mob/living/basic/bot` type (`_bots.dm` and the bot subtypes).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct BotDef {
    /// `bot_mode_flags` (`BOT_MODE_*`).
    pub mode_flags: u32,
    /// `bot_access_flags` (`BOT_COVER_*`).
    pub access_flags: u32,
    pub req_one_access: Vec<String>,
    /// `ACCESS_ROBOTICS` plus the `additional_access` trim's `access` and wildcards
    /// (`provide_additional_access`).
    pub access: Vec<String>,
    /// `radio_channel`, as the channel's name.
    pub radio_channel: String,
    pub icon_state: String,
    pub base_icon_state: String,
    /// Secbots: `security_mode_flags`, `commissioned`.
    pub security_mode_flags: u32,
    pub commissioned: bool,
    /// Medbots: `medical_mode_flags`, `heal_amount`, `heal_threshold`, `damage_type_healer`,
    /// `skin`.
    pub medical_mode_flags: u32,
    pub heal_amount: f32,
    pub heal_threshold: f32,
    pub damage_type_healer: String,
    pub skin: String,
    /// Cleanbots: `janitor_mode_flags`.
    pub janitor_mode_flags: u32,
}

/// `/mob/living/basic/bot`.
pub const BOT_ROOT: &str = "/mob/living/basic/bot";

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct MobsData {
    pub mobs: BTreeMap<String, MobDef>,
}

impl MobsData {
    pub fn read(path: &Path) -> anyhow::Result<Self> {
        let text = ss13_data::read_to_string(path)
            .with_context(|| format!("reading {}", path.display()))?;
        ron::from_str(&text).with_context(|| format!("parsing {}", path.display()))
    }

    pub fn to_ron(&self) -> String {
        let mut out = String::from("// Generated by `ss13-mapload convert`. Do not edit.\n");
        out.push_str(
            &ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())
                .expect("mob data serializes"),
        );
        out.push('\n');
        out
    }
}

/// BandaStation's verb translations (`modular_bandastation/translations/public/ru_verbs.toml`).
#[derive(Default, Debug)]
pub struct RuVerbs {
    pub attack: HashMap<String, String>,
    pub say: HashMap<String, String>,
}

impl RuVerbs {
    pub fn read(source: &Path) -> Self {
        let path = source.join("modular_bandastation/translations/public/ru_verbs.toml");
        std::fs::read_to_string(path)
            .map(|t| Self::parse(&t))
            .unwrap_or_default()
    }

    /// `[section]` headers and `"key" = "value"` lines; nothing else of TOML is used there.
    pub fn parse(text: &str) -> Self {
        let mut out = Self::default();
        let mut section = "";
        for line in text.lines().map(str::trim) {
            if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                section = match name {
                    "attack_verbs" => "attack",
                    "say_verbs" => "say",
                    _ => "",
                };
                continue;
            }
            let Some((key, value)) = line.split_once(" = ") else {
                continue;
            };
            let unquote = |s: &str| s.trim().trim_matches('"').to_owned();
            let map = match section {
                "attack" => &mut out.attack,
                "say" => &mut out.say,
                _ => continue,
            };
            map.insert(unquote(key), unquote(value));
        }
        out
    }

    fn attack_verb(&self, verb: &str) -> String {
        self.attack
            .get(verb)
            .cloned()
            .unwrap_or_else(|| verb.into())
    }
}

/// `#define FACTION_X "y"` values (`code/__DEFINES/mobfactions.dm`).
pub fn read_factions(source: &Path) -> HashMap<String, String> {
    let text =
        std::fs::read_to_string(source.join("code/__DEFINES/mobfactions.dm")).unwrap_or_default();
    parse_defines(&text)
}

fn parse_defines(text: &str) -> HashMap<String, String> {
    text.lines()
        .filter_map(|l| {
            let rest = l.trim().strip_prefix("#define ")?;
            let (name, value) = rest.split_once(char::is_whitespace)?;
            let value = value.trim();
            let value = value.split("//").next()?.trim().strip_prefix('"')?;
            Some((name.to_owned(), value.strip_suffix('"')?.to_owned()))
        })
        .collect()
}

/// The `dm_mob` prototype field of a basic mob type.
pub fn type_fields(types: &DmTypes, path: &str) -> Vec<Field> {
    if !is_under(path, BASIC_ROOT) || !types.types.contains_key(path) {
        return Vec::new();
    }
    let ron = ron::to_string(path).expect("strings always serialize");
    vec![ext(COMPONENT, "path", ron)]
}

/// Every `/mob/living/basic` type.
pub fn resolve(types: &DmTypes, verbs: &RuVerbs, factions: &HashMap<String, String>) -> MobsData {
    let mobs = types
        .types
        .keys()
        .filter(|p| is_under(p, BASIC_ROOT))
        .map(|p| (p.clone(), mob_def(types, p, verbs, factions)))
        .collect();
    MobsData { mobs }
}

/// The element that makes basic mobs fly.
const SIMPLE_FLYING: &str = "/datum/element/simple_flying";

/// `simple_flying` added in the `Initialize` of `path` or an ancestor (calls in other procs come
/// and go with status effects and fish traits).
fn flies(types: &DmTypes, path: &str) -> bool {
    types
        .element_calls
        .get(SIMPLE_FLYING)
        .into_iter()
        .flatten()
        .any(|c| {
            c.call == "AddElement"
                && c.proc == "Initialize"
                && crate::fidelity::is_under(path, &c.owner)
        })
}

fn mob_def(
    types: &DmTypes,
    path: &str,
    verbs: &RuVerbs,
    factions: &HashMap<String, String>,
) -> MobDef {
    let v = |var: &str| types.resolved(path, var);
    let num =
        |var: &str, default: f32| v(var).and_then(Value::as_f64).map_or(default, |n| n as f32);
    let s = |var: &str| text(v(var));
    let resource = |var: &str| {
        v(var)
            .and_then(|v| v.get("resource").and_then(Value::as_str).or(v.as_str()))
            .unwrap_or_default()
            .to_owned()
    };
    let verb = |var: &str| verbs.attack_verb(&s(var));
    let mut physiology = [1.0; 5];
    for (key, value) in list(v("physiology")) {
        let i = match key.as_str() {
            Some("brute") => 0,
            Some("burn") => 1,
            Some("toxin") => 2,
            Some("oxygen") => 3,
            Some("stamina") => 4,
            _ => continue,
        };
        physiology[i] = value.and_then(Value::as_f64).unwrap_or(1.0) as f32;
    }
    let habitable_atmos = (!v("habitable_atmos").is_none_or(Value::is_null)).then(|| {
        let mut a = HabitableAtmos::default();
        for (key, value) in list(v("habitable_atmos")) {
            let n = value.and_then(Value::as_f64).unwrap_or(0.0) as f32;
            match key.as_str().unwrap_or_default() {
                "min_oxy" => a.min_oxy = n,
                "max_oxy" => a.max_oxy = n,
                "min_plas" => a.min_plas = n,
                "max_plas" => a.max_plas = n,
                "min_co2" => a.min_co2 = n,
                "max_co2" => a.max_co2 = n,
                "min_n2" => a.min_n2 = n,
                "max_n2" => a.max_n2 = n,
                _ => {}
            }
        }
        a
    });
    // `faction = list(FACTION_CARP)` is dumped as an expression.
    let faction = v("faction")
        .and_then(|f| f.get("expr"))
        .and_then(Value::as_str)
        .map(|expr| {
            expr.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .filter_map(|token| factions.get(token).cloned())
                .collect()
        })
        .unwrap_or_else(|| vec!["neutral".to_owned()]);
    let speak_emote = list(v("speak_emote"))
        .into_iter()
        .filter_map(|(k, _)| k.as_str())
        .map(|verb| verbs.say.get(verb).cloned().unwrap_or_else(|| verb.into()))
        .collect();
    let possible_body_colors = list(v("possible_body_colors"))
        .into_iter()
        .filter_map(|(k, _)| k.as_str().map(str::to_owned))
        .collect();
    let max_health = num("maxHealth", 20.0);
    MobDef {
        path: path.to_owned(),
        name: plain(&s("name")),
        gender: s("gender"),
        max_health,
        health: num("health", max_health),
        physiology,
        melee_damage_lower: num("melee_damage_lower", 0.0),
        melee_damage_upper: num("melee_damage_upper", 0.0),
        melee_damage_type: s("melee_damage_type"),
        melee_attack_cooldown: num("melee_attack_cooldown", 20.0),
        obj_damage: num("obj_damage", 0.0),
        attack_verb_continuous: verb("attack_verb_continuous"),
        attack_verb_simple: verb("attack_verb_simple"),
        friendly_verb_continuous: verb("friendly_verb_continuous"),
        friendly_verb_simple: verb("friendly_verb_simple"),
        response_help_continuous: verb("response_help_continuous"),
        response_help_simple: verb("response_help_simple"),
        response_disarm_continuous: verb("response_disarm_continuous"),
        response_disarm_simple: verb("response_disarm_simple"),
        response_harm_continuous: verb("response_harm_continuous"),
        response_harm_simple: verb("response_harm_simple"),
        attack_sound: resource("attack_sound"),
        attacked_sound: resource("attacked_sound"),
        death_sound: resource("death_sound"),
        death_message: s("death_message"),
        speed: num("speed", 1.0),
        basic_mob_flags: num("basic_mob_flags", 0.0) as u32,
        habitable_atmos,
        unsuitable_atmos_damage: num("unsuitable_atmos_damage", 1.0),
        minimum_survivable_temperature: num("minimum_survivable_temperature", 250.0),
        maximum_survivable_temperature: num("maximum_survivable_temperature", 350.0),
        unsuitable_cold_damage: num("unsuitable_cold_damage", 1.0),
        unsuitable_heat_damage: num("unsuitable_heat_damage", 1.0),
        icon: resource("icon"),
        icon_living: s("icon_living"),
        icon_dead: s("icon_dead"),
        density: v("density")
            .and_then(Value::as_f64)
            .is_none_or(|d| d != 0.0),
        mob_size: num("mob_size", 2.0) as u8,
        faction,
        ai_controller: dm_path(v("ai_controller")).unwrap_or_default(),
        speak_emote,
        body_icon_state: s("body_icon_state"),
        body_color: s("body_color"),
        possible_body_colors,
        cowardly: v("cowardly")
            .and_then(Value::as_f64)
            .is_some_and(|c| c != 0.0),
        flying: flies(types, path),
        cable_zap_prob: num("cable_zap_prob", 0.0),
        bot: is_under(path, BOT_ROOT).then(|| bot_def(types, path)),
    }
}

fn bot_def(types: &DmTypes, path: &str) -> BotDef {
    let v = |var: &str| types.resolved(path, var);
    let int = |var: &str| v(var).and_then(Value::as_f64).unwrap_or(0.0) as i64 as u32;
    let num = |var: &str| v(var).and_then(Value::as_f64).unwrap_or(0.0) as f32;
    let strings = |value: Option<&Value>| -> Vec<String> {
        list(value)
            .into_iter()
            .filter_map(|(k, _)| k.as_str().map(str::to_owned))
            .collect()
    };
    let flag_groups = crate::jobs::flag_groups(types);
    let mut access = vec!["robotics".to_owned()];
    if let Some(trim) = dm_path(v("additional_access")) {
        let (trim_access, _) = crate::jobs::trim_access(types, &trim, &flag_groups);
        for a in strings(types.resolved(&trim, "access"))
            .into_iter()
            .chain(trim_access)
            .chain(crate::jobs::trim_wildcard_access(
                types,
                &trim,
                &flag_groups,
            ))
        {
            if !access.contains(&a) {
                access.push(a);
            }
        }
    }
    BotDef {
        mode_flags: int("bot_mode_flags"),
        access_flags: int("bot_access_flags"),
        req_one_access: strings(v("req_one_access")),
        access,
        radio_channel: text(v("radio_channel")),
        icon_state: text(v("icon_state")),
        base_icon_state: text(v("base_icon_state")),
        security_mode_flags: int("security_mode_flags"),
        commissioned: num("commissioned") != 0.0,
        medical_mode_flags: int("medical_mode_flags"),
        heal_amount: num("heal_amount"),
        heal_threshold: num("heal_threshold"),
        damage_type_healer: text(v("damage_type_healer")),
        skin: text(v("skin")),
        janitor_mode_flags: int("janitor_mode_flags"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TYPES: &str = r#"{
        "schema": 3,
        "types": {
            "/atom": { "parent": null },
            "/mob": { "parent": "/atom", "declared_vars": { "faction": { "expr": "list(FACTION_NEUTRAL)" } } },
            "/mob/living": { "parent": "/mob", "declared_vars": { "maxHealth": 100, "health": 100 } },
            "/mob/living/basic": { "parent": "/mob/living", "declared_vars": {
                "maxHealth": 20, "health": 20, "speed": 1, "melee_attack_cooldown": 20,
                "attack_verb_continuous": "атакует", "response_help_continuous": "тычет",
                "habitable_atmos": { "list": [ { "assoc": ["min_oxy", 5] }, { "assoc": ["max_co2", 5] } ] },
                "minimum_survivable_temperature": 250, "maximum_survivable_temperature": 350,
                "icon": { "resource": "icons/mob/simple/animal.dmi" } } },
            "/mob/living/basic/carp": { "parent": "/mob/living/basic", "declared_vars": {
                "name": "space carp", "maxHealth": 25, "health": 25, "habitable_atmos": null,
                "melee_damage_lower": 20, "melee_damage_upper": 20, "attack_verb_continuous": "bites",
                "response_help_continuous": "pets", "faction": { "expr": "list(FACTION_CARP)" },
                "ai_controller": { "path": "/datum/ai_controller/basic_controller/carp" },
                "icon_living": "base", "icon_dead": "base_dead", "minimum_survivable_temperature": 0,
                "speak_emote": { "list": ["squeaks"] },
                "physiology": { "list": [ { "assoc": ["brute", 3] } ] } } },
            "/mob/living/basic/carp/lia": { "parent": "/mob/living/basic/carp", "declared_vars": { "name": "\\improper Lia" } }
        }
    }"#;

    fn data() -> MobsData {
        let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
        let verbs = RuVerbs::parse(
            "[attack_verbs]\n\"pets\" = \"гладит\"\n[eat_verbs]\n\"bites\" = \"кусаете\"\n\
             [say_verbs]\n\"squeaks\" = \"пищит\"\n",
        );
        let factions = parse_defines(
            "#define FACTION_NEUTRAL \"neutral\"\n#define FACTION_CARP \"carp\" // fish\n",
        );
        resolve(&types, &verbs, &factions)
    }

    #[test]
    fn vars_resolve_through_parents_and_translate() {
        let data = data();
        let carp = &data.mobs["/mob/living/basic/carp"];
        assert_eq!(carp.max_health, 25.0);
        assert_eq!(carp.speed, 1.0, "inherited");
        assert_eq!(carp.melee_damage_upper, 20.0);
        assert_eq!(carp.response_help_continuous, "гладит");
        // Only `[attack_verbs]` translates attacks: "bites" stays (it is an eat verb there).
        assert_eq!(carp.attack_verb_continuous, "bites");
        assert_eq!(carp.speak_emote, ["пищит"]);
        assert_eq!(carp.faction, ["carp"]);
        assert_eq!(carp.habitable_atmos, None, "space carp breathe nothing");
        assert_eq!(carp.minimum_survivable_temperature, 0.0);
        assert_eq!(carp.physiology, [3.0, 1.0, 1.0, 1.0, 1.0]);
        assert_eq!(carp.icon, "icons/mob/simple/animal.dmi");
        assert_eq!(
            carp.ai_controller,
            "/datum/ai_controller/basic_controller/carp"
        );
        let lia = &data.mobs["/mob/living/basic/carp/lia"];
        assert_eq!(lia.name, "Lia");
        assert_eq!(lia.icon_dead, "base_dead");
        let base = &data.mobs["/mob/living/basic"];
        assert_eq!(base.faction, ["neutral"]);
        let atmos = base.habitable_atmos.unwrap();
        assert_eq!(
            (atmos.min_oxy, atmos.max_co2, atmos.max_oxy),
            (5.0, 5.0, 0.0)
        );
    }

    #[test]
    fn every_basic_type_gets_its_path_component() {
        let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
        assert_eq!(type_fields(&types, "/mob/living/basic/carp").len(), 1);
        assert!(type_fields(&types, "/mob/living").is_empty());
        let data = data();
        assert_eq!(data, ron::from_str(&data.to_ron()).unwrap());
    }
}
