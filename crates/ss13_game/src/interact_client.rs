//! Client half of pulling and throwing: tg's keys (`stop_pulling` C and Delete in BandaStation,
//! `resist` B, `toggle_throw_mode` R) and HUD buttons (`icons/hud/screen_midnight.dmi`: the pull
//! icon `pull`/`pull0`, `act_resist`, `act_throw`/`act_throw_on`, the move intent
//! `running`/`walking`; BandaStation leaves its keybinding unbound), drawn from the server's
//! [`InteractHudData`] (headless tgui interface [`InteractHudUi`]).

use std::collections::HashMap;

use bevy::input_focus::InputFocus;
use bevy::picking::events::PointerClick;
use bevy::picking::pointer::PointerButton;
use bevy::prelude::*;
use bevy::text::EditableText;
use ss13_net::client::{NetEntityMap, SendIntent};
use ss13_net::ui::client::UiMessageReceived;
use ss13_render::atlas::frame_rect;
use ss13_render::{IconAtlas, MapInsets};
use ss13_shared::protocol::ClientIntent;
use ss13_shared::ui::UiServerMessage;
use ss13_shared::{Dir, IconStateId};
use ss13_tgui::client::AppTguiClientExt;
use ss13_tgui::{UiInterface, decode};

use crate::hud::{InteractHudData, InteractHudUi};

/// One 32 px HUD tile at the map's zoom 2.
const TILE_PX: f32 = 64.0;
const STYLE_DMI: &str = "icons/hud/screen_midnight.dmi";

pub struct InteractHudPlugin;

impl Plugin for InteractHudPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_headless::<InteractHudUi>()
            .init_resource::<InteractHud>()
            .add_systems(Startup, spawn_buttons)
            .add_systems(
                Update,
                (
                    send_keys,
                    receive_hud,
                    draw_buttons.run_if(resource_changed::<InteractHud>),
                    follow_map_insets.run_if(resource_changed::<MapInsets>),
                ),
            );
    }
}

/// The server's interact HUD window and its last data.
#[derive(Resource, Default, Debug)]
pub struct InteractHud {
    pub window: Option<u32>,
    pub data: InteractHudData,
}

#[derive(Component)]
struct ButtonsRoot;

#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
enum Button {
    Pull,
    Resist,
    Throw,
    MoveIntent,
}

impl Button {
    fn intent(self) -> ClientIntent {
        match self {
            Self::Pull => ClientIntent::StopPulling,
            Self::Resist => ClientIntent::Resist,
            Self::Throw => ClientIntent::ToggleThrowMode,
            Self::MoveIntent => ClientIntent::ToggleMoveIntent,
        }
    }

    fn state(self, data: &InteractHudData) -> &'static str {
        match self {
            Self::Pull if data.pulling => "pull",
            Self::Pull => "pull0",
            Self::Resist => "act_resist",
            Self::Throw if data.throw_mode => "act_throw_on",
            Self::Throw => "act_throw",
            Self::MoveIntent if data.walking => "walking",
            Self::MoveIntent => "running",
        }
    }
}

#[derive(Resource, Default)]
struct ButtonStates(HashMap<String, IconStateId>);

fn spawn_buttons(atlas: Option<Res<IconAtlas>>, mut commands: Commands) {
    let states = atlas
        .as_ref()
        .map(|a| {
            a.manifest
                .index()
                .into_iter()
                .filter(|((dmi, _), _)| *dmi == STYLE_DMI)
                .map(|((_, state), id)| (state.to_owned(), id))
                .collect()
        })
        .unwrap_or_default();
    commands.insert_resource(ButtonStates(states));
    let root = commands
        .spawn((
            ButtonsRoot,
            Name::new("interact hud"),
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                right: px(0),
                top: px(0),
                bottom: px(0),
                ..default()
            },
            Pickable::IGNORE,
        ))
        .id();
    // tg screen_loc on a 19x15 view, x2: `ui_above_movement_top` "EAST-2:26,SOUTH+1:24" (pull),
    // `ui_above_movement` "EAST-2:26,SOUTH+1:7" (resist), `ui_drop_throw` "EAST-1:28,SOUTH+1:24",
    // `ui_movi` "EAST-2:26,SOUTH:5" (move intent).
    for (button, name, right, bottom) in [
        (Button::MoveIntent, "move intent", 76.0, 10.0),
        (Button::Resist, "resist", 76.0, 78.0),
        (Button::Pull, "pull", 76.0, 112.0),
        (Button::Throw, "throw", 8.0, 112.0),
    ] {
        commands
            .spawn((
                button,
                Name::new(name),
                Node {
                    position_type: PositionType::Absolute,
                    width: px(TILE_PX),
                    height: px(TILE_PX),
                    right: px(right),
                    bottom: px(bottom),
                    ..default()
                },
                ImageNode::default(),
                Visibility::Hidden,
                ChildOf(root),
            ))
            .observe(click_button);
    }
}

fn click_button(
    click: On<PointerClick>,
    buttons: Query<&Button>,
    map: Res<NetEntityMap>,
    mut intents: MessageWriter<SendIntent>,
) {
    if click.button != PointerButton::Primary || map.controlled().is_none() {
        return;
    }
    if let Ok(button) = buttons.get(click.entity) {
        intents.write(SendIntent(button.intent()));
    }
}

/// C or Delete stop pulling, B resists, R toggles throw mode, unless a text field has the
/// keyboard.
fn send_keys(
    keys: Res<ButtonInput<KeyCode>>,
    focus: Option<Res<InputFocus>>,
    text_fields: Query<(), With<EditableText>>,
    map: Res<NetEntityMap>,
    mut intents: MessageWriter<SendIntent>,
) {
    let typing = focus
        .and_then(|f| f.get())
        .is_some_and(|e| text_fields.contains(e));
    if typing || map.controlled().is_none() {
        return;
    }
    if keys.any_just_pressed([KeyCode::KeyC, KeyCode::Delete]) {
        intents.write(SendIntent(ClientIntent::StopPulling));
    }
    if keys.just_pressed(KeyCode::KeyB) {
        intents.write(SendIntent(ClientIntent::Resist));
    }
    if keys.just_pressed(KeyCode::KeyR) {
        intents.write(SendIntent(ClientIntent::ToggleThrowMode));
    }
}

fn receive_hud(mut received: MessageReader<UiMessageReceived>, mut hud: ResMut<InteractHud>) {
    for UiMessageReceived(message) in received.read() {
        let data = match message {
            UiServerMessage::Open {
                window,
                interface,
                data,
                ..
            } if interface == InteractHudUi::NAME => {
                hud.window = Some(*window);
                Some(data)
            }
            UiServerMessage::Update {
                window,
                data: Some(data),
                ..
            } if hud.window == Some(*window) => Some(data),
            UiServerMessage::Close { window } if hud.window == Some(*window) => {
                *hud = InteractHud::default();
                None
            }
            _ => None,
        };
        if let Some(data) = data.filter(|d| !d.is_empty()) {
            match decode::<InteractHudData>(data) {
                Ok(data) => hud.data = data,
                Err(e) => warn!("undecodable interact HUD data: {e}"),
            }
        }
    }
}

fn draw_buttons(
    hud: Res<InteractHud>,
    states: Res<ButtonStates>,
    mut atlas: Option<ResMut<IconAtlas>>,
    mut buttons: Query<(&Button, &mut ImageNode, &mut Visibility)>,
) {
    for (button, mut image, mut visibility) in &mut buttons {
        let drawn = hud.window.and_then(|_| {
            let id = *states.0.get(button.state(&hud.data))?;
            let (info, handle) = atlas.as_deref_mut()?.state(id)?;
            Some(ImageNode {
                image: handle,
                rect: frame_rect(info, Dir::South, 0),
                ..default()
            })
        });
        *visibility = match drawn {
            Some(drawn) => {
                *image = drawn;
                Visibility::Inherited
            }
            None => Visibility::Hidden,
        };
    }
}

/// The buttons stay on the map viewport, left of the docked chat.
fn follow_map_insets(insets: Res<MapInsets>, mut roots: Query<&mut Node, With<ButtonsRoot>>) {
    for mut node in &mut roots {
        node.left = px(insets.left);
        node.right = px(insets.right);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn button_states_follow_tg() {
        let off = InteractHudData::default();
        let on = InteractHudData {
            throw_mode: true,
            pulling: true,
            walking: true,
        };
        assert_eq!(Button::Pull.state(&off), "pull0");
        assert_eq!(Button::Pull.state(&on), "pull");
        assert_eq!(Button::Throw.state(&off), "act_throw");
        assert_eq!(Button::Throw.state(&on), "act_throw_on");
        assert_eq!(Button::MoveIntent.state(&off), "running");
        assert_eq!(Button::MoveIntent.state(&on), "walking");
        assert_eq!(Button::Resist.intent(), ClientIntent::Resist);
        assert_eq!(Button::MoveIntent.intent(), ClientIntent::ToggleMoveIntent);
    }
}
