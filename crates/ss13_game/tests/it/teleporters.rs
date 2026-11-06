//! Map teleporters (`ss13_game::effects::teleporters`) on a fixture floor, server side: a pair
//! of bump teleporters linked by `id`/`id_target` var edits (CentCom's), an offset step trigger
//! (a looping hallway) and a pair of permanent portals sharing an `id`. Walkers are moved with
//! `MoveRequest` directly (no client).

use std::path::Path;

use bevy::prelude::*;
use ss13_game::testing::{fixture_map, server_app, write_fixture};
use ss13_mapload::{DmObj, DmTrigger};
use ss13_shared::{Dir, Facing, TilePos};
use ss13_world::{Density, MoveRequest, Mover, SpatialIndex};

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
    "/obj/effect/step_trigger": { "parent": "/obj/effect", "declared_vars": {
        "invisibility": 101, "affect_ghosts": 0, "stopper": 1, "mobs_only": 0 } },
    "/obj/effect/step_trigger/teleporter": { "parent": "/obj/effect/step_trigger",
        "declared_vars": { "teleport_x": 0, "teleport_y": 0, "teleport_z": 0 } },
    "/obj/effect/step_trigger/teleporter/offset": {
        "parent": "/obj/effect/step_trigger/teleporter",
        "declared_vars": { "teleport_x_offset": 0, "teleport_y_offset": 0 } },
    "/obj/effect/bump_teleporter": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101,
        "density": 1, "id": null, "id_target": null } },
    "/obj/effect/portal": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "p.dmi" }, "icon_state": "portal", "density": 1 } },
    "/obj/effect/portal/permanent": { "parent": "/obj/effect/portal",
        "declared_vars": { "id": null } },
    "/obj/item": { "parent": "/obj", "declared_vars": { "interaction_flags_item": 1 } },
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
    "/mob": { "parent": "/atom/movable", "declared_vars": { "layer": 4, "density": 1 } },
    "/mob/living": { "parent": "/mob" },
    "/mob/living/carbon": { "parent": "/mob/living" },
    "/mob/living/carbon/human": { "parent": "/mob/living/carbon", "declared_vars": {
        "icon": { "resource": "h.dmi" }, "icon_state": "human_basic" } },
    "/turf": { "parent": "/atom", "declared_vars": { "icon": { "resource": "t.dmi" } } },
    "/turf/open": { "parent": "/turf" },
    "/turf/open/floor": { "parent": "/turf/open", "declared_vars": { "icon_state": "floor" } },
    "/turf/open/space": { "parent": "/turf/open" },
    "/turf/open/space/basic": { "parent": "/turf/open/space" },
    "/area": { "parent": "/atom" },
    "/area/space": { "parent": "/area" }
  }
}"##;

/// Rows top-down: the last line is y = 1. Portals `P` (2,4) and (10,4), id "P"; bump
/// teleporters `A` (2,2) -> `B` (10,2) and back; the offset trigger `O` (6,1) sends 4 east and
/// 2 north, to (10,3).
const MAP: &str = r##""f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"P" = (/obj/effect/portal/permanent{id = "P"},/turf/open/floor,/area/space)
"A" = (/obj/effect/bump_teleporter{id = "A"; id_target = "B"},/turf/open/floor,/area/space)
"B" = (/obj/effect/bump_teleporter{id = "B"; id_target = "A"},/turf/open/floor,/area/space)
"O" = (/obj/effect/step_trigger/teleporter/offset{teleport_x_offset = 4; teleport_y_offset = 2},/turf/open/floor,/area/space)

(1,1,1) = {"
ffffffffffff
fPfffffffPff
ffffffffffff
fAfffffffBff
sffffOffffff
"}
"##;

const STATES: [(&str, &str); 4] = [
    ("t.dmi", "floor"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("p.dmi", "portal"),
];

fn fixture(test: &str) -> App {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("teleporters_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/effect/portal/permanent",
        "/obj/effect/bump_teleporter",
        "/obj/effect/step_trigger/teleporter/offset",
        "/turf/open/floor",
        "/turf/open/space/basic",
        "/area/space",
    ];
    let assets = write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)]);
    server_app(fixture_map(&assets, &["map.dmm"]), |_| {})
}

fn walker(app: &mut App, pos: TilePos) -> Entity {
    app.world_mut()
        .spawn((pos, Mover::new(0.0), Density, Facing(Dir::East)))
        .id()
}

fn pos(app: &App, e: Entity) -> TilePos {
    *app.world().get::<TilePos>(e).unwrap()
}

fn walk(app: &mut App, entity: Entity, dir: Dir) {
    app.world_mut().write_message(MoveRequest { entity, dir });
    app.update();
}

fn trigger_at(app: &App, at: TilePos) -> DmTrigger {
    let world = app.world();
    world
        .resource::<SpatialIndex>()
        .on_tile(at)
        .iter()
        .find_map(|&e| world.get::<DmTrigger>(e).cloned())
        .unwrap_or_else(|| panic!("no trigger at {at:?}"))
}

#[test]
fn map_edits_link_the_teleporters() {
    let app = fixture("edits");
    let a = trigger_at(&app, TilePos::new(2, 2, 1));
    assert_eq!(a.path, "/obj/effect/bump_teleporter");
    assert_eq!((a.id.as_str(), a.id_target.as_str()), ("A", "B"));
    let offset = trigger_at(&app, TilePos::new(6, 1, 1));
    assert_eq!(
        (offset.teleport_x_offset, offset.teleport_y_offset),
        (4.0, 2.0)
    );
    assert!(offset.stopper);
    assert_eq!(trigger_at(&app, TilePos::new(10, 4, 1)).id, "P");
}

#[test]
fn bumping_a_teleporter_lands_on_its_target() {
    let mut app = fixture("bump");
    let w = walker(&mut app, TilePos::new(1, 2, 1));
    walk(&mut app, w, Dir::East);
    assert_eq!(pos(&app, w), TilePos::new(10, 2, 1));
}

#[test]
fn offset_trigger_moves_the_walker_by_its_offset() {
    let mut app = fixture("offset");
    let w = walker(&mut app, TilePos::new(5, 1, 1));
    walk(&mut app, w, Dir::East);
    assert_eq!(pos(&app, w), TilePos::new(10, 3, 1));
    // Landing by teleport does not trigger anything; walking on is a plain step.
    walk(&mut app, w, Dir::West);
    assert_eq!(pos(&app, w), TilePos::new(9, 3, 1));
}

#[test]
fn permanent_portals_carry_objects_to_their_twin() {
    let mut app = fixture("portal");
    let crate_obj = walker(&mut app, TilePos::new(1, 4, 1));
    app.world_mut().entity_mut(crate_obj).insert(DmObj {
        path: "/obj/structure/closet/crate".into(),
        ..default()
    });
    walk(&mut app, crate_obj, Dir::East);
    assert_eq!(pos(&app, crate_obj), TilePos::new(10, 4, 1));

    // A bare movable that is neither an object nor a mob stays.
    let ghostly = walker(&mut app, TilePos::new(1, 4, 1));
    walk(&mut app, ghostly, Dir::East);
    assert_eq!(pos(&app, ghostly), TilePos::new(1, 4, 1));
}
