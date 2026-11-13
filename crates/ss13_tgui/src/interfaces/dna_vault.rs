//! `DnaVault`: `tgui/packages/tgui/interfaces/DnaVault.tsx` and `/obj/machinery/dna_vault/
//! ui_data` (`code/modules/station_goals/dna_vault.dm`). The server half lives with the game
//! (`ss13_game::genetics::dna_vault`).

use serde::{Deserialize, Serialize};

use crate::UiInterface;

pub struct DnaVaultUi;

/// `ui_data`: the samples against the goal, and the viewer's two gene therapy choices.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct DnaVaultData {
    pub plants: u32,
    pub plants_max: u32,
    pub animals: u32,
    pub animals_max: u32,
    pub dna: u32,
    pub dna_max: u32,
    pub completed: bool,
    /// The viewer has no choices (took one, or never rolled).
    pub used: bool,
    pub choice_a: String,
    pub choice_b: String,
}

impl UiInterface for DnaVaultUi {
    const NAME: &'static str = "DnaVault";
    type Data = DnaVaultData;
    type StaticData = ();
}

#[cfg(feature = "client")]
mod client_side {
    use bevy::scene::EntityScene;
    use ss13_ui_kit::prelude::*;

    use super::{DnaVaultData, DnaVaultUi};
    use crate::client::ClientUi;

    fn bar_row(label: &'static str, have: u32, max: u32) -> impl Scene {
        let fill = if max == 0 {
            1.0
        } else {
            have as f32 / max as f32
        };
        let text = format!("{have} / {max} Samples");
        bsn! {
            @LabeledListItem { @label: label }
            Children [
                @ProgressBar { @empty: true }
                ProgressBar { value: fill }
                Node { flex_grow: 1.0, width: percent(100) }
                Children [ Text(text) ]
            ]
        }
    }

    fn choice(name: &str) -> impl Scene {
        let params = ActParams(vec![("choice".to_owned(), name.to_owned())]);
        let name = name.to_owned();
        bsn! {
            Node { flex_grow: 1.0 }
            Children [
                @Button { @content: name }
                Act("gene")
                params
            ]
        }
    }

    impl ClientUi for DnaVaultUi {
        const WIDTH: f32 = 350.0;
        const HEIGHT: f32 = 400.0;

        fn content(_: &(), data: &DnaVaultData) -> impl Scene {
            let therapy = (data.completed && !data.used).then(|| {
                let (a, b) = (choice(&data.choice_a), choice(&data.choice_b));
                EntityScene(bsn! {
                    @Section { @title: "Personal Gene Therapy" }
                    Children [
                        Node { justify_content: JustifyContent::Center }
                        Children [ Text("Applicable Gene Therapy Treatments") ]
                        --
                        Node { flex_direction: FlexDirection::Row, column_gap: px(4) }
                        Children [ {EntityScene(a)} -- {EntityScene(b)} ]
                    ]
                })
            });
            let (human, plant, animal) = (
                bar_row("Human DNA", data.dna, data.dna_max),
                bar_row("Plant DNA", data.plants, data.plants_max),
                bar_row("Animal DNA", data.animals, data.animals_max),
            );
            bsn! {
                @WindowContent
                Children [
                    @Section { @title: "DNA Vault Database" }
                    Children [
                        @LabeledList
                        Children [ {EntityScene(human)} -- {EntityScene(plant)} -- {EntityScene(animal)} ]
                    ]
                    --
                    {therapy}
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
        let data = DnaVaultData {
            plants: 3,
            plants_max: 7,
            animals: 16,
            animals_max: 18,
            dna: 20,
            dna_max: 20,
            completed: false,
            used: true,
            choice_a: String::new(),
            choice_b: String::new(),
        };
        let bytes = crate::encode(&data);
        assert_eq!(crate::decode::<DnaVaultData>(&bytes).unwrap(), data);
    }
}
