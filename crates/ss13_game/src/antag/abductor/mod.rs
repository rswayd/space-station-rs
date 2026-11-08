//! Abductors after tg `code/modules/antagonists/abductor`: the alien team (agent and scientist)
//! that kidnaps the crew, swaps their hearts for glands and sends them back. [`server`] is the
//! antagonist, its ship, machines, glands and gear; [`client`] draws the windows; this module
//! holds what both share.
//!
//! - `AbductorConsole` (`AbductorConsole.tsx`): the gear shop (`act('buy', { name })`, the
//!   category tabs `act('select', { category })`), the emergency teleport (`teleporter_send`,
//!   `teleporter_retrieve`) and the agent vest (`flip_vest`, `toggle_vest`, `select_disguise`).
//! - `ProbingConsole` (`ProbingConsole.jsx`): the experimentation machine (`door`,
//!   `act('experiment', { experiment_type })`).
//! - `GlandDispenser` (`GlandDispenser.tsx`): the replacement organ storage
//!   (`act('dispense', { gland_id })`).

use serde::{Deserialize, Serialize};
use ss13_tgui::UiInterface;

#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "server")]
pub mod server;

/// tg `ROLE_ABDUCTOR`: the `be_special` key.
pub const ROLE_ABDUCTOR: &str = "Abductor";

/// The abductor console.
pub struct AbductorConsoleUi;

impl UiInterface for AbductorConsoleUi {
    const NAME: &'static str = "AbductorConsole";
    type Data = AbductorConsoleData;
    type StaticData = AbductorConsoleStatic;
}

/// One `/datum/abductor_gear` of a category.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AbductorGearItem {
    pub name: String,
    pub cost: u32,
    pub desc: String,
}

/// A category and its gear.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AbductorGearCategory {
    pub name: String,
    pub items: Vec<AbductorGearItem>,
}

/// `ui_static_data`: `categories`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AbductorConsoleStatic {
    pub categories: Vec<AbductorGearCategory>,
}

/// `ui_data`. `None` fields are tg's missing keys (no linked machine).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AbductorConsoleData {
    pub compact_mode: bool,
    /// The selected gear category (tg keeps the tab in `selected_cat`).
    pub selected: Option<String>,
    pub experiment: bool,
    pub points: Option<u32>,
    pub credits: Option<u32>,
    pub pad: bool,
    /// A linked science tool has a marked specimen.
    pub gizmo: bool,
    pub vest: bool,
    /// `VEST_STEALTH` (1) or `VEST_COMBAT` (2).
    pub vest_mode: Option<u8>,
    pub vest_lock: Option<bool>,
}

impl AbductorConsoleData {
    /// `Abductsoft`'s buy button: enough credits.
    pub fn can_buy(&self, item: &AbductorGearItem) -> bool {
        self.credits.unwrap_or(0) >= item.cost
    }
}

/// `act('buy', { name })`.
pub const BUY: &str = "buy";
pub const NAME: &str = "name";
pub const TELEPORTER_SEND: &str = "teleporter_send";
pub const TELEPORTER_RETRIEVE: &str = "teleporter_retrieve";
pub const FLIP_VEST: &str = "flip_vest";
pub const TOGGLE_VEST: &str = "toggle_vest";
pub const SELECT_DISGUISE: &str = "select_disguise";
/// `act('select', { category })`.
pub const SELECT: &str = "select";
pub const CATEGORY: &str = "category";
pub const COMPACT_TOGGLE: &str = "compact_toggle";

/// `VEST_STEALTH`, `VEST_COMBAT`.
pub const VEST_STEALTH: u8 = 1;
pub const VEST_COMBAT: u8 = 2;

/// The experimentation machine's console.
pub struct ProbingConsoleUi;

impl UiInterface for ProbingConsoleUi {
    const NAME: &'static str = "ProbingConsole";
    type Data = ProbingConsoleData;
    type StaticData = ();
}

/// `/obj/machinery/abductor/experiment/ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProbingConsoleData {
    pub open: bool,
    pub feedback: String,
    pub occupant: bool,
    pub occupant_name: Option<String>,
    /// tg `stat`: 0 conscious, 1 soft crit, 2 unconscious, 3 dead.
    pub occupant_status: Option<u8>,
}

/// `act('door')`, `act('experiment', { experiment_type })` (1 probe, 2 dissect, 3 analyze).
pub const DOOR: &str = "door";
pub const EXPERIMENT: &str = "experiment";
pub const EXPERIMENT_TYPE: &str = "experiment_type";

/// The replacement organ storage.
pub struct GlandDispenserUi;

impl UiInterface for GlandDispenserUi {
    const NAME: &'static str = "GlandDispenser";
    type Data = GlandDispenserData;
    type StaticData = ();
}

/// One slot of `ui_data` `glands`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlandSlot {
    /// 1-based, the `gland_id` of `dispense`.
    pub id: u32,
    /// `#rrggbb`.
    pub color: String,
    pub amount: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlandDispenserData {
    pub glands: Vec<GlandSlot>,
}

/// `act('dispense', { gland_id })`.
pub const DISPENSE: &str = "dispense";
pub const GLAND_ID: &str = "gland_id";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buying_needs_credits() {
        let item = AbductorGearItem {
            cost: 2,
            ..Default::default()
        };
        let mut data = AbductorConsoleData {
            credits: Some(1),
            ..Default::default()
        };
        assert!(!data.can_buy(&item));
        data.credits = Some(2);
        assert!(data.can_buy(&item));
        data.credits = None;
        assert!(!data.can_buy(&item));
    }
}
