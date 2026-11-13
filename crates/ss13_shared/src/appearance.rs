use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::Dir;

/// Index of an icon state in the asset manifest that server and client both load
/// (built by `ss13_assets`). Replicated instead of `"icons/obj/doors.dmi" + state` strings.
#[derive(Reflect, Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct IconStateId(pub u32);

/// Draw order inside a plane, BYOND `layer` semantics (higher draws on top).
#[derive(Reflect, Clone, Copy, Debug, Default, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct Layer(pub f32);

/// What a client needs to draw an entity, except its direction, which is the separate
/// [`Facing`](crate::Facing) component (absent = south). Replicated; kept small on purpose:
/// extend it through the coordinator when a rendering feature needs a new field.
#[derive(Component, Reflect, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Appearance {
    pub icon_state: IconStateId,
    pub layer: Layer,
    /// Render plane, BYOND `plane` semantics; planes are composited separately (lighting etc.).
    pub plane: i16,
    /// RGBA multiply, `None` = untinted. BYOND `alpha` folds into the 4th byte.
    pub color: Option<[u8; 4]>,
    /// BYOND `pixel_x`/`pixel_y`: sprite offset from the tile in pixels, +y up.
    pub pixel_offset: [i16; 2],
    /// BYOND `transform` turned clockwise by this many degrees around the icon's center, with
    /// overlays (tg `lying_angle`: 90 or 270 for a mob lying down); 0 = upright.
    pub rotation: u16,
    /// Drawn above the base sprite in list order.
    pub overlays: Vec<Overlay>,
    /// BYOND `mouse_opacity`: whether clicks stop at this atom.
    pub mouse_opacity: MouseOpacity,
    /// BYOND `glide_size` (an appearance var there too), icon pixels per second; 0 = the client's
    /// default step glide. Nonzero: the drawn point, tile plus `pixel_offset`, moves as one at
    /// this rate (tg projectiles: `animate_movement = NO_STEPS`, `pixel_x`/`pixel_y` animated
    /// every tick).
    pub glide: u16,
}

/// BYOND `mouse_opacity` (`MOUSE_OPACITY_TRANSPARENT` 0, `MOUSE_OPACITY_ICON` 1,
/// `MOUSE_OPACITY_OPAQUE` 2): a transparent atom is never clicked, clicks go to what is below.
#[derive(Reflect, Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MouseOpacity {
    Transparent,
    /// Clicked where its icon is opaque (BYOND's default).
    #[default]
    Icon,
    /// Clicked anywhere in its tile, transparent pixels included.
    Opaque,
}

impl MouseOpacity {
    /// The DM value (0, 1, 2); anything else is BYOND's default.
    pub fn from_dm(value: u8) -> Self {
        match value {
            0 => Self::Transparent,
            2 => Self::Opaque,
            _ => Self::Icon,
        }
    }
}

/// One BYOND overlay: an extra sprite drawn with its parent.
#[derive(Reflect, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Overlay {
    pub icon_state: IconStateId,
    /// `None` = the parent's facing.
    pub dir: Option<Dir>,
    /// RGBA multiply (alpha in the 4th byte), `None` = the parent's color.
    pub color: Option<[u8; 4]>,
    /// Pixels relative to the parent's `pixel_offset`, +y up.
    pub pixel_offset: [i16; 2],
    /// BYOND `plane`, `None` = the parent's. Planes from [`LIGHTING_PLANE`] up are not darkened;
    /// [`EMISSIVE_PLANE`] is a mask: where it covers, whatever is drawn below is shown unlit.
    pub plane: Option<i16>,
    /// BYOND direction bits of the parent's facings this overlay is drawn for, `0` = all (tg
    /// crops legs per facing with all-or-nothing masks, `icons/mob/leg_masks.dmi`).
    pub only_dirs: u8,
}

/// tg `LIGHTING_PLANE`: sprites below it are multiplied by the light, from it up they are not.
pub const LIGHTING_PLANE: i16 = 10;
/// tg `EMISSIVE_PLANE` (`emissive_appearance()`): the overlay's alpha lights what is below it.
pub const EMISSIVE_PLANE: i16 = 13;
/// tg `ABOVE_LIGHTING_PLANE`: drawn as is, above the lighting.
pub const ABOVE_LIGHTING_PLANE: i16 = 25;

impl Overlay {
    /// A plain overlay: the parent's facing, color and plane, no offset.
    pub fn new(icon_state: IconStateId) -> Self {
        Self {
            icon_state,
            dir: None,
            color: None,
            pixel_offset: [0, 0],
            plane: None,
            only_dirs: 0,
        }
    }

    /// Drawn for facing `dir`. A diagonal counts as its east/west part, the frame BYOND draws for
    /// a 4-direction icon.
    pub fn shown_for(&self, dir: Dir) -> bool {
        let bits = dir as u8;
        let drawn = if bits & 0b1100 != 0 {
            bits & 0b1100
        } else {
            bits
        };
        self.only_dirs == 0 || self.only_dirs & drawn != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_dirs_matches_the_drawn_frame() {
        let mut o = Overlay::new(IconStateId(1));
        assert!(Dir::DMI_ORDER.iter().all(|&d| o.shown_for(d)));
        // tg's `right_leg` mask: south, north and east.
        o.only_dirs = Dir::South as u8 | Dir::North as u8 | Dir::East as u8;
        assert!(o.shown_for(Dir::North) && o.shown_for(Dir::East));
        assert!(!o.shown_for(Dir::West));
        assert!(o.shown_for(Dir::NorthEast) && o.shown_for(Dir::SouthEast));
        assert!(!o.shown_for(Dir::NorthWest) && !o.shown_for(Dir::SouthWest));
    }
}
