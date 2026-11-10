//! Zero gravity (server): tg `Process_Spacemove`, `get_spacemove_backup` (`mob_movement.dm`),
//! `newtonian_move` (`atoms_movable.dm`), `/datum/drift_handler` and the `weightless` alert of
//! `update_gravity` (`living.dm`).
//!
//! - A living mob over space without gravity is [`Weightless`] and has the `weightless` alert.
//!   Its own step is taken only off a backup within one tile (a wall or another dense turf, a
//!   dense thing, a lattice or catwalk, not what it walks into); otherwise the step is refused.
//!   With no backup at all the mob is `stuck`: its HUD says it cannot move
//!   ([`HealthHudData::immobile`]), so its client predicts no steps.
//! - A step off an unanchored backup (a crate, another mob) pushes the backup the other way.
//! - Whatever moves by a new force with no gravity (a step, the space wind, a push) and has
//!   nothing to hold on to on its new tile drifts on ([`Drift`]): a tile per 0.5 s at 1 N
//!   (`get_loop_delay`), until it comes next to a backup, bumps into something (doors hear the
//!   bump) or reaches gravity. Drifting does not turn it.
//! - Active MOD magboots and magboots ([`crate::modsuit::modules::NoSlip`], the clothing trait,
//!   `TRAIT_NEGATES_GRAVITY`) keep the wearer's footing; flying mobs, the buckled, the pulled and
//!   thrown things do not drift.
//! - An active MOD ion jetpack ([`crate::modsuit::modules::Jetpack`]) with charge or an active
//!   gas jetpack ([`crate::jetpack::GasJetpack`]) with gas lets its wearer step without a backup
//!   and, stabilized, stops its drift; without stabilizers the wearer always drifts, faster by
//!   the jetpack's force.
// shortcut: weightless only over space turfs of space areas (no gravity generators; a floor in a
// space area keeps gravity, which fixture stations rely on) — add gravity generators and
// `force_no_gravity` with the gravity subsystem. Cardinal drift only (a diagonal step drifts along
// its last half); no throw recoil, no floating bob
// (`DO_FLOATING_ANIM` loops forever: it needs replicated state, not an animation event), no
// input block while gliding to a halt.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::{DmItem, DmObj, DmTurfBuild, Turf};
use ss13_shared::chat::Span;
use ss13_shared::{Dir, Facing, TilePos};
use ss13_tgui::server::{OpenUi, UiSystems};
use ss13_world::{Border, Bumped, Density, MoveRequest, Moved, SpatialIndex, WorldSystems};

use crate::breath::{AlertCategory, Alerts};
use crate::health::{Health, HealthSystems, Stat};
use crate::hud::{HealthHudData, HealthHudUi};
use crate::obj_damage::{Anchored, is_under};
use crate::server::PlayerMob;
use crate::status_effects::Incapacitation;
use crate::tools::tell;

const SPACE_TURF: &str = "/turf/open/space";
const LATTICE: &str = "/obj/structure/lattice";
const DIAGONALS: [Dir; 4] = [
    Dir::NorthEast,
    Dir::NorthWest,
    Dir::SouthEast,
    Dir::SouthWest,
];

/// `DEFAULT_INERTIA_SPEED`, deciseconds per tile.
const DEFAULT_INERTIA_SPEED: f32 = 5.0;
/// `INERTIA_FORCE_CAP`, newtons.
const INERTIA_FORCE_CAP: f32 = 25.0;
/// `INERTIA_SPEED_COEF`.
const INERTIA_SPEED_COEF: f32 = 0.375;

/// `/datum/drift_handler/get_loop_delay` at `force` newtons, in seconds per tile.
pub fn drift_delay(force: f32) -> f64 {
    f64::from(DEFAULT_INERTIA_SPEED / ((1.0 - INERTIA_SPEED_COEF) + force * INERTIA_SPEED_COEF))
        / 10.0
}

/// `get_loop_delay` of `e`: [`drift_delay`] times its inertia multiplier (a jetpack's).
fn loop_delay(world: &World, e: Entity, force: f32) -> f64 {
    drift_delay(force)
        * crate::modsuit::modules::inertia_multiplier(world, e)
        * crate::jetpack::inertia_multiplier(world, e)
}

/// tg `drift_handler`: the entity drifts a tile in `dir` every [`drift_delay`] of `force`.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Drift {
    pub dir: Dir,
    pub force: f32,
    /// `Time::elapsed` of the next drift step.
    pub next_at: f64,
}

/// On a living mob without gravity (tg `ALERT_GRAVITY` weightless); `stuck`: there is nothing
/// within a tile to push off.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Weightless {
    pub stuck: bool,
}

/// tg `last_pushoff`: when this pushed off something or was pushed off; for that tick it is
/// no backup, so a pushed mob does not hold on to its pusher.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct LastPushoff(pub f64);

/// A weightless mob's own step (`crate::server::forward_moves`), taken only off a backup.
#[derive(Message, Clone, Copy, Debug)]
pub struct SpaceStep {
    pub mob: Entity,
    pub dir: Dir,
}

pub struct SpaceMovePlugin;

impl Plugin for SpaceMovePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<SpaceStep>()
            .add_systems(
                Update,
                space_steps
                    .after(crate::server::forward_moves)
                    .before(WorldSystems::Movement),
            )
            .add_systems(
                Update,
                (drift, track_weightless, weightless_hud)
                    .chain()
                    .after(WorldSystems::Movement)
                    .after(HealthSystems)
                    .before(UiSystems::Open),
            );
    }
}

fn on_space_turf(world: &World, pos: TilePos) -> bool {
    crate::construction::turf_at(world, pos)
        .and_then(|t| world.get::<DmTurfBuild>(t))
        .is_some_and(|b| is_under(&b.path, SPACE_TURF))
}

/// `mob_negates_gravity`: `TRAIT_NEGATES_GRAVITY` from active MOD magboots or worn magboots.
pub fn negates_gravity(world: &World, e: Entity) -> bool {
    use crate::clothing::traits::{NEGATES_GRAVITY, has_trait};
    world.get::<crate::modsuit::modules::NoSlip>(e).is_some()
        || has_trait(world, e, NEGATES_GRAVITY)
}

/// `!has_gravity()` of `e` at `pos`: `mob_negates_gravity` counts, and a forced gravity of 0
/// (the MOD anti-gravity module, `/datum/element/forced_gravity`) floats it anywhere.
pub fn weightless_at(world: &World, e: Entity, pos: TilePos) -> bool {
    if negates_gravity(world, e) {
        return false;
    }
    world
        .get::<crate::gravity::ForcedGravity>(e)
        .is_some_and(|g| g.0 <= 0.0)
        || (!crate::multiz::has_gravity(world, pos) && on_space_turf(world, pos))
}

fn is_lattice(world: &World, e: Entity) -> bool {
    world
        .get::<DmObj>(e)
        .is_some_and(|o| is_under(&o.path, LATTICE))
}

/// tg `anchored`: items and structures by their state, mobs never, anything else (walls,
/// effects) always.
fn anchored(world: &World, e: Entity) -> bool {
    if let Some(item) = world.get::<DmItem>(e) {
        return item.anchored;
    }
    if world.get::<Health>(e).is_some() {
        return false;
    }
    if world.get::<DmObj>(e).is_some() {
        return world.get::<Anchored>(e).is_none_or(|a| a.0);
    }
    true
}

/// What can drift at all: mobs, items and structures; not projectiles, mechs or their pilots.
fn drifts(world: &World, e: Entity) -> bool {
    let Ok(entity) = world.get_entity(e) else {
        return false;
    };
    (entity.contains::<Health>() || entity.contains::<DmItem>() || entity.contains::<DmObj>())
        && !entity.contains::<crate::projectile::Projectile>()
        && !entity.contains::<crate::mecha::Mech>()
        && !entity.contains::<crate::mecha::Pilot>()
}

fn dir_of(dx: i32, dy: i32) -> Option<Dir> {
    Dir::DMI_ORDER.into_iter().find(|d| d.offset() == (dx, dy))
}

/// tg `/mob/get_spacemove_backup(moving_direction, continuous_move)`: something within a tile
/// of `pos` to push off; one in the priority directions (the cardinals for a cardinal move,
/// else the diagonals) or on the mob's own tile first.
pub fn spacemove_backup(
    world: &World,
    e: Entity,
    pos: TilePos,
    dir: Option<Dir>,
    continuous: bool,
) -> Option<Entity> {
    let priority: &[Dir] = if dir.is_some_and(|d| Dir::CARDINALS.contains(&d)) {
        &Dir::CARDINALS
    } else {
        &DIAGONALS
    };
    let index = world.resource::<SpatialIndex>();
    let buckled_to = world
        .get::<crate::medical::surgery::Buckled>(e)
        .map(|b| b.0);
    let pulling = world.get::<crate::pull::Pulling>(e).map(|p| p.0);
    let mut secondary = None;
    let now = world.resource::<Time>().elapsed_secs_f64();
    for dy in -1..=1 {
        for dx in -1..=1 {
            let at = TilePos::new(pos.x + dx, pos.y + dy, pos.z);
            if !index.in_bounds(at) {
                continue;
            }
            let towards = dir_of(dx, dy);
            let is_priority = towards.is_none_or(|d| priority.contains(&d));
            for &thing in index.on_tile(at) {
                if thing == e {
                    continue;
                }
                let dense = world.get::<Density>(thing).is_some();
                if world.get::<Turf>(thing).is_some() {
                    // Floors hold only with gravity, which a weightless mob lacks here.
                    if on_space_turf(world, at) || !dense {
                        continue;
                    }
                } else {
                    if Some(thing) == buckled_to
                        || world
                            .get::<crate::medical::surgery::Buckled>(thing)
                            .is_some()
                    {
                        continue;
                    }
                    if !dense && !is_lattice(world, thing) {
                        continue;
                    }
                    if world.get::<LastPushoff>(thing).is_some_and(|p| p.0 == now) {
                        continue;
                    }
                    // `CanPass(src, get_dir(rebound, src))`: a border object lets through
                    // what does not cross its edges.
                    let pass_allowed = !dense
                        || world
                            .get::<Border>(thing)
                            .is_some_and(|b| towards.is_none_or(|d| !b.blocks(d.reverse())));
                    if !pass_allowed && towards.is_some() && towards == dir {
                        // Not off what it walks into; while drifting, not off what drifts along.
                        if !continuous
                            || world
                                .get::<Drift>(thing)
                                .is_some_and(|d| Some(d.dir) == dir)
                        {
                            continue;
                        }
                    }
                    if !anchored(world, thing) && Some(thing) == pulling {
                        continue;
                    }
                }
                if is_priority {
                    return Some(thing);
                }
                secondary = Some(thing);
            }
        }
    }
    secondary
}

/// tg `Process_Spacemove(movement_dir, continuous_move)`: whether `e` may move (or must stop
/// drifting). A mob's own step (`continuous` false) off an unanchored backup pushes it away.
pub fn process_spacemove(world: &mut World, e: Entity, dir: Option<Dir>, continuous: bool) -> bool {
    let Some(&pos) = world.get::<TilePos>(e) else {
        return true;
    };
    if !weightless_at(world, e, pos) || holds_on(world, e) {
        return true;
    }
    if world.get::<Health>(e).is_none() {
        // `/atom/movable/handle_spacemove_grabbing`.
        return lattice_near(world, pos);
    }
    // `COMSIG_MOVABLE_SPACEMOVE` (the jetpack) and `TRAIT_NOGRAV_ALWAYS_DRIFT`.
    if crate::modsuit::modules::jetpack_holds(world, e, dir, continuous)
        || crate::jetpack::holds(world, e, dir, continuous)
    {
        return true;
    }
    if crate::modsuit::modules::always_drifts(world, e) || crate::jetpack::always_drifts(world, e) {
        return false;
    }
    let Some(backup) = spacemove_backup(world, e, pos, dir, continuous) else {
        return false;
    };
    let Some(dir) = dir else {
        return true;
    };
    if continuous || world.get::<Turf>(backup).is_some() || anchored(world, backup) {
        return true;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    world.entity_mut(e).insert(LastPushoff(now));
    if !newtonian_move(world, backup, dir.reverse(), true, 1.0) {
        return true;
    }
    world.entity_mut(backup).insert(LastPushoff(now));
    if let Some(client) = world.get::<PlayerMob>(e).map(|p| p.client) {
        let name = crate::medical::name_of(world, backup);
        tell(
            world,
            client,
            Span::Info,
            format!("Вы отталкиваетесь от {name} для движения."),
        );
    }
    true
}

/// The early returns of `Process_Spacemove` that need no backup: pulled, thrown, not on a turf,
/// buckled, flying.
fn holds_on(world: &World, e: Entity) -> bool {
    let Ok(entity) = world.get_entity(e) else {
        return true;
    };
    entity.contains::<crate::pull::PulledBy>()
        || entity.contains::<crate::throw::Thrown>()
        || entity.contains::<crate::closets::InCloset>()
        || entity.contains::<crate::medical::surgery::Buckled>()
        || crate::multiz::flies(world, e)
}

/// On a tile without gravity and holding on to nothing that needs no backup.
fn floating(world: &World, e: Entity) -> bool {
    world
        .get::<TilePos>(e)
        .is_some_and(|&pos| weightless_at(world, e, pos))
        && !holds_on(world, e)
}

fn lattice_near(world: &World, pos: TilePos) -> bool {
    let index = world.resource::<SpatialIndex>();
    (-1..=1).any(|dy| {
        (-1..=1).any(|dx| {
            let at = TilePos::new(pos.x + dx, pos.y + dy, pos.z);
            index.in_bounds(at) && index.on_tile(at).iter().any(|&t| is_lattice(world, t))
        })
    })
}

/// tg `newtonian_move(dir, instant, drift_force)`: `e` starts drifting, or its drift takes the
/// impulse (`drift_handler/newtonian_impulse`: a weaker push the same way changes nothing).
/// False when it holds on and does not drift.
pub fn newtonian_move(world: &mut World, e: Entity, dir: Dir, instant: bool, force: f32) -> bool {
    if !drifts(world, e) || process_spacemove(world, e, Some(dir), true) {
        return false;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    let force = force.clamp(0.0, INERTIA_FORCE_CAP);
    if let Some(old) = world.get::<Drift>(e).copied() {
        if force < old.force && old.dir == dir {
            return true;
        }
        if force < 0.1 {
            world.entity_mut(e).remove::<Drift>();
            return true;
        }
        let next = now + loop_delay(world, e, force);
        world.entity_mut(e).insert(Drift {
            dir,
            force,
            next_at: if instant { now } else { old.next_at.min(next) },
        });
        return true;
    }
    if force < 0.1 {
        return false;
    }
    let next_at = if instant {
        now
    } else {
        now + loop_delay(world, e, force)
    };
    world.entity_mut(e).insert(Drift {
        dir,
        force,
        next_at,
    });
    true
}

/// The weightless mobs' own steps: taken off a backup, refused without one (tg `client/Move`
/// returns before spending the move delay).
fn space_steps(world: &mut World, mut cursor: Local<MessageCursor<SpaceStep>>) {
    let steps: Vec<SpaceStep> = cursor
        .read(world.resource::<Messages<SpaceStep>>())
        .copied()
        .collect();
    for SpaceStep { mob, dir } in steps {
        if world.get_entity(mob).is_ok() && process_spacemove(world, mob, Some(dir), false) {
            world.write_message(MoveRequest { entity: mob, dir });
        }
    }
}

/// `Moved` -> `newtonian_move` of every new move, then the drift loop's steps.
// Cost: per tick, one gravity lookup per moved thing (an area lookup and a path compare; a
// 3x3 scan only for those without gravity) plus the drifting things.
fn drift(
    world: &mut World,
    mut cursor: Local<MessageCursor<Moved>>,
    mut drifting: Local<QueryState<(Entity, &'static Drift)>>,
) {
    let moved: Vec<(Entity, Dir)> = cursor
        .read(world.resource::<Messages<Moved>>())
        .map(|m| (m.entity, m.dir))
        .collect();
    for (e, dir) in moved {
        if world.get_entity(e).is_err() {
            continue;
        }
        // `drift_handler/handle_move`: a move of its own that finds a hold ends the drift.
        if let Some(d) = world.get::<Drift>(e).copied()
            && process_spacemove(world, e, Some(d.dir), true)
        {
            world.entity_mut(e).remove::<Drift>();
            continue;
        }
        newtonian_move(world, e, dir, false, 1.0);
    }
    let time = world.resource::<Time>();
    let (now, tick) = (time.elapsed_secs_f64(), time.delta_secs_f64());
    let due: Vec<(Entity, Drift)> = drifting
        .iter(world)
        .filter(|(_, d)| d.next_at <= now)
        .map(|(e, d)| (e, *d))
        .collect();
    for (e, d) in due {
        // tg checks for a hold only after each drift step; before it, only what ends a drift
        // outright (picked up, pulled, thrown, gravity).
        if !drifts(world, e) || !floating(world, e) {
            world.entity_mut(e).remove::<Drift>();
            continue;
        }
        let facing = world.get::<Facing>(e).copied();
        let result = crate::pull::step_once(world, e, d.dir, None);
        // `drift_handler/after_move`: `setDir(old_dir)`.
        if let Some(facing) = facing
            && let Some(mut now_facing) = world.get_mut::<Facing>(e)
        {
            now_facing.set_if_neq(facing);
        }
        match result {
            Ok(()) if !process_spacemove(world, e, Some(d.dir), true) => {
                // Carries the remainder within one tick, like `next_ready_at`.
                let base = if d.next_at + tick > now {
                    d.next_at
                } else {
                    now
                };
                let delay = loop_delay(world, e, d.force);
                if let Some(mut drift) = world.get_mut::<Drift>(e) {
                    drift.next_at = base + delay;
                }
            }
            Ok(()) => {
                world.entity_mut(e).remove::<Drift>();
            }
            Err(blockers) => {
                for obstacle in blockers {
                    world.write_message(Bumped {
                        entity: e,
                        obstacle,
                        diagonal: !Dir::CARDINALS.contains(&d.dir),
                    });
                }
                world.entity_mut(e).remove::<Drift>();
            }
        }
    }
    // The drift's own steps are no new force.
    cursor.clear(world.resource::<Messages<Moved>>());
}

/// Files [`Weightless`] on living mobs that moved or are weightless.
// Cost: per tick, the mobs that moved plus the weightless ones (a 3x3 scan each).
fn track_weightless(
    world: &World,
    mobs: Query<
        (Entity, &TilePos, Option<&Weightless>),
        (
            With<Health>,
            Or<(
                Changed<TilePos>,
                With<Weightless>,
                Changed<crate::gravity::ForcedGravity>,
            )>,
        ),
    >,
    mut commands: Commands,
) {
    for (mob, &pos, current) in &mobs {
        let state = weightless_at(world, mob, pos).then(|| Weightless {
            stuck: !holds_on(world, mob)
                && !crate::modsuit::modules::jetpack_ready(world, mob)
                && !crate::jetpack::ready(world, mob)
                && spacemove_backup(world, mob, pos, None, false).is_none(),
        });
        match (state, current) {
            (Some(state), Some(&current)) if state == current => {}
            (Some(state), _) => {
                commands.entity(mob).insert(state);
            }
            (None, Some(_)) => {
                commands.entity(mob).remove::<Weightless>();
            }
            (None, None) => {}
        }
    }
}

type HudItem<'a> = (
    Option<&'a Weightless>,
    Option<&'a mut Alerts>,
    Option<&'a mut HealthHudData>,
    &'a Health,
    &'a Stat,
    Option<&'a Incapacitation>,
    Option<&'a PlayerMob>,
);

/// The `weightless` alert, and a stuck mob's HUD says it cannot move (on top of `body_state`).
fn weightless_hud(
    mut mobs: ParamSet<(
        Query<Entity, Or<(Changed<Weightless>, Changed<HealthHudData>)>>,
        Query<HudItem>,
    )>,
    mut removed: RemovedComponents<Weightless>,
    mut open: MessageWriter<OpenUi>,
) {
    let mut touched: Vec<Entity> = mobs.p0().iter().collect();
    touched.extend(removed.read());
    let mut all = mobs.p1();
    for mob in touched {
        let Ok((weightless, alerts, hud, health, stat, traits, player)) = all.get_mut(mob) else {
            continue;
        };
        if let Some(mut alerts) = alerts {
            // `crate::gravity` shares the category with the high gravity alerts.
            let shown = alerts
                .get(AlertCategory::Gravity)
                .is_some_and(|a| a.state == "weightless");
            if weightless.is_some() && !shown {
                alerts.throw(AlertCategory::Gravity, "weightless", 0);
            } else if weightless.is_none() && shown {
                alerts.clear(AlertCategory::Gravity);
            }
        }
        if let Some(mut hud) = hud {
            let base = !health.can_move(*stat) || traits.is_some_and(|t| t.immobilized);
            let immobile = base || weightless.is_some_and(|w| w.stuck);
            if hud.immobile != immobile {
                hud.immobile = immobile;
                if let Some(player) = player {
                    open.write(OpenUi::new::<HealthHudUi>(player.client, mob, mob, ""));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drift_delay_follows_get_loop_delay() {
        // 1 N: 5 / (0.625 + 0.375) ds.
        assert!((drift_delay(1.0) - 0.5).abs() < 1e-9);
        // The force cap: 5 / (0.625 + 9.375) = 0.5 ds, a tile per tick.
        assert!((drift_delay(INERTIA_FORCE_CAP) - 0.05).abs() < 1e-6);
    }

    #[test]
    fn dir_of_offsets() {
        assert_eq!(dir_of(0, 0), None);
        assert_eq!(dir_of(1, 0), Some(Dir::East));
        assert_eq!(dir_of(-1, -1), Some(Dir::SouthWest));
    }
}
