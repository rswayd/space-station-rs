//! Planning and targeting on a bare world (no server): composites, observer aborts, target
//! choice by faction, death and sight, flee locations.

use bevy::prelude::*;
use ss13_shared::TilePos;

use super::behaviors::{Behavior, Strategy};
use super::bt::{Abort, Cond, ParallelOpts, Tree, decorator, leaf, parallel, selector, sequence};
use super::movement::{AiMovement, find_path};
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
    let path = find_path(index, pawn, TilePos::new(2, 5, 1), TilePos::new(8, 5, 1), 0).unwrap();
    assert_eq!(path.last(), Some(&TilePos::new(8, 5, 1)));
    assert!(path.iter().all(|p| p.x != 5 || p.y > 8), "{path:?}");
    // Walled in: no path.
    for x in 1..=20 {
        w.spawn((TilePos::new(x, 9, 1), ss13_world::Density));
    }
    let index = w.resource::<ss13_world::SpatialIndex>();
    assert!(find_path(index, pawn, TilePos::new(2, 5, 1), TilePos::new(8, 5, 1), 0).is_none());
}
