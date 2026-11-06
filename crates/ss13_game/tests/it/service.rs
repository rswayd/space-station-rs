//! Service items over a loopback server: a pen writes on a mapped paper (after its
//! `default_raw_text`) through the text prompt and a stamp stamps it; an empty hand takes a
//! sheet out of a paper bin; a die rolls within its sides; a mapped bookcase shelves the book on
//! its tile and hands it out from the list; the DSM and IDC manuals open their windows.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::inventory::{CarriedBy, Inventory};
use ss13_game::service::{Die, Paper};
use ss13_game::storage::StoredIn;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::DmItem;
use ss13_shared::TilePos;
use ss13_shared::protocol::{ClientIntent, InventorySlot};
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::list_input::ListInputUi;
use ss13_tgui::interfaces::text_input::TextInputUi;
use ss13_tgui::server::UiWindow;

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3, "anchored": 0 } },
    "/obj": { "parent": "/atom/movable", "declared_vars": { "force": 0, "damtype": "brute" } },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start" },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "card_grey", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null, "w_class": 1,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null, "attack_speed": 8 } },
    "/obj/item/paper": { "parent": "/obj/item", "declared_vars": { "default_raw_text": null } },
    "/obj/item/paper/fluff": { "parent": "/obj/item/paper", "declared_vars": {
        "default_raw_text": "Rule one" } },
    "/obj/item/pen": { "parent": "/obj/item" },
    "/obj/item/stamp": { "parent": "/obj/item" },
    "/obj/item/stamp/denied": { "parent": "/obj/item/stamp" },
    "/obj/item/paper_bin": { "parent": "/obj/item" },
    "/obj/item/dice": { "parent": "/obj/item", "declared_vars": { "sides": 6 } },
    "/obj/item/dice/d20": { "parent": "/obj/item/dice", "declared_vars": { "sides": 20 } },
    "/obj/item/book": { "parent": "/obj/item", "declared_vars": {
        "starting_title": null, "starting_author": null, "starting_content": null } },
    "/obj/item/book/law": { "parent": "/obj/item/book", "declared_vars": {
        "starting_title": "Space Law", "starting_content": "Do not." } },
    "/obj/item/tgui_book": { "parent": "/obj/item" },
    "/obj/item/tgui_book/manual": { "parent": "/obj/item/tgui_book" },
    "/obj/item/tgui_book/manual/dsm": { "parent": "/obj/item/tgui_book/manual" },
    "/obj/item/tgui_book/manual/idc": { "parent": "/obj/item/tgui_book/manual" },
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
    "/obj/structure": { "parent": "/obj", "declared_vars": { "density": 1, "anchored": 1 } },
    "/obj/structure/bookcase": { "parent": "/obj/structure", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "card_grey" } },
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

/// The player starts at (2,3): the bookcase and its book north (2,4), the fluff paper east
/// (3,3), the paper bin south (2,2); the pen, stamp and die lie further east.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"b" = (/obj/structure/bookcase,/obj/item/book/law,/turf/open/floor,/area/space)
"o" = (/obj/item/paper/fluff,/turf/open/floor,/area/space)
"k" = (/obj/item/paper_bin,/turf/open/floor,/area/space)
"n" = (/obj/item/pen,/turf/open/floor,/area/space)
"m" = (/obj/item/stamp/denied,/turf/open/floor,/area/space)
"d" = (/obj/item/dice/d20,/turf/open/floor,/area/space)
"q" = (/obj/item/tgui_book/manual/dsm,/turf/open/floor,/area/space)
"r" = (/obj/item/tgui_book/manual/idc,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwww
wbfmrw
wsonfw
wkfdqw
wwwwww
"}
"##;

const STATES: [(&str, &str); 5] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "card_grey"),
];

const FLUFF: &str = "/obj/item/paper/fluff";
const PAPER: &str = "/obj/item/paper";
const PEN: &str = "/obj/item/pen";
const STAMP: &str = "/obj/item/stamp/denied";
const BIN: &str = "/obj/item/paper_bin";
const D20: &str = "/obj/item/dice/d20";
const BOOK: &str = "/obj/item/book/law";
const BOOKCASE: &str = "/obj/structure/bookcase";
const DSM: &str = "/obj/item/tgui_book/manual/dsm";
const IDC: &str = "/obj/item/tgui_book/manual/idc";

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("service_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        FLUFF,
        PAPER,
        PEN,
        STAMP,
        BIN,
        D20,
        BOOK,
        BOOKCASE,
        DSM,
        IDC,
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
    fn start(test: &str) -> Self {
        let assets = fixture_assets(test);
        let mut s = testing::Station::start(fixture_map(&assets, &["map.dmm"]), |app| {
            app.add_plugins(ss13_chat::server::ChatServerPlugin);
        });
        s.join_all(1, |_| {});
        let a = s.players[0].mob;
        s.server
            .world_mut()
            .entity_mut(a)
            .insert(TilePos::new(2, 3, 1));
        Self(s)
    }

    fn item(&mut self, path: &str) -> Entity {
        let world = self.server.world_mut();
        let mut q = world.query::<(Entity, &DmItem)>();
        q.iter(world)
            .find(|(_, i)| i.path == path)
            .map(|(e, _)| e)
            .unwrap_or_else(|| panic!("{path} on the map"))
    }

    fn bookcase(&mut self) -> Entity {
        let world = self.server.world_mut();
        let mut q = world.query::<(Entity, &ss13_mapload::DmObj)>();
        q.iter(world)
            .find(|(_, o)| o.path == BOOKCASE)
            .map(|(e, _)| e)
            .expect("the bookcase")
    }

    /// `item` into the mob's active hand, whatever was there dropped out of the inventory.
    fn put_in_hand(&mut self, item: Entity) {
        let mob = self.players[0].mob;
        let world = self.server.world_mut();
        let hand = world.get::<Inventory>(mob).unwrap().active;
        if let Some(old) = world.get::<Inventory>(mob).unwrap().hands[hand] {
            world
                .entity_mut(old)
                .remove::<CarriedBy>()
                .insert(TilePos::new(4, 3, 1));
        }
        world
            .entity_mut(item)
            .remove::<TilePos>()
            .insert(CarriedBy(mob));
        world.get_mut::<Inventory>(mob).unwrap().hands[hand] = Some(item);
    }

    fn active_slot(&self) -> InventorySlot {
        if self.get::<Inventory>(self.players[0].mob).active == 0 {
            InventorySlot::LeftHand
        } else {
            InventorySlot::RightHand
        }
    }

    /// The newest open window of `interface` for the player.
    fn window(&mut self, interface: &str) -> Option<u32> {
        let link = self.players[0].link;
        let world = self.server.world_mut();
        let mut q = world.query::<&UiWindow>();
        q.iter(world)
            .filter(|w| w.client == link && w.interface == interface)
            .map(|w| w.id)
            .max()
    }

    fn wait_window(&mut self, interface: &str) -> u32 {
        self.run_until(&format!("{interface} opens"), |s| {
            s.window(interface).is_some()
        });
        self.window(interface).unwrap()
    }
}

#[test]
fn a_pen_writes_and_a_stamp_stamps() {
    let mut s = Station::start("a_pen_writes_and_a_stamp_stamps");
    let paper = s.item(FLUFF);
    let texts = |s: &Station| -> Vec<String> {
        s.get::<Paper>(paper)
            .inputs
            .iter()
            .map(|i| i.raw_text.clone())
            .collect()
    };
    s.run_until("the paper has its default text", |s| {
        s.try_get::<Paper>(paper).is_some()
    });
    assert_eq!(texts(&s), ["Rule one"]);

    let pen = s.item(PEN);
    s.put_in_hand(pen);
    s.click(0, paper);
    let prompt = s.wait_window(TextInputUi::NAME);
    s.act(0, prompt, "submit", &[("entry", "Hello")]);
    s.run_until("the text is written", |s| texts(s).len() == 2);
    assert_eq!(texts(&s), ["Rule one", "Hello"]);
    assert_eq!(s.get::<Paper>(paper).inputs[1].color, "#000000");

    let stamp = s.item(STAMP);
    s.put_in_hand(stamp);
    s.click(0, paper);
    s.run_until("the paper is stamped", |s| {
        !s.get::<Paper>(paper).stamps.is_empty()
    });
}

#[test]
fn a_bin_hands_out_paper_and_a_die_rolls() {
    let mut s = Station::start("a_bin_hands_out_paper_and_a_die_rolls");
    let mob = s.players[0].mob;
    let bin = s.item(BIN);
    s.click(0, bin);
    s.run_until("a sheet is in hand", |s| {
        s.get::<Inventory>(mob)
            .active_item()
            .is_some_and(|i| s.try_get::<Paper>(i).is_some())
    });
    assert_eq!(
        *s.get::<TilePos>(bin),
        TilePos::new(2, 2, 1),
        "the bin stays"
    );

    let die = s.item(D20);
    s.put_in_hand(die);
    let slot = s.active_slot();
    s.send(0, ClientIntent::ClickSlot(slot));
    for _ in 0..5 {
        s.tick();
    }
    let face = *s.get::<Die>(die);
    assert!(
        (1..=20).contains(&face.result) && face.sides == 20,
        "{face:?}"
    );
}

#[test]
fn a_bookcase_shelves_and_hands_out_its_book() {
    let mut s = Station::start("a_bookcase_shelves_and_hands_out_its_book");
    let mob = s.players[0].mob;
    let (case, book) = (s.bookcase(), s.item(BOOK));
    s.run_until("the book is shelved", |s| {
        s.try_get::<StoredIn>(book).is_some_and(|i| i.0 == case)
    });
    s.click(0, case);
    let list = s.wait_window(ListInputUi::NAME);
    s.act(0, list, "submit", &[("entry", "law")]);
    s.run_until("the book is in hand", |s| {
        s.get::<Inventory>(mob).active_item() == Some(book)
    });
}

#[test]
fn the_manuals_open_their_windows() {
    let mut s = Station::start("the_manuals_open_their_windows");
    for (path, interface) in [(DSM, "DSMBook"), (IDC, "IDCBook")] {
        let manual = s.item(path);
        s.put_in_hand(manual);
        let slot = s.active_slot();
        s.send(0, ClientIntent::ClickSlot(slot));
        let window = s.wait_window(interface);
        // The page-flip act is accepted and keeps the window open.
        s.act(0, window, "play_flip_sound", &[]);
        for _ in 0..3 {
            s.tick();
        }
        assert_eq!(s.window(interface), Some(window));
    }
}
