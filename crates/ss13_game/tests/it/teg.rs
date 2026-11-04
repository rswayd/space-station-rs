//! The thermoelectric generator over real WebTransport on 127.0.0.1: a fixture room with a
//! wired generator between a hot circulator (west, facing east) and a cold one (east, facing
//! west), as mappers place them.
//!
//! - At load the generator links its circulators; hot gas in one loop and cold gas in the other
//!   cross the circulators, swap heat and the generator feeds its powernet.
//! - The client opens `ThermoElectricGenerator` with a click and reads the output and both loops.
//! - A multitool flips the cold circulator to hot: the generator loses it and the window says so;
//!   the generator's multitool finds it again once it is cold. A circulator wrenched loose (panel
//!   open) leaves the pipe graph with its gas and comes back with it.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_atmos::gas::R_IDEAL_GAS_EQUATION;
use ss13_atmos::pipenet::Pipenet;
use ss13_atmos::{GasId, GasMixture};
use ss13_game::atmos_machinery::AtmosDevice;
use ss13_game::power::Grid;
use ss13_game::power::teg::{Circulator, Teg, tool_act};
use ss13_game::testing::{self, Drive, write_fixture};
use ss13_game::tools::ToolKind;
use ss13_mapload::MapLoadPlugin;
use ss13_net::ui::client::UiMessageReceived;
use ss13_shared::TilePos;
use ss13_shared::ui::UiServerMessage;
use ss13_tgui::interfaces::teg::{ThermoElectricGeneratorData, ThermoElectricGeneratorUi};
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
    "/obj/machinery": { "parent": "/obj", "declared_vars": { "anchored": 1,
        "use_power": 1, "idle_power_usage": 100, "active_power_usage": 1000, "power_channel": 1 } },
    "/obj/machinery/power": { "parent": "/obj/machinery", "declared_vars": {
        "use_power": 0, "idle_power_usage": 0, "active_power_usage": 0, "cable_layer": 2 } },
    "/obj/machinery/power/thermoelectric_generator": { "parent": "/obj/machinery/power",
        "declared_vars": { "name": "thermoelectric generator", "icon": { "resource": "o.dmi" },
        "icon_state": "teg", "density": 1 } },
    "/obj/machinery/atmospherics": { "parent": "/obj/machinery", "declared_vars": {
        "piping_layer": 3, "pipe_flags": 0, "device_type": 0, "initialize_directions": 0,
        "pipe_color": null, "power_channel": 3 } },
    "/obj/machinery/atmospherics/components": { "parent": "/obj/machinery/atmospherics" },
    "/obj/machinery/atmospherics/components/binary": {
        "parent": "/obj/machinery/atmospherics/components", "declared_vars": {
        "icon": { "resource": "b.dmi" }, "device_type": 2, "use_power": 1,
        "idle_power_usage": 25 } },
    "/obj/machinery/atmospherics/components/binary/circulator": {
        "parent": "/obj/machinery/atmospherics/components/binary", "declared_vars": {
        "name": "circulator/heat exchanger", "icon_state": "circ_base", "density": 1,
        "pipe_flags": 6 } },
    "/obj/machinery/atmospherics/components/binary/circulator/cold": {
        "parent": "/obj/machinery/atmospherics/components/binary/circulator" },
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
    "/turf/closed": { "parent": "/turf", "declared_vars": { "density": 1, "opacity": 1,
        "blocks_air": 1 } },
    "/turf/closed/wall": { "parent": "/turf/closed", "declared_vars": { "icon_state": "wall" } },
    "/area": { "parent": "/atom", "declared_vars": { "requires_power": 1, "always_unpowered": 0,
        "power_equip": 1, "power_light": 1, "power_environ": 1, "lightswitch": 1,
        "static_lighting": 1 } },
    "/area/station": { "parent": "/area" }
  },
  "element_calls": { "/datum/element/undertile": [
    { "type": "/obj/structure/cable", "proc": "Initialize", "call": "AddElement", "line": "" }
  ] }
}"##;

/// Rows top-down, y = 1 at the bottom: the hot circulator `H` (facing east), the generator `G`
/// on a cable, the cold circulator `C` (facing west); the player spawns at `s` below them.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/station)
"f" = (/turf/open/floor,/area/station)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/station)
"H" = (/obj/machinery/atmospherics/components/binary/circulator{dir = 4},/turf/open/floor,/area/station)
"G" = (/obj/structure/cable,/obj/machinery/power/thermoelectric_generator,/turf/open/floor,/area/station)
"C" = (/obj/machinery/atmospherics/components/binary/circulator/cold{dir = 8},/turf/open/floor,/area/station)

(1,1,1) = {"
wwwwwww
wfHGCfw
wffsffw
wwwwwww
"}
"##;

const HOT: TilePos = TilePos::new(3, 3, 1);
const GEN: TilePos = TilePos::new(4, 3, 1);
const COLD: TilePos = TilePos::new(5, 3, 1);

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("teg_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/machinery/power/thermoelectric_generator",
        "/obj/machinery/atmospherics/components/binary/circulator",
        "/obj/machinery/atmospherics/components/binary/circulator/cold",
        "/obj/structure/cable",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/area/station",
    ];
    let mut states: Vec<(String, String)> = [
        ("t.dmi", "floor"),
        ("t.dmi", "wall"),
        ("t.dmi", ""),
        ("l.dmi", "x2"),
        ("h.dmi", "human_basic"),
        ("c.dmi", "l2-1-2-4-8-node"),
        ("o.dmi", "teg"),
        ("b.dmi", "circ_base"),
        ("b.dmi", "circ_anchor"),
        ("b.dmi", "flow"),
        ("b.dmi", "flow_on"),
        ("b.dmi", "flow_on-emissive"),
        ("b.dmi", "display"),
    ]
    .map(|(d, s)| (d.to_owned(), s.to_owned()))
    .into();
    for s in ["00", "01", "10", "11"] {
        states.push(("o.dmi".into(), format!("teg-oc{s}")));
    }
    for i in 1..=11 {
        states.push(("o.dmi".into(), format!("teg-op{i}")));
    }
    for s in [
        "panel_0",
        "panel_1",
        "display_0",
        "display_1",
        "fan_0",
        "fan_1",
        "fan_0_0",
        "fan_0_1",
        "fan_1_0",
        "fan_1_1",
    ] {
        states.push(("b.dmi".into(), s.into()));
    }
    let states: Vec<(&str, &str)> = states.iter().map(|(d, s)| (&**d, &**s)).collect();
    write_fixture(&dir, TYPES, &used, &states, &[("map.dmm", MAP)])
}

/// The client's `ThermoElectricGenerator` window: id and last data.
#[derive(Resource, Default)]
struct TegWindow {
    id: Option<u32>,
    data: Option<ThermoElectricGeneratorData>,
}

fn record_window(mut received: MessageReader<UiMessageReceived>, mut w: ResMut<TegWindow>) {
    for UiMessageReceived(message) in received.read() {
        match message {
            UiServerMessage::Open {
                window,
                interface,
                data,
                ..
            } if interface == ThermoElectricGeneratorUi::NAME => {
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

struct Session {
    st: testing::Station,
}

ss13_game::drive_station!(Session, st);

impl Session {
    fn start(test: &str) -> Self {
        let assets = fixture_assets(test);
        let map = MapLoadPlugin {
            source: assets.clone(),
            maps: vec!["map.dmm".into()],
            assets,
        };
        let mut st = testing::Station::start(map, |app| {
            app.add_plugins(ss13_atmos::AtmosPlugin);
        });
        st.join("a", |client| {
            client
                .init_resource::<TegWindow>()
                .add_systems(Update, record_window);
        });
        Self { st }
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

    fn port(&self, circulator: Entity, port: usize) -> GasMixture {
        let world = self.server.world();
        let id = world.get::<AtmosDevice>(circulator).expect("plumbed").id;
        world
            .resource::<Pipenet>()
            .air(id, port)
            .unwrap()
            .gas
            .clone()
    }

    /// Fills a circulator port with `gas` at `kpa` and `kelvin`.
    fn fill(&mut self, circulator: Entity, port: usize, gas: GasId, kpa: f64, kelvin: f64) {
        let world = self.server.world_mut();
        let id = world.get::<AtmosDevice>(circulator).unwrap().id;
        let mut net = world.resource_mut::<Pipenet>();
        let air = net.air_mut(id, port).unwrap();
        air.gas = GasMixture::vacuum();
        air.gas.temperature = kelvin;
        air.gas
            .set(gas, kpa * air.volume / (R_IDEAL_GAS_EQUATION * kelvin));
        net.update_parents(id);
    }

    fn tool(&mut self, target: Entity, tool: ToolKind) {
        let client = self.players[0].link;
        let world = self.server.world_mut();
        assert!(tool_act(world, client, target, Some(tool)));
    }

    fn window(&self) -> Option<ThermoElectricGeneratorData> {
        self.players[0]
            .app
            .world()
            .resource::<TegWindow>()
            .data
            .clone()
    }
}

#[test]
fn teg_turns_a_temperature_difference_into_power() {
    let mut s = Session::start("teg");
    let teg = s.find::<Teg>(GEN);
    let hot = s.find::<Circulator>(HOT);
    let cold = s.find::<Circulator>(COLD);
    let linked = s.get::<Teg>(teg).clone();
    assert_eq!((linked.hot, linked.cold), (Some(hot), Some(cold)));
    assert!(
        s.server.world().resource::<Grid>().node_net(teg).is_some(),
        "the generator is on its cable's powernet"
    );

    // Hot plasma into the hot loop's input, cold nitrogen into the cold loop's.
    s.fill(hot, 1, GasId::PLASMA, 4000.0, 1500.0);
    s.fill(cold, 1, GasId::N2, 4000.0, 80.0);
    s.run_for(6.0);
    let t = s.get::<Teg>(teg).clone();
    assert!(t.lastgenlev > 0.0, "{t:?}");
    // Anchored, it does not turn.
    let client = s.players[0].link;
    assert!(ss13_game::power::teg::rotate(
        s.server.world_mut(),
        client,
        teg,
        true
    ));
    assert!(
        s.server
            .world()
            .get::<ss13_shared::Facing>(teg)
            .is_none_or(|f| f.0 == ss13_shared::Dir::South)
    );
    let avail = {
        let grid = s.server.world().resource::<Grid>();
        grid.node_net(teg)
            .and_then(|n| grid.net(n))
            .map_or(0.0, |n| n.avail)
    };
    assert!(avail > 0.0, "the powernet carries the generator's output");
    // The gas crossed: the hot output is cooler than its input, the cold one warmer.
    let (hot_out, cold_out) = (s.port(hot, 0), s.port(cold, 0));
    assert!(hot_out.total_moles() > 0.0 && hot_out.temperature < 1500.0);
    assert!(cold_out.total_moles() > 0.0 && cold_out.temperature > 80.0);

    // The window: the output and both loops.
    s.run_until_for(5.0, "the generator is replicated", |s| {
        s.net_id(0, teg).is_some()
    });
    s.st.click(0, teg);
    s.run_until_for(3.0, "the window opens", |s| s.window().is_some());
    let data = s.window().unwrap();
    assert_eq!(data.error_message, None);
    let hot_loop = data.hot_data.expect("hot loop");
    assert_eq!(hot_loop.temperature_inlet, 1500.0);
    assert!(hot_loop.temperature_outlet < 1500.0);
    assert!(data.cold_data.is_some());

    // A multitool flips the cold circulator to hot: the generator has no cold loop any more.
    s.tool(cold, ToolKind::Multitool);
    assert!(!s.get::<Circulator>(cold).cold);
    assert_eq!(s.get::<Teg>(teg).cold, None);
    s.run_until_for(3.0, "the window reports it", |s| {
        s.window().is_some_and(|d| {
            d.error_message.as_deref() == Some("Unable to locate cold circulator!")
        })
    });
    // Flipped back, the generator's multitool finds it again.
    s.tool(cold, ToolKind::Multitool);
    s.tool(teg, ToolKind::Multitool);
    assert_eq!(s.get::<Teg>(teg).cold, Some(cold));

    // Wrenching needs the panel open; a loose circulator leaves the pipe graph with its gas.
    let kept = s.port(cold, 1).total_moles();
    s.tool(cold, ToolKind::Wrench);
    assert!(s.server.world().get::<AtmosDevice>(cold).is_some());
    s.tool(cold, ToolKind::Screwdriver);
    s.tool(cold, ToolKind::Wrench);
    assert!(s.server.world().get::<AtmosDevice>(cold).is_none());
    assert_eq!(s.get::<Teg>(teg).cold, None);
    // Loose, it turns with alt-clicks (`simple_rotation`): clockwise from west is north.
    let client = s.players[0].link;
    assert!(ss13_game::power::teg::rotate(
        s.server.world_mut(),
        client,
        cold,
        true
    ));
    assert_eq!(
        s.get::<ss13_shared::Facing>(cold).0,
        ss13_shared::Dir::North
    );
    assert!(ss13_game::power::teg::rotate(
        s.server.world_mut(),
        client,
        cold,
        false
    ));
    s.tool(cold, ToolKind::Wrench);
    assert_eq!(s.get::<AtmosDevice>(cold).dir, 8);
    let back = s.port(cold, 1).total_moles();
    assert!((back - kept).abs() < 1e-6, "{kept} -> {back}");
}
