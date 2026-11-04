//! The communications console window's data, shared by both halves: a minimal port of tgui
//! `CommunicationsConsole` (`tgui/packages/tgui/interfaces/CommunicationsConsole/`) with the
//! authentication section and the emergency shuttle and alert level sections of the main page. The server half
//! is `crate::shuttle`, the client view `crate::comms_client`.

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
}

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
