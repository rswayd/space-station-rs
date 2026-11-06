//! Engineering machines of `ss13_game::eng_machines` on a fixture floor, server side: machines
//! and items are spawned directly and the modules' procs called with a stand-in client.

use std::path::Path;

use bevy::prelude::*;
use ss13_game::eng_machines::sheetifier::{self, MEAT_SHEET, PROCESS_SECS, Sheetifier};
use ss13_game::testing::{fixture_map, server_app, steps, write_fixture};
use ss13_game::tools::Stack;
use ss13_mapload::{DmFood, DmItem, DmObj};
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
    "/obj/item/stack": { "parent": "/obj/item", "declared_vars": { "amount": 1,
        "max_amount": 50, "merge_type": null, "singular_name": null, "turf_type": null } },
    "/obj/item/stack/sheet": { "parent": "/obj/item/stack" },
    "/obj/item/stack/sheet/meat": { "parent": "/obj/item/stack/sheet", "declared_vars": {
        "name": "meat", "merge_type": { "path": "/obj/item/stack/sheet/meat" } } },
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
    "/area/space": { "parent": "/area" }
  }
}"##;

const MAP: &str = r##""f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)

(1,1,1) = {"
ffffff
ffffff
sfffff
"}
"##;

const STATES: [(&str, &str); 3] = [
    ("t.dmi", "floor"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
];

fn fixture(test: &str) -> App {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("eng_machines_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/item/stack/sheet/meat",
        "/turf/open/floor",
        "/turf/open/space/basic",
        "/area/space",
    ];
    let assets = write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)]);
    server_app(fixture_map(&assets, &["map.dmm"]), |_| {})
}

fn t(x: i32, y: i32) -> TilePos {
    TilePos::new(x, y, 1)
}

fn obj(app: &mut App, path: &str, pos: TilePos) -> Entity {
    app.world_mut()
        .spawn((
            pos,
            DmObj {
                path: path.into(),
                anchored: true,
                ..default()
            },
        ))
        .id()
}

fn meat(app: &mut App, path: &str, sheets: f32) -> Entity {
    app.world_mut()
        .spawn((
            DmItem {
                path: path.into(),
                ..default()
            },
            DmFood {
                meat_sheets: sheets,
                ..default()
            },
        ))
        .id()
}

fn meat_sheets_on(app: &mut App, pos: TilePos) -> u32 {
    let world = app.world_mut();
    let mut q = world.query::<(&DmItem, &TilePos, &Stack)>();
    q.iter(world)
        .filter(|(i, p, _)| i.path == MEAT_SHEET && **p == pos)
        .map(|(_, _, s)| s.amount)
        .sum()
}

fn run(app: &mut App, secs: f64) {
    for _ in 0..steps(secs) {
        app.update();
    }
}

#[test]
fn sheetifier_turns_meat_into_sheets_after_processing() {
    let mut app = fixture("sheetifier");
    let machine = obj(&mut app, sheetifier::SHEETIFIER, t(3, 2));
    sheetifier::setup(app.world_mut(), machine);
    let client = app.world_mut().spawn_empty().id();
    let slab = meat(&mut app, "/obj/item/food/meat/slab", 4.0);
    let cutlet = meat(&mut app, "/obj/item/food/meat/cutlet", 4.0 / 3.0);
    let plant = meat(&mut app, "/obj/item/food/meat/slab/human/mutant/plant", 0.0);
    let burger = meat(&mut app, "/obj/item/food/burger", 1.0);

    // A left click with a non-stack is not an insert (`on_item_insert`).
    let w = app.world_mut();
    assert!(!sheetifier::item_interaction(
        w, client, client, slab, machine, false
    ));
    // Not meat: rejected and kept; meat without meat material: kept.
    assert!(sheetifier::item_interaction(
        w, client, client, burger, machine, true
    ));
    assert!(sheetifier::item_interaction(
        w, client, client, plant, machine, true
    ));
    assert!(w.get_entity(burger).is_ok() && w.get_entity(plant).is_ok());

    assert!(sheetifier::item_interaction(
        w, client, client, slab, machine, true
    ));
    assert!(w.get_entity(slab).is_err(), "the slab is consumed");
    let s = *w.get::<Sheetifier>(machine).unwrap();
    assert_eq!(s.meat, 4.0);
    assert!(s.busy_until.is_some());
    // Busy: the cutlet is blocked and stays.
    assert!(sheetifier::item_interaction(
        w, client, client, cutlet, machine, true
    ));
    assert!(w.get_entity(cutlet).is_ok());

    run(&mut app, PROCESS_SECS - 0.5);
    assert_eq!(meat_sheets_on(&mut app, t(3, 2)), 0, "still processing");
    run(&mut app, 1.0);
    assert_eq!(meat_sheets_on(&mut app, t(3, 2)), 4);
    let s = *app.world().get::<Sheetifier>(machine).unwrap();
    assert!(s.busy_until.is_none() && s.meat == 0.0);

    // Four thirds of a sheet: one comes out, the third stays inside.
    let w = app.world_mut();
    assert!(sheetifier::item_interaction(
        w, client, client, cutlet, machine, true
    ));
    run(&mut app, PROCESS_SECS + 0.5);
    assert_eq!(meat_sheets_on(&mut app, t(3, 2)), 5);
    let left = app.world().get::<Sheetifier>(machine).unwrap().meat;
    assert!((left - 1.0 / 3.0).abs() < 1e-3, "{left}");
}
