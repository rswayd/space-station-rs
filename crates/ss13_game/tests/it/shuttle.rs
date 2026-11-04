//! The emergency shuttle over real WebTransport on 127.0.0.1: on a fixture station with a
//! fixture shuttle template, a player logs into the communications console with a command ID,
//! calls the shuttle (short timers), it docks at the station's `emergency_home`, the player
//! walks aboard, it launches, arrives at CentCom and the round report counts the player as
//! escaped on the shuttle. Also: login refused without access, recall, the moved tiles.
//!
//! `cargo test -p ss13_game --all-features --test it shuttle::metastation -- --ignored
//! --nocapture` docks the real MetaStation emergency shuttle with a client watching the
//! departure lounge and prints the move's tick time and replication bytes (needs generated
//! assets in `SS13_ASSETS`).

use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use bevy::prelude::*;
use ss13_atmos::AtmosPlugin;
use ss13_game::comms::{CommsConsoleData, CommsConsoleUi};
use ss13_game::comms_server::CommsConsole;
use ss13_game::construction::{Baseturfs, turf_at};
use ss13_game::inventory::{IdCard, Inventory};
use ss13_game::shuttle::{
    EmergencyShuttle, MobileDock, ShuttleConfig, ShuttleMode, StationaryDock,
};
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::{Areas, DmPower, MapLoadPlugin};
use ss13_net::chat::client::ChatReceived;
use ss13_net::server::ReplicationStats;
use ss13_net::ui::client::UiMessageReceived;
use ss13_shared::ui::UiServerMessage;
use ss13_shared::{Dir, TilePos};
use ss13_tgui::{UiInterface, decode};
use ss13_world::SpatialIndex;

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 }, "declared_vars": { "can_atmos_pass": 1 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3, "anchored": 0 } },
    "/obj": { "parent": "/atom/movable" },
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
    "/obj/docking_port/mobile/emergency": { "parent": "/obj/docking_port/mobile",
        "declared_vars": { "shuttle_id": "emergency", "dir": 4 } },
    "/obj/machinery": { "parent": "/obj", "declared_vars": { "anchored": 1 } },
    "/obj/machinery/door": { "parent": "/obj/machinery", "declared_vars": {
        "icon": { "resource": "d.dmi" }, "icon_state": "door_closed", "density": 1, "opacity": 1,
        "layer": 2.7, "base_icon_state": "door", "autoclose": 0, "can_open_with_hands": 1,
        "closingLayer": 3.1, "glass": 0, "locked": 0, "safe": 1, "visible": 1, "welded": 0,
        "can_atmos_pass": -1 } },
    "/obj/machinery/door/airlock": { "parent": "/obj/machinery/door", "declared_vars": {
        "icon": { "resource": "a.dmi" }, "icon_state": "closed", "autoclose": 1,
        "base_icon_state": null, "normalspeed": 0, "overlays_file": { "resource": "o.dmi" },
        "airlock_material": null } },
    "/obj/machinery/computer": { "parent": "/obj/machinery", "declared_vars": {
        "icon": { "resource": "c.dmi" }, "icon_state": "computer", "density": 1 } },
    "/obj/machinery/computer/communications": { "parent": "/obj/machinery/computer",
        "declared_vars": { "name": "communications console" } },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null } },
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
    "/area/station": { "parent": "/area", "declared_vars": { "name": "Мостик" } },
    "/area/shuttle": { "parent": "/area" },
    "/area/shuttle/escape": { "parent": "/area/shuttle", "declared_vars": {
        "name": "Emergency Shuttle" } }
  }
}"##;

/// Rows top-down, the last line is y = 1. The player spawns at `s` (8,7) next to the console
/// `K` (9,7); an airlock at (5,5) opens onto `emergency_home` `D` (5,4), facing south, sized
/// for the 5x3 shuttle below the gap.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/station)
"f" = (/turf/open/floor,/area/station)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/station)
"K" = (/obj/machinery/computer/communications,/turf/open/floor,/area/station)
"A" = (/obj/machinery/door/airlock,/turf/open/floor,/area/station)
"e" = (/turf/open/space/basic,/area/space)
"D" = (/obj/docking_port/stationary{dir = 2; dwidth = 2; height = 3; name = "Test evac bay"; shuttle_id = "emergency_home"; width = 5},/turf/open/space/basic,/area/space)

(1,1,1) = {"
wwwwwwwwww
wffffffsKw
wffffffffw
wwwwAwwwww
eeeeDeeeee
eeeeeeeeee
eeeeeeeeee
eeeeeeeeee
"}
"##;

/// The shuttle: walls around one row of floor, the port in the top row's middle.
const SHUTTLE: &str = r##""w" = (/turf/closed/wall/shuttle,/area/shuttle/escape)
"f" = (/turf/open/floor/shuttle,/area/shuttle/escape)
"p" = (/obj/docking_port/mobile/emergency{dir = 2; name = "Test shuttle"},/turf/open/floor/shuttle,/area/shuttle/escape)

(1,1,1) = {"
wwpww
wfffw
wwwww
"}
"##;

const SPAWN: TilePos = TilePos::new(8, 7, 1);
const CONSOLE: TilePos = TilePos::new(9, 7, 1);
const DOCK: TilePos = TilePos::new(5, 4, 1);

const STATES: [(&str, &str); 19] = [
    ("a.dmi", "closed"),
    ("a.dmi", "opening"),
    ("a.dmi", "open"),
    ("a.dmi", "closing"),
    ("a.dmi", "fill_closed"),
    ("a.dmi", "fill_opening"),
    ("a.dmi", "fill_open"),
    ("a.dmi", "fill_closing"),
    ("t.dmi", "floor"),
    ("t.dmi", "plating"),
    ("t.dmi", "wall"),
    ("t.dmi", "space"),
    ("t.dmi", "shuttle"),
    ("t.dmi", "swall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "card_grey"),
    ("c.dmi", "computer"),
    ("i.dmi", "toy"),
];

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("shuttle_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/docking_port/stationary",
        "/obj/docking_port/mobile/emergency",
        "/obj/machinery/computer/communications",
        "/obj/machinery/door/airlock",
        "/turf/open/floor",
        "/turf/open/floor/plating",
        "/turf/open/floor/shuttle",
        "/turf/closed/wall",
        "/turf/closed/wall/shuttle",
        "/turf/open/space/basic",
        "/area/space",
        "/area/station",
        "/area/shuttle/escape",
    ];
    write_fixture(
        &dir,
        TYPES,
        &used,
        &STATES,
        &[("map.dmm", MAP), ("shuttle.dmm", SHUTTLE)],
    )
}

#[derive(Resource, Default)]
struct Chat(Vec<String>);

#[derive(Resource, Default)]
struct ConsoleWindow {
    id: Option<u32>,
    data: Option<CommsConsoleData>,
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
                data,
                ..
            } if interface == CommsConsoleUi::NAME => {
                window.id = Some(*id);
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
    fn start(test: &str, config: ShuttleConfig, map: MapLoadPlugin) -> Self {
        let mut s = testing::Station::start(map, |app| {
            app.insert_resource(config).add_plugins(AtmosPlugin);
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

    fn link(&self) -> Entity {
        self.players[0].link
    }

    fn client(&self) -> &App {
        &self.players[0].app
    }

    fn fixture(test: &str) -> Self {
        let assets = fixture_assets(test);
        let config = ShuttleConfig {
            call_secs: 5.0,
            dock_secs: 12.0,
            escape_secs: 1.0,
            refuel_secs: 0.0,
            autocall_threshold: None,
            template: Some(assets.join("shuttle.dmm")),
        };
        let s = Self::start(test, config, fixture_map(&assets, &["map.dmm"]));
        assert_eq!(s.pos(), SPAWN);
        s
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

    fn mode(&self) -> ShuttleMode {
        self.server
            .world()
            .resource::<EmergencyShuttle>()
            .state
            .mode
    }

    fn console(&self) -> Entity {
        let world = self.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(CONSOLE)
            .iter()
            .copied()
            .find(|&e| world.get::<CommsConsole>(e).is_some())
            .unwrap()
    }

    fn click(&mut self, entity: Entity) {
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

    fn data(&self) -> CommsConsoleData {
        self.client()
            .world()
            .resource::<ConsoleWindow>()
            .data
            .clone()
            .unwrap_or_default()
    }

    fn set_access(&mut self, access: &[&str]) {
        let mob = self.mob();
        let world = self.server.world_mut();
        let id = world.get::<Inventory>(mob).unwrap().id.expect("ID card");
        world.entity_mut(id).insert(IdCard {
            access: access.iter().map(|a| (*a).to_owned()).collect(),
        });
    }

    fn open_console(&mut self) {
        let console = self.console();
        self.run_until_for(10.0, "the console is replicated", |s| {
            s.net_id(0, console).is_some()
        });
        self.click(console);
        self.run_until_for(10.0, "the console window opens", |s| {
            s.client().world().resource::<ConsoleWindow>().id.is_some()
        });
    }

    fn turf_path(&self, pos: TilePos) -> String {
        let world = self.server.world();
        let turf = turf_at(world, pos).unwrap();
        world
            .get::<ss13_mapload::DmTurfBuild>(turf)
            .map(|t| t.path.clone())
            .unwrap_or_default()
    }
}

#[test]
fn call_dock_board_launch_arrive_escape() {
    let mut s = Station::fixture("call_dock_board_launch_arrive_escape");
    let world = s.server.world();
    let shuttle = world.resource::<EmergencyShuttle>();
    let (port, home, centcom_z) = (shuttle.port, shuttle.home.unwrap(), shuttle.centcom_z);
    assert_eq!(
        world.get::<StationaryDock>(home).unwrap().id,
        "emergency_home"
    );
    assert_eq!(*world.get::<TilePos>(home).unwrap(), DOCK);
    let mobile = world.get::<MobileDock>(port).unwrap();
    assert_eq!(
        (mobile.size.width, mobile.size.height, mobile.size.dwidth),
        (5, 3, 2)
    );
    assert_eq!(mobile.areas.len(), 1);
    assert_eq!(world.get::<TilePos>(port).unwrap().z, centcom_z);

    // An assistant ID cannot log in.
    s.open_console();
    assert!(!s.data().authenticated);
    s.act("toggleAuthentication", &[]);
    s.act("callShuttle", &[("reason", "the station is on fire")]);
    for _ in 0..30 {
        s.step();
    }
    assert!(!s.data().authenticated, "no command access");
    assert_eq!(s.mode(), ShuttleMode::Idle, "calls need a login");

    // A command ID logs in; a too short reason is refused.
    s.set_access(&["command"]);
    s.act("toggleAuthentication", &[]);
    s.run_until_for(10.0, "logged in", |s| s.data().authenticated);
    assert!(s.data().authorize_name.unwrap().ends_with("- Assistant"));
    assert_eq!(s.data().shuttle_can_evac_or_fail_reason, None);
    s.act("callShuttle", &[("reason", "fire")]);
    for _ in 0..30 {
        s.step();
    }
    assert_eq!(s.mode(), ShuttleMode::Idle, "reason under 12 characters");

    // Called, then recalled while early enough, then called again.
    s.act("callShuttle", &[("reason", "the station is on fire")]);
    s.run_until_for(10.0, "called", |s| s.said("Эвакуационный шаттл вызван"));
    assert!(s.said("Характер происшествия:\nthe station is on fire"));
    assert_eq!(s.mode(), ShuttleMode::Call);
    s.run_until_for(10.0, "the window shows the call", |s| {
        s.data().shuttle_called
    });
    s.act("recallShuttle", &[]);
    s.run_until_for(10.0, "recalled", |s| {
        s.said("Эвакуационный шаттл был отозван")
    });
    s.run_until_for(10.0, "idle again", |s| s.mode() == ShuttleMode::Idle);
    s.run_until_for(10.0, "the window shows idle", |s| !s.data().shuttle_called);
    s.act("callShuttle", &[("reason", "the station is on fire")]);
    s.run_until_for(10.0, "called again", |s| s.mode() == ShuttleMode::Call);

    // Docked at the station: the template's turfs and the port are on the dock, the CentCom
    // tiles are back to what was under the skipover.
    s.run_until_for(20.0, "docked", |s| s.mode() == ShuttleMode::Docked);
    s.run_until_for(5.0, "docked announcement", |s| {
        s.said("Test shuttle состыковался со станцией. У вас есть 12 секунд")
    });
    let world = s.server.world();
    assert_eq!(*world.get::<TilePos>(port).unwrap(), DOCK);
    assert_eq!(s.turf_path(DOCK), "/turf/open/floor/shuttle");
    assert_eq!(
        s.turf_path(TilePos::new(3, 3, 1)),
        "/turf/closed/wall/shuttle"
    );
    let floor = turf_at(world, TilePos::new(5, 3, 1)).unwrap();
    assert_eq!(
        world.get::<Baseturfs>(floor).unwrap().0,
        [
            "/turf/open/space",
            "/turf/open/space/basic",
            "/turf/baseturf_skipover/shuttle",
            "/turf/open/floor/plating"
        ]
    );
    let area = world.resource::<Areas>().area_at(DOCK).unwrap();
    assert_eq!(
        world.get::<DmPower>(area).unwrap().path,
        "/area/shuttle/escape"
    );
    assert_eq!(
        s.turf_path(TilePos::new(3, 3, centcom_z)),
        "/turf/open/space/basic",
        "CentCom keeps what was under the skipover"
    );

    // Walk aboard through the gap.
    for dir in [
        Dir::West,
        Dir::West,
        Dir::West,
        Dir::South,
        Dir::South,
        Dir::South,
        Dir::South,
    ] {
        s.walk(dir);
    }
    assert_eq!(s.pos(), TilePos::new(5, 3, 1));

    // Launch: the player rides into transit, the station tiles are space again.
    s.run_until_for(20.0, "launched", |s| s.mode() == ShuttleMode::Escape);
    s.run_until_for(5.0, "departure announcement", |s| {
        s.said("Эвакуационный шаттл покинул станцию")
    });
    let transit_z = s.server.world().resource::<EmergencyShuttle>().transit_z;
    assert_eq!(s.pos().z, transit_z);
    assert_eq!(s.turf_path(DOCK), "/turf/open/space/basic");
    let world = s.server.world();
    let area = world.resource::<Areas>().area_at(DOCK).unwrap();
    assert_eq!(world.get::<DmPower>(area).unwrap().path, "/area/space");

    // Arrival at CentCom ends the round with the player escaped on the shuttle.
    s.run_until_for(10.0, "arrived", |s| s.mode() == ShuttleMode::Endgame);
    assert_eq!(s.pos().z, centcom_z);
    s.run_until_for(10.0, "round report", |s| s.said("Раунд закончился."));
    assert!(
        s.said("Коэффициент эвакуированных: 1 (100%)"),
        "{:?}",
        s.chat()
    );
    assert!(s.said("На эвакуационном шаттле: 1 (100%)"));
    assert!(s.said("Вам удалось пережить события на map как"));
}

/// At red alert a call is a priority shuttle at half the call time; going back to green
/// during the call rescales what is left (x4).
#[test]
fn red_alert_call_and_level_change() {
    use ss13_game::security_level::{self, SecLevel};
    let mut s = Station::fixture("red_alert_call_and_level_change");
    s.set_access(&["command"]);
    s.open_console();
    s.act("toggleAuthentication", &[]);
    s.run_until_for(10.0, "logged in", |s| s.data().authenticated);
    security_level::set_level(s.server.world_mut(), SecLevel::Red, true);
    s.act("callShuttle", &[("reason", "the station is on fire")]);
    s.run_until_for(10.0, "called", |s| s.mode() == ShuttleMode::Call);
    s.run_until_for(10.0, "announced", |s| {
        s.said("Красный уровень угрозы подтверждён: Отправлен приоритетный шаттл.")
    });
    let left = |s: &Station| {
        let now = s.now();
        s.server
            .world()
            .resource::<EmergencyShuttle>()
            .state
            .timer
            .left(now)
            .unwrap()
    };
    let before = left(&s);
    assert!(before <= 2.5, "red: 5 s x 0.5, {before} left");
    security_level::set_level(s.server.world_mut(), SecLevel::Green, true);
    let after = left(&s);
    assert!((after - before * 4.0).abs() < 1e-6, "{before} -> {after}");
}

/// Docks the real MetaStation emergency shuttle with a client in the departure lounge and
/// reports the move's time and the replication bytes it cost that client.
#[test]
#[ignore = "needs generated assets and MetaStation; run with --ignored"]
fn metastation() {
    let assets =
        PathBuf::from(std::env::var("SS13_ASSETS").unwrap_or_else(|_| "assets/generated".into()));
    let map = MapLoadPlugin {
        source: PathBuf::from(env!("SS13_SOURCE")),
        maps: vec!["MetaStation".into()],
        assets,
    };
    let config = ShuttleConfig {
        call_secs: 1.0,
        dock_secs: 30.0,
        escape_secs: 2.0,
        refuel_secs: 0.0,
        autocall_threshold: None,
        template: None,
    };
    let mut s = Station::start("metastation", config, map);
    let mob = s.mob();
    let world = s.server.world_mut();
    let shuttle = world.resource::<EmergencyShuttle>();
    let home = shuttle.home.expect("MetaStation has emergency_home");
    let dock = *world.get::<TilePos>(home).unwrap();
    println!("emergency_home at {dock:?}");
    // Stand in the lounge, north of the dock's airlock row, so the whole shuttle is in view.
    let watch = TilePos::new(dock.x, dock.y + 3, dock.z);
    world.entity_mut(mob).insert(watch);
    world.resource_mut::<ReplicationStats>().count_payload = true;
    for _ in 0..60 {
        s.step();
    }
    let bytes = |s: &Station| {
        s.server.world().resource::<ReplicationStats>().clients[&s.link()].replication_bytes_total
    };
    // Baseline: 2 s of the idle lounge.
    let before = bytes(&s);
    s.run_for(2.0);
    let idle = bytes(&s) - before;
    {
        let world = s.server.world_mut();
        assert!(ss13_game::shuttle::call_shuttle(world, "test", None, 1.0));
    }
    // The tick that docks, timed alone (server update only).
    let start = bytes(&s);
    let mut dock_tick = Duration::ZERO;
    let deadline = s.now() + 10.0;
    while s.mode() != ShuttleMode::Docked {
        assert!(s.now() < deadline, "no docking");
        let t = Instant::now();
        s.server.update();
        let took = t.elapsed();
        if s.mode() == ShuttleMode::Docked {
            dock_tick = took;
        }
        s.players[0].app.update();
    }
    let report = s
        .server
        .world()
        .resource::<EmergencyShuttle>()
        .last_move
        .clone()
        .unwrap();
    // Settle snapshots follow the moves a few ticks later.
    s.run_for(2.0);
    let docking = bytes(&s) - start;
    println!(
        "dock: {} turfs, {} movables, {} crushed; initiate_docking {:.2?}, whole server tick \
         {:.2?}; replication payload to the watching client: {docking} bytes over the ~2 s \
         call + 2 s after the dock (idle lounge: {idle} bytes per 2 s)",
        report.turfs, report.movables, report.crushed, report.elapsed, dock_tick
    );
    assert!(report.turfs > 200, "{report:?}");
    // The lounge sees the shuttle's floor.
    let world = s.server.world();
    let port = world.resource::<EmergencyShuttle>().port;
    assert_eq!(*world.get::<TilePos>(port).unwrap(), dock);
}
