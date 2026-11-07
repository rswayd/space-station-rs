//! Brain traumas (server), after tg `code/datums/brain_damage` (`brain_trauma.dm`, `mild.dm`,
//! `severe.dm`), the brain's trauma procs (`mob/living/brain/brain_item.dm`: `can_gain_trauma`,
//! `brain_gain_trauma`, `gain_trauma_type`, `cure_trauma_type`, `cure_all_traumas`,
//! `roll_for_brain_trauma`) and the carbon wrappers (`carbon/status_procs.dm`).
//!
//! A trauma lives in the brain ([`Traumas`] on the brain organ) and goes with it: a removed
//! brain keeps them, the body loses their effects, an inserted brain brings them. Every hit of
//! brain damage above [`organs::BRAIN_DAMAGE_MILD`] rolls for a mild trauma (chance = the
//! damage, +1% per point past the threshold) and, from [`organs::BRAIN_DAMAGE_SEVERE`], for a
//! severe or (20%) special one; natural gains stop at the tier limits (3 basic, 2 surgery, ...).
//! A trauma's [`Resilience`] decides what cures it: neurine and brain surgery the basic ones,
//! brain surgery the surgery ones, lobotomy the deep-rooted ones, nothing normal the rest.
//!
//! What the body gets is derived from the traumas ([`refresh`]): speech impairments for
//! `ss13_chat` ([`ss13_chat::speech::Impediments`]), [`Pacifist`] for melee and guns, paralyzed
//! limbs ([`super::wounds::Limb::paralyzed`]: legs count as disabled, a paralyzed hand drops its
//! item and holds nothing). The rest acts every `Life()` ([`life`]).
//!
//! Ported traumas: mild — stuttering, dumbness, speech impediment, concussion, anosognosia
//! (`healthy`), muscle weakness, nervous cough, possessiveness, achromatopsia, hallucinations
//! (`crate::hallucination`, Gibbering; mindbreaker holds them off), phobias
//! ([`super::fear`]); severe — mute, paralysis, pacifism, kleptomania, aphasia
//! (`ss13_chat::language`), cortical blindness (both read by `crate::vision`), narcolepsy (and
//! its permanent kind), monophobia ([`super::fear`]), dyslexia ([`illiterate`], papers), split
//! personality ([`super::split_personality`]); special — imaginary friend
//! ([`super::imaginary_friend`]), obsession ([`super::obsession`], [`gain_obsessed`]); hypnosis
//! ([`gain_hypnosis`]); the other special and the magic ones in [`super::special_traumas`].
//! Random gains pick among all of tg's types
//! ([`TG_POOL`]): an unported pick gives nothing, so each ported trauma comes as often as in tg.
// shortcut: no muscle spasms (no `/datum/status_effect/spasms`), expressive aphasia (needs the
// common words list), mind echo (needs per-hearer speech rewriting), hypnotic stupor and
// triggers (no trance), discoordination (no `TRAIT_DISCOORDINATED_TOOL_USER` gates),
// achromatopsia's `TRAIT_COLORBLIND` (wire colours), psychotic brawling, primal instincts and
// fire-axe adoration; no forced brain damage lines of dumbness (`traumas.json` is not converted), no
// fake health HUD, mood events or `TRAIT_DUMB`; kleptomania ignores the damage cooldown; an
// inserted brain applies its traumas without their gain messages; no head-wound
// (`TRAUMA_RESILIENCE_WOUND`) traumas, no `TRAIT_SPECIAL_TRAUMA_BOOST`; no admin-only phobia
// subtypes or uncapped hallucinations. Add each with the system it needs.

use bevy::prelude::*;
use ss13_mapload::{DmItem, DmWeight};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;

use super::organs::{self, OrganOf, Slot};
use super::wounds::{Limbs, spt_prob};
use crate::combat::CombatRng;
use crate::health::{BodyPosition, Health, Stat};
use crate::inventory::{CarriedBy, Inventory, NoDrop};
use crate::server::PlayerMob;
use crate::status_effects::Incap;
use crate::tools::tell;
use crate::zone::BodyZone;

mod data;
mod gain;
mod life;

pub use data::*;
pub use gain::*;
pub use life::*;

/// `TRAUMA_RESILIENCE_*`: what cures a trauma (a cure of a level takes every lower one too).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Resilience {
    /// Neurine, brain surgery.
    Basic = 1,
    /// Brain surgery.
    Surgery,
    Lobotomy,
    /// Healing the head wound.
    Wound,
    Magic,
    Absolute,
}

impl Resilience {
    /// `TRAUMA_LIMIT_*`: natural gains stop at this many of one tier.
    fn limit(self) -> usize {
        match self {
            Self::Basic | Self::Lobotomy | Self::Magic => 3,
            Self::Surgery | Self::Wound => 2,
            Self::Absolute => usize::MAX,
        }
    }

    /// The scanner's tier word (`get_status_appendix`).
    fn scan_word(self) -> &'static str {
        match self {
            Self::Basic => "Mild",
            Self::Surgery => "Severe",
            Self::Lobotomy => "Deep-rooted",
            Self::Wound => "Fracture-derived",
            Self::Magic => "Persistent",
            Self::Absolute => "Permanent",
        }
    }
}

/// `BRAIN_TRAUMA_MILD`/`SEVERE`/`SPECIAL`/`MAGIC`: the subtype trees random gains pick from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    Mild,
    Severe,
    Special,
    Magic,
}

/// `/datum/brain_trauma/severe/paralysis` `paralysis_type`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Paralysis {
    Full,
    Left,
    Right,
    Arms,
    Legs,
    RArm,
    LArm,
    RLeg,
    LLeg,
}

impl Paralysis {
    pub const ALL: [Self; 9] = [
        Self::Full,
        Self::Left,
        Self::Right,
        Self::Arms,
        Self::Legs,
        Self::RArm,
        Self::LArm,
        Self::RLeg,
        Self::LLeg,
    ];

    /// The limbs of its `paralysis_traits`.
    pub fn zones(self) -> &'static [BodyZone] {
        use BodyZone::{LArm, LLeg, RArm, RLeg};
        match self {
            Self::Full => &[LArm, RArm, LLeg, RLeg],
            Self::Left => &[LArm, LLeg],
            Self::Right => &[RArm, RLeg],
            Self::Arms => &[LArm, RArm],
            Self::Legs => &[LLeg, RLeg],
            Self::RArm => &[RArm],
            Self::LArm => &[LArm],
            Self::RLeg => &[RLeg],
            Self::LLeg => &[LLeg],
        }
    }

    fn subject(self) -> &'static str {
        match self {
            Self::Full => "ваше тело",
            Self::Left => "левую сторону вашего тела",
            Self::Right => "правую сторону вашего тела",
            Self::Arms => "ваши руки",
            Self::Legs => "ваши ноги",
            Self::RArm => "вашу правую руку",
            Self::LArm => "вашу левую руку",
            Self::RLeg => "вашу правую ногу",
            Self::LLeg => "вашу левую ногу",
        }
    }
}

/// The ported trauma types.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Stuttering,
    Dumbness,
    SpeechImpediment,
    Concussion,
    Healthy,
    MuscleWeakness,
    NervousCough,
    Possessive,
    Mute,
    Paralysis(Paralysis),
    Pacifism,
    Kleptomaniac,
    ColorBlindness,
    Blindness,
    Aphasia,
    /// `/datum/brain_trauma/mild/hallucinations`.
    Hallucinations,
    /// `/datum/brain_trauma/mild/phobia` of a phobia (an index into
    /// [`super::phobia_data::PHOBIAS`]).
    Phobia(usize),
    /// `/datum/brain_trauma/severe/narcolepsy` (`permanent`: its `/permanent` subtype).
    Narcolepsy {
        permanent: bool,
    },
    Monophobia,
    Dyslexia,
    /// `/datum/brain_trauma/severe/split_personality` ([`super::split_personality`]).
    SplitPersonality,
    /// `/datum/brain_trauma/special/imaginary_friend` ([`super::imaginary_friend`]).
    ImaginaryFriend,
    /// `/datum/brain_trauma/hypnosis`: the phrase is the trauma's ([`Trauma::phrase`],
    /// [`gain_hypnosis`]).
    Hypnosis,
    /// `/datum/brain_trauma/special/obsessed` ([`super::obsession`], [`gain_obsessed`]).
    Obsessed,
    // `special.dm` and `magic.dm` ([`super::special_traumas`]).
    Godwoken,
    BluespaceProphet,
    QuantumAlignment,
    Tenacity,
    DeathWhispers,
    ExistentialCrisis,
    /// `/datum/brain_trauma/special/beepsky` (never random, hidden from scanners).
    Beepsky,
    /// `/datum/brain_trauma/special/ptsd` (the veteran advisor's).
    Ptsd,
    Lumiphobia,
    Poltergeist,
    Antimagic,
    /// `/datum/brain_trauma/magic/stalker` (`cosmic`: the heretic's star gazer).
    Stalker {
        cosmic: bool,
    },
}

/// Quantum alignment's entangled target.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Link {
    Entity(Entity),
    Turf(TilePos),
}

/// The state of the special and magic traumas ([`super::special_traumas`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Extra {
    /// A cooldown's end (game time): portals, crises, snapbacks, hallucinations, warnings.
    pub next_at: f64,
    /// The end of an episode: death whispers, a crisis, a snapback under way.
    pub until: Option<f64>,
    /// A phantom chasing the owner: its tile and the image its owner sees.
    pub phantom: Option<(TilePos, Option<Entity>)>,
    pub link: Option<Link>,
    /// An existential crisis' veil.
    pub entity: Option<Entity>,
    /// The stalker is within 8 tiles.
    pub close: bool,
}

/// The static part of a trauma type (tg vars).
#[derive(Clone, Copy, Debug)]
pub struct Def {
    pub path: &'static str,
    pub name: &'static str,
    pub scan_desc: &'static str,
    pub gain: (Span, &'static str),
    pub lose: (Span, &'static str),
    pub category: Category,
    pub resilience: Resilience,
}

/// One trauma in a brain.
#[derive(Clone, Debug, PartialEq)]
pub struct Trauma {
    pub kind: Kind,
    pub resilience: Resilience,
    /// Possessiveness: the items held on to and when each is let go (game time).
    possessed: Vec<(Entity, f64)>,
    /// Kleptomania's `steal_cd` (game time).
    steal_at: f64,
    /// Hallucinations: held off by mindbreaker (`TRAIT_RDS_SUPPRESSED`), Gibbering taken away.
    suppressed: bool,
    /// Hypnosis: `hypnotic_phrase`.
    pub phrase: String,
    /// Obsession: the obsession and its counters.
    pub obsession: Option<super::obsession::Obsession>,
    /// The special and magic traumas' state.
    pub extra: Extra,
}

/// A brain's traumas (tg `traumas`), on the brain organ.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct Traumas(pub Vec<Trauma>);

/// tg `TRAIT_PACIFISM` from a trauma: no harmful attacks or lethal shots.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Pacifist;

fn owner(world: &World, brain: Entity) -> Option<Entity> {
    world.get::<OrganOf>(brain).map(|o| o.0)
}

fn traumas(world: &World, brain: Entity) -> &[Trauma] {
    world.get::<Traumas>(brain).map_or(&[], |t| t.0.as_slice())
}

fn tell_owner(world: &mut World, brain: Entity, (span, text): (Span, String)) {
    if text.is_empty() {
        return;
    }
    if let Some(client) = owner(world, brain)
        .and_then(|mob| world.get::<PlayerMob>(mob))
        .map(|p| p.client)
    {
        tell(world, client, span, text);
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod world_tests;
