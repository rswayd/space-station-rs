//! Client half: the egui debug overlay.
//!
//! F9 (or the backtick key outside text fields) opens and closes it. While closed it draws
//! nothing, takes no input and turns the network tap off; the trackers that feed the panels
//! (frame times, tgui messages, the log tail) keep a few hundred entries each. While open, egui
//! takes the pointer over its windows (the world under them gets no clicks) and the keyboard
//! while one of its text fields has focus. Not `bevy_egui`'s own absorption: it takes the
//! keyboard whenever any egui widget has focus, and a clicked button keeps it, so the mob would
//! stop walking after a click on a panel toggle.
//!
//! `SS13_DEBUG_UI=1` starts with the overlay open; `SS13_DEBUG_UI=net,entities,...` also opens
//! those panels (keys: [`Panel::key`]). Scripted visual checks use it, as egui gets no injected
//! input.

mod log;
mod panels;
#[cfg(not(target_arch = "wasm32"))]
mod record;

use std::collections::{BTreeMap, VecDeque};

use bevy::ecs::message::MessageCursor;
use bevy::input::keyboard::KeyboardInput;
use bevy::input_focus::InputFocus;
use bevy::picking::PickingSystems;
use bevy::picking::hover::HoverMap;
use bevy::picking::pointer::{PointerAction, PointerButton, PointerId, PointerInput};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::text::EditableText;
use bevy_egui::input::EguiWantsInput;
use bevy_egui::{
    EguiContexts, EguiGlobalSettings, EguiPlugin, EguiPreUpdateSet, EguiPrimaryContextPass,
};
use ss13_net::client::SendIntent;
use ss13_net::debug::client::{DebugResponseReceived, SendDebugRequest};
use ss13_net::tap::{NetPacket, NetTap};
use ss13_net::ui::client::UiMessageReceived;
use ss13_render::{ActiveZ, WorldCamera};
use ss13_shared::TilePos;
use ss13_shared::debug::{DebugRequest, DebugResponse, ServerStats, TileReport};
use ss13_shared::protocol::ClientIntent;
use ss13_shared::ui::{UiServerMessage, UiStatus};

use crate::netlog::{NetLog, PacketFilter, PacketRecord};

pub use log::log_layer;

/// Packets the network panel keeps.
const NET_LOG_CAPACITY: usize = 5000;
/// Frame times the perf graph shows.
const FRAME_HISTORY: usize = 300;
/// Acts the UI panel keeps.
const ACT_HISTORY: usize = 50;
/// Hovered tile queries at most this often (seconds); the server allows 10 per second.
const HOVER_QUERY_SECS: f64 = 0.25;
const STATS_QUERY_SECS: f64 = 1.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Panel {
    Perf,
    Network,
    Entities,
    Prediction,
    Tile,
    Ui,
    Log,
    Server,
    ScreenFx,
}

impl Panel {
    pub const ALL: [Panel; 9] = [
        Panel::Perf,
        Panel::Network,
        Panel::Entities,
        Panel::Prediction,
        Panel::Tile,
        Panel::Ui,
        Panel::Log,
        Panel::Server,
        Panel::ScreenFx,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Panel::Perf => "Perf",
            Panel::Network => "Network",
            Panel::Entities => "Entities",
            Panel::Prediction => "Prediction",
            Panel::Tile => "Tile",
            Panel::Ui => "UI",
            Panel::Log => "Log",
            Panel::Server => "Server",
            Panel::ScreenFx => "Screen FX",
        }
    }

    /// Name in `SS13_DEBUG_UI`.
    pub fn key(self) -> &'static str {
        match self {
            Panel::Perf => "perf",
            Panel::Network => "net",
            Panel::Entities => "entities",
            Panel::Prediction => "prediction",
            Panel::Tile => "tile",
            Panel::Ui => "ui",
            Panel::Log => "log",
            Panel::Server => "server",
            Panel::ScreenFx => "fx",
        }
    }
}

/// Overlay and panel visibility; kept for the session.
#[derive(Resource, Debug, Default)]
pub struct DebugUi {
    pub open: bool,
    shown: [bool; Panel::ALL.len()],
}

impl DebugUi {
    /// `SS13_DEBUG_UI` (see the module docs).
    pub fn from_env() -> Self {
        std::env::var("SS13_DEBUG_UI").map_or_else(|_| Self::default(), |v| Self::parse(&v))
    }

    fn parse(value: &str) -> Self {
        let mut ui = Self {
            open: !value.is_empty() && value != "0",
            ..default()
        };
        for key in value.split(',').map(str::trim) {
            if let Some(panel) = Panel::ALL.into_iter().find(|p| p.key() == key) {
                ui.set(panel, true);
            }
        }
        ui
    }

    pub fn shown(&self, panel: Panel) -> bool {
        self.shown[panel as usize]
    }

    pub fn set(&mut self, panel: Panel, shown: bool) {
        self.shown[panel as usize] = shown;
    }

    /// Open and showing `panel`.
    pub fn visible(&self, panel: Panel) -> bool {
        self.open && self.shown(panel)
    }
}

#[derive(Resource)]
struct NetPanel {
    log: NetLog,
    filter: PacketFilter,
    status: String,
}

/// Tile under the cursor, the clicked tile and the clicked entity on it.
#[derive(Resource, Default)]
struct Inspector {
    cursor: Option<Vec2>,
    hovered: Option<TilePos>,
    selected: Option<TilePos>,
    /// A world click selects the tile instead of reaching the game.
    pick: bool,
    entity: Option<Entity>,
    /// Component name and reflected value of `entity`.
    components: Vec<(String, Option<String>)>,
    components_at: f64,
}

/// What the server half answered.
#[derive(Resource, Default)]
struct ServerData {
    tiles: HashMap<TilePos, TileReport>,
    stats: Option<ServerStats>,
    prototypes: Vec<String>,
    last_reply: String,
    hover_asked: Option<(TilePos, f64)>,
    selected_asked: Option<(TilePos, f64)>,
    stats_asked: f64,
}

struct TrackedWindow {
    interface: String,
    title: String,
    status: UiStatus,
    static_data: Vec<u8>,
    data: Vec<u8>,
}

#[derive(Resource, Default)]
struct UiTracker {
    windows: BTreeMap<u32, TrackedWindow>,
    /// Time, window, action, params.
    acts: VecDeque<(f64, u32, String, Vec<(String, String)>)>,
}

#[derive(Resource, Default)]
struct FrameTimes(VecDeque<f32>);

#[derive(Resource)]
struct DevTools {
    search: String,
    searched: String,
    god: bool,
    time_scale: f32,
    confirm_end: bool,
}

impl Default for DevTools {
    fn default() -> Self {
        Self {
            search: String::new(),
            searched: String::new(),
            god: false,
            time_scale: 1.0,
            confirm_end: false,
        }
    }
}

pub struct DebugUiPlugin;

impl Plugin for DebugUiPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<EguiPlugin>() {
            app.insert_resource(EguiGlobalSettings {
                auto_create_primary_context: false,
                enable_absorb_bevy_input_system: false,
                ..default()
            })
            .add_plugins(EguiPlugin::default());
        }
        if !app.is_plugin_added::<bevy::diagnostic::FrameTimeDiagnosticsPlugin>() {
            app.add_plugins(bevy::diagnostic::FrameTimeDiagnosticsPlugin::default());
        }
        if !app.is_plugin_added::<bevy::diagnostic::EntityCountDiagnosticsPlugin>() {
            app.add_plugins(bevy::diagnostic::EntityCountDiagnosticsPlugin::default());
        }
        let ui = DebugUi::from_env();
        app.insert_resource(ui)
            .insert_resource(NetPanel {
                log: NetLog::new(NET_LOG_CAPACITY),
                filter: PacketFilter::default(),
                status: String::new(),
            })
            .insert_resource(Inspector {
                pick: true,
                ..default()
            })
            .init_resource::<ServerData>()
            .init_resource::<UiTracker>()
            .init_resource::<FrameTimes>()
            .init_resource::<DevTools>()
            .init_resource::<log::LogPanel>()
            .add_observer(egui_on_ui_camera)
            .add_systems(
                PreUpdate,
                (
                    pointer.after(PickingSystems::Last),
                    absorb_keyboard.after(EguiPreUpdateSet::ProcessInput),
                )
                    .run_if(|ui: Res<DebugUi>| ui.open),
            )
            .add_systems(
                Update,
                (
                    toggle,
                    sync_tap,
                    collect_packets,
                    collect_ui,
                    collect_responses,
                    collect_frame_time,
                    request_server.run_if(|ui: Res<DebugUi>| ui.open),
                    collect_components.run_if(|ui: Res<DebugUi>| ui.visible(Panel::Entities)),
                )
                    .chain(),
            )
            .add_systems(
                EguiPrimaryContextPass,
                (
                    panels::menu,
                    panels::perf,
                    panels::network,
                    panels::entities,
                    panels::prediction,
                    panels::tile,
                    panels::ui,
                    log::panel,
                    panels::server,
                    panels::screen_fx,
                )
                    .chain()
                    .run_if(|ui: Res<DebugUi>| ui.open),
            );
        // Keeps recording while the overlay is closed, so the video shows the game alone.
        #[cfg(not(target_arch = "wasm32"))]
        app.init_resource::<record::Recorder>()
            .add_systems(Last, record::capture);
    }
}

/// egui draws in its camera's pass: on the world camera (order 0) the UI camera's `bevy_ui` (order 1,
/// the full-window lobby background among it) would cover it.
fn egui_on_ui_camera(add: On<Add<IsDefaultUiCamera>>, mut commands: Commands) {
    commands
        .entity(add.entity)
        .insert(bevy_egui::PrimaryEguiContext);
}

fn toggle(
    keys: Res<ButtonInput<KeyCode>>,
    focus: Option<Res<InputFocus>>,
    text_fields: Query<(), With<EditableText>>,
    mut ui: ResMut<DebugUi>,
) {
    let typing = focus
        .and_then(|f| f.get())
        .is_some_and(|e| text_fields.contains(e));
    if keys.just_pressed(KeyCode::F9) || (keys.just_pressed(KeyCode::Backquote) && !typing) {
        ui.open = !ui.open;
    }
}

/// While an egui text field has focus the game sees no keys (F9 still closes the overlay).
/// After `bevy_egui` has read this frame's keyboard input.
fn absorb_keyboard(
    mut contexts: EguiContexts,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut messages: ResMut<Messages<KeyboardInput>>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    if !ctx.text_edit_focused() {
        return;
    }
    let close = keys.just_pressed(KeyCode::F9);
    keys.reset_all();
    messages.clear();
    if close {
        keys.press(KeyCode::F9);
    }
}

fn sync_tap(ui: Res<DebugUi>, mut tap: ResMut<NetTap>) {
    let on = ui.visible(Panel::Network);
    if tap.enabled != on {
        tap.enabled = on;
    }
}

fn collect_packets(
    mut packets: MessageReader<NetPacket>,
    mut panel: ResMut<NetPanel>,
    time: Res<Time<Real>>,
) {
    let now = time.elapsed_secs_f64();
    for p in packets.read() {
        panel.log.push(PacketRecord {
            time: now,
            outgoing: p.outgoing,
            kind: p.kind.clone(),
            size: p.size,
            summary: p.summary.clone(),
        });
    }
}

fn collect_ui(
    mut received: MessageReader<UiMessageReceived>,
    mut sent: MessageReader<SendIntent>,
    mut tracker: ResMut<UiTracker>,
    time: Res<Time<Real>>,
) {
    for UiMessageReceived(message) in received.read() {
        match message {
            UiServerMessage::Open {
                window,
                interface,
                title,
                status,
                static_data,
                data,
            } => {
                tracker.windows.insert(
                    *window,
                    TrackedWindow {
                        interface: interface.clone(),
                        title: title.clone(),
                        status: *status,
                        static_data: static_data.clone(),
                        data: data.clone(),
                    },
                );
            }
            UiServerMessage::Update {
                window,
                status,
                data,
            } => {
                if let Some(w) = tracker.windows.get_mut(window) {
                    w.status = *status;
                    if let Some(data) = data {
                        w.data.clone_from(data);
                    }
                }
            }
            UiServerMessage::Close { window } => {
                tracker.windows.remove(window);
            }
        }
    }
    for SendIntent(intent) in sent.read() {
        if let ClientIntent::UiAct {
            window,
            action,
            params,
        } = intent
        {
            if tracker.acts.len() == ACT_HISTORY {
                tracker.acts.pop_front();
            }
            let now = time.elapsed_secs_f64();
            tracker
                .acts
                .push_back((now, *window, action.clone(), params.clone()));
        }
    }
}

fn collect_responses(
    mut responses: MessageReader<DebugResponseReceived>,
    mut data: ResMut<ServerData>,
    mut dev: ResMut<DevTools>,
) {
    for DebugResponseReceived(response) in responses.read() {
        match response {
            DebugResponse::Tile(report) => {
                // Hovering a long way would keep every tile otherwise.
                if data.tiles.len() > 64 {
                    data.tiles.clear();
                }
                data.tiles.insert(report.pos, report.clone());
            }
            DebugResponse::ServerStats(stats) => {
                dev.time_scale = stats.time_scale;
                data.stats = Some(stats.clone());
            }
            DebugResponse::Prototypes { ids, .. } => data.prototypes.clone_from(ids),
            DebugResponse::Done(text) => data.last_reply = format!("ok: {text}"),
            DebugResponse::Refused(text) => data.last_reply = format!("refused: {text}"),
            // Functional test bots (`ss13_bots`) only.
            DebugResponse::Test { .. } => {}
        }
    }
}

fn collect_frame_time(mut frames: ResMut<FrameTimes>, time: Res<Time<Real>>) {
    if frames.0.len() == FRAME_HISTORY {
        frames.0.pop_front();
    }
    frames.0.push_back(time.delta_secs() * 1000.0);
}

/// Hover and click on the map while the overlay is open. Runs after picking has turned this
/// frame's pointer input into UI events; the game's world click reads [`PointerInput`] in
/// `Update`, so clearing it here keeps clicks on egui windows (and inspector picks) out of the
/// game without touching the UI.
fn pointer(
    mut input: ResMut<Messages<PointerInput>>,
    mut cursor_reader: Local<MessageCursor<PointerInput>>,
    wants: Res<EguiWantsInput>,
    hover: Option<Res<HoverMap>>,
    ui_nodes: Query<(), With<Node>>,
    cameras: Query<(&Camera, &GlobalTransform), With<WorldCamera>>,
    active_z: Res<ActiveZ>,
    debug_ui: Res<DebugUi>,
    mut inspector: ResMut<Inspector>,
) {
    let mut press = None;
    for event in cursor_reader.read(&input) {
        if event.pointer_id != PointerId::Mouse {
            continue;
        }
        inspector.cursor = Some(event.location.position);
        if matches!(event.action, PointerAction::Press(PointerButton::Primary)) {
            press = Some(event.location.position);
        }
    }
    let over_egui = wants.wants_any_pointer_input();
    let over_ui = hover
        .as_ref()
        .and_then(|h| h.get(&PointerId::Mouse))
        .is_some_and(|hits| hits.keys().any(|&e| ui_nodes.contains(e)));
    let to_tile = |at: Vec2| {
        cameras
            .iter()
            .find_map(|(camera, transform)| camera.viewport_to_world_2d(transform, at).ok())
            .map(|world| ss13_game::client::tile_at(world, active_z.0))
    };
    inspector.hovered = match inspector.cursor {
        Some(at) if !over_egui && !over_ui => to_tile(at),
        _ => None,
    };
    let mut swallow = over_egui;
    if let Some(at) = press
        && !over_egui
        && !over_ui
        && inspector.pick
        && debug_ui.visible(Panel::Entities)
        && let Some(tile) = to_tile(at)
    {
        inspector.selected = Some(tile);
        inspector.entity = None;
        swallow = true;
    }
    if swallow {
        input.clear();
    }
}

fn request_server(
    ui: Res<DebugUi>,
    inspector: Res<Inspector>,
    mut data: ResMut<ServerData>,
    time: Res<Time<Real>>,
    mut requests: MessageWriter<SendDebugRequest>,
) {
    let now = time.elapsed_secs_f64();
    if ui.shown(Panel::Tile)
        && let Some(tile) = inspector.hovered
        && data
            .hover_asked
            .is_none_or(|(asked, at)| asked != tile && now - at >= HOVER_QUERY_SECS)
    {
        data.hover_asked = Some((tile, now));
        requests.write(SendDebugRequest(DebugRequest::Tile(tile)));
    }
    // The selected tile is refreshed every second (air and power change).
    if ui.shown(Panel::Entities)
        && let Some(tile) = inspector.selected
        && data
            .selected_asked
            .is_none_or(|(asked, at)| asked != tile || now - at >= STATS_QUERY_SECS)
    {
        data.selected_asked = Some((tile, now));
        requests.write(SendDebugRequest(DebugRequest::Tile(tile)));
    }
    if ui.shown(Panel::Server) && now - data.stats_asked >= STATS_QUERY_SECS {
        data.stats_asked = now;
        requests.write(SendDebugRequest(DebugRequest::ServerStats));
    }
}

/// Every component of the inspected entity, with its value where it is reflected. Exclusive:
/// it reads arbitrary components. Twice a second, only while the entities panel is shown.
fn collect_components(world: &mut World) {
    let now = world.resource::<Time<Real>>().elapsed_secs_f64();
    let inspector = world.resource::<Inspector>();
    let Some(entity) = inspector.entity else {
        return;
    };
    if now - inspector.components_at < 0.5 {
        return;
    }
    let mut components = Vec::new();
    if let Ok(infos) = world.inspect_entity(entity) {
        let registry = world.resource::<AppTypeRegistry>().read();
        let entity_ref = world.entity(entity);
        for (_, info) in infos {
            let value = info
                .type_id()
                .and_then(|id| registry.get(id))
                .and_then(|reg| reg.data::<ReflectComponent>())
                .and_then(|reflect| reflect.reflect(entity_ref))
                .map(|value| bounded_pretty(value, 2000));
            components.push((info.name().to_string(), value));
        }
    }
    let mut inspector = world.resource_mut::<Inspector>();
    inspector.components = components;
    inspector.components_at = now;
}

/// `{:#?}` cut at `max` bytes.
fn bounded_pretty(value: &dyn Reflect, max: usize) -> String {
    let mut text = format!("{value:#?}");
    if text.len() > max {
        let mut end = max;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
        text.push('…');
    }
    text
}

/// Where exports go: `SS13_DEBUG_DIR`, else the system temp dir.
fn export_dir() -> std::path::PathBuf {
    std::env::var_os("SS13_DEBUG_DIR")
        .map(Into::into)
        .unwrap_or_else(std::env::temp_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panels_from_the_env_value() {
        let parse = DebugUi::parse;

        let ui = parse("net,entities");
        assert!(ui.open && ui.visible(Panel::Network) && ui.visible(Panel::Entities));
        assert!(!ui.shown(Panel::Perf));
        assert!(!parse("0").open);
    }

    #[test]
    fn f12_and_backtick_toggle_but_not_while_typing() {
        use bevy::ecs::system::RunSystemOnce;
        let mut world = World::new();
        world.init_resource::<DebugUi>();
        world.init_resource::<ButtonInput<KeyCode>>();
        let tap = |world: &mut World, key| {
            let mut keys = world.resource_mut::<ButtonInput<KeyCode>>();
            keys.clear();
            keys.release_all();
            keys.press(key);
            world.run_system_once(toggle).unwrap();
            world.resource::<DebugUi>().open
        };
        assert!(tap(&mut world, KeyCode::F9));
        assert!(!tap(&mut world, KeyCode::Backquote));
        // A focused text field (the say line) types the backtick instead.
        let field = world.spawn(EditableText::default()).id();
        world.insert_resource(InputFocus::from_entity(field));
        assert!(!tap(&mut world, KeyCode::Backquote));
        assert!(tap(&mut world, KeyCode::F9));
    }
}
