//! Advanced (virology) diseases and their symptoms (server), after tg
//! `code/datums/diseases/advance/advance.dm` (`GenerateProperties`, `assign_properties`,
//! `Mix`, `Advance_Mix`, `try_infect`, `has_cure`), `presets.dm` (`/random`) and
//! `advance/symptoms/*.dm`.
//!
//! An advanced disease has up to 6 symptoms. Their stats add up into its properties: stealth
//! (hidden from scanners when at least the severity), resistance (slower natural cure, beats
//! rivals), stage speed (`stage_prob`) and transmission (spread: blood, fluids, skin, air), and
//! the symptoms' severities into its severity. Every symptom has its own cure reagent: each
//! remedied symptom stops acting and speeds the cure; beating the disease leaves an immunity to
//! its symptoms that helps against later diseases with them. Symptoms act on their own timers
//! (`symptom_delay`) and grow stronger past stat thresholds.
//!
//! Ported effects: coughing and sneezing (spreading the virus), headache, itching, fever,
//! chills, confusion, dizziness, narcolepsy, vomiting, weight loss, choking, respiratory
//! distress, necrotizing fasciitis, autophagocytosis, spontaneous combustion, toxolysis,
//! aggressive healing, radioactive resonance, self-respiration, mind and sensory restoration.
//! Every other symptom counts for the stats only.
// shortcut: no hallucinations, deafness, blindness, drugginess (dizziness's transmission 6),
// voice change, DNA activation,
// hair, skin or face changes, bleeding, hypertension, alkali, the light, water, plasma,
// coma, genetic and metabolism healing, thermoregulation, eternal youth; no
// `TRAIT_NOBREATH` of self-respiration, no fever/chills set point (they push the body
// temperature directly), no reagents synthesized by thresholds, no evolve/devolve/neuter
// (virus food, mutagen), no disease archive names.

use bevy::prelude::*;
use ss13_shared::chat::Span;

use super::diseases::{
    self, Disease, HIDDEN_SCANNER, Kind, Outcome, SPREAD_AIRBORNE, SPREAD_BLOOD, SPREAD_FLUIDS,
    SPREAD_SKIN, Severity, adjust_nutrition, airborne_spread, damage, emote, say_to,
};
use super::organs::{self, Slot};
use crate::combat::CombatRng;
use crate::health::{DamageType, Health};
use crate::status_effects::Incap;

/// `VIRUS_SYMPTOM_LIMIT`.
pub const SYMPTOM_LIMIT: usize = 6;
const SPACEACILLIN: &str = diseases::SPACEACILLIN;

/// The `/datum/symptom` types, in [`DEFS`] order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SymptomKind {
    Beard,
    Bleeding,
    Hypertension,
    Chills,
    Choking,
    Asphyxiation,
    Confusion,
    Cough,
    Deafness,
    Disfiguration,
    Dizzy,
    Fever,
    Fire,
    Alkali,
    FleshEating,
    FleshDeath,
    GeneticMutation,
    Hallucigen,
    Headache,
    HealStarlight,
    HealChem,
    HealMetabolism,
    HealDarkness,
    HealComa,
    HealWater,
    HealPlasma,
    HealRadiation,
    HealAggressive,
    HealGenetic,
    Itching,
    Narcolepsy,
    Oxygen,
    MindRestoration,
    SensoryRestoration,
    Shedding,
    Polyvitiligo,
    Sneeze,
    UndeadAdaptation,
    InorganicAdaptation,
    Thermoregulation,
    ViralAdaptation,
    ViralEvolution,
    VisionLoss,
    VoiceChange,
    Vomit,
    VomitNebula,
    WeightLoss,
    Youth,
}

/// The vars of a symptom type.
#[derive(Clone, Copy, Debug)]
pub struct SymptomDef {
    pub kind: SymptomKind,
    pub path: &'static str,
    pub name: &'static str,
    pub stealth: i32,
    pub resistance: i32,
    pub stage_speed: i32,
    pub transmittable: i32,
    /// Higher is harder to generate; 0 never generates.
    pub level: u8,
    pub severity: i32,
    /// Seconds between activations, ± `delay_variation`.
    pub delay: f32,
    pub variation: f32,
    pub message_chance: f32,
    pub organ: Option<Slot>,
    /// `symptom_cure`: the reagent that remedies it.
    pub cure: Option<&'static str>,
    /// `naturally_occuring`.
    pub natural: bool,
    pub immunity_proof: bool,
}

const fn def(
    kind: SymptomKind,
    path: &'static str,
    name: &'static str,
    [stealth, resistance, stage_speed, transmittable]: [i32; 4],
    level: u8,
    severity: i32,
    delay: f32,
) -> SymptomDef {
    SymptomDef {
        kind,
        path,
        name,
        stealth,
        resistance,
        stage_speed,
        transmittable,
        level,
        severity,
        delay,
        variation: 0.25,
        message_chance: 10.0,
        organ: None,
        cure: Some(SPACEACILLIN),
        natural: true,
        immunity_proof: false,
    }
}

const fn msg(mut d: SymptomDef, chance: f32) -> SymptomDef {
    d.message_chance = chance;
    d
}

const fn organ(mut d: SymptomDef, slot: Slot) -> SymptomDef {
    d.organ = Some(slot);
    d
}

const fn cure(mut d: SymptomDef, reagent: Option<&'static str>) -> SymptomDef {
    d.cure = reagent;
    d
}

const fn steady(mut d: SymptomDef) -> SymptomDef {
    d.variation = 0.0;
    d
}

const fn immune(mut d: SymptomDef) -> SymptomDef {
    d.immunity_proof = true;
    d
}

use SymptomKind as K;

const fn heal(
    kind: SymptomKind,
    path: &'static str,
    name: &'static str,
    stats: [i32; 4],
    level: u8,
) -> SymptomDef {
    steady(msg(def(kind, path, name, stats, level, 0, 1.0), 20.0))
}

/// Every symptom type (`subtypesof(/datum/symptom)` without the abstract healing one).
pub const DEFS: [SymptomDef; 48] = [
    cure(
        def(
            K::Beard,
            "/datum/symptom/beard",
            "Facial Hypertrichosis",
            [0, 3, 2, 1],
            5,
            1,
            27.0,
        ),
        Some("/datum/reagent/medicine/mannitol"),
    ),
    cure(
        def(
            K::Bleeding,
            "/datum/symptom/bleeding",
            "Anticoagulant",
            [1, -1, -3, 0],
            8,
            3,
            1.0,
        ),
        Some("/datum/reagent/acetaldehyde"),
    ),
    cure(
        def(
            K::Hypertension,
            "/datum/symptom/hypertension",
            "Hypertension",
            [-2, -1, -1, -1],
            9,
            6,
            52.5,
        ),
        Some("/datum/reagent/inverse/penthrite"),
    ),
    cure(
        def(
            K::Chills,
            "/datum/symptom/chills",
            "Chills",
            [1, 2, 3, 2],
            2,
            2,
            20.0,
        ),
        Some("/datum/reagent/medicine/leporazine"),
    ),
    cure(
        organ(
            msg(
                def(
                    K::Choking,
                    "/datum/symptom/choking",
                    "Choking",
                    [-1, 0, -1, -1],
                    6,
                    3,
                    20.0,
                ),
                15.0,
            ),
            Slot::Lungs,
        ),
        Some("/datum/reagent/medicine/c2/tirimol"),
    ),
    cure(
        organ(
            msg(
                def(
                    K::Asphyxiation,
                    "/datum/symptom/asphyxiation",
                    "Acute respiratory distress syndrome",
                    [-2, 0, -1, -2],
                    9,
                    6,
                    22.0,
                ),
                15.0,
            ),
            Slot::Lungs,
        ),
        Some("/datum/reagent/toxin/bonehurtingjuice"),
    ),
    cure(
        msg(
            def(
                K::Confusion,
                "/datum/symptom/confusion",
                "Confusion",
                [2, 1, 1, 2],
                3,
                2,
                20.0,
            ),
            25.0,
        ),
        Some("/datum/reagent/medicine/synaptizine"),
    ),
    organ(
        msg(
            def(
                K::Cough,
                "/datum/symptom/cough",
                "Cough",
                [-1, 3, 1, 2],
                1,
                1,
                8.5,
            ),
            15.0,
        ),
        Slot::Lungs,
    ),
    cure(
        organ(
            msg(
                def(
                    K::Deafness,
                    "/datum/symptom/deafness",
                    "Deafness",
                    [0, 1, 1, -2],
                    4,
                    4,
                    52.5,
                ),
                100.0,
            ),
            Slot::Ears,
        ),
        Some("/datum/reagent/medicine/psicodine"),
    ),
    cure(
        def(
            K::Disfiguration,
            "/datum/symptom/disfiguration",
            "Disfiguration",
            [2, 0, 3, 1],
            5,
            1,
            50.0,
        ),
        Some("/datum/reagent/consumable/milk"),
    ),
    cure(
        msg(
            def(
                K::Dizzy,
                "/datum/symptom/dizzy",
                "Dizziness",
                [1, 1, 3, 3],
                2,
                2,
                22.5,
            ),
            50.0,
        ),
        Some("/datum/reagent/medicine/haloperidol"),
    ),
    cure(
        msg(
            def(
                K::Fever,
                "/datum/symptom/fever",
                "Fever",
                [0, 3, 3, 2],
                2,
                2,
                20.0,
            ),
            20.0,
        ),
        Some("/datum/reagent/medicine/leporazine"),
    ),
    cure(
        msg(
            def(
                K::Fire,
                "/datum/symptom/fire",
                "Spontaneous Combustion",
                [0, 0, -1, -2],
                6,
                5,
                47.5,
            ),
            20.0,
        ),
        Some("/datum/reagent/medicine/leporazine"),
    ),
    cure(
        msg(
            def(
                K::Alkali,
                "/datum/symptom/alkali",
                "Alkali perspiration",
                [2, -2, -2, -2],
                9,
                6,
                60.0,
            ),
            100.0,
        ),
        Some("/datum/reagent/consumable/frostoil"),
    ),
    cure(
        msg(
            def(
                K::FleshEating,
                "/datum/symptom/flesh_eating",
                "Necrotizing Fasciitis",
                [-2, -1, 0, -1],
                6,
                5,
                37.5,
            ),
            50.0,
        ),
        Some("/datum/reagent/medicine/omnizine"),
    ),
    cure(
        msg(
            def(
                K::FleshDeath,
                "/datum/symptom/flesh_death",
                "Autophagocytosis Necrosis",
                [-2, -2, 1, -2],
                9,
                6,
                4.5,
            ),
            50.0,
        ),
        Some("/datum/reagent/medicine/earthsblood"),
    ),
    cure(
        msg(
            def(
                K::GeneticMutation,
                "/datum/symptom/genetic_mutation",
                "Dormant DNA Activator",
                [-2, -3, 0, -3],
                7,
                4,
                45.0,
            ),
            50.0,
        ),
        Some("/datum/reagent/acetaldehyde"),
    ),
    cure(
        msg(
            def(
                K::Hallucigen,
                "/datum/symptom/hallucigen",
                "Hallucigen",
                [1, -2, 2, -1],
                3,
                2,
                57.5,
            ),
            25.0,
        ),
        Some("/datum/reagent/medicine/psicodine"),
    ),
    cure(
        msg(
            def(
                K::Headache,
                "/datum/symptom/headache",
                "Headache",
                [-1, 4, 2, 0],
                1,
                1,
                22.5,
            ),
            100.0,
        ),
        Some("/datum/reagent/drug/space_drugs"),
    ),
    heal(
        K::HealStarlight,
        "/datum/symptom/heal/starlight",
        "Starlight Condensation",
        [-1, -2, 0, 1],
        8,
    ),
    organ(
        heal(
            K::HealChem,
            "/datum/symptom/heal/chem",
            "Toxolysis",
            [0, -2, 2, -2],
            7,
        ),
        Slot::Heart,
    ),
    organ(
        heal(
            K::HealMetabolism,
            "/datum/symptom/heal/metabolism",
            "Metabolic Boost",
            [-1, -2, 2, 1],
            7,
        ),
        Slot::Stomach,
    ),
    heal(
        K::HealDarkness,
        "/datum/symptom/heal/darkness",
        "Nocturnal Regeneration",
        [2, -1, -2, -1],
        6,
    ),
    heal(
        K::HealComa,
        "/datum/symptom/heal/coma",
        "Regenerative Coma",
        [0, 2, -3, -2],
        8,
    ),
    organ(
        heal(
            K::HealWater,
            "/datum/symptom/heal/water",
            "Tissue Hydration",
            [0, -1, 0, 1],
            4,
        ),
        Slot::Liver,
    ),
    organ(
        heal(
            K::HealPlasma,
            "/datum/symptom/heal/plasma",
            "Plasma Fixation",
            [0, 3, -2, -2],
            8,
        ),
        Slot::Liver,
    ),
    heal(
        K::HealRadiation,
        "/datum/symptom/heal/radiation",
        "Radioactive Resonance",
        [-1, -2, 2, -3],
        7,
    ),
    {
        let mut d = msg(
            heal(
                K::HealAggressive,
                "/datum/symptom/heal/aggressive_healing",
                "Aggressive Healing",
                [-4, 1, 0, -1],
                5,
            ),
            0.0,
        );
        d.severity = 1;
        d
    },
    heal(
        K::HealGenetic,
        "/datum/symptom/heal/genetic",
        "Mutated Regeneration",
        [1, -3, -4, -4],
        9,
    ),
    def(
        K::Itching,
        "/datum/symptom/itching",
        "Itching",
        [0, 3, 3, 1],
        1,
        1,
        15.0,
    ),
    cure(
        def(
            K::Narcolepsy,
            "/datum/symptom/narcolepsy",
            "Narcolepsy",
            [-1, -2, -2, 0],
            4,
            4,
            57.5,
        ),
        Some("/datum/reagent/medicine/ondansetron"),
    ),
    organ(
        steady(msg(
            def(
                K::Oxygen,
                "/datum/symptom/oxygen",
                "Self-Respiration",
                [1, -3, -3, -4],
                7,
                0,
                1.0,
            ),
            3.0,
        )),
        Slot::Lungs,
    ),
    def(
        K::MindRestoration,
        "/datum/symptom/mind_restoration",
        "Mind Restoration",
        [-1, -2, 1, -3],
        6,
        0,
        7.5,
    ),
    steady(msg(
        def(
            K::SensoryRestoration,
            "/datum/symptom/sensory_restoration",
            "Sensory Restoration",
            [0, 1, -2, 2],
            3,
            0,
            1.0,
        ),
        7.0,
    )),
    cure(
        msg(
            def(
                K::Shedding,
                "/datum/symptom/shedding",
                "Alopecia",
                [0, 1, 2, 2],
                5,
                1,
                67.5,
            ),
            50.0,
        ),
        Some("/datum/reagent/barbers_aid"),
    ),
    cure(
        def(
            K::Polyvitiligo,
            "/datum/symptom/polyvitiligo",
            "Polyvitiligo",
            [-1, 3, 1, 2],
            5,
            1,
            10.5,
        ),
        Some("/datum/reagent/water/salt"),
    ),
    {
        let mut d = organ(
            def(
                K::Sneeze,
                "/datum/symptom/sneeze",
                "Sneezing",
                [-2, 3, 0, 4],
                1,
                1,
                20.0,
            ),
            Slot::Lungs,
        );
        d.variation = 0.4;
        d
    },
    immune(cure(
        def(
            K::UndeadAdaptation,
            "/datum/symptom/undead_adaptation",
            "Necrotic Metabolism",
            [2, -2, 1, 0],
            3,
            0,
            1.0,
        ),
        None,
    )),
    immune(cure(
        def(
            K::InorganicAdaptation,
            "/datum/symptom/inorganic_adaptation",
            "Inorganic Biology",
            [-1, 4, -2, 3],
            3,
            0,
            1.0,
        ),
        None,
    )),
    def(
        K::Thermoregulation,
        "/datum/symptom/thermoregulation",
        "Thermoregulation",
        [0, 1, -2, 0],
        4,
        0,
        1.0,
    ),
    cure(
        def(
            K::ViralAdaptation,
            "/datum/symptom/viraladaptation",
            "Viral self-adaptation",
            [3, 5, -3, 0],
            4,
            0,
            1.0,
        ),
        None,
    ),
    cure(
        def(
            K::ViralEvolution,
            "/datum/symptom/viralevolution",
            "Viral evolutionary acceleration",
            [-2, -3, 5, 3],
            4,
            0,
            1.0,
        ),
        None,
    ),
    cure(
        organ(
            msg(
                def(
                    K::VisionLoss,
                    "/datum/symptom/visionloss",
                    "Hyphema",
                    [0, -2, -3, -2],
                    8,
                    5,
                    52.5,
                ),
                50.0,
            ),
            Slot::Eyes,
        ),
        Some("/datum/reagent/medicine/oculine"),
    ),
    cure(
        organ(
            msg(
                def(
                    K::VoiceChange,
                    "/datum/symptom/voice_change",
                    "Voice Change",
                    [2, -1, -2, 2],
                    5,
                    2,
                    90.0,
                ),
                100.0,
            ),
            Slot::Tongue,
        ),
        Some("/datum/reagent/inverse/healing/convermol"),
    ),
    cure(
        organ(
            msg(
                def(
                    K::Vomit,
                    "/datum/symptom/vomit",
                    "Vomiting",
                    [-3, 2, 0, 3],
                    3,
                    3,
                    52.5,
                ),
                100.0,
            ),
            Slot::Stomach,
        ),
        Some("/datum/reagent/medicine/ondansetron"),
    ),
    {
        let mut d = cure(
            organ(
                msg(
                    def(
                        K::VomitNebula,
                        "/datum/symptom/vomit/nebula",
                        "Nebula Vomiting",
                        [-3, 2, 0, 3],
                        3,
                        3,
                        52.5,
                    ),
                    100.0,
                ),
                Slot::Stomach,
            ),
            Some("/datum/reagent/medicine/ondansetron"),
        );
        d.natural = false;
        d
    },
    cure(
        organ(
            msg(
                def(
                    K::WeightLoss,
                    "/datum/symptom/weight_loss",
                    "Weight Loss",
                    [0, 2, -2, -2],
                    7,
                    3,
                    30.0,
                ),
                100.0,
            ),
            Slot::Stomach,
        ),
        Some("/datum/reagent/medicine/diphenhydramine"),
    ),
    immune(cure(
        msg(
            def(
                K::Youth,
                "/datum/symptom/youth",
                "Eternal Youth",
                [3, 4, 4, -4],
                6,
                0,
                37.5,
            ),
            100.0,
        ),
        None,
    )),
];

/// The English names of the symptom cures (the analyzer's and pandemic's cure text).
const CURE_NAMES: [(&str, &str); 22] = [
    (SPACEACILLIN, "Spaceacillin"),
    ("/datum/reagent/medicine/mannitol", "Mannitol"),
    ("/datum/reagent/acetaldehyde", "Acetaldehyde"),
    ("/datum/reagent/inverse/penthrite", "Nooartrium"),
    ("/datum/reagent/medicine/leporazine", "Leporazine"),
    ("/datum/reagent/medicine/c2/tirimol", "Tirimol"),
    (
        "/datum/reagent/toxin/bonehurtingjuice",
        "Bone Hurting Juice",
    ),
    ("/datum/reagent/medicine/synaptizine", "Synaptizine"),
    ("/datum/reagent/medicine/psicodine", "Psicodine"),
    ("/datum/reagent/consumable/milk", "Milk"),
    ("/datum/reagent/medicine/haloperidol", "Haloperidol"),
    ("/datum/reagent/consumable/frostoil", "Frost Oil"),
    ("/datum/reagent/medicine/omnizine", "Omnizine"),
    ("/datum/reagent/medicine/earthsblood", "Earthsblood"),
    ("/datum/reagent/drug/space_drugs", "Space Drugs"),
    ("/datum/reagent/medicine/ondansetron", "Ondansetron"),
    ("/datum/reagent/barbers_aid", "Barber's Aid"),
    ("/datum/reagent/water/salt", "Saltwater"),
    ("/datum/reagent/medicine/oculine", "Oculine"),
    ("/datum/reagent/inverse/healing/convermol", "Coveroli"),
    ("/datum/reagent/medicine/diphenhydramine", "Diphenhydramine"),
    ("/datum/reagent/medicine/c2/convermol", "Convermol"),
];

impl SymptomKind {
    pub fn def(self) -> &'static SymptomDef {
        &DEFS[self as usize]
    }

    /// The type of a DM path (`/datum/symptom/...`).
    pub fn from_path(path: &str) -> Option<Self> {
        DEFS.iter().find(|d| d.path == path).map(|d| d.kind)
    }
}

/// One symptom of a disease: what tg copies (type, neutered) and its live state.
#[derive(Clone, Debug, PartialEq)]
pub struct Symptom {
    pub kind: SymptomKind,
    /// Stats only, no effect.
    pub neutered: bool,
    /// Its cure is in the host's blood: no effect, counts towards the cure.
    pub remedied: bool,
    next_activation: f64,
    power: f32,
    delay: f32,
    message_chance: f32,
    suppress: bool,
    /// The symptom's first and second threshold switches (see `start`).
    flag: bool,
    flag2: bool,
    cooldown_until: f64,
    off_cooldown: bool,
}

impl Symptom {
    pub fn new(kind: SymptomKind) -> Self {
        let def = kind.def();
        Self {
            kind,
            neutered: false,
            remedied: false,
            next_activation: 0.0,
            power: 1.0,
            delay: def.delay,
            message_chance: def.message_chance,
            suppress: false,
            flag: false,
            flag2: false,
            cooldown_until: 0.0,
            off_cooldown: false,
        }
    }

    /// tg `Copy`: the type and `neutered`.
    fn copy(&self) -> Self {
        Self {
            neutered: self.neutered,
            ..Self::new(self.kind)
        }
    }

    /// tg `id`: the index of the type, `N` when neutered.
    fn id(&self) -> String {
        let n = if self.neutered { "N" } else { "" };
        format!("{}{n}", self.kind as usize + 1)
    }
}

/// tg `properties`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Props {
    pub resistance: i32,
    pub stealth: i32,
    pub stage_rate: i32,
    pub transmittable: i32,
    pub severity: i32,
}

/// The advanced part of a disease.
#[derive(Clone, Debug, PartialEq)]
pub struct Advance {
    pub symptoms: Vec<Symptom>,
    pub props: Props,
    /// The symptoms have started (`processing`).
    processing: bool,
}

impl Advance {
    /// tg `GetDiseaseID`: the sorted symptom ids.
    pub fn id(&self) -> String {
        let mut ids: Vec<String> = self.symptoms.iter().map(Symptom::id).collect();
        ids.sort();
        ids.join(":")
    }

    pub(crate) fn copy(&self) -> Self {
        Self {
            symptoms: self.symptoms.iter().map(Symptom::copy).collect(),
            props: self.props,
            processing: false,
        }
    }

    pub fn has(&self, kind: SymptomKind) -> bool {
        self.symptoms.iter().any(|s| s.kind == kind)
    }

    /// tg `has_cure`: the remedied share of the symptoms that are not neutered.
    pub fn cure_status(&self) -> f32 {
        let remedied = self.symptoms.iter().filter(|s| s.remedied).count();
        let active = self
            .symptoms
            .iter()
            .filter(|s| !s.remedied && !s.neutered)
            .count();
        if remedied + active == 0 {
            return 0.0;
        }
        remedied as f32 / (remedied + active) as f32
    }

    /// tg `generate_cure_text(cure_count)`.
    pub fn cure_text(&self, count: usize) -> String {
        let mut remedies: Vec<&str> = Vec::new();
        for symptom in &self.symptoms {
            if remedies.len() >= count {
                break;
            }
            let Some(path) = symptom.kind.def().cure else {
                continue;
            };
            let name = CURE_NAMES
                .iter()
                .find(|(p, _)| *p == path)
                .map_or(path, |&(_, n)| n);
            if !symptom.neutered && !remedies.contains(&name) {
                remedies.push(name);
            }
        }
        // DM `english_list(nothing_text = "Nothing")`.
        match remedies.as_slice() {
            [] => "Nothing".to_owned(),
            [one] => (*one).to_owned(),
            [init @ .., last] => format!("{} and {last}", init.join(", ")),
        }
    }

    /// The symptoms a beaten host becomes resistant to (`cure`).
    pub(crate) fn resisted_symptoms(&self) -> Vec<&'static str> {
        self.symptoms
            .iter()
            .filter(|s| !s.neutered && !s.kind.def().immunity_proof)
            .map(|s| s.kind.def().name)
            .collect()
    }

    /// tg `get_immunity_recovery`.
    pub(crate) fn immunity_recovery(&self, resisted: &[&'static str]) -> f32 {
        self.symptoms
            .iter()
            .filter(|s| !s.neutered && resisted.contains(&s.kind.def().name))
            .count() as f32
            * diseases::SYMPTOM_IMMUNITY_RECOVERY_BONUS
    }

    /// tg `AddSymptom`: a new type; over the limit a random one makes room.
    fn add(&mut self, symptom: Symptom, rng: &mut CombatRng) {
        if self.has(symptom.kind) {
            return;
        }
        if self.symptoms.len() >= SYMPTOM_LIMIT {
            let i = rng.range(0, self.symptoms.len() as u32 - 1) as usize;
            self.symptoms.remove(i);
        }
        self.symptoms.push(symptom);
    }

    /// tg `GenerateProperties`.
    fn generate_properties(&mut self) {
        let mut p = Props::default();
        for s in &self.symptoms {
            let d = s.kind.def();
            p.resistance += d.resistance;
            p.stealth += d.stealth;
            p.stage_rate += d.stage_speed;
            p.transmittable += d.transmittable;
            if !s.neutered {
                p.severity += d.severity;
            }
        }
        if p.severity > 0 {
            let round = |x: f32| (x + 0.5).floor();
            let mut severity = p.severity as f32;
            severity += round(p.resistance as f32 / 12.0);
            severity += round(p.stage_rate as f32 / 11.0);
            severity += round(p.transmittable as f32 / 8.0);
            severity = round(severity / 2.0);
            severity *= self.symptoms.len() as f32 / SYMPTOM_LIMIT as f32;
            p.severity = round(severity.clamp(1.0, 7.0)) as i32;
        }
        self.props = p;
    }
}

impl Disease {
    /// A new `/datum/disease/advance` named `name` with these symptoms (tg `New` -> `Refresh`).
    pub fn advanced(name: &str, kinds: &[SymptomKind]) -> Self {
        let mut disease = Self::new(Kind::Advance);
        disease.name = name.to_owned();
        let mut advance = Advance {
            symptoms: Vec::new(),
            props: Props::default(),
            processing: false,
        };
        for &kind in kinds {
            if !advance.has(kind) && advance.symptoms.len() < SYMPTOM_LIMIT {
                advance.symptoms.push(Symptom::new(kind));
            }
        }
        disease.advance = Some(advance);
        disease.refresh();
        disease
    }

    /// tg `/datum/disease/advance/random` (virus crates, outbreaks): `max_symptoms` (1 to 6 by
    /// default) different symptoms of levels 1 to `max_level`, named `Sample #N`.
    pub fn random_advance(rng: &mut CombatRng, max_symptoms: Option<usize>, max_level: u8) -> Self {
        let count = max_symptoms.unwrap_or_else(|| rng.range(1, SYMPTOM_LIMIT as u32) as usize);
        let mut pool: Vec<SymptomKind> = DEFS
            .iter()
            .filter(|d| d.level > 0 && d.level <= max_level)
            .map(|d| d.kind)
            .collect();
        let mut kinds = Vec::new();
        for _ in 0..count {
            if pool.is_empty() {
                break;
            }
            let i = rng.range(0, pool.len() as u32 - 1) as usize;
            kinds.push(pool.swap_remove(i));
        }
        let name = format!("Sample #{}", rng.range(1, 10000));
        Self::advanced(&name, &kinds)
    }

    /// tg `Refresh` -> `GenerateProperties`, `assign_properties`: the stats decide visibility,
    /// spread, cure and stage speed and severity.
    pub(crate) fn refresh(&mut self) {
        let Some(advance) = self.advance.as_mut() else {
            return;
        };
        advance.generate_properties();
        let p = advance.props;
        if p.stealth >= p.severity && p.severity > 0 {
            self.visibility |= HIDDEN_SCANNER;
        } else {
            self.visibility &= !HIDDEN_SCANNER;
        }
        let (flags, text) = if p.transmittable >= 11 {
            (
                SPREAD_BLOOD | SPREAD_FLUIDS | SPREAD_SKIN | SPREAD_AIRBORNE,
                "Respiration",
            )
        } else if p.transmittable >= 7 {
            (SPREAD_BLOOD | SPREAD_FLUIDS | SPREAD_SKIN, "Skin contact")
        } else if p.transmittable >= 3 {
            (SPREAD_BLOOD | SPREAD_FLUIDS, "Fluids")
        } else {
            (SPREAD_BLOOD, "Blood")
        };
        self.spread_flags = flags;
        self.spread_text = text.to_owned();
        self.spreading_modifier = (0.4 * p.transmittable as f32).ceil().max(1.0);
        self.cure_chance = (10.0 * 0.94f32.powi(p.resistance)).clamp(3.5, 12.0);
        self.stage_prob = (0.2 * p.stage_rate as f32).max(0.0) + 1.5;
        self.severity = Severity::from_level(p.severity);
    }

    /// tg `Mix`: `other`'s symptoms join (in random order) unless it is the same disease.
    fn mix(&mut self, other: &Self, rng: &mut CombatRng) {
        if self.is_same(other) {
            return;
        }
        let (Some(mine), Some(theirs)) = (self.advance.as_mut(), other.advance.as_ref()) else {
            return;
        };
        let mut incoming: Vec<Symptom> = theirs.symptoms.iter().map(Symptom::copy).collect();
        while !incoming.is_empty() {
            let i = rng.range(0, incoming.len() as u32 - 1) as usize;
            mine.add(incoming.swap_remove(i), rng);
        }
    }
}

/// tg `Advance_Mix`: copies of the advanced diseases mixed pairwise into one.
pub fn advance_mix(list: Vec<Disease>, rng: &mut CombatRng) -> Option<Disease> {
    let mut all: Vec<Disease> = list
        .iter()
        .filter(|d| d.advance.is_some())
        .map(Disease::copy)
        .collect();
    let mut rounds = 0;
    while rounds < 20 && all.len() > 1 {
        rounds += 1;
        let i = rng.range(0, all.len() as u32 - 1) as usize;
        let first = all.swap_remove(i);
        let j = rng.range(0, all.len() as u32 - 1) as usize;
        all[j].mix(&first, rng);
    }
    let mut result = all.pop()?;
    result.refresh();
    Some(result)
}

fn rand_delay(rng: &mut CombatRng, delay: f32, variation: f32) -> f64 {
    let (low, high) = (delay * (1.0 - variation), delay * (1.0 + variation));
    f64::from(low + rng.unit() * (high - low))
}

/// tg `Start` of a symptom: its threshold switches from the disease's properties.
fn start(s: &mut Symptom, p: Props, resisted: bool, now: f64, rng: &mut CombatRng) {
    if s.neutered {
        return;
    }
    let def = s.kind.def();
    s.power = 1.0;
    s.delay = def.delay;
    s.message_chance = def.message_chance;
    s.suppress = false;
    s.flag = false;
    s.flag2 = false;
    if resisted {
        s.delay *= 1.75;
    }
    match s.kind {
        K::Cough => {
            s.suppress = p.stealth >= 4;
            s.flag = p.transmittable >= 7;
            if p.resistance >= 11 {
                s.power = 1.5;
            }
            if p.resistance >= 15 {
                s.power = 2.0;
            }
            if p.stage_rate >= 6 {
                s.delay = 6.0;
            }
        }
        K::Sneeze => {
            s.flag = p.transmittable >= 9;
            s.suppress = p.stealth >= 4;
        }
        K::Headache => {
            if p.stealth >= 4 {
                s.message_chance = 50.0;
            }
            if p.stage_rate >= 6 {
                s.power = 2.0;
            }
            if p.stage_rate >= 9 {
                s.delay = 45.0;
                s.power = 3.0;
            }
        }
        K::Itching => {
            if p.transmittable >= 6 {
                s.delay = 2.5;
            }
            s.flag = p.stage_rate >= 7;
        }
        K::Fever => {
            if p.resistance >= 5 {
                s.power = 1.5;
                s.flag = true;
            }
            if p.resistance >= 10 {
                s.power = 2.5;
            }
        }
        K::Chills => {
            if p.stage_rate >= 5 {
                s.power = 1.5;
                s.flag = true;
            }
            if p.stage_rate >= 10 {
                s.power = 2.5;
            }
        }
        K::Confusion => {
            s.flag = p.resistance >= 6;
            if p.transmittable >= 6 {
                s.power = 1.5;
            }
            s.suppress = p.stealth >= 4;
        }
        K::Dizzy => s.suppress = p.stealth >= 4,
        K::Narcolepsy => {
            // Yawning.
            s.flag = p.transmittable >= 4;
            if p.stage_rate >= 10 {
                s.delay = 32.5;
            }
        }
        K::Vomit | K::VomitNebula => {
            s.suppress = p.stealth >= 4;
            s.flag = p.resistance >= 9;
            s.flag2 = p.transmittable >= 9;
        }
        K::WeightLoss => {
            if p.stealth >= 4 {
                s.message_chance = 25.0;
            }
        }
        K::Choking => {
            if p.stage_rate >= 8 {
                s.delay = 15.5;
            }
            s.suppress = p.stealth >= 4;
        }
        K::Asphyxiation => {
            if p.transmittable >= 8 {
                s.power = 2.0;
            }
        }
        K::FleshEating => s.flag2 = p.transmittable >= 8,
        K::FleshDeath => s.suppress = p.stealth >= 5,
        K::Fire => {
            if p.stage_rate >= 4 {
                s.power = 1.5;
            }
            if p.stage_rate >= 8 {
                s.power = 2.0;
            }
            s.suppress = p.stealth >= 4;
            s.flag = p.transmittable >= 8;
        }
        K::HealChem => {
            s.flag = p.stage_rate >= 6;
            if p.resistance >= 7 {
                s.power = 2.0;
            }
        }
        K::HealAggressive => s.power = 2.0,
        K::HealRadiation => {
            if p.resistance >= 7 {
                s.power = 2.0;
            }
        }
        K::Oxygen => s.flag = p.resistance >= 8,
        K::MindRestoration => {
            s.flag = p.resistance >= 6;
            s.flag2 = p.resistance >= 9;
            // Purges alcohol.
            s.suppress = p.transmittable >= 8;
        }
        _ => {}
    }
    s.next_activation = now + rand_delay(rng, s.delay, def.variation);
}

/// tg base `Activate`: the organ is there, the cure is not, the timer is up.
fn ready(world: &World, mob: Entity, s: &mut Symptom, now: f64, rng: &mut CombatRng) -> bool {
    if s.neutered {
        return false;
    }
    let def = s.kind.def();
    if let Some(slot) = def.organ
        && !diseases::has_organ(world, mob, slot)
    {
        return false;
    }
    if let Some(cure) = def.cure {
        if diseases::blood_has(world, mob, cure, false) {
            s.remedied = true;
            return false;
        }
        s.remedied = false;
    }
    if now < s.next_activation {
        return false;
    }
    s.next_activation = now + rand_delay(rng, s.delay, def.variation);
    true
}

/// tg advanced `stage_act` after the base one let it act: the symptoms start on the first
/// tick and act on their timers.
pub(crate) fn act(
    world: &mut World,
    mob: Entity,
    disease: &mut Disease,
    rng: &mut CombatRng,
) -> Outcome {
    let now = world
        .get_resource::<Time>()
        .map_or(0.0, Time::elapsed_secs_f64);
    let resisted: Vec<&'static str> = world
        .get::<diseases::Diseases>(mob)
        .map(|d| d.symptom_resistances.clone())
        .unwrap_or_default();
    let snapshot = disease.clone();
    let stage = disease.stage;
    let Some(advance) = disease.advance.as_mut() else {
        return Outcome::Keep;
    };
    let props = advance.props;
    if !advance.processing {
        advance.processing = true;
        for s in &mut advance.symptoms {
            let name = s.kind.def().name;
            start(s, props, resisted.contains(&name), now, rng);
        }
    }
    for s in &mut advance.symptoms {
        if world.get_entity(mob).is_err() {
            return Outcome::Gone;
        }
        if ready(world, mob, s, now, rng) {
            activate(world, mob, &snapshot, s, stage, props, now, rng);
        }
    }
    Outcome::Keep
}

fn pick<'a>(rng: &mut CombatRng, lines: &[&'a str]) -> &'a str {
    lines[rng.range(0, lines.len() as u32 - 1) as usize]
}

/// The "emote off cooldown" rule of cough, sneeze and itching: on cooldown it still goes off
/// 60% of the time, every other time.
fn cooldown_emote(s: &mut Symptom, now: f64, rng: &mut CombatRng) -> bool {
    let ready = now >= s.cooldown_until || (rng.chance(60.0) && !s.off_cooldown);
    if ready {
        s.cooldown_until = now + 5.0;
        s.off_cooldown = !s.off_cooldown;
    }
    ready
}

#[allow(clippy::too_many_arguments)]
fn activate(
    world: &mut World,
    mob: Entity,
    disease: &Disease,
    s: &mut Symptom,
    stage: u8,
    p: Props,
    now: f64,
    rng: &mut CombatRng,
) {
    let tell = |world: &mut World, span: Span, text: &str| say_to(world, mob, span, text);
    match s.kind {
        K::Cough => {
            if stage <= 3 {
                if rng.chance(s.message_chance) && !s.suppress {
                    let line = pick(rng, &["You swallow excess mucus.", "You lightly cough."]);
                    tell(world, Span::Warning, line);
                }
                return;
            }
            if cooldown_emote(s, now, rng) {
                emote(world, mob, "cough");
            }
            airborne_spread(
                world,
                mob,
                disease,
                if s.flag { 2 } else { 1 },
                true,
                false,
                rng,
            );
            if s.power >= 1.5 {
                diseases::drop_held(world, mob, false, true);
            }
            if s.power >= 2.0 && rng.chance(30.0) {
                let line = pick(
                    rng,
                    &["You have a coughing fit!", "You can't stop coughing!"],
                );
                tell(world, Span::BoldWarning, line);
                crate::status_effects::apply(world, mob, Incap::Immobilize, 2.0);
                // shortcut: tg spaces these by 0.6 s timers.
                for _ in 0..3 {
                    emote(world, mob, "cough");
                }
            }
        }
        K::Sneeze => {
            if stage <= 3 {
                if !s.suppress {
                    emote(world, mob, "sniff");
                }
                return;
            }
            emote(world, mob, "sneeze");
            airborne_spread(
                world,
                mob,
                disease,
                if s.flag { 6 } else { 4 },
                true,
                true,
                rng,
            );
        }
        K::Headache => {
            if s.power < 2.0 && (rng.chance(s.message_chance) || stage >= 4) {
                let line = pick(rng, &["Your head hurts.", "Your head pounds."]);
                tell(world, Span::Warning, line);
            }
            if s.power >= 2.0 && stage >= 4 {
                let line = pick(
                    rng,
                    &["Your head hurts a lot.", "Your head pounds incessantly."],
                );
                tell(world, Span::Warning, line);
                crate::status_effects::adjust_stamina(world, mob, 25.0);
            }
            if s.power >= 3.0 && stage >= 5 {
                let line = pick(
                    rng,
                    &[
                        "Your head hurts!",
                        "You feel a burning knife inside your brain!",
                        "A wave of pain fills your head!",
                    ],
                );
                tell(world, Span::BoldWarning, line);
                crate::status_effects::apply(world, mob, Incap::Stun, 3.5);
            }
        }
        K::Itching => {
            // tg `itch(silent, can_scratch)`: 0.5 brute on the side when scratching.
            if !cooldown_emote(s, now, rng) {
                return;
            }
            let incapacitated = crate::status_effects::incapacitation(world, mob).incapacitated;
            if s.flag && !incapacitated {
                damage(world, mob, DamageType::Brute, 0.5);
            }
            let name = ss13_chat::speech::capitalize(&super::name_of(world, mob));
            let (own, others) = if s.flag {
                (
                    "Вы хотите почесать бок. И успешно чешетесь.".to_owned(),
                    format!("{name} чешет у себя бок."),
                )
            } else {
                ("Вы хотите почесать бок. ".to_owned(), String::new())
            };
            super::visible(world, mob, &own, &others, Span::Warning);
        }
        K::Fever | K::Chills => {
            let fever = s.kind == K::Fever;
            let unsafe_ = s.flag;
            let line = match (fever, unsafe_ && stage >= 4) {
                (true, false) => pick(rng, &["You feel hot.", "You feel like you're burning."]),
                (true, true) => pick(
                    rng,
                    &["You feel too hot.", "You feel like your blood is boiling."],
                ),
                (false, false) => pick(rng, &["You feel cold.", "You shiver."]),
                (false, true) => pick(
                    rng,
                    &[
                        "You feel your blood run cold.",
                        "You feel ice in your veins.",
                        "You feel like you can't heat up.",
                        "You shiver violently.",
                    ],
                ),
            };
            let span = if unsafe_ && stage >= 4 {
                Span::BoldWarning
            } else {
                Span::Warning
            };
            tell(world, span, line);
            // tg `add_body_temperature_change`; a safe fever stays 5 K short of the damage
            // limits.
            use crate::breath::{
                BODYTEMP_COLD_DAMAGE_LIMIT, BODYTEMP_HEAT_DAMAGE_LIMIT, BODYTEMP_NORMAL,
            };
            let change = 6.0 * f64::from(s.power) * f64::from(stage);
            if let Some(mut r) = world.get_mut::<crate::breath::Respiration>(mob) {
                if fever {
                    let change = if unsafe_ {
                        change
                    } else {
                        change.min(BODYTEMP_HEAT_DAMAGE_LIMIT - 5.0 - BODYTEMP_NORMAL)
                    };
                    r.bodytemperature = r.bodytemperature.max(BODYTEMP_NORMAL + change);
                } else {
                    let change = if unsafe_ {
                        change
                    } else {
                        change.min(BODYTEMP_NORMAL - BODYTEMP_COLD_DAMAGE_LIMIT - 5.0)
                    };
                    r.bodytemperature = r.bodytemperature.min(BODYTEMP_NORMAL - change);
                }
            }
        }
        K::Confusion => {
            if stage <= 4 {
                if rng.chance(s.message_chance) && !s.suppress {
                    let line = pick(rng, &["Your head hurts.", "Your mind blanks for a moment."]);
                    tell(world, Span::Warning, line);
                }
                return;
            }
            tell(world, Span::BoldWarning, "You can't think straight!");
            diseases::confusion_up_to(world, mob, 16.0 * s.power, 30.0);
            if s.flag {
                organs::adjust_organ_loss_up_to(world, mob, Slot::Brain, 3.0 * s.power, 80.0);
            }
        }
        K::Dizzy => {
            if stage <= 4 {
                if rng.chance(s.message_chance) && !s.suppress {
                    let line = pick(rng, &["You feel dizzy.", "Your head spins."]);
                    tell(world, Span::Warning, line);
                }
                return;
            }
            tell(
                world,
                Span::BoldWarning,
                "A wave of dizziness washes over you!",
            );
            crate::status_effects::adjust_timed(
                world,
                mob,
                crate::status_effects::Timed::Dizziness,
                40.0,
                Some(80.0),
            );
        }
        K::Narcolepsy => {
            let (line, drowsy) = match stage {
                1 => ("You feel tired.", 0.0),
                2 => ("You feel very tired.", 0.0),
                3 => ("You try to focus on staying awake.", 10.0),
                4 if s.flag => ("You try and fail to suppress a yawn.", 20.0),
                4 => ("You nod off for a moment.", 20.0),
                _ => (
                    pick(
                        rng,
                        &[
                            "So tired...",
                            "You feel very sleepy.",
                            "You have a hard time keeping your eyes open.",
                            "You try to stay awake.",
                        ],
                    ),
                    80.0,
                ),
            };
            if rng.chance(50.0) {
                tell(world, Span::Warning, line);
            }
            if drowsy > 0.0 {
                crate::status_effects::adjust_timed(
                    world,
                    mob,
                    crate::status_effects::Timed::Drowsiness,
                    drowsy,
                    Some(140.0),
                );
            }
            if stage >= 4 && s.flag {
                emote(world, mob, "yawn");
                airborne_spread(world, mob, disease, 6, true, false, rng);
            }
        }
        K::Vomit | K::VomitNebula => {
            if stage <= 4 {
                if rng.chance(s.message_chance) && !s.suppress {
                    let line = pick(
                        rng,
                        &[
                            "You feel nauseated.",
                            "You feel like you're going to throw up!",
                        ],
                    );
                    tell(world, Span::Warning, line);
                }
                return;
            }
            let nebula = s.kind == K::VomitNebula;
            let lost = if nebula { 10.0 } else { 20.0 };
            let distance = if s.flag2 { 5 } else { 1 };
            diseases::vomit(world, mob, lost, !nebula, s.flag, distance);
        }
        K::WeightLoss => {
            if stage <= 4 {
                if rng.chance(s.message_chance) {
                    let line = pick(rng, &["You feel hungry.", "You crave for food."]);
                    tell(world, Span::Warning, line);
                }
                return;
            }
            let line = pick(
                rng,
                &[
                    "So hungry...",
                    "You'd kill someone for a bite of food...",
                    "Hunger cramps seize you...",
                ],
            );
            tell(world, Span::Warning, line);
            adjust_nutrition(world, mob, -100.0, -200.0);
        }
        K::Choking => match stage {
            1 | 2 => {
                if rng.chance(s.message_chance) && !s.suppress {
                    let line = pick(
                        rng,
                        &[
                            "You're having difficulty breathing.",
                            "Your breathing becomes heavy.",
                        ],
                    );
                    tell(world, Span::Warning, line);
                }
            }
            3 | 4 => {
                let line = if s.suppress {
                    match rng.range(0, 2) {
                        0 => "You feel very dizzy.",
                        1 => "You feel very woozy.",
                        _ => "You feel very faint.",
                    }
                } else {
                    pick(
                        rng,
                        &[
                            "Your windpipe feels like a straw.",
                            "Your breathing becomes tremendously difficult.",
                        ],
                    )
                };
                tell(world, Span::Warning, line);
                damage(world, mob, DamageType::Oxy, rng.range(6, 13) as f32);
                emote(world, mob, "gasp");
            }
            _ => {
                let line = pick(rng, &["You're choking!", "You can't breathe!"]);
                tell(world, Span::BoldWarning, line);
                damage(world, mob, DamageType::Oxy, rng.range(10, 18) as f32);
                emote(world, mob, "gasp");
            }
        },
        K::Asphyxiation => match stage {
            3 | 4 => {
                let line = pick(
                    rng,
                    &["Your windpipe feels thin.", "Your lungs feel small."],
                );
                tell(world, Span::Warning, line);
                damage(
                    world,
                    mob,
                    DamageType::Oxy,
                    rng.range(10, 15) as f32 * s.power,
                );
                emote(world, mob, "gasp");
            }
            5 => {
                let line = pick(rng, &["Your lungs hurt!", "It hurts to breathe!"]);
                tell(world, Span::BoldWarning, line);
                damage(
                    world,
                    mob,
                    DamageType::Oxy,
                    rng.range(15, 21) as f32 * s.power,
                );
                emote(world, mob, "gasp");
                let (oxy, max) = world
                    .get::<Health>(mob)
                    .map_or((0.0, 100.0), |h| (h.oxy, h.max));
                if oxy >= max / (200.0 / 120.0) {
                    let name = super::name_of(world, mob);
                    super::visible(
                        world,
                        mob,
                        "",
                        &format!(
                            "{name} stops breathing, as if their lungs have totally collapsed!"
                        ),
                        Span::Warning,
                    );
                    let hurt = rng.range(25, 35) as f32 * s.power;
                    damage(world, mob, DamageType::Oxy, hurt);
                    organs::adjust_organ_loss(world, mob, Slot::Brain, hurt / 2.0);
                }
            }
            _ => {}
        },
        K::FleshEating => match stage {
            2 | 3 => {
                if rng.chance(s.message_chance) {
                    let line = pick(
                        rng,
                        &[
                            "You feel a sudden pain across your body.",
                            "Drops of blood appear suddenly on your skin.",
                        ],
                    );
                    tell(world, Span::Warning, line);
                }
            }
            4 | 5 => {
                let line = pick(
                    rng,
                    &[
                        "You cringe as a violent pain takes over your body.",
                        "It feels like your body is eating itself inside out.",
                        "IT HURTS.",
                    ],
                );
                tell(world, Span::BoldWarning, line);
                let hurt = rng.range(15, 25) as f32 * s.power;
                damage(world, mob, DamageType::Brute, hurt);
                if s.flag2 {
                    crate::status_effects::adjust_stamina(world, mob, hurt * 2.0);
                }
            }
            _ => {}
        },
        K::FleshDeath => match stage {
            2 | 3 => {
                if rng.chance(s.message_chance) && !s.suppress {
                    let line = pick(
                        rng,
                        &[
                            "You feel your body break apart.",
                            "Your skin rubs off like dust.",
                        ],
                    );
                    tell(world, Span::Warning, line);
                }
            }
            4 | 5 => {
                if rng.chance(s.message_chance / 2.0) {
                    let line = pick(
                        rng,
                        &[
                            "You feel your muscles weakening.",
                            "Some of your skin detaches itself.",
                            "You feel sandy.",
                        ],
                    );
                    tell(world, Span::BoldWarning, line);
                }
                damage(world, mob, DamageType::Brute, rng.range(6, 10) as f32);
            }
            _ => {}
        },
        K::Fire => fire(world, mob, disease, s, stage, rng),
        // `/datum/symptom/heal/radiation`: heals the irradiated at stages 4 and 5.
        K::HealRadiation => {
            if stage < 4 {
                return;
            }
            if crate::radiation::is_irradiated(world, mob) {
                damage(world, mob, DamageType::Tox, -2.0 * s.power);
                damage(world, mob, DamageType::Brute, -s.power);
                damage(world, mob, DamageType::Burn, -s.power);
            } else if rng.chance(2.0) {
                tell(world, Span::Notice, "Your skin glows faintly for a moment.");
            }
        }
        K::HealChem | K::HealAggressive => {
            if stage < 4 {
                return;
            }
            if s.kind == K::HealAggressive {
                let amount = s.power + p.severity as f32 * 0.25;
                damage(world, mob, DamageType::Brute, -amount);
                damage(world, mob, DamageType::Burn, -amount);
                return;
            }
            let removed = diseases::remove_blood(world, mob, None, s.power);
            if s.flag {
                adjust_nutrition(world, mob, 0.3 * removed, 0.0);
            }
            if removed > 0.0 && rng.chance(2.0) {
                tell(
                    world,
                    Span::Notice,
                    "You feel a mild warmth as your blood purifies itself.",
                );
            }
        }
        K::Oxygen => {
            if stage < 4 {
                if rng.chance(s.message_chance) {
                    tell(world, Span::Notice, "Your lungs feel great.");
                }
                return;
            }
            if let Some(mut r) = world.get_mut::<crate::breath::Respiration>(mob) {
                r.losebreath = (r.losebreath - 4.0).max(0.0);
            }
            damage(world, mob, DamageType::Oxy, -7.0);
            if rng.chance(s.message_chance) {
                tell(
                    world,
                    Span::Notice,
                    "You realize you haven't been breathing.",
                );
            }
            if s.flag
                && let Some(mut blood) = world.get_mut::<super::blood::Blood>(mob)
            {
                blood.adjust(1.0, 0.0, super::blood::BLOOD_VOLUME_NORMAL);
            }
        }
        K::MindRestoration => {
            if stage >= 3 {
                if let Some(mut effects) =
                    world.get_mut::<crate::status_effects::StatusEffects>(mob)
                {
                    use crate::status_effects::Timed;
                    effects.adjust_timed(Timed::Dizziness, -4.0, None);
                    effects.adjust_timed(Timed::Drowsiness, -4.0, None);
                    // Every slurring effect loses a second.
                    for slur in &mut effects.slurring {
                        *slur = (*slur - 1.0).max(0.0);
                    }
                    effects.confusion = (effects.confusion - 2.0).max(0.0);
                    if stage >= 4 {
                        effects.adjust_timed(Timed::Drowsiness, -4.0, None);
                    }
                }
                if s.suppress {
                    diseases::remove_blood(
                        world,
                        mob,
                        Some("/datum/reagent/consumable/ethanol"),
                        3.0,
                    );
                    if let Some(mut m) = world.get_mut::<crate::chem::MobReagents>(mob) {
                        m.drunk = (m.drunk - 5.0).max(0.0);
                    }
                }
            }
            if stage >= 4 {
                diseases::remove_blood(world, mob, Some("/datum/reagent/toxin/mindbreaker"), 5.0);
                diseases::remove_blood(world, mob, Some("/datum/reagent/toxin/histamine"), 5.0);
            }
            if stage >= 5 {
                organs::adjust_organ_loss(world, mob, Slot::Brain, -3.0);
                if s.flag
                    && rng.chance(10.0)
                    && let Some(brain) = organs::organ(world, mob, Slot::Brain)
                {
                    use super::traumas::Resilience;
                    let level = if s.flag2 {
                        Resilience::Surgery
                    } else {
                        Resilience::Basic
                    };
                    super::traumas::cure_one(world, brain, level, rng);
                }
            }
        }
        K::SensoryRestoration => {
            if stage < 4 {
                if rng.chance(s.message_chance) {
                    let line = pick(
                        rng,
                        &[
                            "Your eyes feel great.",
                            "You feel like your eyes can focus more clearly.",
                            "You don't feel the need to blink.",
                            "Your ears feel great.",
                            "Your hearing feels more acute.",
                        ],
                    );
                    tell(world, Span::Notice, line);
                }
                return;
            }
            organs::adjust_organ_loss(world, mob, Slot::Ears, -4.0);
            organs::adjust_organ_loss(world, mob, Slot::Eyes, -2.0);
        }
        _ => {}
    }
}

/// `/datum/symptom/fire/Activate`.
fn fire(
    world: &mut World,
    mob: Entity,
    disease: &Disease,
    s: &Symptom,
    stage: u8,
    rng: &mut CombatRng,
) {
    let warn = |world: &mut World, rng: &mut CombatRng| {
        let line = if rng.chance(33.33) {
            "You hear a crackling noise."
        } else {
            pick(rng, &["You feel hot.", "You smell smoke."])
        };
        say_to(world, mob, Span::Warning, line);
    };
    match stage {
        0..=2 => {}
        3 => {
            if rng.chance(s.message_chance) && !s.suppress {
                warn(world, rng);
            }
        }
        _ => {
            let advanced = stage > 4;
            let stacks = if advanced { 3.0 } else { 1.0 };
            crate::fire::adjust_fire_stacks(world, mob, stacks * s.power);
            let burn = if advanced { 5.0 } else { 3.0 };
            damage(world, mob, DamageType::Burn, burn * s.power);
            crate::fire::ignite_mob(world, mob);
            if crate::fire::on_fire(world, mob) {
                let name = super::name_of(world, mob);
                let own = if advanced {
                    "Your skin erupts into an inferno!"
                } else {
                    "Your skin bursts into flames!"
                };
                super::visible(
                    world,
                    mob,
                    "",
                    &format!("{name} catches fire!"),
                    Span::Warning,
                );
                say_to(world, mob, Span::BoldWarning, own);
                emote(world, mob, "scream");
            } else if !s.suppress {
                warn(world, rng);
            }
            if s.flag {
                airborne_spread(
                    world,
                    mob,
                    disease,
                    if advanced { 4 } else { 2 },
                    true,
                    false,
                    rng,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defs_follow_the_enum() {
        for (i, d) in DEFS.iter().enumerate() {
            assert_eq!(d.kind as usize, i, "{}", d.path);
        }
        assert_eq!(
            SymptomKind::from_path("/datum/symptom/cough"),
            Some(K::Cough)
        );
    }

    #[test]
    fn cold_preset_stats() {
        // tg `/datum/disease/advance/cold`: one sneeze.
        let cold = Disease::advanced("Cold", &[K::Sneeze]);
        let a = cold.advance.as_ref().unwrap();
        assert_eq!(
            a.props,
            Props {
                resistance: 3,
                stealth: -2,
                stage_rate: 0,
                transmittable: 4,
                // 1 + round(3/12) + 0 + round(4/8) = 2; round(2/2) = 1; 1 * 1/6 -> clamp 1.
                severity: 1,
            }
        );
        assert_eq!(cold.spread_flags, SPREAD_BLOOD | SPREAD_FLUIDS);
        assert_eq!(cold.spread_text, "Fluids");
        assert_eq!(cold.severity, Severity::Nonthreat);
        assert_eq!(cold.spreading_modifier, 2.0);
        assert!((cold.stage_prob - 1.5).abs() < 1e-6);
        assert_eq!(cold.visibility & HIDDEN_SCANNER, 0);
        assert_eq!(cold.cure_text(2), "Spaceacillin");
    }

    #[test]
    fn transmission_picks_the_spread() {
        // Sneeze 4, cough 2, polyvitiligo 2, itching 1, viral evolution 3.
        let d = Disease::advanced(
            "x",
            &[
                K::Sneeze,
                K::Cough,
                K::Polyvitiligo,
                K::Itching,
                K::ViralEvolution,
            ],
        );
        assert_eq!(d.advance.as_ref().unwrap().props.transmittable, 12);
        assert_ne!(d.spread_flags & SPREAD_AIRBORNE, 0);
        assert_eq!(d.spread_text, "Respiration");
    }

    #[test]
    fn ids_ignore_order_and_mark_neutered() {
        let a = Disease::advanced("a", &[K::Cough, K::Sneeze]);
        let b = Disease::advanced("b", &[K::Sneeze, K::Cough]);
        assert_eq!(a.id(), b.id());
        assert!(a.is_same(&b));
        let mut c = b.clone();
        c.advance.as_mut().unwrap().symptoms[0].neutered = true;
        assert_ne!(a.id(), c.id());
    }

    #[test]
    fn mixing_merges_symptoms_up_to_the_limit() {
        let mut rng = CombatRng(5);
        let a = Disease::advanced("a", &[K::Cough, K::Sneeze]);
        let b = Disease::advanced("b", &[K::Fever, K::Headache]);
        let mixed = advance_mix(vec![a, b], &mut rng).unwrap();
        assert_eq!(mixed.advance.as_ref().unwrap().symptoms.len(), 4);
        let many = Disease::advanced(
            "m",
            &[
                K::Chills,
                K::Itching,
                K::Vomit,
                K::Choking,
                K::Confusion,
                K::Dizzy,
            ],
        );
        let mixed = advance_mix(vec![mixed, many], &mut rng).unwrap();
        assert_eq!(
            mixed.advance.as_ref().unwrap().symptoms.len(),
            SYMPTOM_LIMIT
        );
    }

    #[test]
    fn remedied_share_is_the_cure_status() {
        let mut d = Disease::advanced("a", &[K::Cough, K::Fever]);
        let a = d.advance.as_mut().unwrap();
        assert_eq!(a.cure_status(), 0.0);
        a.symptoms[0].remedied = true;
        assert_eq!(a.cure_status(), 0.5);
        assert_eq!(a.cure_text(3), "Spaceacillin and Leporazine");
    }

    #[test]
    fn random_samples_respect_the_level() {
        let mut rng = CombatRng(11);
        for _ in 0..50 {
            let d = Disease::random_advance(&mut rng, None, 3);
            let a = d.advance.as_ref().unwrap();
            assert!((1..=SYMPTOM_LIMIT).contains(&a.symptoms.len()));
            assert!(
                a.symptoms
                    .iter()
                    .all(|s| (1..=3).contains(&s.kind.def().level))
            );
            assert!(d.name.starts_with("Sample #"));
        }
    }
}
