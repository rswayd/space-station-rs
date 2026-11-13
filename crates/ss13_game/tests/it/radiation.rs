//! Radiation (`ss13_game::radiation`) on the power fixture room (`super::power`) without clients:
//! a sure pulse irradiates a bare human in the open (10 toxin, the alert, the Geiger counter in
//! his hand reads the extreme level), not one behind a full insulator, not one in a radiation
//! suit and hood; a pulse with a minimum exposure time only warms a human up, a pulse 3 s later
//! irradiates him; healing the toxin damage ends the irradiation. A uranium statue (the
//! `radioactive` element) pulses every 3 s. A scanning counter clicks while pulses reach it.

use bevy::prelude::*;
use ss13_game::breath::{AlertCategory, Alerts};
use ss13_game::health::{DamageType, Health, Stat};
use ss13_game::human::HumanLook;
use ss13_game::inventory::{CarriedBy, Inventory};
use ss13_game::radiation::{self, DANGER_EXTREME, GEIGER, Geiger, Irradiated, RadiationCountdown};
use ss13_game::testing::{self, fixture_map, steps};
use ss13_mapload::{DmClothing, DmItem, DmRadInsulation};
use ss13_shared::TilePos;

fn server(test: &str) -> App {
    let assets = super::power::fixture_assets(&format!("radiation_{test}"));
    let mut app = testing::server_app(fixture_map(&assets, &["map.dmm"]), |_| {});
    app.update();
    app
}

fn at(x: i32) -> TilePos {
    TilePos::new(x, 2, 1)
}

fn item(world: &mut World, path: &str, covered: u32) -> Entity {
    world
        .spawn((
            DmItem {
                path: path.into(),
                ..default()
            },
            DmClothing {
                body_parts_covered: covered,
                ..default()
            },
        ))
        .id()
}

fn human(world: &mut World, pos: TilePos) -> Entity {
    world
        .spawn((
            pos,
            HumanLook::random_assistant(1),
            Health::default(),
            Stat::Conscious,
            Inventory::default(),
            Alerts::default(),
        ))
        .id()
}

fn run(app: &mut App, secs: f64) {
    for _ in 0..steps(secs) {
        app.update();
    }
}

#[test]
fn pulse_irradiates_the_unprotected_in_line() {
    let mut app = server("pulse");
    let world = app.world_mut();
    let source = world.spawn(at(3)).id();
    let bare = human(world, at(5));
    let counter = item(world, GEIGER, 0);
    world.entity_mut(counter).insert(CarriedBy(bare));
    world.get_mut::<Inventory>(bare).unwrap().hands[0] = Some(counter);
    // A full insulator on (6,2) shields (7,2).
    world.spawn((at(6), DmRadInsulation { value: 0.0 }));
    let shielded = human(world, at(7));
    // The radiation suit and hood cover every bodypart.
    let suited = human(world, at(4));
    let suit = item(
        world,
        "/obj/item/clothing/suit/utility/radiation",
        1 | 2 | 24 | 96 | 384 | 1536,
    );
    let hood = item(world, "/obj/item/clothing/head/utility/radiation", 4);
    let mut inventory = world.get_mut::<Inventory>(suited).unwrap();
    inventory.suit = Some(suit);
    inventory.head = Some(hood);
    assert!(radiation::wearing_rad_protected_clothing(world, suited));
    assert!(radiation::radiation_pulse(
        world, source, 4.0, 0.5, 100.0, 0.0
    ));
    run(&mut app, 0.6);

    let world = app.world();
    assert!(world.get::<Irradiated>(bare).is_some(), "in the open");
    assert_eq!(world.get::<Health>(bare).unwrap().tox, 10.0);
    assert!(
        world
            .get::<Alerts>(bare)
            .unwrap()
            .get(AlertCategory::Irradiated)
            .is_some()
    );
    assert_eq!(
        world.get::<Geiger>(counter).and_then(|g| g.danger),
        Some(DANGER_EXTREME)
    );
    assert!(
        world.get::<Irradiated>(shielded).is_none(),
        "behind the insulator"
    );
    assert!(
        world.get::<Irradiated>(suited).is_none(),
        "in the radiation suit"
    );

    // Healed toxin damage ends it on the next process (2 s).
    app.world_mut()
        .get_mut::<Health>(bare)
        .unwrap()
        .adjust(DamageType::Tox, -100.0);
    run(&mut app, 2.5);
    let world = app.world();
    assert!(world.get::<Irradiated>(bare).is_none());
    assert!(
        world
            .get::<Alerts>(bare)
            .unwrap()
            .get(AlertCategory::Irradiated)
            .is_none()
    );
}

#[test]
fn minimum_exposure_time_warms_up_first() {
    let mut app = server("exposure");
    let world = app.world_mut();
    let source = world.spawn(at(8)).id();
    let mob = human(world, at(9));
    radiation::radiation_pulse(world, source, 3.0, 0.5, 100.0, 3.0);
    run(&mut app, 0.6);
    assert!(app.world().get::<RadiationCountdown>(mob).is_some());
    assert!(
        app.world().get::<Irradiated>(mob).is_none(),
        "only warmed up"
    );

    run(&mut app, 3.0);
    radiation::radiation_pulse(app.world_mut(), source, 3.0, 0.5, 100.0, 3.0);
    run(&mut app, 0.6);
    assert!(app.world().get::<Irradiated>(mob).is_some(), "stayed 3 s");
}

/// The `radioactive` element on a uranium statue (by its type): it pulses every 3 s from when it
/// appeared, so the human beside it is first warmed up (3 s exposure) and then irradiated.
#[test]
fn uranium_statue_pulses_every_three_seconds() {
    let mut app = server("radioactive");
    let world = app.world_mut();
    let statue = world
        .spawn((
            at(8),
            ss13_mapload::DmObj {
                path: "/obj/structure/statue/uranium/nuke".into(),
                ..default()
            },
        ))
        .id();
    let mob = human(world, at(9));
    app.update();
    app.world_mut()
        .get_mut::<radiation::Radioactive>(statue)
        .expect("the uranium material makes it radioactive")
        .chance = 100.0;
    run(&mut app, 2.0);
    assert!(
        app.world().get::<RadiationCountdown>(mob).is_none(),
        "no pulse before 3 s"
    );
    run(&mut app, 2.0);
    assert!(app.world().get::<RadiationCountdown>(mob).is_some());
    assert!(app.world().get::<Irradiated>(mob).is_none());
    // The next pulses come 3 s apart; one of them lands past the exposure time.
    run(&mut app, 6.5);
    assert!(app.world().get::<Irradiated>(mob).is_some());
}

#[derive(Resource, Default)]
struct Clicks(Vec<String>);

fn record_clicks(mut sounds: MessageReader<ss13_game::sound::PlaySound>, mut out: ResMut<Clicks>) {
    out.0.extend(
        sounds
            .read()
            .filter(|s| s.sound.starts_with("sound/items/geiger/"))
            .map(|s| s.sound.clone()),
    );
}

/// A scanning counter in a hand clicks the pulse's danger (`geiger_sound`): the extreme clicks
/// every 0.2 s, silence 5 s after the last pulse; switched off it stops at once.
#[test]
fn scanning_geiger_clicks_until_the_radiation_is_gone() {
    use ss13_game::looping_sound;
    const LOOP: &str = "/datum/looping_sound/geiger";
    let mut app = server("geiger_clicks");
    app.init_resource::<Clicks>()
        .add_systems(PostUpdate, record_clicks);
    let world = app.world_mut();
    let source = world.spawn(at(3)).id();
    let holder = human(world, at(5));
    let counter = item(world, GEIGER, 0);
    world.entity_mut(counter).insert(CarriedBy(holder));
    world.get_mut::<Inventory>(holder).unwrap().hands[0] = Some(counter);
    world.entity_mut(counter).insert(Geiger::default());
    world.get_mut::<Geiger>(counter).unwrap().scanning = true;
    // An off counter on the floor next to it stays quiet.
    let off = item(world, GEIGER, 0);
    world.entity_mut(off).insert(at(5));

    radiation::radiation_pulse(world, source, 4.0, 0.5, 100.0, 0.0);
    run(&mut app, 1.0);
    assert!(looping_sound::is_active(app.world(), counter, LOOP));
    assert!(!looping_sound::is_active(app.world(), off, LOOP));
    let clicks = std::mem::take(&mut app.world_mut().resource_mut::<Clicks>().0);
    assert!(clicks.len() >= 3, "a click every 0.2 s: {clicks:?}");
    assert!(
        clicks
            .iter()
            .all(|c| c.starts_with("sound/items/geiger/ext")),
        "the extreme level: {clicks:?}"
    );

    // 5 s without a pulse: silence.
    run(&mut app, 5.0);
    assert!(!looping_sound::is_active(app.world(), counter, LOOP));
    app.world_mut().resource_mut::<Clicks>().0.clear();
    run(&mut app, 1.0);
    assert!(app.world().resource::<Clicks>().0.is_empty());

    // Clicking again, then switched off.
    let world = app.world_mut();
    radiation::radiation_pulse(world, source, 4.0, 0.5, 100.0, 0.0);
    run(&mut app, 1.0);
    assert!(looping_sound::is_active(app.world(), counter, LOOP));
    let world = app.world_mut();
    let client = world.spawn_empty().id();
    assert!(radiation::geiger_attack_self(world, client, counter));
    assert!(!looping_sound::is_active(app.world(), counter, LOOP));
}
