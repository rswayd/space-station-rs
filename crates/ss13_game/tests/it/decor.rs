//! Flora, posters and showcases over real WebTransport on 127.0.0.1: a saw chops a tree into
//! logs and a stump, a shovel uproots a bush; a poster is ripped, another rolled up with
//! wirecutters and hung back on a wall; a showcase is unfastened, unscrewed and pried into iron.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::decor::{RolledPoster, ShowcaseUnscrewed};
use ss13_game::flora::Uprooted;
use ss13_game::inventory::{CarriedBy, Inventory};
use ss13_game::obj_damage::Anchored;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::{DmItem, DmObj, DmTurfBuild};
use ss13_shared::{Appearance, TilePos};
use ss13_world::SpatialIndex;

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
    "/obj/structure/flora": { "parent": "/obj/structure", "declared_vars": {
        "name": "flora", "icon": { "resource": "fl.dmi" }, "icon_state": "bush" } },
    "/obj/structure/flora/tree": { "parent": "/obj/structure/flora", "declared_vars": {
        "name": "tree", "icon_state": "tree", "density": 1 } },
    "/obj/structure/flora/tree/stump": { "parent": "/obj/structure/flora/tree",
        "declared_vars": { "name": "stump", "icon_state": "stump", "density": 0 } },
    "/obj/structure/flora/bush": { "parent": "/obj/structure/flora", "declared_vars": {
        "name": "bush" } },
    "/obj/structure/sign": { "parent": "/obj/structure", "declared_vars": {
        "icon": { "resource": "p.dmi" } } },
    "/obj/structure/sign/poster": { "parent": "/obj/structure/sign", "declared_vars": {
        "name": "poster", "icon_state": "poster" } },
    "/obj/structure/sign/poster/ripped": { "parent": "/obj/structure/sign/poster",
        "declared_vars": { "name": "ripped poster", "icon_state": "poster_ripped" } },
    "/obj/structure/showcase": { "parent": "/obj/structure", "declared_vars": {
        "name": "showcase", "icon": { "resource": "fl.dmi" }, "icon_state": "showcase",
        "density": 1 } },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null, "attack_speed": 8,
        "tool_behaviour": null, "toolspeed": 1 } },
    "/obj/item/circular_saw": { "parent": "/obj/item", "declared_vars": {
        "tool_behaviour": "saw" } },
    "/obj/item/shovel": { "parent": "/obj/item", "declared_vars": {
        "tool_behaviour": "shovel" } },
    "/obj/item/wirecutters": { "parent": "/obj/item", "declared_vars": {
        "tool_behaviour": "cutters" } },
    "/obj/item/screwdriver": { "parent": "/obj/item", "declared_vars": {
        "tool_behaviour": "screwdriver" } },
    "/obj/item/wrench": { "parent": "/obj/item", "declared_vars": {
        "tool_behaviour": "wrench" } },
    "/obj/item/crowbar": { "parent": "/obj/item", "declared_vars": {
        "tool_behaviour": "crowbar" } },
    "/obj/item/grown": { "parent": "/obj/item" },
    "/obj/item/grown/log": { "parent": "/obj/item/grown" },
    "/obj/item/grown/log/tree": { "parent": "/obj/item/grown/log" },
    "/obj/item/poster": { "parent": "/obj/item", "declared_vars": {
        "icon": { "resource": "p.dmi" }, "icon_state": "rolled_poster" } },
    "/obj/item/stack": { "parent": "/obj/item", "declared_vars": { "amount": 1, "max_amount": 50 } },
    "/obj/item/stack/sheet": { "parent": "/obj/item/stack" },
    "/obj/item/stack/sheet/iron": { "parent": "/obj/item/stack/sheet" },
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

/// Rows top-down, the last line is y = 1. The spawn `s` (3,2) with a poster on it, a tree `T`
/// (2,2), a bush `B` (4,2), a poster `P` (3,3) and a showcase `C` (4,3).
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/obj/structure/sign/poster,/turf/open/floor,/area/space)
"T" = (/obj/structure/flora/tree,/turf/open/floor,/area/space)
"B" = (/obj/structure/flora/bush,/turf/open/floor,/area/space)
"P" = (/obj/structure/sign/poster,/turf/open/floor,/area/space)
"C" = (/obj/structure/showcase,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwww
wfPCfw
wTsBfw
wwwwww
"}
"##;

const SPAWN: TilePos = TilePos::new(3, 2, 1);
const TREE: TilePos = TilePos::new(2, 2, 1);
const BUSH: TilePos = TilePos::new(4, 2, 1);
const POSTER: TilePos = TilePos::new(3, 3, 1);
const SHOWCASE: TilePos = TilePos::new(4, 3, 1);
const SOUTH_WALL: TilePos = TilePos::new(3, 1, 1);

const STATES: [(&str, &str); 12] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
    ("fl.dmi", "bush"),
    ("fl.dmi", "tree"),
    ("fl.dmi", "stump"),
    ("fl.dmi", "showcase"),
    ("p.dmi", "poster"),
    ("p.dmi", "poster_ripped"),
    ("p.dmi", "rolled_poster"),
];

fn fixture_assets() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("decor_fixture");
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/structure/flora/tree",
        "/obj/structure/flora/tree/stump",
        "/obj/structure/flora/bush",
        "/obj/structure/sign/poster",
        "/obj/structure/sign/poster/ripped",
        "/obj/structure/showcase",
        "/obj/item/circular_saw",
        "/obj/item/shovel",
        "/obj/item/wirecutters",
        "/obj/item/screwdriver",
        "/obj/item/wrench",
        "/obj/item/crowbar",
        "/obj/item/grown/log/tree",
        "/obj/item/poster",
        "/obj/item/stack/sheet/iron",
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

    fn obj_on(&self, pos: TilePos, path: &str) -> Option<Entity> {
        let world = self.server.world();
        (world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied())
        .find(|&e| world.get::<DmObj>(e).is_some_and(|o| o.path == path))
    }

    fn items_on(&self, pos: TilePos, path: &str) -> usize {
        let world = self.server.world();
        (world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied())
        .filter(|&e| world.get::<DmItem>(e).is_some_and(|i| i.path == path))
        .count()
    }

    fn turf_at(&mut self, pos: TilePos) -> Entity {
        let world = self.server.world_mut();
        let mut q = world.query::<(Entity, &TilePos, &DmTurfBuild)>();
        q.iter(world)
            .find(|(_, p, _)| **p == pos)
            .map(|(e, ..)| e)
            .unwrap()
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
        let item = ss13_game::construction::spawn_obj(world, path, TilePos::new(5, 3, 1)).unwrap();
        world
            .entity_mut(item)
            .remove::<TilePos>()
            .insert(CarriedBy(mob));
        let mut inventory = world.get_mut::<Inventory>(mob).unwrap();
        let hand = inventory.active;
        inventory.hands[hand] = Some(item);
        item
    }

    fn switch_hands(&mut self) {
        let mob = self.mob();
        let mut inventory = self.server.world_mut().get_mut::<Inventory>(mob).unwrap();
        inventory.active = 1 - inventory.active;
    }
}

#[test]
fn chop_a_tree_and_uproot_a_bush() {
    let mut s = Station::start();
    let tree = s.obj_on(TREE, "/obj/structure/flora/tree").unwrap();
    s.give_new("/obj/item/circular_saw");
    s.run_for(1.0);
    s.click(0, tree);
    s.run_for(3.0);
    assert!(
        s.obj_on(TREE, "/obj/structure/flora/tree").is_some(),
        "still chopping"
    );
    s.run_until("chopped down", |s| {
        s.obj_on(TREE, "/obj/structure/flora/tree").is_none()
    });
    let logs = s.items_on(TREE, "/obj/item/grown/log/tree");
    assert!((6..=10).contains(&logs), "{logs} logs");
    assert!(s.obj_on(TREE, "/obj/structure/flora/tree/stump").is_some());

    let bush = s.obj_on(BUSH, "/obj/structure/flora/bush").unwrap();
    s.give_new("/obj/item/shovel");
    s.run_for(1.0);
    s.click(0, bush);
    s.run_until("uprooted", |s| s.try_get::<Uprooted>(bush).is_some());
    assert!(!s.get::<Anchored>(bush).0);
    assert!(matches!(s.get::<Appearance>(bush).rotation, 90 | 270));
}

#[test]
fn rip_roll_and_hang_posters() {
    let mut s = Station::start();
    // An empty hand rips the poster to the north.
    let poster = s.obj_on(POSTER, "/obj/structure/sign/poster").unwrap();
    s.run_for(1.0);
    s.click(0, poster);
    s.run_until("ripped", |s| {
        s.obj_on(POSTER, "/obj/structure/sign/poster/ripped")
            .is_some()
    });
    assert!(s.obj_on(POSTER, "/obj/structure/sign/poster").is_none());

    // Wirecutters clear the remains away.
    let ripped = s
        .obj_on(POSTER, "/obj/structure/sign/poster/ripped")
        .unwrap();
    s.give_new("/obj/item/wirecutters");
    s.run_for(1.0);
    s.click(0, ripped);
    s.run_until("cleared", |s| {
        s.obj_on(POSTER, "/obj/structure/sign/poster/ripped")
            .is_none()
    });

    // ...and roll the whole one under the user into the other hand.
    let mine = s.obj_on(SPAWN, "/obj/structure/sign/poster").unwrap();
    s.run_for(1.0);
    s.click(0, mine);
    s.run_until("rolled up", |s| {
        s.obj_on(SPAWN, "/obj/structure/sign/poster").is_none()
    });
    s.switch_hands();
    let rolled = s.held().expect("the rolled poster in the other hand");
    assert_eq!(
        s.get::<RolledPoster>(rolled).structure,
        "/obj/structure/sign/poster"
    );

    // Hung on the south wall: on the user's tile, drawn on that wall.
    let wall = s.turf_at(SOUTH_WALL);
    s.run_for(1.0);
    s.click(0, wall);
    s.run_until("hung", |s| {
        s.obj_on(SPAWN, "/obj/structure/sign/poster").is_some()
    });
    let hung = s.obj_on(SPAWN, "/obj/structure/sign/poster").unwrap();
    assert_eq!(s.get::<Appearance>(hung).pixel_offset, [0, -32]);
    assert!(s.held().is_none(), "the rolled poster is used up");
}

#[test]
fn take_a_showcase_apart() {
    let mut s = Station::start();
    let showcase = s.obj_on(SHOWCASE, "/obj/structure/showcase").unwrap();
    // Anchored: the screwdriver does nothing; the wrench unfastens it.
    s.give_new("/obj/item/screwdriver");
    s.run_for(1.0);
    s.click(0, showcase);
    s.run_for(1.0);
    assert!(s.try_get::<ShowcaseUnscrewed>(showcase).is_none());
    s.give_new("/obj/item/wrench");
    s.run_for(1.0);
    s.click(0, showcase);
    s.run_until("unfastened", |s| !s.get::<Anchored>(showcase).0);
    s.give_new("/obj/item/screwdriver");
    s.run_for(1.0);
    s.click(0, showcase);
    s.run_until("unscrewed", |s| {
        s.try_get::<ShowcaseUnscrewed>(showcase).is_some()
    });
    s.give_new("/obj/item/crowbar");
    s.run_for(1.0);
    s.click(0, showcase);
    s.run_until("pried apart", |s| {
        s.obj_on(SHOWCASE, "/obj/structure/showcase").is_none()
    });
    assert_eq!(s.items_on(SHOWCASE, "/obj/item/stack/sheet/iron"), 1);
}
