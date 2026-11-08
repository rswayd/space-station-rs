//! Power over real WebTransport on 127.0.0.1: a fixture station with an RTG charging a SMES,
//! the SMES feeding an APC, two lights and an airlock; then an outage (the APC cell drained,
//! charging off): the lights go to emergency light or dark on the client and the airlock stays
//! shut, until charging is back.
//!
//! `cargo test -p ss13_game --all-features --test it power::metastation -- --ignored --nocapture`
//! reports MetaStation's grid and runs the outage on a real area (needs generated assets in
//! `SS13_ASSETS`).

use std::path::{Path, PathBuf};
use std::time::Duration;

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use ss13_game::doors::{Door, DoorState};
use ss13_game::power::{
    Apc, AreaPower, Channel, Grid, LightFixture, LightMode, PowerMachine, PowerReport, Smes,
    Unpowered,
};
use ss13_game::server::MoveQueue;
use ss13_game::testing::{self, Drive, steps, write_fixture};
use ss13_mapload::{DoorKind, MapLoadPlugin};
use ss13_net::client::{LightBlocks, NetEntity};
use ss13_net::server::{Interest, ReplicationStats};
use ss13_shared::protocol::{ClickModifiers, ClientIntent, NetId};
use ss13_shared::{Appearance, Dir, IconStateId, LightSource, Optics, TilePos};
use ss13_world::{Density, SpatialIndex};

pub(crate) const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 }, "declared_vars": { "light_power": 1 } },
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
    "/obj/machinery": { "parent": "/obj", "declared_vars": { "anchored": 1,
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
        "start_with_cell": 1, "overlay_icon": { "resource": "lo.dmi" } } },
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
    "/turf": { "parent": "/atom", "declared_vars": { "icon": { "resource": "t.dmi" } } },
    "/turf/open": { "parent": "/turf" },
    "/turf/open/floor": { "parent": "/turf/open", "declared_vars": { "icon_state": "floor" } },
    "/turf/open/space": { "parent": "/turf/open" },
    "/turf/open/space/basic": { "parent": "/turf/open/space" },
    "/turf/closed": { "parent": "/turf", "declared_vars": { "density": 1, "opacity": 1 } },
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

/// Rows top-down, y = 1 at the bottom. Row 5: an RTG and the SMES terminal (`T`, facing east)
/// on the input cable, the SMES (`S`) and the output cable to the APC (`A`). `l` and `n`
/// (`no_low_power`: no emergency cell) are lights, `D` an airlock that autocloses in 1.5 s.
pub(crate) const MAP: &str = r##""w" = (/turf/closed/wall,/area/station)
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

/// [`MAP`] 30 tiles wide, with a third light 15 tiles east of the spawn, beyond the entity view.
const WIDE_MAP: &str = r##""w" = (/turf/closed/wall,/area/station)
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
wwwwwwwwwwwwwwwwwwwwwwwwwwwwww
wRTSccAffffffffffffffffffffffw
wfffflfffnffffffflfffffffffffw
wfsfDffffffffffffffffffffffffw
wffffffffffffffffffffffffffffw
wwwwwwwwwwwwwwwwwwwwwwwwwwwwww
"}
"##;

const SPAWN: TilePos = TilePos::new(3, 3, 1);
const FAR_LIGHT: TilePos = TilePos::new(18, 4, 1);
const DOOR: TilePos = TilePos::new(5, 3, 1);
const APC: TilePos = TilePos::new(7, 5, 1);
const LIGHT: TilePos = TilePos::new(6, 4, 1);
const BARE_LIGHT: TilePos = TilePos::new(10, 4, 1);

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
        ("light.dmi", "tube-empty"),
        ("light.dmi", "tube-broken"),
        ("light.dmi", "tube-burned"),
        ("lo.dmi", "tube"),
        ("lo.dmi", "tube_emergency"),
        ("lo.dmi", "tube_nightshift"),
        ("lo.dmi", "tube_flickering"),
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

fn state_id(dmi: &str, state: &str) -> IconStateId {
    let i = states()
        .iter()
        .position(|(d, s)| d == dmi && s == state)
        .unwrap();
    IconStateId(i as u32)
}

pub(crate) fn fixture_assets(test: &str) -> PathBuf {
    fixture_assets_with(test, MAP)
}

fn fixture_assets_with(test: &str, map: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("power_fixture")
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
    write_fixture(&dir, TYPES, &used, &states, &[("map.dmm", map)])
}

/// A started server and one joined client; every frame is one tick of game time on both sides.
struct Session {
    st: testing::Station,
    link: Entity,
    mob: Entity,
}

ss13_game::drive_station!(Session, st);

impl Session {
    fn start(map: MapLoadPlugin) -> Self {
        let mut st = testing::Station::start(map, |_| {});
        // Light block bytes per client (`ClientTraffic::light_bytes_total`).
        st.server
            .world_mut()
            .resource_mut::<ReplicationStats>()
            .count_payload = true;
        let p = st.join("a", |_| {});
        let (link, mob) = (st.players[p].link, st.players[p].mob);
        Self { st, link, mob }
    }

    fn client(&self) -> &App {
        &self.players[0].app
    }

    fn pos(&self) -> TilePos {
        *self.server.world().get::<TilePos>(self.mob).unwrap()
    }

    fn press(&mut self, dir: Dir) {
        let queue = self.server.world().get::<MoveQueue>(self.mob).unwrap();
        if self.players[0].seq == queue.applied() {
            self.players[0].seq += 1;
            let seq = self.players[0].seq;
            self.st.send(0, ClientIntent::Move { dir, seq });
        }
    }

    fn walk_to(&mut self, dir: Dir, to: TilePos) {
        self.run_until(&format!("walk {dir:?} to {to:?}"), |s| {
            if s.pos() == to {
                return true;
            }
            s.press(dir);
            false
        });
    }

    /// The first entity on `pos` with component `T`.
    fn find<T: Component>(&self, pos: TilePos) -> Entity {
        let world = self.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .find(|&e| world.get::<T>(e).is_some())
            .unwrap_or_else(|| panic!("nothing with {} at {pos:?}", std::any::type_name::<T>()))
    }

    fn net_id(&self, e: Entity) -> Option<NetId> {
        self.server
            .world()
            .resource::<Interest>()
            .net_id(self.link, e)
    }

    /// The light the client draws on server entity `e`'s tile (`None`: its light block is not
    /// known yet).
    fn client_light(&mut self, e: Entity) -> Option<Option<LightSource>> {
        let pos = *self.get::<TilePos>(e);
        let lights = self
            .client()
            .world()
            .resource::<LightBlocks>()
            .lights(pos)?;
        Some(lights.first().copied())
    }

    fn client_opaque(&self, pos: TilePos) -> Option<bool> {
        self.client().world().resource::<LightBlocks>().opaque(pos)
    }

    fn light_bytes(&self) -> u64 {
        let stats = self.server.world().resource::<ReplicationStats>();
        stats
            .clients
            .get(&self.link)
            .map_or(0, |c| c.light_bytes_total)
    }

    fn client_overlays(&mut self, e: Entity) -> Vec<IconStateId> {
        let Some(id) = self.net_id(e) else {
            return Vec::new();
        };
        let world = self.players[0].app.world_mut();
        let mut q = world.query::<(&NetEntity, &Appearance)>();
        q.iter(world)
            .find(|(n, _)| n.0 == id)
            .map(|(_, a)| a.overlays.iter().map(|o| o.icon_state).collect())
            .unwrap_or_default()
    }

    fn click(&mut self, e: Entity) {
        let target = self.net_id(e).expect("replicated");
        self.st.send(
            0,
            ClientIntent::Click {
                target,
                modifiers: ClickModifiers::default(),
            },
        );
    }

    fn door_state(&self, door: Entity) -> DoorState {
        self.get::<Door>(door).state
    }

    fn apc_mut(&mut self, apc: Entity) -> Mut<'_, Apc> {
        self.server.world_mut().get_mut::<Apc>(apc).unwrap()
    }
}

#[test]
fn outage_turns_lights_off_and_holds_doors() {
    let assets = fixture_assets("outage");
    let mut s = Session::start(MapLoadPlugin {
        source: assets.clone(),
        maps: vec!["map.dmm".into()],
        assets,
    });
    assert_eq!(s.pos(), SPAWN);
    let report = s.server.world().resource::<PowerReport>().clone();
    // Input net (RTG + terminal) and output net (SMES -> APC).
    assert_eq!((report.nets, report.cables), (2, 6));
    assert_eq!(
        (report.apcs, report.apcs_unwired, report.duplicate_apcs),
        (1, 0, 0)
    );
    assert_eq!(
        (report.smes, report.smes_broken, report.generators),
        (1, 0, 1)
    );
    assert_eq!((report.areas_needing_power, report.areas_with_apc), (1, 1));
    let (apc, smes) = (s.find::<Apc>(APC), s.find::<Smes>(TilePos::new(4, 5, 1)));
    let (light, bare) = (
        s.find::<LightFixture>(LIGHT),
        s.find::<LightFixture>(BARE_LIGHT),
    );
    let door = s.find::<Door>(DOOR);
    assert_eq!(s.get::<Door>(door).kind, DoorKind::Airlock);
    assert_eq!(s.get::<PowerMachine>(door).channel, Channel::Environ);
    let area = s.get::<Apc>(apc).area;
    // 2 lights at ceil(8 * 1 * 20) W; the airlock idles at 10 W.
    let static_power = s.get::<AreaPower>(area).static_power;
    assert_eq!(static_power, [0.0, 320.0, 10.0]);

    // A few machine ticks: the SMES feeds the APC, which charges at 1%/s of 2.5 MJ.
    let charge0 = s.get::<Apc>(apc).cell.unwrap().charge;
    s.hold(6.0, "powered at start", |s| {
        s.server.world().get::<Unpowered>(door).is_none()
    });
    let charged = s.get::<Apc>(apc).cell.unwrap().charge - charge0;
    assert!(
        (90_000.0..=150_000.0).contains(&charged),
        "charged {charged} J in 6 s"
    );
    assert!(s.get::<Smes>(smes).outputting);
    let grid = s.server.world().resource::<Grid>();
    let out = grid.net(grid.node_net(smes).unwrap()).unwrap();
    assert_eq!(out.avail, 180_000.0, "90 kW for 2 s");
    let input = s.get::<Smes>(smes).terminal.unwrap();
    let input = grid.net(grid.node_net(input).unwrap()).unwrap();
    assert_eq!(input.avail, 2000.0, "the RTG's 1 kW");
    assert_eq!(s.get::<LightFixture>(light).mode, LightMode::On);
    let on = s.client_light(light).expect("replicated").expect("lit");
    assert_eq!(on.range, 8.0);
    let channels_on = |c| state_id("apc.dmi", &format!("apco{c}-3"));
    s.run_until("the client draws the APC lights", |s| {
        let overlays = s.client_overlays(apc);
        (0..3).all(|c| overlays.contains(&channels_on(c)))
    });

    // Powered: a bump opens the airlock, it closes by itself.
    s.walk_to(Dir::East, TilePos::new(4, 3, 1));
    s.run_until("the airlock opens on a bump", |s| {
        s.press(Dir::East);
        s.door_state(door) != DoorState::Closed
    });
    s.walk_to(Dir::West, TilePos::new(4, 3, 1));
    s.run_until("the airlock closes", |s| {
        s.door_state(door) == DoorState::Closed
    });

    // Outage: the cell is drained and charging is off. Within a machine tick every channel goes
    // off (charge 0: `AUTOSET_FORCE_OFF`).
    {
        let mut a = s.apc_mut(apc);
        a.cell.as_mut().unwrap().charge = 0.0;
        a.chargemode = false;
    }
    s.run_until("the area loses power", |s| {
        s.get::<AreaPower>(area).power == [false; 3]
    });
    assert!(s.server.world().get::<Unpowered>(door).is_some());
    assert_eq!(s.get::<LightFixture>(light).mode, LightMode::Emergency);
    assert_eq!(s.get::<LightFixture>(bare).mode, LightMode::Off);
    // Lighting load is gone with the lights.
    assert_eq!(s.get::<AreaPower>(area).static_power[1], 0.0);
    s.run_until(
        "the client sees the emergency light and the dark fixture",
        |s| {
            let emergency = s.client_light(light).flatten();
            let bare = s.client_light(bare);
            emergency.is_some_and(|l| l.color == [0xFF, 0x32, 0x32] && l.range == 2.0)
                && bare == Some(None)
        },
    );
    let channels_off = |c| state_id("apc.dmi", &format!("apco{c}-1"));
    s.run_until("the client draws the APC channels off", |s| {
        let overlays = s.client_overlays(apc);
        overlays.contains(&channels_off(0)) && overlays.contains(&channels_off(1))
    });

    // The unpowered airlock ignores bumps and clicks.
    for _ in 0..steps(1.0) {
        s.press(Dir::East);
        s.step();
    }
    s.click(door);
    s.hold(1.5, "the unpowered airlock stays shut", |s| {
        s.door_state(door) == DoorState::Closed && s.server.world().get::<Density>(door).is_some()
    });
    assert_eq!(s.pos(), TilePos::new(4, 3, 1));

    // Charging back on: the first charge brings the environment channel back (any charge
    // under 15% keeps it on), so the airlock works again.
    s.apc_mut(apc).chargemode = true;
    s.run_until("environment power is back", |s| {
        s.server.world().get::<Unpowered>(door).is_none()
    });
    assert_eq!(s.get::<LightFixture>(light).mode, LightMode::Emergency);
    s.click(door);
    s.run_until("the powered airlock opens on a click", |s| {
        s.door_state(door) != DoorState::Closed
    });
}

/// [`MAP`] with the area's light switch off (tg `turn_off_lights_with_lightswitch`).
const DARK_MAP: &str = r##""w" = (/turf/closed/wall,/area/station{lightswitch = 0})
"f" = (/turf/open/floor,/area/station{lightswitch = 0})
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/station{lightswitch = 0})
"l" = (/obj/machinery/light{dir = 1},/turf/open/floor,/area/station{lightswitch = 0})

(1,1,1) = {"
wwwwww
wflffw
wfsffw
wwwwww
"}
"##;

/// A fixture that starts off keeps neither the map's light nor its lit overlays.
#[test]
fn fixture_off_at_round_start_is_dark() {
    let assets = fixture_assets_with("dark", DARK_MAP);
    let mut s = Session::start(MapLoadPlugin {
        source: assets.clone(),
        maps: vec!["map.dmm".into()],
        assets,
    });
    let light = s.find::<LightFixture>(TilePos::new(3, 3, 1));
    s.hold(1.0, "stays dark", |s| {
        s.get::<LightFixture>(light).mode == LightMode::Off
    });
    assert_eq!(s.get::<Optics>(light).light, None);
    assert!(s.get::<Appearance>(light).overlays.is_empty());
}

/// tg `update_overlays()` through every state of a tube: the lit overlays and their emissive mask
/// come and go with the light, on the server and on the client.
#[test]
fn fixture_overlays_follow_its_state() {
    use ss13_game::power::lighting::{LightStatus, set_status};
    use ss13_game::power::{set_area_fire, set_nightshift};

    let assets = fixture_assets("overlays");
    let mut s = Session::start(MapLoadPlugin {
        source: assets.clone(),
        maps: vec!["map.dmm".into()],
        assets,
    });
    let (apc, light, bare) = (
        s.find::<Apc>(APC),
        s.find::<LightFixture>(LIGHT),
        s.find::<LightFixture>(BARE_LIGHT),
    );
    let area = s.get::<Apc>(apc).area;
    let glow = |tint: &str| vec![state_id("lo.dmi", "tube"), state_id("lo.dmi", tint)];
    let server = |s: &Session, e: Entity| -> (IconStateId, Vec<IconStateId>) {
        let a = s.get::<Appearance>(e);
        (
            a.icon_state,
            a.overlays.iter().map(|o| o.icon_state).collect(),
        )
    };
    let sprite = |state: &str| state_id("light.dmi", state);
    s.run_until("the client draws the lit overlays", |s| {
        s.client_overlays(light) == glow("tube")
    });
    assert_eq!(
        s.get::<Appearance>(light).overlays[0].plane,
        Some(ss13_shared::EMISSIVE_PLANE),
        "the first overlay is the emissive mask"
    );

    set_area_fire(s.server.world_mut(), area, true);
    assert_eq!(
        server(&s, light),
        (sprite("tube_emergency"), glow("tube_emergency"))
    );
    set_area_fire(s.server.world_mut(), area, false);
    set_nightshift(s.server.world_mut(), apc, true);
    assert_eq!(server(&s, light), (sprite("tube"), glow("tube_nightshift")));
    set_nightshift(s.server.world_mut(), apc, false);
    assert_eq!(server(&s, light), (sprite("tube"), glow("tube")));

    for (status, state) in [
        (LightStatus::Broken, "tube-broken"),
        (LightStatus::Burned, "tube-burned"),
        (LightStatus::Empty, "tube-empty"),
    ] {
        set_status(s.server.world_mut(), bare, status, None);
        assert_eq!(server(&s, bare), (sprite(state), Vec::new()), "{state}");
        s.run_until("the client drops the overlays", |s| {
            s.client_overlays(bare).is_empty()
        });
        // A new bulb lights up again.
        set_status(s.server.world_mut(), bare, LightStatus::Ok, None);
        assert_eq!(server(&s, bare), (sprite("tube"), glow("tube")), "{state}");
        s.run_until("the client draws them again", |s| {
            s.client_overlays(bare) == glow("tube")
        });
    }

    // No power: the cell's emergency light has no overlays either, only its icon state.
    {
        let mut a = s.apc_mut(apc);
        a.cell.as_mut().unwrap().charge = 0.0;
        a.chargemode = false;
    }
    s.run_until("the area loses power", |s| {
        s.get::<AreaPower>(area).power == [false; 3]
    });
    s.run_until("the client drops the overlays of both", |s| {
        s.client_overlays(light).is_empty() && s.client_overlays(bare).is_empty()
    });
    assert_eq!(s.get::<LightFixture>(light).mode, LightMode::Emergency);
    assert_eq!(server(&s, light), (sprite("tube_emergency"), Vec::new()));
    assert_eq!(s.get::<LightFixture>(bare).mode, LightMode::Off);
    assert_eq!(server(&s, bare), (sprite("tube"), Vec::new()));
}

/// Light beyond the entity view: a fixture 15 tiles away is known to the client without its
/// entity, the outage reaches it, and the airlock's opacity follows it opening and closing.
#[test]
fn far_light_and_door_opacity_reach_the_client() {
    let assets = fixture_assets_with("far_light", WIDE_MAP);
    let mut s = Session::start(MapLoadPlugin {
        source: assets.clone(),
        maps: vec!["map.dmm".into()],
        assets,
    });
    assert_eq!(s.pos(), SPAWN);
    let (far, apc, door) = (
        s.find::<LightFixture>(FAR_LIGHT),
        s.find::<Apc>(APC),
        s.find::<Door>(DOOR),
    );
    s.run_until("the far light is known", |s| {
        s.client_light(far)
            .flatten()
            .is_some_and(|l| l.range == 8.0)
    });
    assert!(
        s.net_id(far).is_none(),
        "15 tiles away: not a replicated entity"
    );
    assert_eq!(s.client_opaque(DOOR), Some(true));
    assert_eq!(
        s.client_opaque(TilePos::new(30, 6, 1)),
        Some(true),
        "the far corner wall"
    );
    println!("fixture join: {} light bytes", s.light_bytes());

    s.walk_to(Dir::East, TilePos::new(4, 3, 1));
    s.run_until("the airlock opens and lets light through", |s| {
        s.press(Dir::East);
        s.client_opaque(DOOR) == Some(false)
    });
    s.walk_to(Dir::West, TilePos::new(3, 3, 1));
    s.run_until("the airlock closes and blocks light", |s| {
        s.door_state(door) == DoorState::Closed && s.client_opaque(DOOR) == Some(true)
    });

    let before = s.light_bytes();
    {
        let mut a = s.apc_mut(apc);
        a.cell.as_mut().unwrap().charge = 0.0;
        a.chargemode = false;
    }
    s.run_until("the client sees the far light in emergency mode", |s| {
        s.client_light(far)
            .flatten()
            .is_some_and(|l| l.color == [0xFF, 0x32, 0x32] && l.range == 2.0)
    });
    println!("outage: {} light bytes", s.light_bytes() - before);
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

#[test]
#[ignore = "needs generated assets and MetaStation; run with --ignored"]
fn metastation() {
    let mut s = Session::start(metastation_map());
    let report = s.server.world().resource::<PowerReport>().clone();
    println!("MetaStation power: {report:#?}");
    assert!(report.apcs > 100 && report.nets > 1);
    assert!(report.areas_with_apc * 10 >= report.areas_needing_power * 9);

    // Supply and demand after a few machine ticks.
    s.hold(8.0, "", |_| true);
    let world = s.server.world();
    let grid = world.resource::<Grid>();
    let (mut avail, mut load) = (0.0, 0.0);
    for (_, net) in grid.nets() {
        avail += net.avail;
        load += net.load;
    }
    let mut apcs = s.server.world_mut().query::<&Apc>();
    let apcs: Vec<Apc> = apcs.iter(s.server.world()).cloned().collect();
    let charging = apcs.iter().filter(|a| a.charging == 1).count();
    let full = apcs.iter().filter(|a| a.charging == 2).count();
    let no_power = apcs.iter().filter(|a| a.main_status == 0).count();
    let mut smes = s.server.world_mut().query::<&Smes>();
    let smes: Vec<String> = smes
        .iter(s.server.world())
        .map(|s| {
            format!(
                "{:.1}/{:.1} MJ out {} kW{}",
                s.charge / 1e6,
                s.capacity / 1e6,
                s.output_level / 1e3,
                if s.outputting { "" } else { " (off)" }
            )
        })
        .collect();
    println!(
        "grid per 2 s tick: {:.1} kW available, {:.1} kW load; APCs: {charging} charging, {full} \
         full, {no_power} without grid power; SMES: {smes:?}",
        avail / 2000.0,
        load / 2000.0,
    );
    assert!(avail > 0.0 && load > 0.0);

    // Outage on a real area: an APC whose area has a working light and an airlock anyone opens.
    let mut q = s.server.world_mut().query::<(Entity, &Apc)>();
    let candidates: Vec<(Entity, Entity)> =
        q.iter(s.server.world()).map(|(e, a)| (e, a.area)).collect();
    let world = s.server.world();
    let mut doors = world
        .try_query::<(Entity, &Door, &PowerMachine, &TilePos)>()
        .unwrap();
    let mut lights = world.try_query::<(Entity, &LightFixture)>().unwrap();
    let (apc, area, door, light) = candidates
        .into_iter()
        .find_map(|(apc, area)| {
            let door = doors.iter(world).find(|(_, d, m, _)| {
                m.area == area
                    && d.kind == DoorKind::Airlock
                    && d.req_access.is_empty()
                    && d.req_one_access.is_empty()
                    && !d.locked
                    && !d.welded
                    && d.state == DoorState::Closed
            })?;
            let light = lights.iter(world).find(|(_, l)| {
                l.area == area && l.mode == LightMode::On && l.emergency.is_some()
            })?;
            Some((apc, area, door.0, light.0))
        })
        .expect("an area with an open-access airlock and a light");
    let door_pos = *s.get::<TilePos>(door);
    println!(
        "outage: area {:?} (APC {apc:?}), airlock at {door_pos:?}, light at {:?}",
        s.server
            .world()
            .get::<ss13_mapload::DmName>(area)
            .map(|n| n.name.clone()),
        s.get::<TilePos>(light)
    );
    {
        let mut a = s.apc_mut(apc);
        a.cell.as_mut().unwrap().charge = 0.0;
        a.chargemode = false;
    }
    s.run_until("the area loses power", |s| {
        s.get::<AreaPower>(area).power == [false; 3]
    });
    // Next to the airlock, on a free tile; the client must see both.
    let beside = Dir::CARDINALS
        .into_iter()
        .map(|d| door_pos.step(d))
        .find(|&p| {
            let world = s.server.world();
            world.resource::<SpatialIndex>().dense_on_tile(p).is_empty()
        })
        .expect("a free tile beside the airlock");
    s.st.server.world_mut().entity_mut(s.mob).insert(beside);
    s.run_until("the client sees the light in emergency mode", |s| {
        s.client_light(light)
            .flatten()
            .is_some_and(|l| l.color == [0xFF, 0x32, 0x32])
    });
    s.run_until("the airlock is replicated", |s| s.net_id(door).is_some());
    s.click(door);
    s.hold(1.5, "the unpowered airlock stays shut", |s| {
        s.door_state(door) == DoorState::Closed
    });
    println!("outage ok: lights in emergency mode, airlock shut");
}

/// Light block bytes on MetaStation: at join, for an outage of one area next to the player, and
/// for a station-wide outage, against what sending the whole z-level would cost.
/// `cargo test -p ss13_game --all-features --test it power::metastation_light_bytes -- --ignored --nocapture`
#[test]
#[ignore = "needs generated assets and MetaStation; run with --ignored"]
fn metastation_light_bytes() {
    let mut s = Session::start(metastation_map());
    s.hold(1.0, "", |_| true);
    let blocks = s.client().world().resource::<LightBlocks>().len();
    let known = s.client().world().resource::<LightBlocks>().sources();
    let join = s.light_bytes();
    // The whole level as one message: every source (~20 B) and a 1-bit opacity map.
    let z = s.pos().z;
    let mut q = s.server.world_mut().query::<(&TilePos, &Optics)>();
    let (mut level_sources, mut level_light_bytes) = (0, 0);
    for (pos, o) in q.iter(s.server.world()) {
        if let (true, Some(light)) = (pos.z == z, o.light) {
            level_sources += 1;
            level_light_bytes += postcard::to_stdvec(&light).unwrap().len() + 2;
        }
    }
    let (w, h) = s
        .server
        .world()
        .resource::<SpatialIndex>()
        .level_size(z)
        .unwrap();
    let bitmap = (w * h).div_ceil(8) as usize;
    println!(
        "join: {blocks} light blocks, {known} sources, {join} B (whole z-level: {level_sources} \
         sources, ~{level_light_bytes} B + {bitmap} B opacity)"
    );

    // One area out, next to the player: the area of the nearest APC.
    let me = s.pos();
    let mut q = s.server.world_mut().query::<(Entity, &Apc, &TilePos)>();
    let (apc, area) = q
        .iter(s.server.world())
        .filter(|(_, _, p)| p.z == me.z)
        .min_by_key(|(_, _, p)| p.chebyshev(me).unwrap())
        .map(|(e, a, _)| (e, a.area))
        .unwrap();
    let mut lights = s.server.world_mut().query::<&LightFixture>();
    let area_lights = lights
        .iter(s.server.world())
        .filter(|l| l.area == area && l.mode == LightMode::On)
        .count();
    let before = s.light_bytes();
    {
        let mut a = s.apc_mut(apc);
        a.cell.as_mut().unwrap().charge = 0.0;
        a.chargemode = false;
    }
    s.run_until("the area loses power", |s| {
        s.get::<AreaPower>(area).power == [false; 3]
    });
    s.hold(1.0, "", |_| true);
    println!(
        "area outage ({area_lights} lights on before): {} B to this client",
        s.light_bytes() - before
    );

    // Every APC drained at once.
    let mut q = s.server.world_mut().query::<&mut Apc>();
    for mut a in q.iter_mut(s.server.world_mut()) {
        if let Some(cell) = a.cell.as_mut() {
            cell.charge = 0.0;
        }
        a.chargemode = false;
    }
    let before = s.light_bytes();
    s.hold(6.0, "", |_| true);
    let mut lights = s.server.world_mut().query::<&LightFixture>();
    let on = lights
        .iter(s.server.world())
        .filter(|l| l.mode == LightMode::On)
        .count();
    let total = lights.iter(s.server.world()).count();
    println!(
        "station-wide outage: {} B to this client ({} of {total} fixtures still on); a whole-level \
         delta would send every changed fixture to every client (~{} B each)",
        s.light_bytes() - before,
        on,
        (total - on) * 22
    );
}

/// Random events on the fixture grid (`ss13_game::events`), forced as an admin would: Grid
/// Check fails the APC for 1-4 minutes (lights to emergency, the airlock unpowered) until its
/// `failure_timer` runs out and power comes back by itself; an Electrical Storm then breaks
/// both light tubes for good.
#[test]
fn grid_check_and_electrical_storm() {
    let assets = fixture_assets("events");
    let mut s = Session::start(MapLoadPlugin {
        source: assets.clone(),
        maps: vec!["map.dmm".into()],
        assets,
    });
    let (apc, door) = (s.find::<Apc>(APC), s.find::<Door>(DOOR));
    let (light, bare) = (
        s.find::<LightFixture>(LIGHT),
        s.find::<LightFixture>(BARE_LIGHT),
    );
    let area = s.get::<Apc>(apc).area;
    s.hold(3.0, "powered at start", |s| {
        s.get::<AreaPower>(area).power == [true; 3]
    });

    ss13_game::events::force_event(s.server.world_mut(), "Grid Check", None).unwrap();
    s.run_until("the grid check fails the APC", |s| {
        s.get::<AreaPower>(area).power == [false; 3]
    });
    let timer = s.get::<Apc>(apc).failure_timer;
    assert!((58.0..=240.0).contains(&timer), "failure_timer {timer}");
    assert!(s.server.world().get::<Unpowered>(door).is_some());
    assert_eq!(s.get::<LightFixture>(light).mode, LightMode::Emergency);
    s.run_until("the client sees the emergency light", |s| {
        s.client_light(light)
            .flatten()
            .is_some_and(|l| l.color == [0xFF, 0x32, 0x32])
    });
    let occurrences = |s: &Session| {
        let events = s.server.world().resource::<ss13_game::events::Events>();
        events.controls[events.index("Grid Check").unwrap()].occurrences
    };
    assert_eq!(occurrences(&s), 1);

    // Up to 4 minutes: one simulated second per frame on the server.
    s.server
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs(1)));
    s.run_until_for(300.0, "power comes back by itself", |s| {
        s.get::<AreaPower>(area).power == [true; 3]
    });
    assert!(s.server.world().get::<Unpowered>(door).is_none());
    s.server
        .insert_resource(TimeUpdateStrategy::ManualDuration(testing::TICK));
    s.run_until("the client sees the light on again", |s| {
        s.client_light(light)
            .flatten()
            .is_some_and(|l| l.range == 8.0)
    });

    ss13_game::events::force_event(s.server.world_mut(), "Electrical Storm", None).unwrap();
    s.run_until("the storm breaks both tubes", |s| {
        s.get::<LightFixture>(light).on_light.is_none()
            && s.get::<LightFixture>(bare).on_light.is_none()
    });
    assert_eq!(s.get::<LightFixture>(light).mode, LightMode::Off);
    assert_eq!(
        s.get::<AreaPower>(area).static_power[1],
        0.0,
        "no lighting load"
    );
    s.run_until("the client sees both fixtures dark", |s| {
        s.client_light(light) == Some(None) && s.client_light(bare) == Some(None)
    });
}
