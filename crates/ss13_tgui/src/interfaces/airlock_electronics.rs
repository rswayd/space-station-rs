//! `AirlockElectronics`: `tgui/packages/tgui/interfaces/AirlockElectronics.tsx` (with its
//! `common/AccessConfig`) and `/obj/item/electronics/airlock/ui_data`, `ui_static_data`
//! (`code/game/machinery/doors/airlock_electronics.dm`). The server half lives with the game
//! (`ss13_game::airlock_construction`).
//!
//! Acts: `one_access`, `direc_set` (`unres_direction`: 1, 2, 4, 8), `set` (`access`),
//! `grant_all`, `clear_all`, `grant_region` / `deny_region` (`region`), `passedName`
//! (`passedName`).
// shortcut: no "Integrated Circuit Shell" checkbox (`set_shell`: airlocks take no circuit shell
// here) and no "Cycling Id" field (`passedCycleId`: cycling airlocks are not ported); add them
// with those mechanics.

use serde::{Deserialize, Serialize};

use crate::UiInterface;

pub struct AirlockElectronicsUi;

/// One access of a region (`AccessConfig`'s `{desc, ref}`).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct RegionAccess {
    pub desc: String,
    pub access: String,
}

/// `SSid_access.all_region_access_tgui[region]` of a station region.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct AccessRegion {
    pub name: String,
    pub accesses: Vec<RegionAccess>,
}

/// `ui_static_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct AirlockElectronicsStatic {
    pub regions: Vec<AccessRegion>,
}

/// `ui_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct AirlockElectronicsData {
    pub accesses: Vec<String>,
    pub one_access: bool,
    /// BYOND dir bits (`unres_sides`).
    pub unres_direction: u8,
    pub passed_name: String,
}

impl UiInterface for AirlockElectronicsUi {
    const NAME: &'static str = "AirlockElectronics";
    type Data = AirlockElectronicsData;
    type StaticData = AirlockElectronicsStatic;
}

#[cfg(feature = "client")]
mod client_side {
    use bevy::scene::EntityScene;
    use ss13_ui_kit::prelude::*;
    use ss13_ui_kit::{ActParams, TabsTab};

    use super::{AirlockElectronicsData, AirlockElectronicsStatic, AirlockElectronicsUi};
    use crate::client::{ActOnSubmit, ClientUi, SetLocal, UiLocalState};

    /// The local key of the shown region tab.
    const REGION_TAB: &str = "region";

    fn params(key: &str, value: String) -> ActParams {
        ActParams(vec![(key.to_owned(), value)])
    }

    impl ClientUi for AirlockElectronicsUi {
        const WIDTH: f32 = 420.0;
        const HEIGHT: f32 = 485.0;

        fn content(st: &AirlockElectronicsStatic, data: &AirlockElectronicsData) -> impl Scene {
            view(st, data, &UiLocalState::default())
        }

        fn content_local(
            st: &AirlockElectronicsStatic,
            data: &AirlockElectronicsData,
            local: &UiLocalState,
        ) -> impl Scene {
            view(st, data, local)
        }
    }

    fn unres_button(label: &'static str, bit: u8, sides: u8) -> impl Scene + use<> {
        let on = sides & bit != 0;
        let icon = if on { "check-square-o" } else { "square-o" };
        let p = params("unres_direction", bit.to_string());
        bsn! {
            @ss13_ui_kit::Button { @content: label, @icon: icon }
            ss13_ui_kit::Button { selected: on }
            Act("direc_set")
            p
        }
    }

    fn view(
        st: &AirlockElectronicsStatic,
        data: &AirlockElectronicsData,
        local: &UiLocalState,
    ) -> impl Scene + use<> {
        let (one_icon, one_text) = if data.one_access {
            ("unlock", "One")
        } else {
            ("lock", "All")
        };
        let sides = data.unres_direction;
        let north = EntityScene(unres_button("North", 1, sides));
        let south = EntityScene(unres_button("South", 2, sides));
        let east = EntityScene(unres_button("East", 4, sides));
        let west = EntityScene(unres_button("West", 8, sides));
        let name_field = ActOnSubmit {
            action: "passedName".into(),
            param: "passedName".into(),
            params: Vec::new(),
        };
        let name_value = data.passed_name.clone();
        let name_max: Option<usize> = Some(30);
        let selected = &data.accesses;
        let shown = st
            .regions
            .iter()
            .find(|r| r.name == local.get(REGION_TAB))
            .or(st.regions.first());
        let shown_name = shown.map(|r| r.name.clone()).unwrap_or_default();
        // `AccessConfig`: a region's tab shows whether all, some or none of it is granted.
        let tabs: Vec<_> = st
            .regions
            .iter()
            .map(|r| {
                let any = r.accesses.iter().any(|a| selected.contains(&a.access));
                let all = r.accesses.iter().all(|a| selected.contains(&a.access));
                let icon = if all {
                    "check"
                } else if any {
                    "minus"
                } else {
                    "times"
                };
                let pick = SetLocal {
                    key: REGION_TAB.into(),
                    value: r.name.clone(),
                    rerender: true,
                };
                let (label, sel) = (r.name.clone(), r.name == shown_name);
                bsn! {
                    @TabsTab { @content: label, @icon: icon }
                    TabsTab { selected: sel }
                    pick
                }
            })
            .collect();
        let mut entries = shown.map(|r| r.accesses.clone()).unwrap_or_default();
        entries.sort_by(|a, b| a.desc.cmp(&b.desc));
        let checkboxes: Vec<_> = entries
            .into_iter()
            .map(|a| {
                let checked = selected.contains(&a.access);
                let icon = if checked {
                    "check-square-o"
                } else {
                    "square-o"
                };
                let p = params("access", a.access.clone());
                let label = a.desc.clone();
                bsn! {
                    @ss13_ui_kit::Button { @content: label, @icon: icon, @fluid: true }
                    ss13_ui_kit::Button { selected: checked }
                    Act("set")
                    p
                }
            })
            .collect();
        let grant = params("region", shown_name.clone());
        let deny = params("region", shown_name);
        bsn! {
            @WindowContent
            Children [
                @Section
                Children [
                    @LabeledList
                    Children [
                        @LabeledListItem { @label: "Access Required" }
                        Children [
                            @ss13_ui_kit::Button { @icon: one_icon, @content: one_text }
                            Act("one_access")
                        ]
                        --
                        @LabeledListItem { @label: "Unrestricted Access" }
                        Children [ {north} -- {south} -- {east} -- {west} ]
                        --
                        @LabeledListItem { @label: "Airlock Name" }
                        Children [
                            Input { value: name_value, max_length: name_max, fluid: true }
                            name_field
                        ]
                    ]
                ]
                --
                @Section {
                    @title: "Access",
                    @buttons: bsn_list! {
                        @ss13_ui_kit::Button { @icon: "check-double", @content: "Grant All" }
                        ss13_ui_kit::Button { color: UiColor::Good }
                        Act("grant_all")
                        --
                        @ss13_ui_kit::Button { @icon: "undo", @content: "Deny All" }
                        ss13_ui_kit::Button { color: UiColor::Bad }
                        Act("clear_all")
                    },
                }
                Children [
                    Node { flex_direction: FlexDirection::Row, column_gap: px(6) }
                    Children [
                        Node { min_width: px(150) }
                        Children [ @Tabs { @vertical: true } Children [ {tabs} ] ]
                        --
                        Node { flex_grow: 1.0, flex_direction: FlexDirection::Column }
                        Children [
                            Node
                            Children [
                                @ss13_ui_kit::Button { @icon: "check", @content: "Grant Region" }
                                ss13_ui_kit::Button { color: UiColor::Good }
                                Act("grant_region")
                                grant
                                --
                                @ss13_ui_kit::Button { @icon: "times", @content: "Deny Region" }
                                ss13_ui_kit::Button { color: UiColor::Bad }
                                Act("deny_region")
                                deny
                            ]
                            --
                            {checkboxes}
                        ]
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
    fn data_round_trips() {
        let data = AirlockElectronicsData {
            accesses: vec!["engineering".into(), "maint_tunnels".into()],
            one_access: true,
            unres_direction: 1 | 4,
            passed_name: "Engine Room".into(),
        };
        let bytes = crate::encode(&data);
        assert_eq!(
            crate::decode::<AirlockElectronicsData>(&bytes).unwrap(),
            data
        );
        let st = AirlockElectronicsStatic {
            regions: vec![AccessRegion {
                name: "Engineering".into(),
                accesses: vec![RegionAccess {
                    desc: "Engineering".into(),
                    access: "engineering".into(),
                }],
            }],
        };
        let bytes = crate::encode(&st);
        assert_eq!(
            crate::decode::<AirlockElectronicsStatic>(&bytes).unwrap(),
            st
        );
    }
}
