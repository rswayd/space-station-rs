//! Client half of the screen alerts: tg's alert column on the right edge of the map
//! (`get_ui_alert_placement`: `EAST-1:28,CENTER+[6 - i]:[29 - i*2]`, at most 5), drawn from
//! [`AlertHudData`] through the headless tgui interface [`AlertHudUi`].

use bevy::prelude::*;
use ss13_net::ui::client::UiMessageReceived;
use ss13_render::atlas::frame_rect;
use ss13_render::{IconAtlas, MapInsets};
use ss13_shared::Dir;
use ss13_shared::ui::UiServerMessage;
use ss13_tgui::client::AppTguiClientExt;
use ss13_tgui::{UiInterface, decode};

use crate::hud::{AlertHudData, AlertHudUi, MAX_SHOWN_ALERTS};
use crate::inventory_client::{ScreenAxis, screen_loc};

pub struct AlertHudPlugin;

impl Plugin for AlertHudPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_headless::<AlertHudUi>()
            .init_resource::<AlertHud>()
            .add_systems(Startup, spawn_column)
            .add_systems(
                Update,
                (
                    receive_alerts,
                    draw_alerts.run_if(resource_changed::<AlertHud>),
                    follow_map_insets.run_if(resource_changed::<MapInsets>),
                ),
            );
    }
}

/// The server's alert window and its last data.
#[derive(Resource, Default, Debug)]
pub struct AlertHud {
    pub window: Option<u32>,
    pub data: AlertHudData,
}

#[derive(Component)]
struct AlertRoot;

/// The `i`th alert's place (0-based).
#[derive(Component)]
struct AlertSlot(usize);

fn spawn_column(mut commands: Commands) {
    let root = commands
        .spawn((
            AlertRoot,
            Name::new("alerts"),
            Node {
                position_type: PositionType::Absolute,
                top: px(0),
                bottom: px(0),
                left: px(0),
                right: px(0),
                ..default()
            },
            Pickable::IGNORE,
        ))
        .id();
    for i in 0..MAX_SHOWN_ALERTS {
        let index = i as i32 + 1;
        commands.spawn((
            AlertSlot(i),
            Name::new("alert"),
            screen_loc(
                ScreenAxis(1, -1, 28),
                ScreenAxis(0, 6 - index, 29 - index * 2),
            ),
            ImageNode::default(),
            Visibility::Hidden,
            Pickable::IGNORE,
            ChildOf(root),
        ));
    }
}

fn receive_alerts(mut received: MessageReader<UiMessageReceived>, mut hud: ResMut<AlertHud>) {
    for UiMessageReceived(message) in received.read() {
        let data = match message {
            UiServerMessage::Open {
                window,
                interface,
                data,
                ..
            } if interface == AlertHudUi::NAME => {
                hud.window = Some(*window);
                Some(data)
            }
            UiServerMessage::Update {
                window,
                data: Some(data),
                ..
            } if hud.window == Some(*window) => Some(data),
            UiServerMessage::Close { window } if hud.window == Some(*window) => {
                *hud = AlertHud::default();
                None
            }
            _ => None,
        };
        if let Some(data) = data.filter(|d| !d.is_empty()) {
            match decode::<AlertHudData>(data) {
                Ok(data) => hud.data = data,
                Err(e) => warn!("undecodable alert data: {e}"),
            }
        }
    }
}

fn draw_alerts(
    hud: Res<AlertHud>,
    mut atlas: Option<ResMut<IconAtlas>>,
    mut slots: Query<(&AlertSlot, &mut ImageNode, &mut Visibility, &mut Name)>,
) {
    for (AlertSlot(i), mut image, mut visibility, mut name) in &mut slots {
        let alert = hud.data.alerts.get(*i);
        let drawn = alert.and_then(|a| a.icon).and_then(|id| {
            atlas
                .as_deref_mut()?
                .state(id)
                .map(|(info, img)| (info.clone(), img.clone()))
        });
        match (alert, drawn) {
            (Some(alert), Some((info, img))) => {
                *image = ImageNode {
                    image: img,
                    rect: frame_rect(&info, Dir::South, 0),
                    ..default()
                };
                name.set(alert.name.clone());
                *visibility = Visibility::Inherited;
            }
            // Without the sprite: a red tile, so the alert still shows.
            (Some(alert), None) => {
                *image = ImageNode::solid_color(Color::srgb(0.6, 0.1, 0.1));
                name.set(alert.name.clone());
                *visibility = Visibility::Inherited;
            }
            (None, _) => *visibility = Visibility::Hidden,
        }
    }
}

fn follow_map_insets(insets: Res<MapInsets>, mut roots: Query<&mut Node, With<AlertRoot>>) {
    for mut node in &mut roots {
        node.left = px(insets.left);
        node.right = px(insets.right);
    }
}
