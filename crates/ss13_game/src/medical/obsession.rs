//! Obsession (server), after tg `code/datums/brain_damage/creepy_trauma.dm`
//! (`/datum/brain_trauma/special/obsessed`) with BandaStation's edits (an obsession given in
//! `New`, `set_obsession` resetting the progress, characters leaving the round excluded, no
//! speech debuff).
//!
//! The trauma ([`super::traumas::Kind::Obsessed`], lobotomy-deep, never random) picks its
//! obsession on gain ([`find_obsession`]: a living human with a player, not the owner) or takes
//! the one it was given; none: no trauma. Every `Life()` ([`on_life`]) it tracks whether the
//! owner sees the obsession within 7 tiles: near — the hidden `creeping` mood (+18); away —
//! `notcreeping` (−6), after 3 minutes `notcreepingsevere` (−30); the obsession dead in view —
//! `creeping/dead` (+8) for good. The cure clears the mood.
// shortcut: no `/datum/antagonist/obsessed` (objectives, greeting, `former_obsessed`: the
// antagonists group), so no objective timers or hug counts; no stronger grabs and stamina
// resistance near the obsession (`is_defensive`), no eye-contact stare, no
// `desensitized`/obsession stutter status effects, no job-trait pools (mime fan, clown enjoyer,
// spiritual), no cryo replacement (no cryo storage), the severe mood line does not shout the
// name.

use bevy::prelude::*;

use super::traumas::Trauma;
use crate::health::Stat;
use crate::mood::MoodEvent;
use crate::server::PlayerMob;

/// The obsession and the time counters of a trauma.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Obsession {
    pub target: Entity,
    pub viewing: bool,
    /// Seconds: `total_time_creeping`, `time_spent_away`, `time_spend_creeping`.
    pub total_creeping: f32,
    pub away: f32,
    pub creeping: f32,
    pub witnessed_death: bool,
}

impl Obsession {
    /// `set_obsession`: a new target, fresh progress.
    pub fn new(target: Entity) -> Self {
        Self {
            target,
            viewing: false,
            total_creeping: 0.0,
            away: 0.0,
            creeping: 0.0,
            witnessed_death: false,
        }
    }
}

/// The mood category.
pub const CATEGORY: &str = "creeping";
/// `IN_GIVEN_RANGE(owner, obsession, 7)`.
const VIEW_RANGE: u32 = 7;
/// `time_spent_away > 3 MINUTES`.
const SEVERE_AFTER: f32 = 180.0;

const fn hidden(
    id: &'static str,
    description: &'static str,
    change: f32,
    timeout: f32,
) -> MoodEvent {
    MoodEvent {
        id,
        description,
        mood_change: change,
        timeout,
        hidden: true,
    }
}

/// `/datum/mood_event/creeping`, `creeping/dead`, `notcreeping`, `notcreepingsevere`.
pub const CREEPING: MoodEvent = hidden(
    "creeping",
    "Голоса отпустили свои крючки с моего разума! Я снова свободен!",
    18.0,
    3.0,
);
pub const CREEPING_DEAD: MoodEvent = hidden(
    "creeping_dead",
    "Голоса отпустили свои крючки с моего разума! Я снова свободен!",
    8.0,
    0.0,
);
pub const NOT_CREEPING: MoodEvent = hidden(
    "notcreeping",
    "Голоса недовольны и болезненно заставляют мои мысли вернуться к выполнению задачи.",
    -6.0,
    3.0,
);
pub const NOT_CREEPING_SEVERE: MoodEvent = MoodEvent {
    hidden: false,
    ..hidden(
        "notcreepingsevere",
        "ИМ НУЖНАААА ОДЕРЖИМООООСТЬ!!",
        -30.0,
        3.0,
    )
};

/// `find_obsession(blacklist)`: a random living human with a player who is not `owner` nor in
/// `blacklist` (BandaStation: those leaving the round).
pub fn find_obsession(
    world: &mut World,
    owner: Entity,
    blacklist: &[Entity],
    rng: &mut crate::combat::CombatRng,
) -> Option<Entity> {
    let mut pool: Vec<Entity> = world
        .query_filtered::<(Entity, &Stat), (With<PlayerMob>, With<crate::human::HumanLook>)>()
        .iter(world)
        .filter(|&(e, &stat)| stat != Stat::Dead && e != owner && !blacklist.contains(&e))
        .map(|(e, _)| e)
        .collect();
    pool.sort_unstable();
    if pool.is_empty() {
        return None;
    }
    Some(pool[rng.range(0, pool.len() as u32 - 1) as usize])
}

/// `/datum/brain_trauma/special/obsessed/on_life`.
pub fn on_life(world: &mut World, mob: Entity, trauma: &mut Trauma, secs: f32) {
    let Some(state) = trauma.obsession.as_mut() else {
        return;
    };
    if world.get_entity(state.target).is_err() {
        // `obession_deleted`.
        trauma.obsession = None;
        return;
    }
    state.viewing = super::fear::in_view(world, mob, VIEW_RANGE).contains(&state.target);
    if !state.viewing {
        if state.witnessed_death {
            return;
        }
        state.away += secs;
        state.creeping = 0.0;
        let event = if state.away > SEVERE_AFTER {
            &NOT_CREEPING_SEVERE
        } else {
            &NOT_CREEPING
        };
        crate::mood::add_event(world, mob, CATEGORY, event);
        return;
    }
    if world.get::<Stat>(state.target) == Some(&Stat::Dead) {
        if !state.witnessed_death {
            state.witnessed_death = true;
            crate::mood::add_event(world, mob, CATEGORY, &CREEPING_DEAD);
        }
        return;
    }
    state.witnessed_death = false;
    crate::mood::add_event(world, mob, CATEGORY, &CREEPING);
    state.total_creeping += secs;
    state.creeping += secs;
    state.away = 0.0;
}
