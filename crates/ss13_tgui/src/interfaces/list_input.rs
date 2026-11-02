//! `ListInputWindow` (tg `tgui_input_list`): `tgui/packages/tgui/interfaces/ListInputWindow` and
//! `/datum/tgui_list_input` (`code/modules/tgui_input/list.dm`). The server half is whoever asks
//! (`crate::prompt` for `tgui_input_list`, `ss13_game::medical::surgery` for the operation pick, tg's
//! radial menu); this module has the data and the client view.
//!
//! Acts: `submit` with `entry` (one of `items`), `cancel`. A click selects, "Готово" or Enter
//! submits the selection, as in tgui.

use serde::{Deserialize, Serialize};

use crate::UiInterface;

pub struct ListInputUi;

/// `ui_static_data` and `ui_data` in one (the list does not change while it is open).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct ListInputData {
    pub title: String,
    pub message: String,
    pub init_value: String,
    pub items: Vec<String>,
    /// [`super::alert::PromptTimer`]: the share of the timeout left.
    pub timeout: Option<f32>,
}

impl UiInterface for ListInputUi {
    const NAME: &'static str = "ListInputWindow";
    type Data = ListInputData;
    type StaticData = ();
}

/// `325 + ceil(message.length / 3)`.
pub fn window_height(data: &ListInputData) -> f32 {
    325.0 + (data.message.chars().count() as f32 / 3.0).ceil()
}

#[cfg(feature = "client")]
mod client_side {
    use bevy::input::keyboard::KeyCode;
    use bevy::scene::{EntityScene, SceneList};
    use ss13_ui_kit::prelude::*;
    use ss13_ui_kit::theme::UiColor;

    use super::{ListInputData, ListInputUi, window_height};
    use crate::client::{
        ActOnDoubleClick, ActOnKey, AutoFocusWindow, ClearLocal, ClientUi, ListNav, LocalOnInput,
        SearchRow, SelectRow, SetLocal, UiLocalState,
    };
    use crate::interfaces::alert::{CANCEL, SUBMIT, loader};
    use crate::interfaces::input_buttons::{self, ENTRY};

    const SEARCH: &str = "search";

    impl ClientUi for ListInputUi {
        const WIDTH: f32 = 325.0;
        const HEIGHT: f32 = 325.0;

        fn size(_: &(), data: &ListInputData) -> (f32, f32) {
            (Self::WIDTH, window_height(data))
        }

        fn content(_: &(), data: &ListInputData) -> impl Scene {
            view(data, &UiLocalState::default())
        }

        fn content_local(_: &(), data: &ListInputData, local: &UiLocalState) -> impl Scene {
            view(data, local)
        }
    }

    /// The search field, shown for long lists (tgui `items.length > 9`).
    fn search_bar() -> Box<dyn SceneList> {
        Box::new(EntityScene(bsn! {
            @StackItem
            Children [
                Input { placeholder: {"Search...".to_owned()}, auto_focus: true, fluid: true }
                LocalOnInput({SEARCH.to_owned()})
                Name("list input: search")
            ]
        }))
    }

    /// Local key of the search bar's visibility: unset follows the list's length.
    const SEARCH_BAR: &str = "search_bar";

    fn view(data: &ListInputData, local: &UiLocalState) -> impl Scene + use<> {
        let selected = local.value(ENTRY).unwrap_or(&data.init_value).to_owned();
        let items: Vec<_> = data
            .items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let pick = SetLocal {
                    key: ENTRY.into(),
                    value: item.clone(),
                    rerender: false,
                };
                let shown = SelectRow {
                    key: ENTRY.into(),
                    value: item.clone(),
                    default: *item == data.init_value,
                };
                let row = SearchRow {
                    key: SEARCH.into(),
                    text: item.to_lowercase(),
                };
                let double = ActOnDoubleClick {
                    action: SUBMIT.into(),
                    param: ENTRY.into(),
                    value: item.clone(),
                };
                // Scripted clicks find an entry by its position (`click_ui "list item 2"`).
                let name = Name::new(format!("list item {}", i + 1));
                // tgui capitalizes the first letter.
                let mut chars = item.chars();
                let label: String = chars
                    .next()
                    .map(|c| c.to_uppercase().chain(chars).collect())
                    .unwrap_or_default();
                let is_selected = *item == selected;
                bsn! {
                    @ss13_ui_kit::Button { @content: label, @fluid: true }
                    Button { color: UiColor::Transparent, selected: is_selected }
                    pick
                    shown
                    row
                    double
                    name
                }
            })
            .collect();
        // tgui `searchBarVisible`: on for long lists; the title button flips it and clears the
        // query, and without it letters jump through the list.
        let search_visible = match local.value(SEARCH_BAR) {
            Some(v) => v == "1",
            None => data.items.len() > 9,
        };
        let nav = ListNav {
            key: ENTRY.into(),
            default: data.init_value.clone(),
            search_key: SEARCH.into(),
            items: data.items.clone(),
            hotkeys: !search_visible,
            submit: SUBMIT.into(),
        };
        let keys = ActOnKey(vec![(KeyCode::Escape, CANCEL.into(), Vec::new())]);
        // With a search bar the field takes the keyboard instead.
        let autofocus = AutoFocusWindow(!search_visible);
        let toggle = SetLocal {
            key: SEARCH_BAR.into(),
            value: if search_visible { "0" } else { "1" }.into(),
            rerender: true,
        };
        let clear = ClearLocal(vec![SEARCH.into()]);
        let (icon, tooltip) = if search_visible {
            (
                "search",
                "Search Mode. Type to search or use arrow keys to select manually.",
            )
        } else {
            (
                "font",
                "Hotkey Mode. Type a letter to jump to the first match. Enter to select.",
            )
        };
        let toggle_button: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
            @ss13_ui_kit::Button {
                @icon: icon,
                @compact: true,
                @tooltip: tooltip,
                @tooltip_position: ss13_ui_kit::Placement::Left,
            }
            Button { selected: true }
            toggle
            clear
            Name("list input: mode")
        }));
        let message = data.message.clone();
        let search = search_visible.then(search_bar);
        let buttons = input_buttons::buttons(false, true, String::new(), &data.init_value, false);
        let loader = loader(&super::super::alert::PromptTimer {
            timeout: data.timeout,
        });
        bsn! {
            @WindowContent
            keys
            nav
            autofocus
            Children [
                {loader}
                --
                // tgui puts the list, search and buttons in one titled section; here the title is
                // the list's section's.
                @Stack { @vertical: true, @fill: true }
                Children [
                    @StackItem { @grow: 1.0 }
                    // A flex item does not shrink below its content without it (CSS scroll
                    // containers do).
                    Node { min_height: px(0), flex_direction: FlexDirection::Column }
                    Children [
                        @Section {
                            @title: message,
                            @buttons: {Some(toggle_button)},
                            @fill: true,
                            @scrollable: true,
                        }
                        Children [
                            @Stack { @vertical: true, @zebra: true }
                            Children [ {items} ]
                        ]
                    ]
                    --
                    {search}
                    --
                    @StackItem
                    Children [ {buttons} ]
                ]
            ]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_round_trips_and_sizes() {
        let data = ListInputData {
            title: "Операция".into(),
            message: "abc".into(),
            init_value: "a".into(),
            items: vec!["a".into(), "b".into()],
            timeout: None,
        };
        let back: ListInputData = crate::decode(&crate::encode(&data)).unwrap();
        assert_eq!(back, data);
        assert_eq!(window_height(&data), 326.0);
    }
}
