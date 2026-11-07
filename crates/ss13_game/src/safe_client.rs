//! The mechanical safe's window (client): tgui `Safe.jsx`. The data is `crate::safe_ui`, the
//! server half `crate::safes`.
// shortcut: plain buttons and a text list instead of tgui's dial picture and item sprites.

use ss13_tgui::client::{AppTguiClientExt, ClientUi};
use ss13_ui_kit::ActParams;
use ss13_ui_kit::prelude::*;

use crate::safe_ui::*;

pub struct SafeClientPlugin;

impl Plugin for SafeClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_view::<SafeUi>()
            .register_ui_view::<LockedSafeUi>();
    }
}

impl ClientUi for SafeUi {
    const WIDTH: f32 = 625.0;
    const HEIGHT: f32 = 340.0;

    fn content(_: &(), data: &SafeData) -> impl Scene {
        safe_view(data)
    }
}

fn button(label: String, action: &str, params: Vec<(String, String)>) -> Box<dyn Scene> {
    let action = action.to_owned();
    let params = ActParams(params);
    Box::new(bsn! {
        @Button { @content: label }
        Act(action)
        params
    })
}

fn text(t: String) -> Box<dyn Scene> {
    Box::new(bsn! { Text(t) })
}

fn safe_view(d: &SafeData) -> impl Scene + use<> {
    let mut rows: Vec<Box<dyn Scene>> = Vec::new();
    let state = if d.broken {
        "Механизм сломан"
    } else if d.locked {
        "Заперт"
    } else {
        "Не заперт"
    };
    rows.push(text(format!("Диск: {} — {state}", d.dial)));
    rows.push(button(
        if d.open {
            "Закрыть"
        } else {
            "Открыть"
        }
        .to_owned(),
        OPEN,
        Vec::new(),
    ));
    if !d.open {
        for ticks in TICKS {
            let num = vec![("num".to_owned(), ticks.to_string())];
            rows.push(button(format!("← {ticks}"), TURN_LEFT, num.clone()));
            rows.push(button(format!("{ticks} →"), TURN_RIGHT, num));
        }
    }
    for (i, item) in d.contents.iter().enumerate() {
        let index = vec![("index".to_owned(), (i + 1).to_string())];
        rows.push(button(item.name.clone(), RETRIEVE, index));
    }
    bsn! {
        @WindowContent
        Children [
            @Section { @title: "Сейф" }
            Children [ {rows} ]
        ]
    }
}

impl ClientUi for LockedSafeUi {
    const WIDTH: f32 = 300.0;
    const HEIGHT: f32 = 400.0;

    fn content(_: &(), data: &LockedSafeData) -> impl Scene {
        keypad_view(data)
    }
}

fn keypad_view(d: &LockedSafeData) -> impl Scene + use<> {
    let state = if !d.lock_code {
        "Задайте код из 5 цифр"
    } else if d.locked {
        "Заперто"
    } else {
        "Открыто"
    };
    let mut rows: Vec<Box<dyn Scene>> = vec![text(format!("{} — {state}", d.input_code))];
    for key in KEYS {
        let digit = vec![("digit".to_owned(), key.to_owned())];
        rows.push(button(key.to_owned(), KEYPAD, digit));
    }
    bsn! {
        @WindowContent
        Children [
            @Section { @title: "Кодовый замок" }
            Children [ {rows} ]
        ]
    }
}
