//! Airlock construction on a fixture room (`ss13_game::airlock_construction`): an assembly
//! anchored, wired, given electronics set to "one of engineering" with the north side
//! unrestricted, glassed and finished into a glass airlock with that access and name; a loose
//! assembly welded apart into its iron; the painter repaints an airlock.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::airlock_construction::{
    ASSEMBLY, AirlockElectronics, AssemblyJob, DoorAssembly, ELECTRONICS, NEEDS_SCREWDRIVER,
    PainterInk, complete, paint,
};
use ss13_game::construction::{Job, spawn_obj};
use ss13_game::doors::Door;
use ss13_game::obj_damage::Anchored;
use ss13_game::testing::{self, write_fixture};
use ss13_game::tools::DoAfter;
use ss13_mapload::{DmAtom, DmDoor, DmItem, DmName, DmObj, MapLoadPlugin};
use ss13_shared::TilePos;
use ss13_world::SpatialIndex;

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
        "damage_deflection": 0, "resistance_flags": 0 } },
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
    "/obj/item/airlock_painter": { "parent": "/obj/item", "declared_vars": {
        "name": "airlock painter", "icon_state": "paint_sprayer" } },
    "/obj/item/stack": { "parent": "/obj/item", "declared_vars": { "amount": 1,
        "max_amount": 50, "merge_type": null, "singular_name": null, "turf_type": null } },
    "/obj/item/stack/sheet": { "parent": "/obj/item/stack" },
    "/obj/item/stack/sheet/iron": { "parent": "/obj/item/stack/sheet", "declared_vars": {
        "name": "iron", "icon_state": "sheet-metal",
        "merge_type": { "path": "/obj/item/stack/sheet/iron" } } },
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
    "/obj/structure/door_assembly": { "parent": "/obj/structure", "declared_vars": {
        "name": "airlock assembly", "icon": { "resource": "a.dmi" },
        "icon_state": "construction", "overlays_file": { "resource": "o.dmi" },
        "anchored": 0, "density": 1, "state": 0, "base_name": "Airlock",
        "created_name": null, "mineral": null, "multi_tile": 0,
        "airlock_type": { "path": "/obj/machinery/door/airlock" },
        "glass_type": { "path": "/obj/machinery/door/airlock/glass" },
        "glass": 0, "heat_proof_finished": 0,
        "previous_assembly": { "path": "/obj/structure/door_assembly" },
        "noglass": 0, "nomineral": 0,
        "material_type": { "path": "/obj/item/stack/sheet/iron" }, "material_amt": 4 } },
    "/obj/structure/door_assembly/door_assembly_eng": { "parent": "/obj/structure/door_assembly",
        "declared_vars": { "icon": { "resource": "e.dmi" }, "base_name": "engineering airlock",
        "airlock_type": { "path": "/obj/machinery/door/airlock/engineering" } } },
    "/obj/machinery": { "parent": "/obj" },
    "/obj/machinery/door": { "parent": "/obj/machinery", "declared_vars": {
        "icon": { "resource": "d.dmi" }, "icon_state": "door_closed", "density": 1, "opacity": 1,
        "layer": 2.7, "base_icon_state": "door", "autoclose": 0, "can_open_with_hands": 1,
        "closingLayer": 3.1, "glass": 0, "locked": 0, "safe": 1, "visible": 1, "welded": 0 } },
    "/obj/machinery/door/airlock": { "parent": "/obj/machinery/door", "declared_vars": {
        "name": "airlock", "icon": { "resource": "a.dmi" }, "icon_state": "closed",
        "autoclose": 1, "base_icon_state": null, "normalspeed": 1,
        "overlays_file": { "resource": "o.dmi" }, "airlock_material": null,
        "assemblytype": { "path": "/obj/structure/door_assembly" } } },
    "/obj/machinery/door/airlock/glass": { "parent": "/obj/machinery/door/airlock",
        "declared_vars": { "glass": 1, "opacity": 0 } },
    "/obj/machinery/door/airlock/engineering": { "parent": "/obj/machinery/door/airlock",
        "declared_vars": { "icon": { "resource": "e.dmi" },
        "assemblytype": { "path": "/obj/structure/door_assembly/door_assembly_eng" } } },
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

(1,1,1) = {"
wwwwww
wsfffw
wffffw
wwwwww
"}
"##;

const BUILD_AT: TilePos = TilePos::new(3, 3, 1);
const LOOSE_AT: TilePos = TilePos::new(4, 2, 1);

/// Manifest order = `IconStateId`.
const STATES: [(&str, &str); 22] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
    ("i.dmi", "door_electronics"),
    ("i.dmi", "sheet-metal"),
    ("i.dmi", "paint_sprayer"),
    ("a.dmi", "construction"),
    ("a.dmi", "fill_construction"),
    ("o.dmi", "glass_construction"),
    ("o.dmi", "panel_c1"),
    ("o.dmi", "panel_c2"),
    ("o.dmi", "panel_c3"),
    ("a.dmi", "closed"),
    ("a.dmi", "opening"),
    ("a.dmi", "open"),
    ("a.dmi", "closing"),
    ("e.dmi", "closed"),
    ("e.dmi", "opening"),
    ("e.dmi", "open"),
    ("e.dmi", "closing"),
];

fn fixture(test: &str) -> MapLoadPlugin {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("airlock_construction_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/item/electronics/airlock",
        "/obj/item/airlock_painter",
        "/obj/item/stack/sheet/iron",
        "/obj/structure/door_assembly",
        "/obj/structure/door_assembly/door_assembly_eng",
        "/obj/machinery/door/airlock",
        "/obj/machinery/door/airlock/glass",
        "/obj/machinery/door/airlock/engineering",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/area/space",
    ];
    let assets: PathBuf = write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)]);
    testing::fixture_map(&assets, &["map.dmm"])
}

/// Runs `job` to its end as a finished `do_after` of `user` on `target`.
fn run(world: &mut World, user: Entity, target: Entity, item: Option<Entity>, job: AssemblyJob) {
    let pos = *world.get::<TilePos>(target).unwrap();
    let action = DoAfter {
        client: user,
        target,
        target_pos: pos,
        user_pos: pos,
        item,
        amount: 0,
        ends_at: 0.0,
        job: Job::Airlock(job),
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

#[test]
fn an_assembly_becomes_a_glass_airlock_with_its_electronics_access() {
    let mut app = testing::server_app(fixture("build"), |_| {});
    let world = app.world_mut();
    let user = world.spawn(BUILD_AT).id();
    let assembly = spawn_obj(world, ASSEMBLY, BUILD_AT).expect("assembly prototype");

    run(
        world,
        user,
        assembly,
        None,
        AssemblyJob::Anchor { anchored: false },
    );
    assert!(world.get::<Anchored>(assembly).is_some_and(|a| a.0));
    run(world, user, assembly, None, AssemblyJob::Wire);

    let electronics = spawn_obj(world, ELECTRONICS, BUILD_AT).unwrap();
    world.entity_mut(electronics).insert(AirlockElectronics {
        accesses: vec!["engineering".into()],
        one_access: true,
        unres_sides: 1,
        passed_name: "Engine Room".into(),
    });
    run(
        world,
        user,
        assembly,
        Some(electronics),
        AssemblyJob::InstallElectronics,
    );
    assert!(
        world.get_entity(electronics).is_err(),
        "the electronics went in"
    );
    assert_eq!(
        world.get::<DoorAssembly>(assembly).unwrap().state,
        NEEDS_SCREWDRIVER
    );
    run(
        world,
        user,
        assembly,
        None,
        AssemblyJob::Glass { reinforced: false },
    );
    assert!(world.get::<DoorAssembly>(assembly).unwrap().glass);

    run(world, user, assembly, None, AssemblyJob::Finish);
    assert!(world.get_entity(assembly).is_err(), "the assembly is gone");
    let doors = on_tile(world, BUILD_AT, |w, e| w.get::<Door>(e).is_some());
    assert_eq!(doors.len(), 1, "one airlock built");
    let door = doors[0];
    assert_eq!(
        world.get::<DmObj>(door).unwrap().path,
        "/obj/machinery/door/airlock/glass"
    );
    assert_eq!(world.get::<DmName>(door).unwrap().name, "Engine Room");
    let d = world.get::<Door>(door).unwrap();
    assert_eq!(d.req_one_access, vec!["engineering".to_owned()]);
    assert!(d.req_access.is_empty());
    assert_eq!(d.unres_sides, 1);
    assert!(d.unres_latch && d.autoclose);
    assert!(d.allows(Some(&["engineering".to_owned()])));
    assert!(!d.allows(None));
    assert!(world.get::<ss13_game::wires::Wires>(door).is_some());

    // The painter: the engineering paint job takes that airlock type's icon.
    let painter = spawn_obj(world, "/obj/item/airlock_painter", BUILD_AT).unwrap();
    assert!(paint(
        world,
        user,
        painter,
        door,
        "/obj/machinery/door/airlock/engineering"
    ));
    assert_eq!(world.get::<DmAtom>(door).unwrap().icon, "e.dmi");
    assert_eq!(
        world.get::<DmObj>(door).unwrap().assembly_type,
        "/obj/structure/door_assembly/door_assembly_eng"
    );
    assert_eq!(world.get::<PainterInk>(painter), Some(&PainterInk(4)));
    assert_eq!(world.get::<DmDoor>(door).unwrap().overlays_file, "o.dmi");
}

#[test]
fn a_loose_assembly_welds_apart_into_its_iron() {
    let mut app = testing::server_app(fixture("apart"), |_| {});
    let world = app.world_mut();
    let user = world.spawn(LOOSE_AT).id();
    let assembly = spawn_obj(world, ASSEMBLY, LOOSE_AT).unwrap();
    run(world, user, assembly, None, AssemblyJob::Disassemble);
    assert!(world.get_entity(assembly).is_err());
    let iron: u32 = on_tile(world, LOOSE_AT, |w, e| {
        w.get::<DmItem>(e)
            .is_some_and(|i| i.path == "/obj/item/stack/sheet/iron")
    })
    .into_iter()
    .map(|e| ss13_game::tools::stack_amount(world, e))
    .sum();
    assert_eq!(iron, 4);
}
