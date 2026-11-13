//! Sealed cars over real WebTransport on 127.0.0.1: climbing into the speedwagon by dragging
//! yourself onto it makes you its driver, its steps carry you, "Climb Out" lets you out; a
//! non-clown in the clown car is kidnapped (no driving) and pushes out of the trunk.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::actions::{ActionButtonsUi, TRIGGER};
use ss13_game::cars::{Car, CarOccupant, DRIVE, KIDNAPPED};
use ss13_game::closets::InCloset;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::DmObj;
use ss13_shared::protocol::{ClientIntent, DragEnd};
use ss13_shared::{Dir, TilePos};
use ss13_tgui::UiInterface;
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
    "/obj/vehicle/sealed": { "parent": "/obj/vehicle" },
    "/obj/vehicle/sealed/car": { "parent": "/obj/vehicle/sealed", "declared_vars": {
        "layer": 4.1 } },
    "/obj/vehicle/sealed/car/clowncar": { "parent": "/obj/vehicle/sealed/car",
        "declared_vars": { "name": "clown car", "icon_state": "clowncar",
        "max_integrity": 150 } },
    "/obj/vehicle/sealed/car/speedwagon": { "parent": "/obj/vehicle/sealed/car",
        "declared_vars": { "name": "BM Speedwagon", "icon_state": "speedwagon" } },
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
    "/area/space": { "parent": "/area" },
    "/area/station": { "parent": "/area" }
  }
}"##;

/// Rows top-down, the last line is y = 1. The spawn `s` (2,2), the speedwagon `W` (3,2), the
/// clown car `C` (6,3).
const MAP: &str = r##""w" = (/turf/closed/wall,/area/station)
"f" = (/turf/open/floor,/area/station)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/station)
"W" = (/obj/vehicle/sealed/car/speedwagon,/turf/open/floor,/area/station)
"C" = (/obj/vehicle/sealed/car/clowncar,/turf/open/floor,/area/station)

(1,1,1) = {"
wwwwwwwww
wfffffffw
wffffCffw
wsWfffffw
wwwwwwwww
"}
"##;

const WAGON: TilePos = TilePos::new(3, 2, 1);
const CLOWN_CAR: TilePos = TilePos::new(6, 3, 1);

const STATES: [(&str, &str); 7] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
    ("v.dmi", "clowncar"),
    ("v.dmi", "speedwagon"),
];

fn fixture_assets() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("cars_fixture");
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/vehicle/sealed/car/speedwagon",
        "/obj/vehicle/sealed/car/clowncar",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
        "/area/station",
    ];
    write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)])
}

struct Station(testing::Station);

ss13_game::drive_station!(Station, 0);
use crate::harness::Client;
impl Client for Station {}

impl Station {
    fn start() -> Self {
        let assets = fixture_assets();
        let mut s = testing::Station::start(fixture_map(&assets, &["map.dmm"]), |_| {});
        // The client records what it receives, for the action buttons window.
        s.join_all(1, |app| {
            app.init_resource::<crate::harness::Received>()
                .add_systems(Update, crate::harness::record);
        });
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

    fn seat(&self) -> Option<CarOccupant> {
        self.try_get::<CarOccupant>(self.mob()).copied()
    }

    fn trigger(&mut self, id: &str) {
        let window = self
            .window(0, ActionButtonsUi::NAME)
            .expect("action buttons");
        self.act(0, window, TRIGGER, &[("id", id)]);
    }

    fn walk(&mut self, dir: Dir) {
        let to = self.pos().step(dir);
        self.run_until(&format!("walk {dir:?} to {to:?}"), |s| {
            if s.pos() == to {
                return true;
            }
            s.try_step(0, dir);
            false
        });
    }
}

#[test]
fn speedwagon_drives_and_clown_car_kidnaps() {
    let mut s = Station::start();
    let mob = s.mob();
    assert_eq!(s.pos(), TilePos::new(2, 2, 1));

    // Dragging yourself onto the speedwagon: in at once, its driver, hidden on its tile.
    let wagon = s.obj(WAGON, "/obj/vehicle/sealed/car/speedwagon").unwrap();
    s.drag(mob, wagon);
    s.run_until("in the speedwagon", |s| s.seat().is_some());
    let seat = s.seat().unwrap();
    assert_eq!(seat.car, wagon);
    assert!(seat.flags & DRIVE != 0, "the first one in drives");
    assert_eq!(s.pos(), WAGON);
    assert!(
        s.try_get::<InCloset>(mob)
            .is_some_and(|c| c.closet == wagon)
    );
    assert_eq!(s.get::<Car>(wagon).occupants, vec![mob]);

    // Its steps are the car's; the driver comes along.
    let east = WAGON.step(Dir::East);
    s.run_until("drove east", |s| {
        if s.pos_of(wagon) == east {
            return true;
        }
        s.try_step(0, Dir::East);
        false
    });
    s.run_until("the driver came along", |s| s.pos() == east);

    // "Climb Out": out on the car's tile, the car empty again.
    s.wait_window(0, ActionButtonsUi::NAME);
    s.trigger("car:climb_out");
    s.run_until("out of the speedwagon", |s| s.seat().is_none());
    assert!(s.try_get::<InCloset>(mob).is_none());
    assert!(s.get::<Car>(wagon).occupants.is_empty());

    // Off the speedwagon's tile (it is dense) and next to the clown car.
    s.walk(Dir::North);
    s.walk(Dir::East);
    assert_eq!(s.pos(), TilePos::new(5, 3, 1));

    // An assistant is no clown: in the clown car after its 2 s, kidnapped, not driving.
    let clown_car = s
        .obj(CLOWN_CAR, "/obj/vehicle/sealed/car/clowncar")
        .unwrap();
    s.drag(mob, clown_car);
    s.run_until("in the clown car", |s| s.seat().is_some());
    let seat = s.seat().unwrap();
    assert_eq!(seat.car, clown_car);
    assert_eq!(seat.flags, KIDNAPPED);
    s.try_step(0, Dir::East);
    s.hold(1.0, "a kidnapped occupant cannot drive", |s| {
        s.pos_of(clown_car) == CLOWN_CAR
    });

    // Out of the trunk takes `escape_time`.
    s.trigger("car:climb_out");
    s.hold(3.0, "still pushing at the trunk", |s| s.seat().is_some());
    s.run_until("out of the trunk", |s| s.seat().is_none());
    assert_eq!(s.pos(), CLOWN_CAR);
}
