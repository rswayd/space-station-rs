//! Border objects and multi-tile airlocks on a fixture station, server side: a reception desk of
//! directional windows around a windoor, and a two-tile airlock with its filler. Mobs are moved
//! with `MoveRequest` directly (no client), so every frame is one simulated tick.
//!
//! `cargo test -p ss13_game --all-features --test it border::metastation -- --ignored` walks through
//! a MetaStation windoor (needs generated assets in `SS13_ASSETS`).

use std::path::Path;

use bevy::prelude::*;
use ss13_game::doors::{Door, DoorFiller, DoorState};
use ss13_game::testing::{fixture_map, server_app, steps, write_fixture};
use ss13_mapload::{DoorKind, MapLoadPlugin};
use ss13_shared::{Dir, Facing, TilePos};
use ss13_world::{Border, Density, MoveRequest, Mover, SpatialIndex};

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
    "/obj/structure": { "parent": "/obj" },
    "/obj/structure/window": { "parent": "/obj/structure", "declared_vars": {
        "icon": { "resource": "w.dmi" }, "icon_state": "window", "density": 1, "flags_1": 2 } },
    "/obj/structure/window/fulltile": { "parent": "/obj/structure/window", "declared_vars": {
        "flags_1": 8 } },
    "/obj/machinery": { "parent": "/obj" },
    "/obj/machinery/door": { "parent": "/obj/machinery", "declared_vars": {
        "icon": { "resource": "d.dmi" }, "icon_state": "door_closed", "density": 1, "opacity": 1,
        "layer": 2.7, "base_icon_state": "door", "autoclose": 0, "can_open_with_hands": 1,
        "closingLayer": 3.1, "glass": 0, "locked": 0, "safe": 1, "visible": 1, "welded": 0,
        "flags_1": 8, "multi_tile": null } },
    "/obj/machinery/door/airlock": { "parent": "/obj/machinery/door", "declared_vars": {
        "icon": { "resource": "a.dmi" }, "icon_state": "closed", "autoclose": 1,
        "base_icon_state": null, "normalspeed": 1, "overlays_file": { "resource": "o.dmi" },
        "airlock_material": null } },
    "/obj/machinery/door/airlock/multi_tile": { "parent": "/obj/machinery/door/airlock",
        "declared_vars": { "icon": { "resource": "m.dmi" }, "multi_tile": 1, "opacity": 0,
        "glass": 1 } },
    "/obj/machinery/door/window": { "parent": "/obj/machinery/door", "declared_vars": {
        "icon": { "resource": "wd.dmi" }, "icon_state": "left", "base_state": "left",
        "visible": 0, "opacity": 0, "flags_1": 2, "layer": 3.1, "closingLayer": 3.1 } },
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

/// Rows top-down: the last line is y = 1. Row y = 4: panes facing south at x 2, 3, 5, 6, the
/// windoor `W` (4,4) facing south, a full-tile window `F` (7,4), the multi-tile airlock `M`
/// (9,4) facing south (its filler on (10,4)).
const MAP: &str = r##""f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"p" = (/obj/structure/window,/turf/open/floor,/area/space)
"W" = (/obj/machinery/door/window,/turf/open/floor,/area/space)
"F" = (/obj/structure/window/fulltile,/turf/open/floor,/area/space)
"M" = (/obj/machinery/door/airlock/multi_tile,/turf/open/floor,/area/space)

(1,1,1) = {"
ffffffffffff
ffffffffffff
fppWppFfMfff
ffffffffffff
ffffffffffff
sfffffffffff
"}
"##;

const WINDOOR: TilePos = TilePos::new(4, 4, 1);
const AIRLOCK: TilePos = TilePos::new(9, 4, 1);
const FILLER: TilePos = TilePos::new(10, 4, 1);

/// Manifest order = `IconStateId`.
const STATES: [(&str, &str); 14] = [
    ("t.dmi", "floor"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("w.dmi", "window"),
    ("wd.dmi", "left"),
    ("wd.dmi", "leftopening"),
    ("wd.dmi", "leftopen"),
    ("wd.dmi", "leftclosing"),
    ("m.dmi", "closed"),
    ("m.dmi", "opening"),
    ("m.dmi", "open"),
    ("m.dmi", "closing"),
    ("a.dmi", "closed"),
    ("d.dmi", "door_closed"),
];

fn fixture(test: &str) -> App {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("border_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/structure/window",
        "/obj/structure/window/fulltile",
        "/obj/machinery/door/window",
        "/obj/machinery/door/airlock/multi_tile",
        "/turf/open/floor",
        "/turf/open/space/basic",
        "/area/space",
    ];
    let assets = write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)]);
    server_app(fixture_map(&assets, &["map.dmm"]), |_| {})
}

/// A bare walker: no ID, so it opens only doors without access requirements.
fn mob(app: &mut App, pos: TilePos) -> Entity {
    app.world_mut()
        .spawn((pos, Mover::new(0.0), Density, Facing(Dir::South)))
        .id()
}

fn pos(app: &App, e: Entity) -> TilePos {
    *app.world().get::<TilePos>(e).unwrap()
}

/// One step request and one tick; whether the mob moved.
fn walk(app: &mut App, entity: Entity, dir: Dir) -> bool {
    let before = pos(app, entity);
    app.world_mut().write_message(MoveRequest { entity, dir });
    app.update();
    pos(app, entity) != before
}

fn door_at(app: &App, at: TilePos) -> Entity {
    let world = app.world();
    world
        .resource::<SpatialIndex>()
        .on_tile(at)
        .iter()
        .copied()
        .find(|&e| world.get::<Door>(e).is_some())
        .unwrap_or_else(|| panic!("no door at {at:?}"))
}

fn state(app: &App, door: Entity) -> DoorState {
    app.world().get::<Door>(door).unwrap().state
}

fn run_until(app: &mut App, what: &str, mut done: impl FnMut(&App) -> bool) {
    for _ in 0..1200 {
        if done(app) {
            return;
        }
        app.update();
    }
    panic!("timed out: {what}");
}

#[test]
fn desk_windoor_opens_only_across_its_edge() {
    let mut app = fixture("desk");
    let windoor = door_at(&app, WINDOOR);
    assert_eq!(
        app.world().get::<Door>(windoor).unwrap().kind,
        DoorKind::Windoor
    );
    assert_eq!(
        app.world().get::<Border>(windoor),
        Some(&Border {
            edges: Dir::South as u8
        })
    );
    assert!(
        app.world()
            .get::<Border>(door_at_or_window(&app, TilePos::new(7, 4, 1)))
            .is_none(),
        "full-tile windows are not border objects"
    );

    // Sideways along the desk from the windoor's neighbor pane: passes, does not open it.
    let side = mob(&mut app, TilePos::new(5, 4, 1));
    assert!(walk(&mut app, side, Dir::West));
    assert_eq!(pos(&app, side), WINDOOR);
    assert_eq!(state(&app, windoor), DoorState::Closed);
    // Out through its edge from its own tile: refused, and the bump opens it.
    assert!(!walk(&mut app, side, Dir::South));
    assert!(matches!(state(&app, windoor), DoorState::Opening { .. }));
    run_until(&mut app, "windoor open", |a| {
        state(a, windoor) == DoorState::Open
    });
    assert!(walk(&mut app, side, Dir::South));
    assert!(walk(&mut app, side, Dir::East));
    assert!(walk(&mut app, side, Dir::East));

    // Panes block the south edge only: from the south refused, from the side and north free.
    let front = mob(&mut app, TilePos::new(2, 3, 1));
    assert!(!walk(&mut app, front, Dir::North));
    let west = mob(&mut app, TilePos::new(1, 4, 1));
    assert!(walk(&mut app, west, Dir::East));
    assert!(walk(&mut app, west, Dir::North));
    assert!(walk(&mut app, west, Dir::South));
    assert!(!walk(&mut app, west, Dir::South));
    // A full-tile window blocks every side.
    let full = mob(&mut app, TilePos::new(8, 4, 1));
    assert!(!walk(&mut app, full, Dir::West));

    // The windoor closes while a mob stands on its tile (no safety check, no reopen), and it
    // then only blocks its edge again.
    assert!(walk(&mut app, west, Dir::East));
    assert!(walk(&mut app, west, Dir::East));
    assert_eq!(pos(&app, west), WINDOOR);
    run_until(&mut app, "windoor closed", |a| {
        state(a, windoor) == DoorState::Closed
    });
    for _ in 0..10 {
        app.update();
    }
    assert_eq!(state(&app, windoor), DoorState::Closed);
    assert!(walk(&mut app, west, Dir::West));
    // The front approach: bump from the south opens it.
    let customer = mob(&mut app, TilePos::new(4, 2, 1));
    assert!(walk(&mut app, customer, Dir::North));
    assert!(!walk(&mut app, customer, Dir::North));
    assert!(matches!(state(&app, windoor), DoorState::Opening { .. }));
}

fn door_at_or_window(app: &App, at: TilePos) -> Entity {
    let world = app.world();
    world
        .resource::<SpatialIndex>()
        .on_tile(at)
        .iter()
        .copied()
        .find(|&e| world.get::<Density>(e).is_some())
        .unwrap_or_else(|| panic!("nothing dense at {at:?}"))
}

#[test]
fn multi_tile_airlock_blocks_and_opens_as_one() {
    let mut app = fixture("multi");
    let airlock = door_at(&app, AIRLOCK);
    let world = app.world();
    let filler = world
        .resource::<SpatialIndex>()
        .on_tile(FILLER)
        .iter()
        .copied()
        .find(|&e| world.get::<DoorFiller>(e).is_some())
        .expect("a filler east of a south-facing multi-tile airlock");
    assert_eq!(world.get::<DoorFiller>(filler).unwrap().0, airlock);
    assert!(world.get::<Density>(filler).is_some());

    // Both tiles block; bumping the filler opens the airlock, and both tiles open.
    let m = mob(&mut app, TilePos::new(10, 3, 1));
    assert!(!walk(&mut app, m, Dir::North));
    assert!(matches!(state(&app, airlock), DoorState::Opening { .. }));
    run_until(&mut app, "airlock open", |a| {
        state(a, airlock) == DoorState::Open
    });
    assert!(app.world().get::<Density>(filler).is_none());
    assert!(walk(&mut app, m, Dir::North));
    assert_eq!(pos(&app, m), FILLER);

    // Standing in the second tile keeps it from closing (8 s autoclose, then retries).
    for _ in 0..steps(12.0) {
        app.update();
    }
    assert_eq!(state(&app, airlock), DoorState::Open);
    assert!(walk(&mut app, m, Dir::North));
    run_until(&mut app, "airlock closed", |a| {
        state(a, airlock) == DoorState::Closed
    });
    assert!(app.world().get::<Density>(filler).is_some());
    assert!(!walk(&mut app, m, Dir::South));

    // Despawning the airlock takes the filler with it.
    app.world_mut().despawn(airlock);
    assert!(app.world().get_entity(filler).is_err());
    assert!(
        app.world()
            .resource::<SpatialIndex>()
            .dense_on_tile(FILLER)
            .is_empty()
    );
}

#[test]
#[ignore = "needs generated assets and MetaStation; run with --ignored"]
fn metastation() {
    let map = MapLoadPlugin {
        maps: vec!["MetaStation".into()],
        ..MapLoadPlugin::from_env().unwrap_or(MapLoadPlugin {
            source: env!("SS13_SOURCE").into(),
            maps: vec![],
            assets: std::env::var("SS13_ASSETS")
                .unwrap_or_else(|_| "assets/generated".into())
                .into(),
        })
    };
    let mut app = server_app(map, |_| {});
    let mut borders = app.world_mut().query::<(Entity, &Border, &TilePos)>();
    let total = borders.iter(app.world()).count();
    let mut windoors: Vec<(Entity, Dir, TilePos)> = Vec::new();
    let mut q = app
        .world_mut()
        .query::<(Entity, &Door, &Border, &TilePos)>();
    for (e, door, border, &at) in q.iter(app.world()) {
        let free = door.req_access.is_empty() && door.req_one_access.is_empty();
        let dir = Dir::from_bits(border.edges).unwrap_or_default();
        if door.kind == DoorKind::Windoor && free && !door.locked && Dir::CARDINALS.contains(&dir) {
            windoors.push((e, dir, at));
        }
    }
    windoors.sort_by_key(|&(_, _, p)| (p.y, p.x));
    println!(
        "MetaStation: {total} dense border objects, {} windoors without access",
        windoors.len()
    );
    // The first windoor whose front and back tiles are free of full-tile blockers.
    let index = app.world().resource::<SpatialIndex>();
    let &(windoor, dir, at) = windoors
        .iter()
        .find(|&&(_, d, p)| {
            index.dense_on_tile(p.step(d)).is_empty() && index.dense_on_tile(p).is_empty()
        })
        .expect("a walkable public windoor");
    println!("walking through the windoor at {at:?} facing {dir:?}");
    let front = at.step(dir);
    let m = mob(&mut app, front);
    // From the front: bumps it open, then through it onto its tile.
    assert!(!walk(&mut app, m, dir.reverse()));
    run_until(&mut app, "windoor open", |a| {
        state(a, windoor) == DoorState::Open
    });
    assert!(walk(&mut app, m, dir.reverse()));
    assert_eq!(pos(&app, m), at);
    // Back out through its edge once it closed: the exit bump opens it again.
    run_until(&mut app, "windoor closed", |a| {
        state(a, windoor) == DoorState::Closed
    });
    assert!(!walk(&mut app, m, dir));
    assert!(matches!(state(&app, windoor), DoorState::Opening { .. }));
    run_until(&mut app, "windoor open again", |a| {
        state(a, windoor) == DoorState::Open
    });
    assert!(walk(&mut app, m, dir));
    assert_eq!(pos(&app, m), front);
}
