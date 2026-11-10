//! Smoking over real WebTransport on 127.0.0.1 (`ss13_game::smoking`): a zippo lights in hand
//! and is as hot as tg's (`heat_while_on`); held over the cigarette in the other hand it lights
//! it, and the cigarette burns down to its butt.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::combat::NextMove;
use ss13_game::inventory::{CarriedBy, Inventory};
use ss13_game::smoking::Smokable;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_game::tools::Welder;
use ss13_mapload::DmItem;
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
        "tool_behaviour": null, "toolspeed": 1, "heat": 0 } },
    "/obj/item/lighter": { "parent": "/obj/item", "declared_vars": {
        "name": "Zippo lighter", "icon_state": "zippo", "tool_behaviour": "welder",
        "toolspeed": 1.5 } },
    "/obj/item/cigarette": { "parent": "/obj/item", "declared_vars": {
        "name": "cigarette", "icon_state": "cigoff", "heat": 1000, "smoketime": 20,
        "dragtime": 100, "chem_volume": 30, "list_reagents": null, "smoke_all": 0,
        "lung_harm": 1, "type_butt": { "path": "/obj/item/cigbutt" }, "icon_on": "cigon",
        "icon_off": "cigoff", "inhand_icon_on": "cigon", "inhand_icon_off": "cigoff",
        "starts_lit": 0 } },
    "/obj/item/cigbutt": { "parent": "/obj/item", "declared_vars": {
        "name": "cigarette butt", "icon_state": "cigbutt" } },
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

/// Rows top-down, the last line is y = 1. The spawn `s` (2,2).
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)

(1,1,1) = {"
wwww
wffw
wsfw
wwww
"}
"##;

const SPAWN: TilePos = TilePos::new(2, 2, 1);

const STATES: [(&str, &str); 10] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
    ("i.dmi", "zippo"),
    ("i.dmi", "zippo-on"),
    ("i.dmi", "cigoff"),
    ("i.dmi", "cigon"),
    ("i.dmi", "cigbutt"),
];

fn fixture_assets() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("smoking_fixture");
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/item/lighter",
        "/obj/item/cigarette",
        "/obj/item/cigbutt",
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
        s.join("p0", |_| {});
        let s = Self(s);
        assert_eq!(*s.get::<TilePos>(s.mob()), SPAWN);
        s
    }

    fn mob(&self) -> Entity {
        self.players[0].mob
    }

    /// A new `path` item straight into hand `hand`.
    fn give(&mut self, path: &str, hand: usize) -> Entity {
        let mob = self.mob();
        let world = self.server.world_mut();
        let item = ss13_game::construction::spawn_obj(world, path, SPAWN).unwrap();
        world
            .entity_mut(item)
            .remove::<TilePos>()
            .insert(CarriedBy(mob));
        world.get_mut::<Inventory>(mob).unwrap().hands[hand] = Some(item);
        item
    }

    /// A click on hand slot `hand` once off the click cooldown.
    fn click_hand(&mut self, hand: usize) {
        let mob = self.mob();
        self.run_until("off the click cooldown", |s| {
            s.now() >= s.get::<NextMove>(mob).0
        });
        let slot = if hand == 0 {
            InventorySlot::LeftHand
        } else {
            InventorySlot::RightHand
        };
        self.send(0, ClientIntent::ClickSlot(slot));
    }

    fn butts(&mut self) -> usize {
        let world = self.server.world_mut();
        let mut q = world.query::<&DmItem>();
        q.iter(world)
            .filter(|i| i.path == "/obj/item/cigbutt")
            .count()
    }
}

#[test]
fn a_zippo_lights_a_cigarette_that_burns_to_its_butt() {
    let mut s = Station::start();
    let active = s.get::<Inventory>(s.mob()).active;
    let zippo = s.give("/obj/item/lighter", active);
    let cig = s.give("/obj/item/cigarette", 1 - active);
    s.run_until("the zippo has its fuel", |s| {
        s.try_get::<Welder>(zippo).is_some_and(|w| w.fuel == 6.0)
    });

    // In hand it lights, as hot as `HIGH_TEMPERATURE_REQUIRED - 100`.
    s.click_hand(active);
    s.run_until("the zippo is lit", |s| s.get::<Welder>(zippo).on);
    let heat =
        ss13_game::medical::surgery::ToolInfo::read(s.server.world(), Some(zippo)).temperature;
    assert_eq!(heat, 1400.0);

    // Over the cigarette in the other hand: lit, then burnt down to the butt.
    assert_eq!(s.butts(), 0);
    s.click_hand(1 - active);
    s.run_until("the cigarette is lit", |s| {
        s.try_get::<Smokable>(cig).is_some_and(|c| c.lit)
    });
    s.run_until("the cigarette burnt out", |s| {
        s.server.world().get_entity(cig).is_err()
    });
    assert_eq!(s.butts(), 1);
    // The zippo stays lit.
    assert!(s.get::<Welder>(zippo).on);
}
