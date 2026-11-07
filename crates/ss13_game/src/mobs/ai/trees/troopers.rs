//! The trooper controllers' trees (`code/modules/mob/living/basic/trooper/*.bt.json`):
//! `trooper.bt.json` (melee), `trooper_ranged.bt.json` with each controller's bindings
//! (`ranged`, `burst`, `shotgunner`) and the retaliating `peaceful`/`peaceful_burst`.
// Left out: escape captivity, attacking obstructions, `call_reinforcements` and
// `move_to_reinforce` (only `calls_reinforcements` and the peaceful troopers call; the branch
// falls through as tg's does without friends in view).

use super::super::Key;
use super::super::behaviors::{Behavior, FIND_TARGET_RATE, Strategy};
use super::super::bt::{Abort, Cond, Node, decorator, leaf, selector};
use super::super::fauna::FaunaLeaf;
use super::{BASIC, Built, par, random_walk, retaliate_target, under, while_closing_in};
use crate::mobs::lavaland::ai::LavaLeaf;

fn acquire() -> Node {
    leaf(Behavior::AcquireCombatTarget {
        key: Key::CurrentTarget,
        strategy: Strategy::Basic,
    })
}

fn move_to_target() -> Node {
    leaf(Behavior::MoveToTarget {
        key: Key::CurrentTarget,
        dist: 1,
        finish_on_arrival: false,
    })
}

/// `basic_ranged_attack` with the subtree's bindings.
fn shoot(time_between_perform: f64, max_range: u32, avoid_friendly_fire: bool) -> Node {
    leaf(Behavior::Fauna(FaunaLeaf::RangedAttack {
        key: Key::CurrentTarget,
        cooldown: time_between_perform,
        max_range,
        avoid_friendly_fire,
    }))
}

/// The common shape: fight the target (`attack` while `mover` runs), else wander; `targeting`
/// in parallel every `BASIC_MOB_FIND_TARGET_RATE`.
fn combat(attack: Node, mover: Node, targeting: Vec<Node>) -> Node {
    let mut children = vec![selector(vec![
        decorator(
            Cond::KeySet(Key::CurrentTarget),
            false,
            Abort::Both,
            while_closing_in(attack, mover),
        ),
        random_walk(25.0),
    ])];
    children.extend(targeting);
    par(children, FIND_TARGET_RATE)
}

/// `trooper_ranged.bt.json`: shoots keeping `BB_RANGED_SKIRMISH_*_DISTANCE`.
fn ranged(time: f64, max_range: u32, avoid: bool, (min, max): (u32, u32)) -> Node {
    combat(
        // The `succeed` after the shot only ends the selector; the subplan loops either way.
        shoot(time, max_range, avoid),
        leaf(Behavior::Fauna(FaunaLeaf::Lavaland(
            LavaLeaf::MaintainDistance {
                key: Key::CurrentTarget,
                min,
                max,
            },
        ))),
        vec![acquire()],
    )
}

/// The trooper controllers; `None` for anything else.
pub(super) fn build(path: &str) -> Option<Built> {
    let is = |sub: &str| under(path, &format!("{BASIC}/{sub}"));
    if !is("trooper") {
        return None;
    }
    let mut retaliates = false;
    let root = if is("trooper/ranged/burst/peaceful_burst") {
        // Closes in to a tile and fires 3 s bursts at whoever hit it (or a fellow trooper).
        retaliates = true;
        combat(
            shoot(3.0, 9, true),
            move_to_target(),
            vec![retaliate_target(Key::CurrentTarget)],
        )
    } else if is("trooper/ranged/burst") {
        // `burst.bt.json`: 3 s, default `max_range` 3; skirmish 2-3.
        ranged(3.0, 3, true, (2, 3))
    } else if is("trooper/ranged/shotgunner") {
        ranged(3.0, 3, true, (2, 3))
    } else if is("trooper/ranged") {
        // `ranged.bt.json`: 1 s, 5 tiles; skirmish 3-4.
        ranged(1.0, 5, true, (3, 4))
    } else if is("trooper/peaceful") {
        retaliates = true;
        combat(
            leaf(Behavior::MeleeAttack {
                key: Key::CurrentTarget,
                strategy: Strategy::Basic,
            }),
            move_to_target(),
            vec![retaliate_target(Key::CurrentTarget)],
        )
    } else {
        // `trooper.bt.json` (and `calls_reinforcements`, `viscerator`, which inherit it).
        combat(
            leaf(Behavior::MeleeAttack {
                key: Key::CurrentTarget,
                strategy: Strategy::Basic,
            }),
            move_to_target(),
            vec![acquire()],
        )
    };
    Some(Built {
        roots: vec![root],
        blackboard: Vec::new(),
        traits: 0,
        retaliates,
    })
}
