//! The operating computer's window data, shared by both halves: tgui `OperatingComputer`
//! (`tgui/packages/tgui/interfaces/OperatingComputer`). The server half is
//! `crate::medical::operating`, the client view `crate::operating_client`.

use serde::{Deserialize, Serialize};
use ss13_tgui::UiInterface;

pub struct OperatingComputerUi;

impl UiInterface for OperatingComputerUi {
    const NAME: &'static str = "OperatingComputer";
    type Data = OperatingComputerData;
    type StaticData = ();
}

/// `/obj/machinery/computer/operating/ui_data` with its `ui_static_data` (`surgeries`; no
/// techwebs or experiments).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct OperatingComputerData {
    pub has_table: bool,
    /// tg body zone name (`chest`, `l_arm`, `eyes`, ...).
    pub target_zone: String,
    pub patient: Option<PatientData>,
    pub surgeries: Vec<OperationData>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PatientData {
    pub stat: String,
    /// `good`, `average` or `bad`.
    pub statstate: String,
    pub health: f32,
    pub blood_type: String,
    pub max_health: f32,
    pub min_health: f32,
    pub brute_loss: f32,
    pub fire_loss: f32,
    pub tox_loss: f32,
    pub oxy_loss: f32,
    pub blood_level: f32,
    pub standard_blood_level: f32,
    pub surgery_state: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct OperationData {
    pub name: String,
    pub desc: String,
    pub tool_rec: String,
    /// All of, one of, optional, blocking (`get_requirements()`).
    pub requirements: Vec<Vec<String>>,
    /// Recommended as a next step for the patient on the table.
    pub show_as_next: bool,
    pub show_in_list: bool,
    /// `OPERATION_PRIORITY_NEXT_STEP`.
    pub priority: bool,
}

/// `change_zone` with `new_zone`.
pub const CHANGE_ZONE: &str = "change_zone";
pub const NEW_ZONE: &str = "new_zone";
