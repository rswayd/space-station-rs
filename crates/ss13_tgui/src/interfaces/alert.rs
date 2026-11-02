//! `AlertModal` (tg `tgui_alert`): `tgui/packages/tgui/interfaces/AlertModal.tsx` and
//! `/datum/tgui_alert` (`code/modules/tgui_input/alert.dm`). The server half is
//! [`crate::prompt`].
//!
//! Acts: `choose` with `choice` (one of `buttons`), `cancel`.

use serde::{Deserialize, Serialize};

use crate::UiInterface;

pub struct AlertUi;

/// `ui_static_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct AlertStatic {
    pub message: String,
    pub buttons: Vec<String>,
    pub autofocus: bool,
    pub large_buttons: bool,
    pub swapped_buttons: bool,
}

/// `ui_data` of every input modal: the share of the timeout left, `None` without one.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub struct PromptTimer {
    pub timeout: Option<f32>,
}

impl UiInterface for AlertUi {
    const NAME: &'static str = "AlertModal";
    type Data = PromptTimer;
    type StaticData = AlertStatic;
}

pub const CHOOSE: &str = "choose";
pub const SUBMIT: &str = "submit";
pub const CANCEL: &str = "cancel";

/// Verdana 12px averages ~7 px per glyph; tgui measures the text on a canvas.
const GLYPH_PX: f32 = 7.0;
const GLYPH_PX_LARGE: f32 = 8.2;

/// Whether a button label does not fit its share of the row (tgui `isVerbose`): then the
/// buttons are stacked.
pub fn is_verbose(data: &AlertStatic) -> bool {
    let n = data.buttons.len().max(1) as f32;
    let width = window_width(data);
    let glyph = if data.large_buttons {
        GLYPH_PX_LARGE
    } else {
        GLYPH_PX
    };
    let room = width / n - (67.0 / n + 23.0);
    data.buttons
        .iter()
        .any(|b| b.chars().count() as f32 * glyph > room)
}

/// `windowWidth` and `windowHeight` of `AlertModal.tsx`.
pub fn window_size(data: &AlertStatic) -> (f32, f32) {
    let len = data.message.chars().count();
    let verbose = is_verbose(data);
    let spacing = if verbose && data.large_buttons {
        20.0
    } else {
        15.0
    };
    let height = 120.0
        + if verbose {
            spacing * data.buttons.len() as f32
        } else {
            0.0
        }
        + if len > 30 {
            (len as f32 / 4.0).ceil()
        } else {
            0.0
        }
        + if len > 0 && data.large_buttons {
            5.0
        } else {
            0.0
        };
    (window_width(data), height)
}

fn window_width(data: &AlertStatic) -> f32 {
    345.0 + if data.buttons.len() > 2 { 55.0 } else { 0.0 }
}

/// The modal's top bar (tgui `Loader`): a strip that shrinks as the timeout runs out.
#[cfg(feature = "client")]
pub(crate) fn loader(timer: &PromptTimer) -> Option<Box<dyn bevy::scene::SceneList>> {
    use bevy::scene::EntityScene;
    use ss13_ui_kit::prelude::*;
    let left = timer.timeout?.clamp(0.0, 1.0);
    Some(Box::new(EntityScene(bsn! {
        Node {
            position_type: PositionType::Absolute,
            top: px(0),
            left: px(0),
            width: percent(100),
            height: px(4),
        }
        BackgroundColor({ss13_ui_kit::theme::COLOR_BASE_END})
        Children [
            Node { width: {percent(left * 100.0)}, height: percent(100) }
            BackgroundColor({ss13_ui_kit::theme::UiColor::Green.base()})
        ]
    })))
}

#[cfg(feature = "client")]
mod client_side {
    use bevy::input::keyboard::KeyCode;
    use ss13_ui_kit::ActParams;
    use ss13_ui_kit::prelude::*;
    use ss13_ui_kit::theme::UiColor;

    use super::*;
    use crate::client::{ActOnKey, AutoFocusWindow, ClientUi, KeySetLocal, UiLocalState};

    impl ClientUi for AlertUi {
        const WIDTH: f32 = 345.0;
        const HEIGHT: f32 = 120.0;

        fn size(st: &AlertStatic, _: &PromptTimer) -> (f32, f32) {
            window_size(st)
        }

        fn content(st: &AlertStatic, data: &PromptTimer) -> impl Scene {
            view(st, data, &UiLocalState::default())
        }

        fn content_local(st: &AlertStatic, data: &PromptTimer, local: &UiLocalState) -> impl Scene {
            view(st, data, local)
        }
    }

    fn view(st: &AlertStatic, data: &PromptTimer, local: &UiLocalState) -> impl Scene + use<> {
        {
            let n = st.buttons.len().max(1);
            let selected = local.get("selected").parse::<usize>().unwrap_or(0) % n;
            let verbose = is_verbose(st);
            let message = st.message.clone();
            let large = st.large_buttons;
            let buttons: Vec<_> = st
                .buttons
                .iter()
                .enumerate()
                .map(|(i, label)| {
                    let params = ActParams(vec![("choice".into(), label.clone())]);
                    let label = if large {
                        label.to_uppercase()
                    } else {
                        label.clone()
                    };
                    let name = Name::new(format!("alert button {}", i + 1));
                    let grow: f32 = if large || verbose { 1.0 } else { 0.0 };
                    let fluid = large || verbose;
                    let is_selected = i == selected;
                    bsn! {
                        @StackItem { @grow: grow }
                        Children [
                            @ss13_ui_kit::Button { @content: label, @fluid: fluid }
                            Button { selected: is_selected }
                            Act(CHOOSE)
                            params
                            name
                        ]
                    }
                })
                .collect();
            let choice = st.buttons.get(selected).cloned().unwrap_or_default();
            let next = ((selected + 1) % n).to_string();
            let prev = ((selected + n - 1) % n).to_string();
            let keys = ActOnKey(vec![
                (
                    KeyCode::Enter,
                    CHOOSE.into(),
                    vec![("choice".into(), choice.clone())],
                ),
                (
                    KeyCode::Space,
                    CHOOSE.into(),
                    vec![("choice".into(), choice)],
                ),
                (KeyCode::Escape, CANCEL.into(), Vec::new()),
            ]);
            let move_keys = vec![
                (KeyCode::ArrowRight, next.clone()),
                (KeyCode::Tab, next),
                (KeyCode::ArrowLeft, prev),
            ];
            let autofocus = AutoFocusWindow(st.autofocus);
            let reverse = !st.swapped_buttons;
            let loader = loader(data);
            bsn! {
                @WindowContent
                keys
                KeySetLocal { key: {"selected".to_owned()}, keys: move_keys }
                autofocus
                Children [
                    {loader}
                    --
                    @Section { @fill: true }
                    Children [
                        @Stack { @vertical: true, @fill: true }
                        Children [
                            @StackItem
                            Node { margin: {UiRect::all(px(6))} }
                            Children [ @UiBox { @color: UiColor::Label } Children [ Text(message) ] ]
                            --
                            @StackItem { @grow: 1.0 }
                            Children [
                                // `reverse={!swapped_buttons}`: the first button is on the right.
                                @Stack {
                                    @vertical: verbose,
                                    @fill: true,
                                    @reverse: reverse,
                                    @justify: {Some(JustifyContent::SpaceAround)},
                                }
                                Children [ {buttons} ]
                            ]
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
    fn short_buttons_stay_in_a_row_long_ones_stack() {
        let mut data = AlertStatic {
            message: "Вы уверены?".into(),
            buttons: vec!["Да".into(), "Нет".into()],
            ..Default::default()
        };
        assert!(!is_verbose(&data));
        assert_eq!(window_size(&data), (345.0, 120.0));
        data.buttons[0] = "Очень длинная надпись на кнопке".into();
        assert!(is_verbose(&data));
        assert_eq!(window_size(&data).1, 150.0);
    }
}
