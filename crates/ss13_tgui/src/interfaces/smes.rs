//! `Smes`: `tgui/packages/tgui/interfaces/Smes.tsx` and `/obj/machinery/power/smes/ui_data`,
//! `ui_static_data` (`code/modules/power/smes.dm`). The server half lives with the game
//! (`ss13_game::power::smes`).
// shortcut: the input and output `Slider`s are `NumberInput`s in watts (ss13_ui_kit has no
// slider yet; the slider's available-input fill is the "Доступно" line only).

use serde::{Deserialize, Serialize};

use crate::UiInterface;

pub struct SmesUi;

/// `ui_static_data`: joules, watts.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct SmesStatic {
    pub capacity: f64,
    pub input_level_max: f64,
    pub output_level_max: f64,
}

/// `ui_data`: the charge in joules, levels and flows in watts.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct SmesData {
    pub charge: f64,
    pub input_attempt: bool,
    pub inputting: bool,
    pub input_level: f64,
    pub input_available: f64,
    pub output_attempt: bool,
    pub output_level: f64,
    pub output_used: f64,
    pub outputting: bool,
}

impl UiInterface for SmesUi {
    const NAME: &'static str = "Smes";
    type Data = SmesData;
    type StaticData = SmesStatic;
}

#[cfg(feature = "client")]
mod client_side {
    use bevy::scene::EntityScene;
    use ss13_ui_kit::ActParams;
    use ss13_ui_kit::prelude::*;

    use super::{SmesData, SmesStatic, SmesUi};
    use crate::client::{ActOnChange, ClientUi};
    use crate::interfaces::solar_control::format_power;

    fn colored(text: &str, color: UiColor) -> Box<dyn Scene> {
        let text = text.to_owned();
        Box::new(bsn! {
            Text(text)
            TextColor({color.base()})
        })
    }

    fn param(key: &str, value: String) -> ActParams {
        ActParams(vec![(key.into(), value)])
    }

    /// The «Целевой ввод/вывод» row: min, -10 kW, the level, +10 kW, max.
    fn level_row(action: &'static str, level: f64, max: f64) -> impl Scene {
        let at_min = level == 0.0;
        let at_max = level == max;
        let input = NumberInput {
            value: level,
            min_value: 0.0,
            max_value: max,
            step: 5000.0,
            step_pixel_size: 4.0,
            unit: "W".into(),
            ..Default::default()
        };
        let on_change = ActOnChange {
            action: action.into(),
            param: "target".into(),
        };
        let (to_min, down, up, to_max) = (
            param("target", "min".into()),
            param("adjust", "-10000".into()),
            param("adjust", "10000".into()),
            param("target", "max".into()),
        );
        bsn! {
            Node { flex_direction: FlexDirection::Row }
            Children [
                @Button { @icon: "fast-backward" }
                Button { disabled: at_min }
                Act(action)
                to_min
                --
                @Button { @icon: "backward" }
                Button { disabled: at_min }
                Act(action)
                down
                --
                Node { width: px(90) }
                input
                on_change
                --
                @Button { @icon: "forward" }
                Button { disabled: at_max }
                Act(action)
                up
                --
                @Button { @icon: "fast-forward" }
                Button { disabled: at_max }
                Act(action)
                to_max
            ]
        }
    }

    impl ClientUi for SmesUi {
        const WIDTH: f32 = 340.0;
        const HEIGHT: f32 = 350.0;

        fn content(st: &SmesStatic, data: &SmesData) -> impl Scene {
            let pct = if st.capacity > 0.0 {
                ((1000.0 * data.charge / st.capacity).round() / 10.0) as f32
            } else {
                0.0
            };
            let ranges = vec![
                (UiColor::Good, 0.5, f32::INFINITY),
                (UiColor::Average, 0.15, 0.5),
                (UiColor::Bad, f32::NEG_INFINITY, 0.15),
            ];
            let value = pct * 0.01;
            let input_state = EntityScene(if pct >= 100.0 {
                colored("Полностью заряжен", UiColor::Good)
            } else if data.inputting {
                colored("Заряжается", UiColor::Average)
            } else {
                colored("Не заряжается", UiColor::Bad)
            });
            let output_state = EntityScene(if data.outputting {
                colored("Передача", UiColor::Good)
            } else if data.charge > 0.0 {
                colored("Нет передачи", UiColor::Average)
            } else {
                colored("Нет заряда", UiColor::Bad)
            });
            let (in_icon, in_text) = if data.input_attempt {
                ("sync-alt", "Авто")
            } else {
                ("times", "Выкл")
            };
            let (out_icon, out_text) = if data.output_attempt {
                ("power-off", "Вкл")
            } else {
                ("times", "Выкл")
            };
            let (in_on, out_on) = (data.input_attempt, data.output_attempt);
            let input_row = EntityScene(level_row("input", data.input_level, st.input_level_max));
            let output_row =
                EntityScene(level_row("output", data.output_level, st.output_level_max));
            let available = format_power(data.input_available as f32);
            let used = format_power(data.output_used as f32);
            bsn! {
                @WindowContent
                Children [
                    @Section { @title: "Накопленная энергия" }
                    Children [
                        @ProgressBar
                        ProgressBar { value: value, ranges: ranges }
                        Node { flex_grow: 1.0, width: percent(100) }
                    ]
                    --
                    @Section {
                        @title: "Ввод",
                        @buttons: bsn! {
                            @Button { @icon: in_icon, @content: in_text }
                            Button { selected: in_on }
                            Act("tryinput")
                        },
                    }
                    Children [
                        @LabeledList
                        Children [
                            @LabeledListItem { @label: "Режим зарядки" }
                            Children [ {input_state} ]
                            --
                            @LabeledListItem { @label: "Целевой ввод" }
                            Children [ {input_row} ]
                            --
                            @LabeledListItem { @label: "Доступно" }
                            Children [ Text(available) ]
                        ]
                    ]
                    --
                    @Section {
                        @title: "Вывод",
                        @buttons: bsn! {
                            @Button { @icon: out_icon, @content: out_text }
                            Button { selected: out_on }
                            Act("tryoutput")
                        },
                    }
                    Children [
                        @LabeledList
                        Children [
                            @LabeledListItem { @label: "Режим вывода" }
                            Children [ {output_state} ]
                            --
                            @LabeledListItem { @label: "Целевой вывод" }
                            Children [ {output_row} ]
                            --
                            @LabeledListItem { @label: "Текущий вывод" }
                            Children [ Text(used) ]
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
        let data = SmesData {
            charge: 2.5e7,
            input_attempt: true,
            inputting: true,
            input_level: 50_000.0,
            input_available: 120_000.0,
            output_attempt: true,
            output_level: 90_000.0,
            output_used: 60_000.0,
            outputting: true,
        };
        let bytes = crate::encode(&data);
        assert_eq!(crate::decode::<SmesData>(&bytes).unwrap(), data);
        let st = SmesStatic {
            capacity: 5.0e7,
            input_level_max: 200_000.0,
            output_level_max: 200_000.0,
        };
        assert_eq!(
            crate::decode::<SmesStatic>(&crate::encode(&st)).unwrap(),
            st
        );
    }
}
