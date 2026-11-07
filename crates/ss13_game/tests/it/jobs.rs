//! Jobs and the round over real WebTransport on 127.0.0.1: headless clients in the lobby set job
//! preferences, get ready and spawn at round start; later ones latejoin through the latejoin
//! menu's tgui act; the round ends and the rebooted server takes them back to a new lobby.
//!
//! `cargo test -p ss13_game --all-features --test it jobs::metastation -- --ignored` starts a
//! MetaStation round as a Security Officer (needs generated assets in `SS13_ASSETS`).

use core::net::SocketAddr;
use core::time::Duration;
use std::net::UdpSocket;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::harness::{Client, Received, record};
use bevy::prelude::*;
use ss13_game::doors::{Door, DoorState};
use ss13_game::ghost::Ghost;
use ss13_game::human::{AppearancePrefs, HumanLook, SKIN_TONES};
use ss13_game::inventory::{IdCard, Inventory};
use ss13_game::jobs::{CharacterName, JobSpawns, Jobs, PlayerJob, fallback_jobs};
use ss13_game::lobby::{
    CHARACTER_SETUP, GameState, JobPrefsUi, LobbyData, OBSERVE, SET_APPEARANCE, SET_JOB_PREFERENCE,
    TOGGLE_READY,
};
use ss13_game::outfit::IdRegistration;
use ss13_game::round::{NewPlayer, REBOOT_EXIT_CODE, RoundConfig, Ticker};
use ss13_game::server::Players;
use ss13_game::storage::StoredIn;
use ss13_game::testing::{self, Drive, write_fixture};
use ss13_mapload::convert::DmTypes;
use ss13_mapload::jobs::{JOBS_FILE, JobsData, OutfitSlot};
use ss13_mapload::{DmName, MapLoadPlugin};
use ss13_net::client::{NetClientState, NetEntityMap};
use ss13_net::server::NetServerConfig;
use ss13_shared::protocol::ClientIntent;
use ss13_shared::{Dir, TilePos};
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::job_selection::{JobSelectionStatic, JobSelectionUi, SELECT_JOB};
use ss13_world::SpatialIndex;

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3, "anchored": 0 } },
    "/obj": { "parent": "/atom/movable" },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark",
        "vars": { "name": "start" }, "declared_vars": { "name": "start" } },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start",
        "vars": { "name": "Assistant" }, "declared_vars": { "name": "Assistant" } },
    "/obj/effect/landmark/start/security_officer": { "parent": "/obj/effect/landmark/start",
        "vars": { "name": "Security Officer" }, "declared_vars": { "name": "Security Officer" } },
    "/obj/effect/landmark/latejoin": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/observer_start": { "parent": "/obj/effect/landmark" },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null, "w_class": 3 } },
    "/datum/storage": { "parent": "/datum", "declared_vars": {
        "max_slots": 7, "max_specific_storage": 3, "max_total_storage": 14 } },
    "/datum/storage/backpack": { "parent": "/datum/storage", "declared_vars": {
        "max_slots": 21, "max_total_storage": 21 } },
    "/obj/item/storage": { "parent": "/obj/item", "declared_vars": {
        "storage_type": { "path": "/datum/storage" } } },
    "/obj/item/storage/backpack": { "parent": "/obj/item/storage", "declared_vars": {
        "slot_flags": 1, "w_class": 4, "storage_type": { "path": "/datum/storage/backpack" } } },
    "/obj/item/storage/box": { "parent": "/obj/item/storage", "declared_vars": { "w_class": 2 } },
    "/obj/item/storage/box/survival": { "parent": "/obj/item/storage/box" },
    "/obj/item/storage/box/survival/security": { "parent": "/obj/item/storage/box/survival" },
    "/obj/item/melee": { "parent": "/obj/item" },
    "/obj/item/melee/baton/security/loaded": { "parent": "/obj/item/melee" },
    "/obj/item/bulky": { "parent": "/obj/item", "declared_vars": { "w_class": 5 } },
    "/obj/item/card": { "parent": "/obj/item" },
    "/obj/item/card/id": { "parent": "/obj/item/card", "declared_vars": {
        "name": "identification card", "access": { "list": [] }, "slot_flags": 256 } },
    "/obj/item/card/id/advanced": { "parent": "/obj/item/card/id" },
    "/obj/item/clothing": { "parent": "/obj/item" },
    "/obj/item/clothing/under": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 2 } },
    "/obj/item/clothing/under/color": { "parent": "/obj/item/clothing/under" },
    "/obj/item/clothing/under/color/grey": { "parent": "/obj/item/clothing/under/color" },
    "/obj/item/clothing/under/rank/security/officer": { "parent": "/obj/item/clothing/under" },
    "/obj/item/clothing/shoes": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 128 } },
    "/obj/item/clothing/shoes/sneakers": { "parent": "/obj/item/clothing/shoes" },
    "/obj/item/clothing/shoes/sneakers/black": { "parent": "/obj/item/clothing/shoes/sneakers" },
    "/obj/item/clothing/shoes/jackboots/sec": { "parent": "/obj/item/clothing/shoes" },
    "/obj/machinery": { "parent": "/obj" },
    "/obj/machinery/door": { "parent": "/obj/machinery", "declared_vars": {
        "icon": { "resource": "d.dmi" }, "icon_state": "door_closed", "density": 1, "opacity": 1,
        "layer": 2.7, "base_icon_state": "door", "autoclose": 0, "can_open_with_hands": 1,
        "closingLayer": 3.1, "glass": 0, "locked": 0, "safe": 1, "visible": 1, "welded": 0 } },
    "/obj/machinery/door/airlock": { "parent": "/obj/machinery/door", "declared_vars": {
        "icon": { "resource": "a.dmi" }, "icon_state": "closed", "autoclose": 1,
        "base_icon_state": null, "normalspeed": 1, "overlays_file": { "resource": "o.dmi" },
        "airlock_material": null } },
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

/// Rows top-down, the last line is y = 1. `s` assistant start (2,2), `S` security officer start
/// (6,2), `D` (7,2) needs security access, `L` latejoin (3,3), `O` observer start (9,3).
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"S" = (/obj/effect/landmark/start/security_officer,/turf/open/floor,/area/space)
"D" = (/obj/machinery/door/airlock{req_access = list("security")},/turf/open/floor,/area/space)
"L" = (/obj/effect/landmark/latejoin,/turf/open/floor,/area/space)
"O" = (/obj/effect/landmark/observer_start,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwwwwwww
wfLfffffOw
wsfffSDffw
wwwwwwwwww
"}
"##;

const SPAWN: TilePos = TilePos::new(2, 2, 1);
const OFFICER_START: TilePos = TilePos::new(6, 2, 1);
const D: TilePos = TilePos::new(7, 2, 1);
const LATEJOIN: TilePos = TilePos::new(3, 3, 1);
const OBSERVER_START: TilePos = TilePos::new(9, 3, 1);
const OFFICER: &str = "Security Officer";
const WARDEN: &str = "Warden";
const OFFICER_UNIFORM: &str = "/obj/item/clothing/under/rank/security/officer";

/// Manifest order = `IconStateId`.
const STATES: [(&str, &str); 10] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
    ("a.dmi", "closed"),
    ("a.dmi", "opening"),
    ("a.dmi", "open"),
    ("a.dmi", "closing"),
    ("o.dmi", "lights_denied"),
];

/// The fallback assistant plus a one-slot Security Officer (five once a round starts), a job
/// without an outfit and a one-slot Warden.
fn fixture_jobs() -> JobsData {
    let mut data = fallback_jobs();
    let mut officer = data.jobs[0].clone();
    officer.path = "/datum/job/security_officer".into();
    officer.title = OFFICER.into();
    officer.total_positions = 1;
    officer.spawn_positions = 1;
    officer.assignment = "Офицер".into();
    officer.access = vec!["brig".into(), "security".into(), "maint_tunnels".into()];
    let outfit = officer.outfit.as_mut().unwrap();
    outfit.items = vec![
        (OutfitSlot::Uniform, OFFICER_UNIFORM.into()),
        (
            OutfitSlot::Shoes,
            "/obj/item/clothing/shoes/jackboots/sec".into(),
        ),
        (OutfitSlot::Id, "/obj/item/card/id/advanced".into()),
        // No slot for it yet: recorded, not spawned.
        (
            OutfitSlot::Head,
            "/obj/item/clothing/head/helmet/sec".into(),
        ),
    ];
    // tg `box` first, then `backpack_contents`; the bulky item does not fit and is deleted.
    outfit.backpack_contents = vec![
        ("/obj/item/storage/box/survival/security".into(), 1),
        ("/obj/item/melee/baton/security/loaded".into(), 2),
        ("/obj/item/bulky".into(), 1),
    ];
    data.jobs.push(officer);
    let mut ai = data.jobs[0].clone();
    ai.title = "AI".into();
    ai.outfit = None;
    ai.unresolved = vec!["no outfit (not a human job)".into()];
    data.jobs.push(ai);
    // One slot that round start does not rescale (it does the officers').
    let mut warden = data.jobs[1].clone();
    warden.path = "/datum/job/warden".into();
    warden.title = WARDEN.into();
    warden.assignment = "Смотритель".into();
    data.jobs.push(warden);
    data.departments.push(ss13_mapload::jobs::Department {
        path: "/datum/job_department/security".into(),
        name: "Security".into(),
        color: "#d46a78".into(),
        jobs: vec![1, 2, 3],
    });
    data.names.first_male = vec!["Аарон".into(), "Абель".into(), "Адам".into()];
    data.names.last = vec!["Адамс".into(), "Эддисон".into()];
    data
}

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("jobs_fixture_{test}"));
    let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
    let data = fixture_jobs();
    let mut used = vec![
        "/obj/effect/landmark/start/assistant",
        "/obj/effect/landmark/start/security_officer",
        "/obj/effect/landmark/latejoin",
        "/obj/effect/landmark/observer_start",
        "/obj/machinery/door/airlock",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
    ];
    used.extend(ss13_mapload::RUNTIME_CONTAINER_TYPES);
    // As `ss13-mapload convert` does; the helmet is not a type here.
    used.extend(
        data.outfit_items()
            .into_iter()
            .filter(|p| types.types.contains_key(*p)),
    );
    write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)]);
    std::fs::write(dir.join(JOBS_FILE), data.to_ron()).unwrap();
    dir
}

struct Station {
    st: testing::Station,
    map: MapLoadPlugin,
    config: RoundConfig,
}

impl Deref for Station {
    type Target = testing::Station;
    fn deref(&self) -> &testing::Station {
        &self.st
    }
}

impl DerefMut for Station {
    fn deref_mut(&mut self) -> &mut testing::Station {
        &mut self.st
    }
}

impl Drive for Station {
    fn station(&mut self) -> &mut testing::Station {
        &mut self.st
    }

    fn context(&self) -> String {
        let world = self.server.world();
        let windows: Vec<String> = world
            .try_query::<&ss13_tgui::server::UiWindow>()
            .map(|mut q| {
                q.iter(world)
                    .map(|w| format!("{}#{} {:?} {}", w.interface, w.id, w.status(), w.client))
                    .collect()
            })
            .unwrap_or_default();
        let notices: Vec<_> = self
            .players
            .iter()
            .map(|p| &p.app.world().resource::<Received>().notices)
            .collect();
        format!("; windows {windows:?}; notices {notices:?}")
    }
}

impl Client for Station {}

/// A test server on `addr` (a free port when `None`) in the round state of `config`.
fn server_app(addr: Option<SocketAddr>, map: MapLoadPlugin, config: RoundConfig) -> App {
    testing::server_app(map, |app| {
        app.insert_resource(config);
        if let Some(bind_addr) = addr {
            app.insert_resource(NetServerConfig {
                bind_addr,
                ..default()
            });
        }
    })
}

impl Station {
    fn start(map: MapLoadPlugin, config: RoundConfig) -> Self {
        Self {
            st: testing::Station::new(server_app(None, map.clone(), config.clone())),
            map,
            config,
        }
    }

    /// tg `world.Reboot()`: the server app exited for a reboot; a new one takes its port.
    fn reboot(&mut self) {
        let addr = self.server.world().resource::<NetServerConfig>().bind_addr;
        drop(std::mem::replace(&mut self.st.server, App::new()));
        let start = Instant::now();
        while UdpSocket::bind(addr).is_err() {
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "port stays taken"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
        self.st.server = server_app(Some(addr), self.map.clone(), self.config.clone());
        for p in &mut self.st.players {
            p.seq = 0;
        }
    }

    /// A new client, in the lobby once its handshake is accepted.
    fn connect(&mut self) -> usize {
        let token = format!("p{}", self.players.len());
        let i = self.st.connect(&token, |app| {
            app.init_resource::<Received>().add_systems(Update, record);
        });
        self.wait_lobby(i);
        i
    }

    /// Until client `p` is connected and has the lobby open; finds its connection entity (new
    /// after a reboot).
    fn wait_lobby(&mut self, p: usize) {
        self.run_until("in the lobby", |s| {
            let app = &s.players[p].app;
            *app.world().resource::<NetClientState>() == NetClientState::Connected
                && s.received(p).lobby.is_some()
        });
        self.st.players[p].link = self.st.link_of(&format!("p{p}")).unwrap();
    }

    fn lobby(&self, p: usize) -> &LobbyData {
        &self.received(p).lobby.as_ref().expect("in the lobby").1
    }

    fn state(&self) -> GameState {
        self.server.world().resource::<Ticker>().state()
    }

    fn has_mob(&self, p: usize) -> bool {
        self.server
            .world()
            .resource::<Players>()
            .mob(self.players[p].link)
            .is_some()
    }

    fn pos(&self, p: usize) -> TilePos {
        *self.server.world().get::<TilePos>(self.mob(p)).unwrap()
    }

    fn walk(&mut self, p: usize, dir: Dir) {
        let to = self.pos(p).step(dir);
        self.run_until(&format!("walk {dir:?} to {to:?}"), |s| {
            if s.pos(p) == to {
                return true;
            }
            s.try_step(p, dir);
            false
        });
    }

    /// Holds `dir` for a second: the mob must stay (a closed door it cannot open).
    fn bump(&mut self, p: usize, dir: Dir) {
        let at = self.pos(p);
        for _ in 0..testing::steps(1.0) {
            self.try_step(p, dir);
            self.step();
            assert_eq!(self.pos(p), at, "bumping {dir:?} moved the mob");
        }
    }

    fn door(&self, pos: TilePos) -> Entity {
        let world = self.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .find(|&e| world.get::<Door>(e).is_some())
            .expect("a door")
    }

    /// Waits for a new window of `interface` opened after `before` windows.
    fn wait_window(&mut self, p: usize, before: usize, interface: &str) -> (u32, Vec<u8>) {
        self.run_until(&format!("{interface} opens"), |s| {
            s.received(p).windows[before..]
                .iter()
                .any(|(_, i, _)| i == interface)
        });
        let (id, _, st) = self.received(p).windows[before..]
            .iter()
            .find(|(_, i, _)| i == interface)
            .unwrap();
        (*id, st.clone())
    }

    /// Opens the latejoin menu ("Join game") and returns its window id and static data.
    fn open_menu(&mut self, p: usize) -> (u32, JobSelectionStatic) {
        let before = self.received(p).windows.len();
        self.send(p, ClientIntent::LateJoinMenu);
        let (id, st) = self.wait_window(p, before, JobSelectionUi::NAME);
        (id, ss13_tgui::decode(&st).unwrap())
    }

    fn select(&mut self, p: usize, window: u32, job: &str) {
        self.act(p, window, SELECT_JOB, &[("job", job)]);
    }

    /// Character setup -> the jobs page; `title` at `level` (`""` never, `"1"`..`"3"`).
    fn set_job_pref(&mut self, p: usize, title: &str, level: &str) {
        let before = self.received(p).windows.len();
        self.lobby_act(p, CHARACTER_SETUP);
        let (window, _) = self.wait_window(p, before, JobPrefsUi::NAME);
        self.act(
            p,
            window,
            SET_JOB_PREFERENCE,
            &[("job", title), ("level", level)],
        );
    }

    fn wait_notice(&mut self, p: usize, part: &str) {
        self.run_until(&format!("a notice with {part:?}"), |s| {
            s.received(p).notices.iter().any(|n| n.contains(part))
        });
    }

    fn job(&self, p: usize) -> String {
        let world = self.server.world();
        let job = world.get::<PlayerJob>(self.mob(p)).unwrap().0;
        world.resource::<Jobs>().data.jobs[job].title.clone()
    }

    fn id_card(&self, p: usize) -> Entity {
        self.server
            .world()
            .get::<Inventory>(self.mob(p))
            .unwrap()
            .id
            .expect("an ID")
    }
}

fn fixture_map(test: &str) -> MapLoadPlugin {
    testing::fixture_map(&fixture_assets(test), &["map.dmm"])
}

/// The round is in progress from the start (tg `start_immediately`): clients land in the lobby
/// and latejoin.
fn started() -> RoundConfig {
    RoundConfig {
        lobby_secs: 0.0,
        ..RoundConfig::tg()
    }
}

#[test]
fn latejoin_officer_slots_and_access() {
    let mut s = Station::start(fixture_map("latejoin"), started());
    let a = s.connect();
    s.run_until("the round starts", |s| s.state() == GameState::Playing);
    // In the lobby: no body, a name already.
    assert!(!s.has_mob(a));
    let name = s.lobby(a).name.clone();
    assert!(
        name.contains(' ') && !name.starts_with("Игрок"),
        "random name {name:?}"
    );
    s.run_until("the lobby shows the round", |s| {
        s.lobby(a).state == GameState::Playing
    });

    // The menu lists the departments and jobs of the data.
    let (window, menu) = s.open_menu(a);
    let listed: Vec<(&str, Vec<&str>)> = menu
        .departments
        .iter()
        .map(|d| {
            let jobs = d.jobs.iter().map(|j| j.title.as_str()).collect();
            (d.name.as_str(), jobs)
        })
        .collect();
    assert_eq!(
        listed,
        [
            ("No Department", vec!["Assistant"]),
            ("Security", vec![OFFICER, "AI", WARDEN])
        ]
    );
    // A latejoin assistant arrives at the latejoin landmark, with the lobby name.
    s.select(a, window, "Assistant");
    s.run_until("a body", |s| s.has_mob(a));
    assert_eq!(s.job(a), "Assistant");
    assert_eq!(s.pos(a), LATEJOIN);
    assert_eq!(
        s.server.world().get::<Name>(s.mob(a)).unwrap().as_str(),
        name
    );
    s.run_until("the lobby closes", |s| s.received(a).lobby.is_none());
    assert!(
        s.server
            .world()
            .get::<NewPlayer>(s.players[a].link)
            .is_none()
    );

    // The assistant cannot open the security door.
    s.walk(a, Dir::East);
    s.walk(a, Dir::East);
    s.walk(a, Dir::East);
    s.walk(a, Dir::South);
    assert_eq!(s.pos(a), OFFICER_START);
    let door = s.door(D);
    s.bump(a, Dir::East);
    assert_eq!(
        s.server.world().get::<Door>(door).unwrap().state,
        DoorState::Closed
    );
    // A body has no latejoin menu.
    s.send(a, ClientIntent::LateJoinMenu);
    s.run_for(0.3);
    assert_eq!(s.menus(a), 1);

    // The officer latejoins at the latejoin landmark with the trim's access and opens the door.
    let b = s.connect();
    let (window, _) = s.open_menu(b);
    s.select(b, window, OFFICER);
    s.run_until("b's body", |s| s.has_mob(b));
    assert_eq!(s.job(b), OFFICER);
    assert_eq!(s.pos(b), LATEJOIN);
    let b_name = s.lobby_name_of(b);
    let world = s.server.world();
    let card = s.id_card(b);
    assert_eq!(
        world.get::<IdCard>(card).unwrap().access,
        ["brig", "security", "maint_tunnels"]
    );
    assert_eq!(
        world.get::<DmName>(card).unwrap().name,
        format!("ID-карта «{b_name}» (Офицер)")
    );
    let registration = world.get::<IdRegistration>(card).unwrap();
    assert_eq!(
        (
            registration.job.as_str(),
            registration.registered_name.as_str()
        ),
        (OFFICER, b_name.as_str())
    );
    let inventory = world.get::<Inventory>(s.mob(b)).unwrap();
    let uniform = world
        .get::<ss13_mapload::DmItem>(inventory.uniform.unwrap())
        .unwrap();
    assert_eq!(uniform.path, OFFICER_UNIFORM);
    assert!(inventory.shoes.is_some());
    // The backpack holds the outfit's `box` and `backpack_contents`; the bulky item did not fit.
    let pack = inventory.back.expect("a backpack");
    let mut packed: Vec<String> = s
        .server
        .world_mut()
        .query::<(&StoredIn, &ss13_mapload::DmItem)>()
        .iter(s.server.world())
        .filter(|(stored, _)| stored.0 == pack)
        .map(|(_, item)| item.path.clone())
        .collect();
    packed.sort();
    assert_eq!(
        packed,
        [
            "/obj/item/melee/baton/security/loaded",
            "/obj/item/melee/baton/security/loaded",
            "/obj/item/storage/box/survival/security",
        ]
    );
    let world = s.server.world();
    assert_eq!(world.resource::<Jobs>().positions(1), 1);
    // Around the assistant to the far side, then into the doorway.
    for _ in 0..5 {
        s.walk(b, Dir::East);
    }
    s.walk(b, Dir::South);
    assert_eq!(s.pos(b), D.step(Dir::East));
    s.walk(b, Dir::West);
    assert_eq!(s.pos(b), D);

    // Round start gave the officers their minimum of slots (`setup_officer_positions`).
    assert_eq!(
        s.server.world().resource::<Jobs>().data.jobs[1].total_positions,
        5
    );

    // c takes the one warden slot; d is refused it, and unknown and unplayable jobs too.
    let c = s.connect();
    let (window, _) = s.open_menu(c);
    s.select(c, window, WARDEN);
    s.run_until("c's body", |s| s.has_mob(c));
    let d = s.connect();
    let (window, _) = s.open_menu(d);
    s.select(d, window, WARDEN);
    s.wait_notice(d, "Роль Смотритель уже заполнена до максимума.");
    s.select(d, window, "Clown");
    s.wait_notice(d, "Профессии «Clown» нет.");
    s.select(d, window, "AI");
    s.wait_notice(d, "ИИ недоступна для выбора.");
    s.run_for(0.3);
    assert!(!s.has_mob(d));

    // The warden's body is gone (a disconnect leaves it, as in tg): the slot frees, d takes it.
    let body = s.mob(c);
    s.server.world_mut().despawn(body);
    s.run_until("the slot frees", |s| {
        s.server.world().resource::<Jobs>().positions(3) == 0
    });
    s.select(d, window, WARDEN);
    s.run_until("d becomes the warden", |s| s.has_mob(d));
    assert_eq!(s.job(d), WARDEN);

    // e observes: a ghost at the observer start, and no latejoin menu any more.
    let e = s.connect();
    s.lobby_act(e, OBSERVE);
    s.run_until("e observes", |s| s.has_mob(e));
    let ghost = s.mob(e);
    assert_eq!(s.server.world().get::<Ghost>(ghost).unwrap().body, None);
    assert_eq!(s.pos(e), OBSERVER_START);
    s.send(e, ClientIntent::LateJoinMenu);
    s.run_for(0.3);
    assert_eq!(s.menus(e), 0);
}

impl Station {
    /// Latejoin menus client `p` was shown.
    fn menus(&self, p: usize) -> usize {
        self.received(p)
            .windows
            .iter()
            .filter(|(_, i, _)| i == JobSelectionUi::NAME)
            .count()
    }

    fn lobby_name_of(&self, p: usize) -> String {
        self.server
            .world()
            .get::<CharacterName>(self.players[p].link)
            .unwrap()
            .0
            .clone()
    }
}

/// Two clients in the lobby set preferences and get ready; the countdown runs out and both spawn
/// in the same tick at their jobs' start landmarks; a third latejoins; an admin ends the round:
/// the report goes to the chat and the rebooted server has all three back in a new lobby.
/// The appearance section of character setup: checked choices are kept for the connection and
/// shape the body the player joins with; an index outside its table is refused.
#[test]
fn appearance_preferences_shape_the_body() {
    let mut s = Station::start(fixture_map("appearance"), started());
    let a = s.connect();
    s.run_until("the lobby shows the round", |s| {
        s.lobby(a).state == GameState::Playing
    });
    let before = s.received(a).windows.len();
    s.lobby_act(a, CHARACTER_SETUP);
    let (window, _) = s.wait_window(a, before, JobPrefsUi::NAME);
    let eye_before = s
        .server
        .world()
        .get::<AppearancePrefs>(s.players[a].link)
        .unwrap()
        .eye_color;
    for (key, value) in [
        ("female", "1"),
        ("skin_tone", "15"),
        ("hair_style", ""),
        ("eye_color", "99"),
    ] {
        s.act(a, window, SET_APPEARANCE, &[("key", key), ("value", value)]);
    }
    s.run_until("the appearance is stored", |s| {
        s.server
            .world()
            .get::<AppearancePrefs>(s.players[a].link)
            .is_some_and(|p| p.female && p.skin_tone == 15 && p.hair_style.is_none())
    });
    let prefs = *s
        .server
        .world()
        .get::<AppearancePrefs>(s.players[a].link)
        .unwrap();
    assert_eq!(
        prefs.eye_color, eye_before,
        "out-of-range eye color refused"
    );

    let (window, _) = s.open_menu(a);
    s.select(a, window, "Assistant");
    s.run_until("a body", |s| s.has_mob(a));
    let look = s.server.world().get::<HumanLook>(s.mob(a)).unwrap().clone();
    assert!(look.female);
    assert_eq!(look.skin_tone, SKIN_TONES[15].0);
    assert_eq!(look.hair_style, None);
}

#[test]
fn lobby_round_start_latejoin_and_reboot() {
    let config = RoundConfig {
        lobby_secs: 600.0,
        round_end_secs: 1.0,
        source_url: Some("https://example.org/ss13-rs".into()),
        ..RoundConfig::tg()
    };
    let mut s = Station::start(fixture_map("round"), config);
    let a = s.connect();
    let b = s.connect();
    s.wait_chat(a, "нажмите \"Готов\"");
    s.run_until("the countdown shows", |s| {
        s.lobby(a).state == GameState::Pregame && s.lobby(a).time_left.is_some()
    });
    // The AGPL source offer reaches the lobby.
    assert_eq!(
        s.lobby(a).source_url.as_deref(),
        Some("https://example.org/ss13-rs")
    );

    // Ready needs a job preference (BandaStation).
    s.lobby_act(a, TOGGLE_READY);
    s.wait_chat(a, "Выберите предпочитаемую должность");
    assert!(!s.lobby(a).ready);

    s.set_job_pref(a, OFFICER, "3");
    s.set_job_pref(b, "Assistant", "3");
    s.run_until("preferences are stored", |s| {
        let world = s.server.world();
        [a, b].iter().all(|&p| {
            world
                .get::<ss13_game::round::JobPreferences>(s.players[p].link)
                .is_some_and(|prefs| !prefs.levels.is_empty())
        })
    });
    s.lobby_act(a, TOGGLE_READY);
    s.lobby_act(b, TOGGLE_READY);
    s.run_until("both ready", |s| {
        s.lobby(a).ready && s.lobby(b).ready && s.lobby(a).ready_players == 2
    });
    assert_eq!(s.state(), GameState::Pregame);

    // Round start: both spawn in the tick the round begins.
    s.start_now();
    s.run_until("the round starts", |s| s.state() >= GameState::Playing);
    assert!(s.has_mob(a) && s.has_mob(b), "spawned with the round start");
    assert_eq!(s.job(a), OFFICER);
    assert_eq!(s.pos(a), OFFICER_START);
    assert_eq!(s.job(b), "Assistant");
    assert_eq!(s.pos(b), SPAWN);
    s.wait_chat(a, "Игра начинается...");
    s.wait_chat(b, "Приветствуем вас на map");
    s.run_until("bodies reach the clients", |s| {
        [a, b].iter().all(|&p| {
            s.players[p]
                .app
                .world()
                .resource::<NetEntityMap>()
                .controlled()
                .is_some()
                && s.received(p).lobby.is_none()
        })
    });

    // A third client latejoins.
    let c = s.connect();
    s.run_until("c's lobby shows the round", |s| {
        s.lobby(c).state == GameState::Playing
    });
    let (window, _) = s.open_menu(c);
    s.select(c, window, "Assistant");
    s.run_until("c's body", |s| s.has_mob(c));
    assert_eq!(s.pos(c), LATEJOIN);

    // An admin ends the round: report, reboot notice, reboot.
    s.server.world_mut().resource_mut::<Ticker>().force_ending = true;
    for p in [a, b, c] {
        s.wait_chat(p, "Раунд закончился.");
    }
    s.wait_chat(a, "Вам удалось пережить события на map как");
    s.wait_chat(c, "Общая численность экипажа: 3");
    s.wait_chat(c, "Перезагрузка мира через 1 секунда. Раунд окончен.");
    s.run_until("the world reboots", |s| {
        s.server.should_exit() == Some(AppExit::from_code(REBOOT_EXIT_CODE))
    });
    s.reboot();
    for p in [a, b, c] {
        s.players[p]
            .app
            .world_mut()
            .resource_mut::<Received>()
            .lobby = None;
    }
    for p in [a, b, c] {
        s.wait_lobby(p);
    }
    s.run_until("a new countdown", |s| {
        [a, b, c]
            .iter()
            .all(|&p| s.lobby(p).state == GameState::Pregame && !s.lobby(p).ready)
    });
    for p in [a, b, c] {
        assert!(!s.has_mob(p));
        assert!(
            s.players[p]
                .app
                .world()
                .resource::<NetEntityMap>()
                .controlled()
                .is_none()
        );
    }
}

fn metastation_map() -> MapLoadPlugin {
    MapLoadPlugin {
        maps: vec!["MetaStation".into()],
        ..MapLoadPlugin::from_env().unwrap_or(MapLoadPlugin {
            source: env!("SS13_SOURCE").into(),
            maps: vec![],
            assets: std::env::var("SS13_ASSETS")
                .unwrap_or_else(|_| "assets/generated".into())
                .into(),
        })
    }
}

/// Starts a MetaStation round as a Security Officer: spawns on a security officer start with the
/// sec ID, and opens a security door the assistant ID cannot.
#[test]
#[ignore = "needs generated assets and MetaStation; run with --ignored"]
fn metastation_security_officer() {
    let config = RoundConfig {
        lobby_secs: 600.0,
        ..RoundConfig::tg()
    };
    let mut s = Station::start(metastation_map(), config);
    let a = s.connect();
    let assistant = {
        let world = s.server.world();
        let jobs = world.resource::<Jobs>();
        let data = &jobs.data;
        let spawns = &world.resource::<JobSpawns>().by_title;
        let playable: Vec<&str> = data
            .jobs
            .iter()
            .filter(|j| j.unresolved.is_empty())
            .map(|j| j.title.as_str())
            .collect();
        let without: Vec<&str> = playable
            .iter()
            .copied()
            .filter(|t| !spawns.contains_key(*t))
            .collect();
        println!(
            "{} jobs, {} playable, {} of them without a MetaStation start landmark: {without:?}",
            data.jobs.len(),
            playable.len(),
            without.len()
        );
        data.jobs[jobs.default_job].access.clone()
    };
    s.set_job_pref(a, OFFICER, "3");
    s.run_until("the preference is stored", |s| {
        s.server
            .world()
            .get::<ss13_game::round::JobPreferences>(s.players[a].link)
            .is_some_and(|prefs| !prefs.levels.is_empty())
    });
    s.lobby_act(a, TOGGLE_READY);
    s.run_until("ready", |s| s.lobby(a).ready);
    s.start_now();
    s.run_until("the round starts", |s| s.has_mob(a));
    assert_eq!(s.job(a), OFFICER);
    let pos = s.pos(a);
    let starts = s.server.world().resource::<JobSpawns>().by_title[OFFICER].clone();
    println!("officer spawned at {pos:?}; security officer starts: {starts:?}");
    assert!(starts.contains(&pos), "not on a security officer start");
    let world = s.server.world();
    let card = s.id_card(a);
    let access = world.get::<IdCard>(card).unwrap().access.clone();
    println!(
        "card {:?}: {access:?}",
        world.get::<DmName>(card).unwrap().name
    );
    assert!(access.iter().any(|a| a == "security"));

    // The nearest closed, unbolted security airlock the officer opens and the assistant does not.
    let mut q = s.server.world_mut().query::<(Entity, &Door, &TilePos)>();
    let door = q
        .iter(s.server.world())
        .filter(|(_, d, p)| {
            d.kind == ss13_mapload::DoorKind::Airlock
                && d.state == DoorState::Closed
                && !d.locked
                && !d.welded
                && d.unres_sides == 0
                && p.z == pos.z
                && d.req_access
                    .iter()
                    .chain(&d.req_one_access)
                    .any(|r| r == "security")
                && d.allows(Some(&access))
                && !d.allows(Some(&assistant))
        })
        .min_by_key(|(e, _, p)| (p.chebyshev(pos).unwrap_or(u32::MAX), *e))
        .map(|(e, _, _)| e)
        .expect("a security door");
    let at = *s.server.world().get::<TilePos>(door).unwrap();
    let beside = Dir::CARDINALS
        .into_iter()
        .map(|d| at.step(d))
        .find(|&p| {
            let world = s.server.world();
            world.resource::<SpatialIndex>().dense_on_tile(p).is_empty()
        })
        .expect("a free tile beside the door");
    let mob = s.mob(a);
    s.server.world_mut().entity_mut(mob).insert(beside);
    s.run_until("the door is replicated", |s| s.net_id(a, door).is_some());
    s.click(a, door);
    s.run_until("the security door opens", |s| {
        matches!(
            s.server.world().get::<Door>(door).unwrap().state,
            DoorState::Opening { .. } | DoorState::Open
        )
    });
    let d = s.server.world().get::<Door>(door).unwrap();
    println!(
        "security door at {at:?} {:?} {:?} opened for the officer",
        d.req_access, d.req_one_access
    );
}
