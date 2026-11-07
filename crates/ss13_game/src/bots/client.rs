//! The `SimpleBot` window (client), after `SimpleBot.tsx`: the lock, hack and rename buttons,
//! the settings (power, airplane mode, patrol, maintenance hatch) and the kind's controls
//! (toggles; the medbot's threshold steps by 5). The `Mule` window, after `Mule.tsx`: status
//! (rename, power, the cell, mode, load), then unless locked the controls (unload, ID, home,
//! destination), go/stop/home and the delivery settings.
// shortcut: buttons instead of tgui's icon toggles, slider and checkboxes; no pAI or personality
// buttons (no pAIs or ghost possession here).

use bevy::scene::EntityScene;
use ss13_tgui::client::{AppTguiClientExt, ClientUi};
use ss13_ui_kit::prelude::*;
use ss13_ui_kit::{ActParams, UiColor};

use super::{HEAL_THRESHOLD, MuleData, MuleUi, SimpleBotData, SimpleBotUi};

pub struct BotsClientPlugin;

impl Plugin for BotsClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_view::<SimpleBotUi>()
            .register_ui_view::<MuleUi>();
    }
}

/// `capitalizeAll(control.replace('_', ' '))`.
fn label(name: &str) -> String {
    name.replacen('_', " ", 1)
        .split(' ')
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_uppercase().chain(c).collect())
                .unwrap_or_default()
        })
        .collect::<Vec<String>>()
        .join(" ")
}

fn toggle(text: &'static str, on: bool, act: &'static str) -> Box<dyn SceneList> {
    let icon = if on { "toggle-on" } else { "toggle-off" };
    Box::new(EntityScene(bsn! {
        Node { margin: UiRect::bottom(px(2)) }
        Children [
            @Button { @icon: icon, @content: text }
            Button { selected: on }
            Act(act)
        ]
    }))
}

fn control(name: &str, value: i32) -> Box<dyn SceneList> {
    let text = label(name);
    if name == HEAL_THRESHOLD {
        let down = ActParams(vec![("threshold".to_owned(), (value - 5).to_string())]);
        let up = ActParams(vec![("threshold".to_owned(), (value + 5).to_string())]);
        let shown = format!("{text}: {value}%");
        return Box::new(EntityScene(bsn! {
            Node { margin: UiRect::bottom(px(2)) }
            Children [
                @Button { @icon: "minus" }
                Act(HEAL_THRESHOLD)
                down
                --
                Text(shown)
                --
                @Button { @icon: "plus" }
                Act(HEAL_THRESHOLD)
                up
            ]
        }));
    }
    let on = value != 0;
    let icon = if on { "toggle-on" } else { "toggle-off" };
    let act = name.to_owned();
    Box::new(EntityScene(bsn! {
        Node { margin: UiRect::bottom(px(2)) }
        Children [
            @Button { @icon: icon, @content: text }
            Button { selected: on }
            Act(act)
        ]
    }))
}

impl ClientUi for SimpleBotUi {
    const WIDTH: f32 = 450.0;
    const HEIGHT: f32 = 300.0;

    fn content(_: &(), data: &SimpleBotData) -> impl Scene {
        let (hack_icon, hack_text) = if data.emagged {
            ("bug", "Неисправен")
        } else {
            ("lock", "Блокировка безопасности")
        };
        let hack_hidden = !data.can_hack;
        let lock_icon = if data.locked { "lock" } else { "lock-open" };
        let lock_disabled = !data.has_access && !data.can_hack;
        let locked = data.locked;
        let settings: Vec<Box<dyn SceneList>> = match &data.settings {
            None => vec![Box::new(EntityScene(bsn! {
                @NoticeBox
                Children [ Text("Заблокировано!") ]
            }))],
            Some(s) => vec![
                toggle("Питание", s.power, "power"),
                toggle("Режим 'в самолёте'", s.airplane_mode, "airplane"),
                toggle("Патрулирование станции", s.patrol_station, "patrol"),
                toggle("Maintenance Lock", s.maintenance_lock, "maintenance"),
            ],
        };
        let controls: Vec<Box<dyn SceneList>> = data
            .custom_controls
            .iter()
            .map(|(name, value)| control(name, *value))
            .collect();
        let danger = UiColor::Bad;
        bsn! {
            @WindowContent
            Children [
                @Section {
                    @title: "Настройки",
                    @buttons: bsn_list! {
                        @Button { @icon: hack_icon, @content: hack_text }
                        Button { color: danger, disabled: hack_hidden }
                        Act("hack")
                        --
                        @Button { @icon: "fa-poll-h", @content: "Переименовать" }
                        Act("rename")
                        --
                        @Button { @icon: lock_icon, @content: "Controls Lock" }
                        Button { selected: locked, disabled: lock_disabled }
                        Act("lock")
                    },
                }
                Children [ @Stack { @vertical: true } Children [ {settings} ] ]
                --
                @Section { @title: "Управление", @scrollable: true }
                Children [ @Stack { @vertical: true } Children [ {controls} ] ]
            ]
        }
    }
}

fn checkbox(text: &'static str, on: bool, act: &'static str) -> Box<dyn SceneList> {
    let icon = if on { "check-square-o" } else { "square-o" };
    Box::new(EntityScene(bsn! {
        Node { margin: UiRect::bottom(px(2)) }
        Children [
            @Button { @icon: icon, @content: text }
            Button { selected: on }
            Act(act)
        ]
    }))
}

/// `MuleControls`.
fn mule_controls(data: &MuleData) -> Box<dyn SceneList> {
    let unload_hidden = data.load_name.is_none();
    let id = data.bot_id.clone();
    let home = data.home.clone().unwrap_or_default();
    let options = data.destinations.clone();
    let shown = data
        .destination
        .clone()
        .unwrap_or_else(|| "None".to_owned());
    let settings = vec![
        checkbox("Auto-Return", data.auto_return, "autoret"),
        checkbox("Auto-Pickup", data.auto_pickup, "autopick"),
        checkbox("Report-Delivery", data.report_delivery, "report"),
    ];
    let bad = UiColor::Bad;
    let average = UiColor::Average;
    Box::new(EntityScene(bsn! {
        Node { flex_direction: FlexDirection::Column }
        Children [
            @Section {
                @title: "Controls",
                @buttons: bsn_list! {
                    @Button { @icon: "eject", @content: "Unload" }
                    Button { disabled: unload_hidden }
                    Act("unload")
                },
            }
            Children [
                @LabeledList
                Children [
                    @LabeledListItem { @label: "ID" }
                    Children [ @Button { @content: id } Act("setid") ]
                    --
                    @LabeledListItem { @label: "Home" }
                    Children [ @Button { @content: home } Act("sethome") ]
                    --
                    @LabeledListItem { @label: "Destination" }
                    Children [
                        @Dropdown { @value_param: "value", @width: 12.0, @over: true }
                        Dropdown { options: options, selected: shown }
                        Act("destination")
                    ]
                ]
            ]
            --
            @Section { @title: "Actions" }
            Children [
                @Stack
                Children [
                    @Button { @icon: "stop", @content: "Stop" }
                    Button { color: bad }
                    Act("stop")
                    --
                    @Button { @icon: "play", @content: "Go" }
                    Button { color: average }
                    Act("go")
                    --
                    @Button { @icon: "home", @content: "Home" }
                    Act("home")
                ]
            ]
            --
            @Section { @title: "Settings" }
            Children [ @Stack { @vertical: true } Children [ {settings} ] ]
        ]
    }))
}

impl ClientUi for MuleUi {
    const WIDTH: f32 = 350.0;
    const HEIGHT: f32 = 540.0;

    fn content(_: &(), data: &MuleData) -> impl Scene {
        let locked = data.locked && !data.silicon;
        let power = data.power;
        let (power_icon, power_text) = if power {
            ("power-off", "On")
        } else {
            ("times", "Off")
        };
        let percent = data.cell_percent.unwrap_or(0.0) / 100.0;
        let bar_color = if data.cell_percent.unwrap_or(0.0) > 0.0 {
            UiColor::Good
        } else {
            UiColor::Bad
        };
        let mode_color = match data.mode_status.as_str() {
            "good" => UiColor::Good,
            "average" => UiColor::Average,
            "bad" => UiColor::Bad,
            _ => UiColor::Default,
        };
        let mode = data.mode.clone();
        let (load_color, load) = match &data.load_name {
            Some(name) => (UiColor::Good, capitalize(name)),
            None => (UiColor::Average, "None".to_owned()),
        };
        let notice: Vec<Box<dyn SceneList>> = if data.locked {
            vec![Box::new(EntityScene(bsn! {
                @NoticeBox
                Children [ Text("Swipe an ID card to unlock this interface.") ]
            }))]
        } else {
            Vec::new()
        };
        let controls: Vec<Box<dyn SceneList>> = if locked {
            Vec::new()
        } else {
            vec![mule_controls(data)]
        };
        bsn! {
            @WindowContent
            Children [
                {notice}
                @Section {
                    @title: "Status",
                    @buttons: bsn_list! {
                        @Button { @icon: "fa-poll-h", @content: "Rename" }
                        Act("rename")
                        --
                        @Button { @icon: power_icon, @content: power_text }
                        Button { selected: power, disabled: locked }
                        Act("on")
                    },
                }
                Children [
                    @ProgressBar { @empty: true }
                    ProgressBar { value: percent, color: bar_color }
                    --
                    @LabeledList
                    Children [
                        @LabeledListItem { @label: "Mode", @color: mode_color, @content: mode }
                        --
                        @LabeledListItem { @label: "Load", @color: load_color, @content: load }
                    ]
                ]
                --
                {controls}
            ]
        }
    }
}

use ss13_chat::capitalize;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_labels_capitalize_like_tgui() {
        assert_eq!(label("check_id"), "Check Id");
        assert_eq!(label("heal_threshold"), "Heal Threshold");
        assert_eq!(label("extinguish_people"), "Extinguish People");
    }
}
