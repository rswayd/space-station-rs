//! Single items of `code/game/objects/items` without a family of their own (the parity group
//! `docs/parity/items/misc-items.md`).
//!
//! - [`charter`]: the station charter renaming the station.

pub mod charter;

use bevy::prelude::*;

pub struct MiscItemsPlugin;

impl Plugin for MiscItemsPlugin {
    fn build(&self, app: &mut App) {
        charter::build(app);
    }
}
