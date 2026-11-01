//! `ListInputWindow` (tg `tgui_input_list`): `tgui/packages/tgui/interfaces/ListInputWindow` and
//! `/datum/tgui_list_input` (`code/modules/tgui_input/list.dm`). The server half is whoever asks
//! (`ss13_game::medical::surgery` for the operation pick, tg's radial menu); this module has the
//! data and the client view.
//!
//! Acts: `submit` with `entry` (one of `items`), `cancel`.
// shortcut: no search box, keyboard selection, timeout bar or large/swapped button preferences.

use serde::{Deserialize, Serialize};

use crate::UiInterface;

pub struct ListInputUi;

/// `ui_static_data` and `ui_data` in one (the list does not change while it is open).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct ListInputData {
    pub title: String,
    pub message: String,
    pub init_value: String,
    pub items: Vec<String>,
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
    use bevy::scene::EntityScene;
    use ss13_ui_kit::ActParams;
    use ss13_ui_kit::prelude::*;

    use super::{ListInputData, ListInputUi, window_height};
    use crate::client::ClientUi;

    impl ClientUi for ListInputUi {
        const WIDTH: f32 = 325.0;
        const HEIGHT: f32 = 325.0;

        fn size(_: &(), data: &ListInputData) -> (f32, f32) {
            (Self::WIDTH, window_height(data))
        }

        fn content(_: &(), data: &ListInputData) -> impl Scene {
            let message = (!data.message.is_empty()).then(|| {
                let text = data.message.clone();
                EntityScene(bsn! {
                    @StackItem
                    Children [ @UiBox Children [ Text(text) ] ]
                })
            });
            let items: Vec<_> = data
                .items
                .iter()
                .enumerate()
                .map(|(i, item)| {
                    let entry = ActParams(vec![("entry".into(), item.clone())]);
                    // Scripted clicks find an entry by its position (`click_ui "list item 2"`).
                    let name = Name::new(format!("list item {}", i + 1));
                    let label = item.clone();
                    let selected = *item == data.init_value;
                    bsn! {
                        @ss13_ui_kit::Button { @content: label, @fluid: true }
                        Button { selected: selected }
                        Act("submit")
                        entry
                        name
                    }
                })
                .collect();
            bsn! {
                @WindowContent
                Children [
                    @Stack { @vertical: true, @fill: true }
                    Children [
                        {message}
                        --
                        @StackItem { @grow: 1.0 }
                        Children [
                            @Section { @fill: true, @scrollable: true }
                            Children [ {items} ]
                        ]
                        --
                        @StackItem
                        Children [
                            @ss13_ui_kit::Button { @content: "Отмена", @fluid: true }
                            Act("cancel")
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
    fn data_round_trips_and_sizes() {
        let data = ListInputData {
            title: "Операция".into(),
            message: "abc".into(),
            init_value: "a".into(),
            items: vec!["a".into(), "b".into()],
        };
        let back: ListInputData = crate::decode(&crate::encode(&data)).unwrap();
        assert_eq!(back, data);
        assert_eq!(window_height(&data), 326.0);
    }
}
