//! Assemblies (tg `code/modules/assembly/`): signalers, timers, proximity, health and infrared
//! sensors, igniters and voice analyzers, the holders that join them, their wires, and what
//! they trigger: the chemical grenade casing (`/obj/item/grenade/chem_grenade`), the tank
//! transfer valve (`/obj/item/transfer_valve`) and the single tank bomb (`bomb_assemble`).
//!
//! The windows' data (tgui `Signaler`, `Timer`, `ProximitySensor`, `HealthSensor`,
//! `InfraredEmitter`, `TransferValve`) lives here, shared by both halves; [`server`] runs the
//! devices, [`client`] draws the windows.

#[cfg(feature = "server")]
pub mod chem_grenade;
#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "server")]
pub mod server;
#[cfg(feature = "server")]
pub mod ttv;

use serde::{Deserialize, Serialize};
use ss13_tgui::UiInterface;

/// tgui `Signaler` (`tgui/packages/tgui/interfaces/Signaler.tsx`).
pub struct SignalerUi;

impl UiInterface for SignalerUi {
    const NAME: &'static str = "Signaler";
    type Data = SignalerData;
    type StaticData = ();
}

/// `/obj/item/assembly/signaler/ui_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct SignalerData {
    /// Deci-kHz (1457 is 145.7).
    pub frequency: u16,
    /// `cooldown_length`, deciseconds.
    pub cooldown: u32,
    pub code: i32,
    pub min_frequency: u16,
    pub max_frequency: u16,
}

/// tgui `Timer` (`Timer.tsx`).
pub struct TimerUi;

impl UiInterface for TimerUi {
    const NAME: &'static str = "Timer";
    type Data = TimerData;
    type StaticData = ();
}

/// `/obj/item/assembly/timer/ui_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct TimerData {
    pub minutes: u32,
    pub seconds: u32,
    pub timing: bool,
    #[serde(rename = "loop")]
    pub looping: bool,
}

/// tgui `ProximitySensor` (`ProximitySensor.jsx`).
pub struct ProximitySensorUi;

impl UiInterface for ProximitySensorUi {
    const NAME: &'static str = "ProximitySensor";
    type Data = ProximityData;
    type StaticData = ();
}

/// `/obj/item/assembly/prox_sensor/ui_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct ProximityData {
    pub minutes: u32,
    pub seconds: u32,
    pub timing: bool,
    pub scanning: bool,
    pub sensitivity: u8,
}

/// tgui `HealthSensor` (`HealthSensor.tsx`).
pub struct HealthSensorUi;

impl UiInterface for HealthSensorUi {
    const NAME: &'static str = "HealthSensor";
    type Data = HealthSensorData;
    type StaticData = ();
}

/// `/obj/item/assembly/health/ui_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct HealthSensorData {
    /// `health_scan`: the last health read (`null` before the first scan).
    pub health: Option<f32>,
    pub scanning: bool,
    /// `health_target`: `HEALTH_THRESHOLD_CRIT` (0) or `HEALTH_THRESHOLD_DEAD` (-100).
    pub target: f32,
}

/// tgui `InfraredEmitter` (`InfraredEmitter.tsx`).
pub struct InfraredEmitterUi;

impl UiInterface for InfraredEmitterUi {
    const NAME: &'static str = "InfraredEmitter";
    type Data = InfraredData;
    type StaticData = ();
}

/// `/obj/item/assembly/infra/ui_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct InfraredData {
    pub on: bool,
    pub visible: bool,
}

/// tgui `TransferValve` (`TransferValve.jsx`).
pub struct TransferValveUi;

impl UiInterface for TransferValveUi {
    const NAME: &'static str = "TransferValve";
    type Data = TransferValveData;
    type StaticData = ();
}

/// `/obj/item/transfer_valve/ui_data`: the names of what is attached.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct TransferValveData {
    pub tank_one: Option<String>,
    pub tank_two: Option<String>,
    pub attached_device: Option<String>,
    pub valve: bool,
}

/// Act names and params, as the tgui interfaces send them.
pub mod acts {
    pub const SIGNAL: &str = "signal";
    pub const FREQ: &str = "freq";
    pub const CODE: &str = "code";
    pub const RESET: &str = "reset";
    pub const TIME: &str = "time";
    pub const REPEAT: &str = "repeat";
    pub const INPUT: &str = "input";
    /// Param of [`INPUT`]: seconds to add.
    pub const ADJUST: &str = "adjust";
    pub const SCANNING: &str = "scanning";
    pub const SENSE: &str = "sense";
    /// Param of [`SENSE`]: the sensitivity step.
    pub const RANGE: &str = "range";
    pub const TARGET: &str = "target";
    pub const POWER: &str = "power";
    pub const VISIBILITY: &str = "visibility";
    pub const TANK_ONE: &str = "tankone";
    pub const TANK_TWO: &str = "tanktwo";
    pub const TOGGLE: &str = "toggle";
    pub const DEVICE: &str = "device";
    pub const REMOVE_DEVICE: &str = "remove_device";
}

/// `format_frequency`: `145.7`.
pub fn format_frequency(freq: u16) -> String {
    format!("{}.{}", freq / 10, freq % 10)
}

/// The `minutes`/`seconds` split of tg's timer windows: `round(time % 60)` and
/// `round((time - seconds) / 60)` (DM one-argument `round` floors).
pub fn minutes_seconds(time: f64) -> (u32, u32) {
    let time = time.max(0.0);
    let seconds = (time % 60.0).floor();
    let minutes = ((time - seconds) / 60.0).floor();
    (minutes as u32, seconds as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timer_windows_split_like_tg() {
        assert_eq!(minutes_seconds(10.0), (0, 10));
        assert_eq!(minutes_seconds(75.5), (1, 15));
        assert_eq!(minutes_seconds(600.0), (10, 0));
        assert_eq!(minutes_seconds(-1.0), (0, 0));
        assert_eq!(format_frequency(1457), "145.7");
    }
}
