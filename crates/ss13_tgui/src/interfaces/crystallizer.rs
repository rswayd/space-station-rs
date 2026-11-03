//! `Crystallizer`: `tgui/packages/tgui/interfaces/Crystallizer.tsx` and
//! `/obj/machinery/atmospherics/components/binary/crystallizer/ui_data`
//! (`code/modules/atmospherics/machinery/components/gas_recipe_machines/crystallizer.dm`). The
//! server half lives with the game (`ss13_game::atmos_machinery::crystallizer`).

use serde::{Deserialize, Serialize};

use crate::UiInterface;

pub struct CrystallizerUi;

/// `ui_static_data`: "Nothing" (id `""`) and the crystallizer's recipes.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct CrystallizerStatic {
    pub selected_recipes: Vec<RecipeEntry>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct RecipeEntry {
    pub name: String,
    pub id: String,
}

/// `ui_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct CrystallizerData {
    pub on: bool,
    /// The selected recipe's id, `""` for none.
    pub selected: String,
    pub internal_gas_data: Vec<InternalGas>,
    /// Lines joined with `\n`.
    pub requirements: String,
    /// Kelvin, 0 when the internal mixture is empty.
    pub internal_temperature: f32,
    /// 0-100.
    pub progress_bar: f32,
    pub gas_input: f32,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct InternalGas {
    pub name: String,
    pub id: String,
    pub amount: f32,
}

impl UiInterface for CrystallizerUi {
    const NAME: &'static str = "Crystallizer";
    type Data = CrystallizerData;
    type StaticData = CrystallizerStatic;
}

/// `logScale`: `log2(16 + max(0, value)) - 4`.
pub fn log_scale(value: f32) -> f32 {
    (16.0 + value.max(0.0)).log2() - 4.0
}

#[cfg(feature = "client")]
mod client_side {
    use ss13_ui_kit::prelude::*;
    use ss13_ui_kit::{ActParams, UiColor};

    use super::{CrystallizerData, CrystallizerStatic, CrystallizerUi, log_scale};
    use crate::client::{ActOnChange, ClientUi};

    /// `getGasColor` (`tgui/packages/tgui/constants.ts`). tg sends DM gas ids, two of which
    /// (`nitrium`, and `hypernoblium` matching only by luck) differ from the table's; both
    /// spellings are known here.
    fn gas_color(id: &str) -> UiColor {
        match id {
            "o2" => UiColor::Blue,
            "n2" => UiColor::Yellow,
            "co2" => UiColor::Grey,
            "plasma" => UiColor::Pink,
            "water_vapor" => UiColor::Css([176, 196, 222]),
            "hypernoblium" | "nob" => UiColor::Teal,
            "n2o" => UiColor::Css([255, 228, 196]),
            "no2" | "nitrium" => UiColor::Brown,
            "tritium" => UiColor::Css([50, 205, 50]),
            "bz" => UiColor::Css([147, 112, 219]),
            "pluoxium" => UiColor::Css([123, 104, 238]),
            "miasma" => UiColor::Olive,
            "freon" => UiColor::Css([175, 238, 238]),
            "hydrogen" => UiColor::White,
            "healium" => UiColor::Css([250, 128, 114]),
            "proto_nitrate" => UiColor::Css([173, 255, 47]),
            "zauker" => UiColor::Css([0, 100, 0]),
            "halon" => UiColor::Purple,
            "helium" => UiColor::Css([240, 248, 255]),
            "antinoblium" => UiColor::Css([128, 0, 0]),
            _ => UiColor::Black,
        }
    }

    fn param(key: &str, value: &str) -> ActParams {
        ActParams(vec![(key.to_owned(), value.to_owned())])
    }

    impl ClientUi for CrystallizerUi {
        const WIDTH: f32 = 500.0;
        const HEIGHT: f32 = 600.0;

        fn content(statics: &CrystallizerStatic, data: &CrystallizerData) -> impl Scene {
            let on = data.on;
            let (power_icon, power_text) = if on {
                ("power-off", "On")
            } else {
                ("times", "Off")
            };
            let recipes: Vec<_> = statics
                .selected_recipes
                .iter()
                .map(|r| {
                    let (name, selected) = (r.name.clone(), r.id == data.selected);
                    let mode = param("mode", &r.id);
                    bsn! {
                        @Button { @content: name }
                        Button { selected: selected }
                        Act("recipe")
                        mode
                    }
                })
                .collect();
            let input = NumberInput {
                value: f64::from(data.gas_input),
                min_value: 0.0,
                max_value: 250.0,
                step: 0.1,
                unit: "moles/s".into(),
                tick_while_dragging: true,
                ..Default::default()
            };
            let on_input = ActOnChange {
                action: "gas_input".into(),
                param: "gas_input".into(),
            };
            let progress = data.progress_bar / 100.0;
            let progress_ranges = vec![
                (UiColor::Good, 0.67, 1.0),
                (UiColor::Average, 0.34, 0.66),
                (UiColor::Bad, 0.0, 0.33),
            ];
            let lines: Vec<_> = data
                .requirements
                .split('\n')
                .map(|l| {
                    let l = l.to_owned();
                    bsn! { Text(l) }
                })
                .collect();
            let temperature = log_scale(data.internal_temperature);
            let temperature_ranges = vec![
                (UiColor::Teal, f32::NEG_INFINITY, log_scale(80.0)),
                (UiColor::Good, log_scale(80.0), log_scale(600.0)),
                (UiColor::Average, log_scale(600.0), log_scale(5000.0)),
                (UiColor::Bad, log_scale(5000.0), f32::INFINITY),
            ];
            let max_temperature = log_scale(10000.0);
            let temperature_text = format!("{:.0} K", data.internal_temperature);
            let gases: Vec<_> = data
                .internal_gas_data
                .iter()
                .map(|g| {
                    let (label, color) = (g.name.clone(), gas_color(&g.id));
                    let value = g.amount / 1000.0;
                    let text = format!("{:.2} moles", g.amount);
                    bsn! {
                        @LabeledListItem { @label: label }
                        Children [
                            @ProgressBar
                            ProgressBar { value: value, color: color }
                            Node { flex_grow: 1.0, width: percent(100) }
                            Children [ Text(text) ]
                        ]
                    }
                })
                .collect();
            bsn! {
                @WindowContent { @scrollable: true }
                Children [
                    @Section { @title: "Controls" }
                    Children [
                        @LabeledList
                        Children [
                            @LabeledListItem { @label: "Power" }
                            Children [
                                @Button { @icon: power_icon, @content: power_text }
                                Button { selected: on }
                                Act("power")
                            ]
                            --
                            @LabeledListItem { @label: "Recipe" }
                            Children [
                                Node { flex_wrap: FlexWrap::Wrap }
                                Children [ {recipes} ]
                            ]
                            --
                            @LabeledListItem { @label: "Gas Input" }
                            Children [
                                // tg 63px; our font does not fit "0.5 moles/s" in it.
                                Node { width: px(90) }
                                input
                                on_input
                            ]
                        ]
                    ]
                    --
                    @Section { @title: "Requirements and progress" }
                    Children [
                        @LabeledList
                        Children [
                            @LabeledListItem { @label: "Progress" }
                            Children [
                                @ProgressBar
                                ProgressBar { value: progress, ranges: progress_ranges }
                                Node { flex_grow: 1.0, width: percent(100) }
                            ]
                            --
                            @LabeledListItem { @label: "Recipe" }
                            Children [
                                Node {
                                    flex_direction: FlexDirection::Column,
                                    margin: UiRect::all(px(6)),
                                }
                                Children [ {lines} ]
                            ]
                            --
                            @LabeledListItem { @label: "Temperature" }
                            Children [
                                @ProgressBar
                                ProgressBar {
                                    value: temperature,
                                    min_value: 0.0,
                                    max_value: max_temperature,
                                    ranges: temperature_ranges,
                                }
                                Node { flex_grow: 1.0, width: percent(100) }
                                Children [ Text(temperature_text) ]
                            ]
                        ]
                    ]
                    --
                    @Section { @title: "Gases" }
                    Children [
                        @LabeledList
                        Children [ {gases} ]
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
    fn data_round_trips_and_the_scale_is_tgs() {
        let data = CrystallizerData {
            on: true,
            selected: "diamond".into(),
            internal_gas_data: vec![InternalGas {
                name: "Carbon Dioxide".into(),
                id: "co2".into(),
                amount: 512.25,
            }],
            requirements: "a\nb".into(),
            internal_temperature: 12000.0,
            progress_bar: 40.0,
            gas_input: 0.5,
        };
        let bytes = crate::encode(&data);
        assert_eq!(crate::decode::<CrystallizerData>(&bytes).unwrap(), data);
        assert_eq!(log_scale(0.0), 0.0);
        assert_eq!(log_scale(-5.0), 0.0);
        assert!((log_scale(48.0) - 2.0).abs() < 1e-6);
    }
}
