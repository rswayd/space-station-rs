//! Single items over real WebTransport on 127.0.0.1 (`ss13_game::misc_items`): the station
//! charter renames the station (a standard name at once, another after the admins' wait unless
//! rejected) and is used up; a keycard door opens only for its keycard, a keycard panel opens its
//! puzzle door once; a typed flatpack holds its board, a flatpack cart takes three packs and
//! gives back the topmost.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::combat::NextMove;
use ss13_game::doors::{Door, DoorState};
use ss13_game::eng_machines::flatpacker::Flatpack;
use ss13_game::inventory::{CarriedBy, Inventory};
use ss13_game::misc_items::charter::{Charter, StationName, reject_proposed};
use ss13_game::misc_items::flatpacks::FlatpackCart;
use ss13_game::misc_items::puzzle::PuzzleUsed;
use ss13_game::storage::StoredIn;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_net::chat::client::ChatReceived;
use ss13_shared::TilePos;
use ss13_shared::protocol::{ClientIntent, InventorySlot};
use ss13_tgui::prompt::{Prompt, PromptAnswer, PromptAnswered};
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
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null, "attack_speed": 8,
        "tool_behaviour": null, "toolspeed": 1 } },
    "/obj/item/station_charter": { "parent": "/obj/item", "declared_vars": {
        "name": "station charter", "icon_state": "charter" } },
    "/obj/item/keycard": { "parent": "/obj/item", "declared_vars": {
        "name": "security keycard", "icon_state": "keycard", "puzzle_id": null } },
    "/obj/item/keycard/yellow": { "parent": "/obj/item/keycard", "declared_vars": {
        "name": "yellow keycard", "puzzle_id": "yellow" } },
    "/obj/item/keycard/blue": { "parent": "/obj/item/keycard", "declared_vars": {
        "name": "blue keycard", "puzzle_id": "blue" } },
    "/obj/item/keycard/vault": { "parent": "/obj/item/keycard", "declared_vars": {
        "puzzle_id": "vault" } },
    "/obj/machinery": { "parent": "/obj", "declared_vars": { "anchored": 1,
        "use_power": 1, "idle_power_usage": 100, "active_power_usage": 1000, "power_channel": 1,
        "req_access": null, "req_one_access": null } },
    "/obj/machinery/door": { "parent": "/obj/machinery", "declared_vars": {
        "icon": { "resource": "pz.dmi" }, "icon_state": "door_closed", "density": 1,
        "opacity": 1, "layer": 2.7, "base_icon_state": "door", "autoclose": 0,
        "can_open_with_hands": 1, "closingLayer": 3.1, "glass": 0, "locked": 0, "safe": 1,
        "visible": 1, "welded": 0, "power_channel": 3, "idle_power_usage": 10,
        "active_power_usage": 200, "id_tag": null } },
    "/obj/machinery/door/puzzle": { "parent": "/obj/machinery/door", "declared_vars": {
        "name": "locked door", "can_open_with_hands": 0, "puzzle_id": null,
        "uses_queuelinks": 1, "open_message": "The door beeps, and slides opens." } },
    "/obj/machinery/door/puzzle/keycard": { "parent": "/obj/machinery/door/puzzle",
        "declared_vars": { "uses_queuelinks": 0 } },
    "/obj/machinery/door/puzzle/keycard/yellow_required": {
        "parent": "/obj/machinery/door/puzzle/keycard", "declared_vars": { "puzzle_id": "yellow" } },
    "/obj/machinery/puzzle": { "parent": "/obj/machinery", "declared_vars": {
        "icon": { "resource": "wm.dmi" }, "used": 0, "single_use": 1, "id": null,
        "queue_size": 2 } },
    "/obj/machinery/puzzle/keycardpad": { "parent": "/obj/machinery/puzzle", "declared_vars": {
        "name": "keycard panel", "icon_state": "keycardpad0", "base_icon_state": "keycardpad" } },
    "/obj/item/flatpack": { "parent": "/obj/item", "declared_vars": {
        "name": "flatpack", "icon_state": "flatpack", "density": 1 } },
    "/obj/item/flatpack/flatpacker": { "parent": "/obj/item/flatpack", "declared_vars": {
        "name": "flatpacker" } },
    "/obj/item/circuitboard": { "parent": "/obj/item" },
    "/obj/item/circuitboard/machine": { "parent": "/obj/item/circuitboard" },
    "/obj/item/circuitboard/machine/flatpacker": { "parent": "/obj/item/circuitboard/machine" },
    "/obj/structure/flatpack_cart": { "parent": "/obj/structure", "declared_vars": {
        "name": "flatpack cart", "icon": { "resource": "st.dmi" }, "icon_state": "flatcart",
        "density": 1 } },
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

/// Rows top-down, the last line is y = 1. The spawn `s` (2,3); a yellow keycard door `K`
/// (3,3); a keycard panel `B` (2,2) of the puzzle `vault`, whose puzzle door is `D` (3,2); a
/// flatpack cart `C` (2,4).
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"K" = (/obj/machinery/door/puzzle/keycard/yellow_required,/turf/open/floor,/area/space)
"B" = (/obj/machinery/puzzle/keycardpad{id = "vault"},/turf/open/floor,/area/space)
"D" = (/obj/machinery/door/puzzle{puzzle_id = "vault"},/turf/open/floor,/area/space)
"C" = (/obj/structure/flatpack_cart,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwww
wCffw
wsKfw
wBDfw
wwwww
"}
"##;

const KEYCARD_DOOR: TilePos = TilePos::new(3, 3, 1);
const PAD: TilePos = TilePos::new(2, 2, 1);
const PUZZLE_DOOR: TilePos = TilePos::new(3, 2, 1);
const CART: TilePos = TilePos::new(2, 4, 1);

const SPAWN: TilePos = TilePos::new(2, 3, 1);

const STATES: [(&str, &str); 16] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
    ("i.dmi", "charter"),
    ("i.dmi", "keycard"),
    ("pz.dmi", "door_closed"),
    ("pz.dmi", "door_opening"),
    ("pz.dmi", "door_open"),
    ("pz.dmi", "door_closing"),
    ("wm.dmi", "keycardpad0"),
    ("wm.dmi", "keycardpad1"),
    ("i.dmi", "flatpack"),
    ("st.dmi", "flatcart"),
    ("st.dmi", "flatcart_flat"),
];

fn fixture_assets() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("misc_items_fixture");
    std::fs::create_dir_all(dir.join("strings")).unwrap();
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/item/station_charter",
        "/obj/item/keycard/yellow",
        "/obj/item/keycard/blue",
        "/obj/item/keycard/vault",
        "/obj/machinery/door/puzzle",
        "/obj/machinery/door/puzzle/keycard/yellow_required",
        "/obj/machinery/puzzle/keycardpad",
        "/obj/item/flatpack",
        "/obj/item/flatpack/flatpacker",
        "/obj/item/circuitboard/machine/flatpacker",
        "/obj/structure/flatpack_cart",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
    ];
    write_fixture(
        &dir,
        TYPES,
        &used,
        &STATES,
        &[
            ("map.dmm", MAP),
            ("strings/station_prefixes.txt", "Imperial\n"),
            ("strings/station_names.txt", "Aegis\nSpace\n"),
            ("strings/station_suffixes.txt", "Station\nOutpost\n"),
            ("strings/greek_letters.txt", "Alpha\n"),
            ("strings/phonetic_alphabet.txt", "Bravo\n"),
            ("strings/numbers_as_words.txt", "One\n"),
        ],
    )
}

#[derive(Resource, Default)]
struct Received(Vec<String>);

fn record(mut chat: MessageReader<ChatReceived>, mut received: ResMut<Received>) {
    for ChatReceived(message) in chat.read() {
        received.0.push(message.plain_text());
    }
}

struct Station(testing::Station);

ss13_game::drive_station!(Station, 0);

impl Station {
    fn start() -> Self {
        let assets = fixture_assets();
        let mut s = testing::Station::start(fixture_map(&assets, &["map.dmm"]), |_| {});
        s.join("p0", |app| {
            app.init_resource::<Received>().add_systems(Update, record);
        });
        let s = Self(s);
        assert_eq!(*s.get::<TilePos>(s.mob()), SPAWN);
        s
    }

    fn mob(&self) -> Entity {
        self.players[0].mob
    }

    fn held(&self) -> Option<Entity> {
        self.get::<Inventory>(self.mob()).active_item()
    }

    /// A new `path` item straight into the active hand, the old one gone.
    fn give_new(&mut self, path: &str) -> Entity {
        let mob = self.mob();
        let old = self.held();
        let world = self.server.world_mut();
        if let Some(old) = old {
            world.despawn(old);
        }
        let item = ss13_game::construction::spawn_obj(world, path, SPAWN).unwrap();
        world
            .entity_mut(item)
            .remove::<TilePos>()
            .insert(CarriedBy(mob));
        let mut inventory = world.get_mut::<Inventory>(mob).unwrap();
        let hand = inventory.active;
        inventory.hands[hand] = Some(item);
        item
    }

    /// A click on the active hand's slot (tg `attack_self`) once off the click cooldown.
    fn use_in_hand(&mut self) {
        let mob = self.mob();
        self.run_until("off the click cooldown", |s| {
            s.now() >= s.get::<NextMove>(mob).0
        });
        let slot = if self.get::<Inventory>(mob).active == 0 {
            InventorySlot::LeftHand
        } else {
            InventorySlot::RightHand
        };
        self.send(0, ClientIntent::ClickSlot(slot));
    }

    fn prompt(&mut self) -> Option<Entity> {
        let mob = self.mob();
        let world = self.server.world_mut();
        let mut q = world.query::<(Entity, &Prompt)>();
        q.iter(world).find(|(_, p)| p.actor == mob).map(|(e, _)| e)
    }

    /// Waits for the question and answers it with `text`.
    fn answer_text(&mut self, text: &str) {
        self.run_until("a question", |s| s.prompt().is_some());
        let prompt = self.prompt().unwrap();
        let (client, actor) = (self.players[0].link, self.mob());
        self.server.world_mut().write_message(PromptAnswered {
            prompt,
            client,
            actor,
            answer: Some(PromptAnswer::Text(text.to_owned())),
        });
        self.tick();
        if let Ok(e) = self.server.world_mut().get_entity_mut(prompt) {
            e.despawn();
        }
    }

    fn said(&self, text: &str) -> bool {
        self.players[0]
            .app
            .world()
            .resource::<Received>()
            .0
            .iter()
            .any(|l| l.contains(text))
    }

    fn door_at(&self, pos: TilePos) -> Entity {
        self.on_tile::<Door>(pos)
    }

    fn on_tile<T: Component>(&self, pos: TilePos) -> Entity {
        let world = self.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .find(|&e| world.get::<T>(e).is_some())
            .unwrap_or_else(|| panic!("nothing with {} on {pos:?}", std::any::type_name::<T>()))
    }

    fn closed(&self, door: Entity) -> bool {
        self.get::<Door>(door).state == DoorState::Closed
    }

    /// A click on `e` once off the click cooldown and replicated.
    fn click_on(&mut self, e: Entity) {
        let mob = self.mob();
        self.run_until("off the click cooldown and replicated", |s| {
            s.now() >= s.get::<NextMove>(mob).0 && s.net_id(0, e).is_some()
        });
        self.0.click(0, e);
    }

    fn station_name(&self) -> String {
        self.server.world().resource::<StationName>().0.clone()
    }
}

#[test]
fn charter_renames_the_station_once() {
    let mut s = Station::start();
    let charter = s.give_new("/obj/item/station_charter");

    // A name outside the standard format waits for the admins; one rejects it.
    s.use_in_hand();
    s.answer_text("Butt Land");
    s.run_until("sent for approval", |s| s.said("отправлено работодателям"));
    assert!(s.get::<Charter>(charter).pending.is_some());
    s.use_in_hand();
    s.run_until("still waiting", |s| s.said("всё ещё ждёте ответа"));
    assert!(reject_proposed(s.server.world_mut(), charter));
    assert!(s.get::<Charter>(charter).pending.is_none());
    assert!(!reject_proposed(s.server.world_mut(), charter));
    assert_eq!(s.station_name(), "");

    // A standard name is approved at once and uses the charter up.
    s.use_in_hand();
    s.answer_text("Imperial Aegis Station 13");
    s.run_until("renamed", |s| {
        s.station_name() == "Imperial Aegis Station 13"
    });
    // The chat line reaches the client a frame after the rename.
    s.run_until("told", |s| s.said("одобрено автоматически"));
    assert!(s.get::<Charter>(charter).used);
    assert_eq!(
        s.get::<Name>(charter).as_str(),
        "station charter for Imperial Aegis Station 13"
    );
    s.use_in_hand();
    s.run_until("used up", |s| s.said("уже назвали станцию"));

    // Nobody rejects another charter's name: approved after the wait.
    s.give_new("/obj/item/station_charter");
    s.use_in_hand();
    s.answer_text("Butt Land");
    s.hold(50.0, "waiting for the admins", |s| {
        s.station_name() == "Imperial Aegis Station 13"
    });
    s.run_until_for(15.0, "approved by itself", |s| {
        s.station_name() == "Butt Land"
    });
}

#[test]
fn keycards_open_their_puzzle_doors() {
    let mut s = Station::start();
    let keycard_door = s.door_at(KEYCARD_DOOR);
    let puzzle_door = s.door_at(PUZZLE_DOOR);
    let pad = s.on_tile::<ss13_mapload::DmPuzzle>(PAD);
    assert!(s.closed(keycard_door) && s.closed(puzzle_door));

    // The wrong keycard buzzes; the yellow one opens the door for good.
    s.give_new("/obj/item/keycard/blue");
    s.click_on(keycard_door);
    s.run_until("buzzed", |s| s.said("не тот ключ"));
    assert!(s.closed(keycard_door));
    s.give_new("/obj/item/keycard/yellow");
    s.click_on(keycard_door);
    s.run_until("opened", |s| !s.closed(keycard_door));
    s.hold(3.0, "it stays open", |s| !s.closed(keycard_door));

    // The panel takes only the vault keycard, once, and opens the vault door.
    s.give_new("/obj/item/keycard/yellow");
    s.click_on(pad);
    s.hold(1.5, "a wrong card opens nothing", |s| s.closed(puzzle_door));
    assert!(s.try_get::<PuzzleUsed>(pad).is_none());
    s.give_new("/obj/item/keycard/vault");
    s.click_on(pad);
    s.run_until("the vault door opens", |s| !s.closed(puzzle_door));
    assert!(s.try_get::<PuzzleUsed>(pad).is_some());
}

#[test]
fn typed_flatpacks_and_the_cart() {
    let mut s = Station::start();
    let cart = s.on_tile::<ss13_mapload::DmObj>(CART);
    // A typed flatpack holds its board from the start.
    let typed = s.give_new("/obj/item/flatpack/flatpacker");
    s.run_until("packed", |s| s.try_get::<Flatpack>(typed).is_some());
    let board = s.get::<Flatpack>(typed).board;
    assert_eq!(
        s.try_get::<StoredIn>(board).map(|i| i.0),
        Some(typed),
        "the board is inside"
    );

    // Three packs go onto the cart, a fourth does not; an empty hand takes the topmost.
    let mut loaded = Vec::new();
    for _ in 0..3 {
        let pack = s.give_new("/obj/item/flatpack/flatpacker");
        s.click_on(cart);
        s.run_until("loaded", |s| {
            s.try_get::<StoredIn>(pack).is_some_and(|i| i.0 == cart)
        });
        loaded.push(pack);
    }
    let extra = s.give_new("/obj/item/flatpack/flatpacker");
    s.click_on(cart);
    s.run_until("full", |s| s.said("Тележка заполнена"));
    assert_eq!(s.held(), Some(extra));
    assert_eq!(s.get::<FlatpackCart>(cart).0, loaded);
    assert_eq!(s.get::<ss13_shared::Appearance>(cart).overlays.len(), 3);

    let mob = s.mob();
    let world = s.server.world_mut();
    world.despawn(extra);
    let mut inventory = world.get_mut::<Inventory>(mob).unwrap();
    let hand = inventory.active;
    inventory.hands[hand] = None;
    s.click_on(cart);
    s.run_until("the topmost in hand", |s| s.held() == Some(loaded[2]));
    assert_eq!(s.get::<FlatpackCart>(cart).0, loaded[..2]);
    assert_eq!(s.get::<ss13_shared::Appearance>(cart).overlays.len(), 2);
}
