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
/// `VOLUME_PUMP_LEAK_AMOUNT`.
const VOLUME_PUMP_LEAK_AMOUNT: f64 = 0.1;
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
    /// `fan_overclocked`: no 50 atm limit and full flow at any integrity, but the fan wears
    /// out (`crate::atmos_machinery::overclock`).
    pub overclocked: bool,
}

/// `/obj/machinery/atmospherics/components/binary/dp_vent_pump`: releases from its input port
/// into the tile, or siphons the tile into its output port. Driven by consoles (`id_tag`).
#[derive(Component, Clone, Debug, PartialEq)]
pub struct DpVentPump {
    pub on: bool,
    pub releasing: bool,
    /// [`EXTERNAL_BOUND`] | [`INTERNAL_BOUND`].
    pub pressure_checks: u8,
    pub external_pressure_bound: f64,
    pub input_pressure_min: f64,
    pub output_pressure_max: f64,
}

/// `dp_vent_pump/process_atmos` (tg checks only `on`, not power). True when gas moved.
pub fn dp_vent_step(
    net: &mut Pipenet,
    atmos: &mut Atmos,
    id: DeviceId,
    pos: TilePos,
    vent: &DpVentPump,
) -> bool {
    // `isclosedturf`: no air there.
    let Some(env) = atmos.air(pos) else {
        return false;
    };
    let (env_pressure, env_temperature) = (env.pressure(), env.temperature);
    let checks = vent.pressure_checks;
    if vent.releasing {
        let Some(air1) = net.air(id, 0) else {
            return false;
        };
        let mut delta: f64 = 10000.0;
        if checks & EXTERNAL_BOUND != 0 {
            delta = delta.min(vent.external_pressure_bound - env_pressure);
        }
        if checks & INTERNAL_BOUND != 0 {
            delta = delta.min(air1.pressure() - vent.input_pressure_min);
        }
        if delta <= 0.0 || air1.temperature() <= 0.0 {
            return false;
        }
        let moles = delta * CELL_VOLUME / (air1.temperature() * R_IDEAL_GAS_EQUATION);
        let Some(removed) = net.air_mut(id, 0).and_then(|a| a.remove(moles)) else {
            return false;
        };
        atmos.merge_air(pos, &removed);
    } else {
        let Some(air2) = net.air(id, 1) else {
            return false;
        };
        let mut delta: f64 = 10000.0;
        if checks & EXTERNAL_BOUND != 0 {
            delta = delta.min(env_pressure - vent.external_pressure_bound);
        }
        if checks & INTERNAL_BOUND != 0 {
            delta = delta.min(vent.output_pressure_max - air2.pressure());
        }
        if delta <= 0.0 || env_temperature <= 0.0 {
            return false;
        }
        let moles = delta * air2.volume / (env_temperature * R_IDEAL_GAS_EQUATION);
        let Some(removed) = atmos.remove_air(pos, moles) else {
            return false;
        };
        if let Some(air) = net.air_mut(id, 1) {
            air.merge(&removed);
        }
    }
    net.update_parents(id);
    true
}

pub fn process_dp_vents(
    mut net: ResMut<Pipenet>,
    mut atmos: ResMut<Atmos>,
    vents: Query<(&AtmosDevice, &TilePos, &DpVentPump)>,
) {
    for (device, &pos, vent) in &vents {
        if vent.on {
            dp_vent_step(&mut net, &mut atmos, device.id, pos, vent);
        }
    }
}

/// `unary/heat_exchanger`: back to back with a partner on the tile behind it, the two port
/// mixtures take one temperature every fire.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct HeatExchanger {
    pub partner: Option<Entity>,
}

/// `heat_exchanger/atmos_init`: the exchanger behind each one facing the other way.
pub fn pair_heat_exchangers(world: &mut World) {
    let mut q = world.query::<(Entity, &AtmosDevice, &TilePos, &HeatExchanger)>();
    let all: Vec<(Entity, u8, TilePos)> =
        q.iter(world).map(|(e, d, &p, _)| (e, d.dir, p)).collect();
    for &(e, dir, pos) in &all {
        let back = super::reverse_dir(dir);
        let (dx, dy) = ss13_shared::Dir::from_bits(back).map_or((0, 0), |d| d.offset());
        let behind = TilePos::new(pos.x + dx, pos.y + dy, pos.z);
        let partner = all
            .iter()
            .find(|&&(o, odir, opos)| o != e && opos == behind && odir & back != 0)
            .map(|&(o, ..)| o);
        if let Some(partner) = partner {
            world.entity_mut(e).insert(HeatExchanger {
                partner: Some(partner),
            });
            world
                .entity_mut(partner)
                .insert(HeatExchanger { partner: Some(e) });
        }
    }
}

/// `heat_exchanger/process_atmos` for one pair: both ports at their combined temperature.
pub fn heat_exchange_step(net: &mut Pipenet, a: DeviceId, b: DeviceId) {
    let (Some(air_a), Some(air_b)) = (net.air(a, 0), net.air(b, 0)) else {
        return;
    };
    let (ca, cb) = (air_a.heat_capacity(), air_b.heat_capacity());
    let (ta, tb) = (air_a.temperature(), air_b.temperature());
    let combined = ca + cb;
    if combined <= 0.0 {
        return;
    }
    let t = (ta * ca + tb * cb) / combined;
    for (id, old) in [(a, ta), (b, tb)] {
        if let Some(air) = net.air_mut(id, 0) {
            air.gas.temperature = t;
        }
        if (old - t).abs() > 1.0 {
            net.update_parents(id);
        }
    }
}

pub fn process_heat_exchangers(
    mut net: ResMut<Pipenet>,
    exchangers: Query<(Entity, &AtmosDevice, &HeatExchanger)>,
    devices: Query<&AtmosDevice>,
) {
    for (e, device, exchanger) in &exchangers {
        // `update_cycle`: each pair once a fire, by its lower entity.
        let Some(partner) = exchanger.partner.filter(|&p| e < p) else {
            continue;
        };
        if let Ok(other) = devices.get(partner) {
            heat_exchange_step(&mut net, device.id, other.id);
        }
    }
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
    /// `overclocked` (multitool): no output pressure limit, a tenth of the flow leaks out.
    pub overclocked: bool,
}

/// `binary/passive_gate`: needs no power.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct PassiveGate {
    pub on: bool,
    pub target_pressure: f64,
}

/// What opens a [`ConditionalGate`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GateCondition {
    /// `binary/pressure_valve`: the input is above `target` kPa.
    Pressure { target: f64 },
    /// `binary/temperature_gate`: the input is colder than `target` K (hotter if `inverted`).
    Temperature { target: f64, inverted: bool },
}

/// `binary/pressure_valve` and `binary/temperature_gate`: while the condition holds, gas is
/// released to the output up to the input's own pressure (`release_gas_to`). The pressure
/// valve needs no power (`NO_POWER_USE`).
#[derive(Component, Clone, Debug, PartialEq)]
pub struct ConditionalGate {
    pub on: bool,
    pub condition: GateCondition,
    /// `is_gas_flowing`: drawn as `_flow`.
    pub flowing: bool,
}

/// `binary/temperature_gate`: `minimum_temperature`, `max_temperature`.
pub const TEMPERATURE_GATE_MIN: f64 = ss13_atmos::gas::TCMB;
pub const TEMPERATURE_GATE_MAX: f64 = 4500.0;

/// `binary/temperature_pump`: moves `heat_transfer_rate` percent of the heat delta from the
/// input to the output each fire.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct TemperaturePump {
    pub on: bool,
    pub heat_transfer_rate: f64,
}

/// `max_heat_transfer_rate`.
pub const MAX_HEAT_TRANSFER_RATE: f64 = 100.0;

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
// shortcut: stock parts are tier 1 (`RefreshParts` needs machine parts, see
// docs/parity/engineering/machines.md).
#[derive(Component, Clone, Debug, PartialEq)]
pub struct ThermoMachine {
    pub on: bool,
    pub min_temperature: f64,
    pub max_temperature: f64,
    pub target_temperature: f64,
    /// J/K.
    pub heat_capacity: f64,
    /// Joules this fire's `use_energy` still owes ([`draw_thermo_energy`]).
    pub energy_due: f64,
    pub panel_open: bool,
    /// Index into `GLOB.pipe_paint_colors` of the last right-click multitool color (tg starts
    /// at the first, omni, whatever the pipe's color).
    pub color_index: usize,
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
            energy_due: 0.0,
            panel_open: false,
            color_index: 0,
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

/// `vent_pump/process_atmos` after the power, node and fan damage steps: moves gas between the
/// port and the tile. `integrity` is `get_integrity_percentage()`. True when gas moved.
pub fn vent_step(
    net: &mut Pipenet,
    atmos: &mut Atmos,
    id: DeviceId,
    pos: TilePos,
    vent: &VentPump,
    integrity: f64,
) -> bool {
    // A worn fan moves less unless overclocked.
    let scale = if !vent.overclocked && integrity < 1.0 {
        integrity
    } else {
        1.0
    };
    let limited = !vent.overclocked;
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
        if delta <= 0.0
            || air_temperature <= 0.0
            || (limited && env_pressure >= 50.0 * ONE_ATMOSPHERE)
        {
            return false;
        }
        let moles = delta * CELL_VOLUME / (air_temperature * R_IDEAL_GAS_EQUATION) * scale;
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
        if delta <= 0.0
            || env_temperature <= 0.0
            || (limited && air_pressure >= 50.0 * ONE_ATMOSPHERE)
        {
            return false;
        }
        let moles = delta * air_volume / (env_temperature * R_IDEAL_GAS_EQUATION) * scale;
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

/// `pressure_valve/process_atmos`, `temperature_gate/process_atmos`: whether gas flows now
/// (`is_gas_flowing`, which tg leaves as it was when the release moves nothing).
pub fn conditional_gate_step(net: &mut Pipenet, id: DeviceId, gate: &ConditionalGate) -> bool {
    let pipenet = net.pipeline_air(id, 1).cloned();
    let Some(airs) = net.airs_mut(id) else {
        return gate.flowing;
    };
    let [input, output] = airs else {
        return gate.flowing;
    };
    let open = match gate.condition {
        GateCondition::Pressure { target } => input.pressure() > target,
        GateCondition::Temperature { target, inverted } => {
            if inverted {
                input.temperature() > target
            } else {
                input.temperature() < target
            }
        }
    };
    if !open {
        return false;
    }
    let target = input.pressure();
    let moved = release_moles(
        &*input,
        &*output,
        target,
        1.0,
        pipenet.as_ref().map(|p| p as &dyn Mix),
    )
    .and_then(|moles| input.remove(moles));
    match moved {
        Some(removed) => {
            output.merge(&removed);
            net.update_parents(id);
            true
        }
        None => gate.flowing,
    }
}

/// `temperature_pump/process_atmos`: 90% of each side exchange heat, input to output only.
pub fn temperature_pump_step(net: &mut Pipenet, id: DeviceId, heat_transfer_rate: f64) -> bool {
    let Some(airs) = net.airs_mut(id) else {
        return false;
    };
    let [input, output] = airs else {
        return false;
    };
    if ss13_atmos::gas::quantize(input.total_moles()) == 0.0
        || ss13_atmos::gas::quantize(output.total_moles()) == 0.0
    {
        return false;
    }
    let mut take_in = input.remove_ratio(0.9);
    let mut take_out = output.remove_ratio(0.9);
    let delta = take_in.temperature - take_out.temperature;
    let moved = delta > 0.0;
    if moved {
        let input_capacity = take_in.heat_capacity_raw();
        let output_capacity = take_out.heat_capacity_raw();
        let heat = heat_transfer_rate
            * 0.01
            * ss13_atmos::gas::conduction_energy(delta, output_capacity, input_capacity);
        take_out.temperature =
            (take_out.temperature + heat / output_capacity).max(ss13_atmos::gas::TCMB);
        take_in.temperature =
            (take_in.temperature - heat / input_capacity).max(ss13_atmos::gas::TCMB);
    }
    input.merge(&take_in);
    output.merge(&take_out);
    if moved {
        net.update_parents(id);
    }
    moved
}

/// `binary/volume_pump/process_atmos`. Overclocked, the output limit is off and
/// `VOLUME_PUMP_LEAK_AMOUNT` of what moves is returned to leak onto the tile.
pub fn volume_pump_step(
    net: &mut Pipenet,
    id: DeviceId,
    transfer_rate: f64,
    overclocked: bool,
) -> Option<GasMixture> {
    let airs = net.airs_mut(id)?;
    let [input, output] = airs else {
        return None;
    };
    // DM precedence: `a < MIN || (b > MAX && !overclocked)`.
    if input.pressure() < VOLUME_PUMP_MIN_INPUT
        || (output.pressure() > VOLUME_PUMP_MAX_OUTPUT && !overclocked)
    {
        return None;
    }
    let mut removed = input.remove_ratio(transfer_rate / input.volume);
    if removed.total_moles() == 0.0 {
        return None;
    }
    let leaked = overclocked.then(|| removed.remove_ratio(VOLUME_PUMP_LEAK_AMOUNT));
    output.merge(&removed);
    net.update_parents(id);
    Some(leaked.unwrap_or_else(GasMixture::vacuum))
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
/// Returns the heat moved (J, `* THERMOMACHINE_POWER_CONVERSION`), `None` with an empty port
/// (tg returns before `use_energy`).
pub fn thermo_step(net: &mut Pipenet, id: DeviceId, machine: &ThermoMachine) -> Option<f64> {
    let port = net.air_mut(id, 0)?;
    if port.total_moles() == 0.0 {
        return None;
    }
    let capacity = port.heat_capacity();
    let delta = machine.target_temperature - port.temperature();
    let heat = conduction_energy(delta, capacity, machine.heat_capacity);
    port.gas.temperature = ((port.gas.temperature * capacity + heat) / capacity).max(TCMB);
    net.update_parents(id);
    Some(heat.abs().min(1e8) * THERMOMACHINE_POWER_CONVERSION)
}

/// `process_atmos`: `idle_power_usage + (heat_amount * 0.05) ** (1.05 - (5e7 * 0.16 /
/// max(heat_amount, 5e7)))` joules for one fire.
pub fn thermo_energy(heat: f64, idle: f64) -> f64 {
    idle + (heat * 0.05).powf(1.05 - 5e7 * 0.16 / heat.max(5e7))
}

/// `use_energy(power_usage)` of the fire's thermomachines.
pub fn draw_thermo_energy(world: &mut World) {
    let mut q = world.query::<(Entity, &mut ThermoMachine)>();
    let due: Vec<(Entity, f64)> = q
        .iter_mut(world)
        .filter(|(_, t)| t.energy_due > 0.0)
        .map(|(e, mut t)| (e, std::mem::take(&mut t.energy_due)))
        .collect();
    for (machine, joules) in due {
        crate::power::machine_use_energy(world, machine, joules, false);
    }
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
    mut vents: Query<(
        &AtmosDevice,
        &TilePos,
        &mut VentPump,
        Has<Unpowered>,
        Option<&crate::obj_damage::Integrity>,
        Option<&ss13_mapload::DmObj>,
    )>,
) {
    for (device, &pos, mut vent, unpowered, integrity, obj) in &mut vents {
        if unpowered {
            continue;
        }
        if !connected(&net, device.id, 0) && vent.on {
            vent.on = false;
        }
        if !vent.on || vent.welded {
            continue;
        }
        let percent = match (integrity, obj) {
            (Some(i), Some(o)) if o.max_integrity > 0.0 => {
                (f64::from(i.current / o.max_integrity) * 100.0).round() / 100.0
            }
            _ => 1.0,
        };
        vent_step(&mut net, &mut atmos, device.id, pos, &vent, percent);
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
        &'static TilePos,
        Option<&'static GasPump>,
        Option<&'static mut VolumePump>,
        Option<&'static PassiveGate>,
        Option<&'static mut Valve>,
        Option<&'static mut ConditionalGate>,
        Option<&'static TemperaturePump>,
        Has<Unpowered>,
    ),
    Or<(
        With<GasPump>,
        With<VolumePump>,
        With<PassiveGate>,
        With<Valve>,
        With<ConditionalGate>,
        With<TemperaturePump>,
    )>,
>;

pub fn process_binary(
    mut net: ResMut<Pipenet>,
    mut atmos: ResMut<Atmos>,
    time: Res<Time>,
    mut machines: BinaryQuery,
) {
    let now = time.elapsed_secs_f64();
    for (device, &pos, pump, volume, gate, valve, conditional, heat_pump, unpowered) in
        &mut machines
    {
        let id = device.id;
        if let Some(mut c) = conditional {
            let powered = !unpowered || matches!(c.condition, GateCondition::Pressure { .. });
            if c.on && powered {
                let flowing = conditional_gate_step(&mut net, id, &c);
                c.set_if_neq(ConditionalGate {
                    flowing,
                    ..c.clone()
                });
            }
            continue;
        }
        if let Some(p) = heat_pump.filter(|p| p.on && !unpowered) {
            temperature_pump_step(&mut net, id, p.heat_transfer_rate);
            continue;
        }
        if let Some(pump) = pump.filter(|p| p.on && !unpowered) {
            pump_step(&mut net, id, pump.target_pressure);
        } else if let Some(mut volume) = volume.filter(|p| p.on && !unpowered) {
            // `isclosedturf(loc)`: jammed, the overclock goes off.
            let mut overclocked = volume.overclocked;
            if overclocked && atmos.air(pos).is_none() {
                overclocked = false;
                volume.set_if_neq(VolumePump {
                    overclocked: false,
                    ..volume.clone()
                });
            }
            if let Some(leaked) = volume_pump_step(&mut net, id, volume.transfer_rate, overclocked)
                && leaked.total_moles() > 0.0
            {
                atmos.merge_air(pos, &leaked);
            }
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
        (
            Option<&'static mut ThermoMachine>,
            Option<&'static ss13_mapload::DmPower>,
        ),
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
    for (device, &pos, (thermo, idle), connector, injector, passive, unpowered) in &mut machines {
        let id = device.id;
        if let Some(mut thermo) = thermo {
            if !thermo.on {
                continue;
            }
            if unpowered {
                thermo.on = false;
                continue;
            }
            if let Some(heat) = thermo_step(&mut net, id, &thermo) {
                thermo.energy_due = thermo_energy(heat, idle.map_or(0.0, |p| p.idle_power_usage));
            }
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
/// its tile every fire: the tile's air, or the turf itself under a wall.
// shortcut: no heating or burning of buckled mobs (there is no buckling to pipes, see
// docs/parity/engineering/structures-furniture.md).
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
/// A heat-exchanging pipe on lava (tg `islava`): it compares 5000 K with its pipeline.
// shortcut: set when the map loads; a turf changed to or from lava later keeps it.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct HeOnLava;

/// `islava`'s `environment_temperature`.
const LAVA_TEMPERATURE: f64 = 5000.0;

pub fn he_step(
    net: &mut Pipenet,
    atmos: &mut Atmos,
    id: DeviceId,
    pos: TilePos,
    he: HePipe,
    lava: bool,
) {
    // A junction conducts nothing; tg still wakes its tile with a zero change.
    if he.thermal_conductivity == 0.0 {
        return;
    }
    let Some(pipe_air) = net.pipeline_air(id, 0) else {
        return;
    };
    // `GetTemperature`, `GetHeatCapacity`: the open turf's air, else a wall's own.
    let (turf_temperature, turf_heat_capacity, solid) = if let Some(air) = atmos.air(pos) {
        (air.temperature, air.heat_capacity(), false)
    } else if let Some((t, c)) = atmos.solid_heat(pos) {
        (t, c, true)
    } else {
        return;
    };
    // Lava only changes what the threshold compares; the exchange uses the turf's air.
    let environment = if lava {
        LAVA_TEMPERATURE
    } else {
        turf_temperature
    };
    if (environment - pipe_air.temperature()).abs() <= he.minimum_temperature_difference {
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
        if solid {
            atmos.take_solid_temperature(pos, delta);
        } else {
            atmos.modify_air(pos, |a| a.temperature += delta);
        }
    }
}

pub fn process_he_pipes(
    mut net: ResMut<Pipenet>,
    mut atmos: ResMut<Atmos>,
    pipes: Query<(&AtmosDevice, &TilePos, &HePipe, Has<HeOnLava>)>,
) {
    for (device, &pos, &he, lava) in &pipes {
        he_step(&mut net, &mut atmos, device.id, pos, he, lava);
    }
}

/// tg `heat_exchanging.icon_temperature`: the temperature the pipe's glow was last drawn at.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct HeGlow(pub f64);

impl Default for HeGlow {
    fn default() -> Self {
        Self(T20C)
    }
}

/// The glow part of tg `heat_exchanging/process`: from 500 K, once the pipeline's temperature
/// has moved more than 10 K from the drawn one, the new color (`heat2colour`, blended from the
/// `#404040` grey up to 2000 K). `None` keeps the current color.
pub fn he_glow(glow: &mut HeGlow, temperature: f64) -> Option<[u8; 4]> {
    if temperature == 0.0 || (glow.0 <= 500.0 && temperature <= 500.0) {
        return None;
    }
    if (temperature - glow.0).abs() <= 10.0 {
        return None;
    }
    glow.0 = temperature;
    let mut rgb = ss13_atmos::heat2colour(temperature);
    if temperature < 2000.0 {
        let scale = (temperature - 500.0) / 1500.0;
        for c in &mut rgb {
            *c = 64.0 + (*c - 64.0) * scale;
        }
    }
    let [r, g, b] = rgb.map(|c| c.round().clamp(0.0, 255.0) as u8);
    Some([r, g, b, 255])
}

/// tg runs the glow in `process` (SSmachines), once per machine tick.
// shortcut: the color is set at once; tg tweens it over 2 s (`animate(..., time = 20)`), the
// client has no color tweens yet.
pub fn glow_he_pipes(
    ticks: Option<Res<crate::power::MachineTicks>>,
    mut last: Local<Option<u64>>,
    net: Res<Pipenet>,
    mut pipes: Query<(&AtmosDevice, &mut HeGlow, &mut ss13_shared::Appearance)>,
) {
    let Some(ticks) = ticks else { return };
    if last.replace(ticks.0) == Some(ticks.0) {
        return;
    }
    for (device, mut glow, mut appearance) in &mut pipes {
        let Some(air) = net.pipeline_air(device.id, 0) else {
            continue;
        };
        if let Some(color) = he_glow(&mut glow, air.temperature()) {
            appearance.color = Some(color);
        }
    }
}
