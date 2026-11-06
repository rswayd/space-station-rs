//! `Limbgrower`: `tgui/packages/tgui/interfaces/Limbgrower.jsx` and
//! `/obj/machinery/limbgrower/ui_data`, `ui_static_data` (`code/game/machinery/limbgrower.dm`).
//! The server half lives with the game (`ss13_game::medical::limbgrower`).

use serde::{Deserialize, Serialize};

use crate::UiInterface;

pub struct LimbgrowerUi;

/// One `reagents` entry.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct LimbgrowerReagent {
    pub reagent_name: String,
    pub reagent_amount: f32,
    /// The type path (the `empty_reagent` act's `reagent_type`).
    pub reagent_type: String,
}

/// `ui_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct LimbgrowerData {
    pub reagents: Vec<LimbgrowerReagent>,
    pub total_reagents: f32,
    pub max_reagents: f32,
    pub busy: bool,
}

/// One design of a category.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct LimbgrowerDesign {
    pub parent_category: String,
    pub name: String,
    /// The design's type path (`make_limb`'s `design_path`).
    pub path: String,
    /// (reagent name, units after the production coefficient).
    pub needed_reagents: Vec<(String, f32)>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct LimbgrowerCategory {
    pub name: String,
    pub designs: Vec<LimbgrowerDesign>,
}

/// `ui_static_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct LimbgrowerStatic {
    pub categories: Vec<LimbgrowerCategory>,
}

impl UiInterface for LimbgrowerUi {
    const NAME: &'static str = "Limbgrower";
    type Data = LimbgrowerData;
    type StaticData = LimbgrowerStatic;
}

/// The shown tab: the picked category, else the first.
pub fn shown_category<'a>(
    st: &'a LimbgrowerStatic,
    picked: &str,
) -> Option<&'a LimbgrowerCategory> {
    st.categories
        .iter()
        .find(|c| c.name == picked)
        .or_else(|| st.categories.first())
}

#[cfg(feature = "client")]
mod client_side {
    use bevy::scene::EntityScene;
    use ss13_ui_kit::prelude::*;
    use ss13_ui_kit::{ActParams, TabsTab, UiColor};

    use super::{LimbgrowerData, LimbgrowerStatic, LimbgrowerUi, shown_category};
    use crate::client::{ClientUi, SetLocal, UiLocalState};
    use crate::interfaces::chem_dispenser::js_num;

    /// `useSharedState('category')`.
    const LOCAL_CATEGORY: &str = "category";

    impl ClientUi for LimbgrowerUi {
        const WIDTH: f32 = 500.0;
        const HEIGHT: f32 = 550.0;

        fn content(st: &LimbgrowerStatic, data: &LimbgrowerData) -> impl Scene {
            view(st, data, &UiLocalState::default())
        }

        fn content_local(
            st: &LimbgrowerStatic,
            data: &LimbgrowerData,
            local: &UiLocalState,
        ) -> impl Scene {
            view(st, data, local)
        }
    }

    fn view(
        st: &LimbgrowerStatic,
        data: &LimbgrowerData,
        local: &UiLocalState,
    ) -> impl Scene + use<> {
        let capacity = format!(
            "{} / {} reagent capacity used.",
            js_num(data.total_reagents),
            js_num(data.max_reagents)
        );
        let reagents: Vec<_> = data
            .reagents
            .iter()
            .map(|r| {
                let params = ActParams(vec![("reagent_type".into(), r.reagent_type.clone())]);
                let remove: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
                    @Button { @content: "Remove Reagent" }
                    Button { color: UiColor::Bad }
                    Node { width: px(120) }
                    Act("empty_reagent")
                    params
                }));
                let (label, amount) = (
                    r.reagent_name.clone(),
                    format!("{}u", js_num(r.reagent_amount)),
                );
                bsn! {
                    @LabeledListItem { @label: label, @buttons: {Some(remove)}, @content: amount }
                }
            })
            .collect();
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
        let designs: Vec<_> = shown
            .map(|c| c.designs.as_slice())
            .unwrap_or_default()
            .iter()
            .map(|d| {
                let params = ActParams(vec![
                    ("design_path".into(), d.path.clone()),
                    ("active_tab".into(), d.parent_category.clone()),
                ]);
                let make: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
                    @Button { @content: "Make" }
                    Button { color: UiColor::Good }
                    Act("make_limb")
                    params
                }));
                let needs: Vec<_> = d
                    .needed_reagents
                    .iter()
                    .map(|(name, amount)| {
                        let text = format!("{name}: {}u", js_num(*amount));
                        bsn! { Text(text) }
                    })
                    .collect();
                let label = d.name.clone();
                bsn! {
                    @LabeledListItem { @label: label, @buttons: {Some(make)} }
                    Children [ Node { flex_direction: FlexDirection::Column } Children [ {needs} ] ]
                }
            })
            .collect();
        let dimmer: Box<dyn Scene> = if data.busy {
            Box::new(bsn! {
                @Dimmer
                Children [
                    Node { align_items: AlignItems::Center }
                    Children [
                        @Icon { @name: "cog", @spin: true, @size: 2.0 }
                        --
                        Text(" Building...")
                    ]
                ]
            })
        } else {
            Box::new(bsn! { Node })
        };
        bsn! {
            Node { flex_direction: FlexDirection::Column, flex_grow: 1.0 }
            Children [
                @WindowContent { @scrollable: true }
                Children [
                    @Section { @title: "Reagents" }
                    Children [
                        Node { margin: UiRect::bottom(px(6)) }
                        Children [ Text(capacity) ]
                        --
                        @LabeledList Children [ {reagents} ]
                    ]
                    --
                    @Section { @title: "Designs" }
                    Children [
                        @Tabs Children [ {tabs} ]
                        --
                        @LabeledList Children [ {designs} ]
                    ]
                ]
                --
                {EntityScene(dimmer)}
            ]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_round_trips_and_the_first_tab_shows() {
        let st = LimbgrowerStatic {
            categories: vec![
                LimbgrowerCategory {
                    name: "human".into(),
                    designs: vec![LimbgrowerDesign {
                        parent_category: "human".into(),
                        name: "Left Arm".into(),
                        path: "/datum/design/leftarm".into(),
                        needed_reagents: vec![("Synthflesh".into(), 25.0)],
                    }],
                },
                LimbgrowerCategory {
                    name: "lizard".into(),
                    designs: Vec::new(),
                },
            ],
        };
        let bytes = crate::encode(&st);
        assert_eq!(crate::decode::<LimbgrowerStatic>(&bytes).unwrap(), st);
        assert_eq!(
            shown_category(&st, "").map(|c| c.name.as_str()),
            Some("human")
        );
        assert_eq!(
            shown_category(&st, "lizard").map(|c| c.name.as_str()),
            Some("lizard")
        );
        let data = LimbgrowerData {
            reagents: vec![LimbgrowerReagent {
                reagent_name: "Synthflesh".into(),
                reagent_amount: 50.0,
                reagent_type: "/datum/reagent/medicine/c2/synthflesh".into(),
            }],
            total_reagents: 50.0,
            max_reagents: 100.0,
            busy: true,
        };
        let bytes = crate::encode(&data);
        assert_eq!(crate::decode::<LimbgrowerData>(&bytes).unwrap(), data);
    }
}
