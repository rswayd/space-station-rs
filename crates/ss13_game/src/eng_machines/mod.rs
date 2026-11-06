//! Engineering machines without a home of their own (server), after tg
//! `code/game/machinery/`: the sheetifier ([`sheetifier`]).

use bevy::prelude::*;

pub mod sheetifier;

pub struct EngMachinesPlugin;

impl Plugin for EngMachinesPlugin {
    fn build(&self, app: &mut App) {
        sheetifier::build(app);
    }
}
