//! The safes' window data, shared by both halves: tgui `Safe.jsx` (mechanical safes) and
//! `LockedSafe.tsx` (`lockable_storage`). The server halves are `crate::safes` and
//! `crate::secure_safes`, the client views `crate::safe_client`.

use serde::{Deserialize, Serialize};
use ss13_tgui::UiInterface;

/// tgui `Safe`.
pub struct SafeUi;

impl UiInterface for SafeUi {
    const NAME: &'static str = "Safe";
    type Data = SafeData;
    type StaticData = ();
}

/// A `contents` entry.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SafeItem {
    pub name: String,
    pub sprite: String,
}

/// `ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SafeData {
    pub dial: u8,
    pub open: bool,
    pub locked: bool,
    pub broken: bool,
    /// Only while open.
    pub contents: Vec<SafeItem>,
}

pub const OPEN: &str = "open";
pub const TURN_RIGHT: &str = "turnright";
pub const TURN_LEFT: &str = "turnleft";
pub const RETRIEVE: &str = "retrieve";
/// The dial turns the window offers (and the only ones `ui_act` accepts).
pub const TICKS: [u32; 3] = [1, 10, 50];

/// tgui `LockedSafe` (`/datum/component/lockable_storage`).
pub struct LockedSafeUi;

impl UiInterface for LockedSafeUi {
    const NAME: &'static str = "LockedSafe";
    type Data = LockedSafeData;
    type StaticData = ();
}

/// `ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockedSafeData {
    /// The digits typed so far, `*****` when none.
    pub input_code: String,
    pub locked: bool,
    /// Whether a code is set.
    pub lock_code: bool,
}

pub const KEYPAD: &str = "keypad";
/// The keypad's keys: digits, `C` (clear and lock), `E` (enter).
pub const KEYS: [&str; 12] = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "C", "0", "E"];
