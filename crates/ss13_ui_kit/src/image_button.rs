//! tgui-core `ImageButton` (`styles/components/ImageButton.scss`): a picture with a caption,
//! as a grid tile or, `fluid`, as a full-width row. The picture is any scene the caller passes
//! (`@image`); the caption is the children written under the widget.

use bevy::app::Propagate;
use bevy::ecs::change_detection::DetectChangesMut;
use bevy::input_focus::tab_navigation::TabIndex;
use bevy::picking::hover::Hovered;
use bevy::prelude::{
    AlignItems, AlignSelf, App, BackgroundColor, BorderColor, BorderRadius, Changed, Children,
    Color, Commands, Component, Display, Entity, FlexDirection, Has, IntoScheduleConfigs, Node, Or,
    Overflow, PositionType, PostUpdate, Query, UiRect, Val, With, px,
};
use bevy::scene::{Scene, SceneComponent, SceneList, bsn};
use bevy::text::TextColor;
use bevy::ui::{InteractionDisabled, UiSystems};

use crate::button::TrailingMargin;
use crate::layout::{HasSlot, Inline, Part, Slot};
use crate::overlay::Placement;
use crate::theme::{self, UiColor, hsl, hsla};
use crate::tooltip::Tooltip;

/// tgui-core `ImageButton`. Fields are live state (style follows); the rest comes from the props.
/// Clicks go through [`crate::Act`] / [`crate::ActParams`] on the same entity, as on `Button`.
#[derive(SceneComponent, Clone, Debug, Default, PartialEq)]
#[scene(image_button(ImageButtonProps))]
pub struct ImageButton {
    pub color: UiColor,
    pub selected: bool,
    pub disabled: bool,
}

#[derive(Default)]
pub struct ImageButtonProps {
    /// The picture, drawn at `image_size` px square.
    pub image: Option<Box<dyn SceneList>>,
    /// `imageSize`; `0` means tgui's default 64.
    pub image_size: f32,
    /// Full-width row instead of a tile.
    pub fluid: bool,
    pub tooltip: String,
    pub tooltip_position: Placement,
    /// `buttons`: over the picture's bottom-left corner (tile) or at the row's right end.
    pub buttons: Option<Box<dyn SceneList>>,
    /// `buttonsAlt`: over the picture's top (tile) or at the row's right end.
    pub buttons_alt: Option<Box<dyn SceneList>>,
}

/// `.ImageButton--fluid`.
#[derive(Component, Default, Clone)]
struct Fluid;

/// The caption box (`.ImageButton__content`).
#[derive(Component, Default, Clone)]
struct ImageButtonContent;

/// `.ImageButton--disabled .ImageButton__container { opacity: 0.5 }`: `bevy_ui` has no subtree
/// opacity, so a half-transparent veil of the section background covers the container instead.
#[derive(Component, Default, Clone)]
struct DisabledVeil;

/// Default `--imagebutton-color`: `hsl(from var(--color-base) h s 30)`.
fn base_color(button: &ImageButton) -> Color {
    if button.selected {
        // `--button-background-selected` (green) lightened by 15%.
        let green = theme::lighten(UiColor::Green.base(), -theme::ADJUST_COLOR);
        let hsl_c: bevy::color::Hsla = green.into();
        return Color::hsl(
            hsl_c.hue,
            hsl_c.saturation,
            (hsl_c.lightness * 1.15).min(1.0),
        );
    }
    match button.color {
        UiColor::Default => hsl(0.0, 0.0, 30.0),
        color => color.base(),
    }
}

/// `--imagebutton-transparecy`.
const TRANSPARENCY: f32 = 0.25;

fn with_lightness(color: Color, delta: f32, alpha: f32) -> Color {
    let mut c: bevy::color::Hsla = theme::lighten(color, delta).into();
    c.alpha = alpha;
    c.into()
}

/// (background, border, caption background, caption top border) for a state.
pub fn image_button_colors(button: &ImageButton, hovered: bool) -> [Color; 4] {
    let color = base_color(button);
    let a = theme::ADJUST_COLOR;
    let interactive = hovered && !button.disabled;
    let bg = if interactive {
        with_lightness(color, -a + theme::ADJUST_HOVER, TRANSPARENCY)
    } else {
        with_lightness(color, -a * 2.0, TRANSPARENCY)
    };
    let border = with_lightness(color, a, TRANSPARENCY);
    let caption = with_lightness(color, -theme::ADJUST_HOVER, 1.0);
    [bg, border, caption, color]
}

fn image_button(props: ImageButtonProps) -> impl Scene {
    let size = if props.image_size > 0.0 {
        props.image_size
    } else {
        64.0
    };
    let fluid = props.fluid;
    let image = props.image;
    let tooltip = props.tooltip;
    let tooltip_position = props.tooltip_position;
    let radius = theme::RADIUS_MEDIUM;
    let tiny = theme::BORDER_TINY;
    // `calc(${size}px + 0.5em + 2px)`.
    let container_width = if fluid {
        Val::Auto
    } else {
        px(size + theme::HALF_UNIT + 2.0)
    };
    let image_padding = if fluid { 0.0 } else { theme::SPACE_S };
    let image_margin = if fluid {
        UiRect::horizontal(Val::Auto)
    } else {
        UiRect::ZERO
    };
    let image_border = if fluid {
        UiRect::ZERO
    } else {
        UiRect {
            left: px(tiny),
            right: px(tiny),
            top: px(tiny),
            bottom: Val::ZERO,
        }
    };
    let image_radius = if fluid {
        BorderRadius::ZERO
    } else {
        BorderRadius::top(px(radius))
    };
    let (direction, root_margin, root_border) = if fluid {
        (
            FlexDirection::Row,
            UiRect::bottom(px(theme::SPACE_SM)),
            UiRect::all(px(tiny)),
        )
    } else {
        (
            FlexDirection::Column,
            UiRect::all(px(theme::SPACE_S)),
            UiRect::ZERO,
        )
    };
    let content: Box<dyn Scene> = if fluid {
        // Fluid caption: `flex: 1`, column, centered, `margin: 0 0.5em`, normal text color.
        Box::new(bsn! {
            Node {
                flex_grow: 1.0,
                flex_direction: FlexDirection::Column,
                justify_content: bevy::prelude::JustifyContent::Center,
                margin: UiRect::horizontal(px(theme::HALF_UNIT)),
                min_width: px(0),
            }
        })
    } else {
        // Tile caption: one line, `padding: 0.25em 0.33em`, opaque, `border-top: 1px`.
        Box::new(bsn! {
            Node {
                padding: UiRect::axes(px(4.0), px(3.0)),
                border: UiRect::top(px(tiny)),
                overflow: Overflow::clip(),
                min_width: px(0),
            }
            BackgroundColor(Color::NONE)
            BorderColor::all(Color::NONE)
            Propagate::<TextColor>(TextColor(theme::COLOR_TEXT))
        })
    };
    let buttons = props.buttons.map(|b| {
        let place: Box<dyn Scene> = if fluid {
            Box::new(bsn! { Node { align_self: AlignSelf::Stretch } })
        } else {
            Box::new(bsn! {
                Node {
                    position_type: PositionType::Absolute,
                    left: px(tiny),
                    bottom: px(theme::UNIT * 1.8),
                }
            })
        };
        bsn! {
            Node { flex_direction: FlexDirection::Row, overflow: Overflow::clip() }
            Part
            @{place}
            Children [ {b} ]
        }
    });
    let buttons_alt = props.buttons_alt.map(|b| {
        let place: Box<dyn Scene> = if fluid {
            Box::new(bsn! { Node { align_self: AlignSelf::Stretch } })
        } else {
            Box::new(bsn! {
                Node {
                    position_type: PositionType::Absolute,
                    top: px(tiny),
                    left: px(tiny),
                    width: px(size + theme::HALF_UNIT),
                }
                bevy::prelude::Pickable::IGNORE
            })
        };
        bsn! {
            Node { flex_direction: FlexDirection::Column }
            Part
            @{place}
            Children [ {b} ]
        }
    });
    (
        bsn! {
            Node {
                flex_direction: direction,
                margin: root_margin,
                border: root_border,
                border_radius: BorderRadius::all(px(radius)),
                overflow: Overflow::clip(),
                align_items: AlignItems::Stretch,
            }
            bevy::ui_widgets::Button
            HasSlot
            Hovered
            TabIndex(0)
            TrailingMargin(root_margin)
            BackgroundColor(Color::NONE)
            BorderColor::all(Color::NONE)
            Propagate::<TextColor>(TextColor(theme::COLOR_TEXT))
            Children [
                // `.ImageButton__container`.
                Node {
                    flex_direction: direction,
                    flex_grow: 1.0,
                    width: container_width,
                    min_width: px(0),
                }
                Part
                bevy::prelude::Pickable::IGNORE
                Children [
                    // `.ImageButton__image`.
                    Node {
                        align_self: AlignSelf::Center,
                        margin: image_margin,
                        padding: UiRect::all(px(image_padding)),
                        border: image_border,
                        border_radius: image_radius,
                        overflow: Overflow::clip(),
                        flex_shrink: 0.0,
                    }
                    BorderColor::all(Color::NONE)
                    bevy::prelude::Pickable::IGNORE
                    Children [
                        Node { width: px(size), height: px(size) }
                        bevy::prelude::Pickable::IGNORE
                        Children [ {image} ]
                    ]
                    --
                    ImageButtonContent
                    Slot
                    Part
                    bevy::prelude::Pickable::IGNORE
                    @{content}
                ]
                --
                Node {
                    position_type: PositionType::Absolute,
                    left: px(0),
                    right: px(0),
                    top: px(0),
                    bottom: px(0),
                    display: Display::None,
                }
                DisabledVeil
                Part
                BackgroundColor(hsla(0.0, 0.0, 10.0, 0.5))
                bevy::prelude::Pickable::IGNORE
            ]
        },
        if fluid {
            Box::new(bsn! { Fluid Node { align_self: AlignSelf::Stretch } }) as Box<dyn Scene>
        } else {
            Box::new(bsn! { Inline })
        },
        buttons.map(|b| bsn! { Children [ @{b} ] }),
        buttons_alt.map(|b| bsn! { Children [ @{b} ] }),
        (!tooltip.is_empty()).then(|| {
            bsn! { Tooltip { content: tooltip, position: tooltip_position } }
        }),
    )
}

fn style_image_buttons(
    mut buttons: Query<
        (
            Entity,
            &ImageButton,
            &Hovered,
            Has<InteractionDisabled>,
            Has<Fluid>,
            &mut BackgroundColor,
            &mut BorderColor,
            &Children,
        ),
        Or<(Changed<ImageButton>, Changed<Hovered>)>,
    >,
    children: Query<&Children>,
    mut contents: Query<
        (&mut BackgroundColor, &mut BorderColor),
        (
            With<ImageButtonContent>,
            bevy::prelude::Without<ImageButton>,
        ),
    >,
    mut images: Query<
        &mut BorderColor,
        (
            bevy::prelude::Without<ImageButtonContent>,
            bevy::prelude::Without<ImageButton>,
        ),
    >,
    mut veils: Query<&mut Node, With<DisabledVeil>>,
    mut commands: Commands,
) {
    for (entity, button, hovered, has_disabled, fluid, mut bg, mut border, kids) in &mut buttons {
        if button.disabled != has_disabled {
            if button.disabled {
                commands.entity(entity).insert(InteractionDisabled);
            } else {
                commands.entity(entity).remove::<InteractionDisabled>();
            }
        }
        let [bg_color, border_color, caption, caption_border] =
            image_button_colors(button, hovered.get());
        bg.set_if_neq(BackgroundColor(bg_color));
        border.set_if_neq(BorderColor::all(border_color));
        for &kid in kids {
            if let Ok(mut veil) = veils.get_mut(kid) {
                let display = if button.disabled {
                    Display::Flex
                } else {
                    Display::None
                };
                if veil.display != display {
                    veil.display = display;
                }
            }
            let Ok(container) = children.get(kid) else {
                continue;
            };
            for &part in container {
                if let Ok((mut cbg, mut cborder)) = contents.get_mut(part) {
                    if !fluid {
                        cbg.set_if_neq(BackgroundColor(caption));
                        cborder.set_if_neq(BorderColor::all(caption_border));
                    }
                } else if let Ok(mut image_border) = images.get_mut(part) {
                    // `border-color: inherit` from the root.
                    image_border.set_if_neq(BorderColor::all(border_color));
                }
            }
        }
    }
}

pub(crate) fn plugin(app: &mut App) {
    app.add_systems(PostUpdate, style_image_buttons.before(UiSystems::Layout));
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::color::Alpha;

    #[test]
    fn default_colors_follow_the_scss() {
        let [bg, border, caption, top] = image_button_colors(&ImageButton::default(), false);
        let l = |c: Color| bevy::color::Hsla::from(c).lightness;
        assert!((l(bg) - 0.20).abs() < 1e-3 && (bg.alpha() - 0.25).abs() < 1e-4);
        assert!((l(border) - 0.35).abs() < 1e-3);
        assert!((l(caption) - 0.20).abs() < 1e-3 && caption.alpha() == 1.0);
        assert!((l(top) - 0.30).abs() < 1e-3);
        let [hover, ..] = image_button_colors(&ImageButton::default(), true);
        assert!((l(hover) - 0.35).abs() < 1e-3);
    }
}
