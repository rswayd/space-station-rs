//! Office and library machines over a loopback server: a pen fills a paper's input field, a
//! clipboard takes a paper, a paper folds into a plane and back; the photocopier copies a
//! paper and secret documents (a crayon forges the copy's seal); a fax reaches another fax; the binder binds a paper, the scanner caches the book, the
//! librarian's console uploads and prints it; a canvas is painted, finished and put on an easel;
//! a carbon sheet tears off its copy, the paper cutter cuts a paper into slips, a filing cabinet
//! files and returns a paper, a biscuit card cracks open; the barcode scanner connects to the
//! librarian's console and adds, sets for check-out and checks in a book; the console's window
//! lends a book to a name, lists it overdue, checks it in, prints a bible and, emagged, the
//! forbidden lore.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::inventory::{CarriedBy, Inventory};
use ss13_game::misc_items::documents::DocumentCopy;
use ss13_game::service::{
    BarcodeScanner, Biscuit, Book, Canvas, CarbonPaper, Clipboard, FilingCabinet, Ledger,
    LibraryArchive, PaintingFrame, Palette, Paper, PaperCutter, Photocopier, ScanMode,
};
use ss13_game::storage::StoredIn;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::{DmItem, DmObj};
use ss13_shared::TilePos;
use ss13_shared::protocol::{ClickModifiers, ClientIntent, InventorySlot};
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::alert::AlertUi;
use ss13_tgui::interfaces::color_picker::ColorPickerUi;
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
    "/obj/item/paper/form": { "parent": "/obj/item/paper", "declared_vars": {
        "default_raw_text": "Name: [input_field]" } },
    "/obj/item/paper/fluff": { "parent": "/obj/item/paper", "declared_vars": {
        "default_raw_text": "Rule one" } },
    "/obj/item/paper/carbon": { "parent": "/obj/item/paper" },
    "/obj/item/paper/carbon_copy": { "parent": "/obj/item/paper" },
    "/obj/item/paper/paperslip": { "parent": "/obj/item/paper" },
    "/obj/item/papercutter": { "parent": "/obj/item", "declared_vars": { "w_class": 3 } },
    "/obj/item/hatchet": { "parent": "/obj/item" },
    "/obj/item/hatchet/cutterblade": { "parent": "/obj/item/hatchet" },
    "/obj/item/folder": { "parent": "/obj/item" },
    "/obj/item/folder/biscuit": { "parent": "/obj/item/folder" },
    "/obj/item/paperplane": { "parent": "/obj/item" },
    "/obj/item/pen": { "parent": "/obj/item" },
    "/obj/item/clipboard": { "parent": "/obj/item" },
    "/obj/item/toner": { "parent": "/obj/item" },
    "/obj/item/documents": { "parent": "/obj/item", "declared_vars": {
        "name": "secret documents", "resistance_flags": 115 } },
    "/obj/item/documents/nanotrasen": { "parent": "/obj/item/documents" },
    "/obj/item/documents/photocopy": { "parent": "/obj/item/documents" },
    "/obj/item/barcodescanner": { "parent": "/obj/item" },
    "/obj/item/book/bible": { "parent": "/obj/item/book" },
    "/obj/item/toy": { "parent": "/obj/item" },
    "/obj/item/toy/crayon": { "parent": "/obj/item/toy" },
    "/obj/item/toy/crayon/red": { "parent": "/obj/item/toy/crayon" },
    "/obj/item/canvas": { "parent": "/obj/item" },
    "/obj/item/paint_palette": { "parent": "/obj/item" },
    "/obj/item/wirecutters": { "parent": "/obj/item", "declared_vars": {
        "tool_behaviour": "cutters" } },
    "/obj/item/book": { "parent": "/obj/item", "declared_vars": {
        "starting_title": null, "starting_author": null, "starting_content": null } },
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
    "/obj/structure": { "parent": "/obj", "declared_vars": { "density": 1, "anchored": 1,
        "icon": { "resource": "i.dmi" }, "icon_state": "card_grey" } },
    "/obj/structure/easel": { "parent": "/obj/structure" },
    "/obj/structure/sign": { "parent": "/obj/structure", "declared_vars": { "density": 0 } },
    "/obj/structure/sign/painting": { "parent": "/obj/structure/sign", "declared_vars": {
        "name": "Painting" } },
    "/obj/structure/filingcabinet": { "parent": "/obj/structure" },
    "/obj/machinery": { "parent": "/obj", "declared_vars": { "density": 1, "anchored": 1,
        "icon": { "resource": "i.dmi" }, "icon_state": "card_grey" } },
    "/obj/machinery/photocopier": { "parent": "/obj/machinery" },
    "/obj/machinery/photocopier/prebuilt": { "parent": "/obj/machinery/photocopier" },
    "/obj/machinery/fax": { "parent": "/obj/machinery", "declared_vars": { "fax_name": null } },
    "/obj/machinery/fax/bridge": { "parent": "/obj/machinery/fax", "declared_vars": {
        "fax_name": "Bridge" } },
    "/obj/machinery/fax/captain": { "parent": "/obj/machinery/fax", "declared_vars": {
        "fax_name": "Captain's Office" } },
    "/obj/machinery/libraryscanner": { "parent": "/obj/machinery" },
    "/obj/machinery/bookbinder": { "parent": "/obj/machinery" },
    "/obj/machinery/computer": { "parent": "/obj/machinery" },
    "/obj/machinery/computer/libraryconsole": { "parent": "/obj/machinery/computer" },
    "/obj/machinery/computer/libraryconsole/bookmanagement": {
        "parent": "/obj/machinery/computer/libraryconsole" },
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
    "/area": { "parent": "/atom", "declared_vars": { "requires_power": 0 } },
    "/area/space": { "parent": "/area" }
  }
}"##;

/// The player starts at (2,3); the loose items lie east of it on (3,3). The machines stand on
/// the top row (y 4) and the bottom row (y 2), each reached from the floor row (y 3) below or
/// above it.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"o" = (/obj/item/paper/form,/obj/item/paper/fluff,/obj/item/paper,/obj/item/pen,/obj/item/clipboard,/obj/item/canvas,/obj/item/paint_palette,/obj/item/wirecutters,/obj/item/toy/crayon/red,/obj/item/paper/carbon,/obj/item/papercutter,/obj/item/folder/biscuit,/obj/item/barcodescanner,/obj/item/book,/turf/open/floor,/area/space)
"K" = (/obj/item/paper/paperslip,/obj/structure/filingcabinet,/turf/open/floor,/area/space)
"Q" = (/obj/machinery/photocopier/prebuilt,/turf/open/floor,/area/space)
"Y" = (/obj/machinery/fax/bridge,/turf/open/floor,/area/space)
"Z" = (/obj/machinery/fax/captain,/turf/open/floor,/area/space)
"S" = (/obj/machinery/libraryscanner,/turf/open/floor,/area/space)
"C" = (/obj/machinery/computer/libraryconsole/bookmanagement,/turf/open/floor,/area/space)
"B" = (/obj/machinery/bookbinder,/turf/open/floor,/area/space)
"E" = (/obj/structure/easel,/turf/open/floor,/area/space)
"P" = (/obj/structure/sign/painting,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwwwwwww
wQfYfSPCKw
wsoffffffw
wfffBfZfEw
wwwwwwwwww
"}
"##;

const STATES: [(&str, &str); 5] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "card_grey"),
];

const FORM: &str = "/obj/item/paper/form";
const FLUFF: &str = "/obj/item/paper/fluff";
const PAPER: &str = "/obj/item/paper";
const PLANE: &str = "/obj/item/paperplane";
const PEN: &str = "/obj/item/pen";
const CLIPBOARD: &str = "/obj/item/clipboard";
const CANVAS: &str = "/obj/item/canvas";
const CRAYON: &str = "/obj/item/toy/crayon/red";
const COPIER: &str = "/obj/machinery/photocopier/prebuilt";
const BRIDGE_FAX: &str = "/obj/machinery/fax/bridge";
const CAPTAIN_FAX: &str = "/obj/machinery/fax/captain";
const SCANNER: &str = "/obj/machinery/libraryscanner";
const CONSOLE: &str = "/obj/machinery/computer/libraryconsole/bookmanagement";
const BINDER: &str = "/obj/machinery/bookbinder";
const EASEL: &str = "/obj/structure/easel";
const PALETTE: &str = "/obj/item/paint_palette";
const CUTTERS: &str = "/obj/item/wirecutters";
const FRAME: &str = "/obj/structure/sign/painting";
const CARBON: &str = "/obj/item/paper/carbon";
const CARBON_COPY: &str = "/obj/item/paper/carbon_copy";
const SLIP: &str = "/obj/item/paper/paperslip";
const CUTTER: &str = "/obj/item/papercutter";
const BISCUIT: &str = "/obj/item/folder/biscuit";
const CABINET: &str = "/obj/structure/filingcabinet";
const BARCODE: &str = "/obj/item/barcodescanner";

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("paperwork_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        FORM,
        FLUFF,
        PAPER,
        PLANE,
        PEN,
        CLIPBOARD,
        CANVAS,
        PALETTE,
        CUTTERS,
        FRAME,
        CRAYON,
        "/obj/item/toner",
        "/obj/item/documents/nanotrasen",
        "/obj/item/documents/photocopy",
        "/obj/item/book",
        BARCODE,
        "/obj/item/book/bible",
        COPIER,
        BRIDGE_FAX,
        CAPTAIN_FAX,
        SCANNER,
        CONSOLE,
        BINDER,
        EASEL,
        CARBON,
        CARBON_COPY,
        SLIP,
        CUTTER,
        "/obj/item/hatchet",
        "/obj/item/hatchet/cutterblade",
        "/obj/item/folder",
        BISCUIT,
        CABINET,
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
    ];
    write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)])
}

/// The station and the id of the last prompt window answered.
struct Station(testing::Station, u32);
ss13_game::drive_station!(Station, 0);

impl Station {
    fn start(test: &str) -> Self {
        let assets = fixture_assets(test);
        let mut s = testing::Station::start(fixture_map(&assets, &["map.dmm"]), |app| {
            app.add_plugins(ss13_chat::server::ChatServerPlugin);
        });
        s.join_all(1, |_| {});
        let mut s = Self(s, 0);
        s.stand(TilePos::new(2, 3, 1));
        s
    }

    /// Moves the player to `pos` and lets the view follow.
    fn stand(&mut self, pos: TilePos) {
        let mob = self.players[0].mob;
        self.server.world_mut().entity_mut(mob).insert(pos);
        for _ in 0..5 {
            self.tick();
        }
    }

    fn item(&mut self, path: &str) -> Entity {
        let world = self.server.world_mut();
        let mut q = world.query::<(Entity, &DmItem)>();
        q.iter(world)
            .find(|(_, i)| i.path == path)
            .map(|(e, _)| e)
            .unwrap_or_else(|| panic!("{path} on the map"))
    }

    fn obj(&mut self, path: &str) -> Entity {
        let world = self.server.world_mut();
        let mut q = world.query::<(Entity, &DmObj)>();
        q.iter(world)
            .find(|(_, o)| o.path == path)
            .map(|(e, _)| e)
            .unwrap_or_else(|| panic!("{path} on the map"))
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
                .insert(TilePos::new(3, 3, 1));
        }
        world
            .entity_mut(item)
            .remove::<(TilePos, StoredIn)>()
            .insert(CarriedBy(mob));
        world.get_mut::<Inventory>(mob).unwrap().hands[hand] = Some(item);
    }

    fn empty_hand(&mut self) {
        let mob = self.players[0].mob;
        let world = self.server.world_mut();
        let hand = world.get::<Inventory>(mob).unwrap().active;
        if let Some(old) = world.get::<Inventory>(mob).unwrap().hands[hand] {
            world
                .entity_mut(old)
                .remove::<CarriedBy>()
                .insert(TilePos::new(3, 3, 1));
        }
        world.get_mut::<Inventory>(mob).unwrap().hands[hand] = None;
    }

    fn active(&self) -> Option<Entity> {
        self.get::<Inventory>(self.players[0].mob).active_item()
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

    /// Waits for a window of `interface` newer than the last one answered and answers it.
    fn answer(&mut self, interface: &str, action: &str, key: &str, value: &str) {
        let last = self.1;
        self.run_until(&format!("{interface} opens for {value}"), |s| {
            s.window(interface).is_some_and(|id| id > last)
        });
        let id = self.window(interface).unwrap();
        self.1 = id;
        self.act(0, id, action, &[(key, value)]);
    }

    fn pick(&mut self, entry: &str) {
        self.answer(ListInputUi::NAME, "submit", "entry", entry);
    }

    fn type_in(&mut self, text: &str) {
        self.answer(TextInputUi::NAME, "submit", "entry", text);
    }

    /// Waits for the librarian console's window.
    fn console_window(&mut self) -> u32 {
        self.run_until("the console window opens", |s| {
            s.window("LibraryConsole").is_some()
        });
        self.window("LibraryConsole").unwrap()
    }

    fn books_titled(&mut self, title: &str) -> usize {
        let world = self.server.world_mut();
        let mut q = world.query::<&Book>();
        q.iter(world).filter(|b| b.title == title).count()
    }
}

#[test]
fn a_pen_fills_a_field_and_a_paper_clips_and_folds() {
    let mut s = Station::start("a_pen_fills_a_field_and_a_paper_clips_and_folds");
    let form = s.item(FORM);
    s.run_until("the form has its text", |s| {
        s.try_get::<Paper>(form)
            .is_some_and(|p| !p.inputs.is_empty())
    });
    let pen = s.item(PEN);
    s.put_in_hand(pen);
    s.click(0, form);
    s.pick("Field 1: Name:");
    s.type_in("Bob");
    s.run_until("the field is filled", |s| {
        s.get::<Paper>(form).inputs[0].full_text() == "Name: Bob"
    });
    assert!(s.get::<Paper>(form).open_fields().is_empty());

    let board = s.item(CLIPBOARD);
    s.put_in_hand(form);
    s.click(0, board);
    s.run_until("the form is clipped", |s| {
        s.try_get::<StoredIn>(form).is_some_and(|i| i.0 == board)
    });
    assert_eq!(s.get::<Clipboard>(board).top, Some(form));

    let sheet = s.item(PAPER);
    s.put_in_hand(sheet);
    let slot = s.active_slot();
    s.send(0, ClientIntent::AltClickSlot(slot));
    s.run_until("a plane is in hand", |s| {
        s.active()
            .is_some_and(|i| s.try_get::<DmItem>(i).is_some_and(|d| d.path == PLANE))
    });
    assert!(
        s.try_get::<StoredIn>(sheet).is_some(),
        "the paper is inside"
    );
    s.send(0, ClientIntent::ClickSlot(slot));
    s.run_until("the paper is unfolded into the hand", |s| {
        s.active() == Some(sheet)
    });
}

#[test]
fn the_photocopier_prints_a_blank_with_the_users_keys() {
    let mut s = Station::start("the_photocopier_prints_a_blank_with_the_users_keys");
    let copier = s.obj(COPIER);
    s.run_until("the copier is set up with paper and toner", |s| {
        s.try_get::<Photocopier>(copier)
            .is_some_and(|c| c.paper == 30 && c.toner.is_some())
    });
    let blank = |code: &str, name: &str, access: Option<&str>| ss13_game::service::blanks::Blank {
        code: code.into(),
        category: "Общие формы".into(),
        name: name.into(),
        info: "Я, [name], прошу: [input_field]".into(),
        required_access: access.map(str::to_owned),
    };
    s.server
        .world_mut()
        .insert_resource(ss13_game::service::blanks::PaperBlanks(vec![
            blank("NT-CC", "Секретная", Some("cent_general")),
            blank("NT-GENERAL", "Заявление", None),
        ]));
    let mob = s.players[0].mob;
    let name = s.get::<Name>(mob).as_str().to_owned();
    s.click(0, copier);
    s.pick("Печать бланка (1)");
    // The CentCom form needs its access: only the general one is listed.
    s.pick("Общие формы: Заявление (NT-GENERAL)");
    let wanted = format!("Я, {name}, прошу: [input_field]");
    s.run_until("the form comes out", |s| {
        let world = s.server.world_mut();
        let mut q = world.query::<(&Paper, &ss13_mapload::DmName)>();
        q.iter(world).any(|(p, n)| {
            n.name == "бумага - 'Заявление'" && p.inputs.iter().any(|i| i.raw_text == wanted)
        })
    });
    assert_eq!(s.get::<Photocopier>(copier).paper, 29);
    assert_eq!(s.get::<Photocopier>(copier).blank, None);
}

#[test]
fn the_photocopier_copies_and_the_fax_sends() {
    let mut s = Station::start("the_photocopier_copies_and_the_fax_sends");
    let fluff = s.item(FLUFF);
    let copier = s.obj(COPIER);
    s.run_until("the copier is set up with paper and toner", |s| {
        s.try_get::<Photocopier>(copier)
            .is_some_and(|c| c.paper == 30 && c.toner.is_some())
    });
    s.run_until("the fluff paper has its text", |s| {
        s.try_get::<Paper>(fluff)
            .is_some_and(|p| !p.inputs.is_empty())
    });
    s.put_in_hand(fluff);
    s.click(0, copier);
    s.run_until("the original is in the tray", |s| {
        s.get::<Photocopier>(copier).object == Some(fluff)
    });
    s.click(0, copier);
    s.pick("Copy (1)");
    let original = s.get::<Paper>(fluff).inputs.clone();
    s.run_until("a copy comes out", |s| {
        let world = s.server.world_mut();
        let mut q = world.query::<(Entity, &Paper, &TilePos)>();
        q.iter(world)
            .any(|(e, p, at)| e != fluff && *at == TilePos::new(2, 4, 1) && p.inputs == original)
    });
    assert_eq!(s.get::<Photocopier>(copier).paper, 29);

    // The original back out, then into the bridge fax and to the captain's.
    s.click(0, copier);
    s.pick("Take out: fluff");
    s.run_until("the original is in hand", |s| s.active() == Some(fluff));
    let bridge = s.obj(BRIDGE_FAX);
    s.stand(TilePos::new(4, 3, 1));
    s.click(0, bridge);
    s.run_until("the paper is loaded", |s| {
        s.try_get::<StoredIn>(fluff).is_some_and(|i| i.0 == bridge)
    });
    s.click(0, bridge);
    s.pick("Send to Captain's Office");
    s.run_until("the paper comes out of the captain's fax", |s| {
        s.try_get::<TilePos>(fluff) == Some(&TilePos::new(7, 2, 1))
    });
    assert!(s.try_get::<StoredIn>(fluff).is_none());
}

#[test]
fn secret_documents_are_photocopied_and_a_seal_forged() {
    let mut s = Station::start("secret_documents_are_photocopied_and_a_seal_forged");
    let copier = s.obj(COPIER);
    s.run_until("the copier is set up with paper and toner", |s| {
        s.try_get::<Photocopier>(copier)
            .is_some_and(|c| c.paper == 30 && c.toner.is_some())
    });
    let docs = ss13_game::construction::spawn_obj(
        s.server.world_mut(),
        "/obj/item/documents/nanotrasen",
        TilePos::new(3, 3, 1),
    )
    .unwrap();
    s.put_in_hand(docs);
    s.click(0, copier);
    s.run_until("the documents are in the tray", |s| {
        s.get::<Photocopier>(copier).object == Some(docs)
    });
    s.click(0, copier);
    s.pick("Copy (1)");
    let copy = |s: &mut Station| {
        let world = s.server.world_mut();
        let mut q = world.query::<(Entity, &DocumentCopy)>();
        q.iter(world).map(|(e, _)| e).next()
    };
    s.run_until("a photocopy comes out", |s| copy(s).is_some());
    let photocopy = copy(&mut s).unwrap();
    assert_eq!(
        s.get::<DocumentCopy>(photocopy).copy_type,
        "/obj/item/documents/nanotrasen"
    );
    assert_eq!(
        s.get::<Photocopier>(copier).paper,
        10,
        "DOCUMENT_PAPER_USE sheets"
    );

    // A red crayon forges the seal once.
    let crayon = s.item(CRAYON);
    s.put_in_hand(crayon);
    s.click(0, photocopy);
    s.run_until("the seal is forged", |s| {
        s.get::<DocumentCopy>(photocopy).forged_seal == Some("red")
    });
    assert_eq!(s.get::<Name>(photocopy).as_str(), "red secret documents");
}

#[test]
fn a_bound_book_is_scanned_uploaded_and_printed() {
    let mut s = Station::start("a_bound_book_is_scanned_uploaded_and_printed");
    let fluff = s.item(FLUFF);
    s.run_until("the fluff paper has its text", |s| {
        s.try_get::<Paper>(fluff)
            .is_some_and(|p| !p.inputs.is_empty())
    });
    let (binder, scanner, console) = (s.obj(BINDER), s.obj(SCANNER), s.obj(CONSOLE));
    s.stand(TilePos::new(5, 3, 1));
    // No ID scanned: the first paper only sets "unknown author".
    s.put_in_hand(fluff);
    s.click(0, binder);
    s.run_until("the binder takes the author", |s| {
        s.get::<ss13_game::service::BookBinder>(binder)
            .scanned_name
            .is_some()
    });
    s.click(0, binder);
    s.run_until("the paper is bound", |s| {
        s.world_books_by("unknown author") == 1
    });
    let book = s.book_by("unknown author");
    assert!(s.get::<Book>(book).content.contains("Rule one"));
    assert!(s.try_get::<Paper>(fluff).is_none(), "the paper is gone");

    s.put_in_hand(book);
    s.click(0, scanner);
    s.run_until("the book is in the scanner", |s| {
        s.try_get::<StoredIn>(book).is_some_and(|i| i.0 == scanner)
    });
    s.empty_hand();
    s.click(0, scanner);
    s.pick("Scan");
    s.run_until("the book is cached", |s| {
        s.get::<ss13_game::service::BookScanner>(scanner)
            .cache
            .is_some()
    });

    s.stand(TilePos::new(8, 3, 1));
    s.click(0, console);
    let window = s.console_window();
    s.act(0, window, "set_cache_title", &[("title", "Rules")]);
    s.run_until("the cache has a title", |s| {
        s.get::<ss13_game::service::BookScanner>(scanner)
            .cache
            .as_ref()
            .is_some_and(|c| c.title == "Rules")
    });
    s.act(0, window, "upload", &[("category", "Fiction")]);
    s.run_until("the book is archived", |s| {
        s.server.world().resource::<LibraryArchive>().books.len() == 1
    });
    let id = s.server.world().resource::<LibraryArchive>().books[0]
        .id
        .to_string();
    s.act(0, window, "print_book", &[("book_id", &id)]);
    s.run_until("the printed book is out", |s| s.books_titled("Rules") == 1);
}

impl Station {
    fn world_books_by(&mut self, author: &str) -> usize {
        let world = self.server.world_mut();
        let mut q = world.query::<&Book>();
        q.iter(world).filter(|b| b.author == author).count()
    }

    fn book_by(&mut self, author: &str) -> Entity {
        let world = self.server.world_mut();
        let mut q = world.query::<(Entity, &Book)>();
        q.iter(world)
            .find(|(_, b)| b.author == author)
            .map(|(e, _)| e)
            .expect("the book")
    }
}

#[test]
fn a_canvas_is_painted_finished_and_put_up() {
    let mut s = Station::start("a_canvas_is_painted_finished_and_put_up");
    let (canvas, crayon) = (s.item(CANVAS), s.item(CRAYON));
    s.run_until("the canvas is set up", |s| {
        s.try_get::<Canvas>(canvas).is_some()
    });
    s.put_in_hand(crayon);
    s.click(0, canvas);
    s.run_until("the canvas is painted", |s| s.get::<Canvas>(canvas).used);
    assert_eq!(s.get::<Canvas>(canvas).medium, "Crayon on canvas");

    s.put_in_hand(canvas);
    let slot = s.active_slot();
    s.send(0, ClientIntent::ClickSlot(slot));
    s.type_in("Sunset");
    s.answer(AlertUi::NAME, "choose", "choice", "No");
    s.run_until("the painting is finished", |s| {
        s.get::<Canvas>(canvas).finalized
    });
    assert_eq!(s.get::<Canvas>(canvas).title, "Sunset");
    assert_eq!(s.get::<Canvas>(canvas).author, "Anonymous");

    let easel = s.obj(EASEL);
    s.stand(TilePos::new(9, 3, 1));
    s.click(0, easel);
    s.run_until("the canvas is on the easel", |s| {
        s.try_get::<TilePos>(canvas) == Some(&TilePos::new(9, 2, 1))
    });
}

#[test]
fn a_painting_is_framed_cut_out_and_the_palette_keeps_colours() {
    let mut s = Station::start("a_painting_is_framed_cut_out_and_the_palette_keeps_colours");
    let (canvas, crayon, frame) = (s.item(CANVAS), s.item(CRAYON), s.obj(FRAME));
    s.run_until("the canvas and frame are set up", |s| {
        s.try_get::<Canvas>(canvas).is_some() && s.try_get::<PaintingFrame>(frame).is_some()
    });
    s.put_in_hand(crayon);
    s.click(0, canvas);
    s.run_until("the canvas is painted", |s| s.get::<Canvas>(canvas).used);

    // Framing an unfinished canvas finishes it.
    s.put_in_hand(canvas);
    s.stand(TilePos::new(7, 3, 1));
    s.click(0, frame);
    s.run_until("the canvas is framed", |s| {
        s.get::<PaintingFrame>(frame).canvas == Some(canvas)
    });
    assert_eq!(s.get::<StoredIn>(canvas).0, frame);
    s.type_in("Sunset");
    s.answer(AlertUi::NAME, "choose", "choice", "Yes");
    s.run_until("the framed painting is finished", |s| {
        s.get::<Canvas>(canvas).finalized
    });
    s.run_until("the frame is named after it", |s| {
        s.get::<ss13_mapload::DmName>(frame).name == "картина - Sunset"
    });

    // Wirecutters take it out onto the frame's tile.
    let cutters = s.item(CUTTERS);
    s.put_in_hand(cutters);
    s.click(0, frame);
    s.run_until("the painting is cut out", |s| {
        s.try_get::<TilePos>(canvas) == Some(&TilePos::new(7, 4, 1))
    });
    assert_eq!(s.get::<PaintingFrame>(frame).canvas, None);
    assert_eq!(s.get::<ss13_mapload::DmName>(frame).name, "Painting");

    // The palette picks a colour in hand and saves it from its list.
    let palette = s.item(PALETTE);
    s.put_in_hand(palette);
    let slot = s.active_slot();
    s.send(0, ClientIntent::ClickSlot(slot));
    s.answer(ColorPickerUi::NAME, "submit", "entry", "#aabbcc");
    s.run_until("the colour is picked", |s| {
        s.get::<Palette>(palette).selected == "#aabbcc"
    });
    s.send(0, ClientIntent::RightClickSlot(slot));
    s.pick("Добавить цвет (#aabbcc)");
    s.run_until("the colour is saved", |s| {
        s.get::<Palette>(palette).colors == ["#aabbcc"]
    });
}

#[test]
fn carbon_cutter_cabinet_and_biscuit() {
    let mut s = Station::start("carbon_cutter_cabinet_and_biscuit");

    // The carbon sheet tears off its copy once, then folds like any paper.
    let carbon = s.item(CARBON);
    s.run_until("the carbon is set up", |s| {
        s.try_get::<CarbonPaper>(carbon).is_some()
    });
    s.put_in_hand(carbon);
    let slot = s.active_slot();
    s.send(0, ClientIntent::AltClickSlot(slot));
    s.run_until("the copy is torn off", |s| {
        s.get::<CarbonPaper>(carbon).copied
    });
    let copy = {
        let world = s.server.world_mut();
        let mut q = world.query::<(Entity, &DmItem, &ss13_mapload::DmName)>();
        q.iter(world)
            .find(|(_, i, _)| i.path == CARBON_COPY)
            .map(|(e, _, n)| (e, n.name.clone()))
    };
    let (copy, name) = copy.expect("a carbon copy");
    assert!(name.starts_with("Copy - "), "{name}");
    assert!(s.try_get::<Paper>(copy).is_some());
    let mob = s.players[0].mob;
    assert!(s.get::<Inventory>(mob).holds(copy), "the copy is in hand");

    // A paper into the cutter, cut by a right click: two slips where the cutter lies.
    let (cutter, paper) = (s.item(CUTTER), s.item(PAPER));
    s.run_until("the cutter has its blade", |s| {
        s.try_get::<PaperCutter>(cutter)
            .is_some_and(|c| c.blade.is_some() && c.secured)
    });
    s.put_in_hand(paper);
    s.click(0, cutter);
    s.run_until("the paper is in the cutter", |s| {
        s.get::<PaperCutter>(cutter).paper == Some(paper)
    });
    s.empty_hand();
    let right = ClickModifiers {
        right: true,
        ..Default::default()
    };
    s.click_with(0, cutter, right);
    s.run_until("the paper is cut", |s| {
        s.get::<PaperCutter>(cutter).paper.is_none()
    });
    let slips = {
        let world = s.server.world_mut();
        let mut q = world.query::<(&DmItem, &TilePos)>();
        q.iter(world)
            .filter(|(i, at)| i.path == SLIP && **at == TilePos::new(3, 3, 1))
            .count()
    };
    assert_eq!(slips, 2);

    // The biscuit card cracks open in hand.
    let biscuit = s.item(BISCUIT);
    s.run_until("the biscuit is set up", |s| {
        s.try_get::<Biscuit>(biscuit).is_some()
    });
    s.put_in_hand(biscuit);
    let slot = s.active_slot();
    s.send(0, ClientIntent::ClickSlot(slot));
    s.answer(AlertUi::NAME, "choose", "choice", "Yes");
    s.run_until("the biscuit is cracked", |s| {
        s.get::<Biscuit>(biscuit).cracked
    });

    // The mapped slip went into the cabinet; a paper goes in and comes back out.
    let cabinet = s.obj(CABINET);
    let filed = {
        let world = s.server.world_mut();
        let mut q = world.query::<(Entity, &DmItem, &StoredIn)>();
        q.iter(world)
            .any(|(_, i, inside)| i.path == SLIP && inside.0 == cabinet)
    };
    assert!(filed, "the mapped slip is filed");
    assert!(s.try_get::<FilingCabinet>(cabinet).is_some());
    let fluff = s.item(FLUFF);
    s.stand(TilePos::new(9, 3, 1));
    s.put_in_hand(fluff);
    s.click(0, cabinet);
    s.run_until("the paper is filed", |s| {
        s.try_get::<StoredIn>(fluff).is_some_and(|i| i.0 == cabinet)
    });
    s.empty_hand();
    s.click(0, cabinet);
    s.pick("fluff");
    s.run_until("the paper is back in hand", |s| s.active() == Some(fluff));
}

impl Station {
    /// The barcode scanner in the active hand used in hand, until it is in `mode`.
    fn scan_mode(&mut self, scanner: Entity, mode: ScanMode) {
        let slot = self.active_slot();
        self.send(0, ClientIntent::ClickSlot(slot));
        self.run_until(&format!("the scanner is in {mode:?}"), |s| {
            s.get::<BarcodeScanner>(scanner).mode == mode
        });
    }
}

#[test]
fn the_barcode_scanner_keeps_the_console_ledger() {
    let mut s = Station::start("the_barcode_scanner_keeps_the_console_ledger");
    let (scanner, console, book) = (s.item(BARCODE), s.obj(CONSOLE), s.item("/obj/item/book"));
    s.run_until("the book has its data", |s| {
        s.try_get::<Book>(book).is_some()
    });
    s.put_in_hand(scanner);
    // Not connected: the mode does not change.
    let slot = s.active_slot();
    s.send(0, ClientIntent::ClickSlot(slot));
    for _ in 0..5 {
        s.tick();
    }
    assert_eq!(s.get::<BarcodeScanner>(scanner).mode, ScanMode::CheckIn);

    s.stand(TilePos::new(8, 3, 1));
    s.click(0, console);
    s.run_until("the scanner connects", |s| {
        s.get::<BarcodeScanner>(scanner).computer == Some(console)
    });
    s.scan_mode(scanner, ScanMode::CheckOut);
    s.scan_mode(scanner, ScanMode::Inventory);
    s.stand(TilePos::new(2, 3, 1));
    s.click(0, book);
    s.run_until("the book is in the inventory", |s| {
        s.get::<Ledger>(console).inventory.len() == 1
    });

    s.scan_mode(scanner, ScanMode::CheckIn);
    s.scan_mode(scanner, ScanMode::CheckOut);
    s.click(0, book);
    s.run_until("the book is set for check-out", |s| {
        s.get::<Ledger>(console).checking_out.is_some()
    });
    let lent = s
        .server
        .world_mut()
        .get_mut::<Ledger>(console)
        .unwrap()
        .lend("Bob", 600.0);
    assert!(lent);

    s.scan_mode(scanner, ScanMode::Inventory);
    s.scan_mode(scanner, ScanMode::CheckIn);
    s.click(0, book);
    s.run_until("the book is checked in", |s| {
        s.get::<Ledger>(console).checkouts.is_empty()
    });
    assert_eq!(s.get::<Ledger>(console).inventory.len(), 1);
}

#[test]
fn the_librarian_console_lends_prints_and_hides_lore() {
    let mut s = Station::start("the_librarian_console_lends_prints_and_hides_lore");
    let (console, book) = (s.obj(CONSOLE), s.item("/obj/item/book"));
    s.run_until("the book has its data", |s| {
        s.try_get::<Book>(book).is_some()
    });
    let data = s.get::<Book>(book).clone();
    s.server
        .world_mut()
        .get_mut::<Ledger>(console)
        .unwrap()
        .inventory
        .push(data.clone());
    s.stand(TilePos::new(8, 3, 1));
    s.click(0, console);
    let window = s.console_window();
    s.act(0, window, "set_screen", &[("screen_index", "2")]);
    s.act(0, window, "set_checkout", &[("book_name", &data.title)]);
    s.run_until("the book is set for check-out", |s| {
        s.get::<Ledger>(console).checking_out.is_some()
    });
    s.act(
        0,
        window,
        "checkout",
        &[("loaned_to", "Bob"), ("checkout_time", "1")],
    );
    s.run_until("the book is lent", |s| {
        s.get::<Ledger>(console).checkouts.len() == 1
    });
    let loan = s.get::<Ledger>(console).checkouts[0].clone();
    assert_eq!(loan.loaned_to, "Bob");
    assert_eq!(loan.book, data);
    s.act(0, window, "checkin", &[("checked_out_id", "1")]);
    s.run_until("the book is checked in", |s| {
        s.get::<Ledger>(console).checkouts.is_empty()
    });

    let bibles = |s: &mut Station| {
        let world = s.server.world_mut();
        let mut q = world.query::<&DmItem>();
        q.iter(world)
            .filter(|i| i.path == "/obj/item/book/bible")
            .count()
    };
    s.act(0, window, "print_bible", &[]);
    s.run_until("the bible is printed", |s| bibles(s) == 1);

    // Not emagged: the lore is refused.
    s.act(0, window, "lore_spawn", &[]);
    for _ in 0..3 {
        s.tick();
    }
    assert!(!s.get::<Ledger>(console).lore_taken);
    let card = s.server.world_mut().spawn_empty().id();
    let actor = ss13_game::tools::Actor {
        client: s.players[0].link,
        mob: s.players[0].mob,
        item: Some(card),
    };
    assert!(ss13_game::emag::emag_act(
        s.server.world_mut(),
        actor,
        card,
        console
    ));
    assert!(s.get::<Ledger>(console).emagged);
    s.act(0, window, "lore_deny", &[]);
    s.run_until("the lore tab is gone", |s| {
        s.get::<Ledger>(console).lore_taken
    });
}
