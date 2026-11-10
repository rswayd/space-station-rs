//! The transport windows' data, shared by both halves: tgui `TramControl.tsx` of
//! `/obj/machinery/computer/tram_controls` (`code/modules/transport/tram/tram_controls.dm`,
//! server half `crate::transport::tram`) and `ElevatorPanel.tsx` of
//! `/obj/machinery/elevator_control_panel` (`code/modules/transport/elevator/elev_panel.dm`,
//! server half `crate::transport::elevator`). Both views are in `crate::transport_ui_client`.

use serde::{Deserialize, Serialize};
use ss13_tgui::UiInterface;

/// The tram controls.
pub struct TramControlUi;

impl UiInterface for TramControlUi {
    const NAME: &'static str = "TramControl";
    type Data = TramControlData;
    type StaticData = TramControlStatic;
}

/// `/obj/machinery/computer/tram_controls/ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TramControlData {
    /// The tram is busy (`controller_active`).
    pub moving: bool,
    /// No tram, or no controller cabinet on it.
    pub broken: bool,
    /// The platform it stands at.
    pub tram_location: Option<String>,
}

/// `ui_static_data`: `destinations`, `(name, platform code)` each.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TramControlStatic {
    pub destinations: Vec<(String, String)>,
}

/// `act('send', { destination })`, the platform code.
pub const SEND: &str = "send";
pub const DESTINATION: &str = "destination";

/// The elevator panel.
pub struct ElevatorPanelUi;

impl UiInterface for ElevatorPanelUi {
    const NAME: &'static str = "ElevatorPanel";
    type Data = ElevatorPanelData;
    type StaticData = ElevatorPanelStatic;
}

/// `/obj/machinery/elevator_control_panel/ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ElevatorPanelData {
    pub emergency_level: String,
    /// Red alert or above: the emergency door button shows.
    pub is_emergency: bool,
    pub doors_open: bool,
    pub lift_exists: bool,
    pub currently_moving: bool,
    /// `last_move_target`.
    pub currently_moving_to_floor: Option<u16>,
    pub current_floor: u16,
}

/// `ui_static_data`: `all_floor_data`, `(name, z)` top floor first.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ElevatorPanelStatic {
    pub all_floor_data: Vec<(String, u16)>,
}

/// `act('move_lift', { z })`, `act('emergency_door')`, `act('reset_doors')`.
pub const MOVE_LIFT: &str = "move_lift";
pub const Z: &str = "z";
pub const EMERGENCY_DOOR: &str = "emergency_door";
pub const RESET_DOORS: &str = "reset_doors";
