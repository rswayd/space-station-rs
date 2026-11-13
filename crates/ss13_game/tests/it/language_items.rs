//! Language items over real WebTransport on 127.0.0.1 (`ss13_game::language_manuals`,
//! `ss13_game::speechmods`): a codespeak manual read in hand teaches Codespeak and is used up,
//! a second read of an unlimited one only says it is known, the pirate manual counts its
//! charges; a French beret on the head and a foam lobster head held add their accents, which go
//! with them.

use std::path::{Path, PathBuf};

use crate::harness::chat::{Received, record};
use bevy::prelude::*;
use ss13_chat::accents::{Accent, Accents};
use ss13_chat::language::{LanguageHolder, LanguageId, UNDERSTOOD};
use ss13_game::combat::NextMove;
use ss13_game::inventory::{CarriedBy, Inventory};
use ss13_game::language_manuals::LanguageManual;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_shared::TilePos;
use ss13_shared::protocol::{ClientIntent, InventorySlot};

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
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null, "attack_speed": 8,
        "tool_behaviour": null, "toolspeed": 1 } },
    "/obj/item/language_manual": { "parent": "/obj/item", "declared_vars": {
        "icon_state": "book2", "charges": 1 } },
    "/obj/item/language_manual/codespeak_manual": { "parent": "/obj/item/language_manual",
        "declared_vars": { "name": "codespeak manual" } },
    "/obj/item/language_manual/codespeak_manual/unlimited": {
        "parent": "/obj/item/language_manual/codespeak_manual",
        "declared_vars": { "name": "deluxe codespeak manual" } },
    "/obj/item/language_manual/piratespeak": { "parent": "/obj/item/language_manual",
        "declared_vars": { "name": "Captain Pete's Guide to Pirate Lingo", "charges": 5 } },
    "/obj/item/book": { "parent": "/obj/item" },
    "/obj/item/book/manual": { "parent": "/obj/item/book" },
    "/obj/item/book/manual/chef_recipes": { "parent": "/obj/item/book/manual",
        "declared_vars": { "name": "Chef Recipes" } },
    "/obj/item/clothing": { "parent": "/obj/item" },
    "/obj/item/clothing/head": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 64 } },
    "/obj/item/clothing/head/beret": { "parent": "/obj/item/clothing/head" },
    "/obj/item/clothing/head/beret/frenchberet": { "parent": "/obj/item/clothing/head/beret",
        "declared_vars": { "name": "french beret" } },
    "/obj/item/clothing/head/costume": { "parent": "/obj/item/clothing/head" },
    "/obj/item/clothing/head/costume/lobsterhat": { "parent": "/obj/item/clothing/head/costume",
        "declared_vars": { "name": "foam lobster head" } },
    "/obj/item/card": { "parent": "/obj/item" },
    "/obj/item/card/id": { "parent": "/obj/item/card", "declared_vars": {
        "access": { "list": [] }, "slot_flags": 256 } },
    "/obj/item/card/id/advanced": { "parent": "/obj/item/card/id" },
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

/// Rows top-down, the last line is y = 1. The spawn `s` (2,2).
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)

(1,1,1) = {"
wwww
wsfw
wwww
"}
"##;

const SPAWN: TilePos = TilePos::new(2, 2, 1);

const STATES: [(&str, &str); 6] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
    ("i.dmi", "book2"),
];

fn fixture_assets() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("language_items_fixture");
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/item/language_manual/codespeak_manual",
        "/obj/item/language_manual/codespeak_manual/unlimited",
        "/obj/item/language_manual/piratespeak",
        "/obj/item/book/manual/chef_recipes",
        "/obj/item/clothing/head/beret/frenchberet",
        "/obj/item/clothing/head/costume/lobsterhat",
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

    /// A new `path` item straight into `slot`, what was there gone.
    fn put(&mut self, slot: InventorySlot, path: &str) -> Entity {
        let mob = self.mob();
        let world = self.server.world_mut();
        if let Some(old) = world.get::<Inventory>(mob).and_then(|i| i.slot(slot)) {
            world.despawn(old);
        }
        let item = ss13_game::construction::spawn_obj(world, path, SPAWN).unwrap();
        world
            .entity_mut(item)
            .remove::<TilePos>()
            .insert(CarriedBy(mob));
        *world.get_mut::<Inventory>(mob).unwrap().slot_mut(slot) = Some(item);
        item
    }

    fn active_slot(&self) -> InventorySlot {
        if self.get::<Inventory>(self.mob()).active == 0 {
            InventorySlot::LeftHand
        } else {
            InventorySlot::RightHand
        }
    }

    /// A click on the active hand's slot (tg `attack_self`) once off the click cooldown.
    fn use_in_hand(&mut self) {
        let mob = self.mob();
        self.run_until("off the click cooldown", |s| {
            s.now() >= s.get::<NextMove>(mob).0
        });
        let slot = self.active_slot();
        self.send(0, ClientIntent::ClickSlot(slot));
    }

    fn knows(&self, language: LanguageId) -> bool {
        self.try_get::<LanguageHolder>(self.mob())
            .is_some_and(|h| h.has(language, UNDERSTOOD))
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

    fn accents(&self) -> Vec<Accent> {
        self.try_get::<Accents>(self.mob())
            .map(|a| a.0.clone())
            .unwrap_or_default()
    }
}

#[test]
fn manuals_teach_and_wear_out() {
    let mut s = Station::start();
    assert!(!s.knows(LanguageId::Codespeak));
    let slot = s.active_slot();
    let book = s.put(slot, "/obj/item/language_manual/codespeak_manual");
    s.use_in_hand();
    s.run_until("Codespeak learnt", |s| s.knows(LanguageId::Codespeak));
    // One charge: the manual turns into another book.
    s.run_until("the manual gone", |s| {
        s.server.world().get_entity(book).is_err()
    });
    s.run_until("told", |s| s.said("выскальзывает у вас из рук"));

    // Known already: nothing is used up.
    let deluxe = s.put(slot, "/obj/item/language_manual/codespeak_manual/unlimited");
    s.use_in_hand();
    s.run_until("already known", |s| s.said("но вы уже знаете Codespeak"));
    assert!(s.server.world().get_entity(deluxe).is_ok());

    // Five charges: one lesson leaves four.
    let pirate = s.put(slot, "/obj/item/language_manual/piratespeak");
    s.use_in_hand();
    s.run_until("pirate learnt", |s| s.knows(LanguageId::Piratespeak));
    assert_eq!(s.get::<LanguageManual>(pirate).charges, Some(4));
}

#[test]
fn worn_speechmods_follow_the_inventory() {
    let mut s = Station::start();
    let beret = s.put(
        InventorySlot::Head,
        "/obj/item/clothing/head/beret/frenchberet",
    );
    s.run_until("French", |s| s.accents() == [Accent::French]);
    // The crab hat works held too.
    let hand = s.active_slot();
    let lobster = s.put(hand, "/obj/item/clothing/head/costume/lobsterhat");
    s.run_until("crustacean", |s| {
        s.accents() == [Accent::French, Accent::Crustacean]
    });
    s.server.world_mut().despawn(beret);
    s.run_until("beret off", |s| s.accents() == [Accent::Crustacean]);
    s.server.world_mut().despawn(lobster);
    s.run_until("hat gone", |s| s.accents().is_empty());
}
