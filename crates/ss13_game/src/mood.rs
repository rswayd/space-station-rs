//! Mood and sanity (server), after tg `code/datums/mood.dm` (`/datum/mood`), the mood events of
//! `code/datums/mood_events` and `SSmood` (`code/controllers/subsystem/moods.dm`).
//!
//! - Every human (tg `/mob/living/carbon/human/setup_mood`) gets a [`Mood`]: mood events by
//!   category ([`Mood::add_event`], [`Mood::clear_event`], tg `add_mood_event`/
//!   `clear_mood_event`), each with an optional timeout. Their sum (times the modifiers) is the
//!   mood, its band the mood level 1..=9 (`update_mood`).
//! - Every `SSmood` fire (1 s) a living mob's sanity drifts with the mood level (`process`,
//!   `set_sanity`); the sanity level slows movement (`/datum/movespeed_modifier/sanity`) and
//!   actions (`/datum/actionspeed_modifier/*_sanity`, `crate::tools::do_after`) and scales
//!   hunger (`crate::hunger`).
//! - The common need events follow the mob's state each fire: nutrition (`update_nutrition_
//!   moodlets`), drunkenness (`/datum/status_effect/inebriated/drunk`), handcuffs, burning,
//!   suffocation and body temperature. Seeing a human die (`send_death_moodlets`) and hugs
//!   (`help_shake_act`) add theirs when they happen.
//! - Crazy and insane sanity bring hallucinations (`crate::hallucination::SanityHallucinations`).
//! - The face on the owner's HUD ([`MoodHudData`], headless tgui [`MoodHudUi`]) shows the level
//!   tinted by sanity; clicking it prints the mood (`print_mood`).
// shortcut: no crit threshold change from insanity (crit thresholds are constants in
// `crate::health`), no area beauty or area
// mood bonuses, no special screen icons, no personalities (their mood multipliers), no full-heal
// reset (no aheal yet), gibbed/dusted deaths count as plain ones, conditional death moodlets other
// than the base one (cult, revolution, pets) are not ported. The food, drug, morbid and most
// generic events wait for the mechanics that raise them.

use std::collections::HashSet;

use bevy::prelude::*;
use ss13_net::chat::server::SendChat;
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};
use ss13_world::SpatialIndex;

use crate::breath::{BODYTEMP_COLD_DAMAGE_LIMIT, BODYTEMP_HEAT_DAMAGE_LIMIT, Respiration};
use crate::chem::MobReagents;
use crate::fire::FireStacks;
use crate::health::{DeathComa, HealthSystems, Stat};
use crate::hud::{MOOD_CLICK, MoodHudData, MoodHudUi};
use crate::human::HumanLook;
use crate::hunger::{
    Fat, Hunger, NUTRITION_LEVEL_FED, NUTRITION_LEVEL_FULL, NUTRITION_LEVEL_HUNGRY,
    NUTRITION_LEVEL_STARVING, NUTRITION_LEVEL_VERY_HUNGRY, NUTRITION_LEVEL_WELL_FED,
};
use crate::quirks::{Quirk, Quirks};
use crate::restraints::Handcuffed;
use crate::server::PlayerMob;

/// `MOOD_HAPPY4` .. `MOOD_SAD4`: the bands of the mood sum.
pub const MOOD_HAPPY4: f32 = 15.0;
pub const MOOD_HAPPY3: f32 = 10.0;
pub const MOOD_HAPPY2: f32 = 6.0;
pub const MOOD_HAPPY1: f32 = 2.0;
pub const MOOD_NEUTRAL: f32 = 0.0;
pub const MOOD_SAD1: f32 = -3.0;
pub const MOOD_SAD2: f32 = -7.0;
pub const MOOD_SAD3: f32 = -15.0;
pub const MOOD_SAD4: f32 = -20.0;
/// `MOOD_LEVEL_NEUTRAL`.
pub const MOOD_LEVEL_NEUTRAL: u8 = 5;

pub const SANITY_MAXIMUM: f32 = 150.0;
pub const SANITY_GREAT: f32 = 125.0;
pub const SANITY_NEUTRAL: f32 = 100.0;
pub const SANITY_DISTURBED: f32 = 75.0;
pub const SANITY_UNSTABLE: f32 = 50.0;
pub const SANITY_CRAZY: f32 = 25.0;
pub const SANITY_INSANE: f32 = 0.0;

pub const SANITY_LEVEL_GREAT: u8 = 1;
pub const SANITY_LEVEL_NEUTRAL: u8 = 2;
pub const SANITY_LEVEL_DISTURBED: u8 = 3;
pub const SANITY_LEVEL_UNSTABLE: u8 = 4;
pub const SANITY_LEVEL_CRAZY: u8 = 5;
pub const SANITY_LEVEL_INSANE: u8 = 6;

/// `SSmood` `wait`, seconds.
pub const MOOD_WAIT_SECS: f64 = 1.0;
/// tg `viewers(src)` for the death moodlets: `world.view` (7).
const DEATH_VIEW_RANGE: u32 = 7;

/// A mood event type (tg `/datum/mood_event` subtype): its `description`, `mood_change`,
/// `timeout` (seconds, 0: until cleared) and `hidden`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MoodEvent {
    pub id: &'static str,
    pub description: &'static str,
    pub mood_change: f32,
    pub timeout: f32,
    pub hidden: bool,
}

const fn ev(
    id: &'static str,
    description: &'static str,
    mood_change: f32,
    timeout: f32,
) -> MoodEvent {
    MoodEvent {
        id,
        description,
        mood_change,
        timeout,
        hidden: false,
    }
}

/// The ported mood event types (BandaStation texts).
pub mod events {
    use super::{MoodEvent, ev};

    // needs_events.dm
    pub const FAT: MoodEvent = ev("fat", "Я ожирел...", -6.0, 0.0);
    pub const TOO_WELLFED: MoodEvent = ev("too_wellfed", "Кажется, я переел.", 0.0, 0.0);
    pub const WELLFED: MoodEvent = ev("wellfed", "Ух, как наелся!", 8.0, 0.0);
    pub const FED: MoodEvent = ev("fed", "Я недавно поел.", 5.0, 0.0);
    pub const HUNGRY: MoodEvent = ev("hungry", "Я немного проголодался.", -3.0, 0.0);
    pub const HUNGRY_VERY: MoodEvent = ev("hungry_very", "Я голодаю!", -6.0, 0.0);
    pub const STARVING: MoodEvent = ev("starving", "Я умираю с голода!", -10.0, 0.0);
    // generic_negative_events.dm
    pub const HANDCUFFED: MoodEvent =
        ev("handcuffed", "Похоже, я получу за свои выходки.", -1.0, 0.0);
    pub const ON_FIRE: MoodEvent = ev("on_fire", "Я ГОРЮ!!!", -12.0, 0.0);
    pub const SUFFOCATION: MoodEvent = ev("suffocation", "НЕ МОГУ... ДЫШАТЬ...", -12.0, 0.0);
    pub const COLD: MoodEvent = ev("cold", "Здесь слишком холодно.", -5.0, 0.0);
    pub const HOT: MoodEvent = ev("hot", "Становится слишком жарко.", -5.0, 0.0);
    pub const DEPRESSION: MoodEvent = ev(
        "depression",
        "Мне грустно без особой на то причины.",
        -12.0,
        120.0,
    );
    pub const BACK_PAIN: MoodEvent = ev(
        "back_pain",
        "Сумки никогда не сидят хорошо на моей спине, черт, как же больно!",
        -15.0,
        0.0,
    );
    // food_events.dm
    pub const GROSS_FOOD: MoodEvent = ev(
        "gross_food",
        "Мне очень не понравилась эта еда.",
        -2.0,
        240.0,
    );
    pub const DISGUSTING_FOOD: MoodEvent = ev(
        "disgusting_food",
        "Эта еда была отвратительна!",
        -6.0,
        240.0,
    );
    // generic_positive_events.dm
    pub const HUG: MoodEvent = ev("hug", "Люблю обнимашки.", 1.0, 120.0);
    pub const WARMHUG: MoodEvent = ev(
        "warmhug",
        "Теплые и уютные обнимашки самые лучшие!",
        1.0,
        120.0,
    );
    pub const BETTERHUG: MoodEvent = ev("betterhug", "Кто-то был очень добр ко мне.", 3.0, 240.0);
    pub const BESTHUG: MoodEvent = ev(
        "besthug",
        "Весело находиться рядом с кем-то, не могу нарадоваться!",
        5.0,
        240.0,
    );
    pub const JOLLY: MoodEvent = ev(
        "jolly",
        "Мне радостно без особой на то причины.",
        6.0,
        120.0,
    );
    /// `/datum/mood_event/conditional/see_death` (`mood_change` -8, 5 minutes); the description
    /// names the dead.
    pub const SEE_DEATH: MoodEvent = ev("see_death", "", -8.0, 300.0);
    /// `/datum/mood_event/drunk`: its value and description follow the drunkenness.
    pub const DRUNK: MoodEvent = ev("drunk", "", 3.0, 0.0);
}

/// One active mood event (tg `mood_events[category]`).
#[derive(Clone, Debug, PartialEq)]
pub struct ActiveMood {
    pub category: String,
    pub id: &'static str,
    pub description: String,
    pub mood_change: f32,
    /// After the length modifiers, seconds; 0: until cleared.
    pub timeout: f32,
    /// Game time it clears at.
    pub expires: Option<f64>,
    pub hidden: bool,
    /// `see_death`'s `be_refreshed`: another one makes it 1.5 times stronger.
    pub stacks: bool,
}

impl ActiveMood {
    pub fn new(category: &str, event: &MoodEvent) -> Self {
        Self {
            category: category.to_owned(),
            id: event.id,
            description: event.description.to_owned(),
            mood_change: event.mood_change,
            timeout: event.timeout,
            expires: None,
            hidden: event.hidden,
            stacks: false,
        }
    }
}

/// tg `/datum/mood` of a human.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Mood {
    /// In the order they were added (tg's assoc list).
    pub events: Vec<ActiveMood>,
    pub mood: f32,
    pub shown_mood: f32,
    pub sanity: f32,
    pub mood_level: u8,
    pub sanity_level: u8,
    pub mood_modifier: f32,
    pub positive_mood_modifier: f32,
    pub negative_mood_modifier: f32,
    pub positive_length_modifier: f32,
    pub negative_length_modifier: f32,
    /// tg `TRAIT_UNSTABLE` (the Unstable quirk): sanity never rises on its own.
    pub unstable: bool,
    /// tg applies the action speed modifier only from the first sanity change on
    /// (`set_actionspeed_effect` runs in `set_sanity`): a new mood has none.
    pub speed_applied: bool,
}

impl Default for Mood {
    fn default() -> Self {
        Self {
            events: Vec::new(),
            mood: 0.0,
            shown_mood: 0.0,
            sanity: SANITY_NEUTRAL,
            mood_level: MOOD_LEVEL_NEUTRAL,
            sanity_level: SANITY_LEVEL_NEUTRAL,
            mood_modifier: 1.0,
            positive_mood_modifier: 1.0,
            negative_mood_modifier: 1.0,
            positive_length_modifier: 1.0,
            negative_length_modifier: 1.0,
            unstable: false,
            speed_applied: false,
        }
    }
}

/// tg's `switch(mood)` of `update_mood` (DM ranges are inclusive, the first match wins).
pub fn mood_level(mood: f32) -> u8 {
    if mood <= MOOD_SAD4 {
        1
    } else if mood <= MOOD_SAD3 {
        2
    } else if mood <= MOOD_SAD2 {
        3
    } else if mood <= MOOD_SAD1 {
        4
    } else if mood <= MOOD_HAPPY1 {
        5
    } else if mood <= MOOD_HAPPY2 {
        6
    } else if mood <= MOOD_HAPPY3 {
        7
    } else if mood <= MOOD_HAPPY4 {
        8
    } else {
        9
    }
}

/// tg's `switch(sanity)` of `set_sanity`; `None` where no case matches (below 0, or between
/// 100 and 101), which keeps the level.
fn sanity_level_of(sanity: f32) -> Option<u8> {
    let within = |a: f32, b: f32| (a..=b).contains(&sanity);
    if within(SANITY_INSANE, SANITY_CRAZY) {
        Some(SANITY_LEVEL_INSANE)
    } else if within(SANITY_CRAZY, SANITY_UNSTABLE) {
        Some(SANITY_LEVEL_CRAZY)
    } else if within(SANITY_UNSTABLE, SANITY_DISTURBED) {
        Some(SANITY_LEVEL_UNSTABLE)
    } else if within(SANITY_DISTURBED, SANITY_NEUTRAL) {
        Some(SANITY_LEVEL_DISTURBED)
    } else if within(SANITY_NEUTRAL + 1.0, SANITY_GREAT + 1.0) {
        Some(SANITY_LEVEL_NEUTRAL)
    } else if sanity >= SANITY_GREAT + 1.0 {
        Some(SANITY_LEVEL_GREAT)
    } else {
        None
    }
}

impl Mood {
    pub fn get(&self, category: &str) -> Option<&ActiveMood> {
        self.events.iter().find(|e| e.category == category)
    }

    /// tg `add_mood_event(category, type)` at game time `now`.
    pub fn add_event(&mut self, now: f64, category: &str, event: &MoodEvent) {
        self.add_instance(now, ActiveMood::new(category, event));
    }

    /// tg `add_mood_event_instance`: the same type in the category refreshes its timer (and
    /// stacks, for `see_death`); another type is replaced.
    pub fn add_instance(&mut self, now: f64, mut new: ActiveMood) {
        if let Some(i) = self.events.iter().position(|e| e.category == new.category) {
            if self.events[i].id == new.id {
                let existing = &mut self.events[i];
                if existing.stacks {
                    existing.mood_change *= 1.5;
                }
                if existing.timeout > 0.0 {
                    existing.expires = Some(now + f64::from(existing.timeout));
                }
                self.update_mood();
                return;
            }
            self.events.remove(i);
        }
        // `on_add`.
        new.mood_change = new.mood_change.floor();
        let length = if new.mood_change > 0.0 {
            self.positive_length_modifier
        } else {
            self.negative_length_modifier
        };
        new.timeout *= length.max(0.1);
        new.expires = (new.timeout > 0.0).then(|| now + f64::from(new.timeout));
        self.events.push(new);
        self.update_mood();
    }

    /// tg `clear_mood_event`: true if there was one.
    pub fn clear_event(&mut self, category: &str) -> bool {
        let before = self.events.len();
        self.events.retain(|e| e.category != category);
        let cleared = self.events.len() != before;
        if cleared {
            self.update_mood();
        }
        cleared
    }

    /// The timers of timed events (tg `addtimer(clear_mood_event)`).
    pub fn expire(&mut self, now: f64) {
        let before = self.events.len();
        self.events.retain(|e| e.expires.is_none_or(|t| t > now));
        if self.events.len() != before {
            self.update_mood();
        }
    }

    /// tg `remove_temp_moods`.
    pub fn remove_temp_moods(&mut self) {
        self.events.retain(|e| e.timeout <= 0.0);
        self.update_mood();
    }

    /// tg `update_mood`.
    pub fn update_mood(&mut self) {
        let (mut mood, mut shown) = (0.0, 0.0);
        for event in &self.events {
            let modifier = if event.mood_change > 0.0 {
                self.positive_mood_modifier
            } else {
                self.negative_mood_modifier
            };
            let value = event.mood_change * modifier.max(0.0);
            mood += value;
            if !event.hidden {
                shown += value;
            }
        }
        self.mood = mood * self.mood_modifier.max(0.0);
        self.shown_mood = shown * self.mood_modifier.max(0.0);
        self.mood_level = mood_level(self.mood);
    }

    /// tg `set_sanity(amount, minimum, maximum, override)`: true if it changed.
    pub fn set_sanity(&mut self, mut amount: f32, minimum: f32, maximum: f32, force: bool) -> bool {
        // Out of the allowed range: back towards it by 0.7 a step.
        if amount < minimum && self.sanity < minimum {
            amount = self.sanity + 0.7;
        }
        if !force && self.unstable {
            amount = amount.min(self.sanity);
        }
        if amount > maximum {
            amount = maximum;
        }
        if amount == self.sanity {
            return false;
        }
        self.sanity = amount;
        if let Some(level) = sanity_level_of(amount) {
            self.sanity_level = level;
        }
        self.speed_applied = true;
        true
    }

    pub fn adjust_sanity(&mut self, amount: f32, minimum: f32, maximum: f32) -> bool {
        self.set_sanity(self.sanity + amount, minimum, maximum, false)
    }

    /// tg `/datum/mood/process(seconds_per_tick)`.
    pub fn process(&mut self, seconds: f32) {
        let (rate, minimum, maximum) = match self.mood_level {
            1 => (-0.3, SANITY_INSANE, SANITY_GREAT),
            2 => (-0.15, SANITY_CRAZY, SANITY_GREAT),
            3 => (-0.1, SANITY_UNSTABLE, SANITY_GREAT),
            4 => (-0.05, SANITY_UNSTABLE, SANITY_GREAT),
            // Brings sanity back up should it be below the minimum.
            5 => (0.0, SANITY_UNSTABLE, SANITY_GREAT),
            6 => (0.2, SANITY_UNSTABLE, SANITY_GREAT),
            7 => (0.3, SANITY_UNSTABLE, SANITY_GREAT),
            8 => (0.4, SANITY_NEUTRAL, SANITY_MAXIMUM),
            _ => (0.6, SANITY_NEUTRAL, SANITY_MAXIMUM),
        };
        self.adjust_sanity(rate * seconds, minimum, maximum);
    }

    /// `/datum/movespeed_modifier/sanity/*`, deciseconds per tile.
    pub fn move_slowdown(&self) -> f32 {
        match self.sanity_level {
            SANITY_LEVEL_INSANE => 1.0,
            SANITY_LEVEL_CRAZY => 0.5,
            SANITY_LEVEL_UNSTABLE => 0.25,
            _ => 0.0,
        }
    }

    /// `/datum/actionspeed_modifier/low_sanity` (+0.25) and `high_sanity` (-0.1): the factor
    /// of `do_after` delays.
    pub fn action_multiplier(&self) -> f32 {
        if !self.speed_applied {
            return 1.0;
        }
        match self.sanity_level {
            SANITY_LEVEL_INSANE | SANITY_LEVEL_CRAZY | SANITY_LEVEL_UNSTABLE => 1.25,
            SANITY_LEVEL_NEUTRAL | SANITY_LEVEL_GREAT => 0.9,
            _ => 1.0,
        }
    }

    /// The face: `mood[level]` tinted by the sanity level (`update_mood_icon`).
    pub fn hud_data(&self) -> MoodHudData {
        let color = match self.sanity_level {
            SANITY_LEVEL_GREAT => [0x2e, 0xeb, 0x9a],
            SANITY_LEVEL_NEUTRAL => [0x86, 0xd6, 0x56],
            SANITY_LEVEL_DISTURBED => [0x4b, 0x96, 0xc4],
            SANITY_LEVEL_UNSTABLE => [0xdf, 0xa6, 0x5b],
            SANITY_LEVEL_CRAZY => [0xf3, 0x89, 0x43],
            _ => [0xf1, 0x5d, 0x36],
        };
        MoodHudData {
            level: self.mood_level,
            color,
        }
    }

    /// Adds or clears `event` in `category` as `on` says.
    fn toggle(&mut self, now: f64, on: bool, category: &str, event: &MoodEvent) {
        if on {
            self.add_event(now, category, event);
        } else {
            self.clear_event(category);
        }
    }

    /// The events that follow the mob's state (see [`Needs`]).
    pub fn update_needs(&mut self, now: f64, needs: &Needs) {
        // `update_nutrition_moodlets`.
        match needs.nutrition {
            None => {
                self.clear_event("nutrition");
            }
            Some(_) if needs.fat && !needs.voracious => {
                self.add_event(now, "nutrition", &events::FAT);
            }
            Some(n) => match nutrition_event(n, needs.voracious) {
                Some(event) => self.add_event(now, "nutrition", event),
                None => {
                    self.clear_event("nutrition");
                }
            },
        }
        // `/datum/status_effect/inebriated/drunk`: from the tipsy threshold, its value follows
        // the drunkenness (`update_change`).
        if needs.drunk >= ss13_reagents::metabolism::TIPSY_THRESHOLD {
            let (change, description) = drunk_mood(needs.drunk);
            let existing = self
                .events
                .iter()
                .position(|e| e.category == "drunk" && e.id == events::DRUNK.id);
            match existing {
                Some(i) => {
                    if self.events[i].mood_change != change {
                        self.events[i].mood_change = change;
                        self.events[i].description = description.to_owned();
                        self.update_mood();
                    }
                }
                None => {
                    let mut drunk = ActiveMood::new("drunk", &events::DRUNK);
                    drunk.mood_change = change;
                    drunk.description = description.to_owned();
                    self.add_instance(now, drunk);
                }
            }
        } else {
            self.clear_event("drunk");
        }
        self.toggle(now, needs.handcuffed, "handcuffed", &events::HANDCUFFED);
        self.toggle(now, needs.on_fire, "on_fire", &events::ON_FIRE);
        self.toggle(now, needs.suffocating, "suffocation", &events::SUFFOCATION);
        // `body_temperature_alerts`: no cold mood while inebriated.
        if let Some(temperature) = needs.bodytemperature {
            let hot = temperature > BODYTEMP_HEAT_DAMAGE_LIMIT;
            let cold = !hot && temperature < BODYTEMP_COLD_DAMAGE_LIMIT && needs.drunk <= 0.0;
            self.toggle(now, hot, "hot", &events::HOT);
            self.toggle(now, cold, "cold", &events::COLD);
        }
    }
}

/// The nutrition moodlet for `nutrition` (`update_nutrition_moodlets`' switch; `None` clears).
pub fn nutrition_event(nutrition: f32, voracious: bool) -> Option<&'static MoodEvent> {
    if nutrition >= NUTRITION_LEVEL_FULL {
        Some(if voracious {
            &events::WELLFED
        } else {
            &events::TOO_WELLFED
        })
    } else if nutrition >= NUTRITION_LEVEL_WELL_FED {
        Some(&events::WELLFED)
    } else if nutrition >= NUTRITION_LEVEL_FED {
        Some(&events::FED)
    } else if nutrition >= NUTRITION_LEVEL_HUNGRY {
        None
    } else if nutrition >= NUTRITION_LEVEL_VERY_HUNGRY {
        Some(&events::HUNGRY)
    } else if nutrition >= NUTRITION_LEVEL_STARVING {
        Some(&events::HUNGRY_VERY)
    } else {
        Some(&events::STARVING)
    }
}

/// `/datum/mood_event/drunk/update_change`.
pub fn drunk_mood(drunk: f32) -> (f32, &'static str) {
    if drunk <= 30.0 {
        (3.0, "После одной-двух рюмок всё становится лучше.")
    } else if drunk <= 45.0 {
        (
            4.0,
            "Становится всё жарче, или мне только кажется? Мне нужно выпить ещё, чтобы остыть.",
        )
    } else if drunk <= 60.0 {
        (
            5.0,
            "Кто продолжает двигать пол? Я собираюсь поговорить с ним... после того, как выпью.",
        )
    } else if drunk <= 90.0 {
        (
            6.0,
            "Я вооовсе не пьян, ты пьян! На самом деле... Мне нужно выпить ещё!",
        )
    } else {
        (
            3.0,
            "Ты мой лууууууучший дру'хх! Мы с тобо'й прооотив все'гхо мииира, при...ятель. \
             Давааай ещё по одной...",
        )
    }
}

/// What the need events read from the mob.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Needs {
    /// `None`: no hunger (tg `TRAIT_NOHUNGER`).
    pub nutrition: Option<f32>,
    pub fat: bool,
    /// tg `TRAIT_VORACIOUS` (the Voracious quirk).
    pub voracious: bool,
    pub drunk: f32,
    pub handcuffed: bool,
    pub on_fire: bool,
    /// tg `failed_last_breath`.
    pub suffocating: bool,
    pub bodytemperature: Option<f64>,
}

/// The sanity movespeed modifier, deciseconds per tile (`crate::health` adds it to the delay).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct SanitySlowdown(pub f32);

/// tg `add_mood_event` on `mob` (no mood: nothing).
pub fn add_event(world: &mut World, mob: Entity, category: &str, event: &MoodEvent) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    if let Some(mut mood) = world.get_mut::<Mood>(mob) {
        mood.add_event(now, category, event);
    }
}

/// tg `/datum/mood_event/food` (`quality_food`) of a food of `quality` (1-7): `ceil(1 + 1.5 *
/// quality)` for 5 minutes times `timeout_mod`; another one keeps the stronger change and the
/// longer time (`be_refreshed`).
// shortcut: no personalities (ascetic, gourmand).
pub fn add_food_event(world: &mut World, mob: Entity, quality: i32, label: &str, timeout_mod: f32) {
    const CATEGORY: &str = "quality_food";
    const ID: &str = "food";
    const TIMEOUT: f32 = 300.0;
    let now = world.resource::<Time>().elapsed_secs_f64();
    let Some(mut mood) = world.get_mut::<Mood>(mob) else {
        return;
    };
    let change = (1.0 + 1.5 * quality as f32).ceil();
    if let Some(existing) = mood
        .events
        .iter_mut()
        .find(|e| e.category == CATEGORY && e.id == ID)
    {
        existing.timeout = existing.timeout.max(TIMEOUT * timeout_mod);
        if change > existing.mood_change {
            existing.mood_change = change;
            existing.description = format!("Эта еда была {label}.");
        }
        existing.expires = Some(now + f64::from(existing.timeout));
        mood.update_mood();
        return;
    }
    mood.add_instance(
        now,
        ActiveMood {
            category: CATEGORY.to_owned(),
            id: ID,
            description: format!("Эта была {label} пища."),
            mood_change: change,
            timeout: TIMEOUT * timeout_mod,
            expires: None,
            hidden: false,
            stacks: false,
        },
    );
}

/// tg `clear_mood_event` on `mob`.
pub fn clear_event(world: &mut World, mob: Entity, category: &str) {
    if let Some(mut mood) = world.get_mut::<Mood>(mob)
        && mood.get(category).is_some()
    {
        mood.clear_event(category);
    }
}

/// The sanity of `mob`, `SANITY_NEUTRAL` without a mood.
pub fn sanity(world: &World, mob: Entity) -> f32 {
    world.get::<Mood>(mob).map_or(SANITY_NEUTRAL, |m| m.sanity)
}

/// The mood moodlets of `carbon_defense.dm` `help_shake_act`'s hug: the hugged gets a hug (a
/// warm one from a warmer hugger, who then gets one back); a Friendly hugger in a good mood
/// gives a better one.
// shortcut: no bear hugs (aggressive grabs) and no `TRAIT_BADTOUCH`.
pub fn on_hug(world: &mut World, helper: Entity, target: Entity) {
    let temperature = |e: Entity| world.get::<Respiration>(e).map(|r| r.bodytemperature);
    let target_warmer = match (temperature(target), temperature(helper)) {
        (Some(t), Some(h)) => t > h,
        _ => false,
    };
    if target_warmer {
        add_event(world, helper, "hug", &events::WARMHUG);
        add_event(world, target, "hug", &events::HUG);
    } else {
        add_event(world, target, "hug", &events::WARMHUG);
    }
    if crate::quirks::has(world, helper, Quirk::Friendly) {
        let sanity = sanity(world, helper);
        if sanity >= SANITY_GREAT {
            add_event(world, target, "friendly_hug", &events::BESTHUG);
        } else if sanity >= SANITY_DISTURBED {
            add_event(world, target, "friendly_hug", &events::BETTERHUG);
        }
    }
}

/// tg `/datum/mood_event/conditional/see_death/get_descriptor`: the dead crew member's job, else
/// "кто-то".
fn death_descriptor(world: &World, dead: Entity) -> String {
    let named = world
        .get::<Name>(dead)
        .is_some_and(|n| n.as_str() != "Unknown");
    let job = world
        .get::<crate::jobs::PlayerJob>(dead)
        .and_then(|j| {
            world
                .get_resource::<crate::jobs::Jobs>()?
                .data
                .jobs
                .get(j.0)
        })
        .map(|j| j.title.to_lowercase());
    match job {
        Some(title) if named => title,
        _ => "кто-то".to_owned(),
    }
}

/// The `saw_death` moodlet for a death described as `descriptor`.
pub fn see_death_event(descriptor: &str) -> ActiveMood {
    let mut event = ActiveMood::new("saw_death", &events::SEE_DEATH);
    event.description = capitalize(&format!("Я видел, как {descriptor} умер. Как ужасно..."));
    event.stacks = true;
    event
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// tg `send_death_moodlets`: every conscious, seeing human that sees `dead` die at `pos`.
pub fn send_death_moodlets(world: &mut World, dead: Entity, pos: TilePos) {
    let descriptor = death_descriptor(world, dead);
    let now = world.resource::<Time>().elapsed_secs_f64();
    let nearby: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .in_radius(pos, DEATH_VIEW_RANGE)
        .filter(|&e| e != dead)
        .collect();
    for viewer in nearby {
        // `IS_UNCONSCIOUS`: hard crit and worse here.
        if !matches!(
            world.get::<Stat>(viewer),
            Some(Stat::Conscious | Stat::SoftCrit)
        ) || world.get::<Mood>(viewer).is_none()
            || crate::vision::is_blind(world, viewer)
        {
            continue;
        }
        let Some(&from) = world.get::<TilePos>(viewer) else {
            continue;
        };
        if !crate::mobs::ai::behaviors::can_see(world, from, pos, DEATH_VIEW_RANGE) {
            continue;
        }
        if let Some(mut mood) = world.get_mut::<Mood>(viewer) {
            mood.add_instance(now, see_death_event(&descriptor));
        }
    }
}

/// tg `print_mood` for the owner's chat.
pub fn print_mood(
    mood: &Mood,
    nutrition: Option<f32>,
    drunk: f32,
    quirks: &[&str],
) -> Vec<ChatSegment> {
    fn line(segments: &mut Vec<ChatSegment>, parts: &[(Span, &str)]) {
        for &(span, text) in parts {
            segments.push(ChatSegment::new(span, text));
        }
        segments.push(ChatSegment::new(Span::Plain, "\n"));
    }
    let mut out = Vec::new();
    // tg `span_nicegreen`/`span_boldnicegreen`; `span_grey` is plain text.
    let green = Span::GreenText;
    line(
        &mut out,
        &[(Span::Info, "Мое текущее ментальное состояние:")],
    );
    if let Some(n) = nutrition {
        let (span, text) = if n >= NUTRITION_LEVEL_FULL {
            (Span::Info, "У меня полный желудок!")
        } else if n >= NUTRITION_LEVEL_WELL_FED {
            (Span::Info, "Я неплохо поел!")
        } else if n >= NUTRITION_LEVEL_FED {
            (Span::Info, "Я не голоден.")
        } else if n >= NUTRITION_LEVEL_HUNGRY {
            (Span::Info, "Было бы неплохо что-то съесть.")
        } else if n >= NUTRITION_LEVEL_VERY_HUNGRY {
            (Span::Warning, "Я немного голоден.")
        } else if n >= NUTRITION_LEVEL_STARVING {
            (Span::Warning, "Я очень голоден.")
        } else {
            (Span::BoldWarning, "Я умираю с голоду!")
        };
        line(
            &mut out,
            &[(Span::Notice, "Мой текущий голод: "), (span, text)],
        );
    }
    if drunk >= 1.0 {
        let (span, text) = if drunk <= 10.0 {
            (Span::Info, "Я слегка навеселе.")
        } else if drunk <= 21.0 {
            (Span::Info, "Я чувствую себя немного пьяным.")
        } else if drunk <= 41.0 {
            (Span::Info, "Я чувствую себя довольно пьяным.")
        } else if drunk <= 61.0 {
            (Span::Info, "Сколько я выпил?")
        } else if drunk <= 81.0 {
            (Span::Warning, "Ещё одна рюмочка точно не повредит!")
        } else {
            (Span::BoldWarning, "Ик... где... где я? Кто... я?")
        };
        line(
            &mut out,
            &[(Span::Notice, "Моё текущее опьянение: "), (span, text)],
        );
    }
    let s = mood.sanity;
    let (span, text) = if s >= SANITY_GREAT {
        (green, "Мой разум словно храм!")
    } else if s >= SANITY_NEUTRAL {
        (green, "Я чувствую себя прекрасно!")
    } else if s >= SANITY_DISTURBED {
        (green, "Я чувствую себя вполне прилично.")
    } else if s >= SANITY_UNSTABLE {
        (
            Span::Warning,
            "Я чувствую себя немного не в своей тарелке...",
        )
    } else if s >= SANITY_CRAZY {
        (Span::Warning, "Я схожу с ума!!")
    } else {
        (Span::BoldWarning, "АХАХАХАХАХАХАХАХАХАХ!!")
    };
    line(
        &mut out,
        &[(Span::Notice, "Мой текущий рассудок: "), (span, text)],
    );
    let (span, text) = match mood.mood_level {
        1 => (Span::BoldWarning, "Лучше бы я подох!"),
        2 => (Span::BoldWarning, "Я чувствую себя ужасно..."),
        3 => (Span::BoldWarning, "Я чувствую себя расстроенно."),
        4 => (Span::Warning, "Я немного грущу."),
        5 => (Span::Plain, "Я в порядке."),
        6 => (green, "Я чувствую себя вполне нормально."),
        7 => (green, "Я чувствую себя довольно хорошо."),
        8 => (green, "Я чувствую себя потрясающе!"),
        _ => (green, "Я обожаю жизнь!"),
    };
    line(
        &mut out,
        &[(Span::Notice, "Моё текущее настроение: "), (span, text)],
    );
    line(&mut out, &[(Span::Notice, "Модификаторы:")]);
    if mood.events.is_empty() {
        line(
            &mut out,
            &[(
                Span::Plain,
                "• Сейчас у меня нет особой реакции на что-либо.",
            )],
        );
    }
    for event in &mood.events {
        let span = if event.mood_change <= MOOD_SAD2 {
            Span::BoldWarning
        } else if event.mood_change <= MOOD_SAD1 {
            Span::Warning
        } else if event.mood_change <= MOOD_NEUTRAL {
            Span::Plain
        } else if event.mood_change <= MOOD_HAPPY1 {
            Span::Info
        } else {
            green
        };
        line(
            &mut out,
            &[(Span::Plain, "• "), (span, event.description.as_str())],
        );
    }
    if !quirks.is_empty() {
        let text = format!(
            "У вас имеются следующие модификаторы: {}.",
            quirks.join(", ")
        );
        out.push(ChatSegment::new(Span::Notice, text));
    } else {
        out.pop();
    }
    out
}

impl ServerUi for MoodHudUi {
    type Source = &'static MoodHudData;
    type UserParam = ();
    type Act = ();

    fn static_data(_: &MoodHudData) {}

    fn data(source: &MoodHudData, _: Entity, _: &()) -> MoodHudData {
        *source
    }

    fn parse_act(action: &str, _: &UiParams) -> Result<(), UiReject> {
        if action == MOOD_CLICK {
            Ok(())
        } else {
            Err(UiReject::UnknownAction)
        }
    }
}

pub struct MoodServerPlugin;

impl Plugin for MoodServerPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui::<MoodHudUi>()
            .add_observer(give_mood)
            .add_systems(
                Update,
                (death_moodlets, mood_life, sync_mood_hud, mood_clicks)
                    .chain()
                    .before(HealthSystems)
                    .before(UiSystems::Open),
            );
    }
}

/// tg `/mob/living/carbon/human/setup_mood`.
fn give_mood(add: On<Add<HumanLook>>, mut commands: Commands) {
    commands.entity(add.entity).insert((
        Mood::default(),
        MoodHudData::default(),
        SanitySlowdown::default(),
    ));
}

type MoodItem<'a> = (
    &'a mut Mood,
    &'a Stat,
    Option<&'a Hunger>,
    Option<&'a Fat>,
    Option<&'a MobReagents>,
    Has<Handcuffed>,
    Option<&'a FireStacks>,
    Option<&'a Respiration>,
    Option<&'a Quirks>,
);

/// `SSmood`: timers, the need events and the sanity drift of every mood, every second.
// Cost: 1 Hz, one pass over humans (one per player), a few lookups and a clone of a short list.
fn mood_life(time: Res<Time>, mut clock: Local<f64>, mut mobs: Query<MoodItem>) {
    *clock += time.delta_secs_f64();
    let mut fires = 0;
    while *clock >= MOOD_WAIT_SECS {
        *clock -= MOOD_WAIT_SECS;
        fires += 1;
    }
    if fires == 0 {
        return;
    }
    let now = time.elapsed_secs_f64();
    for (mut mood, stat, hunger, fat, reagents, cuffed, fire, respiration, quirks) in &mut mobs {
        let mut new = mood.clone();
        new.expire(now);
        // tg stops processing a dead mob's mood (`handle_mob_death`).
        if *stat != Stat::Dead {
            let drunk = reagents.map_or(0.0, |r| r.drunk);
            new.update_needs(
                now,
                &Needs {
                    nutrition: hunger.map(|h| h.nutrition),
                    fat: fat.is_some_and(|f| f.0),
                    voracious: quirks.is_some_and(|q| q.has(Quirk::Voracious)),
                    drunk,
                    handcuffed: cuffed,
                    on_fire: fire.is_some_and(|f| f.on_fire),
                    suffocating: respiration.is_some_and(|r| r.failed_last_breath),
                    bodytemperature: respiration.map(|r| r.bodytemperature),
                },
            );
            for _ in 0..fires {
                new.process(MOOD_WAIT_SECS as f32);
            }
        }
        mood.set_if_neq(new);
    }
}

/// The owner's face and the sanity slowdown follow the mood.
fn sync_mood_hud(
    mut mobs: Query<
        (
            Entity,
            &Mood,
            &mut MoodHudData,
            &mut SanitySlowdown,
            Option<&PlayerMob>,
        ),
        Changed<Mood>,
    >,
    mut open: MessageWriter<OpenUi>,
) {
    for (mob, mood, mut hud, mut slowdown, player) in &mut mobs {
        slowdown.set_if_neq(SanitySlowdown(mood.move_slowdown()));
        if hud.set_if_neq(mood.hud_data())
            && let Some(player) = player
        {
            open.write(OpenUi::new::<MoodHudUi>(player.client, mob, mob, ""));
        }
    }
}

/// `hud_click`: the owner, awake, gets its mood printed.
fn mood_clicks(
    mut acts: MessageReader<UiActed<MoodHudUi>>,
    mobs: Query<(
        &Mood,
        &Stat,
        Option<&Hunger>,
        Option<&MobReagents>,
        Option<&Quirks>,
    )>,
    mut chat: MessageWriter<SendChat>,
) {
    for act in acts.read() {
        if act.actor != act.source {
            continue;
        }
        let Ok((mood, stat, hunger, reagents, quirks)) = mobs.get(act.actor) else {
            continue;
        };
        if !matches!(stat, Stat::Conscious | Stat::SoftCrit) {
            continue;
        }
        let names: Vec<&str> = quirks
            .map(|q| q.0.iter().map(|q| q.def().name).collect())
            .unwrap_or_default();
        chat.write(SendChat {
            clients: vec![act.client],
            message: ChatMessage {
                channel: ChatChannel::Info,
                segments: print_mood(
                    mood,
                    hunger.map(|h| h.nutrition),
                    reagents.map_or(0.0, |r| r.drunk),
                    &names,
                ),
            },
        });
    }
}

/// A human that dies (not a fake death) makes those who see it sad (`death()` ->
/// `send_death_moodlets`).
fn death_moodlets(
    changed: Query<(Entity, Ref<Stat>, &TilePos, Has<DeathComa>), (Changed<Stat>, With<HumanLook>)>,
    mut removed: RemovedComponents<Stat>,
    mut dead: Local<HashSet<Entity>>,
    mut commands: Commands,
) {
    for gone in removed.read() {
        dead.remove(&gone);
    }
    for (mob, stat, &pos, coma) in &changed {
        if *stat != Stat::Dead {
            dead.remove(&mob);
            continue;
        }
        // A body that comes into the world dead (a corpse) did not die in front of anyone.
        if !dead.insert(mob) || coma || stat.is_added() {
            continue;
        }
        commands.queue(move |world: &mut World| send_death_moodlets(world, mob, pos));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mood_levels_follow_the_dm_switch() {
        assert_eq!(mood_level(-25.0), 1);
        assert_eq!(mood_level(-20.0), 1);
        assert_eq!(mood_level(-19.0), 2);
        assert_eq!(mood_level(-7.0), 3);
        assert_eq!(mood_level(-3.0), 4);
        assert_eq!(mood_level(0.0), 5);
        assert_eq!(mood_level(2.0), 5);
        assert_eq!(mood_level(3.0), 6);
        assert_eq!(mood_level(8.0), 7);
        assert_eq!(mood_level(15.0), 8);
        assert_eq!(mood_level(16.0), 9);
    }

    #[test]
    fn events_add_refresh_replace_and_time_out() {
        let mut mood = Mood::default();
        mood.add_event(0.0, "hug", &events::HUG);
        assert_eq!((mood.mood, mood.mood_level), (1.0, 5));
        assert_eq!(mood.get("hug").unwrap().expires, Some(120.0));
        // The same type refreshes the timer.
        mood.add_event(60.0, "hug", &events::HUG);
        assert_eq!(mood.get("hug").unwrap().expires, Some(180.0));
        assert_eq!(mood.events.len(), 1);
        // Another type in the category replaces it.
        mood.add_event(60.0, "hug", &events::WARMHUG);
        assert_eq!(mood.get("hug").unwrap().id, "warmhug");
        mood.add_event(60.0, "nutrition", &events::STARVING);
        assert_eq!(mood.mood, -9.0);
        assert_eq!(mood.mood_level, 3);
        mood.expire(179.0);
        assert!(mood.get("hug").is_some());
        mood.expire(180.0);
        assert!(mood.get("hug").is_none());
        assert_eq!(mood.mood, -10.0);
        assert!(mood.clear_event("nutrition"));
        assert!(!mood.clear_event("nutrition"));
        assert_eq!((mood.mood, mood.mood_level), (0.0, 5));
    }

    #[test]
    fn food_moodlets_keep_the_best() {
        let mut world = World::new();
        world.init_resource::<Time>();
        let mob = world.spawn(Mood::default()).id();
        add_food_event(&mut world, mob, 2, "хорошая", 1.0);
        let get = |world: &World| {
            world
                .get::<Mood>(mob)
                .unwrap()
                .get("quality_food")
                .unwrap()
                .clone()
        };
        let e = get(&world);
        assert_eq!((e.mood_change, e.expires), (4.0, Some(300.0)));
        // A worse one refreshes: the stronger change stays, the longer time wins.
        add_food_event(&mut world, mob, 1, "приличная", 2.0);
        let e = get(&world);
        assert_eq!((e.mood_change, e.timeout), (4.0, 600.0));
        assert_eq!(world.get::<Mood>(mob).unwrap().mood, 4.0);
    }

    #[test]
    fn modifiers_scale_mood_and_lengths() {
        let mut mood = Mood {
            mood_modifier: 1.5,
            negative_length_modifier: 2.0,
            ..default()
        };
        mood.add_event(0.0, "depression", &events::DEPRESSION);
        assert_eq!(mood.mood, -18.0);
        assert_eq!(mood.get("depression").unwrap().expires, Some(240.0));
        // See-death stacks on a refresh.
        mood.add_instance(0.0, see_death_event("кто-то"));
        mood.add_instance(10.0, see_death_event("кто-то"));
        assert_eq!(mood.get("saw_death").unwrap().mood_change, -12.0);
        assert_eq!(
            mood.get("saw_death").unwrap().description,
            "Я видел, как кто-то умер. Как ужасно..."
        );
    }

    #[test]
    fn sanity_drifts_with_the_mood_level() {
        let mut mood = Mood::default();
        // Neutral: nothing changes, the level stays neutral.
        mood.process(1.0);
        assert_eq!(
            (mood.sanity, mood.sanity_level),
            (100.0, SANITY_LEVEL_NEUTRAL)
        );
        // Very sad: down 0.3/s to 0.
        mood.add_event(0.0, "x", &ev("x", "", -25.0, 0.0));
        for _ in 0..100 {
            mood.process(1.0);
        }
        assert!((mood.sanity - 70.0).abs() < 1e-3, "{}", mood.sanity);
        assert_eq!(mood.sanity_level, SANITY_LEVEL_UNSTABLE);
        assert_eq!(mood.move_slowdown(), 0.25);
        assert_eq!(mood.action_multiplier(), 1.25);
        // Happy (level 6) brings it back up, by 0.2/s, but no higher than great.
        mood.clear_event("x");
        mood.add_event(0.0, "x", &ev("x", "", 4.0, 0.0));
        for _ in 0..1000 {
            mood.process(1.0);
        }
        assert_eq!(mood.sanity, SANITY_GREAT);
        assert_eq!(mood.sanity_level, SANITY_LEVEL_NEUTRAL);
        // Unstable: sanity never rises.
        let mut unstable = Mood {
            unstable: true,
            sanity: 60.0,
            ..default()
        };
        unstable.add_event(0.0, "x", &ev("x", "", 20.0, 0.0));
        unstable.process(1.0);
        assert_eq!(unstable.sanity, 60.0);
    }

    #[test]
    fn needs_follow_nutrition_drunkenness_and_restraints() {
        let mut mood = Mood::default();
        let needs = Needs {
            nutrition: Some(100.0),
            drunk: 50.0,
            handcuffed: true,
            bodytemperature: Some(200.0),
            ..default()
        };
        mood.update_needs(0.0, &needs);
        assert_eq!(mood.get("nutrition").unwrap().id, "starving");
        assert_eq!(mood.get("drunk").unwrap().mood_change, 5.0);
        assert!(mood.get("handcuffed").is_some());
        // Drunk: no cold mood.
        assert!(mood.get("cold").is_none());
        mood.update_needs(
            1.0,
            &Needs {
                nutrition: Some(300.0),
                bodytemperature: Some(200.0),
                ..default()
            },
        );
        assert!(mood.get("nutrition").is_none());
        assert!(mood.get("drunk").is_none());
        assert!(mood.get("handcuffed").is_none());
        assert!(mood.get("cold").is_some());
        // Fat, unless voracious.
        mood.update_needs(
            2.0,
            &Needs {
                nutrition: Some(600.0),
                fat: true,
                ..default()
            },
        );
        assert_eq!(mood.get("nutrition").unwrap().id, "fat");
        mood.update_needs(
            3.0,
            &Needs {
                nutrition: Some(600.0),
                fat: true,
                voracious: true,
                ..default()
            },
        );
        assert_eq!(mood.get("nutrition").unwrap().id, "wellfed");
    }

    #[test]
    fn print_mood_lists_state_and_events() {
        let mut mood = Mood::default();
        mood.add_event(0.0, "hug", &events::HUG);
        let text: String = print_mood(&mood, Some(100.0), 0.0, &["Jolly"])
            .iter()
            .map(|s| s.text.as_str())
            .collect();
        assert!(text.contains("Я умираю с голоду!"), "{text}");
        assert!(text.contains("Я в порядке."), "{text}");
        assert!(text.contains("• Люблю обнимашки."), "{text}");
        assert!(text.contains("следующие модификаторы: Jolly."), "{text}");
        assert!(!text.contains("опьянение"));
    }
}
