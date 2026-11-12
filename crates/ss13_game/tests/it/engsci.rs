//! Reinforced floors, metal foam walls and rust on a fixture room without clients (the
//! click chain run directly, as `super::eng_tools`):
//!
//! - an engine floor ignores the crowbar; a wrench takes it up in 3 s into two rods and
//!   plating;
//! - two iron sheets on foamed metal make a metal foam wall in 4 s that gives the foam back;
//! - a mapped rusty wall is rusty; a wirebrush (right click) scrapes it clean in 2 s.

use bevy::prelude::*;
use ss13_atmos::AtmosPlugin;
use ss13_game::click_handlers::{self, ClickCtx, ClickPhase};
use ss13_game::health::{Health, Stat};
use ss13_game::human::HumanLook;
use ss13_game::inventory::{CarriedBy, Inventory};
use ss13_game::testing::{self, fixture_map, steps, write_fixture};
use ss13_game::tools::DoAfter;
use ss13_game::wall_damage::Rusty;
use ss13_mapload::{DmItem, DmObj, DmTurfBuild, Turf};
use ss13_shared::TilePos;
use ss13_shared::protocol::ClickModifiers;
use ss13_world::SpatialIndex;

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/datum/armor": { "parent": "/datum", "declared_vars": { "melee": { "expr": "0" } } },
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
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null, "attack_speed": 8,
        "tool_behaviour": null, "toolspeed": 1, "heat": 0 } },
    "/obj/item/crowbar": { "parent": "/obj/item", "declared_vars": {
        "icon_state": "crowbar", "tool_behaviour": "crowbar", "force": 5 } },
    "/obj/item/wrench": { "parent": "/obj/item", "declared_vars": {
        "icon_state": "wrench", "tool_behaviour": "wrench", "force": 5 } },
    "/obj/item/wirebrush": { "parent": "/obj/item", "declared_vars": {
        "icon_state": "wirebrush", "tool_behaviour": "rustscraper" } },
    "/obj/item/stack": { "parent": "/obj/item", "declared_vars": { "amount": 1,
        "max_amount": 50, "merge_type": null, "singular_name": null, "turf_type": null } },
    "/obj/item/stack/sheet": { "parent": "/obj/item/stack" },
    "/obj/item/stack/sheet/iron": { "parent": "/obj/item/stack/sheet", "declared_vars": {
        "name": "iron", "icon_state": "sheet-metal",
        "merge_type": { "path": "/obj/item/stack/sheet/iron" } } },
    "/obj/item/stack/rods": { "parent": "/obj/item/stack", "declared_vars": {
        "icon_state": "rods", "merge_type": { "path": "/obj/item/stack/rods" } } },
    "/obj/structure": { "parent": "/obj", "declared_vars": { "anchored": 1,
        "max_integrity": 300, "icon": { "resource": "s.dmi" } } },
    "/obj/structure/foamedmetal": { "parent": "/obj/structure", "declared_vars": {
        "name": "foamed metal", "icon_state": "metalfoam", "density": 1, "opacity": 1,
        "max_integrity": 20 } },
    "/mob": { "parent": "/atom/movable", "declared_vars": { "layer": 4, "density": 1 } },
    "/turf": { "parent": "/atom", "declared_vars": { "icon": { "resource": "t.dmi" },
        "blocks_air": 0, "can_atmos_pass": 0, "initial_gas_mix": "o2=22;n2=82;TEMP=293.15",
        "temperature": 293.15, "baseturfs": { "path": "/turf/baseturf_bottom" } } },
    "/turf/open": { "parent": "/turf", "declared_vars": { "can_atmos_pass": -1,
        "planetary_atmos": 0 } },
    "/turf/open/floor": { "parent": "/turf/open", "declared_vars": { "icon_state": "floor",
        "baseturfs": { "path": "/turf/open/floor/plating" }, "overfloor_placed": 1,
        "floor_tile": null } },
    "/turf/open/floor/plating": { "parent": "/turf/open/floor", "declared_vars": {
        "icon_state": "plating", "overfloor_placed": 0,
        "baseturfs": { "path": "/turf/baseturf_bottom" } } },
    "/turf/open/floor/engine": { "parent": "/turf/open/floor", "declared_vars": {
        "icon_state": "engine", "floor_tile": { "path": "/obj/item/stack/rods" } } },
    "/turf/open/space": { "parent": "/turf/open", "declared_vars": { "temperature": 2.7,
        "initial_gas_mix": "TEMP=2.7" } },
    "/turf/open/space/basic": { "parent": "/turf/open/space" },
    "/turf/closed": { "parent": "/turf", "declared_vars": { "density": 1, "opacity": 1,
        "blocks_air": 1 } },
    "/turf/closed/wall": { "parent": "/turf/closed", "declared_vars": { "icon_state": "wall",
        "baseturfs": { "path": "/turf/open/floor/plating" },
        "sheet_type": { "path": "/obj/item/stack/sheet/iron" }, "sheet_amount": 2,
        "girder_type": { "path": "/obj/structure/girder" }, "slicing_duration": 100 } },
    "/turf/closed/wall/rust": { "parent": "/turf/closed/wall" },
    "/turf/closed/wall/metal_foam_base": { "parent": "/turf/closed/wall", "declared_vars": {
        "girder_type": { "path": "/obj/structure/foamedmetal" } } },
    "/area": { "parent": "/atom" },
    "/area/space": { "parent": "/area" }
  }
}"##;

/// Rows top-down, the last line is y = 1: an engine floor `E` at (2,3), foamed metal `M` at
/// (4,3), a rusty wall `r` at (3,4); the human stands at (3,3).
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"r" = (/turf/closed/wall/rust,/area/space)
"p" = (/turf/open/floor/plating,/area/space)
"E" = (/turf/open/floor/engine,/area/space)
"M" = (/obj/structure/foamedmetal,/turf/open/floor/plating,/area/space)

(1,1,1) = {"
wwwww
wwrww
wEpMw
wpppw
wwwww
"}
"##;

const STATES: [(&str, &str); 12] = [
    ("t.dmi", "floor"),
    ("t.dmi", "plating"),
    ("t.dmi", "wall"),
    ("t.dmi", "engine"),
    ("l.dmi", "x2"),
    ("i.dmi", "crowbar"),
    ("i.dmi", "wrench"),
    ("i.dmi", "wirebrush"),
    ("i.dmi", "sheet-metal"),
    ("i.dmi", "rods"),
    ("i.dmi", "toy"),
    ("s.dmi", "metalfoam"),
];

const HUMAN_AT: TilePos = TilePos::new(3, 3, 1);
const ENGINE_AT: TilePos = TilePos::new(2, 3, 1);
const FOAM_AT: TilePos = TilePos::new(4, 3, 1);
const RUST_AT: TilePos = TilePos::new(3, 4, 1);

fn server(test: &str) -> App {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("engsci_fixture")
        .join(test);
    let used = [
        "/obj/item/crowbar",
        "/obj/item/wrench",
        "/obj/item/wirebrush",
        "/obj/item/stack/sheet/iron",
        "/obj/item/stack/rods",
        "/obj/structure/foamedmetal",
        "/turf/closed/wall",
        "/turf/closed/wall/rust",
        "/turf/closed/wall/metal_foam_base",
        "/turf/open/floor/plating",
        "/turf/open/floor/engine",
        "/turf/open/space/basic",
        "/area/space",
    ];
    let assets = write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)]);
    let mut app = testing::server_app(fixture_map(&assets, &["map.dmm"]), |app| {
        app.add_plugins(AtmosPlugin);
    });
    app.update();
    app
}

fn run(app: &mut App, secs: f64) {
    for _ in 0..steps(secs) {
        app.update();
    }
}

fn human(world: &mut World) -> Entity {
    world
        .spawn((
            HUMAN_AT,
            HumanLook::random_assistant(1),
            Health::default(),
            Stat::Conscious,
            Inventory::default(),
        ))
        .id()
}

/// `path` spawned into the human's active hand.
fn hold(world: &mut World, mob: Entity, path: &str) -> Entity {
    let item = ss13_game::construction::spawn_detached(world, path).expect("a prototype");
    world.entity_mut(item).insert(CarriedBy(mob));
    world.get_mut::<Inventory>(mob).unwrap().hands[0] = Some(item);
    item
}

fn turf_path(world: &World, pos: TilePos) -> String {
    let turf = ss13_game::construction::turf_at(world, pos).expect("a turf");
    world.get::<DmTurfBuild>(turf).unwrap().path.clone()
}

fn click(app: &mut App, mob: Entity, held: Entity, target: Entity, right: bool) -> bool {
    let world = app.world_mut();
    let pos = *world.get::<TilePos>(target).unwrap();
    let ctx = ClickCtx {
        client: mob,
        mob,
        mob_pos: HUMAN_AT,
        held: Some(held),
        target,
        pos,
        modifiers: ClickModifiers { right, ..default() },
        active_hand: 0,
    };
    click_handlers::run(world, ClickPhase::HeldOnTarget, &ctx)
}

fn on_tile(world: &World, pos: TilePos, path: &str) -> Vec<Entity> {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .filter(|&e| {
            world.get::<DmItem>(e).is_some_and(|i| i.path == path)
                || world.get::<DmObj>(e).is_some_and(|o| o.path == path)
        })
        .collect()
}

#[test]
fn engine_floor_needs_a_wrench() {
    let mut app = server("engine");
    let world = app.world_mut();
    let mob = human(world);
    let crowbar = hold(world, mob, "/obj/item/crowbar");
    let engine = ss13_game::construction::turf_at(world, ENGINE_AT).unwrap();
    assert!(click(&mut app, mob, crowbar, engine, false));
    run(&mut app, 0.5);
    assert_eq!(turf_path(app.world(), ENGINE_AT), "/turf/open/floor/engine");
    let world = app.world_mut();
    world.despawn(crowbar);
    let wrench = hold(world, mob, "/obj/item/wrench");
    assert!(click(&mut app, mob, wrench, engine, false));
    assert!(app.world().get::<DoAfter>(mob).is_some());
    run(&mut app, 2.5);
    assert_eq!(turf_path(app.world(), ENGINE_AT), "/turf/open/floor/engine");
    run(&mut app, 1.0);
    assert_eq!(
        turf_path(app.world(), ENGINE_AT),
        "/turf/open/floor/plating"
    );
    let rods = on_tile(app.world(), ENGINE_AT, "/obj/item/stack/rods");
    assert_eq!(rods.len(), 1);
    assert_eq!(ss13_game::tools::stack_amount(app.world(), rods[0]), 2);
}

#[test]
fn iron_on_foamed_metal_makes_a_foam_wall() {
    let mut app = server("foam");
    let world = app.world_mut();
    let mob = human(world);
    let iron = hold(world, mob, "/obj/item/stack/sheet/iron");
    world
        .get_mut::<ss13_game::tools::Stack>(iron)
        .unwrap()
        .amount = 5;
    let foam = on_tile(world, FOAM_AT, "/obj/structure/foamedmetal")[0];
    assert!(click(&mut app, mob, iron, foam, false));
    run(&mut app, 4.2);
    let world = app.world();
    assert_eq!(
        turf_path(world, FOAM_AT),
        "/turf/closed/wall/metal_foam_base"
    );
    assert!(on_tile(world, FOAM_AT, "/obj/structure/foamedmetal").is_empty());
    assert_eq!(ss13_game::tools::stack_amount(world, iron), 3);
    let wall = ss13_game::construction::turf_at(world, FOAM_AT).unwrap();
    assert_eq!(
        world.get::<DmTurfBuild>(wall).unwrap().girder_type,
        "/obj/structure/foamedmetal"
    );
    assert!(world.get::<Turf>(wall).is_some());
}

#[test]
fn a_wirebrush_scrapes_rust_off() {
    let mut app = server("rust");
    let world = app.world_mut();
    let mob = human(world);
    let wall = ss13_game::construction::turf_at(world, RUST_AT).unwrap();
    assert!(world.get::<Rusty>(wall).is_some(), "mapped rust");
    let brush = hold(world, mob, "/obj/item/wirebrush");
    assert!(click(&mut app, mob, brush, wall, true));
    run(&mut app, 1.5);
    assert!(app.world().get::<Rusty>(wall).is_some());
    run(&mut app, 1.0);
    assert!(app.world().get::<Rusty>(wall).is_none());
}
