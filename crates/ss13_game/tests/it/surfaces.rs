//! Tables and racks over real WebTransport on 127.0.0.1: an item goes onto a table at the clicked
//! point (tg `table_place_act`) but not in combat mode, onto a rack centered; Q drops it near the
//! tile's center; reach passes a table's corner; a throw flies over a table.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::inventory::Inventory;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::{DmItem, Turf};
use ss13_shared::protocol::{ClickModifiers, ClientIntent};
use ss13_shared::{Appearance, TilePos};
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
    "/obj": { "parent": "/atom/movable" },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start" },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null, "throw_range": 7, "throw_speed": 2,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null } },
    "/obj/item/toy": { "parent": "/obj/item" },
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
    "/obj/structure": { "parent": "/obj", "declared_vars": { "anchored": 1 } },
    "/obj/structure/table": { "parent": "/obj/structure", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "table", "density": 1, "layer": 2.8,
        "pass_flags_self": 65 } },
    "/obj/structure/rack": { "parent": "/obj/structure", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "rack", "density": 1, "layer": 2.8,
        "pass_flags_self": 64 } },
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
    "/turf/closed": { "parent": "/turf", "declared_vars": { "density": 1, "opacity": 1 } },
    "/turf/closed/wall": { "parent": "/turf/closed", "declared_vars": { "icon_state": "wall" } },
    "/area": { "parent": "/atom" },
    "/area/space": { "parent": "/area" }
  }
}"##;

/// Rows top-down, the last line is y = 1. The player spawns on `s` (2,3) with a table `A` east
/// (3,3), a wall north (2,4), a toy `G` past both corners (3,4), a toy `T` south (2,2) and a
/// rack `R` (3,2).
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"T" = (/obj/item/toy,/turf/open/floor,/area/space)
"G" = (/obj/item/toy,/turf/open/floor,/area/space)
"A" = (/obj/structure/table,/turf/open/floor,/area/space)
"R" = (/obj/structure/rack,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwwwww
wffffffw
wwGffffw
wsAffffw
wTRffffw
wwwwwwww
"}
"##;

const SPAWN: TilePos = TilePos::new(2, 3, 1);
const TABLE: TilePos = TilePos::new(3, 3, 1);
const PAST_CORNER: TilePos = TilePos::new(3, 4, 1);
const TOY: TilePos = TilePos::new(2, 2, 1);
const RACK: TilePos = TilePos::new(3, 2, 1);
const BEYOND: TilePos = TilePos::new(6, 3, 1);

const STATES: [(&str, &str); 8] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
    ("i.dmi", "table"),
    ("i.dmi", "rack"),
    ("l.dmi", "toy"),
];

fn fixture_assets() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("surfaces_fixture");
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/item/toy",
        "/obj/structure/table",
        "/obj/structure/rack",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
    ];
    write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)])
}

struct Station(testing::Station);

ss13_game::drive_station!(Station, 0);

impl Station {
    fn start() -> Self {
        let assets = fixture_assets();
        let mut s = testing::Station::start(fixture_map(&assets, &["map.dmm"]), |_| {});
        s.join_all(1, |_| {});
        Self(s)
    }

    fn held(&self) -> Option<Entity> {
        self.server
            .world()
            .get::<Inventory>(self.players[0].mob)
            .unwrap()
            .active_item()
    }

    /// The first entity on `pos` matching `filter`.
    fn on(&self, pos: TilePos, filter: impl Fn(EntityRef) -> bool) -> Entity {
        let world = self.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .find(|&e| filter(world.entity(e)))
            .unwrap_or_else(|| panic!("nothing matching on {pos:?}"))
    }

    fn at(&self, item: Entity) -> Option<TilePos> {
        self.server.world().get::<TilePos>(item).copied()
    }

    fn offset(&self, item: Entity) -> [i16; 2] {
        self.server
            .world()
            .get::<Appearance>(item)
            .unwrap()
            .pixel_offset
    }

    fn click_at(&mut self, target: Entity, pixel: [i8; 2]) {
        let modifiers = ClickModifiers {
            pixel: Some(pixel),
            ..default()
        };
        self.click_with(0, target, modifiers);
    }

    fn take(&mut self, item: Entity) {
        self.click(0, item);
        self.run_until("picked up", |s| s.held() == Some(item));
    }
}

#[test]
fn tables_racks_drops_reach_and_throws() {
    let mut s = Station::start();
    let toy = s.on(TOY, |e| e.contains::<DmItem>());
    let table = s.on(TABLE, |e| !e.contains::<DmItem>() && !e.contains::<Turf>());
    let rack = s.on(RACK, |e| !e.contains::<DmItem>() && !e.contains::<Turf>());
    s.take(toy);

    // Onto the table where it was clicked; a click past the tile's edge stays on the tile.
    s.click_at(table, [5, -3]);
    s.run_until("on the table", |s| s.at(toy) == Some(TABLE));
    assert_eq!(s.offset(toy), [5, -3]);
    s.take(toy);
    s.click_at(table, [40, -40]);
    s.run_until("on the table again", |s| s.at(toy) == Some(TABLE));
    assert_eq!(s.offset(toy), [16, -16]);
    s.take(toy);

    // In combat mode a click on the table is an attack, not a placement.
    s.send(0, ClientIntent::ToggleCombatMode);
    s.run_for(0.2);
    s.click_at(table, [0, 0]);
    s.hold(0.5, "kept in hand", |s| s.held() == Some(toy));
    s.send(0, ClientIntent::ToggleCombatMode);
    s.run_for(0.2);

    // Onto the rack, centered.
    s.click_at(rack, [7, 7]);
    s.run_until("on the rack", |s| s.at(toy) == Some(RACK));
    assert_eq!(s.offset(toy), [0, 0]);
    s.take(toy);

    // Q: at the feet, up to 6 pixels off.
    s.send(0, ClientIntent::DropActive);
    s.run_until("dropped", |s| s.at(toy) == Some(SPAWN));
    let [x, y] = s.offset(toy);
    assert!(x.abs() <= 6 && y.abs() <= 6, "{x} {y}");

    // A diagonal reach between the wall and the table.
    let past = s.on(PAST_CORNER, |e| e.contains::<DmItem>());
    s.take(past);

    // Thrown over the table onto the floor beyond it.
    let floor = s.on(BEYOND, |e| e.contains::<Turf>());
    s.send(0, ClientIntent::ToggleThrowMode);
    s.run_for(0.2);
    s.click(0, floor);
    s.run_until("landed beyond the table", |s| s.at(past) == Some(BEYOND));
}

#[test]
fn resting_lies_down_at_once_and_gets_up_after_a_second() {
    use ss13_game::health::BodyPosition;
    use ss13_world::Density;

    let mut s = Station::start();
    let mob = s.players[0].mob;
    let lying = |s: &Station| {
        s.server
            .world()
            .get::<BodyPosition>(mob)
            .is_some_and(|b| b.lying())
    };
    s.send(0, ClientIntent::ToggleRest);
    s.run_until("lying down", |s| lying(s));
    s.run_until("not dense", |s| {
        s.server.world().get::<Density>(mob).is_none()
    });
    s.send(0, ClientIntent::ToggleRest);
    s.hold(0.8, "getting up takes a second", lying);
    s.run_until("standing", |s| !lying(s));
    assert!(s.server.world().get::<Density>(mob).is_some());
}
