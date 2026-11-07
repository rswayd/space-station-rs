//! The tgui contract of the station programs (`programs::station`): one [`StationScreen`] per
//! program, inside `NtosScreen::Station`, and their act names.

use serde::{Deserialize, Serialize};

/// The screen of a station program in front.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum StationScreen {
    /// `NtosPay`.
    Pay(PayData),
    /// `NtosRadar` (Lifeline, Custodial Locator).
    Radar(RadarData),
    /// `NtosCyborgRemoteMonitor`.
    Borgs(Vec<BorgRow>),
    /// `NtosRoboControl`.
    Bots(Vec<BotRow>),
    /// `NtosRevelation`.
    Revelation { armed: bool },
    /// `NtosPhysScanner`: the last health scan, line by line.
    PhysScanner { last_record: Vec<String> },
    /// `NtosSpectreMeter`.
    SpectreMeter {
        spook_value: u32,
        auto_mode: bool,
        on_cooldown: bool,
    },
}

/// `/datum/computer_file/program/nt_pay/ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PayData {
    /// `name`: the ID's account holder; `None` without an account.
    pub name: Option<String>,
    pub owner_token: String,
    pub money: i64,
    /// `wanted_token`: the answer to the last `GetPayToken`.
    pub wanted_token: Option<String>,
    /// `transaction_list`, newest first.
    pub transactions: Vec<PayRecord>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PayRecord {
    pub adjusted_money: i64,
    pub reason: String,
}

/// `/datum/computer_file/program/radar/ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RadarData {
    pub selected: Option<String>,
    pub scanning: bool,
    pub objects: Vec<RadarObject>,
    pub target: Option<RadarTarget>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RadarObject {
    pub object_ref: String,
    pub name: String,
}

/// `track()`: the target on the 48x48 grid around the computer (24, 24), or a rotating arrow
/// farther than 24 tiles; `pointer` is `crosshairs`, `caret-up` or `caret-down`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RadarTarget {
    pub locx: i32,
    pub locy: i32,
    pub userot: bool,
    pub rot: i32,
    pub pointer: String,
    /// `get_dist_euclidean`, tiles.
    pub distance: f32,
}

/// A cyborg of `/datum/computer_file/program/borg_monitor/ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BorgRow {
    pub name: String,
    pub borg_ref: String,
    /// Cell charge, percent; `None` without a cell.
    pub charge: Option<u8>,
    pub low_power: bool,
    pub area: String,
}

/// A bot of `/datum/computer_file/program/robocontrol/ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BotRow {
    pub name: String,
    pub bot_ref: String,
    pub mode: String,
    pub on: bool,
    pub area: String,
}

// `NtosPay` acts.
pub const PAY_TRANSACTION: &str = "Transaction";
pub const PAY_GET_TOKEN: &str = "GetPayToken";
// `NtosRadar` acts.
pub const RADAR_SELECT: &str = "selecttarget";
pub const RADAR_SCAN: &str = "scan";
// `NtosCyborgRemoteMonitor`: the message comes as the `message` param (tg asks with an input).
pub const BORG_MESSAGE: &str = "messagebot";
// `NtosRoboControl`: `interaction` is `on` or `off`.
pub const BOT_INTERACT: &str = "interact";
// `NtosRevelation` acts.
pub const REVELATION_ARM: &str = "PRG_arm";
pub const REVELATION_ACTIVATE: &str = "PRG_activate";
// `NtosSpectreMeter` acts.
pub const SPECTRE_SCAN: &str = "manual_scan";
pub const SPECTRE_MODE: &str = "toggle_mode";
