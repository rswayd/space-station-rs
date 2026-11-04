//! The ported controllers' trees, node for node after their tg `*.bt.json` (the file named on
//! each builder). Branches whose leaves need systems ss13-rs lacks are left out and named
//! (escape captivity, pet commands, breeding, cheese, instruments, carp rifts and magic, cat
//! homes and food); a selector then falls through to the next branch as tg's does when those
//! fail.

use ss13_mapload::mobs::MobDef;

use super::behaviors::{
    Behavior, CAT_SPEECH, COCKROACH_SPEECH, DOG_SPEECH, FIND_TARGET_RATE, FOX_SPEECH,
    IDLE_BEHAVIOR_RATE, MOUSE_SPEECH, Speech, Strategy,
};
use super::bt::{
    Abort, Cond, Node, ParallelOpts, decorator, leaf, parallel, selector, sequence, subplan,
    time_limit,
};
use super::{Key, STOP_MOVING_WHEN_PULLED, Value};

pub const CARP: &str = "/datum/ai_controller/basic_controller/carp";
const DOG: &str = "/datum/ai_controller/basic_controller/dog";
const CORGI: &str = "/datum/ai_controller/basic_controller/dog/corgi";
const CAT: &str = "/datum/ai_controller/basic_controller/cat";
const MOUSE: &str = "/datum/ai_controller/basic_controller/mouse";
const MOUSE_RAT: &str = "/datum/ai_controller/basic_controller/mouse/rat";
const COCKROACH: &str = "/datum/ai_controller/basic_controller/cockroach";
const FOX: &str = "/datum/ai_controller/basic_controller/fox";

pub struct Built {
    pub roots: Vec<Node>,
    pub blackboard: Vec<(Key, Value)>,
    pub traits: u32,
    /// tg `ai_retaliate` on the mob.
    pub retaliates: bool,
}

fn under(path: &str, root: &str) -> bool {
    ss13_mapload::fidelity::is_under(path, root)
}

/// The tree of tg controller `path`; `None` without a controller.
pub fn build(path: &str, _def: &MobDef) -> Option<Built> {
    if path.is_empty() {
        return None;
    }
    let mut built = Built {
        roots: Vec::new(),
        blackboard: Vec::new(),
        traits: 0,
        retaliates: false,
    };
    if under(path, "/datum/ai_controller/basic_controller/bot") {
        return Some(crate::bots::ai::tree(path));
    }
    if under(path, CARP) {
        let passive =
            path != CARP && path != format!("{CARP}/mega") && path != format!("{CARP}/ranged");
        built.roots = vec![carp(passive)];
        built.blackboard = vec![(Key::StopFleeing, Value::Bool(true))];
        built.retaliates = true;
        if passive {
            built.traits = STOP_MOVING_WHEN_PULLED;
        }
    } else if under(path, CORGI) {
        built.roots = vec![corgi()];
    } else if under(path, DOG) {
        built.roots = vec![dog()];
    } else if under(path, CAT) {
        built.roots = vec![cat()];
        built.retaliates = true;
    } else if under(path, MOUSE) && !under(path, MOUSE_RAT) {
        built.roots = vec![mouse()];
        built.traits = STOP_MOVING_WHEN_PULLED;
    } else if under(path, COCKROACH) {
        built.roots = vec![cockroach()];
    } else if under(path, FOX) {
        built.roots = vec![fox()];
        built.retaliates = true;
    } else {
        // shortcut: controllers not ported yet only wander (`random_walk`), so placed pets are
        // not statues; port their trees one by one.
        built.roots = vec![random_walk(25.0)];
    }
    Some(built)
}

fn par(children: Vec<Node>, repeat_delay: f64) -> Node {
    parallel(
        children,
        ParallelOpts {
            repeat_secondary: true,
            repeat_secondary_delay: repeat_delay,
            finish_on_primary: true,
            ..ParallelOpts::default()
        },
    )
}

/// `generic_subtrees/random_walk.bt.json`.
pub fn random_walk(chance: f64) -> Node {
    subplan(
        leaf(Behavior::IdleRandomWalk { chance }),
        true,
        false,
        IDLE_BEHAVIOR_RATE,
    )
}

/// `basic_subtrees/random_speech_loop.bt.json`.
fn speech_loop(speech: &'static Speech) -> Node {
    subplan(leaf(Behavior::RandomSpeech(speech)), true, true, 1.0)
}

/// `basic_subtrees/run_away_from_target.bt.json`.
fn run_away(target: Key) -> Node {
    decorator(
        Cond::KeySet(Key::StopFleeing),
        true,
        Abort::Both,
        par(
            vec![
                subplan(
                    leaf(Behavior::FindFleeLocation { key: target }),
                    true,
                    true,
                    0.0,
                ),
                leaf(Behavior::MoveToTarget {
                    key: Key::FleeLocation,
                    dist: 0,
                    finish_on_arrival: true,
                }),
            ],
            0.0,
        ),
    )
}

/// Melee while closing in (the combat parallels of `carp_combat.bt.json`, `cat.bt.json`,
/// `fox.bt.json`).
fn melee_combat(strategy: Strategy) -> Node {
    parallel(
        vec![
            subplan(
                leaf(Behavior::MeleeAttack {
                    key: Key::CurrentTarget,
                    strategy,
                }),
                true,
                true,
                0.0,
            ),
            leaf(Behavior::MoveToTarget {
                key: Key::CurrentTarget,
                dist: 1,
                finish_on_arrival: false,
            }),
        ],
        ParallelOpts {
            failure_any: true,
            repeat_secondary: true,
            finish_on_primary: true,
            ..ParallelOpts::default()
        },
    )
}

/// `carp/basic_carp_tree.bt.json` with `carp_target_selection` (or, for passive carp,
/// `carp_retaliate_selection`), `carp_flee`, `carp_combat` and `carp_migration`.
// Left out: escape captivity, pet commands, magicarp spells, rifts, attacking obstacles.
fn carp(passive: bool) -> Node {
    let flee = decorator(
        Cond::CarpShouldFlee(Key::CurrentTarget),
        false,
        Abort::Both,
        par(
            vec![
                subplan(
                    leaf(Behavior::FindFleeLocation {
                        key: Key::CurrentTarget,
                    }),
                    true,
                    true,
                    0.0,
                ),
                leaf(Behavior::MoveToTarget {
                    key: Key::FleeLocation,
                    dist: 0,
                    finish_on_arrival: true,
                }),
            ],
            0.0,
        ),
    );
    let migration = selector(vec![
        decorator(
            Cond::KeySet(Key::MigrationTarget),
            false,
            Abort::None,
            sequence(vec![
                leaf(Behavior::MoveToTarget {
                    key: Key::MigrationTarget,
                    dist: 3,
                    finish_on_arrival: true,
                }),
                leaf(Behavior::ClearArrivedMigration),
            ]),
        ),
        leaf(Behavior::NextMigrationStep),
    ]);
    let targeting = if passive {
        leaf(Behavior::TargetFromRetaliateList {
            key: Key::CurrentTarget,
            strategy: Strategy::Basic,
        })
    } else {
        selector(vec![
            decorator(
                Cond::KeySet(Key::StopFleeing),
                true,
                Abort::None,
                leaf(Behavior::AcquireCombatTarget {
                    key: Key::CurrentTarget,
                    strategy: Strategy::Basic,
                }),
            ),
            leaf(Behavior::AcquireCombatTarget {
                key: Key::CurrentTarget,
                strategy: Strategy::Basic,
            }),
        ])
    };
    selector(vec![par(
        vec![
            selector(vec![
                decorator(
                    Cond::KeySet(Key::CurrentTarget),
                    false,
                    Abort::Both,
                    selector(vec![flee, melee_combat(Strategy::Basic)]),
                ),
                selector(vec![migration, random_walk(25.0)]),
            ]),
            targeting,
        ],
        FIND_TARGET_RATE,
    )])
}

/// `dog/dog.bt.json`.
// Left out: pet commands; `dog_harassment` (no `TRAIT_HATED_BY_DOGS` here, so it never fires).
fn dog() -> Node {
    selector(vec![par(
        vec![
            subplan(leaf(Behavior::IdleDog), true, false, 0.0),
            speech_loop(&DOG_SPEECH),
        ],
        1.0,
    )])
}

/// `dog/dog_corgi.bt.json`: runs from anyone holding tongs.
// Left out: pet commands, harassment, breeding.
fn corgi() -> Node {
    selector(vec![par(
        vec![
            selector(vec![
                decorator(
                    Cond::KeySet(Key::CurrentTarget),
                    false,
                    Abort::Both,
                    run_away(Key::CurrentTarget),
                ),
                subplan(leaf(Behavior::IdleDog), true, false, 0.0),
            ]),
            leaf(Behavior::AcquireCombatTarget {
                key: Key::CurrentTarget,
                strategy: Strategy::HoldingTongs,
            }),
            speech_loop(&DOG_SPEECH),
        ],
        FIND_TARGET_RATE,
    )])
}

/// `pets/cat/cat.bt.json` with `cat_hunt_mice`.
// Left out: resisting, pet commands, home, hauling and finding food, trespassers, breeding.
fn cat() -> Node {
    selector(vec![
        decorator(
            Cond::KeySet(Key::FleeTarget),
            false,
            Abort::Lower,
            sequence(vec![
                time_limit(3.0, true, run_away(Key::FleeTarget)),
                leaf(Behavior::ClearKey(Key::FleeTarget)),
            ]),
        ),
        par(
            vec![
                par(
                    vec![
                        selector(vec![
                            decorator(
                                Cond::KeySet(Key::MouseTarget),
                                false,
                                Abort::Both,
                                sequence(vec![
                                    leaf(Behavior::MoveToTarget {
                                        key: Key::MouseTarget,
                                        dist: 1,
                                        finish_on_arrival: true,
                                    }),
                                    leaf(Behavior::PlayWithMouse {
                                        key: Key::MouseTarget,
                                    }),
                                ]),
                            ),
                            decorator(
                                Cond::KeySet(Key::MouseTarget),
                                true,
                                Abort::None,
                                decorator(
                                    Cond::KeySet(Key::CurrentTarget),
                                    false,
                                    Abort::Both,
                                    melee_combat(Strategy::Basic),
                                ),
                            ),
                            random_walk(25.0),
                        ]),
                        speech_loop(&CAT_SPEECH),
                    ],
                    1.0,
                ),
                leaf(Behavior::AcquireCombatTarget {
                    key: Key::CurrentTarget,
                    strategy: Strategy::Basic,
                }),
                leaf(Behavior::AcquireMouse {
                    key: Key::MouseTarget,
                    vision: 9,
                }),
            ],
            FIND_TARGET_RATE,
        ),
    ])
}

/// `vermin/mouse.bt.json` with `eat_cable.bt.json`.
// Left out: pet commands, cheese, instruments.
fn mouse() -> Node {
    let flee = decorator(
        Cond::KeySet(Key::CurrentTarget),
        false,
        Abort::Both,
        par(
            vec![
                run_away(Key::CurrentTarget),
                leaf(Behavior::AcquireCombatTarget {
                    key: Key::CurrentTarget,
                    strategy: Strategy::Basic,
                }),
            ],
            FIND_TARGET_RATE,
        ),
    );
    let eat_cable = decorator(
        Cond::KeySet(Key::LowPriorityHunting),
        false,
        Abort::Both,
        par(
            vec![
                sequence(vec![
                    leaf(Behavior::MoveToTarget {
                        key: Key::LowPriorityHunting,
                        dist: 0,
                        finish_on_arrival: true,
                    }),
                    leaf(Behavior::ClearKey(Key::WantsToEatCable)),
                    leaf(Behavior::BiteCable {
                        key: Key::LowPriorityHunting,
                    }),
                ]),
                leaf(Behavior::AcquireCable {
                    key: Key::LowPriorityHunting,
                }),
            ],
            FIND_TARGET_RATE,
        ),
    );
    let idle = par(
        vec![
            random_walk(25.0),
            leaf(Behavior::AcquireCombatTarget {
                key: Key::CurrentTarget,
                strategy: Strategy::Basic,
            }),
            decorator(
                Cond::KeySet(Key::WantsToEatCable),
                false,
                Abort::None,
                leaf(Behavior::AcquireCable {
                    key: Key::LowPriorityHunting,
                }),
            ),
        ],
        FIND_TARGET_RATE,
    );
    selector(vec![par(
        vec![
            selector(vec![flee, eat_cable, idle]),
            speech_loop(&MOUSE_SPEECH),
            decorator(
                Cond::KeyOffCooldown(Key::CableHuntCooldown),
                false,
                Abort::None,
                decorator(
                    Cond::RandomChance(0.01),
                    false,
                    Abort::None,
                    leaf(Behavior::SetBool(Key::WantsToEatCable, true)),
                ),
            ),
        ],
        1.0,
    )])
}

/// `vermin/cockroach/cockroach.bt.json`.
// Left out: hunting ants (no ants here).
fn cockroach() -> Node {
    par(
        vec![random_walk(25.0), speech_loop(&COCKROACH_SPEECH)],
        FIND_TARGET_RATE,
    )
}

/// `pets/fox.bt.json`.
// Left out: escape captivity, pet commands.
fn fox() -> Node {
    selector(vec![
        decorator(
            Cond::KeySet(Key::FleeTarget),
            false,
            Abort::Lower,
            run_away(Key::FleeTarget),
        ),
        par(
            vec![
                par(
                    vec![
                        selector(vec![
                            decorator(
                                Cond::KeySet(Key::CurrentTarget),
                                false,
                                Abort::Both,
                                melee_combat(Strategy::Basic),
                            ),
                            random_walk(25.0),
                        ]),
                        speech_loop(&FOX_SPEECH),
                    ],
                    1.0,
                ),
                decorator(
                    Cond::NoHumansWatching,
                    false,
                    Abort::None,
                    leaf(Behavior::AcquireCombatTarget {
                        key: Key::CurrentTarget,
                        strategy: Strategy::Basic,
                    }),
                ),
                leaf(Behavior::TargetFromRetaliateList {
                    key: Key::FleeTarget,
                    strategy: Strategy::IgnoreFaction,
                }),
            ],
            FIND_TARGET_RATE,
        ),
    ])
}
