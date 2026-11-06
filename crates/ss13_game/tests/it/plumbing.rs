//! Sinks over real WebTransport on 127.0.0.1: a bottle filled from a sink and emptied down its
//! drain, hands washed (4 s, 5 units of water), a wrench taking the sink apart into its sheet
//! and its water recycler.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::chem::Reagents;
use ss13_game::inventory::{CarriedBy, Inventory};
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::convert::DmTypes;
use ss13_mapload::{DmItem, DmObj};
use ss13_shared::TilePos;
use ss13_shared::protocol::ClickModifiers;
use ss13_world::SpatialIndex;

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/datum/reagent": { "parent": "/datum", "declared_vars": {
        "abstract_type": { "path": "/datum/reagent" }, "name": "", "description": "",
        "color": "#000000", "taste_description": "metaphorical salt", "taste_mult": 1,
        "metabolization_rate": 0.2, "overdose_threshold": 0, "ph": 7, "specific_heat": 200,
        "chemical_flags": 0, "purity": 1, "creation_purity": 1,
        "inverse_chem": { "path": "/datum/reagent/inverse" }, "inverse_chem_val": 0.25,
        "purge_multiplier": 1 } },
    "/datum/reagent/water": { "parent": "/datum/reagent", "declared_vars": {
        "name": "Water", "color": "#AAAAAA77", "chemical_flags": 144 } },
    "/datum/chemical_reaction": { "parent": "/datum" },
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
    "/obj/structure": { "parent": "/obj", "declared_vars": { "anchored": 1 } },
    "/obj/structure/sink": { "parent": "/obj/structure", "declared_vars": {
        "name": "раковина", "icon": { "resource": "w.dmi" }, "icon_state": "sink",
        "capacity": 100, "dispensedreagent": { "path": "/datum/reagent/water" },
        "buildstacktype": { "path": "/obj/item/stack/sheet/iron" }, "buildstackamount": 1,
        "has_water_reclaimer": 1, "reclaim_rate": 0.5 } },
    "/obj/machinery": { "parent": "/obj", "declared_vars": { "anchored": 1 } },
    "/obj/machinery/shower": { "parent": "/obj/machinery", "declared_vars": {
        "name": "душ", "icon": { "resource": "w.dmi" }, "icon_state": "shower",
        "reagent_capacity": 200, "refill_rate": 0.5, "has_water_reclaimer": 1,
        "reagent_id": { "path": "/datum/reagent/water" } } },
    "/obj/structure/sink/still": { "parent": "/obj/structure/sink", "declared_vars": {
        "reclaim_rate": 0 } },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null, "attack_speed": 8,
        "tool_behaviour": null, "toolspeed": 1 } },
    "/obj/item/wrench": { "parent": "/obj/item", "declared_vars": {
        "tool_behaviour": "wrench" } },
    "/obj/item/stock_parts": { "parent": "/obj/item" },
    "/obj/item/stock_parts/water_recycler": { "parent": "/obj/item/stock_parts" },
    "/obj/item/stack": { "parent": "/obj/item", "declared_vars": { "amount": 1, "max_amount": 50 } },
    "/obj/item/stack/sheet": { "parent": "/obj/item/stack" },
    "/obj/item/stack/sheet/iron": { "parent": "/obj/item/stack/sheet" },
    "/obj/item/reagent_containers": { "parent": "/obj/item", "declared_vars": {
        "name": "Container", "icon": { "resource": "c.dmi" },
        "amount_per_transfer_from_this": 5, "has_variable_transfer_amount": 1,
        "possible_transfer_amounts": { "list": [5, 10, 15, 20, 25, 30] }, "volume": 30,
        "initial_reagent_flags": 0, "list_reagents": null, "list_reagents_purity": null,
        "fill_icon_thresholds": null, "fill_icon_state": null,
        "fill_icon": { "resource": "f.dmi" } } },
    "/obj/item/reagent_containers/cup": { "parent": "/obj/item/reagent_containers",
        "declared_vars": { "amount_per_transfer_from_this": 10,
        "possible_transfer_amounts": { "list": [5, 10, 15, 20, 25, 30, 50] }, "volume": 50,
        "initial_reagent_flags": 60, "gulp_size": 5, "icon_state": "bottle" } },
    "/obj/item/reagent_containers/cup/bottle": { "parent": "/obj/item/reagent_containers/cup",
        "declared_vars": { "name": "bottle" } },
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

/// Rows top-down, the last line is y = 1. The spawn `s` (2,2), a sink that does not refill
/// `S` (3,2), a shower `H` (3,3).
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"S" = (/obj/structure/sink/still,/turf/open/floor,/area/space)
"H" = (/obj/machinery/shower,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwww
wfHfw
wsSfw
wwwww
"}
"##;

const SINK: TilePos = TilePos::new(3, 2, 1);
const SHOWER: TilePos = TilePos::new(3, 3, 1);

const STATES: [(&str, &str); 8] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
    ("c.dmi", "bottle"),
    ("w.dmi", "sink"),
    ("w.dmi", "shower"),
];

fn fixture_assets() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("plumbing_fixture");
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/structure/sink/still",
        "/obj/machinery/shower",
        "/obj/item/wrench",
        "/obj/item/stock_parts/water_recycler",
        "/obj/item/stack/sheet/iron",
        "/obj/item/reagent_containers/cup/bottle",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
    ];
    let dir = write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)]);
    let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
    let (chem, _) = ss13_mapload::reagents::chem_data(&types);
    std::fs::write(dir.join(ss13_reagents::CHEM_FILE), chem.to_ron()).unwrap();
    dir
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

    fn sink(&self) -> Option<Entity> {
        let world = self.server.world();
        (world
            .resource::<SpatialIndex>()
            .on_tile(SINK)
            .iter()
            .copied())
        .find(|&e| world.get::<DmObj>(e).is_some())
    }

    fn item_on(&self, path: &str) -> bool {
        let world = self.server.world();
        (world
            .resource::<SpatialIndex>()
            .on_tile(SINK)
            .iter()
            .copied())
        .any(|e| world.get::<DmItem>(e).is_some_and(|i| i.path == path))
    }

    fn volume(&self, e: Entity) -> f32 {
        self.get::<Reagents>(e).0.total_volume
    }

    /// A new `path` item straight into the active hand.
    fn give_new(&mut self, path: &str) -> Entity {
        let mob = self.mob();
        let world = self.server.world_mut();
        let item = ss13_game::construction::spawn_obj(world, path, TilePos::new(4, 3, 1)).unwrap();
        world
            .entity_mut(item)
            .remove::<TilePos>()
            .insert(CarriedBy(mob));
        let mut inventory = world.get_mut::<Inventory>(mob).unwrap();
        let hand = inventory.active;
        inventory.hands[hand] = Some(item);
        item
    }

    fn switch_hands(&mut self) {
        let mob = self.mob();
        let mut inventory = self.server.world_mut().get_mut::<Inventory>(mob).unwrap();
        inventory.active = 1 - inventory.active;
    }
}

#[test]
fn fill_drain_wash_and_take_apart() {
    let mut s = Station::start();
    let sink = s.sink().unwrap();
    s.run_until("full of water", |s| s.volume(sink) == 100.0);

    // A bottle fills from the sink by its transfer amount.
    let bottle = s.give_new("/obj/item/reagent_containers/cup/bottle");
    s.run_until("the bottle has its holder", |s| {
        s.try_get::<Reagents>(bottle).is_some()
    });
    s.click(0, sink);
    s.run_until("filled", |s| s.volume(bottle) == 10.0);
    assert_eq!(s.volume(sink), 90.0);

    // Right click: down the drain.
    s.run_for(1.0);
    s.click_with(
        0,
        sink,
        ClickModifiers {
            right: true,
            ..default()
        },
    );
    s.run_until("emptied", |s| s.volume(bottle) == 0.0);
    assert_eq!(s.volume(sink), 90.0);

    // An empty hand washes: 4 s, 5 units.
    s.switch_hands();
    s.run_for(1.0);
    s.click(0, sink);
    s.run_for(2.0);
    assert_eq!(s.volume(sink), 90.0, "still washing");
    s.run_until("washed", |s| s.volume(sink) == 85.0);

    // A wrench takes it apart: the sheet and the water recycler.
    s.give_new("/obj/item/wrench");
    s.run_for(1.0);
    s.click(0, sink);
    s.run_until("taken apart", |s| s.sink().is_none());
    assert!(s.item_on("/obj/item/stack/sheet/iron"));
    assert!(s.item_on("/obj/item/stock_parts/water_recycler"));
}

/// A shower turned on by hand sprays 5 units a second (`SSplumbing`) and, turned off, leaves
/// its tile wet.
#[test]
fn shower_runs_and_wets_the_floor() {
    let mut s = Station::start();
    let shower = {
        let world = s.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(SHOWER)
            .iter()
            .copied()
            .find(|&e| world.get::<ss13_game::plumbing::Shower>(e).is_some())
            .unwrap()
    };
    s.run_until("full of water", |s| s.volume(shower) == 200.0);
    s.click(0, shower);
    s.run_until("running", |s| {
        s.get::<ss13_game::plumbing::Shower>(shower).actually_on
    });
    s.run_until("spraying", |s| s.volume(shower) <= 190.0);
    s.run_for(1.0);
    s.click(0, shower);
    s.run_until("off", |s| {
        !s.get::<ss13_game::plumbing::Shower>(shower).actually_on
    });
    let world = s.server.world();
    let wet = world
        .resource::<SpatialIndex>()
        .on_tile(SHOWER)
        .iter()
        .any(|&e| world.get::<ss13_game::janitor::WetFloor>(e).is_some());
    assert!(wet, "the floor under the shower is wet");
}
