//! The modular shield windows' data, shared by both halves: tgui `ModularShieldGen`
//! (`/obj/machinery/modular_shield_generator/ui_data`) and `ModularShieldConsole`
//! (`/obj/machinery/computer/modular_shield/ui_data`). The server half is
//! `crate::eng_machines::modular_shield`, the client views `crate::modular_shield_client`.

use serde::{Deserialize, Serialize};
use ss13_tgui::UiInterface;

pub struct ModularShieldGenUi;

impl UiInterface for ModularShieldGenUi {
    const NAME: &'static str = "ModularShieldGen";
    type Data = ModularShieldGenData;
    type StaticData = ();
}

/// `/obj/machinery/modular_shield_generator/ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ModularShieldGenData {
    pub max_radius: f32,
    pub current_radius: f32,
    pub max_strength: f32,
    pub max_regeneration: f32,
    pub current_regeneration: f32,
    pub current_strength: f32,
    pub active: bool,
    pub recovering: bool,
    pub exterior_only: bool,
    pub initiating_field: bool,
}

pub struct ModularShieldConsoleUi;

impl UiInterface for ModularShieldConsoleUi {
    const NAME: &'static str = "ModularShieldConsole";
    type Data = ModularShieldConsoleData;
    type StaticData = ();
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ModularShieldConsoleData {
    pub generators: Vec<LinkedGenerator>,
}

/// One linked generator; `id` (1-based) is the acts' `id`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LinkedGenerator {
    pub name: String,
    pub id: u32,
    pub max_strength: f32,
    pub current_strength: f32,
    pub active: bool,
    /// `recovering || initiating`.
    pub recovering: bool,
    pub current_regeneration: f32,
    /// Unpowered.
    pub inactive: bool,
}

pub const SET_RADIUS: &str = "set_radius";
pub const NEW_RADIUS: &str = "new_radius";
pub const TOGGLE_SHIELDS: &str = "toggle_shields";
pub const TOGGLE_EXTERIOR: &str = "toggle_exterior";
pub const RENAME: &str = "rename";
pub const ID: &str = "id";
pub const NAME: &str = "name";
