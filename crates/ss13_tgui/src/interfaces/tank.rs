//! `Tank`: `tgui/packages/tgui/interfaces/Tank.jsx` and `/obj/item/tank/ui_data`,
//! `ui_static_data` (`code/game/objects/items/tanks/tanks.dm`). The server half lives with the
//! game (`ss13_game::internals`).
// shortcut: `RoundGauge` (the pressure dial) is coloured text like `ChemHeater`'s; add the dial
// with the widget.

use serde::{Deserialize, Serialize};

use crate::UiInterface;

pub struct TankUi;

/// `ui_static_data`, rounded kPa.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct TankStatic {
    pub default_release_pressure: f32,
    pub min_release_pressure: f32,
    pub max_release_pressure: f32,
    pub leak_pressure: f32,
    pub fragment_pressure: f32,
}

/// `ui_data`, rounded kPa.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct TankData {
    pub tank_pressure: f32,
    pub release_pressure: f32,
    pub connected: bool,
}

impl UiInterface for TankUi {
    const NAME: &'static str = "Tank";
    type Data = TankData;
    type StaticData = TankStatic;
}

#[cfg(feature = "client")]
mod client_side {
    use bevy::scene::EntityScene;
    use ss13_ui_kit::ActParams;
    use ss13_ui_kit::prelude::*;

    use super::{TankData, TankStatic, TankUi};
    use crate::client::{ActOnChange, ClientUi};

    fn pressure_button(
        pressure: &'static str,
        icon: &'static str,
        disabled: bool,
    ) -> Box<dyn Scene> {
        let params = ActParams(vec![("pressure".into(), pressure.to_owned())]);
        Box::new(bsn! {
            @Button { @icon: icon }
            Button { disabled: disabled }
            Act("pressure")
            params
        })
    }

    impl ClientUi for TankUi {
        const WIDTH: f32 = 275.0;
        const HEIGHT: f32 = 120.0;

        fn content(s: &TankStatic, data: &TankData) -> impl Scene {
            let gauge_color = if data.tank_pressure >= s.fragment_pressure {
                UiColor::Bad
            } else if data.tank_pressure >= s.leak_pressure {
                UiColor::Average
            } else {
                UiColor::Good
            };
            let gauge = format!("{} kPa", data.tank_pressure);
            let input = NumberInput {
                value: f64::from(data.release_pressure),
                min_value: f64::from(s.min_release_pressure),
                max_value: f64::from(s.max_release_pressure),
                step: 1.0,
                unit: "kPa".into(),
                ..Default::default()
            };
            let on_change = ActOnChange {
                action: "pressure".into(),
                param: "pressure".into(),
            };
            let min = pressure_button(
                "min",
                "fast-backward",
                data.release_pressure == s.min_release_pressure,
            );
            let max = pressure_button(
                "max",
                "fast-forward",
                data.release_pressure == s.max_release_pressure,
            );
            let reset = pressure_button(
                "reset",
                "undo",
                data.release_pressure == s.default_release_pressure,
            );
            let number: Box<dyn Scene> = Box::new(bsn! {
                Node { width: px(65) }
                input
                on_change
            });
            let regulator: Vec<Box<dyn Scene>> = vec![min, number, max, reset];
            let pressure: Box<dyn Scene> = Box::new(bsn! {
                Text(gauge)
                TextColor({gauge_color.base()})
            });
            bsn! {
                @WindowContent
                Children [
                    @Section
                    Children [
                        @LabeledList
                        Children [
                            @LabeledListItem { @label: "Pressure" }
                            Children [ {EntityScene(pressure)} ]
                            --
                            @LabeledListItem { @label: "Pressure Regulator" }
                            Children [ {regulator} ]
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
        let data = TankData {
            tank_pressure: 1013.0,
            release_pressure: 16.0,
            connected: true,
        };
        let bytes = crate::encode(&data);
        assert_eq!(crate::decode::<TankData>(&bytes).unwrap(), data);
    }
}
