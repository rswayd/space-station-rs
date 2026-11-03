//! `AtmosAlertConsole` (`tgui/packages/tgui/interfaces/AtmosAlertConsole.tsx`): the areas whose
//! air alarms are in hazard (priority) or warning (minor); a button clears one until the next
//! check. Server half: `ss13_game::atmos_machinery::consoles`.

use serde::{Deserialize, Serialize};

use crate::UiInterface;

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct AtmosAlertData {
    pub priority: Vec<String>,
    pub minor: Vec<String>,
}

pub struct AtmosAlertUi;

impl UiInterface for AtmosAlertUi {
    const NAME: &'static str = "AtmosAlertConsole";
    type Data = AtmosAlertData;
    type StaticData = ();
}

#[cfg(feature = "client")]
mod client_side {
    use bevy::app::Propagate;
    use ss13_ui_kit::prelude::*;
    use ss13_ui_kit::{ActParams, UiColor};

    use super::*;
    use crate::client::ClientUi;

    fn rows(zones: &[String], color: UiColor, empty: &'static str) -> Vec<Box<dyn SceneList>> {
        if zones.is_empty() {
            let good = UiColor::Good.base();
            return vec![Box::new(bevy::scene::EntityScene(bsn! {
                Node Propagate::<TextColor>(TextColor(good)) Children [ Text(empty) ]
            }))];
        }
        zones
            .iter()
            .map(|zone| -> Box<dyn SceneList> {
                let params = ActParams(vec![("zone".to_owned(), zone.clone())]);
                let zone = zone.clone();
                Box::new(bevy::scene::EntityScene(bsn! {
                    Node { margin: UiRect::bottom(px(2)) }
                    Children [
                        @ss13_ui_kit::Button { @icon: "times", @content: zone }
                        ss13_ui_kit::Button { color: color }
                        Act("clear")
                        params
                    ]
                }))
            })
            .collect()
    }

    impl ClientUi for AtmosAlertUi {
        const WIDTH: f32 = 350.0;
        const HEIGHT: f32 = 300.0;

        fn content(_: &(), data: &AtmosAlertData) -> impl Scene {
            let priority = rows(&data.priority, UiColor::Bad, "No priority alerts");
            let minor = rows(&data.minor, UiColor::Average, "No minor alerts");
            bsn! {
                @WindowContent
                Children [
                    @Section { @title: "Alarms", @scrollable: true }
                    Children [
                        @Stack { @vertical: true }
                        Children [ {priority} --  {minor} ]
                    ]
                ]
            }
        }
    }
}
