//! Species (server), after tg `/datum/species` (`human/_species.dm`: `on_species_gain`,
//! `on_species_loss`, `regenerate_organs`, `handle_body_temperature`,
//! `handle_environment_pressure`), `carbon/set_species` (`dna.dm`), the concrete species
//! (`human/species_types/*.dm`) and BandaStation's (`modular_bandastation/species`: skrell,
//! tajaran, vulpkanin, the moth blood type).
//!
//! A human mob's [`Species`] (a mob without one is human) picks a static [`SpeciesDef`]: its
//! inherent traits, organ set, limb sprites, bodypart damage modifiers, temperature and pressure
//! limits, exotic blood, payday modifier, forbidden equipment slots. [`set_species`] moves a
//! living body to another species: organs of the old set are replaced by the new set's, the
//! damage modifiers, blood type and look follow, hunger is reset on gaining `TRAIT_NOHUNGER`
//! and mutations are cleared on gaining `TRAIT_GENELESS`.
//!
//! Consumers read the traits through [`has_trait`]: `crate::breath` (no breath, pressure and
//! temperature resistance, cold blood, the species temperature limits and modifiers, plasma
//! breathing lungs, plasmaman ignition), `crate::health` (bodypart modifiers, toxin love and
//! immunity, no soft crit), `crate::hunger`, `crate::medical::blood` (no blood, exotic blood),
//! `crate::wires` (`siemens_coeff`), `crate::inventory` (`no_equip_flags`), `crate::genetics`
//! (geneless), `crate::human` (limb sprites).
// shortcut: no sprite accessories of species (tails,
// ears, horns, snouts, wings, antennae, frills, markings: their mutant organs are not spawned),
// no species perks or descriptions in the preferences menu, no species-specific procs beyond the
// ones named above (jelly limb regrowth and body swapping, ethereal charge, pod light healing,
// shadow light damage, golem minerals, dullahan heads, vampire blood drinking, mush martial art,
// snail shell, moth wings and cloth eating, skrell head pocket, felinid/tajaran cat behaviour);
// their traits are recorded for the systems that will read them. Exotic blood is the blood
// reagent with the species type, not the type's own reagent (liquid electricity, oil, lube...).

use bevy::prelude::*;
use ss13_shared::chat::Span;

use crate::health::{BodyMods, Health};
use crate::medical::organs::{self, Organ, Slot};
use crate::obj_damage::is_under;

/// `BODYTEMP_NORMAL`, `BODYTEMP_HEAT_DAMAGE_LIMIT`, `BODYTEMP_COLD_DAMAGE_LIMIT`, kelvin.
const BODYTEMP_NORMAL: f64 = 310.15;
const HEAT_LIMIT: f64 = BODYTEMP_NORMAL + 30.0;
const COLD_LIMIT: f64 = BODYTEMP_NORMAL - 40.0;
/// `BODYTEMP_HEAT_LAVALAND_SAFE` (`LAVALAND_MAX_TEMPERATURE + 5`), `BODYTEMP_COLD_ICEBOX_SAFE`
/// (`ICEBOX_MIN_TEMPERATURE - 5`).
const HEAT_LAVALAND_SAFE: f64 = 350.0 + 5.0;
const COLD_ICEBOX_SAFE: f64 = 180.0 - 5.0;
const T0C: f64 = 273.15;
const T20C: f64 = 293.15;
/// `NUTRITION_LEVEL_FED`: what gaining `TRAIT_NOHUNGER` sets nutrition to.
const NUTRITION_LEVEL_FED: f32 = 350.0;

/// The species a human mob can be (`SPECIES_*` ids). Zombies and nightmares are antagonist
/// species (`docs/parity/antagonists`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SpeciesId {
    #[default]
    Human,
    Felinid,
    Cerulean,
    Vampire,
    Lizard,
    Ashwalker,
    Silverscale,
    Moth,
    Ethereal,
    Lustrous,
    Plasmaman,
    Skrell,
    Tajaran,
    Vulpkanin,
    Jelly,
    Slime,
    Luminescent,
    Stargazer,
    Fly,
    Pod,
    Mush,
    Snail,
    Skeleton,
    Shadow,
    Golem,
    Android,
    Abductor,
    Dullahan,
    Spirit,
    Ghost,
    Monkey,
    /// `SPECIES_NIGHTMARE` (`crate::antag::nightmare`).
    Nightmare,
    /// `SPECIES_ZOMBIE` (`crate::antag::zombie`).
    Zombie,
}

/// The mob's species (tg `dna.species`). A human mob without it is [`SpeciesId::Human`].
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Species(pub SpeciesId);

/// The tg traits species carry (`inherent_traits`, plus organ traits of [`organ_traits`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Trait {
    AbductorHud,
    AdvancedToolUser,
    Agender,
    BloodClans,
    BrainlessCarbon,
    CanStrip,
    CatlikeInstinct,
    ChunkyfingersIgnoreBaton,
    ClothEater,
    ColdBlooded,
    EasyDismember,
    FakeDeath,
    FixedMutantColors,
    Geneless,
    GhostlyMob,
    HardlyWounded,
    HatedByDogs,
    Holy,
    LavaImmune,
    LesserHumanoid,
    LimbAttachment,
    Literate,
    LiverlessMetabolism,
    MoveFlying,
    MutantColors,
    NeverWounded,
    NoAugments,
    NoBlood,
    NoBloodOverlay,
    NoBreath,
    NoCritDamage,
    NoDismember,
    NoDnaCopy,
    NoDrown,
    NoFat,
    NoFire,
    NoFlash,
    NoFloatingAnim,
    NoHunger,
    NoMirrorReflection,
    NoPlasmaTransform,
    NoSlipAll,
    NoSoftcrit,
    NoUnderwear,
    OverdoseImmune,
    PierceImmune,
    PlantSafe,
    RadImmune,
    RequiredAdvHealthScanner,
    ResistCold,
    ResistHeat,
    ResistHighPressure,
    ResistLowPressure,
    SeeBlessedTiles,
    Simian,
    SnowstormImmune,
    Swimmer,
    ToxImmune,
    ToxinLover,
    UnholyBaneable,
    Unhuskable,
    UsesSkintones,
    VirusImmune,
    WineTaster,
    XenoImmune,
}

/// `MOB_*` biotypes (`code/__DEFINES/mobs.dm`).
pub mod biotype {
    pub const ORGANIC: u16 = 1 << 0;
    pub const MINERAL: u16 = 1 << 1;
    pub const ROBOTIC: u16 = 1 << 2;
    pub const UNDEAD: u16 = 1 << 3;
    pub const HUMANOID: u16 = 1 << 4;
    pub const BUG: u16 = 1 << 5;
    pub const REPTILE: u16 = 1 << 8;
    pub const SPIRIT: u16 = 1 << 9;
    pub const PLANT: u16 = 1 << 10;
    pub const SLIME: u16 = 1 << 11;
    pub const SKELETAL: u16 = 1 << 15;
}
use biotype::*;

/// `ITEM_SLOT_*` bits of `no_equip_flags` (the same bits as `crate::inventory::slot_bit`).
pub mod slot {
    pub const OCLOTHING: u32 = 1 << 0;
    pub const ICLOTHING: u32 = 1 << 1;
    pub const GLOVES: u32 = 1 << 2;
    pub const MASK: u32 = 1 << 5;
    pub const FEET: u32 = 1 << 7;
    pub const SUITSTORE: u32 = 1 << 13;
}

/// The organ types of a species (`mutantbrain`, `mutantheart`, ...); `None`: the species has
/// none (`mutantheart = null`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OrganSet {
    pub brain: Option<&'static str>,
    pub heart: Option<&'static str>,
    pub lungs: Option<&'static str>,
    pub eyes: Option<&'static str>,
    pub ears: Option<&'static str>,
    pub tongue: Option<&'static str>,
    pub liver: Option<&'static str>,
    pub stomach: Option<&'static str>,
    pub appendix: Option<&'static str>,
}

impl OrganSet {
    /// The organ type of `slot`.
    pub fn get(&self, slot: Slot) -> Option<&'static str> {
        match slot {
            Slot::Brain => self.brain,
            Slot::Heart => self.heart,
            Slot::Lungs => self.lungs,
            Slot::Eyes => self.eyes,
            Slot::Ears => self.ears,
            Slot::Tongue => self.tongue,
            Slot::Liver => self.liver,
            Slot::Stomach => self.stomach,
            Slot::Appendix => self.appendix,
            Slot::Zombie => None,
        }
    }
}

/// `/datum/species` defaults.
const HUMAN_ORGANS: OrganSet = OrganSet {
    brain: Some("/obj/item/organ/brain"),
    heart: Some("/obj/item/organ/heart"),
    lungs: Some("/obj/item/organ/lungs"),
    eyes: Some("/obj/item/organ/eyes"),
    ears: Some("/obj/item/organ/ears"),
    tongue: Some("/obj/item/organ/tongue"),
    liver: Some("/obj/item/organ/liver"),
    stomach: Some("/obj/item/organ/stomach"),
    appendix: Some("/obj/item/organ/appendix"),
};

/// How a species' limbs are colored (`should_draw_greyscale` and the color source).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LimbColor {
    /// `TRAIT_USES_SKINTONES`: the skin tone.
    Skin,
    /// `TRAIT_MUTANT_COLORS`: the character's mutant color.
    Mutant,
    /// `fixed_mut_color` (or a color the species sets on gain).
    Fixed([u8; 3]),
    /// Not greyscale: drawn as is.
    Static,
}

/// The bodypart sprites of a species (its `bodypart_overrides`' `limb_id`, icon, `is_dimorphic`
/// and `head_flags`): `{id}_{zone}` states, `_m`/`_f` for dimorphic parts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limbs {
    pub dmi: &'static str,
    pub id: &'static str,
    pub color: LimbColor,
    pub head_dimorphic: bool,
    pub chest_dimorphic: bool,
    /// `{id}_l_hand`/`{id}_r_hand` exist.
    pub hands: bool,
    /// Legs exist (ghosts have none).
    pub legs: bool,
    /// `DIGITIGRADE_FORCED`: `digitigrade_*_leg` of the same icon.
    pub digitigrade: bool,
    /// `HEAD_HAIR`.
    pub hair: bool,
    /// `HEAD_EYESPRITES`: the eyes organ's `eye_icon_state` in `human_eyes.dmi`.
    pub eyes: Option<&'static str>,
}

const GREYSCALE_DMI: &str = "icons/mob/human/bodyparts_greyscale.dmi";
const STATIC_DMI: &str = "icons/mob/human/bodyparts.dmi";

const HUMAN_LIMBS: Limbs = Limbs {
    dmi: GREYSCALE_DMI,
    id: "human",
    color: LimbColor::Skin,
    head_dimorphic: true,
    chest_dimorphic: true,
    hands: true,
    legs: true,
    digitigrade: false,
    hair: true,
    eyes: Some("eyes"),
};

/// The `.dmi` files species limbs are drawn from (`crate::human` keeps their icon ids).
pub const LIMB_DMIS: [&str; 13] = [
    GREYSCALE_DMI,
    STATIC_DMI,
    "icons/mob/human/species/lizard/bodyparts.dmi",
    "icons/mob/human/species/moth/bodyparts.dmi",
    "icons/mob/human/species/ethereal/bodyparts.dmi",
    "icons/mob/human/species/plasmaman/bodyparts.dmi",
    "icons/mob/human/species/golems.dmi",
    "icons/mob/human/species/monkey/bodyparts.dmi",
    "icons/mob/human/species/ghost.dmi",
    "icons/mob/augmentation/augments.dmi",
    "icons/bandastation/mob/species/skrell/bodyparts.dmi",
    "icons/bandastation/mob/species/tajaran/bodyparts.dmi",
    "icons/bandastation/mob/species/vulpkanin/bodyparts.dmi",
];

/// A `/datum/species` type: the vars the port reads.
#[derive(Clone, Copy, Debug)]
pub struct SpeciesDef {
    pub id: SpeciesId,
    /// `SPECIES_*`.
    pub key: &'static str,
    pub path: &'static str,
    /// tg `name` (BandaStation's are Russian already).
    pub name: &'static str,
    /// `sexes`: FALSE bodies are never female.
    pub sexes: bool,
    pub traits: &'static [Trait],
    pub biotypes: u16,
    pub organs: OrganSet,
    pub limbs: Limbs,
    /// The `brute_modifier`/`burn_modifier` of its bodyparts.
    pub brute_mod: f32,
    pub burn_mod: f32,
    pub coldmod: f64,
    pub heatmod: f64,
    pub payday_modifier: f32,
    pub siemens_coeff: f32,
    pub bodytemp_normal: f64,
    pub bodytemp_heat_damage_limit: f64,
    pub bodytemp_cold_damage_limit: f64,
    /// `exotic_bloodtype`.
    pub blood: Option<&'static str>,
    /// `no_equip_flags` (`ITEM_SLOT_*`).
    pub no_equip: u32,
    /// `ROUNDSTART_RACES` of BandaStation's config (`game_options.txt`,
    /// `bandastation_config.txt`).
    pub roundstart: bool,
    /// `species_language_holder`.
    pub languages: &'static str,
    pub meat: Option<&'static str>,
    /// `outfit_important_for_life`.
    pub outfit: Option<&'static str>,
}

const HUMAN: SpeciesDef = SpeciesDef {
    id: SpeciesId::Human,
    key: "human",
    path: "/datum/species/human",
    name: "Human",
    sexes: true,
    traits: &[Trait::UsesSkintones],
    biotypes: ORGANIC | HUMANOID,
    organs: HUMAN_ORGANS,
    limbs: HUMAN_LIMBS,
    brute_mod: 1.0,
    burn_mod: 1.0,
    coldmod: 1.0,
    heatmod: 1.0,
    payday_modifier: 1.1,
    siemens_coeff: 1.0,
    bodytemp_normal: BODYTEMP_NORMAL,
    bodytemp_heat_damage_limit: HEAT_LIMIT,
    bodytemp_cold_damage_limit: COLD_LIMIT,
    blood: None,
    no_equip: 0,
    roundstart: true,
    languages: "/datum/language_holder/human_basic",
    meat: Some("/obj/item/food/meat/slab/human"),
    outfit: None,
};

/// `/datum/species` itself: what a species type does not set.
const BASE: SpeciesDef = SpeciesDef {
    payday_modifier: 1.0,
    traits: &[],
    roundstart: false,
    ..HUMAN
};

const LIZARD_TRAITS: &[Trait] = &[Trait::ColdBlooded, Trait::MutantColors];
const LIZARD_LIMBS: Limbs = Limbs {
    dmi: "icons/mob/human/species/lizard/bodyparts.dmi",
    id: "lizard",
    color: LimbColor::Mutant,
    head_dimorphic: false,
    hair: false,
    ..HUMAN_LIMBS
};
const LIZARD: SpeciesDef = SpeciesDef {
    id: SpeciesId::Lizard,
    key: "lizard",
    path: "/datum/species/lizard",
    name: "Lizardperson",
    traits: LIZARD_TRAITS,
    biotypes: ORGANIC | HUMANOID | REPTILE,
    organs: OrganSet {
        brain: Some("/obj/item/organ/brain/lizard"),
        eyes: Some("/obj/item/organ/eyes/lizard"),
        tongue: Some("/obj/item/organ/tongue/lizard"),
        liver: Some("/obj/item/organ/liver/lizard"),
        ..HUMAN_ORGANS
    },
    limbs: LIZARD_LIMBS,
    coldmod: 1.5,
    heatmod: 0.67,
    bodytemp_heat_damage_limit: HEAT_LAVALAND_SAFE,
    bodytemp_cold_damage_limit: COLD_LIMIT - 10.0,
    blood: Some("/datum/blood_type/lizard"),
    roundstart: true,
    languages: "/datum/language_holder/lizard",
    meat: Some("/obj/item/food/meat/slab/human/mutant/lizard"),
    ..BASE
};

const ETHEREAL: SpeciesDef = SpeciesDef {
    id: SpeciesId::Ethereal,
    key: "ethereal",
    path: "/datum/species/ethereal",
    name: "Ethereal",
    sexes: false,
    traits: &[
        Trait::MutantColors,
        Trait::FixedMutantColors,
        Trait::Agender,
    ],
    organs: OrganSet {
        heart: Some("/obj/item/organ/heart/ethereal"),
        lungs: Some("/obj/item/organ/lungs/ethereal"),
        tongue: Some("/obj/item/organ/tongue/ethereal"),
        stomach: Some("/obj/item/organ/stomach/ethereal"),
        ..HUMAN_ORGANS
    },
    limbs: Limbs {
        dmi: "icons/mob/human/species/ethereal/bodyparts.dmi",
        id: "ethereal",
        color: LimbColor::Mutant,
        head_dimorphic: false,
        chest_dimorphic: false,
        ..HUMAN_LIMBS
    },
    brute_mod: 1.25,
    siemens_coeff: 0.5,
    bodytemp_normal: BODYTEMP_NORMAL + 50.0,
    // `FIRE_MINIMUM_TEMPERATURE_TO_SPREAD`, `T20C - 10`.
    bodytemp_heat_damage_limit: 150.0 + T0C,
    bodytemp_cold_damage_limit: T20C - 10.0,
    blood: Some("/datum/blood_type/ethereal"),
    roundstart: true,
    languages: "/datum/language_holder/ethereal",
    meat: Some("/obj/item/food/meat/slab/human/mutant/ethereal"),
    ..BASE
};

const JELLY_TRAITS: &[Trait] = &[Trait::MutantColors, Trait::ToxinLover];
const JELLY: SpeciesDef = SpeciesDef {
    id: SpeciesId::Jelly,
    key: "jelly",
    path: "/datum/species/jelly",
    name: "Jellyperson",
    traits: JELLY_TRAITS,
    biotypes: ORGANIC | HUMANOID | SLIME,
    organs: OrganSet {
        heart: None,
        lungs: Some("/obj/item/organ/lungs/slime"),
        eyes: Some("/obj/item/organ/eyes/jelly"),
        tongue: Some("/obj/item/organ/tongue/jelly"),
        ..HUMAN_ORGANS
    },
    limbs: Limbs {
        id: "jelly",
        color: LimbColor::Mutant,
        eyes: Some("jelleyes"),
        ..HUMAN_LIMBS
    },
    burn_mod: 0.5,
    coldmod: 6.0,
    heatmod: 0.5,
    blood: Some("/datum/blood_type/slime"),
    languages: "/datum/language_holder/jelly",
    meat: Some("/obj/item/food/meat/slab/human/mutant/slime"),
    ..BASE
};

const SPIRIT_TRAITS: &[Trait] = &[
    Trait::Geneless,
    Trait::NeverWounded,
    Trait::NoBlood,
    Trait::NoDismember,
    Trait::NoDnaCopy,
    Trait::NoPlasmaTransform,
    Trait::NoUnderwear,
    Trait::Unhuskable,
    Trait::NoFloatingAnim,
    Trait::MoveFlying,
    Trait::GhostlyMob,
];
const SPIRIT: SpeciesDef = SpeciesDef {
    id: SpeciesId::Spirit,
    key: "spirit",
    path: "/datum/species/spirit",
    name: "Spirit",
    sexes: false,
    traits: SPIRIT_TRAITS,
    biotypes: biotype::SPIRIT | UNDEAD,
    organs: OrganSet {
        brain: Some("/obj/item/organ/brain/ghost"),
        heart: None,
        lungs: Some("/obj/item/organ/lungs/ghost"),
        eyes: Some("/obj/item/organ/eyes/ghost"),
        ears: Some("/obj/item/organ/ears/ghost"),
        tongue: Some("/obj/item/organ/tongue/ghost"),
        liver: Some("/obj/item/organ/liver/ghost"),
        stomach: Some("/obj/item/organ/stomach/ghost"),
        appendix: None,
    },
    limbs: Limbs {
        dmi: "icons/mob/human/species/ghost.dmi",
        id: "ghost",
        color: LimbColor::Static,
        head_dimorphic: false,
        chest_dimorphic: false,
        legs: false,
        ..HUMAN_LIMBS
    },
    no_equip: slot::FEET,
    meat: Some("/obj/item/ectoplasm"),
    ..BASE
};

/// Every species, in [`SpeciesId`] order.
pub static SPECIES: [SpeciesDef; 33] = [
    HUMAN,
    SpeciesDef {
        id: SpeciesId::Felinid,
        key: "felinid",
        path: "/datum/species/human/felinid",
        name: "Felinid",
        traits: &[Trait::HatedByDogs, Trait::UsesSkintones],
        organs: OrganSet {
            brain: Some("/obj/item/organ/brain/felinid"),
            eyes: Some("/obj/item/organ/eyes/felinid"),
            ears: Some("/obj/item/organ/ears/cat"),
            tongue: Some("/obj/item/organ/tongue/cat"),
            ..HUMAN_ORGANS
        },
        payday_modifier: 1.0,
        languages: "/datum/language_holder/felinid",
        ..HUMAN
    },
    SpeciesDef {
        id: SpeciesId::Cerulean,
        key: "cerulean",
        path: "/datum/species/human/cerulean",
        name: "Cerulean",
        organs: OrganSet {
            lungs: Some("/obj/item/organ/lungs/fish"),
            tongue: Some("/obj/item/organ/tongue/fish"),
            liver: Some("/obj/item/organ/liver/fish"),
            stomach: Some("/obj/item/organ/stomach/fish"),
            ..HUMAN_ORGANS
        },
        payday_modifier: 0.9,
        languages: "/datum/language_holder/cerulean",
        ..HUMAN
    },
    SpeciesDef {
        id: SpeciesId::Vampire,
        key: "vampire",
        path: "/datum/species/human/vampire",
        name: "Vampire",
        traits: &[
            Trait::BloodClans,
            Trait::UsesSkintones,
            Trait::NoMirrorReflection,
            Trait::UnholyBaneable,
        ],
        biotypes: UNDEAD | HUMANOID,
        organs: OrganSet {
            heart: Some("/obj/item/organ/heart/vampire"),
            ..HUMAN_ORGANS
        },
        blood: Some("/datum/blood_type/universal/vampire"),
        roundstart: false,
        ..HUMAN
    },
    LIZARD,
    SpeciesDef {
        id: SpeciesId::Ashwalker,
        key: "ashwalker",
        path: "/datum/species/lizard/ashwalker",
        name: "Ash Walker",
        traits: &[Trait::ColdBlooded, Trait::MutantColors, Trait::VirusImmune],
        organs: OrganSet {
            brain: Some("/obj/item/organ/brain/primitive"),
            lungs: Some("/obj/item/organ/lungs/lavaland"),
            ..LIZARD.organs
        },
        limbs: Limbs {
            digitigrade: true,
            ..LIZARD_LIMBS
        },
        roundstart: false,
        languages: "/datum/language_holder/lizard/ash",
        ..LIZARD
    },
    SpeciesDef {
        id: SpeciesId::Silverscale,
        key: "silverscale",
        path: "/datum/species/lizard/silverscale",
        name: "Silver Scale",
        traits: &[
            Trait::ColdBlooded,
            Trait::Holy,
            Trait::NoBreath,
            Trait::PierceImmune,
            Trait::ResistHighPressure,
            Trait::ResistLowPressure,
            Trait::VirusImmune,
            Trait::WineTaster,
        ],
        organs: OrganSet {
            lungs: None,
            tongue: Some("/obj/item/organ/tongue/lizard/silver"),
            ..LIZARD.organs
        },
        // `on_species_gain`: `features[FEATURE_MUTANT_COLOR] = "#eeeeee"`.
        limbs: Limbs {
            color: LimbColor::Fixed([0xee, 0xee, 0xee]),
            ..LIZARD_LIMBS
        },
        roundstart: false,
        languages: "/datum/language_holder/lizard/silver",
        ..LIZARD
    },
    SpeciesDef {
        id: SpeciesId::Moth,
        key: "moth",
        path: "/datum/species/moth",
        name: "Mothman",
        traits: &[Trait::ClothEater],
        biotypes: ORGANIC | HUMANOID | BUG,
        organs: OrganSet {
            eyes: Some("/obj/item/organ/eyes/moth"),
            tongue: Some("/obj/item/organ/tongue/moth"),
            stomach: Some("/obj/item/organ/stomach/moth"),
            ..HUMAN_ORGANS
        },
        limbs: Limbs {
            dmi: "icons/mob/human/species/moth/bodyparts.dmi",
            id: "moth",
            color: LimbColor::Static,
            head_dimorphic: false,
            hair: false,
            eyes: Some("motheyes"),
            ..HUMAN_LIMBS
        },
        brute_mod: 1.15,
        burn_mod: 1.15,
        // BandaStation `species_type/moth.dm`.
        blood: Some("/datum/blood_type/moth"),
        roundstart: true,
        languages: "/datum/language_holder/moth",
        meat: Some("/obj/item/food/meat/slab/human/mutant/moth"),
        ..BASE
    },
    ETHEREAL,
    SpeciesDef {
        id: SpeciesId::Lustrous,
        key: "lustrous",
        path: "/datum/species/ethereal/lustrous",
        name: "Lustrous",
        traits: &[
            Trait::MutantColors,
            Trait::FixedMutantColors,
            Trait::Agender,
            Trait::NoBreath,
            Trait::ResistHighPressure,
            Trait::ResistLowPressure,
            Trait::VirusImmune,
        ],
        organs: OrganSet {
            brain: Some("/obj/item/organ/brain/lustrous"),
            tongue: Some("/obj/item/organ/tongue/ethereal/lustrous"),
            ..ETHEREAL.organs
        },
        // shortcut: the `lustrous_head` sprite; the head is drawn as an ethereal's.
        limbs: Limbs {
            hair: false,
            eyes: None,
            ..ETHEREAL.limbs
        },
        roundstart: false,
        ..ETHEREAL
    },
    SpeciesDef {
        id: SpeciesId::Plasmaman,
        key: "plasmaman",
        path: "/datum/species/plasmaman",
        name: "Plasmaman",
        sexes: false,
        traits: &[
            Trait::Geneless,
            Trait::HardlyWounded,
            Trait::NoBlood,
            Trait::NoDnaCopy,
            Trait::NoPlasmaTransform,
            Trait::RadImmune,
            Trait::ResistCold,
            Trait::Unhuskable,
        ],
        biotypes: HUMANOID | MINERAL | SKELETAL,
        organs: OrganSet {
            heart: None,
            lungs: Some("/obj/item/organ/lungs/plasmaman"),
            tongue: Some("/obj/item/organ/tongue/bone/plasmaman"),
            liver: Some("/obj/item/organ/liver/bone/plasmaman"),
            stomach: Some("/obj/item/organ/stomach/bone/plasmaman"),
            appendix: None,
            ..HUMAN_ORGANS
        },
        limbs: Limbs {
            dmi: "icons/mob/human/species/plasmaman/bodyparts.dmi",
            id: "plasmaman",
            color: LimbColor::Static,
            head_dimorphic: false,
            chest_dimorphic: false,
            hair: false,
            ..HUMAN_LIMBS
        },
        heatmod: 1.5,
        bodytemp_normal: BODYTEMP_NORMAL - 40.0,
        bodytemp_heat_damage_limit: HEAT_LIMIT - 20.0,
        bodytemp_cold_damage_limit: COLD_LIMIT - 50.0,
        roundstart: true,
        languages: "/datum/language_holder/skeleton",
        meat: Some("/obj/item/stack/sheet/mineral/plasma"),
        outfit: Some("/datum/outfit/plasmaman"),
        ..BASE
    },
    SpeciesDef {
        id: SpeciesId::Skrell,
        key: "skrell",
        path: "/datum/species/skrell",
        name: "Скрелл",
        traits: &[Trait::MutantColors, Trait::NoDrown, Trait::Swimmer],
        organs: OrganSet {
            brain: Some("/obj/item/organ/brain/skrell"),
            heart: Some("/obj/item/organ/heart/skrell"),
            lungs: Some("/obj/item/organ/lungs/skrell"),
            eyes: Some("/obj/item/organ/eyes/skrell"),
            tongue: Some("/obj/item/organ/tongue/skrell"),
            liver: Some("/obj/item/organ/liver/skrell"),
            stomach: Some("/obj/item/organ/stomach/skrell"),
            ..HUMAN_ORGANS
        },
        limbs: Limbs {
            dmi: "icons/bandastation/mob/species/skrell/bodyparts.dmi",
            id: "skrell",
            color: LimbColor::Mutant,
            head_dimorphic: false,
            chest_dimorphic: false,
            hair: false,
            eyes: Some("skrell_eyes"),
            ..HUMAN_LIMBS
        },
        payday_modifier: 1.5,
        blood: Some("/datum/blood_type/skrell"),
        roundstart: true,
        languages: "/datum/language_holder/skrell",
        ..BASE
    },
    SpeciesDef {
        id: SpeciesId::Tajaran,
        key: "tajaran",
        path: "/datum/species/tajaran",
        name: "Таяран",
        traits: &[
            Trait::MutantColors,
            Trait::CatlikeInstinct,
            Trait::HatedByDogs,
        ],
        organs: OrganSet {
            brain: Some("/obj/item/organ/brain/tajaran"),
            heart: Some("/obj/item/organ/heart/tajaran"),
            lungs: Some("/obj/item/organ/lungs/tajaran"),
            eyes: Some("/obj/item/organ/eyes/tajaran"),
            ears: Some("/obj/item/organ/ears/tajaran"),
            tongue: Some("/obj/item/organ/tongue/tajaran"),
            liver: Some("/obj/item/organ/liver/tajaran"),
            stomach: Some("/obj/item/organ/stomach/tajaran"),
            ..HUMAN_ORGANS
        },
        limbs: Limbs {
            dmi: "icons/bandastation/mob/species/tajaran/bodyparts.dmi",
            id: "tajaran",
            color: LimbColor::Mutant,
            head_dimorphic: false,
            digitigrade: true,
            ..HUMAN_LIMBS
        },
        coldmod: 0.7,
        heatmod: 1.3,
        payday_modifier: 0.9,
        bodytemp_heat_damage_limit: HEAT_LIMIT + 15.0,
        bodytemp_cold_damage_limit: COLD_LIMIT - 30.0,
        roundstart: true,
        languages: "/datum/language_holder/tajaran",
        ..BASE
    },
    SpeciesDef {
        id: SpeciesId::Vulpkanin,
        key: "vulpkanin",
        path: "/datum/species/vulpkanin",
        name: "Вульпканин",
        traits: &[Trait::MutantColors],
        organs: OrganSet {
            brain: Some("/obj/item/organ/brain/vulpkanin"),
            heart: Some("/obj/item/organ/heart/vulpkanin"),
            lungs: Some("/obj/item/organ/lungs/vulpkanin"),
            eyes: Some("/obj/item/organ/eyes/vulpkanin"),
            ears: Some("/obj/item/organ/ears/vulpkanin"),
            tongue: Some("/obj/item/organ/tongue/vulpkanin"),
            liver: Some("/obj/item/organ/liver/vulpkanin"),
            stomach: Some("/obj/item/organ/stomach/vulpkanin"),
            ..HUMAN_ORGANS
        },
        limbs: Limbs {
            dmi: "icons/bandastation/mob/species/vulpkanin/bodyparts.dmi",
            id: "vulpkanin",
            color: LimbColor::Mutant,
            head_dimorphic: false,
            digitigrade: true,
            ..HUMAN_LIMBS
        },
        payday_modifier: 0.8,
        bodytemp_heat_damage_limit: HEAT_LIMIT + 30.0,
        bodytemp_cold_damage_limit: COLD_LIMIT - 60.0,
        roundstart: true,
        languages: "/datum/language_holder/vulpkanin",
        ..BASE
    },
    JELLY,
    SpeciesDef {
        id: SpeciesId::Slime,
        key: "slime",
        path: "/datum/species/jelly/slime",
        name: "Slimeperson",
        organs: OrganSet {
            eyes: Some("/obj/item/organ/eyes"),
            ..JELLY.organs
        },
        limbs: Limbs {
            id: "slime",
            head_dimorphic: false,
            eyes: Some("eyes"),
            ..JELLY.limbs
        },
        ..JELLY
    },
    SpeciesDef {
        id: SpeciesId::Luminescent,
        key: "luminescent",
        path: "/datum/species/jelly/luminescent",
        name: "Luminescent",
        organs: OrganSet {
            eyes: Some("/obj/item/organ/eyes"),
            ..JELLY.organs
        },
        limbs: Limbs {
            id: "luminescent",
            eyes: Some("eyes"),
            ..JELLY.limbs
        },
        ..JELLY
    },
    SpeciesDef {
        id: SpeciesId::Stargazer,
        key: "stargazer",
        path: "/datum/species/jelly/stargazer",
        name: "Stargazer",
        limbs: Limbs {
            id: "stargazer",
            ..JELLY.limbs
        },
        ..JELLY
    },
    SpeciesDef {
        id: SpeciesId::Fly,
        key: "fly",
        path: "/datum/species/fly",
        name: "Flyperson",
        biotypes: ORGANIC | HUMANOID | BUG,
        organs: OrganSet {
            heart: Some("/obj/item/organ/heart/fly"),
            lungs: Some("/obj/item/organ/lungs/fly"),
            eyes: Some("/obj/item/organ/eyes/fly"),
            tongue: Some("/obj/item/organ/tongue/fly"),
            liver: Some("/obj/item/organ/liver/fly"),
            stomach: Some("/obj/item/organ/stomach/fly"),
            appendix: Some("/obj/item/organ/appendix/fly"),
            ..HUMAN_ORGANS
        },
        limbs: Limbs {
            dmi: STATIC_DMI,
            id: "fly",
            color: LimbColor::Static,
            head_dimorphic: false,
            hair: false,
            eyes: Some("flyeyes"),
            ..HUMAN_LIMBS
        },
        languages: "/datum/language_holder/fly",
        meat: Some("/obj/item/food/meat/slab/human/mutant/fly"),
        ..BASE
    },
    SpeciesDef {
        id: SpeciesId::Pod,
        key: "pod",
        path: "/datum/species/pod",
        name: "Podperson",
        traits: &[
            Trait::MutantColors,
            Trait::PlantSafe,
            Trait::RequiredAdvHealthScanner,
        ],
        biotypes: ORGANIC | HUMANOID | PLANT,
        organs: OrganSet {
            brain: Some("/obj/item/organ/brain/pod"),
            heart: Some("/obj/item/organ/heart/pod"),
            lungs: Some("/obj/item/organ/lungs/pod"),
            eyes: Some("/obj/item/organ/eyes/pod"),
            ears: Some("/obj/item/organ/ears/pod"),
            tongue: Some("/obj/item/organ/tongue/pod"),
            liver: Some("/obj/item/organ/liver/pod"),
            stomach: Some("/obj/item/organ/stomach/pod"),
            appendix: Some("/obj/item/organ/appendix/pod"),
        },
        limbs: Limbs {
            id: "pod",
            color: LimbColor::Mutant,
            hair: false,
            ..HUMAN_LIMBS
        },
        burn_mod: 1.25,
        heatmod: 1.5,
        blood: Some("/datum/blood_type/water"),
        languages: "/datum/language_holder/plant",
        meat: Some("/obj/item/food/meat/slab/human/mutant/plant"),
        ..BASE
    },
    SpeciesDef {
        id: SpeciesId::Mush,
        key: "mush",
        path: "/datum/species/mush",
        name: "Mushroomperson",
        traits: &[
            Trait::MutantColors,
            Trait::NoBreath,
            Trait::NoFlash,
            Trait::NoUnderwear,
        ],
        organs: OrganSet {
            lungs: None,
            eyes: Some("/obj/item/organ/eyes/night_vision/mushroom"),
            tongue: Some("/obj/item/organ/tongue/mush"),
            ..HUMAN_ORGANS
        },
        limbs: Limbs {
            id: "mush",
            color: LimbColor::Fixed([0xdb, 0xbf, 0x92]),
            hands: false,
            hair: false,
            eyes: None,
            ..HUMAN_LIMBS
        },
        burn_mod: 1.25,
        heatmod: 1.5,
        no_equip: slot::MASK | slot::OCLOTHING | slot::GLOVES | slot::FEET | slot::ICLOTHING,
        languages: "/datum/language_holder/mushroom",
        ..BASE
    },
    SpeciesDef {
        id: SpeciesId::Snail,
        key: "snail",
        path: "/datum/species/snail",
        name: "Snailperson",
        sexes: false,
        traits: &[Trait::MutantColors, Trait::NoUnderwear, Trait::NoSlipAll],
        organs: OrganSet {
            eyes: Some("/obj/item/organ/eyes/snail"),
            tongue: Some("/obj/item/organ/tongue/snail"),
            liver: Some("/obj/item/organ/liver/snail"),
            ..HUMAN_ORGANS
        },
        limbs: Limbs {
            id: "snail",
            color: LimbColor::Mutant,
            head_dimorphic: false,
            chest_dimorphic: false,
            hair: false,
            eyes: Some("snail_eyes"),
            ..HUMAN_LIMBS
        },
        burn_mod: 2.0,
        coldmod: 0.5,
        siemens_coeff: 2.0,
        blood: Some("/datum/blood_type/snail"),
        ..BASE
    },
    SpeciesDef {
        id: SpeciesId::Skeleton,
        key: "skeleton",
        path: "/datum/species/skeleton",
        name: "Spooky Scary Skeleton",
        sexes: false,
        traits: &[
            Trait::EasyDismember,
            Trait::FakeDeath,
            Trait::Geneless,
            Trait::LimbAttachment,
            Trait::NoBlood,
            Trait::NoBreath,
            Trait::NoDnaCopy,
            Trait::NoUnderwear,
            Trait::PierceImmune,
            Trait::RadImmune,
            Trait::ResistCold,
            Trait::ResistHeat,
            Trait::ResistHighPressure,
            Trait::ResistLowPressure,
            Trait::ToxImmune,
            Trait::Unhuskable,
            Trait::XenoImmune,
        ],
        biotypes: UNDEAD | HUMANOID | SKELETAL,
        organs: OrganSet {
            heart: None,
            lungs: None,
            tongue: Some("/obj/item/organ/tongue/bone"),
            liver: Some("/obj/item/organ/liver/bone"),
            stomach: Some("/obj/item/organ/stomach/bone"),
            appendix: None,
            ..HUMAN_ORGANS
        },
        limbs: Limbs {
            dmi: STATIC_DMI,
            id: "skeleton",
            color: LimbColor::Static,
            head_dimorphic: false,
            chest_dimorphic: false,
            hair: false,
            eyes: None,
            ..HUMAN_LIMBS
        },
        languages: "/datum/language_holder/skeleton",
        meat: Some("/obj/item/food/meat/slab/human/mutant/skeleton"),
        ..BASE
    },
    SpeciesDef {
        id: SpeciesId::Shadow,
        key: "shadow",
        path: "/datum/species/shadow",
        name: "Shadow",
        sexes: false,
        traits: &[
            Trait::NoBreath,
            Trait::RadImmune,
            Trait::VirusImmune,
            Trait::NoBlood,
            Trait::NoDismember,
            Trait::NeverWounded,
        ],
        organs: OrganSet {
            brain: Some("/obj/item/organ/brain/shadow"),
            heart: None,
            lungs: None,
            eyes: Some("/obj/item/organ/eyes/shadow"),
            tongue: Some("/obj/item/organ/tongue/shadow"),
            ..HUMAN_ORGANS
        },
        limbs: Limbs {
            dmi: STATIC_DMI,
            id: "shadow",
            color: LimbColor::Static,
            head_dimorphic: false,
            chest_dimorphic: false,
            hair: false,
            eyes: None,
            ..HUMAN_LIMBS
        },
        burn_mod: 1.5,
        languages: "/datum/language_holder/shadowpeople",
        meat: Some("/obj/item/food/meat/slab/human/mutant/shadow"),
        ..BASE
    },
    SpeciesDef {
        id: SpeciesId::Golem,
        key: "golem",
        path: "/datum/species/golem",
        name: "Golem",
        sexes: false,
        traits: &[
            Trait::Geneless,
            Trait::LavaImmune,
            Trait::NeverWounded,
            Trait::NoBlood,
            Trait::NoBreath,
            Trait::NoCritDamage,
            Trait::NoDismember,
            Trait::NoFat,
            Trait::NoFire,
            Trait::NoSoftcrit,
            Trait::NoAugments,
            Trait::NoDnaCopy,
            Trait::NoPlasmaTransform,
            Trait::NoUnderwear,
            Trait::PierceImmune,
            Trait::RadImmune,
            Trait::SnowstormImmune,
            Trait::Unhuskable,
        ],
        biotypes: HUMANOID | MINERAL,
        organs: OrganSet {
            brain: Some("/obj/item/organ/brain/golem"),
            heart: None,
            lungs: None,
            eyes: Some("/obj/item/organ/eyes/golem"),
            tongue: Some("/obj/item/organ/tongue/golem"),
            liver: Some("/obj/item/organ/liver/golem"),
            stomach: Some("/obj/item/organ/stomach/golem"),
            appendix: Some("/obj/item/organ/appendix/golem"),
            ..HUMAN_ORGANS
        },
        limbs: Limbs {
            dmi: "icons/mob/human/species/golems.dmi",
            id: "golem",
            color: LimbColor::Static,
            head_dimorphic: false,
            chest_dimorphic: false,
            hands: false,
            hair: false,
            eyes: None,
            ..HUMAN_LIMBS
        },
        brute_mod: 0.5,
        burn_mod: 0.5,
        siemens_coeff: 0.01,
        bodytemp_heat_damage_limit: HEAT_LAVALAND_SAFE,
        bodytemp_cold_damage_limit: COLD_ICEBOX_SAFE,
        no_equip: slot::MASK
            | slot::OCLOTHING
            | slot::GLOVES
            | slot::FEET
            | slot::ICLOTHING
            | slot::SUITSTORE,
        languages: "/datum/language_holder/golem",
        meat: Some("/obj/item/food/meat/slab/human/mutant/golem"),
        ..BASE
    },
    SpeciesDef {
        id: SpeciesId::Android,
        key: "android",
        path: "/datum/species/android",
        name: "Android",
        traits: &[
            Trait::Geneless,
            Trait::LimbAttachment,
            Trait::LiverlessMetabolism,
            Trait::NoBlood,
            Trait::NoBreath,
            Trait::NoCritDamage,
            Trait::NoFire,
            Trait::NoHunger,
            Trait::NoDnaCopy,
            Trait::NoPlasmaTransform,
            Trait::NoUnderwear,
            Trait::OverdoseImmune,
            Trait::PierceImmune,
            Trait::RadImmune,
            Trait::ResistCold,
            Trait::ResistHeat,
            Trait::ResistHighPressure,
            Trait::ResistLowPressure,
            Trait::ToxImmune,
        ],
        biotypes: ROBOTIC | HUMANOID,
        organs: OrganSet {
            brain: Some("/obj/item/organ/brain/cybernetic"),
            heart: None,
            lungs: None,
            eyes: Some("/obj/item/organ/eyes/robotic"),
            ears: Some("/obj/item/organ/ears/cybernetic"),
            tongue: Some("/obj/item/organ/tongue/robot"),
            liver: None,
            stomach: None,
            appendix: None,
        },
        limbs: Limbs {
            dmi: "icons/mob/augmentation/augments.dmi",
            id: "robotic",
            color: LimbColor::Static,
            head_dimorphic: false,
            chest_dimorphic: false,
            hair: false,
            ..HUMAN_LIMBS
        },
        // `/obj/item/bodypart/*/robot`.
        brute_mod: 0.8,
        burn_mod: 0.8,
        blood: Some("/datum/blood_type/oil"),
        languages: "/datum/language_holder/synthetic",
        meat: None,
        ..BASE
    },
    SpeciesDef {
        id: SpeciesId::Abductor,
        key: "abductor",
        path: "/datum/species/abductor",
        name: "Abductor",
        sexes: false,
        traits: &[
            Trait::AbductorHud,
            Trait::ChunkyfingersIgnoreBaton,
            Trait::NeverWounded,
            Trait::NoBlood,
            Trait::NoBreath,
            Trait::NoDismember,
            Trait::NoHunger,
            Trait::NoUnderwear,
            Trait::VirusImmune,
        ],
        organs: OrganSet {
            brain: Some("/obj/item/organ/brain/abductor"),
            heart: None,
            lungs: None,
            tongue: Some("/obj/item/organ/tongue/abductor"),
            stomach: None,
            ..HUMAN_ORGANS
        },
        limbs: Limbs {
            dmi: STATIC_DMI,
            id: "abductor",
            color: LimbColor::Static,
            head_dimorphic: false,
            chest_dimorphic: false,
            hair: false,
            eyes: None,
            ..HUMAN_LIMBS
        },
        ..BASE
    },
    SpeciesDef {
        id: SpeciesId::Dullahan,
        key: "dullahan",
        path: "/datum/species/dullahan",
        name: "Dullahan",
        traits: &[
            Trait::NoBreath,
            Trait::NoHunger,
            Trait::UsesSkintones,
            Trait::AdvancedToolUser,
            Trait::Literate,
            Trait::CanStrip,
            Trait::BrainlessCarbon,
        ],
        biotypes: UNDEAD | HUMANOID,
        organs: OrganSet {
            brain: Some("/obj/item/organ/brain/dullahan"),
            lungs: None,
            eyes: Some("/obj/item/organ/eyes/dullahan"),
            ears: Some("/obj/item/organ/ears/dullahan"),
            tongue: Some("/obj/item/organ/tongue/dullahan"),
            stomach: None,
            ..HUMAN_ORGANS
        },
        ..BASE
    },
    SPIRIT,
    SpeciesDef {
        id: SpeciesId::Ghost,
        key: "ghost",
        path: "/datum/species/spirit/ghost",
        name: "Ghost",
        traits: &[
            Trait::Geneless,
            Trait::NeverWounded,
            Trait::NoBlood,
            Trait::NoDismember,
            Trait::NoDnaCopy,
            Trait::NoPlasmaTransform,
            Trait::NoUnderwear,
            Trait::Unhuskable,
            Trait::NoFloatingAnim,
            Trait::MoveFlying,
            Trait::SeeBlessedTiles,
        ],
        ..SPIRIT
    },
    SpeciesDef {
        id: SpeciesId::Monkey,
        key: "monkey",
        path: "/datum/species/monkey",
        name: "Monkey",
        sexes: false,
        traits: &[
            Trait::NoBloodOverlay,
            Trait::NoDnaCopy,
            Trait::NoUnderwear,
            Trait::LesserHumanoid,
            Trait::Simian,
        ],
        organs: OrganSet {
            brain: Some("/obj/item/organ/brain/primate"),
            tongue: Some("/obj/item/organ/tongue/monkey"),
            ..HUMAN_ORGANS
        },
        limbs: Limbs {
            dmi: "icons/mob/human/species/monkey/bodyparts.dmi",
            id: "monkey",
            color: LimbColor::Static,
            head_dimorphic: false,
            chest_dimorphic: false,
            hair: false,
            eyes: None,
            ..HUMAN_LIMBS
        },
        payday_modifier: 1.5,
        no_equip: slot::OCLOTHING | slot::GLOVES | slot::FEET | slot::SUITSTORE,
        languages: "/datum/language_holder/monkey",
        meat: Some("/obj/item/food/meat/slab/monkey"),
        ..BASE
    },
    // `/datum/species/shadow/nightmare`: the shadow's body, its own brain and heart.
    SpeciesDef {
        id: SpeciesId::Nightmare,
        key: "nightmare",
        path: "/datum/species/shadow/nightmare",
        name: "Nightmare",
        sexes: false,
        traits: &[
            Trait::NoUnderwear,
            Trait::ResistCold,
            Trait::NoBreath,
            Trait::ResistHighPressure,
            Trait::ResistLowPressure,
            Trait::RadImmune,
            Trait::VirusImmune,
            Trait::PierceImmune,
            Trait::NoDismember,
            Trait::NoHunger,
            Trait::NoBlood,
            Trait::NoDnaCopy,
            Trait::NeverWounded,
        ],
        organs: OrganSet {
            brain: Some("/obj/item/organ/brain/shadow/nightmare"),
            heart: Some("/obj/item/organ/heart/nightmare"),
            lungs: None,
            eyes: Some("/obj/item/organ/eyes/shadow"),
            tongue: Some("/obj/item/organ/tongue/shadow"),
            ..HUMAN_ORGANS
        },
        limbs: Limbs {
            dmi: STATIC_DMI,
            id: "shadow",
            color: LimbColor::Static,
            head_dimorphic: false,
            chest_dimorphic: false,
            hair: false,
            eyes: None,
            ..HUMAN_LIMBS
        },
        burn_mod: 1.5,
        no_equip: slot::MASK
            | slot::OCLOTHING
            | slot::GLOVES
            | slot::FEET
            | slot::ICLOTHING
            | slot::SUITSTORE,
        languages: "/datum/language_holder/shadowpeople",
        meat: Some("/obj/item/food/meat/slab/human/mutant/shadow"),
        ..BASE
    },
    // `/datum/species/zombie`: the zombie itself is `/datum/status_effect/zombie/uninfected`
    // (`crate::antag::zombie`), given by `set_species`.
    SpeciesDef {
        id: SpeciesId::Zombie,
        key: "zombie",
        path: "/datum/species/zombie",
        name: "Зомби",
        ..BASE
    },
];

/// Organ types that carry traits of their own (`organ_traits`).
const ORGAN_TRAITS: [(&str, &[Trait]); 1] = [
    // "A fresh breakfast of plasma is a great start to any morning."
    ("/obj/item/organ/lungs/plasmaman", &[Trait::NoHunger]),
];

/// The `organ_traits` of an organ type.
pub fn organ_traits(path: &str) -> &'static [Trait] {
    ORGAN_TRAITS
        .iter()
        .find(|(root, _)| is_under(path, root))
        .map_or(&[], |(_, traits)| *traits)
}

impl SpeciesId {
    pub fn def(self) -> &'static SpeciesDef {
        let def = &SPECIES[self as usize];
        debug_assert_eq!(def.id, self);
        def
    }

    /// A `SPECIES_*` id.
    pub fn from_key(key: &str) -> Option<Self> {
        SPECIES.iter().find(|d| d.key == key).map(|d| d.id)
    }

    /// The species players may pick (`get_selectable_species`), in [`SpeciesId`] order.
    pub fn roundstart() -> impl Iterator<Item = Self> {
        SPECIES.iter().filter(|d| d.roundstart).map(|d| d.id)
    }
}

impl SpeciesDef {
    pub fn has(&self, t: Trait) -> bool {
        self.traits.contains(&t)
    }

    /// What its bodyparts and traits do to damage (`crate::health`).
    pub fn body_mods(&self) -> BodyMods {
        BodyMods {
            brute: self.brute_mod,
            burn: self.burn_mod,
            tox_lover: self.has(Trait::ToxinLover),
            tox_immune: self.has(Trait::ToxImmune),
            no_softcrit: self.has(Trait::NoSoftcrit),
            no_hardcrit: false,
            no_death: false,
        }
    }
}

/// The species of `mob` (human without [`Species`]).
pub fn species_of(world: &World, mob: Entity) -> SpeciesId {
    world.get::<Species>(mob).map_or(SpeciesId::Human, |s| s.0)
}

pub fn def_of(world: &World, mob: Entity) -> &'static SpeciesDef {
    species_of(world, mob).def()
}

/// tg `HAS_TRAIT(mob, trait)` for the traits species and their organs give.
pub fn has_trait(world: &World, mob: Entity, t: Trait) -> bool {
    if def_of(world, mob).has(t) || crate::antag::zombie::grants(world, mob, t) {
        return true;
    }
    world.get::<organs::Organs>(mob).is_some_and(|owned| {
        owned
            .iter()
            .filter_map(|o| world.get::<Organ>(o))
            .any(|o| organ_traits(&o.path).contains(&t))
    })
}

/// tg `can_mutate()`: an organic, non-geneless body.
pub fn can_mutate(world: &World, mob: Entity) -> bool {
    def_of(world, mob).biotypes & ORGANIC != 0 && !has_trait(world, mob, Trait::Geneless)
}

/// tg `/datum/species/proc/can_equip`'s `no_equip_flags` check for `slot_bit`.
pub fn can_wear(world: &World, mob: Entity, slot_bit: u32) -> bool {
    def_of(world, mob).no_equip & slot_bit == 0
}

/// tg `human/set_species(new_race)`: `on_species_loss` of the old species, `on_species_gain`
/// of the new one with `regenerate_organs(replace_current = TRUE)`. Nothing happens when the
/// species does not change; a body without [`Species`] (a new one) gets it applied.
pub fn set_species(world: &mut World, mob: Entity, id: SpeciesId) {
    let old = world.get::<Species>(mob).map(|s| s.0);
    if old == Some(id) {
        return;
    }
    let old_def = old.unwrap_or_default().def();
    let def = id.def();
    world.entity_mut(mob).insert(Species(id));
    update_languages(world, mob, old.map(|o| o.def().languages), def.languages);
    // `COMSIG_SPECIES_GAIN` of the Common Second Language quirk.
    crate::quirks::reremove_common(world, mob);
    regenerate_organs(world, mob, true);
    if let Some(mut health) = world.get_mut::<Health>(mob) {
        let mut mods = def.body_mods();
        // The tenacity trauma's traits stay (`crate::medical::traumas::refresh`).
        mods.no_softcrit |= health.mods.no_hardcrit;
        mods.no_hardcrit = health.mods.no_hardcrit;
        if health.mods != mods {
            health.mods = mods;
        }
    }
    // `dna.species.exotic_bloodtype`: the new blood type; a species without one gets a human
    // type back when it had an exotic one.
    {
        use crate::medical::blood::{Blood, BloodType};
        let current = world.get::<Blood>(mob).map(|b| b.blood_type);
        let new = match (current, def.blood.and_then(BloodType::from_path)) {
            (Some(_), Some(exotic)) => Some(exotic),
            (Some(t), None) if !BloodType::HUMAN.contains(&t) => {
                let mut rng = world.get_resource_or_init::<crate::combat::CombatRng>();
                Some(BloodType::random_human(&mut rng))
            }
            _ => None,
        };
        if let Some(new) = new
            && let Some(mut blood) = world.get_mut::<Blood>(mob)
        {
            blood.blood_type = new;
        }
    }
    // `on_nohunger`: gaining `TRAIT_NOHUNGER` leaves the hunger system at "fed".
    if def.has(Trait::NoHunger) && !old_def.has(Trait::NoHunger) {
        if let Some(mut hunger) = world.get_mut::<crate::hunger::Hunger>(mob) {
            hunger.nutrition = NUTRITION_LEVEL_FED;
            hunger.satiety = 0.0;
            hunger.overeat_secs = 0.0;
        }
        if let Some(mut fat) = world.get_mut::<crate::hunger::Fat>(mob) {
            fat.set_if_neq(crate::hunger::Fat(false));
        }
    }
    // `on_geneless_trait_gain`: `dna.remove_all_mutations()`.
    if def.has(Trait::Geneless) && !old_def.has(Trait::Geneless) {
        crate::genetics::remove_all_mutations(world, mob);
    }
    // The body is drawn as the new species (`replace_body`, `update_body`).
    if let Some(mut look) = world.get_mut::<crate::human::HumanLook>(mob) {
        let female = look.female && def.sexes;
        if look.species != id || look.female != female {
            look.species = id;
            look.female = female;
        }
    }
    crate::inventory::refresh_look(world, mob);
    // `on_species_gain` of the zombie, `COMSIG_SPECIES_GAIN` of the zombie status.
    crate::antag::zombie::species_changed(world, mob, old, id);
    if old.is_some() {
        worn_items_fit_body_check(world, mob);
        let name = def.name;
        if let Some(client) = world.get::<crate::server::PlayerMob>(mob).map(|p| p.client) {
            crate::tools::tell(world, client, Span::Notice, format!("Теперь вы — {name}."));
        }
    }
    info!("{mob} is now {:?}", id);
}

/// tg `worn_items_fit_body_check`: what the new species cannot wear falls to the floor.
// shortcut: only the `no_equip_flags` (no `mob_can_equip` per item).
fn worn_items_fit_body_check(world: &mut World, mob: Entity) {
    use crate::inventory::{ALL_SLOTS, Inventory, put_down, slot_bit, unequip};
    let (Some(&pos), Some(inventory)) = (
        world.get::<ss13_shared::TilePos>(mob),
        world.get::<Inventory>(mob).cloned(),
    ) else {
        return;
    };
    for slot in ALL_SLOTS {
        if let Some(item) = inventory.slot(slot)
            && !can_wear(world, mob, slot_bit(slot))
        {
            unequip(world, mob, pos, slot);
            put_down(world, item, pos);
        }
    }
}

/// `on_species_loss` / `on_species_gain`: the old species' language holder is taken back and
/// the new one's granted, both from `LANGUAGE_SPECIES` (a human starts with the empty holder).
fn update_languages(world: &mut World, mob: Entity, old: Option<&str>, new: &str) {
    use ss13_chat::language::{LanguageHolder, LanguageSource, Preset};
    let mut holder = world
        .get::<LanguageHolder>(mob)
        .cloned()
        .unwrap_or_default();
    if let Some(preset) = old.and_then(Preset::of_path) {
        holder.unapply(&preset, LanguageSource::Species);
    }
    if let Some(preset) = Preset::of_path(new) {
        holder.apply(&preset, LanguageSource::Species);
    }
    world.entity_mut(mob).insert(holder);
}

/// `GLOB.uncommon_roundstart_languages`: what the roundstart species understand, Galactic
/// Common out.
pub fn uncommon_roundstart_languages() -> Vec<ss13_chat::language::LanguageId> {
    use ss13_chat::language::{LanguageId, Preset};
    let mut out: Vec<LanguageId> = Vec::new();
    for id in SpeciesId::roundstart() {
        let preset = Preset::of_path(id.def().languages).unwrap_or_default();
        for &l in preset.understood {
            if l != LanguageId::Common && !out.contains(&l) {
                out.push(l);
            }
        }
    }
    out
}

/// tg `regenerate_organs(replace_current)`: every slot gets the species' organ type; an organ
/// of another type is replaced (`replace_current`) and one the species lacks is deleted. Without
/// `replace_current` only missing organs come back. A mob without modelled organs is left alone.
pub fn regenerate_organs(world: &mut World, mob: Entity, replace_current: bool) {
    if world.get::<organs::OrganHost>(mob).is_none() {
        return;
    }
    let set = def_of(world, mob).organs;
    for slot in Slot::ALL {
        // No species' organ: the infection survives a species change.
        if slot == Slot::Zombie {
            continue;
        }
        let current = organs::organ(world, mob, slot);
        let wanted = set.get(slot);
        let current_path = current.and_then(|o| world.get::<Organ>(o).map(|s| s.path.clone()));
        let keep = match (&current_path, wanted) {
            (Some(path), Some(wanted)) => path == wanted || !replace_current,
            (Some(_), None) => !replace_current,
            (None, _) => false,
        };
        if keep {
            continue;
        }
        if let Some(old) = current {
            // `special`: a species change does not kill through a vital organ.
            organs::remove(world, old, true);
            world.despawn(old);
        }
        if let Some(path) = wanted
            && let Some(new) = organs::spawn_organ(world, path)
        {
            organs::insert(world, new, mob);
        }
    }
}

/// `/datum/outfit/plasmaman`'s uniform, gloves, head and tank, in [`equip_species_outfit`]'s
/// slot order.
const PLASMAMAN_ITEMS: [&str; 4] = [
    "/obj/item/clothing/under/plasmaman",
    "/obj/item/clothing/gloves/color/plasmaman",
    "/obj/item/clothing/head/helmet/space/plasmaman",
    // BandaStation: the tank goes to the right pocket (`internals_slot = ITEM_SLOT_RPOCKET`).
    "/obj/item/tank/internals/plasmaman/belt/full",
];

/// tg `pre_equip_species_outfit` of a species with `outfit_important_for_life`: the plasmaman
/// envirosuit, helmet, gloves and plasma tank, which take the job outfit's slots; the tank is
/// opened (`internals_slot`).
// shortcut: the generic `/datum/outfit/plasmaman`, not the job's `plasmaman_outfit` (jobs data
// has no plasmaman outfits); the job's uniform, gloves and head are deleted, as tg's
// `equip_to_slot_or_del` does with them after the species outfit took the slots.
pub fn equip_species_outfit(
    world: &mut World,
    mob: Entity,
    inventory: &mut crate::inventory::Inventory,
) {
    let Some(outfit) = def_of(world, mob).outfit else {
        return;
    };
    if outfit != "/datum/outfit/plasmaman" {
        return;
    }
    let mut tank = None;
    for (i, path) in PLASMAMAN_ITEMS.into_iter().enumerate() {
        let Some(item) = crate::construction::spawn_detached(world, path) else {
            warn!("species outfit: no prototype for {path}");
            continue;
        };
        world.entity_mut(item).insert((
            ss13_net::server::Networked,
            crate::inventory::CarriedBy(mob),
        ));
        let target = match i {
            0 => &mut inventory.uniform,
            1 => &mut inventory.gloves,
            2 => &mut inventory.head,
            _ => &mut inventory.pockets[1],
        };
        if let Some(old) = target.replace(item) {
            world.despawn(old);
        }
        if i == 3 {
            tank = Some(item);
        }
    }
    if let Some(tank) = tank {
        world
            .entity_mut(mob)
            .insert(crate::internals::Internals(Some(tank)));
    }
}

/// `PLASMAMAN_PREVENT_IGNITION` (`clothing_flags`).
const PLASMAMAN_PREVENT_IGNITION: u32 = 1 << 10;
/// `/datum/status_effect/grouped/bodypart_effect/plasma_based`: `fire_stacks_per_second` and
/// `fire_stacks_loss` per plasma limb.
const PLASMA_FIRE_STACKS_PER_SECOND: f32 = 0.0416;
const PLASMA_FIRE_STACKS_LOSS: f32 = 1.66;

/// The plasmaman limbs' `plasma_based` effect for `secs`: unsealed in air with oxygen, the body
/// gains fire stacks per limb and bursts into flame; hyper-noblium puts it out.
// shortcut: runs every `Life()` (2 s) instead of its 1 s tick; no `TRAIT_IGNORE_FIRE_PROTECTION`
// (fire protection is not modelled), no stasis.
pub fn plasma_ignition(
    world: &mut World,
    mob: Entity,
    worn: &crate::breath::Worn,
    environment: &ss13_atmos::GasMixture,
    secs: f64,
) {
    if species_of(world, mob) != SpeciesId::Plasmaman
        || has_trait(world, mob, Trait::NoFire)
        || worn.atmos_sealed(PLASMAMAN_PREVENT_IGNITION, true)
        || environment.total_moles() == 0.0
    {
        return;
    }
    let limbs = world.get::<Health>(mob).map_or(6, |h| {
        crate::zone::BodyZone::ALL
            .into_iter()
            .filter(|z| h.has_part(*z))
            .count()
    }) as f32;
    let secs = secs as f32;
    let moles = |id: ss13_atmos::GasId| environment.moles[usize::from(id.0)];
    let nob = ss13_atmos::GasId::parse("hypernoblium").expect("tg gas");
    if moles(nob) >= 5.0 {
        if crate::fire::on_fire(world, mob) {
            crate::fire::adjust_fire_stacks(world, mob, -PLASMA_FIRE_STACKS_LOSS * secs * limbs);
        }
        return;
    }
    // The threshold that puts fires out.
    if moles(ss13_atmos::GasId::O2) < 1.0 {
        return;
    }
    crate::fire::adjust_fire_stacks(world, mob, PLASMA_FIRE_STACKS_PER_SECOND * secs * limbs);
    if crate::fire::ignite_mob(world, mob)
        && let Some(client) = world.get::<crate::server::PlayerMob>(mob).map(|p| p.client)
    {
        crate::tools::tell(
            world,
            client,
            Span::Danger,
            "Ваше тело вступает в реакцию с атмосферой и вспыхивает!",
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn organs_meat_and_envirosuit_are_runtime_types() {
        let registry = ss13_mapload::runtime::runtime_types();
        let missing: Vec<&str> = SPECIES
            .iter()
            .flat_map(|s| {
                Slot::ALL
                    .into_iter()
                    .filter_map(move |slot| s.organs.get(slot))
                    .chain(s.meat)
            })
            .chain(PLASMAMAN_ITEMS)
            .filter(|p| !registry.contains(p))
            .collect();
        assert!(
            missing.is_empty(),
            "not in ss13_mapload::runtime: {missing:?}"
        );
    }
}
