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
//! Medicine Allergy (`crate::chem` metabolism) and Food Allergy (`crate::food`).
// shortcut: the other tg quirks wait for their mechanics (phobias, skillchips, hallucinations, wheelchairs...); no medical record lines, no
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

const fn q(
    quirk: Quirk,
    path: &'static str,
    name: &'static str,
    desc: &'static str,
    icon: &'static str,
    value: i32,
    gain: Option<(Span, &'static str)>,
    lose: Option<(Span, &'static str)>,
) -> QuirkDef {
    QuirkDef {
        quirk,
        path,
        name,
        desc,
        icon,
        value,
        gain,
        lose,
    }
}

/// Every ported quirk in tg's menu order (`cmp_quirk_asc`: positive, negative, neutral, then by
/// name). BandaStation texts.
pub const QUIRKS: [QuirkDef; 40] = [
    q(
        Quirk::AlcoholTolerance,
        "/datum/quirk/alcohol_tolerance",
        "Alcohol Tolerance",
        "Вы пьянеете медленнее и испытываете меньше проблем от алкоголя.",
        "beer",
        4,
        Some((
            Span::Notice,
            "Вам кажется, что вы можете выпить целую кегу пива!",
        )),
        Some((
            Span::Danger,
            "Вы больше не испытываете особой стойкости к алкоголю.",
        )),
    ),
    q(
        Quirk::Bilingual,
        "/datum/quirk/bilingual",
        "Bilingual",
        "За эти годы вы освоили еще один язык!",
        "globe",
        4,
        Some((
            Span::Notice,
            "Некоторые из слов ваших собеседников точно не являются обычными. Хорошо, что вы \
             научились их понимать.",
        )),
        Some((Span::Notice, "Кажется, вы забыли свой второй язык.")),
    ),
    q(
        Quirk::DrunkHealing,
        "/datum/quirk/drunkhealing",
        "Drunken Resilience",
        "Ничто так не помогает почувствовать себя на вершине мира, как хорошая выпивка. Когда вы \
         пьяны, вы медленно восстанавливаетесь от повреждений.",
        "wine-bottle",
        8,
        Some((Span::Notice, "Вам кажется, что немного выпить не помешает.")),
        Some((
            Span::Danger,
            "Вам больше не хочется пить, чтобы облегчить боль.",
        )),
    ),
    q(
        Quirk::Freerunning,
        "/datum/quirk/freerunning",
        "Freerunning",
        "У вас отлично получаются быстрые движения! Вы можете быстрее взбираться на столы и не получать повреждений от коротких падений.",
        "running",
        8,
        Some((Span::Notice, "Вы чувствуете себя уверенно на своих двоих!")),
        Some((Span::Danger, "Вы снова чувствуете себя неуклюжим.")),
    ),
    q(
        Quirk::Friendly,
        "/datum/quirk/friendly",
        "Friendly",
        "Вы обнимаете лучше всех, особенно когда вы в хорошем настроении.",
        "hands-helping",
        2,
        Some((Span::Notice, "Вам хочется обнять кого-то.")),
        Some((
            Span::Danger,
            "Вы больше не чувствуете себя обязанным обнимать других.",
        )),
    ),
    q(
        Quirk::Jolly,
        "/datum/quirk/jolly",
        "Jolly",
        "Иногда вы просто чувствуете себя счастливым без всякой причины.",
        "grin",
        4,
        None,
        None,
    ),
    q(
        Quirk::LightStep,
        "/datum/quirk/light_step",
        "Light Step",
        "Вы ходите мягким шагом; шаги и наступание на острые предметы будут тише и менее болезненными. Кроме того, вы не испачкаете руки и одежду, если наступите в кровь.",
        "shoe-prints",
        4,
        Some((Span::Notice, "Вы ходите с большей гибкостью.")),
        Some((Span::Danger, "Вы начинаете рыскать повсюду, как варвар.")),
    ),
    q(
        Quirk::Voracious,
        "/datum/quirk/voracious",
        "Voracious",
        "Ничто не встает между вами и едой. Вы едите быстрее и можете налегать на нездоровую \
         пищу! Быть толстым вам очень идет.",
        "drumstick-bite",
        4,
        Some((Span::Notice, "ХОЧУ ЖРАААААТЬ!!")),
        Some((Span::Danger, "Кажется вы больше не хотите ЖРАААААТЬ.")),
    ),
    q(
        Quirk::Alcoholic,
        "/datum/quirk/item_quirk/addict/alcoholic",
        "Alcoholic",
        "Вы просто не можете жить без алкоголя. Ваша печень - это машина, которая превращает этанол в ацетальдегид.",
        "wine-glass",
        -4,
        Some((Span::Danger, "Вам действительно нужно выпить.")),
        Some((
            Span::Notice,
            "Алкоголь уже не кажется таким привлекательным.",
        )),
    ),
    q(
        Quirk::BadBack,
        "/datum/quirk/badback",
        "Bad Back",
        "Из-за плохой осанки рюкзаки и другие сумки никогда не сидят на вашей спине правильно. \
         Впрочем, более равномерно взвешенные предметы вполне подойдут.",
        "hiking",
        -8,
        Some((Span::Danger, "Ваша спина действительно болит!")),
        Some((Span::Notice, "Ваша спина чувствует себя лучше.")),
    ),
    q(
        Quirk::Blind,
        "/datum/quirk/item_quirk/blindness",
        "Blind",
        "Вы полностью слепой, и это необратимо.",
        "blind",
        -16,
        Some((Span::Danger, "Вы ослепли и ничего не видите.")),
        Some((Span::Notice, "Чудесным образом к вам вернулось зрение.")),
    ),
    q(
        Quirk::BloodDeficiency,
        "/datum/quirk/blooddeficiency",
        "Blood Deficiency",
        "Ваш организм не может вырабатывать достаточно крови для поддержания жизнедеятельности.",
        "tint",
        -8,
        Some((Span::Danger, "Вы чувствуете, как силы покидают вас.")),
        Some((Span::Notice, "Вы снова полны сил.")),
    ),
    q(
        Quirk::BodyPurist,
        "/datum/quirk/body_purist",
        "Body Purist",
        "Вы верите, что ваше тело - это храм, а его естественная форма - воплощение совершенства. Соответственно, вы презираете идею когда-либо дополнять его неестественными частями, кибернетическими протезами или чем-то подобным.",
        "person-rays",
        -2,
        Some((
            Span::Danger,
            "Теперь вам начинает не нравиться идея установки каких-либо имплантатов.",
        )),
        Some((
            Span::Notice,
            "Может быть, импланты - это не так уж и плохо. Теперь вы нормально относитесь ко всем видам изменения тела.",
        )),
    ),
    q(
        Quirk::BrainTumor,
        "/datum/quirk/item_quirk/brainproblems",
        "Brain Tumor",
        "У вас в мозгу есть маленький друг, который медленно разрушает его. Лучше взять с собой \
         немного маннитола!",
        "brain",
        -12,
        Some((Span::Danger, "Вы ощущаете тяжесть мышления.")),
        Some((Span::Notice, "Вы снова здраво мыслите.")),
    ),
    q(
        Quirk::Csl,
        "/datum/quirk/csl",
        "Common Second Language",
        "Общегалактический язык не является вашим родным языком — это нечто, чему вам пришлось научиться в процессе жизни. Некоторые слова на общем языке звучат чуждо, и в моменты тревоги или расстройства вы можете вернуться к своему родному языку.",
        "landmark-dome",
        -2,
        Some((Span::Danger, "Вам трудно понимать общегалактический язык.")),
        Some((
            Span::Notice,
            "Вы начинаете лучше разбираться в общегалактическом языке.",
        )),
    ),
    q(
        Quirk::Deaf,
        "/datum/quirk/item_quirk/deafness",
        "Deaf",
        "Вы неизлечимо глухой.",
        "deaf",
        -8,
        Some((Span::Danger, "Вы ничего не слышите.")),
        Some((Span::Notice, "Вы снова можете слышать!")),
    ),
    q(
        Quirk::Depression,
        "/datum/quirk/depression",
        "Depression",
        "Иногда вы просто ненавидите жизнь.",
        "frown",
        -3,
        Some((Span::Danger, "У вас начинается депрессия.")),
        Some((Span::Notice, "Вы больше не испытываете депрессию.")),
    ),
    q(
        Quirk::MedicineAllergy,
        "/datum/quirk/item_quirk/allergic",
        "Extreme Medicine Allergy",
        "С самого детства у вас была аллергия на некоторые химические вещества...",
        "prescription-bottle",
        -6,
        Some((
            Span::Danger,
            "Вы чувствуете, как меняется ваша иммунная система.",
        )),
        Some((
            Span::Notice,
            "Вы чувствуете, как ваша иммунная система приходит норму.",
        )),
    ),
    q(
        Quirk::FoodAllergy,
        "/datum/quirk/item_quirk/food_allergic",
        "Food Allergy",
        "С самого детства у вас была аллергия на некоторые продукты.",
        "shrimp",
        -2,
        Some((
            Span::Danger,
            "Вы чувствуете, как меняется ваша иммунная система.",
        )),
        Some((
            Span::Notice,
            "Вы чувствуете, как ваша иммунная система возвращается к норме.",
        )),
    ),
    q(
        Quirk::Hemiplegic,
        "/datum/quirk/hemiplegic",
        "Hemiplegic",
        "Половина вашего тела не работает. Ничто никогда это не исправит.",
        "circle-half-stroke",
        -10,
        None,
        None,
    ),
    q(
        Quirk::Hypersensitive,
        "/datum/quirk/hypersensitive",
        "Hypersensitive",
        "К сожалению или к счастью, вам кажется, что всё влияет на ваше настроение сильнее \
         обычного.",
        "flushed",
        -2,
        Some((
            Span::Danger,
            "Вы, кажется, делаете из мухи слона где только возможно.",
        )),
        Some((
            Span::Notice,
            "Кажется, вы больше не придаете всему так много значения.",
        )),
    ),
    q(
        Quirk::Illiterate,
        "/datum/quirk/illiterate",
        "Illiterate",
        "Вы бросили школу и не умеете читать и писать. Это влияет на чтение, письмо, использование компьютеров и другой электроники.",
        "graduation-cap",
        -8,
        None,
        None,
    ),
    q(
        Quirk::Junkie,
        "/datum/quirk/item_quirk/addict/junkie",
        "Junkie",
        "Вы не можете перестать употреблять тяжелые наркотики.",
        "pills",
        -6,
        Some((Span::Danger, "Вы вдруг почувствовали тягу к наркотикам.")),
        None,
    ),
    q(
        Quirk::LightDrinker,
        "/datum/quirk/light_drinker",
        "Light Drinker",
        "Вы просто не умеете обращаться с напитками и очень быстро пьянеете.",
        "cocktail",
        -2,
        Some((
            Span::Notice,
            "От одной мысли об употреблении алкоголя голова идет кругом.",
        )),
        Some((
            Span::Danger,
            "Вы больше не подвержены сильному влиянию алкоголя.",
        )),
    ),
    q(
        Quirk::Mute,
        "/datum/quirk/mute",
        "Mute",
        "По какой-то причине вы совершенно не можете говорить.",
        "volume-xmark",
        -4,
        Some((Span::Danger, "Вы обнаруживаете, что не можете говорить!")),
        Some((
            Span::Notice,
            "Вы чувствуете, как голосовые связки приходят в норму.",
        )),
    ),
    q(
        Quirk::Nearsighted,
        "/datum/quirk/item_quirk/nearsighted",
        "Nearsighted",
        "У вас близорукость, но зато с парой очков выписанных по рецепту.",
        "glasses",
        -4,
        Some((
            Span::Danger,
            "Предметы, находящиеся на большом расстоянии от вас, выглядят размыто.",
        )),
        Some((
            Span::Notice,
            "Вы начинаете видеть вещи на далеком расстоянии как обычно.",
        )),
    ),
    q(
        Quirk::Pacifist,
        "/datum/quirk/nonviolent",
        "Pacifist",
        "Мысли о насилии вызывают у вас тошноту. Настолько, что вы не можете причинить никому \
         вреда.",
        "peace",
        -8,
        Some((
            Span::Danger,
            "Вы чувствуете, как любая мысль о насилии отвращает вас!",
        )),
        Some((
            Span::Notice,
            "Кажется, вы уже и не такой беззащитный и можете дать отпор, как раньше.",
        )),
    ),
    q(
        Quirk::Paraplegic,
        "/datum/quirk/paraplegic",
        "Paraplegic",
        "У вас отказали ноги, и вы больше никогда не сможете ходить, зато у вас есть бесплатная \
         коляска!",
        "wheelchair",
        -12,
        // Handled by the trauma.
        None,
        None,
    ),
    q(
        Quirk::Smoker,
        "/datum/quirk/item_quirk/addict/smoker",
        "Smoker",
        "Иногда вам очень хочется закурить. Возможно, это не очень полезно для ваших легких.",
        "smoking",
        -4,
        Some((Span::Danger, "Эх, вот бы сейчас закурить.")),
        Some((
            Span::Notice,
            "Вы больше не чувствуете такой сильной зависимости от никотина.",
        )),
    ),
    q(
        Quirk::SoftSpoken,
        "/datum/quirk/softspoken",
        "Soft-Spoken",
        "Вы немногословны, и ваш голос трудно услышать.",
        "comment",
        -2,
        Some((Span::Danger, "Вам кажется, что вы стали говорить тише.")),
        Some((
            Span::Notice,
            "Вам кажется, что вы стали говорить как обычно.",
        )),
    ),
    q(
        Quirk::TinMan,
        "/datum/quirk/tin_man",
        "Tin Man",
        "Ой! Всё в протезах! В результате поистине жестокого космического наказания большинство ваших внутренних органов было заменено на дешевые протезы.",
        "user-gear",
        -6,
        None,
        None,
    ),
    q(
        Quirk::Unstable,
        "/datum/quirk/unstable",
        "Unstable",
        "Из-за прошлых неприятностей вы не сможете восстановить рассудок, если потеряете его. \
         Будьте очень осторожны в управлении своим настроением!",
        "angry",
        -10,
        Some((
            Span::Danger,
            "У вас сейчас довольно много разных мыслей на уме.",
        )),
        Some((Span::Notice, "Ваш разум наконец-то успокоился.")),
    ),
    q(
        Quirk::Ageusia,
        "/datum/quirk/no_taste",
        "Ageusia",
        "Вы не сможете ничего попробовать на вкус! Токсичная пища всё еще может отравить вас.",
        "meh-blank",
        0,
        Some((
            Span::Notice,
            "Кажется, вы больше не чувствуете каких-либо вкусов при приеме пищи!",
        )),
        Some((Span::Notice, "У вас снова появилось ощущение вкуса во рту!")),
    ),
    q(
        Quirk::PineappleLiker,
        "/datum/quirk/pineapple_liker",
        "Ananas Affinity",
        "Вам очень нравятся ананасы. Кажется, вы никогда не сможете насытиться их великолепием!",
        "thumbs-up",
        0,
        Some((Span::Notice, "Вы чувствуете сильную тягу к ананасам.")),
        Some((
            Span::Notice,
            "Ваше отношение к ананасам, похоже, вернулось к нейтральному состоянию.",
        )),
    ),
    q(
        Quirk::PineappleHater,
        "/datum/quirk/pineapple_hater",
        "Ananas Aversion",
        "Вы испытываете сильное отвращение к ананасам. Серьезно, как, черт возьми, кто-то может говорить, что они вкусные? И какой безумец вообще осмелится положить их на пиццу!?",
        "thumbs-down",
        0,
        Some((
            Span::Notice,
            "Вы размышляете о том, каким извращенцам нравятся ананасы...",
        )),
        Some((
            Span::Notice,
            "Ваше отношение к ананасам, похоже, вернулось к нейтральному состоянию.",
        )),
    ),
    q(
        Quirk::DeviantTastes,
        "/datum/quirk/deviant_tastes",
        "Deviant Tastes",
        "Вам не нравится еда, которая нравится большинству людей, и вы находите вкусным то, что не всем по нраву.",
        "grin-tongue-squint",
        0,
        Some((
            Span::Notice,
            "Вам начинает хотеться чего-то странного на вкус.",
        )),
        Some((Span::Notice, "Вам снова хочется есть нормальную пищу.")),
    ),
    q(
        Quirk::Foreigner,
        "/datum/quirk/foreigner",
        "Foreigner",
        "Вы не местный. Вы не знаете общегалактический!",
        "language",
        0,
        Some((
            Span::Notice,
            "Слова, которые произносятся вокруг вас, не имеют никакого смысла.",
        )),
        Some((Span::Notice, "Вы свободно владеете общегалактическим.")),
    ),
    q(
        Quirk::Monochromacy,
        "/datum/quirk/monochromatic",
        "Monochromacy",
        "Вы страдаете полным дальтонизмом и воспринимаете весь мир в черно-белых тонах.",
        "adjust",
        0,
        None,
        None,
    ),
    q(
        Quirk::Bald,
        "/datum/quirk/item_quirk/bald",
        "Smooth-Headed",
        "У вас нет волос, и вы чувствуете себя неуверенно из-за этого! Не снимайте парик или хотя бы прикрывайте голову.",
        "egg",
        0,
        Some((
            Span::Notice,
            "Ваша голова настолько гладкая, насколько это возможно, это ужасно.",
        )),
        Some((
            Span::Notice,
            "У вас чешется голова, может, это... растут волосы?!",
        )),
    ),
    q(
        Quirk::Vegetarian,
        "/datum/quirk/vegetarian",
        "Vegetarian",
        "Вы считаете идею употребления мяса аморальной и физически отвратительной.",
        "carrot",
        0,
        Some((Span::Notice, "Вы испытываете отвращение к идее есть мясо.")),
        Some((
            Span::Notice,
            "Вам кажется, что есть мясо не так уж и плохо.",
        )),
    ),
];

/// `GLOB.quirk_blacklist`, the lists with at least two ported quirks: quirks of one list
/// exclude each other.
pub const BLACKLIST: [&[Quirk]; 7] = [
    &[Quirk::Blind, Quirk::Nearsighted],
    &[Quirk::Jolly, Quirk::Depression, Quirk::Hypersensitive],
    &[Quirk::Ageusia, Quirk::Vegetarian, Quirk::DeviantTastes],
    &[Quirk::PineappleLiker, Quirk::PineappleHater],
    &[Quirk::AlcoholTolerance, Quirk::LightDrinker],
    &[Quirk::Mute, Quirk::SoftSpoken],
    &[Quirk::Bilingual, Quirk::Foreigner, Quirk::Csl],
];

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

/// tg `has_quirk`.
pub fn has(world: &World, mob: Entity, quirk: Quirk) -> bool {
    world.get::<Quirks>(mob).is_some_and(|q| q.has(quirk))
}

/// tg `SSquirks.filter_invalid_quirks`: drops unknown names, blacklisted pairs and positive
/// quirks past the limit, then, while the point balance is positive (more spent than given),
/// positive quirks from the first on. Returns the same list when all of it is valid.
pub fn filter_invalid(names: &[String]) -> Vec<String> {
    let mut new: Vec<String> = Vec::new();
    let mut positive: Vec<(String, i32)> = Vec::new();
    let mut balance = -DEFAULT_QUIRK_POINTS;
    for name in names {
        let Some(quirk) = Quirk::by_name(name) else {
            continue;
        };
        let blacklisted = BLACKLIST.iter().any(|list| {
            list.contains(&quirk)
                && list
                    .iter()
                    .any(|other| *other != quirk && new.iter().any(|n| n == other.def().name))
        });
        if blacklisted {
            continue;
        }
        let value = quirk.def().value;
        if value > 0 {
            if positive.len() == MAX_POSITIVE_QUIRKS {
                continue;
            }
            positive.push((name.clone(), value));
        }
        balance += value;
        new.push(name.clone());
    }
    if balance > 0 {
        let mut left = balance;
        for (name, value) in positive {
            left -= value;
            new.retain(|n| *n != name);
            if left <= 0 {
                break;
            }
        }
    }
    new
}

/// tg `give_quirk`/`remove_quirk` of the preferences middleware: the change is kept only when
/// the result is valid as a whole. True if it was applied.
pub fn change_prefs(prefs: &mut QuirkPrefs, name: &str, give: bool) -> bool {
    let current = filter_invalid(&prefs.0);
    let new: Vec<String> = if give {
        let mut new = current.clone();
        if !new.iter().any(|n| n == name) {
            new.push(name.to_owned());
        }
        new
    } else {
        if !current.iter().any(|n| n == name) {
            return false;
        }
        current.iter().filter(|n| *n != name).cloned().collect()
    };
    if filter_invalid(&new) != new {
        return false;
    }
    prefs.0 = new;
    true
}

/// tg `AssignQuirks`: the client's chosen quirks onto its new human, silently, with their items.
pub fn assign(world: &mut World, client: Entity, mob: Entity) {
    let Some(prefs) = world.get::<QuirkPrefs>(client) else {
        return;
    };
    let names = filter_invalid(&prefs.0);
    for name in names {
        if let Some(quirk) = Quirk::by_name(&name) {
            add_quirk(world, mob, quirk, false, true);
        }
    }
}

fn tell_mob(world: &mut World, mob: Entity, (span, text): (Span, &str)) {
    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
        crate::tools::tell(world, client, span, text);
    }
}

/// tg `add_quirk` -> `add_to_holder`: false if the mob has it already. `announce`: the gain
/// line; `unique`: the one-time effects (items).
pub fn add_quirk(
    world: &mut World,
    mob: Entity,
    quirk: Quirk,
    announce: bool,
    unique: bool,
) -> bool {
    if world.get_entity(mob).is_err() || has(world, mob, quirk) {
        return false;
    }
    match world.get_mut::<Quirks>(mob) {
        Some(mut quirks) => quirks.0.push(quirk),
        None => {
            world.entity_mut(mob).insert(Quirks(vec![quirk]));
        }
    }
    let def = quirk.def();
    info!("{mob} gained the quirk {}", def.path);
    // `add`.
    match quirk {
        Quirk::Hypersensitive => {
            if let Some(mut mood) = world.get_mut::<Mood>(mob) {
                mood.mood_modifier += 0.5;
                mood.update_mood();
            }
        }
        Quirk::Unstable => {
            if let Some(mut mood) = world.get_mut::<Mood>(mob) {
                mood.unstable = true;
            }
        }
        Quirk::Mute | Quirk::Pacifist | Quirk::SoftSpoken => traumas::refresh(world, mob),
        Quirk::Bilingual => bilingual(world, mob),
        Quirk::Csl => csl(world, mob, true),
        Quirk::Foreigner => foreigner(world, mob, true),
        Quirk::Bald => {
            let covered = head_covered(world, mob);
            let old_hair = world
                .get_mut::<crate::human::HumanLook>(mob)
                .map(|mut look| look.hair_style.take());
            if let Some(old_hair) = old_hair {
                world.entity_mut(mob).insert(BaldHead { old_hair, covered });
                crate::inventory::refresh_look(world, mob);
            }
        }
        Quirk::BodyPurist => update_cybernetics(world, mob),
        Quirk::Junkie | Quirk::Smoker | Quirk::Alcoholic => addict_add(world, mob, quirk),
        Quirk::MedicineAllergy | Quirk::FoodAllergy => allergy_add(world, mob, quirk),
        // `/datum/brain_trauma/severe/paralysis/hemiplegic` of a random side.
        Quirk::Hemiplegic => {
            let side = if world.get_resource_or_init::<CombatRng>().chance(50.0) {
                Paralysis::Left
            } else {
                Paralysis::Right
            };
            if let Some(brain) = organs::organ(world, mob, Slot::Brain) {
                traumas::gain(
                    world,
                    brain,
                    Kind::Paralysis(side),
                    Some(Resilience::Absolute),
                );
            }
        }
        // `/datum/brain_trauma/severe/paralysis/paraplegic`, `TRAUMA_RESILIENCE_ABSOLUTE`.
        Quirk::Paraplegic => {
            if let Some(brain) = organs::organ(world, mob, Slot::Brain) {
                traumas::gain(
                    world,
                    brain,
                    Kind::Paralysis(Paralysis::Legs),
                    Some(Resilience::Absolute),
                );
            }
        }
        _ => {}
    }
    if announce && let Some(gain) = def.gain {
        tell_mob(world, mob, gain);
    }
    if unique {
        match quirk {
            Quirk::Blind => {
                give_item(world, mob, BLINDFOLD, &[Location::Eyes, Location::Hands]);
            }
            Quirk::Nearsighted => {
                give_item(
                    world,
                    mob,
                    GLASSES,
                    &[Location::Eyes, Location::Backpack, Location::Hands],
                );
            }
            Quirk::Deaf => {
                give_item(world, mob, DEAF_PIN, &[Location::Backpack, Location::Hands]);
            }
            Quirk::TinMan => tin_man(world, mob),
            Quirk::Junkie | Quirk::Smoker | Quirk::Alcoholic => addict_items(world, mob, quirk),
            Quirk::MedicineAllergy | Quirk::FoodAllergy => allergy_items(world, mob, quirk),
            Quirk::BrainTumor => {
                if let Some(at) = give_item(
                    world,
                    mob,
                    MANNITOL_BOTTLE,
                    &[
                        Location::LeftPocket,
                        Location::RightPocket,
                        Location::Backpack,
                        Location::Hands,
                    ],
                ) {
                    let text = format!(
                        "У вас есть баночка таблеток маннитола {at}. Они помогут вам выжить, пока \
                         вы не получите запас лекарств. Не полагайтесь на них слишком сильно!"
                    );
                    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
                        crate::tools::tell(world, client, Span::BoldNotice, text);
                    }
                }
            }
            _ => {}
        }
    }
    true
}

/// `/datum/quirk/bilingual/add` with the "Random" language preference: one roundstart species
/// language, or Galactic Uncommon when that one is already known.
// shortcut: no language, "speakable" or skill preferences (always spoken and understood).
fn bilingual(world: &mut World, mob: Entity) {
    use ss13_chat::language::{ALL, LanguageHolder, LanguageId, LanguageSource};
    let pool = crate::species::uncommon_roundstart_languages();
    if pool.is_empty() {
        return;
    }
    let pick = {
        let mut rng = world.get_resource_or_init::<CombatRng>();
        pool[rng.range(0, pool.len() as u32 - 1) as usize]
    };
    let knows = |w: &World, l: LanguageId| {
        ss13_chat::language::understanding(w.get::<LanguageHolder>(mob), false, l)
            >= ss13_chat::language::FULL
    };
    let mut language = pick;
    if knows(world, language) {
        language = LanguageId::Uncommon;
        let text = if knows(world, language) {
            "Вы уже знакомы с данной чертой в своих предпочтениях, поэтому не стали изучать ее."
        } else {
            "Вы уже знакомы с данной чертой в своих предпочтениях, поэтому вместо нее вы выучили \
             старогалактический."
        };
        if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
            crate::tools::tell(world, client, Span::BoldNotice, text);
        }
        if knows(world, language) {
            return;
        }
    }
    ss13_chat::language::edit_holder(world, mob, |h| {
        h.grant(language, ALL, LanguageSource::Quirk);
    });
}

/// tg `remove_quirk`: false if the mob does not have it.
pub fn remove_quirk(world: &mut World, mob: Entity, quirk: Quirk) -> bool {
    let Some(mut quirks) = world.get_mut::<Quirks>(mob) else {
        return false;
    };
    let Some(i) = quirks.0.iter().position(|q| *q == quirk) else {
        return false;
    };
    quirks.0.remove(i);
    match quirk {
        Quirk::Hypersensitive => {
            if let Some(mut mood) = world.get_mut::<Mood>(mob) {
                mood.mood_modifier -= 0.5;
                mood.update_mood();
            }
        }
        Quirk::Unstable => {
            if let Some(mut mood) = world.get_mut::<Mood>(mob) {
                mood.unstable = false;
            }
        }
        Quirk::Jolly => mood::clear_event(world, mob, "jolly"),
        Quirk::Depression => mood::clear_event(world, mob, "depression"),
        Quirk::BadBack => mood::clear_event(world, mob, "back_pain"),
        Quirk::Mute | Quirk::Pacifist | Quirk::SoftSpoken => traumas::refresh(world, mob),
        Quirk::Csl => csl(world, mob, false),
        Quirk::Foreigner => foreigner(world, mob, false),
        Quirk::MedicineAllergy | Quirk::FoodAllergy => {
            if let Some(mut allergies) = world.get_mut::<Allergies>(mob) {
                if quirk == Quirk::MedicineAllergy {
                    allergies.medicines.clear();
                } else {
                    allergies.foods = 0;
                }
            }
        }
        // `/datum/quirk/item_quirk/addict/remove`: every addiction goes.
        Quirk::Junkie | Quirk::Smoker | Quirk::Alcoholic => {
            if let Some(mut addicts) = world.get_mut::<AddictQuirks>(mob) {
                addicts.0.retain(|a| a.quirk != quirk);
            }
            for a in crate::addictions::Addiction::ALL {
                crate::addictions::remove_points(
                    world,
                    mob,
                    a,
                    crate::addictions::MAX_ADDICTION_POINTS,
                );
            }
        }
        Quirk::BodyPurist => {
            world.entity_mut(mob).remove::<Cybernetics>();
            mood::clear_event(world, mob, "body_purist");
        }
        Quirk::Bald => {
            mood::clear_event(world, mob, "bad_hair_day");
            let bald = world.entity_mut(mob).take::<BaldHead>();
            let restored = bald.is_some_and(|bald| {
                world
                    .get_mut::<crate::human::HumanLook>(mob)
                    .is_some_and(|mut look| {
                        let bald_now = look.hair_style.is_none();
                        if bald_now {
                            look.hair_style = bald.old_hair;
                        }
                        bald_now
                    })
            });
            if restored {
                crate::inventory::refresh_look(world, mob);
            }
        }
        // `remove_all_languages(LANGUAGE_QUIRK)`, `remove_all_partial_languages(LANGUAGE_QUIRK)`.
        Quirk::Bilingual => {
            if let Some(mut holder) = world.get_mut::<ss13_chat::language::LanguageHolder>(mob) {
                holder.remove_all(Some(ss13_chat::language::LanguageSource::Quirk));
            }
        }
        _ => {}
    }
    if let Some(lose) = quirk.def().lose {
        tell_mob(world, mob, lose);
    }
    true
}

/// `/datum/quirk/alcohol_tolerance` (×0.7) and `light_drinker` (×2): the booze power of
/// `/datum/reagent/consumable/ethanol/on_mob_life`.
pub fn booze_multiplier(world: &World, mob: Entity) -> f32 {
    let Some(quirks) = world.get::<Quirks>(mob) else {
        return 1.0;
    };
    let mut multiplier = 1.0;
    if quirks.has(Quirk::AlcoholTolerance) {
        multiplier *= 0.7;
    }
    if quirks.has(Quirk::LightDrinker) {
        multiplier *= 2.0;
    }
    multiplier
}

/// tg `LOCATION_*` of `give_item_to_holder`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Location {
    Eyes,
    LeftPocket,
    RightPocket,
    Backpack,
    Hands,
}

/// tg `give_item_to_holder`: a new `path` into the first location that takes it, else at the
/// mob's feet. Where it went (`None`: no template for it).
fn give_item(
    world: &mut World,
    mob: Entity,
    path: &str,
    locations: &[Location],
) -> Option<&'static str> {
    let item = spawn_item(world, path)?;
    Some(place_item(world, mob, item, locations))
}

/// A new quirk item of `path` from its outfit template, nowhere yet.
fn spawn_item(world: &mut World, path: &str) -> Option<Entity> {
    if !world.contains_resource::<OutfitTemplates>() {
        return None;
    }
    let item = world.resource_scope(|world, templates: Mut<OutfitTemplates>| {
        Some(templates.0.get(path)?.spawn(world, None))
    });
    let Some(item) = item else {
        warn!("quirk item {path} has no prototype");
        return None;
    };
    world.entity_mut(item).insert(Networked);
    crate::storage::finish_item(world, item, 0);
    Some(item)
}

/// The rest of `give_item_to_holder`: `item` into the first location that takes it.
fn place_item(
    world: &mut World,
    mob: Entity,
    item: Entity,
    locations: &[Location],
) -> &'static str {
    for &location in locations {
        let Some(mut inventory) = world.get::<Inventory>(mob).cloned() else {
            break;
        };
        let (slot, where_) = match location {
            Location::Eyes => (InventorySlot::Eyes, "на глазах"),
            Location::LeftPocket => (InventorySlot::LeftPocket, "в левом кармане"),
            Location::RightPocket => (InventorySlot::RightPocket, "в правом кармане"),
            Location::Hands => {
                let Some(hand) = inventory.hands.iter().position(Option::is_none) else {
                    continue;
                };
                inventory.hands[hand] = Some(item);
                world.entity_mut(item).insert(CarriedBy(mob));
                world.entity_mut(mob).insert(inventory);
                return "в руках";
            }
            Location::Backpack => {
                let Some(pack) = inventory.back else {
                    continue;
                };
                if crate::storage::can_insert(world, pack, item).is_err() {
                    continue;
                }
                crate::storage::put_inside(world, item, pack);
                return "в рюкзаке";
            }
        };
        // Pockets need a jumpsuit (tg `mob_can_equip`).
        if matches!(slot, InventorySlot::LeftPocket | InventorySlot::RightPocket)
            && inventory.uniform.is_none()
        {
            continue;
        }
        if !crate::inventory::can_equip(world, mob, &inventory, item, slot) {
            continue;
        }
        match slot {
            InventorySlot::Eyes => inventory.eyes = Some(item),
            InventorySlot::LeftPocket => inventory.pockets[0] = Some(item),
            _ => inventory.pockets[1] = Some(item),
        }
        world.entity_mut(item).insert(CarriedBy(mob));
        world.entity_mut(mob).insert(inventory);
        return where_;
    }
    if let Some(&pos) = world.get::<TilePos>(mob) {
        crate::inventory::put_down(world, item, pos);
    }
    "у ваших ног"
}

/// Extreme Medicine Allergy's pick: `MEDICINE_ALLERGIES` of the `/datum/reagent/medicine`
/// subtypes outside the blacklist.
pub fn pick_medicine_allergies(medicines: &[&str], rng: &mut CombatRng) -> Vec<String> {
    use ss13_mapload::fidelity::is_under;
    let mut pool: Vec<&str> = medicines
        .iter()
        .copied()
        .filter(|p| is_under(p, "/datum/reagent/medicine") && *p != "/datum/reagent/medicine")
        .filter(|p| !ALLERGY_BLACKLIST.iter().any(|b| is_under(p, b)))
        .collect();
    let mut out = Vec::new();
    while out.len() < MEDICINE_ALLERGIES && !pool.is_empty() {
        let i = rng.range(0, pool.len() as u32 - 1) as usize;
        out.push(pool.swap_remove(i).to_owned());
    }
    out
}

/// `/datum/quirk/item_quirk/allergic/add` and `food_allergic/add`: the allergens.
// shortcut: no Food Allergy preference (always "Random").
fn allergy_add(world: &mut World, mob: Entity, quirk: Quirk) {
    let medicines: Vec<String> = if quirk == Quirk::MedicineAllergy {
        let paths: Vec<String> = world
            .get_resource::<crate::chem::Chem>()
            .map(|c| c.0.reagents.iter().map(|r| r.path.clone()).collect())
            .unwrap_or_default();
        let paths: Vec<&str> = paths.iter().map(String::as_str).collect();
        let mut rng = world.get_resource_or_init::<CombatRng>();
        pick_medicine_allergies(&paths, &mut rng)
    } else {
        Vec::new()
    };
    let food = if quirk == Quirk::FoodAllergy {
        let mut rng = world.get_resource_or_init::<CombatRng>();
        FOOD_ALLERGIES[rng.range(0, FOOD_ALLERGIES.len() as u32 - 1) as usize].1
    } else {
        0
    };
    if world.get::<Allergies>(mob).is_none() {
        world.entity_mut(mob).insert(Allergies::default());
    }
    let mut allergies = world.get_mut::<Allergies>(mob).expect("inserted above");
    if quirk == Quirk::MedicineAllergy {
        allergies.medicines = medicines;
    } else {
        allergies.foods = food;
    }
}

/// What the mob is allergic to, for the dog tag and the chat.
pub fn allergy_string(world: &World, mob: Entity, quirk: Quirk) -> String {
    let Some(allergies) = world.get::<Allergies>(mob) else {
        return String::new();
    };
    if quirk == Quirk::FoodAllergy {
        return FOOD_ALLERGIES
            .iter()
            .filter(|(_, bit)| allergies.foods & bit != 0)
            .map(|(name, _)| *name)
            .collect::<Vec<_>>()
            .join(", ");
    }
    let chem = world.get_resource::<crate::chem::Chem>();
    allergies
        .medicines
        .iter()
        .map(|path| {
            chem.and_then(|c| c.0.id(path)).map_or(path.clone(), |id| {
                chem.unwrap().0.def(id).name.to_lowercase()
            })
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// `add_unique` and `post_add`: the allergy dog tag and the warning.
// shortcut: the dog tag does not name the allergens (no `dogtag/allergy` text).
fn allergy_items(world: &mut World, mob: Entity, quirk: Quirk) {
    let allergens = allergy_string(world, mob, quirk);
    let text = if quirk == Quirk::FoodAllergy {
        format!("У вас аллергия на: <b>{allergens}</b>. Следите за тем, что вы едите!")
    } else {
        format!(
            "У вас аллергия на: <b>{allergens}</b>. Убедитесь, что вы не употребляете ничего из \
             этого!"
        )
    };
    let span = if quirk == Quirk::FoodAllergy {
        Span::Info
    } else {
        Span::BoldNotice
    };
    tell_mob(world, mob, (span, &text));
    if let Some(at) = give_item(
        world,
        mob,
        ALLERGY_DOGTAG,
        &[Location::Backpack, Location::Hands],
    ) {
        let flavour = if quirk == Quirk::FoodAllergy {
            "Держите его поблизости с кухней."
        } else {
            "Make sure medical staff can see this..."
        };
        tell_mob(
            world,
            mob,
            (
                Span::BoldNotice,
                &format!("У вас есть жетон аллергика {at}. {flavour}"),
            ),
        );
    }
}

/// `histamine` into the blood (no chemistry: nothing).
fn add_histamine(world: &mut World, mob: Entity, amount: f32) {
    let Some(chem) = world
        .get_resource::<crate::chem::Chem>()
        .map(|c| c.0.clone())
    else {
        return;
    };
    let Some(id) = chem.id(HISTAMINE) else {
        return;
    };
    if let Some(mut reagents) = world.get_mut::<crate::chem::MobReagents>(mob) {
        reagents.blood.add_reagent(
            &chem,
            id,
            amount,
            ss13_reagents::DEFAULT_REAGENT_TEMPERATURE,
            None,
            None,
            false,
            false,
        );
    }
}

/// Extreme Medicine Allergy's `block_metab` on each metabolism tick (`crate::chem`): an
/// allergen in the blood deals 3 toxin a second and adds 3 histamine a second, with 10% a
/// second to vomit and take 10 damage to a random organ; epinephrine stops it.
// shortcut: epinephrine stops the reaction but not the allergen's metabolism.
pub fn allergic_reagents(world: &mut World, mob: Entity, secs: f32) {
    let Some(allergies) = world.get::<Allergies>(mob) else {
        return;
    };
    if allergies.medicines.is_empty() || world.get::<Stat>(mob) == Some(&Stat::Dead) {
        return;
    }
    let (count, epinephrine) = {
        let (Some(chem), Some(reagents)) = (
            world.get_resource::<crate::chem::Chem>(),
            world.get::<crate::chem::MobReagents>(mob),
        ) else {
            return;
        };
        let mut count = 0;
        let mut epinephrine = false;
        for r in &reagents.blood.reagents {
            let path = &chem.0.def(r.id).path;
            if ss13_mapload::fidelity::is_under(path, EPINEPHRINE) {
                epinephrine = true;
            }
            if allergies
                .medicines
                .iter()
                .any(|a| ss13_mapload::fidelity::is_under(path, a))
            {
                count += 1;
            }
        }
        (count, epinephrine)
    };
    if count == 0 || epinephrine {
        return;
    }
    for _ in 0..count {
        if let (Some(mut health), Some(mut stat)) = (
            world.get::<Health>(mob).cloned(),
            world.get::<Stat>(mob).copied(),
        ) {
            crate::health::apply_damage(&mut health, &mut stat, DamageType::Tox, 3.0 * secs);
            world.entity_mut(mob).insert((health, stat));
        }
        add_histamine(world, mob, 3.0 * secs);
        if spt_prob(world, 10.0, secs) {
            crate::medical::diseases::vomit(world, mob, 10.0, true, false, 1);
            let slot = {
                let mut rng = world.get_resource_or_init::<CombatRng>();
                [
                    Slot::Brain,
                    Slot::Appendix,
                    Slot::Lungs,
                    Slot::Heart,
                    Slot::Liver,
                    Slot::Stomach,
                ][rng.range(0, 5) as usize]
            };
            organs::adjust_organ_loss(world, mob, slot, 10.0);
        }
    }
}

/// `checkLiked`'s `check_allergic_reaction(foodtypes, chance = 100, histamine_add = 10)` for
/// a food of `foodtypes`: anaphylaxis (or more histamine when it is already there). True if
/// the mob is allergic to it (the other food reactions are skipped).
// The reagent exposures (sugar, alcohol, bugs) that also check it are chemistry's.
pub fn food_allergy_reaction(world: &mut World, mob: Entity, foodtypes: u32) -> bool {
    let allergic = world.get::<Allergies>(mob).map_or(0, |a| a.foods);
    if allergic & foodtypes == 0 {
        return false;
    }
    use crate::medical::diseases::{Disease, Kind, force_contract};
    if force_contract(world, mob, &Disease::new(Kind::Anaphylaxis)) {
        tell_mob(
            world,
            mob,
            (Span::Warning, "Вы чувствуете начинающийся зуд в горле."),
        );
        mood::add_event(world, mob, "allergic_food", &ALLERGIC_FOOD_EVENT);
    } else {
        add_histamine(world, mob, 10.0);
    }
    true
}

/// The addiction of an addict quirk's reagent.
fn addict_addiction(reagent: &str) -> Option<crate::addictions::Addiction> {
    crate::addictions::of_reagent(reagent, 1.0)
}

/// `/datum/quirk/item_quirk/addict/add` with the subtypes' `New`: the drug (Junkie: a random
/// one; Smoker: nicotine and a random pack; Alcoholic: ethanol and a random bottle with its
/// favourite booze), then its addiction maxed out.
// shortcut: no Junkie, Smoker or Alcoholic preference (always "Random").
fn addict_add(world: &mut World, mob: Entity, quirk: Quirk) {
    let state = {
        let mut rng = world.get_resource_or_init::<CombatRng>();
        let mut pick = |n: usize| rng.range(0, n as u32 - 1) as usize;
        match quirk {
            Quirk::Junkie => AddictState {
                quirk,
                reagent: JUNKIE_DRUGS[pick(JUNKIE_DRUGS.len())],
                container: PILL_BOTTLE.to_owned(),
                favorite: None,
                next_process: 0.0,
            },
            Quirk::Smoker => {
                let (pack, cigarette) = SMOKER_PACKS[pick(SMOKER_PACKS.len())];
                AddictState {
                    quirk,
                    reagent: NICOTINE,
                    container: pack.to_owned(),
                    favorite: Some(cigarette.to_owned()),
                    next_process: 0.0,
                }
            }
            _ => {
                let (bottle, booze) = ALCOHOLIC_BOTTLES[pick(ALCOHOLIC_BOTTLES.len())];
                AddictState {
                    quirk,
                    reagent: ETHANOL,
                    container: format!("{BOTTLE_ROOT}{bottle}"),
                    favorite: Some(format!("{ETHANOL}/{booze}")),
                    next_process: 0.0,
                }
            }
        }
    };
    let addiction = addict_addiction(state.reagent);
    match world.get_mut::<AddictQuirks>(mob) {
        Some(mut addicts) => addicts.0.push(state),
        None => {
            world.entity_mut(mob).insert(AddictQuirks(vec![state]));
        }
    }
    if let Some(addiction) = addiction {
        crate::addictions::add_points(
            world,
            mob,
            addiction,
            crate::addictions::MAX_ADDICTION_POINTS,
        );
    }
}

fn addict_state(world: &World, mob: Entity, quirk: Quirk) -> Option<AddictState> {
    world
        .get::<AddictQuirks>(mob)?
        .0
        .iter()
        .find(|a| a.quirk == quirk)
        .cloned()
}

/// `/datum/quirk/item_quirk/addict/add_unique`: the drug container (the Junkie's pill bottle
/// with 7 pills of 3 units), the Smoker's lighter.
// shortcut: the pills keep the template's look (no random `pill[1-20]` state).
fn addict_items(world: &mut World, mob: Entity, quirk: Quirk) {
    let Some(state) = addict_state(world, mob, quirk) else {
        return;
    };
    let Some(container) = spawn_item(world, &state.container) else {
        return;
    };
    if quirk == Quirk::Junkie {
        let chem = world
            .get_resource::<crate::chem::Chem>()
            .map(|c| c.0.clone());
        let id = chem.as_ref().and_then(|c| c.id(state.reagent));
        for _ in 0..JUNKIE_PILLS {
            let Some(pill) = spawn_item(world, PILL) else {
                break;
            };
            crate::storage::put_inside(world, pill, container);
            if let (Some(chem), Some(id)) = (chem.as_ref(), id)
                && let Some(mut reagents) = world.get_mut::<crate::chem::Reagents>(pill)
            {
                reagents.0.add_reagent(
                    chem,
                    id,
                    JUNKIE_PILL_UNITS,
                    ss13_reagents::DEFAULT_REAGENT_TEMPERATURE,
                    None,
                    None,
                    true,
                    false,
                );
            }
        }
    }
    let at = place_item(
        world,
        mob,
        container,
        &[
            Location::LeftPocket,
            Location::RightPocket,
            Location::Backpack,
            Location::Hands,
        ],
    );
    let flavour = match quirk {
        Quirk::Junkie => "Лучше надеяться, что у вас они не закончатся...",
        Quirk::Smoker => "Убедитесь, что вы получите свои любимые, до того, как они закончатся.",
        _ => "Убедитесь, что запасы вашего любимого напитка не закончатся.",
    };
    let name = crate::chem::name_of(world, container);
    tell_mob(
        world,
        mob,
        (
            Span::BoldNotice,
            &format!("У вас есть {name} {at}. {flavour}"),
        ),
    );
    if quirk == Quirk::Smoker {
        give_item(
            world,
            mob,
            LIGHTER,
            &[
                Location::LeftPocket,
                Location::RightPocket,
                Location::Backpack,
                Location::Hands,
            ],
        );
    }
}

/// `/datum/quirk/item_quirk/addict/process` (every 30 s: the addiction comes back if it was
/// kicked), the Smoker's brand check of the cigarette in the mask slot and the Alcoholic's
/// `check_brandy` (followed every fire instead of each reagent tick).
// The booze check looks at the blood and the stomach (tg: the reagents that tick).
fn addict_process(world: &mut World, mob: Entity, quirk: Quirk) {
    let Some(state) = addict_state(world, mob, quirk) else {
        return;
    };
    // `no_process_traits = list(TRAIT_LIVERLESS_METABOLISM)`.
    if crate::species::has_trait(world, mob, Trait::LiverlessMetabolism) {
        return;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    if now >= state.next_process {
        if let Some(mut addicts) = world.get_mut::<AddictQuirks>(mob)
            && let Some(a) = addicts.0.iter_mut().find(|a| a.quirk == quirk)
        {
            a.next_process = now + ADDICT_PROCESS_SECS;
        }
        if let Some(addiction) = addict_addiction(state.reagent) {
            let active = world
                .get::<crate::addictions::Addictions>(mob)
                .and_then(|a| a.cycle(addiction))
                .is_some();
            if !active {
                tell_mob(
                    world,
                    mob,
                    (
                        Span::Danger,
                        "Вы думали, что завязали, но чувствуете, что возвращаетесь к вредным привычкам...",
                    ),
                );
                crate::addictions::add_points(
                    world,
                    mob,
                    addiction,
                    crate::addictions::MAX_ADDICTION_POINTS,
                );
            }
        }
    }
    let Some(favorite) = state.favorite else {
        return;
    };
    match quirk {
        Quirk::Smoker => {
            let mask = world
                .get::<Inventory>(mob)
                .and_then(|i| i.mask)
                .and_then(|m| world.get::<DmItem>(m))
                .map(|dm| dm.path.clone());
            let Some(mask) = mask else {
                return;
            };
            if !ss13_mapload::fidelity::is_under(&mask, "/obj/item/cigarette") {
                return;
            }
            if ss13_mapload::fidelity::is_under(&mask, &favorite) {
                mood::clear_event(world, mob, "wrong_cigs");
            } else {
                mood::add_event(world, mob, "wrong_cigs", &WRONG_BRAND_EVENT);
            }
        }
        Quirk::Alcoholic => {
            let (mut favourite, mut other) = (false, false);
            if let (Some(chem), Some(reagents)) = (
                world.get_resource::<crate::chem::Chem>(),
                world.get::<crate::chem::MobReagents>(mob),
            ) {
                for r in reagents
                    .blood
                    .reagents
                    .iter()
                    .chain(&reagents.stomach.reagents)
                {
                    let path = &chem.0.def(r.id).path;
                    if !ss13_mapload::fidelity::is_under(path, ETHANOL) {
                        continue;
                    }
                    if ss13_mapload::fidelity::is_under(path, &favorite) {
                        favourite = true;
                    } else {
                        other = true;
                    }
                }
            }
            if favourite {
                mood::clear_event(world, mob, "wrong_alcohol");
            } else if other {
                mood::add_event(world, mob, "wrong_alcohol", &WRONG_BRANDY_EVENT);
            }
        }
        _ => {}
    }
}

/// The taste quirks on a tongue's `(liked, disliked, toxic)` foodtypes: tg
/// `get_liked_foodtypes`/`get_disliked_foodtypes` (`TRAIT_AGEUSIA`, `TRAIT_VEGETARIAN`) and the
/// tongue changes of Deviant Tastes and Ananas Affinity/Aversion. Toxic food stays toxic.
pub fn food_tastes(world: &World, mob: Entity, tastes: (u32, u32, u32)) -> (u32, u32, u32) {
    let Some(quirks) = world.get::<Quirks>(mob) else {
        return tastes;
    };
    let (mut liked, mut disliked, toxic) = tastes;
    if quirks.has(Quirk::DeviantTastes) {
        std::mem::swap(&mut liked, &mut disliked);
    }
    if quirks.has(Quirk::PineappleLiker) {
        liked |= PINEAPPLE;
    }
    if quirks.has(Quirk::PineappleHater) {
        disliked |= PINEAPPLE;
    }
    if quirks.has(Quirk::Ageusia) {
        return (0, 0, toxic);
    }
    if quirks.has(Quirk::Vegetarian) {
        liked &= !MEAT;
        disliked |= MEAT;
    }
    (liked, disliked, toxic)
}

/// `/datum/quirk/csl/add` and `remove`: Galactic Common only partly understood (90%, the
/// default strength), Galactic Uncommon for a mob left with no language at all.
fn csl(world: &mut World, mob: Entity, add: bool) {
    use ss13_chat::language::{ALL, LanguageId, LanguageSource, Preset, UNDERSTOOD};
    let species_speaks_common = Preset::of_path(crate::species::def_of(world, mob).languages)
        .is_some_and(|p| p.spoken.contains(&LanguageId::Common));
    ss13_chat::language::edit_holder(world, mob, |h| {
        if add {
            h.remove(
                LanguageId::Common,
                UNDERSTOOD,
                Some(LanguageSource::Species),
            );
            h.remove(LanguageId::Common, UNDERSTOOD, Some(LanguageSource::Atom));
            if h.understood_languages().is_empty() {
                h.grant(LanguageId::Uncommon, ALL, LanguageSource::Quirk);
            }
            h.grant_partial(LanguageId::Common, 90, LanguageSource::Quirk);
        } else {
            h.remove_partial(LanguageId::Common, LanguageSource::Quirk);
            h.remove(LanguageId::Uncommon, ALL, Some(LanguageSource::Quirk));
            // Only a species that speaks Common understands it again.
            if species_speaks_common {
                h.grant(LanguageId::Common, UNDERSTOOD, LanguageSource::Species);
            }
        }
    });
}

/// CSL's `reremove_common` on `COMSIG_SPECIES_GAIN` (`crate::species::set_species`).
pub fn reremove_common(world: &mut World, mob: Entity) {
    use ss13_chat::language::{LanguageId, LanguageSource, UNDERSTOOD};
    if !has(world, mob, Quirk::Csl) {
        return;
    }
    ss13_chat::language::edit_holder(world, mob, |h| {
        h.remove(
            LanguageId::Common,
            UNDERSTOOD,
            Some(LanguageSource::Species),
        );
    });
}

/// `/datum/quirk/foreigner/add` and `remove`: Galactic Common blocked; a plain human learns
/// Galactic Uncommon instead.
// The removal takes back only the quirk's Uncommon (tg removes it from every source).
fn foreigner(world: &mut World, mob: Entity, add: bool) {
    use ss13_chat::language::{ALL, LanguageId, LanguageSource};
    let human = crate::species::species_of(world, mob) == crate::species::SpeciesId::Human;
    ss13_chat::language::edit_holder(world, mob, |h| {
        if add {
            h.block(LanguageId::Common, ALL, LanguageSource::Quirk);
            if human {
                h.grant(LanguageId::Uncommon, ALL, LanguageSource::Quirk);
            }
        } else {
            h.unblock(LanguageId::Common, ALL, Some(LanguageSource::Quirk));
            if human {
                h.remove(LanguageId::Uncommon, ALL, Some(LanguageSource::Quirk));
            }
        }
    });
}

/// `/datum/quirk/tin_man/add_unique`: the heart, lungs, liver and stomach the species has
/// become surplus prosthetics (the old organs are deleted), then `post_add`'s warning.
fn tin_man(world: &mut World, mob: Entity) {
    for (slot, path, skip) in TIN_MAN_ORGANS {
        if crate::species::has_trait(world, mob, skip) {
            continue;
        }
        let Some(new) = organs::spawn_organ(world, path) else {
            continue;
        };
        // `DELETE_IF_REPLACED`.
        if let Some(old) = organs::organ(world, mob, slot) {
            organs::remove(world, old, true);
            world.despawn(old);
        }
        if !organs::insert(world, new, mob) {
            world.despawn(new);
        }
    }
    tell_mob(
        world,
        mob,
        (
            Span::BoldWarning,
            "Большинство ваших внутренних органов заменены на дешевые протезы. Они хрупкие и \
             легко разлетаются на части под давлением. Кроме того, любое ЭМИ воздействие \
             заставит их полностью прекратить работу.",
        ),
    );
}

/// Body Purist's `check_cybernetics`/`update_mood`: −10 mood per robotic limb or organ.
// Cost: 1 Hz per Body Purist, a pass over its organs (about ten).
fn update_cybernetics(world: &mut World, mob: Entity) {
    let limbs = world
        .get::<crate::medical::wounds::Limbs>(mob)
        .map_or(0, |l| {
            l.parts.iter().filter(|p| p.robotic && !p.missing).count()
        });
    let organ_count = world.get::<organs::Organs>(mob).map_or(0, |owned| {
        owned
            .iter()
            .filter_map(|o| world.get::<organs::Organ>(o))
            .filter(|o| o.robotic)
            .count()
    });
    let level = (limbs + organ_count) as u32;
    if world.get::<Cybernetics>(mob) == Some(&Cybernetics(level)) {
        return;
    }
    world.entity_mut(mob).insert(Cybernetics(level));
    mood::clear_event(world, mob, "body_purist");
    if level == 0 {
        return;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    if let Some(mut mood) = world.get_mut::<Mood>(mob) {
        let mut event = ActiveMood::new("body_purist", &BODY_PURIST_EVENT);
        event.mood_change = -(level as f32) * 10.0;
        mood.add_instance(now, event);
    }
}

fn head_covered(world: &World, mob: Entity) -> bool {
    world
        .get::<Inventory>(mob)
        .is_some_and(|i| i.head.is_some())
}

/// Smooth-Headed's `equip_hat`/`unequip_hat`: taking the headwear off is a `bald` moodlet,
/// putting one on clears it.
// shortcut: no wigs (`confident_mane`).
fn bald_head(world: &mut World, mob: Entity) {
    let covered = head_covered(world, mob);
    {
        let Some(mut bald) = world.get_mut::<BaldHead>(mob) else {
            return;
        };
        if bald.covered == covered {
            return;
        }
        bald.covered = covered;
    }
    if covered {
        mood::clear_event(world, mob, "bad_hair_day");
    } else {
        mood::add_event(world, mob, "bad_hair_day", &BALD_EVENT);
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
        _ => {}
    }
}

fn spt_prob(world: &mut World, percent: f32, secs: f32) -> bool {
    let mut rng = world.get_resource_or_init::<CombatRng>();
    crate::medical::wounds::spt_prob(&mut rng, percent, secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|n| (*n).to_owned()).collect()
    }

    #[test]
    fn every_quirk_has_one_definition_and_name() {
        for def in &QUIRKS {
            assert_eq!(def.quirk.def().path, def.path);
            assert_eq!(Quirk::by_name(def.name), Some(def.quirk));
        }
    }

    #[test]
    fn filter_follows_points_blacklist_and_limits() {
        // 2 free points: Friendly (2) alone is fine, Jolly (4) is not.
        let ok = names(&["Friendly"]);
        assert_eq!(filter_invalid(&ok), ok);
        assert_eq!(filter_invalid(&names(&["Jolly"])), Vec::<String>::new());
        // Negative quirks pay for positive ones.
        let paid = names(&["Jolly", "Mute"]);
        assert_eq!(filter_invalid(&paid), paid);
        // Blacklisted: the later one is dropped.
        assert_eq!(
            filter_invalid(&names(&["Blind", "Nearsighted"])),
            names(&["Blind"])
        );
        // Unknown names are dropped.
        assert_eq!(filter_invalid(&names(&["Nonsense"])), Vec::<String>::new());
        // Over budget: positive quirks go from the first until the balance holds.
        assert_eq!(
            filter_invalid(&names(&["Friendly", "Jolly", "Light Drinker"])),
            names(&["Jolly", "Light Drinker"])
        );
    }

    #[test]
    fn new_blacklists_hold() {
        assert_eq!(
            filter_invalid(&names(&["Mute", "Soft-Spoken"])),
            names(&["Mute"])
        );
        assert_eq!(
            filter_invalid(&names(&["Foreigner", "Common Second Language"])),
            names(&["Foreigner"])
        );
        assert_eq!(
            filter_invalid(&names(&["Ananas Affinity", "Ananas Aversion"])),
            names(&["Ananas Affinity"])
        );
    }

    #[test]
    fn taste_quirks_change_the_tongue() {
        let tastes = (
            crate::food::LIKED_FOODTYPES,
            crate::food::DISLIKED_FOODTYPES,
            crate::food::TOXIC_FOODTYPES,
        );
        let mut world = World::new();
        let none = world.spawn_empty().id();
        assert_eq!(food_tastes(&world, none, tastes), tastes);
        let with = |world: &mut World, list: Vec<Quirk>| world.spawn(Quirks(list)).id();
        let vegetarian = with(&mut world, vec![Quirk::Vegetarian]);
        let (liked, disliked, _) = food_tastes(&world, vegetarian, tastes);
        assert_eq!(liked & MEAT, 0);
        assert_ne!(disliked & MEAT, 0);
        let deviant = with(&mut world, vec![Quirk::DeviantTastes]);
        assert_eq!(
            food_tastes(&world, deviant, tastes),
            (tastes.1, tastes.0, tastes.2)
        );
        // Ageusia tastes nothing but toxic food.
        let ageusia = with(&mut world, vec![Quirk::Ageusia, Quirk::PineappleLiker]);
        assert_eq!(food_tastes(&world, ageusia, tastes), (0, 0, tastes.2));
        let hater = with(&mut world, vec![Quirk::PineappleHater]);
        assert_ne!(food_tastes(&world, hater, tastes).1 & PINEAPPLE, 0);
    }

    #[test]
    fn prefs_change_only_to_valid_sets() {
        let mut prefs = QuirkPrefs::default();
        assert_eq!(prefs.balance(), 2);
        assert!(!change_prefs(&mut prefs, "Jolly", true));
        assert!(change_prefs(&mut prefs, "Mute", true));
        assert!(change_prefs(&mut prefs, "Jolly", true));
        assert_eq!(prefs.balance(), 2);
        // Removing Mute would leave Jolly unpaid.
        assert!(!change_prefs(&mut prefs, "Mute", false));
        assert!(change_prefs(&mut prefs, "Jolly", false));
        assert!(!change_prefs(&mut prefs, "Jolly", false));
        assert_eq!(prefs.0, names(&["Mute"]));
    }

    #[test]
    fn addict_quirks_max_their_addiction_and_take_it_back() {
        use crate::addictions::{Addiction, Addictions, MAX_ADDICTION_POINTS};
        let mut world = World::new();
        world.init_resource::<Time>();
        let mob = world.spawn_empty().id();
        assert!(add_quirk(&mut world, mob, Quirk::Smoker, false, true));
        assert!(add_quirk(&mut world, mob, Quirk::Alcoholic, false, true));
        let a = world.get::<Addictions>(mob).unwrap();
        assert_eq!(a.points(Addiction::Nicotine), MAX_ADDICTION_POINTS);
        assert_eq!(a.cycle(Addiction::Nicotine), Some(1.0));
        assert_eq!(a.cycle(Addiction::Alcohol), Some(1.0));
        let addicts = world.get::<AddictQuirks>(mob).unwrap();
        let smoker = addicts.0.iter().find(|s| s.quirk == Quirk::Smoker).unwrap();
        assert!(SMOKER_PACKS.iter().any(|(p, _)| *p == smoker.container));
        // A kicked habit comes back on the next 30 s process.
        crate::addictions::remove_points(&mut world, mob, Addiction::Nicotine, 900.0);
        assert_eq!(
            world
                .get::<Addictions>(mob)
                .unwrap()
                .cycle(Addiction::Nicotine),
            None
        );
        world.entity_mut(mob).insert(Stat::Conscious);
        process(&mut world, mob, Quirk::Smoker, 1.0);
        assert_eq!(
            world
                .get::<Addictions>(mob)
                .unwrap()
                .cycle(Addiction::Nicotine),
            Some(1.0)
        );
        assert!(remove_quirk(&mut world, mob, Quirk::Smoker));
        let a = world.get::<Addictions>(mob).unwrap();
        assert_eq!(a.points(Addiction::Nicotine), 0.0);
        assert_eq!(a.points(Addiction::Alcohol), 0.0);
    }

    #[test]
    fn medicine_allergies_skip_the_blacklist() {
        let medicines = [
            "/datum/reagent/medicine",
            "/datum/reagent/medicine/epinephrine",
            "/datum/reagent/medicine/omnizine/godblood",
            "/datum/reagent/medicine/c2/libital",
            "/datum/reagent/medicine/c2/aiuri",
            "/datum/reagent/medicine/morphine",
            "/datum/reagent/medicine/spaceacillin",
            "/datum/reagent/medicine/inacusiate",
            "/datum/reagent/medicine/oculine",
            "/datum/reagent/medicine/mannitol",
            "/datum/reagent/water",
        ];
        let mut rng = CombatRng::default();
        let picked = pick_medicine_allergies(&medicines, &mut rng);
        assert_eq!(picked.len(), MEDICINE_ALLERGIES);
        for p in &picked {
            assert!(p.starts_with("/datum/reagent/medicine/"));
            assert!(!p.contains("epinephrine") && !p.contains("godblood"));
        }
        let mut unique = picked.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), picked.len());
    }

    #[test]
    fn food_allergy_reacts_only_to_its_foodtype() {
        let mut world = World::new();
        world.init_resource::<Time>();
        let mob = world.spawn_empty().id();
        assert!(add_quirk(&mut world, mob, Quirk::FoodAllergy, false, true));
        let foods = world.get::<Allergies>(mob).unwrap().foods;
        assert_eq!(foods.count_ones(), 1);
        assert!(FOOD_ALLERGIES.iter().any(|(_, bit)| *bit == foods));
        assert!(!food_allergy_reaction(&mut world, mob, !foods));
        assert!(food_allergy_reaction(&mut world, mob, foods));
        assert!(remove_quirk(&mut world, mob, Quirk::FoodAllergy));
        assert!(!food_allergy_reaction(&mut world, mob, foods));
    }

    #[test]
    fn quirk_items_are_runtime_types() {
        let registry = ss13_mapload::runtime::runtime_types();
        let missing: Vec<String> = item_paths()
            .into_iter()
            .filter(|p| !registry.contains(p.as_str()))
            .collect();
        assert!(
            missing.is_empty(),
            "not in ss13_mapload::runtime: {missing:?}"
        );
    }
}
