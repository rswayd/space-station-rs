//! Requests consoles over real WebTransport on 127.0.0.1 (`ss13_game::wallmount::requests`):
//! the map names them by department and the assistance helper lists Medbay; the player opens
//! the Genetics console, an extreme message without the hack and a message to an unknown
//! department are refused, a high priority request reaches Medbay (unread priority, the console
//! says who sent it), and an emergency call is remembered.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::requests_console::*;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_game::wallmount::requests::RequestsConsole;
use ss13_mapload::DmName;
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
    "/obj/effect/mapping_helpers": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/mapping_helpers/requests_console": {
        "parent": "/obj/effect/mapping_helpers" },
    "/obj/effect/mapping_helpers/requests_console/assistance": {
        "parent": "/obj/effect/mapping_helpers/requests_console" },
    "/obj/item": { "parent": "/obj", "declared_vars": { "icon": { "resource": "i.dmi" },
        "icon_state": "toy", "interaction_flags_item": 1, "slot_flags": 0 } },
    "/obj/item/card": { "parent": "/obj/item" },
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
    "/obj/machinery/requests_console": { "parent": "/obj/machinery", "declared_vars": {
        "name": "requests console", "icon": { "resource": "rc.dmi" },
        "icon_state": "req_comp_off", "department": "", "auto_name": 0 } },
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

/// The player spawns at `s` (3,2) next to the Genetics console `G` (2,2); the Medbay console
/// `M` (5,2) has the assistance helper.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/station)
"f" = (/turf/open/floor,/area/station)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/station)
"G" = (/obj/machinery/requests_console{department = "Genetics"; name = "Genetics Requests Console"},/turf/open/floor,/area/station)
"M" = (/obj/machinery/requests_console{department = "Medbay"},/obj/effect/mapping_helpers/requests_console/assistance,/turf/open/floor,/area/station)

(1,1,1) = {"
wwwwwww
wGsfMfw
wwwwwww
"}
"##;

const GENETICS: TilePos = TilePos::new(2, 2, 1);
const MEDBAY: TilePos = TilePos::new(5, 2, 1);

fn states() -> Vec<(&'static str, &'static str)> {
    vec![
        ("t.dmi", "floor"),
        ("t.dmi", "wall"),
        ("l.dmi", "x2"),
        ("h.dmi", "human_basic"),
        ("i.dmi", "toy"),
        ("i.dmi", "card_grey"),
        ("rc.dmi", "req_comp_off"),
        ("rc.dmi", "req_comp0"),
        ("rc.dmi", "req_comp2"),
        ("rc.dmi", "req_comp3"),
    ]
}

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("requests_console_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/effect/mapping_helpers/requests_console/assistance",
        "/obj/machinery/requests_console",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
        "/area/station",
    ];
    write_fixture(&dir, TYPES, &used, &states(), &[("map.dmm", MAP)])
}

/// The client's chat and its console window (id, static data, latest data).
#[derive(Resource, Default)]
struct Received {
    chat: Vec<String>,
    console: Option<(u32, RequestsStatic, RequestsData)>,
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
                static_data,
                data,
                ..
            } if interface == RequestsConsoleUi::NAME => {
                out.console = Some((*window, decode(static_data).unwrap(), decode(data).unwrap()));
            }
            UiServerMessage::Update {
                window,
                data: Some(data),
                ..
            } => {
                if let Some((_, _, d)) = out.console.as_mut().filter(|(id, ..)| id == window) {
                    *d = decode(data).unwrap();
                }
            }
            _ => {}
        }
    }
}

struct Station(testing::Station);

ss13_game::drive_station!(Station, 0);

impl Station {
    fn start(test: &str) -> Self {
        let assets = fixture_assets(test);
        let mut s = testing::Station::start(fixture_map(&assets, &["map.dmm"]), |_| {});
        s.join_all(1, |app| {
            app.init_resource::<Received>().add_systems(Update, record);
        });
        Self(s)
    }

    fn find(&self, pos: TilePos) -> Entity {
        let world = self.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .find(|&e| world.get::<RequestsConsole>(e).is_some())
            .unwrap_or_else(|| panic!("no requests console on {pos:?}"))
    }

    fn console(&self, pos: TilePos) -> &RequestsConsole {
        self.get::<RequestsConsole>(self.find(pos))
    }

    fn received(&self) -> &Received {
        self.players[0].app.world().resource::<Received>()
    }

    fn said(&self, text: &str) -> bool {
        self.received().chat.iter().any(|line| line.contains(text))
    }

    fn act(&mut self, action: &str, params: &[(&str, &str)]) {
        let window = self.received().console.as_ref().expect("console window").0;
        self.0.act(0, window, action, params);
    }
}

#[test]
fn departments_messages_and_emergencies() {
    let mut s = Station::start("messages");
    let genetics = s.find(GENETICS);
    assert_eq!(s.console(GENETICS).department, "Genetics");
    assert_eq!(s.console(MEDBAY).department, "Medbay");
    assert_eq!(s.get::<DmName>(genetics).name, "Genetics requests console");
    assert_eq!(
        s.console(GENETICS).targets.assistance_consoles,
        ["Medbay".to_owned()]
    );
    assert!(s.console(MEDBAY).targets.assistance_consoles.is_empty());

    s.run_until("the console is replicated", |s| {
        s.net_id(0, genetics).is_some()
    });
    s.0.click(0, genetics);
    s.run_until("the window opens", |s| s.received().console.is_some());
    let (_, statics, data) = s.received().console.clone().unwrap();
    assert_eq!(statics.assistance_consoles, ["Medbay".to_owned()]);
    assert_eq!(data.department, "Genetics");

    // Extreme priority needs the hack; the recipient must be a known department.
    let send = |priority: &'static str, recipient: &'static str| {
        [
            (RECIPIENT, recipient),
            (PRIORITY, priority),
            (REQUEST_TYPE, ASSISTANCE_REQUEST),
            (MESSAGE, "Нужен врач"),
        ]
    };
    s.act(SEND_MESSAGE, &send("3", "Medbay"));
    s.act(SEND_MESSAGE, &send("1", "Nowhere"));
    s.hold(1.0, "refused", |s| s.console(MEDBAY).messages.is_empty());

    s.act(SEND_MESSAGE, &send("2", "Medbay"));
    s.run_until("Medbay gets it", |s| s.console(MEDBAY).messages.len() == 1);
    let medbay = s.console(MEDBAY);
    assert_eq!(medbay.new_message_priority, PRIORITY_HIGH);
    let message = &medbay.messages[0];
    assert_eq!(
        (
            message.sender.as_str(),
            message.content.as_str(),
            message.priority
        ),
        ("Genetics", "Нужен врач", PRIORITY_HIGH)
    );
    assert_eq!(message.request_type, ASSISTANCE_REQUEST);
    s.run_until("Medbay says who sent it", |s| {
        s.said("Message from Genetics")
    });
    assert!(!s.console(GENETICS).has_mail_send_error);

    s.act(SET_EMERGENCY, &[(EMERGENCY, EMERGENCY_SECURITY)]);
    s.run_until("the emergency is on", |s| {
        s.console(GENETICS)
            .emergency
            .as_ref()
            .is_some_and(|e| e.0 == EMERGENCY_SECURITY)
    });
    s.run_until("the window shows it", |s| {
        s.received()
            .console
            .as_ref()
            .is_some_and(|c| c.2.emergency == EMERGENCY_SECURITY)
    });
}
