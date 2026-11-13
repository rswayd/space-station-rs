//! Quirks (server), after tg `code/datums/quirks` (`/datum/quirk`), `SSquirks`
//! (`code/controllers/subsystem/processing/quirks.dm`) and the preferences menu's quirk
//! middleware (`preferences/middleware/quirks.dm`).
//!
//! - The character's quirks are picked by name in the preferences window ([`QuirkPrefs`] on the
//!   client, `give_quirk`/`remove_quirk` acts checked by [`filter_invalid`]: blacklisted pairs,
//!   at most [`MAX_POSITIVE_QUIRKS`] positive ones and a non-negative point balance from
//!   [`DEFAULT_QUIRK_POINTS`]).
//! - A human joining the round gets them (`AssignQuirks` -> `add_quirk`, [`assign`]): a
//!   [`Quirks`] component, the quirk's own effect and, once, its items (`item_quirk`).
//! - Processing quirks act every `SSquirks` fire (1 s) while the holder is below hard crit.
//!
//! Ported quirks: Bilingual, Foreigner and Common Second Language (`ss13_chat::language`),
//! Alcohol Tolerance and Light Drinker (booze power, `ss13_reagents`), Drunken Resilience,
//! Friendly (`crate::mood::on_hug`), Jolly, Voracious (nutrition moodlets), Monochromacy, Blind
//! and Nearsighted (`crate::vision`), Depression, Hypersensitive, Unstable (`crate::mood`), Mute,
//! Soft-Spoken and Pacifist (`crate::medical::traumas::refresh`), Brain Tumor, Bad Back,
//! Paraplegic and Hemiplegic (absolute paralysis traumas), Freerunning (`crate::tables` climbing,
//! `crate::multiz` short falls), Light Step (`crate::footsteps`), Deaf (`crate::sound`), Blood
//! Deficiency (`crate::medical::blood`), Illiterate (papers), Tin Man (surplus cybernetic
//! organs), Body Purist (a moodlet per cybernetic part), Smooth-Headed (bald, a moodlet when the
//! head is uncovered), and the tastes of `crate::food`: Ageusia, Vegetarian, Deviant Tastes,
//! Ananas Affinity and Aversion; Junkie, Smoker and Alcoholic (`crate::addictions`), Extreme
//! Medicine Allergy (`crate::chem` metabolism), Food Allergy (`crate::food`), Social Anxiety
//! (`ss13_chat::speech::Anxiety`, eye contact on examine), Clown Enjoyer, Mime Fan, Musician,
//! Tagger (`crate::crayons`, `crate::syndie_spraycan`), Pyromania (`crate::breath`), Chip
//! Connector and Chipped (`crate::skillchips`).
// shortcut: the other tg quirks wait for their mechanics (phobias, hallucinations, wheelchairs...); no medical record lines, no
// customization options (blindfold colour, glasses type, paraplegic variant, hemiplegic side,
// CSL strength), no hardcore or random quirks, no mail goodies, no quirk transfer between
// bodies; the point settings are tg's config defaults, not read from `config/game_options.txt`.
// Removing Paraplegic or Hemiplegic keeps its trauma. CSL does not fall back to the native
// language at low sanity (`translate_parts`); Light Step leaves bloody soles and glass shards
// alone (not ported); Smooth-Headed gives no wig; Deviant Tastes swaps the tastes of every
// tongue, not the one it found; Ageusia still tastes handmade food's complexity.

use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_net::server::Networked;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_shared::protocol::InventorySlot;

use crate::combat::CombatRng;
use crate::health::{DamageType, Health, Stat, update_stat};
use crate::inventory::{CarriedBy, Inventory};
use crate::medical::organs::{self, Slot};
use crate::medical::traumas::{self, Kind, Paralysis, Resilience};
use crate::mood::{self, ActiveMood, Mood, MoodEvent, events};
use crate::outfit::OutfitTemplates;
use crate::server::PlayerMob;
use crate::species::Trait;

mod afflictions;
mod defs;
mod effects;
mod lifecycle;
#[cfg(test)]
mod tests;

use afflictions::*;
pub use afflictions::{
    allergic_reagents, allergy_string, food_allergy_reaction, pick_medicine_allergies,
};
pub use defs::{BLACKLIST, QUIRKS};
use effects::*;
pub use effects::{
    anxiety_mod, anxiety_mood_mod, booze_multiplier, count_nearby_fire_sources, examined,
    food_tastes, heat_multiplier, reremove_common, tagger_multiplier,
};
use lifecycle::*;
pub use lifecycle::{add_quirk, assign, change_prefs, filter_invalid, has, remove_quirk};

/// `default_quirk_points` (config default).
pub const DEFAULT_QUIRK_POINTS: i32 = 2;
/// `max_positive_quirks` (BandaStation `config/game_options.txt`).
pub const MAX_POSITIVE_QUIRKS: usize = 6;
/// `SSquirks` `wait`, seconds.
const QUIRKS_WAIT_SECS: f64 = 1.0;
/// Jolly's and Depression's `SPT_PROB(0.416, seconds_per_tick)`.
const MOOD_SWING_PERCENT: f32 = 0.416;
/// Brain Tumor's `degradation_speed`, brain damage per second.
const TUMOR_DAMAGE_PER_SECOND: f32 = 0.2;
const BACKPACK: &str = "/obj/item/storage/backpack";
const BLINDFOLD: &str = "/obj/item/clothing/glasses/blindfold/white";
const GLASSES: &str = "/obj/item/clothing/glasses/regular";
const MANNITOL_BOTTLE: &str = "/obj/item/storage/pill_bottle/mannitol/braintumor";
const DEAF_PIN: &str = "/obj/item/clothing/accessory/deaf_pin";
const CANE: &str = "/obj/item/cane";
/// `MEAT`, `PINEAPPLE` (`crate::food` foodtype bits).
const MEAT: u32 = 1 << 0;
const PINEAPPLE: u32 = 1 << 12;
/// Tin Man's `organ_slots`: the surplus prosthetic and the species trait that skips it.
const TIN_MAN_ORGANS: [(Slot, &str, Trait); 4] = [
    (
        Slot::Heart,
        "/obj/item/organ/heart/cybernetic/surplus",
        Trait::NoBlood,
    ),
    (
        Slot::Lungs,
        "/obj/item/organ/lungs/cybernetic/surplus",
        Trait::NoBreath,
    ),
    (
        Slot::Liver,
        "/obj/item/organ/liver/cybernetic/surplus",
        Trait::LiverlessMetabolism,
    ),
    (
        Slot::Stomach,
        "/obj/item/organ/stomach/cybernetic/surplus",
        Trait::NoHunger,
    ),
];
/// `/datum/mood_event/body_purist` (its `mood_change` is set per cybernetic part) and
/// `/datum/mood_event/bald`.
const BODY_PURIST_EVENT: MoodEvent = MoodEvent {
    id: "body_purist",
    description: "Я чувствую кибернетику в своем теле, и Я ЭТО НЕНАВИЖУ!",
    mood_change: 0.0,
    timeout: 0.0,
    hidden: false,
};
const BALD_EVENT: MoodEvent = MoodEvent {
    id: "bald",
    description: "Мне бы прикрыть чем-нибудь мою голову...",
    mood_change: -3.0,
    timeout: 0.0,
    hidden: false,
};
/// `/datum/quirk/item_quirk/addict`: `process_interval`, seconds.
const ADDICT_PROCESS_SECS: f64 = 30.0;
const PILL_BOTTLE: &str = "/obj/item/storage/pill_bottle";
const PILL: &str = "/obj/item/reagent_containers/applicator/pill";
/// The Junkie's pills: 7 of 3 units.
const JUNKIE_PILLS: usize = 7;
const JUNKIE_PILL_UNITS: f32 = 3.0;
const LIGHTER: &str = "/obj/item/lighter/greyscale";
const NICOTINE: &str = "/datum/reagent/drug/nicotine";
const ETHANOL: &str = "/datum/reagent/consumable/ethanol";
/// `GLOB.possible_junkie_addictions` (with BandaStation's cocaine).
const JUNKIE_DRUGS: [&str; 7] = [
    "/datum/reagent/drug/blastoff",
    "/datum/reagent/drug/krokodil",
    "/datum/reagent/medicine/morphine",
    "/datum/reagent/drug/happiness",
    "/datum/reagent/drug/methamphetamine",
    "/datum/reagent/drug/cocaine/freebase_cocaine",
    "/datum/reagent/drug/cocaine",
];
/// `GLOB.possible_smoker_addictions`: the pack and its `spawn_type`.
const SMOKER_PACKS: [(&str, &str); 9] = [
    (
        "/obj/item/storage/fancy/cigarettes",
        "/obj/item/cigarette/space_cigarette",
    ),
    (
        "/obj/item/storage/fancy/cigarettes/cigpack_midori",
        "/obj/item/cigarette/rollie/nicotine",
    ),
    (
        "/obj/item/storage/fancy/cigarettes/cigpack_uplift",
        "/obj/item/cigarette/uplift",
    ),
    (
        "/obj/item/storage/fancy/cigarettes/cigpack_robust",
        "/obj/item/cigarette/robust",
    ),
    (
        "/obj/item/storage/fancy/cigarettes/cigpack_robustgold",
        "/obj/item/cigarette/robustgold",
    ),
    (
        "/obj/item/storage/fancy/cigarettes/cigpack_carp",
        "/obj/item/cigarette/carp",
    ),
    (
        "/obj/item/storage/fancy/cigarettes/cigars",
        "/obj/item/cigarette/cigar/premium",
    ),
    (
        "/obj/item/storage/fancy/cigarettes/cigars/cohiba",
        "/obj/item/cigarette/cigar/cohiba",
    ),
    (
        "/obj/item/storage/fancy/cigarettes/cigars/havana",
        "/obj/item/cigarette/cigar/havana",
    ),
];
/// `GLOB.possible_alcoholic_addictions`: the bottle and the favourite reagent.
const ALCOHOLIC_BOTTLES: [(&str, &str); 26] = [
    ("curacao", "curacao"),
    ("applejack", "applejack"),
    ("wine_voltaic", "wine_voltaic"),
    ("tequila", "tequila"),
    ("rum", "rum"),
    ("cognac", "cognac"),
    ("wine", "wine"),
    ("absinthe", "absinthe"),
    ("vermouth", "vermouth"),
    ("gin", "gin"),
    ("hcider", "hcider"),
    ("amaretto", "amaretto"),
    ("ale", "ale"),
    ("grappa", "grappa"),
    ("navy_rum", "navy_rum"),
    ("maltliquor", "beer/maltliquor"),
    ("kahlua", "kahlua"),
    ("sake", "sake"),
    ("beer", "beer"),
    ("vodka", "vodka"),
    ("whiskey", "whiskey"),
    ("coconut_rum", "coconut_rum"),
    ("yuyake", "yuyake"),
    ("shochu", "shochu"),
    ("aperitivo", "aperitivo"),
    ("maraschino", "maraschino"),
];
const BOTTLE_ROOT: &str = "/obj/item/reagent_containers/cup/glass/bottle/";
/// `/datum/mood_event/wrong_brand` and `wrong_brandy`.
const WRONG_BRAND_EVENT: MoodEvent = MoodEvent {
    id: "wrong_brand",
    description: "Я ненавижу эту марку сигарет.",
    mood_change: -2.0,
    timeout: 360.0,
    hidden: false,
};
const WRONG_BRANDY_EVENT: MoodEvent = MoodEvent {
    id: "wrong_brandy",
    description: "Я ненавижу такие напитки.",
    mood_change: -2.0,
    timeout: 360.0,
    hidden: false,
};

const ALLERGY_DOGTAG: &str = "/obj/item/clothing/accessory/dogtag/allergy";
const HISTAMINE: &str = "/datum/reagent/toxin/histamine";
const EPINEPHRINE: &str = "/datum/reagent/medicine/epinephrine";
/// Extreme Medicine Allergy: 6 medicines (`for(var/i in 0 to 5)`).
const MEDICINE_ALLERGIES: usize = 6;
/// `GLOB.allergy_reagent_blacklist` (a typecache: subtypes too).
const ALLERGY_BLACKLIST: [&str; 21] = [
    "/datum/reagent/medicine/adminordrazine",
    "/datum/reagent/medicine/albuterol",
    "/datum/reagent/medicine/changelingadrenaline",
    "/datum/reagent/medicine/changelinghaste",
    "/datum/reagent/medicine/coagulant/banana_peel",
    "/datum/reagent/medicine/coagulant/seraka_extract",
    "/datum/reagent/medicine/cordiolis_hepatico",
    "/datum/reagent/medicine/diphenhydramine",
    "/datum/reagent/medicine/epinephrine",
    "/datum/reagent/medicine/insulin",
    "/datum/reagent/medicine/muscle_stimulant",
    "/datum/reagent/medicine/oculine/flumpuline",
    "/datum/reagent/medicine/omnizine/godblood",
    "/datum/reagent/medicine/omnizine/protozine",
    "/datum/reagent/medicine/regen_jelly",
    "/datum/reagent/medicine/salglu_solution",
    "/datum/reagent/medicine/sansufentanyl",
    "/datum/reagent/medicine/strange_reagent",
    "/datum/reagent/medicine/synaphydramine",
    "/datum/reagent/medicine/synaptizine/synaptizinevirusfood",
    "/datum/reagent/medicine/syndicate_nanites",
];
/// `GLOB.possible_food_allergies` (foodtype bits, BandaStation names).
pub const FOOD_ALLERGIES: [(&str, u32); 10] = [
    ("алкоголь", 1 << 8),
    ("жуки", 1 << 18),
    ("молочные продукты", 1 << 6),
    ("фрукты", 1 << 5),
    ("зерна", 1 << 4),
    ("мясо", 1 << 0),
    ("орехи", 1 << 15),
    ("морепродукты", 1 << 16),
    ("сахар", 1 << 9),
    ("овощи", 1 << 1),
];
/// `/datum/mood_event/allergic_food`.
const ALLERGIC_FOOD_EVENT: MoodEvent = MoodEvent {
    id: "allergic_food",
    description: "Моё горло зудит.",
    mood_change: -2.0,
    timeout: 240.0,
    hidden: false,
};

/// Social Anxiety: `oview(3)` for the people around, `EYE_CONTACT_RANGE`,
/// `RECENT_EXAMINE_MAX_WINDOW` (seconds).
const ANXIETY_RANGE: u32 = 3;
const EYE_CONTACT_RANGE: u32 = 5;
const RECENT_EXAMINE_SECS: f64 = 2.0;
const SPAGHETTI: &str = "/obj/item/food/spaghetti/pastatomato";
/// `/datum/mood_event/anxiety_eyecontact`.
const ANXIETY_EYECONTACT_EVENT: MoodEvent = MoodEvent {
    id: "anxiety_eyecontact",
    description: "Иногда взгляд в глаза заставляет меня нервничать...",
    mood_change: -5.0,
    timeout: 180.0,
    hidden: false,
};

const CLOWN_PIN: &str = "/obj/item/clothing/accessory/clown_enjoyer_pin";
const MIME_PIN: &str = "/obj/item/clothing/accessory/mime_fan_pin";
const MUSIC_BEACON: &str = "/obj/item/choice_beacon/music";
const SPRAYCAN: &str = "/obj/item/toy/crayon/spraycan";
/// `/datum/mood_event/clown_enjoyer_pin`, `mime_fan_pin`, `pyromania` (its change is the
/// fire count).
const CLOWN_PIN_EVENT: MoodEvent = MoodEvent {
    id: "clown_enjoyer_pin",
    description: "Мне нравится показывать свою клоунскую булавку!",
    mood_change: 1.0,
    timeout: 0.0,
    hidden: false,
};
const MIME_PIN_EVENT: MoodEvent = MoodEvent {
    id: "mime_fan_pin",
    description: "Мне нравится показывать свою мимскую булавку!",
    mood_change: 1.0,
    timeout: 0.0,
    hidden: false,
};
const PYROMANIA_EVENT: MoodEvent = MoodEvent {
    id: "pyromania",
    description: "Прекрасный, тёплый огонь...",
    mood_change: 1.0,
    timeout: 0.0,
    hidden: false,
};
/// Pyromania's `damage_mod` (`PHYS_COEFF_HEAT`) and `count_nearby_fire_sources` radius.
const PYROMANIA_HEAT_MOD: f64 = 0.67;
const PYROMANIA_RADIUS: u32 = 5;

/// Items the quirks hand out: `crate::jobs` builds their templates with the outfits'.
pub fn item_paths() -> Vec<String> {
    let mut paths: Vec<String> = [
        BLINDFOLD,
        GLASSES,
        MANNITOL_BOTTLE,
        DEAF_PIN,
        PILL_BOTTLE,
        PILL,
        LIGHTER,
        ALLERGY_DOGTAG,
        CLOWN_PIN,
        MIME_PIN,
        MUSIC_BEACON,
        SPRAYCAN,
    ]
    .iter()
    .map(|p| (*p).to_owned())
    .collect();
    paths.extend(SMOKER_PACKS.iter().map(|(pack, _)| (*pack).to_owned()));
    paths.extend(
        ALCOHOLIC_BOTTLES
            .iter()
            .map(|(bottle, _)| format!("{BOTTLE_ROOT}{bottle}")),
    );
    paths
}

/// The ported tg quirk types.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Quirk {
    AlcoholTolerance,
    Bilingual,
    /// A CNS skillchip connector implant (`crate::skillchips::connector_click`).
    ChipConnector,
    /// A random commercial skillchip, implanted and active (`crate::skillchips`).
    Chipped,
    DrunkHealing,
    Friendly,
    Jolly,
    Voracious,
    BadBack,
    Blind,
    BrainTumor,
    Depression,
    Hypersensitive,
    LightDrinker,
    Mute,
    Nearsighted,
    Pacifist,
    /// Stormtrooper Aim: every shot spreads 10 to 35 degrees more (`crate::guns`).
    PoorAim,
    /// `see_no_names/allow_ids`: other humans are named by their ID or not at all
    /// (`crate::clothing::cover::unrecognized_name`).
    Prosopagnosia,
    /// `TRAIT_CLUMSY` (`crate::genetics::is_clumsy`).
    Clumsy,
    /// `TRAIT_EASILY_WOUNDED` (`crate::medical::wounds`).
    Frail,
    /// A brute hit to the head may knock out (`crate::medical::wounds::after_hit`).
    GlassJaw,
    /// `TRAIT_GRABWEAKNESS`: a grab counts one state higher when resisted (`crate::pull`).
    Pushover,
    /// `/datum/status_effect/limp/quirk`: one leg limps (`crate::medical`).
    Limper,
    Paraplegic,
    Unstable,
    Monochromacy,
    Freerunning,
    LightStep,
    BloodDeficiency,
    BodyPurist,
    Csl,
    Deaf,
    Hemiplegic,
    Illiterate,
    SoftSpoken,
    TinMan,
    Ageusia,
    PineappleLiker,
    PineappleHater,
    DeviantTastes,
    Foreigner,
    Bald,
    Vegetarian,
    Junkie,
    Smoker,
    Alcoholic,
    MedicineAllergy,
    FoodAllergy,
    SocialAnxiety,
    ClownEnjoyer,
    MimeFan,
    Musician,
    Tagger,
    Pyromania,
}

/// The static part of a quirk (tg vars).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct QuirkDef {
    pub quirk: Quirk,
    pub path: &'static str,
    pub name: &'static str,
    pub desc: &'static str,
    /// A FontAwesome icon (`FA_ICON_*`).
    pub icon: &'static str,
    /// Points: positive quirks cost them, negative ones give them.
    pub value: i32,
    pub gain: Option<(Span, &'static str)>,
    pub lose: Option<(Span, &'static str)>,
}

impl Quirk {
    pub fn def(self) -> &'static QuirkDef {
        QUIRKS
            .iter()
            .find(|d| d.quirk == self)
            .expect("every quirk has a definition")
    }

    /// `SSquirks.quirks[name]`.
    pub fn by_name(name: &str) -> Option<Self> {
        QUIRKS.iter().find(|d| d.name == name).map(|d| d.quirk)
    }
}

/// The quirks of a mob (tg `quirks`).
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct Quirks(pub Vec<Quirk>);

impl Quirks {
    pub fn has(&self, quirk: Quirk) -> bool {
        self.0.contains(&quirk)
    }
}

/// Chip Connector's implant and Chipped's chip (`connector`, `installed_chip`), despawned when
/// the quirk goes.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct QuirkChips {
    pub connector: Option<Entity>,
    pub chip: Option<Entity>,
}

/// Smooth-Headed's `old_hair`, and whether the head was covered at the last check.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct BaldHead {
    pub old_hair: Option<String>,
    pub covered: bool,
}

/// Body Purist's `cybernetics_level` at the last check.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cybernetics(pub u32);

/// One addict quirk's state: `reagent_type`, `drug_container_type`, the Smoker's pack
/// cigarette or the Alcoholic's `favorite_alcohol`, and the `next_process` cooldown.
#[derive(Clone, Debug, PartialEq)]
pub struct AddictState {
    pub quirk: Quirk,
    pub reagent: &'static str,
    pub container: String,
    pub favorite: Option<String>,
    pub next_process: f64,
}

/// The mob's addict quirks (Junkie, Smoker, Alcoholic can all be taken).
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct AddictQuirks(pub Vec<AddictState>);

/// Extreme Medicine Allergy's `allergies` (reagent paths) and Food Allergy's
/// `target_foodtypes`.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct Allergies {
    pub medicines: Vec<String>,
    pub foods: u32,
}

/// Pyromania's `last_fire_size`.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct LastFireSize(pub i32);

/// A client's `recent_examines`: what its mob examined and when (game seconds).
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct RecentExamines(pub Vec<(Entity, f64)>);

/// On a client connection: the character's quirks by name (tg `prefs.all_quirks`).
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct QuirkPrefs(pub Vec<String>);

impl QuirkPrefs {
    /// tg's menu balance: the free points left after the chosen quirks.
    pub fn balance(&self) -> i32 {
        DEFAULT_QUIRK_POINTS
            - self
                .0
                .iter()
                .filter_map(|n| Quirk::by_name(n))
                .map(|q| q.def().value)
                .sum::<i32>()
    }
}

pub struct QuirksServerPlugin;

impl Plugin for QuirksServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            quirk_life
                .before(crate::health::HealthSystems)
                .run_if(any_with_component::<Quirks>),
        );
        app.add_systems(
            Update,
            anxious_speech
                .after(ss13_chat::server::SaySystems)
                .run_if(any_with_component::<ss13_chat::speech::Anxiety>),
        );
        // `crate::addictions` has no plugin of its own: the addict quirks are its only source.
        app.add_systems(
            Update,
            crate::addictions::addiction_life
                .before(crate::health::HealthSystems)
                .run_if(any_with_component::<crate::addictions::Addictions>),
        );
    }
}

/// `SSquirks`: the processing quirks of every holder, every second.
// Cost: 1 Hz, one pass over mobs with quirks (a few players), a few lookups each.
fn quirk_life(world: &mut World, mut clock: Local<f64>) {
    *clock += world.resource::<Time>().delta_secs_f64();
    let mut fires = 0;
    while *clock >= QUIRKS_WAIT_SECS {
        *clock -= QUIRKS_WAIT_SECS;
        fires += 1;
    }
    if fires == 0 {
        return;
    }
    let mut query = world.query::<(Entity, &Quirks)>();
    let holders: Vec<(Entity, Vec<Quirk>)> =
        query.iter(world).map(|(e, q)| (e, q.0.clone())).collect();
    let secs = QUIRKS_WAIT_SECS as f32;
    for (mob, quirks) in holders {
        for _ in 0..fires {
            for &quirk in &quirks {
                process(world, mob, quirk, secs);
            }
        }
    }
}

/// One quirk's `process` (and Bad Back's equip signals, followed here every fire).
fn process(world: &mut World, mob: Entity, quirk: Quirk, secs: f32) {
    let Some(&stat) = world.get::<Stat>(mob) else {
        return;
    };
    // Signal-driven in tg (organ, limb and equipment changes): followed here every fire.
    match quirk {
        Quirk::BodyPurist => {
            update_cybernetics(world, mob);
            return;
        }
        Quirk::Bald => {
            bald_head(world, mob);
            return;
        }
        // The pins' `accessory_equipped`/`accessory_dropped`, followed every fire.
        Quirk::ClownEnjoyer | Quirk::MimeFan => {
            fan_pin(world, mob, quirk);
            return;
        }
        // `calculate_mood_mod` at the time of speech in tg; refreshed every fire here.
        Quirk::SocialAnxiety => {
            let mood_mod = anxiety_mood_mod(world, mob);
            if let Some(mut anxiety) = world.get_mut::<ss13_chat::speech::Anxiety>(mob)
                && anxiety.mood_mod != mood_mod
            {
                anxiety.mood_mod = mood_mod;
            }
            return;
        }
        _ => {}
    }
    if quirk == Quirk::BadBack {
        let backpack = world
            .get::<Inventory>(mob)
            .and_then(|i| i.back)
            .and_then(|b| world.get::<DmItem>(b))
            .is_some_and(|dm| ss13_mapload::fidelity::is_under(&dm.path, BACKPACK));
        let has_pain = world
            .get::<Mood>(mob)
            .is_some_and(|m| m.get("back_pain").is_some());
        if backpack && !has_pain {
            mood::add_event(world, mob, "back_pain", &events::BACK_PAIN);
        } else if !backpack && has_pain {
            mood::clear_event(world, mob, "back_pain");
        }
        return;
    }
    // `maximum_process_stat = HARD_CRIT`.
    if stat >= Stat::HardCrit {
        return;
    }
    match quirk {
        Quirk::Jolly => {
            if spt_prob(world, MOOD_SWING_PERCENT, secs) {
                mood::add_event(world, mob, "jolly", &events::JOLLY);
            }
        }
        Quirk::Depression => {
            if spt_prob(world, MOOD_SWING_PERCENT, secs) {
                mood::add_event(world, mob, "depression", &events::DEPRESSION);
            }
        }
        Quirk::DrunkHealing => {
            let drunk = world
                .get::<crate::chem::MobReagents>(mob)
                .map_or(0.0, |r| r.drunk);
            let (brute, burn) = if drunk >= 61.0 {
                (0.8, 0.4)
            } else if drunk >= 41.0 {
                (0.4, 0.2)
            } else if drunk >= 6.0 {
                (0.1, 0.05)
            } else {
                return;
            };
            let Some(mut health) = world.get::<Health>(mob).cloned() else {
                return;
            };
            if health.brute <= 0.0 && health.burn <= 0.0 {
                return;
            }
            health.adjust(DamageType::Brute, -brute * secs);
            health.adjust(DamageType::Burn, -burn * secs);
            let mut new_stat = stat;
            update_stat(&health, &mut new_stat);
            let mut e = world.entity_mut(mob);
            e.insert(health);
            if new_stat != stat {
                e.insert(new_stat);
            }
        }
        Quirk::BrainTumor => {
            organs::adjust_organ_loss(world, mob, Slot::Brain, TUMOR_DAMAGE_PER_SECOND * secs);
        }
        Quirk::Junkie | Quirk::Smoker | Quirk::Alcoholic => addict_process(world, mob, quirk),
        Quirk::Pyromania => pyromania(world, mob),
        _ => {}
    }
}

fn spt_prob(world: &mut World, percent: f32, secs: f32) -> bool {
    let mut rng = world.get_resource_or_init::<CombatRng>();
    crate::medical::wounds::spt_prob(&mut rng, percent, secs)
}
