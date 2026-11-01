//! Atmos machinery (server), after tg `code/modules/atmospherics/machinery/`: pipes, vents,
//! scrubbers, pumps, valves, filters, mixers, thermomachines, connectors, injectors, portable
//! machines (canisters, pumps, scrubbers) and air alarms.
//!
//! - The pipe graph and its gas are `ss13_atmos::pipenet` ([`Pipenet`]); every map-loaded atmos
//!   object becomes a device there at startup ([`init_atmos_machinery`], tg `SSair.Initialize`:
//!   `setup_atmos_machinery` + `setup_pipenets`) and carries [`AtmosDevice`] and its machine
//!   state component.
//! - Every SSair fire, after `process_pipenets`, [`devices`] runs tg `process_atmos` of each
//!   machine ([`AtmosSystems::Machinery`]), then the turfs fire; machines exchange gas with their
//!   tile through [`Atmos`].
//! - [`portable`]: canisters and portable pumps and scrubbers, connector ports, tank insertion.
//! - [`alarm`]: air alarms per area: thresholds, alert levels, modes applied to the area's vents
//!   and scrubbers, access lock.
//! - [`windows`]: the tgui windows (`AtmosPump`, `AtmosFilter`, `AtmosMixer`, `ThermoMachine`,
//!   `Canister`, `PortablePump`, `PortableScrubber`, `AirAlarm`) and the click handlers.
//! - [`looks`]: tg's runtime icon states (smart pipes by connections, vents and pumps by state).
//!
//! Cost per fire: O(active machines) with O(1) work each (a vent reads its tile and one port);
//! scrubbers skip tiles without the gases they filter; idle pipelines cost nothing (see
//! `ss13_atmos::pipenet`).

pub mod alarm;
pub mod devices;
pub mod looks;
pub mod portable;
pub mod windows;

use std::collections::HashMap;
use std::time::{Duration, Instant};

use bevy::prelude::*;
use ss13_atmos::pipenet::{
    Custom, DeviceId, DeviceKind, DeviceSpec, HeatExchange, PORT_VOLUME, Pipenet,
    default_node_dirs, flags,
};
use ss13_atmos::{AirTick, AtmosSystems, GASES, GasId, air_tick_due};
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{Areas, DmAtmosDevice, DmAtom};
use ss13_shared::TilePos;

pub use alarm::AirAlarm;
pub use devices::{
    GasFilter, GasMixer, GasPump, OutletInjector, PassiveGate, PassiveVent, PortsConnector,
    ThermoMachine, Valve, VentPump, VentScrubber, VolumePump,
};
pub use portable::{Canister, PortableAtmos, PortablePump, PortableScrubber};

const PIPE_ROOT: &str = "/obj/machinery/atmospherics/pipe";
const SMART_PIPE: &str = "/obj/machinery/atmospherics/pipe/smart";
const LAYER_MANIFOLD: &str = "/obj/machinery/atmospherics/pipe/layer_manifold";
pub const HE_PIPE: &str = "/obj/machinery/atmospherics/pipe/heat_exchanging";
const HE_JUNCTION: &str = "/obj/machinery/atmospherics/pipe/heat_exchanging/junction";
const COMPONENTS: &str = "/obj/machinery/atmospherics/components";
const UNARY: &str = "/obj/machinery/atmospherics/components/unary";
const BINARY: &str = "/obj/machinery/atmospherics/components/binary";
const TRINARY: &str = "/obj/machinery/atmospherics/components/trinary";
pub const VENT_PUMP: &str = "/obj/machinery/atmospherics/components/unary/vent_pump";
pub const HIGH_VOLUME_VENT: &str =
    "/obj/machinery/atmospherics/components/unary/vent_pump/high_volume";
pub const VENT_SCRUBBER: &str = "/obj/machinery/atmospherics/components/unary/vent_scrubber";
const PUMP: &str = "/obj/machinery/atmospherics/components/binary/pump";
const VOLUME_PUMP: &str = "/obj/machinery/atmospherics/components/binary/volume_pump";
const PASSIVE_GATE: &str = "/obj/machinery/atmospherics/components/binary/passive_gate";
const VALVE: &str = "/obj/machinery/atmospherics/components/binary/valve";
const FILTER: &str = "/obj/machinery/atmospherics/components/trinary/filter";
const MIXER: &str = "/obj/machinery/atmospherics/components/trinary/mixer";
const THERMOMACHINE: &str = "/obj/machinery/atmospherics/components/unary/thermomachine";
const FREEZER_ON: &str = "/obj/machinery/atmospherics/components/unary/thermomachine/freezer/on";
const COLDROOM: &str =
    "/obj/machinery/atmospherics/components/unary/thermomachine/freezer/on/coldroom";
pub const CONNECTOR: &str = "/obj/machinery/atmospherics/components/unary/portables_connector";
const INJECTOR: &str = "/obj/machinery/atmospherics/components/unary/outlet_injector";
const PASSIVE_VENT: &str = "/obj/machinery/atmospherics/components/unary/passive_vent";
/// `game/machinery/computer/atmos_computers/outlets.dm`: chamber vents and injectors that
/// `disconnect_from_area()` so air alarms leave them alone.
const MONITORED: [&str; 3] = [
    "/obj/machinery/atmospherics/components/unary/vent_pump/siphon/monitored",
    "/obj/machinery/atmospherics/components/unary/vent_pump/high_volume/siphon/monitored",
    "/obj/machinery/atmospherics/components/unary/outlet_injector/monitored",
];

/// `COLD_ROOM_TEMP`.
pub const COLD_ROOM_TEMP: f64 = 259.15;

/// On every atmos object in the pipe graph.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct AtmosDevice {
    pub id: DeviceId,
    pub layer: u8,
    /// BYOND `dir` (after `normalize_cardinal_directions`).
    pub dir: u8,
}

/// The vents and scrubbers an area's air alarms control (tg `area.air_vents`,
/// `area.air_scrubbers`), by area entity.
#[derive(Resource, Default, Debug)]
pub struct AreaAtmos(pub HashMap<Entity, AreaDevices>);

#[derive(Default, Debug, Clone)]
pub struct AreaDevices {
    pub vents: Vec<Entity>,
    pub scrubbers: Vec<Entity>,
}

/// Numbers for logs and the perf test.
#[derive(Resource, Default, Debug, Clone)]
pub struct AtmosMachineryReport {
    pub devices: usize,
    pub pipelines: usize,
    pub vents: usize,
    pub scrubbers: usize,
    pub alarms: usize,
    pub portables: usize,
    pub build_time: Duration,
    /// CPU time of the machinery systems in the last fire.
    pub last_machinery: Duration,
    started: Option<Instant>,
}

pub struct AtmosMachineryPlugin;

impl Plugin for AtmosMachineryPlugin {
    fn build(&self, app: &mut App) {
        if !app.world().contains_resource::<Pipenet>() {
            app.init_resource::<Pipenet>();
        }
        if !app.world().contains_resource::<AirTick>() {
            app.init_resource::<AirTick>();
        }
        if !app.world().contains_resource::<ss13_atmos::Atmos>() {
            app.init_resource::<ss13_atmos::Atmos>();
        }
        app.init_resource::<AreaAtmos>()
            .init_resource::<AtmosMachineryReport>()
            .add_systems(
                Startup,
                (init_atmos_machinery, alarm::init_alarms, looks::init_looks).chain(),
            )
            .add_systems(
                Update,
                (
                    machinery_start,
                    devices::process_vents,
                    devices::process_scrubbers,
                    devices::process_binary,
                    devices::process_trinary,
                    devices::process_unary,
                    devices::process_he_pipes,
                    portable::process_portables,
                    alarm::process_alarms,
                    machinery_end,
                )
                    .chain()
                    .run_if(air_tick_due)
                    .in_set(AtmosSystems::Machinery),
            )
            .add_systems(
                Update,
                (looks::sync_pipe_looks, looks::sync_machine_looks).after(AtmosSystems::Machinery),
            )
            .add_plugins(windows::plugin);
    }
}

fn machinery_start(mut report: ResMut<AtmosMachineryReport>) {
    report.started = Some(Instant::now());
}

fn machinery_end(mut report: ResMut<AtmosMachineryReport>) {
    if let Some(start) = report.started {
        report.last_machinery = start.elapsed();
    }
}

/// `#RRGGBB` -> RGB; anything else is omni grey.
pub fn parse_pipe_color(s: &str) -> [u8; 3] {
    ss13_mapload::value::parse_color(s)
        .map(|c| [c[0], c[1], c[2]])
        .unwrap_or(ss13_atmos::pipenet::COLOR_OMNI)
}

/// tg `turn(dir, -90)`: a quarter turn clockwise.
pub fn turn_cw(dir: u8) -> u8 {
    match dir {
        1 => 4,
        4 => 2,
        2 => 8,
        8 => 1,
        d => d,
    }
}

pub fn reverse_dir(dir: u8) -> u8 {
    match dir {
        1 => 2,
        2 => 1,
        4 => 8,
        8 => 4,
        d => d,
    }
}

/// tg gas ids -> a bit mask over [`GASES`].
pub fn gas_mask(ids: &[String]) -> u32 {
    ids.iter()
        .filter_map(|id| GasId::parse(id))
        .fold(0, |m, g| m | (1 << g.0))
}

/// The gases of a mask as tg ids, in [`GASES`] order.
pub fn mask_ids(mask: u32) -> impl Iterator<Item = &'static str> {
    GASES
        .iter()
        .enumerate()
        .filter(move |(i, _)| mask & (1 << i) != 0)
        .map(|(_, g)| g.id)
}

/// The device spec of a map object: tg `set_init_directions` and `get_node_connects` per
/// family. `None` for portables, alarms and helpers.
pub fn device_spec(
    entity: Entity,
    pos: TilePos,
    dm: &DmAtmosDevice,
    dir: u8,
) -> Option<(DeviceSpec, u8)> {
    let path = dm.path.as_str();
    if !is_under(path, PIPE_ROOT) && !is_under(path, COMPONENTS) {
        return None;
    }
    let pipe_flags = dm.pipe_flags as u16;
    let he = is_under(path, HE_PIPE);
    // `heat_exchanging/simple/set_init_directions`: a diagonal dir is a bend between its two
    // cardinals.
    let he_bend = he && dm.device_type as usize == 2 && matches!(dir, 5 | 6 | 9 | 10);
    let mut dir = if matches!(dir, 1 | 2 | 4 | 8) || he_bend {
        dir
    } else {
        2
    };
    if pipe_flags & flags::CARDINAL_AUTONORMALIZE != 0 {
        dir = match dir {
            2 => 1,
            8 => 4,
            d => d,
        };
    }
    let axis = if matches!(dir, 1 | 2) { 1 | 2 } else { 4 | 8 };
    let layer = if pipe_flags & flags::DEFAULT_LAYER_ONLY != 0 {
        3
    } else {
        (dm.piping_layer as u8).clamp(1, 5)
    };
    let device_type = dm.device_type as usize;
    let mut port_volumes = Vec::new();
    let mut custom = Custom::None;
    let (kind, init_dirs, node_dirs) = if is_under(path, LAYER_MANIFOLD) {
        (DeviceKind::LayerManifold, axis, Vec::new())
    } else if is_under(path, SMART_PIPE) {
        let init = match dm.initialize_directions as u8 & 15 {
            0 => 15,
            d => d,
        };
        (
            DeviceKind::Pipe {
                volume: 0.0,
                smart: true,
                he: HeatExchange::None,
            },
            init,
            default_node_dirs(4, init),
        )
    } else if is_under(path, PIPE_ROOT) {
        // shortcut: multi-z and other special pipes connect like plain binary pipes along their
        // axis; port them with their features.
        let count = device_type.clamp(1, 4);
        let init = match count {
            4 if he => 15,
            3 if he => 15 & !dir,
            3 | 4 => 15 & !reverse_dir(dir),
            _ if he_bend => dir,
            _ => axis,
        };
        let heat_exchange = if is_under(path, HE_JUNCTION) {
            HeatExchange::Junction {
                plain_side: reverse_dir(dir),
            }
        } else if he {
            HeatExchange::Pipe
        } else {
            HeatExchange::None
        };
        // `junction/get_node_connects`: `list(REVERSE_DIR(dir), dir)`.
        let node_dirs = if heat_exchange != HeatExchange::Pipe && is_under(path, HE_JUNCTION) {
            vec![reverse_dir(dir), dir]
        } else {
            default_node_dirs(count, init)
        };
        (
            DeviceKind::Pipe {
                volume: dm.volume,
                smart: false,
                he: heat_exchange,
            },
            init,
            node_dirs,
        )
    } else if is_under(path, TRINARY) {
        let init = match dir {
            1 => 4 | 1 | 2,
            2 => 2 | 8 | 1,
            4 => 4 | 8 | 2,
            _ => 8 | 1 | 4,
        };
        let (mut n1, n2, mut n3) = (reverse_dir(dir), turn_cw(dir), dir);
        if dm.flipped {
            n1 = reverse_dir(n1);
            n3 = reverse_dir(n3);
        }
        port_volumes = vec![PORT_VOLUME; 3];
        if is_under(path, MIXER) {
            port_volumes[2] = 300.0;
        }
        (DeviceKind::Component, init, vec![n1, n2, n3])
    } else if is_under(path, BINARY) {
        port_volumes = vec![PORT_VOLUME; 2];
        if is_under(path, VALVE) {
            custom = Custom::Valve { open: dm.on };
        }
        (DeviceKind::Component, axis, vec![reverse_dir(dir), dir])
    } else if is_under(path, UNARY) {
        let volume = if is_under(path, CONNECTOR) {
            custom = Custom::Connector { attached: None };
            0.0
        } else if is_under(path, HIGH_VOLUME_VENT) {
            1000.0
        } else {
            PORT_VOLUME
        };
        port_volumes = vec![volume];
        (DeviceKind::Component, dir, vec![dir])
    } else {
        // shortcut: other components (stationary tanks, cryo, the HFR, crystallizers) take
        // part in the graph as plain unary machines facing their dir and do nothing yet.
        port_volumes = vec![PORT_VOLUME];
        (DeviceKind::Component, dir, vec![dir])
    };
    Some((
        DeviceSpec {
            entity,
            pos,
            init_dirs,
            node_dirs,
            layer,
            color: parse_pipe_color(&dm.pipe_color),
            flags: pipe_flags,
            kind,
            port_volumes,
            custom,
        },
        dir,
    ))
}

/// The machine state component of a component's DM type, if it does something.
fn machine_state(world: &mut World, e: Entity, dm: &DmAtmosDevice, area: Option<Entity>) {
    let path = dm.path.as_str();
    let mut entity = world.entity_mut(e);
    if is_under(path, VENT_PUMP) {
        let monitored = MONITORED.iter().any(|m| is_under(path, m));
        entity.insert(VentPump {
            on: dm.on,
            releasing: dm.pump_direction != 0.0,
            pressure_checks: dm.pressure_checks as u8,
            external_pressure_bound: dm.external_pressure_bound,
            internal_pressure_bound: dm.internal_pressure_bound,
            welded: dm.welded,
            area: if monitored { None } else { area },
        });
    } else if is_under(path, VENT_SCRUBBER) {
        entity.insert(VentScrubber {
            on: dm.on,
            scrubbing: dm.scrubbing != 0.0,
            filter: gas_mask(&dm.filter_types),
            widenet: dm.widenet,
            volume_rate: dm.volume_rate,
            welded: dm.welded,
            area,
        });
    } else if is_under(path, PUMP) {
        entity.insert(GasPump {
            on: dm.on,
            target_pressure: dm.target_pressure,
        });
    } else if is_under(path, VOLUME_PUMP) {
        entity.insert(VolumePump {
            on: dm.on,
            transfer_rate: dm.transfer_rate,
        });
    } else if is_under(path, PASSIVE_GATE) {
        entity.insert(PassiveGate {
            on: dm.on,
            target_pressure: dm.target_pressure,
        });
    } else if is_under(path, VALVE) {
        entity.insert(Valve {
            digital: dm.valve_type == "d",
            switching_until: None,
        });
    } else if is_under(path, FILTER) {
        entity.insert(GasFilter {
            on: dm.on,
            transfer_rate: dm.transfer_rate,
            filter: gas_mask(&dm.filter_types),
            flipped: dm.flipped,
        });
    } else if is_under(path, MIXER) {
        entity.insert(GasMixer {
            on: dm.on,
            target_pressure: dm.target_pressure,
            node1_concentration: dm.node1_concentration,
            node2_concentration: dm.node2_concentration,
            flipped: dm.flipped,
        });
    } else if is_under(path, THERMOMACHINE) {
        let mut machine = ThermoMachine::tier1(dm.base_heating, dm.base_cooling, dm.on);
        if is_under(path, COLDROOM) {
            machine.target_temperature = COLD_ROOM_TEMP;
        } else if is_under(path, FREEZER_ON) {
            machine.target_temperature = machine.min_temperature;
        }
        entity.insert(machine);
    } else if is_under(path, CONNECTOR) {
        entity.insert(PortsConnector { connected: None });
    } else if is_under(path, INJECTOR) {
        entity.insert(OutletInjector {
            on: dm.on,
            volume_rate: if dm.volume_rate > 0.0 {
                dm.volume_rate
            } else {
                50.0
            },
        });
    } else if is_under(path, PASSIVE_VENT) {
        entity.insert(PassiveVent);
    }
}

/// tg `SSair.Initialize` for machines: every map atmos object into the pipe graph, connected
/// and in pipelines, with its machine state; vents and scrubbers listed by area.
pub fn init_atmos_machinery(world: &mut World) {
    let start = Instant::now();
    let mut query = world.query::<(Entity, &DmAtmosDevice, &TilePos, Option<&DmAtom>)>();
    let found: Vec<(Entity, DmAtmosDevice, TilePos, u8)> = query
        .iter(world)
        .map(|(e, dm, &pos, atom)| (e, dm.clone(), pos, atom.map_or(2, |a| a.dir)))
        .collect();
    if found.is_empty() {
        return;
    }
    let mut net = world.remove_resource::<Pipenet>().unwrap_or_default();
    let mut area_atmos = AreaAtmos::default();
    let mut report = AtmosMachineryReport::default();
    for (e, dm, pos, dir) in &found {
        if let Some((spec, dir)) = device_spec(*e, *pos, dm, *dir) {
            let layer = spec.layer;
            let he = match spec.kind {
                DeviceKind::Pipe { he, .. } => he,
                _ => HeatExchange::None,
            };
            let id = net.add(spec);
            world.entity_mut(*e).insert(AtmosDevice { id, layer, dir });
            match he {
                HeatExchange::Pipe => {
                    world.entity_mut(*e).insert(devices::HePipe::PIPE);
                }
                HeatExchange::Junction { .. } => {
                    world.entity_mut(*e).insert(devices::HePipe::JUNCTION);
                }
                HeatExchange::None => {}
            }
            let area = world.resource::<Areas>().area_at(*pos);
            machine_state(world, *e, dm, area);
            if let Some(v) = world.get::<VentPump>(*e).and_then(|v| v.area) {
                report.vents += 1;
                area_atmos.0.entry(v).or_default().vents.push(*e);
            } else if world.get::<VentScrubber>(*e).is_some() {
                report.scrubbers += 1;
                if let Some(area) = area {
                    area_atmos.0.entry(area).or_default().scrubbers.push(*e);
                }
            }
        }
    }
    net.build();
    report.devices = net.device_count();
    report.pipelines = net.pipelines().count();
    report.build_time = start.elapsed();
    info!(
        "atmos machinery: {} devices, {} pipelines, {} area vents, {} scrubbers, built in {:.2?}",
        report.devices, report.pipelines, report.vents, report.scrubbers, report.build_time
    );
    world.insert_resource(net);
    world.insert_resource(area_atmos);
    world.insert_resource(report);
    portable::init_portables(world, &found);
}

#[cfg(test)]
mod tests;
