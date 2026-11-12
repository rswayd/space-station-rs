//! Frozen mobs (server), after tg `/datum/status_effect/freon` (`code/datums/status_effects/gas.dm`)
//! and `IsFrozen()`.
//!
//! - [`Frozen`]: the mob is in an ice cube — immobilized (`TRAIT_IMMOBILIZED`, in
//!   `crate::status_effects::Held`), the `frozen` alert and the `ice_cube` overlay; it can still
//!   use its hands. It lasts its duration (10 s by default) or melts once the body is back at its
//!   species' normal temperature; resisting breaks out after a 4 s `do_after`. Leaving it melts
//!   the cube: +100 K body temperature.
//! - Sources: a cold mob (≤ 50 K, no `TRAIT_RESISTCOLD`) on a turf vapor freezes
//!   (`freeze_turf`, `crate::janitor`), wet stacks on a body below `WATER_VAPOR_DEPOSITION_POINT`
//!   (`crate::fire`), [`freeze`] for the rest.
// shortcut: one cube at a time (tg's `freon/lasting` has its own id and stacks with the plain
// one); no watcher gaze, cone of cold, slimes or ice traps (their sources are not ported).

use bevy::prelude::*;
use ss13_mapload::IconIndex;
use ss13_net::server::NetServerSystems;
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Overlay};

use crate::breath::{AlertCategory, Alerts, Respiration};
use crate::health::{Health, Stat};
use crate::tools::tell_mob;

const FREEZE_DMI: &str = "icons/effects/freeze.dmi";
/// `/datum/status_effect/freon::duration`.
pub const FREON_SECS: f32 = 10.0;
/// `do_resist`: `do_after(owner, 4 SECONDS)`, deciseconds.
const BREAK_OUT_DS: f32 = 40.0;
/// `on_remove`: `adjust_bodytemperature(100)`.
const MELT_HEAT: f64 = 100.0;
/// `freeze_turf`: `L.bodytemperature <= 50`.
pub const FREEZE_TURF_TEMP: f64 = 50.0;

/// `/datum/status_effect/freon` on a mob.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Frozen {
    /// Game time it ends at; `None`: `STATUS_EFFECT_PERMANENT`.
    pub until: Option<f64>,
    /// `can_melt`: warmth ends it.
    pub can_melt: bool,
}

pub(crate) fn plugin(app: &mut App) {
    app.add_systems(
        Update,
        tick.run_if(any_with_component::<Frozen>)
            .before(crate::status_effects::StatusSystems),
    )
    .add_systems(PostUpdate, cube_overlays.before(NetServerSystems::Send));
}

/// tg `IsFrozen()`.
pub fn is_frozen(world: &World, mob: Entity) -> bool {
    world.get::<Frozen>(mob).is_some()
}

/// `IS_UNCONSCIOUS_OR_CRIT`.
fn out_or_crit(world: &World, mob: Entity) -> bool {
    world.get::<Stat>(mob) != Some(&Stat::Conscious)
        || crate::status_effects::knocked_out(world, mob)
}

/// `apply_status_effect(/datum/status_effect/freon, secs)` (`None`: permanent): false when the
/// mob is not living or already frozen (`STATUS_EFFECT_UNIQUE`).
pub fn freeze(world: &mut World, mob: Entity, secs: Option<f32>, can_melt: bool) -> bool {
    if world.get::<Health>(mob).is_none() || is_frozen(world, mob) {
        return false;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    world.entity_mut(mob).insert(Frozen {
        until: secs.map(|s| now + f64::from(s)),
        can_melt,
    });
    if let Some(mut alerts) = world.get_mut::<Alerts>(mob) {
        alerts.throw(AlertCategory::Frozen, "frozen", 0);
    }
    if !out_or_crit(world, mob) {
        tell_mob(
            world,
            mob,
            Span::Danger,
            "Вы оказываетесь вморожены в ледяной куб!",
        );
    }
    true
}

/// `on_remove`: the cube melts.
pub fn unfreeze(world: &mut World, mob: Entity) {
    if world.get::<Frozen>(mob).is_none() {
        return;
    }
    world.entity_mut(mob).remove::<Frozen>();
    if let Some(mut alerts) = world.get_mut::<Alerts>(mob) {
        alerts.clear(AlertCategory::Frozen);
    }
    if !out_or_crit(world, mob) {
        tell_mob(world, mob, Span::Notice, "Куб тает!");
    }
    if let Some(mut r) = world.get_mut::<Respiration>(mob) {
        r.bodytemperature += MELT_HEAT;
    }
}

/// `freeze_turf` on the mobs of a tile: the cold ones without `TRAIT_RESISTCOLD` freeze.
pub fn freeze_turf_mobs(world: &mut World, pos: ss13_shared::TilePos) {
    let mobs: Vec<Entity> = world
        .resource::<ss13_world::SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .filter(|&e| {
            world
                .get::<Respiration>(e)
                .is_some_and(|r| r.bodytemperature <= FREEZE_TURF_TEMP)
                && !crate::breath::BodyLife::of(world, e).resist_cold
        })
        .collect();
    for mob in mobs {
        freeze(world, mob, Some(FREON_SECS), true);
    }
}

/// `owner_resist` on `COMSIG_LIVING_RESIST`: breaking out of the cube takes 4 s.
pub fn resist(world: &mut World, client: Entity, mob: Entity) {
    if !is_frozen(world, mob) || world.get::<crate::tools::DoAfter>(mob).is_some() {
        return;
    }
    tell_mob(
        world,
        mob,
        Span::Notice,
        "Вы начинаете выбираться из ледяного куба...",
    );
    let actor = crate::tools::Actor {
        client,
        mob,
        item: None,
    };
    crate::tools::do_after(
        world,
        actor,
        mob,
        BREAK_OUT_DS,
        0,
        crate::construction::Job::BreakIce,
    );
}

/// The break-out `do_after` finished.
pub fn complete_break_out(world: &mut World, mob: Entity) {
    if !is_frozen(world, mob) {
        return;
    }
    tell_mob(world, mob, Span::Notice, "Вы выбираетесь из ледяного куба!");
    unfreeze(world, mob);
}

/// `tick` (every second in tg; the check is cheap, so every frame) and the duration: a cube
/// that can melt does once the body is at its normal temperature.
/// Cost: per tick, one pass over frozen mobs only.
fn tick(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let frozen: Vec<(Entity, Frozen)> = world
        .query::<(Entity, &Frozen)>()
        .iter(world)
        .map(|(e, f)| (e, *f))
        .collect();
    for (mob, f) in frozen {
        let expired = f.until.is_some_and(|t| now >= t);
        let warm = f.can_melt
            && world.get::<Respiration>(mob).is_some_and(|r| {
                r.bodytemperature >= crate::breath::BodyLife::of(world, mob).normal
            });
        if expired || warm {
            unfreeze(world, mob);
        }
    }
}

/// The `ice_cube` overlay of a frozen mob, on top of whatever built the appearance
/// (idempotent, like `crate::fire`'s).
fn cube_overlays(
    mut mobs: Query<(Option<Ref<Frozen>>, &mut Appearance), With<Health>>,
    mut removed: RemovedComponents<Frozen>,
    icons: Option<Res<IconIndex>>,
) {
    let Some(cube) = icons.and_then(|i| i.get(FREEZE_DMI, "ice_cube")) else {
        return;
    };
    // Cost: one pass over living mobs per tick, writing only on a change.
    for (frozen, mut appearance) in &mut mobs {
        if !appearance.is_changed() && frozen.as_ref().is_none_or(|f| !f.is_changed()) {
            continue;
        }
        let has = appearance.overlays.iter().any(|o| o.icon_state == cube);
        if frozen.is_some() && !has {
            appearance.overlays.push(Overlay::new(cube));
        } else if frozen.is_none() && has {
            appearance.overlays.retain(|o| o.icon_state != cube);
        }
    }
    for mob in removed.read() {
        if let Ok((None, mut appearance)) = mobs.get_mut(mob)
            && appearance.overlays.iter().any(|o| o.icon_state == cube)
        {
            appearance.overlays.retain(|o| o.icon_state != cube);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn frozen_mob(temp: f64) -> (World, Entity) {
        let mut world = World::new();
        world.insert_resource(Time::<()>::default());
        let mob = world
            .spawn((
                Health::default(),
                Stat::Conscious,
                Respiration {
                    bodytemperature: temp,
                    ..default()
                },
            ))
            .id();
        (world, mob)
    }

    #[test]
    fn the_cube_lasts_its_time_then_melts_warmer() {
        let (mut world, mob) = frozen_mob(100.0);
        assert!(freeze(&mut world, mob, Some(FREON_SECS), true));
        // Unique: a second cube does not apply.
        assert!(!freeze(&mut world, mob, Some(1.0), true));
        world
            .resource_mut::<Time>()
            .advance_by(Duration::from_secs_f32(9.0));
        tick(&mut world);
        assert!(is_frozen(&world, mob), "still cold, 1 s left");
        world
            .resource_mut::<Time>()
            .advance_by(Duration::from_secs_f32(1.5));
        tick(&mut world);
        assert!(!is_frozen(&world, mob));
        let temp = world.get::<Respiration>(mob).unwrap().bodytemperature;
        assert_eq!(temp, 200.0, "melting warms by 100 K");
    }

    #[test]
    fn a_warm_body_melts_only_a_cube_that_can_melt() {
        let (mut world, mob) = frozen_mob(crate::breath::BODYTEMP_NORMAL);
        freeze(&mut world, mob, None, false);
        tick(&mut world);
        assert!(is_frozen(&world, mob), "a watcher's cube does not melt");
        unfreeze(&mut world, mob);
        freeze(&mut world, mob, None, true);
        tick(&mut world);
        assert!(
            !is_frozen(&world, mob),
            "at the normal temperature it melts"
        );
    }
}
