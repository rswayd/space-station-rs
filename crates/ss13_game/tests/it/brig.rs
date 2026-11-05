//! Brig cell timers over real WebTransport on 127.0.0.1 (`ss13_game::brig::timer`): an
//! assistant without security access opens the timer but cannot start it; with access, a preset
//! sets 5 minutes, stopping opens the cell's security airlock, a 2 second timer closes it and
//! opens it again when it runs out. The cell's brig locker locks with every start and unlocks
//! with every end.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::brig::timer::DoorTimer;
use ss13_game::brig::{BrigTimerData, BrigTimerUi, PRESET, PRESET_SHORT, START, STOP, TIME};
use ss13_game::closets::Closet;
use ss13_game::doors::Door;
use ss13_game::inventory::{IdCard, Inventory};
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_net::ui::client::UiMessageReceived;
use ss13_shared::TilePos;
use ss13_shared::ui::UiServerMessage;
use ss13_tgui::UiInterface;
use ss13_world::{Density, SpatialIndex};

/// tg vars of the types (`brigdoors.dm`, `airlock.dm`).
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
    "/obj/item": { "parent": "/obj", "declared_vars": { "icon": { "resource": "i.dmi" },
        "icon_state": "toy", "interaction_flags_item": 1 } },
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
    "/obj/machinery": { "parent": "/obj", "declared_vars": { "anchored": 1,
        "use_power": 1, "idle_power_usage": 100, "active_power_usage": 1000, "power_channel": 1,
        "req_access": null, "req_one_access": null } },
    "/obj/machinery/door": { "parent": "/obj/machinery", "declared_vars": {
        "icon": { "resource": "a.dmi" }, "icon_state": "closed", "density": 1, "opacity": 1,
        "layer": 2.7, "base_icon_state": "door", "autoclose": 0, "can_open_with_hands": 1,
        "closingLayer": 3.1, "glass": 0, "locked": 0, "safe": 1, "visible": 1, "welded": 0,
        "power_channel": 3, "idle_power_usage": 10, "active_power_usage": 200, "id_tag": null,
        "id": null } },
    "/obj/machinery/door/airlock": { "parent": "/obj/machinery/door", "declared_vars": {
        "autoclose": 1, "base_icon_state": null, "normalspeed": 1,
        "overlays_file": { "resource": "o.dmi" }, "airlock_material": null } },
    "/obj/machinery/door/airlock/security": { "parent": "/obj/machinery/door/airlock",
        "declared_vars": { "name": "security airlock" } },
    "/obj/machinery/status_display": { "parent": "/obj/machinery", "declared_vars": {
        "name": "status display", "icon": { "resource": "sd.dmi" }, "icon_state": "frame" } },
    "/obj/machinery/status_display/door_timer": { "parent": "/obj/machinery/status_display",
        "declared_vars": { "name": "door timer", "id": null,
        "req_access": { "list": ["security"] } } },
    "/obj/structure": { "parent": "/obj", "declared_vars": { "anchored": 1 } },
    "/obj/structure/closet": { "parent": "/obj/structure", "declared_vars": {
        "icon": { "resource": "cl.dmi" }, "icon_state": "generic", "density": 1,
        "anchored": 0, "opened": 0, "locked": 0, "welded": 0, "secure": 0, "icon_door": null,
        "icon_welded": "welded", "enable_door_overlay": 1, "has_opened_overlay": 1,
        "has_closed_overlay": 1, "dense_when_open": 0, "wall_mounted": 0, "horizontal": 0,
        "storage_capacity": 30, "name": "closet" } },
    "/obj/structure/closet/secure_closet": { "parent": "/obj/structure/closet",
        "declared_vars": { "locked": 1, "secure": 1, "req_access": { "list": ["security"] } } },
    "/obj/structure/closet/secure_closet/brig": { "parent": "/obj/structure/closet/secure_closet",
        "declared_vars": { "id": null, "name": "brig locker" } },
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

/// The player at (2,3), the timer at (3,3), the cell airlock at (5,3), its locker at (4,2).
const MAP: &str = r##""w" = (/turf/closed/wall,/area/station)
"f" = (/turf/open/floor,/area/station)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/station)
"T" = (/obj/machinery/status_display/door_timer{id = "Cell 1"; name = "Cell 1"},/turf/open/floor,/area/station)
"A" = (/obj/machinery/door/airlock/security{id = "Cell 1"},/turf/open/floor,/area/station)
"L" = (/obj/structure/closet/secure_closet/brig{id = "Cell 1"; name = "Cell 1 locker"},/turf/open/floor,/area/station)

(1,1,1) = {"
wwwwwww
wfffffw
wsTfAfw
wffLffw
wwwwwww
"}
"##;

const SPAWN: TilePos = TilePos::new(2, 3, 1);
const TIMER: TilePos = TilePos::new(3, 3, 1);
const CELL_DOOR: TilePos = TilePos::new(5, 3, 1);
const LOCKER: TilePos = TilePos::new(4, 2, 1);

fn states() -> Vec<(&'static str, &'static str)> {
    let mut states = vec![
        ("t.dmi", "floor"),
        ("t.dmi", "wall"),
        ("l.dmi", "x2"),
        ("h.dmi", "human_basic"),
        ("i.dmi", "toy"),
        ("sd.dmi", "frame"),
        ("cl.dmi", "generic"),
    ];
    for f in ["closed", "opening", "open", "closing"] {
        states.push(("a.dmi", f));
    }
    states
}

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("brig_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/machinery/status_display/door_timer",
        "/obj/machinery/door/airlock/security",
        "/obj/structure/closet/secure_closet/brig",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
        "/area/station",
    ];
    write_fixture(&dir, TYPES, &used, &states(), &[("map.dmm", MAP)])
}

/// Windows opened (id, interface) and every data message per window.
#[derive(Resource, Default)]
struct Received {
    windows: Vec<(u32, String)>,
    data: Vec<(u32, Vec<u8>)>,
}

fn record(mut ui: MessageReader<UiMessageReceived>, mut received: ResMut<Received>) {
    for UiMessageReceived(message) in ui.read() {
        match message {
            UiServerMessage::Open {
                window,
                interface,
                data,
                ..
            } => {
                received.windows.push((*window, interface.clone()));
                received.data.push((*window, data.clone()));
            }
            UiServerMessage::Update {
                window,
                data: Some(data),
                ..
            } => received.data.push((*window, data.clone())),
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
        s.join("p0", |app| {
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

    fn latest(&self, window: u32) -> Option<BrigTimerData> {
        let (_, data) = self
            .received()
            .data
            .iter()
            .rev()
            .find(|(w, d)| *w == window && !d.is_empty())?;
        ss13_tgui::decode(data).ok()
    }

    fn open_window(&mut self, timer: Entity) -> u32 {
        self.run_until("the timer is replicated", |s| s.net_id(0, timer).is_some());
        self.0.click(0, timer);
        self.run_until("BrigTimer opens", |s| {
            s.received()
                .windows
                .iter()
                .any(|(_, i)| i == BrigTimerUi::NAME)
        });
        self.received()
            .windows
            .iter()
            .rev()
            .find(|(_, i)| i == BrigTimerUi::NAME)
            .unwrap()
            .0
    }

    fn grant(&mut self, access: &str) {
        let mob = self.players[0].mob;
        let world = self.server.world_mut();
        let card = {
            let w: &World = world;
            w.get::<Inventory>(mob).and_then(|i| i.id_card(w))
        }
        .expect("the assistant has an ID");
        world
            .get_mut::<IdCard>(card)
            .unwrap()
            .access
            .push(access.to_owned());
    }

    fn dense(&self, e: Entity) -> bool {
        self.try_get::<Density>(e).is_some()
    }
}

#[test]
fn cell_timer_closes_and_releases_the_cell() {
    let mut s = Station::start("cell_timer");
    let timer = s.find::<DoorTimer>(TIMER);
    let door = s.find::<Door>(CELL_DOOR);
    assert_eq!(s.get::<DoorTimer>(timer).doors, vec![door]);
    assert!(s.dense(door));
    let locker = s.find::<Closet>(LOCKER);
    assert_eq!(s.get::<DoorTimer>(timer).closets, vec![locker]);
    assert_eq!(s.get::<Closet>(locker).id.as_deref(), Some("Cell 1"));
    let locked = |s: &Station| s.get::<Closet>(locker).locked;

    let window = s.open_window(timer);
    // No security access: refused.
    s.act(0, window, START, &[]);
    s.run_for(0.5);
    assert!(!s.get::<DoorTimer>(timer).timing);

    s.grant("security");
    s.act(0, window, PRESET, &[("preset", PRESET_SHORT)]);
    s.run_until("the window shows 5 minutes", |s| {
        s.latest(window)
            .is_some_and(|d| d.minutes == 5 && d.seconds == 0)
    });
    s.act(0, window, START, &[]);
    s.run_until("the timer runs", |s| s.get::<DoorTimer>(timer).timing);
    s.run_until("the window shows it running", |s| {
        s.latest(window).is_some_and(|d| d.timing && d.minutes == 4)
    });
    assert!(s.dense(door), "a closed cell door stays closed");
    assert!(locked(&s), "the locker locks");

    s.act(0, window, STOP, &[]);
    s.run_until("stopping releases the cell", |s| !s.dense(door));
    assert!(!locked(&s), "stopping unlocks the locker");
    assert!(!s.get::<DoorTimer>(timer).timing);
    assert_eq!(s.get::<DoorTimer>(timer).duration, 0.0);

    // 2 seconds (`adjust` is in deciseconds): the open door closes, then opens when it ends.
    s.act(0, window, TIME, &[("adjust", "20")]);
    s.run_until("the timer is set", |s| {
        s.get::<DoorTimer>(timer).duration == 2.0
    });
    s.act(0, window, START, &[]);
    s.run_until("starting closes the cell", |s| s.dense(door));
    assert!(locked(&s));
    s.run_until_for(5.0, "the timer runs out", |s| {
        !s.get::<DoorTimer>(timer).timing
    });
    s.run_until("running out releases the cell", |s| !s.dense(door));
    assert!(!locked(&s), "running out unlocks the locker");
}
