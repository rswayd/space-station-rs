//! tgui `Window` (title bar, close, drag, resize, z-order, focus) and `Window.Content`.
//!
//! A tgui window is a native BYOND window; here it is a root UI node floating over the game
//! view. Focus reuses `bevy_input_focus`: the window root carries `TabIndex(-1)`, so a click on
//! any non-focusable part of it focuses the window through the standard `AcquireFocus` bubbling.

use bevy::app::Propagate;
use bevy::color::Alpha;
use bevy::ecs::change_detection::DetectChangesMut;
use bevy::ecs::lifecycle::Add;
use bevy::input_focus::tab_navigation::{TabGroup, TabIndex};
use bevy::picking::events::{PointerDrag, PointerPress};
use bevy::picking::hover::Hovered;
use bevy::prelude::{
    AlignItems, App, BackgroundColor, BackgroundGradient, BorderColor, BoxShadow, Changed, ChildOf,
    Children, Color, ColorStop, Commands, Component, Entity, EntityEvent, FlexDirection,
    GlobalZIndex, JustifyContent, LinearGradient, Local, Node, On, Overflow, PositionType,
    PostUpdate, Query, Res, ResMut, Resource, UiRect, UiScale, Val, With, ZIndex, percent, px,
};
use bevy::scene::{Scene, SceneComponent, bsn};
use bevy::text::TextColor;
use bevy::ui::widget::Text;
use bevy::ui_widgets::Activate;
use bevy::window::{CursorIcon, SystemCursorIcon};

use crate::button::ActScope;
use crate::icon::Icon;
use crate::layout::{HasSlot, Part, Slot, block};
use crate::scroll::{scroll_children, scroll_row};
use crate::theme::{self, FontPx, fade, root_text_style};

/// tgui `Window`. User children land in the area under the title bar; put a
/// [`WindowContent`] there, as in JSX.
#[derive(SceneComponent, Clone, Debug, Default)]
#[scene(window(WindowProps))]
pub struct Window;

pub struct WindowProps {
    pub title: String,
    pub width: f32,
    pub height: f32,
    /// Initial position of the top-left corner, in logical px.
    pub left: f32,
    pub top: f32,
    pub can_close: bool,
    pub resizable: bool,
}

impl Default for WindowProps {
    fn default() -> Self {
        Self {
            title: String::new(),
            width: 400.0,
            height: 300.0,
            left: 50.0,
            top: 50.0,
            can_close: true,
            resizable: true,
        }
    }
}

/// tgui `Window.Content`.
#[derive(SceneComponent, Clone, Debug, Default)]
#[scene(window_content(WindowContentProps))]
pub struct WindowContent;

#[derive(Default)]
pub struct WindowContentProps {
    /// `fitted`: no padding.
    pub fitted: bool,
    /// `scrollable`: `overflow-y: scroll`, so the track is always shown, as in tgui.
    pub scrollable: bool,
}

/// The draggable title bar.
#[derive(Component, Clone, Debug, Default)]
pub struct TitleBar;

/// The bottom-right resize grip.
#[derive(Component, Clone, Debug, Default)]
pub struct ResizeHandle;

/// The right (`width`) or bottom (`height`) resize edge, tgui `Window__resizeHandle__e`/`__s`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ResizeEdge {
    #[default]
    Right,
    Bottom,
}

/// Thickness of the edge handles, logical px.
const EDGE_PX: f32 = 6.0;
/// Side of the corner grip.
const GRIP_PX: f32 = 20.0;
/// Above the title bar and the content of its window (a local z, unlike the window's global
/// one: a `GlobalZIndex(0)` here put the grip under every raised window, out of the pointer's
/// reach).
const HANDLE_Z: i32 = 10;

/// The title bar close button.
#[derive(Component, Clone, Debug, Default)]
pub struct CloseButton;

/// The close button was pressed. Triggered on the window right before it is despawned, so the
/// owner can tell the server (tgui sends `close`).
#[derive(EntityEvent, Clone, Debug)]
pub struct WindowClose {
    pub entity: Entity,
}

/// Global z-order: the last raised window is on top.
#[derive(Resource, Default)]
pub struct WindowOrder {
    pub next: i32,
    pub top: Option<Entity>,
}

const MIN_WIDTH: f32 = 150.0;
const MIN_HEIGHT: f32 = 60.0;

fn window(props: WindowProps) -> impl Scene {
    let title = props.title;
    let gradient = BackgroundGradient::from(LinearGradient::to_bottom(vec![
        ColorStop::auto(theme::COLOR_BASE_START),
        ColorStop::auto(theme::COLOR_BASE_END),
    ]));
    let shadow = BoxShadow::new(Color::BLACK.with_alpha(0.6), px(0), px(2), px(0), px(8));
    (
        bsn! {
            Node {
                position_type: PositionType::Absolute,
                left: px(props.left),
                top: px(props.top),
                width: px(props.width),
                height: px(props.height),
                flex_direction: FlexDirection::Column,
            }
            Window
            ActScope
            HasSlot
            GlobalZIndex(0)
            TabIndex(-1)
            TabGroup::new(0)
            BackgroundColor(theme::COLOR_BASE)
            gradient
            shadow
            @root_text_style()
            Children [
                @title_bar(title, props.can_close)
                --
                Node {
                    flex_grow: 1.0,
                    min_height: px(0),
                    flex_direction: FlexDirection::Column,
                    overflow: Overflow::clip(),
                }
                Part
                Slot
            ]
        },
        props.resizable.then(|| {
            bsn! {
                Children [
                    Node {
                        position_type: PositionType::Absolute,
                        right: px(0),
                        top: px(0),
                        bottom: px(GRIP_PX),
                        width: px(EDGE_PX),
                    }
                    Part
                    ResizeEdge::Right
                    Hovered
                    ZIndex(HANDLE_Z)
                    --
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(0),
                        right: px(GRIP_PX),
                        bottom: px(0),
                        height: px(EDGE_PX),
                    }
                    Part
                    ResizeEdge::Bottom
                    Hovered
                    ZIndex(HANDLE_Z)
                    --
                    Node {
                        position_type: PositionType::Absolute,
                        right: px(0),
                        bottom: px(0),
                        width: px(GRIP_PX),
                        height: px(GRIP_PX),
                    }
                    Part
                    ResizeHandle
                    Hovered
                    ZIndex(HANDLE_Z)
                ]
            }
        }),
    )
}

fn title_bar(title: String, can_close: bool) -> impl Scene {
    let border = UiRect::bottom(px(theme::BORDER_TINY));
    let close = can_close.then(|| {
        bsn! {
            Children [
                Node {
                    width: px(theme::TITLEBAR_BUTTON_WIDTH),
                    height: percent(100),
                    flex_shrink: 0.0,
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                }
                CloseButton
                bevy::ui_widgets::Button
                Hovered
                BackgroundColor(Color::NONE)
                Propagate::<TextColor>(TextColor(fade(theme::COLOR_TEXT, 0.5)))
                Propagate::<FontPx>(FontPx(16.0))
                Children [ @Icon { @name: "times" } ]
            ]
        }
    });
    (
        bsn! {
            Node {
                height: px(theme::TITLEBAR_HEIGHT),
                flex_shrink: 0.0,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                border: border,
            }
            Part
            TitleBar
            BackgroundColor(theme::COLOR_SECONDARY)
            BorderColor::all(theme::COLOR_BORDER_DARK)
            Children [
                // `.TitleBar__statusIcon`: "tools" at 0.5 opacity while no status is known.
                Node {
                    width: px(theme::TITLEBAR_BUTTON_WIDTH),
                    flex_shrink: 0.0,
                    justify_content: JustifyContent::Center,
                }
                Propagate::<TextColor>(TextColor(fade(theme::COLOR_TEXT, 0.5)))
                Propagate::<FontPx>(FontPx(16.0))
                Children [ @Icon { @name: "tools" } ]
                --
                Node {
                    flex_grow: 1.0,
                    overflow: Overflow::clip(),
                }
                Propagate::<TextColor>(TextColor(theme::COLOR_TEXT_TRANSLUCENT_LIGHT))
                Propagate::<FontPx>(FontPx(theme::FONT_SIZE_TITLE))
                Children [ Text(title) ]
            ]
        },
        close,
    )
}

fn window_content(props: WindowContentProps) -> impl Scene {
    let padding = if props.fitted {
        UiRect::ZERO
    } else {
        UiRect::all(px(theme::SPACE_M))
    };
    if props.scrollable {
        return Box::new(bsn! {
            @scroll_row()
            Node { flex_grow: 1.0, flex_basis: px(0) }
            HasSlot
            @scroll_children(bsn! { @block() Node { padding: padding } Part Slot }, true)
        }) as Box<dyn Scene>;
    }
    Box::new((
        block(),
        bsn! {
            Node {
                flex_grow: 1.0,
                min_height: px(0),
                padding: padding,
            }
        },
    ))
}

fn raise(window: Entity, order: &mut WindowOrder, z: &mut Query<&mut GlobalZIndex, With<Window>>) {
    if order.top == Some(window) {
        return;
    }
    if let Ok(mut z) = z.get_mut(window) {
        order.next += 1;
        z.0 = order.next;
        order.top = Some(window);
    }
}

fn raise_new_window(
    add: On<Add<Window>>,
    mut order: ResMut<WindowOrder>,
    mut z: Query<&mut GlobalZIndex, With<Window>>,
) {
    raise(add.entity, &mut order, &mut z);
}

fn raise_on_press(
    press: On<PointerPress>,
    parents: Query<&ChildOf>,
    mut order: ResMut<WindowOrder>,
    mut z: Query<&mut GlobalZIndex, With<Window>>,
) {
    // `PointerPress` bubbles and global observers see every step; handle the leaf only, since
    // widgets (buttons) stop propagation before it reaches the window.
    if press.entity != press.original_event_target() {
        return;
    }
    let window = std::iter::once(press.entity)
        .chain(parents.iter_ancestors(press.entity))
        .find(|e| z.contains(*e));
    if let Some(window) = window {
        raise(window, &mut order, &mut z);
    }
}

fn drag_window(
    mut drag: On<PointerDrag>,
    bars: Query<(), With<TitleBar>>,
    grips: Query<(), With<ResizeHandle>>,
    edges: Query<&ResizeEdge>,
    widgets: Query<(), With<bevy::ui_widgets::Button>>,
    parents: Query<&ChildOf>,
    mut windows: Query<&mut Node, With<Window>>,
    scale: Res<UiScale>,
) {
    let target = drag.entity;
    let is_bar = bars.contains(target);
    let edge = edges.get(target).ok().copied();
    if !is_bar && !grips.contains(target) && edge.is_none() {
        return;
    }
    drag.propagate(false);
    if widgets.contains(drag.original_event_target()) {
        return;
    }
    let Ok(window) = parents.get(target).map(ChildOf::parent) else {
        return;
    };
    let Ok(mut node) = windows.get_mut(window) else {
        return;
    };
    let delta = drag.delta / scale.0;
    let shift = |v: Val, d: f32, min: f32| match v {
        Val::Px(x) => Val::Px((x + d).max(min)),
        other => other,
    };
    if is_bar {
        node.left = shift(node.left, delta.x, f32::MIN);
        node.top = shift(node.top, delta.y, f32::MIN);
        return;
    }
    if edge != Some(ResizeEdge::Bottom) {
        node.width = shift(node.width, delta.x, MIN_WIDTH);
    }
    if edge != Some(ResizeEdge::Right) {
        node.height = shift(node.height, delta.y, MIN_HEIGHT);
    }
}

fn close_window(
    activate: On<Activate>,
    close_buttons: Query<(), With<CloseButton>>,
    parents: Query<&ChildOf>,
    windows: Query<(), With<Window>>,
    mut commands: Commands,
) {
    if !close_buttons.contains(activate.entity) {
        return;
    }
    let Some(window) = parents
        .iter_ancestors(activate.entity)
        .find(|e| windows.contains(*e))
    else {
        return;
    };
    commands.trigger(WindowClose { entity: window });
    commands.entity(window).despawn();
}

/// `.TitleBar__close:hover`: full opacity on a danger background.
fn style_close_buttons(
    mut buttons: Query<
        (&Hovered, &mut BackgroundColor, &mut Propagate<TextColor>),
        (With<CloseButton>, Changed<Hovered>),
    >,
) {
    for (hovered, mut bg, mut text) in &mut buttons {
        let (b, t) = if hovered.get() {
            (crate::theme::UiColor::Danger.base(), theme::COLOR_TEXT)
        } else {
            (Color::NONE, fade(theme::COLOR_TEXT, 0.5))
        };
        bg.set_if_neq(BackgroundColor(b));
        text.set_if_neq(Propagate(TextColor(t)));
    }
}

/// The resize cursor over a window's edges and grip (tgui's CSS `cursor`), the default elsewhere.
fn resize_cursor(
    grips: Query<&Hovered, With<ResizeHandle>>,
    edges: Query<(&Hovered, &ResizeEdge)>,
    screens: Query<Entity, With<bevy::window::PrimaryWindow>>,
    mut shown: Local<Option<SystemCursorIcon>>,
    mut commands: Commands,
) {
    let wanted = if grips.iter().any(Hovered::get) {
        Some(SystemCursorIcon::SeResize)
    } else {
        edges
            .iter()
            .find(|(h, _)| h.get())
            .map(|(_, edge)| match edge {
                ResizeEdge::Right => SystemCursorIcon::EwResize,
                ResizeEdge::Bottom => SystemCursorIcon::NsResize,
            })
    };
    if *shown == wanted {
        return;
    }
    let Ok(screen) = screens.single() else {
        return;
    };
    *shown = wanted;
    let icon = wanted.unwrap_or_default();
    commands.entity(screen).insert(CursorIcon::System(icon));
}

pub(crate) fn plugin(app: &mut App) {
    app.init_resource::<WindowOrder>()
        .add_observer(raise_new_window)
        .add_observer(raise_on_press)
        .add_observer(drag_window)
        .add_observer(close_window)
        .add_systems(PostUpdate, (style_close_buttons, resize_cursor));
}
