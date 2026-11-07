//! Station teleportation (`ss13_game::science::teleportation`) on a fixture floor, server side:
//! the teleporter hub/station/console, hand tele portals, launchpads, the satellite console and
//! gateways. Machines and items are spawned directly and the modules' procs called with a
//! stand-in client.

use std::path::Path;

use bevy::prelude::*;
use ss13_game::devices::beacons::TrackingBeacon;
use ss13_game::science::teleportation::hand_tele::{self, Dest, Portal};
use ss13_game::science::teleportation::{gateway, launchpad, sat_control, teleporter};
use ss13_game::station_goals::Satellite;
use ss13_game::stock_parts::{PartKind, PartRatings};
use ss13_game::testing::{fixture_map, server_app, steps, write_fixture};
use ss13_mapload::{DmItem, DmObj};
use ss13_shared::TilePos;

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 }, "declared_vars": { "flags_1": 0 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3 } },
    "/obj": { "parent": "/atom/movable" },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start" },
    "/obj/item": { "parent": "/obj", "declared_vars": { "interaction_flags_item": 1 } },
    "/obj/item/clothing": { "parent": "/obj/item" },
    "/obj/item/clothing/under": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 2 } },
    "/obj/item/clothing/under/color": { "parent": "/obj/item/clothing/under" },
    "/obj/item/clothing/under/color/grey": { "parent": "/obj/item/clothing/under/color" },
    "/obj/item/clothing/shoes": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 128 } },
    "/obj/item/clothing/shoes/sneakers": { "parent": "/obj/item/clothing/shoes" },
    "/obj/item/clothing/shoes/sneakers/black": { "parent": "/obj/item/clothing/shoes/sneakers" },
    "/obj/item/card": { "parent": "/obj/item" },
    "/obj/item/card/id": { "parent": "/obj/item/card", "declared_vars": {
        "access": { "list": [] }, "slot_flags": 256 } },
    "/obj/item/card/id/advanced": { "parent": "/obj/item/card/id" },
    "/mob": { "parent": "/atom/movable", "declared_vars": { "layer": 4, "density": 1 } },
    "/mob/living": { "parent": "/mob" },
    "/mob/living/carbon": { "parent": "/mob/living" },
    "/mob/living/carbon/human": { "parent": "/mob/living/carbon" },
    "/turf": { "parent": "/atom", "declared_vars": { "icon": { "resource": "t.dmi" } } },
    "/turf/open": { "parent": "/turf" },
    "/turf/open/floor": { "parent": "/turf/open", "declared_vars": { "icon_state": "floor" } },
    "/turf/open/space": { "parent": "/turf/open" },
    "/turf/open/space/basic": { "parent": "/turf/open/space" },
    "/area": { "parent": "/atom", "declared_vars": { "area_flags": 0, "requires_power": 0 } },
    "/area/space": { "parent": "/area" }
  }
}"##;

/// Rows top-down: the last line is y = 1; the top row (y = 4) is space.
const MAP: &str = r##""f" = (/turf/open/floor,/area/space)
"e" = (/turf/open/space/basic,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)

(1,1,1) = {"
eeeeeeee
ffffffff
ffffffff
sfffffff
"}
"##;

const STATES: [(&str, &str); 2] = [("t.dmi", "floor"), ("l.dmi", "x2")];
const CRATE: &str = "/obj/structure/closet/crate";

fn fixture(test: &str) -> App {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("teleportation_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/turf/open/floor",
        "/turf/open/space/basic",
        "/area/space",
    ];
    let assets = write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)]);
    server_app(fixture_map(&assets, &["map.dmm"]), |_| {})
}

fn obj(app: &mut App, path: &str, pos: TilePos, anchored: bool) -> Entity {
    app.world_mut()
        .spawn((
            pos,
            DmObj {
                path: path.into(),
                anchored,
                ..default()
            },
        ))
        .id()
}

fn item(app: &mut App, path: &str, pos: TilePos) -> Entity {
    app.world_mut()
        .spawn((
            pos,
            DmItem {
                path: path.into(),
                ..default()
            },
        ))
        .id()
}

fn pos(app: &App, e: Entity) -> TilePos {
    *app.world().get::<TilePos>(e).unwrap()
}

fn t(x: i32, y: i32) -> TilePos {
    TilePos::new(x, y, 1)
}

#[test]
fn an_engaged_teleporter_sends_what_bumps_the_hub_to_the_beacon() {
    let mut app = fixture("teleporter");
    let console = obj(&mut app, teleporter::CONSOLE, t(1, 2), true);
    let station = obj(&mut app, teleporter::STATION, t(2, 2), true);
    let hub = obj(&mut app, teleporter::HUB, t(3, 2), true);
    let beacon = item(&mut app, "/obj/item/beacon", t(6, 1));
    app.world_mut()
        .entity_mut(beacon)
        .insert(TrackingBeacon { enabled: true });
    let c = obj(&mut app, CRATE, t(4, 2), false);
    let client = app.world_mut().spawn_empty().id();
    app.update();
    let world = app.world_mut();
    assert_eq!(teleporter::station_of(world, console), Some(station));
    assert_eq!(teleporter::hub_of(world, station), Some(hub));
    assert_eq!(teleporter::console_of(world, station), Some(console));

    // No target: the station does not engage, the hub does nothing.
    teleporter::toggle(world, client, station);
    assert!(!teleporter::station_state(world, station).engaged);
    assert!(teleporter::bumped(world, c, hub));
    assert_eq!(pos(&app, c), t(4, 2));

    let world = app.world_mut();
    let targets = teleporter::get_targets(world, console);
    assert_eq!(
        targets.iter().map(|(_, e)| *e).collect::<Vec<_>>(),
        vec![beacon]
    );
    world.entity_mut(console).insert(teleporter::TeleConsole {
        target: Some(beacon),
        ..default()
    });
    teleporter::toggle(world, client, station);
    assert!(teleporter::station_state(world, station).engaged);
    assert!(teleporter::is_ready(world, hub));
    assert!(teleporter::bumped(world, c, hub));
    assert_eq!(pos(&app, c), t(6, 1));

    // The beacon switched off: target lost, station disengaged.
    let world = app.world_mut();
    teleporter::beacon_disabled(world, beacon);
    assert_eq!(teleporter::console_state(world, console).target, None);
    assert!(!teleporter::station_state(world, station).engaged);
}

#[test]
fn calibration_finishes_after_its_delay() {
    let mut app = fixture("calibration");
    let console = obj(&mut app, teleporter::CONSOLE, t(1, 2), true);
    let _station = obj(&mut app, teleporter::STATION, t(2, 2), true);
    let hub = obj(&mut app, teleporter::HUB, t(3, 2), true);
    app.update();
    let now = app.world().resource::<Time>().elapsed_secs_f64();
    app.world_mut()
        .entity_mut(console)
        .insert(teleporter::TeleConsole {
            calibrating_until: Some(now + 1.0),
            ..default()
        });
    for _ in 0..steps(1.5) {
        app.update();
    }
    assert!(teleporter::hub_state(app.world(), hub).calibrated);
    assert!(
        teleporter::console_state(app.world(), console)
            .calibrating_until
            .is_none()
    );
}

#[test]
fn portals_carry_bumpers_and_close_after_thirty_seconds() {
    let mut app = fixture("portals");
    let c = obj(&mut app, CRATE, t(1, 2), false);
    app.update();
    let (p1, p2) = hand_tele::create_portal_pair(app.world_mut(), t(2, 2), t(7, 2)).unwrap();
    let to = pos(&app, p2);
    assert!(to.chebyshev(t(7, 2)).is_some_and(|d| d <= 1));
    assert!(hand_tele::bumped(app.world_mut(), c, p1));
    assert_eq!(pos(&app, c), to);
    for _ in 0..steps(31.0) {
        app.update();
    }
    assert!(app.world().get::<Portal>(p1).is_none());
    assert!(app.world().get::<Portal>(p2).is_none());
}

#[test]
fn a_hand_tele_keeps_at_most_three_portal_pairs() {
    let mut app = fixture("hand_tele");
    let console = obj(&mut app, teleporter::CONSOLE, t(1, 2), true);
    let beacon = item(&mut app, "/obj/item/beacon", t(7, 3));
    let mob = app.world_mut().spawn(t(4, 1)).id();
    let tele = item(&mut app, hand_tele::HAND_TELE, t(4, 1));
    app.world_mut()
        .entity_mut(console)
        .insert(teleporter::TeleConsole {
            target: Some(beacon),
            ..default()
        });
    app.update();
    let dest = Dest::Console(console, beacon);
    for _ in 0..3 {
        assert!(hand_tele::try_create_portal_to(
            app.world_mut(),
            mob,
            mob,
            tele,
            dest
        ));
    }
    assert!(!hand_tele::try_create_portal_to(
        app.world_mut(),
        mob,
        mob,
        tele,
        dest
    ));
}

#[test]
fn a_launchpad_launches_and_pulls_after_charging() {
    let mut app = fixture("launchpad");
    let pad = obj(&mut app, launchpad::PAD, t(2, 2), true);
    app.world_mut().entity_mut(pad).insert(PartRatings {
        parts: vec![(PartKind::Servo, 1)],
        cells: vec![],
    });
    let c = obj(&mut app, CRATE, t(2, 2), false);
    let bolted = obj(&mut app, CRATE, t(2, 2), true);
    app.update();
    assert_eq!(launchpad::range(app.world(), pad), 10);
    // Clamped to the range.
    launchpad::set_offset(app.world_mut(), pad, Some(30), Some(0));
    assert_eq!(launchpad::pad_state(app.world(), pad).x, 10);
    launchpad::set_offset(app.world_mut(), pad, Some(4), Some(0));
    assert_eq!(launchpad::teleport_checks(app.world(), pad), None);
    launchpad::doteleport(app.world_mut(), pad, true);
    assert_eq!(
        launchpad::teleport_checks(app.world(), pad),
        Some("ERROR: Launchpad busy.")
    );
    for _ in 0..steps(3.0) {
        app.update();
    }
    assert_eq!(pos(&app, c), t(2, 2), "still charging");
    for _ in 0..steps(1.0) {
        app.update();
    }
    assert_eq!(pos(&app, c), t(6, 2));
    assert_eq!(pos(&app, bolted), t(2, 2), "anchored things stay");

    launchpad::doteleport(app.world_mut(), pad, false);
    for _ in 0..steps(4.0) {
        app.update();
    }
    assert_eq!(pos(&app, c), t(2, 2), "pulled back");
}

#[test]
fn the_satellite_console_switches_numbered_satellites_in_space_only() {
    let mut app = fixture("satellites");
    let console = obj(&mut app, sat_control::CONSOLE, t(1, 2), true);
    let in_space = obj(
        &mut app,
        ss13_game::station_goals::METEOR_SHIELD,
        t(3, 4),
        false,
    );
    let indoors = obj(
        &mut app,
        ss13_game::station_goals::METEOR_SHIELD,
        t(5, 2),
        false,
    );
    app.update();
    let id = |app: &App, e: Entity| app.world().get::<sat_control::SatelliteId>(e).unwrap().0;
    let (a, b) = (id(&app, in_space), id(&app, indoors));
    assert_ne!(a, b);
    sat_control::toggle_id(app.world_mut(), console, a);
    sat_control::toggle_id(app.world_mut(), console, b);
    let active = |app: &App, e: Entity| app.world().get::<Satellite>(e).is_some_and(|s| s.active);
    assert!(active(&app, in_space));
    assert!(!active(&app, indoors), "only in space");
    sat_control::toggle_id(app.world_mut(), console, a);
    assert!(!active(&app, in_space));
}

#[test]
fn an_away_gateway_links_home_and_carries_from_the_front() {
    let mut app = fixture("gateway");
    let home = obj(&mut app, gateway::HOME, t(2, 2), true);
    let away = obj(&mut app, gateway::AWAY, t(6, 3), true);
    let client = app.world_mut().spawn_empty().id();
    let c = obj(&mut app, CRATE, t(2, 1), false);
    let side = obj(&mut app, CRATE, t(3, 2), false);
    app.update();
    assert!(gateway::attack_hand(app.world_mut(), client, client, away));
    assert_eq!(gateway::state(app.world(), away).target, Some(home));
    assert_eq!(gateway::state(app.world(), home).target, Some(away));
    // From the side: nothing; from the front (south): out in front of the other gateway.
    assert!(gateway::bumped(app.world_mut(), side, home));
    assert_eq!(pos(&app, side), t(3, 2));
    assert!(gateway::bumped(app.world_mut(), c, home));
    assert_eq!(pos(&app, c), t(6, 2));
    // Closing one closes both.
    assert!(gateway::attack_hand(app.world_mut(), client, client, away));
    assert_eq!(gateway::state(app.world(), away).target, None);
    assert_eq!(gateway::state(app.world(), home).target, None);
}
