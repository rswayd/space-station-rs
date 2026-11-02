//! Contracts every other crate builds on. Changes here touch the whole workspace:
//! they go through the coordinator, never through a feature agent.

pub mod admin;
pub mod appearance;
pub mod chat;
pub mod debug;
pub mod dir;
pub mod light;
pub mod line_of_sight;
pub mod protocol;
pub mod tile;
pub mod ui;

pub use appearance::{
    ABOVE_LIGHTING_PLANE, Appearance, EMISSIVE_PLANE, IconStateId, LIGHTING_PLANE, Layer,
    MouseOpacity, Overlay,
};
pub use dir::{Dir, Facing};
pub use light::{
    LIGHT_BLOCK, LightSource, LightUpdate, Optics, STARLIGHT_AMBIENT, TileLighting, light_block,
};
pub use tile::{TILE_SIZE, TilePos};
