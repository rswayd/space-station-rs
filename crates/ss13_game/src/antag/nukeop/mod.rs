//! Nuclear operatives after tg `/datum/antagonist/nukeop`, `/datum/team/nuclear`,
//! `/datum/dynamic_ruleset/roundstart/nukies`, `/obj/machinery/nuclearbomb` and
//! `/obj/item/disk/nuclear`. [`server`] is the antagonist, the bomb ([`bomb`]) and the
//! infiltrator; [`client`] draws the windows; this module holds what both share.
//!
//! - `NuclearBomb`: the bomb's keypad (`NuclearBomb.tsx`).
//! - `ShuttleConsole`: tg's shuttle console (`ShuttleConsole.jsx`), here on the infiltrator's
//!   `/obj/machinery/computer/shuttle/syndicate`.

use serde::{Deserialize, Serialize};
use ss13_tgui::UiInterface;

#[cfg(feature = "server")]
pub mod bomb;
#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "server")]
pub mod server;

/// tg `ROLE_OPERATIVE`: the nuclear operative's `be_special` key.
pub const ROLE_OPERATIVE: &str = "Operative";

/// The nuclear bomb's keypad.
pub struct NuclearBombUi;

impl UiInterface for NuclearBombUi {
    const NAME: &'static str = "NuclearBomb";
    type Data = NuclearBombData;
    type StaticData = ();
}

/// `/obj/machinery/nuclearbomb/ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NuclearBombData {
    pub disk_present: bool,
    pub status1: String,
    pub status2: String,
    pub anchored: bool,
}

/// `act('eject_disk')`.
pub const EJECT_DISK: &str = "eject_disk";
/// `act('keypad', { digit })`: `0`-`9`, `C`, `E`.
pub const KEYPAD: &str = "keypad";
pub const DIGIT: &str = "digit";
/// `act('arm')`.
pub const ARM: &str = "arm";
/// `act('anchor')`.
pub const ANCHOR: &str = "anchor";

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
