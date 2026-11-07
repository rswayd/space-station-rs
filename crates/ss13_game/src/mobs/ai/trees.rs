//! The ported controllers' trees, node for node after their tg `*.bt.json` (the file named on
//! each builder) or BandaStation's `behavior_nodes` (`modular_bandastation/mobs/code/
//! ai_controller.dm`). Branches whose leaves need systems ss13-rs lacks are left out and named
//! (escape captivity, cheese, instruments, carp rifts and magic, cat homes and food, climbing
//! trees, swimming, attacking obstructions, most mob abilities); a selector then falls
//! through to the next branch as tg's does when those fail. A parallel that keeps one child
//! is that child.
//!
//! tg guards several "fight/flee when someone is on the list" branches with `BT_ABORT_SELF`
//! next to a looping `random_walk` sibling: the selector resumes the walk forever and the
//! branch is never entered once the mob walks (skittish mobs never flee, goats never butt
//! back). Those decorators abort lower priorities here too (marked `tg BT_ABORT_SELF`).
//!
//! Mobs with tg's `SUBPLAN_ID_PET_COMMAND` override slot get [`pet_command_root`] as their first
//! root (`crate::mobs::pets`).
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
use super::fauna::{FaunaCond, FaunaLeaf};
use super::{Key, STOP_MOVING_WHEN_PULLED, Value};
use crate::mobs::pets::PetCommand;

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

mod lavaland;
mod troopers;

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
    } else if let Some(lava) = lavaland::build(path) {
        built = lava;
    } else if let Some(trooper) = troopers::build(path) {
        built = trooper;
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
    if crate::mobs::pets::has_pet_subtree(&def.path) {
        built.roots.insert(0, pet_command_root());
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
    } else if is("cow/wisdom") {
        // `farm_animals/cow/cow_wisdom.bt.json`: no food, `GLOB.wisdoms` lines.
        selector(vec![
            tip_reaction(),
            par(
                vec![
                    random_walk(25.0),
                    speech_loop(&crate::mobs::wisdoms::WISDOM_SPEECH),
                ],
                1.0,
            ),
        ])
    } else if is("cow") {
        // `farm_animals/cow/cow.bt.json`.
        selector(vec![
            tip_reaction(),
            par(
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
            ),
        ])
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
        // `vermin/mothroach/mothroach.bt.json`.
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
    } else if is("crab") {
        // `vermin/crab.bt.json`: hunts smaller mobs, flees whoever hurt it while hurt; left out:
        // escape captivity, swimming.
        retaliates = true;
        flees_injured = true;
        par(
            vec![
                selector(vec![
                    decorator(
                        Cond::KeySet(Key::FleeTarget),
                        false,
                        Abort::Both,
                        run_away(Key::FleeTarget),
                    ),
                    decorator(
                        Cond::KeySet(Key::CurrentTarget),
                        false,
                        Abort::Both,
                        melee_combat(Strategy::Smaller),
                    ),
                    random_walk(25.0),
                ]),
                decorator(
                    Cond::KeySet(Key::StopFleeing),
                    true,
                    Abort::None,
                    leaf(Behavior::TargetFromRetaliateList {
                        key: Key::FleeTarget,
                        strategy: Strategy::Basic,
                    }),
                ),
                leaf(Behavior::AcquireCombatTarget {
                    key: Key::CurrentTarget,
                    strategy: Strategy::Smaller,
                }),
            ],
            FIND_TARGET_RATE,
        )
    } else if is("turtle") {
        turtle()
    } else if is("parrot") {
        // `pets/parrot/parrot_ai/parrot.bt.json`; left out: escape captivity, perching, hoarding.
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
                retaliate_target(Key::CurrentTarget),
                leaf(Behavior::Fauna(FaunaLeaf::ParrotRepeat)),
            ],
            FIND_TARGET_RATE,
        )])
    } else if is("hivebot/mechanic") {
        // `hivebot_mechanic.bt.json`; left out: repairing machines, relaying to a partner.
        hivebot(Some((0.6, false)))
    } else if is("hivebot/ranged/rapid") {
        hivebot(Some((1.5, true)))
    } else if is("hivebot/ranged") {
        hivebot(Some((3.0, true)))
    } else if is("hivebot") {
        hivebot(None)
    } else if is("simple/simple_ranged_retaliate") {
        // `simple_ranged_retaliate_combat.bt.json`.
        retaliates = true;
        ranged_combat(retaliate_target(Key::CurrentTarget))
    } else if is("simple/simple_ranged") {
        // `simple_ranged_combat.bt.json` (`idle_behavior` bound to `random_walk`).
        ranged_combat(leaf(Behavior::AcquireCombatTarget {
            key: Key::CurrentTarget,
            strategy: Strategy::Basic,
        }))
    } else if is("simple/simple_skirmisher") {
        // `simple_skirmisher_combat.bt.json`: walks up, bites and shoots.
        par(
            vec![
                selector(vec![
                    decorator(
                        Cond::KeySet(Key::CurrentTarget),
                        false,
                        Abort::Both,
                        while_closing_in(
                            selector(vec![
                                leaf(Behavior::MeleeAttack {
                                    key: Key::CurrentTarget,
                                    strategy: Strategy::Basic,
                                }),
                                ranged_attack(0.6, false),
                            ]),
                            leaf(Behavior::MoveToTarget {
                                key: Key::CurrentTarget,
                                dist: 1,
                                finish_on_arrival: false,
                            }),
                        ),
                    ),
                    random_walk(25.0),
                ]),
                leaf(Behavior::AcquireCombatTarget {
                    key: Key::CurrentTarget,
                    strategy: Strategy::Basic,
                }),
            ],
            FIND_TARGET_RATE,
        )
    } else if is("stickman/ranged") {
        // `stickman_ranged.bt.json`: `simple_ranged_combat` (5 s between shots).
        ranged_combat(leaf(Behavior::AcquireCombatTarget {
            key: Key::CurrentTarget,
            strategy: Strategy::Basic,
        }))
    } else if is("stickman") || is("fleshblob") || is("mad_piano") || is("faithless") {
        // `stickman.bt.json`, `fleshblob.bt.json`, `mad_piano.bt.json`: `simple_hostile_combat`;
        // `faithless.bt.json` without its light smashing and obstacle attacks.
        hostile_combat(25.0)
    } else if is("alien/sentinel") || is("alien/queen") {
        // `ranged_alien_combat` without the alien projectiles (not ported): they only wander.
        random_walk(25.0)
    } else if is("alien") {
        // `basic_alien.bt.json` with `melee_alien_combat`; left out: weeds and eggs.
        hostile_combat(25.0)
    } else if is("zombie") {
        // `zombie.bt.json`, `zombie_stupid.bt.json`: hostile and moaning; left out: obstacles.
        par(vec![hostile_combat(25.0), speech_loop_of(None)], 1.0)
    } else if is("skeleton") {
        // `skeleton.bt.json`; its "rattles" `perform_emote` is not a tg emote key (nothing shows).
        par(
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
                leaf(Behavior::AcquireCombatTarget {
                    key: Key::CurrentTarget,
                    strategy: Strategy::Basic,
                }),
                find_food(),
            ],
            FIND_TARGET_RATE,
        )
    } else if is("clown") {
        // `clown/clown.bt.json`: `simple_retaliate_combat` and its honks.
        retaliates = true;
        par(vec![retaliate_combat(), speech_loop_of(None)], 1.0)
    } else if is("cat_butcherer") {
        // `simple_hostile_combat_with_retaliate`.
        retaliates = true;
        par(
            vec![hostile_combat(25.0), retaliate_target(Key::CurrentTarget)],
            FIND_TARGET_RATE,
        )
    } else if is("garden_gnome") {
        // `garden_gnome.bt.json`: fights back; left out: escape captivity.
        retaliates = true;
        retaliate_combat()
    } else if is("juggernaut") {
        // `juggernaut.bt.json`: `simple_hostile_obstacles_combat` without the obstacles.
        hostile_combat(25.0)
    } else if is("wraith") {
        // `wraith.bt.json`: hunts the most wounded in view; left out: obstacles.
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
                leaf(Behavior::AcquireMostWounded {
                    key: Key::CurrentTarget,
                    strategy: Strategy::Basic,
                }),
            ],
            FIND_TARGET_RATE,
        )
    } else if is("proteon") {
        // `proteon.bt.json`: `skittish_brawler_combat` on whoever hit it
        // (`ai_retaliate_advanced`); left out: forgetting them after 2-4 s.
        retaliates = true;
        brawler_combat()
    } else if is("artificer") {
        // `artificer.bt.json`: repairs the most wounded construct or shade in view
        // (`healing_touch` on its "attack"), else runs from whoever hit it (`ai_retaliate`).
        retaliates = true;
        par(
            vec![
                selector(vec![
                    decorator(
                        Cond::KeySet(Key::CurrentTarget),
                        false,
                        Abort::Both,
                        melee_combat(Strategy::Construct),
                    ),
                    decorator(
                        Cond::KeySet(Key::FleeTarget),
                        false,
                        Abort::Both,
                        run_away(Key::FleeTarget),
                    ),
                    random_walk(25.0),
                ]),
                leaf(Behavior::AcquireMostWounded {
                    key: Key::CurrentTarget,
                    strategy: Strategy::Construct,
                }),
                retaliate_target(Key::FleeTarget),
            ],
            FIND_TARGET_RATE,
        )
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

/// `basic_subtrees/tip_reaction.bt.json`.
fn tip_reaction() -> Node {
    decorator(
        Cond::KeySet(Key::TipReacting),
        false,
        Abort::Lower,
        leaf(Behavior::Fauna(FaunaLeaf::TippedReaction)),
    )
}

/// The `SUBPLAN_ID_PET_COMMAND` subtree with the ported commands' subtrees
/// (`pet_command_stay`, `_follow`, `_attack`, `_play_dead`): active while a command is set.
pub fn pet_command_root() -> Node {
    let command = |c: PetCommand, child: Node| {
        decorator(
            Cond::Fauna(FaunaCond::PetCommand(c)),
            false,
            Abort::SelfOnly,
            child,
        )
    };
    decorator(
        Cond::KeySet(Key::ActivePetCommand),
        false,
        Abort::Both,
        selector(vec![
            command(PetCommand::Stay, leaf(Behavior::Fauna(FaunaLeaf::Wait))),
            command(
                PetCommand::Follow,
                decorator(
                    Cond::KeySet(Key::PetTarget),
                    false,
                    Abort::Both,
                    subplan(
                        leaf(Behavior::MoveToTarget {
                            key: Key::PetTarget,
                            dist: 1,
                            finish_on_arrival: true,
                        }),
                        true,
                        false,
                        0.0,
                    ),
                ),
            ),
            command(
                PetCommand::Attack,
                // No target pointed out yet: the subtree fails and the pet goes on with its
                // own behaviour until one is.
                decorator(
                    Cond::KeySet(Key::PetTarget),
                    false,
                    Abort::Both,
                    while_closing_in(
                        leaf(Behavior::MeleeAttack {
                            key: Key::PetTarget,
                            strategy: Strategy::NotFriends,
                        }),
                        leaf(Behavior::MoveToTarget {
                            key: Key::PetTarget,
                            dist: 1,
                            finish_on_arrival: false,
                        }),
                    ),
                ),
            ),
            command(
                PetCommand::PlayDead,
                sequence(vec![
                    leaf(Behavior::Fauna(FaunaLeaf::PlayDead)),
                    leaf(Behavior::Fauna(FaunaLeaf::ClearPetCommand)),
                ]),
            ),
        ]),
    )
}

/// `generic_subtrees/find_partner.bt.json`.
fn find_partner() -> Node {
    decorator(
        Cond::KeySet(Key::WantsToBreed),
        false,
        Abort::Lower,
        decorator(
            Cond::KeySet(Key::BreedReady),
            false,
            Abort::None,
            decorator(
                Cond::KeyOffCooldown(Key::PartnerSearchTimeout),
                false,
                Abort::None,
                leaf(Behavior::Fauna(FaunaLeaf::FindPartner)),
            ),
        ),
    )
}

/// `babies/make_babies.bt.json`.
fn make_babies() -> Node {
    decorator(
        Cond::KeySet(Key::BabiesTarget),
        false,
        Abort::Both,
        sequence(vec![
            leaf(Behavior::MoveToTarget {
                key: Key::BabiesTarget,
                dist: 1,
                finish_on_arrival: true,
            }),
            leaf(Behavior::Fauna(FaunaLeaf::MakeBabies {
                key: Key::BabiesTarget,
            })),
        ]),
    )
}

/// The attack parallel of the combat subtrees: `attack` looping while `mover` closes in.
fn while_closing_in(attack: Node, mover: Node) -> Node {
    parallel(
        vec![subplan(attack, true, true, 0.0), mover],
        ParallelOpts {
            failure_any: true,
            repeat_secondary: true,
            finish_on_primary: true,
            ..ParallelOpts::default()
        },
    )
}

/// `basic_ranged_attack` (`avoid_friendly_fire` with it).
fn ranged_attack(cooldown: f64, avoid_friendly_fire: bool) -> Node {
    leaf(Behavior::Fauna(FaunaLeaf::RangedAttack {
        key: Key::CurrentTarget,
        cooldown,
        max_range: 9,
        avoid_friendly_fire,
    }))
}

/// `simple_ranged_combat.bt.json` / `simple_ranged_retaliate_combat.bt.json` with `targeting`:
/// shoots every 0.6 s keeping 4-6 tiles away. The retaliate one's `BT_ABORT_SELF` next to the
/// looping walk aborts lower priorities too (see the module docs).
fn ranged_combat(targeting: Node) -> Node {
    par(
        vec![
            selector(vec![
                decorator(
                    Cond::KeySet(Key::CurrentTarget),
                    false,
                    Abort::Both,
                    while_closing_in(
                        ranged_attack(0.6, false),
                        leaf(Behavior::Fauna(FaunaLeaf::MaintainDistance {
                            key: Key::CurrentTarget,
                        })),
                    ),
                ),
                random_walk(25.0),
            ]),
            targeting,
        ],
        FIND_TARGET_RATE,
    )
}

/// `hivebot.bt.json` (melee, `None`) and `hivebot_ranged*.bt.json`/`hivebot_mechanic.bt.json`
/// (`Some((time_between_perform, avoid_friendly_fire))`): they close in to one tile either way.
/// Left out: escape captivity, attacking obstructions, relaying to a hive partner.
fn hivebot(ranged: Option<(f64, bool)>) -> Node {
    let attack = match ranged {
        Some((cooldown, avoid)) => ranged_attack(cooldown, avoid),
        None => leaf(Behavior::MeleeAttack {
            key: Key::CurrentTarget,
            strategy: Strategy::Basic,
        }),
    };
    par(
        vec![
            subplan(
                selector(vec![
                    decorator(
                        Cond::KeySet(Key::CurrentTarget),
                        false,
                        Abort::Both,
                        while_closing_in(
                            attack,
                            leaf(Behavior::MoveToTarget {
                                key: Key::CurrentTarget,
                                dist: 1,
                                finish_on_arrival: false,
                            }),
                        ),
                    ),
                    random_walk(25.0),
                ]),
                true,
                true,
                0.0,
            ),
            leaf(Behavior::AcquireCombatTarget {
                key: Key::CurrentTarget,
                strategy: Strategy::Basic,
            }),
        ],
        FIND_TARGET_RATE,
    )
}

/// `turtle/turtle.bt.json`: uses its tree's ability when ready, headbutts legs and sniffs
/// flora once a minute, eats seeds. tg's flora branch checks the headbutt victim key (it never
/// runs without someone to headbutt); here it checks its own target. Left out:
/// `express_happiness` (no happiness component).
fn turtle() -> Node {
    let visit = |target: Key, cooldown: Key, act: FaunaLeaf| {
        decorator(
            Cond::KeySet(target),
            false,
            Abort::Lower,
            sequence(vec![
                leaf(Behavior::MoveToTarget {
                    key: target,
                    dist: 1,
                    finish_on_arrival: true,
                }),
                leaf(Behavior::Fauna(FaunaLeaf::SetCooldown {
                    key: cooldown,
                    secs: 60.0,
                })),
                leaf(Behavior::Fauna(act)),
            ]),
        )
    };
    let acquire = |target: Key, cooldown: Key, flora: bool| {
        decorator(
            Cond::KeyOffCooldown(cooldown),
            false,
            Abort::None,
            leaf(Behavior::Fauna(FaunaLeaf::AcquireTurtleTarget {
                key: target,
                flora,
            })),
        )
    };
    selector(vec![par(
        vec![
            selector(vec![
                decorator(
                    Cond::Fauna(FaunaCond::TurtleAbilityReady),
                    false,
                    Abort::Lower,
                    leaf(Behavior::Fauna(FaunaLeaf::TurtleAbility)),
                ),
                visit(
                    Key::HeadbuttVictim,
                    Key::HeadbuttCooldown,
                    FaunaLeaf::Headbutt {
                        key: Key::HeadbuttVictim,
                    },
                ),
                visit(
                    Key::FloraTarget,
                    Key::FloraCooldown,
                    FaunaLeaf::SniffFlora {
                        key: Key::FloraTarget,
                    },
                ),
                move_to_and_eat(),
                random_walk(25.0),
            ]),
            find_food(),
            acquire(Key::HeadbuttVictim, Key::HeadbuttCooldown, false),
            acquire(Key::FloraTarget, Key::FloraCooldown, true),
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
// Left out: escape captivity, magicarp spells, rifts, attacking obstacles.
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
// Left out: `dog_harassment` (no `TRAIT_HATED_BY_DOGS` here, so it never fires).
fn dog() -> Node {
    selector(vec![par(
        vec![
            subplan(leaf(Behavior::IdleDog), true, false, 0.0),
            speech_loop(&DOG_SPEECH),
        ],
        1.0,
    )])
}

/// `dog/dog_corgi.bt.json`: runs from anyone holding tongs, breeds.
// Left out: harassment.
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
                make_babies(),
                subplan(leaf(Behavior::IdleDog), true, false, 0.0),
            ]),
            leaf(Behavior::AcquireCombatTarget {
                key: Key::CurrentTarget,
                strategy: Strategy::HoldingTongs,
            }),
            find_partner(),
            speech_loop(&DOG_SPEECH),
        ],
        FIND_TARGET_RATE,
    )])
}

/// `pets/cat/cat.bt.json` with `cat_hunt_mice` and breeding.
// Left out: resisting, home, hauling and finding food, trespassers.
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
                            make_babies(),
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
                find_partner(),
            ],
            FIND_TARGET_RATE,
        ),
    ])
}

/// `vermin/mouse.bt.json` with `eat_cable.bt.json`.
// Left out: cheese, instruments.
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
// Left out: escape captivity.
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
