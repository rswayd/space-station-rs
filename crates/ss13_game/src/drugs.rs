//! Drugs (server): `on_mob_life`, `overdose_process` and `on_mob_(end_)metabolize` of tg
//! `chemistry/reagents/drug_reagents.dm`, and the stacked metabolization effects of
//! `reagents/stacked_effects` (`bevarages.dm`).
//!
//! Every `SSmobs` tick `crate::chem` hands over what metabolized ([`metabolized`]) with its
//! `metabolization_ratio`; each drug acts on it with tg's numbers:
//! - space drugs: random steps and emotes, an overdose trips (moodlet, hallucinations); the
//!   drugginess itself is `crate::buffs`;
//! - cannabis: relaxed lines, munchies, dozing off lying down;
//! - nicotine: the `smoked` moodlet, no jitter, shorter stuns; an overdose poisons and chokes;
//! - krokodil: the heavy narcotic moodlet; impure krokodil (≤ 60%) at cycle 36 sloughs the skin
//!   off (bald, husk, 25 brute); an overdose damages the brain;
//! - methamphetamine: faster movement while it metabolizes, shorter stuns, stamina, jitter and
//!   brain damage; an overdose walks randomly, laughs, flails the hands empty;
//! - bath salts, aranesp, happiness, pump-up, the maintenance powder, sludge and tar, the
//!   mushroom hallucinogen, kronkaine and syndol with their own effects and overdoses.
//!
//! Any reagent that starts overdosing gives the `overdose` moodlet (-8 for 5 minutes).
//!
//! The stacked effects: coffee with sugar or tea with sugar give their moodlets, coffee with
//! iced coffee (and hot iced coffee) heal suffocation.
// shortcut: no blast-off, saturn-x, gore or BandaStation cocaine; no psychotic brawling of bath
// salts, no monkey AI calming, no `stoned` status effect, no mushroom colour shift or kronkaine
// sound environment and heartbeat, no maintenance metabolism (`TRAIT_MAINTENANCE_METABOLISM`),
// no experience multiplier of the maintenance powder, no adrenal crisis of kronkaine with
// purgers, no syndol delusions (random hallucinations for everyone); no disgust (happiness,
// astrotame with cream). Steps are one move request (the mover's cooldown takes the rest).
// The `metabolized_traits` with a port (bath salts' stun immunity) are `crate::buffs`.

use bevy::prelude::*;
use ss13_reagents::Chemistry;
use ss13_reagents::metabolism::Metabolized;
use ss13_shared::chat::Span;
use ss13_shared::{Dir, TilePos};
use ss13_world::MoveRequest;

use crate::chem::{blood_amount, name_of, tell_mob, visible};
use crate::combat::CombatRng;
use crate::health::DamageType;
use crate::medical::organs::{self, Slot};
use crate::mood::MoodEvent;
use crate::status_effects::{self, Incap, Timed};

pub const SPACE_DRUGS: &str = "/datum/reagent/drug/space_drugs";
pub const CANNABIS: &str = "/datum/reagent/drug/cannabis";
pub const NICOTINE: &str = "/datum/reagent/drug/nicotine";
pub const KROKODIL: &str = "/datum/reagent/drug/krokodil";
pub const METH: &str = "/datum/reagent/drug/methamphetamine";
pub const BATH_SALTS: &str = "/datum/reagent/drug/bath_salts";
pub const ARANESP: &str = "/datum/reagent/drug/aranesp";
pub const HAPPINESS: &str = "/datum/reagent/drug/happiness";
pub const PUMPUP: &str = "/datum/reagent/drug/pumpup";
pub const MAINT_POWDER: &str = "/datum/reagent/drug/maint/powder";
pub const MAINT_SLUDGE: &str = "/datum/reagent/drug/maint/sludge";
pub const MAINT_TAR: &str = "/datum/reagent/drug/maint/tar";
pub const MUSHROOM: &str = "/datum/reagent/drug/mushroomhallucinogen";
pub const KRONKAINE: &str = "/datum/reagent/drug/kronkaine";
pub const SYNDOL: &str = "/datum/reagent/drug/syndol";

/// `/datum/movespeed_modifier/reagent/methamphetamine`, deciseconds a tile.
const METH_SPEED: f32 = -0.65;
/// `/datum/actionspeed_modifier/kronkaine`.
const KRONKAINE_ACTIONSPEED: f32 = -0.5;
const KRONKAINE_ACTIONSPEED_ID: &str = "kronkaine";

const fn ev(id: &'static str, description: &'static str, change: f32, timeout: f32) -> MoodEvent {
    MoodEvent {
        id,
        description,
        mood_change: change,
        timeout,
        hidden: false,
    }
}

/// `/datum/mood_event/*` of `drug_events.dm` and `drink_events.dm` (BandaStation's texts).
pub(crate) const SMOKED: MoodEvent = ev("smoked", "Я недавно курил.", 2.0, 360.0);
const NARCOTIC_HEAVY: MoodEvent = ev("narcotic_heavy", "Я как будто окутан в хлопок!", 9.0, 180.0);
const STIMULANT_MEDIUM: MoodEvent = ev(
    "stimulant_medium",
    "Я наполнен энергией! Такое ощущение, что я могу всё!",
    4.0,
    180.0,
);
const STIMULANT_HEAVY: MoodEvent = ev(
    "stimulant_heavy",
    "ЕЕЕ-ХАААА! ХА ХА ХА ХА ХАА! Аххх.",
    6.0,
    180.0,
);
const HAPPINESS_DRUG: MoodEvent = ev("happiness_drug", "Я ничего не чувствую...", 50.0, 0.0);
const HAPPINESS_GOOD_OD: MoodEvent = ev("happiness_drug_good_od", "ДА! ДА!! ДА!!!", 100.0, 30.0);
const HAPPINESS_BAD_OD: MoodEvent = ev("happiness_drug_bad_od", "НЕТ! НЕТ!! НЕТ!!!", -100.0, 30.0);
const HIGH: MoodEvent = ev(
    "high",
    "Вооооу, чувааааак... Меня таааак качааааает...",
    6.0,
    0.0,
);
const SWEET_COFFEE: MoodEvent = ev(
    "sweetcoffee",
    "The bitter sweet taste of coffee was not too bad",
    2.0,
    300.0,
);
const SWEET_TEA: MoodEvent = ev(
    "sweettea",
    "Let your worries dissolve like sugar in tea.",
    4.0,
    150.0,
);

/// The reagent movespeed modifiers on a mob by id (deciseconds a tile), already added to
/// [`crate::health::MoveSpeedModifier`].
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct ReagentSpeedMods(pub Vec<(&'static str, f32)>);

/// tg `add_movespeed_modifier` (`Some`) / `remove_movespeed_modifier` (`None`) of a reagent or
/// withdrawal modifier: the shared [`crate::health::MoveSpeedModifier`] moves by the difference.
pub fn set_speed_mod(world: &mut World, mob: Entity, id: &'static str, slowdown: Option<f32>) {
    let Ok(mut e) = world.get_entity_mut(mob) else {
        return;
    };
    let mut mods = e.take::<ReagentSpeedMods>().unwrap_or_default();
    let old = mods
        .0
        .iter()
        .find(|(k, _)| *k == id)
        .map_or(0.0, |&(_, v)| v);
    mods.0.retain(|(k, _)| *k != id);
    if let Some(v) = slowdown {
        mods.0.push((id, v));
    }
    let delta = slowdown.unwrap_or(0.0) - old;
    if !mods.0.is_empty() {
        e.insert(mods);
    }
    if delta != 0.0 {
        let current = e
            .get::<crate::health::MoveSpeedModifier>()
            .map_or(0.0, |m| m.0);
        e.insert(crate::health::MoveSpeedModifier(current + delta));
    }
}

/// The drugs whose `on_mob_metabolize` ran on this mob: their `on_mob_end_metabolize` comes
/// when they leave the blood. `crate::chem` keeps metabolizing a mob with it.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct DrugsActive(pub Vec<&'static str>);

const STARTS: [&str; 5] = [METH, HAPPINESS, MUSHROOM, KRONKAINE, SYNDOL];

fn on_metabolize(world: &mut World, mob: Entity, path: &'static str) {
    match path {
        METH => set_speed_mod(world, mob, METH, Some(METH_SPEED)),
        HAPPINESS => crate::mood::add_event(world, mob, "happiness_drug", &HAPPINESS_DRUG),
        MUSHROOM => crate::mood::add_event(world, mob, "tripping", &HIGH),
        KRONKAINE => crate::actionspeed::add_actionspeed_modifier(
            world,
            mob,
            KRONKAINE_ACTIONSPEED_ID,
            KRONKAINE_ACTIONSPEED,
        ),
        _ => {}
    }
}

fn on_end_metabolize(world: &mut World, mob: Entity, path: &'static str) {
    match path {
        METH => set_speed_mod(world, mob, METH, None),
        // `on_mob_delete`.
        HAPPINESS => crate::mood::clear_event(world, mob, "happiness_drug"),
        MUSHROOM => crate::mood::clear_event(world, mob, "tripping"),
        KRONKAINE => {
            crate::actionspeed::remove_actionspeed_modifier(world, mob, KRONKAINE_ACTIONSPEED_ID);
        }
        SYNDOL => crate::hallucination::adjust_hallucinations(world, mob, -16.0),
        _ => {}
    }
}

fn prob(world: &mut World, percent: f32, secs: f32) -> bool {
    let mut rng = world.get_resource_or_init::<CombatRng>();
    crate::medical::wounds::spt_prob(&mut rng, percent, secs)
}

fn chance(world: &mut World, percent: f32) -> bool {
    world.get_resource_or_init::<CombatRng>().chance(percent)
}

fn rand(world: &mut World, low: u32, high: u32) -> u32 {
    world.get_resource_or_init::<CombatRng>().range(low, high)
}

fn pick<'a>(world: &mut World, list: &[&'a str]) -> &'a str {
    list[rand(world, 0, list.len() as u32 - 1) as usize]
}

/// `emote(pick(keys))`; keys without a human emote here do nothing, as tg's unknown keys.
fn emote(world: &mut World, mob: Entity, keys: &[&str]) {
    let key = pick(world, keys);
    if let Some(emote) = ss13_chat::emote::find(key) {
        crate::mobs::defense::emote(world, mob, emote.message);
    }
}

fn hurt(world: &mut World, mob: Entity, kind: DamageType, amount: f32) {
    crate::mobs::defense::hurt(world, mob, kind, amount, None);
}

/// `step(mob, pick(GLOB.cardinals))` of a mob on a turf (not in space) that can move.
fn random_step(world: &mut World, mob: Entity) {
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    if crate::construction::is_space(world, pos)
        || status_effects::incapacitation(world, mob).immobilized
    {
        return;
    }
    let dir = Dir::CARDINALS[rand(world, 0, 3) as usize];
    world.write_message(MoveRequest { entity: mob, dir });
}

fn immobility(world: &mut World, mob: Entity, secs: f32) {
    if let Some(mut effects) = status_effects::effects_mut(world, mob) {
        effects.adjust_all_immobility(secs);
    }
}

fn drop_all(world: &mut World, mob: Entity) {
    crate::medical::diseases::effects::drop_held(world, mob, true, false);
}

fn brain(world: &mut World, mob: Entity, amount: f32) {
    organs::adjust_organ_loss(world, mob, Slot::Brain, amount);
}

/// `crate::chem` hands over the reagents metabolized this tick (tg `metabolize_reagent`'s
/// `overdose_start`, `overdose_process`, `on_mob_life`, then `on_mob_end_metabolize`).
// Cost: every 2 s per mob with reagents, one match per metabolized reagent.
pub fn metabolized(
    world: &mut World,
    chem: &Chemistry,
    mob: Entity,
    list: &[Metabolized],
    secs: f32,
) {
    for m in list {
        let path = chem.def(m.id).path.as_str();
        if let Some(&start) = STARTS.iter().find(|p| **p == path)
            && world
                .get::<DrugsActive>(mob)
                .is_none_or(|a| !a.0.contains(&start))
        {
            let mut active = world.get::<DrugsActive>(mob).cloned().unwrap_or_default();
            active.0.push(start);
            world.entity_mut(mob).insert(active);
            on_metabolize(world, mob, start);
        }
        if m.overdose_started {
            overdose_start(world, chem, mob, path);
        }
        if m.overdosed {
            overdose(world, mob, path, m, secs);
        }
        life(world, mob, path, m, secs);
    }
    let Some(active) = world.get::<DrugsActive>(mob).cloned() else {
        return;
    };
    let (gone, left): (Vec<&'static str>, Vec<&'static str>) = active
        .0
        .into_iter()
        .partition(|p| blood_amount(world, mob, p) <= 0.0);
    if gone.is_empty() {
        return;
    }
    if left.is_empty() {
        world.entity_mut(mob).remove::<DrugsActive>();
    } else {
        world.entity_mut(mob).insert(DrugsActive(left));
    }
    for path in gone {
        on_end_metabolize(world, mob, path);
    }
}

/// `overdose_start`: every reagent's `[type]_overdose` moodlet (the line is
/// `ss13_reagents::metabolism`'s), then the drug's own.
fn overdose_start(world: &mut World, chem: &Chemistry, mob: Entity, path: &str) {
    let name = chem
        .id(path)
        .map(|id| chem.def(id).name.clone())
        .unwrap_or_default();
    let mut mood = crate::mood::ActiveMood::new(
        &format!("{path}_overdose"),
        &ev("overdose", "", -8.0, 300.0),
    );
    mood.description = format!("Кажется, я немного переборщил с {name}!");
    let now = world.resource::<Time>().elapsed_secs_f64();
    if let Some(mut m) = world.get_mut::<crate::mood::Mood>(mob) {
        m.add_instance(now, mood);
    }
    match path {
        SPACE_DRUGS => tell_mob(world, mob, Span::BoldWarning, "You start tripping hard!"),
        PUMPUP => tell_mob(
            world,
            mob,
            Span::BoldWarning,
            "You can't stop shaking, your heart beats faster and faster...",
        ),
        _ => {}
    }
}

fn overdose(world: &mut World, mob: Entity, path: &str, m: &Metabolized, secs: f32) {
    let r = m.ratio;
    match path {
        SPACE_DRUGS => {
            if crate::hallucination::remaining(world, mob) < m.held && prob(world, 10.0, secs) {
                crate::hallucination::adjust_hallucinations(world, mob, 10.0);
            }
        }
        NICOTINE => {
            hurt(world, mob, DamageType::Tox, 0.4 * r * secs);
            hurt(world, mob, DamageType::Oxy, 4.4 * r * secs);
        }
        KROKODIL => {
            brain(world, mob, 0.25 * r * secs);
            hurt(world, mob, DamageType::Tox, 0.25 * r * secs);
        }
        METH => {
            if (2.67 * r * secs).round() >= 1.0 {
                random_step(world, mob);
            }
            if prob(world, 10.0, secs) {
                emote(world, mob, &["laugh"]);
            }
            if prob(world, 18.0, secs) {
                let name = ss13_chat::capitalize(&name_of(world, mob));
                visible(
                    world,
                    mob,
                    &format!("{name}'s hands flip out and flail everywhere!"),
                    &format!("{name}'s hands flip out and flail everywhere!"),
                    Span::Danger,
                );
                drop_all(world, mob);
            }
            hurt(world, mob, DamageType::Tox, 0.67 * r * secs);
            let brain_hit = rand(world, 5, 10) as f32 / 10.0;
            brain(world, mob, 0.67 * brain_hit * r * secs);
        }
        BATH_SALTS => {
            crate::hallucination::adjust_hallucinations(world, mob, 10.0 * r * secs);
            if (8.0 * r * secs).round() >= 1.0 {
                random_step(world, mob);
            }
            if prob(world, 10.0, secs) {
                emote(world, mob, &["twitch", "drool", "moan"]);
            }
            if prob(world, 28.0, secs) {
                drop_all(world, mob);
            }
        }
        HAPPINESS => {
            if prob(world, 16.0, secs) {
                match rand(world, 1, 3) {
                    1 => {
                        emote(world, mob, &["laugh"]);
                        crate::mood::add_event(world, mob, "happiness_drug", &HAPPINESS_GOOD_OD);
                    }
                    2 => {
                        emote(world, mob, &["sway"]);
                        status_effects::set_timed(world, mob, Timed::Dizziness, 50.0, true);
                    }
                    _ => {
                        emote(world, mob, &["frown"]);
                        crate::mood::add_event(world, mob, "happiness_drug", &HAPPINESS_BAD_OD);
                    }
                }
            }
            brain(world, mob, 0.5 * r * secs);
        }
        PUMPUP => {
            status_effects::set_timed(world, mob, Timed::Jitter, 2.5 * r * secs, true);
            if prob(world, 2.5, secs) {
                drop_all(world, mob);
            }
            if prob(world, 7.5, secs) {
                emote(world, mob, &["twitch", "drool"]);
            }
            if prob(world, 10.0, secs) {
                add_losebreath(world, mob, 1.0);
                status_effects::adjust_stamina(world, mob, 4.0);
            }
            if prob(world, 7.5, secs) {
                hurt(world, mob, DamageType::Tox, 2.0);
            }
        }
        MAINT_POWDER => brain(world, mob, 6.0 * r * secs),
        MAINT_SLUDGE => {
            hurt(world, mob, DamageType::Tox, 0.25 * r * secs);
            if prob(world, 5.0, secs) {
                hurt(world, mob, DamageType::Tox, 2.5 * r);
                crate::medical::diseases::effects::vomit(world, mob, 10.0, true, false, 1);
            }
        }
        MAINT_TAR => {
            hurt(world, mob, DamageType::Tox, 5.0 * r * secs);
            organs::adjust_organ_loss(world, mob, Slot::Liver, 3.0 * r * secs);
        }
        KRONKAINE => {
            organs::adjust_organ_loss(world, mob, Slot::Heart, 0.34 * r * secs);
            status_effects::set_timed(world, mob, Timed::Jitter, 13.34 * r * secs, true);
            if prob(world, 10.0, secs) {
                let line = pick(
                    world,
                    &[
                        "Your heart is racing!",
                        "Your ears are ringing!",
                        "You sweat like a pig!",
                        "You clench your jaw and grind your teeth.",
                        "You feel prickles of pain in your chest.",
                    ],
                );
                tell_mob(world, mob, Span::Danger, line);
            }
        }
        _ => {}
    }
}

fn add_losebreath(world: &mut World, mob: Entity, amount: f32) {
    if let Some(mut r) = world.get_mut::<crate::breath::Respiration>(mob) {
        r.losebreath += amount;
    }
}

fn notice(world: &mut World, mob: Entity, percent: f32, secs: f32, lines: &[&str]) {
    if prob(world, percent, secs) {
        let line = pick(world, lines);
        tell_mob(world, mob, Span::Notice, line);
    }
}

fn life(world: &mut World, mob: Entity, path: &str, m: &Metabolized, secs: f32) {
    let r = m.ratio;
    match path {
        SPACE_DRUGS => {
            if prob(world, 5.0, secs) {
                random_step(world, mob);
            }
            if prob(world, 3.5, secs) {
                emote(world, mob, &["twitch", "drool", "moan", "giggle"]);
            }
        }
        CANNABIS => {
            notice(
                world,
                mob,
                1.0,
                secs,
                &[
                    "You feel relaxed.",
                    "You feel calmed.",
                    "Your mouth feels dry.",
                    "You could use some water.",
                    "Your heart beats quickly.",
                    "You feel clumsy.",
                    "You crave junk food.",
                    "You notice you've been moving more slowly.",
                ],
            );
            if prob(world, 2.0, secs) {
                emote(world, mob, &["smile", "laugh", "giggle"]);
            }
            if let Some(mut hunger) = world.get_mut::<crate::hunger::Hunger>(mob) {
                hunger.adjust(-0.6 * r * secs);
            }
            let lying = world
                .get::<crate::health::BodyPosition>(mob)
                .is_some_and(|p| p.lying());
            let sleeping = world
                .get::<status_effects::StatusEffects>(mob)
                .is_some_and(|e| e.sleeping > 0.0);
            if prob(world, 4.0, secs) && lying && !sleeping {
                tell_mob(world, mob, Span::Warning, "You doze off...");
                status_effects::apply(world, mob, Incap::Sleeping, 10.0);
            }
        }
        NICOTINE => {
            notice(
                world,
                mob,
                0.5,
                secs,
                &[
                    "You feel relaxed.",
                    "You feel calmed.",
                    "You feel alert.",
                    "You feel rugged.",
                ],
            );
            crate::mood::add_event(world, mob, "smoked", &SMOKED);
            status_effects::set_timed(world, mob, Timed::Jitter, 0.0, false);
            immobility(world, mob, -20.0 * r * secs);
        }
        KROKODIL => {
            notice(
                world,
                mob,
                2.5,
                secs,
                &[
                    "You feel calm.",
                    "You feel collected.",
                    "You feel like you need to relax.",
                ],
            );
            crate::mood::add_event(world, mob, "smacked out", &NARCOTIC_HEAVY);
            if m.current_cycle != 36
                || m.creation_purity > 0.6
                || world.get::<crate::human::Husk>(mob).is_some()
            {
                return;
            }
            tell_mob(world, mob, Span::BoldWarning, "Your skin falls off easily!");
            if let Some(mut look) = world.get_mut::<crate::human::HumanLook>(mob) {
                look.facial_hair_style = None;
                look.hair_style = None;
            }
            // shortcut: a burn husk (synthflesh cures it), not tg's zombie-sourced one.
            crate::death::become_husk(world, mob);
            hurt(world, mob, DamageType::Brute, 25.0);
        }
        METH => {
            notice(
                world,
                mob,
                2.5,
                secs,
                &[
                    "You feel hyper.",
                    "You feel like you need to go faster.",
                    "You feel like you can run the world.",
                    "You understand now.",
                ],
            );
            crate::mood::add_event(world, mob, "tweaking", &STIMULANT_MEDIUM);
            // `-26.67` deciseconds.
            immobility(world, mob, -2.667 * r * secs);
            status_effects::adjust_stamina(world, mob, -3.34 * r * secs);
            status_effects::set_timed(world, mob, Timed::Jitter, 2.67 * r * secs, true);
            let hit = rand(world, 1, 4) as f32;
            brain(world, mob, 0.67 * hit * r * secs);
            if prob(world, 2.5, secs) {
                emote(world, mob, &["twitch", "shiver"]);
            }
        }
        BATH_SALTS => {
            notice(
                world,
                mob,
                2.5,
                secs,
                &[
                    "You feel amped up.",
                    "You feel ready.",
                    "You feel like you can push it to the limit.",
                ],
            );
            crate::mood::add_event(world, mob, "salted", &STIMULANT_HEAVY);
            status_effects::adjust_stamina(world, mob, -6.0 * r * secs);
            brain(world, mob, 4.0 * r * secs);
            // The hallucinations are `crate::hallucination::reagents`.
            random_step(world, mob);
        }
        ARANESP => {
            notice(
                world,
                mob,
                2.5,
                secs,
                &[
                    "You feel amped up.",
                    "You feel ready.",
                    "You feel like you can push it to the limit.",
                ],
            );
            status_effects::adjust_stamina(world, mob, -18.0 * r * secs);
            hurt(world, mob, DamageType::Tox, 0.5 * r * secs);
            if prob(world, 30.0, secs) {
                add_losebreath(world, mob, 1.0);
                hurt(world, mob, DamageType::Oxy, 1.0);
            }
        }
        HAPPINESS => {
            status_effects::set_timed(world, mob, Timed::Jitter, 0.0, false);
            status_effects::set_timed(world, mob, Timed::Confusion, 0.0, false);
            brain(world, mob, 0.2 * r * secs);
        }
        PUMPUP => {
            status_effects::set_timed(world, mob, Timed::Jitter, 2.5 * r * secs, true);
            notice(
                world,
                mob,
                2.5,
                secs,
                &["Go! Go! GO!", "You feel ready...", "You feel invincible..."],
            );
            if prob(world, 7.5, secs) {
                add_losebreath(world, mob, 1.0);
                hurt(world, mob, DamageType::Tox, 2.0);
            }
        }
        MAINT_POWDER => brain(world, mob, 0.1 * r * secs),
        MAINT_SLUDGE => hurt(world, mob, DamageType::Tox, 0.125 * r * secs),
        MAINT_TAR => {
            // `-10` deciseconds.
            immobility(world, mob, -r * secs);
            organs::adjust_organ_loss(world, mob, Slot::Liver, 1.5 * r * secs);
        }
        MUSHROOM => {
            status_effects::set_timed(
                world,
                mob,
                Timed::Slurring(status_effects::SlurKind::Generic),
                2.5 * r * secs,
                true,
            );
            // DM `switch` ranges: the first match wins (6 and 11 belong to the lower one).
            let (jitter, percent) = match m.current_cycle {
                2..=6 => (0.0, 5.0),
                7..=11 => (50.0, 10.0),
                c if c > 11 => (100.0, 16.0),
                _ => (0.0, 0.0),
            };
            if jitter > 0.0 {
                status_effects::set_timed(world, mob, Timed::Jitter, jitter * r * secs, true);
            }
            if percent > 0.0 && prob(world, percent, secs) {
                emote(world, mob, &["twitch", "giggle"]);
            }
        }
        KRONKAINE => {
            crate::mood::add_event(world, mob, "tweaking", &STIMULANT_MEDIUM);
            let heart = 0.67 * (0.1 + 0.04 * m.held) * r * secs;
            if let Some(organ) = organs::organ(world, mob, Slot::Heart) {
                organs::adjust_organ_loss(world, mob, Slot::Heart, heart);
                let damage = world.get::<organs::Organ>(organ).map_or(0.0, |o| o.damage);
                if damage >= 75.0 && chance(world, 15.0) {
                    tell_mob(
                        world,
                        mob,
                        Span::BoldWarning,
                        "You feel like your heart is about to explode!",
                    );
                }
            }
            status_effects::set_timed(world, mob, Timed::Jitter, 13.34 * r * secs, true);
            if let Some(mut effects) = status_effects::effects_mut(world, mob) {
                effects.sleeping = (effects.sleeping - 1.34 * r * secs).max(0.0);
            }
            status_effects::adjust_timed(world, mob, Timed::Drowsiness, -6.67 * r * secs, None);
            status_effects::adjust_stamina(world, mob, -0.54 * m.held * r * secs);
        }
        SYNDOL => {
            organs::adjust_organ_loss(world, mob, Slot::Liver, 0.5 * r * secs);
            let haloperidol = blood_amount(world, mob, "/datum/reagent/medicine/haloperidol");
            if haloperidol < 3.0 {
                crate::hallucination::adjust_hallucinations_up_to(world, mob, 4.0 * r * secs, 30.0);
            }
        }
        _ => {}
    }
}

/// `/datum/stacked_metabolization_effect`s (`metabolize`: more than one reagent metabolized):
/// every requirement needs its own metabolized reagent of that type or a subtype.
pub fn stacked_effects(
    world: &mut World,
    chem: &Chemistry,
    mob: Entity,
    list: &[Metabolized],
    secs: f32,
) {
    if list.len() < 2 {
        return;
    }
    const COFFEE: &str = "/datum/reagent/consumable/coffee";
    const SUGAR: &str = "/datum/reagent/consumable/sugar";
    const TEA: &str = "/datum/reagent/consumable/tea";
    const ICECOFFEE: &str = "/datum/reagent/consumable/icecoffee";
    const HOT_ICE_COFFEE: &str = "/datum/reagent/consumable/hot_ice_coffee";
    let paths: Vec<&str> = list.iter().map(|m| chem.def(m.id).path.as_str()).collect();
    let average = list.iter().map(|m| m.ratio).sum::<f32>() / list.len() as f32;
    let has_mood = |world: &World, category: &str| {
        world
            .get::<crate::mood::Mood>(mob)
            .is_some_and(|m| m.get(category).is_some())
    };
    if requirements_met(&paths, &[COFFEE, SUGAR]) && !has_mood(&*world, "sweet_coffee") {
        crate::mood::add_event(world, mob, "sweet_coffee", &SWEET_COFFEE);
    }
    if requirements_met(&paths, &[TEA, SUGAR]) && !has_mood(&*world, "sweet_tea") {
        crate::mood::add_event(world, mob, "sweet_tea", &SWEET_TEA);
    }
    if requirements_met(&paths, &[COFFEE, ICECOFFEE]) && prob(world, 30.0, secs) {
        hurt(world, mob, DamageType::Oxy, -2.0 * average);
    }
    if requirements_met(&paths, &[COFFEE, ICECOFFEE, HOT_ICE_COFFEE]) && prob(world, 70.0, secs) {
        hurt(world, mob, DamageType::Oxy, -3.0 * average);
    }
}

/// `check_and_apply`: each metabolized reagent takes the first requirement it is a type of.
pub fn requirements_met(metabolized: &[&str], requirements: &[&str]) -> bool {
    let mut needed: Vec<&str> = requirements.to_vec();
    for path in metabolized {
        if let Some(i) = needed
            .iter()
            .position(|req| ss13_mapload::fidelity::is_under(path, req))
        {
            needed.remove(i);
        }
    }
    needed.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stacked_requirements_need_one_reagent_each() {
        let coffee = "/datum/reagent/consumable/coffee";
        let sugar = "/datum/reagent/consumable/sugar";
        assert!(requirements_met(&[sugar, coffee], &[coffee, sugar]));
        assert!(!requirements_met(&[coffee], &[coffee, sugar]));
        assert!(!requirements_met(&[coffee, coffee], &[coffee, sugar]));
        assert!(requirements_met(
            &[
                "/datum/reagent/consumable/coffee/x",
                sugar,
                "/datum/reagent/water"
            ],
            &[coffee, sugar]
        ));
    }

    #[test]
    fn meth_speeds_up_until_it_leaves_the_blood() {
        use std::sync::Arc;
        let def = ss13_reagents::ReagentDef {
            path: METH.into(),
            name: "Methamphetamine".into(),
            description: String::new(),
            color: "#78C8FA".into(),
            taste_description: String::new(),
            taste_mult: 1.0,
            metabolization_rate: 0.15,
            overdose_threshold: 20.0,
            ph: 5.0,
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
            boozepwr: 0.0,
            brute_heal: 0.0,
            nutriment_factor: 0.0,
            penetrates_skin: 4,
            default_container: String::new(),
            addiction_types: Vec::new(),
        };
        let chem = Chemistry::new(ss13_reagents::ChemData {
            reagents: vec![def],
            ..Default::default()
        });
        let mut world = World::new();
        world.init_resource::<Time>();
        world.insert_resource(crate::chem::Chem(Arc::new(chem.clone())));
        let mut blood = crate::chem::mob_holder();
        blood.add_reagent(&chem, 0, 5.0, 310.0, None, None, true, false);
        let mob = world
            .spawn((
                crate::chem::MobReagents {
                    blood,
                    stomach: crate::chem::mob_holder(),
                    drunk: 0.0,
                },
                crate::health::MoveSpeedModifier(0.0),
            ))
            .id();
        let m = Metabolized {
            id: 0,
            volume: 0.3,
            ratio: 0.75,
            overdosed: false,
            overdose_started: false,
            current_cycle: 1,
            creation_purity: 1.0,
            held: 5.0,
        };
        metabolized(&mut world, &chem, mob, &[m], 2.0);
        assert_eq!(world.get::<DrugsActive>(mob).unwrap().0, vec![METH]);
        let speed = world
            .get::<crate::health::MoveSpeedModifier>(mob)
            .unwrap()
            .0;
        assert_eq!(speed, METH_SPEED);
        world
            .get_mut::<crate::chem::MobReagents>(mob)
            .unwrap()
            .blood = crate::chem::mob_holder();
        metabolized(&mut world, &chem, mob, &[], 2.0);
        assert!(world.get::<DrugsActive>(mob).is_none());
        assert_eq!(
            world
                .get::<crate::health::MoveSpeedModifier>(mob)
                .unwrap()
                .0,
            0.0
        );
    }

    #[test]
    fn speed_mods_add_and_remove_their_difference() {
        let mut world = World::new();
        let mob = world.spawn(crate::health::MoveSpeedModifier(1.0)).id();
        set_speed_mod(&mut world, mob, METH, Some(METH_SPEED));
        set_speed_mod(&mut world, mob, METH, Some(METH_SPEED));
        let speed = world
            .get::<crate::health::MoveSpeedModifier>(mob)
            .unwrap()
            .0;
        assert!((speed - 0.35).abs() < 1e-6, "{speed}");
        set_speed_mod(&mut world, mob, METH, None);
        let speed = world
            .get::<crate::health::MoveSpeedModifier>(mob)
            .unwrap()
            .0;
        assert!((speed - 1.0).abs() < 1e-6, "{speed}");
        assert!(world.get::<ReagentSpeedMods>(mob).is_none());
    }
}
