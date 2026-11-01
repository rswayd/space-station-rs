//! `MechBayPowerConsole` (mech bay power control console):
//! `tgui/packages/tgui/interfaces/MechBayPowerConsole.jsx` and
//! `/obj/machinery/computer/mech_bay_power_console/ui_data` (`code/modules/vehicles/mecha/
//! mech_bay.dm`). The server half lives with the game (`ss13_game::mecha::bay`).

use serde::{Deserialize, Serialize};

use crate::UiInterface;

pub struct MechBayPowerConsoleUi;

/// `ui_data`: `recharge_port` is absent without a linked port.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct MechBayData {
    pub recharge_port: Option<RechargePort>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct RechargePort {
    /// The mech on the port's charging tile.
    pub mech: Option<BayMech>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct BayMech {
    pub health: f32,
    pub maxhealth: f32,
    pub cell: Option<BayCell>,
    pub name: String,
}

/// Joules.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub struct BayCell {
    pub charge: f32,
    pub maxcharge: f32,
}

impl UiInterface for MechBayPowerConsoleUi {
    const NAME: &'static str = "MechBayPowerConsole";
    type Data = MechBayData;
    type StaticData = ();
}

#[cfg(feature = "client")]
mod client_side {
    use bevy::scene::EntityScene;
    use ss13_ui_kit::prelude::*;

    use super::{MechBayData, MechBayPowerConsoleUi};
    use crate::client::ClientUi;
    use crate::interfaces::solar_control::format_energy;

    impl ClientUi for MechBayPowerConsoleUi {
        const WIDTH: f32 = 400.0;
        const HEIGHT: f32 = 200.0;

        fn content(_: &(), data: &MechBayData) -> impl Scene {
            let mech = data.recharge_port.as_ref().and_then(|p| p.mech.as_ref());
            let missing = if data.recharge_port.is_none() {
                Some("No power port detected. Please re-sync.")
            } else if mech.is_none() {
                Some("No mech detected.")
            } else {
                None
            };
            let integrity = match (missing, mech) {
                (None, Some(m)) => bar(m.health / m.maxhealth.max(1.0), String::new()),
                _ => notice(missing.unwrap_or_default()),
            };
            let power = match (missing, mech.map(|m| m.cell)) {
                (None, Some(Some(cell))) => bar(
                    cell.charge / cell.maxcharge.max(1.0),
                    format!(
                        "{}/{}",
                        format_energy(cell.charge),
                        format_energy(cell.maxcharge)
                    ),
                ),
                (None, _) => notice("No cell is installed."),
                _ => notice(missing.unwrap_or_default()),
            };
            bsn! {
                @WindowContent
                Children [
                    @Section {
                        @title: "Mech status",
                        @buttons: bsn! {
                            @Button { @icon: "sync", @content: "Sync" }
                            Act("reconnect")
                        },
                    }
                    Children [
                        @LabeledList
                        Children [
                            @LabeledListItem { @label: "Integrity" }
                            Children [ {EntityScene(integrity)} ]
                            --
                            @LabeledListItem { @label: "Power" }
                            Children [ {EntityScene(power)} ]
                        ]
                    ]
                ]
            }
        }
    }

    fn notice(text: &str) -> Box<dyn Scene> {
        let text = text.to_owned();
        Box::new(bsn! {
            @NoticeBox
            Children [ Text(text) ]
        })
    }

    /// `ranges={{ good: [0.7, Infinity], average: [0.3, 0.7], bad: [-Infinity, 0.3] }}`.
    fn bar(value: f32, text: String) -> Box<dyn Scene> {
        let ranges = vec![
            (UiColor::Good, 0.7, f32::INFINITY),
            (UiColor::Average, 0.3, 0.7),
            (UiColor::Bad, f32::NEG_INFINITY, 0.3),
        ];
        Box::new(bsn! {
            @ProgressBar { @empty: true }
            ProgressBar { value: value, ranges: ranges }
            Node { flex_grow: 1.0, width: percent(100) }
            Children [ Text(text) ]
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interfaces::solar_control::format_energy;

    #[test]
    fn data_round_trips_and_energy_formats() {
        let data = MechBayData {
            recharge_port: Some(RechargePort {
                mech: Some(BayMech {
                    health: 200.0,
                    maxhealth: 200.0,
                    cell: Some(BayCell {
                        charge: 5000.0,
                        maxcharge: 100_000.0,
                    }),
                    name: "APLU MK-I \"Ripley\"".into(),
                }),
            }),
        };
        let bytes = crate::encode(&data);
        assert_eq!(crate::decode::<MechBayData>(&bytes).unwrap(), data);
        assert_eq!(format_energy(100_000.0), "100 kJ");
        assert_eq!(format_energy(5000.0), "5.00 kJ");
    }
}
