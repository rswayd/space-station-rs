//! The gorilla (`farm_animals/gorilla/gorilla.bt.json`), statue and mannequin
//! (`space_fauna/statue/*.bt.json`), regal rat (`regal_rat/regal_rat.bt.json`), headslug and bee
//! (`farm_animals/bee/*.bt.json`) controllers, with the leaves of `crate::mobs::space_fauna`,
//! `crate::mobs::gorilla` and `crate::mobs::bees`. Left out everywhere: escape captivity,
//! attacking obstructions; the bees' hive branches (`find_hive`, `transition_hive_status`,
//! `enter_exit_hive`: the apiary belongs to botany and is not ported).

use super::super::behaviors::{Behavior, FIND_TARGET_RATE, Strategy};
use super::super::bt::{Abort, Cond, Node, decorator, leaf, selector, sequence, subplan};
use super::super::fauna::FaunaLeaf;
use super::super::{Key, STOP_MOVING_WHEN_PULLED};
use super::{
    BASIC, Built, find_food, melee_combat, move_to_and_eat, par, random_walk, run_away, under,
};

fn fauna(leaf_: FaunaLeaf) -> Node {
    leaf(Behavior::Fauna(leaf_))
}

fn acquire(strategy: Strategy) -> Node {
    leaf(Behavior::AcquireCombatTarget {
        key: Key::CurrentTarget,
        strategy,
    })
}

/// The controllers of this file by tg path.
pub(super) fn build(path: &str) -> Option<Built> {
    let is = |sub: &str| under(path, &format!("{BASIC}/{sub}"));
    let mut traits = 0;
    let mut retaliates = false;
    let root = if is("gorilla") {
        // `DEFAULT_AI_FLAGS | STOP_MOVING_WHEN_PULLED`; the lesser one ooga-s more.
        traits = STOP_MOVING_WHEN_PULLED;
        gorilla(if is("gorilla/lesser") { 0.6 } else { 0.4 })
    } else if is("statue") {
        statue()
    } else if is("stares_at_people") {
        stares_at_people()
    } else if is("suspicious_mannequin") {
        suspicious_mannequin()
    } else if is("regal_rat") {
        retaliates = true;
        regal_rat()
    } else if is("headslug") {
        random_walk(25.0)
    } else if is("queen_bee") {
        bee_combat()
    } else if is("bee") {
        bee()
    } else {
        return None;
    };
    Some(Built {
        roots: vec![root],
        blackboard: Vec::new(),
        traits,
        retaliates,
    })
}

/// `gorilla.bt.json`: fights what it sees, eats bananas, ooga-s (`BB_EMOTE_CHANCE` percent a
/// second).
fn gorilla(emote_chance: f32) -> Node {
    par(
        vec![
            subplan(
                selector(vec![
                    decorator(
                        Cond::KeySet(Key::CurrentTarget),
                        false,
                        Abort::Both,
                        melee_combat(Strategy::Basic),
                    ),
                    move_to_and_eat(),
                    random_walk(25.0),
                ]),
                true,
                true,
                0.0,
            ),
            acquire(Strategy::Basic),
            find_food(),
            subplan(
                decorator(
                    Cond::RandomChance(emote_chance),
                    false,
                    Abort::None,
                    fauna(FaunaLeaf::Emote("ooga")),
                ),
                true,
                true,
                1.0,
            ),
        ],
        FIND_TARGET_RATE,
    )
}

/// `statue.bt.json`: smashes lit lights in view, else fights.
fn statue() -> Node {
    par(
        vec![
            selector(vec![
                decorator(
                    Cond::KeySet(Key::LowPriorityHunting),
                    false,
                    Abort::Both,
                    sequence(vec![
                        leaf(Behavior::MoveToTarget {
                            key: Key::LowPriorityHunting,
                            dist: 1,
                            finish_on_arrival: true,
                        }),
                        fauna(FaunaLeaf::SmashLight {
                            key: Key::LowPriorityHunting,
                        }),
                    ]),
                ),
                decorator(
                    Cond::KeySet(Key::CurrentTarget),
                    false,
                    Abort::Both,
                    melee_combat(Strategy::Basic),
                ),
            ]),
            acquire(Strategy::Basic),
            fauna(FaunaLeaf::AcquireLight {
                key: Key::LowPriorityHunting,
            }),
        ],
        FIND_TARGET_RATE,
    )
}

/// `stares_at_people.bt.json`: the mannequin only turns to whoever it sees.
fn stares_at_people() -> Node {
    par(
        vec![
            selector(vec![
                decorator(
                    Cond::KeySet(Key::CurrentTarget),
                    false,
                    Abort::Both,
                    fauna(FaunaLeaf::FaceTarget {
                        key: Key::CurrentTarget,
                    }),
                ),
                fauna(FaunaLeaf::Wait),
            ]),
            acquire(Strategy::Basic),
        ],
        FIND_TARGET_RATE,
    )
}

/// `suspicious_mannequin.bt.json`: closes in and claws (while unseen: `crate::mobs::space_fauna`),
/// screams now and then.
// shortcut: targets within 9 tiles (tg looks 14 far).
fn suspicious_mannequin() -> Node {
    par(
        vec![
            selector(vec![
                decorator(
                    Cond::KeySet(Key::CurrentTarget),
                    false,
                    Abort::Both,
                    melee_combat(Strategy::Basic),
                ),
                fauna(FaunaLeaf::Wait),
            ]),
            acquire(Strategy::Basic),
            decorator(
                Cond::KeySet(Key::CurrentTarget),
                false,
                Abort::None,
                decorator(
                    Cond::RandomChance(0.05),
                    false,
                    Abort::None,
                    fauna(FaunaLeaf::Emote("scream")),
                ),
            ),
        ],
        FIND_TARGET_RATE,
    )
}

/// `regal_rat.bt.json`: runs from whoever hurt it, raising its army and biting back on the way;
/// keeps its domain on.
fn regal_rat() -> Node {
    par(
        vec![
            selector(vec![
                decorator(
                    Cond::KeySet(Key::FleeTarget),
                    false,
                    Abort::Both,
                    par(
                        vec![
                            run_away(Key::FleeTarget),
                            selector(vec![
                                fauna(FaunaLeaf::RatRiot),
                                leaf(Behavior::MeleeAttack {
                                    key: Key::FleeTarget,
                                    strategy: Strategy::Basic,
                                }),
                            ]),
                        ],
                        0.0,
                    ),
                ),
                random_walk(25.0),
            ]),
            leaf(Behavior::TargetFromRetaliateList {
                key: Key::FleeTarget,
                strategy: Strategy::Basic,
            }),
            fauna(FaunaLeaf::RatDomain),
        ],
        FIND_TARGET_RATE,
    )
}

/// `basic_mobs/simple_hostile_combat.bt.json` with the bee targeting strategy.
fn bee_combat() -> Node {
    par(
        vec![
            selector(vec![
                decorator(
                    Cond::KeySet(Key::CurrentTarget),
                    false,
                    Abort::Both,
                    melee_combat(Strategy::Bee),
                ),
                random_walk(25.0),
            ]),
            acquire(Strategy::Bee),
        ],
        FIND_TARGET_RATE,
    )
}

/// `bee.bt.json` without the hive: pollinates a tray it found (85% of its 10 s looks), else
/// stings whoever is not bee-friendly.
fn bee() -> Node {
    par(
        vec![
            selector(vec![
                decorator(
                    Cond::KeySet(Key::TargetHydro),
                    false,
                    Abort::Both,
                    sequence(vec![
                        leaf(Behavior::MoveToTarget {
                            key: Key::TargetHydro,
                            dist: 1,
                            finish_on_arrival: true,
                        }),
                        fauna(FaunaLeaf::Pollinate {
                            key: Key::TargetHydro,
                        }),
                    ]),
                ),
                bee_combat(),
            ]),
            decorator(
                Cond::RandomChance(0.85),
                false,
                Abort::None,
                fauna(FaunaLeaf::AcquireTray {
                    key: Key::TargetHydro,
                }),
            ),
        ],
        FIND_TARGET_RATE,
    )
}
