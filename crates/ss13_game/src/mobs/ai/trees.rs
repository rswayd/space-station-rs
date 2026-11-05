//! The ported controllers' trees, node for node after their tg `*.bt.json` (the file named on
//! each builder) or BandaStation's `behavior_nodes` (`modular_bandastation/mobs/code/
//! ai_controller.dm`). Branches whose leaves need systems ss13-rs lacks are left out and named
//! (escape captivity, pet commands, breeding, cheese, instruments, carp rifts and magic, cat
//! homes and food, tipping, climbing trees, swimming, mob abilities); a selector then falls
//! through to the next branch as tg's does when those fail. A parallel that keeps one child
//! is that child.
//!
//! tg guards several "fight/flee when someone is on the list" branches with `BT_ABORT_SELF`
//! next to a looping `random_walk` sibling: the selector resumes the walk forever and the
//! branch is never entered once the mob walks (skittish mobs never flee, goats never butt
//! back). Those decorators abort lower priorities here too (marked `tg BT_ABORT_SELF`).
//!
//! Speech comes from `mobs.ron`: a `random_speech` type by path ([`Behavior::Speak`] with
//! `Some`), or the controller's `BB_BASIC_MOB_SPEAK_LINES` (`None`).

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
const BASIC: &str = "/datum/ai_controller/basic_controller";

/// A `/datum/bt_node/ai_behavior/random_speech` subtype.
macro_rules! speech {
    ($name:literal) => {
        concat!("/datum/bt_node/ai_behavior/random_speech/", $name)
    };
}

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
pub fn build(path: &str, def: &MobDef) -> Option<Built> {
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
    if under(path, crate::restaurant::CUSTOMER_CONTROLLER) {
        return Some(crate::restaurant::tree());
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
    } else if let Some(generic) = generic(path) {
        built = generic;
        // DM `STOP_MOVING_WHEN_PULLED` of the controller's `ai_traits`.
        if def.ai_traits & 1 != 0 {
            built.traits |= STOP_MOVING_WHEN_PULLED;
        }
    } else {
        // shortcut: controllers not ported yet only wander (`random_walk`), so placed pets are
        // not statues; port their trees one by one.
        built.roots = vec![random_walk(25.0)];
    }
    Some(built)
}

/// The controllers built from the generic templates and subtrees (`generic_controllers.dm`,
/// `generic_subtrees/`, `basic_subtrees/`), checked most specific first.
fn generic(path: &str) -> Option<Built> {
    let is = |sub: &str| under(path, &format!("{BASIC}/{sub}"));
    let mut retaliates = false;
    let mut flees_injured = false;
    let roots = if is("simple/simple_hostile") || is("simple/simple_hostile_obstacles") {
        // `simple_hostile.bt.json`; the obstacle smashing of `simple_hostile_obstacles` is
        // left out (no `attack_obstructions`).
        hostile_combat(25.0)
    } else if is("simple/simple_retaliate") {
        retaliates = true;
        retaliate_combat()
    } else if is("simple/simple_fearful") {
        fearful_combat()
    } else if is("simple/simple_skittish") {
        retaliates = true;
        skittish_combat()
    } else if is("simple/simple_capricious") {
        retaliates = true;
        capricious_combat()
    } else if is("talk") {
        speech_loop_of(None)
    } else if is("base_animal") {
        // BandaStation `base_animal`.
        retaliates = true;
        par(
            vec![skittish_combat(), speak(Some(speech!("base_animal")))],
            1.0,
        )
    } else if is("possum") {
        // BandaStation `possum`.
        retaliates = true;
        par(
            vec![capricious_combat(), speak(Some(speech!("possum")))],
            1.0,
        )
    } else if is("lizard/big") {
        // BandaStation `lizard/big`: `simple_hostile_combat` and its own lines.
        retaliates = true;
        par(
            vec![hostile_combat(25.0), speak(Some(speech!("lizard/big")))],
            1.0,
        )
    } else if is("crocodile") {
        // BandaStation `crocodile`: its `go_for_swim` and `find_food` come after
        // `simple_hostile_combat`, which never fails (it walks), so they never run.
        retaliates = true;
        par(
            vec![hostile_combat(25.0), speak(Some(speech!("crocodile")))],
            1.0,
        )
    } else if is("lizard") {
        // `vermin/lizard.bt.json`.
        par(
            vec![
                selector(vec![move_to_and_eat(), random_walk(25.0)]),
                find_food(),
                speech_loop_of(Some(speech!("lizard"))),
            ],
            FIND_TARGET_RATE,
        )
    } else if is("chicken/cock") {
        // BandaStation `chicken/cock`.
        retaliates = true;
        par(vec![capricious_combat(), speak(None)], 1.0)
    } else if is("chicken") || is("rabbit") || is("sheep") {
        // `chicken.bt.json`, `rabbit.bt.json`, `sheep.bt.json`: `skittish_and_speak`.
        retaliates = true;
        skittish_and_speak(None)
    } else if is("pig") {
        // `farm_animals/pig.bt.json` (BandaStation's big pigs keep it).
        retaliates = true;
        flees_injured = true;
        par(
            vec![
                subplan(brawler_combat(), true, true, 0.0),
                speech_loop_of(None),
            ],
            1.0,
        )
    } else if is("pony") {
        // `farm_animals/pony.bt.json`: untamed (no taming here), so the brawler branch.
        retaliates = true;
        flees_injured = true;
        par(
            vec![
                subplan(brawler_combat(), true, true, 0.0),
                speech_loop_of(None),
            ],
            1.0,
        )
    } else if is("cow") {
        // `farm_animals/cow/cow.bt.json`; left out: `tip_reaction`.
        selector(vec![par(
            vec![
                subplan(
                    selector(vec![move_to_and_eat(), random_walk(25.0)]),
                    true,
                    true,
                    0.0,
                ),
                find_food(),
                speech_loop_of(None),
            ],
            FIND_TARGET_RATE,
        )])
    } else if is("goat") {
        // `farm_animals/goat/goat.bt.json`.
        retaliates = true;
        par(
            vec![
                subplan(forage_and_retaliate(true, true), true, true, 0.0),
                speech_loop_of(None),
            ],
            1.0,
        )
    } else if is("goose/calm") {
        // `farm_animals/goose/goose_calm.bt.json` (Birdboat): only fights back; left out: the
        // goose's own food finder (`goose_edible`: anything it can swallow).
        retaliates = true;
        par(
            vec![
                subplan(forage_and_retaliate(false, false), true, true, 0.0),
                speech_loop_of(None),
            ],
            1.0,
        )
    } else if is("goose") {
        // `farm_animals/goose/goose.bt.json`; left out: escape captivity and the goose's own
        // food finder.
        retaliates = true;
        selector(vec![par(
            vec![
                subplan(forage_and_retaliate(false, true), true, true, 0.0),
                speech_loop_of(None),
            ],
            1.0,
        )])
    } else if is("sloth") {
        // `pets/sloth.bt.json`; left out: `climb_tree`.
        retaliates = true;
        par(
            vec![
                selector(vec![
                    decorator(
                        Cond::KeySet(Key::CurrentTarget),
                        false,
                        Abort::Both, // tg BT_ABORT_SELF, see the module docs
                        run_away(Key::CurrentTarget),
                    ),
                    random_walk(25.0),
                ]),
                speech_loop_of(None),
                retaliate_target(Key::CurrentTarget),
            ],
            FIND_TARGET_RATE,
        )
    } else if is("mothroach") {
        // `vermin/mothroach/mothroach.bt.json`; left out: pet commands.
        retaliates = true;
        selector(vec![par(
            vec![
                par(
                    vec![
                        selector(vec![
                            decorator(
                                Cond::KeySet(Key::CurrentTarget),
                                false,
                                Abort::Both, // tg BT_ABORT_SELF, see the module docs
                                run_away(Key::CurrentTarget),
                            ),
                            move_to_and_eat(),
                            random_walk(25.0),
                        ]),
                        retaliate_target(Key::CurrentTarget),
                        find_food(),
                    ],
                    FIND_TARGET_RATE,
                ),
                speak(Some(speech!("mothroach"))),
            ],
            1.0,
        )])
    } else if is("pet/penguin") || is("penguin") {
        // `pets/penguin/penguin.bt.json` (chicks too: `penguin_baby.bt.json` is it without
        // fishing, plus following their mother); left out: fishing, drilling ice, eggs,
        // chicks following their mother.
        retaliates = true;
        flees_injured = true;
        selector(vec![
            decorator(
                Cond::KeySet(Key::CurrentTarget),
                false,
                Abort::Both,
                run_away(Key::CurrentTarget),
            ),
            par(
                vec![
                    subplan(
                        selector(vec![move_to_and_eat(), random_walk(25.0)]),
                        true,
                        true,
                        0.0,
                    ),
                    speech_loop_of(Some(speech!("penguin"))),
                    retaliate_target(Key::CurrentTarget),
                    find_food(),
                ],
                FIND_TARGET_RATE,
            ),
        ])
    } else if is("snake") {
        // `space_fauna/snake/snake.bt.json` (and `banded.bt.json`); left out: escape captivity.
        selector(vec![par(
            vec![
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
                speech_loop_of(None),
                leaf(Behavior::AcquireCombatTarget {
                    key: Key::CurrentTarget,
                    strategy: Strategy::Basic,
                }),
                find_food(),
            ],
            FIND_TARGET_RATE,
        )])
    } else if is("bear") {
        // `space_fauna/bear/bear.bt.json`; left out: escape captivity, climbing trees,
        // raiding beehives and dragging honeycombs.
        retaliates = true;
        selector(vec![par(
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
                leaf(Behavior::AcquireCombatTarget {
                    key: Key::CurrentTarget,
                    strategy: Strategy::Basic,
                }),
                retaliate_target(Key::CurrentTarget),
                speak(Some(speech!("bear"))),
            ],
            FIND_TARGET_RATE,
        )])
    } else if is("frog") {
        // `vermin/frog.bt.json` with `frog_engage_target`; left out: fleeing scary fishermen,
        // swimming.
        retaliates = true;
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
                leaf(Behavior::TargetFromRetaliateList {
                    key: Key::CurrentTarget,
                    strategy: Strategy::Basic,
                }),
                speech_loop_of(Some(speech!("frog"))),
            ],
            FIND_TARGET_RATE,
        )
    } else if is("deer") {
        // `farm_animals/deer/deer.bt.json`; left out: staring at what moves, resting by trees,
        // playing, marking trees, grazing and drinking.
        retaliates = true;
        selector(vec![
            decorator(
                Cond::KeySet(Key::CurrentTarget),
                false,
                Abort::Both,
                run_away(Key::CurrentTarget),
            ),
            par(
                vec![
                    par(
                        vec![subplan(random_walk(25.0), true, true, 0.0), speak(None)],
                        1.0,
                    ),
                    retaliate_target(Key::CurrentTarget),
                ],
                FIND_TARGET_RATE,
            ),
        ])
    } else if is("space_bat") {
        // `vermin/space_bat.bt.json`.
        retaliates = true;
        retaliate_combat()
    } else if is("snail") {
        // `snails/snail.bt.json`; left out: hiding in the shell (a mob ability).
        retaliates = true;
        par(
            vec![
                selector(vec![move_to_and_eat(), random_walk(25.0)]),
                retaliate_target(Key::CurrentTarget),
                find_food(),
            ],
            FIND_TARGET_RATE,
        )
    } else if is("butterfly") || is("axolotl") {
        random_walk(25.0)
    } else if is("gondola") {
        random_walk(10.0)
    } else if is("mouse/rat/syndi") {
        // BandaStation `mouse/rat/syndi`: tg's rat tree binds the mouse's lines, so the
        // syndicate rats' own never play there; they are used here.
        rat(speech!("mouse/rat/syndi"))
    } else if is("mouse/rat") {
        // BandaStation adds a sound to the rat's lines (`random_speech/mouse/rat`), which tg's
        // tree never binds; used here.
        rat(speech!("mouse/rat"))
    } else {
        return None;
    };
    Some(Built {
        roots: vec![roots],
        blackboard: if flees_injured {
            vec![(Key::StopFleeing, Value::Bool(true))]
        } else {
            Vec::new()
        },
        traits: 0,
        retaliates,
    })
}

/// `random_speech` (`Some(path)`) or `random_speech_blackboard` (`None`).
fn speak(speech: Option<&'static str>) -> Node {
    leaf(Behavior::Speak(speech))
}

/// `basic_subtrees/random_speech_loop.bt.json` with mob data.
fn speech_loop_of(speech: Option<&'static str>) -> Node {
    subplan(speak(speech), true, true, 1.0)
}

/// `acquire_target/target_from_retaliate_list/nearest` into `key`.
fn retaliate_target(key: Key) -> Node {
    leaf(Behavior::TargetFromRetaliateList {
        key,
        strategy: Strategy::Basic,
    })
}

/// `basic_mobs/simple_hostile_combat.bt.json` (no `combat_speech_behavior`).
fn hostile_combat(walk_chance: f64) -> Node {
    par(
        vec![
            selector(vec![
                decorator(
                    Cond::KeySet(Key::CurrentTarget),
                    false,
                    Abort::Both,
                    melee_combat(Strategy::Basic),
                ),
                random_walk(walk_chance),
            ]),
            leaf(Behavior::AcquireCombatTarget {
                key: Key::CurrentTarget,
                strategy: Strategy::Basic,
            }),
        ],
        FIND_TARGET_RATE,
    )
}

/// `basic_mobs/simple_retaliate_combat.bt.json`.
fn retaliate_combat() -> Node {
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
            retaliate_target(Key::CurrentTarget),
        ],
        FIND_TARGET_RATE,
    )
}

/// `basic_mobs/simple_fearful_combat.bt.json`: runs from anyone it would fight.
fn fearful_combat() -> Node {
    selector(vec![
        par(
            vec![
                decorator(
                    Cond::KeySet(Key::CurrentTarget),
                    false,
                    Abort::Both,
                    run_away(Key::CurrentTarget),
                ),
                leaf(Behavior::AcquireCombatTarget {
                    key: Key::CurrentTarget,
                    strategy: Strategy::Basic,
                }),
            ],
            FIND_TARGET_RATE,
        ),
        random_walk(25.0),
    ])
}

/// `basic_mobs/simple_skittish_combat.bt.json`: runs from whoever hurt it.
fn skittish_combat() -> Node {
    selector(vec![
        par(
            vec![
                decorator(
                    Cond::KeySet(Key::CurrentTarget),
                    false,
                    Abort::Both, // tg BT_ABORT_SELF, see the module docs
                    run_away(Key::CurrentTarget),
                ),
                retaliate_target(Key::CurrentTarget),
            ],
            FIND_TARGET_RATE,
        ),
        random_walk(25.0),
    ])
}

/// `generic_subtrees/capricious_pick_target.bt.json`.
fn capricious_pick_target() -> Node {
    sequence(vec![
        leaf(Behavior::CapriciousRetaliate),
        retaliate_target(Key::CurrentTarget),
    ])
}

/// `basic_mobs/simple_capricious_combat.bt.json`: fights back, and picks fights at random.
fn capricious_combat() -> Node {
    par(
        vec![
            selector(vec![
                decorator(
                    Cond::KeySet(Key::RetaliateList),
                    false,
                    Abort::Both,
                    melee_combat(Strategy::Basic),
                ),
                random_walk(25.0),
            ]),
            capricious_pick_target(),
        ],
        FIND_TARGET_RATE,
    )
}

/// `basic_subtrees/skittish_brawler_combat.bt.json`: fights whoever hurt it, runs when hurt
/// (`ai_flee_while_injured`) and the attacker is within `DEFAULT_BASIC_FLEE_DISTANCE`.
fn brawler_combat() -> Node {
    par(
        vec![
            selector(vec![
                decorator(
                    Cond::KeySet(Key::CurrentTarget),
                    false,
                    Abort::Both, // tg BT_ABORT_SELF, see the module docs
                    selector(vec![
                        decorator(
                            Cond::AtDistance(Key::CurrentTarget, 9),
                            false,
                            Abort::None,
                            run_away(Key::CurrentTarget),
                        ),
                        melee_combat(Strategy::Basic),
                    ]),
                ),
                random_walk(25.0),
            ]),
            retaliate_target(Key::CurrentTarget),
        ],
        FIND_TARGET_RATE,
    )
}

/// `generic_subtrees/skittish_and_speak.bt.json`.
fn skittish_and_speak(speech: Option<&'static str>) -> Node {
    par(
        vec![subplan(skittish_combat(), true, true, 0.0), speak(speech)],
        1.0,
    )
}

/// `generic_subtrees/find_food.bt.json`.
fn find_food() -> Node {
    decorator(
        Cond::KeyOffCooldown(Key::NextFoodEat),
        false,
        Abort::SelfOnly,
        leaf(Behavior::AcquireFood),
    )
}

/// `generic_subtrees/move_to_and_eat.bt.json`; its `perform_emote` "eats up happily!" is not a
/// tg emote key (`/mob/emote` finds nothing and shows nothing), so it is left out.
fn move_to_and_eat() -> Node {
    decorator(
        Cond::KeySet(Key::TargetFood),
        false,
        Abort::Both,
        sequence(vec![
            leaf(Behavior::MoveToTarget {
                key: Key::TargetFood,
                dist: 1,
                finish_on_arrival: true,
            }),
            leaf(Behavior::EatFood),
        ]),
    )
}

/// `generic_subtrees/forage_and_retaliate.bt.json` (`find_food` as its food finder when
/// `food`; `capricious_pick_target` as its retaliate manager when `capricious`, else
/// `pick_retaliate_target`); `BB_DISABLE_IDLE` is never set by the ported mobs.
fn forage_and_retaliate(food: bool, capricious: bool) -> Node {
    let mut children = vec![
        selector(vec![
            decorator(
                Cond::KeySet(Key::RetaliateList),
                false,
                Abort::Both, // tg BT_ABORT_SELF, see the module docs
                melee_combat(Strategy::Basic),
            ),
            move_to_and_eat(),
            random_walk(25.0),
        ]),
        if capricious {
            capricious_pick_target()
        } else {
            retaliate_target(Key::CurrentTarget)
        },
    ];
    if food {
        children.push(find_food());
    }
    par(children, FIND_TARGET_RATE)
}

/// `eat_cable.bt.json` (mice and rats).
fn eat_cable() -> Node {
    decorator(
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
    )
}

/// The decision to go for a cable now and then (`BB_MOUSE_WANTS_TO_EAT_CABLE`).
fn want_cable() -> Node {
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
    )
}

/// `vermin/mouse_rat.bt.json`: rats fight what is not a rat. Left out: escape captivity, pet
/// commands, cheese, smashing obstacles. tg puts the cable wish after the speech loop in a
/// selector, where the loop never ends and the wish never comes; here both run (as in
/// `mouse.bt.json`).
fn rat(speech: &'static str) -> Node {
    selector(vec![par(
        vec![
            selector(vec![
                decorator(
                    Cond::KeySet(Key::CurrentTarget),
                    false,
                    Abort::Both,
                    par(
                        vec![
                            melee_combat(Strategy::Basic),
                            leaf(Behavior::AcquireCombatTarget {
                                key: Key::CurrentTarget,
                                strategy: Strategy::Basic,
                            }),
                        ],
                        FIND_TARGET_RATE,
                    ),
                ),
                eat_cable(),
                random_walk(25.0),
            ]),
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
            speech_loop_of(Some(speech)),
            want_cable(),
        ],
        FIND_TARGET_RATE,
    )])
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
    let eat_cable = eat_cable();
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
            want_cable(),
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
