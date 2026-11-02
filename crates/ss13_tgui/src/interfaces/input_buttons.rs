//! tg `interfaces/common/InputButtons.tsx`: "Отмена" and "Готово" under an input modal, with an
//! optional label between them. "Готово" sends `submit` with the window's local `entry`.

use bevy::scene::{EntityScene, SceneList};
use ss13_ui_kit::prelude::*;
use ss13_ui_kit::theme::UiColor;

use super::alert::{CANCEL, SUBMIT};
use crate::client::ActWithLocal;

/// The [`crate::client::UiLocalState`] key of the edited value, also the act param.
pub const ENTRY: &str = "entry";

/// `default` is sent when the field was never edited.
pub fn buttons(
    large: bool,
    swapped: bool,
    message: String,
    default: &str,
    disabled: bool,
) -> Box<dyn SceneList> {
    let (submit, cancel) = if large {
        ("ГОТОВО", "ОТМЕНА")
    } else {
        ("Готово", "Отмена")
    };
    let grow: f32 = if large { 1.0 } else { 0.0 };
    let width = if large { Val::Auto } else { px(72) };
    let with_local = ActWithLocal {
        param: ENTRY.into(),
        key: ENTRY.into(),
        default: default.to_owned(),
    };
    let submit = EntityScene(bsn! {
        @StackItem { @grow: grow }
        Children [
            @ss13_ui_kit::Button { @content: submit, @fluid: large }
            Button { color: UiColor::Good, disabled: disabled }
            Node { width: width, justify_content: JustifyContent::Center }
            Act(SUBMIT)
            with_local
            Name("input: submit")
        ]
    });
    let cancel = EntityScene(bsn! {
        @StackItem { @grow: grow }
        Children [
            @ss13_ui_kit::Button { @content: cancel, @fluid: large }
            Button { color: UiColor::Bad }
            Node { width: width, justify_content: JustifyContent::Center }
            Act(CANCEL)
            Name("input: cancel")
        ]
    });
    let label = (!large && !message.is_empty()).then(|| {
        EntityScene(bsn! {
            @StackItem
            Children [ @UiBox { @color: UiColor::Label } Children [ Text(message) ] ]
        })
    });
    // `direction={!swapped_buttons ? 'row' : 'row-reverse'}`: cancel left, submit right.
    Box::new(EntityScene(bsn! {
        @Stack {
            @fill: true,
            @reverse: swapped,
            @justify: {Some(JustifyContent::SpaceAround)},
            @align: {Some(AlignItems::Center)},
        }
        Children [ {cancel} -- {label} -- {submit} ]
    }))
}
