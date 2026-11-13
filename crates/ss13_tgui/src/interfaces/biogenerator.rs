//! `Biogenerator`: `tgui/packages/tgui/interfaces/Biogenerator.tsx` and
//! `/obj/machinery/biogenerator/ui_data`, `ui_static_data` (`code/modules/hydroponics/biogenerator.dm`).
//! The server half lives with the game (`ss13_game::botany::biogenerator`).

use serde::{Deserialize, Serialize};

use crate::UiInterface;

pub struct BiogeneratorUi;

/// One design of a category.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct BiogeneratorDesign {
    /// The design's type path (the `create` act's `design_path`).
    pub path: String,
    pub name: String,
    pub is_reagent: bool,
    /// Biomass per unit, after the efficiency.
    pub cost: f32,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct BiogeneratorCategory {
    pub name: String,
    pub items: Vec<BiogeneratorDesign>,
}

/// `ui_static_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct BiogeneratorStatic {
    pub categories: Vec<BiogeneratorCategory>,
    pub max_visual_biomass: f32,
}

/// `ui_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct BiogeneratorData {
    pub beaker: bool,
    pub beaker_current_volume: f32,
    pub beaker_max_volume: f32,
    pub biomass: f32,
    pub processing: bool,
    pub max_output: u32,
    pub can_process: bool,
}

impl UiInterface for BiogeneratorUi {
    const NAME: &'static str = "Biogenerator";
    type Data = BiogeneratorData;
    type StaticData = BiogeneratorStatic;
}

/// The shown tab: the picked category, else the first.
pub fn shown_category<'a>(
    st: &'a BiogeneratorStatic,
    picked: &str,
) -> Option<&'a BiogeneratorCategory> {
    st.categories
        .iter()
        .find(|c| c.name == picked)
        .or_else(|| st.categories.first())
}

/// `Item`'s amount bounds: the beaker's free space for a reagent (1 without a beaker), else
/// `max_output`, and what the biomass pays for; never below 1.
pub fn max_amount(design: &BiogeneratorDesign, data: &BiogeneratorData) -> u32 {
    let capacity = if design.is_reagent {
        space(data)
    } else {
        data.max_output as f32
    };
    let affordable = if design.cost > 0.0 {
        (data.biomass / design.cost).floor()
    } else {
        capacity
    };
    capacity.min(affordable).max(1.0) as u32
}

fn space(data: &BiogeneratorData) -> f32 {
    if data.beaker {
        (data.beaker_max_volume - data.beaker_current_volume).max(0.0)
    } else {
        1.0
    }
}

/// `Item`'s `disabled`.
pub fn create_disabled(design: &BiogeneratorDesign, data: &BiogeneratorData, amount: u32) -> bool {
    data.processing
        || (design.is_reagent && (!data.beaker || space(data) < amount as f32))
        || data.biomass < design.cost * amount as f32
}

#[cfg(feature = "client")]
mod client_side {
    use bevy::scene::EntityScene;
    use ss13_ui_kit::prelude::*;
    use ss13_ui_kit::{ActParams, TabsTab, UiColor};

    use super::{
        BiogeneratorData, BiogeneratorStatic, BiogeneratorUi, create_disabled, max_amount,
        shown_category,
    };
    use crate::client::{ActWithLocal, ClientUi, LocalOnNumber, SetLocal, UiLocalState};
    use crate::interfaces::chem_dispenser::js_num;

    const LOCAL_CATEGORY: &str = "category";

    impl ClientUi for BiogeneratorUi {
        const WIDTH: f32 = 400.0;
        const HEIGHT: f32 = 500.0;

        fn content(st: &BiogeneratorStatic, data: &BiogeneratorData) -> impl Scene {
            view(st, data, &UiLocalState::default())
        }

        fn content_local(
            st: &BiogeneratorStatic,
            data: &BiogeneratorData,
            local: &UiLocalState,
        ) -> impl Scene {
            view(st, data, local)
        }
    }

    fn view(
        st: &BiogeneratorStatic,
        data: &BiogeneratorData,
        local: &UiLocalState,
    ) -> impl Scene + use<> {
        let biomass_bar = ProgressBar {
            value: data.biomass,
            min_value: 0.0,
            max_value: st.max_visual_biomass.max(1.0),
            color: UiColor::Good,
            ..Default::default()
        };
        let biomass_text = format!("{} units", js_num((data.biomass * 100.0).round() / 100.0));
        let generate_disabled = !data.can_process || data.processing;
        let generate: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
            @Button { @icon: "cog", @content: "Generate" }
            Button { disabled: generate_disabled }
            Act("activate")
        }));
        let container: Box<dyn Scene> = if data.beaker {
            let bar = ProgressBar {
                value: data.beaker_current_volume,
                min_value: 0.0,
                max_value: data.beaker_max_volume.max(1.0),
                color: UiColor::Good,
                ..Default::default()
            };
            let text = format!(
                "{} of {} units",
                js_num(data.beaker_current_volume),
                js_num(data.beaker_max_volume)
            );
            let eject: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
                @Button { @icon: "eject", @content: "Eject" }
                Act("eject")
            }));
            Box::new(bsn! {
                @LabeledListItem { @label: "Container", @buttons: {Some(eject)} }
                Children [ @ProgressBar { @empty: true } bar Children [ Text(text) ] ]
            })
        } else {
            Box::new(bsn! {
                @LabeledListItem { @label: "Container" }
                Children [ @NoticeBox Children [ Text("No liquid container") ] ]
            })
        };
        let shown = shown_category(st, local.get(LOCAL_CATEGORY));
        let shown_name = shown.map_or(String::new(), |c| c.name.clone());
        let tabs: Vec<_> = st
            .categories
            .iter()
            .map(|c| {
                let pick = SetLocal {
                    key: LOCAL_CATEGORY.into(),
                    value: c.name.clone(),
                    rerender: true,
                };
                let (name, selected) = (c.name.clone(), c.name == shown_name);
                bsn! {
                    @TabsTab { @content: name }
                    TabsTab { selected: selected }
                    pick
                }
            })
            .collect();
        let items: Vec<_> = shown
            .map(|c| c.items.as_slice())
            .unwrap_or_default()
            .iter()
            .map(|d| {
                let key = format!("amount:{}", d.path);
                let max = max_amount(d, data);
                let amount = local
                    .get(&key)
                    .parse::<f64>()
                    .ok()
                    .map_or(1, |n| (n as u32).clamp(1, max));
                let input = NumberInput {
                    value: f64::from(amount),
                    min_value: 1.0,
                    max_value: f64::from(max),
                    step: 1.0,
                    ..Default::default()
                };
                let remember = LocalOnNumber(key.clone());
                let send = ActWithLocal {
                    param: "amount".into(),
                    key,
                    default: "1".into(),
                };
                let params = ActParams(vec![("design_path".into(), d.path.clone())]);
                let disabled = create_disabled(d, data, amount);
                let cost = js_num(((d.cost * amount as f32) * 100.0).round() / 100.0);
                let name = d.name.clone();
                bsn! {
                    Node { align_items: AlignItems::Center, margin: UiRect::bottom(px(2)) }
                    Children [
                        Node { flex_grow: 1.0 } Children [ Text(name) ]
                        --
                        Node { width: px(48), margin: UiRect::horizontal(px(4)) }
                        input remember
                        --
                        @Button { @icon: "leaf", @content: cost }
                        Button { disabled: disabled }
                        Node { width: px(70) }
                        Act("create")
                        params
                        send
                    ]
                }
            })
            .collect();
        bsn! {
            @WindowContent { @scrollable: true }
            Children [
                @Section
                Children [
                    @LabeledList
                    Children [
                        @LabeledListItem { @label: "Biomass", @buttons: {Some(generate)} }
                        Children [
                            @ProgressBar { @empty: true } biomass_bar Children [ Text(biomass_text) ]
                        ]
                        --
                        {EntityScene(container)}
                    ]
                ]
                --
                @Tabs Children [ {tabs} ]
                --
                @Section Children [ {items} ]
            ]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn milk() -> BiogeneratorDesign {
        BiogeneratorDesign {
            path: "/datum/design/biogen/milk".into(),
            name: "Synthetic Milk".into(),
            is_reagent: true,
            cost: 0.4,
        }
    }

    #[test]
    fn data_round_trips_and_amounts_follow_space_and_biomass() {
        let st = BiogeneratorStatic {
            categories: vec![BiogeneratorCategory {
                name: "Еда".into(),
                items: vec![milk()],
            }],
            max_visual_biomass: 5000.0,
        };
        assert_eq!(
            crate::decode::<BiogeneratorStatic>(&crate::encode(&st)).unwrap(),
            st
        );
        let mut data = BiogeneratorData {
            beaker: true,
            beaker_current_volume: 40.0,
            beaker_max_volume: 50.0,
            biomass: 100.0,
            processing: false,
            max_output: 50,
            can_process: false,
        };
        assert_eq!(
            crate::decode::<BiogeneratorData>(&crate::encode(&data)).unwrap(),
            data
        );
        assert_eq!(max_amount(&milk(), &data), 10, "the beaker's free space");
        assert!(!create_disabled(&milk(), &data, 10));
        assert!(create_disabled(&milk(), &data, 11));
        data.biomass = 2.0;
        assert_eq!(max_amount(&milk(), &data), 5, "what the biomass pays for");
        data.beaker = false;
        assert!(create_disabled(&milk(), &data, 1), "no container");
    }
}
