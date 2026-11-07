//! The mechanical safe's window data, shared by both halves: tgui `Safe.jsx`. The server half is
//! `crate::safes`, the client view `crate::safe_client`.

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
