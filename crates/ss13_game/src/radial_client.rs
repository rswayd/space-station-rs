//! Client half of the radial menu ([`crate::radial`]): the headless [`RadialUi`] window drawn as
//! tg's slices around the anchor, one page at a time. A slice sends `submit` with its key, the
//! center `cancel`; the next-page slice only turns the page here.
// shortcut: 64 px slices (the map's zoom 2, as `crate::actions_client`), not re-scaled when the
// player zooms; no slide-in animation, no `radial_slice_focus` sprite and no hover tooltip (the
// hovered name is shown under the center); `radial_menu_offset`/`user_space` are not used by any caller yet.

use bevy::picking::events::{PointerClick, PointerOut, PointerOver};
use bevy::picking::pointer::PointerButton;
use bevy::text::FontSize;
use ss13_net::client::{NetEntityMap, SendIntent};
use ss13_net::ui::client::UiMessageReceived;
use ss13_render::atlas::frame_rect;
use ss13_render::{IconAtlas, WorldCamera};
use ss13_shared::protocol::ClientIntent;
use ss13_shared::ui::UiServerMessage;
use ss13_shared::{Dir, IconStateId, TILE_SIZE};
use ss13_tgui::client::AppTguiClientExt;
use ss13_tgui::interfaces::alert::{CANCEL, SUBMIT};
use ss13_tgui::interfaces::radial::{
    RadialData, RadialSlot, RadialUi, radial_offset, radial_pages,
};
use ss13_tgui::{UiInterface, decode};
use ss13_ui_kit::prelude::*;

/// One 32 px icon at the map's zoom 2.
const SLICE_PX: f32 = 64.0;
/// tg `radius = 32` icon pixels.
const RADIUS_PX: f32 = 64.0;

pub struct RadialClientPlugin;

impl Plugin for RadialClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_headless::<RadialUi>()
            .init_resource::<RadialState>()
            .add_systems(Startup, spawn_root)
            .add_systems(
                Update,
                (
                    receive,
                    rebuild.run_if(resource_changed::<RadialState>),
                    follow_anchor,
                )
                    .chain(),
            );
    }
}

/// The server's menu and the shown page.
#[derive(Resource, Default, Debug)]
pub struct RadialState {
    pub window: Option<u32>,
    pub data: RadialData,
    pub page: usize,
}

#[derive(Component)]
struct RadialRoot;

/// The name of the hovered slice.
#[derive(Component)]
struct RadialLabel;

/// What a click on this node does.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
enum RadialButton {
    Slot(RadialSlot),
    Close,
}

fn spawn_root(mut commands: Commands) {
    commands.spawn((
        RadialRoot,
        Name::new("radial menu"),
        Node {
            position_type: PositionType::Absolute,
            ..default()
        },
        Visibility::Hidden,
        GlobalZIndex(10),
    ));
}

fn receive(mut received: MessageReader<UiMessageReceived>, mut state: ResMut<RadialState>) {
    for UiMessageReceived(message) in received.read() {
        let data = match message {
            UiServerMessage::Open {
                window,
                interface,
                data,
                ..
            } if interface == RadialUi::NAME => {
                state.window = Some(*window);
                state.page = 0;
                Some(data)
            }
            UiServerMessage::Update {
                window,
                data: Some(data),
                ..
            } if state.window == Some(*window) => Some(data),
            UiServerMessage::Close { window } if state.window == Some(*window) => {
                *state = RadialState::default();
                None
            }
            _ => None,
        };
        if let Some(data) = data.filter(|d| !d.is_empty()) {
            match decode::<RadialData>(data) {
                Ok(data) => state.data = data,
                Err(e) => warn!("undecodable radial menu: {e}"),
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

fn font(size: f32) -> TextFont {
    TextFont {
        font_size: FontSize::Px(size),
        ..default()
    }
}

/// One 64 px disc at `(x, y)` from the anchor (y north).
fn disc(x: f32, y: f32) -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: px(x - SLICE_PX / 2.0),
        top: px(-y - SLICE_PX / 2.0),
        width: px(SLICE_PX),
        height: px(SLICE_PX),
        align_items: AlignItems::Center,
        justify_content: JustifyContent::Center,
        border_radius: BorderRadius::all(px(SLICE_PX / 2.0)),
        ..default()
    }
}

fn rebuild(
    state: Res<RadialState>,
    root: Single<(Entity, Option<&Children>), With<RadialRoot>>,
    mut visibility: Query<&mut Visibility, With<RadialRoot>>,
    mut atlas: Option<ResMut<IconAtlas>>,
    mut commands: Commands,
) {
    let (root, children) = *root;
    for &child in children.into_iter().flatten() {
        commands.entity(child).despawn();
    }
    let Ok(mut root_visibility) = visibility.single_mut() else {
        return;
    };
    if state.window.is_none() || state.data.items.is_empty() {
        *root_visibility = Visibility::Hidden;
        return;
    }
    *root_visibility = Visibility::Inherited;
    let pages = radial_pages(state.data.items.len());
    let page = &pages[state.page.min(pages.len() - 1)];
    commands.entity(root).with_children(|parent| {
        for (i, slot) in page.iter().enumerate() {
            let (x, y) = radial_offset(i, page.len(), RADIUS_PX);
            let mut slice = parent.spawn((
                disc(x, y),
                RadialButton::Slot(*slot),
                Name::new(format!("radial slice {}", i + 1)),
                BackgroundColor(Color::srgba(0.1, 0.1, 0.15, 0.8)),
            ));
            let frame = state.data.frame;
            if let Some(mut background) = image(&mut atlas, frame.slice) {
                background.color = Color::WHITE;
                slice.insert((background, BackgroundColor(Color::NONE)));
            }
            slice.observe(on_click).observe(on_over).observe(on_out);
            let (icon, label) = match slot {
                RadialSlot::Next => (frame.next, "Далее".to_owned()),
                RadialSlot::Item(i) => {
                    let item = &state.data.items[*i];
                    (item.icon, item.name.clone())
                }
            };
            match image(&mut atlas, icon) {
                Some(icon) => {
                    slice.with_child((
                        Node {
                            width: px(SLICE_PX),
                            height: px(SLICE_PX),
                            ..default()
                        },
                        icon,
                        Pickable::IGNORE,
                    ));
                }
                None => {
                    slice.with_child((
                        Text::new(label),
                        font(11.0),
                        TextLayout::justify(Justify::Center),
                        Pickable::IGNORE,
                    ));
                }
            }
        }
        let mut center = parent.spawn((
            disc(0.0, 0.0),
            RadialButton::Close,
            Name::new("radial center"),
            BackgroundColor(Color::srgba(0.1, 0.1, 0.15, 0.8)),
        ));
        if let Some(sprite) = image(&mut atlas, state.data.frame.center) {
            center.insert((sprite, BackgroundColor(Color::NONE)));
        }
        center.observe(on_click);
        parent.spawn((
            RadialLabel,
            Node {
                position_type: PositionType::Absolute,
                top: px(SLICE_PX * 1.6),
                left: px(-80.0),
                width: px(160.0),
                ..default()
            },
            Text::new(""),
            font(12.0),
            TextLayout::justify(Justify::Center),
            TextShadow::default(),
            Pickable::IGNORE,
        ));
    });
}

fn on_click(
    click: On<PointerClick>,
    buttons: Query<&RadialButton>,
    mut state: ResMut<RadialState>,
    mut intents: MessageWriter<SendIntent>,
) {
    if click.button != PointerButton::Primary {
        return;
    }
    let (Some(window), Ok(button)) = (state.window, buttons.get(click.entity)) else {
        return;
    };
    let (action, params) = match *button {
        RadialButton::Slot(RadialSlot::Next) => {
            state.page = (state.page + 1) % radial_pages(state.data.items.len()).len();
            return;
        }
        RadialButton::Slot(RadialSlot::Item(i)) => {
            let Some(item) = state.data.items.get(i) else {
                return;
            };
            (SUBMIT, vec![("entry".to_owned(), item.key.clone())])
        }
        RadialButton::Close => (CANCEL, Vec::new()),
    };
    intents.write(SendIntent(ClientIntent::UiAct {
        window,
        action: action.to_owned(),
        params,
    }));
}

fn on_over(
    over: On<PointerOver>,
    buttons: Query<&RadialButton>,
    state: Res<RadialState>,
    mut labels: Query<&mut Text, With<RadialLabel>>,
) {
    let Ok(RadialButton::Slot(slot)) = buttons.get(over.entity) else {
        return;
    };
    let name = match slot {
        RadialSlot::Next => "Далее".to_owned(),
        RadialSlot::Item(i) => state
            .data
            .items
            .get(*i)
            .map(|item| item.name.clone())
            .unwrap_or_default(),
    };
    for mut label in &mut labels {
        label.0.clone_from(&name);
    }
}

fn on_out(_: On<PointerOut>, mut labels: Query<&mut Text, With<RadialLabel>>) {
    for mut label in &mut labels {
        label.0.clear();
    }
}

/// The anchor's place on screen, every frame while open: the map scrolls under the menu.
fn follow_anchor(
    state: Res<RadialState>,
    map: Res<NetEntityMap>,
    cameras: Query<(&Camera, &GlobalTransform), With<WorldCamera>>,
    targets: Query<&GlobalTransform>,
    mut roots: Query<&mut Node, With<RadialRoot>>,
) {
    if state.window.is_none() {
        return;
    }
    let target = state
        .data
        .anchor
        .and_then(|id| map.get(id))
        .or_else(|| map.controlled());
    let Some(target) = target.and_then(|e| targets.get(e).ok()) else {
        return;
    };
    let Some((camera, view)) = cameras.iter().next() else {
        return;
    };
    let tile = TILE_SIZE as f32 / 2.0;
    let center = target.translation().truncate() + Vec2::new(tile, tile);
    let Ok(at) = camera.world_to_viewport(view, center.extend(0.0)) else {
        return;
    };
    for mut node in &mut roots {
        node.left = px(at.x);
        node.top = px(at.y);
    }
}
