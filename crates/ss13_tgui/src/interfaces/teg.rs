//! `ThermoElectricGenerator`: `tgui/packages/tgui/interfaces/ThermoElectricGenerator.tsx` and
//! `/obj/machinery/power/thermoelectric_generator/ui_data` (`code/modules/power/
//! thermoelectric_generator.dm`). The server half lives with the game (`ss13_game::power::teg`).

use serde::{Deserialize, Serialize};

use crate::UiInterface;

pub struct ThermoElectricGeneratorUi;

/// One loop's `cold_data`/`hot_data` entry: the circulator's input (`airs[2]`) and output
/// (`airs[1]`), rounded to 0.1.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct CirculatorData {
    pub temperature_inlet: f32,
    pub temperature_outlet: f32,
    pub pressure_inlet: f32,
    pub pressure_outlet: f32,
}

/// `ui_data`: an error, or the output and both loops.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct ThermoElectricGeneratorData {
    pub error_message: Option<String>,
    /// Watts (tg sends `display_power(lastgenlev)`; formatted by the client here).
    pub last_power_output: f32,
    pub cold_data: Option<CirculatorData>,
    pub hot_data: Option<CirculatorData>,
}

impl UiInterface for ThermoElectricGeneratorUi {
    const NAME: &'static str = "ThermoElectricGenerator";
    type Data = ThermoElectricGeneratorData;
    type StaticData = ();
}

#[cfg(feature = "client")]
mod client_side {
    use bevy::scene::EntityScene;
    use ss13_ui_kit::prelude::*;

    use super::{CirculatorData, ThermoElectricGeneratorData, ThermoElectricGeneratorUi};
    use crate::client::ClientUi;
    use crate::interfaces::solar_control::format_power;

    fn rows(data: Option<&CirculatorData>) -> impl SceneList + use<> {
        data.map(|d| {
            let temperature = format!(
                "Temperature Inlet: {} K / Outlet: {} K",
                d.temperature_inlet, d.temperature_outlet
            );
            let pressure = format!(
                "Pressure Inlet: {} kPa / Outlet: {} kPa",
                d.pressure_inlet, d.pressure_outlet
            );
            EntityScene(bsn! {
                @UiBox
                Children [
                    @UiBox Children [ Text(temperature) ]
                    --
                    @UiBox Children [ Text(pressure) ]
                ]
            })
        })
    }

    impl ClientUi for ThermoElectricGeneratorUi {
        // shortcut: one window size; tg shrinks it to 320x100 for an error.
        const WIDTH: f32 = 350.0;
        const HEIGHT: f32 = 195.0;

        fn content(_: &(), data: &ThermoElectricGeneratorData) -> impl Scene {
            let error = data.error_message.as_ref().map(|e| {
                let text = format!("ERROR: {e}");
                EntityScene(bsn! { @Section Children [ Text(text) ] })
            });
            let body = data.error_message.is_none().then(|| {
                let output = format!("Last Output: {}", format_power(data.last_power_output));
                let (cold, hot) = (rows(data.cold_data.as_ref()), rows(data.hot_data.as_ref()));
                // tg's `textColor="cyan"` is a CSS color; teal is the kit's nearest.
                EntityScene(bsn! {
                    @Section
                    Children [
                        @UiBox Children [ Text(output) ]
                        --
                        @Divider
                        --
                        @UiBox { @color: UiColor::Teal, @bold: true } Children [ Text("Cold Loop") ]
                        --
                        {cold}
                        --
                        @UiBox { @color: UiColor::Red, @bold: true } Children [ Text("Hot loop") ]
                        --
                        {hot}
                    ]
                })
            });
            bsn! {
                @WindowContent
                Children [ {error} -- {body} ]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_round_trips() {
        let data = ThermoElectricGeneratorData {
            error_message: None,
            last_power_output: 12_500.0,
            cold_data: Some(CirculatorData {
                temperature_inlet: 80.5,
                temperature_outlet: 120.0,
                pressure_inlet: 4500.0,
                pressure_outlet: 2000.0,
            }),
            hot_data: None,
        };
        let bytes = crate::encode(&data);
        assert_eq!(
            crate::decode::<ThermoElectricGeneratorData>(&bytes).unwrap(),
            data
        );
    }
}
