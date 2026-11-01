//! tgui `Tabs` and `Tabs.Tab` (named [`TabsTab`]).
//!
//! A tab is a button: put an [`Act`](crate::Act) on it and clicking sends `act()` to the window,
//! the way `onClick={() => act('tab', ...)}` does. Local `useState` tabs are the same act handled
//! by the window's own observer.

use bevy::app::Propagate;
use bevy::ecs::change_detection::DetectChangesMut;
use bevy::input_focus::tab_navigation::TabIndex;
use bevy::picking::hover::Hovered;
use bevy::prelude::{
    AlignItems, App, BackgroundColor, BorderColor, BorderRadius, Changed, ChildOf, Children, Color,
    Component, Entity, FlexDirection, Has, IntoScheduleConfigs, JustifyContent, Node, Or, Overflow,
    PostUpdate, Query, UiRect, With, percent, px,
};
use bevy::scene::{Scene, SceneComponent, bsn};
use bevy::text::TextColor;
use bevy::ui::UiSystems;
use bevy::ui::widget::Text;

use crate::button::TrailingMargin;
use crate::icon::Icon;
use crate::section::{Section, SectionFitted};
use crate::theme::{self, UiColor, lighten};

/// tgui `Tabs`.
#[derive(SceneComponent, Clone, Debug, Default)]
#[scene(tabs(TabsProps))]
pub struct Tabs;

#[derive(Default)]
pub struct TabsProps {
    pub vertical: bool,
    /// `fluid`: tabs share the full width.
    pub fluid: bool,
    /// `fill`: full parent height.
    pub fill: bool,
    // TODO: `scrollable` (arrow buttons for overflowing horizontal tabs).
}

/// tgui `Tabs.Tab`. Fields are live state, like [`crate::Button`].
#[derive(SceneComponent, Clone, Debug, Default, PartialEq)]
#[scene(tab(TabProps))]
pub struct TabsTab {
    pub selected: bool,
    pub color: UiColor,
}

#[derive(Default)]
pub struct TabProps {
    /// Text (`children`); `Children` work as well.
    pub content: String,
    pub icon: String,
}

/// `--tab-indicator-size`.
const INDICATOR: f32 = theme::BORDER_SMALL;

fn tabs(props: TabsProps) -> impl Scene {
    let (direction, padding) = if props.vertical {
        let s = px(theme::SPACE_S);
        (
            FlexDirection::Column,
            UiRect {
                left: s,
                top: s,
                bottom: s,
                right: px(0),
            },
        )
    } else {
        let s = px(theme::SPACE_S);
        (
            FlexDirection::Row,
            UiRect {
                left: s,
                right: s,
                top: s,
                bottom: px(0),
            },
        )
    };
    (
        bsn! {
            Node {
                flex_direction: direction,
                align_items: AlignItems::Stretch,
                padding: padding,
                overflow: Overflow::clip(),
            }
            BackgroundColor(theme::COLOR_SECTION)
            TabsLayout { vertical: {props.vertical}, fluid: {props.fluid} }
        },
        (!props.vertical).then_some(bsn! {
            TrailingMargin(UiRect::bottom(px(theme::SPACE_M)))
        }),
        props
            .fill
            .then_some(bsn! { Node { height: percent(100), flex_grow: 1.0 } }),
    )
}

/// How the tabs of a [`Tabs`] lay out.
#[derive(Component, Clone, Debug, Default)]
pub struct TabsLayout {
    pub vertical: bool,
    pub fluid: bool,
}

fn tab(props: TabProps) -> impl Scene {
    let has_icon = !props.icon.is_empty();
    let icon = props.icon;
    let content = props.content;
    let radius = px(theme::RADIUS_SMALL);
    (
        bsn! {
            Node {
                min_width: px(4.0 * theme::UNIT),
                min_height: px(2.25 * theme::UNIT),
                flex_grow: 0.0,
                flex_shrink: 0.0,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::SpaceBetween,
                border: UiRect::bottom(px(INDICATOR)),
                border_radius: BorderRadius { top_left: radius, top_right: radius, ..BorderRadius::DEFAULT },
            }
            bevy::ui_widgets::Button
            Hovered
            TabIndex(0)
            BackgroundColor(Color::NONE)
            BorderColor::all(Color::NONE)
            Propagate::<TextColor>(TextColor(theme::COLOR_TEXT_TRANSLUCENT))
        },
        has_icon.then(|| {
            bsn! {
                Children [
                    // `.Tab__left`.
                    Node {
                        min_width: px(1.5 * theme::UNIT),
                        margin: UiRect::left(px(theme::SPACE_S)),
                        justify_content: JustifyContent::Center,
                    }
                    Children [ @Icon { @name: icon } ]
                ]
            }
        }),
        bsn! {
            Children [
                // `.Tab__text`.
                Node {
                    flex_grow: 1.0,
                    margin: UiRect::horizontal(px(theme::SPACE_M)),
                }
                Children [ Text(content) ]
            ]
        },
    )
}

/// `--tab-color-selected`: primary hue at 90% lightness; a colored tab: color + 17.5.
fn selected_text(color: UiColor) -> Color {
    if color == UiColor::Default {
        Color::hsl(210.0, 0.375, 0.9)
    } else {
        lighten(color.base(), 17.5)
    }
}

/// `--tab-background-hover` / `-selected`: the action colors at 0.75 lightness.
fn darker(color: Color) -> Color {
    let mut c = bevy::color::Hsla::from(color);
    c.lightness *= 0.75;
    c.into()
}

fn style_tabs(
    mut tabs: Query<
        (
            &TabsTab,
            &Hovered,
            &mut BackgroundColor,
            &mut BorderColor,
            &mut Propagate<TextColor>,
        ),
        Or<(Changed<TabsTab>, Changed<Hovered>)>,
    >,
) {
    for (tab, hovered, mut bg, mut border, mut text) in &mut tabs {
        let (background, color) = if tab.selected {
            (darker(theme::COLOR_SELECTED), selected_text(tab.color))
        } else if hovered.get() {
            (darker(theme::COLOR_HOVER), theme::COLOR_TEXT_TRANSLUCENT)
        } else {
            (Color::NONE, theme::COLOR_TEXT_TRANSLUCENT)
        };
        // The indicator bar is `currentColor`, scaled to zero on unselected tabs.
        let indicator = if tab.selected { color } else { Color::NONE };
        bg.set_if_neq(BackgroundColor(background));
        border.set_if_neq(BorderColor::all(indicator));
        text.set_if_neq(Propagate(TextColor(color)));
    }
}

/// Applies the container's direction to its tabs (`Tabs--vertical .Tab`, `Tabs--fluid`), and
/// the `.Section .Tabs` interop: no background and edge-to-edge inside a padded section.
fn layout_tabs(
    containers: Query<(Entity, &TabsLayout, &Children, &ChildOf), Changed<Children>>,
    parents: Query<&ChildOf>,
    sections: Query<Has<SectionFitted>, With<Section>>,
    siblings: Query<&Children>,
    mut nodes: Query<&mut Node>,
    mut backgrounds: Query<&mut BackgroundColor>,
    tab_entities: Query<(), With<TabsTab>>,
) {
    for (container, layout, children, child_of) in &containers {
        for &tab in children.iter().filter(|e| tab_entities.contains(**e)) {
            let Ok(mut node) = nodes.get_mut(tab) else {
                continue;
            };
            node.flex_grow = if layout.fluid { 1.0 } else { 0.0 };
            if layout.vertical {
                let radius = px(theme::RADIUS_SMALL);
                node.min_height = px(2.0 * theme::UNIT);
                node.border = UiRect::right(px(INDICATOR));
                node.border_radius = BorderRadius {
                    top_left: radius.into(),
                    bottom_left: radius.into(),
                    ..BorderRadius::DEFAULT
                };
            }
        }
        let section = parents
            .iter_ancestors(container)
            .find_map(|e| sections.get(e).ok());
        let Some(fitted) = section else {
            continue;
        };
        if let Ok(mut bg) = backgrounds.get_mut(container) {
            bg.set_if_neq(BackgroundColor(Color::NONE));
        }
        if fitted {
            continue;
        }
        let first = siblings
            .get(child_of.parent())
            .is_ok_and(|c| c.first() == Some(&container));
        if let Ok(mut node) = nodes.get_mut(container) {
            let m = px(-theme::SPACE_M);
            node.margin.left = m;
            node.margin.right = m;
            if first {
                node.margin.top = m;
            }
        }
    }
}

pub(crate) fn plugin(app: &mut App) {
    app.add_systems(
        PostUpdate,
        (style_tabs, layout_tabs).before(UiSystems::Layout),
    );
}
