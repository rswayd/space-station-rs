//! The disease table: one `Def` per ported `/datum/disease`.

use super::*;

pub(super) const BASE: Def = Def {
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

pub(super) const CONTACT: u8 = SPREAD_BLOOD | SPREAD_SKIN | SPREAD_FLUIDS;

pub(super) const COLD: Def = Def {
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

pub(super) const COLD9: Def = Def {
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

pub(super) const FLU: Def = Def {
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

pub(super) const FLU_SPANISH: Def = Def {
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

pub(super) const GBS: Def = Def {
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

pub(super) const GBS_NO_TRANSMISSION: Def = Def {
    path: "/datum/disease/gbs/no_transmission",
    name: "Non-Transmissible GBS",
    spread_flags: SPREAD_NON_CONTAGIOUS,
    spreading_modifier: 0.0,
    ..GBS
};

pub(super) const FAKE_GBS: Def = Def {
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

pub(super) const BRAINROT: Def = Def {
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

pub(super) const TUBERCULOSIS: Def = Def {
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

pub(super) const ANXIETY: Def = Def {
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

pub(super) const ADRENAL_CRISIS: Def = Def {
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

pub(super) const DEATH_SANDWICH: Def = Def {
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

pub(super) const ANAPHYLAXIS: Def = Def {
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

pub(super) const DECLONING: Def = Def {
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

pub(super) const ADVANCE: Def = Def {
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
