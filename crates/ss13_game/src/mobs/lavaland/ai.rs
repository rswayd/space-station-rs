//! The lavaland mobs' AI leaves and decorators (tg `targeted_mob_ability` and its goliath,
//! brimbeam and bileworm subtypes, `move_to_cardinal`, `maintain_distance` with the
//! controllers' skirmish distances, the brood skull's `melee_attack`, `ai_target_timer`,
//! `mob_stat_at_least`, `is_at_distance` with a minimum). They run through
//! `crate::mobs::ai::fauna` (`FaunaLeaf::Lavaland`, `FaunaCond::Lavaland`).

use bevy::prelude::*;
use ss13_shared::TilePos;

use super::{abilities, icemoon, mook};
use crate::health::Stat;
use crate::mobs::ai::behaviors::{
    DELAY_FAIL, DELAY_SUCCEED, FAIL, Perform, RUNNING, SUCCEED, Strategy,
};
use crate::mobs::ai::movement::Target;
use crate::mobs::ai::{AiCtx, Key, Value};

/// `goliath_tentacles` `MIN_TIME_TO_TENTACLE`.
const MIN_TIME_TO_TENTACLE: f64 = 3.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LavaCond {
    /// `mob_stat_at_least(SOFT_CRIT)` on the key's mob, or it lies helpless
    /// (`BB_LOBSTROSITY_EXPLOIT_TRAITS`: floored, immobilized, incapacitated, knocked out).
    TargetDown(Key),
    /// `is_at_distance` with `min_distance`: the key's target is at least this far.
    AtLeast(Key, u32),
}

impl LavaCond {
    pub fn check(self, ctx: &mut AiCtx) -> bool {
        match self {
            Self::TargetDown(key) => ctx.bb.entity(key).is_some_and(|t| {
                let world = &*ctx.world;
                let incap = crate::status_effects::incapacitation(world, t);
                world.get::<Stat>(t).is_some_and(|s| *s >= Stat::SoftCrit)
                    || incap.floored
                    || incap.immobilized
                    || incap.incapacitated
                    || incap.knocked_out
            }),
            Self::AtLeast(key, min) => ctx
                .pawn_pos()
                .zip(ctx.target_pos(key))
                .and_then(|(a, b)| a.chebyshev(b))
                .is_some_and(|d| d >= min),
        }
    }
}

/// `ai_target_timer`: seconds the current target has been held (restarted when it changes).
fn held_for(ctx: &mut AiCtx) -> f64 {
    let current = ctx.bb.entity(Key::CurrentTarget);
    if current != ctx.bb.entity(Key::TargetTimerOf) {
        match current {
            Some(t) => {
                ctx.bb.set(Key::TargetTimerOf, Value::Entity(t));
                ctx.bb.set(Key::TargetTimer, Value::Time(ctx.now));
            }
            None => {
                ctx.bb.clear(Key::TargetTimerOf);
                ctx.bb.clear(Key::TargetTimer);
            }
        }
    }
    match (current, ctx.bb.time(Key::TargetTimer)) {
        (Some(_), Some(since)) => ctx.now - since,
        _ => 0.0,
    }
}

#[derive(Debug)]
pub enum LavaLeaf {
    /// `targeted_mob_ability/goliath_tentacles`: a living target, not held by a tentacle,
    /// tracked for 3 s.
    GoliathTentacles { key: Key },
    /// `targeted_mob_ability` with the legion's `skull_launcher` or the hivelord's
    /// `hivelord_spawn`.
    LaunchBrood { key: Key },
    /// `targeted_mob_ability/brimbeam` (`maximum_distance`): only on a cardinal line.
    Brimbeam { key: Key, max_range: u32 },
    /// `move_to_cardinal`: to a tile in line with the target, `min`-`max` tiles from it.
    MoveToCardinal { key: Key, min: u32, max: u32 },
    /// `targeted_mob_ability` with the lobstrosity's rush.
    Rush { key: Key },
    /// `targeted_mob_ability/and_plan_execute` with `bileworm_spew`.
    SpewBile { key: Key },
    /// `basic_melee_attack` of a brood skull: a human out cold is infested.
    BroodBite { key: Key },
    /// `maintain_distance` with `BB_RANGED_SKIRMISH_MIN_DISTANCE`/`MAX_DISTANCE`.
    MaintainDistance { key: Key, min: u32, max: u32 },
    /// `acquire_target/update_combat_targets` with `BB_AGGRO_RANGE`.
    AcquireWithin { key: Key, range: u32 },
    /// The polar bear's `enrage`.
    Enrage,
    /// The wolf's `call_reinforcements` behind its 30 s `cooldown` decorator.
    CallReinforcements { key: Key },
    /// `targeted_mob_ability` with the whelp's eruption (`wide`) or line breath, within `max`.
    WhelpBreath { key: Key, wide: bool, max: u32 },
    /// `ability_available(BB_DEMON_TELEPORT_ABILITY)` + `find_furthest_turf_from_target`
    /// (`escape`, only next to the target) or `find_valid_teleport_location`, then
    /// `targeted_mob_ability/and_clear_target`.
    DemonTeleport { key: Key, escape: bool },
    /// `pawn_health_below(75)` + `ability_available` + `targeted_mob_ability` with
    /// `create_afterimages`.
    DemonClone,
    /// `targeted_mob_ability` with the worker's `mook_leap`.
    MookLeap { key: Key },
    /// `heal_injured`: the nearest hurt mook it sees within `range` (`injured_mob`), walked to
    /// and healed (`interact_with_target` -> `healing_touch`).
    MookHeal { range: u32 },
}

impl LavaLeaf {
    pub fn cooldown(&self) -> f64 {
        0.0
    }

    pub fn setup(&mut self, ctx: &mut AiCtx) -> bool {
        match *self {
            Self::MaintainDistance { key, .. } => {
                let (Some(from), Some(to)) = (ctx.pawn_pos(), ctx.target_pos(key)) else {
                    return false;
                };
                ctx.movement.failed = false;
                crate::mobs::ai::behaviors::can_see(ctx.world, from, to, 10)
            }
            Self::MoveToCardinal { key, .. } => {
                ctx.movement.failed = false;
                ctx.truthy(key)
            }
            _ => true,
        }
    }

    pub fn perform(&mut self, ctx: &mut AiCtx) -> Perform {
        match *self {
            Self::GoliathTentacles { key } => {
                let Some(target) = living_target(ctx, key) else {
                    return FAIL;
                };
                if ctx.world.get::<abilities::Grabbed>(target).is_some()
                    || held_for(ctx) < MIN_TIME_TO_TENTACLE
                {
                    return FAIL;
                }
                if abilities::unleash_tentacles(ctx.world, ctx.pawn, target) {
                    DELAY_SUCCEED
                } else {
                    FAIL
                }
            }
            Self::LaunchBrood { key } => {
                let Some(target) = living_target(ctx, key) else {
                    return FAIL;
                };
                if !abilities::launcher_ready(ctx.world, ctx.pawn, ctx.now) {
                    return FAIL;
                }
                if abilities::launch_brood(ctx.world, ctx.pawn, target) {
                    DELAY_SUCCEED
                } else {
                    DELAY_FAIL
                }
            }
            Self::Brimbeam { key, max_range } => {
                let Some(target) = living_target(ctx, key) else {
                    return FAIL;
                };
                let (Some(from), Some(to)) = (ctx.pawn_pos(), ctx.target_pos(key)) else {
                    return FAIL;
                };
                let Some(dir) = abilities::cardinal_towards(from, to) else {
                    return FAIL;
                };
                let near = from.chebyshev(to).is_some_and(|d| d <= max_range);
                if !near || !Strategy::Basic.valid(ctx, target, 0) {
                    return FAIL;
                }
                if abilities::start_beam(ctx.world, ctx.pawn, dir) {
                    ctx.movement.stop();
                    DELAY_SUCCEED
                } else {
                    FAIL
                }
            }
            Self::MoveToCardinal { key, min, max } => move_to_cardinal(ctx, key, min, max),
            Self::Rush { key } => {
                let Some(target) = living_target(ctx, key) else {
                    return FAIL;
                };
                if !abilities::rush_ready(ctx.world, ctx.pawn, ctx.now) {
                    return FAIL;
                }
                // `basic_charge` is a `click_to_activate` ability: in sight, in its range.
                let (Some(from), Some(to)) = (ctx.pawn_pos(), ctx.target_pos(key)) else {
                    return FAIL;
                };
                if !crate::mobs::ai::behaviors::can_see(ctx.world, from, to, 9) {
                    return FAIL;
                }
                if abilities::start_rush(ctx.world, ctx.pawn, target) {
                    ctx.movement.stop();
                    DELAY_SUCCEED
                } else {
                    FAIL
                }
            }
            Self::SpewBile { key } => {
                let Some(target) = living_target(ctx, key) else {
                    return FAIL;
                };
                let (Some(from), Some(to)) = (ctx.pawn_pos(), ctx.target_pos(key)) else {
                    return FAIL;
                };
                if !crate::mobs::ai::behaviors::can_see(ctx.world, from, to, 9) {
                    return FAIL;
                }
                if abilities::start_spew(ctx.world, ctx.pawn, target) {
                    DELAY_SUCCEED
                } else {
                    FAIL
                }
            }
            Self::BroodBite { key } => brood_bite(ctx, key),
            Self::MaintainDistance { key, min, max } => {
                crate::mobs::ai::fauna::maintain_distance(ctx, key, min, max)
            }
            Self::AcquireWithin { key, range } => acquire_within(ctx, key, range),
            Self::Enrage => {
                let target = ctx.bb.entity(Key::CurrentTarget);
                if icemoon::enrage(ctx.world, ctx.pawn, target) {
                    SUCCEED
                } else {
                    FAIL
                }
            }
            Self::CallReinforcements { key } => {
                // tg's `cooldown` decorator fails the branch while it cools down, which fails
                // the wolf's `BT_PARALLEL_FAILURE_ANY` fight; a howl on cooldown is skipped
                // instead.
                if let Some(target) = living_target(ctx, key) {
                    icemoon::call_reinforcements(ctx.world, ctx.pawn, target, ctx.now);
                }
                DELAY_SUCCEED
            }
            Self::WhelpBreath { key, wide, max } => {
                let Some(target) = living_target(ctx, key) else {
                    return FAIL;
                };
                let near = ctx
                    .pawn_pos()
                    .zip(ctx.target_pos(key))
                    .and_then(|(a, b)| a.chebyshev(b))
                    .is_some_and(|d| d <= max);
                if !near || !icemoon::breath_ready(ctx.world, ctx.pawn, wide, ctx.now) {
                    return FAIL;
                }
                if icemoon::start_breath(ctx.world, ctx.pawn, target, wide) {
                    ctx.movement.stop();
                    DELAY_SUCCEED
                } else {
                    FAIL
                }
            }
            Self::DemonTeleport { key, escape } => {
                if !icemoon::teleport_ready(ctx.world, ctx.pawn, ctx.now) {
                    return FAIL;
                }
                let (Some(from), Some(at)) = (ctx.pawn_pos(), ctx.target_pos(key)) else {
                    return FAIL;
                };
                if escape && from.chebyshev(at).is_none_or(|d| d > 1) {
                    return FAIL;
                }
                let Some(to) = icemoon::teleport_spot(ctx.world, at, escape, &mut ctx.rng) else {
                    return DELAY_FAIL;
                };
                if icemoon::start_teleport(ctx.world, ctx.pawn, to) {
                    DELAY_SUCCEED
                } else {
                    FAIL
                }
            }
            Self::MookLeap { key } => {
                let Some(target) = living_target(ctx, key) else {
                    return FAIL;
                };
                let (Some(from), Some(to)) = (ctx.pawn_pos(), ctx.target_pos(key)) else {
                    return FAIL;
                };
                // A `click_to_activate` ability: on a target it sees.
                if !mook::leap_ready(ctx.world, ctx.pawn, ctx.now)
                    || !crate::mobs::ai::behaviors::can_see(ctx.world, from, to, 9)
                {
                    return FAIL;
                }
                if mook::start_leap(ctx.world, ctx.pawn, target) {
                    ctx.movement.stop();
                    DELAY_SUCCEED
                } else {
                    FAIL
                }
            }
            Self::MookHeal { range } => {
                let Some(patient) = mook::injured_mook_near(ctx.world, ctx.pawn, range) else {
                    return FAIL;
                };
                // `interact_with_target` is a melee click: the attack cooldown paces it.
                let ready = ctx
                    .world
                    .get::<crate::combat::NextMove>(ctx.pawn)
                    .is_none_or(|n| ctx.now >= n.0);
                if !ready {
                    return RUNNING;
                }
                if mook::heal(ctx.world, ctx.pawn, patient) {
                    let next = ctx.now + mook::HEAL_PACE;
                    ctx.world
                        .entity_mut(ctx.pawn)
                        .insert(crate::combat::NextMove(next));
                    ctx.movement.stop();
                    return DELAY_SUCCEED;
                }
                ctx.movement.start(Target::Entity(patient), 1);
                RUNNING
            }
            Self::DemonClone => {
                if icemoon::conjure_afterimages(ctx.world, ctx.pawn) {
                    DELAY_SUCCEED
                } else {
                    FAIL
                }
            }
        }
    }

    pub fn finish(&mut self, ctx: &mut AiCtx, _succeeded: bool) {
        if let Self::MaintainDistance { .. } | Self::MoveToCardinal { .. } | Self::MookHeal { .. } =
            self
        {
            ctx.movement.failed = false;
            ctx.movement.stop();
        }
    }
}

/// The key's mob, when it still exists and lives.
fn living_target(ctx: &AiCtx, key: Key) -> Option<Entity> {
    ctx.bb.entity(key).filter(|&t| {
        ctx.world.get_entity(t).is_ok()
            && ctx.world.get::<Stat>(t).is_some_and(|s| *s != Stat::Dead)
    })
}

/// `move_to_cardinal` `perform`: in line and in range it is done; otherwise it walks to the
/// closest tile in line with the target within `min`-`max` (the nearer axis first).
fn move_to_cardinal(ctx: &mut AiCtx, key: Key, min: u32, max: u32) -> Perform {
    if ctx.movement.failed {
        return FAIL;
    }
    let (Some(from), Some(to)) = (ctx.pawn_pos(), ctx.target_pos(key)) else {
        return FAIL;
    };
    let dist = from.chebyshev(to).unwrap_or(u32::MAX);
    if abilities::cardinal_towards(from, to).is_some() && (min..=max).contains(&dist) {
        ctx.movement.stop();
        return RUNNING;
    }
    let goal = cardinal_spot(from, to, min, max);
    ctx.movement.start(Target::Tile(goal), 0);
    RUNNING
}

/// The tile in line with `to` (same row or column) `min`-`max` away that is closest to `from`.
pub fn cardinal_spot(from: TilePos, to: TilePos, min: u32, max: u32) -> TilePos {
    let clamp = |d: i32| {
        let sign = if d < 0 { -1 } else { 1 };
        sign * d.abs().clamp(min as i32, max as i32)
    };
    let (dx, dy) = (from.x - to.x, from.y - to.y);
    // Line up on the axis that needs the shorter walk.
    if dx.abs() <= dy.abs() {
        TilePos::new(to.x, to.y + clamp(dy), to.z)
    } else {
        TilePos::new(to.x + clamp(dx), to.y, to.z)
    }
}

/// `legion_brood/melee_attack`: a human out cold next to it is infested; anyone else is
/// bitten (`basic_melee_attack`).
fn brood_bite(ctx: &mut AiCtx, key: Key) -> Perform {
    let Some(target) = ctx
        .bb
        .entity(key)
        .filter(|&t| ctx.world.get_entity(t).is_ok())
    else {
        return FAIL;
    };
    let human = ctx
        .world
        .get::<crate::medical::wounds::Limbs>(target)
        .is_some();
    let reach = ctx
        .pawn_pos()
        .zip(ctx.world.get::<TilePos>(target).copied())
        .is_some_and(|(a, b)| {
            ctx.world
                .resource::<ss13_world::SpatialIndex>()
                .adjacent(a, b, |e| e == target)
        });
    if human && reach && abilities::out_cold(ctx.world, target) {
        let ready = ctx
            .world
            .get::<crate::combat::NextMove>(ctx.pawn)
            .is_none_or(|n| ctx.now >= n.0);
        if !ready {
            return FAIL;
        }
        abilities::infest(ctx.world, ctx.pawn, target);
        return DELAY_SUCCEED;
    }
    crate::mobs::ai::behaviors::melee(ctx, key, Strategy::Basic)
}

/// `update_combat_targets` with an aggro range: the nearest valid mob within `range`, kept up
/// to 16 tiles while no other is found.
fn acquire_within(ctx: &mut AiCtx, key: Key, range: u32) -> Perform {
    const LOSS: u32 = 16;
    let current = ctx.bb.entity(key);
    if current.is_some_and(|t| Strategy::Basic.valid(ctx, t, range)) {
        return DELAY_SUCCEED;
    }
    let found = crate::mobs::ai::behaviors::candidates(ctx, range)
        .into_iter()
        .find(|&t| Strategy::Basic.valid(ctx, t, range));
    match found {
        Some(t) => {
            ctx.bb.set(key, Value::Entity(t));
            DELAY_SUCCEED
        }
        None if current.is_some_and(|t| Strategy::Basic.valid(ctx, t, LOSS)) => DELAY_SUCCEED,
        None => {
            ctx.bb.clear(key);
            DELAY_FAIL
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cardinal_spot_lines_up_within_range() {
        let target = TilePos::new(10, 10, 1);
        // Two tiles east, five north: lines up on the column, five tiles north.
        assert_eq!(
            cardinal_spot(TilePos::new(12, 15, 1), target, 2, 9),
            TilePos::new(10, 15, 1)
        );
        // Too close on the row: backs off to the minimum.
        assert_eq!(
            cardinal_spot(TilePos::new(11, 10, 1), target, 2, 9),
            TilePos::new(12, 10, 1)
        );
        // Far on the row: no farther than the maximum.
        assert_eq!(
            cardinal_spot(TilePos::new(30, 11, 1), target, 2, 9),
            TilePos::new(19, 10, 1)
        );
    }
}
