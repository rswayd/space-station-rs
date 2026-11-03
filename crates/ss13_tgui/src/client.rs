//! Client half: spawns `ss13_ui_kit` windows for server windows and sends their acts.
//!
//! Each interface registers a view ([`ClientUi`]) under its tgui name with
//! [`AppTguiClientExt::register_ui_view`]; an `Open` for an unregistered name is answered with a
//! close. The view is re-rendered from scratch on every data change, like a React render without
//! reconciliation: the `Window` root (position, size, z-order) stays, its content is respawned.
//! A new render is laid out hidden next to the shown one and swapped in once its layout settled
//! ([`ss13_ui_kit::LayoutFeedback`] widgets need a frame after the first layout), so no frame
//! shows a half-laid-out tree; a new window stays hidden until its first content is ready.
//! A view marks subtrees that did not change with [`memo`]; they move into the new render
//! instead of being spawned again. New windows open centered, offset when they would cover another one exactly, and where the
//! player last dragged that interface ([`place_window`]).

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::hash::Hash;

use bevy::camera::visibility::VisibilitySystems;
use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input_focus::{FocusedInput, InputFocus};
use bevy::picking::events::PointerClick;
use bevy::picking::pointer::PointerButton;
use bevy::prelude::*;
use bevy::scene::{CommandsSceneExt, EntityScene, Scene, SceneList};
use bevy::text::EditableText;
use bevy::ui::FocusPolicy;
use bevy::ui::ScrollPosition;
use bevy::ui_widgets::Activate;
use ss13_net::client::{NetClientState, SendIntent};
use ss13_net::ui::client::{SendUi, UiMessageReceived};
use ss13_shared::IconStateId;
use ss13_shared::protocol::ClientIntent;
use ss13_shared::ui::{UiClientMessage, UiServerMessage, UiStatus};
use ss13_ui_kit::layout::Slot;
use ss13_ui_kit::scroll::ScrollContent;
use ss13_ui_kit::{
    InputChanged, InputSubmitted, LayoutFeedback, NumberInputChanged, UiAct, WindowClose, theme,
};

use crate::{UiInterface, decode};

/// The client side of a tgui interface: its JSX, as a scene of the window content.
pub trait ClientUi: UiInterface {
    /// `<Window width height>`.
    const WIDTH: f32;
    const HEIGHT: f32;

    /// `<Window width height>` computed from the data (tgui windows whose height follows their
    /// content, `Wires`); read on open.
    fn size(static_data: &Self::StaticData, data: &Self::Data) -> (f32, f32) {
        let _ = (static_data, data);
        (Self::WIDTH, Self::HEIGHT)
    }

    /// The `<Window.Content>` subtree for this data.
    fn content(static_data: &Self::StaticData, data: &Self::Data) -> impl Scene;

    /// [`Self::content`] for views with client-only state (tgui `useState`): `local` survives
    /// re-renders. Views without such state keep the default.
    fn content_local(
        static_data: &Self::StaticData,
        data: &Self::Data,
        local: &UiLocalState,
    ) -> impl Scene {
        let _ = local;
        Self::content(static_data, data)
    }
}

/// Client-only window state (tgui `useState`), string-keyed; on the window root, kept across
/// re-renders and dropped with the window. Changing it does not re-render by itself.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct UiLocalState(HashMap<String, String>);

impl UiLocalState {
    /// `""` when unset.
    pub fn get(&self, key: &str) -> &str {
        self.0.get(key).map_or("", String::as_str)
    }

    /// `None` when unset, unlike [`Self::get`].
    pub fn value(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(String::as_str)
    }

    pub fn set(&mut self, key: &str, value: &str) {
        self.0.insert(key.to_owned(), value.to_owned());
    }
}

/// On a button: activating it sets `key` of its window's [`UiLocalState`] to `value`
/// (`onClick={() => setState(value)}`), and re-renders the window when `rerender`.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct SetLocal {
    pub key: String,
    pub value: String,
    pub rerender: bool,
}

/// On an `Input`: every edit is stored under this key of the window's [`UiLocalState`]
/// (`onChange={setState}`).
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct LocalOnInput(pub String);

/// An atlas sprite inside a window (tgui `DmIcon`): the client's renderer gives the node its
/// `ImageNode` (first frame, facing `dir`, south when unset).
#[derive(Component, Clone, Debug, Default, PartialEq)]
#[require(Node)]
pub struct UiSprite {
    pub icon_state: Option<IconStateId>,
    pub color: Option<[u8; 4]>,
    pub dir: Option<ss13_shared::Dir>,
}

/// The root of a subtree that stays the same while its key does (React `memo` with a key): a
/// re-render reuses the shown entities instead of spawning them again. Put by [`memo`].
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Memo(pub u64);

/// Stands in a pending render for a [`Memo`] subtree of the shown one, at its laid-out size;
/// the shown subtree moves here when the render is swapped in.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
struct MemoSlot(u64);

thread_local! {
    /// Keys of the shown render's [`Memo`] subtrees and their stand-in nodes, while a view runs.
    static REUSABLE: RefCell<HashMap<u64, Node>> = RefCell::default();
}

/// A subtree built by `build`, reused from the previous render when that one had the same `key`
/// (then `build` is not called). The key must change whenever what `build` shows would: derive
/// it from the inputs (static data, the parts of the data it reads). Call it while the view
/// builds its scene, not inside a closure the scene runs later.
pub fn memo<S: Scene>(key: u64, build: impl FnOnce() -> S) -> Box<dyn SceneList> {
    match REUSABLE.with(|r| r.borrow_mut().remove(&key)) {
        Some(node) => {
            let slot = MemoSlot(key);
            Box::new(EntityScene(bsn! { node slot }))
        }
        None => {
            let memo = Memo(key);
            Box::new(EntityScene((build(), bsn! { memo })))
        }
    }
}

/// [`memo`] keyed by what the subtree is drawn from: `build` gets `input` alone (a `fn`, so it
/// cannot read anything else), and the previous render's subtree is reused while `input` hashes
/// the same. Put what a row shows into `input` (computed values, not whole records) so that it
/// changes exactly when the row would.
pub fn memo_of<D: std::hash::Hash, S: Scene>(input: &D, build: fn(&D) -> S) -> Box<dyn SceneList> {
    use std::hash::{BuildHasher, Hasher};
    // A fixed-key hasher: the key must be the same across renders of one session.
    let mut hasher = bevy::platform::hash::FixedHasher.build_hasher();
    // The builder tells two kinds of subtrees with equal inputs apart.
    hasher.write_usize(build as usize);
    std::any::type_name::<D>().hash(&mut hasher);
    input.hash(&mut hasher);
    memo(hasher.finish(), || build(input))
}

/// On a `NumberInput`: `onChange={(value) => act(action, {[param]: value})}`; an `ActParams`
/// on the same entity adds its params first.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct ActOnChange {
    pub action: String,
    pub param: String,
}

/// On an `Input`: Enter sends `act(action, {...params, [param]: text})` (tgui `onEnter`). Empty
/// text sends nothing.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct ActOnSubmit {
    pub action: String,
    pub param: String,
    pub params: Vec<(String, String)>,
}

/// On a button with an `Act`: adds `[param]: local[key]` of the window's [`UiLocalState`] to
/// the act, `default` while `key` is unset (tgui `onClick={() => act('submit', { entry: input })}`
/// with `useState(default)`).
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct ActWithLocal {
    pub param: String,
    pub key: String,
    pub default: String,
}

/// On a view's root node: keys pressed while the window has keyboard focus send
/// `act(action, params)` (tgui `<Window.Content onKeyDown>`). Text fields keep Enter to
/// themselves ([`ActOnSubmit`]); list it here only for windows without one.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct ActOnKey(pub Vec<(KeyCode, String, Vec<(String, String)>)>);

/// On a view's root node: keys pressed while the window has keyboard focus set `key` of its
/// [`UiLocalState`] to the paired value and re-render (`setSelected(next)`).
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct KeySetLocal {
    pub key: String,
    pub keys: Vec<(KeyCode, String)>,
}

/// On a row button: shown selected while the window's local `key` is `value` (or, while `key`
/// is unset, when `default`). Updated in place: a click or arrow key needs no re-render, and a
/// focused search field would hold one back.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct SelectRow {
    pub key: String,
    pub value: String,
    pub default: bool,
}

/// On a view's root: keyboard choice in a list (tgui `ListInputModal`). Up/Down step through
/// the items the search (`search_key`) leaves, Enter sends `submit` with the choice, and in
/// hotkey mode a letter selects the first item starting with it. The choice is local `key`,
/// `default` while unset.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct ListNav {
    pub key: String,
    pub default: String,
    pub search_key: String,
    pub items: Vec<String>,
    pub hotkeys: bool,
    pub submit: String,
}

/// On a button: a double click sends `act(action, {[param]: value})` (`onDoubleClick`).
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct ActOnDoubleClick {
    pub action: String,
    pub param: String,
    pub value: String,
}

/// On a button: activating it unsets these keys of its window's [`UiLocalState`] and
/// re-renders (back to the `useState` defaults).
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct ClearLocal(pub Vec<String>);

/// On a view's root node: when `true`, gives the window keyboard focus when it is rendered
/// (tgui `<Autofocus />`), so [`ActOnKey`] works without a click first.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct AutoFocusWindow(pub bool);

/// On a row: shown only while its `text` contains the window's local `key` (tgui
/// `createSearch`, case-insensitive), filtered in place since typing does not re-render.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct SearchRow {
    pub key: String,
    /// Lowercase.
    pub text: String,
}

/// `ss13_tgui::server::MAX_PARAM_LEN`: longer text goes out as `param`, `param.1`, ... and the
/// server joins it back (`UiParams::text`).
const PARAM_BYTES: usize = 256;

/// `text` cut into pieces of at most `max` bytes at char boundaries.
fn chunks(text: &str, max: usize) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        let mut end = rest.len().min(max);
        while !rest.is_char_boundary(end) {
            end -= 1;
        }
        out.push(&rest[..end]);
        rest = &rest[end..];
    }
    out
}

/// `text` as the params `UiParams::text(param)` reads back. Empty text is one empty param.
fn text_params(param: &str, text: &str) -> Vec<(String, String)> {
    let parts = chunks(text, PARAM_BYTES);
    if parts.is_empty() {
        return vec![(param.to_owned(), String::new())];
    }
    parts
        .into_iter()
        .enumerate()
        .map(|(i, part)| {
            let key = match i {
                0 => param.to_owned(),
                i => format!("{param}.{i}"),
            };
            (key, part.to_owned())
        })
        .collect()
}

pub trait AppTguiClientExt {
    fn register_ui_view<T: ClientUi>(&mut self) -> &mut Self;
    /// An interface drawn by game code instead of a window (a HUD): its messages are left to
    /// systems reading [`UiMessageReceived`], and it is not closed as unknown.
    fn register_ui_headless<T: UiInterface>(&mut self) -> &mut Self;
}

impl AppTguiClientExt for App {
    fn register_ui_headless<T: UiInterface>(&mut self) -> &mut Self {
        self.init_resource::<HeadlessUis>()
            .world_mut()
            .resource_mut::<HeadlessUis>()
            .0
            .insert(T::NAME);
        self
    }

    fn register_ui_view<T: ClientUi>(&mut self) -> &mut Self {
        self.init_resource::<UiViews>()
            .world_mut()
            .resource_mut::<UiViews>()
            .0
            .insert(
                T::NAME,
                View {
                    width: T::WIDTH,
                    height: T::HEIGHT,
                    size: size::<T>,
                    render: render::<T>,
                },
            );
        self
    }
}

/// The names of the views this app can draw, sorted (the `ui_gallery` list).
pub fn view_names(world: &World) -> Vec<&'static str> {
    let mut names: Vec<&'static str> = world
        .get_resource::<UiViews>()
        .map(|v| v.0.keys().copied().collect())
        .unwrap_or_default();
    names.sort_unstable();
    names
}

/// Whether `static_data` and `data` decode as the view `name` expects.
pub fn view_decodes(world: &World, name: &str, static_data: &[u8], data: &[u8]) -> bool {
    world
        .get_resource::<UiViews>()
        .and_then(|v| v.0.get(name))
        .is_some_and(|view| (view.size)(static_data, data).is_some())
}

type Render = fn(&[u8], &[u8], &UiLocalState) -> Result<Box<dyn Scene>, postcard::Error>;

fn size<T: ClientUi>(static_data: &[u8], data: &[u8]) -> Option<(f32, f32)> {
    Some(T::size(
        &decode::<T::StaticData>(static_data).ok()?,
        &decode::<T::Data>(data).ok()?,
    ))
}

fn render<T: ClientUi>(
    static_data: &[u8],
    data: &[u8],
    local: &UiLocalState,
) -> Result<Box<dyn Scene>, postcard::Error> {
    let static_data = decode::<T::StaticData>(static_data)?;
    let data = decode::<T::Data>(data)?;
    Ok(Box::new(T::content_local(&static_data, &data, local)))
}

struct View {
    width: f32,
    height: f32,
    size: fn(&[u8], &[u8]) -> Option<(f32, f32)>,
    render: Render,
}

/// Interface name → view.
#[derive(Resource, Default)]
struct UiViews(HashMap<&'static str, View>);

#[derive(Resource, Default)]
struct HeadlessUis(HashSet<&'static str>);

/// A window opened by the server. On the `ss13_ui_kit::Window` root.
#[derive(Component, Debug)]
pub struct ClientWindow {
    pub id: u32,
    pub interface: String,
    pub status: UiStatus,
    static_data: Vec<u8>,
    data: Vec<u8>,
    /// The shown render.
    content: Option<Entity>,
    /// The next render, hidden until its layout settles, and the frames it has waited.
    pending: Option<(Entity, u8)>,
    /// A settled render with [`MemoSlot`]s, swapped in next frame before the layout runs.
    ready: Option<Entity>,
    dimmer: Option<Entity>,
    dirty: bool,
    /// Scroll offsets of the previous render's scroll areas, in tree order, to restore on the
    /// new content (a respawn would jump back to the top on every stock update).
    scroll: Vec<Vec2>,
    /// Where it was opened (top-left, logical px): a different position was dragged to.
    placed: Vec2,
    /// [`AutoFocusWindow`] already took the keyboard (on the first render only, as tgui mounts).
    autofocused: bool,
}

impl ClientWindow {
    /// The shown render's root node (the view's `WindowContent`).
    pub fn content_root(&self, children: &Query<&Children>) -> Option<Entity> {
        children.get(self.content?).ok()?.first().copied()
    }

    /// Re-render with the current data and local state.
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }
}

/// Window positions the player dragged to, by interface, for this session (tgui
/// `storeWindowGeometry`; tg keys them by `REF(src_object)`, which the client does not know).
#[derive(Resource, Default)]
pub struct WindowPositions(pub HashMap<String, Vec2>);

/// Offset of a window that would open exactly on top of another one.
const CASCADE_PX: f32 = 30.0;
/// How much of a recalled window stays on screen: enough to grab its title bar.
const KEEP_VISIBLE_PX: Vec2 = Vec2::new(80.0, 32.0);

/// tgui `recallWindowGeometry`: a recalled position (kept on screen), else centered on the
/// screen; then moved down-right while it would sit exactly on an open window's corner.
pub fn place_window(size: Vec2, screen: Vec2, recalled: Option<Vec2>, open: &[Vec2]) -> Vec2 {
    let mut pos = match recalled {
        Some(pos) => pos.min(screen - KEEP_VISIBLE_PX).max(Vec2::ZERO),
        None => ((screen - size) / 2.0).max(Vec2::ZERO).round(),
    };
    for _ in 0..=open.len() {
        if !open.iter().any(|o| o.distance(pos) < 1.0) {
            break;
        }
        pos += Vec2::splat(CASCADE_PX);
    }
    pos
}

/// Server window id → window root.
#[derive(Resource, Default)]
pub struct ClientWindows(pub HashMap<u32, Entity>);

pub struct TguiClientPlugin;

impl Plugin for TguiClientPlugin {
    fn build(&self, app: &mut App) {
        // The client bundle knows every ported interface, as tgui's bundle does.
        app.register_ui_view::<crate::interfaces::thermo_machine::ThermoMachineUi>()
            .register_ui_view::<crate::interfaces::vending::VendingUi>()
            .register_ui_view::<crate::interfaces::job_selection::JobSelectionUi>()
            .register_ui_view::<crate::interfaces::wires::WiresUi>()
            .register_ui_view::<crate::interfaces::atmos::AtmosPumpUi>()
            .register_ui_view::<crate::interfaces::atmos::AtmosTempGateUi>()
            .register_ui_view::<crate::interfaces::gas_analyzer::GasAnalyzerUi>()
            .register_ui_view::<crate::interfaces::atmos_alert::AtmosAlertUi>()
            .register_ui_view::<crate::interfaces::atmos_control::AtmosControlUi>()
            .register_ui_view::<crate::interfaces::atmos::AtmosTempPumpUi>()
            .register_ui_view::<crate::interfaces::atmos::AtmosFilterUi>()
            .register_ui_view::<crate::interfaces::atmos::AtmosMixerUi>()
            .register_ui_view::<crate::interfaces::atmos::CanisterUi>()
            .register_ui_view::<crate::interfaces::atmos::PortablePumpUi>()
            .register_ui_view::<crate::interfaces::atmos::PortableScrubberUi>()
            .register_ui_view::<crate::interfaces::atmos::PipeScrubberUi>()
            .register_ui_view::<crate::interfaces::pipe_dispenser::PipeDispenserUi>()
            .register_ui_view::<crate::interfaces::rapid_pipe_dispenser::RapidPipeDispenserUi>()
            .register_ui_view::<crate::interfaces::air_alarm::AirAlarmUi>()
            .register_ui_view::<crate::interfaces::solar_control::SolarControlUi>()
            .register_ui_view::<crate::interfaces::mech_bay::MechBayPowerConsoleUi>()
            .register_ui_view::<crate::interfaces::ai_airlock::AiAirlockUi>()
            .register_ui_view::<crate::interfaces::apc::ApcUi>()
            .register_ui_view::<crate::interfaces::list_input::ListInputUi>()
            .register_ui_view::<crate::interfaces::alert::AlertUi>()
            .register_ui_view::<crate::interfaces::text_input::TextInputUi>()
            .register_ui_view::<crate::interfaces::number_input::NumberInputUi>()
            .register_ui_view::<crate::interfaces::checkbox_input::CheckboxInputUi>()
            .register_ui_view::<crate::interfaces::key_combo::KeyComboUi>()
            .register_ui_view::<crate::interfaces::color_picker::ColorPickerUi>()
            .register_ui_view::<crate::interfaces::disposal_unit::DisposalUnitUi>()
            .register_ui_view::<crate::interfaces::dna_console::DnaConsoleUi>()
            .register_ui_view::<crate::interfaces::destination_tagger::DestinationTaggerUi>();
        app.init_resource::<ClientWindows>()
            .init_resource::<HeadlessUis>()
            .init_resource::<WindowPositions>()
            .add_systems(
                Update,
                (
                    apply_messages,
                    close_all_on_disconnect.run_if(resource_changed::<NetClientState>),
                    swap_memos,
                    render_windows,
                    remember_positions,
                    sync_dimmers,
                    restore_scroll,
                    crate::interfaces::vending::filter_rows,
                    filter_search_rows,
                    sync_selected_rows,
                    autofocus_window,
                )
                    .chain(),
            )
            .add_systems(
                PostUpdate,
                show_settled
                    .after(LayoutFeedback)
                    .before(VisibilitySystems::VisibilityPropagate),
            )
            .add_observer(set_local)
            .add_observer(clear_local)
            .add_observer(crate::interfaces::key_combo::client_side::capture_combo)
            .add_plugins(crate::interfaces::color_picker::client_side::plugin)
            .add_observer(local_on_input)
            .add_observer(send_act)
            .add_observer(submit_act)
            .add_observer(key_acts)
            .add_observer(double_click_acts)
            .add_observer(send_number_act)
            .add_observer(send_close);
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_messages(
    mut received: MessageReader<UiMessageReceived>,
    views: Res<UiViews>,
    headless: Res<HeadlessUis>,
    mut windows: ResMut<ClientWindows>,
    mut roots: Query<(&mut ClientWindow, &Node)>,
    positions: Res<WindowPositions>,
    screens: Query<&bevy::window::Window, With<bevy::window::PrimaryWindow>>,
    scale: Option<Res<UiScale>>,
    mut send: MessageWriter<SendUi>,
    mut commands: Commands,
) {
    // Top-left corners of open windows, including the ones opened by this batch.
    let mut corners: Vec<(u32, Vec2)> = Vec::new();
    let mut corners_read = false;
    for UiMessageReceived(message) in received.read() {
        match message {
            UiServerMessage::Open {
                window: id,
                interface,
                title,
                status,
                static_data,
                data,
            } => {
                if let Some(old) = windows.0.remove(id) {
                    commands.entity(old).despawn();
                }
                if headless.0.contains(interface.as_str()) {
                    continue;
                }
                let Some(view) = views.0.get(interface.as_str()) else {
                    warn!("server opened unknown interface {interface:?}; closing");
                    send.write(SendUi(UiClientMessage::Close { window: *id }));
                    continue;
                };
                let (width, height) =
                    (view.size)(static_data, data).unwrap_or((view.width, view.height));
                if !corners_read {
                    corners_read = true;
                    corners = windows
                        .0
                        .iter()
                        .filter_map(|(id, e)| Some((*id, corner(roots.get(*e).ok()?.1)?)))
                        .collect();
                }
                corners.retain(|(open, _)| open != id);
                let screen = screens
                    .single()
                    .map_or(Vec2::new(1280.0, 720.0), |w| w.resolution.size())
                    / scale.as_ref().map_or(1.0, |s| s.0);
                let open: Vec<Vec2> = corners.iter().map(|(_, c)| *c).collect();
                let placed = place_window(
                    Vec2::new(width, height),
                    screen,
                    positions.0.get(interface).copied(),
                    &open,
                );
                corners.push((*id, placed));
                let (left, top) = (placed.x, placed.y);
                let title = title.clone();
                let root = commands
                    .spawn_scene(bsn! {
                        @ss13_ui_kit::Window {
                            @title: title, @width: width, @height: height, @left: left, @top: top
                        }
                    })
                    .insert(ClientWindow {
                        id: *id,
                        interface: interface.clone(),
                        status: *status,
                        static_data: static_data.clone(),
                        data: data.clone(),
                        content: None,
                        pending: None,
                        ready: None,
                        dimmer: None,
                        dirty: true,
                        scroll: Vec::new(),
                        placed,
                        autofocused: false,
                    })
                    .insert((UiLocalState::default(), Visibility::Hidden))
                    .id();
                windows.0.insert(*id, root);
            }
            UiServerMessage::Update {
                window: id,
                status,
                data,
            } => {
                let Some((mut window, _)) = windows.0.get(id).and_then(|e| roots.get_mut(*e).ok())
                else {
                    continue;
                };
                window.status = *status;
                // Views render data only (the dimmer follows the status): nothing to respawn
                // for a status change or a repeat of the shown data.
                if let Some(data) = data.as_ref().filter(|d| **d != window.data) {
                    window.data.clone_from(data);
                    window.dirty = true;
                }
            }
            UiServerMessage::Close { window: id } => {
                if let Some(root) = windows.0.remove(id) {
                    commands.entity(root).despawn();
                }
                corners.retain(|(open, _)| open != id);
            }
        }
    }
}

fn corner(node: &Node) -> Option<Vec2> {
    match (node.left, node.top) {
        (Val::Px(x), Val::Px(y)) => Some(Vec2::new(x, y)),
        _ => None,
    }
}

/// A window the player dragged reopens there (per interface, this session).
fn remember_positions(
    moved: Query<(&ClientWindow, &Node), Changed<Node>>,
    mut positions: ResMut<WindowPositions>,
) {
    for (window, node) in &moved {
        if let Some(at) = corner(node).filter(|at| *at != window.placed) {
            positions.0.insert(window.interface.clone(), at);
        }
    }
}

fn close_all_on_disconnect(
    state: Res<NetClientState>,
    mut windows: ResMut<ClientWindows>,
    mut commands: Commands,
) {
    if matches!(*state, NetClientState::Disconnected { .. }) {
        for (_, root) in windows.0.drain() {
            commands.entity(root).despawn();
        }
    }
}

/// Re-renders dirty windows into a hidden pending content ([`show_settled`] swaps it in). Deferred
/// while the player is typing into, or dragging, an input of that window: a respawn would drop
/// the edit (React keeps it through reconciliation); and while a render is pending, so a stream
/// of updates still shows its latest data a few frames late instead of never settling.
// shortcut: whole-content respawn per update (~1/s per window, tens to hundreds of entities),
// double-buffered; views keep large unchanged parts with [`memo`] (the Techweb's design icons).
// Patch widgets in place if profiling shows churn in views that cannot.
fn render_windows(
    mut windows: Query<(Entity, &mut ClientWindow, &Children, &mut Visibility)>,
    slots: Query<(), With<Slot>>,
    views: Res<UiViews>,
    focus: Option<Res<InputFocus>>,
    mouse: Option<Res<ButtonInput<MouseButton>>>,
    parents: Query<&ChildOf>,
    editing: Query<(), With<EditableText>>,
    locals: Query<&UiLocalState>,
    descendants: Query<&Children>,
    scrolls: Query<&ScrollPosition, With<ScrollContent>>,
    memos: Query<(&Memo, &Node, &ComputedNode)>,
    mut commands: Commands,
) {
    let focused = focus.and_then(|f| f.get());
    let dragging = mouse.is_some_and(|m| m.pressed(MouseButton::Left));
    for (root, mut window, children, mut visibility) in &mut windows {
        if !window.dirty || window.pending.is_some() || window.ready.is_some() {
            continue;
        }
        if let Some(focused) = focused
            && (dragging || editing.contains(focused))
            && parents.iter_ancestors(focused).any(|e| e == root)
        {
            continue;
        }
        // The window scene spawns its slot on the same command flush; wait for it.
        let Some(slot) = children.iter().find(|c| slots.contains(*c)) else {
            continue;
        };
        window.dirty = false;
        let Some(view) = views.0.get(window.interface.as_str()) else {
            continue;
        };
        let mut reusable = HashMap::new();
        if let Some(old) = window.content {
            window.scroll = descendants
                .iter_descendants(old)
                .filter_map(|e| scrolls.get(e).ok().map(|p| p.0))
                .collect();
            for (Memo(key), node, computed) in descendants
                .iter_descendants(old)
                .filter_map(|e| memos.get(e).ok())
            {
                let size = computed.size * computed.inverse_scale_factor;
                reusable.insert(
                    *key,
                    Node {
                        width: px(size.x),
                        height: px(size.y),
                        min_width: Val::Auto,
                        min_height: Val::Auto,
                        max_width: Val::Auto,
                        max_height: Val::Auto,
                        flex_grow: 0.0,
                        flex_shrink: 0.0,
                        flex_basis: Val::Auto,
                        ..node.clone()
                    },
                );
            }
        }
        let local = locals.get(root).cloned().unwrap_or_default();
        REUSABLE.with(|r| *r.borrow_mut() = reusable);
        let rendered = (view.render)(&window.static_data, &window.data, &local);
        REUSABLE.with(|r| r.borrow_mut().clear());
        match rendered {
            Ok(scene) => {
                // Both renders overlap the slot while the new one is laid out; absolute at inset
                // 0 in the slot's column gives the size an only in-flow child would get.
                let holder = commands
                    .spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            left: px(0),
                            right: px(0),
                            top: px(0),
                            bottom: px(0),
                            flex_direction: FlexDirection::Column,
                            ..default()
                        },
                        Visibility::Hidden,
                        ChildOf(slot),
                    ))
                    .id();
                commands.spawn_scene(scene).insert(ChildOf(holder));
                window.pending = Some((holder, 0));
            }
            Err(error) => {
                error!(
                    "window #{} ({}): undecodable data: {error}",
                    window.id, window.interface
                );
                visibility.set_if_neq(Visibility::Inherited);
            }
        }
    }
}

/// tgui `Window__dimmer`: shown below UI_INTERACTIVE, lets clicks through.
fn sync_dimmers(mut windows: Query<(Entity, &mut ClientWindow)>, mut commands: Commands) {
    for (root, mut window) in &mut windows {
        let dim = window.status < UiStatus::Interactive;
        match (dim, window.dimmer) {
            (true, None) => {
                let base = Hsla::from(theme::COLOR_BASE);
                let color = Hsla::new(base.hue, base.saturation, base.lightness + 0.15, 0.25);
                let dimmer = commands
                    .spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            left: px(0),
                            right: px(0),
                            top: px(0),
                            bottom: px(0),
                            ..default()
                        },
                        BackgroundColor(color.into()),
                        Pickable::IGNORE,
                        FocusPolicy::Pass,
                        GlobalZIndex(i32::MAX),
                        ChildOf(root),
                    ))
                    .id();
                window.dimmer = Some(dimmer);
            }
            (false, Some(dimmer)) => {
                commands.entity(dimmer).despawn();
                window.dimmer = None;
            }
            _ => {}
        }
    }
}

/// Frames a pending render may take to settle before it is shown anyway (a widget whose layout
/// never converges, an endless animation).
const MAX_SETTLE_FRAMES: u8 = 8;

/// Shows a pending render once a frame passed without any change to its nodes, after the
/// layout-feedback widgets had their say: the layout just computed is the final one. Runs before
/// visibility propagation, so the swap lands in the frame it is decided in, without a blank frame.
/// Cost: one walk over the pending tree per frame while a render is pending (tens to hundreds of
/// entities, a few frames per update).
fn show_settled(
    mut windows: Query<(&mut ClientWindow, &mut Visibility)>,
    mut holders: Query<&mut Visibility, Without<ClientWindow>>,
    descendants: Query<&Children>,
    changed: Query<
        (),
        Or<(
            Changed<Node>,
            Changed<ChildOf>,
            Changed<TextFont>,
            Changed<Text>,
        )>,
    >,
    focus: Option<Res<InputFocus>>,
    editing: Query<(), With<EditableText>>,
    parents: Query<&ChildOf>,
    slots: Query<(), With<MemoSlot>>,
    mut commands: Commands,
) {
    let focused = focus.and_then(|f| f.get());
    for (mut window, mut visibility) in &mut windows {
        let Some((holder, frames)) = window.pending else {
            continue;
        };
        let settling = std::iter::once(holder)
            .chain(descendants.iter_descendants(holder))
            .any(|e| changed.contains(e));
        if settling && frames < MAX_SETTLE_FRAMES {
            window.pending = Some((holder, frames + 1));
            continue;
        }
        window.pending = None;
        // The player started typing into the shown render meanwhile: keep it, render again later.
        if let Some(old) = window.content
            && focused
                .is_some_and(|f| editing.contains(f) && parents.iter_ancestors(f).any(|e| e == old))
        {
            commands.entity(holder).despawn();
            window.dirty = true;
            continue;
        }
        // Reused subtrees move in before the next layout, so the frame showing the swap shows
        // them laid out in their new place.
        if descendants
            .iter_descendants(holder)
            .any(|e| slots.contains(e))
        {
            window.ready = Some(holder);
            continue;
        }
        if let Ok(mut shown) = holders.get_mut(holder) {
            *shown = Visibility::Inherited;
        }
        if let Some(old) = window.content.replace(holder) {
            commands.entity(old).despawn();
        }
        visibility.set_if_neq(Visibility::Inherited);
    }
}

/// Swaps in a settled render that reuses subtrees of the shown one: each [`MemoSlot`] is replaced
/// by the shown [`Memo`] subtree with its key, then the rest of the shown render is dropped.
/// Runs in `Update`, before the layout of the frame that shows the result.
fn swap_memos(
    mut windows: Query<(&mut ClientWindow, &mut Visibility)>,
    mut holders: Query<&mut Visibility, Without<ClientWindow>>,
    descendants: Query<&Children>,
    slots: Query<(&MemoSlot, &ChildOf)>,
    memos: Query<&Memo>,
    mut commands: Commands,
) {
    for (mut window, mut visibility) in &mut windows {
        let Some(holder) = window.ready.take() else {
            continue;
        };
        let shown: HashMap<u64, Entity> = window
            .content
            .into_iter()
            .flat_map(|old| descendants.iter_descendants(old))
            .filter_map(|e| memos.get(e).ok().map(|m| (m.0, e)))
            .collect();
        for slot in descendants.iter_descendants(holder) {
            let Ok((MemoSlot(key), ChildOf(parent))) = slots.get(slot) else {
                continue;
            };
            let Some(&old) = shown.get(key) else {
                continue;
            };
            let index = descendants
                .get(*parent)
                .ok()
                .and_then(|c| c.iter().position(|c| c == slot))
                .unwrap_or_default();
            commands.entity(*parent).insert_child(index, old);
            commands.entity(slot).despawn();
        }
        if let Ok(mut shown) = holders.get_mut(holder) {
            *shown = Visibility::Inherited;
        }
        if let Some(old) = window.content.replace(holder) {
            commands.entity(old).despawn();
        }
        visibility.set_if_neq(Visibility::Inherited);
    }
}

/// Puts the previous render's scroll offsets back once the new content's scroll areas exist
/// (while it is still hidden).
fn restore_scroll(
    mut windows: Query<&mut ClientWindow>,
    descendants: Query<&Children>,
    mut scrolls: Query<&mut ScrollPosition, With<ScrollContent>>,
) {
    for mut window in &mut windows {
        let Some((content, _)) = window.pending.filter(|_| !window.scroll.is_empty()) else {
            continue;
        };
        let areas: Vec<Entity> = descendants
            .iter_descendants(content)
            .filter(|e| scrolls.contains(*e))
            .collect();
        if areas.is_empty() {
            continue;
        }
        for (area, offset) in areas.into_iter().zip(std::mem::take(&mut window.scroll)) {
            if let Ok(mut position) = scrolls.get_mut(area) {
                position.0 = offset;
            }
        }
    }
}

fn set_local(
    activate: On<Activate>,
    setters: Query<&SetLocal>,
    parents: Query<&ChildOf>,
    mut windows: Query<(&mut ClientWindow, &mut UiLocalState)>,
) {
    let Ok(setter) = setters.get(activate.entity) else {
        return;
    };
    let Some(root) = parents
        .iter_ancestors(activate.entity)
        .find(|e| windows.contains(*e))
    else {
        return;
    };
    let Ok((mut window, mut local)) = windows.get_mut(root) else {
        return;
    };
    if local.get(&setter.key) != setter.value {
        local.set(&setter.key, &setter.value);
    }
    if setter.rerender {
        window.dirty = true;
    }
}

fn clear_local(
    activate: On<Activate>,
    clearers: Query<&ClearLocal>,
    parents: Query<&ChildOf>,
    mut windows: Query<(&mut ClientWindow, &mut UiLocalState)>,
) {
    let Ok(ClearLocal(keys)) = clearers.get(activate.entity) else {
        return;
    };
    let Some(root) = parents
        .iter_ancestors(activate.entity)
        .find(|e| windows.contains(*e))
    else {
        return;
    };
    if let Ok((mut window, mut local)) = windows.get_mut(root) {
        for key in keys {
            local.0.remove(key);
        }
        window.dirty = true;
    }
}

fn local_on_input(
    changed: On<InputChanged>,
    inputs: Query<&LocalOnInput>,
    parents: Query<&ChildOf>,
    mut locals: Query<&mut UiLocalState>,
) {
    let Ok(LocalOnInput(key)) = inputs.get(changed.entity) else {
        return;
    };
    let Some(root) = parents
        .iter_ancestors(changed.entity)
        .find(|e| locals.contains(*e))
    else {
        return;
    };
    if let Ok(mut local) = locals.get_mut(root)
        && local.get(key) != changed.value
    {
        local.set(key, &changed.value);
    }
}

fn send_act(
    act: On<UiAct>,
    windows: Query<(&ClientWindow, &UiLocalState)>,
    with_local: Query<&ActWithLocal>,
    mut intents: MessageWriter<SendIntent>,
) {
    let Ok((window, local)) = windows.get(act.scope) else {
        return;
    };
    // ui_kit fills params from `ActParams` and from widgets that carry their own (Dropdown).
    let mut params = act.params.clone();
    if let Ok(with) = with_local.get(act.source) {
        let value = local.value(&with.key).unwrap_or(&with.default);
        params.extend(text_params(&with.param, value));
    }
    intents.write(SendIntent(ClientIntent::UiAct {
        window: window.id,
        action: act.action.clone(),
        params,
    }));
}

fn key_acts(
    key: On<FocusedInput<KeyboardInput>>,
    parents: Query<&ChildOf>,
    mut roots: Query<(&mut ClientWindow, &mut UiLocalState)>,
    children: Query<&Children>,
    acts: Query<&ActOnKey>,
    setters: Query<&KeySetLocal>,
    navs: Query<&ListNav>,
    mut intents: MessageWriter<SendIntent>,
) {
    // The event bubbles and a global observer sees every step: handle the focused one.
    if key.event_target() != key.focused_entity
        || key.input.state != ButtonState::Pressed
        || key.input.repeat
    {
        return;
    }
    let Some(root) = std::iter::once(key.focused_entity)
        .chain(parents.iter_ancestors(key.focused_entity))
        .find(|e| roots.contains(*e))
    else {
        return;
    };
    let Ok((mut window, mut local)) = roots.get_mut(root) else {
        return;
    };
    let Some(view_root) = window.content_root(&children) else {
        return;
    };
    let code = key.input.key_code;
    if let Ok(nav) = navs.get(view_root) {
        let current = local.value(&nav.key).unwrap_or(&nav.default).to_owned();
        let query = local.get(&nav.search_key).trim().to_lowercase();
        let shown: Vec<&String> = nav
            .items
            .iter()
            .filter(|i| i.to_lowercase().contains(&query))
            .collect();
        let at = shown.iter().position(|i| **i == current);
        let pick = match (code, &key.input.logical_key) {
            (KeyCode::ArrowDown, _) => match at {
                Some(i) if i + 1 < shown.len() => shown.get(i + 1).copied(),
                _ => shown.first().copied(),
            },
            (KeyCode::ArrowUp, _) => match at {
                Some(i) if i > 0 => shown.get(i - 1).copied(),
                _ => shown.last().copied(),
            },
            (KeyCode::Enter | KeyCode::NumpadEnter, _) => {
                if shown.contains(&&current) {
                    intents.write(SendIntent(ClientIntent::UiAct {
                        window: window.id,
                        action: nav.submit.clone(),
                        params: text_params(&nav.key, &current),
                    }));
                }
                return;
            }
            // The typed letter, not the key code: Cyrillic items on a Russian layout.
            (_, Key::Character(c)) if nav.hotkeys => {
                let c = c.to_lowercase();
                nav.items.iter().find(|i| i.to_lowercase().starts_with(&c))
            }
            _ => None,
        };
        if let Some(pick) = pick.cloned() {
            local.set(&nav.key, &pick);
            return;
        }
    }
    if let Ok(ActOnKey(bindings)) = acts.get(view_root)
        && let Some((_, action, params)) = bindings.iter().find(|(k, ..)| *k == code)
    {
        intents.write(SendIntent(ClientIntent::UiAct {
            window: window.id,
            action: action.clone(),
            params: params.clone(),
        }));
        return;
    }
    if let Ok(setter) = setters.get(view_root)
        && let Some((_, value)) = setter.keys.iter().find(|(k, _)| *k == code)
    {
        local.set(&setter.key, value);
        window.dirty = true;
    }
}

fn double_click_acts(
    click: On<PointerClick>,
    acts: Query<&ActOnDoubleClick>,
    parents: Query<&ChildOf>,
    windows: Query<&ClientWindow>,
    mut intents: MessageWriter<SendIntent>,
) {
    if click.count < 2 || click.button != PointerButton::Primary {
        return;
    }
    let Ok(act) = acts.get(click.event_target()) else {
        return;
    };
    let Some(window) = parents
        .iter_ancestors(click.event_target())
        .find_map(|e| windows.get(e).ok())
    else {
        return;
    };
    intents.write(SendIntent(ClientIntent::UiAct {
        window: window.id,
        action: act.action.clone(),
        params: text_params(&act.param, &act.value),
    }));
}

fn sync_selected_rows(
    windows: Query<(Entity, Ref<UiLocalState>), With<ClientWindow>>,
    added: Query<Entity, Added<SelectRow>>,
    parents: Query<&ChildOf>,
    children: Query<&Children>,
    mut rows: Query<(&SelectRow, &mut ss13_ui_kit::Button)>,
) {
    let rendered: Vec<Entity> = added
        .iter()
        .filter_map(|row| parents.iter_ancestors(row).find(|e| windows.contains(*e)))
        .collect();
    for (root, local) in &windows {
        if !(local.is_changed() || rendered.contains(&root)) {
            continue;
        }
        for entity in children.iter_descendants(root) {
            let Ok((row, mut button)) = rows.get_mut(entity) else {
                continue;
            };
            let selected = local
                .value(&row.key)
                .map_or(row.default, |v| v == row.value);
            if button.selected != selected {
                button.selected = selected;
            }
        }
    }
}

/// Runs when a window's local state changed or a render spawned rows; cost: one walk over that
/// window's entities.
fn filter_search_rows(
    windows: Query<(Entity, Ref<UiLocalState>), With<ClientWindow>>,
    added: Query<Entity, Added<SearchRow>>,
    parents: Query<&ChildOf>,
    children: Query<&Children>,
    mut rows: Query<(&SearchRow, &mut Node)>,
) {
    let rendered: Vec<Entity> = added
        .iter()
        .filter_map(|row| parents.iter_ancestors(row).find(|e| windows.contains(*e)))
        .collect();
    for (root, local) in &windows {
        if !(local.is_changed() || rendered.contains(&root)) {
            continue;
        }
        for entity in children.iter_descendants(root) {
            let Ok((row, mut node)) = rows.get_mut(entity) else {
                continue;
            };
            let query = local.get(&row.key).to_lowercase();
            let display = if row.text.contains(query.trim()) {
                Display::Flex
            } else {
                Display::None
            };
            if node.display != display {
                node.display = display;
            }
        }
    }
}

/// A system, not an `Add` observer: the view's root gets its `ChildOf` after its components.
fn autofocus_window(
    added: Query<(Entity, &AutoFocusWindow), Added<AutoFocusWindow>>,
    parents: Query<&ChildOf>,
    mut windows: Query<&mut ClientWindow>,
    focus: Option<ResMut<InputFocus>>,
) {
    let Some(mut focus) = focus else {
        return;
    };
    for (entity, autofocus) in &added {
        let Some(root) = std::iter::once(entity)
            .chain(parents.iter_ancestors(entity))
            .find(|e| windows.contains(*e))
        else {
            continue;
        };
        if let Ok(mut window) = windows.get_mut(root)
            && autofocus.0
            && !window.autofocused
        {
            window.autofocused = true;
            focus.set(root, bevy::input_focus::FocusCause::Navigated);
        }
    }
}

fn submit_act(
    submitted: On<InputSubmitted>,
    acts: Query<&ActOnSubmit>,
    parents: Query<&ChildOf>,
    windows: Query<&ClientWindow>,
    mut intents: MessageWriter<SendIntent>,
) {
    let Ok(act) = acts.get(submitted.entity) else {
        return;
    };
    if submitted.value.trim().is_empty() {
        return;
    }
    let Some(window) = parents
        .iter_ancestors(submitted.entity)
        .find_map(|e| windows.get(e).ok())
    else {
        return;
    };
    let mut params = act.params.clone();
    params.extend(text_params(&act.param, &submitted.value));
    intents.write(SendIntent(ClientIntent::UiAct {
        window: window.id,
        action: act.action.clone(),
        params,
    }));
}

fn send_number_act(
    changed: On<NumberInputChanged>,
    acts: Query<(&ActOnChange, Option<&ss13_ui_kit::ActParams>)>,
    parents: Query<&ChildOf>,
    windows: Query<&ClientWindow>,
    mut intents: MessageWriter<SendIntent>,
) {
    let Ok((act, extra)) = acts.get(changed.entity) else {
        return;
    };
    let Some(window) = parents
        .iter_ancestors(changed.entity)
        .find_map(|e| windows.get(e).ok())
    else {
        return;
    };
    intents.write(SendIntent(ClientIntent::UiAct {
        window: window.id,
        action: act.action.clone(),
        // `ActParams` beside it: the other params of the act (`{ref: vent, value}`).
        params: extra
            .map(|p| p.0.clone())
            .unwrap_or_default()
            .into_iter()
            .chain([(act.param.clone(), changed.value.to_string())])
            .collect(),
    }));
}

fn send_close(
    close: On<WindowClose>,
    roots: Query<&ClientWindow>,
    mut windows: ResMut<ClientWindows>,
    mut send: MessageWriter<SendUi>,
) {
    if let Ok(window) = roots.get(close.entity) {
        windows.0.remove(&window.id);
        send.write(SendUi(UiClientMessage::Close { window: window.id }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_text_splits_at_char_boundaries() {
        let text = "я".repeat(300);
        let params = text_params("entry", &text);
        let keys: Vec<_> = params.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, ["entry", "entry.1", "entry.2"]);
        assert!(params.iter().all(|(_, p)| p.len() <= PARAM_BYTES));
        assert_eq!(
            params.iter().map(|(_, p)| p.as_str()).collect::<String>(),
            text
        );
        assert_eq!(text_params("entry", ""), [("entry".into(), String::new())]);
    }

    #[test]
    fn placement_centers_cascades_and_keeps_recalled_windows_on_screen() {
        let screen = Vec2::new(1000.0, 800.0);
        let size = Vec2::new(400.0, 300.0);
        assert_eq!(
            place_window(size, screen, None, &[]),
            Vec2::new(300.0, 250.0)
        );
        // Two windows already there: the third goes past both.
        let open = [Vec2::new(300.0, 250.0), Vec2::new(330.0, 280.0)];
        assert_eq!(
            place_window(size, screen, None, &open),
            Vec2::new(360.0, 310.0)
        );
        // Larger than the screen: pinned to the top-left corner.
        assert_eq!(
            place_window(Vec2::new(1200.0, 900.0), screen, None, &[]),
            Vec2::ZERO
        );
        // Dragged off screen last time (or a smaller window now): the title bar stays reachable.
        assert_eq!(
            place_window(size, screen, Some(Vec2::new(990.0, -40.0)), &[]),
            Vec2::new(920.0, 0.0)
        );
    }
}
