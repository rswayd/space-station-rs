//! Floating boxes anchored to another node (tooltips, dropdown menus), placed like tgui's
//! floating-ui setup: a side and alignment, an offset, `flip` and `shift` to stay on screen.
//!
//! An overlay is a root UI node, not a child of its anchor: a child would be clipped by every
//! scrolling or `overflow: hidden` ancestor of the anchor, while tgui portals floats to the
//! document root.

use bevy::camera::visibility::Visibility;
use bevy::ecs::change_detection::DetectChangesMut;
use bevy::math::{Rect, Vec2};
use bevy::prelude::{App, Component, Entity, IntoScheduleConfigs, Node, PostUpdate, Query, Val};
use bevy::ui::{ComputedNode, ComputedUiRenderTargetInfo, UiGlobalTransform};

/// GlobalZIndex of open dropdown menus: above every window.
pub const MENU_Z: i32 = i32::MAX - 2;
/// GlobalZIndex of tooltips: above menus.
pub const TOOLTIP_Z: i32 = i32::MAX - 1;

/// floating-ui `Placement` (tgui `position="top-start"`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Placement {
    Top,
    TopStart,
    TopEnd,
    #[default]
    Bottom,
    BottomStart,
    BottomEnd,
    Left,
    LeftStart,
    LeftEnd,
    Right,
    RightStart,
    RightEnd,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Side {
    Top,
    Bottom,
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Align {
    Center,
    Start,
    End,
}

impl Placement {
    fn split(self) -> (Side, Align) {
        use Placement::*;
        match self {
            Top => (Side::Top, Align::Center),
            TopStart => (Side::Top, Align::Start),
            TopEnd => (Side::Top, Align::End),
            Bottom => (Side::Bottom, Align::Center),
            BottomStart => (Side::Bottom, Align::Start),
            BottomEnd => (Side::Bottom, Align::End),
            Left => (Side::Left, Align::Center),
            LeftStart => (Side::Left, Align::Start),
            LeftEnd => (Side::Left, Align::End),
            Right => (Side::Right, Align::Center),
            RightStart => (Side::Right, Align::Start),
            RightEnd => (Side::Right, Align::End),
        }
    }
}

/// floating-ui `flip({ padding: 6 })`.
const FLIP_PADDING: f32 = 6.0;

fn side_rect(anchor: Rect, size: Vec2, side: Side, align: Align, offset: f32) -> Vec2 {
    let cross = |a_min: f32, a_max: f32, len: f32| match align {
        Align::Start => a_min,
        Align::End => a_max - len,
        Align::Center => (a_min + a_max - len) / 2.0,
    };
    match side {
        Side::Top => Vec2::new(
            cross(anchor.min.x, anchor.max.x, size.x),
            anchor.min.y - offset - size.y,
        ),
        Side::Bottom => Vec2::new(
            cross(anchor.min.x, anchor.max.x, size.x),
            anchor.max.y + offset,
        ),
        Side::Left => Vec2::new(
            anchor.min.x - offset - size.x,
            cross(anchor.min.y, anchor.max.y, size.y),
        ),
        Side::Right => Vec2::new(
            anchor.max.x + offset,
            cross(anchor.min.y, anchor.max.y, size.y),
        ),
    }
}

fn fits(pos: Vec2, size: Vec2, side: Side, viewport: Rect) -> bool {
    let p = FLIP_PADDING;
    match side {
        Side::Top => pos.y >= viewport.min.y + p,
        Side::Bottom => pos.y + size.y <= viewport.max.y - p,
        Side::Left => pos.x >= viewport.min.x + p,
        Side::Right => pos.x + size.x <= viewport.max.x - p,
    }
}

/// Top-left corner of a floating box of `size` next to `anchor`, all in logical px.
pub fn place(anchor: Rect, size: Vec2, viewport: Rect, placement: Placement, offset: f32) -> Vec2 {
    let (side, align) = placement.split();
    let mut pos = side_rect(anchor, size, side, align, offset);
    if !fits(pos, size, side, viewport) {
        let opposite = match side {
            Side::Top => Side::Bottom,
            Side::Bottom => Side::Top,
            Side::Left => Side::Right,
            Side::Right => Side::Left,
        };
        let flipped = side_rect(anchor, size, opposite, align, offset);
        if fits(flipped, size, opposite, viewport) {
            pos = flipped;
        }
    }
    // `shift()`: slide along the cross axis to stay inside the viewport.
    let max = (viewport.max - size).max(viewport.min);
    match side {
        Side::Top | Side::Bottom => pos.x = pos.x.clamp(viewport.min.x, max.x),
        Side::Left | Side::Right => pos.y = pos.y.clamp(viewport.min.y, max.y),
    }
    pos
}

/// Keeps a root overlay node next to `anchor`. Hidden until its first placement, since its own
/// size is only known after a layout pass.
#[derive(Component, Clone, Debug)]
#[require(Visibility::Hidden)]
pub struct Anchored {
    pub anchor: Entity,
    pub placement: Placement,
    /// Gap between anchor and overlay, logical px (floating-ui `offset`).
    pub offset: f32,
    /// `contentAutoWidth`: the overlay is as wide as the anchor.
    pub match_width: bool,
}

/// Logical-px rect of a laid out node.
pub fn node_rect(node: &ComputedNode, transform: &UiGlobalTransform) -> Rect {
    let scale = node.inverse_scale_factor;
    Rect::from_center_size(transform.translation * scale, node.size * scale)
}

// shortcut: reads this frame's layout and moves the overlay for the next one, so an overlay
// trails a dragged window by a frame. Upgrade to patching `UiGlobalTransform` after layout
// (as `bevy_ui_widgets::popover` does) if that lag is ever visible.
fn position_overlays(
    mut overlays: Query<(
        &Anchored,
        &mut Node,
        &mut Visibility,
        &ComputedNode,
        &ComputedUiRenderTargetInfo,
    )>,
    anchors: Query<(&ComputedNode, &UiGlobalTransform)>,
) {
    for (anchored, mut node, mut visibility, computed, target) in &mut overlays {
        let Ok((anchor_node, anchor_transform)) = anchors.get(anchored.anchor) else {
            continue;
        };
        let anchor = node_rect(anchor_node, anchor_transform);
        let scale = computed.inverse_scale_factor;
        let mut size = computed.size * scale;
        if anchored.match_width {
            size.x = anchor.width();
            let width = Val::Px(anchor.width());
            if node.width != width {
                node.width = width;
            }
        }
        let viewport = Rect::from_corners(Vec2::ZERO, target.physical_size().as_vec2() * scale);
        let pos = place(anchor, size, viewport, anchored.placement, anchored.offset);
        let (left, top) = (Val::Px(pos.x.round()), Val::Px(pos.y.round()));
        if node.left != left || node.top != top {
            node.left = left;
            node.top = top;
        }
        // Shown once a real size has been measured; the first layout has none.
        if computed.size != Vec2::ZERO {
            visibility.set_if_neq(Visibility::Inherited);
        }
    }
}

pub(crate) fn plugin(app: &mut App) {
    app.add_systems(PostUpdate, position_overlays.in_set(crate::LayoutFeedback));
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIEW: Rect = Rect {
        min: Vec2::ZERO,
        max: Vec2::new(800.0, 600.0),
    };

    #[test]
    fn bottom_is_centered_below() {
        let anchor = Rect::new(100.0, 100.0, 200.0, 120.0);
        let pos = place(anchor, Vec2::new(50.0, 20.0), VIEW, Placement::Bottom, 6.0);
        assert_eq!(pos, Vec2::new(125.0, 126.0));
    }

    #[test]
    fn flips_when_no_room_and_shifts_into_view() {
        // Top placement at the top edge flips below; a wide box at the left edge is shifted.
        let anchor = Rect::new(0.0, 0.0, 20.0, 20.0);
        let pos = place(anchor, Vec2::new(100.0, 30.0), VIEW, Placement::Top, 6.0);
        assert_eq!(pos, Vec2::new(0.0, 26.0));
    }
}
