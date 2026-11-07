//! One headless bot: a real client `App` (`ss13_net` client over the real transport, no window,
//! renderer or GPU) plus what the player would see, recorded from the same messages the game's
//! client plugins read: replicated entities, chat, tgui windows (the HUDs are tgui interfaces
//! without windows), move acks and debug answers.
//!
//! Movement is not predicted: a bot steps, then waits for the server's ack (the client's
//! prediction only hides latency, and it lives in `ss13_game::client` with the renderer).

use std::collections::{BTreeMap, HashMap};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use bevy::prelude::*;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value, json};
use ss13_assets::manifest::AssetManifest;
use ss13_net::admin::client::{AdminResponseReceived, SendAdminRequest};
use ss13_net::chat::client::ChatReceived;
use ss13_net::client::{
    LocalControl, NetClientConfig, NetClientPlugin, NetClientState, NetEntity, NetEntityMap,
    NoticeReceived, SendIntent,
};
use ss13_net::debug::client::{DebugResponseReceived, SendDebugRequest};
use ss13_net::move_ack::client::MoveAckReceived;
use ss13_net::ui::client::{SendUi, UiMessageReceived};
use ss13_shared::admin::{AdminRequest, AdminResponse};
use ss13_shared::debug::{DebugRequest, DebugResponse};
use ss13_shared::protocol::{ClientIntent, NetId};
use ss13_shared::ui::{UiClientMessage, UiServerMessage, UiStatus};
use ss13_shared::{Appearance, Dir, EMISSIVE_PLANE, Facing, IconStateId, MouseOpacity, TilePos};
use ss13_tgui::UiInterface;

/// Chat lines kept per bot.
const CHAT_KEEP: usize = 500;

/// Icon state names by `IconStateId`, from the asset manifest: HUD slots and replicated
/// entities show icon ids only. Also what a tile click needs to hit pixels like the client.
#[derive(Clone, Default)]
pub struct IconNames {
    names: Arc<Vec<String>>,
    pixels: Option<Arc<IconPixels>>,
}

impl IconNames {
    pub fn load(assets: &std::path::Path) -> anyhow::Result<Self> {
        let manifest = ss13_mapload::read_manifest(assets)?;
        Ok(Self {
            names: Arc::new(manifest.states.iter().map(|s| s.state.clone()).collect()),
            pixels: Some(Arc::new(IconPixels {
                assets: assets.to_owned(),
                manifest,
                pages: Mutex::default(),
            })),
        })
    }

    fn name(&self, id: u64) -> Value {
        self.names
            .get(id as usize)
            .map_or_else(|| json!(id), |s| json!(s))
    }
}

/// Atlas alpha, decoded per page on the first click that needs it and shared by every bot.
// Cost: a 1024x1024 page decode (a few ms) once per page per run; clicks are a few per case.
struct IconPixels {
    assets: std::path::PathBuf,
    manifest: AssetManifest,
    pages: Mutex<HashMap<u32, Option<Arc<PageAlpha>>>>,
}

/// One bit per atlas pixel: alpha above zero.
struct PageAlpha {
    width: u32,
    bits: Vec<u64>,
}

impl PageAlpha {
    fn decode(path: &std::path::Path) -> anyhow::Result<Self> {
        Ok(Self::from_page(ss13_assets::page::decode(&std::fs::read(
            path,
        )?)?))
    }

    fn from_page(page: ss13_assets::page::PagePixels) -> Self {
        let mut bits = vec![0u64; (page.rgba.len() / 4).div_ceil(64)];
        for (i, px) in page.rgba.chunks_exact(4).enumerate() {
            if px[3] > 0 {
                bits[i / 64] |= 1 << (i % 64);
            }
        }
        Self {
            width: page.width,
            bits,
        }
    }

    fn opaque(&self, x: u32, y: u32) -> bool {
        let i = y as usize * self.width as usize + x as usize;
        self.bits
            .get(i / 64)
            .is_some_and(|w| w & (1 << (i % 64)) != 0)
    }
}

impl IconPixels {
    fn page(&self, atlas: u32) -> Option<Arc<PageAlpha>> {
        let mut pages = self.pages.lock().expect("atlas pages");
        pages
            .entry(atlas)
            .or_insert_with(|| {
                let file = self.manifest.atlases.get(atlas as usize)?;
                PageAlpha::decode(&self.assets.join(file))
                    .map_err(|e| error!("bot atlas page {file}: {e:#}"))
                    .ok()
                    .map(Arc::new)
            })
            .clone()
    }

    /// Whether the icon pixel `(16, 16)` of the tile (from its bottom-left corner) is drawn by
    /// `state` facing `dir` and offset by `offset`, like the client's `topmost_at` with
    /// `MOUSE_OPACITY_ICON`. A state or page that cannot be read counts as a hit.
    // shortcut: animation frame 0 and no turn; at the tile's center a turn moves nothing for
    // 32x32 icons.
    fn hits_center(&self, state: IconStateId, dir: Dir, offset: [i16; 2], whole: bool) -> bool {
        let Some(info) = self.manifest.get(state) else {
            return true;
        };
        let d = match (info.dirs, dir) {
            (4, Dir::NorthEast | Dir::SouthEast) => Dir::East,
            (4, Dir::NorthWest | Dir::SouthWest) => Dir::West,
            _ => dir,
        };
        let d = match info.dirs {
            4 | 8 => Dir::DMI_ORDER[..usize::from(info.dirs)]
                .iter()
                .position(|x| *x == d)
                .unwrap_or(0),
            _ => 0,
        };
        let Some(&[x, y, w, h]) = info.rects.get(d) else {
            return true;
        };
        let (px, py) = (16 - i32::from(offset[0]), 16 - i32::from(offset[1]));
        if px < 0 || py < 0 || px >= w as i32 || py >= h as i32 {
            return false;
        }
        whole
            || self
                .page(info.atlas)
                .is_none_or(|page| page.opaque(x + px as u32, y + h - 1 - py as u32))
    }

    fn hits(&self, appearance: &Appearance, dir: Dir) -> bool {
        let whole = appearance.mouse_opacity == MouseOpacity::Opaque;
        let base = appearance.pixel_offset;
        self.hits_center(appearance.icon_state, dir, base, whole)
            || appearance
                .overlays
                .iter()
                // Emissive masks are drawn by the lighting only.
                .filter(|o| o.plane != Some(EMISSIVE_PLANE) && o.shown_for(dir))
                .any(|o| {
                    let offset = [base[0] + o.pixel_offset[0], base[1] + o.pixel_offset[1]];
                    self.hits_center(o.icon_state, o.dir.unwrap_or(dir), offset, whole)
                })
    }
}

pub struct Window {
    pub interface: String,
    pub title: String,
    pub status: UiStatus,
    data: Vec<u8>,
    static_data: Vec<u8>,
}

/// What the bot received, filled by [`record`].
#[derive(Resource, Default)]
pub struct Observed {
    pub chat: Vec<String>,
    /// Index into `chat` where the current test began.
    pub test_start: usize,
    pub windows: BTreeMap<u32, Window>,
    pub responses: HashMap<u32, Result<String, String>>,
    /// `DebugResponse::Refused` texts (the gate is closed or the request was not understood).
    pub refused: Vec<String>,
    /// Newest acked move `seq`.
    pub acked: u32,
    pub notices: Vec<String>,
    /// Files of the sounds the client was sent, oldest first.
    pub sounds: Vec<String>,
    /// Admin channel: the last `Rights`, player list, tickets, and every `Done`/`Refused` as
    /// `ok: ...`/`refused: ...`.
    pub admin_rights: Option<(String, u32)>,
    pub admin_players: Value,
    pub admin_tickets: Value,
    pub admin_lists: Map<String, Value>,
    pub admin_replies: Vec<String>,
}

pub struct Bot {
    pub name: String,
    pub token: String,
    pub app: App,
    seq: u32,
    next_request: u32,
}

impl Bot {
    pub fn connect(name: &str, token: String, addr: SocketAddr, digest: &str) -> Self {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(NetClientConfig {
                server_addr: addr,
                certificate_digest: digest.to_owned(),
                token: token.clone(),
            })
            .add_plugins(NetClientPlugin)
            .init_resource::<Observed>()
            .add_systems(Update, record);
        app.finish();
        app.cleanup();
        Self {
            name: name.to_owned(),
            token,
            app,
            seq: 0,
            next_request: 1,
        }
    }

    pub fn update(&mut self) {
        self.app.update();
    }

    pub fn observed(&self) -> &Observed {
        self.app.world().resource::<Observed>()
    }

    pub fn observed_mut(&mut self) -> Mut<'_, Observed> {
        self.app.world_mut().resource_mut::<Observed>()
    }

    pub fn state(&self) -> &NetClientState {
        self.app.world().resource::<NetClientState>()
    }

    /// Connected with its own mob replicated.
    pub fn ready(&self) -> bool {
        *self.state() == NetClientState::Connected
            && self
                .app
                .world()
                .resource::<NetEntityMap>()
                .controlled()
                .is_some()
    }

    pub fn send(&mut self, intent: ClientIntent) {
        self.app.world_mut().write_message(SendIntent(intent));
    }

    /// Sends the next `Move`; returns its `seq` (acked when `observed().acked >= seq`).
    pub fn step(&mut self, dir: ss13_shared::Dir) -> u32 {
        self.seq += 1;
        let seq = self.seq;
        self.send(ClientIntent::Move { dir, seq });
        seq
    }

    pub fn send_admin(&mut self, request: AdminRequest) {
        self.app
            .world_mut()
            .write_message(SendAdminRequest(request));
    }

    /// The newest ticket of `ckey` among the tickets this bot was sent.
    pub fn ticket_of(&self, ckey: &str) -> Option<u32> {
        self.observed()
            .admin_tickets
            .as_array()?
            .iter()
            .rev()
            .find(|t| t["initiator"] == ckey)
            .and_then(|t| t["id"].as_u64())
            .map(|id| id as u32)
    }

    /// Sends a test hook command; the answer lands in `observed().responses[id]`.
    pub fn request(&mut self, command: &Value) -> u32 {
        let id = self.next_request;
        self.next_request += 1;
        self.app
            .world_mut()
            .write_message(SendDebugRequest(DebugRequest::Test {
                id,
                command: command.to_string(),
            }));
        id
    }

    pub fn close_window(&mut self, window: u32) {
        self.app
            .world_mut()
            .write_message(SendUi(UiClientMessage::Close { window }));
    }

    /// The newest open window of `interface`.
    pub fn window(&self, interface: &str) -> Option<u32> {
        self.observed()
            .windows
            .iter()
            .rev()
            .find(|(_, w)| w.interface == interface)
            .map(|(&id, _)| id)
    }

    /// The own mob's tile as replicated to this bot.
    pub fn pos(&self) -> Option<TilePos> {
        let world = self.app.world();
        let mob = world.resource::<NetEntityMap>().controlled()?;
        world.get::<TilePos>(mob).copied()
    }

    pub fn own_id(&self) -> Option<NetId> {
        let world = self.app.world();
        let mob = world.resource::<NetEntityMap>().controlled()?;
        world.get::<NetEntity>(mob).map(|n| n.0)
    }

    /// Whether `id` is replicated to this bot (it can only click what it has).
    pub fn sees(&self, id: NetId) -> bool {
        self.app
            .world()
            .resource::<NetEntityMap>()
            .get(id)
            .is_some()
    }

    /// What a click on the tile's center hits: the topmost replicated thing by plane and layer
    /// whose icon or overlay is drawn at that pixel, skipping click-through atoms
    /// (`mouse_opacity = 0`), like `ss13_game::client::topmost_at` (a wall light on the tile's
    /// edge does not cover the console under it).
    pub fn topmost(&mut self, pos: TilePos, icons: &IconNames) -> Option<NetId> {
        let world = self.app.world_mut();
        let mut q = world.query::<(&NetEntity, &TilePos, Option<&Appearance>, Option<&Facing>)>();
        let mut on_tile: Vec<_> = q
            .iter(world)
            .filter(|(_, p, a, _)| {
                **p == pos && a.is_none_or(|a| a.mouse_opacity != MouseOpacity::Transparent)
            })
            .collect();
        let key = |a: Option<&Appearance>| a.map_or((i16::MIN, 0.0), |a| (a.plane, a.layer.0));
        on_tile.sort_by(|a, b| {
            let (ka, kb) = (key(a.2), key(b.2));
            kb.0.cmp(&ka.0)
                .then(kb.1.total_cmp(&ka.1))
                .then(b.0.0.0.cmp(&a.0.0.0))
        });
        on_tile
            .into_iter()
            .find(
                |(_, _, appearance, facing)| match (&icons.pixels, appearance) {
                    (Some(pixels), Some(a)) => pixels.hits(a, facing.map_or(Dir::South, |f| f.0)),
                    _ => true,
                },
            )
            .map(|(n, ..)| n.0)
    }

    /// The bot's observation as JSON (docs/RUNNING.md, "Functional test bots").
    pub fn observe(&mut self, icons: &IconNames) -> Value {
        let world = self.app.world_mut();
        let connected = *world.resource::<NetClientState>() == NetClientState::Connected;
        let own = world
            .resource::<NetEntityMap>()
            .controlled()
            .and_then(|e| world.get::<TilePos>(e).copied());
        let mut q = world
            .query_filtered::<(&NetEntity, &TilePos, Option<&Appearance>), Without<LocalControl>>();
        let seen: Vec<Value> = q
            .iter(world)
            // A client whose view is elsewhere (a camera console) knows no own mob: its
            // replicated entities are all on the viewed level.
            .filter(|(_, p, _)| own.is_none_or(|o| o.z == p.z))
            .map(|(n, p, a)| {
                json!({
                    "id": n.0.0,
                    "pos": [p.x, p.y],
                    "icon": a.map(|a| icons.name(u64::from(a.icon_state.0))),
                })
            })
            .collect();
        let observed = world.resource::<Observed>();
        let mut windows = Map::new();
        for w in observed.windows.values() {
            windows.insert(
                w.interface.clone(),
                json!({
                    "title": w.title,
                    "status": format!("{:?}", w.status),
                    "data": named_icons(decode_ui(&w.interface, &w.data, false), icons),
                    "static": named_icons(decode_ui(&w.interface, &w.static_data, true), icons),
                }),
            );
        }
        let data = |name: &str| windows.get(name).map_or(Value::Null, |w| w["data"].clone());
        let inventory = data(ss13_game::hud::InventoryHudUi::NAME);
        let health = data(ss13_game::hud::HealthHudUi::NAME);
        let interact = data(ss13_game::hud::InteractHudUi::NAME);
        let slots: Map<String, Value> = ss13_game::inventory::ALL_SLOTS
            .iter()
            .skip(2)
            .map(|&s| {
                let key = ss13_debug::test_hooks::slot_name(s);
                (
                    key.to_owned(),
                    inventory.get(key).cloned().unwrap_or(Value::Null),
                )
            })
            .collect();
        let pockets = inventory
            .get("pockets")
            .cloned()
            .unwrap_or(json!([null, null]));
        let mut slots = slots;
        slots.insert("left_pocket".into(), pockets[0].clone());
        slots.insert("right_pocket".into(), pockets[1].clone());
        let alerts: Vec<Value> = data(ss13_game::hud::AlertHudUi::NAME)["alerts"]
            .as_array()
            .map(|a| a.iter().map(|a| a["name"].clone()).collect())
            .unwrap_or_default();
        json!({
            "name": self.name,
            "connected": connected,
            "pos": own.map(|p| json!([p.x, p.y])),
            "z": own.map(|p| p.z),
            "hands": inventory.get("hands").cloned().unwrap_or(json!([null, null])),
            "active": inventory.get("active").cloned(),
            "slots": slots,
            "health": health.get("health").cloned(),
            "combat": health.get("combat_mode").cloned(),
            "zone": health.get("zone_selected").cloned(),
            "immobile": health.get("immobile").cloned(),
            "hunger": data(ss13_game::hud::HungerHudUi::NAME),
            "mood": data(ss13_game::hud::MoodHudUi::NAME),
            "alerts": alerts,
            "throw_mode": interact.get("throw_mode").cloned(),
            "pulling": interact.get("pulling").cloned(),
            "windows": windows,
            "chat": observed.chat[observed.test_start.min(observed.chat.len())..],
            "chat_all": observed.chat,
            "notices": observed.notices,
            "sounds": observed.sounds,
            "seen": seen,
            "admin": {
                "rank": observed.admin_rights.as_ref().map(|r| r.0.clone()),
                "rights": observed.admin_rights.as_ref().map(|r| r.1),
                "players": observed.admin_players,
                "tickets": observed.admin_tickets,
                "lists": observed.admin_lists,
                "replies": observed.admin_replies,
                "last": observed.admin_replies.last(),
            },
        })
    }
}

fn record(
    mut chat: MessageReader<ChatReceived>,
    mut ui: MessageReader<UiMessageReceived>,
    mut debug: MessageReader<DebugResponseReceived>,
    mut acks: MessageReader<MoveAckReceived>,
    mut notices: MessageReader<NoticeReceived>,
    mut admin: MessageReader<AdminResponseReceived>,
    mut sounds: MessageReader<ss13_net::sound::client::SoundReceived>,
    mut observed: ResMut<Observed>,
) {
    observed
        .sounds
        .extend(sounds.read().map(|s| s.0.file.clone()));
    for AdminResponseReceived(response) in admin.read() {
        match response {
            AdminResponse::Rights { rank, rights } => {
                observed.admin_rights = Some((rank.clone(), *rights));
            }
            AdminResponse::Players(p) => observed.admin_players = json(p),
            AdminResponse::Tickets(t) => observed.admin_tickets = json(t),
            AdminResponse::Prototypes { ids, .. } => {
                observed.admin_lists.insert("prototypes".into(), json(ids));
            }
            AdminResponse::Events(names) => {
                observed.admin_lists.insert("events".into(), json(names));
            }
            AdminResponse::Dynamic(lines) => {
                observed.admin_lists.insert("dynamic".into(), json(lines));
            }
            AdminResponse::Mind(panel) => {
                observed.admin_lists.insert("mind".into(), json(panel));
            }
            AdminResponse::Done(text) => observed.admin_replies.push(format!("ok: {text}")),
            AdminResponse::Refused(text) => {
                observed.admin_replies.push(format!("refused: {text}"));
            }
            AdminResponse::AdminList { admins, .. } => {
                observed.admin_lists.insert("admins".into(), json(admins));
            }
        }
    }
    for ChatReceived(message) in chat.read() {
        observed.chat.push(message.plain_text());
    }
    let excess = observed.chat.len().saturating_sub(CHAT_KEEP);
    if excess > 0 {
        observed.chat.drain(..excess);
        observed.test_start = observed.test_start.saturating_sub(excess);
    }
    for UiMessageReceived(message) in ui.read() {
        match message {
            UiServerMessage::Open {
                window,
                interface,
                title,
                status,
                static_data,
                data,
            } => {
                observed.windows.insert(
                    *window,
                    Window {
                        interface: interface.clone(),
                        title: title.clone(),
                        status: *status,
                        data: data.clone(),
                        static_data: static_data.clone(),
                    },
                );
            }
            UiServerMessage::Update {
                window,
                status,
                data,
            } => {
                if let Some(w) = observed.windows.get_mut(window) {
                    w.status = *status;
                    if let Some(data) = data {
                        w.data.clone_from(data);
                    }
                }
            }
            UiServerMessage::Close { window } => {
                observed.windows.remove(window);
            }
        }
    }
    for DebugResponseReceived(response) in debug.read() {
        match response {
            DebugResponse::Test { id, result } => {
                observed.responses.insert(*id, result.clone());
            }
            DebugResponse::Refused(why) => observed.refused.push(why.clone()),
            _ => {}
        }
    }
    for MoveAckReceived(ack) in acks.read() {
        observed.acked = observed.acked.max(ack.seq);
    }
    for NoticeReceived(notice) in notices.read() {
        observed.notices.push(notice.text.clone());
    }
}

fn json<T: Serialize>(value: &T) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

fn to_json<T: DeserializeOwned + Serialize>(bytes: &[u8]) -> Value {
    ss13_tgui::decode::<T>(bytes)
        .ok()
        .and_then(|v| serde_json::to_value(v).ok())
        .unwrap_or(Value::Null)
}

macro_rules! ui_decoders {
    ($($t:ty),* $(,)?) => {
        /// Window data as JSON for the interfaces this crate knows; `null` for others.
        pub fn decode_ui(interface: &str, bytes: &[u8], static_data: bool) -> Value {
            $(
                if interface == <$t as UiInterface>::NAME {
                    return if static_data {
                        to_json::<<$t as UiInterface>::StaticData>(bytes)
                    } else {
                        to_json::<<$t as UiInterface>::Data>(bytes)
                    };
                }
            )*
            Value::Null
        }
    };
}

ui_decoders!(
    ss13_game::mecha::ExosuitFabricatorUi,
    ss13_game::hud::InventoryHudUi,
    ss13_game::hud::StorageUi,
    ss13_game::hud::HealthHudUi,
    ss13_game::hud::InteractHudUi,
    ss13_game::hud::AlertHudUi,
    ss13_game::hud::HungerHudUi,
    ss13_game::hud::MoodHudUi,
    ss13_game::hud::FullscreenHudUi,
    ss13_game::lobby::LobbyUi,
    ss13_game::lobby::JobPrefsUi,
    ss13_game::comms::CommsConsoleUi,
    ss13_game::antag::AntagInfoTraitorUi,
    ss13_game::antag::AntagInfoBrotherUi,
    ss13_game::antag::UplinkUi,
    ss13_game::antag::AntagHudUi,
    ss13_game::antag::vampire::AntagInfoVampireUi,
    ss13_game::antag::vampire::VampireHudUi,
    ss13_game::antag::changeling::AntagInfoChangelingUi,
    ss13_game::antag::changeling::CellularEmporiumUi,
    ss13_game::antag::changeling::ChangelingHudUi,
    ss13_game::antag::nukeop::NuclearBombUi,
    ss13_game::antag::nukeop::ShuttleConsoleUi,
    ss13_game::antag::cult::AntagInfoGenericUi,
    ss13_game::antag::revolution::AntagInfoRevolutionUi,
    ss13_tgui::interfaces::text_input::TextInputUi,
    ss13_game::antag::heretic::AntagInfoHereticUi,
    ss13_game::actions::ActionButtonsUi,
    ss13_game::modpc::NtosUi,
    ss13_game::panels::StripMenuUi,
    ss13_game::panels::LootPanelUi,
    ss13_tgui::interfaces::vending::VendingUi,
    ss13_tgui::interfaces::wires::WiresUi,
    ss13_tgui::interfaces::solar_control::SolarControlUi,
    ss13_tgui::interfaces::mech_bay::MechBayPowerConsoleUi,
    ss13_tgui::interfaces::ai_airlock::AiAirlockUi,
    ss13_tgui::interfaces::apc::ApcUi,
    ss13_tgui::interfaces::thermo_machine::ThermoMachineUi,
    ss13_tgui::interfaces::job_selection::JobSelectionUi,
    ss13_tgui::interfaces::atmos::AtmosPumpUi,
    ss13_tgui::interfaces::atmos::AtmosFilterUi,
    ss13_tgui::interfaces::atmos::AtmosMixerUi,
    ss13_tgui::interfaces::atmos::CanisterUi,
    ss13_tgui::interfaces::atmos::PortablePumpUi,
    ss13_tgui::interfaces::atmos::PortableScrubberUi,
    ss13_tgui::interfaces::air_alarm::AirAlarmUi,
    ss13_tgui::interfaces::list_input::ListInputUi,
    ss13_tgui::interfaces::disposal_unit::DisposalUnitUi,
    ss13_tgui::interfaces::destination_tagger::DestinationTaggerUi,
    ss13_tgui::interfaces::dna_console::DnaConsoleUi,
    ss13_game::science::TechwebUi,
    ss13_game::science::FabricatorUi,
    ss13_game::science::DestructiveAnalyzerUi,
    ss13_game::science::ExperimentConfigureUi,
    ss13_game::science::ServerControlUi,
    ss13_game::silicon::ui::CameraConsoleUi,
    ss13_game::silicon::ui::CameraViewUi,
    ss13_game::silicon::ui::ListInputUi,
    ss13_game::records::SecurityRecordsUi,
    ss13_game::records::MedicalRecordsUi,
    ss13_game::records::DataHudUi,
);

/// HUD icons (`{"icon_state": id, "color": ...}`) become their icon state names.
fn named_icons(value: Value, icons: &IconNames) -> Value {
    match value {
        Value::Object(o) if o.contains_key("icon_state") && o.len() <= 2 => o["icon_state"]
            .as_u64()
            .map_or(Value::Object(o.clone()), |id| icons.name(id)),
        Value::Object(o) => Value::Object(
            o.into_iter()
                .map(|(k, v)| (k, named_icons(v, icons)))
                .collect(),
        ),
        Value::Array(a) => Value::Array(a.into_iter().map(|v| named_icons(v, icons)).collect()),
        v => v,
    }
}

#[cfg(test)]
mod tests {
    use super::PageAlpha;

    #[test]
    fn page_alpha_from_a_page_file() {
        // 3x2, rows top-down: (1,0) has alpha 1, (2,1) alpha 255, the rest 0.
        let mut rgba = vec![200u8; 3 * 2 * 4];
        for (i, px) in rgba.chunks_exact_mut(4).enumerate() {
            px[3] = [0, 1, 0, 0, 0, 255][i];
        }
        let file = std::env::temp_dir().join(format!("ss13_bots_page_{}.qoi", std::process::id()));
        std::fs::write(&file, ss13_assets::page::encode(3, 2, &rgba).unwrap()).unwrap();
        let page = PageAlpha::decode(&file).unwrap();
        std::fs::remove_file(&file).ok();
        let drawn: Vec<(u32, u32)> = (0..2)
            .flat_map(|y| (0..3).map(move |x| (x, y)))
            .filter(|&(x, y)| page.opaque(x, y))
            .collect();
        assert_eq!(drawn, [(1, 0), (2, 1)]);
        assert!(!page.opaque(0, 9), "outside");
    }
}
