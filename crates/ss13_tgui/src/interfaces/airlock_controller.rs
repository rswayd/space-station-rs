//! Airlock chamber consoles: `AirlockButtonController`
//! (`tgui/packages/tgui/interfaces/AirlockButtonController.tsx`, the access console of
//! `code/game/machinery/embedded_controller/access_controller.dm`) and `AirlockController`
//! (`AirlockController.tsx`, the embedded console of `airlock_controller.dm`). The server half
//! lives with the game (`ss13_game::airlock_controllers`).
//!
//! The access console names its doors `"interior"`/`"exterior"` in `requested_door` where tg
//! sends the doors' refs.

use serde::{Deserialize, Serialize};

use crate::UiInterface;

pub struct AirlockButtonControllerUi;

/// `door_buttons/airlock_controller/ui_data`; a door flag is false when that door is missing.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct AirlockButtonControllerData {
    pub interior_door: bool,
    pub exterior_door: bool,
    pub interior_door_closed: bool,
    pub exterior_door_closed: bool,
    pub busy: bool,
}

impl UiInterface for AirlockButtonControllerUi {
    const NAME: &'static str = "AirlockButtonController";
    type Data = AirlockButtonControllerData;
    type StaticData = ();
}

pub struct AirlockControllerUi;

/// `airlock_controller/ui_data`: `airlockState` (`inopen`, `pressurize`, `closed`,
/// `depressurize`, `outopen`), the sensor's pressure (`None`: `"----"`), the pump
/// (`release`, `siphon`, `----`) and the doors (`open`, `closed`, `----`).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct AirlockControllerData {
    pub airlock_state: String,
    pub sensor_pressure: Option<f32>,
    pub pump_status: String,
    pub interior_status: String,
    pub exterior_status: String,
}

impl UiInterface for AirlockControllerUi {
    const NAME: &'static str = "AirlockController";
    type Data = AirlockControllerData;
    type StaticData = ();
}

/// `getAirlockStatus`: the status line, the pressure icon and its color name.
pub fn airlock_status(state: &str) -> (&'static str, &'static str, &'static str) {
    match state {
        "inopen" => ("Interior Airlock Open", "", "good"),
        "pressurize" => ("Cycling to Interior Airlock", "fan", "average"),
        "closed" => ("Inactive", "", "white"),
        "depressurize" => ("Cycling to Exterior Airlock", "fan", "average"),
        "outopen" => ("Exterior Airlock Open", "exclamation-triangle", "bad"),
        _ => ("Unknown", "", "average"),
    }
}

/// `nameToUpperCase`.
pub fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// `RetrieveButton`'s label and act for one door.
pub fn door_button(data: &AirlockButtonControllerData, interior: bool) -> (String, &'static str) {
    let side = if interior {
        "interior door"
    } else {
        "exterior door"
    };
    let (ours_closed, other_closed, other) = if interior {
        (
            data.interior_door_closed,
            data.exterior_door_closed,
            data.exterior_door,
        )
    } else {
        (
            data.exterior_door_closed,
            data.interior_door_closed,
            data.interior_door,
        )
    };
    if !ours_closed {
        (format!("Close {side}"), "close")
    } else if !other_closed && other {
        (format!("Cycle to {side}"), "open")
    } else {
        (format!("Open {side}"), "open")
    }
}

#[cfg(feature = "client")]
mod client_side {
    use bevy::scene::EntityScene;
    use ss13_ui_kit::prelude::*;
    use ss13_ui_kit::{ActParams, UiColor};

    use super::*;
    use crate::client::ClientUi;

    impl ClientUi for AirlockButtonControllerUi {
        const WIDTH: f32 = 500.0;
        const HEIGHT: f32 = 130.0;

        fn content(_: &(), data: &AirlockButtonControllerData) -> impl Scene {
            let body: Box<dyn Scene> = if !data.interior_door && !data.exterior_door {
                Box::new(bsn! {
                    @NoticeBox { @danger: true }
                    Children [ Text("No doors detected") ]
                })
            } else {
                let busy = data.busy;
                let buttons: Vec<_> = [(data.interior_door, true), (data.exterior_door, false)]
                    .into_iter()
                    .filter(|&(present, _)| present)
                    .map(|(_, interior)| {
                        let (label, action) = door_button(data, interior);
                        let closed = if interior {
                            data.interior_door_closed
                        } else {
                            data.exterior_door_closed
                        };
                        let icon = if closed { "lock-open" } else { "lock" };
                        let side = if interior { "interior" } else { "exterior" };
                        let params = ActParams(vec![("requested_door".into(), side.into())]);
                        bsn! {
                            @StackItem { @grow: 1.0 }
                            Children [
                                @Button { @icon: icon, @content: label }
                                Button { disabled: busy, color: UiColor::Green }
                                Act(action)
                                params
                            ]
                        }
                    })
                    .collect();
                Box::new(bsn! {
                    @Stack
                    Children [ {buttons} ]
                })
            };
            bsn! {
                @WindowContent
                Children [
                    @Section { @title: "Airlock Controller" }
                    Children [ {EntityScene(body)} ]
                ]
            }
        }
    }

    fn color_of(name: &str) -> UiColor {
        match name {
            "good" => UiColor::Good,
            "average" => UiColor::Average,
            "bad" => UiColor::Bad,
            _ => UiColor::Default,
        }
    }

    impl ClientUi for AirlockControllerUi {
        const WIDTH: f32 = 500.0;
        const HEIGHT: f32 = 190.0;

        fn content(_: &(), data: &AirlockControllerData) -> impl Scene {
            let (primary, icon, color) = airlock_status(&data.airlock_state);
            let color = color_of(color);
            let buttons: Box<dyn SceneList> = match data.airlock_state.as_str() {
                "pressurize" | "depressurize" => Box::new(bsn_list! {
                    @Button { @icon: "stop-circle", @content: "Abort" }
                    Act("abort")
                }),
                "closed" => Box::new(bsn_list! {
                    @Button { @icon: "lock-open", @content: "Open Interior Airlock" }
                    Act("cycleInterior")
                    --
                    @Button { @icon: "lock-open", @content: "Open Exterior Airlock" }
                    Act("cycleExterior")
                }),
                "inopen" => Box::new(bsn_list! {
                    @Button { @icon: "lock", @content: "Close Interior Airlock" }
                    Act("cycleClosed")
                    --
                    @Button { @icon: "sync", @content: "Cycle to Exterior Airlock" }
                    Act("cycleExterior")
                }),
                "outopen" => Box::new(bsn_list! {
                    @Button { @icon: "lock", @content: "Close Exterior Airlock" }
                    Act("cycleClosed")
                    --
                    @Button { @icon: "sync", @content: "Cycle to Interior Airlock" }
                    Act("cycleInterior")
                }),
                _ => Box::new(bsn_list! { Node }),
            };
            let pressure = match data.sensor_pressure {
                Some(p) => format!("{} kPa", (p * 10.0).round() / 10.0),
                None => "---- kPa".to_owned(),
            };
            // tg draws the icon after the number; the kit's text carries no icon here.
            let pressure = if icon.is_empty() {
                pressure
            } else {
                format!("{pressure} ({icon})")
            };
            let pump = capitalized(&data.pump_status);
            let door_color = |s: &str| {
                if s == "open" {
                    UiColor::Good
                } else {
                    UiColor::Default
                }
            };
            let (interior, interior_color) = (
                capitalized(&data.interior_status),
                door_color(&data.interior_status),
            );
            let (exterior, exterior_color) = (
                capitalized(&data.exterior_status),
                door_color(&data.exterior_status),
            );
            bsn! {
                @WindowContent
                Children [
                    @Section { @title: "Airlock Status", @buttons: {Some(buttons)} }
                    Children [
                        @LabeledList
                        Children [
                            @LabeledListItem { @label: "Current Status" }
                            Children [ Text(primary) ]
                            --
                            @LabeledListItem { @label: "Chamber Pressure" }
                            Children [
                                @UiBox { @color: color } Children [ Text(pressure) ]
                            ]
                            --
                            @LabeledListItem { @label: "Control Pump" }
                            Children [ Text(pump) ]
                            --
                            @LabeledListItem { @label: "Interior Door" }
                            Children [
                                @UiBox { @color: interior_color } Children [ Text(interior) ]
                            ]
                            --
                            @LabeledListItem { @label: "Exterior Door" }
                            Children [
                                @UiBox { @color: exterior_color } Children [ Text(exterior) ]
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
    fn data_round_trips() {
        let data = AirlockControllerData {
            airlock_state: "pressurize".into(),
            sensor_pressure: Some(42.5),
            pump_status: "release".into(),
            interior_status: "closed".into(),
            exterior_status: "----".into(),
        };
        let bytes = crate::encode(&data);
        assert_eq!(
            crate::decode::<AirlockControllerData>(&bytes).unwrap(),
            data
        );
    }

    #[test]
    fn retrieve_button_labels() {
        let mut data = AirlockButtonControllerData {
            interior_door: true,
            exterior_door: true,
            interior_door_closed: true,
            exterior_door_closed: true,
            busy: false,
        };
        assert_eq!(
            door_button(&data, true),
            ("Open interior door".into(), "open")
        );
        data.exterior_door_closed = false;
        assert_eq!(
            door_button(&data, true),
            ("Cycle to interior door".into(), "open")
        );
        assert_eq!(
            door_button(&data, false),
            ("Close exterior door".into(), "close")
        );
        assert_eq!(capitalized("siphon"), "Siphon");
        assert_eq!(airlock_status("outopen").0, "Exterior Airlock Open");
    }
}
