//! The supermatter crystal (server): tg `code/modules/power/supermatter/` (`supermatter.dm`
//! `process_atmos` and its `calculate_*` procs, `supermatter_gas.dm`, `supermatter_zap`,
//! `handle_high_power`, the delamination strategies' `can_select`, `delam_progress` and
//! `count_down`), `code/__DEFINES/supermatter.dm`.
//!
//! Every SSair fire ([`ss13_atmos::FIRE_INTERVAL`]) the crystal breathes: it takes
//! `absorption_ratio` of its tile's air ([`Crystal::process_atmos`], pure and unit-tested):
//! gas composition sets the heat/power/transmission factors, energy grows from emitter hits
//! and hot oxygen-rich gas and decays cubically, damage follows heat, energy, moles and space
//! exposure, and the waste (heat, plasma, oxygen) goes back into the tile. Its energy
//! accumulates as zap energy (`internal_energy * 1040 W/MeV * gas bonus` per second) that it
//! discharges in arcs ([`zap`]): tesla coils keep a quarter of each arc for their powernet
//! ([`super::accumulator`]), grounding rods swallow the rest.
//!
//! Damage over 5 starts radio warnings on the engineering channel (common in an emergency);
//! over 100 a 15 s countdown, then the delamination.
//!
//! Cost: one crystal per station, 2 Hz; a zap scans the 7x7..15x15 box around each hop.

use std::collections::HashMap;

use bevy::prelude::*;
use ss13_atmos::gas::{CELL_VOLUME, R_IDEAL_GAS_EQUATION, T0C, TCMB, byond_round};
use ss13_atmos::{Atmos, GAS_COUNT, GasId, GasMixture};
use ss13_chat::radio::{FREQ_COMMON, FREQ_ENGINEERING};
use ss13_mapload::fidelity::{IconIndex, is_under};
use ss13_mapload::{DmAtom, DmItem, DmName, DmObj};
use ss13_net::chat::server::SendChat;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_shared::{Appearance, IconStateId, LightSource, Optics, Overlay, TilePos};
use ss13_world::SpatialIndex;

use super::accumulator::{Accumulator, ZAP_GENERATES_POWER};
use super::{EngineRng, Grid, MACHINE_TICK_SECS, MachineTicks};
use crate::combat::CombatRng;
use crate::health::{DamageType, Health, Stat, apply_damage};
use crate::radio::MachineRadio;

pub const SM_ROOT: &str = "/obj/machinery/power/supermatter_crystal";
pub const SM_ENGINE: &str = "/obj/machinery/power/supermatter_crystal/engine";
pub const SM_SHARD: &str = "/obj/machinery/power/supermatter_crystal/shard";

/// `MINIMUM_MOLE_COUNT`.
pub const MINIMUM_MOLE_COUNT: f64 = 0.01;
pub const MOLE_PENALTY_THRESHOLD: f64 = 1800.0;
pub const POWER_PENALTY_THRESHOLD: f64 = 5000.0;
pub const SEVERE_POWER_PENALTY_THRESHOLD: f64 = 7000.0;
pub const CRITICAL_POWER_PENALTY_THRESHOLD: f64 = 9000.0;
pub const HEAT_PENALTY_THRESHOLD: f64 = 40.0;
pub const THERMAL_RELEASE_MODIFIER: f64 = 4.0;
pub const PLASMA_RELEASE_MODIFIER: f64 = 650.0;
pub const OXYGEN_RELEASE_MODIFIER: f64 = 340.0;
pub const REACTION_POWER_MODIFIER: f64 = 0.65;
pub const MATTER_POWER_CONVERSION: f64 = 10.0;
pub const GAS_HEAT_POWER_SCALING_COEFFICIENT: f64 = 1.0 / 6.0;
/// W/MeV.
pub const BASE_POWER_TRANSMISSION_RATE: f64 = 1040.0;
pub const POWERLOSS_CUBIC_DIVISOR: f64 = 500.0;
pub const POWERLOSS_LINEAR_RATE: f64 = 0.83;
pub const ZAP_ENERGY_DISCHARGE_PORTION: f64 = 0.1;
pub const SUPERMATTER_DEFAULT_BULLET_ENERGY: f64 = 2.0;
/// kPa.
const ONE_ATMOSPHERE: f64 = ss13_atmos::gas::ONE_ATMOSPHERE;
const CO2_CONSUMPTION_PP: f64 = ONE_ATMOSPHERE * 0.01;
const CO2_PRESSURE_SCALING: f64 = ONE_ATMOSPHERE * 0.25;
const MIASMA_CONSUMPTION_PP: f64 = ONE_ATMOSPHERE * 0.01;
const MIASMA_PRESSURE_SCALING: f64 = ONE_ATMOSPHERE * 0.5;
const MIASMA_GASMIX_SCALING: f64 = 0.3;
const MIASMA_POWER_GAIN: f64 = 10.0;
/// `SUPERMATTER_WARNING_DELAY`, `SUPERMATTER_COMMON_RADIO_DELAY`, seconds.
const WARNING_DELAY: f64 = 60.0;
const COMMON_RADIO_DELAY: f64 = 360.0;
const FAST_HEALING_RATE: f64 = 0.025;
/// `SUPERMATTER_COUNTDOWN_TIME`, deciseconds.
const COUNTDOWN_DS: i32 = 150;
/// `ZAP_*` flags.
pub const ZAP_MACHINE_EXPLOSIVE: u32 = 1 << 0;
pub const ZAP_OBJ_DAMAGE: u32 = 1 << 2;
pub const ZAP_MOB_DAMAGE: u32 = 1 << 3;
pub const ZAP_MOB_STUN: u32 = 1 << 4;
pub const ZAP_SUPERMATTER_FLAGS: u32 = ZAP_GENERATES_POWER;
/// `SUPERMATTER_COLOUR`, `SUPERMATTER_RED`, `SUPERMATTER_TESLA_COLOUR`,
/// `SUPERMATTER_SINGULARITY_LIGHT_COLOUR`.
const COLOUR: [u8; 3] = [0xFF, 0xD0, 0x4F];
const RED: [u8; 3] = [0xAA, 0x2C, 0x16];
const TESLA_COLOUR: [u8; 3] = [0x00, 0xFF, 0xFF];
const SINGULARITY_LIGHT_COLOUR: [u8; 3] = [0x40, 0x00, 0x60];

/// tg `/datum/sm_gas`: what a gas does to the crystal, per unit of its share of the absorbed
/// moles.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SmGas {
    pub power_transmission: f64,
    pub heat_modifier: f64,
    pub heat_resistance: f64,
    pub heat_power_generation: f64,
    pub powerloss_inhibition: f64,
}

const fn g(
    power_transmission: f64,
    heat_modifier: f64,
    heat_resistance: f64,
    heat_power_generation: f64,
    powerloss_inhibition: f64,
) -> SmGas {
    SmGas {
        power_transmission,
        heat_modifier,
        heat_resistance,
        heat_power_generation,
        powerloss_inhibition,
    }
}

/// `GLOB.sm_gas_behavior`, by [`GasId`]; nitrium, halon and helium have none.
pub const SM_GASES: [SmGas; GAS_COUNT] = [
    g(0.15, 0.0, 0.0, 1.0, 0.0),   // oxygen
    g(0.0, -2.5, 0.0, -1.0, 0.0),  // nitrogen
    g(0.0, 1.0, 0.0, 1.0, 1.0),    // carbon dioxide
    g(0.4, 14.0, 0.0, 1.0, 0.0),   // plasma
    g(-0.25, 11.0, 0.0, 1.0, 0.0), // water vapor
    g(0.3, -14.0, 0.0, -1.0, 0.0), // hyper-noblium
    g(0.0, 0.0, 5.0, 0.0, 0.0),    // nitrous oxide
    g(0.0, 0.0, 0.0, 0.0, 0.0),    // nitrium
    g(3.0, 9.0, 0.0, 1.0, 0.0),    // tritium
    g(-0.2, 4.0, 0.0, 1.0, 0.0),   // bz
    g(-0.5, -1.5, 0.0, -1.0, 0.0), // pluoxium
    g(0.0, 0.0, 0.0, 0.5, 0.0),    // miasma
    g(-3.0, -9.0, 0.0, -1.0, 0.0), // freon
    g(2.5, 9.0, 1.0, 1.0, 0.0),    // hydrogen
    g(0.24, 3.0, 0.0, 1.0, 0.0),   // healium
    g(1.5, -4.0, 4.0, 1.0, 0.0),   // proto-nitrate
    g(2.0, 7.0, 0.0, 1.0, 0.0),    // zauker
    g(0.0, 0.0, 0.0, 0.0, 0.0),    // halon
    g(0.0, 0.0, 0.0, 0.0, 0.0),    // helium
    g(-0.5, 14.0, 0.0, 1.0, 0.0),  // anti-noblium
];

/// `sqrt(POWERLOSS_LINEAR_RATE / 3 * POWERLOSS_CUBIC_DIVISOR ** 3)`: where the cubic decay
/// turns linear, and the offset that keeps it differentiable.
pub fn powerloss_linear_threshold() -> f64 {
    (POWERLOSS_LINEAR_RATE / 3.0 * POWERLOSS_CUBIC_DIVISOR.powi(3)).sqrt()
}

fn powerloss_linear_offset() -> f64 {
    let t = powerloss_linear_threshold();
    -t * POWERLOSS_LINEAR_RATE + (t / POWERLOSS_CUBIC_DIVISOR).powi(3)
}

/// The delamination a crystal would go through now (`GLOB.sm_delam_list`, first that
/// `can_select`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Delam {
    Cascade,
    Singularity,
    Tesla,
    Explosive,
}

impl Delam {
    pub fn name(self) -> &'static str {
        match self {
            Self::Cascade => "resonance cascade",
            Self::Singularity => "singularity delamination",
            Self::Tesla => "tesla delamination",
            Self::Explosive => "explosive delamination",
        }
    }
}

/// `get_status()`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SmStatus {
    Inactive,
    Normal,
    Notify,
    Warning,
    Danger,
    Emergency,
    Delaminating,
}

/// What one `process_atmos` asks the world to do.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Breath {
    /// Waste gas to merge back into the tile (the absorbed air plus the crystal's output).
    pub merged: GasMixture,
    /// The main discharge (`supermatter_zap(range 3, cutoff 240 kJ)`), joules.
    pub zap: Option<f64>,
    /// `handle_high_power` discharges, joules, with their range, flags and cutoff.
    pub high_zaps: Vec<f64>,
    pub high_range: f64,
    pub high_flags: u32,
    /// `damage > explosion_point` and no countdown yet.
    pub start_countdown: bool,
}

/// The crystal's state: everything `process_atmos` reads and writes.
#[derive(Clone, Debug, PartialEq)]
pub struct Crystal {
    pub absorption_ratio: f64,
    /// MeV ("EER").
    pub internal_energy: f64,
    pub damage: f64,
    pub damage_archived: f64,
    /// W/MeV.
    pub zap_transmission_rate: f64,
    /// Kelvin.
    pub temp_limit: f64,
    pub waste_multiplier: f64,
    pub gas_percentage: [f64; GAS_COUNT],
    pub gas_heat_modifier: f64,
    pub gas_heat_resistance: f64,
    pub gas_powerloss_inhibition: f64,
    pub gas_power_transmission_rate: f64,
    pub gas_heat_power_generation: f64,
    pub external_power_trickle: f64,
    pub external_power_immediate: f64,
    pub external_damage_immediate: f64,
    pub psy_coeff: f64,
    pub zap_cutoff: f64,
    pub warning_point: f64,
    pub danger_point: f64,
    pub emergency_point: f64,
    pub explosion_point: f64,
    pub explosion_power: f64,
    pub final_countdown: bool,
    pub is_main_engine: bool,
    /// `zap_energy_accumulation["normal"]` and `["high"]`, joules.
    pub zap_accumulation: [f64; 2],
    /// Seconds.
    pub last_power_zap: f64,
    /// `SSmachines.times_fired` of the last accumulation, normal and high.
    pub last_energy_accumulation: u64,
    pub last_high_energy_accumulation: u64,
    /// The absorbed gas of the last breath (temperature K, moles), for status and monitors.
    pub gas_temperature: f64,
    pub gas_total_moles: f64,
    pub delam: Delam,
    /// `bullet_energy`: MeV per point of beam damage (the Supermatter Surge raises it).
    pub bullet_energy: f64,
    /// `current_gas_behavior[/datum/gas/nitrogen]`: tg's one shared datum, which the
    /// Supermatter Surge changes for its duration (`crate::events::supermatter_surge`).
    pub nitrogen: SmGas,
    /// What the last breath's `calculate_*` procs added up, for the monitor.
    pub factors: SmFactors,
}

/// `SM_POWER_*` (`internal_energy_factors`, MeV), in tg's order.
pub const SM_POWER_FACTORS: [&str; 6] = [
    "Внешнее питание малым током",
    "Внешнее увеличение мощности",
    "Увеличение тепловой мощности газа",
    "Внутреннее снижение мощности",
    "Компенсация спада газовой мощности",
    "Компенсация психологом потери мощности",
];
/// `SM_DAMAGE_*` (`damage_factors`), in tg's order.
pub const SM_DAMAGE_FACTORS: [&str; 6] = [
    "Внешнее повреждение",
    "Термическое повреждение",
    "Повреждение от заряда",
    "Молярное повреждение",
    "Повреждение от воздействия космоса",
    "Восстановление с помощью охлаждения",
];
/// `SM_TEMP_LIMIT_*` (`temp_limit_factors`, K).
pub const SM_TEMP_LIMIT_FACTORS: [&str; 4] = [
    "Базовая термостойкость",
    "Термостойкость к нагреву газа",
    "Психологическая термостойкость",
    "Термостойкость при низком уровне молей",
];
/// `SM_WASTE_*` (`waste_multiplier_factors`).
pub const SM_WASTE_FACTORS: [&str; 3] = [
    "Множитель базовых отходов",
    "Множитель газовых отходов",
    "Множитель отходов (психолог)",
];
/// `SM_ZAP_*` (`zap_factors`, W/MeV).
pub const SM_ZAP_FACTORS: [&str; 2] = [
    "Базовая передача импульса",
    "Модификатор передачи газового импульса",
];

/// The `*_factors` lists of the last breath, by the name tables above.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SmFactors {
    pub internal_energy: [f64; 6],
    pub damage: [f64; 6],
    pub temp_limit: [f64; 4],
    pub waste_multiplier: [f64; 3],
    pub zap: [f64; 2],
}

impl Default for Crystal {
    fn default() -> Self {
        Self {
            absorption_ratio: 0.15,
            internal_energy: 0.0,
            damage: 0.0,
            damage_archived: 0.0,
            zap_transmission_rate: BASE_POWER_TRANSMISSION_RATE,
            temp_limit: T0C + HEAT_PENALTY_THRESHOLD,
            waste_multiplier: 0.0,
            gas_percentage: [0.0; GAS_COUNT],
            gas_heat_modifier: 0.0,
            gas_heat_resistance: 0.0,
            gas_powerloss_inhibition: 0.0,
            gas_power_transmission_rate: 0.0,
            gas_heat_power_generation: 0.0,
            external_power_trickle: 0.0,
            external_power_immediate: 0.0,
            external_damage_immediate: 0.0,
            psy_coeff: 0.0,
            zap_cutoff: 1.2e6,
            warning_point: 5.0,
            danger_point: 60.0,
            emergency_point: 75.0,
            explosion_point: 100.0,
            explosion_power: 35.0,
            final_countdown: false,
            is_main_engine: false,
            zap_accumulation: [0.0; 2],
            last_power_zap: 0.0,
            last_energy_accumulation: 0,
            last_high_energy_accumulation: 0,
            gas_temperature: 0.0,
            gas_total_moles: 0.0,
            delam: Delam::Explosive,
            bullet_energy: SUPERMATTER_DEFAULT_BULLET_ENERGY,
            nitrogen: SM_GASES[GasId::N2.0 as usize],
            factors: SmFactors::default(),
        }
    }
}

/// The absorbed mix's pressure: its volume is the tile's times the ratio taken, so this is the
/// tile's pressure before the breath.
fn absorbed_pressure(absorbed: &GasMixture, ratio: f64) -> f64 {
    if ratio <= 0.0 {
        return 0.0;
    }
    absorbed.total_moles() * R_IDEAL_GAS_EQUATION * absorbed.temperature / (CELL_VOLUME * ratio)
}

impl Crystal {
    /// `calculate_gases`.
    pub fn calculate_gases(&mut self, absorbed: &GasMixture) {
        self.gas_percentage = [0.0; GAS_COUNT];
        self.gas_power_transmission_rate = 0.0;
        self.gas_heat_modifier = 0.0;
        self.gas_heat_resistance = 0.0;
        self.gas_heat_power_generation = 0.0;
        self.gas_powerloss_inhibition = 0.0;
        let total = absorbed.total_moles();
        if total < MINIMUM_MOLE_COUNT {
            return;
        }
        for (i, &moles) in absorbed.moles.iter().enumerate() {
            if moles < MINIMUM_MOLE_COUNT {
                continue;
            }
            let pct = moles / total;
            self.gas_percentage[i] = pct;
            let gas = if i == GasId::N2.0 as usize {
                self.nitrogen
            } else {
                SM_GASES[i]
            };
            self.gas_power_transmission_rate += gas.power_transmission * pct;
            self.gas_heat_modifier += gas.heat_modifier * pct;
            self.gas_heat_resistance += gas.heat_resistance * pct;
            self.gas_heat_power_generation += gas.heat_power_generation * pct;
            self.gas_powerloss_inhibition += gas.powerloss_inhibition * pct;
        }
        self.gas_heat_power_generation = self.gas_heat_power_generation.clamp(0.0, 1.0);
        self.gas_powerloss_inhibition = self.gas_powerloss_inhibition.clamp(0.0, 1.0);
    }

    /// The `extra_effects` that touch the gas: CO2 + O2 -> pluoxium, miasma eaten for power.
    // shortcut: BZ's nuclear particles and zauker's stun arcs are not modelled (no radiation,
    // and zauker arcs carry no power).
    pub fn extra_effects(&mut self, absorbed: &mut GasMixture, ratio: f64) {
        let pressure = absorbed_pressure(absorbed, ratio);
        let co2 = self.gas_percentage[usize::from(GasId::CO2.0)];
        let o2 = self.gas_percentage[usize::from(GasId::O2.0)];
        if co2 > 0.0 && o2 > 0.0 {
            let co2_pp = pressure * co2;
            let co2_ratio = (0.5 * (co2_pp - CO2_CONSUMPTION_PP) / (co2_pp + CO2_PRESSURE_SCALING))
                .clamp(0.0, 1.0);
            let consumed = (absorbed.get(GasId::CO2) * co2_ratio)
                .min(absorbed.get(GasId::CO2))
                .min(absorbed.get(GasId::O2));
            if consumed != 0.0 {
                absorbed.add(GasId::CO2, -consumed);
                absorbed.add(GasId::O2, -consumed);
                absorbed.add(GasId::PLUOXIUM, consumed);
            }
        }
        let miasma = self.gas_percentage[usize::from(GasId::MIASMA.0)];
        if miasma > 0.0 {
            let pp = pressure * miasma;
            let ratio = (((pp - MIASMA_CONSUMPTION_PP) / (pp + MIASMA_PRESSURE_SCALING))
                * (1.0 + self.gas_heat_power_generation * MIASMA_GASMIX_SCALING))
                .clamp(0.0, 1.0);
            let consumed = absorbed.get(GasId::MIASMA) * ratio;
            if consumed != 0.0 {
                absorbed.add(GasId::MIASMA, -consumed);
                self.external_power_trickle += consumed * MIASMA_POWER_GAIN;
            }
        }
    }

    /// `calculate_internal_energy`.
    pub fn calculate_internal_energy(&mut self, absorbed: &GasMixture, ticks: u64, now: f64) {
        let trickle = if self.external_power_trickle != 0.0 {
            (self.external_power_trickle / MATTER_POWER_CONVERSION).max(40.0)
        } else {
            0.0
        };
        self.external_power_trickle -= trickle.min(self.external_power_trickle);
        let immediate = std::mem::take(&mut self.external_power_immediate);
        let heat = self.gas_heat_power_generation
            * absorbed.temperature
            * GAS_HEAT_POWER_SCALING_COEFFICIENT;
        let momentary = self.internal_energy + trickle + immediate + heat;
        let powerloss = if momentary < powerloss_linear_threshold() {
            -(momentary / POWERLOSS_CUBIC_DIVISOR).powi(3)
        } else {
            -(momentary * POWERLOSS_LINEAR_RATE + powerloss_linear_offset())
        };
        let gas = -self.gas_powerloss_inhibition * powerloss;
        let soothed = -(1.0 - self.gas_powerloss_inhibition).min(0.2 * self.psy_coeff) * powerloss;
        self.factors.internal_energy = [trickle, immediate, heat, powerloss, gas, soothed];
        self.internal_energy =
            (self.internal_energy + trickle + immediate + heat + powerloss + gas + soothed)
                .max(0.0);
        if self.internal_energy == 0.0 {
            self.last_power_zap = now;
            self.last_energy_accumulation = ticks;
        }
    }

    /// `calculate_zap_transmission_rate`.
    pub fn calculate_zap_transmission_rate(&mut self) {
        let gas = BASE_POWER_TRANSMISSION_RATE * self.gas_power_transmission_rate;
        self.factors.zap = [BASE_POWER_TRANSMISSION_RATE, gas];
        self.zap_transmission_rate = (BASE_POWER_TRANSMISSION_RATE + gas).max(0.0);
    }

    /// `calculate_temp_limit`.
    pub fn calculate_temp_limit(&mut self, total_moles: f64) {
        let base = T0C + HEAT_PENALTY_THRESHOLD;
        let factors = [
            base,
            self.gas_heat_resistance * base,
            self.psy_coeff * 45.0,
            (2.0 - total_moles / 100.0).clamp(0.0, 1.0) * base,
        ];
        self.factors.temp_limit = factors;
        self.temp_limit = factors.iter().sum::<f64>().max(TCMB);
    }

    /// `calculate_damage`.
    pub fn calculate_damage(&mut self, absorbed: &GasMixture, space_exposed: bool) {
        let total = absorbed.total_moles();
        let external = self.external_damage_immediate
            * ((self.emergency_point - self.damage) / self.emergency_point).clamp(0.0, 1.0);
        self.external_damage_immediate = 0.0;
        let heat = ((absorbed.temperature - self.temp_limit) / 24000.0).clamp(0.0, 0.15);
        let power = ((self.internal_energy - POWER_PENALTY_THRESHOLD) / 40000.0).clamp(0.0, 0.1);
        let moles = ((total - MOLE_PENALTY_THRESHOLD) / 3200.0).clamp(0.0, 0.1);
        let spaced = if space_exposed {
            (self.internal_energy * 0.000125).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let heal = if total > 0.0 && !space_exposed {
            ((absorbed.temperature - self.temp_limit) / 6000.0).clamp(-0.1, 0.0)
        } else {
            0.0
        };
        self.factors.damage = [external, heat, power, moles, spaced, heal];
        self.damage = (self.damage + external + heat + power + moles + spaced + heal).max(0.0);
    }

    /// `calculate_waste_multiplier`.
    pub fn calculate_waste_multiplier(&mut self) {
        self.factors.waste_multiplier = [1.0, self.gas_heat_modifier, -0.2 * self.psy_coeff];
        self.waste_multiplier = (1.0 + self.gas_heat_modifier - 0.2 * self.psy_coeff).max(0.5);
    }

    /// The first delamination that `can_select`.
    pub fn select_delam(&self, total_moles: f64) -> Delam {
        let pct = |g: GasId| self.gas_percentage[usize::from(g.0)];
        if self.is_main_engine
            && total_moles >= MOLE_PENALTY_THRESHOLD * self.absorption_ratio
            && pct(GasId::ANTINOBLIUM) >= 0.4
            && pct(GasId::HYPERNOBLIUM) >= 0.4
        {
            Delam::Cascade
        } else if total_moles >= MOLE_PENALTY_THRESHOLD {
            Delam::Singularity
        } else if self.internal_energy > POWER_PENALTY_THRESHOLD {
            Delam::Tesla
        } else {
            Delam::Explosive
        }
    }

    /// `get_status()`.
    pub fn status(&self) -> SmStatus {
        if self.final_countdown {
            SmStatus::Delaminating
        } else if self.damage >= self.emergency_point {
            SmStatus::Emergency
        } else if self.damage >= self.danger_point {
            SmStatus::Danger
        } else if self.damage >= self.warning_point {
            SmStatus::Warning
        } else if self.gas_temperature > self.temp_limit * 0.8 {
            SmStatus::Notify
        } else if self.internal_energy > 0.0 {
            SmStatus::Normal
        } else {
            SmStatus::Inactive
        }
    }

    /// `get_integrity_percent()`.
    pub fn integrity_percent(&self) -> f64 {
        (100.0 - self.damage / self.explosion_point * 100.0).max(0.0)
    }

    /// `emit_radiation`'s pulse (range 8): (threshold, chance percent). Power raises it,
    /// integrity loss raises the power floor, the chance and the reach through walls.
    pub fn radiation(&self) -> (f64, f64) {
        // `MAX_ACCEPTED_POWER_OUTPUT`, `THRESHOLD_EQUATION_SLOPE`, `INTEGRITY_EXPONENTIAL_DEGREE`,
        // `INTEGRITY_MAX_POWER_NUDGE`, `INTEGRITY_MIN_NUDGABLE_AMOUNT`,
        // `RADIATION_CHANCE_AT_FULL_INTEGRITY`, `RADIATION_CHANCE_AT_ZERO_INTEGRITY`.
        const MAX_POWER: f64 = 5000.0;
        const SLOPE: f64 = -1.0 / MAX_POWER;
        const DEGREE: i32 = 2;
        const MAX_NUDGE: f64 = 1500.0;
        const MIN_NUDGABLE: f64 = 0.7;
        const CHANCE_FULL: f64 = 0.03;
        const CHANCE_ZERO: f64 = 0.4;
        let integrity = 1.0 - (self.damage / self.explosion_point).clamp(0.0, 1.0);
        let nudge_t = ((integrity - MIN_NUDGABLE) / (1.0 - MIN_NUDGABLE)).clamp(0.0, 1.0);
        // `LERP(INTEGRITY_MAX_POWER_NUDGE, 0, t)`.
        let nudge = MAX_NUDGE + (0.0 - MAX_NUDGE) * nudge_t;
        let power = self.internal_energy.min(MAX_POWER).max(nudge);
        let threshold = if integrity == 0.0 {
            if power != 0.0 { 0.0 } else { 1.0 }
        } else if integrity == 1.0 {
            SLOPE * power + 1.0
        } else {
            (SLOPE * power + 1.0).powf((1.0 / integrity).powi(DEGREE))
        };
        let chance = (CHANCE_ZERO - CHANCE_FULL) * (1.0 - integrity) + CHANCE_FULL;
        (threshold, chance * 100.0)
    }

    /// One `process_atmos` on the air already taken from the tile (`ratio` of it).
    pub fn process_atmos(
        &mut self,
        absorbed: &mut GasMixture,
        ratio: f64,
        space_exposed: bool,
        ticks: u64,
        now: f64,
        rng: &mut CombatRng,
    ) -> Breath {
        let mut out = Breath::default();
        // Part 2: gas.
        self.calculate_gases(absorbed);
        self.extra_effects(absorbed, ratio);

        // Part 3: power.
        self.calculate_internal_energy(absorbed, ticks, now);
        self.calculate_zap_transmission_rate();
        let delta_time =
            ticks.saturating_sub(self.last_energy_accumulation) as f64 * MACHINE_TICK_SECS;
        self.zap_accumulation[0] += self.internal_energy * self.zap_transmission_rate * delta_time;
        if self.zap_accumulation[0] != 0.0
            && self.last_power_zap + (4.0 - self.internal_energy * 0.001) < now
        {
            let discharged = ZAP_ENERGY_DISCHARGE_PORTION * self.zap_accumulation[0];
            self.zap_accumulation[0] -= discharged;
            out.zap = Some(discharged);
            self.last_power_zap = now;
        }
        self.last_energy_accumulation = ticks;

        // Part 4: damage.
        let total = absorbed.total_moles();
        self.calculate_temp_limit(total);
        self.damage_archived = self.damage;
        self.calculate_damage(absorbed, space_exposed);
        if self.damage == 0.0 || !self.final_countdown {
            self.delam = self.select_delam(total);
        }
        out.start_countdown = self.damage > self.explosion_point && !self.final_countdown;

        // Part 5: waste.
        self.calculate_waste_multiplier();
        let device_energy = self.internal_energy * REACTION_POWER_MODIFIER;
        let mut merged = absorbed.clone();
        let wm = self.waste_multiplier;
        merged.temperature += device_energy * wm / THERMAL_RELEASE_MODIFIER;
        merged.temperature = merged.temperature.clamp(TCMB, 2500.0 * wm);
        merged.add(
            GasId::PLASMA,
            (device_energy * wm / PLASMA_RELEASE_MODIFIER).max(0.0),
        );
        merged.add(
            GasId::O2,
            ((device_energy + merged.temperature * wm - T0C) / OXYGEN_RELEASE_MODIFIER).max(0.0),
        );
        merged.garbage_collect();
        out.merged = merged;

        // Part 6: high power arcs.
        self.handle_high_power(absorbed, ratio, ticks, rng, &mut out);

        self.gas_temperature = absorbed.temperature;
        self.gas_total_moles = total;
        self.absorption_ratio = (self.absorption_ratio - 0.05).clamp(0.15, 1.0);
        out
    }

    /// `handle_high_power`.
    // shortcut: no anomalies (flux, hallucination, gravitational, pyro) yet.
    fn handle_high_power(
        &mut self,
        absorbed: &GasMixture,
        ratio: f64,
        ticks: u64,
        rng: &mut CombatRng,
        out: &mut Breath,
    ) {
        let e = self.internal_energy;
        if e <= POWER_PENALTY_THRESHOLD && self.damage <= self.danger_point {
            self.last_high_energy_accumulation = ticks;
            return;
        }
        let mut range = 4.0;
        self.zap_cutoff = 1500.0;
        let total = absorbed.total_moles();
        let pressure = absorbed_pressure(absorbed, ratio);
        let temp = absorbed.temperature;
        if pressure > 0.0 && temp > 0.0 {
            self.zap_cutoff = (1.2e6 - (e * total * 40.0) / temp).clamp(1.4e5, 1.2e6);
            range = (e / pressure * 10.0).clamp(2.0, 7.0);
        }
        let mut flags = ZAP_SUPERMATTER_FLAGS;
        let mut zap_count = 0;
        if (POWER_PENALTY_THRESHOLD..=SEVERE_POWER_PENALTY_THRESHOLD).contains(&e) {
            zap_count = 2;
        } else if e > SEVERE_POWER_PENALTY_THRESHOLD && e <= CRITICAL_POWER_PENALTY_THRESHOLD {
            flags |= ZAP_MOB_DAMAGE | ZAP_OBJ_DAMAGE;
            zap_count = 3;
        } else if e > CRITICAL_POWER_PENALTY_THRESHOLD {
            flags |= ZAP_MOB_STUN | ZAP_MACHINE_EXPLOSIVE | ZAP_MOB_DAMAGE | ZAP_OBJ_DAMAGE;
            zap_count = 4;
        }
        if self.damage > self.danger_point && rng.chance(20.0) {
            zap_count += 1;
        }
        if zap_count >= 1 {
            let delta_time =
                ticks.saturating_sub(self.last_high_energy_accumulation) as f64 * MACHINE_TICK_SECS;
            self.zap_accumulation[1] += (e * 3200.0).clamp(6.4e6, 3.2e7) * delta_time;
            if self.zap_accumulation[1] != 0.0 {
                let portion =
                    1.0 - (1.0 - ZAP_ENERGY_DISCHARGE_PORTION).powf(1.0 / f64::from(zap_count));
                for _ in 0..zap_count {
                    let discharged = portion * self.zap_accumulation[1];
                    self.zap_accumulation[1] -= discharged;
                    out.high_zaps.push(discharged);
                }
            }
            self.last_high_energy_accumulation = ticks;
        }
        out.high_range = range;
        out.high_flags = flags;
    }
}

/// A live crystal.
#[derive(Component, Clone, Debug)]
pub struct Supermatter {
    pub crystal: Crystal,
    /// Hugbox variants: never processes (`SM_PROCESS_DISABLED`).
    pub disabled: bool,
    pub name: String,
    /// Seconds (real time in tg; game time here) of the last radio warning.
    pub lastwarning: f64,
    common_radio_at: f64,
    /// The countdown's next step (deciseconds left) and when it runs.
    pub countdown: Option<(i32, f64)>,
    looks: SmLooks,
}

#[derive(Clone, Debug, Default)]
struct SmLooks {
    base: Option<IconStateId>,
    glow: Option<IconStateId>,
    causality: Option<IconStateId>,
    key: Option<(bool, bool)>,
}

impl Supermatter {
    /// `eat_bullets` for a beam (not a `BULLET` armor flag): `damage * bullet_energy` MeV.
    pub fn eat_beam(&mut self, damage: f32) {
        self.crystal.external_power_immediate += f64::from(damage) * self.crystal.bullet_energy;
    }
}

/// When the last SSair fire was processed.
#[derive(Resource, Default)]
struct LastFire(u64);

/// `TRAIT_BEING_SHOCKED` of zapped objects and mobs (1 s), by entity.
#[derive(Resource, Default)]
pub(super) struct Shocked(pub(super) HashMap<Entity, f64>);

/// A crystal delaminated (tests, logs).
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Delaminated {
    pub crystal: Entity,
    pub pos: TilePos,
    pub kind: Delam,
}

pub struct SupermatterServerPlugin;

impl Plugin for SupermatterServerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LastFire>()
            .init_resource::<Shocked>()
            .add_message::<Delaminated>()
            .add_message::<MachineRadio>()
            .add_message::<SendChat>()
            .add_systems(
                ss13_world::MapInit,
                init_supermatter.after(super::init_power),
            )
            .add_systems(
                Update,
                supermatter_tick
                    .after(ss13_atmos::AtmosSystems::Fire)
                    .after(super::PowerSystems),
            );
    }
}

fn init_supermatter(world: &mut World) {
    let index = world.remove_resource::<IconIndex>();
    let mut query = world.query_filtered::<(Entity, &super::DmPower, Option<&DmAtom>, Option<&DmName>), With<ss13_world::Mapped>>();
    let found: Vec<(Entity, Supermatter)> = query
        .iter(world)
        .filter(|(_, dm, ..)| is_under(&dm.path, SM_ROOT))
        .map(|(e, dm, atom, name)| {
            let shard = is_under(&dm.path, SM_SHARD);
            let mut crystal = Crystal {
                is_main_engine: dm.path == SM_ENGINE || dm.path == format!("{SM_SHARD}/engine"),
                ..default()
            };
            if shard {
                crystal.absorption_ratio = 0.125;
                crystal.explosion_power = 12.0;
            }
            let state = |s: &str| {
                let icon = &atom?.icon;
                index.as_ref()?.get(icon, s)
            };
            let base = if shard { "sm_shard" } else { "sm" };
            let looks = SmLooks {
                base: state(base),
                glow: state(&format!("{base}-glow")),
                causality: state("causality_field"),
                key: None,
            };
            (
                e,
                Supermatter {
                    crystal,
                    disabled: dm.path.ends_with("/hugbox"),
                    name: name
                        .map(|n| crate::vending::plain_name(&n.name).to_owned())
                        .unwrap_or_else(|| "supermatter crystal".into()),
                    lastwarning: f64::NEG_INFINITY,
                    common_radio_at: f64::NEG_INFINITY,
                    countdown: None,
                    looks,
                },
            )
        })
        .collect();
    info!("supermatter: {} crystals", found.len());
    world.insert_batch(found);
    if let Some(index) = index {
        world.insert_resource(index);
    }
}

/// DM number text for the integrity (`round(x, 0.01)`): no trailing zeros.
fn dm_number(x: f64) -> String {
    let s = format!("{:.2}", byond_round(x, 0.01));
    s.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// `process_atmos` on every SSair fire, then the countdowns.
fn supermatter_tick(world: &mut World) {
    let fires = world.get_resource::<Atmos>().map_or(0, |a| a.fires);
    let new_fire = fires != world.resource::<LastFire>().0;
    world.resource_mut::<LastFire>().0 = fires;
    let mut query = world.query::<(Entity, &TilePos, &Supermatter)>();
    let crystals: Vec<(Entity, TilePos)> = query
        .iter(world)
        .filter(|(_, _, s)| !s.disabled)
        .map(|(e, &p, _)| (e, p))
        .collect();
    let now = world.resource::<Time>().elapsed_secs_f64();
    for (e, pos) in crystals {
        if new_fire {
            breathe(world, e, pos, now);
        }
        count_down(world, e, pos, now);
    }
}

fn breathe(world: &mut World, e: Entity, pos: TilePos, now: f64) {
    let ticks = world.resource::<MachineTicks>().0;
    // shortcut: tg melts a closed turf under the crystal; here it just stops processing.
    if !world
        .get_resource::<Atmos>()
        .is_some_and(|a| a.air(pos).is_some())
    {
        return;
    }
    let ratio = world
        .get::<Supermatter>(e)
        .expect("listed")
        .crystal
        .absorption_ratio;
    let mut atmos = world.resource_mut::<Atmos>();
    let mut absorbed = atmos.remove_ratio_air(pos, ratio).unwrap_or_default();
    let space = atmos.space_exposed(pos);
    let mut rng = std::mem::take(&mut world.resource_mut::<EngineRng>().0);
    let breath = {
        let mut sm = world.get_mut::<Supermatter>(e).expect("listed");
        sm.crystal
            .process_atmos(&mut absorbed, ratio, space, ticks, now, &mut rng)
    };
    world.resource_mut::<Atmos>().merge_air(pos, &breath.merged);
    let power_level = world
        .get::<Supermatter>(e)
        .expect("listed")
        .crystal
        .internal_energy;
    if let Some(energy) = breath.zap {
        let mut hit = Vec::new();
        zap(
            world,
            &mut rng,
            ZapArc {
                from: e,
                from_pos: pos,
                range: 3,
                energy,
                flags: ZAP_SUPERMATTER_FLAGS,
                cutoff: 240e3,
                power_level,
            },
            &mut hit,
            0,
        );
    }
    let cutoff = world
        .get::<Supermatter>(e)
        .expect("listed")
        .crystal
        .zap_cutoff;
    for energy in breath.high_zaps {
        let mut hit = Vec::new();
        zap(
            world,
            &mut rng,
            ZapArc {
                from: e,
                from_pos: pos,
                range: breath.high_range as u32,
                energy,
                flags: breath.high_flags,
                cutoff,
                power_level,
            },
            &mut hit,
            0,
        );
    }
    world.resource_mut::<EngineRng>().0 = rng;
    // PART 6: `emit_radiation()`.
    let (threshold, chance) = world
        .get::<Supermatter>(e)
        .expect("listed")
        .crystal
        .radiation();
    crate::radiation::radiation_pulse(world, e, 8.0, threshold as f32, chance as f32, 0.0);
    delam_progress(world, e, pos, now);
    if breath.start_countdown {
        start_countdown(world, e, pos, now);
    }
    update_looks(world, e);
}

/// One arc of `supermatter_zap`.
#[derive(Clone, Copy, Debug)]
pub struct ZapArc {
    pub from: Entity,
    pub from_pos: TilePos,
    pub range: u32,
    /// Joules.
    pub energy: f64,
    pub flags: u32,
    pub cutoff: f64,
    pub power_level: f64,
}

/// Arc target classes, best first (`BIKE` has no counterpart here).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Target {
    Coil,
    Rod,
    Living,
    Machinery,
    Object,
}

/// `supermatter_zap`: picks the best target class in range (coils 70%, rods, living 20%,
/// machines 40%, any object), hits it and arcs on with what is left.
// shortcut: range only, no line of sight (`oview`); mobs take the shock without gloves or
// stun; machines and objects do not take damage or explode.
pub fn zap(world: &mut World, rng: &mut CombatRng, arc: ZapArc, hit: &mut Vec<Entity>, depth: u32) {
    let ZapArc {
        mut energy,
        flags,
        cutoff,
        power_level,
        ..
    } = arc;
    if energy <= 0.0 || energy < cutoff || depth > 32 {
        return;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    let grid_wired = |world: &World, e: Entity| world.resource::<Grid>().node_net(e).is_some();
    let mut best: Option<Target> = None;
    let mut pool: Vec<Entity> = Vec::new();
    let candidates: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .in_radius(arc.from_pos, arc.range)
        .filter(|&t| t != arc.from && !hit.contains(&t))
        .collect();
    let shocked = |world: &World, t: Entity| {
        world
            .get::<Accumulator>(t)
            .map(|a| a.shocked_at)
            .or_else(|| world.resource::<Shocked>().0.get(&t).copied())
            .is_some_and(|at| now - at < 1.0)
    };
    for t in candidates {
        let anchored = world
            .get::<crate::obj_damage::Anchored>(t)
            .is_some_and(|a| a.0);
        let panel_open = world
            .get::<crate::construction::PanelOpen>(t)
            .is_some_and(|p| p.0);
        let class = if let Some(acc) = world.get::<Accumulator>(t) {
            let coil = matches!(
                acc.kind,
                super::accumulator::AccumulatorKind::TeslaCoil { .. }
            );
            match (coil, anchored && !panel_open) {
                (true, true) if !shocked(world, t) && rng.chance(70.0) => Some(Target::Coil),
                (false, true) => Some(Target::Rod),
                _ => None,
            }
        } else if let Some(stat) = world.get::<Stat>(t) {
            (world.get::<Health>(t).is_some()
                && *stat != Stat::Dead
                && !shocked(world, t)
                && rng.chance(20.0))
            .then_some(Target::Living)
        } else if let Some(obj) = world.get::<DmObj>(t) {
            if shocked(world, t) {
                None
            } else if is_under(&obj.path, "/obj/machinery") {
                rng.chance(40.0)
                    .then_some(Target::Machinery)
                    .or(Some(Target::Object))
            } else {
                Some(Target::Object)
            }
        } else if world.get::<DmItem>(t).is_some() && !shocked(world, t) {
            Some(Target::Object)
        } else {
            None
        };
        let Some(class) = class else {
            continue;
        };
        match best {
            Some(b) if b < class => {}
            Some(b) if b == class => pool.push(t),
            _ => {
                best = Some(class);
                pool = vec![t];
            }
        }
    }
    let Some(class) = best else {
        return;
    };
    let target = pool[rng.range(0, pool.len() as u32 - 1) as usize];
    hit.push(target);
    let Some(&target_pos) = world.get::<TilePos>(target) else {
        return;
    };
    match class {
        Target::Coil | Target::Rod => {
            let multi = if power_level >= CRITICAL_POWER_PENALTY_THRESHOLD {
                4.0
            } else if power_level >= SEVERE_POWER_PENALTY_THRESHOLD {
                2.0
            } else {
                1.0
            };
            if flags & ZAP_SUPERMATTER_FLAGS != 0 {
                let wired = grid_wired(world, target);
                let mut acc = world.get_mut::<Accumulator>(target).expect("classified");
                let remaining = acc.zap(energy * multi, flags, wired, now);
                energy = remaining / multi;
            } else {
                energy /= 3.0;
            }
        }
        Target::Living => {
            world.resource_mut::<Shocked>().0.insert(target, now);
            let damage = if flags & ZAP_MOB_DAMAGE != 0 {
                (power_level / 200.0 - 10.0) as f32
            } else {
                rng.range(5, 10) as f32
            };
            if let Ok(mut entity) = world.get_entity_mut(target)
                && let Ok((mut health, mut stat)) =
                    entity.get_components_mut::<(&mut Health, &mut Stat)>()
            {
                apply_damage(&mut health, &mut stat, DamageType::Burn, damage.max(0.0));
            }
            energy /= 1.5;
        }
        Target::Machinery | Target::Object => {
            let mut shocked = world.resource_mut::<Shocked>();
            shocked.0.retain(|_, at| now - *at < 1.0);
            shocked.0.insert(target, now);
            // `/obj/structure` and `/obj/machinery` `zap_act` take 0.05% off, `/obj` halves.
            if class == Target::Machinery
                || world
                    .get::<DmObj>(target)
                    .is_some_and(|o| is_under(&o.path, "/obj/structure"))
            {
                energy -= energy * 5e-4;
            }
            energy /= 2.0;
        }
    }
    // The target's air: its pressure, then `electrolyze(zap_str / 200, power_level)`.
    let pressure = world
        .get_resource_mut::<Atmos>()
        .and_then(|mut a| {
            a.modify_air(target_pos, |air| {
                let pressure = air.pressure();
                air.electrolyze(energy / 200.0, Some(power_level));
                pressure
            })
        })
        .unwrap_or(1.0)
        .max(1.0);
    let range = (energy / pressure * 10.0).clamp(2.0, 7.0) as u32;
    let mut count = 1;
    if rng.chance(5.0) {
        energy -= energy / 10.0;
        count = 2;
    }
    for _ in 0..count {
        let mut child = hit.clone();
        zap(
            world,
            rng,
            ZapArc {
                from: target,
                from_pos: target_pos,
                range,
                energy,
                flags,
                cutoff,
                power_level,
            },
            if count > 1 { &mut child } else { hit },
            depth + 1,
        );
    }
}

fn talk(world: &mut World, e: Entity, pos: TilePos, freq: u16, text: String) {
    let name = world
        .get::<Supermatter>(e)
        .map_or_else(|| "supermatter crystal".into(), |s| s.name.clone());
    info!(target: "engine", "{name} ({freq}): {text}");
    world.write_message(MachineRadio {
        freq,
        z: pos.z,
        name,
        text,
    });
}

/// `delam_progress` of the current strategy.
fn delam_progress(world: &mut World, e: Entity, pos: TilePos, now: f64) {
    let Some(mut sm) = world.get_mut::<Supermatter>(e) else {
        return;
    };
    let c = sm.crystal.clone();
    if c.damage <= c.warning_point || now - sm.lastwarning < WARNING_DELAY {
        return;
    }
    sm.lastwarning = now;
    let integrity = dm_number(c.integrity_percent());
    let (warning, emergency) = (FREQ_ENGINEERING, FREQ_COMMON);
    let safe = format!(
        "Кристаллическая гиперструктура возвращается к безопасным рабочим параметрам. Целостность: {integrity}%"
    );
    if c.damage_archived - c.damage > FAST_HEALING_RATE && c.damage_archived >= c.emergency_point {
        // `should_alert_common`: common once per cooldown, else engineering.
        let freq = if now >= sm.common_radio_at {
            sm.common_radio_at = now + COMMON_RADIO_DELAY;
            emergency
        } else {
            warning
        };
        talk(world, e, pos, freq, safe);
        return;
    }
    if c.damage >= c.emergency_point {
        sm.lastwarning = now - WARNING_DELAY / 2.0;
        talk(
            world,
            e,
            pos,
            emergency,
            format!("РАССЛОЕНИЕ КРИСТАЛЛА НЕИЗБЕЖНО! Целостность: {integrity}%"),
        );
    } else if c.damage_archived > c.damage {
        talk(world, e, pos, warning, safe);
        return;
    } else {
        talk(
            world,
            e,
            pos,
            warning,
            format!(
                "Опасность! Нарушается целостность кристаллической гиперструктуры! Целостность: {integrity}%"
            ),
        );
    }
    let channel = if c.damage > c.emergency_point {
        emergency
    } else {
        warning
    };
    let extra = match c.delam {
        Delam::Singularity => {
            Some("Предупреждение: достигнута критическая масса охлаждающего вещества.")
        }
        Delam::Tesla => Some("ОПАСНОСТЬ: ПРОИСХОДИТ ЦЕПНАЯ РЕАКЦИЯ ИНЕРЦИИ ЗАРЯДА."),
        // shortcut: the cascade's station announcement and its effects are not modelled.
        Delam::Cascade => Some("ОПАСНОСТЬ: ЧАСТОТА КОЛЕБАНИЙ ГИПЕРСТРУКТУРЫ ВЫШЛА ЗА ГРАНИЦЫ."),
        Delam::Explosive => None,
    };
    if let Some(text) = extra {
        talk(world, e, pos, channel, text.into());
    }
}

/// `count_down` starts: the first message and the 15 s timer.
fn start_countdown(world: &mut World, e: Entity, pos: TilePos, now: f64) {
    let Some(mut sm) = world.get_mut::<Supermatter>(e) else {
        return;
    };
    if sm.crystal.final_countdown {
        return;
    }
    sm.crystal.final_countdown = true;
    sm.countdown = Some((COUNTDOWN_DS, now));
    talk(
        world,
        e,
        pos,
        FREQ_COMMON,
        "РАССЛОЕНИЕ КРИСТАЛЛА НЕИЗБЕЖНО. Целостность суперматерии достигла критического уровня. Аварийное поле на случай дестабилизации активировано.".into(),
    );
}

/// The countdown's steps (one per second; messages every 5 s, then each of the last 5), and
/// the delamination one second after "0...".
fn count_down(world: &mut World, e: Entity, pos: TilePos, now: f64) {
    loop {
        let Some(sm) = world.get::<Supermatter>(e) else {
            return;
        };
        let Some((i, at)) = sm.countdown else {
            return;
        };
        if now < at {
            return;
        }
        if i < 0 {
            delaminate(world, e, pos);
            return;
        }
        let healed = sm.crystal.damage < sm.crystal.explosion_point;
        let message = if healed {
            Some("Кристаллическая гиперструктура возвращается к безопасным рабочим параметрам. Аварийное поле отключено.".to_owned())
        } else if i % 50 != 0 && i > 50 {
            None
        } else if i > 50 {
            let secs = u64::try_from(i / 10).unwrap_or(0);
            Some(format!(
                "{secs} {} для стабилизации ситуации.",
                crate::jobs::declension_ru(secs, "секунда", "секунды", "секунд")
            ))
        } else {
            Some(format!("{}...", i / 10))
        };
        if let Some(text) = message {
            talk(world, e, pos, FREQ_COMMON, text);
        }
        let mut sm = world.get_mut::<Supermatter>(e).expect("checked");
        if healed {
            sm.crystal.final_countdown = false;
            sm.countdown = None;
            return;
        }
        sm.countdown = Some((i - 10, at + 1.0));
    }
}

/// `delaminate`: [`Delaminated`] makes the singularity or the tesla ball
/// (`super::singularity`, `super::tesla`).
// shortcut: no explosions, delamination radiation, hallucinations or anomalies yet: the crystal
// is announced as gone to everyone on its level and deleted (tg `qdel(sm)` ends every strategy).
fn delaminate(world: &mut World, e: Entity, pos: TilePos) {
    let kind = world
        .get::<Supermatter>(e)
        .map_or(Delam::Explosive, |s| s.crystal.delam);
    warn!(target: "engine", "supermatter at {pos:?} triggered a {}", kind.name());
    let clients = crate::construction::clients_near(world, pos, 255, &[]);
    world.write_message(SendChat {
        clients,
        message: ChatMessage {
            channel: ChatChannel::System,
            segments: vec![ChatSegment::new(
                Span::Danger,
                "Вы слышите оглушительный грохот: кристалл суперматерии расслоился!",
            )],
        },
    });
    world.write_message(Delaminated {
        crystal: e,
        pos,
        kind,
    });
    world.despawn(e);
}

/// tg `update_icon` (`-glow` above 0.8 heat power), the causality field while counting down,
/// and `lights()`.
fn update_looks(world: &mut World, e: Entity) {
    let Some(sm) = world.get::<Supermatter>(e) else {
        return;
    };
    let c = &sm.crystal;
    let glow = c.gas_heat_power_generation > 0.8;
    let key = (glow, c.final_countdown);
    let looks = sm.looks.clone();
    let light = (c.internal_energy > 0.0).then(|| {
        let color = match c.delam {
            Delam::Tesla => TESLA_COLOUR,
            Delam::Singularity => SINGULARITY_LIGHT_COLOUR,
            _ if glow => RED,
            _ => COLOUR,
        };
        (
            (c.internal_energy / 500.0).clamp(4.0, 10.0).ceil() as f32,
            (c.internal_energy / 1000.0).clamp(1.0, 5.0).ceil() as f32,
            color,
        )
    });
    let final_countdown = c.final_countdown;
    if looks.key != Some(key) {
        world.get_mut::<Supermatter>(e).expect("checked").looks.key = Some(key);
        if let Some(mut appearance) = world.get_mut::<Appearance>(e) {
            if let Some(state) = if glow { looks.glow } else { looks.base } {
                appearance.icon_state = state;
            }
            appearance
                .overlays
                .retain(|o| Some(o.icon_state) != looks.causality);
            if let (true, Some(state)) = (final_countdown, looks.causality) {
                appearance.overlays.push(Overlay::new(state));
            }
        }
    }
    if let Some((range, power, color)) = light
        && let Some(mut optics) = world.get_mut::<Optics>(e)
    {
        let new = optics.light.map(|l| LightSource {
            range,
            power,
            color,
            ..l
        });
        if optics.light != new {
            optics.light = new;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6 * b.abs().max(1.0)
    }

    fn mix(gases: &[(GasId, f64)], temperature: f64) -> GasMixture {
        let mut m = GasMixture::vacuum();
        for &(g, n) in gases {
            m.set(g, n);
        }
        m.temperature = temperature;
        m
    }

    #[test]
    fn radiation_follows_power_and_integrity() {
        // A standard N2 crystal (~1500 MeV) at full integrity: threshold 0.7, stopped by walls.
        let mut c = Crystal {
            internal_energy: 1500.0,
            ..Crystal::default()
        };
        let (threshold, chance) = c.radiation();
        assert!(
            close(threshold, 0.7) && close(chance, 3.0),
            "{threshold} {chance}"
        );
        // Unpowered and broken down to nothing: the nudge keeps it radiating at 0.
        c.internal_energy = 0.0;
        c.damage = c.explosion_point;
        let (threshold, chance) = c.radiation();
        assert!(
            close(threshold, 0.0) && close(chance, 40.0),
            "{threshold} {chance}"
        );
    }

    #[test]
    fn powerloss_threshold_and_offset() {
        // sqrt(0.83 / 3 * 500^3) = 5880.8; offset -5880.8 * 0.83 + (5880.8 / 500)^3.
        let t = powerloss_linear_threshold();
        assert!((t - 5880.76).abs() < 0.01);
        assert!((powerloss_linear_offset() + 3254.02).abs() < 0.01);
        let offset = powerloss_linear_offset();
        assert!((offset - (-t * 0.83 + (t / 500.0).powi(3))).abs() < 1e-9);
        // The two branches meet at the threshold.
        assert!(((t / 500.0).powi(3) - (t * 0.83 + offset)).abs() < 1e-6);
    }

    #[test]
    fn gas_factors_of_room_air_and_n2_o2() {
        // 20% O2, 80% N2: transmission 0.2 * 0.15 = 0.03, heat modifier -2, heat power
        // 0.2 - 0.8 = -0.6 -> 0, no inhibition.
        let mut c = Crystal::default();
        c.calculate_gases(&mix(&[(GasId::O2, 20.0), (GasId::N2, 80.0)], 293.15));
        assert!(close(c.gas_power_transmission_rate, 0.03));
        assert!(close(c.gas_heat_modifier, -2.0));
        assert_eq!(c.gas_heat_power_generation, 0.0);
        assert_eq!(c.gas_powerloss_inhibition, 0.0);
        c.calculate_zap_transmission_rate();
        assert!(close(c.zap_transmission_rate, 1040.0 * 1.03));
        c.calculate_waste_multiplier();
        assert_eq!(c.waste_multiplier, 0.5);
        // Pure plasma: heat power 1, transmission 1.4x, waste 15x.
        c.calculate_gases(&mix(&[(GasId::PLASMA, 10.0)], 500.0));
        assert!(close(c.gas_heat_power_generation, 1.0));
        c.calculate_zap_transmission_rate();
        assert!(close(c.zap_transmission_rate, 1040.0 * 1.4));
        c.calculate_waste_multiplier();
        assert!(close(c.waste_multiplier, 15.0));
        // Under MINIMUM_MOLE_COUNT a gas does not count.
        c.calculate_gases(&mix(&[(GasId::N2, 50.0), (GasId::PLASMA, 0.005)], 293.15));
        assert!(close(c.gas_heat_modifier, -2.5 * 50.0 / 50.005));
    }

    #[test]
    fn energy_from_emitter_hits_and_decay() {
        let mut c = Crystal::default();
        let n2 = mix(&[(GasId::N2, 100.0)], 293.15);
        c.calculate_gases(&n2);
        // One emitter hit: 30 * 2 = 60 MeV in, then cubic decay of the momentary power:
        // 60 - (60 / 500)^3 = 59.998272.
        let mut sm = Supermatter {
            crystal: c,
            disabled: false,
            name: String::new(),
            lastwarning: 0.0,
            common_radio_at: 0.0,
            countdown: None,
            looks: SmLooks::default(),
        };
        sm.eat_beam(30.0);
        sm.crystal.calculate_internal_energy(&n2, 10, 5.0);
        assert!(close(
            sm.crystal.internal_energy,
            60.0 - (60.0f64 / 500.0).powi(3)
        ));
        // 1000 MeV decays by (1000 / 500)^3 = 8 MeV per breath.
        sm.crystal.internal_energy = 1000.0;
        sm.crystal.calculate_internal_energy(&n2, 11, 5.5);
        assert!(close(sm.crystal.internal_energy, 992.0));
        // CO2 inhibits the loss entirely.
        let co2 = mix(&[(GasId::CO2, 100.0)], 293.15);
        sm.crystal.calculate_gases(&co2);
        let before = sm.crystal.internal_energy;
        sm.crystal
            .calculate_internal_energy(&mix(&[(GasId::CO2, 100.0)], 0.0), 12, 6.0);
        // heat power 1 * 0 K adds nothing; loss and its inhibition cancel.
        assert!(close(sm.crystal.internal_energy, before));
        // Hot plasma: + T / 6 MeV.
        let hot = mix(&[(GasId::PLASMA, 10.0)], 600.0);
        sm.crystal.calculate_gases(&hot);
        sm.crystal.internal_energy = 0.0;
        sm.crystal.calculate_internal_energy(&hot, 13, 6.5);
        assert!(close(
            sm.crystal.internal_energy,
            100.0 - (100.0f64 / 500.0).powi(3)
        ));
    }

    #[test]
    fn damage_and_healing() {
        let mut c = Crystal::default();
        // 50 moles: low-mole bonus clamp(2 - 0.5, 0, 1) = 1 -> limit 2 * 313.15 = 626.3 K.
        c.calculate_temp_limit(50.0);
        assert!(close(c.temp_limit, 626.3));
        // 300 moles: no bonus, 313.15 K; gas at 1000 K: heat damage (1000 - 313.15) / 24000.
        c.calculate_temp_limit(300.0);
        let hot = mix(&[(GasId::N2, 300.0)], 1000.0);
        c.calculate_damage(&hot, false);
        assert!(close(c.damage, (1000.0 - 313.15) / 24000.0));
        // Cold gas heals up to 0.1 per breath, never below zero.
        let cold = mix(&[(GasId::N2, 300.0)], 100.0);
        c.calculate_damage(&cold, false);
        assert_eq!(c.damage, 0.0);
        // Over 5000 MeV: (E - 5000) / 40000 power damage, capped at 0.1; spaced: E * 1.25e-4.
        c.internal_energy = 6000.0;
        c.calculate_damage(&mix(&[(GasId::N2, 300.0)], 313.15), false);
        assert!(close(c.damage, 1000.0 / 40000.0));
        c.damage = 0.0;
        c.internal_energy = 1000.0;
        c.calculate_damage(&mix(&[], 2.7), true);
        assert!(close(c.damage, 0.125));
        // 2000 moles: (2000 - 1800) / 3200 = 0.0625 mole damage.
        c.damage = 0.0;
        c.internal_energy = 0.0;
        c.calculate_temp_limit(2000.0);
        c.calculate_damage(&mix(&[(GasId::N2, 2000.0)], 313.15), false);
        assert!(close(c.damage, 0.0625));
        assert_eq!(c.select_delam(2000.0), Delam::Singularity);
    }

    #[test]
    fn breath_zaps_and_waste() {
        let mut rng = CombatRng(1);
        let mut c = Crystal {
            internal_energy: 1000.0,
            last_power_zap: 0.0,
            last_energy_accumulation: 100,
            ..default()
        };
        // Room air at 15% absorption: 0.15 * 103.98 mol.
        let mut absorbed = mix(
            &[(GasId::O2, 0.15 * 21.8), (GasId::N2, 0.15 * 82.0)],
            293.15,
        );
        let out = c.process_atmos(&mut absorbed, 0.15, false, 101, 10.0, &mut rng);
        // Decay to 992 MeV; one machine tick (2 s) of accumulation at the gas-boosted rate,
        // 10% discharged.
        let o2 = 21.8 / (21.8 + 82.0);
        let rate = 1040.0 * (1.0 + 0.15 * o2);
        let e = 1000.0 - 8.0;
        assert!(close(c.internal_energy, e));
        let accumulated = e * rate * 2.0;
        assert!(close(out.zap.unwrap(), 0.1 * accumulated));
        assert!(close(c.zap_accumulation[0], 0.9 * accumulated));
        // Waste: multiplier 0.5 (nitrogen-heavy): +E * 0.65 * 0.5 / 4 K, plasma E * 0.65 * 0.5 /
        // 650, oxygen ((E * 0.65 + T * 0.5) - 273.15) / 340.
        let de = e * 0.65;
        let t = 293.15 + de * 0.5 / 4.0;
        assert!(close(out.merged.temperature, t));
        assert!(close(out.merged.get(GasId::PLASMA), de * 0.5 / 650.0));
        let o2_out = 0.15 * 21.8 + (de + t * 0.5 - 273.15) / 340.0;
        assert!(close(out.merged.get(GasId::O2), o2_out));
        assert!(out.high_zaps.is_empty());
        // The next breath within 4 - E/1000 = 3 s does not zap again.
        let out = c.process_atmos(&mut absorbed.clone(), 0.15, false, 101, 11.0, &mut rng);
        assert!(out.zap.is_none());
    }

    #[test]
    fn high_power_arcs() {
        let mut rng = CombatRng(3);
        let mut c = Crystal {
            internal_energy: 6000.0,
            last_power_zap: 0.0,
            last_energy_accumulation: 10,
            last_high_energy_accumulation: 10,
            ..default()
        };
        let absorbed = mix(&[(GasId::N2, 15.0)], 293.15);
        let mut out = Breath::default();
        c.handle_high_power(&absorbed, 0.15, 11, &mut rng, &mut out);
        // 5000..7000 MeV: two arcs sharing 1 - 0.9^(1/2) each of clamp(E * 3200) * 2 s.
        assert_eq!(out.high_zaps.len(), 2);
        let acc = (6000.0f64 * 3200.0).clamp(6.4e6, 3.2e7) * 2.0;
        let p = 1.0 - 0.9f64.sqrt();
        assert!(close(out.high_zaps[0], acc * p));
        assert!(close(out.high_zaps[1], acc * (1.0 - p) * p));
        // Pressure 15 * 8.31 * 293.15 / (2500 * 0.15) = 97.4 kPa: range clamp(6000 / 97.4 * 10)
        // = 7; cutoff 1.2e6 - 6000 * 15 * 40 / 293.15.
        assert_eq!(out.high_range, 7.0);
        assert!(close(c.zap_cutoff, 1.2e6 - 6000.0 * 15.0 * 40.0 / 293.15));
        assert_eq!(out.high_flags, ZAP_SUPERMATTER_FLAGS);
        // The linear decay above 5880 MeV: 6000 drops to 0.17 * 6000 + 3254 = 4274 MeV.
        let mut d = c.clone();
        d.calculate_gases(&absorbed);
        d.calculate_internal_energy(&absorbed, 12, 60.0);
        assert!((d.internal_energy - (6000.0 - (6000.0 * 0.83 - 3254.0203))).abs() < 0.01);
        assert_eq!(c.select_delam(15.0), Delam::Tesla);
    }

    #[test]
    fn pluoxium_from_co2_and_o2() {
        let mut c = Crystal::default();
        // 50/50 CO2 and O2 at 1 atm in the tile; 15% absorbed.
        let total = ONE_ATMOSPHERE * CELL_VOLUME / (R_IDEAL_GAS_EQUATION * 293.15);
        let mut absorbed = mix(
            &[(GasId::CO2, 0.075 * total), (GasId::O2, 0.075 * total)],
            293.15,
        );
        c.calculate_gases(&absorbed);
        c.extra_effects(&mut absorbed, 0.15);
        let co2_pp = ONE_ATMOSPHERE * 0.5;
        let r = 0.5 * (co2_pp - CO2_CONSUMPTION_PP) / (co2_pp + CO2_PRESSURE_SCALING);
        let consumed = 0.075 * total * r;
        assert!(close(absorbed.get(GasId::PLUOXIUM), consumed));
        assert!(close(absorbed.get(GasId::CO2), 0.075 * total - consumed));
    }

    #[test]
    fn integrity_text() {
        assert_eq!(dm_number(100.0), "100");
        assert_eq!(dm_number(99.5), "99.5");
        assert_eq!(dm_number(87.456), "87.46");
    }
}
