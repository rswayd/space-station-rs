//! Firelocks and the machines buttons drive, with air, over real WebTransport on 127.0.0.1
//! (`ss13_game::wallmount::{firelock, machines}`):
//! - hot air next to a row of firelocks closes the row (not a firelock in another row) with
//!   hot hazard lights, turns both areas red and makes the fire alarm alert; a reset cracks them
//!   open and they close again 3 s later while the air stays hot; cooled air opens them;
//! - a firelock closed by the fire alarm opens by hand after 5 s and closes again 2 s later; a
//!   crowbar holds it open until the player walks away;
//! - the ignition button lights an igniter and sparks a sparker, both set plasma on fire;
//! - the mass driver button throws an item on the driver along its facing.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_atmos::{Atmos, AtmosPlugin};
use ss13_game::combat::NextMove;
use ss13_game::doors::{Door, DoorState};
use ss13_game::power::AreaPower;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_game::tools::DoAfter;
use ss13_game::wallmount::controls::Button;
use ss13_game::wallmount::fire_alarm::FireAlarm;
use ss13_game::wallmount::firelock::{AlarmType, Firelock};
use ss13_game::wallmount::machines::{Igniter, MassDriver, Sparker};
use ss13_mapload::{Areas, DmItem, DmWallMount};
use ss13_shared::protocol::ClickModifiers;
use ss13_shared::{Appearance, Dir, IconStateId, TilePos};
use ss13_world::{Density, SpatialIndex};

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 }, "declared_vars": { "light_power": 1,
        "can_atmos_pass": 1, "flags_1": 0 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3, "anchored": 0 } },
    "/obj": { "parent": "/atom/movable" },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start" },
    "/obj/item": { "parent": "/obj", "declared_vars": { "icon": { "resource": "i.dmi" },
        "icon_state": "toy", "interaction_flags_item": 1, "tool_behaviour": null,
        "toolspeed": 1, "throw_speed": 3, "throw_range": 7 } },
    "/obj/item/toy": { "parent": "/obj/item", "declared_vars": { "name": "toy" } },
    "/obj/item/crowbar": { "parent": "/obj/item", "declared_vars": { "name": "crowbar",
        "icon_state": "crowbar", "tool_behaviour": "crowbar" } },
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
        "power_channel": 3, "can_atmos_pass": -2, "heat_proof": 0, "id_tag": null } },
    "/obj/machinery/door/firedoor": { "parent": "/obj/machinery/door", "declared_vars": {
        "name": "firelock", "icon": { "resource": "fd.dmi" }, "icon_state": "door_open",
        "density": 0, "opacity": 0, "glass": 1, "safe": 0, "can_open_with_hands": 0,
        "heat_proof": 1 } },
    "/obj/machinery/firealarm": { "parent": "/obj/machinery", "declared_vars": {
        "name": "fire alarm", "icon": { "resource": "wm.dmi" }, "icon_state": "fire0",
        "power_channel": 3, "light_range": 1.6, "light_color": "#00FFFF" } },
    "/obj/machinery/button": { "parent": "/obj/machinery", "declared_vars": {
        "name": "button", "icon": { "resource": "wm.dmi" }, "icon_state": "button",
        "base_icon_state": "button", "skin": "", "id": null, "device_type": null,
        "power_channel": 3 } },
    "/obj/machinery/button/ignition": { "parent": "/obj/machinery/button", "declared_vars": {
        "device_type": { "path": "/obj/item/assembly/control/igniter" } } },
    "/obj/machinery/button/massdriver": { "parent": "/obj/machinery/button", "declared_vars": {
        "device_type": { "path": "/obj/item/assembly/control/massdriver" } } },
    "/obj/machinery/igniter": { "parent": "/obj/machinery", "declared_vars": {
        "name": "igniter", "icon": { "resource": "floor.dmi" }, "icon_state": "igniter0",
        "base_icon_state": "igniter", "id": null, "on": 0 } },
    "/obj/machinery/sparker": { "parent": "/obj/machinery", "declared_vars": {
        "name": "mounted igniter", "icon": { "resource": "wm.dmi" }, "icon_state": "migniter",
        "base_icon_state": "migniter", "id": null } },
    "/obj/machinery/mass_driver": { "parent": "/obj/machinery", "declared_vars": {
        "name": "mass driver", "icon": { "resource": "floor.dmi" }, "icon_state": "mass_driver",
        "base_icon_state": "mass_driver", "id": 1, "power": 1, "drive_range": 10,
        "power_per_obj": 1000 } },
    "/mob": { "parent": "/atom/movable", "declared_vars": { "layer": 4, "density": 1 } },
    "/mob/living": { "parent": "/mob" },
    "/mob/living/carbon": { "parent": "/mob/living" },
    "/mob/living/carbon/human": { "parent": "/mob/living/carbon", "declared_vars": {
        "icon": { "resource": "h.dmi" }, "icon_state": "human_basic" } },
    "/turf": { "parent": "/atom", "declared_vars": { "icon": { "resource": "t.dmi" },
        "blocks_air": 0, "can_atmos_pass": 0, "initial_gas_mix": "o2=22;n2=82;TEMP=293.15",
        "temperature": 293.15 } },
    "/turf/open": { "parent": "/turf", "declared_vars": { "can_atmos_pass": -1,
        "planetary_atmos": 0 } },
    "/turf/open/floor": { "parent": "/turf/open", "declared_vars": { "icon_state": "floor",
        "thermal_conductivity": 0.02, "heat_capacity": 20000 } },
    "/turf/open/space": { "parent": "/turf/open", "declared_vars": { "temperature": 2.7,
        "initial_gas_mix": "TEMP=2.7" } },
    "/turf/open/space/basic": { "parent": "/turf/open/space" },
    "/turf/closed": { "parent": "/turf", "declared_vars": { "density": 1, "opacity": 1,
        "blocks_air": 1 } },
    "/turf/closed/wall": { "parent": "/turf/closed", "declared_vars": { "icon_state": "wall" } },
    "/area": { "parent": "/atom", "declared_vars": { "requires_power": 0, "always_unpowered": 0,
        "power_equip": 1, "power_light": 1, "power_environ": 1, "lightswitch": 1,
        "static_lighting": 1 } },
    "/area/space": { "parent": "/area" },
    "/area/station": { "parent": "/area" },
    "/area/station/other": { "parent": "/area/station" }
  }
}"##;

/// `f` the station, `o` the next area. The player stands at (5,4) with a crowbar, next to the
/// wall mounts on (4,4) (fire alarm, ignition and mass driver buttons) and to a row of two
/// firelocks (6,5), (6,4); another firelock (6,2) is its own row. In the next area an igniter
/// (8,2), a sparker (7,4) and a mass driver facing north with a toy on it (9,2).
const MAP: &str = r##""w" = (/turf/closed/wall,/area/station)
"f" = (/turf/open/floor,/area/station)
"o" = (/turf/open/floor,/area/station/other)
"s" = (/obj/effect/landmark/start/assistant,/obj/item/crowbar,/turf/open/floor,/area/station)
"M" = (/obj/machinery/firealarm,/obj/machinery/button/ignition{id = "ign"},/obj/machinery/button/massdriver{id = "md"},/turf/open/floor,/area/station)
"F" = (/obj/machinery/door/firedoor,/turf/open/floor,/area/station)
"I" = (/obj/machinery/igniter{id = "ign"},/turf/open/floor,/area/station/other)
"K" = (/obj/machinery/sparker{id = "ign"},/turf/open/floor,/area/station/other)
"D" = (/obj/machinery/mass_driver{id = "md"; dir = 1},/obj/item/toy,/turf/open/floor,/area/station/other)

(1,1,1) = {"
wwwwwwwwww
wffffFooow
wffMsFKoow
wffffwooow
wffffFoIDw
wwwwwwwwww
"}
"##;

const SPAWN: TilePos = TilePos::new(5, 4, 1);
const MOUNTS: TilePos = TilePos::new(4, 4, 1);

fn states() -> Vec<(&'static str, &'static str)> {
    let mut states = vec![
        ("t.dmi", "floor"),
        ("t.dmi", "wall"),
        ("l.dmi", "x2"),
        ("h.dmi", "human_basic"),
        ("i.dmi", "toy"),
        ("i.dmi", "crowbar"),
        ("floor.dmi", "igniter0"),
        ("floor.dmi", "igniter1"),
        ("floor.dmi", "mass_driver"),
        ("floor.dmi", "mass_driver1"),
    ];
    for f in ["door_closed", "door_opening", "door_open", "door_closing"] {
        states.push(("fd.dmi", f));
    }
    for t in AlarmType::ALL {
        states.push(("fd.dmi", t.icon_state()));
    }
    states.extend(
        [
            "fire0",
            "fire_0",
            "fire_level_e",
            "fire_alerting",
            "fire_alerting_e",
            "button",
            "button-nopower",
            "button-light-mask",
            "button-overlay-success",
            "button-overlay-error",
            "migniter",
            "migniter-p",
            "migniter-spark",
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
        .join("firelocks_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/item/crowbar",
        "/obj/item/toy",
        "/obj/machinery/firealarm",
        "/obj/machinery/button/ignition",
        "/obj/machinery/button/massdriver",
        "/obj/machinery/door/firedoor",
        "/obj/machinery/igniter",
        "/obj/machinery/sparker",
        "/obj/machinery/mass_driver",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
        "/area/station",
        "/area/station/other",
    ];
    write_fixture(&dir, TYPES, &used, &states(), &[("map.dmm", MAP)])
}

struct Station(testing::Station);

ss13_game::drive_station!(Station, 0);

impl Station {
    fn start(test: &str) -> Self {
        let assets = fixture_assets(test);
        let mut s = testing::Station::start(fixture_map(&assets, &["map.dmm"]), |app| {
            app.add_plugins(AtmosPlugin);
        });
        s.join("p0", |_| {});
        let s = Self(s);
        assert_eq!(*s.get::<TilePos>(s.players[0].mob), SPAWN);
        s
    }

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

    fn button(&self, id: &str) -> Entity {
        self.find::<Button>(MOUNTS, |w, e| {
            w.get::<Button>(e).unwrap().id.as_deref() == Some(id)
        })
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

    fn atmos(&mut self) -> Mut<'_, Atmos> {
        self.server.world_mut().resource_mut::<Atmos>()
    }

    /// Sets the air temperature of the open tiles in columns `xs`.
    fn heat(&mut self, xs: std::ops::RangeInclusive<i32>, temperature: f64) {
        let mut atmos = self.atmos();
        for x in xs {
            for y in 2..=5 {
                atmos.modify_air(TilePos::new(x, y, 1), |m| m.temperature = temperature);
            }
        }
    }

    fn click_with(&mut self, e: Entity, modifiers: ClickModifiers) {
        let mob = self.players[0].mob;
        self.run_until("off the click cooldown and replicated", |s| {
            s.now() >= s.get::<NextMove>(mob).0
                && s.net_id(0, e).is_some()
                && s.try_get::<DoAfter>(mob).is_none()
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

    fn overlays(&self, e: Entity) -> Vec<IconStateId> {
        self.get::<Appearance>(e)
            .overlays
            .iter()
            .map(|o| o.icon_state)
            .collect()
    }

    /// Picks up the crowbar from the spawn tile.
    fn take_crowbar(&mut self) {
        let crowbar = self.find::<DmItem>(SPAWN, |w, e| {
            w.get::<DmItem>(e).unwrap().path == "/obj/item/crowbar"
        });
        self.click(crowbar);
        let mob = self.players[0].mob;
        self.run_until("the crowbar is in hand", |s| {
            s.get::<ss13_game::inventory::Inventory>(mob).active_item() == Some(crowbar)
        });
    }
}

#[test]
fn hot_air_closes_a_row_and_cool_air_opens_it() {
    let mut s = Station::start("hot_air");
    let (top, bottom, apart) = (s.door_at(6, 5), s.door_at(6, 4), s.door_at(6, 2));
    let alarm = s.find::<FireAlarm>(MOUNTS, |_, _| true);
    let (station, other) = (s.area(SPAWN), s.area(TilePos::new(8, 4, 1)));
    s.run_for(1.0);
    assert!(!s.dense(top) && !s.dense(bottom) && !s.dense(apart));

    // Above `FIRE_MINIMUM_TEMPERATURE_TO_EXIST` (373.15 K), and still after a few seconds of
    // mixing with the station side.
    s.heat(7..=9, 800.0);
    s.run_until("the row closes", |s| s.dense(top) && s.dense(bottom));
    let hot = state_id("fd.dmi", AlarmType::Hot.icon_state());
    assert!(s.overlays(top).contains(&hot) && s.overlays(bottom).contains(&hot));
    assert!(s.get::<AreaPower>(station).fire && s.get::<AreaPower>(other).fire);
    assert!(
        s.overlays(alarm)
            .contains(&state_id("wm.dmi", "fire_alerting"))
    );
    let ringing = [top, bottom]
        .iter()
        .filter(|&&f| s.get::<Firelock>(f).ringing.is_some())
        .count();
    assert_eq!(ringing, 1, "the firelock that noticed rings");
    assert!(s.get::<FireAlarm>(alarm).ringing.is_none(), "not pulled");
    // (6,2) watches the hot air too, as its own row.
    s.run_until("the other row closes too", |s| s.dense(apart));

    // A reset cracks them open; the air is still hot, so they close again after 3 s.
    s.right_click(alarm);
    s.run_until("reset opens the row", |s| !s.dense(top) && !s.dense(bottom));
    assert!(
        s.overlays(top).is_empty(),
        "no lights while ignoring alarms"
    );
    s.run_until_for(5.0, "and it closes again", |s| {
        s.dense(top) && s.dense(bottom)
    });
    assert!(s.get::<Firelock>(top).active);

    // Cool air everywhere (some heat got through while they were open): the rows reset and open.
    s.heat(2..=9, 293.15);
    s.run_until("cool air opens the row", |s| {
        !s.dense(top) && !s.dense(bottom) && !s.dense(apart)
    });
    assert!(!s.get::<AreaPower>(station).fire && !s.get::<AreaPower>(other).fire);
    assert!(s.overlays(top).is_empty());
    assert!(
        s.get::<Firelock>(top).ringing.is_none() && s.get::<Firelock>(bottom).ringing.is_none()
    );
}

#[test]
fn opening_by_hand_and_crowbar() {
    let mut s = Station::start("by_hand");
    let firelock = s.door_at(6, 4);
    let alarm = s.find::<FireAlarm>(MOUNTS, |_, _| true);
    s.click(alarm);
    s.run_until("the alarm closes it", |s| {
        s.get::<Door>(firelock).state == DoorState::Closed
    });
    let station = s.area(SPAWN);
    assert!(s.get::<AreaPower>(station).fire);

    // By hand: 5 s, then 2 s open before the active firelock closes again.
    s.click(firelock);
    let mob = s.players[0].mob;
    s.run_until("a do_after starts", |s| s.try_get::<DoAfter>(mob).is_some());
    s.hold(4.5, "still closed during the do_after", |s| {
        s.dense(firelock)
    });
    s.run_until_for(3.0, "it opens", |s| !s.dense(firelock));
    s.run_until_for(4.0, "and closes again", |s| {
        s.get::<Door>(firelock).state == DoorState::Closed
    });

    // A crowbar holds it open while the player stays next to it.
    s.take_crowbar();
    s.click(firelock);
    s.run_until("pried open", |s| !s.dense(firelock));
    assert_eq!(s.get::<Firelock>(firelock).held_by, Some(mob));
    s.hold(3.0, "held open", |s| !s.dense(firelock));
    s.run_until("the player walks away", |s| {
        s.try_step(0, Dir::West);
        s.get::<TilePos>(mob).x <= 4 || s.get::<TilePos>(mob).y != 4
    });
    s.run_until("released, it closes", |s| s.dense(firelock));
    assert!(s.get::<Firelock>(firelock).held_by.is_none());
}

#[test]
fn ignition_button_lights_plasma() {
    let mut s = Station::start("ignition");
    let igniter = s.find::<Igniter>(TilePos::new(8, 2, 1), |_, _| true);
    let sparker = s.find::<Sparker>(TilePos::new(7, 4, 1), |_, _| true);
    {
        let mut atmos = s.atmos();
        for pos in [TilePos::new(8, 2, 1), TilePos::new(7, 4, 1)] {
            assert!(atmos.spawn_air(pos, "plasma=20;TEMP=293.15").unwrap());
        }
    }
    s.run_for(0.5);
    assert_eq!(s.atmos().hotspots(), 0);
    let button = s.button("ign");
    s.click(button);
    s.run_until("the igniter turns on", |s| s.get::<Igniter>(igniter).on);
    assert_eq!(
        s.get::<Appearance>(igniter).icon_state,
        state_id("floor.dmi", "igniter1")
    );
    assert!(
        s.get::<Sparker>(sparker).last_spark > 0.0,
        "the sparker sparked"
    );
    s.run_until("the sparker's tile burns", |s| {
        s.atmos().hotspot(TilePos::new(7, 4, 1)).is_some()
            || s.atmos().hotspot(TilePos::new(7, 3, 1)).is_some()
    });
    s.run_until_for(5.0, "the igniter's tile burns within a machine tick", |s| {
        s.atmos().hotspot(TilePos::new(8, 2, 1)).is_some()
    });
    // After the controller's 3 s cooldown, another press turns the igniter off.
    s.run_for(3.0);
    s.click(button);
    s.run_until("the igniter turns off", |s| !s.get::<Igniter>(igniter).on);
}

#[test]
fn mass_driver_throws_along_its_facing() {
    let mut s = Station::start("mass_driver");
    let at = TilePos::new(9, 2, 1);
    let driver = s.find::<MassDriver>(at, |_, _| true);
    assert_eq!(s.get::<MassDriver>(driver).drive_range, 20.0);
    let toy = s.find::<DmItem>(at, |_, _| true);
    s.click(s.button("md"));
    s.hold(0.9, "nothing flies before stage 2", |s| {
        *s.get::<TilePos>(toy) == at
    });
    s.run_until("thrown north to the wall", |s| {
        *s.get::<TilePos>(toy) == TilePos::new(9, 5, 1)
    });
    assert!(s.get::<DmWallMount>(driver).path.ends_with("mass_driver"));
}
