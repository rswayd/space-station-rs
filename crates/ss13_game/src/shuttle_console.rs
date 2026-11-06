//! The shuttle console windows' data, shared by both halves: tgui `EmergencyShuttleConsole.tsx`
//! of `/obj/machinery/computer/emergency_shuttle`
//! (`code/modules/shuttle/mobile_port/variants/emergency/emergency_console.dm`), server half
//! `crate::shuttle_console_server`; and `ShuttleConsole.jsx` of every
//! `/obj/machinery/computer/shuttle` (`code/modules/shuttle/shuttle_consoles/shuttle_console.dm`),
//! server half `crate::shuttle_nav`. Both views are in `crate::shuttle_console_client`.

use serde::{Deserialize, Serialize};
use ss13_tgui::UiInterface;

pub struct EmergencyConsoleUi;

impl UiInterface for EmergencyConsoleUi {
    const NAME: &'static str = "EmergencyShuttleConsole";
    type Data = EmergencyConsoleData;
    type StaticData = ();
}

/// `/obj/machinery/computer/emergency_shuttle/ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmergencyConsoleData {
    /// tg `getTimerStr`.
    pub timer_str: String,
    pub engines_started: bool,
    pub authorizations_remaining: u32,
    /// `(name, job)` of each authorizing ID (gibberish when emagged).
    pub authorizations: Vec<(String, String)>,
    /// Docked, the engines idle, and this user has not acted in the last 5 s.
    pub enabled: bool,
    pub emagged: bool,
}

/// `act('authorize')`, `act('repeal')`, `act('abort')` (the window's "Repeal All").
pub const AUTHORIZE: &str = "authorize";
pub const REPEAL: &str = "repeal";
pub const ABORT: &str = "abort";

/// A shuttle console.
pub struct ShuttleConsoleUi;

impl UiInterface for ShuttleConsoleUi {
    const NAME: &'static str = "ShuttleConsole";
    type Data = ShuttleConsoleData;
    type StaticData = ();
}

/// `/obj/machinery/computer/shuttle/ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShuttleConsoleData {
    pub docked_location: String,
    /// `(id, name)` of the valid destinations.
    pub locations: Vec<(String, String)>,
    pub locked: bool,
    pub authorization_required: bool,
    pub timer_str: String,
    pub destination: Option<String>,
    pub status: String,
}

/// `act('move', { shuttle_id })`.
pub const MOVE: &str = "move";
pub const SHUTTLE_ID: &str = "shuttle_id";
/// `act('set_destination', { destination })`.
pub const SET_DESTINATION: &str = "set_destination";
pub const DESTINATION: &str = "destination";
