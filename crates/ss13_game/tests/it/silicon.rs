//! Silicons over real WebTransport on 127.0.0.1, on a fixture station: the AI job spawns the AI
//! on its landmark and the client controls the eye (the viewer follows it through walls); the
//! eye's CameraView covers what the room's camera sees; a shift-click opens a door the camera
//! sees, not one it does not, and nothing once the AI wire is cut; ctrl drops the bolts. A camera
//! console moves its user's viewer to the chosen camera and back when closed. A cyborg links to
//! the AI and takes its laws, picks a model, gets its modules, and charges in a station.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::doors::{Door, DoorState};
use ss13_game::inventory::Inventory;
use ss13_game::jobs::Jobs;
use ss13_game::power::Apc;
use ss13_game::server::Players;
use ss13_game::silicon::ai::{AiCore, AiEye};
use ss13_game::silicon::cameras::{Camera, CameraNet};
use ss13_game::silicon::cyborg::{Cyborg, RechargeStation};
use ss13_game::silicon::laws::Laws;
use ss13_game::silicon::ui::{
    CameraConsoleStatic, CameraConsoleUi, CameraViewData, CameraViewUi, ListInputUi,
};
use ss13_game::storage::StoredIn;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_game::wires::{WIRE_AI, Wires};
use ss13_mapload::convert::DmTypes;
use ss13_mapload::jobs::{Job, JobsData};
use ss13_net::chat::client::ChatReceived;
use ss13_net::client::LocalControl;
use ss13_net::server::Viewer;
use ss13_net::ui::client::UiMessageReceived;
use ss13_shared::chat::ChatChannel;
use ss13_shared::protocol::{ClickModifiers, ClientIntent, DragEnd, InventorySlot};
use ss13_shared::ui::UiServerMessage;
use ss13_shared::{Dir, TilePos};
use ss13_tgui::interfaces::ai_airlock::{AiAirlockData, AiAirlockUi};
use ss13_tgui::interfaces::apc::{ApcData, ApcUi};
use ss13_tgui::{UiInterface, decode};
use ss13_world::SpatialIndex;

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/datum/ai_laws": { "parent": "/datum", "declared_vars": { "id": "default",
        "name": "Unknown Laws", "inherent": { "list": [] }, "zeroth": null, "zeroth_borg": null } },
    "/datum/ai_laws/crewsimovpp": { "parent": "/datum/ai_laws", "declared_vars": {
        "id": "crewsimovpp", "name": "Крюзимов++", "inherent": { "list": ["L1", "L2", "L3"] } } },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3 } },
    "/obj": { "parent": "/atom/movable" },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start",
        "declared_vars": { "name": "Assistant" } },
    "/obj/effect/landmark/start/ai": { "parent": "/obj/effect/landmark/start",
        "declared_vars": { "name": "AI" } },
    "/obj/item": { "parent": "/obj", "declared_vars": { "interaction_flags_item": 1,
        "icon": { "resource": "i.dmi" }, "icon_state": "tool", "slot_flags": 0,
        "tool_behaviour": null, "toolspeed": 1, "heat": 0, "siemens_coefficient": 1 } },
    "/obj/item/screwdriver": { "parent": "/obj/item", "declared_vars": {
        "name": "screwdriver", "tool_behaviour": "screwdriver" } },
    "/obj/item/multitool": { "parent": "/obj/item", "declared_vars": {
        "name": "multitool", "tool_behaviour": "multitool" } },
    "/obj/item/robot_model": { "parent": "/obj/item", "declared_vars": { "name": "Default",
        "cyborg_base_icon": "robot", "basic_modules": { "list": [] } } },
    "/obj/item/robot_model/engineering": { "parent": "/obj/item/robot_model", "declared_vars": {
        "name": "Engineering", "cyborg_base_icon": "engineer",
        "basic_modules": { "list": [{ "path": "/obj/item/screwdriver" }, { "path": "/obj/item/multitool" }] },
        "radio_channels": { "list": ["Инженерия"] } } },
    "/obj/item/ai_module": { "parent": "/obj/item", "declared_vars": { "name": "AI module" } },
    "/obj/item/ai_module/law": { "parent": "/obj/item/ai_module", "declared_vars": {
        "laws": { "list": [] } } },
    "/obj/item/ai_module/law/core": { "parent": "/obj/item/ai_module/law" },
    "/obj/item/ai_module/law/core/full": { "parent": "/obj/item/ai_module/law/core",
        "declared_vars": { "law_id": null } },
    "/obj/item/ai_module/law/core/full/crewsimovpp": { "parent": "/obj/item/ai_module/law/core/full",
        "declared_vars": { "name": "crew module", "law_id": "crewsimovpp" } },
    "/obj/item/ai_module/law/supplied": { "parent": "/obj/item/ai_module/law" },
    "/obj/item/ai_module/law/supplied/freeform": { "parent": "/obj/item/ai_module/law/supplied",
        "declared_vars": { "name": "freeform module", "laws": { "list": [""] } } },
    "/obj/item/card": { "parent": "/obj/item" },
    "/obj/item/card/id": { "parent": "/obj/item/card", "declared_vars": { "slot_flags": 256 } },
    "/obj/item/card/id/advanced": { "parent": "/obj/item/card/id" },
    "/obj/item/clothing": { "parent": "/obj/item" },
    "/obj/item/clothing/under": { "parent": "/obj/item/clothing" },
    "/obj/item/clothing/under/color": { "parent": "/obj/item/clothing/under" },
    "/obj/item/clothing/under/color/grey": { "parent": "/obj/item/clothing/under/color" },
    "/obj/item/clothing/shoes": { "parent": "/obj/item/clothing" },
    "/obj/item/clothing/shoes/sneakers": { "parent": "/obj/item/clothing/shoes" },
    "/obj/item/clothing/shoes/sneakers/black": { "parent": "/obj/item/clothing/shoes/sneakers" },
    "/obj/machinery": { "parent": "/obj", "declared_vars": {
        "use_power": 1, "idle_power_usage": 100, "active_power_usage": 1000, "power_channel": 1 } },
    "/obj/machinery/camera": { "parent": "/obj/machinery", "declared_vars": {
        "name": "security camera", "icon": { "resource": "c.dmi" }, "icon_state": "camera",
        "c_tag": null, "network": { "list": ["ss13"] }, "view_range": 7, "camera_enabled": 1 } },
    "/obj/machinery/computer": { "parent": "/obj/machinery", "declared_vars": {
        "icon": { "resource": "computer.dmi" }, "icon_state": "computer", "density": 1 } },
    "/obj/machinery/computer/security": { "parent": "/obj/machinery/computer", "declared_vars": {
        "name": "security camera console", "network": { "list": ["ss13"] } } },
    "/obj/machinery/computer/station_alert": { "parent": "/obj/machinery/computer",
        "declared_vars": { "name": "station alert console" } },
    "/obj/machinery/ai_law_rack": { "parent": "/obj/machinery", "declared_vars": {
        "name": "module rack", "icon": { "resource": "computer.dmi" }, "icon_state": "computer",
        "density": 1 } },
    "/obj/machinery/ai_law_rack/base": { "parent": "/obj/machinery/ai_law_rack" },
    "/obj/machinery/ai_law_rack/base/core": { "parent": "/obj/machinery/ai_law_rack/base",
        "declared_vars": { "name": "core module rack" } },
    "/obj/machinery/holopad": { "parent": "/obj/machinery", "declared_vars": {
        "name": "holopad", "icon": { "resource": "floor.dmi" }, "icon_state": "holopad0" } },
    "/obj/machinery/ai_slipper": { "parent": "/obj/machinery", "declared_vars": {
        "name": "foam dispenser", "icon": { "resource": "tool.dmi" },
        "icon_state": "ai-slipper0", "uses": 20 } },
    "/obj/machinery/recharge_station": { "parent": "/obj/machinery", "declared_vars": {
        "name": "recharging station", "icon": { "resource": "borg_charger.dmi" },
        "icon_state": "borgcharger0" } },
    "/obj/machinery/door": { "parent": "/obj/machinery", "declared_vars": {
        "name": "door", "icon": { "resource": "d.dmi" }, "icon_state": "door_closed",
        "density": 1, "opacity": 1,
        "layer": 2.7, "base_icon_state": "door", "autoclose": 0, "can_open_with_hands": 1,
        "closingLayer": 3.1, "glass": 0, "locked": 0, "safe": 1, "visible": 1, "welded": 0,
        "power_channel": 3, "idle_power_usage": 10, "active_power_usage": 200 } },
    "/obj/machinery/door/airlock": { "parent": "/obj/machinery/door", "declared_vars": {
        "name": "airlock", "icon": { "resource": "a.dmi" }, "icon_state": "closed",
        "autoclose": 1, "base_icon_state": null, "normalspeed": 1,
        "overlays_file": { "resource": "o.dmi" }, "airlock_material": null } },
    "/obj/item/stock_parts": { "parent": "/obj/item" },
    "/obj/item/stock_parts/power_store": { "parent": "/obj/item/stock_parts", "declared_vars": {
        "maxcharge": 10000, "chargerate": 500 } },
    "/obj/item/stock_parts/power_store/battery": { "parent": "/obj/item/stock_parts/power_store",
        "declared_vars": { "maxcharge": 1000000, "chargerate": 1000000 } },
    "/obj/item/stock_parts/power_store/battery/upgraded": { "parent": "/obj/item/stock_parts/power_store/battery",
        "declared_vars": { "maxcharge": 2500000, "chargerate": 500000 } },
    "/obj/machinery/power": { "parent": "/obj/machinery", "declared_vars": {
        "use_power": 0, "idle_power_usage": 0, "active_power_usage": 0, "cable_layer": 2 } },
    "/obj/machinery/power/apc": { "parent": "/obj/machinery/power", "declared_vars": {
        "name": "APC", "icon": { "resource": "apc.dmi" }, "icon_state": "apc0", "layer": 4,
        "cell_type": {"path": "/obj/item/stock_parts/power_store/battery/upgraded"},
        "start_charge": 90, "lighting": 3, "equipment": 3, "environ": 3, "operating": 1,
        "chargemode": 1, "areastring": null } },
    "/obj/structure": { "parent": "/obj" },
    "/obj/structure/ai_core": { "parent": "/obj/structure", "declared_vars": {
        "name": "AI core", "icon": { "resource": "ai.dmi" }, "icon_state": "build_0",
        "density": 1 } },
    "/mob": { "parent": "/atom/movable", "declared_vars": { "layer": 4, "density": 1 } },
    "/mob/living": { "parent": "/mob" },
    "/mob/living/carbon": { "parent": "/mob/living" },
    "/mob/living/carbon/human": { "parent": "/mob/living/carbon", "declared_vars": {
        "icon": { "resource": "h.dmi" }, "icon_state": "human_basic" } },
    "/mob/living/silicon": { "parent": "/mob/living" },
    "/mob/living/silicon/ai": { "parent": "/mob/living/silicon", "declared_vars": {
        "name": "AI", "icon": { "resource": "ai.dmi" }, "icon_state": "ai-core" } },
    "/mob/living/silicon/robot": { "parent": "/mob/living/silicon", "declared_vars": {
        "name": "Cyborg", "icon": { "resource": "robots.dmi" }, "icon_state": "robot" } },
    "/turf": { "parent": "/atom", "declared_vars": { "icon": { "resource": "t.dmi" } } },
    "/turf/open": { "parent": "/turf" },
    "/turf/open/floor": { "parent": "/turf/open", "declared_vars": { "icon_state": "floor" } },
    "/turf/open/space": { "parent": "/turf/open" },
    "/turf/open/space/basic": { "parent": "/turf/open/space" },
    "/turf/closed": { "parent": "/turf", "declared_vars": { "density": 1, "opacity": 1 } },
    "/turf/closed/wall": { "parent": "/turf/closed", "declared_vars": { "icon_state": "wall" } },
    "/area": { "parent": "/atom", "declared_vars": { "requires_power": 0 } },
    "/area/space": { "parent": "/area" },
    "/area/station": { "parent": "/area" },
    "/area/station/ai": { "parent": "/area/station", "declared_vars": { "requires_power": 1 } }
  }
}"##;

/// Rows top-down, y = 1 at the bottom. Room 1 (x 2-3): the assistant start `s` (2,3), the
/// camera `1` (2,4), the console `c` (3,4). Door `D` (4,3) to room 2 (x 5-7): the AI start `A`
/// (6,3), the recharging station `R` (6,2). Door `E` (8,3) to room 3, which no camera sees.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/station)
"f" = (/turf/open/floor,/area/station)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/station)
"1" = (/obj/machinery/camera{c_tag = "Room One"},/turf/open/floor,/area/station)
"c" = (/obj/machinery/computer/security,/turf/open/floor,/area/station)
"D" = (/obj/machinery/door/airlock,/turf/open/floor,/area/station)
"E" = (/obj/machinery/door/airlock,/turf/open/floor,/area/station)
"A" = (/obj/effect/landmark/start/ai,/turf/open/floor,/area/station)
"R" = (/obj/machinery/recharge_station,/turf/open/floor,/area/station)

(1,1,1) = {"
wwwwwwwwwww
w1cwfffwffw
wsfDfAfEffw
wffwfRfwffw
wwwwwwwwwww
"}
"##;

/// [`MAP`] with an APC `P` (3,2) in room 1, which the camera sees.
const MAP_APC: &str = r##""w" = (/turf/closed/wall,/area/station)
"f" = (/turf/open/floor,/area/station)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/station)
"1" = (/obj/machinery/camera{c_tag = "Room One"},/turf/open/floor,/area/station)
"c" = (/obj/machinery/computer/security,/turf/open/floor,/area/station)
"D" = (/obj/machinery/door/airlock,/turf/open/floor,/area/station)
"E" = (/obj/machinery/door/airlock,/turf/open/floor,/area/station)
"A" = (/obj/effect/landmark/start/ai,/turf/open/floor,/area/station)
"R" = (/obj/machinery/recharge_station,/turf/open/floor,/area/station)
"P" = (/obj/machinery/power/apc,/turf/open/floor,/area/station)

(1,1,1) = {"
wwwwwwwwwww
w1cwfffwffw
wsfDfAfEffw
wfPwfRfwffw
wwwwwwwwwww
"}
"##;
const APC_POS: TilePos = TilePos::new(3, 2, 1);

/// [`MAP`] with a core law rack `K` (2,2) right below the assistant start.
const MAP_RACK: &str = r##""w" = (/turf/closed/wall,/area/station)
"f" = (/turf/open/floor,/area/station)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/station)
"1" = (/obj/machinery/camera{c_tag = "Room One"},/turf/open/floor,/area/station)
"c" = (/obj/machinery/computer/security,/turf/open/floor,/area/station)
"D" = (/obj/machinery/door/airlock,/turf/open/floor,/area/station)
"E" = (/obj/machinery/door/airlock,/turf/open/floor,/area/station)
"A" = (/obj/effect/landmark/start/ai,/turf/open/floor,/area/station)
"R" = (/obj/machinery/recharge_station,/turf/open/floor,/area/station)
"K" = (/obj/machinery/ai_law_rack/base/core,/turf/open/floor,/area/station)

(1,1,1) = {"
wwwwwwwwwww
w1cwfffwffw
wsfDfAfEffw
wKfwfRfwffw
wwwwwwwwwww
"}
"##;
const RACK_POS: TilePos = TilePos::new(2, 2, 1);

/// [`MAP`] with a foam dispenser `F` (2,2) of 3 uses and a holopad `H` (3,2) in room 1, which
/// the camera sees.
const MAP_HOLO: &str = r##""w" = (/turf/closed/wall,/area/station)
"f" = (/turf/open/floor,/area/station)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/station)
"1" = (/obj/machinery/camera{c_tag = "Room One"},/turf/open/floor,/area/station)
"c" = (/obj/machinery/computer/security,/turf/open/floor,/area/station)
"D" = (/obj/machinery/door/airlock,/turf/open/floor,/area/station)
"E" = (/obj/machinery/door/airlock,/turf/open/floor,/area/station)
"A" = (/obj/effect/landmark/start/ai,/turf/open/floor,/area/station)
"R" = (/obj/machinery/recharge_station,/turf/open/floor,/area/station)
"F" = (/obj/machinery/ai_slipper{uses = 3},/turf/open/floor,/area/station)
"H" = (/obj/machinery/holopad,/turf/open/floor,/area/station)

(1,1,1) = {"
wwwwwwwwwww
w1cwfffwffw
wsfDfAfEffw
wFHwfRfwffw
wwwwwwwwwww
"}
"##;
const SLIPPER_POS: TilePos = TilePos::new(2, 2, 1);
const PAD_POS: TilePos = TilePos::new(3, 2, 1);

const SPAWN: TilePos = TilePos::new(2, 3, 1);
const CAMERA: TilePos = TilePos::new(2, 4, 1);
const CONSOLE: TilePos = TilePos::new(3, 4, 1);
const DOOR_SEEN: TilePos = TilePos::new(4, 3, 1);
const DOOR_UNSEEN: TilePos = TilePos::new(8, 3, 1);
const AI_START: TilePos = TilePos::new(6, 3, 1);
/// Right behind the seen door, in room 2: the camera sees it only while the door is open.
const ITEM_BEHIND_DOOR: TilePos = TilePos::new(5, 3, 1);
const STATION: TilePos = TilePos::new(6, 2, 1);

const STATES: [(&str, &str); 26] = [
    ("apc.dmi", "apc0"),
    ("floor.dmi", "holopad0"),
    ("floor.dmi", "holopad1"),
    ("tool.dmi", "ai-slipper0"),
    ("tool.dmi", "ai-slipper1"),
    ("ai.dmi", "build_0"),
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "tool"),
    ("a.dmi", "closed"),
    ("a.dmi", "opening"),
    ("a.dmi", "open"),
    ("a.dmi", "closing"),
    ("c.dmi", "camera"),
    ("computer.dmi", "computer"),
    ("ai.dmi", "ai-core"),
    ("icons/mob/silicon/ai.dmi", "ai"),
    ("icons/mob/eyemob.dmi", "ai_camera"),
    ("robots.dmi", "robot"),
    ("icons/mob/silicon/robots.dmi", "engineer"),
    ("borg_charger.dmi", "borgcharger0"),
    ("icons/obj/machines/borg_charger.dmi", "borgcharger0"),
    ("icons/obj/machines/borg_charger.dmi", "borgcharger1"),
    ("icons/obj/machines/borg_charger.dmi", "borgcharger2"),
];

fn fixture_assets_on(test: &str, map: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("silicon_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/effect/landmark/start/ai",
        "/obj/machinery/camera",
        "/obj/machinery/holopad",
        "/obj/machinery/ai_slipper",
        "/obj/machinery/computer/security",
        "/obj/machinery/computer/station_alert",
        "/obj/machinery/recharge_station",
        "/obj/machinery/door/airlock",
        "/obj/machinery/power/apc",
        "/obj/item/screwdriver",
        "/obj/item/multitool",
        "/obj/item/robot_model/engineering",
        "/obj/machinery/ai_law_rack/base/core",
        "/obj/item/ai_module/law/core/full/crewsimovpp",
        "/obj/item/ai_module/law/supplied/freeform",
        "/mob/living/silicon/ai",
        "/mob/living/silicon/robot",
        "/obj/structure/ai_core",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
        "/area/station",
        "/area/station/ai",
    ];
    write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", map)]);
    let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
    std::fs::write(
        dir.join(ss13_mapload::silicon::SILICON_FILE),
        ss13_mapload::silicon::resolve(&types).to_ron(),
    )
    .unwrap();
    dir
}

fn jobs() -> JobsData {
    let mut data = ss13_game::jobs::fallback_jobs();
    for (path, title) in [("/datum/job/ai", "AI"), ("/datum/job/cyborg", "Cyborg")] {
        data.jobs.push(Job {
            path: path.into(),
            title: title.into(),
            total_positions: 1,
            spawn_positions: 1,
            ..default()
        });
    }
    data
}

/// The windows the client's tgui got, by interface: (window id, static data, data, open).
#[derive(Resource, Default)]
struct Windows(Vec<(u32, String, Vec<u8>, Vec<u8>, bool)>);

impl Windows {
    fn get(&self, interface: &str) -> Option<&(u32, String, Vec<u8>, Vec<u8>, bool)> {
        self.0.iter().rev().find(|w| w.1 == interface && w.4)
    }
}

fn record_windows(mut received: MessageReader<UiMessageReceived>, mut w: ResMut<Windows>) {
    for UiMessageReceived(message) in received.read() {
        match message {
            UiServerMessage::Open {
                window,
                interface,
                static_data,
                data,
                ..
            } => {
                w.0.retain(|x| x.0 != *window);
                w.0.push((
                    *window,
                    interface.clone(),
                    static_data.clone(),
                    data.clone(),
                    true,
                ));
            }
            UiServerMessage::Update {
                window,
                data: Some(data),
                ..
            } => {
                if let Some(x) = w.0.iter_mut().find(|x| x.0 == *window) {
                    x.3 = data.clone();
                }
            }
            UiServerMessage::Close { window } => {
                if let Some(x) = w.0.iter_mut().find(|x| x.0 == *window) {
                    x.4 = false;
                }
            }
            _ => {}
        }
    }
}

/// Chat lines a client received, with their channel.
#[derive(Resource, Default)]
struct Heard(Vec<(ChatChannel, String)>);

fn record_chat(mut chat: MessageReader<ChatReceived>, mut heard: ResMut<Heard>) {
    for ChatReceived(message) in chat.read() {
        heard.0.push((message.channel, message.plain_text()));
    }
}

/// The AI's right click on a holopad the camera sees projects its hologram there and moves the
/// eye onto the pad; the hologram follows the eye within the pad's range and goes, with the
/// eye's camera-only senses back, once the eye leaves it. The foam dispenser keeps its map's
/// `uses`, spends one per activation and refuses during its 10 s cooldown.
#[test]
fn ai_hologram_and_foam_dispenser() {
    use ss13_chat::server::MaskedSenses;
    use ss13_game::silicon::ai_slipper::{AiSlipper, interact};
    use ss13_game::silicon::holopad::{HoloProjection, Hologram, Holopad};
    let mut s = Station::start_on("ai_hologram_and_foam_dispenser", MAP_HOLO, 1, false);
    let core = s.join("AI");
    let slipper = s.find::<AiSlipper>(SLIPPER_POS);
    assert_eq!(s.get::<AiSlipper>(slipper).uses, 3);
    let (link, eye) = (s.link(), s.mob());
    assert!(interact(s.server.world_mut(), link, eye, slipper));
    let used = *s.get::<AiSlipper>(slipper);
    assert_eq!(used.uses, 2);
    assert!(used.ready_at > s.now());
    // Cooling down: refused, nothing spent.
    assert!(interact(s.server.world_mut(), link, eye, slipper));
    assert_eq!(s.get::<AiSlipper>(slipper).uses, 2);

    let pad = s.find::<Holopad>(PAD_POS);
    s.run_until("the camera view arrives", |s| {
        s.window::<CameraViewUi>().is_some()
    });
    for _ in 0..3 {
        s.step_dir(Dir::West);
    }
    s.run_until("the eye is in room 1", |s| s.pos().x == 3);
    s.click(pad, RIGHT);
    s.run_until("the hologram is projected", |s| {
        s.try_get::<HoloProjection>(core).is_some()
    });
    let hologram = s.get::<HoloProjection>(core).hologram;
    assert_eq!(s.get::<HoloProjection>(core).pad, pad);
    assert_eq!(s.get::<Hologram>(hologram).ai, core);
    assert_eq!(*s.get::<TilePos>(hologram), PAD_POS);
    assert!(s.get::<Name>(hologram).as_str().ends_with(" (Hologram)"));
    assert_eq!(s.pos(), PAD_POS);
    assert!(s.try_get::<MaskedSenses>(eye).is_none());
    // Within range: the hologram follows the eye.
    let north = TilePos::new(3, 3, 1);
    s.server.world_mut().entity_mut(eye).insert(north);
    s.run_until("the hologram follows", |s| {
        s.try_get::<TilePos>(hologram) == Some(&north)
    });
    // Out of range of every pad: gone.
    s.server
        .world_mut()
        .entity_mut(eye)
        .insert(TilePos::new(9, 3, 1));
    s.run_until("the hologram is removed", |s| {
        s.try_get::<HoloProjection>(core).is_none()
    });
    assert!(s.server.world().get_entity(hologram).is_err());
    assert!(s.try_get::<MaskedSenses>(eye).is_some());
}

struct Station(testing::Station);

ss13_game::drive_station!(Station, 0);

impl Station {
    fn start(test: &str) -> Self {
        Self::start_on(test, MAP, 1, false)
    }

    /// `clients` players `p0`, `p1`, ... on `map`; with `chat`, the server speaks
    /// (`ChatServerPlugin`).
    fn start_on(test: &str, map: &str, clients: usize, chat: bool) -> Self {
        let assets = fixture_assets_on(test, map);
        let mut s = testing::Station::start(fixture_map(&assets, &["map.dmm"]), |app| {
            app.insert_resource(Jobs::new(jobs(), 1));
            if chat {
                app.add_plugins(ss13_chat::server::ChatServerPlugin);
            }
        });
        for i in 0..clients {
            s.join(&format!("p{i}"), |app| {
                app.init_resource::<Windows>()
                    .init_resource::<Heard>()
                    .add_systems(Update, (record_windows, record_chat));
            });
        }
        Self(s)
    }

    /// Whether client `p` received a line containing `text`, on `channel` when given.
    fn heard(&self, p: usize, channel: Option<ChatChannel>, text: &str) -> bool {
        self.players[p]
            .app
            .world()
            .resource::<Heard>()
            .0
            .iter()
            .any(|(c, line)| channel.is_none_or(|w| w == *c) && line.contains(text))
    }

    fn link(&self) -> Entity {
        self.players[0].link
    }

    fn send(&mut self, intent: ClientIntent) {
        self.0.send(0, intent);
    }

    fn mob(&self) -> Entity {
        self.server
            .world()
            .resource::<Players>()
            .mob(self.link())
            .unwrap()
    }

    fn pos(&self) -> TilePos {
        *self.server.world().get::<TilePos>(self.mob()).unwrap()
    }

    fn join(&mut self, title: &str) -> Entity {
        let link = self.link();
        let world = self.server.world_mut();
        let job = world.resource::<Jobs>().data.job(title).unwrap();
        let body = ss13_game::jobs::join(world, link, job, false);
        self.step();
        body
    }

    fn step_dir(&mut self, dir: Dir) {
        self.players[0].seq += 1;
        let seq = self.players[0].seq;
        self.send(ClientIntent::Move { dir, seq });
    }

    fn find<C: Component>(&self, pos: TilePos) -> Entity {
        let world = self.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .find(|&e| world.get::<C>(e).is_some())
            .unwrap()
    }

    fn click(&mut self, entity: Entity, modifiers: ClickModifiers) {
        self.run_until("the target is replicated", |s| {
            s.net_id(0, entity).is_some()
        });
        self.0.click_with(0, entity, modifiers);
    }

    fn door(&self, pos: TilePos) -> (DoorState, bool) {
        let d = self
            .server
            .world()
            .get::<Door>(self.find::<Door>(pos))
            .unwrap();
        (d.state, d.locked)
    }

    fn window<T: UiInterface>(&self) -> Option<(u32, T::StaticData, T::Data)> {
        let w = self.players[0].app.world().resource::<Windows>();
        let (id, _, st, data, _) = w.get(T::NAME)?;
        Some((*id, decode(st).ok()?, decode(data).ok()?))
    }
}

const SHIFT: ClickModifiers = ClickModifiers {
    shift: true,
    ctrl: false,
    alt: false,
    middle: false,
    right: false,
    pixel: None,
};
const PLAIN: ClickModifiers = ClickModifiers {
    shift: false,
    ctrl: false,
    alt: false,
    middle: false,
    right: false,
    pixel: None,
};
const RIGHT: ClickModifiers = ClickModifiers {
    shift: false,
    ctrl: false,
    alt: false,
    middle: false,
    right: true,
    pixel: None,
};
const CTRL: ClickModifiers = ClickModifiers {
    shift: false,
    ctrl: true,
    alt: false,
    middle: false,
    right: false,
    pixel: None,
};

#[test]
fn ai_eye_cameras_and_door_control() {
    let mut s = Station::start("ai_eye_cameras_and_door_control");
    let net = s.server.world().resource::<CameraNet>();
    assert!(net.visible(CAMERA) && net.visible(DOOR_SEEN));
    assert!(!net.visible(AI_START), "behind the closed door");
    assert!(!net.visible(DOOR_UNSEEN));

    let core = s.join("AI");
    assert_eq!(s.server.world().get::<TilePos>(core), Some(&AI_START));
    let eye = s.mob();
    assert_eq!(s.server.world().get::<AiEye>(eye).unwrap().ai, core);
    assert!(s.server.world().get::<AiCore>(core).is_some());
    assert_eq!(
        s.server.world().get::<Laws>(core).unwrap().law_list(true),
        ["1: L1", "2: L2", "3: L3"]
    );
    // The client controls the eye: its own copy of it is `LocalControl`.
    s.run_until("the client controls the eye", |s| {
        let world = s.players[0].app.world_mut();
        let mut q = world.query_filtered::<&TilePos, With<LocalControl>>();
        q.iter(world).next() == Some(&AI_START)
    });
    // The CameraView: the door is covered, the core's tile is static.
    s.run_until("the camera view arrives", |s| {
        s.window::<CameraViewUi>().is_some()
    });
    let (_, (), view): (u32, (), CameraViewData) = s.window::<CameraViewUi>().unwrap();
    assert!(view.sees(DOOR_SEEN) && view.sees(CAMERA));
    assert!(!view.sees(AI_START) && !view.sees(DOOR_UNSEEN));

    // The eye walks west through the wall at x 4 (y 2); the core stays.
    for _ in 0..3 {
        s.step_dir(Dir::West);
    }
    s.run_until("the eye moved through the wall", |s| s.pos().x == 3);
    assert_eq!(s.server.world().get::<TilePos>(core), Some(&AI_START));
    s.run_until("the viewer follows the eye", |s| {
        s.server.world().get::<Viewer>(s.link()).unwrap().center.x == 3
    });

    // Under static nothing is replicated to the AI: not the unseen door, not its own core, not
    // a screwdriver behind the closed door; the client has nothing on those tiles. The eye
    // itself is shown wherever it is.
    let seen = s.find::<Door>(DOOR_SEEN);
    let unseen = s.find::<Door>(DOOR_UNSEEN);
    let item = ss13_game::construction::spawn_obj(
        s.server.world_mut(),
        "/obj/item/screwdriver",
        ITEM_BEHIND_DOOR,
    )
    .unwrap();
    let replicated = |s: &Station, e: Entity| s.net_id(0, e).is_some();
    let client_has = |s: &mut Station, pos: TilePos| {
        let world = s.players[0].app.world_mut();
        let mut q = world.query_filtered::<&TilePos, Without<LocalControl>>();
        q.iter(world).any(|p| *p == pos)
    };
    s.run_until("the floor the camera sees", |s| client_has(s, DOOR_SEEN));
    s.hold(0.5, "nothing under static", |s| {
        !replicated(s, unseen) && !replicated(s, item) && !replicated(s, core)
    });
    for pos in [DOOR_UNSEEN, ITEM_BEHIND_DOOR, AI_START] {
        assert!(
            !client_has(&mut s, pos),
            "the client has something on {pos:?}"
        );
    }
    assert!(replicated(&s, eye) && replicated(&s, seen));

    // Shift-click: the seen door opens and the camera sees the screwdriver, which arrives; it
    // closes and the screwdriver goes.
    s.click(seen, SHIFT);
    s.run_until("the seen door opens", |s| {
        s.door(DOOR_SEEN).0 == DoorState::Open
    });
    assert!(
        s.server
            .world()
            .resource::<CameraNet>()
            .visible(ITEM_BEHIND_DOOR)
    );
    s.run_until("the screwdriver arrives", |s| {
        replicated(s, item) && client_has(s, ITEM_BEHIND_DOOR)
    });
    s.click(seen, SHIFT);
    s.run_until("and closes", |s| s.door(DOOR_SEEN).0 == DoorState::Closed);
    s.run_until("the screwdriver is gone", |s| {
        !replicated(s, item) && !client_has(s, ITEM_BEHIND_DOOR)
    });
    // Ctrl: the bolts drop and rise.
    s.click(seen, CTRL);
    s.run_until("bolted", |s| s.door(DOOR_SEEN).1);
    s.click(seen, CTRL);
    s.run_until("unbolted", |s| !s.door(DOOR_SEEN).1);

    // The AI wire cut: no more control.
    ss13_game::wires::cut(s.server.world_mut(), seen, WIRE_AI, None);
    assert!(s.server.world().get::<Wires>(seen).unwrap().is_cut(WIRE_AI));
    s.click(seen, SHIFT);
    s.click(seen, CTRL);
    s.hold(1.0, "the door ignores the AI", |s| {
        s.door(DOOR_SEEN) == (DoorState::Closed, false)
    });
}

impl Station {
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
}

#[test]
fn ai_opens_airlock_and_apc_windows() {
    let mut s = Station::start_on("ai_opens_airlock_and_apc_windows", MAP_APC, 1, false);
    s.join("AI");
    let eye = s.mob();
    let seen = s.find::<Door>(DOOR_SEEN);
    let unseen = s.find::<Door>(DOOR_UNSEEN);
    let apc = s.find::<Apc>(APC_POS);
    assert!(s.server.world().resource::<CameraNet>().visible(APC_POS));
    s.run_until("the camera view arrives", |s| {
        s.window::<CameraViewUi>().is_some()
    });
    for _ in 0..3 {
        s.step_dir(Dir::West);
    }
    s.run_until("the eye is in room 1", |s| s.pos().x == 3);

    // A plain click on the airlock the camera sees: its AiAirlock window, bolts and door.
    s.click(seen, PLAIN);
    s.run_until("the AiAirlock window opens", |s| {
        s.window::<AiAirlockUi>().is_some()
    });
    let (_, (), data): (u32, (), AiAirlockData) = s.window::<AiAirlockUi>().unwrap();
    assert!(!data.locked && !data.opened && data.wires.bolts && data.power.main == 2);
    s.act::<AiAirlockUi>("bolt-toggle", &[]);
    s.run_until("bolted from the window", |s| s.door(DOOR_SEEN).1);
    s.act::<AiAirlockUi>("open-close", &[]);
    s.hold(0.5, "a bolted door stays shut", |s| {
        s.door(DOOR_SEEN) == (DoorState::Closed, true)
    });
    s.act::<AiAirlockUi>("bolt-toggle", &[]);
    s.run_until("unbolted", |s| !s.door(DOOR_SEEN).1);
    s.act::<AiAirlockUi>("open-close", &[]);
    s.run_until("opened from the window", |s| {
        s.door(DOOR_SEEN).0 == DoorState::Open
    });
    s.run_until("the window shows it open", |s| {
        s.window::<AiAirlockUi>().is_some_and(|(_, (), d)| d.opened)
    });

    // The APC: its window, as a silicon, and the breaker.
    s.click(apc, PLAIN);
    s.run_until("the Apc window opens", |s| s.window::<ApcUi>().is_some());
    let (_, (), data): (u32, (), ApcData) = s.window::<ApcUi>().unwrap();
    assert!(data.silicon_user && data.locked && data.is_operating);
    assert_eq!(data.power_channels.len(), 3);
    s.act::<ApcUi>("breaker", &[]);
    s.run_until("the breaker trips", |s| {
        !s.server.world().get::<Apc>(apc).unwrap().operating
    });
    s.act::<ApcUi>("channel", &[("lgt", "1")]);
    s.run_until("lighting off", |s| {
        s.server.world().get::<Apc>(apc).unwrap().channels[1] == 0
    });

    s.act::<AiAirlockUi>("open-close", &[]);
    s.run_until("closed again", |s| s.door(DOOR_SEEN).0 == DoorState::Closed);
    s.run_until("the camera no longer sees room 3's door", |s| {
        !s.server
            .world()
            .resource::<CameraNet>()
            .visible(DOOR_UNSEEN)
    });
    // Out of the cameras' view nothing opens: `attack_ai` on the unseen airlock makes a window
    // the status check closes before it is ever sent.
    let link = s.link();
    ss13_game::silicon::interact::attack_ai(s.server.world_mut(), link, eye, unseen);
    s.hold(1.0, "no window on the unseen airlock", |s| {
        let world = s.server.world();
        world
            .try_query::<&ss13_tgui::server::UiWindow>()
            .is_none_or(|mut q| q.iter(world).all(|w| w.source != unseen))
    });
    let opened = s.players[0]
        .app
        .world()
        .resource::<Windows>()
        .0
        .iter()
        .filter(|w| w.1 == AiAirlockUi::NAME)
        .count();
    assert_eq!(
        opened, 1,
        "only the seen airlock's window reached the client"
    );
}

#[test]
fn camera_console_moves_the_viewer() {
    let mut s = Station::start("camera_console_moves_the_viewer");
    assert_eq!(s.pos(), SPAWN);
    let console = s.find::<ss13_game::silicon::cameras::CameraConsole>(CONSOLE);
    let camera = s.find::<Camera>(CAMERA);
    s.click(console, ClickModifiers::default());
    s.run_until("the console window opens", |s| {
        s.window::<CameraConsoleUi>().is_some()
    });
    let (window, st, _): (u32, CameraConsoleStatic, _) = s.window::<CameraConsoleUi>().unwrap();
    assert_eq!(st.cameras.len(), 1);
    assert_eq!(st.cameras[0].name, "Room One");
    // A ref outside the console's list is refused.
    s.send(ClientIntent::UiAct {
        window,
        action: "switch_camera".into(),
        params: vec![("camera".into(), console.to_bits().to_string())],
    });
    s.hold(0.3, "a bad ref moves nothing", |s| {
        s.server.world().get::<Viewer>(s.link()).unwrap().center == SPAWN
    });
    s.send(ClientIntent::UiAct {
        window,
        action: "switch_camera".into(),
        params: vec![("camera".into(), st.cameras[0].cam_ref.clone())],
    });
    s.run_until("the viewer moves to the camera", |s| {
        s.server.world().get::<Viewer>(s.link()).unwrap().center == CAMERA
    });
    s.run_until("the client gets the camera's view", |s| {
        s.window::<CameraViewUi>()
            .is_some_and(|(_, (), v)| v.center == CAMERA && v.moves_view)
    });
    let (_, (), view) = s.window::<CameraViewUi>().unwrap();
    assert!(view.sees(DOOR_SEEN) && !view.sees(AI_START));
    assert_eq!(camera.to_bits().to_string(), st.cameras[0].cam_ref);
    // Only what this camera sees is replicated: not the door it does not see.
    let replicated = |s: &Station, e: Entity| s.net_id(0, e).is_some();
    let (seen, unseen) = (s.find::<Door>(DOOR_SEEN), s.find::<Door>(DOOR_UNSEEN));
    s.run_until("the seen door is replicated", |s| replicated(s, seen));
    s.hold(0.3, "the unseen door is not", |s| !replicated(s, unseen));
    // Closing the window puts the view back on the mob.
    s.players[0]
        .app
        .world_mut()
        .write_message(ss13_net::ui::client::SendUi(
            ss13_shared::ui::UiClientMessage::Close { window },
        ));
    s.run_until("the viewer is back", |s| {
        s.server.world().get::<Viewer>(s.link()).unwrap().center == SPAWN
    });
    s.run_until("the camera view closes", |s| {
        s.window::<CameraViewUi>().is_none()
    });
    // The mob's own line of sight: the near door, not the far one behind it.
    s.run_until("the mob's own view has the near door", |s| {
        replicated(s, seen)
    });
    s.hold(0.3, "the far door stays behind the near one", |s| {
        !replicated(s, unseen)
    });
}

#[test]
fn cyborg_links_picks_a_model_and_charges() {
    let mut s = Station::start("cyborg_links_picks_a_model_and_charges");
    let core = s.join("AI");
    let borg = s.join("Cyborg");
    assert_eq!(
        s.server.world().get::<Cyborg>(borg).unwrap().connected_ai,
        Some(core)
    );
    assert_eq!(
        s.server.world().get::<Laws>(borg),
        s.server.world().get::<Laws>(core)
    );
    assert!(
        s.server
            .world()
            .get::<AiCore>(core)
            .unwrap()
            .connected_robots
            .contains(&borg)
    );
    // The AI's Status tab lists the cyborg (tg `get_status_tab_items`).
    // The client is the cyborg's now (the AI's eye went with the second join): read the core's.
    let lines = ss13_game::silicon::ai_status::ai_lines(s.server.world(), core);
    assert_eq!(lines[0], "System integrity: 100%");
    assert!(
        lines.contains(&"Connected cyborgs: 1".to_owned()),
        "{lines:?}"
    );
    let borg_line = lines.iter().find(|l| l.contains("S.Integrity")).unwrap();
    assert!(
        // The fixture has no high-capacity cell type: the chassis is empty.
        borg_line.contains("S.Integrity: 100%")
            && borg_line.contains("Cell: Empty")
            && borg_line.contains("Status: DEPOWERED"),
        "{borg_line}"
    );
    // No lungs, stomach or reagents.
    assert!(
        s.server
            .world()
            .get::<ss13_game::hunger::Hunger>(borg)
            .is_none()
    );
    assert!(
        s.server
            .world()
            .get::<ss13_game::chem::MobReagents>(borg)
            .is_none()
    );

    s.run_until("the model list opens", |s| {
        s.window::<ListInputUi>().is_some()
    });
    let (window, st, ()) = s.window::<ListInputUi>().unwrap();
    assert_eq!(st.items, ["Engineering"]);
    s.send(ClientIntent::UiAct {
        window,
        action: "submit".into(),
        params: vec![("entry".into(), "Engineering".into())],
    });
    s.run_until("the model is applied", |s| {
        s.server.world().get::<Cyborg>(borg).unwrap().model == Some(0)
    });
    let model = s
        .server
        .world()
        .get::<Inventory>(borg)
        .unwrap()
        .back
        .unwrap();
    let world = s.server.world_mut();
    let mut q = world.query::<&StoredIn>();
    let modules = q.iter(world).filter(|i| i.0 == model).count();
    assert_eq!(modules, 2);
    // The model and its modules never leave the cyborg (`TRAIT_NODROP`): dragging the back
    // slot onto the floor dumps nothing (a storage would after 2 s).
    let turf = ss13_game::construction::turf_at(s.server.world(), s.pos()).unwrap();
    s.run_until("the turf is replicated", |s| s.net_id(0, turf).is_some());
    let to = DragEnd::Atom(s.net_id(0, turf).unwrap());
    s.send(ClientIntent::Drag {
        from: DragEnd::Slot(InventorySlot::Back),
        to,
        modifiers: ClickModifiers::default(),
    });
    s.hold(2.5, "the modules stay in the model", |s| {
        let world = s.server.world();
        world.get::<Inventory>(borg).unwrap().back == Some(model)
            && world
                .iter_entities()
                .filter(|e| e.get::<StoredIn>().is_some_and(|i| i.0 == model))
                .count()
                == 2
    });

    // A low cell charges in a closed station.
    s.server.world_mut().entity_mut(borg).insert(STATION);
    s.server.world_mut().get_mut::<Cyborg>(borg).unwrap().charge = 1000.0;
    let station = s.find::<RechargeStation>(STATION);
    s.click(station, ClickModifiers::default());
    s.run_until("the station takes the cyborg in", |s| {
        s.server
            .world()
            .get::<RechargeStation>(station)
            .unwrap()
            .occupant
            == Some(borg)
    });
    s.run_until("the cell charges", |s| {
        s.server.world().get::<Cyborg>(borg).unwrap().charge > 1500.0
    });
    // Damage: brute counts, no crit before -100.
    let world = s.server.world_mut();
    let mut health = world.get_mut::<ss13_game::health::Health>(borg).unwrap();
    health.adjust(ss13_game::health::DamageType::Brute, 150.0);
    health.adjust(ss13_game::health::DamageType::Oxy, 50.0);
    s.step();
    s.step();
    let world = s.server.world();
    assert_eq!(
        world.get::<ss13_game::health::Stat>(borg),
        Some(&ss13_game::health::Stat::Conscious)
    );
    assert_eq!(
        world.get::<ss13_game::health::Health>(borg).unwrap().oxy,
        0.0
    );
}

/// The AI eye perceives only what the cameras see (tg `/mob/eye/camera/ai/Hear`:
/// `checkCameraVis`): speech and a deathgasp behind the closed door, right next to the eye, never
/// reach its client; speech the camera sees does. The view mask is on the client from the join.
/// Dead (its core), the AI speaks in deadchat: no binary, no radio.
#[test]
fn ai_senses_follow_the_cameras_and_death() {
    use ss13_game::health::{DamageType, Health, Stat};
    use ss13_game::silicon::cameras::CAMERANET_MASK;
    use ss13_net::server::ViewMask;
    let mut s = Station::start_on("ai_senses_follow_the_cameras_and_death", MAP, 2, true);
    let (link, job) = {
        let world = s.server.world();
        (s.link(), world.resource::<Jobs>().data.job("AI").unwrap())
    };
    let core = ss13_game::jobs::join(s.server.world_mut(), link, job, false);
    // No tick in between: the first replication pass already sees the mask.
    assert_eq!(
        s.server.world().get::<ViewMask>(link),
        Some(&ViewMask(CAMERANET_MASK))
    );
    s.run_until("the client controls the eye", |s| {
        let world = s.players[0].app.world_mut();
        let mut q = world.query_filtered::<&TilePos, With<LocalControl>>();
        q.iter(world).next() == Some(&AI_START)
    });
    let human = s.controlled(1);
    let say = |s: &mut Station, text: &str| s.0.send(1, ClientIntent::Say(text.into()));

    // Behind the closed door, one tile from the eye: under static.
    s.server
        .world_mut()
        .entity_mut(human)
        .insert(ITEM_BEHIND_DOOR);
    s.step();
    say(&mut s, "Тайный разговор");
    s.run_until("the speaker hears itself", |s| {
        s.heard(1, None, "Тайный разговор")
    });
    s.hold(0.5, "the AI does not hear it", |s| {
        !s.heard(0, None, "Тайный разговор")
    });
    // In room 1, which the camera sees, 3 tiles from the eye.
    s.server
        .world_mut()
        .entity_mut(human)
        .insert(TilePos::new(3, 3, 1));
    s.step();
    say(&mut s, "Открытый разговор");
    s.run_until("the AI hears it", |s| s.heard(0, None, "Открытый разговор"));
    // A deathgasp behind the door (a visible message): seen by the dying one, not by the AI.
    s.server
        .world_mut()
        .entity_mut(human)
        .insert(ITEM_BEHIND_DOOR);
    s.server
        .world_mut()
        .get_mut::<Health>(human)
        .unwrap()
        .adjust(DamageType::Brute, 500.0);
    s.run_until("the human dies and sees its deathgasp", |s| {
        s.heard(1, None, "цепенеет")
    });
    s.hold(0.5, "the AI does not see it", |s| {
        !s.heard(0, None, "цепенеет")
    });

    // Alive, the AI talks in binary (it reads its own line).
    let ai_say = |s: &mut Station, text: &str| s.send(ClientIntent::Say(text.into()));
    ai_say(&mut s, ":b Живой");
    s.run_until("binary works", |s| s.heard(0, None, "Робоболтовня"));
    // Dead: deadchat, and neither binary nor its radio carry a word.
    s.server
        .world_mut()
        .get_mut::<Health>(core)
        .unwrap()
        .adjust(DamageType::Brute, 500.0);
    s.run_until("the core dies", |s| {
        s.server.world().get::<Stat>(core) == Some(&Stat::Dead)
    });
    s.step();
    ai_say(&mut s, ":b Мёртвый бинарный");
    ai_say(&mut s, ":с Мёртвое радио");
    s.run_until("both go to deadchat", |s| {
        s.heard(0, None, "МЕРТВ: ") && s.heard(0, None, "Мёртвое радио")
    });
    s.hold(0.5, "nothing goes out", |s| {
        !s.heard(0, Some(ChatChannel::Radio), "Мёртв")
            && !s.heard(1, Some(ChatChannel::Radio), "Мёртв")
    });
}

/// Room 2 also has a camera of another network (`2`, (7,4)), which sees the whole room.
const MAP_TWO_NETS: &str = r##""w" = (/turf/closed/wall,/area/station)
"f" = (/turf/open/floor,/area/station)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/station)
"1" = (/obj/machinery/camera{c_tag = "Room One"},/turf/open/floor,/area/station)
"2" = (/obj/machinery/camera{c_tag = "Room Two"; network = list("other")},/turf/open/floor,/area/station)
"c" = (/obj/machinery/computer/security,/turf/open/floor,/area/station)
"D" = (/obj/machinery/door/airlock,/turf/open/floor,/area/station)
"E" = (/obj/machinery/door/airlock,/turf/open/floor,/area/station)
"A" = (/obj/effect/landmark/start/ai,/turf/open/floor,/area/station)
"R" = (/obj/machinery/recharge_station,/turf/open/floor,/area/station)

(1,1,1) = {"
wwwwwwwwwww
w1cwff2wffw
wsfDfAfEffw
wffwfRfwffw
wwwwwwwwwww
"}
"##;

/// A console's view is its camera's own: the door in front of the camera closing hides what is
/// behind it again, although another network's camera still sees those tiles (the network's
/// chunks do not change).
#[test]
fn console_view_follows_its_camera_when_the_net_does_not_change() {
    let mut s = Station::start_on(
        "console_view_follows_its_camera_when_the_net_does_not_change",
        MAP_TWO_NETS,
        1,
        false,
    );
    let console = s.find::<ss13_game::silicon::cameras::CameraConsole>(CONSOLE);
    s.click(console, ClickModifiers::default());
    s.run_until("the console window opens", |s| {
        s.window::<CameraConsoleUi>().is_some()
    });
    let (window, st, _): (u32, CameraConsoleStatic, _) = s.window::<CameraConsoleUi>().unwrap();
    assert_eq!(st.cameras.len(), 1, "only the ss13 camera is listed");
    s.send(ClientIntent::UiAct {
        window,
        action: "switch_camera".into(),
        params: vec![("camera".into(), st.cameras[0].cam_ref.clone())],
    });
    s.run_until("the viewer moves to the camera", |s| {
        s.server.world().get::<Viewer>(s.link()).unwrap().center == CAMERA
    });
    let item = ss13_game::construction::spawn_obj(
        s.server.world_mut(),
        "/obj/item/screwdriver",
        ITEM_BEHIND_DOOR,
    )
    .unwrap();
    let replicated = |s: &Station, e: Entity| s.net_id(0, e).is_some();
    s.hold(0.3, "the screwdriver behind the door is hidden", |s| {
        !replicated(s, item)
    });
    let door = s.find::<Door>(DOOR_SEEN);
    let set_door = |s: &mut Station, open: bool| {
        ss13_game::doors::with_door(s.server.world_mut(), door, |d, a, ctx| {
            let done = if open {
                ctx.open(d, a, true)
            } else {
                ctx.close(d, a, true)
            };
            (done, true)
        });
    };
    set_door(&mut s, true);
    s.run_until("the door opens and the screwdriver shows", |s| {
        s.door(DOOR_SEEN).0 == DoorState::Open && replicated(s, item)
    });
    let version = s.server.world().resource::<CameraNet>().version;
    set_door(&mut s, false);
    s.run_until("the door closes and the screwdriver goes", |s| {
        s.door(DOOR_SEEN).0 == DoorState::Closed && !replicated(s, item)
    });
    assert_eq!(
        s.server.world().resource::<CameraNet>().version,
        version,
        "the other network's camera kept the chunks as they were"
    );
}

/// Replication cost of an AI client on MetaStation: the eye flies a fixed square from the core
/// (through walls), then idles. Prints the replication pass time and the replication bytes.
/// `SS13_ASSETS` must hold the converted MetaStation assets.
#[test]
#[ignore = "needs generated assets and MetaStation; run with --ignored --nocapture"]
fn metastation_ai_replication() {
    use core::time::Duration;
    use ss13_net::server::{Interest, ReplicationStats};
    let map = ss13_mapload::MapLoadPlugin {
        source: std::env::var("SS13_SOURCE")
            .unwrap_or_else(|_| env!("SS13_SOURCE").into())
            .into(),
        maps: vec!["MetaStation".into()],
        assets: std::env::var("SS13_ASSETS")
            .unwrap_or_else(|_| "assets/generated".into())
            .into(),
    };
    let mut server = testing::server_app(map, |_| {});
    server
        .world_mut()
        .resource_mut::<ReplicationStats>()
        .count_payload = true;
    let mut st = testing::Station::new(server);
    st.join("ai", |app| {
        app.init_resource::<Windows>()
            .add_systems(Update, record_windows);
    });
    let mut s = Station(st);
    s.join("AI");
    s.run_until("the client controls the eye", |s| {
        let world = s.players[0].app.world_mut();
        let mut q = world.query_filtered::<(), With<LocalControl>>();
        q.iter(world).next().is_some()
    });
    for _ in 0..30 {
        s.step();
    }
    let link = s.link();
    let bytes = |s: &Station| {
        s.server.world().resource::<ReplicationStats>().clients[&link].replication_bytes_total
    };
    // Per server frame: replication pass time, entities known.
    let measure = |s: &mut Station, plan: &[(Dir, u32)], idle_frames: u32| {
        let b0 = bytes(s);
        let (mut sum, mut max, mut frames, mut known) = (Duration::ZERO, Duration::ZERO, 0u32, 0);
        let mut frame = |s: &mut Station| {
            s.step();
            let t = s
                .server
                .world()
                .resource::<ReplicationStats>()
                .replicate_time;
            sum += t;
            max = max.max(t);
            frames += 1;
            known += s.server.world().resource::<Interest>().visible_count(link);
        };
        for &(dir, n) in plan {
            for _ in 0..n {
                let from = s.pos();
                s.step_dir(dir);
                for _ in 0..20 {
                    frame(s);
                    if s.pos() != from {
                        break;
                    }
                }
            }
        }
        for _ in 0..idle_frames {
            frame(s);
        }
        (
            sum.as_micros() as f64 / f64::from(frames),
            max.as_micros(),
            bytes(s) - b0,
            known / frames as usize,
            frames,
        )
    };
    let start = s.pos();
    let fly = measure(
        &mut s,
        &[
            (Dir::East, 40),
            (Dir::North, 40),
            (Dir::West, 80),
            (Dir::South, 80),
            (Dir::East, 80),
            (Dir::North, 40),
        ],
        0,
    );
    let idle = measure(&mut s, &[], 300);
    println!(
        "MetaStation AI from {start:?}: flying {} frames: replicate avg {:.1} us max {} us, {} B \
         replication, {} entities known on average; idle {} frames: avg {:.1} us max {} us, {} B, \
         {} known",
        fly.4, fly.0, fly.1, fly.2, fly.3, idle.4, idle.0, idle.1, idle.2, idle.3
    );
}

/// Room 2 is the AI's own powered area (`/area/station/ai`), its APC `P` (7,2) with the
/// breaker off: no equipment power.
const MAP_AI_UNPOWERED: &str = r##""w" = (/turf/closed/wall,/area/station)
"f" = (/turf/open/floor,/area/station)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/station)
"1" = (/obj/machinery/camera{c_tag = "Room One"},/turf/open/floor,/area/station)
"c" = (/obj/machinery/computer/security,/turf/open/floor,/area/station)
"D" = (/obj/machinery/door/airlock,/turf/open/floor,/area/station)
"E" = (/obj/machinery/door/airlock,/turf/open/floor,/area/station)
"g" = (/turf/open/floor,/area/station/ai)
"A" = (/obj/effect/landmark/start/ai,/turf/open/floor,/area/station/ai)
"P" = (/obj/machinery/power/apc{operating = 0},/turf/open/floor,/area/station/ai)

(1,1,1) = {"
wwwwwwwwwww
w1cwgggwffw
wsfDgAgEffw
wffwggPwffw
wwwwwwwwwww
"}
"##;

/// tg `ai_lose_power` / `start_RestorePowerRoutine` / `ai_restore_power`: without equipment
/// power the AI goes blind and incapacitated, its routine finds the APC and opens its window,
/// the breaker turned on from it restores the AI; a dead backup battery kills it.
#[test]
fn ai_loses_and_restores_power() {
    use ss13_game::health::Stat;
    use ss13_game::silicon::ai::AI_BLIND_MASK;
    use ss13_game::silicon::ai_power::{AiPower, Routine};
    use ss13_net::server::ViewMask;
    let mut s = Station::start_on("ai_loses_and_restores_power", MAP_AI_UNPOWERED, 1, true);
    let core = s.join("AI");
    let eye = s.mob();
    let apc = s.find::<Apc>(TilePos::new(7, 2, 1));
    s.run_until("the AI loses power", |s| {
        s.heard(0, None, "You've lost power!")
    });
    s.run_until("the eye goes blind", |s| {
        s.server.world().get::<ViewMask>(s.link()) == Some(&ViewMask(AI_BLIND_MASK))
    });
    assert!(s.server.world().get::<AiEye>(eye).unwrap().blind);
    s.run_until_for(60.0, "the routine hacks the APC", |s| {
        s.window::<ApcUi>().is_some()
    });
    for line in [
        "Backup battery online",
        "Fault confirmed",
        "APC located",
        "Transfer complete",
        "APC ready for connection.",
    ] {
        assert!(s.heard(0, None, line), "{line}");
    }
    let power = s.server.world().get::<AiPower>(core).unwrap().clone();
    assert_eq!(power.routine, Routine::ApcFound);
    assert_eq!(power.apc_override, Some(apc));
    assert!(
        power.battery < 190.0,
        "drained while depowered: {}",
        power.battery
    );

    // The camera does not see the APC, but the hacked window works: the breaker restores.
    s.act::<ApcUi>("breaker", &[]);
    s.run_until("the AI's power is restored", |s| {
        s.heard(0, None, "Alert cancelled. Power has been restored.")
    });
    assert!(s.heard(0, None, "APC backdoor has been closed."));
    s.run_until("the eye sees again", |s| {
        !s.server.world().get::<AiEye>(eye).unwrap().blind
            && s.server.world().get::<ViewMask>(s.link()) != Some(&ViewMask(AI_BLIND_MASK))
    });

    // Breaker off again with an empty battery: the AI dies.
    {
        let world = s.server.world_mut();
        world.get_mut::<AiPower>(core).unwrap().battery = 0.0;
        let mut a = world.get_mut::<Apc>(apc).unwrap();
        a.operating = false;
        a.force_update = true;
    }
    s.run_until_for(10.0, "the backup battery fails", |s| {
        s.heard(0, None, "backup battery's output drops below usable levels")
    });
    s.run_until("the core dies", |s| {
        s.server.world().get::<Stat>(core) == Some(&Stat::Dead)
    });
}

/// The AI's HUD buttons: the alerts window lists a fire alarm raised in its area (and the AI is
/// told after tg's 3 s), its camera button jumps the eye to the area's camera; the eye goes back
/// to the core; a VOX announcement (text only on fixture assets) reaches the players on the
/// level once a minute. A cyborg has the alert panel too.
#[test]
fn ai_hud_alerts_and_announcement() {
    use ss13_game::actions::{ActionButtonsData, ActionButtonsUi};
    use ss13_game::silicon::ui::{StationAlertData, StationAlertUi};
    use ss13_tgui::interfaces::text_input::TextInputUi;
    let mut s = Station::start_on("ai_hud_alerts_and_announcement", MAP, 2, true);
    let core = s.join("AI");
    s.run_until("the HUD buttons arrive", |s| {
        s.window::<ActionButtonsUi>().is_some()
    });
    let (_, (), buttons): (u32, (), ActionButtonsData) = s.window::<ActionButtonsUi>().unwrap();
    let ids: Vec<&str> = buttons.buttons.iter().map(|b| b.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "ai_core",
            "camera",
            "camera_track",
            "alerts",
            "announcement",
            "call_shuttle",
            "core_display"
        ]
    );
    s.act::<ActionButtonsUi>("trigger", &[("id", "alerts")]);
    s.run_until("the alerts window opens", |s| {
        s.window::<StationAlertUi>().is_some()
    });
    let (_, (), data): (u32, (), StationAlertData) = s.window::<StationAlertUi>().unwrap();
    assert!(data.camera_view);
    assert_eq!(data.alarms.len(), 6);
    assert!(data.alarms.iter().all(|c| c.alerts.is_empty()));

    let area = {
        let world = s.server.world();
        world
            .resource::<ss13_mapload::Areas>()
            .area_at(CAMERA)
            .unwrap()
    };
    assert!(ss13_game::power::set_area_fire(
        s.server.world_mut(),
        area,
        true
    ));
    s.run_until("the window lists the fire alarm", |s| {
        s.window::<StationAlertUi>()
            .is_some_and(|(_, (), d)| d.alarms[0].name == "Fire" && d.alarms[0].alerts.len() == 1)
    });
    let (_, (), data): (u32, (), StationAlertData) = s.window::<StationAlertUi>().unwrap();
    let alert = data.alarms[0].alerts[0].clone();
    assert_eq!((alert.cameras, alert.sources), (Some(1), Some(1)));
    s.run_until("the AI is told after 3 s", |s| {
        s.heard(0, None, "--- Fire alarm detected in") && s.heard(0, None, "(Room One)")
    });
    s.act::<StationAlertUi>("select_camera", &[("alert", &alert.alert_ref.unwrap())]);
    s.run_until("the eye jumps to the camera", |s| s.pos() == CAMERA);
    s.act::<ActionButtonsUi>("trigger", &[("id", "ai_core")]);
    s.run_until("the eye is back on the core", |s| s.pos() == AI_START);
    assert!(ss13_game::power::set_area_fire(
        s.server.world_mut(),
        area,
        false
    ));
    s.run_until("cleared", |s| s.heard(0, None, "has been cleared."));

    // The announcement: the other player on the level reads it.
    s.act::<ActionButtonsUi>("trigger", &[("id", "announcement")]);
    s.run_until("the announcement prompt", |s| {
        s.window::<TextInputUi>().is_some()
    });
    s.act::<TextInputUi>("submit", &[("entry", "attention crew")]);
    s.run_until("the crew reads it", |s| {
        s.heard(1, None, "объявляет") && s.heard(1, None, "Attention crew")
    });
    s.act::<ActionButtonsUi>("trigger", &[("id", "announcement")]);
    s.run_until("the cooldown", |s| s.heard(0, None, "Please wait"));
    assert!(
        s.server
            .world()
            .get::<ss13_game::silicon::ai::AiCore>(core)
            .is_some()
    );
}

/// [`MAP`] with a station alert console `S` (3,3) next to the assistant start.
const MAP_ALERT_CONSOLE: &str = r##""w" = (/turf/closed/wall,/area/station)
"f" = (/turf/open/floor,/area/station)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/station)
"1" = (/obj/machinery/camera{c_tag = "Room One"},/turf/open/floor,/area/station)
"S" = (/obj/machinery/computer/station_alert,/turf/open/floor,/area/station)
"D" = (/obj/machinery/door/airlock,/turf/open/floor,/area/station)
"E" = (/obj/machinery/door/airlock,/turf/open/floor,/area/station)
"A" = (/obj/effect/landmark/start/ai,/turf/open/floor,/area/station)

(1,1,1) = {"
wwwwwwwwwww
w1fwfffwffw
wsSDfAfEffw
wffwfffwffw
wwwwwwwwwww
"}
"##;

/// tg `/obj/machinery/computer/station_alert`: the crew's window lists fire, atmosphere and
/// power alarms of station areas, without cameras.
#[test]
fn station_alert_console_lists_alarms() {
    use ss13_game::silicon::alarms::StationAlertConsole;
    use ss13_game::silicon::ui::{StationAlertData, StationAlertUi};
    let mut s = Station::start_on(
        "station_alert_console_lists_alarms",
        MAP_ALERT_CONSOLE,
        1,
        false,
    );
    s.join("Assistant");
    let console = s.find::<StationAlertConsole>(TilePos::new(3, 3, 1));
    let area = {
        let world = s.server.world();
        world
            .resource::<ss13_mapload::Areas>()
            .area_at(SPAWN)
            .unwrap()
    };
    ss13_game::power::set_area_fire(s.server.world_mut(), area, true);
    s.click(console, PLAIN);
    s.run_until("the console's window opens", |s| {
        s.window::<StationAlertUi>().is_some()
    });
    let (_, (), data): (u32, (), StationAlertData) = s.window::<StationAlertUi>().unwrap();
    assert!(!data.camera_view);
    let names: Vec<&str> = data.alarms.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["Fire", "Atmosphere", "Power"]);
    assert_eq!(data.alarms[0].alerts.len(), 1);
    assert_eq!(data.alarms[0].alerts[0].alert_ref, None);
}

/// The mapped core rack starts with the default lawset's module welded in and names itself; the
/// AI links to it on joining. A human with a configured freeform module opens the rack, inserts
/// it into slot 2 (the AI gets the supplied law), takes it out again (the law goes), and unlinks
/// the AI (its laws are wiped).
#[test]
fn law_rack_modules_change_the_ai_laws() {
    use ss13_game::inventory::CarriedBy;
    use ss13_game::silicon::law_rack::{LawModule, LawRack, LinkedRack, ModuleKind, Security};
    use ss13_game::silicon::ui::LawRackUi;
    let mut s = Station::start_on("law_rack_modules_change_the_ai_laws", MAP_RACK, 2, false);
    let rack = s.find::<LawRack>(RACK_POS);
    {
        let r = s.get::<LawRack>(rack);
        assert_eq!(r.slots.len(), 10);
        assert_eq!(r.slots[0].as_ref().unwrap().security, Security::Welded);
        assert_eq!(r.combined.law_list(true), ["1: L1", "2: L2", "3: L3"]);
    }
    // The fixture type is named after its path ("core"); the designation is appended.
    assert_eq!(s.get::<Name>(rack).as_str(), "core 'alpha'");
    // p1 becomes the AI.
    let job = s.server.world().resource::<Jobs>().data.job("AI").unwrap();
    let link = s.players[1].link;
    let core = ss13_game::jobs::join(s.server.world_mut(), link, job, false);
    s.step();
    assert_eq!(s.get::<LinkedRack>(core), &LinkedRack(rack));
    assert_eq!(
        s.get::<Laws>(core).law_list(true),
        ["1: L1", "2: L2", "3: L3"]
    );

    // p0 holds a configured freeform module.
    let human = s.mob();
    let module = ss13_game::construction::spawn_obj(
        s.server.world_mut(),
        "/obj/item/ai_module/law/supplied/freeform",
        SPAWN,
    )
    .unwrap();
    let world = s.server.world_mut();
    world.entity_mut(module).remove::<TilePos>().insert((
        CarriedBy(human),
        LawModule {
            kind: ModuleKind::Supplied,
            laws: vec![("Obey Bob".to_owned(), false)],
            ..default()
        },
    ));
    let mut inventory = world.get_mut::<Inventory>(human).unwrap();
    let active = inventory.active;
    inventory.hands[active] = Some(module);
    s.click(rack, PLAIN);
    s.run_until("the rack window opens", |s| {
        s.window::<LawRackUi>().is_some()
    });
    let (_, (), data) = s.window::<LawRackUi>().unwrap();
    assert!(data.holding_module);
    let ai_name = s.get::<Name>(core).as_str().to_owned();
    assert_eq!(data.linked_mobs, Some(vec![ai_name]));
    assert_eq!(data.slots[0].name, "unidentified welded module");
    s.act::<LawRackUi>("insert_module", &[("slot", "2")]);
    s.run_until("the AI gets the supplied law", |s| {
        s.get::<Laws>(core)
            .law_list(true)
            .last()
            .map(String::as_str)
            == Some("4: Obey Bob")
    });
    assert_eq!(s.get::<StoredIn>(module).0, rack);
    s.act::<LawRackUi>("remove_module", &[("slot", "2")]);
    s.run_until("the module is back in hand", |s| {
        s.get::<Inventory>(human).hands.contains(&Some(module))
    });
    assert_eq!(s.get::<Laws>(core).law_list(true).len(), 3);
    s.act::<LawRackUi>("unlink_silicon", &[("silicon_index", "0")]);
    s.run_until("the AI is unlinked", |s| {
        s.try_get::<LinkedRack>(core).is_none()
    });
    assert!(s.get::<Laws>(core).law_list(true).is_empty());
    assert!(s.get::<LawRack>(rack).linked_mobs.is_empty());
}

/// An ion storm's scramble of the mapped core rack: the welded default module gets the ion law on
/// top and is ioned, and the linked AI gets it as its first (ion-numbered) law.
#[test]
fn ion_storm_adds_an_ion_law_to_the_ai() {
    use ss13_game::events::EventRng;
    use ss13_game::events::ion_storm::{Scramble, scramble_all_racks};
    use ss13_game::silicon::law_rack::{LawModule, LawRack};
    let mut s = Station::start_on("ion_storm_adds_an_ion_law_to_the_ai", MAP_RACK, 2, false);
    let rack = s.find::<LawRack>(RACK_POS);
    let job = s.server.world().resource::<Jobs>().data.job("AI").unwrap();
    let link = s.players[1].link;
    let core = ss13_game::jobs::join(s.server.world_mut(), link, job, false);
    s.step();
    assert_eq!(s.get::<Laws>(core).law_list(true).len(), 3);
    let world = s.server.world_mut();
    if !world.contains_resource::<EventRng>() {
        world.insert_resource(EventRng(ss13_mapload::spawner::Rng::new(1)));
    }
    scramble_all_racks(
        world,
        &Scramble::add_law_only(Some("ЛЮДИ ДОЛЖНЫ ЕСТЬ ЛАМПЫ".into())),
    );
    let module = s.get::<LawRack>(rack).slots[0].as_ref().unwrap().module;
    assert!(s.get::<LawModule>(module).ioned);
    assert_eq!(
        s.get::<Laws>(core).law_list(true),
        ["#: ЛЮДИ ДОЛЖНЫ ЕСТЬ ЛАМПЫ", "1: L1", "2: L2", "3: L3"]
    );
}

/// "Track With Camera" lists the human the room's camera sees; the eye follows it while the
/// camera sees it, and a step of the eye ends the tracking.
#[test]
fn ai_tracks_a_mob_with_the_cameras() {
    use ss13_game::actions::ActionButtonsUi;
    use ss13_game::silicon::ai_track::AiTracking;
    use ss13_tgui::interfaces::list_input::{ListInputData, ListInputUi as PromptList};
    let mut s = Station::start_on("ai_tracks_a_mob_with_the_cameras", MAP, 2, false);
    s.join("AI");
    // The second player's body does not share the first one's start tile: where it stands.
    let human = s.players[1].mob;
    let start = *s.get::<TilePos>(human);
    assert!(s.server.world().resource::<CameraNet>().visible(start));
    s.run_until("the HUD buttons arrive", |s| {
        s.window::<ActionButtonsUi>().is_some()
    });
    s.act::<ActionButtonsUi>("trigger", &[("id", "camera_track")]);
    s.run_until("the target list", |s| s.window::<PromptList>().is_some());
    let (_, (), data): (u32, (), ListInputData) = s.window::<PromptList>().unwrap();
    assert_eq!(data.items.len(), 1);
    let entry = data.items[0].clone();
    s.act::<PromptList>("submit", &[("entry", &entry)]);
    s.run_until("the eye is on the human", |s| s.pos() == start);
    // The human moves within the camera's room: the eye follows.
    let east = [TilePos::new(3, 3, 1), TilePos::new(2, 2, 1)]
        .into_iter()
        .find(|&p| p != start)
        .unwrap();
    s.server.world_mut().entity_mut(human).insert(east);
    s.run_until("the eye follows", |s| s.pos() == east);
    s.step_dir(Dir::South);
    s.run_until("a manual step ends the tracking", |s| {
        s.try_get::<AiTracking>(s.mob()).is_none()
    });
}

/// A malfunctioning AI gets its protected law 0 and 50 processing time; the APC window offers it
/// "Override Programming", and the finished hack locks the APC and adds 9 points (the first
/// point of the hacked APC follows).
#[test]
fn malf_ai_hacks_an_apc() {
    use ss13_game::silicon::malf::{MalfAi, MalfHacked, START_PROCESSING_TIME, add_malf};
    let mut s = Station::start_on("malf_ai_hacks_an_apc", MAP_APC, 1, false);
    let core = s.join("AI");
    assert!(add_malf(s.server.world_mut(), core));
    {
        let laws = s.get::<Laws>(core);
        assert!(laws.protected_zeroth && laws.zeroth.is_some());
        assert_eq!(s.get::<MalfAi>(core).processing_time, START_PROCESSING_TIME);
        assert_eq!(s.get::<MalfAi>(core).objectives.len(), 3);
    }
    let apc = s.find::<Apc>(APC_POS);
    s.run_until("the camera view arrives", |s| {
        s.window::<CameraViewUi>().is_some()
    });
    for _ in 0..3 {
        s.step_dir(Dir::West);
    }
    s.run_until("the eye is in room 1", |s| s.pos().x == 3);
    s.click(apc, PLAIN);
    s.run_until("the Apc window opens", |s| s.window::<ApcUi>().is_some());
    let (_, (), data): (u32, (), ApcData) = s.window::<ApcUi>().unwrap();
    assert_eq!(data.malf_status, 1);
    s.act::<ApcUi>("hack", &[]);
    s.run_until("the hack starts", |s| {
        s.get::<MalfAi>(core).hacking.is_some_and(|(a, _)| a == apc)
    });
    // Skip the 30 s.
    let now = s.now();
    s.server
        .world_mut()
        .get_mut::<MalfAi>(core)
        .unwrap()
        .hacking = Some((apc, now));
    s.run_until("the APC is hacked", |s| {
        s.try_get::<MalfHacked>(apc).is_some()
    });
    let world = s.server.world();
    assert!(world.get::<Apc>(apc).unwrap().locked);
    let malf = world.get::<MalfAi>(core).unwrap();
    assert_eq!(malf.hacked_apcs, [apc]);
    assert!(malf.processing_time >= START_PROCESSING_TIME + 9);
    s.run_until("the window offers the shunt", |s| {
        s.window::<ApcUi>()
            .is_some_and(|(_, (), d)| d.malf_status == 2)
    });

    // Shunting: confirmed, the AI is in the APC, an empty core linked to it on its tile.
    use ss13_game::silicon::ai::{AiInside, LinkedCore};
    use ss13_game::silicon::ai_core::{CoreFrame, CoreState};
    use ss13_game::silicon::malf::{ShuntOccupier, vacate};
    use ss13_tgui::interfaces::alert::AlertUi;
    s.act::<ApcUi>("occupy", &[]);
    s.run_until("the confirmation", |s| s.window::<AlertUi>().is_some());
    s.act::<AlertUi>("choose", &[("choice", "Да")]);
    s.run_until("the AI is in the APC", |s| {
        s.try_get::<AiInside>(core) == Some(&AiInside::Apc(apc))
    });
    let linked = s.get::<LinkedCore>(core).0;
    {
        let world = s.server.world();
        assert!(world.get::<TilePos>(core).is_none());
        assert_eq!(world.get::<ShuntOccupier>(apc), Some(&ShuntOccupier(core)));
        assert_eq!(world.get::<TilePos>(linked), Some(&AI_START));
        let frame = world.get::<CoreFrame>(linked).unwrap();
        assert_eq!(
            (frame.state, frame.linked_ai),
            (CoreState::Finished, Some(core))
        );
        assert!(
            world
                .get::<Name>(core)
                .unwrap()
                .as_str()
                .starts_with("Копия ЛКП")
        );
    }
    // Back: on the core's tile, the core gone, the APC empty.
    assert!(vacate(s.server.world_mut(), core));
    let world = s.server.world();
    assert_eq!(world.get::<TilePos>(core), Some(&AI_START));
    assert!(world.get::<AiInside>(core).is_none());
    assert!(world.get_entity(linked).is_err());
    assert!(world.get::<ShuntOccupier>(apc).is_none());
}
