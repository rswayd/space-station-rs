//! Walking into things over real WebTransport on 127.0.0.1 (fixture station, two headless
//! clients), tg `/mob/living/Bump`: two mobs swap tiles; a walking mob or one bumping a human in
//! combat mode stays put; a mob in combat mode pushes a peaceful one; a closet is pushed and the
//! mob steps after it; an anchored closet stays.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::combat::CombatMode;
use ss13_game::janitor::Walking;
use ss13_game::obj_damage::Anchored;
use ss13_game::server::MoveQueue;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::DmCloset;
use ss13_shared::protocol::ClientIntent;
use ss13_shared::{Dir, TilePos};

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/datum/storage": { "parent": "/datum", "declared_vars": {
        "max_slots": 7, "max_specific_storage": 3, "max_total_storage": 14 } },
    "/datum/storage/backpack": { "parent": "/datum/storage", "declared_vars": {
        "max_slots": 21, "max_total_storage": 21 } },
    "/datum/storage/box": { "parent": "/datum/storage", "declared_vars": {
        "max_specific_storage": 2 } },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3, "anchored": 0 } },
    "/obj": { "parent": "/atom/movable", "declared_vars": { "req_access": null } },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start" },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null, "w_class": 3, "breakouttime": 0,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null } },
    "/obj/item/toy": { "parent": "/obj/item", "declared_vars": { "w_class": 1 } },
    "/obj/item/pen": { "parent": "/obj/item", "declared_vars": {
        "name": "pen", "icon_state": "pen", "w_class": 1 } },
    "/obj/item/restraints": { "parent": "/obj/item", "declared_vars": { "breakouttime": 600 } },
    "/obj/item/restraints/handcuffs": { "parent": "/obj/item/restraints", "declared_vars": {
        "name": "handcuffs", "icon_state": "handcuff", "breakouttime": 600, "handcuff_time": 40,
        "used": 0 } },
    "/obj/item/card": { "parent": "/obj/item" },
    "/obj/item/card/id": { "parent": "/obj/item/card", "declared_vars": {
        "access": { "list": [] }, "icon_state": "card_grey", "slot_flags": 256 } },
    "/obj/item/card/id/advanced": { "parent": "/obj/item/card/id" },
    "/obj/item/clothing": { "parent": "/obj/item" },
    "/obj/item/clothing/under": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 2 } },
    "/obj/item/clothing/under/color": { "parent": "/obj/item/clothing/under" },
    "/obj/item/clothing/under/color/grey": { "parent": "/obj/item/clothing/under/color" },
    "/obj/item/clothing/shoes": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 128 } },
    "/obj/item/clothing/shoes/sneakers": { "parent": "/obj/item/clothing/shoes" },
    "/obj/item/clothing/shoes/sneakers/black": { "parent": "/obj/item/clothing/shoes/sneakers" },
    "/obj/item/storage": { "parent": "/obj/item", "declared_vars": {
        "storage_type": { "path": "/datum/storage" } } },
    "/obj/item/storage/backpack": { "parent": "/obj/item/storage", "declared_vars": {
        "name": "backpack", "icon_state": "backpack", "slot_flags": 1, "w_class": 4,
        "storage_type": { "path": "/datum/storage/backpack" } } },
    "/obj/item/storage/box": { "parent": "/obj/item/storage", "declared_vars": {
        "name": "box", "icon_state": "box", "w_class": 2,
        "storage_type": { "path": "/datum/storage/box" } } },
    "/obj/item/storage/box/survival": { "parent": "/obj/item/storage/box" },
    "/obj/structure": { "parent": "/obj", "declared_vars": { "anchored": 1 } },
    "/obj/structure/closet": { "parent": "/obj/structure", "declared_vars": {
        "name": "closet", "icon": { "resource": "c.dmi" }, "icon_state": "generic", "anchored": 0,
        "density": 1, "opened": 0, "locked": 0, "welded": 0, "secure": 0, "icon_door": null,
        "icon_welded": "welded", "enable_door_overlay": 1, "has_opened_overlay": 1,
        "has_closed_overlay": 1, "dense_when_open": 0, "wall_mounted": 0, "horizontal": 0,
        "storage_capacity": 30 } },
    "/mob": { "parent": "/atom/movable", "declared_vars": { "layer": 4, "density": 1 } },
    "/mob/living": { "parent": "/mob" },
    "/mob/living/carbon": { "parent": "/mob/living" },
    "/mob/living/carbon/human": { "parent": "/mob/living/carbon", "declared_vars": {
        "icon": { "resource": "h.dmi" }, "icon_state": "human_basic" } },
    "/turf": { "parent": "/atom", "declared_vars": { "icon": { "resource": "t.dmi" } } },
    "/turf/open": { "parent": "/turf" },
    "/turf/open/floor": { "parent": "/turf/open", "declared_vars": {
        "name": "floor", "icon_state": "floor" } },
    "/turf/open/space": { "parent": "/turf/open" },
    "/turf/open/space/basic": { "parent": "/turf/open/space" },
    "/turf/closed": { "parent": "/turf", "declared_vars": { "density": 1, "opacity": 1 } },
    "/turf/closed/wall": { "parent": "/turf/closed", "declared_vars": { "icon_state": "wall" } },
    "/area": { "parent": "/atom" },
    "/area/space": { "parent": "/area" }
  },
  "populate": {
    "/obj/structure/closet": { "PopulateContents": ["return"] },
    "/obj/item/storage/box/survival": { "PopulateContents": [
        { "repeat": { "count": 2, "body": [{ "new": "/obj/item/toy" }] } }] }
  }
}"##;

/// Rows top-down, the last line is y = 1. A spawns at (3,3), B at (3,4); a closet at (4,4).
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"C" = (/obj/structure/closet,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwwwww
wffffffw
wfsCfffw
wfsffffw
wffffffw
wwwwwwww
"}
"##;

const A_SPAWN: TilePos = TilePos::new(3, 3, 1);
const B_SPAWN: TilePos = TilePos::new(3, 4, 1);
const CLOSET: TilePos = TilePos::new(4, 4, 1);

const STATES: [(&str, &str); 6] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("c.dmi", "generic"),
    ("c.dmi", "generic_door"),
];

fn fixture_assets() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("bump_fixture");
    let mut used = vec![
        "/obj/effect/landmark/start/assistant",
        "/obj/structure/closet",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
    ];
    used.extend(ss13_mapload::RUNTIME_CONTAINER_TYPES);
    write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)])
}

struct Station(testing::Station);

ss13_game::drive_station!(Station, 0);

impl Station {
    fn pos(&self, e: Entity) -> TilePos {
        *self.server.world().get::<TilePos>(e).unwrap()
    }

    /// One step of `p`, waited for until the server applied it (and the bump that followed).
    fn step_once(&mut self, p: usize, dir: Dir) {
        let mob = self.players[p].mob;
        self.players[p].seq += 1;
        let seq = self.players[p].seq;
        self.send(p, ClientIntent::Move { dir, seq });
        self.run_until("the step is applied", |s| {
            s.server.world().get::<MoveQueue>(mob).unwrap().applied() == seq
        });
        self.run_for(0.1);
    }

    fn toggle(&mut self, p: usize, intent: ClientIntent, on: impl Fn(&Self, Entity) -> bool) {
        let mob = self.players[p].mob;
        let want = !on(self, mob);
        self.send(p, intent);
        self.run_until("the toggle is applied", |s| on(s, mob) == want);
    }
}

fn combat(s: &Station, mob: Entity) -> bool {
    s.server.world().get::<CombatMode>(mob).is_some_and(|c| c.0)
}

fn walking(s: &Station, mob: Entity) -> bool {
    s.server.world().get::<Walking>(mob).is_some_and(|w| w.0)
}

#[test]
fn mobs_swap_and_push_closets_but_not_fighters_or_anchored_ones() {
    let assets = fixture_assets();
    let mut s = Station(testing::Station::start(
        fixture_map(&assets, &["map.dmm"]),
        |_| {},
    ));
    s.join_all(2, |_| {});
    let (a, b) = (s.players[0].mob, s.players[1].mob);
    assert_eq!((s.pos(a), s.pos(b)), (A_SPAWN, B_SPAWN));
    let closet = {
        let world = s.server.world_mut();
        let mut q = world.query_filtered::<Entity, With<DmCloset>>();
        q.single(world).unwrap()
    };
    assert_eq!(s.pos(closet), CLOSET);

    // A walks north into B: they swap.
    s.step_once(0, Dir::North);
    assert_eq!((s.pos(a), s.pos(b)), (B_SPAWN, A_SPAWN), "A and B swapped");

    // A walking (move intent) does not swap or push.
    s.toggle(0, ClientIntent::ToggleMoveIntent, walking);
    s.step_once(0, Dir::South);
    assert_eq!(
        (s.pos(a), s.pos(b)),
        (B_SPAWN, A_SPAWN),
        "a walking mob stays"
    );
    s.toggle(0, ClientIntent::ToggleMoveIntent, walking);

    // B in combat mode: no swap, and a human in combat mode is not pushed.
    s.toggle(1, ClientIntent::ToggleCombatMode, combat);
    s.step_once(0, Dir::South);
    assert_eq!(
        (s.pos(a), s.pos(b)),
        (B_SPAWN, A_SPAWN),
        "B in combat mode holds"
    );
    s.toggle(1, ClientIntent::ToggleCombatMode, combat);

    // A in combat mode does not swap but pushes peaceful B a tile south and steps after him.
    s.toggle(0, ClientIntent::ToggleCombatMode, combat);
    s.step_once(0, Dir::South);
    assert_eq!(
        (s.pos(a), s.pos(b)),
        (A_SPAWN, TilePos::new(3, 2, 1)),
        "A pushed B"
    );
    s.toggle(0, ClientIntent::ToggleCombatMode, combat);

    // A walks back north and pushes the closet east, stepping after it.
    s.step_once(0, Dir::North);
    assert_eq!(s.pos(a), B_SPAWN);
    s.step_once(0, Dir::East);
    assert_eq!(s.pos(closet), TilePos::new(5, 4, 1), "the closet moved");
    assert_eq!(s.pos(a), CLOSET, "A followed the closet");

    // Anchored, the closet stays and so does A.
    s.server
        .world_mut()
        .entity_mut(closet)
        .insert(Anchored(true));
    s.step_once(0, Dir::East);
    assert_eq!(
        s.pos(closet),
        TilePos::new(5, 4, 1),
        "the anchored closet stays"
    );
    assert_eq!(s.pos(a), CLOSET);
}
