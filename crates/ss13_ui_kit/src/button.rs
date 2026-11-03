//! tgui `Button`, plus the `act()` bridge: a clicked button with an [`Act`] triggers [`UiAct`] on
//! the nearest [`ActScope`] (the window), the way tgui buttons call `act(action)` on their
//! window's backend.

use bevy::app::Propagate;
use bevy::ecs::change_detection::DetectChangesMut;
use bevy::input_focus::tab_navigation::TabIndex;
use bevy::picking::hover::Hovered;
use bevy::prelude::{
    Added, AlignItems, AlignSelf, App, BackgroundColor, BorderRadius, Changed, ChildOf, Children,
    Color, Commands, Component, Entity, EntityEvent, FlexDirection, Has, IntoScheduleConfigs,
    JustifyContent, Node, On, Or, PostUpdate, Query, RemovedComponents, UiRect, Val, With, px,
};
use bevy::scene::{Scene, SceneComponent, bsn};
use bevy::text::{TextColor, TextLayout};
use bevy::ui::widget::Text;
use bevy::ui::{InteractionDisabled, Overflow, Pressed, UiSystems};
use bevy::ui_widgets::Activate;

use crate::icon::Icon;
use crate::layout::{Inline, InlineLine, Stack, StackItem};
use crate::overlay::Placement;
use crate::theme::{self, UiColor, fade, lighten};
use crate::tooltip::Tooltip;

/// tgui `Button`. Fields are live state: change them and the style follows. Structure (content,
/// icon, compact, ...) comes from the props.
#[derive(SceneComponent, Clone, Debug, Default, PartialEq)]
#[scene(button(ButtonProps))]
pub struct Button {
    pub color: UiColor,
    pub selected: bool,
    pub disabled: bool,
}

#[derive(Default)]
pub struct ButtonProps {
    /// Text content (`content="..."`); children work as well.
    pub content: String,
    /// Font Awesome icon name, as [`crate::Icon`] takes it.
    pub icon: String,
    pub compact: bool,
    /// `fluid`: full width.
    pub fluid: bool,
    pub circular: bool,
    /// `Button--ellipsis`: one line, the rest cut off instead of wrapping onto the next row.
    // shortcut: cut at the button's edge without the trailing "…" (Bevy text has no
    // `text-overflow`); pair it with a tooltip of the whole text, as tgui pairs it with `title`.
    pub ellipsis: bool,
    /// Hover text, see [`Tooltip`].
    pub tooltip: String,
    pub tooltip_position: Placement,
}

/// The action name sent when the button is clicked: tgui `onClick={() => act('power')}`.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct Act(pub String);

/// Where [`UiAct`] is delivered: the window, or any root that owns a tgui backend.
#[derive(Component, Clone, Debug, Default)]
pub struct ActScope;

/// A button with an [`Act`] was activated. Targets the nearest [`ActScope`] ancestor (or the
/// button itself if there is none), so game code observes one entity per window:
/// `commands.entity(window).observe(|act: On<UiAct>| ...)`.
#[derive(EntityEvent, Clone, Debug, PartialEq)]
pub struct UiAct {
    #[event_target]
    pub scope: Entity,
    /// The activated button.
    pub source: Entity,
    pub action: String,
    /// `act(action, params)`: string-valued, as `ss13_shared::protocol::ClientIntent::UiAct` sends.
    pub params: Vec<(String, String)>,
}

/// Fixed params sent with the [`Act`]: `act('vend', { ref: product.ref })`.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct ActParams(pub Vec<(String, String)>);

/// Margin that tgui's `:last-child` rules drop on the last sibling. Only the fields it sets are
/// touched, so `Node` margin patches on the other sides survive.
#[derive(Component, Clone, Debug, Default)]
pub struct TrailingMargin(pub UiRect);

fn button(props: ButtonProps) -> impl Scene {
    let has_icon = !props.icon.is_empty();
    let has_content = !props.content.is_empty();
    let height = if props.compact {
        theme::BUTTON_HEIGHT_COMPACT
    } else {
        theme::BUTTON_HEIGHT
    };
    let pad_x = if props.compact {
        theme::SPACE_S
    } else {
        theme::SPACE_M
    };
    // `.Button--hasIcon` drops the left padding; the icon carries its own margin.
    let padding = match (has_icon, has_content) {
        (true, false) => UiRect::ZERO,
        (true, true) => UiRect::right(px(pad_x)),
        _ => UiRect::horizontal(px(pad_x)),
    };
    let radius = if props.circular {
        BorderRadius::MAX
    } else {
        BorderRadius::all(px(theme::RADIUS_TINY))
    };
    let trailing = UiRect {
        right: px(theme::SPACE_XS),
        bottom: px(theme::SPACE_XS),
        ..UiRect::DEFAULT
    };
    let icon_margin = if props.compact {
        theme::SPACE_XXS
    } else {
        theme::SPACE_S
    };
    let content = props.content;
    let icon = props.icon;
    let tooltip = props.tooltip;
    let tooltip_position = props.tooltip_position;
    (
        bsn! {
            Node {
                min_height: px(height),
                padding: padding,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                align_self: AlignSelf::FlexStart,
                border_radius: radius,
            }
            bevy::ui_widgets::Button
            Hovered
            TabIndex(0)
            TrailingMargin(trailing)
            BackgroundColor(Color::NONE)
            Propagate::<TextColor>(TextColor(theme::COLOR_TEXT))
        },
        has_icon.then(|| {
            bsn! {
                Children [
                    Node {
                        min_width: px(16),
                        margin: UiRect::horizontal(px(icon_margin)),
                        justify_content: JustifyContent::Center,
                    }
                    Children [ @Icon { @name: icon } ]
                ]
            }
        }),
        (has_content && !props.ellipsis).then(|| {
            let content = content.clone();
            bsn! { Children [ Text(content) ] }
        }),
        (has_content && props.ellipsis).then(|| {
            let one_line = TextLayout::no_wrap();
            bsn! {
                Node { min_width: px(0), overflow: Overflow::clip() }
                Children [ Text(content) one_line ]
            }
        }),
        // `.Button--fluid` is `display: block`; any other button is inline-block.
        if props.fluid {
            Box::new(bsn! {
                Node {
                    align_self: AlignSelf::Stretch,
                    width: Val::Auto,
                }
            }) as Box<dyn Scene>
        } else {
            Box::new(bsn! { Inline })
        },
        (!tooltip.is_empty()).then(|| {
            bsn! { Tooltip { content: tooltip, position: tooltip_position } }
        }),
    )
}

/// Background and text color for a button state, per `Button.scss`.
pub fn button_colors(button: &Button, hovered: bool, pressed: bool) -> (Color, Color) {
    if button.color == UiColor::Transparent {
        let text = if button.disabled {
            UiColor::Bad.base()
        } else if button.selected {
            lighten(UiColor::Green.base(), theme::ADJUST_COLOR)
        } else if hovered {
            theme::COLOR_TEXT
        } else {
            theme::COLOR_TEXT_TRANSLUCENT
        };
        let bg = if button.disabled {
            fade(UiColor::Bad.base(), 0.2)
        } else if hovered && !pressed {
            fade(theme::COLOR_TEXT, 0.1)
        } else {
            Color::NONE
        };
        return (bg, text);
    }
    let color = if button.selected {
        UiColor::Green
    } else {
        button.color
    };
    let base = lighten(color.base(), -theme::ADJUST_COLOR);
    let text = if color.is_low_contrast() {
        UiColor::Black.base()
    } else {
        theme::COLOR_TEXT
    };
    if button.disabled {
        let o = theme::BUTTON_DISABLED_OPACITY;
        return (fade(base, o), fade(text, o));
    }
    let bg = if pressed {
        lighten(base, -theme::ADJUST_ACTIVE)
    } else if hovered {
        lighten(base, theme::ADJUST_HOVER)
    } else {
        base
    };
    (bg, text)
}

fn style_buttons(
    changed: Query<
        Entity,
        (
            With<Button>,
            Or<(Changed<Button>, Changed<Hovered>, Added<Pressed>)>,
        ),
    >,
    mut released: RemovedComponents<Pressed>,
    mut buttons: Query<(
        &Button,
        &Hovered,
        Has<Pressed>,
        Has<InteractionDisabled>,
        &mut BackgroundColor,
        &mut Propagate<TextColor>,
    )>,
    mut commands: Commands,
) {
    let dirty: Vec<Entity> = changed.iter().chain(released.read()).collect();
    for entity in dirty {
        let Ok((button, hovered, pressed, has_disabled, mut bg, mut text)) =
            buttons.get_mut(entity)
        else {
            continue;
        };
        if button.disabled != has_disabled {
            if button.disabled {
                commands.entity(entity).insert(InteractionDisabled);
            } else {
                commands.entity(entity).remove::<InteractionDisabled>();
            }
        }
        let (bg_color, text_color) = button_colors(button, hovered.get(), pressed);
        bg.set_if_neq(BackgroundColor(bg_color));
        text.set_if_neq(Propagate(TextColor(text_color)));
    }
}

fn trailing_margins(
    parents: Query<
        (
            &Children,
            Has<Stack>,
            Has<StackItem>,
            Has<InlineLine>,
            Option<&ChildOf>,
        ),
        Changed<Children>,
    >,
    stack_items: Query<(), With<StackItem>>,
    mut items: Query<(&TrailingMargin, &mut Node, Has<Button>)>,
) {
    for (children, in_stack, in_item, is_line, line_parent) in &parents {
        // A line box is transparent for the selector: its parent is the CSS parent.
        let in_item =
            in_item || (is_line && line_parent.is_some_and(|p| stack_items.contains(p.parent())));
        let last = children.last().copied();
        let children: Vec<Entity> = children.iter().copied().collect();
        for child in children {
            let Ok((trailing, mut node, is_button)) = items.get_mut(child) else {
                continue;
            };
            // `.Stack > .Button, .Stack__item > .Button { margin: 0 }`.
            let zero = Some(child) == last || (is_button && (in_stack || in_item));
            let mut margin = node.margin;
            for (side, wanted) in [
                (&mut margin.right, trailing.0.right),
                (&mut margin.bottom, trailing.0.bottom),
            ] {
                if wanted != Val::ZERO && wanted != Val::Auto {
                    *side = if zero { Val::ZERO } else { wanted };
                }
            }
            if node.margin != margin {
                node.margin = margin;
            }
        }
    }
}

fn emit_act(
    activate: On<Activate>,
    acts: Query<(&Act, Option<&ActParams>)>,
    parents: Query<&ChildOf>,
    scopes: Query<(), With<ActScope>>,
    mut commands: Commands,
) {
    let source = activate.entity;
    let Ok((act, params)) = acts.get(source) else {
        return;
    };
    let params = params.map(|p| p.0.clone()).unwrap_or_default();
    commands.trigger(act_event(source, act, params, &parents, &scopes));
}

/// The [`UiAct`] a widget with an [`Act`] sends, addressed to its nearest [`ActScope`].
pub(crate) fn act_event(
    source: Entity,
    act: &Act,
    params: Vec<(String, String)>,
    parents: &Query<&ChildOf>,
    scopes: &Query<(), With<ActScope>>,
) -> UiAct {
    let scope = parents
        .iter_ancestors(source)
        .find(|e| scopes.contains(*e))
        .unwrap_or(source);
    UiAct {
        scope,
        source,
        action: act.0.clone(),
        params,
    }
}

pub(crate) fn plugin(app: &mut App) {
    app.add_observer(emit_act).add_systems(
        PostUpdate,
        (style_buttons, trailing_margins).before(UiSystems::Layout),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::color::Alpha;

    #[test]
    fn disabled_button_is_half_transparent() {
        let b = Button {
            disabled: true,
            ..Default::default()
        };
        let (bg, text) = button_colors(&b, true, true);
        assert!((bg.alpha() - 0.5).abs() < 1e-4);
        assert!((text.alpha() - 0.5).abs() < 1e-4);
    }

    #[test]
    fn selected_overrides_color_with_green() {
        let b = Button {
            color: UiColor::Bad,
            selected: true,
            ..Default::default()
        };
        let (bg, _) = button_colors(&b, false, false);
        assert_eq!(bg, lighten(UiColor::Green.base(), -theme::ADJUST_COLOR));
    }

    #[test]
    fn caution_uses_black_text() {
        let b = Button {
            color: UiColor::Caution,
            ..Default::default()
        };
        assert_eq!(button_colors(&b, false, false).1, UiColor::Black.base());
    }
}
