//! `GasAnalyzer` (`tgui/packages/tgui/interfaces/GasAnalyzer.tsx`, `common/GasmixParser.tsx`,
//! `common/AtmosHandbook.tsx`): the last scanned mixtures, and the atmos handbook (gases and
//! reactions, `return_atmos_handbooks()`), whose text the converter extracts into
//! `atmos_handbook.ron` ([`AtmosHandbook`]).
//!
//! Client-only state (`useAtom`/`useState` in the JSX): the handbook tab, the gas and the
//! reaction shown, and the search texts ([`LOCAL_TAB`], [`LOCAL_GAS`], [`LOCAL_REACTION`],
//! [`LOCAL_GAS_SEARCH`], [`LOCAL_REACTION_SEARCH`]).
// shortcut: the search filters names by substring (tg fuzzy-matches); a search does not pick
// the first match on Enter.

use serde::{Deserialize, Serialize};

use crate::UiInterface;

/// `gas_mixture_parser()`: `[id, name, amount]` entries.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Gasmix {
    pub name: String,
    pub gases: Vec<(String, String, f32)>,
    pub temperature: f32,
    pub volume: f32,
    pub pressure: f32,
    pub total_moles: f32,
    pub reactions: Vec<(String, String, f32)>,
    pub reference: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct GasAnalyzerData {
    pub gasmixes: Vec<Gasmix>,
}

/// One gas of the handbook (`gas_handbook`).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct HandbookGas {
    pub id: String,
    pub name: String,
    pub description: String,
    pub specific_heat: f32,
    pub export_value: f32,
    /// `reactions`: `[reaction id, reaction name]` it takes part in.
    pub reactions: Vec<(String, String)>,
}

/// What influences a reaction (`factor_info`).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct HandbookFactor {
    /// The gas id of a `"gas"` factor.
    pub factor_id: Option<String>,
    /// `"gas"` or `"misc"`.
    pub factor_type: String,
    pub factor_name: String,
    pub desc: String,
    pub tooltip: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct HandbookReaction {
    pub id: String,
    pub name: String,
    pub description: String,
    pub factors: Vec<HandbookFactor>,
}

/// `return_atmos_handbooks()`: the static data.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct AtmosHandbook {
    pub gas_info: Vec<HandbookGas>,
    pub reaction_info: Vec<HandbookReaction>,
    pub money_symbol: String,
    pub money_name: String,
}

pub struct GasAnalyzerUi;

impl UiInterface for GasAnalyzerUi {
    const NAME: &'static str = "GasAnalyzer";
    type Data = GasAnalyzerData;
    type StaticData = AtmosHandbook;
}

pub const LOCAL_TAB: &str = "handbook_tab";
pub const LOCAL_GAS: &str = "active_gas";
pub const LOCAL_REACTION: &str = "active_reaction";
pub const LOCAL_GAS_SEARCH: &str = "gas_search";
pub const LOCAL_REACTION_SEARCH: &str = "reaction_search";

/// `getSpecificHeatColor`.
pub fn specific_heat_color(specific_heat: f32) -> &'static str {
    match specific_heat {
        h if h <= 10.0 => "red",
        h if h <= 20.0 => "orange",
        h if h <= 100.0 => "yellow",
        h if h <= 200.0 => "green",
        _ => "blue",
    }
}

/// `getExportValueColor`.
pub fn export_value_color(value: f32) -> &'static str {
    match value {
        v if v <= 0.2 => "red",
        v if v <= 0.5 => "orange",
        v if v <= 2.5 => "yellow",
        v if v <= 5.0 => "green",
        _ => "blue",
    }
}

/// JS `toFixed(2)`.
pub fn fixed2(x: f32) -> String {
    format!("{x:.2}")
}

#[cfg(feature = "client")]
pub(crate) mod client_side {
    use bevy::app::Propagate;
    use bevy::scene::EntityScene;
    use ss13_ui_kit::prelude::*;
    use ss13_ui_kit::theme::{Bold, Italic};
    use ss13_ui_kit::{Placement, Tooltip, UiColor};

    use super::*;
    use crate::client::{ClientUi, LocalOnInput, SearchRow, SetLocal, UiLocalState};

    fn color(name: &str) -> UiColor {
        match name {
            "red" => UiColor::Red,
            "orange" => UiColor::Orange,
            "yellow" => UiColor::Yellow,
            "green" => UiColor::Green,
            _ => UiColor::Blue,
        }
    }

    fn set_local(key: &str, value: &str) -> SetLocal {
        SetLocal {
            key: key.into(),
            value: value.into(),
            rerender: true,
        }
    }

    /// `GasmixParser`.
    pub(crate) fn gasmix_view(gasmix: &Gasmix) -> Box<dyn SceneList> {
        if gasmix.total_moles <= 0.0 {
            return Box::new(EntityScene(bsn! {
                Node { margin: UiRect::bottom(px(10)) }
                Propagate::<Italic>(Italic(true))
                Children [ Text("No Gas Detected!") ]
            }));
        }
        let total = gasmix.total_moles;
        let gases: Vec<_> = gasmix
            .gases
            .iter()
            .map(|(id, name, amount)| {
                let value = format!(
                    "{} mol ({} %)",
                    fixed2(*amount),
                    fixed2(amount / total * 100.0)
                );
                let pick = set_local(LOCAL_GAS, id);
                let tab = set_local(LOCAL_TAB, "gases");
                let name = name.clone();
                bsn! {
                    Node {
                        flex_direction: FlexDirection::Row,
                        align_items: AlignItems::Center,
                        margin: UiRect::bottom(px(2)),
                    }
                    Children [
                        @ss13_ui_kit::Button { @content: name }
                        Node { margin: UiRect::right(px(6)) }
                        pick
                        tab
                        --
                        Text(value)
                    ]
                }
            })
            .collect();
        let numbers = [
            ("Temperature", format!("{} K", fixed2(gasmix.temperature))),
            ("Volume", format!("{} L", fixed2(gasmix.volume))),
            ("Pressure", format!("{} kPa", fixed2(gasmix.pressure))),
        ]
        .map(|(label, value)| {
            bsn! { @LabeledListItem { @label: label } Children [ Text(value) ] }
        });
        let reactions: Vec<_> = gasmix
            .reactions
            .iter()
            .map(|(id, name, _)| {
                let pick = set_local(LOCAL_REACTION, id);
                let tab = set_local(LOCAL_TAB, "reactions");
                let name = name.clone();
                bsn! {
                    @StackItem
                    Children [
                        @ss13_ui_kit::Button { @content: name, @fluid: true }
                        pick
                        tab
                    ]
                }
            })
            .collect();
        let reaction_block = (!reactions.is_empty()).then(|| {
            EntityScene(bsn! {
                @StackItem
                Children [
                    @Stack { @vertical: true }
                    Children [
                        @StackItem Children [ Text("Active Reactions") ]
                        --
                        {reactions}
                    ]
                ]
            })
        });
        Box::new(EntityScene(bsn! {
            @Stack { @vertical: true }
            Children [
                @StackItem
                Children [
                    @Stack
                    Children [
                        @StackItem Node { width: percent(60) }
                        Children [ {gases} ]
                        --
                        @StackItem { @grow: 1.0 }
                        Children [ @LabeledList Children [ {Vec::from(numbers)} ] ]
                    ]
                ]
                --
                {reaction_block}
            ]
        }))
    }

    /// A list column of `(id, name)` buttons, filtered in place by `search_key`.
    fn pick_list(
        entries: Vec<(String, String)>,
        local_key: &str,
        search_key: &str,
        active: &str,
    ) -> Vec<impl Scene + use<>> {
        let mut entries = entries;
        entries.sort_by(|a, b| a.1.cmp(&b.1));
        entries
            .into_iter()
            .map(|(id, name)| {
                let row = SearchRow {
                    key: search_key.into(),
                    text: name.to_lowercase(),
                };
                let pick = set_local(local_key, &id);
                let selected = id == active;
                bsn! {
                    @ss13_ui_kit::Button { @content: name, @fluid: true }
                    ss13_ui_kit::Button { selected: selected }
                    pick
                    row
                }
            })
            .collect()
    }

    fn search(key: &str, placeholder: &str) -> impl Scene + use<> {
        let key = key.to_owned();
        let placeholder = placeholder.to_owned();
        bsn! {
            Input { placeholder: placeholder, fluid: true }
            LocalOnInput(key)
        }
    }

    fn nothing_selected(text: &'static str) -> Box<dyn SceneList> {
        Box::new(EntityScene(bsn! {
            Node { margin: UiRect::top(px(16)), justify_content: JustifyContent::Center }
            Propagate::<Italic>(Italic(true))
            Children [ Text(text) ]
        }))
    }

    fn gas_tab(st: &AtmosHandbook, local: &UiLocalState) -> impl Scene + use<> {
        let active = local.get(LOCAL_GAS).to_owned();
        let detail: Box<dyn SceneList> = match st.gas_info.iter().find(|g| g.id == active) {
            None => nothing_selected("No Gas Selected"),
            Some(gas) => {
                let title = format!("> {}", gas.name);
                let desc = gas.description.clone();
                let heat = format!("{}", gas.specific_heat);
                let heat_color = color(specific_heat_color(gas.specific_heat));
                let export = (gas.export_value > 0.0).then(|| {
                    let value = format!("{}", gas.export_value);
                    let unit = format!("{}/mol", st.money_symbol);
                    let tip = format!(
                        "The amount of {} gained per mol of gas exported.",
                        st.money_name
                    );
                    let c = color(export_value_color(gas.export_value));
                    EntityScene(bsn! {
                        @StackItem
                        Children [
                            @Stack
                            Children [
                                @StackItem Tooltip { content: tip, position: Placement::Top }
                                Children [ Text("Export Value:") ]
                                --
                                @StackItem Propagate::<TextColor>(TextColor({c.base()})) Children [ Text(value) ]
                                --
                                @StackItem Propagate::<Italic>(Italic(true)) Children [ Text(unit) ]
                            ]
                        ]
                    })
                });
                let mut related = gas.reactions.clone();
                related.sort_by(|a, b| a.1.cmp(&b.1));
                let related: Vec<_> = related
                    .into_iter()
                    .map(|(id, name)| {
                        let pick = set_local(LOCAL_REACTION, &id);
                        let tab = set_local(LOCAL_TAB, "reactions");
                        bsn! {
                            @StackItem
                            Children [
                                @ss13_ui_kit::Button { @content: name, @fluid: true }
                                pick
                                tab
                            ]
                        }
                    })
                    .collect();
                let related = (!related.is_empty()).then(|| {
                    EntityScene(bsn! {
                        @StackItem { @grow: 1.0 }
                        Children [
                            @Section { @title: "Relevant Reactions", @scrollable: true }
                            Children [ @Stack { @vertical: true } Children [ {related} ] ]
                        ]
                    })
                });
                let heat_tip = "The amount of energy required to raise the temperature of one \
                    mole of gas by one Kelvin. In simpler terms: The lower the specific heat of \
                    the gas, the harder it will be to heat it up."
                    .to_owned();
                Box::new(EntityScene(bsn! {
                    @Stack { @vertical: true }
                    Children [
                        @StackItem Propagate::<Bold>(Bold(true)) Children [ Text(title) ]
                        --
                        @StackItem Propagate::<Italic>(Italic(true)) Children [ Text(desc) ]
                        --
                        @StackItem
                        Children [
                            @Stack
                            Children [
                                @StackItem Tooltip { content: heat_tip, position: Placement::Top }
                                Children [ Text("Specific Heat:") ]
                                --
                                @StackItem Propagate::<TextColor>(TextColor({heat_color.base()})) Children [ Text(heat) ]
                                --
                                @StackItem Propagate::<Italic>(Italic(true)) Children [ Text("J/(mol * K)") ]
                            ]
                        ]
                        --
                        {export}
                        --
                        {related}
                    ]
                }))
            }
        };
        let list = pick_list(
            st.gas_info
                .iter()
                .map(|g| (g.id.clone(), g.name.clone()))
                .collect(),
            LOCAL_GAS,
            LOCAL_GAS_SEARCH,
            &active,
        );
        let bar = search(LOCAL_GAS_SEARCH, "Gas Lookup");
        bsn! {
            @Section
            Children [
                {EntityScene(bar)}
                --
                @Stack
                Children [
                    @StackItem Node { width: percent(60) } Children [ {detail} ]
                    --
                    @StackItem { @grow: 1.0 }
                    Children [
                        @Section { @scrollable: true }
                        Children [ @Stack { @vertical: true } Children [ {list} ] ]
                    ]
                ]
            ]
        }
    }

    fn reaction_tab(st: &AtmosHandbook, local: &UiLocalState) -> impl Scene + use<> {
        let active = local.get(LOCAL_REACTION).to_owned();
        let detail: Box<dyn SceneList> = match st.reaction_info.iter().find(|r| r.id == active) {
            None => nothing_selected("No Reaction Selected"),
            Some(reaction) => {
                let title = format!("> {}", reaction.name);
                let desc = reaction.description.clone();
                let factors: Vec<Box<dyn SceneList>> = reaction
                    .factors
                    .iter()
                    .map(|f| -> Box<dyn SceneList> {
                        let desc = f.desc.clone();
                        match (&f.factor_id, &f.tooltip) {
                            (Some(gas), _) if f.factor_type == "gas" => {
                                let pick = set_local(LOCAL_GAS, gas);
                                let tab = set_local(LOCAL_TAB, "gases");
                                let name = f.factor_name.clone();
                                Box::new(EntityScene(bsn! {
                                    @LabeledListItem { @label: "" }
                                    Children [
                                        @ss13_ui_kit::Button { @content: name, @fluid: true }
                                        pick
                                        tab
                                        --
                                        Text(desc)
                                    ]
                                }))
                            }
                            (_, Some(tip)) => {
                                let tip = tip.clone();
                                let label = format!("{}:", f.factor_name);
                                Box::new(EntityScene(bsn! {
                                    @LabeledListItem { @label: "" }
                                    Children [
                                        Node Tooltip { content: tip, position: Placement::Top }
                                        Children [ Text(label) ]
                                        --
                                        Text(desc)
                                    ]
                                }))
                            }
                            _ => {
                                let label = f.factor_name.clone();
                                Box::new(EntityScene(bsn! {
                                    @LabeledListItem { @label: label }
                                    Children [ Text(desc) ]
                                }))
                            }
                        }
                    })
                    .collect();
                Box::new(EntityScene(bsn! {
                    @Stack { @vertical: true }
                    Children [
                        @StackItem Propagate::<Bold>(Bold(true)) Children [ Text(title) ]
                        --
                        @StackItem Propagate::<Italic>(Italic(true)) Children [ Text(desc) ]
                        --
                        @StackItem { @grow: 1.0 }
                        Children [
                            @Section { @scrollable: true }
                            Children [ @LabeledList Children [ {factors} ] ]
                        ]
                    ]
                }))
            }
        };
        let list = pick_list(
            st.reaction_info
                .iter()
                .map(|r| (r.id.clone(), r.name.clone()))
                .collect(),
            LOCAL_REACTION,
            LOCAL_REACTION_SEARCH,
            &active,
        );
        let bar = search(LOCAL_REACTION_SEARCH, "Reaction Lookup");
        bsn! {
            @Section
            Children [
                {EntityScene(bar)}
                --
                @Stack
                Children [
                    @StackItem Node { width: percent(60) } Children [ {detail} ]
                    --
                    @StackItem { @grow: 1.0 }
                    Children [
                        @Section { @scrollable: true }
                        Children [ @Stack { @vertical: true } Children [ {list} ] ]
                    ]
                ]
            ]
        }
    }

    fn view(
        st: &AtmosHandbook,
        data: &GasAnalyzerData,
        local: &UiLocalState,
    ) -> impl Scene + use<> {
        let mixes: Vec<_> = data
            .gasmixes
            .iter()
            .map(|g| {
                let title = g.name.clone();
                let body = gasmix_view(g);
                bsn! { @Section { @title: title } Children [ {body} ] }
            })
            .collect();
        let book = handbook(st, local);
        bsn! {
            @WindowContent
            Children [
                @Stack { @vertical: true }
                Children [
                    @StackItem Children [ {mixes} ]
                    --
                    @StackItem { @grow: 1.0 } Children [ {EntityScene(book)} ]
                ]
            ]
        }
    }

    /// `AtmosHandbookContent`: the gas and reaction tabs.
    pub(crate) fn handbook(st: &AtmosHandbook, local: &UiLocalState) -> impl Scene + use<> {
        let reactions = local.get(LOCAL_TAB) == "reactions";
        let tabs = [
            ("gases", "Gases", !reactions),
            ("reactions", "Reactions", reactions),
        ]
        .map(|(key, label, selected)| {
            let pick = set_local(LOCAL_TAB, key);
            bsn! {
                @TabsTab { @content: label }
                TabsTab { selected: selected }
                pick
            }
        });
        let body: Box<dyn SceneList> = if reactions {
            Box::new(EntityScene(reaction_tab(st, local)))
        } else {
            Box::new(EntityScene(gas_tab(st, local)))
        };
        bsn! {
            Node { flex_direction: FlexDirection::Column }
            Children [
                @Tabs Children [ {Vec::from(tabs)} ]
                --
                {body}
            ]
        }
    }

    impl ClientUi for GasAnalyzerUi {
        const WIDTH: f32 = 500.0;
        const HEIGHT: f32 = 500.0;

        fn content(st: &AtmosHandbook, data: &GasAnalyzerData) -> impl Scene {
            view(st, data, &UiLocalState::default())
        }

        fn content_local(
            st: &AtmosHandbook,
            data: &GasAnalyzerData,
            local: &UiLocalState,
        ) -> impl Scene {
            view(st, data, local)
        }
    }
}
