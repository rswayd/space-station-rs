//! The stat panel's place in the chat column, after the BYOND skin (`interface/skin.dmf`
//! `info_child`: `statwindow` above `outputwindow`, `splitter = 40`, the split saved): a dock at
//! the top of the panel, a draggable divider under it, and the top bar's toggle (tg
//! `toggle_stat_panel`, the `statpanel` preference: hidden, the chat takes the whole column).
//!
//! The dock is an empty node marked [`StatDock`]; the game puts the stat panel's window in it.

use bevy::picking::events::{PointerDrag, PointerDragEnd};
use bevy::picking::hover::Hovered;
use ss13_ui_kit::prelude::*;
use ss13_ui_kit::theme;

use super::panel::ChatPanel;
use super::prefs::{self, STAT_SPLIT_MAX, STAT_SPLIT_MIN};

/// The dock of the stat panel's window.
#[derive(Component, Clone, Debug, Default)]
pub struct StatDock;

/// The divider between the dock and the chat.
#[derive(Component, Clone, Debug, Default)]
pub struct StatDivider;

/// The top bar's toggle.
#[derive(Component, Clone, Debug, Default)]
pub struct StatToggle;

/// The top bar action of [`StatToggle`].
pub const TOGGLE_ACT: &str = "stat_dock";

const DIVIDER_PX: f32 = 5.0;

pub(crate) fn plugin(app: &mut App) {
    app.add_systems(Update, sync)
        .add_observer(toggle)
        .add_observer(drag)
        .add_observer(drag_end);
}

/// The dock, put at the top of the panel by [`super::panel`].
pub(crate) fn dock() -> impl Scene {
    bsn! {
        Node {
            flex_shrink: 0.0,
            height: percent(40),
            min_height: px(0),
            flex_direction: FlexDirection::Column,
            overflow: Overflow::clip(),
        }
        StatDock
    }
}

/// The divider under the dock.
pub(crate) fn divider() -> impl Scene {
    let divider = theme::COLOR_BORDER_DARK;
    bsn! {
        Node { flex_shrink: 0.0, height: px(DIVIDER_PX) }
        BackgroundColor(divider)
        StatDivider
        // The kit's bottom edge: its resize cursor (the kit's window drag skips it, it is no
        // window's edge).
        ss13_ui_kit::window::ResizeEdge::Bottom
        Hovered
    }
}

/// The dock follows the settings: its share of the column, shown or hidden.
fn sync(
    prefs: Res<prefs::ChatPrefs>,
    mut docks: Query<&mut Node, (With<StatDock>, Without<StatDivider>)>,
    mut dividers: Query<&mut Node, (With<StatDivider>, Without<StatDock>)>,
    mut toggles: Query<&mut Button, With<StatToggle>>,
    added: Query<(), Added<StatDock>>,
) {
    if !prefs.is_changed() && added.is_empty() {
        return;
    }
    let display = if prefs.stat_shown {
        Display::Flex
    } else {
        Display::None
    };
    for mut node in &mut docks {
        node.display = display;
        node.height = percent(prefs.stat_split * 100.0);
    }
    for mut node in &mut dividers {
        node.display = display;
    }
    for mut button in &mut toggles {
        if button.selected != prefs.stat_shown {
            button.selected = prefs.stat_shown;
        }
    }
}

fn toggle(act: On<UiAct>, panels: Query<(), With<ChatPanel>>, mut prefs: ResMut<prefs::ChatPrefs>) {
    if act.action == TOGGLE_ACT && panels.contains(act.scope) {
        prefs.stat_shown = !prefs.stat_shown;
    }
}

/// Dragging the divider moves the dock's edge; the node changes as it moves, the setting once
/// the drag ends (a settings change rewraps nothing, but is written to the file).
fn drag(
    mut drag: On<PointerDrag>,
    dividers: Query<(), With<StatDivider>>,
    panels: Query<&ComputedNode, With<ChatPanel>>,
    mut docks: Query<(&mut Node, &ComputedNode), With<StatDock>>,
) {
    if !dividers.contains(drag.entity) {
        return;
    }
    drag.propagate(false);
    let Ok(panel) = panels.single() else {
        return;
    };
    let column = panel.size.y * panel.inverse_scale_factor;
    let Ok((mut node, computed)) = docks.single_mut() else {
        return;
    };
    if column <= 0.0 {
        return;
    }
    let height = computed.size.y * computed.inverse_scale_factor;
    let share = match node.height {
        Val::Percent(p) => p / 100.0,
        _ => height / column,
    };
    let share = (share + drag.delta.y / column).clamp(STAT_SPLIT_MIN, STAT_SPLIT_MAX);
    node.height = percent(share * 100.0);
}

fn drag_end(
    end: On<PointerDragEnd>,
    dividers: Query<(), With<StatDivider>>,
    docks: Query<&Node, With<StatDock>>,
    mut prefs: ResMut<prefs::ChatPrefs>,
) {
    if !dividers.contains(end.entity) {
        return;
    }
    if let Ok(Node {
        height: Val::Percent(p),
        ..
    }) = docks.single()
    {
        let share = p / 100.0;
        if (share - prefs.stat_split).abs() > f32::EPSILON {
            prefs.stat_split = share;
        }
    }
}
