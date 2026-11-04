//! The `SimpleBot` window (client), after `SimpleBot.tsx`: the lock and hack buttons, the
//! settings (power, airplane mode, patrol, maintenance hatch) and the kind's controls (toggles;
//! the medbot's threshold steps by 5).
// shortcut: buttons instead of tgui's icon toggles and slider; no rename or pAI buttons.

use bevy::scene::EntityScene;
use ss13_tgui::client::{AppTguiClientExt, ClientUi};
use ss13_ui_kit::prelude::*;
use ss13_ui_kit::{ActParams, UiColor};

use super::{HEAL_THRESHOLD, SimpleBotData, SimpleBotUi};

pub struct BotsClientPlugin;

impl Plugin for BotsClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_view::<SimpleBotUi>();
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
