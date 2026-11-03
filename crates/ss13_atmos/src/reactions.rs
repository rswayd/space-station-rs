//! tg gas reactions (`code/modules/atmospherics/gasmixtures/reactions.dm`,
//! `code/__DEFINES/reactions.dm`): [`GasMixture::react`] runs every reaction whose requirements
//! the mixture meets, in tg's priority order. Formulas are kept term for term.
//!
//! Order: tg builds the candidate list from the gases present (each reaction is keyed by its
//! rarest required gas, `major_gas`), concatenated by priority group (pre-formation, formation,
//! post-formation, fire); within a group by gas, then by declaration order. Gases are visited in
//! [`crate::GASES`] order here.
// tg visits them in the mixture's insertion order, which only reorders reactions of one
// priority group keyed by different gases (plasma and hydrogen fires in one mixture).
//
// Cost: a mixture without any reaction's key gas (station air: oxygen and nitrogen key no
// reaction) returns after one pass over its 20 mole counts.

use crate::gas::{
    CELL_VOLUME, GasId, GasMixture, MINIMUM_HEAT_CAPACITY, R_IDEAL_GAS_EQUATION, T0C, T20C, TCMB,
    quantize,
};

/// Reaction flags (`NO_REACTION`, `REACTING`, `STOP_REACTIONS`, `VOLATILE_REACTION`).
pub const NO_REACTION: u8 = 0;
pub const REACTING: u8 = 1;
pub const STOP_REACTIONS: u8 = 2;
pub const VOLATILE_REACTION: u8 = 4;

pub const MINIMUM_MOLE_COUNT: f64 = 0.01;
pub const MOLES_GAS_VISIBLE: f64 = 0.25;
/// DM `NUM_E` (2.71828183; the difference is below 1e-9).
const NUM_E: f64 = std::f64::consts::E;

pub const FIRE_MINIMUM_TEMPERATURE_TO_EXIST: f64 = 100.0 + T0C;
pub const FIRE_MINIMUM_TEMPERATURE_TO_SPREAD: f64 = 150.0 + T0C;
pub const FIRE_SPREAD_RADIOSITY_SCALE: f64 = 0.85;
pub const COLD_FIRE_SPREAD_RADIOSITY_SCALE: f64 = 0.95;
pub const FIRE_GROWTH_RATE: f64 = 40000.0;

pub const WATER_VAPOR_CONDENSATION_POINT: f64 = T20C + 10.0;
pub const WATER_VAPOR_DEPOSITION_POINT: f64 = 200.0;
pub const MIASTER_STERILIZATION_TEMP: f64 = FIRE_MINIMUM_TEMPERATURE_TO_EXIST + 70.0;
pub const MIASTER_STERILIZATION_MAX_HUMIDITY: f64 = 0.1;
pub const MIASTER_STERILIZATION_RATE_BASE: f64 = 20.0;
pub const MIASTER_STERILIZATION_RATE_SCALE: f64 = 20.0;
pub const MIASTER_STERILIZATION_ENERGY: f64 = 2e-3;
pub const PLASMA_MINIMUM_BURN_TEMPERATURE: f64 = FIRE_MINIMUM_TEMPERATURE_TO_EXIST;
pub const PLASMA_UPPER_TEMPERATURE: f64 = PLASMA_MINIMUM_BURN_TEMPERATURE + 1270.0;
pub const OXYGEN_BURN_RATIO_BASE: f64 = 1.4;
pub const PLASMA_OXYGEN_FULLBURN: f64 = 10.0;
pub const SUPER_SATURATION_THRESHOLD: f64 = 96.0;
pub const PLASMA_BURN_RATE_DELTA: f64 = 9.0;
pub const FIRE_PLASMA_ENERGY_RELEASED: f64 = 3e6;
pub const HYDROGEN_MINIMUM_BURN_TEMPERATURE: f64 = FIRE_MINIMUM_TEMPERATURE_TO_EXIST;
pub const FIRE_HYDROGEN_ENERGY_RELEASED: f64 = 2.8e6;
pub const HYDROGEN_OXYGEN_FULLBURN: f64 = 10.0;
pub const FIRE_HYDROGEN_BURN_RATE_DELTA: f64 = 2.0;
pub const TRITIUM_MINIMUM_BURN_TEMPERATURE: f64 = FIRE_MINIMUM_TEMPERATURE_TO_EXIST;
pub const FIRE_TRITIUM_ENERGY_RELEASED: f64 = FIRE_HYDROGEN_ENERGY_RELEASED;
pub const TRITIUM_OXYGEN_FULLBURN: f64 = HYDROGEN_OXYGEN_FULLBURN;
pub const FIRE_TRITIUM_BURN_RATE_DELTA: f64 = FIRE_HYDROGEN_BURN_RATE_DELTA;
pub const FREON_MAXIMUM_BURN_TEMPERATURE: f64 = 283.0;
pub const FREON_LOWER_TEMPERATURE: f64 = 60.0;
pub const FREON_TERMINAL_TEMPERATURE: f64 = 20.0;
pub const FREON_OXYGEN_FULLBURN: f64 = 10.0;
pub const FREON_BURN_RATE_DELTA: f64 = 4.0;
pub const FIRE_FREON_ENERGY_CONSUMED: f64 = 3e5;
pub const N2O_FORMATION_MIN_TEMPERATURE: f64 = 200.0;
pub const N2O_FORMATION_MAX_TEMPERATURE: f64 = 250.0;
pub const N2O_FORMATION_ENERGY: f64 = 10000.0;
pub const N2O_DECOMPOSITION_MIN_TEMPERATURE: f64 = 1400.0;
pub const N2O_DECOMPOSITION_MAX_TEMPERATURE: f64 = 100_000.0;
pub const N2O_DECOMPOSITION_RATE_DIVISOR: f64 = 2.0;
pub const N2O_DECOMPOSITION_MIN_SCALE_TEMP: f64 = 0.0;
pub const N2O_DECOMPOSITION_MAX_SCALE_TEMP: f64 = 100_000.0;
pub const N2O_DECOMPOSITION_SCALE_DIVISOR: f64 = (-1.0 / 4.0)
    * ((N2O_DECOMPOSITION_MAX_SCALE_TEMP - N2O_DECOMPOSITION_MIN_SCALE_TEMP)
        * (N2O_DECOMPOSITION_MAX_SCALE_TEMP - N2O_DECOMPOSITION_MIN_SCALE_TEMP));
pub const N2O_DECOMPOSITION_ENERGY: f64 = 200_000.0;
pub const BZ_FORMATION_MAX_TEMPERATURE: f64 = FIRE_MINIMUM_TEMPERATURE_TO_EXIST - 60.0;
pub const BZ_FORMATION_ENERGY: f64 = 80000.0;
pub const PLUOXIUM_FORMATION_MIN_TEMP: f64 = 50.0;
pub const PLUOXIUM_FORMATION_MAX_TEMP: f64 = T0C;
pub const PLUOXIUM_FORMATION_MAX_RATE: f64 = 5.0;
pub const PLUOXIUM_FORMATION_ENERGY: f64 = 250.0;
pub const NITRIUM_FORMATION_MIN_TEMP: f64 = 1500.0;
pub const NITRIUM_FORMATION_TEMP_DIVISOR: f64 = FIRE_MINIMUM_TEMPERATURE_TO_EXIST * 8.0;
pub const NITRIUM_FORMATION_ENERGY: f64 = 100_000.0;
pub const NITRIUM_DECOMPOSITION_MAX_TEMP: f64 = T0C + 70.0;
pub const NITRIUM_DECOMPOSITION_TEMP_DIVISOR: f64 = FIRE_MINIMUM_TEMPERATURE_TO_EXIST * 8.0;
pub const NITRIUM_DECOMPOSITION_ENERGY: f64 = 30000.0;
pub const FREON_FORMATION_MIN_TEMPERATURE: f64 = FIRE_MINIMUM_TEMPERATURE_TO_EXIST + 100.0;
pub const NOBLIUM_FORMATION_MIN_TEMP: f64 = TCMB;
pub const NOBLIUM_FORMATION_MAX_TEMP: f64 = 15.0;
pub const NOBLIUM_FORMATION_ENERGY: f64 = 2e7;
pub const REACTION_OPPRESSION_THRESHOLD: f64 = 5.0;
pub const REACTION_OPPRESSION_MIN_TEMP: f64 = 20.0;
pub const HALON_COMBUSTION_ENERGY: f64 = 2500.0;
pub const HALON_COMBUSTION_MIN_TEMPERATURE: f64 = T0C + 70.0;
pub const HALON_COMBUSTION_TEMPERATURE_SCALE: f64 = FIRE_MINIMUM_TEMPERATURE_TO_EXIST * 10.0;
pub const HEALIUM_FORMATION_MIN_TEMP: f64 = 25.0;
pub const HEALIUM_FORMATION_MAX_TEMP: f64 = 300.0;
pub const HEALIUM_FORMATION_ENERGY: f64 = 9000.0;
pub const ZAUKER_FORMATION_MIN_TEMPERATURE: f64 = 50000.0;
pub const ZAUKER_FORMATION_MAX_TEMPERATURE: f64 = 75000.0;
pub const ZAUKER_FORMATION_TEMPERATURE_SCALE: f64 = 5e-6;
pub const ZAUKER_FORMATION_ENERGY: f64 = 5000.0;
pub const ZAUKER_DECOMPOSITION_MAX_RATE: f64 = 20.0;
pub const ZAUKER_DECOMPOSITION_ENERGY: f64 = 460.0;
pub const PN_FORMATION_MIN_TEMPERATURE: f64 = 5000.0;
pub const PN_FORMATION_MAX_TEMPERATURE: f64 = 10000.0;
pub const PN_FORMATION_ENERGY: f64 = 650.0;
pub const PN_HYDROGEN_CONVERSION_THRESHOLD: f64 = 150.0;
pub const PN_HYDROGEN_CONVERSION_MAX_RATE: f64 = 5.0;
pub const PN_HYDROGEN_CONVERSION_ENERGY: f64 = 2500.0;
pub const PN_TRITIUM_CONVERSION_MIN_TEMP: f64 = 150.0;
pub const PN_TRITIUM_CONVERSION_MAX_TEMP: f64 = 340.0;
pub const PN_TRITIUM_CONVERSION_ENERGY: f64 = 10000.0;
pub const PN_TRITIUM_CONVERSION_RAD_RELEASE_THRESHOLD: f64 = 10000.0;
pub const PN_BZASE_MIN_TEMP: f64 = 260.0;
pub const PN_BZASE_MAX_TEMP: f64 = 280.0;
pub const PN_BZASE_ENERGY: f64 = 60000.0;
pub const PN_BZASE_RAD_RELEASE_THRESHOLD: f64 = 60000.0;
pub const ANTINOBLIUM_CONVERSION_DIVISOR: f64 = 90.0;
pub const ATMOS_RADIATION_VOLUME_EXP: i32 = 3;

/// The reactions whose results grow hotspots (`expands_hotspot`), in
/// [`GasMixture::reaction_results`] order: plasma, hydrogen, tritium and freon fires.
pub const FIRE_RESULTS: usize = 4;
const PLASMA_FIRE: usize = 0;
const H2_FIRE: usize = 1;
const TRIT_FIRE: usize = 2;
const FREON_FIRE: usize = 3;

/// `reaction_results` of the fire reactions a mixture last ran: (id, name, amount), nonzero.
pub fn fire_results(air: &GasMixture) -> Vec<(&'static str, &'static str, f64)> {
    const FIRES: [(&str, &str); FIRE_RESULTS] = [
        ("plasmafire", "Plasma Combustion"),
        ("h2fire", "Hydrogen Combustion"),
        ("tritfire", "Tritium Combustion"),
        ("freonfire", "Freon Combustion"),
    ];
    FIRES
        .iter()
        .zip(air.reaction_results)
        .filter(|(_, amount)| *amount != 0.0)
        .map(|(&(id, name), amount)| (id, name, amount))
        .collect()
}

/// What tg passes as `holder` to `react()`: reactions that touch the location (fires expose
/// the turf, steam condenses on it) do that only for a turf.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Holder {
    /// An open turf's own air.
    Turf,
    /// The air sample a hotspot reacts (`perform_exposure`).
    Hotspot,
    /// A gas tank or anything else.
    Other,
}

/// `location.hotspot_expose(temperature, CELL_VOLUME)` calls the fire reactions of a turf's
/// air made, in order, for the caller to apply after the reactions.
// tg calls it in the middle of `react()` (a new hotspot samples the air before the other fire
// reactions of the same mixture run); one fire tick later the result is the same.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Exposures {
    temperatures: [f64; FIRE_RESULTS],
    len: usize,
    /// Vapor condensed on the turf: `water_vapor_gas_act` or `freeze_turf`.
    pub condensed: Option<Condensation>,
}

/// What water vapor did to the turf it condensed on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Condensation {
    /// `water_vapor_gas_act`: wet (`TURF_WET_WATER`, 10 s, +5 s) and washed; not on
    /// groundless or no-slip turfs.
    Wet,
    /// `freeze_turf`: permafrost for 10 s, objects freeze, cold mobs get frozen.
    Freeze,
}

impl Exposures {
    fn push(&mut self, temperature: f64) {
        if self.len < FIRE_RESULTS {
            self.temperatures[self.len] = temperature;
            self.len += 1;
        }
    }

    /// Exposure temperatures, each at `CELL_VOLUME`.
    pub fn temperatures(&self) -> &[f64] {
        &self.temperatures[..self.len]
    }
}

struct Ctx {
    holder: Holder,
    /// Litres of the mixture (`air.volume`).
    volume: f64,
    exposures: Exposures,
}

type ReactFn = fn(&mut GasMixture, &mut Ctx) -> u8;

struct Reaction {
    /// tg `id`.
    id: &'static str,
    /// `PRIORITY_PRE_FORMATION` (1) .. `PRIORITY_FIRE` (4).
    priority: u8,
    /// `major_gas`: the rarest required gas.
    major: GasId,
    reqs: &'static [(GasId, f64)],
    min_temp: Option<f64>,
    max_temp: Option<f64>,
    react: ReactFn,
}

const fn r(
    id: &'static str,
    priority: u8,
    major: GasId,
    reqs: &'static [(GasId, f64)],
    min_temp: Option<f64>,
    max_temp: Option<f64>,
    react: ReactFn,
) -> Reaction {
    Reaction {
        id,
        priority,
        major,
        reqs,
        min_temp,
        max_temp,
        react,
    }
}

use GasId as G;

/// Every `/datum/gas_reaction/standard` subtype in declaration order.
const TABLE: [Reaction; 23] = [
    r(
        "vapor",
        3,
        G::WATER_VAPOR,
        &[(G::WATER_VAPOR, MOLES_GAS_VISIBLE)],
        None,
        Some(WATER_VAPOR_CONDENSATION_POINT),
        water_vapor,
    ),
    r(
        "sterilization",
        3,
        G::MIASMA,
        &[(G::MIASMA, MINIMUM_MOLE_COUNT)],
        Some(MIASTER_STERILIZATION_TEMP),
        None,
        miaster,
    ),
    r(
        "plasmafire",
        4,
        G::PLASMA,
        &[(G::PLASMA, MINIMUM_MOLE_COUNT), (G::O2, MINIMUM_MOLE_COUNT)],
        Some(PLASMA_MINIMUM_BURN_TEMPERATURE),
        None,
        plasmafire,
    ),
    r(
        "h2fire",
        4,
        G::HYDROGEN,
        &[
            (G::HYDROGEN, MINIMUM_MOLE_COUNT),
            (G::O2, MINIMUM_MOLE_COUNT),
        ],
        Some(HYDROGEN_MINIMUM_BURN_TEMPERATURE),
        None,
        h2fire,
    ),
    r(
        "tritfire",
        4,
        G::TRITIUM,
        &[
            (G::TRITIUM, MINIMUM_MOLE_COUNT),
            (G::O2, MINIMUM_MOLE_COUNT),
        ],
        Some(TRITIUM_MINIMUM_BURN_TEMPERATURE),
        None,
        tritfire,
    ),
    r(
        "freonfire",
        4,
        G::FREON,
        &[(G::O2, MINIMUM_MOLE_COUNT), (G::FREON, MINIMUM_MOLE_COUNT)],
        Some(FREON_TERMINAL_TEMPERATURE),
        Some(FREON_MAXIMUM_BURN_TEMPERATURE),
        freonfire,
    ),
    r(
        "nitrousformation",
        2,
        G::BZ,
        &[(G::O2, 10.0), (G::N2, 20.0), (G::BZ, 5.0)],
        Some(N2O_FORMATION_MIN_TEMPERATURE),
        Some(N2O_FORMATION_MAX_TEMPERATURE),
        nitrous_formation,
    ),
    r(
        "nitrous_decomp",
        3,
        G::N2O,
        &[(G::N2O, MINIMUM_MOLE_COUNT * 2.0)],
        Some(N2O_DECOMPOSITION_MIN_TEMPERATURE),
        Some(N2O_DECOMPOSITION_MAX_TEMPERATURE),
        nitrous_decomp,
    ),
    r(
        "bzformation",
        2,
        G::N2O,
        &[(G::N2O, 10.0), (G::PLASMA, 10.0)],
        None,
        Some(BZ_FORMATION_MAX_TEMPERATURE),
        bz_formation,
    ),
    r(
        "pluox_formation",
        2,
        G::TRITIUM,
        &[
            (G::CO2, MINIMUM_MOLE_COUNT),
            (G::O2, MINIMUM_MOLE_COUNT),
            (G::TRITIUM, MINIMUM_MOLE_COUNT),
        ],
        Some(PLUOXIUM_FORMATION_MIN_TEMP),
        Some(PLUOXIUM_FORMATION_MAX_TEMP),
        pluox_formation,
    ),
    r(
        "nitrium_formation",
        2,
        G::TRITIUM,
        &[(G::TRITIUM, 20.0), (G::N2, 10.0), (G::BZ, 5.0)],
        Some(NITRIUM_FORMATION_MIN_TEMP),
        None,
        nitrium_formation,
    ),
    r(
        "nitrium_decomp",
        1,
        G::NITRIUM,
        &[
            (G::O2, MINIMUM_MOLE_COUNT),
            (G::NITRIUM, MINIMUM_MOLE_COUNT),
        ],
        None,
        Some(NITRIUM_DECOMPOSITION_MAX_TEMP),
        nitrium_decomposition,
    ),
    r(
        "freonformation",
        2,
        G::BZ,
        &[
            (G::PLASMA, MINIMUM_MOLE_COUNT * 6.0),
            (G::CO2, MINIMUM_MOLE_COUNT * 3.0),
            (G::BZ, MINIMUM_MOLE_COUNT),
        ],
        Some(FREON_FORMATION_MIN_TEMPERATURE),
        None,
        freon_formation,
    ),
    r(
        "nobformation",
        2,
        G::TRITIUM,
        &[(G::N2, 10.0), (G::TRITIUM, 5.0)],
        Some(NOBLIUM_FORMATION_MIN_TEMP),
        Some(NOBLIUM_FORMATION_MAX_TEMP),
        noblium_formation,
    ),
    r(
        "halon_o2removal",
        1,
        G::HALON,
        &[(G::HALON, MINIMUM_MOLE_COUNT), (G::O2, MINIMUM_MOLE_COUNT)],
        Some(HALON_COMBUSTION_MIN_TEMPERATURE),
        None,
        halon_o2removal,
    ),
    r(
        "healium_formation",
        2,
        G::FREON,
        &[(G::BZ, MINIMUM_MOLE_COUNT), (G::FREON, MINIMUM_MOLE_COUNT)],
        Some(HEALIUM_FORMATION_MIN_TEMP),
        Some(HEALIUM_FORMATION_MAX_TEMP),
        healium_formation,
    ),
    r(
        "zauker_formation",
        2,
        G::NITRIUM,
        &[
            (G::HYPERNOBLIUM, MINIMUM_MOLE_COUNT),
            (G::NITRIUM, MINIMUM_MOLE_COUNT),
        ],
        Some(ZAUKER_FORMATION_MIN_TEMPERATURE),
        Some(ZAUKER_FORMATION_MAX_TEMPERATURE),
        zauker_formation,
    ),
    r(
        "zauker_decomp",
        3,
        G::ZAUKER,
        &[(G::N2, MINIMUM_MOLE_COUNT), (G::ZAUKER, MINIMUM_MOLE_COUNT)],
        None,
        None,
        zauker_decomp,
    ),
    r(
        "proto_nitrate_formation",
        2,
        G::PLUOXIUM,
        &[
            (G::PLUOXIUM, MINIMUM_MOLE_COUNT),
            (G::HYDROGEN, MINIMUM_MOLE_COUNT),
        ],
        Some(PN_FORMATION_MIN_TEMPERATURE),
        Some(PN_FORMATION_MAX_TEMPERATURE),
        proto_nitrate_formation,
    ),
    r(
        "proto_nitrate_hydrogen_response",
        1,
        G::PROTO_NITRATE,
        &[
            (G::PROTO_NITRATE, MINIMUM_MOLE_COUNT),
            (G::HYDROGEN, PN_HYDROGEN_CONVERSION_THRESHOLD),
        ],
        None,
        None,
        proto_nitrate_hydrogen_response,
    ),
    r(
        "proto_nitrate_tritium_response",
        1,
        G::PROTO_NITRATE,
        &[
            (G::PROTO_NITRATE, MINIMUM_MOLE_COUNT),
            (G::TRITIUM, MINIMUM_MOLE_COUNT),
        ],
        Some(PN_TRITIUM_CONVERSION_MIN_TEMP),
        Some(PN_TRITIUM_CONVERSION_MAX_TEMP),
        proto_nitrate_tritium_response,
    ),
    r(
        "proto_nitrate_bz_response",
        1,
        G::PROTO_NITRATE,
        &[
            (G::PROTO_NITRATE, MINIMUM_MOLE_COUNT),
            (G::BZ, MINIMUM_MOLE_COUNT),
        ],
        Some(PN_BZASE_MIN_TEMP),
        Some(PN_BZASE_MAX_TEMP),
        proto_nitrate_bz_response,
    ),
    r(
        "antinoblium_replication",
        2,
        G::ANTINOBLIUM,
        &[(G::ANTINOBLIUM, MOLES_GAS_VISIBLE)],
        Some(REACTION_OPPRESSION_MIN_TEMP),
        None,
        antinoblium_replication,
    ),
];

static REACTIONS: [Reaction; 23] = TABLE;

/// Bit per gas that keys at least one reaction.
const fn major_mask(table: &[Reaction]) -> u32 {
    let mut mask = 0;
    let mut i = 0;
    while i < table.len() {
        mask |= 1 << table[i].major.0;
        i += 1;
    }
    mask
}

const MAJOR_MASK: u32 = major_mask(&TABLE);

/// The tg `id` of every reaction, in declaration order.
pub fn reaction_ids() -> impl Iterator<Item = &'static str> {
    REACTIONS.iter().map(|r| r.id)
}

impl GasMixture {
    /// `react(holder)` of a `volume`-litre mixture: every reaction whose requirements hold, in
    /// tg order. Returns the reaction flags; `exposures` gets the turf exposures fires asked for
    /// (only with [`Holder::Turf`]).
    pub fn react(&mut self, holder: Holder, volume: f64, exposures: &mut Exposures) -> u8 {
        let mut present = 0u32;
        for (g, &m) in self.moles.iter().enumerate() {
            if m != 0.0 {
                present |= 1 << g;
            }
        }
        let keys = present & MAJOR_MASK;
        if keys == 0 {
            return NO_REACTION;
        }
        if self.get(G::HYPERNOBLIUM) >= REACTION_OPPRESSION_THRESHOLD
            && self.temperature > REACTION_OPPRESSION_MIN_TEMP
        {
            return STOP_REACTIONS;
        }
        self.reaction_results = [0.0; FIRE_RESULTS];
        let temp = self.temperature;
        let mut ctx = Ctx {
            holder,
            volume,
            exposures: Exposures::default(),
        };
        let mut flags = NO_REACTION;
        for priority in 1..=4 {
            for g in 0..crate::GAS_COUNT as u8 {
                if keys & (1 << g) == 0 {
                    continue;
                }
                for reaction in REACTIONS
                    .iter()
                    .filter(|r| r.priority == priority && r.major.0 == g)
                {
                    if reaction.min_temp.is_some_and(|t| temp < t)
                        || reaction.max_temp.is_some_and(|t| temp > t)
                        || reaction.reqs.iter().any(|&(id, v)| self.get(id) < v)
                    {
                        continue;
                    }
                    flags |= (reaction.react)(self, &mut ctx);
                }
            }
        }
        if flags != NO_REACTION {
            self.garbage_collect();
        }
        *exposures = ctx.exposures;
        flags
    }

    /// Hotspot volume the last reactions add (`cached_results[reaction] * FIRE_GROWTH_RATE` over
    /// `SSair.hotspot_reactions`).
    pub fn hotspot_growth(&self) -> f64 {
        self.reaction_results
            .iter()
            .map(|r| r * FIRE_GROWTH_RATE)
            .sum()
    }

    /// `return_pressure` at `volume` litres.
    fn pressure_at(&self, volume: f64) -> f64 {
        if volume > 0.0 {
            self.total_moles() * R_IDEAL_GAS_EQUATION * self.temperature / volume
        } else {
            0.0
        }
    }

    /// `(temperature * old_heat_capacity + energy) / new_heat_capacity` when the new capacity
    /// exceeds `MINIMUM_HEAT_CAPACITY`, optionally floored at TCMB.
    fn release(&mut self, temperature: f64, old_heat_capacity: f64, energy: f64, floor: bool) {
        let new_heat_capacity = self.heat_capacity();
        if new_heat_capacity > MINIMUM_HEAT_CAPACITY {
            let t = (temperature * old_heat_capacity + energy) / new_heat_capacity;
            self.temperature = if floor { t.max(TCMB) } else { t };
        }
    }
}

fn water_vapor(air: &mut GasMixture, ctx: &mut Ctx) -> u8 {
    if ctx.holder != Holder::Turf {
        return NO_REACTION;
    }
    let mut flags = NO_REACTION;
    let consumed = if air.temperature <= WATER_VAPOR_DEPOSITION_POINT {
        // `freeze_turf()` always freezes an open turf: its permafrost keeps the group volatile.
        flags |= VOLATILE_REACTION;
        ctx.exposures.condensed = Some(Condensation::Freeze);
        MOLES_GAS_VISIBLE
    } else if air.temperature <= WATER_VAPOR_CONDENSATION_POINT {
        ctx.exposures.condensed = Some(Condensation::Wet);
        MOLES_GAS_VISIBLE
    } else {
        0.0
    };
    if consumed != 0.0 {
        air.add(G::WATER_VAPOR, -consumed);
        flags |= REACTING;
    }
    flags
}

fn miaster(air: &mut GasMixture, _: &mut Ctx) -> u8 {
    let water_vapor_moles = air.get(G::WATER_VAPOR);
    let miasma_moles = air.get(G::MIASMA);
    if water_vapor_moles != 0.0
        && water_vapor_moles / air.total_moles() > MIASTER_STERILIZATION_MAX_HUMIDITY
    {
        return NO_REACTION;
    }
    let cleaned_air = miasma_moles.min(
        MIASTER_STERILIZATION_RATE_BASE
            + (air.temperature - MIASTER_STERILIZATION_TEMP) / MIASTER_STERILIZATION_RATE_SCALE,
    );
    air.add(G::MIASMA, -cleaned_air);
    air.add(G::O2, cleaned_air);
    air.temperature += cleaned_air * MIASTER_STERILIZATION_ENERGY;
    REACTING
}

/// The turf exposure every fire reaction ends with (`location.hotspot_expose(temperature,
/// CELL_VOLUME)`), above `FIRE_MINIMUM_TEMPERATURE_TO_EXIST`.
fn expose_hot(air: &GasMixture, ctx: &mut Ctx) {
    if ctx.holder == Holder::Turf && air.temperature > FIRE_MINIMUM_TEMPERATURE_TO_EXIST {
        ctx.exposures.push(air.temperature);
    }
}

fn plasmafire(air: &mut GasMixture, ctx: &mut Ctx) -> u8 {
    let temperature = air.temperature;
    let temperature_scale = if temperature > PLASMA_UPPER_TEMPERATURE {
        1.0
    } else {
        let scale = (temperature - PLASMA_MINIMUM_BURN_TEMPERATURE)
            / (PLASMA_UPPER_TEMPERATURE - PLASMA_MINIMUM_BURN_TEMPERATURE);
        if scale <= 0.0 {
            return NO_REACTION;
        }
        scale
    };
    let oxygen_burn_ratio = OXYGEN_BURN_RATIO_BASE - temperature_scale;
    let oxygen_moles = air.get(G::O2);
    let plasma_moles = air.get(G::PLASMA);
    let ratio = oxygen_moles / plasma_moles;
    let mut super_saturation = false;
    let mut plasma_burn_rate = if ratio >= SUPER_SATURATION_THRESHOLD {
        super_saturation = true;
        (plasma_moles / PLASMA_BURN_RATE_DELTA) * temperature_scale
    } else if ratio >= PLASMA_OXYGEN_FULLBURN {
        (plasma_moles / PLASMA_BURN_RATE_DELTA) * temperature_scale
    } else {
        ((oxygen_moles / PLASMA_OXYGEN_FULLBURN) / PLASMA_BURN_RATE_DELTA) * temperature_scale
    };
    if plasma_burn_rate < MINIMUM_HEAT_CAPACITY {
        return NO_REACTION;
    }
    let old_heat_capacity = air.heat_capacity();
    plasma_burn_rate = plasma_burn_rate
        .min(plasma_moles)
        .min(oxygen_moles * (1.0 / oxygen_burn_ratio));
    air.set(G::PLASMA, quantize(plasma_moles - plasma_burn_rate));
    air.set(
        G::O2,
        quantize(oxygen_moles - plasma_burn_rate * oxygen_burn_ratio),
    );
    if super_saturation {
        air.add(G::TRITIUM, plasma_burn_rate);
    } else {
        air.add(G::CO2, plasma_burn_rate * 0.75);
        air.add(G::WATER_VAPOR, plasma_burn_rate * 0.25);
    }
    air.reaction_results[PLASMA_FIRE] = plasma_burn_rate * (1.0 + oxygen_burn_ratio);
    let energy_released = FIRE_PLASMA_ENERGY_RELEASED * plasma_burn_rate;
    air.release(temperature, old_heat_capacity, energy_released, false);
    expose_hot(air, ctx);
    REACTING | VOLATILE_REACTION
}

/// Hydrogen and tritium burn alike (`h2fire`, `tritfire`).
fn hydrogen_like_fire(
    air: &mut GasMixture,
    ctx: &mut Ctx,
    fuel: GasId,
    delta: f64,
    fullburn: f64,
    energy: f64,
    result: usize,
) -> u8 {
    let fuel_moles = air.get(fuel);
    let oxygen_moles = air.get(G::O2);
    let old_heat_capacity = air.heat_capacity();
    let temperature = air.temperature;
    let burned_fuel = (fuel_moles / delta)
        .min(oxygen_moles / (delta * fullburn))
        .min(fuel_moles)
        .min(oxygen_moles * (1.0 / 0.5));
    if burned_fuel <= 0.0
        || fuel_moles - burned_fuel < 0.0
        || oxygen_moles - burned_fuel * 0.5 < 0.0
    {
        return NO_REACTION;
    }
    air.add(fuel, -burned_fuel);
    air.add(G::O2, -burned_fuel * 0.5);
    air.add(G::WATER_VAPOR, burned_fuel);
    air.reaction_results[result] = burned_fuel;
    let energy_released = energy * burned_fuel;
    if energy_released > 0.0 {
        air.release(temperature, old_heat_capacity, energy_released, false);
    }
    expose_hot(air, ctx);
    REACTING | VOLATILE_REACTION
}

fn h2fire(air: &mut GasMixture, ctx: &mut Ctx) -> u8 {
    hydrogen_like_fire(
        air,
        ctx,
        G::HYDROGEN,
        FIRE_HYDROGEN_BURN_RATE_DELTA,
        HYDROGEN_OXYGEN_FULLBURN,
        FIRE_HYDROGEN_ENERGY_RELEASED,
        H2_FIRE,
    )
}

// shortcut: no radiation pulse (`TRITIUM_RADIATION_*`, 10% chance over the energy threshold);
// add with radiation.
fn tritfire(air: &mut GasMixture, ctx: &mut Ctx) -> u8 {
    hydrogen_like_fire(
        air,
        ctx,
        G::TRITIUM,
        FIRE_TRITIUM_BURN_RATE_DELTA,
        TRITIUM_OXYGEN_FULLBURN,
        FIRE_TRITIUM_ENERGY_RELEASED,
        TRIT_FIRE,
    )
}

// shortcut: no hot ice sheets (2% between 120 and 160 K); add with the hot ice item.
fn freonfire(air: &mut GasMixture, ctx: &mut Ctx) -> u8 {
    let temperature = air.temperature;
    let temperature_scale = if temperature < FREON_TERMINAL_TEMPERATURE {
        0.0
    } else if temperature < FREON_LOWER_TEMPERATURE {
        0.5
    } else {
        (FREON_MAXIMUM_BURN_TEMPERATURE - temperature)
            / (FREON_MAXIMUM_BURN_TEMPERATURE - FREON_TERMINAL_TEMPERATURE)
    };
    if temperature_scale <= 0.0 {
        return NO_REACTION;
    }
    let oxygen_burn_ratio = OXYGEN_BURN_RATIO_BASE - temperature_scale;
    let freon_moles = air.get(G::FREON);
    let oxygen_moles = air.get(G::O2);
    let mut freon_burn_rate = if oxygen_moles < freon_moles * FREON_OXYGEN_FULLBURN {
        ((oxygen_moles / FREON_OXYGEN_FULLBURN) / FREON_BURN_RATE_DELTA) * temperature_scale
    } else {
        (freon_moles / FREON_BURN_RATE_DELTA) * temperature_scale
    };
    if freon_burn_rate < MINIMUM_HEAT_CAPACITY {
        return NO_REACTION;
    }
    let old_heat_capacity = air.heat_capacity();
    freon_burn_rate = freon_burn_rate
        .min(freon_moles)
        .min(oxygen_moles * (1.0 / oxygen_burn_ratio));
    air.set(G::FREON, quantize(freon_moles - freon_burn_rate));
    air.set(
        G::O2,
        quantize(oxygen_moles - freon_burn_rate * oxygen_burn_ratio),
    );
    air.add(G::CO2, freon_burn_rate);
    air.reaction_results[FREON_FIRE] = freon_burn_rate * (1.0 + oxygen_burn_ratio);
    let energy_consumed = FIRE_FREON_ENERGY_CONSUMED * freon_burn_rate;
    air.release(temperature, old_heat_capacity, -energy_consumed, true);
    if ctx.holder == Holder::Turf && air.temperature < FREON_MAXIMUM_BURN_TEMPERATURE {
        ctx.exposures.push(air.temperature);
    }
    REACTING | VOLATILE_REACTION
}

fn nitrous_formation(air: &mut GasMixture, _: &mut Ctx) -> u8 {
    let oxygen_moles = air.get(G::O2);
    let nitrogen_moles = air.get(G::N2);
    let heat_efficiency = (oxygen_moles * (1.0 / 0.5)).min(nitrogen_moles);
    if oxygen_moles - heat_efficiency * 0.5 < 0.0 || nitrogen_moles - heat_efficiency < 0.0 {
        return NO_REACTION;
    }
    let old_heat_capacity = air.heat_capacity();
    air.add(G::O2, -heat_efficiency * 0.5);
    air.add(G::N2, -heat_efficiency);
    air.add(G::N2O, heat_efficiency);
    let t = air.temperature;
    air.release(
        t,
        old_heat_capacity,
        heat_efficiency * N2O_FORMATION_ENERGY,
        true,
    );
    REACTING
}

fn nitrous_decomp(air: &mut GasMixture, _: &mut Ctx) -> u8 {
    let nitrous_oxide_moles = air.get(G::N2O);
    let temperature = air.temperature;
    let burned_fuel = (nitrous_oxide_moles / N2O_DECOMPOSITION_RATE_DIVISOR)
        * ((temperature - N2O_DECOMPOSITION_MIN_SCALE_TEMP)
            * (temperature - N2O_DECOMPOSITION_MAX_SCALE_TEMP)
            / N2O_DECOMPOSITION_SCALE_DIVISOR);
    if burned_fuel <= 0.0 || nitrous_oxide_moles - burned_fuel < 0.0 {
        return NO_REACTION;
    }
    let old_heat_capacity = air.heat_capacity();
    air.add(G::N2O, -burned_fuel);
    air.add(G::N2, burned_fuel);
    air.add(G::O2, burned_fuel / 2.0);
    air.release(
        temperature,
        old_heat_capacity,
        N2O_DECOMPOSITION_ENERGY * burned_fuel,
        false,
    );
    REACTING
}

fn bz_formation(air: &mut GasMixture, ctx: &mut Ctx) -> u8 {
    let nitrous_oxide_moles = air.get(G::N2O);
    let plasma_moles = air.get(G::PLASMA);
    let pressure = air.pressure_at(ctx.volume);
    let environment_efficiency = ctx.volume / pressure;
    let ratio_efficiency = (nitrous_oxide_moles / plasma_moles).min(1.0);
    let decomposed_factor =
        (4.0 * (plasma_moles / (nitrous_oxide_moles + plasma_moles) - 0.75)).max(0.0);
    let bz_formed = (0.01 * ratio_efficiency * environment_efficiency)
        .min(nitrous_oxide_moles * (1.0 / 0.4))
        .min(plasma_moles * (1.0 / (0.8 * (1.0 - decomposed_factor))));
    if nitrous_oxide_moles - bz_formed * 0.4 < 0.0
        || plasma_moles - 0.8 * bz_formed * (1.0 - decomposed_factor) < 0.0
        || bz_formed <= 0.0
    {
        return NO_REACTION;
    }
    let old_heat_capacity = air.heat_capacity();
    if decomposed_factor > 0.0 {
        let amount_decomposed = 0.4 * bz_formed * decomposed_factor;
        air.add(G::N2, amount_decomposed);
        air.add(G::O2, 0.5 * amount_decomposed);
    }
    air.add(G::BZ, bz_formed * (1.0 - decomposed_factor));
    air.add(G::N2O, -0.4 * bz_formed);
    air.add(G::PLASMA, -0.8 * bz_formed * (1.0 - decomposed_factor));
    let energy_released = bz_formed
        * (BZ_FORMATION_ENERGY
            + decomposed_factor * (N2O_DECOMPOSITION_ENERGY - BZ_FORMATION_ENERGY));
    let t = air.temperature;
    air.release(t, old_heat_capacity, energy_released, true);
    REACTING
}

fn pluox_formation(air: &mut GasMixture, _: &mut Ctx) -> u8 {
    let carbon_dioxide_moles = air.get(G::CO2);
    let oxygen_moles = air.get(G::O2);
    let tritium_moles = air.get(G::TRITIUM);
    let produced_amount = PLUOXIUM_FORMATION_MAX_RATE
        .min(carbon_dioxide_moles)
        .min(oxygen_moles * (1.0 / 0.5))
        .min(tritium_moles * (1.0 / 0.01));
    if produced_amount <= 0.0
        || carbon_dioxide_moles - produced_amount < 0.0
        || oxygen_moles - produced_amount * 0.5 < 0.0
        || tritium_moles - produced_amount * 0.01 < 0.0
    {
        return NO_REACTION;
    }
    let old_heat_capacity = air.heat_capacity();
    air.add(G::CO2, -produced_amount);
    air.add(G::O2, -produced_amount * 0.5);
    air.add(G::TRITIUM, -produced_amount * 0.01);
    air.add(G::PLUOXIUM, produced_amount);
    air.add(G::HYDROGEN, produced_amount * 0.01);
    let t = air.temperature;
    air.release(
        t,
        old_heat_capacity,
        produced_amount * PLUOXIUM_FORMATION_ENERGY,
        true,
    );
    REACTING
}

fn nitrium_formation(air: &mut GasMixture, _: &mut Ctx) -> u8 {
    let tritium_moles = air.get(G::TRITIUM);
    let nitrogen_moles = air.get(G::N2);
    let bz_moles = air.get(G::BZ);
    let temperature = air.temperature;
    let heat_efficiency = (temperature / NITRIUM_FORMATION_TEMP_DIVISOR)
        .min(tritium_moles)
        .min(nitrogen_moles)
        .min(bz_moles * (1.0 / 0.05));
    if heat_efficiency <= 0.0
        || tritium_moles - heat_efficiency < 0.0
        || nitrogen_moles - heat_efficiency < 0.0
        || bz_moles - heat_efficiency * 0.05 < 0.0
    {
        return NO_REACTION;
    }
    let old_heat_capacity = air.heat_capacity();
    air.add(G::TRITIUM, -heat_efficiency);
    air.add(G::N2, -heat_efficiency);
    air.add(G::BZ, -heat_efficiency * 0.05);
    air.add(G::NITRIUM, heat_efficiency);
    air.release(
        temperature,
        old_heat_capacity,
        -heat_efficiency * NITRIUM_FORMATION_ENERGY,
        true,
    );
    REACTING
}

fn nitrium_decomposition(air: &mut GasMixture, _: &mut Ctx) -> u8 {
    let nitrium_moles = air.get(G::NITRIUM);
    let temperature = air.temperature;
    let heat_efficiency = (temperature / NITRIUM_DECOMPOSITION_TEMP_DIVISOR).min(nitrium_moles);
    if heat_efficiency <= 0.0 || nitrium_moles - heat_efficiency < 0.0 {
        return NO_REACTION;
    }
    let old_heat_capacity = air.heat_capacity();
    air.add(G::NITRIUM, -heat_efficiency);
    air.add(G::HYDROGEN, heat_efficiency);
    air.add(G::N2, heat_efficiency);
    air.release(
        temperature,
        old_heat_capacity,
        heat_efficiency * NITRIUM_DECOMPOSITION_ENERGY,
        true,
    );
    REACTING
}

fn freon_formation(air: &mut GasMixture, _: &mut Ctx) -> u8 {
    let plasma_moles = air.get(G::PLASMA);
    let carbon_dioxide_moles = air.get(G::CO2);
    let bz_moles = air.get(G::BZ);
    let temperature = air.temperature;
    let minimal_mole_factor = (plasma_moles * (1.0 / 0.6))
        .min(bz_moles * (1.0 / 0.1))
        .min(carbon_dioxide_moles * (1.0 / 0.3));
    let equation_first_part = NUM_E.powf(-((temperature - 800.0) / 200.0).powi(2));
    let equation_second_part = 3.0 / (1.0 + NUM_E.powf(-0.001 * (temperature - 6000.0)));
    let heat_factor = equation_first_part + equation_second_part;
    let freon_formed = (heat_factor * minimal_mole_factor * 0.05)
        .min(plasma_moles * (1.0 / 0.6))
        .min(carbon_dioxide_moles * (1.0 / 0.3))
        .min(bz_moles * (1.0 / 0.1));
    if freon_formed <= 0.0
        || plasma_moles - freon_formed * 0.6 < 0.0
        || carbon_dioxide_moles - freon_formed * 0.3 < 0.0
        || bz_moles - freon_formed * 0.1 < 0.0
    {
        return NO_REACTION;
    }
    let old_heat_capacity = air.heat_capacity();
    air.add(G::PLASMA, -freon_formed * 0.6);
    air.add(G::CO2, -freon_formed * 0.3);
    air.add(G::BZ, -freon_formed * 0.1);
    air.add(G::FREON, freon_formed);
    let energy_consumed = (7000.0 / (1.0 + NUM_E.powf(-0.0015 * (temperature - 6000.0))) + 1000.0)
        * freon_formed
        * 0.1;
    air.release(temperature, old_heat_capacity, -energy_consumed, true);
    REACTING
}

fn noblium_formation(air: &mut GasMixture, _: &mut Ctx) -> u8 {
    let nitrogen_moles = air.get(G::N2);
    let tritium_moles = air.get(G::TRITIUM);
    let bz_moles = air.get(G::BZ);
    let reduction_factor = (tritium_moles / (tritium_moles + bz_moles)).clamp(0.001, 1.0);
    let nob_formed = ((nitrogen_moles + tritium_moles) * 0.01)
        .min(tritium_moles * (1.0 / (5.0 * reduction_factor)))
        .min(nitrogen_moles * (1.0 / 10.0));
    if quantize(nob_formed) <= 0.0
        || quantize(tritium_moles - 5.0 * nob_formed * reduction_factor) < 0.0
        || quantize(nitrogen_moles - 10.0 * nob_formed) < 0.0
    {
        return NO_REACTION;
    }
    let old_heat_capacity = air.heat_capacity();
    air.add(G::TRITIUM, -5.0 * nob_formed * reduction_factor);
    air.add(G::N2, -10.0 * nob_formed);
    air.add(G::HYPERNOBLIUM, nob_formed);
    let energy_released = nob_formed * NOBLIUM_FORMATION_ENERGY / bz_moles.max(1.0);
    let t = air.temperature;
    air.release(t, old_heat_capacity, energy_released, true);
    REACTING | VOLATILE_REACTION
}

// shortcut: no resin foam (`do_foam`, which also made the group volatile); add with foam.
fn halon_o2removal(air: &mut GasMixture, _: &mut Ctx) -> u8 {
    let halon_moles = air.get(G::HALON);
    let oxygen_moles = air.get(G::O2);
    let temperature = air.temperature;
    let heat_efficiency = (temperature / HALON_COMBUSTION_TEMPERATURE_SCALE)
        .min(halon_moles)
        .min(oxygen_moles * (1.0 / 20.0));
    if heat_efficiency <= 0.0
        || halon_moles - heat_efficiency < 0.0
        || oxygen_moles - heat_efficiency * 20.0 < 0.0
    {
        return NO_REACTION;
    }
    let old_heat_capacity = air.heat_capacity();
    air.add(G::HALON, -heat_efficiency);
    air.add(G::O2, -heat_efficiency * 20.0);
    air.add(G::PLUOXIUM, heat_efficiency * 2.5);
    air.release(
        temperature,
        old_heat_capacity,
        -heat_efficiency * HALON_COMBUSTION_ENERGY,
        true,
    );
    REACTING
}

fn healium_formation(air: &mut GasMixture, _: &mut Ctx) -> u8 {
    let bz_moles = air.get(G::BZ);
    let freon_moles = air.get(G::FREON);
    let temperature = air.temperature;
    let heat_efficiency = (temperature * 0.3)
        .min(freon_moles * (1.0 / 2.75))
        .min(bz_moles * (1.0 / 0.25));
    if heat_efficiency <= 0.0
        || freon_moles - heat_efficiency * 2.75 < 0.0
        || bz_moles - heat_efficiency * 0.25 < 0.0
    {
        return NO_REACTION;
    }
    let old_heat_capacity = air.heat_capacity();
    air.add(G::FREON, -heat_efficiency * 2.75);
    air.add(G::BZ, -heat_efficiency * 0.25);
    air.add(G::HEALIUM, heat_efficiency * 3.0);
    air.release(
        temperature,
        old_heat_capacity,
        heat_efficiency * HEALIUM_FORMATION_ENERGY,
        true,
    );
    REACTING
}

fn zauker_formation(air: &mut GasMixture, _: &mut Ctx) -> u8 {
    let hypernoblium_moles = air.get(G::HYPERNOBLIUM);
    let nitrium_moles = air.get(G::NITRIUM);
    let temperature = air.temperature;
    let heat_efficiency = (temperature * ZAUKER_FORMATION_TEMPERATURE_SCALE)
        .min(hypernoblium_moles * (1.0 / 0.01))
        .min(nitrium_moles * (1.0 / 0.5));
    if heat_efficiency <= 0.0
        || hypernoblium_moles - heat_efficiency * 0.01 < 0.0
        || nitrium_moles - heat_efficiency * 0.5 < 0.0
    {
        return NO_REACTION;
    }
    let old_heat_capacity = air.heat_capacity();
    air.add(G::HYPERNOBLIUM, -heat_efficiency * 0.01);
    air.add(G::NITRIUM, -heat_efficiency * 0.5);
    air.add(G::ZAUKER, heat_efficiency * 0.5);
    air.release(
        temperature,
        old_heat_capacity,
        -heat_efficiency * ZAUKER_FORMATION_ENERGY,
        true,
    );
    REACTING
}

fn zauker_decomp(air: &mut GasMixture, _: &mut Ctx) -> u8 {
    let nitrogen_moles = air.get(G::N2);
    let zauker_moles = air.get(G::ZAUKER);
    let burned_fuel = ZAUKER_DECOMPOSITION_MAX_RATE
        .min(nitrogen_moles)
        .min(zauker_moles);
    if burned_fuel <= 0.0 || zauker_moles - burned_fuel < 0.0 {
        return NO_REACTION;
    }
    let old_heat_capacity = air.heat_capacity();
    air.add(G::ZAUKER, -burned_fuel);
    air.add(G::O2, burned_fuel * 0.3);
    air.add(G::N2, burned_fuel * 0.7);
    let t = air.temperature;
    air.release(
        t,
        old_heat_capacity,
        ZAUKER_DECOMPOSITION_ENERGY * burned_fuel,
        true,
    );
    REACTING
}

fn proto_nitrate_formation(air: &mut GasMixture, _: &mut Ctx) -> u8 {
    let pluoxium_moles = air.get(G::PLUOXIUM);
    let hydrogen_moles = air.get(G::HYDROGEN);
    let temperature = air.temperature;
    let heat_efficiency = (temperature * 0.005)
        .min(pluoxium_moles * (1.0 / 0.2))
        .min(hydrogen_moles * (1.0 / 2.0));
    if heat_efficiency <= 0.0
        || pluoxium_moles - heat_efficiency * 0.2 < 0.0
        || hydrogen_moles - heat_efficiency * 2.0 < 0.0
    {
        return NO_REACTION;
    }
    let old_heat_capacity = air.heat_capacity();
    air.add(G::HYDROGEN, -heat_efficiency * 2.0);
    air.add(G::PLUOXIUM, -heat_efficiency * 0.2);
    air.add(G::PROTO_NITRATE, heat_efficiency * 2.2);
    air.release(
        temperature,
        old_heat_capacity,
        heat_efficiency * PN_FORMATION_ENERGY,
        true,
    );
    REACTING
}

fn proto_nitrate_hydrogen_response(air: &mut GasMixture, _: &mut Ctx) -> u8 {
    let proto_nitrate_moles = air.get(G::PROTO_NITRATE);
    let hydrogen_moles = air.get(G::HYDROGEN);
    let produced_amount = PN_HYDROGEN_CONVERSION_MAX_RATE
        .min(hydrogen_moles)
        .min(proto_nitrate_moles);
    if produced_amount <= 0.0 || hydrogen_moles - produced_amount < 0.0 {
        return NO_REACTION;
    }
    let old_heat_capacity = air.heat_capacity();
    air.add(G::HYDROGEN, -produced_amount);
    air.add(G::PROTO_NITRATE, produced_amount * 0.5);
    let t = air.temperature;
    air.release(
        t,
        old_heat_capacity,
        -produced_amount * PN_HYDROGEN_CONVERSION_ENERGY,
        true,
    );
    REACTING
}

/// `(air.volume / CELL_VOLUME) ** ATMOS_RADIATION_VOLUME_EXP`.
fn radiation_volume_scale(volume: f64) -> f64 {
    (volume / CELL_VOLUME).powi(ATMOS_RADIATION_VOLUME_EXP)
}

// shortcut: no radiation pulses here and in the BZ response; their volatile flag is kept.
fn proto_nitrate_tritium_response(air: &mut GasMixture, ctx: &mut Ctx) -> u8 {
    let proto_nitrate_moles = air.get(G::PROTO_NITRATE);
    let tritium_moles = air.get(G::TRITIUM);
    let temperature = air.temperature;
    let produced_amount = (temperature / 34.0 * (tritium_moles * proto_nitrate_moles)
        / (tritium_moles + 10.0 * proto_nitrate_moles))
        .min(tritium_moles)
        .min(proto_nitrate_moles * (1.0 / 0.01));
    if tritium_moles - produced_amount < 0.0 || proto_nitrate_moles - produced_amount * 0.01 < 0.0 {
        return NO_REACTION;
    }
    let old_heat_capacity = air.heat_capacity();
    air.add(G::PROTO_NITRATE, -produced_amount * 0.01);
    air.add(G::TRITIUM, -produced_amount);
    air.add(G::HYDROGEN, produced_amount);
    let energy_released = produced_amount * PN_TRITIUM_CONVERSION_ENERGY;
    let mut flags = NO_REACTION;
    if ctx.holder != Holder::Other
        && energy_released
            > PN_TRITIUM_CONVERSION_RAD_RELEASE_THRESHOLD * radiation_volume_scale(ctx.volume)
    {
        flags |= VOLATILE_REACTION;
    }
    if energy_released != 0.0 {
        air.release(temperature, old_heat_capacity, energy_released, true);
        flags |= REACTING;
    }
    flags
}

fn proto_nitrate_bz_response(air: &mut GasMixture, ctx: &mut Ctx) -> u8 {
    let proto_nitrate_moles = air.get(G::PROTO_NITRATE);
    let bz_moles = air.get(G::BZ);
    let temperature = air.temperature;
    let consumed_amount = (temperature / 2240.0 * bz_moles * proto_nitrate_moles
        / (bz_moles + proto_nitrate_moles))
        .min(bz_moles)
        .min(proto_nitrate_moles);
    if consumed_amount <= 0.0 || bz_moles - consumed_amount < 0.0 {
        return NO_REACTION;
    }
    let old_heat_capacity = air.heat_capacity();
    air.add(G::BZ, -consumed_amount);
    air.add(G::N2, consumed_amount * 0.4);
    air.add(G::HELIUM, consumed_amount * 1.6);
    air.add(G::PLASMA, consumed_amount * 0.8);
    let energy_released = consumed_amount * PN_BZASE_ENERGY;
    let mut flags = REACTING;
    if ctx.holder != Holder::Other
        && energy_released > PN_BZASE_RAD_RELEASE_THRESHOLD * radiation_volume_scale(ctx.volume)
    {
        flags |= VOLATILE_REACTION;
    }
    air.release(temperature, old_heat_capacity, energy_released, true);
    flags
}

fn antinoblium_replication(air: &mut GasMixture, _: &mut Ctx) -> u8 {
    let mut flags = REACTING | VOLATILE_REACTION;
    let heat_capacity = air.heat_capacity();
    let total_moles = air.total_moles();
    let antinoblium_moles = air.get(G::ANTINOBLIUM);
    let total_not_antinoblium_moles = total_moles - antinoblium_moles;
    let mut reaction_rate =
        (antinoblium_moles / ANTINOBLIUM_CONVERSION_DIVISOR).min(total_not_antinoblium_moles);
    if total_not_antinoblium_moles < MINIMUM_MOLE_COUNT {
        flags = NO_REACTION;
        reaction_rate = total_not_antinoblium_moles;
    }
    let anti = usize::from(G::ANTINOBLIUM.0);
    for (g, m) in air.moles.iter_mut().enumerate() {
        if g == anti || *m == 0.0 {
            continue;
        }
        if flags == NO_REACTION {
            *m = 0.0;
        } else {
            *m -= reaction_rate * *m / total_not_antinoblium_moles;
        }
    }
    air.add(G::ANTINOBLIUM, reaction_rate);
    let new_heat_capacity = air.heat_capacity();
    if new_heat_capacity > MINIMUM_HEAT_CAPACITY {
        air.temperature = (air.temperature * heat_capacity / new_heat_capacity).max(TCMB);
    }
    flags
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-3
    }

    fn mix(text: &str) -> GasMixture {
        GasMixture::parse(text).unwrap()
    }

    fn react(air: &mut GasMixture, holder: Holder) -> (u8, Exposures) {
        let mut exposures = Exposures::default();
        let flags = air.react(holder, CELL_VOLUME, &mut exposures);
        (flags, exposures)
    }

    #[test]
    fn keys_follow_tg_rarity() {
        // A reaction is keyed by its rarest gas (lowest `rarity`): spot checks.
        let key = |id| REACTIONS.iter().find(|r| r.id == id).unwrap().major;
        assert_eq!(key("plasmafire"), G::PLASMA);
        assert_eq!(key("nitrousformation"), G::BZ);
        assert_eq!(key("healium_formation"), G::FREON);
        assert_eq!(key("zauker_formation"), G::NITRIUM);
        assert_eq!(MAJOR_MASK & (1 << G::O2.0 | 1 << G::N2.0), 0);
    }

    #[test]
    fn station_air_does_not_react() {
        let mut air = mix("o2=22;n2=82;TEMP=293.15");
        let before = air.clone();
        assert_eq!(react(&mut air, Holder::Turf).0, NO_REACTION);
        assert_eq!(air, before);
        // Hot station air does not either: nothing burns without fuel.
        let mut air = mix("o2=22;n2=82;TEMP=2000");
        assert_eq!(react(&mut air, Holder::Turf).0, NO_REACTION);
    }

    #[test]
    fn plasma_fire_matches_dm() {
        // 10 plasma, 20 O2 at 600 K. temperature_scale = (600 - 373.15) / 1270 = 0.178622;
        // oxygen_burn_ratio = 1.4 - 0.178622 = 1.221378; O2/plasma = 2 < 10: rate =
        // (20/10)/9 * 0.178622 = 0.0396938. Capacities: before 10*200 + 20*20 = 2400.
        let mut air = mix("plasma=10;o2=20;TEMP=600");
        let (flags, exposures) = react(&mut air, Holder::Turf);
        assert_eq!(flags, REACTING | VOLATILE_REACTION);
        let rate = (20.0 / 10.0) / 9.0 * ((600.0 - 373.15) / 1270.0);
        let ratio = 1.4 - (600.0 - 373.15) / 1270.0;
        assert!(close(air.get(G::PLASMA), quantize(10.0 - rate)));
        assert!(close(air.get(G::O2), quantize(20.0 - rate * ratio)));
        assert!(close(air.get(G::CO2), rate * 0.75));
        assert!(close(air.get(G::WATER_VAPOR), rate * 0.25));
        // New capacity: 9.9603*200 + 19.9515*20 + 0.02977*30 + 0.00992*40 = 2392.38 J/K;
        // T = (600*2400 + 3e6*0.0396938) / 2392.38 = 651.686 K.
        let new_hc = air.heat_capacity();
        let t = (600.0 * 2400.0 + 3e6 * rate) / new_hc;
        assert!(close(air.temperature, t), "{} vs {t}", air.temperature);
        assert!(close(air.temperature, 651.686), "{}", air.temperature);
        assert!(close(
            air.reaction_results[PLASMA_FIRE],
            rate * (1.0 + ratio)
        ));
        assert_eq!(exposures.temperatures(), [air.temperature]);
        // Under the minimum burn temperature nothing happens.
        let mut cold = mix("plasma=10;o2=20;TEMP=373");
        assert_eq!(react(&mut cold, Holder::Turf).0, NO_REACTION);
    }

    #[test]
    fn plasma_fire_super_saturation_makes_tritium() {
        // O2/plasma = 100 >= 96 at 1700 K (> upper temperature: scale 1): rate = 1/9 of the
        // plasma, oxygen ratio 0.4, the plasma turns into tritium.
        let mut air = mix("plasma=1;o2=100;TEMP=1700");
        react(&mut air, Holder::Other);
        let rate = 1.0 / 9.0;
        assert!(close(air.get(G::TRITIUM), rate));
        assert!(close(air.get(G::O2), quantize(100.0 - rate * 0.4)));
        assert_eq!(air.get(G::CO2), 0.0);
    }

    #[test]
    fn hydrogen_fire_matches_dm() {
        // 10 H2, 10 O2 at 500 K: burned = min(10/2, 10/(2*10), 10, 10/0.5) = 0.5.
        // Capacity 10*15 + 10*20 = 350 -> 9.5*15 + 9.75*20 + 0.5*40 = 357.5;
        // T = (500*350 + 2.8e6*0.5) / 357.5 = 4405.594 K.
        let mut air = mix("hydrogen=10;o2=10;TEMP=500");
        let (flags, exposures) = react(&mut air, Holder::Turf);
        assert_eq!(flags, REACTING | VOLATILE_REACTION);
        assert!(close(air.get(G::HYDROGEN), 9.5) && close(air.get(G::O2), 9.75));
        assert!(close(air.get(G::WATER_VAPOR), 0.5));
        assert!(close(air.temperature, 4405.594), "{}", air.temperature);
        assert!(close(air.reaction_results[H2_FIRE], 0.5));
        assert_eq!(exposures.temperatures().len(), 1);
    }

    #[test]
    fn tritium_fire_matches_dm() {
        // Like hydrogen with tritium's 10 J/K/mol: 10 T + 10 O2 at 500 K, burned 0.5;
        // 300 J/K -> 9.5*10 + 9.75*20 + 0.5*40 = 310; T = (150000 + 1.4e6) / 310 = 5000 K.
        let mut air = mix("tritium=10;o2=10;TEMP=500");
        react(&mut air, Holder::Hotspot);
        assert!(close(air.get(G::TRITIUM), 9.5));
        assert!(close(air.temperature, 5000.0), "{}", air.temperature);
        assert!(close(air.reaction_results[TRIT_FIRE], 0.5));
    }

    #[test]
    fn freon_fire_cools() {
        // 10 freon, 100 O2 at 200 K: scale = (283 - 200) / 263 = 0.315589; ratio 1.084411;
        // O2 100 >= 10*10: rate = 10/4 * 0.315589 = 0.788973. Capacity 10*600 + 100*20 = 8000.
        let mut air = mix("freon=10;o2=100;TEMP=200");
        let (flags, exposures) = react(&mut air, Holder::Turf);
        assert_eq!(flags, REACTING | VOLATILE_REACTION);
        let scale = 83.0 / 263.0;
        let rate = 10.0 / 4.0 * scale;
        assert!(close(air.get(G::FREON), quantize(10.0 - rate)));
        assert!(close(air.get(G::CO2), rate));
        let t = (200.0 * 8000.0 - 3e5 * rate) / air.heat_capacity();
        assert!(close(air.temperature, t) && air.temperature < 200.0);
        assert_eq!(exposures.temperatures(), [air.temperature]);
    }

    #[test]
    fn steam_condenses_on_turfs_only() {
        // 1 mol vapor at 293.15 K: 0.25 condenses per reaction, nothing in a tank.
        let mut air = mix("water_vapor=1;n2=50;TEMP=293.15");
        let (flags, exposures) = react(&mut air, Holder::Turf);
        assert_eq!(flags, REACTING);
        assert_eq!(exposures.condensed, Some(Condensation::Wet));
        assert!(close(air.get(G::WATER_VAPOR), 0.75));
        let mut tank = mix("water_vapor=1;TEMP=293.15");
        assert_eq!(react(&mut tank, Holder::Other).0, NO_REACTION);
        // Too warm (> 303.15 K): no reaction; at 200 K it freezes (volatile).
        let mut warm = mix("water_vapor=1;TEMP=310");
        assert_eq!(react(&mut warm, Holder::Turf).0, NO_REACTION);
        let mut cold = mix("water_vapor=1;TEMP=200");
        let (flags, exposures) = react(&mut cold, Holder::Turf);
        assert_eq!(flags, REACTING | VOLATILE_REACTION);
        assert_eq!(exposures.condensed, Some(Condensation::Freeze));
    }

    #[test]
    fn n2o_decomposition_matches_dm() {
        // 10 N2O at 2000 K: burned = 10/2 * (2000 * (2000 - 1e5) / (-2.5e9)) = 5 * 0.0784
        // = 0.392. Capacity 400 -> 9.608*40 + 0.392*20 + 0.196*20 = 396.08;
        // T = (2000*400 + 2e5*0.392) / 396.08 = 2217.734 K.
        let mut air = mix("n2o=10;TEMP=2000");
        assert_eq!(react(&mut air, Holder::Turf).0, REACTING);
        assert!(close(air.get(G::N2O), 9.608));
        assert!(close(air.get(G::N2), 0.392) && close(air.get(G::O2), 0.196));
        assert!(close(air.temperature, 2217.734), "{}", air.temperature);
    }

    #[test]
    fn bz_formation_matches_dm() {
        // 20 N2O, 20 plasma at 293.15 K in a cell: pressure = 40*8.31*293.15/2500 = 38.9775;
        // efficiency = 2500/38.9775 = 64.1396; ratio 1; decomposition factor
        // max(4*(0.5 - 0.75), 0) = 0; bz = min(0.01*64.1396, 50, 25) = 0.641396.
        let mut air = mix("n2o=20;plasma=20;TEMP=293.15");
        assert_eq!(react(&mut air, Holder::Turf).0, REACTING);
        let pressure = 40.0 * 8.31 * 293.15 / 2500.0;
        let bz = 0.01 * 2500.0 / pressure;
        assert!(close(air.get(G::BZ), bz));
        assert!(close(air.get(G::N2O), 20.0 - 0.4 * bz));
        assert!(close(air.get(G::PLASMA), 20.0 - 0.8 * bz));
        // Capacity 20*40 + 20*200 = 4800 -> new; energy 80000 * bz.
        let t = (293.15 * 4800.0 + 80000.0 * bz) / air.heat_capacity();
        assert!(close(air.temperature, t), "{}", air.temperature);
        // Above 313.15 K nothing forms.
        let mut hot = mix("n2o=20;plasma=20;TEMP=320");
        assert_eq!(react(&mut hot, Holder::Turf).0, NO_REACTION);
    }

    #[test]
    fn freon_formation_matches_dm() {
        // 6 plasma, 3 CO2, 1 BZ at 800 K: factor min(10, 10, 10) = 10; heat factor
        // e^0 + 3/(1 + e^5.2) = 1 + 0.016436 = 1.016436; formed = min(0.508218, 10, ...).
        let mut air = mix("plasma=6;co2=3;bz=1;TEMP=800");
        let heat_factor = 1.0 + 3.0 / (1.0 + NUM_E.powf(5.2));
        let formed = heat_factor * 10.0 * 0.05;
        assert!(react(&mut air, Holder::Other).0 & REACTING != 0);
        assert!(close(air.get(G::FREON), formed), "{}", air.get(G::FREON));
        assert!(close(air.get(G::BZ), 1.0 - formed * 0.1));
    }

    #[test]
    fn hypernoblium_stops_reactions() {
        let mut air = mix("plasma=10;o2=20;hypernoblium=5;TEMP=600");
        let before = air.clone();
        assert_eq!(react(&mut air, Holder::Turf).0, STOP_REACTIONS);
        assert_eq!(air, before);
    }

    #[test]
    fn priority_runs_formation_before_fire() {
        // N2O + plasma below BZ's limit forms BZ (formation) and nothing burns (too cold);
        // a hot mix burns and decomposes N2O (post-formation) before the fire.
        let mut air = mix("n2o=10;plasma=10;o2=50;TEMP=2000");
        let (flags, _) = react(&mut air, Holder::Other);
        assert_eq!(flags, REACTING | VOLATILE_REACTION);
        assert!(air.get(G::BZ) == 0.0 && air.get(G::N2O) < 10.0 && air.get(G::CO2) > 0.0);
    }
}
