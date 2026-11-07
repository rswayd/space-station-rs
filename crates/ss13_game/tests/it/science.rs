//! Research over real WebTransport on 127.0.0.1, on a fixture lab: the R&D server earns points;
//! the (locked) R&D console refuses everything but unlocking, and unlocking needs research
//! access; a node is researched for its points (the science budget gets the bounty); iron and
//! glass go into the protolathe (the ore silo's storage); the researched design prints two
//! wrenches for the materials; the silo refuses an ID without a bank account; the destructive
//! analyzer gives points for a point item once and unhides a node for another.

use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::cargo::economy::{ACCOUNT_SCI, BankAccountId, Economy};
use ss13_game::inventory::{IdCard, Inventory};
use ss13_game::science::analyzer::Analyzer;
use ss13_game::science::console::RdConsole;
use ss13_game::science::experi_scanner::{DestructiveScanner, ExperimentHandler};
use ss13_game::science::lathe::{Lathe, MaterialStore, OreSilo};
use ss13_game::science::points::ResearchServer;
use ss13_game::science::server_control::ServerControl;
use ss13_game::science::techweb::{POINT_GENERIC, ResearchDb, Techweb};
use ss13_game::science::{
    AnalyzerData, DestructiveAnalyzerUi, ExperimentConfigureData, ExperimentConfigureUi,
    FabricatorData, FabricatorStatic, FabricatorUi, ServerControlData, ServerControlUi,
    TechwebData, TechwebStatic, TechwebUi,
};
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_game::tools::Stack;
use ss13_mapload::convert::DmTypes;
use ss13_mapload::{DmItem, MapLoadPlugin};
use ss13_net::chat::client::ChatReceived;
use ss13_net::ui::client::UiMessageReceived;
use ss13_shared::protocol::{ClientIntent, InventorySlot};
use ss13_shared::ui::UiServerMessage;
use ss13_shared::{Dir, TilePos};
use ss13_tgui::{UiInterface, decode};
use ss13_world::SpatialIndex;

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/datum/controller": { "parent": "/datum" },
    "/datum/controller/subsystem": { "parent": "/datum/controller" },
    "/datum/controller/subsystem/research": { "parent": "/datum/controller/subsystem",
        "declared_vars": {
            "point_types": { "list": [ { "assoc": [ "General Research", "Gen. Res." ] } ] },
            "single_server_income": { "list": [ { "assoc": [ "General Research", 1 ] } ] },
            "techweb_point_items": { "list": [ { "assoc": [ { "path": "/obj/item/screwdriver" },
                { "list": [ { "assoc": [ "General Research", 200 ] } ] } ] } ] } } },
    "/datum/controller/subsystem/economy": { "parent": "/datum/controller/subsystem",
        "declared_vars": { "techweb_bounty": 250 } },
    "/datum/job_department": { "parent": "/datum", "declared_vars": {
        "department_bitflags": 0, "department_name": "" } },
    "/datum/job_department/science": { "parent": "/datum/job_department", "declared_vars": {
        "department_bitflags": 32, "department_name": "Science" } },
    "/datum/material": { "parent": "/datum", "declared_vars": {
        "abstract_type": { "path": "/datum/material" }, "mat_flags": 0, "name": "material",
        "sheet_type": null, "ore_type": null, "color": null } },
    "/datum/material/iron": { "parent": "/datum/material", "declared_vars": { "mat_flags": 23,
        "name": "iron", "color": "#B6BEC2",
        "sheet_type": { "path": "/obj/item/stack/sheet/iron" } } },
    "/datum/material/glass": { "parent": "/datum/material", "declared_vars": { "mat_flags": 23,
        "name": "glass", "color": "#88CCEE",
        "sheet_type": { "path": "/obj/item/stack/sheet/glass" } } },
    "/datum/design": { "parent": "/datum", "declared_vars": { "name": null, "desc": null,
        "build_type": null, "build_path": null, "materials": { "list": [] },
        "category": { "list": [] }, "departmental_flags": 16777215,
        "construction_time": 32, "lathe_time_factor": 1, "fixed_cost_efficiency": 0 } },
    "/datum/design/wrench": { "parent": "/datum/design", "declared_vars": {
        "name": "Wrench", "build_type": 2, "build_path": { "path": "/obj/item/wrench" },
        "materials": { "list": [ { "assoc": [ { "path": "/datum/material/iron" }, 150 ] },
            { "assoc": [ { "path": "/datum/material/glass" }, 50 ] } ] },
        "category": { "list": [ "/Tools" ] } } },
    "/datum/techweb_node": { "parent": "/datum", "declared_vars": { "display_name": null,
        "description": null, "category": "Misc", "node_flags": 8,
        "prerequisite_nodes": null, "unlocked_designs": null, "research_costs": null,
        "required_experiments": null, "discount_experiments": null,
        "required_items_to_unlock": null, "announce_channels": null } },
    "/datum/techweb_node/base": { "parent": "/datum/techweb_node", "declared_vars": {
        "display_name": "Base", "node_flags": 9 } },
    "/datum/techweb_node/tools": { "parent": "/datum/techweb_node", "declared_vars": {
        "display_name": "Tools", "description": "Wrenches.",
        "prerequisite_nodes": { "list": [ { "path": "/datum/techweb_node/base" } ] },
        "unlocked_designs": { "list": [ { "path": "/datum/design/wrench" } ] },
        "research_costs": { "list": [ { "assoc": [ "General Research", 40 ] } ] } } },
    "/datum/techweb_node/secret": { "parent": "/datum/techweb_node", "declared_vars": {
        "display_name": "Secret", "node_flags": 10,
        "prerequisite_nodes": { "list": [ { "path": "/datum/techweb_node/base" } ] },
        "required_items_to_unlock": { "list": [ { "path": "/obj/item/screwdriver" } ] },
        "research_costs": { "list": [ { "assoc": [ "General Research", 40 ] } ] } } },
    "/datum/techweb_node/gated": { "parent": "/datum/techweb_node", "declared_vars": {
        "display_name": "Gated",
        "prerequisite_nodes": { "list": [ { "path": "/datum/techweb_node/base" } ] },
        "required_experiments": { "list": [ { "path": "/datum/experiment/scanning/wrenches" } ] },
        "discount_experiments": { "list": [ { "assoc": [
            { "path": "/datum/experiment/scanning/shred" }, 20 ] } ] },
        "research_costs": { "list": [ { "assoc": [ "General Research", 40 ] } ] } } },
    "/datum/experiment": { "parent": "/datum", "declared_vars": { "name": "Experiment",
        "description": "Base", "exp_tag": "Base", "traits": null,
        "allowed_experimentors": null, "performance_hint": null, "points_reward": null } },
    "/datum/experiment/scanning": { "parent": "/datum/experiment", "declared_vars": {
        "exp_tag": "Scan", "required_atoms": { "list": [] }, "scan_message": null,
        "allowed_experimentors": { "list": [ { "path": "/obj/item/experi_scanner" },
            { "path": "/obj/machinery/destructive_scanner" } ] } } },
    "/datum/experiment/scanning/wrenches": { "parent": "/datum/experiment/scanning",
        "declared_vars": { "name": "Wrench Scan", "required_atoms": { "list": [
            { "assoc": [ { "path": "/obj/item/wrench" }, 2 ] } ] } } },
    "/datum/experiment/scanning/shred": { "parent": "/datum/experiment/scanning",
        "declared_vars": { "name": "Screwdriver Shredding", "traits": 1,
            "required_atoms": { "list": [
                { "assoc": [ { "path": "/obj/item/screwdriver" }, 1 ] } ] } } },
    "/datum/stock_part": { "parent": "/datum" },
    "/datum/stock_part/servo": { "parent": "/datum/stock_part" },
    "/datum/stock_part/matter_bin": { "parent": "/datum/stock_part" },
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
    "/obj/machinery": { "parent": "/obj", "declared_vars": { "anchored": 1, "density": 1,
        "use_power": 1, "idle_power_usage": 100, "active_power_usage": 500,
        "power_channel": 1 } },
    "/obj/machinery/computer": { "parent": "/obj/machinery", "declared_vars": {
        "icon": { "resource": "c.dmi" }, "icon_state": "computer" } },
    "/obj/machinery/computer/rdconsole": { "parent": "/obj/machinery/computer",
        "declared_vars": { "name": "R&D Console", "req_access": { "list": [ "research" ] },
            "circuit": { "path": "/obj/item/circuitboard/computer/rdconsole" } } },
    "/obj/machinery/computer/rdservercontrol": { "parent": "/obj/machinery/computer",
        "declared_vars": { "name": "\\improper R&D server control console" } },
    "/obj/machinery/rnd": { "parent": "/obj/machinery", "declared_vars": {
        "icon": { "resource": "r.dmi" }, "icon_state": "server" } },
    "/obj/machinery/rnd/server": { "parent": "/obj/machinery/rnd", "declared_vars": {
        "name": "\\improper R&D Server" } },
    "/obj/machinery/rnd/destructive_analyzer": { "parent": "/obj/machinery/rnd",
        "declared_vars": { "name": "destructive analyzer", "icon_state": "d_analyzer" } },
    "/obj/machinery/rnd/production": { "parent": "/obj/machinery/rnd", "declared_vars": {
        "allowed_buildtypes": 0, "allowed_department_flags": 16777215,
        "payment_department": null } },
    "/obj/machinery/rnd/production/protolathe": { "parent": "/obj/machinery/rnd/production",
        "declared_vars": { "name": "protolathe", "icon_state": "protolathe",
            "allowed_buildtypes": 2,
            "circuit": { "path": "/obj/item/circuitboard/machine/protolathe" } } },
    "/obj/machinery/gizmo": { "parent": "/obj/machinery", "declared_vars": {
        "name": "gizmo", "icon": { "resource": "r.dmi" }, "icon_state": "server",
        "anchored": 0 } },
    "/obj/machinery/rnd/experimentor": { "parent": "/obj/machinery/rnd", "declared_vars": {
        "name": "\\improper E.X.P.E.R.I-MENTOR", "icon_state": "server" } },
    "/obj/machinery/destructive_scanner": { "parent": "/obj/machinery", "declared_vars": {
        "name": "experimental destructive scanner", "icon": { "resource": "r.dmi" },
        "icon_state": "tube_open" } },
    "/obj/machinery/ore_silo": { "parent": "/obj/machinery", "declared_vars": {
        "name": "ore silo", "icon": { "resource": "r.dmi" }, "icon_state": "silo" } },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null, "w_class": 3,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null } },
    "/obj/item/wrench": { "parent": "/obj/item", "declared_vars": { "name": "wrench",
        "icon_state": "wrench" } },
    "/obj/item/screwdriver": { "parent": "/obj/item", "declared_vars": {
        "name": "screwdriver", "icon_state": "screwdriver" } },
    "/obj/item/experi_scanner": { "parent": "/obj/item", "declared_vars": {
        "name": "Experi-Scanner", "icon_state": "experiscanner" } },
    "/obj/item/disk": { "parent": "/obj/item" },
    "/obj/item/assembly": { "parent": "/obj/item" },
    "/obj/item/assembly/relic": { "parent": "/obj/item/assembly", "declared_vars": {
        "name": "strange object" } },
    "/obj/item/disk/tech_disk": { "parent": "/obj/item/disk", "declared_vars": {
        "name": "technology disk" } },
    "/obj/item/disk/design_disk": { "parent": "/obj/item/disk", "declared_vars": {
        "name": "Component Design Disk" } },
    "/obj/item/circuitboard": { "parent": "/obj/item" },
    "/obj/item/circuitboard/computer": { "parent": "/obj/item/circuitboard" },
    "/obj/item/circuitboard/computer/rdconsole": { "parent": "/obj/item/circuitboard/computer",
        "declared_vars": { "locked": 1 } },
    "/obj/item/circuitboard/machine": { "parent": "/obj/item/circuitboard" },
    "/obj/item/circuitboard/machine/protolathe": { "parent": "/obj/item/circuitboard/machine",
        "declared_vars": { "req_components": { "list": [
            { "assoc": [ { "path": "/datum/stock_part/matter_bin" }, 2 ] },
            { "assoc": [ { "path": "/datum/stock_part/servo" }, 2 ] } ] } } },
    "/obj/item/stack": { "parent": "/obj/item", "declared_vars": { "amount": 1,
        "max_amount": 50, "merge_type": null, "singular_name": null, "mats_per_unit": null } },
    "/obj/item/stack/sheet": { "parent": "/obj/item/stack" },
    "/obj/item/stack/sheet/iron": { "parent": "/obj/item/stack/sheet", "declared_vars": {
        "name": "iron", "icon_state": "sheet-metal", "singular_name": "iron sheet",
        "merge_type": { "path": "/obj/item/stack/sheet/iron" },
        "mats_per_unit": { "list": [ { "assoc": [ { "path": "/datum/material/iron" }, 100 ] } ] } } },
    "/obj/item/stack/sheet/glass": { "parent": "/obj/item/stack/sheet", "declared_vars": {
        "name": "glass", "icon_state": "sheet-glass", "singular_name": "glass sheet",
        "merge_type": { "path": "/obj/item/stack/sheet/glass" },
        "mats_per_unit": { "list": [ { "assoc": [ { "path": "/datum/material/glass" }, 100 ] } ] } } },
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
    "/turf": { "parent": "/atom", "declared_vars": { "icon": { "resource": "t.dmi" },
        "blocks_air": 0, "can_atmos_pass": 0, "initial_gas_mix": "o2=22;n2=82;TEMP=293.15",
        "temperature": 293.15, "baseturfs": { "path": "/turf/baseturf_bottom" } } },
    "/turf/open": { "parent": "/turf", "declared_vars": { "can_atmos_pass": -1,
        "planetary_atmos": 0 } },
    "/turf/open/floor": { "parent": "/turf/open", "declared_vars": { "icon_state": "floor",
        "baseturfs": { "path": "/turf/open/floor/plating" } } },
    "/turf/open/floor/plating": { "parent": "/turf/open/floor", "declared_vars": {
        "icon_state": "plating", "baseturfs": { "path": "/turf/baseturf_bottom" } } },
    "/turf/closed": { "parent": "/turf", "declared_vars": { "density": 1, "opacity": 1,
        "blocks_air": 1 } },
    "/turf/closed/wall": { "parent": "/turf/closed", "declared_vars": { "icon_state": "wall",
        "baseturfs": { "path": "/turf/open/floor/plating" } } },
    "/area": { "parent": "/atom" },
    "/area/station": { "parent": "/area", "declared_vars": { "name": "Лаборатория" } }
  }
}"##;

/// Rows top-down, the last line is y = 1. The player spawns at `s` (4,4) next to the console
/// `C` (3,3), the protolathe `L` (4,3) and the destructive analyzer `A` (5,3); the destructive
/// scanner `D` (6,4), the server `R` and the ore silo `O` are further away. Dense machines on
/// both sides of a diagonal block reaching across it, so `D` stays off (3,4) and (5,4).
const MAP: &str = r##""w" = (/turf/closed/wall,/area/station)
"f" = (/turf/open/floor,/area/station)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/station)
"C" = (/obj/machinery/computer/rdconsole,/turf/open/floor,/area/station)
"L" = (/obj/machinery/rnd/production/protolathe,/turf/open/floor,/area/station)
"A" = (/obj/machinery/rnd/destructive_analyzer,/turf/open/floor,/area/station)
"R" = (/obj/machinery/rnd/server,/turf/open/floor,/area/station)
"O" = (/obj/machinery/ore_silo,/turf/open/floor,/area/station)
"D" = (/obj/machinery/destructive_scanner,/turf/open/floor,/area/station)

(1,1,1) = {"
wwwwwwwww
wffsfDfOw
wfCLAffRw
wfffffffw
wwwwwwwww
"}
"##;

const SPAWN: TilePos = TilePos::new(4, 4, 1);
const CONSOLE: TilePos = TilePos::new(3, 3, 1);
const LATHE: TilePos = TilePos::new(4, 3, 1);
const ANALYZER: TilePos = TilePos::new(5, 3, 1);
const TOOLS: &str = "/datum/techweb_node/tools";
const SECRET: &str = "/datum/techweb_node/secret";
const GATED: &str = "/datum/techweb_node/gated";
const DESTRUCTIVE_SCANNER: TilePos = TilePos::new(6, 4, 1);
const WRENCH_SCAN: &str = "/datum/experiment/scanning/wrenches";
const SHRED: &str = "/datum/experiment/scanning/shred";

const STATES: [(&str, &str); 17] = [
    ("t.dmi", "floor"),
    ("t.dmi", "plating"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "card_grey"),
    ("i.dmi", "toy"),
    ("i.dmi", "wrench"),
    ("i.dmi", "screwdriver"),
    ("i.dmi", "sheet-metal"),
    ("i.dmi", "sheet-glass"),
    ("c.dmi", "computer"),
    ("r.dmi", "server"),
    ("r.dmi", "d_analyzer"),
    ("r.dmi", "protolathe"),
    ("r.dmi", "tube_open"),
    ("i.dmi", "experiscanner"),
];

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("science_fixture")
        .join(test);
    let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
    let research = ss13_mapload::research::resolve(&types);
    let mut used = vec![
        "/obj/effect/landmark/start/assistant",
        "/obj/machinery/computer/rdconsole",
        "/obj/machinery/rnd/production/protolathe",
        "/obj/machinery/rnd/destructive_analyzer",
        "/obj/machinery/rnd/server",
        "/obj/machinery/ore_silo",
        "/obj/item/screwdriver",
        "/obj/item/experi_scanner",
        "/obj/item/disk/tech_disk",
        "/obj/item/assembly/relic",
        "/obj/machinery/rnd/experimentor",
        "/obj/machinery/gizmo",
        "/obj/item/wrench",
        "/obj/item/disk/design_disk",
        "/obj/machinery/destructive_scanner",
        "/obj/machinery/computer/rdservercontrol",
        "/turf/open/floor",
        "/turf/open/floor/plating",
        "/turf/closed/wall",
        "/area/station",
    ];
    used.extend(research.prototype_types());
    write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)]);
    std::fs::write(
        dir.join(ss13_mapload::research::RESEARCH_FILE),
        research.to_ron(),
    )
    .unwrap();
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

struct Lab(testing::Station);

impl Deref for Lab {
    type Target = testing::Station;
    fn deref(&self) -> &testing::Station {
        &self.0
    }
}

impl DerefMut for Lab {
    fn deref_mut(&mut self) -> &mut testing::Station {
        &mut self.0
    }
}

impl Drive for Lab {
    fn station(&mut self) -> &mut testing::Station {
        &mut self.0
    }

    fn context(&self) -> String {
        format!("; chat: {:?}", self.client().world().resource::<Chat>().0)
    }
}

impl Lab {
    fn start(test: &str) -> Self {
        let assets = fixture_assets(test);
        let s = Self::start_with(test, fixture_map(&assets, &["map.dmm"]));
        assert_eq!(*s.server.world().get::<TilePos>(s.mob()).unwrap(), SPAWN);
        s
    }

    fn start_with(test: &str, map: MapLoadPlugin) -> Self {
        let mut s = testing::Station::start(map, |_| {});
        s.join(test, |app| {
            app.init_resource::<Chat>()
                .init_resource::<Windows>()
                .add_systems(Update, record);
        });
        Self(s)
    }

    fn mob(&self) -> Entity {
        self.players[0].mob
    }

    fn client(&self) -> &App {
        &self.players[0].app
    }

    fn steps(&mut self, n: usize) {
        for _ in 0..n {
            self.step();
        }
    }

    fn said(&self, text: &str) -> bool {
        self.client()
            .world()
            .resource::<Chat>()
            .0
            .iter()
            .any(|l| l.contains(text))
    }

    fn machine<T: Component>(&self, pos: TilePos) -> Entity {
        let world = self.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .find(|&e| world.get::<T>(e).is_some())
            .unwrap()
    }

    fn click(&mut self, entity: Entity) {
        self.run_until_for(10.0, "the target is replicated", |s| {
            s.net_id(0, entity).is_some()
        });
        self.0.click(0, entity);
    }

    fn window<T: UiInterface>(&self) -> Option<(u32, T::StaticData, T::Data)> {
        let windows = self.client().world().resource::<Windows>();
        let (_, id, st, data) = windows.0.iter().find(|w| w.0 == T::NAME)?;
        Some((*id, decode(st).unwrap(), decode(data).unwrap()))
    }

    fn open<T: UiInterface>(&mut self, machine: Entity) {
        self.click(machine);
        self.run_until_for(10.0, &format!("{} opens", T::NAME), |s| {
            s.window::<T>().is_some()
        });
    }

    fn act<T: UiInterface>(&mut self, action: &str, params: &[(&str, &str)]) {
        let (window, ..) = self.window::<T>().expect("window");
        self.0.act(0, window, action, params);
    }

    fn web(&self) -> &Techweb {
        self.server.world().resource::<Techweb>()
    }

    fn node(&self, path: &str) -> usize {
        self.server.world().resource::<ResearchDb>().node_index[path]
    }

    fn card(&self) -> Entity {
        self.server
            .world()
            .get::<Inventory>(self.mob())
            .unwrap()
            .id
            .expect("ID card")
    }

    /// Spawns `path` (a stack of `amount` when > 1) into the empty active hand.
    fn give(&mut self, path: &str, amount: u32) -> Entity {
        let mob = self.mob();
        let world = self.server.world_mut();
        let pos = *world.get::<TilePos>(mob).unwrap();
        let item = ss13_game::construction::spawn_obj(world, path, pos).unwrap();
        if amount > 1 {
            world.entity_mut(item).insert(Stack { amount });
        }
        let hand = world.get::<Inventory>(mob).unwrap().active;
        world
            .entity_mut(item)
            .remove::<TilePos>()
            .insert(ss13_game::inventory::CarriedBy(mob));
        world.get_mut::<Inventory>(mob).unwrap().hands[hand] = Some(item);
        item
    }

    fn held(&self) -> Option<Entity> {
        self.server
            .world()
            .get::<Inventory>(self.mob())
            .unwrap()
            .active_item()
    }

    fn silo_store(&self) -> MaterialStore {
        let world = self.server.world();
        let lathe = world.get::<Lathe>(self.machine::<Lathe>(LATHE)).unwrap();
        world
            .get::<MaterialStore>(lathe.silo.expect("linked to the silo"))
            .unwrap()
            .clone()
    }

    fn items_on(&self, pos: TilePos, path: &str) -> usize {
        let world = self.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .filter(|&&e| world.get::<DmItem>(e).is_some_and(|i| i.path == path))
            .count()
    }
}

#[test]
fn research_insert_print_and_analyze() {
    let mut s = Lab::start("research_insert_print_and_analyze");

    // The server earns 1 point per second.
    let tools = s.node(TOOLS);
    assert!(s.web().available.contains(&tools));
    s.run_until_for(10.0, "points", |s| s.web().points_of(POINT_GENERIC) >= 2.0);

    // The console is locked: research is refused, unlocking needs research access.
    let console = s.machine::<RdConsole>(CONSOLE);
    s.open::<TechwebUi>(console);
    let (_, st, data): (u32, TechwebStatic, TechwebData) = s.window::<TechwebUi>().unwrap();
    assert!(data.locked && data.stored_research);
    let tools_info = st.nodes.iter().position(|n| n.path == TOOLS).unwrap();
    assert_eq!(
        st.nodes[tools_info].costs,
        vec![(POINT_GENERIC.to_owned(), 40.0)]
    );
    assert_eq!(st.designs.len(), 1);
    assert_eq!(st.designs[0].name, "Wrench");
    assert_eq!(
        st.designs[0].cost,
        vec![("iron".to_owned(), 150), ("glass".to_owned(), 50)]
    );
    s.server
        .world_mut()
        .resource_scope(|world, mut web: Mut<Techweb>| {
            web.adjust_points(world.resource::<ResearchDb>(), POINT_GENERIC, 100.0);
        });
    s.act::<TechwebUi>("researchNode", &[("node_path", TOOLS)]);
    s.run_until_for(5.0, "locked refusal", |s| {
        s.said("Консоль заблокирована. Дальнейшие действия невозможны.")
    });
    assert!(!s.web().researched.contains(&tools));
    s.act::<TechwebUi>("toggleLock", &[]);
    s.run_until_for(5.0, "access refusal", |s| {
        s.said("Несанкционированный доступ.")
    });
    let card = s.card();
    s.server.world_mut().entity_mut(card).insert(IdCard {
        access: vec!["research".into()],
    });
    s.act::<TechwebUi>("toggleLock", &[]);
    s.run_until_for(5.0, "unlocked", |s| {
        s.window::<TechwebUi>().is_some_and(|(_, _, d)| !d.locked)
    });

    // Researching: 40 points, the design, the science budget's bounty.
    let budget = s
        .server
        .world()
        .resource::<Economy>()
        .department(ACCOUNT_SCI)
        .unwrap()
        .balance;
    let points = s.web().points_of(POINT_GENERIC);
    s.act::<TechwebUi>("researchNode", &[("node_path", SECRET)]);
    s.act::<TechwebUi>("researchNode", &[("node_path", TOOLS)]);
    s.run_until_for(5.0, "researched", |s| s.said("Успешно исследовано Tools."));
    assert!(s.web().researched.contains(&tools));
    assert!(!s.web().researched.contains(&s.node(SECRET)), "hidden");
    let left = s.web().points_of(POINT_GENERIC);
    assert!(
        left < points - 39.0 && left > points - 41.0,
        "{points} -> {left}"
    );
    let budget_now = s
        .server
        .world()
        .resource::<Economy>()
        .department(ACCOUNT_SCI)
        .unwrap()
        .balance;
    assert_eq!(budget_now, budget + 250);
    s.run_until_for(5.0, "the window shows it researched", |s| {
        s.window::<TechwebUi>()
            .is_some_and(|(_, _, d)| d.node(tools_info as u16).is_some_and(|n| n.tier == 0))
    });

    // Iron and glass into the protolathe go to the silo.
    let lathe = s.machine::<Lathe>(LATHE);
    assert!(
        s.server
            .world()
            .get::<OreSilo>(s.silo_store_entity())
            .is_some()
    );
    s.give("/obj/item/stack/sheet/iron", 10);
    s.click(lathe);
    s.run_until_for(5.0, "iron inserted", |s| {
        s.said("10 iron sheets were added to protolathe.")
    });
    assert!(s.held().is_none(), "the whole stack went in");
    s.give("/obj/item/stack/sheet/glass", 5);
    s.click(lathe);
    s.run_until_for(5.0, "glass inserted", |s| s.held().is_none());
    // Materials are in type path order: glass, iron.
    assert_eq!(s.silo_store().amounts, vec![500, 1000]);

    // The Fabricator: the researched wrench, printed twice (150 iron, 50 glass each).
    s.open::<FabricatorUi>(lathe);
    let (_, st, data): (u32, FabricatorStatic, FabricatorData) =
        s.window::<FabricatorUi>().unwrap();
    assert_eq!(st.designs.len(), 1);
    assert_eq!(st.designs[0].cost, vec![(1, 150), (0, 50)]);
    assert_eq!(data.researched, vec![0]);
    assert_eq!(data.materials, vec![500, 1000]);
    assert_eq!(data.material_maximum, 7500);
    s.act::<FabricatorUi>(
        "build",
        &[("design_path", "/datum/design/nope"), ("amount", "1")],
    );
    s.act::<FabricatorUi>(
        "build",
        &[("design_path", "/datum/design/wrench"), ("amount", "2")],
    );
    s.run_until_for(5.0, "busy", |s| {
        s.server.world().get::<Lathe>(lathe).unwrap().busy
    });
    s.run_until_for(10.0, "two wrenches", |s| {
        s.items_on(LATHE, "/obj/item/wrench") == 2
    });
    s.run_until_for(5.0, "done", |s| {
        !s.server.world().get::<Lathe>(lathe).unwrap().busy
    });
    assert_eq!(s.silo_store().amounts, vec![400, 700]);

    // The station silo refuses an ID without a bank account.
    s.server
        .world_mut()
        .entity_mut(card)
        .remove::<BankAccountId>();
    s.act::<FabricatorUi>(
        "build",
        &[("design_path", "/datum/design/wrench"), ("amount", "1")],
    );
    s.run_until_for(5.0, "silo refusal", |s| {
        s.said("SILO ERR: No account ID found. Please contact Head of Personnel.")
    });
    s.steps(20);
    assert_eq!(s.items_on(LATHE, "/obj/item/wrench"), 2);

    // The destructive analyzer: points for a point item once, a hidden node for another.
    let analyzer = s.machine::<Analyzer>(ANALYZER);
    s.give("/obj/item/screwdriver", 1);
    s.click(analyzer);
    s.run_until_for(5.0, "loaded", |s| {
        s.server
            .world()
            .get::<Analyzer>(analyzer)
            .unwrap()
            .loaded
            .is_some()
    });
    s.open::<DestructiveAnalyzerUi>(analyzer);
    let (_, (), data): (u32, (), AnalyzerData) = s.window::<DestructiveAnalyzerUi>().unwrap();
    assert_eq!(data.loaded_item.as_deref(), Some("screwdriver"));
    assert_eq!(data.recoverable_points, "Gen. Res.: 200");
    assert_eq!(data.node_data.len(), 1);
    assert!(data.node_data[0].node_hidden);
    let before = s.web().points_of(POINT_GENERIC);
    s.act::<DestructiveAnalyzerUi>("deconstruct", &[]);
    s.run_until_for(5.0, "destroyed", |s| {
        s.server
            .world()
            .get::<Analyzer>(analyzer)
            .unwrap()
            .loaded
            .is_none()
    });
    let gained = s.web().points_of(POINT_GENERIC) - before;
    assert!((200.0..203.0).contains(&gained), "gained {gained}");
    // Wait out the 2.4 s busy time, then the second one unhides the node.
    s.run_until_for(10.0, "not busy", |s| {
        let busy_until = s
            .server
            .world()
            .get::<Analyzer>(analyzer)
            .unwrap()
            .busy_until;
        s.server.world().resource::<Time>().elapsed_secs_f64() > busy_until
    });
    s.give("/obj/item/screwdriver", 1);
    s.click(analyzer);
    s.run_until_for(5.0, "loaded again", |s| {
        s.server
            .world()
            .get::<Analyzer>(analyzer)
            .unwrap()
            .loaded
            .is_some()
    });
    s.run_until_for(5.0, "already deconstructed", |s| {
        s.window::<DestructiveAnalyzerUi>()
            .is_some_and(|(_, _, d)| d.already_deconstructed)
    });
    let before = s.web().points_of(POINT_GENERIC);
    s.act::<DestructiveAnalyzerUi>("deconstruct", &[("deconstruct_path", SECRET)]);
    let secret = s.node(SECRET);
    s.run_until_for(5.0, "unhidden", |s| !s.web().hidden.contains(&secret));
    assert!(s.web().available.contains(&secret));
    assert!(
        s.web().points_of(POINT_GENERIC) - before < 5.0,
        "no points twice"
    );
}

/// Experiments on the fixture lab: the destructive scanner shreds a screwdriver for the gated
/// node's discount experiment; the experi-scanner scans two wrenches for its required one, which
/// then lets the node be researched.
#[test]
fn experiments_scan_and_unlock() {
    let mut s = Lab::start("experiments_scan_and_unlock");
    let gated = s.node(GATED);
    // Both experiments came with the node becoming reachable; the node waits for its scan.
    assert!(s.web().experiment(WRENCH_SCAN).is_some());
    assert!(s.web().experiment(SHRED).is_some());
    s.server
        .world_mut()
        .resource_scope(|world, mut web: Mut<Techweb>| {
            web.adjust_points(world.resource::<ResearchDb>(), POINT_GENERIC, 1000.0);
        });
    let gated_now = |s: &Lab| {
        let world = s.server.world();
        let db = world.resource::<ResearchDb>();
        let web = world.resource::<Techweb>();
        (
            web.have_experiments_for_node(db, gated),
            web.price(db, gated),
        )
    };
    assert_eq!(
        gated_now(&s),
        (false, vec![(POINT_GENERIC.to_owned(), 40.0)])
    );

    // The destructive scanner: a screwdriver under the tube, the shredding experiment selected.
    // One step east to stand next to it.
    let mob = s.mob();
    s.run_until_for(5.0, "next to the scanner", |s| {
        if *s.server.world().get::<TilePos>(mob).unwrap() == TilePos::new(5, 4, 1) {
            return true;
        }
        s.try_step(0, Dir::East);
        false
    });
    let scanner = s.machine::<DestructiveScanner>(DESTRUCTIVE_SCANNER);
    let screwdriver = ss13_game::construction::spawn_obj(
        s.server.world_mut(),
        "/obj/item/screwdriver",
        DESTRUCTIVE_SCANNER,
    )
    .unwrap();
    s.open::<ExperimentConfigureUi>(scanner);
    let (_, (), data): (u32, (), ExperimentConfigureData) =
        s.window::<ExperimentConfigureUi>().unwrap();
    assert!(data.has_start_callback);
    assert_eq!(data.techwebs.len(), 1);
    assert!(data.techwebs[0].selected);
    assert_eq!(data.experiments.len(), 2, "{:?}", data.experiments);
    let shred = data.experiments.iter().find(|e| e.path == SHRED).unwrap();
    assert_eq!(shred.tag, "Destructive Scan");
    s.act::<ExperimentConfigureUi>("select_experiment", &[("ref", SHRED)]);
    s.run_until_for(5.0, "shredding selected", |s| {
        s.server
            .world()
            .get::<ExperimentHandler>(scanner)
            .unwrap()
            .selected
            .as_deref()
            == Some(SHRED)
    });
    s.act::<ExperimentConfigureUi>("start_experiment_callback", &[]);
    s.run_until_for(5.0, "the tube closes", |s| {
        s.server
            .world()
            .get::<DestructiveScanner>(scanner)
            .unwrap()
            .cycle
            .is_some()
    });
    assert!(
        s.server.world().get::<TilePos>(screwdriver).is_none(),
        "the screwdriver is inside"
    );
    s.run_until_for(15.0, "shredded", |s| {
        s.web().completed_experiments.contains(SHRED)
    });
    assert!(
        s.server.world().get_entity(screwdriver).is_err(),
        "destroyed"
    );
    s.run_until_for(5.0, "announced", |s| {
        s.said("Screwdriver Shredding has been completed!")
    });
    assert_eq!(
        gated_now(&s),
        (false, vec![(POINT_GENERIC.to_owned(), 20.0)]),
        "discounted, still gated"
    );

    // The experi-scanner: its window lists only the non-destructive experiment.
    let handheld = s.give("/obj/item/experi_scanner", 1);
    let slot = if s.server.world().get::<Inventory>(mob).unwrap().active == 0 {
        InventorySlot::LeftHand
    } else {
        InventorySlot::RightHand
    };
    s.send(0, ClientIntent::ClickSlot(slot));
    s.run_until_for(5.0, "the scanner's window", |s| {
        s.window::<ExperimentConfigureUi>()
            .is_some_and(|(_, _, d)| !d.has_start_callback)
    });
    let (_, (), data): (u32, (), ExperimentConfigureData) =
        s.window::<ExperimentConfigureUi>().unwrap();
    assert_eq!(data.experiments.len(), 1);
    assert_eq!(data.experiments[0].path, WRENCH_SCAN);
    s.act::<ExperimentConfigureUi>("select_experiment", &[("ref", WRENCH_SCAN)]);
    s.run_until_for(5.0, "wrench scan selected", |s| {
        s.server
            .world()
            .get::<ExperimentHandler>(handheld)
            .unwrap()
            .selected
            .is_some()
    });

    // Unrelated things are refused; each wrench counts once, after a second.
    let next_to = TilePos::new(4, 4, 1);
    let spawn = |s: &mut Lab, path: &str| {
        ss13_game::construction::spawn_obj(s.server.world_mut(), path, next_to).unwrap()
    };
    let other = spawn(&mut s, "/obj/item/screwdriver");
    s.click(other);
    s.run_until_for(5.0, "not related", |s| {
        s.said("Screwdriver не имеет отношения к выбранному эксперименту.")
    });
    let first = spawn(&mut s, "/obj/item/wrench");
    s.click(first);
    s.run_until_for(5.0, "first scan", |s| s.said("Вы сканируете wrench."));
    assert!(
        s.web()
            .experiment(WRENCH_SCAN)
            .is_some_and(|e| e.scanned[0].seen.len() == 1)
    );
    let second = spawn(&mut s, "/obj/item/wrench");
    s.click(second);
    s.run_until_for(5.0, "completed", |s| {
        s.web().completed_experiments.contains(WRENCH_SCAN)
    });
    assert!(
        s.server.world().get_entity(second).is_ok(),
        "handheld scans destroy nothing"
    );
    assert!(gated_now(&s).0, "the required experiment is done");
    let researched = s
        .server
        .world_mut()
        .resource_scope(|world, mut web: Mut<Techweb>| {
            web.research_node(world.resource::<ResearchDb>(), gated, false, true)
        });
    assert!(researched);
}

/// The server control console: servers and consoles listed, toggling needs RD access, a
/// disabled server stops the points.
#[test]
fn server_control_toggles_servers() {
    let mut s = Lab::start("server_control_toggles_servers");
    let console = ss13_game::construction::spawn_obj(
        s.server.world_mut(),
        "/obj/machinery/computer/rdservercontrol",
        TilePos::new(5, 4, 1),
    )
    .unwrap();
    s.server
        .world_mut()
        .entity_mut(console)
        .insert(ServerControl);
    s.open::<ServerControlUi>(console);
    let (_, (), data): (u32, (), ServerControlData) = s.window::<ServerControlUi>().unwrap();
    assert!(data.server_connected);
    assert_eq!(data.servers.len(), 1);
    assert_eq!(data.servers[0].server_details, "Nominal");
    assert!(
        // The fixture names types after their path.
        data.servers[0].server_name.starts_with("server "),
        "{}",
        data.servers[0].server_name
    );
    assert_eq!(data.consoles.len(), 1);
    assert!(data.consoles[0].console_locked);
    assert_eq!(data.consoles[0].console_location, "station");

    s.act::<ServerControlUi>("lockdown_server", &[("selected_server", "0")]);
    s.run_until_for(5.0, "access refusal", |s| s.said("в доступе отказано!"));
    let card = s.card();
    s.server.world_mut().entity_mut(card).insert(IdCard {
        access: vec!["rd".into()],
    });
    s.act::<ServerControlUi>("lockdown_server", &[("selected_server", "0")]);
    s.act::<ServerControlUi>("lock_console", &[("selected_console", "0")]);
    s.run_until_for(5.0, "disabled", |s| {
        s.window::<ServerControlUi>().is_some_and(|(_, _, d)| {
            d.servers[0].server_disabled
                && d.servers[0].server_details == "Offline - Server Control Disabled"
                && !d.consoles[0].console_locked
        })
    });
    let server = {
        let world = s.server.world_mut();
        let mut q = world.query_filtered::<Entity, With<ResearchServer>>();
        q.iter(world).next().unwrap()
    };
    assert!(
        s.server
            .world()
            .get::<ResearchServer>(server)
            .unwrap()
            .research_disabled
    );
    let before = s.web().points_of(POINT_GENERIC);
    s.steps(testing::steps(3.0) as usize);
    assert_eq!(s.web().points_of(POINT_GENERIC), before, "no income");
}

/// Tech and design disks in the R&D console: a design disk's designs upload; a tech disk starts
/// with the starting nodes, its nodes are researched on upload, the researched nodes copy onto
/// it behind the 5 s copy cooldown; both eject onto the console's tile.
#[test]
fn rd_console_disks() {
    use ss13_game::science::console::{DesignDisk, TechDisk};
    let mut s = Lab::start("rd_console_disks");
    let console = s.machine::<RdConsole>(CONSOLE);
    s.server
        .world_mut()
        .get_mut::<RdConsole>(console)
        .unwrap()
        .locked = false;
    let tools = s.node(TOOLS);
    let base = s.node("/datum/techweb_node/base");
    let wrench = s.server.world().resource::<ResearchDb>().design_index["/datum/design/wrench"];

    // A design disk with the wrench.
    let disk = s.give("/obj/item/disk/design_disk", 1);
    s.server.world_mut().entity_mut(disk).insert(DesignDisk {
        blueprints: vec![wrench],
        bepis_node: None,
    });
    s.click(console);
    s.run_until_for(5.0, "design disk inserted", |s| {
        s.server.world().get::<RdConsole>(console).unwrap().d_disk == Some(disk)
    });
    assert_eq!(s.held(), None);
    s.open::<TechwebUi>(console);
    s.run_until_for(5.0, "the window lists the disk", |s| {
        s.window::<TechwebUi>()
            .is_some_and(|(_, _, d)| d.d_disk == Some(vec!["Wrench".to_owned()]))
    });
    assert!(!s.web().researched_designs.contains(&wrench));
    s.act::<TechwebUi>("uploadDisk", &[("type", "design")]);
    s.run_until_for(5.0, "design uploaded", |s| {
        s.web().researched_designs.contains(&wrench)
    });
    assert!(!s.web().researched.contains(&tools), "designs only");
    s.act::<TechwebUi>("ejectDisk", &[("type", "design")]);
    s.run_until_for(5.0, "design disk ejected", |s| {
        s.items_on(CONSOLE, "/obj/item/disk/design_disk") == 1
    });
    assert_eq!(
        s.server.world().get::<RdConsole>(console).unwrap().d_disk,
        None
    );

    // A tech disk: the starting nodes; Tools added and uploaded.
    let disk = s.give("/obj/item/disk/tech_disk", 1);
    s.click(console);
    s.run_until_for(5.0, "tech disk inserted", |s| {
        s.server.world().get::<RdConsole>(console).unwrap().t_disk == Some(disk)
    });
    let stored = &s.server.world().get::<TechDisk>(disk).unwrap().stored_nodes;
    assert!(stored.contains(&base) && !stored.contains(&tools));
    s.server
        .world_mut()
        .get_mut::<TechDisk>(disk)
        .unwrap()
        .stored_nodes
        .insert(tools);
    let points = s.web().points_of(POINT_GENERIC);
    s.act::<TechwebUi>("uploadDisk", &[("type", "tech")]);
    s.run_until_for(5.0, "tech uploaded", |s| {
        s.web().researched.contains(&tools)
    });
    assert!(s.web().points_of(POINT_GENERIC) >= points, "free");

    // Copying back waits for the cooldown.
    let secret = s.node(SECRET);
    s.server
        .world_mut()
        .resource_mut::<Techweb>()
        .researched
        .insert(secret);
    s.act::<TechwebUi>("loadTech", &[]);
    s.run_until_for(5.0, "busy", |s| s.said("Серверы заняты!"));
    assert!(
        !s.server
            .world()
            .get::<TechDisk>(disk)
            .unwrap()
            .stored_nodes
            .contains(&secret)
    );
    s.steps(testing::steps(5.0) as usize);
    s.act::<TechwebUi>("loadTech", &[]);
    s.run_until_for(5.0, "copied", |s| {
        s.server
            .world()
            .get::<TechDisk>(disk)
            .unwrap()
            .stored_nodes
            .contains(&secret)
    });
    s.act::<TechwebUi>("ejectDisk", &[("type", "tech")]);
    s.run_until_for(5.0, "tech disk ejected", |s| {
        s.items_on(CONSOLE, "/obj/item/disk/tech_disk") == 1
    });
}

/// The E.X.P.E.R.I-MENTOR: a loaded relic is revealed by `Discover` and ejected east, then its
/// power fires in hand and waits out its cooldown; a scan that is not the item's reaction fails;
/// the matching scan unhides a node the item unlocks (Obliterate destroys the screwdriver).
#[test]
fn experimentor_reveals_relics() {
    use ss13_game::science::experimentor::{Experimentor, Scan};
    use ss13_game::science::relics::{Relic, RelicPower, attack_self};
    use ss13_game::science::{ExperimentorData, ExperimentorUi};
    let mut s = Lab::start("experimentor_reveals_relics");
    let at = TilePos::new(5, 4, 1);
    let machine = ss13_game::construction::spawn_obj(
        s.server.world_mut(),
        "/obj/machinery/rnd/experimentor",
        at,
    )
    .unwrap();
    s.server
        .world_mut()
        .entity_mut(machine)
        .insert(Experimentor::default());

    // A relic: only Discover is offered; it reveals and ejects it.
    let relic = s.give("/obj/item/assembly/relic", 1);
    s.click(machine);
    s.run_until_for(5.0, "relic loaded", |s| {
        s.server
            .world()
            .get::<Experimentor>(machine)
            .unwrap()
            .loaded
            == Some(relic)
    });
    s.open::<ExperimentorUi>(machine);
    let (_, (), data): (u32, (), ExperimentorData) = s.window::<ExperimentorUi>().unwrap();
    let item = data.loaded_item.expect("loaded item");
    assert!(item.is_relic);
    assert!(item.name.starts_with("strange "), "{}", item.name);
    let offered: Vec<&str> = data
        .available_experiments
        .iter()
        .filter(|e| e.is_available)
        .map(|e| e.id.as_str())
        .collect();
    assert_eq!(offered, ["Discover"]);
    s.act::<ExperimentorUi>("experiment", &[("id", "Poke")]);
    s.run_until_for(5.0, "revealed", |s| {
        s.server
            .world()
            .get::<Relic>(relic)
            .is_some_and(|r| r.activated)
    });
    let state = s.server.world().get::<Relic>(relic).unwrap().clone();
    assert!(state.hidden_power.is_some());
    assert!((6.0..=30.0).contains(&state.cooldown));
    s.run_until_for(5.0, "ejected east", |s| {
        s.items_on(TilePos::new(6, 4, 1), "/obj/item/assembly/relic") == 1
    });
    let name = s
        .server
        .world()
        .get::<ss13_mapload::DmName>(relic)
        .unwrap()
        .name
        .clone();
    assert_eq!(name, state.real_name);

    // In hand: the power (the harmless hugger here) fires once, then the cooldown.
    s.server
        .world_mut()
        .get_mut::<Relic>(relic)
        .unwrap()
        .hidden_power = Some(RelicPower::Hugger);
    let mob = s.mob();
    let link = s.players[0].link;
    {
        let world = s.server.world_mut();
        world
            .entity_mut(relic)
            .remove::<TilePos>()
            .insert(ss13_game::inventory::CarriedBy(mob));
        let hand = world.get::<Inventory>(mob).unwrap().active;
        world.get_mut::<Inventory>(mob).unwrap().hands[hand] = Some(relic);
        assert!(attack_self(world, link, mob, relic));
    }
    let next = s.server.world().get::<Relic>(relic).unwrap().next_activate;
    assert!(next > s.server.world().resource::<Time>().elapsed_secs_f64());
    assert!(attack_self(s.server.world_mut(), link, mob, relic));
    s.run_until_for(5.0, "cooldown", |s| s.said("does not react!"));

    // A screwdriver whose reaction is Obliterate: Heat fails.
    {
        let world = s.server.world_mut();
        let hand = world.get::<Inventory>(mob).unwrap().active;
        world.get_mut::<Inventory>(mob).unwrap().hands[hand] = None;
        if world.get_entity(relic).is_ok() {
            world.despawn(relic);
        }
    }
    s.steps(testing::steps(2.5) as usize);
    let screwdriver = s.give("/obj/item/screwdriver", 1);
    s.click(machine);
    s.run_until_for(5.0, "screwdriver loaded", |s| {
        s.server
            .world()
            .get::<Experimentor>(machine)
            .unwrap()
            .loaded
            == Some(screwdriver)
    });
    s.server
        .world_mut()
        .get_mut::<Experimentor>(machine)
        .unwrap()
        .item_reactions
        .insert("/obj/item/screwdriver".into(), Scan::Obliterate);
    s.act::<ExperimentorUi>("experiment", &[("id", "Heat")]);
    s.run_until_for(5.0, "failure", |s| s.said("the experiment was a failure."));
    let secret = s.node(SECRET);
    assert!(s.web().hidden.contains(&secret));
    // A failure may eject the item (a global malfunction); reload it then.
    s.steps(testing::steps(2.5) as usize);
    if s.server
        .world()
        .get::<Experimentor>(machine)
        .unwrap()
        .loaded
        != Some(screwdriver)
    {
        return;
    }
    s.act::<ExperimentorUi>("experiment", &[("id", "Obliterate")]);
    s.run_until_for(5.0, "secret unhidden", |s| {
        !s.web().hidden.contains(&secret)
    });
    assert!(s.server.world().get_entity(screwdriver).is_err(), "crushed");
}

/// A gizmo: a random name and eight cryptic wires; pulsing its wires walks the puzzle, a solved
/// sequence runs its gizpulse (here a dispenser) once per cooldown.
#[test]
fn gizmo_wires_solve_the_puzzle() {
    use ss13_game::science::gizmo::{
        CRYPTIC_WIRES, Control, Gizmo, Gizmode, Pulse, Solution, ensure,
    };
    let mut s = Lab::start("gizmo_wires_solve_the_puzzle");
    let at = TilePos::new(5, 4, 1);
    let gizmo = {
        let world = s.server.world_mut();
        let e = ss13_game::construction::spawn_obj(world, "/obj/machinery/gizmo", at).unwrap();
        assert!(ensure(world, e));
        let wires = world.get::<ss13_game::wires::Wires>(e).unwrap();
        assert_eq!(wires.colors.len(), 8);
        assert_ne!(world.get::<ss13_mapload::DmName>(e).unwrap().name, "gizmo");
        assert!(!world.get::<Gizmo>(e).unwrap().codes.is_empty());
        // A known puzzle: wires 1, 2, 3 dispense.
        world.entity_mut(e).insert(Gizmo {
            modes: vec![Gizmode {
                pulses: vec![Pulse::Food("/obj/item/wrench")],
                current: 0,
                control: Control::Direct,
                cooldown: 1.0,
                ready_at: 0.0,
            }],
            codes: vec![(vec![0, 1, 2], Solution::Direct(0, 0))],
            ..Default::default()
        });
        e
    };
    let mob = s.mob();
    let pulse = |s: &mut Lab, i: usize| {
        ss13_game::wires::pulse(s.server.world_mut(), gizmo, CRYPTIC_WIRES[i], Some(mob));
    };
    pulse(&mut s, 0);
    pulse(&mut s, 5);
    assert!(
        s.server
            .world()
            .get::<Gizmo>(gizmo)
            .unwrap()
            .current
            .is_empty(),
        "reset"
    );
    for i in [0, 1, 2] {
        pulse(&mut s, i);
    }
    s.steps(2);
    assert_eq!(s.items_on(at, "/obj/item/wrench"), 1);
    // The cooldown: an immediate second solution dispenses nothing.
    for i in [0, 1, 2] {
        pulse(&mut s, i);
    }
    s.steps(2);
    assert_eq!(s.items_on(at, "/obj/item/wrench"), 1);
}

impl Lab {
    fn silo_store_entity(&self) -> Entity {
        let world = self.server.world();
        world
            .get::<Lathe>(self.machine::<Lathe>(LATHE))
            .unwrap()
            .silo
            .unwrap()
    }
}

/// MetaStation's R&D: two servers earning points, three unlocked consoles, every lathe linked to
/// the ore silo, the departmental techfabs' catalogues.
#[test]
#[ignore = "needs generated assets and MetaStation; run with --ignored"]
fn metastation_research() {
    let assets =
        PathBuf::from(std::env::var("SS13_ASSETS").unwrap_or_else(|_| "assets/generated".into()));
    let map = MapLoadPlugin {
        source: PathBuf::from(env!("SS13_SOURCE")),
        maps: vec!["MetaStation".into()],
        assets,
    };
    let mut s = Lab::start_with("metastation_research", map);
    let world = s.server.world_mut();
    let servers = world
        .query::<&ss13_game::science::points::ResearchServer>()
        .iter(world)
        .count();
    let consoles: Vec<bool> = world
        .query::<&RdConsole>()
        .iter(world)
        .map(|c| c.locked)
        .collect();
    let lathes: Vec<(String, bool)> = world
        .query::<(&Lathe, &ss13_mapload::DmObj)>()
        .iter(world)
        .map(|(l, o)| (o.path.clone(), l.silo.is_some()))
        .collect();
    let silos = world.query::<&OreSilo>().iter(world).count();
    let db = world.resource::<ResearchDb>();
    let web = world.resource::<Techweb>();
    println!(
        "{} nodes, {} designs; {} researched nodes, {} researched designs",
        db.nodes.len(),
        db.designs.len(),
        web.researched.len(),
        web.researched_designs.len()
    );
    println!("{servers} servers, consoles locked {consoles:?}, {silos} silos, lathes {lathes:?}");
    assert_eq!(servers, 2);
    assert_eq!(
        consoles,
        vec![false; 3],
        "MetaStation maps the unlocked consoles"
    );
    assert_eq!(silos, 1);
    // Nine R&D lathes and the two wiremod component printers.
    assert_eq!(lathes.len(), 11);
    assert!(lathes.iter().all(|(_, linked)| *linked));
    assert!(db.nodes.len() > 100 && web.researched.len() >= 10);
    s.run_until_for(30.0, "points from the servers", |s| {
        s.web().points_of(POINT_GENERIC) >= 2.0
    });
}
