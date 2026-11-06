//! Door structures on a fixture room (`ss13_game::door_structures`, `ss13_game::doors`): a
//! firelock frame given its circuit, reinforced and wrenched into a heavy firelock, that
//! firelock taken back apart into its frame and the frame cut into its sheets; a windoor
//! assembly anchored, made secure, wired, given electronics and pried into a brig windoor with
//! their access; a loose assembly flipped and welded apart; a mineral door opened and dug into
//! its sheets; the hostile lockdown of an airlock and its end; a password door typed and a
//! voice door spoken open.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::airlock_construction::{AirlockElectronics, ELECTRONICS};
use ss13_game::construction::{Job, spawn_obj, spawn_stack};
use ss13_game::door_structures::{
    DoorBuildJob, FIRELOCK_ELECTRONICS, FIRELOCK_FRAME, FiredoorBoltsUnlocked, FirelockFrame,
    MINERAL_DOOR, MineralDoorMoving, NO_CIRCUIT, PANEL_OPEN, WINDOOR_ASSEMBLY, WindoorAssembly,
    complete, finish_switch, try_switch,
};
use ss13_game::doors::{Door, DoorState, disable_lockdown, hostile_lockdown};
use ss13_game::obj_damage::Anchored;
use ss13_game::password_door::{hear_line, try_password};
use ss13_game::testing::{self, write_fixture};
use ss13_game::tools::DoAfter;
use ss13_game::wallmount::firelock::Firelock;
use ss13_game::wires::Electrified;
use ss13_mapload::{DmDoor, DmItem, DmObj, DoorKind, MapLoadPlugin, Opaque};
use ss13_shared::TilePos;
use ss13_world::{Density, SpatialIndex};

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/datum/armor": { "parent": "/datum", "declared_vars": { "melee": { "expr": "0" } } },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 }, "declared_vars": { "max_integrity": 500,
        "armor_type": { "expr": "/datum/armor" }, "integrity_failure": 0,
        "damage_deflection": 0, "resistance_flags": 0, "flags_1": 0 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3, "anchored": 0 } },
    "/obj": { "parent": "/atom/movable", "declared_vars": { "obj_flags": 2 } },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start" },
    "/obj/item": { "parent": "/obj", "declared_vars": { "icon": { "resource": "i.dmi" },
        "icon_state": "toy", "interaction_flags_item": 1, "tool_behaviour": null,
        "toolspeed": 1, "heat": 0 } },
    "/obj/item/electronics": { "parent": "/obj/item" },
    "/obj/item/electronics/airlock": { "parent": "/obj/item/electronics", "declared_vars": {
        "name": "airlock electronics", "icon_state": "door_electronics" } },
    "/obj/item/electronics/firelock": { "parent": "/obj/item/electronics", "declared_vars": {
        "name": "firelock circuitry", "icon_state": "mainboard" } },
    "/obj/item/stack": { "parent": "/obj/item", "declared_vars": { "amount": 1,
        "max_amount": 50, "merge_type": null, "singular_name": null, "turf_type": null } },
    "/obj/item/stack/rods": { "parent": "/obj/item/stack", "declared_vars": {
        "name": "rods", "icon_state": "rods",
        "merge_type": { "path": "/obj/item/stack/rods" } } },
    "/obj/item/stack/cable_coil": { "parent": "/obj/item/stack", "declared_vars": {
        "name": "cable", "icon_state": "coil",
        "merge_type": { "path": "/obj/item/stack/cable_coil" } } },
    "/obj/item/stack/sheet": { "parent": "/obj/item/stack" },
    "/obj/item/stack/sheet/iron": { "parent": "/obj/item/stack/sheet", "declared_vars": {
        "name": "iron", "icon_state": "sheet-metal",
        "merge_type": { "path": "/obj/item/stack/sheet/iron" } } },
    "/obj/item/stack/sheet/plasteel": { "parent": "/obj/item/stack/sheet", "declared_vars": {
        "name": "plasteel", "icon_state": "sheet-plasteel",
        "merge_type": { "path": "/obj/item/stack/sheet/plasteel" } } },
    "/obj/item/stack/sheet/rglass": { "parent": "/obj/item/stack/sheet", "declared_vars": {
        "name": "reinforced glass", "icon_state": "sheet-rglass",
        "merge_type": { "path": "/obj/item/stack/sheet/rglass" } } },
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
    "/obj/structure": { "parent": "/obj", "declared_vars": { "anchored": 1,
        "max_integrity": 300 } },
    "/obj/structure/firelock_frame": { "parent": "/obj/structure", "declared_vars": {
        "name": "firelock frame", "icon": { "resource": "f.dmi" }, "icon_state": "frame1",
        "anchored": 0, "density": 1 } },
    "/obj/structure/firelock_frame/heavy": { "parent": "/obj/structure/firelock_frame" },
    "/obj/structure/windoor_assembly": { "parent": "/obj/structure", "declared_vars": {
        "name": "windoor Assembly", "icon": { "resource": "w.dmi" },
        "icon_state": "l_windoor_assembly01", "anchored": 0, "density": 0, "dir": 1 } },
    "/obj/structure/mineral_door": { "parent": "/obj/structure", "declared_vars": {
        "name": "iron door", "icon": { "resource": "m.dmi" }, "icon_state": "metal",
        "density": 1, "opacity": 1, "layer": 3.1 } },
    "/obj/structure/mineral_door/iron": { "parent": "/obj/structure/mineral_door" },
    "/obj/machinery": { "parent": "/obj" },
    "/obj/machinery/door": { "parent": "/obj/machinery", "declared_vars": {
        "icon": { "resource": "d.dmi" }, "icon_state": "door_closed", "density": 1, "opacity": 1,
        "layer": 2.7, "base_icon_state": "door", "autoclose": 0, "can_open_with_hands": 1,
        "closingLayer": 3.1, "glass": 0, "locked": 0, "safe": 1, "visible": 1, "welded": 0 } },
    "/obj/machinery/door/airlock": { "parent": "/obj/machinery/door", "declared_vars": {
        "name": "airlock", "icon": { "resource": "a.dmi" }, "icon_state": "closed",
        "autoclose": 1, "base_icon_state": null, "normalspeed": 1,
        "overlays_file": { "resource": "o.dmi" }, "airlock_material": null } },
    "/obj/machinery/door/firedoor": { "parent": "/obj/machinery/door", "declared_vars": {
        "name": "firelock", "icon": { "resource": "f.dmi" }, "icon_state": "door_open",
        "density": 0, "opacity": 0, "base_icon_state": "door", "can_open_with_hands": 0 } },
    "/obj/machinery/door/firedoor/heavy": { "parent": "/obj/machinery/door/firedoor" },
    "/obj/machinery/door/password": { "parent": "/obj/machinery/door", "declared_vars": {
        "name": "door", "icon": { "resource": "b.dmi" }, "icon_state": "closed",
        "password": "Swordfish", "interaction_activated": 1, "voice_activated": 0 } },
    "/obj/machinery/door/password/voice": { "parent": "/obj/machinery/door/password",
        "declared_vars": { "voice_activated": 1 } },
    "/obj/machinery/door/window": { "parent": "/obj/machinery/door", "declared_vars": {
        "name": "interior door", "icon": { "resource": "w.dmi" }, "icon_state": "left",
        "base_state": "left", "opacity": 0, "visible": 0, "dir": 1 } },
    "/obj/machinery/door/window/brigdoor": { "parent": "/obj/machinery/door/window",
        "declared_vars": { "base_state": "leftsecure", "icon_state": "leftsecure" } },
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

const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"a" = (/obj/machinery/door/airlock,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwwww
wsfffaw
wfffffw
wwwwwww
"}
"##;

const FRAME_AT: TilePos = TilePos::new(3, 2, 1);
const WINDOOR_AT: TilePos = TilePos::new(4, 2, 1);
const MINERAL_AT: TilePos = TilePos::new(5, 2, 1);
const LOOSE_AT: TilePos = TilePos::new(3, 3, 1);
const AIRLOCK_AT: TilePos = TilePos::new(6, 3, 1);

/// Manifest order = `IconStateId`.
const STATES: [(&str, &str); 24] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
    ("i.dmi", "door_electronics"),
    ("i.dmi", "mainboard"),
    ("i.dmi", "sheet-metal"),
    ("f.dmi", "frame1"),
    ("f.dmi", "frame2"),
    ("f.dmi", "door_open"),
    ("f.dmi", "door_closed"),
    ("w.dmi", "l_windoor_assembly01"),
    ("w.dmi", "r_windoor_assembly01"),
    ("w.dmi", "l_secure_windoor_assembly02"),
    ("w.dmi", "leftsecure"),
    ("m.dmi", "metal"),
    ("m.dmi", "metalopening"),
    ("m.dmi", "metalopen"),
    ("a.dmi", "closed"),
    ("a.dmi", "opening"),
    ("a.dmi", "open"),
    ("b.dmi", "closed"),
    ("b.dmi", "opening"),
];

fn fixture(test: &str) -> MapLoadPlugin {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("door_structures_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/item/electronics/airlock",
        "/obj/item/electronics/firelock",
        "/obj/item/stack/rods",
        "/obj/item/stack/cable_coil",
        "/obj/item/stack/sheet/iron",
        "/obj/item/stack/sheet/plasteel",
        "/obj/item/stack/sheet/rglass",
        "/obj/structure/firelock_frame",
        "/obj/structure/firelock_frame/heavy",
        "/obj/structure/windoor_assembly",
        "/obj/structure/mineral_door",
        "/obj/structure/mineral_door/iron",
        "/obj/machinery/door/airlock",
        "/obj/machinery/door/firedoor",
        "/obj/machinery/door/firedoor/heavy",
        "/obj/machinery/door/window",
        "/obj/machinery/door/window/brigdoor",
        "/obj/machinery/door/password",
        "/obj/machinery/door/password/voice",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/area/space",
    ];
    let assets: PathBuf = write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)]);
    testing::fixture_map(&assets, &["map.dmm"])
}

/// Runs `job` to its end as a finished `do_after` of `user` on `target`.
fn run(world: &mut World, user: Entity, target: Entity, item: Option<Entity>, job: DoorBuildJob) {
    let pos = *world.get::<TilePos>(target).unwrap();
    let amount = match job {
        DoorBuildJob::FrameReinforce | DoorBuildJob::WindoorReinforce => 2,
        DoorBuildJob::WindoorWire => 1,
        _ => 0,
    };
    let action = DoAfter {
        client: user,
        target,
        target_pos: pos,
        user_pos: pos,
        item,
        amount,
        ends_at: 0.0,
        job: Job::DoorBuild(job),
    };
    complete(world, user, &action, job);
}

fn on_tile(world: &World, pos: TilePos, f: impl Fn(&World, Entity) -> bool) -> Vec<Entity> {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .filter(|&e| f(world, e))
        .collect()
}

fn obj_path(world: &World, e: Entity) -> String {
    world
        .get::<DmObj>(e)
        .map(|o| o.path.clone())
        .or_else(|| world.get::<DmItem>(e).map(|i| i.path.clone()))
        .unwrap_or_default()
}

fn sheets(world: &World, pos: TilePos, path: &str) -> u32 {
    on_tile(world, pos, |w, e| obj_path(w, e) == path)
        .into_iter()
        .map(|e| ss13_game::tools::stack_amount(world, e))
        .sum()
}

#[test]
fn a_firelock_frame_becomes_a_heavy_firelock_and_comes_apart_again() {
    let mut app = testing::server_app(fixture("firelock"), |_| {});
    let world = app.world_mut();
    let user = world.spawn(FRAME_AT).id();
    let frame = spawn_obj(world, FIRELOCK_FRAME, FRAME_AT).expect("frame prototype");

    let circuit = spawn_obj(world, FIRELOCK_ELECTRONICS, FRAME_AT).unwrap();
    run(
        world,
        user,
        frame,
        Some(circuit),
        DoorBuildJob::FrameCircuit,
    );
    assert!(world.get_entity(circuit).is_err(), "the circuit went in");
    assert_eq!(world.get::<FirelockFrame>(frame).unwrap().step, PANEL_OPEN);

    let plasteel = spawn_stack(world, "/obj/item/stack/sheet/plasteel", FRAME_AT, 2).unwrap();
    run(
        world,
        user,
        frame,
        Some(plasteel),
        DoorBuildJob::FrameReinforce,
    );
    assert!(world.get::<FirelockFrame>(frame).unwrap().reinforced);
    assert!(world.get_entity(plasteel).is_err(), "both sheets used");

    run(world, user, frame, None, DoorBuildJob::FrameFinish);
    assert!(world.get_entity(frame).is_err(), "the frame is gone");
    let doors = on_tile(world, FRAME_AT, |w, e| w.get::<Door>(e).is_some());
    assert_eq!(doors.len(), 1, "one firelock built");
    let door = doors[0];
    assert_eq!(obj_path(world, door), "/obj/machinery/door/firedoor/heavy");
    assert_eq!(world.get::<Door>(door).unwrap().kind, DoorKind::Firedoor);
    assert!(world.get::<Firelock>(door).is_some(), "it follows alarms");

    // Taken apart: welded, its bolts unscrewed, wrenched.
    run(world, user, door, None, DoorBuildJob::FiredoorUnbolt);
    assert!(world.get_entity(door).is_ok(), "not welded: the bolts hold");
    world.get_mut::<Door>(door).unwrap().welded = true;
    run(world, user, door, None, DoorBuildJob::FiredoorUnbolt);
    assert!(world.get_entity(door).is_ok(), "the screws lock the bolts");
    world.entity_mut(door).insert(FiredoorBoltsUnlocked);
    run(world, user, door, None, DoorBuildJob::FiredoorUnbolt);
    assert!(world.get_entity(door).is_err(), "the firelock is gone");
    let frames = on_tile(world, FRAME_AT, |w, e| w.get::<FirelockFrame>(e).is_some());
    assert_eq!(frames.len(), 1);
    let frame = frames[0];
    assert_eq!(
        obj_path(world, frame),
        "/obj/structure/firelock_frame/heavy"
    );
    assert_eq!(
        world.get::<FirelockFrame>(frame),
        Some(&FirelockFrame {
            step: PANEL_OPEN,
            reinforced: true
        })
    );

    run(world, user, frame, None, DoorBuildJob::FrameUncircuit);
    assert_eq!(world.get::<FirelockFrame>(frame).unwrap().step, NO_CIRCUIT);
    assert_eq!(
        on_tile(world, FRAME_AT, |w, e| obj_path(w, e)
            == FIRELOCK_ELECTRONICS)
        .len(),
        1,
        "the circuit came out"
    );
    run(world, user, frame, None, DoorBuildJob::FrameCut);
    assert!(world.get_entity(frame).is_err());
    assert_eq!(sheets(world, FRAME_AT, "/obj/item/stack/sheet/iron"), 3);
    assert_eq!(sheets(world, FRAME_AT, "/obj/item/stack/sheet/plasteel"), 2);
}

#[test]
fn a_windoor_assembly_becomes_a_brig_windoor_with_its_electronics_access() {
    let mut app = testing::server_app(fixture("windoor"), |_| {});
    let world = app.world_mut();
    let user = world.spawn(WINDOOR_AT).id();
    let assembly = spawn_obj(world, WINDOOR_ASSEMBLY, WINDOOR_AT).expect("assembly prototype");

    // Not anchored: no wiring.
    let cable = spawn_stack(world, "/obj/item/stack/cable_coil", WINDOOR_AT, 5).unwrap();
    run(
        world,
        user,
        assembly,
        Some(cable),
        DoorBuildJob::WindoorWire,
    );
    assert!(!world.get::<WindoorAssembly>(assembly).unwrap().cables_added);

    run(
        world,
        user,
        assembly,
        None,
        DoorBuildJob::WindoorAnchor { anchored: false },
    );
    assert!(world.get::<Anchored>(assembly).is_some_and(|a| a.0));
    let plasteel = spawn_stack(world, "/obj/item/stack/sheet/plasteel", WINDOOR_AT, 2).unwrap();
    run(
        world,
        user,
        assembly,
        Some(plasteel),
        DoorBuildJob::WindoorReinforce,
    );
    run(
        world,
        user,
        assembly,
        Some(cable),
        DoorBuildJob::WindoorWire,
    );
    let a = world.get::<WindoorAssembly>(assembly).unwrap().clone();
    assert!(a.secure && a.cables_added);

    let electronics = spawn_obj(world, ELECTRONICS, WINDOOR_AT).unwrap();
    world.entity_mut(electronics).insert(AirlockElectronics {
        accesses: vec!["brig".into()],
        one_access: false,
        // North and east: only north runs along a windoor facing south.
        unres_sides: 1 | 4,
        passed_name: "Cell 1".into(),
    });
    run(
        world,
        user,
        assembly,
        Some(electronics),
        DoorBuildJob::WindoorElectronics,
    );
    assert!(
        world.get_entity(electronics).is_err(),
        "the electronics went in"
    );

    run(world, user, assembly, None, DoorBuildJob::WindoorFinish);
    assert!(world.get_entity(assembly).is_err(), "the assembly is gone");
    let doors = on_tile(world, WINDOOR_AT, |w, e| w.get::<Door>(e).is_some());
    assert_eq!(doors.len(), 1, "one windoor built");
    let door = doors[0];
    assert_eq!(obj_path(world, door), "/obj/machinery/door/window/brigdoor");
    assert_eq!(
        world.get::<DmDoor>(door).unwrap().base_icon_state,
        "leftsecure"
    );
    let d = world.get::<Door>(door).unwrap();
    assert_eq!(d.kind, DoorKind::Windoor);
    assert_eq!(d.req_access, vec!["brig".to_owned()]);
    assert_eq!(d.unres_sides, 1);
    assert!(d.unres_latch && d.autoclose);
    assert!(d.allows(Some(&["brig".to_owned()])));
    assert!(!d.allows(None));
}

#[test]
fn a_loose_windoor_assembly_flips_and_welds_apart() {
    let mut app = testing::server_app(fixture("windoor_apart"), |_| {});
    let world = app.world_mut();
    let user = world.spawn(LOOSE_AT).id();
    let assembly = spawn_obj(world, WINDOOR_ASSEMBLY, LOOSE_AT).unwrap();
    let plasteel = spawn_stack(world, "/obj/item/stack/sheet/plasteel", LOOSE_AT, 2).unwrap();
    run(
        world,
        user,
        assembly,
        Some(plasteel),
        DoorBuildJob::WindoorReinforce,
    );
    assert!(world.get::<WindoorAssembly>(assembly).unwrap().secure);
    run(world, user, assembly, None, DoorBuildJob::WindoorApart);
    assert!(world.get_entity(assembly).is_err());
    assert_eq!(sheets(world, LOOSE_AT, "/obj/item/stack/sheet/rglass"), 5);
    assert_eq!(sheets(world, LOOSE_AT, "/obj/item/stack/rods"), 4);
}

#[test]
fn a_mineral_door_opens_by_hand_and_digs_into_its_sheets() {
    let mut app = testing::server_app(fixture("mineral"), |_| {});
    let world = app.world_mut();
    let user = world.spawn(MINERAL_AT).id();
    let door = spawn_obj(world, "/obj/structure/mineral_door/iron", MINERAL_AT).unwrap();
    // The `Anchored` observer runs on the next flush.
    world.flush();
    assert!(obj_path(world, door).starts_with(MINERAL_DOOR));

    assert!(try_switch(world, door, user));
    assert!(
        world
            .get::<MineralDoorMoving>(door)
            .is_some_and(|m| m.opening)
    );
    assert!(world.get::<Opaque>(door).is_none(), "sight passes at once");
    assert!(
        world.get::<Density>(door).is_some(),
        "still blocking for a second"
    );
    assert!(
        try_switch(world, door, user),
        "a moving door is still a door"
    );
    finish_switch(world, door);
    assert!(world.get::<Density>(door).is_none(), "open");
    assert!(world.get::<MineralDoorMoving>(door).is_none());

    run(world, user, door, None, DoorBuildJob::MineralDig);
    assert!(world.get_entity(door).is_err(), "dug out");
    assert_eq!(sheets(world, MINERAL_AT, "/obj/item/stack/sheet/iron"), 20);
}

#[test]
fn hostile_lockdown_bolts_and_shocks_an_airlock_until_it_ends() {
    let mut app = testing::server_app(fixture("lockdown"), |_| {});
    let world = app.world_mut();
    let airlock = on_tile(world, AIRLOCK_AT, |w, e| w.get::<Door>(e).is_some())[0];
    world
        .entity_mut(airlock)
        .remove::<ss13_game::power::Unpowered>();

    assert!(hostile_lockdown(world, airlock));
    let d = world.get::<Door>(airlock).unwrap();
    assert!(d.locked && !d.safe, "bolted and crushing");
    assert_eq!(
        world.get::<Electrified>(airlock),
        Some(&Electrified::Permanent)
    );

    assert!(disable_lockdown(world, airlock));
    let d = world.get::<Door>(airlock).unwrap();
    assert!(!d.locked && d.safe);
    assert!(matches!(d.state, DoorState::Opening { .. }), "it opens");
    assert!(world.get::<Electrified>(airlock).is_none());

    // An unpowered door takes no part.
    world
        .entity_mut(airlock)
        .insert(ss13_game::power::Unpowered);
    assert!(!hostile_lockdown(world, airlock));
    assert!(!world.get::<Door>(airlock).unwrap().locked);
}

#[test]
fn a_password_door_opens_only_on_its_password() {
    let mut app = testing::server_app(fixture("password"), |_| {});
    let world = app.world_mut();
    let door = spawn_obj(world, "/obj/machinery/door/password", MINERAL_AT).unwrap();
    assert!(ss13_game::doors::setup_built(world, door));
    let dm = world.get::<DmDoor>(door).unwrap();
    assert_eq!(dm.password, "Swordfish");
    assert!(dm.interaction_activated && !dm.voice_activated);
    assert!(world.get::<Door>(door).unwrap().password_lock);

    let user = world.spawn(WINDOOR_AT).id();
    let far = world.spawn(LOOSE_AT).id();
    assert!(
        !try_password(world, user, door, "swordfish"),
        "typed exactly"
    );
    assert_eq!(world.get::<Door>(door).unwrap().state, DoorState::Closed);
    assert!(
        !try_password(world, far, door, "Swordfish"),
        "from out of reach"
    );
    assert!(try_password(world, user, door, "Swordfish"));
    assert!(matches!(
        world.get::<Door>(door).unwrap().state,
        DoorState::Opening { .. }
    ));

    // A voice door hears the password in a line, ignoring case; the typed one does not.
    let voice = spawn_obj(world, "/obj/machinery/door/password/voice", FRAME_AT).unwrap();
    assert!(ss13_game::doors::setup_built(world, voice));
    hear_line(world, AIRLOCK_AT, "the word is not here", false);
    assert_eq!(world.get::<Door>(voice).unwrap().state, DoorState::Closed);
    hear_line(world, AIRLOCK_AT, "Open up, SWORDFISH!", true);
    assert_eq!(
        world.get::<Door>(voice).unwrap().state,
        DoorState::Closed,
        "a whisper carries one tile"
    );
    hear_line(world, AIRLOCK_AT, "Open up, SWORDFISH!", false);
    assert!(matches!(
        world.get::<Door>(voice).unwrap().state,
        DoorState::Opening { .. }
    ));
}
