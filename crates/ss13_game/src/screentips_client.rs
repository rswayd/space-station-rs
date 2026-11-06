//! Client half of the screentips ([`crate::screentips`]): tg `/atom/movable/screen/screentip`
//! (`TOP,LEFT`, the width of the map, centered): the hovered atom's name in the default
//! `LIGHT_COLOR_FAINT_BLUE` with a black outline, the context lines under it. What the cursor is
//! over is sent by `crate::client`'s click system (`ClientIntent::Hover`).
// shortcut: Bevy's default font instead of `Pix Cyrillic` / `TinyUnicode`, a drop shadow instead of
// the outline.

use bevy::prelude::*;
use ss13_net::ui::client::UiMessageReceived;
use ss13_render::MapInsets;
use ss13_shared::ui::UiServerMessage;
use ss13_tgui::client::AppTguiClientExt;
use ss13_tgui::{UiInterface, decode};

use crate::screentips::{ScreentipData, ScreentipUi};

/// tg `LIGHT_COLOR_FAINT_BLUE`.
const TIP_COLOR: Color = Color::srgb_u8(0xDE, 0xEF, 0xFF);
const SUBCONTEXT_COLOR: Color = Color::WHITE;

pub struct ScreentipsClientPlugin;

impl Plugin for ScreentipsClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_headless::<ScreentipUi>()
            .init_resource::<ScreentipHud>()
            .add_systems(Startup, spawn_line)
            .add_systems(
                Update,
                (
                    receive,
                    draw.run_if(resource_changed::<ScreentipHud>),
                    follow_map_insets.run_if(resource_changed::<MapInsets>),
                ),
            );
    }
}

/// The server's screentip window and the tip shown.
#[derive(Resource, Default, Debug)]
pub struct ScreentipHud {
    pub window: Option<u32>,
    pub data: ScreentipData,
}

#[derive(Component)]
struct ScreentipRoot;

fn spawn_line(mut commands: Commands) {
    commands.spawn((
        ScreentipRoot,
        Name::new("screentip"),
        Node {
            position_type: PositionType::Absolute,
            top: px(4),
            left: px(0),
            right: px(0),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            ..default()
        },
        Pickable::IGNORE,
        GlobalZIndex(1),
    ));
}

fn receive(mut received: MessageReader<UiMessageReceived>, mut hud: ResMut<ScreentipHud>) {
    for UiMessageReceived(message) in received.read() {
        let data = match message {
            UiServerMessage::Open {
                window,
                interface,
                data,
                ..
            } if interface == ScreentipUi::NAME => {
                hud.window = Some(*window);
                Some(data)
            }
            UiServerMessage::Update {
                window,
                data: Some(data),
                ..
            } if hud.window == Some(*window) => Some(data),
            UiServerMessage::Close { window } if hud.window == Some(*window) => {
                *hud = ScreentipHud::default();
                None
            }
            _ => None,
        };
        if let Some(data) = data.filter(|d| !d.is_empty()) {
            match decode::<ScreentipData>(data) {
                Ok(data) => hud.data = data,
                Err(e) => warn!("bad screentip data: {e}"),
            }
        }
    }
}

fn draw(hud: Res<ScreentipHud>, root: Single<Entity, With<ScreentipRoot>>, mut commands: Commands) {
    let root = *root;
    commands.entity(root).despawn_related::<Children>();
    if hud.window.is_none() || hud.data.name.is_empty() {
        return;
    }
    let shadow = TextShadow {
        offset: Vec2::splat(1.0),
        color: Color::BLACK,
    };
    commands.entity(root).with_child((
        Text::new(hud.data.name.clone()),
        TextColor(TIP_COLOR),
        shadow,
        Pickable::IGNORE,
    ));
    for line in &hud.data.lines {
        commands.entity(root).with_child((
            Text::new(line.clone()),
            TextColor(SUBCONTEXT_COLOR),
            shadow,
            Pickable::IGNORE,
        ));
    }
}

/// The line spans the map viewport, left of the docked chat.
fn follow_map_insets(insets: Res<MapInsets>, mut roots: Query<&mut Node, With<ScreentipRoot>>) {
    for mut node in &mut roots {
        node.left = px(insets.left);
        node.right = px(insets.right);
    }
}
