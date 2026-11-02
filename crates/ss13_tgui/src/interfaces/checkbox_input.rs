//! `CheckboxInput` (tg `tgui_input_checkboxes`): `tgui/packages/tgui/interfaces/CheckboxInput.tsx`
//! and `/datum/tgui_checkbox_input` (`code/modules/tgui_input/checkboxes.dm`). The server half is
//! [`crate::prompt`].
//!
//! Acts: `submit` with `entry` (the checked items' indices, comma-separated: tg sends
//! `[name, index]` pairs, params here are strings), `cancel`.

use serde::{Deserialize, Serialize};

use super::alert::PromptTimer;
use crate::UiInterface;

pub struct CheckboxInputUi;

/// `ui_static_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct CheckboxInputStatic {
    pub message: String,
    pub items: Vec<String>,
    pub min_checked: u32,
    pub max_checked: u32,
    pub large_buttons: bool,
    pub swapped_buttons: bool,
}

impl UiInterface for CheckboxInputUi {
    const NAME: &'static str = "CheckboxInput";
    type Data = PromptTimer;
    type StaticData = CheckboxInputStatic;
}

/// `0,2,5` -> `[0, 2, 5]`; `None` on anything else (duplicates, junk). Empty is no selection.
pub fn parse_indices(text: &str) -> Option<Vec<usize>> {
    if text.is_empty() {
        return Some(Vec::new());
    }
    let mut out: Vec<usize> = Vec::new();
    for part in text.split(',') {
        let i = part.parse::<usize>().ok()?;
        if out.contains(&i) {
            return None;
        }
        out.push(i);
    }
    Some(out)
}

pub fn format_indices(indices: &[usize]) -> String {
    indices
        .iter()
        .map(usize::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(feature = "client")]
mod client_side {
    use bevy::input::keyboard::KeyCode;
    use ss13_ui_kit::prelude::*;

    use super::*;
    use crate::client::{ActOnKey, ClientUi, LocalOnInput, SearchRow, SetLocal, UiLocalState};
    use crate::interfaces::alert::{CANCEL, loader};
    use crate::interfaces::input_buttons::{self, ENTRY};

    const SEARCH: &str = "search";

    impl ClientUi for CheckboxInputUi {
        const WIDTH: f32 = 425.0;
        const HEIGHT: f32 = 300.0;

        fn content(st: &CheckboxInputStatic, data: &PromptTimer) -> impl Scene {
            view(st, data, &UiLocalState::default())
        }

        fn content_local(
            st: &CheckboxInputStatic,
            data: &PromptTimer,
            local: &UiLocalState,
        ) -> impl Scene {
            view(st, data, local)
        }
    }

    fn view(
        st: &CheckboxInputStatic,
        data: &PromptTimer,
        local: &UiLocalState,
    ) -> impl Scene + use<> {
        {
            let selected = parse_indices(local.get(ENTRY)).unwrap_or_default();
            let full = selected.len() as u32 >= st.max_checked;
            let rows: Vec<_> = st
                .items
                .iter()
                .enumerate()
                .map(|(i, item)| {
                    let checked = selected.contains(&i);
                    let mut toggled = selected.clone();
                    if checked {
                        toggled.retain(|&s| s != i);
                    } else {
                        toggled.push(i);
                    }
                    let setter = SetLocal {
                        key: ENTRY.into(),
                        value: format_indices(&toggled),
                        rerender: true,
                    };
                    let icon = if checked {
                        "check-square-o"
                    } else {
                        "square-o"
                    };
                    let row = SearchRow {
                        key: SEARCH.into(),
                        text: item.to_lowercase(),
                    };
                    let label = item.clone();
                    let name = Name::new(format!("checkbox {}", i + 1));
                    let disabled = full && !checked;
                    bsn! {
                        @StackItem
                        row
                        Children [
                            @ss13_ui_kit::Button { @content: label, @icon: icon, @fluid: true }
                            Button { selected: checked, disabled: disabled }
                            setter
                            name
                        ]
                    }
                })
                .collect();
            let mut notice = st.message.clone();
            if st.min_checked > 0 {
                notice.push_str(&format!(" (Min: {})", st.min_checked));
            }
            if st.max_checked < 50 {
                notice.push_str(&format!(" (Max: {})", st.max_checked));
            }
            let count_ok = (st.min_checked..=st.max_checked).contains(&(selected.len() as u32));
            let buttons = input_buttons::buttons(
                st.large_buttons,
                st.swapped_buttons,
                String::new(),
                "",
                !count_ok,
            );
            let loader = loader(data);
            bsn! {
                @WindowContent
                ActOnKey(vec![(KeyCode::Escape, CANCEL.into(), Vec::new())])
                Children [
                    {loader}
                    --
                    @Stack { @vertical: true, @fill: true }
                    Children [
                        @StackItem
                        Children [ @NoticeBox { @info: true } Children [ Text(notice) ] ]
                        --
                        @StackItem { @grow: 1.0 }
                        // A flex item does not shrink below its content without it (CSS scroll containers do).
                        Node { min_height: px(0), flex_direction: FlexDirection::Column }
                        Children [
                            @Section { @fill: true, @scrollable: true }
                            Children [
                                @Stack { @vertical: true, @zebra: true }
                                Children [ {rows} ]
                            ]
                        ]
                        --
                        @StackItem
                        Node { margin: {UiRect::all(px(6))} }
                        Children [
                            @Stack
                            Children [
                                @StackItem
                                Children [ @Icon { @name: "search" } ]
                                --
                                @StackItem { @grow: 1.0 }
                                Children [
                                    Input { fluid: true }
                                    LocalOnInput({SEARCH.to_owned()})
                                    Name("checkbox input: search")
                                ]
                            ]
                        ]
                        --
                        @StackItem
                        Children [ @Section Children [ {buttons} ] ]
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
    fn indices_round_trip_and_reject_duplicates() {
        assert_eq!(parse_indices("0,2,5"), Some(vec![0, 2, 5]));
        assert_eq!(parse_indices(""), Some(vec![]));
        assert_eq!(parse_indices("1,1"), None);
        assert_eq!(parse_indices("a"), None);
        assert_eq!(format_indices(&[3, 1]), "3,1");
    }
}
