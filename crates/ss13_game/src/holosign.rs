//! Holographic sign projectors and their signs (server), after tg `/obj/item/holosign_creator`
//! (`code/game/objects/items/holosign_creator.dm`) and `/obj/structure/holosign`
//! (`code/game/objects/structures/holosign.dm`).
//!
//! A projector in hand clicked on something adjacent projects its sign onto that tile (after
//! its `creation_time`), at most `max_signs` at once, never onto a tile with a sign of its kind
//! or a dense non-mob object (doors excepted; the holofan projector also through windows and
//! grilles). Using it in hand clears its signs; clicking one of its barriers with it removes
//! that barrier; signs vanish with their projector. A sign has 1-20 integrity: a hand swat
//! does 5 (`attack_holosign`), items hit it as any structure. The holofan's barrier blocks air
//! and heat but not movement (prototype `can_atmos_pass`).
// shortcut: dense barriers block every mover (tg lets walking and unconscious carbons through,
// the PENLITE one only the healthy: `CanAllowThrough` needs a movement hook in `ss13_world`);
// no remote opening (`ranged_item_interaction`), no clearview, no firelock opened under a new
// holofan, no painted colour.

use bevy::prelude::*;
use ss13_mapload::{DmItem, DmObj};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_world::SpatialIndex;

use crate::construction::Job;
use crate::health::{DamageType, Health};
use crate::obj_damage::is_under;
use crate::tools::{Actor, do_after, tell};

pub const HOLOSIGN: &str = "/obj/structure/holosign";
const BARRIER: &str = "/obj/structure/holosign/barrier";
const ATMOS_PROJECTOR: &str = "/obj/item/holosign_creator/atmos";
const PROJECTOR: &str = "/obj/item/holosign_creator";

/// (projector, `holosign_type`, `creation_time` ds, `max_signs`), subtypes before the base.
const PROJECTORS: [(&str, &str, f32, usize); 7] = [
    (
        "/obj/item/holosign_creator/janibarrier",
        "/obj/structure/holosign/barrier/wetsign",
        10.0,
        12,
    ),
    (
        "/obj/item/holosign_creator/security",
        "/obj/structure/holosign/barrier",
        20.0,
        6,
    ),
    (
        "/obj/item/holosign_creator/engineering",
        "/obj/structure/holosign/barrier/engineering",
        10.0,
        12,
    ),
    (
        ATMOS_PROJECTOR,
        "/obj/structure/holosign/barrier/atmos",
        0.0,
        6,
    ),
    (
        "/obj/item/holosign_creator/medical",
        "/obj/structure/holosign/barrier/medical",
        10.0,
        6,
    ),
    (
        "/obj/item/holosign_creator/cyborg",
        "/obj/structure/holosign/barrier/cyborg",
        15.0,
        9,
    ),
    (PROJECTOR, "/obj/structure/holosign/wetsign", 0.0, 10),
];

/// tg `projectable_through` of every projector; the holofan's adds windows and grilles.
const THROUGH: [&str; 2] = ["/obj/machinery/door", "/obj/structure/mineral_door"];
const THROUGH_ATMOS: [&str; 2] = ["/obj/structure/window", "/obj/structure/grille"];

/// A projector's live signs (tg `signs`).
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct HoloSigns(pub Vec<Entity>);

/// A projected sign and the projector keeping it (tg `projector`).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Holosign {
    pub projector: Entity,
}

pub struct HolosignServerPlugin;

impl Plugin for HolosignServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(projector_gone);
    }
}

/// tg `/obj/item/holosign_creator/Destroy`: its signs go with it.
fn projector_gone(
    remove: On<Remove<HoloSigns>>,
    projectors: Query<&HoloSigns>,
    mut commands: Commands,
) {
    let Ok(signs) = projectors.get(remove.entity) else {
        return;
    };
    let signs = signs.0.clone();
    commands.queue(move |world: &mut World| {
        for sign in signs {
            remove_sign(world, sign);
        }
    });
}

fn remove_sign(world: &mut World, sign: Entity) {
    let Some(&pos) = world.get::<TilePos>(sign) else {
        return;
    };
    if world.get::<Holosign>(sign).is_none() {
        return;
    }
    world.despawn(sign);
    crate::construction::tile_changed(world, pos);
}

/// (`holosign_type`, `creation_time` ds, `max_signs`) of a projector type.
fn projector_kind(path: &str) -> Option<(&'static str, f32, usize)> {
    PROJECTORS
        .iter()
        // The base only for itself: other subtypes (the restaurant's robot seats) project
        // their own signs.
        .find(|(p, ..)| {
            if *p == PROJECTOR {
                path == *p
            } else {
                is_under(path, p)
            }
        })
        .map(|&(_, sign, delay, max)| (sign, delay, max))
}

fn projector_of(world: &World, item: Entity) -> Option<(&'static str, f32, usize)> {
    projector_kind(&world.get::<DmItem>(item)?.path)
}

/// The projector's signs that still exist (its list pruned to them).
fn live_signs(world: &mut World, projector: Entity) -> Vec<Entity> {
    let signs: Vec<Entity> = world
        .get::<HoloSigns>(projector)
        .map(|s| s.0.clone())
        .unwrap_or_default()
        .into_iter()
        .filter(|&s| {
            world
                .get::<Holosign>(s)
                .is_some_and(|h| h.projector == projector)
        })
        .collect();
    if let Some(mut list) = world.get_mut::<HoloSigns>(projector)
        && list.0 != signs
    {
        list.0.clone_from(&signs);
    }
    signs
}

/// `is_blocked_turf(exclude_mobs = TRUE, ignore_atoms = projectable_through)`, or a sign of
/// this kind already there.
fn tile_refuses(world: &World, pos: TilePos, sign: &str, atmos: bool) -> bool {
    let index = world.resource::<SpatialIndex>();
    let same = index.on_tile(pos).iter().any(|&e| {
        world
            .get::<DmObj>(e)
            .is_some_and(|o| is_under(&o.path, sign) && world.get::<Holosign>(e).is_some())
    });
    same || index.dense_on_tile(pos).iter().any(|&e| {
        if world.get::<Health>(e).is_some() {
            return false;
        }
        let path = crate::construction::path_of(world, e).unwrap_or_default();
        let through = THROUGH.iter().any(|t| is_under(&path, t))
            || (atmos && THROUGH_ATMOS.iter().any(|t| is_under(&path, t)));
        !through
    })
}

/// The held projector's `interact_with_atom` on `target` (and a barrier's `item_interaction`
/// with its own projector). True when the held item is a projector.
pub fn interact_with_atom(world: &mut World, actor: Actor, held: Entity, target: Entity) -> bool {
    let Some((sign, delay, max)) = projector_of(world, held) else {
        return false;
    };
    let own_barrier = world
        .get::<Holosign>(target)
        .is_some_and(|h| h.projector == held)
        && world
            .get::<DmObj>(target)
            .is_some_and(|o| is_under(&o.path, BARRIER));
    if own_barrier {
        remove_sign(world, target);
        return true;
    }
    let Some(&pos) = world.get::<TilePos>(target) else {
        return true;
    };
    let atmos = world
        .get::<DmItem>(held)
        .is_some_and(|i| is_under(&i.path, ATMOS_PROJECTOR));
    if tile_refuses(world, pos, sign, atmos) {
        return true;
    }
    if live_signs(world, held).len() >= max {
        tell(
            world,
            actor.client,
            Span::Warning,
            "Достигнут предел проекций!",
        );
        return true;
    }
    if delay <= 0.0 {
        project(world, held, pos);
    } else if !do_after(world, actor, target, delay, 0, Job::Holosign) {
        tell(
            world,
            actor.client,
            Span::Warning,
            "Проектор уже создаёт голограмму!",
        );
    }
    true
}

/// The do_after's end: the limit and the tile checked again, then the sign.
pub fn complete(world: &mut World, item: Option<Entity>, pos: TilePos) {
    let Some(held) = item else {
        return;
    };
    let Some((sign, _, max)) = projector_of(world, held) else {
        return;
    };
    let atmos = world
        .get::<DmItem>(held)
        .is_some_and(|i| is_under(&i.path, ATMOS_PROJECTOR));
    if live_signs(world, held).len() >= max || tile_refuses(world, pos, sign, atmos) {
        return;
    }
    project(world, held, pos);
}

/// `create_holosign`.
fn project(world: &mut World, projector: Entity, pos: TilePos) {
    let Some((sign, ..)) = projector_of(world, projector) else {
        return;
    };
    let Some(new) = crate::construction::spawn_obj(world, sign, pos) else {
        return;
    };
    world.entity_mut(new).insert(Holosign { projector });
    live_signs(world, projector);
    if let Some(mut list) = world.get_mut::<HoloSigns>(projector) {
        list.0.push(new);
        return;
    }
    world.entity_mut(projector).insert(HoloSigns(vec![new]));
}

/// `attack_self`: the projector's signs are cleared. True when `item` is a projector.
pub fn attack_self(world: &mut World, client: Entity, item: Entity) -> bool {
    if projector_of(world, item).is_none() {
        return false;
    }
    let signs = live_signs(world, item);
    if signs.is_empty() {
        return true;
    }
    for sign in signs {
        remove_sign(world, sign);
    }
    live_signs(world, item);
    tell(world, client, Span::Notice, "Голограммы убраны.");
    true
}

/// `attack_hand` -> `attack_holosign`: a swat for 5 brute, melee click cooldown. True when
/// `target` is a holosign.
pub fn attack_hand(world: &mut World, mob: Entity, target: Entity) -> bool {
    let sign = world
        .get::<DmObj>(target)
        .is_some_and(|o| is_under(&o.path, HOLOSIGN));
    if !sign {
        return false;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    world
        .entity_mut(mob)
        .insert(crate::combat::NextMove(now + crate::combat::CLICK_CD_MELEE));
    crate::obj_damage::take_damage(world, target, 5.0, DamageType::Brute);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projector_kinds_prefer_their_subtype() {
        assert_eq!(
            projector_kind("/obj/item/holosign_creator/atmos").map(|k| k.0),
            Some("/obj/structure/holosign/barrier/atmos")
        );
        assert_eq!(
            projector_kind("/obj/item/holosign_creator/engineering").map(|k| (k.1, k.2)),
            Some((10.0, 12))
        );
        assert_eq!(
            projector_kind("/obj/item/holosign_creator").map(|k| k.0),
            Some("/obj/structure/holosign/wetsign")
        );
        assert_eq!(projector_kind("/obj/item/wrench"), None);
        assert_eq!(
            projector_kind("/obj/item/holosign_creator/robot_seat"),
            None
        );
    }

    /// Signs that were destroyed drop out of the projector's count; removing the projector's
    /// component removes its signs.
    #[test]
    fn signs_follow_their_projector() {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            ss13_world::WorldPlugin,
            HolosignServerPlugin,
        ));
        let world = app.world_mut();
        world
            .resource_mut::<SpatialIndex>()
            .create_level(1, 5, 5)
            .unwrap();
        let projector = world
            .spawn(DmItem {
                path: "/obj/item/holosign_creator/engineering".into(),
                ..default()
            })
            .id();
        let sign = |world: &mut World, x| {
            world
                .spawn((
                    DmObj {
                        path: "/obj/structure/holosign/barrier/engineering".into(),
                        ..default()
                    },
                    Holosign { projector },
                    TilePos::new(x, 1, 1),
                ))
                .id()
        };
        let (a, b) = (sign(world, 1), sign(world, 2));
        world.entity_mut(projector).insert(HoloSigns(vec![a, b]));
        world.despawn(a);
        assert_eq!(live_signs(world, projector), vec![b]);
        assert!(tile_refuses(
            world,
            TilePos::new(2, 1, 1),
            "/obj/structure/holosign/barrier/engineering",
            false
        ));
        assert!(!tile_refuses(
            world,
            TilePos::new(3, 1, 1),
            "/obj/structure/holosign/barrier/engineering",
            false
        ));
        world.despawn(projector);
        app.update();
        assert!(app.world().get_entity(b).is_err(), "the sign went with it");
    }
}
