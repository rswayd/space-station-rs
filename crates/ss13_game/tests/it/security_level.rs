//! Security levels over real WebTransport on 127.0.0.1 (`ss13_game::security_level`): the
//! communications console sets blue only with a captain's ID swiped in hand and refuses under
//! Gamma; two heads raise red alert on two keycard devices (one head cannot confirm their own
//! request, a player without the access cannot send one); red alert lifts the access of an
//! airlock with `red_alert_access`, turns the fire alarm's light and the evac display's picture
//! (back to the logo after 30 s); emergency maintenance access opens the maintenance airlocks.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::comms::{CommsConsoleData, CommsConsoleUi, SWIPE_NEEDED};
use ss13_game::doors::{Door, DoorState};
use ss13_game::inventory::{IdCard, Inventory};
use ss13_game::keycard::{KeycardAuthData, KeycardAuthUi};
use ss13_game::security_level::keycard::KeycardAuth;
use ss13_game::security_level::{self, SecLevel, StationAccess};
use ss13_game::shuttle::CommsConsole;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_game::wallmount::fire_alarm::FireAlarm;
use ss13_net::chat::client::ChatReceived;
use ss13_net::ui::client::UiMessageReceived;
use ss13_shared::ui::UiServerMessage;
use ss13_shared::{Appearance, Dir, IconStateId, TilePos};
use ss13_tgui::{UiInterface, decode};
use ss13_world::SpatialIndex;

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 }, "declared_vars": { "light_power": 1 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3, "anchored": 0 } },
    "/obj": { "parent": "/atom/movable" },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start" },
    "/obj/effect/mapping_helpers": { "parent": "/obj/effect" },
    "/obj/effect/mapping_helpers/airlock": { "parent": "/obj/effect/mapping_helpers" },
    "/obj/effect/mapping_helpers/airlock/red_alert_access": {
        "parent": "/obj/effect/mapping_helpers/airlock" },
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
    "/obj/machinery/door": { "parent": "/obj/machinery", "declared_vars": {
        "icon": { "resource": "a.dmi" }, "icon_state": "closed", "density": 1, "opacity": 1,
        "layer": 2.7, "base_icon_state": "door", "autoclose": 0, "can_open_with_hands": 1,
        "closingLayer": 3.1, "glass": 0, "locked": 0, "safe": 1, "visible": 1, "welded": 0,
        "power_channel": 3, "idle_power_usage": 10, "active_power_usage": 200, "id_tag": null } },
    "/obj/machinery/door/airlock": { "parent": "/obj/machinery/door", "declared_vars": {
        "name": "airlock", "autoclose": 1, "base_icon_state": null, "normalspeed": 1,
        "overlays_file": { "resource": "o.dmi" }, "airlock_material": null } },
    "/obj/machinery/computer": { "parent": "/obj/machinery", "declared_vars": {
        "icon": { "resource": "c.dmi" }, "icon_state": "computer", "density": 1 } },
    "/obj/machinery/computer/communications": { "parent": "/obj/machinery/computer",
        "declared_vars": { "name": "communications console" } },
    "/obj/machinery/keycard_auth": { "parent": "/obj/machinery", "declared_vars": {
        "name": "keycard authentication device", "icon": { "resource": "k.dmi" },
        "icon_state": "auth_off", "power_channel": 3 } },
    "/obj/machinery/status_display": { "parent": "/obj/machinery", "declared_vars": {
        "name": "status display", "icon": { "resource": "sd.dmi" }, "icon_state": "frame" } },
    "/obj/machinery/status_display/evac": { "parent": "/obj/machinery/status_display" },
    "/obj/machinery/firealarm": { "parent": "/obj/machinery", "declared_vars": {
        "name": "fire alarm", "icon": { "resource": "wm.dmi" }, "icon_state": "fire0",
        "power_channel": 3, "light_range": 1.6, "light_color": "#00FFFF" } },
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
    "/area/station": { "parent": "/area" },
    "/area/station/maintenance": { "parent": "/area/station" }
  }
}"##;

/// The first player spawns at `s` (3,4) (the second next to it), between the keycard devices
/// `K` (2,4) and (4,4), under the console `C` (3,5); the red alert airlock `R` (3,3) needs brig
/// access and leads to (3,2); the fire alarm and the evac display hang at `M` (5,4); the
/// maintenance airlock `A` (6,2) stands in the maintenance area `m`.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/station)
"f" = (/turf/open/floor,/area/station)
"m" = (/turf/open/floor,/area/station/maintenance)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/station)
"K" = (/obj/machinery/keycard_auth,/turf/open/floor,/area/station)
"C" = (/obj/machinery/computer/communications,/turf/open/floor,/area/station)
"M" = (/obj/machinery/firealarm,/obj/machinery/status_display/evac,/turf/open/floor,/area/station)
"R" = (/obj/machinery/door/airlock{req_access = list("brig")},/obj/effect/mapping_helpers/airlock/red_alert_access,/turf/open/floor,/area/station)
"A" = (/obj/machinery/door/airlock{req_access = list("maint_tunnels")},/turf/open/floor,/area/station/maintenance)

(1,1,1) = {"
wwwwwwww
wwCwwwww
wKsKMffw
wwRwwwww
wfffwAmw
wwwwwwww
"}
"##;

const SPAWN: TilePos = TilePos::new(3, 4, 1);
const LEFT_DEVICE: TilePos = TilePos::new(2, 4, 1);
const RIGHT_DEVICE: TilePos = TilePos::new(4, 4, 1);
const CONSOLE: TilePos = TilePos::new(3, 5, 1);
const RED_DOOR: TilePos = TilePos::new(3, 3, 1);
const MOUNTS: TilePos = TilePos::new(5, 4, 1);
const MAINT_DOOR: TilePos = TilePos::new(6, 2, 1);

fn states() -> Vec<(&'static str, &'static str)> {
    let mut states = vec![
        ("t.dmi", "floor"),
        ("t.dmi", "wall"),
        ("l.dmi", "x2"),
        ("h.dmi", "human_basic"),
        ("i.dmi", "toy"),
        ("i.dmi", "card_grey"),
        ("c.dmi", "computer"),
        ("k.dmi", "auth_off"),
        ("k.dmi", "auth_on"),
    ];
    for f in ["closed", "opening", "open", "closing"] {
        states.push(("a.dmi", f));
    }
    for s in [
        "frame",
        "outline",
        "default",
        "greenalert",
        "bluealert",
        "redalert",
        "gammaalert",
        "epsilonalert",
        "deltaalert",
    ] {
        states.push(("sd.dmi", s));
    }
    for s in [
        "fire0",
        "fire_0",
        "fire_1",
        "fire_2",
        "fire_3",
        "fire_4",
        "fire_5",
        "fire_level_e",
        "fire_alerting",
        "fire_alerting_e",
    ] {
        states.push(("wm.dmi", s));
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
        .join("security_level_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/effect/mapping_helpers/airlock/red_alert_access",
        "/obj/machinery/door/airlock",
        "/obj/machinery/computer/communications",
        "/obj/machinery/keycard_auth",
        "/obj/machinery/status_display/evac",
        "/obj/machinery/firealarm",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
        "/area/station",
        "/area/station/maintenance",
    ];
    write_fixture(&dir, TYPES, &used, &states(), &[("map.dmm", MAP)])
}

/// One player's chat and its open windows' latest data.
#[derive(Resource, Default)]
struct Received {
    chat: Vec<String>,
    console: Option<(u32, CommsConsoleData)>,
    keycard: Option<(u32, KeycardAuthData)>,
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
                if interface == CommsConsoleUi::NAME {
                    out.console = Some((*window, decode(data).unwrap()));
                } else if interface == KeycardAuthUi::NAME {
                    out.keycard = Some((*window, decode(data).unwrap()));
                }
            }
            UiServerMessage::Update {
                window,
                data: Some(data),
                ..
            } => {
                if let Some((_, d)) = out.console.as_mut().filter(|(id, _)| id == window) {
                    *d = decode(data).unwrap();
                } else if let Some((_, d)) = out.keycard.as_mut().filter(|(id, _)| id == window) {
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
    fn start(test: &str, players: usize) -> Self {
        let assets = fixture_assets(test);
        let mut s = testing::Station::start(fixture_map(&assets, &["map.dmm"]), |_| {});
        s.join_all(players, |app| {
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

    fn received(&self, p: usize) -> &Received {
        self.players[p].app.world().resource::<Received>()
    }

    fn said(&self, p: usize, text: &str) -> bool {
        self.received(p).chat.iter().any(|line| line.contains(text))
    }

    fn level(&self) -> SecLevel {
        security_level::current(self.server.world())
    }

    fn set_level(&mut self, level: SecLevel) {
        security_level::set_level(self.server.world_mut(), level, true);
    }

    /// `p`'s ID card (in its ID slot after the outfit).
    fn id_card(&self, p: usize) -> Entity {
        let mob = self.players[p].mob;
        let world = self.server.world();
        world
            .get::<Inventory>(mob)
            .unwrap()
            .id_card(world)
            .expect("an ID card")
    }

    fn set_access(&mut self, p: usize, access: &[&str]) {
        let card = self.id_card(p);
        self.server.world_mut().entity_mut(card).insert(IdCard {
            access: access.iter().map(|a| (*a).to_owned()).collect(),
        });
    }

    /// Moves `p`'s ID from its slot to its active hand (the swipe).
    fn id_to_hand(&mut self, p: usize) {
        let mob = self.players[p].mob;
        let card = self.id_card(p);
        let world = self.server.world_mut();
        let mut inventory = world.get_mut::<Inventory>(mob).unwrap();
        inventory.id = None;
        let active = inventory.active;
        inventory.hands[active] = Some(card);
    }

    fn open(&mut self, p: usize, target: Entity, opened: impl Fn(&Received) -> bool) {
        self.run_until("the target is replicated", |s| {
            s.net_id(p, target).is_some()
        });
        self.0.click(p, target);
        self.run_until("the window opens", |s| opened(s.received(p)));
    }

    fn console_act(&mut self, p: usize, action: &str, params: &[(&str, &str)]) {
        let window = self.received(p).console.as_ref().expect("console window").0;
        self.0.act(p, window, action, params);
    }

    fn keycard_act(&mut self, p: usize, action: &str) {
        let window = self.received(p).keycard.as_ref().expect("keycard window").0;
        self.0.act(p, window, action, &[]);
    }

    fn overlays(&self, e: Entity) -> Vec<IconStateId> {
        self.get::<Appearance>(e)
            .overlays
            .iter()
            .map(|o| o.icon_state)
            .collect()
    }
}

#[test]
fn console_sets_blue_with_a_swiped_captain_id() {
    let mut s = Station::start("console", 1);
    s.set_access(0, &["command", "captain"]);
    let console = s.find::<CommsConsole>(CONSOLE);
    s.open(0, console, |r| r.console.is_some());
    s.console_act(0, "toggleAuthentication", &[]);
    s.run_until("logged in as captain", |s| {
        s.received(0).console.as_ref().is_some_and(|(_, d)| {
            d.authenticated && d.can_set_alert_level.as_deref() == Some(SWIPE_NEEDED)
        })
    });
    assert_eq!(
        s.received(0).console.as_ref().unwrap().1.alert_level,
        "green"
    );

    // The ID stays in its slot: no swipe.
    s.console_act(0, "changeSecurityLevel", &[("newSecurityLevel", "blue")]);
    s.run_until("asked to swipe", |s| {
        s.said(0, "You need to swipe your ID!")
    });
    assert_eq!(s.level(), SecLevel::Green);

    s.id_to_hand(0);
    // Only green and blue are set here.
    s.console_act(0, "changeSecurityLevel", &[("newSecurityLevel", "red")]);
    s.hold(1.0, "red is not set from the console", |s| {
        s.level() == SecLevel::Green
    });
    s.console_act(0, "changeSecurityLevel", &[("newSecurityLevel", "blue")]);
    s.run_until("blue alert", |s| s.level() == SecLevel::Blue);
    s.run_until("announced and confirmed", |s| {
        s.said(0, "Внимание! Код повышен до: синий")
            && s.said(0, "Authorization confirmed. Modifying security level.")
            && s.received(0).console.as_ref().unwrap().1.alert_level == "blue"
    });

    // A captain without captain access on the swiped card is refused.
    s.set_access(0, &["command"]);
    s.console_act(0, "changeSecurityLevel", &[("newSecurityLevel", "green")]);
    s.run_until("refused", |s| {
        s.said(0, "You are not authorized to do this!")
    });
    assert_eq!(s.level(), SecLevel::Blue);

    // CentCom's martial law locks the console.
    s.set_access(0, &["command", "captain"]);
    s.set_level(SecLevel::Gamma);
    s.console_act(0, "changeSecurityLevel", &[("newSecurityLevel", "green")]);
    s.run_until("locked", |s| {
        s.said(0, "Центральным командованием установлено военное положение")
    });
    assert_eq!(s.level(), SecLevel::Gamma);
}

#[test]
fn two_heads_raise_red_alert_with_keycards() {
    let mut s = Station::start("keycards", 2);
    let left = s.find::<KeycardAuth>(LEFT_DEVICE);
    let right = s.find::<KeycardAuth>(RIGHT_DEVICE);
    s.open(0, left, |r| r.keycard.is_some());
    s.open(1, right, |r| r.keycard.is_some());
    let left_window = s.received(0).keycard.as_ref().unwrap().0;

    // Without `keycard_auth` nothing is sent.
    s.keycard_act(0, "red_alert");
    s.hold(1.0, "no access, no request", |s| {
        s.get::<KeycardAuth>(right).asked_by.is_none()
    });

    s.set_access(0, &["keycard_auth"]);
    s.set_access(1, &["keycard_auth"]);
    // One head cannot confirm their own request on the other device.
    s.keycard_act(0, "red_alert");
    s.run_until("the right device asks", |s| {
        s.get::<KeycardAuth>(right).asked_by.is_some()
    });
    let window = s.received(0).keycard.as_ref().unwrap().0;
    s.0.click(0, right);
    s.run_until("p0 sees the right device", |s| {
        s.received(0)
            .keycard
            .as_ref()
            .is_some_and(|(id, d)| *id != window && d.auth_required.as_deref() == Some("Red Alert"))
    });
    s.keycard_act(0, "auth_swipe");
    s.hold(3.0, "a request confirmed by its sender does nothing", |s| {
        s.level() == SecLevel::Green
    });

    // Two heads: p0 asks on the left device (its first window, still open), p1 confirms on
    // the right one.
    s.0.act(0, left_window, "red_alert", &[]);
    s.run_until("p1 is asked", |s| {
        s.received(1)
            .keycard
            .as_ref()
            .is_some_and(|(_, d)| d.auth_required.as_deref() == Some("Red Alert"))
    });
    s.keycard_act(1, "auth_swipe");
    s.run_until("red alert", |s| s.level() == SecLevel::Red);
    s.run_until("announced", |s| {
        s.said(1, "Внимание! Код повышен до: красный")
    });

    // The consumers follow: the airlock lifts its access, the fire alarm and the evac display
    // show red, the display goes back to the logo after 30 s.
    let door = s.find::<Door>(RED_DOOR);
    assert!(s.get::<Door>(door).red_alert);
    let alarm = s.find::<FireAlarm>(MOUNTS);
    assert!(s.overlays(alarm).contains(&state_id("wm.dmi", "fire_2")));
    let display = s.find_display();
    assert!(
        s.overlays(display)
            .contains(&state_id("sd.dmi", "redalert"))
    );
    s.run_for(30.0);
    s.run_until("the logo is back", |s| {
        s.overlays(display).contains(&state_id("sd.dmi", "default"))
    });
}

impl Station {
    fn find_display(&self) -> Entity {
        let world = self.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(MOUNTS)
            .iter()
            .copied()
            .find(|&e| {
                world
                    .get::<security_level::displays::EvacDisplay>(e)
                    .is_some()
            })
            .expect("an evac display")
    }
}

#[test]
fn red_alert_lifts_airlock_access() {
    let mut s = Station::start("red_door", 1);
    let door = s.find::<Door>(RED_DOOR);
    assert!(s.get::<Door>(door).red_alert_access);
    s.run_for(1.0);
    for _ in 0..3 {
        s.0.try_step(0, Dir::South);
        s.run_for(0.3);
    }
    s.hold(1.0, "no brig access: the airlock stays shut", |s| {
        s.get::<Door>(door).state == DoorState::Closed
    });
    s.set_level(SecLevel::Red);
    s.run_until("the airlock opens for anyone", |s| {
        if s.get::<Door>(door).state != DoorState::Closed {
            return true;
        }
        s.0.try_step(0, Dir::South);
        false
    });
    // Back to blue: the access is required again.
    s.set_level(SecLevel::Blue);
    assert!(!s.get::<Door>(door).red_alert);
}

#[test]
fn keycard_emergency_maintenance_access() {
    let mut s = Station::start("maint", 2);
    s.set_access(0, &["keycard_auth"]);
    s.set_access(1, &["keycard_auth"]);
    let left = s.find::<KeycardAuth>(LEFT_DEVICE);
    let right = s.find::<KeycardAuth>(RIGHT_DEVICE);
    s.open(0, left, |r| r.keycard.is_some());
    s.open(1, right, |r| r.keycard.is_some());
    s.keycard_act(0, "emergency_maint");
    s.run_until("p1 is asked", |s| {
        s.received(1).keycard.as_ref().is_some_and(|(_, d)| {
            d.auth_required.as_deref() == Some("Emergency Maintenance Access")
        })
    });
    s.keycard_act(1, "auth_swipe");
    let maint = s.find::<Door>(MAINT_DOOR);
    s.run_until("maintenance is open to all", |s| {
        s.get::<Door>(maint).emergency
    });
    assert!(
        s.server
            .world()
            .resource::<StationAccess>()
            .emergency_access
    );
    assert!(!s.get::<Door>(s.find::<Door>(RED_DOOR)).emergency);
    s.run_until("announced", |s| {
        s.said(
            0,
            "Ограничения на доступ к техническим и внешним шлюзам были сняты.",
        )
    });
    s.run_until("the window shows it", |s| {
        s.received(1)
            .keycard
            .as_ref()
            .is_some_and(|(_, d)| d.emergency_maint)
    });
    // The level is untouched.
    assert_eq!(s.level(), SecLevel::Green);
    // The revocation (the communications console's, not ported there yet).
    security_level::revoke_maint_all_access(s.server.world_mut());
    assert!(!s.get::<Door>(maint).emergency);
}
