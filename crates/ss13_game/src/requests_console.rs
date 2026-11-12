//! The requests console window's data, shared by both halves: tgui `RequestsConsole`
//! (`tgui/packages/tgui/interfaces/RequestsConsole/`): the header (send error, unread
//! messages, the emergency buttons), the message list, the write and announcement tabs and the
//! speaker toggle. The server half is `crate::wallmount::requests`, the client view
//! `crate::requests_console_client`.

use serde::{Deserialize, Serialize};
use ss13_tgui::UiInterface;

pub struct RequestsConsoleUi;

impl UiInterface for RequestsConsoleUi {
    const NAME: &'static str = "RequestsConsole";
    type Data = RequestsData;
    type StaticData = RequestsStatic;
}

/// `REQ_*_MESSAGE_PRIORITY`.
pub const PRIORITY_NONE: u8 = 0;
pub const PRIORITY_NORMAL: u8 = 1;
pub const PRIORITY_HIGH: u8 = 2;
pub const PRIORITY_EXTREME: u8 = 3;

/// `ASSISTANCE_REQUEST`, `SUPPLIES_REQUEST`, `INFORMATION_REQUEST`, `ORE_UPDATE_REQUEST`,
/// `REPLY_REQUEST`.
pub const ASSISTANCE_REQUEST: &str = "Assistance Request";
pub const SUPPLIES_REQUEST: &str = "Supplies Request";
pub const INFORMATION_REQUEST: &str = "Relay Information";
pub const ORE_UPDATE_REQUEST: &str = "Ore Update";
pub const REPLY_REQUEST: &str = "Reply";

/// `REQ_EMERGENCY_*`.
pub const EMERGENCY_SECURITY: &str = "Security";
pub const EMERGENCY_ENGINEERING: &str = "Engineering";
pub const EMERGENCY_MEDICAL: &str = "Medical";

/// Actions.
pub const CLEAR_MESSAGE_STATUS: &str = "clear_message_status";
pub const CLEAR_AUTHENTICATION: &str = "clear_authentication";
pub const TOGGLE_SILENT: &str = "toggle_silent";
pub const SET_EMERGENCY: &str = "set_emergency";
pub const SEND_ANNOUNCEMENT: &str = "send_announcement";
pub const QUICK_REPLY: &str = "quick_reply";
pub const SEND_MESSAGE: &str = "send_message";
/// Params.
pub const EMERGENCY: &str = "emergency";
pub const MESSAGE: &str = "message";
pub const RECIPIENT: &str = "recipient";
pub const PRIORITY: &str = "priority";
pub const REQUEST_TYPE: &str = "request_type";
pub const REPLY_RECIPIENT: &str = "reply_recipient";

/// `authentication_data`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthenticationData {
    pub message_verified_by: String,
    pub message_stamped_by: String,
    pub announcement_authenticated: bool,
}

/// `/datum/request_message/message_ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestMessage {
    pub sender_department: String,
    pub received_time: String,
    pub content: String,
    pub message_verified_by: String,
    pub message_stamped_by: String,
    pub priority: u8,
    pub request_type: String,
}

/// `ui_data` of `/obj/machinery/requests_console`, without the RETA fields (no RETA).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestsData {
    pub can_send_announcements: bool,
    pub department: String,
    /// The emergency called from here (`REQ_EMERGENCY_*`), empty when none.
    pub emergency: String,
    pub hack_state: bool,
    pub new_message_priority: u8,
    pub silent: bool,
    pub has_mail_send_error: bool,
    pub authentication_data: AuthenticationData,
    /// Newest first.
    pub messages: Vec<RequestMessage>,
}

/// `ui_static_data`: the departments each request type can go to, this one left out.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestsStatic {
    pub assistance_consoles: Vec<String>,
    pub supply_consoles: Vec<String>,
    pub information_consoles: Vec<String>,
}
