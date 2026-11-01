//! tgui default (Nanotrasen) theme: colors, sizes, fonts and CSS-like text inheritance.
//!
//! Values come from `tgui-core/styles/{base,colors,vars-components}.scss` and the fork's
//! `tgui/packages/tgui/styles/{css-variables,layouts/*}.scss`.

use bevy::app::{HierarchyPropagatePlugin, Propagate, PropagateSet};
use bevy::color::{Alpha, Color, Hsla, Oklcha};
use bevy::prelude::{
    App, Assets, Changed, Commands, Component, Handle, IntoScheduleConfigs, Or, PostUpdate,
    PreStartup, Query, ResMut, Resource, With,
};
use bevy::scene::{Scene, bsn};
use bevy::text::{
    Font, FontSize, FontSource, FontStyle, FontWeight, Justify, TextColor, TextFont, TextLayout,
};
use bevy::ui::{UiSystems, widget::Text};

use crate::icon::{ICON_FONT_FAMILY, IconGlyph};

/// Font family every tgui text uses. tgui asks for Verdana, which is proprietary; DejaVu Sans is
/// the usual metric-compatible-ish wide humanist substitute and covers Cyrillic.
pub const FONT_FAMILY: &str = "DejaVu Sans";

/// `--font-size`: 1em, the base of every `em`/`rem` value in tgui.
pub const FONT_SIZE: f32 = 12.0;
/// `.Section__titleText` / `.TitleBar__title`: 14px.
pub const FONT_SIZE_TITLE: f32 = 14.0;

/// tgui `unit()`: numeric Box sizes (`width={2}`) are rem.
pub const UNIT: f32 = FONT_SIZE;
/// tgui `halfUnit()`: numeric spacing (`m={1}`, `mt`, `px`, ...) is half a rem.
pub const HALF_UNIT: f32 = FONT_SIZE / 2.0;

// `--space-*` (base.scss), in px at 12px font.
pub const SPACE_XXS: f32 = 1.0;
pub const SPACE_XS: f32 = 2.0;
pub const SPACE_S: f32 = 3.0;
pub const SPACE_SM: f32 = 4.0;
pub const SPACE_M: f32 = 6.0;
pub const SPACE_ML: f32 = 8.0;
pub const SPACE_L: f32 = 10.0;
pub const SPACE_XL: f32 = 12.0;

// `--border-thickness-*` / `--border-radius-*`.
pub const BORDER_TINY: f32 = 1.0;
pub const BORDER_SMALL: f32 = 2.0;
pub const RADIUS_TINY: f32 = 2.0;
/// `--border-radius-small`: 0.25rem.
pub const RADIUS_SMALL: f32 = 3.0;
/// `--border-radius-medium`: 0.33rem.
pub const RADIUS_MEDIUM: f32 = 4.0;
/// `--border-radius-large`: 0.5rem.
pub const RADIUS_LARGE: f32 = 6.0;

/// `--button-height`: 1.667em.
pub const BUTTON_HEIGHT: f32 = 20.0;
/// `.Button--compact` line-height: 1.333em.
pub const BUTTON_HEIGHT_COMPACT: f32 = 16.0;
/// `.TitleBar` height: 32px.
pub const TITLEBAR_HEIGHT: f32 = 32.0;
/// `.TitleBar__statusIcon` / `.TitleBar__close` width: 45px.
pub const TITLEBAR_BUTTON_WIDTH: f32 = 45.0;

/// `--adjust-color`: lightness shift for "bg"/"fg" variants of a color, in percent points.
pub const ADJUST_COLOR: f32 = 5.0;
/// `--adjust-hover`.
pub const ADJUST_HOVER: f32 = 10.0;
/// `--adjust-active`.
pub const ADJUST_ACTIVE: f32 = 7.5;

pub(crate) const fn hsl(h: f32, s: f32, l: f32) -> Color {
    Color::Hsla(Hsla::new(h, s / 100.0, l / 100.0, 1.0))
}

pub(crate) const fn hsla(h: f32, s: f32, l: f32, a: f32) -> Color {
    Color::Hsla(Hsla::new(h, s / 100.0, l / 100.0, a))
}

/// Shifts HSL lightness by `points` percent points, like `hsl(from c h s calc(l + points))`.
pub fn lighten(color: Color, points: f32) -> Color {
    let mut c = Hsla::from(color);
    c.lightness = (c.lightness + points / 100.0).clamp(0.0, 1.0);
    c.into()
}

/// Multiplies the alpha channel.
pub fn fade(color: Color, factor: f32) -> Color {
    color.with_alpha(color.alpha() * factor)
}

// colors.scss
pub const COLOR_BASE: Color = hsl(0.0, 0.0, 15.0);
pub const COLOR_SECTION: Color = hsla(0.0, 0.0, 0.0, 0.33);
/// `--color-secondary`: base + 7.5 lightness (title bar).
pub const COLOR_SECONDARY: Color = hsl(0.0, 0.0, 22.5);
/// `--color-base-start` / `--color-base-end`: the window background gradient.
pub const COLOR_BASE_START: Color = hsl(0.0, 0.0, 17.0);
pub const COLOR_BASE_END: Color = hsl(0.0, 0.0, 13.0);
pub const COLOR_TEXT: Color = hsl(0.0, 0.0, 100.0);
pub const COLOR_TEXT_TRANSLUCENT: Color = hsla(0.0, 0.0, 100.0, 0.5);
pub const COLOR_TEXT_TRANSLUCENT_LIGHT: Color = hsla(0.0, 0.0, 100.0, 0.75);
pub const COLOR_BORDER: Color = hsla(0.0, 0.0, 100.0, 0.1);
pub const COLOR_BORDER_DARK: Color = hsla(0.0, 0.0, 0.0, 0.33);
pub const COLOR_PRIMARY: Color = hsl(210.0, 37.5, 45.0);
/// `--color-hover` / `--color-active` / `--color-selected`: `--color-action` (grey 50%) at
/// 0.25 / 0.3 / 0.4 alpha.
pub const COLOR_HOVER: Color = hsla(0.0, 0.0, 50.0, 0.25);
pub const COLOR_ACTIVE: Color = hsla(0.0, 0.0, 50.0, 0.3);
pub const COLOR_SELECTED: Color = hsla(0.0, 0.0, 50.0, 0.4);
/// `--color-scrollbar-base` / `--color-scrollbar-thumb` (base + 9.25 lightness).
pub const COLOR_SCROLLBAR_BASE: Color = COLOR_SECTION;
pub const COLOR_SCROLLBAR_THUMB: Color = hsl(0.0, 0.0, 24.25);
/// `--candystripe-odd`.
pub const CANDYSTRIPE_ODD: Color = hsla(0.0, 0.0, 0.0, 0.25);
/// `--notice-box-background`.
pub const NOTICE_BOX_BACKGROUND: Color = hsl(37.5, 55.0, 57.5);
/// `--button-disabled-opacity`.
pub const BUTTON_DISABLED_OPACITY: f32 = 0.5;

/// tgui color names (`color="bad"`, `<Button color="caution">`). `Default` means "no override":
/// the component's own default (e.g. primary for buttons).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum UiColor {
    #[default]
    Default,
    /// Button-only: no background, translucent text.
    Transparent,
    /// Button alias for yellow.
    Caution,
    /// Button alias for red.
    Danger,
    Red,
    Orange,
    Yellow,
    Olive,
    Green,
    Teal,
    Blue,
    Violet,
    Purple,
    Pink,
    Brown,
    Gold,
    Black,
    White,
    Grey,
    LightGrey,
    Primary,
    Good,
    Average,
    Bad,
    Label,
    /// A CSS color that is not a tgui color name (`color="crimson"`): tgui-core sets it as the
    /// inline `color` style, so it is used as is, without `--adjust-color`.
    Css([u8; 3]),
}

impl UiColor {
    /// The `--color-<name>` value. `Default`/`Transparent` resolve to primary, as buttons do.
    pub fn base(self) -> Color {
        match self {
            UiColor::Default | UiColor::Transparent | UiColor::Primary => COLOR_PRIMARY,
            UiColor::Red | UiColor::Danger => hsl(0.0, 70.0, 50.0),
            UiColor::Orange => hsl(25.0, 90.0, 50.0),
            UiColor::Yellow | UiColor::Caution => hsl(50.0, 97.5, 50.0),
            UiColor::Olive => hsl(70.0, 75.0, 45.0),
            UiColor::Green => hsl(140.0, 70.0, 40.0),
            UiColor::Teal => hsl(180.0, 100.0, 35.0),
            UiColor::Blue => hsl(210.0, 65.0, 47.5),
            UiColor::Violet => hsl(260.0, 60.0, 50.0),
            UiColor::Purple => hsl(290.0, 60.0, 50.0),
            UiColor::Pink => hsl(325.0, 70.0, 50.0),
            UiColor::Brown => hsl(25.0, 47.5, 45.0),
            UiColor::Gold => hsl(40.0, 90.0, 50.0),
            UiColor::Black => hsl(0.0, 0.0, 0.0),
            UiColor::White => hsl(0.0, 0.0, 100.0),
            UiColor::Grey => hsl(0.0, 0.0, 50.0),
            UiColor::LightGrey => hsl(0.0, 0.0, 66.6),
            UiColor::Good => hsl(95.0, 62.5, 40.0),
            UiColor::Average => hsl(32.5, 90.0, 50.0),
            UiColor::Bad => hsl(0.0, 72.5, 50.0),
            UiColor::Label => hsl(210.0, 17.5, 57.5),
            UiColor::Css([r, g, b]) => Color::srgb_u8(r, g, b),
        }
    }

    /// Atomic `.color-<name>` text color (base lightened by `--adjust-color`); `None` for
    /// `Default`, which inherits.
    pub fn text(self) -> Option<Color> {
        if let UiColor::Css(_) = self {
            return Some(self.base());
        }
        (self != UiColor::Default).then(|| lighten(self.base(), ADJUST_COLOR))
    }

    /// Atomic `.color-bg-<name>` background color; `None` for `Default`.
    pub fn background(self) -> Option<Color> {
        (self != UiColor::Default).then(|| lighten(self.base(), -ADJUST_COLOR))
    }

    /// `$low-contrast-color-map`: these backgrounds get black text.
    pub fn is_low_contrast(self) -> bool {
        matches!(self, UiColor::Yellow | UiColor::Caution | UiColor::White)
    }
}

/// NoticeBox background: `oklch(from c calc(l * 0.825) calc(c * 0.75) h)`.
pub fn notice_background(color: Color) -> Color {
    let mut c = Oklcha::from(color);
    c.lightness *= 0.825;
    c.chroma *= 0.75;
    c.into()
}

/// Inherited CSS `font-size`, in px. Spread down the tree with `Propagate<FontPx>`.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct FontPx(pub f32);

impl Default for FontPx {
    fn default() -> Self {
        Self(FONT_SIZE)
    }
}

/// Inherited CSS `font-weight: bold`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct Bold(pub bool);

/// Inherited CSS `font-style: italic`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct Italic(pub bool);

/// Inherited CSS `text-align`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct TextAlign(pub Justify);

/// Keeps the bundled font faces alive; they are looked up by family name, not by handle.
#[derive(Resource)]
pub struct UiFonts(pub Vec<Handle<Font>>);

const FONT_FACES: [&[u8]; 6] = [
    include_bytes!("../assets/fonts/DejaVuSans.ttf"),
    include_bytes!("../assets/fonts/DejaVuSans-Bold.ttf"),
    include_bytes!("../assets/fonts/DejaVuSans-Oblique.ttf"),
    include_bytes!("../assets/fonts/DejaVuSans-BoldOblique.ttf"),
    // Whole files (420 + 68 KB): subsetting would break any icon name a ported window adds.
    include_bytes!("../assets/fonts/fa-solid-900.ttf"),
    include_bytes!("../assets/fonts/fa-regular-400.ttf"),
];

fn load_fonts(mut fonts: ResMut<Assets<Font>>, mut commands: Commands) {
    let handles = FONT_FACES
        .iter()
        .map(|bytes| fonts.add(Font::from_bytes(bytes.to_vec())))
        .collect();
    commands.insert_resource(UiFonts(handles));
}

/// Builds `TextFont`/`TextLayout` of tgui text from the inherited pieces. Only text under a tgui
/// root (which seeds `FontPx`) is touched, so other UI keeps its own fonts.
fn compose_text_style(
    mut texts: Query<
        (
            &mut TextFont,
            &mut TextLayout,
            &FontPx,
            Option<&Bold>,
            Option<&Italic>,
            Option<&TextAlign>,
            Option<&IconGlyph>,
        ),
        (
            With<Text>,
            Or<(
                Changed<IconGlyph>,
                Changed<FontPx>,
                Changed<Bold>,
                Changed<Italic>,
                Changed<TextAlign>,
            )>,
        ),
    >,
) {
    for (mut font, mut layout, size, bold, italic, align, icon) in &mut texts {
        let weight = if bold.is_some_and(|b| b.0) {
            FontWeight::BOLD
        } else {
            FontWeight::NORMAL
        };
        let style = if italic.is_some_and(|i| i.0) {
            FontStyle::Italic
        } else {
            FontStyle::Normal
        };
        if let Some(icon) = icon {
            font.font = FontSource::Family(ICON_FONT_FAMILY.into());
            font.font_size = FontSize::Px(size.0 * icon.scale);
            font.weight = if icon.regular {
                FontWeight::NORMAL
            } else {
                FontWeight::BLACK
            };
            font.style = FontStyle::Normal;
        } else {
            font.font = FontSource::Family(FONT_FAMILY.into());
            font.font_size = FontSize::Px(size.0);
            font.weight = weight;
            font.style = style;
        }
        let justify = align.map_or(Justify::Left, |a| a.0);
        if layout.justify != justify {
            layout.justify = justify;
        }
    }
}

/// Seeds the inherited text style at a tgui root (window, standalone panel).
pub fn root_text_style() -> impl Scene {
    bsn! {
        Propagate::<FontPx>(FontPx(FONT_SIZE))
        Propagate::<TextColor>(TextColor(COLOR_TEXT))
    }
}

pub(crate) fn plugin(app: &mut App) {
    app.add_plugins((
        HierarchyPropagatePlugin::<TextColor>::new(PostUpdate),
        HierarchyPropagatePlugin::<FontPx>::new(PostUpdate),
        HierarchyPropagatePlugin::<Bold>::new(PostUpdate),
        HierarchyPropagatePlugin::<Italic>::new(PostUpdate),
        HierarchyPropagatePlugin::<TextAlign>::new(PostUpdate),
    ));
    // Text must be styled before `UiSystems::Content` measures it.
    app.configure_sets(
        PostUpdate,
        (
            PropagateSet::<TextColor>::default(),
            PropagateSet::<FontPx>::default(),
            PropagateSet::<Bold>::default(),
            PropagateSet::<Italic>::default(),
            PropagateSet::<TextAlign>::default(),
        )
            .in_set(UiSystems::Propagate),
    );
    app.add_systems(PreStartup, load_fonts).add_systems(
        PostUpdate,
        compose_text_style
            .in_set(UiSystems::Propagate)
            .after(PropagateSet::<FontPx>::default())
            .after(PropagateSet::<Bold>::default())
            .after(PropagateSet::<Italic>::default())
            .after(PropagateSet::<TextAlign>::default()),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn button_default_background_matches_css() {
        // `.Button--color--default`: hsl(210 37.5% calc(45% - 5)).
        let bg = Hsla::from(UiColor::Primary.background().unwrap());
        assert!((bg.lightness - 0.40).abs() < 1e-4);
        assert!((bg.hue - 210.0).abs() < 1e-3);
    }

    #[test]
    fn default_color_inherits() {
        assert_eq!(UiColor::Default.text(), None);
        assert!(UiColor::Bad.text().is_some());
    }
}
