//! Display cases on a fixture map: an ID without the captain's access leaves the captain's case
//! shut; any ID opens a plain case, a toy goes on display and an empty hand takes it out; broken,
//! the case drops a shard, stops being dense and raises its burglar alarm.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::displaycase::{BurglarAlarm, DisplayCase};
use ss13_game::inventory::{CarriedBy, Inventory};
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::DmItem;
use ss13_shared::TilePos;
use ss13_world::{Density, SpatialIndex};

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3, "anchored": 0 } },
    "/obj": { "parent": "/atom/movable", "declared_vars": { "force": 0, "damtype": "brute",
        "obj_flags": 2, "max_integrity": 100 } },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start" },
    "/obj/structure": { "parent": "/obj", "declared_vars": { "anchored": 1 } },
    "/obj/structure/displaycase": { "parent": "/obj/structure", "declared_vars": {
        "name": "display case", "icon": { "resource": "s.dmi" }, "icon_state": "glassbox",
        "density": 1, "max_integrity": 200, "integrity_failure": 0.25 } },
    "/obj/structure/displaycase/captain": { "parent": "/obj/structure/displaycase" },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null, "attack_speed": 8,
        "tool_behaviour": null, "toolspeed": 1 } },
    "/obj/item/toy": { "parent": "/obj/item", "declared_vars": { "name": "toy" } },
    "/obj/item/shard": { "parent": "/obj/item", "declared_vars": { "name": "shard" } },
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

/// Rows top-down, the last line is y = 1. The spawn `s` (3,2) between a plain case `P` (2,2) and
/// the captain's `C` (4,2).
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"P" = (/obj/structure/displaycase,/turf/open/floor,/area/space)
"C" = (/obj/structure/displaycase/captain,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwww
wfffw
wPsCw
wwwww
"}
"##;

const PLAIN: TilePos = TilePos::new(2, 2, 1);
const CAPTAIN: TilePos = TilePos::new(4, 2, 1);

const STATES: [(&str, &str); 8] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
    ("s.dmi", "glassbox"),
    ("s.dmi", "glassbox_closed"),
    ("s.dmi", "glassbox_broken"),
];

fn fixture_assets() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("displaycase_fixture");
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/structure/displaycase",
        "/obj/structure/displaycase/captain",
        "/obj/item/toy",
        "/obj/item/shard",
        "/obj/item/card/id",
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

    fn mob(&self) -> Entity {
        self.players[0].mob
    }

    fn case_on(&self, pos: TilePos) -> Entity {
        let world = self.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .find(|&e| world.get::<DisplayCase>(e).is_some())
            .expect("a display case")
    }

    fn items_on(&self, pos: TilePos, path: &str) -> usize {
        let world = self.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .filter(|&&e| world.get::<DmItem>(e).is_some_and(|i| i.path == path))
            .count()
    }

    fn held(&self) -> Option<Entity> {
        let inventory = self.get::<Inventory>(self.mob());
        inventory.hands[inventory.active]
    }

    /// A new `path` item straight into the active hand, the old one gone.
    fn give_new(&mut self, path: &str) -> Entity {
        let mob = self.mob();
        let old = self.held();
        let world = self.server.world_mut();
        if let Some(old) = old {
            world.despawn(old);
        }
        let item = ss13_game::construction::spawn_obj(world, path, TilePos::new(3, 3, 1)).unwrap();
        world
            .entity_mut(item)
            .remove::<TilePos>()
            .insert(CarriedBy(mob));
        let mut inventory = world.get_mut::<Inventory>(mob).unwrap();
        let hand = inventory.active;
        inventory.hands[hand] = Some(item);
        item
    }
}

#[test]
fn cases_open_by_access_and_hold_a_showpiece() {
    let mut s = Station::start();
    let (plain, captain) = (s.case_on(PLAIN), s.case_on(CAPTAIN));
    assert!(s.get::<DisplayCase>(plain).alert);
    assert_eq!(s.get::<DisplayCase>(captain).req_access, vec!["captain"]);
    // An ID without access does not open the captain's case.
    s.give_new("/obj/item/card/id");
    s.run_for(1.0);
    s.click(0, captain);
    s.run_for(1.0);
    assert!(!s.get::<DisplayCase>(captain).open, "access denied");
    // A plain case asks for no access.
    s.click(0, plain);
    s.run_until("plain case open", |s| s.get::<DisplayCase>(plain).open);
    // Open and empty, a toy goes on display.
    let toy = s.give_new("/obj/item/toy");
    s.run_for(1.0);
    s.click(0, plain);
    s.run_until("toy on display", |s| {
        s.get::<DisplayCase>(plain).showpiece == Some(toy)
    });
    assert!(s.held().is_none());
    // An empty hand takes it out onto the floor.
    s.run_for(1.0);
    s.click(0, plain);
    s.run_until("toy out", |s| s.items_on(PLAIN, "/obj/item/toy") == 1);
    assert!(s.get::<DisplayCase>(plain).showpiece.is_none());
}

#[test]
fn a_broken_case_sounds_the_alarm() {
    let mut s = Station::start();
    let plain = s.case_on(PLAIN);
    assert!(s.try_get::<Density>(plain).is_some());
    s.run_for(1.0);
    let world = s.server.world_mut();
    let hit = ss13_game::obj_damage::take_damage(
        world,
        plain,
        160.0,
        ss13_game::health::DamageType::Brute,
    );
    assert!(hit.broke, "a quarter of 200 integrity is left");
    s.run_for(0.5);
    assert!(
        s.try_get::<Density>(plain).is_none(),
        "broken glass lets through"
    );
    assert_eq!(s.items_on(PLAIN, "/obj/item/shard"), 1);
    assert!(s.try_get::<BurglarAlarm>(plain).is_some());
}
