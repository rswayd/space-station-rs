//! AI movement, after tg `/datum/ai_movement/basic_avoidance` (`SSmove_manager.move_to`: step
//! towards the target, sidestepping blockers, until within `min_distance`) and `jps` for the
//! detours: when no step towards the target is free, a bounded A* over cardinal steps finds the
//! way around. A target that stays unreachable fails the movement (tg
//! `COMSIG_MOB_AI_MOVEMENT_FAILED`), which fails the `move_to_target` behavior.
// shortcut: A* over free tiles only (closed doors and dense mobs count as walls) and at most
// `MAX_NODES` expansions; tg JPS also plans through doors it has access to. Upgrade with doors.

use std::collections::{BinaryHeap, HashMap};

use bevy::prelude::*;
use ss13_shared::{Dir, TilePos};
use ss13_world::{MoveRequest, Mover, SpatialIndex};

use super::{AiController, AiStatus, STOP_MOVING_WHEN_PULLED};
use crate::health::Stat;

/// Expansions of one A* search.
const MAX_NODES: usize = 600;
/// tg `AI_MAX_PATH_LENGTH`.
const MAX_PATH: usize = 30;
/// Steps without progress before the movement fails (tg `max_pathing_attempts`).
const MAX_FAILS: u32 = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Entity(Entity),
    Tile(TilePos),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Order {
    pub target: Target,
    pub min_dist: u32,
    path: Vec<TilePos>,
    fails: u32,
}

/// A controller's `ai_movement` state: where it is moving to, and whether it gave up.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AiMovement {
    pub order: Option<Order>,
    /// Set when the movement gave up; `move_to_target` reads and clears it.
    pub failed: bool,
}

impl AiMovement {
    /// tg `start_moving_towards`.
    pub fn start(&mut self, target: Target, min_dist: u32) {
        if self
            .order
            .as_ref()
            .is_some_and(|o| o.target == target && o.min_dist == min_dist)
        {
            return;
        }
        self.order = Some(Order {
            target,
            min_dist,
            path: Vec::new(),
            fails: 0,
        });
        self.failed = false;
    }

    /// tg `stop_moving_towards`.
    pub fn stop(&mut self) {
        self.order = None;
    }

    pub fn moving(&self) -> bool {
        self.order.is_some()
    }
}

/// Whether a mob at `from` may step in `dir` (tg `Move` blockers: border edges, dense things).
pub fn can_step(index: &SpatialIndex, mob: Entity, from: TilePos, dir: Dir) -> bool {
    let to = from.step(dir);
    index.in_bounds(to)
        && index.exit_blockers(from, dir, mob).next().is_none()
        && index.enter_blockers(to, dir, mob).next().is_none()
}

/// A diagonal step needs one of its two cardinal halves.
fn can_move(index: &SpatialIndex, mob: Entity, from: TilePos, dir: Dir) -> bool {
    let (dx, dy) = dir.offset();
    if dx == 0 || dy == 0 {
        return can_step(index, mob, from, dir);
    }
    let v = if dy > 0 { Dir::North } else { Dir::South };
    let h = if dx > 0 { Dir::East } else { Dir::West };
    (can_step(index, mob, from, v) && can_step(index, mob, from.step(v), h))
        || (can_step(index, mob, from, h) && can_step(index, mob, from.step(h), v))
}

/// The direction from `from` towards `to` (BYOND `get_dir`).
pub fn dir_towards(from: TilePos, to: TilePos) -> Option<Dir> {
    let (dx, dy) = ((to.x - from.x).signum(), (to.y - from.y).signum());
    Dir::DMI_ORDER.into_iter().find(|d| d.offset() == (dx, dy))
}

/// BYOND `turn(dir, angle)` by 45 degree steps (positive counterclockwise).
fn turn(dir: Dir, steps: i32) -> Dir {
    const RING: [Dir; 8] = [
        Dir::North,
        Dir::NorthWest,
        Dir::West,
        Dir::SouthWest,
        Dir::South,
        Dir::SouthEast,
        Dir::East,
        Dir::NorthEast,
    ];
    let i = RING.iter().position(|&d| d == dir).unwrap_or(0) as i32;
    RING[(i + steps).rem_euclid(8) as usize]
}

/// tg `step_to`: towards the target, else 45 degrees either side.
pub fn greedy_step(index: &SpatialIndex, mob: Entity, from: TilePos, to: TilePos) -> Option<Dir> {
    let dir = dir_towards(from, to)?;
    [dir, turn(dir, 1), turn(dir, -1)]
        .into_iter()
        .find(|&d| can_move(index, mob, from, d))
}

/// Bounded A* over cardinal steps from `from` to a tile within `min_dist` of `to`; the tiles
/// after `from`, in order.
pub fn find_path(
    index: &SpatialIndex,
    mob: Entity,
    from: TilePos,
    to: TilePos,
    min_dist: u32,
) -> Option<Vec<TilePos>> {
    #[derive(PartialEq, Eq)]
    struct Open(u32, u32, TilePos);
    impl Ord for Open {
        fn cmp(&self, o: &Self) -> std::cmp::Ordering {
            // Min-heap on f, then on h.
            (o.0, o.1).cmp(&(self.0, self.1))
        }
    }
    impl PartialOrd for Open {
        fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(o))
        }
    }
    let h = |p: TilePos| (p.x - to.x).unsigned_abs() + (p.y - to.y).unsigned_abs();
    let mut open = BinaryHeap::from([Open(h(from), h(from), from)]);
    let mut came: HashMap<TilePos, (TilePos, u32)> = HashMap::from([(from, (from, 0))]);
    let mut expanded = 0;
    while let Some(Open(_, _, pos)) = open.pop() {
        if pos.chebyshev(to).is_some_and(|d| d <= min_dist) {
            let mut path = vec![pos];
            let mut cur = pos;
            while let Some(&(prev, _)) = came.get(&cur) {
                if prev == cur || prev == from {
                    break;
                }
                path.push(prev);
                cur = prev;
            }
            path.reverse();
            return (path.len() <= MAX_PATH).then_some(path);
        }
        expanded += 1;
        if expanded > MAX_NODES {
            return None;
        }
        let g = came[&pos].1;
        for dir in Dir::CARDINALS {
            let next = pos.step(dir);
            // The goal tile itself may hold the (dense) target.
            let passable = can_step(index, mob, pos, dir)
                || (next == to
                    && index.in_bounds(next)
                    && index.exit_blockers(pos, dir, mob).next().is_none());
            if !passable || came.get(&next).is_some_and(|&(_, ng)| ng <= g + 1) {
                continue;
            }
            came.insert(next, (pos, g + 1));
            open.push(Open(g + 1 + h(next), h(next), next));
        }
    }
    None
}

/// The next step of a movement order, or why there is none.
enum Next {
    Arrived,
    Step(Dir),
    Blocked,
}

fn next_step(
    index: &SpatialIndex,
    mob: Entity,
    from: TilePos,
    to: TilePos,
    order: &mut Order,
) -> Next {
    if from.z != to.z {
        return Next::Blocked;
    }
    if from.chebyshev(to).is_some_and(|d| d <= order.min_dist) {
        order.path.clear();
        return Next::Arrived;
    }
    // Follow a planned detour while it stays free.
    while order.path.first() == Some(&from) {
        order.path.remove(0);
    }
    if let Some(&next) = order.path.first()
        && let Some(dir) = dir_towards(from, next)
        && from.chebyshev(next) == Some(1)
        && can_move(index, mob, from, dir)
    {
        return Next::Step(dir);
    }
    order.path.clear();
    if let Some(dir) = greedy_step(index, mob, from, to) {
        return Next::Step(dir);
    }
    match find_path(index, mob, from, to, order.min_dist) {
        Some(path) if !path.is_empty() => {
            let dir = dir_towards(from, path[0]);
            order.path = path;
            dir.map_or(Next::Blocked, Next::Step)
        }
        _ => Next::Blocked,
    }
}

/// tg `SSmovement` for AI move loops: one step per mob whose `Mover` is ready.
// Cost: per tick, per moving controller a few index lookups; an A* (<= 600 expansions) only
// when no step towards the target is free.
pub(super) fn step_movers(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut moving: Vec<(Entity, TilePos, Order, bool)> = Vec::new();
    let mut q = world.query::<(
        Entity,
        &AiController,
        &TilePos,
        &Mover,
        Option<&Stat>,
        Has<crate::pull::PulledBy>,
    )>();
    for (e, c, &pos, mover, stat, pulled) in q.iter(world) {
        let Some(order) = &c.movement.order else {
            continue;
        };
        if c.status == AiStatus::Off
            || now < mover.ready_at()
            || stat.is_some_and(|s| *s != Stat::Conscious)
            || (pulled && c.traits & STOP_MOVING_WHEN_PULLED != 0)
        {
            continue;
        }
        moving.push((e, pos, order.clone(), pulled));
    }
    for (mob, from, mut order, _) in moving {
        let to = match order.target {
            Target::Entity(e) => world.get::<TilePos>(e).copied(),
            Target::Tile(t) => Some(t),
        };
        let result = match to {
            Some(to) => next_step(world.resource::<SpatialIndex>(), mob, from, to, &mut order),
            None => Next::Blocked,
        };
        let failed = match result {
            Next::Arrived => {
                order.fails = 0;
                false
            }
            Next::Step(dir) => {
                order.fails = 0;
                world.write_message(MoveRequest { entity: mob, dir });
                false
            }
            Next::Blocked => {
                order.fails += 1;
                order.fails >= MAX_FAILS
            }
        };
        if let Some(mut c) = world.get_mut::<AiController>(mob) {
            if failed {
                c.movement.order = None;
                c.movement.failed = true;
            } else if c
                .movement
                .order
                .as_ref()
                .is_some_and(|o| o.target == order.target)
            {
                c.movement.order = Some(order);
            }
        }
    }
}
