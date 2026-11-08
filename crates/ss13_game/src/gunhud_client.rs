//! Client half of the ammo counter: BandaStation's `/atom/movable/screen/gunhud_screen` at
//! `ui_gunhud` (`RIGHT-1:28,CENTER-5:9`): the base state of [`GUNHUD_DMI`], its overlays tinted
//! with `backing_color`, and an energy gun's two-line maptext (cell percent, shot cost), from
//! [`GunHudData`] through the headless tgui interface [`GunHudUi`].
// shortcut: the maptext is centred in the slot (tg shifts it by `maptext_x`); animated states
// (`empty_flash`) show their first frame.

use std::collections::HashMap;

use bevy::prelude::*;
use ss13_net::ui::client::UiMessageReceived;
use ss13_render::atlas::frame_rect;
use ss13_render::{IconAtlas, MapInsets};
use ss13_shared::ui::UiServerMessage;
use ss13_shared::{Dir, IconStateId};
use ss13_tgui::client::AppTguiClientExt;
use ss13_tgui::{UiInterface, decode};
use ss13_ui_kit::theme::{Bold, FontPx};

use crate::hud::{GUNHUD_DMI, GunHudData, GunHudUi};
use crate::inventory_client::{ScreenAxis, screen_loc};

/// Overlays a counter can have at once (`oth_light`, three digits, the indicator).
const LAYERS: usize = 5;
/// The states of `gun_hud.dmi` the counter uses.
const STATES: [&str; 46] = [
    "backing",
    "eammo_counter",
    "eammo_counter_empty",
    "oth_light",
    "oe",
    "te",
    "he",
    "no_mag",
    "empty_flash",
    "flame_on",
    "flame_off",
    "o0",
    "o1",
    "o2",
    "o3",
    "o4",
    "o5",
    "o6",
    "o7",
    "o8",
    "o9",
    "t0",
    "t1",
    "t2",
    "t3",
    "t4",
    "t5",
    "t6",
    "t7",
    "t8",
    "t9",
    "h0",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "h7",
    "h8",
    "h9",
    "auto",
    "burst",
    "semi",
    "safe",
    "jam",
];

pub struct GunHudPlugin;

impl Plugin for GunHudPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_headless::<GunHudUi>()
            .init_resource::<GunHud>()
            .add_systems(Startup, spawn_counter)
            .add_systems(
                Update,
                (
                    receive_gunhud,
                    draw_counter.run_if(resource_changed::<GunHud>),
                    follow_map_insets.run_if(resource_changed::<MapInsets>),
                ),
            );
    }
}

/// The server's counter window and its last data.
#[derive(Resource, Default, Debug)]
pub struct GunHud {
    pub window: Option<u32>,
    pub data: GunHudData,
}

#[derive(Resource, Default)]
struct GunHudStates(HashMap<&'static str, IconStateId>);

#[derive(Component)]
#[require(crate::client_keys::HudInfo)]
struct GunHudRoot;

#[derive(Component)]
struct GunHudSlot;

/// The base sprite (`None`) or overlay `n`.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
struct Layer(Option<usize>);

/// The maptext's cell line (`true`) or shot line.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
struct Line(bool);

fn spawn_counter(atlas: Option<Res<IconAtlas>>, mut commands: Commands) {
    let index = atlas
        .as_ref()
        .map(|a| a.manifest.index())
        .unwrap_or_default();
    let states = STATES
        .iter()
        .filter_map(|&s| index.get(&(GUNHUD_DMI, s)).map(|&id| (s, id)))
        .collect();
    commands.insert_resource(GunHudStates(states));
    let root = commands
        .spawn((
            GunHudRoot,
            Name::new("gunhud"),
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
    let slot = commands
        .spawn((
            GunHudSlot,
            Name::new("ammo counter"),
            screen_loc(ScreenAxis(1, -1, 28), ScreenAxis(0, -5, 9)),
            Visibility::Hidden,
            Pickable::IGNORE,
            ChildOf(root),
        ))
        .id();
    let full = || Node {
        position_type: PositionType::Absolute,
        width: percent(100),
        height: percent(100),
        ..default()
    };
    for layer in std::iter::once(None).chain((0..LAYERS).map(Some)) {
        commands.spawn((
            Layer(layer),
            full(),
            ImageNode::default(),
            Visibility::Hidden,
            Pickable::IGNORE,
            ChildOf(slot),
        ));
    }
    let text = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            Pickable::IGNORE,
            ChildOf(slot),
        ))
        .id();
    for cell in [true, false] {
        commands.spawn((
            Line(cell),
            Text::default(),
            TextColor(Color::WHITE),
            FontPx(10.0),
            Bold(cell),
            Pickable::IGNORE,
            ChildOf(text),
        ));
    }
}

fn receive_gunhud(mut received: MessageReader<UiMessageReceived>, mut hud: ResMut<GunHud>) {
    for UiMessageReceived(message) in received.read() {
        let data = match message {
            UiServerMessage::Open {
                window,
                interface,
                data,
                ..
            } if interface == GunHudUi::NAME => {
                hud.window = Some(*window);
                Some(data)
            }
            UiServerMessage::Update {
                window,
                data: Some(data),
                ..
            } if hud.window == Some(*window) => Some(data),
            UiServerMessage::Close { window } if hud.window == Some(*window) => {
                *hud = GunHud::default();
                None
            }
            _ => None,
        };
        if let Some(data) = data.filter(|d| !d.is_empty()) {
            match decode::<GunHudData>(data) {
                Ok(data) => hud.data = data,
                Err(e) => warn!("undecodable gunhud data: {e}"),
            }
        }
    }
}

fn sprite(atlas: &mut Option<ResMut<IconAtlas>>, id: Option<IconStateId>) -> Option<ImageNode> {
    let (info, handle) = atlas.as_deref_mut()?.state(id?)?;
    Some(ImageNode {
        image: handle,
        rect: frame_rect(info, Dir::South, 0),
        ..default()
    })
}

fn rgb([r, g, b]: [u8; 3]) -> Color {
    Color::srgb_u8(r, g, b)
}

fn draw_counter(
    hud: Res<GunHud>,
    states: Res<GunHudStates>,
    mut atlas: Option<ResMut<IconAtlas>>,
    mut slot: Single<&mut Visibility, (With<GunHudSlot>, Without<Layer>)>,
    mut layers: Query<(&Layer, &mut ImageNode, &mut Visibility), Without<GunHudSlot>>,
    mut lines: Query<(&Line, &mut Text, &mut TextColor)>,
) {
    let shown = hud.window.is_some() && hud.data.shown;
    **slot = if shown {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    if !shown {
        return;
    }
    let data = &hud.data;
    for (layer, mut image, mut visibility) in &mut layers {
        let (state, tint) = match layer.0 {
            None => (Some(data.base.as_str()), None),
            Some(n) => match data.layers.get(n) {
                Some((state, color)) => (Some(state.as_str()), Some(rgb(*color))),
                None => (None, None),
            },
        };
        let drawn = state
            .and_then(|s| states.0.get(s).copied())
            .and_then(|id| sprite(&mut atlas, Some(id)));
        match drawn {
            Some(mut drawn) => {
                if let Some(tint) = tint {
                    drawn.color = tint;
                }
                *image = drawn;
                *visibility = Visibility::Inherited;
            }
            None => *visibility = Visibility::Hidden,
        }
    }
    for (line, mut text, mut color) in &mut lines {
        let (value, tint) = match (data.energy, line.0) {
            (Some((cell, tint, _)), true) => (format!("{cell}%"), rgb(tint)),
            (Some((_, _, shot)), false) => (format!("{shot}%"), rgb([0x00, 0xFF, 0xFF])),
            (None, _) => (String::new(), Color::WHITE),
        };
        if text.0 != value {
            text.0 = value;
        }
        color.0 = tint;
    }
}

/// The counter stays on the map viewport, left of the docked chat.
fn follow_map_insets(insets: Res<MapInsets>, mut roots: Query<&mut Node, With<GunHudRoot>>) {
    for mut node in &mut roots {
        node.left = px(insets.left);
        node.right = px(insets.right);
    }
}
