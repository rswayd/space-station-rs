//! Lighting contract. The server replicates only what lights and blocks light; every client
//! computes its own lightmap from it (tg's per-corner math, see `ss13_render::lighting`).

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{Dir, TilePos};

/// What an entity does to light. Replicated; absent = transparent, emits nothing, and (on a turf)
/// a tile lit by light sources only.
#[derive(Component, Reflect, Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Optics {
    /// Blocks light (BYOND `opacity`). A tile is opaque when any of its entities is.
    pub opaque: bool,
    /// Turfs only: how the tile itself is lit. `None` = by light sources, no ambient.
    pub tile: Option<TileLighting>,
    pub light: Option<LightSource>,
}

/// tg area lighting of one tile.
#[derive(Reflect, Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TileLighting {
    /// A `space_lit` turf under default starlight: no lighting object, [`STARLIGHT_AMBIENT`].
    /// One byte on the wire for most of a station's tiles.
    Space,
    Area {
        /// `false` for tiles tg gives no lighting object (areas without `static_lighting`,
        /// `space_lit` turfs): light sources do not light the tile itself.
        dynamic: bool,
        /// tg base lighting (`base_lighting_color` + `base_lighting_alpha`, or starlight),
        /// added to the tile's light.
        ambient: [u8; 4],
    },
}

/// tg `COLOR_STARLIGHT` at full alpha: the ambient of [`TileLighting::Space`].
// shortcut: tg's round-start starlight; replicate it once per connection when events change it
// (`set_starlight`).
pub const STARLIGHT_AMBIENT: [u8; 4] = [0x85, 0x89, 0xfa, 255];

impl TileLighting {
    /// The compact form of `Area { dynamic, ambient }`.
    pub fn new(dynamic: bool, ambient: [u8; 4]) -> Self {
        if !dynamic && ambient == STARLIGHT_AMBIENT {
            Self::Space
        } else {
            Self::Area { dynamic, ambient }
        }
    }

    pub fn dynamic(self) -> bool {
        matches!(self, Self::Area { dynamic: true, .. })
    }

    pub fn ambient(self) -> [u8; 4] {
        match self {
            Self::Space => STARLIGHT_AMBIENT,
            Self::Area { ambient, .. } => ambient,
        }
    }
}

/// One tg `COMPLEX_LIGHT` source, in tg units.
#[derive(Reflect, Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct LightSource {
    /// `light_range`, tiles.
    pub range: f32,
    /// `light_power`; negative darkens.
    pub power: f32,
    pub color: [u8; 3],
    /// `light_height`, tiles above the floor (pseudo-z of the falloff).
    pub height: f32,
    /// `light_angle`, degrees; 360 = all around.
    pub angle: u16,
    /// `light_dir`: the cone axis when `angle < 360`.
    pub dir: Dir,
    /// tg `offset_x`/`offset_y` of the light source in 1/32 tile: added to every corner's
    /// offset from the source tile before the falloff is evaluated.
    pub offset: [i8; 2],
}

/// Side of a light block in tiles. Light sources and opacity reach clients per block, from a
/// wider box than entities (a source lights up to its range beyond itself, and its shadows need
/// the opacity in between); blocks start at tile 1 like the map.
pub const LIGHT_BLOCK: i32 = 16;

/// The origin (lowest x and y) of the light block holding `pos`, and the index of `pos` in it:
/// `(y - origin.y) * LIGHT_BLOCK + (x - origin.x)`.
pub fn light_block(pos: TilePos) -> (TilePos, usize) {
    let (bx, by) = (
        (pos.x - 1).div_euclid(LIGHT_BLOCK),
        (pos.y - 1).div_euclid(LIGHT_BLOCK),
    );
    let origin = TilePos::new(bx * LIGHT_BLOCK + 1, by * LIGHT_BLOCK + 1, pos.z);
    let index = (pos.y - origin.y) * LIGHT_BLOCK + (pos.x - origin.x);
    (origin, index as usize)
}

/// Light data the client learns beyond replicated entities, in the order the server sent it.
/// `ss13_net` writes these; the client lighting reads them. Light sources and opacity of tiles
/// come only this way; entities keep [`Optics::tile`].
#[derive(Message, Clone, Debug, PartialEq)]
pub enum LightUpdate {
    /// Opacity of the block at `origin`: tile index `i` (see [`light_block`]) is opaque when bit
    /// `i % 8` of `bits[i / 8]` is set. Replaces what was known of the block's opacity.
    Opacity { origin: TilePos, bits: [u8; 32] },
    /// Every light source on `pos` (empty: none), replacing those known there.
    Lights {
        pos: TilePos,
        lights: Vec<LightSource>,
    },
    /// The block at `origin` left the client's interest: forget its opacity, lights and seen
    /// tiles.
    Forget { origin: TilePos },
    /// Tiles of the block at `origin` a view mask (camera static) shows (bit layout of
    /// `Opacity`); the others are drawn black. A block without it is shown whole.
    Seen { origin: TilePos, bits: [u8; 32] },
    /// Draw only the tiles the camera target's eye sees (BYOND `view()` over the known
    /// opacity) within this Chebyshev radius; `None`: no line of sight (observers).
    LineOfSight(Option<u32>),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_start_at_tile_one() {
        assert_eq!(
            light_block(TilePos::new(1, 1, 2)),
            (TilePos::new(1, 1, 2), 0)
        );
        assert_eq!(
            light_block(TilePos::new(16, 16, 2)),
            (TilePos::new(1, 1, 2), 255)
        );
        assert_eq!(
            light_block(TilePos::new(17, 3, 2)),
            (TilePos::new(17, 1, 2), 32)
        );
        assert_eq!(
            light_block(TilePos::new(0, 0, 1)),
            (TilePos::new(-15, -15, 1), 255)
        );
    }
}
