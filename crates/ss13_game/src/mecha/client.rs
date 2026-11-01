//! The exosuit fabricator window (client): tgui `ExosuitFabricator` (`ExosuitFabricator.tsx`
//! with `Fabrication/DesignBrowser.tsx`, `MaterialAccessBar.tsx`): the researched designs by
//! category (a click builds now; `+` queues, `▶` builds now; "Всё в очередь" queues the
//! category), the materials with eject buttons, and the queue with its total cost, start/stop,
//! clear and per-part removal. The data is in [`super`], the server half in [`super::fab`].
// shortcut: no search bar, no queue progress bar (the time left is text), costs as tooltip text.

use bevy::scene::EntityScene;
use ss13_tgui::client::{AppTguiClientExt, ClientUi, SetLocal, UiLocalState};
use ss13_ui_kit::prelude::*;

use super::*;
use crate::science::client::{
    ALL_CATEGORY, CATEGORY, material_bar, sheets_text, sprite, top_categories,
};
use crate::science::{FabDesign, FabricatorData, FabricatorStatic};

pub struct MechaClientPlugin;

impl Plugin for MechaClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_view::<ExosuitFabricatorUi>();
    }
}

impl ClientUi for ExosuitFabricatorUi {
    const WIDTH: f32 = 1100.0;
    const HEIGHT: f32 = 600.0;

    fn content(st: &ExoFabStatic, data: &ExoFabData) -> impl Scene {
        view(st, data, &UiLocalState::default())
    }

    fn content_local(st: &ExoFabStatic, data: &ExoFabData, local: &UiLocalState) -> impl Scene {
        view(st, data, local)
    }
}

fn categories_of(design: &ExoDesign) -> Vec<String> {
    top_categories(&FabDesign {
        categories: design.categories.clone(),
        ..default()
    })
}

fn build_params(paths: Vec<String>, now: bool) -> ActParams {
    let mut params = vec![(DESIGNS.to_owned(), paths.join(","))];
    if now {
        params.push((NOW.to_owned(), "1".to_owned()));
    }
    ActParams(params)
}

fn cost_text(st: &ExoFabStatic, design: &ExoDesign) -> String {
    let mut out = design.desc.clone();
    for (m, n) in &design.cost {
        let name = st
            .materials
            .get(usize::from(*m))
            .map_or("?", |m| m.name.as_str());
        out.push_str(&format!(
            "\n{name}: {}",
            sheets_text(f64::from(*n) / st.sheet_amount)
        ));
    }
    out
}

fn view(st: &ExoFabStatic, data: &ExoFabData, local: &UiLocalState) -> impl Scene + use<> {
    let designs: Vec<&ExoDesign> = data
        .researched
        .iter()
        .filter_map(|i| st.designs.get(usize::from(*i)))
        .collect();
    let mut categories: Vec<String> = designs.iter().flat_map(|d| categories_of(d)).collect();
    categories.sort();
    categories.dedup();
    let selected = match local.get(CATEGORY) {
        "" => ALL_CATEGORY.to_owned(),
        c => c.to_owned(),
    };
    let tab = |key: String, count: usize| {
        let label = key.trim_start_matches('/').to_owned();
        let is_selected = key == selected;
        let count = count.to_string();
        bsn! {
            @TabsTab
            TabsTab { selected: is_selected }
            SetLocal { key: {CATEGORY.to_owned()}, value: key, rerender: true }
            Children [
                Node {
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::SpaceBetween,
                    flex_grow: 1.0,
                }
                Children [ Text(label) -- Text(count) ]
            ]
        }
    };
    let mut tabs = vec![tab(ALL_CATEGORY.to_owned(), designs.len())];
    for c in &categories {
        let count = designs
            .iter()
            .filter(|d| categories_of(d).contains(c))
            .count();
        tabs.push(tab(c.clone(), count));
    }
    let mut shown: Vec<&ExoDesign> = designs
        .iter()
        .copied()
        .filter(|d| selected == ALL_CATEGORY || categories_of(d).contains(&selected))
        .collect();
    shown.sort_by(|a, b| a.name.cmp(&b.name));
    let all_paths: Vec<String> = shown.iter().map(|d| d.path.clone()).collect();
    let rows: Vec<_> = shown.into_iter().map(|d| recipe(st, data, d)).collect();
    let queue_params = build_params(all_paths, false);
    let queue_all: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
        @ss13_ui_kit::Button { @content: "Всё в очередь" }
        ss13_ui_kit::Button { color: UiColor::Transparent }
        Act({BUILD.to_owned()})
        queue_params
        Name("exofab: queue all")
    }));
    let materials = {
        let fst = FabricatorStatic {
            materials: st.materials.clone(),
            sheet_amount: st.sheet_amount,
            ..default()
        };
        let fdata = FabricatorData {
            materials: data.materials.clone(),
            ..default()
        };
        EntityScene(material_bar(&fst, &fdata))
    };
    bsn! {
        @WindowContent
        Children [
            @Stack { @fill: true }
            Children [
                @StackItem { @grow: 1.0 }
                Node { min_width: px(0) }
                Children [
                    @Stack { @vertical: true, @fill: true }
                    Children [
                        @StackItem { @grow: 1.0 }
                        Node { min_height: px(0) }
                        Children [
                            @Stack { @fill: true }
                            Children [
                                @StackItem
                                Node { width: px(220) }
                                Children [
                                    @Section { @fill: true, @scrollable: true }
                                    Children [ @Tabs { @vertical: true } Children [ {tabs} ] ]
                                ]
                                --
                                @StackItem { @grow: 1.0 }
                                Node { min_width: px(0) }
                                Children [
                                    @Section { @fill: true, @scrollable: true, @title: {selected.trim_start_matches('/').to_owned()}, @buttons: queue_all }
                                    Children [ {rows} ]
                                ]
                            ]
                        ]
                        --
                        @StackItem
                        Children [ @Section Children [ {materials} ] ]
                    ]
                ]
                --
                @StackItem
                Node { width: px(420) }
                Children [ {EntityScene(queue(st, data))} ]
            ]
        ]
    }
}

/// `Recipe`: a click builds now; `+` queues, `▶` builds now.
fn recipe(st: &ExoFabStatic, data: &ExoFabData, design: &ExoDesign) -> impl Scene + use<> {
    let can_print = data.can_print(design);
    let button = |label: &str, now: bool, name: String| {
        let params = build_params(vec![design.path.clone()], now);
        let label = label.to_owned();
        EntityScene(bsn! {
            @ss13_ui_kit::Button { @content: label, @compact: true }
            ss13_ui_kit::Button { color: UiColor::Transparent, disabled: {!can_print} }
            Act({BUILD.to_owned()})
            params
            Name(name)
        })
    };
    let buttons: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
        Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center }
        Children [
            {button("+", false, format!("queue: {}", design.name))}
            --
            {button("▶", true, format!("build now: {}", design.name))}
        ]
    }));
    let image = sprite(design.icon);
    let name = design.name.clone();
    let tooltip = cost_text(st, design);
    let params = build_params(vec![design.path.clone()], true);
    let button_name = Name::new(format!("print: {}", design.name));
    bsn! {
        @ImageButton {
            @image: image,
            @image_size: 32.0,
            @fluid: true,
            @tooltip: tooltip,
            @buttons_alt: buttons,
        }
        ImageButton { disabled: {!can_print} }
        Act({BUILD.to_owned()})
        params
        button_name
        Children [ Text(name) ]
    }
}

/// `Queue` and `QueueList`.
fn queue(st: &ExoFabStatic, data: &ExoFabData) -> impl Scene + use<> {
    let mut totals: Vec<u32> = vec![0; st.materials.len()];
    for entry in &data.queue {
        if let Some(d) = st.designs.get(usize::from(entry.design)) {
            for &(m, n) in &d.cost {
                if let Some(t) = totals.get_mut(usize::from(m)) {
                    *t += n;
                }
            }
        }
    }
    let total_text: Vec<String> = totals
        .iter()
        .enumerate()
        .filter(|(_, n)| **n > 0)
        .map(|(i, n)| {
            format!(
                "{}: {}",
                st.materials[i].name,
                sheets_text(f64::from(*n) / st.sheet_amount)
            )
        })
        .collect();
    let empty = data.queue.is_empty();
    let control: Box<dyn SceneList> = if data.processing {
        Box::new(EntityScene(bsn! {
            @ss13_ui_kit::Button { @icon: "stop", @content: "Остановить" }
            ss13_ui_kit::Button { disabled: empty }
            Act({STOP_QUEUE.to_owned()})
            Name("exofab: stop")
        }))
    } else {
        Box::new(EntityScene(bsn! {
            @ss13_ui_kit::Button { @icon: "play", @content: "Начать сборку" }
            ss13_ui_kit::Button { disabled: empty }
            Act({BUILD_QUEUE.to_owned()})
            Name("exofab: start")
        }))
    };
    let buttons: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
        Node { flex_direction: FlexDirection::Row }
        Children [
            @ss13_ui_kit::Button { @icon: "minus-circle", @content: "Очистить очередь" }
            ss13_ui_kit::Button { color: UiColor::Bad, disabled: empty }
            Act({CLEAR_QUEUE.to_owned()})
            Name("exofab: clear")
            --
            {control}
        ]
    }));
    let first_processing = data.queue.first().is_some_and(|e| e.processing);
    let rows: Vec<_> = data
        .queue
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let design = st.designs.get(usize::from(entry.design)).cloned().unwrap_or_default();
            let label = if entry.processing {
                format!("{} ({:.0} с)", design.name, entry.time_left / 10.0)
            } else {
                design.name.clone()
            };
            // `del_queue_part` indexes the queue without the part being built, from 1.
            let index = if first_processing { i } else { i + 1 };
            let remove = (!entry.processing).then(|| {
                let params = ActParams(vec![(INDEX.to_owned(), index.to_string())]);
                EntityScene(bsn! {
                    @ss13_ui_kit::Button { @icon: "minus-circle", @compact: true }
                    ss13_ui_kit::Button { color: UiColor::Transparent }
                    Act({DEL_QUEUE_PART.to_owned()})
                    params
                })
            });
            let image = sprite(design.icon);
            bsn! {
                Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, margin: UiRect::bottom(px(2)) }
                Children [
                    Node { width: px(32), height: px(32) } Children [ {image} ]
                    --
                    Node { flex_grow: 1.0, margin: UiRect::left(px(4)) } Children [ Text(label) ]
                    --
                    {remove}
                ]
            }
        })
        .collect();
    let totals = total_text.join("\n");
    bsn! {
        @Section { @fill: true }
        Children [
            @Stack { @vertical: true, @fill: true }
            Children [
                @StackItem
                Children [
                    @Section { @title: "Очередь", @buttons: buttons }
                    Children [ Text(totals) ]
                ]
                --
                @StackItem { @grow: 1.0 }
                Node { min_height: px(0) }
                Children [ @Section { @fill: true, @scrollable: true } Children [ {rows} ] ]
            ]
        ]
    }
}
