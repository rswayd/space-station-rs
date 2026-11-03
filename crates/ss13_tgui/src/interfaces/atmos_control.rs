//! `AtmosControlConsole` (`tgui/packages/tgui/interfaces/AtmosControlConsole.tsx`): a gas
//! chamber's sensor reading, its input injector and output vent, and the atmos handbook.
//! Server half: `ss13_game::atmos_machinery::control`.
// shortcut: a console with several chambers picks one with buttons, not a `Dropdown` (whose
// pick goes to the server here, while tgui keeps the chamber in `useState`).

use serde::{Deserialize, Serialize};

use super::gas_analyzer::{AtmosHandbook, Gasmix};
use crate::UiInterface;

/// `input_info` / `output_info`: on, and the injector's rate or the vent's pressure bound.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub struct PortInfo {
    pub active: bool,
    pub amount: f32,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Chamber {
    pub id: String,
    pub name: String,
    pub gasmix: Option<Gasmix>,
    pub input_info: Option<PortInfo>,
    pub output_info: Option<PortInfo>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct AtmosControlData {
    pub chambers: Vec<Chamber>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct AtmosControlStatic {
    pub max_input: f32,
    pub max_output: f32,
    pub control: bool,
    pub reconnecting: bool,
    pub default_gas: Option<String>,
    pub handbook: AtmosHandbook,
}

pub struct AtmosControlUi;

impl UiInterface for AtmosControlUi {
    const NAME: &'static str = "AtmosControlConsole";
    type Data = AtmosControlData;
    type StaticData = AtmosControlStatic;
}

/// Local key of the chamber shown.
pub const LOCAL_CHAMBER: &str = "chamber";

#[cfg(feature = "client")]
mod client_side {
    use bevy::scene::EntityScene;
    use ss13_ui_kit::prelude::*;
    use ss13_ui_kit::{ActParams, theme::Italic};

    use super::*;
    use crate::client::{ActOnChange, ClientUi, SetLocal, UiLocalState};
    use crate::interfaces::gas_analyzer::LOCAL_GAS;
    use crate::interfaces::gas_analyzer::client_side::{gasmix_view, handbook};

    fn chamber_param(id: &str) -> ActParams {
        ActParams(vec![("chamber".to_owned(), id.to_owned())])
    }

    fn italic(text: &'static str) -> Box<dyn SceneList> {
        Box::new(EntityScene(bsn! {
            Node bevy::app::Propagate::<Italic>(Italic(true)) Children [ Text(text) ]
        }))
    }

    /// One side of "Chamber Controls": a power button and a number.
    fn port(
        info: Option<PortInfo>,
        chamber: &str,
        (label, on, off): (&'static str, &'static str, &'static str),
        (rate_label, unit, max): (&'static str, &'static str, f32),
        (toggle, adjust): (&'static str, &'static str),
        missing: &'static str,
    ) -> Box<dyn SceneList> {
        let Some(info) = info else {
            return italic(missing);
        };
        let (icon, text) = if info.active {
            ("power-off", on)
        } else {
            ("times", off)
        };
        let input = NumberInput {
            value: f64::from(info.amount),
            min_value: 0.0,
            max_value: f64::from(max),
            step: 1.0,
            unit: unit.to_owned(),
            ..Default::default()
        };
        let change = ActOnChange {
            action: adjust.to_owned(),
            param: "rate".to_owned(),
        };
        let toggle_params = chamber_param(chamber);
        let adjust_params = chamber_param(chamber);
        let active = info.active;
        Box::new(EntityScene(bsn! {
            @LabeledList
            Children [
                @LabeledListItem { @label: label }
                Children [
                    @ss13_ui_kit::Button { @icon: icon, @content: text }
                    ss13_ui_kit::Button { selected: active }
                    Act(toggle)
                    toggle_params
                ]
                --
                @LabeledListItem { @label: rate_label }
                Children [ Node { width: px(63) } input change adjust_params ]
            ]
        }))
    }

    fn view(
        st: &AtmosControlStatic,
        data: &AtmosControlData,
        local: &UiLocalState,
    ) -> impl Scene + use<> {
        let chosen = local
            .value(LOCAL_CHAMBER)
            .map(str::to_owned)
            .or_else(|| data.chambers.first().map(|c| c.id.clone()))
            .unwrap_or_default();
        let selected = if data.chambers.len() == 1 {
            data.chambers.first()
        } else {
            data.chambers.iter().find(|c| c.id == chosen)
        };
        // `setActiveGasId(defaultGas)` until a gas is picked.
        let mut local = local.clone();
        if let (Some(gas), None) = (&st.default_gas, local.value(LOCAL_GAS)) {
            local.set(LOCAL_GAS, gas);
        }
        let picker = (data.chambers.len() > 1).then(|| {
            let buttons: Vec<_> = data
                .chambers
                .iter()
                .map(|c| {
                    let pick = SetLocal {
                        key: LOCAL_CHAMBER.into(),
                        value: c.id.clone(),
                        rerender: true,
                    };
                    let is = selected.is_some_and(|s| s.id == c.id);
                    let name = c.name.clone();
                    bsn! {
                        @ss13_ui_kit::Button { @content: name }
                        ss13_ui_kit::Button { selected: is }
                        pick
                    }
                })
                .collect();
            EntityScene(bsn! {
                @Section { @title: "Chamber Selection" }
                Children [ Node { flex_direction: FlexDirection::Row } Children [ {buttons} ] ]
            })
        });
        let title = selected.map_or_else(|| "Chamber Reading".to_owned(), |c| c.name.clone());
        let reading = match selected.and_then(|c| c.gasmix.as_ref()) {
            Some(g) => gasmix_view(g),
            None => italic("No Sensors Detected!"),
        };
        let reconnect = st.reconnecting.then(|| {
            EntityScene(bsn! {
                @ss13_ui_kit::Button { @icon: "undo", @content: "Reconnect" }
                Act("reconnect")
            })
        });
        let controls = selected.filter(|_| st.control).map(|c| {
            let input = port(
                c.input_info,
                &c.id,
                ("Input Injector", "Injecting", "Off"),
                ("Input Rate", "L/s", st.max_input),
                ("toggle_input", "adjust_input"),
                "No Input Device Detected!",
            );
            let output = port(
                c.output_info,
                &c.id,
                ("Output Regulator", "Open", "Closed"),
                ("Output Pressure", "kPa", st.max_output),
                ("toggle_output", "adjust_output"),
                "No Output Device Detected!",
            );
            EntityScene(bsn! {
                @Section { @title: "Chamber Controls" }
                Children [
                    @Stack
                    Children [
                        @StackItem { @grow: 1.0 } Children [ {input} ]
                        --
                        @StackItem { @grow: 1.0 } Children [ {output} ]
                    ]
                ]
            })
        });
        let book = handbook(&st.handbook, &local);
        bsn! {
            @WindowContent
            Children [
                @Stack { @vertical: true }
                Children [
                    @StackItem
                    Children [
                        {picker}
                        --
                        @Section { @title: title }
                        Children [ {reconnect} -- {reading} ]
                        --
                        {controls}
                    ]
                    --
                    @StackItem { @grow: 1.0 } Children [ {EntityScene(book)} ]
                ]
            ]
        }
    }

    impl ClientUi for AtmosControlUi {
        const WIDTH: f32 = 550.0;
        const HEIGHT: f32 = 600.0;

        fn content(st: &AtmosControlStatic, data: &AtmosControlData) -> impl Scene {
            view(st, data, &UiLocalState::default())
        }

        fn content_local(
            st: &AtmosControlStatic,
            data: &AtmosControlData,
            local: &UiLocalState,
        ) -> impl Scene {
            view(st, data, local)
        }
    }
}
