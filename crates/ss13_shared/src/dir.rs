use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Direction with BYOND bit values, so values imported from `.dmm`/DM sources map 1:1.
#[derive(Reflect, Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum Dir {
    North = 1,
    #[default]
    South = 2,
    East = 4,
    West = 8,
    NorthEast = 5,
    NorthWest = 9,
    SouthEast = 6,
    SouthWest = 10,
}

/// Which way an entity faces (BYOND `dir` of a movable); picks the `.dmi` direction its
/// sprite and parent-facing overlays draw with. Separate from `Appearance` so that turning
/// replicates two bytes instead of the whole appearance with its overlays.
#[derive(
    Component, Reflect, Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize,
)]
pub struct Facing(pub Dir);

impl Dir {
    pub const CARDINALS: [Dir; 4] = [Dir::North, Dir::South, Dir::East, Dir::West];

    /// Order of directions inside a `.dmi` state with 4 or 8 dirs.
    pub const DMI_ORDER: [Dir; 8] = [
        Dir::South,
        Dir::North,
        Dir::East,
        Dir::West,
        Dir::SouthEast,
        Dir::SouthWest,
        Dir::NorthEast,
        Dir::NorthWest,
    ];

    pub fn from_bits(bits: u8) -> Option<Self> {
        Self::DMI_ORDER.into_iter().find(|d| *d as u8 == bits)
    }

    /// Grid offset; +y is north, as in BYOND.
    pub fn offset(self) -> (i32, i32) {
        let b = self as u8;
        let dx = i32::from(b & 4 != 0) - i32::from(b & 8 != 0);
        let dy = i32::from(b & 1 != 0) - i32::from(b & 2 != 0);
        (dx, dy)
    }

    pub fn reverse(self) -> Self {
        let (dx, dy) = self.offset();
        Self::DMI_ORDER
            .into_iter()
            .find(|d| d.offset() == (-dx, -dy))
            .expect("every direction has a reverse")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bits_roundtrip_and_reverse() {
        for d in Dir::DMI_ORDER {
            assert_eq!(Dir::from_bits(d as u8), Some(d));
            assert_eq!(d.reverse().reverse(), d);
        }
        assert_eq!(Dir::from_bits(3), None);
        assert_eq!(Dir::SouthWest.offset(), (-1, -1));
    }
}
