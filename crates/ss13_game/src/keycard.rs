//! The keycard authentication device's window data, shared by both halves: tgui `KeycardAuth`
//! (`tgui/packages/tgui/interfaces/KeycardAuth.jsx`). The server half is
//! `crate::security_level::keycard`, the client view `crate::keycard_client`.

use serde::{Deserialize, Serialize};
use ss13_tgui::UiInterface;

pub struct KeycardAuthUi;

impl UiInterface for KeycardAuthUi {
    const NAME: &'static str = "KeycardAuth";
    type Data = KeycardAuthData;
    type StaticData = ();
}

/// `/obj/machinery/keycard_auth/ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeycardAuthData {
    /// This device sent an event and waits for another to confirm it.
    pub waiting: bool,
    /// `auth_required`: the event another device asks this one to confirm (`0`: none).
    pub auth_required: Option<String>,
    /// The station is at red alert or above.
    pub red_alert: bool,
    /// `GLOB.emergency_access`.
    pub emergency_maint: bool,
    /// `GLOB.bsa_unlock`.
    pub bsa_unlock: bool,
}

pub const RED_ALERT: &str = "red_alert";
pub const EMERGENCY_MAINT: &str = "emergency_maint";
pub const BSA_UNLOCK: &str = "bsa_unlock";
pub const AUTH_SWIPE: &str = "auth_swipe";
