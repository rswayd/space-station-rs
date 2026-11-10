//! Trams and elevators (server side, no client), on fixture maps:
//! - a two-level map: a one-tile public lift with a crate on it goes up into the open space
//!   above and back down, its panel's floors are named tg's way, a mapped shuttle lift
//!   (`/obj/docking_port/mobile/elevator`) docks at once on a request;
//! - a one-level map: a two-tile tram with its nav beacon, cabinet and a crate rides from the
//!   west platform to the east one, smashing an anchored structure on the track and leaving
//!   the rail; requests that must fail do.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::shuttle::{MobileDock, StationaryDock};
use ss13_game::shuttle_nav::{self, NavShuttle};
use ss13_game::testing::{self, fixture_map, steps, write_fixture};
use ss13_game::transport::elevator::{self, LiftTrip};
use ss13_game::transport::tram::{self, Refusal, Tram, TramBeacon};
use ss13_game::transport::{Transport, TransportModule};
use ss13_mapload::DmObj;
use ss13_shared::TilePos;

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3, "anchored": 0 } },
    "/obj": { "parent": "/atom/movable", "declared_vars": { "max_integrity": 100,
        "obj_flags": 0 } },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/transport": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/transport/transport_id": {
        "parent": "/obj/effect/landmark/transport",
        "declared_vars": { "specific_transport_id": null } },
    "/obj/effect/landmark/transport/nav_beacon": { "parent": "/obj/effect/landmark/transport" },
    "/obj/effect/landmark/transport/nav_beacon/tram": {
        "parent": "/obj/effect/landmark/transport/nav_beacon",
        "declared_vars": { "specific_transport_id": "tram_1", "platform_code": null } },
    "/obj/effect/landmark/transport/nav_beacon/tram/nav": {
        "parent": "/obj/effect/landmark/transport/nav_beacon/tram",
        "declared_vars": { "invisibility": 100, "specific_transport_id": "tram_nav", "dir": 8,
        "name": "tram announcement system" } },
    "/obj/effect/landmark/transport/nav_beacon/tram/platform": {
        "parent": "/obj/effect/landmark/transport/nav_beacon/tram" },
    "/obj/effect/landmark/transport/nav_beacon/tram/platform/west": {
        "parent": "/obj/effect/landmark/transport/nav_beacon/tram/platform",
        "declared_vars": { "name": "Arrivals", "platform_code": 1 } },
    "/obj/effect/landmark/transport/nav_beacon/tram/platform/east": {
        "parent": "/obj/effect/landmark/transport/nav_beacon/tram/platform",
        "declared_vars": { "name": "Escape", "platform_code": 3 } },
    "/obj/structure": { "parent": "/obj", "declared_vars": { "anchored": 1 } },
    "/obj/structure/fluff": { "parent": "/obj/structure", "declared_vars": {
        "icon": { "resource": "s.dmi" }, "icon_state": "fluff", "density": 1 } },
    "/obj/structure/fluff/tram_rail": { "parent": "/obj/structure/fluff", "declared_vars": {
        "density": 0, "layer": 2.2 } },
    "/obj/structure/fluff/box": { "parent": "/obj/structure/fluff", "declared_vars": {
        "icon_state": "crate", "anchored": 0, "density": 0 } },
    "/obj/structure/transport": { "parent": "/obj/structure" },
    "/obj/structure/transport/linear": { "parent": "/obj/structure/transport",
        "declared_vars": { "icon": { "resource": "s.dmi" }, "icon_state": "catwalk",
        "obj_flags": 32, "layer": 2.5, "transport_id": "icts_elev",
        "elevator_vertical_speed": 20, "violent_landing": 1, "warns_on_down_movement": 0,
        "radial_travel": 1 } },
    "/obj/structure/transport/linear/public": { "parent": "/obj/structure/transport/linear",
        "declared_vars": { "elevator_vertical_speed": 30, "violent_landing": 0,
        "radial_travel": 0 } },
    "/obj/structure/transport/linear/tram": { "parent": "/obj/structure/transport/linear",
        "declared_vars": { "transport_id": "icts_tram", "radial_travel": 0 } },
    "/obj/machinery": { "parent": "/obj", "declared_vars": { "anchored": 1 } },
    "/obj/machinery/transport": { "parent": "/obj/machinery",
        "declared_vars": { "configured_transport_id": "tram_1" } },
    "/obj/machinery/transport/tram_controller": { "parent": "/obj/machinery/transport",
        "declared_vars": { "icon": { "resource": "s.dmi" }, "icon_state": "cabinet",
        "name": "tram controller" } },
    "/obj/machinery/elevator_control_panel": { "parent": "/obj/machinery",
        "declared_vars": { "icon": { "resource": "s.dmi" }, "icon_state": "panel",
        "linked_elevator_id": null, "req_access": null, "preset_destination_names": null } },
    "/obj/docking_port": { "parent": "/obj", "declared_vars": { "invisibility": 101,
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "anchored": 1, "dir": 1,
        "shuttle_id": null, "width": 0, "height": 0, "dwidth": 0, "dheight": 0 } },
    "/obj/docking_port/stationary": { "parent": "/obj/docking_port", "declared_vars": {
        "json_key": null } },
    "/obj/docking_port/mobile": { "parent": "/obj/docking_port" },
    "/obj/docking_port/mobile/elevator": { "parent": "/obj/docking_port/mobile",
        "declared_vars": { "shuttle_id": "elevator", "name": "elevator" } },
    "/turf": { "parent": "/atom", "declared_vars": { "icon": { "resource": "t.dmi" },
        "blocks_air": 0, "baseturfs": { "path": "/turf/baseturf_bottom" } } },
    "/turf/open": { "parent": "/turf" },
    "/turf/open/floor": { "parent": "/turf/open", "declared_vars": { "icon_state": "floor",
        "baseturfs": { "path": "/turf/open/floor/plating" } } },
    "/turf/open/floor/plating": { "parent": "/turf/open/floor", "declared_vars": {
        "icon_state": "plating", "baseturfs": { "path": "/turf/baseturf_bottom" } } },
    "/turf/open/openspace": { "parent": "/turf/open", "declared_vars": {
        "icon_state": "openspace" } },
    "/turf/closed": { "parent": "/turf", "declared_vars": { "density": 1, "opacity": 1,
        "blocks_air": 1 } },
    "/turf/closed/wall": { "parent": "/turf/closed", "declared_vars": { "icon_state": "wall",
        "baseturfs": { "path": "/turf/open/floor/plating" } } },
    "/area": { "parent": "/atom", "declared_vars": { "requires_power": 0 } },
    "/area/station": { "parent": "/area", "declared_vars": { "name": "Station" } },
    "/area/shuttle": { "parent": "/area", "declared_vars": { "name": "Lift" } }
  }
}"##;

/// Rows top-down, the last line is y = 1. Level 1: the lift `L` (3,2) with a crate, its panel
/// `P` beside it; the shuttle lift `E` (6,2) on its dock. Level 2: open space `o` over the lift,
/// the shuttle lift's upper dock `U` (6,2).
const LIFT_MAP: &str = r##""w" = (/turf/closed/wall,/area/station)
"f" = (/turf/open/floor,/area/station)
"L" = (/obj/structure/transport/linear/public,/obj/effect/landmark/transport/transport_id{specific_transport_id = "lift"},/obj/structure/fluff/box,/turf/open/floor,/area/station)
"P" = (/obj/machinery/elevator_control_panel{linked_elevator_id = "lift"; preset_destination_names = list("3" = "Upper   Deck")},/turf/open/floor,/area/station)
"E" = (/obj/docking_port/mobile/elevator{width = 1; height = 1},/obj/docking_port/stationary{shuttle_id = "lift_lower"; width = 1; height = 1; name = "Lower"},/turf/open/floor/plating,/area/shuttle)
"o" = (/turf/open/openspace,/area/station)
"U" = (/obj/docking_port/stationary{shuttle_id = "lift_upper"; width = 1; height = 1; name = "Upper"},/turf/open/floor/plating,/area/station)

(1,1,1) = {"
wwwwwww
wfLPfEw
wwwwwww
"}
(1,1,2) = {"
wwwwwww
wfofwUw
wwwwwww
"}
"##;

/// Level 1: the tram (two modules `T`, `C`) at the west platform; `C` holds the cabinet, the
/// nav beacon and a crate; an anchored fluff `X` on the track, the east platform `E` (12,2).
const TRAM_MAP: &str = r##""w" = (/turf/closed/wall,/area/station)
"r" = (/obj/structure/fluff/tram_rail,/turf/open/floor/plating,/area/station)
"T" = (/obj/structure/transport/linear/tram,/obj/effect/landmark/transport/transport_id{specific_transport_id = "tram_1"},/obj/effect/landmark/transport/nav_beacon/tram/platform/west,/turf/open/floor/plating,/area/station)
"C" = (/obj/structure/transport/linear/tram,/obj/machinery/transport/tram_controller,/obj/effect/landmark/transport/nav_beacon/tram/nav,/obj/structure/fluff/box,/turf/open/floor/plating,/area/station)
"X" = (/obj/structure/fluff,/obj/structure/fluff/tram_rail,/turf/open/floor/plating,/area/station)
"E" = (/obj/effect/landmark/transport/nav_beacon/tram/platform/east,/obj/structure/fluff/tram_rail,/turf/open/floor/plating,/area/station)

(1,1,1) = {"
wwwwwwwwwwwwww
wTCrrrXrrrrErw
wwwwwwwwwwwwww
"}
"##;

const STATES: [(&str, &str); 11] = [
    ("t.dmi", "floor"),
    ("t.dmi", "plating"),
    ("t.dmi", "wall"),
    ("t.dmi", "openspace"),
    ("l.dmi", "x2"),
    ("s.dmi", "fluff"),
    ("s.dmi", "crate"),
    ("s.dmi", "catwalk"),
    ("s.dmi", "cabinet"),
    ("s.dmi", "panel"),
    ("i.dmi", "toy"),
];

fn fixture(test: &str, map: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("transport_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/transport/transport_id",
        "/obj/effect/landmark/transport/nav_beacon/tram/nav",
        "/obj/effect/landmark/transport/nav_beacon/tram/platform/west",
        "/obj/effect/landmark/transport/nav_beacon/tram/platform/east",
        "/obj/structure/fluff",
        "/obj/structure/fluff/tram_rail",
        "/obj/structure/fluff/box",
        "/obj/structure/transport/linear/public",
        "/obj/structure/transport/linear/tram",
        "/obj/machinery/transport/tram_controller",
        "/obj/machinery/elevator_control_panel",
        "/obj/docking_port/stationary",
        "/obj/docking_port/mobile/elevator",
        "/turf/open/floor",
        "/turf/open/floor/plating",
        "/turf/open/openspace",
        "/turf/closed/wall",
        "/area/station",
        "/area/shuttle",
    ];
    write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", map)])
}

fn run(app: &mut App, secs: f64) {
    for _ in 0..steps(secs) {
        app.update();
    }
}

fn find_obj(app: &mut App, path: &str) -> Vec<(Entity, TilePos)> {
    let world = app.world_mut();
    let mut q = world.query::<(Entity, &DmObj, &TilePos)>();
    q.iter(world)
        .filter(|(_, o, _)| o.path == path)
        .map(|(e, _, p)| (e, *p))
        .collect()
}

fn pos(app: &App, e: Entity) -> TilePos {
    *app.world().get::<TilePos>(e).unwrap()
}

#[test]
fn lift_goes_up_and_down_with_its_load() {
    let mut app = testing::server_app(
        fixture_map(&fixture("lift", LIFT_MAP), &["map.dmm"]),
        |_| {},
    );
    app.update();
    let lift_module = find_obj(&mut app, "/obj/structure/transport/linear/public")[0].0;
    let crate_e = find_obj(&mut app, "/obj/structure/fluff/box")[0].0;
    let lift = app
        .world()
        .get::<TransportModule>(lift_module)
        .expect("the module joined a transport")
        .transport;
    let start = pos(&app, lift_module);
    assert_eq!(start, TilePos::new(3, 2, 1));
    assert_eq!(
        app.world().get::<Transport>(lift).unwrap().specific_id,
        "lift"
    );
    assert!(
        find_obj(&mut app, "/obj/effect/landmark/transport/transport_id").is_empty(),
        "the id landmark is used up"
    );

    // The panel names the floors: tg's level 3 is the preset, level 2 "Floor 1".
    let floors = elevator::floors(
        app.world(),
        lift,
        &[("3".to_owned(), "Upper Deck".to_owned())],
    );
    assert_eq!(
        floors,
        vec![("Upper Deck".to_owned(), 2), ("Floor 1".to_owned(), 1)]
    );
    let panel = find_obj(&mut app, "/obj/machinery/elevator_control_panel")[0].0;
    let linked = app
        .world()
        .get::<elevator::ElevatorPanel>(panel)
        .expect("the panel linked")
        .clone();
    assert_eq!(linked.lift, Some(lift));
    assert_eq!(linked.floors, floors);

    assert!(elevator::move_to_zlevel(
        app.world_mut(),
        lift,
        2,
        None,
        None
    ));
    assert!(app.world().get::<Transport>(lift).unwrap().locked());
    // A second trip is refused while it moves (`CONTROLS_LOCKED` via the panel's act).
    run(&mut app, 1.0);
    assert_eq!(pos(&app, lift_module).z, 1, "the doors close first (1.1 s)");
    run(&mut app, 4.0);
    assert_eq!(pos(&app, lift_module), TilePos::new(3, 2, 2));
    assert_eq!(
        pos(&app, crate_e),
        TilePos::new(3, 2, 2),
        "the crate rode along"
    );
    assert!(app.world().get::<LiftTrip>(lift).is_none());
    assert!(!app.world().get::<Transport>(lift).unwrap().locked());
    // Already there: nothing to do.
    assert!(!elevator::move_to_zlevel(
        app.world_mut(),
        lift,
        2,
        None,
        None
    ));

    assert!(elevator::move_to_zlevel(
        app.world_mut(),
        lift,
        1,
        None,
        None
    ));
    run(&mut app, 5.0);
    assert_eq!(pos(&app, lift_module), start);
    assert_eq!(pos(&app, crate_e), start);
}

#[test]
fn shuttle_lift_docks_at_once() {
    let mut app = testing::server_app(
        fixture_map(&fixture("shuttle_lift", LIFT_MAP), &["map.dmm"]),
        |_| {},
    );
    app.update();
    let world = app.world_mut();
    let port = shuttle_nav::get_shuttle(world, "elevator").expect("the lift registered");
    assert!(world.get::<NavShuttle>(port).unwrap().instant);
    let mut q = world.query::<(Entity, &StationaryDock)>();
    let upper = q
        .iter(world)
        .find(|(_, d)| d.id == "lift_upper")
        .map(|(e, _)| e)
        .unwrap();
    assert!(shuttle_nav::request(world, port, upper));
    assert_eq!(world.get::<MobileDock>(port).unwrap().docked, Some(upper));
    assert_eq!(*world.get::<TilePos>(port).unwrap(), TilePos::new(6, 2, 2));
    assert_eq!(shuttle_nav::flight(world, port), shuttle_nav::Flight::Idle);
}

#[test]
fn tram_rides_to_the_east_platform() {
    let mut app = testing::server_app(
        fixture_map(&fixture("tram", TRAM_MAP), &["map.dmm"]),
        |_| {},
    );
    app.update();
    let modules = find_obj(&mut app, "/obj/structure/transport/linear/tram");
    assert_eq!(modules.len(), 2);
    let tram_e = app
        .world()
        .get::<TransportModule>(modules[0].0)
        .unwrap()
        .transport;
    let crate_e = find_obj(&mut app, "/obj/structure/fluff/box")[0].0;
    let rails_before = find_obj(&mut app, "/obj/structure/fluff/tram_rail").len();
    let state = app.world().get::<Tram>(tram_e).expect("a tram").clone();
    assert!(state.cabinet.is_some() && state.nav_beacon.is_some());
    let west = state.idle_platform.expect("it starts at the west platform");
    assert_eq!(
        app.world().get::<TramBeacon>(west).unwrap().name,
        "Arrivals"
    );

    let world = app.world_mut();
    assert_eq!(
        tram::request(world, "tram_1", "1"),
        Err(Refusal::NoCallRequired)
    );
    assert_eq!(
        tram::request(world, "tram_1", "9"),
        Err(Refusal::InvalidPlatform)
    );
    assert_eq!(
        tram::request(world, "no_line", "3"),
        Err(Refusal::BrokenBeyondRepair)
    );
    assert_eq!(tram::request(world, "tram_1", "3"), Ok("Escape".to_owned()));
    assert_eq!(tram::request(world, "tram_1", "1"), Err(Refusal::InUse));
    // Nav beacon at (3,2), the east platform at (12,2): nine tiles east.
    let tram = world.get::<Tram>(tram_e).unwrap();
    assert_eq!((tram.travel, tram.travel_remaining), ((1, 0), 9));

    run(&mut app, 1.0);
    assert_eq!(pos(&app, crate_e), TilePos::new(3, 2, 1), "still boarding");
    run(&mut app, 8.0);
    let positions: Vec<TilePos> = modules.iter().map(|(e, _)| pos(&app, *e)).collect();
    assert!(positions.contains(&TilePos::new(11, 2, 1)));
    assert!(positions.contains(&TilePos::new(12, 2, 1)));
    assert_eq!(
        pos(&app, crate_e),
        TilePos::new(12, 2, 1),
        "the crate rode along"
    );
    assert!(
        find_obj(&mut app, "/obj/structure/fluff").is_empty(),
        "the tram smashed the anchored fluff on the track"
    );
    assert_eq!(
        find_obj(&mut app, "/obj/structure/fluff/tram_rail").len(),
        rails_before,
        "the rails stay where they are"
    );
    let tram = app.world().get::<Tram>(tram_e).unwrap();
    assert!(!tram.active, "the controls unlock 2 s after arrival");
    let east = tram.idle_platform.unwrap();
    assert_eq!(app.world().get::<TramBeacon>(east).unwrap().name, "Escape");
    assert_eq!(
        tram::request(app.world_mut(), "tram_1", "3"),
        Err(Refusal::NoCallRequired)
    );
}
