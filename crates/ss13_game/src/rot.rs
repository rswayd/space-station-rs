//! Corpse rot (server), after tg `/datum/component/rot` (`datums/components/rot.dm`), attached by
//! carbon `death()` (`attach_rot`: 6 minutes' delay, 10 minutes' scaling, severity 1).
//!
//! An organic or undead carbon that really died (not fake death, not a hologram) starts rotting.
//! Formaldehyde (15 u) or any cryostylane in the blood, a core below -10 C and a husk stop the
//! clock, keeping the delay already served ([`Rot::rest`]). Reviving ends it. Once the delay has
//! passed, whoever touches the body, pulls it, hits it or walks onto it (unless the air around it
//! is below -10 C) has `prob(severity * scaling)` to contract a weak random disease.
// shortcut: blockers are re-read once a second instead of on signals; no bump reaction (a body
// flying into someone), no hulk or animal attack routes beyond the click.

use std::collections::HashMap;

use bevy::prelude::*;
use ss13_atmos::Atmos;
use ss13_shared::TilePos;
use ss13_world::movement::Moved;

use crate::breath::Respiration;
use crate::click::ClickResolved;
use crate::death::TimeOfDeath;
use crate::health::{DeathComa, Health, Stat};
use crate::holodeck::Hologram;
use crate::human::Husk;
use crate::medical::diseases::{Disease, contact_contract};
use crate::medical::wounds::Limbs;
use crate::pull::Pulling;
use crate::species::biotype::{ORGANIC, UNDEAD};
use crate::zone::BodyZone;

/// `attach_rot`: `start_delay`, seconds.
const START_DELAY: f64 = 6.0 * 60.0;
/// `attach_rot`: `scaling_delay`, seconds.
const SCALING_DELAY: f64 = 10.0 * 60.0;
/// `attach_rot`: `severity`.
const STRENGTH: f64 = 1.0;
/// `T0C - 10`, kelvin: no rot of a colder core, no contagion in colder air.
const COLD_LIMIT: f64 = crate::effects::frozen::T0C - 10.0;
const FORMALDEHYDE: &str = "/datum/reagent/toxin/formaldehyde";
const CRYOSTYLANE: &str = "/datum/reagent/cryostylane";

const REAGENT_BLOCKER: u8 = 1 << 0;
const TEMPERATURE_BLOCKER: u8 = 1 << 1;
const HUSK_BLOCKER: u8 = 1 << 2;

/// tg `/datum/component/rot`.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Rot {
    start_time: f64,
    start_delay: f64,
    active: bool,
    blockers: u8,
}

impl Rot {
    fn new(now: f64) -> Self {
        Self {
            start_time: now,
            start_delay: START_DELAY,
            active: true,
            blockers: 0,
        }
    }

    /// tg `start_up`: a blocker gone; the last one gone starts the clock again.
    fn start_up(&mut self, now: f64, blocker: u8) {
        self.blockers &= !blocker;
        if self.blockers != 0 || self.active {
            return;
        }
        self.start_time = now;
        self.active = true;
    }

    /// tg `rest`: a blocker added; the first one stops the clock, keeping what was served.
    fn rest(&mut self, now: f64, blocker: u8) {
        let old = self.blockers;
        self.blockers |= blocker;
        if old != 0 || !self.active {
            return;
        }
        self.start_delay = (self.start_time + self.start_delay - now).max(0.0);
        self.active = false;
    }

    fn set_blocker(&mut self, now: f64, blocker: u8, blocked: bool) {
        if blocked {
            self.rest(now, blocker);
        } else {
            self.start_up(now, blocker);
        }
    }

    /// `time_scaling` of `rot_react`: `None` before the delay or while stopped.
    fn scaling(&self, now: f64) -> Option<f64> {
        let delta = now - self.start_time;
        if !self.active || delta < self.start_delay {
            return None;
        }
        Some(((delta - self.start_delay) / SCALING_DELAY).min(1.0))
    }
}

pub struct RotPlugin;

impl Plugin for RotPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                attach_rot,
                touched_by_click,
                touched_by_pull,
                entered_rot,
                upkeep.run_if(any_with_component::<Rot>),
            ),
        );
    }
}

/// carbon `death()` -> `attach_rot()`.
fn attach_rot(
    dead: Query<
        (Entity, &Stat),
        (
            Added<TimeOfDeath>,
            With<Limbs>,
            Without<DeathComa>,
            Without<Hologram>,
        ),
    >,
    time: Res<Time>,
    mut commands: Commands,
) {
    let now = time.elapsed_secs_f64();
    for (mob, stat) in &dead {
        if *stat == Stat::Dead {
            commands.queue(move |world: &mut World| {
                // `Initialize`: `COMPONENT_INCOMPATIBLE` without `MOB_ORGANIC|MOB_UNDEAD`.
                if crate::species::def_of(world, mob).biotypes & (ORGANIC | UNDEAD) != 0
                    && let Ok(mut body) = world.get_entity_mut(mob)
                {
                    body.insert(Rot::new(now));
                }
            });
        }
    }
}

fn pick_arm(world: &mut World) -> BodyZone {
    if world
        .resource_mut::<crate::combat::CombatRng>()
        .chance(50.0)
    {
        BodyZone::LArm
    } else {
        BodyZone::RArm
    }
}

/// `COMSIG_ATOM_ATTACK_HAND`, `COMSIG_ATOM_ATTACKBY` -> `rot_react_touch`.
fn touched_by_click(
    mut clicks: MessageReader<ClickResolved>,
    rots: Query<(), With<Rot>>,
    mut commands: Commands,
) {
    for click in clicks.read() {
        if rots.contains(click.target) {
            let (body, victim) = (click.target, click.mob);
            commands.queue(move |world: &mut World| {
                let zone = pick_arm(world);
                rot_react(world, body, victim, Some(zone));
            });
        }
    }
}

/// `COMSIG_LIVING_GET_PULLED` -> `rot_react_touch`.
fn touched_by_pull(
    pulls: Query<(Entity, &Pulling), Added<Pulling>>,
    rots: Query<(), With<Rot>>,
    mut commands: Commands,
) {
    for (puller, pulling) in &pulls {
        if rots.contains(pulling.0) {
            let body = pulling.0;
            commands.queue(move |world: &mut World| {
                let zone = pick_arm(world);
                rot_react(world, body, puller, Some(zone));
            });
        }
    }
}

/// `connect_loc` -> `on_entered`: whoever walks onto a rotting body.
// Cost: per tick with rotting bodies, one map of their tiles (few) and a lookup per step.
fn entered_rot(
    mut moves: MessageReader<Moved>,
    rots: Query<(Entity, &TilePos), With<Rot>>,
    living: Query<(), With<Health>>,
    mut commands: Commands,
) {
    if rots.is_empty() {
        moves.clear();
        return;
    }
    let mut tiles: HashMap<TilePos, Vec<Entity>> = HashMap::new();
    for (body, &pos) in &rots {
        tiles.entry(pos).or_default().push(body);
    }
    for step in moves.read() {
        let Some(bodies) = tiles.get(&step.to) else {
            continue;
        };
        if !living.contains(step.entity) {
            continue;
        }
        let victim = step.entity;
        for &body in bodies {
            if body != victim {
                commands.queue(move |world: &mut World| rot_react(world, body, victim, None));
            }
        }
    }
}

/// tg `rot_react`: the chance of a disease for `victim` touching `body` on `zone`.
fn rot_react(world: &mut World, body: Entity, victim: Entity, zone: Option<BodyZone>) {
    let Some(&rot) = world.get::<Rot>(body) else {
        return;
    };
    if world.get::<Health>(victim).is_none() {
        return;
    }
    let cold_air = crate::explosions::turf_of(world, body)
        .and_then(|pos| world.get_resource::<Atmos>()?.temperature(pos))
        .is_none_or(|t| t <= COLD_LIMIT);
    let now = world.resource::<Time>().elapsed_secs_f64();
    if cold_air {
        return;
    }
    let Some(scaling) = rot.scaling(now) else {
        return;
    };
    let mut rng = crate::medical::fork_rng(world);
    if !rng.chance((STRENGTH * scaling) as f32) {
        return;
    }
    // `new /datum/disease/advance/random(rand(4 * strength * scaling), rand(strength * 5 *
    // scaling))`; DM `rand(max)` is `0..=max` and `!0` falls back to the default symptom count.
    let symptoms = rng.range(0, (4.0 * STRENGTH * scaling) as u32) as usize;
    let level = rng.range(0, (5.0 * STRENGTH * scaling) as u32) as u8;
    let mut disease = Disease::random_advance(&mut rng, (symptoms > 0).then_some(symptoms), level);
    disease.name = "Unknown".to_owned();
    contact_contract(world, victim, &disease, zone, &mut rng);
}

/// `react_to_revive`; the blocker signals (`check_reagent`, `check_for_temperature`,
/// `check_husk_trait`) once a second.
fn upkeep(world: &mut World, mut next: Local<f64>) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    if now < *next {
        return;
    }
    *next = now + 1.0;
    let bodies: Vec<Entity> = world
        .query_filtered::<Entity, With<Rot>>()
        .iter(world)
        .collect();
    for body in bodies {
        if world.get::<Stat>(body) != Some(&Stat::Dead) {
            if let Ok(mut body) = world.get_entity_mut(body) {
                body.remove::<Rot>();
            }
            continue;
        }
        use crate::chem::blood_amount;
        let preserved = blood_amount(world, body, FORMALDEHYDE) >= 15.0
            || blood_amount(world, body, CRYOSTYLANE) > 0.0;
        let cold = world
            .get::<Respiration>(body)
            .is_some_and(|r| r.coretemperature <= COLD_LIMIT);
        let husk = world.get::<Husk>(body).is_some();
        if let Some(mut rot) = world.get_mut::<Rot>(body) {
            rot.set_blocker(now, REAGENT_BLOCKER, preserved);
            rot.set_blocker(now, TEMPERATURE_BLOCKER, cold);
            rot.set_blocker(now, HUSK_BLOCKER, husk);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blocker_keeps_the_delay_already_served() {
        let mut rot = Rot::new(0.0);
        assert_eq!(rot.scaling(10.0), None);
        rot.set_blocker(100.0, REAGENT_BLOCKER, true);
        assert!(!rot.active);
        assert_eq!(rot.start_delay, START_DELAY - 100.0);
        // A second blocker changes nothing; the clock restarts when the last one goes.
        rot.set_blocker(150.0, HUSK_BLOCKER, true);
        rot.set_blocker(200.0, REAGENT_BLOCKER, false);
        assert!(!rot.active);
        rot.set_blocker(300.0, HUSK_BLOCKER, false);
        assert!(rot.active);
        assert_eq!(rot.start_time, 300.0);
        assert_eq!(rot.scaling(300.0 + START_DELAY - 100.0), Some(0.0));
    }

    #[test]
    fn scaling_reaches_one_after_the_scaling_delay() {
        let rot = Rot::new(0.0);
        assert_eq!(rot.scaling(START_DELAY + SCALING_DELAY / 2.0), Some(0.5));
        assert_eq!(rot.scaling(START_DELAY + SCALING_DELAY * 3.0), Some(1.0));
    }
}
