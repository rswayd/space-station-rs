//! `GravityGenerator`: `tgui/packages/tgui/interfaces/GravityGenerator.jsx` and
//! `/obj/machinery/gravity_generator/main/ui_data` (`code/modules/power/gravitygenerator.dm`).
//! The server half lives with the game (`ss13_game::power::gravity_generator`).

use serde::{Deserialize, Serialize};

use crate::UiInterface;

pub struct GravityGeneratorUi;

/// `ui_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct GravityGeneratorData {
    pub breaker: bool,
    /// 0..100.
    pub charge_count: u32,
    /// `POWER_IDLE` 0, `POWER_UP` 1, `POWER_DOWN` 2.
    pub charging_state: u8,
    pub on: bool,
    /// Not broken.
    pub operational: bool,
}

impl UiInterface for GravityGeneratorUi {
    const NAME: &'static str = "GravityGenerator";
    type Data = GravityGeneratorData;
    type StaticData = ();
}

#[cfg(feature = "client")]
mod client_side {
    use bevy::scene::EntityScene;
    use ss13_ui_kit::prelude::*;

    use super::{GravityGeneratorData, GravityGeneratorUi};
    use crate::client::ClientUi;

    fn colored(text: &str, color: UiColor) -> Box<dyn Scene> {
        let text = text.to_owned();
        Box::new(bsn! {
            Text(text)
            TextColor({color.base()})
        })
    }

    impl ClientUi for GravityGeneratorUi {
        const WIDTH: f32 = 400.0;
        const HEIGHT: f32 = 155.0;

        fn content(_: &(), data: &GravityGeneratorData) -> impl Scene {
            let notice = (!data.operational).then(|| {
                EntityScene(bsn! {
                    @NoticeBox
                    Children [ Text("No data available") ]
                })
            });
            let body = data.operational.then(|| {
                let (icon, text) = if data.breaker {
                    ("power-off", "On")
                } else {
                    ("times", "Off")
                };
                let selected = data.breaker;
                let ranges = vec![
                    (UiColor::Good, 0.7, f32::INFINITY),
                    (UiColor::Average, 0.3, 0.7),
                    (UiColor::Bad, f32::NEG_INFINITY, 0.3),
                ];
                let value = data.charge_count as f32 / 100.0;
                let mode = EntityScene(match data.charging_state {
                    1 => colored("Charging", UiColor::Average),
                    2 => colored("Discharging", UiColor::Average),
                    _ if data.on => colored("Fully Charged", UiColor::Good),
                    _ => colored("Not Charging", UiColor::Bad),
                });
                EntityScene(bsn! {
                    @Section
                    Children [
                        @LabeledList
                        Children [
                            @LabeledListItem { @label: "Power" }
                            Children [
                                @Button { @icon: icon, @content: text }
                                Button { selected: selected }
                                Act("gentoggle")
                            ]
                            --
                            @LabeledListItem { @label: "Gravity Charge" }
                            Children [
                                @ProgressBar
                                ProgressBar { value: value, ranges: ranges }
                                Node { flex_grow: 1.0, width: percent(100) }
                            ]
                            --
                            @LabeledListItem { @label: "Charge Mode" }
                            Children [ {mode} ]
                        ]
                    ]
                })
            });
            bsn! {
                @WindowContent
                Children [ {notice} -- {body} ]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_round_trips() {
        let data = GravityGeneratorData {
            breaker: true,
            charge_count: 42,
            charging_state: 1,
            on: false,
            operational: true,
        };
        let bytes = crate::encode(&data);
        assert_eq!(crate::decode::<GravityGeneratorData>(&bytes).unwrap(), data);
    }
}
