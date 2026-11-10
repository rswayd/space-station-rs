//! The brig after tg `brigdoors.dm` (`/obj/machinery/status_display/door_timer`, tgui
//! `BrigTimer`) and `computer/prisoner/management.dm` (tgui `PrisonerManagement`).
//!
//! [`timer`] holds the cell timers' server half, [`prisoner`] the prisoner management console
//! and the prisoner ID card's points, [`client`] draws both windows. This module holds what both
//! halves share: the windows' data and act names. [`gulag`] holds the labor camp teleporter,
//! its console and the item reclaimer, [`labor`] the labor camp's point consoles, the
//! license plate press and the produceporter.

use serde::{Deserialize, Serialize};
use ss13_tgui::UiInterface;

#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "server")]
pub mod gulag;
#[cfg(feature = "server")]
pub mod labor;
#[cfg(feature = "server")]
pub mod prisoner;
#[cfg(feature = "server")]
pub mod sentence;
#[cfg(feature = "server")]
pub mod timer;

/// The brig on the server: cell timers and the prisoner management console.
#[cfg(feature = "server")]
pub struct BrigServerPlugin;

#[cfg(feature = "server")]
impl bevy::prelude::Plugin for BrigServerPlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        app.add_plugins((
            timer::DoorTimerPlugin,
            prisoner::PrisonerConsolePlugin,
            gulag::GulagPlugin,
            labor::LaborPlugin,
            sentence::SentencePlugin,
        ));
    }
}

/// tgui `BrigTimer`.
pub struct BrigTimerUi;

impl UiInterface for BrigTimerUi {
    const NAME: &'static str = "BrigTimer";
    type Data = BrigTimerData;
    type StaticData = BrigTimerPresets;
}

/// BandaStation's `presets` (minutes): constant, so sent once.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrigTimerPresets {
    pub short: u32,
    pub medium: u32,
    pub long: u32,
}

/// `door_timer/ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrigTimerData {
    pub timing: bool,
    pub minutes: u32,
    pub seconds: u32,
    pub flash_charging: bool,
}

// `BrigTimer` acts.
pub const TIME: &str = "time";
pub const START: &str = "start";
pub const STOP: &str = "stop";
pub const FLASH: &str = "flash";
pub const PRESET: &str = "preset";
pub const PRESET_SHORT: &str = "short";
pub const PRESET_MEDIUM: &str = "medium";
pub const PRESET_LONG: &str = "long";

/// tgui `PrisonerManagement`.
pub struct PrisonerManagementUi;

impl UiInterface for PrisonerManagementUi {
    const NAME: &'static str = "PrisonerManagement";
    type Data = PrisonerManagementData;
    type StaticData = ();
}

/// `management/ui_data`; only `authorized` until a login.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrisonerManagementData {
    pub authorized: bool,
    pub inserted_id: Option<PrisonerIdInfo>,
    pub implants: Vec<ImplantInfo>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrisonerIdInfo {
    pub name: String,
    pub points: u32,
    pub goal: u32,
}

/// One tracked implant: `get_management_console_data()` (label, value pairs in order),
/// `get_management_console_buttons()`, `initial(name)` and `REF(implant)`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImplantInfo {
    pub info: Vec<(String, String)>,
    pub buttons: Vec<ImplantButton>,
    pub category: String,
    pub implant_ref: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImplantButton {
    pub name: String,
    /// tgui color name (`bad`, `average`, `good`).
    pub color: String,
    pub tooltip: String,
    pub action_key: String,
    pub action_params: Vec<(String, String)>,
}

// `PrisonerManagement` acts.
pub const LOGIN: &str = "login";
pub const LOGOUT: &str = "logout";
pub const INSERT_ID: &str = "insert_id";
pub const EJECT_ID: &str = "eject_id";
pub const SET_ID_GOAL: &str = "set_id_goal";
pub const RESET_ID: &str = "reset_id";
pub const HANDLE_IMPLANT: &str = "handle_implant";
// `implant_action` keys.
pub const SELF_DESTRUCT: &str = "self_destruct";
pub const WARN: &str = "warn";
pub const INJECT: &str = "inject";

/// tgui `GulagTeleporterConsole`.
pub struct GulagTeleporterUi;

impl UiInterface for GulagTeleporterUi {
    const NAME: &'static str = "GulagTeleporterConsole";
    type Data = GulagTeleporterData;
    type StaticData = ();
}

/// `gulag_teleporter_computer/ui_data`; `None` locations are tg's `null` teleporter/beacon.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GulagTeleporterData {
    /// `prisoner["name"]`, `prisoner["crimstat"]`.
    pub prisoner_name: Option<String>,
    pub prisoner_crimstat: Option<String>,
    pub teleporter_location: Option<String>,
    pub teleporter_lock: bool,
    pub teleporter_state_open: bool,
    pub beacon_location: Option<String>,
    /// `id_name` of the inserted prisoner ID; `None` without one.
    pub id_name: Option<String>,
    pub goal: u32,
    pub can_teleport: bool,
}

// `GulagTeleporterConsole` acts.
pub const SCAN_TELEPORTER: &str = "scan_teleporter";
pub const SCAN_BEACON: &str = "scan_beacon";
pub const HANDLE_ID: &str = "handle_id";
pub const SET_GOAL: &str = "set_goal";
pub const TOGGLE_OPEN: &str = "toggle_open";
pub const TELEPORTER_LOCK: &str = "teleporter_lock";
pub const TELEPORT: &str = "teleport";

/// tgui `GulagItemReclaimer`.
pub struct GulagItemReclaimerUi;

impl UiInterface for GulagItemReclaimerUi {
    const NAME: &'static str = "GulagItemReclaimer";
    type Data = GulagItemReclaimerData;
    type StaticData = ();
}

/// `gulag_item_reclaimer/ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GulagItemReclaimerData {
    pub can_reclaim: bool,
    /// `mobs`: (`mob` ref, `name`).
    pub mobs: Vec<(String, String)>,
}

/// `GulagItemReclaimer` act, with the `mobref` param.
pub const RELEASE_ITEMS: &str = "release_items";

/// tgui `LaborClaimConsole`.
pub struct LaborClaimUi;

impl UiInterface for LaborClaimUi {
    const NAME: &'static str = "LaborClaimConsole";
    type Data = LaborClaimData;
    type StaticData = ();
}

/// `labor_claim_console/ui_data` with `ui_static_data`'s `shuttle_exists`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaborClaimData {
    pub can_go_home: bool,
    pub id_points: u32,
    pub status_info: String,
    pub unclaimed_points: i64,
    pub shuttle_exists: bool,
}

// `LaborClaimConsole` acts.
pub const CLAIM_POINTS: &str = "claim_points";
pub const MOVE_SHUTTLE: &str = "move_shuttle";
