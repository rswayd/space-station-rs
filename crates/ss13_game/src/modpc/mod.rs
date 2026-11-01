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
