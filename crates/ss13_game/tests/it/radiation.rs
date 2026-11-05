//! Radiation (`ss13_game::radiation`) on the power fixture room (`super::power`) without clients:
//! a sure pulse irradiates a bare human in the open (10 toxin, the alert, the Geiger counter in
//! his hand reads the extreme level), not one behind a full insulator, not one in a radiation
//! suit and hood; a pulse with a minimum exposure time only warms a human up, a pulse 3 s later
//! irradiates him; healing the toxin damage ends the irradiation.

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
