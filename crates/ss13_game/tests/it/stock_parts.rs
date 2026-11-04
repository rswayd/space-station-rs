//! Stock parts, RPED and the autolathe on a fixture room without clients
//! (`ss13_game::stock_parts`, `ss13_game::science::autolathe`): a mapped recharger's parts come
//! out of thin air as items the first time they are needed, an RPED swaps its capacitor for a
//! better one only with the panel open (and the recharger charges faster), deconstruction gives
//! the very same items back, an RPED fills a wired frame with a board and parts, the RPED storage
//! refuses what is not a part; the autolathe takes sheets up to its matter bins, prints a design
//! with its servo's efficiency, refuses hacked designs until hacked and drops its sheets when
//! taken apart.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::construction::{PanelOpen, spawn_obj, spawn_stack};
use ss13_game::machine_frame::{FRAME_BOARD, FRAME_WIRED, Frame};
use ss13_game::mecha::core::MechaDb;
use ss13_game::obj_damage::ObjState;
use ss13_game::power::Unpowered;
use ss13_game::power::chargers::{cell_charger_rate, recharge_coeff};
use ss13_game::science::autolathe::{Autolathe, start_build};
use ss13_game::science::lathe::MaterialStore;
use ss13_game::stock_parts::{ComponentParts, PartOf, exchange_parts};
use ss13_game::storage::{Refusal, Stored, can_insert, put_inside};
use ss13_game::testing::{self, fixture_map, write_fixture};
use ss13_game::tools::Stack;
use ss13_mapload::convert::DmTypes;
use ss13_mapload::mecha::MechaData;
use ss13_mapload::{DmItem, DmObj};
use ss13_shared::TilePos;
use ss13_world::SpatialIndex;

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/datum/armor": { "parent": "/datum", "declared_vars": { "melee": { "expr": "0" } } },
    "/datum/storage": { "parent": "/datum", "declared_vars": {
        "max_slots": 7, "max_specific_storage": 3, "max_total_storage": 14 } },
    "/datum/storage/rped": { "parent": "/datum/storage", "declared_vars": {
        "max_slots": 400, "max_total_storage": 800 } },
    "/datum/material": { "parent": "/datum", "declared_vars": {
        "abstract_type": { "path": "/datum/material" }, "mat_flags": 0, "name": "material",
        "sheet_type": null, "ore_type": null, "color": null } },
    "/datum/material/iron": { "parent": "/datum/material", "declared_vars": { "mat_flags": 23,
        "name": "iron", "sheet_type": { "path": "/obj/item/stack/sheet/iron" } } },
    "/datum/material/glass": { "parent": "/datum/material", "declared_vars": { "mat_flags": 23,
        "name": "glass", "sheet_type": { "path": "/obj/item/stack/sheet/glass" } } },
    "/datum/design": { "parent": "/datum", "declared_vars": { "name": null, "desc": null,
        "build_type": null, "build_path": null, "materials": { "list": [] },
        "category": { "list": [] }, "departmental_flags": 16777215,
        "construction_time": 32, "lathe_time_factor": 1, "fixed_cost_efficiency": 0 } },
    "/datum/design/wrench": { "parent": "/datum/design", "declared_vars": {
        "name": "Wrench", "build_type": 4, "build_path": { "path": "/obj/item/wrench" },
        "materials": { "list": [ { "assoc": [ { "path": "/datum/material/iron" }, 100 ] } ] },
        "category": { "list": [ "initial", "/Tools" ] } } },
    "/datum/design/crowbar": { "parent": "/datum/design", "declared_vars": {
        "name": "Crowbar", "build_type": 4, "build_path": { "path": "/obj/item/crowbar" },
        "materials": { "list": [ { "assoc": [ { "path": "/datum/material/iron" }, 100 ] } ] },
        "category": { "list": [ "hacked", "/Tools" ] } } },
    "/datum/stock_part": { "parent": "/datum" },
    "/datum/stock_part/capacitor": { "parent": "/datum/stock_part", "declared_vars": {
        "physical_object_type": { "path": "/obj/item/stock_parts/capacitor" } } },
    "/datum/stock_part/servo": { "parent": "/datum/stock_part", "declared_vars": {
        "physical_object_type": { "path": "/obj/item/stock_parts/servo" } } },
    "/datum/stock_part/matter_bin": { "parent": "/datum/stock_part", "declared_vars": {
        "physical_object_type": { "path": "/obj/item/stock_parts/matter_bin" } } },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 }, "declared_vars": { "can_atmos_pass": 1,
        "flags_1": 0, "max_integrity": 500, "armor_type": { "expr": "/datum/armor" },
        "integrity_failure": 0, "damage_deflection": 0, "resistance_flags": 0 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3, "anchored": 0 } },
    "/obj": { "parent": "/atom/movable", "declared_vars": { "force": 0, "damtype": "brute",
        "obj_flags": 2 } },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start" },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null, "w_class": 2,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null, "attack_speed": 8,
        "tool_behaviour": null, "toolspeed": 1, "heat": 0 } },
    "/obj/item/wrench": { "parent": "/obj/item", "declared_vars": { "name": "wrench",
        "icon_state": "wrench", "tool_behaviour": "wrench" } },
    "/obj/item/crowbar": { "parent": "/obj/item", "declared_vars": { "name": "crowbar",
        "icon_state": "crowbar", "tool_behaviour": "crowbar" } },
    "/obj/item/stock_parts": { "parent": "/obj/item", "declared_vars": { "rating": 1 } },
    "/obj/item/stock_parts/capacitor": { "parent": "/obj/item/stock_parts", "declared_vars": {
        "name": "capacitor", "icon_state": "capacitor" } },
    "/obj/item/stock_parts/capacitor/adv": { "parent": "/obj/item/stock_parts/capacitor",
        "declared_vars": { "name": "advanced capacitor", "rating": 2 } },
    "/obj/item/stock_parts/servo": { "parent": "/obj/item/stock_parts", "declared_vars": {
        "name": "micro-servo", "icon_state": "capacitor" } },
    "/obj/item/stock_parts/servo/nano": { "parent": "/obj/item/stock_parts/servo",
        "declared_vars": { "name": "nano-servo", "rating": 2 } },
    "/obj/item/stock_parts/matter_bin": { "parent": "/obj/item/stock_parts", "declared_vars": {
        "name": "matter bin", "icon_state": "capacitor" } },
    "/obj/item/storage": { "parent": "/obj/item", "declared_vars": {
        "storage_type": { "path": "/datum/storage" } } },
    "/obj/item/storage/part_replacer": { "parent": "/obj/item/storage", "declared_vars": {
        "name": "rapid part exchange device", "w_class": 5,
        "storage_type": { "path": "/datum/storage/rped" } } },
    "/obj/item/circuitboard": { "parent": "/obj/item", "declared_vars": {
        "icon_state": "circuit_map", "build_path": null } },
    "/obj/item/circuitboard/machine": { "parent": "/obj/item/circuitboard",
        "declared_vars": { "needs_anchored": 1, "req_components": null,
        "def_components": null } },
    "/obj/item/circuitboard/machine/recharger": { "parent": "/obj/item/circuitboard/machine",
        "declared_vars": { "name": "Weapon Recharger",
        "build_path": { "path": "/obj/machinery/recharger" }, "needs_anchored": 0,
        "req_components": { "list": [
            { "assoc": [{ "path": "/datum/stock_part/capacitor" }, 1] } ] } } },
    "/obj/item/circuitboard/machine/autolathe": { "parent": "/obj/item/circuitboard/machine",
        "declared_vars": { "name": "Autolathe",
        "build_path": { "path": "/obj/machinery/autolathe" },
        "req_components": { "list": [
            { "assoc": [{ "path": "/datum/stock_part/matter_bin" }, 3] },
            { "assoc": [{ "path": "/datum/stock_part/servo" }, 1] },
            { "assoc": [{ "path": "/obj/item/stack/sheet/glass" }, 1] } ] } } },
    "/obj/item/stack": { "parent": "/obj/item", "declared_vars": { "amount": 1,
        "max_amount": 50, "merge_type": null, "singular_name": null, "turf_type": null,
        "mats_per_unit": null } },
    "/obj/item/stack/sheet": { "parent": "/obj/item/stack" },
    "/obj/item/stack/sheet/iron": { "parent": "/obj/item/stack/sheet", "declared_vars": {
        "name": "iron", "icon_state": "sheet-metal", "singular_name": "iron sheet",
        "merge_type": { "path": "/obj/item/stack/sheet/iron" },
        "mats_per_unit": { "list": [ { "assoc": [ { "path": "/datum/material/iron" }, 100 ] } ] } } },
    "/obj/item/stack/sheet/glass": { "parent": "/obj/item/stack/sheet", "declared_vars": {
        "name": "glass", "icon_state": "sheet-glass", "singular_name": "glass sheet",
        "merge_type": { "path": "/obj/item/stack/sheet/glass" },
        "mats_per_unit": { "list": [ { "assoc": [ { "path": "/datum/material/glass" }, 100 ] } ] } } },
    "/obj/item/stack/cable_coil": { "parent": "/obj/item/stack", "declared_vars": {
        "name": "cable coil", "icon_state": "coil",
        "merge_type": { "path": "/obj/item/stack/cable_coil" } } },
    "/obj/item/card": { "parent": "/obj/item" },
    "/obj/item/card/id": { "parent": "/obj/item/card", "declared_vars": {
        "access": { "list": [] }, "icon_state": "card_grey", "slot_flags": 256 } },
    "/obj/item/card/id/advanced": { "parent": "/obj/item/card/id" },
    "/obj/item/clothing": { "parent": "/obj/item" },
    "/obj/item/clothing/under": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 2 } },
    "/obj/item/clothing/under/color": { "parent": "/obj/item/clothing/under" },
    "/obj/item/clothing/under/color/grey": { "parent": "/obj/item/clothing/under/color" },
    "/obj/item/clothing/shoes": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 128 } },
    "/obj/item/clothing/shoes/sneakers": { "parent": "/obj/item/clothing/shoes" },
    "/obj/item/clothing/shoes/sneakers/black": { "parent": "/obj/item/clothing/shoes/sneakers" },
    "/obj/structure": { "parent": "/obj", "declared_vars": { "anchored": 1,
        "max_integrity": 300, "icon": { "resource": "s.dmi" } } },
    "/obj/structure/frame": { "parent": "/obj/structure", "declared_vars": {
        "name": "frame", "icon_state": "box_0", "density": 1, "anchored": 0, "state": 0,
        "max_integrity": 250 } },
    "/obj/structure/frame/machine": { "parent": "/obj/structure/frame", "declared_vars": {
        "name": "machine frame" } },
    "/obj/structure/frame/computer": { "parent": "/obj/structure/frame", "declared_vars": {
        "name": "computer frame", "icon_state": "0" } },
    "/obj/machinery": { "parent": "/obj", "declared_vars": { "anchored": 1, "density": 1,
        "icon": { "resource": "m.dmi" }, "max_integrity": 200, "use_power": 1,
        "idle_power_usage": 100, "active_power_usage": 1000, "power_channel": 1,
        "circuit": null } },
    "/obj/machinery/recharger": { "parent": "/obj/machinery", "declared_vars": {
        "name": "recharger", "icon_state": "recharger", "density": 0,
        "circuit": { "path": "/obj/item/circuitboard/machine/recharger" } } },
    "/obj/machinery/autolathe": { "parent": "/obj/machinery", "declared_vars": {
        "name": "autolathe", "icon_state": "autolathe",
        "circuit": { "path": "/obj/item/circuitboard/machine/autolathe" } } },
    "/mob": { "parent": "/atom/movable", "declared_vars": { "layer": 4, "density": 1 } },
    "/mob/living": { "parent": "/mob" },
    "/mob/living/carbon": { "parent": "/mob/living" },
    "/mob/living/carbon/human": { "parent": "/mob/living/carbon", "declared_vars": {
        "icon": { "resource": "h.dmi" }, "icon_state": "human_basic" } },
    "/turf": { "parent": "/atom", "declared_vars": { "icon": { "resource": "t.dmi" },
        "blocks_air": 0, "can_atmos_pass": 0, "initial_gas_mix": "o2=22;n2=82;TEMP=293.15",
        "temperature": 293.15, "baseturfs": { "path": "/turf/baseturf_bottom" } } },
    "/turf/open": { "parent": "/turf", "declared_vars": { "can_atmos_pass": -1,
        "planetary_atmos": 0 } },
    "/turf/open/floor": { "parent": "/turf/open", "declared_vars": { "icon_state": "floor",
        "baseturfs": { "path": "/turf/open/floor/plating" }, "overfloor_placed": 1,
        "floor_tile": null } },
    "/turf/open/floor/iron": { "parent": "/turf/open/floor" },
    "/turf/closed": { "parent": "/turf", "declared_vars": { "density": 1, "opacity": 1,
        "blocks_air": 1 } },
    "/turf/closed/wall": { "parent": "/turf/closed", "declared_vars": { "icon_state": "wall" } },
    "/area": { "parent": "/atom" },
    "/area/space": { "parent": "/area" }
  }
}"##;

/// A recharger `R` (3,3) and an autolathe `L` (5,3); (3,2) and (5,2) are free.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor/iron,/area/space)
"A" = (/obj/effect/landmark/start/assistant,/turf/open/floor/iron,/area/space)
"R" = (/obj/machinery/recharger,/turf/open/floor/iron,/area/space)
"L" = (/obj/machinery/autolathe,/turf/open/floor/iron,/area/space)

(1,1,1) = {"
wwwwwww
wAfffAw
wfRfLfw
wfffffw
wwwwwww
"}
"##;

const RECHARGER_AT: TilePos = TilePos::new(3, 3, 1);
const AUTOLATHE_AT: TilePos = TilePos::new(5, 3, 1);
const FREE: TilePos = TilePos::new(3, 2, 1);
const CAPACITOR: &str = "/obj/item/stock_parts/capacitor";
const ADV_CAPACITOR: &str = "/obj/item/stock_parts/capacitor/adv";
const RPED: &str = "/obj/item/storage/part_replacer";
const RECHARGER_BOARD: &str = "/obj/item/circuitboard/machine/recharger";

const STATES: [(&str, &str); 15] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
    ("i.dmi", "wrench"),
    ("i.dmi", "crowbar"),
    ("i.dmi", "capacitor"),
    ("i.dmi", "circuit_map"),
    ("i.dmi", "sheet-metal"),
    ("i.dmi", "sheet-glass"),
    ("i.dmi", "coil"),
    ("s.dmi", "box_0"),
    ("m.dmi", "recharger"),
    ("m.dmi", "autolathe"),
];

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("stock_parts_fixture")
        .join(test);
    let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
    let research = ss13_mapload::research::resolve(&types);
    let mut used = vec![
        "/obj/effect/landmark/start/assistant",
        "/obj/machinery/recharger",
        "/obj/machinery/autolathe",
        "/obj/item/stock_parts/capacitor/adv",
        "/obj/item/stock_parts/servo/nano",
        "/obj/item/storage/part_replacer",
        "/obj/item/circuitboard/machine/recharger",
        "/obj/item/stack/cable_coil",
        "/obj/structure/frame/machine",
        "/turf/open/floor/iron",
        "/turf/closed/wall",
        "/area/space",
    ];
    used.extend(research.prototype_types());
    write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)]);
    std::fs::write(
        dir.join(ss13_mapload::research::RESEARCH_FILE),
        research.to_ron(),
    )
    .unwrap();
    dir
}

fn server(test: &str) -> App {
    let assets = fixture_assets(test);
    let mut app = testing::server_app(fixture_map(&assets, &["map.dmm"]), |app| {
        app.insert_resource(MechaDb::new(MechaData {
            stock_parts: vec![
                (CAPACITOR.into(), 1),
                (ADV_CAPACITOR.into(), 2),
                ("/obj/item/stock_parts/servo".into(), 1),
                ("/obj/item/stock_parts/servo/nano".into(), 2),
                ("/obj/item/stock_parts/matter_bin".into(), 1),
            ],
            ..default()
        }));
    });
    app.update();
    app
}

fn obj_at(world: &World, pos: TilePos, path: &str) -> Option<Entity> {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .find(|&e| {
            world.get::<DmObj>(e).is_some_and(|o| o.path == path)
                || world.get::<DmItem>(e).is_some_and(|i| i.path == path)
        })
}

fn parts(world: &World, owner: Entity) -> Vec<String> {
    world
        .get::<ComponentParts>(owner)
        .map(|p| {
            p.parts()
                .iter()
                .map(|&e| world.get::<DmItem>(e).unwrap().path.clone())
                .collect()
        })
        .unwrap_or_default()
}

fn rped_with(world: &mut World, contents: &[&str]) -> Entity {
    let rped = spawn_obj(world, RPED, FREE).unwrap();
    for path in contents {
        let item = spawn_obj(world, path, FREE).unwrap();
        assert_eq!(can_insert(world, rped, item), Ok(()), "{path} fits an RPED");
        put_inside(world, item, rped);
    }
    rped
}

#[test]
fn rped_upgrades_and_parts_come_back() {
    let mut app = server("rped_upgrades_and_parts_come_back");
    let world = app.world_mut();
    let client = world.spawn_empty().id();
    let recharger = obj_at(world, RECHARGER_AT, "/obj/machinery/recharger").unwrap();
    assert_eq!(
        recharge_coeff(world, recharger),
        1.0,
        "a mapped one: tier 1"
    );
    assert!(
        world.get::<ComponentParts>(recharger).is_none(),
        "no items yet"
    );

    let rped = rped_with(world, &[ADV_CAPACITOR, CAPACITOR]);
    // Only parts, boards and the allowed stacks go in.
    let wrench = spawn_obj(world, "/obj/item/wrench", FREE).unwrap();
    assert_eq!(can_insert(world, rped, wrench), Err(Refusal::CantHold));

    // A closed panel lists the parts and swaps nothing.
    assert!(!exchange_parts(world, client, rped, recharger));
    assert_eq!(recharge_coeff(world, recharger), 1.0);
    assert_eq!(world.get::<Stored>(rped).unwrap().items().len(), 2);

    world.entity_mut(recharger).insert(PanelOpen(true));
    assert!(exchange_parts(world, client, rped, recharger));
    assert_eq!(
        parts(world, recharger),
        [RECHARGER_BOARD, ADV_CAPACITOR],
        "the board made, the capacitor swapped for the better one"
    );
    assert_eq!(recharge_coeff(world, recharger), 2.0, "RefreshParts");
    let in_rped: Vec<String> = world
        .get::<Stored>(rped)
        .unwrap()
        .items()
        .iter()
        .map(|&e| world.get::<DmItem>(e).unwrap().path.clone())
        .collect();
    assert_eq!(in_rped, [CAPACITOR, CAPACITOR], "the old one went in");
    // Nothing better left: a second use swaps nothing.
    assert!(exchange_parts(world, client, rped, recharger));
    assert_eq!(parts(world, recharger), [RECHARGER_BOARD, ADV_CAPACITOR]);

    let installed: Vec<Entity> = world
        .get::<ComponentParts>(recharger)
        .unwrap()
        .parts()
        .to_vec();
    ss13_game::obj_damage::deconstruct(world, recharger, true);
    assert!(world.get_entity(recharger).is_err());
    for part in installed {
        assert_eq!(
            world.get::<TilePos>(part),
            Some(&RECHARGER_AT),
            "the same item is on the floor"
        );
        assert!(world.get::<PartOf>(part).is_none());
    }

    // The frame left behind takes its board and parts from an RPED.
    let frame = obj_at(world, RECHARGER_AT, "/obj/structure/frame/machine").unwrap();
    assert_eq!(world.get::<ObjState>(frame).unwrap().0, FRAME_WIRED);
    let board = obj_at(world, RECHARGER_AT, RECHARGER_BOARD).unwrap();
    let fill = spawn_obj(world, RPED, FREE).unwrap();
    put_inside(world, board, fill);
    let cap = spawn_obj(world, ADV_CAPACITOR, FREE).unwrap();
    put_inside(world, cap, fill);
    assert!(ss13_game::machine_frame::rped_interaction(
        world, client, fill, frame
    ));
    assert_eq!(world.get::<ObjState>(frame).unwrap().0, FRAME_BOARD);
    assert!(
        world
            .get::<Frame>(frame)
            .unwrap()
            .needed
            .iter()
            .all(|n| n.amount == 0)
    );
    assert_eq!(parts(world, frame), [RECHARGER_BOARD, ADV_CAPACITOR]);
}

#[test]
fn autolathe_prints_with_its_parts() {
    let mut app = server("autolathe_prints_with_its_parts");
    let world = app.world_mut();
    let client = world.spawn_empty().id();
    let mob = world.spawn_empty().id();
    let lathe = obj_at(world, AUTOLATHE_AT, "/obj/machinery/autolathe").unwrap();
    world.entity_mut(lathe).remove::<Unpowered>();
    assert_eq!(world.get::<Autolathe>(lathe).unwrap().efficiency, 1.6);
    assert_eq!(
        world.get::<MaterialStore>(lathe).unwrap().max,
        Some(11_250),
        "3 tier 1 matter bins: 112.5 sheets"
    );

    // 150 sheets: 112 go in.
    let iron = spawn_stack(world, "/obj/item/stack/sheet/iron", FREE, 50).unwrap();
    for _ in 0..3 {
        let iron = spawn_stack(world, "/obj/item/stack/sheet/iron", FREE, 50).unwrap();
        ss13_game::science::autolathe::item_interaction(world, client, mob, iron, lathe);
    }
    let _ = iron;
    let fe = world
        .resource::<ss13_game::science::techweb::ResearchDb>()
        .material_index["/datum/material/iron"];
    assert_eq!(
        world.get::<MaterialStore>(lathe).unwrap().amounts[fe],
        11_200
    );

    // The hacked design needs the hack.
    assert!(!start_build(world, lathe, "/datum/design/crowbar", 1.0));
    world.get_mut::<Autolathe>(lathe).unwrap().hacked = true;
    assert!(start_build(world, lathe, "/datum/design/crowbar", 1.0));
    assert!(
        !start_build(world, lathe, "/datum/design/wrench", 1.0),
        "busy"
    );
    let printed = |world: &World, path: &str| {
        world
            .resource::<SpatialIndex>()
            .on_tile(AUTOLATHE_AT)
            .iter()
            .filter(|&&e| world.get::<DmItem>(e).is_some_and(|i| i.path == path))
            .count()
    };
    for _ in 0..100 {
        app.update();
    }
    let world = app.world_mut();
    assert_eq!(printed(world, "/obj/item/crowbar"), 1);
    // 100 units at 1.6.
    assert_eq!(
        world.get::<MaterialStore>(lathe).unwrap().amounts[fe],
        11_040
    );

    // A nano-servo: efficiency 1.4.
    world.entity_mut(lathe).insert(PanelOpen(true));
    let rped = rped_with(world, &["/obj/item/stock_parts/servo/nano"]);
    assert!(exchange_parts(world, client, rped, lathe));
    assert_eq!(world.get::<Autolathe>(lathe).unwrap().efficiency, 1.4);
    world.entity_mut(lathe).insert(PanelOpen(false));
    assert!(start_build(world, lathe, "/datum/design/wrench", 2.0));
    for _ in 0..200 {
        app.update();
    }
    let world = app.world_mut();
    assert_eq!(printed(world, "/obj/item/wrench"), 2);
    assert_eq!(
        world.get::<MaterialStore>(lathe).unwrap().amounts[fe],
        11_040 - 280
    );

    // Taken apart, it drops its sheets and its parts.
    ss13_game::obj_damage::deconstruct(world, lathe, true);
    let sheets: u32 = world
        .resource::<SpatialIndex>()
        .on_tile(AUTOLATHE_AT)
        .iter()
        .filter(|&&e| {
            world
                .get::<DmItem>(e)
                .is_some_and(|i| i.path == "/obj/item/stack/sheet/iron")
        })
        .map(|&e| world.get::<Stack>(e).unwrap().amount)
        .sum();
    assert_eq!(sheets, 107, "10760 units");
    assert!(obj_at(world, AUTOLATHE_AT, "/obj/item/stock_parts/servo/nano").is_some());
}

#[test]
fn cell_charger_rate_multiplies_capacitor_tiers() {
    let mut app = server("cell_charger_rate_multiplies_capacitor_tiers");
    let world = app.world_mut();
    let recharger = obj_at(world, RECHARGER_AT, "/obj/machinery/recharger").unwrap();
    // `ratings` of a mapped machine: its board's default capacitor.
    assert_eq!(cell_charger_rate(world, recharger), 2_500.0);
}
