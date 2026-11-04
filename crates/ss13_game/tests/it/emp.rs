//! EMPs (`ss13_game::emp`) on the power fixture room (`super::power`) without clients: a pulse's
//! falloff (heavy inside `heavy_range`, a coin flip on it, light up to `light_range`, nothing
//! beyond), `EMP_PROTECT_SELF`, a mob's contents with its tinfoil hat lowering their severity, a
//! bag's contents; the APC's channels going off and coming back, the lights burning out, the
//! SMES drained; a radio silenced for 20 s; a camera off the network for 90 s.

use bevy::prelude::*;
use ss13_game::emp::{EmpProtection, Emped, Empulse, PROTECT_SELF};
use ss13_game::health::Health;
use ss13_game::inventory::{CarriedBy, Inventory};
use ss13_game::mecha::core::ItemCell;
use ss13_game::power::model::{CHANNEL_AUTO_ON, CHANNEL_OFF};
use ss13_game::power::{Apc, Cell, LightFixture, Smes};
use ss13_game::radio::Radio;
use ss13_game::silicon::cameras::{Camera, CameraNet};
use ss13_game::storage::{Storage, StoredIn};
use ss13_game::testing::{self, fixture_map, steps};
use ss13_mapload::DmItem;
use ss13_mapload::radio::DmRadio;
use ss13_shared::TilePos;
use ss13_world::SpatialIndex;

const FULL: f64 = 100_000.0;
const APC: TilePos = TilePos::new(7, 5, 1);
const LIGHT: TilePos = TilePos::new(6, 4, 1);
const SMES: TilePos = TilePos::new(4, 5, 1);

fn server(test: &str) -> App {
    let assets = super::power::fixture_assets(&format!("emp_{test}"));
    let mut app = testing::server_app(fixture_map(&assets, &["map.dmm"]), |_| {});
    app.update();
    app
}

fn cell() -> ItemCell {
    ItemCell(Cell {
        charge: FULL,
        maxcharge: FULL,
        chargerate: 1000.0,
    })
}

fn charge(app: &App, e: Entity) -> f64 {
    app.world().get::<ItemCell>(e).unwrap().0.charge
}

fn item(path: &str) -> DmItem {
    DmItem {
        path: path.into(),
        ..default()
    }
}

fn pulse(app: &mut App, epicenter: TilePos, heavy: i32, light: i32) {
    app.world_mut().write_message(Empulse {
        epicenter,
        heavy,
        light,
    });
    // Resolve, then the handlers (the same tick).
    app.update();
}

fn find<T: Component>(app: &mut App, pos: TilePos) -> Entity {
    let world = app.world();
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .find(|&e| world.get::<T>(e).is_some())
        .unwrap()
}

/// Cells on the floor of row 2 at distances 0..3 from (3,2), a pulse of heavy 1 and light 2: the
/// cell on the epicenter loses `STANDARD_CELL_CHARGE` (heavy), the one at 1 heavy or light, at 2
/// half of it (light), at 3 nothing; a `EMP_PROTECT_SELF` cell nothing. A mob on the epicenter
/// passes the heavy pulse to the cell in its hand; with a tinfoil hat it passes a light one. A
/// bag at 2 passes its light pulse to the cell inside.
#[test]
fn pulse_falls_off_and_protection_and_contents() {
    let mut app = server("falloff");
    let at = |x| TilePos::new(x, 2, 1);
    let world = app.world_mut();
    let cells: Vec<Entity> = (3..=6).map(|x| world.spawn((at(x), cell())).id()).collect();
    let shielded = world
        .spawn((at(3), cell(), EmpProtection(PROTECT_SELF)))
        .id();
    let mut human = |hat: bool| {
        let held = world.spawn(cell()).id();
        let mob = world.spawn((at(3), Health::default())).id();
        world.entity_mut(held).insert(CarriedBy(mob));
        let mut inventory = Inventory::default();
        inventory.hands[0] = Some(held);
        if hat {
            let foil = world
                .spawn((
                    item("/obj/item/clothing/head/costume/foilhat"),
                    CarriedBy(mob),
                ))
                .id();
            inventory.head = Some(foil);
        }
        world.entity_mut(mob).insert(inventory);
        held
    };
    let bare = human(false);
    let hatted = human(true);
    let bag = world
        .spawn((
            at(5),
            Storage {
                max_slots: 7,
                max_specific_storage: 3,
                max_total_storage: 14,
            },
        ))
        .id();
    let bagged = world.spawn((cell(), StoredIn(bag))).id();

    pulse(&mut app, at(3), 1, 2);

    assert_eq!(
        charge(&app, cells[0]),
        FULL - 10_000.0,
        "heavy on the epicenter"
    );
    let edge = charge(&app, cells[1]);
    assert!(
        edge == FULL - 10_000.0 || edge == FULL - 5_000.0,
        "heavy or light at heavy_range: {edge}"
    );
    assert_eq!(charge(&app, cells[2]), FULL - 5_000.0, "light");
    assert_eq!(charge(&app, cells[3]), FULL, "beyond light_range");
    assert_eq!(charge(&app, shielded), FULL, "EMP_PROTECT_SELF");
    assert_eq!(
        charge(&app, bare),
        FULL - 10_000.0,
        "held: the mob's severity"
    );
    assert_eq!(
        charge(&app, hatted),
        FULL - 5_000.0,
        "the hat adds 1 to the severity"
    );
    assert_eq!(
        charge(&app, bagged),
        FULL - 5_000.0,
        "a bag passes its pulse on"
    );
}

/// A heavy pulse on the APC: every channel off, equipment and environment back on auto a
/// minute later (the lights stay off, as in tg); the light next to it burns out and loses its
/// emergency charge; the SMES three tiles away loses `STANDARD_BATTERY_CHARGE / 2`.
#[test]
fn apc_lights_and_smes() {
    let mut app = server("apc");
    let apc = find::<Apc>(&mut app, APC);
    let light = find::<LightFixture>(&mut app, LIGHT);
    let smes = find::<Smes>(&mut app, SMES);
    let smes_before = app.world().get::<Smes>(smes).unwrap().charge;
    assert!(
        app.world()
            .get::<LightFixture>(light)
            .unwrap()
            .on_light
            .is_some()
    );

    pulse(&mut app, APC, 2, 3);

    assert_eq!(
        app.world().get::<Apc>(apc).unwrap().channels,
        [CHANNEL_OFF; 3]
    );
    let fixture = app.world().get::<LightFixture>(light).unwrap();
    assert!(
        fixture.on_light.is_none(),
        "150% to burn out on a heavy pulse"
    );
    let smes_after = app.world().get::<Smes>(smes).unwrap().charge;
    assert!(
        (smes_before - smes_after - 500_000.0).abs() < 1e3,
        "light pulse on the SMES: {smes_before} -> {smes_after}"
    );

    for _ in 0..steps(61.0) {
        app.update();
    }
    let channels = app.world().get::<Apc>(apc).unwrap().channels;
    assert_eq!(channels[0], CHANNEL_AUTO_ON, "equipment");
    assert_eq!(channels[2], CHANNEL_AUTO_ON, "environment");
    assert_eq!(channels[1], CHANNEL_OFF, "the lights stay off");
    assert!(app.world().get::<Emped>(apc).is_none());
}

/// A radio goes off with every channel unlistened, and on again 20 s after the last pulse; a
/// camera leaves the network for 90 s.
#[test]
fn radio_and_camera_come_back() {
    let mut app = server("radio");
    let at = TilePos::new(9, 2, 1);
    let radio = Radio::new(&DmRadio {
        path: "/obj/item/radio".into(),
        on: true,
        listening: true,
        ..default()
    });
    let world = app.world_mut();
    let radio = world.spawn((at, radio)).id();
    let camera = world
        .spawn((
            at,
            Camera {
                c_tag: "test".into(),
                network: vec!["ss13".into()],
                view_range: 7,
                enabled: true,
            },
        ))
        .id();

    pulse(&mut app, at, 1, 1);
    assert!(!app.world().get::<Radio>(radio).unwrap().on);
    assert!(!app.world().get::<Camera>(camera).unwrap().enabled);
    assert!(!app.world().resource::<CameraNet>().visible(at));

    for _ in 0..steps(21.0) {
        app.update();
    }
    assert!(app.world().get::<Radio>(radio).unwrap().on, "on after 20 s");
    assert!(!app.world().get::<Camera>(camera).unwrap().enabled);
    for _ in 0..steps(70.0) {
        app.update();
    }
    assert!(
        app.world().get::<Camera>(camera).unwrap().enabled,
        "back after 90 s"
    );
    assert!(app.world().resource::<CameraNet>().visible(at));
}
