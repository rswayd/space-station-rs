//! Airlock control on a fixture room: cyclelinked and cycle-group airlocks close each other
//! (`ss13_game::airlock_cycle`), plating goes on and comes off (`ss13_game::airlock_security`),
//! an access button opens its door through its console (`ss13_game::airlock_controllers`), and a
//! door remote works the doors its access fits (`ss13_game::devices::door_remote`).

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::airlock_controllers::{AccessControl, AccessKind};
use ss13_game::airlock_security::{NONE, PLASTEEL, PLASTEEL_O};
use ss13_game::construction::{spawn_obj, spawn_stack};
use ss13_game::devices::door_remote::{DoorRemote, Mode, use_on};
use ss13_game::doors::{Door, DoorJob, DoorState, with_door};
use ss13_game::inventory::CarriedBy;
use ss13_game::testing::{self, steps, write_fixture};
use ss13_mapload::{DmDoor, DmItem, MapLoadPlugin};
use ss13_shared::TilePos;
use ss13_world::SpatialIndex;

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3, "anchored": 0 } },
    "/obj": { "parent": "/atom/movable", "declared_vars": { "id_tag": null } },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start" },
    "/obj/effect/mapping_helpers": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "m.dmi" }, "icon_state": "", "layer": 5 } },
    "/obj/effect/mapping_helpers/airlock": { "parent": "/obj/effect/mapping_helpers" },
    "/obj/effect/mapping_helpers/airlock/cyclelink_helper": {
        "parent": "/obj/effect/mapping_helpers/airlock" },
    "/obj/effect/mapping_helpers/airlock/cyclelink_helper_multi": {
        "parent": "/obj/effect/mapping_helpers/airlock", "declared_vars": { "cycle_id": null } },
    "/obj/item": { "parent": "/obj", "declared_vars": { "icon": { "resource": "i.dmi" },
        "icon_state": "toy", "interaction_flags_item": 1, "tool_behaviour": null,
        "toolspeed": 1, "heat": 0 } },
    "/obj/item/door_remote": { "parent": "/obj/item", "declared_vars": {
        "name": "control wand", "icon_state": "remote" } },
    "/obj/item/door_remote/chief_engineer": { "parent": "/obj/item/door_remote" },
    "/obj/item/stack": { "parent": "/obj/item", "declared_vars": { "amount": 1,
        "max_amount": 50, "merge_type": null, "singular_name": null, "turf_type": null } },
    "/obj/item/stack/sheet": { "parent": "/obj/item/stack" },
    "/obj/item/stack/sheet/iron": { "parent": "/obj/item/stack/sheet", "declared_vars": {
        "name": "iron", "icon_state": "sheet-metal",
        "merge_type": { "path": "/obj/item/stack/sheet/iron" } } },
    "/obj/item/stack/sheet/plasteel": { "parent": "/obj/item/stack/sheet", "declared_vars": {
        "name": "plasteel", "icon_state": "sheet-plasteel",
        "merge_type": { "path": "/obj/item/stack/sheet/plasteel" } } },
    "/obj/item/card": { "parent": "/obj/item" },
    "/obj/item/card/id": { "parent": "/obj/item/card", "declared_vars": { "slot_flags": 256 } },
    "/obj/item/card/id/advanced": { "parent": "/obj/item/card/id" },
    "/obj/item/clothing": { "parent": "/obj/item" },
    "/obj/item/clothing/under": { "parent": "/obj/item/clothing" },
    "/obj/item/clothing/under/color": { "parent": "/obj/item/clothing/under" },
    "/obj/item/clothing/under/color/grey": { "parent": "/obj/item/clothing/under/color" },
    "/obj/item/clothing/shoes": { "parent": "/obj/item/clothing" },
    "/obj/item/clothing/shoes/sneakers": { "parent": "/obj/item/clothing/shoes" },
    "/obj/item/clothing/shoes/sneakers/black": { "parent": "/obj/item/clothing/shoes/sneakers" },
    "/obj/machinery": { "parent": "/obj", "declared_vars": { "anchored": 1,
        "req_access": null, "req_one_access": null } },
    "/obj/machinery/door": { "parent": "/obj/machinery", "declared_vars": {
        "icon": { "resource": "d.dmi" }, "icon_state": "door_closed", "density": 1, "opacity": 1,
        "layer": 2.7, "base_icon_state": "door", "autoclose": 0, "can_open_with_hands": 1,
        "closingLayer": 3.1, "glass": 0, "locked": 0, "safe": 1, "visible": 1, "welded": 0 } },
    "/obj/machinery/door/airlock": { "parent": "/obj/machinery/door", "declared_vars": {
        "name": "airlock", "icon": { "resource": "a.dmi" }, "icon_state": "closed",
        "autoclose": 0, "base_icon_state": null, "normalspeed": 1,
        "overlays_file": { "resource": "o.dmi" }, "airlock_material": null,
        "security_level": 0, "closeOtherId": null } },
    "/obj/machinery/door_buttons": { "parent": "/obj/machinery", "declared_vars": {
        "icon": { "resource": "w.dmi" }, "idSelf": null } },
    "/obj/machinery/door_buttons/access_button": { "parent": "/obj/machinery/door_buttons",
        "declared_vars": { "icon_state": "access_button_standby", "idDoor": null } },
    "/obj/machinery/door_buttons/airlock_controller": { "parent": "/obj/machinery/door_buttons",
        "declared_vars": { "name": "access console", "icon_state": "access_control_standby",
        "idInterior": null, "idExterior": null } },
    "/mob": { "parent": "/atom/movable", "declared_vars": { "layer": 4, "density": 1 } },
    "/mob/living": { "parent": "/mob" },
    "/mob/living/carbon": { "parent": "/mob/living" },
    "/mob/living/carbon/human": { "parent": "/mob/living/carbon", "declared_vars": {
        "icon": { "resource": "h.dmi" }, "icon_state": "human_basic" } },
    "/turf": { "parent": "/atom", "declared_vars": { "icon": { "resource": "t.dmi" } } },
    "/turf/open": { "parent": "/turf" },
    "/turf/open/floor": { "parent": "/turf/open", "declared_vars": { "icon_state": "floor" } },
    "/turf/closed": { "parent": "/turf", "declared_vars": { "density": 1, "opacity": 1 } },
    "/turf/closed/wall": { "parent": "/turf/closed", "declared_vars": { "icon_state": "wall" } },
    "/area": { "parent": "/atom" },
    "/area/space": { "parent": "/area" }
  }
}"##;

/// Rows top-down: the last line is y = 1. `A` (2,4) and `B` (5,4) are cyclelinked facing each
/// other; `G` (7,4) and (9,4) share the cycle id "pod". `I` (2,2) and `E` (4,2) are the console
/// `C`'s doors, `K` its button for `E`. `N` (8,3) needs engineering access, `S` (9,3) security.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"A" = (/obj/machinery/door/airlock,/obj/effect/mapping_helpers/airlock/cyclelink_helper{dir = 4},/turf/open/floor,/area/space)
"B" = (/obj/machinery/door/airlock,/obj/effect/mapping_helpers/airlock/cyclelink_helper{dir = 8},/turf/open/floor,/area/space)
"G" = (/obj/machinery/door/airlock,/obj/effect/mapping_helpers/airlock/cyclelink_helper_multi{cycle_id = "pod"},/turf/open/floor,/area/space)
"I" = (/obj/machinery/door/airlock{id_tag = "in"},/turf/open/floor,/area/space)
"E" = (/obj/machinery/door/airlock{id_tag = "out"},/turf/open/floor,/area/space)
"C" = (/obj/machinery/door_buttons/airlock_controller{idInterior = "in"; idExterior = "out"; idSelf = "ctl"},/turf/open/floor,/area/space)
"K" = (/obj/machinery/door_buttons/access_button{idDoor = "out"; idSelf = "ctl"; req_access = list("virology")},/turf/open/floor,/area/space)
"N" = (/obj/machinery/door/airlock{req_access = list("engineering")},/turf/open/floor,/area/space)
"S" = (/obj/machinery/door/airlock{req_access = list("security")},/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwwwwwww
wAffBfGfGw
wsfffffNSw
wIfEfCfKfw
wwwwwwwwww
"}
"##;

const A: TilePos = TilePos::new(2, 4, 1);
const B: TilePos = TilePos::new(5, 4, 1);
const G1: TilePos = TilePos::new(7, 4, 1);
const G2: TilePos = TilePos::new(9, 4, 1);
const I: TilePos = TilePos::new(2, 2, 1);
const E: TilePos = TilePos::new(4, 2, 1);
const C: TilePos = TilePos::new(6, 2, 1);
const K: TilePos = TilePos::new(8, 2, 1);
const N: TilePos = TilePos::new(8, 3, 1);
const S: TilePos = TilePos::new(9, 3, 1);
/// Where the sheets for the plating lie (the user stands on [`USER`]).
const SHEETS_AT: TilePos = TilePos::new(4, 3, 1);
const USER: TilePos = TilePos::new(3, 3, 1);

/// Manifest order = `IconStateId`.
const STATES: [(&str, &str); 16] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
    ("i.dmi", "remote"),
    ("i.dmi", "sheet-metal"),
    ("i.dmi", "sheet-plasteel"),
    ("a.dmi", "closed"),
    ("a.dmi", "opening"),
    ("a.dmi", "open"),
    ("a.dmi", "closing"),
    ("w.dmi", "access_button_standby"),
    ("w.dmi", "access_button_cycle"),
    ("w.dmi", "access_control_standby"),
    ("w.dmi", "access_control_process"),
];

fn fixture(test: &str) -> MapLoadPlugin {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("airlock_control_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/effect/mapping_helpers/airlock/cyclelink_helper",
        "/obj/effect/mapping_helpers/airlock/cyclelink_helper_multi",
        "/obj/item/door_remote/chief_engineer",
        "/obj/item/stack/sheet/iron",
        "/obj/item/stack/sheet/plasteel",
        "/obj/machinery/door/airlock",
        "/obj/machinery/door_buttons/access_button",
        "/obj/machinery/door_buttons/airlock_controller",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/area/space",
    ];
    let assets: PathBuf = write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)]);
    testing::fixture_map(&assets, &["map.dmm"])
}

fn first(world: &World, pos: TilePos, f: impl Fn(&World, Entity) -> bool) -> Entity {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .find(|&e| f(world, e))
        .unwrap_or_else(|| panic!("nothing wanted on {pos:?}"))
}

fn door_at(world: &World, pos: TilePos) -> Entity {
    first(world, pos, |w, e| w.get::<Door>(e).is_some())
}

fn state(world: &World, door: Entity) -> DoorState {
    world.get::<Door>(door).unwrap().state
}

fn open(world: &mut World, door: Entity) {
    with_door(world, door, |d, appearance, ctx| {
        ((), ctx.open(d, appearance, false))
    });
}

fn run(app: &mut App, secs: f64) {
    for _ in 0..steps(secs) {
        app.update();
    }
}

#[test]
fn linked_airlocks_close_each_other() {
    let mut app = testing::server_app(fixture("cycle"), |_| {});
    let world = app.world_mut();
    let (a, b) = (door_at(world, A), door_at(world, B));
    let (g1, g2) = (door_at(world, G1), door_at(world, G2));
    assert_eq!(world.get::<Door>(a).unwrap().cyclelinked, Some(b));
    assert_eq!(world.get::<Door>(b).unwrap().cyclelinked, Some(a));
    assert_eq!(world.get::<Door>(g1).unwrap().close_others, vec![g2]);
    assert_eq!(world.get::<Door>(g2).unwrap().close_other_id, "pod");

    open(world, b);
    open(world, g2);
    run(&mut app, 1.0);
    let world = app.world_mut();
    assert_eq!(state(world, b), DoorState::Open);
    assert_eq!(state(world, g2), DoorState::Open);

    open(world, a);
    open(world, g1);
    run(&mut app, 2.0);
    let world = app.world();
    assert_eq!(state(world, a), DoorState::Open);
    assert_eq!(state(world, b), DoorState::Closed, "its partner opened");
    assert_eq!(state(world, g2), DoorState::Closed, "its group opened");
}

#[test]
fn plasteel_goes_on_and_comes_off_in_steps() {
    let mut app = testing::server_app(fixture("plating"), |_| {});
    let world = app.world_mut();
    let door = door_at(world, N);
    let user = world.spawn(USER).id();
    world.get_mut::<Door>(door).unwrap().panel_open = true;
    let sheets = spawn_stack(world, "/obj/item/stack/sheet/plasteel", SHEETS_AT, 5).unwrap();
    world.entity_mut(sheets).insert(CarriedBy(user));

    let complete = ss13_game::airlock_security::complete;
    complete(
        world,
        user,
        door,
        DoorJob::Reinforce {
            sheet: sheets,
            level: PLASTEEL,
        },
    );
    assert_eq!(world.get::<Door>(door).unwrap().security_level, PLASTEEL);
    assert_eq!(ss13_game::tools::stack_amount(world, sheets), 3);

    // A step that is not the current one does nothing.
    complete(world, user, door, DoorJob::Shield { from: PLASTEEL_O });
    assert_eq!(world.get::<Door>(door).unwrap().security_level, PLASTEEL);
    for from in [6, 5, 4, 3, 2] {
        complete(world, user, door, DoorJob::Shield { from });
    }
    assert_eq!(world.get::<Door>(door).unwrap().security_level, NONE);
    let plasteel: u32 = world
        .resource::<SpatialIndex>()
        .on_tile(USER)
        .iter()
        .copied()
        .filter(|&e| {
            world
                .get::<DmItem>(e)
                .is_some_and(|i| i.path == "/obj/item/stack/sheet/plasteel")
        })
        .map(|e| ss13_game::tools::stack_amount(world, e))
        .sum();
    assert_eq!(plasteel, 2, "one sheet back from each crowbar layer");
}

#[test]
fn access_button_opens_its_door_through_the_console() {
    let mut app = testing::server_app(fixture("button"), |_| {});
    let world = app.world_mut();
    let (interior, exterior) = (door_at(world, I), door_at(world, E));
    assert_eq!(world.get::<DmDoor>(exterior).unwrap().id_tag, "out");
    let console = first(world, C, |w, e| w.get::<AccessControl>(e).is_some());
    let button = first(world, K, |w, e| w.get::<AccessControl>(e).is_some());
    match &world.get::<AccessControl>(console).unwrap().kind {
        AccessKind::Console {
            interior: i,
            exterior: e,
            ..
        } => assert_eq!((*i, *e), (Some(interior), Some(exterior))),
        other => panic!("not a console: {other:?}"),
    }
    let user = world.spawn(USER).id();

    // No ID: no virology access.
    ss13_game::airlock_controllers::attack_hand(world, user, user, button);
    run(&mut app, 1.0);
    assert_eq!(state(app.world(), exterior), DoorState::Closed);

    let world = app.world_mut();
    assert!(ss13_game::airlock_controllers::emag_act(
        world, user, button
    ));
    ss13_game::airlock_controllers::attack_hand(world, user, user, button);
    run(&mut app, 1.0);
    let world = app.world();
    assert_eq!(state(world, exterior), DoorState::Open);
    assert!(world.get::<Door>(exterior).unwrap().locked, "bolted open");
    assert_eq!(state(world, interior), DoorState::Closed);
}

#[test]
fn door_remote_works_the_doors_its_access_fits() {
    let mut app = testing::server_app(fixture("remote"), |_| {});
    let world = app.world_mut();
    let (eng, sec) = (door_at(world, N), door_at(world, S));
    let user = world.spawn(USER).id();
    let remote = spawn_obj(world, "/obj/item/door_remote/chief_engineer", USER).unwrap();

    use_on(world, user, remote, sec);
    use_on(world, user, remote, eng);
    run(&mut app, 1.0);
    let world = app.world_mut();
    assert_eq!(state(world, sec), DoorState::Closed, "no security access");
    assert_eq!(state(world, eng), DoorState::Open);

    world.get_mut::<DoorRemote>(remote).unwrap().mode = Mode::Bolt;
    use_on(world, user, remote, eng);
    assert!(world.get::<Door>(eng).unwrap().locked);
    world.get_mut::<DoorRemote>(remote).unwrap().mode = Mode::Emergency;
    use_on(world, user, remote, eng);
    assert!(world.get::<Door>(eng).unwrap().emergency);
}
