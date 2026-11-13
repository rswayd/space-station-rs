//! Nuclear operatives after tg `/datum/antagonist/nukeop`, `/datum/team/nuclear`,
//! `/datum/dynamic_ruleset/roundstart/nukies`, `/obj/machinery/nuclearbomb` and
//! `/obj/item/disk/nuclear`. [`server`] is the antagonist, the bomb ([`bomb`]) and the
//! infiltrator; [`client`] draws the windows; this module holds what both share.
//!
//! - `NuclearBomb`: the bomb's keypad (`NuclearBomb.tsx`).
//! - `ShuttleConsole`: tg's shuttle console (`ShuttleConsole.jsx`), shared with every shuttle
//!   console (`crate::shuttle_console`, server half `crate::shuttle_nav`); the infiltrator's is
//!   `/obj/machinery/computer/shuttle/syndicate`.

use serde::{Deserialize, Serialize};
use ss13_tgui::UiInterface;

#[cfg(feature = "server")]
pub mod battlecruiser;
#[cfg(feature = "server")]
pub mod bomb;
#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "server")]
pub mod core_theft;
#[cfg(feature = "server")]
pub mod lone;
#[cfg(feature = "server")]
pub mod reinforcements;
#[cfg(feature = "server")]
pub mod server;
#[cfg(feature = "server")]
pub mod war;

/// tg `ROLE_OPERATIVE`: the nuclear operative's `be_special` key.
pub const ROLE_OPERATIVE: &str = "Operative";
/// tg `ROLE_LONE_OPERATIVE`.
pub const ROLE_LONE_OPERATIVE: &str = "Lone Operative";
/// tg `ROLE_CLOWN_OPERATIVE`.
pub const ROLE_CLOWN_OPERATIVE: &str = "Clown Operative";
/// tg `ROLE_CLOWN_OPERATIVE_MIDROUND`.
pub const ROLE_CLOWN_OPERATIVE_MIDROUND: &str = "Clown Operative (Midround)";
/// tg `ROLE_BATTLECRUISER_CREW`.
pub const ROLE_BATTLECRUISER_CREW: &str = "Battlecruiser Crew";

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

/// The shuttle console window, shared with every `/obj/machinery/computer/shuttle`
/// (`crate::shuttle_console`).
pub use crate::shuttle_console::{
    DESTINATION, MOVE, SET_DESTINATION, SHUTTLE_ID, ShuttleConsoleData, ShuttleConsoleUi,
};
