//! `NumberInputModal` (tg `tgui_input_number`): `tgui/packages/tgui/interfaces/NumberInputModal.tsx`
//! and `/datum/tgui_input_number` (`code/modules/tgui_input/number.dm`). The server half is
//! [`crate::prompt`].
//!
//! Acts: `submit` with `entry` (a number), `cancel`.

use serde::{Deserialize, Serialize};

use super::alert::PromptTimer;
use crate::UiInterface;

pub struct NumberInputUi;

/// `ui_static_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct NumberInputStatic {
    pub message: String,
    pub init_value: f64,
    pub min_value: f64,
    pub max_value: f64,
    /// Whole numbers only (`round_value`).
    pub round_value: bool,
    pub large_buttons: bool,
    pub swapped_buttons: bool,
}

impl UiInterface for NumberInputUi {
    const NAME: &'static str = "NumberInputModal";
    type Data = PromptTimer;
    type StaticData = NumberInputStatic;
}

/// `windowHeight` of `NumberInputModal.tsx`.
pub fn window_height(data: &NumberInputStatic) -> f32 {
    let len = data.message.chars().count();
    140.0
        + if len > 30 {
            (len as f32 / 3.0).ceil()
        } else {
            0.0
        }
        + if len > 0 && data.large_buttons {
            5.0
        } else {
            0.0
        }
}

/// A number as the field shows it: no trailing `.0`.
pub fn format_number(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}

/// What `RestrictedInput` accepts: a finite number in range, whole when `round_value`.
pub fn parse_entry(text: &str, data: &NumberInputStatic) -> Option<f64> {
    let value = text.trim().replace(',', ".").parse::<f64>().ok()?;
    let ok = value.is_finite()
        && (!data.round_value || value.fract() == 0.0)
        && (data.min_value..=data.max_value).contains(&value);
    ok.then_some(value)
}

#[cfg(feature = "client")]
mod client_side {
    use bevy::input::keyboard::KeyCode;
    use ss13_ui_kit::prelude::*;
    use ss13_ui_kit::theme::UiColor;

    use super::*;
    use crate::client::{ActOnKey, ActOnSubmit, ClientUi, LocalOnInput, SetLocal, UiLocalState};
    use crate::interfaces::alert::{CANCEL, SUBMIT, loader};
    use crate::interfaces::input_buttons::{self, ENTRY};

    /// A step button: sets the field to `to`.
    fn step(icon: &str, to: f64, disabled: bool, tooltip: String) -> impl Scene + use<> {
        let icon = icon.to_owned();
        let setter = SetLocal {
            key: ENTRY.into(),
            value: format_number(to),
            rerender: true,
        };
        bsn! {
            @StackItem
            Children [
                @ss13_ui_kit::Button { @icon: icon, @tooltip: tooltip }
                Button { disabled: disabled }
                setter
            ]
        }
    }

    impl ClientUi for NumberInputUi {
        const WIDTH: f32 = 270.0;
        const HEIGHT: f32 = 140.0;

        fn size(st: &NumberInputStatic, _: &PromptTimer) -> (f32, f32) {
            (Self::WIDTH, window_height(st))
        }

        fn content(st: &NumberInputStatic, data: &PromptTimer) -> impl Scene {
            view(st, data, &UiLocalState::default())
        }

        fn content_local(
            st: &NumberInputStatic,
            data: &PromptTimer,
            local: &UiLocalState,
        ) -> impl Scene {
            view(st, data, local)
        }
    }

    fn view(
        st: &NumberInputStatic,
        data: &PromptTimer,
        local: &UiLocalState,
    ) -> impl Scene + use<> {
        {
            let initial = format_number(st.init_value);
            let text = local.value(ENTRY).unwrap_or(&initial).to_owned();
            let value = parse_entry(&text, st);
            let current = value.unwrap_or(st.init_value);
            let (min, max) = (st.min_value, st.max_value);
            let steps = vec![
                step(
                    "angle-double-left",
                    min,
                    current == min,
                    format!("Min ({})", format_number(min)),
                ),
                step(
                    "angle-down",
                    (current - 1.0).max(min),
                    current <= min,
                    String::new(),
                ),
            ];
            let more = vec![
                step(
                    "angle-up",
                    (current + 1.0).min(max),
                    current >= max,
                    String::new(),
                ),
                step(
                    "angle-double-right",
                    max,
                    current == max,
                    format!("Max ({})", format_number(max)),
                ),
                step(
                    "redo",
                    st.init_value,
                    current == st.init_value,
                    format!("Reset ({initial})"),
                ),
            ];
            let message = st.message.clone();
            let buttons = input_buttons::buttons(
                st.large_buttons,
                st.swapped_buttons,
                String::new(),
                &initial,
                value.is_none(),
            );
            let loader = loader(data);
            bsn! {
                @WindowContent
                ActOnKey(vec![(KeyCode::Escape, CANCEL.into(), Vec::new())])
                Children [
                    {loader}
                    --
                    @Section { @fill: true }
                    Children [
                        @Stack { @vertical: true, @fill: true }
                        Children [
                            @StackItem { @grow: 1.0 }
                            Children [ @UiBox { @color: UiColor::Label } Children [ Text(message) ] ]
                            --
                            @StackItem
                            Children [
                                @Stack { @fill: true }
                                Children [
                                    {steps}
                                    --
                                    @StackItem { @grow: 1.0 }
                                    Children [
                                        Input { value: text, auto_select: true, fluid: true }
                                        LocalOnInput({ENTRY.to_owned()})
                                        ActOnSubmit {
                                            action: {SUBMIT.to_owned()},
                                            param: {ENTRY.to_owned()},
                                        }
                                        Name("number input: field")
                                    ]
                                    --
                                    {more}
                                ]
                            ]
                            --
                            @StackItem
                            Children [ {buttons} ]
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
    fn entry_is_checked_against_range_and_rounding() {
        let data = NumberInputStatic {
            min_value: 0.0,
            max_value: 100.0,
            round_value: true,
            ..Default::default()
        };
        assert_eq!(parse_entry(" 42 ", &data), Some(42.0));
        assert_eq!(parse_entry("4.5", &data), None);
        assert_eq!(parse_entry("101", &data), None);
        assert_eq!(parse_entry("NaN", &data), None);
        assert_eq!(format_number(42.0), "42");
        assert_eq!(format_number(0.5), "0.5");
    }
}
