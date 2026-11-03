//! Random planetary atmospheres: tg `/datum/atmosphere` (`code/datums/atmosphere/`). Turfs whose
//! `initial_gas_mix` is an atmosphere id (`LAVALAND_ATMOS`, `ICEMOON_ATMOS`) get the gas string
//! generated for that id once per round (`SSair.atmos_gen`, `preprocess_gas_string`).
// shortcut: no `STATION_TRAIT_UNNATURAL_ATMOSPHERE` (+40% restricted chance); station traits are
// not ported.

use crate::gas::{GasId, GasMixture, T20C};
use crate::scan::num;

/// One `/datum/atmosphere` type.
#[derive(Clone, Copy, Debug)]
pub struct AtmosphereTemplate {
    pub id: &'static str,
    pub base_gases: &'static [(GasId, f64)],
    pub normal_gases: &'static [(GasId, f64)],
    pub restricted_gases: &'static [(GasId, f64)],
    /// Percent per pick.
    pub restricted_chance: f64,
    pub minimum_pressure: f64,
    pub maximum_pressure: f64,
    pub minimum_temp: f64,
    pub maximum_temp: f64,
}

/// `HAZARD_LOW_PRESSURE + 10`, `LAVALAND_EQUIPMENT_EFFECT_PRESSURE - 1`.
const MIN_PRESSURE: f64 = 20.0 + 10.0;
const MAX_PRESSURE: f64 = 50.0 - 1.0;
const NORMAL: &[(GasId, f64)] = &[(GasId::O2, 10.0), (GasId::N2, 10.0), (GasId::CO2, 10.0)];
const BASE: &[(GasId, f64)] = &[(GasId::O2, 5.0), (GasId::N2, 10.0)];

/// `/datum/atmosphere/lavaland`.
pub const LAVALAND: AtmosphereTemplate = AtmosphereTemplate {
    id: "LAVALAND_ATMOS",
    base_gases: BASE,
    normal_gases: NORMAL,
    restricted_gases: &[
        (GasId::PLASMA, 0.1),
        (GasId::BZ, 1.2),
        (GasId::MIASMA, 1.2),
        (GasId::WATER_VAPOR, 0.1),
    ],
    restricted_chance: 30.0,
    minimum_pressure: MIN_PRESSURE,
    maximum_pressure: MAX_PRESSURE,
    // `BODYTEMP_COLD_DAMAGE_LIMIT + 1`, `LAVALAND_MAX_TEMPERATURE`.
    minimum_temp: 310.15 - 40.0 + 1.0,
    maximum_temp: 350.0,
};

/// `/datum/atmosphere/icemoon`.
pub const ICEMOON: AtmosphereTemplate = AtmosphereTemplate {
    id: "ICEMOON_ATMOS",
    base_gases: BASE,
    normal_gases: NORMAL,
    restricted_gases: &[
        (GasId::PLASMA, 0.1),
        (GasId::WATER_VAPOR, 0.1),
        (GasId::MIASMA, 1.2),
    ],
    restricted_chance: 20.0,
    minimum_pressure: MIN_PRESSURE,
    maximum_pressure: MAX_PRESSURE,
    // `ICEBOX_MIN_TEMPERATURE`.
    minimum_temp: 180.0,
    maximum_temp: 180.0,
};

pub const TEMPLATES: [AtmosphereTemplate; 2] = [LAVALAND, ICEMOON];

/// The round's random numbers (splitmix64).
#[derive(Clone, Debug)]
pub struct AtmosRng(pub u64);

impl AtmosRng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// DM `rand(low, high)`: integers when both ends are, else uniform.
    fn rand(&mut self, low: f64, high: f64) -> f64 {
        if low.fract() == 0.0 && high.fract() == 0.0 {
            let span = (high - low) as u64 + 1;
            low + (self.next() % span) as f64
        } else {
            low + (self.next() >> 11) as f64 / (1u64 << 53) as f64 * (high - low)
        }
    }

    fn prob(&mut self, percent: f64) -> bool {
        (self.next() % 10_000) as f64 / 100.0 < percent
    }

    fn pick<T: Copy>(&mut self, list: &[T]) -> T {
        list[(self.next() % list.len() as u64) as usize]
    }
}

/// `generate_gas_string`.
pub fn generate_gas_string(t: &AtmosphereTemplate, rng: &mut AtmosRng) -> String {
    let mut spicy: Vec<(GasId, f64)> = t.restricted_gases.to_vec();
    let mut target = rng.rand(t.minimum_pressure, t.maximum_pressure);
    let scalar = target / t.maximum_pressure;
    let mut mix = GasMixture::vacuum();
    mix.temperature = rng.rand(t.minimum_temp, t.maximum_temp);
    for &(gas, moles) in t.base_gases {
        mix.set(gas, moles);
    }
    let mut last = t.normal_gases[0].0;
    while mix.pressure() < target {
        let (gas, base) = if !rng.prob(t.restricted_chance) || spicy.is_empty() {
            rng.pick(t.normal_gases)
        } else {
            let i = (rng.next() % spicy.len() as u64) as usize;
            spicy.remove(i)
        };
        let mut amount = base * rng.rand(50.0, 200.0) / 100.0;
        amount *= scalar;
        // `CEILING(amount, 0.1)`.
        amount = (amount / 0.1).ceil() * 0.1;
        mix.add(gas, amount);
        last = gas;
    }
    target = target.clamp(t.minimum_pressure + mix.get(last) * 0.1, t.maximum_pressure);
    while mix.pressure() > target {
        let m = mix.get(last);
        mix.set(last, m - m * 0.1);
    }
    mix.set(last, (mix.get(last) / 0.1).floor() * 0.1);
    mix.garbage_collect();
    let mut parts: Vec<String> = crate::gas::GASES
        .iter()
        .zip(mix.moles)
        .filter(|(_, m)| *m > 0.0)
        .map(|(g, m)| format!("{}={}", g.id, num(m)))
        .collect();
    parts.push(format!("TEMP={}", num(mix.temperature)));
    parts.join(";")
}

/// `SSair.atmos_gen`: the gas string of every template id for this round.
pub fn generate_atmos(seed: u64) -> Vec<(&'static str, String)> {
    let mut rng = AtmosRng(seed);
    TEMPLATES
        .iter()
        .map(|t| (t.id, generate_gas_string(t, &mut rng)))
        .collect()
}

/// Room temperature for strings without `TEMP` (tg `parse_gas_string`).
pub const DEFAULT_TEMPERATURE: f64 = T20C;

#[cfg(test)]
mod tests {
    use super::*;

    /// Every generated atmosphere is breathable-ish thin air within its template's bounds.
    #[test]
    fn generated_atmospheres_stay_in_bounds() {
        for seed in 0..200 {
            for (id, text) in generate_atmos(seed) {
                let mix = GasMixture::parse(&text).unwrap();
                let t = TEMPLATES.iter().find(|t| t.id == id).unwrap();
                let p = mix.pressure();
                assert!(
                    p <= t.maximum_pressure + 0.5 && p >= t.minimum_pressure - 3.0,
                    "{id} {text}: {p} kPa"
                );
                assert!(
                    (t.minimum_temp..=t.maximum_temp + 1e-9).contains(&mix.temperature),
                    "{text}"
                );
                assert!(mix.get(GasId::O2) > 0.0 && mix.get(GasId::N2) > 0.0);
            }
        }
    }
}
