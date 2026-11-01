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
use ss13_game::science::lathe::{Lathe, MaterialStore, OreSilo};
use ss13_game::science::techweb::{POINT_GENERIC, ResearchDb, Techweb};
use ss13_game::science::{
    AnalyzerData, DestructiveAnalyzerUi, FabricatorData, FabricatorStatic, FabricatorUi,
    TechwebData, TechwebStatic, TechwebUi,
};
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_game::tools::Stack;
use ss13_mapload::convert::DmTypes;
use ss13_mapload::{DmItem, MapLoadPlugin};
use ss13_net::chat::client::ChatReceived;
use ss13_net::ui::client::UiMessageReceived;
use ss13_shared::TilePos;
use ss13_shared::ui::UiServerMessage;
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
/// `C` (3,3), the protolathe `L` (4,3) and the destructive analyzer `A` (5,3); the server `R`
/// and the ore silo `O` are further away.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/station)
"f" = (/turf/open/floor,/area/station)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/station)
"C" = (/obj/machinery/computer/rdconsole,/turf/open/floor,/area/station)
"L" = (/obj/machinery/rnd/production/protolathe,/turf/open/floor,/area/station)
"A" = (/obj/machinery/rnd/destructive_analyzer,/turf/open/floor,/area/station)
"R" = (/obj/machinery/rnd/server,/turf/open/floor,/area/station)
"O" = (/obj/machinery/ore_silo,/turf/open/floor,/area/station)

(1,1,1) = {"
wwwwwwwww
wffsfffOw
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

const STATES: [(&str, &str); 15] = [
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
    assert_eq!(lathes.len(), 9);
    assert!(lathes.iter().all(|(_, linked)| *linked));
    assert!(db.nodes.len() > 100 && web.researched.len() >= 10);
    s.run_until_for(30.0, "points from the servers", |s| {
        s.web().points_of(POINT_GENERIC) >= 2.0
    });
}
