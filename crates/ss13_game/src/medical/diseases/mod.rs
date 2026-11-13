//! Diseases (server), after tg `code/datums/diseases` (`_disease.dm`: `stage_act`, `has_cure`,
//! `airborne_spread`, `cure`, `Copy`; `_MobProcs.dm`: `CanContractDisease`,
//! `ContactContractDisease`, `contract_airborne_disease`, `ForceContractDisease`) and the
//! concrete diseases (`cold.dm`, `cold9.dm`, `flu.dm`, `fluspanish.dm`, `gbs.dm`, `fake_gbs.dm`,
//! `brainrot.dm`, `tuberculosis.dm`, `anxiety.dm`, `adrenal_crisis.dm`,
//! `death_sandwich_poisoning.dm`, `anaphylaxis.dm`, `decloning.dm`). Advanced (virology)
//! diseases and their symptoms are [`super::symptoms`].
//!
//! - A human's diseases are [`Diseases`] (with the ids it resists). Every `Life()` (2 s) each
//!   disease of a living host (or a dead one for `process_dead`) runs `stage_act`: cure
//!   reagents in the blood cure it (`cure_chance` a second), it climbs a half stage at
//!   `stage_prob`, and the body fights it off over time (faster at its peak, with
//!   spaceacillin, slower starving); beaten diseases leave a resistance.
//! - Spread: airborne diseases spread on every breath to humans within 2 tiles in view with an
//!   air path (masks, helmets with bio armour and internals protect both sides); skin-contact
//!   diseases on a hand touch (hug, shake, punch) and a grab, both ways, through bio armour of
//!   the touched zone; blood carries every disease into a syringe or IV bag drawn from the host
//!   ([`BloodDiseases`]) and infects whoever gets that blood injected or swallowed; injected
//!   blood with antibodies (`resistances`) cures those diseases.
//! - Infecting from elsewhere (events, admins, reagents): [`force_contract`] with a
//!   [`Disease::new`] or [`Disease::random_advance`].
//! - The health analyzer lists the diseases the scanner sees (`crate::health_scan`).
// shortcut: no darkness or blindfold sleep bonus, no immunodeficiency/virus
// immunity traits (stasis skips `life`: `crate::medical`); no chronic or incremental-cure
// diseases (chronic illness, asthma), no magic or floor diseases, no bites or
// bumps spreading contact diseases, no infective items, food or blood decals, no disease
// archive, HUD icons or logging. Unported effects: butterflies, the decloning status effect.

use bevy::prelude::*;
use ss13_mapload::{DmArmor, DmClothing, DmWeight};
use ss13_shared::chat::Span;
use ss13_shared::{Dir, Facing, TilePos};
use ss13_world::SpatialIndex;

use super::organs::{self, Slot};
use super::symptoms::{self, Advance};
use super::wounds::spt_prob;
use crate::combat::CombatRng;
use crate::health::{BodyPosition, DamageType, Health, Stat};
use crate::inventory::Inventory;
use crate::server::PlayerMob;
use crate::status_effects::{Incap, SlurKind, Timed, adjust_timed, set_timed};
use crate::tools::tell;
use crate::zone::BodyZone;

mod data;
pub(crate) mod effects;
mod life;
mod spread;
mod transfer;
mod transformation;

use data::*;
pub(crate) use effects::*;
pub use life::*;
pub use spread::*;
pub use transfer::*;
pub use transformation::reagent_expose;

/// `disease_flags`.
pub const CURABLE: u8 = 1 << 0;
pub const CAN_CARRY: u8 = 1 << 1;
pub const CAN_RESIST: u8 = 1 << 2;
/// `spread_flags` (`DISEASE_SPREAD_*`).
pub const SPREAD_SPECIAL: u8 = 1 << 0;
pub const SPREAD_NON_CONTAGIOUS: u8 = 1 << 1;
pub const SPREAD_BLOOD: u8 = 1 << 2;
pub const SPREAD_FLUIDS: u8 = 1 << 3;
pub const SPREAD_SKIN: u8 = 1 << 4;
pub const SPREAD_AIRBORNE: u8 = 1 << 5;
/// `visibility_flags`.
pub const HIDDEN_SCANNER: u8 = 1 << 0;
pub const HIDDEN_PANDEMIC: u8 = 1 << 1;
pub const HIDDEN_BOOK: u8 = 1 << 2;

/// tg `infectivity`: the chance a second of spreading on a breath (every disease keeps 41).
const INFECTIVITY: f32 = 41.0;
/// `DISEASE_RECOVERY_*`, `DISEASE_*` balance levers (`code/__DEFINES/diseases.dm`).
const RECOVERY_SCALING: f32 = 2.0;
const PEAKED_RECOVERY_MULTIPLIER: f32 = 1.2;
const SLOWDOWN_RECOVERY_BONUS: f32 = 2.0;
const SLOWDOWN_RECOVERY_BONUS_DURATION: f32 = 120.0;
const MALNUTRITION_RECOVERY_PENALTY: f32 = 3.0;
const FINAL_CURE_CHANCE_MULTIPLIER: f32 = 6.0;
const SYMPTOM_OFFSET_DURATION: f32 = 200.0;
pub(crate) const SYMPTOM_IMMUNITY_RECOVERY_BONUS: f32 = 1.5;

pub const SPACEACILLIN: &str = "/datum/reagent/medicine/spaceacillin";
const SYNAPTIZINE: &str = "/datum/reagent/medicine/synaptizine";
const SULFUR: &str = "/datum/reagent/sulfur";
const MANNITOL: &str = "/datum/reagent/medicine/mannitol";
const CONVERMOL: &str = "/datum/reagent/medicine/c2/convermol";
const ETHANOL: &str = "/datum/reagent/consumable/ethanol";
const DETERMINATION: &str = "/datum/reagent/determination";
const ANACEA: &str = "/datum/reagent/toxin/anacea";
const EPINEPHRINE: &str = "/datum/reagent/medicine/epinephrine";
const REZADONE: &str = "/datum/reagent/medicine/rezadone";

/// `HANDS`, `FEET` (`body_parts_covered` bits).
const HANDS: u32 = (1 << 9) | (1 << 10);
const FEET: u32 = (1 << 5) | (1 << 6);

/// `DISEASE_SEVERITY_*`, in `get_disease_severity_value` order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    Uncurable,
    Positive,
    Nonthreat,
    Minor,
    Medium,
    Harmful,
    Dangerous,
    Biohazard,
}

impl Severity {
    /// BandaStation's severity names.
    pub fn text(self) -> &'static str {
        match self {
            Self::Positive => "Положительная",
            Self::Nonthreat => "Безопасная",
            Self::Minor => "Незначительная",
            Self::Medium => "Средняя",
            Self::Harmful => "Вредоносная",
            Self::Dangerous => "Опасная",
            Self::Biohazard => "БИООПАСНОСТЬ",
            Self::Uncurable => "Неизлечимая",
        }
    }

    /// `DISEASE_CYCLES_*`: how long before the body may beat it.
    fn cycles(self) -> f32 {
        match self {
            Self::Positive => 15.0,
            Self::Minor => 150.0,
            Self::Medium => 120.0,
            Self::Dangerous => 90.0,
            Self::Harmful => 60.0,
            Self::Biohazard => 40.0,
            Self::Nonthreat | Self::Uncurable => 180.0,
        }
    }

    /// `set_severity` of an advanced disease from its severity property.
    pub(crate) fn from_level(level: i32) -> Self {
        match level {
            i32::MIN..=0 => Self::Positive,
            1 => Self::Nonthreat,
            2 => Self::Minor,
            3 => Self::Medium,
            4 => Self::Harmful,
            5 => Self::Dangerous,
            _ => Self::Biohazard,
        }
    }
}

/// The ported `/datum/disease` types.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Cold,
    Cold9,
    Flu,
    FluSpanish,
    Gbs,
    GbsNoTransmission,
    FakeGbs,
    Brainrot,
    Tuberculosis,
    Anxiety,
    AdrenalCrisis,
    DeathSandwich,
    Anaphylaxis,
    Decloning,
    /// `/datum/disease/revblight` (`crate::antag::revenant`).
    Revblight,
    /// `/datum/disease/transformation/*` ([`transformation`]).
    JungleFlu,
    TransformRobot,
    TransformXeno,
    TransformSlime,
    TransformCorgi,
    TransformMorph,
    TransformGondola,
    /// `/datum/disease/advance`: [`Disease::advance`] holds the symptoms.
    Advance,
}

/// The static vars of a disease type.
#[derive(Clone, Copy, Debug)]
pub struct Def {
    pub path: &'static str,
    pub name: &'static str,
    pub form: &'static str,
    pub desc: &'static str,
    pub agent: &'static str,
    pub spread_text: &'static str,
    pub cure_text: &'static str,
    /// Reagent paths (subtypes count); all of them are needed (`needs_all_cures`).
    pub cures: &'static [&'static str],
    pub max_stages: u8,
    pub stage_prob: f32,
    pub cure_chance: f32,
    pub spreading_modifier: f32,
    pub spread_flags: u8,
    pub disease_flags: u8,
    pub visibility: u8,
    pub severity: Severity,
    pub required_organ: Option<Slot>,
    pub bypasses_immunity: bool,
    pub process_dead: bool,
}

/// One `/datum/disease` instance: the vars tg copies (`Copy`) and its progress.
#[derive(Clone, Debug, PartialEq)]
pub struct Disease {
    pub kind: Kind,
    pub name: String,
    pub stage: u8,
    pub max_stages: u8,
    pub stage_prob: f32,
    pub cure_chance: f32,
    pub spreading_modifier: f32,
    pub spread_flags: u8,
    pub spread_text: String,
    pub visibility: u8,
    pub severity: Severity,
    /// tg `carrier`: spreads without symptoms.
    pub carrier: bool,
    pub(crate) stage_peaked: bool,
    peaked_cycles: f32,
    symptom_offsets: f32,
    chemical_offsets: f32,
    half_stage: bool,
    /// The symptoms and properties of an advanced disease.
    pub advance: Option<Advance>,
}

impl Disease {
    /// `new type()` of a simple disease.
    pub fn new(kind: Kind) -> Self {
        let def = kind.def();
        Self {
            kind,
            name: def.name.to_owned(),
            stage: 1,
            max_stages: def.max_stages,
            stage_prob: def.stage_prob,
            cure_chance: def.cure_chance,
            spreading_modifier: def.spreading_modifier,
            spread_flags: def.spread_flags,
            spread_text: def.spread_text.to_owned(),
            visibility: def.visibility,
            severity: def.severity,
            carrier: false,
            stage_peaked: false,
            peaked_cycles: 0.0,
            symptom_offsets: 0.0,
            chemical_offsets: 0.0,
            half_stage: false,
            advance: None,
        }
    }

    pub fn def(&self) -> &'static Def {
        self.kind.def()
    }

    /// tg `GetDiseaseID`: the type path, or the symptom ids of an advanced disease.
    pub fn id(&self) -> String {
        match &self.advance {
            Some(advance) => advance.id(),
            None => self.def().path.to_owned(),
        }
    }

    /// tg `IsSame`: `other` is of this type (or a subtype); advanced ones by id.
    pub fn is_same(&self, other: &Self) -> bool {
        match (&self.advance, &other.advance) {
            (Some(a), Some(b)) => a.id() == b.id(),
            (None, None) => {
                let (mine, theirs) = (self.def().path, other.def().path);
                theirs == mine
                    || theirs
                        .strip_prefix(mine)
                        .is_some_and(|rest| rest.starts_with('/'))
            }
            _ => false,
        }
    }

    /// tg `Copy`: the same disease starting over at stage 1.
    pub fn copy(&self) -> Self {
        let mut copy = Self::new(self.kind);
        copy.name.clone_from(&self.name);
        copy.max_stages = self.max_stages;
        copy.stage_prob = self.stage_prob;
        copy.cure_chance = self.cure_chance;
        copy.spreading_modifier = self.spreading_modifier;
        copy.spread_flags = self.spread_flags;
        copy.spread_text.clone_from(&self.spread_text);
        copy.visibility = self.visibility;
        copy.severity = self.severity;
        copy.advance = self.advance.as_ref().map(Advance::copy);
        copy
    }

    /// The analyzer's cure text (advanced: the first `count` symptom remedies).
    pub fn cure_text(&self, count: usize) -> String {
        match &self.advance {
            Some(advance) => advance.cure_text(count),
            None => self.def().cure_text.to_owned(),
        }
    }

    pub fn form(&self) -> &'static str {
        self.def().form
    }

    /// `update_stage`: false when it went to 0 (cured).
    pub(crate) fn update_stage(&mut self, stage: u8) -> bool {
        self.stage = stage;
        if stage == self.max_stages && !self.stage_peaked {
            self.stage_peaked = true;
        }
        stage > 0
    }

    /// `get_recovery_failure_chance`, percent.
    fn recovery_failure_chance(&self) -> f32 {
        self.advance
            .as_ref()
            .map_or(0.0, |a| (a.props.resistance as f32 * 1.5).clamp(0.0, 50.0))
    }
}

/// tg `diseases`, `disease_resistances` and `symptom_resistances` of a mob.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct Diseases {
    pub list: Vec<Disease>,
    /// Disease ids ([`Disease::id`]) the mob cannot catch again.
    pub resistances: Vec<String>,
    /// Symptom names an advanced disease was beaten with (`get_immunity_recovery`).
    pub symptom_resistances: Vec<&'static str>,
}

/// tg blood reagent `data["viruses"]` and `data["resistances"]`, kept on the container that
/// holds the blood.
// shortcut: the viruses ride on the container, not on the blood reagent: blood keeps them through
// container-to-container transfers and into mobs, but not through other reagent operations.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct BloodDiseases {
    pub viruses: Vec<Disease>,
    pub resistances: Vec<String>,
}

/// The diseases of `mob`.
pub fn of_mob(world: &World, mob: Entity) -> &[Disease] {
    world
        .get::<Diseases>(mob)
        .map_or(&[], |d| d.list.as_slice())
}

/// tg `HasDisease`.
pub fn has_disease(world: &World, mob: Entity, disease: &Disease) -> bool {
    of_mob(world, mob).iter().any(|d| disease.is_same(d))
}
#[cfg(test)]
mod tests;
