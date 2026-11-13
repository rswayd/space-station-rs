//! Basic mob data from the DM dump (`/mob/living/basic`, `code/modules/mob/living/basic/`),
//! written by `ss13-mapload convert` to [`MOBS_FILE`]: health, physiology, melee, verbs (with
//! BandaStation's `ru_verbs.toml` translations), environment tolerances, icons, factions and
//! the AI controller type. Every basic mob prototype also carries [`DmMob`] (its own path), so
//! the game finds its definition wherever the mob is spawned (map load or at runtime).

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

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
/// tg `/mob/living/simple_animal/hostile/megafauna`: the lavaland bosses, read as basic mobs
/// (`del_on_death` as `DEL_ON_DEATH`, `minbodytemp`/`maxbodytemp` as the survivable range).
pub const MEGAFAUNA_ROOT: &str = "/mob/living/simple_animal/hostile/megafauna";
/// tg `/mob/living/carbon/alien`: the xenomorph castes, read as basic mobs with their icon
/// states as the living and dead sprites and tox and stamina immune.
pub const ALIEN_ROOT: &str = "/mob/living/carbon/alien";
/// tg `/mob/living/simple_animal/hostile/ooze`: read as basic mobs like the megafauna.
pub const OOZE_ROOT: &str = "/mob/living/simple_animal/hostile/ooze";
/// tg lavaland elites (`mining_mobs/elites/`): read as basic mobs like the megafauna.
pub const ELITE_ROOT: &str = "/mob/living/simple_animal/hostile/asteroid/elite";
const COMPONENT: &str = "dm_mob";
/// `CANPUSH` of `status_flags`; `MOVE_RESIST_DEFAULT`.
const CANPUSH: u32 = 1 << 3;
const MOVE_RESIST_DEFAULT: f32 = 1000.0;
/// `BODYTEMP_HEAT_DAMAGE_LIMIT`.
const ALIEN_MAX_TEMP: f32 = 360.15;

/// Types the game spawns by itself: the carp of the migration event (tg `carp_type`,
/// `boss_type`), round-start vermin (tg `SSminor_mapping`), and the dead mouse a mouse leaves.
pub const MOB_RUNTIME_TYPES: [&str; 16] = [
    "/mob/living/basic/carp",
    "/mob/living/basic/carp/mega",
    "/mob/living/basic/mouse",
    "/mob/living/basic/cockroach",
    "/obj/item/food/deadmouse",
    // Guardians a creator summons (`ss13_game::mobs::guardian`).
    "/mob/living/basic/guardian/assassin",
    "/mob/living/basic/guardian/charger",
    "/mob/living/basic/guardian/dextrous",
    "/mob/living/basic/guardian/explosive",
    "/mob/living/basic/guardian/gaseous",
    "/mob/living/basic/guardian/gravitokinetic",
    "/mob/living/basic/guardian/lightning",
    "/mob/living/basic/guardian/protector",
    "/mob/living/basic/guardian/ranged",
    "/mob/living/basic/guardian/standard",
    "/mob/living/basic/guardian/support",
];

/// Lavaland fauna the game spawns by itself (`ss13_game::mobs::lavaland`): the cave population
/// (tg `/datum/biome/lavaland/basalt` `fauna_types`), the legion's skulls and the legions they
/// make of corpses, the hivelord's brood, the watchers' projectiles and the death drops.
pub const LAVALAND_RUNTIME_TYPES: [&str; 24] = [
    "/mob/living/basic/mining/goliath",
    "/mob/living/basic/mining/goliath/ancient/immortal",
    "/mob/living/basic/mining/legion",
    "/mob/living/basic/mining/legion/dwarf",
    "/mob/living/basic/mining/legion_brood",
    "/mob/living/basic/mining/watcher",
    "/mob/living/basic/mining/watcher/magmawing",
    "/mob/living/basic/mining/watcher/icewing",
    "/mob/living/basic/mining/bileworm",
    "/mob/living/basic/mining/lobstrosity/lava",
    "/mob/living/basic/mining/goldgrub",
    "/mob/living/basic/mining/brimdemon",
    "/mob/living/basic/mining/hivelord",
    "/mob/living/basic/hivelord_brood",
    "/mob/living/basic/mining/basilisk",
    "/obj/projectile/temp/watcher",
    "/obj/projectile/temp/watcher/magma_wing",
    "/obj/projectile/temp/watcher/ice_wing",
    "/obj/item/organ/monster_core/regenerative_core",
    "/obj/item/organ/monster_core/regenerative_core/legion",
    "/obj/item/stack/ore/silver",
    "/obj/item/stack/ore/gold",
    "/obj/item/stack/ore/uranium",
    "/obj/item/stack/ore/diamond",
];

/// What `ss13_game::mobs::jungle` spawns: the leaper's suicide frogs, bubbles and sludge, the
/// projectiles of the leaper, the mega arachnid and seedlings, the arachnid's snare and acid,
/// the venus human trap a flower bud grows.
pub const JUNGLE_RUNTIME_TYPES: [&str; 10] = [
    "/mob/living/basic/frog/suicide",
    "/mob/living/basic/venus_human_trap",
    "/obj/structure/leaper_bubble",
    "/obj/effect/decal/cleanable/leaper_sludge",
    "/obj/projectile/leaper",
    "/obj/projectile/mega_arachnid",
    "/obj/projectile/seedling",
    "/obj/item/restraints/legcuffs/beartrap/mega_arachnid",
    "/obj/effect/slippery_acid",
    "/obj/structure/alien/resin/flower_bud",
];

/// What the elite tumor makes (`ss13_game::mobs::megafauna::elites`): the four elites, the
/// broodmother's babies, the tumor shard and the elites' trophies.
pub const ELITE_RUNTIME_TYPES: [&str; 9] = [
    "/mob/living/simple_animal/hostile/asteroid/elite/broodmother",
    "/mob/living/simple_animal/hostile/asteroid/elite/broodmother_child",
    "/mob/living/simple_animal/hostile/asteroid/elite/pandora",
    "/mob/living/simple_animal/hostile/asteroid/elite/legionnaire",
    "/mob/living/simple_animal/hostile/asteroid/elite/herald",
    "/obj/item/tumor_shard",
    "/obj/item/crusher_trophy/broodmother_tongue",
    "/obj/item/clothing/accessory/pandora_hope",
    "/obj/item/clothing/neck/cloak/herald_cloak",
];

/// What `ss13_game::mobs::megafauna` spawns: the ported bosses (placed on the mining level),
/// the colossus' death bolt, the necropolis chests and what their `PopulateContents` makes
/// (tg `necropolis_chests.dm`).
pub const MEGAFAUNA_RUNTIME_TYPES: [&str; 37] = [
    "/mob/living/simple_animal/hostile/megafauna/bubblegum",
    "/obj/structure/closet/crate/necropolis/bubblegum",
    "/obj/item/clothing/suit/hooded/hostile_environment",
    "/obj/item/mayhem",
    "/obj/item/soulscythe",
    "/obj/item/food/bubblegum/bubblegum",
    "/mob/living/simple_animal/hostile/megafauna/colossus",
    "/mob/living/simple_animal/hostile/megafauna/dragon",
    "/obj/projectile/colossus",
    "/obj/structure/closet/crate/necropolis/colossus",
    "/obj/structure/closet/crate/necropolis/dragon",
    "/obj/item/organ/vocal_cords/colossus",
    "/obj/item/cain_and_abel",
    "/obj/machinery/anomalous_crystal/dark_reprise",
    "/obj/machinery/anomalous_crystal/emitter",
    "/obj/machinery/anomalous_crystal/helpers",
    "/obj/machinery/anomalous_crystal/honk",
    "/obj/machinery/anomalous_crystal/possessor",
    "/obj/machinery/anomalous_crystal/theme_warp",
    "/obj/item/melee/ghost_sword",
    "/obj/item/lava_staff",
    "/obj/item/book/granter/action/spell/sacredflame",
    "/obj/item/dragons_blood",
    // Legion: its splits, sentinels and drops; the icemoon clockwork defender and its alloy.
    "/mob/living/simple_animal/hostile/megafauna/legion",
    "/mob/living/simple_animal/hostile/megafauna/legion/medium/left",
    "/mob/living/simple_animal/hostile/megafauna/legion/medium/right",
    "/mob/living/simple_animal/hostile/megafauna/legion/medium/eye",
    "/mob/living/simple_animal/hostile/megafauna/legion/small",
    "/obj/structure/legionturret",
    "/obj/item/storm_staff",
    "/obj/structure/closet/crate/necropolis/tendril",
    "/obj/item/stack/sheet/bone",
    "/mob/living/simple_animal/hostile/megafauna/clockwork_defender",
    "/obj/item/clockwork_alloy",
    // The hierophant (admin spawns; its arena ruin is not mapped), its beacon and its club.
    "/mob/living/simple_animal/hostile/megafauna/hierophant",
    "/obj/effect/hierophant",
    "/obj/item/hierophant_club",
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

/// `SENTIENCE_ORGANIC`: what the lazarus injector revives.
pub const SENTIENCE_ORGANIC: u8 = 1;

fn organic() -> u8 {
    SENTIENCE_ORGANIC
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
    /// `sentience_type` (`SENTIENCE_ORGANIC` 1, `_ARTIFICIAL` 2, `_BOSS` 5, ...).
    #[serde(default = "organic")]
    pub sentience_type: u8,
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
    /// `BB_BASIC_MOB_SPEAK_LINES` of its controller's `blackboard` (what
    /// `random_speech_blackboard` says).
    #[serde(default)]
    pub speech_lines: Option<MobSpeech>,
    /// Its controller's `ai_traits` (DM bits: `STOP_MOVING_WHEN_PULLED` 1, ...).
    #[serde(default)]
    pub ai_traits: u32,
    /// `AddElement(/datum/element/can_be_held)` in its `Initialize` (or an ancestor's): picked
    /// up into a hand as a `/obj/item/mob_holder`.
    #[serde(default)]
    pub can_be_held: bool,
    /// The holder's in-hand state (`held_state`) and files (`held_lh`, `held_rh`, `head_icon`),
    /// and the slots it is worn in (`worn_slot_flags`).
    #[serde(default)]
    pub held_state: String,
    #[serde(default)]
    pub held_lh: String,
    #[serde(default)]
    pub held_rh: String,
    #[serde(default)]
    pub head_icon: String,
    #[serde(default)]
    pub worn_slot_flags: u32,
    /// `combat_mode` (hostile fauna start in it).
    #[serde(default)]
    pub combat_mode: bool,
    /// `status_flags` without `CANPUSH`: nobody pushes it out of the way (the alien queen,
    /// spiders, megafauna).
    #[serde(default)]
    pub unpushable: bool,
    /// `move_resist` when not `MOVE_RESIST_DEFAULT` (1000).
    #[serde(default)]
    pub move_resist: Option<f32>,
    /// `pass_flags` (`PASSTABLE`, `PASSMOB`, ...): what it walks through.
    #[serde(default)]
    pub pass_flags: u32,
    /// `bubble_icon`: the `talk.dmi` state prefix of its speech and typing bubbles (empty in
    /// data converted before it: `"default"`).
    #[serde(default)]
    pub bubble_icon: String,
}

/// What a mob says at random: a `/datum/bt_node/ai_behavior/random_speech` type's vars, or a
/// controller's `BB_BASIC_MOB_SPEAK_LINES` (`emote_say`, `emote_hear`, ...).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct MobSpeech {
    /// `speech_chance` / `emote_chance`, percent per perform (one a second).
    pub chance: f32,
    pub emote_hear: Vec<String>,
    pub emote_see: Vec<String>,
    pub speak: Vec<String>,
    /// Sound files (`sound/...ogg`) or SFX keys, played with `emote_hear` and `speak` lines.
    pub sound: Vec<String>,
}

/// `/datum/bt_node/ai_behavior/random_speech`.
pub const RANDOM_SPEECH_ROOT: &str = "/datum/bt_node/ai_behavior/random_speech";

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
    /// Every [`RANDOM_SPEECH_ROOT`] type (the root too) by path.
    #[serde(default)]
    pub speech: BTreeMap<String, MobSpeech>,
}

impl MobsData {
    pub fn read(path: &Path) -> anyhow::Result<Self> {
        crate::read_ron(path)
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
    if !is_basic(path) || !types.types.contains_key(path) {
        return Vec::new();
    }
    let ron = ron::to_string(path).expect("strings always serialize");
    vec![ext(COMPONENT, "path", ron)]
}

/// A type read as a basic mob: `/mob/living/basic` and the megafauna.
fn is_basic(path: &str) -> bool {
    is_under(path, BASIC_ROOT)
        || is_under(path, MEGAFAUNA_ROOT)
        || is_under(path, ALIEN_ROOT)
        || is_under(path, OOZE_ROOT)
        || is_under(path, ELITE_ROOT)
}

/// Every `/mob/living/basic` type and the megafauna.
pub fn resolve(types: &DmTypes, verbs: &RuVerbs, factions: &HashMap<String, String>) -> MobsData {
    let mobs = types
        .types
        .keys()
        .filter(|p| is_basic(p))
        .map(|p| (p.clone(), mob_def(types, p, verbs, factions)))
        .collect();
    let speech = types
        .types
        .keys()
        .filter(|p| is_under(p, RANDOM_SPEECH_ROOT))
        .map(|p| (p.clone(), speech_datum(types, p)))
        .collect();
    MobsData { mobs, speech }
}

/// The strings of a dumped list (sounds: `'file'` resources or SFX keys).
fn strings(value: Option<&Value>) -> Vec<String> {
    list(value)
        .into_iter()
        .filter_map(|(k, _)| {
            k.as_str()
                .or_else(|| k.get("resource").and_then(Value::as_str))
                .map(str::to_owned)
        })
        .collect()
}

/// The vars of a `random_speech` type.
fn speech_datum(types: &DmTypes, path: &str) -> MobSpeech {
    let v = |var: &str| types.resolved(path, var);
    MobSpeech {
        chance: v("speech_chance").and_then(Value::as_f64).unwrap_or(1.0) as f32,
        emote_hear: strings(v("emote_hear")),
        emote_see: strings(v("emote_see")),
        speak: strings(v("speak")),
        sound: strings(v("sound")),
    }
}

/// `BB_BASIC_MOB_SPEAK_LINES` in the `blackboard` of controller type `controller`
/// (`random_speech_blackboard` defaults the chance to 1).
fn speech_lines(types: &DmTypes, controller: &str) -> Option<MobSpeech> {
    if controller.is_empty() {
        return None;
    }
    let blackboard = types.resolved(controller, "blackboard");
    let (_, lines) = list(blackboard)
        .into_iter()
        .find(|(k, _)| k.as_str() == Some("BB_speech_lines"))?;
    let entries = list(lines);
    let get = |key: &str| {
        entries
            .iter()
            .find(|(k, _)| k.as_str() == Some(key))
            .and_then(|(_, v)| *v)
    };
    Some(MobSpeech {
        chance: get("emote_chance")
            .and_then(Value::as_f64)
            .filter(|c| *c != 0.0)
            .unwrap_or(1.0) as f32,
        emote_hear: strings(get("emote_hear")),
        emote_see: strings(get("emote_see")),
        speak: strings(get("emote_say")),
        sound: strings(get("emote_sound")),
    })
}

/// The element that makes basic mobs fly.
const SIMPLE_FLYING: &str = "/datum/element/simple_flying";

/// `simple_flying` added in the `Initialize` of `path` or an ancestor (calls in other procs come
/// and go with status effects and fish traits).
const CAN_BE_HELD: &str = "/datum/element/can_be_held";

fn flies(types: &DmTypes, path: &str) -> bool {
    adds_element(types, path, SIMPLE_FLYING)
}

/// An `AddElement(element)` in the `Initialize` of `path` or an ancestor.
fn adds_element(types: &DmTypes, path: &str, element: &str) -> bool {
    types
        .element_calls
        .get(element)
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
    // `/mob/living/simple_animal` names for the same things.
    let simple =
        is_under(path, MEGAFAUNA_ROOT) || is_under(path, OOZE_ROOT) || is_under(path, ELITE_ROOT);
    let basic_mob_flags = if simple {
        // `del_on_death` -> `DEL_ON_DEATH`.
        u32::from(num("del_on_death", 0.0) != 0.0)
    } else {
        num("basic_mob_flags", 0.0) as u32
    };
    let (min_temp, max_temp) = if simple {
        ("minbodytemp", "maxbodytemp")
    } else {
        (
            "minimum_survivable_temperature",
            "maximum_survivable_temperature",
        )
    };
    let alien = is_under(path, ALIEN_ROOT);
    if alien {
        // `/mob/living/carbon/alien`'s `get_tox_loss`, `adjust_stamina_loss`.
        physiology[2] = 0.0;
        physiology[4] = 0.0;
    }
    let ai_controller = dm_path(v("ai_controller")).unwrap_or_default();
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
        basic_mob_flags,
        habitable_atmos,
        unsuitable_atmos_damage: num("unsuitable_atmos_damage", 1.0),
        // `alien/handle_environment`: only heat over `BODYTEMP_HEAT_DAMAGE_LIMIT` hurts.
        minimum_survivable_temperature: if alien { 0.0 } else { num(min_temp, 250.0) },
        maximum_survivable_temperature: if alien {
            ALIEN_MAX_TEMP
        } else {
            num(max_temp, 350.0)
        },
        unsuitable_cold_damage: num("unsuitable_cold_damage", 1.0),
        // `HEAT_DAMAGE_LEVEL_2`.
        unsuitable_heat_damage: if alien {
            3.0
        } else {
            num("unsuitable_heat_damage", 1.0)
        },
        icon: resource("icon"),
        icon_living: if alien {
            s("icon_state")
        } else {
            s("icon_living")
        },
        icon_dead: if alien {
            format!("{}_dead", s("icon_state"))
        } else {
            s("icon_dead")
        },
        density: v("density")
            .and_then(Value::as_f64)
            .is_none_or(|d| d != 0.0),
        mob_size: num("mob_size", 2.0) as u8,
        sentience_type: num("sentience_type", f32::from(SENTIENCE_ORGANIC)) as u8,
        faction,
        speech_lines: speech_lines(types, &ai_controller),
        ai_traits: (!ai_controller.is_empty())
            .then(|| types.resolved(&ai_controller, "ai_traits"))
            .flatten()
            .and_then(Value::as_f64)
            .unwrap_or(0.0) as u32,
        ai_controller,
        speak_emote,
        body_icon_state: s("body_icon_state"),
        body_color: s("body_color"),
        possible_body_colors,
        cowardly: v("cowardly")
            .and_then(Value::as_f64)
            .is_some_and(|c| c != 0.0),
        flying: flies(types, path),
        can_be_held: adds_element(types, path, CAN_BE_HELD),
        held_state: s("held_state"),
        held_lh: resource("held_lh"),
        held_rh: resource("held_rh"),
        head_icon: resource("head_icon"),
        worn_slot_flags: num("worn_slot_flags", 0.0) as u32,
        cable_zap_prob: num("cable_zap_prob", 0.0),
        bot: is_under(path, BOT_ROOT).then(|| bot_def(types, path)),
        combat_mode: num("combat_mode", 0.0) != 0.0,
        unpushable: num("status_flags", CANPUSH as f32) as u32 & CANPUSH == 0,
        move_resist: Some(num("move_resist", MOVE_RESIST_DEFAULT))
            .filter(|&r| r != MOVE_RESIST_DEFAULT),
        pass_flags: num("pass_flags", 0.0) as u32,
        bubble_icon: s("bubble_icon"),
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
            "/mob/living": { "parent": "/mob", "declared_vars": { "maxHealth": 100, "health": 100, "bubble_icon": "default" } },
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
            "/mob/living/basic/carp/lia": { "parent": "/mob/living/basic/carp", "declared_vars": { "name": "\\improper Lia" } },
            "/mob/living/simple_animal": { "parent": "/mob/living", "declared_vars": {
                "minbodytemp": 250, "maxbodytemp": 350, "del_on_death": 0, "speed": 1 } },
            "/mob/living/simple_animal/hostile": { "parent": "/mob/living/simple_animal" },
            "/mob/living/simple_animal/hostile/megafauna": { "parent": "/mob/living/simple_animal/hostile",
                "declared_vars": { "minbodytemp": 0, "maxbodytemp": 1e31, "maxHealth": 1000,
                "physiology": { "list": [ { "assoc": ["burn", 0.5] }, { "assoc": ["stamina", 0] } ] } } },
            "/mob/living/simple_animal/hostile/megafauna/colossus": {
                "parent": "/mob/living/simple_animal/hostile/megafauna", "declared_vars": {
                "name": "colossus", "maxHealth": 2500, "health": 2500, "del_on_death": 1, "speed": 10 } },
            "/mob/living/carbon": { "parent": "/mob/living" },
            "/mob/living/carbon/alien": { "parent": "/mob/living/carbon",
                "declared_vars": { "name": "alien", "bubble_icon": "alien", "icon": { "resource": "icons/mob/nonhuman-player/alien.dmi" } } },
            "/mob/living/carbon/alien/larva": { "parent": "/mob/living/carbon/alien",
                "declared_vars": { "name": "alien larva", "icon_state": "larva0", "maxHealth": 25, "health": 25 } },
            "/datum/ai_controller/basic_controller/carp": { "parent": null, "declared_vars": {
                "blackboard": { "list": [ { "assoc": ["targeting_strategy", { "path": "/datum/targeting_strategy/basic" }] },
                    { "assoc": ["BB_speech_lines", { "list": [
                        { "assoc": ["emote_say", { "list": ["moo"] }] },
                        { "assoc": ["emote_sound", { "list": [{ "resource": "sound/cow.ogg" }] }] },
                        { "assoc": ["emote_chance", 3] } ] }] } ] }, "ai_traits": 21 } },
            "/datum/bt_node/ai_behavior/random_speech": { "parent": null, "declared_vars": { "speech_chance": 1 } },
            "/datum/bt_node/ai_behavior/random_speech/cats": { "parent": "/datum/bt_node/ai_behavior/random_speech",
                "declared_vars": { "emote_hear": { "list": ["meows."] }, "sound": { "list": ["cat_meow"] }, "speech_chance": 10 } },
            "/datum/bt_node/ai_behavior/random_speech_blackboard": { "parent": null }
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
        assert_eq!(carp.bubble_icon, "default", "inherited from /mob/living");
        assert_eq!(
            data.mobs["/mob/living/carbon/alien/larva"].bubble_icon,
            "alien"
        );
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
    fn speech_from_speech_types_and_controller_blackboards() {
        let data = data();
        let carp = &data.mobs["/mob/living/basic/carp"];
        let lines = carp.speech_lines.as_ref().unwrap();
        assert_eq!(lines.speak, ["moo"]);
        assert_eq!(lines.sound, ["sound/cow.ogg"]);
        assert_eq!(lines.chance, 3.0);
        assert_eq!(carp.ai_traits, 21);
        assert!(lines.emote_hear.is_empty());
        assert_eq!(data.mobs["/mob/living/basic"].speech_lines, None);
        let cats = &data.speech["/datum/bt_node/ai_behavior/random_speech/cats"];
        assert_eq!(cats.chance, 10.0);
        assert_eq!(cats.emote_hear, ["meows."]);
        assert_eq!(cats.sound, ["cat_meow"]);
        assert_eq!(
            data.speech["/datum/bt_node/ai_behavior/random_speech"].chance,
            1.0
        );
        assert!(
            !data
                .speech
                .contains_key("/datum/bt_node/ai_behavior/random_speech_blackboard")
        );
    }

    #[test]
    fn megafauna_read_as_basic_mobs() {
        let data = data();
        let colossus = &data.mobs["/mob/living/simple_animal/hostile/megafauna/colossus"];
        assert_eq!(colossus.max_health, 2500.0);
        assert_eq!(colossus.speed, 10.0);
        assert_eq!(colossus.basic_mob_flags, 1, "del_on_death");
        assert_eq!(colossus.physiology, [1.0, 0.5, 1.0, 1.0, 0.0]);
        assert_eq!(colossus.minimum_survivable_temperature, 0.0);
        assert!(colossus.maximum_survivable_temperature >= 1e30);
        assert!(!data.mobs.contains_key("/mob/living/simple_animal/hostile"));
        let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
        assert_eq!(
            type_fields(
                &types,
                "/mob/living/simple_animal/hostile/megafauna/colossus"
            )
            .len(),
            1
        );
    }

    #[test]
    fn aliens_read_as_basic_mobs() {
        let data = data();
        let larva = &data.mobs["/mob/living/carbon/alien/larva"];
        assert_eq!(larva.max_health, 25.0);
        assert_eq!(larva.icon_living, "larva0");
        assert_eq!(larva.icon_dead, "larva0_dead");
        assert_eq!(larva.physiology, [1.0, 1.0, 0.0, 1.0, 0.0]);
        assert_eq!(larva.maximum_survivable_temperature, ALIEN_MAX_TEMP);
        let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
        assert_eq!(
            type_fields(&types, "/mob/living/carbon/alien/larva").len(),
            1
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
