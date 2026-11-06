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
// diseases (chronic illness, asthma), no transformation, magic or floor diseases, no bites or
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

const BASE: Def = Def {
    path: "/datum/disease",
    name: "No disease",
    form: "Virus",
    desc: "",
    agent: "Unknown",
    spread_text: "",
    cure_text: "",
    cures: &[],
    max_stages: 0,
    stage_prob: 2.0,
    cure_chance: 4.0,
    spreading_modifier: 1.0,
    spread_flags: SPREAD_AIRBORNE | SPREAD_FLUIDS | SPREAD_SKIN,
    disease_flags: CURABLE | CAN_CARRY | CAN_RESIST,
    visibility: 0,
    severity: Severity::Nonthreat,
    required_organ: None,
    bypasses_immunity: false,
    process_dead: false,
};

const CONTACT: u8 = SPREAD_BLOOD | SPREAD_SKIN | SPREAD_FLUIDS;

const COLD: Def = Def {
    path: "/datum/disease/cold",
    name: "The Cold",
    desc: "A common, mildly annoying contagion. If left untreated the subject will contract the flu.",
    max_stages: 3,
    cure_text: "Spaceacillin or rest",
    cures: &[SPACEACILLIN],
    agent: "XY-rhinovirus",
    spreading_modifier: 0.5,
    spread_text: "Airborne",
    severity: Severity::Nonthreat,
    required_organ: Some(Slot::Lungs),
    ..BASE
};

const COLD9: Def = Def {
    path: "/datum/disease/cold9",
    name: "The Cold",
    max_stages: 3,
    spread_text: "Skin contact",
    spread_flags: CONTACT,
    cure_text: "Spaceacillin or common Cold antibodies",
    cures: &[SPACEACILLIN],
    agent: "ICE9-rhinovirus",
    desc: "An adaption of the common cold, slightly more dangerous in nature. If left untreated \
           the subject will slow, as if partly frozen.",
    severity: Severity::Harmful,
    required_organ: Some(Slot::Lungs),
    ..BASE
};

const FLU: Def = Def {
    path: "/datum/disease/flu",
    name: "The Flu",
    max_stages: 3,
    spread_text: "Airborne",
    cure_text: "Spaceacillin, abated by rest",
    cures: &[SPACEACILLIN],
    cure_chance: 5.0,
    agent: "H13N1 Flu Virion",
    spreading_modifier: 0.75,
    desc: "A common, mildly annoying contagion. If left untreated the subject will feel quite \
           unwell.",
    severity: Severity::Minor,
    required_organ: Some(Slot::Lungs),
    ..BASE
};

const FLU_SPANISH: Def = Def {
    path: "/datum/disease/fluspanish",
    name: "Spanish inquisition Flu",
    max_stages: 3,
    spread_text: "Airborne",
    cure_text: "Spaceacillin or common Flu antibodies",
    cures: &[SPACEACILLIN],
    cure_chance: 5.0,
    agent: "1nqu1s1t10n Flu Virion",
    spreading_modifier: 0.75,
    desc: "An adaptation of the common flu, slightly more dangerous in nature. If left untreated \
           the subject will burn to death for being a heretic.",
    severity: Severity::Dangerous,
    required_organ: Some(Slot::Lungs),
    ..BASE
};

const GBS: Def = Def {
    path: "/datum/disease/gbs",
    name: "GBS",
    desc: "An extremely rare and dangerous disease that has been researched little due to its \
           potentially apocalyptic nature.",
    max_stages: 4,
    spread_text: "Skin contact",
    spread_flags: CONTACT,
    cure_text: "Synaptizine & Sulfur",
    cures: &[SYNAPTIZINE, SULFUR],
    cure_chance: 7.5,
    agent: "Gravitokinetic Bipotential SADS+",
    severity: Severity::Biohazard,
    bypasses_immunity: true,
    ..BASE
};

const GBS_NO_TRANSMISSION: Def = Def {
    path: "/datum/disease/gbs/no_transmission",
    name: "Non-Transmissible GBS",
    spread_flags: SPREAD_NON_CONTAGIOUS,
    spreading_modifier: 0.0,
    ..GBS
};

const FAKE_GBS: Def = Def {
    path: "/datum/disease/fake_gbs",
    name: "GBS",
    desc: GBS.desc,
    max_stages: 5,
    spread_text: "Skin contact",
    spread_flags: CONTACT,
    cure_text: "Synaptizine & Sulfur",
    cures: &[SYNAPTIZINE, SULFUR],
    agent: "Gravitokinetic Bipotential SADS-",
    severity: Severity::Biohazard,
    visibility: HIDDEN_BOOK,
    ..BASE
};

const BRAINROT: Def = Def {
    path: "/datum/disease/brainrot",
    name: "Brainrot",
    max_stages: 4,
    spread_text: "Skin contact",
    spread_flags: CONTACT,
    cure_text: "Mannitol",
    cures: &[MANNITOL],
    agent: "Cryptococcus Cosmosis",
    cure_chance: 7.5,
    desc: "A disease which targets brain cells, leading to brain fog - though it is otherwise \
           non-lethal.",
    required_organ: Some(Slot::Brain),
    severity: Severity::Harmful,
    bypasses_immunity: true,
    ..BASE
};

const TUBERCULOSIS: Def = Def {
    path: "/datum/disease/tuberculosis",
    form: "Fungus",
    name: "Fungal Tuberculosis",
    max_stages: 5,
    spread_text: "Airborne",
    cure_text: "Spaceacillin & Convermol",
    cures: &[SPACEACILLIN, CONVERMOL],
    agent: "Fungal Tubercle Bacillus Cosmosis",
    cure_chance: 2.5,
    desc: "A rare and highly transmissible virulent fungus. Few samples exist, rumoured to be \
           carefully grown and cultured by clandestine bio-weapon specialists. Causes fever, \
           blood vomiting, lung damage, weight loss, fatigue, and eventually death.",
    required_organ: Some(Slot::Lungs),
    severity: Severity::Biohazard,
    bypasses_immunity: true,
    ..BASE
};

const ANXIETY: Def = Def {
    path: "/datum/disease/anxiety",
    name: "Severe Anxiety",
    form: "Condition",
    max_stages: 4,
    spread_text: "Skin contact",
    spread_flags: CONTACT,
    cure_text: "Ethanol (Liquid Courage)",
    cures: &[ETHANOL],
    agent: "Excess Lepidopticides",
    desc: "A well documented condition leading to 'butterflies in the stomach' in a literal \
           sense, which are often regurgitated.",
    severity: Severity::Minor,
    ..BASE
};

const ADRENAL_CRISIS: Def = Def {
    path: "/datum/disease/adrenal_crisis",
    form: "Condition",
    name: "Adrenal Crisis",
    max_stages: 2,
    cure_text: "Trauma (Adrenaline / \"Determination\")",
    cures: &[DETERMINATION],
    cure_chance: 10.0,
    agent: "Kronkaine Abuse",
    desc: "A rare condition caused by continued abuse of Kronkaine. If left untreated the \
           subject will suffer from lethargy, dizziness and periodic loss of consciousness.",
    severity: Severity::Medium,
    spread_flags: SPREAD_NON_CONTAGIOUS,
    spread_text: "None",
    visibility: HIDDEN_PANDEMIC,
    bypasses_immunity: true,
    ..BASE
};

const DEATH_SANDWICH: Def = Def {
    path: "/datum/disease/death_sandwich_poisoning",
    name: "Death Sandwich Poisoning",
    desc: "If left untreated the subject will ultimately perish.",
    form: "Condition",
    max_stages: 3,
    cure_text: "Anacea",
    cures: &[ANACEA],
    agent: "Eating the Death Sandwich wrong",
    spread_text: "None",
    severity: Severity::Dangerous,
    disease_flags: CURABLE,
    spread_flags: SPREAD_SPECIAL,
    visibility: HIDDEN_SCANNER,
    bypasses_immunity: true,
    required_organ: Some(Slot::Stomach),
    ..BASE
};

const ANAPHYLAXIS: Def = Def {
    path: "/datum/disease/anaphylaxis",
    form: "Condition",
    name: "Anaphylaxis",
    desc: "Patient is undergoing a life-threatening allergic reaction and will die if not treated.",
    max_stages: 3,
    cure_text: "Epinephrine",
    cures: &[EPINEPHRINE],
    cure_chance: 20.0,
    agent: "Allergy",
    disease_flags: CURABLE,
    severity: Severity::Dangerous,
    spread_flags: SPREAD_NON_CONTAGIOUS,
    spread_text: "None",
    visibility: HIDDEN_PANDEMIC,
    bypasses_immunity: true,
    stage_prob: 5.0,
    ..BASE
};

const DECLONING: Def = Def {
    path: "/datum/disease/decloning",
    name: "Cellular Degeneration",
    max_stages: 5,
    stage_prob: 0.5,
    cure_text: "Rezadone, abated by Mutadone, or death",
    agent: "Severe Genetic Damage",
    desc: "If left untreated the subject will [REDACTED]!",
    // tg's `"Dangerous!"` string: only shown, its natural recovery is bypassed anyway.
    severity: Severity::Dangerous,
    cures: &[REZADONE],
    spread_flags: SPREAD_NON_CONTAGIOUS,
    spread_text: "None",
    process_dead: true,
    bypasses_immunity: true,
    ..BASE
};

const ADVANCE: Def = Def {
    path: "/datum/disease/advance",
    name: "Unknown",
    desc: "An engineered disease which can contain a multitude of symptoms.",
    form: "Advanced Disease",
    agent: "advance microbes",
    max_stages: 5,
    spread_text: "Unknown",
    visibility: HIDDEN_BOOK,
    ..BASE
};

impl Kind {
    pub const ALL: [Self; 14] = [
        Self::Cold,
        Self::Cold9,
        Self::Flu,
        Self::FluSpanish,
        Self::Gbs,
        Self::GbsNoTransmission,
        Self::FakeGbs,
        Self::Brainrot,
        Self::Tuberculosis,
        Self::Anxiety,
        Self::AdrenalCrisis,
        Self::DeathSandwich,
        Self::Anaphylaxis,
        Self::Decloning,
    ];

    pub fn def(self) -> &'static Def {
        match self {
            Self::Cold => &COLD,
            Self::Cold9 => &COLD9,
            Self::Flu => &FLU,
            Self::FluSpanish => &FLU_SPANISH,
            Self::Gbs => &GBS,
            Self::GbsNoTransmission => &GBS_NO_TRANSMISSION,
            Self::FakeGbs => &FAKE_GBS,
            Self::Brainrot => &BRAINROT,
            Self::Tuberculosis => &TUBERCULOSIS,
            Self::Anxiety => &ANXIETY,
            Self::AdrenalCrisis => &ADRENAL_CRISIS,
            Self::DeathSandwich => &DEATH_SANDWICH,
            Self::Anaphylaxis => &ANAPHYLAXIS,
            Self::Decloning => &DECLONING,
            Self::Advance => &ADVANCE,
        }
    }

    /// The type of a DM path (`/datum/disease/...`), for admins and events.
    pub fn from_path(path: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.def().path == path)
    }
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

/// A human (`viable_mobtypes = /mob/living/carbon/human` for every ported disease).
fn is_human(world: &World, mob: Entity) -> bool {
    world.get::<super::wounds::Limbs>(mob).is_some() && world.get::<Health>(mob).is_some()
}

/// `has_required_infectious_organ`: the organ is there and not robotic; a mob without
/// modelled organs has them all.
// shortcut: inorganic biology does not open robotic organs to the disease.
pub(crate) fn has_organ(world: &World, mob: Entity, slot: Slot) -> bool {
    world.get::<organs::OrganHost>(mob).is_none()
        || organs::state(world, mob, slot).is_some_and(|o| !o.robotic)
}

/// tg `CanContractDisease` (human).
pub fn can_contract(world: &World, mob: Entity, disease: &Disease) -> bool {
    if !is_human(world, mob) {
        return false;
    }
    if let Some(slot) = disease.def().required_organ
        && !has_organ(world, mob, slot)
    {
        return false;
    }
    if world.get::<Stat>(mob) == Some(&Stat::Dead) && !disease.def().process_dead {
        return false;
    }
    let id = disease.id();
    if world
        .get::<Diseases>(mob)
        .is_some_and(|d| d.resistances.contains(&id))
    {
        return false;
    }
    !has_disease(world, mob, disease)
}

/// tg `ForceContractDisease`: infects with a copy of `disease` unless the mob cannot catch it.
/// The API for events and admins.
pub fn force_contract(world: &mut World, mob: Entity, disease: &Disease) -> bool {
    can_contract(world, mob, disease) && try_infect(world, mob, disease)
}

/// tg `try_infect`; advanced diseases first push out a weaker advanced one (`DISEASE_LIMIT`).
fn try_infect(world: &mut World, mob: Entity, disease: &Disease) -> bool {
    if let Some(advance) = &disease.advance {
        let mut rivals: Vec<(String, i32)> = of_mob(world, mob)
            .iter()
            .filter_map(|d| d.advance.as_ref().map(|a| (a.id(), a.props.resistance)))
            .collect();
        // `DISEASE_LIMIT` 1: all of them must go.
        rivals.sort_by_key(|r| r.1);
        for (id, resistance) in rivals {
            if advance.props.transmittable <= resistance {
                return false;
            }
            if let Some(index) = of_mob(world, mob).iter().position(|d| d.id() == id) {
                cure(world, mob, index, false);
            }
        }
    }
    infect(world, mob, disease.copy());
    true
}

/// tg `infect`: no checks.
fn infect(world: &mut World, mob: Entity, disease: Disease) {
    let Ok(mut entity) = world.get_entity_mut(mob) else {
        return;
    };
    match entity.get_mut::<Diseases>() {
        Some(mut all) => all.list.push(disease),
        None => {
            entity.insert(Diseases {
                list: vec![disease],
                ..default()
            });
        }
    }
}

/// tg `cure(add_resistance)` of `mob`'s disease at `index`: an uncurable one stays.
pub fn cure(world: &mut World, mob: Entity, index: usize, add_resistance: bool) {
    let Some(mut all) = world.get_mut::<Diseases>(mob) else {
        return;
    };
    let Some(disease) = all.list.get(index) else {
        return;
    };
    if disease.severity == Severity::Uncurable {
        return;
    }
    let disease = all.list.remove(index);
    if !add_resistance {
        return;
    }
    if let Some(advance) = &disease.advance {
        for name in advance.resisted_symptoms() {
            if !all.symptom_resistances.contains(&name) {
                all.symptom_resistances.push(name);
            }
        }
    }
    if disease.def().disease_flags & CAN_RESIST == 0 {
        return;
    }
    let mut ids = vec![disease.id()];
    // "Buy one, get one free": the variant's antibodies come along.
    let extra = match disease.kind {
        Kind::Cold => Some(COLD9.path),
        Kind::Cold9 => Some(COLD.path),
        Kind::Flu => Some(FLU_SPANISH.path),
        Kind::FluSpanish => Some(FLU.path),
        _ => None,
    };
    ids.extend(extra.map(str::to_owned));
    for id in ids {
        if !all.resistances.contains(&id) {
            all.resistances.push(id);
        }
    }
}

/// tg `fully_heal(HEAL_NEGATIVE_DISEASES)`: every disease but the positive ones, no immunity.
pub fn cure_negative(world: &mut World, mob: Entity) {
    let mut index = of_mob(world, mob).len();
    while index > 0 {
        index -= 1;
        if of_mob(world, mob)[index].severity != Severity::Positive {
            cure(world, mob, index, false);
        }
    }
}

/// tg `ContactContractDisease` of a carbon: a touch on `zone` (random by tg's weights without
/// one) passes the bio armour covering it.
pub fn contact_contract(
    world: &mut World,
    mob: Entity,
    disease: &Disease,
    zone: Option<BodyZone>,
    rng: &mut CombatRng,
) -> bool {
    if !can_contract(world, mob, disease) {
        return false;
    }
    // DM divides by zero here (a runtime): nothing spreads.
    if disease.spreading_modifier <= 0.0 || rng.chance(15.0 / disease.spreading_modifier) {
        return false;
    }
    let satiety = world
        .get::<crate::hunger::Hunger>(mob)
        .map_or(0.0, |h| h.satiety);
    if satiety > 0.0 && rng.chance(satiety / 2.0) {
        return false;
    }
    let zone = zone.unwrap_or_else(|| {
        // head 80, chest 100, hands 17.5 each, feet 7.5 each.
        let roll = rng.unit() * 230.0;
        match roll {
            r if r < 80.0 => BodyZone::Head,
            r if r < 180.0 => BodyZone::Chest,
            r if r < 197.5 => BodyZone::RArm,
            r if r < 215.0 => BodyZone::LArm,
            r if r < 222.5 => BodyZone::RLeg,
            _ => BodyZone::LLeg,
        }
    });
    if virus_resistant(world, mob) && rng.chance(75.0) {
        return false;
    }
    let inventory = world.get::<Inventory>(mob).cloned().unwrap_or_default();
    let covers = |item: Option<Entity>, bits: u32| {
        item.filter(|&i| {
            world
                .get::<DmClothing>(i)
                .is_some_and(|c| c.body_parts_covered & bits != 0)
        })
    };
    let layers: [Option<Entity>; 3] = match zone {
        BodyZone::Head => [inventory.head, inventory.mask, inventory.neck],
        BodyZone::Chest => [inventory.suit, inventory.uniform, None],
        BodyZone::LArm | BodyZone::RArm => [covers(inventory.suit, HANDS), inventory.gloves, None],
        BodyZone::LLeg | BodyZone::RLeg => [covers(inventory.suit, FEET), inventory.shoes, None],
    };
    for item in layers.into_iter().flatten() {
        if !rng.chance(100.0 - bio(world, Some(item))) {
            return false;
        }
    }
    try_infect(world, mob, disease)
}

/// tg `attack_hand`/`start_pulling` between two living mobs: their skin-contact diseases go
/// both ways.
pub fn touch(world: &mut World, a: Entity, b: Entity) {
    if a == b {
        return;
    }
    let skin = |world: &World, mob: Entity| -> Vec<Disease> {
        of_mob(world, mob)
            .iter()
            .filter(|d| d.spread_flags & SPREAD_SKIN != 0)
            .cloned()
            .collect()
    };
    let (from_a, from_b) = (skin(world, a), skin(world, b));
    if from_a.is_empty() && from_b.is_empty() {
        return;
    }
    let mut rng = super::fork_rng(world);
    for disease in &from_b {
        contact_contract(world, a, disease, None, &mut rng);
    }
    for disease in &from_a {
        contact_contract(world, b, disease, None, &mut rng);
    }
}

/// The bio rating of a worn item.
fn bio(world: &World, item: Option<Entity>) -> f32 {
    item.and_then(|i| world.get::<DmArmor>(i))
        .map_or(0.0, |a| a.bio)
}

/// Bio armour over the mouth: helmet and mask (`is_mouth_covered`).
// shortcut: any head or mask item counts as covering the mouth (tg: `HEADCOVERSMOUTH` /
// `MASKCOVERSMOUTH`); items without bio armour add nothing either way.
fn mouth_bio(world: &World, mob: Entity) -> f32 {
    world
        .get::<Inventory>(mob)
        .map_or(0.0, |inv| bio(world, inv.head) + bio(world, inv.mask))
}

/// `TRAIT_VIRUS_RESISTANCE`: spaceacillin in the blood.
pub(crate) fn virus_resistant(world: &World, mob: Entity) -> bool {
    blood_has(world, mob, SPACEACILLIN, false)
}

/// tg `has_reagent` on the blood (`check_subtypes`).
pub(crate) fn blood_has(world: &World, mob: Entity, path: &str, subtypes: bool) -> bool {
    let (Some(chem), Some(m)) = (
        world.get_resource::<crate::chem::Chem>(),
        world.get::<crate::chem::MobReagents>(mob),
    ) else {
        return false;
    };
    m.blood.reagents.iter().any(|r| {
        r.volume > 0.0
            && if subtypes {
                chem.0.is_type(r.id, path)
            } else {
                chem.0.def(r.id).path == path
            }
    })
}

/// Removes up to `amount` of every blood reagent of type `path` (`include_subtypes`); returns
/// the units removed.
pub(crate) fn remove_blood(world: &mut World, mob: Entity, path: Option<&str>, amount: f32) -> f32 {
    let Some(chem) = world
        .get_resource::<crate::chem::Chem>()
        .map(|c| c.0.clone())
    else {
        return 0.0;
    };
    let Some(mut m) = world.get_mut::<crate::chem::MobReagents>(mob) else {
        return 0.0;
    };
    let ids: Vec<_> = m
        .blood
        .reagents
        .iter()
        .filter(|r| path.is_none_or(|p| chem.is_type(r.id, p)))
        .map(|r| r.id)
        .collect();
    let mut removed = 0.0;
    for id in ids {
        removed += m.blood.remove_reagent(id, amount);
    }
    removed
}

fn losebreath(world: &World, mob: Entity) -> f32 {
    world
        .get::<crate::breath::Respiration>(mob)
        .map_or(0.0, |r| r.losebreath)
}

fn on_internals(world: &World, mob: Entity) -> bool {
    world
        .get::<crate::internals::Internals>(mob)
        .is_some_and(|i| i.0.is_some())
}

/// tg `can_spread_airborne_diseases` (carbon).
fn can_spread_airborne(world: &World, mob: Entity, rng: &mut CombatRng) -> bool {
    if on_internals(world, mob) || losebreath(world, mob) >= 1.0 {
        return false;
    }
    if world
        .get::<Health>(mob)
        .is_some_and(|h| !h.has_part(BodyZone::Head))
    {
        return true;
    }
    !rng.chance(2.0 * mouth_bio(world, mob))
}

/// tg `can_be_spread_airborne_disease` (carbon).
fn can_catch_airborne(world: &World, mob: Entity, rng: &mut CombatRng) -> bool {
    if on_internals(world, mob) && rng.chance(75.0) {
        return false;
    }
    if losebreath(world, mob) >= 1.0 {
        return false;
    }
    if virus_resistant(world, mob) && rng.chance(75.0) {
        return false;
    }
    !rng.chance(mouth_bio(world, mob))
}

/// tg `contract_airborne_disease`.
fn contract_airborne(world: &mut World, mob: Entity, disease: &Disease, rng: &mut CombatRng) {
    if !can_catch_airborne(world, mob, rng)
        || !rng.chance((50.0 * disease.spreading_modifier - 1.0).min(50.0))
        || !has_organ(world, mob, Slot::Lungs)
    {
        return;
    }
    force_contract(world, mob, disease);
}

/// tg `airborne_spread(spread_range, force_spread, require_facing)` from `mob`: every human in
/// view within `range` with an air path.
// tg returns early when the host *can* spread (`if(affected_mob.can_spread_airborne_diseases())
// return FALSE`), an inverted check; the intent (masks and internals stop the spreader) is
// ported.
pub(crate) fn airborne_spread(
    world: &mut World,
    mob: Entity,
    disease: &Disease,
    range: u32,
    force: bool,
    require_facing: bool,
    rng: &mut CombatRng,
) -> bool {
    if disease.spread_flags & SPREAD_AIRBORNE == 0 && !force {
        return false;
    }
    if !can_spread_airborne(world, mob, rng)
        || !has_organ(world, mob, Slot::Lungs)
        || virus_resistant(world, mob)
    {
        return false;
    }
    let Some(&from) = world.get::<TilePos>(mob) else {
        return false;
    };
    let Some(index) = world.get_resource::<SpatialIndex>() else {
        return false;
    };
    let facing = world.get::<Facing>(mob).map(|f| f.0);
    let lying = world.get::<BodyPosition>(mob).is_some_and(|b| b.lying());
    let targets: Vec<Entity> = index
        .in_radius(from, range)
        .filter(|&e| e != mob && is_human(world, e))
        .filter(|&e| {
            let Some(&to) = world.get::<TilePos>(e) else {
                return false;
            };
            (!require_facing || (!lying && facing.is_some_and(|d| faces(d, from, to))))
                && crate::mobs::ai::behaviors::can_see(world, from, to, range)
                && air_path(world, from, to)
        })
        .collect();
    for target in targets {
        contract_airborne(world, target, disease, rng);
    }
    true
}

/// tg `is_source_facing_target`: the target is in front or 45 degrees off.
fn faces(dir: Dir, from: TilePos, to: TilePos) -> bool {
    const RING: [Dir; 8] = [
        Dir::North,
        Dir::NorthEast,
        Dir::East,
        Dir::SouthEast,
        Dir::South,
        Dir::SouthWest,
        Dir::West,
        Dir::NorthWest,
    ];
    let Some(goal) = dir_of((to.x - from.x).signum(), (to.y - from.y).signum()) else {
        return false;
    };
    let at = |d: Dir| RING.iter().position(|&r| r == d).unwrap_or(0);
    matches!((at(goal) + 8 - at(dir)) % 8, 0 | 1 | 7)
}

fn dir_of(dx: i32, dy: i32) -> Option<Dir> {
    let bits = (u8::from(dy > 0))
        | (u8::from(dy < 0) << 1)
        | (u8::from(dx > 0) << 2)
        | (u8::from(dx < 0) << 3);
    Dir::from_bits(bits)
}

/// tg `disease_air_spread_walk`: from `end` towards `start`, every step between turfs that
/// share air (`TURFS_CAN_SHARE`). Without simulated air every path is open.
fn air_path(world: &World, start: TilePos, end: TilePos) -> bool {
    let Some(level) = world
        .get_resource::<ss13_atmos::Atmos>()
        .and_then(|a| a.level(start.z))
    else {
        return true;
    };
    let mut at = end;
    while at != start {
        let (dx, dy) = ((start.x - at.x).signum(), (start.y - at.y).signum());
        let next = TilePos::new(at.x + dx, at.y + dy, at.z);
        if !shares_air(level, at, next) {
            return false;
        }
        at = next;
    }
    true
}

/// `TURFS_CAN_SHARE(a, b)` of neighbours: adjacent for air, or (diagonal) sharing an
/// air-adjacent turf.
fn shares_air(level: &ss13_atmos::LevelAir, a: TilePos, b: TilePos) -> bool {
    let (Some(ta), Some(tb)) = (level.tile(a.x, a.y), level.tile(b.x, b.y)) else {
        return false;
    };
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let (Some(h), Some(v)) = (dir_of(dx, 0), dir_of(0, dy)) else {
        return dir_of(dx, dy).is_some_and(|d| level.adjacent(ta, d));
    };
    (level.adjacent(ta, h) && level.adjacent(tb, v.reverse()))
        || (level.adjacent(ta, v) && level.adjacent(tb, h.reverse()))
}

/// What one `stage_act` asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Keep,
    Cure {
        resistance: bool,
    },
    /// The host is gone (gibbed).
    Gone,
}

/// The host's state `stage_act` reads.
#[derive(Clone, Copy, Debug)]
pub struct StageCtx {
    pub secs: f32,
    /// `has_cure`: 0 none; 1 all (an advanced disease: the remedied share of its symptoms).
    pub cure_status: f32,
    /// `TRAIT_VIRUS_RESISTANCE` (spaceacillin).
    pub virus_resistant: bool,
    pub malnourished: bool,
    pub has_required_organ: bool,
    /// `get_immunity_recovery`.
    pub immunity_recovery: f32,
    /// The mood's recovery bonus (`sanity_level`: +0.4 great ... -0.4 insane).
    pub mood_recovery: f32,
    /// Asleep and not malnourished: the sleeping bonuses (bed, bedsheet, pillow, deafness),
    /// then everything ×`DISEASE_SLEEPING_RECOVERY_MULTIPLIER`.
    pub sleep_bonus: Option<f32>,
}

/// `DISEASE_GOOD_SLEEPING_RECOVERY_BONUS`, `DISEASE_SLEEPING_RECOVERY_MULTIPLIER`.
const GOOD_SLEEPING_RECOVERY_BONUS: f32 = 0.6;
const SLEEPING_RECOVERY_MULTIPLIER: f32 = 6.0;

/// `stage_act`'s mood recovery bonus.
fn mood_recovery(sanity_level: Option<u8>) -> f32 {
    use crate::mood::{
        SANITY_LEVEL_CRAZY, SANITY_LEVEL_GREAT, SANITY_LEVEL_INSANE, SANITY_LEVEL_NEUTRAL,
    };
    match sanity_level {
        Some(SANITY_LEVEL_GREAT) => 0.4,
        Some(SANITY_LEVEL_NEUTRAL) => 0.2,
        Some(SANITY_LEVEL_CRAZY) => -0.2,
        Some(SANITY_LEVEL_INSANE) => -0.4,
        _ => 0.0,
    }
}

/// `stage_act`'s sleeping bonuses for the paths on the sleeper's tile.
fn sleep_recovery_bonus(tile: &[String], deaf: bool) -> f32 {
    use crate::obj_damage::is_under;
    let on = |root: &str| tile.iter().any(|p| is_under(p, root));
    let mut bonus = 0.0;
    if deaf {
        bonus += GOOD_SLEEPING_RECOVERY_BONUS;
    }
    if on("/obj/structure/bed") {
        bonus += GOOD_SLEEPING_RECOVERY_BONUS;
    } else if on("/obj/structure/table") {
        bonus += GOOD_SLEEPING_RECOVERY_BONUS / 2.0;
    }
    if on("/obj/item/bedsheet") {
        bonus += GOOD_SLEEPING_RECOVERY_BONUS;
    }
    if on("/obj/item/pillow") {
        bonus += GOOD_SLEEPING_RECOVERY_BONUS;
    }
    bonus
}

/// The base `stage_act`: `Keep` with `true` when the symptoms act this tick.
// shortcut: `bad_immune` is always 1 (no immunodeficiency).
pub fn stage_act(disease: &mut Disease, ctx: &StageCtx, rng: &mut CombatRng) -> (Outcome, bool) {
    let def = disease.def();
    let slowdown = if ctx.virus_resistant { 0.5 } else { 1.0 };
    if !ctx.has_required_organ {
        return (Outcome::Cure { resistance: false }, false);
    }
    if ctx.cure_status > 0.0 {
        let mut cure_mod = disease.cure_chance;
        if disease.advance.is_some() {
            cure_mod *= 2.0 * ctx.cure_status;
        }
        if def.disease_flags & CURABLE != 0 && spt_prob(rng, cure_mod, ctx.secs) {
            return (Outcome::Cure { resistance: true }, false);
        }
    }
    if disease.stage == disease.max_stages {
        disease.stage_peaked = true;
    }
    if spt_prob(rng, disease.stage_prob * slowdown * 2.0, ctx.secs) {
        if disease.half_stage {
            disease.half_stage = false;
            let next = (disease.stage + 1).min(disease.max_stages);
            disease.update_stage(next);
        } else if disease.stage <= disease.max_stages {
            disease.half_stage = true;
        }
    }
    if def.disease_flags & CURABLE != 0 && !def.bypasses_immunity {
        let cycles = match disease.severity {
            Severity::Positive if slowdown >= 1.0 && !ctx.malnourished => {
                return (Outcome::Keep, !disease.carrier);
            }
            severity => severity.cycles(),
        };
        let cycles_to_beat = cycles.max(RECOVERY_SCALING);
        disease.peaked_cycles += f32::from(disease.stage) / f32::from(disease.max_stages.max(1));
        let mut recovery = disease.peaked_cycles / (cycles_to_beat / RECOVERY_SCALING);
        if disease.stage_peaked {
            recovery *= PEAKED_RECOVERY_MULTIPLIER;
        }
        if slowdown < 1.0 {
            recovery += ((1.0 - slowdown)
                * (SLOWDOWN_RECOVERY_BONUS * 2.0)
                * ((SLOWDOWN_RECOVERY_BONUS_DURATION - disease.chemical_offsets)
                    / SLOWDOWN_RECOVERY_BONUS_DURATION))
                .clamp(0.0, SLOWDOWN_RECOVERY_BONUS);
            disease.chemical_offsets =
                (disease.chemical_offsets + 1.0).min(SLOWDOWN_RECOVERY_BONUS_DURATION);
        }
        if ctx.malnourished {
            recovery -= MALNUTRITION_RECOVERY_PENALTY;
        }
        recovery += ctx.mood_recovery;
        recovery += ctx.immunity_recovery;
        if let Some(bonus) = ctx.sleep_bonus {
            recovery += bonus;
            recovery *= SLEEPING_RECOVERY_MULTIPLIER;
        }
        let recovery = recovery.clamp(0.0, 100.0);
        if recovery > 0.0 {
            let failure = 1.0 - disease.recovery_failure_chance() / 100.0;
            if spt_prob(rng, recovery * failure, ctx.secs) {
                if disease.stage == 1
                    && rng.chance(disease.cure_chance * FINAL_CURE_CHANCE_MULTIPLIER)
                {
                    if disease.severity == Severity::Positive && slowdown < 1.0 {
                        return (Outcome::Keep, !disease.carrier);
                    }
                    if !ctx.malnourished {
                        return (Outcome::Cure { resistance: true }, false);
                    }
                    if !disease.stage_peaked {
                        return (Outcome::Cure { resistance: false }, false);
                    }
                    if rng.chance(50.0) {
                        return (Outcome::Cure { resistance: true }, false);
                    }
                }
                let lower = disease.stage.saturating_sub(1).max(1);
                disease.update_stage(lower);
            }
        }
        // Spaceacillin (and sleep) hold the symptoms back, less and less over time.
        if slowdown != 1.0 {
            let held = (100.0 * (disease.symptom_offsets / SYMPTOM_OFFSET_DURATION))
                .min(100.0 - disease.cure_chance * FINAL_CURE_CHANCE_MULTIPLIER);
            if rng.chance(100.0 - held) {
                disease.symptom_offsets =
                    (disease.symptom_offsets + 1.0).min(SYMPTOM_OFFSET_DURATION);
                return (Outcome::Keep, false);
            }
        }
    }
    (Outcome::Keep, !disease.carrier)
}

/// tg `has_cure` of a simple disease: all its cures (subtypes count) are in the blood.
fn has_cure(world: &World, mob: Entity, disease: &Disease) -> f32 {
    let def = disease.def();
    if def.disease_flags & CURABLE == 0 {
        return 0.0;
    }
    if let Some(advance) = &disease.advance {
        return advance.cure_status();
    }
    let all = !def.cures.is_empty() && def.cures.iter().all(|c| blood_has(world, mob, c, true));
    if all { 1.0 } else { 0.0 }
}

/// Every disease of `mob` for one `SSmobs` fire of `secs` (tg `on_life` and `on_breath`).
// Cost: every 2 s, per human with diseases (few), a handful of rolls per disease; airborne
// spread is one radius-2 index query per breath of an airborne host.
pub fn life(world: &mut World, mob: Entity, secs: f32) {
    let Some(all) = world.get::<Diseases>(mob) else {
        return;
    };
    if all.list.is_empty() {
        return;
    }
    let ids: Vec<String> = all.list.iter().map(Disease::id).collect();
    let dead = world.get::<Stat>(mob) == Some(&Stat::Dead);
    let mut rng = super::fork_rng(world);
    // `on_breath`: `COMSIG_CARBON_PRE_BREATHE` fires on the breathing fires (every 4th).
    let breathes = !dead
        && world
            .get_resource::<crate::breath::MobsClock>()
            .is_none_or(|c| c.times_fired.is_multiple_of(4));
    for id in ids {
        if world.get_entity(mob).is_err() {
            return;
        }
        let Some(mut disease) = of_mob(world, mob).iter().find(|d| d.id() == id).cloned() else {
            continue;
        };
        if breathes
            && disease.spread_flags & SPREAD_AIRBORNE != 0
            && spt_prob(&mut rng, INFECTIVITY * 4.0, secs)
        {
            airborne_spread(world, mob, &disease, 2, true, false, &mut rng);
        }
        if dead && !disease.def().process_dead {
            continue;
        }
        let malnourished = world
            .get::<crate::hunger::Hunger>(mob)
            .is_some_and(|h| h.nutrition <= crate::hunger::NUTRITION_LEVEL_STARVING);
        let sleeping = world
            .get::<crate::status_effects::StatusEffects>(mob)
            .is_some_and(|e| e.sleeping > 0.0);
        let sleep_bonus = (sleeping && !malnourished).then(|| {
            sleep_recovery_bonus(
                &crate::status_effects::tile_paths(world, mob),
                !crate::sound::can_hear(world, mob),
            )
        });
        let ctx = StageCtx {
            secs,
            cure_status: has_cure(world, mob, &disease),
            virus_resistant: virus_resistant(world, mob),
            malnourished,
            mood_recovery: mood_recovery(
                world.get::<crate::mood::Mood>(mob).map(|m| m.sanity_level),
            ),
            sleep_bonus,
            has_required_organ: disease
                .def()
                .required_organ
                .is_none_or(|slot| has_organ(world, mob, slot)),
            immunity_recovery: disease.advance.as_ref().map_or(0.0, |a| {
                let resisted = world
                    .get::<Diseases>(mob)
                    .map_or(&[][..], |d| d.symptom_resistances.as_slice());
                a.immunity_recovery(resisted)
            }),
        };
        let (mut outcome, act) = stage_act(&mut disease, &ctx, &mut rng);
        if act {
            outcome = match disease.kind {
                Kind::Advance => symptoms::act(world, mob, &mut disease, &mut rng),
                _ => kind_act(world, mob, &mut disease, secs, &mut rng),
            };
        }
        if outcome == Outcome::Gone || world.get_entity(mob).is_err() {
            return;
        }
        let Some(index) = of_mob(world, mob).iter().position(|d| d.id() == id) else {
            continue;
        };
        match outcome {
            Outcome::Cure { resistance } => cure(world, mob, index, resistance),
            _ => {
                if let Some(mut all) = world.get_mut::<Diseases>(mob)
                    && all.list[index] != disease
                {
                    all.list[index] = disease;
                }
            }
        }
    }
}

pub(crate) fn say_to(world: &mut World, mob: Entity, span: Span, text: impl Into<String>) {
    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
        tell(world, client, span, text);
    }
}

/// `mob.emote(key)` from code: nothing while unconscious or dead.
pub(crate) fn emote(world: &mut World, mob: Entity, key: &str) {
    if world.get::<Stat>(mob) != Some(&Stat::Conscious) {
        return;
    }
    // BandaStation's lines of the emotes the chat does not have yet.
    let text = match ss13_chat::emote::find(key) {
        Some(emote) => emote.message,
        None => match key {
            "drool" => "пускает слюни.",
            "gag" => "тужится.",
            "sniff" => "шмыгает носом.",
            "snore" => "храпит.",
            "stare" => "уставился.",
            "blink" => "моргает.",
            _ => return,
        },
    };
    crate::mobs::defense::emote(world, mob, text);
}

/// `amount` of `kind` over the whole body (tg `adjust_*_loss`, `take_overall_damage`; negative
/// heals).
pub(crate) fn damage(world: &mut World, mob: Entity, kind: DamageType, amount: f32) {
    let (Some(mut health), Some(&stat)) =
        (world.get::<Health>(mob).cloned(), world.get::<Stat>(mob))
    else {
        return;
    };
    let mut new = stat;
    crate::health::apply_damage(&mut health, &mut new, kind, amount);
    world.entity_mut(mob).insert(health);
    if new != stat {
        world.entity_mut(mob).insert(new);
    }
}

/// tg `take_bodypart_damage`: one random bodypart.
fn bodypart_damage(
    world: &mut World,
    mob: Entity,
    kind: DamageType,
    amount: f32,
    rng: &mut CombatRng,
) {
    let zone = BodyZone::ALL[rng.range(0, 5) as usize];
    super::hurt_limb(world, mob, zone, kind, amount);
}

pub(crate) fn adjust_bodytemperature(world: &mut World, mob: Entity, amount: f64, min: f64) {
    if let Some(mut r) = world.get_mut::<crate::breath::Respiration>(mob) {
        if amount < 0.0 && r.bodytemperature <= min {
            return;
        }
        r.bodytemperature = (r.bodytemperature + amount).max(min);
    }
}

pub(crate) fn confusion_up_to(world: &mut World, mob: Entity, secs: f32, max: f32) {
    if let Some(mut effects) = crate::status_effects::effects_mut(world, mob)
        && effects.confusion < max
    {
        effects.confusion = (effects.confusion + secs).min(max);
    }
}

/// tg `adjust_nutrition` (and `overeatduration` for the starving symptoms).
pub(crate) fn adjust_nutrition(world: &mut World, mob: Entity, amount: f32, overeat_secs: f32) {
    if let Some(mut hunger) = world.get_mut::<crate::hunger::Hunger>(mob) {
        hunger.nutrition = (hunger.nutrition + amount).max(0.0);
        hunger.overeat_secs = (hunger.overeat_secs + overeat_secs).max(0.0);
    }
}

/// Drops the active hand's item (only a `WEIGHT_CLASS_TINY` one with `tiny_only`), or both
/// hands' (`drop_all_held_items`).
pub(crate) fn drop_held(world: &mut World, mob: Entity, all: bool, tiny_only: bool) {
    let (Some(inventory), Some(&pos)) = (
        world.get::<Inventory>(mob).cloned(),
        world.get::<TilePos>(mob),
    ) else {
        return;
    };
    let items: Vec<Entity> = if all {
        inventory.hands.iter().flatten().copied().collect()
    } else {
        inventory.active_item().into_iter().collect()
    };
    for item in items {
        if tiny_only && world.get::<DmWeight>(item).is_none_or(|w| w.w_class != 1) {
            continue;
        }
        if !crate::inventory::no_drop(world, item) {
            crate::inventory::put_down(world, item, pos);
        }
    }
}

/// tg carbon `vomit(flags, lost_nutrition, distance)`.
// shortcut: no vomit decal (blood vomit splatters blood), no mood event, no purge.
pub(crate) fn vomit(
    world: &mut World,
    mob: Entity,
    lost_nutrition: f32,
    stun: bool,
    blood: bool,
    distance: u32,
) {
    let name = super::name_of(world, mob);
    let nutrition = world
        .get::<crate::hunger::Hunger>(mob)
        .map_or(f32::MAX, |h| h.nutrition);
    if !blood && nutrition < 100.0 {
        super::visible(
            world,
            mob,
            "Вы пытаетесь блевать, но в желудке ничего нет!",
            &format!("{} всухую тужится!", ss13_chat::speech::capitalize(&name)),
            Span::Warning,
        );
        if stun {
            crate::status_effects::apply(world, mob, Incap::Stun, 20.0);
        }
        return;
    }
    super::visible(
        world,
        mob,
        "Вы блюете!",
        &format!("{} блюет!", ss13_chat::speech::capitalize(&name)),
        Span::Danger,
    );
    if stun {
        crate::status_effects::apply(world, mob, Incap::Stun, 8.0);
    }
    if blood {
        if let Some(&pos) = world.get::<TilePos>(mob) {
            crate::janitor::add_splatter_floor(world, pos, false);
        }
        damage(world, mob, DamageType::Brute, 3.0 * (distance + 1) as f32);
    } else {
        adjust_nutrition(world, mob, -lost_nutrition, 0.0);
        damage(world, mob, DamageType::Tox, -3.0);
    }
}

fn pick<'a>(rng: &mut CombatRng, lines: &[&'a str]) -> &'a str {
    lines[rng.range(0, lines.len() as u32 - 1) as usize]
}

fn has_resistance(world: &World, mob: Entity, path: &str) -> bool {
    world
        .get::<Diseases>(mob)
        .is_some_and(|d| d.resistances.iter().any(|r| r == path))
}

/// The `stage_act` of each simple disease after the base one let it act.
fn kind_act(
    world: &mut World,
    mob: Entity,
    disease: &mut Disease,
    secs: f32,
    rng: &mut CombatRng,
) -> Outcome {
    let stage = disease.stage;
    let lying = world.get::<BodyPosition>(mob).is_some_and(|b| b.lying());
    let mut p = |percent: f32| spt_prob(rng, percent, secs);
    match disease.kind {
        Kind::Cold => {
            if !(2..=3).contains(&stage) {
                return Outcome::Keep;
            }
            if p(0.5) {
                emote(world, mob, "sneeze");
            }
            if p(0.5) {
                emote(world, mob, "cough");
            }
            if p(0.5) {
                say_to(world, mob, Span::Danger, "Your throat feels sore.");
            }
            if p(0.5) {
                say_to(
                    world,
                    mob,
                    Span::Danger,
                    "Mucous runs down the back of your throat.",
                );
            }
            if stage == 3 && p(0.25) && !has_resistance(world, mob, FLU.path) {
                force_contract(world, mob, &Disease::new(Kind::Flu));
                return Outcome::Cure { resistance: true };
            }
            let (rest, chance) = if stage == 2 {
                (23.0, 0.025)
            } else {
                (12.5, 0.005)
            };
            if (lying && p(rest)) || p(chance) {
                say_to(world, mob, Span::Notice, "You feel better.");
                return Outcome::Cure { resistance: true };
            }
        }
        Kind::Cold9 => {
            if !(2..=3).contains(&stage) {
                return Outcome::Keep;
            }
            let cold = if stage == 2 { -5.0 } else { -10.0 };
            adjust_bodytemperature(world, mob, cold * f64::from(secs), 0.0);
            if p(0.5) {
                emote(world, mob, "sneeze");
            }
            if p(0.5) {
                emote(world, mob, "cough");
            }
            if p(0.5) {
                say_to(world, mob, Span::Danger, "Your throat feels sore.");
            }
            if p(if stage == 2 { 2.5 } else { 5.0 }) {
                say_to(world, mob, Span::Danger, "You feel stiff.");
            }
            if stage == 2 && p(0.05) {
                say_to(world, mob, Span::Notice, "You feel better.");
                return Outcome::Cure { resistance: true };
            }
        }
        Kind::Flu => {
            if !(2..=3).contains(&stage) {
                return Outcome::Keep;
            }
            if p(0.5) {
                emote(world, mob, "sneeze");
            }
            if p(0.5) {
                emote(world, mob, "cough");
            }
            if p(0.5) {
                say_to(world, mob, Span::Danger, "Your muscles ache.");
                if rng.chance(20.0) {
                    bodypart_damage(world, mob, DamageType::Brute, 1.0, rng);
                }
            }
            if spt_prob(rng, 0.5, secs) {
                say_to(world, mob, Span::Danger, "Your stomach hurts.");
                if rng.chance(20.0) {
                    damage(world, mob, DamageType::Tox, 1.0);
                }
            }
            let rest = if stage == 2 { 10.0 } else { 7.5 };
            if lying && spt_prob(rng, rest, secs) {
                say_to(world, mob, Span::Notice, "You feel better.");
                disease.stage -= 1;
            }
        }
        Kind::FluSpanish => {
            if !(2..=3).contains(&stage) {
                return Outcome::Keep;
            }
            let heat = if stage == 2 { 5.0 } else { 10.0 };
            adjust_bodytemperature(world, mob, heat * f64::from(secs), 0.0);
            if p(2.5) {
                emote(world, mob, "sneeze");
            }
            if p(2.5) {
                emote(world, mob, "cough");
            }
            if p(if stage == 2 { 0.5 } else { 2.5 }) {
                say_to(world, mob, Span::Danger, "You're burning in your own skin!");
                bodypart_damage(world, mob, DamageType::Burn, 5.0, rng);
            }
        }
        Kind::Gbs | Kind::GbsNoTransmission => match stage {
            2 => {
                if p(2.5) {
                    emote(world, mob, "cough");
                }
            }
            3 => {
                if p(2.5) {
                    emote(world, mob, "gasp");
                }
                if p(5.0) {
                    say_to(world, mob, Span::Danger, "Your body hurts all over!");
                }
            }
            4 => {
                say_to(
                    world,
                    mob,
                    Span::BoldWarning,
                    "Your body feels as if it's trying to rip itself apart!",
                );
                if p(30.0) {
                    crate::health::gib(world, mob);
                    return Outcome::Gone;
                }
            }
            _ => {}
        },
        Kind::FakeGbs => match stage {
            2 => {
                if p(0.5) {
                    emote(world, mob, "sneeze");
                }
            }
            3 => {
                if p(2.5) {
                    emote(world, mob, "cough");
                } else if p(2.5) {
                    emote(world, mob, "gasp");
                }
                if p(5.0) {
                    say_to(
                        world,
                        mob,
                        Span::Danger,
                        "You're starting to feel very weak...",
                    );
                }
            }
            4 | 5 if p(5.0) => emote(world, mob, "cough"),
            _ => {}
        },
        Kind::Brainrot => brainrot(world, mob, stage, secs, rng),
        Kind::Tuberculosis => tuberculosis(world, mob, stage, secs, rng),
        Kind::Anxiety => anxiety(world, mob, stage, secs, rng),
        Kind::AdrenalCrisis => match stage {
            1 => {
                if p(2.5) {
                    let line = pick(rng, &["You feel lightheaded.", "You feel lethargic."]);
                    say_to(world, mob, Span::Warning, line);
                }
            }
            2 => {
                if p(5.0) {
                    crate::status_effects::apply(world, mob, Incap::Unconscious, 4.0);
                }
                if p(10.0) {
                    adjust_timed(world, mob, Timed::Slurring(SlurKind::Generic), 14.0, None);
                }
                if p(7.0) {
                    set_timed(world, mob, Timed::Dizziness, 20.0, true);
                }
                if spt_prob(rng, 2.5, secs) {
                    let line = pick(
                        rng,
                        &[
                            "You feel pain shoot down your legs!",
                            "You feel like you are going to pass out at any moment.",
                            "You feel really dizzy.",
                        ],
                    );
                    say_to(world, mob, Span::Warning, line);
                }
            }
            _ => {}
        },
        Kind::DeathSandwich => death_sandwich(world, mob, stage, secs, rng),
        Kind::Anaphylaxis => anaphylaxis(world, mob, stage, secs, rng),
        Kind::Decloning => {
            if world.get::<Stat>(mob) == Some(&Stat::Dead) {
                return Outcome::Cure { resistance: true };
            }
            if stage >= 3 && p(1.0) {
                emote(world, mob, "drool");
            }
            if stage == 2 && p(1.0) {
                emote(world, mob, "yawn");
            }
            match stage {
                3 => {
                    if p(1.0) {
                        say_to(world, mob, Span::Danger, "Your skin feels strange.");
                    }
                }
                4 => {
                    if p(2.5) {
                        organs::adjust_organ_loss_up_to(world, mob, Slot::Brain, 1.0, 170.0);
                    }
                    if p(7.5) {
                        adjust_timed(world, mob, Timed::Stutter, 6.0, None);
                    }
                }
                5 => {
                    if p(2.5) {
                        say_to(world, mob, Span::Danger, "Your skin starts degrading!");
                    }
                    if spt_prob(rng, 5.0, secs) {
                        organs::adjust_organ_loss_up_to(world, mob, Slot::Brain, 2.0, 170.0);
                    }
                }
                _ => {}
            }
        }
        Kind::Advance => {}
    }
    Outcome::Keep
}

fn brainrot(world: &mut World, mob: Entity, stage: u8, secs: f32, rng: &mut CombatRng) {
    match stage {
        2 => {
            if spt_prob(rng, 1.0, secs) {
                emote(world, mob, "blink");
            }
            if spt_prob(rng, 1.0, secs) {
                emote(world, mob, "yawn");
            }
            if spt_prob(rng, 1.0, secs) {
                say_to(world, mob, Span::Danger, "You don't feel like yourself.");
            }
            if spt_prob(rng, 2.5, secs) {
                organs::adjust_organ_loss_up_to(world, mob, Slot::Brain, 1.0, 170.0);
            }
        }
        3 | 4 => {
            if spt_prob(rng, 1.0, secs) {
                emote(world, mob, "stare");
            }
            if spt_prob(rng, 1.0, secs) {
                emote(world, mob, "drool");
            }
            let (chance, amount, line) = if stage == 3 {
                (
                    5.0,
                    2.0,
                    "Your try to remember something important...but can't.",
                )
            } else {
                (
                    7.5,
                    3.0,
                    "Strange buzzing fills your head, removing all thoughts.",
                )
            };
            if spt_prob(rng, chance, secs) {
                organs::adjust_organ_loss_up_to(world, mob, Slot::Brain, amount, 170.0);
                if rng.chance(2.0) {
                    say_to(world, mob, Span::Danger, line);
                }
            }
            if stage == 4 && spt_prob(rng, 1.5, secs) {
                say_to(world, mob, Span::Danger, "You lose consciousness...");
                let name = super::name_of(world, mob);
                super::visible(
                    world,
                    mob,
                    "You suddenly collapse!",
                    &format!("{name} suddenly collapses!"),
                    Span::Warning,
                );
                let knocked = rng.range(10, 20) as f32;
                crate::status_effects::apply(world, mob, Incap::Unconscious, knocked);
            }
            if stage == 4 && spt_prob(rng, 7.5, secs) {
                adjust_timed(world, mob, Timed::Stutter, 6.0, None);
            }
        }
        _ => {}
    }
}

fn tuberculosis(world: &mut World, mob: Entity, stage: u8, secs: f32, rng: &mut CombatRng) {
    if spt_prob(rng, f32::from(stage) * 2.0, secs) {
        emote(world, mob, "cough");
        say_to(world, mob, Span::Danger, "Your chest hurts.");
    }
    match stage {
        2 => {
            if spt_prob(rng, 1.0, secs) {
                say_to(world, mob, Span::Danger, "Your stomach violently rumbles!");
            }
            if spt_prob(rng, 2.5, secs) {
                say_to(world, mob, Span::Danger, "You feel a cold sweat form.");
            }
        }
        4 => {
            if spt_prob(rng, 1.0, secs) {
                say_to(world, mob, Span::BoldWarning, "You see four of everything!");
                set_timed(world, mob, Timed::Dizziness, 10.0, true);
            }
            if spt_prob(rng, 1.0, secs) {
                say_to(
                    world,
                    mob,
                    Span::Danger,
                    "You feel a sharp pain from your lower chest!",
                );
                damage(world, mob, DamageType::Oxy, 5.0);
                emote(world, mob, "gasp");
            }
            if spt_prob(rng, 5.0, secs) {
                say_to(
                    world,
                    mob,
                    Span::Danger,
                    "You feel air escape from your lungs painfully.",
                );
                damage(world, mob, DamageType::Oxy, 25.0);
                emote(world, mob, "gasp");
            }
        }
        5 => {
            if spt_prob(rng, 1.0, secs) {
                let line = pick(
                    rng,
                    &[
                        "You feel your heart slowing...",
                        "You relax and slow your heartbeat.",
                    ],
                );
                say_to(world, mob, Span::BoldWarning, line);
                crate::status_effects::adjust_stamina(world, mob, 70.0);
            }
            if spt_prob(rng, 5.0, secs) {
                crate::status_effects::adjust_stamina(world, mob, 100.0);
                let name = super::name_of(world, mob);
                super::visible(
                    world,
                    mob,
                    "You surrender yourself and feel at peace...",
                    &format!("{name} faints!"),
                    Span::Warning,
                );
                crate::status_effects::adjust(world, mob, Incap::Sleeping, 10.0);
            }
            if spt_prob(rng, 1.0, secs) {
                say_to(
                    world,
                    mob,
                    Span::BoldWarning,
                    "You feel your mind relax and your thoughts drift!",
                );
                confusion_up_to(world, mob, 8.0, 100.0);
            }
            if spt_prob(rng, 5.0, secs) {
                vomit(world, mob, 20.0, true, false, 1);
            }
            if spt_prob(rng, 1.5, secs) {
                let line = pick(
                    rng,
                    &[
                        "Your stomach silently rumbles...",
                        "Your stomach seizes up and falls limp, muscles dead and lifeless.",
                        "You could eat a crayon",
                    ],
                );
                say_to(world, mob, Span::Warning, line);
                adjust_nutrition(world, mob, -100.0, -200.0);
            }
            if spt_prob(rng, 7.5, secs) {
                let dressed = world
                    .get::<Inventory>(mob)
                    .is_some_and(|i| i.uniform.is_some());
                let line = if dressed {
                    pick(
                        rng,
                        &[
                            "You feel uncomfortably hot...",
                            "You feel like unzipping your jumpsuit...",
                            "You feel like taking off some clothes...",
                        ],
                    )
                } else {
                    "You feel uncomfortably hot..."
                };
                say_to(world, mob, Span::Danger, line);
                adjust_bodytemperature(world, mob, 40.0, 0.0);
            }
        }
        _ => {}
    }
}

fn anxiety(world: &mut World, mob: Entity, stage: u8, secs: f32, rng: &mut CombatRng) {
    match stage {
        2 => {
            if spt_prob(rng, 2.5, secs) {
                say_to(world, mob, Span::Notice, "You feel anxious.");
            }
        }
        3 => {
            if spt_prob(rng, 5.0, secs) {
                say_to(world, mob, Span::Notice, "Your stomach flutters.");
            }
            if spt_prob(rng, 2.5, secs) {
                say_to(world, mob, Span::Notice, "You feel panicky.");
            }
            if spt_prob(rng, 1.0, secs) {
                say_to(world, mob, Span::Danger, "You're overtaken with panic!");
                let secs = rng.range(2, 3) as f32;
                confusion_up_to(world, mob, secs, f32::MAX);
            }
        }
        4 => {
            if spt_prob(rng, 5.0, secs) {
                say_to(
                    world,
                    mob,
                    Span::Danger,
                    "You feel butterflies in your stomach.",
                );
            }
            let name = super::name_of(world, mob);
            if spt_prob(rng, 2.5, secs) {
                super::visible(
                    world,
                    mob,
                    "You have a panic attack!",
                    &format!("{name} stumbles around in a panic."),
                    Span::Danger,
                );
                let secs = rng.range(6, 8) as f32;
                confusion_up_to(world, mob, secs, f32::MAX);
                let jitter = rng.range(12, 16) as f32;
                adjust_timed(world, mob, Timed::Jitter, jitter, None);
            }
            if spt_prob(rng, 1.0, secs) {
                super::visible(
                    world,
                    mob,
                    "You cough up butterflies!",
                    &format!("{name} coughs up butterflies!"),
                    Span::Danger,
                );
            }
        }
        _ => {}
    }
}

fn death_sandwich(world: &mut World, mob: Entity, stage: u8, secs: f32, rng: &mut CombatRng) {
    match stage {
        1 => {
            if spt_prob(rng, 1.5, secs) {
                emote(world, mob, "cough");
            }
            if spt_prob(rng, 0.5, secs) {
                emote(world, mob, "gag");
            }
            if spt_prob(rng, 0.5, secs) {
                damage(world, mob, DamageType::Tox, 5.0);
            }
        }
        2 => {
            if spt_prob(rng, 5.0, secs) {
                emote(world, mob, "cough");
            }
            if spt_prob(rng, 2.5, secs) {
                emote(world, mob, "gag");
            }
            if spt_prob(rng, 1.0, secs) {
                say_to(world, mob, Span::Danger, "Your body feels hot!");
                if rng.chance(20.0) {
                    bodypart_damage(world, mob, DamageType::Burn, 1.0, rng);
                }
            }
            if spt_prob(rng, 3.0, secs) {
                damage(world, mob, DamageType::Tox, 10.0);
            }
        }
        3 => {
            if spt_prob(rng, 5.0, secs) {
                emote(world, mob, "gag");
            }
            if spt_prob(rng, 10.0, secs) {
                emote(world, mob, "gasp");
            }
            if spt_prob(rng, 2.5, secs) {
                vomit(world, mob, 20.0, true, true, 1);
            }
            if spt_prob(rng, 2.5, secs) {
                say_to(world, mob, Span::Danger, "Your body feels hot!");
                if rng.chance(60.0) {
                    bodypart_damage(world, mob, DamageType::Burn, 2.0, rng);
                }
            }
            if spt_prob(rng, 6.0, secs) {
                damage(world, mob, DamageType::Tox, 15.0);
            }
            if spt_prob(rng, 1.5, secs) {
                say_to(
                    world,
                    mob,
                    Span::Danger,
                    "You try to scream, but nothing comes out!",
                );
                set_timed(world, mob, Timed::Silence, 5.0, true);
            }
        }
        _ => {}
    }
}

fn anaphylaxis(world: &mut World, mob: Entity, stage: u8, secs: f32, rng: &mut CombatRng) {
    adjust_bodytemperature(
        world,
        mob,
        -10.0 * f64::from(secs) * f64::from(stage),
        crate::breath::BODYTEMP_COLD_DAMAGE_LIMIT - 70.0,
    );
    match stage {
        1 => {
            if spt_prob(rng, 5.0, secs) {
                say_to(
                    world,
                    mob,
                    Span::Warning,
                    "You feel your hands start to shake.",
                );
                adjust_timed(world, mob, Timed::Jitter, 4.0 * secs, Some(60.0));
            }
            if spt_prob(rng, 5.0, secs) {
                say_to(
                    world,
                    mob,
                    Span::Warning,
                    "You feel your legs start to shake.",
                );
                adjust_timed(world, mob, Timed::Jitter, 4.0 * secs, Some(60.0));
            }
            if spt_prob(rng, 2.0, secs) {
                adjust_timed(world, mob, Timed::Dizziness, 5.0 * secs, Some(60.0));
            }
            if spt_prob(rng, 1.0, secs) {
                say_to(world, mob, Span::Danger, "Your throat itches.");
            }
        }
        2 => {
            damage(world, mob, DamageType::Tox, 0.33 * secs);
            if spt_prob(rng, 5.0, secs) {
                say_to(
                    world,
                    mob,
                    Span::Warning,
                    "You feel your hands shake violently.",
                );
                adjust_timed(world, mob, Timed::Jitter, 8.0 * secs, Some(60.0));
                if rng.chance(20.0) {
                    drop_held(world, mob, true, false);
                }
            }
            if spt_prob(rng, 5.0, secs) {
                say_to(
                    world,
                    mob,
                    Span::Warning,
                    "You feel your legs shake violently.",
                );
                adjust_timed(world, mob, Timed::Jitter, 8.0 * secs, Some(60.0));
                let stamina = world
                    .get::<crate::status_effects::StatusEffects>(mob)
                    .map_or(0.0, |e| e.stamina);
                if rng.chance(40.0) && stamina < 75.0 {
                    crate::status_effects::adjust_stamina(world, mob, 15.0);
                }
            }
            if has_organ(world, mob, Slot::Eyes) && spt_prob(rng, 4.0, secs) {
                crate::vision::set_eye_blur_if_lower(world, mob, 4.0 * secs);
                say_to(
                    world,
                    mob,
                    Span::Warning,
                    "It's getting harder to see clearly.",
                );
            }
            if spt_prob(rng, 4.0, secs) {
                damage(world, mob, DamageType::Oxy, 2.0 * secs);
                if let Some(mut r) = world.get_mut::<crate::breath::Respiration>(mob) {
                    r.losebreath += 2.0 * secs;
                }
                say_to(world, mob, Span::Warning, "It's getting harder to breathe.");
            }
            if spt_prob(rng, 2.0, secs) {
                adjust_timed(world, mob, Timed::Drowsiness, 3.0 * secs, Some(30.0));
            }
            if spt_prob(rng, 2.0, secs) {
                adjust_timed(world, mob, Timed::Dizziness, 5.0 * secs, Some(60.0));
                confusion_up_to(world, mob, secs, 10.0);
            }
            if spt_prob(rng, 2.0, secs) {
                vomit(world, mob, 10.0, false, false, 1);
                crate::status_effects::apply(world, mob, Incap::Stun, 2.0);
            }
            if spt_prob(rng, 1.0, secs) {
                emote(world, mob, "cough");
            }
            if spt_prob(rng, 1.0, secs) {
                say_to(world, mob, Span::Danger, "Your throat feels sore.");
            }
        }
        3 => {
            damage(world, mob, DamageType::Tox, 3.0 * secs);
            damage(world, mob, DamageType::Oxy, secs);
            crate::status_effects::apply(world, mob, Incap::Unconscious, 3.0 * secs);
        }
        _ => {}
    }
}

/// tg `healthscan`'s disease block: one entry per disease the scanner sees.
pub fn scan_lines(world: &World, mob: Entity) -> Vec<(Span, String)> {
    let mut lines = Vec::new();
    for disease in of_mob(world, mob) {
        if disease.visibility & HIDDEN_SCANNER != 0 {
            continue;
        }
        lines.push((
            Span::BoldWarning,
            format!("Внимание: {} обнаружена", disease.form()),
        ));
        lines.push((Span::Warning, format!("Название: {}.", disease.name)));
        lines.push((
            Span::Warning,
            format!("Распространение: {}.", disease.spread_text),
        ));
        lines.push((
            Span::Warning,
            format!("Stage: {}/{}.", disease.stage, disease.max_stages),
        ));
        lines.push((
            Span::Warning,
            format!("Возможное лекарство: {}", disease.cure_text(2)),
        ));
    }
    lines
}

/// tg `get_blood_data` into a container: a copy of each of the donor's diseases and its
/// antibodies join the container's blood (`blood_reagent` `on_merge`).
pub fn blood_drawn(world: &mut World, donor: Entity, container: Entity) {
    let Some(all) = world.get::<Diseases>(donor) else {
        return;
    };
    let data = BloodDiseases {
        viruses: all.list.iter().map(Disease::copy).collect(),
        resistances: all.resistances.clone(),
    };
    merge_blood(world, container, data);
}

/// Blood moved from container `from` to container `to`: its viruses go along.
pub fn carry_blood(world: &mut World, from: Entity, to: Entity) {
    let Some(data) = world.get::<BloodDiseases>(from).cloned() else {
        return;
    };
    if blood_volume(world, to) > 0.0 {
        merge_blood(world, to, data);
    }
    forget_if_bloodless(world, from);
}

/// tg `blood_reagent` `on_merge`: simple diseases are kept, advanced ones mixed into one
/// (`Advance_Mix`); antibodies add up.
fn merge_blood(world: &mut World, container: Entity, data: BloodDiseases) {
    if data.viruses.is_empty() && data.resistances.is_empty() {
        return;
    }
    let mut rng = super::fork_rng(world);
    let Ok(mut entity) = world.get_entity_mut(container) else {
        return;
    };
    let Some(mut held) = entity.get_mut::<BloodDiseases>() else {
        entity.insert(data);
        return;
    };
    if !held.viruses.is_empty() && !data.viruses.is_empty() {
        let mut keep: Vec<Disease> = Vec::new();
        let mut mix: Vec<Disease> = Vec::new();
        for d in held.viruses.drain(..).chain(data.viruses) {
            if d.advance.is_some() {
                mix.push(d);
            } else if !keep.iter().any(|k| k.id() == d.id()) {
                keep.push(d);
            }
        }
        keep.extend(symptoms::advance_mix(mix, &mut rng));
        held.viruses = keep;
    } else if held.viruses.is_empty() {
        held.viruses = data.viruses;
    }
    for id in data.resistances {
        if !held.resistances.contains(&id) {
            held.resistances.push(id);
        }
    }
}

fn blood_volume(world: &World, container: Entity) -> f32 {
    let (Some(chem), Some(reagents)) = (
        world.get_resource::<crate::chem::Chem>(),
        world.get::<crate::chem::Reagents>(container),
    ) else {
        return 0.0;
    };
    chem.0
        .id(super::blood::BLOOD_REAGENT)
        .map_or(0.0, |id| reagents.0.amount(id))
}

fn forget_if_bloodless(world: &mut World, container: Entity) {
    if blood_volume(world, container) <= 0.0
        && let Ok(mut entity) = world.get_entity_mut(container)
    {
        entity.remove::<BloodDiseases>();
    }
}

/// tg `blood_reagent` `on_expose_mob` for blood from `container` that went into `mob`
/// (`INGEST` into the stomach, else `INJECT`): every contagious virus infects (through the
/// stomach or the heart), antibodies cure the diseases they match.
pub fn blood_exposed(world: &mut World, container: Entity, mob: Entity, ingest: bool) {
    let Some(data) = world.get::<BloodDiseases>(container).cloned() else {
        return;
    };
    let organ = if ingest { Slot::Stomach } else { Slot::Heart };
    for strain in &data.viruses {
        if strain.spread_flags & (SPREAD_SPECIAL | SPREAD_NON_CONTAGIOUS) != 0 {
            continue;
        }
        if has_organ(world, mob, organ) {
            force_contract(world, mob, strain);
        }
    }
    if !data.resistances.is_empty() {
        let mut index = of_mob(world, mob).len();
        while index > 0 {
            index -= 1;
            let d = &of_mob(world, mob)[index];
            if !d.def().bypasses_immunity && data.resistances.contains(&d.id()) {
                cure(world, mob, index, false);
            }
        }
    }
    forget_if_bloodless(world, container);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> StageCtx {
        StageCtx {
            secs: 2.0,
            cure_status: 0.0,
            virus_resistant: false,
            malnourished: false,
            has_required_organ: true,
            immunity_recovery: 0.0,
            mood_recovery: 0.0,
            sleep_bonus: None,
        }
    }

    #[test]
    fn cures_in_the_blood_cure() {
        let mut rng = CombatRng(7);
        let mut cured = 0;
        for _ in 0..200 {
            let mut cold = Disease::new(Kind::Cold);
            let c = StageCtx {
                cure_status: 1.0,
                ..ctx()
            };
            if stage_act(&mut cold, &c, &mut rng).0 == (Outcome::Cure { resistance: true }) {
                cured += 1;
            }
        }
        // 4% a second over 2 s: ~7.8%.
        assert!((5..=30).contains(&cured), "{cured}");
    }

    #[test]
    fn stages_climb_in_half_steps_and_stop_at_max() {
        let mut rng = CombatRng(3);
        let mut gbs = Disease::new(Kind::Gbs);
        for _ in 0..2000 {
            stage_act(&mut gbs, &ctx(), &mut rng);
        }
        assert_eq!(gbs.stage, gbs.max_stages);
        assert!(gbs.stage_peaked);
    }

    #[test]
    fn missing_organ_cures_without_immunity() {
        let mut rng = CombatRng(1);
        let mut cold = Disease::new(Kind::Cold);
        let c = StageCtx {
            has_required_organ: false,
            ..ctx()
        };
        assert_eq!(
            stage_act(&mut cold, &c, &mut rng).0,
            Outcome::Cure { resistance: false }
        );
    }

    #[test]
    fn subtypes_are_the_same_disease() {
        let gbs = Disease::new(Kind::Gbs);
        let quiet = Disease::new(Kind::GbsNoTransmission);
        assert!(gbs.is_same(&quiet));
        assert!(!quiet.is_same(&gbs));
        assert!(!gbs.is_same(&Disease::new(Kind::FakeGbs)));
        assert_eq!(Kind::from_path("/datum/disease/flu"), Some(Kind::Flu));
    }

    #[test]
    fn facing_covers_the_front_cone() {
        let at = TilePos::new(5, 5, 1);
        assert!(faces(Dir::North, at, TilePos::new(5, 8, 1)));
        assert!(faces(Dir::North, at, TilePos::new(6, 6, 1)));
        assert!(!faces(Dir::North, at, TilePos::new(6, 5, 1)));
        assert!(!faces(Dir::North, at, TilePos::new(5, 3, 1)));
    }
}
