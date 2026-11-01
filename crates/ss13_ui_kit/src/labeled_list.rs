//! tgui `LabeledList`, `LabeledList.Item`, `LabeledList.Divider`.
//!
//! tgui renders a `<table>`, so every label shares the width of the widest one. `bevy_ui` has
//! no table or subgrid layout; labels are equalized by a system after layout instead.

use bevy::app::Propagate;
use bevy::prelude::{
    AlignItems, App, Changed, Children, Component, Entity, FlexDirection, FlexWrap,
    IntoScheduleConfigs, JustifyContent, Node, PostUpdate, Query, UiRect, Val, With, px,
};
use bevy::scene::{Scene, SceneComponent, SceneList, bsn};
use bevy::text::{Justify, TextColor};
use bevy::ui::ComputedNode;
use bevy::ui::widget::Text;

use crate::layout::{Divider, HasSlot, Part, Slot, block};
use crate::theme::{self, TextAlign, UiColor};
use crate::tooltip::Tooltip;

/// tgui `LabeledList`.
#[derive(SceneComponent, Clone, Debug, Default)]
#[scene(labeled_list)]
pub struct LabeledList;

/// tgui `LabeledList.Item`. User children are the value cell.
#[derive(SceneComponent, Clone, Debug, Default)]
#[scene(labeled_list_item(LabeledListItemProps))]
pub struct LabeledListItem;

#[derive(Default)]
pub struct LabeledListItemProps {
    /// A trailing `:` is added, as tgui does for string labels.
    pub label: String,
    /// Defaults to `label`.
    pub label_color: UiColor,
    /// Value cell text color.
    pub color: UiColor,
    pub text_align: Option<Justify>,
    /// Right-aligned buttons column.
    pub buttons: Option<Box<dyn SceneList>>,
    /// Value cell text (`content="..."`); children work as well.
    pub content: String,
    /// Hover text on the label.
    pub tooltip: String,
    // TODO: `labelWrap`, `verticalAlign`.
}

/// tgui `LabeledList.Divider`.
#[derive(SceneComponent, Clone, Debug, Default)]
#[scene(labeled_list_divider(LabeledListDividerProps))]
pub struct LabeledListDivider;

#[derive(Default)]
pub struct LabeledListDividerProps {
    /// Extra vertical space, in units of 12px (`size - 1` on each side).
    pub size: f32,
}

/// The label cell of an item; its width is synced across the list.
#[derive(Component, Clone, Debug, Default)]
pub struct LabelCell;

/// `.LabeledList__label--nowrap { min-width: 5em }`.
const LABEL_MIN_WIDTH: f32 = 60.0;

fn cell_padding() -> UiRect {
    UiRect::axes(px(theme::SPACE_M), px(theme::SPACE_S))
}

fn labeled_list() -> impl Scene {
    let m = px(-theme::SPACE_M);
    (
        block(),
        bsn! {
            Node {
                margin: UiRect { left: m, right: m, top: px(-theme::SPACE_S), bottom: px(0) },
            }
        },
    )
}

fn labeled_list_item(props: LabeledListItemProps) -> impl Scene {
    let label = if props.label.is_empty() {
        String::new()
    } else {
        format!("{}:", props.label)
    };
    let label_color = if props.label_color == UiColor::Default {
        UiColor::Label
    } else {
        props.label_color
    }
    .text()
    .unwrap_or(theme::COLOR_TEXT);
    let value = (!props.content.is_empty()).then(|| {
        let content = props.content;
        bsn! { Children [ Text(content) ] }
    });
    let value_color = props
        .color
        .text()
        .map(|c| bsn! { Propagate::<TextColor>(TextColor(c)) });
    let value_align = props
        .text_align
        .map(|a| bsn! { Propagate::<TextAlign>(TextAlign(a)) });
    let tooltip = (!props.tooltip.is_empty()).then(|| {
        let content = props.tooltip;
        bsn! { Tooltip { content: content } }
    });
    let buttons = props.buttons.map(|buttons| {
        bsn! {
            Children [
                Node {
                    padding: UiRect { left: px(theme::SPACE_M), right: px(theme::SPACE_M), top: px(theme::SPACE_XXS), bottom: px(0) },
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::FlexEnd,
                    align_items: AlignItems::Center,
                    flex_shrink: 0.0,
                }
                Part
                Children [ {buttons} ]
            ]
        }
    });
    (
        bsn! {
            Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Baseline,
            }
            LabeledListItem
            HasSlot
            Children [
                Node {
                    padding: {cell_padding()},
                    min_width: px(LABEL_MIN_WIDTH),
                    flex_shrink: 0.0,
                }
                Part
                LabelCell
                @{tooltip}
                Propagate::<TextColor>(TextColor(label_color))
                Children [ Text(label) ]
                --
                Node {
                    padding: {cell_padding()},
                    flex_grow: 1.0,
                    flex_direction: FlexDirection::Row,
                    flex_wrap: FlexWrap::Wrap,
                    align_items: AlignItems::Baseline,
                }
                Part
                Slot
                @{value}
                @{value_color}
                @{value_align}
            ]
        },
        buttons,
    )
}

fn labeled_list_divider(props: LabeledListDividerProps) -> impl Scene {
    let pad = px((props.size - 1.0).max(0.0) * theme::UNIT);
    bsn! {
        @block()
        Node { padding: UiRect::vertical(pad) }
        Children [ @Divider ]
    }
}

/// Gives every label cell of a list the width of the widest label text, like a table column.
// shortcut: reads last frame's layout, so a label change settles one frame later; fine for UI
// that changes at human speed.
fn equalize_labels(
    changed: Query<(), (With<Text>, Changed<ComputedNode>)>,
    lists: Query<&Children, With<LabeledList>>,
    items: Query<&Children, With<LabeledListItem>>,
    mut cells: Query<(&Children, &mut Node), With<LabelCell>>,
    texts: Query<&ComputedNode, With<Text>>,
) {
    if changed.is_empty() {
        return;
    }
    let pad = 2.0 * theme::SPACE_M;
    for list in &lists {
        let label_cells = || {
            list.iter()
                .filter_map(|item| items.get(*item).ok())
                .flat_map(|cells| cells.iter())
        };
        let widest = label_cells()
            .filter_map(|cell| cells.get(*cell).ok())
            .flat_map(|(texts_in_cell, _)| texts_in_cell.iter())
            .filter_map(|t| texts.get(*t).ok())
            .map(|n| n.size.x * n.inverse_scale_factor + pad)
            .fold(LABEL_MIN_WIDTH, f32::max);
        let width = Val::Px(widest.ceil());
        let ids: Vec<Entity> = label_cells().copied().collect();
        for cell in ids {
            if let Ok((_, mut node)) = cells.get_mut(cell)
                && node.min_width != width
            {
                node.min_width = width;
            }
        }
    }
}

pub(crate) fn plugin(app: &mut App) {
    app.add_systems(PostUpdate, equalize_labels.in_set(crate::LayoutFeedback));
}
