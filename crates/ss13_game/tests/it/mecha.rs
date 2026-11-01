//! Exosuits over real WebTransport on 127.0.0.1, on a fixture robotics bay: iron goes into the
//! exosuit fabricator, which prints a Ripley chassis; the parts, boards, stock parts, a cell and
//! the plating build it with tg's steps; a clamp goes on its left arm; the player climbs in
//! (dragging itself onto it), drives 3 tiles east (the first step turns it; the cell pays every
//! attempt), clamps a crate into the cargo compartment and ejects.

use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_assets::manifest::{AssetManifest, IconStateInfo};
use ss13_game::inventory::Inventory;
use ss13_game::mecha::core::ItemCell;
use ss13_game::mecha::fab::MechFab;
use ss13_game::mecha::{ExoFabData, ExoFabStatic, ExosuitFabricatorUi, Mech, Pilot};
use ss13_game::science::lathe::MaterialStore;
use ss13_game::testing::{self, Drive, fixture_map};
use ss13_game::tools::{Stack, Welder};
use ss13_mapload::DmItem;
use ss13_mapload::convert::{DmTypes, convert};
use ss13_net::chat::client::ChatReceived;
use ss13_net::ui::client::UiMessageReceived;
use ss13_shared::protocol::{ClickModifiers, ClientIntent, DragEnd};
use ss13_shared::ui::UiServerMessage;
use ss13_shared::{Dir, TilePos};
use ss13_tgui::{UiInterface, decode};
use ss13_world::SpatialIndex;

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/datum/armor": { "parent": "/datum" },
    "/datum/armor/mecha_ripley": { "parent": "/datum/armor", "declared_vars": {
        "melee": { "expr": "40" }, "bullet": { "expr": "20" }, "laser": { "expr": "10" } } },
    "/datum/controller": { "parent": "/datum" },
    "/datum/controller/subsystem": { "parent": "/datum/controller" },
    "/datum/controller/subsystem/research": { "parent": "/datum/controller/subsystem",
        "declared_vars": {
            "point_types": { "list": [ { "assoc": [ "General Research", "Gen. Res." ] } ] },
            "single_server_income": { "list": [ { "assoc": [ "General Research", 1 ] } ] } } },
    "/datum/material": { "parent": "/datum", "declared_vars": {
        "abstract_type": { "path": "/datum/material" }, "mat_flags": 0, "name": "material",
        "sheet_type": null, "ore_type": null, "color": null } },
    "/datum/material/iron": { "parent": "/datum/material", "declared_vars": { "mat_flags": 23,
        "name": "iron", "sheet_type": { "path": "/obj/item/stack/sheet/iron" } } },
    "/datum/design": { "parent": "/datum", "declared_vars": { "name": null, "desc": null,
        "build_type": null, "build_path": null, "materials": { "list": [] },
        "category": { "list": [] }, "departmental_flags": 16777215,
        "construction_time": 32, "lathe_time_factor": 1, "fixed_cost_efficiency": 0 } },
    "/datum/design/ripley_chassis": { "parent": "/datum/design", "declared_vars": {
        "name": "Exosuit Chassis (APLU \"Ripley\")", "build_type": 8,
        "build_path": { "path": "/obj/item/mecha_parts/chassis/ripley" },
        "materials": { "list": [ { "assoc": [ { "path": "/datum/material/iron" }, 1000 ] } ] },
        "construction_time": 10, "category": { "list": [ "/Ripley/Chassis" ] } } },
    "/datum/techweb_node": { "parent": "/datum", "declared_vars": { "display_name": null,
        "description": null, "category": "Misc", "node_flags": 8,
        "prerequisite_nodes": null, "unlocked_designs": null, "research_costs": null,
        "required_experiments": null, "discount_experiments": null,
        "required_items_to_unlock": null, "announce_channels": null } },
    "/datum/techweb_node/mech_assembly": { "parent": "/datum/techweb_node", "declared_vars": {
        "display_name": "Mech", "node_flags": 9,
        "unlocked_designs": { "list": [ { "path": "/datum/design/ripley_chassis" } ] } } },
    "/datum/stock_part": { "parent": "/datum" },
    "/datum/stock_part/servo": { "parent": "/datum/stock_part" },
    "/datum/stock_part/matter_bin": { "parent": "/datum/stock_part" },
    "/datum/stock_part/micro_laser": { "parent": "/datum/stock_part" },
    "/datum/component": { "parent": "/datum" },
    "/datum/component/construction": { "parent": "/datum/component" },
    "/datum/component/construction/unordered": { "parent": "/datum/component/construction" },
    "/datum/component/construction/unordered/mecha_chassis": {
        "parent": "/datum/component/construction/unordered" },
    "/datum/component/construction/unordered/mecha_chassis/ripley": {
        "parent": "/datum/component/construction/unordered/mecha_chassis", "declared_vars": {
            "result": { "path": "/datum/component/construction/mecha/ripley" },
            "steps": { "list": [ { "path": "/obj/item/mecha_parts/part/ripley_torso" },
                { "path": "/obj/item/mecha_parts/part/ripley_left_arm" },
                { "path": "/obj/item/mecha_parts/part/ripley_right_arm" },
                { "path": "/obj/item/mecha_parts/part/ripley_left_leg" },
                { "path": "/obj/item/mecha_parts/part/ripley_right_leg" } ] } } },
    "/datum/component/construction/mecha": { "parent": "/datum/component/construction" },
    "/datum/component/construction/mecha/ripley": {
        "parent": "/datum/component/construction/mecha", "procs": [ "get_outer_plating_steps" ],
        "declared_vars": { "base_icon": "ripley",
            "result": { "path": "/obj/vehicle/sealed/mecha/ripley" },
            "circuit_control": { "path": "/obj/item/circuitboard/mecha/ripley/main" },
            "circuit_periph": { "path": "/obj/item/circuitboard/mecha/ripley/peripherals" },
            "inner_plating": { "path": "/obj/item/stack/sheet/iron" }, "inner_plating_amount": 5,
            "outer_plating": { "path": "/obj/item/stack/rods" }, "outer_plating_amount": 10 } },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 }, "declared_vars": { "can_atmos_pass": 1 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3, "anchored": 0 } },
    "/obj": { "parent": "/atom/movable", "declared_vars": { "req_access": null } },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start" },
    "/obj/machinery": { "parent": "/obj", "declared_vars": { "anchored": 1, "density": 1 } },
    "/obj/machinery/mecha_part_fabricator": { "parent": "/obj/machinery", "declared_vars": {
        "name": "exosuit fabricator", "icon": { "resource": "m.dmi" }, "icon_state": "fab-idle",
        "link_on_init": 1, "circuit": { "path": "/obj/item/circuitboard/machine/mechfab" } } },
    "/obj/machinery/mecha_part_fabricator/maint": {
        "parent": "/obj/machinery/mecha_part_fabricator", "declared_vars": { "link_on_init": 0 } },
    "/obj/machinery/cell_charger": { "parent": "/obj/machinery", "declared_vars": {
        "name": "cell charger", "icon": { "resource": "icons/obj/machines/cell_charger.dmi" },
        "icon_state": "ccharger", "density": 0 } },
    "/obj/machinery/mech_bay_recharge_port": { "parent": "/obj/machinery", "declared_vars": {
        "name": "mech bay power port", "icon": { "resource": "mb.dmi" },
        "icon_state": "recharge_port", "dir": 4 } },
    "/obj/machinery/computer": { "parent": "/obj/machinery", "declared_vars": {
        "icon": { "resource": "icons/obj/machines/computer.dmi" }, "icon_state": "computer" } },
    "/obj/machinery/computer/mech_bay_power_console": { "parent": "/obj/machinery/computer",
        "declared_vars": { "name": "mech bay power control console" } },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null, "w_class": 3,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null, "tool_behaviour": null, "toolspeed": 1,
        "heat": 0 } },
    "/obj/item/wrench": { "parent": "/obj/item", "declared_vars": { "name": "wrench",
        "icon_state": "wrench", "tool_behaviour": "wrench", "force": 10 } },
    "/obj/item/crowbar": { "parent": "/obj/item", "declared_vars": { "name": "crowbar",
        "icon_state": "crowbar", "tool_behaviour": "crowbar" } },
    "/obj/item/screwdriver": { "parent": "/obj/item", "declared_vars": { "name": "screwdriver",
        "icon_state": "screwdriver", "tool_behaviour": "screwdriver" } },
    "/obj/item/wirecutters": { "parent": "/obj/item", "declared_vars": { "name": "wirecutters",
        "icon_state": "cutters", "tool_behaviour": "cutters" } },
    "/obj/item/weldingtool": { "parent": "/obj/item", "declared_vars": { "name": "welder",
        "icon_state": "welder", "tool_behaviour": "welder", "max_fuel": 20, "heat": 3800 } },
    "/obj/item/circuitboard": { "parent": "/obj/item" },
    "/obj/item/circuitboard/machine": { "parent": "/obj/item/circuitboard" },
    "/obj/item/circuitboard/machine/mechfab": { "parent": "/obj/item/circuitboard/machine",
        "declared_vars": { "req_components": { "list": [
            { "assoc": [ { "path": "/datum/stock_part/matter_bin" }, 2 ] },
            { "assoc": [ { "path": "/datum/stock_part/servo" }, 1 ] },
            { "assoc": [ { "path": "/datum/stock_part/micro_laser" }, 1 ] } ] } } },
    "/obj/item/circuitboard/mecha": { "parent": "/obj/item/circuitboard" },
    "/obj/item/circuitboard/mecha/ripley": { "parent": "/obj/item/circuitboard/mecha" },
    "/obj/item/circuitboard/mecha/ripley/main": { "parent": "/obj/item/circuitboard/mecha/ripley",
        "declared_vars": { "name": "central control module" } },
    "/obj/item/circuitboard/mecha/ripley/peripherals": {
        "parent": "/obj/item/circuitboard/mecha/ripley",
        "declared_vars": { "name": "peripherals control module" } },
    "/obj/item/stock_parts": { "parent": "/obj/item", "declared_vars": { "rating": 1 } },
    "/obj/item/stock_parts/scanning_module": { "parent": "/obj/item/stock_parts" },
    "/obj/item/stock_parts/capacitor": { "parent": "/obj/item/stock_parts" },
    "/obj/item/stock_parts/servo": { "parent": "/obj/item/stock_parts" },
    "/obj/item/stock_parts/power_store": { "parent": "/obj/item/stock_parts", "declared_vars": {
        "maxcharge": 10000, "chargerate": 500, "empty": 0 } },
    "/obj/item/stock_parts/power_store/cell": { "parent": "/obj/item/stock_parts/power_store" },
    "/obj/item/stock_parts/power_store/cell/high": {
        "parent": "/obj/item/stock_parts/power_store/cell", "declared_vars": {
            "name": "high-capacity power cell", "maxcharge": 100000 } },
    "/obj/item/stack": { "parent": "/obj/item", "declared_vars": { "amount": 1,
        "max_amount": 50, "merge_type": null, "singular_name": null, "mats_per_unit": null } },
    "/obj/item/stack/sheet": { "parent": "/obj/item/stack" },
    "/obj/item/stack/sheet/iron": { "parent": "/obj/item/stack/sheet", "declared_vars": {
        "name": "iron", "icon_state": "sheet-metal", "singular_name": "iron sheet",
        "merge_type": { "path": "/obj/item/stack/sheet/iron" },
        "mats_per_unit": { "list": [ { "assoc": [ { "path": "/datum/material/iron" }, 100 ] } ] } } },
    "/obj/item/stack/rods": { "parent": "/obj/item/stack", "declared_vars": { "name": "iron rod",
        "icon_state": "rods", "merge_type": { "path": "/obj/item/stack/rods" } } },
    "/obj/item/stack/cable_coil": { "parent": "/obj/item/stack", "declared_vars": {
        "name": "cable", "icon_state": "coil", "merge_type": { "path": "/obj/item/stack/cable_coil" } } },
    "/obj/item/mecha_parts": { "parent": "/obj/item", "declared_vars": {
        "icon": { "resource": "icons/mob/rideables/mech_construct.dmi" }, "icon_state": "blank",
        "w_class": 6 } },
    "/obj/item/mecha_parts/chassis": { "parent": "/obj/item/mecha_parts", "declared_vars": {
        "icon_state": "backbone", "interaction_flags_item": 0 } },
    "/obj/item/mecha_parts/chassis/ripley": { "parent": "/obj/item/mecha_parts/chassis",
        "declared_vars": { "name": "\\improper Ripley chassis", "construct_type": {
            "path": "/datum/component/construction/unordered/mecha_chassis/ripley" } } },
    "/obj/item/mecha_parts/part": { "parent": "/obj/item/mecha_parts" },
    "/obj/item/mecha_parts/part/ripley_torso": { "parent": "/obj/item/mecha_parts/part",
        "declared_vars": { "name": "\\improper Ripley torso", "icon_state": "ripley_harness" } },
    "/obj/item/mecha_parts/part/ripley_left_arm": { "parent": "/obj/item/mecha_parts/part",
        "declared_vars": { "name": "\\improper Ripley left arm", "icon_state": "ripley_l_arm" } },
    "/obj/item/mecha_parts/part/ripley_right_arm": { "parent": "/obj/item/mecha_parts/part",
        "declared_vars": { "name": "\\improper Ripley right arm", "icon_state": "ripley_r_arm" } },
    "/obj/item/mecha_parts/part/ripley_left_leg": { "parent": "/obj/item/mecha_parts/part",
        "declared_vars": { "name": "\\improper Ripley left leg", "icon_state": "ripley_l_leg" } },
    "/obj/item/mecha_parts/part/ripley_right_leg": { "parent": "/obj/item/mecha_parts/part",
        "declared_vars": { "name": "\\improper Ripley right leg", "icon_state": "ripley_r_leg" } },
    "/obj/item/mecha_parts/mecha_equipment": { "parent": "/obj/item/mecha_parts",
        "declared_vars": { "abstract_type": { "path": "/obj/item/mecha_parts/mecha_equipment" },
            "icon": { "resource": "e.dmi" }, "equipment_slot": "mecha_weapon",
            "equip_cooldown": 0, "range": 1, "mech_flags": 16777215, "detachable": 1,
            "energy_drain": 0 } },
    "/obj/item/mecha_parts/mecha_equipment/hydraulic_clamp": {
        "parent": "/obj/item/mecha_parts/mecha_equipment", "declared_vars": {
            "name": "hydraulic clamp", "icon_state": "mecha_clamp", "equip_cooldown": 15,
            "energy_drain": 100, "mech_flags": 1, "harmful": 1, "clamp_damage": 20 } },
    "/obj/item/mecha_parts/mecha_equipment/ejector": {
        "parent": "/obj/item/mecha_parts/mecha_equipment", "declared_vars": {
            "name": "cargo compartment", "icon_state": "mecha_bin",
            "equipment_slot": "mecha_utility", "detachable": 0, "cargo_capacity": 15 } },
    "/obj/vehicle": { "parent": "/obj", "declared_vars": { "density": 1, "layer": 3.9,
        "movedelay": 2, "max_integrity": 300, "abstract_type": { "path": "/obj/vehicle" } } },
    "/obj/vehicle/sealed": { "parent": "/obj/vehicle", "declared_vars": { "enter_delay": 20 } },
    "/obj/vehicle/sealed/mecha": { "parent": "/obj/vehicle/sealed", "declared_vars": {
        "name": "exosuit", "icon": { "resource": "icons/mob/rideables/mecha.dmi" },
        "movedelay": 10, "step_energy_drain": 80, "melee_energy_drain": 150,
        "max_integrity": 300, "exit_delay": 20, "allow_diagonal_movement": 1,
        "mecha_flags": 52324,
        "facing_modifiers": { "list": [ { "assoc": [ "mechafront", 0.5 ] },
            { "assoc": [ "mechaside", 1 ] }, { "assoc": [ "mechaback", 1.5 ] } ] },
        "max_equip_by_category": { "list": [ { "assoc": [ "mecha_l_arm", 1 ] },
            { "assoc": [ "mecha_r_arm", 1 ] }, { "assoc": [ "mecha_utility", 2 ] } ] } } },
    "/obj/vehicle/sealed/mecha/ripley": { "parent": "/obj/vehicle/sealed/mecha",
        "procs": [ "Move", "update_pressure" ],
        "declared_vars": { "name": "\\improper APLU MK-I \\\"Ripley\\\"",
            "icon_state": "ripley", "base_icon_state": "ripley", "movedelay": 1.5,
            "max_integrity": 200, "armor_type": { "expr": "/datum/armor/mecha_ripley" },
            "enter_delay": 10, "exit_delay": 10, "mecha_flags": 16452, "mech_type": 1,
            "accesses": { "list": [ "mech_engine" ] },
            "fast_pressure_step_in": 1.5, "slow_pressure_step_in": 2,
            "wreckage": { "path": "/obj/structure/mecha_wreckage/ripley" },
            "max_equip_by_category": { "list": [ { "assoc": [ "mecha_l_arm", 1 ] },
                { "assoc": [ "mecha_r_arm", 1 ] }, { "assoc": [ "mecha_utility", 4 ] } ] },
            "equip_by_category": { "list": [ { "assoc": [ "mecha_l_arm", null ] },
                { "assoc": [ "mecha_utility", { "list": [
                    { "path": "/obj/item/mecha_parts/mecha_equipment/ejector" } ] } ] } ] } } },
    "/obj/structure": { "parent": "/obj", "declared_vars": { "anchored": 1 } },
    "/obj/structure/mecha_wreckage": { "parent": "/obj/structure", "declared_vars": {
        "name": "exosuit wreckage", "icon": { "resource": "icons/mob/rideables/mecha.dmi" },
        "density": 1, "anchored": 0, "salvage_num": 5 } },
    "/obj/structure/mecha_wreckage/ripley": { "parent": "/obj/structure/mecha_wreckage",
        "declared_vars": { "icon_state": "ripley-broken" } },
    "/obj/structure/closet": { "parent": "/obj/structure", "declared_vars": {
        "name": "closet", "icon": { "resource": "c.dmi" }, "icon_state": "generic",
        "density": 1, "anchored": 0, "opened": 0, "locked": 0, "welded": 0, "secure": 0,
        "icon_door": null, "icon_welded": "welded", "enable_door_overlay": 1,
        "has_opened_overlay": 1, "has_closed_overlay": 1, "dense_when_open": 0,
        "wall_mounted": 0, "horizontal": 0, "storage_capacity": 30 } },
    "/obj/structure/closet/crate": { "parent": "/obj/structure/closet", "declared_vars": {
        "name": "crate", "icon_state": "crate", "horizontal": 1 } },
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
    "/turf/closed": { "parent": "/turf", "declared_vars": { "density": 1, "opacity": 1 } },
    "/turf/closed/wall": { "parent": "/turf/closed", "declared_vars": { "icon_state": "wall" } },
    "/area": { "parent": "/atom" },
    "/area/space": { "parent": "/area" }
  },
  "populate": {
    "/obj/structure/closet": { "PopulateContents": ["return"] }
  }
}"##;

/// Rows top-down, the last line is y = 1. The player spawns at (5,3) next to the maintenance
/// exosuit fabricator `F` (4,4), which prints onto (4,3); a crate `C` at (9,3); a mapped
/// Ripley `R` at (7,2).
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"F" = (/obj/machinery/mecha_part_fabricator/maint,/turf/open/floor,/area/space)
"C" = (/obj/structure/closet/crate,/turf/open/floor,/area/space)
"R" = (/obj/vehicle/sealed/mecha/ripley,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwwwwwwwww
wffFfffffffw
wfffsfffCffw
wfffffRffffw
wwwwwwwwwwww
"}
"##;

const SPAWN: TilePos = TilePos::new(5, 3, 1);
const FAB: TilePos = TilePos::new(4, 4, 1);
const OUTLET: TilePos = TilePos::new(4, 3, 1);
const CRATE: TilePos = TilePos::new(9, 3, 1);
const CHASSIS: &str = "/obj/item/mecha_parts/chassis/ripley";
const MECHA_DMI: &str = "icons/mob/rideables/mecha.dmi";

fn states() -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = [
        ("t.dmi", "floor"),
        ("t.dmi", "wall"),
        ("l.dmi", "x2"),
        ("h.dmi", "human_basic"),
        ("i.dmi", "toy"),
        ("i.dmi", "card_grey"),
        ("i.dmi", "wrench"),
        ("i.dmi", "crowbar"),
        ("i.dmi", "screwdriver"),
        ("i.dmi", "cutters"),
        ("i.dmi", "welder"),
        ("i.dmi", "sheet-metal"),
        ("i.dmi", "rods"),
        ("i.dmi", "coil"),
        ("m.dmi", "fab-idle"),
        ("c.dmi", "crate"),
        ("icons/obj/machines/cell_charger.dmi", "ccharger"),
        ("icons/obj/machines/cell_charger.dmi", "cell"),
        ("icons/obj/machines/cell_charger.dmi", "ccharger-o0"),
        ("icons/obj/machines/cell_charger.dmi", "ccharger-o1"),
        (
            "icons/obj/machines/cell_charger.dmi",
            "ccharger-standard-on",
        ),
        ("icons/obj/machines/cell_charger.dmi", "cell-standard-o1"),
        ("mb.dmi", "recharge_port"),
        ("icons/obj/machines/computer.dmi", "computer"),
        ("icons/obj/machines/computer.dmi", "recharge_comp_on"),
        ("e.dmi", "mecha_clamp"),
        ("e.dmi", "mecha_bin"),
        (MECHA_DMI, "ripley"),
        (MECHA_DMI, "ripley-open"),
        (MECHA_DMI, "ripley-broken"),
        ("icons/mob/rideables/mech_construct.dmi", "backbone"),
    ]
    .iter()
    .map(|&(d, s)| (d.to_owned(), s.to_owned()))
    .collect();
    for part in [
        "ripley_harness",
        "ripley_l_arm",
        "ripley_r_arm",
        "ripley_l_leg",
        "ripley_r_leg",
    ] {
        let dmi = "icons/mob/rideables/mech_construct.dmi";
        out.push((dmi.to_owned(), part.to_owned()));
        out.push((dmi.to_owned(), format!("{part}+o")));
    }
    for i in 0..21 {
        out.push((
            "icons/mob/rideables/mech_construction.dmi".to_owned(),
            format!("ripley{i}"),
        ));
    }
    out
}

fn fixture_assets_on(test: &str, map: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("mecha_fixture")
        .join(test);
    std::fs::create_dir_all(dir.join("prototypes")).unwrap();
    let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
    let research = ss13_mapload::research::resolve(&types);
    let mecha = ss13_mapload::mecha::resolve(&types);
    let mut used = vec![
        "/obj/effect/landmark/start/assistant",
        "/obj/machinery/mecha_part_fabricator/maint",
        "/obj/machinery/cell_charger",
        "/obj/machinery/mech_bay_recharge_port",
        "/obj/machinery/computer/mech_bay_power_console",
        "/obj/item/stock_parts/power_store/cell/high",
        "/obj/structure/closet/crate",
        "/obj/item/wrench",
        "/obj/item/crowbar",
        "/obj/vehicle/sealed/mecha/ripley",
        "/obj/item/screwdriver",
        "/obj/item/wirecutters",
        "/obj/item/weldingtool",
        "/obj/item/stack/cable_coil",
        "/obj/item/stock_parts/scanning_module",
        "/obj/item/stock_parts/capacitor",
        "/obj/item/stock_parts/servo",
        "/obj/item/mecha_parts/mecha_equipment/hydraulic_clamp",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/area/space",
    ];
    used.extend(ss13_mapload::RUNTIME_TYPES);
    used.extend(research.prototype_types());
    used.extend(mecha.prototype_types());
    let conversion = convert(&types, used).unwrap();
    std::fs::write(dir.join("prototypes/dm_types.ron"), conversion.ron).unwrap();
    std::fs::write(
        dir.join(ss13_mapload::research::RESEARCH_FILE),
        research.to_ron(),
    )
    .unwrap();
    std::fs::write(dir.join(ss13_mapload::mecha::MECHA_FILE), mecha.to_ron()).unwrap();
    let manifest = AssetManifest {
        atlases: vec![],
        states: states()
            .into_iter()
            .map(|(dmi, state)| IconStateInfo {
                dmi,
                state,
                atlas: 0,
                width: 32,
                height: 32,
                dirs: 4,
                rects: vec![[0, 0, 32, 32]; 4],
                delays: vec![],
                loop_count: 0,
                rewind: false,
                movement: false,
            })
            .collect(),
    };
    std::fs::write(
        dir.join(ss13_assets::import::MANIFEST_FILE),
        postcard::to_stdvec(&manifest).unwrap(),
    )
    .unwrap();
    std::fs::write(dir.join("map.dmm"), map).unwrap();
    dir
}

#[derive(Resource, Default)]
struct Chat(Vec<String>);

/// The newest window of each interface: id, static data and data, still encoded.
#[derive(Resource, Default)]
struct Windows(Vec<(String, u32, Vec<u8>, Vec<u8>)>);

fn record(
    mut chat: MessageReader<ChatReceived>,
    mut ui: MessageReader<UiMessageReceived>,
    mut out: ResMut<Chat>,
    mut windows: ResMut<Windows>,
) {
    for ChatReceived(message) in chat.read() {
        out.0.push(message.plain_text());
    }
    for UiMessageReceived(message) in ui.read() {
        match message {
            UiServerMessage::Open {
                window,
                interface,
                static_data,
                data,
                ..
            } => {
                windows.0.retain(|(i, ..)| i != interface);
                windows.0.push((
                    interface.clone(),
                    *window,
                    static_data.clone(),
                    data.clone(),
                ));
            }
            UiServerMessage::Update {
                window,
                data: Some(data),
                ..
            } => {
                if let Some(w) = windows.0.iter_mut().find(|w| w.1 == *window) {
                    w.3.clone_from(data);
                }
            }
            UiServerMessage::Close { window } => windows.0.retain(|w| w.1 != *window),
            _ => {}
        }
    }
}

struct Bay {
    st: testing::Station,
    mob: Entity,
}

impl Deref for Bay {
    type Target = testing::Station;
    fn deref(&self) -> &testing::Station {
        &self.st
    }
}

impl DerefMut for Bay {
    fn deref_mut(&mut self) -> &mut testing::Station {
        &mut self.st
    }
}

impl Drive for Bay {
    fn station(&mut self) -> &mut testing::Station {
        &mut self.st
    }

    fn context(&self) -> String {
        let chat = &self.st.players[0].app.world().resource::<Chat>().0;
        format!("; chat: {chat:?}")
    }
}

impl Bay {
    fn start(test: &str) -> Self {
        Self::start_on(test, MAP, SPAWN)
    }

    fn start_on(test: &str, map: &str, spawn: TilePos) -> Self {
        let assets = fixture_assets_on(test, map);
        let mut st = testing::Station::start(fixture_map(&assets, &["map.dmm"]), |_| {});
        let p = st.join(test, |app| {
            app.init_resource::<Chat>()
                .init_resource::<Windows>()
                .add_systems(Update, record);
        });
        let mob = st.players[p].mob;
        let s = Self { st, mob };
        assert_eq!(s.pos(s.mob), spawn);
        s
    }

    fn send(&mut self, intent: ClientIntent) {
        self.st.send(0, intent);
    }

    fn pos(&self, e: Entity) -> TilePos {
        *self.server.world().get::<TilePos>(e).unwrap()
    }

    /// `entity`'s id, once replicated to the player.
    fn net_id(&mut self, entity: Entity) -> ss13_shared::protocol::NetId {
        self.run_until_for(10.0, "the target is replicated", |s| {
            s.st.net_id(0, entity).is_some()
        });
        self.st.net_id(0, entity).unwrap()
    }

    /// Clicks stay under tg's 15 per second of game time (`crate::click::SECOND_CLICK_LIMIT`).
    fn click(&mut self, entity: Entity) {
        self.run_for(0.1);
        self.net_id(entity);
        self.st.click(0, entity);
    }

    fn walk(&mut self, dir: Dir) {
        self.st.players[0].seq += 1;
        let seq = self.st.players[0].seq;
        self.send(ClientIntent::Move { dir, seq });
    }

    fn window<T: UiInterface>(&self) -> Option<(u32, T::StaticData, T::Data)> {
        let windows = self.players[0].app.world().resource::<Windows>();
        let (_, id, st, data) = windows.0.iter().find(|w| w.0 == T::NAME)?;
        Some((*id, decode(st).unwrap(), decode(data).unwrap()))
    }

    fn act<T: UiInterface>(&mut self, action: &str, params: &[(&str, &str)]) {
        let (window, ..) = self.window::<T>().expect("window");
        self.send(ClientIntent::UiAct {
            window,
            action: action.into(),
            params: params
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect(),
        });
    }

    fn find(&self, pos: TilePos, path: &str) -> Option<Entity> {
        let world = self.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .find(|&e| {
                world.get::<DmItem>(e).is_some_and(|i| i.path == path)
                    || world
                        .get::<ss13_mapload::DmObj>(e)
                        .is_some_and(|o| o.path == path)
            })
    }

    /// Empties the hands, then spawns `path` (a stack of `amount` when > 1) into the active one.
    fn give(&mut self, path: &str, amount: u32) -> Entity {
        let world = self.st.server.world_mut();
        let hands = world.get::<Inventory>(self.mob).unwrap().hands;
        for item in hands.into_iter().flatten() {
            world.despawn(item);
        }
        world.get_mut::<Inventory>(self.mob).unwrap().hands = [None, None];
        let pos = *world.get::<TilePos>(self.mob).unwrap();
        let item = ss13_game::construction::spawn_obj(world, path, pos)
            .unwrap_or_else(|| panic!("no prototype for {path}"));
        if amount > 1 {
            world.entity_mut(item).insert(Stack { amount });
        }
        let hand = world.get::<Inventory>(self.mob).unwrap().active;
        world
            .entity_mut(item)
            .remove::<TilePos>()
            .insert(ss13_game::inventory::CarriedBy(self.mob));
        world.get_mut::<Inventory>(self.mob).unwrap().hands[hand] = Some(item);
        item
    }

    fn empty_hands(&mut self) {
        let world = self.st.server.world_mut();
        let hands = world.get::<Inventory>(self.mob).unwrap().hands;
        for item in hands.into_iter().flatten() {
            world.despawn(item);
        }
        world.get_mut::<Inventory>(self.mob).unwrap().hands = [None, None];
    }

    fn said(&self, text: &str) -> bool {
        self.players[0]
            .app
            .world()
            .resource::<Chat>()
            .0
            .iter()
            .any(|l| l.contains(text))
    }

    fn mech_at(&self, pos: TilePos) -> Option<Entity> {
        let world = self.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .find(|&e| world.get::<Mech>(e).is_some())
    }

    fn charge(&self, mech: Entity) -> f64 {
        let world = self.server.world();
        let cell = world.get::<Mech>(mech).unwrap().cell.expect("a cell");
        world.get::<ItemCell>(cell).unwrap().0.charge
    }
}

#[test]
fn print_build_drive_clamp_and_eject() {
    let mut s = Bay::start("print_build_drive_clamp_and_eject");
    let world = s.server.world();
    let fab = s
        .find(FAB, "/obj/machinery/mecha_part_fabricator/maint")
        .unwrap();
    assert!(
        world.get::<MechFab>(fab).unwrap().silo.is_none(),
        "maint: no silo"
    );

    // 20 iron sheets into the fabricator, then its window.
    s.give("/obj/item/stack/sheet/iron", 20);
    s.click(fab);
    s.run_until_for(10.0, "iron inserted", |s| {
        s.server
            .world()
            .get::<MaterialStore>(fab)
            .is_some_and(|m| m.amounts[0] == 2000)
    });
    s.empty_hands();
    s.click(fab);
    s.run_until_for(10.0, "the window opens", |s| {
        s.window::<ExosuitFabricatorUi>().is_some()
    });
    let (_, st, data): (u32, ExoFabStatic, ExoFabData) = s.window::<ExosuitFabricatorUi>().unwrap();
    assert_eq!(st.designs.len(), 1);
    assert_eq!(data.researched, vec![0], "the starter node's design");
    let queue = |s: &Bay| s.server.world().get::<MechFab>(fab).unwrap().queue.len();
    // Unpowered, it takes no orders (tg `can_interact`).
    s.server
        .world_mut()
        .entity_mut(fab)
        .insert(ss13_game::power::Unpowered);
    s.act::<ExosuitFabricatorUi>("build", &[("designs", "/datum/design/ripley_chassis")]);
    s.hold(0.5, "no order without power", |s| queue(s) == 0);
    s.server
        .world_mut()
        .entity_mut(fab)
        .remove::<ss13_game::power::Unpowered>();
    // The queue stops at `MAX_QUEUE`.
    // Eight designs fit `MAX_PARAM_LEN`; at most ten acts a second.
    let eight = ["/datum/design/ripley_chassis"; 8].join(",");
    for _ in 0..=ss13_game::mecha::fab::MAX_QUEUE / 8 {
        s.act::<ExosuitFabricatorUi>("build", &[("designs", &eight)]);
        s.run_for(0.15);
    }
    s.run_until("the queue is full", |s| {
        queue(s) == ss13_game::mecha::fab::MAX_QUEUE
    });
    s.hold(0.5, "and stays capped", |s| {
        queue(s) == ss13_game::mecha::fab::MAX_QUEUE
    });
    s.act::<ExosuitFabricatorUi>("clear_queue", &[]);
    s.run_until("the queue is clear", |s| queue(s) == 0);
    s.act::<ExosuitFabricatorUi>(
        "build",
        &[("designs", "/datum/design/ripley_chassis"), ("now", "1")],
    );
    s.run_until_for(10.0, "the part is being built", |s| {
        s.window::<ExosuitFabricatorUi>()
            .is_some_and(|(_, _, d)| d.queue.first().is_some_and(|e| e.processing))
    });
    assert_eq!(
        s.server.world().get::<MaterialStore>(fab).unwrap().amounts[0],
        1000,
        "paid at start"
    );
    s.run_until_for(10.0, "the chassis comes out", |s| {
        s.find(OUTLET, CHASSIS).is_some()
    });
    s.run_until_for(5.0, "the fabricator says so", |s| {
        s.said("The fabrication of")
    });
    let chassis = s.find(OUTLET, CHASSIS).unwrap();

    // The parts, in any order, then tg's steps.
    for part in [
        "ripley_left_leg",
        "ripley_torso",
        "ripley_right_arm",
        "ripley_left_arm",
        "ripley_right_leg",
    ] {
        let item = s.give(&format!("/obj/item/mecha_parts/part/{part}"), 1);
        s.click(chassis);
        let mob = s.mob;
        s.run_until_for(5.0, &format!("{part} goes on the chassis"), |s| {
            !s.get::<Inventory>(mob).hands.contains(&Some(item))
        });
    }
    s.run_until_for(5.0, "the frame", |s| {
        s.server
            .world()
            .get::<ss13_game::mecha::construction::Frame>(chassis)
            .is_some_and(|f| {
                matches!(
                    f,
                    ss13_game::mecha::construction::Frame::Steps { index: 0, .. }
                )
            })
    });
    let welder = "/obj/item/weldingtool";
    let chain: [(&str, u32); 21] = [
        ("/obj/item/wrench", 1),
        ("/obj/item/screwdriver", 1),
        ("/obj/item/stack/cable_coil", 5),
        ("/obj/item/wirecutters", 1),
        ("/obj/item/circuitboard/mecha/ripley/main", 1),
        ("/obj/item/screwdriver", 1),
        ("/obj/item/circuitboard/mecha/ripley/peripherals", 1),
        ("/obj/item/screwdriver", 1),
        ("/obj/item/stock_parts/scanning_module", 1),
        ("/obj/item/screwdriver", 1),
        ("/obj/item/stock_parts/capacitor", 1),
        ("/obj/item/screwdriver", 1),
        ("/obj/item/stock_parts/servo", 1),
        ("/obj/item/screwdriver", 1),
        ("/obj/item/stock_parts/power_store/cell/high", 1),
        ("/obj/item/screwdriver", 1),
        ("/obj/item/stack/sheet/iron", 5),
        ("/obj/item/wrench", 1),
        (welder, 1),
        ("/obj/item/stack/rods", 10),
        (welder, 1),
    ];
    for (i, (path, amount)) in chain.into_iter().enumerate() {
        let item = s.give(path, amount);
        if path == welder {
            s.server.world_mut().get_mut::<Welder>(item).unwrap().on = true;
        }
        s.click(chassis);
        if i + 1 < chain.len() {
            s.run_until_for(5.0, &format!("step {i} ({path})"), |s| {
                s.server.world().get::<ss13_game::mecha::construction::Frame>(chassis).is_some_and(|f| {
                    matches!(f, ss13_game::mecha::construction::Frame::Steps { index, .. } if *index == i + 1)
                })
            });
        }
    }
    s.run_until_for(5.0, "the Ripley is built", |s| s.mech_at(OUTLET).is_some());
    let mech = s.mech_at(OUTLET).unwrap();
    assert!(
        s.server.world().get_entity(chassis).is_err(),
        "the chassis is used up"
    );
    assert_eq!(s.charge(mech), 100_000.0, "the frame's cell");
    {
        let world = s.server.world();
        let m = world.get::<Mech>(mech).unwrap();
        assert!(m.servo.is_some() && m.capacitor.is_some() && m.scanmod.is_some());
        assert_eq!(m.modules.len(), 1, "the cargo compartment");
    }

    // A clamp on the left arm.
    s.give("/obj/item/mecha_parts/mecha_equipment/hydraulic_clamp", 1);
    s.click(mech);
    s.run_until_for(5.0, "the clamp is attached", |s| {
        s.server.world().get::<Mech>(mech).unwrap().l_arm.is_some()
    });

    // Climbing in: drag yourself onto it with empty hands; 1 s.
    s.empty_hands();
    let from = s.net_id(s.mob);
    let to = s.net_id(mech);
    s.send(ClientIntent::Drag {
        from: DragEnd::Atom(from),
        to: DragEnd::Atom(to),
        modifiers: ClickModifiers::default(),
    });
    s.run_for(0.25);
    assert!(
        s.server.world().get::<Pilot>(s.mob).is_none(),
        "entering takes time"
    );
    s.run_until_for(5.0, "in the mech", |s| {
        s.server.world().get::<Pilot>(s.mob).is_some()
    });
    assert_eq!(s.pos(s.mob), OUTLET, "the pilot rides on the mech's tile");
    let ripley = s
        .server
        .world()
        .resource::<ss13_mapload::IconIndex>()
        .get(MECHA_DMI, "ripley")
        .unwrap();
    assert_eq!(
        s.server
            .world()
            .get::<ss13_shared::Appearance>(mech)
            .unwrap()
            .icon_state,
        ripley,
        "occupied"
    );

    // Driving east: the first step turns (it faced south), then 3 tiles; every attempt pays
    // 80 J.
    let full = s.charge(mech);
    for _ in 0..4 {
        s.walk(Dir::East);
    }
    s.run_until_for(10.0, "3 tiles east", |s| {
        s.mech_at(TilePos::new(7, 3, 1)).is_some()
    });
    assert_eq!(s.pos(s.mob), TilePos::new(7, 3, 1));
    assert!(
        (full - s.charge(mech) - 320.0).abs() < 1e-6,
        "{}",
        full - s.charge(mech)
    );

    // The crate ahead (8,3)... is at (9,3): one more step, then the clamp loads it.
    s.walk(Dir::East);
    s.run_until_for(5.0, "next to the crate", |s| {
        s.mech_at(TilePos::new(8, 3, 1)).is_some()
    });
    let crate_ = s.find(CRATE, "/obj/structure/closet/crate").unwrap();
    s.click(crate_);
    s.run_until_for(5.0, "the crate is loaded", |s| {
        s.server
            .world()
            .get::<Mech>(mech)
            .unwrap()
            .cargo
            .contains(&crate_)
    });
    assert!(
        s.server.world().get::<TilePos>(crate_).is_none(),
        "off the map"
    );
    s.run_until_for(5.0, "the pilot is told", |s| s.said("successfully loaded"));

    // Ejecting: resist, 1 s.
    s.send(ClientIntent::Resist);
    s.run_until_for(5.0, "out of the mech", |s| {
        s.server.world().get::<Pilot>(s.mob).is_none()
    });
    assert_eq!(s.pos(s.mob), TilePos::new(8, 3, 1));
    assert!(
        s.server
            .world()
            .get::<Mech>(mech)
            .unwrap()
            .occupant
            .is_none()
    );
}

/// A robotics corner: a cell charger `H` (2,3), the mech bay console `K` (4,3), a Ripley `R`
/// (5,3) on the tile the power port `P` (6,3) faces (west).
const BAY_MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"H" = (/obj/machinery/cell_charger,/turf/open/floor,/area/space)
"K" = (/obj/machinery/computer/mech_bay_power_console,/turf/open/floor,/area/space)
"R" = (/obj/vehicle/sealed/mecha/ripley,/turf/open/floor,/area/space)
"P" = (/obj/machinery/mech_bay_recharge_port{dir = 8},/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwwww
wfsfffw
wHfKRPw
wfffffw
wwwwwww
"}
"##;

#[test]
fn cell_charger_and_mech_bay_charge() {
    use ss13_game::mecha::bay::{MechBayConsole, MechBayPort, RECHARGE_POWER};
    use ss13_game::power::chargers::{CELL_CHARGER_RATE, CellCharger};
    use ss13_tgui::interfaces::mech_bay::{MechBayData, MechBayPowerConsoleUi};

    let mut s = Bay::start_on(
        "cell_charger_and_mech_bay_charge",
        BAY_MAP,
        TilePos::new(3, 4, 1),
    );
    let charger = s
        .find(TilePos::new(2, 3, 1), "/obj/machinery/cell_charger")
        .unwrap();
    let console = s
        .find(
            TilePos::new(4, 3, 1),
            "/obj/machinery/computer/mech_bay_power_console",
        )
        .unwrap();
    let port = s
        .find(
            TilePos::new(6, 3, 1),
            "/obj/machinery/mech_bay_recharge_port",
        )
        .unwrap();
    // `reconnect()`: the port two tiles east links to the console.
    let world = s.server.world();
    assert_eq!(
        world.get::<MechBayConsole>(console).unwrap().port,
        Some(port)
    );
    assert_eq!(
        world.get::<MechBayPort>(port).unwrap().console,
        Some(console)
    );
    assert_eq!(
        world.get::<MechBayPort>(port).unwrap().turf,
        TilePos::new(5, 3, 1)
    );

    // A flat cell into the charger; it gains 2.5 kW x 2 s per machine tick.
    let cell = s.give("/obj/item/stock_parts/power_store/cell/high", 1);
    s.server
        .world_mut()
        .get_mut::<ItemCell>(cell)
        .unwrap()
        .0
        .charge = 0.0;
    s.click(charger);
    s.run_until("the cell is in the charger", |s| {
        s.server
            .world()
            .get::<CellCharger>(charger)
            .unwrap()
            .charging
            == Some(cell)
    });
    s.run_until("told", |s| s.said("You insert a cell into cell charger."));
    let cell_charge = |s: &Bay| s.server.world().get::<ItemCell>(cell).unwrap().0.charge;
    s.run_until("the cell charges", |s| cell_charge(s) > 0.0);
    let start = cell_charge(&s);
    s.run_for(10.0);
    let gained = cell_charge(&s) - start;
    // Five machine ticks in ten seconds (one may fall either side of the window).
    assert!(
        (4.0 * CELL_CHARGER_RATE * 2.0..=6.0 * CELL_CHARGER_RATE * 2.0).contains(&gained),
        "{gained} J in 10 s"
    );
    // An empty hand takes it out.
    s.empty_hands();
    s.click(charger);
    s.run_until("the cell is back in hand", |s| {
        s.server
            .world()
            .get::<Inventory>(s.mob)
            .unwrap()
            .hands
            .contains(&Some(cell))
    });
    assert!(
        s.server
            .world()
            .get::<CellCharger>(charger)
            .unwrap()
            .charging
            .is_none()
    );

    // The Ripley on the port's tile: its emptied cell gains 625 W x 2 s per tick.
    let mech = s.mech_at(TilePos::new(5, 3, 1)).expect("the Ripley");
    let mech_cell = s.server.world().get::<Mech>(mech).unwrap().cell.unwrap();
    s.server
        .world_mut()
        .get_mut::<ItemCell>(mech_cell)
        .unwrap()
        .0
        .charge = 0.0;
    s.run_until("the port charges the mech", |s| s.charge(mech) > 0.0);
    let start = s.charge(mech);
    s.run_for(10.0);
    let gained = s.charge(mech) - start;
    assert!(
        (4.0 * RECHARGE_POWER * 2.0..=6.0 * RECHARGE_POWER * 2.0).contains(&gained),
        "{gained} J in 10 s"
    );

    // The console's window shows the mech and its cell.
    s.empty_hands();
    s.click(console);
    s.run_until("the console window opens", |s| {
        s.window::<MechBayPowerConsoleUi>().is_some()
    });
    let (_, (), data): (u32, (), MechBayData) = s.window::<MechBayPowerConsoleUi>().unwrap();
    let mech_data = data
        .recharge_port
        .unwrap()
        .mech
        .expect("the mech on the port");
    assert_eq!(mech_data.maxhealth, 200.0);
    let cell_data = mech_data.cell.expect("its cell");
    assert_eq!(cell_data.maxcharge, 100_000.0);
    assert!(cell_data.charge > 0.0 && cell_data.charge < 100_000.0);
    assert!(
        s.server
            .world()
            .get::<MechBayConsole>(console)
            .unwrap()
            .charging
    );
}

#[test]
fn hits_and_destruction_into_wreckage() {
    let mut s = Bay::start("hits_and_destruction_into_wreckage");
    let at = TilePos::new(7, 2, 1);
    let mech = s.mech_at(at).expect("the mapped Ripley");
    // `populate_parts`: a full high-capacity cell, tier 1 parts, the cargo compartment.
    assert_eq!(s.charge(mech), 100_000.0);
    assert_eq!(s.server.world().get::<Mech>(mech).unwrap().integrity, 200.0);
    let mob = s.mob;
    s.server
        .world_mut()
        .entity_mut(mob)
        .insert(TilePos::new(6, 2, 1));
    s.run_for(0.15);

    // A 10 force wrench from the west (its side): 40 melee armor, x1.
    s.give("/obj/item/wrench", 1);
    s.click(mech);
    s.run_until_for(5.0, "the hit lands", |s| {
        s.server.world().get::<Mech>(mech).unwrap().integrity < 200.0
    });
    assert!((s.server.world().get::<Mech>(mech).unwrap().integrity - 194.0).abs() < 1e-4);

    // In, then destroyed: the pilot is thrown out, the wreckage keeps the cell.
    s.empty_hands();
    let (from, to) = (s.net_id(s.mob), s.net_id(mech));
    s.send(ClientIntent::Drag {
        from: DragEnd::Atom(from),
        to: DragEnd::Atom(to),
        modifiers: ClickModifiers::default(),
    });
    s.run_until_for(5.0, "in the mech", |s| {
        s.server.world().get::<Pilot>(s.mob).is_some()
    });
    let world = s.st.server.world_mut();
    let taken = ss13_game::mecha::core::take_damage(
        world,
        mech,
        1000.0,
        ss13_game::health::DamageType::Brute,
        "melee",
        None,
    );
    assert!(taken > 100.0);
    assert!(world.get_entity(mech).is_err(), "the mech is gone");
    assert!(world.get::<Pilot>(s.mob).is_none());
    assert_eq!(*world.get::<TilePos>(s.mob).unwrap(), at);
    let wreck = s
        .find(at, "/obj/structure/mecha_wreckage/ripley")
        .expect("a wreckage");
    let salvage = s
        .server
        .world()
        .get::<ss13_game::mecha::core::Wreckage>(wreck)
        .unwrap()
        .crowbar_salvage
        .clone();
    assert!(!salvage.is_empty(), "the cell");
    // tg's 2 s sleep: paralyzed, then a crowbar pries the cell out.
    s.run_until_for(5.0, "asleep", |s| {
        ss13_game::status_effects::incapacitation(s.server.world(), s.mob).incapacitated
    });
    s.run_until_for(5.0, "awake", |s| {
        !ss13_game::status_effects::incapacitation(s.server.world(), s.mob).incapacitated
    });
    s.give("/obj/item/crowbar", 1);
    s.click(wreck);
    s.run_until_for(5.0, "salvaged", |s| {
        s.server
            .world()
            .get::<ss13_game::mecha::core::Wreckage>(wreck)
            .unwrap()
            .crowbar_salvage
            .len()
            < salvage.len()
    });
}
