//! `Apc` (area power controller): `tgui/packages/tgui/interfaces/Apc.jsx` and
//! `/obj/machinery/power/apc/ui_data` (`code/modules/power/apc/apc_main.dm`). The server half
//! lives with the game (`ss13_game::power::apc_ui`).

use serde::{Deserialize, Serialize};

use crate::UiInterface;

pub struct ApcUi;

/// `ui_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct ApcData {
    pub locked: bool,
    /// Seconds of the power failure left.
    pub fail_time: i32,
    pub is_operating: bool,
    /// `main_status`: 0 none, 1 low, 2 good.
    pub external_power: u8,
    /// Percent; `None` without a cell.
    pub power_cell_status: Option<f32>,
    pub charge_mode: bool,
    /// 0 not charging, 1 charging, 2 full.
    pub charging_status: u8,
    pub charging_power_display: String,
    pub total_load: String,
    pub cover_locked: bool,
    pub remote_access: bool,
    pub silicon_user: bool,
    pub malf_status: u8,
    pub emergency_lights: bool,
    pub nightshift_lights: bool,
    pub disable_nightshift_toggle: bool,
    pub power_channels: Vec<PowerChannel>,
}

/// One `powerChannels` entry; `topicParams` is fixed per channel (`key`: `eqp`, `lgt`, `env`).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct PowerChannel {
    pub title: String,
    pub power_load: String,
    /// `APC_CHANNEL_*`: 0 off, 1 auto off, 2 on, 3 auto on.
    pub status: u8,
    pub key: String,
}

impl UiInterface for ApcUi {
    const NAME: &'static str = "Apc";
    type Data = ApcData;
    type StaticData = ();
}

/// `powerStatusMap`: color, external power text, charging text.
pub fn power_status(level: u8) -> (&'static str, &'static str, &'static str) {
    match level {
        2 => ("good", "Внешнее питание", "Полный заряд"),
        1 => ("average", "Низкое внешнее питание", "Зарядка: "),
        _ => ("bad", "Нет внешнего питания", "Не заряжается"),
    }
}

#[cfg(feature = "client")]
mod client_side {
    use bevy::scene::EntityScene;
    use ss13_ui_kit::ActParams;
    use ss13_ui_kit::prelude::*;

    use super::{ApcData, ApcUi, PowerChannel, power_status};
    use crate::client::ClientUi;

    fn color(name: &str) -> UiColor {
        match name {
            "good" => UiColor::Good,
            "average" => UiColor::Average,
            _ => UiColor::Bad,
        }
    }

    fn channel_row(channel: &PowerChannel, locked: bool) -> Box<dyn Scene> {
        let on = channel.status >= 2;
        let (state, state_color) = if on {
            ("On", UiColor::Good)
        } else {
            ("Off", UiColor::Bad)
        };
        let param = |v: u8| ActParams(vec![(channel.key.clone(), v.to_string())]);
        let (auto, on_p, off) = (param(3), param(2), param(1));
        let auto_sel = !locked && (channel.status == 1 || channel.status == 3);
        let on_sel = !locked && channel.status == 2;
        let off_sel = !locked && channel.status == 0;
        let label = channel.title.clone();
        let load = channel.power_load.clone();
        Box::new(bsn! {
            @LabeledListItem {
                @label: label,
                @buttons: bsn_list! {
                    Node { margin: UiRect::horizontal(px(12)) }
                    TextColor({state_color.base()})
                    Children [ Text(state) ]
                    --
                    @Button { @icon: "sync", @content: "Авто" }
                    Button { selected: auto_sel, disabled: locked }
                    Act("channel")
                    auto
                    --
                    @Button { @icon: "power-off", @content: "Вкл" }
                    Button { selected: on_sel, disabled: locked }
                    Act("channel")
                    on_p
                    --
                    @Button { @icon: "times", @content: "Выкл" }
                    Button { selected: off_sel, disabled: locked }
                    Act("channel")
                    off
                },
            }
            Children [ Text(load) ]
        })
    }

    impl ClientUi for ApcUi {
        const WIDTH: f32 = 450.0;
        const HEIGHT: f32 = 445.0;

        fn content(_: &(), data: &ApcData) -> impl Scene {
            let failure = (data.fail_time > 0).then(|| {
                let text = format!(
                    "SYSTEM FAILURE\nI/O regulators have malfunctioned!\nAwaiting system reboot.\n\
                     Executing software reboot in {} seconds...",
                    data.fail_time
                );
                EntityScene(bsn! {
                    @NoticeBox { @info: true }
                    Children [
                        Text(text)
                        --
                        @Button {
                            @icon: "sync",
                            @content: "Перезагрузить",
                            @tooltip: "Принудительный сброс.",
                        }
                        Act("reboot")
                    ]
                })
            });
            let normal = (data.fail_time <= 0).then(|| EntityScene(view(data)));
            bsn! {
                @WindowContent
                Children [ {failure} -- {normal} ]
            }
        }
    }

    fn view(data: &ApcData) -> impl Scene + use<> {
        let locked = data.locked && !data.silicon_user;
        let (ext_color, ext_text, _) = power_status(data.external_power);
        let (chg_color, _, chg_text) = power_status(data.charging_status);
        let ext_color = color(ext_color);
        let chg_color = color(chg_color);
        let ext_text = format!("[ {ext_text} ]");
        let chg_text = format!(
            "[ {}{} ]",
            chg_text,
            if data.charging_status == 1 {
                data.charging_power_display.as_str()
            } else {
                ""
            }
        );
        let cell = data.power_cell_status.unwrap_or(0.0) / 100.0;
        let (breaker_icon, breaker_text) = if data.is_operating {
            ("power-off", "Вкл")
        } else {
            ("times", "Выкл")
        };
        let breaker_sel = data.is_operating && !locked;
        let (mode_icon, mode_text) = if data.charge_mode {
            ("sync", "Авто")
        } else {
            ("times", "Выкл")
        };
        // `InterfaceLockNoticeBox`.
        let lock_notice = if data.silicon_user || data.remote_access {
            let (lock_color, lock_icon, lock_text) = if data.locked {
                (UiColor::Red, "lock", "Заблокирован")
            } else {
                (UiColor::Green, "unlock", "Разблокирован")
            };
            let prevent = data.remote_access;
            Box::new(bsn! {
                @NoticeBox { @color: UiColor::Grey }
                Children [
                    @Stack { @align: {Some(AlignItems::Center)} }
                    Children [
                        @StackItem Children [ Text("Состояние блокировки интерфейса:") ]
                        --
                        @StackItem { @grow: 1.0 }
                        --
                        @StackItem Children [
                            @Button { @icon: lock_icon, @content: lock_text }
                            Button { color: lock_color, disabled: prevent }
                            Act("lock")
                        ]
                    ]
                ]
            }) as Box<dyn Scene>
        } else {
            let text = format!(
                "Проведите ID-картой, чтобы {} интерфейс.",
                if data.locked {
                    "разблокировать"
                } else {
                    "заблокировать"
                }
            );
            Box::new(bsn! {
                @NoticeBox
                Children [ Text(text) ]
            }) as Box<dyn Scene>
        };
        let channels: Vec<Box<dyn Scene>> = data
            .power_channels
            .iter()
            .map(|c| channel_row(c, locked))
            .collect();
        let total = data.total_load.clone();
        let silicon = data.silicon_user;
        let overload = silicon.then(|| {
            EntityScene(bsn! {
                @Button { @icon: "lightbulb-o", @content: "Перегрузка" }
                Act("overload")
            })
        });
        let (cover_icon, cover_text) = if data.cover_locked {
            ("lock", "Включён")
        } else {
            ("unlock", "Выключен")
        };
        let emergency_text = if data.emergency_lights {
            "Включён"
        } else {
            "Выключен"
        };
        let night_text = if data.nightshift_lights {
            "Включён"
        } else {
            "Выключен"
        };
        let night_disabled = data.disable_nightshift_toggle;
        bsn! {
            Node { flex_direction: FlexDirection::Column }
            Children [
                {EntityScene(lock_notice)}
                --
                @Section { @title: "Состояние питания" }
                Children [
                    @LabeledList
                    Children [
                        @LabeledListItem {
                            @label: "Главный выключатель",
                            @color: ext_color,
                            @buttons: bsn! {
                                @Button { @icon: breaker_icon, @content: breaker_text }
                                Button { selected: breaker_sel, disabled: locked }
                                Act("breaker")
                            },
                        }
                        Children [ Text(ext_text) ]
                        --
                        @LabeledListItem { @label: "Батарея" }
                        Children [
                            @ProgressBar { @empty: true }
                            ProgressBar { value: cell, color: UiColor::Good }
                            Node { flex_grow: 1.0, width: percent(100) }
                        ]
                        --
                        @LabeledListItem {
                            @label: "Режим зарядки",
                            @color: chg_color,
                            @buttons: bsn! {
                                @Button { @icon: mode_icon, @content: mode_text }
                                Button { disabled: locked }
                                Act("charge")
                            },
                        }
                        Children [ Text(chg_text) ]
                    ]
                ]
                --
                @Section { @title: "Кабели питания" }
                Children [
                    @LabeledList
                    Children [
                        {channels}
                        --
                        @LabeledListItem { @label: "Общая нагрузка" }
                        Children [ Text(total) ss13_ui_kit::theme::Bold ]
                    ]
                ]
                --
                @Section {
                    @title: "Прочее",
                    @buttons: bsn_list! { {overload} },
                }
                Children [
                    @LabeledList
                    Children [
                        @LabeledListItem {
                            @label: "Болты панели",
                            @buttons: bsn! {
                                @Button {
                                    @icon: cover_icon,
                                    @content: cover_text,
                                    @tooltip: "Панель ЛКП может быть открыта с помощью лома.",
                                }
                                Button { disabled: locked }
                                Act("cover")
                            },
                        }
                        --
                        @LabeledListItem {
                            @label: "Аварийное освещение",
                            @buttons: bsn! {
                                @Button {
                                    @icon: "lightbulb-o",
                                    @content: emergency_text,
                                    @tooltip: "При отсутствии источника питания в лампах используется внутренний источник питания.",
                                }
                                Button { disabled: locked }
                                Act("emergency_lighting")
                            },
                        }
                        --
                        @LabeledListItem {
                            @label: "Ночное освещение",
                            @buttons: bsn! {
                                @Button {
                                    @icon: "lightbulb-o",
                                    @content: night_text,
                                    @tooltip: "Приглушенный свет для снижения энергопотребления.",
                                }
                                Button { disabled: night_disabled }
                                Act("toggle_nightshift")
                            },
                        }
                    ]
                ]
            ]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_map_and_round_trip() {
        assert_eq!(power_status(1).2, "Зарядка: ");
        assert_eq!(power_status(9).0, "bad");
        let data = ApcData {
            power_cell_status: Some(90.0),
            power_channels: vec![PowerChannel {
                title: "Оборудование".into(),
                power_load: "1.5 kW".into(),
                status: 3,
                key: "eqp".into(),
            }],
            ..Default::default()
        };
        assert_eq!(
            crate::decode::<ApcData>(&crate::encode(&data)).unwrap(),
            data
        );
    }
}
