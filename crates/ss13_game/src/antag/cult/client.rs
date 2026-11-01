//! Client half of the blood cult: the `AntagInfoGeneric` panel and the `TextInputModal`
//! (Communion, rune keywords).
// shortcut: no character counter, timeout bar or large-button preference in the text input.

use bevy::app::Propagate;
use ss13_tgui::client::{AppTguiClientExt, ClientUi};
use ss13_ui_kit::prelude::*;
use ss13_ui_kit::theme::{Bold, FontPx};

use super::{AntagInfoGenericUi, CANCEL, SUBMIT, TextInputData, TextInputUi};
use crate::antag::BrotherInfo;
use crate::modpc::client::ActOnSubmit;

pub struct CultClientPlugin;

impl Plugin for CultClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_view::<AntagInfoGenericUi>()
            .register_ui_view::<TextInputUi>();
    }
}

/// tg `AntagInfoGeneric.tsx`: "You are the {antag_name}!" and `ObjectivePrintout`.
impl ClientUi for AntagInfoGenericUi {
    const WIDTH: f32 = 620.0;
    const HEIGHT: f32 = 250.0;

    fn content(info: &BrotherInfo, _: &()) -> impl Scene {
        let title = format!("You are the {}!", info.antag_name);
        let mut lines: Vec<String> = info
            .objectives
            .iter()
            .map(|o| format!("#{}: {}", o.count, o.explanation))
            .collect();
        if lines.is_empty() {
            lines.push("Ни одной!".to_owned());
        }
        let objectives: Vec<_> = lines
            .into_iter()
            .map(|line| bsn! { Node Children [ Text(line) ] })
            .collect();
        let red = UiColor::Red.text().unwrap_or_default();
        bsn! {
            @WindowContent
            Children [
                @Section { @fill: true, @scrollable: true }
                Children [
                    Node { flex_direction: FlexDirection::Column, row_gap: px(6) }
                    Children [
                        Node
                        Propagate::<FontPx>(FontPx(20.0))
                        Propagate::<TextColor>(TextColor(red))
                        Name("antag info: title")
                        Children [ Text(title) ]
                        --
                        Node Propagate::<Bold>(Bold(true))
                        Children [ Text("Ваши текущие задачи:") ]
                        --
                        Node { flex_direction: FlexDirection::Column }
                        Children [ {objectives} ]
                    ]
                ]
            ]
        }
    }
}

/// tg `TextInputModal.tsx`: the message, the field (Enter submits) and the buttons.
impl ClientUi for TextInputUi {
    const WIDTH: f32 = 325.0;
    const HEIGHT: f32 = 135.0;

    fn content(_: &(), data: &TextInputData) -> impl Scene {
        let message = data.message.clone();
        let max = (data.max_length > 0).then_some(data.max_length as usize);
        let placeholder = data.placeholder.clone();
        bsn! {
            @WindowContent
            Children [
                @Stack { @vertical: true, @fill: true }
                Children [
                    @StackItem
                    Children [ @UiBox Children [ Text(message) ] ]
                    --
                    @StackItem
                    Children [
                        Input {
                            placeholder: placeholder,
                            max_length: max,
                            auto_focus: true,
                            fluid: true,
                        }
                        ActOnSubmit {
                            action: {SUBMIT.to_owned()},
                            param: {"entry".to_owned()},
                        }
                        Name("text input: field")
                    ]
                    --
                    @StackItem
                    Children [
                        @ss13_ui_kit::Button { @content: "Отмена", @fluid: true }
                        Act(CANCEL)
                    ]
                ]
            ]
        }
    }
}
