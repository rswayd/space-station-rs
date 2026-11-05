//! `PortableGenerator`: `tgui/packages/tgui/interfaces/PortableGenerator.jsx` and
//! `/obj/machinery/power/port_gen/pacman/ui_data` (`code/modules/power/port_gen.dm`). The
//! server half lives with the game (`ss13_game::power::pacman`).

use serde::{Deserialize, Serialize};

use crate::UiInterface;

pub struct PortableGeneratorUi;

/// `ui_data`. Powers are watts (tg sends `display_power`; formatted by the client here).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct PortableGeneratorData {
    pub active: bool,
    pub sheet_name: String,
    pub sheets: u32,
    /// `round(sheet_left * 100, 0.1)`.
    pub stack_percent: f32,
    pub anchored: bool,
    pub connected: bool,
    pub ready_to_boot: bool,
    pub power_generated: f32,
    pub power_output: f32,
    /// The powernet's `avail` (0 unconnected).
    pub power_available: f32,
    pub current_heat: f32,
}

impl UiInterface for PortableGeneratorUi {
    const NAME: &'static str = "PortableGenerator";
    type Data = PortableGeneratorData;
    type StaticData = ();
}

#[cfg(feature = "client")]
mod client_side {
    use bevy::scene::EntityScene;
    use ss13_ui_kit::prelude::*;

    use super::{PortableGeneratorData, PortableGeneratorUi};
    use crate::client::ClientUi;
    use crate::interfaces::solar_control::format_power;

    fn colored(text: String, color: UiColor) -> Box<dyn Scene> {
        Box::new(bsn! {
            Text(text)
            TextColor({color.base()})
        })
    }

    impl ClientUi for PortableGeneratorUi {
        const WIDTH: f32 = 450.0;
        const HEIGHT: f32 = 340.0;

        fn content(_: &(), data: &PortableGeneratorData) -> impl Scene {
            let notice = (!data.anchored).then(|| {
                EntityScene(bsn! {
                    @NoticeBox
                    Children [ Text("Generator not anchored.") ]
                })
            });
            let (power_icon, power_text) = if data.active {
                ("power-off", "On")
            } else {
                ("times", "Off")
            };
            let power_disabled = !data.ready_to_boot;
            let stack_color = if data.stack_percent > 50.0 {
                UiColor::Good
            } else if data.stack_percent > 15.0 {
                UiColor::Average
            } else {
                UiColor::Bad
            };
            let sheets_label = format!("{} sheets", data.sheet_name);
            let sheets = EntityScene(colored(data.sheets.to_string(), stack_color));
            let eject = (data.sheets >= 1).then(|| {
                let disabled = data.active;
                EntityScene(bsn! {
                    @Button { @icon: "eject", @content: "Eject" }
                    Button { disabled: disabled }
                    Act("eject")
                })
            });
            // `ranges={{ good: [0.1, Infinity], average: [0.01, 0.1], bad: [-Infinity, 0.01] }}`.
            let ranges = vec![
                (UiColor::Good, 0.1, f32::INFINITY),
                (UiColor::Average, 0.01, 0.1),
                (UiColor::Bad, f32::NEG_INFINITY, 0.01),
            ];
            let value = data.stack_percent / 100.0;
            let heat = EntityScene(if data.current_heat < 100.0 {
                colored("Nominal".to_owned(), UiColor::Good)
            } else if data.current_heat < 200.0 {
                colored("Caution".to_owned(), UiColor::Average)
            } else {
                colored("DANGER".to_owned(), UiColor::Bad)
            });
            let output = format_power(data.power_output);
            let lower = format_power(data.power_generated);
            let higher = lower.clone();
            let available = EntityScene(if data.connected {
                let text = format_power(data.power_available);
                Box::new(bsn! { Text(text) }) as Box<dyn Scene>
            } else {
                colored("Unconnected".to_owned(), UiColor::Bad)
            });
            bsn! {
                @WindowContent
                Children [
                    {notice}
                    --
                    @Section { @title: "Status" }
                    Children [
                        @LabeledList
                        Children [
                            @LabeledListItem { @label: "Power switch" }
                            Children [
                                @Button { @icon: power_icon, @content: power_text }
                                Button { disabled: power_disabled }
                                Act("toggle_power")
                            ]
                            --
                            @LabeledListItem { @label: sheets_label }
                            Children [ {sheets} -- {eject} ]
                            --
                            @LabeledListItem { @label: "Current sheet level" }
                            Children [
                                @ProgressBar
                                ProgressBar { value: value, ranges: ranges }
                                Node { flex_grow: 1.0, width: percent(100) }
                            ]
                            --
                            @LabeledListItem { @label: "Heat level" }
                            Children [ {heat} ]
                        ]
                    ]
                    --
                    @Section { @title: "Output" }
                    Children [
                        @LabeledList
                        Children [
                            @LabeledListItem { @label: "Current output" }
                            Children [ Text(output) ]
                            --
                            @LabeledListItem { @label: "Adjust output" }
                            Children [
                                @Button { @icon: "minus", @content: lower }
                                Act("lower_power")
                                --
                                @Button { @icon: "plus", @content: higher }
                                Act("higher_power")
                            ]
                            --
                            @LabeledListItem { @label: "Power available" }
                            Children [ {available} ]
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
        let data = PortableGeneratorData {
            active: true,
            sheet_name: "Plasma".into(),
            sheets: 15,
            stack_percent: 42.5,
            anchored: true,
            connected: true,
            ready_to_boot: true,
            power_generated: 10_000.0,
            power_output: 20_000.0,
            power_available: 150_000.0,
            current_heat: 76.0,
        };
        let bytes = crate::encode(&data);
        assert_eq!(
            crate::decode::<PortableGeneratorData>(&bytes).unwrap(),
            data
        );
    }
}
