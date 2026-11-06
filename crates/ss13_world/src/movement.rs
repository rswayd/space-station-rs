//! Tile movement with BYOND/tg rules (`/atom/movable/Move`, `/turf/Enter`, `/client/Move`).

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_shared::{Dir, Facing, TilePos};

use crate::index::SpatialIndex;

/// Blocks movers from entering its tile (BYOND `density = TRUE`, `CanAllowThrough` = `!density`).
/// Walls are entities with `TilePos` + `Density` like any other blocker.
#[derive(Component, Clone, Copy, Debug, Default)]
#[component(
    on_insert = crate::index::on_density_insert,
    on_discard = crate::index::on_density_discard
)]
pub struct Density;

/// tg `/datum/element/climb_walkable` (tables): a dense mover standing on one steps onto
/// another as if it were not dense (`TRAIT_ON_CLIMBABLE` -> `COMSIG_COMPONENT_PERMIT_PASSAGE`).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct ClimbWalkable;

/// tg holobarriers' `CanAllowThrough` (`/obj/structure/holosign/barrier`): a dense blocker a
/// [`PassesWalkBarriers`] mover steps through.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct WalkPassable;

/// A mover that passes [`WalkPassable`] blockers: tg a carbon walking (`MOVE_INTENT_WALK`) or
/// unconscious/in crit. The game keeps it in sync.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct PassesWalkBarriers;

/// tg `ON_BORDER_1`: a dense border object blocks only moves across the tile edges in `edges`
/// (BYOND dir bits): entering its tile from that side (`CanPass` with `border_dir`) and leaving
/// its tile through it (`COMSIG_ATOM_EXIT`). Directional windows, windoors, railings,
/// directional firelocks. Immutable, so every change goes through the index hooks.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
#[component(
    immutable,
    on_insert = crate::index::on_border_insert,
    on_discard = crate::index::on_border_discard
)]
pub struct Border {
    pub edges: u8,
}

impl Border {
    /// A border object facing `dir`. Windows, windoors and firelocks test `border_dir == dir`,
    /// so a diagonal one blocks no cardinal step; railings (`any_bit`) test `border_dir & dir`,
    /// so a diagonal railing blocks both of its edges.
    pub fn facing(dir: Dir, any_bit: bool) -> Self {
        let bits = dir as u8;
        let cardinal = Dir::CARDINALS.contains(&dir);
        Self {
            edges: if cardinal || any_bit { bits } else { 0 },
        }
    }

    pub fn blocks(self, dir: Dir) -> bool {
        self.edges & dir as u8 != 0
    }
}

/// A [`Border`] that blocks without [`Density`]: tg stairs keep movers off their far side
/// through `COMSIG_ATOM_EXIT` and `Cross`, yet are not dense (bullets, throws and crates pass).
/// Insert it with the `Border` (the index files it when the `Border` comes).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct NonDenseBorder;

/// The last stair of a flight going up in this direction (tg `stairs/isTerminator`): a step off
/// its far side from on it climbs to the level above. The server acts on the bump; the client
/// predicts it ([`Border`] + [`NonDenseBorder`] come with it).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClimbsUp(pub Dir);

/// Open space whatever walks onto falls through (the level below exists, nothing like a
/// lattice holds, there is gravity): on the turf entity. The server makes things fall; the
/// client predicts its own mob's fall.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FallsThrough;

/// Something that moves by itself on request.
#[derive(Component, Clone, Copy, Debug)]
pub struct Mover {
    /// Seconds per tile step (tg `movement_delay`); diagonal steps cost `delay * sqrt(2)`.
    pub delay: f32,
    /// `Time::elapsed` in seconds at which the next move is allowed.
    ready_at: f64,
}

impl Mover {
    pub fn new(delay: f32) -> Self {
        Self {
            delay,
            ready_at: 0.0,
        }
    }

    pub fn ready_at(&self) -> f64 {
        self.ready_at
    }
}

/// Ask `entity` to step one tile in `dir`. Validated at the moment of effect: the entity must
/// have `TilePos` and `Mover`, be off cooldown, and the target must be enterable.
#[derive(Message, Clone, Copy, Debug)]
pub struct MoveRequest {
    pub entity: Entity,
    pub dir: Dir,
}

/// A step of `entity` was refused by `obstacle`, a dense entity on the target tile (BYOND
/// `Bump`/`Bumped`). One per blocker, so a door under a dense crate still hears it.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bumped {
    pub entity: Entity,
    pub obstacle: Entity,
}

/// One successful single-tile step. A diagonal move emits two (one per cardinal half), as BYOND
/// enters the intermediate tile.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Moved {
    pub entity: Entity,
    pub from: TilePos,
    pub to: TilePos,
    pub dir: Dir,
}

/// Exclusive: each step must see the index after the previous step in the same tick, otherwise
/// two dense movers could enter one tile.
pub(crate) fn apply_move_requests(
    world: &mut World,
    mut cursor: Local<MessageCursor<MoveRequest>>,
    mut batch: Local<Vec<MoveRequest>>,
) {
    batch.extend(cursor.read(world.resource::<Messages<MoveRequest>>()));
    if batch.is_empty() {
        return;
    }
    let time = world.resource::<Time>();
    let (now, tick) = (time.elapsed_secs_f64(), time.delta_secs_f64());
    for request in batch.drain(..) {
        try_move(world, request, now, tick);
    }
}

fn try_move(world: &mut World, MoveRequest { entity, dir }: MoveRequest, now: f64, tick: f64) {
    let Ok(e) = world.get_entity(entity) else {
        return;
    };
    let (Some(&start), Some(&mover)) = (e.get::<TilePos>(), e.get::<Mover>()) else {
        return;
    };
    if now < mover.ready_at {
        return;
    }

    let target = start.step(dir);
    if let Some(face) = resolve_move(dir, |d| step(world, entity, d)) {
        set_facing(world, entity, face);
    }
    let reached_diagonal =
        !Dir::CARDINALS.contains(&dir) && world.get::<TilePos>(entity) == Some(&target);
    let ready_at = next_ready_at(mover.ready_at, now, tick, mover.delay, reached_diagonal);
    if let Some(mut m) = world.get_mut::<Mover>(entity) {
        m.ready_at = ready_at;
    }
}

/// tg `/atom/movable/Move` for one requested direction, over a `step` that tries one cardinal
/// step and says whether it moved. A diagonal tries the vertical half first, else the horizontal
/// one; the second half failing leaves the mover on the intermediate tile. Returns the direction
/// the mover must face afterwards when that happens (the first half's). Shared with the client's
/// prediction, so both sides walk by the same rules.
pub fn resolve_move(dir: Dir, mut step: impl FnMut(Dir) -> bool) -> Option<Dir> {
    let (dx, dy) = dir.offset();
    if dx == 0 || dy == 0 {
        step(dir);
        return None;
    }
    let v = if dy > 0 { Dir::North } else { Dir::South };
    let h = if dx > 0 { Dir::East } else { Dir::West };
    let first = if step(v) {
        v
    } else if step(h) {
        h
    } else {
        return None;
    };
    let second = if first == v { h } else { v };
    (!step(second)).then_some(first)
}

/// When the next move is allowed after an attempt at `now`, tg `/client/Move`: the delay is spent
/// even when blocked, a reached diagonal costs `delay * sqrt(2)`, and a recent move carries its
/// remainder (within one `tick`) so fractional delays average out.
pub fn next_ready_at(
    ready_at: f64,
    now: f64,
    tick: f64,
    delay: f32,
    reached_diagonal: bool,
) -> f64 {
    let delay = f64::from(delay)
        * if reached_diagonal {
            std::f64::consts::SQRT_2
        } else {
            1.0
        };
    let base = if ready_at + tick > now { ready_at } else { now };
    base + delay
}

/// One cardinal step, BYOND `step()`: turns the mover first (a blocked step still turns it),
/// except at the map edge where there is no target turf. tg `Move`: the old tile's `Exit`
/// (border objects facing `dir`) is asked first and, only if it allows, the new tile's `Enter`;
/// each refusing object hears a `Bumped`.
fn step(world: &mut World, entity: Entity, dir: Dir) -> bool {
    let Some(&from) = world.get::<TilePos>(entity) else {
        return false;
    };
    let to = from.step(dir);
    if !world.resource::<SpatialIndex>().in_bounds(to) {
        return false;
    }
    set_facing(world, entity, dir);
    let index = world.resource::<SpatialIndex>();
    // Empty (no allocation) on a free step.
    let mut blockers: Vec<Entity> = index.exit_blockers(from, dir, entity).collect();
    if blockers.is_empty() {
        let walkable = |e: &Entity| world.get::<ClimbWalkable>(*e).is_some();
        let on_climbable =
            world.get::<Density>(entity).is_some() && index.on_tile(from).iter().any(walkable);
        let passes = world.get::<PassesWalkBarriers>(entity).is_some();
        blockers.extend(
            index
                .enter_blockers(to, dir, entity)
                .filter(|e| !(on_climbable && walkable(e)))
                .filter(|&e| !(passes && world.get::<WalkPassable>(e).is_some())),
        );
    }
    if !blockers.is_empty() {
        for obstacle in blockers {
            world.write_message(Bumped { entity, obstacle });
        }
        return false;
    }
    // Replaces through the component hooks, which move the entity in the index.
    world
        .modify_component(entity, |pos: &mut TilePos| *pos = to)
        .ok();
    world.write_message(Moved {
        entity,
        from,
        to,
        dir,
    });
    true
}

/// Turns only entities that already have `Facing`: things without it (e.g. pushed crates
/// spawned without one) are drawn facing south and stay that way. Writes only on an actual
/// change, so change detection (and replication) fires only when the entity really turns.
fn set_facing(world: &mut World, entity: Entity, dir: Dir) {
    if let Some(mut facing) = world.get_mut::<Facing>(entity) {
        facing.set_if_neq(Facing(dir));
    }
}
