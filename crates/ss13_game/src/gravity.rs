//! High gravity on mobs (server), after tg `handle_gravity`, `handle_high_gravity` (`life.dm`) and
//! `update_gravity` (`living.dm`): above standard gravity a mob is slowed by `gravity - 1`
//! deciseconds per tile (`/datum/movespeed_modifier/gravity`) and has the high gravity alert; at
//! the crushing threshold it takes brute damage every `Life()`, dead or alive (not in stasis).
//!
//! [`ForcedGravity`] is the `/datum/element/forced_gravity` of a mob; inverse gravitum in the
//! blood forces 5 (`/datum/reagent/inverse/gravitum`).
//!
//! The gravity of a turf ([`turf_gravity`], tg `/atom/proc/has_gravity`): a forced gravity
//! field over it ([`GravityFields`], `/datum/proximity_monitor/advanced/gravity`: the gravity
//! generator's 6 G within 2 tiles, the portable gravity unit's) wins; else none in a space area
//! (`NO_GRAVITY`); else its level's (`SSmapping.gravity_by_z_level`, [`LevelGravity`]): the
//! gravity generators switched on there.
// shortcut: a level with no gravity generator at all keeps standard gravity off space areas
// (tg: the area's `default_gravity` or `ZTRAIT_GRAVITY`, neither converted; mining, centcom and
// test fixtures rely on it) — convert `default_gravity` and the level traits to drop it. No
// `animate` blur (`gravity_animate`), and `blacklisted_movetypes = FLOATING` is not checked; a
// mob whose gravity falls back to 1 by moving keeps the state until the next `Life()` (at most
// 2 s).

use std::collections::{HashMap, HashSet};

use bevy::prelude::*;

use crate::breath::{AlertCategory, Alerts, MobsClock};
use crate::chem::MobReagents;
use crate::health::{DamageType, Health, MoveSpeedModifier, Stat, apply_damage};
use crate::medical::stasis::InStasis;
use ss13_shared::TilePos;

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

/// `GLOB.gravity_generators` and `SSmapping.gravity_by_z_level`: the gravity generators of each
/// level and which of them are on (every generator has `setting = 1`).
#[derive(Resource, Debug, Default)]
pub struct LevelGravity {
    on: HashMap<u16, Vec<Entity>>,
    /// Levels with a gravity generator, on or off: their gravity is the generators'.
    controlled: HashSet<u16>,
}

impl LevelGravity {
    /// `update_list` of `generator` over `levels`.
    pub fn set(&mut self, levels: &[u16], generator: Entity, on: bool) {
        for &z in levels {
            self.controlled.insert(z);
            let list = self.on.entry(z).or_default();
            list.retain(|&g| g != generator);
            if on {
                list.push(generator);
            }
        }
    }

    /// `gravity_in_level`: the generators on at level `z`.
    pub fn generators_on(&self, z: u16) -> usize {
        self.on.get(&z).map_or(0, Vec::len)
    }

    /// `gravity_by_z_level[z]` of a level with a generator; `None` for the others.
    pub fn level(&self, z: u16) -> Option<f32> {
        self.controlled.contains(&z).then(|| {
            if self.generators_on(z) > 0 {
                STANDARD_GRAVITY
            } else {
                0.0
            }
        })
    }

    pub fn controlled(&self, z: u16) -> bool {
        self.controlled.contains(&z)
    }
}

/// A `/datum/proximity_monitor/advanced/gravity`: `/datum/element/forced_gravity` on every turf
/// within `range` of `center`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GravityField {
    pub source: Entity,
    pub center: TilePos,
    pub range: u32,
    pub gravity: f32,
}

/// The gravity fields, oldest first: a turf already under one is not taken by a later one
/// (`setup_field_turf` skips `TRAIT_FORCED_GRAVITY` turfs).
// Cost: a handful of fields; every gravity check walks them.
#[derive(Resource, Debug, Default)]
pub struct GravityFields(Vec<GravityField>);

impl GravityFields {
    /// The field of `source` set (replacing its old one) or, with `None`, removed.
    pub fn set(&mut self, source: Entity, field: Option<GravityField>) {
        match (self.0.iter().position(|f| f.source == source), field) {
            (Some(i), Some(field)) => self.0[i] = field,
            (Some(i), None) => {
                self.0.remove(i);
            }
            (None, Some(field)) => self.0.push(field),
            (None, None) => {}
        }
    }

    /// The forced gravity at `pos`, if a field covers it.
    pub fn at(&self, pos: TilePos) -> Option<f32> {
        self.0
            .iter()
            .find(|f| pos.chebyshev(f.center).is_some_and(|d| d <= f.range))
            .map(|f| f.gravity)
    }

    pub fn of(&self, source: Entity) -> Option<GravityField> {
        self.0.iter().find(|f| f.source == source).copied()
    }
}

/// The forced gravity of a field at `pos`.
pub fn field_gravity(world: &World, pos: TilePos) -> Option<f32> {
    world.get_resource::<GravityFields>()?.at(pos)
}

/// tg `has_gravity(turf)` as a value.
pub fn turf_gravity(world: &World, pos: TilePos) -> f32 {
    if let Some(forced) = field_gravity(world, pos) {
        return forced;
    }
    if crate::antag::nukeop::bomb::is_space_area(world, pos) {
        return 0.0;
    }
    world
        .get_resource::<LevelGravity>()
        .and_then(|l| l.level(pos.z))
        .unwrap_or(STANDARD_GRAVITY)
}

/// Whether gravity at `pos` comes from a generator or a field, so that losing it floats things
/// off any floor (elsewhere only space turfs are weightless, see the module shortcut).
pub fn gravity_managed(world: &World, pos: TilePos) -> bool {
    field_gravity(world, pos).is_some()
        || world
            .get_resource::<LevelGravity>()
            .is_some_and(|l| l.controlled(pos.z))
}

pub struct GravityPlugin;

impl Plugin for GravityPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LevelGravity>()
            .init_resource::<GravityFields>()
            .add_systems(Update, (drop_stale_fields, life));
    }
}

/// A field whose source is gone goes with it (`QDEL_NULL(gravity_field)` in `Destroy`).
fn drop_stale_fields(world: &mut World) {
    let stale: Vec<Entity> = world
        .resource::<GravityFields>()
        .0
        .iter()
        .map(|f| f.source)
        .filter(|&s| world.get_entity(s).is_err())
        .collect();
    for source in stale {
        world.resource_mut::<GravityFields>().set(source, None);
    }
}

/// `gravity_state` of `mob` above standard gravity; `None` at standard gravity or below. The
/// forced gravities (the mob's, gravitum's, a field over its turf) count by the largest.
pub fn high_gravity(world: &World, mob: Entity, gravitum: bool) -> Option<f32> {
    let forced = world.get::<ForcedGravity>(mob).map(|f| f.0);
    let field = world
        .get::<TilePos>(mob)
        .and_then(|&pos| field_gravity(world, pos));
    let gravity = [forced, gravitum.then_some(5.0), field]
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
