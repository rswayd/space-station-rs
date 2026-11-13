//! BandaStation's donor name effects in chat (`tgui-panel/styles/tgchat/chat-dark.scss`,
//! `.tier-3`..`.tier-5`): a [`Span::DonorKey`] with a [`NameShine`] is animated here; the panel
//! only tags its text piece with [`DonorShine`] ([`shine_patch`]).
//!
//! The CSS keeps the color's OKLCH lightness within 0.33..0.8 (`--stabilized-color`) and sweeps a
//! gradient towards a lighter (`l + 20`) color over the name: every 5 s for metallic, every 3 s
//! and 15° further round the hue for glowing.
// shortcut: the whole name pulses through the gradient's colors instead of a band moving across
// its glyphs, and tier 5's blurred halo (`@mixin shine`) is not drawn; per-glyph colors need the
// row's text split per character.

use std::f32::consts::TAU;

use bevy::color::{Hsla, Oklcha};
use ss13_shared::chat::{NameShine, Span};
use ss13_ui_kit::prelude::*;

/// On a chat text piece of a [`Span::DonorKey`] with an effect.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct DonorShine {
    pub rgb: [u8; 3],
    pub shine: Option<NameShine>,
}

/// What the panel adds to a text piece of `span`: the effect's marker, if it has one.
pub fn shine_patch(span: Span) -> Option<impl Scene> {
    match span {
        Span::DonorKey {
            rgb,
            shine: Some(shine),
        } => Some(bsn! { DonorShine { rgb: {rgb}, shine: {Some(shine)} } }),
        _ => None,
    }
}

/// `--stabilized-color`: `oklch(from c min(max(l, 0.33), 0.8) c h)`.
fn stabilized(rgb: [u8; 3]) -> Oklcha {
    let mut c = Oklcha::from(Color::srgb_u8(rgb[0], rgb[1], rgb[2]));
    c.lightness = c.lightness.clamp(0.33, 0.8);
    c
}

/// The gradient's light stop: `hsl(from c h s calc(l + 20))`, `calc(h + 15)` for glowing.
fn light_stop(base: Oklcha, shine: NameShine) -> Hsla {
    let mut c = Hsla::from(Color::from(base));
    c.lightness = (c.lightness + 0.2).min(1.0);
    if shine != NameShine::Metallic {
        c.hue = (c.hue + 15.0).rem_euclid(360.0);
    }
    c
}

/// The animation's period: `metalic-anim 5s`, `glowing-anim 3s`.
fn period(shine: NameShine) -> f32 {
    match shine {
        NameShine::Metallic => 5.0,
        NameShine::Glowing | NameShine::GlowingShine => 3.0,
    }
}

/// The name's color `secs` into the animation: the stabilized color at the start of a period,
/// the light stop halfway.
pub fn shine_color(rgb: [u8; 3], shine: NameShine, secs: f32) -> Color {
    let base = stabilized(rgb);
    let from = Color::from(base).to_srgba();
    let to = Color::from(light_stop(base, shine)).to_srgba();
    let phase = (secs / period(shine)).fract();
    let f = 0.5 - 0.5 * (TAU * phase).cos();
    let lerp = |a: f32, b: f32| a + (b - a) * f;
    Color::srgb(
        lerp(from.red, to.red),
        lerp(from.green, to.green),
        lerp(from.blue, to.blue),
    )
}

/// Cost: one color per visible donor name per frame (only rows in view exist).
pub(crate) fn animate(time: Res<Time<Real>>, mut names: Query<(&DonorShine, &mut TextColor)>) {
    let secs = time.elapsed_secs_wrapped();
    for (name, mut color) in &mut names {
        if let Some(shine) = name.shine {
            color.0 = shine_color(name.rgb, shine, secs);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_pulse_from_the_stabilized_color_to_the_light_stop() {
        let rgb = [0xc4, 0x3b, 0x23];
        let start = shine_color(rgb, NameShine::Metallic, 0.0).to_srgba();
        let base = Color::from(stabilized(rgb)).to_srgba();
        assert!((start.red - base.red).abs() < 1e-3 && (start.blue - base.blue).abs() < 1e-3);
        let mid = Hsla::from(shine_color(rgb, NameShine::Metallic, 2.5));
        let base = Hsla::from(Color::from(stabilized(rgb)));
        assert!((mid.lightness - (base.lightness + 0.2)).abs() < 1e-2);
        // A full period comes back.
        let again = shine_color(rgb, NameShine::Glowing, 3.0).to_srgba();
        let first = shine_color(rgb, NameShine::Glowing, 0.0).to_srgba();
        assert!((again.green - first.green).abs() < 1e-3);
    }

    #[test]
    fn dark_and_light_colors_are_kept_readable() {
        assert!((stabilized([0, 0, 0]).lightness - 0.33).abs() < 1e-6);
        assert!((stabilized([255, 255, 255]).lightness - 0.8).abs() < 1e-6);
    }
}
