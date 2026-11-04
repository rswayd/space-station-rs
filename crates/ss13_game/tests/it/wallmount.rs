//! Wall mounts over real WebTransport on 127.0.0.1 (`ss13_game::wallmount`): a fire alarm closes
//! the firelocks serving its area (also one in the next area, not one deeper there), turns the
//! area's light red and rings until reset; a light switch turns its area's lights off and on; a
//! door button opens and closes a blast door, a restricted one refuses an assistant, an
//! unpowered one does nothing; an airlock button bolts its airlock; a flasher button fires a
//! mounted flasher at the player in front of it; the extinguisher cabinet gives and takes its
//! extinguisher; the fire axe cabinet stays shut until a multitool unlocks it.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::combat::NextMove;
use ss13_game::doors::Door;
use ss13_game::inventory::Inventory;
use ss13_game::power::{AreaPower, LightFixture, LightMode};
use ss13_game::status_effects::StatusEffects;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_game::tools::DoAfter;
use ss13_game::wallmount::LightSwitch;
use ss13_game::wallmount::cabinets::{ExtinguisherCabinet, FireaxeCabinet};
use ss13_game::wallmount::controls::{Button, Flasher};
use ss13_game::wallmount::fire_alarm::{FireAlarm, Firelock};
use ss13_mapload::{Areas, DmItem, DmWallMount};
use ss13_net::chat::client::ChatReceived;
use ss13_shared::protocol::ClickModifiers;
use ss13_shared::{Appearance, IconStateId, Optics, TilePos};
use ss13_world::{Density, SpatialIndex};

/// tg vars of the types (`firealarm.dm`, `buttons.dm`, `flasher.dm`, `lightswitch.dm`,
/// `extinguisher.dm`, `fireaxe.dm`, `firedoor.dm`, `poddoor.dm`).
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
    "/obj/item/extinguisher": { "parent": "/obj/item", "declared_vars": {
        "name": "fire extinguisher", "icon_state": "fire_extinguisher0" } },
    "/obj/item/fireaxe": { "parent": "/obj/item", "declared_vars": {
        "name": "fire axe", "icon_state": "fireaxe0" } },
    "/obj/item/multitool": { "parent": "/obj/item", "declared_vars": {
        "name": "multitool", "icon_state": "multitool", "tool_behaviour": "multitool",
        "toolspeed": 1 } },
    "/obj/machinery": { "parent": "/obj", "declared_vars": { "anchored": 1,
        "use_power": 1, "idle_power_usage": 100, "active_power_usage": 1000, "power_channel": 1,
        "req_access": null, "req_one_access": null } },
    "/obj/machinery/door": { "parent": "/obj/machinery", "declared_vars": {
        "icon": { "resource": "a.dmi" }, "icon_state": "closed", "density": 1, "opacity": 1,
        "layer": 2.7, "base_icon_state": "door", "autoclose": 0, "can_open_with_hands": 1,
        "closingLayer": 3.1, "glass": 0, "locked": 0, "safe": 1, "visible": 1, "welded": 0,
        "power_channel": 3, "idle_power_usage": 10, "active_power_usage": 200, "id_tag": null } },
    "/obj/machinery/door/airlock": { "parent": "/obj/machinery/door", "declared_vars": {
        "autoclose": 1, "base_icon_state": null, "normalspeed": 1,
        "overlays_file": { "resource": "o.dmi" }, "airlock_material": null } },
    "/obj/machinery/door/firedoor": { "parent": "/obj/machinery/door", "declared_vars": {
        "icon": { "resource": "fd.dmi" }, "icon_state": "door_open", "density": 0,
        "opacity": 0, "glass": 1, "safe": 0, "can_open_with_hands": 0 } },
    "/obj/machinery/door/poddoor": { "parent": "/obj/machinery/door", "declared_vars": {
        "icon": { "resource": "pd.dmi" }, "icon_state": "closed", "id": null, "safe": 0,
        "can_open_with_hands": 0 } },
    "/obj/machinery/light": { "parent": "/obj/machinery", "declared_vars": {
        "icon": { "resource": "light.dmi" }, "icon_state": "tube", "layer": 5,
        "use_power": 2, "idle_power_usage": 2, "active_power_usage": 20, "power_channel": 2,
        "light_angle": 170, "brightness": 8, "bulb_power": 1, "bulb_colour": "#f3fffa",
        "status": 0, "base_state": "tube", "power_consumption_rate": 20, "no_low_power": 0,
        "start_with_cell": 1, "fire_brightness": 9, "fire_power": 0.5, "fire_colour": "#B61C1C" } },
    "/obj/machinery/firealarm": { "parent": "/obj/machinery", "declared_vars": {
        "name": "fire alarm", "icon": { "resource": "wm.dmi" }, "icon_state": "fire0",
        "power_channel": 3, "light_range": 1.6, "light_color": "#00FFFF" } },
    "/obj/machinery/light_switch": { "parent": "/obj/machinery", "declared_vars": {
        "name": "light switch", "icon": { "resource": "wm.dmi" }, "icon_state": "light-nopower",
        "base_icon_state": "light", "power_channel": 2, "light_on_range": 1, "area": null } },
    "/obj/machinery/button": { "parent": "/obj/machinery", "declared_vars": {
        "name": "button", "icon": { "resource": "wm.dmi" }, "icon_state": "button",
        "base_icon_state": "button", "skin": "", "id": null, "device_type": null,
        "power_channel": 3, "light_range": 1.5, "light_power": 0.5, "light_color": "#3CF046" } },
    "/obj/machinery/button/door": { "parent": "/obj/machinery/button", "declared_vars": {
        "name": "door button", "normaldoorcontrol": 0, "specialfunctions": 1, "sync_doors": 1 } },
    "/obj/machinery/button/flasher": { "parent": "/obj/machinery/button", "declared_vars": {
        "name": "flasher button", "skin": "-warning",
        "device_type": { "path": "/obj/item/assembly/control/flasher" } } },
    "/obj/machinery/flasher": { "parent": "/obj/machinery", "declared_vars": {
        "name": "mounted flash", "icon": { "resource": "wm.dmi" }, "icon_state": "mflash1",
        "base_icon_state": "mflash", "id": null, "flash_range": 2, "strength": 50 } },
    "/obj/structure": { "parent": "/obj", "declared_vars": { "anchored": 1 } },
    "/obj/structure/extinguisher_cabinet": { "parent": "/obj/structure", "declared_vars": {
        "name": "extinguisher cabinet", "icon": { "resource": "wm.dmi" },
        "icon_state": "extinguisher" } },
    "/obj/structure/fireaxecabinet": { "parent": "/obj/structure", "declared_vars": {
        "name": "fire axe cabinet", "icon": { "resource": "wm.dmi" }, "icon_state": "fireaxe",
        "locked": 1, "item_path": { "path": "/obj/item/fireaxe" }, "item_overlay": "axe",
        "populate_contents": 1 } },
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
    "/area/station/other": { "parent": "/area/station" },
    "/area/station/dark": { "parent": "/area/station", "declared_vars": { "requires_power": 1,
        "power_equip": 0, "power_light": 0, "power_environ": 0 } }
  }
}"##;

/// `w`/`f` station, `o` the next area, `d` an area without power. The player stands at (3,5)
/// with a multitool; every wall mount but the flasher hangs on (4,5); the flasher (3,4) faces
/// the player; firelocks at (6,5) (station), (8,4) (next area, beside a station tile) and
/// (10,4) (deep in the next area); a blast door (11,6), an airlock (6,2), a light (5,6) and an
/// unpowered door button (2,5).
const MAP: &str = r##""w" = (/turf/closed/wall,/area/station)
"f" = (/turf/open/floor,/area/station)
"o" = (/turf/open/floor,/area/station/other)
"s" = (/obj/effect/landmark/start/assistant,/obj/item/multitool,/turf/open/floor,/area/station)
"Z" = (/obj/machinery/button/door{id = "pod"},/turf/open/floor,/area/station/dark)
"M" = (/obj/machinery/firealarm,/obj/machinery/light_switch,/obj/machinery/button/door{id = "pod"},/obj/machinery/button/door{id = "lock"; normaldoorcontrol = 1; specialfunctions = 4},/obj/machinery/button/door{id = "pod"; req_access = list("brig")},/obj/machinery/button/flasher{id = "fl"},/obj/structure/extinguisher_cabinet,/obj/structure/fireaxecabinet,/turf/open/floor,/area/station)
"X" = (/obj/machinery/flasher{id = "fl"},/turf/open/floor,/area/station)
"F" = (/obj/machinery/door/firedoor,/turf/open/floor,/area/station)
"G" = (/obj/machinery/door/firedoor,/turf/open/floor,/area/station/other)
"P" = (/obj/machinery/door/poddoor{id = "pod"},/turf/open/floor,/area/station/other)
"D" = (/obj/machinery/door/airlock{id_tag = "lock"},/turf/open/floor,/area/station)
"l" = (/obj/machinery/light{dir = 1},/turf/open/floor,/area/station)

(1,1,1) = {"
wwwwwwwwwwww
wfffloooofPw
wZsMfFfoooow
wfXffffGoGow
wffffffoooow
wffffDfoooow
wwwwwwwwwwww
"}
"##;

const SPAWN: TilePos = TilePos::new(3, 5, 1);
const MOUNTS: TilePos = TilePos::new(4, 5, 1);

fn states() -> Vec<(&'static str, &'static str)> {
    let mut states = vec![
        ("t.dmi", "floor"),
        ("t.dmi", "wall"),
        ("l.dmi", "x2"),
        ("h.dmi", "human_basic"),
        ("i.dmi", "toy"),
        ("i.dmi", "multitool"),
        ("i.dmi", "fire_extinguisher0"),
        ("i.dmi", "fireaxe0"),
        ("light.dmi", "tube"),
        ("light.dmi", "tube_emergency"),
    ];
    for f in ["closed", "opening", "open", "closing"] {
        states.push(("a.dmi", f));
        states.push(("pd.dmi", f));
    }
    for f in ["door_closed", "door_opening", "door_open", "door_closing"] {
        states.push(("fd.dmi", f));
    }
    states.push(("fd.dmi", "firelock_alarm_type_generic"));
    states.extend(
        [
            "fire0",
            "fire_0",
            "fire_level_e",
            "fire_alerting",
            "fire_alerting_e",
            "light-on",
            "light-off",
            "light-emissive-on",
            "light-emissive-off",
            "light-nopower",
            "button",
            "button-nopower",
            "button-light-mask",
            "button-overlay-success",
            "button-overlay-error",
            "button-warning",
            "button-warning-nopower",
            "mflash1",
            "mflash1-p",
            "extinguisher",
            "extinguisher_default",
            "door",
            "door_open",
            "fireaxe",
            "axe",
            "glass",
            "glass_raised",
            "locked",
            "unlocked",
        ]
        .map(|s| ("wm.dmi", s)),
    );
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
        .join("wallmount_fixture")
        .join(test);
    let mut used = vec![
        "/obj/effect/landmark/start/assistant",
        "/obj/item/multitool",
        "/obj/machinery/button/door",
        "/obj/machinery/button/flasher",
        "/obj/machinery/firealarm",
        "/obj/machinery/light_switch",
        "/obj/machinery/flasher",
        "/obj/machinery/light",
        "/obj/machinery/door/firedoor",
        "/obj/machinery/door/poddoor",
        "/obj/machinery/door/airlock",
        "/obj/structure/extinguisher_cabinet",
        "/obj/structure/fireaxecabinet",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
        "/area/station",
        "/area/station/other",
        "/area/station/dark",
    ];
    used.extend(ss13_mapload::WALLMOUNT_RUNTIME_TYPES);
    write_fixture(&dir, TYPES, &used, &states(), &[("map.dmm", MAP)])
}

#[derive(Resource, Default)]
struct Received(Vec<String>);

fn record(mut chat: MessageReader<ChatReceived>, mut received: ResMut<Received>) {
    for ChatReceived(message) in chat.read() {
        received.0.push(message.plain_text());
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

    /// The entities on `pos` with `T` that `keep` accepts.
    fn find<T: Component>(&self, pos: TilePos, keep: impl Fn(&World, Entity) -> bool) -> Entity {
        let world = self.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .find(|&e| world.get::<T>(e).is_some() && keep(world, e))
            .unwrap_or_else(|| panic!("nothing with {} on {pos:?}", std::any::type_name::<T>()))
    }

    fn door_at(&self, x: i32, y: i32) -> Entity {
        self.find::<Door>(TilePos::new(x, y, 1), |_, _| true)
    }

    fn dense(&self, e: Entity) -> bool {
        self.try_get::<Density>(e).is_some()
    }

    fn area(&self, pos: TilePos) -> Entity {
        self.server
            .world()
            .resource::<Areas>()
            .area_at(pos)
            .unwrap()
    }

    fn click_with(&mut self, e: Entity, modifiers: ClickModifiers) {
        let mob = self.players[0].mob;
        self.run_until("off the click cooldown and replicated", |s| {
            s.now() >= s.get::<NextMove>(mob).0 && s.net_id(0, e).is_some()
        });
        self.0.click_with(0, e, modifiers);
    }

    fn click(&mut self, e: Entity) {
        self.click_with(e, ClickModifiers::default());
    }

    fn right_click(&mut self, e: Entity) {
        self.click_with(
            e,
            ClickModifiers {
                right: true,
                ..default()
            },
        );
    }

    fn held(&self) -> Option<Entity> {
        self.get::<Inventory>(self.players[0].mob).active_item()
    }

    fn said(&self, text: &str) -> bool {
        self.players[0]
            .app
            .world()
            .resource::<Received>()
            .0
            .iter()
            .any(|l| l.contains(text))
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
fn fire_alarm_and_light_switch() {
    let mut s = Station::start("fire_alarm");
    let alarm = s.find::<FireAlarm>(MOUNTS, |_, _| true);
    let switch = s.find::<LightSwitch>(MOUNTS, |_, _| true);
    let light = s.find::<LightFixture>(TilePos::new(5, 6, 1), |_, _| true);
    let (inner, border, deep) = (s.door_at(6, 5), s.door_at(8, 4), s.door_at(10, 4));
    let station = s.area(SPAWN);
    assert_eq!(
        s.get::<Firelock>(border).areas.len(),
        2,
        "next area and station"
    );
    assert!(!s.dense(inner) && !s.dense(border) && !s.dense(deep));
    assert!(s.overlays(alarm).contains(&state_id("wm.dmi", "fire_0")));
    let normal = s.get::<Optics>(light).light.unwrap();

    s.click(alarm);
    s.run_until("the area's firelocks close", |s| {
        s.dense(inner) && s.dense(border)
    });
    assert!(s.get::<AreaPower>(station).fire);
    assert!(
        s.get::<FireAlarm>(alarm).ringing.is_some(),
        "the pulled alarm rings"
    );
    assert!(
        s.overlays(alarm)
            .contains(&state_id("wm.dmi", "fire_alerting"))
    );
    assert!(
        s.overlays(inner)
            .contains(&state_id("fd.dmi", "firelock_alarm_type_generic"))
    );
    let red = s.get::<Optics>(light).light.unwrap();
    assert_eq!(
        (red.color, red.range),
        ([0xB6, 0x1C, 0x1C], 9.0),
        "fire_colour"
    );
    assert_eq!(
        s.get::<Appearance>(light).icon_state,
        state_id("light.dmi", "tube_emergency")
    );
    s.hold(2.0, "a firelock deeper in the next area stays open", |s| {
        !s.dense(deep)
    });

    s.right_click(alarm);
    s.run_until("reset opens them", |s| !s.dense(inner) && !s.dense(border));
    assert!(!s.get::<AreaPower>(station).fire);
    assert!(s.get::<FireAlarm>(alarm).ringing.is_none());
    assert_eq!(s.get::<Optics>(light).light, Some(normal));
    assert!(s.overlays(inner).is_empty());

    // The light switch.
    s.click(switch);
    s.run_until("the lights go off", |s| {
        s.get::<LightFixture>(light).mode == LightMode::Off
    });
    assert!(!s.get::<AreaPower>(station).lightswitch);
    assert_eq!(
        s.get::<Appearance>(switch).icon_state,
        state_id("wm.dmi", "light-off")
    );
    assert!(
        s.get::<Optics>(switch).light.is_some(),
        "an off switch glows"
    );
    s.click(switch);
    s.run_until("and on again", |s| {
        s.get::<LightFixture>(light).mode == LightMode::On
    });
}

#[test]
fn buttons_and_flasher() {
    let mut s = Station::start("buttons");
    let button = |s: &Station, keep: fn(&DmWallMount) -> bool| {
        s.find::<Button>(MOUNTS, move |w, e| {
            w.get::<DmWallMount>(e).is_some_and(keep)
        })
    };
    let pod_button = button(&s, |d| d.id == "pod" && d.req_access.is_empty());
    let secure = button(&s, |d| !d.req_access.is_empty());
    let bolts = button(&s, |d| d.normaldoorcontrol);
    let flash_button = button(&s, |d| d.id == "fl");
    let dark = s.find::<Button>(TilePos::new(2, 5, 1), |_, _| true);
    let pod = s.door_at(11, 6);
    let airlock = s.door_at(6, 2);
    assert!(s.dense(pod));

    s.click(dark);
    s.hold(1.5, "an unpowered button does nothing", |s| s.dense(pod));
    assert_eq!(
        s.get::<Appearance>(dark).icon_state,
        state_id("wm.dmi", "button-nopower")
    );

    s.click(secure);
    s.run_until("access denied", |s| s.said("access denied"));
    s.hold(1.5, "the blast door stays shut", |s| s.dense(pod));

    s.click(pod_button);
    s.run_until("the blast door opens", |s| !s.dense(pod));
    assert!(
        s.overlays(pod_button)
            .contains(&state_id("wm.dmi", "button-overlay-success"))
    );
    s.run_until("the flick ends", |s| {
        !s.overlays(pod_button)
            .contains(&state_id("wm.dmi", "button-overlay-success"))
    });
    s.run_for(1.0);
    s.click(pod_button);
    s.run_until("and closes", |s| s.dense(pod));

    s.click(bolts);
    s.run_until("the airlock is bolted", |s| s.get::<Door>(airlock).locked);

    let flasher = s.find::<Flasher>(TilePos::new(3, 4, 1), |_, _| true);
    let mob = s.players[0].mob;
    s.click(flash_button);
    s.run_until("the flasher paralyzes the player", |s| {
        s.try_get::<StatusEffects>(mob)
            .is_some_and(|e| e.paralyze > 0.0)
    });
    assert!(s.get::<Flasher>(flasher).ready_at > s.now(), "cooling down");
}

#[test]
fn cabinets() {
    let mut s = Station::start("cabinets");
    let cabinet = s.find::<ExtinguisherCabinet>(MOUNTS, |_, _| true);
    let axe_cabinet = s.find::<FireaxeCabinet>(MOUNTS, |_, _| true);
    let extinguisher = s.get::<ExtinguisherCabinet>(cabinet).stored.unwrap();
    assert_eq!(
        s.get::<Appearance>(cabinet).icon_state,
        state_id("wm.dmi", "extinguisher_default")
    );

    s.click(cabinet);
    s.run_until("the extinguisher is in hand", |s| {
        s.held() == Some(extinguisher)
    });
    assert!(s.get::<ExtinguisherCabinet>(cabinet).opened);
    assert!(
        s.overlays(cabinet)
            .contains(&state_id("wm.dmi", "door_open"))
    );
    s.click(cabinet);
    s.run_until("and back in", |s| {
        s.get::<ExtinguisherCabinet>(cabinet).stored == Some(extinguisher)
    });
    assert_eq!(s.held(), None);

    s.click(axe_cabinet);
    s.run_until("locked", |s| s.said("won't budge!"));
    assert!(!s.get::<FireaxeCabinet>(axe_cabinet).open);

    let tool = s.find::<DmItem>(SPAWN, |w, e| {
        w.get::<DmItem>(e)
            .is_some_and(|i| i.path == "/obj/item/multitool")
    });
    s.click(tool);
    s.run_until("the multitool is in hand", |s| s.held() == Some(tool));
    let mob = s.players[0].mob;
    s.click(axe_cabinet);
    s.run_until("resetting", |s| s.try_get::<DoAfter>(mob).is_some());
    s.run_until("unlocked after 2 s", |s| {
        !s.get::<FireaxeCabinet>(axe_cabinet).locked
    });
    // Drop the multitool for an empty hand.
    s.send(0, ss13_shared::protocol::ClientIntent::DropActive);
    s.run_until("dropped", |s| s.held().is_none());
    s.click(axe_cabinet);
    s.run_until("open", |s| s.get::<FireaxeCabinet>(axe_cabinet).open);
    let axe = s.get::<FireaxeCabinet>(axe_cabinet).held.unwrap();
    s.click(axe_cabinet);
    s.run_until("the axe is in hand", |s| s.held() == Some(axe));
    assert!(!s.overlays(axe_cabinet).contains(&state_id("wm.dmi", "axe")));
}

/// MetaStation's wall mounts: counts by kind, buttons that reach something, the brig flashers;
/// pulls the fire alarm of the area with the most firelocks and resets it.
/// `SS13_ASSETS=$ASSETS cargo test -p ss13_game --all-features --test it wallmount::metastation -- --ignored --nocapture`
#[test]
#[ignore = "needs generated assets in SS13_ASSETS"]
fn metastation() {
    let map = ss13_mapload::MapLoadPlugin {
        maps: vec!["MetaStation".into()],
        ..ss13_mapload::MapLoadPlugin::from_env().unwrap_or(ss13_mapload::MapLoadPlugin {
            source: env!("SS13_SOURCE").into(),
            maps: vec![],
            assets: std::env::var("SS13_ASSETS")
                .unwrap_or_else(|_| "assets/generated".into())
                .into(),
        })
    };
    let mut app = testing::server_app(map, |_| {});
    let world = app.world_mut();
    let count = |world: &mut World, name: &str, n: usize| {
        println!("{name}: {n}");
        assert!(n > 0, "no {name}");
    };
    let n = world.query::<&FireAlarm>().iter(world).count();
    count(world, "fire alarms", n);
    let n = world.query::<&Firelock>().iter(world).count();
    count(world, "firelocks", n);
    let n = world.query::<&LightSwitch>().iter(world).count();
    count(world, "light switches", n);
    let n = world.query::<&Flasher>().iter(world).count();
    count(world, "flashers", n);
    let n = world.query::<&ExtinguisherCabinet>().iter(world).count();
    count(world, "extinguisher cabinets", n);
    let n = world.query::<&FireaxeCabinet>().iter(world).count();
    count(world, "fire axe cabinets", n);
    let buttons: Vec<Button> = world.query::<&Button>().iter(world).cloned().collect();
    let links = world.resource::<ss13_game::wallmount::controls::Links>();
    let linked = buttons
        .iter()
        .filter(|b| {
            b.id.as_ref().is_some_and(|id| {
                links.poddoors.contains_key(id)
                    || links.airlocks.contains_key(id)
                    || links.flashers.contains_key(id)
            })
        })
        .count();
    println!(
        "buttons: {}, linked to something: {linked}; blast door ids {}, airlock ids {}",
        buttons.len(),
        links.poddoors.len(),
        links.airlocks.len()
    );
    assert!(linked * 2 > buttons.len(), "most buttons reach their doors");
    let stocked = world
        .query::<&ExtinguisherCabinet>()
        .iter(world)
        .filter(|c| c.stored.is_some())
        .count();
    assert!(stocked > 0, "cabinets start with extinguishers");

    let (alarm, area) = world
        .query::<(Entity, &FireAlarm)>()
        .iter(world)
        .map(|(e, a)| (e, a.area))
        .max_by_key(|&(_, area)| {
            world
                .get::<ss13_game::wallmount::fire_alarm::AreaFire>(area)
                .map_or(0, |f| f.firedoors.len())
        })
        .unwrap();
    ss13_game::wallmount::fire_alarm::alarm(app.world_mut(), alarm, None);
    for _ in 0..60 {
        app.update();
    }
    let world = app.world_mut();
    let doors = world
        .get::<ss13_game::wallmount::fire_alarm::AreaFire>(area)
        .unwrap()
        .firedoors
        .clone();
    let closed = doors
        .iter()
        .filter(|&&d| world.get::<Density>(d).is_some())
        .count();
    println!("alarm pulled: {closed}/{} firelocks closed", doors.len());
    assert!(world.get::<AreaPower>(area).unwrap().fire);
    assert_eq!(closed, doors.len());
}
