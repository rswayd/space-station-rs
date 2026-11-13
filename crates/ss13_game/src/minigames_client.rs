//! Client half of the ghost minigame windows ([`crate::minigames_ui`]): the `MinigamesMenu`
//! and the `BasketballPanel` lobby.
// shortcut: the menu lists only the minigames the port has (basketball); tg's Mafia, CTF and
// Deathmatch buttons come with those games.

use bevy::scene::EntityScene;
use ss13_tgui::client::{AppTguiClientExt, ClientUi};
use ss13_ui_kit::prelude::*;

use crate::minigames_ui::*;

pub struct MinigamesClientPlugin;

impl Plugin for MinigamesClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_view::<MinigamesMenuUi>()
            .register_ui_view::<BasketballPanelUi>();
    }
}

fn act_button(label: &str, act: &str, disabled: bool) -> impl Scene + use<> {
    let (label, act) = (label.to_owned(), act.to_owned());
    bsn! {
        @ss13_ui_kit::Button { @content: label, @fluid: true }
        ss13_ui_kit::Button { disabled: disabled }
        Act(act)
    }
}

impl ClientUi for MinigamesMenuUi {
    const WIDTH: f32 = 530.0;
    const HEIGHT: f32 = 320.0;

    fn content(_: &(), _: &()) -> impl Scene {
        let basketball = EntityScene(act_button("Баскетбол", MENU_BASKETBALL, false));
        bsn! {
            @WindowContent
            Children [
                @Section { @title: "Выберите мини-игру", @fill: true }
                Children [
                    Node { flex_direction: FlexDirection::Column, row_gap: px(4) }
                    Children [ {basketball} ]
                ]
            ]
        }
    }
}

impl ClientUi for BasketballPanelUi {
    const WIDTH: f32 = 650.0;
    const HEIGHT: f32 = 580.0;

    fn content(_: &(), data: &BasketballPanelData) -> impl Scene {
        let signup = EntityScene(act_button("Записаться", BASKETBALL_SIGNUP, false));
        let start = EntityScene(act_button(
            "Начать",
            BASKETBALL_START,
            data.total_votes < data.players_min,
        ));
        let notice = format!(
            "В лобби записано игроков: {}. Мини-игра рассчитана на {}–{} игроков.",
            data.total_votes, data.players_min, data.players_max
        );
        let rows: Vec<_> = data
            .lobbydata
            .iter()
            .map(|m| {
                let (status, color) = if m.disconnected {
                    ("Отключён", UiColor::Red)
                } else {
                    ("Готов", UiColor::Green)
                };
                let ckey = m.ckey.clone();
                let status = format!("Статус: {status}");
                bsn! {
                    Node { flex_direction: FlexDirection::Row, column_gap: px(8) }
                    Children [
                        Node { flex_grow: 1.0 } Children [ Text(ckey) ]
                        --
                        @NoticeBox { @color: color } Children [ Text(status) ]
                    ]
                }
            })
            .collect();
        bsn! {
            @WindowContent
            Children [
                @Section { @title: "Лобби", @fill: true, @scrollable: true }
                Children [
                    Node { flex_direction: FlexDirection::Column, row_gap: px(4) }
                    Children [
                        Node { flex_direction: FlexDirection::Row, column_gap: px(4) }
                        Children [ {signup} -- {start} ]
                        --
                        @NoticeBox Children [ Text(notice) ]
                        --
                        Node { flex_direction: FlexDirection::Column, row_gap: px(2) }
                        Children [ {rows} ]
                    ]
                ]
            ]
        }
    }
}
