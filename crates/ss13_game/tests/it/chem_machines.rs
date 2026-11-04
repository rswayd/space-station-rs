//! The chemist's machines over real WebTransport on 127.0.0.1: a beaker into the ChemMaster,
//! reagents moved into its buffer and destroyed, two named pills printed with a thicker sugar
//! shell and one swallowed (its contents reach the stomach when the shell is gone); a beaker in
//! the reaction chamber heated until its reaction runs, and a buffer injected into it; a chem
//! dispenser's recipe recorded, saved and dispensed, and an emag unlocking its toxin.

use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::chem::{Chem, DigestingPill, MobReagents, PillShell, Reagents};
use ss13_game::chem_heater::ChemHeater;
use ss13_game::chem_master::ChemMaster;
use ss13_game::combat::NextMove;
use ss13_game::inventory::Inventory;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::convert::DmTypes;
use ss13_mapload::{DmItem, DmName};
use ss13_net::chat::client::ChatReceived;
use ss13_net::ui::client::UiMessageReceived;
use ss13_shared::TilePos;
use ss13_shared::protocol::ClickModifiers;
use ss13_shared::ui::UiServerMessage;
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::chem_heater::{ChemHeaterData, ChemHeaterUi};
use ss13_tgui::interfaces::chem_master::{ChemMasterData, ChemMasterUi};
use ss13_tgui::interfaces::text_input::TextInputUi;
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
        "inverse_chem_val": 0.25, "purge_multiplier": 1,
        "default_container": { "path": "/obj/item/reagent_containers/cup/bottle" } } },
    "/datum/reagent/water": { "parent": "/datum/reagent", "declared_vars": {
        "name": "Water", "color": "#AAAAAA77" } },
    "/datum/reagent/toxin": { "parent": "/datum/reagent", "declared_vars": { "name": "Toxin" } },
    "/datum/reagent/alpha": { "parent": "/datum/reagent", "declared_vars": { "name": "Alpha" } },
    "/datum/reagent/beta": { "parent": "/datum/reagent", "declared_vars": { "name": "Beta" } },
    "/datum/reagent/gamma": { "parent": "/datum/reagent", "declared_vars": { "name": "Gamma" } },
    "/datum/reagent/reaction_agent": { "parent": "/datum/reagent" },
    "/datum/reagent/reaction_agent/acidic_buffer": { "parent": "/datum/reagent/reaction_agent",
        "declared_vars": { "name": "Strong Acidic Buffer", "ph": 3 } },
    "/datum/reagent/reaction_agent/basic_buffer": { "parent": "/datum/reagent/reaction_agent",
        "declared_vars": { "name": "Strong Basic Buffer", "ph": 11 } },
    "/datum/chemical_reaction": { "parent": "/datum", "declared_vars": {
        "H_ion_release": 0, "determin_ph_range": 4, "is_cold_recipe": 0,
        "mix_message": "The solution begins to bubble.", "mob_react": 1, "optimal_ph_max": 14,
        "optimal_ph_min": 0, "optimal_temp": 500, "overheat_temp": 900, "ph_exponent_factor": 2,
        "purity_min": 0.15, "rate_up_lim": 30, "reaction_flags": 0,
        "required_catalysts": { "list": [] }, "required_container": null,
        "required_container_accepts_subtypes": 0, "required_reagents": { "list": [] },
        "required_temp": 100, "results": { "list": [] }, "temp_exponent_factor": 2,
        "thermic_constant": 0 } },
    "/datum/chemical_reaction/gamma": { "parent": "/datum/chemical_reaction",
        "declared_vars": { "required_temp": 400, "optimal_temp": 450,
        "results": { "list": [{ "assoc": [{ "path": "/datum/reagent/gamma" }, 2] }] },
        "required_reagents": { "list": [
            { "assoc": [{ "path": "/datum/reagent/alpha" }, 1] },
            { "assoc": [{ "path": "/datum/reagent/beta" }, 1] }] } } },
    "/datum/stock_part": { "parent": "/datum" },
    "/datum/stock_part/capacitor": { "parent": "/datum/stock_part", "declared_vars": {
        "physical_object_type": { "path": "/obj/item/stock_parts/capacitor" } } },
    "/datum/stock_part/servo": { "parent": "/datum/stock_part", "declared_vars": {
        "physical_object_type": { "path": "/obj/item/stock_parts/servo" } } },
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
    "/obj/machinery/chem_dispenser": { "parent": "/obj/machinery", "vars": {
        "name": "chem dispenser" }, "declared_vars": {
        "name": "chem dispenser", "icon": { "resource": "c.dmi" }, "icon_state": "dispenser",
        "amount": 30, "power_cost": 100, "recharge_amount": 300, "dispensed_temperature": 300,
        "base_reagent_purity": 1, "dispensable_reagents": { "list": [] },
        "upgrade_reagents": { "list": [] }, "emagged_reagents": { "list": [] },
        "default_dispensable_reagents": { "list": [{ "path": "/datum/reagent/water" }] },
        "default_upgrade_reagents": { "list": [] },
        "default_emagged_reagents": { "list": [{ "path": "/datum/reagent/toxin" }] },
        "circuit": { "path": "/obj/item/circuitboard/machine/chem_dispenser" } } },
    "/obj/machinery/chem_master": { "parent": "/obj/machinery", "vars": {
        "name": "ChemMaster 3000" }, "declared_vars": { "name": "ChemMaster 3000",
        "icon": { "resource": "c.dmi" }, "icon_state": "chemmaster",
        "circuit": { "path": "/obj/item/circuitboard/machine/chem_master" } } },
    "/obj/machinery/chem_heater": { "parent": "/obj/machinery", "vars": {
        "name": "reaction chamber" }, "declared_vars": { "name": "reaction chamber",
        "icon": { "resource": "c.dmi" }, "icon_state": "mixer0b",
        "circuit": { "path": "/obj/item/circuitboard/machine/chem_heater" } } },
    "/obj/machinery/chem_heater/withbuffer": { "parent": "/obj/machinery/chem_heater" },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null, "w_class": 2,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null, "attack_speed": 8, "tool_behaviour": null,
        "toolspeed": 1 } },
    "/obj/item/stock_parts": { "parent": "/obj/item" },
    "/obj/item/stock_parts/capacitor": { "parent": "/obj/item/stock_parts", "declared_vars": {
        "name": "capacitor", "rating": 1 } },
    "/obj/item/stock_parts/power_store": { "parent": "/obj/item/stock_parts", "declared_vars": {
        "maxcharge": 1000000, "chargerate": 1000000 } },
    "/obj/item/stock_parts/power_store/cell": { "parent": "/obj/item/stock_parts/power_store" },
    "/obj/item/stock_parts/servo": { "parent": "/obj/item/stock_parts", "declared_vars": {
        "name": "servo", "rating": 1 } },
    "/obj/item/stock_parts/micro_laser": { "parent": "/obj/item/stock_parts", "declared_vars": {
        "name": "micro-laser", "rating": 1 } },
    "/obj/item/circuitboard": { "parent": "/obj/item", "declared_vars": { "build_path": null } },
    "/obj/item/circuitboard/machine": { "parent": "/obj/item/circuitboard", "declared_vars": {
        "needs_anchored": 1, "req_components": null, "def_components": null } },
    "/obj/item/circuitboard/machine/chem_dispenser": { "parent": "/obj/item/circuitboard/machine",
        "declared_vars": { "build_path": { "path": "/obj/machinery/chem_dispenser" },
        "req_components": { "list": [
            { "assoc": [{ "path": "/datum/stock_part/capacitor" }, 1] },
            { "assoc": [{ "path": "/obj/item/stock_parts/power_store/cell" }, 1] } ] } } },
    "/obj/item/circuitboard/machine/chem_master": { "parent": "/obj/item/circuitboard/machine",
        "declared_vars": { "build_path": { "path": "/obj/machinery/chem_master" },
        "req_components": { "list": [
            { "assoc": [{ "path": "/obj/item/reagent_containers/cup/beaker" }, 2] },
            { "assoc": [{ "path": "/datum/stock_part/servo" }, 1] } ] } } },
    "/obj/item/circuitboard/machine/chem_heater": { "parent": "/obj/item/circuitboard/machine",
        "declared_vars": { "build_path": { "path": "/obj/machinery/chem_heater" },
        "req_components": { "list": [
            { "assoc": [{ "path": "/datum/stock_part/micro_laser" }, 1] } ] } } },
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
        "vars": { "name": "bottle" }, "declared_vars": { "name": "bottle", "volume": 30 } },
    "/obj/item/reagent_containers/cup/tube": { "parent": "/obj/item/reagent_containers/cup",
        "vars": { "name": "tube" }, "declared_vars": { "name": "tube", "volume": 30,
        "icon_state": "test_tube" } },
    "/obj/item/reagent_containers/cup/beaker": { "parent": "/obj/item/reagent_containers/cup",
        "vars": { "name": "beaker" }, "declared_vars": { "name": "beaker",
        "icon_state": "beaker" } },
    "/obj/item/reagent_containers/cup/beaker/water": {
        "parent": "/obj/item/reagent_containers/cup/beaker", "declared_vars": {
        "list_reagents": { "list": [{ "assoc": [{ "path": "/datum/reagent/water" }, 40] }] } } },
    "/obj/item/reagent_containers/cup/beaker/mix": {
        "parent": "/obj/item/reagent_containers/cup/beaker", "declared_vars": {
        "list_reagents": { "list": [{ "assoc": [{ "path": "/datum/reagent/alpha" }, 10] },
            { "assoc": [{ "path": "/datum/reagent/beta" }, 10] }] } } },
    "/obj/item/reagent_containers/applicator": { "parent": "/obj/item/reagent_containers",
        "declared_vars": { "abstract_type": { "path": "/obj/item/reagent_containers/applicator" },
        "has_variable_transfer_amount": 0 } },
    "/obj/item/reagent_containers/applicator/pill": {
        "parent": "/obj/item/reagent_containers/applicator", "vars": { "name": "pill" },
        "declared_vars": { "name": "pill", "icon_state": "pill", "volume": 50 } },
    "/obj/item/reagent_containers/applicator/pill/style": {
        "parent": "/obj/item/reagent_containers/applicator/pill",
        "declared_vars": { "icon_state": "pill0" } },
    "/obj/item/reagent_containers/applicator/pill/style/red": {
        "parent": "/obj/item/reagent_containers/applicator/pill/style",
        "declared_vars": { "icon_state": "pill1" } },
    "/obj/item/card/emag": { "parent": "/obj/item/card", "vars": { "name": "cryptographic sequencer" },
        "declared_vars": { "name": "cryptographic sequencer", "icon_state": "emag" } },
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
  }
}"##;

/// Rows top-down, the last line is y = 1. The spawn `s` (3,3) with the beakers, the ChemMaster
/// east of it and the reaction chamber (with buffers) west.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/obj/item/reagent_containers/cup/beaker/water,/obj/item/reagent_containers/cup/beaker/mix,/obj/item/reagent_containers/cup/beaker,/obj/item/card/emag,/turf/open/floor,/area/space)
"D" = (/obj/machinery/chem_dispenser,/turf/open/floor,/area/space)
"M" = (/obj/machinery/chem_master,/turf/open/floor,/area/space)
"H" = (/obj/machinery/chem_heater/withbuffer,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwww
wfDfw
wHsMw
wfffw
wwwww
"}
"##;

const SPAWN: TilePos = TilePos::new(3, 3, 1);
const MASTER: TilePos = TilePos::new(4, 3, 1);
const HEATER: TilePos = TilePos::new(2, 3, 1);
const DISPENSER: TilePos = TilePos::new(3, 4, 1);
const PILL: &str = "/obj/item/reagent_containers/applicator/pill/style/red";
const WATER: &str = "/datum/reagent/water";

/// Manifest order = `IconStateId`.
const STATES: [(&str, &str); 14] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("c.dmi", "chemmaster"),
    ("c.dmi", "mixer0b"),
    ("c.dmi", "mixer1b"),
    ("c.dmi", "beaker"),
    ("c.dmi", "test_tube"),
    ("c.dmi", "pill0"),
    ("c.dmi", "pill1"),
    ("i.dmi", "toy"),
    ("c.dmi", "dispenser"),
    ("i.dmi", "emag"),
];

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("chem_machines_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/machinery/chem_master",
        "/obj/machinery/chem_heater/withbuffer",
        "/obj/machinery/chem_dispenser",
        "/obj/item/card/emag",
        "/obj/item/stock_parts/capacitor",
        "/obj/item/stock_parts/power_store/cell",
        "/obj/item/circuitboard/machine/chem_dispenser",
        "/obj/item/reagent_containers/cup/beaker/water",
        "/obj/item/reagent_containers/cup/beaker/mix",
        "/obj/item/reagent_containers/cup/beaker",
        "/obj/item/reagent_containers/cup/tube",
        "/obj/item/reagent_containers/applicator/pill/style",
        PILL,
        "/obj/item/stock_parts/servo",
        "/obj/item/stock_parts/micro_laser",
        "/obj/item/circuitboard/machine/chem_master",
        "/obj/item/circuitboard/machine/chem_heater",
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

/// The station and the acts sent since the last pause.
struct Station(testing::Station, u32);

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
        let chat = &self.players[0].app.world().resource::<Received>().chat;
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
        Self(s, 0)
    }

    fn click_with(&mut self, entity: Entity, modifiers: ClickModifiers) {
        let mob = self.players[0].mob;
        self.run_until("off cooldown", |s| s.now() >= s.get::<NextMove>(mob).0);
        self.0.click_with(0, entity, modifiers);
    }

    fn click(&mut self, entity: Entity) {
        self.click_with(entity, ClickModifiers::default());
    }

    /// The server takes 10 acts a second per client (`ACTS_PER_SECOND`; the test's clock is
    /// the manual tick for real time too): after 8, a pause of more than a second.
    fn act(&mut self, window: u32, action: &str, params: &[(&str, &str)]) {
        if self.1 >= 8 {
            for _ in 0..25 {
                self.tick();
            }
            self.1 = 0;
        }
        self.1 += 1;
        self.0.act(0, window, action, params);
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

    fn window_id(&self, name: &str) -> Option<u32> {
        let received = self.players[0].app.world().resource::<Received>();
        received
            .windows
            .iter()
            .filter(|(_, (n, _))| n == name)
            .map(|(id, _)| *id)
            .max()
    }

    fn items(&self, pos: TilePos, path: &str) -> Vec<Entity> {
        let world = self.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .filter(|&e| world.get::<DmItem>(e).is_some_and(|i| i.path == path))
            .collect()
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

    fn amount(&self, container: Entity, path: &str) -> f32 {
        let id = self.server.world().resource::<Chem>().0.id(path).unwrap();
        self.get::<Reagents>(container).0.amount(id)
    }
}

#[test]
fn chem_master_prints_pills_with_a_sugar_shell() {
    let mut s = Station::start("chem_master_prints_pills_with_a_sugar_shell");
    let mob = s.players[0].mob;
    let master = s.machine::<ChemMaster>(MASTER);
    // `RefreshParts`: the buffer is its two 50 u beakers.
    assert_eq!(s.get::<Reagents>(master).0.maximum_volume, 100.0);
    let beaker = s.items(SPAWN, "/obj/item/reagent_containers/cup/beaker/water")[0];
    s.take(beaker);
    s.click(master);
    s.run_until("the beaker in the window", |s| {
        s.window::<ChemMasterUi>()
            .is_some_and(|(_, d)| d.beaker.is_some())
    });
    let (window, data): (u32, ChemMasterData) = s.window::<ChemMasterUi>().unwrap();
    assert_eq!(data.beaker.as_ref().unwrap().contents[0].name, "Water");
    // Water's `default_container` is a bottle, not printable here: the tube is suggested.
    assert_eq!(
        data.suggested_container_ref,
        "/obj/item/reagent_containers/cup/tube"
    );

    // 10 into the buffer, then "All"; a reagent that is not there and a bad target move nothing.
    s.act(
        window,
        "transfer",
        &[
            ("reagentRef", WATER),
            ("amount", "10"),
            ("target", "buffer"),
        ],
    );
    s.run_until("10 in the buffer", |s| s.amount(master, WATER) == 10.0);
    s.act(
        window,
        "transfer",
        &[
            ("reagentRef", "/datum/reagent/nope"),
            ("amount", "5"),
            ("target", "buffer"),
        ],
    );
    s.act(
        window,
        "transfer",
        &[("reagentRef", WATER), ("amount", "5"), ("target", "space")],
    );
    s.act(
        window,
        "transfer",
        &[
            ("reagentRef", WATER),
            ("amount", "1000"),
            ("target", "buffer"),
        ],
    );
    s.run_until("all in the buffer", |s| s.amount(master, WATER) == 40.0);
    assert_eq!(s.amount(beaker, WATER), 0.0);
    // Destroying mode: 5 back "to the beaker" is deleted.
    s.act(window, "toggleTransferMode", &[]);
    s.act(
        window,
        "transfer",
        &[("reagentRef", WATER), ("amount", "5"), ("target", "beaker")],
    );
    s.run_until("5 destroyed", |s| s.amount(master, WATER) == 35.0);
    assert_eq!(s.amount(beaker, WATER), 0.0);

    // An unknown container is refused, the pill style is taken; the shell is capped at 30 s.
    s.act(window, "selectContainer", &[("ref", "/obj/item/toy")]);
    s.act(window, "selectContainer", &[("ref", PILL)]);
    s.act(window, "setPillDuration", &[("duration", "99")]);
    s.run_until("pills selected", |s| {
        let m = s.get::<ChemMaster>(master);
        m.selected == PILL && m.pill_layers == 30.0
    });
    s.act(window, "setPillDuration", &[("duration", "4")]);
    s.act(window, "create", &[("itemCount", "2")]);
    s.run_until("the name prompt", |s| {
        s.window_id(TextInputUi::NAME).is_some()
    });
    let prompt = s.window_id(TextInputUi::NAME).unwrap();
    s.act(prompt, "submit", &[("entry", "test pill")]);
    s.run_until_for(5.0, "two pills printed", |s| {
        s.items(MASTER, PILL).len() == 2 && s.get::<ChemMaster>(master).printing.is_none()
    });
    let pills = s.items(MASTER, PILL);
    for &p in &pills {
        assert_eq!(s.get::<DmName>(p).name, "test pill");
        assert_eq!(s.amount(p, WATER), 17.5);
        assert_eq!(*s.get::<PillShell>(p), PillShell(4.0));
    }
    assert_eq!(s.get::<Reagents>(master).0.total_volume, 0.0);

    // Swallowed: the shell holds the water out of the stomach for its 4 s.
    s.act(window, "eject", &[]);
    s.run_until("the beaker in hand", |s| {
        s.get::<Inventory>(mob).active_item() == Some(beaker)
    });
    s.send(0, ss13_shared::protocol::ClientIntent::DropActive);
    s.run_until("dropped", |s| {
        s.get::<Inventory>(mob).active_item().is_none()
    });
    let pill = pills[0];
    s.take(pill);
    s.click(mob);
    s.run_until("swallowed", |s| {
        s.server.world().get::<DigestingPill>(pill).is_some()
    });
    let swallowed = s.now();
    let stomach_water = |s: &Station| {
        let id = s.server.world().resource::<Chem>().0.id(WATER).unwrap();
        s.get::<MobReagents>(mob).stomach.amount(id)
    };
    assert_eq!(stomach_water(&s), 0.0);
    s.run_until_for(10.0, "digested", |s| {
        s.server.world().get_entity(pill).is_err()
    });
    assert!(s.now() - swallowed >= 3.9, "{}", s.now() - swallowed);
    assert!(stomach_water(&s) > 15.0, "{}", stomach_water(&s));
}

#[test]
fn reaction_chamber_heats_until_the_reaction_runs() {
    let mut s = Station::start("reaction_chamber_heats_until_the_reaction_runs");
    let heater = s.machine::<ChemHeater>(HEATER);
    let beaker = s.items(SPAWN, "/obj/item/reagent_containers/cup/beaker/mix")[0];
    // At room temperature nothing reacts.
    assert_eq!(s.amount(beaker, "/datum/reagent/gamma"), 0.0);
    s.take(beaker);
    s.click(heater);
    s.run_until("the beaker in the window", |s| {
        s.window::<ChemHeaterUi>()
            .is_some_and(|(_, d)| d.beaker.is_some())
    });
    let (window, data): (u32, ChemHeaterData) = s.window::<ChemHeaterUi>().unwrap();
    // `withbuffer`: 20 u of each buffer.
    assert_eq!(
        (data.acidic_buffer_vol, data.basic_buffer_vol),
        (20.0, 20.0)
    );
    s.act(window, "acidBuffer", &[("target", "1")]);
    s.run_until("1 u of acidic buffer injected", |s| {
        s.amount(beaker, "/datum/reagent/reaction_agent/acidic_buffer") == 1.0
    });
    assert_eq!(
        s.amount(heater, "/datum/reagent/reaction_agent/acidic_buffer"),
        19.0
    );
    // Out of range targets are clamped; on, it heats towards 500 K and Gamma forms past 400 K.
    s.act(window, "temperature", &[("target", "5000")]);
    s.run_until("clamped", |s| {
        s.get::<ChemHeater>(heater).target_temperature == 1000.0
    });
    s.act(window, "temperature", &[("target", "500")]);
    s.act(window, "power", &[]);
    s.run_until("on", |s| s.get::<ChemHeater>(heater).on);
    s.run_until_for(40.0, "gamma formed", |s| {
        s.amount(beaker, "/datum/reagent/gamma") >= 19.0
    });
    let temp = s.get::<Reagents>(beaker).0.chem_temp;
    assert!(temp > 400.0 && temp <= 500.0, "{temp}");
}

#[test]
fn dispenser_recipes_and_emag() {
    use ss13_game::chem_dispenser::ChemDispenser;
    use ss13_tgui::interfaces::alert::AlertUi;
    use ss13_tgui::interfaces::chem_dispenser::{ChemDispenserData, ChemDispenserUi};

    let mut s = Station::start("dispenser_recipes_and_emag");
    let mob = s.players[0].mob;
    let dispenser = s.machine::<ChemDispenser>(DISPENSER);
    let beaker = s.items(SPAWN, "/obj/item/reagent_containers/cup/beaker")[0];
    s.take(beaker);
    s.click(dispenser);
    s.run_until("the beaker in the window", |s| {
        s.window::<ChemDispenserUi>()
            .is_some_and(|(_, d)| d.beaker.is_some())
    });
    let (window, data): (u32, ChemDispenserData) = s.window::<ChemDispenserUi>().unwrap();
    assert!(data.show_ph, "a chem dispenser shows the pH");
    assert_eq!(data.chemicals[0].ph_col, "green");

    // Recording: dispensing adds up in the virtual beaker, nothing reaches the real one.
    s.act(window, "record_recipe", &[]);
    s.act(window, "amount", &[("target", "10")]);
    s.act(window, "dispense", &[("reagent", "Water")]);
    s.act(window, "dispense", &[("reagent", "Water")]);
    s.run_until("20 recorded", |s| {
        s.get::<ChemDispenser>(dispenser).recording == Some(vec![("Water".into(), 20.0)])
    });
    assert_eq!(s.amount(beaker, WATER), 0.0);
    s.act(window, "save_recording", &[]);
    s.run_until("the name prompt", |s| {
        s.window_id(TextInputUi::NAME).is_some()
    });
    let prompt = s.window_id(TextInputUi::NAME).unwrap();
    s.act(prompt, "submit", &[("entry", "twenty water")]);
    s.run_until("saved", |s| {
        let d = s.get::<ChemDispenser>(dispenser);
        d.recording.is_none() && d.saved_recipes.len() == 1
    });
    // An unknown recipe does nothing; the saved one dispenses its 20.
    s.act(window, "dispense_recipe", &[("recipe", "nope")]);
    s.act(window, "dispense_recipe", &[("recipe", "twenty water")]);
    s.run_until("the recipe dispensed", |s| s.amount(beaker, WATER) == 20.0);

    // Clearing asks first.
    s.act(window, "clear_recipes", &[]);
    s.run_until("the alert", |s| s.window_id(AlertUi::NAME).is_some());
    let alert = s.window_id(AlertUi::NAME).unwrap();
    s.act(alert, "choose", &[("choice", "Yes")]);
    s.run_until("cleared", |s| {
        s.get::<ChemDispenser>(dispenser).saved_recipes.is_empty()
    });

    // The emag adds the toxin, once.
    s.act(window, "eject", &[]);
    s.run_until("the beaker back", |s| {
        s.get::<Inventory>(mob).active_item() == Some(beaker)
    });
    s.send(0, ss13_shared::protocol::ClientIntent::DropActive);
    s.run_until("dropped", |s| {
        s.get::<Inventory>(mob).active_item().is_none()
    });
    let emag = s.items(SPAWN, "/obj/item/card/emag")[0];
    s.take(emag);
    s.click(dispenser);
    s.run_until("emagged", |s| s.get::<ChemDispenser>(dispenser).emagged);
    let chem = s.server.world().resource::<Chem>().0.clone();
    let names: Vec<String> = s
        .get::<ChemDispenser>(dispenser)
        .dispensable
        .iter()
        .map(|&id| chem.def(id).name.clone())
        .collect();
    assert_eq!(names, ["Water", "Toxin"]);
}
