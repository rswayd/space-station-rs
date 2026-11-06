//! Force fields (server), after tg `/obj/effect/forcefield`
//! (`code/game/objects/effects/forcefields.dm`): anchored walls that vanish after their
//! `initial_duration` (`QDEL_IN`; 0 keeps them). Every kind but the cosmic field is dense;
//! air passes as its prototype's `can_atmos_pass` says (the cult's blocks it).
//!
//! | type | lasts |
//! | --- | --- |
//! | `forcefield`, `wizard`, `mime` | 30 s |
//! | `cult` | 20 s, `cult/permanent` forever |
//! | `mime/advanced` | 60 s |
//! | `psychic` | 10 s |
//! | `cosmic_field` (not dense) | 30 s; `star_blast`, `fast` 5 s, `extrafast` 2.5 s |
//!
//! Spawners: the wizard's Forcewall (`crate::spells::effects`); the other casters (cult,
//! mime, psyker, cosmic heretic) spawn theirs with [`spawn_forcefield`] when they land.
// shortcut: a field blocks every mover, its caster and antimagic holders too (tg
// `wizard/CanAllowThrough`, `cosmic_field/CanAllowThrough`: needs a movement hook in
// `ss13_world`); no projectile ricochet off the psychic field or slowing in the cosmic one,
// no cosmic movespeed bonus or explosion stop, no leaning on the mime's wall.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_shared::TilePos;
use ss13_world::Density;

pub const FORCEFIELD: &str = "/obj/effect/forcefield";
pub const WIZARD: &str = "/obj/effect/forcefield/wizard";
pub const COSMIC: &str = "/obj/effect/forcefield/cosmic_field";

/// `initial_duration` by type, subtypes first (seconds; 0 = forever).
const DURATIONS: [(&str, f64); 8] = [
    ("/obj/effect/forcefield/cult/permanent", 0.0),
    ("/obj/effect/forcefield/cult", 20.0),
    ("/obj/effect/forcefield/mime/advanced", 60.0),
    ("/obj/effect/forcefield/psychic", 10.0),
    ("/obj/effect/forcefield/cosmic_field/star_blast", 5.0),
    ("/obj/effect/forcefield/cosmic_field/fast", 5.0),
    ("/obj/effect/forcefield/cosmic_field/extrafast", 2.5),
    // `cosmic_field`, `star_touch`, `wizard`, `mime` and the base.
    (FORCEFIELD, 30.0),
];

/// A force field, gone at `until` (`None`: permanent).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Forcefield {
    pub until: Option<f64>,
}

/// `initial_duration` of `path`, seconds (0: permanent).
pub fn duration(path: &str) -> f64 {
    DURATIONS
        .iter()
        .find(|(p, _)| is_under(path, p))
        .map_or(30.0, |&(_, secs)| secs)
}

/// `new path(pos)`: a force field of `path` on `pos`; `None` without its prototype.
pub fn spawn_forcefield(world: &mut World, path: &str, pos: TilePos) -> Option<Entity> {
    let secs = duration(path);
    let now = world.resource::<Time>().elapsed_secs_f64();
    let field = crate::construction::spawn_obj(world, path, pos)?;
    let until = (secs > 0.0).then_some(now + secs);
    world.entity_mut(field).insert(Forcefield { until });
    if !is_under(path, COSMIC) {
        world.entity_mut(field).insert(Density);
    }
    crate::construction::tile_changed(world, pos);
    Some(field)
}

/// `QDEL_IN(src, initial_duration)`.
// Cost: one pass over the live fields (a handful per round), only while any exist.
pub(super) fn expire_forcefields(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let gone: Vec<(Entity, Option<TilePos>)> = world
        .query::<(Entity, &Forcefield, Option<&TilePos>)>()
        .iter(world)
        .filter(|(_, f, _)| f.until.is_some_and(|t| now >= t))
        .map(|(e, _, p)| (e, p.copied()))
        .collect();
    for (field, pos) in gone {
        world.despawn(field);
        if let Some(pos) = pos {
            crate::construction::tile_changed(world, pos);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_follow_the_types() {
        assert_eq!(duration(WIZARD), 30.0);
        assert_eq!(duration("/obj/effect/forcefield/mime"), 30.0);
        assert_eq!(duration("/obj/effect/forcefield/mime/advanced"), 60.0);
        assert_eq!(duration("/obj/effect/forcefield/cult"), 20.0);
        assert_eq!(duration("/obj/effect/forcefield/cult/permanent"), 0.0);
        assert_eq!(duration("/obj/effect/forcefield/psychic"), 10.0);
        assert_eq!(duration(COSMIC), 30.0);
        assert_eq!(
            duration("/obj/effect/forcefield/cosmic_field/star_touch"),
            30.0
        );
        assert_eq!(
            duration("/obj/effect/forcefield/cosmic_field/extrafast"),
            2.5
        );
    }
}
