//! tg gas mixtures (`code/modules/atmospherics/gasmixtures/gas_mixture.dm`) and the constants of
//! `code/__DEFINES/atmospherics/atmos_core.dm`. Formulas are kept term for term so results can be
//! checked against the DM code by hand.

/// kPa*L/(K*mol).
pub const R_IDEAL_GAS_EQUATION: f64 = 8.31;
/// kPa.
pub const ONE_ATMOSPHERE: f64 = 101.325;
/// Cosmic microwave background: space and the floor of every temperature.
pub const TCMB: f64 = 2.7;
pub const T0C: f64 = 273.15;
pub const T20C: f64 = 293.15;
/// `O2STANDARD`, `N2STANDARD`: station air fractions.
pub const O2STANDARD: f64 = 0.21;
pub const N2STANDARD: f64 = 0.79;
/// Liters in a cell.
pub const CELL_VOLUME: f64 = 2500.0;
/// Moles in a standard cell (101.325 kPa, 20 °C), ~103.98.
pub const MOLES_CELLSTANDARD: f64 = ONE_ATMOSPHERE * CELL_VOLUME / (T20C * R_IDEAL_GAS_EQUATION);
pub const MINIMUM_HEAT_CAPACITY: f64 = 0.0003;
pub const MOLAR_ACCURACY: f64 = 1e-4;
pub const MINIMUM_AIR_RATIO_TO_SUSPEND: f64 = 0.1;
pub const MINIMUM_AIR_RATIO_TO_MOVE: f64 = 0.001;
/// ~10.4 mol.
pub const MINIMUM_AIR_TO_SUSPEND: f64 = MOLES_CELLSTANDARD * MINIMUM_AIR_RATIO_TO_SUSPEND;
/// ~0.104 mol.
pub const MINIMUM_MOLES_DELTA_TO_MOVE: f64 = MOLES_CELLSTANDARD * MINIMUM_AIR_RATIO_TO_MOVE;
pub const MINIMUM_TEMPERATURE_TO_MOVE: f64 = T20C + 100.0;
pub const MINIMUM_TEMPERATURE_DELTA_TO_SUSPEND: f64 = 4.0;
pub const MINIMUM_TEMPERATURE_DELTA_TO_CONSIDER: f64 = 0.5;
pub const OPEN_HEAT_TRANSFER_COEFFICIENT: f64 = 0.4;
/// "A hack to help make vacuums cold": the heat capacity of an empty turf mixture.
pub const HEAT_CAPACITY_VACUUM: f64 = 7000.0;
/// `MOLES_GAS_VISIBLE_STEP` and `TOTAL_VISIBLE_STATES` of the gas overlays.
pub const MOLES_GAS_VISIBLE_STEP: f64 = 0.25;
pub const TOTAL_VISIBLE_STATES: u8 = 80;
const MOLES_GAS_VISIBLE: f64 = 0.25;

/// One `/datum/gas` subtype.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GasInfo {
    /// `id`, as gas strings name it (`o2=22;n2=82`).
    pub id: &'static str,
    /// Last element of the DM path (`/datum/gas/oxygen`), also accepted in gas strings.
    pub path: &'static str,
    pub name: &'static str,
    /// J/(K*mol).
    pub specific_heat: f64,
    /// `gas_overlay` icon state in `icons/effects/atmospherics.dmi`.
    pub overlay: Option<&'static str>,
    /// Moles above which the overlay shows (`moles_visible`); unused without an overlay.
    pub moles_visible: f64,
}

const fn gas(
    id: &'static str,
    path: &'static str,
    name: &'static str,
    specific_heat: f64,
    overlay: Option<&'static str>,
    moles_visible: f64,
) -> GasInfo {
    GasInfo {
        id,
        path,
        name,
        specific_heat,
        overlay,
        moles_visible,
    }
}

/// `GAS_TYPE_COUNT`.
pub const GAS_COUNT: usize = 20;

/// Every tg gas, in `gas_types.dm` order; [`GasId`] indexes it.
// shortcut: a table in code instead of reading `/datum/gas` from dm_types.json; tg changes gases
// rarely and the simulation must not depend on generated assets. Read it from the dump when a
// fork adds gases.
pub static GASES: [GasInfo; GAS_COUNT] = [
    gas("o2", "oxygen", "Oxygen", 20.0, None, 0.0),
    gas("n2", "nitrogen", "Nitrogen", 20.0, None, 0.0),
    gas("co2", "carbon_dioxide", "Carbon Dioxide", 30.0, None, 0.0),
    gas(
        "plasma",
        "plasma",
        "Plasma",
        200.0,
        Some("plasma"),
        MOLES_GAS_VISIBLE,
    ),
    gas(
        "water_vapor",
        "water_vapor",
        "Water Vapor",
        40.0,
        Some("water_vapor"),
        MOLES_GAS_VISIBLE,
    ),
    gas(
        "hypernoblium",
        "hypernoblium",
        "Hyper-Noblium",
        2000.0,
        Some("freon"),
        MOLES_GAS_VISIBLE,
    ),
    gas(
        "n2o",
        "nitrous_oxide",
        "Nitrous Oxide",
        40.0,
        Some("nitrous_oxide"),
        MOLES_GAS_VISIBLE * 2.0,
    ),
    gas(
        "nitrium",
        "nitrium",
        "Nitrium",
        10.0,
        Some("nitrium"),
        MOLES_GAS_VISIBLE,
    ),
    gas(
        "tritium",
        "tritium",
        "Tritium",
        10.0,
        Some("tritium"),
        MOLES_GAS_VISIBLE,
    ),
    gas("bz", "bz", "BZ", 20.0, None, 0.0),
    gas("pluoxium", "pluoxium", "Pluoxium", 80.0, None, 0.0),
    gas(
        "miasma",
        "miasma",
        "Miasma",
        20.0,
        Some("miasma"),
        MOLES_GAS_VISIBLE * 60.0,
    ),
    gas(
        "freon",
        "freon",
        "Freon",
        600.0,
        Some("freon"),
        MOLES_GAS_VISIBLE * 30.0,
    ),
    gas("hydrogen", "hydrogen", "Hydrogen", 15.0, None, 0.0),
    gas(
        "healium",
        "healium",
        "Healium",
        10.0,
        Some("healium"),
        MOLES_GAS_VISIBLE,
    ),
    gas(
        "proto_nitrate",
        "proto_nitrate",
        "Proto-Nitrate",
        30.0,
        Some("proto_nitrate"),
        MOLES_GAS_VISIBLE,
    ),
    gas(
        "zauker",
        "zauker",
        "Zauker",
        350.0,
        Some("zauker"),
        MOLES_GAS_VISIBLE,
    ),
    gas(
        "halon",
        "halon",
        "Halon",
        175.0,
        Some("halon"),
        MOLES_GAS_VISIBLE,
    ),
    gas("helium", "helium", "Helium", 15.0, None, 0.0),
    gas(
        "antinoblium",
        "antinoblium",
        "Anti-Noblium",
        1.0,
        Some("antinoblium"),
        MOLES_GAS_VISIBLE,
    ),
];

/// Index into [`GASES`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GasId(pub u8);

impl GasId {
    pub const O2: Self = Self(0);
    pub const N2: Self = Self(1);
    pub const CO2: Self = Self(2);
    pub const PLASMA: Self = Self(3);
    pub const WATER_VAPOR: Self = Self(4);
    pub const HYPERNOBLIUM: Self = Self(5);
    pub const N2O: Self = Self(6);
    pub const NITRIUM: Self = Self(7);
    pub const TRITIUM: Self = Self(8);
    pub const BZ: Self = Self(9);
    pub const PLUOXIUM: Self = Self(10);
    pub const MIASMA: Self = Self(11);
    pub const FREON: Self = Self(12);
    pub const HYDROGEN: Self = Self(13);
    pub const HEALIUM: Self = Self(14);
    pub const PROTO_NITRATE: Self = Self(15);
    pub const ZAUKER: Self = Self(16);
    pub const HALON: Self = Self(17);
    pub const HELIUM: Self = Self(18);
    pub const ANTINOBLIUM: Self = Self(19);

    /// By gas string key: the tg id (`o2`) or the DM path, with or without `/datum/gas/`.
    pub fn parse(key: &str) -> Option<Self> {
        let key = key.strip_prefix("/datum/gas/").unwrap_or(key);
        GASES
            .iter()
            .position(|g| g.id == key || g.path == key)
            .map(|i| Self(i as u8))
    }

    pub fn info(self) -> &'static GasInfo {
        &GASES[usize::from(self.0)]
    }
}

/// BYOND `round(x, step)`: the nearest multiple, halves up.
pub fn byond_round(x: f64, step: f64) -> f64 {
    (x / step + 0.5).floor() * step
}

/// `QUANTIZE`.
pub fn quantize(x: f64) -> f64 {
    byond_round(x, MOLAR_ACCURACY)
}

/// `CALCULATE_CONDUCTION_ENERGY`: energy gained by mixture one.
pub fn conduction_energy(
    temperature_delta: f64,
    heat_capacity_one: f64,
    heat_capacity_two: f64,
) -> f64 {
    temperature_delta
        * (heat_capacity_one * (heat_capacity_two / (heat_capacity_one + heat_capacity_two)))
}

/// A tile's gas: tg `/datum/gas_mixture/turf` at `CELL_VOLUME`, with the archived copy LINDA
/// computes shares from. Dense over [`GASES`]: a zero is an absent gas (tg removes gases under
/// [`MOLAR_ACCURACY`] in `garbage_collect`, which [`Self::garbage_collect`] mirrors by zeroing).
#[derive(Clone, Debug, PartialEq)]
pub struct GasMixture {
    pub moles: [f64; GAS_COUNT],
    /// Kelvin.
    pub temperature: f64,
    pub(crate) moles_archive: [f64; GAS_COUNT],
    pub(crate) temperature_archived: f64,
    /// Moles moved by the last [`Self::share`] (absolute sum).
    pub(crate) last_share: f64,
    /// tg `reaction_results` of the last [`Self::react`] that had candidate reactions, for the
    /// reactions that grow hotspots (`expands_hotspot`), see [`crate::reactions::FIRE_RESULTS`].
    pub reaction_results: [f64; crate::reactions::FIRE_RESULTS],
}

impl Default for GasMixture {
    fn default() -> Self {
        Self::vacuum()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseError {
    UnknownGas(String),
    BadNumber(String),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownGas(g) => write!(f, "unknown gas {g:?}"),
            Self::BadNumber(n) => write!(f, "not a number: {n:?}"),
        }
    }
}

impl std::error::Error for ParseError {}

impl GasMixture {
    /// Empty at [`TCMB`] (a fresh `/datum/gas_mixture`, and space).
    pub const fn vacuum() -> Self {
        Self {
            moles: [0.0; GAS_COUNT],
            temperature: TCMB,
            moles_archive: [0.0; GAS_COUNT],
            temperature_archived: TCMB,
            last_share: 0.0,
            reaction_results: [0.0; crate::reactions::FIRE_RESULTS],
        }
    }

    /// `SSair.parse_gas_string`: `"o2=22;n2=82;TEMP=293.15"`; no `TEMP` means [`T20C`].
    pub fn parse(text: &str) -> Result<Self, ParseError> {
        let mut mix = Self::vacuum();
        mix.temperature = T20C;
        for pair in text.split([';', '&']).filter(|p| !p.trim().is_empty()) {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            let (key, value) = (key.trim(), value.trim());
            let num: f64 = value
                .parse()
                .map_err(|_| ParseError::BadNumber(value.to_owned()))?;
            if key == "TEMP" {
                mix.temperature = num;
            } else {
                let gas =
                    GasId::parse(key).ok_or_else(|| ParseError::UnknownGas(key.to_owned()))?;
                mix.moles[usize::from(gas.0)] = num;
            }
        }
        mix.temperature_archived = mix.temperature;
        Ok(mix)
    }

    pub fn get(&self, gas: GasId) -> f64 {
        self.moles[usize::from(gas.0)]
    }

    pub fn set(&mut self, gas: GasId, moles: f64) {
        self.moles[usize::from(gas.0)] = moles;
    }

    pub fn add(&mut self, gas: GasId, moles: f64) {
        self.moles[usize::from(gas.0)] += moles;
    }

    /// `remove_ratio`: that fraction (at most all) of every gas; an empty mixture at 0.
    pub fn remove_ratio(&mut self, ratio: f64) -> Self {
        let mut removed = Self::vacuum();
        if ratio <= 0.0 {
            return removed;
        }
        let ratio = ratio.min(1.0);
        removed.temperature = self.temperature;
        for (m, r) in self.moles.iter_mut().zip(removed.moles.iter_mut()) {
            *r = quantize(*m * ratio);
            *m -= *r;
        }
        self.garbage_collect();
        removed
    }

    pub fn total_moles(&self) -> f64 {
        self.moles.iter().sum()
    }

    /// kPa.
    pub fn pressure(&self) -> f64 {
        self.total_moles() * R_IDEAL_GAS_EQUATION * self.temperature / CELL_VOLUME
    }

    /// kPa of one gas.
    pub fn partial_pressure(&self, gas: GasId) -> f64 {
        self.get(gas) * R_IDEAL_GAS_EQUATION * self.temperature / CELL_VOLUME
    }

    /// J/K; an empty turf mixture counts as [`HEAT_CAPACITY_VACUUM`].
    pub fn heat_capacity(&self) -> f64 {
        nonzero_or_vacuum(dot(&self.moles))
    }

    pub(crate) fn heat_capacity_archive(&self) -> f64 {
        nonzero_or_vacuum(dot(&self.moles_archive))
    }

    /// J/K of a mixture outside turfs (tg base `heat_capacity()`): 0 when empty.
    pub fn heat_capacity_raw(&self) -> f64 {
        dot(&self.moles)
    }

    /// Joules.
    pub fn thermal_energy(&self) -> f64 {
        self.temperature * self.heat_capacity()
    }

    pub(crate) fn archive(&mut self) {
        self.moles_archive = self.moles;
        self.temperature_archived = self.temperature;
    }

    /// Zeroes gases at or under [`MOLAR_ACCURACY`] (`values_cut_under(.., inclusive)`).
    pub fn garbage_collect(&mut self) {
        for m in self.moles.iter_mut().chain(self.moles_archive.iter_mut()) {
            if *m <= MOLAR_ACCURACY {
                *m = 0.0;
            }
        }
    }

    /// `copy_from`: the sample's gas and temperature; the archive restarts at zero moles.
    pub(crate) fn copy_from(&mut self, sample: &Self) {
        self.moles = sample.moles;
        self.temperature = sample.temperature;
        self.moles_archive = [0.0; GAS_COUNT];
    }

    /// `merge`: all of `giver` into this mixture, heat included.
    pub fn merge(&mut self, giver: &Self) {
        if (self.temperature - giver.temperature).abs() > MINIMUM_TEMPERATURE_DELTA_TO_CONSIDER {
            let self_heat_capacity = self.heat_capacity();
            let giver_heat_capacity = giver.heat_capacity();
            let combined = giver_heat_capacity + self_heat_capacity;
            if combined != 0.0 {
                self.temperature = (giver.temperature * giver_heat_capacity
                    + self.temperature * self_heat_capacity)
                    / combined;
            }
        }
        for (m, g) in self.moles.iter_mut().zip(giver.moles) {
            *m += g;
        }
    }

    /// `merge` of a mixture outside turfs: an empty mixture takes the giver's temperature
    /// (base `heat_capacity()` has no vacuum floor).
    pub fn merge_raw(&mut self, giver: &Self) {
        if (self.temperature - giver.temperature).abs() > MINIMUM_TEMPERATURE_DELTA_TO_CONSIDER {
            let self_heat_capacity = self.heat_capacity_raw();
            let giver_heat_capacity = giver.heat_capacity_raw();
            let combined = giver_heat_capacity + self_heat_capacity;
            if combined != 0.0 {
                self.temperature = (giver.temperature * giver_heat_capacity
                    + self.temperature * self_heat_capacity)
                    / combined;
            }
        }
        for (m, g) in self.moles.iter_mut().zip(giver.moles) {
            *m += g;
        }
    }

    /// `remove`: takes `amount` moles in proportion; `None` when there is nothing to take.
    pub fn remove(&mut self, amount: f64) -> Option<Self> {
        let total = self.total_moles();
        let amount = amount.min(total);
        if amount <= 0.0 {
            return None;
        }
        let ratio = amount / total;
        let mut removed = Self::vacuum();
        removed.temperature = self.temperature;
        for (m, r) in self.moles.iter_mut().zip(removed.moles.iter_mut()) {
            *r = quantize(*m * ratio);
            *m -= *r;
        }
        self.garbage_collect();
        Some(removed)
    }

    /// `share`: moves gas from the archived difference, scaled by the giving side's
    /// coefficient (1 / (adjacent turfs + 1)), and the heat it carries. Commutative by design.
    /// Returns the pressure difference that moves objects (positive when `sharer` received),
    /// or `None` when too little moved.
    pub(crate) fn share(
        &mut self,
        sharer: &mut Self,
        our_coeff: f64,
        sharer_coeff: f64,
    ) -> Option<f64> {
        let only_in_one = self
            .moles
            .iter()
            .zip(sharer.moles)
            .any(|(&a, b)| (a != 0.0) != (b != 0.0));
        let temperature_delta = self.temperature_archived - sharer.temperature_archived;
        let temp_delta_threshold = temperature_delta.abs() > MINIMUM_TEMPERATURE_DELTA_TO_CONSIDER;
        let (old_self_heat_capacity, old_sharer_heat_capacity) = if temp_delta_threshold {
            (self.heat_capacity(), sharer.heat_capacity())
        } else {
            (0.0, 0.0)
        };
        let mut heat_capacity_self_to_sharer = 0.0;
        let mut heat_capacity_sharer_to_self = 0.0;
        let mut moved_moles = 0.0;
        let mut abs_moved_moles = 0.0;
        for (g, info) in GASES.iter().enumerate() {
            let mut delta = quantize(self.moles_archive[g] - sharer.moles_archive[g]);
            if delta == 0.0 {
                continue;
            }
            delta *= if delta > 0.0 { our_coeff } else { sharer_coeff };
            if temp_delta_threshold {
                let gas_heat_capacity = delta * info.specific_heat;
                if delta > 0.0 {
                    heat_capacity_self_to_sharer += gas_heat_capacity;
                } else {
                    heat_capacity_sharer_to_self -= gas_heat_capacity;
                }
            }
            self.moles[g] -= delta;
            sharer.moles[g] += delta;
            moved_moles += delta;
            abs_moved_moles += delta.abs();
        }
        self.last_share = abs_moved_moles;

        if temp_delta_threshold {
            let new_self_heat_capacity = old_self_heat_capacity + heat_capacity_sharer_to_self
                - heat_capacity_self_to_sharer;
            let new_sharer_heat_capacity = old_sharer_heat_capacity + heat_capacity_self_to_sharer
                - heat_capacity_sharer_to_self;
            if new_self_heat_capacity > MINIMUM_HEAT_CAPACITY {
                self.temperature = (old_self_heat_capacity * self.temperature
                    - heat_capacity_self_to_sharer * self.temperature_archived
                    + heat_capacity_sharer_to_self * sharer.temperature_archived)
                    / new_self_heat_capacity;
            }
            if new_sharer_heat_capacity > MINIMUM_HEAT_CAPACITY {
                sharer.temperature = (old_sharer_heat_capacity * sharer.temperature
                    - heat_capacity_sharer_to_self * sharer.temperature_archived
                    + heat_capacity_self_to_sharer * self.temperature_archived)
                    / new_sharer_heat_capacity;
                if old_sharer_heat_capacity.abs() > MINIMUM_HEAT_CAPACITY
                    && (new_sharer_heat_capacity / old_sharer_heat_capacity - 1.0).abs() < 0.1
                {
                    self.temperature_share(sharer, OPEN_HEAT_TRANSFER_COEFFICIENT, None);
                }
            }
        }

        if only_in_one {
            self.garbage_collect();
            sharer.garbage_collect();
        }

        if temperature_delta > MINIMUM_TEMPERATURE_TO_MOVE
            || moved_moles.abs() > MINIMUM_MOLES_DELTA_TO_MOVE
        {
            let our_moles = self.total_moles();
            let their_moles = sharer.total_moles();
            return Some(
                (self.temperature_archived * (our_moles + moved_moles)
                    - sharer.temperature_archived * (their_moles - moved_moles))
                    * R_IDEAL_GAS_EQUATION
                    / CELL_VOLUME,
            );
        }
        None
    }

    /// `temperature_share` with a sharer mixture: conduction from the archived temperatures.
    /// `sharer_heat_capacity` replaces the sharer's (planetary atmospheres pass 5x theirs).
    pub(crate) fn temperature_share(
        &mut self,
        sharer: &mut Self,
        conduction_coefficient: f64,
        sharer_heat_capacity: Option<f64>,
    ) {
        let sharer_temperature = sharer.temperature_archived;
        let temperature_delta = self.temperature_archived - sharer_temperature;
        if temperature_delta.abs() <= MINIMUM_TEMPERATURE_DELTA_TO_CONSIDER {
            return;
        }
        let self_heat_capacity = self.heat_capacity_archive();
        let sharer_heat_capacity = sharer_heat_capacity
            .filter(|&c| c != 0.0)
            .unwrap_or_else(|| sharer.heat_capacity_archive());
        if sharer_heat_capacity > MINIMUM_HEAT_CAPACITY
            && self_heat_capacity > MINIMUM_HEAT_CAPACITY
        {
            let heat = conduction_energy(
                conduction_coefficient * temperature_delta,
                sharer_heat_capacity,
                self_heat_capacity,
            );
            self.temperature = (self.temperature - heat / self_heat_capacity).max(TCMB);
            sharer.temperature = (sharer_temperature + heat / sharer_heat_capacity).max(TCMB);
        }
    }

    /// `compare`: true when the two differ enough for gas to move (tg returns the failed check's
    /// name, "" when equal). The total is compared on current moles even with `cmp_archive`,
    /// as in tg.
    pub(crate) fn compare(&self, sample: &Self, cmp_archive: bool) -> bool {
        let (ours, theirs) = if cmp_archive {
            (&self.moles_archive, &sample.moles_archive)
        } else {
            (&self.moles, &sample.moles)
        };
        if (self.total_moles() - sample.total_moles()).abs() > MINIMUM_MOLES_DELTA_TO_MOVE {
            return true;
        }
        for (&gas_moles, &sample_moles) in ours.iter().zip(theirs) {
            let abs_delta = (gas_moles - sample_moles).abs();
            if abs_delta > MINIMUM_MOLES_DELTA_TO_MOVE
                && abs_delta > gas_moles * MINIMUM_AIR_RATIO_TO_MOVE
            {
                return true;
            }
        }
        if ours.iter().sum::<f64>() > MINIMUM_MOLES_DELTA_TO_MOVE {
            let (a, b) = if cmp_archive {
                (self.temperature_archived, sample.temperature_archived)
            } else {
                (self.temperature, sample.temperature)
            };
            if (a - b).abs() > MINIMUM_TEMPERATURE_DELTA_TO_SUSPEND {
                return true;
            }
        }
        false
    }

    /// `GAS_OVERLAYS`: per gas, the overlay step `1..=80` (alpha index), 0 when not drawn.
    pub fn overlay_steps(&self) -> [u8; GAS_COUNT] {
        let mut steps = [0; GAS_COUNT];
        for ((step, &amount), info) in steps.iter_mut().zip(&self.moles).zip(&GASES) {
            if info.overlay.is_some() && amount > info.moles_visible {
                let j = (amount / MOLES_GAS_VISIBLE_STEP).ceil();
                *step = j.min(f64::from(TOTAL_VISIBLE_STATES)) as u8;
            }
        }
        steps
    }
}

/// `generate_gas_overlays`: the alpha of overlay step `j` (1..=80).
pub fn overlay_alpha(j: u8) -> u8 {
    let total = f64::from(TOTAL_VISIBLE_STATES);
    let alpha = ((f64::from(j) + 0.4 * total) / (0.35 * total)).log(4.0) * 255.0;
    alpha.round().clamp(0.0, 255.0) as u8
}

fn dot(moles: &[f64; GAS_COUNT]) -> f64 {
    moles
        .iter()
        .zip(&GASES)
        .map(|(m, g)| m * g.specific_heat)
        .sum()
}

fn nonzero_or_vacuum(heat_capacity: f64) -> f64 {
    if heat_capacity == 0.0 {
        HEAT_CAPACITY_VACUUM
    } else {
        heat_capacity
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-3
    }

    fn archived(text: &str) -> GasMixture {
        let mut mix = GasMixture::parse(text).unwrap();
        mix.archive();
        mix
    }

    #[test]
    fn constants_and_parse() {
        // 101.325 * 2500 / (293.15 * 8.31) = 253312.5 / 2436.0765
        assert!(close(MOLES_CELLSTANDARD, 103.9838));
        assert!(close(MINIMUM_MOLES_DELTA_TO_MOVE, 0.103_984));
        let air = GasMixture::parse("o2=22;n2=82;TEMP=293.15").unwrap();
        assert_eq!((air.get(GasId::O2), air.get(GasId::N2)), (22.0, 82.0));
        // 104 * 8.31 * 293.15 / 2500
        assert!(close(air.pressure(), 101.340_782));
        assert_eq!(air.heat_capacity(), 2080.0);
        assert_eq!(GasMixture::parse("plasma=5").unwrap().temperature, T20C);
        assert_eq!(
            GasMixture::parse("/datum/gas/plasma=5")
                .unwrap()
                .get(GasId::PLASMA),
            5.0
        );
        assert_eq!(
            GasMixture::parse("TEMP=2.7").unwrap().heat_capacity(),
            HEAT_CAPACITY_VACUUM
        );
        assert!(GasMixture::parse("bogus=1").is_err());
        assert!((quantize(0.123_45) - 0.1235).abs() < 1e-12);
        assert_eq!(quantize(-0.000_04), 0.0);
    }

    /// Standard air shares with an airless floor (both with 4 neighbours, coefficient 1/5).
    #[test]
    fn share_into_vacuum_floor_matches_dm() {
        let mut room = archived("o2=22;n2=82;TEMP=293.15");
        let mut floor = archived("TEMP=2.7");
        let diff = room.share(&mut floor, 0.2, 0.2).unwrap();
        // Gas: 22 * 0.2 = 4.4 O2 and 82 * 0.2 = 16.4 N2 move.
        assert!(close(room.get(GasId::O2), 17.6) && close(room.get(GasId::N2), 65.6));
        assert!(close(floor.get(GasId::O2), 4.4) && close(floor.get(GasId::N2), 16.4));
        assert!(close(room.last_share, 20.8));
        // Heat: old capacities 2080 and 7000 (vacuum), 416 J/K carried over.
        // Room: (2080*293.15 - 416*293.15) / 1664 = 293.15. Floor: (7000*2.7 + 416*293.15)
        // / 7416 = 18.99; 7416/7000 - 1 < 0.1, so `temperature_share` at 0.4 follows, from the
        // archives: heat = 0.4*290.45 * (7000 * 2080 / 9080) = 186297.445 J;
        // room 293.15 - 186297.445/2080 = 203.584, floor 2.7 + 186297.445/7000 = 29.314.
        assert!(close(room.temperature, 203.583_92), "{}", room.temperature);
        assert!(close(floor.temperature, 29.313_92), "{}", floor.temperature);
        // Returned: (293.15 * (83.2 + 20.8) - 2.7 * (20.8 - 20.8)) * 8.31 / 2500.
        assert!(close(diff, 101.340_782), "{diff}");
    }

    #[test]
    fn share_heat_only() {
        // Same gas, 300 K vs 200 K: nothing moves, capacities stay 2080, `temperature_share`:
        // heat = 0.4*100 * (2080*2080/4160) = 41600 J, 41600/2080 = 20 K each way.
        let mut hot = archived("o2=22;n2=82;TEMP=300");
        let mut cold = archived("o2=22;n2=82;TEMP=200");
        assert_eq!(
            hot.share(&mut cold, 0.2, 0.2),
            None,
            "no gas moved, delta T < 393.15"
        );
        assert!(close(hot.temperature, 280.0) && close(cold.temperature, 220.0));
        assert_eq!(hot.last_share, 0.0);
        // A planetary-style override of the sharer's capacity (5x):
        // heat = 0.4*100 * (10400*2080/12480) = 40 * 1733.33 = 69333.3 J.
        let mut hot = archived("o2=22;n2=82;TEMP=300");
        let mut cold = archived("o2=22;n2=82;TEMP=200");
        hot.temperature_share(&mut cold, 0.4, Some(10400.0));
        assert!(close(hot.temperature, 300.0 - 69_333.333 / 2080.0));
        assert!(close(cold.temperature, 200.0 + 69_333.333 / 10400.0));
    }

    #[test]
    fn share_is_commutative_and_conserves() {
        let mut a = archived("o2=30;plasma=7.5;TEMP=350");
        let mut b = archived("n2=40;co2=3;TEMP=250");
        let moles = a.total_moles() + b.total_moles();
        let mut a2 = a.clone();
        let mut b2 = b.clone();
        a.share(&mut b, 0.25, 0.5);
        b2.share(&mut a2, 0.5, 0.25);
        for g in 0..GAS_COUNT {
            assert!(close(a.moles[g], a2.moles[g]) && close(b.moles[g], b2.moles[g]));
        }
        // Temperatures are not: tg conducts from the caller's side only.
        assert!(close(a.total_moles() + b.total_moles(), moles));
    }

    #[test]
    fn compare_thresholds() {
        let base = archived("o2=22;n2=82;TEMP=293.15");
        let with = |text: &str| archived(text);
        // 0.1 mol more in total: under MINIMUM_MOLES_DELTA_TO_MOVE (0.10398).
        assert!(!base.compare(&with("o2=22.1;n2=82;TEMP=293.15"), true));
        assert!(base.compare(&with("o2=22.11;n2=82;TEMP=293.15"), true));
        // Same total, 0.2 mol swapped between gases: per-gas delta 0.2 > 0.104 and > 0.1%.
        assert!(base.compare(&with("o2=21.8;n2=82.2;TEMP=293.15"), true));
        // Temperature: 3.85 K apart suspends, 4.05 K does not.
        assert!(!base.compare(&with("o2=22;n2=82;TEMP=297"), true));
        assert!(base.compare(&with("o2=22;n2=82;TEMP=297.2"), true));
        // Vacuum ignores temperature.
        assert!(!with("TEMP=2.7").compare(&with("TEMP=500"), true));
    }

    #[test]
    fn overlays() {
        let mut mix = GasMixture::vacuum();
        mix.moles[usize::from(GasId::PLASMA.0)] = 0.25;
        assert_eq!(mix.overlay_steps()[3], 0, "visible only above 0.25");
        mix.moles[usize::from(GasId::PLASMA.0)] = 0.3;
        mix.moles[usize::from(GasId::N2O.0)] = 0.5;
        mix.moles[usize::from(GasId::O2.0)] = 100.0;
        let steps = mix.overlay_steps();
        assert_eq!((steps[3], steps[6], steps[0]), (2, 0, 0));
        mix.moles[usize::from(GasId::PLASMA.0)] = 1000.0;
        assert_eq!(mix.overlay_steps()[3], 80);
        // log4((1 + 32) / 28) * 255 = 30.2; log4(112 / 28) * 255 = 255.
        assert_eq!((overlay_alpha(1), overlay_alpha(80)), (30, 255));
    }
}
