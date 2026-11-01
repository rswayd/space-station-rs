use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::Dir;

/// Size of one tile in world pixels, as in BYOND `world.icon_size`.
pub const TILE_SIZE: u32 = 32;

/// Position on the tile grid. `z` is a separate map level, not height on the same map.
///
/// Immutable: `ss13_world` keeps a spatial index in sync through component hooks, which a
/// `&mut TilePos` write would bypass. Move an entity by inserting a new `TilePos`
/// (or `World::modify_component`), which runs the hooks.
#[derive(
    Component, Reflect, Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize,
)]
#[component(immutable)]
pub struct TilePos {
    pub x: i32,
    pub y: i32,
    pub z: u16,
}

impl TilePos {
    pub const fn new(x: i32, y: i32, z: u16) -> Self {
        Self { x, y, z }
    }

    /// Neighbouring tile in `dir` on the same z-level.
    pub fn step(self, dir: Dir) -> Self {
        let (dx, dy) = dir.offset();
        Self::new(self.x + dx, self.y + dy, self.z)
    }

    /// Chebyshev distance, the metric BYOND `get_dist` and view ranges use.
    /// Different z-levels are infinitely far apart.
    pub fn chebyshev(self, other: Self) -> Option<u32> {
        (self.z == other.z).then(|| self.x.abs_diff(other.x).max(self.y.abs_diff(other.y)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_and_distance() {
        let a = TilePos::new(5, 5, 1);
        assert_eq!(a.step(Dir::NorthEast), TilePos::new(6, 6, 1));
        assert_eq!(a.chebyshev(TilePos::new(8, 3, 1)), Some(3));
        assert_eq!(a.chebyshev(TilePos::new(5, 5, 2)), None);
    }
}
