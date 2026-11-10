//! `ImplantPad`: `tgui/packages/tgui/interfaces/ImplantPad.tsx` and `/obj/item/implantpad/ui_data`
//! (`code/game/objects/items/implants/implantpad.dm`). The server half lives with the game
//! (`ss13_game::implants::pad`).
// shortcut: the extended information (`implant_lore`) is not ported; both sections are always
// open (no `Collapsible`).

use serde::{Deserialize, Serialize};

use crate::UiInterface;

pub struct ImplantPadUi;

/// `ui_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct ImplantPadData {
    pub has_case: bool,
    pub has_implant: bool,
    pub case_information: String,
    pub saved_deathrattle_group: Option<String>,
    pub current_deathrattle_group: Option<String>,
}

impl UiInterface for ImplantPadUi {
    const NAME: &'static str = "ImplantPad";
    type Data = ImplantPadData;
    type StaticData = ();
}

pub const ACT_EJECT: &str = "eject_implant";
pub const ACT_SAVE_GROUP: &str = "save_deathrattle_group";
pub const ACT_SET_GROUP: &str = "set_deathrattle_group";
pub const ACT_INIT_GROUP: &str = "init_deathrattle_group";

/// The information section's text.
pub fn information(data: &ImplantPadData) -> String {
    if !data.has_case {
        "No implant case detected. Please insert one to see its contents.".to_owned()
    } else if !data.has_implant {
        "Implant case does not have an implant. Please insert an implant to continue.".to_owned()
    } else {
        data.case_information.clone()
    }
}

#[cfg(feature = "client")]
mod client_side {
    use ss13_ui_kit::prelude::*;

    use super::{
        ACT_EJECT, ACT_INIT_GROUP, ACT_SAVE_GROUP, ACT_SET_GROUP, ImplantPadData, ImplantPadUi,
        information,
    };
    use crate::client::ClientUi;

    impl ClientUi for ImplantPadUi {
        const WIDTH: f32 = 300.0;
        const HEIGHT: f32 = 350.0;

        fn content(_: &(), data: &ImplantPadData) -> impl Scene {
            let no_case = !data.has_case;
            let has_group = data.current_deathrattle_group.is_some();
            let saved = format!(
                "Saved Deathrattle Group: {}",
                data.saved_deathrattle_group.as_deref().unwrap_or("None")
            );
            let current = format!(
                "Current Deathrattle Group: {}",
                data.current_deathrattle_group.as_deref().unwrap_or("None")
            );
            let info = information(data);
            let save_disabled = !has_group;
            let set_disabled = has_group || no_case;
            let init_disabled = has_group || no_case;
            bsn! {
                @WindowContent
                Children [
                    @Section { @title: "Implant Mini-Computer" }
                    Children [
                        @Button { @icon: "eject", @content: "Eject Case" }
                        Button { disabled: no_case }
                        Act(ACT_EJECT)
                    ]
                    --
                    @Section
                    Children [
                        Text(saved)
                        --
                        Text(current)
                        --
                        Node { flex_direction: FlexDirection::Row }
                        Children [
                            @Button { @content: "Save Group" }
                            Button { disabled: save_disabled }
                            Act(ACT_SAVE_GROUP)
                            --
                            @Button { @content: "Set Group" }
                            Button { disabled: set_disabled }
                            Act(ACT_SET_GROUP)
                            --
                            @Button { @content: "Initialize Group" }
                            Button { disabled: init_disabled }
                            Act(ACT_INIT_GROUP)
                        ]
                    ]
                    --
                    @Section { @title: "Implant Information" }
                    Children [ Text(info) ]
                ]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn information_follows_the_case() {
        let mut data = ImplantPadData::default();
        assert!(information(&data).starts_with("No implant case"));
        data.has_case = true;
        assert!(information(&data).starts_with("Implant case does not"));
        data.has_implant = true;
        data.case_information = "info".into();
        assert_eq!(information(&data), "info");
        let bytes = crate::encode(&data);
        assert_eq!(crate::decode::<ImplantPadData>(&bytes).unwrap(), data);
    }
}
