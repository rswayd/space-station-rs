//! Chairs and beds over real WebTransport on 127.0.0.1: sitting down on a chair (upright, held in
//! place, facing the chair's way), alt-click turning the chair with its sitter, getting up by
//! resisting, a bar stool lifting its sitter 7 px, a wrench taking a stool apart into its sheet,
//! a wooden chair picked up by dragging it onto yourself and set down again from the hand, and a
//! bed drawing whoever stands on it 8 px up.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::combat::NextMove;
use ss13_game::furniture::Seated;
use ss13_game::health::BodyPosition;
use ss13_game::inventory::{CarriedBy, Inventory};
use ss13_game::medical::surgery::Buckled;
use ss13_game::server::MoveQueue;
use ss13_game::tables::Elevated;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::{DmItem, DmObj};
use ss13_shared::protocol::{ClickModifiers, ClientIntent, DragEnd, InventorySlot};
use ss13_shared::{Dir, Facing, TilePos};
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
    "/obj": { "parent": "/atom/movable", "declared_vars": { "max_integrity": 100,
        "buckle_sound": null, "unbuckle_sound": null } },
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
    "/obj/item/wrench": { "parent": "/obj/item", "declared_vars": {
        "tool_behaviour": "wrench", "toolspeed": 1 } },
    "/obj/item/chair": { "parent": "/obj/item", "declared_vars": {
        "icon": { "resource": "c.dmi" }, "icon_state": "chair_toppled" } },
    "/obj/item/chair/wood": { "parent": "/obj/item/chair", "declared_vars": {
        "icon_state": "wooden_chair_toppled" } },
    "/obj/item/stack": { "parent": "/obj/item", "declared_vars": { "amount": 1, "max_amount": 50 } },
    "/obj/item/stack/sheet": { "parent": "/obj/item/stack" },
    "/obj/item/stack/sheet/iron": { "parent": "/obj/item/stack/sheet" },
    "/obj/item/stack/sheet/mineral": { "parent": "/obj/item/stack/sheet" },
    "/obj/item/stack/sheet/mineral/wood": { "parent": "/obj/item/stack/sheet/mineral" },
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
    "/obj/structure/chair": { "parent": "/obj/structure", "declared_vars": {
        "name": "chair", "icon": { "resource": "c.dmi" }, "icon_state": "chair",
        "buildstacktype": { "path": "/obj/item/stack/sheet/iron" }, "buildstackamount": 1,
        "item_chair": { "path": "/obj/item/chair" } } },
    "/obj/structure/chair/wood": { "parent": "/obj/structure/chair", "declared_vars": {
        "icon_state": "wooden_chair",
        "buildstacktype": { "path": "/obj/item/stack/sheet/mineral/wood" },
        "buildstackamount": 3, "item_chair": { "path": "/obj/item/chair/wood" } } },
    "/obj/structure/chair/stool": { "parent": "/obj/structure/chair", "declared_vars": {
        "icon_state": "stool" } },
    "/obj/structure/chair/stool/bar": { "parent": "/obj/structure/chair/stool",
        "declared_vars": { "icon_state": "bar" } },
    "/obj/structure/bed": { "parent": "/obj/structure", "declared_vars": {
        "name": "bed", "icon": { "resource": "c.dmi" }, "icon_state": "bed",
        "build_stack_type": { "path": "/obj/item/stack/sheet/iron" }, "build_stack_amount": 2,
        "elevation": 8, "can_deconstruct": 1 } },
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

/// Rows top-down, the last line is y = 1. The spawn `s` (2,2); a chair `C` (3,3), a bar stool
/// `S` (4,3), a bed `B` (5,3), a wooden chair `W` (3,2).
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"C" = (/obj/structure/chair,/turf/open/floor,/area/space)
"S" = (/obj/structure/chair/stool/bar,/turf/open/floor,/area/space)
"B" = (/obj/structure/bed,/turf/open/floor,/area/space)
"W" = (/obj/structure/chair/wood,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwwww
wfffffw
wfCSBfw
wsWfffw
wwwwwww
"}
"##;

const CHAIR: TilePos = TilePos::new(3, 3, 1);
const STOOL: TilePos = TilePos::new(4, 3, 1);
const BED: TilePos = TilePos::new(5, 3, 1);
const WOOD: TilePos = TilePos::new(3, 2, 1);

const STATES: [(&str, &str); 12] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
    ("c.dmi", "chair"),
    ("c.dmi", "wooden_chair"),
    ("c.dmi", "stool"),
    ("c.dmi", "bar"),
    ("c.dmi", "bed"),
    ("c.dmi", "chair_toppled"),
    ("c.dmi", "wooden_chair_toppled"),
];

fn fixture_assets() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("furniture_fixture");
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/item/wrench",
        "/obj/item/chair",
        "/obj/item/chair/wood",
        "/obj/item/stack/sheet/iron",
        "/obj/item/stack/sheet/mineral/wood",
        "/obj/structure/chair",
        "/obj/structure/chair/wood",
        "/obj/structure/chair/stool/bar",
        "/obj/structure/bed",
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

    fn pos(&self) -> TilePos {
        *self.server.world().get::<TilePos>(self.mob()).unwrap()
    }

    fn facing(&self, e: Entity) -> Dir {
        self.server
            .world()
            .get::<Facing>(e)
            .map_or(Dir::South, |f| f.0)
    }

    fn step_once(&mut self, dir: Dir) {
        self.players[0].seq += 1;
        let seq = self.players[0].seq;
        self.send(0, ClientIntent::Move { dir, seq });
    }

    fn walk(&mut self, dir: Dir) {
        let to = self.pos().step(dir);
        self.run_until(&format!("walk {dir:?} to {to:?}"), |s| {
            if s.pos() == to {
                return true;
            }
            let applied = s
                .server
                .world()
                .get::<MoveQueue>(s.mob())
                .unwrap()
                .applied();
            if s.players[0].seq == applied {
                s.step_once(dir);
            }
            false
        });
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

    fn item_on(&self, pos: TilePos, path: &str) -> Option<Entity> {
        let world = self.server.world();
        (world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied())
        .find(|&e| world.get::<DmItem>(e).is_some_and(|i| i.path == path))
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

    fn elevated(&self) -> i16 {
        self.server
            .world()
            .get::<Elevated>(self.mob())
            .map_or(0, |e| e.0)
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

    fn held(&self, path: &str) -> Option<(usize, Entity)> {
        let world = self.server.world();
        let inventory = world.get::<Inventory>(self.mob()).unwrap();
        (0..2).find_map(|h| {
            let item = inventory.hands[h]?;
            world
                .get::<DmItem>(item)
                .is_some_and(|i| i.path == path)
                .then_some((h, item))
        })
    }
}

#[test]
fn sit_turn_stand_stools_wrench_fold_and_beds() {
    let mut s = Station::start();
    let mob = s.mob();
    assert_eq!(s.pos(), TilePos::new(2, 2, 1));

    // Dragging yourself onto the chair: seated on its tile, upright, facing its way.
    let chair = s.obj(CHAIR, "/obj/structure/chair").unwrap();
    s.drag(mob, chair);
    s.run_until("seated", |s| s.buckled());
    assert_eq!(s.pos(), CHAIR);
    let seat = *s.get::<Seated>(mob);
    assert_eq!(seat.chair, chair);
    s.run_for(0.3);
    assert!(!s.get::<BodyPosition>(mob).lying(), "a chair seats upright");
    assert_eq!(s.facing(mob), s.facing(chair));

    // Held in place.
    s.step_once(Dir::North);
    s.hold(1.0, "a seated mob does not walk off", |s| s.pos() == CHAIR);

    // Alt-click turns the chair counterclockwise, and its sitter with it.
    let before = s.facing(chair);
    s.click_with(
        0,
        chair,
        ClickModifiers {
            alt: true,
            ..default()
        },
    );
    s.run_until("the chair turned", |s| s.facing(chair) != before);
    assert_eq!(s.facing(chair), Dir::East);
    s.run_until("the sitter turned", |s| s.facing(mob) == Dir::East);

    // Resisting gets up.
    s.send(0, ClientIntent::Resist);
    s.run_until("got up", |s| !s.buckled());
    s.run_until("no longer seated", |s| s.try_get::<Seated>(mob).is_none());

    // A bar stool lifts its sitter 7 px, getting up brings it down.
    let stool = s.obj(STOOL, "/obj/structure/chair/stool/bar").unwrap();
    s.drag(mob, stool);
    s.run_until("on the stool", |s| s.buckled() && s.pos() == STOOL);
    s.run_until("drawn up", |s| s.elevated() == 7);
    s.send(0, ClientIntent::Resist);
    s.run_until("off the stool", |s| !s.buckled());
    s.run_until("drawn down", |s| s.elevated() == 0);

    // A wrench (right click) takes the stool apart into its sheet.
    let wrench = ss13_game::construction::spawn_obj(
        s.server.world_mut(),
        "/obj/item/wrench",
        TilePos::new(6, 4, 1),
    )
    .unwrap();
    s.give(wrench);
    // Resisting started the click cooldown (tg `CLICK_CD_RESIST`): a click now is dropped.
    s.run_until("the click cooldown is over", |s| {
        let now = s.now();
        s.try_get::<NextMove>(s.mob()).is_none_or(|n| now >= n.0)
    });
    s.click_with(
        0,
        stool,
        ClickModifiers {
            right: true,
            ..default()
        },
    );
    s.run_until("the stool is apart", |s| {
        s.server.world().get_entity(stool).is_err()
    });
    assert!(s.item_on(STOOL, "/obj/item/stack/sheet/iron").is_some());

    // The wooden chair dragged onto yourself is picked up as its item.
    let wood = s.obj(WOOD, "/obj/structure/chair/wood").unwrap();
    s.drag(wood, mob);
    s.run_until("picked up", |s| s.held("/obj/item/chair/wood").is_some());
    assert!(s.obj(WOOD, "/obj/structure/chair/wood").is_none());

    // A bed draws whoever stands on it 8 px up.
    assert_eq!(s.pos(), STOOL);
    s.walk(Dir::East);
    assert_eq!(s.pos(), BED);
    s.run_until("up on the bed", |s| s.elevated() == 8);
    s.walk(Dir::West);
    s.run_until("down off the bed", |s| s.elevated() == 0);

    // Used in hand, the item chair stands on the user's tile, facing the user's way.
    let (hand, _) = s.held("/obj/item/chair/wood").unwrap();
    s.server
        .world_mut()
        .get_mut::<Inventory>(mob)
        .unwrap()
        .active = hand;
    let slot = if hand == 0 {
        InventorySlot::LeftHand
    } else {
        InventorySlot::RightHand
    };
    s.send(0, ClientIntent::ClickSlot(slot));
    s.run_until("set down", |s| {
        s.obj(STOOL, "/obj/structure/chair/wood").is_some()
    });
    assert!(s.held("/obj/item/chair/wood").is_none());
    let planted = s.obj(STOOL, "/obj/structure/chair/wood").unwrap();
    assert_eq!(s.facing(planted), s.facing(mob));
}
