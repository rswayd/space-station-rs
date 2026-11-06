//! `SpaceHeater`: `tgui/packages/tgui/interfaces/SpaceHeater.jsx` and
//! `/obj/machinery/space_heater/ui_data` (`code/game/machinery/spaceheater.dm`). The server half
//! lives with the game (`ss13_game::atmos_machinery::space_heater`).

use serde::{Deserialize, Serialize};

use crate::UiInterface;

pub struct SpaceHeaterUi;

/// `ui_data`; `power_level` (`round(cell.percent())`) only with a cell, `current_temp` is
/// `None` where tg sends `"N/A"`. Temperatures are °C. `chemHacked` (always false on a
/// station heater) and the improvised chem heater's beaker are not sent.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct SpaceHeaterData {
    pub open: bool,
    pub on: bool,
    /// `set_mode`: "auto", "heat" or "cool".
    pub mode: String,
    pub has_powercell: bool,
    pub power_level: Option<f32>,
    pub target_temp: f32,
    pub min_temp: f32,
    pub max_temp: f32,
    pub current_temp: Option<f32>,
}

impl UiInterface for SpaceHeaterUi {
    const NAME: &'static str = "SpaceHeater";
    type Data = SpaceHeaterData;
    type StaticData = ();
}

#[cfg(feature = "client")]
mod client_side {
    use bevy::scene::EntityScene;
    use ss13_ui_kit::ActParams;
    use ss13_ui_kit::prelude::*;

    use super::{SpaceHeaterData, SpaceHeaterUi};
    use crate::client::{ActOnChange, ClientUi};

    fn mode_button(
        mode: &str,
        icon: &'static str,
        label: &'static str,
        current: &str,
    ) -> Box<dyn Scene> {
        let selected = current == mode;
        let params = ActParams(vec![("mode".into(), mode.to_owned())]);
        Box::new(bsn! {
            @Button { @icon: icon, @content: label }
            Button { selected: selected }
            Act("mode")
            params
        })
    }

    impl ClientUi for SpaceHeaterUi {
        const WIDTH: f32 = 400.0;
        const HEIGHT: f32 = 305.0;

        fn content(_: &(), data: &SpaceHeaterData) -> impl Scene {
            let eject_disabled = !data.has_powercell || !data.open;
            let (power_icon, power_text) = if data.on {
                ("power-off", "On")
            } else {
                ("times", "Off")
            };
            let power_selected = data.on;
            let power_disabled = !data.has_powercell;
            let cell: Box<dyn Scene> = match data.power_level.filter(|_| data.has_powercell) {
                Some(level) => {
                    let ranges = vec![
                        (UiColor::Good, 0.6, f32::INFINITY),
                        (UiColor::Average, 0.3, 0.6),
                        (UiColor::Bad, f32::NEG_INFINITY, 0.3),
                    ];
                    let value = level / 100.0;
                    Box::new(bsn! {
                        @ProgressBar
                        ProgressBar { value: value, ranges: ranges }
                        Node { flex_grow: 1.0, width: percent(100) }
                    })
                }
                None => Box::new(bsn! {
                    Text("None")
                    TextColor({UiColor::Bad.base()})
                }),
            };
            // shortcut: the 18px font of the current temperature is the default size; add it when
            // ui_kit has a `Box fontSize`.
            let current: Box<dyn Scene> = match data.current_temp {
                Some(t) => {
                    let gap = (data.target_temp - t).abs();
                    let color = if gap > 50.0 {
                        UiColor::Bad
                    } else if gap > 20.0 {
                        UiColor::Average
                    } else {
                        UiColor::Good
                    };
                    let text = format!("{t}°C");
                    Box::new(bsn! { Text(text) TextColor({color.base()}) })
                }
                None => Box::new(bsn! { Text("N/A°C") }),
            };
            let target: Box<dyn Scene> = if data.open {
                let input = NumberInput {
                    value: f64::from(data.target_temp),
                    min_value: f64::from(data.min_temp),
                    max_value: f64::from(data.max_temp),
                    step: 1.0,
                    unit: "°C".into(),
                    ..Default::default()
                };
                let on_change = ActOnChange {
                    action: "target".into(),
                    param: "target".into(),
                };
                Box::new(bsn! {
                    Node { width: px(65) }
                    input
                    on_change
                })
            } else {
                let text = format!("{}°C", data.target_temp);
                Box::new(bsn! { Text(text) })
            };
            let mode: Vec<Box<dyn Scene>> = if data.open {
                vec![
                    mode_button("auto", "thermometer-half", "Auto", &data.mode),
                    mode_button("heat", "fire-alt", "Heat", &data.mode),
                    mode_button("cool", "fan", "Cool", &data.mode),
                ]
            } else {
                // `capitalize(data.mode)`.
                let mut chars = data.mode.chars();
                let text: String = chars
                    .next()
                    .map(|c| c.to_uppercase().chain(chars).collect())
                    .unwrap_or_default();
                vec![Box::new(bsn! { Text(text) })]
            };
            bsn! {
                @WindowContent
                Children [
                    @Section {
                        @title: "Power",
                        @buttons: bsn_list! {
                            @Button { @icon: "eject", @content: "Eject Cell" }
                            Button { disabled: eject_disabled }
                            Act("eject")
                            --
                            @Button { @icon: power_icon, @content: power_text }
                            Button { selected: power_selected, disabled: power_disabled }
                            Act("power")
                        },
                    }
                    Children [
                        @LabeledList
                        Children [
                            @LabeledListItem { @label: "Cell" }
                            Children [ {EntityScene(cell)} ]
                        ]
                    ]
                    --
                    @Section { @title: "Thermostat" }
                    Children [
                        @LabeledList
                        Children [
                            @LabeledListItem { @label: "Current Temperature" }
                            Children [ {EntityScene(current)} ]
                            --
                            @LabeledListItem { @label: "Target Temperature" }
                            Children [ {EntityScene(target)} ]
                            --
                            @LabeledListItem { @label: "Mode" }
                            Children [ {mode} ]
                        ]
                    ]
                ]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_round_trips() {
        let data = SpaceHeaterData {
            open: true,
            on: true,
            mode: "auto".into(),
            has_powercell: true,
            power_level: Some(80.0),
            target_temp: 20.0,
            min_temp: 0.0,
            max_temp: 60.0,
            current_temp: None,
        };
        let bytes = crate::encode(&data);
        assert_eq!(crate::decode::<SpaceHeaterData>(&bytes).unwrap(), data);
    }
}
