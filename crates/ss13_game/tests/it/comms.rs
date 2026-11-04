//! The communications console and announcements over real WebTransport on 127.0.0.1
//! (`ss13_game::comms_server`, `ss13_game::announce`): a head without captain access sees no
//! captain functions; the captain makes a priority announcement through the text prompt (30 s
//! cooldown), messages CentCom (60 s cooldown, a request for the admins), sets the evac
//! display's text and pictures, spends the free emergency access toggles, emags the console
//! (messages go to the Syndicate) and restores it, answers and deletes a CentCom message; the
//! round-start report is printed on the console and raises the blue alert.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::announce::{self, CommsController};
use ss13_game::comms::{CommsConsoleData, CommsConsoleUi, CommsMessage};
use ss13_game::comms_server::{CommsConsole, CommsRequest, RequestKind};
use ss13_game::inventory::{CarriedBy, IdCard, Inventory};
use ss13_game::security_level::displays::{DisplayState, EvacDisplay};
use ss13_game::security_level::{self, SecLevel, StationAccess};
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::DmName;
use ss13_net::chat::client::ChatReceived;
use ss13_net::ui::client::UiMessageReceived;
use ss13_shared::ui::UiServerMessage;
use ss13_shared::{Appearance, IconStateId, TilePos};
use ss13_tgui::interfaces::text_input::TextInputUi;
use ss13_tgui::{UiInterface, decode};
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
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start" },
    "/obj/item": { "parent": "/obj", "declared_vars": { "icon": { "resource": "i.dmi" },
        "icon_state": "toy", "interaction_flags_item": 1, "slot_flags": 0 } },
    "/obj/item/paper": { "parent": "/obj/item", "declared_vars": { "name": "paper" } },
    "/obj/item/card": { "parent": "/obj/item" },
    "/obj/item/card/emag": { "parent": "/obj/item/card", "declared_vars": {
        "name": "cryptographic sequencer" } },
    "/obj/item/card/id": { "parent": "/obj/item/card", "declared_vars": {
        "access": { "list": [] }, "icon_state": "card_grey", "slot_flags": 256 } },
    "/obj/item/card/id/advanced": { "parent": "/obj/item/card/id" },
    "/obj/item/clothing": { "parent": "/obj/item" },
    "/obj/item/clothing/under": { "parent": "/obj/item/clothing" },
    "/obj/item/clothing/under/color": { "parent": "/obj/item/clothing/under" },
    "/obj/item/clothing/under/color/grey": { "parent": "/obj/item/clothing/under/color" },
    "/obj/item/clothing/shoes": { "parent": "/obj/item/clothing" },
    "/obj/item/clothing/shoes/sneakers": { "parent": "/obj/item/clothing/shoes" },
    "/obj/item/clothing/shoes/sneakers/black": { "parent": "/obj/item/clothing/shoes/sneakers" },
    "/obj/machinery": { "parent": "/obj", "declared_vars": { "anchored": 1,
        "use_power": 1, "idle_power_usage": 100, "active_power_usage": 1000, "power_channel": 1,
        "req_access": null, "req_one_access": null } },
    "/obj/machinery/computer": { "parent": "/obj/machinery", "declared_vars": {
        "icon": { "resource": "c.dmi" }, "icon_state": "computer", "density": 1 } },
    "/obj/machinery/computer/communications": { "parent": "/obj/machinery/computer",
        "declared_vars": { "name": "communications console" } },
    "/obj/machinery/status_display": { "parent": "/obj/machinery", "declared_vars": {
        "name": "status display", "icon": { "resource": "sd.dmi" }, "icon_state": "frame" } },
    "/obj/machinery/status_display/evac": { "parent": "/obj/machinery/status_display" },
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
    "/area": { "parent": "/atom", "declared_vars": { "requires_power": 0, "always_unpowered": 0,
        "power_equip": 1, "power_light": 1, "power_environ": 1, "lightswitch": 1,
        "static_lighting": 1 } },
    "/area/space": { "parent": "/area" },
    "/area/station": { "parent": "/area" }
  }
}"##;

/// The player spawns at `s` (3,2) under the console `C` (3,3), next to the evac display `D`
/// (4,2).
const MAP: &str = r##""w" = (/turf/closed/wall,/area/station)
"f" = (/turf/open/floor,/area/station)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/station)
"C" = (/obj/machinery/computer/communications,/turf/open/floor,/area/station)
"D" = (/obj/machinery/status_display/evac,/turf/open/floor,/area/station)

(1,1,1) = {"
wwwww
wwCww
wfsDw
wwwww
"}
"##;

const SPAWN: TilePos = TilePos::new(3, 2, 1);
const CONSOLE: TilePos = TilePos::new(3, 3, 1);
const DISPLAY: TilePos = TilePos::new(4, 2, 1);

fn states() -> Vec<(&'static str, &'static str)> {
    let mut states = vec![
        ("t.dmi", "floor"),
        ("t.dmi", "wall"),
        ("l.dmi", "x2"),
        ("h.dmi", "human_basic"),
        ("i.dmi", "toy"),
        ("i.dmi", "card_grey"),
        ("c.dmi", "computer"),
    ];
    for s in [
        "frame",
        "outline",
        "default",
        "biohazard",
        "greenalert",
        "bluealert",
    ] {
        states.push(("sd.dmi", s));
    }
    states
}

fn state_id(dmi: &str, state: &str) -> IconStateId {
    let i = states()
        .iter()
        .position(|&(d, s)| d == dmi && s == state)
        .unwrap_or_else(|| panic!("no state {dmi} {state}"));
    IconStateId(i as u32)
}

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("comms_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/machinery/computer/communications",
        "/obj/machinery/status_display/evac",
        "/obj/item/paper",
        "/obj/item/card/emag",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
        "/area/station",
    ];
    write_fixture(&dir, TYPES, &used, &states(), &[("map.dmm", MAP)])
}

/// The client's chat, its console window's latest data and every window it was sent.
#[derive(Resource, Default)]
struct Received {
    chat: Vec<String>,
    console: Option<(u32, CommsConsoleData)>,
    windows: Vec<(String, u32)>,
}

fn record(
    mut chat: MessageReader<ChatReceived>,
    mut ui: MessageReader<UiMessageReceived>,
    mut out: ResMut<Received>,
) {
    for ChatReceived(message) in chat.read() {
        out.chat.push(message.plain_text());
    }
    for UiMessageReceived(message) in ui.read() {
        match message {
            UiServerMessage::Open {
                window,
                interface,
                data,
                ..
            } => {
                out.windows.push((interface.clone(), *window));
                if interface == CommsConsoleUi::NAME {
                    out.console = Some((*window, decode(data).unwrap()));
                }
            }
            UiServerMessage::Update {
                window,
                data: Some(data),
                ..
            } => {
                if let Some((_, d)) = out.console.as_mut().filter(|(id, _)| id == window) {
                    *d = decode(data).unwrap();
                }
            }
            _ => {}
        }
    }
}

/// The server's [`CommsRequest`]s.
#[derive(Resource, Default)]
struct Requests(Vec<CommsRequest>);

fn record_requests(mut requests: MessageReader<CommsRequest>, mut out: ResMut<Requests>) {
    out.0.extend(requests.read().cloned());
}

struct Station(testing::Station);

ss13_game::drive_station!(Station, 0);

impl Station {
    fn start(test: &str) -> Self {
        let assets = fixture_assets(test);
        let mut s = testing::Station::start(fixture_map(&assets, &["map.dmm"]), |app| {
            app.init_resource::<Requests>()
                .add_systems(Update, record_requests);
        });
        s.join_all(1, |app| {
            app.init_resource::<Received>().add_systems(Update, record);
        });
        let s = Self(s);
        assert_eq!(*s.get::<TilePos>(s.players[0].mob), SPAWN);
        s
    }

    fn find<T: Component>(&self, pos: TilePos) -> Entity {
        let world = self.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .find(|&e| world.get::<T>(e).is_some())
            .unwrap_or_else(|| panic!("nothing with {} on {pos:?}", std::any::type_name::<T>()))
    }

    fn received(&self) -> &Received {
        self.players[0].app.world().resource::<Received>()
    }

    fn data(&self) -> &CommsConsoleData {
        &self.received().console.as_ref().expect("console window").1
    }

    fn said(&self, text: &str) -> bool {
        self.received().chat.iter().any(|line| line.contains(text))
    }

    fn said_count(&self, text: &str) -> usize {
        self.received()
            .chat
            .iter()
            .filter(|line| line.contains(text))
            .count()
    }

    fn requests(&self) -> &[CommsRequest] {
        &self.server.world().resource::<Requests>().0
    }

    fn set_access(&mut self, access: &[&str]) {
        let mob = self.players[0].mob;
        let world = self.server.world_mut();
        let card = world.get::<Inventory>(mob).unwrap().id_card(world).unwrap();
        world.entity_mut(card).insert(IdCard {
            access: access.iter().map(|a| (*a).to_owned()).collect(),
        });
    }

    fn act(&mut self, action: &str, params: &[(&str, &str)]) {
        let window = self.received().console.as_ref().expect("console window").0;
        self.0.act(0, window, action, params);
    }

    /// A new `path` item in the empty active hand.
    fn hold_item(&mut self, path: &str) -> Entity {
        let mob = self.players[0].mob;
        let world = self.server.world_mut();
        let item = ss13_game::construction::spawn_obj(world, path, SPAWN).unwrap();
        let mut inventory = world.get::<Inventory>(mob).unwrap().clone();
        inventory.hands[inventory.active] = Some(item);
        world
            .entity_mut(item)
            .remove::<TilePos>()
            .insert(CarriedBy(mob));
        world.entity_mut(mob).insert(inventory);
        item
    }

    /// Opens the console and logs in.
    fn log_in(&mut self, console: Entity) {
        self.run_until("the console is replicated", |s| {
            s.net_id(0, console).is_some()
        });
        self.0.click(0, console);
        self.run_until("the window opens", |s| s.received().console.is_some());
        self.act("toggleAuthentication", &[]);
        self.run_until("logged in", |s| s.data().authenticated);
    }

    fn display(&self) -> &EvacDisplay {
        self.get::<EvacDisplay>(self.find::<EvacDisplay>(DISPLAY))
    }

    fn display_overlays(&self) -> Vec<IconStateId> {
        let display = self.find::<EvacDisplay>(DISPLAY);
        self.get::<Appearance>(display)
            .overlays
            .iter()
            .map(|o| o.icon_state)
            .collect()
    }

    fn reset_important_cooldown(&mut self) {
        self.server
            .world_mut()
            .resource_mut::<CommsController>()
            .important_action_until = 0.0;
    }
}

#[test]
fn captain_functions_status_displays_emag_and_messages() {
    let mut s = Station::start("captain");
    let console = s.find::<CommsConsole>(CONSOLE);

    // A head without captain access: no captain functions, the acts do nothing.
    s.set_access(&["command"]);
    s.log_in(console);
    let d = s.data();
    assert!(!d.can_make_announcement && !d.can_message_associates && !d.can_request_ert);
    s.act("makePriorityAnnouncement", &[]);
    s.act("requestNukeCodes", &[("reason", "просто так")]);
    s.hold(1.0, "no prompt, no request", |s| {
        s.requests().is_empty()
            && !s
                .received()
                .windows
                .iter()
                .any(|(i, _)| i == TextInputUi::NAME)
    });
    s.act("toggleAuthentication", &[]);
    s.run_until("logged out", |s| !s.data().authenticated);

    s.set_access(&["command", "captain"]);
    s.act("toggleAuthentication", &[]);
    s.run_until("logged in as captain", |s| {
        s.data().can_make_announcement && s.data().can_request_ert
    });

    // The priority announcement: a text prompt, then the captain's header to everyone.
    s.act("makePriorityAnnouncement", &[]);
    s.run_until("the prompt opens", |s| {
        s.received()
            .windows
            .iter()
            .any(|(i, _)| i == TextInputUi::NAME)
    });
    let prompt = s
        .received()
        .windows
        .iter()
        .find(|(i, _)| i == TextInputUi::NAME)
        .unwrap()
        .1;
    s.0.act(
        0,
        prompt,
        "submit",
        &[("entry", "Всем собраться на мостике")],
    );
    s.run_until("announced", |s| {
        s.said("Оповещение от капитана\nВсем собраться на мостике")
    });
    // 30 s cooldown.
    s.act("makePriorityAnnouncement", &[]);
    s.run_until("recharging", |s| {
        s.said("Intercomms recharging. Please stand by.")
    });

    // CentCom: a request for the admins, then the 60 s cooldown.
    s.act("messageAssociates", &[("message", "  Нужна помощь  ")]);
    s.run_until("sent", |s| {
        s.said("Message transmitted to Central Command.")
    });
    s.run_until("the window shows the cooldown", |s| {
        !s.data().important_action_ready
    });
    s.run_until("the admins' request", |s| s.requests().len() == 1);
    assert_eq!(
        (s.requests()[0].kind, s.requests()[0].text.as_str()),
        (RequestKind::Centcom, "Нужна помощь")
    );
    s.act("requestERT", &[("reason", "Абордаж")]);
    s.hold(1.0, "cooling down", |s| s.requests().len() == 1);
    s.reset_important_cooldown();
    s.act("requestERT", &[("reason", "Абордаж")]);
    s.run_until("ERT requested and announced", |s| {
        s.requests().len() == 2 && s.said("Отправлен запрос ОБР.")
    });
    assert_eq!(s.requests()[1].kind, RequestKind::Ert);

    // Status displays: two lines (a bad one is dropped), a picture, the current alert, back to
    // the logo.
    s.act("setState", &[("state", "changing_status")]);
    s.run_until("the status page", |s| s.data().page == "changing_status");
    s.act(
        "setStatusMessage",
        &[("upperText", "ЭВАКУАЦИЯ"), ("lowerText", "<b>")],
    );
    s.run_until("the message is shown", |s| {
        s.display().showing == DisplayState::Message("ЭВАКУАЦИЯ".into(), String::new())
    });
    assert!(s.display_overlays().is_empty());
    s.run_until("the window remembers the lines", |s| {
        s.data().upper_text == "ЭВАКУАЦИЯ"
    });
    s.act("setStatusPicture", &[("picture", "biohazard")]);
    s.run_until("biohazard", |s| {
        s.display_overlays()
            .contains(&state_id("sd.dmi", "biohazard"))
    });
    s.act("setStatusPicture", &[("picture", "currentalert")]);
    s.run_until("the current alert", |s| {
        s.display().showing == DisplayState::Picture("greenalert".into())
    });
    // A level change shows for 30 s, then the manual picture comes back.
    security_level::set_level(s.server.world_mut(), SecLevel::Blue, false);
    assert!(
        s.display_overlays()
            .contains(&state_id("sd.dmi", "bluealert"))
    );
    s.run_for(31.0);
    s.run_until("back to the manual picture", |s| {
        s.display_overlays()
            .contains(&state_id("sd.dmi", "greenalert"))
    });
    s.act("setStatusPicture", &[("picture", "default")]);
    s.run_until("the logo", |s| {
        s.display().showing == DisplayState::Logo && s.display().manual.is_none()
    });
    s.act("setStatusPicture", &[("picture", "nyan")]);
    s.hold(0.5, "unknown pictures are refused", |s| {
        s.display().showing == DisplayState::Logo
    });
    s.act("setState", &[("state", "main")]);

    // Emergency access: three free toggles, the fourth starts the cooldown, the fifth waits.
    let access = |s: &Station| {
        s.server
            .world()
            .resource::<StationAccess>()
            .emergency_access
    };
    for (i, on) in [true, false, true, false].into_iter().enumerate() {
        s.act("toggleEmergencyAccess", &[]);
        s.run_until(&format!("toggle {i}"), |s| access(s) == on);
    }
    s.run_until("the last free use", |s| {
        s.said("This was your last free use without cooldown")
    });
    s.act("toggleEmergencyAccess", &[]);
    s.run_until("cooling down", |s| {
        s.said("Emergency Access is still in cooldown for")
    });
    assert!(!access(&s));

    // An emag scrambles the routing: messages go to the Syndicate until restored.
    s.hold_item("/obj/item/card/emag");
    s.0.click(0, console);
    s.run_until("emagged", |s| s.data().emagged);
    s.reset_important_cooldown();
    s.act("messageAssociates", &[("message", "Привет")]);
    s.run_until("sent to the Syndicate", |s| {
        s.said("MESSAGE TRANSMITTED TO SYNDICATE COMMAND")
    });
    // The server records its requests a tick after the chat went out.
    s.run_until("the Syndicate request", |s| s.requests().len() == 3);
    assert_eq!(s.requests()[2].kind, RequestKind::Syndicate);
    s.act("restoreBackupRoutingData", &[]);
    s.run_until("restored", |s| !s.data().emagged);

    // A message from CentCom with answers: one answer only, then deleted.
    let mut state = s.get::<CommsConsole>(console).clone();
    state.messages.push(CommsMessage {
        title: "Запрос".into(),
        content: "Да или нет?".into(),
        possible_answers: vec!["Да".into(), "Нет".into()],
        answered: 0,
    });
    s.server.world_mut().entity_mut(console).insert(state);
    s.act("answerMessage", &[("message", "1"), ("answer", "2")]);
    s.run_until("answered", |s| {
        s.data().messages.first().is_some_and(|m| m.answered == 2)
    });
    s.act("answerMessage", &[("message", "1"), ("answer", "1")]);
    s.act("answerMessage", &[("message", "1"), ("answer", "3")]);
    s.hold(0.5, "the answer stays", |s| {
        s.data().messages[0].answered == 2
    });
    s.act("deleteMessage", &[("message", "1")]);
    s.run_until("deleted", |s| s.data().messages.is_empty());
}

#[test]
fn roundstart_report_is_printed_and_raises_blue() {
    let mut s = Station::start("report");
    let console = s.find::<CommsConsole>(CONSOLE);
    announce::queue_roundstart_report(s.server.world_mut());
    s.run_until("blue alert", |s| {
        security_level::current(s.server.world()) == SecLevel::Blue
    });
    s.run_until("announced once", |s| {
        s.said_count("Уровень угрозы повышен.") == 1
    });
    // Set quietly: no level announcement of its own.
    assert!(!s.said("Внимание! Код повышен до"));
    let title = "Центральное Командование. Отчет об обстановке";
    assert_eq!(s.get::<CommsConsole>(console).messages[0].title, title);
    let world = s.server.world();
    let printed = world
        .resource::<SpatialIndex>()
        .on_tile(CONSOLE)
        .iter()
        .filter_map(|&e| world.get::<DmName>(e))
        .any(|n| n.name == format!("paper - '{title}'") && n.desc.contains("NT-INTEL-01"));
    assert!(printed, "the report is printed on the console's tile");
}
