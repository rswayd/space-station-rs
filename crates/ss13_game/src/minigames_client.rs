//! Client half of the ghost minigame windows ([`crate::minigames_ui`]): the `MinigamesMenu`
//! the `BasketballPanel` lobby, the `DeathmatchPanel` and `DeathmatchLobby` windows.
// shortcut: the menu lists only the minigames the port has (basketball, deathmatch); tg's Mafia
// and CTF buttons come with those games. Loadouts and maps are picked with buttons, not
// dropdowns.

use bevy::scene::{EntityScene, SceneList};
use ss13_tgui::client::{AppTguiClientExt, ClientUi};
use ss13_ui_kit::prelude::*;

use crate::minigames_ui::*;

pub struct MinigamesClientPlugin;

impl Plugin for MinigamesClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_view::<MinigamesMenuUi>()
            .register_ui_view::<BasketballPanelUi>()
            .register_ui_view::<DeathmatchPanelUi>()
            .register_ui_view::<DeathmatchLobbyUi>();
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

fn param_button(
    label: &str,
    act: &str,
    params: &[(&str, &str)],
    disabled: bool,
) -> impl Scene + use<> {
    let (label, act) = (label.to_owned(), act.to_owned());
    let params: Vec<(String, String)> = params
        .iter()
        .map(|&(k, v)| (k.to_owned(), v.to_owned()))
        .collect();
    bsn! {
        @ss13_ui_kit::Button { @content: label }
        ss13_ui_kit::Button { disabled: disabled }
        Act(act)
        ActParams(params)
    }
}

fn line(text: String) -> impl Scene + use<> {
    bsn! { Text(text) }
}

impl ClientUi for MinigamesMenuUi {
    const WIDTH: f32 = 530.0;
    const HEIGHT: f32 = 320.0;

    fn content(_: &(), _: &()) -> impl Scene {
        let basketball = EntityScene(act_button("Баскетбол", MENU_BASKETBALL, false));
        let deathmatch = EntityScene(act_button("Дезматч", MENU_DEATHMATCH, false));
        bsn! {
            @WindowContent
            Children [
                @Section { @title: "Выберите мини-игру", @fill: true }
                Children [
                    Node { flex_direction: FlexDirection::Column, row_gap: px(4) }
                    Children [ {basketball} -- {deathmatch} ]
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

impl ClientUi for DeathmatchPanelUi {
    const WIDTH: f32 = 360.0;
    const HEIGHT: f32 = 400.0;

    fn content(_: &(), data: &DeathmatchPanelData) -> impl Scene {
        let host = EntityScene(param_button("Создать лобби", DM_HOST, &[], data.hosting));
        let rows: Vec<_> = data
            .lobbies
            .iter()
            .map(|l| {
                let state = match l.playing {
                    0 => "в лобби",
                    1 => "загрузка",
                    _ => "идёт бой",
                };
                let text = format!(
                    "{} — {} ({}/{}), {}",
                    l.name, l.map, l.players, l.max_players, state
                );
                let mine = data.playing.as_deref() == Some(l.name.as_str());
                let join_label = if mine { "Открыть" } else { "Войти" };
                let join = EntityScene(param_button(
                    join_label,
                    DM_JOIN,
                    &[("id", l.name.as_str())],
                    l.playing != 0 && !mine,
                ));
                let spectate = EntityScene(param_button(
                    "Смотреть",
                    DM_SPECTATE,
                    &[("id", l.name.as_str())],
                    false,
                ));
                bsn! {
                    Node { flex_direction: FlexDirection::Row, column_gap: px(4) }
                    Children [
                        Node { flex_grow: 1.0 } Children [ Text(text) ]
                        -- {join} -- {spectate}
                    ]
                }
            })
            .collect();
        bsn! {
            @WindowContent
            Children [
                @Section { @title: "Лобби дезматча", @fill: true, @scrollable: true }
                Children [
                    Node { flex_direction: FlexDirection::Column, row_gap: px(4) }
                    Children [ {host} -- {rows} ]
                ]
            ]
        }
    }
}

impl ClientUi for DeathmatchLobbyUi {
    const WIDTH: f32 = 560.0;
    const HEIGHT: f32 = 520.0;

    fn content(static_data: &DeathmatchLobbyStatic, data: &DeathmatchLobbyData) -> impl Scene {
        let not_playing = data.playing == 0;
        let me_player = data.players.iter().any(|p| p.key == data.self_key);
        let mut controls = vec![
            param_button(
                if me_player {
                    "Наблюдать"
                } else {
                    "Играть"
                },
                DM_OBSERVE,
                &[],
                !not_playing,
            ),
            param_button("Покинуть", DM_LEAVE, &[], !not_playing),
        ];
        if data.host {
            controls.push(param_button(
                "Начать",
                DM_START,
                &[],
                !not_playing || (data.players.len() as u32) < data.map.min_players,
            ));
        }
        if me_player {
            controls.push(param_button("Готов", DM_READY, &[], !not_playing));
        }
        let map_text = format!(
            "{}: {} Игроков: {}–{}. Время: {} мин.",
            data.map.name,
            data.map.desc,
            data.map.min_players,
            data.map.max_players,
            data.map.time / 60
        );
        let maps: Vec<_> = if data.host {
            static_data
                .maps
                .iter()
                .map(|m| {
                    param_button(
                        m,
                        DM_HOST_FUNC,
                        &[("func", DM_FUNC_MAP), ("map", m.as_str())],
                        !not_playing || *m == data.map.name,
                    )
                })
                .collect()
        } else {
            Vec::new()
        };
        let mut rows: Vec<Box<dyn SceneList>> = Vec::new();
        for p in &data.players {
            let mark = if p.ready {
                "готов"
            } else {
                "не готов"
            };
            let host = if p.host { " (хост)" } else { "" };
            rows.push(Box::new(EntityScene(line(format!(
                "{}{} — {}, {}",
                p.key, host, p.loadout, mark
            )))));
            let can_edit = not_playing && (data.host || p.key == data.self_key);
            if can_edit {
                for l in &data.loadouts {
                    rows.push(Box::new(EntityScene(param_button(
                        l,
                        DM_CHANGE_LOADOUT,
                        &[("player", p.key.as_str()), ("loadout", l.as_str())],
                        *l == p.loadout,
                    ))));
                }
            }
            if data.host && not_playing && p.key != data.self_key {
                for (label, func) in [
                    ("Выгнать", DM_FUNC_KICK),
                    ("Сделать хостом", DM_FUNC_TRANSFER),
                    ("В наблюдатели", DM_FUNC_OBSERVE),
                ] {
                    rows.push(Box::new(EntityScene(param_button(
                        label,
                        DM_HOST_FUNC,
                        &[("func", func), ("id", p.key.as_str())],
                        false,
                    ))));
                }
            }
        }
        for o in &data.observers {
            let host = if o.host { " (хост)" } else { "" };
            rows.push(Box::new(EntityScene(line(format!(
                "{}{} — наблюдатель",
                o.key, host
            )))));
            if data.host && not_playing && o.key != data.self_key {
                rows.push(Box::new(EntityScene(param_button(
                    "В игроки",
                    DM_HOST_FUNC,
                    &[("func", DM_FUNC_OBSERVE), ("id", o.key.as_str())],
                    false,
                ))));
            }
        }
        let desc = data.loadoutdesc.clone();
        bsn! {
            @WindowContent
            Children [
                @Section { @title: "Лобби", @fill: true, @scrollable: true }
                Children [
                    Node { flex_direction: FlexDirection::Column, row_gap: px(4) }
                    Children [
                        Node { flex_direction: FlexDirection::Row, column_gap: px(4) }
                        Children [ {controls} ]
                        --
                        @NoticeBox Children [ Text(map_text) ]
                        --
                        Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: px(4) }
                        Children [ {maps} ]
                        --
                        Text(desc)
                        --
                        Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: px(4), row_gap: px(2) }
                        Children [ {rows} ]
                    ]
                ]
            ]
        }
    }
}
