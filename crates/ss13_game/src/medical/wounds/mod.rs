//! Wounds (server), after tg `code/datums/wounds` (`_wounds.dm`, `_wound_static_data.dm`,
//! `slash.dm`, `pierce.dm`, `bones.dm`, `burns.dm`) and the bodypart side
//! (`code/modules/surgery/bodyparts/wounds.dm` `check_wounding`/`check_woundings_mods`,
//! `_bodyparts.dm` `receive_damage`, `refresh_bleed_rate`, `update_disabled`).
//!
//! - A brute or burn hit of at least [`WOUND_MINIMUM_DAMAGE`] rolls `rand(1, damage^1.4)` (damage
//!   capped at 25), adds the attack's `wound_bonus`, the limb's wounds' `threshold_penalty`, the
//!   disabled-limb penalty and the exposed bonus (or the limb's `wound_resistance` as armor), and
//!   picks among the wounds of the hit's type (blunt, slash, pierce, burn) whose threshold it
//!   reaches, the worst severity winning; a worse wound replaces a milder one of its series.
//! - Slashes and pierces bleed (`blood_flow`, see `crate::medical::blood`): slashes clot over
//!   time (critical ones open up), demote to milder cuts and close; gauze soaks the flow away;
//!   sutures and cauteries take flow off at once. Pierces bleed internally when hit again.
//! - Bones: a dislocation (arms and legs) limps and is reset with a bonesetter; a hairline
//!   fracture and a compound fracture limp, the compound one disables the limb until splinted;
//!   bone gel then surgical tape knit them over minutes; chest fractures bleed internally.
//! - Burns: flesh damage heals with ointment, mesh and gauze; severe and critical burns get
//!   infected without sanitization (toxins, a limb that goes limp, sepsis).
//! - Effects on the body ([`Limbs::limits`]): limping legs slow every step on average, disabled
//!   legs slow by 3 deciseconds each and none left floors the mob.
//! - Dismemberment (`dismemberment.dm` `try_dismember`, `check_wounding`'s outright roll): a hit
//!   on a limb whose skin and bone are both mangled rolls its damage plus half the limb's damage
//!   share (+15 with a critical blunt wound) to come off instead of wounding; a roll over
//!   `WOUND_DISMEMBER_OUTRIGHT_THRESH` takes it off outright by the limb's damage share
//!   (`super::bodyparts::apply_dismember`, tg `/datum/wound/loss`).
// shortcut: no cranial fissure, no eye puncture, no
// bruises, scars, brain traumas from skull fractures, wound status effects, actionspeed
// penalties, grasping wounds, embedded objects, blood sprays or splatters; clothing has no
// `wound` armor (the exposed bonus always applies off the head and chest); `determination` from
// `second_wind` is not added; disinfecting reagents (`on_burn_wound_processing`) do nothing.
// shortcut: limping slows every step by the average (`limp_slowdown × limp_chance`, alternating
// legs) instead of rolling per step: `ss13_world::Mover` has no per-step delay hook.

use bevy::prelude::*;
use ss13_mapload::DmMedical;
use ss13_shared::chat::Span;

use crate::combat::CombatRng;
use crate::health::{DamageType, Health, Stat};
use crate::zone::BodyZone;
use ss13_chat::capitalize;

mod data;
mod process;
mod roll;
mod treat;

pub use process::*;
pub use roll::*;
pub use treat::*;

/// `WOUND_DAMAGE_EXPONENT`, `WOUND_MAX_CONSIDERED_DAMAGE`, `WOUND_MINIMUM_DAMAGE`.
pub const WOUND_DAMAGE_EXPONENT: f32 = 1.4;
pub const WOUND_MAX_CONSIDERED_DAMAGE: f32 = 25.0;
pub const WOUND_MINIMUM_DAMAGE: f32 = 5.0;
/// `CANT_WOUND`: a `wound_bonus` that never wounds.
pub const CANT_WOUND: f32 = -100.0;
/// `DISMEMBER_MINIMUM_DAMAGE`, `WOUND_DISMEMBER_OUTRIGHT_THRESH`,
/// `WOUND_CRITICAL_BLUNT_DISMEMBER_BONUS`.
pub const DISMEMBER_MINIMUM_DAMAGE: f32 = 10.0;
pub const WOUND_DISMEMBER_OUTRIGHT_THRESH: f32 = 150.0;
const CRITICAL_BLUNT_DISMEMBER_BONUS: f32 = 15.0;
/// `WOUND_MAX_BLOODFLOW`, `WOUND_SLASH_DAMAGE_FLOW_COEFF`, `WOUND_BURN_SANITIZATION_RATE`.
pub const WOUND_MAX_BLOODFLOW: f32 = 4.5;
pub const WOUND_SLASH_DAMAGE_FLOW_COEFF: f32 = 0.025;
pub const WOUND_BURN_SANITIZATION_RATE: f32 = 0.075;
/// `WOUND_INFECTION_*`.
pub const WOUND_INFECTION_MODERATE: f32 = 4.0;
pub const WOUND_INFECTION_SEVERE: f32 = 8.0;
pub const WOUND_INFECTION_CRITICAL: f32 = 12.0;
pub const WOUND_INFECTION_SEPTIC: f32 = 20.0;
/// `SHARP_EDGED`, `SHARP_POINTY`.
pub const SHARP_EDGED: u32 = 1;
pub const SHARP_POINTY: u32 = 2;
/// `UNCLAMPED_VESSELS_BLEEDING`, `CLAMPED_VESSELS_BLEEDING`.
pub const UNCLAMPED_VESSELS_BLEEDING: f32 = 1.5;
pub const CLAMPED_VESSELS_BLEEDING: f32 = 0.2;
/// `NUTRITION_LEVEL_FED`: burns heal on their own above it.
const NUTRITION_LEVEL_FED: f32 = 350.0;
/// `BODYTEMP_NORMAL`: pierces clot faster 10 K below it.
const BODYTEMP_NORMAL: f64 = 310.15;

/// tg `surgery_state` bits of a bodypart (`code/__DEFINES/bodyparts.dm`).
pub mod state {
    pub const SKIN_CUT: u16 = 1 << 0;
    pub const SKIN_OPEN: u16 = 1 << 1;
    pub const VESSELS_UNCLAMPED: u16 = 1 << 2;
    pub const VESSELS_CLAMPED: u16 = 1 << 3;
    pub const ORGANS_CUT: u16 = 1 << 4;
    pub const BONE_DRILLED: u16 = 1 << 5;
    pub const BONE_SAWED: u16 = 1 << 6;
    pub const CAVITY_WIDENED: u16 = 1 << 9;
    pub const ALL_SKIN: u16 = SKIN_CUT | SKIN_OPEN;
    pub const ALL_VESSEL: u16 = VESSELS_UNCLAMPED | VESSELS_CLAMPED;
    pub const ALL_BONE: u16 = BONE_DRILLED | BONE_SAWED;
    /// `ALL_SURGERY_STATES_UNSET_ON_CLOSE`.
    pub const UNSET_ON_CLOSE: u16 = ALL_SKIN | ALL_VESSEL | ALL_BONE | ORGANS_CUT | CAVITY_WIDENED;
}

/// tg `WOUND_BLUNT`, `WOUND_SLASH`, `WOUND_PIERCE`, `WOUND_BURN`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WoundType {
    Blunt,
    Slash,
    Pierce,
    Burn,
}

/// tg `wound_series`: one wound per series and limb, worse ones replace milder ones.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Series {
    Slash,
    Pierce,
    Bone,
    Burn,
}

/// The randomly generated wounds of a human (tg `/datum/wound_pregen_data` with
/// `can_be_randomly_generated`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WoundKind {
    SlashModerate,
    SlashSevere,
    SlashCritical,
    PierceModerate,
    PierceSevere,
    PierceCritical,
    Dislocation,
    Hairline,
    Compound,
    BurnModerate,
    BurnSevere,
    BurnCritical,
}

/// The static vars of one wound type and its pregen data.
#[derive(Clone, Copy, Debug)]
pub struct WoundDef {
    pub series: Series,
    pub wounding: WoundType,
    /// `WOUND_SEVERITY_MODERATE` 1 .. `CRITICAL` 3.
    pub severity: u8,
    pub threshold_minimum: f32,
    pub threshold_penalty: f32,
    pub series_threshold_penalty: f32,
    pub name: &'static str,
    pub desc: &'static str,
    pub treat_text_short: &'static str,
    pub occur_text: &'static str,
    pub examine_desc: &'static str,
    /// `MANGLES_INTERIOR` / `MANGLES_EXTERIOR`.
    pub mangles_interior: bool,
    pub mangles_exterior: bool,
    pub initial_flow: f32,
    pub minimum_flow: f32,
    pub clot_rate: f32,
    pub gauzed_clot_rate: f32,
    pub internal_bleeding_chance: f32,
    pub internal_bleeding_coefficient: f32,
    pub demotes_to: Option<WoundKind>,
    pub limp_slowdown: f32,
    pub limp_chance: f32,
    pub disabling: bool,
    pub damage_multiplier_penalty: f32,
    pub flesh_damage: f32,
    pub infection_rate: f32,
    pub regen_ticks_needed: u32,
    /// `surgery_states` the wound puts on its limb.
    pub surgery_states: u16,
    /// `BIO_JOINTED` required (arms and legs).
    pub jointed_only: bool,
}

/// tg `severity_text()`.
pub fn severity_text(severity: u8) -> &'static str {
    match severity {
        0 => "Тривиальная",
        1 => "Умеренная",
        2 => "Тяжелая",
        _ => "Критическая",
    }
}

/// One wound instance on a limb.
#[derive(Clone, Debug, PartialEq)]
pub struct Wound {
    pub kind: WoundKind,
    pub blood_flow: f32,
    /// tg `surgery_states` of this instance: what it put on the limb and still owns.
    pub surgery_states: u16,
    /// Bones.
    pub gelled: bool,
    pub taped: bool,
    pub regen_ticks: u32,
    pub regen_ticks_needed: u32,
    /// Compound fracture reset in surgery.
    pub reset: bool,
    /// Pierce: `mend_state`, vessels joined and waiting to be sealed.
    pub mend_state: bool,
    /// Burns.
    pub flesh_damage: f32,
    pub flesh_healing: f32,
    pub infection: f32,
    pub sanitization: f32,
    pub strikes_to_lose_limb: u32,
    pub threshold_penalty: f32,
    /// tg `disabling` (live: splints and infection change it).
    pub disabling: bool,
}

impl Wound {
    pub fn new(kind: WoundKind) -> Self {
        let def = kind.def();
        Self {
            kind,
            blood_flow: 0.0,
            surgery_states: def.surgery_states,
            gelled: false,
            taped: false,
            regen_ticks: 0,
            regen_ticks_needed: def.regen_ticks_needed,
            reset: false,
            mend_state: false,
            flesh_damage: def.flesh_damage,
            flesh_healing: 0.0,
            infection: 0.0,
            sanitization: 0.0,
            strikes_to_lose_limb: 3,
            threshold_penalty: def.threshold_penalty,
            disabling: def.disabling,
        }
    }

    pub fn def(&self) -> WoundDef {
        self.kind.def()
    }

    /// `/datum/wound/burn/flesh/can_be_ointmented_or_meshed`.
    pub fn can_be_ointmented_or_meshed(&self) -> bool {
        (self.infection > 0.0 && self.sanitization < self.infection)
            || (self.flesh_damage > 0.0 && self.flesh_healing <= self.flesh_damage)
    }
}

/// A wrap on a limb (tg `applied_items[LIMB_ITEM_GAUZE]`): the wrap's live absorption and its
/// static vars.
#[derive(Clone, Debug, PartialEq)]
pub struct Gauze {
    pub name: String,
    pub absorption_capacity: f32,
    pub absorption_rate: f32,
    /// 1 when the wrap does not splint (`splint_factor` unset).
    pub splint_factor: f32,
    pub burn_cleanliness_bonus: f32,
}

impl Gauze {
    pub fn from_dm(name: String, med: &DmMedical) -> Self {
        Self {
            name,
            absorption_capacity: med.absorption_capacity,
            absorption_rate: med.absorption_rate,
            splint_factor: if med.splint_factor > 0.0 {
                med.splint_factor
            } else {
                1.0
            },
            burn_cleanliness_bonus: med.burn_cleanliness_bonus,
        }
    }
}

/// A bodypart's medical state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Limb {
    pub wounds: Vec<Wound>,
    pub gauze: Option<Gauze>,
    /// tg `surgery_state` bits ([`state`]).
    pub surgery: u16,
    /// `TRAIT_READY_TO_OPERATE` of the bodypart (surgical drapes).
    pub prepped: bool,
    /// Amputated: the stump (tg `BODYPART_STUMP`) left by `super::bodyparts::drop_limb`.
    pub missing: bool,
    /// tg `TRAIT_PARALYSIS_*` of this limb (brain traumas, `super::traumas`): disabled.
    pub paralyzed: bool,
    /// A robotic prosthetic (`/obj/item/bodypart/*/robot`, `BIO_ROBOTIC`) is attached here.
    // shortcut: robotic limbs get no wounds (tg has metal/wired wounds) and keep the body's
    // damage modifiers (tg `brute_modifier`/`burn_modifier` 0.8 per limb).
    pub robotic: bool,
}

impl Limb {
    /// tg `get_splint_factor`.
    pub fn splint_factor(&self) -> f32 {
        self.gauze.as_ref().map_or(1.0, |g| g.splint_factor)
    }

    pub fn is_splinted(&self) -> bool {
        self.splint_factor() < 1.0
    }

    /// tg `wound_damage_multiplier`.
    pub fn damage_multiplier(&self) -> f32 {
        self.wounds
            .iter()
            .map(|w| w.def().damage_multiplier_penalty)
            .product()
    }

    /// tg `mangled_state`: (exterior, interior).
    pub fn mangled(&self) -> (bool, bool) {
        let ext = self.wounds.iter().any(|w| w.def().mangles_exterior);
        let int = self.wounds.iter().any(|w| w.def().mangles_interior);
        (ext, int)
    }

    pub fn wound(&self, series: Series) -> Option<&Wound> {
        self.wounds.iter().find(|w| w.def().series == series)
    }

    pub fn has_state(&self, states: u16) -> bool {
        self.surgery & states == states
    }

    pub fn has_any_state(&self, states: u16) -> bool {
        self.surgery & states != 0
    }

    /// tg `add_surgical_state`: wounds give up the states surgery touches.
    pub fn add_surgery(&mut self, states: u16) {
        self.surgery |= states;
        for wound in &mut self.wounds {
            wound.surgery_states &= !states;
        }
    }

    /// tg `remove_surgical_state`.
    pub fn remove_surgery(&mut self, states: u16) {
        if self.surgery & states == 0 {
            return;
        }
        self.surgery &= !states;
        for wound in &mut self.wounds {
            wound.surgery_states &= !states;
        }
    }

    /// tg `refresh_bleed_rate` of a human bodypart in `zone`.
    pub fn bleed_rate(&self, zone: BodyZone, lying: bool) -> f32 {
        let mut rate = 0.0;
        let mut surgery = 0.0;
        if self.has_any_state(state::VESSELS_UNCLAMPED) {
            surgery += UNCLAMPED_VESSELS_BLEEDING;
        } else if self.has_any_state(state::VESSELS_CLAMPED | state::ORGANS_CUT) {
            surgery += CLAMPED_VESSELS_BLEEDING;
        }
        match zone {
            BodyZone::Head => surgery *= 0.5,
            BodyZone::Chest => {}
            _ => surgery *= 0.25,
        }
        if self.gauze.is_some() {
            surgery *= 0.4;
        }
        rate += surgery;
        for wound in &self.wounds {
            rate += wound.blood_flow;
            if wound.surgery_states & state::VESSELS_UNCLAMPED == 0 || surgery == 0.0 {
                continue;
            }
            // tg counts the open vessels as the wound's own bleeding.
            rate -= UNCLAMPED_VESSELS_BLEEDING;
            surgery = 0.0;
        }
        if lying {
            rate *= 0.75;
        }
        rate
    }

    /// tg `bodypart_disabled` of a limb that `can_be_disabled` (arms and legs).
    pub fn disabled(&self, damage: f32, max_damage: f32) -> bool {
        self.missing
            || self.paralyzed
            || self.wounds.iter().any(|w| w.disabling)
            || damage >= max_damage
    }

    /// The limp of a leg: (`limp_slowdown` sum, max `limp_chance`), scaled by its splint
    /// (tg `update_inefficiencies` and `/datum/status_effect/limp/update_limp`).
    fn limp(&self) -> (f32, f32) {
        let splint = self.splint_factor();
        let mut slowdown = 0.0;
        let mut chance: f32 = 0.0;
        for wound in &self.wounds {
            let def = wound.def();
            slowdown += def.limp_slowdown * splint;
            chance = chance.max(def.limp_chance * splint);
        }
        (slowdown, chance)
    }
}

/// The medical state of a human's six bodyparts, by [`BodyZone::index`].
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct Limbs {
    pub parts: [Limb; 6],
    /// `/datum/status_effect/surgery_prepped` `movement_counter`.
    pub prep_moves: u32,
}

/// What the wounds do to moving (see [`Limbs::limits`]).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Limits {
    /// Deciseconds per step.
    pub slowdown: f32,
    pub floored: bool,
}

impl Limbs {
    pub fn limb(&self, zone: BodyZone) -> &Limb {
        &self.parts[zone.index()]
    }

    pub fn limb_mut(&mut self, zone: BodyZone) -> &mut Limb {
        &mut self.parts[zone.index()]
    }

    /// tg `get_bleed_rate`.
    pub fn bleed_rate(&self, lying: bool) -> f32 {
        BodyZone::ALL
            .iter()
            .map(|&z| self.limb(z).bleed_rate(z, lying))
            .sum()
    }

    pub fn any_wounds(&self) -> bool {
        self.parts.iter().any(|l| !l.wounds.is_empty())
    }

    /// tg `update_usable_leg_status` (`limbless_slowdown`) and the limp of both legs.
    pub fn limits(&self, health: &Health) -> Limits {
        let legs = [BodyZone::LLeg, BodyZone::RLeg];
        let disabled = legs
            .iter()
            .filter(|&&z| {
                self.limb(z)
                    .disabled(health.part(z).damage(), z.max_damage())
            })
            .count();
        let usable = 2 - disabled;
        let mut slowdown = (2 - usable) as f32 * 3.0;
        // The limp alternates legs: each step rolls one leg's chance.
        for leg in legs {
            let (limp, chance) = self.limb(leg).limp();
            slowdown += limp * chance / 100.0 / 2.0;
        }
        Limits {
            slowdown,
            floored: usable == 0,
        }
    }
}

/// tg `SPT_PROB(percent, seconds)`.
pub fn spt_prob(rng: &mut CombatRng, percent: f32, secs: f32) -> bool {
    rng.chance((1.0 - (1.0 - percent / 100.0).powf(secs)) * 100.0)
}

#[cfg(test)]
mod tests;
