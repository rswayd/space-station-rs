//! `ThermoMachine` (freezer/heater): `tgui/packages/tgui/interfaces/ThermoMachine.jsx` and
//! `code/modules/atmospherics/machinery/components/unary_devices/thermomachine.dm`.
//!
//! Chosen over `Vending` as the reference port: Vending needs `ImageButton` with item sprites,
//! `Icon`, a scrollable `Section` and the `LayoutToggle`, which `ss13_ui_kit` does not have yet.
//! ThermoMachine uses only existing primitives and still covers the whole protocol: a toggle act,
//! an act with a numeric param from buttons and from a `NumberInput`, live-changing data.

use serde::{Deserialize, Serialize};

use crate::UiInterface;

/// `T20C`: the initial `target_temperature`.
pub const T20C: f32 = 293.15;

pub struct ThermoMachineUi;

/// `ui_data` of `/obj/machinery/atmospherics/components/unary/thermomachine`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ThermoMachineData {
    pub on: bool,
    pub min: f32,
    pub max: f32,
    pub target: f32,
    pub initial: f32,
    pub temperature: f32,
    pub pressure: f32,
}

impl UiInterface for ThermoMachineUi {
    const NAME: &'static str = "ThermoMachine";
    type Data = ThermoMachineData;
    type StaticData = ();
}

#[cfg(feature = "server")]
pub use server_side::*;

#[cfg(feature = "server")]
mod server_side {
    use bevy::ecs::query::QueryItem;
    use bevy::prelude::*;

    use super::{T20C, ThermoMachineData, ThermoMachineUi};
    use crate::server::{AppTguiExt, ServerUi, UiActed, UiParams, UiReject, UiSystems};

    /// The machine state the window shows.
    // shortcut: no atmos yet. `temperature` relaxes toward the target while on and `pressure` is a
    // constant, instead of a port gas mixture; replace with the port's `gas_mixture` when atmos lands.
    #[derive(Component, Clone, Debug, PartialEq)]
    pub struct ThermoMachine {
        pub on: bool,
        pub min_temperature: f32,
        pub max_temperature: f32,
        pub target_temperature: f32,
        pub temperature: f32,
        pub pressure: f32,
    }

    impl Default for ThermoMachine {
        /// T1 stock parts: `T0C - 200` .. `T20C + 280` (`RefreshParts`).
        fn default() -> Self {
            Self {
                on: false,
                min_temperature: 73.15,
                max_temperature: 573.15,
                target_temperature: T20C,
                temperature: T20C,
                pressure: 101.325,
            }
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq)]
    pub enum ThermoMachineAct {
        Power,
        Target(f32),
    }

    impl ServerUi for ThermoMachineUi {
        type Source = &'static ThermoMachine;
        type UserParam = ();
        type Act = ThermoMachineAct;

        fn static_data(_: QueryItem<'_, '_, Self::Source>) {}

        fn data(m: QueryItem<'_, '_, Self::Source>, _: Entity, _: &()) -> ThermoMachineData {
            ThermoMachineData {
                on: m.on,
                min: m.min_temperature,
                max: m.max_temperature,
                target: m.target_temperature,
                initial: T20C,
                temperature: m.temperature,
                pressure: m.pressure,
            }
        }

        fn parse_act(action: &str, params: &UiParams) -> Result<ThermoMachineAct, UiReject> {
            match action {
                "power" => Ok(ThermoMachineAct::Power),
                // DM also takes `target=input` (a BYOND `input()` prompt) and `adjust=<delta>`;
                // this interface sends neither, so they are rejected rather than ported.
                "target" => Ok(ThermoMachineAct::Target(params.number("target")? as f32)),
                _ => Err(UiReject::UnknownAction),
            }
        }
    }

    /// Relaxation rate of the stub temperature, 1/s.
    const STUB_RATE: f32 = 0.5;

    pub fn plugin(app: &mut App) {
        app.register_ui::<ThermoMachineUi>()
            .add_systems(Update, (apply_acts.in_set(UiSystems::Handle), simulate));
    }

    /// `ui_act`: effects at the moment of handling; the window status was checked this tick.
    fn apply_acts(
        mut acts: MessageReader<UiActed<ThermoMachineUi>>,
        mut machines: Query<&mut ThermoMachine>,
    ) {
        for acted in acts.read() {
            let Ok(mut machine) = machines.get_mut(acted.source) else {
                continue;
            };
            match acted.act {
                ThermoMachineAct::Power => machine.on = !machine.on,
                ThermoMachineAct::Target(target) => {
                    machine.target_temperature =
                        target.clamp(machine.min_temperature, machine.max_temperature);
                }
            }
        }
    }

    fn simulate(mut machines: Query<&mut ThermoMachine>, time: Res<Time>) {
        let k = 1.0 - (-STUB_RATE * time.delta_secs()).exp();
        for mut machine in &mut machines {
            if machine.on && machine.temperature != machine.target_temperature {
                let delta = machine.target_temperature - machine.temperature;
                machine.temperature = if delta.abs() < 0.01 {
                    machine.target_temperature
                } else {
                    machine.temperature + delta * k
                };
            }
        }
    }
}

#[cfg(feature = "client")]
mod client_side {
    use ss13_ui_kit::prelude::*;

    use super::{ThermoMachineData, ThermoMachineUi};
    use crate::client::{ActOnChange, ClientUi};
    use ss13_ui_kit::ActParams;

    impl ClientUi for ThermoMachineUi {
        const WIDTH: f32 = 300.0;
        const HEIGHT: f32 = 350.0;

        fn content(_: &(), data: &ThermoMachineData) -> impl Scene {
            // shortcut: `AnimatedNumber` renders the final value; animate when ui_kit has it.
            let temperature = format!("{:.2} K", data.temperature);
            let pressure = format!("{:.2} kPa", data.pressure);
            let (power_icon, power_text) = if data.on {
                ("power-off", "On")
            } else {
                ("times", "Off")
            };
            let on = data.on;
            let target_param = |value: f32| ActParams(vec![("target".into(), value.to_string())]);
            let (to_min, to_initial, to_max) = (
                target_param(data.min),
                target_param(data.initial),
                target_param(data.max),
            );
            let at_min = data.target == data.min;
            let at_initial = data.target == data.initial;
            let at_max = data.target == data.max;
            let target = NumberInput {
                value: f64::from(data.target.round()),
                min_value: f64::from(data.min.round()),
                max_value: f64::from(data.max.round()),
                step: 5.0,
                step_pixel_size: 3.0,
                unit: "K".into(),
                tick_while_dragging: true,
                ..Default::default()
            };
            let on_target = ActOnChange {
                action: "target".into(),
                param: "target".into(),
            };
            bsn! {
                @WindowContent
                Children [
                    @Section { @title: "Status" }
                    Children [
                        @LabeledList
                        Children [
                            @LabeledListItem { @label: "Temperature", @content: {temperature} }
                            --
                            @LabeledListItem { @label: "Pressure", @content: {pressure} }
                        ]
                    ]
                    --
                    @Section {
                        @title: "Controls",
                        @buttons: bsn! {
                            @Button { @icon: power_icon, @content: power_text }
                            Button { selected: on }
                            Act("power")
                        },
                    }
                    Children [
                        @LabeledList
                        Children [
                            @LabeledListItem { @label: "Target Temperature" }
                            Children [
                                Node { width: px(62) }
                                target
                                on_target
                            ]
                            --
                            @LabeledListItem { @label: "Presets" }
                            Children [
                                @Button { @icon: "fast-backward" }
                                Button { disabled: at_min }
                                Act("target")
                                to_min
                                --
                                @Button { @icon: "sync" }
                                Button { disabled: at_initial }
                                Act("target")
                                to_initial
                                --
                                @Button { @icon: "fast-forward" }
                                Button { disabled: at_max }
                                Act("target")
                                to_max
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
    use crate::encode;

    /// The numbers behind the "full state, no diff" decision in `server`.
    #[test]
    fn encoded_size() {
        let data = ThermoMachineData {
            on: true,
            min: 73.15,
            max: 573.15,
            target: 200.0,
            initial: T20C,
            temperature: 250.5,
            pressure: 101.325,
        };
        assert_eq!(encode(&data).len(), 25);
        assert!(encode(&()).is_empty());
    }
}
