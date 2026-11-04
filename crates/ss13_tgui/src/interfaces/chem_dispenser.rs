//! `ChemDispenser` (chem, soda and booze dispensers): `tgui/packages/tgui/interfaces/
//! ChemDispenser.tsx`, `interfaces/common/BeakerDisplay.tsx` and `/obj/machinery/chem_dispenser/
//! ui_data` (`code/modules/reagents/chemistry/machinery/chem_dispenser.dm`). The server half lives
//! with the game (`ss13_game::chem_dispenser`).
// shortcut: no custom recipes (recording, saving, dispensing them), no reaction list panel, no
// pH colors or tooltips and no "Reactions" lookup window; the hallucination names are not
// ported.

use serde::{Deserialize, Serialize};

use crate::UiInterface;

pub struct ChemDispenserUi;

/// `ui_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct ChemDispenserData {
    pub amount: f32,
    /// Joules.
    pub energy: f32,
    pub max_energy: f32,
    /// `cell.charge / power_cost`.
    pub displayed_units: f32,
    pub displayed_max_units: f32,
    pub show_ph: bool,
    pub has_beaker_in_hand: bool,
    pub chemicals: Vec<DispensableReagent>,
    pub beaker: Option<DispenserBeaker>,
}

/// One `chemicals` entry: `id` is the reagent's name (`GLOB.name2reagent`).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct DispensableReagent {
    pub title: String,
    pub id: String,
    pub ph: f32,
    /// DM color text (`#RRGGBB`).
    pub color: String,
}

/// `beaker` (`TransferableBeaker`).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct DispenserBeaker {
    pub max_volume: f32,
    pub transfer_amounts: Vec<f32>,
    pub ph: f32,
    pub current_volume: f32,
    /// (name, volume).
    pub contents: Vec<(String, f32)>,
}

impl UiInterface for ChemDispenserUi {
    const NAME: &'static str = "ChemDispenser";
    type Data = ChemDispenserData;
    type StaticData = ();
}

/// JS number text of a DM number (`String(n)` of what `json_encode` sent).
pub fn js_num(n: f32) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        return format!("{}", n as i64);
    }
    let text = format!("{n:.4}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

#[cfg(feature = "client")]
mod client_side {
    use bevy::scene::EntityScene;
    use ss13_ui_kit::prelude::*;

    use super::{ChemDispenserData, ChemDispenserUi, js_num};
    use crate::client::ClientUi;
    use crate::interfaces::job_selection::parse_hex;

    impl ClientUi for ChemDispenserUi {
        const WIDTH: f32 = 565.0;
        const HEIGHT: f32 = 620.0;

        fn content(_: &(), data: &ChemDispenserData) -> impl Scene {
            let amounts: Vec<f32> = data
                .beaker
                .as_ref()
                .map(|b| b.transfer_amounts.clone())
                .unwrap_or_default();
            let energy = if data.max_energy > 0.0 {
                data.energy / data.max_energy
            } else {
                0.0
            };
            let energy_text = format!(
                "{} / {} units",
                js_num(data.displayed_units),
                js_num(data.displayed_max_units)
            );
            let dispense_amounts: Vec<_> = amounts
                .iter()
                .map(|&a| {
                    let selected = a == data.amount;
                    let params = ActParams(vec![("target".into(), js_num(a))]);
                    bsn! {
                        @Button { @icon: "plus", @content: {js_num(a)} }
                        Button { selected: selected }
                        Act("amount")
                        params
                    }
                })
                .collect();
            let remove_amounts: Vec<_> = amounts
                .iter()
                .map(|&a| {
                    let params = ActParams(vec![("amount".into(), js_num(a))]);
                    bsn! {
                        @Button { @icon: "minus", @content: {js_num(a)} }
                        Act("remove")
                        params
                    }
                })
                .collect();
            let chemicals: Vec<_> = data
                .chemicals
                .iter()
                .map(|c| {
                    let color = parse_hex(&c.color).map_or(UiColor::Default, UiColor::Css);
                    let title = c.title.clone();
                    let params = ActParams(vec![("reagent".into(), c.id.clone())]);
                    bsn! {
                        @StackItem
                        Node { width: percent(24.5), margin: UiRect::right(px(-6.0)) }
                        Children [
                            @Button { @fluid: true, @ellipsis: true, @tooltip: {title.clone()} }
                            Act("dispense")
                            params
                            Children [
                                Node {
                                    min_width: px(16),
                                    margin: UiRect::horizontal(px(6.0)),
                                    justify_content: JustifyContent::Center,
                                }
                                Children [ @Icon { @name: "tint", @color: color } ]
                                --
                                Node { min_width: px(0), overflow: Overflow::clip() }
                                Children [ Text(title) TextLayout::no_wrap() ]
                            ]
                        ]
                    }
                })
                .collect();
            let beaker = beaker_display(data);
            let dispense_amounts: Box<dyn SceneList> = Box::new(dispense_amounts);
            let remove_amounts: Box<dyn SceneList> = Box::new(remove_amounts);
            bsn! {
                @WindowContent { @scrollable: true }
                Children [
                    @Stack { @vertical: true }
                    Children [
                        @StackItem
                        Children [
                            @Section { @title: "Status" }
                            Children [
                                @LabeledList
                                Children [
                                    @LabeledListItem { @label: "Energy" }
                                    Children [
                                        @ProgressBar { @empty: true }
                                        ProgressBar { value: energy }
                                        Node { flex_grow: 1.0, width: percent(100) }
                                        Children [ Text(energy_text) ]
                                    ]
                                ]
                            ]
                        ]
                        --
                        @StackItem
                        Children [
                            @Section { @title: "Dispense", @buttons: {Some(dispense_amounts)} }
                            Children [
                                @Stack { @wrap: true }
                                Children [ {chemicals} ]
                            ]
                        ]
                        --
                        @StackItem { @grow: 1.0 }
                        Children [
                            @Section { @title: "Beaker", @buttons: {Some(remove_amounts)} }
                            Children [ {EntityScene(beaker)} ]
                        ]
                    ]
                ]
            }
        }
    }

    /// `BeakerDisplay`, or "No beaker loaded." with the insert button.
    fn beaker_display(data: &ChemDispenserData) -> Box<dyn Scene> {
        let Some(beaker) = &data.beaker else {
            let disabled = !data.has_beaker_in_hand;
            let tooltip = if disabled {
                "You need to hold a container in your hand!"
            } else {
                ""
            };
            return Box::new(bsn! {
                @Stack { @align: {Some(AlignItems::Center)}, @justify: {Some(JustifyContent::SpaceBetween)} }
                Children [
                    @StackItem
                    Children [ Text("No beaker loaded.") TextColor({UiColor::Label.base()}) ]
                    --
                    @StackItem
                    Children [
                        @Button { @icon: "eject", @content: "Insert", @tooltip: tooltip }
                        Button { disabled: disabled }
                        Act("insert")
                    ]
                ]
            });
        };
        let volume = format!(
            "{}/{} units",
            js_num(beaker.current_volume),
            js_num(beaker.max_volume)
        );
        let texts: Vec<String> = if beaker.contents.is_empty() {
            vec!["Nothing".to_owned()]
        } else {
            beaker
                .contents
                .iter()
                .map(|(name, volume)| format!("{} units of {name}", js_num(*volume)))
                .collect()
        };
        let contents: Vec<_> = texts
            .into_iter()
            .map(|text| bsn! { Text(text) TextColor({UiColor::Label.base()}) })
            .collect();
        let eject: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
            @Button { @icon: "eject", @content: "Eject" }
            Act("eject")
        }));
        Box::new(bsn! {
            @LabeledList
            Children [
                @LabeledListItem {
                    @label: "Beaker",
                    @content: volume,
                    @buttons: {Some(eject)},
                }
                --
                @LabeledListItem { @label: "Contents" }
                Children [
                    Node { flex_direction: FlexDirection::Column }
                    Children [ {contents} ]
                ]
            ]
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_round_trips_and_numbers_print_like_js() {
        let data = ChemDispenserData {
            amount: 10.0,
            energy: 5000.0,
            max_energy: 10000.0,
            displayed_units: 50.0,
            displayed_max_units: 100.0,
            chemicals: vec![DispensableReagent {
                title: "Space Cola".into(),
                id: "Space Cola".into(),
                ph: 7.0,
                color: "#100800".into(),
            }],
            beaker: Some(DispenserBeaker {
                max_volume: 50.0,
                transfer_amounts: vec![5.0, 10.0],
                contents: vec![("Space Cola".into(), 10.0)],
                current_volume: 10.0,
                ph: 7.0,
            }),
            ..Default::default()
        };
        let bytes = crate::encode(&data);
        assert_eq!(crate::decode::<ChemDispenserData>(&bytes).unwrap(), data);
        assert_eq!(js_num(100000.0), "100000");
        assert_eq!(js_num(2.5), "2.5");
        assert_eq!(js_num(1.0 / 3.0), "0.3333");
    }
}
