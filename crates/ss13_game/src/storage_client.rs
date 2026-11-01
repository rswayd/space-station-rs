//! Client half of the storage view: tg's storage screen (`/datum/storage_interface`) as a grid
//! of `storage_cell` slots above the inventory row, the contents' icons in order and the
//! `storage_close` button after the bottom row. Placement follows tg `update_position`
//! (`screen_start_x = 4`, `screen_start_y = 2`, 16 px offsets, rows grow upwards).
//!
//! It draws what the server sends ([`StorageData`] through the headless [`StorageUi`]); a click
//! on an item sends the `take` act, on an empty cell `insert`, dragging an item onto another
//! cell `move` (tg `mousedrop_receive`), and the close button closes the window (tg
//! `hide_contents`).

use bevy::picking::events::{PointerClick, PointerDragDrop};
use bevy::picking::pointer::PointerButton;
use bevy::prelude::*;
use ss13_net::client::SendIntent;
use ss13_net::ui::client::{SendUi, UiMessageReceived};
use ss13_render::atlas::frame_rect;
use ss13_render::{IconAtlas, MapInsets};
use ss13_shared::protocol::ClientIntent;
use ss13_shared::ui::{UiClientMessage, UiServerMessage};
use ss13_shared::{Dir, IconStateId};
use ss13_tgui::client::AppTguiClientExt;
use ss13_tgui::{UiInterface, decode};

use crate::hud::{HudIcon, StorageData, StorageUi, storage_grid};

/// One 32 px screen tile at the map's zoom 2.
const TILE_PX: f32 = 64.0;
const STYLE_DMI: &str = "icons/hud/screen_midnight.dmi";
/// tg `screen_start_x`/`screen_start_y` (1-based tiles) and `screen_pixel_x/y` (16 px, doubled).
const START_X: f32 = 3.0 * TILE_PX + 32.0;
const START_Y: f32 = TILE_PX + 32.0;

pub struct StorageClientPlugin;

impl Plugin for StorageClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_headless::<StorageUi>()
            .init_resource::<StoragePanel>()
            .add_systems(
                Update,
                (
                    receive_storage,
                    draw_storage.run_if(resource_changed::<StoragePanel>),
                    follow_map_insets.run_if(resource_changed::<MapInsets>),
                ),
            );
    }
}

/// The server's storage window and its last data.
#[derive(Resource, Default, Debug)]
pub struct StoragePanel {
    pub window: Option<u32>,
    pub data: StorageData,
}

#[derive(Component)]
struct StorageRoot;

/// A grid cell: its index in the contents order.
#[derive(Component, Clone, Copy)]
struct StorageCell(usize);

#[derive(Component)]
struct StorageClose;

fn receive_storage(
    mut received: MessageReader<UiMessageReceived>,
    mut panel: ResMut<StoragePanel>,
) {
    for UiMessageReceived(message) in received.read() {
        let data = match message {
            UiServerMessage::Open {
                window,
                interface,
                data,
                ..
            } if interface == StorageUi::NAME => {
                panel.window = Some(*window);
                Some(data)
            }
            UiServerMessage::Update {
                window,
                data: Some(data),
                ..
            } if panel.window == Some(*window) => Some(data),
            UiServerMessage::Close { window } if panel.window == Some(*window) => {
                *panel = StoragePanel::default();
                None
            }
            _ => None,
        };
        if let Some(data) = data.filter(|d| !d.is_empty()) {
            match decode::<StorageData>(data) {
                Ok(data) => panel.data = data,
                Err(e) => warn!("undecodable storage data: {e}"),
            }
        }
    }
}

fn image(atlas: &mut IconAtlas, icon_state: IconStateId, color: Color) -> Option<ImageNode> {
    let (info, image) = atlas.state(icon_state)?;
    Some(ImageNode {
        image: image.clone(),
        rect: frame_rect(info, Dir::South, 0),
        color,
        ..default()
    })
}

fn style_state(atlas: &IconAtlas, state: &str) -> Option<IconStateId> {
    atlas
        .manifest
        .states
        .iter()
        .position(|s| s.dmi == STYLE_DMI && s.state == state)
        .map(|i| IconStateId(i as u32))
}

/// Respawns the grid from the panel data (a handful of nodes, only when the data changes).
fn draw_storage(
    panel: Res<StoragePanel>,
    insets: Option<Res<MapInsets>>,
    mut atlas: Option<ResMut<IconAtlas>>,
    roots: Query<Entity, With<StorageRoot>>,
    mut commands: Commands,
) {
    for root in &roots {
        commands.entity(root).despawn();
    }
    if panel.window.is_none() {
        return;
    }
    let data = &panel.data;
    let (columns, rows) = storage_grid(usize::from(data.max_slots), data.items.len());
    let left = insets.map_or(0.0, |i| i.left);
    let root = commands
        .spawn((
            StorageRoot,
            Name::new("storage"),
            Node {
                position_type: PositionType::Absolute,
                left: px(left + START_X),
                bottom: px(START_Y),
                width: px((columns as f32 + 1.0) * TILE_PX),
                height: px(rows as f32 * TILE_PX),
                ..default()
            },
            Pickable::IGNORE,
        ))
        .id();
    let cell_state = atlas
        .as_deref()
        .and_then(|a| style_state(a, "storage_cell"));
    let close_state = atlas
        .as_deref()
        .and_then(|a| style_state(a, "storage_close"));
    let tile = |col: usize, row: usize| Node {
        position_type: PositionType::Absolute,
        left: px(col as f32 * TILE_PX),
        bottom: px(row as f32 * TILE_PX),
        width: px(TILE_PX),
        height: px(TILE_PX),
        ..default()
    };
    for i in 0..columns * rows {
        let (col, row) = (i % columns, i / columns);
        let mut cell = commands.spawn((
            StorageCell(i),
            Name::new(format!("storage slot {i}")),
            tile(col, row),
            ChildOf(root),
        ));
        let background = cell_state.and_then(|s| image(atlas.as_deref_mut()?, s, Color::WHITE));
        match background {
            Some(background) => cell.insert(background),
            None => cell.insert(BackgroundColor(Color::srgba(0.1, 0.1, 0.15, 0.8))),
        };
        cell.observe(click_cell).observe(drop_on_cell);
        let cell = cell.id();
        let item = data.items.get(i).and_then(|item| item.icon);
        if let Some(icon) = item.and_then(|icon| item_image(atlas.as_deref_mut()?, icon)) {
            commands.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: px(TILE_PX),
                    height: px(TILE_PX),
                    ..default()
                },
                icon,
                Pickable::IGNORE,
                ChildOf(cell),
            ));
        }
    }
    let mut close = commands.spawn((
        StorageClose,
        Name::new("storage close"),
        tile(columns, 0),
        ChildOf(root),
    ));
    match close_state.and_then(|s| image(atlas.as_deref_mut()?, s, Color::WHITE)) {
        Some(sprite) => close.insert(sprite),
        None => close.insert(BackgroundColor(Color::srgba(0.5, 0.1, 0.1, 0.8))),
    };
    close.observe(click_close);
}

fn item_image(atlas: &mut IconAtlas, icon: HudIcon) -> Option<ImageNode> {
    let color = icon
        .color
        .map_or(Color::WHITE, |[r, g, b, a]| Color::srgba_u8(r, g, b, a));
    image(atlas, icon.icon_state, color)
}

/// tg: clicking a stored item with an empty hand takes it (`attack_hand` of the item in the
/// storage); clicking a cell with an item in hand inserts it. Which one applies is decided by the
/// server; the client only says where it clicked.
fn click_cell(
    click: On<PointerClick>,
    cells: Query<&StorageCell>,
    panel: Res<StoragePanel>,
    mut intents: MessageWriter<SendIntent>,
) {
    if click.button != PointerButton::Primary {
        return;
    }
    let (Ok(&StorageCell(index)), Some(window)) = (cells.get(click.entity), panel.window) else {
        return;
    };
    let intent = if let Some(item) = panel.data.items.get(index) {
        ClientIntent::UiAct {
            window,
            action: "take".into(),
            params: vec![("key".into(), item.key.to_string())],
        }
    } else {
        ClientIntent::UiAct {
            window,
            action: "insert".into(),
            params: vec![],
        }
    };
    intents.write(SendIntent(intent));
}

/// tg `/datum/storage/proc/mousedrop_receive` (onto an item) and `storage/cell/
/// mouse_drop_receive` (onto a free cell): the dragged item moves to that place.
fn drop_on_cell(
    drop: On<PointerDragDrop>,
    cells: Query<&StorageCell>,
    panel: Res<StoragePanel>,
    mut intents: MessageWriter<SendIntent>,
) {
    if drop.button != PointerButton::Primary {
        return;
    }
    let (Ok(&StorageCell(to)), Ok(&StorageCell(from)), Some(window)) = (
        cells.get(drop.entity),
        cells.get(drop.dropped),
        panel.window,
    ) else {
        return;
    };
    let Some(item) = panel.data.items.get(from).filter(|_| from != to) else {
        return;
    };
    intents.write(SendIntent(ClientIntent::UiAct {
        window,
        action: "move".into(),
        params: vec![
            ("key".into(), item.key.to_string()),
            ("to".into(), to.to_string()),
        ],
    }));
}

fn click_close(
    click: On<PointerClick>,
    mut panel: ResMut<StoragePanel>,
    mut out: MessageWriter<SendUi>,
) {
    if click.button != PointerButton::Primary {
        return;
    }
    if let Some(window) = panel.window {
        out.write(SendUi(UiClientMessage::Close { window }));
        *panel = StoragePanel::default();
    }
}

fn follow_map_insets(insets: Res<MapInsets>, mut roots: Query<&mut Node, With<StorageRoot>>) {
    for mut node in &mut roots {
        node.left = px(insets.left + START_X);
    }
}
