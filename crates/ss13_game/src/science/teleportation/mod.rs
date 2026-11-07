//! Station teleportation, after tg `code/game/machinery/teleporter.dm`,
//! `code/game/machinery/computer/teleporter.dm`, `code/game/objects/items/teleportation.dm`,
//! `code/game/machinery/launch_pad.dm`, `code/game/machinery/computer/launchpad_control.dm`,
//! `code/modules/awaymissions/gateway.dm` and `code/game/machinery/satellite/satellite_control.dm`.
//!
//! [`teleporter`]: the teleporter hub, station and console (tgui `Teleporter`);
//! [`hand_tele`]: the hand teleporter's portals and the bluespace locator (tgui
//! `BluespaceLocator`); [`launchpad`]: bluespace launchpads and their console (tgui
//! `LaunchpadConsole`); [`gateway`]: gateways and the gateway control console (tgui `Gateway`);
//! [`sat_control`]: the satellite control console (tgui `SatelliteControl`); [`server`] holds
//! the plugin, [`client`] draws the windows. This module holds the windows' data, which both
//! halves share.

use serde::{Deserialize, Serialize};
use ss13_tgui::UiInterface;

#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "server")]
pub mod gateway;
#[cfg(feature = "server")]
pub mod hand_tele;
#[cfg(feature = "server")]
pub mod launchpad;
#[cfg(feature = "server")]
pub mod sat_control;
#[cfg(feature = "server")]
pub mod server;
#[cfg(feature = "server")]
pub mod teleporter;

/// tgui `Teleporter`.
pub struct TeleporterUi;

impl UiInterface for TeleporterUi {
    const NAME: &'static str = "Teleporter";
    type Data = TeleporterData;
    type StaticData = ();
}

/// `/obj/machinery/computer/teleporter/ui_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct TeleporterData {
    pub power_station: bool,
    pub teleporter_hub: bool,
    /// `REGIME_TELEPORTER` ("Teleporter") or `REGIME_GATE` ("Gate").
    pub regime_set: String,
    /// "None", or the target's area name.
    pub target: String,
    pub calibrating: bool,
    pub calibrated: bool,
}

pub const REGIME_TELEPORTER: &str = "Teleporter";
pub const REGIME_GATE: &str = "Gate";
pub const ACT_REGIME: &str = "regimeset";
pub const ACT_TARGET: &str = "settarget";
pub const ACT_CALIBRATE: &str = "calibrate";

/// tgui `BluespaceLocator`.
pub struct BluespaceLocatorUi;

impl UiInterface for BluespaceLocatorUi {
    const NAME: &'static str = "BluespaceLocator";
    type Data = LocatorData;
    type StaticData = ();
}

/// `/obj/item/locator/ui_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct LocatorData {
    pub trackingrange: i32,
    pub trackimplants: Vec<Trackable>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct Trackable {
    pub name: String,
    /// `dir2text(get_dir(...))`; empty on the locator's own tile.
    pub direction: String,
    /// Chebyshev distance in tiles.
    pub distance: u32,
}

/// tgui `LaunchpadConsole`.
pub struct LaunchpadConsoleUi;

impl UiInterface for LaunchpadConsoleUi {
    const NAME: &'static str = "LaunchpadConsole";
    type Data = LaunchpadConsoleData;
    type StaticData = ();
}

/// `/obj/machinery/computer/launchpad/ui_data`: the pad fields only with a pad selected.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct LaunchpadConsoleData {
    pub launchpads: Vec<LaunchpadEntry>,
    /// 1-based, as in tg.
    pub selected_id: Option<u32>,
    pub selected: Option<SelectedPad>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct LaunchpadEntry {
    pub id: u32,
    pub name: String,
    /// Unpowered.
    pub inactive: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct SelectedPad {
    pub x: i32,
    pub y: i32,
    pub pad_name: String,
    pub range: i32,
    pub pad_active: bool,
}

pub const ACT_SELECT_PAD: &str = "select_pad";
pub const ACT_SET_POS: &str = "set_pos";
pub const ACT_MOVE_POS: &str = "move_pos";
pub const ACT_RENAME: &str = "rename";
pub const ACT_REMOVE: &str = "remove";
pub const ACT_LAUNCH: &str = "launch";
pub const ACT_PULL: &str = "pull";

/// tgui `SatelliteControl`.
pub struct SatelliteControlUi;

impl UiInterface for SatelliteControlUi {
    const NAME: &'static str = "SatelliteControl";
    type Data = SatelliteControlData;
    type StaticData = ();
}

/// `/obj/machinery/computer/sat_control/ui_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct SatelliteControlData {
    pub satellites: Vec<SatelliteEntry>,
    /// The meteor shield goal is active.
    pub meteor_shield: bool,
    pub meteor_shield_coverage: u32,
    pub meteor_shield_coverage_max: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct SatelliteEntry {
    pub id: u32,
    pub active: bool,
    pub mode: String,
}

pub const ACT_TOGGLE: &str = "toggle";

/// tgui `Gateway`.
pub struct GatewayUi;

impl UiInterface for GatewayUi {
    const NAME: &'static str = "Gateway";
    type Data = GatewayData;
    type StaticData = ();
}

/// `/obj/machinery/computer/gateway_control/ui_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct GatewayData {
    pub gateway_present: bool,
    /// The gateway is powered.
    pub gateway_status: bool,
    pub current_target: Option<GatewayDestination>,
    pub destinations: Vec<GatewayDestination>,
}

/// `/datum/gateway_destination/get_ui_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct GatewayDestination {
    /// `REF(src)`: here the destination gateway's entity bits.
    pub r#ref: String,
    pub name: String,
    pub available: bool,
    pub reason: String,
    /// Calibration progress (0..1) of a destination that opens after round start.
    pub timeout: Option<f32>,
}

pub const ACT_LINKUP: &str = "linkup";
pub const ACT_ACTIVATE: &str = "activate";
pub const ACT_DEACTIVATE: &str = "deactivate";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_data_round_trips() {
        let data = LaunchpadConsoleData {
            launchpads: vec![LaunchpadEntry {
                id: 1,
                name: "Launchpad".into(),
                inactive: false,
            }],
            selected_id: Some(1),
            selected: Some(SelectedPad {
                x: 2,
                y: -3,
                pad_name: "Launchpad".into(),
                range: 10,
                pad_active: true,
            }),
        };
        let bytes = ss13_tgui::encode(&data);
        assert_eq!(
            ss13_tgui::decode::<LaunchpadConsoleData>(&bytes).unwrap(),
            data
        );
    }
}
