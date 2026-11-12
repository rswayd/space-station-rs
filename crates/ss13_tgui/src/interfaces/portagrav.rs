//! `Portagrav`: `tgui/packages/tgui/interfaces/Portagrav.tsx` and
//! `/obj/machinery/power/portagrav/ui_data` (`code/game/machinery/portagrav.dm`). The server
//! half lives with the game (`ss13_game::power::portagrav`).

use serde::{Deserialize, Serialize};

use crate::UiInterface;

pub struct PortagravUi;

/// `ui_data`. `draw` is watts (tg sends `display_power`; formatted by the client here).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct PortagravData {
    /// The cell's charge, percent; `None` without a cell.
    pub percentage: Option<f32>,
    pub gravity: f32,
    pub range: u32,
    pub maxrange: u32,
    pub on: bool,
    pub wiremode: bool,
    pub draw: f32,
}

impl UiInterface for PortagravUi {
    const NAME: &'static str = "Portagrav";
    type Data = PortagravData;
    type StaticData = ();
}

#[cfg(feature = "client")]
mod client_side {
    use ss13_ui_kit::prelude::*;

    use super::{PortagravData, PortagravUi};
    use crate::client::ClientUi;
    use crate::interfaces::solar_control::format_power;

    fn adjust(by: i32) -> ActParams {
        ActParams(vec![("adjustment".into(), by.to_string())])
    }

    impl ClientUi for PortagravUi {
        const WIDTH: f32 = 320.0;
        const HEIGHT: f32 = 320.0;

        fn content(_: &(), data: &PortagravData) -> impl Scene {
            let power = match (data.wiremode, data.percentage) {
                (false, Some(p)) => format!("{}%", p.round()),
                (true, _) => "WIRE POWERED".to_owned(),
                (false, None) => "NO CELL".to_owned(),
            };
            let top = format!("{power} - {}G", data.gravity);
            let draw = if data.on {
                format_power(data.draw)
            } else {
                "OFF".to_owned()
            };
            let bottom = format!("{draw} / Rng: {}/{}", data.range, data.maxrange);
            let wire = data.wiremode;
            let on = data.on;
            let (grav_up, grav_down, range_up, range_down) =
                (adjust(1), adjust(-1), adjust(1), adjust(-1));
            bsn! {
                @WindowContent
                Children [
                    @Section
                    Children [
                        Text(top)
                        --
                        Text(bottom)
                    ]
                    --
                    @Section
                    Children [
                        @LabeledList
                        Children [
                            @LabeledListItem { @label: "Power" }
                            Children [
                                @Button { @icon: "plug", @content: "Wire" }
                                Button { selected: wire }
                                Act("toggle_wire")
                                --
                                @Button { @icon: "power-off", @content: "Power" }
                                Button { selected: on }
                                Act("toggle_power")
                            ]
                            --
                            @LabeledListItem { @label: "Gravity Control" }
                            Children [
                                @Button { @icon: "arrow-up" }
                                Act("adjust_grav")
                                grav_up
                                --
                                @Button { @icon: "arrow-down" }
                                Act("adjust_grav")
                                grav_down
                            ]
                            --
                            @LabeledListItem { @label: "Range Control" }
                            Children [
                                @Button { @icon: "arrow-up" }
                                Act("adjust_range")
                                range_up
                                --
                                @Button { @icon: "arrow-down" }
                                Act("adjust_range")
                                range_down
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
        let data = PortagravData {
            percentage: Some(87.0),
            gravity: 1.0,
            range: 4,
            maxrange: 6,
            on: true,
            wiremode: false,
            draw: 4000.0,
        };
        let bytes = crate::encode(&data);
        assert_eq!(crate::decode::<PortagravData>(&bytes).unwrap(), data);
    }
}
