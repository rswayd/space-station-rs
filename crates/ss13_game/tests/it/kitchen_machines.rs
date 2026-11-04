//! The second batch of kitchen machines over real WebTransport on 127.0.0.1: a human stuffed
//! into the gibber and gibbed into meat and skin (tg `gibber.dm`), coffee brewed from a
//! cartridge (`coffeemaker.dm`), a waffle cone made and filled at the ice cream vat
//! (`icecream_vat.dm`, `ice_cream_holder`), a meatball soup cooked in the range's pot
//! (`stove_component.dm`, `soup_mixtures.dm`), a toy burnt to ash on the griddle and a thrown
//! glass smashed (`drinks.dm`).

use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::chem::{Chem, Reagents};
use ss13_game::closets::InCloset;
use ss13_game::coffee::{Cartridge, CoffeeMaker};
use ss13_game::combat::NextMove;
use ss13_game::cooking::Griddle;
use ss13_game::gibber::Gibber;
use ss13_game::icecream::{IceCreamVat, Scoops};
use ss13_game::inventory::Inventory;
use ss13_game::stove::{SoupPot, Stove};
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::convert::DmTypes;
use ss13_mapload::{DmItem, DmName};
use ss13_net::chat::client::ChatReceived;
use ss13_net::ui::client::UiMessageReceived;
use ss13_shared::TilePos;
use ss13_shared::protocol::ClickModifiers;
use ss13_shared::ui::UiServerMessage;
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::list_input::{ListInputData, ListInputUi};
use ss13_world::SpatialIndex;

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
    "/datum/reagent/consumable/nutriment/vitamin": {
        "parent": "/datum/reagent/consumable/nutriment", "declared_vars": { "name": "Vitamin" } },
    "/datum/reagent/consumable/nutriment/fat": { "parent": "/datum/reagent/consumable/nutriment",
        "declared_vars": { "name": "Fat", "nutriment_factor": 18 } },
    "/datum/reagent/consumable/nutriment/soup": { "parent": "/datum/reagent/consumable/nutriment",
        "declared_vars": { "abstract_type": { "path": "/datum/reagent/consumable/nutriment/soup" },
        "name": "Soup" } },
    "/datum/reagent/consumable/nutriment/soup/meatball_soup": {
        "parent": "/datum/reagent/consumable/nutriment/soup", "declared_vars": {
        "name": "Meatball Soup", "color": "#FFFDCF" } },
    "/datum/reagent/consumable/coffee": { "parent": "/datum/reagent/consumable",
        "declared_vars": { "name": "Coffee", "color": "#482000" } },
    "/datum/reagent/consumable/milk": { "parent": "/datum/reagent/consumable",
        "declared_vars": { "name": "Milk", "color": "#DFDFDF" } },
    "/datum/reagent/consumable/ice": { "parent": "/datum/reagent/consumable",
        "declared_vars": { "name": "Ice", "color": "#619494" } },
    "/datum/reagent/consumable/vanilla": { "parent": "/datum/reagent/consumable",
        "declared_vars": { "name": "Vanilla Powder", "color": "#FFFACD" } },
    "/datum/reagent/consumable/sugar": { "parent": "/datum/reagent/consumable",
        "declared_vars": { "name": "Sugar", "color": "#FFFFFF" } },
    "/datum/reagent/consumable/flour": { "parent": "/datum/reagent/consumable",
        "declared_vars": { "name": "Flour", "color": "#FFFFFF" } },
    "/datum/chemical_reaction": { "parent": "/datum", "declared_vars": {
        "results": { "list": [] }, "required_reagents": { "list": [] },
        "required_catalysts": { "list": [] }, "required_container_accepts_subtypes": 0,
        "required_container": null, "mob_react": 1, "mix_message": "The solution begins to bubble.",
        "is_cold_recipe": 0, "required_temp": 100, "optimal_temp": 500, "overheat_temp": 900,
        "optimal_ph_min": 5, "optimal_ph_max": 9, "determin_ph_range": 4,
        "temp_exponent_factor": 2, "ph_exponent_factor": 2, "thermic_constant": 0,
        "H_ion_release": 0.01, "rate_up_lim": 30, "purity_min": 0.15, "reaction_flags": 0 } },
    "/datum/chemical_reaction/food": { "parent": "/datum/chemical_reaction",
        "procs": ["pre_reaction_other_checks"] },
    "/datum/chemical_reaction/food/soup": { "parent": "/datum/chemical_reaction/food",
        "declared_vars": { "required_temp": 450, "optimal_temp": 480, "overheat_temp": 540,
        "optimal_ph_min": 1, "optimal_ph_max": 14, "thermic_constant": 10,
        "required_reagents": null, "mob_react": 0, "required_container_accepts_subtypes": 1,
        "required_container": { "path": "/obj/item/reagent_containers/cup/soup_pot" },
        "mix_message": "You smell something good coming from the steaming pot of soup.",
        "required_ingredients": null, "ingredient_reagent_multiplier": 0.8,
        "percentage_of_nutriment_converted": 0.25, "resulting_food_path": null },
        "procs": ["pre_reaction_other_checks", "on_reaction", "reaction_step", "reaction_finish"] },
    "/datum/chemical_reaction/food/soup/meatballsoup": {
        "parent": "/datum/chemical_reaction/food/soup", "declared_vars": {
        "required_reagents": { "list": [{ "assoc": [{ "path": "/datum/reagent/water" }, 50] }] },
        "required_ingredients": { "list": [{ "assoc": [{ "path": "/obj/item/food/meatball" }, 1] }] },
        "results": { "list": [
            { "assoc": [{ "path": "/datum/reagent/consumable/nutriment/soup/meatball_soup" }, 30] },
            { "assoc": [{ "path": "/datum/reagent/water" }, 9] }] } } },
    "/datum/stock_part": { "parent": "/datum" },
    "/datum/stock_part/matter_bin": { "parent": "/datum/stock_part", "declared_vars": {
        "physical_object_type": { "path": "/obj/item/stock_parts/matter_bin" } } },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3, "anchored": 0 } },
    "/obj": { "parent": "/atom/movable", "declared_vars": { "force": 0, "damtype": "brute",
        "obj_flags": 2, "resistance_flags": 0, "max_integrity": 500 } },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start" },
    "/obj/effect/decal": { "parent": "/obj/effect" },
    "/obj/effect/decal/cleanable": { "parent": "/obj/effect/decal" },
    "/obj/effect/decal/cleanable/ash": { "parent": "/obj/effect/decal/cleanable",
        "vars": { "name": "ashes" }, "declared_vars": { "name": "ashes",
        "icon": { "resource": "t.dmi" }, "icon_state": "floor" } },
    "/obj/machinery": { "parent": "/obj", "declared_vars": { "use_power": 1,
        "idle_power_usage": 100, "active_power_usage": 1000, "power_channel": 1,
        "density": 1, "anchored": 1 } },
    "/obj/machinery/gibber": { "parent": "/obj/machinery", "vars": { "name": "gibber" },
        "declared_vars": { "name": "gibber", "icon": { "resource": "k.dmi" },
        "icon_state": "grinder", "circuit": { "path": "/obj/item/circuitboard/machine/gibber" } } },
    "/obj/machinery/coffeemaker": { "parent": "/obj/machinery", "vars": { "name": "coffeemaker" },
        "declared_vars": { "name": "coffeemaker", "icon": { "resource": "cm.dmi" },
        "icon_state": "coffeemaker_nopot_nocart" } },
    "/obj/machinery/icecream_vat": { "parent": "/obj/machinery",
        "vars": { "name": "ice cream vat" }, "declared_vars": { "name": "ice cream vat",
        "icon": { "resource": "kt.dmi" }, "icon_state": "icecream_vat", "use_power": 0,
        "anchored": 0 } },
    "/obj/machinery/oven": { "parent": "/obj/machinery", "vars": { "name": "oven" },
        "declared_vars": { "name": "oven", "icon": { "resource": "k.dmi" },
        "icon_state": "oven_off", "base_icon_state": "oven" } },
    "/obj/machinery/oven/range": { "parent": "/obj/machinery/oven", "vars": { "name": "range" },
        "declared_vars": { "name": "range", "icon_state": "range_off",
        "base_icon_state": "range" } },
    "/obj/machinery/barsign": { "parent": "/obj/machinery", "vars": { "name": "bar sign" },
        "declared_vars": { "name": "bar sign", "icon": { "resource": "bs.dmi" },
        "icon_state": "empty", "req_access": { "list": ["bar"] } } },
    "/datum/barsign": { "parent": "/datum", "declared_vars": { "name": null,
        "icon_state": null, "desc": null, "hidden": 0, "ss220_icon": null } },
    "/datum/barsign/hiddensigns": { "parent": "/datum/barsign", "declared_vars": { "hidden": 1 } },
    "/datum/barsign/hiddensigns/signoff": { "parent": "/datum/barsign/hiddensigns",
        "declared_vars": { "name": "Off", "icon_state": "empty",
        "desc": "This sign doesn't seem to be on." } },
    "/datum/barsign/thebark": { "parent": "/datum/barsign", "declared_vars": {
        "name": "The Bark", "icon_state": "thebark", "desc": "Ian's bar of choice." } },
    "/obj/machinery/smartfridge": { "parent": "/obj/machinery", "declared_vars": {
        "name": "smartfridge", "icon": { "resource": "sf.dmi" }, "icon_state": "smartfridge-icon",
        "contents_overlay_icon": "plant", "visible_contents": 1, "has_emissive": 1,
        "max_n_of_items": 1500, "initial_contents": null } },
    "/obj/machinery/smartfridge/drying": { "parent": "/obj/machinery/smartfridge",
        "vars": { "name": "dehydrator" }, "declared_vars": { "name": "dehydrator",
        "icon_state": "dehydrator-icon", "contents_overlay_icon": "contents",
        "has_emissive": 0, "max_n_of_items": 25 } },
    "/obj/machinery/smartfridge/drying/rack": { "parent": "/obj/machinery/smartfridge/drying",
        "vars": { "name": "drying rack" }, "declared_vars": { "name": "drying rack",
        "icon_state": "drying-rack", "visible_contents": 0, "use_power": 0 } },
    "/obj/item/food/sosjerky": { "parent": "/obj/item/food", "vars": { "name": "jerky" },
        "declared_vars": { "name": "jerky" } },
    "/obj/item/food/sosjerky/healthy": { "parent": "/obj/item/food/sosjerky" },
    "/obj/machinery/griddle": { "parent": "/obj/machinery", "vars": { "name": "griddle" },
        "declared_vars": { "name": "griddle", "icon": { "resource": "k.dmi" },
        "icon_state": "griddle1_off" } },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null, "w_class": 2,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null, "attack_speed": 8, "tool_behaviour": null,
        "toolspeed": 1, "max_integrity": 200 } },
    "/obj/item/toy": { "parent": "/obj/item", "vars": { "name": "toy" }, "declared_vars": {
        "name": "toy", "resistance_flags": 4 } },
    "/obj/item/plate": { "parent": "/obj/item", "declared_vars": { "icon_state": "plate" } },
    "/obj/item/plate/oven_tray": { "parent": "/obj/item/plate", "vars": { "name": "oven tray" },
        "declared_vars": { "name": "oven tray", "icon_state": "oven_tray" } },
    "/obj/item/coffee_cartridge": { "parent": "/obj/item", "vars": {
        "name": "coffeemaker cartridge- Caffè Generico" }, "declared_vars": {
        "name": "coffeemaker cartridge- Caffè Generico", "icon_state": "cartridge_basic" } },
    "/obj/item/stock_parts": { "parent": "/obj/item" },
    "/obj/item/stock_parts/matter_bin": { "parent": "/obj/item/stock_parts", "declared_vars": {
        "name": "matter bin", "rating": 1 } },
    "/obj/item/circuitboard": { "parent": "/obj/item", "declared_vars": { "build_path": null } },
    "/obj/item/circuitboard/machine": { "parent": "/obj/item/circuitboard", "declared_vars": {
        "needs_anchored": 1, "req_components": null, "def_components": null } },
    "/obj/item/circuitboard/machine/gibber": { "parent": "/obj/item/circuitboard/machine",
        "declared_vars": { "build_path": { "path": "/obj/machinery/gibber" },
        "req_components": { "list": [
            { "assoc": [{ "path": "/datum/stock_part/matter_bin" }, 1] } ] } } },
    "/obj/item/stack": { "parent": "/obj/item", "declared_vars": { "amount": 1,
        "max_amount": 50, "singular_name": null, "merge_type": null } },
    "/obj/item/stack/sheet": { "parent": "/obj/item/stack" },
    "/obj/item/stack/sheet/animalhide": { "parent": "/obj/item/stack/sheet" },
    "/obj/item/stack/sheet/animalhide/carbon": { "parent": "/obj/item/stack/sheet/animalhide" },
    "/obj/item/stack/sheet/animalhide/carbon/human": {
        "parent": "/obj/item/stack/sheet/animalhide/carbon", "vars": { "name": "human skin" },
        "declared_vars": { "name": "human skin" } },
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
        "initial_reagent_flags": 60, "gulp_size": 5, "icon_state": "bottle", "isGlass": 0 } },
    "/obj/item/reagent_containers/cup/glass": { "parent": "/obj/item/reagent_containers/cup",
        "declared_vars": { "isGlass": 1 } },
    "/obj/item/reagent_containers/cup/glass/drinkingglass": {
        "parent": "/obj/item/reagent_containers/cup/glass", "vars": { "name": "drinking glass" },
        "declared_vars": { "name": "drinking glass" } },
    "/obj/item/broken_bottle": { "parent": "/obj/item", "vars": { "name": "broken bottle" },
        "declared_vars": { "name": "broken bottle" } },
    "/obj/item/reagent_containers/cup/coffeepot": { "parent": "/obj/item/reagent_containers/cup",
        "vars": { "name": "coffeepot" }, "declared_vars": { "name": "coffeepot",
        "volume": 120 } },
    "/obj/item/reagent_containers/cup/soup_pot": { "parent": "/obj/item/reagent_containers/cup",
        "vars": { "name": "soup pot" }, "declared_vars": { "name": "soup pot",
        "icon": { "resource": "kt.dmi" }, "icon_state": "pot", "volume": 200, "w_class": 5,
        "initial_reagent_flags": 12 } },
    "/obj/item/food": { "parent": "/obj/item", "declared_vars": {
        "food_reagents": null, "max_volume": null, "bite_consumption": null,
        "eat_time": null, "tastes": null, "eatverbs": null, "trash_type": null,
        "junkiness": null, "food_flags": null, "starting_reagent_purity": null,
        "resistance_flags": 4, "foodtypes": null, "crafting_complexity": 0,
        "preserved_food": 0, "decomp_type": { "path": "/obj/item/food/badrecipe/moldy" },
        "decomp_req_handle": 0, "decomposition_time": 0, "ant_attracting": 0 },
        "procs": ["make_germ_sensitive", "make_dryable"] },
    "/obj/item/food/meatball": { "parent": "/obj/item/food", "vars": { "name": "meatball" },
        "declared_vars": { "name": "meatball",
        "food_reagents": { "list": [{ "assoc": [{ "path": "/datum/reagent/consumable/nutriment" }, 4] }] } } },
    "/obj/item/food/meat": { "parent": "/obj/item/food" },
    "/obj/item/food/meat/slab": { "parent": "/obj/item/food/meat", "vars": { "name": "meat" },
        "procs": ["make_dryable"],
        "declared_vars": { "name": "meat", "foodtypes": 5, "decomposition_time": 50,
        "food_reagents": { "list": [{ "assoc": [{ "path": "/datum/reagent/consumable/nutriment" }, 4] }] } } },
    "/obj/item/food/badrecipe": { "parent": "/obj/item/food", "vars": { "name": "burned mess" },
        "declared_vars": { "name": "burned mess" } },
    "/obj/item/food/badrecipe/moldy": { "parent": "/obj/item/food/badrecipe",
        "vars": { "name": "moldy mess" }, "declared_vars": { "name": "moldy mess",
        "ant_attracting": 1, "decomp_type": null, "decomposition_time": 30 } },
    "/obj/effect/decal/cleanable/ants": { "parent": "/obj/effect/decal/cleanable",
        "vars": { "name": "space ants" }, "declared_vars": { "name": "space ants" } },
    "/obj/item/food/meat/slab/human": { "parent": "/obj/item/food/meat/slab",
        "vars": { "name": "meat" } },
    "/obj/item/food/icecream": { "parent": "/obj/item/food", "vars": { "name": "waffle cone" },
        "declared_vars": { "name": "waffle cone", "icon": { "resource": "kt.dmi" },
        "icon_state": "icecream_cone_waffle", "max_volume": 10,
        "food_reagents": { "list": [{ "assoc": [{ "path": "/datum/reagent/consumable/nutriment" }, 5] }] } } },
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
    "/datum/element/dryable": [
      { "type": "/obj/item/food/meat/slab", "proc": "make_dryable", "call": "AddElement",
        "line": "AddElement(/datum/element/dryable,  /obj/item/food/sosjerky/healthy)" }
    ]
  }
}"##;

/// The spawn `s` (3,4) with the ingredients; the gibber east, the coffeemaker west, the vat
/// north, the range south-west and the griddle south-east (reached past the free tile south).
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/obj/item/food/meatball,/obj/item/toy,/obj/item/reagent_containers/cup/glass/drinkingglass,/turf/open/floor,/area/space)
"B" = (/obj/machinery/gibber,/turf/open/floor,/area/space)
"C" = (/obj/machinery/coffeemaker,/turf/open/floor,/area/space)
"V" = (/obj/machinery/icecream_vat,/turf/open/floor,/area/space)
"R" = (/obj/machinery/oven/range,/turf/open/floor,/area/space)
"G" = (/obj/machinery/griddle,/turf/open/floor,/area/space)
"m" = (/obj/item/food/meat/slab,/turf/open/floor,/area/space)
"Z" = (/obj/machinery/barsign,/turf/open/floor,/area/space)
"D" = (/obj/machinery/smartfridge/drying/rack,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwww
wmVfZw
wCsBfw
wRfGDw
wwwwww
"}
"##;

const SPAWN: TilePos = TilePos::new(3, 3, 1);
const GIBBER: TilePos = TilePos::new(4, 3, 1);
const COFFEE: TilePos = TilePos::new(2, 3, 1);
const VAT: TilePos = TilePos::new(3, 4, 1);
const RANGE: TilePos = TilePos::new(2, 2, 1);
const GRIDDLE: TilePos = TilePos::new(4, 2, 1);

const STATES: [(&str, &str); 29] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
    ("i.dmi", "oven_tray"),
    ("i.dmi", "cartridge_basic"),
    ("c.dmi", "bottle"),
    ("k.dmi", "grinder"),
    ("k.dmi", "grinder_empty"),
    ("k.dmi", "grinder_loaded"),
    ("k.dmi", "grinder_active"),
    ("k.dmi", "range_off"),
    ("k.dmi", "range_on"),
    ("k.dmi", "range_on_flame"),
    ("k.dmi", "griddle1_off"),
    ("k.dmi", "griddle1_on"),
    ("k.dmi", "griddle2_off"),
    ("k.dmi", "griddle2_on"),
    ("k.dmi", "griddle3_off"),
    ("k.dmi", "griddle3_on"),
    ("cm.dmi", "coffeemaker_nopot_nocart"),
    ("kt.dmi", "icecream_vat"),
    ("kt.dmi", "icecream_cone_waffle"),
    ("kt.dmi", "icecream_custom"),
    ("kt.dmi", "pot"),
    ("bs.dmi", "empty"),
    ("bs.dmi", "thebark"),
    ("sf.dmi", "drying-rack"),
];

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("kitchen_machines_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/machinery/gibber",
        "/obj/machinery/coffeemaker",
        "/obj/machinery/icecream_vat",
        "/obj/machinery/oven/range",
        "/obj/machinery/griddle",
        "/obj/machinery/barsign",
        "/obj/machinery/smartfridge/drying/rack",
        "/obj/item/food/meatball",
        "/obj/item/food/meat/slab",
        "/obj/item/toy",
        "/obj/item/reagent_containers/cup/glass/drinkingglass",
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

/// The newest `SmartVend` window.
fn fridge_window(s: &Station) -> Option<u32> {
    s.players[0]
        .app
        .world()
        .resource::<Received>()
        .windows
        .iter()
        .filter(|(_, (name, _))| name == "SmartVend")
        .map(|(id, _)| *id)
        .max()
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

const RIGHT: ClickModifiers = ClickModifiers {
    shift: false,
    ctrl: false,
    alt: false,
    middle: false,
    right: true,
    pixel: None,
};

impl Station {
    fn start(test: &str, players: usize) -> Self {
        let assets = fixture_assets(test);
        let mut s = testing::Station::start(fixture_map(&assets, &["map.dmm"]), |_| {});
        s.join_all(players, |app| {
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

    /// The newest list prompt: its window and items.
    fn list(&self) -> Option<(u32, Vec<String>)> {
        let received = self.players[0].app.world().resource::<Received>();
        received
            .windows
            .iter()
            .filter(|(_, (name, _))| name == ListInputUi::NAME)
            .max_by_key(|(id, _)| **id)
            .map(|(id, (_, data))| {
                let data: ListInputData = ss13_tgui::decode(data).unwrap();
                (*id, data.items)
            })
    }

    /// Opens `target`'s menu with an empty hand and picks `entry`.
    fn pick(&mut self, target: Entity, entry: &str) {
        let before = self.list().map(|(id, _)| id);
        self.click(target);
        self.run_until("the menu", |s| {
            s.list().is_some_and(|(id, _)| Some(id) != before)
        });
        let (window, items) = self.list().unwrap();
        assert!(items.iter().any(|i| i == entry), "{entry} in {items:?}");
        self.act(0, window, "submit", &[("entry", entry)]);
    }

    fn on_tile(&self, pos: TilePos, path: &str) -> Option<Entity> {
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

    fn drop_held(&mut self) {
        self.send(0, ss13_shared::protocol::ClientIntent::DropActive);
        self.run_until("dropped", |s| s.held().is_none());
    }

    fn amount(&self, container: Entity, path: &str) -> f32 {
        let id = self.server.world().resource::<Chem>().0.id(path).unwrap();
        self.get::<Reagents>(container).0.amount(id)
    }

    fn count(&mut self, path: &str) -> usize {
        let world = self.0.server.world_mut();
        let mut q = world.query::<&DmItem>();
        q.iter(world).filter(|i| i.path == path).count()
    }
}

#[test]
fn gibber_coffee_and_ice_cream() {
    let mut s = Station::start("gibber_coffee_and_ice_cream", 2);
    let (a, b) = (s.players[0].mob, s.players[1].mob);

    // B, dressed, pulled to the gibber: "no abiotic items". Undressed, stuffed in after
    // `gibtime` (4 s), then gibbed into human meat and skin, thrown out 4 s later.
    let gibber = s.machine::<Gibber>(GIBBER);
    s.server
        .world_mut()
        .entity_mut(a)
        .insert(ss13_game::pull::Pulling(b));
    s.click(gibber);
    s.run_until("refused", |s| s.said("абиотических"));
    {
        let world = s.0.server.world_mut();
        let mut inventory = world.get::<Inventory>(b).unwrap().clone();
        for slot in ss13_game::inventory::ALL_SLOTS {
            if let Some(item) = inventory.slot_mut(slot).take() {
                world.despawn(item);
            }
        }
        world.entity_mut(b).insert(inventory);
    }
    s.click(gibber);
    s.run_until("starts stuffing", |s| s.said("начинает запихивать"));
    s.run_until_for(6.0, "B inside", |s| {
        s.try_get::<InCloset>(b).is_some_and(|c| c.closet == gibber)
    });
    s.click(gibber);
    s.run_until("gibbing", |s| s.get::<Gibber>(gibber).finishes_at.is_some());
    assert!(s.server.world().get_entity(b).is_err(), "B is gone");
    s.run_until_for(6.0, "done", |s| s.get::<Gibber>(gibber).finishes_at.is_none());
    // One tier 1 matter bin: efficiency 0.5, so the carried chance makes at least the chest's
    // meat, one of the two skins and two of the four limbs drop.
    let meat = s.count("/obj/item/food/meat/slab/human");
    let skin = s.count("/obj/item/stack/sheet/animalhide/carbon/human");
    assert!((3..=5).contains(&meat), "{meat} meat");
    assert!((1..=2).contains(&skin), "{skin} skin");

    // The coffeemaker came with a pot and a cartridge: brew from the menu.
    let maker = s.machine::<CoffeeMaker>(COFFEE);
    let m = s.get::<CoffeeMaker>(maker).clone();
    let (pot, cartridge) = (m.pot.unwrap(), m.cartridge.unwrap());
    assert_eq!(s.get::<Cartridge>(cartridge).charges, 4);
    s.pick(maker, "Заварить");
    s.run_until("brewing", |s| s.get::<CoffeeMaker>(maker).brewing_until.is_some());
    assert_eq!(s.amount(pot, "/datum/reagent/consumable/coffee"), 120.0);
    assert_eq!(s.get::<Cartridge>(cartridge).charges, 3);
    // Busy for 20 s: a right click does nothing; then it takes the pot out.
    s.click_with(maker, RIGHT);
    s.run_for(1.0);
    assert_eq!(s.get::<CoffeeMaker>(maker).pot, Some(pot));
    s.run_until_for(25.0, "brewed", |s| {
        s.get::<CoffeeMaker>(maker).brewing_until.is_none()
    });
    s.click_with(maker, RIGHT);
    s.run_until("the pot in hand", |s| s.held() == Some(pot));
    s.drop_held();

    // The vat: a waffle cone in cone mode, then a vanilla scoop on it.
    let vat = s.machine::<IceCreamVat>(VAT);
    let flour = s.amount(vat, "/datum/reagent/consumable/flour");
    assert_eq!(flour, 6.0);
    s.click_with(vat, RIGHT);
    s.run_until("cone mode", |s| s.get::<IceCreamVat>(vat).cones_mode);
    s.pick(vat, "waffle cone");
    s.run_until("a cone in hand", |s| {
        s.held()
            .is_some_and(|h| s.get::<DmItem>(h).path == "/obj/item/food/icecream")
    });
    let cone = s.held().unwrap();
    assert_eq!(s.amount(vat, "/datum/reagent/consumable/flour"), 5.0);
    assert_eq!(s.amount(vat, "/datum/reagent/consumable/sugar"), 5.0);
    s.click(vat);
    s.run_until("a scoop", |s| s.try_get::<Scoops>(cone).is_some());
    assert_eq!(s.get::<Scoops>(cone).0, ["vanilla"]);
    assert_eq!(s.get::<Name>(cone).as_str(), "vanilla ice cream");
    assert_eq!(s.amount(cone, "/datum/reagent/consumable/vanilla"), 3.0);
    assert_eq!(s.amount(cone, "/datum/reagent/consumable/sugar"), 10.0);
    assert_eq!(s.get::<Reagents>(cone).0.maximum_volume, 30.0);
    assert_eq!(s.amount(vat, "/datum/reagent/consumable/milk"), 5.0);
}

#[test]
fn soup_and_burning() {
    let mut s = Station::start("soup_and_burning", 1);

    // The mapped range has a soup pot on its burner; the meatball goes in, water is poured.
    let range = s.machine::<Stove>(RANGE);
    let pot = s.get::<Stove>(range).container.expect("a mapped range has a pot");
    assert_eq!(*s.get::<TilePos>(pot), RANGE);
    let meatball = s.on_tile(SPAWN, "/obj/item/food/meatball").unwrap();
    s.take(meatball);
    s.click(pot);
    s.run_until("in the pot", |s| {
        s.get::<SoupPot>(pot).ingredients == [meatball]
    });
    {
        let world = s.0.server.world_mut();
        let chem = world.resource::<Chem>().0.clone();
        let water = chem.id("/datum/reagent/water").unwrap();
        let mut r = world.get_mut::<Reagents>(pot).unwrap();
        r.0.add_reagent(&chem, water, 60.0, 300.0, None, None, true, false);
        // Most of the way to the soup's 450 K, so the test needs not boil it for a minute.
        r.0.set_temperature(440.0);
    }
    // The burner on (right click of the closed range): it heats the pot and the soup starts.
    s.click_with(range, RIGHT);
    s.run_until("lit", |s| s.get::<Stove>(range).on);
    s.run_until_for(30.0, "soup", |s| {
        s.amount(pot, "/datum/reagent/consumable/nutriment/soup/meatball_soup") > 0.0
    });
    s.run_until_for(30.0, "the soup done", |s| {
        s.get::<SoupPot>(pot).soup.is_none()
            && s.amount(pot, "/datum/reagent/consumable/nutriment/soup/meatball_soup") >= 29.0
    });
    assert!(s.server.world().get_entity(meatball).is_err(), "used up");
    assert!(s.get::<SoupPot>(pot).ingredients.is_empty());
    // The meatball's nutriment went in (4, a quarter of each 2 unit step converted, 0.8 of the
    // rest).
    let nutriment = s.amount(pot, "/datum/reagent/consumable/nutriment");
    assert!(nutriment > 1.0 && nutriment < 4.0, "{nutriment}");

    // The toy on the lit griddle: nothing grills it, so it burns (20 a tick from 200) to ash.
    let griddle = s.machine::<Griddle>(GRIDDLE);
    let toy = s.on_tile(SPAWN, "/obj/item/toy").unwrap();
    s.take(toy);
    s.click(griddle);
    s.run_until("on the griddle", |s| s.get::<Griddle>(griddle).items == [toy]);
    s.click(griddle);
    s.run_until("on", |s| s.get::<Griddle>(griddle).on);
    s.run_until_for(30.0, "burnt", |s| s.server.world().get_entity(toy).is_err());
    let world = s.server.world();
    let ash = world
        .resource::<SpatialIndex>()
        .on_tile(GRIDDLE)
        .iter()
        .find(|&&e| {
            world
                .get::<DmName>(e)
                .is_some_and(|n| n.name == "ashes")
        })
        .copied();
    assert!(ash.is_some(), "ashes on the griddle");

    // A glass of water thrown onto the floor south: it smashes, a broken bottle is left.
    let glass = s
        .on_tile(SPAWN, "/obj/item/reagent_containers/cup/glass/drinkingglass")
        .unwrap();
    {
        let world = s.0.server.world_mut();
        let chem = world.resource::<Chem>().0.clone();
        let water = chem.id("/datum/reagent/water").unwrap();
        let mut r = world.get_mut::<Reagents>(glass).unwrap();
        r.0.add_reagent(&chem, water, 10.0, 300.0, None, None, true, false);
        let south = TilePos::new(3, 2, 1);
        assert!(ss13_game::throw::throw_at(world, glass, south, 3, 1.0));
    }
    s.run_until("smashed", |s| s.server.world().get_entity(glass).is_err());
    assert!(s.on_tile(TilePos::new(3, 2, 1), "/obj/item/broken_bottle").is_some());
    s.run_until("the splash is seen", |s| s.said("разливает своё содержимое"));
}

#[test]
fn food_goes_bad_on_the_floor() {
    let mut s = Station::start("food_goes_bad_on_the_floor", 1);
    let mapped = s.on_tile(TilePos::new(2, 4, 1), "/obj/item/food/meat/slab").unwrap();
    // A slab that was not mapped, on the bare floor: 5 s of germs, then its 5 s
    // (`decomposition_time = 50`) into a moldy mess with its reagents, and ants.
    let floor = TilePos::new(4, 4, 1);
    let slab = {
        let world = s.0.server.world_mut();
        ss13_game::construction::spawn_obj(world, "/obj/item/food/meat/slab", floor).unwrap()
    };
    s.run_until_for(20.0, "rotten", |s| s.server.world().get_entity(slab).is_err());
    let moldy = s.on_tile(floor, "/obj/item/food/badrecipe/moldy").expect("moldy");
    assert_eq!(s.amount(moldy, "/datum/reagent/consumable/nutriment"), 4.0);
    // The moldy mess attracts ants: in its turn (3 s) it leaves them.
    s.run_until_for(20.0, "ants", |s| {
        let world = s.server.world();
        world.get_entity(moldy).is_err()
            && world.resource::<SpatialIndex>().on_tile(floor).iter().any(|&e| {
                world
                    .get::<DmName>(e)
                    .is_some_and(|n| n.name == "space ants")
            })
    });
    s.run_until("the mold is seen", |s| s.said("gets overtaken by mold and ants"));
    // The mapped one has not moved: still fresh.
    assert!(s.server.world().get_entity(mapped).is_ok());
}

#[test]
fn bar_sign() {
    let mut s = Station::start("bar_sign", 1);
    let a = s.players[0].mob;
    s.server.world_mut().entity_mut(a).insert(TilePos::new(5, 3, 1));
    s.run_until("moved", |s| *s.get::<TilePos>(a) == TilePos::new(5, 3, 1));
    let sign = s.machine::<ss13_game::barsign::Barsign>(TilePos::new(5, 4, 1));
    assert_eq!(s.get::<ss13_game::barsign::Barsign>(sign).sign, "Off");
    // An assistant's ID has no bar access.
    s.click(sign);
    s.run_until("denied", |s| s.said("в доступе отказано"));
    {
        let world = s.0.server.world_mut();
        let card = world
            .get::<Inventory>(a)
            .unwrap()
            .clone()
            .id_card(world)
            .unwrap();
        world
            .get_mut::<ss13_game::inventory::IdCard>(card)
            .unwrap()
            .access
            .push("bar".into());
    }
    s.pick(sign, "The Bark");
    s.run_until("the bark", |s| {
        s.get::<ss13_game::barsign::Barsign>(sign).sign == "The Bark"
    });
    assert_eq!(s.get::<Name>(sign).as_str(), "bar sign (The Bark)");
    assert_eq!(s.get::<DmName>(sign).desc, "Ian's bar of choice.");
    let world = s.server.world();
    assert_eq!(
        Some(world.get::<ss13_shared::Appearance>(sign).unwrap().icon_state),
        world
            .resource::<ss13_mapload::IconIndex>()
            .get("bs.dmi", "thebark")
    );
}

#[test]
fn drying_rack() {
    let mut s = Station::start("drying_rack", 1);
    let a = s.players[0].mob;
    let by = TilePos::new(5, 3, 1);
    s.server.world_mut().entity_mut(a).insert(by);
    s.run_until("moved", |s| *s.get::<TilePos>(a) == by);
    let rack = s.machine::<ss13_game::smartfridge::SmartFridge>(TilePos::new(5, 2, 1));
    let slab = {
        let world = s.0.server.world_mut();
        ss13_game::construction::spawn_obj(world, "/obj/item/food/meat/slab", by).unwrap()
    };
    s.run_for(1.0);
    s.take(slab);
    s.click(rack);
    s.run_until("on the rack", |s| {
        s.try_get::<ss13_game::storage::Stored>(rack)
            .is_some_and(|st| st.items().contains(&slab))
    });
    // Its window's "Dry": the next machine tick turns the slab into jerky (with its reagents).
    s.click(rack);
    s.run_until("the window", |s| fridge_window(s).is_some());
    let window = fridge_window(&s).unwrap();
    s.act(0, window, "Dry", &[]);
    s.run_until("drying", |s| {
        s.get::<ss13_game::smartfridge::SmartFridge>(rack).drying
    });
    s.run_until_for(5.0, "jerky", |s| s.server.world().get_entity(slab).is_err());
    let jerky = s.get::<ss13_game::storage::Stored>(rack).items()[0];
    assert_eq!(
        s.get::<DmItem>(jerky).path,
        "/obj/item/food/sosjerky/healthy"
    );
    assert!(s.try_get::<ss13_game::food::Dried>(jerky).is_some());
    assert_eq!(s.amount(jerky, "/datum/reagent/consumable/nutriment"), 4.0);
}
