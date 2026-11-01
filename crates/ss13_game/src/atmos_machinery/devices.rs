//! tg `process_atmos` of pipe machines: vents, scrubbers, pumps, gates, valves, filters, mixers,
//! thermomachines, connectors, injectors and passive vents. Each step is a function over the
//! [`Pipenet`] and the tile air so tests can run it alone; the systems below call them once
//! per SSair fire.

use bevy::prelude::*;
use ss13_atmos::gas::{
    CELL_VOLUME, MOLAR_ACCURACY, ONE_ATMOSPHERE, R_IDEAL_GAS_EQUATION, T0C, T20C, TCMB,
    conduction_energy, quantize,
};
use ss13_atmos::pipenet::air::{
    Mix, PipeAir, TurfMix, equalize_with_turf, pump_moles, release_moles,
};
use ss13_atmos::pipenet::{DeviceId, Pipenet};
use ss13_atmos::{Atmos, GAS_COUNT, GasMixture};
use ss13_shared::TilePos;

use super::AtmosDevice;
use crate::power::Unpowered;

/// `ATMOS_EXTERNAL_BOUND`, `ATMOS_INTERNAL_BOUND`.
pub const EXTERNAL_BOUND: u8 = 1;
pub const INTERNAL_BOUND: u8 = 2;
/// `ATMOS_PUMP_MAX_PRESSURE`: what air alarms may set vent bounds to.
pub const ATMOS_PUMP_MAX_PRESSURE: f64 = ONE_ATMOSPHERE * 50.0;
/// `MAX_OUTPUT_PRESSURE`, `MAX_TRANSFER_RATE`.
pub const MAX_OUTPUT_PRESSURE: f64 = 4500.0;
pub const MAX_TRANSFER_RATE: f64 = 200.0;
/// `VOLUME_PUMP_MINIMUM_OUTPUT_PRESSURE`, `VOLUME_PUMP_MAX_OUTPUT_PRESSURE`.
const VOLUME_PUMP_MIN_INPUT: f64 = 0.01;
const VOLUME_PUMP_MAX_OUTPUT: f64 = 9000.0;
/// `MINIMUM_MOLES_TO_SCRUB` of vent scrubbers.
const MINIMUM_MOLES_TO_SCRUB: f64 = MOLAR_ACCURACY * 100.0;
/// `THERMOMACHINE_POWER_CONVERSION`.
const THERMOMACHINE_POWER_CONVERSION: f64 = 0.01;

/// `/obj/machinery/atmospherics/components/unary/vent_pump`.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct VentPump {
    pub on: bool,
    /// `pump_direction`: `ATMOS_DIRECTION_RELEASING` (true) or siphoning.
    pub releasing: bool,
    /// [`EXTERNAL_BOUND`] | [`INTERNAL_BOUND`].
    pub pressure_checks: u8,
    pub external_pressure_bound: f64,
    pub internal_pressure_bound: f64,
    pub welded: bool,
    /// `assigned_area`: its air alarms control it (`None` for monitored chamber vents).
    pub area: Option<Entity>,
}

/// `/obj/machinery/atmospherics/components/unary/vent_scrubber`.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct VentScrubber {
    pub on: bool,
    /// `scrubbing`: `ATMOS_DIRECTION_SCRUBBING` (true) or siphoning everything.
    pub scrubbing: bool,
    /// `filter_types` as a bit mask over `ss13_atmos::GASES`.
    pub filter: u32,
    pub widenet: bool,
    /// L per fire.
    pub volume_rate: f64,
    pub welded: bool,
    pub area: Option<Entity>,
}

/// `binary/pump`.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct GasPump {
    pub on: bool,
    pub target_pressure: f64,
}

/// `binary/volume_pump`.
// shortcut: no multitool overclock (pressure limits off, leaking); add with multitool acts.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct VolumePump {
    pub on: bool,
    pub transfer_rate: f64,
}

/// `binary/passive_gate`: needs no power.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct PassiveGate {
    pub on: bool,
    pub target_pressure: f64,
}

/// `binary/valve`: open or closed is `Custom::Valve` in the [`Pipenet`]. A hand turns a manual
/// one in a second (`interact` -> `finish_interact`).
// shortcut: digital valves turn by hand like manual ones; AI control comes with silicons.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Valve {
    pub digital: bool,
    /// `switching`: when the turn in progress completes (`Time` seconds).
    pub switching_until: Option<f64>,
}

/// `trinary/filter`.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct GasFilter {
    pub on: bool,
    pub transfer_rate: f64,
    pub filter: u32,
    pub flipped: bool,
}

/// `trinary/mixer`.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct GasMixer {
    pub on: bool,
    pub target_pressure: f64,
    pub node1_concentration: f64,
    pub node2_concentration: f64,
    pub flipped: bool,
}

/// `unary/thermomachine`.
// shortcut: stock parts are tier 1 (no upgrades, no panel or rotation); the heat moved draws
// no extra power beyond the idle load (`use_energy` of `process_atmos`).
#[derive(Component, Clone, Debug, PartialEq)]
pub struct ThermoMachine {
    pub on: bool,
    pub min_temperature: f64,
    pub max_temperature: f64,
    pub target_temperature: f64,
    /// J/K.
    pub heat_capacity: f64,
}

impl ThermoMachine {
    /// `RefreshParts` with the board's two tier 1 matter bins and micro lasers.
    pub fn tier1(base_heating: f64, base_cooling: f64, on: bool) -> Self {
        let (bins, lasers) = (2.0_f64, 2.0_f64);
        let base_heating = if base_heating > 0.0 {
            base_heating
        } else {
            140.0
        };
        let base_cooling = if base_cooling > 0.0 {
            base_cooling
        } else {
            170.0
        };
        Self {
            on,
            min_temperature: (T0C - (base_cooling + lasers * 15.0)).max(TCMB),
            max_temperature: T20C + base_heating * lasers,
            target_temperature: T20C,
            heat_capacity: 5000.0 * (bins - 1.0).powi(2),
        }
    }
}

/// `unary/portables_connector`: the portable machine wrenched to it.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct PortsConnector {
    pub connected: Option<Entity>,
}

/// `unary/outlet_injector`.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct OutletInjector {
    pub on: bool,
    /// L per fire.
    pub volume_rate: f64,
}

/// `unary/passive_vent`.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct PassiveVent;

fn connected(net: &Pipenet, id: DeviceId, port: usize) -> bool {
    net.device(id)
        .and_then(|d| d.nodes.get(port).copied().flatten())
        .is_some()
}

/// `vent_pump/process_atmos` after the power and node checks: moves gas between the port and
/// the tile. True when gas moved.
pub fn vent_step(
    net: &mut Pipenet,
    atmos: &mut Atmos,
    id: DeviceId,
    pos: TilePos,
    vent: &VentPump,
) -> bool {
    let Some(env) = atmos.air(pos) else {
        return false;
    };
    let (env_pressure, env_temperature) = (env.pressure(), env.temperature);
    let Some(air) = net.air(id, 0) else {
        return false;
    };
    let (air_pressure, air_temperature, air_volume) =
        (air.pressure(), air.temperature(), air.volume);
    let checks = vent.pressure_checks;
    if vent.releasing {
        let mut delta: f64 = 10000.0;
        if checks & EXTERNAL_BOUND != 0 {
            delta = delta.min(vent.external_pressure_bound - env_pressure);
        }
        if checks & INTERNAL_BOUND != 0 {
            delta = delta.min(air_pressure - vent.internal_pressure_bound);
        }
        if delta <= 0.0 || air_temperature <= 0.0 || env_pressure >= 50.0 * ONE_ATMOSPHERE {
            return false;
        }
        let moles = delta * CELL_VOLUME / (air_temperature * R_IDEAL_GAS_EQUATION);
        let Some(removed) = net.air_mut(id, 0).and_then(|a| a.remove(moles)) else {
            return false;
        };
        if removed.total_moles() == 0.0 {
            return false;
        }
        atmos.merge_air(pos, &removed);
    } else {
        let mut delta: f64 = 10000.0;
        if checks & EXTERNAL_BOUND != 0 {
            delta = delta.min(env_pressure - vent.external_pressure_bound);
        }
        if checks & INTERNAL_BOUND != 0 {
            delta = delta.min(vent.internal_pressure_bound - air_pressure);
        }
        if delta <= 0.0 || env_temperature <= 0.0 || air_pressure >= 50.0 * ONE_ATMOSPHERE {
            return false;
        }
        let moles = delta * air_volume / (env_temperature * R_IDEAL_GAS_EQUATION);
        let Some(removed) = atmos.remove_air(pos, moles) else {
            return false;
        };
        if removed.total_moles() == 0.0 {
            return false;
        }
        if let Some(air) = net.air_mut(id, 0) {
            air.merge(&removed);
        }
    }
    net.update_parents(id);
    true
}

/// Bits of the gases present in a mixture.
fn present(gas: &GasMixture) -> u32 {
    gas.moles
        .iter()
        .enumerate()
        .filter(|(_, m)| **m != 0.0)
        .fold(0, |mask, (i, _)| mask | (1 << i))
}

/// `vent_scrubber/scrub(tile)`. True when it took gas.
pub fn scrub(
    net: &mut Pipenet,
    atmos: &mut Atmos,
    id: DeviceId,
    tile: TilePos,
    scrubber: &VentScrubber,
) -> bool {
    if net
        .air(id, 0)
        .is_none_or(|a| a.pressure() >= 50.0 * ONE_ATMOSPHERE)
    {
        return false;
    }
    let ratio = (scrubber.volume_rate / CELL_VOLUME).min(1.0);
    let taken = if scrubber.scrubbing {
        let filter = scrubber.filter;
        if atmos.air(tile).is_none_or(|env| present(env) & filter == 0) {
            return false;
        }
        atmos.modify_air(tile, |env| {
            scrub_gases(env, filter, ratio, MINIMUM_MOLES_TO_SCRUB)
        })
    } else {
        let total = atmos.air(tile).map_or(0.0, GasMixture::total_moles);
        atmos.remove_air(tile, total * ratio)
    };
    let Some(taken) = taken else {
        return false;
    };
    if let Some(air) = net.air_mut(id, 0) {
        air.merge(&taken);
    }
    net.update_parents(id);
    true
}

/// The filtering half of tg `scrub()` (vent and portable scrubbers): each filtered gas in
/// proportion to its share, at least `minimum` (or all of it).
pub fn scrub_gases(env: &mut GasMixture, filter: u32, ratio: f64, minimum: f64) -> GasMixture {
    let mut out = GasMixture::vacuum();
    out.temperature = env.temperature;
    let total: f64 = (0..GAS_COUNT)
        .filter(|g| filter & (1 << g) != 0)
        .map(|g| env.moles[g])
        .sum();
    if total == 0.0 {
        return out;
    }
    for g in 0..GAS_COUNT {
        let m = env.moles[g];
        if filter & (1 << g) == 0 || m == 0.0 {
            continue;
        }
        let moved = quantize(m * ratio * (m / total)).max(minimum.min(m));
        out.moles[g] = moved;
        env.moles[g] -= moved;
    }
    out
}

/// `binary/pump/process_atmos`.
pub fn pump_step(net: &mut Pipenet, id: DeviceId, target_pressure: f64) -> bool {
    let pipenet = net.pipeline_air(id, 1).cloned();
    let Some(airs) = net.airs_mut(id) else {
        return false;
    };
    let [input, output] = airs else {
        return false;
    };
    let moles = pump_moles(
        &*input,
        &*output,
        target_pressure,
        pipenet.as_ref().map(|p| p as &dyn Mix),
    );
    let Some(removed) = input.remove(moles) else {
        return false;
    };
    output.merge(&removed);
    net.update_parents(id);
    true
}

/// `binary/passive_gate/process_atmos`.
pub fn passive_gate_step(net: &mut Pipenet, id: DeviceId, target_pressure: f64) -> bool {
    let pipenet = net.pipeline_air(id, 1).cloned();
    let Some(airs) = net.airs_mut(id) else {
        return false;
    };
    let [input, output] = airs else {
        return false;
    };
    let Some(moles) = release_moles(
        &*input,
        &*output,
        target_pressure,
        1.0,
        pipenet.as_ref().map(|p| p as &dyn Mix),
    ) else {
        return false;
    };
    let Some(removed) = input.remove(moles) else {
        return false;
    };
    output.merge(&removed);
    net.update_parents(id);
    true
}

/// `binary/volume_pump/process_atmos` (not overclocked).
pub fn volume_pump_step(net: &mut Pipenet, id: DeviceId, transfer_rate: f64) -> bool {
    let Some(airs) = net.airs_mut(id) else {
        return false;
    };
    let [input, output] = airs else {
        return false;
    };
    if input.pressure() < VOLUME_PUMP_MIN_INPUT || output.pressure() > VOLUME_PUMP_MAX_OUTPUT {
        return false;
    }
    let removed = input.remove_ratio(transfer_rate / input.volume);
    if removed.total_moles() == 0.0 {
        return false;
    }
    output.merge(&removed);
    net.update_parents(id);
    true
}

/// `trinary/filter/process_atmos` after the node and power checks.
pub fn filter_step(net: &mut Pipenet, id: DeviceId, transfer_rate: f64, filter: u32) -> bool {
    let Some(airs) = net.airs_mut(id) else {
        return false;
    };
    let [air1, air2, air3] = airs else {
        return false;
    };
    if air1.temperature() <= 0.0 {
        return false;
    }
    let ratio = transfer_rate / air1.volume;
    if ratio <= 0.0 {
        return false;
    }
    let side_full = air2.pressure() >= MAX_OUTPUT_PRESSURE;
    let main_full = air3.pressure() >= MAX_OUTPUT_PRESSURE;
    if side_full && main_full {
        return false;
    }
    let removed = air1.remove_ratio(ratio);
    if removed.total_moles() == 0.0 {
        return false;
    }
    let mut removed = PipeAir::with_gas(removed, air1.volume);
    if filter != 0 {
        let filtered = removed.remove_gases(filter);
        if side_full {
            air1.merge(&filtered);
        } else {
            air2.merge(&filtered);
        }
    }
    if main_full {
        air1.merge(&removed.gas);
    } else {
        air3.merge(&removed.gas);
    }
    net.update_parents(id);
    true
}

/// `trinary/mixer/process_atmos` after the node and power checks.
pub fn mixer_step(net: &mut Pipenet, id: DeviceId, mixer: &GasMixer) -> bool {
    let Some(airs) = net.airs_mut(id) else {
        return false;
    };
    let [air1, air2, air3] = airs else {
        return false;
    };
    let output_pressure = air3.pressure();
    if output_pressure >= mixer.target_pressure {
        return false;
    }
    let general = (mixer.target_pressure - output_pressure) * air3.volume / R_IDEAL_GAS_EQUATION;
    let combined = air1.heat_capacity() + air2.heat_capacity();
    let equalized = if combined != 0.0 {
        (air1.thermal_energy() + air2.thermal_energy()) / combined
    } else {
        0.0
    };
    let (c1, c2) = (mixer.node1_concentration, mixer.node2_concentration);
    let mut moles1 = if equalized != 0.0 {
        c1 * general / equalized
    } else {
        0.0
    };
    let mut moles2 = if equalized != 0.0 {
        c2 * general / equalized
    } else {
        0.0
    };
    let (n1, n2) = (air1.total_moles(), air2.total_moles());
    if c2 == 0.0 {
        if air1.temperature() <= 0.0 {
            return false;
        }
        moles1 = moles1.min(n1);
        moles2 = 0.0;
    } else if c1 == 0.0 {
        if air2.temperature() <= 0.0 {
            return false;
        }
        moles2 = moles2.min(n2);
        moles1 = 0.0;
    } else {
        if air1.temperature() <= 0.0 || air2.temperature() <= 0.0 {
            return false;
        }
        if moles2 <= 0.0 || moles1 <= 0.0 {
            return false;
        }
        if n1 < moles1 || n2 < moles2 {
            let ratio = (n1 / moles1).min(n2 / moles2);
            moles1 *= ratio;
            moles2 *= ratio;
        }
    }
    if moles1 != 0.0
        && let Some(r) = air1.remove(moles1)
    {
        air3.merge(&r);
    }
    if moles2 != 0.0
        && let Some(r) = air2.remove(moles2)
    {
        air3.merge(&r);
    }
    // tg marks the pipelines of the ports it used.
    net.update_parents(id);
    true
}

/// `thermomachine/process_atmos` after the power check: the port's gas toward the target.
/// Returns the heat moved (J).
pub fn thermo_step(net: &mut Pipenet, id: DeviceId, machine: &ThermoMachine) -> f64 {
    let Some(port) = net.air_mut(id, 0) else {
        return 0.0;
    };
    if port.total_moles() == 0.0 {
        return 0.0;
    }
    let capacity = port.heat_capacity();
    let delta = machine.target_temperature - port.temperature();
    let heat = conduction_energy(delta, capacity, machine.heat_capacity);
    port.gas.temperature = ((port.gas.temperature * capacity + heat) / capacity).max(TCMB);
    net.update_parents(id);
    heat.abs().min(1e8) * THERMOMACHINE_POWER_CONVERSION
}

/// `outlet_injector/process_atmos` after the checks: `volume_rate` litres of the port into
/// the tile.
pub fn injector_step(
    net: &mut Pipenet,
    atmos: &mut Atmos,
    id: DeviceId,
    pos: TilePos,
    volume_rate: f64,
) -> bool {
    if atmos.air(pos).is_none() {
        return false;
    }
    let Some(air) = net.air_mut(id, 0) else {
        return false;
    };
    if air.temperature() <= 0.0 {
        return false;
    }
    let moles = air.pressure() * volume_rate / (air.temperature() * R_IDEAL_GAS_EQUATION);
    if moles == 0.0 {
        return false;
    }
    let Some(removed) = air.remove(moles) else {
        return false;
    };
    atmos.merge_air(pos, &removed);
    net.update_parents(id);
    true
}

/// `passive_vent/process_atmos`: the port and the tile equalize.
pub fn passive_vent_step(net: &mut Pipenet, atmos: &mut Atmos, id: DeviceId, pos: TilePos) -> bool {
    let Some(mut port) = net.air(id, 0).cloned() else {
        return false;
    };
    if port.volume == 0.0 {
        return false;
    }
    let moved = atmos
        .modify_air(pos, |env| equalize_with_turf(&mut port, env))
        .unwrap_or(false);
    if moved {
        if let Some(air) = net.air_mut(id, 0) {
            *air = port;
        }
        net.update_parents(id);
    }
    moved
}

/// Turf-side pump for portables: moles `input` pushes into a turf toward `target`.
pub fn pump_into_turf(input: &PipeAir, turf: &GasMixture, target: f64) -> f64 {
    pump_moles(input, &TurfMix(turf), target, None)
}

pub fn process_vents(
    mut net: ResMut<Pipenet>,
    mut atmos: ResMut<Atmos>,
    mut vents: Query<(&AtmosDevice, &TilePos, &mut VentPump, Has<Unpowered>)>,
) {
    for (device, &pos, mut vent, unpowered) in &mut vents {
        if unpowered {
            continue;
        }
        if !connected(&net, device.id, 0) && vent.on {
            vent.on = false;
        }
        if !vent.on || vent.welded {
            continue;
        }
        vent_step(&mut net, &mut atmos, device.id, pos, &vent);
    }
}

pub fn process_scrubbers(
    mut net: ResMut<Pipenet>,
    mut atmos: ResMut<Atmos>,
    mut scrubbers: Query<(&AtmosDevice, &TilePos, &mut VentScrubber, Has<Unpowered>)>,
) {
    for (device, &pos, mut scrubber, unpowered) in &mut scrubbers {
        if scrubber.welded || unpowered {
            continue;
        }
        if !connected(&net, device.id, 0) && scrubber.on {
            scrubber.on = false;
        }
        if !scrubber.on {
            continue;
        }
        scrub(&mut net, &mut atmos, device.id, pos, &scrubber);
        if scrubber.widenet {
            for tile in atmos.adjacent_turfs(pos, true) {
                scrub(&mut net, &mut atmos, device.id, tile, &scrubber);
            }
        }
    }
}

type BinaryQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static AtmosDevice,
        Option<&'static GasPump>,
        Option<&'static VolumePump>,
        Option<&'static PassiveGate>,
        Option<&'static mut Valve>,
        Has<Unpowered>,
    ),
    Or<(
        With<GasPump>,
        With<VolumePump>,
        With<PassiveGate>,
        With<Valve>,
    )>,
>;

pub fn process_binary(mut net: ResMut<Pipenet>, time: Res<Time>, mut machines: BinaryQuery) {
    let now = time.elapsed_secs_f64();
    for (device, pump, volume, gate, valve, unpowered) in &mut machines {
        let id = device.id;
        if let Some(pump) = pump.filter(|p| p.on && !unpowered) {
            pump_step(&mut net, id, pump.target_pressure);
        } else if let Some(volume) = volume.filter(|p| p.on && !unpowered) {
            volume_pump_step(&mut net, id, volume.transfer_rate);
        } else if let Some(gate) = gate.filter(|g| g.on) {
            passive_gate_step(&mut net, id, gate.target_pressure);
        } else if let Some(mut valve) = valve
            && valve.switching_until.is_some_and(|t| now >= t)
        {
            // `finish_interact` -> `set_open(!on)`.
            valve.switching_until = None;
            let open = valve_open(&net, id);
            net.set_valve_open(id, !open);
        }
    }
}

/// A valve's state in the pipe graph.
pub fn valve_open(net: &Pipenet, id: DeviceId) -> bool {
    matches!(
        net.device(id).map(|d| &d.custom),
        Some(ss13_atmos::pipenet::Custom::Valve { open: true })
    )
}

fn all_connected(net: &Pipenet, id: DeviceId) -> bool {
    net.device(id)
        .is_some_and(|d| d.nodes.iter().all(Option::is_some))
}

pub fn process_trinary(
    mut net: ResMut<Pipenet>,
    machines: Query<
        (
            &AtmosDevice,
            Option<&GasFilter>,
            Option<&GasMixer>,
            Has<Unpowered>,
        ),
        Or<(With<GasFilter>, With<GasMixer>)>,
    >,
) {
    for (device, filter, mixer, unpowered) in &machines {
        if unpowered || !all_connected(&net, device.id) {
            continue;
        }
        if let Some(filter) = filter.filter(|f| f.on) {
            filter_step(&mut net, device.id, filter.transfer_rate, filter.filter);
        } else if let Some(mixer) = mixer.filter(|m| m.on) {
            mixer_step(&mut net, device.id, mixer);
        }
    }
}

type UnaryQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static AtmosDevice,
        &'static TilePos,
        Option<&'static mut ThermoMachine>,
        Option<&'static PortsConnector>,
        Option<&'static OutletInjector>,
        Has<PassiveVent>,
        Has<Unpowered>,
    ),
    Or<(
        With<ThermoMachine>,
        With<PortsConnector>,
        With<OutletInjector>,
        With<PassiveVent>,
    )>,
>;

pub fn process_unary(mut net: ResMut<Pipenet>, mut atmos: ResMut<Atmos>, mut machines: UnaryQuery) {
    for (device, &pos, thermo, connector, injector, passive, unpowered) in &mut machines {
        let id = device.id;
        if let Some(mut thermo) = thermo {
            if !thermo.on {
                continue;
            }
            if unpowered {
                thermo.on = false;
                continue;
            }
            thermo_step(&mut net, id, &thermo);
        } else if connector.is_some_and(|c| c.connected.is_some()) {
            net.update_parents(id);
        } else if let Some(injector) = injector.filter(|i| i.on && !unpowered) {
            injector_step(&mut net, &mut atmos, id, pos, injector.volume_rate);
        } else if passive {
            passive_vent_step(&mut net, &mut atmos, id, pos);
        }
    }
}

/// `/obj/machinery/atmospherics/pipe/heat_exchanging`: exchanges heat between its pipeline and
/// its tile every fire.
// shortcut: no glow above 500 K (`process` animating `color`), no heating or burning of
// buckled mobs; closed turfs (an HE pipe under a wall) and lava exchange nothing.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct HePipe {
    pub minimum_temperature_difference: f64,
    pub thermal_conductivity: f64,
}

impl HePipe {
    /// `heat_exchanging`: 20 K, `WINDOW_HEAT_TRANSFER_COEFFICIENT`.
    pub const PIPE: Self = Self {
        minimum_temperature_difference: 20.0,
        thermal_conductivity: 0.1,
    };
    /// `heat_exchanging/junction`: 300 K, `WALL_HEAT_TRANSFER_COEFFICIENT` (0: it exchanges
    /// nothing).
    pub const JUNCTION: Self = Self {
        minimum_temperature_difference: 300.0,
        thermal_conductivity: 0.0,
    };
}

/// tg `heat_exchanging/process_atmos`: when its tile's temperature differs from the pipeline's
/// by more than `minimum_temperature_difference`, `parent.temperature_interact(turf, volume,
/// thermal_conductivity)`. Space is 2.7 K with `HEAT_CAPACITY_VACUUM` and keeps its
/// temperature (`/turf/open/space/TakeTemperature` does nothing); an open tile's air takes the
/// heat and wakes.
pub fn he_step(net: &mut Pipenet, atmos: &mut Atmos, id: DeviceId, pos: TilePos, he: HePipe) {
    // A junction conducts nothing; tg still wakes its tile with a zero change.
    if he.thermal_conductivity == 0.0 {
        return;
    }
    let (Some(air), Some(pipe_air)) = (atmos.air(pos), net.pipeline_air(id, 0)) else {
        return;
    };
    let (turf_temperature, turf_heat_capacity) = (air.temperature, air.heat_capacity());
    if (turf_temperature - pipe_air.temperature()).abs() <= he.minimum_temperature_difference {
        return;
    }
    let volume = net.device(id).map_or(0.0, |d| d.volume);
    if let Some(delta) = net.temperature_interact(
        id,
        volume,
        he.thermal_conductivity,
        turf_temperature,
        turf_heat_capacity,
    ) {
        atmos.modify_air(pos, |a| a.temperature += delta);
    }
}

pub fn process_he_pipes(
    mut net: ResMut<Pipenet>,
    mut atmos: ResMut<Atmos>,
    pipes: Query<(&AtmosDevice, &TilePos, &HePipe)>,
) {
    for (device, &pos, &he) in &pipes {
        he_step(&mut net, &mut atmos, device.id, pos, he);
    }
}
