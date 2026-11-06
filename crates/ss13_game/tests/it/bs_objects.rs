//! BandaStation's modpack objects over real WebTransport on 127.0.0.1: the paper shredder eats a
//! paper and a folder and gives shredded paper back on a right click, a wrench unfastens it; a
//! flag folds into its item in the hands on a right click.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::inventory::{CarriedBy, Inventory};
use ss13_game::obj_damage::Anchored;
use ss13_game::papershredder::ShredderBin;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::{DmItem, DmObj};
use ss13_shared::TilePos;
use ss13_shared::protocol::ClickModifiers;
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
    "/obj/structure/sign": { "parent": "/obj/structure", "declared_vars": {
        "icon": { "resource": "fl.dmi" } } },
    "/obj/structure/sign/flag": { "parent": "/obj/structure/sign", "declared_vars": {
        "name": "flag", "icon_state": "flag" } },
    "/obj/structure/sign/flag/nanotrasen": { "parent": "/obj/structure/sign/flag",
        "declared_vars": { "name": "flag of Nanotrasen", "icon_state": "flag_nt" } },
    "/obj/machinery": { "parent": "/obj", "declared_vars": { "use_power": 0,
        "density": 1, "anchored": 1 } },
    "/obj/machinery/papershredder": { "parent": "/obj/machinery", "declared_vars": {
        "name": "paper shredder", "icon": { "resource": "ps.dmi" },
        "icon_state": "papershredder0" } },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null, "attack_speed": 8,
        "tool_behaviour": null, "toolspeed": 1 } },
    "/obj/item/wrench": { "parent": "/obj/item", "declared_vars": {
        "tool_behaviour": "wrench" } },
    "/obj/item/photo": { "parent": "/obj/item" },
    "/obj/item/folder": { "parent": "/obj/item" },
    "/obj/item/shredded_paper": { "parent": "/obj/item", "declared_vars": {
        "name": "shredded paper", "icon": { "resource": "ps.dmi" }, "icon_state": "shredp" } },
    "/obj/item/sign": { "parent": "/obj/item" },
    "/obj/item/sign/flag": { "parent": "/obj/item/sign", "declared_vars": {
        "name": "folded flag" } },
    "/obj/item/sign/flag/nanotrasen": { "parent": "/obj/item/sign/flag", "declared_vars": {
        "name": "folded flag of the Nanotrasen" } },
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

/// Rows top-down, the last line is y = 1. The spawn `s` (3,2), the shredder `S` (4,2) and a
/// flag `F` (3,3).
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"S" = (/obj/machinery/papershredder,/turf/open/floor,/area/space)
"F" = (/obj/structure/sign/flag/nanotrasen,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwww
wfFffw
wfsSfw
wwwwww
"}
"##;

const SHREDDER: TilePos = TilePos::new(4, 2, 1);
const FLAG: TilePos = TilePos::new(3, 3, 1);

const STATES: [(&str, &str); 12] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
    ("fl.dmi", "flag"),
    ("fl.dmi", "flag_nt"),
    ("ps.dmi", "papershredder0"),
    ("ps.dmi", "papershredder1"),
    ("ps.dmi", "papershredder2"),
    ("ps.dmi", "papershredder3"),
    ("ps.dmi", "shredp"),
];

fn fixture_assets() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("bs_objects_fixture");
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/machinery/papershredder",
        "/obj/structure/sign/flag/nanotrasen",
        "/obj/item/wrench",
        "/obj/item/photo",
        "/obj/item/folder",
        "/obj/item/shredded_paper",
        "/obj/item/sign/flag",
        "/obj/item/sign/flag/nanotrasen",
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

    fn held(&self) -> Option<Entity> {
        let inventory = self.get::<Inventory>(self.mob());
        inventory.hands[inventory.active]
    }

    fn held_path(&self) -> Option<String> {
        let world = self.server.world();
        let inventory = world.get::<Inventory>(self.mob())?;
        inventory
            .hands
            .iter()
            .flatten()
            .find_map(|&i| world.get::<DmItem>(i).map(|d| d.path.clone()))
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

    fn bin(&self, shredder: Entity) -> u32 {
        self.try_get::<ShredderBin>(shredder).map_or(0, |b| b.0)
    }
}

fn right() -> ClickModifiers {
    ClickModifiers {
        right: true,
        ..default()
    }
}

#[test]
fn shred_documents_and_empty_the_bin() {
    let mut s = Station::start();
    let shredder = s.obj_on(SHREDDER, "/obj/machinery/papershredder").unwrap();
    // A photo (1) and a folder (4).
    let photo = s.give_new("/obj/item/photo");
    s.run_for(1.0);
    s.click(0, shredder);
    s.run_until("the photo is shredded", |s| {
        s.server.world().get_entity(photo).is_err()
    });
    assert_eq!(s.bin(shredder), 1);
    s.give_new("/obj/item/folder");
    s.run_for(1.0);
    s.click(0, shredder);
    s.run_until("the folder is shredded", |s| s.bin(shredder) == 5);
    assert!(s.held().is_none());

    // A right click with an empty hand drops one shredded paper.
    s.run_for(1.0);
    s.click_with(0, shredder, right());
    s.run_until("one shred out", |s| s.bin(shredder) == 4);
    assert_eq!(s.items_on(SHREDDER, "/obj/item/shredded_paper"), 1);

    // A wrench unfastens it (2 s).
    s.give_new("/obj/item/wrench");
    s.run_for(1.0);
    s.click(0, shredder);
    s.run_until("unfastened", |s| !s.get::<Anchored>(shredder).0);
}

#[test]
fn fold_a_flag_into_the_hands() {
    let mut s = Station::start();
    let flag = s
        .obj_on(FLAG, "/obj/structure/sign/flag/nanotrasen")
        .unwrap();
    s.run_for(1.0);
    s.click_with(0, flag, right());
    s.run_until("folded", |s| s.server.world().get_entity(flag).is_err());
    assert_eq!(
        s.held_path().as_deref(),
        Some("/obj/item/sign/flag/nanotrasen")
    );
}
