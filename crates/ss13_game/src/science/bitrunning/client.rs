//! The quantum console window (client): tgui `QuantumConsole.tsx`. The data is [`super`], the
//! server half [`super::server`].
//!
//! Client-only state (`useSharedState('tab')`): the difficulty tab ([`UiLocalState`] key).
// shortcut: domains are plain sections (no `Collapsible`), no broadcast checkbox (no
// entertainment monitors), no loading screen overlay (a notice while the server cools down),
// "Остановить домен" stops on one click (no `Button.Confirm`), the cost and reward are numbers
// instead of rows of icons, tooltips are left out.

use bevy::scene::{EntityScene, SceneList};
use ss13_tgui::client::{AppTguiClientExt, ClientUi, SetLocal, UiLocalState};
use ss13_ui_kit::prelude::*;

use super::*;

const TAB: &str = "tab";

pub struct BitrunningClientPlugin;

impl Plugin for BitrunningClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_view::<QuantumConsoleUi>();
    }
}

impl ClientUi for QuantumConsoleUi {
    const WIDTH: f32 = 500.0;
    const HEIGHT: f32 = 500.0;

    fn content(_: &(), data: &QuantumConsoleData) -> impl Scene {
        view(data, &UiLocalState::default())
    }

    fn content_local(_: &(), data: &QuantumConsoleData, local: &UiLocalState) -> impl Scene {
        view(data, local)
    }
}

/// `getColor(difficulty)`.
fn difficulty_color(difficulty: u8) -> UiColor {
    match difficulty {
        1 => UiColor::Yellow,
        2 => UiColor::Average,
        3 => UiColor::Bad,
        _ => UiColor::Green,
    }
}

fn view(data: &QuantumConsoleData, local: &UiLocalState) -> impl Scene + use<> {
    let body: Box<dyn SceneList> = if data.connected {
        Box::new(EntityScene(access_view(data, local)))
    } else {
        Box::new(EntityScene(bsn! {
            @NoticeBox { @danger: true } Children [ Text("Нет подключеня к серверу!") ]
        }))
    };
    bsn! { @WindowContent Children [ {body} ] }
}

fn tab(label: &str, index: u8, selected: u8) -> impl Scene + use<> {
    let label = label.to_owned();
    let color = difficulty_color(index);
    bsn! {
        @TabsTab { @content: label, @icon: "chevron-down" }
        TabsTab { selected: {index == selected}, color: color }
        SetLocal { key: {TAB.to_owned()}, value: {index.to_string()}, rerender: true }
    }
}

/// `AccessView`.
fn access_view(data: &QuantumConsoleData, local: &UiLocalState) -> impl Scene + use<> {
    let selected_tab: u8 = local.get(TAB).parse().unwrap_or(0);
    let mut sorted: Vec<&DomainInfo> = data.available_domains.iter().collect();
    sorted.sort_by_key(|d| d.cost);
    let selected = match &data.generated_domain {
        Some(_) if data.randomized => REDACTED.to_owned(),
        Some(id) => sorted
            .iter()
            .find(|d| &d.id == id)
            .map(|d| d.name.clone())
            .unwrap_or_default(),
        None => "Ничего не загружено".to_owned(),
    };
    let entries: Vec<_> = sorted
        .iter()
        .filter(|d| d.difficulty == selected_tab)
        .map(|d| domain_entry(d, data))
        .collect();
    let random_disabled =
        !data.ready || data.occupants > 0 || data.points < 1 || data.generated_domain.is_some();
    let points = format!("★ {}", data.points);
    let buttons: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
        Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center }
        Children [
            @ss13_ui_kit::Button { @icon: "random", @content: "Случайно" }
            ss13_ui_kit::Button { disabled: random_disabled }
            Act({RANDOM_DOMAIN.to_owned()})
            --
            Node { margin: UiRect::left(px(8)) } Children [ Text(points) ]
        ]
    }));
    let (peaceful, easy, medium, hard) = (
        EntityScene(tab("Мирные", 0, selected_tab)),
        EntityScene(tab("Легкие", 1, selected_tab)),
        EntityScene(tab("Средние", 2, selected_tab)),
        EntityScene(tab("Сложные", 3, selected_tab)),
    );
    let cooling: Option<Box<dyn SceneList>> = (!data.ready).then(|| -> Box<dyn SceneList> {
        Box::new(EntityScene(bsn! {
            @NoticeBox { @info: true }
            Children [ Text("Сервер охлаждается, пожалуйста, ожидайте.") ]
        }))
    });
    let avatars = EntityScene(avatar_display(data));
    let stop_disabled = !data.ready || data.generated_domain.is_none();
    let loaded = data.generated_domain.is_some();
    bsn! {
        Node { flex_direction: FlexDirection::Column, flex_grow: 1.0, min_height: px(0) }
        Children [
            {cooling}
            --
            Node { flex_direction: FlexDirection::Column, flex_grow: 1.0, min_height: px(0) }
            Children [
                @Section {
                    @title: "Виртуальные домены",
                    @buttons: buttons,
                    @fill: true,
                    @scrollable: true,
                }
                Children [
                    @Tabs { @fluid: true }
                    Children [ {peaceful} -- {easy} -- {medium} -- {hard} ]
                    --
                    Node { flex_direction: FlexDirection::Column } Children [ {entries} ]
                ]
            ]
            --
            {avatars}
            --
            @Section
            Children [
                Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center }
                Children [
                    Node { flex_grow: 1.0 }
                    Children [ @NoticeBox { @info: loaded } Children [ Text(selected) ] ]
                    --
                    @ss13_ui_kit::Button { @content: "Остановить домен" }
                    ss13_ui_kit::Button { disabled: stop_disabled }
                    Act({STOP_DOMAIN.to_owned()})
                ]
            ]
        ]
    }
}

/// `DisplayDetails`: "Никакие" for 0, the number otherwise.
fn amount_text(amount: &str) -> String {
    if amount == "0" {
        "Никакие".to_owned()
    } else {
        amount.to_owned()
    }
}

/// `DomainEntry`.
fn domain_entry(d: &DomainInfo, data: &QuantumConsoleData) -> impl Scene + use<> {
    let current = data.generated_domain.as_deref() == Some(d.id.as_str());
    let (icon, label) = if data.randomized {
        ("", REDACTED)
    } else if current {
        ("download", "Загружен")
    } else {
        ("coins", "Загрузить")
    };
    let disabled = data.generated_domain.is_some()
        || !data.ready
        || data.occupants > 0
        || data.points < d.cost;
    let params = ActParams(vec![(DOMAIN_ID.to_owned(), d.id.clone())]);
    let button: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
        @ss13_ui_kit::Button { @icon: icon, @content: label }
        ss13_ui_kit::Button { disabled: disabled }
        Act({SET_DOMAIN.to_owned()})
        params
    }));
    let can_view = d.name != REDACTED;
    let mut desc = d.desc.clone();
    if can_view {
        if d.is_modular {
            desc.push_str(" (Модульное)");
        }
        if d.has_secondary_objectives {
            desc.push_str(" (Доступны дополнительные задачи)");
        }
        if d.announce_ghosts {
            desc.push_str(" (Взаимодействие с призраками)");
        }
    }
    let title = d.name.clone();
    let color = difficulty_color(d.difficulty);
    let grade = d.grade_symbol.clone();
    let cost = format!("Стоимость: {}", amount_text(&d.cost.to_string()));
    let reward = format!("Награда: {}", amount_text(&d.reward));
    bsn! {
        @Section { @title: title, @buttons: button }
        Children [
            Node { flex_direction: FlexDirection::Row }
            Children [
                Node { flex_grow: 4.0, margin: UiRect::right(px(8)) } Children [ Text(desc) ]
                --
                Node { flex_direction: FlexDirection::Column, flex_grow: 1.0 }
                Children [
                    Node Children [ @Icon { @name: grade, @color: color } ]
                    --
                    Node Children [ Text(cost) ]
                    --
                    Node Children [ Text(reward) ]
                ]
            ]
        ]
    }
}

/// `AvatarDisplay`.
fn avatar_display(data: &QuantumConsoleData) -> impl Scene + use<> {
    let retries: Option<Box<dyn SceneList>> =
        data.generated_domain
            .is_some()
            .then(|| -> Box<dyn SceneList> {
                let text = format!("Каналы: {}", data.retries_left);
                Box::new(EntityScene(bsn! {
                    Node { margin: UiRect::right(px(8)) } Children [ Text(text) ]
                }))
            });
    let buttons: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
        Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center }
        Children [
            {retries}
            --
            @ss13_ui_kit::Button { @icon: "sync", @content: "Обновить" }
            Act({REFRESH.to_owned()})
        ]
    }));
    let rows: Vec<_> = data
        .avatars
        .iter()
        .map(|a| {
            let label = format!("{} как \"{}\"", a.pilot, a.name);
            let mark = |v: f32, hot: UiColor| if v > 50.0 { hot } else { UiColor::Grey };
            let (brute, burn, tox, oxy) = (
                mark(a.brute, UiColor::Bad),
                mark(a.burn, UiColor::Average),
                mark(a.tox, UiColor::Green),
                mark(a.oxy, UiColor::Blue),
            );
            let ranges = vec![
                (UiColor::Good, 90.0, f32::INFINITY),
                (UiColor::Average, 50.0, 89.0),
                (UiColor::Bad, f32::NEG_INFINITY, 45.0),
            ];
            let health = a.health;
            let percent = format!("{health:.0}");
            bsn! {
                Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    margin: UiRect::bottom(px(4)),
                }
                Children [
                    Node { flex_grow: 1.0 } Children [ Text(label) ]
                    --
                    Node { flex_direction: FlexDirection::Row, margin: UiRect::horizontal(px(4)) }
                    Children [
                        @Icon { @name: "tint", @color: brute }
                        --
                        @Icon { @name: "fire", @color: burn }
                        --
                        @Icon { @name: "skull-crossbones", @color: tox }
                        --
                        @Icon { @name: "lungs", @color: oxy }
                    ]
                    --
                    Node { width: px(120) }
                    Children [
                        @ProgressBar { @empty: true }
                        ProgressBar {
                            value: health,
                            min_value: -100.0,
                            max_value: 100.0,
                            ranges: ranges,
                        }
                        Children [ Text(percent) ]
                    ]
                ]
            }
        })
        .collect();
    bsn! {
        @Section { @title: "Подключенные клиенты", @buttons: buttons }
        Children [ Node { flex_direction: FlexDirection::Column } Children [ {rows} ] ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_and_amounts() {
        assert_eq!(difficulty_color(0), UiColor::Green);
        assert_eq!(difficulty_color(3), UiColor::Bad);
        assert_eq!(amount_text("0"), "Никакие");
        assert_eq!(amount_text(REDACTED), REDACTED);
    }
}
