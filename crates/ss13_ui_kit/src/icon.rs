//! tgui `Icon`: Font Awesome Free glyphs.
//!
//! The bundled fonts and name table are Font Awesome Free 6.7.2, the version BandaStation serves
//! from `html/font-awesome` (fonts: SIL OFL 1.1, see `assets/fonts/FontAwesome-LICENSE.txt`).
//! Names follow tgui's `Icon`: `"cog"` is solid, a `-o` suffix (`"square-o"`) is regular, an
//! optional `fa-` prefix is ignored, and v5 aliases (`"times"`, `"edit"`) resolve like the CSS.

use std::collections::HashMap;
use std::sync::LazyLock;

use bevy::app::Propagate;
use bevy::math::Rot2;
use bevy::prelude::{App, Component, Query, Res, Time, Update, With};
use bevy::scene::{Scene, SceneComponent, bsn};
use bevy::text::TextColor;
use bevy::ui::UiTransform;
use bevy::ui::widget::Text;

use crate::layout::Inline;
use crate::theme::UiColor;

/// Family name inside both bundled Font Awesome files; the weight picks solid (900) or regular.
pub const ICON_FONT_FAMILY: &str = "Font Awesome 6 Free";

/// `name codepoint styles` lines generated from the package's `metadata/icon-families.json`.
const ICON_TABLE: &str = include_str!("../assets/fonts/fa-icons.txt");

struct Entry {
    glyph: char,
    solid: bool,
    regular: bool,
}

static ICONS: LazyLock<HashMap<&'static str, Entry>> = LazyLock::new(|| {
    ICON_TABLE
        .lines()
        .filter(|l| !l.starts_with('#'))
        .filter_map(|line| {
            let mut parts = line.split(' ');
            let name = parts.next()?;
            let glyph = char::from_u32(u32::from_str_radix(parts.next()?, 16).ok()?)?;
            let styles = parts.next()?;
            let entry = Entry {
                glyph,
                solid: styles.contains('s'),
                regular: styles.contains('r'),
            };
            Some((name, entry))
        })
        .collect()
});

/// Resolves a tgui icon name to its glyph and whether it is the regular (outlined) style.
/// `None` when Font Awesome Free has no such icon in that style (tgui renders nothing then).
pub fn icon_glyph(name: &str) -> Option<(char, bool)> {
    let name = name.strip_prefix("fa-").unwrap_or(name);
    let (base, regular) = match name.strip_suffix("-o") {
        Some(base) => (base, true),
        None => (name, false),
    };
    let entry = ICONS.get(base)?;
    let available = if regular { entry.regular } else { entry.solid };
    available.then_some((entry.glyph, regular))
}

/// tgui `Icon`. Inline, inherits the text color unless `color` is set.
#[derive(SceneComponent, Clone, Debug, Default)]
#[scene(icon(IconProps))]
pub struct Icon;

#[derive(Default)]
pub struct IconProps {
    pub name: String,
    /// `size`: font size multiplier (`size={2}` is 200%); `0` means 1.
    pub size: f32,
    /// `rotation` in degrees, clockwise.
    pub rotation: f32,
    /// `spin`: one turn every 2 s (`fa-spin`).
    pub spin: bool,
    pub color: UiColor,
}

/// The text entity of an icon: picks the icon font instead of the tgui font. `scale` multiplies
/// the inherited font size.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct IconGlyph {
    pub regular: bool,
    pub scale: f32,
}

impl Default for IconGlyph {
    fn default() -> Self {
        Self {
            regular: false,
            scale: 1.0,
        }
    }
}

/// `fa-spin`.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct IconSpin;

fn icon(props: IconProps) -> impl Scene {
    let (glyph, regular) = icon_glyph(&props.name).unwrap_or((' ', false));
    let glyph = glyph.to_string();
    let scale = if props.size > 0.0 { props.size } else { 1.0 };
    let rotation = UiTransform::from_rotation(Rot2::degrees(props.rotation));
    (
        bsn! {
            Text(glyph)
            IconGlyph { regular: regular, scale: scale }
            Inline
            rotation
        },
        props
            .color
            .text()
            .map(|c| bsn! { Propagate::<TextColor>(TextColor(c)) }),
        props.spin.then_some(bsn! { IconSpin }),
    )
}

/// `@keyframes spin`: linear, 2 s per turn.
fn spin_icons(time: Res<Time>, mut icons: Query<&mut UiTransform, With<IconSpin>>) {
    let angle = Rot2::radians(time.elapsed_secs_wrapped() * std::f32::consts::PI);
    for mut transform in &mut icons {
        transform.rotation = angle;
    }
}

pub(crate) fn plugin(app: &mut App) {
    app.add_systems(Update, spin_icons);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_resolve_like_tgui() {
        // v5 alias, canonical name and `fa-` prefix all hit `xmark`.
        assert_eq!(icon_glyph("times"), Some(('\u{f00d}', false)));
        assert_eq!(icon_glyph("xmark"), icon_glyph("fa-times"));
        assert_eq!(icon_glyph("cog"), Some(('\u{f013}', false)));
        // `-o` is the regular style; `square-check` has one, `cog` does not.
        assert_eq!(icon_glyph("check-square-o"), Some(('\u{f14a}', true)));
        assert_eq!(icon_glyph("cog-o"), None);
        assert_eq!(icon_glyph("no-such-icon"), None);
    }
}
