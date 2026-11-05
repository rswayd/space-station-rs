//! Health events of the third wave (`ss13_game::events`) on a bare world: the heart attack's
//! countdown, its defibrillator relief and the heart stopping at the end; the fake virus's
//! three minutes.

use std::time::Duration;

use bevy::prelude::*;
use ss13_game::combat::CombatRng;
use ss13_game::events::fake_virus::{self, FakeVirus};
use ss13_game::events::heart_attack::{
    self, CURE_THRESHOLD, DEFIB_RELIEF, HeartAttack, HeartDesperation, START,
};
use ss13_game::health::{Health, Stat};
use ss13_game::medical::organs::{self, Organ, OrganHost, OrganOf, Slot};

fn world() -> World {
    let mut world = World::new();
    world.insert_resource(Time::<()>::default());
    world.insert_resource(CombatRng(7));
    world
}

/// A conscious human with bare organ entities (no prototypes, no tile: no one sees messages).
fn human(world: &mut World) -> Entity {
    let mob = world
        .spawn((Health::default(), Stat::Conscious, OrganHost))
        .id();
    for slot in Slot::ALL {
        let organ = organs::spawn_organ(world, slot.spec().path).unwrap();
        world.entity_mut(organ).insert(OrganOf(mob));
    }
    mob
}

fn advance(world: &mut World, secs: f64) {
    world
        .resource_mut::<Time>()
        .advance_by(Duration::from_secs_f64(secs));
}

#[test]
fn heart_attack_needs_an_organic_heart_and_is_unique() {
    let mut world = world();
    let mob = human(&mut world);
    assert!(heart_attack::can_heartattack(&world, mob));
    assert!(heart_attack::apply(&mut world, mob));
    assert!(!heart_attack::apply(&mut world, mob), "unique");
    assert_eq!(
        world.get::<HeartAttack>(mob).unwrap().time_until_stoppage,
        START
    );

    let robot = human(&mut world);
    let heart = organs::organ(&world, robot, Slot::Heart).unwrap();
    world.get_mut::<Organ>(heart).unwrap().robotic = true;
    assert!(!heart_attack::can_heartattack(&world, robot));
    assert!(!heart_attack::apply(&mut world, robot));
}

#[test]
fn defib_shocks_cure_a_heart_attack() {
    let mut world = world();
    let mob = human(&mut world);
    assert!(!heart_attack::defib_shock(&mut world, mob, "defibrillator"));
    assert!(heart_attack::apply(&mut world, mob));
    assert!(heart_attack::defib_shock(&mut world, mob, "defibrillator"));
    let left = world.get::<HeartAttack>(mob).unwrap().time_until_stoppage;
    assert_eq!(left, START + DEFIB_RELIEF);
    assert!(left > CURE_THRESHOLD);
    // The next tick (1 s later) sees it past the cure threshold.
    advance(&mut world, 1.0);
    heart_attack::process_heart_attacks(&mut world);
    assert!(world.get::<HeartAttack>(mob).is_none(), "cured");
    assert!(!organs::cardiac_arrest(&world, mob));
}

#[test]
fn the_countdown_ends_in_cardiac_arrest() {
    let mut world = world();
    let mob = human(&mut world);
    assert!(heart_attack::apply(&mut world, mob));
    world
        .get_mut::<HeartAttack>(mob)
        .unwrap()
        .time_until_stoppage = 2.0;
    for _ in 0..3 {
        advance(&mut world, 1.0);
        heart_attack::process_heart_attacks(&mut world);
    }
    assert!(world.get::<HeartAttack>(mob).is_none());
    assert!(organs::cardiac_arrest(&world, mob), "the heart stopped");
    assert!(world.get::<HeartDesperation>(mob).is_some());
    // The desperation window passes after 4 s.
    for _ in 0..5 {
        advance(&mut world, 1.0);
        heart_attack::process_heart_attacks(&mut world);
    }
    assert!(world.get::<HeartDesperation>(mob).is_none());
    assert!(
        world.get::<Health>(mob).unwrap().oxy > 0.0,
        "stage four chokes"
    );
}

#[test]
fn fake_virus_lasts_three_minutes_and_skips_the_unconscious() {
    let mut world = world();
    let mob = human(&mut world);
    assert!(fake_virus::apply(&mut world, mob));
    assert!(world.get::<FakeVirus>(mob).is_some());
    for _ in 0..179 {
        advance(&mut world, 1.0);
        fake_virus::process_fake_viruses(&mut world);
    }
    let virus = world.get::<FakeVirus>(mob).unwrap();
    assert!(virus.stage > 800, "five ticks a second: {}", virus.stage);
    advance(&mut world, 2.0);
    fake_virus::process_fake_viruses(&mut world);
    assert!(world.get::<FakeVirus>(mob).is_none(), "over");
    assert_eq!(
        world.get::<Health>(mob).unwrap().oxy,
        0.0,
        "no harm (only lines and emotes)"
    );

    let sleeper = human(&mut world);
    world.entity_mut(sleeper).insert(Stat::HardCrit);
    assert!(!fake_virus::apply(&mut world, sleeper));
}
