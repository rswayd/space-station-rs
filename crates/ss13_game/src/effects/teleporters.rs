//! Map teleporters: step triggers (`code/game/objects/effects/step_triggers.dm`), bump
//! teleporters (`bump_teleporter.dm`) and permanent portals (`portals.dm`), configured by map
//! var edits ([`ss13_mapload::DmTrigger`]).
//!
//! - step triggers act on whatever steps onto their tile (`on_entered`): `message` (a line to
//!   the mob's player, once by default), `teleporter` (absolute), `teleporter/random` (within a
//!   box), `teleporter/offset` (relative: looping hallways), `teleport_fancy` (a `Move` with
//!   sparks and smoke, limited uses), `sound_effect` (a sound, to everyone or only the
//!   triggerer); ghosts only with `affect_ghosts`, non-mobs not with `mobs_only`;
//! - bump teleporters (dense, invisible): whatever bumps one is moved onto the teleporter whose
//!   `id` is its `id_target` (BandaStation: any movable, not only mobs);
//! - permanent portals: whatever bumps or uses one goes onto the other permanent portal with the
//!   same `id`; `one_way` goes to a random `/obj/effect/landmark/portal_exit` with its `id`,
//!   `one_use` closes after carrying something alive.
//!
//! Cost: per step one look at the target tile's contents; per bump of a teleporter one pass over
//! the triggers.
// shortcut: no `thrower` (move loops, only on the museum away mission) and no `outfitter` (event
// maps, outfit datums): they are inert, as is `stopper`. Only steps trigger: something put on a
// trigger's tile by a teleport does not set it off (tg `on_entered` runs on `forceMove` too; the
// offset teleporter guards against exactly that loop). `bump_teleporter/filtering` moves like
// the plain one (no `TRAIT_NO_TELEPORT` here yet). Ghosts cannot click portals.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::DmTrigger;
use ss13_mapload::triggers::{
    BUMP_TELEPORTER_ROOT, PERMANENT_PORTAL_ROOT, PORTAL_EXIT_ROOT, STEP_TRIGGER_ROOT,
};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_world::{Bumped, Moved, SpatialIndex};

use crate::health::Health;
use crate::obj_damage::is_under;

/// `INVISIBILITY_ABSTRACT`.
const INVISIBILITY_ABSTRACT: u8 = 101;
const EFFECT: &str = "/obj/effect";
const MECHA: &str = "/obj/vehicle/sealed/mecha";
const ONE_WAY: &str = "/obj/effect/portal/permanent/one_way";
const ONE_USE: &str = "/obj/effect/portal/permanent/one_way/one_use";
/// `SFX_PORTAL_ENTER`, `SFX_PORTAL_CLOSE`, `SHORT_RANGE_SOUND_EXTRARANGE`.
const SFX_ENTER: &str = "portal_enter";
const SFX_CLOSE: &str = "portal_close";
const SHORT_RANGE_EXTRARANGE: f32 = -7.0;

/// What a step trigger does, by its type (most specific first).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepKind {
    Message,
    TeleportRandom,
    TeleportOffset,
    Teleport,
    TeleportFancy,
    Sound,
    /// `stopper`, `thrower`, `outfitter` and the base type.
    Inert,
}

pub fn step_kind(path: &str) -> StepKind {
    const KINDS: [(&str, StepKind); 6] = [
        ("/obj/effect/step_trigger/message", StepKind::Message),
        (
            "/obj/effect/step_trigger/teleporter/random",
            StepKind::TeleportRandom,
        ),
        (
            "/obj/effect/step_trigger/teleporter/offset",
            StepKind::TeleportOffset,
        ),
        ("/obj/effect/step_trigger/teleporter", StepKind::Teleport),
        (
            "/obj/effect/step_trigger/teleport_fancy",
            StepKind::TeleportFancy,
        ),
        ("/obj/effect/step_trigger/sound_effect", StepKind::Sound),
    ];
    KINDS
        .iter()
        .find(|(root, _)| is_under(path, root))
        .map_or(StepKind::Inert, |&(_, k)| k)
}

/// The type of a trigger, portal or portal exit.
fn obj_path(world: &World, e: Entity) -> Option<String> {
    world.get::<DmTrigger>(e).map(|t| t.path.clone())
}

/// The triggers of `root` on `pos`.
fn on_tile_under(world: &World, pos: TilePos, root: &str) -> Vec<Entity> {
    super::on_tile(world, pos)
        .into_iter()
        .filter(|&e| {
            world.get::<DmTrigger>(e).is_some()
                && obj_path(world, e).is_some_and(|p| is_under(&p, root))
        })
        .collect()
}

fn is_mob(world: &World, e: Entity) -> bool {
    world.get::<Health>(e).is_some() || world.get::<crate::ghost::Ghost>(e).is_some()
}

/// `forceMove(dest)`.
fn force_move(world: &mut World, e: Entity, dest: TilePos) {
    if let Ok(mut entity) = world.get_entity_mut(e) {
        entity.insert(dest);
    }
}

fn in_bounds(world: &World, pos: TilePos) -> bool {
    world.resource::<SpatialIndex>().in_bounds(pos)
}

/// DM `locate(x, y, z)` of numeric vars: `None` off the map.
fn locate(world: &World, x: f32, y: f32, z: f32) -> Option<TilePos> {
    if z < 0.0 || z > f32::from(u16::MAX) {
        return None;
    }
    let pos = TilePos::new(x as i32, y as i32, z as u16);
    in_bounds(world, pos).then_some(pos)
}

/// DM `rand(a, b)` on integers, either order.
fn rand_between(world: &mut World, a: f32, b: f32) -> f32 {
    let (lo, hi) = (a.min(b) as i64, a.max(b) as i64);
    let span = (hi - lo).clamp(0, i64::from(u32::MAX - 1)) as u32;
    let roll = world
        .get_resource_or_init::<crate::combat::CombatRng>()
        .range(0, span);
    (lo + i64::from(roll)) as f32
}

/// `on_entered` of the step triggers for every step.
pub(super) fn step_on_triggers(world: &mut World, mut cursor: Local<MessageCursor<Moved>>) {
    let moved: Vec<Moved> = cursor
        .read(world.resource::<Messages<Moved>>())
        .copied()
        .collect();
    for m in moved {
        for trigger in on_tile_under(world, m.to, STEP_TRIGGER_ROOT) {
            entered(world, trigger, m.entity);
        }
    }
}

/// `step_trigger/on_entered`.
fn entered(world: &mut World, trigger: Entity, who: Entity) {
    if trigger == who || world.get_entity(who).is_err() {
        return;
    }
    let Some(dm) = world.get::<DmTrigger>(trigger).cloned() else {
        return;
    };
    let invisible = world
        .get::<ss13_mapload::DmAtom>(who)
        .is_some_and(|a| a.invisibility >= INVISIBILITY_ABSTRACT);
    if invisible {
        return;
    }
    if world.get::<crate::ghost::Ghost>(who).is_some() && !dm.affect_ghosts {
        return;
    }
    if dm.mobs_only && !is_mob(world, who) {
        return;
    }
    let (Some(path), Some(&here)) = (obj_path(world, trigger), world.get::<TilePos>(trigger))
    else {
        return;
    };
    match step_kind(&path) {
        StepKind::Message => {
            let Some(client) = world.get::<crate::server::PlayerMob>(who).map(|p| p.client) else {
                return;
            };
            crate::tools::tell(world, client, Span::Info, dm.message.clone());
            if dm.once {
                world.despawn(trigger);
            }
        }
        StepKind::Teleport => {
            if dm.teleport_x != 0.0 && dm.teleport_y != 0.0 && dm.teleport_z != 0.0 {
                // `forceMove(null)` off the map is refused here instead of nullspacing.
                if let Some(dest) = locate(world, dm.teleport_x, dm.teleport_y, dm.teleport_z) {
                    force_move(world, who, dest);
                }
            }
        }
        StepKind::TeleportRandom => {
            let set = [
                dm.teleport_x,
                dm.teleport_y,
                dm.teleport_z,
                dm.teleport_x_offset,
                dm.teleport_y_offset,
                dm.teleport_z_offset,
            ];
            if set.iter().all(|&v| v != 0.0) {
                let x = rand_between(world, dm.teleport_x, dm.teleport_x_offset);
                let y = rand_between(world, dm.teleport_y, dm.teleport_y_offset);
                let z = rand_between(world, dm.teleport_z, dm.teleport_z_offset);
                if let Some(dest) = locate(world, x, y, z) {
                    force_move(world, who, dest);
                }
            }
        }
        StepKind::TeleportOffset => {
            // A step always comes from an adjacent tile (`old_loc.Adjacent(loc)`).
            let dest = TilePos::new(
                here.x + dm.teleport_x_offset as i32,
                here.y + dm.teleport_y_offset as i32,
                here.z,
            );
            if in_bounds(world, dest) {
                force_move(world, who, dest);
            }
        }
        StepKind::TeleportFancy => fancy(world, trigger, who, &dm, here),
        StepKind::Sound => {
            let Some(&at) = world.get::<TilePos>(who) else {
                return;
            };
            if dm.triggerer_only && is_mob(world, who) {
                crate::hallucination::play_local(
                    world,
                    who,
                    Some(at),
                    &dm.sound,
                    dm.volume,
                    dm.freq_vary,
                );
            } else {
                crate::sound::playsound(
                    world,
                    crate::sound::PlaySound::new(at, dm.sound.clone(), dm.volume, dm.freq_vary)
                        .extrarange(dm.extra_range),
                );
            }
            if dm.happens_once {
                world.despawn(trigger);
            }
        }
        StepKind::Inert => {}
    }
}

/// `teleport_fancy/Trigger`: `Move` to the destination (refused onto dense things), sparks and
/// smoke at either end, one use gone (`0` uses: never runs out, as `uses--` skips `0`).
fn fancy(world: &mut World, trigger: Entity, who: Entity, dm: &DmTrigger, here: TilePos) {
    let dest = locate(world, dm.locationx, dm.locationy, f32::from(here.z));
    let free = dest.is_some_and(|d| world.resource::<SpatialIndex>().dense_on_tile(d).is_empty());
    if let Some(dest) = dest.filter(|_| free) {
        force_move(world, who, dest);
    }
    if dm.entersparks {
        super::particles::do_sparks(world, 4, true, here);
    }
    if dm.entersmoke {
        super::smoke::do_smoke(world, 4, here, None, super::smoke::SmokeKind::Basic);
    }
    if let Some(dest) = dest {
        if dm.exitsparks {
            super::particles::do_sparks(world, 4, true, dest);
        }
        if dm.exitsmoke {
            super::smoke::do_smoke(world, 4, dest, None, super::smoke::SmokeKind::Basic);
        }
    }
    let uses = dm.uses - 1.0;
    if uses == 0.0 {
        world.despawn(trigger);
    } else if let Some(mut live) = world.get_mut::<DmTrigger>(trigger) {
        live.uses = uses;
    }
}

/// `Bumped` of bump teleporters and permanent portals.
pub(super) fn bump_teleporters(world: &mut World, mut cursor: Local<MessageCursor<Bumped>>) {
    let pending: Vec<Bumped> = cursor
        .read(world.resource::<Messages<Bumped>>())
        .copied()
        .collect();
    for Bumped { entity, obstacle } in pending {
        if world.get::<DmTrigger>(obstacle).is_none() {
            continue;
        }
        let Some(path) = obj_path(world, obstacle) else {
            continue;
        };
        if is_under(&path, BUMP_TELEPORTER_ROOT) {
            bump_teleport(world, obstacle, entity);
        } else if is_under(&path, PERMANENT_PORTAL_ROOT) {
            portal_teleport(world, obstacle, entity);
        }
    }
}

/// Every trigger of `root` with `id`, in query order.
fn with_id(world: &mut World, root: &str, id: &str, except: Entity) -> Vec<TilePos> {
    let mut q = world.query::<(Entity, &DmTrigger, &TilePos)>();
    let found: Vec<(Entity, TilePos)> = q
        .iter(world)
        .filter(|(e, t, _)| *e != except && t.id == id)
        .map(|(e, _, &p)| (e, p))
        .collect();
    found
        .into_iter()
        .filter(|&(e, _)| obj_path(world, e).is_some_and(|p| is_under(&p, root)))
        .map(|(_, p)| p)
        .collect()
}

/// `bump_teleporter/Bumped`: onto the first teleporter whose `id` is our `id_target`.
fn bump_teleport(world: &mut World, teleporter: Entity, bumper: Entity) {
    let Some(target) = world
        .get::<DmTrigger>(teleporter)
        .map(|t| t.id_target.clone())
    else {
        return;
    };
    if target.is_empty() {
        warn!("bump teleporter {teleporter:?} has no id_target set");
        return;
    }
    match with_id(world, BUMP_TELEPORTER_ROOT, &target, Entity::PLACEHOLDER).first() {
        Some(&dest) => force_move(world, bumper, dest),
        None => warn!("bump teleporter {teleporter:?} found no teleporter with id {target}"),
    }
}

/// Where a permanent portal sends things now (`set_linked` + `get_link_target_turf`): another
/// permanent portal with its `id`, or for `one_way` a random portal exit with it.
fn portal_target(world: &mut World, portal: Entity, path: &str) -> Option<TilePos> {
    let id = world.get::<DmTrigger>(portal)?.id.clone();
    if id.is_empty() {
        return None;
    }
    if is_under(path, ONE_WAY) {
        let mut exits = with_id(world, PORTAL_EXIT_ROOT, &id, portal);
        exits.sort_unstable_by_key(|p| (p.z, p.y, p.x));
        exits.dedup();
        if exits.is_empty() {
            return None;
        }
        let i = rand_between(world, 0.0, (exits.len() - 1) as f32) as usize;
        return exits.get(i).copied();
    }
    with_id(world, PERMANENT_PORTAL_ROOT, &id, portal)
        .first()
        .copied()
}

/// `portal/teleport(moving)` of a permanent portal (`force_teleport`). True when it moved.
pub fn portal_teleport(world: &mut World, portal: Entity, moving: Entity) -> bool {
    let Some(path) = obj_path(world, portal) else {
        return false;
    };
    let moving_path = crate::construction::path_of(world, moving);
    let mob = is_mob(world, moving);
    // Effects, mechs (`mech_sized = FALSE`), and anything neither an object nor a mob stay.
    if moving_path
        .as_deref()
        .is_some_and(|p| is_under(p, EFFECT) || is_under(p, MECHA))
        || (moving_path.is_none() && !mob)
    {
        return false;
    }
    let anchored = world
        .get::<crate::obj_damage::Anchored>(moving)
        .map_or_else(
            || {
                world
                    .get::<ss13_mapload::DmObj>(moving)
                    .is_some_and(|o| o.anchored)
            },
            |a| a.0,
        );
    if anchored {
        return false;
    }
    let Some(dest) = portal_target(world, portal, &path) else {
        return false;
    };
    let Some(&start) = world.get::<TilePos>(moving) else {
        return false;
    };
    force_move(world, moving, dest);
    for pos in [start, dest] {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, SFX_ENTER, 50.0, true)
                .extrarange(SHORT_RANGE_EXTRARANGE),
        );
    }
    let dead = world.get::<crate::health::Stat>(moving) == Some(&crate::health::Stat::Dead);
    if is_under(&path, ONE_USE) && !dead {
        // `expire()`.
        if let Some(&here) = world.get::<TilePos>(portal) {
            crate::sound::playsound(
                world,
                crate::sound::PlaySound::new(here, SFX_CLOSE, 50.0, false)
                    .extrarange(SHORT_RANGE_EXTRARANGE),
            );
        }
        world.despawn(portal);
    }
    true
}

/// `portal/attack_hand` and `item_interaction`: a click from next to a permanent portal steps
/// the clicker through. True when the click was a portal's.
pub fn portal_click(world: &mut World, mob: Entity, mob_pos: TilePos, target: Entity) -> bool {
    if world.get::<DmTrigger>(target).is_none()
        || !obj_path(world, target).is_some_and(|p| is_under(&p, PERMANENT_PORTAL_ROOT))
    {
        return false;
    }
    let near = world
        .get::<TilePos>(target)
        .and_then(|&p| p.chebyshev(mob_pos))
        .is_some_and(|d| d <= 1);
    if !near {
        return false;
    }
    portal_teleport(world, target, mob);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_kinds_follow_the_subtype() {
        assert_eq!(
            step_kind("/obj/effect/step_trigger/teleporter/offset"),
            StepKind::TeleportOffset
        );
        assert_eq!(
            step_kind("/obj/effect/step_trigger/teleporter/random"),
            StepKind::TeleportRandom
        );
        assert_eq!(
            step_kind("/obj/effect/step_trigger/teleporter"),
            StepKind::Teleport
        );
        assert_eq!(
            step_kind("/obj/effect/step_trigger/sound_effect/lavaland_cult_altar"),
            StepKind::Sound
        );
        assert_eq!(
            step_kind("/obj/effect/step_trigger/message"),
            StepKind::Message
        );
        assert_eq!(
            step_kind("/obj/effect/step_trigger/thrower"),
            StepKind::Inert
        );
        assert_eq!(step_kind("/obj/effect/step_trigger"), StepKind::Inert);
    }
}
