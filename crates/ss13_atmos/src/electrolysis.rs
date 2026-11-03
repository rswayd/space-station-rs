//! Electrolysis: tg `/datum/gas_reaction/electrolyzer` (`machinery/components/electrolyzer/
//! electrolyzer_reactions.dm`) and `gas_mixture/electrolyze`. Separate from [`crate::reactions`]:
//! they run only where something puts energy into the air (an electrolyzer, a supermatter zap),
//! never on their own.

use crate::gas::{GasId as G, GasMixture, MINIMUM_HEAT_CAPACITY, TCMB};
use crate::reactions::{FIRE_MINIMUM_TEMPERATURE_TO_EXIST, MINIMUM_MOLE_COUNT};

/// `HALON_FORMATION_ENERGY`, joules per mole of BZ.
pub const HALON_FORMATION_ENERGY: f64 = 91232.1;
/// `POWER_PENALTY_THRESHOLD` and `CRITICAL_POWER_PENALTY_THRESHOLD` (`__DEFINES/supermatter.dm`):
/// the zap power range over which hyper-noblium turns into anti-noblium.
pub const NOB_CONVERSION_MIN_POWER: f64 = 5000.0;
pub const NOB_CONVERSION_FULL_POWER: f64 = 9000.0;

impl GasMixture {
    /// `electrolyze(working_power, electrolyzer_args)`: every electrolyzer reaction whose
    /// requirements hold, in tg order. `working_power` 1 is a tier 1 electrolyzer;
    /// `supermatter_power` is the zap's power level (`ELECTROLYSIS_ARGUMENT_SUPERMATTER_POWER`).
    pub fn electrolyze(&mut self, working_power: f64, supermatter_power: Option<f64>) {
        if self.get(G::WATER_VAPOR) >= MINIMUM_MOLE_COUNT {
            self.h2o_conversion(working_power);
        }
        if let Some(power) = supermatter_power.filter(|&p| p > NOB_CONVERSION_MIN_POWER)
            && self.get(G::HYPERNOBLIUM) >= MINIMUM_MOLE_COUNT
        {
            self.nob_conversion(power);
        }
        if self.get(G::BZ) >= MINIMUM_MOLE_COUNT {
            self.halon_generation(working_power);
        }
        self.garbage_collect();
    }

    /// `h2o_conversion`: 2 H2O -> 2 H2 + O2, at most `2.5 * working_power²` a call.
    fn h2o_conversion(&mut self, working_power: f64) {
        let old_heat_capacity = self.heat_capacity();
        let proportion = (self.get(G::WATER_VAPOR) / 2.0).min(2.5 * working_power.powi(2));
        self.add(G::WATER_VAPOR, -proportion * 2.0);
        self.add(G::O2, proportion);
        self.add(G::HYDROGEN, proportion * 2.0);
        self.rescale_temperature(old_heat_capacity, self.heat_capacity());
    }

    /// `nob_conversion`: hyper-noblium -> anti-noblium, all of it at the critical power.
    fn nob_conversion(&mut self, supermatter_power: f64) {
        let span = NOB_CONVERSION_FULL_POWER - NOB_CONVERSION_MIN_POWER;
        let old_heat_capacity = self.heat_capacity();
        let electrolysed = self.get(G::HYPERNOBLIUM)
            * (supermatter_power - NOB_CONVERSION_MIN_POWER).clamp(0.0, span)
            / span;
        self.add(G::HYPERNOBLIUM, -electrolysed);
        self.add(G::ANTINOBLIUM, electrolysed);
        let new_heat_capacity = old_heat_capacity
            + electrolysed
                * (G::ANTINOBLIUM.info().specific_heat - G::HYPERNOBLIUM.info().specific_heat);
        self.rescale_temperature(old_heat_capacity, new_heat_capacity);
    }

    /// `halon_generation`: BZ -> 2 halon + 0.2 O2, faster the hotter, releasing
    /// [`HALON_FORMATION_ENERGY`] per mole.
    fn halon_generation(&mut self, working_power: f64) {
        let old_heat_capacity = self.heat_capacity();
        let bz = self.get(G::BZ);
        let efficiency = (bz
            * (1.0
                - (-0.5 * self.temperature * working_power / FIRE_MINIMUM_TEMPERATURE_TO_EXIST)
                    .exp()))
        .min(bz);
        self.add(G::BZ, -efficiency);
        self.add(G::O2, efficiency * 0.2);
        self.add(G::HALON, efficiency * 2.0);
        let new_heat_capacity = self.heat_capacity();
        if new_heat_capacity > MINIMUM_HEAT_CAPACITY {
            let energy = efficiency * HALON_FORMATION_ENERGY;
            self.temperature =
                ((self.temperature * old_heat_capacity + energy) / new_heat_capacity).max(TCMB);
        }
    }

    /// `temperature * old / new` floored at TCMB, when the new capacity is meaningful.
    fn rescale_temperature(&mut self, old_heat_capacity: f64, new_heat_capacity: f64) {
        if new_heat_capacity > MINIMUM_HEAT_CAPACITY {
            self.temperature = (self.temperature * old_heat_capacity / new_heat_capacity).max(TCMB);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn water_splits_at_most_two_and_a_half_working_power_squared() {
        let mut air = GasMixture::vacuum();
        air.set(G::WATER_VAPOR, 100.0);
        air.temperature = 300.0;
        air.electrolyze(2.0, None);
        // min(100 / 2, 2.5 * 4) = 10.
        assert!(close(air.get(G::WATER_VAPOR), 80.0));
        assert!(close(air.get(G::O2), 10.0));
        assert!(close(air.get(G::HYDROGEN), 20.0));

        let mut small = GasMixture::vacuum();
        small.set(G::WATER_VAPOR, 3.0);
        small.temperature = 300.0;
        small.electrolyze(2.0, None);
        assert_eq!(small.get(G::WATER_VAPOR), 0.0);
        assert!(close(small.get(G::HYDROGEN), 3.0));
        assert!(close(small.get(G::O2), 1.5));
    }

    #[test]
    fn noblium_turns_only_under_a_strong_zap() {
        let base = {
            let mut air = GasMixture::vacuum();
            air.set(G::HYPERNOBLIUM, 10.0);
            air.temperature = 300.0;
            air
        };
        for power in [None, Some(5000.0)] {
            let mut air = base.clone();
            air.electrolyze(1.0, power);
            assert_eq!(air.get(G::HYPERNOBLIUM), 10.0, "{power:?}");
        }
        let mut half = base.clone();
        half.electrolyze(1.0, Some(7000.0));
        assert!(close(half.get(G::HYPERNOBLIUM), 5.0));
        assert!(close(half.get(G::ANTINOBLIUM), 5.0));
        let mut full = base;
        full.electrolyze(1.0, Some(20000.0));
        assert_eq!(full.get(G::HYPERNOBLIUM), 0.0);
        assert!(close(full.get(G::ANTINOBLIUM), 10.0));
    }

    #[test]
    fn bz_becomes_halon_faster_when_hot_releasing_its_energy() {
        let at = |temperature: f64| {
            let mut air = GasMixture::vacuum();
            air.set(G::BZ, 10.0);
            air.temperature = temperature;
            air.electrolyze(1.0, None);
            air
        };
        let (cold, hot) = (at(100.0), at(1000.0));
        assert!(cold.get(G::HALON) < hot.get(G::HALON));
        // Everything balances: each mole of BZ gives 2 halon and 0.2 oxygen.
        let used = 10.0 - hot.get(G::BZ);
        assert!(close(hot.get(G::HALON), used * 2.0));
        assert!(close(hot.get(G::O2), used * 0.2));
        // The energy goes into a mixture with a much larger heat capacity (halon).
        let before = 10.0 * G::BZ.info().specific_heat;
        let expected = (1000.0 * before + used * HALON_FORMATION_ENERGY) / hot.heat_capacity();
        assert!(close(hot.temperature, expected));
    }
}
