//! `AirAlarm`: `tgui/packages/tgui/interfaces/AirAlarm/` (+ `common/AtmosControls.tsx`) and
//! `/obj/machinery/airalarm/ui_data`. The server half is `ss13_game::atmos_machinery::alarm`.
//!
//! The screens (home, vents, scrubbers, modes, thresholds) are client state (tgui `useState`),
//! kept in the window's [`UiLocalState`](crate::client::UiLocalState).
// shortcut: thresholds are edited in place with number inputs instead of tgui's
// `AlarmEditingModal`; vents have no overclock button (the server has no overclock).

use serde::{Deserialize, Serialize};

use crate::UiInterface;
use crate::interfaces::atmos::GasToggle;

pub struct AirAlarmUi;

/// `envData` entry.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct EnvEntry {
    pub name: String,
    pub value: String,
    /// `AIR_ALARM_ALERT_*`; `None` for informational lines.
    pub danger: Option<u8>,
}

/// `tlvSettings` entry.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct TlvEntry {
    /// `"pressure"`, `"temperature"` or a gas id: what `set_threshold` sends back.
    pub id: String,
    pub name: String,
    pub unit: String,
    pub hazard_min: f32,
    pub warning_min: f32,
    pub warning_max: f32,
    pub hazard_max: f32,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct VentEntry {
    /// `refID`: what the acts send back as `ref`.
    pub ref_id: String,
    pub long_name: String,
    pub power: bool,
    /// `fan_overclocked`.
    pub overclock: bool,
    /// `get_integrity_percentage()`, 0..1.
    pub integrity: f32,
    pub checks: u8,
    pub excheck: bool,
    pub incheck: bool,
    /// Releasing (pressurizing) rather than siphoning.
    pub direction: bool,
    pub external: f32,
    pub internal: f32,
    pub extdefault: bool,
    pub intdefault: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct ScrubberEntry {
    pub ref_id: String,
    pub long_name: String,
    pub power: bool,
    pub scrubbing: bool,
    pub widenet: bool,
    pub filter_types: Vec<GasToggle>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct ModeEntry {
    pub name: String,
    pub desc: String,
    pub danger: bool,
    pub path: String,
}

/// What an unlocked interface (or a silicon) sees.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct AlarmControls {
    pub vents: Vec<VentEntry>,
    pub scrubbers: Vec<ScrubberEntry>,
    pub selected_mode_path: String,
    pub modes: Vec<ModeEntry>,
    pub panic_siphon_path: String,
    pub filtering_path: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct AirAlarmData {
    pub locked: bool,
    pub silicon_user: bool,
    pub emagged: bool,
    pub danger_level: u8,
    pub atmos_alarm: bool,
    pub fire_alarm: bool,
    /// A sensor is connected (`connected_sensor`).
    pub sensor: bool,
    pub allow_link_change: bool,
    pub env_data: Vec<EnvEntry>,
    pub tlv_settings: Vec<TlvEntry>,
    pub controls: Option<AlarmControls>,
}

impl UiInterface for AirAlarmUi {
    const NAME: &'static str = "AirAlarm";
    type Data = AirAlarmData;
    type StaticData = ();
}

/// `thresholdTypeMap`: `TLV_VAR_*`.
pub const TLV_TYPES: [(&str, u8); 4] = [
    ("hazard_min", 2),
    ("warning_min", 1),
    ("warning_max", 4),
    ("hazard_max", 8),
];
pub const TLV_ALL: u8 = 15;

/// Local state key of the shown screen.
pub const SCREEN: &str = "screen";

#[cfg(feature = "client")]
mod client_side {
    use bevy::scene::EntityScene;
    use ss13_ui_kit::prelude::*;
    use ss13_ui_kit::{ActParams, UiColor};

    use super::*;
    use crate::client::{ActOnChange, ClientUi, SetLocal, UiLocalState};

    fn params(pairs: &[(&str, String)]) -> ActParams {
        ActParams(
            pairs
                .iter()
                .map(|(k, v)| ((*k).to_owned(), v.clone()))
                .collect(),
        )
    }

    /// `dangerMap`.
    fn danger(level: u8) -> (UiColor, &'static str) {
        match level {
            1 => (UiColor::Average, "Caution"),
            2 => (UiColor::Bad, "Danger (Internals Required)"),
            _ => (UiColor::Good, "Optimal"),
        }
    }

    fn screen_button(icon: &'static str, text: &'static str, screen: &'static str) -> impl Scene {
        bsn! {
            @StackItem
            Children [
                @ss13_ui_kit::Button { @icon: icon, @content: text }
                SetLocal { key: {SCREEN.to_owned()}, value: {screen.to_owned()}, rerender: true }
            ]
        }
    }

    fn home(data: &AirAlarmData, c: &AlarmControls) -> impl Scene + use<> {
        let panic = c.selected_mode_path == c.panic_siphon_path;
        let alarm_action = if data.atmos_alarm { "reset" } else { "alarm" };
        let alarm_icon = if data.atmos_alarm {
            "exclamation-triangle"
        } else {
            "exclamation"
        };
        let alarm_color = if data.atmos_alarm {
            UiColor::Caution
        } else {
            UiColor::Default
        };
        let panic_icon = if panic {
            "exclamation-triangle"
        } else {
            "exclamation"
        };
        let panic_color = if panic {
            UiColor::Danger
        } else {
            UiColor::Default
        };
        let panic_mode = params(&[(
            "mode",
            if panic {
                c.filtering_path.clone()
            } else {
                c.panic_siphon_path.clone()
            },
        )]);
        // shortcut: tgui's `Button.Confirm` asks a second click; a plain button here.
        let disconnect: Vec<_> = (data.sensor && data.allow_link_change)
            .then(|| {
                bsn! {
                    @StackItem
                    Children [
                        @ss13_ui_kit::Button { @icon: "link-slash", @content: "Disconnect Sensor" }
                        ss13_ui_kit::Button { color: UiColor::Danger }
                        Act("disconnect_sensor")
                    ]
                }
            })
            .into_iter()
            .collect();
        bsn! {
            @Stack { @vertical: true }
            Children [
                @StackItem
                Children [
                    @ss13_ui_kit::Button { @icon: alarm_icon, @content: "Area Atmosphere Alarm" }
                    ss13_ui_kit::Button { color: alarm_color }
                    Act(alarm_action)
                ]
                --
                @StackItem
                Children [
                    @ss13_ui_kit::Button { @icon: panic_icon, @content: "Panic Siphon" }
                    ss13_ui_kit::Button { color: panic_color }
                    Act("mode")
                    panic_mode
                ]
                --
                {EntityScene(screen_button("sign-out-alt", "Vent Controls", "vents"))}
                --
                {EntityScene(screen_button("filter", "Scrubber Controls", "scrubbers"))}
                --
                {EntityScene(screen_button("cog", "Operating Mode", "modes"))}
                --
                {EntityScene(screen_button("chart-bar", "Alarm Thresholds", "thresholds"))}
                --
                {disconnect}
            ]
        }
    }

    fn target_row(
        label: &'static str,
        value: f32,
        set: &'static str,
        reset: &'static str,
        is_default: bool,
        r: &str,
    ) -> impl Scene + use<> {
        let input = NumberInput {
            value: f64::from(value.round()),
            min_value: 0.0,
            max_value: 5066.0,
            step: 10.0,
            unit: "kPa".into(),
            ..Default::default()
        };
        let on_change = ActOnChange {
            action: set.into(),
            param: "value".into(),
        };
        let (p1, p2) = (
            params(&[("ref", r.to_owned())]),
            params(&[("ref", r.to_owned())]),
        );
        bsn! {
            @LabeledListItem { @label: label }
            Children [
                Node { flex_direction: FlexDirection::Row }
                Children [
                    Node { width: px(75) } input on_change p1
                    --
                    @ss13_ui_kit::Button { @icon: "undo", @content: "Reset" }
                    ss13_ui_kit::Button { disabled: is_default }
                    Act(reset)
                    p2
                ]
            ]
        }
    }

    /// `AtmosControls.tsx` integrity hint.
    const INTEGRITY_TIP: &str = "Overclocking will allow the vent to overpower extreme pressure \
        conditions. However, it will also cause the vent to become damaged over time and \
        eventually fail. The lower the integrity, the less effective the vent will be when in \
        normal operation.";

    fn vent(v: &VentEntry) -> impl Scene + use<> {
        let r = v.ref_id.clone();
        let p = |extra: &[(&str, String)]| {
            let mut all = vec![("ref", r.clone())];
            all.extend(extra.iter().cloned());
            params(&all)
        };
        let power = p(&[("val", u8::from(!v.power).to_string())]);
        let overclock = p(&[]);
        let overclock_color = if v.overclock {
            UiColor::Green
        } else {
            UiColor::Yellow
        };
        let overclock_tip = if v.overclock {
            "Disable overclocking"
        } else {
            "Enable overclocking"
        };
        let dead = v.integrity <= 0.0;
        let integrity = format!("{:.2}%", v.integrity * 100.0);
        let direction = p(&[("val", u8::from(!v.direction).to_string())]);
        let (incheck, excheck) = (
            p(&[("val", v.checks.to_string())]),
            p(&[("val", v.checks.to_string())]),
        );
        let (power_icon, power_text) = if v.power {
            ("power-off", "On")
        } else {
            ("times", "Off")
        };
        let on = v.power;
        let mode_text = if v.direction {
            "Pressurizing"
        } else {
            "Siphoning"
        };
        let mode_color = if v.direction {
            UiColor::Default
        } else {
            UiColor::Danger
        };
        let (ins, exs) = (v.incheck, v.excheck);
        let internal = v.incheck.then(|| {
            EntityScene(target_row(
                "Internal Target",
                v.internal,
                "set_internal_pressure",
                "reset_internal_pressure",
                v.intdefault,
                &r,
            ))
        });
        let external = v.excheck.then(|| {
            EntityScene(target_row(
                "External Target",
                v.external,
                "set_external_pressure",
                "reset_external_pressure",
                v.extdefault,
                &r,
            ))
        });
        let title = v.long_name.clone();
        bsn! {
            @Section {
                @title: title,
                @buttons: bsn! {
                    Node { flex_direction: FlexDirection::Row }
                    Children [
                        @ss13_ui_kit::Button { @icon: power_icon, @content: power_text }
                        ss13_ui_kit::Button { selected: on, disabled: dead }
                        Act("power")
                        power
                        --
                        @ss13_ui_kit::Button { @icon: "gauge-high", @tooltip: overclock_tip }
                        ss13_ui_kit::Button { color: overclock_color, disabled: dead }
                        Act("overclock")
                        overclock
                    ]
                },
            }
            Children [
                @LabeledList
                Children [
                    @LabeledListItem {
                        @label: "Integrity",
                        @tooltip: INTEGRITY_TIP,
                    }
                    Children [ Text(integrity) ]
                    --
                    @LabeledListItem { @label: "Mode" }
                    Children [
                        @ss13_ui_kit::Button { @icon: "sign-in-alt", @content: mode_text }
                        ss13_ui_kit::Button { color: mode_color }
                        Act("direction")
                        direction
                    ]
                    --
                    @LabeledListItem { @label: "Pressure Regulator" }
                    Children [
                        @ss13_ui_kit::Button { @icon: "sign-in-alt", @content: "Internal" }
                        ss13_ui_kit::Button { selected: ins }
                        Act("incheck")
                        incheck
                        --
                        @ss13_ui_kit::Button { @icon: "sign-out-alt", @content: "External" }
                        ss13_ui_kit::Button { selected: exs }
                        Act("excheck")
                        excheck
                    ]
                    --
                    {internal}
                    --
                    {external}
                ]
            ]
        }
    }

    fn scrubber(s: &ScrubberEntry) -> impl Scene + use<> {
        let r = s.ref_id.clone();
        let p = |val: String| params(&[("ref", r.clone()), ("val", val)]);
        let power = p(u8::from(!s.power).to_string());
        let scrubbing = p(u8::from(!s.scrubbing).to_string());
        let widenet = p(u8::from(!s.widenet).to_string());
        let (power_icon, power_text) = if s.power {
            ("power-off", "On")
        } else {
            ("times", "Off")
        };
        let on = s.power;
        let (mode_icon, mode_text, mode_color) = if s.scrubbing {
            ("filter", "Scrubbing", UiColor::Default)
        } else {
            ("sign-in-alt", "Siphoning", UiColor::Danger)
        };
        let wide = s.widenet;
        let (wide_icon, wide_text) = if s.widenet {
            ("expand", "Expanded range")
        } else {
            ("compress", "Normal range")
        };
        let filters: Vec<_> = if s.scrubbing {
            s.filter_types
                .iter()
                .map(|g| {
                    let icon = if g.enabled {
                        "check-square-o"
                    } else {
                        "square-o"
                    };
                    let (name, enabled) = (g.gas_name.clone(), g.enabled);
                    let val = p(g.gas_id.clone());
                    bsn! {
                        @ss13_ui_kit::Button { @icon: icon, @content: name }
                        ss13_ui_kit::Button { selected: enabled }
                        Act("toggle_filter")
                        val
                    }
                })
                .collect()
        } else {
            Vec::new()
        };
        let na = (!s.scrubbing).then_some(EntityScene(bsn! { Text("N/A") }));
        let title = s.long_name.clone();
        bsn! {
            @Section {
                @title: title,
                @buttons: bsn! {
                    @ss13_ui_kit::Button { @icon: power_icon, @content: power_text }
                    ss13_ui_kit::Button { selected: on }
                    Act("power")
                    power
                },
            }
            Children [
                @LabeledList
                Children [
                    @LabeledListItem { @label: "Mode" }
                    Children [
                        @ss13_ui_kit::Button { @icon: mode_icon, @content: mode_text }
                        ss13_ui_kit::Button { color: mode_color }
                        Act("scrubbing")
                        scrubbing
                        --
                        @ss13_ui_kit::Button { @icon: wide_icon, @content: wide_text }
                        ss13_ui_kit::Button { selected: wide }
                        Act("widenet")
                        widenet
                    ]
                    --
                    @LabeledListItem { @label: "Filters" }
                    Children [
                        Node { flex_wrap: FlexWrap::Wrap }
                        Children [ {filters} -- {na} ]
                    ]
                ]
            ]
        }
    }

    fn modes(c: &AlarmControls) -> impl Scene + use<> {
        let items: Vec<_> = c
            .modes
            .iter()
            .map(|m| {
                let selected = m.path == c.selected_mode_path;
                let icon = if selected {
                    "check-square-o"
                } else {
                    "square-o"
                };
                let color = match (selected, m.danger) {
                    (true, true) => UiColor::Red,
                    (true, false) => UiColor::Green,
                    _ => UiColor::Default,
                };
                let text = format!("{} - {}", m.name, m.desc);
                let mode = params(&[("mode", m.path.clone())]);
                bsn! {
                    @StackItem
                    Children [
                        @ss13_ui_kit::Button { @icon: icon, @content: text }
                        ss13_ui_kit::Button { color: color }
                        Act("mode")
                        mode
                    ]
                }
            })
            .collect();
        bsn! { @Stack { @vertical: true } Children [ {items} ] }
    }

    fn thresholds(data: &AirAlarmData) -> impl Scene + use<> {
        let rows: Vec<_> = data
            .tlv_settings
            .iter()
            .map(|t| {
                let cells: Vec<_> = [t.hazard_min, t.warning_min, t.warning_max, t.hazard_max]
                    .into_iter()
                    .zip(TLV_TYPES)
                    .map(|(value, (_, bits))| {
                        let input = NumberInput {
                            value: f64::from(value),
                            min_value: -1.0,
                            max_value: 100_000.0,
                            step: 1.0,
                            ..Default::default()
                        };
                        let on_change = ActOnChange {
                            action: "set_threshold".into(),
                            param: "value".into(),
                        };
                        let p = params(&[
                            ("threshold", t.id.clone()),
                            ("threshold_type", bits.to_string()),
                        ]);
                        bsn! { @TableCell Children [ Node { width: px(56) } input on_change p ] }
                    })
                    .collect();
                let name = format!("{} ({})", t.name, t.unit);
                let reset = params(&[
                    ("threshold", t.id.clone()),
                    ("threshold_type", TLV_ALL.to_string()),
                ]);
                bsn! {
                    @TableRow
                    Children [
                        @TableCell Children [ Text(name) ]
                        --
                        {cells}
                        --
                        @TableCell
                        Children [
                            @ss13_ui_kit::Button { @icon: "undo" }
                            Act("reset_threshold")
                            reset
                        ]
                    ]
                }
            })
            .collect();
        bsn! {
            @Table
            Children [
                @TableRow
                Children [
                    @TableCell Children [ Text("Threshold") ]
                    --
                    @TableCell Children [ Text("Danger Below") ]
                    --
                    @TableCell Children [ Text("Warning Below") ]
                    --
                    @TableCell Children [ Text("Warning Above") ]
                    --
                    @TableCell Children [ Text("Danger Above") ]
                    --
                    @TableCell Children [ Text("Actions") ]
                ]
                --
                {rows}
            ]
        }
    }

    fn status(data: &AirAlarmData) -> impl Scene + use<> {
        let env: Vec<_> = data
            .env_data
            .iter()
            .map(|e| {
                let (color, _) = danger(e.danger.unwrap_or(0));
                let color = if e.danger.is_some() {
                    color
                } else {
                    UiColor::Default
                };
                let (name, value) = (e.name.clone(), e.value.clone());
                bsn! { @LabeledListItem { @label: name, @content: value, @color: color } }
            })
            .collect();
        let (local_color, local_text) = danger(data.danger_level);
        let (area_color, area_text) = if data.atmos_alarm {
            (UiColor::Bad, "Atmosphere Alarm")
        } else if data.fire_alarm {
            (UiColor::Bad, "Fire Alarm")
        } else {
            (UiColor::Good, "Nominal")
        };
        bsn! {
            @Section { @title: "Air Status" }
            Children [
                @LabeledList
                Children [
                    {env}
                    --
                    @LabeledListItem { @label: "Local Status", @content: local_text, @color: local_color }
                    --
                    @LabeledListItem { @label: "Area Status", @content: area_text, @color: area_color }
                    --
                    @LabeledListItem { @label: "Fault Status", @content: "None", @color: UiColor::Good }
                    --
                    @LabeledListItem { @label: "Fault Location", @content: "None", @color: UiColor::Good }
                ]
            ]
        }
    }

    impl ClientUi for AirAlarmUi {
        const WIDTH: f32 = 475.0;
        const HEIGHT: f32 = 650.0;

        fn content(_: &(), data: &AirAlarmData) -> impl Scene {
            view(data, "home")
        }

        fn content_local(_: &(), data: &AirAlarmData, local: &UiLocalState) -> impl Scene {
            view(data, local.get(SCREEN))
        }
    }

    fn view(data: &AirAlarmData, screen: &str) -> impl Scene + use<> {
        {
            let locked = data.locked && !data.silicon_user;
            // `InterfaceLockNoticeBox`.
            let (notice_text, notice_color) = if locked {
                (
                    "Swipe an ID card to unlock this interface.",
                    UiColor::Default,
                )
            } else {
                ("Swipe an ID card to lock this interface.", UiColor::Good)
            };
            let status = status(data);
            let control = data.controls.as_ref().filter(|_| !locked).map(|c| {
                let title = match screen {
                    "vents" => "Vent Controls",
                    "scrubbers" => "Scrubber Controls",
                    "modes" => "Operating Mode",
                    "thresholds" => "Alarm Thresholds",
                    _ => "Air Controls",
                };
                let vents = (screen == "vents").then(|| {
                    let list: Vec<_> = c.vents.iter().map(vent).collect();
                    EntityScene(bsn! {
                        Node { flex_direction: FlexDirection::Column }
                        Children [ {list} ]
                    })
                });
                let scrubbers = (screen == "scrubbers").then(|| {
                    let list: Vec<_> = c.scrubbers.iter().map(scrubber).collect();
                    EntityScene(bsn! {
                        Node { flex_direction: FlexDirection::Column }
                        Children [ {list} ]
                    })
                });
                let modes = (screen == "modes").then(|| EntityScene(modes(c)));
                let thresholds = (screen == "thresholds").then(|| EntityScene(thresholds(data)));
                let is_home = title == "Air Controls";
                let home = is_home.then(|| EntityScene(home(data, c)));
                EntityScene(bsn! {
                    @StackItem { @grow: 1.0 }
                    Children [
                        @Section {
                            @title: title,
                            @fill: true,
                            @scrollable: true,
                            @buttons: bsn! {
                                @ss13_ui_kit::Button { @icon: "arrow-left", @content: "Back" }
                                ss13_ui_kit::Button { disabled: is_home }
                                SetLocal { key: {SCREEN.to_owned()}, value: {"home".to_owned()}, rerender: true }
                            },
                        }
                        Children [ {home} -- {vents} -- {scrubbers} -- {modes} -- {thresholds} ]
                    ]
                })
            });
            bsn! {
                @WindowContent
                Children [
                    @Stack { @vertical: true, @fill: true }
                    Children [
                        @StackItem
                        Children [
                            @NoticeBox { @color: notice_color }
                            Children [ Text(notice_text) ]
                        ]
                        --
                        @StackItem Children [ {EntityScene(status)} ]
                        --
                        {control}
                    ]
                ]
            }
        }
    }
}
