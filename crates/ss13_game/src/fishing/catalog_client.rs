//! tgui `FishAnalyzer`, `FishCatalog` and the admin `FishingCalculator` (client).
// shortcut: no fish icons (the fish spritesheet), evolutions or breeding rows; the catalog
// picks a fish from a list of buttons instead of the paged book.

use bevy::scene::EntityScene;
use ss13_tgui::client::{ActOnSubmit, ClientUi, SetLocal, UiLocalState};
use ss13_ui_kit::prelude::*;

use super::{
    AnalyzedFish, CALC_RECALC, CALC_SET_BAIT, CALC_SET_ROD, CALC_SET_SPOT, CALC_VALUE,
    CalculatorData, CalculatorStatic, FishAnalyzerData, FishAnalyzerUi, FishCatalogStatic,
    FishCatalogUi, FishingCalculatorUi,
};

/// `useLocalState('currentPage')` of the catalog: the shown fish's index.
const LOCAL_FISH: &str = "fish";

fn row(label: &'static str, value: String) -> impl Scene {
    bsn! { @LabeledListItem { @label: label, @content: value } }
}

impl ClientUi for FishAnalyzerUi {
    const WIDTH: f32 = 500.0;
    const HEIGHT: f32 = 500.0;

    fn content(_: &(), data: &FishAnalyzerData) -> impl Scene {
        analyzer_view(data)
    }
}

fn analyzer_view(data: &FishAnalyzerData) -> impl Scene + use<> {
    let fish: Vec<_> = data.fish_list.iter().map(fish_section).collect();
    let empty = data
        .fish_list
        .is_empty()
        .then(|| EntityScene(bsn! { Node Children [ Text({"No fish detected.".to_owned()}) ] }));
    bsn! {
        @WindowContent { @scrollable: true }
        Children [
            Node { flex_direction: FlexDirection::Column }
            Children [ {empty} -- {fish} ]
        ]
    }
}

fn fish_section(f: &AnalyzedFish) -> impl Scene + use<> {
    let title = f.fish_name.clone();
    let traits: Vec<_> = f
        .fish_traits
        .iter()
        .map(|t| {
            let text = format!(
                "{} ({}% inheritance): {}",
                t.trait_name, t.trait_inherit, t.trait_desc
            );
            bsn! { Node Children [ Text(text) ] }
        })
        .collect();
    let temp = format!(
        "{}-{} K{}",
        f.fish_min_temp.round(),
        f.fish_max_temp.round(),
        if f.fish_suitable_temp {
            ""
        } else {
            " (unsuitable)"
        }
    );
    let fluid = format!(
        "{}{}",
        f.fish_fluid_type,
        if f.fish_fluid_compatible {
            ""
        } else {
            " (incompatible)"
        }
    );
    bsn! {
        @Section { @title: title }
        Children [
            @LabeledList
            Children [
                {row("Health", format!("{}%", f.fish_health.round()))}
                --
                {row("Size", format!("{} cm", f.fish_size))}
                --
                {row("Weight", format!("{} g", f.fish_weight))}
                --
                {row("Hunger", format!("{}%", (f.fish_hunger * 100.0).round()))}
                --
                {row("Food", f.fish_food.clone())}
                --
                {row("Temperature", temp)}
                --
                {row("Fluid", fluid)}
            ]
            --
            Node { flex_direction: FlexDirection::Column }
            Children [ {traits} ]
        ]
    }
}

impl ClientUi for FishCatalogUi {
    const WIDTH: f32 = 600.0;
    const HEIGHT: f32 = 500.0;

    fn content(s: &FishCatalogStatic, _: &()) -> impl Scene {
        catalog_view(s, &UiLocalState::default())
    }

    fn content_local(s: &FishCatalogStatic, _: &(), local: &UiLocalState) -> impl Scene {
        catalog_view(s, local)
    }
}

fn catalog_view(s: &FishCatalogStatic, local: &UiLocalState) -> impl Scene + use<> {
    let shown: Option<usize> = local.get(LOCAL_FISH).parse().ok();
    let list: Vec<_> = s
        .fish_info
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let name = f.name.clone();
            let pick = SetLocal {
                key: LOCAL_FISH.into(),
                value: i.to_string(),
                rerender: true,
            };
            let selected = shown == Some(i);
            bsn! {
                @Button { @content: name, @fluid: true }
                Button { selected: selected }
                pick
            }
        })
        .collect();
    let details: Box<dyn Scene> = match shown.and_then(|i| s.fish_info.get(i)) {
        Some(f) => {
            let traits: Vec<_> = f
                .traits
                .iter()
                .map(|t| {
                    let t = t.clone();
                    bsn! { Node Children [ Text(t) ] }
                })
                .collect();
            let title = f.name.clone();
            let desc = f.desc.clone();
            Box::new(bsn! {
                @Section { @title: title }
                Children [
                    Node Children [ Text(desc) ]
                    --
                    @LabeledList
                    Children [
                        {row("Required fluid", f.fluid.clone())}
                        --
                        {row("Temperature", format!("{}-{} K", f.temp_min.round(), f.temp_max.round()))}
                        --
                        {row("Average size", format!("{} cm", f.size))}
                        --
                        {row("Average weight", format!("{} g", f.weight))}
                        --
                        {row("Feed", f.feed.clone())}
                        --
                        {row("Acquisition", f.source.clone())}
                        --
                        {row("Spots", f.spots.clone())}
                        --
                        {row("Difficulty", f.difficulty.clone())}
                        --
                        {row("Favorite bait", f.favorite_bait.clone())}
                        --
                        {row("Disliked bait", f.disliked_bait.clone())}
                    ]
                    --
                    Node { flex_direction: FlexDirection::Column }
                    Children [ {traits} ]
                ]
            })
        }
        None => {
            let sponsor = format!("Sponsored by {}", s.sponsored_by);
            Box::new(bsn! { @Section Children [ Text(sponsor) ] })
        }
    };
    bsn! {
        @WindowContent
        Children [
            Node
            Children [
                Node { width: px(180), margin: UiRect::right(px(6)) }
                Children [
                    @Section { @fill: true, @scrollable: true }
                    Children [ Node { flex_direction: FlexDirection::Column } Children [ {list} ] ]
                ]
                --
                Node { flex_grow: 1.0, flex_basis: px(0) }
                Children [ {EntityScene(details)} ]
            ]
        ]
    }
}

impl ClientUi for FishingCalculatorUi {
    const WIDTH: f32 = 500.0;
    const HEIGHT: f32 = 500.0;

    fn content(s: &CalculatorStatic, data: &CalculatorData) -> impl Scene {
        calculator_view(s, data)
    }
}

fn choice(value: &str, current: &str, action: &'static str) -> impl Scene + use<> {
    let label = value.to_owned();
    let selected = value == current;
    let params = ActParams(vec![(CALC_VALUE.to_owned(), value.to_owned())]);
    bsn! {
        @Button { @content: label }
        Button { selected: selected }
        Act({action.to_owned()})
        params
    }
}

fn calculator_view(s: &CalculatorStatic, data: &CalculatorData) -> impl Scene + use<> {
    let spots: Vec<_> = s
        .spot_types
        .iter()
        .map(|p| choice(p, &data.spot, CALC_SET_SPOT))
        .collect();
    let rods: Vec<_> = s
        .rod_types
        .iter()
        .map(|p| choice(p, &data.rod, CALC_SET_ROD))
        .collect();
    let bait = data.bait.clone();
    let submit = ActOnSubmit {
        action: CALC_SET_BAIT.to_owned(),
        param: CALC_VALUE.to_owned(),
        params: Vec::new(),
    };
    let sum: f64 = data.info.iter().map(|r| r.weight).sum::<f64>().max(1.0);
    let rows: Vec<_> = data
        .info
        .iter()
        .map(|r| {
            let text = format!(
                "{} | weight {} | {:.2}% | difficulty {} | count {}",
                r.result,
                r.weight,
                r.weight / sum * 100.0,
                r.difficulty,
                r.count
            );
            bsn! { Node Children [ Text(text) ] }
        })
        .collect();
    bsn! {
        @WindowContent { @scrollable: true }
        Children [
            @Section { @title: "Spot" }
            Children [ Node { flex_wrap: FlexWrap::Wrap } Children [ {spots} ] ]
            --
            @Section { @title: "Rod" }
            Children [ Node { flex_wrap: FlexWrap::Wrap } Children [ {rods} ] ]
            --
            @Section { @title: "Bait (Enter to set)" }
            Children [
                Node { width: percent(100) }
                Input { value: bait, placeholder: {"Bait".to_owned()}, fluid: true }
                submit
            ]
            --
            @Button { @icon: "calculator", @content: "Calculate" }
            Act({CALC_RECALC.to_owned()})
            --
            @Section { @title: "Outcomes" }
            Children [ Node { flex_direction: FlexDirection::Column } Children [ {rows} ] ]
        ]
    }
}
