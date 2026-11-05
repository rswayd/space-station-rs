//! Zero gravity on a fixture: a walled box of space turfs with a lattice. Mobs are spawned and
//! moved directly (no client), so every frame is one simulated tick.

use std::path::Path;

use bevy::prelude::*;
use ss13_game::breath::{AlertCategory, Alerts};
use ss13_game::health::living_bundle;
use ss13_game::hud::HealthHudData;
use ss13_game::spacemove::{Drift, SpaceStep, Weightless, process_spacemove};
use ss13_game::testing::{fixture_map, server_app, steps, write_fixture};
use ss13_shared::{Dir, Facing, TilePos};
use ss13_world::{Density, MoveRequest, Mover};

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 }, "declared_vars": { "flags_1": 0 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3, "anchored": 0 } },
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
    "/obj/structure": { "parent": "/obj", "declared_vars": { "anchored": 1 } },
    "/obj/structure/lattice": { "parent": "/obj/structure", "declared_vars": {
        "name": "lattice", "icon": { "resource": "s.dmi" }, "icon_state": "lattice" } },
    "/mob": { "parent": "/atom/movable", "declared_vars": { "layer": 4, "density": 1 } },
    "/mob/living": { "parent": "/mob" },
    "/mob/living/carbon": { "parent": "/mob/living" },
    "/mob/living/carbon/human": { "parent": "/mob/living/carbon", "declared_vars": {
        "icon": { "resource": "h.dmi" }, "icon_state": "human_basic" } },
    "/turf": { "parent": "/atom", "declared_vars": { "icon": { "resource": "t.dmi" } } },
    "/turf/open": { "parent": "/turf" },
    "/turf/open/space": { "parent": "/turf/open", "declared_vars": { "icon_state": "space" } },
    "/turf/open/space/basic": { "parent": "/turf/open/space" },
    "/turf/closed": { "parent": "/turf", "declared_vars": { "density": 1, "opacity": 1 } },
    "/turf/closed/wall": { "parent": "/turf/closed", "declared_vars": { "icon_state": "wall" } },
    "/area": { "parent": "/atom" },
    "/area/space": { "parent": "/area" }
  }
}"##;

/// Rows top-down, the last line is y = 1: walls at x = 1 and 12, y = 1 and 7; a lattice `L` at
/// (9,2).
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"s" = (/turf/open/space/basic,/area/space)
"A" = (/obj/effect/landmark/start/assistant,/turf/open/space/basic,/area/space)
"L" = (/obj/structure/lattice,/turf/open/space/basic,/area/space)

(1,1,1) = {"
wwwwwwwwwwww
wAsssssssssw
wssssssssssw
wssssssssssw
wssssssssssw
wsssssssLssw
wwwwwwwwwwww
"}
"##;

/// Manifest order = `IconStateId`.
const STATES: [(&str, &str); 5] = [
    ("t.dmi", "space"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("s.dmi", "lattice"),
];

fn fixture(test: &str) -> App {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("spacemove_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/structure/lattice",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
    ];
    let assets = write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)]);
    server_app(fixture_map(&assets, &["map.dmm"]), |_| {})
}

/// A living mob without a client.
fn mob(app: &mut App, pos: TilePos) -> Entity {
    let e = app
        .world_mut()
        .spawn((
            pos,
            Mover::new(0.0),
            Density,
            Facing(Dir::South),
            living_bundle(),
            Alerts::default(),
        ))
        .id();
    app.update();
    app.update();
    e
}

fn pos(app: &App, e: Entity) -> TilePos {
    *app.world().get::<TilePos>(e).unwrap()
}

fn run_until(app: &mut App, secs: f64, what: &str, mut done: impl FnMut(&App) -> bool) {
    for _ in 0..steps(secs) {
        if done(app) {
            return;
        }
        app.update();
    }
    panic!("timed out: {what}");
}

#[test]
fn a_step_off_the_wall_drifts_to_the_far_wall() {
    let mut app = fixture("drift");
    let m = mob(&mut app, TilePos::new(2, 4, 1));
    assert_eq!(
        app.world().get::<Weightless>(m),
        Some(&Weightless { stuck: false })
    );
    app.world_mut().write_message(MoveRequest {
        entity: m,
        dir: Dir::East,
    });
    app.update();
    assert_eq!(pos(&app, m), TilePos::new(3, 4, 1));
    assert_eq!(app.world().get::<Drift>(m).map(|d| d.dir), Some(Dir::East));
    // A tile per 0.5 s at 1 N: eight more tiles, stopping beside the east wall (x = 12).
    let start = app.world().resource::<Time>().elapsed_secs_f64();
    run_until(&mut app, 10.0, "the drift stops", |app| {
        app.world().get::<Drift>(m).is_none()
    });
    let took = app.world().resource::<Time>().elapsed_secs_f64() - start;
    assert_eq!(pos(&app, m), TilePos::new(11, 4, 1));
    assert!((3.5..=4.6).contains(&took), "{took}");
    // The drift does not turn the mob.
    assert_eq!(app.world().get::<Facing>(m), Some(&Facing(Dir::East)));
}

#[test]
fn with_nothing_to_push_off_the_mob_is_stuck() {
    let mut app = fixture("stuck");
    let m = mob(&mut app, TilePos::new(6, 4, 1));
    assert_eq!(
        app.world().get::<Weightless>(m),
        Some(&Weightless { stuck: true })
    );
    assert!(
        app.world()
            .get::<Alerts>(m)
            .unwrap()
            .get(AlertCategory::Gravity)
            .is_some()
    );
    assert!(app.world().get::<HealthHudData>(m).unwrap().immobile);
    // The step is refused.
    app.world_mut().write_message(SpaceStep {
        mob: m,
        dir: Dir::North,
    });
    app.update();
    assert_eq!(pos(&app, m), TilePos::new(6, 4, 1));

    // A lattice within a tile is a hold.
    let held = mob(&mut app, TilePos::new(9, 3, 1));
    assert_eq!(
        app.world().get::<Weightless>(held),
        Some(&Weightless { stuck: false })
    );
    assert!(!app.world().get::<HealthHudData>(held).unwrap().immobile);
    app.world_mut().write_message(SpaceStep {
        mob: held,
        dir: Dir::North,
    });
    app.update();
    assert_eq!(pos(&app, held), TilePos::new(9, 4, 1));
}

#[test]
fn pushing_off_a_mob_sends_it_the_other_way() {
    let mut app = fixture("push");
    let pusher = mob(&mut app, TilePos::new(6, 4, 1));
    let pushed = mob(&mut app, TilePos::new(5, 4, 1));
    // Each is the other's hold.
    assert_eq!(
        app.world().get::<Weightless>(pusher),
        Some(&Weightless { stuck: false })
    );
    assert!(process_spacemove(
        app.world_mut(),
        pusher,
        Some(Dir::East),
        false
    ));
    assert_eq!(
        app.world().get::<Drift>(pushed).map(|d| d.dir),
        Some(Dir::West)
    );
    run_until(&mut app, 10.0, "the pushed mob stops", |app| {
        app.world().get::<Drift>(pushed).is_none()
    });
    // Beside the west wall (x = 1); the pusher stays.
    assert_eq!(pos(&app, pushed), TilePos::new(2, 4, 1));
    assert_eq!(pos(&app, pusher), TilePos::new(6, 4, 1));
}
