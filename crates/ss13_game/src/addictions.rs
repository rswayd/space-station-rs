//! Addictions (server), after tg `code/modules/reagents/withdrawal/_addiction.dm`
//! (`/datum/addiction`), `generic_addictions.dm`, the mind's
//! `add_addiction_points`/`remove_addiction_points` (`code/datums/mind/_mind.dm`) and the
//! points `metabolize_reagent` gives (`chemistry/holder/mob_life.dm`).
//!
//! - [`Addictions`]: points per addiction type (at most [`MAX_ADDICTION_POINTS`]); reaching
//!   the type's gain threshold (600) makes it active at cycle 1, dropping under the loss
//!   threshold (400) ends it.
//! - A reagent with `addiction_types` (ethanol: `max(50, round(150 - boozepwr, 5))` of alcohol)
//!   gives a mob with a mind `600 / (threshold / metabolized)` points of each type every time
//!   it metabolizes ([`metabolized`]).
//! - Every carbon `Life` (2 s) each type with points is processed: a reagent of that type in
//!   the blood at the relief amount keeps the withdrawal at cycle 1; without it the points
//!   drop by the stage's rate and an active addiction advances a cycle per second: stage 1
//!   from cycle 61 (light moodlet), 2 from 121, 3 from 181, each with its chat lines and the
//!   type's effects: opioids yawn and raise the blood pressure; stimulants slow actions, make
//!   woozy and slow movement; alcohol jitters, hallucinates and seizes; hallucinogens end in a
//!   trance; maintenance drugs grow a beard; medicine coughs; nicotine jitters and coughs.
// The points live on the mind's mob and move with it (`crate::mind::transfer`).
// tg's maintenance-drug `emote("growls")` names no human emote and does nothing; not ported.
// shortcut: no disgust (opioid stage 3, its halving at the end), no hallucinogen screen blur
// and wave filters, no maintenance-drug fake healthy HUD, tongue food types, night vision or
// bright-light dizziness, no medicine fake health doll, fake alerts, fake crit or organ pain
// lines; no `TRAIT_NO_WITHDRAWALS`, `TRAIT_HOPELESSLY_ADDICTED`, `TRAIT_ADDICTIONRESILIENT`,
// `TRAIT_ANTICONVULSANT`, `TRAIT_OVERDOSEIMMUNE` or high sanity loss bonus. Add each with the
// system it needs.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_reagents::metabolism::Metabolized;
use ss13_reagents::{Chemistry, ReagentDef, round_to};
use ss13_shared::chat::Span;

use crate::chem::{Chem, MobReagents};
use crate::mood::{ActiveMood, Mood, MoodEvent};
use crate::server::PlayerMob;
use crate::status_effects::Timed;

/// `MAX_ADDICTION_POINTS`.
pub const MAX_ADDICTION_POINTS: f32 = 1000.0;
/// `/datum/addiction` `addiction_gain_threshold`, `addiction_loss_threshold`.
pub const GAIN_THRESHOLD: f32 = 600.0;
pub const LOSS_THRESHOLD: f32 = 400.0;
/// `WITHDRAWAL_STAGE{1,2,3}_START_CYCLE`.
pub const STAGE1_CYCLE: f32 = 61.0;
pub const STAGE2_CYCLE: f32 = 121.0;
pub const STAGE3_CYCLE: f32 = 181.0;
/// `addiction_loss_per_stage`, units a second per stage 0-3.
const LOSS_PER_STAGE: [f32; 4] = [0.5, 0.5, 1.0, 1.5];
/// The carbon `Life` interval (`SSmobs` wait), seconds.
const LIFE_SECS: f64 = 2.0;
/// The chat line chances of each stage, percent a second.
const STAGE_MESSAGE_PERCENT: [f32; 3] = [5.0, 10.0, 15.0];
/// `ACTIONSPEED_ID_STIMULANTS` (`/datum/actionspeed_modifier/stimulants`: 0.25).
const ACTIONSPEED_STIMULANTS: &str = "stimulants";
const ACTIONSPEED_STIMULANTS_SLOWDOWN: f32 = 0.25;
/// `/datum/movespeed_modifier/stimulants` (`MOVESPEED_ID_STIMULANTS`), deciseconds a tile.
const MOVESPEED_STIMULANTS: &str = "stimulants_withdrawal";
const MOVESPEED_STIMULANTS_SLOWDOWN: f32 = 1.0;

/// The `/datum/addiction` types.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Addiction {
    Opioids,
    Stimulants,
    Alcohol,
    Hallucinogens,
    MaintenanceDrugs,
    Medicine,
    Nicotine,
}

impl Addiction {
    pub const ALL: [Addiction; 7] = [
        Addiction::Opioids,
        Addiction::Stimulants,
        Addiction::Alcohol,
        Addiction::Hallucinogens,
        Addiction::MaintenanceDrugs,
        Addiction::Medicine,
        Addiction::Nicotine,
    ];

    pub fn path(self) -> &'static str {
        match self {
            Addiction::Opioids => "/datum/addiction/opioids",
            Addiction::Stimulants => "/datum/addiction/stimulants",
            Addiction::Alcohol => "/datum/addiction/alcohol",
            Addiction::Hallucinogens => "/datum/addiction/hallucinogens",
            Addiction::MaintenanceDrugs => "/datum/addiction/maintenance_drugs",
            Addiction::Medicine => "/datum/addiction/medicine",
            Addiction::Nicotine => "/datum/addiction/nicotine",
        }
    }

    /// The type of an `addiction_types` key.
    pub fn from_path(path: &str) -> Option<Addiction> {
        Addiction::ALL.into_iter().find(|a| a.path() == path)
    }

    /// `name`.
    pub fn name(self) -> &'static str {
        match self {
            Addiction::Opioids => "opioid",
            Addiction::Stimulants => "stimulant",
            Addiction::Alcohol => "alcohol",
            Addiction::Hallucinogens => "hallucinogen",
            Addiction::MaintenanceDrugs => "maintenance drug",
            Addiction::Medicine => "medicine",
            Addiction::Nicotine => "nicotine",
        }
    }

    /// `withdrawal_stage_messages` (empty: none).
    fn messages(self) -> [&'static str; 3] {
        match self {
            Addiction::Opioids => [
                "I feel aches in my bodies..",
                "I need some pain relief...",
                "It aches all over...I need some opioids!",
            ],
            Addiction::Stimulants => [
                "You feel a bit tired...You could really use a pick me up.",
                "You are getting a bit woozy...",
                "So...Tired...",
            ],
            Addiction::Alcohol => [
                "I could use a drink...",
                "Maybe the bar is still open?..",
                "God I need a drink!",
            ],
            Addiction::Hallucinogens => [
                "I feel so empty...",
                "I wonder what the machine elves are up to?..",
                "I need to see the beautiful colors again!!",
            ],
            Addiction::MaintenanceDrugs | Addiction::Medicine => ["", "", ""],
            Addiction::Nicotine => [
                "Feel like having a smoke...",
                "Getting antsy. Really need a smoke now.",
                "I can't take it! Need a smoke NOW!",
            ],
        }
    }

    /// `addiction_relief_treshold` (`MIN_ADDICTION_REAGENT_AMOUNT`, nicotine's
    /// `MIN_NICOTINE_ADDICTION_REAGENT_AMOUNT`).
    fn relief(self) -> f32 {
        match self {
            Addiction::Nicotine => 0.01,
            _ => 1.0,
        }
    }

    /// The light, medium and severe withdrawal moodlets with the drug's name.
    fn moodlet(self, stage: usize) -> ActiveMood {
        let event = match (self, stage) {
            (Addiction::Nicotine, 2) => &NICOTINE_MODERATE,
            (Addiction::Nicotine, 3) => &NICOTINE_SEVERE,
            (_, 1) => &WITHDRAWAL_LIGHT,
            (_, 2) => &WITHDRAWAL_MEDIUM,
            _ => &WITHDRAWAL_SEVERE,
        };
        let mut mood = ActiveMood::new(&self.mood_category(), event);
        if self != Addiction::Nicotine || stage == 1 {
            mood.description = match stage {
                1 => format!("Вот бы еще немножечко {}...", self.name()),
                2 => format!("Мне очень нужно {}.", self.name()),
                _ => format!("Боже, кто-нибудь дайте мне {}!", self.name()),
            };
        }
        mood
    }

    /// `"[type]_addiction"`.
    pub fn mood_category(self) -> String {
        format!("{}_addiction", self.path())
    }
}

/// `/datum/mood_event/withdrawal_*` (descriptions set per drug) and the nicotine ones.
const WITHDRAWAL_LIGHT: MoodEvent = MoodEvent {
    id: "withdrawal_light",
    description: "",
    mood_change: -2.0,
    timeout: 0.0,
    hidden: false,
};
const WITHDRAWAL_MEDIUM: MoodEvent = MoodEvent {
    id: "withdrawal_medium",
    description: "",
    mood_change: -5.0,
    timeout: 0.0,
    hidden: false,
};
const WITHDRAWAL_SEVERE: MoodEvent = MoodEvent {
    id: "withdrawal_severe",
    description: "",
    mood_change: -8.0,
    timeout: 0.0,
    hidden: false,
};
const NICOTINE_MODERATE: MoodEvent = MoodEvent {
    id: "nicotine_withdrawal_moderate",
    description: "Давно я не закуривал. Немного на взводе... ",
    mood_change: -5.0,
    timeout: 0.0,
    hidden: false,
};
const NICOTINE_SEVERE: MoodEvent = MoodEvent {
    id: "nicotine_withdrawal_severe",
    description: "Голова трещит. Холодный пот. Тревожность. Нужно расслабиться и закурить!",
    mood_change: -8.0,
    timeout: 0.0,
    hidden: false,
};

/// `addiction_types` of a reagent, with the alcohol entry ethanol's `New()` sets from
/// `boozepwr`: (type, units metabolized to get addicted).
pub fn of_def(def: &ReagentDef) -> Vec<(Addiction, f32)> {
    let mut out: Vec<(Addiction, f32)> = def
        .addiction_types
        .iter()
        .filter_map(|(path, threshold)| Addiction::from_path(path).map(|a| (a, *threshold)))
        .collect();
    if is_under(&def.path, "/datum/reagent/consumable/ethanol") && def.boozepwr > 0.0 {
        // `LAZYSET`: replaces an alcohol entry of the type's own.
        let threshold = round_to(150.0 - def.boozepwr, 5.0).max(50.0);
        out.retain(|(a, _)| *a != Addiction::Alcohol);
        out.push((Addiction::Alcohol, threshold));
    }
    out
}

/// The ChemMaster's addiction line of a reagent (`ui_data.dm` `get_addiction_text`).
pub fn describe(def: &ReagentDef) -> Vec<String> {
    of_def(def)
        .into_iter()
        .map(|(a, threshold)| match threshold {
            t if t <= 10.0 => format!("Potent {}", a.name()),
            t if t <= 30.0 => format!("Strong {}", a.name()),
            t if t <= 100.0 => a.name().to_owned(),
            _ => format!("Weak {}", a.name()),
        })
        .collect()
}

/// The addiction of an addict quirk's reagent by its path alone (the quirk drugs and every
/// ethanol with booze power).
pub fn of_reagent(path: &str, boozepwr: f32) -> Option<Addiction> {
    if is_under(path, "/datum/reagent/consumable/ethanol") {
        return (boozepwr > 0.0).then_some(Addiction::Alcohol);
    }
    if is_under(path, "/datum/reagent/drug/nicotine") {
        return Some(Addiction::Nicotine);
    }
    if is_under(path, "/datum/reagent/drug/krokodil")
        || is_under(path, "/datum/reagent/medicine/morphine")
    {
        return Some(Addiction::Opioids);
    }
    if is_under(path, "/datum/reagent/drug/methamphetamine")
        || is_under(path, "/datum/reagent/drug/cocaine")
    {
        return Some(Addiction::Stimulants);
    }
    if is_under(path, "/datum/reagent/drug/happiness")
        || is_under(path, "/datum/reagent/drug/blastoff")
    {
        return Some(Addiction::Hallucinogens);
    }
    None
}

/// The mind's `addiction_points` and `active_addictions` (the withdrawal cycle).
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct Addictions {
    pub points: Vec<(Addiction, f32)>,
    pub active: Vec<(Addiction, f32)>,
}

impl Addictions {
    pub fn points(&self, a: Addiction) -> f32 {
        self.points
            .iter()
            .find(|(t, _)| *t == a)
            .map_or(0.0, |(_, p)| *p)
    }

    pub fn cycle(&self, a: Addiction) -> Option<f32> {
        self.active.iter().find(|(t, _)| *t == a).map(|(_, c)| *c)
    }

    fn set_cycle(&mut self, a: Addiction, cycle: f32) {
        match self.active.iter_mut().find(|(t, _)| *t == a) {
            Some(entry) => entry.1 = cycle,
            None => self.active.push((a, cycle)),
        }
    }

    fn set_points(&mut self, a: Addiction, points: f32) {
        if points <= 0.0 {
            self.points.retain(|(t, _)| *t != a);
            return;
        }
        match self.points.iter_mut().find(|(t, _)| *t == a) {
            Some(entry) => entry.1 = points,
            None => self.points.push((a, points)),
        }
    }
}

fn tell(world: &mut World, mob: Entity, span: Span, text: impl Into<String>) {
    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
        crate::tools::tell(world, client, span, text);
    }
}

/// `add_addiction_points` -> `on_gain_addiction_points`: crossing the gain threshold makes the
/// addiction active at cycle 1.
pub fn add_points(world: &mut World, mob: Entity, a: Addiction, amount: f32) {
    if world.get_entity(mob).is_err() {
        return;
    }
    if world.get::<Addictions>(mob).is_none() {
        world.entity_mut(mob).insert(Addictions::default());
    }
    let mut addictions = world.get_mut::<Addictions>(mob).expect("inserted above");
    let last = addictions.points(a);
    let new = (last + amount).min(MAX_ADDICTION_POINTS);
    addictions.set_points(a, new);
    if new >= GAIN_THRESHOLD && last < GAIN_THRESHOLD {
        addictions.set_cycle(a, 1.0);
        info!("{mob} has become addicted to {}", a.name());
    }
}

/// `remove_addiction_points` -> `on_lose_addiction_points`: true if the addiction ended.
pub fn remove_points(world: &mut World, mob: Entity, a: Addiction, amount: f32) -> bool {
    let Some(mut addictions) = world.get_mut::<Addictions>(mob) else {
        return false;
    };
    let last = addictions.points(a);
    let new = (last - amount).max(0.0);
    addictions.set_points(a, new);
    if last < LOSS_THRESHOLD || new >= LOSS_THRESHOLD {
        return false;
    }
    // `lose_addiction`.
    crate::mood::clear_event(world, mob, &a.mood_category());
    tell(
        world,
        mob,
        Span::Notice,
        "Вы чувствуете, как употребление наркотиков теряет какой-либо смысл.",
    );
    end_withdrawal(world, mob, a);
    if let Some(mut addictions) = world.get_mut::<Addictions>(mob) {
        addictions.active.retain(|(t, _)| *t != a);
    }
    true
}

/// `end_withdrawal`: back to cycle 1, the moodlet and the type's effects cleared.
fn end_withdrawal(world: &mut World, mob: Entity, a: Addiction) {
    if let Some(mut addictions) = world.get_mut::<Addictions>(mob) {
        addictions.set_cycle(a, 1.0);
    }
    crate::mood::clear_event(world, mob, &a.mood_category());
    match a {
        Addiction::Opioids => crate::debuffs::set_high_blood_pressure(world, mob, false),
        Addiction::Stimulants => {
            crate::actionspeed::remove_actionspeed_modifier(world, mob, ACTIONSPEED_STIMULANTS);
            crate::debuffs::set_woozy(world, mob, false);
            crate::drugs::set_speed_mod(world, mob, MOVESPEED_STIMULANTS, None);
        }
        Addiction::Hallucinogens => crate::curses::remove_trance(world, mob),
        _ => {}
    }
}

/// The withdrawal stage of a cycle (0: none).
pub fn stage(cycle: f32) -> usize {
    if cycle >= STAGE3_CYCLE {
        3
    } else if cycle >= STAGE2_CYCLE {
        2
    } else if cycle >= STAGE1_CYCLE {
        1
    } else {
        0
    }
}

/// Whether the blood holds `a`'s reagents at its relief amount.
fn on_drug(world: &World, mob: Entity, a: Addiction) -> bool {
    let (Some(chem), Some(reagents)) =
        (world.get_resource::<Chem>(), world.get::<MobReagents>(mob))
    else {
        return false;
    };
    reagents
        .blood
        .reagents
        .iter()
        .any(|r| r.volume >= a.relief() && of_def(chem.0.def(r.id)).iter().any(|(t, _)| *t == a))
}

/// `metabolize_reagent`'s `add_addiction_points` (`owner.mind?`): `GAIN_THRESHOLD /
/// (threshold / metabolized_volume)` of every type of every reagent metabolized this tick.
pub fn metabolized(world: &mut World, chem: &Chemistry, mob: Entity, list: &[Metabolized]) {
    if world.get::<crate::mind::MindData>(mob).is_none() {
        return;
    }
    for m in list {
        for (a, threshold) in of_def(chem.def(m.id)) {
            if threshold > 0.0 && m.volume > 0.0 {
                add_points(world, mob, a, GAIN_THRESHOLD / (threshold / m.volume));
            }
        }
    }
}

fn rng_prob(world: &mut World, percent: f32, secs: f32) -> bool {
    let mut rng = world.get_resource_or_init::<crate::combat::CombatRng>();
    crate::medical::wounds::spt_prob(&mut rng, percent, secs)
}

fn emote(world: &mut World, mob: Entity, key: &str) {
    crate::speech::run_emote(world, mob, key);
}

/// `withdrawal_enters_stage_*` beyond the moodlet.
fn enters_stage(world: &mut World, mob: Entity, a: Addiction, stage: usize, secs: f32) {
    match (a, stage) {
        (Addiction::Opioids, 2) => crate::debuffs::set_high_blood_pressure(world, mob, true),
        (Addiction::Stimulants, 1) => crate::actionspeed::add_actionspeed_modifier(
            world,
            mob,
            ACTIONSPEED_STIMULANTS,
            ACTIONSPEED_STIMULANTS_SLOWDOWN,
        ),
        (Addiction::Stimulants, 2) => crate::debuffs::set_woozy(world, mob, true),
        (Addiction::Stimulants, 3) => crate::drugs::set_speed_mod(
            world,
            mob,
            MOVESPEED_STIMULANTS,
            Some(MOVESPEED_STIMULANTS_SLOWDOWN),
        ),
        (Addiction::Hallucinogens, 3) => {
            crate::curses::apply_trance(world, mob, 40.0, true);
        }
        (Addiction::MaintenanceDrugs, 2) => {
            let male = world
                .get::<crate::human::HumanLook>(mob)
                .is_some_and(|l| !l.female);
            if male {
                tell(world, mob, Span::Warning, "Your chin itches.");
                if let Some(mut look) = world.get_mut::<crate::human::HumanLook>(mob) {
                    look.facial_hair_style = Some(crate::human::FACIAL_HAIR_STYLES[2].into());
                }
            }
        }
        (Addiction::MaintenanceDrugs, 3) => {
            if world.get::<crate::human::HumanLook>(mob).is_some() {
                tell(
                    world,
                    mob,
                    Span::Warning,
                    "You feel yourself adapt to the darkness.",
                );
            }
        }
        (Addiction::Nicotine, 1) => {
            crate::status_effects::set_timed(world, mob, Timed::Jitter, 10.0 * secs, true);
        }
        _ => {}
    }
}

/// `withdrawal_stage_*_process` beyond the chat line.
fn stage_process(world: &mut World, mob: Entity, a: Addiction, stage: usize, secs: f32) {
    use crate::status_effects::set_timed;
    match (a, stage) {
        (Addiction::Opioids, 1) => {
            if rng_prob(world, 10.0, secs) {
                emote(world, mob, "yawn");
            }
        }
        (Addiction::Alcohol, _) => {
            let jitter = [10.0, 20.0, 30.0][stage - 1];
            set_timed(world, mob, Timed::Jitter, jitter * secs, true);
            if stage >= 2 {
                crate::hallucination::set_hallucinations(world, mob, 10.0, true);
            }
            if stage == 3 && rng_prob(world, 4.0, secs) {
                crate::debuffs::seizure(world, mob);
            }
        }
        (Addiction::Medicine, _) => {
            if rng_prob(world, [1.0, 2.0, 5.0][stage - 1], secs) {
                emote(world, mob, "cough");
            }
        }
        (Addiction::Nicotine, 2 | 3) => {
            let jitter = if stage == 2 { 20.0 } else { 30.0 };
            set_timed(world, mob, Timed::Jitter, jitter * secs, true);
            if rng_prob(world, if stage == 2 { 2.0 } else { 5.0 }, secs) {
                emote(world, mob, "cough");
            }
        }
        _ => {}
    }
}

/// `process_addiction` of one type.
pub fn process_addiction(world: &mut World, mob: Entity, a: Addiction, secs: f32) {
    let Some(cycle) = world.get::<Addictions>(mob).map(|x| x.cycle(a)) else {
        return;
    };
    let relieved = on_drug(world, mob, a);
    if relieved && cycle.is_some() {
        end_withdrawal(world, mob, a);
    }
    let cycle = world.get::<Addictions>(mob).and_then(|x| x.cycle(a));
    let stage_now = cycle.map_or(0, stage);
    if !relieved && remove_points(world, mob, a, LOSS_PER_STAGE[stage_now] * secs) {
        return;
    }
    let Some(cycle) = cycle else {
        return;
    };
    // `withdrawal_enters_stage_*`: the cycle lands on the start exactly (odd cycles, +2 a Life).
    let entered = [STAGE1_CYCLE, STAGE2_CYCLE, STAGE3_CYCLE]
        .iter()
        .position(|&start| cycle >= start && cycle - secs < start);
    if let Some(i) = entered {
        let now = world.resource::<Time>().elapsed_secs_f64();
        let moodlet = a.moodlet(i + 1);
        if let Some(mut mood) = world.get_mut::<Mood>(mob) {
            mood.add_instance(now, moodlet);
        }
        enters_stage(world, mob, a, i + 1, secs);
    }
    if stage_now > 0 {
        let line = a.messages()[stage_now - 1];
        if rng_prob(world, STAGE_MESSAGE_PERCENT[stage_now - 1], secs) && !line.is_empty() {
            tell(world, mob, Span::Danger, line);
        }
        stage_process(world, mob, a, stage_now, secs);
    }
    if let Some(mut addictions) = world.get_mut::<Addictions>(mob) {
        addictions.set_cycle(a, cycle + secs);
    }
}

/// Carbon `Life`'s `handle_addictions`, every 2 s for every mob with points, alive.
// Cost: 0.5 Hz, the few mobs with points, a pass over their blood reagents per type.
pub(crate) fn addiction_life(world: &mut World, mut clock: Local<f64>) {
    *clock += world.resource::<Time>().delta_secs_f64();
    if *clock < LIFE_SECS {
        return;
    }
    *clock -= LIFE_SECS;
    let mut query = world.query::<(Entity, &Addictions)>();
    let mobs: Vec<(Entity, Vec<Addiction>)> = query
        .iter(world)
        .map(|(e, a)| (e, a.points.iter().map(|(t, _)| *t).collect()))
        .collect();
    for (mob, types) in mobs {
        if world
            .get::<crate::health::Stat>(mob)
            .is_none_or(|s| *s == crate::health::Stat::Dead)
        {
            continue;
        }
        for a in types {
            process_addiction(world, mob, a, LIFE_SECS as f32);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reagents_map_to_their_addictions() {
        assert_eq!(
            of_reagent("/datum/reagent/consumable/ethanol/whiskey", 75.0),
            Some(Addiction::Alcohol)
        );
        assert_eq!(of_reagent("/datum/reagent/consumable/ethanol", 0.0), None);
        assert_eq!(
            of_reagent("/datum/reagent/drug/cocaine/freebase_cocaine", 0.0),
            Some(Addiction::Stimulants)
        );
        assert_eq!(
            of_reagent("/datum/reagent/drug/nicotine", 0.0),
            Some(Addiction::Nicotine)
        );
        assert_eq!(of_reagent("/datum/reagent/water", 0.0), None);
    }

    fn def(path: &str, boozepwr: f32, types: &[(&str, f32)]) -> ReagentDef {
        ReagentDef {
            path: path.into(),
            name: String::new(),
            description: String::new(),
            color: "#000000".into(),
            taste_description: String::new(),
            taste_mult: 1.0,
            metabolization_rate: 0.2,
            overdose_threshold: 0.0,
            ph: 7.0,
            specific_heat: 200.0,
            chemical_flags: 0,
            purity: 1.0,
            creation_purity: 1.0,
            inverse_chem: None,
            inverse_chem_val: 0.25,
            purge_multiplier: 1.0,
            toxpwr: 0.0,
            health_required: 0.0,
            liver_tolerance_multiplier: 1.0,
            boozepwr,
            brute_heal: 0.0,
            nutriment_factor: 0.0,
            penetrates_skin: 4,
            default_container: String::new(),
            addiction_types: types.iter().map(|(p, n)| ((*p).to_owned(), *n)).collect(),
        }
    }

    #[test]
    fn addiction_types_come_from_the_data_and_the_booze_power() {
        let meth = def(
            "/datum/reagent/drug/methamphetamine",
            0.0,
            &[("/datum/addiction/stimulants", 75.0)],
        );
        assert_eq!(of_def(&meth), vec![(Addiction::Stimulants, 75.0)]);
        // `max(50, round(150 - boozepwr, 5))`: whiskey (75) 75, absinthe (80) 70, 120 -> 50.
        let whiskey = def("/datum/reagent/consumable/ethanol/whiskey", 75.0, &[]);
        assert_eq!(of_def(&whiskey), vec![(Addiction::Alcohol, 75.0)]);
        let strong = def("/datum/reagent/consumable/ethanol/x", 120.0, &[]);
        assert_eq!(of_def(&strong), vec![(Addiction::Alcohol, 50.0)]);
        let maint = def(
            "/datum/reagent/consumable/ethanol/y",
            30.0,
            &[("/datum/addiction/maintenance_drugs", 600.0)],
        );
        assert_eq!(
            of_def(&maint),
            vec![
                (Addiction::MaintenanceDrugs, 600.0),
                (Addiction::Alcohol, 120.0)
            ]
        );
        assert_eq!(describe(&meth), vec!["stimulant".to_owned()]);
        assert_eq!(
            describe(&def(
                "/datum/reagent/drug/nicotine",
                0.0,
                &[("/datum/addiction/nicotine", 10.0)]
            )),
            vec!["Potent nicotine".to_owned()]
        );
    }

    #[test]
    fn metabolizing_gives_points_to_a_mind() {
        let mut world = World::new();
        world.init_resource::<Time>();
        let meth = def(
            "/datum/reagent/drug/methamphetamine",
            0.0,
            &[("/datum/addiction/stimulants", 75.0)],
        );
        let chem = Chemistry::new(ss13_reagents::ChemData {
            reagents: vec![meth],
            ..Default::default()
        });
        let m = Metabolized {
            id: 0,
            volume: 0.3,
            ratio: 0.75,
            overdosed: false,
            overdose_started: false,
            current_cycle: 1,
            creation_purity: 1.0,
            held: 10.0,
        };
        let mindless = world.spawn_empty().id();
        metabolized(&mut world, &chem, mindless, &[m]);
        assert!(
            world.get::<Addictions>(mindless).is_none(),
            "no mind, no points"
        );
        let mob = world.spawn(crate::mind::MindData::new("key", "name")).id();
        metabolized(&mut world, &chem, mob, &[m]);
        // 600 / (75 / 0.3) = 2.4.
        let points = world
            .get::<Addictions>(mob)
            .unwrap()
            .points(Addiction::Stimulants);
        assert!((points - 2.4).abs() < 1e-4, "{points}");
    }

    #[test]
    fn stages_follow_the_cycles() {
        assert_eq!(stage(1.0), 0);
        assert_eq!(stage(61.0), 1);
        assert_eq!(stage(121.0), 2);
        assert_eq!(stage(400.0), 3);
    }

    #[test]
    fn points_start_and_end_an_addiction() {
        let mut world = World::new();
        world.init_resource::<Time>();
        let mob = world.spawn_empty().id();
        add_points(&mut world, mob, Addiction::Nicotine, 500.0);
        assert_eq!(
            world
                .get::<Addictions>(mob)
                .unwrap()
                .cycle(Addiction::Nicotine),
            None
        );
        add_points(&mut world, mob, Addiction::Nicotine, 5000.0);
        let a = world.get::<Addictions>(mob).unwrap();
        assert_eq!(a.points(Addiction::Nicotine), MAX_ADDICTION_POINTS);
        assert_eq!(a.cycle(Addiction::Nicotine), Some(1.0));
        assert!(!remove_points(&mut world, mob, Addiction::Nicotine, 500.0));
        assert!(remove_points(&mut world, mob, Addiction::Nicotine, 200.0));
        assert_eq!(
            world
                .get::<Addictions>(mob)
                .unwrap()
                .cycle(Addiction::Nicotine),
            None
        );
    }

    #[test]
    fn withdrawal_advances_to_a_moodlet_without_the_drug() {
        let mut world = World::new();
        world.init_resource::<Time>();
        let mob = world.spawn(Mood::default()).id();
        add_points(&mut world, mob, Addiction::Opioids, MAX_ADDICTION_POINTS);
        // 30 Lives of 2 s: cycle 1 -> 61, stage 1 entered on the last.
        for _ in 0..31 {
            process_addiction(&mut world, mob, Addiction::Opioids, 2.0);
        }
        let category = Addiction::Opioids.mood_category();
        let mood = world.get::<Mood>(mob).unwrap();
        assert_eq!(mood.get(&category).map(|m| m.mood_change), Some(-2.0));
        let a = world.get::<Addictions>(mob).unwrap();
        assert_eq!(a.cycle(Addiction::Opioids), Some(63.0));
        // 0.5 points a second at stage 0 for 60 s, then stage 1's rate once.
        assert!(a.points(Addiction::Opioids) < MAX_ADDICTION_POINTS - 30.0);
    }
}
