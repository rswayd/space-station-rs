//! The telecomms windows' data, shared by both halves: tgui `Telecomms.jsx` (a machine's
//! configuration), `TelecommsMonitor.tsx`, `ServerMonitor.tsx` (the server log browser) and
//! `NtnetRelay.tsx`. The server half is `crate::telecomms_config` and `crate::ntnet_relay`, the
//! client views `crate::telecomms_client`.

use serde::{Deserialize, Serialize};
use ss13_tgui::UiInterface;

/// tgui `Telecomms`: a telecomms machine's settings.
pub struct TelecommsUi;

impl UiInterface for TelecommsUi {
    const NAME: &'static str = "Telecomms";
    type Data = TelecommsData;
    type StaticData = ();
}

/// A `links` entry; `index` is 1-based (tg `unlink`'s `value`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkedMachine {
    pub index: usize,
    pub name: String,
    pub id: String,
}

/// `ui_data` with `add_option`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TelecommsData {
    /// `add_option` `type`: "bus", "relay" or empty.
    pub kind: String,
    pub changefrequency: u16,
    pub broadcasting: bool,
    pub receiving: bool,
    pub minfreq: u16,
    pub maxfreq: u16,
    /// `tempfreq`, in tenths of kHz.
    pub frequency: u16,
    /// A multitool in the active hand (or a silicon).
    pub multitool: bool,
    /// The multitool's buffer: its name and id.
    pub multibuff: Option<String>,
    pub toggled: bool,
    pub id: String,
    pub network: String,
    pub prefab: bool,
    pub linked: Vec<LinkedMachine>,
    pub frequencies: Vec<u16>,
}

pub const TOGGLE: &str = "toggle";
pub const ID: &str = "id";
pub const NETWORK: &str = "network";
pub const TEMPFREQ: &str = "tempfreq";
pub const FREQ: &str = "freq";
pub const DELETE: &str = "delete";
pub const UNLINK: &str = "unlink";
pub const LINK: &str = "link";
pub const BUFFER: &str = "buffer";
pub const FLUSH: &str = "flush";
pub const BROADCAST: &str = "broadcast";
pub const RECEIVE: &str = "receive";
pub const CHANGE_FREQ: &str = "change_freq";
pub const VALUE: &str = "value";

/// tgui `TelecommsMonitor`.
pub struct TelecommsMonitorUi;

impl UiInterface for TelecommsMonitorUi {
    const NAME: &'static str = "TelecommsMonitor";
    type Data = TelecommsMonitorData;
    type StaticData = ();
}

/// A machine in the monitor's buffer.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonitorMachine {
    pub id: String,
    pub name: String,
    pub icon: String,
}

/// The `MACHINE_VIEW` machine.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonitorSelected {
    pub id: String,
    pub name: String,
    pub network: String,
    pub linked_machines: Vec<MonitorMachine>,
}

/// `ui_data`: `screen` 0 is `MAIN_VIEW`, 1 `MACHINE_VIEW`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TelecommsMonitorData {
    pub screen: u8,
    pub status_message: Option<String>,
    pub network: String,
    pub machines: Vec<MonitorMachine>,
    pub machine: Option<MonitorSelected>,
}

pub const HOME: &str = "home";
pub const VIEW: &str = "view";
pub const PROBE: &str = "probe";
/// `view`'s and `probe`'s param.
pub const MACHINE_ID: &str = "id";

/// tgui `ServerMonitor`: the telecommunications server monitoring console.
pub struct ServerMonitorUi;

impl UiInterface for ServerMonitorUi {
    const NAME: &'static str = "ServerMonitor";
    type Data = ServerMonitorData;
    type StaticData = ();
}

/// A server in the buffer; `server_ref` is its index there.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonitoredServer {
    pub server_ref: String,
    pub name: String,
    pub id: String,
}

/// A `/datum/comm_log_entry` as the viewer reads it; `packet_ref` is its index in the log.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Packet {
    pub name: String,
    pub language: String,
    pub message: String,
    pub race: String,
    pub source: String,
    pub job: String,
    pub kind: String,
    pub packet_ref: String,
}

/// `SERVER_VIEW`'s server.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerView {
    pub name: String,
    pub traffic: u32,
    pub packets: Vec<Packet>,
}

/// `ui_data`: `screen` 0 is `MAIN_VIEW`, 1 `SERVER_VIEW`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerMonitorData {
    pub screen: u8,
    pub network: String,
    pub error: String,
    pub servers: Vec<MonitoredServer>,
    pub server: Option<ServerView>,
}

pub const SCAN_NETWORK: &str = "scan_network";
pub const NETWORK_ID: &str = "network_id";
pub const CLEAR_BUFFER: &str = "clear_buffer";
pub const VIEW_SERVER: &str = "view_server";
pub const SERVER: &str = "server";
pub const RETURN_HOME: &str = "return_home";
pub const DELETE_PACKET: &str = "delete_packet";
pub const PACKET_REF: &str = "ref";

/// tgui `NtnetRelay`.
pub struct NtnetRelayUi;

impl UiInterface for NtnetRelayUi {
    const NAME: &'static str = "NtnetRelay";
    type Data = NtnetRelayData;
    type StaticData = ();
}

/// `ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NtnetRelayData {
    pub enabled: bool,
    pub dos_capacity: f64,
    pub dos_overload: f64,
    pub dos_crashed: bool,
}

pub const RESTART: &str = "restart";
