//! The emergency shuttle console window's data, shared by both halves: tgui
//! `EmergencyShuttleConsole.tsx` of `/obj/machinery/computer/emergency_shuttle`
//! (`code/modules/shuttle/mobile_port/variants/emergency/emergency_console.dm`). The server half
//! is `crate::shuttle_console_server`, the client view `crate::shuttle_console_client`.

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
