//! Religion over a loopback server: the high priest's null rod becomes the picked form (the
//! station's holy weapon); the bible on the altar of the gods picks the machine god; with the
//! favor for it the "Receive Blessing" rite runs its invocations and pays the favor.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::inventory::{CarriedBy, Inventory};
use ss13_game::service::{HolyRole, PerformingRite, Religion, Sect};
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::{DmItem, DmObj};
use ss13_shared::TilePos;
use ss13_shared::protocol::{ClientIntent, InventorySlot};
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
    "/obj/item/book": { "parent": "/obj/item", "declared_vars": {
        "starting_title": null, "starting_author": null, "starting_content": null } },
    "/obj/item/book/bible": { "parent": "/obj/item/book" },
    "/obj/item/nullrod": { "parent": "/obj/item" },
    "/obj/item/nullrod/claymore": { "parent": "/obj/item/nullrod" },
    "/obj/item/nullrod/staff": { "parent": "/obj/item/nullrod" },
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
    "/obj/structure/altar": { "parent": "/obj/structure", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "card_grey" } },
    "/obj/structure/altar/of_gods": { "parent": "/obj/structure/altar" },
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

/// The player starts at (2,3): the altar east (3,3), the null rod south (2,2), the bible (3,2).
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"a" = (/obj/structure/altar/of_gods,/turf/open/floor,/area/space)
"n" = (/obj/item/nullrod,/turf/open/floor,/area/space)
"b" = (/obj/item/book/bible,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwww
wfffw
wsafw
wnbfw
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

const NULLROD: &str = "/obj/item/nullrod";
const CLAYMORE: &str = "/obj/item/nullrod/claymore";
const BIBLE: &str = "/obj/item/book/bible";
const ALTAR: &str = "/obj/structure/altar/of_gods";

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("religion_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        NULLROD,
        CLAYMORE,
        "/obj/item/nullrod/staff",
        BIBLE,
        ALTAR,
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
            .insert((TilePos::new(2, 3, 1), HolyRole::HighPriest));
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

    fn altar(&mut self) -> Entity {
        let world = self.server.world_mut();
        let mut q = world.query::<(Entity, &DmObj)>();
        q.iter(world)
            .find(|(_, o)| o.path == ALTAR)
            .map(|(e, _)| e)
            .expect("the altar")
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
                .insert(TilePos::new(3, 4, 1));
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

    fn list_window(&mut self) -> Option<u32> {
        let link = self.players[0].link;
        let world = self.server.world_mut();
        let mut q = world.query::<&UiWindow>();
        q.iter(world)
            .filter(|w| w.client == link && w.interface == ListInputUi::NAME)
            .map(|w| w.id)
            .max()
    }

    /// A list prompt newer than `after` opens; its id.
    fn next_list(&mut self, after: Option<u32>) -> u32 {
        self.run_until("a list prompt opens", |s| s.list_window() > after);
        self.list_window().unwrap()
    }

    fn religion(&mut self) -> Religion {
        self.server.world_mut().resource::<Religion>().clone()
    }
}

#[test]
fn the_high_priest_picks_a_holy_weapon_a_sect_and_performs_a_rite() {
    let mut s = Station::start("the_high_priest_picks_a_holy_weapon_a_sect_and_performs_a_rite");
    let mob = s.players[0].mob;

    // The null rod in hand asks for its form.
    let rod = s.item(NULLROD);
    s.put_in_hand(rod);
    let slot = s.active_slot();
    s.send(0, ClientIntent::ClickSlot(slot));
    let list = s.next_list(None);
    s.act(0, list, "submit", &[("entry", "claymore")]);
    s.run_until("the rod becomes a claymore", |s| {
        let held = s.get::<Inventory>(mob).active_item();
        held.and_then(|h| s.try_get::<DmItem>(h))
            .is_some_and(|i| i.path == CLAYMORE)
    });
    assert_eq!(s.religion().holy_weapon.as_deref(), Some(CLAYMORE));

    // The bible on the altar: the sects.
    let (bible, altar) = (s.item(BIBLE), s.altar());
    s.put_in_hand(bible);
    s.click(0, altar);
    let list = s.next_list(Some(list));
    s.act(0, list, "submit", &[("entry", "Mechanical God")]);
    s.run_until("the machine god is the sect", |s| {
        s.religion().sect == Some(Sect::Mechanical)
    });

    // With the favor for it, the blessing rite runs and pays.
    s.server.world_mut().resource_mut::<Religion>().favor = 2100.0;
    s.click(0, altar);
    let list = s.next_list(Some(list));
    s.act(
        0,
        list,
        "submit",
        &[("entry", "Receive Blessing (2000 favor)")],
    );
    s.run_until("the rite begins", |s| {
        s.try_get::<PerformingRite>(mob).is_some()
    });
    s.run_until_for(10.0, "the rite is paid", |s| s.religion().favor == 100.0);
    assert!(s.try_get::<PerformingRite>(mob).is_none());
}
