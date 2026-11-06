//! The shared teleport (`ss13_game::teleport`) and quantum pads
//! (`ss13_game::science::quantum_pad`) on a fixture floor, server side: a plain area and a
//! `NOTELEPORT` one (`/area/centcom`). Pads, crates and tools are spawned directly and the
//! module's procs called with a stand-in client.

use std::path::Path;

use bevy::prelude::*;
use ss13_game::construction::PanelOpen;
use ss13_game::science::quantum_pad::{self, QuantumPad};
use ss13_game::teleport::{Tele, do_teleport};
use ss13_game::testing::{fixture_map, server_app, steps, write_fixture};
use ss13_mapload::{DmItem, DmObj, DmTool};
use ss13_shared::TilePos;

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
    "/area": { "parent": "/atom", "declared_vars": { "area_flags": 0, "requires_power": 0 } },
    "/area/space": { "parent": "/area" },
    "/area/centcom": { "parent": "/area", "declared_vars": { "area_flags": 4 } }
  }
}"##;

/// Rows top-down: the last line is y = 1. x 1-6 plain, x 7-8 CentCom (`NOTELEPORT`).
const MAP: &str = r##""f" = (/turf/open/floor,/area/space)
"c" = (/turf/open/floor,/area/centcom)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)

(1,1,1) = {"
ffffffcc
ffffffcc
sfffffcc
"}
"##;

const STATES: [(&str, &str); 3] = [
    ("t.dmi", "floor"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
];

fn fixture(test: &str) -> App {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("quantum_pad_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/turf/open/floor",
        "/turf/open/space/basic",
        "/area/space",
        "/area/centcom",
    ];
    let assets = write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)]);
    server_app(fixture_map(&assets, &["map.dmm"]), |_| {})
}

fn obj(app: &mut App, path: &str, pos: TilePos, anchored: bool) -> Entity {
    app.world_mut()
        .spawn((
            pos,
            DmObj {
                path: path.into(),
                anchored,
                ..default()
            },
        ))
        .id()
}

fn pos(app: &App, e: Entity) -> TilePos {
    *app.world().get::<TilePos>(e).unwrap()
}

fn t(x: i32, y: i32) -> TilePos {
    TilePos::new(x, y, 1)
}

const CRATE: &str = "/obj/structure/closet/crate";

#[test]
fn do_teleport_keeps_out_of_noteleport_areas_unless_forced() {
    let mut app = fixture("areas");
    let c = obj(&mut app, CRATE, t(2, 2), false);
    let world = app.world_mut();
    assert!(do_teleport(world, c, t(5, 2), Tele::default()));
    assert_eq!(*world.get::<TilePos>(c).unwrap(), t(5, 2));
    // Into CentCom: refused, it stays.
    assert!(!do_teleport(world, c, t(7, 2), Tele::default()));
    assert_eq!(*world.get::<TilePos>(c).unwrap(), t(5, 2));
    let forced = Tele {
        forced: true,
        ..default()
    };
    assert!(do_teleport(world, c, t(7, 2), forced));
    // And out of it: the origin's area counts too.
    assert!(!do_teleport(world, c, t(2, 2), Tele::default()));
    assert_eq!(*world.get::<TilePos>(c).unwrap(), t(7, 2));
}

#[test]
fn imprecise_teleports_land_near_the_target_outside_noteleport_areas() {
    let mut app = fixture("precision");
    let c = obj(&mut app, CRATE, t(1, 1), false);
    for _ in 0..20 {
        let tele = Tele {
            precision: 2,
            ..default()
        };
        assert!(do_teleport(app.world_mut(), c, t(6, 2), tele));
        let at = pos(&app, c);
        assert!((4..=6).contains(&at.x) && (1..=3).contains(&at.y), "{at:?}");
    }
}

#[test]
fn teleported_effects_are_deleted() {
    let mut app = fixture("effects");
    let decal = obj(&mut app, "/obj/effect/decal/cleanable/dirt", t(2, 2), false);
    assert!(!do_teleport(
        app.world_mut(),
        decal,
        t(4, 2),
        Tele::default()
    ));
    assert!(app.world().get_entity(decal).is_err());
}

#[test]
fn a_linked_pad_sends_what_stands_on_it_after_charging() {
    let mut app = fixture("pad");
    let a = obj(&mut app, quantum_pad::PAD, t(2, 2), true);
    let b = obj(&mut app, quantum_pad::PAD, t(5, 2), true);
    let c = obj(&mut app, CRATE, t(2, 2), false);
    let bolted = obj(&mut app, CRATE, t(2, 2), true);
    let client = app.world_mut().spawn_empty().id();
    app.world_mut().entity_mut(a).insert(QuantumPad {
        linked: Some(b),
        ..default()
    });
    quantum_pad::interact(app.world_mut(), client, client, a, Some(b));
    assert!(app.world().get::<QuantumPad>(a).unwrap().charging.is_some());
    // No parts: `teleport_speed` 5 s, `teleport_cooldown` 40 s.
    let (speed, cooldown, _) = quantum_pad::ratings(app.world(), a);
    assert_eq!((speed, cooldown), (5.0, 40.0));
    for _ in 0..steps(4.5) {
        app.update();
    }
    assert_eq!(pos(&app, c), t(2, 2), "still charging");
    for _ in 0..steps(1.0) {
        app.update();
    }
    assert_eq!(pos(&app, c), t(5, 2));
    assert_eq!(pos(&app, bolted), t(2, 2), "anchored things stay");
    assert_eq!((pos(&app, a), pos(&app, b)), (t(2, 2), t(5, 2)));
    let state = *app.world().get::<QuantumPad>(a).unwrap();
    assert!(state.charging.is_none() && state.last_teleport.is_some());

    // Recharging: a second use right away does not charge.
    quantum_pad::interact(app.world_mut(), client, client, a, Some(b));
    assert!(app.world().get::<QuantumPad>(a).unwrap().charging.is_none());
}

#[test]
fn a_multitool_links_a_closed_pad_to_the_buffered_one() {
    let mut app = fixture("link");
    let a = obj(&mut app, quantum_pad::PAD, t(2, 2), true);
    let b = obj(&mut app, quantum_pad::PAD, t(5, 2), true);
    let tool = app
        .world_mut()
        .spawn((
            DmItem {
                path: "/obj/item/multitool".into(),
                ..default()
            },
            DmTool {
                behaviour: "multitool".into(),
                speed: 1.0,
                ..default()
            },
        ))
        .id();
    let client = app.world_mut().spawn_empty().id();
    let world = app.world_mut();
    // Closed panel, empty buffer: nothing to upload.
    assert!(quantum_pad::item_interaction(
        world, client, client, tool, a
    ));
    assert!(
        world
            .get::<QuantumPad>(a)
            .is_none_or(|p| p.linked.is_none())
    );
    world.entity_mut(b).insert(PanelOpen(true));
    assert!(quantum_pad::item_interaction(
        world, client, client, tool, b
    ));
    // Not onto itself.
    world.entity_mut(b).insert(PanelOpen(false));
    assert!(quantum_pad::item_interaction(
        world, client, client, tool, b
    ));
    assert!(
        world
            .get::<QuantumPad>(b)
            .is_none_or(|p| p.linked.is_none())
    );
    assert!(quantum_pad::item_interaction(
        world, client, client, tool, a
    ));
    assert_eq!(world.get::<QuantumPad>(a).unwrap().linked, Some(b));
}
