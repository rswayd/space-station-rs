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
    /// `NtosCivCargoHoldTerminal`: the civilian bounty console's data without a pad.
    CivBounties(Box<crate::cargo::CivBountyData>),
    /// `NtosBountyBoard`.
    BountyBoard(BountyBoardData),
    /// `NtosRestock`: `vending_list`.
    Restock(Vec<VendorRow>),
    /// `NtosScipaper` (NT Frontier).
    Scipaper(Box<ScipaperData>),
    /// `NtosRobotact`.
    Robotact(Box<RobotactData>),
}

/// `/datum/computer_file/program/robotact/ui_data` and `ui_static_data`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RobotactData {
    pub borg_name: String,
    /// `designation`: the model's name.
    pub designation: String,
    /// `masterAI`: `None` when not connected (`MasterAI_connected`).
    pub master_ai: Option<String>,
    pub master_ai_online: bool,
    /// Joules.
    pub charge: f64,
    pub max_charge: f64,
    /// `(health + 100) / 2`.
    pub integrity: f32,
    pub sensors: bool,
    pub self_destruct_able: bool,
    /// Every cyborg on the same AI (`evaluate_borg`).
    pub cyborgs: Vec<RobotactPeer>,
    pub cover_locked: bool,
    /// `FAULT`, `DISABLED`, `ENABLED`.
    pub locomotion: String,
    /// `FAULT` or `NOMINAL`.
    pub wire_module: String,
    /// `FAULT`, `DISABLED` or `NOMINAL`.
    pub wire_camera: String,
    /// `FAULT`, `CONNECTED` or `READY`.
    pub wire_ai: String,
    pub wire_law: String,
    /// `Laws`: `get_law_list(TRUE, TRUE, FALSE)`.
    pub laws: Vec<String>,
    pub borg_log: Vec<String>,
    pub upgrades: Vec<String>,
}

/// An entry of `cyborg_groups`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RobotactPeer {
    pub name: String,
    /// `round((health + 100) / 2)`.
    pub integ: i32,
    pub locked_down: bool,
    /// tg `stat`: 0 conscious, 3 unconscious, 4 dead.
    pub status: u8,
    /// An AI shell nobody controls.
    pub shell_discon: bool,
    /// Cell charge, percent; `None` without a cell.
    pub charge: Option<u8>,
    pub module: String,
}

/// `scipaper_program/return_gist()` of a paper.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PaperGist {
    /// Cooperation and funding as words ("None" .. "Huge").
    pub gains: (String, String),
    pub title: String,
    pub author: String,
    pub et_alia: bool,
    pub abstract_text: String,
    pub experiment_name: Option<String>,
    pub tier: Option<u32>,
    pub partner: Option<String>,
}

/// An `experimentInformation` entry.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct OrdnanceExperimentInfo {
    pub path: String,
    pub name: String,
    pub description: String,
    /// `target_amount` by tier.
    pub target: Vec<f64>,
    /// "Range" / "Gas".
    pub prefix: String,
    /// "tiles" / "moles".
    pub suffix: String,
}

/// A `partnersInformation` entry with its `relations` and `purchaseableBoosts`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PartnerInfo {
    pub path: String,
    pub name: String,
    pub flufftext: String,
    /// Cooperation and funding multipliers.
    pub multipliers: (f64, f64),
    /// Node name, discount, node path.
    pub boosted_nodes: Vec<(String, f64, String)>,
    pub accepted_experiments: Vec<String>,
    pub relation: String,
    /// Node paths that can be bought now.
    pub purchaseable: Vec<String>,
}

/// `scipaper_program/ui_data` and `ui_static_data`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ScipaperData {
    /// 1 publish, 2 published papers, 3 experiments, 4 partners.
    pub current_tab: u8,
    pub has_techweb: bool,
    /// Tab 1: file name -> uid, experiment name -> path, tiers, partner name -> path.
    pub file_list: Vec<(String, u32)>,
    pub exp_list: Vec<(String, String)>,
    pub allowed_tiers: Vec<u32>,
    pub allowed_partners: Vec<(String, String)>,
    pub selected_file: Option<String>,
    pub paper: PaperGist,
    /// Tab 2.
    pub published: Vec<PaperGist>,
    /// Tabs 3 and 4.
    pub experiments: Vec<OrdnanceExperimentInfo>,
    pub partners: Vec<PartnerInfo>,
    /// Paths of the available and researched nodes.
    pub visible_nodes: Vec<String>,
}

/// A vending machine of `restock_tracker/ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct VendorRow {
    pub name: String,
    pub location: String,
    pub credits: i64,
    /// Legal stock, percent.
    pub percentage: f32,
}

/// `/datum/computer_file/program/bounty_board/ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BountyBoardData {
    /// `user`: the account holder and job, `None` without an account.
    pub user: Option<(String, String)>,
    pub requests: Vec<RequestRow>,
    pub applicants: Vec<ApplicantRow>,
}

/// A `/datum/station_request`; `acc_number` is the owner's account id (one request each).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestRow {
    pub owner: String,
    pub value: i64,
    pub description: String,
    pub acc_number: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplicantRow {
    pub name: String,
    pub request_id: u32,
    pub requestee_id: u32,
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

// `NtosRobotact` acts.
pub const ROBOTACT_COVER_UNLOCK: &str = "coverunlock";
pub const ROBOTACT_LAW_CHANNEL: &str = "lawchannel";
pub const ROBOTACT_LAW_STATE: &str = "lawstate";
pub const ROBOTACT_ALERT_POWER: &str = "alertPower";
pub const ROBOTACT_SENSORS: &str = "toggleSensors";
pub const ROBOTACT_SELF_DESTRUCT: &str = "selfDestruct";
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
// `NtosBountyBoard` acts (`request`, `applicant`: account ids); `createBounty` takes
// `bountyval` and `bountytext` (this port's screen sends both as `bounty`: "VALUE TEXT").
pub const BOARD_CREATE: &str = "createBounty";
pub const BOARD_APPLY: &str = "apply";
pub const BOARD_PAY: &str = "payApplicant";
pub const BOARD_DELETE: &str = "deleteRequest";
// `NtosSpectreMeter` acts.
pub const SPECTRE_SCAN: &str = "manual_scan";
pub const SPECTRE_MODE: &str = "toggle_mode";
// `NtosScipaper` acts. `rewrite` takes `title`, `author` and/or `abstract`.
pub const PAPER_ET_ALIA: &str = "et_alia";
pub const PAPER_PUBLISH: &str = "publish";
pub const PAPER_REWRITE: &str = "rewrite";
pub const PAPER_CHANGE_TAB: &str = "change_tab";
pub const PAPER_SELECT_FILE: &str = "select_file";
pub const PAPER_SELECT_EXPERIMENT: &str = "select_experiment";
pub const PAPER_SELECT_TIER: &str = "select_tier";
pub const PAPER_SELECT_PARTNER: &str = "select_partner";
pub const PAPER_PURCHASE_BOOST: &str = "purchase_boost";
