//! Riding basic mobs (server), after tg `/datum/component/riding/creature` (`riding_mob.dm`) and
//! `/datum/element/ridable` on mobs.
//!
//! - A tamed cow, pig, pony or vatbeast becomes ridable (their `tamed()` add the element):
//!   [`Ridable`] and a [`Vehicle`] with the creature's riding numbers, so the seat, the steps,
//!   the offsets and getting off are the ridden vehicles' (`crate::vehicles`).
//! - A mob drags itself onto a ridable mob next to it (`mouse_buckle_handling` ->
//!   `user_buckle_mob` -> `is_buckle_possible`): not itself, neither buckled, the mount alive and
//!   free, the rider not incapacitated, with a usable hand. A rider pulling its mount lets go, the
//!   mount stops pulling.
//! - While ridden the mount's AI is off (`RIDING_PAUSE_AI_PLANNING | RIDING_PAUSE_AI_MOVEMENT`)
//!   and its own step delay gives way to the component's `vehicle_move_delay` (cow, pig,
//!   vatbeast 2 ds, pony 1.5 ds); the vatbeast flies in space (`override_allow_spacemove`).
//! - The rider sits `y` pixels up per facing (`get_rider_offsets_and_layers`), the mount is drawn
//!   above it facing south, below otherwise (`MOB_ABOVE/BELOW_PIGGYBACK_LAYER`).
//! - `ride_check`: a mount that ends up lying throws its rider off (paralyzed 1 s, knocked down
//!   4 s); a dead mount just drops it (`ridable/on_stat_change`).
//!
//! Cost: per tick, one pass over the ridable mobs (tamed ones only).
// shortcut: no riding of wolves, spiders, goliaths (saddle and lasso), bears, carp, leapers or
// raptors (their taming or player control is not ported), no syndicate ponies refusing anyone
// but their owner, no rider x offsets (the vehicles'
// shortcut), no buckling someone else onto a mount (2 s `do_after`), no shared abilities
// (`can_use_abilities`), no spin emote throwing the rider, no roughrider speed, no clown gallop;
// add each with its mount.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_world::Mover;

use super::BasicMob;
use super::ai::AiPaused;
use crate::health::{BodyPosition, Stat};
use crate::medical::surgery::Buckled;
use crate::medical::{name_of, visible};
use crate::pull::carry::{MOB_ABOVE_PIGGYBACK_LAYER, MOB_BELOW_PIGGYBACK_LAYER};
use crate::status_effects::Incap;
use crate::tools::Actor;
use crate::vehicles::{Kind, RidingDef, Vehicle};

/// `get_parent_offsets_and_layers` of the farm creatures, north, south, east, west.
const PARENT: [(i16, i16, Option<f32>); 4] = [
    (0, 0, Some(MOB_BELOW_PIGGYBACK_LAYER)),
    (0, 0, Some(MOB_ABOVE_PIGGYBACK_LAYER)),
    (0, 0, Some(MOB_BELOW_PIGGYBACK_LAYER)),
    (0, 0, Some(MOB_BELOW_PIGGYBACK_LAYER)),
];

/// The `/datum/component/riding/creature/*` of mob type `path`, `None` for mobs not ridden here.
pub fn creature_def(path: &str) -> Option<RidingDef> {
    let under = |root: &str| is_under(path, root);
    // (`vehicle_move_delay`, the rider's `y` offset, `override_allow_spacemove`).
    let (move_delay, up, spacemove) = if under("/mob/living/basic/pony") {
        (1.5, 9, false)
    } else if under("/mob/living/basic/cow") || under("/mob/living/basic/pig") {
        (2.0, 8, false)
    } else if under(super::oneoffs::VATBEAST) {
        (2.0, 15, true)
    } else {
        return None;
    };
    Some(RidingDef {
        kind: Kind::Plain,
        move_delay,
        key: None,
        flags: 0,
        spacemove,
        rider_up: [up; 4],
        parent: PARENT,
        instability: 0.0,
        item: None,
    })
}

/// A mob with `/datum/element/ridable`.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Ridable {
    /// The mount's own step delay, kept while someone rides it (`None`: nobody does).
    own_delay: Option<f32>,
}

pub(super) fn build(app: &mut App) {
    app.add_systems(
        Update,
        ride_checks.after(ss13_world::WorldSystems::Movement),
    );
}

/// `AddElement(/datum/element/ridable, ...)` of a tamed mob, when its riding is ported.
pub fn make_ridable(world: &mut World, mob: Entity) {
    if world.get::<Ridable>(mob).is_some() {
        return;
    }
    let Some(def) = world
        .get::<BasicMob>(mob)
        .and_then(|b| creature_def(&b.def.path))
    else {
        return;
    };
    world
        .entity_mut(mob)
        .insert((Ridable::default(), Vehicle::new(def)));
}

/// `/atom/movable/mouse_drop_receive` of a ridable mob: `dragged` mounting it. True when the drop
/// was a mount's.
pub fn mouse_drop_receive(world: &mut World, actor: Actor, mount: Entity, dragged: Entity) -> bool {
    if world.get::<Ridable>(mount).is_none() || dragged != actor.mob {
        return false;
    }
    if !can_mount(world, dragged, mount) {
        return true;
    }
    let Some(&pos) = world.get::<TilePos>(mount) else {
        return true;
    };
    // `riding/creature/Initialize` -> `stop_pulling`, `handle_buckle`.
    world.entity_mut(mount).remove::<crate::pull::Pulling>();
    if world
        .get::<crate::pull::Pulling>(dragged)
        .is_some_and(|p| p.0 == mount)
    {
        world.entity_mut(dragged).remove::<crate::pull::Pulling>();
    }
    world.entity_mut(dragged).insert((pos, Buckled(mount)));
    crate::vehicles::post_buckle(world, actor, dragged, mount);
    true
}

/// `is_user_buckle_possible` + `is_buckle_possible` of `rider` onto `mount`.
pub fn can_mount(world: &World, rider: Entity, mount: Entity) -> bool {
    let (Some(&at), Some(&pos)) = (world.get::<TilePos>(rider), world.get::<TilePos>(mount)) else {
        return false;
    };
    let free = world
        .get::<Vehicle>(mount)
        .is_some_and(|v| v.rider.is_none());
    let alive = world.get::<Stat>(mount).is_none_or(|s| *s != Stat::Dead);
    // A carbon needs a usable hand to climb on.
    let hands = world.get::<crate::inventory::Inventory>(rider).is_none()
        || (0..2).any(|h| crate::medical::bodyparts::has_hand(world, rider, h));
    rider != mount
        && free
        && alive
        && hands
        && at.chebyshev(pos).is_some_and(|d| d <= 1)
        && world.get::<Buckled>(rider).is_none()
        && world.get::<Buckled>(mount).is_none()
        && world
            .get::<Vehicle>(rider)
            .is_none_or(|v| v.rider.is_none())
        && !crate::status_effects::incapacitation(world, rider).incapacitated
}

/// tg `user_unbuckle_mob` of a mount, after `crate::medical::surgery::unbuckle` took
/// [`Buckled`]. False when `furniture` is no ridable mob.
pub(crate) fn post_unbuckle(world: &mut World, actor: Actor, mob: Entity, mount: Entity) -> bool {
    if world.get::<Ridable>(mount).is_none() {
        return false;
    }
    crate::vehicles::post_unbuckle(world, actor, mob, mount);
    true
}

/// The ride starting and ending on the mount (`vehicle_mob_buckle`/`vehicle_mob_unbuckle`), and
/// `ride_check` while it lasts.
fn ride_checks(world: &mut World) {
    let mounts: Vec<(Entity, Option<Entity>, Option<f32>)> = world
        .query::<(Entity, &Ridable, &Vehicle)>()
        .iter(world)
        .map(|(e, r, v)| (e, v.rider, r.own_delay))
        .collect();
    for (mount, rider, own) in mounts {
        match (rider, own) {
            (Some(_), None) => start_ride(world, mount),
            (None, Some(delay)) => end_ride(world, mount, delay),
            _ => {}
        }
        if let Some(rider) = rider {
            ride_check(world, mount, rider);
        }
    }
}

fn start_ride(world: &mut World, mount: Entity) {
    let own = world.get::<Mover>(mount).map_or(0.0, |m| m.delay);
    if let Some(mut mover) = world.get_mut::<Mover>(mount) {
        mover.delay = 0.0;
    }
    if let Some(mut r) = world.get_mut::<Ridable>(mount) {
        r.own_delay = Some(own);
    }
    world.entity_mut(mount).insert(AiPaused);
}

fn end_ride(world: &mut World, mount: Entity, delay: f32) {
    if let Some(mut mover) = world.get_mut::<Mover>(mount) {
        mover.delay = delay;
    }
    if let Some(mut r) = world.get_mut::<Ridable>(mount) {
        r.own_delay = None;
    }
    world.entity_mut(mount).remove::<AiPaused>();
}

/// `riding/creature/ride_check` and `ridable/on_stat_change`.
fn ride_check(world: &mut World, mount: Entity, rider: Entity) {
    if world.get::<Stat>(mount) == Some(&Stat::Dead) {
        crate::vehicles::dismount(world, rider);
        return;
    }
    if !world.get::<BodyPosition>(mount).is_some_and(|b| b.lying()) {
        return;
    }
    let (who, what) = (name_of(world, rider), name_of(world, mount));
    visible(
        world,
        rider,
        &format!("Вы падаете с {what}!"),
        &format!("{who} падает с {what}!"),
        Span::Warning,
    );
    crate::status_effects::apply(world, rider, Incap::Paralyze, 1.0);
    crate::status_effects::apply(world, rider, Incap::Knockdown, 4.0);
    crate::vehicles::dismount(world, rider);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creature_numbers_follow_tg() {
        let pony = creature_def("/mob/living/basic/pony/syndicate").unwrap();
        assert_eq!(pony.move_delay, 1.5);
        assert_eq!(pony.rider_up, [9; 4]);
        assert!(!pony.spacemove);
        let cow = creature_def("/mob/living/basic/cow/moonicorn").unwrap();
        assert_eq!((cow.move_delay, cow.rider_up[0]), (2.0, 8));
        let vatbeast = creature_def("/mob/living/basic/vatbeast").unwrap();
        assert!(vatbeast.spacemove);
        assert_eq!(vatbeast.rider_up, [15; 4]);
        assert_eq!(vatbeast.parent[1].2, Some(MOB_ABOVE_PIGGYBACK_LAYER));
        assert!(creature_def("/mob/living/basic/carp").is_none());
    }
}
