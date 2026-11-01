//! Power generation over real WebTransport on 127.0.0.1: a fixture engine room (an RTG feeding
//! an emitter aimed at a supermatter crystal, a tesla coil and a grounding rod beside it, the
//! coil's net charging a SMES) and a solar array with its control console.
//!
//! - The client turns the emitter on with a click: beams feed the crystal, its arcs charge the
//!   coil, the coil's net carries more than the RTG makes and the SMES takes it.
//! - The client opens `SolarControl` with a click, scans for hardware and turns the panel away
//!   from the sun: output follows Lambert's cosine; a wall between the panel and the sun blocks it.
//! - A crystal pushed past its explosion point counts down and delaminates.
//!
//! `cargo test -p ss13_game --all-features --test it engine::metastation -- --ignored --nocapture`
//! runs MetaStation's engine and solars and prints the generation numbers (needs generated
//! assets in `SS13_ASSETS`).

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::combat::CombatRng;
use ss13_game::power::accumulator::Accumulator;
use ss13_game::power::emitter::Emitter;
use ss13_game::power::solar::{SolarControl, SolarPanel, Sun};
use ss13_game::power::supermatter::{Delaminated, Supermatter};
use ss13_game::power::{EngineRng, Grid, Smes};
use ss13_game::testing::{self, Drive, steps, write_fixture};
use ss13_mapload::MapLoadPlugin;
use ss13_net::ui::client::UiMessageReceived;
use ss13_shared::TilePos;
use ss13_shared::protocol::ClientIntent;
use ss13_shared::ui::UiServerMessage;
use ss13_tgui::interfaces::solar_control::{SolarControlData, SolarControlUi};
use ss13_tgui::{UiInterface, decode};
use ss13_world::SpatialIndex;

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/datum/armor": { "parent": "/datum", "declared_vars": { "melee": { "expr": "0" } } },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 }, "declared_vars": { "light_power": 1,
        "can_atmos_pass": 1, "flags_1": 0, "max_integrity": 500,
        "armor_type": { "expr": "/datum/armor" }, "integrity_failure": 0,
        "damage_deflection": 0, "resistance_flags": 0 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3, "anchored": 0 } },
    "/obj": { "parent": "/atom/movable", "declared_vars": { "obj_flags": 2 } },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start" },
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
        "declared_vars": { "charge": 1000000, "input_level": 200000, "output_level": 5000 } },
    "/obj/machinery/power/terminal": { "parent": "/obj/machinery/power", "declared_vars": {
        "icon": { "resource": "c.dmi" }, "icon_state": "term" } },
    "/obj/machinery/power/rtg": { "parent": "/obj/machinery/power", "declared_vars": {
        "icon": { "resource": "smes.dmi" }, "icon_state": "rtg", "power_gen": 1000 } },
    "/obj/machinery/power/emitter": { "parent": "/obj/machinery/power", "declared_vars": {
        "icon": { "resource": "e.dmi" }, "icon_state": "emitter", "density": 1, "anchored": 0 } },
    "/obj/machinery/power/emitter/welded": { "parent": "/obj/machinery/power/emitter" },
    "/obj/machinery/power/supermatter_crystal": { "parent": "/obj/machinery/power",
        "declared_vars": { "name": "supermatter crystal", "icon": { "resource": "sm.dmi" },
        "icon_state": "sm", "density": 1, "light_range": 1.4, "light_power": 2,
        "light_color": "#ffd04f", "light_on": 1 } },
    "/obj/machinery/power/supermatter_crystal/engine": { "parent": "/obj/machinery/power/supermatter_crystal" },
    "/obj/machinery/power/energy_accumulator": { "parent": "/obj/machinery/power",
        "declared_vars": { "density": 1, "anchored": 0, "icon": { "resource": "coil.dmi" } } },
    "/obj/machinery/power/energy_accumulator/tesla_coil": { "parent": "/obj/machinery/power/energy_accumulator",
        "declared_vars": { "icon_state": "coil0" } },
    "/obj/machinery/power/energy_accumulator/tesla_coil/anchored": { "parent": "/obj/machinery/power/energy_accumulator/tesla_coil",
        "declared_vars": { "anchored": 1 } },
    "/obj/machinery/power/energy_accumulator/grounding_rod": { "parent": "/obj/machinery/power/energy_accumulator",
        "declared_vars": { "icon_state": "grounding_rod0" } },
    "/obj/machinery/power/energy_accumulator/grounding_rod/anchored": { "parent": "/obj/machinery/power/energy_accumulator/grounding_rod",
        "declared_vars": { "anchored": 1 } },
    "/obj/machinery/power/solar": { "parent": "/obj/machinery/power", "declared_vars": {
        "icon": { "resource": "solar.dmi" }, "icon_state": "sp_base", "density": 1 } },
    "/obj/machinery/power/tracker": { "parent": "/obj/machinery/power", "declared_vars": {
        "icon": { "resource": "solar.dmi" }, "icon_state": "tracker_base", "density": 1 } },
    "/obj/machinery/power/solar_control": { "parent": "/obj/machinery/power", "declared_vars": {
        "name": "solar panel control", "icon": { "resource": "comp.dmi" },
        "icon_state": "computer", "density": 1, "use_power": 1, "idle_power_usage": 100 } },
    "/obj/structure": { "parent": "/obj", "declared_vars": { "anchored": 1 } },
    "/obj/structure/cable": { "parent": "/obj/structure", "declared_vars": {
        "icon": { "resource": "c.dmi" }, "icon_state": "l2-1-2-4-8-node", "cable_layer": 2 } },
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
    "/turf/open/floor": { "parent": "/turf/open", "declared_vars": { "icon_state": "floor" } },
    "/turf/open/space": { "parent": "/turf/open", "declared_vars": { "temperature": 2.7,
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

/// Rows top-down, y = 1 at the bottom. Second row: the RTG `R` and the SMES terminal `T`
/// (facing east) on the engine net, the SMES `S` feeding the APC `A`; the solar control `C`
/// wired through the wall to the panel `P` and tracker `K` out in space (open to the north, the
/// station wall to the west).
/// Column 2 carries the engine net down to the emitter `E` (facing east, welded) and along row 6
/// to the tesla coil `Q`; the crystal `X` sits in the emitter's line, the grounding rod `G`
/// below it. The player spawns at `s`, next to the emitter.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/station)
"f" = (/turf/open/floor,/area/station)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/station)
"c" = (/obj/structure/cable,/turf/open/floor,/area/station)
"R" = (/obj/structure/cable,/obj/machinery/power/rtg,/turf/open/floor,/area/station)
"T" = (/obj/structure/cable,/obj/machinery/power/terminal{dir = 4},/turf/open/floor,/area/station)
"S" = (/obj/structure/cable,/obj/machinery/power/smes/engineering,/turf/open/floor,/area/station)
"A" = (/obj/structure/cable,/obj/machinery/power/apc{dir = 1; pixel_y = 25},/turf/open/floor,/area/station)
"P" = (/obj/structure/cable,/obj/machinery/power/solar,/turf/open/space/basic,/area/space)
"K" = (/obj/structure/cable,/obj/machinery/power/tracker,/turf/open/space/basic,/area/space)
"C" = (/obj/structure/cable,/obj/machinery/power/solar_control,/turf/open/floor,/area/station)
"E" = (/obj/structure/cable,/obj/machinery/power/emitter/welded{dir = 4},/turf/open/floor,/area/station)
"Q" = (/obj/structure/cable,/obj/machinery/power/energy_accumulator/tesla_coil/anchored,/turf/open/floor,/area/station)
"G" = (/obj/machinery/power/energy_accumulator/grounding_rod/anchored,/turf/open/floor,/area/station)
"X" = (/obj/machinery/power/supermatter_crystal/engine,/turf/open/floor,/area/station)
"W" = (/obj/structure/cable,/turf/closed/wall,/area/station)
"." = (/turf/open/space/basic,/area/space)
"," = (/obj/structure/cable,/turf/open/space/basic,/area/space)

(1,1,1) = {"
wwwwwwwwwwwwwwww....
wRcTScAfffffffCW,PK.
wcfffffffffffffw....
wcccccccQffffffw....
wcsffffffffffffw....
wcEffffXfffffffw....
wffffffffffffffw....
wffffffGfffffffw....
wwwwwwwwwwwwwwww....
"}
"##;

const EMITTER: TilePos = TilePos::new(3, 4, 1);
const CRYSTAL: TilePos = TilePos::new(8, 4, 1);
const COIL: TilePos = TilePos::new(9, 6, 1);
const ROD: TilePos = TilePos::new(8, 2, 1);
const SMES: TilePos = TilePos::new(5, 8, 1);
const PANEL: TilePos = TilePos::new(18, 8, 1);
const CONTROL: TilePos = TilePos::new(15, 8, 1);

fn states() -> Vec<(String, String)> {
    let mut states: Vec<(String, String)> = [
        ("t.dmi", "floor"),
        ("t.dmi", "wall"),
        ("t.dmi", ""),
        ("l.dmi", "x2"),
        ("h.dmi", "human_basic"),
        ("apc.dmi", "apc0"),
        ("smes.dmi", "smes"),
        ("smes.dmi", "rtg"),
        ("c.dmi", "term"),
        ("c.dmi", "l2-1-2-4-8-node"),
        ("e.dmi", "emitter"),
        ("e.dmi", "emitter_overlay"),
        ("sm.dmi", "sm"),
        ("sm.dmi", "sm-glow"),
        ("sm.dmi", "causality_field"),
        ("coil.dmi", "coil0"),
        ("coil.dmi", "grounding_rod0"),
        ("solar.dmi", "sp_base"),
        ("solar.dmi", "tracker_base"),
        ("comp.dmi", "computer"),
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

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("engine_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/machinery/power/apc",
        "/obj/machinery/power/smes/engineering",
        "/obj/machinery/power/terminal",
        "/obj/machinery/power/rtg",
        "/obj/machinery/power/emitter/welded",
        "/obj/machinery/power/supermatter_crystal/engine",
        "/obj/machinery/power/energy_accumulator/tesla_coil/anchored",
        "/obj/machinery/power/energy_accumulator/grounding_rod/anchored",
        "/obj/machinery/power/solar",
        "/obj/machinery/power/tracker",
        "/obj/machinery/power/solar_control",
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

/// The client's `SolarControl` window: id and last data.
#[derive(Resource, Default)]
struct SolarWindow {
    id: Option<u32>,
    data: Option<SolarControlData>,
}

fn record_window(mut received: MessageReader<UiMessageReceived>, mut w: ResMut<SolarWindow>) {
    for UiMessageReceived(message) in received.read() {
        match message {
            UiServerMessage::Open {
                window,
                interface,
                data,
                ..
            } if interface == SolarControlUi::NAME => {
                w.id = Some(*window);
                w.data = Some(decode(data).unwrap());
            }
            UiServerMessage::Update {
                window,
                data: Some(data),
                ..
            } if w.id == Some(*window) => w.data = Some(decode(data).unwrap()),
            _ => {}
        }
    }
}

/// A started server and one joined client, on game time.
struct Session {
    st: testing::Station,
    mob: Entity,
}

ss13_game::drive_station!(Session, st);

impl Session {
    fn start(map: MapLoadPlugin) -> Self {
        let mut st = testing::Station::start(map, |app| {
            app.add_plugins(ss13_atmos::AtmosPlugin);
        });
        // A fixed engine seed, after `finish` reseeded it from the map seed: the arcs' pick of
        // the coil or the rod stays the one this test was written against.
        st.server.insert_resource(EngineRng(CombatRng(0x5eed)));
        let p = st.join("a", |client| {
            client
                .init_resource::<SolarWindow>()
                .add_systems(Update, record_window);
        });
        let mob = st.players[p].mob;
        Self { st, mob }
    }

    /// Steps until `done` holds; fails after `secs` of game time. For server-side timing only:
    /// a wait on the client's network goes through `run_until_for`.
    fn within(&mut self, secs: f64, what: &str, mut done: impl FnMut(&mut Self) -> bool) {
        let mut frames = 0;
        while !done(self) {
            assert!(frames < steps(secs), "timed out: {what}");
            frames += 1;
            self.step();
        }
    }

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

    fn send(&mut self, intent: ClientIntent) {
        self.st.send(0, intent);
    }

    fn click(&mut self, e: Entity) {
        self.run_until_for(5.0, "the target is replicated", |s| {
            s.net_id(0, e).is_some()
        });
        self.st.click(0, e);
    }

    fn act(&mut self, action: &str, params: &[(&str, &str)]) {
        let window = self.players[0]
            .app
            .world()
            .resource::<SolarWindow>()
            .id
            .expect("open");
        self.send(ClientIntent::UiAct {
            window,
            action: action.into(),
            params: params
                .iter()
                .map(|&(k, v)| (k.to_owned(), v.to_owned()))
                .collect(),
        });
    }

    fn net_avail(&self, e: Entity) -> f64 {
        let grid = self.server.world().resource::<Grid>();
        grid.node_net(e)
            .and_then(|n| grid.net(n))
            .map_or(0.0, |n| n.avail)
    }

    fn solar_data(&self) -> Option<SolarControlData> {
        self.players[0]
            .app
            .world()
            .resource::<SolarWindow>()
            .data
            .clone()
    }
}

fn fixture(test: &str) -> Session {
    let assets = fixture_assets(test);
    Session::start(MapLoadPlugin {
        source: assets.clone(),
        maps: vec!["map.dmm".into()],
        assets,
    })
}

#[test]
fn emitter_feeds_the_crystal_and_the_coil_charges_the_smes() {
    let mut s = fixture("engine");
    let emitter = s.find::<Emitter>(EMITTER);
    let crystal = s.find::<Supermatter>(CRYSTAL);
    let coil = s.find::<Accumulator>(COIL);
    let rod = s.find::<Accumulator>(ROD);
    let smes = s.find::<Smes>(SMES);
    assert!(s.get::<Emitter>(emitter).welded);
    assert!(
        s.server
            .world()
            .resource::<Grid>()
            .node_net(emitter)
            .is_some()
    );
    assert_eq!(
        s.server.world().resource::<Grid>().node_net(emitter),
        s.server.world().resource::<Grid>().node_net(coil),
        "the emitter and the coil share the engine net"
    );

    // Idle engine: only the RTG feeds the engine net (1 kW = 2 kJ per tick).
    s.run_for(6.0);
    assert_eq!(s.get::<Supermatter>(crystal).crystal.internal_energy, 0.0);
    assert!((s.net_avail(coil) - 2000.0).abs() < 1.0);
    let charge_before = s.get::<Smes>(smes).charge;

    // The player turns the emitter on.
    s.click(emitter);
    s.run_until_for(2.0, "the emitter is on", |s| {
        s.get::<Emitter>(emitter).active
    });
    s.within(10.0, "a beam feeds the crystal", |s| {
        s.get::<Supermatter>(crystal).crystal.internal_energy > 0.0
    });
    // Arcs under 240 kJ fizzle: the accumulated energy has to build up first.
    s.within(60.0, "an arc charges the coil", |s| {
        s.get::<Accumulator>(coil).processed_energy > 0.0
    });
    s.run_for(20.0);
    let sm = s.get::<Supermatter>(crystal).crystal.clone();
    let coil_now = *s.get::<Accumulator>(coil);
    let rod_now = *s.get::<Accumulator>(rod);
    let avail = s.net_avail(coil);
    let smes_now = s.get::<Smes>(smes).clone();
    println!(
        "after 20 s: EER {:.1} MeV, damage {:.3}; coil released {:.0} J/tick ({:.0} J stored), \
         rod {:.0} J stored; engine net {:.1} kW; SMES {:.0} -> {:.0} J, input {:.1} kW",
        sm.internal_energy,
        sm.damage,
        coil_now.processed_energy,
        coil_now.stored_energy,
        rod_now.stored_energy,
        avail / 2000.0,
        charge_before,
        smes_now.charge,
        smes_now.input_available / 2000.0,
    );
    assert!(sm.internal_energy > 100.0, "hits accumulate: {sm:?}");
    assert_eq!(sm.damage, 0.0, "room air keeps it cool");
    assert!(rod_now.stored_energy > 0.0 || rod_now.processed_energy > 0.0);
    // The engine net carries far more than the RTG's 2 kJ per tick, and the SMES takes it.
    assert!(avail > 10_000.0, "engine net {avail} J/tick");
    assert!(smes_now.charge > charge_before);

    // Off again: the crystal decays, beams stop.
    s.click(emitter);
    s.run_until_for(2.0, "the emitter is off", |s| {
        !s.get::<Emitter>(emitter).active
    });
    let e_off = s.get::<Supermatter>(crystal).crystal.internal_energy;
    s.run_for(10.0);
    assert!(s.get::<Supermatter>(crystal).crystal.internal_energy < e_off);
}

#[test]
fn solar_control_links_turns_and_the_station_shades() {
    let mut s = fixture("solar");
    // A fixed sun due north, moving east.
    s.server.world_mut().insert_resource(Sun::new(0.0, 1.0));
    let panel = s.find::<SolarPanel>(PANEL);
    let control = s.find::<SolarControl>(CONTROL);

    // Unlinked panels face north from the start: full sun, 2500 W = 5 kJ per tick.
    s.run_for(5.0);
    assert_eq!(s.get::<SolarPanel>(panel).sunfrac, 1.0);
    assert!((s.net_avail(panel) - 5000.0).abs() < 1.0);

    // The player walks up to the console (teleported next to it) and opens the window.
    s.st.server
        .world_mut()
        .entity_mut(s.mob)
        .insert(TilePos::new(15, 7, 1));
    s.run_for(0.5);
    s.click(control);
    s.run_until_for(3.0, "the SolarControl window opens", |s| {
        s.solar_data().is_some()
    });
    let data = s.solar_data().unwrap();
    assert_eq!(data.connected_panels, 0);
    assert!(!data.connected_tracker);

    s.act("refresh", &[]);
    s.run_until_for(3.0, "the panel and the tracker are linked", |s| {
        s.solar_data()
            .is_some_and(|d| d.connected_panels == 1 && d.connected_tracker && d.capacity == 2500.0)
    });

    // Turned to 60°: half the light (Lambert), then the supply readout follows.
    s.act("azimuth", &[("value", "60")]);
    s.run_until_for(5.0, "the panel turns", |s| {
        s.get::<SolarPanel>(panel).azimuth_current == 60.0
    });
    s.run_for(4.5);
    assert_eq!(s.get::<SolarPanel>(panel).sunfrac, 0.5);
    assert!((s.net_avail(panel) - 2500.0).abs() < 1.0);
    s.run_until_for(3.0, "the window shows 1250 W", |s| {
        s.solar_data().is_some_and(|d| d.supply == 1250.0)
    });

    // Sun and panel due west: the station wall shades the panel on the next turn.
    s.server.world_mut().insert_resource(Sun::new(270.0, 1.0));
    s.act("azimuth", &[("value", "270")]);
    s.run_until_for(5.0, "the panel turns west", |s| {
        s.get::<SolarPanel>(panel).azimuth_current == 270.0
    });
    s.run_for(4.5);
    assert!(s.get::<SolarPanel>(panel).obscured);
    assert_eq!(s.net_avail(panel), 0.0);

    // Auto tracking follows the tracker; bad modes are refused.
    s.act("tracking", &[("mode", "7")]);
    s.act("tracking", &[("mode", "2")]);
    s.run_until_for(3.0, "auto tracking", |s| {
        s.get::<SolarControl>(control).track == 2
    });
}

#[test]
fn crystal_past_its_explosion_point_delaminates() {
    let mut s = fixture("delam");
    let crystal = s.find::<Supermatter>(CRYSTAL);
    s.server
        .world_mut()
        .get_mut::<Supermatter>(crystal)
        .unwrap()
        .crystal
        .damage = 120.0;
    s.within(3.0, "the countdown starts", |s| {
        s.get::<Supermatter>(crystal).crystal.final_countdown
    });
    // 15 s of countdown, then one more second.
    s.run_for(14.0);
    assert!(s.server.world().get_entity(crystal).is_ok());
    let mut seen = false;
    s.within(4.0, "the crystal delaminates", |s| {
        let messages = s.server.world().resource::<Messages<Delaminated>>();
        seen |= messages.iter_current_update_messages().next().is_some();
        seen
    });
    s.run_for(0.2);
    assert!(s.server.world().get_entity(crystal).is_err());
}

/// Joins every panel net within 30 tiles of each console to the console's net with new cable
/// along BFS paths over non-wall tiles. Returns (distinct panel nets before, cables laid).
fn wire_arrays(world: &mut World, controls: &[(Entity, TilePos)]) -> (usize, usize) {
    use std::collections::{HashMap, HashSet, VecDeque};
    let mut q = world.query::<(Entity, &TilePos, &SolarPanel)>();
    let panels: Vec<(Entity, TilePos)> = q.iter(world).map(|(e, &p, _)| (e, p)).collect();
    let mut walls = HashSet::new();
    let mut q =
        world.query_filtered::<&TilePos, (With<ss13_mapload::Turf>, With<ss13_world::Density>)>();
    for &p in q.iter(world) {
        walls.insert(p);
    }
    let nets_before = {
        let grid = world.resource::<Grid>();
        let mut nets: Vec<_> = panels
            .iter()
            .filter_map(|&(e, _)| grid.node_net(e))
            .collect();
        nets.sort_unstable();
        nets.dedup();
        nets.len()
    };
    let mut laid = 0;
    for &(control, cpos) in controls {
        loop {
            let grid = world.resource::<Grid>();
            let Some(net) = grid.node_net(control) else {
                break;
            };
            // Tiles of the console's net and of the nearby unjoined panels' nets.
            let index = world.resource::<SpatialIndex>();
            let cable_tiles = |n| -> Vec<TilePos> {
                let mut out = Vec::new();
                for y in (cpos.y - 30)..=(cpos.y + 30) {
                    for x in (cpos.x - 30)..=(cpos.x + 30) {
                        let p = TilePos::new(x, y, cpos.z);
                        if index
                            .on_tile(p)
                            .iter()
                            .any(|&e| grid.cable_net(e) == Some(n))
                        {
                            out.push(p);
                        }
                    }
                }
                out
            };
            let targets: HashSet<_> = panels
                .iter()
                .filter(|(_, p)| p.chebyshev(cpos).is_some_and(|d| d <= 30))
                .filter_map(|&(e, _)| grid.node_net(e))
                .filter(|&n| n != net)
                .collect();
            if targets.is_empty() {
                break;
            }
            let goal: HashSet<TilePos> = targets.iter().flat_map(|&n| cable_tiles(n)).collect();
            let mut prev: HashMap<TilePos, TilePos> = HashMap::new();
            let mut queue: VecDeque<TilePos> = cable_tiles(net).into();
            let mut seen: HashSet<TilePos> = queue.iter().copied().collect();
            let mut found = None;
            while let Some(p) = queue.pop_front() {
                if goal.contains(&p) {
                    found = Some(p);
                    break;
                }
                for d in ss13_shared::Dir::CARDINALS {
                    let n = p.step(d);
                    if n.chebyshev(cpos).is_some_and(|d| d <= 32)
                        && !walls.contains(&n)
                        && seen.insert(n)
                    {
                        prev.insert(n, p);
                        queue.push_back(n);
                    }
                }
            }
            let Some(mut p) = found else {
                break;
            };
            let mut path = Vec::new();
            while let Some(&q) = prev.get(&p) {
                path.push(q);
                p = q;
            }
            // The last element is a tile of the console's net already.
            path.pop();
            if path.is_empty() {
                break;
            }
            for tile in path {
                let e = world.spawn(tile).id();
                world.resource_mut::<Grid>().add_cable(e, tile, 2);
                laid += 1;
            }
        }
    }
    (nets_before, laid)
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

/// MetaStation's generation: solar arrays linked and auto-tracking, the three engine emitters
/// on; prints supply and SMES charge over simulated minutes.
#[test]
#[ignore = "needs generated assets and MetaStation; run with --ignored"]
fn metastation() {
    let mut s = Session::start(metastation_map());
    s.server.world_mut().insert_resource(Sun::new(45.0, 1.0));
    let world = s.server.world_mut();
    let mut q = world.query::<(Entity, &TilePos, &Emitter)>();
    let emitters: Vec<(Entity, TilePos, bool)> =
        q.iter(world).map(|(e, &p, em)| (e, p, em.welded)).collect();
    let mut q = world.query::<(Entity, &TilePos, &SolarControl)>();
    let controls: Vec<(Entity, TilePos)> = q.iter(world).map(|(e, &p, _)| (e, p)).collect();
    let mut q = world.query::<(Entity, &TilePos, &Supermatter)>();
    let (crystal, crystal_pos) = q.iter(world).map(|(e, &p, _)| (e, p)).next().unwrap();
    println!("emitters: {emitters:?}\nsolar controls: {controls:?}\ncrystal at {crystal_pos:?}");

    // MetaStation's arrays come unwired, as in tg: the runs stop short of the consoles and the
    // panel rows are split. Lay the missing cable an engineer would (shortest path over
    // non-wall tiles from each console's net to every panel net within 30 tiles).
    let (nets_before, wired) = wire_arrays(s.server.world_mut(), &controls);
    println!(
        "solar arrays: panels in {nets_before} nets at roundstart, none on a console's net; laid {wired} cable pieces"
    );
    // Every solar control scans and tracks automatically (what an engineer does at roundstart).
    for &(c, _) in &controls {
        ss13_game::power::solar::apply_act(
            s.server.world_mut(),
            c,
            ss13_game::power::solar::SolarAct::Refresh,
        );
        ss13_game::power::solar::apply_act(
            s.server.world_mut(),
            c,
            ss13_game::power::solar::SolarAct::Tracking(2),
        );
    }
    // The engine SMES inputs go to their 200 kW maximum (the SMES window is not ported; tg
    // engineers do this at roundstart: the map's 50 kW input is below their 90 kW output).
    {
        let world = s.server.world_mut();
        let mut q = world.query::<(&TilePos, &mut Smes)>();
        for (p, mut smes) in q.iter_mut(world) {
            if p.y == 148 && (152..=154).contains(&p.x) {
                smes.input_level = 200_000.0;
            }
        }
    }
    // The welded engine emitters go on.
    for &(e, _, welded) in &emitters {
        if welded {
            let wired = s.server.world().resource::<Grid>().node_net(e).is_some();
            s.server
                .world_mut()
                .get_mut::<Emitter>(e)
                .unwrap()
                .toggle(wired);
        }
    }

    let smes_report = |s: &Session| -> Vec<String> {
        let world = s.server.world();
        let mut q = world.try_query::<(&TilePos, &Smes)>().unwrap();
        q.iter(world)
            .map(|(p, m)| {
                format!(
                    "({},{}) {:.2}/{:.0} MJ in {:.1} kW out {:.1} kW",
                    p.x,
                    p.y,
                    m.charge / 1e6,
                    m.capacity / 1e6,
                    m.input_available / 2000.0,
                    m.output_used / 2000.0
                )
            })
            .collect()
    };
    let start: Vec<f64> = {
        let world = s.server.world();
        let mut q = world.try_query::<&Smes>().unwrap();
        q.iter(world).map(|m| m.charge).collect()
    };
    for minute in 0..4 {
        s.run_for(60.0);
        let world = s.server.world();
        let solar: f64 = {
            let mut q = world.try_query::<&SolarControl>().unwrap();
            q.iter(world).map(|c| c.lastgen).sum()
        };
        let panels: usize = {
            let mut q = world.try_query::<&SolarControl>().unwrap();
            q.iter(world).map(|c| c.panels.len()).sum()
        };
        let coils: f64 = {
            let mut q = world.try_query::<&Accumulator>().unwrap();
            q.iter(world)
                .filter(|a| {
                    matches!(
                        a.kind,
                        ss13_game::power::accumulator::AccumulatorKind::TeslaCoil { .. }
                    )
                })
                .map(|a| a.processed_energy / 2.0)
                .sum()
        };
        let sm = world.get::<Supermatter>(crystal).map(|s| s.crystal.clone());
        let sun = world.resource::<Sun>().azimuth;
        println!(
            "t={}m sun {sun:.1}°: solars {:.1} kW from {panels} panels; coils {:.1} kW; EER {:.0} MeV, \
             damage {:.2}, gas {:.0} K; SMES {:?}",
            minute + 1,
            solar / 1000.0,
            coils / 1000.0,
            sm.as_ref().map_or(0.0, |c| c.internal_energy),
            sm.as_ref().map_or(0.0, |c| c.damage),
            sm.as_ref().map_or(0.0, |c| c.gas_temperature),
            smes_report(&s),
        );
    }
    let end: Vec<f64> = {
        let world = s.server.world();
        let mut q = world.try_query::<&Smes>().unwrap();
        q.iter(world).map(|m| m.charge).collect()
    };
    let delta: f64 = end.iter().zip(&start).map(|(e, s)| e - s).sum();
    println!("SMES total change over 4 min: {:+.2} MJ", delta / 1e6);
    let world = s.server.world();
    assert!(
        world
            .get::<Supermatter>(crystal)
            .unwrap()
            .crystal
            .internal_energy
            > 0.0
    );
}
