//! `Holodeck` (`tgui/packages/tgui/interfaces/Holodeck.jsx`): the program buttons of a holodeck
//! control console, the dangerous ones once emagged, and the safeties toggle for silicons.
//! Server half: `ss13_game::holodeck`.

use serde::{Deserialize, Serialize};

use crate::UiInterface;

/// One entry of `program_cache` / `emag_programs`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct HoloProgram {
    pub id: String,
    pub name: String,
}

/// `ui_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct HolodeckData {
    pub default_programs: Vec<HoloProgram>,
    pub emagged: bool,
    /// Empty unless emagged.
    pub emag_programs: Vec<HoloProgram>,
    pub program: String,
    pub can_toggle_safety: bool,
}

pub struct HolodeckUi;

impl UiInterface for HolodeckUi {
    const NAME: &'static str = "Holodeck";
    type Data = HolodeckData;
    type StaticData = ();
}

/// `name.substring(11)`: "Holodeck - Beach" shows as "Beach".
pub fn short_name(name: &str) -> &str {
    name.get(11..).unwrap_or(name)
}

#[cfg(feature = "client")]
mod client_side {
    use ss13_ui_kit::prelude::*;
    use ss13_ui_kit::{ActParams, UiColor};

    use super::*;
    use crate::client::ClientUi;

    fn buttons(programs: &[HoloProgram], current: &str, color: UiColor) -> Vec<Box<dyn SceneList>> {
        programs
            .iter()
            .map(|p| -> Box<dyn SceneList> {
                let params = ActParams(vec![("id".to_owned(), p.id.clone())]);
                let label = short_name(&p.name).to_owned();
                let selected = p.id == current;
                Box::new(bevy::scene::EntityScene(bsn! {
                    @ss13_ui_kit::Button { @content: label, @fluid: true }
                    ss13_ui_kit::Button { color: color, selected: selected }
                    Act("load_program")
                    params
                }))
            })
            .collect()
    }

    impl ClientUi for HolodeckUi {
        const WIDTH: f32 = 400.0;
        const HEIGHT: f32 = 500.0;

        fn content(_: &(), data: &HolodeckData) -> impl Scene {
            let defaults = buttons(&data.default_programs, &data.program, UiColor::Default);
            let lock = if data.emagged { "unlock" } else { "lock" };
            let safe = !data.emagged;
            let locked = !data.can_toggle_safety;
            let danger: Vec<Box<dyn SceneList>> = if data.emagged {
                let list = buttons(&data.emag_programs, &data.program, UiColor::Bad);
                vec![Box::new(bevy::scene::EntityScene(bsn! {
                    @Section { @title: "Dangerous Programs" }
                    Children [ {list} ]
                }))]
            } else {
                Vec::new()
            };
            bsn! {
                @WindowContent
                Children [
                    @Section {
                        @title: "Default Programs",
                        @scrollable: true,
                        @buttons: bsn! {
                            @ss13_ui_kit::Button { @icon: lock, @content: "Safeties" }
                            ss13_ui_kit::Button { color: UiColor::Bad, selected: safe, disabled: locked }
                            Act("safety")
                        },
                    }
                    Children [ {defaults} ]
                    --
                    {danger}
                ]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_lose_the_holodeck_prefix() {
        assert_eq!(short_name("Holodeck - Beach"), "Beach");
        assert_eq!(short_name("Short"), "Short");
    }
}
