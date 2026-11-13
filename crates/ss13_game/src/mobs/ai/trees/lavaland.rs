//! The lavaland controllers' trees (`code/modules/mob/living/basic/lavaland/*/*.bt.json`), with
//! the leaves of `crate::mobs::lavaland::ai`. Left out everywhere: escape captivity, attacking
//! obstructions, calling mining reinforcements (`call_reinforcements/mining`), the mining
//! target priority strategy (the nearest valid target is taken).

use super::super::behaviors::{Behavior, FIND_TARGET_RATE, Strategy};
use super::super::bt::{
    Abort, Cond, Node, ParallelOpts, decorator, leaf, parallel, selector, subplan,
};
use super::super::fauna::{FaunaCond, FaunaLeaf};
use super::super::{Key, Value};
use super::{
    BASIC, Built, fearful_combat, hostile_combat, melee_combat, par, random_walk, ranged_attack,
    retaliate_combat, retaliate_target, run_away, speech_loop_of, under, while_closing_in,
};
use crate::mobs::lavaland::ai::{LavaCond, LavaLeaf};

fn lava(leaf_: LavaLeaf) -> Node {
    leaf(Behavior::Fauna(FaunaLeaf::Lavaland(leaf_)))
}

fn lava_cond(cond: LavaCond) -> Cond {
    Cond::Fauna(FaunaCond::Lavaland(cond))
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

fn move_to_target() -> Node {
    leaf(Behavior::MoveToTarget {
        key: Key::CurrentTarget,
        dist: 1,
        finish_on_arrival: false,
    })
}

/// The fight parallel of these trees: `attack` looping (`subplan`), `mover` alongside
/// (`BT_PARALLEL_FAILURE_CHILD_ONE`: only the attack loop's end ends it).
fn fight(attack: Node, mover: Node) -> Node {
    parallel(
        vec![subplan(attack, true, true, 0.0), mover],
        ParallelOpts {
            repeat_secondary: true,
            finish_on_primary: true,
            ..ParallelOpts::default()
        },
    )
}

/// `selector(bb_key_set(BB_CURRENT_TARGET) -> combat, random_walk)` in a parallel with the
/// target finders.
fn combat_or_walk(combat: Node, targeting: Node) -> Node {
    par(
        vec![
            selector(vec![
                decorator(Cond::KeySet(Key::CurrentTarget), false, Abort::Both, combat),
                random_walk(25.0),
            ]),
            targeting,
        ],
        FIND_TARGET_RATE,
    )
}

/// `update_combat_targets`, then `target_from_retaliate_list`.
fn acquire_then_retaliate() -> Node {
    selector(vec![acquire(), retaliate_target(Key::CurrentTarget)])
}

/// `target_from_retaliate_list`, then `update_combat_targets`.
fn retaliate_then_acquire() -> Node {
    selector(vec![retaliate_target(Key::CurrentTarget), acquire()])
}

/// The tree of lavaland controller `path`, `None` when it is not one of them.
pub(super) fn build(path: &str) -> Option<Built> {
    let is = |sub: &str| under(path, &format!("{BASIC}/{sub}"));
    let mut flees_injured = false;
    let roots = if is("goliath") {
        // `goliath.bt.json`; left out: eating ash flora, digging holes.
        combat_or_walk(
            fight(
                selector(vec![
                    lava(LavaLeaf::GoliathTentacles {
                        key: Key::CurrentTarget,
                    }),
                    melee(),
                ]),
                move_to_target(),
            ),
            acquire_then_retaliate(),
        )
    } else if is("watcher") {
        // `watcher.bt.json` (skirmish 3-5); left out: the gaze (`use_mob_ability` after 5 s on
        // a target), the overwatch check.
        combat_or_walk(
            fight(
                ranged_attack(0.6, false),
                lava(LavaLeaf::MaintainDistance {
                    key: Key::CurrentTarget,
                    min: 3,
                    max: 5,
                }),
            ),
            retaliate_then_acquire(),
        )
    } else if is("legion_brood") {
        // `legion_brood.bt.json`.
        combat_or_walk(
            while_closing_in(
                lava(LavaLeaf::BroodBite {
                    key: Key::CurrentTarget,
                }),
                move_to_target(),
            ),
            acquire(),
        )
    } else if is("legion") || is("legion_lesser") {
        // `legion.bt.json` (`BB_AGGRO_RANGE` 5, skirmish 4-6), with its gurgles.
        par(
            vec![
                selector(vec![
                    decorator(
                        Cond::KeySet(Key::CurrentTarget),
                        false,
                        Abort::Both,
                        fight(
                            lava(LavaLeaf::LaunchBrood {
                                key: Key::CurrentTarget,
                            }),
                            lava(LavaLeaf::MaintainDistance {
                                key: Key::CurrentTarget,
                                min: 4,
                                max: 6,
                            }),
                        ),
                    ),
                    random_walk(25.0),
                ]),
                lava(LavaLeaf::AcquireWithin {
                    key: Key::CurrentTarget,
                    range: 5,
                }),
                speech_loop_of(Some(speech!("legion"))),
            ],
            FIND_TARGET_RATE,
        )
    } else if is("brimdemon") {
        // `brimdemon.bt.json`: bites what is next to it, lines up 2-9 tiles away and fires.
        combat_or_walk(
            fight(
                selector(vec![
                    melee(),
                    lava(LavaLeaf::Brimbeam {
                        key: Key::CurrentTarget,
                        max_range: 9,
                    }),
                ]),
                lava(LavaLeaf::MoveToCardinal {
                    key: Key::CurrentTarget,
                    min: 2,
                    max: 9,
                }),
            ),
            retaliate_then_acquire(),
        )
    } else if is("lobstrosity") {
        // `lobstrosity.bt.json`: rushes, bites, closes in on a downed target and keeps away
        // from one standing; left out: fishing, snipping arms, pet commands, eating arms.
        let combat = fight(
            selector(vec![
                lava(LavaLeaf::Rush {
                    key: Key::CurrentTarget,
                }),
                melee(),
            ]),
            selector(vec![
                decorator(
                    lava_cond(LavaCond::TargetDown(Key::CurrentTarget)),
                    false,
                    Abort::SelfOnly,
                    move_to_target(),
                ),
                decorator(
                    lava_cond(LavaCond::TargetDown(Key::CurrentTarget)),
                    true,
                    Abort::SelfOnly,
                    run_away(Key::CurrentTarget),
                ),
            ]),
        );
        par(
            vec![
                selector(vec![
                    decorator(Cond::KeySet(Key::CurrentTarget), false, Abort::Both, combat),
                    random_walk(25.0),
                ]),
                speech_loop_of(None),
                acquire(),
            ],
            FIND_TARGET_RATE,
        )
    } else if is("bileworm") {
        // `bileworm.bt.json`: it never walks; spews at its target. Left out: burrowing and
        // resurfacing next to the target, devouring the downed.
        par(
            vec![
                selector(vec![
                    decorator(
                        Cond::KeySet(Key::CurrentTarget),
                        false,
                        Abort::Both,
                        subplan(
                            lava(LavaLeaf::SpewBile {
                                key: Key::CurrentTarget,
                            }),
                            true,
                            true,
                            0.0,
                        ),
                    ),
                    leaf(Behavior::Fauna(FaunaLeaf::Wait)),
                ]),
                acquire(),
            ],
            FIND_TARGET_RATE,
        )
    } else if is("goldgrub") || is("babygrub") {
        // `goldgrub.bt.json`: runs from whoever it sees; left out: burrowing away, eating ore,
        // mining walls, spitting ore.
        fearful_combat()
    } else if is("hivelord") {
        // `simple_ability_ranged_combat`: throws brood keeping its distance.
        combat_or_walk(
            fight(
                lava(LavaLeaf::LaunchBrood {
                    key: Key::CurrentTarget,
                }),
                lava(LavaLeaf::MaintainDistance {
                    key: Key::CurrentTarget,
                    min: 4,
                    max: 6,
                }),
            ),
            acquire(),
        )
    } else if is("basilisk") {
        // `basilisk.bt.json` (`BB_AGGRO_RANGE` 5): shoots from 2 tiles, bites closer; left out:
        // the overwatch check, heating up in lava.
        combat_or_walk(
            fight(
                selector(vec![
                    decorator(
                        lava_cond(LavaCond::AtLeast(Key::CurrentTarget, 2)),
                        false,
                        Abort::None,
                        ranged_attack(0.6, false),
                    ),
                    melee(),
                ]),
                lava(LavaLeaf::MaintainDistance {
                    key: Key::CurrentTarget,
                    min: 4,
                    max: 6,
                }),
            ),
            lava(LavaLeaf::AcquireWithin {
                key: Key::CurrentTarget,
                range: 5,
            }),
        )
    } else if is("gutlunch/gutlunch_warrior") {
        // `gutlunch_warrior.bt.json`: fights back; left out: breeding, befriending ash walkers.
        retaliate_combat()
    } else if is("polar") {
        // `polar.bt.json`: `simple_hostile_combat` (`BB_AGGRO_RANGE` 2) with the retaliate
        // list and `enrage` alongside; left out: its `bear` combat lines.
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
                lava(LavaLeaf::AcquireWithin {
                    key: Key::CurrentTarget,
                    range: 2,
                }),
                retaliate_target(Key::CurrentTarget),
                lava(LavaLeaf::Enrage),
            ],
            FIND_TARGET_RATE,
        )
    } else if is("wolf") {
        // `wolf.bt.json`: runs while hurt (`ai_flee_while_injured`), else bites closing in and
        // howls for the pack; left out: pet commands (taming), attacking obstructions.
        flees_injured = true;
        combat_or_walk(
            selector(vec![
                run_away(Key::CurrentTarget),
                parallel(
                    vec![
                        subplan(melee(), true, true, 0.0),
                        lava(LavaLeaf::CallReinforcements {
                            key: Key::CurrentTarget,
                        }),
                        move_to_target(),
                    ],
                    ParallelOpts {
                        failure_any: true,
                        repeat_secondary: true,
                        finish_on_primary: true,
                        ..ParallelOpts::default()
                    },
                ),
            ]),
            acquire(),
        )
    } else if is("ice_whelp") {
        // `ice_whelp.bt.json`: the eruption within 3, the line within 7, else a bite; left
        // out: attacking obstructions, eating dead whelps, sculpting rocks, burning trees.
        combat_or_walk(
            fight(
                selector(vec![
                    lava(LavaLeaf::WhelpBreath {
                        key: Key::CurrentTarget,
                        wide: true,
                        max: 3,
                    }),
                    lava(LavaLeaf::WhelpBreath {
                        key: Key::CurrentTarget,
                        wide: false,
                        max: 7,
                    }),
                    melee(),
                ]),
                move_to_target(),
            ),
            acquire(),
        )
    } else if is("mook/support") {
        // `support.bt.json`: fights back (`target_from_retaliate_list`), else heals hurt
        // mooks it sees (`heal_injured`); left out: the village (see `lavaland::mook`).
        par(
            vec![
                selector(vec![
                    decorator(
                        Cond::KeySet(Key::CurrentTarget),
                        false,
                        Abort::Both,
                        fight(melee(), move_to_target()),
                    ),
                    lava(LavaLeaf::MookHeal { range: 9 }),
                    random_walk(25.0),
                ]),
                retaliate_target(Key::CurrentTarget),
            ],
            FIND_TARGET_RATE,
        )
    } else if is("mook") {
        // `mook.bt.json` and the bard's and chief's: `worker_find_targets` (the retaliate list,
        // then `update_combat_targets`), leaping at the target, else hacking at it; left out:
        // mining, the chief's commands, the bard's song, storm shelter.
        combat_or_walk(
            fight(
                selector(vec![
                    lava(LavaLeaf::MookLeap {
                        key: Key::CurrentTarget,
                    }),
                    melee(),
                ]),
                move_to_target(),
            ),
            retaliate_then_acquire(),
        )
    } else if is("minebot") {
        // `minebot.bt.json`: fights back (drilling: no gun here), repairs hurt NODE drones,
        // collects loose ore; left out: following and protecting its miner, pet commands,
        // shooting, missiles and mines, mining walls, the SOS (see `lavaland::minebot`).
        par(
            vec![
                selector(vec![
                    decorator(
                        Cond::KeySet(Key::CurrentTarget),
                        false,
                        Abort::Both,
                        fight(melee(), move_to_target()),
                    ),
                    lava(LavaLeaf::MinebotRepair),
                    lava(LavaLeaf::MinebotCollect),
                    leaf(Behavior::Fauna(FaunaLeaf::Wait)),
                ]),
                retaliate_target(Key::CurrentTarget),
            ],
            FIND_TARGET_RATE,
        )
    } else if is("raptor") || is("baby_raptor") {
        // `raptor_ai.bt.json` (and the aggressive breeds', the babies'): fights back; left out:
        // fleeing when hurt, eating, troughs, breeding, playing with the owner, healing
        // (white), mothers caring for the young and babies following them, pet commands.
        retaliate_combat()
    } else if is("node_drone") {
        // `node_drone.bt.json`: it only latches onto an unclaimed ore vent (vents land their
        // own here, `crate::mining::vents`);
        // `PASSIVE_AI_FLAGS`, it never fights.
        leaf(Behavior::Fauna(FaunaLeaf::Wait))
    } else if is("ice_demon/afterimage") {
        // `ice_demon_afterimage.bt.json`: `simple_hostile_combat`; left out: fleeing fire.
        hostile_combat(25.0)
    } else if is("ice_demon") {
        // `ice_demon.bt.json` with `ice_demon_combat` (`BB_RANGED_SKIRMISH_MAX_DISTANCE` 7):
        // jumps away from a target next to it, conjures afterimages under 75 health, jumps
        // around its target, shoots; left out: fleeing a lit welder or flare (and its iced
        // floors).
        combat_or_walk(
            fight(
                selector(vec![
                    lava(LavaLeaf::DemonTeleport {
                        key: Key::CurrentTarget,
                        escape: true,
                    }),
                    lava(LavaLeaf::DemonClone),
                    lava(LavaLeaf::DemonTeleport {
                        key: Key::CurrentTarget,
                        escape: false,
                    }),
                    ranged_attack(0.6, false),
                ]),
                lava(LavaLeaf::MaintainDistance {
                    key: Key::CurrentTarget,
                    min: 4,
                    max: 7,
                }),
            ),
            acquire(),
        )
    } else {
        return None;
    };
    // `ai_retaliate` on every mining mob.
    Some(Built {
        roots: vec![roots],
        blackboard: if flees_injured {
            vec![(Key::StopFleeing, Value::Bool(true))]
        } else {
            Vec::new()
        },
        traits: 0,
        retaliates: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_lavaland_controller_has_a_tree() {
        for sub in [
            "goliath",
            "watcher",
            "legion",
            "legion_brood",
            "legion_lesser",
            "brimdemon",
            "lobstrosity",
            "lobstrosity/juvenile",
            "bileworm",
            "goldgrub",
            "hivelord",
            "basilisk",
            "gutlunch/gutlunch_warrior",
            "polar",
            "wolf",
            "ice_whelp",
            "ice_demon",
            "ice_demon/afterimage",
            "mook",
            "mook/support",
            "mook/bard",
            "mook/tribal_chief",
            "minebot",
            "node_drone",
            "raptor",
            "raptor/aggressive",
            "baby_raptor",
        ] {
            let built = build(&format!("{BASIC}/{sub}"));
            assert!(built.is_some_and(|b| b.retaliates), "{sub}");
        }
        assert!(build(&format!("{BASIC}/carp")).is_none());
        // The wolf starts not fleeing (`ai_flee_while_injured`).
        let wolf = build(&format!("{BASIC}/wolf")).unwrap();
        assert_eq!(wolf.blackboard, vec![(Key::StopFleeing, Value::Bool(true))]);
    }
}
