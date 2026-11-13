//! The jungle controllers' trees (`code/modules/mob/living/basic/jungle/**/*.bt.json` and the
//! suicide frog's `vermin/suicide_frog.bt.json`), with the leaves of `crate::mobs::jungle`.
//! Left out everywhere: escape captivity, pet commands, climbing trees (`climb_tree`), the
//! seedling's watering can errands (fetching a can, filling it, watering and weeding trays).

use super::super::Key;
use super::super::behaviors::{Behavior, FIND_TARGET_RATE, Strategy};
use super::super::bt::{
    Abort, Cond, Node, ParallelOpts, decorator, leaf, parallel, selector, sequence, subplan,
};
use super::super::fauna::{FaunaCond, FaunaLeaf};
use super::{BASIC, Built, melee_combat, par, random_walk, run_away, under};
use crate::mobs::jungle::ai::{JungleCond, JungleLeaf};

fn jungle(leaf_: JungleLeaf) -> Node {
    leaf(Behavior::Fauna(FaunaLeaf::Jungle(leaf_)))
}

fn jungle_cond(cond: JungleCond) -> Cond {
    Cond::Fauna(FaunaCond::Jungle(cond))
}

fn acquire() -> Node {
    leaf(Behavior::AcquireCombatTarget {
        key: Key::CurrentTarget,
        strategy: Strategy::Basic,
    })
}

fn melee() -> Node {
    leaf(Behavior::MeleeAttack {
        key: Key::CurrentTarget,
        strategy: Strategy::Basic,
    })
}

fn move_to(key: Key, dist: u32, finish_on_arrival: bool) -> Node {
    leaf(Behavior::MoveToTarget {
        key,
        dist,
        finish_on_arrival,
    })
}

/// `parallel(BT_PARALLEL_FAILURE_ANY)` of an attack loop and the walk to the target.
fn close_in(attack: Node) -> Node {
    parallel(
        vec![
            subplan(attack, true, true, 0.0),
            move_to(Key::CurrentTarget, 1, false),
        ],
        ParallelOpts {
            failure_any: true,
            repeat_secondary: true,
            finish_on_primary: true,
            ..ParallelOpts::default()
        },
    )
}

/// `selector(bb_key_set(BB_CURRENT_TARGET) -> combat, random_walk)` beside `update_combat_targets`.
fn combat_or_walk(combat: Node) -> Node {
    par(
        vec![
            selector(vec![
                decorator(Cond::KeySet(Key::CurrentTarget), false, Abort::Both, combat),
                random_walk(25.0),
            ]),
            acquire(),
        ],
        FIND_TARGET_RATE,
    )
}

/// The tree of jungle controller `path`, `None` when it is not one of them.
pub(super) fn build(path: &str) -> Option<Built> {
    let is = |sub: &str| under(path, &format!("{BASIC}/{sub}"));
    let mut retaliates = false;
    let roots = if is("leaper") {
        // `leaper.bt.json`: bubble, flop, blood rain, toads, else a bite, closing in.
        combat_or_walk(close_in(selector(vec![
            jungle(JungleLeaf::LeaperBubble {
                key: Key::CurrentTarget,
            }),
            jungle(JungleLeaf::LeaperFlop {
                key: Key::CurrentTarget,
            }),
            jungle(JungleLeaf::LeaperVolley {
                key: Key::CurrentTarget,
            }),
            jungle(JungleLeaf::LeaperSummon {
                key: Key::CurrentTarget,
            }),
            melee(),
        ])))
    } else if is("mega_arachnid") {
        mega_arachnid()
    } else if is("seedling/meanie") {
        // `seedling_meanie.bt.json`.
        combat_or_walk(close_in(selector(vec![
            jungle(JungleLeaf::SeedlingSeeds {
                key: Key::CurrentTarget,
            }),
            jungle(JungleLeaf::SeedlingBeam {
                key: Key::CurrentTarget,
            }),
            melee(),
        ])))
    } else if is("seedling") {
        seedling()
    } else if is("human_trap") {
        // `human_trap.bt.json` = `simple_ability_ranged_combat`: the tangle, else a shot (the
        // trap has none), keeping 4-6 tiles away.
        combat_or_walk(parallel(
            vec![
                subplan(
                    selector(vec![
                        jungle(JungleLeaf::VenusTangle {
                            key: Key::CurrentTarget,
                        }),
                        leaf(Behavior::Fauna(FaunaLeaf::RangedAttack {
                            key: Key::CurrentTarget,
                            cooldown: 0.0,
                            max_range: 9,
                            avoid_friendly_fire: false,
                        })),
                    ]),
                    true,
                    true,
                    0.0,
                ),
                leaf(Behavior::Fauna(FaunaLeaf::MaintainDistance {
                    key: Key::CurrentTarget,
                })),
            ],
            ParallelOpts {
                failure_any: true,
                repeat_secondary: true,
                finish_on_primary: true,
                ..ParallelOpts::default()
            },
        ))
    } else if is("frog/suicide_frog") {
        // `suicide_frog.bt.json`: anyone it sees is bitten (and blown up, `explode_on_attack`).
        retaliates = true;
        combat_or_walk(melee_combat(Strategy::Basic))
    } else {
        return None;
    };
    Some(Built {
        roots: vec![roots],
        blackboard: Vec::new(),
        traits: 0,
        retaliates,
    })
}

/// `mega_arachnid.bt.json` with `mega_arachnid_combat.bt.json`, as the mob is meant to act
/// ("won't engage anything above 50 health, or anything without legcuffs"): tg's flee subtree
/// loops forever and starves the attack branch, so here prey (not human, legcuffed or under
/// half health) is closed in on and bitten, anyone else is fled from within 5 tiles, leaving
/// acid, while snares fly at them. Between fights it breaks the lights it sees
/// (`surveillance_equipment`).
fn mega_arachnid() -> Node {
    let harass = selector(vec![
        decorator(
            jungle_cond(JungleCond::ArachnidSlipReady),
            false,
            Abort::None,
            jungle(JungleLeaf::ArachnidSlip),
        ),
        decorator(
            jungle_cond(JungleCond::TargetIsHuman(Key::CurrentTarget)),
            false,
            Abort::None,
            jungle(JungleLeaf::ArachnidRestrain {
                key: Key::CurrentTarget,
            }),
        ),
    ]);
    let flee = parallel(
        vec![
            run_away(Key::CurrentTarget),
            subplan(harass, true, true, 0.0),
        ],
        ParallelOpts {
            repeat_secondary: true,
            finish_on_primary: true,
            ..ParallelOpts::default()
        },
    );
    let combat = decorator(
        Cond::KeySet(Key::CurrentTarget),
        false,
        Abort::Both,
        selector(vec![
            decorator(
                jungle_cond(JungleCond::TargetIsPrey(Key::CurrentTarget)),
                false,
                Abort::Both,
                close_in(melee()),
            ),
            flee,
        ]),
    );
    let surveillance = decorator(
        Cond::KeySet(Key::LowPriorityHunting),
        false,
        Abort::Both, // tg BT_ABORT_SELF, see the module docs of `trees`
        sequence(vec![
            move_to(Key::LowPriorityHunting, 1, true),
            jungle(JungleLeaf::SmashMachine {
                key: Key::LowPriorityHunting,
            }),
            leaf(Behavior::ClearKey(Key::LowPriorityHunting)),
        ]),
    );
    par(
        vec![
            selector(vec![combat, surveillance, random_walk(25.0)]),
            acquire(),
            leaf(Behavior::Fauna(FaunaLeaf::AcquireLight {
                key: Key::LowPriorityHunting,
            })),
        ],
        FIND_TARGET_RATE,
    )
}

/// `seedling.bt.json` without the watering can: the pacifist gardener beams the trays it sees
/// whose plant is hurt (`beamable_hydro`, looked for every 4 s while the beam is ready).
fn seedling() -> Node {
    let beam = decorator(
        jungle_cond(JungleCond::SeedlingBeamReady),
        false,
        Abort::Lower,
        decorator(
            Cond::KeySet(Key::TargetHydro),
            false,
            Abort::Both,
            sequence(vec![
                move_to(Key::TargetHydro, 2, true),
                jungle(JungleLeaf::SeedlingBeam {
                    key: Key::TargetHydro,
                }),
                leaf(Behavior::ClearKey(Key::TargetHydro)),
            ]),
        ),
    );
    par(
        vec![
            selector(vec![beam, random_walk(25.0)]),
            decorator(
                jungle_cond(JungleCond::SeedlingBeamReady),
                false,
                Abort::None,
                jungle(JungleLeaf::AcquireBeamableTray {
                    key: Key::TargetHydro,
                }),
            ),
        ],
        FIND_TARGET_RATE,
    )
}
