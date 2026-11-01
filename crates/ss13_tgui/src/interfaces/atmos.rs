//! Atmos machine windows: `AtmosPump`, `AtmosFilter`, `AtmosMixer`, `Canister`, `PortablePump`
//! and `PortableScrubber` (`tgui/packages/tgui/interfaces/*.tsx`, the `ui_data` of
//! `code/modules/atmospherics/machinery/`). The server halves live with the machines
//! (`ss13_game::atmos_machinery`); this module has the data both halves share and the views.
//!
//! Gas lists carry the gas name next to the id: the tgui client maps ids to labels with its
//! own constants (`getGasLabel`), which this crate does not have.
// shortcut: `RoundGauge` and `Knob` are drawn as a `ProgressBar` and a `NumberInput`, until
// ss13_ui_kit has them.

use serde::{Deserialize, Serialize};

use crate::UiInterface;

/// `/obj/machinery/atmospherics/components/binary/pump` (and gates, volume pumps, injectors):
/// `max_rate` > 0 shows a transfer rate, else an output pressure.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct AtmosPumpData {
    pub on: bool,
    pub rate: f32,
    pub max_rate: f32,
    pub pressure: f32,
    pub max_pressure: f32,
}

pub struct AtmosPumpUi;

impl UiInterface for AtmosPumpUi {
    const NAME: &'static str = "AtmosPump";
    type Data = AtmosPumpData;
    type StaticData = ();
}

/// One gas of a filter list.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct GasToggle {
    pub gas_id: String,
    pub gas_name: String,
    pub enabled: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct AtmosFilterData {
    pub on: bool,
    pub rate: f32,
    pub max_rate: f32,
    pub filter_types: Vec<GasToggle>,
}

pub struct AtmosFilterUi;

impl UiInterface for AtmosFilterUi {
    const NAME: &'static str = "AtmosFilter";
    type Data = AtmosFilterData;
    type StaticData = ();
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct AtmosMixerData {
    pub on: bool,
    pub set_pressure: f32,
    pub max_pressure: f32,
    /// Percent.
    pub node1_concentration: f32,
    pub node2_concentration: f32,
}

pub struct AtmosMixerUi;

impl UiInterface for AtmosMixerUi {
    const NAME: &'static str = "AtmosMixer";
    type Data = AtmosMixerData;
    type StaticData = ();
}

/// A portable machine's inserted tank.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct HoldingTank {
    pub name: String,
    pub pressure: f32,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct CanisterStatic {
    pub default_release_pressure: f32,
    pub min_release_pressure: f32,
    pub max_release_pressure: f32,
    pub pressure_limit: f32,
    pub holding_tank_leak_pressure: f32,
    pub holding_tank_frag_pressure: f32,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct CanisterData {
    pub port_connected: bool,
    pub tank_pressure: f32,
    pub release_pressure: f32,
    pub valve_open: bool,
    pub holding_tank: Option<HoldingTank>,
    pub shielding: bool,
    /// Percent; 0 shows "Missing Cell".
    pub cell_charge: f32,
}

pub struct CanisterUi;

impl UiInterface for CanisterUi {
    const NAME: &'static str = "Canister";
    type Data = CanisterData;
    type StaticData = CanisterStatic;
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct PortablePumpData {
    pub on: bool,
    /// `PUMP_IN` (true) / `PUMP_OUT`.
    pub direction: bool,
    pub connected: bool,
    pub pressure: f32,
    pub target_pressure: f32,
    pub default_pressure: f32,
    pub min_pressure: f32,
    pub max_pressure: f32,
    pub holding: Option<HoldingTank>,
}

pub struct PortablePumpUi;

impl UiInterface for PortablePumpUi {
    const NAME: &'static str = "PortablePump";
    type Data = PortablePumpData;
    type StaticData = ();
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct PortableScrubberData {
    pub on: bool,
    pub connected: bool,
    pub pressure: f32,
    pub filter_types: Vec<GasToggle>,
    pub holding: Option<HoldingTank>,
}

pub struct PortableScrubberUi;

impl UiInterface for PortableScrubberUi {
    const NAME: &'static str = "PortableScrubber";
    type Data = PortableScrubberData;
    type StaticData = ();
}

/// `formatPressure` of `Canister.tsx`: kPa below 10 000, then SI pascals.
pub fn format_pressure(value: f32) -> String {
    if value < 10000.0 {
        format!("{value:.0} kPa")
    } else if value < 10_000_000.0 {
        format!("{:.1} MPa", value / 1000.0)
    } else {
        format!("{:.1} GPa", value / 1_000_000.0)
    }
}

#[cfg(feature = "client")]
mod client_side {
    use bevy::scene::EntityScene;
    use ss13_ui_kit::prelude::*;
    use ss13_ui_kit::{ActParams, UiColor};

    use super::*;
    use crate::client::{ActOnChange, ClientUi};

    fn param(key: &str, value: impl ToString) -> ActParams {
        ActParams(vec![(key.to_owned(), value.to_string())])
    }

    fn power_look(on: bool) -> (&'static str, &'static str) {
        if on {
            ("power-off", "On")
        } else {
            ("times", "Off")
        }
    }

    fn power_button(on: bool) -> impl Scene {
        let (icon, text) = power_look(on);
        bsn! {
            @ss13_ui_kit::Button { @icon: icon, @content: text }
            ss13_ui_kit::Button { selected: on }
            Act("power")
        }
    }

    /// `NumberInput` + `Max` button of the pump windows.
    fn number_with_max(
        value: f32,
        max: f32,
        step: f64,
        unit: &str,
        action: &str,
        key: &str,
    ) -> impl Scene + use<> {
        let input = NumberInput {
            value: f64::from(value),
            min_value: 0.0,
            max_value: f64::from(max),
            step,
            unit: unit.to_owned(),
            tick_while_dragging: true,
            ..Default::default()
        };
        let on_change = ActOnChange {
            action: action.to_owned(),
            param: key.to_owned(),
        };
        let max_param = param(key, "max");
        let action = action.to_owned();
        let at_max = value == max;
        bsn! {
            Node { flex_direction: FlexDirection::Row }
            Children [
                Node { width: px(75) } input on_change
                --
                @ss13_ui_kit::Button { @icon: "plus", @content: "Max" }
                ss13_ui_kit::Button { disabled: at_max }
                Node { margin: UiRect::left(px(6)) }
                Act(action)
                max_param
            ]
        }
    }

    fn gas_buttons(gases: &[GasToggle]) -> Vec<impl Scene + use<>> {
        gases
            .iter()
            .map(|g| {
                let icon = if g.enabled {
                    "check-square-o"
                } else {
                    "square-o"
                };
                let (name, enabled) = (g.gas_name.clone(), g.enabled);
                let val = param("val", &g.gas_id);
                bsn! {
                    @ss13_ui_kit::Button { @icon: icon, @content: name }
                    ss13_ui_kit::Button { selected: enabled }
                    Act("toggle_filter")
                    val
                }
            })
            .collect()
    }

    impl ClientUi for AtmosPumpUi {
        const WIDTH: f32 = 335.0;
        const HEIGHT: f32 = 115.0;

        fn content(_: &(), data: &AtmosPumpData) -> impl Scene {
            let row = if data.max_rate > 0.0 {
                let input = number_with_max(data.rate, data.max_rate, 1.0, "L/s", "rate", "rate");
                EntityScene(bsn! {
                    @LabeledListItem { @label: "Transfer Rate" }
                    Children [ {EntityScene(input)} ]
                })
            } else {
                let input = number_with_max(
                    data.pressure,
                    data.max_pressure,
                    10.0,
                    "kPa",
                    "pressure",
                    "pressure",
                );
                EntityScene(bsn! {
                    @LabeledListItem { @label: "Output Pressure" }
                    Children [ {EntityScene(input)} ]
                })
            };
            let power = power_button(data.on);
            bsn! {
                @WindowContent
                Children [
                    @Section
                    Children [
                        @LabeledList
                        Children [
                            @LabeledListItem { @label: "Power" }
                            Children [ {EntityScene(power)} ]
                            --
                            {row}
                        ]
                    ]
                ]
            }
        }
    }

    impl ClientUi for AtmosFilterUi {
        const WIDTH: f32 = 440.0;
        const HEIGHT: f32 = 240.0;

        fn content(_: &(), data: &AtmosFilterData) -> impl Scene {
            let rate = number_with_max(data.rate, data.max_rate, 1.0, "L/s", "rate", "rate");
            let buttons = gas_buttons(&data.filter_types);
            let (on, (icon, text)) = (data.on, power_look(data.on));
            bsn! {
                @WindowContent
                Children [
                    @Section {
                        @title: "Gas Filter",
                        @fill: true,
                        @buttons: bsn! {
                            @ss13_ui_kit::Button { @icon: icon, @content: text }
                            ss13_ui_kit::Button { selected: on }
                            Act("power")
                        },
                    }
                    Children [
                        @LabeledList
                        Children [
                            @LabeledListItem { @label: "Transfer Rate" }
                            Children [ {EntityScene(rate)} ]
                            --
                            @LabeledListItem { @label: "Filter Types" }
                            Children [
                                Node { flex_wrap: FlexWrap::Wrap }
                                Children [ {buttons} ]
                            ]
                        ]
                    ]
                ]
            }
        }
    }

    impl ClientUi for AtmosMixerUi {
        const WIDTH: f32 = 370.0;
        const HEIGHT: f32 = 165.0;

        fn content(_: &(), data: &AtmosMixerData) -> impl Scene {
            let pressure = number_with_max(
                data.set_pressure,
                data.max_pressure,
                10.0,
                "kPa",
                "pressure",
                "pressure",
            );
            let node = |value: f32, action: &str| {
                let input = NumberInput {
                    value: f64::from(value),
                    min_value: 0.0,
                    max_value: 100.0,
                    step: 1.0,
                    unit: "%".into(),
                    tick_while_dragging: true,
                    ..Default::default()
                };
                let on_change = ActOnChange {
                    action: action.to_owned(),
                    param: "concentration".into(),
                };
                bsn! { Node { width: px(62) } input on_change }
            };
            let (node1, node2) = (
                node(data.node1_concentration, "node1"),
                node(data.node2_concentration, "node2"),
            );
            let power = power_button(data.on);
            bsn! {
                @WindowContent
                Children [
                    @Section
                    Children [
                        @LabeledList
                        Children [
                            @LabeledListItem { @label: "Power" }
                            Children [ {EntityScene(power)} ]
                            --
                            @LabeledListItem { @label: "Output Pressure" }
                            Children [ {EntityScene(pressure)} ]
                            --
                            @LabeledListItem { @label: "Main Node", @label_color: UiColor::Green }
                            Children [ {EntityScene(node1)} ]
                            --
                            @LabeledListItem { @label: "Side Node", @label_color: UiColor::Blue }
                            Children [ {EntityScene(node2)} ]
                        ]
                    ]
                ]
            }
        }
    }

    /// The `Holding Tank` section of the portable windows.
    fn holding_section(holding: Option<&HoldingTank>, danger: bool) -> impl Scene + use<> {
        let has = holding.is_some();
        let rows = holding.map(|h| {
            let (name, pressure) = (h.name.clone(), format!("{:.0} kPa", h.pressure));
            EntityScene(bsn! {
                @LabeledList
                Children [
                    @LabeledListItem { @label: "Label", @content: name }
                    --
                    @LabeledListItem { @label: "Pressure", @content: pressure }
                ]
            })
        });
        let empty =
            (!has).then(|| EntityScene(bsn! { @UiBox Children [ Text("No Holding Tank") ] }));
        let color = if danger {
            UiColor::Danger
        } else {
            UiColor::Default
        };
        bsn! {
            @Section {
                @title: "Holding Tank",
                @buttons: bsn! {
                    @ss13_ui_kit::Button { @icon: "eject", @content: "Eject" }
                    ss13_ui_kit::Button { disabled: {!has}, color: color }
                    Act("eject")
                },
            }
            Children [ {rows} -- {empty} ]
        }
    }

    impl ClientUi for CanisterUi {
        const WIDTH: f32 = 350.0;
        const HEIGHT: f32 = 335.0;

        fn content(st: &CanisterStatic, data: &CanisterData) -> impl Scene {
            let limit = st.pressure_limit.max(1.0);
            let ranges = vec![
                (UiColor::Good, 0.0, limit * 0.7),
                (UiColor::Average, limit * 0.7, limit * 0.85),
                (UiColor::Bad, limit * 0.85, f32::INFINITY),
            ];
            let gauge = ProgressBar {
                value: data.tank_pressure,
                min_value: 0.0,
                max_value: limit,
                ranges,
                ..Default::default()
            };
            let gauge_text = format_pressure(data.tank_pressure);
            let regulator = NumberInput {
                value: f64::from(data.release_pressure),
                min_value: f64::from(st.min_release_pressure),
                max_value: f64::from(st.max_release_pressure),
                step: 5.0,
                unit: "kPa".into(),
                tick_while_dragging: true,
                ..Default::default()
            };
            let on_change = ActOnChange {
                action: "pressure".into(),
                param: "pressure".into(),
            };
            let (max_p, reset_p) = (
                param("pressure", st.max_release_pressure),
                param("pressure", st.default_release_pressure),
            );
            let valve_color = match (data.valve_open, data.holding_tank.is_some()) {
                (true, true) => UiColor::Caution,
                (true, false) => UiColor::Danger,
                _ => UiColor::Default,
            };
            let valve_text = if data.valve_open { "Open" } else { "Closed" };
            let (port_text, port_color) = if data.port_connected {
                ("Connected", UiColor::Good)
            } else {
                ("Disconnected", UiColor::Bad)
            };
            let cell = if data.cell_charge > 0.0 {
                format!("{:.0}%", data.cell_charge)
            } else {
                "Missing Cell".to_owned()
            };
            let holding = holding_section(data.holding_tank.as_ref(), data.valve_open);
            bsn! {
                @WindowContent
                Children [
                    @Section { @title: "Canister" }
                    Children [
                        @LabeledList
                        Children [
                            @LabeledListItem { @label: "Pressure" }
                            Children [ @ProgressBar gauge Children [ Text(gauge_text) ] ]
                            --
                            @LabeledListItem { @label: "Regulator" }
                            Children [
                                Node { flex_direction: FlexDirection::Row }
                                Children [
                                    Node { width: px(80) } regulator on_change
                                    --
                                    @ss13_ui_kit::Button { @icon: "fast-forward" }
                                    Act("pressure")
                                    max_p
                                    --
                                    @ss13_ui_kit::Button { @icon: "undo" }
                                    Act("pressure")
                                    reset_p
                                ]
                            ]
                            --
                            @LabeledListItem { @label: "Valve" }
                            Children [
                                @ss13_ui_kit::Button { @content: valve_text }
                                ss13_ui_kit::Button { color: valve_color }
                                Act("valve")
                            ]
                            --
                            @LabeledListItem { @label: "Port", @content: port_text, @color: port_color }
                            --
                            @LabeledListItem { @label: "Cell Charge", @content: cell }
                        ]
                    ]
                    --
                    {EntityScene(holding)}
                ]
            }
        }
    }

    /// `PortableBasicInfo` (`common/PortableAtmos.tsx`).
    fn portable_status(on: bool, pressure: f32, connected: bool) -> impl Scene + use<> {
        let pressure = format!("{pressure:.0} kPa");
        let (port, port_color) = if connected {
            ("Connected", UiColor::Good)
        } else {
            ("Not Connected", UiColor::Average)
        };
        let (icon, text) = power_look(on);
        bsn! {
            @Section {
                @title: "Status",
                @buttons: bsn! {
                    @ss13_ui_kit::Button { @icon: icon, @content: text }
                    ss13_ui_kit::Button { selected: on }
                    Act("power")
                },
            }
            Children [
                @LabeledList
                Children [
                    @LabeledListItem { @label: "Pressure", @content: pressure }
                    --
                    @LabeledListItem { @label: "Port", @content: port, @color: port_color }
                ]
            ]
        }
    }

    impl ClientUi for PortablePumpUi {
        const WIDTH: f32 = 300.0;
        const HEIGHT: f32 = 340.0;

        fn content(_: &(), data: &PortablePumpData) -> impl Scene {
            let status = portable_status(data.on, data.pressure, data.connected);
            let (dir_icon, dir_text) = if data.direction {
                ("sign-in-alt", "In")
            } else {
                ("sign-out-alt", "Out")
            };
            let pump_in = data.direction;
            let input = NumberInput {
                value: f64::from(data.target_pressure),
                min_value: f64::from(data.min_pressure),
                max_value: f64::from(data.max_pressure),
                step: 10.0,
                unit: "kPa".into(),
                tick_while_dragging: true,
                ..Default::default()
            };
            let on_change = ActOnChange {
                action: "pressure".into(),
                param: "pressure".into(),
            };
            let preset = |p: &str, icon: &'static str, disabled: bool| {
                let params = param("pressure", p);
                bsn! {
                    @ss13_ui_kit::Button { @icon: icon }
                    ss13_ui_kit::Button { disabled: disabled }
                    Act("pressure")
                    params
                }
            };
            let (min, reset, max) = (
                preset("min", "minus", data.target_pressure == data.min_pressure),
                preset(
                    "reset",
                    "sync",
                    data.target_pressure == data.default_pressure,
                ),
                preset("max", "plus", data.target_pressure == data.max_pressure),
            );
            let holding = holding_section(data.holding.as_ref(), false);
            bsn! {
                @WindowContent
                Children [
                    {EntityScene(status)}
                    --
                    @Section {
                        @title: "Pumping",
                        @buttons: bsn! {
                            @ss13_ui_kit::Button { @icon: dir_icon, @content: dir_text }
                            ss13_ui_kit::Button { selected: pump_in }
                            Act("direction")
                        },
                    }
                    Children [
                        @LabeledList
                        Children [
                            @LabeledListItem { @label: "Output" }
                            Children [ Node { width: px(80) } input on_change ]
                            --
                            @LabeledListItem { @label: "Presets" }
                            Children [ {EntityScene(min)} -- {EntityScene(reset)} -- {EntityScene(max)} ]
                        ]
                    ]
                    --
                    {EntityScene(holding)}
                ]
            }
        }
    }

    impl ClientUi for PortableScrubberUi {
        const WIDTH: f32 = 320.0;
        const HEIGHT: f32 = 420.0;

        fn content(_: &(), data: &PortableScrubberData) -> impl Scene {
            let status = portable_status(data.on, data.pressure, data.connected);
            let buttons = gas_buttons(&data.filter_types);
            let holding = holding_section(data.holding.as_ref(), false);
            bsn! {
                @WindowContent
                Children [
                    {EntityScene(status)}
                    --
                    @Section { @title: "Filters" }
                    Children [
                        Node { flex_wrap: FlexWrap::Wrap }
                        Children [ {buttons} ]
                    ]
                    --
                    {EntityScene(holding)}
                ]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pressure_text_like_canister_tsx() {
        assert_eq!(format_pressure(101.3), "101 kPa");
        assert_eq!(format_pressure(45000.0), "45.0 MPa");
    }
}
