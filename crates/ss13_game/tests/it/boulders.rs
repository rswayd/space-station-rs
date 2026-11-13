//! Ore vents and boulders on a fixture station without clients (`ss13_game::mining::vents`,
//! `ss13_game::mining::boulders`): the starter vent comes tapped with iron and glass and makes a
//! boulder of them; the BRM teleports an available boulder onto its tile (and is blocked while
//! it lies there); in the smelter the boulder wears down, its iron goes into the ore silo with
//! its mining points and the glass stays in it, put back out; crumbled by hand it gives ore.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::inventory::Inventory;
use ss13_game::mining::boulders::{self, Boulder, Bouldertech, Brm, Collect};
use ss13_game::mining::vents::{self, OreGeneration, OreVent};
use ss13_game::science::lathe::{MaterialStore, OreSilo};
use ss13_game::science::techweb::ResearchDb;
use ss13_game::stock_parts::{PartKind, PartRatings};
use ss13_game::testing::{self, fixture_map, write_fixture};
use ss13_mapload::DmItem;
use ss13_mapload::research::{MaterialDef, ResearchData};
use ss13_shared::TilePos;
use ss13_world::SpatialIndex;

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0, "pass_flags_self": 0 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3, "anchored": 0 } },
    "/obj": { "parent": "/atom/movable", "declared_vars": { "max_integrity": 100,
        "req_access": null } },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start" },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null, "w_class": 3, "tool_behaviour": null,
        "toolspeed": 1,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null } },
    "/obj/item/boulder": { "parent": "/obj/item", "declared_vars": { "name": "boulder",
        "w_class": 5 } },
    "/obj/item/stack": { "parent": "/obj/item", "declared_vars": { "amount": 1,
        "max_amount": 50, "merge_type": null, "singular_name": null } },
    "/obj/item/stack/ore": { "parent": "/obj/item/stack", "declared_vars": { "points": 0,
        "refined_type": null, "name": "rock" } },
    "/obj/item/stack/ore/glass": { "parent": "/obj/item/stack/ore", "declared_vars": {
        "name": "sand", "points": 1, "merge_type": { "path": "/obj/item/stack/ore/glass" } } },
    "/obj/item/stack/ore/glass/basalt": { "parent": "/obj/item/stack/ore/glass",
        "declared_vars": { "name": "volcanic ash",
        "merge_type": { "path": "/obj/item/stack/ore/glass/basalt" } } },
    "/obj/item/stack/ore/iron": { "parent": "/obj/item/stack/ore", "declared_vars": {
        "name": "iron ore", "points": 1, "merge_type": { "path": "/obj/item/stack/ore/iron" } } },
    "/obj/structure": { "parent": "/obj", "declared_vars": { "anchored": 1 } },
    "/obj/structure/ore_vent": { "parent": "/obj/structure", "declared_vars": {
        "name": "ore vent", "icon": { "resource": "i.dmi" }, "icon_state": "toy",
        "density": 1 } },
    "/obj/structure/ore_vent/starter_resources": { "parent": "/obj/structure/ore_vent",
        "declared_vars": { "name": "active ore vent" } },
    "/obj/machinery": { "parent": "/obj", "declared_vars": { "anchored": 1, "density": 1,
        "icon": { "resource": "m.dmi" }, "icon_state": "machine" } },
    "/obj/machinery/ore_silo": { "parent": "/obj/machinery", "declared_vars": {
        "name": "ore silo" } },
    "/obj/machinery/brm": { "parent": "/obj/machinery", "declared_vars": {
        "name": "boulder retrieval matrix" } },
    "/obj/machinery/bouldertech": { "parent": "/obj/machinery", "declared_vars": {
        "name": "bouldertech brand refining machine" } },
    "/obj/machinery/bouldertech/refinery": { "parent": "/obj/machinery/bouldertech",
        "declared_vars": { "name": "boulder refinery" } },
    "/obj/machinery/bouldertech/refinery/smelter": {
        "parent": "/obj/machinery/bouldertech/refinery",
        "declared_vars": { "name": "boulder smelter" } },
    "/mob": { "parent": "/atom/movable", "declared_vars": { "layer": 4, "density": 1 } },
    "/obj/item/card": { "parent": "/obj/item" },
    "/obj/item/card/id": { "parent": "/obj/item/card", "declared_vars": {
        "access": { "list": [] }, "slot_flags": 256 } },
    "/obj/item/card/id/advanced": { "parent": "/obj/item/card/id" },
    "/obj/item/clothing": { "parent": "/obj/item" },
    "/obj/item/clothing/under": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 2 } },
    "/obj/item/clothing/under/color": { "parent": "/obj/item/clothing/under" },
    "/obj/item/clothing/under/color/grey": { "parent": "/obj/item/clothing/under/color" },
    "/obj/item/clothing/shoes": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 128 } },
    "/obj/item/clothing/shoes/sneakers": { "parent": "/obj/item/clothing/shoes" },
    "/obj/item/clothing/shoes/sneakers/black": { "parent": "/obj/item/clothing/shoes/sneakers" },
    "/mob/living": { "parent": "/mob" },
    "/mob/living/carbon": { "parent": "/mob/living" },
    "/mob/living/carbon/human": { "parent": "/mob/living/carbon", "declared_vars": {
        "icon": { "resource": "h.dmi" }, "icon_state": "human_basic" } },
    "/turf": { "parent": "/atom", "declared_vars": { "icon": { "resource": "t.dmi" } } },
    "/turf/open": { "parent": "/turf" },
    "/turf/open/floor": { "parent": "/turf/open", "declared_vars": { "icon_state": "floor" } },
    "/turf/open/space": { "parent": "/turf/open" },
    "/turf/open/space/basic": { "parent": "/turf/open/space" },
    "/turf/closed": { "parent": "/turf", "declared_vars": { "density": 1, "opacity": 1 } },
    "/turf/closed/wall": { "parent": "/turf/closed", "declared_vars": { "icon_state": "wall" } },
    "/area": { "parent": "/atom" },
    "/area/space": { "parent": "/area" }
  }
}"##;

/// Rows top-down, the last line is y = 1: the starter vent `V` (2,3), the BRM `R` (4,3), the
/// silo `O` (6,3), the smelter `S` (5,2).
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"V" = (/obj/structure/ore_vent/starter_resources,/turf/open/floor,/area/space)
"O" = (/obj/machinery/ore_silo,/turf/open/floor,/area/space)
"R" = (/obj/machinery/brm,/turf/open/floor,/area/space)
"S" = (/obj/machinery/bouldertech/refinery/smelter,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwwww
wVfRfOw
wsffSfw
wwwwwww
"}
"##;

const VENT: TilePos = TilePos::new(2, 3, 1);
const BRM_AT: TilePos = TilePos::new(4, 3, 1);
const SMELTER_AT: TilePos = TilePos::new(5, 2, 1);
const NEXT_TO_SMELTER: TilePos = TilePos::new(4, 2, 1);
const IRON: &str = "/datum/material/iron";
const GLASS: &str = "/datum/material/glass";

const STATES: [(&str, &str); 6] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
    ("m.dmi", "machine"),
];

fn fixture_assets() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("boulders_fixture");
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/item/boulder",
        "/obj/item/stack/ore/glass/basalt",
        "/obj/item/stack/ore/iron",
        "/obj/structure/ore_vent/starter_resources",
        "/obj/machinery/ore_silo",
        "/obj/machinery/brm",
        "/obj/machinery/bouldertech/refinery/smelter",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
    ];
    write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)])
}

fn research() -> ResearchDb {
    let material = |id: &str, name: &str, sheet: &str| MaterialDef {
        id: id.into(),
        name: name.into(),
        sheet_type: Some(sheet.into()),
        color: None,
    };
    ResearchDb::new(ResearchData {
        materials: vec![
            material(IRON, "iron", "/obj/item/stack/sheet/iron"),
            material(GLASS, "glass", "/obj/item/stack/sheet/glass"),
        ],
        ..default()
    })
}

fn on<T: Component>(app: &App, pos: TilePos) -> Option<Entity> {
    let world = app.world();
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .find(|&e| world.get::<T>(e).is_some())
}

fn silo_units(app: &mut App, material: usize) -> u32 {
    let world = app.world_mut();
    let mut silos = world.query_filtered::<&MaterialStore, With<OreSilo>>();
    silos.single(world).unwrap().amounts[material]
}

#[test]
fn a_vent_boulder_goes_through_the_brm_and_the_smelter() {
    let assets = fixture_assets();
    let mut app = testing::server_app(fixture_map(&assets, &["map.dmm"]), |app| {
        app.insert_resource(research());
    });
    app.update();

    // The starter vent: tapped, iron and glass.
    let vent = on::<OreVent>(&app, VENT).expect("vent set up");
    let state = app.world().get::<OreVent>(vent).unwrap().clone();
    assert!(state.tapped && state.discovered);
    let minerals: Vec<&str> = state.minerals.iter().map(|(m, _)| m.as_str()).collect();
    assert_eq!(minerals, [IRON, GLASS]);

    // `produce_boulder`: on the vent's tile, small, of its minerals.
    let boulder = vents::produce_boulder(app.world_mut(), vent, false).expect("boulder");
    let made = app.world().get::<Boulder>(boulder).unwrap().clone();
    assert_eq!(app.world().get::<TilePos>(boulder), Some(&VENT));
    assert_eq!(made.size, 5);
    assert!((2..=5).contains(&made.durability));
    assert!(!made.materials.is_empty());
    assert!(
        made.materials
            .iter()
            .all(|(m, units)| (m == IRON || m == GLASS) && *units >= 50.0)
    );

    // The BRM pulls an available boulder onto its tile, then is blocked by it.
    let brm = on::<Brm>(&app, BRM_AT).expect("BRM set up");
    // Only this boulder is on offer (the vent may have made others meanwhile).
    app.world_mut().resource_mut::<OreGeneration>().available = vec![boulder];
    assert_eq!(
        boulders::pre_collect(app.world_mut(), brm, true, 1),
        Collect::Teleported
    );
    assert_eq!(app.world().get::<TilePos>(boulder), Some(&BRM_AT));
    // Only this boulder is on offer (the vent may have made others meanwhile).
    app.world_mut().resource_mut::<OreGeneration>().available = vec![boulder];
    assert_eq!(
        boulders::pre_collect(app.world_mut(), brm, true, 1),
        Collect::Blocked
    );

    // Into the smelter by hand, worn through in one machine tick.
    let smelter = on::<Bouldertech>(&app, SMELTER_AT).expect("smelter set up");
    let world = app.world_mut();
    world.entity_mut(smelter).insert(PartRatings {
        parts: vec![
            (PartKind::Servo, 1),
            (PartKind::Servo, 1),
            (PartKind::MatterBin, 1),
            (PartKind::MatterBin, 1),
        ],
        cells: vec![],
    });
    {
        let mut b = world.get_mut::<Boulder>(boulder).unwrap();
        b.materials = vec![(IRON.into(), 300.0), (GLASS.into(), 200.0)];
        b.durability = 1;
    }
    let miner = world
        .spawn((
            NEXT_TO_SMELTER,
            Inventory {
                hands: [Some(boulder), None],
                ..default()
            },
        ))
        .id();
    assert!(ss13_game::mining::item_interaction(
        world,
        Entity::PLACEHOLDER,
        miner,
        boulder,
        smelter
    ));
    assert_eq!(app.world().get::<TilePos>(boulder), None, "inside");
    assert_eq!(
        app.world().get::<Inventory>(miner).unwrap().hands[0],
        None,
        "out of the hand"
    );
    for _ in 0..testing::steps(2.5) {
        app.update();
    }
    assert_eq!(silo_units(&mut app, 0), 300, "iron in the silo");
    assert_eq!(silo_units(&mut app, 1), 0, "the smelter leaves glass");
    assert_eq!(app.world().get::<Bouldertech>(smelter).unwrap().points, 3);
    let left = app.world().get::<Boulder>(boulder).unwrap().clone();
    assert_eq!(left.materials, vec![(GLASS.to_owned(), 200.0)]);
    assert_eq!(left.processed_by, None);
    assert!((2..=4).contains(&left.durability));
    assert_eq!(app.world().get::<TilePos>(boulder), Some(&SMELTER_AT));

    // Crumbled by hand: one ash ore for two sheets of glass.
    boulders::convert_to_ore(app.world_mut(), boulder);
    app.update();
    let world = app.world();
    let ore = world
        .resource::<SpatialIndex>()
        .on_tile(SMELTER_AT)
        .iter()
        .copied()
        .find(|&e| {
            world
                .get::<DmItem>(e)
                .is_some_and(|i| i.path == "/obj/item/stack/ore/glass/basalt")
        })
        .expect("ash ore");
    assert_eq!(ss13_game::tools::stack_amount(world, ore), 1);
}
