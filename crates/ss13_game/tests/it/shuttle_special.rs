//! The luxury shuttle's ticket field over real WebTransport on 127.0.0.1: a passenger without
//! a ticket is stopped, one carrying 500 cr pays with the bill on bumping it and walks through.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::inventory::{CarriedBy, Inventory};
use ss13_game::shuttle_special::LuxuryPassengers;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::DmItem;
use ss13_shared::{Dir, TilePos};
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
    "/obj/item/stack": { "parent": "/obj/item", "declared_vars": { "amount": 1,
        "max_amount": 50, "merge_type": null, "singular_name": null, "mats_per_unit": null } },
    "/obj/item/stack/spacecash": { "parent": "/obj/item/stack", "declared_vars": {
        "name": "space cash" } },
    "/obj/item/stack/spacecash/c500": { "parent": "/obj/item/stack/spacecash" },
    "/obj/item/holochip": { "parent": "/obj/item" },
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
    "/obj/machinery": { "parent": "/obj", "declared_vars": { "anchored": 1 } },
    "/obj/machinery/scanner_gate": { "parent": "/obj/machinery", "declared_vars": {
        "icon": { "resource": "g.dmi" }, "icon_state": "scangate", "name": "scanner gate" } },
    "/obj/machinery/scanner_gate/luxury_shuttle": { "parent": "/obj/machinery/scanner_gate",
        "declared_vars": { "name": "luxury shuttle ticket field", "density": 0 } },
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
    "/area/space": { "parent": "/area" },
    "/area/station": { "parent": "/area" }
  }
}"##;

/// The spawn `s` (2,2) with a 500 cr bill `$` north of it (2,3), the ticket field `G` (3,2).
const MAP: &str = r##""w" = (/turf/closed/wall,/area/station)
"f" = (/turf/open/floor,/area/station)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/station)
"$" = (/obj/item/stack/spacecash/c500,/turf/open/floor,/area/station)
"G" = (/obj/machinery/scanner_gate/luxury_shuttle,/turf/open/floor,/area/station)

(1,1,1) = {"
wwwwww
w$fffw
wsGffw
wwwwww
"}
"##;

const SPAWN: TilePos = TilePos::new(2, 2, 1);
const BILL: TilePos = TilePos::new(2, 3, 1);
const GATE: TilePos = TilePos::new(3, 2, 1);

const STATES: [(&str, &str); 6] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
    ("g.dmi", "scangate"),
];

fn fixture_assets() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("shuttle_special_fixture");
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/item/stack/spacecash/c500",
        "/obj/item/holochip",
        "/obj/machinery/scanner_gate/luxury_shuttle",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
        "/area/station",
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

    fn item(&self, pos: TilePos, path: &str) -> Option<Entity> {
        let world = self.server.world();
        (world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied())
        .find(|&e| world.get::<DmItem>(e).is_some_and(|i| i.path == path))
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
}

#[test]
fn luxury_gate_takes_the_fare() {
    let mut s = Station::start();
    let mob = s.mob();
    assert_eq!(s.pos(), SPAWN);

    // No ticket, no money: the field stops the passenger.
    s.try_step(0, Dir::East);
    s.hold(1.0, "stopped at the ticket field", |s| s.pos() == SPAWN);
    assert!(
        !s.server
            .world()
            .resource::<LuxuryPassengers>()
            .approved
            .contains(&mob)
    );

    // With a 500 cr bill: bumping the field pays it, then the passenger walks through.
    let bill = s.item(BILL, "/obj/item/stack/spacecash/c500").unwrap();
    s.give(bill);
    s.run_until("through the ticket field", |s| {
        if s.pos() == GATE {
            return true;
        }
        s.try_step(0, Dir::East);
        false
    });
    let world = s.server.world();
    assert!(world.resource::<LuxuryPassengers>().approved.contains(&mob));
    assert!(world.get_entity(bill).is_err(), "the bill was taken");
}
