//! Ridden vehicles over real WebTransport on 127.0.0.1: mounting a secway by dragging yourself
//! onto it, no driving without its key, the key used on it goes in, driving carries the rider,
//! alt-click takes the key back, resisting gets off; a skateboard bounces off a wall once and
//! throws its exhausted skater the second time.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::combat::NextMove;
use ss13_game::inventory::{CarriedBy, Inventory};
use ss13_game::medical::surgery::Buckled;
use ss13_game::status_effects::StatusEffects;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_game::vehicles::{Rider, Vehicle};
use ss13_mapload::DmObj;
use ss13_shared::protocol::{ClickModifiers, ClientIntent, DragEnd};
use ss13_shared::{Dir, TilePos};
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
    "/obj": { "parent": "/atom/movable", "declared_vars": { "max_integrity": 100 } },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start" },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null } },
    "/obj/item/key": { "parent": "/obj/item", "declared_vars": {
        "name": "key", "icon": { "resource": "v.dmi" }, "icon_state": "key" } },
    "/obj/item/key/security": { "parent": "/obj/item/key", "declared_vars": {
        "icon_state": "keysec" } },
    "/obj/item/oar": { "parent": "/obj/item", "declared_vars": {
        "name": "oar", "icon": { "resource": "v.dmi" }, "icon_state": "oar" } },
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
    "/obj/vehicle": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "v.dmi" }, "icon_state": "error", "density": 1,
        "max_integrity": 300, "layer": 3.9 } },
    "/obj/vehicle/ridden": { "parent": "/obj/vehicle" },
    "/obj/vehicle/ridden/secway": { "parent": "/obj/vehicle/ridden", "declared_vars": {
        "name": "secway", "icon_state": "secway", "max_integrity": 60 } },
    "/obj/vehicle/ridden/lavaboat": { "parent": "/obj/vehicle/ridden", "declared_vars": {
        "name": "lava boat", "icon_state": "goliath_boat", "density": 0 } },
    "/obj/vehicle/ridden/scooter": { "parent": "/obj/vehicle/ridden", "declared_vars": {
        "name": "scooter", "icon_state": "scooter" } },
    "/obj/vehicle/ridden/scooter/skateboard": { "parent": "/obj/vehicle/ridden/scooter",
        "declared_vars": { "name": "skateboard", "icon_state": "skateboard", "density": 0 } },
    "/mob": { "parent": "/atom/movable", "declared_vars": { "layer": 4, "density": 1 } },
    "/mob/living": { "parent": "/mob" },
    "/mob/living/carbon": { "parent": "/mob/living" },
    "/mob/living/carbon/human": { "parent": "/mob/living/carbon", "declared_vars": {
        "icon": { "resource": "h.dmi" }, "icon_state": "human_basic" } },
    "/turf": { "parent": "/atom", "declared_vars": { "icon": { "resource": "t.dmi" } } },
    "/turf/open": { "parent": "/turf" },
    "/turf/open/floor": { "parent": "/turf/open", "declared_vars": { "icon_state": "floor" } },
    "/turf/open/water": { "parent": "/turf/open", "declared_vars": { "icon_state": "water" } },
    "/turf/open/space": { "parent": "/turf/open" },
    "/turf/open/space/basic": { "parent": "/turf/open/space" },
    "/turf/closed": { "parent": "/turf", "declared_vars": { "density": 1, "opacity": 1 } },
    "/turf/closed/wall": { "parent": "/turf/closed", "declared_vars": { "icon_state": "wall" } },
    "/area": { "parent": "/atom" },
    "/area/space": { "parent": "/area" },
    "/area/station": { "parent": "/area" }
  }
}"##;

/// Rows top-down, the last line is y = 1. The spawn `s` (2,2), a secway `V` (3,2), a skateboard
/// `B` (8,3) against the east wall. A station area: vehicles need gravity.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/station)
"f" = (/turf/open/floor,/area/station)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/station)
"V" = (/obj/vehicle/ridden/secway,/turf/open/floor,/area/station)
"B" = (/obj/vehicle/ridden/scooter/skateboard,/turf/open/floor,/area/station)

(1,1,1) = {"
wwwwwwwww
wfffffffw
wffffffBw
wsVfffffw
wwwwwwwww
"}
"##;

/// A lavaboat `L` (3,2) on water `~` next to the spawn; the floor north of the water.
const BOAT_MAP: &str = r##""w" = (/turf/closed/wall,/area/station)
"f" = (/turf/open/floor,/area/station)
"~" = (/turf/open/water,/area/station)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/station)
"L" = (/obj/vehicle/ridden/lavaboat,/turf/open/water,/area/station)

(1,1,1) = {"
wwwwww
wfff~w
wsL~~w
wwwwww
"}
"##;

const BOAT: TilePos = TilePos::new(3, 2, 1);

const SECWAY: TilePos = TilePos::new(3, 2, 1);
const BOARD: TilePos = TilePos::new(8, 3, 1);

const STATES: [(&str, &str); 13] = [
    ("t.dmi", "water"),
    ("v.dmi", "oar"),
    ("v.dmi", "goliath_boat"),
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
    ("v.dmi", "secway"),
    ("v.dmi", "skateboard"),
    ("v.dmi", "scooter"),
    ("v.dmi", "key"),
    ("v.dmi", "keysec"),
];

fn fixture_assets() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("vehicles_fixture");
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/item/key/security",
        "/obj/item/oar",
        "/obj/vehicle/ridden/lavaboat",
        "/turf/open/water",
        "/obj/vehicle/ridden/secway",
        "/obj/vehicle/ridden/scooter/skateboard",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
        "/area/station",
    ];
    write_fixture(
        &dir,
        TYPES,
        &used,
        &STATES,
        &[("map.dmm", MAP), ("boat.dmm", BOAT_MAP)],
    )
}

struct Station(testing::Station);

ss13_game::drive_station!(Station, 0);

impl Station {
    fn start() -> Self {
        Self::start_on("map.dmm")
    }

    fn start_on(map: &str) -> Self {
        let assets = fixture_assets();
        let mut s = testing::Station::start(fixture_map(&assets, &[map]), |_| {});
        s.join_all(1, |_| {});
        Self(s)
    }

    fn mob(&self) -> Entity {
        self.players[0].mob
    }

    fn pos_of(&self, e: Entity) -> TilePos {
        *self.server.world().get::<TilePos>(e).unwrap()
    }

    fn pos(&self) -> TilePos {
        self.pos_of(self.mob())
    }

    fn obj(&self, pos: TilePos, path: &str) -> Option<Entity> {
        let world = self.server.world();
        (world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied())
        .find(|&e| world.get::<DmObj>(e).is_some_and(|o| o.path == path))
    }

    fn drag(&mut self, from: Entity, to: Entity) {
        let from = self.net_id(0, from).unwrap();
        let to = self.net_id(0, to).unwrap();
        self.send(
            0,
            ClientIntent::Drag {
                from: DragEnd::Atom(from),
                to: DragEnd::Atom(to),
                modifiers: default(),
            },
        );
    }

    fn buckled(&self) -> bool {
        self.server.world().get::<Buckled>(self.mob()).is_some()
    }

    /// Steps `dir` until `done`.
    fn step_until(&mut self, what: &str, dir: Dir, done: impl Fn(&Self) -> bool) {
        self.run_until(what, |s| {
            if done(s) {
                return true;
            }
            s.try_step(0, dir);
            false
        });
    }

    fn walk(&mut self, dir: Dir) {
        let to = self.pos().step(dir);
        self.step_until(&format!("walk {dir:?} to {to:?}"), dir, |s| s.pos() == to);
    }

    fn wait_click_cooldown(&mut self) {
        self.run_until("the click cooldown is over", |s| {
            let now = s.now();
            s.try_get::<NextMove>(s.mob()).is_none_or(|n| now >= n.0)
        });
    }

    /// Puts `item` straight into the active hand.
    fn give(&mut self, item: Entity) {
        let mob = self.mob();
        let world = self.server.world_mut();
        world
            .entity_mut(item)
            .remove::<TilePos>()
            .insert(CarriedBy(mob));
        let mut inventory = world.get_mut::<Inventory>(mob).unwrap();
        let hand = inventory.active;
        inventory.hands[hand] = Some(item);
    }

    fn holds(&self, item: Entity) -> bool {
        self.server
            .world()
            .get::<Inventory>(self.mob())
            .unwrap()
            .hands
            .contains(&Some(item))
    }
}

#[test]
fn secway_key_drive_dismount_and_skateboard_crashes() {
    let mut s = Station::start();
    let mob = s.mob();
    assert_eq!(s.pos(), TilePos::new(2, 2, 1));

    // Dragging yourself onto the secway: its driver, on its tile.
    let secway = s.obj(SECWAY, "/obj/vehicle/ridden/secway").unwrap();
    s.drag(mob, secway);
    s.run_until("on the secway", |s| s.buckled());
    assert_eq!(s.pos(), SECWAY);
    assert_eq!(s.get::<Rider>(mob).vehicle, secway);
    assert_eq!(s.get::<Vehicle>(secway).rider, Some(mob));

    // No key, no driving.
    s.try_step(0, Dir::East);
    s.hold(1.0, "a secway without its key stays", |s| {
        s.pos_of(secway) == SECWAY && s.pos() == SECWAY
    });

    // The key used on it goes in.
    let key =
        ss13_game::construction::spawn_obj(s.server.world_mut(), "/obj/item/key/security", SECWAY)
            .unwrap();
    s.give(key);
    s.click(0, secway);
    s.run_until("the key is in", |s| {
        s.get::<Vehicle>(secway).key == Some(key)
    });
    assert!(!s.holds(key));

    // Driving: the secway steps and carries its driver.
    let east = SECWAY.step(Dir::East);
    s.step_until("drove east", Dir::East, |s| s.pos_of(secway) == east);
    s.run_until("the driver came along", |s| s.pos() == east);
    assert!(s.buckled(), "still riding");

    // Alt-click by the driver takes the key back.
    s.wait_click_cooldown();
    s.click_with(
        0,
        secway,
        ClickModifiers {
            alt: true,
            ..default()
        },
    );
    s.run_until("the key is back in hand", |s| s.holds(key));
    assert_eq!(s.get::<Vehicle>(secway).key, None);

    // Resisting gets off; the secway is free again.
    s.send(0, ClientIntent::Resist);
    s.run_until("off the secway", |s| !s.buckled());
    s.run_until("no longer its rider", |s| {
        s.try_get::<Rider>(mob).is_none() && s.get::<Vehicle>(secway).rider.is_none()
    });

    // On foot to the skateboard, then onto it.
    for dir in [Dir::East, Dir::East, Dir::East] {
        s.walk(dir);
    }
    assert_eq!(s.pos(), TilePos::new(7, 2, 1));
    let board = s
        .obj(BOARD, "/obj/vehicle/ridden/scooter/skateboard")
        .unwrap();
    s.wait_click_cooldown();
    s.drag(mob, board);
    s.run_until("on the skateboard", |s| s.buckled() && s.pos() == BOARD);

    // Into the wall at run intent: stamina spent, bounced back a tile, still on.
    s.step_until("bounced off the wall", Dir::East, |s| {
        s.try_get::<StatusEffects>(mob)
            .is_some_and(|e| e.stamina > 0.0)
    });
    let back = BOARD.step(Dir::West);
    s.run_until("bounced back", |s| {
        s.pos_of(board) == back && s.pos() == back
    });
    assert!(s.buckled(), "the first crash keeps the skater on");

    // Back to the wall and into it again: exhausted, thrown off and paralyzed.
    s.step_until("thrown off", Dir::East, |s| !s.buckled());
    assert!(s.get::<StatusEffects>(mob).paralyze > 0.0);
    s.run_until("no longer a skater", |s| {
        s.try_get::<Rider>(mob).is_none() && s.get::<Vehicle>(board).rider.is_none()
    });
}

/// tg `lavaboat.dm`: the oar attaches as its key; it goes over water (or lava) and off it onto
/// the shore once, never from land to land.
#[test]
fn lavaboat_needs_an_oar_and_water() {
    let mut s = Station::start_on("boat.dmm");
    let mob = s.mob();
    let boat = s.obj(BOAT, "/obj/vehicle/ridden/lavaboat").unwrap();
    s.drag(mob, boat);
    s.run_until("in the boat", |s| s.buckled() && s.pos() == BOAT);

    s.try_step(0, Dir::East);
    s.hold(1.0, "no oar, no rowing", |s| s.pos_of(boat) == BOAT);

    let oar =
        ss13_game::construction::spawn_obj(s.server.world_mut(), "/obj/item/oar", BOAT).unwrap();
    s.give(oar);
    s.click(0, boat);
    s.run_until("the oar is attached", |s| {
        s.get::<Vehicle>(boat).key == Some(oar)
    });

    let water = BOAT.step(Dir::East);
    s.step_until("rowed east over water", Dir::East, |s| {
        s.pos_of(boat) == water
    });
    let shore = water.step(Dir::North);
    s.step_until("beached on the shore", Dir::North, |s| {
        s.pos_of(boat) == shore
    });
    s.run_until("the rower came along", |s| s.pos() == shore);
    s.try_step(0, Dir::West);
    s.hold(1.0, "no rowing over land", |s| s.pos_of(boat) == shore);
    assert!(s.buckled());
}
