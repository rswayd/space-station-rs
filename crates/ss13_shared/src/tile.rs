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

/// Which z-levels stand over which (tg `ZTRAIT_UP` / `ZTRAIT_DOWN`: the levels of one
/// multi-deck map, stacked as loaded). The server sets it at round start; a client gets it
/// with its first replication batch, to draw the level below its own through open space.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LevelStack {
    /// `(below, above)` pairs.
    pub links: Vec<(u16, u16)>,
}

impl LevelStack {
    pub fn link(&mut self, below: u16, above: u16) {
        if !self.links.contains(&(below, above)) {
            self.links.push((below, above));
        }
    }

    pub fn above(&self, z: u16) -> Option<u16> {
        self.links.iter().find(|(b, _)| *b == z).map(|(_, a)| *a)
    }

    pub fn below(&self, z: u16) -> Option<u16> {
        self.links.iter().find(|(_, a)| *a == z).map(|(b, _)| *b)
    }

    /// `get_step_multiz(pos, UP/DOWN)`.
    pub fn step(&self, pos: TilePos, up: bool) -> Option<TilePos> {
        let z = if up {
            self.above(pos.z)
        } else {
            self.below(pos.z)
        }?;
        Some(TilePos { z, ..pos })
    }
}
