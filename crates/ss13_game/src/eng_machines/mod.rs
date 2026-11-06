//! Engineering machines without a home of their own (server), after tg
//! `code/game/machinery/`: the sheetifier ([`sheetifier`]) and atmospheric shield generators
//! ([`atmos_shield`]).

use bevy::prelude::*;

pub mod atmos_shield;
pub mod sheetifier;

pub struct EngMachinesPlugin;

impl Plugin for EngMachinesPlugin {
    fn build(&self, app: &mut App) {
        sheetifier::build(app);
        atmos_shield::build(app);
    }
}
