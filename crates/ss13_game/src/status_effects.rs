//! Status effects and stamina (server), after tg `code/datums/status_effects/debuffs/debuffs.dm`
//! (`/datum/status_effect/incapacitating/*`), `stamcrit.dm`, `confusion.dm`, `staggered.dm`,
//! `jitteriness.dm`, `dizziness.dm`, `drowsiness.dm`, `speech_debuffs.dm`, `silenced.dm`,
//! `pacifism.dm`, `drunk.dm` (the drunk stage's side effects), `drugginess.dm` (its timer),
//! `buffs/stun_absorption.dm`, `code/modules/mob/living/status_procs.dm` and `damage_procs.dm`
//! (stamina).
//!
//! - [`StatusEffects`] holds the remaining seconds of tg's `Stun`, `Knockdown`, `Paralyze`,
//!   `Immobilize`, `incapacitate`, `Unconscious` and `Sleeping` ([`Incap`]), of the timed
//!   effects ([`Timed`]: confusion, jitter, dizziness, drowsiness, stutter, slurring, silence,
//!   pacifism, drugginess; dazed, staggered, no side kick), the mob's `staminaloss` and whether
//!   it is in stamina crit. `Stun(amount)` never shortens what is left (`max(amount,
//!   duration)`), `Set*` replaces it, `Adjust*` adds to it.
//! - Stamina: damage is clamped to `max_stamina` (120); any positive change restarts tg's
//!   `timed_stamina_reset`: `stamina_regen_time` (10 s) after the last stamina damage it is set
//!   back to 0 at once (there is no gradual regeneration in current tg). At `maxHealth -
//!   staminaloss <= crit_threshold` (100 stamina) a living mob enters stamcrit
//!   (incapacitated, immobilized, floored; +30 stamina below 120); further hits there have
//!   diminishing returns; it leaves stamcrit when stamina drops below 100.
//! - Unconsciousness and sleep are tg's `TRAIT_KNOCKEDOUT` (`/datum/status_effect/knocked_out`):
//!   incapacitated, immobilized, floored, hands blocked, blind (`crate::vision`), speechless
//!   (`crate::speech` reports hard crit to the chat), 3 stamina a second back. Sleep heals
//!   minor damage by its quality (mood, bed, table, bedsheet, pillow) and sobers.
//! - Every 2 s (tg `tick_interval`) the pulse: jitter and dizziness wear off 5 times faster
//!   while resting, dizziness sways the player's view ([`CameraEffect::Recoil`]), drowsiness
//!   blurs and may put to sleep, sleep heals, the drunk stage slurs, confuses, dizzies, blurs,
//!   makes vomit, damages the brain and blacks out.
//! - Speech effects reach the chat as [`SpeechEffects`] (stutter, slurring, silence);
//!   pacifism as `crate::medical::traumas::Pacifist` (`traumas::refresh`).
//! - [`Incapacitation`] is tg's traits that follow: `TRAIT_INCAPACITATED` (stun, paralyze,
//!   incapacitate, knocked out, stamcrit), `TRAIT_IMMOBILIZED` (+ immobilize), `TRAIT_FLOORED`
//!   (knockdown, paralyze, knocked out, stamcrit), `TRAIT_HANDS_BLOCKED` (stun, paralyze,
//!   knocked out, restraints), `TRAIT_RESTRAINED` (handcuffs, `crate::restraints`). Moves,
//!   clicks, hands, lying and resisting read it.
//! - `check_stun_immunity`: [`StunImmunity`] (`TRAIT_STUNIMMUNE` by source) and
//!   [`StunAbsorption`] (`/datum/status_effect/stun_absorption`: by priority, with messages, a
//!   cap and recharge) keep new stuns, knockdowns, paralysis, immobilization, incapacitation and
//!   unconsciousness from holding.
//!
//! The component exists only while something is active, so the per-tick countdown touches only
//! affected mobs.
// shortcut: no godmode or `status_flags` in the stun immunity (the CNS rebooter implant clears
// stuns itself), physiology multipliers only from bioware
// (`crate::medical::bioware`, in [`apply`], [`knockdown`], [`adjust_stamina`]), no force_say,
// no status alerts. Knocked out by a status effect, a mob keeps its `Stat` (tg `UNCONSCIOUS`):
// code that checks `IS_UNCONSCIOUS` through `Health::knocked_out` sees only crit; use
// [`knocked_out`] for both. No
// sleep immunity, dreams, snoring, organ or fitness healing while asleep,
// darkness or blindfold sleep bonus, drunk gun spread, drunken brawler or Ballmer peak lines,
// no TTS filters of the speech effects.

use bevy::prelude::*;
use ss13_net::chat::server::SendChat;
use ss13_shared::Dir;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};

use ss13_chat::speech::{Slur, SpeechEffects};
use ss13_net::camera::server::SendCameraEffect;
use ss13_shared::protocol::CameraEffect;

use crate::combat::CombatRng;
use crate::health::{BodyPosition, DamageType, Health, HealthSystems, MAX_HEALTH, Stat};
use crate::restraints::Handcuffed;
use crate::server::PlayerMob;

/// `max_stamina` of `/mob/living`.
pub const MAX_STAMINA: f32 = 120.0;
/// `stamina_regen_time`, seconds.
pub const STAMINA_REGEN_TIME: f64 = 10.0;
/// `maxHealth - crit_threshold`: stamina at or above which a carbon enters stamcrit.
pub const STAMINA_CRIT: f32 = MAX_HEALTH;
/// Stamina stamcrit's `on_creation` adds below 120.
pub const STAMCRIT_PUSH: f32 = 30.0;
/// `CONFUSION_FULL_THRESHOLD`, `CONFUSION_SIDEWAYS_MOVE_PROB_PER_SECOND`,
/// `CONFUSION_DIAGONAL_MOVE_PROB_PER_SECOND`.
const CONFUSION_FULL_THRESHOLD: f32 = 40.0;
const CONFUSION_SIDEWAYS_PER_SECOND: f32 = 1.5;
const CONFUSION_DIAGONAL_PER_SECOND: f32 = 3.0;
/// `/datum/status_effect/dazed` default duration, `/datum/status_effect/no_side_kick::duration`.
pub const DAZED_SECS: f32 = 3.0;
pub const NO_SIDE_KICK_SECS: f32 = 3.6;
/// `STAGGERED_SLOWDOWN_STRENGTH`, deciseconds per tile.
pub const STAGGERED_SLOWDOWN: f32 = 0.85;

/// tg's incapacitating status effects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Incap {
    Stun,
    Knockdown,
    Paralyze,
    Immobilize,
    /// `/datum/status_effect/incapacitating/incapacitated` (`incapacitate()`): only
    /// `TRAIT_INCAPACITATED`.
    Incapacitated,
    /// `/datum/status_effect/incapacitating/unconscious` (`Unconscious()`).
    Unconscious,
    /// `/datum/status_effect/incapacitating/sleeping` (`Sleeping()`).
    Sleeping,
}

/// The slurring status effects (`/datum/status_effect/speech/slurring/*`); each runs on its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlurKind {
    Generic,
    Drunk,
    Cult,
    Heretic,
}

impl SlurKind {
    const ALL: [Self; 4] = [Self::Generic, Self::Drunk, Self::Cult, Self::Heretic];

    fn index(self) -> usize {
        self as usize
    }
}

/// tg's timed status effects that do not incapacitate (`adjust_timed_status_effect`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Timed {
    Confusion,
    /// `/datum/status_effect/jitter`.
    Jitter,
    /// `/datum/status_effect/dizziness`.
    Dizziness,
    /// `/datum/status_effect/drowsiness`.
    Drowsiness,
    /// `/datum/status_effect/speech/stutter`.
    Stutter,
    Slurring(SlurKind),
    /// `/datum/status_effect/silenced` (`TRAIT_MUTE`).
    Silence,
    /// `/datum/status_effect/pacify` (`TRAIT_PACIFISM`).
    Pacify,
    /// `/datum/status_effect/drugginess` (the "high" alert and moodlet, `crate::buffs`).
    Drugginess,
}

/// What stamina damage did (for the messages tg prints).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StaminaEvent {
    None,
    /// `stamcrit/on_apply`.
    EnteredCrit,
    /// `update_diminishing_return` swallowed the damage.
    Ignored,
}

/// tg `tick_interval` of jitter, dizziness, drowsiness, sleeping and drunkenness, seconds.
pub const PULSE_SECS: f32 = 2.0;
/// `/datum/status_effect/knocked_out/tick`: stamina healed per second.
const KNOCKED_OUT_STAMINA_PER_SECOND: f32 = 3.0;

/// Remaining seconds of each effect (0 = none) and the stamina state.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct StatusEffects {
    pub stun: f32,
    pub knockdown: f32,
    pub paralyze: f32,
    pub immobilize: f32,
    pub incapacitate: f32,
    pub unconscious: f32,
    pub sleeping: f32,
    pub confusion: f32,
    pub dazed: f32,
    pub staggered: f32,
    pub no_side_kick: f32,
    pub jitter: f32,
    pub dizziness: f32,
    pub drowsiness: f32,
    pub stutter: f32,
    /// By [`SlurKind`].
    pub slurring: [f32; 4],
    pub silence: f32,
    pub pacify: f32,
    pub drugginess: f32,
    /// `staminaloss`.
    pub stamina: f32,
    /// `/datum/status_effect/incapacitating/stamcrit`.
    pub stamcrit: bool,
    /// `Time::elapsed_secs_f64` of `timed_stamina_reset`.
    stamina_reset_at: Option<f64>,
    /// stamcrit's `diminishing_return_counter`.
    diminishing: f32,
    /// Seconds towards the next 2 s pulse.
    pulse_timer: f32,
    /// A pulse is due ([`pulse_effects`]).
    pub(crate) pulse_due: bool,
    /// The pacifism `crate::medical::traumas::refresh` last saw.
    pacified_seen: bool,
}

impl StatusEffects {
    fn slot(&mut self, kind: Incap) -> &mut f32 {
        match kind {
            Incap::Stun => &mut self.stun,
            Incap::Knockdown => &mut self.knockdown,
            Incap::Paralyze => &mut self.paralyze,
            Incap::Immobilize => &mut self.immobilize,
            Incap::Incapacitated => &mut self.incapacitate,
            Incap::Unconscious => &mut self.unconscious,
            Incap::Sleeping => &mut self.sleeping,
        }
    }

    pub fn remaining(&self, kind: Incap) -> f32 {
        match kind {
            Incap::Stun => self.stun,
            Incap::Knockdown => self.knockdown,
            Incap::Paralyze => self.paralyze,
            Incap::Immobilize => self.immobilize,
            Incap::Incapacitated => self.incapacitate,
            Incap::Unconscious => self.unconscious,
            Incap::Sleeping => self.sleeping,
        }
    }

    fn timed_slot(&mut self, kind: Timed) -> &mut f32 {
        match kind {
            Timed::Confusion => &mut self.confusion,
            Timed::Jitter => &mut self.jitter,
            Timed::Dizziness => &mut self.dizziness,
            Timed::Drowsiness => &mut self.drowsiness,
            Timed::Stutter => &mut self.stutter,
            Timed::Slurring(kind) => &mut self.slurring[kind.index()],
            Timed::Silence => &mut self.silence,
            Timed::Pacify => &mut self.pacify,
            Timed::Drugginess => &mut self.drugginess,
        }
    }

    /// Seconds left of a timed effect (tg `has_status_effect(...)?.duration`).
    pub fn timed(&self, kind: Timed) -> f32 {
        match kind {
            Timed::Confusion => self.confusion,
            Timed::Jitter => self.jitter,
            Timed::Dizziness => self.dizziness,
            Timed::Drowsiness => self.drowsiness,
            Timed::Stutter => self.stutter,
            Timed::Slurring(kind) => self.slurring[kind.index()],
            Timed::Silence => self.silence,
            Timed::Pacify => self.pacify,
            Timed::Drugginess => self.drugginess,
        }
    }

    /// tg `adjust_timed_status_effect(secs, effect, max)`: adds (up to `max` for a positive
    /// amount), removes at 0.
    pub fn adjust_timed(&mut self, kind: Timed, secs: f32, max: Option<f32>) {
        let slot = self.timed_slot(kind);
        *slot = match max {
            Some(max) => adjust_up_to(*slot, secs.min(max), max),
            None => (*slot + secs).max(0.0),
        };
    }

    /// tg `set_timed_status_effect(secs, effect, only_if_higher)`.
    pub fn set_timed(&mut self, kind: Timed, secs: f32, only_if_higher: bool) {
        let slot = self.timed_slot(kind);
        *slot = if only_if_higher {
            slot.max(secs)
        } else {
            secs.max(0.0)
        };
    }

    /// `dazed/clear_daze`: stuns, paralysis and immobilization above 0 end the daze.
    fn on_status(&mut self, kind: Incap, secs: f32) {
        if secs > 0.0 && matches!(kind, Incap::Stun | Incap::Paralyze | Incap::Immobilize) {
            self.dazed = 0.0;
        }
    }

    /// tg `Stun`/`Knockdown`/`Paralyze`/`Immobilize(amount)`: never shortens what is left.
    /// True when the effect is on afterwards.
    pub fn apply(&mut self, kind: Incap, secs: f32) -> bool {
        self.on_status(kind, secs);
        let slot = self.slot(kind);
        *slot = slot.max(secs);
        *slot > 0.0
    }

    /// tg `Set*`: replaces the remaining time (0 removes it).
    pub fn set(&mut self, kind: Incap, secs: f32) {
        self.on_status(kind, secs);
        *self.slot(kind) = secs.max(0.0);
    }

    /// tg `Adjust*`: adds to the remaining time; starts the effect only for a positive amount.
    pub fn adjust(&mut self, kind: Incap, secs: f32) {
        self.on_status(kind, secs);
        let slot = self.slot(kind);
        *slot = (*slot + secs).max(0.0);
    }

    /// tg `AllImmobility(secs)`: stun, knockdown, paralysis, immobilization and unconsciousness.
    pub fn all_immobility(&mut self, secs: f32) {
        for kind in [
            Incap::Paralyze,
            Incap::Knockdown,
            Incap::Stun,
            Incap::Immobilize,
            Incap::Unconscious,
        ] {
            self.apply(kind, secs);
        }
    }

    /// tg `SetAllImmobility(secs)`.
    pub fn set_all_immobility(&mut self, secs: f32) {
        for kind in [
            Incap::Paralyze,
            Incap::Knockdown,
            Incap::Stun,
            Incap::Immobilize,
            Incap::Unconscious,
        ] {
            self.set(kind, secs);
        }
    }

    /// tg `AdjustAllImmobility(secs)`.
    pub fn adjust_all_immobility(&mut self, secs: f32) {
        for kind in [
            Incap::Paralyze,
            Incap::Knockdown,
            Incap::Stun,
            Incap::Immobilize,
            Incap::Unconscious,
        ] {
            self.adjust(kind, secs);
        }
    }

    /// tg `adjust_status_effects_on_shake_up` (a help-shake or hug).
    pub fn shake_up(&mut self) {
        for kind in [
            Incap::Stun,
            Incap::Knockdown,
            Incap::Unconscious,
            Incap::Paralyze,
            Incap::Immobilize,
        ] {
            self.adjust(kind, -6.0);
        }
        self.adjust(Incap::Sleeping, -10.0);
    }

    /// tg `Knockdown(amount, daze_amount)`: the knockdown, then `/datum/status_effect/dazed`
    /// (unique: an existing daze is kept; refused for the dead, under `no_side_kick` or while
    /// immobilized).
    pub fn knockdown(&mut self, secs: f32, daze: f32, dead: bool) {
        self.apply(Incap::Knockdown, secs);
        if daze > 0.0
            && self.dazed <= 0.0
            && !dead
            && self.no_side_kick <= 0.0
            && !self.immobilized()
        {
            self.dazed = daze;
        }
    }

    /// tg `TRAIT_KNOCKEDOUT` from `Unconscious` or `Sleeping`.
    pub fn knocked_out(&self) -> bool {
        self.unconscious > 0.0 || self.sleeping > 0.0
    }

    pub fn incapacitated(&self) -> bool {
        self.stun > 0.0
            || self.paralyze > 0.0
            || self.incapacitate > 0.0
            || self.knocked_out()
            || self.stamcrit
    }

    pub fn immobilized(&self) -> bool {
        self.stun > 0.0
            || self.paralyze > 0.0
            || self.knocked_out()
            || self.stamcrit
            || self.immobilize > 0.0
    }

    pub fn floored(&self) -> bool {
        self.knockdown > 0.0 || self.paralyze > 0.0 || self.knocked_out() || self.stamcrit
    }

    pub fn hands_blocked(&self) -> bool {
        self.stun > 0.0 || self.paralyze > 0.0 || self.knocked_out()
    }

    /// tg `TRAIT_PACIFISM` from `/datum/status_effect/pacify`.
    pub fn pacified(&self) -> bool {
        self.pacify > 0.0
    }

    /// tg `adjust_stamina_loss(amount)` on a carbon (`forced`: skips the stamcrit diminishing
    /// returns), at `now`.
    pub fn adjust_stamina(
        &mut self,
        amount: f32,
        now: f64,
        dead: bool,
        forced: bool,
    ) -> StaminaEvent {
        if amount == 0.0 {
            return StaminaEvent::None;
        }
        if amount > 0.0 && !forced && self.stamcrit {
            // `update_diminishing_return`.
            let modded = (amount.sqrt() / 2.0).ceil() - self.diminishing;
            if amount >= 5.0 {
                self.diminishing += 1.0;
            } else if amount >= 2.0 {
                self.diminishing += 0.05;
            }
            if modded <= 0.0 {
                return StaminaEvent::Ignored;
            }
        }
        let old = self.stamina;
        self.stamina = (self.stamina + amount).clamp(0.0, MAX_STAMINA);
        let mut event = StaminaEvent::None;
        if self.stamina > old {
            self.stamina_reset_at = Some(now + STAMINA_REGEN_TIME);
            if self.stamina >= STAMINA_CRIT && !dead && !self.stamcrit {
                self.stamcrit = true;
                self.diminishing = 0.0;
                if self.stamina < MAX_STAMINA {
                    self.stamina = (self.stamina + STAMCRIT_PUSH).min(MAX_STAMINA);
                }
                event = StaminaEvent::EnteredCrit;
            }
        }
        self.check_stamcrit();
        event
    }

    /// `stamcrit/check_remove`.
    fn check_stamcrit(&mut self) {
        if self.stamcrit && self.stamina < STAMINA_CRIT {
            self.stamcrit = false;
        }
    }

    /// tg `adjust_timed_status_effect(amount, effect, max)` for confusion.
    pub fn adjust_confusion_up_to(&mut self, secs: f32, max: f32) {
        self.confusion = adjust_up_to(self.confusion, secs, max);
    }

    /// tg `set_confusion_if_lower`.
    pub fn set_confusion_if_lower(&mut self, secs: f32) {
        self.confusion = self.confusion.max(secs);
    }

    /// tg `adjust_staggered_up_to`.
    pub fn adjust_staggered_up_to(&mut self, secs: f32, max: f32, dead: bool) {
        if !dead {
            self.staggered = adjust_up_to(self.staggered, secs, max);
        }
    }

    /// Whether anything that acts on the 2 s pulse runs.
    fn pulsing(&self) -> bool {
        self.jitter > 0.0 || self.dizziness > 0.0 || self.drowsiness > 0.0 || self.sleeping > 0.0
    }

    /// One countdown step of `secs` at `now` (tg `process` of each effect, the stamina reset
    /// timer). `lying`: the body lies (a standing mob loses its daze); `dead`: tg's
    /// `COMSIG_LIVING_DEATH` ends jitter, dizziness and silence. True while anything is left.
    pub fn tick(&mut self, secs: f32, now: f64, lying: bool, dead: bool) -> bool {
        if dead {
            self.jitter = 0.0;
            self.dizziness = 0.0;
            self.silence = 0.0;
            // `drugginess/remove_drugginess` on `COMSIG_LIVING_DEATH`.
            self.drugginess = 0.0;
        }
        // `/datum/status_effect/knocked_out/tick` (it stops ticking for the dead).
        if self.knocked_out() && !dead && self.stamina > 0.0 {
            self.stamina = (self.stamina - KNOCKED_OUT_STAMINA_PER_SECOND * secs).max(0.0);
            self.check_stamcrit();
        }
        for value in [
            &mut self.stun,
            &mut self.knockdown,
            &mut self.paralyze,
            &mut self.immobilize,
            &mut self.incapacitate,
            &mut self.unconscious,
            &mut self.sleeping,
            &mut self.confusion,
            &mut self.dazed,
            &mut self.staggered,
            &mut self.no_side_kick,
            &mut self.jitter,
            &mut self.dizziness,
            &mut self.drowsiness,
            &mut self.stutter,
            &mut self.silence,
            &mut self.pacify,
            &mut self.drugginess,
        ] {
            *value = (*value - secs).max(0.0);
        }
        for value in &mut self.slurring {
            *value = (*value - secs).max(0.0);
        }
        if self.stamina_reset_at.is_some_and(|at| now >= at) {
            // `set_stamina_loss(0, forced = TRUE)`.
            self.stamina = 0.0;
            self.stamina_reset_at = None;
            self.check_stamcrit();
        }
        // `dazed/clear_daze_on_stand`.
        if !lying && !self.floored() {
            self.dazed = 0.0;
        }
        if self.pulsing() {
            self.pulse_timer += secs;
            if self.pulse_timer >= PULSE_SECS {
                self.pulse_timer -= PULSE_SECS;
                self.pulse_due = true;
            }
        } else {
            self.pulse_timer = 0.0;
        }
        !self.idle()
    }

    pub fn idle(&self) -> bool {
        self.stun <= 0.0
            && self.knockdown <= 0.0
            && self.paralyze <= 0.0
            && self.immobilize <= 0.0
            && self.incapacitate <= 0.0
            && !self.knocked_out()
            && self.confusion <= 0.0
            && self.dazed <= 0.0
            && self.staggered <= 0.0
            && self.no_side_kick <= 0.0
            && self.jitter <= 0.0
            && self.dizziness <= 0.0
            && self.drowsiness <= 0.0
            && self.stutter <= 0.0
            && self.slurring.iter().all(|s| *s <= 0.0)
            && self.silence <= 0.0
            && self.pacify <= 0.0
            && self.drugginess <= 0.0
            // Kept until the pacifism's end reached `traumas::refresh`.
            && !self.pacified_seen
            && self.stamina <= 0.0
            && !self.stamcrit
    }

    /// The speech effects the chat applies.
    pub fn speech(&self, drunk: f32) -> SpeechEffects {
        let mut slurs = [None; 4];
        for kind in SlurKind::ALL {
            if self.slurring[kind.index()] > 0.0 {
                slurs[kind.index()] = Some(match kind {
                    SlurKind::Generic => Slur::GENERIC,
                    SlurKind::Drunk => Slur::drunk(drunk),
                    SlurKind::Cult => Slur::CULT,
                    SlurKind::Heretic => Slur::HERETIC,
                });
            }
        }
        SpeechEffects {
            stutter: self.stutter > 0.0,
            silenced: self.silence > 0.0,
            slurs,
        }
    }

    /// `/datum/status_effect/jitter/get_examine_text`.
    pub fn jitter_examine(&self) -> Option<(Span, &'static str)> {
        if self.jitter >= 300.0 {
            Some((Span::BoldWarning, "сильно бьётся в конвульсиях!"))
        } else if self.jitter >= 180.0 {
            Some((Span::Warning, "крайне взвинчен."))
        } else if self.jitter >= 60.0 {
            Some((Span::Warning, "слегка подёргивается."))
        } else {
            None
        }
    }
}

fn adjust_up_to(current: f32, secs: f32, max: f32) -> f32 {
    if secs > 0.0 {
        if current >= max {
            return current;
        }
        current + secs.min(max - current)
    } else {
        (current + secs).max(0.0)
    }
}

/// tg `/datum/status_effect/confusion/on_move`: the direction a confused mob actually steps.
/// `rand(n)` is a uniform roll below `n` (DM `prob(p)` is `rand(10000) < p * 100`).
pub fn confused_dir(confusion: f32, dir: Dir, mut rand: impl FnMut(u32) -> u32) -> Dir {
    const CLOCKWISE: [Dir; 8] = [
        Dir::North,
        Dir::NorthEast,
        Dir::East,
        Dir::SouthEast,
        Dir::South,
        Dir::SouthWest,
        Dir::West,
        Dir::NorthWest,
    ];
    let index = CLOCKWISE.iter().position(|&d| d == dir).unwrap_or(0);
    let mut roll = |p: f32| (rand(10_000) as f32) / 100.0 < p;
    let turn = |steps: i32| CLOCKWISE[(index as i32 + steps).rem_euclid(8) as usize];
    if confusion > CONFUSION_FULL_THRESHOLD {
        return CLOCKWISE[rand(8) as usize];
    }
    if roll(confusion * CONFUSION_SIDEWAYS_PER_SECOND) {
        return turn(if rand(2) == 0 { 2 } else { -2 });
    }
    if roll(confusion * CONFUSION_DIAGONAL_PER_SECOND) {
        return turn(if rand(2) == 0 { 1 } else { -1 });
    }
    dir
}

/// tg traits that follow from status effects and restraints.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Incapacitation {
    pub incapacitated: bool,
    pub immobilized: bool,
    pub floored: bool,
    pub hands_blocked: bool,
    pub restrained: bool,
    /// `/datum/status_effect/staggered` (a movespeed modifier).
    pub staggered: bool,
    /// Lying by choice (tg `resting`, [`crate::rest`]): drawn lying and crawling, not floored.
    pub resting: bool,
    /// tg `TRAIT_KNOCKEDOUT` from `Unconscious` or `Sleeping` (not from crit: see
    /// [`knocked_out`]).
    pub knocked_out: bool,
}

impl Incapacitation {
    fn of(
        effects: Option<&StatusEffects>,
        cuffed: bool,
        limbs: Option<&crate::medical::LimbLimits>,
        rest: Option<&crate::rest::Rest>,
        locked: bool,
        held: Held,
    ) -> Self {
        let e = |f: fn(&StatusEffects) -> bool| effects.is_some_and(f);
        // `CHOKEHOLD_TRAIT` of a grab (`crate::pull`), `BUCKLED_TRAIT` of a carry.
        let choked = held.grab.is_some();
        let neck = held.grab >= Some(crate::pull::GrabState::Neck);
        Self {
            incapacitated: e(StatusEffects::incapacitated),
            // Buckled lying, no usable legs (`crate::medical`); a locked down cyborg
            // (`crate::silicon::chassis::LockedDown`).
            immobilized: e(StatusEffects::immobilized)
                || limbs.is_some_and(|l| l.immobilized)
                || locked
                || choked
                || held.carried.is_some()
                || held.frozen,
            floored: e(StatusEffects::floored)
                || limbs.is_some_and(|l| l.floored)
                || neck
                || held.carried == Some(true),
            hands_blocked: e(StatusEffects::hands_blocked) || cuffed || locked || choked,
            restrained: cuffed,
            staggered: effects.is_some_and(|e| e.staggered > 0.0),
            resting: rest.is_some_and(|r| r.down),
            knocked_out: e(StatusEffects::knocked_out),
        }
    }
}

/// What holds a mob in place from outside its status effects: the grab state of a
/// [`crate::pull::Chokehold`], and a carry ([`crate::pull::carry::Carried`], `Some(true)` for a
/// fireman carry, which floors).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Held {
    pub grab: Option<crate::pull::GrabState>,
    pub carried: Option<bool>,
    /// In an ice cube ([`crate::freon::Frozen`]).
    pub frozen: bool,
}

/// The mob's [`Incapacitation`] (none for things without it).
pub fn incapacitation(world: &World, mob: Entity) -> Incapacitation {
    world
        .get::<Incapacitation>(mob)
        .copied()
        .unwrap_or_default()
}

/// The mob's status effects, added when missing; `None` for things that are not living mobs.
pub fn effects_mut(world: &mut World, mob: Entity) -> Option<Mut<'_, StatusEffects>> {
    let entity = world.get_entity(mob).ok()?;
    if !entity.contains::<Stat>() {
        return None;
    }
    if !entity.contains::<StatusEffects>() {
        world.entity_mut(mob).insert(StatusEffects::default());
    }
    world.get_mut::<StatusEffects>(mob)
}

fn is_dead(world: &World, mob: Entity) -> bool {
    world.get::<Stat>(mob) == Some(&Stat::Dead)
}

/// tg `TRAIT_STUNIMMUNE` by source (`ADD_TRAIT(mob, TRAIT_STUNIMMUNE, source)`): no new stun,
/// knockdown, paralysis, immobilization, incapacitation or unconsciousness holds.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct StunImmunity(pub Vec<&'static str>);

/// tg `ADD_TRAIT(mob, TRAIT_STUNIMMUNE, source)`.
pub fn add_stun_immunity(world: &mut World, mob: Entity, source: &'static str) {
    if world.get::<StunImmunity>(mob).is_none() {
        if let Ok(mut entity) = world.get_entity_mut(mob) {
            entity.insert(StunImmunity(vec![source]));
        }
        return;
    }
    if let Some(mut immunity) = world.get_mut::<StunImmunity>(mob)
        && !immunity.0.contains(&source)
    {
        immunity.0.push(source);
    }
}

/// tg `REMOVE_TRAIT(mob, TRAIT_STUNIMMUNE, source)`.
pub fn remove_stun_immunity(world: &mut World, mob: Entity, source: &'static str) {
    let Some(mut immunity) = world.get_mut::<StunImmunity>(mob) else {
        return;
    };
    immunity.0.retain(|s| *s != source);
    if immunity.0.is_empty() {
        world.entity_mut(mob).remove::<StunImmunity>();
    }
}

/// One `/datum/status_effect/stun_absorption` (`add_stun_absorption`'s arguments).
#[derive(Clone, Debug, PartialEq)]
pub struct StunAbsorber {
    pub source: &'static str,
    pub priority: i32,
    /// `visible_message` to the others around on a trigger, `%EFFECT_OWNER` the owner's name.
    pub message: Option<String>,
    /// `to_chat` of the owner on a trigger.
    pub self_message: Option<String>,
    /// The examine line while it can absorb, `%EFFECT_OWNER_THEYRE` the owner's pronoun.
    pub examine_message: Option<String>,
    /// `max_seconds_of_stuns_blocked` (`f32::INFINITY`: no limit).
    pub max_secs: f32,
    /// `delete_after_passing_max`; else it recharges each absorbed stun after `recharge_secs`.
    pub delete_after_max: bool,
    pub recharge_secs: f32,
    /// Game time it ends at (`duration`; `None`: until removed).
    pub until: Option<f64>,
    /// `seconds_of_stuns_absorbed`.
    pub absorbed: f32,
    /// The pending `recharge_absorption` timers: (game time, seconds given back).
    recharges: Vec<(f64, f32)>,
}

impl StunAbsorber {
    /// `add_stun_absorption(source, priority = -1)` with tg's other defaults.
    pub fn new(source: &'static str) -> Self {
        Self {
            source,
            priority: -1,
            message: None,
            self_message: None,
            examine_message: None,
            max_secs: f32::INFINITY,
            delete_after_max: true,
            recharge_secs: 60.0,
            until: None,
            absorbed: 0.0,
            recharges: Vec::new(),
        }
    }

    pub fn priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    pub fn messages(mut self, message: &str, self_message: &str) -> Self {
        self.message = (!message.is_empty()).then(|| message.to_owned());
        self.self_message = (!self_message.is_empty()).then(|| self_message.to_owned());
        self
    }

    /// The amount `absorb_stun` still takes (`seconds_of_stuns_absorbed > max`: none).
    fn charged(&self) -> bool {
        self.absorbed <= self.max_secs
    }
}

/// The mob's stun absorptions, in the order they were added.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct StunAbsorption(pub Vec<StunAbsorber>);

/// tg `add_stun_absorption`: a source already absorbing at the same or a higher priority is
/// kept; a lower one is replaced. `duration_secs` is from now.
pub fn add_stun_absorption(
    world: &mut World,
    mob: Entity,
    mut absorber: StunAbsorber,
    duration_secs: Option<f32>,
) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    absorber.until = duration_secs.map(|d| now + f64::from(d));
    if world.get::<StunAbsorption>(mob).is_none() {
        let Ok(mut entity) = world.get_entity_mut(mob) else {
            return;
        };
        entity.insert(StunAbsorption::default());
    }
    let mut all = world
        .get_mut::<StunAbsorption>(mob)
        .expect("inserted above");
    if let Some(i) = all.0.iter().position(|a| a.source == absorber.source) {
        if all.0[i].priority >= absorber.priority {
            return;
        }
        all.0.remove(i);
    }
    all.0.push(absorber);
}

/// tg `remove_stun_absorption(source)`.
pub fn remove_stun_absorption(world: &mut World, mob: Entity, source: &'static str) {
    let Some(mut all) = world.get_mut::<StunAbsorption>(mob) else {
        return;
    };
    all.0.retain(|a| a.source != source);
    if all.0.is_empty() {
        world.entity_mut(mob).remove::<StunAbsorption>();
    }
}

/// `stun_absorption/can_absorb_stun`'s state part: not unconscious or in crit.
fn can_absorb(world: &World, mob: Entity) -> bool {
    world.get::<Stat>(mob) == Some(&Stat::Conscious) && !knocked_out(world, mob)
}

/// `stun_absorption/absorb_stun(amount)` of the absorber that takes it: the first of the highest
/// priority (tg lets a higher priority one decide, absorbing or not). Messages and the count
/// only for an amount above 0 (a generic check absorbs "0 seconds" silently).
// shortcut: two absorbers of the same top priority both absorb in tg (two messages, both
// counted); here only the first does.
fn absorb_stun(world: &mut World, mob: Entity, secs: f32) -> bool {
    if !can_absorb(world, mob) {
        return false;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    let (message, self_message) = {
        let Some(mut all) = world.get_mut::<StunAbsorption>(mob) else {
            return false;
        };
        let Some(top) = all.0.iter().map(|a| a.priority).max() else {
            return false;
        };
        let Some(index) = all.0.iter().position(|a| a.priority == top) else {
            return false;
        };
        if !all.0[index].charged() {
            return false;
        }
        if secs <= 0.0 {
            return true;
        }
        let absorber = &mut all.0[index];
        absorber.absorbed += secs;
        let texts = (absorber.message.clone(), absorber.self_message.clone());
        let spent = absorber.absorbed >= absorber.max_secs;
        if !absorber.delete_after_max && absorber.recharge_secs > 0.0 {
            let at = now + f64::from(absorber.recharge_secs);
            absorber.recharges.push((at, secs));
        }
        if absorber.delete_after_max && spent {
            all.0.remove(index);
        }
        texts
    };
    if let Some(message) = message {
        let text = message.replace("%EFFECT_OWNER", &crate::chem::name_of(world, mob));
        if let Some(pos) = crate::chem::map_pos(world, mob) {
            let clients = crate::chem::clients_near(world, pos, Some(mob));
            if !clients.is_empty() {
                world.write_message(SendChat {
                    clients,
                    message: ChatMessage {
                        channel: ChatChannel::Info,
                        segments: vec![ChatSegment::new(Span::Warning, text)],
                    },
                });
            }
        }
    }
    if let Some(text) = self_message {
        tell_mob(world, mob, Span::BoldWarning, &text);
    }
    true
}

/// tg `check_stun_immunity` (`COMSIG_LIVING_GENERIC_STUN_CHECK`, `TRAIT_STUNIMMUNE`) and the
/// `COMSIG_LIVING_STATUS_*` signal a stun absorption blocks, for `kind` of `secs` (after the
/// physiology). Sleep only checks godmode in tg, which is not here.
// shortcut: no godmode (the dev toggle lives in `ss13_debug`) and no `status_flags`
// (`CANSTUN`, `CANKNOCKDOWN`, `CANUNCONSCIOUS`): every living mob can be stunned. Deviation: tg
// also refuses `Set*(0)`/`Adjust*(-x)` to an immune mob (the check runs before the amount);
// here removing an effect always works.
fn stun_blocked(world: &mut World, mob: Entity, kind: Incap, secs: f32) -> bool {
    if secs <= 0.0 || kind == Incap::Sleeping {
        return false;
    }
    let absorbing = world.get::<StunAbsorption>(mob).is_some();
    // The status signal: every incapacitating effect but unconsciousness.
    if absorbing && kind != Incap::Unconscious && absorb_stun(world, mob, secs) {
        return true;
    }
    // `COMSIG_LIVING_GENERIC_STUN_CHECK`: absorbed as 0 seconds.
    if absorbing && absorb_stun(world, mob, 0.0) {
        return true;
    }
    world.get::<StunImmunity>(mob).is_some()
}

/// The absorptions' `duration` and `recharge_absorption` timers. Cost: per tick, one pass over
/// the few mobs with an absorption.
fn stun_absorption_upkeep(
    time: Res<Time>,
    mut mobs: Query<(Entity, &mut StunAbsorption)>,
    mut commands: Commands,
) {
    let now = time.elapsed_secs_f64();
    for (mob, mut all) in &mut mobs {
        let expired = all.0.iter().any(|a| a.until.is_some_and(|t| now >= t))
            || all.0.iter().any(|a| a.recharges.iter().any(|r| now >= r.0));
        if !expired && !all.0.is_empty() {
            continue;
        }
        all.0.retain(|a| a.until.is_none_or(|t| now < t));
        for absorber in &mut all.0 {
            let given: f32 = absorber
                .recharges
                .iter()
                .filter(|r| now >= r.0)
                .map(|r| r.1)
                .sum();
            absorber.recharges.retain(|r| now < r.0);
            absorber.absorbed = (absorber.absorbed - given).max(0.0);
        }
        if all.0.is_empty() {
            commands.entity(mob).remove::<StunAbsorption>();
        }
    }
}

/// tg `Stun`/`Paralyze`/`Immobilize`/`Knockdown`/`incapacitate`/`Unconscious`/`Sleeping(amount)`
/// on a living mob.
pub fn apply(world: &mut World, mob: Entity, kind: Incap, secs: f32) -> bool {
    // `PHYS_COEFF_STUN` (spliced nerves); `Sleeping` does not use it.
    let secs = if kind == Incap::Sleeping {
        secs
    } else {
        crate::antag::zombie::spec_stun(world, mob, secs)
            * crate::medical::bioware::stun_mod(world, mob)
    };
    if stun_blocked(world, mob, kind, secs) {
        return world
            .get::<StatusEffects>(mob)
            .is_some_and(|e| e.remaining(kind) > 0.0);
    }
    effects_mut(world, mob).is_some_and(|mut e| e.apply(kind, secs))
}

/// tg `Set*(amount)` on a living mob (0 removes the effect).
pub fn set(world: &mut World, mob: Entity, kind: Incap, secs: f32) {
    if secs <= 0.0 && world.get::<StatusEffects>(mob).is_none() {
        return;
    }
    if stun_blocked(world, mob, kind, secs) {
        return;
    }
    if let Some(mut e) = effects_mut(world, mob) {
        e.set(kind, secs);
    }
}

/// tg `Adjust*(amount)` on a living mob.
pub fn adjust(world: &mut World, mob: Entity, kind: Incap, secs: f32) {
    if secs <= 0.0 && world.get::<StatusEffects>(mob).is_none() {
        return;
    }
    if stun_blocked(world, mob, kind, secs) {
        return;
    }
    if let Some(mut e) = effects_mut(world, mob) {
        e.adjust(kind, secs);
    }
}

/// tg `adjust_timed_status_effect(secs, effect, max)` on a living mob: `adjust_jitter`,
/// `adjust_dizzy`, `adjust_drowsiness`, `adjust_stutter`, `adjust_slurring`, `adjust_silence`,
/// `adjust_confusion`, `adjust_pacifism` and their `_up_to` forms. A dead mob gets no new
/// jitter, dizziness or silence (their `on_apply` and death signal).
pub fn adjust_timed(world: &mut World, mob: Entity, kind: Timed, secs: f32, max: Option<f32>) {
    if secs <= 0.0 && world.get::<StatusEffects>(mob).is_none() {
        return;
    }
    if secs > 0.0
        && is_dead(world, mob)
        && matches!(kind, Timed::Jitter | Timed::Dizziness | Timed::Silence)
    {
        dead_jitter(world, mob, kind, max.map_or(secs, |m| secs.min(m)));
        return;
    }
    if let Some(mut e) = effects_mut(world, mob) {
        e.adjust_timed(kind, secs, max);
    }
}

/// tg `set_timed_status_effect(secs, effect, only_if_higher)` on a living mob: `set_jitter`,
/// `set_dizzy_if_lower`, `set_stutter_if_lower`, `set_silence_if_lower` and the like.
pub fn set_timed(world: &mut World, mob: Entity, kind: Timed, secs: f32, only_if_higher: bool) {
    if secs <= 0.0 && world.get::<StatusEffects>(mob).is_none() {
        return;
    }
    if secs > 0.0
        && is_dead(world, mob)
        && matches!(kind, Timed::Jitter | Timed::Dizziness | Timed::Silence)
    {
        dead_jitter(world, mob, kind, secs);
        return;
    }
    if let Some(mut e) = effects_mut(world, mob) {
        e.set_timed(kind, secs, only_if_higher);
    }
}

/// `jitter/on_apply` on a dead mob: no effect, one shake of `secs`.
fn dead_jitter(world: &mut World, mob: Entity, kind: Timed, secs: f32) {
    if kind == Timed::Jitter {
        crate::animation::do_jitter_animation(world, mob, secs);
    }
}

/// Seconds left of a timed effect on `mob` (0 without it).
pub fn timed(world: &World, mob: Entity, kind: Timed) -> f32 {
    world
        .get::<StatusEffects>(mob)
        .map_or(0.0, |e| e.timed(kind))
}

/// tg `IS_UNCONSCIOUS`: hard crit, death, passout oxyloss, or knocked out by `Unconscious` or
/// `Sleeping`.
pub fn knocked_out(world: &World, mob: Entity) -> bool {
    let by_status = world
        .get::<StatusEffects>(mob)
        .is_some_and(StatusEffects::knocked_out);
    let by_health = match (
        world.get::<crate::health::Health>(mob),
        world.get::<Stat>(mob),
    ) {
        (Some(health), Some(&stat)) => health.knocked_out(stat),
        _ => false,
    };
    by_status || by_health
}

/// tg `adjust_status_effects_on_shake_up` of `mob` (a help-shake or hug).
pub fn shake_up(world: &mut World, mob: Entity) {
    if let Some(mut e) = world.get_mut::<StatusEffects>(mob) {
        e.shake_up();
    }
}

/// tg `Knockdown(amount, daze_amount)`.
pub fn knockdown(world: &mut World, mob: Entity, secs: f32, daze: f32) {
    let secs = crate::antag::zombie::spec_stun(world, mob, secs)
        * crate::medical::bioware::stun_mod(world, mob);
    // tg returns before the daze too.
    if stun_blocked(world, mob, Incap::Knockdown, secs) {
        return;
    }
    let dead = is_dead(world, mob);
    if let Some(mut e) = effects_mut(world, mob) {
        e.knockdown(secs, daze, dead);
    }
}

/// tg `adjust_stamina_loss(amount)` with stamcrit's messages.
pub fn adjust_stamina(world: &mut World, mob: Entity, amount: f32) {
    // `GET_PHYSIOLOGY(src, STAMINA)` on damage (spliced nerves).
    let amount = if amount > 0.0 {
        amount
            * crate::medical::bioware::stamina_mod(world, mob)
            * crate::antag::zombie::stamina_mod(world, mob)
    } else {
        amount
    };
    let now = world.resource::<Time>().elapsed_secs_f64();
    let dead = is_dead(world, mob);
    let conscious = world.get::<Stat>(mob) == Some(&Stat::Conscious);
    let Some(event) =
        effects_mut(world, mob).map(|mut e| e.adjust_stamina(amount, now, dead, false))
    else {
        return;
    };
    if event == StaminaEvent::EnteredCrit && conscious {
        tell_mob(
            world,
            mob,
            Span::Notice,
            "Вы слишком истощены, чтобы двигаться...",
        );
    }
}

fn tell_mob(world: &mut World, mob: Entity, span: Span, text: &str) {
    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
        world.write_message(SendChat {
            clients: vec![client],
            message: ChatMessage {
                channel: ChatChannel::Info,
                segments: vec![ChatSegment::new(span, text)],
            },
        });
    }
}

/// Status effect countdown and the traits; gameplay that applies effects runs before it, the
/// body (`crate::health`) after.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StatusSystems;

pub struct StatusEffectsPlugin;

impl Plugin for StatusEffectsPlugin {
    fn build(&self, app: &mut App) {
        crate::freon::plugin(app);
        app.add_systems(
            Update,
            (
                stun_absorption_upkeep,
                tick_effects,
                pulse_effects,
                refresh_incapacitation,
            )
                .chain()
                .in_set(StatusSystems)
                .before(HealthSystems),
        );
    }
}

/// Cost: per tick, one pass over mobs with active effects only.
fn tick_effects(
    time: Res<Time>,
    mut mobs: Query<(
        Entity,
        &mut StatusEffects,
        Option<&BodyPosition>,
        Option<&Stat>,
    )>,
    mut commands: Commands,
) {
    let (secs, now) = (time.delta_secs(), time.elapsed_secs_f64());
    for (mob, mut effects, body, stat) in &mut mobs {
        let lying = body.is_some_and(|b| b.lying());
        let dead = stat == Some(&Stat::Dead);
        if !effects.tick(secs, now, lying, dead) {
            commands.entity(mob).remove::<StatusEffects>();
        }
    }
}

/// `getviewsize(client.view)[1]` of the default 15x15 view: dizziness's offset bound (tg clamps
/// pixels by the view's width in tiles).
const DIZZY_VIEW_RANGE: f32 = 15.0;
/// `HEALING_SLEEP_DEFAULT`.
const HEALING_SLEEP_DEFAULT: f32 = 0.2;
/// `TIPSY_THRESHOLD`: the drunk stage from here.
const TIPSY_THRESHOLD: f32 = 6.0;

/// The 2 s pulse of the mobs whose pulse is due, then the speech effects and pacifism follow
/// the effects. Cost: per tick one pass over mobs with active effects; the pulse work only
/// every 2 s for each.
fn pulse_effects(world: &mut World) {
    let mut query = world.query::<(Entity, &StatusEffects)>();
    let due: Vec<Entity> = query
        .iter(world)
        .filter(|(_, e)| e.pulse_due)
        .map(|(e, _)| e)
        .collect();
    for mob in due {
        if let Some(mut e) = world.get_mut::<StatusEffects>(mob) {
            e.pulse_due = false;
        }
        pulse(world, mob);
    }
    sync_status_traits(world);
}

fn pulse(world: &mut World, mob: Entity) {
    let Some(effects) = world.get::<StatusEffects>(mob).cloned() else {
        return;
    };
    let dead = is_dead(world, mob);
    let resting = world
        .get::<crate::rest::Rest>(mob)
        .is_some_and(|r| r.resting);
    // `jitter/tick`: resting takes 4 more ticks off, then the sprite shakes by the time left.
    if effects.jitter > 0.0 {
        if resting {
            adjust_timed(world, mob, Timed::Jitter, -4.0 * PULSE_SECS, None);
        }
        let left = timed(world, mob, Timed::Jitter);
        if left > 0.0 {
            crate::animation::do_jitter_animation(world, mob, left);
        }
    }
    if effects.dizziness > 0.0 {
        dizzy_pulse(world, mob, effects.dizziness, resting);
    }
    // `drowsiness/tick`.
    if effects.drowsiness > 0.0 && !knocked_out(world, mob) {
        let mut drowsy = true;
        if resting {
            adjust_timed(world, mob, Timed::Drowsiness, -2.0 * PULSE_SECS, None);
            drowsy = timed(world, mob, Timed::Drowsiness) > 0.0;
        }
        if drowsy {
            crate::vision::set_eye_blur_if_lower(world, mob, 4.0);
            if world.get_resource_or_init::<CombatRng>().chance(5.0) {
                adjust(world, mob, Incap::Sleeping, 10.0);
            }
        }
    }
    if effects.sleeping > 0.0 && !dead {
        sleep_pulse(world, mob);
    }
}

/// `dizziness/tick`'s first view offset, pixels: `amount` seconds left, `time_ds` `world.time`.
/// DM's `sin`/`cos` take degrees.
pub fn dizzy_offset(amount: f32, time_ds: f64) -> [i16; 2] {
    let angle = (f64::from(amount) * time_ds).rem_euclid(360.0).to_radians();
    let amplitude = f64::from(amount) * (angle.sin() + 1.0);
    let range = f64::from(DIZZY_VIEW_RANGE);
    let x = (amplitude * angle.sin()).clamp(-range, range);
    let y = (amplitude * angle.cos()).clamp(-range, range);
    [x.round() as i16, y.round() as i16]
}

/// `dizziness/tick`: resting makes it 5 times as strong and 5 times as short; the player's
/// view jumps away and back.
// shortcut: tg jumps twice (0.3 s each, the second by the next tick's offset) and back; here one
// `CameraEffect::Recoil` out over 0.3 s and back over 0.6 s.
fn dizzy_pulse(world: &mut World, mob: Entity, left: f32, resting: bool) {
    let strength = if resting { 5.0 } else { 1.0 };
    if strength > 1.0 {
        adjust_timed(
            world,
            mob,
            Timed::Dizziness,
            -(strength - 1.0) * PULSE_SECS,
            None,
        );
        if timed(world, mob, Timed::Dizziness) <= 0.0 {
            return;
        }
    }
    let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) else {
        return;
    };
    let time_ds = world.resource::<Time>().elapsed_secs_f64() * 10.0;
    let offset = dizzy_offset(left, time_ds);
    if offset == [0, 0] {
        return;
    }
    world.write_message(SendCameraEffect {
        client,
        effect: CameraEffect::Recoil {
            offset,
            out_ds: 3.0,
            back_ds: 6.0,
        },
    });
}

/// `sleeping/tick`'s sleep quality: the mood's sanity level (`None`: no mood), then +0.2 on a
/// bed or +0.1 on a table, +0.1 each for a bedsheet, a pillow and deafness.
pub fn sleep_quality(sanity_level: Option<u8>, tile: &[String], deaf: bool) -> f32 {
    use crate::mood::{
        SANITY_LEVEL_CRAZY, SANITY_LEVEL_DISTURBED, SANITY_LEVEL_GREAT, SANITY_LEVEL_INSANE,
        SANITY_LEVEL_NEUTRAL, SANITY_LEVEL_UNSTABLE,
    };
    use crate::obj_damage::is_under;
    let mut quality = match sanity_level {
        Some(SANITY_LEVEL_GREAT) => 0.2,
        Some(SANITY_LEVEL_NEUTRAL) => 0.1,
        Some(SANITY_LEVEL_DISTURBED | SANITY_LEVEL_UNSTABLE) => 0.0,
        Some(SANITY_LEVEL_CRAZY) => -0.1,
        Some(SANITY_LEVEL_INSANE) => -0.2,
        _ => HEALING_SLEEP_DEFAULT,
    };
    if deaf {
        quality += 0.1;
    }
    let on = |root: &str| tile.iter().any(|p| is_under(p, root));
    if on("/obj/structure/bed") {
        quality += 0.2;
    } else if on("/obj/structure/table") {
        quality += 0.1;
    }
    if on("/obj/item/bedsheet") {
        quality += 0.1;
    }
    if on("/obj/item/pillow") {
        quality += 0.1;
    }
    quality
}

/// The type paths on `mob`'s tile (`locate(/obj/structure/bed) in loc` and the like).
pub fn tile_paths(world: &World, mob: Entity) -> Vec<String> {
    world
        .get::<ss13_shared::TilePos>(mob)
        .map_or_else(Vec::new, |&pos| {
            world
                .resource::<ss13_world::SpatialIndex>()
                .on_tile(pos)
                .iter()
                .filter_map(|&e| crate::construction::path_of(world, e))
                .collect()
        })
}

/// `sleeping/tick`: minor damage heals by the sleep quality (above 80% health), stamina comes
/// back, drunkenness drops by 0.3%.
fn sleep_pulse(world: &mut World, mob: Entity) {
    let secs = PULSE_SECS;
    let tile = tile_paths(world, mob);
    let sanity = world.get::<crate::mood::Mood>(mob).map(|m| m.sanity_level);
    let quality = sleep_quality(sanity, &tile, !crate::sound::can_hear(world, mob));
    // `exercised`: a carbon's good sleep turns the workout into athletics.
    if world.get::<crate::inventory::Inventory>(mob).is_some() {
        crate::buffs::sleep_workout(world, mob, secs, quality);
    }
    let ratio = world
        .get::<Health>(mob)
        .filter(|h| h.max > 0.0)
        .map(|h| h.health() / h.max);
    if let Some(ratio) = ratio {
        if quality > 0.0 && ratio > 0.8 {
            use crate::medical::diseases::damage;
            damage(world, mob, DamageType::Brute, -0.4 * quality * secs);
            damage(world, mob, DamageType::Burn, -0.4 * quality * secs);
            damage(world, mob, DamageType::Tox, -0.2 * quality * secs);
        }
        let stamina = (-0.4 * quality * secs).min(-0.4 * HEALING_SLEEP_DEFAULT * secs);
        let now = world.resource::<Time>().elapsed_secs_f64();
        if let Some(mut e) = world.get_mut::<StatusEffects>(mob) {
            e.adjust_stamina(stamina, now, false, true);
        }
    }
    if let Some(mut reagents) = world.get_mut::<crate::chem::MobReagents>(mob)
        && reagents.drunk > 0.0
    {
        reagents.drunk *= 0.997;
    }
    // `handle_dreams` of a carbon.
    if world.contains_resource::<crate::dreams::DreamRng>()
        && world.get::<crate::inventory::Inventory>(mob).is_some()
        && world.get::<crate::silicon::Silicon>(mob).is_none()
        && world.get::<crate::mobs::BasicMob>(mob).is_none()
    {
        crate::dreams::handle_dreams(world, mob);
    }
}

/// `/datum/status_effect/inebriated/drunk/on_tick_effects` after the drunkenness decayed to
/// `drunk` (the toxin damage is `ss13_reagents::metabolism::inebriation_tick`).
// shortcut: no blackout split personality (the trauma is not ported): a blackout is always the
// 90 s sleep, also with the shuttle docked.
pub fn drunk_pulse(world: &mut World, mob: Entity, drunk: f32) {
    if drunk < TIPSY_THRESHOLD || is_dead(world, mob) {
        return;
    }
    if drunk >= 16.0 {
        adjust_timed(
            world,
            mob,
            Timed::Slurring(SlurKind::Drunk),
            4.0,
            Some(20.0),
        );
    }
    adjust_timed(world, mob, Timed::Jitter, -6.0, None);
    let roll = |world: &mut World, p: f32| world.get_resource_or_init::<CombatRng>().chance(p);
    if drunk >= 41.0 {
        if roll(world, 30.0) {
            adjust_timed(world, mob, Timed::Confusion, 2.0, None);
        }
        set_timed(world, mob, Timed::Dizziness, 20.0, true);
    }
    if drunk >= 51.0 {
        set_timed(world, mob, Timed::Dizziness, 50.0, true);
        if roll(world, 3.0) {
            adjust_timed(world, mob, Timed::Confusion, 15.0, None);
            crate::medical::diseases::vomit(world, mob, 10.0, true, false, 1);
        }
    }
    if drunk >= 71.0 {
        crate::vision::set_eye_blur_if_lower(world, mob, drunk * 2.0 - 140.0);
    }
    let awake = |world: &World| {
        world.get::<Stat>(mob) == Some(&Stat::Conscious) && !knocked_out(world, mob)
    };
    if drunk >= 81.0 && awake(&*world) && roll(world, 5.0) {
        tell_mob(
            world,
            mob,
            Span::Warning,
            "Может, вам стоит немного полежать...",
        );
    }
    if drunk >= 91.0 {
        crate::medical::organs::adjust_organ_loss(
            world,
            mob,
            crate::medical::organs::Slot::Brain,
            0.4,
        );
        if awake(&*world) {
            apply(world, mob, Incap::Sleeping, 90.0);
        }
    }
}

/// The examine lines of `mob`'s status effects (`get_examine_text`): jitter and drunkenness.
// shortcut: a covered face does not hide the drunkenness (no `obscured_slots`).
pub fn examine_lines(world: &World, mob: Entity, lines: &mut Vec<(Span, String)>) {
    // `fire_handler/get_examine_text` (fire and wet stacks, `crate::fire`).
    if let Some(text) = crate::fire::examine_text(world, mob) {
        lines.push((Span::Notice, text.to_owned()));
    }
    // Choking, slime, the slit throat (`crate::afflictions`).
    crate::afflictions::examine_lines(world, mob, lines);
    // Death ends the jitter; the dead do not look drunk (`IS_DEAD_OR_FAKING`).
    if world.get::<Stat>(mob) == Some(&Stat::Dead) {
        return;
    }
    if let Some((span, text)) = world
        .get::<StatusEffects>(mob)
        .and_then(StatusEffects::jitter_examine)
    {
        lines.push((span, format!("Он {text}")));
    }
    let drunk = world
        .get::<crate::chem::MobReagents>(mob)
        .map_or(0.0, |r| r.drunk);
    if let Some(text) = drunk_examine(drunk) {
        lines.push((Span::Warning, format!("Он {text}")));
    }
    // `stun_absorption/get_examine_text`: while it can absorb.
    if let Some(all) = world.get::<StunAbsorption>(mob)
        && can_absorb(world, mob)
    {
        for absorber in all.0.iter().filter(|a| a.charged()) {
            if let Some(text) = &absorber.examine_message {
                lines.push((Span::Warning, text.replace("%EFFECT_OWNER_THEYRE", "Он")));
            }
        }
    }
}

/// `/datum/status_effect/inebriated/get_examine_text`.
pub fn drunk_examine(drunk: f32) -> Option<&'static str> {
    Some(if drunk > 91.0 {
        "— пьяная в стельку слюнявая развалина."
    } else if drunk > 61.0 {
        "выглядит как пьяное месиво."
    } else if drunk > 51.0 {
        "сильно раскраснелся, его движения резкие, а дыхание разит алкоголем."
    } else if drunk > 41.0 {
        "довольно раскраснелся, и от него пахнет алкоголем."
    } else if drunk > 21.0 {
        "раскраснелся."
    } else if drunk >= 11.0 {
        "слегка раскраснелся."
    } else {
        return None;
    })
}

/// The chat's [`SpeechEffects`] and the pacifism follow the effects.
fn sync_status_traits(world: &mut World) {
    let mut query = world.query::<(
        Entity,
        &StatusEffects,
        Option<&crate::chem::MobReagents>,
        Option<&SpeechEffects>,
    )>();
    let mut speech = Vec::new();
    let mut pacify = Vec::new();
    for (mob, effects, reagents, current) in query.iter(world) {
        let wanted = effects.speech(reagents.map_or(0.0, |r| r.drunk));
        let wanted = (wanted != SpeechEffects::default()).then_some(wanted);
        if current.copied() != wanted {
            speech.push((mob, wanted));
        }
        if effects.pacified() != effects.pacified_seen {
            pacify.push((mob, effects.pacified()));
        }
    }
    let mut stale = world.query_filtered::<Entity, (With<SpeechEffects>, Without<StatusEffects>)>();
    let stale: Vec<Entity> = stale.iter(world).collect();
    for mob in stale {
        world.entity_mut(mob).remove::<SpeechEffects>();
    }
    for (mob, wanted) in speech {
        match wanted {
            Some(s) => {
                world.entity_mut(mob).insert(s);
            }
            None => {
                world.entity_mut(mob).remove::<SpeechEffects>();
            }
        }
    }
    for (mob, pacified) in pacify {
        if let Some(mut e) = world.get_mut::<StatusEffects>(mob) {
            e.pacified_seen = pacified;
        }
        crate::medical::traumas::refresh(world, mob);
    }
}

fn refresh_incapacitation(
    changed: Query<
        Entity,
        Or<(
            Changed<StatusEffects>,
            Changed<Handcuffed>,
            Changed<crate::restraints::SuitRestrained>,
            Changed<crate::medical::LimbLimits>,
            Changed<crate::rest::Rest>,
            Added<crate::silicon::chassis::LockedDown>,
            Added<crate::medical::stasis::InStasis>,
            Changed<crate::pull::Chokehold>,
            Changed<crate::pull::carry::Carried>,
            Added<crate::freon::Frozen>,
        )>,
    >,
    mut removed_effects: RemovedComponents<StatusEffects>,
    mut removed_cuffs: RemovedComponents<Handcuffed>,
    mut removed_jackets: RemovedComponents<crate::restraints::SuitRestrained>,
    mut removed_locks: RemovedComponents<crate::silicon::chassis::LockedDown>,
    mut removed_stasis: RemovedComponents<crate::medical::stasis::InStasis>,
    (mut removed_chokes, mut removed_carries, mut removed_cubes): (
        RemovedComponents<crate::pull::Chokehold>,
        RemovedComponents<crate::pull::carry::Carried>,
        RemovedComponents<crate::freon::Frozen>,
    ),
    mut mobs: Query<(
        Option<&StatusEffects>,
        (Has<Handcuffed>, Has<crate::restraints::SuitRestrained>),
        Option<&crate::medical::LimbLimits>,
        Option<&crate::rest::Rest>,
        Has<crate::silicon::chassis::LockedDown>,
        Has<crate::medical::stasis::InStasis>,
        (
            Option<&crate::pull::Chokehold>,
            Option<&crate::pull::carry::Carried>,
            Has<crate::freon::Frozen>,
        ),
        &mut Incapacitation,
    )>,
) {
    let removed: Vec<Entity> = removed_effects
        .read()
        .chain(removed_cuffs.read())
        .chain(removed_jackets.read())
        .chain(removed_locks.read())
        .chain(removed_stasis.read())
        .chain(removed_chokes.read())
        .chain(removed_carries.read())
        .chain(removed_cubes.read())
        .collect();
    for mob in changed.iter().chain(removed) {
        if let Ok((
            effects,
            (cuffs, jacket),
            limbs,
            rest,
            locked,
            stasis,
            (choke, carried, frozen),
            mut traits,
        )) = mobs.get_mut(mob)
        {
            let cuffed = cuffs || jacket;
            // Stasis (`TRAIT_IMMOBILIZED`, `TRAIT_HANDS_BLOCKED`) blocks what a lockdown does.
            let locked = locked || stasis;
            let held = Held {
                grab: choke.map(|c| c.0),
                carried: carried.map(|c| c.fireman),
                frozen,
            };
            traits.set_if_neq(Incapacitation::of(
                effects, cuffed, limbs, rest, locked, held,
            ));
        }
    }
}

/// tg `/datum/status_effect/confusion/on_move` for a step about to be requested.
pub fn confused_step(effects: Option<&StatusEffects>, rng: &mut CombatRng, dir: Dir) -> Dir {
    match effects {
        Some(e) if e.confusion > 0.0 => confused_dir(e.confusion, dir, |n| rng.range(0, n - 1)),
        _ => dir,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stun_takes_the_longer_set_replaces_adjust_adds() {
        let mut e = StatusEffects::default();
        assert!(e.apply(Incap::Stun, 5.0));
        assert!(e.apply(Incap::Stun, 2.0));
        assert_eq!(e.stun, 5.0, "a shorter stun does not shorten it");
        e.apply(Incap::Stun, 8.0);
        assert_eq!(e.stun, 8.0);
        e.set(Incap::Stun, 1.0);
        assert_eq!(e.stun, 1.0);
        e.adjust(Incap::Stun, 2.5);
        assert_eq!(e.stun, 3.5);
        e.adjust(Incap::Stun, -10.0);
        assert_eq!(e.stun, 0.0);
        assert!(!e.apply(Incap::Paralyze, 0.0), "0 does not start an effect");
    }

    #[test]
    fn effects_count_down_and_give_tg_traits() {
        let mut e = StatusEffects::default();
        e.apply(Incap::Knockdown, 1.0);
        assert!(e.floored() && !e.immobilized() && !e.incapacitated() && !e.hands_blocked());
        e.apply(Incap::Immobilize, 0.5);
        assert!(e.immobilized() && !e.incapacitated());
        e.apply(Incap::Stun, 0.3);
        assert!(e.incapacitated() && e.hands_blocked() && e.immobilized());
        e.apply(Incap::Paralyze, 0.2);
        assert!(e.floored() && e.hands_blocked());
        assert!(e.tick(0.25, 0.25, true, false));
        assert_eq!(e.paralyze, 0.0);
        assert!(e.stun > 0.0);
        assert!(e.tick(0.25, 0.5, true, false));
        assert!(!e.incapacitated() && !e.immobilized() && e.floored());
        assert!(!e.tick(0.5, 1.0, true, false), "everything ran out");
        assert!(!e.floored());
    }

    #[test]
    fn stamina_crit_at_100_with_tg_push_and_full_reset_after_regen_time() {
        let mut e = StatusEffects::default();
        assert_eq!(
            e.adjust_stamina(99.0, 0.0, false, false),
            StaminaEvent::None
        );
        assert!(!e.stamcrit && !e.incapacitated());
        assert_eq!(
            e.adjust_stamina(1.0, 1.0, false, false),
            StaminaEvent::EnteredCrit
        );
        assert!(e.stamcrit && e.incapacitated() && e.immobilized() && e.floored());
        assert!(!e.hands_blocked(), "stamcrit does not block hands");
        assert_eq!(e.stamina, 120.0, "100 + 30, capped at max_stamina");
        // The reset timer restarts on every hit: 10 s after the last one.
        e.tick(9.0, 10.0, true, false);
        assert!(e.stamcrit);
        e.tick(1.0, 11.0, true, false);
        assert_eq!(e.stamina, 0.0);
        assert!(!e.stamcrit);
        assert!(!e.tick(0.0, 11.0, false, false), "nothing left");
    }

    #[test]
    fn stamina_reset_timer_restarts_on_damage() {
        let mut e = StatusEffects::default();
        e.adjust_stamina(60.0, 0.0, false, false);
        e.tick(8.0, 8.0, false, false);
        assert_eq!(e.stamina, 60.0);
        e.adjust_stamina(10.0, 8.0, false, false);
        e.tick(3.0, 11.0, false, false);
        assert_eq!(e.stamina, 70.0, "the 10 s count from the newest damage");
        e.tick(7.0, 18.0, false, false);
        assert_eq!(e.stamina, 0.0);
        // Healing does not restart it.
        e.adjust_stamina(50.0, 20.0, false, false);
        e.adjust_stamina(-10.0, 25.0, false, false);
        e.tick(5.0, 30.0, false, false);
        assert_eq!(e.stamina, 0.0);
    }

    #[test]
    fn stamcrit_diminishing_returns_and_the_dead() {
        let mut e = StatusEffects::default();
        e.adjust_stamina(100.0, 0.0, false, false);
        assert!(e.stamcrit);
        // ceil(sqrt(5) / 2) = 2, minus the counter (+1 per hit of 5 or more).
        assert_eq!(e.adjust_stamina(5.0, 0.0, false, false), StaminaEvent::None);
        assert_eq!(e.adjust_stamina(5.0, 0.0, false, false), StaminaEvent::None);
        assert_eq!(
            e.adjust_stamina(5.0, 0.0, false, false),
            StaminaEvent::Ignored
        );
        assert_eq!(
            e.adjust_stamina(1.0, 0.0, false, false),
            StaminaEvent::Ignored
        );
        assert!(e.stamcrit);
        let mut dead = StatusEffects::default();
        assert_eq!(
            dead.adjust_stamina(110.0, 0.0, true, false),
            StaminaEvent::None
        );
        assert!(!dead.stamcrit, "the dead do not stamcrit");
    }

    #[test]
    fn daze_follows_knockdown_and_ends_on_stun_or_standing() {
        let mut e = StatusEffects::default();
        e.knockdown(2.0, DAZED_SECS, false);
        assert_eq!(e.dazed, 3.0);
        e.knockdown(2.0, 1.0, false);
        assert_eq!(e.dazed, 3.0, "unique: kept");
        e.apply(Incap::Stun, 1.0);
        assert_eq!(e.dazed, 0.0);
        let mut e = StatusEffects::default();
        e.knockdown(1.0, DAZED_SECS, false);
        e.tick(1.0, 1.0, false, false);
        assert_eq!(e.dazed, 0.0, "standing up ends it");
        let mut e = StatusEffects {
            no_side_kick: 1.0,
            ..default()
        };
        e.knockdown(1.0, DAZED_SECS, false);
        assert_eq!(e.dazed, 0.0);
    }

    #[test]
    fn timed_effects_add_up_to_their_cap() {
        let mut e = StatusEffects::default();
        e.adjust_confusion_up_to(5.0, 10.0);
        e.adjust_confusion_up_to(5.0, 10.0);
        e.adjust_confusion_up_to(5.0, 10.0);
        assert_eq!(e.confusion, 10.0);
        e.set_confusion_if_lower(4.0);
        assert_eq!(e.confusion, 10.0);
        e.set_confusion_if_lower(12.0);
        assert_eq!(e.confusion, 12.0);
        e.adjust_staggered_up_to(3.0, 10.0, true);
        assert_eq!(e.staggered, 0.0, "the dead do not stagger");
    }

    #[test]
    fn confusion_turns_steps_by_tg_odds() {
        // Below the full threshold: sideways at 1.5 %/s, diagonal at 3 %/s of what is left
        // (5 s: 7.5 %, then 15 %). `rolls` answers the rolls in order.
        let rolls = |seq: Vec<u32>| {
            let mut seq = seq.into_iter();
            move |_| seq.next().unwrap()
        };
        assert_eq!(
            confused_dir(5.0, Dir::North, rolls(vec![9999, 9999])),
            Dir::North
        );
        assert_eq!(
            confused_dir(5.0, Dir::North, rolls(vec![749, 0])),
            Dir::East
        );
        assert_eq!(
            confused_dir(5.0, Dir::North, rolls(vec![749, 1])),
            Dir::West
        );
        assert_eq!(
            confused_dir(5.0, Dir::North, rolls(vec![750, 1499, 0])),
            Dir::NorthEast
        );
        assert_eq!(
            confused_dir(5.0, Dir::West, rolls(vec![750, 1499, 0])),
            Dir::NorthWest
        );
        assert_eq!(
            confused_dir(41.0, Dir::West, rolls(vec![3])),
            Dir::SouthEast
        );
    }

    #[test]
    fn unconsciousness_and_sleep_knock_out_and_shaking_shortens_them() {
        let mut e = StatusEffects::default();
        e.apply(Incap::Incapacitated, 3.0);
        assert!(e.incapacitated() && !e.immobilized() && !e.floored() && !e.hands_blocked());
        e.apply(Incap::Sleeping, 20.0);
        assert!(e.knocked_out() && e.immobilized() && e.floored() && e.hands_blocked());
        e.apply(Incap::Unconscious, 4.0);
        e.shake_up();
        assert_eq!(e.sleeping, 10.0, "a shake takes 10 s of sleep");
        assert_eq!(e.unconscious, 0.0, "and 6 s of unconsciousness");
        // Knocked out: 3 stamina a second back.
        e.adjust_stamina(50.0, 0.0, false, false);
        e.tick(2.0, 2.0, true, false);
        assert_eq!(e.stamina, 44.0);
        e.tick(8.0, 10.0, true, false);
        assert!(!e.knocked_out());
    }

    #[test]
    fn timed_effects_cap_and_death_ends_jitter_dizziness_and_silence() {
        let mut e = StatusEffects::default();
        e.adjust_timed(Timed::Slurring(SlurKind::Drunk), 4.0, Some(20.0));
        for _ in 0..10 {
            e.adjust_timed(Timed::Slurring(SlurKind::Drunk), 4.0, Some(20.0));
        }
        assert_eq!(e.timed(Timed::Slurring(SlurKind::Drunk)), 20.0);
        assert_eq!(e.timed(Timed::Slurring(SlurKind::Cult)), 0.0);
        e.set_timed(Timed::Dizziness, 20.0, true);
        e.set_timed(Timed::Dizziness, 10.0, true);
        assert_eq!(e.dizziness, 20.0, "if lower: kept");
        e.adjust_timed(Timed::Jitter, 30.0, None);
        e.adjust_timed(Timed::Jitter, -6.0, None);
        assert_eq!(e.jitter, 24.0);
        e.adjust_timed(Timed::Silence, 5.0, None);
        e.adjust_timed(Timed::Stutter, 5.0, None);
        e.tick(0.0, 0.0, false, true);
        assert_eq!((e.jitter, e.dizziness, e.silence), (0.0, 0.0, 0.0));
        assert_eq!(e.stutter, 5.0, "the stutter outlives its owner");
    }

    #[test]
    fn speech_effects_follow_the_timers() {
        let mut e = StatusEffects::default();
        assert_eq!(e.speech(0.0), SpeechEffects::default());
        e.adjust_timed(Timed::Stutter, 5.0, None);
        e.adjust_timed(Timed::Slurring(SlurKind::Drunk), 4.0, None);
        e.adjust_timed(Timed::Silence, 1.0, None);
        let speech = e.speech(60.0);
        assert!(speech.stutter && speech.silenced);
        assert_eq!(speech.slurs[1], Some(Slur::drunk(60.0)));
        assert_eq!(speech.slurs[0], None);
        // At 60 drunkenness: common 32, uncommon 8, replacement 12 (capped), double 40.
        let drunk = Slur::drunk(60.0);
        assert_eq!(
            (
                drunk.common,
                drunk.uncommon,
                drunk.replacement,
                drunk.doubletext
            ),
            (32.0, 8.0, 12.0, 40.0)
        );
    }

    #[test]
    fn the_pulse_comes_every_two_seconds_while_something_pulses() {
        let mut e = StatusEffects::default();
        e.adjust_timed(Timed::Stutter, 10.0, None);
        e.tick(3.0, 3.0, false, false);
        assert!(!e.pulse_due, "a stutter has no tick");
        e.adjust_timed(Timed::Jitter, 10.0, None);
        e.tick(1.5, 4.5, false, false);
        assert!(!e.pulse_due);
        e.tick(0.5, 5.0, false, false);
        assert!(e.pulse_due);
    }

    #[test]
    fn dizziness_offset_and_sleep_quality_follow_tg() {
        // amount 10 at world.time 9: angle 90 degrees, amplitude 20, clamped to 15.
        assert_eq!(dizzy_offset(10.0, 9.0), [15, 0]);
        // Angle 270: amplitude 0.
        assert_eq!(dizzy_offset(10.0, 27.0), [0, 0]);
        let bed = vec!["/obj/structure/bed/medical".to_owned()];
        let pillow = vec![
            "/obj/structure/table".to_owned(),
            "/obj/item/pillow".to_owned(),
        ];
        use crate::mood::{SANITY_LEVEL_GREAT, SANITY_LEVEL_INSANE};
        assert!((sleep_quality(Some(SANITY_LEVEL_GREAT), &bed, false) - 0.4).abs() < 1e-6);
        assert!((sleep_quality(None, &pillow, true) - 0.5).abs() < 1e-6);
        assert!(sleep_quality(Some(SANITY_LEVEL_INSANE), &[], false) < 0.0);
    }

    #[test]
    fn jitter_and_drunk_examine_thresholds() {
        let e = StatusEffects {
            jitter: 59.0,
            ..default()
        };
        assert_eq!(e.jitter_examine(), None);
        let e = StatusEffects {
            jitter: 200.0,
            ..default()
        };
        assert_eq!(e.jitter_examine().map(|(s, _)| s), Some(Span::Warning));
        assert_eq!(drunk_examine(10.0), None);
        assert_eq!(drunk_examine(11.0), Some("слегка раскраснелся."));
        assert_eq!(
            drunk_examine(95.0),
            Some("— пьяная в стельку слюнявая развалина.")
        );
    }

    #[test]
    fn stun_immunity_and_absorption_keep_new_stuns_from_holding() {
        let mut world = World::new();
        world.init_resource::<Time>();
        let mob = world.spawn(Stat::Conscious).id();
        add_stun_immunity(&mut world, mob, "bath_salts");
        add_stun_immunity(&mut world, mob, "bath_salts");
        assert!(!apply(&mut world, mob, Incap::Stun, 5.0));
        knockdown(&mut world, mob, 5.0, 2.0);
        adjust(&mut world, mob, Incap::Paralyze, 5.0);
        set(&mut world, mob, Incap::Unconscious, 5.0);
        assert!(world.get::<StatusEffects>(mob).is_none_or(|e| e.idle()));
        // Sleep is not a stun.
        assert!(apply(&mut world, mob, Incap::Sleeping, 5.0));
        remove_stun_immunity(&mut world, mob, "bath_salts");
        assert!(world.get::<StunImmunity>(mob).is_none());
        assert!(apply(&mut world, mob, Incap::Stun, 5.0));

        // An absorption of 4 s: the stun that passes the cap is still absorbed, then it is gone.
        let other = world.spawn(Stat::Conscious).id();
        let capped = StunAbsorber {
            max_secs: 4.0,
            ..StunAbsorber::new("grace")
        };
        add_stun_absorption(&mut world, other, capped, None);
        assert!(!apply(&mut world, other, Incap::Knockdown, 3.0));
        assert_eq!(
            world.get::<StunAbsorption>(other).unwrap().0[0].absorbed,
            3.0
        );
        // Unconsciousness is blocked by the generic check, uncounted.
        set(&mut world, other, Incap::Unconscious, 10.0);
        assert_eq!(
            world.get::<StunAbsorption>(other).unwrap().0[0].absorbed,
            3.0
        );
        assert!(!apply(&mut world, other, Incap::Stun, 3.0));
        assert!(world.get::<StunAbsorption>(other).unwrap().0.is_empty());
        assert!(apply(&mut world, other, Incap::Stun, 3.0));
        // Removing stuns is never blocked.
        add_stun_absorption(&mut world, other, StunAbsorber::new("grace"), None);
        set(&mut world, other, Incap::Stun, 0.0);
        assert_eq!(world.get::<StatusEffects>(other).unwrap().stun, 0.0);

        // A higher priority decides; a lower one of the same source does not replace it.
        let third = world.spawn(Stat::Conscious).id();
        add_stun_absorption(&mut world, third, StunAbsorber::new("a").priority(3), None);
        add_stun_absorption(&mut world, third, StunAbsorber::new("a").priority(1), None);
        add_stun_absorption(&mut world, third, StunAbsorber::new("b").priority(2), None);
        assert!(!apply(&mut world, third, Incap::Stun, 2.0));
        let all = &world.get::<StunAbsorption>(third).unwrap().0;
        assert_eq!((all[0].priority, all[0].absorbed), (3, 2.0));
        assert_eq!(all[1].absorbed, 0.0);
        // The unconscious absorb nothing.
        world.entity_mut(third).insert(Stat::HardCrit);
        assert!(apply(&mut world, third, Incap::Stun, 2.0));
    }

    #[test]
    fn speech_effects_reach_the_chat_component_and_leave_with_the_effects() {
        let mut world = World::new();
        world.init_resource::<Time>();
        let mob = world.spawn(Stat::Conscious).id();
        set_timed(&mut world, mob, Timed::Silence, 5.0, true);
        adjust_timed(&mut world, mob, Timed::Slurring(SlurKind::Cult), 5.0, None);
        sync_status_traits(&mut world);
        let speech = *world.get::<SpeechEffects>(mob).unwrap();
        assert!(speech.silenced && !speech.stutter);
        assert_eq!(speech.slurs[2], Some(Slur::CULT));
        world.entity_mut(mob).remove::<StatusEffects>();
        sync_status_traits(&mut world);
        assert!(world.get::<SpeechEffects>(mob).is_none());
        // The dead get no new jitter.
        let corpse = world.spawn(Stat::Dead).id();
        adjust_timed(&mut world, corpse, Timed::Jitter, 10.0, None);
        assert_eq!(timed(&world, corpse, Timed::Jitter), 0.0);
        adjust_timed(&mut world, corpse, Timed::Stutter, 10.0, None);
        assert_eq!(timed(&world, corpse, Timed::Stutter), 10.0);
    }
}
