//! The Hypertorus Fusion Reactor's reaction (tg `code/modules/atmospherics/machinery/
//! components/fusion/`: `hfr_main_processes.dm`, `hfr_fuel_datums.dm`, `_hfr_defines.dm`, the
//! core's vars in `hfr_core.dm`).
//!
//! [`Reactor`] is the core's state with its two internal mixtures (fusion 5000 L, moderator
//! 10000 L); [`Reactor::process`] is one `process_atmos` of a linked, running core on its four
//! port mixtures and returns what the world must do ([`Effects`]): the parts, the machines and
//! the alarms live in the game (`ss13_game::atmos_machinery::hfr`).
//!
//! Randomness (`prob`) comes from the caller (`prob(percent) -> bool`).

use std::f64::consts::PI;

use crate::gas::{GAS_COUNT, GasId, GasMixture, MOLAR_ACCURACY, TCMB, quantize};
use crate::pipenet::air::PipeAir;

/// `LIGHT_SPEED`, `PLANCK_LIGHT_CONSTANT`, `CALCULATED_H2RADIUS`, `CALCULATED_TRITRADIUS`.
const LIGHT_SPEED: f64 = 299_792_458.0;
const PLANCK_LIGHT_CONSTANT: f64 = 2e-16;
const CALCULATED_H2RADIUS: f64 = 120e-4;
const CALCULATED_TRITRADIUS: f64 = 230e-3;
/// `VOID_CONDUCTION`, `FUSION_MOLE_THRESHOLD`, `INSTABILITY_GAS_POWER_FACTOR`,
/// `TOROID_VOLUME_BREAKEVEN`.
const VOID_CONDUCTION: f64 = 1e-2;
pub const FUSION_MOLE_THRESHOLD: f64 = 25.0;
const INSTABILITY_GAS_POWER_FACTOR: f64 = 0.003;
const TOROID_VOLUME_BREAKEVEN: f64 = 1000.0;
/// `METALLIC_VOID_CONDUCTIVITY`, `HIGH_EFFICIENCY_CONDUCTIVITY`.
const METALLIC_VOID_CONDUCTIVITY: f64 = 0.38;
const HIGH_EFFICIENCY_CONDUCTIVITY: f64 = 0.975;
/// `MIN_POWER_USAGE`, watts per power level (plus one).
pub const MIN_POWER_USAGE: f64 = 50_000.0;
/// `DAMAGE_CAP_MULTIPLIER`.
const DAMAGE_CAP_MULTIPLIER: f64 = 0.005;
/// `IRON_CHANCE_PER_FUSION_LEVEL`, `IRON_ACCUMULATED_PER_SECOND`.
const IRON_CHANCE_PER_FUSION_LEVEL: f64 = 17.0;
const IRON_ACCUMULATED_PER_SECOND: f64 = 0.005;
const IRON_OXYGEN_HEAL_PER_SECOND: f64 =
    IRON_ACCUMULATED_PER_SECOND * (100.0 - IRON_CHANCE_PER_FUSION_LEVEL) / 100.0;
const OXYGEN_MOLES_CONSUMED_PER_IRON_HEAL: f64 = 10.0 / IRON_OXYGEN_HEAL_PER_SECOND;
/// `FUSION_MAXIMUM_TEMPERATURE`, `FUSION_INSTABILITY_ENDOTHERMALITY`.
pub const FUSION_MAXIMUM_TEMPERATURE: f64 = 1e8;
const FUSION_INSTABILITY_ENDOTHERMALITY: f64 = 4.0;
/// `MINIMUM_MOLE_COUNT`.
const MINIMUM_MOLE_COUNT: f64 = 0.01;

/// The internal mixtures' volumes (`internal_fusion.volume`, `moderator_internal.volume`).
pub const FUSION_VOLUME: f64 = 5000.0;
pub const MODERATOR_VOLUME: f64 = 10_000.0;

/// `melting_point`, `warning_point`, `emergency_point` of `critical_threshold_proximity`.
pub const MELTING_POINT: f64 = 900.0;
pub const WARNING_POINT: f64 = 50.0;
pub const EMERGENCY_POINT: f64 = 700.0;

// Overfull damage: 2700 moles safe at 1 K down to 1800 at the maximum temperature, then a point
// per 200 moles per second.
const OVERFULL_MIN_POWER_LEVEL: u8 = 6;
const OVERFULL_MOLAR_SLOPE: f64 = 1.0 / 200.0;
const OVERFULL_MAX_SAFE_COLD: f64 = 2700.0;
const OVERFULL_MAX_SAFE_HOT: f64 = 1800.0;
const OVERFULL_TEMPERATURE_SLOPE: f64 = OVERFULL_MOLAR_SLOPE
    * (OVERFULL_MAX_SAFE_COLD - OVERFULL_MAX_SAFE_HOT)
    / (FUSION_MAXIMUM_TEMPERATURE - 1.0);
const OVERFULL_CONSTANT: f64 = -(OVERFULL_MOLAR_SLOPE * OVERFULL_MAX_SAFE_HOT
    + OVERFULL_TEMPERATURE_SLOPE * FUSION_MAXIMUM_TEMPERATURE);
const SUBCRITICAL_MOLES: f64 = 1200.0;
const SUBCRITICAL_SCALE: f64 = 400.0;
const COLD_COOLANT_MAX_RESTORE: f64 = 2.5;
const COLD_COOLANT_THRESHOLD: f64 = 1e5;
/// `HYPERTORUS_COLD_COOLANT_SCALE`: `2.5 / log10(1e5)`.
const COLD_COOLANT_SCALE: f64 = COLD_COOLANT_MAX_RESTORE / 5.0;
const MAX_SAFE_IRON: f64 = 0.35;
const HYPERCRITICAL_MOLES: f64 = 10_000.0;
const HYPERCRITICAL_SCALE: f64 = 0.002;
const HYPERCRITICAL_MAX_DAMAGE: f64 = 20.0;
const WEAK_SPILL_RATE: f64 = 0.0005;
const WEAK_SPILL_CHANCE: f64 = 1.0;
const MEDIUM_SPILL_PRESSURE: f64 = 10_000.0;
pub const MEDIUM_SPILL_INITIAL: f64 = 0.25;
const MEDIUM_SPILL_RATE: f64 = 0.01;
const STRONG_SPILL_PRESSURE: f64 = 12_000.0;
pub const STRONG_SPILL_INITIAL: f64 = 0.75;
const STRONG_SPILL_RATE: f64 = 0.05;

/// Meltdown flags of a fuel (`HYPERTORUS_FLAG_*`).
pub mod meltdown {
    pub const BASE_EXPLOSION: u16 = 1 << 0;
    pub const MEDIUM_EXPLOSION: u16 = 1 << 1;
    pub const DEVASTATING_EXPLOSION: u16 = 1 << 2;
    pub const RADIATION_PULSE: u16 = 1 << 3;
    pub const EMP: u16 = 1 << 4;
    pub const MINIMUM_SPREAD: u16 = 1 << 5;
    pub const MEDIUM_SPREAD: u16 = 1 << 6;
    pub const BIG_SPREAD: u16 = 1 << 7;
    pub const MASSIVE_SPREAD: u16 = 1 << 8;
    pub const CRITICAL_MELTDOWN: u16 = 1 << 9;
}

/// Why the core is taking damage (`warning_damage_flags`).
pub mod warning {
    pub const HIGH_POWER_DAMAGE: u8 = 1 << 0;
    pub const HIGH_FUEL_MIX_MOLE: u8 = 1 << 1;
    pub const IRON_CONTENT_DAMAGE: u8 = 1 << 2;
    pub const IRON_CONTENT_INCREASE: u8 = 1 << 3;
    pub const EMPED: u8 = 1 << 4;
}

/// `/datum/gas/*` `fusion_power`, by [`GasId`].
pub const FUSION_POWER: [f64; GAS_COUNT] = [
    0.0, 0.0, 0.0, 0.0, 8.0, 10.0, 10.0, 7.0, 5.0, 8.0, -10.0, 0.0, -5.0, 2.0, 0.0, 0.0, 0.0, 0.0,
    7.0, 20.0,
];

/// `/datum/hfr_fuel`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fuel {
    pub id: &'static str,
    pub name: &'static str,
    pub negative_temperature_multiplier: f64,
    pub positive_temperature_multiplier: f64,
    pub energy_concentration_multiplier: f64,
    pub fuel_consumption_multiplier: f64,
    pub gas_production_multiplier: f64,
    /// At most 1 (`New`).
    pub temperature_change_multiplier: f64,
    pub requirements: [GasId; 2],
    pub primary_products: &'static [GasId],
    pub secondary_products: [GasId; 6],
    pub meltdown_flags: u16,
}

use meltdown as m;

/// `GLOB.hfr_fuels_list`, in `subtypesof` order.
pub const FUELS: [Fuel; 7] = [
    Fuel {
        id: "plasma_o2_fuel",
        name: "Plasma + Oxygen fuel",
        negative_temperature_multiplier: 2.5,
        positive_temperature_multiplier: 0.1,
        energy_concentration_multiplier: 10.0,
        fuel_consumption_multiplier: 3.3,
        gas_production_multiplier: 1.4,
        temperature_change_multiplier: 0.6,
        requirements: [GasId::PLASMA, GasId::O2],
        primary_products: &[GasId::CO2, GasId::WATER_VAPOR],
        secondary_products: [
            GasId::CO2,
            GasId::WATER_VAPOR,
            GasId::FREON,
            GasId::N2O,
            GasId::PLUOXIUM,
            GasId::HALON,
        ],
        meltdown_flags: m::BASE_EXPLOSION | m::MINIMUM_SPREAD,
    },
    Fuel {
        id: "h2_o2_fuel",
        name: "Hydrogen + Oxygen fuel",
        negative_temperature_multiplier: 2.0,
        positive_temperature_multiplier: 0.6,
        energy_concentration_multiplier: 3.0,
        fuel_consumption_multiplier: 1.1,
        gas_production_multiplier: 0.9,
        temperature_change_multiplier: 0.75,
        requirements: [GasId::HYDROGEN, GasId::O2],
        primary_products: &[GasId::HELIUM, GasId::N2],
        secondary_products: [
            GasId::HELIUM,
            GasId::PLASMA,
            GasId::O2,
            GasId::N2,
            GasId::BZ,
            GasId::HYPERNOBLIUM,
        ],
        meltdown_flags: m::BASE_EXPLOSION | m::EMP | m::MEDIUM_SPREAD,
    },
    Fuel {
        id: "t2_o2_fuel",
        name: "Tritium + Oxygen fuel",
        negative_temperature_multiplier: 2.1,
        positive_temperature_multiplier: 0.5,
        energy_concentration_multiplier: 2.0,
        fuel_consumption_multiplier: 1.2,
        gas_production_multiplier: 0.8,
        temperature_change_multiplier: 0.8,
        requirements: [GasId::TRITIUM, GasId::O2],
        primary_products: &[GasId::HELIUM, GasId::PLUOXIUM],
        secondary_products: [
            GasId::HELIUM,
            GasId::PLASMA,
            GasId::O2,
            GasId::N2,
            GasId::BZ,
            GasId::HYPERNOBLIUM,
        ],
        meltdown_flags: m::BASE_EXPLOSION | m::RADIATION_PULSE | m::MEDIUM_SPREAD,
    },
    Fuel {
        id: "h2_t2_fuel",
        name: "Hydrogen + Tritium fuel",
        negative_temperature_multiplier: 1.0,
        positive_temperature_multiplier: 1.0,
        energy_concentration_multiplier: 1.0,
        fuel_consumption_multiplier: 1.0,
        gas_production_multiplier: 1.0,
        temperature_change_multiplier: 0.85,
        requirements: [GasId::HYDROGEN, GasId::TRITIUM],
        primary_products: &[GasId::HELIUM],
        secondary_products: [
            GasId::HELIUM,
            GasId::PLASMA,
            GasId::O2,
            GasId::N2,
            GasId::BZ,
            GasId::HYPERNOBLIUM,
        ],
        meltdown_flags: m::MEDIUM_EXPLOSION | m::RADIATION_PULSE | m::EMP | m::MEDIUM_SPREAD,
    },
    Fuel {
        id: "hypernob_hydrogen_fuel",
        name: "Hyper-Noblium + Hydrogen fuel",
        negative_temperature_multiplier: 0.2,
        positive_temperature_multiplier: 2.2,
        energy_concentration_multiplier: 0.2,
        fuel_consumption_multiplier: 0.55,
        gas_production_multiplier: 1.4,
        temperature_change_multiplier: 0.9,
        requirements: [GasId::HYPERNOBLIUM, GasId::HYDROGEN],
        primary_products: &[GasId::ANTINOBLIUM],
        secondary_products: [
            GasId::ANTINOBLIUM,
            GasId::HELIUM,
            GasId::PROTO_NITRATE,
            GasId::ZAUKER,
            GasId::HEALIUM,
            GasId::MIASMA,
        ],
        meltdown_flags: m::DEVASTATING_EXPLOSION | m::RADIATION_PULSE | m::EMP | m::BIG_SPREAD,
    },
    Fuel {
        id: "hypernob_trit_fuel",
        name: "Hyper-Noblium + Tritium fuel",
        negative_temperature_multiplier: 0.1,
        positive_temperature_multiplier: 2.5,
        energy_concentration_multiplier: 0.1,
        fuel_consumption_multiplier: 0.45,
        gas_production_multiplier: 1.7,
        temperature_change_multiplier: 0.95,
        requirements: [GasId::HYPERNOBLIUM, GasId::TRITIUM],
        primary_products: &[GasId::ANTINOBLIUM],
        secondary_products: [
            GasId::ANTINOBLIUM,
            GasId::HELIUM,
            GasId::PROTO_NITRATE,
            GasId::ZAUKER,
            GasId::HEALIUM,
            GasId::MIASMA,
        ],
        meltdown_flags: m::DEVASTATING_EXPLOSION | m::RADIATION_PULSE | m::EMP | m::BIG_SPREAD,
    },
    Fuel {
        id: "hypernob_antinob_fuel",
        name: "Hyper-Noblium + Anti-Noblium fuel",
        negative_temperature_multiplier: 0.01,
        positive_temperature_multiplier: 3.5,
        energy_concentration_multiplier: 2.0,
        fuel_consumption_multiplier: 0.01,
        gas_production_multiplier: 3.0,
        temperature_change_multiplier: 1.0,
        requirements: [GasId::HYPERNOBLIUM, GasId::ANTINOBLIUM],
        primary_products: &[GasId::HELIUM],
        secondary_products: [
            GasId::PLASMA,
            GasId::O2,
            GasId::N2,
            GasId::PROTO_NITRATE,
            GasId::NITRIUM,
            GasId::MIASMA,
        ],
        meltdown_flags: m::DEVASTATING_EXPLOSION
            | m::RADIATION_PULSE
            | m::EMP
            | m::MASSIVE_SPREAD
            | m::CRITICAL_MELTDOWN,
    },
];

/// The fuel of `id` (`GLOB.hfr_fuels_list[id]`).
pub fn fuel(id: &str) -> Option<usize> {
    FUELS.iter().position(|f| f.id == id)
}

/// `remove_specific(gas, amount)`: up to `amount` of one gas, at the mixture's temperature.
pub fn remove_specific(mix: &mut GasMixture, gas: GasId, amount: f64) -> Option<GasMixture> {
    let amount = amount.min(mix.get(gas));
    if amount <= 0.0 {
        return None;
    }
    let mut removed = GasMixture::vacuum();
    removed.temperature = mix.temperature;
    let moved = quantize(amount);
    removed.set(gas, moved);
    let left = mix.get(gas) - moved;
    mix.set(gas, if left <= MOLAR_ACCURACY { 0.0 } else { left });
    Some(removed)
}

/// The machine's port mixtures for one process: the core's coolant port (`airs[1]`), the fuel
/// and moderator inputs and the waste output.
pub struct Ports<'a> {
    pub cooling: &'a mut PipeAir,
    pub fuel: &'a mut PipeAir,
    pub moderator: &'a mut PipeAir,
    pub output: &'a mut PipeAir,
}

/// What a process asks the world to do.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Effects {
    /// `visible_hallucination_pulse(src, range, seconds)`.
    pub hallucination: Option<(u32, f64)>,
    /// `check_lightning_arcs`: arcs, their energy (J), cutoff and the power level.
    pub zaps: Option<Zaps>,
    /// `check_gravity_pulse`: living mobs within this range step toward the core.
    pub gravity_range: Option<u32>,
    /// `radiation_pulse(src, max_range = 6, threshold = 0.3)`.
    pub radiation: bool,
    /// `check_nuclear_particles` would fire one from a random corner.
    pub nuclear_particle: bool,
    /// The moderator overflowed ([`Reactor::moderator_overflow`]).
    pub overflow: Option<Overflow>,
}

/// `supermatter_zap(src, 5, power_level * 2.4e5, flags, cutoff, power_level * 1000)` × count.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Zaps {
    pub count: u32,
    pub energy: f64,
    pub cutoff: f64,
    pub power_level: u8,
}

/// `process_moderator_overflow`'s outcome.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Overflow {
    /// A cracked part leaks this fraction of the moderator.
    Leak(f64),
    /// A random part cracks; over 10 MPa it also explodes (`strong` over 12 MPa) and spills
    /// [`MEDIUM_SPILL_INITIAL`] or [`STRONG_SPILL_INITIAL`].
    Crack { explosion: Option<bool> },
}

/// The core's state (`/obj/machinery/atmospherics/components/unary/hypertorus/core`).
#[derive(Clone, Debug, PartialEq)]
pub struct Reactor {
    pub fusion: PipeAir,
    pub moderator: PipeAir,
    pub selected_fuel: Option<usize>,
    pub start_power: bool,
    pub start_cooling: bool,
    pub start_fuel: bool,
    pub start_moderator: bool,
    pub energy: f64,
    pub core_temperature: f64,
    pub internal_power: f64,
    pub power_output: f64,
    pub instability: f64,
    pub delta_temperature: f64,
    pub conduction: f64,
    pub radiation: f64,
    pub efficiency: f64,
    pub heat_limiter_modifier: f64,
    pub heat_output_max: f64,
    pub heat_output_min: f64,
    pub heat_output: f64,
    pub waste_remove: bool,
    pub heating_conductor: f64,
    pub magnetic_constrictor: f64,
    pub current_damper: f64,
    pub power_level: u8,
    pub iron_content: f64,
    pub fuel_injection_rate: f64,
    pub moderator_injection_rate: f64,
    /// Gas bits filtered out of the moderator (`moderator_scrubbing`).
    pub moderator_scrubbing: u32,
    pub moderator_filtering_rate: f64,
    pub critical_threshold_proximity: f64,
    pub critical_threshold_proximity_archived: f64,
    pub warning_damage_flags: u8,
    pub fusion_temperature: f64,
    pub fusion_temperature_archived: f64,
    pub moderator_temperature: f64,
    pub moderator_temperature_archived: f64,
    pub coolant_temperature: f64,
    pub coolant_temperature_archived: f64,
    pub output_temperature: f64,
    pub output_temperature_archived: f64,
    pub temperature_period: f64,
}

impl Default for Reactor {
    fn default() -> Self {
        Self {
            fusion: PipeAir::new(FUSION_VOLUME),
            moderator: PipeAir::new(MODERATOR_VOLUME),
            selected_fuel: None,
            start_power: false,
            start_cooling: false,
            start_fuel: false,
            start_moderator: false,
            energy: 0.0,
            core_temperature: crate::gas::T20C,
            internal_power: 0.0,
            power_output: 0.0,
            instability: 0.0,
            delta_temperature: 0.0,
            conduction: 0.0,
            radiation: 0.0,
            efficiency: 0.0,
            heat_limiter_modifier: 0.0,
            heat_output_max: 0.0,
            heat_output_min: 0.0,
            heat_output: 0.0,
            waste_remove: false,
            heating_conductor: 100.0,
            magnetic_constrictor: 100.0,
            current_damper: 0.0,
            power_level: 0,
            iron_content: 0.0,
            fuel_injection_rate: 25.0,
            moderator_injection_rate: 25.0,
            moderator_scrubbing: 1 << GasId::HELIUM.0,
            moderator_filtering_rate: 100.0,
            critical_threshold_proximity: 0.0,
            critical_threshold_proximity_archived: 0.0,
            warning_damage_flags: 0,
            fusion_temperature: 0.0,
            fusion_temperature_archived: 0.0,
            moderator_temperature: 0.0,
            moderator_temperature_archived: 0.0,
            coolant_temperature: 0.0,
            coolant_temperature_archived: 0.0,
            output_temperature: 0.0,
            output_temperature_archived: 0.0,
            temperature_period: 1.0,
        }
    }
}

/// The power level of a fusion temperature (`update_temperature_status`).
pub fn power_level_of(temperature: f64) -> u8 {
    match temperature {
        t if t <= 500.0 => 0,
        t if t <= 1e3 => 1,
        t if t <= 1e4 => 2,
        t if t <= 1e5 => 3,
        t if t <= 1e6 => 4,
        t if t <= 1e7 => 5,
        _ => 6,
    }
}

/// `1 - (1 - rate) ** seconds`.
fn per_tick(rate: f64, seconds: f64) -> f64 {
    1.0 - (1.0 - rate).powf(seconds)
}

impl Reactor {
    pub fn fuel(&self) -> Option<&'static Fuel> {
        self.selected_fuel.and_then(|i| FUELS.get(i))
    }

    /// `get_integrity_percent`.
    pub fn integrity_percent(&self) -> f64 {
        let integrity = self.critical_threshold_proximity / MELTING_POINT;
        crate::gas::byond_round(100.0 - integrity * 100.0, 0.01).max(0.0)
    }

    /// `check_fuel`: a fuel is selected and each requirement is over the threshold.
    pub fn check_fuel(&self) -> bool {
        let Some(fuel) = self.fuel() else {
            return false;
        };
        self.fusion.total_moles() > 0.0
            && fuel
                .requirements
                .iter()
                .all(|&g| self.fusion.gas.get(g) >= FUSION_MOLE_THRESHOLD)
    }

    /// The selected recipe changed (`ui_act("fuel")`): the fusion mix goes to the waste port.
    pub fn select_fuel(&mut self, fuel: Option<usize>, output: &mut PipeAir) {
        self.selected_fuel = fuel.filter(|&i| i < FUELS.len());
        if self.fusion.total_moles() > 0.0 {
            let all = self.fusion.total_moles();
            if let Some(removed) = self.fusion.remove(all) {
                output.merge(&removed);
            }
        }
    }

    /// One `process_atmos` of a linked core: the reaction (when started or still burning),
    /// the moderator's overflow, damage and healing, waste removal. `powered` is
    /// `check_power_use()` (no `NOPOWER|BROKEN`).
    pub fn process(
        &mut self,
        seconds: f64,
        powered: bool,
        cracked: bool,
        ports: &mut Ports<'_>,
        prob: &mut dyn FnMut(f64) -> bool,
    ) -> Effects {
        let mut effects = Effects::default();
        if self.start_power || self.power_level > 0 {
            self.fusion_process(seconds, powered, ports, prob, &mut effects);
            // Damage and healing run even when the reaction stopped: running out of fuel does
            // not save a core whose moderator and coolant are exploding on their own.
            effects.overflow = self.moderator_overflow(seconds, cracked, prob);
            self.process_damageheal(seconds, &*ports.cooling, prob);
        }
        if self.start_power {
            self.remove_waste(seconds, &mut *ports.output);
        }
        effects
    }

    /// `fusion_process`.
    fn fusion_process(
        &mut self,
        seconds: f64,
        powered: bool,
        ports: &mut Ports<'_>,
        prob: &mut dyn FnMut(f64) -> bool,
        effects: &mut Effects,
    ) {
        if powered {
            if self.start_cooling {
                self.inject_from_side_components(seconds, ports);
                self.process_internal_cooling(seconds, &mut *ports.cooling);
            }
        } else {
            // No power forces bad settings.
            self.magnetic_constrictor = 100.0;
            self.heating_conductor = 500.0;
            self.current_damper = 0.0;
            self.fuel_injection_rate = 20.0;
            self.moderator_injection_rate = 50.0;
            self.waste_remove = false;
            self.iron_content += 0.02 * f64::from(self.power_level) * seconds;
        }
        self.update_temperature_status(seconds, ports);

        let archived_heat = self.fusion.temperature();
        let volume = self.fusion.volume * (self.magnetic_constrictor * 0.01);
        let fuel = self.fuel();
        let (energy_concentration, positive_multiplier, negative_multiplier) =
            fuel.map_or((1.0, 1.0, 1.0), |f| {
                (
                    f.energy_concentration_multiplier,
                    f.positive_temperature_multiplier,
                    f.negative_temperature_multiplier,
                )
            });
        // Scaled by volume / 2: in fusion conditions moles are about twice the volume.
        let scale_factor = volume * 0.5;
        let scaled = |moles: f64| ((moles - FUSION_MOLE_THRESHOLD) / scale_factor).max(0.0);
        let fuel_list = self.fusion.gas.moles;
        let moderator_list = self.moderator.gas.moles;
        let sf = |g: GasId| scaled(fuel_list[usize::from(g.0)]);
        let sm = |g: GasId| scaled(moderator_list[usize::from(g.0)]);
        let ml = |g: GasId| moderator_list[usize::from(g.0)];

        // The instability of the reaction in the phase space hypertorus.
        let toroidal_size =
            2.0 * PI + ((volume - TOROID_VOLUME_BREAKEVEN) / TOROID_VOLUME_BREAKEVEN).atan();
        let dot = |moles: &[f64; GAS_COUNT]| -> f64 {
            moles.iter().zip(FUSION_POWER).map(|(m, p)| m * p).sum()
        };
        let gas_power = dot(&fuel_list) + 0.75 * dot(&moderator_list);
        self.instability = ((gas_power * INSTABILITY_GAS_POWER_FACTOR).powi(2))
            .rem_euclid(toroidal_size)
            + self.current_damper * 0.01
            - self.iron_content * 0.05;
        let internal_instability = if self.instability * 0.5 < FUSION_INSTABILITY_ENDOTHERMALITY {
            1.0
        } else {
            -1.0
        };

        use GasId as G;
        let mut energy_modifiers = sm(G::N2) * 0.35
            + sm(G::CO2) * 0.55
            + sm(G::N2O) * 0.95
            + sm(G::ZAUKER) * 1.55
            + sm(G::ANTINOBLIUM) * 20.0;
        energy_modifiers -= sm(G::HYPERNOBLIUM) * 10.0
            + sm(G::WATER_VAPOR) * 0.75
            + sm(G::NITRIUM) * 0.15
            + sm(G::HEALIUM) * 0.45
            + sm(G::FREON) * 1.15;
        let mut power_modifier = sm(G::O2) * 0.55
            + sm(G::CO2) * 0.95
            + sm(G::NITRIUM) * 1.45
            + sm(G::ZAUKER) * 5.55
            + sm(G::PLASMA) * 0.05
            - sm(G::N2O) * 0.05
            - sm(G::FREON) * 0.75;
        let mut heat_modifier =
            sm(G::PLASMA) * 1.25 - sm(G::N2) * 0.75 - sm(G::N2O) * 1.45 - sm(G::FREON) * 0.95;
        let mut radiation_modifier = sm(G::FREON) * 1.15 - sm(G::N2) * 0.45 - sm(G::PLASMA) * 0.95
            + sm(G::BZ) * 1.9
            + sm(G::PROTO_NITRATE) * 0.1
            + sm(G::ANTINOBLIUM) * 10.0;
        if let Some(f) = fuel {
            let [r1, r2] = f.requirements;
            let p1 = f.primary_products[0];
            energy_modifiers += sf(r1) + sf(r2);
            energy_modifiers -= sf(p1);
            power_modifier += sf(r2) * 1.05 - sf(p1) * 0.55;
            heat_modifier += sf(r1) * 1.15 + sf(p1) * 1.05;
            radiation_modifier += sf(p1);
        }
        let power_modifier = power_modifier.clamp(0.25, 100.0);
        let heat_modifier = heat_modifier.clamp(0.25, 100.0);
        let radiation_modifier = radiation_modifier.clamp(0.005, 1000.0);

        self.internal_power = 0.0;
        self.efficiency = VOID_CONDUCTION;
        if let Some(f) = fuel {
            let [r1, r2] = f.requirements;
            let (a, b) = (sf(r1), sf(r2));
            self.internal_power = (a * power_modifier / 100.0)
                * (b * power_modifier / 100.0)
                * (PI * (2.0 * (a * CALCULATED_H2RADIUS) * (b * CALCULATED_TRITRADIUS)).powi(2))
                * self.energy;
            self.efficiency = VOID_CONDUCTION * sf(f.primary_products[0]).clamp(1.0, 100.0);
        }
        // E=mc^2, changed for gameplay.
        self.energy = (energy_modifiers * LIGHT_SPEED.powi(2))
            * (self.fusion.temperature() * heat_modifier / 100.0).max(1.0);
        self.energy = (self.energy / energy_concentration).clamp(0.0, 1e35);
        self.core_temperature = (self.internal_power * power_modifier / 1000.0).max(TCMB);
        self.delta_temperature = archived_heat - self.core_temperature;
        self.conduction = -self.delta_temperature * (self.magnetic_constrictor * 0.001);
        self.radiation =
            (-(PLANCK_LIGHT_CONSTANT / 5e-18) * radiation_modifier * self.delta_temperature)
                .max(0.0);
        self.power_output =
            self.efficiency * (self.internal_power - self.conduction - self.radiation);
        self.heat_limiter_modifier =
            5.0 * 10_f64.powi(i32::from(self.power_level)) * (self.heating_conductor / 100.0);
        self.heat_output_min = -self.heat_limiter_modifier * 0.01 * negative_multiplier;
        self.heat_output_max = self.heat_limiter_modifier * positive_multiplier;
        self.heat_output = (internal_instability * self.power_output * heat_modifier / 200.0)
            .clamp(self.heat_output_min, self.heat_output_max);

        // The values above always update (the window shows them); the reaction needs fuel.
        let Some(f) = fuel.filter(|_| self.check_fuel()) else {
            return;
        };
        let level = self.power_level;
        let fuel_consumption_rate =
            (self.fuel_injection_rate * 0.01 * 5.0 * f64::from(level)).clamp(0.05, 30.0);
        let consumption_amount = fuel_consumption_rate * seconds;
        let production_amount = match level {
            3 | 4 => (self.heat_output / 1000.0).clamp(0.0, fuel_consumption_rate) * seconds,
            _ => {
                (self.heat_output * 2.0 / 10_f64.powi(i32::from(level) + 1))
                    .clamp(0.0, fuel_consumption_rate)
                    * seconds
            }
        };
        // Antinoblium comes from how stale the mix is (byproduct over fresh fuel rate).
        let dirty_production_rate = sf(f.primary_products[0]) / self.fuel_injection_rate;

        // `moderator_fuel_process`.
        let fuel_consumption = consumption_amount * 0.85 * f.fuel_consumption_multiplier;
        let scaled_production = production_amount * f.gas_production_multiplier;
        for &g in &f.requirements {
            self.fusion
                .gas
                .add(g, -fuel_list[usize::from(g.0)].min(fuel_consumption));
        }
        for &g in f.primary_products {
            self.fusion.gas.add(g, fuel_consumption * 0.5);
        }
        let tier = f.secondary_products;
        let modr = &mut self.moderator.gas;
        match level {
            1 => {
                modr.add(tier[0], scaled_production * 0.95);
                modr.add(tier[1], scaled_production * 0.75);
            }
            2 => {
                modr.add(tier[0], scaled_production * 1.65);
                modr.add(tier[1], scaled_production);
                if ml(G::PLASMA) > 50.0 {
                    modr.add(tier[2], scaled_production * 1.15);
                }
            }
            3 => {
                modr.add(tier[1], scaled_production * 0.5);
                modr.add(tier[2], scaled_production * 0.45);
            }
            4 => {
                modr.add(tier[2], scaled_production * 1.65);
                modr.add(tier[3], scaled_production * 1.25);
                if ml(G::PLASMA) > 50.0 {
                    modr.add(tier[4], scaled_production * 1.15);
                }
            }
            5 => {
                modr.add(tier[3], scaled_production * 0.65);
                modr.add(tier[4], scaled_production);
                modr.add(tier[5], scaled_production * 0.75);
            }
            6 => {
                modr.add(tier[4], scaled_production * 0.35);
                modr.add(tier[5], scaled_production);
            }
            _ => {}
        }

        // `moderator_common_process`: the gases every recipe makes, the modifiers of the
        // moderator. `heat_output` here is the proc's own copy; the window keeps the member.
        let p = production_amount * f.gas_production_multiplier;
        let mut heat_output = self.heat_output;
        let mut out = GasMixture::vacuum();
        let hallucinate = |heat: f64| {
            (
                (heat.abs().powf(0.25).floor() as u32).min(7),
                100.0 * f64::from(level) * seconds,
            )
        };
        // tg adds the consumed moderator gases at levels 1-5 (a missing minus: the `min` with
        // what is there caps a removal); they are consumed here as at level 6.
        let consume = |modr: &mut GasMixture, g: GasId, amount: f64| {
            let take = modr.get(g).min(amount);
            modr.add(g, -take);
        };
        let modr = &mut self.moderator.gas;
        match level {
            1 => {
                if ml(G::PLASMA) > 100.0 {
                    out.add(G::N2O, p * 0.5);
                    consume(modr, G::PLASMA, p * 0.85);
                }
                if ml(G::BZ) > 150.0 {
                    out.add(G::HALON, p * 0.55);
                    consume(modr, G::BZ, p * 0.95);
                }
            }
            2 => {
                if ml(G::PLASMA) > 50.0 {
                    out.add(G::BZ, p * 1.8);
                    consume(modr, G::PLASMA, p * 1.75);
                }
                if ml(G::PROTO_NITRATE) > 20.0 {
                    self.radiation *= 1.55;
                    heat_output *= 1.025;
                    out.add(G::NITRIUM, p * 1.05);
                    consume(modr, G::PROTO_NITRATE, p * 1.35);
                }
            }
            3 | 4 => {
                if ml(G::PLASMA) > 10.0 {
                    out.add(G::FREON, p * 0.15);
                    out.add(G::NITRIUM, p * 1.05);
                    consume(modr, G::PLASMA, p * 0.45);
                }
                if ml(G::FREON) > 50.0 {
                    heat_output *= 0.9;
                    self.radiation *= 0.8;
                }
                if ml(G::PROTO_NITRATE) > 15.0 {
                    out.add(G::NITRIUM, p * 1.25);
                    out.add(G::HALON, p * 1.15);
                    consume(modr, G::PROTO_NITRATE, p * 1.55);
                    self.radiation *= 1.95;
                    heat_output *= 1.25;
                }
                if ml(G::BZ) > 100.0 {
                    out.add(G::HEALIUM, p * 1.5);
                    out.add(G::PROTO_NITRATE, p * 1.5);
                    effects.hallucination = Some(hallucinate(heat_output));
                }
            }
            5 => {
                if ml(G::PLASMA) > 15.0 {
                    out.add(G::FREON, p * 0.25);
                    consume(modr, G::PLASMA, p * 1.45);
                }
                if ml(G::FREON) > 500.0 {
                    heat_output *= 0.5;
                    self.radiation *= 0.2;
                }
                if ml(G::PROTO_NITRATE) > 50.0 {
                    out.add(G::NITRIUM, p * 1.95);
                    out.add(G::PLUOXIUM, p);
                    consume(modr, G::PROTO_NITRATE, p * 1.35);
                    self.radiation *= 1.95;
                    heat_output *= 1.25;
                }
                if ml(G::BZ) > 100.0 {
                    out.add(G::HEALIUM, p);
                    out.add(G::FREON, p * 1.15);
                    effects.hallucination = Some(hallucinate(heat_output));
                }
                if ml(G::HEALIUM) > 100.0 && self.critical_threshold_proximity > 400.0 {
                    self.critical_threshold_proximity = (self.critical_threshold_proximity
                        - ml(G::HEALIUM) / 100.0 * seconds)
                        .max(0.0);
                    consume(modr, G::HEALIUM, p * 20.0);
                }
                if modr.temperature < 1e7 || (ml(G::PLASMA) > 100.0 && ml(G::BZ) > 50.0) {
                    out.add(
                        G::ANTINOBLIUM,
                        dirty_production_rate * 0.9 / 0.065 * seconds,
                    );
                }
            }
            6 => {
                if ml(G::PLASMA) > 30.0 {
                    out.add(G::BZ, p * 1.15);
                    consume(modr, G::PLASMA, p * 1.45);
                }
                if ml(G::PROTO_NITRATE) != 0.0 {
                    out.add(G::ZAUKER, p * 5.35);
                    out.add(G::NITRIUM, p * 2.15);
                    consume(modr, G::PROTO_NITRATE, p * 3.35);
                    self.radiation *= 2.0;
                    heat_output *= 2.25;
                }
                if ml(G::BZ) != 0.0 {
                    effects.hallucination = Some(hallucinate(heat_output));
                    out.add(
                        G::ANTINOBLIUM,
                        (dirty_production_rate / 0.045).clamp(0.0, 10.0) * seconds,
                    );
                }
                if ml(G::HEALIUM) > 100.0 && self.critical_threshold_proximity > 400.0 {
                    self.critical_threshold_proximity = (self.critical_threshold_proximity
                        - ml(G::HEALIUM) / 100.0 * seconds)
                        .max(0.0);
                    consume(modr, G::HEALIUM, p * 20.0);
                }
                self.fusion.gas.add(
                    G::ANTINOBLIUM,
                    dirty_production_rate * 0.01 / 0.095 * seconds,
                );
            }
            _ => {}
        }

        // The heat output changes the fusion mix's temperature, up to the recipe's maximum.
        let max_temperature = FUSION_MAXIMUM_TEMPERATURE * f.temperature_change_multiplier;
        let t = self.fusion.temperature();
        self.fusion.gas.temperature = if t <= max_temperature {
            (t + heat_output * seconds).clamp(TCMB, max_temperature)
        } else {
            t - self.heat_limiter_modifier * 0.01 * seconds
        };

        // The byproducts go hot into the waste port.
        if out.total_moles() > 0.0 {
            out.temperature = if self.moderator.total_moles() > 0.0 {
                self.moderator.temperature() * HIGH_EFFICIENCY_CONDUCTIVITY
            } else {
                self.fusion.temperature() * METALLIC_VOID_CONDUCTIVITY
            };
            ports.output.merge(&out);
        }

        // `evaporate_moderator`: everything in the moderator slowly burns away.
        if level > 0 && self.moderator.total_moles() > 0.0 {
            let amount =
                self.moderator.total_moles() * per_tick(0.0005 * f64::from(level), seconds);
            let _ = self.moderator.remove(amount);
        }
        // `check_nuclear_particles`.
        effects.nuclear_particle = level >= 4 && ml(G::BZ) >= 150.0 / f64::from(level);
        // `check_lightning_arcs`.
        if level >= 4 && (ml(G::ANTINOBLIUM) > 50.0 || self.critical_threshold_proximity > 500.0) {
            let mut count = u32::from(level) - 2;
            if self.critical_threshold_proximity > 650.0 && prob(20.0) {
                count += 1;
            }
            let cutoff = (2.4e6 - f64::from(level) * (self.fusion.total_moles() * 360.0))
                .clamp(3.6e5, 2.4e6);
            effects.zaps = Some(Zaps {
                count,
                energy: f64::from(level) * 2.4e5,
                cutoff,
                power_level: level,
            });
        }
        // Oxygen burns iron away.
        if ml(G::O2) > 150.0 && self.iron_content > 0.0 {
            let removed = (IRON_OXYGEN_HEAL_PER_SECOND * seconds).min(self.iron_content);
            self.iron_content -= removed;
            self.moderator
                .gas
                .add(G::O2, -removed * OXYGEN_MOLES_CONSUMED_PER_IRON_HEAL);
        }
        // `check_gravity_pulse`: `SPT_PROB(100 - proximity / 15)` skips it.
        let skip = 100.0
            * per_tick(
                (100.0 - self.critical_threshold_proximity / 15.0) / 100.0,
                seconds,
            );
        if !prob(skip) && self.critical_threshold_proximity > 0.0 {
            let range = (self.critical_threshold_proximity.ln() / 2.5_f64.ln()).floor();
            effects.gravity_range = Some(range.max(0.0) as u32);
        }
        effects.radiation = true;
    }

    /// `update_temperature_status`.
    fn update_temperature_status(&mut self, seconds: f64, ports: &Ports<'_>) {
        self.fusion_temperature_archived = self.fusion_temperature;
        self.fusion_temperature = self.fusion.temperature();
        self.moderator_temperature_archived = self.moderator_temperature;
        self.moderator_temperature = self.moderator.temperature();
        self.coolant_temperature_archived = self.coolant_temperature;
        self.coolant_temperature = ports.cooling.temperature();
        self.output_temperature_archived = self.output_temperature;
        self.output_temperature = ports.output.temperature();
        self.temperature_period = seconds;
        self.power_level = power_level_of(self.fusion_temperature);
    }

    /// `inject_from_side_components`.
    fn inject_from_side_components(&mut self, seconds: f64, ports: &mut Ports<'_>) {
        if self.start_moderator && ports.moderator.total_moles() > 0.0 {
            if let Some(gas) = ports
                .moderator
                .remove(self.moderator_injection_rate * seconds)
            {
                self.moderator.merge(&gas);
            }
        }
        let Some(fuel) = self.fuel() else { return };
        // `check_gas_requirements`: every fuel gas is in the port.
        if !self.start_fuel
            || fuel
                .requirements
                .iter()
                .any(|&g| ports.fuel.gas.get(g) == 0.0)
        {
            return;
        }
        let amount = self.fuel_injection_rate * seconds / fuel.requirements.len() as f64;
        for &g in &fuel.requirements {
            if let Some(gas) = remove_specific(&mut ports.fuel.gas, g, amount) {
                self.fusion.merge(&gas);
            }
        }
    }

    /// `process_internal_cooling`.
    fn process_internal_cooling(&mut self, seconds: f64, cooling: &mut PipeAir) {
        let exchange = |a: &mut PipeAir, b: &mut PipeAir, conductivity: f64| {
            let (ca, cb) = (a.heat_capacity(), b.heat_capacity());
            if ca <= 0.0 || cb <= 0.0 {
                return;
            }
            let delta = a.temperature() - b.temperature();
            let heat = per_tick(conductivity, seconds) * delta * (ca * cb / (ca + cb));
            a.gas.temperature = (a.temperature() - heat / ca).max(TCMB);
            b.gas.temperature = (b.temperature() + heat / cb).max(TCMB);
        };
        if self.moderator.total_moles() > 0.0 && self.fusion.total_moles() > 0.0 {
            exchange(
                &mut self.fusion,
                &mut self.moderator,
                METALLIC_VOID_CONDUCTIVITY,
            );
        }
        if cooling.total_moles() * 0.05 <= MINIMUM_MOLE_COUNT {
            return;
        }
        let Some(removed) = cooling.remove(0.05 * cooling.total_moles()) else {
            return;
        };
        let mut removed = PipeAir::with_gas(removed, cooling.volume);
        if self.moderator.total_moles() > 0.0 {
            exchange(
                &mut removed,
                &mut self.moderator,
                HIGH_EFFICIENCY_CONDUCTIVITY,
            );
        } else if self.fusion.total_moles() > 0.0 {
            exchange(&mut removed, &mut self.fusion, METALLIC_VOID_CONDUCTIVITY);
        }
        cooling.merge(&removed.gas);
    }

    /// `process_damageheal`.
    fn process_damageheal(
        &mut self,
        seconds: f64,
        cooling: &PipeAir,
        prob: &mut dyn FnMut(f64) -> bool,
    ) {
        self.critical_threshold_proximity_archived = self.critical_threshold_proximity;
        self.warning_damage_flags &= warning::EMPED;
        let level = self.power_level;
        let moles = self.fusion.total_moles();
        let mut ctp = self.critical_threshold_proximity;
        // Too much mass at the highest level.
        if level >= OVERFULL_MIN_POWER_LEVEL {
            let damage = OVERFULL_MOLAR_SLOPE * moles
                + OVERFULL_TEMPERATURE_SLOPE * self.coolant_temperature
                + OVERFULL_CONSTANT;
            ctp = (ctp + (damage * seconds).max(0.0)).max(0.0);
            self.warning_damage_flags |= warning::HIGH_POWER_DAMAGE;
        }
        // A thin fusion mix heals.
        if moles < SUBCRITICAL_MOLES && level <= 5 {
            let restore = (moles - SUBCRITICAL_MOLES) / SUBCRITICAL_SCALE;
            ctp = (ctp + (restore * seconds).min(0.0)).max(0.0);
        }
        // So does cold enough coolant.
        if moles > 0.0
            && cooling.total_moles() > 0.0
            && self.coolant_temperature < COLD_COOLANT_THRESHOLD
            && level <= 4
        {
            let restore = (self.coolant_temperature.max(1.0) * COLD_COOLANT_SCALE).log10()
                - COLD_COOLANT_MAX_RESTORE * 2.0;
            ctp = (ctp + (restore * seconds).min(0.0)).max(0.0);
        }
        ctp += (self.iron_content - MAX_SAFE_IRON).max(0.0) * seconds;
        if self.iron_content - MAX_SAFE_IRON > 0.0 {
            self.warning_damage_flags |= warning::IRON_CONTENT_DAMAGE;
        }
        // The damage cap.
        ctp = ctp.min(
            self.critical_threshold_proximity_archived
                + seconds * DAMAGE_CAP_MULTIPLIER * MELTING_POINT,
        );
        // A preposterous mass gets bad very fast, past the cap.
        if moles >= HYPERCRITICAL_MOLES {
            let over = moles - HYPERCRITICAL_MOLES;
            let damage = (over * HYPERCRITICAL_SCALE)
                .max(0.0)
                .min(HYPERCRITICAL_MAX_DAMAGE)
                * seconds;
            ctp = (ctp + damage).max(0.0);
            self.warning_damage_flags |= warning::HIGH_FUEL_MIX_MOLE;
        }
        self.critical_threshold_proximity = ctp;
        // High power fusion makes iron.
        if level > 4 && prob(IRON_CHANCE_PER_FUSION_LEVEL * f64::from(level)) {
            self.iron_content += IRON_ACCUMULATED_PER_SECOND * seconds;
            self.warning_damage_flags |= warning::IRON_CONTENT_INCREASE;
        }
        if self.iron_content > 0.0 && level <= 4 && prob(25.0 / (f64::from(level) + 1.0)) {
            self.iron_content = (self.iron_content - 0.01 * seconds).max(0.0);
        }
        self.iron_content = self.iron_content.clamp(0.0, 1.0);
    }

    /// `process_moderator_overflow`: a cracked part leaks; a hypercritical moderator cracks one.
    fn moderator_overflow(
        &self,
        seconds: f64,
        cracked: bool,
        prob: &mut dyn FnMut(f64) -> bool,
    ) -> Option<Overflow> {
        let pressure = self.moderator.pressure();
        if cracked {
            let rate = if pressure < MEDIUM_SPILL_PRESSURE {
                if !prob(WEAK_SPILL_CHANCE) {
                    return None;
                }
                WEAK_SPILL_RATE
            } else if pressure < STRONG_SPILL_PRESSURE {
                MEDIUM_SPILL_RATE
            } else {
                STRONG_SPILL_RATE
            };
            return Some(Overflow::Leak(per_tick(rate, seconds)));
        }
        if self.moderator.total_moles() < HYPERCRITICAL_MOLES {
            return None;
        }
        let explosion = if pressure < MEDIUM_SPILL_PRESSURE {
            None
        } else {
            Some(pressure >= STRONG_SPILL_PRESSURE)
        };
        Some(Overflow::Crack { explosion })
    }

    /// `remove_waste`: the selected moderator gases and the fusion byproducts to the waste port.
    fn remove_waste(&mut self, seconds: f64, output: &mut PipeAir) {
        if !self.waste_remove {
            return;
        }
        let filtering_amount = self.moderator_scrubbing.count_ones();
        if filtering_amount > 0 {
            let amount = self.moderator_filtering_rate / f64::from(filtering_amount) * seconds;
            for g in 0..GAS_COUNT as u8 {
                if self.moderator_scrubbing & (1 << g) == 0 {
                    continue;
                }
                if let Some(gas) = remove_specific(&mut self.moderator.gas, GasId(g), amount) {
                    output.merge(&gas);
                }
            }
        }
        if let Some(fuel) = self.fuel() {
            for &g in fuel.primary_products {
                let moles = self.fusion.gas.get(g);
                if moles > 0.0
                    && let Some(gas) =
                        remove_specific(&mut self.fusion.gas, g, moles * per_tick(0.25, seconds))
                {
                    output.merge(&gas);
                }
            }
        }
        self.fusion.gas.garbage_collect();
        self.moderator.gas.garbage_collect();
    }
}

/// `meltdown()`'s sizes from the fuel's flags and the power level.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Meltdown {
    pub devastation: i32,
    pub heavy: i32,
    pub light: i32,
    pub flash: i32,
    /// `empulse` heavy and light ranges.
    pub emp: Option<(i32, i32)>,
    pub radiation_range: Option<i32>,
    pub gas_spread: i32,
    pub gas_pockets: u32,
    pub critical: bool,
}

/// `meltdown()`: explosion, EMP and radiation sizes, gas spread.
pub fn meltdown(flags: u16, power_level: u8) -> Meltdown {
    let pl = i32::from(power_level);
    let mut out = Meltdown {
        critical: flags & m::CRITICAL_MELTDOWN != 0,
        ..Default::default()
    };
    let (emp, rad) = (flags & m::EMP != 0, flags & m::RADIATION_PULSE != 0);
    if flags & m::BASE_EXPLOSION != 0 {
        out.flash = pl * 3;
        out.light = pl * 2;
    }
    if flags & m::MEDIUM_EXPLOSION != 0 {
        out.flash = pl * 6;
        out.light = pl * 5;
        // `power_level * 0.5`, a fractional range.
        out.heavy = pl / 2;
    }
    if flags & m::DEVASTATING_EXPLOSION != 0 {
        out.flash = pl * 8;
        out.light = pl * 7;
        out.heavy = pl * 2;
        out.devastation = pl;
    }
    let spreads = [
        (m::MINIMUM_SPREAD, (3, 1), 2 * pl + 8, 5, 2),
        (m::MEDIUM_SPREAD, (5, 3), pl + 24, 7, 4),
        (m::BIG_SPREAD, (7, 5), pl + 34, 10, 6),
        (m::MASSIVE_SPREAD, (9, 7), pl + 44, 15, 8),
    ];
    let mut emp_size = (0, 0);
    for (flag, (light, heavy), rad_range, pockets, spread) in spreads {
        if flags & flag == 0 {
            continue;
        }
        emp_size = (pl * light, pl * heavy);
        out.radiation_range = rad.then_some(rad_range);
        out.gas_pockets = pockets;
        out.gas_spread = pl * spread;
    }
    if out.critical {
        out.devastation *= 2;
        out.heavy *= 2;
    }
    if emp {
        let (light, heavy) = emp_size;
        // tg passes the heavy size as the light range unless critical (then doubles both).
        out.emp = Some(if out.critical {
            (heavy * 2, light * 2)
        } else {
            (heavy, heavy)
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ports_with(cooling: PipeAir) -> (PipeAir, PipeAir, PipeAir, PipeAir) {
        (
            cooling,
            PipeAir::new(200.0),
            PipeAir::new(200.0),
            PipeAir::new(200.0),
        )
    }

    #[test]
    fn power_levels_follow_the_fusion_temperature() {
        assert_eq!(power_level_of(300.0), 0);
        assert_eq!(power_level_of(500.0), 0);
        assert_eq!(power_level_of(800.0), 1);
        assert_eq!(power_level_of(5e4), 3);
        assert_eq!(power_level_of(2e7), 6);
    }

    #[test]
    fn fuels_and_fusion_powers_match_tg() {
        assert_eq!(fuel("h2_t2_fuel"), Some(3));
        assert_eq!(FUELS[3].requirements, [GasId::HYDROGEN, GasId::TRITIUM]);
        assert_eq!(FUSION_POWER[usize::from(GasId::ANTINOBLIUM.0)], 20.0);
        assert_eq!(FUSION_POWER[usize::from(GasId::PLUOXIUM.0)], -10.0);
        assert!(FUELS.iter().all(|f| f.temperature_change_multiplier <= 1.0));
    }

    #[test]
    fn a_hot_fuelled_core_reacts_and_consumes_fuel() {
        let mut r = Reactor {
            start_power: true,
            selected_fuel: fuel("h2_t2_fuel"),
            ..Default::default()
        };
        r.fusion.gas.set(GasId::HYDROGEN, 1000.0);
        r.fusion.gas.set(GasId::TRITIUM, 1000.0);
        r.fusion.gas.temperature = 5e4;
        let (mut c, mut f, mut md, mut o) = ports_with(PipeAir::new(200.0));
        let mut ports = Ports {
            cooling: &mut c,
            fuel: &mut f,
            moderator: &mut md,
            output: &mut o,
        };
        let effects = r.process(0.5, true, false, &mut ports, &mut |_| false);
        assert_eq!(r.power_level, 3);
        assert!(r.fusion.gas.get(GasId::HYDROGEN) < 1000.0);
        assert!(r.fusion.gas.get(GasId::HELIUM) > 0.0);
        assert!(effects.radiation);
        assert!(r.energy > 0.0);
    }

    #[test]
    fn no_fuel_no_reaction_and_thin_mixes_heal() {
        let mut r = Reactor {
            start_power: true,
            critical_threshold_proximity: 100.0,
            ..Default::default()
        };
        r.fusion.gas.set(GasId::HYDROGEN, 10.0);
        r.fusion.gas.temperature = 300.0;
        let (mut c, mut f, mut md, mut o) = ports_with(PipeAir::new(200.0));
        let mut ports = Ports {
            cooling: &mut c,
            fuel: &mut f,
            moderator: &mut md,
            output: &mut o,
        };
        let effects = r.process(1.0, true, false, &mut ports, &mut |_| false);
        assert!(!effects.radiation);
        // (10 - 1200) / 400 per second.
        assert!((r.critical_threshold_proximity - (100.0 - 1190.0 / 400.0)).abs() < 1e-9);
    }

    #[test]
    fn unpowered_cores_force_bad_settings() {
        let mut r = Reactor {
            start_power: true,
            power_level: 2,
            current_damper: 500.0,
            ..Default::default()
        };
        let (mut c, mut f, mut md, mut o) = ports_with(PipeAir::new(200.0));
        let mut ports = Ports {
            cooling: &mut c,
            fuel: &mut f,
            moderator: &mut md,
            output: &mut o,
        };
        r.process(1.0, false, false, &mut ports, &mut |_| false);
        assert_eq!(r.heating_conductor, 500.0);
        assert_eq!(r.current_damper, 0.0);
        assert!((r.iron_content - 0.04).abs() < 1e-9);
    }

    #[test]
    fn hypercritical_moderators_crack_and_cracks_leak() {
        let mut r = Reactor::default();
        r.moderator.gas.set(GasId::N2, 20_000.0);
        r.moderator.gas.temperature = 300.0;
        // 20000 mol at 300 K in 10000 L: ~4986 kPa, under the medium spill pressure.
        assert_eq!(
            r.moderator_overflow(1.0, false, &mut |_| true),
            Some(Overflow::Crack { explosion: None })
        );
        let Some(Overflow::Leak(rate)) = r.moderator_overflow(1.0, true, &mut |_| true) else {
            panic!("a cracked part leaks");
        };
        assert!((rate - WEAK_SPILL_RATE).abs() < 1e-12);
        assert_eq!(r.moderator_overflow(1.0, true, &mut |_| false), None);
    }

    #[test]
    fn meltdown_sizes_follow_the_flags() {
        let h2t2 = meltdown(FUELS[3].meltdown_flags, 4);
        assert_eq!((h2t2.light, h2t2.heavy, h2t2.flash), (20, 2, 24));
        assert_eq!(h2t2.radiation_range, Some(28));
        assert_eq!(h2t2.emp, Some((12, 12)));
        assert_eq!((h2t2.gas_pockets, h2t2.gas_spread), (7, 16));
        let antinob = meltdown(FUELS[6].meltdown_flags, 6);
        assert!(antinob.critical);
        assert_eq!((antinob.devastation, antinob.heavy), (12, 24));
        assert_eq!(antinob.emp, Some((84, 108)));
    }
}
