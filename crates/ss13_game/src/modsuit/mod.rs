//! MODsuits, after tg `code/modules/mod/`: the control unit (`/obj/item/mod/control`) worn on
//! the back deploys and retracts its parts (helmet, chestplate, gauntlets, boots), seals them one
//! by one when activated (protection, pressure sealing, the theme's slowdown), drains its core's
//! cell every second while on, and carries modules up to its theme's complexity. [`server`] runs
//! it (with the common modules in [`modules`]), [`client`] draws tgui `MODsuit`; the window data
//! lives here, shared by both halves.
//!
//! Themes, control unit types and module types come from `ss13-mapload convert`
//! (`<assets>/modsuits.ron`, [`ss13_mapload::modsuit`]); rerun it after updating.

#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "server")]
pub mod modules;
#[cfg(feature = "server")]
pub mod server;

use serde::{Deserialize, Serialize};
use ss13_tgui::UiInterface;

/// tgui `MODsuit` (`tgui/packages/tgui/interfaces/MODsuit.tsx`).
pub struct ModSuitUi;

impl UiInterface for ModSuitUi {
    const NAME: &'static str = "MODsuit";
    type Data = ModSuitData;
    type StaticData = ModSuitStatic;
}

/// `ui_static_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct ModSuitStatic {
    pub ui_theme: String,
    pub complexity_max: i32,
}

/// `ui_data["suit_status"]`.
// shortcut: no AI/pAI, MODlink, wires or electrification fields (not ported).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct SuitStatus {
    pub core_name: Option<String>,
    /// Joules.
    pub charge_current: f64,
    pub charge_max: f64,
    /// `good`, `average`, `bad`, `teal` or `transparent`.
    pub chargebar_color: String,
    pub chargebar_string: String,
    pub active: bool,
    /// Not tg: the activation sequence is running (tg shows it through the do_after bar).
    pub activating: bool,
    pub open: bool,
    pub malfunctioning: bool,
    pub locked: bool,
    pub complexity: i32,
}

/// `ui_data["user_status"]`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct UserStatus {
    pub user_name: String,
    pub user_assignment: String,
}

/// One `get_configuration` entry (`add_ui_configuration`).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct ModuleConfig {
    pub key: String,
    pub display_name: String,
    /// `number`, `list` or `bool`.
    pub kind: String,
    pub value: String,
    pub values: Vec<String>,
}

/// A `module_info` entry.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct ModuleInfo {
    pub module_name: String,
    pub description: String,
    /// `MODULE_PASSIVE` 0, `MODULE_USABLE` 1, `MODULE_TOGGLE` 2, `MODULE_ACTIVE` 3.
    pub module_type: u8,
    pub module_active: bool,
    pub pinned: bool,
    /// Watts while idle / active, joules per use.
    pub idle_power: f64,
    pub active_power: f64,
    pub use_energy: f64,
    pub module_complexity: i32,
    /// Deciseconds.
    pub cooldown_time: f32,
    /// Deciseconds left.
    pub cooldown: f32,
    pub id: String,
    /// The module entity's bits (tg `REF(module)`).
    pub reference: u64,
    pub configuration_data: Vec<ModuleConfig>,
}

/// A `parts` entry.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct PartInfo {
    pub slot: String,
    pub name: String,
    pub deployed: bool,
    pub reference: u64,
}

/// `ui_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct ModSuitData {
    pub suit_status: SuitStatus,
    pub user_status: UserStatus,
    pub control: String,
    pub module_info: Vec<ModuleInfo>,
    pub parts: Vec<PartInfo>,
}

pub const LOCK: &str = "lock";
pub const ACTIVATE: &str = "activate";
pub const SELECT: &str = "select";
pub const CONFIGURE: &str = "configure";
pub const PIN: &str = "pin";
pub const DEPLOY: &str = "deploy";
pub const REF: &str = "ref";
pub const KEY: &str = "key";
pub const VALUE: &str = "value";

/// `MODULE_PASSIVE`, `MODULE_USABLE`, `MODULE_TOGGLE`, `MODULE_ACTIVE`.
pub const MODULE_PASSIVE: u8 = 0;
pub const MODULE_USABLE: u8 = 1;
pub const MODULE_TOGGLE: u8 = 2;
pub const MODULE_ACTIVE: u8 = 3;

/// `display_energy`: joules with an SI prefix, as the charge bar shows them.
pub fn display_energy(joules: f64) -> String {
    let (value, unit) = if joules >= 1e9 {
        (joules / 1e9, "GJ")
    } else if joules >= 1e6 {
        (joules / 1e6, "MJ")
    } else if joules >= 1e3 {
        (joules / 1e3, "kJ")
    } else {
        (joules, "J")
    };
    let text = format!("{value:.2}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    format!("{text} {unit}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn energy_reads_with_si_prefixes() {
        assert_eq!(display_energy(50_000.0), "50 kJ");
        assert_eq!(display_energy(1_250_000.0), "1.25 MJ");
        assert_eq!(display_energy(0.0), "0 J");
    }
}
