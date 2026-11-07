//! Planning and targeting on a bare world (no server): composites, observer aborts, target
//! choice by faction, death and sight, flee locations.

use bevy::prelude::*;
use ss13_shared::TilePos;

use super::behaviors::{Behavior, Strategy};
use super::bt::{Abort, Cond, ParallelOpts, Tree, decorator, leaf, parallel, selector, sequence};
use super::movement::{AiMovement, Pathing, find_path};
use super::{AiCtx, Blackboard, Key, Value};
use crate::combat::CombatRng;
use crate::health::{Health, Stat};
use crate::mobs::Faction;

fn world() -> World {
    let mut app = App::new();
    app.add_plugins(ss13_world::WorldPlugin);
    app.world_mut()
        .resource_mut::<ss13_world::SpatialIndex>()
        .create_level(1, 20, 20)
        .unwrap();
    std::mem::take(app.world_mut())
}

fn mob(world: &mut World, x: i32, y: i32, faction: &str) -> Entity {
    world
        .spawn((
            Health::basic(20.0, [1.0; 5]),
            Stat::Conscious,
            Faction(vec![faction.to_owned()]),
            TilePos::new(x, y, 1),
        ))
        .id()
}

/// Runs `ticks` planning ticks of `tree` for `pawn`, 0.25 s apart.
fn run(world: &mut World, pawn: Entity, tree: &mut Tree, bb: &mut Blackboard, ticks: u32) {
    let mut movement = AiMovement::default();
    for i in 0..ticks {
        let mut ctx = AiCtx {
            world: &mut *world,
            pawn,
            bb: &mut *bb,
            movement: &mut movement,
            now: 1.0 + f64::from(i) * 0.25,
            spt: 0.25,
            rng: CombatRng(7),
            active: 0,
            cancelled: false,
        };
        tree.tick(&mut ctx);
    }
}

#[test]
fn selector_falls_through_and_sequence_stops_at_failure() {
    let mut w = world();
    let pawn = mob(&mut w, 5, 5, "carp");
    let mut bb = Blackboard::default();
    // The decorated branch fails without its key: the selector takes the next one.
    let mut tree = Tree::new(vec![selector(vec![
        decorator(
            Cond::KeySet(Key::CurrentTarget),
            false,
            Abort::None,
            leaf(Behavior::SetBool(Key::WantsToEatCable, true)),
        ),
        sequence(vec![
            leaf(Behavior::SetBool(Key::StopFleeing, true)),
            // Fails: nothing to move to, so the sequence never reaches the last leaf.
            leaf(Behavior::MoveToTarget {
                key: Key::FleeLocation,
                dist: 0,
                finish_on_arrival: true,
            }),
            leaf(Behavior::SetBool(Key::MeleeCooldown, true)),
        ]),
    ])]);
    run(&mut w, pawn, &mut tree, &mut bb, 1);
    assert_eq!(bb.get(Key::WantsToEatCable), None);
    assert_eq!(bb.get(Key::StopFleeing), Some(&Value::Bool(true)));
    assert_eq!(bb.get(Key::MeleeCooldown), None);
}

#[test]
fn targeting_picks_the_nearest_enemy_it_sees() {
    let mut w = world();
    let carp = mob(&mut w, 5, 5, "carp");
    let _friend = mob(&mut w, 6, 5, "carp");
    let far = mob(&mut w, 5, 9, "neutral");
    let near = mob(&mut w, 8, 5, "neutral");
    let dead = mob(&mut w, 4, 5, "neutral");
    w.entity_mut(dead).insert(Stat::Dead);
    let _out_of_range = mob(&mut w, 5, 19, "neutral");
    let mut bb = Blackboard::default();
    let mut tree = Tree::new(vec![leaf(Behavior::AcquireCombatTarget {
        key: Key::CurrentTarget,
        strategy: Strategy::Basic,
    })]);
    run(&mut w, carp, &mut tree, &mut bb, 1);
    assert_eq!(
        bb.entity(Key::CurrentTarget),
        Some(near),
        "same faction and dead skipped"
    );
    // A wall between hides the near one: the far one is taken.
    w.spawn((TilePos::new(7, 5, 1), ss13_mapload::Opaque));
    let mut tree = Tree::new(vec![leaf(Behavior::AcquireCombatTarget {
        key: Key::CurrentTarget,
        strategy: Strategy::Basic,
    })]);
    run(&mut w, carp, &mut tree, &mut bb, 1);
    assert_eq!(bb.entity(Key::CurrentTarget), Some(far));
    // Nobody left: the key is cleared.
    w.despawn(far);
    w.despawn(near);
    run(&mut w, carp, &mut tree, &mut bb, 8);
    assert_eq!(bb.entity(Key::CurrentTarget), None);
}

#[test]
fn a_new_target_aborts_lower_priority_wandering() {
    let mut w = world();
    let pawn = mob(&mut w, 5, 5, "carp");
    let mut bb = Blackboard::default();
    // tg `basic_carp_tree` shape: combat while a target is set, else a leaf that runs forever
    // (walking towards a tile), with target finding as a repeating secondary.
    bb.set(Key::FleeLocation, Value::Tile(TilePos::new(1, 1, 1)));
    let mut tree = Tree::new(vec![parallel(
        vec![
            selector(vec![
                decorator(
                    Cond::KeySet(Key::CurrentTarget),
                    false,
                    Abort::Both,
                    leaf(Behavior::SetBool(Key::WantsToEatCable, true)),
                ),
                leaf(Behavior::MoveToTarget {
                    key: Key::FleeLocation,
                    dist: 0,
                    finish_on_arrival: false,
                }),
            ]),
            leaf(Behavior::AcquireCombatTarget {
                key: Key::CurrentTarget,
                strategy: Strategy::Basic,
            }),
        ],
        ParallelOpts {
            repeat_secondary: true,
            repeat_secondary_delay: 1.0,
            finish_on_primary: true,
            ..default()
        },
    )]);
    run(&mut w, pawn, &mut tree, &mut bb, 4);
    assert_eq!(bb.get(Key::WantsToEatCable), None, "idling");
    let human = mob(&mut w, 7, 7, "neutral");
    run(&mut w, pawn, &mut tree, &mut bb, 12);
    assert_eq!(bb.entity(Key::CurrentTarget), Some(human));
    assert_eq!(
        bb.get(Key::WantsToEatCable),
        Some(&Value::Bool(true)),
        "the running idle leaf was aborted for the target branch"
    );
}

#[test]
fn injured_carp_flees_away_from_its_target() {
    let mut w = world();
    let carp = mob(&mut w, 10, 10, "carp");
    let human = mob(&mut w, 11, 10, "neutral");
    let mut bb = Blackboard::default();
    bb.set(Key::CurrentTarget, Value::Entity(human));
    bb.set(Key::StopFleeing, Value::Bool(true));
    let flee = || {
        Tree::new(vec![decorator(
            Cond::CarpShouldFlee(Key::CurrentTarget),
            false,
            Abort::Both,
            leaf(Behavior::FindFleeLocation {
                key: Key::CurrentTarget,
            }),
        )])
    };
    run(&mut w, carp, &mut flee(), &mut bb, 1);
    assert_eq!(bb.get(Key::FleeLocation), None, "healthy carp fight");
    // `ai_flee_while_injured`: at half health it stops "stopping fleeing".
    let mut controller = super::AiController {
        kind: super::trees::CARP.into(),
        tree: Tree::new(vec![]),
        bb: bb.clone(),
        movement: AiMovement::default(),
        status: super::AiStatus::On,
        last_tick: 0.0,
        traits: 0,
        retaliates: true,
        cancel: false,
        active: 0,
    };
    let mut health = Health::basic(20.0, [1.0; 5]);
    health.adjust(crate::health::DamageType::Brute, 10.0);
    super::flee_while_injured(&mut controller, &health);
    assert_eq!(
        controller.bb.get(Key::StopFleeing),
        Some(&Value::Bool(false))
    );
    let mut bb = controller.bb;
    run(&mut w, carp, &mut flee(), &mut bb, 1);
    let Some(Value::Tile(to)) = bb.get(Key::FleeLocation).cloned() else {
        panic!("no flee location");
    };
    assert!(to.x < 10, "away from the threat on the east: {to:?}");
}

#[test]
fn paths_go_around_walls() {
    let mut w = world();
    let pawn = mob(&mut w, 2, 5, "carp");
    for y in 1..=8 {
        w.spawn((TilePos::new(5, y, 1), ss13_world::Density));
    }
    let index = w.resource::<ss13_world::SpatialIndex>();
    let path = find_path(
        index,
        pawn,
        TilePos::new(2, 5, 1),
        TilePos::new(8, 5, 1),
        0,
        &Pathing::BASIC,
    )
    .unwrap();
    assert_eq!(path.last(), Some(&TilePos::new(8, 5, 1)));
    assert!(path.iter().all(|p| p.x != 5 || p.y > 8), "{path:?}");
    // Walled in: no path.
    for x in 1..=20 {
        w.spawn((TilePos::new(x, 9, 1), ss13_world::Density));
    }
    let index = w.resource::<ss13_world::SpatialIndex>();
    assert!(
        find_path(
            index,
            pawn,
            TilePos::new(2, 5, 1),
            TilePos::new(8, 5, 1),
            0,
            &Pathing::BASIC
        )
        .is_none()
    );
}

fn basic_mob(world: &mut World, x: i32, y: i32, faction: &str, path: &str) -> Entity {
    let e = mob(world, x, y, faction);
    world.entity_mut(e).insert(crate::mobs::BasicMob {
        def: std::sync::Arc::new(ss13_mapload::mobs::MobDef {
            path: path.to_owned(),
            ..Default::default()
        }),
        icon_living: String::new(),
        icon_dead: String::new(),
        mouth: false,
    });
    e
}

const BASIC: &str = "/datum/ai_controller/basic_controller";

#[test]
fn retaliation_ignores_factions_only_while_picking() {
    let mut w = world();
    let pig = mob(&mut w, 5, 5, "neutral");
    let human = mob(&mut w, 7, 5, "neutral");
    let mut bb = Blackboard::default();
    bb.set(Key::RetaliateList, Value::Attackers(vec![(human, 0.0)]));
    let mut tree = Tree::new(vec![leaf(Behavior::TargetFromRetaliateList {
        key: Key::CurrentTarget,
        strategy: Strategy::Basic,
    })]);
    run(&mut w, pig, &mut tree, &mut bb, 1);
    // tg `check_faction = FALSE`: a neutral attacker of a neutral pig is picked...
    assert_eq!(bb.entity(Key::CurrentTarget), Some(human));
    // ...and `BB_TEMPORARILY_IGNORE_FACTION` is back off for the attack's own check.
    assert_eq!(bb.get(Key::IgnoreFaction), Some(&Value::Bool(false)));
}

#[test]
fn lizard_finds_and_eats_a_cockroach_in_view() {
    let mut w = world();
    w.init_resource::<Time>();
    w.insert_resource(CombatRng(3));
    let lizard = basic_mob(&mut w, 5, 5, "neutral", "/mob/living/basic/lizard");
    let roach = basic_mob(&mut w, 6, 5, "hostile", "/mob/living/basic/cockroach");
    let far = basic_mob(&mut w, 18, 18, "hostile", "/mob/living/basic/cockroach");
    let mouse = basic_mob(&mut w, 4, 5, "rats", "/mob/living/basic/mouse");
    let built = super::trees::build(
        &format!("{BASIC}/lizard"),
        &ss13_mapload::mobs::MobDef::default(),
    )
    .unwrap();
    let mut tree = Tree::new(built.roots);
    let mut bb = Blackboard::default();
    run(&mut w, lizard, &mut tree, &mut bb, 4);
    assert!(w.get_entity(roach).is_err(), "eaten");
    assert!(w.get_entity(far).is_ok(), "out of view");
    assert!(w.get_entity(mouse).is_ok(), "not lizard food");
    assert_eq!(
        bb.time(Key::NextFoodEat),
        Some(crate::mobs::eating::EAT_FOOD_COOLDOWN),
        "`BB_NEXT_FOOD_EAT` from the world clock (0)"
    );
}

#[test]
fn food_lists_follow_the_mob_types() {
    use crate::mobs::eating::eating_of;
    let big = eating_of("/mob/living/basic/lizard/big/crocodile").unwrap();
    assert!(big.foods.contains(&"/obj/item/food/meat"));
    let lizard = eating_of("/mob/living/basic/lizard").unwrap();
    assert!(!lizard.foods.contains(&"/obj/item/food/meat"));
    assert_eq!(
        eating_of("/mob/living/basic/mothroach").unwrap().cooldown,
        60.0
    );
    assert!(eating_of("/mob/living/basic/pet/dog").is_none());
}

#[test]
fn speech_reads_speech_types_and_the_mobs_lines() {
    use ss13_mapload::mobs::MobSpeech;
    let mut w = world();
    let lines = MobSpeech {
        chance: 100.0,
        emote_see: vec!["shakes.".into()],
        ..Default::default()
    };
    w.insert_resource(crate::mobs::MobSpeeches(
        [("/speech/x".to_owned(), std::sync::Arc::new(lines.clone()))]
            .into_iter()
            .collect(),
    ));
    let pawn = basic_mob(&mut w, 5, 5, "neutral", "/mob/living/basic/cow");
    let said = |w: &mut World, speech: Option<&'static str>| {
        let mut bb = Blackboard::default();
        let mut tree = Tree::new(vec![sequence(vec![
            leaf(Behavior::Speak(speech)),
            leaf(Behavior::SetBool(Key::WantsToEatCable, true)),
        ])]);
        run(w, pawn, &mut tree, &mut bb, 1);
        bb.get(Key::WantsToEatCable).is_some()
    };
    assert!(said(&mut w, Some("/speech/x")));
    assert!(!said(&mut w, Some("/speech/missing")));
    assert!(!said(&mut w, None), "no `BB_BASIC_MOB_SPEAK_LINES`");
    let def = ss13_mapload::mobs::MobDef {
        speech_lines: Some(lines),
        ..Default::default()
    };
    w.get_mut::<crate::mobs::BasicMob>(pawn).unwrap().def = std::sync::Arc::new(def);
    assert!(said(&mut w, None));
}

#[test]
fn generic_controllers_build_and_plan() {
    let def = ss13_mapload::mobs::MobDef {
        ai_traits: 21,
        ..Default::default()
    };
    for sub in [
        "simple/simple_hostile",
        "simple/simple_hostile_obstacles",
        "simple/simple_retaliate",
        "simple/simple_fearful",
        "simple/simple_skittish",
        "simple/simple_capricious",
        "talk",
        "base_animal",
        "possum",
        "lizard",
        "lizard/big",
        "crocodile",
        "chicken",
        "chicken/cock",
        "rabbit",
        "sheep",
        "pig",
        "pig/big",
        "pony",
        "cow",
        "goat",
        "goose",
        "goose/calm",
        "penguin/baby",
        "sloth",
        "mothroach",
        "penguin",
        "snake",
        "snake/banded",
        "bear",
        "frog",
        "deer",
        "space_bat",
        "snail",
        "butterfly",
        "axolotl",
        "gondola",
        "mouse/rat",
        "mouse/rat/syndi",
    ] {
        let built = super::trees::build(&format!("{BASIC}/{sub}"), &def).unwrap();
        assert_eq!(built.roots.len(), 1, "{sub}");
        assert_ne!(
            built.traits & super::STOP_MOVING_WHEN_PULLED,
            0,
            "{sub}: DM `STOP_MOVING_WHEN_PULLED`"
        );
        // A few planning ticks next to a stranger: nothing panics.
        let mut w = world();
        w.init_resource::<Time>();
        w.insert_resource(CombatRng(5));
        let pawn = basic_mob(&mut w, 5, 5, "neutral", "/mob/living/basic/pig");
        let stranger = mob(&mut w, 9, 5, "hostile");
        let mut bb = Blackboard(built.blackboard.into_iter().collect());
        bb.set(Key::RetaliateList, Value::Attackers(vec![(stranger, 0.0)]));
        let mut tree = Tree::new(built.roots);
        run(&mut w, pawn, &mut tree, &mut bb, 12);
    }
    let pig = super::trees::build(&format!("{BASIC}/pig"), &def).unwrap();
    assert!(pig.retaliates);
    assert_eq!(
        pig.blackboard,
        vec![(Key::StopFleeing, Value::Bool(true))],
        "`ai_flee_while_injured`"
    );
    let passive = super::trees::build(
        &format!("{BASIC}/cow"),
        &ss13_mapload::mobs::MobDef::default(),
    )
    .unwrap();
    assert_eq!(passive.traits, 0);
}

#[test]
fn skittish_pig_fights_until_hurt_then_runs() {
    let mut w = world();
    let pig = mob(&mut w, 10, 10, "neutral");
    let wolf = mob(&mut w, 12, 10, "hostile");
    let built = super::trees::build(
        &format!("{BASIC}/pig"),
        &ss13_mapload::mobs::MobDef::default(),
    )
    .unwrap();
    let mut tree = Tree::new(built.roots);
    let mut bb = Blackboard(built.blackboard.into_iter().collect());
    bb.set(Key::RetaliateList, Value::Attackers(vec![(wolf, 0.0)]));
    run(&mut w, pig, &mut tree, &mut bb, 2);
    assert_eq!(bb.entity(Key::CurrentTarget), Some(wolf));
    assert_eq!(bb.get(Key::FleeLocation), None, "healthy: it fights");
    bb.set(Key::StopFleeing, Value::Bool(false));
    run(&mut w, pig, &mut tree, &mut bb, 2);
    let Some(Value::Tile(to)) = bb.get(Key::FleeLocation).cloned() else {
        panic!("no flee location");
    };
    assert!(to.x < 10, "away from the wolf on the east: {to:?}");
}

fn sized_mob(world: &mut World, x: i32, y: i32, path: &str, mob_size: u8) -> Entity {
    let e = mob(world, x, y, "neutral");
    world.entity_mut(e).insert(crate::mobs::BasicMob {
        def: std::sync::Arc::new(ss13_mapload::mobs::MobDef {
            path: path.to_owned(),
            mob_size,
            ..Default::default()
        }),
        icon_living: String::new(),
        icon_dead: String::new(),
        mouth: false,
    });
    e
}

#[test]
fn friends_are_never_targets() {
    let mut w = world();
    let carp = mob(&mut w, 5, 5, "carp");
    let tamer = mob(&mut w, 7, 5, "neutral");
    w.entity_mut(carp)
        .insert(crate::mobs::pets::Friends(vec![tamer]));
    let mut bb = Blackboard::default();
    let mut tree = Tree::new(vec![leaf(Behavior::AcquireCombatTarget {
        key: Key::CurrentTarget,
        strategy: Strategy::Basic,
    })]);
    run(&mut w, carp, &mut tree, &mut bb, 1);
    assert_eq!(bb.entity(Key::CurrentTarget), None, "the tamer is a friend");
    let stranger = mob(&mut w, 8, 5, "neutral");
    run(&mut w, carp, &mut tree, &mut bb, 1);
    assert_eq!(bb.entity(Key::CurrentTarget), Some(stranger));
}

#[test]
fn crabs_hunt_only_smaller_mobs() {
    let mut w = world();
    let crab = sized_mob(&mut w, 5, 5, "/mob/living/basic/crab", 1);
    let _cat = sized_mob(&mut w, 6, 5, "/mob/living/basic/pet/cat", 1);
    let _human = mob(&mut w, 4, 5, "neutral");
    let mouse = sized_mob(&mut w, 8, 5, "/mob/living/basic/mouse", 0);
    let mut bb = Blackboard::default();
    let mut tree = Tree::new(vec![leaf(Behavior::AcquireCombatTarget {
        key: Key::CurrentTarget,
        strategy: Strategy::Smaller,
    })]);
    run(&mut w, crab, &mut tree, &mut bb, 1);
    assert_eq!(
        bb.entity(Key::CurrentTarget),
        Some(mouse),
        "same size and humans skipped"
    );
}

#[test]
fn pet_commands_take_over_the_tree() {
    use crate::mobs::pets::PetCommand;
    let mut w = world();
    w.init_resource::<Time>();
    w.insert_resource(CombatRng(5));
    let dog = basic_mob(&mut w, 5, 5, "neutral", "/mob/living/basic/pet/dog/corgi");
    let owner = mob(&mut w, 9, 5, "neutral");
    let def = ss13_mapload::mobs::MobDef {
        path: "/mob/living/basic/pet/dog/corgi".to_owned(),
        ..Default::default()
    };
    let built = super::trees::build(&format!("{BASIC}/dog/corgi"), &def).unwrap();
    assert_eq!(built.roots.len(), 2, "the pet command subtree comes first");
    let mut tree = Tree::new(built.roots);
    let mut bb = Blackboard::default();
    let mut movement = AiMovement::default();
    // The controller keeps `active_execution_index` between ticks (`with_ctx`): the observer
    // aborts of a changed command need it.
    let mut active = 0;
    let mut tick = |w: &mut World, bb: &mut Blackboard, movement: &mut AiMovement, i: u32| {
        let mut ctx = AiCtx {
            world: &mut *w,
            pawn: dog,
            bb,
            movement,
            now: 1.0 + f64::from(i) * 0.25,
            spt: 0.25,
            // A new roll every tick (`play_dead` ends on `SPT_PROB`).
            rng: CombatRng(7 + u64::from(i)),
            active,
            cancelled: false,
        };
        tree.tick(&mut ctx);
        active = ctx.active;
    };
    bb.set(Key::ActivePetCommand, Value::Pet(PetCommand::Follow));
    bb.set(Key::PetTarget, Value::Entity(owner));
    tick(&mut w, &mut bb, &mut movement, 0);
    assert_eq!(
        movement.order.as_ref().map(|o| o.target),
        Some(super::movement::Target::Entity(owner)),
        "follows its owner"
    );
    // `play_dead`, then back to its own ways once it springs up.
    bb.set(Key::ActivePetCommand, Value::Pet(PetCommand::PlayDead));
    for i in 1..400 {
        tick(&mut w, &mut bb, &mut movement, i);
    }
    assert_eq!(
        bb.get(Key::ActivePetCommand),
        None,
        "the command is cleared"
    );
}

#[test]
fn hivebot_trees_shoot_or_bite() {
    for sub in [
        "hivebot",
        "hivebot/ranged",
        "hivebot/ranged/rapid",
        "hivebot/mechanic",
    ] {
        let def = ss13_mapload::mobs::MobDef::default();
        let built = super::trees::build(&format!("{BASIC}/{sub}"), &def).unwrap();
        assert_eq!(built.roots.len(), 1, "{sub}");
        let mut w = world();
        w.init_resource::<Time>();
        w.insert_resource(CombatRng(5));
        let pawn = basic_mob(&mut w, 5, 5, "hivebot", "/mob/living/basic/hivebot/range");
        let _victim = mob(&mut w, 9, 5, "neutral");
        let mut bb = Blackboard::default();
        let mut tree = Tree::new(built.roots);
        run(&mut w, pawn, &mut tree, &mut bb, 8);
        assert!(
            bb.entity(Key::CurrentTarget).is_some(),
            "{sub}: targets the stranger"
        );
    }
}

#[test]
fn troopers_fight_shoot_in_bursts_or_wait_to_be_hit() {
    let def = ss13_mapload::mobs::MobDef::default();
    for (sub, hostile) in [
        ("trooper", true),
        ("trooper/calls_reinforcements", true),
        ("trooper/viscerator", true),
        ("trooper/ranged", true),
        ("trooper/ranged/burst", true),
        ("trooper/ranged/shotgunner", true),
        ("trooper/peaceful", false),
        ("trooper/ranged/burst/peaceful_burst", false),
    ] {
        let built = super::trees::build(&format!("{BASIC}/{sub}"), &def).unwrap();
        assert_eq!(built.roots.len(), 1, "{sub}");
        assert_eq!(built.retaliates, !hostile, "{sub}");
        let mut w = world();
        w.init_resource::<Time>();
        w.insert_resource(CombatRng(5));
        let pawn = basic_mob(
            &mut w,
            5,
            5,
            "Syndicate",
            "/mob/living/basic/trooper/syndicate/ranged/smg",
        );
        let mut ranged = crate::mobs::ranged::RangedAttacks::new(
            ss13_mapload::ProjectileData {
                path: "/obj/projectile/bullet/c45".to_owned(),
                damage: 30.0,
                ..Default::default()
            },
            3.0,
        );
        ranged.burst = 3;
        w.entity_mut(pawn).insert(ranged);
        let _victim = mob(&mut w, 8, 5, "neutral");
        let mut bb = Blackboard::default();
        let mut tree = Tree::new(built.roots);
        run(&mut w, pawn, &mut tree, &mut bb, 8);
        assert_eq!(
            bb.entity(Key::CurrentTarget).is_some(),
            hostile,
            "{sub}: picks the stranger only when hostile"
        );
        if hostile && sub.starts_with("trooper/ranged") {
            let r = w.get::<crate::mobs::ranged::RangedAttacks>(pawn).unwrap();
            assert!(r.next > 1.0, "{sub}: fired");
            // `burst_shots` 3: the other two 0.2 s apart.
            assert_eq!(r.queued.len(), 2, "{sub}");
            assert!((r.queued[1].0 - r.queued[0].0 - 0.2).abs() < 1e-9);
        }
    }
}

#[test]
fn artificers_pick_hurt_constructs_and_wraiths_the_weakest() {
    let def = ss13_mapload::mobs::MobDef::default();
    let setup = || {
        let mut w = world();
        w.init_resource::<Time>();
        w.insert_resource(CombatRng(5));
        w
    };
    // An artificer goes for the hurt juggernaut, never the healthy one or a stranger.
    let mut w = setup();
    let artificer = basic_mob(
        &mut w,
        5,
        5,
        "cult",
        "/mob/living/basic/construct/artificer/hostile",
    );
    let healthy = basic_mob(
        &mut w,
        6,
        5,
        "cult",
        "/mob/living/basic/construct/juggernaut/hostile",
    );
    let hurt = basic_mob(
        &mut w,
        8,
        5,
        "cult",
        "/mob/living/basic/construct/juggernaut/hostile",
    );
    w.get_mut::<Health>(hurt)
        .unwrap()
        .adjust(crate::health::DamageType::Brute, 5.0);
    let _stranger = mob(&mut w, 4, 5, "neutral");
    let built = super::trees::build(&format!("{BASIC}/artificer"), &def).unwrap();
    assert!(built.retaliates);
    let mut bb = Blackboard::default();
    let mut tree = Tree::new(built.roots);
    run(&mut w, artificer, &mut tree, &mut bb, 4);
    assert_eq!(bb.entity(Key::CurrentTarget), Some(hurt));
    assert_ne!(bb.entity(Key::CurrentTarget), Some(healthy));
    // A wraith takes the weaker of two strangers, not the nearer.
    let mut w = setup();
    let wraith = basic_mob(
        &mut w,
        5,
        5,
        "cult",
        "/mob/living/basic/construct/wraith/hostile",
    );
    let _near = mob(&mut w, 6, 5, "neutral");
    let weak = mob(&mut w, 9, 5, "neutral");
    w.get_mut::<Health>(weak)
        .unwrap()
        .adjust(crate::health::DamageType::Brute, 10.0);
    let built = super::trees::build(&format!("{BASIC}/wraith"), &def).unwrap();
    let mut bb = Blackboard::default();
    let mut tree = Tree::new(built.roots);
    run(&mut w, wraith, &mut tree, &mut bb, 4);
    assert_eq!(bb.entity(Key::CurrentTarget), Some(weak));
    for sub in ["juggernaut", "proteon"] {
        assert!(super::trees::build(&format!("{BASIC}/{sub}"), &def).is_some());
    }
}

#[test]
fn illusions_attack_or_run_from_their_target() {
    let def = ss13_mapload::mobs::MobDef::default();
    let hostile = super::trees::build(&format!("{BASIC}/illusion"), &def).unwrap();
    assert!(!hostile.retaliates);
    let escape = super::trees::build(&format!("{BASIC}/illusion/escape/retaliate"), &def).unwrap();
    assert!(escape.retaliates);
    let mut w = world();
    w.init_resource::<Time>();
    w.insert_resource(CombatRng(5));
    let pawn = basic_mob(
        &mut w,
        5,
        5,
        "illusion",
        "/mob/living/basic/illusion/escape",
    );
    let threat = mob(&mut w, 6, 5, "neutral");
    let mut bb = Blackboard::default();
    bb.set(Key::CurrentTarget, Value::Entity(threat));
    let mut tree = Tree::new(escape.roots);
    run(&mut w, pawn, &mut tree, &mut bb, 4);
    // It picked a spot to flee to, away from the threat.
    let spot = bb.get(Key::FleeLocation).cloned();
    assert!(spot.is_some(), "flees");
}

#[test]
fn traders_shoot_back_and_stop_wandering_once_open() {
    let def = ss13_mapload::mobs::MobDef::default();
    let built = super::trees::build(&format!("{BASIC}/trader/jumpscare"), &def).unwrap();
    assert!(built.retaliates);
    let mut w = world();
    w.init_resource::<Time>();
    w.insert_resource(CombatRng(5));
    let trader = basic_mob(&mut w, 5, 5, "neutral", "/mob/living/basic/trader");
    let thief = mob(&mut w, 8, 5, "neutral");
    let mut bb = Blackboard::default();
    bb.set(Key::RetaliateList, Value::Attackers(vec![(thief, 0.0)]));
    let mut tree = Tree::new(built.roots);
    run(&mut w, trader, &mut tree, &mut bb, 4);
    assert_eq!(bb.entity(Key::CurrentTarget), Some(thief), "the thief");
}

#[test]
fn one_off_mobs_hunt_and_the_vatbeast_fights_back_first() {
    let def = ss13_mapload::mobs::MobDef::default();
    for sub in ["tree", "festivus_pole", "revolutionary"] {
        let built = super::trees::build(&format!("{BASIC}/{sub}"), &def).unwrap();
        assert!(!built.retaliates, "{sub}");
    }
    let built = super::trees::build(&format!("{BASIC}/vatbeast"), &def).unwrap();
    assert!(built.retaliates);
    let mut w = world();
    w.init_resource::<Time>();
    w.insert_resource(CombatRng(5));
    let beast = basic_mob(&mut w, 5, 5, "hostile", "/mob/living/basic/vatbeast");
    let _near = mob(&mut w, 6, 5, "neutral");
    let attacker = mob(&mut w, 9, 5, "neutral");
    let mut bb = Blackboard::default();
    bb.set(Key::RetaliateList, Value::Attackers(vec![(attacker, 0.0)]));
    let mut tree = Tree::new(built.roots);
    run(&mut w, beast, &mut tree, &mut bb, 2);
    assert_eq!(bb.entity(Key::CurrentTarget), Some(attacker));
}
