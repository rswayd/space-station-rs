//! Client half of the hunger bar: tg `/atom/movable/screen/hunger` at `ui_hunger`
//! (`EAST-1:2,CENTER:21`): the burger (`icons/obj/food/burgerbread.dmi` `hburger`, 5 px left,
//! dark when fat), the frame (`icons/hud/screen_gen.dmi` `hungerbar`) and the bar
//! (`hungerbar_bar`) colored by tg's `hunger_gradient` and cut by `hungerbar_mask` shifted
//! `-20 + 20 * fullness` px, from [`HungerHudData`] through the headless tgui interface
//! [`HungerHudUi`].
// shortcut: the mask is a rectangular cut from the top; the starving outline is a static red tint
// of the frame instead of a pulsing outline filter; no fade animations.

use bevy::prelude::*;
use ss13_net::ui::client::UiMessageReceived;
use ss13_render::atlas::frame_rect;
use ss13_render::{IconAtlas, MapInsets};
use ss13_shared::ui::UiServerMessage;
use ss13_shared::{Dir, IconStateId};
use ss13_tgui::client::AppTguiClientExt;
use ss13_tgui::{UiInterface, decode};

use crate::hud::{HungerHudData, HungerHudUi};
use crate::inventory_client::{ScreenAxis, screen_loc};

const GEN_DMI: &str = "icons/hud/screen_gen.dmi";
const FOOD_DMI: &str = "icons/obj/food/burgerbread.dmi";
/// The mask moves at most this many icon pixels (`bar_offset` in `-20..=0`).
const MASK_RANGE: f32 = 20.0;
/// Screen pixels per icon pixel (zoom 2).
const ZOOM: f32 = 2.0;
/// `HUNGER_STATE_STARVING`, `HUNGER_STATE_FAT`.
const STARVING: u8 = 0;
const FAT: u8 = 5;

pub struct HungerHudPlugin;

impl Plugin for HungerHudPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_headless::<HungerHudUi>()
            .init_resource::<HungerHud>()
            .add_systems(Startup, spawn_bar)
            .add_systems(
                Update,
                (
                    receive_hunger,
                    draw_bar.run_if(resource_changed::<HungerHud>),
                    follow_map_insets.run_if(resource_changed::<MapInsets>),
                ),
            );
    }
}

/// The server's hunger window and its last data.
#[derive(Resource, Default, Debug)]
pub struct HungerHud {
    pub window: Option<u32>,
    pub data: HungerHudData,
}

#[derive(Component)]
struct HungerRoot;

/// The 32x32 slot all parts sit in.
#[derive(Component)]
struct HungerSlot;

#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum Part {
    Food,
    Frame,
    /// The clip box of the bar.
    Clip,
    Bar,
}

#[derive(Resource, Default)]
struct HungerStates {
    food: Option<IconStateId>,
    frame: Option<IconStateId>,
    bar: Option<IconStateId>,
}

/// tg `hunger_gradient` at `fullness` (clamped to 0..=1.2), linear in RGB like BYOND's
/// `gradient()`.
pub fn hunger_color(fullness: f32) -> [u8; 3] {
    const STOPS: [(f32, [u8; 3]); 7] = [
        (0.0, [0xFF, 0x00, 0x00]),
        (0.2, [0xFF, 0x80, 0x00]),
        (0.4, [0xF0, 0xF0, 0x00]),
        (0.6, [0x00, 0xFF, 0x00]),
        (0.8, [0x46, 0xDA, 0xFF]),
        (1.0, [0x2A, 0x72, 0xAA]),
        (1.2, [0x49, 0x49, 0x49]),
    ];
    let x = fullness.clamp(0.0, 1.2);
    for pair in STOPS.windows(2) {
        let ((a, ca), (b, cb)) = (pair[0], pair[1]);
        if x <= b {
            let t = (x - a) / (b - a);
            let mix = |i: usize| {
                (f32::from(ca[i]) + (f32::from(cb[i]) - f32::from(ca[i])) * t).round() as u8
            };
            return [mix(0), mix(1), mix(2)];
        }
    }
    STOPS[6].1
}

/// Icon pixels the mask hides at the top of the bar (`-bar_offset`).
pub fn hidden_pixels(fullness: f32) -> f32 {
    -(-MASK_RANGE + MASK_RANGE * fullness).clamp(-MASK_RANGE, 0.0)
}

fn spawn_bar(atlas: Option<Res<IconAtlas>>, mut commands: Commands) {
    let index = atlas
        .as_ref()
        .map(|a| a.manifest.index())
        .unwrap_or_default();
    let get = |dmi: &str, state: &str| index.get(&(dmi, state)).copied();
    commands.insert_resource(HungerStates {
        food: get(FOOD_DMI, "hburger"),
        frame: get(GEN_DMI, "hungerbar"),
        bar: get(GEN_DMI, "hungerbar_bar"),
    });
    let root = commands
        .spawn((
            HungerRoot,
            Name::new("hunger"),
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
            HungerSlot,
            Name::new("hunger bar"),
            screen_loc(ScreenAxis(1, -1, 2), ScreenAxis(0, 0, 21)),
            Visibility::Hidden,
            Pickable::IGNORE,
            ChildOf(root),
        ))
        .id();
    let full = |node: Node| Node {
        position_type: PositionType::Absolute,
        width: percent(100),
        height: percent(100),
        ..node
    };
    // `food_image` underlay: `pixel_x = -5`.
    commands.spawn((
        Part::Food,
        full(Node {
            left: px(-5.0 * ZOOM),
            ..default()
        }),
        ImageNode::default(),
        Pickable::IGNORE,
        ChildOf(slot),
    ));
    commands.spawn((
        Part::Frame,
        full(Node::default()),
        ImageNode::default(),
        Pickable::IGNORE,
        ChildOf(slot),
    ));
    let clip = commands
        .spawn((
            Part::Clip,
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                right: px(0),
                bottom: px(0),
                top: px(0),
                overflow: Overflow::clip(),
                ..default()
            },
            Pickable::IGNORE,
            ChildOf(slot),
        ))
        .id();
    commands.spawn((
        Part::Bar,
        Node {
            position_type: PositionType::Absolute,
            left: px(0),
            right: px(0),
            bottom: px(0),
            height: px(32.0 * ZOOM),
            ..default()
        },
        ImageNode::default(),
        Pickable::IGNORE,
        ChildOf(clip),
    ));
}

fn receive_hunger(mut received: MessageReader<UiMessageReceived>, mut hud: ResMut<HungerHud>) {
    for UiMessageReceived(message) in received.read() {
        let data = match message {
            UiServerMessage::Open {
                window,
                interface,
                data,
                ..
            } if interface == HungerHudUi::NAME => {
                hud.window = Some(*window);
                Some(data)
            }
            UiServerMessage::Update {
                window,
                data: Some(data),
                ..
            } if hud.window == Some(*window) => Some(data),
            UiServerMessage::Close { window } if hud.window == Some(*window) => {
                *hud = HungerHud::default();
                None
            }
            _ => None,
        };
        if let Some(data) = data.filter(|d| !d.is_empty()) {
            match decode::<HungerHudData>(data) {
                Ok(data) => hud.data = data,
                Err(e) => warn!("undecodable hunger data: {e}"),
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

fn draw_bar(
    hud: Res<HungerHud>,
    states: Res<HungerStates>,
    mut atlas: Option<ResMut<IconAtlas>>,
    mut slot: Single<&mut Visibility, With<HungerSlot>>,
    mut parts: Query<(&Part, &mut Node, Option<&mut ImageNode>)>,
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
    let data = hud.data;
    let [r, g, b] = hunger_color(data.bar);
    for (part, mut node, image) in &mut parts {
        let (id, tint) = match part {
            Part::Clip => {
                node.top = px(hidden_pixels(data.bar) * ZOOM);
                continue;
            }
            Part::Food => (
                states.food,
                (data.state == FAT).then(|| Color::srgb_u8(0x45, 0x45, 0x45)),
            ),
            Part::Frame => (
                states.frame,
                (data.state == STARVING).then(|| Color::srgb_u8(0xFF, 0x60, 0x70)),
            ),
            Part::Bar => (states.bar, Some(Color::srgb_u8(r, g, b))),
        };
        if let (Some(mut slot), Some(mut drawn)) = (image, sprite(&mut atlas, id)) {
            if let Some(tint) = tint {
                drawn.color = tint;
            }
            *slot = drawn;
        }
    }
}

/// The bar stays on the map viewport, left of the docked chat.
fn follow_map_insets(insets: Res<MapInsets>, mut roots: Query<&mut Node, With<HungerRoot>>) {
    for mut node in &mut roots {
        node.left = px(insets.left);
        node.right = px(insets.right);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bar_follows_tg_gradient_and_mask() {
        assert_eq!(hunger_color(0.0), [0xFF, 0, 0]);
        assert_eq!(hunger_color(0.1), [0xFF, 0x40, 0]);
        assert_eq!(hunger_color(1.0), [0x2A, 0x72, 0xAA]);
        assert_eq!(hunger_color(5.0), [0x49, 0x49, 0x49]);
        assert_eq!(hidden_pixels(0.0), 20.0);
        assert_eq!(hidden_pixels(0.5), 10.0);
        assert_eq!(hidden_pixels(1.3), 0.0);
    }
}
