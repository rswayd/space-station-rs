//! The vampire, BandaStation's antagonist (`modular_bandastation/vampire`, a Paradise port): it
//! drinks the crew's blood, spends it on its powers and picks a specialization. [`server`] is
//! the antagonist, [`client`] draws its windows; this module holds what both share.
//!
//! - `AntagInfoVampire`: the info panel (`AntagInfoVampire.tsx`): introduction, objectives, the
//!   blood drunk and the four specializations with their powers; picking one is the act named
//!   by its `id`.
//! - `VampireHud` (headless): the usable blood counter on the HUD (`/atom/movable/screen/
//!   vampire_blood` at `WEST:6,CENTER-1:15`).

use serde::{Deserialize, Serialize};
use ss13_tgui::UiInterface;

use super::ObjectiveInfo;

#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "server")]
pub mod server;

/// tg `ROLE_VAMPIRE`: the vampire's `be_special` key.
pub const ROLE_VAMPIRE: &str = "Vampire";

pub struct AntagInfoVampireUi;

impl UiInterface for AntagInfoVampireUi {
    const NAME: &'static str = "AntagInfoVampire";
    type Data = VampireInfoData;
    type StaticData = VampireInfoStatic;
}

/// `ui_static_data`: every specialization (`/datum/vampire_subclass/get_ui_data`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct VampireInfoStatic {
    pub subclasses: Vec<SubclassInfo>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SubclassInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub powers: Vec<PowerInfo>,
    pub full_powers: Vec<PowerInfo>,
}

/// `get_power_ui_data`: a spell's name and description, or a passive's `gain_desc`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PowerInfo {
    pub name: Option<String>,
    pub description: String,
    pub blood_required: Option<u32>,
}

/// `ui_data`, with the objectives (tg sends them once; here they follow the specialization).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct VampireInfoData {
    pub total_blood: f32,
    pub specialization_blood_required: u32,
    pub selected_subclass: Option<String>,
    pub can_select_subclass: bool,
    pub objectives: Vec<ObjectiveInfo>,
}

/// The usable blood counter.
pub struct VampireHudUi;

impl UiInterface for VampireHudUi {
    const NAME: &'static str = "VampireHud";
    type Data = VampireHudData;
    type StaticData = ();
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(bevy::prelude::Component))]
pub struct VampireHudData {
    /// `bloodusable`.
    pub usable: f32,
}

/// tg `[value]` of a blood amount: whole numbers without decimals.
pub fn blood_text(value: f32) -> String {
    let rounded = (value * 10.0).round() / 10.0;
    if rounded.fract() == 0.0 {
        format!("{}", rounded as i64)
    } else {
        format!("{rounded}")
    }
}
