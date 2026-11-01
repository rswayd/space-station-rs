//! `VirtualList`: a scrolling list of fixed-height rows where only the rows in view (plus a
//! small overscan) exist as entities. Chat and long lists use it.
//!
//! tgui-core's `VirtualList` grows its rendered prefix as the window fills; here rows are
//! windowed both ways, so 10k-row lists cost the same as 30-row ones: per frame only rows
//! entering or leaving the view are spawned or despawned.

use std::collections::BTreeMap;
use std::ops::Range;
use std::sync::Arc;

use bevy::prelude::{
    App, ChildOf, Children, Commands, Component, Entity, IntoScheduleConfigs, Node, PositionType,
    PostUpdate, Query, ScrollPosition, With, px,
};
use bevy::scene::{CommandsSceneExt, Scene, SceneComponent, bsn};
use bevy::ui::{ComputedNode, UiSystems};

use crate::scroll::{ScrollContent, scroll_children, scroll_row};

/// Builds the scene of row `index`.
#[derive(Clone)]
pub struct VirtualRow(pub Arc<dyn Fn(usize) -> Box<dyn Scene> + Send + Sync>);

impl VirtualRow {
    pub fn new<S: Scene>(build: impl Fn(usize) -> S + Send + Sync + 'static) -> Self {
        Self(Arc::new(move |i| Box::new(build(i)) as Box<dyn Scene>))
    }
}

impl Default for VirtualRow {
    fn default() -> Self {
        Self::new(|_| bsn! {})
    }
}

/// The list. `len` is live: raise it to append rows (chat), lower it to drop rows. Give it a
/// bounded height (`Node { height }`, `flex_grow` in a filled column), or it grows to its rows.
#[derive(SceneComponent, Clone, Debug, Default, PartialEq)]
#[scene(virtual_list(VirtualListProps))]
pub struct VirtualList {
    pub len: usize,
}

pub struct VirtualListProps {
    /// Every row is this tall, in logical px.
    pub item_height: f32,
    /// Rows kept alive beyond each edge of the view, so wheel steps reveal ready rows.
    pub overscan: usize,
    pub row: VirtualRow,
}

impl Default for VirtualListProps {
    fn default() -> Self {
        Self {
            item_height: 20.0,
            overscan: 4,
            row: VirtualRow::default(),
        }
    }
}

/// Rendering state of a [`VirtualList`].
#[derive(Component, Clone)]
pub struct VirtualListState {
    pub item_height: f32,
    pub overscan: usize,
    pub row: VirtualRow,
    /// Spawned rows by index.
    pub rendered: BTreeMap<usize, Entity>,
}

/// The tall node whose height is `len * item_height`; rows sit on it at absolute offsets.
#[derive(Component, Clone, Debug, Default)]
pub struct VirtualSpacer;

/// A spawned row wrapper and its index.
#[derive(Component, Clone, Debug, Default)]
pub struct VirtualListRow(pub usize);

fn virtual_list(props: VirtualListProps) -> impl Scene {
    let state = VirtualListState {
        item_height: props.item_height,
        overscan: props.overscan,
        row: props.row,
        rendered: BTreeMap::new(),
    };
    bsn! {
        @scroll_row()
        state
        @scroll_children(bsn! {
            Children [ Node { flex_shrink: 0.0 } VirtualSpacer ]
        }, false)
    }
}

/// Rows `[first, last)` that intersect the view widened by `overscan` rows on each side.
pub fn visible_rows(
    scroll: f32,
    view: f32,
    item_height: f32,
    overscan: usize,
    len: usize,
) -> Range<usize> {
    if item_height <= 0.0 || view <= 0.0 {
        return 0..0;
    }
    let first = ((scroll / item_height).floor().max(0.0) as usize).saturating_sub(overscan);
    let last = (((scroll + view) / item_height).ceil() as usize + overscan).min(len);
    first.min(last)..last
}

/// Spawns rows scrolled into view and despawns rows scrolled out. Runs before layout with the
/// current scroll position and last frame's view size, so new rows are laid out this frame.
fn update_virtual_lists(
    mut lists: Query<(&VirtualList, &mut VirtualListState, &Children)>,
    contents: Query<(&ComputedNode, &ScrollPosition, &Children), With<ScrollContent>>,
    mut spacers: Query<&mut Node, With<VirtualSpacer>>,
    mut commands: Commands,
) {
    for (list, mut state, children) in &mut lists {
        let Some((view, scroll, content_children)) =
            children.iter().find_map(|c| contents.get(*c).ok())
        else {
            continue;
        };
        let Some(&spacer) = content_children.first() else {
            continue;
        };
        let height = px(list.len as f32 * state.item_height);
        if let Ok(mut node) = spacers.get_mut(spacer)
            && node.height != height
        {
            node.height = height;
        }
        let view_height = view.size.y * view.inverse_scale_factor;
        let range = visible_rows(
            scroll.y,
            view_height,
            state.item_height,
            state.overscan,
            list.len,
        );
        let stale: Vec<usize> = state
            .rendered
            .keys()
            .copied()
            .filter(|i| !range.contains(i))
            .collect();
        for index in stale {
            if let Some(row) = state.rendered.remove(&index) {
                commands.entity(row).despawn();
            }
        }
        for index in range {
            if state.rendered.contains_key(&index) {
                continue;
            }
            let content = (state.row.0)(index);
            let top = px(index as f32 * state.item_height);
            let item_height = px(state.item_height);
            let row = commands
                .spawn_scene(bsn! {
                    Node {
                        position_type: PositionType::Absolute,
                        top: top,
                        left: px(0),
                        right: px(0),
                        height: item_height,
                    }
                    VirtualListRow(index)
                    Children [ @{content} ]
                })
                .insert(ChildOf(spacer))
                .id();
            state.rendered.insert(index, row);
        }
    }
}

pub(crate) fn plugin(app: &mut App) {
    app.add_systems(
        PostUpdate,
        update_virtual_lists.before(UiSystems::Propagate),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_covers_view_plus_overscan() {
        assert_eq!(visible_rows(0.0, 100.0, 20.0, 2, 10_000), 0..7);
        assert_eq!(visible_rows(1000.0, 100.0, 20.0, 2, 10_000), 48..57);
        assert_eq!(
            visible_rows(199_900.0, 100.0, 20.0, 2, 10_000),
            9993..10_000
        );
        assert_eq!(visible_rows(0.0, 0.0, 20.0, 2, 10_000), 0..0);
    }
}
