//! Debug queries and dev tools over real WebTransport on 127.0.0.1: a fixture room with air and
//! a powered area (the power test's station, turfs with tg's station air). With the gate open a
//! tile query reports air, power and the entities, and every dev tool applies; with dev tools
//! closed they are refused and change nothing, and with queries closed even a tile query is.

use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_atmos::AtmosPlugin;
use ss13_debug::server::{DebugGate, DebugServerPlugin, GodMode};
use ss13_game::health::Health;
use ss13_game::round::Ticker;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_net::client::NetEntityMap;
use ss13_net::debug::client::{DebugResponseReceived, SendDebugRequest};
use ss13_shared::TilePos;
use ss13_shared::debug::{DebugRequest, DebugResponse, TileReport};
use ss13_world::SpatialIndex;

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 }, "declared_vars": { "light_power": 1,
        "can_atmos_pass": 1 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3 } },
    "/obj": { "parent": "/atom/movable" },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start",
        "declared_vars": { "icon_state": "Assistant" } },
    "/obj/item": { "parent": "/obj", "declared_vars": { "interaction_flags_item": 1 } },
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
    "/obj/item/stock_parts/power_store": { "parent": "/obj/item", "declared_vars": {
        "maxcharge": 10000, "chargerate": 500 } },
    "/obj/item/stock_parts/power_store/battery": { "parent": "/obj/item/stock_parts/power_store",
        "declared_vars": { "maxcharge": 1000000, "chargerate": 1000000 } },
    "/obj/item/stock_parts/power_store/battery/upgraded": { "parent": "/obj/item/stock_parts/power_store/battery",
        "declared_vars": { "maxcharge": 2500000, "chargerate": 500000 } },
    "/obj/item/stock_parts/power_store/battery/high": { "parent": "/obj/item/stock_parts/power_store/battery",
        "declared_vars": { "maxcharge": 10000000, "chargerate": 750000 } },
    "/obj/item/stock_parts/power_store/battery/high/empty": { "parent": "/obj/item/stock_parts/power_store/battery/high" },
    "/obj/item/circuitboard": { "parent": "/obj/item" },
    "/obj/item/circuitboard/machine/smes": { "parent": "/obj/item/circuitboard", "declared_vars": {
        "def_components": {"list": [{"assoc": [{"path": "/obj/item/stock_parts/power_store/battery"}, {"path": "/obj/item/stock_parts/power_store/battery/high/empty"}]}]},
        "req_components": {"list": [{"assoc": [{"path": "/obj/item/stock_parts/power_store/battery"}, 5]}]} } },
    "/obj/machinery": { "parent": "/obj", "declared_vars": {
        "use_power": 1, "idle_power_usage": 100, "active_power_usage": 1000, "power_channel": 1 } },
    "/obj/machinery/door": { "parent": "/obj/machinery", "declared_vars": {
        "icon": { "resource": "d.dmi" }, "icon_state": "door_closed", "density": 1, "opacity": 1,
        "layer": 2.7, "base_icon_state": "door", "autoclose": 0, "can_open_with_hands": 1,
        "closingLayer": 3.1, "glass": 0, "locked": 0, "safe": 1, "visible": 1, "welded": 0,
        "power_channel": 3, "idle_power_usage": 10, "active_power_usage": 200 } },
    "/obj/machinery/door/airlock": { "parent": "/obj/machinery/door", "declared_vars": {
        "icon": { "resource": "a.dmi" }, "icon_state": "closed", "autoclose": 1,
        "base_icon_state": null, "normalspeed": 1, "overlays_file": { "resource": "o.dmi" },
        "airlock_material": null } },
    "/obj/machinery/light": { "parent": "/obj/machinery", "declared_vars": {
        "icon": { "resource": "light.dmi" }, "icon_state": "tube", "layer": 5,
        "use_power": 2, "idle_power_usage": 2, "active_power_usage": 20, "power_channel": 2,
        "light_angle": 170, "brightness": 8, "bulb_power": 1, "bulb_colour": "#f3fffa",
        "status": 0, "base_state": "tube", "power_consumption_rate": 20, "no_low_power": 0,
        "start_with_cell": 1 } },
    "/obj/machinery/power": { "parent": "/obj/machinery", "declared_vars": {
        "use_power": 0, "idle_power_usage": 0, "active_power_usage": 0, "cable_layer": 2 } },
    "/obj/machinery/power/apc": { "parent": "/obj/machinery/power", "declared_vars": {
        "icon": { "resource": "apc.dmi" }, "icon_state": "apc0", "layer": 4,
        "cell_type": {"path": "/obj/item/stock_parts/power_store/battery/upgraded"},
        "start_charge": 90, "lighting": 3, "equipment": 3, "environ": 3, "operating": 1,
        "chargemode": 1, "areastring": null } },
    "/obj/machinery/power/smes": { "parent": "/obj/machinery/power", "declared_vars": {
        "icon": { "resource": "smes.dmi" }, "icon_state": "smes", "density": 1,
        "circuit": {"path": "/obj/item/circuitboard/machine/smes"}, "charge": 0,
        "input_level": 50000, "output_level": 50000, "input_attempt": 1, "output_attempt": 1 } },
    "/obj/machinery/power/smes/engineering": { "parent": "/obj/machinery/power/smes",
        "declared_vars": { "charge": 50000000, "output_level": 90000 } },
    "/obj/machinery/power/terminal": { "parent": "/obj/machinery/power", "declared_vars": {
        "icon": { "resource": "c.dmi" }, "icon_state": "term" } },
    "/obj/machinery/power/rtg": { "parent": "/obj/machinery/power", "declared_vars": {
        "icon": { "resource": "smes.dmi" }, "icon_state": "rtg", "power_gen": 1000 } },
    "/obj/structure": { "parent": "/obj" },
    "/obj/structure/cable": { "parent": "/obj/structure", "declared_vars": {
        "icon": { "resource": "c.dmi" }, "icon_state": "l2-1-2-4-8-node", "cable_layer": 2 } },
    "/mob": { "parent": "/atom/movable", "declared_vars": { "layer": 4, "density": 1 } },
    "/mob/living": { "parent": "/mob" },
    "/mob/living/carbon": { "parent": "/mob/living" },
    "/mob/living/carbon/human": { "parent": "/mob/living/carbon", "declared_vars": {
        "icon": { "resource": "h.dmi" }, "icon_state": "human_basic" } },
    "/turf": { "parent": "/atom", "declared_vars": { "icon": { "resource": "t.dmi" },
        "blocks_air": 0, "can_atmos_pass": 0, "initial_gas_mix": "o2=22;n2=82;TEMP=293.15" } },
    "/turf/open": { "parent": "/turf", "declared_vars": { "can_atmos_pass": -1,
        "planetary_atmos": 0 } },
    "/turf/open/floor": { "parent": "/turf/open", "declared_vars": { "icon_state": "floor" } },
    "/turf/open/space": { "parent": "/turf/open", "declared_vars": {
        "initial_gas_mix": "TEMP=2.7" } },
    "/turf/open/space/basic": { "parent": "/turf/open/space" },
    "/turf/closed": { "parent": "/turf", "declared_vars": { "density": 1, "opacity": 1,
        "blocks_air": 1 } },
    "/turf/closed/wall": { "parent": "/turf/closed", "declared_vars": { "icon_state": "wall" } },
    "/area": { "parent": "/atom", "declared_vars": { "requires_power": 1, "always_unpowered": 0,
        "power_equip": 1, "power_light": 1, "power_environ": 1, "lightswitch": 1,
        "static_lighting": 1 } },
    "/area/space": { "parent": "/area", "declared_vars": { "always_unpowered": 1,
        "power_equip": 0, "power_light": 0, "power_environ": 0 } },
    "/area/station": { "parent": "/area" }
  },
  "element_calls": { "/datum/element/undertile": [
    { "type": "/obj/machinery/power/terminal", "proc": "Initialize", "call": "AddElement", "line": "" },
    { "type": "/obj/structure/cable", "proc": "Initialize", "call": "AddElement", "line": "" }
  ] }
}"##;

const MAP: &str = r##""w" = (/turf/closed/wall,/area/station)
"f" = (/turf/open/floor,/area/station)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/station)
"R" = (/obj/structure/cable,/obj/machinery/power/rtg,/turf/open/floor,/area/station)
"T" = (/obj/structure/cable,/obj/machinery/power/terminal{dir = 4},/turf/open/floor,/area/station)
"S" = (/obj/structure/cable,/obj/machinery/power/smes/engineering,/turf/open/floor,/area/station)
"c" = (/obj/structure/cable,/turf/open/floor,/area/station)
"A" = (/obj/structure/cable,/obj/machinery/power/apc{dir = 1; pixel_y = 25},/turf/open/floor,/area/station)
"l" = (/obj/machinery/light{dir = 1},/turf/open/floor,/area/station)
"n" = (/obj/machinery/light{dir = 1; no_low_power = 1},/turf/open/floor,/area/station)
"D" = (/obj/machinery/door/airlock{normalspeed = 0},/turf/open/floor,/area/station)

(1,1,1) = {"
wwwwwwwwwwww
wRTSccAfffff
wfffflfffnfw
wfsfDffffffw
wffffffffffw
wwwwwwwwwwww
"}
"##;

fn states() -> Vec<(String, String)> {
    let mut states: Vec<(String, String)> = [
        ("t.dmi", "floor"),
        ("t.dmi", "wall"),
        ("t.dmi", ""),
        ("l.dmi", "x2"),
        ("l.dmi", "Assistant"),
        ("h.dmi", "human_basic"),
        ("a.dmi", "closed"),
        ("a.dmi", "opening"),
        ("a.dmi", "open"),
        ("a.dmi", "closing"),
        ("light.dmi", "tube"),
        ("light.dmi", "tube_emergency"),
        ("apc.dmi", "apc0"),
    ]
    .map(|(d, s)| (d.to_owned(), s.to_owned()))
    .into();
    let apc = |s: String| ("apc.dmi".to_owned(), s);
    states.extend((0..2).map(|i| apc(format!("apcox-{i}"))));
    states.extend((0..3).map(|i| apc(format!("apco3-{i}"))));
    for c in 0..3 {
        states.extend((0..4).map(|i| apc(format!("apco{c}-{i}"))));
    }
    states
}

const SPAWN: TilePos = TilePos::new(3, 3, 1);
const FLOOR: TilePos = TilePos::new(8, 3, 1);
const ID_CARD: &str = "/obj/item/card/id/advanced";

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("debug_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/machinery/door/airlock",
        "/obj/machinery/light",
        "/obj/machinery/power/apc",
        "/obj/machinery/power/smes/engineering",
        "/obj/machinery/power/terminal",
        "/obj/machinery/power/rtg",
        "/obj/structure/cable",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
        "/area/station",
    ];
    let states = states();
    let states: Vec<(&str, &str)> = states.iter().map(|(d, s)| (&**d, &**s)).collect();
    write_fixture(&dir, TYPES, &used, &states, &[("map.dmm", MAP)])
}

/// A started server with the debug half and one joined client, on game time.
struct Session {
    st: testing::Station,
    mob: Entity,
    responses: Vec<DebugResponse>,
}

impl Deref for Session {
    type Target = testing::Station;
    fn deref(&self) -> &testing::Station {
        &self.st
    }
}

impl DerefMut for Session {
    fn deref_mut(&mut self) -> &mut testing::Station {
        &mut self.st
    }
}

impl Drive for Session {
    fn station(&mut self) -> &mut testing::Station {
        &mut self.st
    }

    /// A tick, then the client's debug answers are collected.
    fn step(&mut self) {
        self.st.tick();
        let mut received = self.st.players[0]
            .app
            .world_mut()
            .resource_mut::<Messages<DebugResponseReceived>>();
        self.responses.extend(received.drain().map(|r| r.0));
    }
}

impl Session {
    fn start(test: &str, gate: DebugGate) -> Self {
        let assets = fixture_assets(test);
        let mut st = testing::Station::start(fixture_map(&assets, &["map.dmm"]), |app| {
            app.insert_resource(gate)
                .add_plugins((AtmosPlugin, DebugServerPlugin));
        });
        let p = st.join("dev", |_| {});
        let mob = st.players[p].mob;
        Self {
            st,
            mob,
            responses: Vec::new(),
        }
    }

    fn client(&self) -> &App {
        &self.players[0].app
    }

    /// Sends `request` and returns the first answer to it.
    fn ask(&mut self, request: DebugRequest) -> DebugResponse {
        self.responses.clear();
        self.st.players[0]
            .app
            .world_mut()
            .write_message(SendDebugRequest(request.clone()));
        self.run_until_for(10.0, &format!("an answer to {request:?}"), |s| {
            !s.responses.is_empty()
        });
        self.responses.pop().unwrap()
    }

    fn tile(&mut self, pos: TilePos) -> TileReport {
        match self.ask(DebugRequest::Tile(pos)) {
            DebugResponse::Tile(report) => report,
            other => panic!("tile query answered {other:?}"),
        }
    }

    fn mob_pos(&self) -> TilePos {
        *self.server.world().get::<TilePos>(self.mob).unwrap()
    }
}

fn done(response: DebugResponse) -> String {
    match response {
        DebugResponse::Done(text) => text,
        other => panic!("expected Done, got {other:?}"),
    }
}

fn refused(response: DebugResponse) -> String {
    match response {
        DebugResponse::Refused(text) => text,
        other => panic!("expected Refused, got {other:?}"),
    }
}

#[test]
fn tile_query_reports_air_power_and_entities() {
    let mut s = Session::start(
        "tile_query",
        DebugGate {
            queries: true,
            dev_tools: true,
        },
    );
    let report = s.tile(SPAWN);
    println!("{report:#?}");
    assert_eq!(report.pos, SPAWN);
    assert!(!report.turf.is_empty(), "turf named");
    let air = report.air.expect("station air on a floor");
    // o2=22;n2=82 at 293.15 K in a 2500 l cell: tg's 101.4 kPa.
    assert!(
        (air.pressure_kpa - 101.4).abs() < 1.0,
        "{}",
        air.pressure_kpa
    );
    assert!((air.temperature_k - 293.15).abs() < 0.01);
    let gases: Vec<&str> = air.gases.iter().map(|g| g.0.as_str()).collect();
    assert_eq!(gases, ["o2", "n2"]);
    let area = report.area.expect("an area");
    assert_eq!(area.name, "station");
    assert!(area.requires_power && area.has_apc);
    assert_eq!(area.powered, [true; 3]);
    // The own mob stands there, with the asking client's id.
    let mob_id = s
        .client()
        .world()
        .resource::<NetEntityMap>()
        .controlled()
        .and_then(|e| s.client().world().get::<ss13_net::client::NetEntity>(e))
        .unwrap()
        .0;
    let mob = report
        .entities
        .iter()
        .find(|e| e.net_id == Some(mob_id))
        .expect("own mob in the report");
    assert!(
        mob.components.iter().any(|c| c == "Health"),
        "{:?}",
        mob.components
    );

    // A wall: no air.
    assert!(s.tile(TilePos::new(1, 1, 1)).air.is_none());

    match s.ask(DebugRequest::ServerStats) {
        DebugResponse::ServerStats(stats) => {
            assert_eq!(stats.clients.len(), 1);
            assert_eq!(stats.clients[0].name, "dev");
            assert!(stats.entities > 0 && stats.tick_max_ms > 0.0);
            assert_eq!(stats.round_state, "Playing");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn dev_tools_apply_when_the_gate_is_open() {
    let mut s = Session::start(
        "dev_open",
        DebugGate {
            queries: true,
            dev_tools: true,
        },
    );

    println!("{}", done(s.ask(DebugRequest::Teleport(FLOOR))));
    assert_eq!(s.mob_pos(), FLOOR);
    refused(s.ask(DebugRequest::Teleport(TilePos::new(500, 3, 1))));

    match s.ask(DebugRequest::FindPrototypes("card/id".into())) {
        DebugResponse::Prototypes { ids, .. } => {
            assert!(ids.iter().any(|id| id == ID_CARD), "{ids:?}")
        }
        other => panic!("{other:?}"),
    }
    let before = s
        .server
        .world()
        .resource::<SpatialIndex>()
        .on_tile(SPAWN)
        .len();
    done(s.ask(DebugRequest::Spawn {
        path: ID_CARD.into(),
        pos: SPAWN,
    }));
    let after = s
        .server
        .world()
        .resource::<SpatialIndex>()
        .on_tile(SPAWN)
        .len();
    assert_eq!(after, before + 1);
    refused(s.ask(DebugRequest::Spawn {
        path: "/obj/nope".into(),
        pos: SPAWN,
    }));

    done(s.ask(DebugRequest::GodMode(true)));
    assert!(s.server.world().get::<GodMode>(s.mob).is_some());
    s.st.server
        .world_mut()
        .get_mut::<Health>(s.mob)
        .unwrap()
        .brute = 80.0;
    s.step();
    assert_eq!(s.server.world().get::<Health>(s.mob).unwrap().brute, 0.0);
    done(s.ask(DebugRequest::GodMode(false)));
    assert!(s.server.world().get::<GodMode>(s.mob).is_none());

    done(s.ask(DebugRequest::TimeScale(2.0)));
    assert_eq!(
        s.server
            .world()
            .resource::<Time<Virtual>>()
            .relative_speed(),
        2.0
    );
    refused(s.ask(DebugRequest::TimeScale(f32::NAN)));

    done(s.ask(DebugRequest::EndRound));
    assert!(s.server.world().resource::<Ticker>().force_ending);
}

#[test]
fn dev_tools_are_refused_when_the_gate_is_closed() {
    let mut s = Session::start(
        "dev_closed",
        DebugGate {
            queries: true,
            dev_tools: false,
        },
    );
    let start = s.mob_pos();
    let why = refused(s.ask(DebugRequest::Teleport(FLOOR)));
    assert!(why.contains("dev tools"), "{why}");
    assert_eq!(s.mob_pos(), start);
    let before = s
        .server
        .world()
        .resource::<SpatialIndex>()
        .on_tile(SPAWN)
        .len();
    refused(s.ask(DebugRequest::Spawn {
        path: ID_CARD.into(),
        pos: SPAWN,
    }));
    assert_eq!(
        s.server
            .world()
            .resource::<SpatialIndex>()
            .on_tile(SPAWN)
            .len(),
        before
    );
    refused(s.ask(DebugRequest::GodMode(true)));
    assert!(s.server.world().get::<GodMode>(s.mob).is_none());
    refused(s.ask(DebugRequest::EndRound));
    assert!(!s.server.world().resource::<Ticker>().force_ending);
    refused(s.ask(DebugRequest::TimeScale(2.0)));
    // Queries still work.
    assert!(s.tile(SPAWN).air.is_some());

    // Queries closed: even a tile query is refused.
    s.server.world_mut().insert_resource(DebugGate {
        queries: false,
        dev_tools: true,
    });
    refused(s.ask(DebugRequest::Tile(SPAWN)));
    refused(s.ask(DebugRequest::Teleport(FLOOR)));
    assert_eq!(s.mob_pos(), start);
}
