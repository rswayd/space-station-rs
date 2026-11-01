//! `DisposalUnit` (disposal bin): `tgui/packages/tgui/interfaces/DisposalUnit.tsx` and
//! `/obj/machinery/disposal/bin/ui_data` (`code/modules/recycling/disposal/bin.dm`). The server
//! half lives with the game (`ss13_game::disposals`); this module has the shared data and the
//! client view.

use serde::{Deserialize, Serialize};

use crate::UiInterface;

pub struct DisposalUnitUi;

/// `ui_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct DisposalUnitData {
    pub flush: bool,
    pub full_pressure: bool,
    pub pressure_charging: bool,
    pub panel_open: bool,
    /// `CLAMP01(pressure / SEND_PRESSURE)`.
    pub per: f32,
    pub isai: bool,
}

impl UiInterface for DisposalUnitUi {
    const NAME: &'static str = "DisposalUnit";
    type Data = DisposalUnitData;
    type StaticData = ();
}

/// The "State" row: text and its tgui color class.
pub fn state(data: &DisposalUnitData) -> (&'static str, &'static str) {
    if data.full_pressure {
        ("Ready", "good")
    } else if data.panel_open {
        ("Power Disabled", "bad")
    } else if data.pressure_charging {
        ("Pressurizing", "average")
    } else {
        ("Off", "bad")
    }
}

#[cfg(feature = "client")]
mod client_side {
    use ss13_ui_kit::prelude::*;

    use super::{DisposalUnitData, DisposalUnitUi, state};
    use crate::client::ClientUi;

    impl ClientUi for DisposalUnitUi {
        const WIDTH: f32 = 300.0;
        const HEIGHT: f32 = 180.0;

        fn content(_: &(), data: &DisposalUnitData) -> impl Scene {
            let (state_text, class) = state(data);
            let state_color = match class {
                "good" => UiColor::Good,
                "average" => UiColor::Average,
                _ => UiColor::Bad,
            };
            let per = data.per;
            let (handle_icon, handle_text, handle_act) = if data.flush {
                ("toggle-on", "Disengage", "handle-0")
            } else {
                ("toggle-off", "Engage", "handle-1")
            };
            let pump_act = if data.pressure_charging {
                "pump-0"
            } else {
                "pump-1"
            };
            let handle_disabled = data.isai || data.panel_open;
            let eject_disabled = data.isai;
            let (pump_disabled, pump_selected) = (data.panel_open, data.pressure_charging);
            bsn! {
                @WindowContent
                Children [
                    @Section
                    Children [
                        @LabeledList
                        Children [
                            @LabeledListItem {
                                @label: "State",
                                @color: state_color,
                                @content: state_text,
                            }
                            --
                            @LabeledListItem { @label: "Pressure" }
                            Children [
                                @ProgressBar
                                ProgressBar { value: per, color: UiColor::Good }
                            ]
                            --
                            @LabeledListItem { @label: "Handle" }
                            Children [
                                @Button { @icon: handle_icon, @content: handle_text }
                                Button { disabled: handle_disabled }
                                Act(handle_act)
                            ]
                            --
                            @LabeledListItem { @label: "Eject" }
                            Children [
                                @Button { @icon: "sign-out-alt", @content: "Eject Contents" }
                                Button { disabled: eject_disabled }
                                Act("eject")
                            ]
                            --
                            @LabeledListItem { @label: "Power" }
                            Children [
                                @Button { @icon: "power-off" }
                                Button { disabled: pump_disabled, selected: pump_selected }
                                Act(pump_act)
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
    fn state_text_follows_the_tsx_order() {
        let mut data = DisposalUnitData {
            full_pressure: true,
            panel_open: true,
            ..Default::default()
        };
        assert_eq!(state(&data), ("Ready", "good"));
        data.full_pressure = false;
        assert_eq!(state(&data).0, "Power Disabled");
        data.panel_open = false;
        data.pressure_charging = true;
        assert_eq!(state(&data), ("Pressurizing", "average"));
        data.pressure_charging = false;
        assert_eq!(state(&data), ("Off", "bad"));
    }
}
