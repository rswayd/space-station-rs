//! The communications console window's data, shared by both halves: tgui
//! `CommunicationsConsole` (`tgui/packages/tgui/interfaces/CommunicationsConsole/`) with the
//! authentication section, the main page (emergency shuttle, alert level, functions), the
//! message list and the status display page. The server half is `crate::comms_server`, the
//! client view `crate::comms_client`.

use serde::{Deserialize, Serialize};
use ss13_tgui::UiInterface;

pub struct CommsConsoleUi;

impl UiInterface for CommsConsoleUi {
    const NAME: &'static str = "CommunicationsConsole";
    type Data = CommsConsoleData;
    type StaticData = ();
}

/// `ui_data` of `/obj/machinery/computer/communications`, the fields the ported sections read.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommsConsoleData {
    pub authenticated: bool,
    pub can_log_out: bool,
    /// tg `authorize_name`: `"<registered name> - <assignment>"`.
    pub authorize_name: Option<String>,
    /// The emergency shuttle is on its way, docked or gone (not idle, not recalling).
    pub shuttle_called: bool,
    /// tg `SSshuttle.can_recall(user)`.
    pub shuttle_recallable: bool,
    pub can_recall_shuttles: bool,
    /// tg `shuttleCanEvacOrFailReason`: `None` when the shuttle can be called (`TRUE`).
    pub shuttle_can_evac_or_fail_reason: Option<String>,
    pub shuttle_called_previously: bool,
    /// The area the last call or recall was traced to.
    pub shuttle_last_called: Option<String>,
    /// tg `CALL_SHUTTLE_REASON_LENGTH`.
    pub call_shuttle_reason_min_length: u32,
    /// tg `maxMessageLength`, capped by the act param limit.
    pub max_message_length: u32,
    /// tg `alertLevel`: the security level's name.
    pub alert_level: String,
    /// tg `canSetAlertLevel`: `None` (`FALSE`), [`SWIPE_NEEDED`] or [`NO_SWIPE_NEEDED`].
    pub can_set_alert_level: Option<String>,
    /// Emagged or a Syndicate console (the window's `syndicate` theme).
    pub emagged: bool,
    pub syndicate: bool,
    /// One of [`PAGES`].
    pub page: String,
    pub can_make_announcement: bool,
    pub can_message_associates: bool,
    pub can_request_nuke: bool,
    /// BandaStation `canRequestERT`.
    pub can_request_ert: bool,
    pub can_toggle_emergency_access: bool,
    pub emergency_access: bool,
    pub important_action_ready: bool,
    pub messages: Vec<CommsMessage>,
    /// `STATE_CHANGING_STATUS`: the last lines sent to the status displays.
    pub upper_text: String,
    pub lower_text: String,
    /// tg `MAX_STATUS_LINE_LENGTH`.
    pub max_status_line_length: u32,
}

/// A `/datum/comm_message` in `messages`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommsMessage {
    pub title: String,
    pub content: String,
    pub possible_answers: Vec<String>,
    /// The 1-based answer; 0 while unanswered.
    pub answered: u32,
}

/// `STATE_MAIN`, `STATE_MESSAGES`, `STATE_CHANGING_STATUS`, `STATE_BUYING_SHUTTLE`.
pub const PAGE_MAIN: &str = "main";
pub const PAGE_MESSAGES: &str = "messages";
pub const PAGE_CHANGING_STATUS: &str = "changing_status";
pub const PAGE_BUYING_SHUTTLE: &str = "buying_shuttle";
pub const PAGES: [&str; 4] = [
    PAGE_MAIN,
    PAGE_MESSAGES,
    PAGE_CHANGING_STATUS,
    PAGE_BUYING_SHUTTLE,
];
/// `GLOB.status_display_approved_pictures`.
pub const STATUS_PICTURES: [&str; 11] = [
    "blank",
    "shuttle",
    "default",
    "biohazard",
    "lockdown",
    "greenalert",
    "bluealert",
    "redalert",
    "deltaalert",
    "radiation",
    "currentalert",
];

/// `CommunicationsConsole/constants.ts`.
pub const SWIPE_NEEDED: &str = "SWIPE_NEEDED";
pub const NO_SWIPE_NEEDED: &str = "NO_SWIPE_NEEDED";

pub const TOGGLE_AUTHENTICATION: &str = "toggleAuthentication";
/// `act('callShuttle', { reason })`.
pub const CALL_SHUTTLE: &str = "callShuttle";
pub const RECALL_SHUTTLE: &str = "recallShuttle";
pub const REASON: &str = "reason";
/// `act('changeSecurityLevel', { newSecurityLevel })`.
pub const CHANGE_SECURITY_LEVEL: &str = "changeSecurityLevel";
pub const NEW_SECURITY_LEVEL: &str = "newSecurityLevel";
pub const MAKE_PRIORITY_ANNOUNCEMENT: &str = "makePriorityAnnouncement";
/// `act('messageAssociates', { message })`.
pub const MESSAGE_ASSOCIATES: &str = "messageAssociates";
pub const MESSAGE: &str = "message";
/// `act('requestNukeCodes', { reason })`, BandaStation `act('requestERT', { reason })`.
pub const REQUEST_NUKE_CODES: &str = "requestNukeCodes";
pub const REQUEST_ERT: &str = "requestERT";
pub const RESTORE_BACKUP_ROUTING_DATA: &str = "restoreBackupRoutingData";
pub const TOGGLE_EMERGENCY_ACCESS: &str = "toggleEmergencyAccess";
/// `act('setState', { state })`.
pub const SET_STATE: &str = "setState";
pub const STATE: &str = "state";
/// `act('setStatusMessage', { upperText, lowerText })`.
pub const SET_STATUS_MESSAGE: &str = "setStatusMessage";
pub const UPPER_TEXT: &str = "upperText";
pub const LOWER_TEXT: &str = "lowerText";
/// `act('setStatusPicture', { picture })`.
pub const SET_STATUS_PICTURE: &str = "setStatusPicture";
pub const PICTURE: &str = "picture";
/// `act('answerMessage', { message, answer })` (1-based), `act('deleteMessage', { message })`.
pub const ANSWER_MESSAGE: &str = "answerMessage";
pub const ANSWER: &str = "answer";
pub const DELETE_MESSAGE: &str = "deleteMessage";
