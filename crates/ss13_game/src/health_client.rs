//! Client half of the health HUD: tg's health indicator (`health0`..`health7`) and human health
//! doll (`icons/hud/screen_gen.dmi`) on the right edge of the map, the combat mode toggle
//! (`icons/hud/screen_midnight.dmi`, `ui_acti`) and the zone selector (`zone_sel`, `ui_zonesel`)
//! at the bottom right, the tg default key F (`toggle_combat_mode`) and the numpad zone keys
//! (`code/datums/keybinding/mob.dm`).
//!
//! A click on the zone selector or a numpad key sends the health HUD window's `zone` act; the
//! selected zone's overlay follows the server's answer.
//!
//! Draws only what the server sends ([`HealthHudData`] through the headless tgui interface
//! [`HealthHudUi`]); the move delay in it paces client-side prediction ([`OwnMoveDelay`]).

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

use crate::hud::{HealthHudData, HealthHudUi};
use crate::prediction::{OwnImmobile, OwnMoveDelay};
use crate::zone::TargetZone;

/// One 32 px HUD tile at the map's zoom 2.
const TILE_PX: f32 = 64.0;
const GEN_DMI: &str = "icons/hud/screen_gen.dmi";
const STYLE_DMI: &str = "icons/hud/screen_midnight.dmi";
/// Doll limbs in draw order: tg layers the chest above the others.
const LIMBS: [&str; 6] = ["head", "l_arm", "r_arm", "l_leg", "r_leg", "chest"];

pub struct HealthHudPlugin;

impl Plugin for HealthHudPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_headless::<HealthHudUi>()
            .init_resource::<HealthHud>()
            .add_systems(Startup, spawn_hud)
            .add_systems(
                Update,
                (
                    send_combat_key,
                    send_zone_keys,
                    receive_hud,
                    draw_hud.run_if(resource_changed::<HealthHud>),
                    follow_map_insets.run_if(resource_changed::<MapInsets>),
                ),
            );
    }
}

/// The server's health HUD window and its last data.
#[derive(Resource, Default, Debug)]
pub struct HealthHud {
    pub window: Option<u32>,
    pub data: HealthHudData,
}

#[derive(Component)]
#[require(crate::client_keys::HudInfo)]
struct HudRoot;

#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum Part {
    Health,
    Limb(usize),
    Combat,
    ZoneSel,
    ZoneOverlay,
}

/// `screen_gen.dmi` and `screen_midnight.dmi` states by name.
#[derive(Resource, Default)]
struct HudStates(HashMap<(&'static str, String), IconStateId>);

fn spawn_hud(atlas: Option<Res<IconAtlas>>, mut commands: Commands) {
    let states: HashMap<(&'static str, String), IconStateId> = atlas
        .as_ref()
        .map(|a| {
            a.manifest
                .index()
                .into_iter()
                .filter_map(|((dmi, state), id)| {
                    let dmi = [GEN_DMI, STYLE_DMI].into_iter().find(|d| *d == dmi)?;
                    Some(((dmi, state.to_owned()), id))
                })
                .collect()
        })
        .unwrap_or_default();
    commands.insert_resource(HudStates(states));
    let root = commands
        .spawn((
            HudRoot,
            Name::new("health hud"),
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
    let tile = |node: Node| Node {
        position_type: PositionType::Absolute,
        width: px(TILE_PX),
        height: px(TILE_PX),
        ..node
    };
    // tg `ui_health` "EAST-1:28,CENTER-1:19" and `ui_healthdoll` "EAST-1:28,CENTER-2:17" on a
    // 19x15 view: 4 px from the right edge, 3 px above and 31 px below the center line (x2).
    commands.spawn((
        Part::Health,
        Name::new("health"),
        tile(Node {
            right: px(8),
            top: percent(50),
            margin: UiRect::top(px(-6)),
            ..default()
        }),
        ImageNode::default(),
        Visibility::Hidden,
        Pickable::IGNORE,
        ChildOf(root),
    ));
    let doll = commands
        .spawn((
            Name::new("health doll"),
            tile(Node {
                right: px(8),
                top: percent(50),
                margin: UiRect::top(px(62)),
                ..default()
            }),
            Pickable::IGNORE,
            ChildOf(root),
        ))
        .id();
    for (i, limb) in LIMBS.into_iter().enumerate() {
        commands.spawn((
            Part::Limb(i),
            Name::new(limb),
            tile(Node::default()),
            ImageNode::default(),
            Visibility::Hidden,
            Pickable::IGNORE,
            ChildOf(doll),
        ));
    }
    // tg `ui_acti` "EAST-3:24,SOUTH:5": 72 px from the right edge, 5 px up (x2).
    commands
        .spawn((
            Part::Combat,
            Name::new("combat mode"),
            tile(Node {
                right: px(144),
                bottom: px(10),
                ..default()
            }),
            ImageNode::default(),
            Visibility::Hidden,
            ChildOf(root),
        ))
        .observe(click_combat);
    // tg `ui_zonesel` "EAST-1:28,SOUTH:5": 4 px from the right edge, 5 px up (x2).
    let selector = commands
        .spawn((
            Part::ZoneSel,
            Name::new("zone selector"),
            tile(Node {
                right: px(8),
                bottom: px(10),
                ..default()
            }),
            ImageNode::default(),
            Visibility::Hidden,
            ChildOf(root),
        ))
        .observe(click_zone)
        .id();
    commands.spawn((
        Part::ZoneOverlay,
        Name::new("selected zone"),
        tile(Node::default()),
        ImageNode::default(),
        Visibility::Hidden,
        Pickable::IGNORE,
        ChildOf(selector),
    ));
}

fn send_zone(hud: &HealthHud, zone: TargetZone, intents: &mut MessageWriter<SendIntent>) {
    if let Some(window) = hud.window
        && hud.data.health.is_some()
    {
        intents.write(SendIntent(ClientIntent::UiAct {
            window,
            action: "zone".into(),
            params: vec![("zone".into(), zone.dm().into())],
        }));
    }
}

/// tg `/atom/movable/screen/zone_sel/Click`: the zone under the cursor's icon pixel.
fn click_zone(
    click: On<PointerClick>,
    hud: Res<HealthHud>,
    mut intents: MessageWriter<SendIntent>,
) {
    if click.button != PointerButton::Primary {
        return;
    }
    // Normalized node coordinates, (-0.5, -0.5) top left; BYOND icon pixels count 1..=32 from
    // the bottom left.
    let Some(at) = click.hit.position else {
        return;
    };
    let x = ((at.x + 0.5) * 32.0).floor() as i32 + 1;
    let y = 32 - ((at.y + 0.5) * 32.0).floor() as i32;
    if let Some(zone) = TargetZone::at_icon(x, y) {
        send_zone(&hud, zone, &mut intents);
    }
}

/// tg's default numpad keybinds (Numpad8 cycles head, eyes, mouth), unless a text field has the
/// keyboard.
fn send_zone_keys(
    keys: Res<ButtonInput<KeyCode>>,
    focus: Option<Res<InputFocus>>,
    text_fields: Query<(), With<EditableText>>,
    hud: Res<HealthHud>,
    mut intents: MessageWriter<SendIntent>,
) {
    const NUMPAD: [KeyCode; 9] = [
        KeyCode::Numpad1,
        KeyCode::Numpad2,
        KeyCode::Numpad3,
        KeyCode::Numpad4,
        KeyCode::Numpad5,
        KeyCode::Numpad6,
        KeyCode::Numpad7,
        KeyCode::Numpad8,
        KeyCode::Numpad9,
    ];
    let typing = focus
        .and_then(|f| f.get())
        .is_some_and(|e| text_fields.contains(e));
    if typing {
        return;
    }
    for (i, key) in NUMPAD.into_iter().enumerate() {
        if keys.just_pressed(key)
            && let Some(zone) = TargetZone::numpad(i as u8 + 1, hud.data.zone_selected)
        {
            send_zone(&hud, zone, &mut intents);
        }
    }
}

fn toggle(map: &NetEntityMap, intents: &mut MessageWriter<SendIntent>) {
    if map.controlled().is_some() {
        intents.write(SendIntent(ClientIntent::ToggleCombatMode));
    }
}

fn click_combat(
    click: On<PointerClick>,
    map: Res<NetEntityMap>,
    mut intents: MessageWriter<SendIntent>,
) {
    if click.button == PointerButton::Primary {
        toggle(&map, &mut intents);
    }
}

/// F toggles combat mode, unless a text field has the keyboard.
fn send_combat_key(
    keys: Res<ButtonInput<KeyCode>>,
    focus: Option<Res<InputFocus>>,
    text_fields: Query<(), With<EditableText>>,
    map: Res<NetEntityMap>,
    mut intents: MessageWriter<SendIntent>,
) {
    let typing = focus
        .and_then(|f| f.get())
        .is_some_and(|e| text_fields.contains(e));
    // Shift+F is the quick accessory draw (`inventory_client`).
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    if !typing && !shift && keys.just_pressed(KeyCode::KeyF) {
        toggle(&map, &mut intents);
    }
}

fn receive_hud(
    mut received: MessageReader<UiMessageReceived>,
    mut hud: ResMut<HealthHud>,
    mut delay: ResMut<OwnMoveDelay>,
    mut immobile: ResMut<OwnImmobile>,
    mut phasing: ResMut<crate::prediction::OwnPhasing>,
) {
    for UiMessageReceived(message) in received.read() {
        let data = match message {
            UiServerMessage::Open {
                window,
                interface,
                data,
                ..
            } if interface == HealthHudUi::NAME => {
                hud.window = Some(*window);
                Some(data)
            }
            UiServerMessage::Update {
                window,
                data: Some(data),
                ..
            } if hud.window == Some(*window) => Some(data),
            UiServerMessage::Close { window } if hud.window == Some(*window) => {
                *hud = HealthHud::default();
                None
            }
            _ => None,
        };
        if let Some(data) = data.filter(|d| !d.is_empty()) {
            match decode::<HealthHudData>(data) {
                Ok(data) => {
                    if data.move_delay > 0.0 {
                        delay.set_if_neq(OwnMoveDelay(data.move_delay));
                    }
                    immobile.set_if_neq(OwnImmobile(data.immobile));
                    phasing.set_if_neq(crate::prediction::OwnPhasing(data.phasing));
                    hud.data = data;
                }
                Err(e) => warn!("undecodable health HUD data: {e}"),
            }
        }
    }
}

fn draw_hud(
    hud: Res<HealthHud>,
    states: Res<HudStates>,
    mut atlas: Option<ResMut<IconAtlas>>,
    mut parts: Query<(&Part, &mut ImageNode, &mut Visibility)>,
) {
    let readout = hud.window.and(hud.data.health);
    for (part, mut image, mut visibility) in &mut parts {
        let state = match (*part, readout) {
            (_, None) => None,
            (Part::Health, Some(r)) => Some((GEN_DMI, format!("health{}", r.health_state()))),
            (Part::Limb(i), Some(r)) => Some((
                GEN_DMI,
                match r.doll_state(i) {
                    Some(n) => format!("{}{n}", LIMBS[i]),
                    None => format!("{}DEAD", LIMBS[i]),
                },
            )),
            (Part::ZoneSel, Some(_)) => Some((STYLE_DMI, "zone_sel".to_owned())),
            (Part::ZoneOverlay, Some(_)) => Some((GEN_DMI, hud.data.zone_selected.dm().to_owned())),
            (Part::Combat, Some(_)) => Some((
                STYLE_DMI,
                if hud.data.combat_mode {
                    "combat"
                } else {
                    "combat_off"
                }
                .to_owned(),
            )),
        };
        let drawn = state.and_then(|key| {
            let id = *states.0.get(&key)?;
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

/// The HUD stays on the map viewport, left of the docked chat.
fn follow_map_insets(insets: Res<MapInsets>, mut roots: Query<&mut Node, With<HudRoot>>) {
    for mut node in &mut roots {
        node.left = px(insets.left);
        node.right = px(insets.right);
    }
}
