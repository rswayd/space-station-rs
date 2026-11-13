//! `ExperimentalCloner`: `tgui/packages/tgui/interfaces/ExperimentalCloner.tsx` and
//! `/obj/machinery/computer/experimental_cloner/ui_data`
//! (`code/game/machinery/experimental_cloner/experimental_cloner_computer.dm`). The server half
//! lives with the game (`ss13_game::genetics::cloner`).
// shortcut: no `ntOS95` theme, the stored record's Clear button sits in the section body.

use serde::{Deserialize, Serialize};

use crate::UiInterface;

pub struct ExperimentalClonerUi;

/// `ui_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct ExperimentalClonerData {
    pub record_name: Option<String>,
    pub record_species: Option<String>,
    pub linked_scanner: bool,
    pub scanner_occupant: Option<String>,
    pub scanner_species: Option<String>,
    pub is_scanning: bool,
    pub linked_pod: bool,
    pub is_cloning: bool,
    pub cloning_name: Option<String>,
    pub cloning_species: Option<String>,
    /// 0..=100.
    pub cloning_progress: f32,
}

impl UiInterface for ExperimentalClonerUi {
    const NAME: &'static str = "ExperimentalCloner";
    type Data = ExperimentalClonerData;
    type StaticData = ();
}

pub const ACT_CLEAR_RECORD: &str = "clear_record";
pub const ACT_START_SCAN: &str = "start_scan";
pub const ACT_START_CLONE: &str = "start_clone";

/// `name (species ?? 'Unknown')`.
pub fn name_and_species(name: &str, species: Option<&str>) -> String {
    format!("{name} ({})", species.unwrap_or("Unknown"))
}

#[cfg(feature = "client")]
mod client_side {
    use bevy::scene::EntityScene;
    use ss13_ui_kit::prelude::*;

    use super::{
        ACT_CLEAR_RECORD, ACT_START_CLONE, ACT_START_SCAN, ExperimentalClonerData,
        ExperimentalClonerUi, name_and_species,
    };
    use crate::client::ClientUi;

    fn scanner_section(data: &ExperimentalClonerData) -> impl Scene + use<> {
        let body: Box<dyn Scene> = if !data.linked_scanner {
            Box::new(bsn! { Text("No scanner connected.") })
        } else {
            let occupant = data
                .scanner_occupant
                .clone()
                .unwrap_or_else(|| "None".to_owned());
            let scan: Box<dyn Scene> = match &data.scanner_occupant {
                Some(_) => {
                    let species = data
                        .scanner_species
                        .clone()
                        .unwrap_or_else(|| "Unknown".to_owned());
                    let label = if data.is_scanning {
                        "Scanning..."
                    } else {
                        "Scan Now"
                    };
                    let scanning = data.is_scanning;
                    Box::new(bsn! {
                        Node { flex_direction: FlexDirection::Column }
                        Children [
                            Text("Occupant Species:")
                            --
                            Text(species)
                            --
                            @Button { @content: label }
                            Button { disabled: scanning }
                            Act(ACT_START_SCAN)
                        ]
                    })
                }
                None => Box::new(bsn! { Node }),
            };
            Box::new(bsn! {
                Node { flex_direction: FlexDirection::Column }
                Children [
                    Text("Current Occupant:")
                    --
                    Text(occupant)
                    --
                    {EntityScene(scan)}
                ]
            })
        };
        bsn! {
            Node { width: percent(50) }
            Children [
                @Section { @title: "Scanner" }
                Children [ {EntityScene(body)} ]
            ]
        }
    }

    fn pod_section(data: &ExperimentalClonerData) -> impl Scene + use<> {
        let body: Box<dyn Scene> = if !data.linked_pod {
            Box::new(bsn! { Text("No pod connected.") })
        } else if data.is_cloning {
            let who = name_and_species(
                data.cloning_name.as_deref().unwrap_or_default(),
                data.cloning_species.as_deref(),
            );
            let fill = (data.cloning_progress / 100.0).clamp(0.0, 1.0);
            let kickstart = if data.cloning_progress >= 100.0 {
                "Beginning neural kickstart..."
            } else {
                ""
            };
            Box::new(bsn! {
                Node { flex_direction: FlexDirection::Column }
                Children [
                    Text("Currently Cloning:")
                    --
                    Text(who)
                    --
                    @ProgressBar
                    ProgressBar { value: fill, color: UiColor::Good }
                    --
                    Text(kickstart)
                ]
            })
        } else {
            let no_record = data.record_name.is_none();
            Box::new(bsn! {
                @Button { @content: "Begin Cloning" }
                Button { disabled: no_record }
                Act(ACT_START_CLONE)
            })
        };
        bsn! {
            Node { width: percent(50) }
            Children [
                @Section { @title: "Cloning Pod" }
                Children [ {EntityScene(body)} ]
            ]
        }
    }

    impl ClientUi for ExperimentalClonerUi {
        const WIDTH: f32 = 500.0;
        const HEIGHT: f32 = 300.0;

        fn content(_: &(), data: &ExperimentalClonerData) -> impl Scene {
            let record = match &data.record_name {
                Some(name) => name_and_species(name, data.record_species.as_deref()),
                None => "No stored DNA on record.".to_owned(),
            };
            let no_record = data.record_name.is_none();
            let (scanner, pod) = (scanner_section(data), pod_section(data));
            bsn! {
                @WindowContent
                Children [
                    @Section { @title: "Stored Subject" }
                    Children [
                        Text(record)
                        --
                        @Button { @icon: "x", @content: "Clear" }
                        Button { disabled: no_record }
                        Act(ACT_CLEAR_RECORD)
                    ]
                    --
                    Node { flex_direction: FlexDirection::Row, column_gap: px(4) }
                    Children [ {EntityScene(scanner)} -- {EntityScene(pod)} ]
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
        let data = ExperimentalClonerData {
            record_name: Some("John Doe".into()),
            record_species: None,
            linked_scanner: true,
            scanner_occupant: None,
            scanner_species: None,
            is_scanning: false,
            linked_pod: true,
            is_cloning: true,
            cloning_name: Some("John Doe".into()),
            cloning_species: Some("Human".into()),
            cloning_progress: 40.0,
        };
        assert_eq!(name_and_species("John Doe", None), "John Doe (Unknown)");
        let bytes = crate::encode(&data);
        assert_eq!(
            crate::decode::<ExperimentalClonerData>(&bytes).unwrap(),
            data
        );
    }
}
