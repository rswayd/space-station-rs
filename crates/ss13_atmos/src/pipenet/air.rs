//! Gas mixtures outside turfs and tg's pressure-driven transfers
//! (`code/modules/atmospherics/gasmixtures/gas_mixture.dm`: `gas_pressure_calculate`,
//! `pump_gas_to`, `release_gas_to`, `equalize`).
//!
//! A turf's mixture differs from every other one in two ways tg encodes in
//! `/datum/gas_mixture/turf`: an empty one has [`HEAT_CAPACITY_VACUUM`](crate::gas::HEAT_CAPACITY_VACUUM)
//! and its volume is `CELL_VOLUME`. [`Mix`] lets the transfer math take either side as tg does.

use crate::gas::{
    CELL_VOLUME, GasMixture, MINIMUM_TEMPERATURE_DELTA_TO_SUSPEND, MOLAR_ACCURACY,
    R_IDEAL_GAS_EQUATION, TCMB,
};

/// `ATMOS_PRESSURE_ERROR_TOLERANCE`, `ATMOS_PRESSURE_APPROXIMATION_ITERATIONS`.
const PRESSURE_ERROR_TOLERANCE: f64 = 0.01;
const APPROXIMATION_ITERATIONS: u32 = 20;

/// A `/datum/gas_mixture` of `volume` litres in a pipe, a machine port or a portable machine.
#[derive(Clone, Debug, PartialEq)]
pub struct PipeAir {
    pub gas: GasMixture,
    pub volume: f64,
}

impl PipeAir {
    /// `new /datum/gas_mixture(volume)`: empty at [`TCMB`].
    pub fn new(volume: f64) -> Self {
        Self {
            gas: GasMixture::vacuum(),
            volume,
        }
    }

    pub fn with_gas(gas: GasMixture, volume: f64) -> Self {
        Self { gas, volume }
    }

    pub fn total_moles(&self) -> f64 {
        self.gas.total_moles()
    }

    pub fn temperature(&self) -> f64 {
        self.gas.temperature
    }

    /// tg base `heat_capacity()`.
    pub fn heat_capacity(&self) -> f64 {
        self.gas.heat_capacity_raw()
    }

    /// kPa; 0 without volume (`return_pressure`).
    pub fn pressure(&self) -> f64 {
        if self.volume > 0.0 {
            self.total_moles() * R_IDEAL_GAS_EQUATION * self.gas.temperature / self.volume
        } else {
            0.0
        }
    }

    pub fn merge(&mut self, giver: &GasMixture) {
        self.gas.merge_raw(giver);
    }

    pub fn remove(&mut self, amount: f64) -> Option<GasMixture> {
        self.gas.remove(amount)
    }

    pub fn remove_ratio(&mut self, ratio: f64) -> GasMixture {
        self.gas.remove_ratio(ratio)
    }

    /// `copy_from_ratio`: the sample's gas times `partial`, at its temperature.
    pub fn copy_from_ratio(&mut self, sample: &GasMixture, partial: f64) {
        self.gas.temperature = sample.temperature;
        for (m, s) in self.gas.moles.iter_mut().zip(sample.moles) {
            *m = s * partial;
        }
    }

    /// `remove_specific_ratio(gas, 1)` for every gas in `mask` (bit = [`crate::GasId`] index):
    /// all of those gases, at this temperature.
    pub fn remove_gases(&mut self, mask: u32) -> GasMixture {
        let mut out = GasMixture::vacuum();
        out.temperature = self.gas.temperature;
        for (g, m) in self.gas.moles.iter_mut().enumerate() {
            if mask & (1 << g) != 0 && *m != 0.0 {
                out.moles[g] = *m;
                *m = 0.0;
            }
        }
        self.gas.garbage_collect();
        out
    }
}

/// One side of a transfer: its gas, litres and tg's heat capacity for its kind.
pub trait Mix {
    fn gas(&self) -> &GasMixture;
    fn volume(&self) -> f64;
    fn heat_capacity(&self) -> f64;

    fn total_moles(&self) -> f64 {
        self.gas().total_moles()
    }

    fn temperature(&self) -> f64 {
        self.gas().temperature
    }

    fn pressure(&self) -> f64 {
        let volume = self.volume();
        if volume > 0.0 {
            self.total_moles() * R_IDEAL_GAS_EQUATION * self.temperature() / volume
        } else {
            0.0
        }
    }

    fn thermal_energy(&self) -> f64 {
        self.temperature() * self.heat_capacity()
    }
}

impl Mix for PipeAir {
    fn gas(&self) -> &GasMixture {
        &self.gas
    }

    fn volume(&self) -> f64 {
        self.volume
    }

    fn heat_capacity(&self) -> f64 {
        self.gas.heat_capacity_raw()
    }
}

/// A turf's air seen as a transfer side (`/datum/gas_mixture/turf`, `CELL_VOLUME`).
pub struct TurfMix<'a>(pub &'a GasMixture);

impl Mix for TurfMix<'_> {
    fn gas(&self) -> &GasMixture {
        self.0
    }

    fn volume(&self) -> f64 {
        CELL_VOLUME
    }

    fn heat_capacity(&self) -> f64 {
        self.0.heat_capacity()
    }
}

/// `gas_pressure_minimum_transfer`: the output's pressure after taking `MOLAR_ACCURACY` moles.
fn minimum_transfer_pressure(input: &dyn Mix, output: &dyn Mix) -> f64 {
    let share = MOLAR_ACCURACY / input.total_moles();
    let energy = output.thermal_energy() + share * input.thermal_energy();
    let capacity = output.heat_capacity() + share * input.heat_capacity();
    (output.total_moles() + MOLAR_ACCURACY) * R_IDEAL_GAS_EQUATION * (energy / capacity)
        / output.volume()
}

/// `gas_pressure_calculate`: moles of `input` that bring `output` to `target_pressure` with the
/// temperature they mix to (a quadratic, then Newton, then the plain formula); 0 for tg's
/// `FALSE`.
pub fn gas_pressure_calculate(
    input: &dyn Mix,
    output: &dyn Mix,
    target_pressure: f64,
    mut ignore_temperature: bool,
) -> f64 {
    let our_moles = input.total_moles();
    let output_moles = output.total_moles();
    if our_moles <= 0.0 || input.temperature() <= 0.0 {
        return 0.0;
    }
    let pressure_delta = if output.temperature() <= 0.0 || output_moles <= 0.0 {
        ignore_temperature = true;
        target_pressure
    } else {
        target_pressure - output.pressure()
    };
    if pressure_delta < 0.01 || minimum_transfer_pressure(input, output) > target_pressure {
        return 0.0;
    }
    let simple = || pressure_delta * output.volume() / (input.temperature() * R_IDEAL_GAS_EQUATION);
    if ignore_temperature {
        return simple();
    }
    let pvr = target_pressure * output.volume() / R_IDEAL_GAS_EQUATION;
    let lower = ((pvr / input.temperature().max(output.temperature())) - output_moles).max(0.0);
    let upper = (pvr / input.temperature().min(output.temperature())) - output_moles;
    let lower = (lower - PRESSURE_ERROR_TOLERANCE).max(0.0);
    let upper = upper + PRESSURE_ERROR_TOLERANCE;

    let (w2, n2, c2) = (input.thermal_energy(), our_moles, input.heat_capacity());
    let (w1, n1, c1) = (
        output.thermal_energy(),
        output_moles,
        output.heat_capacity(),
    );
    let a = w2 / n2;
    let b = (n1 * w2) / n2 + w1 - (pvr * c2 / n2);
    let c = -pvr * c1 + n1 * w1;
    if let Some(n) = quadratic(a, b, c, lower, upper).or_else(|| newton(a, b, c, lower, upper)) {
        return n;
    }
    simple()
}

/// `gas_pressure_quadratic` (`SolveQuadratic`, the larger root).
fn quadratic(a: f64, b: f64, c: f64, lower: f64, upper: f64) -> Option<f64> {
    if !(a.is_finite() && b.is_finite() && c.is_finite()) {
        return None;
    }
    let solution = if a == 0.0 {
        (b != 0.0).then(|| -c / b)?
    } else {
        let discriminant = b * b - 4.0 * a * c;
        if discriminant < 0.0 {
            return None;
        }
        let root = discriminant.sqrt();
        ((-b + root) / (2.0 * a)).max((-b - root) / (2.0 * a))
    };
    (solution > lower && solution < upper).then_some(solution)
}

/// `gas_pressure_approximate`: Newton–Raphson from right of the vertex.
fn newton(a: f64, b: f64, c: f64, lower: f64, upper: f64) -> Option<f64> {
    if !(a.is_finite() && b.is_finite() && c.is_finite()) {
        return None;
    }
    let mut solution = -b / (2.0 * a) + 200.0;
    for _ in 0..APPROXIMATION_ITERATIONS {
        let diff = (a * solution * solution + b * solution + c) / (2.0 * a * solution + b);
        solution -= diff;
        if diff.abs() < MOLAR_ACCURACY && solution > lower && solution < upper {
            return Some(solution);
        }
    }
    None
}

/// The moles `pump_gas_to(output, target_pressure, output_pipenet_air)` takes from `input`.
pub fn pump_moles(
    input: &dyn Mix,
    output: &dyn Mix,
    target_pressure: f64,
    output_pipenet: Option<&dyn Mix>,
) -> f64 {
    let close = (input.temperature() - output.temperature()).abs() <= 5.0;
    let to_output = gas_pressure_calculate(input, output, target_pressure, close);
    let to_pipenet = output_pipenet
        .filter(|p| p.volume() != 0.0)
        .map_or(0.0, |p| {
            gas_pressure_calculate(input, p, target_pressure, close)
        });
    to_output.max(to_pipenet)
}

/// The moles `release_gas_to(output, target_pressure, rate, output_pipenet_air)` takes from
/// `input`; `None` when it does not release (tg `FALSE`: within 10 kPa of friction, or the
/// output is already at the target).
pub fn release_moles(
    input: &dyn Mix,
    output: &dyn Mix,
    target_pressure: f64,
    rate: f64,
    output_pipenet: Option<&dyn Mix>,
) -> Option<f64> {
    let output_pressure = output.pressure();
    let input_pressure = input.pressure();
    if output_pressure >= target_pressure.min(input_pressure - 10.0) {
        return None;
    }
    let target = output_pressure
        + (target_pressure - output_pressure).min((input_pressure - output_pressure) / 2.0);
    let close = (input.temperature() - output.temperature()).abs() <= 5.0;
    let to_output = gas_pressure_calculate(input, output, target, close);
    let to_pipenet = output_pipenet
        .filter(|p| p.volume() != 0.0)
        .map_or(0.0, |p| gas_pressure_calculate(input, p, target, close));
    Some(to_output.max(to_pipenet) * rate)
}

/// `pump_gas_to` between two non-turf mixtures; true when gas moved.
pub fn pump_gas(input: &mut PipeAir, output: &mut PipeAir, target_pressure: f64) -> bool {
    let moles = pump_moles(input, output, target_pressure, None);
    match input.remove(moles) {
        Some(removed) => {
            output.merge(&removed);
            true
        }
        None => false,
    }
}

/// `release_gas_to` between two non-turf mixtures; true when gas moved.
pub fn release_gas(input: &mut PipeAir, output: &mut PipeAir, target_pressure: f64) -> bool {
    let Some(moles) = release_moles(input, output, target_pressure, 1.0, None) else {
        return false;
    };
    match input.remove(moles) {
        Some(removed) => {
            output.merge(&removed);
            true
        }
        None => false,
    }
}

/// `equalize(other)` of a non-turf mixture with a turf's: same temperature, then every gas
/// spread by volume. True when anything moved.
pub fn equalize_with_turf(ours: &mut PipeAir, turf: &mut GasMixture) -> bool {
    let mut moved = false;
    if (ours.gas.temperature - turf.temperature).abs() > MINIMUM_TEMPERATURE_DELTA_TO_SUSPEND {
        moved = true;
        let self_heat = ours.heat_capacity();
        let other_heat = turf.heat_capacity();
        let t = (ours.gas.temperature * self_heat + turf.temperature * other_heat)
            / (self_heat + other_heat);
        ours.gas.temperature = t;
        turf.temperature = t;
    }
    let total_volume = ours.volume + CELL_VOLUME;
    let threshold = 0.1 / (R_IDEAL_GAS_EQUATION * ours.gas.temperature.max(TCMB));
    for (m, o) in ours.gas.moles.iter_mut().zip(turf.moles.iter_mut()) {
        if (*m / ours.volume - *o / CELL_VOLUME).abs() > threshold {
            moved = true;
            let total = *m + *o;
            *m = total * (ours.volume / total_volume);
            *o = total * (CELL_VOLUME / total_volume);
        }
    }
    ours.gas.garbage_collect();
    turf.garbage_collect();
    moved
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gas::{GasId, ONE_ATMOSPHERE, T20C};

    fn air(o2: f64, temperature: f64, volume: f64) -> PipeAir {
        let mut gas = GasMixture::vacuum();
        gas.set(GasId::O2, o2);
        gas.temperature = temperature;
        PipeAir::with_gas(gas, volume)
    }

    #[test]
    fn pressure_and_raw_heat_capacity() {
        // PV = nRT: 10 mol at 293.15 K in 200 L.
        let a = air(10.0, T20C, 200.0);
        assert!((a.pressure() - 10.0 * 8.31 * T20C / 200.0).abs() < 1e-9);
        // An empty pipe has no heat capacity, unlike a turf (7000).
        assert_eq!(PipeAir::new(70.0).heat_capacity(), 0.0);
        // Merging into an empty pipe keeps the giver's temperature.
        let mut empty = PipeAir::new(70.0);
        empty.merge(&a.gas);
        assert_eq!(empty.temperature(), T20C);
    }

    #[test]
    fn pump_to_empty_output_is_the_plain_formula() {
        // Output empty: ignore temperature, n = P*V/(T*R) = 101.325*200/(293.15*8.31).
        let input = air(100.0, T20C, 200.0);
        let output = PipeAir::new(200.0);
        let n = pump_moles(&input, &output, ONE_ATMOSPHERE, None);
        assert!(
            (n - ONE_ATMOSPHERE * 200.0 / (T20C * 8.31)).abs() < 1e-9,
            "{n}"
        );
    }

    #[test]
    fn pump_with_temperatures_reaches_the_target() {
        // Hot gas into cold gas: the quadratic accounts for the mixed temperature.
        let mut input = air(100.0, 500.0, 200.0);
        let mut output = air(5.0, 200.0, 200.0);
        assert!(pump_gas(&mut input, &mut output, 300.0));
        assert!(
            (output.pressure() - 300.0).abs() < 0.5,
            "{}",
            output.pressure()
        );
    }

    #[test]
    fn release_stops_at_half_the_difference() {
        // 200 kPa in, 0 out, target 150: at most (200-0)/2 = 100 kPa reached.
        let p = |n: f64| n * 8.31 * T20C / 200.0;
        let n_in = 200.0 * 200.0 / (8.31 * T20C);
        let mut input = air(n_in, T20C, 200.0);
        let mut output = PipeAir::new(200.0);
        assert!(release_gas(&mut input, &mut output, 150.0));
        assert!((output.pressure() - 100.0).abs() < 0.01);
        assert!((p(input.total_moles()) - 100.0).abs() < 0.01);
        // Within 10 kPa of each other: nothing moves.
        assert!(!release_gas(&mut input, &mut output, 150.0));
    }

    #[test]
    fn equalize_spreads_by_volume() {
        let mut ours = air(0.0, T20C, 500.0);
        let mut turf = GasMixture::vacuum();
        turf.set(GasId::O2, 30.0);
        turf.temperature = T20C;
        assert!(equalize_with_turf(&mut ours, &mut turf));
        assert!((ours.gas.get(GasId::O2) - 5.0).abs() < 1e-9);
        assert!((turf.get(GasId::O2) - 25.0).abs() < 1e-9);
    }
}
