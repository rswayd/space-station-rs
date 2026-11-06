//! `Sleeper`: `tgui/packages/tgui/interfaces/Sleeper.jsx` and `/obj/machinery/sleeper/ui_data`
//! (`code/game/machinery/sleepers.dm`). The server half lives with the game
//! (`ss13_game::medical::sleeper`).

use serde::{Deserialize, Serialize};

use crate::UiInterface;

pub struct SleeperUi;

/// One `chems` entry.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct SleeperChem {
    pub name: String,
    /// The reagent's type path (the `inject` act's `chem`).
    pub id: String,
    pub allowed: bool,
}

/// `occupant`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct SleeperOccupant {
    pub name: String,
    pub stat: String,
    /// `statstate`: `good`, `average` or `bad`.
    pub statstate: String,
    pub health: f32,
    pub max_health: f32,
    /// `dead_threshold`.
    pub min_health: f32,
    pub brute_loss: f32,
    pub oxy_loss: f32,
    pub tox_loss: f32,
    pub fire_loss: f32,
    pub brain_loss: f32,
    /// (name, volume) of the visible reagents.
    pub reagents: Vec<(String, f32)>,
}

/// `ui_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct SleeperData {
    pub occupied: bool,
    pub open: bool,
    pub chems: Vec<SleeperChem>,
    pub occupant: Option<SleeperOccupant>,
}

impl UiInterface for SleeperUi {
    const NAME: &'static str = "Sleeper";
    type Data = SleeperData;
    type StaticData = ();
}

/// `chems.sort` by lowercase name.
pub fn sorted_chems(chems: &[SleeperChem]) -> Vec<SleeperChem> {
    let mut out = chems.to_vec();
    out.sort_by_key(|c| c.name.to_lowercase());
    out
}

#[cfg(feature = "client")]
mod client_side {
    use bevy::scene::EntityScene;
    use ss13_ui_kit::prelude::*;
    use ss13_ui_kit::{ActParams, UiColor};

    use super::{SleeperData, SleeperOccupant, SleeperUi, sorted_chems};
    use crate::client::ClientUi;

    impl ClientUi for SleeperUi {
        const WIDTH: f32 = 310.0;
        const HEIGHT: f32 = 465.0;

        fn content(_: &(), data: &SleeperData) -> impl Scene {
            let title = data
                .occupant
                .as_ref()
                .map_or_else(|| "No Occupant".to_owned(), |o| o.name.clone());
            let stat = data.occupant.as_ref().map(|o| {
                let (text, color) = (o.stat.clone(), state_color(&o.statstate));
                Box::new(EntityScene(bsn! {
                    @UiBox { @color: color, @bold: true, @inline: true }
                    Children [ Text(text) ]
                })) as Box<dyn SceneList>
            });
            let occupant: Box<dyn Scene> = match &data.occupant {
                Some(o) if data.occupied => Box::new(occupant_view(o)),
                _ => Box::new(bsn! { Node }),
            };
            let (door_icon, door_text) = if data.open {
                ("door-open", "Open")
            } else {
                ("door-closed", "Closed")
            };
            let door: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
                @Button { @icon: door_icon, @content: door_text }
                Act("door")
            }));
            let occupied = data.occupied;
            let chems: Vec<_> = sorted_chems(&data.chems)
                .into_iter()
                .map(|c| {
                    let disabled = !occupied || !c.allowed;
                    let params = ActParams(vec![("chem".into(), c.id.clone())]);
                    let name = c.name;
                    bsn! {
                        @Button { @icon: "flask", @content: name }
                        Button { disabled: disabled }
                        Node { width: px(140) }
                        Act("inject")
                        params
                    }
                })
                .collect();
            bsn! {
                @WindowContent
                Children [
                    @Section { @title: title, @buttons: {stat} }
                    Node { min_height: px(210) }
                    Children [ {EntityScene(occupant)} ]
                    --
                    @Section { @title: "Medicines", @buttons: {Some(door)} }
                    Node { min_height: px(205) }
                    Children [
                        Node { flex_wrap: FlexWrap::Wrap }
                        Children [ {chems} ]
                    ]
                ]
            }
        }
    }

    fn state_color(state: &str) -> UiColor {
        match state {
            "good" => UiColor::Good,
            "average" => UiColor::Average,
            _ => UiColor::Bad,
        }
    }

    /// The health bar, the damage bars and the brain line.
    fn occupant_view(o: &SleeperOccupant) -> impl Scene + use<> {
        let ranges = vec![
            (UiColor::Good, 50.0, f32::INFINITY),
            (UiColor::Average, 0.0, 50.0),
            (UiColor::Bad, f32::NEG_INFINITY, 0.0),
        ];
        let (health, min, max) = (o.health, o.min_health, o.max_health);
        let damage: Vec<_> = [
            ("Brute", o.brute_loss),
            ("Burn", o.fire_loss),
            ("Toxin", o.tox_loss),
            ("Oxygen", o.oxy_loss),
        ]
        .into_iter()
        .map(|(label, value)| {
            bsn! {
                @LabeledListItem { @label: label }
                Children [
                    @ProgressBar
                    ProgressBar {
                        value: value,
                        min_value: 0.0,
                        max_value: max,
                        color: UiColor::Bad,
                    }
                    Node { flex_grow: 1.0, width: percent(100) }
                ]
            }
        })
        .collect();
        let (brain, brain_color) = if o.brain_loss > 0.0 {
            ("Abnormal", UiColor::Bad)
        } else {
            ("Healthy", UiColor::Good)
        };
        bsn! {
            Node { flex_direction: FlexDirection::Column }
            Children [
                @ProgressBar
                ProgressBar {
                    value: health,
                    min_value: min,
                    max_value: max,
                    ranges: ranges,
                }
                Node { flex_grow: 1.0, width: percent(100), margin: UiRect::bottom(px(6)) }
                --
                @LabeledList
                Children [
                    {damage}
                    --
                    @LabeledListItem { @label: "Brain", @color: brain_color, @content: brain }
                ]
            ]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_round_trips_and_chems_sort_by_name() {
        let data = SleeperData {
            occupied: true,
            open: false,
            chems: vec![
                SleeperChem {
                    name: "morphine".into(),
                    id: "/datum/reagent/medicine/morphine".into(),
                    allowed: true,
                },
                SleeperChem {
                    name: "Epinephrine".into(),
                    id: "/datum/reagent/medicine/epinephrine".into(),
                    allowed: false,
                },
            ],
            occupant: Some(SleeperOccupant {
                name: "John".into(),
                stat: "Stable".into(),
                statstate: "good".into(),
                health: 80.0,
                max_health: 100.0,
                min_health: -100.0,
                brute_loss: 20.0,
                reagents: vec![("Morphine".into(), 5.0)],
                ..Default::default()
            }),
        };
        let bytes = crate::encode(&data);
        assert_eq!(crate::decode::<SleeperData>(&bytes).unwrap(), data);
        let names: Vec<String> = sorted_chems(&data.chems)
            .into_iter()
            .map(|c| c.name)
            .collect();
        assert_eq!(names, ["Epinephrine", "morphine"]);
    }
}
