//! Exploration drones, after tg `code/modules/explorer_drone/`: the scanner array and its
//! console (tgui `ExoscannerConsole`), exploration sites with their events and text adventures,
//! the drones with their launcher and fuel, and the drone control console (tgui
//! `ExodroneConsole`).
//!
//! [`adventure`] is the adventure engine, [`sites`] the sites, events and loot, [`server`] the
//! machines and the drone's state machine, [`client`] the windows. This module holds the
//! windows' data, which both halves share.

use serde::{Deserialize, Serialize};
use ss13_tgui::UiInterface;

#[cfg(feature = "server")]
pub mod adventure;
#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "server")]
pub mod server;
#[cfg(feature = "server")]
pub mod sites;

/// `EXOSCANNER_BAND_*`, in `GLOB.exoscanner_bands` order.
pub const BANDS: [&str; 5] = [
    "Plasma absorption band",
    "Hydrocarbons/Molecular oxygen",
    "Narrow-band radio waves",
    "Exotic Radiation",
    "Increased Density",
];

pub const EXODRONE: &str = "/obj/item/exodrone";
pub const LAUNCHER: &str = "/obj/machinery/exodrone_launcher";
pub const FUEL_PELLET: &str = "/obj/item/fuel_pellet";
pub const FUEL_ADVANCED: &str = "/obj/item/fuel_pellet/advanced";
pub const FUEL_EXOTIC: &str = "/obj/item/fuel_pellet/exotic";
pub const CONTROL_CONSOLE: &str = "/obj/machinery/computer/exodrone_control_console";
pub const SCANNER_CONSOLE: &str = "/obj/machinery/computer/exoscanner_control";
pub const SCANNER: &str = "/obj/machinery/exoscanner";

/// `EXODRONE_TOOL_*` with `GLOB.exodrone_tool_metadata`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Tool {
    Welder,
    Translator,
    Laser,
    Multitool,
    Drill,
}

impl Tool {
    pub const ALL: [Self; 5] = [
        Self::Welder,
        Self::Translator,
        Self::Laser,
        Self::Multitool,
        Self::Drill,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Welder => "welder",
            Self::Translator => "translator",
            Self::Laser => "laser",
            Self::Multitool => "multitool",
            Self::Drill => "drill",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.name() == name)
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Welder => "A heavy duty welder.",
            Self::Translator => "Powerful translation and data recording software.",
            Self::Laser => "Multipurpose tool suitable for combat and precision cutting.",
            Self::Multitool => {
                "Multipurpose tool for electronics manipulation. Comes with suite of radiation \
                 and radiowave sensors."
            }
            Self::Drill => "Heavy duty drill useful for mining.",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Self::Welder => "burn",
            Self::Translator => "language",
            Self::Laser => "bolt",
            Self::Multitool => "broadcast-tower",
            Self::Drill => "screwdriver",
        }
    }
}

/// tgui `ExodroneConsole`.
pub struct ExodroneConsoleUi;

impl UiInterface for ExodroneConsoleUi {
    const NAME: &'static str = "ExodroneConsole";
    type Data = ExodroneData;
    type StaticData = ();
}

/// A site as the windows show it (`site_data`); `id` is its `ref`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SiteInfo {
    pub id: u32,
    pub name: String,
    pub coordinates: String,
    pub description: String,
    pub distance: u32,
    pub revealed: bool,
    pub point_scan_complete: bool,
    pub deep_scan_complete: bool,
    /// Band -> value, empty until the point scan (`band_info`).
    pub band_info: Vec<(String, f64)>,
    /// Revisitable events, `(name, id)` (`events`, exploration only).
    pub events: Vec<(String, u32)>,
}

/// A drone storage slot (`get_cargo_data`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum CargoSlot {
    Tool(Tool),
    /// An item; `id` is its entity bits (`ref`).
    Cargo {
        name: String,
        id: u64,
    },
    Empty,
}

/// A drone in the console's listing (`all_drones`); `id` is its entity bits.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DroneListing {
    pub name: String,
    pub controlled: bool,
    pub description: String,
    pub id: u64,
}

/// A simple event's screen (`build_ui_event`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct EventScreen {
    pub description: String,
    pub action_enabled: bool,
    pub action_text: String,
    pub skippable: bool,
    pub ignore_text: String,
    pub id: u32,
}

/// The adventure's screen (`get_adventure_data`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AdventureScreen {
    pub description: String,
    /// `(key, text)`.
    pub choices: Vec<(String, String)>,
}

/// What the drone screen shows, by `drone_status`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum DroneScreen {
    Idle {
        sites: Vec<SiteInfo>,
    },
    /// Seconds.
    Travel {
        travel_time: f64,
        travel_time_left: f64,
    },
    Busy {
        wait_time_left: f64,
        wait_message: String,
    },
    Exploration {
        sites: Vec<SiteInfo>,
        site: Box<SiteInfo>,
        event: Option<EventScreen>,
    },
    Adventure(AdventureScreen),
}

impl DroneScreen {
    /// `drone_status` as tgui prints it (`capitalize`).
    pub fn status(&self) -> &'static str {
        match self {
            Self::Idle { .. } => "Idle",
            Self::Travel { .. } => "Travel",
            Self::Busy { .. } => "Busy",
            Self::Exploration { .. } => "Exploration",
            Self::Adventure(_) => "Adventure",
        }
    }
}

/// The controlled drone (`ui_data` with `drone`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ActiveDrone {
    pub name: String,
    pub integrity: f32,
    pub max_integrity: f32,
    /// Seconds of travel per unit of distance (`drone_travel_coefficent`).
    pub travel_coefficient: f64,
    /// Newest first.
    pub log: Vec<String>,
    pub configurable: bool,
    pub cargo: Vec<CargoSlot>,
    pub can_travel: bool,
    pub travel_error: String,
    pub screen: DroneScreen,
}

/// `ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ExodroneData {
    pub signal_lost: bool,
    pub drone: Option<ActiveDrone>,
    pub all_drones: Vec<DroneListing>,
}

pub const SELECT_DRONE: &str = "select_drone";
pub const DRONE_REF: &str = "drone_ref";
pub const END_CONTROL: &str = "end_control";
pub const CONFIRM_SIGNAL_LOST: &str = "confirm_signal_lost";
pub const SELF_DESTRUCT: &str = "self_destruct";
pub const ADD_TOOL: &str = "add_tool";
pub const REMOVE_TOOL: &str = "remove_tool";
pub const TOOL_TYPE: &str = "tool_type";
pub const START_TRAVEL: &str = "start_travel";
/// Absent: home.
pub const TARGET_SITE: &str = "target_site";
pub const EXPLORE: &str = "explore";
pub const EXPLORE_EVENT: &str = "explore_event";
pub const TARGET_EVENT: &str = "target_event";
pub const ADVENTURE_CHOICE: &str = "adventure_choice";
pub const CHOICE: &str = "choice";
pub const START_EVENT: &str = "start_event";
pub const SKIP_EVENT: &str = "skip_event";
pub const JETTISON: &str = "jettison";
pub const TARGET_REF: &str = "target_ref";

/// tgui `ExoscannerConsole`.
pub struct ExoscannerConsoleUi;

impl UiInterface for ExoscannerConsoleUi {
    const NAME: &'static str = "ExoscannerConsole";
    type Data = ExoscannerData;
    type StaticData = ();
}

/// The scan in progress (`scan_in_progress` with its fields).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ScanProgress {
    /// Seconds.
    pub time_left: f64,
    pub power: f64,
    pub description: String,
}

/// `ui_data`. ETAs are seconds.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ExoscannerData {
    pub failed: bool,
    pub scan_power: f64,
    pub selected_site: Option<SiteInfo>,
    pub point_scan_eta: f64,
    pub deep_scan_eta: f64,
    pub wide_scan_eta: f64,
    /// `(id, name)`.
    pub possible_sites: Vec<(u32, String)>,
    pub scan_conditions: Vec<String>,
    pub scan: Option<ScanProgress>,
}

pub const SELECT_SITE: &str = "select_site";
/// Absent: back to the site list.
pub const SITE_REF: &str = "site_ref";
pub const STOP_SCAN: &str = "stop_scan";
pub const START_WIDE_SCAN: &str = "start_wide_scan";
pub const START_POINT_SCAN: &str = "start_point_scan";
pub const START_DEEP_SCAN: &str = "start_deep_scan";
pub const CONFIRM_FAIL: &str = "confirm_fail";
