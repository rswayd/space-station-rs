//! Handheld devices over real WebTransport on 127.0.0.1: a megaphone in the active hand makes
//! the holder's words bold, then needs 5 s to recharge; two pair pinpointers from one box are
//! linked, and one turned on points at the other.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::devices::{Megaphone, PairPinpointer};
use ss13_game::inventory::{CarriedBy, Inventory};
use ss13_game::storage::StoredIn;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::DmItem;
use ss13_net::chat::client::ChatReceived;
use ss13_shared::chat::Span;
use ss13_shared::protocol::{ClientIntent, InventorySlot};
use ss13_shared::{Appearance, Dir, TilePos};

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/datum/storage": { "parent": "/datum", "declared_vars": {
        "max_slots": 7, "max_specific_storage": 3, "max_total_storage": 14 } },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3, "anchored": 0 } },
    "/obj": { "parent": "/atom/movable" },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start" },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null, "w_class": 2,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null } },
    "/obj/item/megaphone": { "parent": "/obj/item", "declared_vars": {
        "name": "мегафон", "icon_state": "megaphone" } },
    "/obj/item/pinpointer": { "parent": "/obj/item", "declared_vars": {
        "icon": { "resource": "p.dmi" }, "icon_state": "pinpointer" } },
    "/obj/item/pinpointer/pair": { "parent": "/obj/item/pinpointer" },
    "/obj/item/storage": { "parent": "/obj/item", "declared_vars": {
        "storage_type": { "path": "/datum/storage" } } },
    "/obj/item/storage/box": { "parent": "/obj/item/storage", "declared_vars": {
        "icon_state": "box" } },
    "/obj/item/storage/box/pinpointer_pairs": { "parent": "/obj/item/storage/box" },
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
  },
  "populate": {
    "/obj/item/storage/box/pinpointer_pairs": { "PopulateContents": [
        { "new": "/obj/item/pinpointer/pair" }, { "new": "/obj/item/pinpointer/pair" }] }
  }
}"##;

/// Rows top-down, the last line is y = 1. The spawn `s` (2,2), a megaphone `M` (3,3), a box of
/// pair pinpointers `P` (5,2).
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"M" = (/obj/item/megaphone,/turf/open/floor,/area/space)
"P" = (/obj/item/storage/box/pinpointer_pairs,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwwww
wfMfffw
wsffPfw
wwwwwww
"}
"##;

const STATES: [(&str, &str); 14] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
    ("i.dmi", "megaphone"),
    ("i.dmi", "box"),
    ("p.dmi", "pinpointer"),
    ("p.dmi", "pinonnull"),
    ("p.dmi", "pinondirect"),
    ("p.dmi", "pinonclose"),
    ("p.dmi", "pinonmedium"),
    ("p.dmi", "pinonfar"),
    ("p.dmi", "pinonalert"),
];

fn fixture_assets() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("devices_fixture");
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/item/megaphone",
        "/obj/item/pinpointer/pair",
        "/obj/item/storage/box/pinpointer_pairs",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
    ];
    write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)])
}

/// Every chat line the client got, as its segments.
#[derive(Resource, Default)]
struct Heard(Vec<Vec<(Span, String)>>);

fn record(mut chat: MessageReader<ChatReceived>, mut heard: ResMut<Heard>) {
    for ChatReceived(message) in chat.read() {
        heard.0.push(
            message
                .segments
                .iter()
                .map(|s| (s.span, s.text.clone()))
                .collect(),
        );
    }
}

struct Station(testing::Station);

ss13_game::drive_station!(Station, 0);

impl Station {
    fn start() -> Self {
        let assets = fixture_assets();
        // The say intents are `ss13_chat`'s; the test server has no chat by default.
        let mut s = testing::Station::start(fixture_map(&assets, &["map.dmm"]), |app| {
            app.add_plugins(ss13_chat::server::ChatServerPlugin);
        });
        s.join_all(1, |app| {
            app.init_resource::<Heard>().add_systems(Update, record);
        });
        Self(s)
    }

    fn mob(&self) -> Entity {
        self.players[0].mob
    }

    fn items(&mut self, path: &str) -> Vec<Entity> {
        let world = self.server.world_mut();
        let mut q = world.query::<(Entity, &DmItem)>();
        let mut found: Vec<Entity> = q
            .iter(world)
            .filter(|(_, i)| i.path == path)
            .map(|(e, _)| e)
            .collect();
        found.sort();
        found
    }

    /// Puts `item` straight into the active hand.
    fn give(&mut self, item: Entity) {
        let mob = self.mob();
        let world = self.server.world_mut();
        world
            .entity_mut(item)
            .remove::<(TilePos, StoredIn)>()
            .insert(CarriedBy(mob));
        let mut inventory = world.get_mut::<Inventory>(mob).unwrap();
        let hand = inventory.active;
        inventory.hands[hand] = Some(item);
    }

    fn active_slot(&self) -> InventorySlot {
        if self.get::<Inventory>(self.mob()).active == 0 {
            InventorySlot::LeftHand
        } else {
            InventorySlot::RightHand
        }
    }

    /// Lines heard so far with `text` in a segment of `span`.
    fn heard(&self, span: Span, text: &str) -> usize {
        self.players[0]
            .app
            .world()
            .resource::<Heard>()
            .0
            .iter()
            .filter(|line| line.iter().any(|(s, t)| *s == span && t.contains(text)))
            .count()
    }
}

#[test]
fn megaphone_and_pair_pinpointers() {
    let mut s = Station::start();
    let mob = s.mob();

    // A megaphone in the active hand: the words come out bold, with a recharge after.
    let megaphone = s.items("/obj/item/megaphone")[0];
    s.give(megaphone);
    s.run_until("voiced", |s| {
        s.get::<Megaphone>(megaphone).voiced == Some(mob)
    });
    s.send(0, ClientIntent::Say("внимание".into()));
    s.run_until("a loud line", |s| s.heard(Span::Bold, "Внимание") == 1);
    s.send(0, ClientIntent::Say("ещё раз".into()));
    s.run_until("a plain line", |s| s.heard(Span::Say, "Ещё раз") == 1);
    // The recharge warning and the plain line are separate messages: either may land first.
    s.run_until("the recharge warning", |s| {
        s.heard(Span::Warning, "перезарядиться") >= 1
    });
    assert_eq!(s.heard(Span::Bold, "Ещё раз"), 0);

    // The two pinpointers of the box track each other.
    s.run_until("linked", |s| {
        let pins = s.items("/obj/item/pinpointer/pair");
        pins.len() == 2
            && pins.iter().all(|&p| {
                s.try_get::<PairPinpointer>(p)
                    .is_some_and(|x| x.other.is_some())
            })
    });
    let pins = s.items("/obj/item/pinpointer/pair");
    let (a, b) = (pins[0], pins[1]);
    assert_eq!(s.get::<PairPinpointer>(a).other, Some(b));
    assert_eq!(s.get::<PairPinpointer>(b).other, Some(a));

    // One in hand, turned on: its arrow points east at the other, still in the box.
    let free = s
        .server
        .world_mut()
        .get_mut::<Inventory>(mob)
        .unwrap()
        .hands
        .iter()
        .position(Option::is_none)
        .unwrap();
    s.server
        .world_mut()
        .get_mut::<Inventory>(mob)
        .unwrap()
        .active = free;
    s.give(a);
    let slot = s.active_slot();
    s.send(0, ClientIntent::ClickSlot(slot));
    s.run_until("turned on", |s| s.get::<PairPinpointer>(a).active);
    s.run_until("pointing", |s| !s.get::<Appearance>(a).overlays.is_empty());
    let overlay = s.get::<Appearance>(a).overlays[0].clone();
    assert_eq!(overlay.dir, Some(Dir::East), "{overlay:?}");
    let pairs_box = s.items("/obj/item/storage/box/pinpointer_pairs")[0];
    assert_eq!(s.try_get::<StoredIn>(b).map(|x| x.0), Some(pairs_box));
}
