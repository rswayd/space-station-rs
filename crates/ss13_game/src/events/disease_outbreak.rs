//! Disease Outbreak: Classic and Advanced (`code/modules/events/disease_outbreak.dm`): one
//! crewmember (patient zero) catches a disease ([`crate::medical::diseases::force_contract`]);
//! 150 s later Central Command announces a level 7 biohazard named after it.
//!
//! - Candidates (`generate_candidates`): living crew humans with a client on a station level
//!   and no disease yet; without any the event cannot roll.
//! - Classic: a random one of tg's "safe" diseases.
//! - Advanced (`/datum/disease/advance/random/event`): 3-4 symptoms from the mild pool plus
//!   the harmful or the dangerous pool by a rolled severity (40% medium, 45% harmful, 15%
//!   dangerous), the first one at least that severe; transmission 4-7 (15% airborne and
//!   visible on scanners, else skin contact and hidden; a dangerous one is always skin contact
//!   and visible), faster stages, named after one symptom's illness; two thirds also get viral
//!   adaptation or evolution and eternal youth. A hidden one shows on scanners after its
//!   incubation (140 s).
// shortcut: no admin setup (disease, severity, symptom count, transmissibility), no
// `TRAIT_VIRUSIMMUNE`, no mining levels, no status display biohazard alert, no ghost
// notification; the classic pool lacks Beesease, Magnitis and Weightlessness (not ported),
// so it picks among the other five.

use bevy::prelude::*;
use ss13_shared::TilePos;

use super::controller::{EventRng, EventState, RoundEvent, station_levels};
use crate::combat::CombatRng;
use crate::health::Stat;
use crate::medical::diseases::{
    self, Disease, HIDDEN_SCANNER, Kind, SPREAD_AIRBORNE, SPREAD_BLOOD, SPREAD_FLUIDS, SPREAD_SKIN,
    Severity,
};
use crate::medical::symptoms::SymptomKind as S;

pub const CLASSIC: &str = "Disease Outbreak: Classic";
pub const ADVANCED: &str = "Disease Outbreak: Advanced";

/// `ADV_ANNOUNCE_DELAY`, event fires.
pub const ANNOUNCE_DELAY: u32 = 75;
/// `incubation_time`: `(ADV_ANNOUNCE_DELAY * 2 - 10) SECONDS`, in event fires.
pub const INCUBATION_FIRES: u32 = 70;
/// `ADV_MIN_SYMPTOMS`, `ADV_MAX_SYMPTOMS`.
const MIN_SYMPTOMS: u32 = 3;
const MAX_SYMPTOMS: u32 = 4;
/// `ADV_DISEASE_MEDIUM`, `ADV_DISEASE_HARMFUL`, `ADV_DISEASE_DANGEROUS`.
pub const MEDIUM: i32 = 1;
pub const HARMFUL: i32 = 3;
pub const DANGEROUS: i32 = 5;
/// `ADV_RNG_LOW`, `ADV_RNG_MID`, `ADV_SPREAD_THRESHOLD`.
const RNG_LOW: u32 = 40;
const RNG_MID: u32 = 85;
const SPREAD_THRESHOLD: u32 = 85;
/// `ANNOUNCER_OUTBREAK7`.
const ANNOUNCER_OUTBREAK7: &str = "announcer_outbreak7";

/// Classic `virus_candidates` that are ported.
const CLASSIC_POOL: [Kind; 5] = [
    Kind::Flu,
    Kind::Cold9,
    Kind::Brainrot,
    Kind::FluSpanish,
    Kind::Anxiety,
];

/// `announce()`'s names for a fake outbreak (some never roll for real).
const FAKE_NAMES: [&str; 12] = [
    "Severe Anxiety",
    "Beesease",
    "Brainrot",
    "The Cold",
    "The Flu",
    "Spanish inquisition Flu",
    "Magnitis",
    "Localized Weightloss Malfunction",
    "Death Sandwich Poisoning",
    "Retrovirus",
    "GBS",
    "The Rhumba Beat",
];

/// The event's symptom pools with each symptom's `illness` (the disease name it gives).
const MILD: [(S, &str); 14] = [
    (S::Beard, "Man-Mouth"),
    (S::Chills, "Cold Shoulder"),
    (S::Confusion, "Shattered Reality"),
    (S::Cough, "Jest Infection"),
    (S::Disfiguration, "Broken Face"),
    (S::Dizzy, "Motion Sickness"),
    (S::Fever, "Burning Desire"),
    (S::Hallucigen, "Paranoyance"),
    (S::Headache, "Brain Freeze"),
    (S::Itching, "Discrete Itching"),
    (S::Polyvitiligo, "Chroma Imbalance"),
    (S::Shedding, "Thin Skinned"),
    (S::Sneeze, "Bard Flu"),
    (S::VoiceChange, "Mime Crisis"),
];
const HARMFUL_POOL: [(S, &str); 6] = [
    (S::Choking, "Pneumatic Tubes"),
    (S::Deafness, "Aural Perforation"),
    (S::GeneticMutation, "Lycanthropy"),
    (S::Narcolepsy, "Aurora Snorealis"),
    (S::Vomit, "Cyclonic Irritation"),
    (S::WeightLoss, "Placid Reflux"),
];
const DANGEROUS_POOL: [(S, &str); 6] = [
    (S::Alkali, "Crispy Skin"),
    (S::Asphyxiation, "Iron Lungs"),
    (S::Fire, "Spontaneous Combustion"),
    (S::FleshDeath, "Premature Mummification"),
    (S::FleshEating, "Jellyitis"),
    (S::VisionLoss, "Diluted Pupils"),
];

/// `generate_candidates()`: living crew humans with a client on a station level, not sick.
pub fn candidates(world: &mut World) -> Vec<Entity> {
    let levels = station_levels(world);
    let mut query = world
        .query_filtered::<(Entity, &Stat, Option<&TilePos>, Option<&diseases::Diseases>), (
            With<crate::server::PlayerMob>,
            With<crate::jobs::PlayerJob>,
            With<crate::human::HumanLook>,
        )>();
    let mut out: Vec<Entity> = query
        .iter(world)
        .filter(|(_, stat, pos, sick)| {
            **stat != Stat::Dead
                && pos.is_some_and(|p| levels.contains(&p.z))
                && sick.is_none_or(|d| d.list.is_empty())
        })
        .map(|(e, ..)| e)
        .collect();
    out.sort();
    out
}

/// The controls' `can_spawn_event`: someone to infect.
pub fn has_candidates(world: &mut World) -> bool {
    !candidates(world).is_empty()
}

pub fn make_classic(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(Outbreak {
        advanced: false,
        illness: String::new(),
        strain: None,
    })
}

pub fn make_advanced(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(Outbreak {
        advanced: true,
        illness: String::new(),
        strain: None,
    })
}

/// `/datum/round_event/disease_outbreak` and `/advanced`.
struct Outbreak {
    advanced: bool,
    /// `illness_type`.
    illness: String,
    /// The advanced disease's id, to show it on scanners after the incubation.
    strain: Option<String>,
}

fn rng(world: &mut World) -> CombatRng {
    CombatRng(world.resource_mut::<EventRng>().0.next_u64())
}

impl RoundEvent for Outbreak {
    fn setup(&mut self, _: &mut World, ev: &mut EventState) {
        ev.announce_when = ANNOUNCE_DELAY;
        if self.advanced {
            ev.end_when = INCUBATION_FIRES;
        }
    }

    fn announce(&mut self, world: &mut World, _: &mut EventState, _fake: bool) {
        if self.illness.is_empty() {
            let mut rng = rng(world);
            self.illness = FAKE_NAMES[rng.range(0, FAKE_NAMES.len() as u32 - 1) as usize].into();
        }
        let text = format!(
            "Вспышка биологической угрозы 7-го уровня зафиксирована на борту {}. Всему \
             персоналу надлежит сдержать её распространение.",
            super::station_name(world)
        );
        super::priority_announce_sound(world, &self.illness, &text, ANNOUNCER_OUTBREAK7);
    }

    fn start(&mut self, world: &mut World, _: &mut EventState) {
        let mut rng = rng(world);
        let disease = if self.advanced {
            let symptoms = rng.range(MIN_SYMPTOMS, MAX_SYMPTOMS) as usize;
            let roll = rng.range(1, 100);
            let severity = if roll < RNG_LOW {
                MEDIUM
            } else if roll < RNG_MID {
                HARMFUL
            } else {
                DANGEROUS
            };
            event_disease(&mut rng, symptoms, severity, None)
        } else {
            // tg also sets `carrier` on this template, but `Copy()` does not carry it over:
            // patient zero is symptomatic, as played.
            Disease::new(CLASSIC_POOL[rng.range(0, CLASSIC_POOL.len() as u32 - 1) as usize])
        };
        self.illness.clone_from(&disease.name);
        let mut afflicted = candidates(world);
        while !afflicted.is_empty() {
            let i = rng.range(0, afflicted.len() as u32 - 1) as usize;
            let victim = afflicted.swap_remove(i);
            if diseases::force_contract(world, victim, &disease) {
                info!(
                    "Event triggered: Disease Outbreak - {} starting with patient zero {victim}",
                    disease.name
                );
                if disease.visibility & HIDDEN_SCANNER != 0 {
                    self.strain = Some(disease.id());
                }
                return;
            }
        }
        info!("Event Disease Outbreak attempted to start, but failed to find a candidate target");
    }

    /// The incubation's end (`make_visible`): every case of the strain shows on scanners.
    fn end(&mut self, world: &mut World, _: &mut EventState) {
        let Some(id) = self.strain.take() else {
            return;
        };
        let mut query = world.query::<&mut diseases::Diseases>();
        for mut all in query.iter_mut(world) {
            for d in all.list.iter_mut().filter(|d| d.id() == id) {
                d.visibility &= !HIDDEN_SCANNER;
            }
        }
    }
}

/// `/datum/disease/advance/random/event/New(max_symptoms, requested_severity,
/// requested_transmissibility)` and its `assign_properties`. `transmissibility` is the admin's
/// pick (0 fluids, 70 skin, 90 airborne) or `None` for a roll.
pub fn event_disease(
    rng: &mut CombatRng,
    max_symptoms: usize,
    severity: i32,
    transmissibility: Option<u32>,
) -> Disease {
    let mut pool: Vec<(S, &str)> = MILD.to_vec();
    match severity {
        HARMFUL => pool.extend(HARMFUL_POOL),
        DANGEROUS => pool.extend(DANGEROUS_POOL),
        _ => {}
    }
    let mut chosen: Vec<(S, &str)> = Vec::new();
    let mut current = 0;
    // tg stops the whole generation on an empty pool (a stack trace); here the symptoms so far
    // stay.
    while chosen.len() < max_symptoms && !pool.is_empty() {
        let pick = pool.swap_remove(rng.range(0, pool.len() as u32 - 1) as usize);
        let level = pick.0.def().severity;
        if current < severity && level < severity {
            continue;
        }
        chosen.push(pick);
        current = current.max(level);
    }
    let mut visibility = Kind::Advance.def().visibility | HIDDEN_SCANNER;
    let roll = transmissibility.unwrap_or_else(|| rng.range(1, 100));
    let (flags, text) = if transmissibility == Some(0) {
        (SPREAD_BLOOD | SPREAD_FLUIDS, "Fluids")
    } else if transmissibility == Some(90) {
        (
            SPREAD_BLOOD | SPREAD_FLUIDS | SPREAD_SKIN | SPREAD_AIRBORNE,
            "Respiration",
        )
    } else if current >= DANGEROUS {
        visibility &= !HIDDEN_SCANNER;
        (SPREAD_BLOOD | SPREAD_FLUIDS | SPREAD_SKIN, "Skin contact")
    } else if roll < SPREAD_THRESHOLD {
        (SPREAD_BLOOD | SPREAD_FLUIDS | SPREAD_SKIN, "Skin contact")
    } else {
        visibility &= !HIDDEN_SCANNER;
        (
            SPREAD_BLOOD | SPREAD_FLUIDS | SPREAD_SKIN | SPREAD_AIRBORNE,
            "Respiration",
        )
    };
    let mut name = chosen
        .get(rng.range(0, chosen.len().max(1) as u32 - 1) as usize)
        .map_or("Event Disease", |&(_, illness)| illness)
        .to_owned();
    let mut kinds: Vec<S> = chosen.iter().map(|&(kind, _)| kind).collect();
    if rng.chance(66.0) {
        kinds.push(if rng.range(0, 1) == 0 {
            S::ViralAdaptation
        } else {
            S::ViralEvolution
        });
        kinds.push(S::Youth);
    }
    // `Refresh()`: the properties from the symptoms, then the event's `assign_properties`.
    let mut disease = Disease::advanced(&name, &kinds);
    let transmittable = rng.range(4, 7) as i32;
    let Some(advance) = disease.advance.as_mut() else {
        return disease;
    };
    advance.props.transmittable = transmittable;
    let props = advance.props;
    disease.visibility = visibility;
    disease.spread_flags = flags;
    disease.spread_text = text.to_owned();
    disease.spreading_modifier = (0.4 * transmittable as f32).ceil().max(1.0);
    disease.cure_chance = (10.0 * 0.94f32.powi(props.resistance)).clamp(3.5, 12.0);
    // Faster than regular advanced diseases.
    disease.stage_prob = (0.4 * props.stage_rate as f32).max(1.0);
    disease.severity = Severity::from_level(props.severity);
    if props.stage_rate >= 7 {
        name = format!("Advanced {name}");
        disease.name = name;
    }
    disease
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_disease_meets_the_requested_severity() {
        for seed in 0..200 {
            let mut rng = CombatRng(seed);
            for severity in [MEDIUM, HARMFUL, DANGEROUS] {
                let d = event_disease(&mut rng, 4, severity, None);
                let advance = d.advance.as_ref().unwrap();
                let worst = advance
                    .symptoms
                    .iter()
                    .map(|s| s.kind.def().severity)
                    .max()
                    .unwrap();
                assert!(worst >= severity, "seed {seed}: {worst} < {severity}");
                let picked = advance
                    .symptoms
                    .iter()
                    .filter(|s| {
                        !matches!(s.kind, S::ViralAdaptation | S::ViralEvolution | S::Youth)
                    })
                    .count();
                assert_eq!(picked, 4);
                assert!((4..=7).contains(&advance.props.transmittable));
                assert!(d.stage_prob >= 1.0);
                assert!(d.spread_flags & SPREAD_SKIN != 0, "never blood/fluids only");
                if worst >= DANGEROUS {
                    assert_eq!(d.spread_flags & SPREAD_AIRBORNE, 0);
                    assert_eq!(d.visibility & HIDDEN_SCANNER, 0, "dangerous ones show");
                }
                if d.spread_flags & SPREAD_AIRBORNE != 0 {
                    assert_eq!(d.visibility & HIDDEN_SCANNER, 0, "airborne ones show");
                }
            }
        }
    }

    #[test]
    fn forced_transmissibility_sets_the_spread() {
        let mut rng = CombatRng(9);
        let fluids = event_disease(&mut rng, 3, MEDIUM, Some(0));
        assert_eq!(fluids.spread_flags, SPREAD_BLOOD | SPREAD_FLUIDS);
        assert_eq!(fluids.spread_text, "Fluids");
        let air = event_disease(&mut rng, 3, MEDIUM, Some(90));
        assert!(air.spread_flags & SPREAD_AIRBORNE != 0);
    }

    #[test]
    fn names_come_from_the_symptoms() {
        let illnesses: Vec<&str> = MILD
            .iter()
            .chain(&HARMFUL_POOL)
            .chain(&DANGEROUS_POOL)
            .map(|&(_, n)| n)
            .collect();
        for seed in 0..50 {
            let d = event_disease(&mut CombatRng(seed), 3, HARMFUL, None);
            let base = d.name.strip_prefix("Advanced ").unwrap_or(&d.name);
            assert!(illnesses.contains(&base), "{}", d.name);
        }
    }
}
