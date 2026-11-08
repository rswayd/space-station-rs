//! The research windows (client): tgui `Techweb` (`Techweb/Start.tsx`, `Content.tsx`,
//! `Overview.tsx`, `nodes/TechNode.tsx`, `nodes/detail.tsx`), `Fabricator` (`Fabricator.tsx`
//! with `Fabrication/DesignBrowser.tsx` and `MaterialAccessBar.tsx`), `DestructiveAnalyzer`,
//! `ExperimentConfigure` and `ServerControl`.
//! The data is in [`super`], the server halves in [`super::console`], [`super::lathe`] and
//! [`super::analyzer`].
//!
//! Client-only state (`useState`): the Techweb tab and the node whose details are shown, the
//! Fabricator's category ([`UiLocalState`] keys).
// shortcut: no search bars, no experiment sections in the node cards, no custom print amount
// input (`[Max: n]` prints the maximum), design tooltips list the costs as text.

use bevy::app::Propagate;
use bevy::scene::EntityScene;
use ss13_tgui::client::{AppTguiClientExt, ClientUi, SetLocal, UiLocalState, UiSprite, memo};
use ss13_tgui::interfaces::vending::ProductIcon;
use ss13_ui_kit::prelude::*;
use ss13_ui_kit::theme::{Bold, FontPx};

use super::*;

const TAB: &str = "techtab";
const DETAILS: &str = "techdetails";
pub(crate) const CATEGORY: &str = "selected_category";
/// `DesignBrowser`'s meta-categories and the ones it does not show.
pub(crate) const ALL_CATEGORY: &str = "All Designs";
const UNCATEGORIZED: &str = "/Uncategorized";
const BLACKLISTED_CATEGORIES: [&str; 3] = ["initial", "core", "hacked"];

pub struct ScienceClientPlugin;

impl Plugin for ScienceClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_view::<TechwebUi>()
            .register_ui_view::<FabricatorUi>()
            .register_ui_view::<AutolatheUi>()
            .register_ui_view::<DestructiveAnalyzerUi>()
            .register_ui_view::<ExperimentConfigureUi>()
            .register_ui_view::<ServerControlUi>()
            .register_ui_view::<ExperimentorUi>()
            .register_ui_view::<super::anomaly_refinery::AnomalyRefineryUi>()
            .register_ui_view::<super::ordnance::DopplerArrayUi>()
            .register_ui_view::<super::ordnance::TankCompressorUi>()
            .add_plugins((
                super::exodrone::client::ExodroneClientPlugin,
                super::bitrunning::client::BitrunningClientPlugin,
                super::teleportation::client::TeleportationClientPlugin,
            ));
    }
}

pub(crate) fn sprite(icon: Option<ProductIcon>) -> Box<dyn SceneList> {
    match icon {
        Some(icon) => Box::new(EntityScene(bsn! {
            Node { width: percent(100), height: percent(100) }
            UiSprite { icon_state: {Some(icon.icon_state)}, color: {icon.color} }
        })),
        None => Box::new(EntityScene(bsn! {
            Node {
                width: percent(100),
                height: percent(100),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
            }
            Children [ @Icon { @name: "question" } ]
        })),
    }
}

fn text_row(text: String) -> impl Scene {
    bsn! { Node { margin: UiRect::bottom(px(4)) } Children [ Text(text) ] }
}

/// Points as tg prints them (`0.1` steps).
fn points_text(n: f64) -> String {
    let rounded = (n * 10.0).round() / 10.0;
    if rounded.fract() == 0.0 {
        format!("{rounded:.0}")
    } else {
        format!("{rounded:.1}")
    }
}

// --- Techweb -------------------------------------------------------------------------------

impl ClientUi for TechwebUi {
    const WIDTH: f32 = 640.0;
    const HEIGHT: f32 = 735.0;

    fn content(st: &TechwebStatic, data: &TechwebData) -> impl Scene {
        techweb_view(st, data, &UiLocalState::default())
    }

    fn content_local(st: &TechwebStatic, data: &TechwebData, local: &UiLocalState) -> impl Scene {
        techweb_view(st, data, local)
    }
}

/// `TechwebStart`.
fn techweb_view(
    st: &TechwebStatic,
    data: &TechwebData,
    local: &UiLocalState,
) -> impl Scene + use<> {
    let body: Box<dyn SceneList> = if data.locked {
        Box::new(EntityScene(bsn! {
            @Modal
            Children [
                Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center }
                Children [
                    Node { margin: UiRect::bottom(px(6)) }
                    Propagate::<Bold>(Bold(true))
                    Children [ Text("Console Locked") ]
                    --
                    @ss13_ui_kit::Button { @icon: "unlock", @content: "Unlock" }
                    Act({TOGGLE_LOCK.to_owned()})
                    Name("techweb: unlock")
                ]
            ]
        }))
    } else if !data.stored_research {
        Box::new(EntityScene(bsn! {
            @Modal
            Children [
                Propagate::<Bold>(Bold(true))
                Children [ Text("No research techweb found, please synchronize the console.") ]
            ]
        }))
    } else {
        Box::new(EntityScene(techweb_content(st, data, local)))
    };
    bsn! { @WindowContent Children [ {body} ] }
}

/// `TechwebContent`: the header and the routed view.
fn techweb_content(
    st: &TechwebStatic,
    data: &TechwebData,
    local: &UiLocalState,
) -> impl Scene + use<> {
    let abbreviation = |t: &str| {
        st.point_types
            .iter()
            .find(|(p, _)| p == t)
            .map_or_else(|| t.to_owned(), |(_, a)| a.clone())
    };
    let points: Vec<_> = data
        .points
        .iter()
        .map(|(t, n)| {
            let label = abbreviation(t);
            let income = data
                .points_last_tick
                .iter()
                .find(|(p, _)| p == t)
                .map(|(_, n)| *n)
                .filter(|n| *n != 0.0)
                .map(|n| format!(" (+{}/sec)", points_text(n)))
                .unwrap_or_default();
            let value = points_text(*n);
            bsn! {
                @LabeledListItem { @label: label }
                Children [
                    Node { flex_direction: FlexDirection::Row }
                    Children [
                        Node Propagate::<Bold>(Bold(true)) Children [ Text(value) Name("techweb: points") ]
                        --
                        Text(income)
                    ]
                ]
            }
        })
        .collect();
    let queue: Box<dyn SceneList> = if data.queue.is_empty() {
        Box::new(EntityScene(bsn! { Node Children [ Text("Empty") ] }))
    } else {
        let buttons: Vec<_> = data
            .queue
            .iter()
            .map(|(n, by)| {
                let name = st
                    .nodes
                    .get(usize::from(*n))
                    .map(|n| n.name.clone())
                    .unwrap_or_default();
                let tooltip = format!("Added by: {by}");
                bsn! { @ss13_ui_kit::Button { @content: name, @tooltip: tooltip } }
            })
            .collect();
        Box::new(EntityScene(bsn! {
            Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap }
            Children [ {buttons} ]
        }))
    };
    let (sec_text, sec_color) = if data.sec_protocols {
        ("Engaged", UiColor::Good)
    } else {
        ("Disengaged", UiColor::Bad)
    };
    let sec_color = sec_color.text().unwrap_or_default();
    let disks = disk_rows(st, data);
    let routed: Box<dyn SceneList> = match local
        .get(DETAILS)
        .parse::<u16>()
        .ok()
        .filter(|id| data.node(*id).is_some())
    {
        Some(id) => Box::new(EntityScene(node_detail(st, data, id))),
        None => Box::new(EntityScene(overview(st, data, local))),
    };
    bsn! {
        @Stack { @vertical: true, @fill: true }
        Children [
            @StackItem
            Children [
                Node { flex_direction: FlexDirection::Row, margin: UiRect::bottom(px(6)) }
                Children [
                    Node { flex_grow: 1.0 }
                    Children [
                        @LabeledList
                        Children [
                            @LabeledListItem { @label: "Security" }
                            Children [ Text(sec_text) TextColor(sec_color) ]
                            --
                            {points}
                            --
                            @LabeledListItem { @label: "Queue" }
                            Children [ {queue} ]
                            --
                            {disks}
                        ]
                    ]
                    --
                    Node
                    Children [
                        @ss13_ui_kit::Button { @icon: "lock", @content: "Lock Console", @fluid: true }
                        Act({TOGGLE_LOCK.to_owned()})
                        Name("techweb: lock")
                    ]
                ]
            ]
            --
            @StackItem { @grow: 1.0 }
            Node { min_height: px(0) }
            Children [ {routed} ]
        ]
    }
}

fn disk_button(
    icon: &'static str,
    label: &'static str,
    act: &'static str,
    kind: &'static str,
) -> impl Scene + use<> {
    let params = ActParams(vec![(DISK_TYPE.to_owned(), kind.to_owned())]);
    let name = Name::new(format!("techweb: {act} {kind}"));
    bsn! {
        @ss13_ui_kit::Button { @icon: icon, @content: label }
        Act({act.to_owned()})
        params
        name
    }
}

/// `TechwebDiskMenu` of each inserted disk, as header rows: its contents and buttons.
fn disk_rows(st: &TechwebStatic, data: &TechwebData) -> Vec<Box<dyn SceneList>> {
    let mut rows: Vec<Box<dyn SceneList>> = Vec::new();
    if let Some(nodes) = &data.t_disk {
        let names: Vec<String> = nodes
            .iter()
            .filter_map(|&n| st.nodes.get(usize::from(n)).map(|n| n.name.clone()))
            .collect();
        let summary = format!("{} nodes: {}", names.len(), names.join(", "));
        let upload = EntityScene(disk_button("upload", "Web -> Disk", LOAD_TECH, TECH_DISK));
        let download = EntityScene(disk_button(
            "download",
            "Disk -> Web",
            UPLOAD_DISK,
            TECH_DISK,
        ));
        let eject = EntityScene(disk_button("eject", "Eject Disk", EJECT_DISK, TECH_DISK));
        rows.push(Box::new(EntityScene(bsn! {
            @LabeledListItem { @label: "Tech Disk" }
            Children [
                Node { flex_direction: FlexDirection::Column }
                Children [
                    Text(summary)
                    --
                    Node { flex_direction: FlexDirection::Row }
                    Children [ {upload} -- {download} -- {eject} ]
                ]
            ]
        })));
    }
    if let Some(blueprints) = &data.d_disk {
        let summary = format!("{} designs: {}", blueprints.len(), blueprints.join(", "));
        let upload = EntityScene(disk_button(
            "download",
            "Upload All",
            UPLOAD_DISK,
            DESIGN_DISK,
        ));
        let eject = EntityScene(disk_button("eject", "Eject Disk", EJECT_DISK, DESIGN_DISK));
        rows.push(Box::new(EntityScene(bsn! {
            @LabeledListItem { @label: "Design Disk" }
            Children [
                Node { flex_direction: FlexDirection::Column }
                Children [
                    Text(summary)
                    --
                    Node { flex_direction: FlexDirection::Row }
                    Children [ {upload} -- {eject} ]
                ]
            ]
        })));
    }
    rows
}

/// `TechwebOverview`: Researched (tier 0), Available (tier 1), Future (tier 2+).
fn overview(st: &TechwebStatic, data: &TechwebData, local: &UiLocalState) -> impl Scene + use<> {
    let tab: u32 = local.get(TAB).parse().unwrap_or(1);
    let mut nodes: Vec<&NodeState> = data
        .nodes
        .iter()
        .filter(|n| {
            if tab < 2 {
                n.tier == tab
            } else {
                n.tier >= tab
            }
        })
        .collect();
    let name = |n: &NodeState| {
        st.nodes
            .get(usize::from(n.id))
            .map(|i| i.name.clone())
            .unwrap_or_default()
    };
    nodes.sort_by_key(|n| name(n));
    let cards: Vec<_> = nodes
        .into_iter()
        .map(|n| node_card(st, data, n, true))
        .collect();
    let tab_button = |index: u32, label: &'static str| {
        EntityScene(bsn! {
            @TabsTab { @content: label }
            TabsTab { selected: {tab == index} }
            SetLocal { key: {TAB.to_owned()}, value: {index.to_string()}, rerender: true }
        })
    };
    let (researched, available, future) = (
        tab_button(0, "Researched"),
        tab_button(1, "Available"),
        tab_button(2, "Future"),
    );
    bsn! {
        @Stack { @vertical: true, @fill: true }
        Children [
            @StackItem
            Children [
                Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center }
                Children [
                    Node { margin: UiRect::right(px(8)) }
                    Propagate::<Bold>(Bold(true))
                    Children [ Text("Web View") ]
                    --
                    Node { flex_grow: 1.0 }
                    Children [
                        @Tabs
                        Children [ {researched} -- {available} -- {future} ]
                    ]
                ]
            ]
            --
            @StackItem { @grow: 1.0 }
            Node { min_height: px(0) }
            Children [
                @Section { @fill: true, @scrollable: true }
                Children [ {cards} ]
            ]
        ]
    }
}

/// `TechNode`.
fn node_card(
    st: &TechwebStatic,
    data: &TechwebData,
    node: &NodeState,
    details: bool,
) -> impl Scene + use<> {
    let info = st
        .nodes
        .get(usize::from(node.id))
        .cloned()
        .unwrap_or_default();
    let in_queue = data.queue.iter().any(|(n, _)| *n == node.id);
    let tech_done = info
        .prerequisites
        .iter()
        .filter(|p| data.node(**p).is_some_and(|s| s.tier == 0))
        .count();
    let research_name = Name::new(format!("research: {}", info.name));
    let control: Option<Box<dyn SceneList>> = (node.tier > 0).then(|| -> Box<dyn SceneList> {
        let params = ActParams(vec![(NODE_PATH.to_owned(), info.path.clone())]);
        if node.can_unlock && (node.is_free || data.queue.is_empty()) {
            let disabled = !node.can_unlock || node.tier > 1 || !data.queue.is_empty();
            Box::new(EntityScene(bsn! {
                @ss13_ui_kit::Button { @icon: "lightbulb", @content: "Research" }
                ss13_ui_kit::Button { disabled: disabled }
                Act({RESEARCH_NODE.to_owned()})
                params
                research_name
            }))
        } else if node.enqueued_by_user {
            Box::new(EntityScene(bsn! {
                @ss13_ui_kit::Button { @icon: "trash", @content: "Dequeue" }
                ss13_ui_kit::Button { color: UiColor::Bad }
                Act({DEQUEUE_NODE.to_owned()})
                params
            }))
        } else if in_queue {
            Box::new(EntityScene(bsn! {
                @ss13_ui_kit::Button { @icon: "check", @content: "Queued" }
                ss13_ui_kit::Button { color: UiColor::Good }
            }))
        } else {
            let disabled =
                !node.have_experiments_done || in_queue || tech_done < info.prerequisites.len();
            Box::new(EntityScene(bsn! {
                @ss13_ui_kit::Button { @icon: "lightbulb", @content: "Enqueue" }
                ss13_ui_kit::Button { disabled: disabled }
                Act({ENQUEUE_NODE.to_owned()})
                params
            }))
        }
    });
    let details_button = details.then(|| {
        EntityScene(bsn! {
            @ss13_ui_kit::Button { @icon: "tasks", @content: "Details" }
            SetLocal { key: {DETAILS.to_owned()}, value: {node.id.to_string()}, rerender: true }
        })
    });
    let buttons: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
        Node { flex_direction: FlexDirection::Row }
        Children [ {control} -- {details_button} ]
    }));
    let discount: f64 = info
        .discount_experiments
        .iter()
        .filter(|(e, _)| data.completed_experiments.contains(e))
        .map(|(_, n)| n)
        .sum();
    let progress = (node.tier != 0).then(|| {
        let mut bars: Vec<_> = info
            .costs
            .iter()
            .map(|(t, value)| {
                let required = (value - discount).max(0.0);
                let have = data
                    .points
                    .iter()
                    .find(|(p, _)| p == t)
                    .map_or(0.0, |(_, n)| *n);
                let fraction = if required == 0.0 {
                    1.0
                } else {
                    (have / required).min(1.0)
                };
                let abbreviation = st
                    .point_types
                    .iter()
                    .find(|(p, _)| p == t)
                    .map_or_else(|| t.clone(), |(_, a)| a.clone());
                let label = format!(
                    "{abbreviation} ({}/{})",
                    points_text(required.min(have)),
                    points_text(required)
                );
                progress_bar(fraction as f32, label)
            })
            .collect();
        if !info.prerequisites.is_empty() {
            let fraction = tech_done as f32 / info.prerequisites.len() as f32;
            bars.push(progress_bar(
                fraction,
                format!("Tech ({tech_done}/{})", info.prerequisites.len()),
            ));
        }
        EntityScene(bsn! {
            Node { flex_direction: FlexDirection::Row, column_gap: px(4), margin: UiRect::bottom(px(6)) }
            Children [ {bars} ]
        })
    });
    // The description and design icons come from static data only: reused across the
    // once-a-second point updates instead of respawning hundreds of image buttons.
    let fixed = memo(MEMO_NODE_CARD | u64::from(node.id), || {
        let designs: Vec<_> = info
            .designs
            .iter()
            .filter_map(|d| st.designs.get(usize::from(*d)))
            .map(|d| {
                let tooltip = design_tooltip(st, d);
                let image = sprite(d.icon);
                bsn! {
                    @ImageButton { @image: image, @image_size: 32.0, @tooltip: tooltip }
                }
            })
            .collect();
        let description = info.description.clone();
        bsn! {
            Node { flex_direction: FlexDirection::Column }
            Children [
                Node { margin: UiRect::bottom(px(6)) }
                Children [ Text(description) ]
                --
                Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap }
                Children [ {designs} ]
            ]
        }
    });
    bsn! {
        @Section { @title: {info.name.clone()}, @buttons: buttons }
        Children [
            {progress}
            --
            {fixed}
        ]
    }
}

/// [`memo`] keys of node cards: this tag with the node id.
const MEMO_NODE_CARD: u64 = 0x7EC4 << 32;

fn progress_bar(fraction: f32, label: String) -> impl Scene {
    let ranges = vec![
        (UiColor::Good, 0.5, f32::INFINITY),
        (UiColor::Average, 0.25, 0.5),
        (UiColor::Bad, f32::NEG_INFINITY, 0.25),
    ];
    bsn! {
        Node { flex_grow: 1.0, flex_basis: px(0) }
        Children [
            @ProgressBar { @empty: true }
            ProgressBar { value: fraction, ranges: ranges }
            Children [ Text(label) ]
        ]
    }
}

/// The design icon's tooltip: name, machines, departments (when not all) and cost.
fn design_tooltip(st: &TechwebStatic, design: &DesignInfo) -> String {
    let machines: Vec<&str> = st
        .build_types
        .iter()
        .filter(|(b, _)| design.build_types & b != 0)
        .map(|(_, n)| n.as_str())
        .collect();
    let mut out = design.name.clone();
    if !machines.is_empty() {
        out.push_str(&format!("\n{}", machines.join(", ")));
    }
    if st
        .departments
        .iter()
        .any(|(b, _)| design.department_flags & b == 0)
    {
        let departments: Vec<&str> = st
            .departments
            .iter()
            .filter(|(b, _)| design.department_flags & b != 0)
            .map(|(_, n)| n.as_str())
            .collect();
        out.push_str(&format!("\n{}", departments.join(", ")));
    }
    let cost: Vec<String> = design
        .cost
        .iter()
        .map(|(m, n)| format!("{m}: {}", sheets_text(f64::from(*n) / st.sheet_amount)))
        .collect();
    if !cost.is_empty() {
        out.push_str(&format!("\n{}", cost.join(", ")));
    }
    out
}

pub(crate) fn sheets_text(sheets: f64) -> String {
    let rounded = (sheets * 100.0).round() / 100.0;
    format!("{rounded}")
}

/// `TechwebNodeDetail`: the node, what it needs and what it leads to.
fn node_detail(st: &TechwebStatic, data: &TechwebData, id: u16) -> impl Scene + use<> {
    let info = st.nodes.get(usize::from(id)).cloned().unwrap_or_default();
    let card = data
        .node(id)
        .map(|n| EntityScene(node_card(st, data, n, false)));
    let section = |title: &'static str, ids: &[u16]| {
        let cards: Vec<_> = ids
            .iter()
            .filter_map(|n| data.node(*n))
            .map(|n| node_card(st, data, n, true))
            .collect();
        (!cards.is_empty()).then(|| {
            EntityScene(bsn! {
                Node { flex_direction: FlexDirection::Column }
                Children [
                    Node { margin: UiRect::vertical(px(4)) }
                    Propagate::<Bold>(Bold(true))
                    Children [ Text(title) ]
                    --
                    {cards}
                ]
            })
        })
    };
    let required = section("Required", &info.prerequisites);
    let unlocks = section("Unlocks", &info.unlocked_nodes);
    bsn! {
        @Section { @fill: true, @scrollable: true }
        Children [
            @ss13_ui_kit::Button { @icon: "arrow-left", @content: "Back" }
            SetLocal { key: {DETAILS.to_owned()}, value: {String::new()}, rerender: true }
            --
            {card}
            --
            {required}
            --
            {unlocks}
        ]
    }
}

// --- Fabricator ----------------------------------------------------------------------------

impl ClientUi for FabricatorUi {
    const WIDTH: f32 = 800.0;
    const HEIGHT: f32 = 600.0;

    fn content(st: &FabricatorStatic, data: &FabricatorData) -> impl Scene {
        fabricator_view(st, data, &UiLocalState::default(), FABRICATOR_ACTS)
    }

    fn content_local(
        st: &FabricatorStatic,
        data: &FabricatorData,
        local: &UiLocalState,
    ) -> impl Scene {
        fabricator_view(st, data, local, FABRICATOR_ACTS)
    }
}

/// The acts a fabrication window sends: printing (with its amount parameter) and ejecting.
#[derive(Clone, Copy)]
pub(crate) struct FabActs {
    build: &'static str,
    amount: &'static str,
    remove: &'static str,
}

const FABRICATOR_ACTS: FabActs = FabActs {
    build: BUILD,
    amount: AMOUNT,
    remove: REMOVE_MAT,
};
const AUTOLATHE_ACTS: FabActs = FabActs {
    build: MAKE,
    amount: MULTIPLIER,
    remove: EJECT,
};

// --- Autolathe -----------------------------------------------------------------------------

impl ClientUi for AutolatheUi {
    const WIDTH: f32 = 800.0;
    const HEIGHT: f32 = 600.0;

    fn content(st: &AutolatheStatic, data: &AutolatheData) -> impl Scene {
        autolathe_view(st, data, &UiLocalState::default())
    }

    fn content_local(
        st: &AutolatheStatic,
        data: &AutolatheData,
        local: &UiLocalState,
    ) -> impl Scene {
        autolathe_view(st, data, local)
    }
}

/// `Autolathe.tsx` on the Fabricator layout: the designs at the current efficiency, the hacked
/// ones while hacked, and the storage total.
fn autolathe_view(
    st: &AutolatheStatic,
    data: &AutolatheData,
    local: &UiLocalState,
) -> impl Scene + use<> {
    let fab = st.priced(data.efficiency);
    let fdata = FabricatorData {
        materials: data.materials.clone(),
        on_hold: false,
        busy: data.active,
        material_maximum: data.materials_max,
        researched: st.shown(data.hacked),
    };
    let total = format!(
        "Всего материалов: {}/{} листов",
        sheets_text(f64::from(data.material_total) / fab.sheet_amount),
        sheets_text(f64::from(data.materials_max) / fab.sheet_amount),
    );
    bsn! {
        Node { flex_direction: FlexDirection::Column, width: percent(100), height: percent(100) }
        Children [
            Node { margin: UiRect::all(px(4)) } Children [ Text(total) Name("autolathe: total") ]
            --
            Node { flex_grow: 1.0, min_height: px(0) }
            Children [ {EntityScene(fabricator_view(&fab, &fdata, local, AUTOLATHE_ACTS))} ]
        ]
    }
}

/// The top-level categories of a design (`/Tools/Cleaning` -> `/Tools`).
pub(crate) fn top_categories(design: &FabDesign) -> Vec<String> {
    let mut out: Vec<String> = design
        .categories
        .iter()
        .filter(|c| !BLACKLISTED_CATEGORIES.contains(&c.as_str()))
        .filter_map(|c| {
            let first = c.trim_start_matches('/').split('/').next()?;
            (!first.is_empty()).then(|| format!("/{first}"))
        })
        .collect();
    if out.is_empty() {
        out.push(UNCATEGORIZED.to_owned());
    }
    out.sort();
    out.dedup();
    out
}

fn fabricator_view(
    st: &FabricatorStatic,
    data: &FabricatorData,
    local: &UiLocalState,
    acts: FabActs,
) -> impl Scene + use<> {
    let designs: Vec<&FabDesign> = data
        .researched
        .iter()
        .filter_map(|i| st.designs.get(usize::from(*i)))
        .collect();
    let mut categories: Vec<String> = designs.iter().flat_map(|d| top_categories(d)).collect();
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
            .filter(|d| top_categories(d).contains(c))
            .count();
        tabs.push(tab(c.clone(), count));
    }
    let mut shown: Vec<&FabDesign> = designs
        .iter()
        .copied()
        .filter(|d| selected == ALL_CATEGORY || top_categories(d).contains(&selected))
        .collect();
    shown.sort_by(|a, b| a.name.cmp(&b.name));
    let rows: Vec<_> = shown
        .into_iter()
        .map(|d| recipe(st, data, d, acts))
        .collect();
    let busy = data.busy.then(|| {
        EntityScene(bsn! {
            @Dimmer
            Children [
                Node Propagate::<FontPx>(FontPx(24.0))
                Children [ Text("Building items...") Name("fabricator: busy") ]
            ]
        })
    });
    let on_hold = data.on_hold.then(|| {
        EntityScene(bsn! {
            @Dimmer
            Children [
                Text("Доступ к минералам ограничен. Обратитесь к квартирмейстеру.")
            ]
        })
    });
    bsn! {
        @WindowContent
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
                            @Section { @fill: true, @scrollable: true, @title: {selected.trim_start_matches('/').to_owned()} }
                            Children [ {rows} ]
                            --
                            {busy}
                        ]
                    ]
                ]
                --
                @StackItem
                Children [ @Section Children [ {EntityScene(material_bar_with(st, data, acts.remove))} ] ]
            ]
            --
            {on_hold}
        ]
    }
}

fn cost_text(st: &FabricatorStatic, design: &FabDesign, amount: u32) -> String {
    let mut out = design.desc.clone();
    for (m, n) in &design.cost {
        let name = st
            .materials
            .get(usize::from(*m))
            .map_or("?", |m| m.name.as_str());
        let sheets = f64::from(n * amount) / st.sheet_amount;
        out.push_str(&format!("\n{name}: {}", sheets_text(sheets)));
    }
    out
}

/// `Recipe`: the design (prints one), `x5`, `x10` and the maximum.
fn recipe(
    st: &FabricatorStatic,
    data: &FabricatorData,
    design: &FabDesign,
    acts: FabActs,
) -> impl Scene + use<> {
    let can_print = data.can_print(design, 1);
    let build = |amount: u32| {
        ActParams(vec![
            (DESIGN_PATH.to_owned(), design.path.clone()),
            (acts.amount.to_owned(), amount.to_string()),
        ])
    };
    let max = data.max_prints(design);
    let print_button = |amount: u32, label: String| {
        let params = build(amount.max(1));
        let disabled = amount == 0 || !data.can_print(design, amount);
        EntityScene(bsn! {
            @ss13_ui_kit::Button { @content: label, @compact: true }
            ss13_ui_kit::Button { color: UiColor::Transparent, disabled: disabled }
            Act({acts.build.to_owned()})
            params
        })
    };
    let buttons: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
        Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center }
        Children [
            {print_button(5, "×5".to_owned())}
            --
            {print_button(10, "×10".to_owned())}
            --
            {print_button(max, format!("[Макс: {max}]"))}
        ]
    }));
    let image = sprite(design.icon);
    let name = design.name.clone();
    let tooltip = cost_text(st, design, 1);
    let params = build(1);
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
        Act({acts.build.to_owned()})
        params
        button_name
        Children [ Text(name) ]
    }
}

/// `MaterialAccessBar`: each material's sheets with eject buttons.
pub(crate) fn material_bar(st: &FabricatorStatic, data: &FabricatorData) -> impl Scene + use<> {
    material_bar_with(st, data, REMOVE_MAT)
}

fn material_bar_with(
    st: &FabricatorStatic,
    data: &FabricatorData,
    remove: &'static str,
) -> impl Scene + use<> {
    let docks: Vec<_> = st
        .materials
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let units = data.materials.get(i).copied().unwrap_or(0);
            let sheets = f64::from(units) / st.sheet_amount;
            let label = sheets_text(sheets);
            let image = sprite(m.icon);
            let tooltip = m.name.clone();
            // Not tg: the dock pops out on hover there; here its buttons show while there is a
            // sheet to eject.
            let eject: Vec<_> = [1_u32, 5, 10, 25, 50]
                .into_iter()
                .filter(|_| sheets >= 1.0)
                .map(|amount| {
                    let params = ActParams(vec![
                        (MATERIAL_REF.to_owned(), i.to_string()),
                        (AMOUNT.to_owned(), amount.to_string()),
                    ]);
                    let disabled = f64::from(amount) > sheets;
                    bsn! {
                        @ss13_ui_kit::Button { @content: {format!("×{amount}")}, @compact: true }
                        ss13_ui_kit::Button { color: UiColor::Transparent, disabled: disabled }
                        Act({remove.to_owned()})
                        params
                    }
                })
                .collect();
            let name = Name::new(format!("material: {}", m.name));
            bsn! {
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    flex_grow: 1.0,
                    min_width: px(60),
                }
                Tooltip { content: tooltip }
                Children [
                    Node { width: px(32), height: px(32) }
                    Children [ {image} ]
                    --
                    Node Children [ Text(label) name ]
                    --
                    Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, justify_content: JustifyContent::Center }
                    Children [ {eject} ]
                ]
            }
        })
        .collect();
    bsn! {
        Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap }
        Children [ {docks} ]
    }
}

// --- E.X.P.E.R.I-MENTOR --------------------------------------------------------------------

impl ClientUi for ExperimentorUi {
    const WIDTH: f32 = 400.0;
    const HEIGHT: f32 = 340.0;

    fn content(_: &(), data: &ExperimentorData) -> impl Scene {
        experimentor_view(data)
    }
}

/// `Experimentor.tsx`: the loaded item, its nodes and one button per scan.
fn experimentor_view(data: &ExperimentorData) -> impl Scene + use<> {
    let body: Box<dyn SceneList> = if !data.is_server_connected {
        Box::new(EntityScene(bsn! {
            @NoticeBox { @danger: true }
            Children [ Text("Not connected to a server.") ]
        }))
    } else if let Some(item) = data.loaded_item.clone() {
        let nodes: Vec<_> = item
            .associated_nodes
            .iter()
            .map(|(name, unlocked)| {
                let text = format!("{name}: {}", if *unlocked { "unlocked" } else { "hidden" });
                text_row(text)
            })
            .collect();
        let relic = if item.is_relic { "Strange object" } else { "" }.to_owned();
        let image = sprite(item.icon);
        let item_name = item.name.clone();
        let cooldown = data.is_on_cooldown;
        let buttons: Vec<_> = data
            .available_experiments
            .iter()
            .map(|e| {
                let params = ActParams(vec![(EXPERIMENTOR_ID.to_owned(), e.id.clone())]);
                let disabled = !e.is_available || cooldown;
                let name = Name::new(format!("experimentor: {}", e.id));
                let icon = e.fa_icon.clone();
                let label = e.name.clone();
                bsn! {
                    @ss13_ui_kit::Button { @icon: icon, @content: label }
                    ss13_ui_kit::Button { disabled: disabled }
                    Act({EXPERIMENTOR_EXPERIMENT.to_owned()})
                    params
                    name
                }
            })
            .collect();
        Box::new(EntityScene(bsn! {
            Node { flex_direction: FlexDirection::Column }
            Children [
                Node { flex_direction: FlexDirection::Row, margin: UiRect::bottom(px(6)) }
                Children [
                    Node { width: px(32), height: px(32) } Children [ {image} ]
                    --
                    Node Propagate::<Bold>(Bold(true)) Children [ Text(item_name) ]
                    --
                    @ss13_ui_kit::Button { @icon: "eject", @content: "Eject" }
                    Act({EXPERIMENTOR_EJECT.to_owned()})
                    Name("experimentor: eject")
                ]
                --
                {EntityScene(text_row(relic))}
                --
                {nodes}
                --
                Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap }
                Children [ {buttons} ]
            ]
        }))
    } else {
        Box::new(EntityScene(bsn! {
            @NoticeBox { @info: true }
            Children [ Text("No item loaded.") ]
        }))
    };
    bsn! { @WindowContent Children [ {body} ] }
}

// --- Destructive analyzer ------------------------------------------------------------------

impl ClientUi for DestructiveAnalyzerUi {
    const WIDTH: f32 = 400.0;
    const HEIGHT: f32 = 260.0;

    fn content(_: &(), data: &AnalyzerData) -> impl Scene {
        analyzer_view(data)
    }
}

fn analyzer_view(data: &AnalyzerData) -> impl Scene + use<> {
    let body: Box<dyn SceneList> = if !data.server_connected {
        Box::new(EntityScene(bsn! {
            @NoticeBox { @danger: true }
            Children [ Text("Not connected to a server. Please sync one using a multitool.") ]
        }))
    } else if let Some(item) = data.loaded_item.clone() {
        Box::new(EntityScene(analyzer_item(data, item)))
    } else {
        Box::new(EntityScene(bsn! {
            @NoticeBox { @danger: true }
            Children [ Text("No item loaded! Put any item inside to see what it's capable of!") ]
        }))
    };
    bsn! { @WindowContent Children [ {body} ] }
}

fn analyzer_item(data: &AnalyzerData, item: String) -> impl Scene + use<> {
    let image = sprite(data.item_icon);
    let eject: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
        @ss13_ui_kit::Button {
            @icon: "eject",
            @tooltip: "Ejects the item currently inside the machine.",
        }
        Act({EJECT_ITEM.to_owned()})
        Name("analyzer: eject")
    }));
    let methods: Box<dyn SceneList> = if data.indestructible {
        let points = (!data.recoverable_points.is_empty()).then(|| {
            let text = data.recoverable_points.replace("<BR>", "\n");
            EntityScene(bsn! {
                Node { flex_direction: FlexDirection::Column }
                Children [
                    Node Propagate::<FontPx>(FontPx(14.0)) Children [ Text("Research points from deconstruction") ]
                    --
                    {EntityScene(text_row(text))}
                ]
            })
        });
        let tooltip = if data.already_deconstructed {
            "This item item has already been deconstructed, and will not give any additional information."
        } else {
            "Destroys the object currently residing in the machine."
        };
        Box::new(EntityScene(bsn! {
            Node { flex_direction: FlexDirection::Column }
            Children [
                {points}
                --
                @ss13_ui_kit::Button { @icon: "hammer", @content: "Deconstruct", @tooltip: tooltip }
                Act({DECONSTRUCT.to_owned()})
                Name("analyzer: deconstruct")
            ]
        }))
    } else {
        Box::new(EntityScene(bsn! {
            @NoticeBox { @danger: true }
            Children [ Text("This item can't be deconstructed!") ]
        }))
    };
    let nodes: Vec<_> = data
        .node_data
        .iter()
        .map(|n| {
            let params = ActParams(vec![(DECONSTRUCT_PATH.to_owned(), n.node_path.clone())]);
            let tooltip = if n.node_hidden {
                "Deconstruct this to research the selected node."
            } else {
                "This node has already been researched."
            };
            bsn! {
                Node { margin: UiRect::top(px(4)) }
                Children [
                    @ss13_ui_kit::Button { @icon: "cash-register", @content: {n.node_name.clone()}, @tooltip: tooltip }
                    ss13_ui_kit::Button { disabled: {!n.node_hidden} }
                    Act({DECONSTRUCT.to_owned()})
                    params
                ]
            }
        })
        .collect();
    bsn! {
        Node { flex_direction: FlexDirection::Column }
        Children [
            @Section { @title: item, @buttons: eject }
            Children [ Node { width: px(64), height: px(64) } Children [ {image} ] ]
            --
            @Section { @title: "Deconstruction Methods" }
            Children [ {methods} -- {nodes} ]
        ]
    }
}

// --- Experiment configuration --------------------------------------------------------------

impl ClientUi for ExperimentConfigureUi {
    const WIDTH: f32 = 600.0;
    const HEIGHT: f32 = 735.0;

    fn content(_: &(), data: &ExperimentConfigureData) -> impl Scene {
        experiment_configure_view(data)
    }
}

/// `ExperimentConfigure`.
fn experiment_configure_view(data: &ExperimentConfigureData) -> impl Scene + use<> {
    let webs: Vec<_> = data
        .techwebs
        .iter()
        .map(|w| {
            let header = format!("{} / {}", w.web_id, w.web_org);
            let button: Box<dyn SceneList> = if w.selected {
                Box::new(EntityScene(bsn! {
                    @ss13_ui_kit::Button { @content: "Отключиться" }
                    ss13_ui_kit::Button { color: UiColor::Good }
                    Act({CLEAR_SERVER.to_owned()})
                    Name("experiments: disconnect")
                }))
            } else {
                let params = ActParams(vec![(EXPERIMENT_REF.to_owned(), w.web_id.clone())]);
                Box::new(EntityScene(bsn! {
                    @ss13_ui_kit::Button { @content: "Подключиться" }
                    Act({SELECT_SERVER.to_owned()})
                    params
                    Name("experiments: connect")
                }))
            };
            let servers: Vec<_> = w.all_servers.iter().map(|n| text_row(n.clone())).collect();
            let intro = "Соединение к этой сети поддерживают следующие сервера...".to_owned();
            bsn! {
                @Section { @title: header, @buttons: button }
                Children [
                    {EntityScene(text_row(intro))}
                    --
                    {servers}
                ]
            }
        })
        .collect();
    let servers_label = if data.techwebs.is_empty() {
        "Не найдены сервера, подключенные к техсети!"
    } else {
        "Пожалуйста, выберите техсеть для подключения..."
    };
    let experiments = data.techwebs.iter().any(|w| w.selected).then(|| {
        let mut list = data.experiments.clone();
        list.sort_by(|a, b| a.name.cmp(&b.name));
        let hint = if list.is_empty() {
            "Не найдены эксперименты в этой сети"
        } else if data.always_active {
            "Это устройство настроено для выполнения всех доступных экспериментов. Дальнейшие \
             настройки не нужны."
        } else {
            "Выберите один из следующих экспериментов..."
        };
        let cards: Vec<_> = list.iter().map(experiment_card).collect();
        EntityScene(bsn! {
            @Section { @title: "Эксперименты" }
            Children [
                {EntityScene(text_row(hint.to_owned()))}
                --
                {cards}
            ]
        })
    });
    let start = data.has_start_callback.then(|| {
        let disabled = !data.experiments.iter().any(|e| e.selected);
        EntityScene(bsn! {
            @ss13_ui_kit::Button { @icon: "flask", @content: "Выполнить эксперимент", @fluid: true }
            ss13_ui_kit::Button { disabled: disabled }
            Act({START_EXPERIMENT.to_owned()})
            Name("experiments: start")
        })
    });
    bsn! {
        @WindowContent
        Children [
            @Section { @title: "Сервера", @fill: true, @scrollable: true }
            Children [
                {EntityScene(text_row(servers_label.to_owned()))}
                --
                {webs}
                --
                {experiments}
                --
                {start}
            ]
        ]
    }
}

/// `Experiment` with its `ExperimentStageRow`s.
fn experiment_card(exp: &ExperimentInfo) -> impl Scene + use<> {
    let title = format!("{} — {}", exp.name, exp.tag);
    let (act, params) = if exp.selected {
        (CLEAR_EXPERIMENT, ActParams(vec![]))
    } else {
        (
            SELECT_EXPERIMENT,
            ActParams(vec![(EXPERIMENT_REF.to_owned(), exp.path.clone())]),
        )
    };
    let selected = exp.selected;
    let tooltip = exp.performance_hint.clone();
    let name = Name::new(format!("experiment: {}", exp.path));
    let stages: Vec<_> = exp
        .progress
        .iter()
        .map(|stage| {
            let marker = match stage {
                ProgressStage::Bool { done: true, .. } => "✓".to_owned(),
                ProgressStage::Bool { done: false, .. } => "✗".to_owned(),
                ProgressStage::Integer { value, total, .. } => format!("{value}/{total}"),
                ProgressStage::Detail { .. } => "⤷".to_owned(),
            };
            let color = if stage.complete() {
                UiColor::Good
            } else {
                UiColor::Bad
            };
            let color = color.text().unwrap_or_default();
            let text = stage.text().to_owned();
            bsn! {
                Node { flex_direction: FlexDirection::Row, margin: UiRect::left(px(12)) }
                Children [
                    Node { margin: UiRect::right(px(6)) }
                    Children [ Text(marker) TextColor(color) ]
                    --
                    Text(text)
                ]
            }
        })
        .collect();
    let description = exp.description.clone();
    bsn! {
        Node { flex_direction: FlexDirection::Column, margin: UiRect::bottom(px(8)) }
        Children [
            @ss13_ui_kit::Button { @content: title, @fluid: true, @tooltip: tooltip }
            ss13_ui_kit::Button { selected: selected }
            Act({act.to_owned()})
            params
            name
            --
            {EntityScene(text_row(description))}
            --
            {stages}
        ]
    }
}

// --- Server control ------------------------------------------------------------------------

impl ClientUi for ServerControlUi {
    const WIDTH: f32 = 575.0;
    const HEIGHT: f32 = 400.0;

    fn content(_: &(), data: &ServerControlData) -> impl Scene {
        server_control_view(data)
    }
}

/// `ServerControl`.
fn server_control_view(data: &ServerControlData) -> impl Scene + use<> {
    let body: Box<dyn SceneList> = if data.server_connected {
        Box::new(EntityScene(server_control_content(data)))
    } else {
        Box::new(EntityScene(bsn! {
            @NoticeBox { @danger: true }
            Children [ Text("Not connected to a Server. Please sync one using a multitool.") ]
        }))
    };
    bsn! { @WindowContent Children [ {body} ] }
}

fn server_control_content(data: &ServerControlData) -> impl Scene + use<> {
    let servers: Box<dyn SceneList> = if data.servers.is_empty() {
        Box::new(EntityScene(bsn! {
            @NoticeBox { @info: true } Children [ Text("No servers found.") ]
        }))
    } else {
        let rows: Vec<_> = data
            .servers
            .iter()
            .enumerate()
            .map(|(i, server)| {
                let (content, color) = if server.server_disabled {
                    ("Offline", UiColor::Bad)
                } else {
                    ("Online", UiColor::Good)
                };
                let params = ActParams(vec![(SELECTED_SERVER.to_owned(), i.to_string())]);
                let label = server.server_name.clone();
                let tooltip = server.server_details.clone();
                bsn! {
                    Node { flex_direction: FlexDirection::Column, margin: UiRect::bottom(px(4)) }
                    Children [
                        Text(label)
                        --
                        @ss13_ui_kit::Button { @content: content, @fluid: true, @tooltip: tooltip }
                        ss13_ui_kit::Button { color: color }
                        Act({LOCKDOWN_SERVER.to_owned()})
                        params
                    ]
                }
            })
            .collect();
        Box::new(EntityScene(bsn! {
            @Section { @title: "Research Servers" } Children [ {rows} ]
        }))
    };
    let consoles: Box<dyn SceneList> = if data.consoles.is_empty() {
        Box::new(EntityScene(bsn! {
            @NoticeBox { @info: true } Children [ Text("No consoles found.") ]
        }))
    } else {
        let rows: Vec<_> = data
            .consoles
            .iter()
            .enumerate()
            .map(|(i, console)| {
                let (content, color) = if console.console_locked {
                    ("LOCKED", UiColor::Bad)
                } else {
                    ("UNLOCKED", UiColor::Good)
                };
                let params = ActParams(vec![(SELECTED_CONSOLE.to_owned(), i.to_string())]);
                let label = format!(
                    "{} - Location: {}",
                    console.console_name, console.console_location
                );
                bsn! {
                    Node { flex_direction: FlexDirection::Column, margin: UiRect::bottom(px(4)) }
                    Children [
                        Text(label)
                        --
                        @ss13_ui_kit::Button { @content: content, @fluid: true }
                        ss13_ui_kit::Button { color: color }
                        Act({LOCK_CONSOLE.to_owned()})
                        params
                    ]
                }
            })
            .collect();
        Box::new(EntityScene(bsn! {
            @Section { @title: "Research Consoles" } Children [ {rows} ]
        }))
    };
    let logs: Box<dyn SceneList> = if data.logs.is_empty() {
        Box::new(EntityScene(bsn! {
            @NoticeBox { @info: true } Children [ Text("No history found.") ]
        }))
    } else {
        let rows: Vec<_> = data
            .logs
            .iter()
            .map(|l| {
                text_row(format!(
                    "{} — {} — {} — {}",
                    l.node_name,
                    points_text(l.node_cost),
                    l.node_researcher,
                    l.node_research_location
                ))
            })
            .collect();
        Box::new(EntityScene(
            bsn! { Node { flex_direction: FlexDirection::Column } Children [ {rows} ] },
        ))
    };
    bsn! {
        @Section { @fill: true, @scrollable: true }
        Children [
            {servers}
            --
            {consoles}
            --
            @Section { @title: "Research History" }
            Children [ {logs} ]
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categories_and_numbers() {
        let design = FabDesign {
            categories: vec!["initial".into(), "/Tools/Cleaning".into(), "/Tools".into()],
            ..default()
        };
        assert_eq!(top_categories(&design), vec!["/Tools".to_owned()]);
        assert_eq!(
            top_categories(&FabDesign::default()),
            vec![UNCATEGORIZED.to_owned()]
        );
        assert_eq!(points_text(12.0), "12");
        assert_eq!(points_text(2.5), "2.5");
        assert_eq!(sheets_text(0.25), "0.25");
    }
}
