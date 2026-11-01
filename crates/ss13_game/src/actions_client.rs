//! Client half of the action buttons ([`crate::actions`]): tg's action row at the top left
//! (`ButtonNumberToScreenCoords`: button `n` at `WEST+n:4+2(n+1),NORTH:-6`, ten per row), after
//! the antag info button when there is one. A button is its background, the icon and the border
//! on top; unavailable or cooling down it is tinted `rgb(128,0,0)`, with the cooldown's seconds
//! (or its own maptext) in tiny white text at the bottom left. A click sends `trigger`.
// shortcut: the cooldown text counts down locally from the last update instead of being re-sent
// every tick.

use bevy::picking::events::PointerClick;
use bevy::picking::pointer::PointerButton;
use ss13_net::client::SendIntent;
use ss13_net::ui::client::UiMessageReceived;
use ss13_render::atlas::frame_rect;
use ss13_render::{IconAtlas, MapInsets};
use ss13_shared::protocol::ClientIntent;
use ss13_shared::ui::UiServerMessage;
use ss13_shared::{Dir, IconStateId};
use ss13_tgui::client::AppTguiClientExt;
use ss13_tgui::{UiInterface, decode};
use ss13_ui_kit::prelude::*;
use ss13_ui_kit::theme::{Bold, FontPx};

use crate::actions::{ActionButtonsData, ActionButtonsUi, TRIGGER};

/// One 32 px HUD tile at the map's zoom 2.
const TILE_PX: f32 = 64.0;
/// tg `column_max` of the action row.
const COLUMNS: usize = 10;

pub struct ActionHudPlugin;

impl Plugin for ActionHudPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_headless::<ActionButtonsUi>()
            .init_resource::<ActionHud>()
            .add_systems(Startup, spawn_root)
            .add_systems(
                Update,
                (
                    receive,
                    rebuild.run_if(
                        |hud: Res<ActionHud>, antag: Res<crate::antag::client::AntagHud>| {
                            hud.is_changed() || antag.is_changed()
                        },
                    ),
                    count_down,
                    follow_map_insets.run_if(resource_changed::<MapInsets>),
                )
                    .chain(),
            );
    }
}

/// The server's action window, its last data and when it arrived (`Time::elapsed_secs_f64`).
#[derive(Resource, Default, Debug)]
pub struct ActionHud {
    pub window: Option<u32>,
    pub data: ActionButtonsData,
    pub received_at: f64,
}

#[derive(Component)]
struct ActionRoot;

/// A button and its index in [`ActionHud::data`].
#[derive(Component)]
struct ActionSlot(usize);

#[derive(Component)]
struct Maptext(usize);

/// Left and top of button `n` in screen pixels (zoom 2): tg `4 + 2 * (column + 1)` icon pixels
/// from the west edge plus 32 per column, 6 below the north edge plus 32 per row.
pub fn slot_position(n: usize) -> (f32, f32) {
    let (row, column) = (n / COLUMNS, n % COLUMNS);
    let left = (column as f32 * 32.0 + 4.0 + 2.0 * (column as f32 + 1.0)) * 2.0;
    let top = (6.0 + row as f32 * 32.0) * 2.0;
    (left, top)
}

fn spawn_root(mut commands: Commands) {
    commands.spawn((
        ActionRoot,
        Name::new("action buttons"),
        Node {
            position_type: PositionType::Absolute,
            left: px(0),
            right: px(0),
            top: px(0),
            bottom: px(0),
            ..default()
        },
        Pickable::IGNORE,
    ));
}

fn receive(
    mut received: MessageReader<UiMessageReceived>,
    mut hud: ResMut<ActionHud>,
    time: Res<Time>,
) {
    for UiMessageReceived(message) in received.read() {
        let data = match message {
            UiServerMessage::Open {
                window,
                interface,
                data,
                ..
            } if interface == ActionButtonsUi::NAME => {
                hud.window = Some(*window);
                Some(data)
            }
            UiServerMessage::Update {
                window,
                data: Some(data),
                ..
            } if hud.window == Some(*window) => Some(data),
            UiServerMessage::Close { window } if hud.window == Some(*window) => {
                *hud = ActionHud::default();
                None
            }
            _ => None,
        };
        if let Some(data) = data.filter(|d| !d.is_empty()) {
            match decode::<ActionButtonsData>(data) {
                Ok(data) => {
                    hud.data = data;
                    hud.received_at = time.elapsed_secs_f64();
                }
                Err(e) => warn!("undecodable action buttons: {e}"),
            }
        }
    }
}

fn image(atlas: &mut Option<ResMut<IconAtlas>>, id: Option<IconStateId>) -> Option<ImageNode> {
    let (info, handle) = atlas.as_deref_mut()?.state(id?)?;
    Some(ImageNode {
        image: handle,
        rect: frame_rect(info, Dir::South, 0),
        ..default()
    })
}

/// The buttons again whenever the data changes (a few buttons, a few times a minute).
fn rebuild(
    hud: Res<ActionHud>,
    antag: Res<crate::antag::client::AntagHud>,
    root: Single<(Entity, Option<&Children>), With<ActionRoot>>,
    mut atlas: Option<ResMut<IconAtlas>>,
    mut commands: Commands,
) {
    let (root, children) = *root;
    for &child in children.into_iter().flatten() {
        commands.entity(child).despawn();
    }
    if hud.window.is_none() {
        return;
    }
    // The antag info button is the first action of an antagonist.
    let first = usize::from(antag.window.is_some());
    for (i, button) in hud.data.buttons.iter().enumerate() {
        let (left, top) = slot_position(first + i);
        let tint = if button.available && button.cooldown <= 0.0 {
            Color::WHITE
        } else {
            Color::srgb_u8(128, 0, 0)
        };
        let full = || Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            ..default()
        };
        let mut layers = Vec::new();
        for id in [button.background, button.icon, button.border] {
            if let Some(mut node) = image(&mut atlas, id) {
                node.color = tint;
                layers.push(node);
            }
        }
        let slot = commands
            .spawn((
                ActionSlot(i),
                Name::new(format!("action: {}", button.name)),
                Node {
                    position_type: PositionType::Absolute,
                    left: px(left),
                    top: px(top),
                    width: px(TILE_PX),
                    height: px(TILE_PX),
                    ..default()
                },
                ChildOf(root),
            ))
            .observe(click)
            .id();
        for layer in layers {
            commands.spawn((full(), layer, Pickable::IGNORE, ChildOf(slot)));
        }
        commands.spawn((
            Maptext(i),
            Node {
                position_type: PositionType::Absolute,
                left: px(8),
                bottom: px(4),
                ..default()
            },
            Text::default(),
            TextColor(Color::WHITE),
            FontPx(12.0),
            Bold(true),
            Pickable::IGNORE,
            ChildOf(slot),
        ));
    }
}

/// tg `MAPTEXT_TINY_UNICODE(round(time_left / 10))`, or the button's own maptext.
fn count_down(hud: Res<ActionHud>, time: Res<Time>, mut texts: Query<(&Maptext, &mut Text)>) {
    let passed = (time.elapsed_secs_f64() - hud.received_at) as f32;
    for (Maptext(i), mut text) in &mut texts {
        let Some(button) = hud.data.buttons.get(*i) else {
            continue;
        };
        let left = button.cooldown - passed;
        let shown = if left > 0.0 {
            format!("{}", left.round() as i32)
        } else {
            button.maptext.clone().unwrap_or_default()
        };
        if text.0 != shown {
            text.0 = shown;
        }
    }
}

fn click(
    click: On<PointerClick>,
    slots: Query<&ActionSlot>,
    hud: Res<ActionHud>,
    mut intents: MessageWriter<SendIntent>,
) {
    if click.button != PointerButton::Primary {
        return;
    }
    let (Some(window), Ok(ActionSlot(i))) = (hud.window, slots.get(click.entity)) else {
        return;
    };
    if let Some(button) = hud.data.buttons.get(*i) {
        intents.write(SendIntent(ClientIntent::UiAct {
            window,
            action: TRIGGER.to_owned(),
            params: vec![("id".to_owned(), button.id.clone())],
        }));
    }
}

fn follow_map_insets(insets: Res<MapInsets>, mut roots: Query<&mut Node, With<ActionRoot>>) {
    for mut node in &mut roots {
        node.left = px(insets.left);
        node.right = px(insets.right);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buttons_follow_tg_screen_coords() {
        // `WEST+0:6,NORTH:-6` is the antag info button at (12, 12).
        assert_eq!(slot_position(0), (12.0, 12.0));
        // `WEST+1:8`: 32 + 8 icon pixels.
        assert_eq!(slot_position(1), (80.0, 12.0));
        // The eleventh starts the second row.
        assert_eq!(slot_position(10), (12.0, 76.0));
    }
}
