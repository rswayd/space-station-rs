//! The civilian bounty console window (client): tgui `CivCargoHoldTerminal.tsx` with its
//! personal (`PersonalBountyBlock`, `BountyTextBox`, `BountyPickBox`) and station
//! (`GlobalBountyBlock`) tabs. The data is [`super::CivBountyData`], the server half
//! [`super::bounty`] (feature `server`).
//!
//! Client-only state (`useState`): the tab and the selected station bounty
//! ([`UiLocalState`] keys).
// shortcut: descriptions are shown as plain text (tgui renders their HTML); no marquee or flash;
// the holder's cut is a line under the reward instead of its tooltip, and the "claimed" tab has
// no tooltip.

use bevy::app::Propagate;
use bevy::scene::EntityScene;
use ss13_tgui::client::{AppTguiClientExt, ClientUi, SetLocal, UiLocalState};
use ss13_ui_kit::prelude::*;

use super::*;

const TAB: &str = "tab";
const TAB_PERSONAL: &str = "personal";
const TAB_STATION: &str = "station";
/// The selected station bounty's index.
const SELECTED: &str = "bounty";

pub struct BountyClientPlugin;

impl Plugin for BountyClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_view::<CivBountyUi>();
    }
}

impl ClientUi for CivBountyUi {
    const WIDTH: f32 = 580.0;
    const HEIGHT: f32 = 375.0;

    fn content(_: &(), data: &CivBountyData) -> impl Scene {
        view(data, &UiLocalState::default())
    }

    fn content_local(_: &(), data: &CivBountyData, local: &UiLocalState) -> impl Scene {
        view(data, local)
    }
}

/// The text of a description without its HTML tags (`<br>`, `</br>` become line breaks).
pub fn plain_text(html: &str) -> String {
    let mut out = String::new();
    let mut tag = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => {
                in_tag = true;
                tag.clear();
            }
            '>' if in_tag => {
                in_tag = false;
                let name = tag.trim().trim_start_matches('/').to_lowercase();
                if name.starts_with("br") {
                    out.push('\n');
                }
            }
            c if in_tag => tag.push(c),
            c => out.push(c),
        }
    }
    out
}

/// A number as the JSX prints it (no trailing `.0`).
fn number(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{v:.0}")
    } else {
        format!("{v:.2}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_owned()
    }
}

fn view(data: &CivBountyData, local: &UiLocalState) -> impl Scene + use<> {
    let tab = match local.get(TAB) {
        TAB_STATION => TAB_STATION,
        _ => TAB_PERSONAL,
    };
    let tab_button = |key: &'static str, icon: &'static str, label: &'static str| {
        let color = match (tab == key, key) {
            (true, TAB_PERSONAL) => UiColor::Green,
            (true, _) => UiColor::Brown,
            _ => UiColor::Default,
        };
        EntityScene(bsn! {
            @TabsTab { @content: label, @icon: icon }
            TabsTab { selected: {tab == key}, color: color }
            SetLocal { key: {TAB.to_owned()}, value: {key.to_owned()}, rerender: true }
        })
    };
    let personal = tab_button(TAB_PERSONAL, "user", "Личные заказы");
    let station = tab_button(TAB_STATION, "space-shuttle", "Станционные заказы");
    let (notice_color, notice) = if data.id_inserted {
        (UiColor::Blue, "Приветствуем, ценный сотрудник.")
    } else {
        (
            UiColor::Default,
            "Чтобы начать вставьте ID карту в консоль.",
        )
    };
    let body: Box<dyn SceneList> = if tab == TAB_STATION {
        Box::new(EntityScene(global_block(data, local)))
    } else {
        Box::new(EntityScene(personal_block(data)))
    };
    bsn! {
        @WindowContent
        Node { overflow: Overflow::scroll_y() }
        Children [
            @Section
            Children [
                @Tabs { @fluid: true }
                Children [ {personal} -- {station} ]
            ]
            --
            @NoticeBox { @color: notice_color }
            Children [ Text(notice) ]
            --
            {body}
        ]
    }
}

fn icon_button(
    icon: &'static str,
    tooltip: &'static str,
    action: &'static str,
    disabled: bool,
    selected: bool,
) -> impl Scene + use<> {
    bsn! {
        @ss13_ui_kit::Button { @icon: icon, @tooltip: tooltip }
        ss13_ui_kit::Button { disabled: disabled, selected: selected }
        Act({action.to_owned()})
    }
}

/// `PersonalBountyBlock`.
fn personal_block(data: &CivBountyData) -> impl Scene + use<> {
    let no_pad = !data.pad || !data.id_inserted;
    let recalc = EntityScene(icon_button(
        "sync",
        "Проверить содержимое",
        RECALC,
        no_pad,
        false,
    ));
    let send = EntityScene(if data.sending {
        icon_button("times", "Остановить отправку", STOP, no_pad, true)
    } else {
        icon_button("arrow-up", "Отправить товары", SEND, no_pad, false)
    });
    let has_bounty = data.id_bounty_info.is_some();
    let (bounty_icon, bounty_tip, bounty_color) = if has_bounty {
        ("recycle", "Заменить заказ", UiColor::Green)
    } else {
        ("pen", "Новый заказ", UiColor::Default)
    };
    let id_off = !data.id_inserted;
    let buttons: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
        Node { flex_direction: FlexDirection::Row }
        Children [
            {recalc}
            --
            {send}
            --
            @ss13_ui_kit::Button { @icon: bounty_icon, @tooltip: bounty_tip }
            ss13_ui_kit::Button { color: bounty_color, disabled: id_off }
            Act({BOUNTY.to_owned()})
            --
            @ss13_ui_kit::Button { @icon: "download", @content: "Извлечь ID карту" }
            ss13_ui_kit::Button { disabled: id_off }
            Act({EJECT.to_owned()})
        ]
    }));
    let (status_color, status) = if data.pad {
        (UiColor::Good, "Функционирует")
    } else {
        (UiColor::Bad, "Не обнаружен")
    };
    let report = data.status_report.clone();
    let below: Box<dyn SceneList> = if data.picking {
        Box::new(EntityScene(pick_box(data)))
    } else {
        Box::new(EntityScene(text_box(data)))
    };
    bsn! {
        Node { flex_direction: FlexDirection::Column }
        Children [
            @Section { @title: "Платформа снабжения", @buttons: buttons }
            Children [
                @LabeledList
                Children [
                    @LabeledListItem { @label: "Статус", @color: status_color, @content: status }
                    --
                    @LabeledListItem { @label: "Грузовой отчёт", @content: report }
                ]
            ]
            --
            {below}
        ]
    }
}

/// `BountyTextBox`.
fn text_box(data: &CivBountyData) -> impl Scene + use<> {
    let (info, num, value) = match &data.id_bounty_info {
        Some(info) => (
            plain_text(info),
            data.id_bounty_num.clone(),
            number(data.id_bounty_value),
        ),
        None => (
            "N/A, пожалуйста, возьмите новый заказ.".to_owned(),
            "N/A".to_owned(),
            "N/A".to_owned(),
        ),
    };
    bsn! {
        @Section { @title: "Информация о заказе" }
        Children [
            @LabeledList
            Children [
                @LabeledListItem { @label: "Описание", @content: info }
                --
                @LabeledListItem { @label: "Количество", @content: num }
                --
                @LabeledListItem { @label: "Стоимость", @content: value }
            ]
        ]
    }
}

/// `BountyPickBox`: one button per offered bounty.
fn pick_box(data: &CivBountyData) -> impl Scene + use<> {
    let choices: Vec<_> = data
        .choices
        .iter()
        .enumerate()
        .map(|(i, choice)| {
            let name = choice.name.clone();
            let info = plain_text(&choice.info);
            let pay = format!("Оплата: {} cr", number(choice.value));
            let params = ActParams(vec![(VALUE.to_owned(), (i + 1).to_string())]);
            bsn! {
                Node { flex_grow: 1.0, flex_basis: px(0), padding: UiRect::horizontal(px(4)) }
                Children [
                    @ss13_ui_kit::Button { @fluid: true }
                    ss13_ui_kit::Button { color: UiColor::Green }
                    Act({PICK.to_owned()})
                    params
                    Children [
                        Node { flex_direction: FlexDirection::Column }
                        Children [
                            Text(name)
                            --
                            Node { padding: UiRect::all(px(6)) }
                            BackgroundColor(Color::srgb_u8(0xFA, 0xF0, 0xE6))
                            Propagate::<TextColor>(TextColor(Color::BLACK))
                            Children [ Text(info) ]
                            --
                            Text(pay)
                        ]
                    ]
                ]
            }
        })
        .collect();
    bsn! {
        @Section { @title: "Пожалуйста выберите заказ:" }
        Children [
            Node { flex_direction: FlexDirection::Row, width: percent(100) }
            Children [ {choices} ]
        ]
    }
}

/// `GlobalBountyBlock`: the station bounties on the left, the selected one on the right.
fn global_block(data: &CivBountyData, local: &UiLocalState) -> impl Scene + use<> {
    let selected: Option<usize> = local.get(SELECTED).parse().ok();
    let head: Box<dyn SceneList> = if data.list_bounty.is_empty() {
        Box::new(EntityScene(bsn! {
            @TabsTab { @content: "Update List", @icon: "refresh" }
            TabsTab { color: UiColor::Blue }
            Act({UPDATE_LIST.to_owned()})
            SetLocal { key: {SELECTED.to_owned()}, value: {String::new()}, rerender: true }
        }))
    } else {
        let text = format!("Заказов выполнено: {}.", data.claimed_bounties);
        Box::new(EntityScene(bsn! { @TabsTab { @content: text } }))
    };
    let tabs: Vec<_> = data
        .list_bounty
        .iter()
        .enumerate()
        .map(|(i, bounty)| {
            let icon = if bounty.priority {
                "star"
            } else if bounty.unique {
                "gem"
            } else {
                ""
            };
            let color = if bounty.priority {
                UiColor::Yellow
            } else if bounty.unique {
                UiColor::Teal
            } else {
                UiColor::Default
            };
            let name = bounty.name.clone();
            bsn! {
                @TabsTab { @content: name, @icon: icon }
                TabsTab { selected: {selected == Some(i)}, color: color }
                SetLocal { key: {SELECTED.to_owned()}, value: {i.to_string()}, rerender: true }
            }
        })
        .collect();
    let detail: Box<dyn SceneList> = match selected.and_then(|i| data.list_bounty.get(i)) {
        Some(bounty) => Box::new(EntityScene(bounty_detail(bounty, data))),
        None => Box::new(EntityScene(bsn! {
            @NoticeBox
            Children [ Text("Пожалуйста, выберите заказ из списка.") ]
        })),
    };
    bsn! {
        @Stack { @fill: true }
        Children [
            @StackItem
            Node { width: percent(30) }
            Children [
                @Tabs { @vertical: true, @fluid: true }
                Children [
                    {head}
                    --
                    @TabsTab { @content: "Печать", @icon: "print" }
                    Act({PRINT.to_owned()})
                    --
                    {tabs}
                ]
            ]
            --
            @StackItem { @grow: 1.0 }
            Node { min_width: px(0) }
            Children [ {detail} ]
        ]
    }
}

fn bounty_detail(bounty: &SharedBounty, data: &CivBountyData) -> impl Scene + use<> {
    let title = bounty.name.clone();
    let progress = format!("{} / {} отправлено.", bounty.shipped, bounty.maximum);
    let description = plain_text(&bounty.description);
    let reward = format!("Награда: {}¢", number(bounty.reward));
    let cut = format!(
        "Вы получите долю в {}¢.",
        (bounty.reward * 0.3).round() as i64
    );
    let disabled = !data.pad || !data.id_inserted;
    let (icon, tooltip, action) = if data.sending {
        ("times", "Остановить отправку", STOP)
    } else {
        ("arrow-up", "Отправить товары", SEND)
    };
    let params = ActParams(vec![(GLOBAL.to_owned(), "1".to_owned())]);
    let (value, max) = (bounty.shipped as f32, bounty.maximum as f32);
    bsn! {
        @Section { @title: title }
        Children [
            @ProgressBar { @empty: true }
            ProgressBar { value: value, min_value: 0.0, max_value: max }
            Children [ Text(progress) ]
            --
            Node { padding: UiRect::all(px(6)) }
            Children [ Text(description) ]
            --
            Node {
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(8)),
                margin: UiRect::vertical(px(6)),
            }
            BackgroundColor({UiColor::Green.base()})
            Children [ Text(reward) -- Text(cut) ]
            --
            @ss13_ui_kit::Button { @icon: icon, @content: "Отправка", @tooltip: tooltip, @fluid: true }
            ss13_ui_kit::Button { selected: {data.sending}, disabled: disabled }
            Act({action.to_owned()})
            params
            SetLocal { key: {SELECTED.to_owned()}, value: {"0".to_owned()}, rerender: true }
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptions_lose_their_tags() {
        assert_eq!(
            plain_text("Отправьте <b>мыло</b></br>Срочно!"),
            "Отправьте мыло\nСрочно!"
        );
        assert_eq!(number(129.6), "129.6");
        assert_eq!(number(360.0), "360");
    }
}
