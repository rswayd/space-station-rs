//! Modular computers after tg `/obj/item/modular_computer` (the PDA, `/obj/item/modular_computer/pda`)
//! and its programs (`/datum/computer_file/program/*`). This module holds the tgui interface both
//! halves share; [`pda`] is the server, [`programs`] the programs, [`client`] draws the window.
//!
//! tg swaps one window between interfaces (`update_tablet_open_uis`: `NtosMain`, then the active
//! program's `tgui_id`). Here the window is one interface, `NtosMain`, whose data says which
//! screen it shows ([`NtosScreen`]): the main menu or the active program.

use serde::{Deserialize, Serialize};
use ss13_tgui::UiInterface;

#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "server")]
pub mod pda;
pub mod programs;

/// The NtOS window of one computer.
pub struct NtosUi;

impl UiInterface for NtosUi {
    const NAME: &'static str = "NtosMain";
    type Data = NtosData;
    type StaticData = ();
}

/// `get_header_data()` plus the screen.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NtosData {
    pub header: NtosHeader,
    pub screen: NtosScreen,
}

/// `NtosWindow` header (`PC_*`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NtosHeader {
    /// The window title: `NtosMain`'s "NtOS Главное Меню" or the program's `filedesc`.
    pub title: String,
    /// `PC_stationtime`.
    pub station_time: String,
    /// `PC_batterypercent`; `None` without a cell.
    pub battery_percent: Option<u8>,
    /// `get_ntnet_status()`: 0 none, 1 low, 2 good, 3 ethernet.
    pub ntnet: u8,
    /// `PC_programheaders`: `ui_header` of the programs running in the background.
    pub program_headers: Vec<String>,
    /// `PC_showexitprogram`: a program is open (minimize/close instead of power off).
    pub show_exit: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum NtosScreen {
    Main(NtosMainData),
    Messenger(MessengerData),
    Notepad(NotepadData),
    CrewManifest(Vec<ManifestDepartment>),
    GasAnalyzer(Vec<GasMix>),
    Card(Box<CardModData>),
    /// `NtosStationAlertConsole`.
    Alarms(crate::silicon::ui::StationAlertData),
    /// `NtosRecords` in `security` mode.
    SecRecords(Vec<SecurityRow>),
    /// `NtosRecords` in `medical` mode.
    MedRecords(Vec<MedicalRow>),
    /// `NtosSignaler`.
    Signaler(crate::assemblies::SignalerData),
    /// `NtosStatus`.
    Status(StatusDisplayData),
    /// BandaStation `NtosMinesweeperPanel`.
    Minesweeper(Box<MinesweeperData>),
    /// BandaStation `NtosNavigator`.
    Navigator(Box<NavigatorData>),
    /// `NtosNetDownloader`.
    Downloader(Box<DownloaderData>),
    /// `NtosNetChat`.
    Chat(Box<ChatClientData>),
}

impl Default for NtosScreen {
    fn default() -> Self {
        Self::Main(NtosMainData::default())
    }
}

/// `/obj/item/modular_computer/ui_data` without a program.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NtosMainData {
    pub has_light: bool,
    pub light_on: bool,
    /// `login`: the imprint (`saved_identification`, `saved_job`).
    pub login: Login,
    /// `proposed_login`: the inserted ID's registration.
    pub proposed_login: Login,
    pub id_inserted: bool,
    pub show_imprint: bool,
    pub programs: Vec<ProgramEntry>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Login {
    pub name: Option<String>,
    pub job: Option<String>,
}

/// A `programs` entry.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProgramEntry {
    /// `filename`: the `PC_runprogram` / `PC_killprogram` param.
    pub name: String,
    pub desc: String,
    pub header_program: bool,
    pub running: bool,
    pub icon: String,
    pub alert: bool,
}

/// `/datum/computer_file/program/messenger/ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessengerData {
    /// `null` until the PDA is imprinted ("Please imprint an ID to continue").
    pub owner: Option<MessengerEntry>,
    pub saved_chats: Vec<ChatData>,
    /// Every other visible messenger, sorted by job or name.
    pub messengers: Vec<MessengerEntry>,
    pub sort_by_job: bool,
    pub alert_silenced: bool,
    pub sending_and_receiving: bool,
    /// A chat ref or a messenger ref (a new chat), or the contacts screen.
    pub open_chat: Option<String>,
    pub ringtone: String,
}

impl MessengerData {
    pub fn chat(&self, chat_ref: &str) -> Option<&ChatData> {
        self.saved_chats.iter().find(|c| c.chat_ref == chat_ref)
    }

    pub fn messenger(&self, messenger_ref: &str) -> Option<&MessengerEntry> {
        self.messengers
            .iter()
            .find(|m| m.messenger_ref == messenger_ref)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessengerEntry {
    pub name: String,
    pub job: String,
    /// Empty for a recipient that is gone.
    pub messenger_ref: String,
}

/// `/datum/pda_chat/get_ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatData {
    pub chat_ref: String,
    pub recipient: MessengerEntry,
    pub messages: Vec<PdaMessage>,
    pub visible: bool,
    pub can_reply: bool,
    pub unread_messages: u32,
}

/// `/datum/pda_message`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PdaMessage {
    pub message: String,
    pub outgoing: bool,
    pub everyone: bool,
    /// `hh:mm` of the round.
    pub timestamp: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotepadData {
    pub note: String,
}

/// One department of `GLOB.manifest.get_manifest()`: (name, rank) entries.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestDepartment {
    pub name: String,
    pub entries: Vec<(String, String)>,
}

/// `gas_mixture_parser` of one mixture.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GasMix {
    pub name: String,
    /// (gas name, moles), present gases only.
    pub gases: Vec<(String, f64)>,
    pub temperature: f64,
    pub volume: f64,
    pub pressure: f64,
    pub total_moles: f64,
}

/// `/datum/computer_file/program/card_mod` `ui_data` + `ui_static_data` (tgui `NtosCard`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardModData {
    /// `authed_user`: the authenticated card's name (with "[LIMITED ACCESS]" for a head).
    pub authed_user: Option<String>,
    pub auth_card: Option<CardUi>,
    pub modified_card: Option<CardUi>,
    pub is_holding_id: bool,
    /// The regions this login may edit (all station regions for `ACCESS_CHANGE_IDS`).
    pub regions: Vec<RegionUi>,
    /// `wildcard_flags`: (wildcard name, access flags it accepts).
    pub wildcard_flags: Vec<(String, u32)>,
    /// `access_flag_names`.
    pub access_flag_names: Vec<(u32, String)>,
    pub show_basic: bool,
    /// `templates`: the assignments of the job templates this login may apply.
    pub templates: Vec<String>,
}

/// `get_id_ui_data(card)`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardUi {
    pub id_name: String,
    pub id_rank: String,
    pub id_owner: String,
    pub access_on_card: Vec<String>,
    pub wildcard_slots: Vec<WildcardSlotUi>,
    pub id_age: u32,
    pub has_trim: bool,
    pub trim_assignment: String,
    pub trim_access: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WildcardSlotUi {
    pub name: String,
    /// Negative: unlimited.
    pub limit: i32,
    pub usage: Vec<String>,
}

/// `all_region_access_tgui[region]`: the region and its described accesses.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegionUi {
    pub name: String,
    pub accesses: Vec<AccessUi>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccessUi {
    pub desc: String,
    /// `ref`: the access.
    pub access: String,
    /// `access_flags[ref]`.
    pub flag: u32,
}

/// A security entry of `GetRecordsReadable()`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecurityRow {
    pub name: String,
    pub rank: String,
    pub species: String,
    pub gender: String,
    pub age: u32,
    pub fingerprint: String,
    pub wanted: String,
}

/// A medical entry of `GetRecordsReadable()`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MedicalRow {
    pub name: String,
    pub bloodtype: String,
    /// `minor_disabilities_desc`, `major_disabilities_desc`: `<br>`-separated.
    pub mi_dis: String,
    pub ma_dis: String,
    pub physical_status: String,
    pub mental_status: String,
    /// The medical notes' contents, joined with spaces.
    pub notes: String,
}

/// `/datum/computer_file/program/status` `ui_data` + `ui_static_data`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusDisplayData {
    pub upper_text: String,
    pub lower_text: String,
    pub max_status_line_length: u32,
}

/// BandaStation `/datum/computer_file/program/minesweeper/ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MinesweeperData {
    /// `field_params`: columns, rows; `bombs`.
    pub width: u8,
    pub height: u8,
    /// `matrix`, row by row: `height` rows of `width` cells.
    pub cells: Vec<MineCellUi>,
    /// `flags`: flags set.
    pub flags: u32,
    pub bombs: u32,
    /// `first_touch`: no game started (the field can be changed).
    pub first_touch: bool,
    /// `leaderboard`: this round's results, fastest first.
    pub leaderboard: Vec<MinesweeperResult>,
}

/// One cell of the matrix. A closed cell shows only its flag.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MineCellUi {
    pub open: bool,
    pub bomb: bool,
    pub flag: bool,
    /// Bombs around an open cell.
    pub around: u8,
    /// `final`: the bomb that went off.
    pub final_bomb: bool,
}

/// A leaderboard entry (`result_to_add`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MinesweeperResult {
    pub name: String,
    /// `time`, to a tenth of a second.
    pub seconds: f64,
    /// `points`: the field's 3BV.
    pub points: u32,
    /// `pointsPerSec`: 3BV/s, to a tenth.
    pub points_per_sec: f64,
    /// `fieldParams`: `<width>X<height>(<bombs>)`.
    pub field_params: String,
}

/// BandaStation `/datum/computer_file/program/navigator/ui_data`: the signal, where the user is
/// and the ladders and stairs (`mapData.stairs`) of the user's level.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NavigatorData {
    /// `signal`: 0 lost, 1 low, 2 good.
    pub signal: u8,
    /// `location`; `None` without a signal.
    pub location: Option<NavLocation>,
    /// The ladders and stairs on the user's level, nearest first (the map's stair markers).
    pub stairs: Vec<NavStair>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NavLocation {
    pub x: i32,
    pub y: i32,
    pub z: u16,
    /// `dir`, Russian (`"север"`); `None` on a low signal (the map's arrow is hidden).
    pub dir: Option<String>,
    /// `area.declent_ru(NOMINATIVE)`.
    pub area: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NavStair {
    pub x: i32,
    pub y: i32,
    /// Tiles from the user (Chebyshev).
    pub distance: u32,
    /// Which way it lies from the user, Russian; empty on the user's tile.
    pub toward: String,
    /// A ladder (`false`: stairs).
    pub ladder: bool,
}

/// `/datum/computer_file/program/ntnetdownload/ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DownloaderData {
    /// `disk_size`, `disk_used`: GQ.
    pub disk_size: u32,
    pub disk_used: u32,
    /// `downloading` with `downloadname`, `downloaddesc`, `downloadsize`, `downloadcompletion`.
    pub downloading: Option<DownloadProgress>,
    /// `error`.
    pub error: Option<String>,
    pub emagged: bool,
    /// `programs`: the catalog (categories are `programs::CATEGORIES`).
    pub programs: Vec<DownloadableProgram>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DownloadProgress {
    pub name: String,
    pub desc: String,
    pub size: u32,
    /// GQ done, to a tenth.
    pub completion: f32,
}

/// One catalog entry.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloadableProgram {
    pub icon: String,
    pub filename: String,
    pub filedesc: String,
    pub fileinfo: String,
    pub category: String,
    pub installed: bool,
    pub compatible: bool,
    pub size: u32,
    /// `access`: the computer's ID may download it.
    pub access: bool,
}

/// `/datum/computer_file/program/chatclient/ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatClientData {
    pub username: String,
    /// `adminmode`, `can_admin` (network access on the ID).
    pub adminmode: bool,
    pub can_admin: bool,
    pub all_channels: Vec<ChatRoom>,
    pub active_channel: Option<u32>,
    /// The active channel's `title`.
    pub title: String,
    /// `authed`: the channel's members and messages are shown.
    pub authed: bool,
    pub strong: bool,
    pub is_operator: bool,
    pub clients: Vec<ChatMember>,
    /// Newest first.
    pub messages: Vec<ChatMessageUi>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatRoom {
    pub chan: String,
    pub id: u32,
}

/// A channel member (`clients`); `is_self` replaces tg's `selfref` comparison.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMember {
    pub name: String,
    pub online: bool,
    pub away: bool,
    pub muted: bool,
    pub operator: bool,
    pub is_self: bool,
    pub member_ref: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMessageUi {
    pub key: u32,
    pub msg: String,
}

// `NtosNetChat` acts.
pub const PRG_SPEAK: &str = "PRG_speak";
pub const PRG_JOIN_CHANNEL: &str = "PRG_joinchannel";
pub const PRG_LEAVE_CHANNEL: &str = "PRG_leavechannel";
pub const PRG_NEW_CHANNEL: &str = "PRG_newchannel";
pub const PRG_TOGGLE_ADMIN: &str = "PRG_toggleadmin";
pub const PRG_CHANGE_NAME: &str = "PRG_changename";
pub const PRG_RENAME_CHANNEL: &str = "PRG_renamechannel";
pub const PRG_DELETE_CHANNEL: &str = "PRG_deletechannel";
pub const PRG_SET_PASSWORD: &str = "PRG_setpassword";
pub const PRG_MUTE_USER: &str = "PRG_mute_user";
pub const PRG_PING_USER: &str = "PRG_ping_user";

// `NtosNetDownloader` acts.
pub const PRG_DOWNLOAD_FILE: &str = "PRG_downloadfile";
pub const PRG_RESET_ERROR: &str = "PRG_reseterror";

// BandaStation `NtosMinesweeperPanel` acts.
pub const MINESWEEPER_SQUARE: &str = "Square";
pub const MINESWEEPER_CHANGE_SIZE: &str = "ChangeSize";

// `NtosCard` acts.
pub const PRG_AUTHENTICATE: &str = "PRG_authenticate";
pub const PRG_LOGOUT: &str = "PRG_logout";
pub const PRG_TERMINATE: &str = "PRG_terminate";
pub const PRG_EDIT: &str = "PRG_edit";
pub const PRG_AGE: &str = "PRG_age";
pub const PRG_ASSIGN: &str = "PRG_assign";
pub const PRG_ACCESS: &str = "PRG_access";
pub const PRG_TEMPLATE: &str = "PRG_template";
pub const PRG_INSERT_MAIN_ID: &str = "PRG_insert_main_id";
pub const PRG_REMOVE_MAIN_ID: &str = "PRG_remove_main_id";
pub const PRG_INSERT_ALT_ID: &str = "PRG_insert_alt_id";
pub const PRG_REMOVE_ALT_ID: &str = "PRG_remove_alt_id";

// tgui acts (`act('...')`).
pub const PC_EXIT: &str = "PC_exit";
pub const PC_SHUTDOWN: &str = "PC_shutdown";
pub const PC_MINIMIZE: &str = "PC_minimize";
pub const PC_KILLPROGRAM: &str = "PC_killprogram";
pub const PC_RUNPROGRAM: &str = "PC_runprogram";
pub const PC_TOGGLE_LIGHT: &str = "PC_toggle_light";
pub const PC_EJECT_DISK: &str = "PC_Eject_Disk";
pub const PC_IMPRINT_ID: &str = "PC_Imprint_ID";
pub const PDA_RING_SET: &str = "PDA_ringSet";
pub const PDA_TOGGLE_ALERTS: &str = "PDA_toggleAlerts";
pub const PDA_TOGGLE_SENDING: &str = "PDA_toggleSendingAndReceiving";
pub const PDA_VIEW_MESSAGES: &str = "PDA_viewMessages";
pub const PDA_CLOSE_MESSAGES: &str = "PDA_closeMessages";
pub const PDA_CLEAR_MESSAGES: &str = "PDA_clearMessages";
pub const PDA_CHANGE_SORT: &str = "PDA_changeSortStyle";
pub const PDA_SEND_MESSAGE: &str = "PDA_sendMessage";
pub const PDA_CLEAR_UNREADS: &str = "PDA_clearUnreads";
pub const UPDATE_NOTE: &str = "UpdateNote";
