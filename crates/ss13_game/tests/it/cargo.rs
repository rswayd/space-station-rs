//! Cargo over real WebTransport on 127.0.0.1, on a fixture station with a fixture supply
//! shuttle: the player's ID gets a bank account and a payday pays it from its department; the
//! quartermaster opens the supply console, orders a pack, sends for the shuttle (short timers);
//! it docks with the pack's locked crate (contents and manifest inside), which stays locked to
//! an ID without the pack's access and opens with cargo access; living things keep the shuttle
//! from leaving; sent away, everything aboard is sold into the cargo budget and the console
//! shows CentCom's report.
//!
//! `metastation_supply` (ignored, needs generated assets in `SS13_ASSETS`) registers
//! MetaStation's cargo shuttle and delivers one order to the cargo bay.

use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::cargo::console::CargoConsole;
use ss13_game::cargo::economy::{ACCOUNT_CAR, ACCOUNT_CIV, BankAccountId, Economy, EconomyConfig};
use ss13_game::cargo::export::{Exports, export_value};
use ss13_game::cargo::supply::{
    LastSale, SupplyConfig, SupplyLocation, SupplyMode, SupplyOrders, SupplyShuttle,
};
use ss13_game::cargo::{CargoData, CargoStatic, CargoUi};
use ss13_game::closets::Closet;
use ss13_game::health::Health;
use ss13_game::inventory::{IdCard, Inventory};
use ss13_game::storage::Stored;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::convert::DmTypes;
use ss13_mapload::{DmItem, DmName, MapLoadPlugin};
use ss13_net::chat::client::ChatReceived;
use ss13_net::ui::client::UiMessageReceived;
use ss13_shared::ui::UiServerMessage;
use ss13_shared::{Dir, TilePos};
use ss13_tgui::{UiInterface, decode};
use ss13_world::SpatialIndex;

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/datum/supply_pack": { "parent": "/datum", "declared_vars": { "name": "Crate",
        "group": "", "cost": 280, "access": 0, "access_view": 0, "access_any": 0,
        "contains": null, "crate_name": "crate", "desc": "",
        "crate_type": { "path": "/obj/structure/closet/crate" }, "order_flags": 0 },
        "procs": ["fill", "generate_supply_packs"] },
    "/datum/supply_pack/engineering": { "parent": "/datum/supply_pack",
        "declared_vars": { "group": "Engineering" } },
    "/datum/supply_pack/engineering/tools": { "parent": "/datum/supply_pack/engineering",
        "declared_vars": { "name": "Tool Crate", "cost": 300, "access": "cargo",
            "crate_name": "tool crate", "desc": "A wrench and some iron.",
            "crate_type": { "path": "/obj/structure/closet/crate/secure" },
            "contains": { "list": [ { "path": "/obj/item/wrench" },
                { "path": "/obj/item/stack/sheet/iron/five" } ] } } },
    "/datum/export": { "parent": "/datum", "declared_vars": {
        "abstract_type": { "path": "/datum/export" }, "cost": 1, "k_elasticity": 1,
        "k_hit_percentile": 0.05, "k_recovery_time": 6000, "amount_report_multiplier": 1,
        "include_subtypes": 1, "export_types": { "list": [] },
        "exclude_types": { "list": [] }, "sales_market": "supply", "unit_name": "",
        "message": "" },
        "procs": ["get_base_cost", "get_amount", "applies_to", "sell_object"] },
    "/datum/export/crate": { "parent": "/datum/export", "declared_vars": { "cost": 200,
        "k_elasticity": 0, "unit_name": "crate",
        "export_types": { "list": [ { "path": "/obj/structure/closet/crate" } ] } } },
    "/datum/export/wrench": { "parent": "/datum/export", "declared_vars": { "cost": 2,
        "unit_name": "wrench", "export_types": { "list": [ { "path": "/obj/item/wrench" } ] } } },
    "/datum/export/material": { "parent": "/datum/export", "declared_vars": {
        "abstract_type": { "path": "/datum/export/material" }, "cost": 5,
        "amount_report_multiplier": 100, "material_id": null,
        "export_types": { "list": [ { "path": "/obj/item/stack/sheet/mineral" } ] } },
        "procs": ["get_amount"] },
    "/datum/export/material/market": { "parent": "/datum/export/material", "declared_vars": {
        "abstract_type": { "path": "/datum/export/material/market" }, "cost": 1 },
        "procs": ["get_base_cost", "sell_object"] },
    "/datum/export/material/market/iron": { "parent": "/datum/export/material/market",
        "declared_vars": { "material_id": { "path": "/datum/material/iron" },
            "message": "cm3 of iron",
            "export_types": { "list": [ { "path": "/obj/item/stack/sheet/iron" } ] } } },
    "/datum/material": { "parent": "/datum" },
    "/datum/material/iron": { "parent": "/datum/material", "declared_vars": {
        "value_per_unit": 0.05 } },
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
    "/obj/docking_port": { "parent": "/obj", "declared_vars": { "invisibility": 101,
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "anchored": 1, "dir": 1,
        "shuttle_id": null, "width": 0, "height": 0, "dwidth": 0, "dheight": 0 } },
    "/obj/docking_port/stationary": { "parent": "/obj/docking_port", "declared_vars": {
        "json_key": null } },
    "/obj/docking_port/mobile": { "parent": "/obj/docking_port" },
    "/obj/docking_port/mobile/supply": { "parent": "/obj/docking_port/mobile",
        "declared_vars": { "shuttle_id": "cargo", "dir": 8, "name": "supply shuttle" } },
    "/obj/machinery": { "parent": "/obj", "declared_vars": { "anchored": 1 } },
    "/obj/machinery/computer": { "parent": "/obj/machinery", "declared_vars": {
        "icon": { "resource": "c.dmi" }, "icon_state": "computer", "density": 1 } },
    "/obj/machinery/computer/cargo": { "parent": "/obj/machinery/computer",
        "declared_vars": { "name": "supply console" } },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null, "w_class": 3,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null } },
    "/obj/item/wrench": { "parent": "/obj/item", "declared_vars": { "name": "wrench",
        "icon_state": "wrench" } },
    "/obj/item/stack": { "parent": "/obj/item", "declared_vars": { "amount": 1,
        "max_amount": 50, "merge_type": null, "singular_name": null, "mats_per_unit": null } },
    "/obj/item/stack/sheet": { "parent": "/obj/item/stack" },
    "/obj/item/stack/sheet/iron": { "parent": "/obj/item/stack/sheet", "declared_vars": {
        "name": "iron", "icon_state": "sheet-metal", "singular_name": "iron sheet",
        "merge_type": { "path": "/obj/item/stack/sheet/iron" },
        "mats_per_unit": { "list": [ { "assoc": [ { "path": "/datum/material/iron" }, 100 ] } ] } } },
    "/obj/item/stack/sheet/iron/five": { "parent": "/obj/item/stack/sheet/iron",
        "vars": { "name": "iron" }, "declared_vars": { "amount": 5 } },
    "/obj/item/paper": { "parent": "/obj/item", "declared_vars": { "name": "paper",
        "icon_state": "paper" } },
    "/obj/item/paper/fluff": { "parent": "/obj/item/paper" },
    "/obj/item/paper/fluff/jobs": { "parent": "/obj/item/paper/fluff" },
    "/obj/item/paper/fluff/jobs/cargo": { "parent": "/obj/item/paper/fluff/jobs" },
    "/obj/item/paper/fluff/jobs/cargo/manifest": { "parent": "/obj/item/paper/fluff/jobs/cargo" },
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
    "/obj/structure": { "parent": "/obj", "declared_vars": { "anchored": 1 } },
    "/obj/structure/closet": { "parent": "/obj/structure", "declared_vars": {
        "icon": { "resource": "cr.dmi" }, "icon_state": "generic", "density": 1,
        "anchored": 0, "opened": 0, "locked": 0, "welded": 0, "secure": 0, "icon_door": null,
        "icon_welded": "welded", "enable_door_overlay": 1, "has_opened_overlay": 1,
        "has_closed_overlay": 1, "dense_when_open": 0, "wall_mounted": 0, "horizontal": 0,
        "storage_capacity": 30, "name": "closet" } },
    "/obj/structure/closet/crate": { "parent": "/obj/structure/closet", "declared_vars": {
        "icon_state": "crate", "base_icon_state": "crate", "dense_when_open": 1,
        "horizontal": 1, "name": "crate" } },
    "/obj/structure/closet/crate/secure": { "parent": "/obj/structure/closet/crate",
        "declared_vars": { "icon_state": "securecrate", "base_icon_state": "securecrate",
            "locked": 1, "secure": 1, "name": "secure crate" } },
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
    "/turf/open/floor/shuttle": { "parent": "/turf/open/floor", "declared_vars": {
        "icon_state": "shuttle" } },
    "/turf/open/space": { "parent": "/turf/open", "declared_vars": { "temperature": 2.7,
        "initial_gas_mix": "TEMP=2.7", "icon_state": "space",
        "baseturfs": { "path": "/turf/open/space" } } },
    "/turf/open/space/basic": { "parent": "/turf/open/space" },
    "/turf/closed": { "parent": "/turf", "declared_vars": { "density": 1, "opacity": 1,
        "blocks_air": 1 } },
    "/turf/closed/wall": { "parent": "/turf/closed", "declared_vars": { "icon_state": "wall",
        "baseturfs": { "path": "/turf/open/floor/plating" } } },
    "/turf/closed/wall/shuttle": { "parent": "/turf/closed/wall", "declared_vars": {
        "icon_state": "swall" } },
    "/area": { "parent": "/atom" },
    "/area/space": { "parent": "/area", "declared_vars": { "name": "Space" } },
    "/area/station": { "parent": "/area", "declared_vars": { "name": "Склад" } },
    "/area/shuttle": { "parent": "/area" },
    "/area/shuttle/supply": { "parent": "/area/shuttle", "declared_vars": {
        "name": "Supply Shuttle" } }
  }
}"##;

/// Rows top-down, the last line is y = 1. The player spawns at `s` (5,5) next to the supply
/// console `K` (6,5); the floor row y = 4 opens onto `cargo_home` `D` (5,3), facing south,
/// sized for the 3x2 shuttle.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/station)
"f" = (/turf/open/floor,/area/station)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/station)
"K" = (/obj/machinery/computer/cargo,/turf/open/floor,/area/station)
"e" = (/turf/open/space/basic,/area/space)
"D" = (/obj/docking_port/stationary{dir = 2; dwidth = 1; height = 2; name = "Test Cargo Bay"; shuttle_id = "cargo_home"; width = 3},/turf/open/space/basic,/area/space)

(1,1,1) = {"
wwwwwwwww
wfffffffw
wfffsKffw
wfffffffw
eeeeDeeee
eeeeeeeee
eeeeeeeee
"}
"##;

/// The shuttle: one floor tile (the port's, in the top row's middle) between walls, so the
/// crate always lands on it.
const SHUTTLE: &str = r##""w" = (/turf/closed/wall/shuttle,/area/shuttle/supply)
"p" = (/obj/docking_port/mobile/supply{dir = 2},/turf/open/floor/shuttle,/area/shuttle/supply)

(1,1,1) = {"
wpw
www
"}
"##;

const SPAWN: TilePos = TilePos::new(5, 5, 1);
const CONSOLE: TilePos = TilePos::new(6, 5, 1);
const DOCK: TilePos = TilePos::new(5, 3, 1);
const PACK: &str = "/datum/supply_pack/engineering/tools";

const STATES: [(&str, &str); 19] = [
    ("t.dmi", "floor"),
    ("t.dmi", "plating"),
    ("t.dmi", "wall"),
    ("t.dmi", "space"),
    ("t.dmi", "shuttle"),
    ("t.dmi", "swall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "card_grey"),
    ("i.dmi", "toy"),
    ("i.dmi", "wrench"),
    ("i.dmi", "sheet-metal"),
    ("i.dmi", "paper"),
    ("c.dmi", "computer"),
    ("cr.dmi", "crate"),
    ("cr.dmi", "securecrate"),
    ("cr.dmi", "securecrateopen"),
    ("cr.dmi", "securecrater"),
    ("cr.dmi", "securecrateg"),
];

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("cargo_fixture")
        .join(test);
    let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
    let cargo = ss13_mapload::cargo::resolve(&types);
    let mut used = vec![
        "/obj/effect/landmark/start/assistant",
        "/obj/docking_port/stationary",
        "/obj/docking_port/mobile/supply",
        "/obj/machinery/computer/cargo",
        "/turf/open/floor",
        "/turf/open/floor/plating",
        "/turf/open/floor/shuttle",
        "/turf/closed/wall",
        "/turf/closed/wall/shuttle",
        "/turf/open/space/basic",
        "/area/space",
        "/area/station",
        "/area/shuttle/supply",
    ];
    used.extend(cargo.spawned_types());
    write_fixture(
        &dir,
        TYPES,
        &used,
        &STATES,
        &[("map.dmm", MAP), ("shuttle.dmm", SHUTTLE)],
    );
    std::fs::write(dir.join(ss13_mapload::cargo::CARGO_FILE), cargo.to_ron()).unwrap();
    dir
}

#[derive(Resource, Default)]
struct Chat(Vec<String>);

#[derive(Resource, Default)]
struct ConsoleWindow {
    id: Option<u32>,
    static_data: Option<CargoStatic>,
    data: Option<CargoData>,
}

fn record(
    mut chat: MessageReader<ChatReceived>,
    mut ui: MessageReader<UiMessageReceived>,
    mut out: ResMut<Chat>,
    mut window: ResMut<ConsoleWindow>,
) {
    for ChatReceived(message) in chat.read() {
        out.0.push(message.plain_text());
    }
    for UiMessageReceived(message) in ui.read() {
        match message {
            UiServerMessage::Open {
                window: id,
                interface,
                static_data,
                data,
                ..
            } if interface == CargoUi::NAME => {
                window.id = Some(*id);
                window.static_data = Some(decode(static_data).unwrap());
                window.data = Some(decode(data).unwrap());
            }
            UiServerMessage::Update {
                window: id,
                data: Some(data),
                ..
            } if window.id == Some(*id) => window.data = Some(decode(data).unwrap()),
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
        format!("; chat: {:?}", self.chat())
    }
}

impl Station {
    fn start(test: &str, map: MapLoadPlugin, supply: SupplyConfig, payday_secs: f64) -> Self {
        let mut s = testing::Station::start(map, |app| {
            app.insert_resource(supply)
                .insert_resource(EconomyConfig { payday_secs });
        });
        s.join(test, |app| {
            app.init_resource::<Chat>()
                .init_resource::<ConsoleWindow>()
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

    fn fixture(test: &str) -> Self {
        let assets = fixture_assets(test);
        let supply = SupplyConfig {
            call_secs: 1.0,
            ignition_secs: 0.3,
            template: Some(assets.join("shuttle.dmm")),
        };
        let s = Self::start(test, fixture_map(&assets, &["map.dmm"]), supply, 1.5);
        assert_eq!(s.pos(), SPAWN);
        s
    }

    fn steps(&mut self, n: usize) {
        for _ in 0..n {
            self.step();
        }
    }

    fn pos(&self) -> TilePos {
        *self.server.world().get::<TilePos>(self.mob()).unwrap()
    }

    fn walk(&mut self, dir: Dir) {
        let to = self.pos().step(dir);
        self.run_until_for(10.0, &format!("walk {dir:?} to {to:?}"), |s| {
            if s.pos() == to {
                return true;
            }
            s.try_step(0, dir);
            false
        });
    }

    fn chat(&self) -> &[String] {
        &self.client().world().resource::<Chat>().0
    }

    fn said(&self, text: &str) -> bool {
        self.chat().iter().any(|line| line.contains(text))
    }

    fn click(&mut self, entity: Entity) {
        self.run_until_for(10.0, "the target is replicated", |s| {
            s.net_id(0, entity).is_some()
        });
        self.0.click(0, entity);
    }

    fn act(&mut self, action: &str, params: &[(&str, &str)]) {
        let window = self
            .client()
            .world()
            .resource::<ConsoleWindow>()
            .id
            .expect("window");
        self.0.act(0, window, action, params);
    }

    fn data(&self) -> CargoData {
        self.client()
            .world()
            .resource::<ConsoleWindow>()
            .data
            .clone()
            .unwrap_or_default()
    }

    fn card(&self) -> Entity {
        self.server
            .world()
            .get::<Inventory>(self.mob())
            .unwrap()
            .id
            .expect("ID card")
    }

    fn set_access(&mut self, access: &[&str]) {
        let card = self.card();
        self.server.world_mut().entity_mut(card).insert(IdCard {
            access: access.iter().map(|a| (*a).to_owned()).collect(),
        });
    }

    fn economy(&self) -> &Economy {
        self.server.world().resource::<Economy>()
    }

    fn cargo_budget(&self) -> i64 {
        self.economy().department(ACCOUNT_CAR).unwrap().balance
    }

    fn shuttle(&self) -> &SupplyShuttle {
        self.server.world().resource::<SupplyShuttle>()
    }

    fn console(&self) -> Entity {
        let world = self.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(CONSOLE)
            .iter()
            .copied()
            .find(|&e| {
                world
                    .get::<ss13_game::cargo::console::CargoConsole>(e)
                    .is_some()
            })
            .unwrap()
    }

    fn crate_on(&self, pos: TilePos) -> Option<Entity> {
        let world = self.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .find(|&e| world.get::<Closet>(e).is_some())
    }
}

#[test]
fn order_deliver_open_sell_and_payday() {
    let mut s = Station::fixture("order_deliver_open_sell_and_payday");

    // The assistant's ID has a bank account with five starting paychecks (PAYCHECK_LOWER).
    let account = s
        .server
        .world()
        .get::<BankAccountId>(s.card())
        .expect("the job's ID card carries its account")
        .0;
    assert_eq!(s.economy().account(account).unwrap().balance, 125);
    assert_eq!(s.cargo_budget(), 0, "cargo starts with nothing");
    let civ = s.economy().department(ACCOUNT_CIV).unwrap().balance;
    assert_eq!(civ, 4999);

    // A payday: every budget gets MAX_GRANT_DPT, the assistant is paid 25 from the civilian one.
    s.run_until_for(10.0, "payday", |s| {
        s.economy().account(account).unwrap().balance == 150
    });
    assert_eq!(
        s.economy().department(ACCOUNT_CIV).unwrap().balance,
        4999 + 500 - 25
    );
    assert_eq!(s.cargo_budget(), 500);
    s.run_until_for(5.0, "the bank card message", |s| {
        s.said("Выплата зарплаты успешна. На аккаунте теперь 150¢.")
    });
    // No more paydays during the rest of the test, and no boosted exports (tg skips them under
    // `UNIT_TESTS`).
    s.server
        .world_mut()
        .resource_mut::<EconomyConfig>()
        .payday_secs = 1e9;
    s.server
        .world_mut()
        .resource_mut::<Exports>()
        .clear_boosted();

    // The quartermaster opens the supply console: catalogue and status.
    s.set_access(&["qm", "cargo"]);
    let console = s.console();
    s.click(console);
    s.run_until_for(10.0, "the console window opens", |s| {
        s.client().world().resource::<ConsoleWindow>().id.is_some()
    });
    let st = s
        .client()
        .world()
        .resource::<ConsoleWindow>()
        .static_data
        .clone()
        .unwrap();
    assert_eq!(st.groups.len(), 1);
    assert_eq!(st.groups[0].name, "Engineering");
    let pack = &st.groups[0].packs[0];
    assert_eq!(
        (pack.id.as_str(), pack.cost, pack.access),
        (PACK, 300, true)
    );
    assert_eq!(pack.contains.len(), 2);
    assert_eq!(pack.contains[0].name, "wrench");
    let data = s.data();
    assert_eq!(data.points, 500);
    assert_eq!(data.location, "CentCom");
    assert!(data.away && data.docked && data.can_send && !data.requestonly);

    // Unknown packs and amounts out of range change nothing; a real one lands in the cart.
    s.act("add", &[("id", "/datum/supply_pack/nope")]);
    s.act("modify", &[("order_name", "Tool Crate"), ("amount", "99")]);
    s.act("add", &[("id", PACK)]);
    s.run_until_for(10.0, "the order is in the cart", |s| {
        !s.data().cart.is_empty()
    });
    let cart = s.data().cart;
    assert_eq!(cart.len(), 1);
    assert_eq!((cart[0].amount, cart[0].cost), (1, 300));
    let order_id = cart[0].id;

    // Send: the shuttle buys at CentCom (300 from the budget) and docks at the station.
    s.act("send", &[]);
    s.run_until_for(10.0, "called", |s| {
        s.said("The supply shuttle has been called and will arrive in 0 minutes.")
    });
    s.run_until_for(10.0, "docked at the station", |s| {
        s.shuttle().location == SupplyLocation::Station && s.shuttle().mode == SupplyMode::Idle
    });
    assert_eq!(s.cargo_budget(), 200);
    assert!(
        s.server
            .world()
            .resource::<SupplyOrders>()
            .shopping_list
            .is_empty()
    );
    let crate_entity = s.crate_on(DOCK).expect("the crate is on the shuttle");
    let world = s.server.world();
    assert_eq!(
        world.get::<DmName>(crate_entity).unwrap().name,
        format!("tool crate - #{order_id}")
    );
    let closet = world.get::<Closet>(crate_entity).unwrap();
    assert!(closet.locked && closet.secure);
    assert_eq!(closet.req_access, ["cargo"]);
    let contents: Vec<String> = world
        .get::<Stored>(crate_entity)
        .unwrap()
        .items()
        .iter()
        .map(|&e| world.get::<DmName>(e).unwrap().name.clone())
        .collect();
    assert_eq!(contents.len(), 3, "{contents:?}");
    assert_eq!(contents[0], "wrench");
    assert_eq!(contents[1], "iron");
    assert_eq!(
        contents[2],
        format!("shipping manifest - #{order_id} (Tool Crate)")
    );
    // The export scanner's dry run: crate, wrench and iron, nothing sold.
    let now = world.resource::<Time>().elapsed_secs_f64();
    assert_eq!(
        export_value(world, world.resource::<Exports>(), crate_entity, now),
        (227, true)
    );
    s.run_until_for(10.0, "the window shows the docked shuttle", |s| {
        s.data().location == "Test Cargo Bay" && !s.data().away
    });

    // Without cargo access the crate stays locked; with it, it unlocks and opens.
    s.walk(Dir::South);
    s.set_access(&["maint_tunnels"]);
    s.click(crate_entity);
    s.steps(20);
    assert!(s.server.world().get::<Closet>(crate_entity).unwrap().locked);
    s.set_access(&["cargo"]);
    s.click(crate_entity);
    s.run_until_for(10.0, "unlocked", |s| {
        !s.server.world().get::<Closet>(crate_entity).unwrap().locked
    });
    s.click(crate_entity);
    s.run_until_for(10.0, "opened", |s| {
        s.server.world().get::<Closet>(crate_entity).unwrap().opened
    });
    let world = s.server.world();
    let wrenches = world
        .resource::<SpatialIndex>()
        .on_tile(DOCK)
        .iter()
        .filter(|&&e| {
            world
                .get::<DmItem>(e)
                .is_some_and(|i| i.path == "/obj/item/wrench")
        })
        .count();
    assert_eq!(wrenches, 1, "the contents are on the crate's tile");

    // A living thing aboard keeps the shuttle at the station.
    s.walk(Dir::North);
    let stowaway = s.server.world_mut().spawn((Health::default(), DOCK)).id();
    s.act("send", &[]);
    s.run_until_for(10.0, "safety warning", |s| {
        s.said("For safety and ethical reasons")
    });
    assert_eq!(s.shuttle().mode, SupplyMode::Idle);
    s.server.world_mut().despawn(stowaway);

    // Sent away: crate (200), wrench (2) and 5 iron sheets (5 each) are sold on arrival.
    s.act("send", &[]);
    s.run_until_for(10.0, "departing", |s| {
        s.said("The supply shuttle is departing.")
    });
    s.run_until_for(10.0, "docked at CentCom", |s| {
        s.shuttle().location == SupplyLocation::CentCom && s.shuttle().mode == SupplyMode::Idle
    });
    let sale = s.server.world().resource::<LastSale>().clone();
    assert_eq!(sale.value, 227, "{:?}", sale.report);
    assert_eq!(s.cargo_budget(), 200 + 227);
    assert!(
        s.server.world().get_entity(crate_entity).is_err(),
        "sold things are gone"
    );
    s.run_until_for(10.0, "the console shows CentCom's report", |s| {
        s.data().message.contains("+200¢: Received 1 crate.")
    });
    let message = s.data().message;
    assert!(message.contains("+2¢: Received 1 wrench."), "{message}");
    assert!(
        message.contains("+25¢: Received 500 cm3 of iron."),
        "{message}"
    );
    assert_eq!(s.data().points, 427);
}

/// The supply console as a request console: a request for the requester's department budget
/// is listed with that budget, only a console that approves can approve it, and the department
/// pays when the shuttle buys it while the cargo budget stays as it was.
#[test]
fn department_request_is_paid_by_the_department() {
    let mut s = Station::fixture("department_request_is_paid_by_the_department");
    s.server
        .world_mut()
        .resource_mut::<EconomyConfig>()
        .payday_secs = 1e9;
    let console = s.console();
    let set_request_only = |s: &mut Station, request_only: bool| {
        let world = s.server.world_mut();
        let mut state = world.get_mut::<CargoConsole>(console).unwrap();
        state.request_only = request_only;
        state.can_send = !request_only;
        state.can_approve_requests = !request_only;
    };
    set_request_only(&mut s, true);
    s.click(console);
    s.run_until_for(10.0, "the console window opens", |s| {
        s.client().world().resource::<ConsoleWindow>().id.is_some()
    });
    s.run_until_for(10.0, "request-only data", |s| s.data().requestonly);

    s.act(
        "add",
        &[("id", PACK), ("reason", "for the clinic"), ("department", "1")],
    );
    s.run_until_for(10.0, "the request is listed", |s| {
        !s.data().requests.is_empty()
    });
    let request = s.data().requests[0].clone();
    assert_eq!(request.account, "Гражданский бюджет");
    assert_eq!(request.reason, "for the clinic");
    assert_eq!(
        s.server.world().resource::<SupplyOrders>().request_list[0]
            .paying_department
            .as_deref(),
        Some(ACCOUNT_CIV)
    );
    let id = request.id.to_string();
    s.act("approve", &[("id", id.as_str())]);
    s.steps(20);
    assert_eq!(
        s.server.world().resource::<SupplyOrders>().request_list.len(),
        1,
        "a request console cannot approve"
    );

    set_request_only(&mut s, false);
    s.act("approve", &[("id", id.as_str())]);
    s.run_until_for(10.0, "approved into the cart", |s| !s.data().cart.is_empty());
    let civ = s.economy().department(ACCOUNT_CIV).unwrap().balance;
    let cargo = s.cargo_budget();
    s.act("send", &[]);
    s.run_until_for(10.0, "docked at the station", |s| {
        s.shuttle().location == SupplyLocation::Station && s.shuttle().mode == SupplyMode::Idle
    });
    assert!(s.crate_on(DOCK).is_some(), "the order was delivered");
    assert_eq!(
        s.economy().department(ACCOUNT_CIV).unwrap().balance,
        civ - 300
    );
    assert_eq!(s.cargo_budget(), cargo, "a department order earns cargo nothing");
}

/// Registers MetaStation's supply shuttle from its map json and delivers one order to the
/// cargo bay.
#[test]
#[ignore = "needs generated assets and MetaStation; run with --ignored"]
fn metastation_supply() {
    let assets =
        PathBuf::from(std::env::var("SS13_ASSETS").unwrap_or_else(|_| "assets/generated".into()));
    let map = MapLoadPlugin {
        source: PathBuf::from(env!("SS13_SOURCE")),
        maps: vec!["MetaStation".into()],
        assets,
    };
    let supply = SupplyConfig {
        call_secs: 1.0,
        ignition_secs: 0.2,
        template: None,
    };
    let mut s = Station::start("metastation_supply", map, supply, 1e9);
    let shuttle = s.shuttle();
    assert!(shuttle.home.is_some(), "MetaStation has cargo_home");
    assert_eq!(shuttle.location, SupplyLocation::CentCom);
    let packs = s
        .server
        .world()
        .resource::<ss13_game::cargo::supply::SupplyPacks>();
    println!("{} packs", packs.packs.len());
    let pack = packs
        .packs
        .iter()
        .position(|p| p.id == "/datum/supply_pack/emergency/internals")
        .or_else(|| {
            packs
                .packs
                .iter()
                .position(|p| ss13_game::cargo::supply::orderable(p, false))
        })
        .unwrap();
    {
        let world = s.server.world_mut();
        world
            .resource_mut::<Economy>()
            .department_mut(ACCOUNT_CAR)
            .unwrap()
            .adjust_money(10_000);
        let id = world.resource_mut::<SupplyOrders>().next_id();
        world.resource_mut::<SupplyOrders>().shopping_list.push(
            ss13_game::cargo::supply::SupplyOrder {
                id,
                pack,
                orderer: "test".into(),
                orderer_rank: "Quartermaster".into(),
                reason: String::new(),
                paying_account: None,
                paying_department: None,
                can_be_cancelled: true,
            },
        );
        assert!(ss13_game::cargo::supply::request(world, true));
    }
    s.run_until_for(20.0, "docked at the station", |s| {
        s.shuttle().location == SupplyLocation::Station && s.shuttle().mode == SupplyMode::Idle
    });
    let world = s.server.world_mut();
    let home = world.resource::<SupplyShuttle>().home.unwrap();
    let dock = *world.get::<TilePos>(home).unwrap();
    let mut crates = world.query::<(&Closet, &DmName, &TilePos)>();
    let delivered: Vec<(String, TilePos)> = crates
        .iter(world)
        .filter(|(_, n, p)| n.name.contains(" - #") && p.z == dock.z)
        .map(|(_, n, p)| (n.name.clone(), *p))
        .collect();
    println!("cargo_home at {dock:?}; delivered {delivered:?}");
    assert_eq!(delivered.len(), 1);
}
