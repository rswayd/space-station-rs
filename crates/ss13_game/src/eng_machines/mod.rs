//! Engineering machines without a home of their own (server), after tg
//! `code/game/machinery/`: the sheetifier ([`sheetifier`]), atmospheric shield generators
//! ([`atmos_shield`]), the flatpacker ([`flatpacker`]) and the modular shield
//! ([`modular_shield`]) and the manufactorio lines ([`manufactorio`]).

use bevy::prelude::*;

pub mod atmos_shield;
pub mod flatpacker;
pub mod manufactorio;
pub mod modular_shield;
pub mod sheetifier;

pub struct EngMachinesPlugin;

impl Plugin for EngMachinesPlugin {
    fn build(&self, app: &mut App) {
        sheetifier::build(app);
        atmos_shield::build(app);
        flatpacker::build(app);
        modular_shield::build(app);
        manufactorio::build(app);
    }
}

/// The timed actions of these machines (`crate::construction::Job::EngMachine`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EngJob {
    /// A multitool deploying a flatpack.
    DeployFlatpack,
    /// An empty hand dumping a manufacturing storage unit.
    DumpStorage,
    /// A screwdriver breaking a manufacturing storage unit up.
    BreakStorage,
}

/// A finished [`EngJob`] on `target`.
pub fn complete(world: &mut World, client: Entity, target: Entity, job: EngJob) {
    match job {
        EngJob::DeployFlatpack => flatpacker::complete_deploy(world, client, target),
        EngJob::DumpStorage => manufactorio::dump(world, target),
        EngJob::BreakStorage => manufactorio::break_storage(world, target),
    }
}
