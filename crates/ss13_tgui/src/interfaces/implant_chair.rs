//! `ImplantChair`: `tgui/packages/tgui/interfaces/ImplantChair.jsx` and
//! `/obj/machinery/implantchair/ui_data` (`code/game/objects/items/implants/implantchair.dm`).
//! The server half lives with the game (`ss13_game::implants::chair`).
// shortcut: no spinning cog and sync icons beside the implant button and the count.

use serde::{Deserialize, Serialize};

use crate::UiInterface;

pub struct ImplantChairUi;

/// `occupant`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct ImplantChairOccupant {
    pub name: String,
    /// `stat`: 0 conscious, 1 unconscious (soft or hard crit), 2 dead.
    pub stat: u8,
}

/// `ui_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct ImplantChairData {
    pub occupied: bool,
    pub open: bool,
    pub occupant: Option<ImplantChairOccupant>,
    /// `special_name` of the genetic purifier and the neural imprinter.
    pub special_name: Option<String>,
    pub ready_implants: u32,
    pub ready: bool,
    pub replenishing: bool,
}

impl UiInterface for ImplantChairUi {
    const NAME: &'static str = "ImplantChair";
    type Data = ImplantChairData;
    type StaticData = ();
}

/// The status line of the occupant: text and tgui color name.
pub fn stat_line(stat: u8) -> (&'static str, &'static str) {
    match stat {
        0 => ("Conscious", "good"),
        1 => ("Unconscious", "average"),
        _ => ("Dead", "bad"),
    }
}

#[cfg(feature = "client")]
mod client_side {
    use ss13_ui_kit::UiColor;
    use ss13_ui_kit::prelude::*;

    use super::{ImplantChairData, ImplantChairUi, stat_line};
    use crate::client::ClientUi;

    impl ClientUi for ImplantChairUi {
        const WIDTH: f32 = 375.0;
        const HEIGHT: f32 = 280.0;

        fn content(_: &(), data: &ImplantChairData) -> impl Scene {
            let name = data
                .occupant
                .as_ref()
                .map_or_else(|| "No Occupant".to_owned(), |o| o.name.clone());
            let status: Vec<_> = data
                .occupant
                .as_ref()
                .filter(|_| data.occupied)
                .map(|o| {
                    let (text, color) = stat_line(o.stat);
                    let color = match color {
                        "good" => UiColor::Good,
                        "average" => UiColor::Average,
                        _ => UiColor::Bad,
                    };
                    bsn! { @LabeledListItem { @label: "Status", @color: color, @content: text } }
                })
                .into_iter()
                .collect();
            let (door_icon, door_text) = if data.open {
                ("unlock", "Open")
            } else {
                ("lock", "Closed")
            };
            let implant_text = if data.ready {
                data.special_name
                    .clone()
                    .unwrap_or_else(|| "Implant".to_owned())
            } else {
                "Recharging".to_owned()
            };
            let remaining = data.ready_implants.to_string();
            bsn! {
                @WindowContent
                Children [
                    @Section { @title: "Occupant Information" }
                    Children [
                        @LabeledList
                        Children [
                            @LabeledListItem { @label: "Name", @content: name }
                            --
                            {status}
                        ]
                    ]
                    --
                    @Section { @title: "Operations" }
                    Children [
                        @LabeledList
                        Children [
                            @LabeledListItem { @label: "Door" }
                            Children [
                                @Button { @icon: door_icon, @content: door_text }
                                Act("door")
                            ]
                            --
                            @LabeledListItem { @label: "Implant Occupant" }
                            Children [
                                @Button { @icon: "code-branch", @content: implant_text }
                                Act("implant")
                            ]
                            --
                            @LabeledListItem { @label: "Implants Remaining", @content: remaining }
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
        let data = ImplantChairData {
            occupied: true,
            open: false,
            occupant: Some(ImplantChairOccupant {
                name: "John".into(),
                stat: 1,
            }),
            special_name: Some("Imprint".into()),
            ready_implants: 4,
            ready: false,
            replenishing: true,
        };
        let bytes = crate::encode(&data);
        assert_eq!(crate::decode::<ImplantChairData>(&bytes).unwrap(), data);
        assert_eq!(stat_line(1), ("Unconscious", "average"));
    }
}
