//! tgui-core text inputs: [`Input`], [`TextArea`], [`NumberInput`].
//!
//! Editing itself (Unicode-aware cursor, selection, word jumps, clipboard, IME preedit, mouse
//! selection, max length in chars) is Bevy's `EditableText` + `TextInput` from `bevy_ui_widgets`,
//! driven by `DefaultPlugins`. This module adds the tgui shape on top: decoration, placeholder,
//! `onChange`/`onEnter` semantics, `selfClear`, Shift+Enter/Tab in text areas, and the
//! drag-or-type `NumberInput`.

#[cfg(test)]
mod editing_tests;
mod number;
mod text;

use bevy::input_focus::tab_navigation::TabNavigationPlugin;
use bevy::prelude::*;

pub use number::{NumberInput, NumberInputChanged};
pub use text::{Input, InputChanged, InputSubmitted, TextArea};

/// Registers the input widgets. Expects `DefaultPlugins` (text input, focus, picking).
pub struct InputWidgetsPlugin;

impl Plugin for InputWidgetsPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<TabNavigationPlugin>() {
            app.add_plugins(TabNavigationPlugin);
        }
        text::build(app);
        number::build(app);
    }
}

/// Input colors and sizes. Everything tgui shares with other widgets comes from [`crate::theme`];
/// the rest are input-only variables the theme does not carry.
mod style {
    use bevy::prelude::*;

    pub use crate::theme::{
        BORDER_TINY as BORDER_PX, COLOR_TEXT as TEXT, COLOR_TEXT_TRANSLUCENT as PLACEHOLDER,
        FONT_SIZE, RADIUS_TINY as RADIUS_PX, UNIT,
    };

    /// `--input-background`: `--color-base` (hsl 0 0% 15%) at lightness 5%.
    pub const BACKGROUND: Color = Color::hsl(0.0, 0.0, 0.05);
    /// `--color-border-primary`: hsl(210 100% 75% / 0.75).
    pub const BORDER: Color = Color::hsla(210.0, 1.0, 0.75, 0.75);
    /// `--color-border-secondary`, used for focus and the number input value/bar.
    pub const BORDER_FOCUS: Color = Color::hsl(210.0, 1.0, 0.75);
    pub const SELECTION: Color = Color::hsla(210.0, 1.0, 0.75, 0.4);
    /// `--input-width`: 9em.
    pub const INPUT_WIDTH_PX: f32 = 9.0 * UNIT;
}

/// Font for an input's text: the root's `TextFont` if the caller set one, else the tgui font.
fn root_font(font: Option<&TextFont>) -> TextFont {
    font.cloned().unwrap_or_else(|| TextFont {
        font: bevy::text::FontSource::Family(crate::theme::FONT_FAMILY.into()),
        ..TextFont::from_font_size(style::FONT_SIZE)
    })
}
