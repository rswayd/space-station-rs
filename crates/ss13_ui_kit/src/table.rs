//! tgui `Table`, `Table.Row` ([`TableRow`]), `Table.Cell` ([`TableCell`]).
//!
//! `bevy_ui` has neither `display: table` nor subgrid/`display: contents`, so a row is a flex
//! row and column widths are synced across rows after layout, approximating the HTML automatic
//! table layout:
//! - a column is as wide as its widest cell content (max-content);
//! - spare width of a full-width table is shared out in proportion to those widths, except to
//!   `collapsing` cells (`width: 1%`), which keep their content width.
//!
//! Gaps: no `colSpan`; a table narrower than its contents overflows instead of wrapping cell
//! text down to min-content; widths settle one frame after content changes.

use std::collections::HashSet;

use bevy::app::Propagate;
use bevy::prelude::{
    AlignItems, App, ChildOf, Children, Component, Entity, FlexDirection, IntoScheduleConfigs,
    Node, PostUpdate, Query, UiRect, Val, With, Without, percent, px,
};
use bevy::scene::{Scene, SceneComponent, bsn};
use bevy::text::Justify;
use bevy::ui::ComputedNode;

use crate::layout::{Block, text_props};
use crate::theme::{self, Bold, UiColor};

/// tgui `Table`.
#[derive(SceneComponent, Clone, Debug, Default)]
#[scene(table(TableProps))]
pub struct Table;

#[derive(Default)]
pub struct TableProps {
    /// `collapsing`: as wide as the content instead of 100%.
    pub collapsing: bool,
}

/// tgui `Table.Row`.
#[derive(SceneComponent, Clone, Debug, Default)]
#[scene(table_row(TableRowProps))]
pub struct TableRow;

#[derive(Default)]
pub struct TableRowProps {
    /// `header`: bold cells with extra bottom padding.
    pub header: bool,
}

/// tgui `Table.Cell`. User children (or `content`) are the cell content.
#[derive(SceneComponent, Clone, Debug, Default)]
#[scene(table_cell(TableCellProps))]
pub struct TableCell;

#[derive(Default)]
pub struct TableCellProps {
    pub content: String,
    pub header: bool,
    /// `collapsing`: the column takes only its content width.
    pub collapsing: bool,
    pub text_align: Option<Justify>,
    pub color: UiColor,
    pub bold: bool,
}

/// Column-sizing inputs of a table.
#[derive(Component, Clone, Debug, Default)]
pub struct TableLayout {
    pub collapsing: bool,
}

/// A header row (`Table.Row header`).
#[derive(Component, Clone, Debug, Default)]
pub struct HeaderRow;

/// Per-cell sizing flags.
#[derive(Component, Clone, Debug, Default)]
pub struct CellLayout {
    pub collapsing: bool,
}

fn table(props: TableProps) -> impl Scene {
    let width = if props.collapsing {
        Val::Auto
    } else {
        percent(100)
    };
    let align = if props.collapsing {
        bevy::prelude::AlignSelf::FlexStart
    } else {
        bevy::prelude::AlignSelf::Stretch
    };
    bsn! {
        Node {
            width: width,
            align_self: align,
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Stretch,
        }
        TableLayout { collapsing: {props.collapsing} }
    }
}

fn table_row(props: TableRowProps) -> impl Scene {
    (
        bsn! {
            // `vertical-align: middle` is the UA default for table rows.
            Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
            }
        },
        props
            .header
            .then_some(bsn! { HeaderRow Propagate::<Bold>(Bold(true)) }),
    )
}

fn table_cell(props: TableCellProps) -> impl Scene {
    // Children hug their content (so they can be measured) and are placed by `text-align`.
    let align = match props.text_align {
        Some(Justify::Center) => AlignItems::Center,
        Some(Justify::Right | Justify::End) => AlignItems::FlexEnd,
        _ => AlignItems::FlexStart,
    };
    let content = (!props.content.is_empty()).then(|| {
        let content = props.content;
        bsn! { Children [ bevy::ui::widget::Text(content) ] }
    });
    let bottom = if props.header { theme::SPACE_M } else { 0.0 };
    (
        bsn! {
            Node {
                flex_direction: FlexDirection::Column,
                align_items: align,
                flex_shrink: 0.0,
                padding: UiRect {
                    left: px(theme::SPACE_S),
                    right: px(theme::SPACE_S),
                    top: px(0),
                    bottom: px(bottom),
                },
            }
            Block
            CellLayout { collapsing: {props.collapsing} }
        },
        text_props(
            props.color,
            props.bold || props.header,
            false,
            None,
            props.text_align,
        ),
        content,
    )
}

/// Logical width of a node plus its px horizontal margins.
fn outer_width(node: &ComputedNode, style: Option<&Node>) -> f32 {
    let margin = |v: Val| if let Val::Px(x) = v { x } else { 0.0 };
    let m = style.map_or(0.0, |s| margin(s.margin.left) + margin(s.margin.right));
    node.size.x * node.inverse_scale_factor + m
}

/// Syncs column widths of every table whose cell content changed size.
// shortcut: reads this frame's layout and applies next frame, like `LabeledList` labels.
fn size_columns(
    changed: Query<&ChildOf, bevy::prelude::Changed<ComputedNode>>,
    cells: Query<(&ChildOf, Option<&Children>, &CellLayout)>,
    rows: Query<(&ChildOf, &Children, bevy::prelude::Has<HeaderRow>), With<TableRow>>,
    tables: Query<(&Children, &TableLayout)>,
    content: Query<(&ComputedNode, Option<&Node>), Without<CellLayout>>,
    mut cell_nodes: Query<&mut Node, With<CellLayout>>,
) {
    let mut dirty: HashSet<Entity> = HashSet::new();
    for parent in &changed {
        if let Ok((row, ..)) = cells.get(parent.parent())
            && let Ok((table, ..)) = rows.get(row.parent())
        {
            dirty.insert(table.parent());
        }
    }
    for table in dirty {
        let Ok((table_rows, layout)) = tables.get(table) else {
            continue;
        };
        let row_cells = || {
            table_rows
                .iter()
                .filter_map(|r| rows.get(*r).ok())
                .map(|(_, cells, header)| (cells, header))
        };
        let mut widths: Vec<f32> = Vec::new();
        let mut collapsing: Vec<bool> = Vec::new();
        for (row, _) in row_cells() {
            let count = row.len();
            for (i, &cell) in row.iter().enumerate() {
                let Ok((_, children, flags)) = cells.get(cell) else {
                    continue;
                };
                // `.Table__cell:first-child { padding-left: 0 }`, `:last-child` right.
                let pad_left = if i == 0 { 0.0 } else { theme::SPACE_S };
                let pad_right = if i + 1 == count { 0.0 } else { theme::SPACE_S };
                let inner = children
                    .into_iter()
                    .flatten()
                    .filter_map(|c| content.get(*c).ok())
                    .map(|(n, style)| outer_width(n, style))
                    .fold(0.0_f32, f32::max);
                if widths.len() <= i {
                    widths.resize(i + 1, 0.0);
                    collapsing.resize(i + 1, false);
                }
                widths[i] = widths[i].max((inner + pad_left + pad_right).ceil());
                collapsing[i] |= flags.collapsing;
            }
        }
        for (row, header) in row_cells() {
            let count = row.len();
            for (i, &cell) in row.iter().enumerate() {
                let Ok(mut node) = cell_nodes.get_mut(cell) else {
                    continue;
                };
                let width = px(widths[i]);
                let grow = if layout.collapsing || collapsing[i] {
                    0.0
                } else {
                    widths[i].max(1.0)
                };
                let pad_left = if i == 0 { px(0) } else { px(theme::SPACE_S) };
                let pad_right = if i + 1 == count {
                    px(0)
                } else {
                    px(theme::SPACE_S)
                };
                if node.min_width != width
                    || node.flex_grow != grow
                    || node.padding.left != pad_left
                    || node.padding.right != pad_right
                {
                    node.min_width = width;
                    node.flex_grow = grow;
                    node.padding.left = pad_left;
                    node.padding.right = pad_right;
                }
                if header && node.padding.bottom != px(theme::SPACE_M) {
                    node.padding.bottom = px(theme::SPACE_M);
                }
            }
        }
    }
}

pub(crate) fn plugin(app: &mut App) {
    app.add_systems(PostUpdate, size_columns.in_set(crate::LayoutFeedback));
}
