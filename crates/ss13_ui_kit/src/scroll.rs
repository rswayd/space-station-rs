//! Scrollable areas behind `Section scrollable`, `Window.Content scrollable`, dropdown menus and
//! `VirtualList`: `bevy_ui_widgets`' `ScrollArea` (mouse wheel) and `Scrollbar` (track paging and
//! a draggable thumb), drawn like the browser scrollbar tgui styles with `scrollbar-color`.

use bevy::camera::visibility::Visibility;
use bevy::ecs::change_detection::DetectChangesMut;
use bevy::prelude::{
    App, BackgroundColor, Children, Component, Display, FlexDirection, IntoScheduleConfigs, Node,
    Overflow, PostUpdate, Query, Vec2, px,
};
use bevy::scene::{Scene, bsn};
use bevy::ui::{BorderRadius, ComputedNode};
use bevy::ui_widgets::{ControlOrientation, ScrollArea, Scrollbar, ScrollbarThumb};

use crate::layout::Part;
use crate::theme;

/// Track width. Chromium's classic scrollbar is 15-17 px; tgui windows are small, and with
/// `scrollbar-color` the thumb fills the track, so a slightly thinner track reads the same.
pub const SCROLLBAR_WIDTH: f32 = 12.0;
const MIN_THUMB_LENGTH: f32 = 18.0;

/// The node whose content scrolls (it owns the `ScrollPosition`).
#[derive(Component, Clone, Debug, Default)]
pub struct ScrollContent;

/// The scrollbar track of a [`ScrollContent`]. `always`: `overflow-y: scroll` (always shown);
/// otherwise `auto` (shown only while the content overflows).
#[derive(Component, Clone, Debug, Default)]
pub struct ScrollTrack {
    pub always: bool,
}

/// Children of a node that becomes a vertical scroll container: the scrolling content (patched
/// with `content`, e.g. padding and the widget's `Slot`) and the track beside it. The caller's
/// node must be a row (`scroll_row()`).
pub(crate) fn scroll_children(content: impl Scene, always: bool) -> impl Scene {
    bsn! {
        Children [
            #scroll_content
            Node {
                flex_grow: 1.0,
                min_width: px(0),
                min_height: px(0),
                flex_direction: FlexDirection::Column,
                overflow: Overflow::scroll_y(),
            }
            ScrollArea
            ScrollContent
            @{content}
            --
            Node {
                width: px(SCROLLBAR_WIDTH),
                flex_shrink: 0.0,
            }
            ScrollTrack { always: always }
            Part
            Scrollbar {
                target: #scroll_content,
                orientation: ControlOrientation::Vertical,
                min_thumb_length: MIN_THUMB_LENGTH,
            }
            BackgroundColor(theme::COLOR_SCROLLBAR_BASE)
            Children [
                ScrollbarThumb { border_radius: BorderRadius::all(px(theme::RADIUS_TINY)) }
                BackgroundColor(theme::COLOR_SCROLLBAR_THUMB)
            ]
        ]
    }
}

/// The node patch that hosts [`scroll_children`].
pub(crate) fn scroll_row() -> impl Scene {
    bsn! {
        Node {
            flex_direction: FlexDirection::Row,
            min_height: px(0),
            overflow: Overflow::clip(),
        }
    }
}

/// `overflow: auto`: the track only takes room while the content overflows.
// shortcut: decided from this frame's layout, so the content reflows one frame after it starts
// or stops overflowing; invisible at human editing speed.
fn auto_hide_tracks(
    mut tracks: Query<(&Scrollbar, &ScrollTrack, &mut Node, &mut Visibility)>,
    areas: Query<&ComputedNode>,
) {
    for (scrollbar, track, mut node, mut visibility) in &mut tracks {
        let Ok(area) = areas.get(scrollbar.target) else {
            continue;
        };
        let visible = (area.size() - area.scrollbar_size).max(Vec2::ZERO);
        let shown = track.always || area.content_size().y > visible.y + 0.5;
        let display = if shown { Display::Flex } else { Display::None };
        if node.display != display {
            node.display = display;
        }
        // The thumb is placed outside taffy, so `display: none` alone would leave it drawn.
        visibility.set_if_neq(if shown {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
}

pub(crate) fn plugin(app: &mut App) {
    app.add_systems(PostUpdate, auto_hide_tracks.in_set(crate::LayoutFeedback));
}
