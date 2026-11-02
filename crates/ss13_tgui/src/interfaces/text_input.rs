//! `TextInputModal` (tg `tgui_input_text`): `tgui/packages/tgui/interfaces/TextInputModal.tsx`
//! and `/datum/tgui_input_text` (`code/modules/tgui_input/text.dm`). The server half is
//! [`crate::prompt`].
//!
//! Acts: `submit` with `entry` (split into `entry.1`... when long), `cancel`.

use serde::{Deserialize, Serialize};

use super::alert::PromptTimer;
use crate::UiInterface;

pub struct TextInputUi;

/// `ui_static_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct TextInputStatic {
    pub message: String,
    /// The text the field starts with (tg passes `default` as `placeholder`).
    pub placeholder: String,
    /// In characters; `0` is no limit.
    pub max_length: u32,
    pub multiline: bool,
    pub large_buttons: bool,
    pub swapped_buttons: bool,
}

impl UiInterface for TextInputUi {
    const NAME: &'static str = "TextInputModal";
    type Data = PromptTimer;
    type StaticData = TextInputStatic;
}

/// `windowHeight` of `TextInputModal.tsx` (on open: tgui grows it as the text gets long).
pub fn window_height(data: &TextInputStatic) -> f32 {
    let len = data.message.chars().count();
    let lines = data.message.lines().count().max(1);
    let multiline = data.multiline || data.placeholder.chars().count() >= 30;
    135.0
        + if len > 30 {
            (len as f32 / 4.0).ceil()
        } else {
            0.0
        }
        + (lines - 1) as f32 * 12.0
        + if multiline { 75.0 } else { 0.0 }
        + if len > 0 && data.large_buttons {
            5.0
        } else {
            0.0
        }
}

#[cfg(feature = "client")]
mod client_side {
    use bevy::input::keyboard::KeyCode;
    use bevy::scene::{EntityScene, SceneList};
    use ss13_ui_kit::prelude::*;
    use ss13_ui_kit::theme::UiColor;

    use super::*;
    use crate::client::{ActOnKey, ActOnSubmit, ClientUi, LocalOnInput, UiLocalState};
    use crate::interfaces::alert::{CANCEL, SUBMIT, loader};
    use crate::interfaces::input_buttons::{self, ENTRY};

    /// The field: an `Input`, plus `TextArea` for `multiline` (Enter submits, Shift+Enter breaks).
    fn text_field(value: String, max: Option<usize>, multiline: bool) -> Box<dyn SceneList> {
        let submit = ActOnSubmit {
            action: SUBMIT.into(),
            param: ENTRY.into(),
            params: Vec::new(),
        };
        let field = bsn! {
            Input {
                value: value,
                placeholder: {"Напечатайте что-нибудь...".to_owned()},
                max_length: max,
                auto_select: true,
                fluid: true,
            }
            LocalOnInput({ENTRY.to_owned()})
            submit
            Name("text input: field")
        };
        if multiline {
            Box::new(EntityScene((field, bsn! { TextArea { rows: 4.0 } })))
        } else {
            Box::new(EntityScene(field))
        }
    }

    impl ClientUi for TextInputUi {
        const WIDTH: f32 = 325.0;
        const HEIGHT: f32 = 135.0;

        fn size(st: &TextInputStatic, _: &PromptTimer) -> (f32, f32) {
            (Self::WIDTH, window_height(st))
        }

        fn content(st: &TextInputStatic, data: &PromptTimer) -> impl Scene {
            view(st, data, &UiLocalState::default())
        }

        fn content_local(
            st: &TextInputStatic,
            data: &PromptTimer,
            local: &UiLocalState,
        ) -> impl Scene {
            view(st, data, local)
        }
    }

    fn view(st: &TextInputStatic, data: &PromptTimer, local: &UiLocalState) -> impl Scene + use<> {
        {
            let message = st.message.clone();
            let max = (st.max_length > 0).then_some(st.max_length as usize);
            let value = local.value(ENTRY).unwrap_or(&st.placeholder).to_owned();
            let count = format!(
                "{}/{}",
                value.chars().count(),
                max.map_or("∞".to_owned(), |m| m.to_string())
            );
            let field = text_field(value, max, st.multiline);
            let grow: f32 = if st.multiline { 1.0 } else { 0.0 };
            let buttons = input_buttons::buttons(
                st.large_buttons,
                st.swapped_buttons,
                count,
                &st.placeholder,
                false,
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
                            @StackItem
                            Children [ @UiBox { @color: UiColor::Label } Children [ Text(message) ] ]
                            --
                            @StackItem { @grow: grow }
                            Children [
                                {field}
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
    fn height_grows_with_message_and_multiline() {
        let mut data = TextInputStatic::default();
        assert_eq!(window_height(&data), 135.0);
        data.message = "а".repeat(40);
        data.multiline = true;
        assert_eq!(window_height(&data), 135.0 + 10.0 + 75.0);
    }
}
