//! `KeyComboModal` (tg `tgui_input_keycombo`): `tgui/packages/tgui/interfaces/KeyComboModal.tsx`
//! and `/datum/tgui_input_keycombo` (`code/modules/tgui_input/keycombo.dm`). The server half is
//! [`crate::prompt`].
//!
//! Acts: `submit` with `entry` (a BYOND key name with its modifiers, `CtrlShiftA`), `cancel`.

use serde::{Deserialize, Serialize};

use super::alert::PromptTimer;
use crate::UiInterface;

pub struct KeyComboUi;

/// `ui_static_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct KeyComboStatic {
    pub message: String,
    pub init_value: String,
    pub large_buttons: bool,
    pub swapped_buttons: bool,
}

impl UiInterface for KeyComboUi {
    const NAME: &'static str = "KeyComboModal";
    type Data = PromptTimer;
    type StaticData = KeyComboStatic;
}

/// Longest combo the server takes: `AltCtrlShiftNumpadSoutheast` is 27.
pub const MAX_COMBO_LEN: usize = 32;

/// `windowHeight` of `KeyComboModal.tsx`.
pub fn window_height(data: &KeyComboStatic) -> f32 {
    let len = data.message.chars().count();
    130.0
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

#[cfg(feature = "client")]
pub(crate) mod client_side {
    use bevy::input::ButtonState;
    use bevy::input::keyboard::{KeyCode, KeyboardInput};
    use bevy::input_focus::FocusedInput;
    use ss13_ui_kit::prelude::*;
    use ss13_ui_kit::theme::UiColor;

    use super::*;
    use crate::client::{ActOnKey, AutoFocusWindow, ClientUi, ClientWindow, UiLocalState};
    use crate::interfaces::alert::{CANCEL, SUBMIT, loader};
    use crate::interfaces::input_buttons::{self, ENTRY};

    /// Local key: `"done"` once a key was caught; unset while waiting for one.
    const BOUND: &str = "bound";

    /// On the view root: catches the next key press while waiting.
    #[derive(Component, Clone, Debug, Default, PartialEq)]
    pub struct KeyComboCapture {
        pub init_value: String,
    }

    /// tgui `KEY_CODE_TO_BYOND` plus the key names BYOND macros use.
    fn byond_key(code: KeyCode) -> Option<String> {
        use KeyCode::*;
        let name = match code {
            Delete => "Delete",
            ArrowDown => "South",
            End => "Southwest",
            Home => "Northwest",
            Insert => "Insert",
            ArrowLeft => "West",
            PageDown => "Southeast",
            PageUp => "Northeast",
            ArrowRight => "East",
            Space => "Space",
            ArrowUp => "North",
            Enter | NumpadEnter => "Return",
            Tab => "Tab",
            Backspace => "Back",
            Backquote => "`",
            Minus | NumpadSubtract => "-",
            Equal => "=",
            BracketLeft => "[",
            BracketRight => "]",
            Backslash => "\\",
            Semicolon => ";",
            Quote => "'",
            Comma => ",",
            Period | NumpadDecimal => ".",
            Slash | NumpadDivide => "/",
            NumpadMultiply => "*",
            NumpadAdd => "+",
            _ => {
                let debug = format!("{code:?}");
                let short = debug
                    .strip_prefix("Key")
                    .or_else(|| debug.strip_prefix("Digit"))
                    .or_else(|| debug.strip_prefix("Numpad"))
                    .unwrap_or(&debug);
                let is_function = short.len() <= 3
                    && short.starts_with('F')
                    && short[1..].chars().all(|c| c.is_ascii_digit());
                if short.len() == 1 || is_function {
                    return Some(short.to_owned());
                }
                return None;
            }
        };
        Some(name.to_owned())
    }

    fn is_numpad(code: KeyCode) -> bool {
        format!("{code:?}").starts_with("Numpad")
    }

    /// `formatKeyboardEvent`: modifiers in tg's order, then the key.
    pub fn format_combo(code: KeyCode, alt: bool, ctrl: bool, shift: bool) -> Option<String> {
        let key = byond_key(code)?;
        let mut text = String::new();
        for (on, name) in [(alt, "Alt"), (ctrl, "Ctrl"), (shift, "Shift")] {
            if on {
                text.push_str(name);
            }
        }
        if is_numpad(code) {
            text.push_str("Numpad");
        }
        text.push_str(&key);
        Some(text)
    }

    pub(crate) fn capture_combo(
        key: On<FocusedInput<KeyboardInput>>,
        parents: Query<&ChildOf>,
        mut windows: Query<(&mut ClientWindow, &mut UiLocalState)>,
        children: Query<&Children>,
        captures: Query<&KeyComboCapture>,
        keys: Res<ButtonInput<KeyCode>>,
    ) {
        if key.event_target() != key.focused_entity || key.input.state != ButtonState::Pressed {
            return;
        }
        let Some(root) = std::iter::once(key.focused_entity)
            .chain(parents.iter_ancestors(key.focused_entity))
            .find(|e| windows.contains(*e))
        else {
            return;
        };
        let Ok((mut window, mut local)) = windows.get_mut(root) else {
            return;
        };
        let Some(capture) = window
            .content_root(&children)
            .and_then(|r| captures.get(r).ok())
        else {
            return;
        };
        if local.value(BOUND).is_some() {
            return;
        }
        let code = key.input.key_code;
        let modifier = matches!(
            code,
            KeyCode::AltLeft
                | KeyCode::AltRight
                | KeyCode::ControlLeft
                | KeyCode::ControlRight
                | KeyCode::ShiftLeft
                | KeyCode::ShiftRight
        );
        if modifier {
            return;
        }
        let combo = if code == KeyCode::Escape {
            capture.init_value.clone()
        } else {
            let alt = keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]);
            let ctrl = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
            let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
            let Some(combo) = format_combo(code, alt, ctrl, shift) else {
                return;
            };
            combo
        };
        local.set(ENTRY, &combo);
        local.set(BOUND, "done");
        window.mark_dirty();
    }

    impl ClientUi for KeyComboUi {
        const WIDTH: f32 = 240.0;
        const HEIGHT: f32 = 130.0;

        fn size(st: &KeyComboStatic, _: &PromptTimer) -> (f32, f32) {
            (Self::WIDTH, window_height(st))
        }

        fn content(st: &KeyComboStatic, data: &PromptTimer) -> impl Scene {
            view(st, data, &UiLocalState::default())
        }

        fn content_local(
            st: &KeyComboStatic,
            data: &PromptTimer,
            local: &UiLocalState,
        ) -> impl Scene {
            view(st, data, local)
        }
    }

    fn view(st: &KeyComboStatic, data: &PromptTimer, local: &UiLocalState) -> impl Scene + use<> {
        {
            let binding = local.value(BOUND).is_none();
            let entry = local.value(ENTRY).unwrap_or(&st.init_value).to_owned();
            let label = if binding {
                "Ожидание ввода...".to_owned()
            } else {
                entry.clone()
            };
            // Waiting: every key is caught. Bound: Enter submits, Escape cancels.
            let keys = if binding {
                ActOnKey::default()
            } else {
                ActOnKey(vec![
                    (
                        KeyCode::Enter,
                        SUBMIT.into(),
                        vec![(ENTRY.into(), entry.clone())],
                    ),
                    (KeyCode::Escape, CANCEL.into(), Vec::new()),
                ])
            };
            let rebind = crate::client::ClearLocal(vec![BOUND.into(), ENTRY.into()]);
            let capture = KeyComboCapture {
                init_value: st.init_value.clone(),
            };
            let message = st.message.clone();
            let buttons = input_buttons::buttons(
                st.large_buttons,
                st.swapped_buttons,
                String::new(),
                &st.init_value,
                binding,
            );
            let loader = loader(data);
            bsn! {
                @WindowContent
                keys
                capture
                AutoFocusWindow(true)
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
                                @ss13_ui_kit::Button { @content: label, @fluid: true }
                                Button { disabled: binding }
                                rebind
                                Name("key combo: rebind")
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

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn combos_read_like_byond_macros() {
            assert_eq!(
                format_combo(KeyCode::KeyA, false, true, true).as_deref(),
                Some("CtrlShiftA")
            );
            assert_eq!(
                format_combo(KeyCode::ArrowUp, true, false, false).as_deref(),
                Some("AltNorth")
            );
            assert_eq!(
                format_combo(KeyCode::Numpad8, false, false, false).as_deref(),
                Some("Numpad8")
            );
            assert_eq!(
                format_combo(KeyCode::F12, false, false, false).as_deref(),
                Some("F12")
            );
            assert_eq!(
                format_combo(KeyCode::Digit3, false, false, false).as_deref(),
                Some("3")
            );
            assert_eq!(format_combo(KeyCode::Pause, false, false, false), None);
        }
    }
}
