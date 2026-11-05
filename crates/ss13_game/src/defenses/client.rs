//! The `TurretControl` window (client), after `TurretControl.tsx` with its
//! `InterfaceLockNoticeBox`, and the `PortableTurret` window, after `PortableTurret.jsx`.
// shortcut: no manual-control button (manual turret control is not ported).

use bevy::scene::EntityScene;
use ss13_tgui::client::{AppTguiClientExt, ClientUi};
use ss13_ui_kit::UiColor;
use ss13_ui_kit::prelude::*;

use super::*;

pub struct DefensesClientPlugin;

impl Plugin for DefensesClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_view::<TurretControlUi>()
            .register_ui_view::<PortableTurretUi>();
    }
}

/// `InterfaceLockNoticeBox`: a silicon's lock button, or the ID swipe hint.
fn lock_notice(silicon: bool, locked: bool) -> Box<dyn SceneList> {
    if silicon {
        let (color, icon, text) = if locked {
            (UiColor::Red, "lock", "Заблокирован")
        } else {
            (UiColor::Green, "unlock", "Разблокирован")
        };
        return Box::new(EntityScene(bsn! {
            @NoticeBox { @color: UiColor::Grey }
            Children [
                @Stack { @align: {Some(AlignItems::Center)} }
                Children [
                    @StackItem Children [ Text("Состояние блокировки интерфейса:") ]
                    --
                    @StackItem { @grow: 1.0 }
                    --
                    @StackItem Children [
                        @Button { @icon: icon, @content: text }
                        Button { color: color }
                        Act(LOCK)
                    ]
                ]
            ]
        }));
    }
    let text = format!(
        "Проведите ID-картой, чтобы {} интерфейс.",
        if locked {
            "разблокировать"
        } else {
            "заблокировать"
        }
    );
    Box::new(EntityScene(bsn! {
        @NoticeBox
        Children [ Text(text) ]
    }))
}

/// `Button.Checkbox`.
fn checkbox(text: &'static str, on: bool, disabled: bool, act: &'static str) -> Box<dyn SceneList> {
    let icon = if on { "check-square-o" } else { "square-o" };
    Box::new(EntityScene(bsn! {
        Node { margin: UiRect::bottom(px(2)) }
        Children [
            @Button { @icon: icon, @content: text, @fluid: true }
            Button { selected: on, disabled: disabled }
            Act(act)
        ]
    }))
}

impl ClientUi for TurretControlUi {
    const WIDTH: f32 = 305.0;
    const HEIGHT: f32 = 168.0;

    fn content(_: &(), data: &TurretControlData) -> impl Scene {
        let disabled = data.locked && !data.silicon_user;
        let notice = vec![lock_notice(data.silicon_user, data.locked)];
        let (power_icon, power_text) = if data.enabled {
            ("power-off", "Включены")
        } else {
            ("times", "Выключены")
        };
        let enabled = data.enabled;
        let (mode_icon, mode_text, mode_color) = if data.lethal {
            ("exclamation-triangle", "Летальный", UiColor::Bad)
        } else {
            ("minus-circle", "Оглушение", UiColor::Average)
        };
        let (borg_icon, borg_text) = if data.shoot_cyborgs {
            ("check", "Да")
        } else {
            ("times", "Нет")
        };
        let borgs = data.shoot_cyborgs;
        bsn! {
            @WindowContent
            Children [
                {notice}
                --
                @Section
                Children [
                    @LabeledList
                    Children [
                        @LabeledListItem { @label: "Состояние турелей" }
                        Children [
                            @Button { @icon: power_icon, @content: power_text }
                            Button { selected: enabled, disabled: disabled }
                            Act(POWER)
                        ]
                        --
                        @LabeledListItem { @label: "Режим турелей" }
                        Children [
                            @Button { @icon: mode_icon, @content: mode_text }
                            Button { color: mode_color, disabled: disabled }
                            Act(MODE)
                        ]
                        --
                        @LabeledListItem { @label: "Стрелять по киборгам" }
                        Children [
                            @Button { @icon: borg_icon, @content: borg_text }
                            Button { selected: borgs, disabled: disabled }
                            Act(SHOOT_SILICONS)
                        ]
                    ]
                ]
            ]
        }
    }
}

impl ClientUi for PortableTurretUi {
    const WIDTH: f32 = 310.0;
    const HEIGHT: f32 = 292.0;

    fn content(_: &(), data: &PortableTurretData) -> impl Scene {
        let locked = data.locked && !data.silicon_user;
        let hint = format!(
            "Проведите ID-картой, чтобы {} интерфейс.",
            if data.locked {
                "разблокировать"
            } else {
                "заблокировать"
            }
        );
        let (power_icon, power_text) = if data.on {
            ("power-off", "Вкл")
        } else {
            ("times", "Выкл")
        };
        let on = data.on;
        let targets: Vec<Box<dyn SceneList>> = if data.lasertag_turret {
            Vec::new()
        } else {
            vec![
                checkbox(
                    "Не охрана и не командование",
                    data.neutralize_all,
                    locked,
                    SHOOT_ALL,
                ),
                checkbox(
                    "Неразрешённое оружие",
                    data.check_weapons,
                    locked,
                    AUTH_WEAPON,
                ),
                checkbox(
                    "Неопознанные формы жизни",
                    data.neutralize_unidentified,
                    locked,
                    CHECK_XENOS,
                ),
                checkbox(
                    "Без защиты разума",
                    data.neutralize_nonmindshielded,
                    locked,
                    CHECK_LOYAL,
                ),
                checkbox(
                    "Разыскиваемые преступники",
                    data.neutralize_criminals,
                    locked,
                    SHOOT_CRIMINALS,
                ),
                checkbox("Киборги", data.neutralize_cyborgs, locked, SHOOT_BORGS),
                checkbox(
                    "Не трогать командование",
                    !data.neutralize_heads,
                    locked,
                    SHOOT_HEADS,
                ),
            ]
        };
        bsn! {
            @WindowContent
            Children [
                @NoticeBox
                Children [ Text(hint) ]
                --
                @Section
                Children [
                    @LabeledList
                    Children [
                        @LabeledListItem { @label: "Состояние" }
                        Children [
                            @Button { @icon: power_icon, @content: power_text }
                            Button { selected: on, disabled: locked }
                            Act(POWER)
                        ]
                    ]
                ]
                --
                @Section { @title: "Настройки целей" }
                Children [ @Stack { @vertical: true } Children [ {targets} ] ]
            ]
        }
    }
}
