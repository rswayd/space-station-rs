//! Client half of the mood face: tg `/atom/movable/screen/mood` at `ui_mood`
//! (`EAST-1:28,CENTER:21`), `icons/hud/screen_gen.dmi` `mood1`..`mood9` tinted by the sanity
//! colour, from [`MoodHudData`] through the headless tgui interface [`MoodHudUi`]. A click sends
//! its `click` act: the server prints the mood to the chat.

use bevy::picking::events::PointerClick;
use bevy::picking::pointer::PointerButton;
use bevy::prelude::*;
use ss13_net::client::SendIntent;
use ss13_net::ui::client::UiMessageReceived;
use ss13_render::atlas::frame_rect;
use ss13_render::{IconAtlas, MapInsets};
use ss13_shared::protocol::ClientIntent;
use ss13_shared::ui::UiServerMessage;
use ss13_shared::{Dir, IconStateId};
use ss13_tgui::client::AppTguiClientExt;
use ss13_tgui::{UiInterface, decode};

use crate::hud::{MOOD_CLICK, MoodHudData, MoodHudUi};
use crate::inventory_client::{ScreenAxis, screen_loc};

const GEN_DMI: &str = "icons/hud/screen_gen.dmi";

pub struct MoodHudPlugin;

impl Plugin for MoodHudPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_headless::<MoodHudUi>()
            .init_resource::<MoodHud>()
            .add_systems(Startup, spawn_face)
            .add_systems(
                Update,
                (
                    receive_mood,
                    draw_face.run_if(resource_changed::<MoodHud>),
                    follow_map_insets.run_if(resource_changed::<MapInsets>),
                ),
            );
    }
}

/// The server's mood window and its last data.
#[derive(Resource, Default, Debug)]
pub struct MoodHud {
    pub window: Option<u32>,
    pub data: MoodHudData,
}

#[derive(Component)]
struct MoodRoot;

#[derive(Component)]
struct MoodFace;

/// `mood1`..`mood9`.
#[derive(Resource, Default)]
struct MoodStates([Option<IconStateId>; 9]);

fn spawn_face(atlas: Option<Res<IconAtlas>>, mut commands: Commands) {
    let index = atlas
        .as_ref()
        .map(|a| a.manifest.index())
        .unwrap_or_default();
    let states = std::array::from_fn(|i| {
        let state = format!("mood{}", i + 1);
        index.get(&(GEN_DMI, state.as_str())).copied()
    });
    commands.insert_resource(MoodStates(states));
    let root = commands
        .spawn((
            MoodRoot,
            Name::new("mood"),
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
    commands
        .spawn((
            MoodFace,
            Name::new("mood face"),
            screen_loc(ScreenAxis(1, -1, 28), ScreenAxis(0, 0, 21)),
            ImageNode::default(),
            Visibility::Hidden,
            ChildOf(root),
        ))
        .observe(click_face);
}

fn receive_mood(mut received: MessageReader<UiMessageReceived>, mut hud: ResMut<MoodHud>) {
    for UiMessageReceived(message) in received.read() {
        let data = match message {
            UiServerMessage::Open {
                window,
                interface,
                data,
                ..
            } if interface == MoodHudUi::NAME => {
                hud.window = Some(*window);
                Some(data)
            }
            UiServerMessage::Update {
                window,
                data: Some(data),
                ..
            } if hud.window == Some(*window) => Some(data),
            UiServerMessage::Close { window } if hud.window == Some(*window) => {
                *hud = MoodHud::default();
                None
            }
            _ => None,
        };
        if let Some(data) = data.filter(|d| !d.is_empty()) {
            match decode::<MoodHudData>(data) {
                Ok(data) => hud.data = data,
                Err(e) => warn!("undecodable mood data: {e}"),
            }
        }
    }
}

fn draw_face(
    hud: Res<MoodHud>,
    states: Res<MoodStates>,
    mut atlas: Option<ResMut<IconAtlas>>,
    mut face: Single<(&mut Visibility, &mut ImageNode), With<MoodFace>>,
) {
    let (visibility, image) = &mut *face;
    let level = usize::from(hud.data.level);
    let shown = hud.window.is_some() && (1..=9).contains(&level);
    let id = shown.then(|| states.0[level - 1]).flatten();
    let drawn = id.and_then(|id| {
        let (info, handle) = atlas.as_deref_mut()?.state(id)?;
        Some(ImageNode {
            image: handle,
            rect: frame_rect(info, Dir::South, 0),
            color: {
                let [r, g, b] = hud.data.color;
                Color::srgb_u8(r, g, b)
            },
            ..default()
        })
    });
    match drawn {
        Some(drawn) if shown => {
            **image = drawn;
            **visibility = Visibility::Inherited;
        }
        _ => **visibility = Visibility::Hidden,
    }
}

/// tg `/atom/movable/screen/mood` `Click` -> `hud_click`.
fn click_face(click: On<PointerClick>, hud: Res<MoodHud>, mut intents: MessageWriter<SendIntent>) {
    if click.button != PointerButton::Primary {
        return;
    }
    if let Some(window) = hud.window {
        intents.write(SendIntent(ClientIntent::UiAct {
            window,
            action: MOOD_CLICK.into(),
            params: Vec::new(),
        }));
    }
}

/// The face stays on the map viewport, left of the docked chat.
fn follow_map_insets(insets: Res<MapInsets>, mut roots: Query<&mut Node, With<MoodRoot>>) {
    for mut node in &mut roots {
        node.left = px(insets.left);
        node.right = px(insets.right);
    }
}
