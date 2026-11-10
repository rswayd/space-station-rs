//! The `TurretControl` window (client), after `TurretControl.tsx` with its
//! `InterfaceLockNoticeBox`, and the `PortableTurret` window, after `PortableTurret.jsx`.
// shortcut: no manual-control button (manual turret control is not ported).

use bevy::scene::EntityScene;
use ss13_tgui::client::{AppTguiClientExt, ClientUi};
use ss13_ui_kit::prelude::*;
use ss13_ui_kit::{ActParams, UiColor};

use super::*;

pub struct DefensesClientPlugin;

impl Plugin for DefensesClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_view::<TurretControlUi>()
            .register_ui_view::<PortableTurretUi>()
            .register_ui_view::<ScannerGateUi>();
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

/// One `Button` with a param of the act.
fn param_button(
    text: String,
    selected: bool,
    act: &'static str,
    key: &'static str,
    value: String,
) -> Box<dyn SceneList> {
    let params = ActParams(vec![(key.to_owned(), value)]);
    Box::new(EntityScene(bsn! {
        Node { margin: UiRect::right(px(2)) }
        Children [
            @Button { @content: text }
            Button { selected: selected }
            Act(act)
            params
        ]
    }))
}

/// `ScannerGateMode`: the direction of the check.
fn gate_mode_row(reverse: bool) -> Box<dyn SceneList> {
    let (text, icon, color) = if reverse {
        ("Инвертирован", "random", UiColor::Bad)
    } else {
        ("Обычный", "long-arrow-alt-right", UiColor::Good)
    };
    Box::new(EntityScene(bsn! {
        @LabeledList
        Children [
            @LabeledListItem { @label: "Режим сканирования" }
            Children [
                @Button { @icon: icon, @content: text }
                Button { color: color }
                Act(TOGGLE_REVERSE)
            ]
        ]
    }))
}

/// The mode list of `ScannerGateOff` (the BandaStation labels of the routes).
const GATE_MODE_LABELS: [(&str, &str); 6] = [
    (MODE_WANTED, "Розыск"),
    (MODE_GUNS, "Оружие"),
    (MODE_MINDSHIELD, "Защита разума"),
    (MODE_DISEASE, "Болезни"),
    (MODE_SPECIES, "Вид"),
    (MODE_NUTRITION, "Питание"),
];

/// The route's description and its controls (`SCANNER_GATE_ROUTES`).
fn gate_controls(
    statics: &ScannerGateStatic,
    data: &ScannerGateData,
) -> (String, Vec<Box<dyn SceneList>>) {
    let verb = if data.reverse { "не " } else { "" };
    let has = if data.reverse {
        "не имеет"
    } else {
        "имеет"
    };
    let mut controls: Vec<Box<dyn SceneList>> = Vec::new();
    let text = match data.scan_mode.as_str() {
        MODE_WANTED => format!("Срабатывает, если сканируемый {has} ордеров на арест."),
        MODE_GUNS => format!("Срабатывает, если сканируемый {has} оружие."),
        MODE_MINDSHIELD => format!("Срабатывает, если сканируемый {has} защиту разума."),
        MODE_DISEASE => {
            for threshold in GATE_THRESHOLDS {
                controls.push(param_button(
                    threshold.to_owned(),
                    threshold == data.disease_threshold,
                    SET_THRESHOLD,
                    NEW_THRESHOLD,
                    threshold.to_owned(),
                ));
            }
            format!(
                "Срабатывает, если сканируемый {has} болезнь уровня «{}» или хуже.",
                data.disease_threshold
            )
        }
        MODE_SPECIES => {
            for species in &statics.available_species {
                controls.push(param_button(
                    species.specie_name.clone(),
                    species.specie_id == data.target_species_id,
                    SET_SPECIES,
                    NEW_SPECIES_ID,
                    species.specie_id.clone(),
                ));
            }
            let name = statics
                .available_species
                .iter()
                .find(|s| s.specie_id == data.target_species_id)
                .map_or("", |s| s.specie_name.as_str());
            let zombie = if data.target_zombie {
                " Распознаются все типы зомби, включая спящих."
            } else {
                ""
            };
            format!("Срабатывает, если сканируемый {verb}относится к виду «{name}».{zombie}")
        }
        MODE_NUTRITION => {
            let levels = [
                (NUTRITION_STARVING, STARVING, "Голодание"),
                (NUTRITION_FAT, OBESE, "Ожирение"),
            ];
            for (level, param, label) in levels {
                controls.push(param_button(
                    label.to_owned(),
                    level == data.target_nutrition,
                    SET_NUTRITION,
                    NEW_NUTRITION,
                    param.to_owned(),
                ));
            }
            let current = if data.target_nutrition == NUTRITION_FAT {
                "ожирение"
            } else {
                "голодание"
            };
            format!("Срабатывает, если сканируемый {has} уровень питания «{current}».")
        }
        _ => "Выберите режим сканирования.".to_owned(),
    };
    (text, controls)
}

impl ClientUi for ScannerGateUi {
    const WIDTH: f32 = 400.0;
    const HEIGHT: f32 = 300.0;

    fn content(statics: &ScannerGateStatic, data: &ScannerGateData) -> impl Scene {
        let notice = vec![lock_notice(false, data.locked)];
        let (text, controls) = gate_controls(statics, data);
        let title = format!("Режим сканера: {}", data.scan_mode);
        let off = data.scan_mode == MODE_OFF;
        let modes: Vec<Box<dyn SceneList>> = if off {
            GATE_MODE_LABELS
                .iter()
                .map(|&(mode, label)| {
                    param_button(label.to_owned(), false, SET_MODE, NEW_MODE, mode.to_owned())
                })
                .collect()
        } else {
            vec![
                param_button(
                    "Назад".to_owned(),
                    false,
                    SET_MODE,
                    NEW_MODE,
                    MODE_OFF.to_owned(),
                ),
                gate_mode_row(data.reverse),
            ]
        };
        bsn! {
            @WindowContent
            Children [
                {notice}
                --
                @Section { @title: title }
                Children [
                    @Stack { @vertical: true }
                    Children [
                        @StackItem Children [ Text(text) ]
                        --
                        @StackItem Children [ {controls} ]
                        --
                        @StackItem Children [ {modes} ]
                    ]
                ]
            ]
        }
    }
}
