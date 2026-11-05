//! The assembly windows (client), after tgui `Signaler.tsx`, `Timer.tsx`,
//! `ProximitySensor.jsx`, `HealthSensor.tsx`, `InfraredEmitter.tsx` and `TransferValve.jsx`.
// shortcut: the number inputs are step buttons (as the radio's tuner); the signaler's
// frequency and code are sent as the stepped values.

use ss13_tgui::client::{AppTguiClientExt, ClientUi};
use ss13_ui_kit::prelude::*;

use super::*;

pub struct AssembliesClientPlugin;

impl Plugin for AssembliesClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_view::<SignalerUi>()
            .register_ui_view::<TimerUi>()
            .register_ui_view::<ProximitySensorUi>()
            .register_ui_view::<HealthSensorUi>()
            .register_ui_view::<InfraredEmitterUi>()
            .register_ui_view::<TransferValveUi>();
    }
}

fn button(
    icon: &'static str,
    content: String,
    action: &str,
    params: Vec<(String, String)>,
    selected: bool,
    disabled: bool,
) -> impl Scene + use<> {
    let action = action.to_owned();
    let params = ActParams(params);
    bsn! {
        @ss13_ui_kit::Button { @icon: icon, @content: content }
        ss13_ui_kit::Button { disabled: disabled, selected: selected }
        Act(action)
        params
    }
}

fn plain(icon: &'static str, content: &str, action: &str, selected: bool) -> impl Scene + use<> {
    button(
        icon,
        content.to_owned(),
        action,
        Vec::new(),
        selected,
        false,
    )
}

/// `-30 -1 +1 +30` second steps of the timer windows (`act('input', { adjust })`).
fn time_steps() -> Vec<impl Scene + use<>> {
    ["-30", "-1", "+1", "+30"]
        .iter()
        .map(|step| {
            let adjust = step.trim_start_matches('+').to_owned();
            button(
                "",
                (*step).to_owned(),
                acts::INPUT,
                vec![(acts::ADJUST.to_owned(), adjust)],
                false,
                false,
            )
        })
        .collect()
}

fn clock(minutes: u32, seconds: u32) -> String {
    format!("{minutes:02}:{seconds:02}")
}

impl ClientUi for SignalerUi {
    const WIDTH: f32 = 280.0;
    const HEIGHT: f32 = 160.0;

    fn content(_: &(), data: &SignalerData) -> impl Scene {
        let freq_text = format_frequency(data.frequency);
        let code_text = data.code.to_string();
        let freq: Vec<_> = [-10i32, -2, 2, 10]
            .iter()
            .map(|&step| {
                let next = (i32::from(data.frequency) + step)
                    .clamp(i32::from(data.min_frequency), i32::from(data.max_frequency));
                let label = format!(
                    "{}{}",
                    if step > 0 { "+" } else { "" },
                    f64::from(step) / 10.0
                );
                button(
                    "",
                    label,
                    acts::FREQ,
                    vec![(acts::FREQ.to_owned(), format_frequency(next as u16))],
                    false,
                    false,
                )
            })
            .chain(std::iter::once(button(
                "sync",
                "Reset".to_owned(),
                acts::RESET,
                vec![(acts::RESET.to_owned(), acts::FREQ.to_owned())],
                false,
                false,
            )))
            .collect();
        let code: Vec<_> = [-10i32, -1, 1, 10]
            .iter()
            .map(|&step| {
                let next = (data.code + step).clamp(1, 100);
                let label = format!("{}{step}", if step > 0 { "+" } else { "" });
                button(
                    "",
                    label,
                    acts::CODE,
                    vec![(acts::CODE.to_owned(), next.to_string())],
                    false,
                    false,
                )
            })
            .chain(std::iter::once(button(
                "sync",
                "Reset".to_owned(),
                acts::RESET,
                vec![(acts::RESET.to_owned(), acts::CODE.to_owned())],
                false,
                false,
            )))
            .collect();
        let send = plain("arrow-up", "Send Signal", acts::SIGNAL, false);
        bsn! {
            @WindowContent
            Children [
                @Section
                Children [
                    @LabeledList
                    Children [
                        @LabeledListItem { @label: "Frequency", @content: freq_text }
                        --
                        @LabeledListItem { @label: "Code", @content: code_text }
                    ]
                    --
                    Node { flex_direction: FlexDirection::Row, margin: UiRect::top(px(4)) }
                    Children [ {freq} ]
                    --
                    Node { flex_direction: FlexDirection::Row, margin: UiRect::top(px(4)) }
                    Children [ {code} ]
                    --
                    Node { margin: UiRect::top(px(4)) }
                    Children [ {send} ]
                ]
            ]
        }
    }
}

impl ClientUi for TimerUi {
    const WIDTH: f32 = 275.0;
    const HEIGHT: f32 = 115.0;

    fn content(_: &(), data: &TimerData) -> impl Scene {
        let time = clock(data.minutes, data.seconds);
        let toggles = vec![
            plain("sync", "Repeat", acts::REPEAT, data.looping),
            plain(
                "clock-o",
                if data.timing { "Stop" } else { "Start" },
                acts::TIME,
                data.timing,
            ),
        ];
        let steps = time_steps();
        bsn! {
            @WindowContent
            Children [
                @Section { @title: "Timing Unit" }
                Children [
                    @LabeledList
                    Children [ @LabeledListItem { @label: "Time", @content: time } ]
                    --
                    Node { flex_direction: FlexDirection::Row, margin: UiRect::top(px(4)) }
                    Children [ {steps} ]
                    --
                    Node { flex_direction: FlexDirection::Row, margin: UiRect::top(px(4)) }
                    Children [ {toggles} ]
                ]
            ]
        }
    }
}

impl ClientUi for ProximitySensorUi {
    const WIDTH: f32 = 250.0;
    const HEIGHT: f32 = 185.0;

    fn content(_: &(), data: &ProximityData) -> impl Scene {
        let time = clock(data.minutes, data.seconds);
        let sensitivity = data.sensitivity.to_string();
        let scan = vec![plain(
            if data.scanning { "lock" } else { "unlock" },
            if data.scanning { "Armed" } else { "Not Armed" },
            acts::SCANNING,
            data.scanning,
        )];
        let sense: Vec<_> = ["-1", "1"]
            .iter()
            .map(|step| {
                button(
                    "",
                    if *step == "1" {
                        "+".to_owned()
                    } else {
                        "-".to_owned()
                    },
                    acts::SENSE,
                    vec![(acts::RANGE.to_owned(), (*step).to_owned())],
                    false,
                    data.scanning,
                )
            })
            .collect();
        let arming = vec![plain(
            "clock-o",
            if data.timing { "Stop" } else { "Start" },
            acts::TIME,
            data.timing,
        )];
        let steps = time_steps();
        bsn! {
            @WindowContent
            Children [
                @Section
                Children [
                    @LabeledList
                    Children [
                        @LabeledListItem { @label: "Detection Range", @content: sensitivity }
                    ]
                    --
                    Node { flex_direction: FlexDirection::Row, margin: UiRect::top(px(4)) }
                    Children [ {scan} ]
                    --
                    Node { flex_direction: FlexDirection::Row, margin: UiRect::top(px(4)) }
                    Children [ {sense} ]
                ]
                --
                @Section { @title: "Auto Arm" }
                Children [
                    @LabeledList
                    Children [ @LabeledListItem { @label: "Time", @content: time } ]
                    --
                    Node { flex_direction: FlexDirection::Row, margin: UiRect::top(px(4)) }
                    Children [ {steps} ]
                    --
                    Node { flex_direction: FlexDirection::Row, margin: UiRect::top(px(4)) }
                    Children [ {arming} ]
                ]
            ]
        }
    }
}

impl ClientUi for HealthSensorUi {
    const WIDTH: f32 = 300.0;
    const HEIGHT: f32 = 140.0;

    fn content(_: &(), data: &HealthSensorData) -> impl Scene {
        let health = data
            .health
            .map_or_else(|| "No reading".to_owned(), |h| format!("{}", h.round()));
        let buttons = vec![
            plain(
                if data.scanning { "power-off" } else { "times" },
                if data.scanning { "On" } else { "Off" },
                acts::SCANNING,
                data.scanning,
            ),
            plain(
                "heartbeat",
                if data.target <= -100.0 {
                    "Alarm: Death"
                } else {
                    "Alarm: Crit"
                },
                acts::TARGET,
                false,
            ),
        ];
        bsn! {
            @WindowContent
            Children [
                @Section
                Children [
                    @LabeledList
                    Children [ @LabeledListItem { @label: "Health", @content: health } ]
                    --
                    Node { flex_direction: FlexDirection::Row, margin: UiRect::top(px(4)) }
                    Children [ {buttons} ]
                ]
            ]
        }
    }
}

impl ClientUi for InfraredEmitterUi {
    const WIDTH: f32 = 225.0;
    const HEIGHT: f32 = 110.0;

    fn content(_: &(), data: &InfraredData) -> impl Scene {
        let buttons = vec![
            plain(
                if data.on { "power-off" } else { "times" },
                if data.on { "On" } else { "Off" },
                acts::POWER,
                data.on,
            ),
            plain(
                if data.visible { "eye" } else { "eye-slash" },
                if data.visible { "Visible" } else { "Invisible" },
                acts::VISIBILITY,
                data.visible,
            ),
        ];
        bsn! {
            @WindowContent
            Children [
                @Section
                Children [
                    Node { flex_direction: FlexDirection::Row }
                    Children [ {buttons} ]
                ]
            ]
        }
    }
}

impl ClientUi for TransferValveUi {
    const WIDTH: f32 = 310.0;
    const HEIGHT: f32 = 300.0;

    fn content(_: &(), data: &TransferValveData) -> impl Scene {
        let none = || "None".to_owned();
        let device = data.attached_device.clone().unwrap_or_else(none);
        let one = data.tank_one.clone().unwrap_or_else(none);
        let two = data.tank_two.clone().unwrap_or_else(none);
        let both = data.tank_one.is_some() && data.tank_two.is_some();
        let has_device = data.attached_device.is_some();
        let valve = vec![button(
            if data.valve { "unlock" } else { "lock" },
            if data.valve { "Open" } else { "Closed" }.to_owned(),
            acts::TOGGLE,
            Vec::new(),
            data.valve,
            !both,
        )];
        let device_buttons = vec![
            button(
                "cog",
                "Configure".to_owned(),
                acts::DEVICE,
                Vec::new(),
                false,
                !has_device,
            ),
            button(
                "eject",
                "Remove".to_owned(),
                acts::REMOVE_DEVICE,
                Vec::new(),
                false,
                !has_device,
            ),
        ];
        let tank_buttons = vec![
            button(
                "eject",
                "Remove Primary".to_owned(),
                acts::TANK_ONE,
                Vec::new(),
                false,
                data.tank_one.is_none(),
            ),
            button(
                "eject",
                "Remove Secondary".to_owned(),
                acts::TANK_TWO,
                Vec::new(),
                false,
                data.tank_two.is_none(),
            ),
        ];
        bsn! {
            @WindowContent
            Children [
                @Section
                Children [
                    @LabeledList
                    Children [ @LabeledListItem { @label: "Valve Status" } ]
                    --
                    Node { flex_direction: FlexDirection::Row }
                    Children [ {valve} ]
                ]
                --
                @Section { @title: "Valve Attachment" }
                Children [
                    @LabeledList
                    Children [ @LabeledListItem { @label: "Attachment", @content: device } ]
                    --
                    Node { flex_direction: FlexDirection::Row, margin: UiRect::top(px(4)) }
                    Children [ {device_buttons} ]
                ]
                --
                @Section { @title: "Attached Tanks" }
                Children [
                    @LabeledList
                    Children [
                        @LabeledListItem { @label: "Primary", @content: one }
                        --
                        @LabeledListItem { @label: "Secondary", @content: two }
                    ]
                    --
                    Node { flex_direction: FlexDirection::Row, margin: UiRect::top(px(4)) }
                    Children [ {tank_buttons} ]
                ]
            ]
        }
    }
}
