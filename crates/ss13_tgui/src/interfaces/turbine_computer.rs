//! `TurbineComputer`: `tgui/packages/tgui/interfaces/TurbineComputer.tsx` and
//! `/obj/machinery/computer/turbine_computer/ui_data` (`code/modules/power/turbine/
//! turbine_computer.dm`). The server half lives with the game (`ss13_game::power::turbine`).
//!
//! Acts: `toggle_power`; `regulate` with `regulate` the intake in percent (1-100; tg's window
//! sends the fraction, the server here divides by 100).

use serde::{Deserialize, Serialize};

use crate::UiInterface;

pub struct TurbineComputerUi;

/// `ui_data`; everything past `connected` is zero while not connected.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct TurbineComputerData {
    pub connected: bool,
    pub active: bool,
    pub rpm: f32,
    /// Watts (`energy_to_power(produced_energy)`).
    pub power: f32,
    /// The compressor's input turf, kelvin.
    pub temp: f32,
    /// 0-100.
    pub integrity: f32,
    pub max_rpm: f32,
    pub max_temperature: f32,
    /// `intake_regulator`, 0.01-1.
    pub regulator: f32,
}

impl UiInterface for TurbineComputerUi {
    const NAME: &'static str = "TurbineComputer";
    type Data = TurbineComputerData;
    type StaticData = ();
}

#[cfg(feature = "client")]
mod client_side {
    use bevy::scene::EntityScene;
    use ss13_ui_kit::prelude::*;

    use super::{TurbineComputerData, TurbineComputerUi};
    use crate::client::{ActOnChange, ClientUi};
    use crate::interfaces::solar_control::format_power;

    impl ClientUi for TurbineComputerUi {
        const WIDTH: f32 = 310.0;
        const HEIGHT: f32 = 240.0;

        fn content(_: &(), data: &TurbineComputerData) -> impl Scene {
            // shortcut: tg shows this in a `Modal`; a notice box stands in.
            let out_of_service = (!data.connected).then(|| {
                EntityScene(bsn! {
                    @NoticeBox
                    Children [ Text(
                        "Parts not connected, close all mantainence panels/use a multitool on the rotor before trying again"
                    ) ]
                })
            });
            let display = data.connected.then(|| {
                let (icon, text) = if data.active {
                    ("power-off", "Online")
                } else {
                    ("times", "Offline")
                };
                let (selected, disabled) = (data.active, data.rpm >= 1000.0);
                let input = NumberInput {
                    value: f64::from(data.regulator) * 100.0,
                    min_value: 1.0,
                    max_value: 100.0,
                    step: 1.0,
                    unit: "%".into(),
                    tick_while_dragging: true,
                    ..Default::default()
                };
                let on_change = ActOnChange {
                    action: "regulate".into(),
                    param: "regulate".into(),
                };
                let regulator: Box<dyn Scene> = Box::new(bsn! {
                    Node { width: px(80) }
                    input
                    on_change
                });
                let ranges = vec![
                    (UiColor::Good, 60.0, 100.0),
                    (UiColor::Average, 40.0, 59.0),
                    (UiColor::Bad, 0.0, 39.0),
                ];
                let (integrity, min, max) = (data.integrity, 0.0, 100.0);
                let speed = format!("{} RPM", data.rpm);
                let max_speed = format!("{} RPM", data.max_rpm);
                let temp = format!("{} K", data.temp);
                let max_temp = format!("{} K", data.max_temperature);
                let power = format_power(data.power);
                EntityScene(bsn! {
                    @Section {
                        @title: "Status",
                        @buttons: bsn_list! {
                            @Button { @icon: icon, @content: text }
                            Button { selected: selected, disabled: disabled }
                            Act("toggle_power")
                        },
                    }
                    Children [
                        @LabeledList
                        Children [
                            @LabeledListItem { @label: "Intake Regulator" }
                            Children [ {EntityScene(regulator)} ]
                            --
                            @LabeledListItem { @label: "Turbine Integrity" }
                            Children [
                                @ProgressBar
                                ProgressBar {
                                    value: integrity,
                                    min_value: min,
                                    max_value: max,
                                    ranges: ranges,
                                }
                                Node { flex_grow: 1.0, width: percent(100) }
                            ]
                            --
                            @LabeledListItem { @label: "Turbine Speed" }
                            Children [ Text(speed) ]
                            --
                            @LabeledListItem { @label: "Max Turbine Speed" }
                            Children [ Text(max_speed) ]
                            --
                            @LabeledListItem { @label: "Input Temperature" }
                            Children [ Text(temp) ]
                            --
                            @LabeledListItem { @label: "Max Temperature" }
                            Children [ Text(max_temp) ]
                            --
                            @LabeledListItem { @label: "Generated Power" }
                            Children [ Text(power) ]
                        ]
                    ]
                })
            });
            bsn! {
                @WindowContent
                Children [ {out_of_service} -- {display} ]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_round_trips() {
        let data = TurbineComputerData {
            connected: true,
            active: true,
            rpm: 1200.0,
            power: 27_000.0,
            temp: 900.0,
            integrity: 98.5,
            max_rpm: 50_000.0,
            max_temperature: 50_000.0,
            regulator: 0.5,
        };
        let bytes = crate::encode(&data);
        assert_eq!(crate::decode::<TurbineComputerData>(&bytes).unwrap(), data);
    }
}
