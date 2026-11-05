//! Status effects and stamina (server), after tg `code/datums/status_effects/debuffs/debuffs.dm`
//! (`/datum/status_effect/incapacitating/*`), `stamcrit.dm`, `confusion.dm`, `staggered.dm`,
//! `code/modules/mob/living/status_procs.dm` and `damage_procs.dm` (stamina).
//!
//! - [`StatusEffects`] holds the remaining seconds of tg's `Stun`, `Knockdown`, `Paralyze`,
//!   `Immobilize` (and confusion, dazed, staggered, no side kick), the mob's `staminaloss` and
//!   whether it is in stamina crit. `Stun(amount)` never shortens what is left
//!   (`max(amount, duration)`), `Set*` replaces it, `Adjust*` adds to it.
//! - Stamina: damage is clamped to `max_stamina` (120); any positive change restarts tg's
//!   `timed_stamina_reset`: `stamina_regen_time` (10 s) after the last stamina damage it is set
//!   back to 0 at once (there is no gradual regeneration in current tg). At `maxHealth -
//!   staminaloss <= crit_threshold` (100 stamina) a living mob enters stamcrit
//!   (incapacitated, immobilized, floored; +30 stamina below 120); further hits there have
//!   diminishing returns; it leaves stamcrit when stamina drops below 100.
//! - [`Incapacitation`] is tg's traits that follow: `TRAIT_INCAPACITATED` (stun, paralyze,
//!   stamcrit), `TRAIT_IMMOBILIZED` (+ immobilize), `TRAIT_FLOORED` (knockdown, paralyze,
//!   stamcrit), `TRAIT_HANDS_BLOCKED` (stun, paralyze, restraints), `TRAIT_RESTRAINED`
//!   (handcuffs, `crate::restraints`). Moves, clicks, hands, lying and resisting read it.
//!
//! The component exists only while something is active, so the per-tick countdown touches only
//! affected mobs.
// shortcut: no stun immunity (`check_stun_immunity`, `TRAIT_STUNIMMUNE`, godmode; the CNS
// rebooter implant clears stuns itself), physiology multipliers only from bioware
// (`crate::medical::bioware`, in [`apply`], [`knockdown`], [`adjust_stamina`]), no unconsciousness (`Unconscious`, sleeping), no force_say, jitter or
// stutter; add them with the effects that grant them.

use bevy::prelude::*;
use ss13_net::chat::server::SendChat;
use ss13_shared::Dir;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};

use crate::combat::CombatRng;
use crate::health::{BodyPosition, HealthSystems, MAX_HEALTH, Stat};
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

/// Remaining seconds of each effect (0 = none) and the stamina state.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct StatusEffects {
    pub stun: f32,
    pub knockdown: f32,
    pub paralyze: f32,
    pub immobilize: f32,
    pub confusion: f32,
    pub dazed: f32,
    pub staggered: f32,
    pub no_side_kick: f32,
    /// `staminaloss`.
    pub stamina: f32,
    /// `/datum/status_effect/incapacitating/stamcrit`.
    pub stamcrit: bool,
    /// `Time::elapsed_secs_f64` of `timed_stamina_reset`.
    stamina_reset_at: Option<f64>,
    /// stamcrit's `diminishing_return_counter`.
    diminishing: f32,
}

impl StatusEffects {
    fn slot(&mut self, kind: Incap) -> &mut f32 {
        match kind {
            Incap::Stun => &mut self.stun,
            Incap::Knockdown => &mut self.knockdown,
            Incap::Paralyze => &mut self.paralyze,
            Incap::Immobilize => &mut self.immobilize,
        }
    }

    pub fn remaining(&self, kind: Incap) -> f32 {
        match kind {
            Incap::Stun => self.stun,
            Incap::Knockdown => self.knockdown,
            Incap::Paralyze => self.paralyze,
            Incap::Immobilize => self.immobilize,
        }
    }

    /// `dazed/clear_daze`: stuns, paralysis and immobilization above 0 end the daze.
    fn on_status(&mut self, kind: Incap, secs: f32) {
        if secs > 0.0 && kind != Incap::Knockdown {
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

    pub fn incapacitated(&self) -> bool {
        self.stun > 0.0 || self.paralyze > 0.0 || self.stamcrit
    }

    pub fn immobilized(&self) -> bool {
        self.incapacitated() || self.immobilize > 0.0
    }

    pub fn floored(&self) -> bool {
        self.knockdown > 0.0 || self.paralyze > 0.0 || self.stamcrit
    }

    pub fn hands_blocked(&self) -> bool {
        self.stun > 0.0 || self.paralyze > 0.0
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

    /// One countdown step of `secs` at `now` (tg `process` of each effect, the stamina reset
    /// timer). `lying`: the body lies (a standing mob loses its daze). True while anything is
    /// left.
    pub fn tick(&mut self, secs: f32, now: f64, lying: bool) -> bool {
        for value in [
            &mut self.stun,
            &mut self.knockdown,
            &mut self.paralyze,
            &mut self.immobilize,
            &mut self.confusion,
            &mut self.dazed,
            &mut self.staggered,
            &mut self.no_side_kick,
        ] {
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
        !self.idle()
    }

    pub fn idle(&self) -> bool {
        self.stun <= 0.0
            && self.knockdown <= 0.0
            && self.paralyze <= 0.0
            && self.immobilize <= 0.0
            && self.confusion <= 0.0
            && self.dazed <= 0.0
            && self.staggered <= 0.0
            && self.no_side_kick <= 0.0
            && self.stamina <= 0.0
            && !self.stamcrit
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
}

impl Incapacitation {
    fn of(
        effects: Option<&StatusEffects>,
        cuffed: bool,
        limbs: Option<&crate::medical::LimbLimits>,
        rest: Option<&crate::rest::Rest>,
        locked: bool,
    ) -> Self {
        let e = |f: fn(&StatusEffects) -> bool| effects.is_some_and(f);
        Self {
            incapacitated: e(StatusEffects::incapacitated),
            // Buckled lying, no usable legs (`crate::medical`); a locked down cyborg
            // (`crate::silicon::chassis::LockedDown`).
            immobilized: e(StatusEffects::immobilized)
                || limbs.is_some_and(|l| l.immobilized)
                || locked,
            floored: e(StatusEffects::floored) || limbs.is_some_and(|l| l.floored),
            hands_blocked: e(StatusEffects::hands_blocked) || cuffed || locked,
            restrained: cuffed,
            staggered: effects.is_some_and(|e| e.staggered > 0.0),
            resting: rest.is_some_and(|r| r.down),
        }
    }
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

/// tg `Stun`/`Paralyze`/`Immobilize`/`Knockdown(amount)` on a living mob.
pub fn apply(world: &mut World, mob: Entity, kind: Incap, secs: f32) -> bool {
    // `PHYS_COEFF_STUN` (spliced nerves).
    let secs = secs * crate::medical::bioware::stun_mod(world, mob);
    effects_mut(world, mob).is_some_and(|mut e| e.apply(kind, secs))
}

/// tg `Knockdown(amount, daze_amount)`.
pub fn knockdown(world: &mut World, mob: Entity, secs: f32, daze: f32) {
    let secs = secs * crate::medical::bioware::stun_mod(world, mob);
    let dead = is_dead(world, mob);
    if let Some(mut e) = effects_mut(world, mob) {
        e.knockdown(secs, daze, dead);
    }
}

/// tg `adjust_stamina_loss(amount)` with stamcrit's messages.
pub fn adjust_stamina(world: &mut World, mob: Entity, amount: f32) {
    // `GET_PHYSIOLOGY(src, STAMINA)` on damage (spliced nerves).
    let amount = if amount > 0.0 {
        amount * crate::medical::bioware::stamina_mod(world, mob)
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
        app.add_systems(
            Update,
            (tick_effects, refresh_incapacitation)
                .chain()
                .in_set(StatusSystems)
                .before(HealthSystems),
        );
    }
}

/// Cost: per tick, one pass over mobs with active effects only.
fn tick_effects(
    time: Res<Time>,
    mut mobs: Query<(Entity, &mut StatusEffects, Option<&BodyPosition>)>,
    mut commands: Commands,
) {
    let (secs, now) = (time.delta_secs(), time.elapsed_secs_f64());
    for (mob, mut effects, body) in &mut mobs {
        let lying = body.is_some_and(|b| b.lying());
        if !effects.tick(secs, now, lying) {
            commands.entity(mob).remove::<StatusEffects>();
        }
    }
}

fn refresh_incapacitation(
    changed: Query<
        Entity,
        Or<(
            Changed<StatusEffects>,
            Changed<Handcuffed>,
            Changed<crate::medical::LimbLimits>,
            Changed<crate::rest::Rest>,
            Added<crate::silicon::chassis::LockedDown>,
        )>,
    >,
    mut removed_effects: RemovedComponents<StatusEffects>,
    mut removed_cuffs: RemovedComponents<Handcuffed>,
    mut removed_locks: RemovedComponents<crate::silicon::chassis::LockedDown>,
    mut mobs: Query<(
        Option<&StatusEffects>,
        Has<Handcuffed>,
        Option<&crate::medical::LimbLimits>,
        Option<&crate::rest::Rest>,
        Has<crate::silicon::chassis::LockedDown>,
        &mut Incapacitation,
    )>,
) {
    let removed: Vec<Entity> = removed_effects
        .read()
        .chain(removed_cuffs.read())
        .chain(removed_locks.read())
        .collect();
    for mob in changed.iter().chain(removed) {
        if let Ok((effects, cuffed, limbs, rest, locked, mut traits)) = mobs.get_mut(mob) {
            traits.set_if_neq(Incapacitation::of(effects, cuffed, limbs, rest, locked));
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
        assert!(e.tick(0.25, 0.25, true));
        assert_eq!(e.paralyze, 0.0);
        assert!(e.stun > 0.0);
        assert!(e.tick(0.25, 0.5, true));
        assert!(!e.incapacitated() && !e.immobilized() && e.floored());
        assert!(!e.tick(0.5, 1.0, true), "everything ran out");
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
        e.tick(9.0, 10.0, true);
        assert!(e.stamcrit);
        e.tick(1.0, 11.0, true);
        assert_eq!(e.stamina, 0.0);
        assert!(!e.stamcrit);
        assert!(!e.tick(0.0, 11.0, false), "nothing left");
    }

    #[test]
    fn stamina_reset_timer_restarts_on_damage() {
        let mut e = StatusEffects::default();
        e.adjust_stamina(60.0, 0.0, false, false);
        e.tick(8.0, 8.0, false);
        assert_eq!(e.stamina, 60.0);
        e.adjust_stamina(10.0, 8.0, false, false);
        e.tick(3.0, 11.0, false);
        assert_eq!(e.stamina, 70.0, "the 10 s count from the newest damage");
        e.tick(7.0, 18.0, false);
        assert_eq!(e.stamina, 0.0);
        // Healing does not restart it.
        e.adjust_stamina(50.0, 20.0, false, false);
        e.adjust_stamina(-10.0, 25.0, false, false);
        e.tick(5.0, 30.0, false);
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
        e.tick(1.0, 1.0, false);
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
}
