//! Client half of the inventory: tg's human HUD slots at their `screen_loc` (tg Midnight slot
//! sprites): the bottom row (suit storage, ID, belt, back, hands, pockets) always, the pop-up
//! clothes (`HUD_GROUP_TOGGLEABLE_INVENTORY`) while the toggle button at the bottom left shows
//! them (tg `/atom/movable/screen/human/toggle`, hidden at first). The tg default keys X (swap
//! hands) and Q (drop); clicks on slots as `ClientIntent::ClickSlot`, alt+clicks as
//! `ClientIntent::AltClickSlot`.
//!
//! The HUD only draws what the server sends ([`HudData`] through the headless tgui interface
//! [`InventoryHudUi`]); it never changes state by itself.

use bevy::input_focus::InputFocus;
use bevy::picking::events::PointerClick;
use bevy::picking::pointer::PointerButton;
use bevy::prelude::*;
use bevy::text::EditableText;
use ss13_net::client::{NetEntityMap, SendIntent};
use ss13_net::ui::client::UiMessageReceived;
use ss13_render::atlas::frame_rect;
use ss13_render::{IconAtlas, MapInsets};
use ss13_shared::Dir;
use ss13_shared::protocol::{ClientIntent, InventorySlot};
use ss13_shared::ui::UiServerMessage;
use ss13_tgui::client::AppTguiClientExt;
use ss13_tgui::{UiInterface, decode};

use crate::hud::{HudData, HudIcon, InventoryHudUi};

/// One 32 px HUD tile at the map's zoom 2.
const SLOT_PX: f32 = 64.0;
/// `ui_style` default (`GLOB.available_ui_styles`).
const STYLE_DMI: &str = "icons/hud/screen_midnight.dmi";

/// A tg `screen_loc` coordinate: the anchor (`WEST`/`SOUTH` = -1, `CENTER` = 0,
/// `EAST`/`NORTH` = 1), whole tiles from it and pixels into the tile (at zoom 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScreenAxis(pub i8, pub i32, pub i32);

/// The node offsets of a 32 px element at `x`, `y` inside the map viewport (zoom 2): edges are
/// anchored to the viewport's edges, `CENTER` to its middle (tg: the middle tile's lower left
/// corner is half a tile left of and below the centre of a 19x15 view).
pub fn screen_loc(x: ScreenAxis, y: ScreenAxis) -> Node {
    let offset = |a: ScreenAxis| (a.1 * 32 + a.2) as f32 * 2.0;
    let mut node = Node {
        position_type: PositionType::Absolute,
        width: px(SLOT_PX),
        height: px(SLOT_PX),
        ..default()
    };
    match x.0 {
        ..0 => node.left = px(offset(x)),
        0 => {
            node.left = percent(50);
            node.margin.left = px(offset(x) - SLOT_PX / 2.0);
        }
        _ => node.right = px(-offset(x)),
    }
    match y.0 {
        ..0 => node.bottom = px(offset(y)),
        0 => {
            node.bottom = percent(50);
            node.margin.bottom = px(offset(y) - SLOT_PX / 2.0);
        }
        _ => node.top = px(-offset(y)),
    }
    node
}

const WEST: i8 = -1;
const CENTER: i8 = 0;
const SOUTH: i8 = -1;

/// One HUD slot: tg background state, node name, `screen_loc`, toggleable.
struct SlotDef {
    slot: InventorySlot,
    state: &'static str,
    name: &'static str,
    x: ScreenAxis,
    y: ScreenAxis,
    toggleable: bool,
}

const fn def(
    slot: InventorySlot,
    state: &'static str,
    name: &'static str,
    x: (i8, i32, i32),
    y: (i8, i32, i32),
    toggleable: bool,
) -> SlotDef {
    SlotDef {
        slot,
        state,
        name,
        x: ScreenAxis(x.0, x.1, x.2),
        y: ScreenAxis(y.0, y.1, y.2),
        toggleable,
    }
}

/// `/datum/inventory_slot/human/*` and `ui_hand_position` (`code/__DEFINES/hud.dm`).
const SLOTS: [SlotDef; 17] = [
    def(
        InventorySlot::SuitStorage,
        "suit_storage",
        "hud_suit_storage",
        (CENTER, -5, 10),
        (SOUTH, 0, 5),
        false,
    ),
    def(
        InventorySlot::Id,
        "id",
        "hud_id",
        (CENTER, -4, 12),
        (SOUTH, 0, 5),
        false,
    ),
    def(
        InventorySlot::Belt,
        "belt",
        "hud_belt",
        (CENTER, -3, 14),
        (SOUTH, 0, 5),
        false,
    ),
    def(
        InventorySlot::Back,
        "back",
        "hud_back",
        (CENTER, -2, 14),
        (SOUTH, 0, 5),
        false,
    ),
    def(
        InventorySlot::RightHand,
        "hand_r",
        "hud_right_hand",
        (CENTER, -1, 16),
        (SOUTH, 0, 5),
        false,
    ),
    def(
        InventorySlot::LeftHand,
        "hand_l",
        "hud_left_hand",
        (CENTER, 0, 16),
        (SOUTH, 0, 5),
        false,
    ),
    def(
        InventorySlot::LeftPocket,
        "pocket",
        "hud_left_pocket",
        (CENTER, 1, 18),
        (SOUTH, 0, 5),
        false,
    ),
    def(
        InventorySlot::RightPocket,
        "pocket",
        "hud_right_pocket",
        (CENTER, 2, 20),
        (SOUTH, 0, 5),
        false,
    ),
    def(
        InventorySlot::Shoes,
        "shoes",
        "hud_shoes",
        (WEST, 1, 8),
        (SOUTH, 0, 5),
        true,
    ),
    def(
        InventorySlot::Uniform,
        "uniform",
        "hud_uniform",
        (WEST, 0, 6),
        (SOUTH, 1, 7),
        true,
    ),
    def(
        InventorySlot::Suit,
        "suit",
        "hud_suit",
        (WEST, 1, 8),
        (SOUTH, 1, 7),
        true,
    ),
    def(
        InventorySlot::Gloves,
        "gloves",
        "hud_gloves",
        (WEST, 2, 10),
        (SOUTH, 1, 7),
        true,
    ),
    def(
        InventorySlot::Neck,
        "neck",
        "hud_neck",
        (WEST, 0, 6),
        (SOUTH, 2, 9),
        true,
    ),
    def(
        InventorySlot::Mask,
        "mask",
        "hud_mask",
        (WEST, 1, 8),
        (SOUTH, 2, 9),
        true,
    ),
    def(
        InventorySlot::Ears,
        "ears",
        "hud_ears",
        (WEST, 2, 10),
        (SOUTH, 2, 9),
        true,
    ),
    def(
        InventorySlot::Eyes,
        "glasses",
        "hud_eyes",
        (WEST, 0, 6),
        (SOUTH, 3, 11),
        true,
    ),
    def(
        InventorySlot::Head,
        "head",
        "hud_head",
        (WEST, 1, 8),
        (SOUTH, 3, 11),
        true,
    ),
];

/// `ui_inventory`: the toggle button.
const TOGGLE: (ScreenAxis, ScreenAxis) = (ScreenAxis(WEST, 0, 6), ScreenAxis(SOUTH, 0, 5));

pub struct InventoryHudPlugin;

impl Plugin for InventoryHudPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_headless::<InventoryHudUi>()
            .init_resource::<Hud>()
            .init_resource::<InventoryShown>()
            .add_systems(Startup, spawn_hud)
            .add_systems(
                Update,
                (
                    send_inventory_keys,
                    receive_hud,
                    draw_hud.run_if(resource_changed::<Hud>),
                    show_toggleable.run_if(resource_changed::<InventoryShown>),
                    follow_map_insets.run_if(resource_changed::<MapInsets>),
                ),
            );
    }
}

/// The server's HUD window and its last data.
#[derive(Resource, Default, Debug)]
pub struct Hud {
    pub window: Option<u32>,
    pub data: HudData,
}

/// tg `hud_used.inventory_shown`: the pop-up clothes slots are on screen.
#[derive(Resource, Default, Debug)]
pub struct InventoryShown(pub bool);

#[derive(Component)]
#[require(crate::client_keys::HudInventory)]
struct HudRoot;

#[derive(Component, Clone, Copy)]
pub(crate) struct HudSlot(pub(crate) InventorySlot);

/// A slot of `HUD_GROUP_TOGGLEABLE_INVENTORY`.
#[derive(Component)]
struct Toggleable;

/// The held or worn item's icon inside a slot.
#[derive(Component)]
struct SlotItem(InventorySlot);

/// tg `lhandactive`/`rhandactive` over the active hand.
#[derive(Component)]
struct ActiveMark(usize);

fn spawn_hud(mut atlas: Option<ResMut<IconAtlas>>, mut commands: Commands) {
    let index: std::collections::HashMap<String, ss13_shared::IconStateId> = atlas
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
    let mut sprite = |state: &str| -> Option<ImageNode> {
        let atlas = atlas.as_mut()?;
        let (info, image) = atlas.state(*index.get(state)?)?;
        Some(ImageNode {
            image: image.clone(),
            rect: frame_rect(info, Dir::South, 0),
            ..default()
        })
    };
    let fill = |extra: Option<ImageNode>| {
        (
            Node {
                position_type: PositionType::Absolute,
                width: px(SLOT_PX),
                height: px(SLOT_PX),
                ..default()
            },
            extra.unwrap_or_default(),
            Pickable::IGNORE,
        )
    };
    let root = commands
        .spawn((
            HudRoot,
            Name::new("hud"),
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
    let dark = BackgroundColor(Color::srgba(0.1, 0.1, 0.15, 0.8));
    let mut toggle = commands.spawn((
        Name::new("hud_toggle"),
        screen_loc(TOGGLE.0, TOGGLE.1),
        ChildOf(root),
    ));
    match sprite("toggle") {
        Some(image) => toggle.insert(image),
        None => toggle.insert(dark),
    };
    toggle.observe(click_toggle);
    for def in &SLOTS {
        let slot = def.slot;
        let mut entity = commands.spawn((
            HudSlot(slot),
            Name::new(def.name),
            screen_loc(def.x, def.y),
            ChildOf(root),
        ));
        if def.toggleable {
            entity.insert((Toggleable, Visibility::Hidden));
        }
        match sprite(def.state) {
            Some(image) => entity.insert(image),
            // Without assets: a plain dark tile.
            None => entity.insert(dark),
        };
        entity.observe(click_slot);
        let slot_entity = entity.id();
        commands.spawn((
            fill(None),
            Visibility::Hidden,
            SlotItem(slot),
            ChildOf(slot_entity),
        ));
        let hand = match slot {
            InventorySlot::LeftHand => Some((0, "lhandactive")),
            InventorySlot::RightHand => Some((1, "rhandactive")),
            _ => None,
        };
        if let Some((hand, state)) = hand {
            let mark = sprite(state);
            let outline = mark
                .is_none()
                .then(|| Outline::new(px(2), px(0), Color::WHITE));
            let mut mark = commands.spawn((
                fill(mark),
                Visibility::Hidden,
                ActiveMark(hand),
                ChildOf(slot_entity),
            ));
            if let Some(outline) = outline {
                mark.insert(outline);
            }
        }
    }
}

/// tg `/atom/movable/screen/human/toggle/Click`.
fn click_toggle(click: On<PointerClick>, mut shown: ResMut<InventoryShown>) {
    if click.button == PointerButton::Primary {
        shown.0 = !shown.0;
    }
}

fn show_toggleable(
    shown: Res<InventoryShown>,
    mut slots: Query<&mut Visibility, (With<Toggleable>, With<HudSlot>)>,
) {
    for mut visibility in &mut slots {
        *visibility = if shown.0 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

fn click_slot(
    click: On<PointerClick>,
    slots: Query<&HudSlot>,
    keys: Res<ButtonInput<KeyCode>>,
    map: Res<NetEntityMap>,
    mut intents: MessageWriter<SendIntent>,
) {
    if map.controlled().is_none() {
        return;
    }
    if let Ok(&HudSlot(slot)) = slots.get(click.entity) {
        let intent = if click.button == PointerButton::Secondary {
            ClientIntent::RightClickSlot(slot)
        } else if click.button != PointerButton::Primary {
            return;
        } else if keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]) {
            ClientIntent::AltClickSlot(slot)
        } else {
            ClientIntent::ClickSlot(slot)
        };
        intents.write(SendIntent(intent));
    }
}

/// X swaps hands, Q drops, 1-3 / - / = the silicon keys, unless a text field has the keyboard.
fn send_inventory_keys(
    keys: Res<ButtonInput<KeyCode>>,
    focus: Option<Res<InputFocus>>,
    text_fields: Query<(), With<EditableText>>,
    map: Res<NetEntityMap>,
    hud: Res<Hud>,
    mut intents: MessageWriter<SendIntent>,
) {
    let typing = focus
        .and_then(|f| f.get())
        .is_some_and(|e| text_fields.contains(e));
    if typing || map.controlled().is_none() {
        return;
    }
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    let alt = keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]);
    let smart = |slot| SendIntent(ClientIntent::SmartEquip(slot));
    if keys.just_pressed(KeyCode::KeyX) {
        intents.write(SendIntent(ClientIntent::SwapHands));
    }
    // `keybinding/dextrous/activate_inhand` (tg `mode()`): the active hand's slot clicked uses
    // its item (`attack_self`).
    if keys.just_pressed(KeyCode::KeyZ) && hud.window.is_some() {
        let hand = if hud.data.active == 1 {
            InventorySlot::RightHand
        } else {
            InventorySlot::LeftHand
        };
        intents.write(SendIntent(ClientIntent::ClickSlot(hand)));
    }
    if keys.just_pressed(KeyCode::KeyQ) {
        // `keybinding/human/quick_equip_suit_storage`, else tg's drop.
        intents.write(if shift {
            smart(InventorySlot::SuitStorage)
        } else {
            SendIntent(ClientIntent::DropActive)
        });
    }
    // `keybinding/human`: E, Shift+E (belt), Shift+V (back), Alt+1 and Alt+2 (pockets), Shift+F.
    if keys.just_pressed(KeyCode::KeyE) {
        intents.write(if shift {
            smart(InventorySlot::Belt)
        } else {
            SendIntent(ClientIntent::QuickEquip)
        });
    }
    if shift && keys.just_pressed(KeyCode::KeyV) {
        intents.write(smart(InventorySlot::Back));
    }
    // BandaStation `keybinding/living/give`: V.
    if !shift && !alt && keys.just_pressed(KeyCode::KeyV) {
        intents.write(SendIntent(ClientIntent::GiveItem));
    }
    if alt && keys.just_pressed(KeyCode::Digit1) {
        intents.write(smart(InventorySlot::LeftPocket));
    }
    if alt && keys.just_pressed(KeyCode::Digit2) {
        intents.write(smart(InventorySlot::RightPocket));
    }
    if shift && keys.just_pressed(KeyCode::KeyF) {
        intents.write(SendIntent(ClientIntent::QuickAccessoryDraw));
    }
    // `keybinding/robot` and `keybinding/artificial_intelligence`: the server ignores them for
    // other mobs (their `can_use`).
    if !alt {
        for (n, key) in [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3]
            .into_iter()
            .enumerate()
        {
            if keys.just_pressed(key) {
                intents.write(SendIntent(ClientIntent::ToggleModule(n as u8 + 1)));
            }
        }
    }
    if keys.just_pressed(KeyCode::Minus) {
        intents.write(SendIntent(ClientIntent::ReconnectShell));
    }
    if keys.just_pressed(KeyCode::Equal) {
        intents.write(SendIntent(ClientIntent::UndeployShell));
    }
}

fn receive_hud(mut received: MessageReader<UiMessageReceived>, mut hud: ResMut<Hud>) {
    for UiMessageReceived(message) in received.read() {
        let data = match message {
            UiServerMessage::Open {
                window,
                interface,
                data,
                ..
            } if interface == InventoryHudUi::NAME => {
                hud.window = Some(*window);
                Some(data)
            }
            UiServerMessage::Update {
                window,
                data: Some(data),
                ..
            } if hud.window == Some(*window) => Some(data),
            UiServerMessage::Close { window } if hud.window == Some(*window) => {
                *hud = Hud::default();
                None
            }
            _ => None,
        };
        // Empty data: sent while the window is below `UiStatus::Update`.
        if let Some(data) = data.filter(|d| !d.is_empty()) {
            match decode::<HudData>(data) {
                Ok(data) => hud.data = data,
                Err(e) => warn!("undecodable HUD data: {e}"),
            }
        }
    }
}

fn draw_hud(
    hud: Res<Hud>,
    mut atlas: Option<ResMut<IconAtlas>>,
    mut items: Query<(&SlotItem, &mut ImageNode, &mut Visibility), Without<ActiveMark>>,
    mut marks: Query<(&ActiveMark, &mut Visibility), Without<SlotItem>>,
) {
    let data = &hud.data;
    for (SlotItem(slot), mut image, mut visibility) in &mut items {
        let icon = data.slot(*slot);
        let drawn = icon.and_then(|icon| item_image(atlas.as_deref_mut()?, icon));
        *visibility = match drawn {
            Some(drawn) => {
                *image = drawn;
                Visibility::Inherited
            }
            None => Visibility::Hidden,
        };
    }
    let active = hud.window.map(|_| usize::from(data.active));
    for (ActiveMark(hand), mut visibility) in &mut marks {
        *visibility = if active == Some(*hand) {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

// shortcut: the base icon only; item overlays and animation frames are not drawn on the HUD.
fn item_image(atlas: &mut IconAtlas, icon: HudIcon) -> Option<ImageNode> {
    let (info, image) = atlas.state(icon.icon_state)?;
    let color = icon
        .color
        .map_or(Color::WHITE, |[r, g, b, a]| Color::srgba_u8(r, g, b, a));
    Some(ImageNode {
        image: image.clone(),
        rect: frame_rect(info, Dir::South, 0),
        color,
        ..default()
    })
}

/// The slots stay inside the map viewport, left of the docked chat.
fn follow_map_insets(insets: Res<MapInsets>, mut roots: Query<&mut Node, With<HudRoot>>) {
    for mut node in &mut roots {
        node.left = px(insets.left);
        node.right = px(insets.right);
    }
}
