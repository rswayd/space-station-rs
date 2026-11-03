//! `Electrolyzer`: `tgui/packages/tgui/interfaces/Electrolyzer.tsx` and
//! `/obj/machinery/electrolyzer/ui_data` (`code/modules/atmospherics/machinery/components/
//! electrolyzer/electrolyzer.dm`). The server half lives with the game
//! (`ss13_game::atmos_machinery::electrolyzer`).

use serde::{Deserialize, Serialize};

use crate::UiInterface;

pub struct ElectrolyzerUi;

/// `ui_data`; `power_level` (`round(cell.percent())`) only with a cell.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct ElectrolyzerData {
    pub open: bool,
    pub on: bool,
    pub has_powercell: bool,
    pub anchored: bool,
    pub power_level: Option<f32>,
}

impl UiInterface for ElectrolyzerUi {
    const NAME: &'static str = "Electrolyzer";
    type Data = ElectrolyzerData;
    type StaticData = ();
}

#[cfg(feature = "client")]
mod client_side {
    use bevy::scene::EntityScene;
    use ss13_ui_kit::prelude::*;

    use super::{ElectrolyzerData, ElectrolyzerUi};
    use crate::client::ClientUi;

    impl ClientUi for ElectrolyzerUi {
        const WIDTH: f32 = 400.0;
        const HEIGHT: f32 = 305.0;

        fn content(_: &(), data: &ElectrolyzerData) -> impl Scene {
            let eject_disabled = !data.has_powercell || !data.open;
            let (power_icon, power_text) = if data.on {
                ("power-off", "On")
            } else {
                ("times", "Off")
            };
            let power_selected = data.on;
            let power_disabled = !data.has_powercell && !data.anchored;
            let cell: Box<dyn Scene> = match data.power_level.filter(|_| data.has_powercell) {
                Some(level) => {
                    // `ranges={{ good: [0.6, Infinity], average: [0.3, 0.6], bad: [...] }}`.
                    let ranges = vec![
                        (UiColor::Good, 0.6, f32::INFINITY),
                        (UiColor::Average, 0.3, 0.6),
                        (UiColor::Bad, f32::NEG_INFINITY, 0.3),
                    ];
                    let value = level / 100.0;
                    Box::new(bsn! {
                        @ProgressBar
                        ProgressBar { value: value, ranges: ranges }
                        Node { flex_grow: 1.0, width: percent(100) }
                    })
                }
                None => Box::new(bsn! {
                    Text("None")
                    TextColor({UiColor::Bad.base()})
                }),
            };
            bsn! {
                @WindowContent
                Children [
                    @Section {
                        @title: "Power",
                        @buttons: bsn_list! {
                            @Button { @icon: "eject", @content: "Eject Cell" }
                            Button { disabled: eject_disabled }
                            Act("eject")
                            --
                            @Button { @icon: power_icon, @content: power_text }
                            Button { selected: power_selected, disabled: power_disabled }
                            Act("power")
                        },
                    }
                    Children [
                        @LabeledList
                        Children [
                            @LabeledListItem { @label: "Cell" }
                            Children [ {EntityScene(cell)} ]
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
        let data = ElectrolyzerData {
            open: true,
            on: false,
            has_powercell: true,
            anchored: false,
            power_level: Some(42.0),
        };
        let bytes = crate::encode(&data);
        assert_eq!(crate::decode::<ElectrolyzerData>(&bytes).unwrap(), data);
    }
}
