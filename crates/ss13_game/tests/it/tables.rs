//! Tables and racks over real WebTransport on 127.0.0.1, two players: a grabbed mob slammed onto
//! a table, climbing and walking table to table (drawn 12 px up), flipping and standing a table
//! up, taking a table apart and building it again on its frame, a rack into parts and back, a
//! glass table breaking under a climber, a shove onto a table.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::combat::CombatMode;
use ss13_game::inventory::{CarriedBy, Inventory};
use ss13_game::pull::{GrabState, Pulling};
use ss13_game::server::MoveQueue;
use ss13_game::status_effects::StatusEffects;
use ss13_game::tables::{Elevated, TableState};
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::{DmItem, DmObj};
use ss13_shared::protocol::{ClickModifiers, ClientIntent, DragEnd, InventorySlot};
use ss13_shared::{Appearance, Dir, TilePos};
use ss13_world::{Border, Density, SpatialIndex};

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
    "/obj/item/toy": { "parent": "/obj/item" },
    "/obj/item/screwdriver": { "parent": "/obj/item", "declared_vars": {
        "tool_behaviour": "screwdriver", "toolspeed": 1 } },
    "/obj/item/wrench": { "parent": "/obj/item", "declared_vars": {
        "tool_behaviour": "wrench", "toolspeed": 1 } },
    "/obj/item/shard": { "parent": "/obj/item" },
    "/obj/item/rack_parts": { "parent": "/obj/item" },
    "/obj/item/stack": { "parent": "/obj/item", "declared_vars": { "amount": 1, "max_amount": 50 } },
    "/obj/item/stack/rods": { "parent": "/obj/item/stack" },
    "/obj/item/stack/sheet": { "parent": "/obj/item/stack" },
    "/obj/item/stack/sheet/iron": { "parent": "/obj/item/stack/sheet", "declared_vars": {
        "table_type": { "path": "/obj/structure/table" } } },
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
        "pass_flags_self": 65, "frame": { "path": "/obj/structure/table_frame" },
        "framestack": { "path": "/obj/item/stack/rods" }, "framestackamount": 2,
        "buildstack": { "path": "/obj/item/stack/sheet/iron" }, "buildstackamount": 1,
        "deconstruction_ready": 1, "can_flip": 1,
        "flipped_table_icon": { "resource": "f.dmi" }, "base_icon_state": "table",
        "slam_gently": 0, "glass_shard_type": { "path": "/obj/item/shard" } } },
    "/obj/structure/table/glass": { "parent": "/obj/structure/table" },
    "/obj/structure/table_frame": { "parent": "/obj/structure", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "frame", "anchored": 0,
        "framestack": { "path": "/obj/item/stack/rods" }, "framestackamount": 2 } },
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

/// Rows top-down, the last line is y = 1. Spawns `s` (2,3) and (2,2); tables `A` (3,3) and
/// (9,2), `B` a table with a toy on it (4,3), a glass table `G` (4,4), a rack `R` (6,4).
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"A" = (/obj/structure/table,/turf/open/floor,/area/space)
"B" = (/obj/item/toy,/obj/structure/table,/turf/open/floor,/area/space)
"G" = (/obj/structure/table/glass,/turf/open/floor,/area/space)
"R" = (/obj/structure/rack,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwwwwwww
wffffffffw
wffffffffw
wffGfRfffw
wsABfffffw
wsffffffAw
wwwwwwwwww
"}
"##;

const SLAM_TABLE: TilePos = TilePos::new(3, 3, 1);
const TOY_TABLE: TilePos = TilePos::new(4, 3, 1);
const GLASS: TilePos = TilePos::new(4, 4, 1);
const RACK: TilePos = TilePos::new(6, 4, 1);
const SHOVE_TABLE: TilePos = TilePos::new(9, 2, 1);

const STATES: [(&str, &str); 10] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
    ("i.dmi", "table"),
    ("i.dmi", "frame"),
    ("i.dmi", "rack"),
    ("f.dmi", "table"),
    ("l.dmi", "toy"),
];

fn fixture_assets() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("tables_fixture");
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/item/toy",
        "/obj/item/screwdriver",
        "/obj/item/wrench",
        "/obj/item/shard",
        "/obj/item/rack_parts",
        "/obj/item/stack/rods",
        "/obj/item/stack/sheet/iron",
        "/obj/structure/table",
        "/obj/structure/table/glass",
        "/obj/structure/table_frame",
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
        s.join_all(2, |_| {});
        Self(s)
    }

    fn mob(&self, p: usize) -> Entity {
        self.players[p].mob
    }

    fn pos(&self, p: usize) -> TilePos {
        *self.server.world().get::<TilePos>(self.mob(p)).unwrap()
    }

    fn walk(&mut self, p: usize, dir: Dir) {
        let to = self.pos(p).step(dir);
        self.run_until(&format!("walk {dir:?} to {to:?}"), |s| {
            if s.pos(p) == to {
                return true;
            }
            let mob = s.mob(p);
            let applied = s.server.world().get::<MoveQueue>(mob).unwrap().applied();
            if s.players[p].seq == applied {
                s.players[p].seq += 1;
                let seq = s.players[p].seq;
                s.send(p, ClientIntent::Move { dir, seq });
            }
            false
        });
    }

    /// Every entity on `pos` matching `filter`.
    fn all_on(&self, pos: TilePos, filter: impl Fn(EntityRef) -> bool) -> Vec<Entity> {
        let world = self.server.world();
        (world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied())
        .filter(|&e| filter(world.entity(e)))
        .collect()
    }

    fn obj(&self, pos: TilePos, root: &str) -> Option<Entity> {
        self.all_on(pos, |e| {
            e.get::<DmObj>()
                .is_some_and(|o| o.path == root || o.path.starts_with(&format!("{root}/")))
        })
        .first()
        .copied()
    }

    fn item(&self, pos: TilePos, path: &str) -> Option<Entity> {
        self.all_on(pos, |e| e.get::<DmItem>().is_some_and(|i| i.path == path))
            .first()
            .copied()
    }

    /// Puts `item` straight into `p`'s active hand.
    fn give(&mut self, p: usize, item: Entity) {
        let mob = self.mob(p);
        let world = self.server.world_mut();
        world
            .entity_mut(item)
            .remove::<TilePos>()
            .insert(CarriedBy(mob));
        let mut inventory = world.get_mut::<Inventory>(mob).unwrap();
        let hand = inventory.active;
        inventory.hands[hand] = Some(item);
    }

    fn right_click(&mut self, p: usize, target: Entity) {
        let modifiers = ClickModifiers {
            right: true,
            ..default()
        };
        self.click_with(p, target, modifiers);
    }

    fn drag_self_onto(&mut self, p: usize, target: Entity) {
        let me = self.net_id(p, self.mob(p)).unwrap();
        let onto = self.net_id(p, target).unwrap();
        self.send(
            p,
            ClientIntent::Drag {
                from: DragEnd::Atom(me),
                to: DragEnd::Atom(onto),
                modifiers: default(),
            },
        );
    }

    fn elevated(&self, p: usize) -> i16 {
        self.server
            .world()
            .get::<Elevated>(self.mob(p))
            .map_or(0, |e| e.0)
    }

    fn knocked_down(&self, p: usize) -> bool {
        self.server
            .world()
            .get::<StatusEffects>(self.mob(p))
            .is_some_and(|e| e.knockdown > 0.0)
    }
}

#[test]
fn slam_climb_flip_take_apart_racks_glass_and_shoves() {
    let mut s = Station::start();
    // Who spawned where: player 0 on (2,3), next to the slam table.
    if s.pos(0) != TilePos::new(2, 3, 1) {
        s.players.swap(0, 1);
    }
    assert_eq!(s.pos(0), TilePos::new(2, 3, 1));
    let (a, b) = (s.mob(0), s.mob(1));

    // An aggressive grab slams the other player onto the table.
    let slam_table = s.obj(SLAM_TABLE, "/obj/structure/table").unwrap();
    s.server.world_mut().entity_mut(a).insert((
        Pulling(b),
        GrabState::Aggressive,
        CombatMode(true),
    ));
    s.click(0, slam_table);
    s.run_until("slammed onto the table", |s| s.pos(1) == SLAM_TABLE);
    assert!(s.knocked_down(1));
    assert!(s.server.world().get::<Pulling>(a).is_none(), "let go");
    s.server.world_mut().entity_mut(a).insert(CombatMode(false));

    // Climbing up: 2 s, then 12 px up; on to the next table, down onto the floor.
    s.drag_self_onto(0, slam_table);
    s.run_until("climbed up", |s| s.pos(0) == SLAM_TABLE);
    s.run_until("drawn up", |s| s.elevated(0) == 12);
    let pixel_y = s.server.world().get::<Appearance>(a).unwrap().pixel_offset[1];
    assert_eq!(pixel_y, 12);
    s.run_for(2.5); // the climb stun
    s.walk(0, Dir::East);
    assert_eq!(s.pos(0), TOY_TABLE, "table to table");
    s.walk(0, Dir::East);
    s.run_until("down again", |s| s.elevated(0) == 0);

    // B gets off the slam table (the toy would hit it).
    s.run_until("B up again", |s| !s.knocked_down(1));
    s.walk(1, Dir::South);

    // Flipped from the east: the toy flies west over the next table, the west edge blocks.
    let table = s.obj(TOY_TABLE, "/obj/structure/table").unwrap();
    let toy = s.item(TOY_TABLE, "/obj/item/toy").unwrap();
    s.right_click(0, table);
    s.run_until("flipped", |s| {
        s.server
            .world()
            .get::<TableState>(table)
            .is_some_and(|t| t.flipped.is_some())
    });
    let world = s.server.world();
    assert_eq!(
        world.get::<Border>(table),
        Some(&Border {
            edges: Dir::West as u8
        })
    );
    assert!(world.get::<Density>(table).is_none());
    assert_eq!(world.get::<TilePos>(toy), Some(&SLAM_TABLE));
    s.right_click(0, table);
    s.run_until("stood up", |s| {
        s.server.world().get::<Density>(table).is_some()
    });
    assert!(s.server.world().get::<Border>(table).is_none());

    // A screwdriver leaves the frame and the top's sheet; the sheet builds it again.
    let screwdriver = spawn_item(&mut s, "/obj/item/screwdriver");
    s.give(0, screwdriver);
    s.right_click(0, table);
    s.run_until("the top is off", |s| {
        s.obj(TOY_TABLE, "/obj/structure/table_frame").is_some()
    });
    let frame = s.obj(TOY_TABLE, "/obj/structure/table_frame").unwrap();
    assert!(s.server.world().get_entity(table).is_err());
    let sheet = s.item(TOY_TABLE, "/obj/item/stack/sheet/iron").unwrap();
    s.send(0, ClientIntent::DropActive);
    s.run_until("screwdriver down", |s| {
        s.server.world().get::<TilePos>(screwdriver).is_some()
    });
    s.give(0, sheet);
    s.click(0, frame);
    s.run_until("built again", |s| {
        s.obj(TOY_TABLE, "/obj/structure/table")
            .is_some_and(|t| s.server.world().get::<TableState>(t).is_some())
    });
    assert!(
        s.server.world().get_entity(sheet).is_err(),
        "the sheet was used"
    );

    // A wrench takes it apart: the sheet and the frame's two rods, no frame.
    let table = s.obj(TOY_TABLE, "/obj/structure/table").unwrap();
    let wrench = spawn_item(&mut s, "/obj/item/wrench");
    s.give(0, wrench);
    s.right_click(0, table);
    s.run_until("taken apart", |s| {
        s.server.world().get_entity(table).is_err()
    });
    assert!(s.item(TOY_TABLE, "/obj/item/stack/sheet/iron").is_some());
    assert!(s.item(TOY_TABLE, "/obj/item/stack/rods").is_some());
    assert!(s.obj(TOY_TABLE, "/obj/structure/table_frame").is_none());

    // The rack into parts and the parts into a rack on the user's tile.
    let rack = s.obj(RACK, "/obj/structure/rack").unwrap();
    s.right_click(0, rack);
    s.run_until("rack parts", |s| {
        s.item(RACK, "/obj/item/rack_parts").is_some()
    });
    let parts = s.item(RACK, "/obj/item/rack_parts").unwrap();
    s.send(0, ClientIntent::DropActive);
    s.run_until("wrench down", |s| {
        s.server.world().get::<TilePos>(wrench).is_some()
    });
    s.give(0, parts);
    let hand = if s.server.world().get::<Inventory>(a).unwrap().active == 0 {
        InventorySlot::LeftHand
    } else {
        InventorySlot::RightHand
    };
    s.send(0, ClientIntent::ClickSlot(hand));
    let here = s.pos(0);
    s.run_until("a rack again", |s| {
        s.obj(here, "/obj/structure/rack").is_some()
    });
    assert!(s.server.world().get_entity(parts).is_err());
    s.run_until("on the new rack, up", |s| {
        s.elevated(0) == 12 || s.pos(0) != here
    });

    // Climbing a glass table breaks it: its frame and a shard stay, the climber is paralyzed.
    let glass = s.obj(GLASS, "/obj/structure/table/glass").unwrap();
    s.drag_self_onto(0, glass);
    s.run_until("the glass breaks", |s| {
        s.server.world().get_entity(glass).is_err()
    });
    assert_eq!(s.pos(0), GLASS);
    assert!(s.obj(GLASS, "/obj/structure/table_frame").is_some());
    assert!(s.item(GLASS, "/obj/item/shard").is_some());
    assert!(
        s.server
            .world()
            .get::<StatusEffects>(a)
            .is_some_and(|e| e.paralyze > 0.0)
    );

    // A shove into a table knocks down onto it.
    for _ in 0..5 {
        s.walk(1, Dir::East);
    }
    assert_eq!(s.pos(1), TilePos::new(8, 2, 1));
    s.run_until("A can move", |s| {
        s.server
            .world()
            .get::<StatusEffects>(a)
            .is_none_or(|e| e.paralyze <= 0.0 && e.stun <= 0.0)
    });
    for dir in [Dir::South, Dir::South, Dir::East, Dir::East, Dir::East] {
        s.walk(0, dir);
    }
    assert_eq!(s.pos(0), TilePos::new(7, 2, 1));
    s.right_click(0, b);
    s.run_until("shoved onto the table", |s| s.pos(1) == SHOVE_TABLE);
    assert!(s.knocked_down(1));
}

/// A new `path` item on the server, off the map.
fn spawn_item(s: &mut Station, path: &str) -> Entity {
    let pos = TilePos::new(9, 6, 1);
    ss13_game::construction::spawn_obj(s.server.world_mut(), path, pos).unwrap()
}
