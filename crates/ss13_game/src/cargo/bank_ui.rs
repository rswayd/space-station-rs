//! The vault's bank machine window: tgui `BankMachine.tsx`. `bank_machine` serves it; the view
//! is drawn here on the client.

use serde::{Deserialize, Serialize};
use ss13_tgui::UiInterface;

/// tgui `BankMachine`.
pub struct BankMachineUi;

impl UiInterface for BankMachineUi {
    const NAME: &'static str = "BankMachine";
    type Data = BankMachineData;
    type StaticData = ();
}

/// `/obj/machinery/computer/bank_machine/ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BankMachineData {
    pub current_balance: i64,
    pub siphoning: bool,
    pub station_name: String,
}

pub const SIPHON: &str = "siphon";
pub const HALT: &str = "halt";

#[cfg(feature = "client")]
mod client_side {
    use bevy::scene::{EntityScene, SceneList};
    use ss13_tgui::client::ClientUi;
    use ss13_ui_kit::prelude::*;

    use super::*;
    use crate::cargo::client::format_money;

    impl ClientUi for BankMachineUi {
        const WIDTH: f32 = 350.0;
        const HEIGHT: f32 = 155.0;

        fn content(_: &(), data: &BankMachineData) -> impl Scene {
            let title = format!("{} Vault", data.station_name);
            let balance = format!("{} cr", format_money(data.current_balance));
            let siphoning = data.siphoning;
            let (icon, label, act) = if siphoning {
                ("times", "Stop Siphoning", HALT)
            } else {
                ("sync", "Siphon Credits", SIPHON)
            };
            let buttons: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
                @ss13_ui_kit::Button { @icon: icon, @content: label }
                ss13_ui_kit::Button { selected: siphoning }
                Act({act.to_owned()})
            }));
            bsn! {
                @WindowContent
                Children [
                    @NoticeBox { @danger: true }
                    Children [ Text({"Authorized personnel only".to_owned()}) ]
                    --
                    @Section { @title: title }
                    Children [
                        @LabeledList
                        Children [
                            @LabeledListItem { @label: "Current Balance", @buttons: buttons }
                            Children [ Text(balance) ]
                        ]
                    ]
                ]
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn view_builds() {
            let mut data = BankMachineData {
                current_balance: 12345,
                siphoning: false,
                station_name: "Космическая станция 13".into(),
            };
            let _ = BankMachineUi::content(&(), &data);
            data.siphoning = true;
            let _ = BankMachineUi::content(&(), &data);
        }
    }
}
