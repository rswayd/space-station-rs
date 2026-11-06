//! The flatpacker window's data, shared by both halves: tgui `Flatpacker`
//! (`tgui/packages/tgui/interfaces/Flatpacker.tsx`, `/obj/machinery/flatpacker/ui_data`). The
//! server half is `crate::eng_machines::flatpacker`, the client view `crate::flatpacker_client`.

use serde::{Deserialize, Serialize};
use ss13_tgui::UiInterface;

pub struct FlatpackerUi;

impl UiInterface for FlatpackerUi {
    const NAME: &'static str = "Flatpacker";
    type Data = FlatpackerData;
    type StaticData = ();
}

/// `ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FlatpackerData {
    /// `materials.ui_data()`: what is stored (units), the empty ones left out.
    pub materials: Vec<FlatpackMaterial>,
    pub busy: bool,
    /// The inserted board's machine.
    pub design: Option<FlatpackDesign>,
}

/// A material and its units; `index` is the `ref` of the `eject` act.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FlatpackMaterial {
    pub index: u32,
    pub name: String,
    pub amount: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FlatpackDesign {
    /// `initial(build.name)`.
    pub name: String,
    /// `OPTIMAL_COST(needed * creation_efficiency)` of each.
    pub required: Vec<FlatpackMaterial>,
    /// Empty: it can be built.
    pub disable_reason: String,
}

pub const BUILD: &str = "build";
pub const EJECT_BOARD: &str = "ejectBoard";
pub const EJECT: &str = "eject";
pub const MATERIAL_REF: &str = "ref";
pub const AMOUNT: &str = "amount";
