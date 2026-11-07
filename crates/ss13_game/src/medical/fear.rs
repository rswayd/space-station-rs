//! Fear (server), after tg `code/datums/components/fearful/` (`/datum/component/fearful`, its
//! terror handlers: `sources/_sources.dm`, `sources/phobia.dm`, `effects.dm`) for the phobia
//! and monophobia brain traumas (`crate::medical::traumas`).
//!
//! - [`Fearful`] on a mob is the component: its terror buildup, the sources feeding it (each
//!   phobia, monophobia) and the state of its effects. [`crate::medical::traumas::refresh`] keeps
//!   it in step with the brain's traumas ([`set_sources`]); every `Life()` ([`tick`]) runs the
//!   sources, then the effects, then lets the terror ebb when nothing scared the mob.
//! - Sources: a phobia freaks out at a scary thing in view (checked every 5 s: objects, turfs,
//!   mobs, a human's species or visible gear, [`super::phobia_data`]) and at a scary word heard
//!   ([`hear_words`], from `ss13_chat`'s [`Spoke`]); saying one makes the speaker stutter now
//!   and then. Monophobia builds up while no player or pet is in view.
//! - Effects: shaking and dizziness, stuttering, a skipping heart (oxygen loss) up to a heart
//!   attack, panic (blurry eyes, panic attacks), startles (phobias), sickness (monophobia).
//! - A hug calms (`comfort_owner`, [`on_hug`]); a hug from something the mob fears scares it.
// shortcut: no personalities (fear multiplier 1), no `TRAIT_FEARLESS` or
// `TRAIT_MIND_TEMPORARILY_GONE` (no sources), no examine or mood-check lines, no tunnel vision,
// heartbeat loop or panic breathing (`losebreath`), no stagger or spooked slowdown, no forced
// "AAAAH!!" say and no `jump` emote (not in the chat's emotes), no phobia highlight in the
// heard line or small-voice span on the said one (the stutter only), no hug block (the hug
// still lands); vomiting is not delayed by 5 s; the scary word reacts at once (tg: 1 s later);
// the component ticks with `SSmobs` (2 s) instead of `SSdcs`, every rate is per second. The
// quirks (nyctophobia, claustrophobia, pyrophobia, phobia) are not ported.

use std::collections::HashSet;

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_chat::server::Spoke;
use ss13_mapload::Opaque;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_world::SpatialIndex;

use super::phobia_data::{PHOBIAS, PhobiaDef};
use super::wounds::spt_prob;
use crate::combat::CombatRng;
use crate::health::{DamageType, Health, Stat};
use crate::inventory::Inventory;
use crate::mood::MoodEvent;
use crate::obj_damage::is_under;
use crate::server::PlayerMob;
use crate::status_effects::{self, Incap, Timed};

/// `TERROR_MESSAGE_CD`.
const MESSAGE_CD: f64 = 15.0;
/// `TERROR_BUILDUP_PASSIVE_DECREASE`, per second.
const PASSIVE_DECREASE: f32 = 15.0;
/// `TERROR_BUILDUP_FEAR`, `_TERROR`, `_PANIC`, `_PASSIVE_MAXIMUM`, `_HEART_ATTACK`, `_MAXIMUM`.
pub const FEAR: f32 = 150.0;
pub const TERROR: f32 = 300.0;
pub const PANIC: f32 = 500.0;
pub const PASSIVE_MAXIMUM: f32 = 600.0;
pub const HEART_ATTACK: f32 = 800.0;
pub const MAXIMUM: f32 = 1000.0;
/// `PANIC_ATTACK_TERROR_AMOUNT`, `HUG_TERROR_AMOUNT`.
const PANIC_ATTACK: f32 = 50.0;
const HUG: f32 = 90.0;
/// `PHOBIA_CHECK_DELAY`, `PHOBIA_WORD_TERROR_BUILDUP`, `PHOBIA_FREAKOUT_TERROR_BUILDUP`.
const PHOBIA_CHECK_DELAY: f64 = 5.0;
const PHOBIA_WORD: f32 = 40.0;
const PHOBIA_FREAKOUT: f32 = 120.0;
/// `scare_cooldown`.
const SCARE_CD: f64 = 12.0;
/// `TERROR_STARTLE_COOLDOWN`, `TERROR_STARTLE_MINIMUM_DIFFERENCE`.
const STARTLE_CD: f64 = 12.0;
const STARTLE_DIFFERENCE: f32 = 40.0;
/// `world.view`.
const VIEW: u32 = 7;

/// `/datum/mood_event/phobia`, `/datum/mood_event/spooked`, `/datum/mood_event/startled`.
const PHOBIA_MOOD: MoodEvent = MoodEvent {
    id: "phobia",
    description: "Я увидел нечто очень ужасающее!",
    mood_change: -4.0,
    timeout: 240.0,
    hidden: false,
};
const SPOOKED_MOOD: MoodEvent = MoodEvent {
    id: "spooked",
    description: "Грохот этих костей... Он до сих пор преследует меня.",
    mood_change: -4.0,
    timeout: 240.0,
    hidden: false,
};
const STARTLED_MOOD: MoodEvent = MoodEvent {
    id: "startled",
    description: "Услышав это слово, я подумал о чём-то страшном.",
    mood_change: -1.0,
    timeout: 60.0,
    hidden: false,
};

/// `/datum/terror_handler/phobia_source` of one phobia.
#[derive(Clone, Debug, PartialEq)]
pub struct PhobiaSource {
    /// Index into [`PHOBIAS`].
    pub phobia: usize,
    check_at: f64,
    scare_until: f64,
}

/// `/datum/component/fearful` on a mob.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct Fearful {
    /// `terror_buildup`.
    pub buildup: f32,
    /// `last_tick_buildup`.
    last_tick: f32,
    pub phobias: Vec<PhobiaSource>,
    /// `/datum/terror_handler/simple_source/monophobia` (with `vomiting`).
    pub monophobia: bool,
    mono_message_at: f64,
    jitter_message_at: f64,
    heart_message_at: f64,
    /// `panic`'s `active` and the end of its panic attack (`active_attack`).
    panic_active: bool,
    attack_until: Option<f64>,
    startle_at: f64,
    /// `/datum/terror_handler/simple_source/nyctophobia/terrified` (`crate::afflictions`).
    pub terrified: bool,
    /// Its `active` (the nyctophobia moodlet).
    terrified_active: bool,
}

/// The phobia of `key` (an index into [`PHOBIAS`]).
pub fn phobia_index(key: &str) -> Option<usize> {
    PHOBIAS.iter().position(|p| p.key == key)
}

/// Keeps `mob`'s [`Fearful`] in step with its traumas: `phobias` (indices into [`PHOBIAS`]) and
/// monophobia. No source left: the component goes (`RemoveComponentSource`).
pub fn set_sources(world: &mut World, mob: Entity, phobias: &[usize], monophobia: bool) {
    let Ok(mut entity) = world.get_entity_mut(mob) else {
        return;
    };
    let terrified = entity.get::<Fearful>().is_some_and(|f| f.terrified);
    if phobias.is_empty() && !monophobia && !terrified {
        entity.remove::<Fearful>();
        return;
    }
    let mut fear = entity.get::<Fearful>().cloned().unwrap_or_default();
    fear.phobias.retain(|s| phobias.contains(&s.phobia));
    for &phobia in phobias {
        if !fear.phobias.iter().any(|s| s.phobia == phobia) {
            fear.phobias.push(PhobiaSource {
                phobia,
                check_at: 0.0,
                scare_until: 0.0,
            });
        }
    }
    fear.monophobia = monophobia;
    if entity.get::<Fearful>() != Some(&fear) {
        entity.insert(fear);
    }
}

/// `FEAR_SCALING(base, min, max)`.
fn scaling(buildup: f32, base: f32, min: f32, max: f32) -> f32 {
    (base * (buildup - min) / (max - min)).clamp(0.0, base)
}

fn say_to(world: &mut World, mob: Entity, span: Span, text: impl Into<String>) {
    super::diseases::say_to(world, mob, span, text);
}

/// `/datum/component/fearful/process`: the sources, then the effects, then the terror ebbs
/// when nothing added to it. Run with the traumas' `on_life` (a living mob).
pub fn tick(world: &mut World, mob: Entity, secs: f32, rng: &mut CombatRng) {
    let Some(mut fear) = world.get::<Fearful>(mob).cloned() else {
        return;
    };
    let now = world.resource::<Time>().elapsed_secs_f64();
    let unconscious = status_effects::knocked_out(world, mob);
    let mut adjustment = 0.0;
    // `get_fear_multiplier`: the personalities (Cowardly, Paranoid, Brave).
    let modifier = crate::personality::fear_multiplier(world, mob);
    let add = |fear: &mut Fearful, amount: f32| {
        fear.buildup = (fear.buildup + amount * modifier).clamp(0.0, MAXIMUM);
        amount
    };
    // Sources.
    for i in 0..fear.phobias.len() {
        let amount = phobia_tick(world, mob, &mut fear, i, now, unconscious);
        adjustment += add(&mut fear, amount);
    }
    if fear.monophobia {
        let amount = monophobia_tick(world, mob, &mut fear, now, unconscious, secs, rng);
        adjustment += add(&mut fear, amount);
    }
    if fear.terrified {
        let amount = terrified_tick(world, mob, &mut fear, unconscious, secs);
        adjustment += add(&mut fear, amount);
    }
    // Effects.
    if !unconscious {
        jittering(world, mob, &mut fear, now, secs, rng);
        if fear.buildup >= FEAR
            && (fear.buildup > TERROR
                || spt_prob(rng, 1.0 + scaling(fear.buildup, 4.0, FEAR, TERROR), secs))
        {
            status_effects::set_timed(world, mob, Timed::Stutter, 10.0, true);
        }
        heart_problems(world, mob, &mut fear, now, secs, rng);
        if fear.monophobia {
            vomiting(world, mob, &fear, secs, rng);
        }
    }
    let amount = panic(world, mob, &mut fear, now, unconscious, secs, rng);
    adjustment += add(&mut fear, amount);
    if !fear.phobias.is_empty() && !unconscious {
        startle(world, mob, &mut fear, now, rng);
    }
    if !(adjustment > 0.0 || fear.buildup > fear.last_tick) && fear.buildup > 0.0 {
        fear.buildup = (fear.buildup - PASSIVE_DECREASE / modifier * secs).max(0.0);
    }
    fear.last_tick = fear.buildup;
    // The terrified source removed itself and nothing else feeds the fear.
    if !fear.terrified && fear.phobias.is_empty() && !fear.monophobia {
        if let Ok(mut entity) = world.get_entity_mut(mob) {
            entity.remove::<Fearful>();
        }
        return;
    }
    if world.get::<Fearful>(mob) != Some(&fear)
        && let Ok(mut entity) = world.get_entity_mut(mob)
    {
        entity.insert(fear);
    }
}

/// The tiles BYOND `view(range)` sees from `pos`.
fn seen_tiles(world: &World, pos: TilePos, range: u32) -> HashSet<TilePos> {
    let Some(index) = world.get_resource::<SpatialIndex>() else {
        return HashSet::new();
    };
    let opaque = |p: TilePos| {
        index
            .on_tile(p)
            .iter()
            .any(|&e| world.get::<Opaque>(e).is_some())
    };
    crate::sound::hearing_view(pos, range as f32, opaque, |p| index.in_bounds(p))
}

/// The entities on the tiles `mob` sees within `range` (`view(range, owner)`), without itself.
pub(crate) fn in_view(world: &World, mob: Entity, range: u32) -> Vec<Entity> {
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return Vec::new();
    };
    let seen = seen_tiles(world, pos, range);
    let Some(index) = world.get_resource::<SpatialIndex>() else {
        return Vec::new();
    };
    let mut out: Vec<Entity> = index
        .in_radius(pos, range)
        .filter(|&e| e != mob && world.get::<TilePos>(e).is_some_and(|p| seen.contains(p)))
        .collect();
    out.sort_unstable();
    out
}

/// A path list entry matches `path` (a leading `=`: that type only).
fn listed(list: &[&str], path: &str) -> bool {
    list.iter().any(|entry| match entry.strip_prefix('=') {
        Some(exact) => exact == path,
        None => is_under(path, entry),
    })
}

/// The DM type of a living mob, where one is known.
fn mob_path(world: &World, mob: Entity) -> Option<String> {
    use crate::bots::BotKind;
    if let Some(basic) = world.get::<crate::mobs::BasicMob>(mob) {
        return Some(basic.def.path.clone());
    }
    if let Some(bot) = world.get::<crate::bots::Bot>(mob) {
        let sub = match bot.kind {
            BotKind::Clean => "/cleanbot",
            BotKind::Med => "/medbot",
            BotKind::Sec => "/secbot",
            BotKind::Ed209 => "/secbot/ed209",
            BotKind::Honk => "/secbot/honkbot",
            BotKind::Grievous => "/secbot/grievous",
            BotKind::Fire => "/firebot",
            BotKind::Mule => "/mulebot",
            BotKind::Hygiene => "/hygienebot",
            BotKind::Ded => "/dedbot",
            BotKind::Other => "",
        };
        return Some(format!("/mob/living/basic/bot{sub}"));
    }
    if world.get::<crate::silicon::cyborg::Cyborg>(mob).is_some() {
        return Some("/mob/living/silicon/robot".to_owned());
    }
    if world.get::<crate::silicon::ai::AiCore>(mob).is_some() {
        return Some("/mob/living/silicon/ai".to_owned());
    }
    None
}

/// `is_scary_item` of an object, turf or item in view (worn ones through [`is_scary_mob`]).
fn is_scary_thing(world: &World, def: &PhobiaDef, e: Entity) -> bool {
    let Some(path) = crate::construction::path_of(world, e) else {
        return false;
    };
    if path.starts_with("/turf") {
        listed(def.turfs, &path)
    } else {
        listed(def.objs, &path)
    }
}

/// `is_scary_mob`: its type, a human's species (face shown) or a visible worn item.
fn is_scary_mob(world: &World, def: &PhobiaDef, mob: Entity) -> bool {
    if mob_path(world, mob).is_some_and(|p| listed(def.mobs, &p)) {
        return true;
    }
    if world.get::<crate::human::HumanLook>(mob).is_none() {
        return false;
    }
    if !def.species.is_empty() && listed(def.species, crate::species::def_of(world, mob).path) {
        return true;
    }
    if def.objs.is_empty() {
        return false;
    }
    let Some(inventory) = world.get::<Inventory>(mob) else {
        return false;
    };
    // `get_visible_items`: not the pockets.
    let pockets = inventory.pockets;
    inventory
        .equipped()
        .filter(|e| !pockets.contains(&Some(*e)))
        .any(|item| crate::construction::path_of(world, item).is_some_and(|p| listed(def.objs, &p)))
}

/// `/datum/terror_handler/phobia_source/tick` of `fear.phobias[i]`.
fn phobia_tick(
    world: &mut World,
    mob: Entity,
    fear: &mut Fearful,
    i: usize,
    now: f64,
    unconscious: bool,
) -> f32 {
    if unconscious {
        return 0.0;
    }
    let source = &mut fear.phobias[i];
    if now < source.scare_until {
        return 0.01;
    }
    if now < source.check_at || crate::vision::is_blind(world, mob) {
        return 0.0;
    }
    source.check_at = now + PHOBIA_CHECK_DELAY;
    let def = &PHOBIAS[source.phobia];
    let seen = in_view(world, mob, VIEW);
    // Objects, then turfs, then mobs, as tg.
    let is_turf = |e: Entity| world.get::<ss13_mapload::Turf>(e).is_some();
    let is_mob = |e: Entity| world.get::<Health>(e).is_some() && world.get::<Stat>(e).is_some();
    let scary = seen
        .iter()
        .copied()
        .find(|&e| !is_mob(e) && !is_turf(e) && is_scary_thing(world, def, e))
        .or_else(|| {
            seen.iter()
                .copied()
                .find(|&e| !def.turfs.is_empty() && is_turf(e) && is_scary_thing(world, def, e))
        })
        .or_else(|| {
            seen.iter()
                .copied()
                .find(|&e| is_mob(e) && is_scary_mob(world, def, e))
        });
    let Some(thing) = scary else {
        return 0.0;
    };
    let name = super::name_of(world, thing);
    freak_out(world, mob, source, Some(name.as_str()), false);
    PHOBIA_FREAKOUT
}

/// `freak_out(reason)`: a seen thing (`word` false) or a heard word.
fn freak_out(
    world: &mut World,
    mob: Entity,
    source: &mut PhobiaSource,
    reason: Option<&str>,
    word: bool,
) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    source.scare_until = now + SCARE_CD;
    let mut rng = super::fork_rng(world);
    let message = [
        "spooks you to the bone",
        "shakes you up",
        "terrifies you",
        "sends you into a panic",
        "sends chills down your spine",
    ][rng.range(0, 4) as usize];
    if word {
        say_to(
            world,
            mob,
            Span::Danger,
            format!("Hearing {} {message}!", reason.unwrap_or_default()),
        );
        crate::mood::add_event(world, mob, "phobia_minor", &STARTLED_MOOD);
        return;
    }
    let mood = if PHOBIAS[source.phobia].key == "skeletons" {
        &SPOOKED_MOOD
    } else {
        &PHOBIA_MOOD
    };
    crate::mood::add_event(world, mob, "phobia", mood);
    let text = match reason {
        Some(name) => format!("Seeing {name} {message}!"),
        None => format!("Something {message}!"),
    };
    say_to(world, mob, Span::Danger, text);
}

/// `/datum/terror_handler/simple_source/monophobia`: alone (no player's mob or pet in view, 1
/// tile when blind), 2.5 a second up to the passive maximum.
fn monophobia_tick(
    world: &mut World,
    mob: Entity,
    fear: &mut Fearful,
    now: f64,
    unconscious: bool,
    secs: f32,
    rng: &mut CombatRng,
) -> f32 {
    let mut per_second = 0.0;
    if !unconscious {
        let radius = if crate::vision::is_blind(world, mob) {
            1
        } else {
            VIEW
        };
        let company = in_view(world, mob, radius).into_iter().any(|e| {
            world.get::<Health>(e).is_some()
                && (world.get::<PlayerMob>(e).is_some()
                    || mob_path(world, e).is_some_and(|p| is_under(&p, "/mob/living/basic/pet")))
        });
        if !company {
            if now >= fear.mono_message_at && spt_prob(rng, 10.0, secs) {
                say_to(world, mob, Span::Warning, "You feel terribly lonely...");
                fear.mono_message_at = now + MESSAGE_CD;
            }
            per_second = 2.5;
        }
    }
    if per_second > 0.0 && fear.buildup < PASSIVE_MAXIMUM {
        (per_second * secs).min(PASSIVE_MAXIMUM - fear.buildup)
    } else {
        0.0
    }
}

/// `/datum/mood_event/nyctophobia` (BandaStation's text).
const NYCTOPHOBIA_MOOD: MoodEvent = MoodEvent {
    id: "nyctophobia",
    description: "Здесь слишком темно...",
    mood_change: -3.0,
    timeout: 0.0,
    hidden: false,
};

/// `AddComponentFrom("terrified", /datum/component/fearful, ..., amount)`: the terrified source
/// on the mob's fear, `amount` more terror.
pub fn add_terrified(world: &mut World, mob: Entity, amount: f32) {
    let Ok(mut entity) = world.get_entity_mut(mob) else {
        return;
    };
    let mut fear = entity.get::<Fearful>().cloned().unwrap_or_default();
    fear.terrified = true;
    fear.buildup = (fear.buildup + amount).clamp(0.0, MAXIMUM);
    entity.insert(fear);
}

/// `nyctophobia/terrified/tick`: the simple source of the dark (5 a second up to the passive
/// maximum) three times over; at no terror and no buildup the source goes.
fn terrified_tick(
    world: &mut World,
    mob: Entity,
    fear: &mut Fearful,
    unconscious: bool,
    secs: f32,
) -> f32 {
    let buildup = fear.buildup;
    let per_second = if !unconscious && crate::afflictions::dark_around(world, mob) {
        5.0
    } else {
        0.0
    };
    let mut amount = 0.0;
    if per_second > 0.0 && buildup < PASSIVE_MAXIMUM {
        if !fear.terrified_active {
            fear.terrified_active = true;
            crate::mood::add_event(world, mob, "nyctophobia", &NYCTOPHOBIA_MOOD);
        }
        amount = (per_second * secs).min(PASSIVE_MAXIMUM - buildup);
    } else if fear.terrified_active {
        fear.terrified_active = false;
        crate::mood::clear_event(world, mob, "nyctophobia");
    }
    let amount = amount * 3.0;
    if buildup <= 0.0 && amount <= 0.0 {
        fear.terrified = false;
        if fear.terrified_active {
            fear.terrified_active = false;
            crate::mood::clear_event(world, mob, "nyctophobia");
        }
    }
    amount
}

/// `/datum/terror_handler/jittering`.
fn jittering(
    world: &mut World,
    mob: Entity,
    fear: &mut Fearful,
    now: f64,
    secs: f32,
    rng: &mut CombatRng,
) {
    if fear.buildup < FEAR {
        return;
    }
    if fear.buildup > TERROR {
        status_effects::adjust_timed(world, mob, Timed::Dizziness, 10.0 * secs, Some(10.0));
        status_effects::adjust_timed(world, mob, Timed::Jitter, 10.0 * secs, Some(10.0));
        return;
    }
    if !spt_prob(rng, 1.0 + scaling(fear.buildup, 4.0, FEAR, TERROR), secs) {
        return;
    }
    if now >= fear.jitter_message_at && status_effects::timed(world, mob, Timed::Jitter) <= 0.0 {
        say_to(world, mob, Span::Warning, "You can't stop shaking...");
        fear.jitter_message_at = now + MESSAGE_CD;
    }
    status_effects::set_timed(world, mob, Timed::Jitter, 20.0, true);
    status_effects::set_timed(world, mob, Timed::Dizziness, 20.0, true);
}

/// `/datum/terror_handler/heart_problems`.
fn heart_problems(
    world: &mut World,
    mob: Entity,
    fear: &mut Fearful,
    now: f64,
    secs: f32,
    rng: &mut CombatRng,
) {
    if fear.buildup < FEAR || !spt_prob(rng, 1.0 + scaling(fear.buildup, 4.0, FEAR, PANIC), secs) {
        return;
    }
    if fear.buildup < HEART_ATTACK || !rng.chance(15.0) {
        super::diseases::damage(world, mob, DamageType::Oxy, 8.0);
        if now < fear.heart_message_at {
            return;
        }
        fear.heart_message_at = now + MESSAGE_CD;
        // tg compares with `TERROR_BUILDUP_FEAR`, which never holds past the check above: the
        // milder line is meant for below terror.
        if fear.buildup < TERROR {
            say_to(world, mob, Span::Warning, "Your heart skips a beat.");
        } else {
            say_to(
                world,
                mob,
                Span::Danger,
                "You feel your heart lurching in your chest...",
            );
        }
        return;
    }
    let name = super::name_of(world, mob);
    super::visible(
        world,
        mob,
        "The shadows begin to creep up from the corners of your vision, and then there is \
         nothing...",
        &format!("{name} clutches their chest for a moment, then collapses to the floor."),
        Span::Warning,
    );
    crate::events::heart_attack::apply(world, mob);
    status_effects::apply(world, mob, Incap::Unconscious, 20.0);
}

/// `/datum/terror_handler/vomiting`.
fn vomiting(world: &mut World, mob: Entity, fear: &Fearful, secs: f32, rng: &mut CombatRng) {
    if fear.buildup < TERROR {
        return;
    }
    let chance = if fear.buildup >= PANIC { 3.0 } else { 1.0 };
    if spt_prob(rng, chance, secs) {
        say_to(world, mob, Span::Warning, "You feel sick...");
        let blood = fear.buildup >= PASSIVE_MAXIMUM;
        super::diseases::vomit(world, mob, 10.0, true, blood, 1);
    }
}

/// `/datum/terror_handler/panic`: what it adds to the terror.
fn panic(
    world: &mut World,
    mob: Entity,
    fear: &mut Fearful,
    now: f64,
    unconscious: bool,
    secs: f32,
    rng: &mut CombatRng,
) -> f32 {
    if fear.attack_until.is_some_and(|until| now >= until) {
        fear.attack_until = None;
    }
    if unconscious {
        fear.attack_until = None;
        fear.panic_active = false;
        return 0.0;
    }
    if fear.buildup < PANIC {
        if fear.attack_until.is_some() {
            return PANIC - fear.buildup;
        }
        fear.panic_active = false;
        return 0.0;
    }
    if !fear.panic_active {
        fear.panic_active = true;
        say_to(world, mob, Span::Danger, "You feel your heart racing!");
    }
    if spt_prob(rng, 5.0, secs) {
        crate::vision::set_eye_blur_if_lower(world, mob, 10.0);
    }
    if fear.attack_until.is_some() {
        return 0.0;
    }
    if !spt_prob(rng, 2.0 + scaling(fear.buildup, 3.0, PANIC, MAXIMUM), secs) {
        return 0.0;
    }
    // `panic_attack`.
    fear.attack_until = Some(now + f64::from(rng.range(30, 50)) / 10.0);
    super::diseases::emote(world, mob, "gasp");
    status_effects::knockdown(world, mob, 0.5, 0.0);
    let name = super::name_of(world, mob);
    super::visible(
        world,
        mob,
        "Your heart lurches in your chest. You can't take much more of this!",
        &format!("{name} drops to the floor for a moment, clutching their chest."),
        Span::Warning,
    );
    PANIC_ATTACK
}

/// `/datum/terror_handler/startle`: only on a tick that added enough fear at once.
fn startle(world: &mut World, mob: Entity, fear: &mut Fearful, now: f64, rng: &mut CombatRng) {
    let gained = fear.buildup - fear.last_tick;
    if now < fear.startle_at || fear.buildup < FEAR || gained < STARTLE_DIFFERENCE {
        return;
    }
    if !rng.chance(15.0 * gained / STARTLE_DIFFERENCE * fear.buildup / FEAR) {
        return;
    }
    fear.startle_at = now + STARTLE_CD;
    match rng.range(1, 3) {
        1 => {
            say_to(world, mob, Span::Warning, "You are startled!");
            status_effects::apply(world, mob, Incap::Immobilize, 0.1 * fear.buildup / FEAR);
        }
        2 => {
            super::diseases::emote(world, mob, "scream");
            if !rng.chance(15.0 * fear.buildup / FEAR) {
                return;
            }
            let Some(item) = world.get::<Inventory>(mob).and_then(Inventory::active_item) else {
                return;
            };
            let Some(&pos) = world.get::<TilePos>(mob) else {
                return;
            };
            if crate::inventory::no_drop(world, item) {
                return;
            }
            crate::inventory::put_down(world, item, pos);
            let (name, what) = (super::name_of(world, mob), super::name_of(world, item));
            super::visible(
                world,
                mob,
                &format!("You drop {what}!"),
                &format!("{name} drops {what}!"),
                Span::Danger,
            );
        }
        _ => say_to(world, mob, Span::Warning, "You lose your balance!"),
    }
}

/// `comfort_owner`: a hug from something the mob fears scares it, any other one calms it.
pub fn on_hug(world: &mut World, hugger: Entity, target: Entity) {
    if hugger == target {
        return;
    }
    let Some(mut fear) = world.get::<Fearful>(target).cloned() else {
        return;
    };
    let scary = fear
        .phobias
        .iter()
        .any(|s| is_scary_mob(world, &PHOBIAS[s.phobia], hugger));
    let name = super::name_of(world, target);
    let hugger_name = super::name_of(world, hugger);
    if scary {
        fear.buildup = (fear.buildup + HUG).min(MAXIMUM);
        super::visible(
            world,
            target,
            &format!("You recoil in terror as {hugger_name} attempts to hug you!"),
            &format!("{name} recoils in fear as {hugger_name} attempts to hug them!"),
            Span::Warning,
        );
    } else {
        if fear.buildup >= TERROR {
            super::visible(
                world,
                target,
                &format!(
                    "You feel yourself calm down as {hugger_name} gives you a reassuring hug."
                ),
                &format!("{name} seems to relax as {hugger_name} gives them a comforting hug."),
                Span::Notice,
            );
        }
        fear.buildup = (fear.buildup - HUG).max(0.0);
    }
    world.entity_mut(target).insert(fear);
}

/// The scary word of `words` in `text`, as `construct_phobia_regex` finds it: the first place a
/// listed word starts at a word boundary (or the text's start), the first word listed there;
/// case-insensitive.
pub fn find_word(words: &[&str], text: &str) -> Option<String> {
    let text = text.to_lowercase();
    let word_char = |c: char| c.is_alphanumeric() || c == '_';
    let mut prev: Option<char> = None;
    for (at, c) in text.char_indices() {
        let rest = &text[at..];
        for word in words {
            let word = word.to_lowercase();
            let Some(first) = word.chars().next() else {
                continue;
            };
            // `\b` before the word, or `\A`.
            let boundary = prev.is_none_or(|p| word_char(p) != word_char(first));
            if boundary && rest.starts_with(&word) {
                return Some(word);
            }
        }
        prev = Some(c);
    }
    None
}

/// `handle_hearing` and `handle_speech` of every phobia: a scary word heard (understood, not
/// deaf, not one's own) freaks the hearer out; said, it makes the speaker stutter half the time.
pub(crate) fn hear_words(world: &mut World, mut cursor: Local<MessageCursor<Spoke>>) {
    let Some(messages) = world.get_resource::<Messages<Spoke>>() else {
        return;
    };
    let spoken: Vec<Spoke> = cursor.read(messages).cloned().collect();
    if spoken.is_empty() {
        return;
    }
    let fearful: Vec<(Entity, TilePos)> = world
        .query::<(Entity, &Fearful, &TilePos)>()
        .iter(world)
        .filter(|(_, f, _)| !f.phobias.is_empty())
        .map(|(e, _, &p)| (e, p))
        .collect();
    if fearful.is_empty() {
        return;
    }
    for said in spoken {
        let range = if said.heard_whisper { 1 } else { VIEW };
        let mut heard: Option<HashSet<TilePos>> = None;
        for &(mob, pos) in &fearful {
            if status_effects::knocked_out(world, mob) {
                continue;
            }
            if mob == said.speaker {
                own_speech(world, mob, &said.text);
                continue;
            }
            if pos.chebyshev(said.pos).is_none_or(|d| d > range) {
                continue;
            }
            let tiles = heard.get_or_insert_with(|| seen_tiles(world, said.pos, range));
            if !tiles.contains(&pos) || !crate::sound::can_hear(world, mob) {
                continue;
            }
            let understood = said.language.is_some_and(|language| {
                ss13_chat::language::understanding(
                    world.get::<ss13_chat::language::LanguageHolder>(mob),
                    false,
                    language,
                ) >= ss13_chat::language::FULL
            });
            if understood {
                heard_word(world, mob, &said.text);
            }
        }
    }
}

fn heard_word(world: &mut World, mob: Entity, text: &str) {
    let Some(mut fear) = world.get::<Fearful>(mob).cloned() else {
        return;
    };
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut changed = false;
    for i in 0..fear.phobias.len() {
        if now < fear.phobias[i].scare_until {
            continue;
        }
        let Some(word) = find_word(PHOBIAS[fear.phobias[i].phobia].words, text) else {
            continue;
        };
        let mut source = fear.phobias[i].clone();
        freak_out(world, mob, &mut source, Some(word.as_str()), true);
        fear.phobias[i] = source;
        if fear.buildup < PASSIVE_MAXIMUM {
            fear.buildup = (fear.buildup + PHOBIA_WORD).min(PASSIVE_MAXIMUM);
        }
        changed = true;
    }
    if changed {
        world.entity_mut(mob).insert(fear);
    }
}

fn own_speech(world: &mut World, mob: Entity, text: &str) {
    let Some(fear) = world.get::<Fearful>(mob) else {
        return;
    };
    let words: Vec<String> = fear
        .phobias
        .iter()
        .filter_map(|s| find_word(PHOBIAS[s.phobia].words, text))
        .collect();
    let mut rng = super::fork_rng(world);
    for word in words {
        let stutter = rng.chance(50.0);
        let whisper = rng.chance(30.0);
        if !stutter && !whisper {
            continue;
        }
        if stutter {
            status_effects::set_timed(world, mob, Timed::Stutter, 4.0, true);
        }
        say_to(
            world,
            mob,
            Span::Warning,
            format!("You struggle to say the word \"{word}\"!"),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scary_words_start_at_a_word_boundary() {
        let words = ["spider", "web", "~"];
        assert_eq!(
            find_word(&words, "Look at the SPIDERS!").as_deref(),
            Some("spider")
        );
        assert_eq!(find_word(&words, "cobweb"), None);
        assert_eq!(find_word(&words, "a web of lies").as_deref(), Some("web"));
        assert_eq!(find_word(&words, "nya~").as_deref(), Some("~"));
        assert_eq!(find_word(&words, "nothing here"), None);
    }

    #[test]
    fn phobia_lists_match_types() {
        let greytide = &PHOBIAS[phobia_index("greytide").unwrap()];
        // A subtype matches; `only_root_path` entries match that type only.
        assert!(listed(greytide.objs, "/obj/item/spear/bonespear"));
        assert!(listed(greytide.objs, "/obj/item/clothing/mask/gas"));
        assert!(!listed(
            greytide.objs,
            "/obj/item/clothing/mask/gas/clown_hat"
        ));
        assert!(
            super::super::phobia_data::RANDOM
                .iter()
                .all(|k| phobia_index(k).is_some())
        );
        assert!(PHOBIAS.iter().all(|p| !p.words.is_empty()));
    }

    #[test]
    fn terror_scales_and_ebbs() {
        assert_eq!(scaling(150.0, 4.0, FEAR, TERROR), 0.0);
        assert_eq!(scaling(225.0, 4.0, FEAR, TERROR), 2.0);
        assert_eq!(scaling(900.0, 4.0, FEAR, TERROR), 4.0);
        let mut world = World::new();
        world.init_resource::<Time>();
        let mob = world.spawn(Stat::Conscious).id();
        set_sources(&mut world, mob, &[], true);
        world.get_mut::<Fearful>(mob).unwrap().buildup = 100.0;
        // Alone on no map: monophobia adds 2.5 a second (nobody in view).
        let mut rng = CombatRng(5);
        tick(&mut world, mob, 2.0, &mut rng);
        assert_eq!(world.get::<Fearful>(mob).unwrap().buildup, 105.0);
        // Without a source left the component goes.
        set_sources(&mut world, mob, &[], false);
        assert!(world.get::<Fearful>(mob).is_none());
    }
}
