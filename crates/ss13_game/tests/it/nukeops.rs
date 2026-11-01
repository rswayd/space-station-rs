//! Nuclear operatives over real WebTransport on 127.0.0.1, on a fixture station with a fixture
//! infiltrator: three clients in the lobby, two opt in to "Operative"; an admin-queued
//! nukeops ruleset (forced) makes both operatives (cap `ceil(3 / 18) + 1` = 2) with no station
//! job, aboard the infiltrator in syndicate gear (ID with syndicate access, the nuclear uplink
//! with 25 + 5 TC, the leader's code paper). The leader flies the infiltrator to the station's
//! `syndicate_nw` dock with its console, inserts the disk into the bomb, enters the team's code
//! and a timer, arms it, disarms it, arms it again, flies home; the detonation kills the crew on
//! the station, ends the round, and the report says the syndicate won.

use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_chat::server::ChatServerPlugin;
use ss13_game::antag::dynamic::{DevTraitor, DynamicConfig, NUKEOPS_RULESET};
use ss13_game::antag::nukeop::bomb::{NuclearBomb, UiMode};
use ss13_game::antag::nukeop::server::{
    Flight, Infiltrator, NukeDisk, NukeOutcome, NukeTeam, Nukeop, NukeopConfig,
};
use ss13_game::antag::nukeop::{
    ARM, DIGIT, KEYPAD, MOVE, NuclearBombData, NuclearBombUi, ROLE_OPERATIVE, SHUTTLE_ID,
    ShuttleConsoleData, ShuttleConsoleUi,
};
use ss13_game::antag::uplink::Uplink;
use ss13_game::health::Stat;
use ss13_game::inventory::{CarriedBy, IdCard, Inventory};
use ss13_game::jobs::{PlayerJob, fallback_jobs};
use ss13_game::lobby::{
    CHARACTER_SETUP, GameState, JobPrefsUi, LobbyData, LobbyUi, SET_ANTAGS, SET_JOB_PREFERENCE,
    TOGGLE_READY,
};
use ss13_game::round::{RoundConfig, Ticker};
use ss13_game::server::Players;
use ss13_game::shuttle::MobileDock;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::antag::{ANTAG_FILE, AntagData, NUKEOP_RUNTIME_TYPES};
use ss13_mapload::jobs::{JOBS_FILE, Outfit, OutfitSlot};
use ss13_mapload::{DmItem, DmObj};
use ss13_net::chat::client::ChatReceived;
use ss13_net::ui::client::UiMessageReceived;
use ss13_shared::TilePos;
use ss13_shared::ui::UiServerMessage;
use ss13_tgui::UiInterface;

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/datum/storage": { "parent": "/datum", "declared_vars": {
        "max_slots": 7, "max_specific_storage": 3, "max_total_storage": 14 } },
    "/datum/storage/backpack": { "parent": "/datum/storage", "declared_vars": {
        "max_slots": 21, "max_total_storage": 21 } },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 }, "declared_vars": { "can_atmos_pass": 1 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3, "anchored": 0 } },
    "/obj": { "parent": "/atom/movable" },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark",
        "vars": { "name": "start" }, "declared_vars": { "name": "start" } },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start",
        "vars": { "name": "Assistant" }, "declared_vars": { "name": "Assistant" } },
    "/obj/effect/landmark/latejoin": { "parent": "/obj/effect/landmark" },
    "/obj/docking_port": { "parent": "/obj", "declared_vars": { "invisibility": 101,
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "anchored": 1, "dir": 1,
        "shuttle_id": null, "width": 0, "height": 0, "dwidth": 0, "dheight": 0 } },
    "/obj/docking_port/stationary": { "parent": "/obj/docking_port", "declared_vars": {
        "json_key": null } },
    "/obj/docking_port/mobile": { "parent": "/obj/docking_port" },
    "/obj/docking_port/mobile/infiltrator": { "parent": "/obj/docking_port/mobile",
        "declared_vars": { "name": "syndicate infiltrator", "shuttle_id": "syndicate",
        "dir": 8 } },
    "/obj/machinery": { "parent": "/obj", "declared_vars": { "anchored": 1 } },
    "/obj/machinery/nuclearbomb": { "parent": "/obj/machinery", "declared_vars": {
        "name": "nuclear fission explosive", "icon": { "resource": "icons/obj/machines/nuke.dmi" },
        "icon_state": "nuclearbomb_base", "density": 1, "anchored": 0 } },
    "/obj/machinery/nuclearbomb/syndicate": { "parent": "/obj/machinery/nuclearbomb" },
    "/obj/machinery/computer": { "parent": "/obj/machinery", "declared_vars": {
        "icon": { "resource": "c.dmi" }, "icon_state": "computer", "density": 1 } },
    "/obj/machinery/computer/shuttle": { "parent": "/obj/machinery/computer" },
    "/obj/machinery/computer/shuttle/syndicate": { "parent": "/obj/machinery/computer/shuttle",
        "declared_vars": { "name": "syndicate shuttle terminal" } },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null, "w_class": 2,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null } },
    "/obj/item/disk": { "parent": "/obj/item" },
    "/obj/item/disk/nuclear": { "parent": "/obj/item/disk", "declared_vars": {
        "name": "nuclear authentication disk" } },
    "/obj/item/paper": { "parent": "/obj/item", "declared_vars": { "name": "paper" } },
    "/obj/item/nuclear_challenge": { "parent": "/obj/item", "declared_vars": {
        "name": "Declaration of War (Challenge Mode)" } },
    "/obj/item/uplink": { "parent": "/obj/item", "declared_vars": { "name": "station bounced radio" } },
    "/obj/item/uplink/nuclear": { "parent": "/obj/item/uplink" },
    "/obj/item/pinpointer": { "parent": "/obj/item", "declared_vars": { "name": "pinpointer" } },
    "/obj/item/pinpointer/nuke": { "parent": "/obj/item/pinpointer" },
    "/obj/item/pinpointer/nuke/syndicate": { "parent": "/obj/item/pinpointer/nuke" },
    "/obj/item/card": { "parent": "/obj/item" },
    "/obj/item/card/id": { "parent": "/obj/item/card", "declared_vars": {
        "access": { "list": [] }, "slot_flags": 256 } },
    "/obj/item/card/id/advanced": { "parent": "/obj/item/card/id" },
    "/obj/item/card/id/advanced/chameleon": { "parent": "/obj/item/card/id/advanced" },
    "/obj/item/clothing": { "parent": "/obj/item" },
    "/obj/item/clothing/under": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 2 } },
    "/obj/item/clothing/under/color": { "parent": "/obj/item/clothing/under" },
    "/obj/item/clothing/under/color/grey": { "parent": "/obj/item/clothing/under/color" },
    "/obj/item/clothing/under/syndicate": { "parent": "/obj/item/clothing/under" },
    "/obj/item/clothing/shoes": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 128 } },
    "/obj/item/clothing/shoes/sneakers": { "parent": "/obj/item/clothing/shoes" },
    "/obj/item/clothing/shoes/sneakers/black": { "parent": "/obj/item/clothing/shoes/sneakers" },
    "/obj/item/radio": { "parent": "/obj/item", "declared_vars": { "name": "station bounced radio",
        "on": 1, "frequency": 1459, "broadcasting": 0, "listening": 1, "canhear_range": 3,
        "subspace_transmission": 0, "freerange": 0, "keyslot": null, "special_channels": 0,
        "slot_flags": 512 } },
    "/obj/item/radio/headset": { "parent": "/obj/item/radio", "declared_vars": {
        "name": "radio headset", "canhear_range": 0, "subspace_transmission": 1,
        "keyslot2": null, "slot_flags": 16 } },
    "/obj/item/radio/headset/syndicate": { "parent": "/obj/item/radio/headset",
        "declared_vars": { "special_channels": 4 } },
    "/obj/item/storage": { "parent": "/obj/item", "declared_vars": {
        "storage_type": { "path": "/datum/storage" } } },
    "/obj/item/storage/backpack": { "parent": "/obj/item/storage", "declared_vars": {
        "slot_flags": 1, "w_class": 4, "storage_type": { "path": "/datum/storage/backpack" } } },
    "/obj/item/storage/box": { "parent": "/obj/item/storage" },
    "/obj/item/storage/box/survival": { "parent": "/obj/item/storage/box" },
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
    "/area/shuttle/syndicate": { "parent": "/area/shuttle", "declared_vars": {
        "name": "Syndicate Infiltrator" } },
    "/area/shuttle/syndicate/hallway": { "parent": "/area/shuttle/syndicate" }
  }
}"##;

/// Rows top-down, the last line is y = 1. The crew spawns at `s` (8,8), the disk `D` lies at
/// (4,7); `syndicate_nw` `N` (5,4), facing south, sized for the 5x4 infiltrator below the wall.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/station)
"f" = (/turf/open/floor,/area/station)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/station)
"L" = (/obj/effect/landmark/latejoin,/turf/open/floor,/area/station)
"D" = (/obj/item/disk/nuclear,/turf/open/floor,/area/station)
"e" = (/turf/open/space/basic,/area/space)
"N" = (/obj/docking_port/stationary{dir = 2; dwidth = 2; height = 4; name = "northwest of station"; shuttle_id = "syndicate_nw"; width = 5},/turf/open/space/basic,/area/space)

(1,1,1) = {"
wwwwwwwwww
wffffffsLw
wffDffffsw
wffffffffw
wwwwwwwwww
eeeeNeeeee
eeeeeeeeee
eeeeeeeeee
eeeeeeeeee
"}
"##;

/// The infiltrator: the port in the top row's middle, the bomb `B` and the console `K`.
const INFILTRATOR: &str = r##""w" = (/turf/closed/wall/shuttle,/area/shuttle/syndicate/hallway)
"f" = (/turf/open/floor/shuttle,/area/shuttle/syndicate/hallway)
"p" = (/obj/docking_port/mobile/infiltrator{dir = 2},/turf/open/floor/shuttle,/area/shuttle/syndicate/hallway)
"B" = (/obj/machinery/nuclearbomb/syndicate,/turf/open/floor/shuttle,/area/shuttle/syndicate/hallway)
"K" = (/obj/machinery/computer/shuttle/syndicate,/turf/open/floor/shuttle,/area/shuttle/syndicate/hallway)

(1,1,1) = {"
wwpww
wfBfw
wffKw
wwwww
"}
"##;

const STATES: [(&str, &str); 16] = [
    ("t.dmi", "floor"),
    ("t.dmi", "plating"),
    ("t.dmi", "wall"),
    ("t.dmi", "space"),
    ("t.dmi", "shuttle"),
    ("t.dmi", "swall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
    ("c.dmi", "computer"),
    ("icons/obj/machines/nuke.dmi", "nuclearbomb_base"),
    ("icons/obj/machines/nuke.dmi", "nuclearbomb_timing"),
    ("icons/obj/machines/nuke.dmi", "nuclearbomb_exploding"),
    ("icons/obj/machines/nuke.dmi", "lights-safety"),
    ("icons/obj/machines/nuke.dmi", "lights-timing"),
    ("icons/obj/machines/nuke.dmi", "lights-exploding"),
];

fn operative_outfit(path: &str) -> Outfit {
    Outfit {
        path: path.into(),
        items: vec![
            (
                OutfitSlot::Uniform,
                "/obj/item/clothing/under/syndicate".into(),
            ),
            (OutfitSlot::Ears, "/obj/item/radio/headset/syndicate".into()),
            (
                OutfitSlot::Id,
                "/obj/item/card/id/advanced/chameleon".into(),
            ),
        ],
        backpack_contents: vec![("/obj/item/storage/box/survival".into(), 1)],
    }
}

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("nukeops_fixture")
        .join(test);
    let antag = AntagData {
        nukeop_outfits: vec![
            operative_outfit("/datum/outfit/syndicate"),
            operative_outfit("/datum/outfit/syndicate/leader"),
        ],
        operative_aliases: vec!["Агент".into()],
        last_names: vec!["Смит".into()],
        ..Default::default()
    };
    let mut used = vec![
        "/obj/effect/landmark/start/assistant",
        "/obj/effect/landmark/latejoin",
        "/obj/docking_port/stationary",
        "/obj/docking_port/mobile/infiltrator",
        "/obj/machinery/nuclearbomb/syndicate",
        "/obj/machinery/computer/shuttle/syndicate",
        "/turf/open/floor",
        "/turf/open/floor/plating",
        "/turf/open/floor/shuttle",
        "/turf/closed/wall",
        "/turf/closed/wall/shuttle",
        "/turf/open/space/basic",
        "/area/space",
        "/area/station",
        "/area/shuttle/syndicate/hallway",
    ];
    used.extend(ss13_mapload::RUNTIME_CONTAINER_TYPES);
    used.extend(NUKEOP_RUNTIME_TYPES);
    let types = antag.nukeop_types();
    used.extend(types.iter().copied());
    write_fixture(
        &dir,
        TYPES,
        &used,
        &STATES,
        &[("map.dmm", MAP), ("infiltrator.dmm", INFILTRATOR)],
    );
    std::fs::write(dir.join(JOBS_FILE), fallback_jobs().to_ron()).unwrap();
    std::fs::write(dir.join(ANTAG_FILE), antag.to_ron()).unwrap();
    dir
}

#[derive(Resource, Default)]
struct Received {
    windows: Vec<(u32, String)>,
    data: Vec<(u32, Vec<u8>)>,
    lobby: Option<(u32, LobbyData)>,
    chat: Vec<String>,
}

fn record(
    mut ui: MessageReader<UiMessageReceived>,
    mut chat: MessageReader<ChatReceived>,
    mut received: ResMut<Received>,
) {
    for UiMessageReceived(message) in ui.read() {
        match message {
            UiServerMessage::Open {
                window,
                interface,
                data,
                ..
            } => {
                if interface == LobbyUi::NAME {
                    received.lobby = Some((*window, ss13_tgui::decode(data).unwrap()));
                }
                received.windows.push((*window, interface.clone()));
                received.data.push((*window, data.clone()));
            }
            UiServerMessage::Update {
                window,
                data: Some(data),
                ..
            } => {
                if let Some((id, lobby)) = &mut received.lobby
                    && id == window
                {
                    *lobby = ss13_tgui::decode(data).unwrap();
                }
                received.data.push((*window, data.clone()));
            }
            UiServerMessage::Update { .. } => {}
            UiServerMessage::Close { window } => {
                if received.lobby.as_ref().is_some_and(|(id, _)| id == window) {
                    received.lobby = None;
                }
            }
        }
    }
    for ChatReceived(message) in chat.read() {
        received.chat.push(message.plain_text());
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
        format!("; chat {chat:?}")
    }
}

impl Station {
    fn start(test: &str) -> Self {
        let assets = fixture_assets(test);
        let infiltrator = assets.join("infiltrator.dmm");
        Self(testing::Station::start(
            fixture_map(&assets, &["map.dmm"]),
            |app| {
                app.insert_resource(RoundConfig {
                    lobby_secs: 600.0,
                    round_end_secs: 600.0,
                    ..RoundConfig::tg()
                })
                .insert_resource(DynamicConfig {
                    forced_rulesets: Some(vec![NUKEOPS_RULESET]),
                    protect_roles: true,
                    seed: Some(7),
                    dev_traitor: DevTraitor::Off,
                })
                .insert_resource(NukeopConfig {
                    infiltrator: Some(infiltrator),
                })
                .add_plugins(ChatServerPlugin);
            },
        ))
    }

    fn start_now(&mut self) {
        let world = self.server.world_mut();
        world.resource_mut::<RoundConfig>().lobby_secs = 0.5;
        world.insert_resource(Ticker::default());
    }

    fn connect(&mut self) -> usize {
        let token = format!("p{}", self.players.len());
        let i = self.0.connect(&token, |app| {
            app.init_resource::<Received>().add_systems(Update, record);
        });
        self.run_until("in the lobby", |s| s.received(i).lobby.is_some());
        i
    }

    fn received(&self, p: usize) -> &Received {
        self.players[p].app.world().resource::<Received>()
    }

    fn lobby_act(&mut self, p: usize, action: &str) {
        let window = self.received(p).lobby.as_ref().expect("in the lobby").0;
        self.act(p, window, action, &[]);
    }

    fn wait_window(&mut self, p: usize, interface: &str) -> u32 {
        self.run_until(&format!("{interface} opens"), |s| {
            s.received(p).windows.iter().any(|(_, i)| i == interface)
        });
        self.received(p)
            .windows
            .iter()
            .rev()
            .find(|(_, i)| i == interface)
            .unwrap()
            .0
    }

    fn latest_data<T: serde::de::DeserializeOwned>(&self, p: usize, window: u32) -> T {
        let (_, data) = self
            .received(p)
            .data
            .iter()
            .rev()
            .find(|(w, _)| *w == window)
            .unwrap();
        ss13_tgui::decode(data).unwrap()
    }

    fn wait_chat(&mut self, p: usize, part: &str) {
        self.run_until(&format!("a chat line with {part:?}"), |s| {
            s.received(p).chat.iter().any(|n| n.contains(part))
        });
    }

    fn mob(&self, p: usize) -> Entity {
        self.server
            .world()
            .resource::<Players>()
            .mob(self.players[p].link)
            .unwrap()
    }

    fn find_obj(&mut self, path: &str) -> Entity {
        let world = self.server.world_mut();
        let mut q = world.query::<(Entity, &DmObj)>();
        q.iter(world).find(|(_, o)| o.path == path).unwrap().0
    }

    fn pos(&self, e: Entity) -> TilePos {
        *self.server.world().get::<TilePos>(e).unwrap()
    }

    /// Moves `mob` onto `pos` (the test's stand-in for walking there).
    fn put(&mut self, mob: Entity, pos: TilePos) {
        self.server.world_mut().entity_mut(mob).insert(pos);
        self.run_for(0.2);
    }

    fn bomb_mode(&self, bomb: Entity) -> UiMode {
        self.server
            .world()
            .get::<NuclearBomb>(bomb)
            .unwrap()
            .state
            .ui_mode
    }

    fn keys(&mut self, p: usize, window: u32, keys: &str) {
        for k in keys.chars() {
            self.act(p, window, KEYPAD, &[(DIGIT, &k.to_string())]);
            self.run_for(0.1);
        }
    }

    /// Drops what `mob` holds at its feet (the test's stand-in for the drop key).
    fn free_hands(&mut self, mob: Entity) {
        let world = self.0.server.world_mut();
        let at = *world.get::<TilePos>(mob).unwrap();
        let held: Vec<Entity> = world
            .get::<Inventory>(mob)
            .unwrap()
            .hands
            .iter()
            .flatten()
            .copied()
            .collect();
        for item in held {
            world.entity_mut(item).remove::<CarriedBy>().insert(at);
        }
    }

    /// Flies the infiltrator with its console to `dock`.
    fn fly(&mut self, p: usize, dock: &str) {
        let console = self.find_obj("/obj/machinery/computer/shuttle/syndicate");
        let at = self.pos(console);
        let mob = self.mob(p);
        self.put(mob, at.step(ss13_shared::Dir::West));
        self.free_hands(mob);
        self.click(p, console);
        let window = self.wait_window(p, ShuttleConsoleUi::NAME);
        self.run_until("the console offers the dock", |s| {
            s.latest_data::<ShuttleConsoleData>(p, window)
                .locations
                .iter()
                .any(|(id, _)| id == dock)
        });
        self.act(p, window, MOVE, &[(SHUTTLE_ID, dock)]);
        self.run_until("the infiltrator leaves", |s| {
            s.server.world().resource::<Infiltrator>().flight != Flight::Idle
        });
        self.run_until_for(30.0, "the infiltrator docks", |s| {
            s.server.world().resource::<Infiltrator>().flight == Flight::Idle
        });
    }
}

#[test]
fn operatives_nuke_the_station() {
    let mut s = Station::start("nuke");
    let players: Vec<usize> = (0..3).map(|_| s.connect()).collect();
    for &p in &players {
        s.lobby_act(p, CHARACTER_SETUP);
        let prefs = s.wait_window(p, JobPrefsUi::NAME);
        s.act(
            p,
            prefs,
            SET_JOB_PREFERENCE,
            &[("job", "Assistant"), ("level", "3")],
        );
        if p < 2 {
            s.act(
                p,
                prefs,
                SET_ANTAGS,
                &[("antags", ROLE_OPERATIVE), ("toggled", "1")],
            );
        }
    }
    s.run_until("preferences stored", |s| {
        let world = s.server.world();
        (0..2).all(|p| {
            world
                .get::<ss13_game::antag::dynamic::AntagPreferences>(s.players[p].link)
                .is_some_and(|prefs| prefs.wants(ROLE_OPERATIVE))
        }) && (0..3).all(|p| {
            world
                .get::<ss13_game::round::JobPreferences>(s.players[p].link)
                .is_some_and(|prefs| !prefs.levels.is_empty())
        })
    });
    for &p in &players {
        s.lobby_act(p, TOGGLE_READY);
    }
    s.run_until("everyone is ready", |s| {
        (0..3).all(|p| s.received(p).lobby.as_ref().is_some_and(|(_, l)| l.ready))
    });
    s.start_now();
    s.run_until("the round starts", |s| {
        s.server.world().resource::<Ticker>().state() >= GameState::Playing
    });

    // Both opted-in players are operatives aboard the infiltrator, without a station job.
    let infiltrator_z = s.server.world().resource::<Infiltrator>().level_z;
    let team = s.server.world().resource::<NukeTeam>().clone();
    assert_eq!(team.members.len(), 2);
    assert_eq!(team.syndicate_name, "Смит");
    let code = team
        .memorized_code
        .clone()
        .expect("the team knows the code");
    let leader = team.leader.unwrap();
    let leader_p = (0..2).find(|&p| s.mob(p) == leader).unwrap();
    for p in 0..2 {
        let mob = s.mob(p);
        let world = s.server.world();
        assert!(world.get::<Nukeop>(mob).is_some());
        assert!(world.get::<PlayerJob>(mob).is_none());
        assert_eq!(world.get::<TilePos>(mob).unwrap().z, infiltrator_z);
        let inv = world.get::<Inventory>(mob).unwrap();
        let card = world.get::<IdCard>(inv.id.unwrap()).unwrap();
        assert!(card.access.contains(&"syndicate".to_owned()));
        let radio = world
            .get::<ss13_game::radio::Radio>(inv.ears.unwrap())
            .unwrap();
        assert_eq!(radio.frequency, ss13_chat::radio::FREQ_SYNDICATE);
        // The nuclear uplink in the backpack: 25 TC and `CEILING(3 / 5, 5)`.
        let pack = inv.back.expect("a backpack");
        let mut q =
            s.0.server
                .world_mut()
                .query::<(&Uplink, &ss13_game::storage::StoredIn)>();
        let world = s.0.server.world_mut();
        let uplink = q
            .iter(world)
            .find(|(_, inside)| inside.0 == pack)
            .map(|(u, _)| u.telecrystals);
        assert_eq!(uplink, Some(30));
    }
    let crew = s.mob(2);
    assert!(s.server.world().get::<PlayerJob>(crew).is_some());
    s.wait_chat(leader_p, &format!("Код ядерной аутентификации: {code}"));
    s.wait_chat(1 - leader_p, "Вы - агент Смит!");
    // The leader holds the code paper.
    let world = s.server.world();
    let hands = world.get::<Inventory>(leader).unwrap().hands;
    assert!(hands.iter().flatten().any(|&i| {
        world
            .get::<ss13_mapload::DmName>(i)
            .is_some_and(|n| n.desc.contains(&code))
    }));
    let bomb = s.find_obj("/obj/machinery/nuclearbomb/syndicate");
    assert_eq!(
        s.server
            .world()
            .get::<NuclearBomb>(bomb)
            .unwrap()
            .state
            .r_code,
        code
    );

    // To the station.
    s.fly(leader_p, "syndicate_nw");
    let port = s.server.world().resource::<Infiltrator>().port;
    assert_eq!(s.pos(port).z, 1);
    assert_eq!(s.pos(bomb).z, 1);

    // The leader takes the disk (the test hands it over) and puts it into the bomb.
    let disk = {
        let world = s.0.server.world_mut();
        let mut q = world.query_filtered::<Entity, With<NukeDisk>>();
        q.iter(world).next().unwrap()
    };
    {
        let world = s.0.server.world_mut();
        world
            .entity_mut(disk)
            .remove::<TilePos>()
            .insert(CarriedBy(leader));
        let mut inv = world.get_mut::<Inventory>(leader).unwrap();
        let free = (0..2).find(|&h| inv.hands[h].is_none()).unwrap();
        inv.active = free;
        inv.hands[free] = Some(disk);
    }
    let at = s.pos(bomb);
    s.put(leader, at.step(ss13_shared::Dir::West));
    s.click(leader_p, bomb);
    s.run_until("the disk is in the bomb", |s| {
        s.server.world().get::<NuclearBomb>(bomb).unwrap().disk == Some(disk)
    });
    s.click(leader_p, bomb);
    let window = s.wait_window(leader_p, NuclearBombUi::NAME);
    s.run_until("the keypad asks for the code", |s| {
        s.latest_data::<NuclearBombData>(leader_p, window).status1 == "INPUT CODE"
    });
    // A wrong code, then the right one, then 95 seconds.
    s.keys(leader_p, window, "11111E");
    s.run_until("ERROR", |s| {
        s.latest_data::<NuclearBombData>(leader_p, window).status2 == "CODE: ERROR"
    });
    s.keys(leader_p, window, &format!("C{code}E95E"));
    s.run_until("the bomb is ready", |s| {
        s.bomb_mode(bomb) == UiMode::AwaitArm
    });
    s.act(leader_p, window, ARM, &[]);
    s.run_until("the bomb counts down", |s| {
        s.bomb_mode(bomb) == UiMode::Timing
    });
    // Disarm (ARM again with the disk in and the code entered), then arm again.
    s.act(leader_p, window, ARM, &[]);
    s.run_until("disarmed", |s| s.bomb_mode(bomb) == UiMode::AwaitArm);
    s.act(leader_p, window, ARM, &[]);
    s.run_until("armed again", |s| s.bomb_mode(bomb) == UiMode::Timing);
    s.run_until("the window shows the countdown", |s| {
        s.latest_data::<NuclearBombData>(leader_p, window)
            .status1
            .contains("DEVICE ARMED")
    });

    // The bomb stays on the station; the operatives fly home.
    let station_tile = TilePos::new(3, 6, 1);
    s.server.world_mut().entity_mut(bomb).insert(station_tile);
    s.fly(leader_p, "syndicate_away");
    assert!(ss13_game::antag::nukeop::server::infiltrator_home(
        s.server.world()
    ));
    let world = s.server.world();
    let docked = world.get::<MobileDock>(port).unwrap().docked;
    assert_eq!(docked, Some(world.resource::<Infiltrator>().away));

    // Detonation: the crew on the station dies, the round ends, the syndicate won.
    s.run_until_for(120.0, "the station is nuked", |s| {
        s.server.world().resource::<NukeOutcome>().station_was_nuked
    });
    assert_eq!(s.server.world().get::<Stat>(crew), Some(&Stat::Dead));
    assert_ne!(s.server.world().get::<Stat>(leader), Some(&Stat::Dead));
    s.wait_chat(2, "Разгромная победа Синдиката!");
    s.wait_chat(2, "Оперативники Смит уничтожили");
    s.wait_chat(2, "Задача #1: Уничтожьте станцию");
    s.wait_chat(2, "Раунд закончился.");
    assert!(s.server.world().get::<DmItem>(disk).is_some());
}
