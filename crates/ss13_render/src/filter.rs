//! Filters on the world (tg's game plane master): a colour matrix (`client_colours`) and a
//! gaussian blur (`eye_blur`), drawn as one post-process pass of the world camera, so the UI
//! camera's overlays and HUD on top stay sharp and uncoloured.
//!
//! Insert [`ScreenFilter`] on the [`WorldCamera`](crate::WorldCamera) while a filter applies;
//! without it the pass does not run.

use bevy::asset::embedded_asset;
use bevy::core_pipeline::Core2dSystems;
use bevy::core_pipeline::fullscreen_material::{FullscreenMaterial, FullscreenMaterialPlugin};
use bevy::core_pipeline::schedule::Core2d;
use bevy::core_pipeline::tonemapping::tonemapping;
use bevy::ecs::schedule::{ScheduleConfigs, ScheduleLabel};
use bevy::ecs::system::BoxedSystem;
use bevy::prelude::*;
use bevy::render::RenderApp;
use bevy::render::extract_component::ExtractComponent;
use bevy::render::render_resource::ShaderType;
use bevy::shader::ShaderRef;

/// The world filter. Colours follow BYOND's matrix: `out = r * red + g * green + b * blue +
/// a * alpha + constant`, on sRGB values.
#[derive(Component, ExtractComponent, Clone, Copy, Debug, PartialEq, ShaderType)]
#[extract_app(RenderApp)]
pub struct ScreenFilter {
    pub red: Vec4,
    pub green: Vec4,
    pub blue: Vec4,
    pub alpha: Vec4,
    pub constant: Vec4,
    /// x: the blur's standard deviation in target pixels (0: none); the rest pads to 16 bytes.
    pub blur: Vec4,
}

impl Default for ScreenFilter {
    fn default() -> Self {
        Self::from_matrix(IDENTITY, 0.0)
    }
}

/// The identity colour matrix (rows r, g, b, a, constant).
pub const IDENTITY: [f32; 20] = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0,
    0.0,
];

impl ScreenFilter {
    /// A filter from a BYOND colour matrix and a blur in target pixels.
    pub fn from_matrix(m: [f32; 20], blur: f32) -> Self {
        let row = |i: usize| Vec4::new(m[i], m[i + 1], m[i + 2], m[i + 3]);
        Self {
            red: row(0),
            green: row(4),
            blue: row(8),
            alpha: row(12),
            constant: row(16),
            blur: Vec4::new(blur.max(0.0), 0.0, 0.0, 0.0),
        }
    }

    /// The filter changes nothing (the pass can be skipped).
    pub fn is_identity(&self) -> bool {
        *self == Self::default()
    }
}

/// BYOND colour matrices applied one after the other (`a` first) as one.
pub fn compose(a: &[f32; 20], b: &[f32; 20]) -> [f32; 20] {
    let mut out = [0.0; 20];
    for (i, row) in out.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        // Row i of `a` (an input channel's contribution, or the constant) through `b`.
        for (j, value) in row.iter_mut().enumerate() {
            let through: f32 = (0..4).map(|k| a[i * 4 + k] * b[k * 4 + j]).sum();
            *value = through + if i == 4 { b[16 + j] } else { 0.0 };
        }
    }
    out
}

/// Each entry linearly between `a` and `b` (BYOND animates a filter's matrix so).
pub fn lerp(a: &[f32; 20], b: &[f32; 20], t: f32) -> [f32; 20] {
    if t >= 1.0 {
        return *b;
    }
    let t = t.max(0.0);
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t)
}

impl FullscreenMaterial for ScreenFilter {
    fn fragment_shader() -> ShaderRef {
        "embedded://ss13_render/filter.wgsl".into()
    }

    fn schedule() -> impl ScheduleLabel + Clone {
        Core2d
    }

    fn schedule_configs(system: ScheduleConfigs<BoxedSystem>) -> ScheduleConfigs<BoxedSystem> {
        system
            .in_set(Core2dSystems::PostProcess)
            .before(tonemapping)
    }
}

pub struct ScreenFilterPlugin;

impl Plugin for ScreenFilterPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "filter.wgsl");
        app.add_plugins(FullscreenMaterialPlugin::<ScreenFilter>::default());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matrices_compose_in_order() {
        let grey = [
            0.33, 0.33, 0.33, 0.0, 0.59, 0.59, 0.59, 0.0, 0.11, 0.11, 0.11, 0.0, 0.0, 0.0, 0.0,
            1.0, 0.0, 0.0, 0.0, 0.0,
        ];
        let near = |a: [f32; 20], b: [f32; 20]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-6);
        assert!(near(compose(&IDENTITY, &grey), grey));
        assert!(near(compose(&grey, &IDENTITY), grey));
        // A constant added first goes through the second matrix.
        let mut plus = IDENTITY;
        plus[16] = 0.5;
        let both = compose(&plus, &grey);
        assert!((both[16] - 0.165).abs() < 1e-6 && (both[17] - 0.165).abs() < 1e-6);
        assert_eq!(lerp(&IDENTITY, &grey, 0.0), IDENTITY);
        assert!(near(lerp(&IDENTITY, &grey, 1.0), grey));
        assert!(ScreenFilter::default().is_identity());
        assert!(!ScreenFilter::from_matrix(IDENTITY, 1.0).is_identity());
    }
}
