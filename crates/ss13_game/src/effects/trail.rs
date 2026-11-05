//! Trails, after tg `/datum/effect_system/trail_follow` (`effects_trail.dm`): while a holder
//! with an active [`Trail`] moves, every `SSfastprocess` tick (0.2 s) leaves a particle facing
//! its way on the tile it left, gone after `qdel_in_time`. Ion trails (jetpacks) only appear
//! without gravity, unless `grav_allowed`.
// shortcut: no `ion_fade`/`smoke_trail` flick (the particle keeps its icon until it goes).

use bevy::prelude::*;
use ss13_shared::{Facing, TilePos};

pub const ION_TRAILS: &str = "/obj/effect/particle_effect/ion_trails";
pub const ION_TRAILS_FLIGHT: &str = "/obj/effect/particle_effect/ion_trails/flight";
pub const SMOKE_TRAIL: &str = "/obj/effect/particle_effect/smoke_trail";

/// `SSfastprocess` wait.
const FASTPROCESS_WAIT: f64 = 0.2;

/// The ported `trail_follow` subtypes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrailKind {
    /// `trail_follow/ion`: `nograv_required`, 2 s.
    Ion,
    /// `trail_follow/ion/grav_allowed`.
    IonGravAllowed,
    /// `trail_follow/steam`: 1 s.
    Steam,
    /// `trail_follow/smoke`: 2.3 s.
    Smoke,
}

impl TrailKind {
    fn path(self) -> &'static str {
        match self {
            Self::Ion | Self::IonGravAllowed => ION_TRAILS,
            Self::Steam => super::particles::STEAM,
            Self::Smoke => SMOKE_TRAIL,
        }
    }

    /// `qdel_in_time`.
    fn lifetime(self) -> f64 {
        match self {
            Self::Ion | Self::IonGravAllowed => 2.0,
            Self::Steam => 1.0,
            Self::Smoke => 2.3,
        }
    }
}

/// An active `trail_follow` on its holder (`start()`; removing it is `stop()`).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Trail {
    pub kind: TrailKind,
    /// `oldposition`.
    pub last: Option<TilePos>,
    pub next_tick: f64,
}

impl Trail {
    pub fn new(kind: TrailKind) -> Self {
        Self {
            kind,
            last: None,
            next_tick: 0.0,
        }
    }
}

/// A trail particle, gone at `until`.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct TrailParticle {
    pub until: f64,
}

/// `trail_follow/process` -> `generate_effect` of every active trail.
// Cost: every 0.2 s per active trail (worn jetpacks), one turf lookup.
pub(super) fn tick_trails(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let trails: Vec<(Entity, Trail)> = world
        .query::<(Entity, &Trail)>()
        .iter(world)
        .filter(|(_, t)| t.next_tick <= now)
        .map(|(e, t)| (e, *t))
        .collect();
    for (holder, mut trail) in trails {
        trail.next_tick = now + FASTPROCESS_WAIT;
        let here = crate::explosions::turf_of(world, holder);
        let old = trail.last;
        trail.last = here;
        world.entity_mut(holder).insert(trail);
        let (Some(here), Some(old)) = (here, old) else {
            continue;
        };
        if old == here || (trail.kind == TrailKind::Ion && crate::multiz::has_gravity(world, old)) {
            continue;
        }
        let facing = carrier_facing(world, holder);
        let Some(particle) = crate::construction::spawn_obj(world, trail.kind.path(), old) else {
            continue;
        };
        let mut e = world.entity_mut(particle);
        e.insert(TrailParticle {
            until: now + trail.kind.lifetime(),
        });
        if let Some(dir) = facing {
            e.insert(Facing(dir));
        }
    }
}

/// `set_dir(holder.dir)`: the holder's facing, or its wearer's.
fn carrier_facing(world: &World, holder: Entity) -> Option<ss13_shared::Dir> {
    let mut e = holder;
    for _ in 0..4 {
        if let Some(f) = world.get::<Facing>(e) {
            return Some(f.0);
        }
        e = world.get::<crate::inventory::CarriedBy>(e)?.0;
    }
    None
}

pub(super) fn fade_trail_particles(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let gone: Vec<Entity> = world
        .query::<(Entity, &TrailParticle)>()
        .iter(world)
        .filter(|(_, p)| p.until <= now)
        .map(|(e, _)| e)
        .collect();
    for e in gone {
        world.despawn(e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trail_lifetimes_follow_qdel_in_time() {
        assert_eq!(TrailKind::Ion.lifetime(), 2.0);
        assert_eq!(TrailKind::Steam.lifetime(), 1.0);
        assert_eq!(TrailKind::Smoke.lifetime(), 2.3);
        assert_eq!(TrailKind::IonGravAllowed.path(), ION_TRAILS);
    }
}
