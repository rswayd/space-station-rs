//! Energy accumulators (server): tg `code/modules/power/energy_accumulator.dm` and the tesla
//! coil and grounding rod of `code/modules/power/tesla/coil.dm`.
//!
//! Zaps (the supermatter's, [`super::supermatter`]) put energy into an [`Accumulator`]; every
//! machine tick it releases `dE/dt = -0.04 E - 2000 W` of it: a coil into its powernet, a rod
//! into the ground. A coil keeps `input_power_multiplier` of a zap (25% with the board's tier 1
//! capacitor) and passes the rest on; a rod swallows the whole zap.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;

use super::{Grid, NodeKind};
use crate::obj_damage::Anchored;

pub const TESLA_COIL_ROOT: &str = "/obj/machinery/power/energy_accumulator/tesla_coil";
pub const GROUNDING_ROD_ROOT: &str = "/obj/machinery/power/energy_accumulator/grounding_rod";

/// `ACCUMULATOR_PROPORTIONAL_COEFFICIENT`, 1/s.
pub const PROPORTIONAL_COEFFICIENT: f64 = 0.04;
/// `ACCUMULATOR_CONSTANT_COEFFICIENT`, W.
pub const CONSTANT_COEFFICIENT: f64 = 2000.0;
/// `RefreshParts` of a coil with one tier 1 capacitor: `max(1 / 8, 0.25)`.
pub const COIL_INPUT_MULTIPLIER: f64 = 0.25;
/// `ZAP_GENERATES_POWER`, `ZAP_LOW_POWER_GEN`.
pub const ZAP_GENERATES_POWER: u32 = 1 << 5;
pub const ZAP_LOW_POWER_GEN: u32 = 1 << 6;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AccumulatorKind {
    TeslaCoil { input_power_multiplier: f64 },
    GroundingRod,
}

/// tg `/obj/machinery/power/energy_accumulator`.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Accumulator {
    pub kind: AccumulatorKind,
    /// Joules waiting to be released.
    pub stored_energy: f64,
    /// Joules released last tick.
    pub processed_energy: f64,
    /// `world.time` of the last zap that hit it (`TRAIT_BEING_SHOCKED` for 1 s).
    pub shocked_at: f64,
}

impl Accumulator {
    pub fn new(kind: AccumulatorKind) -> Self {
        Self {
            kind,
            stored_energy: 0.0,
            processed_energy: 0.0,
            shocked_at: f64::NEG_INFINITY,
        }
    }

    /// `calculate_energy_output(time)`: what it releases over `time` seconds.
    pub fn energy_output(&self, time: f64) -> f64 {
        let e = self.stored_energy;
        let k = PROPORTIONAL_COEFFICIENT;
        let c = CONSTANT_COEFFICIENT;
        e.min(e - ((k * e + c) * (-k * time).exp() - c) / k)
    }

    /// `zap_act(power, zap_flags)` of an anchored, closed accumulator: the energy it passes on.
    /// `wired`: a coil has a powernet (without one it keeps the whole zap).
    pub fn zap(&mut self, power: f64, flags: u32, wired: bool, now: f64) -> f64 {
        self.shocked_at = now;
        match self.kind {
            AccumulatorKind::GroundingRod => {
                self.stored_energy += power;
                0.0
            }
            AccumulatorKind::TeslaCoil {
                input_power_multiplier,
            } => {
                if flags & ZAP_GENERATES_POWER == 0 {
                    return 0.0;
                }
                let power = if flags & ZAP_LOW_POWER_GEN != 0 {
                    power / 10.0
                } else {
                    power
                };
                let removed = if wired {
                    power * input_power_multiplier
                } else {
                    power
                };
                self.stored_energy += removed.max(0.0);
                (power - removed).max(0.0)
            }
        }
    }

    /// `process()`: `release_energy(calculate_energy_output(dt))`; the joules that go into the
    /// powernet (none from a rod).
    pub fn process(&mut self, dt: f64) -> f64 {
        let joules = self.energy_output(dt);
        self.stored_energy -= joules;
        self.processed_energy = joules;
        match self.kind {
            AccumulatorKind::TeslaCoil { .. } => joules,
            AccumulatorKind::GroundingRod => 0.0,
        }
    }
}

/// Coils and rods from map load; anchored coils join their powernet (`should_have_node`).
pub(super) fn init_accumulators(world: &mut World) {
    let mut query = world.query_filtered::<
        (Entity, &super::DmPower, &ss13_shared::TilePos),
        With<ss13_world::Mapped>,
    >();
    let found: Vec<(Entity, AccumulatorKind, ss13_shared::TilePos, u8)> = query
        .iter(world)
        .filter_map(|(e, dm, &pos)| {
            let kind = if is_under(&dm.path, TESLA_COIL_ROOT) {
                AccumulatorKind::TeslaCoil {
                    input_power_multiplier: COIL_INPUT_MULTIPLIER,
                }
            } else if is_under(&dm.path, GROUNDING_ROD_ROOT) {
                AccumulatorKind::GroundingRod
            } else {
                return None;
            };
            Some((e, kind, pos, super::layer(dm)))
        })
        .collect();
    for &(e, kind, pos, layer) in &found {
        let anchored = world.get::<Anchored>(e).is_some_and(|a| a.0);
        if anchored && matches!(kind, AccumulatorKind::TeslaCoil { .. }) {
            world
                .resource_mut::<Grid>()
                .add_node(e, pos, layer, NodeKind::Machine);
        }
    }
    info!("energy accumulators: {}", found.len());
    world.insert_batch(
        found
            .into_iter()
            .map(|(e, kind, ..)| (e, Accumulator::new(kind))),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_curve_matches_tg() {
        let mut a = Accumulator::new(AccumulatorKind::TeslaCoil {
            input_power_multiplier: COIL_INPUT_MULTIPLIER,
        });
        assert_eq!(a.energy_output(2.0), 0.0);
        // 1 MJ stored, 2 s: E(1 - e^-0.08) + 2000 (1 - e^-0.08) / 0.04.
        a.stored_energy = 1e6;
        let f = 1.0 - (-0.08f64).exp();
        let want = 1e6 * f + 2000.0 * f / 0.04;
        assert!((a.energy_output(2.0) - want).abs() < 1e-6);
        assert!((want - 80_727.8).abs() < 0.1);
        // A little left: all of it goes.
        a.stored_energy = 1000.0;
        assert_eq!(a.energy_output(2.0), 1000.0);
    }

    #[test]
    fn coil_keeps_a_quarter_rod_keeps_all() {
        let mut coil = Accumulator::new(AccumulatorKind::TeslaCoil {
            input_power_multiplier: COIL_INPUT_MULTIPLIER,
        });
        assert_eq!(coil.zap(1e6, ZAP_GENERATES_POWER, true, 0.0), 750_000.0);
        assert_eq!(coil.stored_energy, 250_000.0);
        // Not a power zap: nothing stored, nothing passed on.
        assert_eq!(coil.zap(1e6, 0, true, 0.0), 0.0);
        // Unwired: keeps everything.
        assert_eq!(coil.zap(1e6, ZAP_GENERATES_POWER, false, 0.0), 0.0);
        let mut rod = Accumulator::new(AccumulatorKind::GroundingRod);
        assert_eq!(rod.zap(5e5, ZAP_GENERATES_POWER, false, 0.0), 0.0);
        assert_eq!(rod.process(2.0), 0.0);
        assert!(rod.stored_energy < 5e5);
    }
}
