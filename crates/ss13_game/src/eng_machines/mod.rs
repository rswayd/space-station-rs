//! Engineering machines without a home of their own (server), after tg
//! `code/game/machinery/`: the sheetifier ([`sheetifier`]), atmospheric shield generators
//! ([`atmos_shield`]) and the flatpacker ([`flatpacker`]).

use bevy::prelude::*;

pub mod atmos_shield;
pub mod flatpacker;
pub mod sheetifier;

pub struct EngMachinesPlugin;

impl Plugin for EngMachinesPlugin {
    fn build(&self, app: &mut App) {
        sheetifier::build(app);
        atmos_shield::build(app);
        flatpacker::build(app);
    }
}

/// The timed actions of these machines (`crate::construction::Job::EngMachine`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EngJob {
    /// A multitool deploying a flatpack.
    DeployFlatpack,
}

/// A finished [`EngJob`] on `target`.
pub fn complete(world: &mut World, client: Entity, target: Entity, job: EngJob) {
    match job {
        EngJob::DeployFlatpack => flatpacker::complete_deploy(world, client, target),
    }
}
