//! High gravity on mobs (server), after tg `handle_gravity`, `handle_high_gravity` (`life.dm`) and
//! `update_gravity` (`living.dm`): above standard gravity a mob is slowed by `gravity - 1`
//! deciseconds per tile (`/datum/movespeed_modifier/gravity`) and has the high gravity alert; at
//! the crushing threshold it takes brute damage every `Life()`, dead or alive (not in stasis).
//!
//! [`ForcedGravity`] is the `/datum/element/forced_gravity` of a mob; inverse gravitum in the
//! blood forces 5 (`/datum/reagent/inverse/gravitum`).
// shortcut: no gravity generator or portable generator fields (they are not ported: the turf
// always gives `1`), no `animate` blur (`gravity_animate`), and `blacklisted_movetypes = FLOATING`
// is not checked; a mob whose gravity falls back to 1 by moving keeps the state until the next
// `Life()` (at most 2 s).

use bevy::prelude::*;

use crate::breath::{AlertCategory, Alerts, MobsClock};
use crate::chem::MobReagents;
use crate::health::{DamageType, Health, MoveSpeedModifier, Stat, apply_damage};
use crate::medical::stasis::InStasis;

/// `STANDARD_GRAVITY`, `GRAVITY_DAMAGE_THRESHOLD`.
const STANDARD_GRAVITY: f32 = 1.0;
const GRAVITY_DAMAGE_THRESHOLD: f32 = 3.0;
/// `GRAVITY_DAMAGE_SCALING`, `GRAVITY_DAMAGE_MAXIMUM`.
const GRAVITY_DAMAGE_SCALING: f32 = 0.5;
const GRAVITY_DAMAGE_MAXIMUM: f32 = 1.5;
const GRAVITUM: &str = "/datum/reagent/inverse/gravitum";

/// tg `/datum/element/forced_gravity` on a mob (only the values above standard gravity matter
/// here).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct ForcedGravity(pub f32);

/// The last applied `speed_change` (`add_or_update_variable_movespeed_modifier`), to adjust the
/// shared [`MoveSpeedModifier`] by the difference.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
struct HighGravity {
    speed_change: f32,
}

pub struct GravityPlugin;

impl Plugin for GravityPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, life);
    }
}

/// `gravity_state` of `mob` above standard gravity; `None` at standard gravity or below.
pub fn high_gravity(world: &World, mob: Entity, gravitum: bool) -> Option<f32> {
    let forced = world.get::<ForcedGravity>(mob).map(|f| f.0);
    let gravity = [forced, gravitum.then_some(5.0)]
        .into_iter()
        .flatten()
        .fold(f32::MIN, f32::max);
    (gravity > STANDARD_GRAVITY).then_some(gravity)
}

/// `handle_high_gravity`: brute per second at `gravity`.
pub fn gravity_brute_per_second(gravity: f32) -> f32 {
    if gravity < GRAVITY_DAMAGE_THRESHOLD {
        return 0.0;
    }
    (GRAVITY_DAMAGE_SCALING * (gravity - GRAVITY_DAMAGE_THRESHOLD)).min(GRAVITY_DAMAGE_MAXIMUM)
}

/// `update_gravity`'s alert: state and severity (1 high, 2 crushing).
fn alert_of(gravity: f32) -> Option<(&'static str, u8)> {
    (gravity > STANDARD_GRAVITY).then_some((
        "paralysis",
        if gravity >= GRAVITY_DAMAGE_THRESHOLD {
            2
        } else {
            1
        },
    ))
}

/// One `Life()` per SSmobs fire for the mobs that can be in high gravity.
// Cost: every 2 s, one pass over the carbons (a reagent lookup each) and forced-gravity mobs.
fn life(world: &mut World, mut last_fired: Local<u64>) {
    let fired = world.resource::<MobsClock>().times_fired;
    if fired == *last_fired {
        return;
    }
    let fires = fired - *last_fired;
    *last_fired = fired;
    let mobs: Vec<Entity> = world
        .query_filtered::<Entity, (
            With<Health>,
            Or<(With<MobReagents>, With<ForcedGravity>, With<HighGravity>)>,
            Without<InStasis>,
        )>()
        .iter(world)
        .collect();
    for mob in mobs {
        let gravitum = crate::chem::blood_amount(world, mob, GRAVITUM) > 0.0;
        let gravity = high_gravity(world, mob, gravitum).unwrap_or(STANDARD_GRAVITY);
        apply(
            world,
            mob,
            gravity,
            fires as f32 * crate::breath::SSMOBS_WAIT_SECS as f32,
        );
    }
}

fn apply(world: &mut World, mob: Entity, gravity: f32, seconds: f32) {
    let old = world
        .get::<HighGravity>(mob)
        .map_or(0.0, |h| h.speed_change);
    let speed_change = (gravity - STANDARD_GRAVITY).max(0.0);
    if old != speed_change {
        let current = world.get::<MoveSpeedModifier>(mob).map_or(0.0, |m| m.0);
        let mut entity = world.entity_mut(mob);
        entity.insert(MoveSpeedModifier(current + speed_change - old));
        if speed_change > 0.0 {
            entity.insert(HighGravity { speed_change });
        } else {
            entity.remove::<HighGravity>();
        }
    }
    if let Some(mut alerts) = world.get_mut::<Alerts>(mob) {
        match alert_of(gravity) {
            Some((state, severity)) => {
                let shown = alerts.get(AlertCategory::Gravity).copied();
                if shown.is_none_or(|a| a.state != state || a.severity != severity) {
                    alerts.throw(AlertCategory::Gravity, state, severity);
                }
            }
            None => {
                // The weightless alert belongs to `crate::spacemove`.
                if alerts
                    .get(AlertCategory::Gravity)
                    .is_some_and(|a| a.state != "weightless")
                {
                    alerts.clear(AlertCategory::Gravity);
                }
            }
        }
    }
    let brute = gravity_brute_per_second(gravity) * seconds;
    if brute > 0.0
        && let Ok((mut health, mut stat)) = world
            .query::<(&mut Health, &mut Stat)>()
            .get_mut(world, mob)
    {
        apply_damage(&mut health, &mut stat, DamageType::Brute, brute);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crushing_gravity_scales_to_the_maximum() {
        assert_eq!(gravity_brute_per_second(2.0), 0.0);
        assert_eq!(gravity_brute_per_second(3.0), 0.0);
        assert_eq!(gravity_brute_per_second(4.0), 0.5);
        assert_eq!(gravity_brute_per_second(9.0), 1.5);
        assert_eq!(alert_of(2.0), Some(("paralysis", 1)));
        assert_eq!(alert_of(3.0), Some(("paralysis", 2)));
        assert_eq!(alert_of(1.0), None);
    }
}
