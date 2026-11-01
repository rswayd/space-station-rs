//! `SolarControl` (solar panel control console): `tgui/packages/tgui/interfaces/SolarControl.tsx`
//! and `/obj/machinery/power/solar_control/ui_data` (`code/modules/power/solar.dm`). The server
//! half lives with the game (`ss13_game::power::solar`); this module has the shared data and the
//! client view.

use serde::{Deserialize, Serialize};

use crate::UiInterface;

pub struct SolarControlUi;

/// `ui_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct SolarControlData {
    /// Watts, rounded.
    pub supply: f32,
    pub capacity: f32,
    pub azimuth_current: f32,
    pub azimuth_rate: f32,
    pub max_rotation_rate: f32,
    /// 0 off, 1 timed, 2 auto.
    pub tracking_state: u8,
    pub connected_panels: u32,
    pub connected_tracker: bool,
    /// `history["supply"]`, `history["capacity"]`.
    pub history_supply: Vec<f32>,
    pub history_capacity: Vec<f32>,
}

impl UiInterface for SolarControlUi {
    const NAME: &'static str = "SolarControl";
    type Data = SolarControlData;
    type StaticData = ();
}

/// tgui-core `formatPower` (`formatSiUnit(value, 0, 'W')`).
pub fn format_power(value: f32) -> String {
    format_si_unit(value, "W")
}

/// tgui-core `formatEnergy` (`formatSiUnit(value, 0, 'J')`).
pub fn format_energy(value: f32) -> String {
    format_si_unit(value, "J")
}

/// tgui-core `formatSiUnit(value, 0, unit)`.
pub fn format_si_unit(value: f32, unit: &str) -> String {
    const SYMBOLS: [&str; 9] = ["", "k", "M", "G", "T", "P", "E", "Z", "Y"];
    if !value.is_finite() {
        return value.to_string();
    }
    let real = if value == 0.0 {
        f32::NEG_INFINITY
    } else {
        value.abs().log10().floor()
    };
    let base10 = real.max(0.0) as i32;
    let base1000 = base10 / 3;
    let symbol = SYMBOLS[(base1000 as usize).min(SYMBOLS.len() - 1)];
    let scaled = value / 1000f32.powi(base1000);
    let precision = (2 - base10 % 3).max(0) as usize;
    format!("{scaled:.precision$} {symbol}{unit}")
}

#[cfg(feature = "client")]
mod client_side {
    use ss13_ui_kit::ActParams;
    use ss13_ui_kit::prelude::*;

    use super::{SolarControlData, SolarControlUi, format_power};
    use crate::client::{ActOnChange, ClientUi};

    impl ClientUi for SolarControlUi {
        const WIDTH: f32 = 330.0;
        const HEIGHT: f32 = 330.0;

        fn content(_: &(), data: &SolarControlData) -> impl Scene {
            // shortcut: no `Chart.Line` in ss13_ui_kit yet; the history is in the data and the
            // chart box is drawn empty.
            let (supply, capacity) = (data.supply, data.capacity);
            let output = if capacity > 0.0 {
                format!(
                    "{} of {} ({}%)",
                    format_power(supply),
                    format_power(capacity),
                    (100.0 * supply / capacity).round()
                )
            } else {
                format_power(0.0)
            };
            let fraction = if capacity > 0.0 {
                supply / capacity
            } else {
                0.0
            };
            let ranges = vec![
                (UiColor::Good, 0.66, f32::INFINITY),
                (UiColor::Average, 0.33, 0.66),
                (UiColor::Bad, f32::NEG_INFINITY, 0.33),
            ];
            let panels = data.connected_panels.to_string();
            let panels_color = if data.connected_panels > 0 {
                UiColor::Good
            } else {
                UiColor::Bad
            };
            let (tracker, tracker_color) = if data.connected_tracker {
                ("OK", UiColor::Good)
            } else {
                ("N/A", UiColor::Bad)
            };
            let mode = |m: u8| ActParams(vec![("mode".into(), m.to_string())]);
            let state = data.tracking_state;
            let no_tracker = !data.connected_tracker;
            let azimuth = data.azimuth_current;
            let azimuth_input = NumberInput {
                value: f64::from(azimuth),
                min_value: -360.0,
                max_value: 720.0,
                step: 1.0,
                step_pixel_size: 2.0,
                unit: "°".into(),
                tick_while_dragging: true,
                ..Default::default()
            };
            let rate_input = NumberInput {
                value: f64::from(data.azimuth_rate),
                min_value: f64::from(-data.max_rotation_rate - 0.01),
                max_value: f64::from(data.max_rotation_rate + 0.01),
                step: 0.01,
                step_pixel_size: 1.0,
                unit: "°/m".into(),
                tick_while_dragging: true,
                ..Default::default()
            };
            let on_azimuth = ActOnChange {
                action: "azimuth".into(),
                param: "value".into(),
            };
            let on_rate = ActOnChange {
                action: "azimuth_rate".into(),
                param: "value".into(),
            };
            let auto_text = format!("{azimuth} ° (auto)");
            let shown = |on: bool| if on { Display::Flex } else { Display::None };
            let manual = shown(state == 0 || state == 1);
            let timed = shown(state == 1);
            let auto = shown(state == 2);
            let (off_sel, timed_sel, auto_sel) = (state == 0, state == 1, state == 2);
            let (mode_off, mode_timed, mode_auto) = (mode(0), mode(1), mode(2));
            bsn! {
                @WindowContent
                Children [
                    @Section { @title: "Status" }
                    Children [
                        Node { height: px(64), margin: UiRect::bottom(px(6)) }
                        BackgroundColor(Color::BLACK)
                        --
                        @LabeledList
                        Children [
                            @LabeledListItem { @label: "Power output" }
                            Children [
                                @ProgressBar { @empty: true }
                                ProgressBar { value: fraction, ranges: ranges }
                                Children [ Text(output) ]
                            ]
                            --
                            @LabeledListItem {
                                @label: "Solar panels",
                                @color: panels_color,
                                @content: panels,
                            }
                            --
                            @LabeledListItem {
                                @label: "Solar tracker",
                                @color: tracker_color,
                                @content: tracker,
                            }
                        ]
                    ]
                    --
                    @Section {
                        @title: "Controls",
                        @buttons: bsn! {
                            @Button { @icon: "sync", @content: "Scan for new hardware" }
                            Act("refresh")
                        },
                    }
                    Children [
                        @LabeledList
                        Children [
                            @LabeledListItem { @label: "Tracking" }
                            Children [
                                @Button { @icon: "times", @content: "Off" }
                                Button { selected: off_sel }
                                Act("tracking")
                                mode_off
                                --
                                @Button { @icon: "clock-o", @content: "Timed" }
                                Button { selected: timed_sel }
                                Act("tracking")
                                mode_timed
                                --
                                @Button { @icon: "sync", @content: "Auto" }
                                Button { selected: auto_sel, disabled: no_tracker }
                                Act("tracking")
                                mode_auto
                            ]
                            --
                            @LabeledListItem { @label: "Azimuth" }
                            Children [
                                Node { margin: UiRect::right(px(6)) }
                                Children [ @Icon { @name: "arrow-up", @rotation: azimuth } ]
                                --
                                Node { width: px(52), display: manual }
                                azimuth_input
                                on_azimuth
                                --
                                Node { width: px(80), display: timed }
                                rate_input
                                on_rate
                                --
                                Node { display: auto }
                                Children [ Text(auto_text) ]
                            ]
                        ]
                    ]
                ]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_power_matches_tgui() {
        assert_eq!(format_power(0.0), "0.00 W");
        assert_eq!(format_power(2500.0), "2.50 kW");
        assert_eq!(format_power(352_500.0), "352 kW");
        assert_eq!(format_power(45_000.0), "45.0 kW");
    }
}
