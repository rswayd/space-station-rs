//! Single items of `code/game/objects/items` without a family of their own (the parity group
//! `docs/parity/items/misc-items.md`).
//!
//! - [`charter`]: the station charter renaming the station.
//! - [`documents`]: secret documents, their photocopies and forged seals.
//! - [`target`]: shooting range targets and their bullet holes.
//! - [`flatpacks`]: typed flatpacks and the flatpack cart.
//! - [`puzzle`]: ruin keycards, keycard doors and the puzzle panels opening puzzle doors.

pub mod charter;
pub mod documents;
pub mod flatpacks;
pub mod puzzle;
pub mod target;

use bevy::prelude::*;

pub struct MiscItemsPlugin;

impl Plugin for MiscItemsPlugin {
    fn build(&self, app: &mut App) {
        charter::build(app);
        documents::build(app);
        target::build(app);
        puzzle::build(app);
        flatpacks::build(app);
    }
}
