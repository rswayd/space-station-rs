//! Playing cards (`ss13_game::service::cards`): a deck deals into the hands and takes cards
//! back, two cards make a hand, a card is taken out of a hand by its list.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::inventory::Inventory;
use ss13_game::service::cards::{CardHand, Deck, SingleCard};
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::DmItem;
use ss13_shared::TilePos;
use ss13_shared::protocol::{ClickModifiers, ClientIntent, InventorySlot};
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::list_input::ListInputUi;
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
    "/obj/item/toy": { "parent": "/obj/item" },
    "/obj/item/toy/cards": { "parent": "/obj/item/toy" },
    "/obj/item/toy/cards/deck": { "parent": "/obj/item/toy/cards" },
    "/obj/item/toy/cards/cardhand": { "parent": "/obj/item/toy/cards" },
    "/obj/item/toy/singlecard": { "parent": "/obj/item/toy" },
    "/obj/item/toy/singlecard/wizoff_ruleset": { "parent": "/obj/item/toy/singlecard" },
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

/// The player starts at (2,3), the deck lies east (3,3).
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"d" = (/obj/item/toy/cards/deck,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwww
wfffw
wsdfw
wfffw
wwwww
"}
"##;

const STATES: [(&str, &str); 5] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "card_grey"),
];

const DECK: &str = "/obj/item/toy/cards/deck";

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("cards_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        DECK,
        "/obj/item/toy/cards/cardhand",
        "/obj/item/toy/singlecard",
        "/obj/item/toy/singlecard/wizoff_ruleset",
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

    fn deck(&mut self) -> Entity {
        let world = self.server.world_mut();
        let mut q = world.query::<(Entity, &DmItem)>();
        q.iter(world)
            .find(|(_, i)| i.path == DECK)
            .map(|(e, _)| e)
            .expect("the deck on the map")
    }

    fn hands(&self) -> [Option<Entity>; 2] {
        self.get::<Inventory>(self.players[0].mob).hands
    }

    fn active(&self) -> Option<Entity> {
        self.get::<Inventory>(self.players[0].mob).active_item()
    }

    fn card_in(&self, item: Option<Entity>) -> Option<(String, bool)> {
        item.and_then(|e| self.try_get::<SingleCard>(e))
            .map(|c| (c.0.name.clone(), c.0.face_up))
    }

    fn deck_len(&self, deck: Entity) -> usize {
        self.get::<Deck>(deck).cards.len()
    }

    fn slot_of_active(&self) -> InventorySlot {
        if self.get::<Inventory>(self.players[0].mob).active == 0 {
            InventorySlot::LeftHand
        } else {
            InventorySlot::RightHand
        }
    }

    fn window(&mut self, interface: &str) -> Option<u32> {
        let link = self.players[0].link;
        let world = self.server.world_mut();
        let mut q = world.query::<&UiWindow>();
        q.iter(world)
            .filter(|w| w.client == link && w.interface == interface)
            .map(|w| w.id)
            .max()
    }
}

#[test]
fn a_deck_deals_takes_back_and_cards_make_a_hand() {
    let mut s = Station::start("a_deck_deals_takes_back_and_cards_make_a_hand");
    let deck = s.deck();
    s.run_until("the deck has its cards", |s| {
        s.try_get::<Deck>(deck).is_some()
    });
    assert_eq!(s.deck_len(deck), 54);

    // An empty hand draws the top card face down.
    s.click(0, deck);
    s.run_until("a card is in hand", |s| s.card_in(s.active()).is_some());
    assert_eq!(
        s.card_in(s.active()),
        Some(("Joker Clown".to_owned(), false))
    );
    assert_eq!(s.deck_len(deck), 53);
    let first = s.active().unwrap();

    // The card goes back in, at the bottom.
    s.click(0, deck);
    s.run_until("the card is back in the deck", |s| s.deck_len(deck) == 54);
    assert!(
        s.try_get::<SingleCard>(first).is_none(),
        "the card atom is gone"
    );
    assert_eq!(s.get::<Deck>(deck).cards[53].name, "Joker Clown");
    assert_eq!(s.active(), None);

    // A right click draws face up.
    s.click_with(
        0,
        deck,
        ClickModifiers {
            right: true,
            ..Default::default()
        },
    );
    s.run_until("a face up card is in hand", |s| {
        s.card_in(s.active()).is_some()
    });
    assert_eq!(s.card_in(s.active()), Some(("Joker Mime".to_owned(), true)));

    // The other hand draws the next one, then uses it on the first: a hand of two.
    s.send(0, ClientIntent::SwapHands);
    s.run_until("the other hand is active", |s| s.active().is_none());
    s.click(0, deck);
    s.run_until("a second card is in hand", |s| {
        s.card_in(s.active()).is_some()
    });
    assert_eq!(s.card_in(s.active()).unwrap().0, "Ace of Hearts");
    let other = if s.slot_of_active() == InventorySlot::LeftHand {
        InventorySlot::RightHand
    } else {
        InventorySlot::LeftHand
    };
    s.send(0, ClientIntent::ClickSlot(other));
    s.run_until("the cards make a hand", |s| {
        s.active()
            .is_some_and(|h| s.try_get::<CardHand>(h).is_some())
    });
    let hand = s.active().unwrap();
    let names: Vec<String> = s
        .get::<CardHand>(hand)
        .cards
        .iter()
        .map(|c| c.name.clone())
        .collect();
    assert_eq!(names, ["Joker Mime", "Ace of Hearts"]);
    assert_eq!(s.hands().iter().flatten().count(), 1, "one hand of cards");

    // Taking a card out of a hand of two leaves the last one in the hands too.
    let slot = s.slot_of_active();
    s.send(0, ClientIntent::ClickSlot(slot));
    s.run_until("the card list opens", |s| {
        s.window(ListInputUi::NAME).is_some()
    });
    let list = s.window(ListInputUi::NAME).unwrap();
    s.act(0, list, "submit", &[("entry", "Ace of Hearts")]);
    s.run_until("both cards are in the hands", |s| {
        s.hands().iter().all(|&h| s.card_in(h).is_some())
    });
    assert!(s.try_get::<CardHand>(hand).is_none(), "the hand is gone");
    let mut held: Vec<String> = s
        .hands()
        .iter()
        .filter_map(|&h| s.card_in(h).map(|c| c.0))
        .collect();
    held.sort();
    assert_eq!(held, ["Ace of Hearts", "Joker Mime"]);
}
