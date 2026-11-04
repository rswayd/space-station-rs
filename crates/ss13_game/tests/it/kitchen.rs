//! The cook's and the bartender's machines over real WebTransport on 127.0.0.1: dough baked in
//! the oven into bread (tg `bakeable`), a slab grilled on the griddle into a steak
//! (`grillable`), a toy deep-fried into something edible (`fried_item`), cola dispensed from the
//! soda dispenser's `ChemDispenser` window into a drinking glass that takes cola's glass style,
//! and a soda can opened by hand.

use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::chem::{Chem, Reagents};
use ss13_game::chem_dispenser::ChemDispenser;
use ss13_game::smartfridge::SmartFridge;
use ss13_game::combat::NextMove;
use ss13_game::cooking::{CookTimes, DeepFryer, Griddle, Oven};
use ss13_game::inventory::Inventory;
use ss13_game::mecha::core::ItemCell;
use ss13_game::storage::Stored;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::convert::DmTypes;
use ss13_mapload::{DmFood, DmItem, DmName, IconIndex};
use ss13_net::chat::client::ChatReceived;
use ss13_net::ui::client::UiMessageReceived;
use ss13_shared::protocol::{ClickModifiers, InventorySlot};
use ss13_shared::ui::UiServerMessage;
use ss13_shared::{Appearance, TilePos};
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::chem_dispenser::{ChemDispenserData, ChemDispenserUi};
use ss13_tgui::interfaces::smart_vend::SmartVendUi;
use ss13_world::SpatialIndex;

/// tg's vars for what the test uses, trimmed to the ones read.
const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/datum/reagent": { "parent": "/datum", "declared_vars": {
        "abstract_type": { "path": "/datum/reagent" }, "name": "", "description": "",
        "color": "#000000", "taste_description": "metaphorical salt", "taste_mult": 1,
        "metabolization_rate": 0.2, "overdose_threshold": 0, "ph": 7, "specific_heat": 200,
        "chemical_flags": 0, "purity": 1, "creation_purity": 1, "inverse_chem": null,
        "inverse_chem_val": 0.25, "purge_multiplier": 1 } },
    "/datum/reagent/water": { "parent": "/datum/reagent", "declared_vars": {
        "name": "Water", "color": "#AAAAAA77" } },
    "/datum/reagent/consumable": { "parent": "/datum/reagent", "declared_vars": {
        "abstract_type": { "path": "/datum/reagent/consumable" }, "taste_mult": 4,
        "nutriment_factor": 1, "creation_purity": 0.5 } },
    "/datum/reagent/consumable/nutriment": { "parent": "/datum/reagent/consumable",
        "declared_vars": { "name": "Nutriment", "nutriment_factor": 15 } },
    "/datum/reagent/consumable/nutriment/fat": { "parent": "/datum/reagent/consumable/nutriment",
        "declared_vars": { "name": "Fat" } },
    "/datum/reagent/consumable/nutriment/fat/oil": {
        "parent": "/datum/reagent/consumable/nutriment/fat", "declared_vars": {
        "name": "Cooking Oil", "color": "#EADD6B" } },
    "/datum/reagent/consumable/space_cola": { "parent": "/datum/reagent/consumable",
        "declared_vars": { "name": "Cola", "color": "#100800" } },
    "/datum/reagent/consumable/ice": { "parent": "/datum/reagent/consumable",
        "declared_vars": { "name": "Ice", "color": "#619494" } },
    "/datum/glass_style": { "parent": "/datum", "declared_vars": { "desc": null, "icon": null,
        "icon_state": null, "name": null, "required_container_type": null,
        "required_drink_type": null } },
    "/datum/glass_style/drinking_glass": { "parent": "/datum/glass_style", "declared_vars": {
        "required_container_type": { "path": "/obj/item/reagent_containers/cup/glass/drinkingglass" },
        "icon": { "resource": "d.dmi" } } },
    "/datum/glass_style/drinking_glass/space_cola": { "parent": "/datum/glass_style/drinking_glass",
        "declared_vars": { "required_drink_type": { "path": "/datum/reagent/consumable/space_cola" },
        "name": "glass of Space Cola", "desc": "A glass of refreshing Space Cola.",
        "icon_state": "spacecola" } },
    "/datum/stock_part": { "parent": "/datum" },
    "/datum/stock_part/capacitor": { "parent": "/datum/stock_part", "declared_vars": {
        "physical_object_type": { "path": "/obj/item/stock_parts/capacitor" } } },
    "/datum/stock_part/micro_laser": { "parent": "/datum/stock_part", "declared_vars": {
        "physical_object_type": { "path": "/obj/item/stock_parts/micro_laser" } } },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3, "anchored": 0 } },
    "/obj": { "parent": "/atom/movable", "declared_vars": { "force": 0, "damtype": "brute",
        "obj_flags": 2 } },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start" },
    "/obj/machinery": { "parent": "/obj", "declared_vars": { "use_power": 1,
        "idle_power_usage": 100, "active_power_usage": 1000, "power_channel": 1,
        "density": 1, "anchored": 1 } },
    "/obj/machinery/oven": { "parent": "/obj/machinery", "vars": { "name": "oven" },
        "declared_vars": { "name": "oven", "icon": { "resource": "k.dmi" },
        "icon_state": "oven_off", "base_icon_state": "oven",
        "circuit": { "path": "/obj/item/circuitboard/machine/oven" } } },
    "/obj/machinery/griddle": { "parent": "/obj/machinery", "vars": { "name": "griddle" },
        "declared_vars": { "name": "griddle", "icon": { "resource": "k.dmi" },
        "icon_state": "griddle1_off" } },
    "/obj/machinery/deepfryer": { "parent": "/obj/machinery", "vars": { "name": "deep fryer" },
        "declared_vars": { "name": "deep fryer", "icon": { "resource": "k.dmi" },
        "icon_state": "fryer_off",
        "circuit": { "path": "/obj/item/circuitboard/machine/deep_fryer" } } },
    "/obj/machinery/chem_dispenser": { "parent": "/obj/machinery", "declared_vars": {
        "name": "chem dispenser", "icon": { "resource": "c.dmi" }, "icon_state": "dispenser",
        "amount": 30, "power_cost": 100, "recharge_amount": 300, "dispensed_temperature": 300,
        "base_reagent_purity": 1, "dispensable_reagents": { "list": [] },
        "upgrade_reagents": { "list": [] }, "emagged_reagents": { "list": [] },
        "default_dispensable_reagents": { "list": [{ "path": "/datum/reagent/water" }] },
        "default_upgrade_reagents": { "list": [] }, "default_emagged_reagents": { "list": [] },
        "circuit": { "path": "/obj/item/circuitboard/machine/chem_dispenser" } } },
    "/obj/machinery/chem_dispenser/drinks": { "parent": "/obj/machinery/chem_dispenser",
        "vars": { "name": "soda dispenser" }, "declared_vars": {
        "name": "soda dispenser", "icon_state": "soda_dispenser", "amount": 10,
        "dispensed_temperature": 274.5, "base_reagent_purity": 0.5, "upgrade_reagents": null,
        "drinks_dispensable_reagents": { "list": [
            { "path": "/datum/reagent/consumable/space_cola" },
            { "path": "/datum/reagent/consumable/ice" }, { "path": "/datum/reagent/water" }] },
        "drink_emagged_reagents": { "list": [] },
        "circuit": { "path": "/obj/item/circuitboard/machine/chem_dispenser/drinks" } } },
    "/obj/machinery/smartfridge": { "parent": "/obj/machinery", "declared_vars": {
        "name": "smartfridge", "icon": { "resource": "sf.dmi" }, "icon_state": "smartfridge-icon",
        "contents_overlay_icon": "plant", "visible_contents": 1, "has_emissive": 1,
        "max_n_of_items": 1500, "initial_contents": null } },
    "/obj/machinery/smartfridge/food": { "parent": "/obj/machinery/smartfridge",
        "vars": { "name": "food fridge" }, "declared_vars": { "name": "food fridge",
        "contents_overlay_icon": "food",
        "initial_contents": { "list": [{ "assoc": [{ "path": "/obj/item/food/dough" }, 2] }] } } },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null, "w_class": 2,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null, "attack_speed": 8, "tool_behaviour": null,
        "toolspeed": 1 } },
    "/obj/item/toy": { "parent": "/obj/item", "vars": { "name": "toy" }, "declared_vars": {
        "name": "toy" } },
    "/obj/item/plate": { "parent": "/obj/item", "declared_vars": { "icon_state": "plate" } },
    "/obj/item/plate/oven_tray": { "parent": "/obj/item/plate", "vars": { "name": "oven tray" },
        "declared_vars": { "name": "oven tray", "icon_state": "oven_tray" } },
    "/obj/item/stock_parts": { "parent": "/obj/item" },
    "/obj/item/stock_parts/capacitor": { "parent": "/obj/item/stock_parts", "declared_vars": {
        "name": "capacitor", "rating": 1 } },
    "/obj/item/stock_parts/micro_laser": { "parent": "/obj/item/stock_parts", "declared_vars": {
        "name": "micro-laser", "rating": 1 } },
    "/obj/item/stock_parts/power_store": { "parent": "/obj/item/stock_parts", "declared_vars": {
        "maxcharge": 1000000, "chargerate": 1000000 } },
    "/obj/item/stock_parts/power_store/cell": { "parent": "/obj/item/stock_parts/power_store" },
    "/obj/item/stock_parts/power_store/cell/high": { "parent": "/obj/item/stock_parts/power_store/cell",
        "declared_vars": { "maxcharge": 10000000 } },
    "/obj/item/circuitboard": { "parent": "/obj/item", "declared_vars": { "build_path": null } },
    "/obj/item/circuitboard/machine": { "parent": "/obj/item/circuitboard", "declared_vars": {
        "needs_anchored": 1, "req_components": null, "def_components": null } },
    "/obj/item/circuitboard/machine/oven": { "parent": "/obj/item/circuitboard/machine",
        "declared_vars": { "build_path": { "path": "/obj/machinery/oven" } } },
    "/obj/item/circuitboard/machine/deep_fryer": { "parent": "/obj/item/circuitboard/machine",
        "declared_vars": { "build_path": { "path": "/obj/machinery/deepfryer" },
        "req_components": { "list": [
            { "assoc": [{ "path": "/datum/stock_part/micro_laser" }, 1] } ] } } },
    "/obj/item/circuitboard/machine/chem_dispenser": { "parent": "/obj/item/circuitboard/machine",
        "declared_vars": { "build_path": { "path": "/obj/machinery/chem_dispenser" },
        "req_components": { "list": [
            { "assoc": [{ "path": "/datum/stock_part/capacitor" }, 1] },
            { "assoc": [{ "path": "/obj/item/stock_parts/power_store/cell" }, 1] } ] },
        "def_components": { "list": [
            { "assoc": [{ "path": "/obj/item/stock_parts/power_store/cell" },
                { "path": "/obj/item/stock_parts/power_store/cell/high" }] } ] } } },
    "/obj/item/circuitboard/machine/chem_dispenser/drinks": {
        "parent": "/obj/item/circuitboard/machine/chem_dispenser", "declared_vars": {
        "build_path": { "path": "/obj/machinery/chem_dispenser/drinks" } } },
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
    "/obj/item/reagent_containers/cup/glass": { "parent": "/obj/item/reagent_containers/cup",
        "declared_vars": { "icon_state": "glass_empty" } },
    "/obj/item/reagent_containers/cup/glass/drinkingglass": {
        "parent": "/obj/item/reagent_containers/cup/glass", "vars": { "name": "drinking glass" },
        "declared_vars": { "name": "drinking glass", "icon": { "resource": "d.dmi" },
        "icon_state": "glass_empty", "fill_icon_state": "drinking_glass",
        "fill_icon_thresholds": { "list": [0] } } },
    "/obj/item/reagent_containers/cup/soda_cans": { "parent": "/obj/item/reagent_containers/cup",
        "declared_vars": { "abstract_type": { "path": "/obj/item/reagent_containers/cup/soda_cans" },
        "icon": { "resource": "s.dmi" }, "icon_state": "cola", "initial_reagent_flags": 0,
        "volume": 30, "possible_transfer_amounts": { "list": [5, 10, 15, 25, 30] } } },
    "/obj/item/reagent_containers/cup/soda_cans/cola": {
        "parent": "/obj/item/reagent_containers/cup/soda_cans", "vars": { "name": "Space Cola" },
        "declared_vars": { "name": "Space Cola",
        "list_reagents": { "list": [{ "assoc": [{ "path": "/datum/reagent/consumable/space_cola" }, 30] }] } } },
    "/obj/item/trash": { "parent": "/obj/item" },
    "/obj/item/trash/can": { "parent": "/obj/item/trash", "declared_vars": { "name": "crushed can" } },
    "/obj/item/food": { "parent": "/obj/item", "declared_vars": {
        "food_reagents": null, "max_volume": null, "bite_consumption": null,
        "eat_time": null, "tastes": null, "eatverbs": null, "trash_type": null,
        "junkiness": null, "food_flags": null, "starting_reagent_purity": null },
        "procs": ["make_bakeable", "make_grillable"] },
    "/obj/item/food/badrecipe": { "parent": "/obj/item/food", "vars": { "name": "burned mess" },
        "declared_vars": { "name": "burned mess" } },
    "/obj/item/food/dough": { "parent": "/obj/item/food", "vars": { "name": "dough" },
        "declared_vars": { "name": "dough",
        "food_reagents": { "list": [{ "assoc": [{ "path": "/datum/reagent/consumable/nutriment" }, 6] }] } },
        "procs": ["make_bakeable"] },
    "/obj/item/food/bread": { "parent": "/obj/item/food" },
    "/obj/item/food/bread/plain": { "parent": "/obj/item/food/bread", "vars": { "name": "bread" },
        "declared_vars": { "name": "bread" } },
    "/obj/item/food/meat": { "parent": "/obj/item/food" },
    "/obj/item/food/meat/slab": { "parent": "/obj/item/food/meat", "vars": { "name": "meat" },
        "declared_vars": { "name": "meat",
        "food_reagents": { "list": [{ "assoc": [{ "path": "/datum/reagent/consumable/nutriment" }, 4] }] } },
        "procs": ["make_grillable"] },
    "/obj/item/food/meat/steak": { "parent": "/obj/item/food/meat", "vars": { "name": "steak" },
        "declared_vars": { "name": "steak" } },
    "/obj/item/card": { "parent": "/obj/item", "declared_vars": { "inhand_icon_state": "card-id" } },
    "/obj/item/card/id": { "parent": "/obj/item/card", "declared_vars": {
        "access": { "list": [] }, "icon_state": "card_grey", "slot_flags": 256,
        "worn_icon_state": "nothing" } },
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
    "/area": { "parent": "/atom", "declared_vars": { "requires_power": 0 } },
    "/area/space": { "parent": "/area" }
  },
  "element_calls": {
    "/datum/component/bakeable": [
      { "type": "/obj/item/food", "proc": "make_bakeable", "call": "AddComponent",
        "line": "AddComponent(/datum/component/bakeable, /obj/item/food/badrecipe, rand(25 SECONDS, 40 SECONDS), FALSE)" },
      { "type": "/obj/item/food/dough", "proc": "make_bakeable", "call": "AddComponent",
        "line": "AddComponent(/datum/component/bakeable, /obj/item/food/bread/plain, rand(30 SECONDS, 45 SECONDS), TRUE, TRUE)" }
    ],
    "/datum/component/grillable": [
      { "type": "/obj/item/food", "proc": "make_grillable", "call": "AddComponent",
        "line": "AddComponent(/datum/component/grillable, /obj/item/food/badrecipe, rand(20 SECONDS, 30 SECONDS), FALSE)" },
      { "type": "/obj/item/food/meat/slab", "proc": "make_grillable", "call": "AddComponent",
        "line": "AddComponent(/datum/component/grillable, /obj/item/food/meat/steak, rand(30 SECONDS, 90 SECONDS), TRUE, TRUE)" }
    ]
  }
}"##;

/// Rows top-down, the last line is y = 1. The spawn `s` (3,4) with the ingredients, the deep
/// fryer north of it, the oven west, the griddle east, the soda dispenser south and the food
/// fridge south of that.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/obj/item/food/dough,/obj/item/food/meat/slab,/obj/item/toy,/obj/item/reagent_containers/cup/glass/drinkingglass,/obj/item/reagent_containers/cup/soda_cans/cola,/turf/open/floor,/area/space)
"O" = (/obj/machinery/oven,/turf/open/floor,/area/space)
"G" = (/obj/machinery/griddle,/turf/open/floor,/area/space)
"F" = (/obj/machinery/deepfryer,/turf/open/floor,/area/space)
"D" = (/obj/machinery/chem_dispenser/drinks,/turf/open/floor,/area/space)
"R" = (/obj/machinery/smartfridge/food,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwww
wfFfw
wOsGw
wfDfw
wfRfw
wwwww
"}
"##;

const SPAWN: TilePos = TilePos::new(3, 4, 1);
const FRYER: TilePos = TilePos::new(3, 5, 1);
const OVEN: TilePos = TilePos::new(2, 4, 1);
const GRIDDLE: TilePos = TilePos::new(4, 4, 1);
const DISPENSER: TilePos = TilePos::new(3, 3, 1);
/// South of the dispenser, reached from [`BY_FRIDGE`].
const FRIDGE: TilePos = TilePos::new(3, 2, 1);
const BY_FRIDGE: TilePos = TilePos::new(2, 2, 1);

/// Manifest order = `IconStateId`.
const STATES: [(&str, &str); 26] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
    ("i.dmi", "oven_tray"),
    ("k.dmi", "oven_off"),
    ("k.dmi", "oven_on"),
    ("k.dmi", "oven_lid_open"),
    ("k.dmi", "oven_lid_closed"),
    ("k.dmi", "griddle1_on"),
    ("k.dmi", "griddle1_off"),
    ("k.dmi", "griddle2_on"),
    ("k.dmi", "griddle2_off"),
    ("k.dmi", "griddle3_on"),
    ("k.dmi", "griddle3_off"),
    ("k.dmi", "fryer_off"),
    ("k.dmi", "fryer_on"),
    ("c.dmi", "soda_dispenser"),
    ("d.dmi", "glass_empty"),
    ("d.dmi", "spacecola"),
    ("s.dmi", "cola"),
    ("sf.dmi", "smartfridge-icon"),
    ("sf.dmi", "smartfridge"),
    ("sf.dmi", "smartfridge-food-1"),
    ("sf.dmi", "smartfridge-glass"),
];

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("kitchen_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/machinery/oven",
        "/obj/machinery/griddle",
        "/obj/machinery/deepfryer",
        "/obj/machinery/chem_dispenser/drinks",
        "/obj/machinery/smartfridge/food",
        "/obj/item/food/dough",
        "/obj/item/food/meat/slab",
        "/obj/item/toy",
        "/obj/item/reagent_containers/cup/glass/drinkingglass",
        "/obj/item/reagent_containers/cup/soda_cans/cola",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
    ];
    write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)]);
    let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
    let (chem, _) = ss13_mapload::reagents::chem_data(&types);
    std::fs::write(dir.join(ss13_reagents::CHEM_FILE), chem.to_ron()).unwrap();
    let mecha = ss13_mapload::mecha::resolve(&types);
    std::fs::write(dir.join(ss13_mapload::mecha::MECHA_FILE), mecha.to_ron()).unwrap();
    dir
}

#[derive(Resource, Default)]
struct Received {
    chat: Vec<String>,
    windows: HashMap<u32, (String, Vec<u8>)>,
}

fn record(
    mut chat: MessageReader<ChatReceived>,
    mut ui: MessageReader<UiMessageReceived>,
    mut received: ResMut<Received>,
) {
    for ChatReceived(message) in chat.read() {
        received.chat.push(message.plain_text());
    }
    for UiMessageReceived(message) in ui.read() {
        match message {
            UiServerMessage::Open {
                window,
                interface,
                data,
                ..
            } => {
                received
                    .windows
                    .insert(*window, (interface.clone(), data.clone()));
            }
            UiServerMessage::Update {
                window,
                data: Some(data),
                ..
            } => {
                if let Some(w) = received.windows.get_mut(window) {
                    w.1 = data.clone();
                }
            }
            UiServerMessage::Close { window } => {
                received.windows.remove(window);
            }
            _ => {}
        }
    }
}

struct Station(testing::Station);

impl Deref for Station {
    type Target = testing::Station;
    fn deref(&self) -> &testing::Station {
        &self.0
    }
}

impl DerefMut for Station {
    fn deref_mut(&mut self) -> &mut testing::Station {
        &mut self.0
    }
}

impl Drive for Station {
    fn station(&mut self) -> &mut testing::Station {
        &mut self.0
    }

    fn context(&self) -> String {
        let chat: Vec<_> = self
            .players
            .iter()
            .map(|p| &p.app.world().resource::<Received>().chat)
            .collect();
        format!("; chat: {chat:?}")
    }
}

impl Station {
    fn start(test: &str) -> Self {
        let assets = fixture_assets(test);
        let mut s = testing::Station::start(fixture_map(&assets, &["map.dmm"]), |_| {});
        s.join_all(1, |app| {
            app.init_resource::<Received>().add_systems(Update, record);
        });
        Self(s)
    }

    fn click_with(&mut self, entity: Entity, modifiers: ClickModifiers) {
        let mob = self.players[0].mob;
        self.run_until("off cooldown", |s| s.now() >= s.get::<NextMove>(mob).0);
        self.0.click_with(0, entity, modifiers);
    }

    fn click(&mut self, entity: Entity) {
        self.click_with(entity, ClickModifiers::default());
    }

    fn said(&self, text: &str) -> bool {
        self.players[0]
            .app
            .world()
            .resource::<Received>()
            .chat
            .iter()
            .any(|line| line.contains(text))
    }

    fn window<T: UiInterface>(&self) -> Option<(u32, T::Data)> {
        let received = self.players[0].app.world().resource::<Received>();
        received
            .windows
            .iter()
            .filter(|(_, (name, _))| name == T::NAME)
            .max_by_key(|(id, _)| **id)
            .map(|(id, (_, data))| (*id, ss13_tgui::decode(data).unwrap()))
    }

    fn item(&self, pos: TilePos, path: &str) -> Option<Entity> {
        let world = self.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .find(|&e| world.get::<DmItem>(e).is_some_and(|i| i.path == path))
    }

    fn machine<C: Component>(&self, pos: TilePos) -> Entity {
        let world = self.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .find(|&e| world.get::<C>(e).is_some())
            .expect("the machine")
    }

    fn take(&mut self, item: Entity) {
        let mob = self.players[0].mob;
        self.click(item);
        self.run_until("picked up", |s| {
            s.get::<Inventory>(mob).active_item() == Some(item)
        });
    }

    fn held(&self) -> Option<Entity> {
        self.get::<Inventory>(self.players[0].mob).active_item()
    }

    fn amount(&self, container: Entity, path: &str) -> f32 {
        let id = self.server.world().resource::<Chem>().0.id(path).unwrap();
        self.get::<Reagents>(container).0.amount(id)
    }

    fn path(&self, e: Entity) -> String {
        self.get::<DmItem>(e).path.clone()
    }
}

#[test]
fn oven_griddle_and_fryer() {
    let mut s = Station::start("oven_griddle_and_fryer");
    let a = s.players[0].mob;
    assert_eq!(*s.get::<TilePos>(a), SPAWN);

    // The oven starts closed with its tray (map load); a click opens it, the dough goes onto
    // the tray, a click closes it and it bakes: 30-45 s, then bread with the dough's reagents.
    let oven = s.machine::<Oven>(OVEN);
    let tray = s.get::<Oven>(oven).tray.expect("a mapped oven has a tray");
    assert_eq!(s.path(tray), "/obj/item/plate/oven_tray");
    s.click(oven);
    s.run_until("open", |s| s.get::<Oven>(oven).open);
    let dough = s.item(SPAWN, "/obj/item/food/dough").unwrap();
    s.take(dough);
    s.click(oven);
    s.run_until("on the tray", |s| {
        s.try_get::<Stored>(tray)
            .is_some_and(|t| t.items().contains(&dough))
    });
    s.click(oven);
    s.run_until("closed", |s| !s.get::<Oven>(oven).open);
    let world = s.server.world();
    let icons = world.resource::<IconIndex>();
    assert_eq!(
        Some(world.get::<Appearance>(oven).unwrap().icon_state),
        icons.get("k.dmi", "oven_on"),
        "baking"
    );
    s.run_until_for(60.0, "bread on the tray", |s| {
        s.try_get::<Stored>(tray).is_some_and(|t| {
            t.items()
                .iter()
                .any(|&i| s.path(i) == "/obj/item/food/bread/plain")
        })
    });
    assert!(s.server.world().get_entity(dough).is_err(), "the dough is gone");
    let bread = s.get::<Stored>(tray).items()[0];
    assert_eq!(
        s.amount(bread, "/datum/reagent/consumable/nutriment"),
        6.0,
        "the dough's reagents"
    );
    s.run_until("it smells nice", |s| s.said("источает приятный запах"));
    // Open, the tray out by a right click.
    s.click(oven);
    s.run_until("open again", |s| s.get::<Oven>(oven).open);
    s.click_with(
        oven,
        ClickModifiers {
            right: true,
            ..default()
        },
    );
    s.run_until("the tray in hand", |s| s.held() == Some(tray));
    assert_eq!(s.get::<Oven>(oven).tray, None);
    s.send(0, ss13_shared::protocol::ClientIntent::DropActive);
    s.run_until("dropped", |s| s.held().is_none());

    // The slab on the griddle, the griddle on: 30-90 s into a steak on the griddle.
    let griddle = s.machine::<Griddle>(GRIDDLE);
    let slab = s.item(SPAWN, "/obj/item/food/meat/slab").unwrap();
    s.take(slab);
    s.click_with(
        griddle,
        ClickModifiers {
            pixel: Some([4, -3]),
            ..default()
        },
    );
    s.run_until("on the griddle", |s| {
        s.get::<Griddle>(griddle).items.contains(&slab)
    });
    assert_eq!(*s.get::<TilePos>(slab), GRIDDLE);
    assert_eq!(s.get::<Appearance>(slab).pixel_offset, [4, -3]);
    s.click(griddle);
    s.run_until("on", |s| s.get::<Griddle>(griddle).on);
    s.run_until_for(10.0, "grilling", |s| {
        s.try_get::<CookTimes>(slab).is_some_and(|t| t.grilled > 0.0)
    });
    s.run_until_for(100.0, "a steak", |s| {
        s.item(GRIDDLE, "/obj/item/food/meat/steak").is_some()
    });
    let steak = s.item(GRIDDLE, "/obj/item/food/meat/steak").unwrap();
    assert_eq!(s.get::<Griddle>(griddle).items, [steak], "the result stays on it");
    assert_eq!(s.get::<Appearance>(steak).pixel_offset, [4, -3]);
    assert_eq!(s.amount(steak, "/datum/reagent/consumable/nutriment"), 4.0);
    // Picked up, it is off the griddle.
    s.take(steak);
    s.run_until_for(5.0, "off the griddle", |s| {
        s.get::<Griddle>(griddle).items.is_empty()
    });
    s.send(0, ss13_shared::protocol::ClientIntent::DropActive);
    s.run_until("dropped", |s| s.held().is_none());

    // The toy in the fryer for ~20 s at fry speed 1: fried (15-50 s), edible, oily.
    let fryer = s.machine::<DeepFryer>(FRYER);
    let oil = s.amount(fryer, "/datum/reagent/consumable/nutriment/fat/oil");
    assert_eq!(oil, 25.0, "25 units of cooking oil to start");
    let toy = s.item(SPAWN, "/obj/item/toy").unwrap();
    s.take(toy);
    s.click(fryer);
    s.run_until("frying", |s| s.get::<DeepFryer>(fryer).frying == Some(toy));
    s.run_until_for(30.0, "fried", |s| {
        s.get::<DeepFryer>(fryer).cook_time >= 200.0
    });
    s.click(fryer);
    s.run_until("out, in hand", |s| s.held() == Some(toy));
    assert_eq!(s.get::<DeepFryer>(fryer).frying, None);
    assert_eq!(s.get::<DmName>(toy).name, "fried toy");
    assert!(
        s.try_get::<DmFood>(toy).is_some(),
        "anything fried can be eaten"
    );
    // `oil_use * 2 s` three times over per machine tick: 0.15 units a tick.
    let soaked = s.amount(toy, "/datum/reagent/consumable/nutriment/fat/oil");
    assert!(soaked > 0.5, "{soaked}");
    assert!(s.amount(fryer, "/datum/reagent/consumable/nutriment/fat/oil") < oil);
}

#[test]
fn soda_dispenser_glass_style_and_cans() {
    let mut s = Station::start("soda_dispenser_glass_style_and_cans");
    let dispenser = s.machine::<ChemDispenser>(DISPENSER);
    let d = s.get::<ChemDispenser>(dispenser).clone();
    let cell = d.cell.expect("the board's high-capacity cell part");
    let full = s.get::<ItemCell>(cell).0.charge;
    assert_eq!(full, 10_000_000.0);
    assert_eq!((d.amount, d.power_cost, d.recharge_amount), (10.0, 100.0, 300.0));

    // The glass in hand goes in and the window opens: cola, ice and water sorted by name.
    let glass = s.item(SPAWN, "/obj/item/reagent_containers/cup/glass/drinkingglass").unwrap();
    s.take(glass);
    s.click(dispenser);
    s.run_until("the window", |s| s.window::<ChemDispenserUi>().is_some());
    s.run_until("the glass is in", |s| {
        s.window::<ChemDispenserUi>()
            .is_some_and(|(_, d)| d.beaker.is_some())
    });
    let (window, data): (u32, ChemDispenserData) = s.window::<ChemDispenserUi>().unwrap();
    let titles: Vec<&str> = data.chemicals.iter().map(|c| c.title.as_str()).collect();
    assert_eq!(titles, ["Cola", "Ice", "Water"]);
    assert_eq!(data.displayed_max_units, 100_000.0);
    // Not a transfer amount of the glass: refused; 15 is.
    s.act(0, window, "amount", &[("target", "7")]);
    s.act(0, window, "amount", &[("target", "15")]);
    s.run_until("15 units at a time", |s| {
        s.get::<ChemDispenser>(dispenser).amount == 15.0
    });
    // A reagent it does not dispense is refused.
    s.act(0, window, "dispense", &[("reagent", "Cooking Oil")]);
    s.act(0, window, "dispense", &[("reagent", "Cola")]);
    s.run_until("cola in the glass", |s| {
        s.amount(glass, "/datum/reagent/consumable/space_cola") == 15.0
    });
    assert_eq!(
        s.amount(glass, "/datum/reagent/consumable/nutriment/fat/oil"),
        0.0
    );
    assert_eq!(s.get::<ItemCell>(cell).0.charge, full - 1500.0);
    // Cola is the main reagent: the glass takes its style's name and sprite.
    s.run_until_for(1.0, "the glass is styled", |s| {
        s.get::<DmName>(glass).name == "glass of Space Cola"
    });
    let world = s.server.world();
    assert_eq!(
        Some(world.get::<Appearance>(glass).unwrap().icon_state),
        world.resource::<IconIndex>().get("d.dmi", "spacecola")
    );
    // Remove 5, then eject: the glass in hand keeps its look.
    s.act(0, window, "remove", &[("amount", "5")]);
    s.run_until("5 removed", |s| {
        s.amount(glass, "/datum/reagent/consumable/space_cola") == 10.0
    });
    s.act(0, window, "eject", &[]);
    s.run_until("the glass back in hand", |s| s.held() == Some(glass));
    // Charged back over machine ticks.
    s.run_until_for(10.0, "recharging", |s| {
        s.get::<ItemCell>(cell).0.charge > full - 1500.0
    });

    // A closed soda can: a click on it in hand opens it (no fizz, no burst).
    s.send(0, ss13_shared::protocol::ClientIntent::DropActive);
    s.run_until("dropped", |s| s.held().is_none());
    let can = s
        .item(SPAWN, "/obj/item/reagent_containers/cup/soda_cans/cola")
        .unwrap();
    assert!(!s.get::<Reagents>(can).0.is_drainable(), "closed");
    s.take(can);
    let slot = if s.get::<Inventory>(s.players[0].mob).active == 0 {
        InventorySlot::LeftHand
    } else {
        InventorySlot::RightHand
    };
    s.send(0, ss13_shared::protocol::ClientIntent::ClickSlot(slot));
    s.run_until("opened", |s| s.get::<Reagents>(can).0.is_drainable());
    assert_eq!(s.amount(can, "/datum/reagent/consumable/space_cola"), 30.0);
    s.run_until("the pop", |s| s.said("приятным хлопком"));
}

#[test]
fn smart_fridge() {
    let mut s = Station::start("smart_fridge");
    let a = s.players[0].mob;
    let toy = s.item(SPAWN, "/obj/item/toy").unwrap();
    s.take(toy);
    s.server.world_mut().entity_mut(a).insert(BY_FRIDGE);
    s.run_until("moved", |s| *s.get::<TilePos>(a) == BY_FRIDGE);
    s.send(0, ss13_shared::protocol::ClientIntent::DropActive);
    s.run_until("the toy dropped", |s| s.held().is_none());
    let fridge = s.machine::<SmartFridge>(FRIDGE);
    // `initial_contents`: two dough; the sprite shows it holds food.
    assert_eq!(s.get::<Stored>(fridge).items().len(), 2);
    let world = s.server.world();
    let food_overlay = world
        .resource::<IconIndex>()
        .get("sf.dmi", "smartfridge-food-1")
        .unwrap();
    assert!(
        world
            .get::<Appearance>(fridge)
            .unwrap()
            .overlays
            .iter()
            .any(|o| o.icon_state == food_overlay)
    );
    // The window groups them; releasing one puts it in the hand.
    s.click(fridge);
    s.run_until("the window", |s| s.window::<SmartVendUi>().is_some());
    let (window, data) = s.window::<SmartVendUi>().unwrap();
    assert_eq!(data.contents.len(), 1);
    assert_eq!(data.contents[0].amount, 2);
    assert_eq!(data.contents[0].name, "Dough");
    let key = data.contents[0].path.clone();
    s.act(0, window, "Release", &[("path", &key), ("amount", "1")]);
    s.run_until("a dough in hand", |s| {
        s.held()
            .is_some_and(|h| s.path(h) == "/obj/item/food/dough")
    });
    assert_eq!(s.get::<Stored>(fridge).items().len(), 1);
    // Back in; a toy is refused.
    s.click(fridge);
    s.run_until("loaded back", |s| s.get::<Stored>(fridge).items().len() == 2);
    s.take(toy);
    s.click(fridge);
    s.run_until("refused", |s| s.said("отказывается принимать"));
    assert_eq!(s.held(), Some(toy));
    assert_eq!(s.get::<Stored>(fridge).items().len(), 2);
}
