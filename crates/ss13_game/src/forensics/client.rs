//! The forensics windows (client): tgui `ForensicScanner/index.tsx` and `ForensicLogs.tsx`,
//! `ForensicsSpoofer.tsx`, `DetectiveBoard/`. The data is `crate::forensics`, the server halves
//! `crate::forensics::server`, `spoofer` and `board`.
// shortcut: no category icons (tgui draws a FontAwesome icon per line), "Clear logs" sends at once
// (tgui `Button.Confirm` asks first). The detective board is a list of evidence cards with
// their strings named, not tgui's canvas of dragged pins and drawn strings; a paper's text shows
// as plain text, a photo as its "Look" button.

use bevy::scene::{EntityScene, SceneList};
use ss13_tgui::client::{AppTguiClientExt, ClientUi, SetLocal, UiLocalState};
use ss13_ui_kit::prelude::*;

use super::{
    BOARD_ADD_CASE, BOARD_CASE, BOARD_CASE_REF, BOARD_CONNECT, BOARD_EVIDENCE_REF, BOARD_FROM_REF,
    BOARD_LOOK, BOARD_REMOVE_CASE, BOARD_REMOVE_EVIDENCE, BOARD_RENAME_CASE, BOARD_SET_CASE,
    BOARD_TO_REF, BoardCase, BoardEvidence, CLEAR, DELETE, DetectiveBoardData, DetectiveBoardUi,
    ForensicScannerData, ForensicScannerUi, ForensicsSpooferUi, INDEX, PRINT, SPOOF_CHOOSE,
    SPOOF_CHOSEN, SPOOF_DELETE, SPOOF_SCANMODE, SPOOF_STEALTH, ScanCategory, ScanLog, SpooferData,
    SpooferStatic, capitalize,
};

pub struct ForensicsClientPlugin;

impl Plugin for ForensicsClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_view::<ForensicScannerUi>()
            .register_ui_view::<ForensicsSpooferUi>()
            .register_ui_view::<DetectiveBoardUi>();
    }
}

// ---------------------------------------------------------------------------------------------
// The forensics spoofer.

/// `useState` key of the spoofer's tab: `"1"` fibers, otherwise prints.
const SPOOFER_TAB: &str = "tab";

impl ClientUi for ForensicsSpooferUi {
    const WIDTH: f32 = 460.0;
    const HEIGHT: f32 = 340.0;

    fn content(st: &SpooferStatic, data: &SpooferData) -> impl Scene {
        spoofer_view(st, data, &UiLocalState::default())
    }

    fn content_local(st: &SpooferStatic, data: &SpooferData, local: &UiLocalState) -> impl Scene {
        spoofer_view(st, data, local)
    }
}

fn spoofer_view(
    st: &SpooferStatic,
    data: &SpooferData,
    local: &UiLocalState,
) -> impl Scene + use<> {
    let fibers_tab = local.get(SPOOFER_TAB) == "1";
    let (silent_icon, silent_text) = if data.silent {
        ("eye-slash", "Silent Mode: On")
    } else {
        ("eye", "Silent Mode: Off")
    };
    let (mode_icon, mode_text, mode_color) = if data.scanmode {
        ("magnifying-glass", "Mode: Scan", UiColor::Blue)
    } else {
        ("share-from-square", "Mode: Apply", UiColor::Red)
    };
    let tab = |index: &'static str, icon: &'static str, label: String, selected: bool| {
        EntityScene(bsn! {
            @TabsTab { @icon: icon, @content: label }
            TabsTab { selected: selected }
            SetLocal { key: {SPOOFER_TAB.to_owned()}, value: {index.to_owned()}, rerender: true }
        })
    };
    let max = st.max_storage;
    let prints_tab = tab(
        "0",
        "fingerprint",
        format!("Fingerprints {}/{max}", data.fingerprints.len()),
        !fibers_tab,
    );
    let fiber_tab = tab(
        "1",
        "shirt",
        format!("Fibers {}/{max}", data.fibers.len()),
        fibers_tab,
    );
    let entries: Vec<(String, Option<String>)> = if fibers_tab {
        data.fibers.iter().map(|f| (f.clone(), None)).collect()
    } else {
        data.fingerprints
            .iter()
            .map(|(p, n)| (p.clone(), Some(n.clone())))
            .collect()
    };
    let chosen = if fibers_tab {
        data.chosen_fiber.clone()
    } else {
        data.chosen_fingerprint.clone()
    };
    let rows: Vec<_> = entries
        .into_iter()
        .map(|(entry, owner)| spoofer_row(entry, owner, chosen.as_deref()))
        .collect();
    bsn! {
        @WindowContent
        Children [
            @Section
            Children [
                Node { flex_direction: FlexDirection::Row }
                Children [
                    Node { width: percent(50) }
                    Children [
                        @ss13_ui_kit::Button { @icon: silent_icon, @content: silent_text, @fluid: true,
                            @tooltip: "On Silent Mode this device will make the same sounds and sights as an actual Forensics Scanner." }
                        Act({SPOOF_STEALTH.to_owned()})
                    ]
                    --
                    Node { width: percent(50) }
                    Children [
                        @ss13_ui_kit::Button { @icon: mode_icon, @content: mode_text, @fluid: true }
                        ss13_ui_kit::Button { color: mode_color }
                        Act({SPOOF_SCANMODE.to_owned()})
                    ]
                ]
            ]
            --
            @Section { @fill: true, @scrollable: true }
            Children [
                @Tabs { @fluid: true }
                Children [ {prints_tab} -- {fiber_tab} ]
                --
                Node { flex_direction: FlexDirection::Column }
                Children [ {rows} ]
            ]
        ]
    }
}

/// One stored print or fiber: choose, delete, the entry (a print cut to 25 characters) and its
/// owner.
fn spoofer_row(entry: String, owner: Option<String>, chosen: Option<&str>) -> impl Scene + use<> {
    let selected = chosen == Some(entry.as_str());
    let check_icon = if selected {
        "check-square-o"
    } else {
        "square-o"
    };
    let params = ActParams(vec![(SPOOF_CHOSEN.to_owned(), entry.clone())]);
    let delete_params = params.clone();
    let shown = if owner.is_some() {
        entry.chars().take(25).collect()
    } else {
        entry
    };
    let owner = owner.map(|o| format!("({o})")).unwrap_or_default();
    bsn! {
        Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center }
        Children [
            @ss13_ui_kit::Button { @icon: check_icon }
            ss13_ui_kit::Button { selected: selected }
            Act({SPOOF_CHOOSE.to_owned()})
            params
            --
            @ss13_ui_kit::Button { @icon: "trash" }
            ss13_ui_kit::Button { color: UiColor::Red }
            Act({SPOOF_DELETE.to_owned()})
            delete_params
            --
            Node { flex_grow: 1.0 }
            Children [ Text(shown) ]
            --
            Node
            Children [ Text(owner) ]
        ]
    }
}

// ---------------------------------------------------------------------------------------------
// The detective board.

/// `useState` key: the evidence a string starts from.
const LINK_FROM: &str = "link_from";

impl ClientUi for DetectiveBoardUi {
    const WIDTH: f32 = 700.0;
    const HEIGHT: f32 = 600.0;

    fn content(_: &(), data: &DetectiveBoardData) -> impl Scene {
        board_view(data, &UiLocalState::default())
    }

    fn content_local(_: &(), data: &DetectiveBoardData, local: &UiLocalState) -> impl Scene {
        board_view(data, local)
    }
}

fn board_view(data: &DetectiveBoardData, local: &UiLocalState) -> impl Scene + use<> {
    let body: Box<dyn SceneList> = match data.cases.get(data.current_case) {
        Some(case) => Box::new(EntityScene(case_view(data, case, local))),
        None => Box::new(EntityScene(bsn! {
            Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center }
            Children [
                @NoticeBox
                Children [ Text("You have no cases! Create the first one") ]
                --
                @ss13_ui_kit::Button { @icon: "plus", @content: "Create case" }
                Act({BOARD_ADD_CASE.to_owned()})
            ]
        })),
    };
    let body = vec![body];
    bsn! {
        @WindowContent
        Children [ {body} ]
    }
}

/// `BoardTabs` and the current case's evidence.
fn case_view(
    data: &DetectiveBoardData,
    case: &BoardCase,
    local: &UiLocalState,
) -> impl Scene + use<> {
    let mut tabs: Vec<Box<dyn SceneList>> = data
        .cases
        .iter()
        .enumerate()
        .map(|(index, c)| {
            let params = ActParams(vec![(BOARD_CASE.to_owned(), index.to_string())]);
            let selected = index == data.current_case;
            let label = c.name.clone();
            let color = case_color(&c.color);
            Box::new(EntityScene(bsn! {
                @TabsTab { @content: label }
                TabsTab { selected: selected, color: color }
                Act({BOARD_SET_CASE.to_owned()})
                params
            })) as Box<dyn SceneList>
        })
        .collect();
    tabs.push(Box::new(EntityScene(bsn! {
        @TabsTab { @icon: "plus" }
        Act({BOARD_ADD_CASE.to_owned()})
    })));
    let from: Option<u32> = local.get(LINK_FROM).parse().ok();
    let case_params = ActParams(vec![(BOARD_CASE_REF.to_owned(), case.id.to_string())]);
    let rename_params = case_params.clone();
    let buttons: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
        Node
        Children [
            @ss13_ui_kit::Button { @icon: "pen", @tooltip: "Rename" }
            Act({BOARD_RENAME_CASE.to_owned()})
            rename_params
            --
            @ss13_ui_kit::Button { @icon: "times", @tooltip: "Remove case" }
            ss13_ui_kit::Button { color: UiColor::Bad }
            Act({BOARD_REMOVE_CASE.to_owned()})
            case_params
        ]
    }));
    let cards: Vec<_> = case
        .evidences
        .iter()
        .map(|e| evidence_card(case, e, from))
        .collect();
    let title = case.name.clone();
    bsn! {
        Node { flex_direction: FlexDirection::Column, flex_grow: 1.0 }
        Children [
            @Tabs
            Children [ {tabs} ]
            --
            @Section { @title: title, @buttons: buttons, @fill: true, @scrollable: true }
            Children [ {cards} ]
        ]
    }
}

/// The case tab colours (`BoardTab__<color>`).
fn case_color(color: &str) -> UiColor {
    match color {
        "red" => UiColor::Red,
        "orange" => UiColor::Orange,
        "yellow" => UiColor::Yellow,
        "green" => UiColor::Green,
        "blue" => UiColor::Blue,
        "violet" => UiColor::Violet,
        _ => UiColor::Default,
    }
}

/// `Evidence.tsx`: name, look, take down, its strings; a string starts here or ends here.
fn evidence_card(case: &BoardCase, e: &BoardEvidence, from: Option<u32>) -> impl Scene + use<> {
    let look = ActParams(vec![
        (BOARD_CASE_REF.to_owned(), case.id.to_string()),
        (BOARD_EVIDENCE_REF.to_owned(), e.id.to_string()),
    ]);
    let remove = ActParams(vec![(BOARD_EVIDENCE_REF.to_owned(), e.id.to_string())]);
    let link: Box<dyn SceneList> = match from {
        Some(start) if start == e.id => Box::new(EntityScene(bsn! {
            @ss13_ui_kit::Button { @icon: "times", @content: "Cancel" }
            SetLocal { key: {LINK_FROM.to_owned()}, value: {String::new()}, rerender: true }
        })),
        Some(start) => {
            let params = ActParams(vec![
                (BOARD_FROM_REF.to_owned(), start.to_string()),
                (BOARD_TO_REF.to_owned(), e.id.to_string()),
            ]);
            Box::new(EntityScene(bsn! {
                @ss13_ui_kit::Button { @icon: "link", @content: "Connect here" }
                ss13_ui_kit::Button { color: UiColor::Red }
                Act({BOARD_CONNECT.to_owned()})
                params
            }))
        }
        None => Box::new(EntityScene(bsn! {
            @ss13_ui_kit::Button { @icon: "link", @content: "Connect" }
            SetLocal { key: {LINK_FROM.to_owned()}, value: {e.id.to_string()}, rerender: true }
        })),
    };
    let look_label = if e.photo { "Look at photo" } else { "Read" };
    let buttons: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
        Node
        Children [
            @ss13_ui_kit::Button { @icon: "eye", @content: look_label }
            Act({BOARD_LOOK.to_owned()})
            look
            --
            {link}
            --
            @ss13_ui_kit::Button { @icon: "trash" }
            ss13_ui_kit::Button { color: UiColor::Bad }
            Act({BOARD_REMOVE_EVIDENCE.to_owned()})
            remove
        ]
    }));
    let linked: Vec<String> = e
        .connections
        .iter()
        .filter_map(|id| case.evidences.iter().find(|x| x.id == *id))
        .map(|x| x.name.clone())
        .collect();
    let strings = if linked.is_empty() {
        String::new()
    } else {
        format!("Connected: {}", linked.join(", "))
    };
    let (title, description, text) = (e.name.clone(), e.description.clone(), e.text.clone());
    bsn! {
        @Section { @title: title, @buttons: buttons }
        Children [
            Node { flex_direction: FlexDirection::Column }
            Children [
                Text(description)
                --
                Text(text)
                --
                Text(strings)
            ]
        ]
    }
}

impl ClientUi for ForensicScannerUi {
    const WIDTH: f32 = 512.0;
    const HEIGHT: f32 = 512.0;

    fn content(_: &(), data: &ForensicScannerData) -> impl Scene {
        view(data)
    }
}

fn view(data: &ForensicScannerData) -> impl Scene + use<> {
    let empty = data.logs.is_empty().then(|| {
        EntityScene(bsn! {
            @NoticeBox
            Children [ Text({"Log empty.".to_owned()}) ]
        })
    });
    let scans = (!data.logs.is_empty()).then(|| EntityScene(history(data)));
    bsn! {
        @WindowContent
        Children [
            {empty}
            --
            {scans}
        ]
    }
}

fn history(data: &ForensicScannerData) -> impl Scene + use<> {
    let (clear, print) = (clear_button(), print_button());
    let buttons: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
        Node
        Children [ {EntityScene(clear)} -- {EntityScene(print)} ]
    }));
    // Newest first, as tgui's `.reverse()`.
    let logs: Vec<_> = data
        .logs
        .iter()
        .enumerate()
        .rev()
        .map(|(index, log)| log_section(index, log))
        .collect();
    bsn! {
        @Section { @title: "Scan history", @fill: true, @scrollable: true, @buttons: buttons }
        Children [ {logs} ]
    }
}

fn clear_button() -> impl Scene + use<> {
    bsn! {
        @ss13_ui_kit::Button { @icon: "trash", @content: "Clear logs" }
        ss13_ui_kit::Button { color: UiColor::Bad }
        Act({CLEAR.to_owned()})
    }
}

fn print_button() -> impl Scene + use<> {
    bsn! {
        @ss13_ui_kit::Button { @icon: "print", @content: "Print report" }
        Act({PRINT.to_owned()})
    }
}

/// `ForensicLogs`: one scan.
fn log_section(index: usize, log: &ScanLog) -> impl Scene + use<> {
    let params = ActParams(vec![(INDEX.to_owned(), index.to_string())]);
    let delete: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
        @ss13_ui_kit::Button { @icon: "trash" }
        Act({DELETE.to_owned()})
        params
    }));
    let title = format!("{} scan at {}", capitalize(&log.scan_target), log.scan_time);
    let nothing = log.data_entries.is_empty().then(|| {
        EntityScene(bsn! {
            Node Children [ Text({"No forensic traces found.".to_owned()}) ]
        })
    });
    let entries: Vec<_> = log
        .data_entries
        .iter()
        .map(|entry| {
            let category = entry.category;
            let lines: Vec<_> = entry
                .data
                .iter()
                .map(|(key, value)| {
                    let line = line_text(category, key, value);
                    bsn! { Node Children [ Text(line) ] }
                })
                .collect();
            let label = category.name().to_owned();
            bsn! {
                @LabeledListItem { @label: label }
                Children [
                    Node { flex_direction: FlexDirection::Column }
                    Children [ {lines} ]
                ]
            }
        })
        .collect();
    let list = (!log.data_entries.is_empty()).then(|| {
        EntityScene(bsn! {
            @LabeledList
            Children [ {entries} ]
        })
    });
    bsn! {
        @Section { @title: title, @buttons: delete }
        Children [
            {nothing}
            --
            {list}
        ]
    }
}

/// The log formatters: prints and blood upper case, reagents in units, access per region.
fn line_text(category: ScanCategory, key: &str, value: &str) -> String {
    match category {
        ScanCategory::Fingers => value.to_uppercase(),
        ScanCategory::Blood => format!("{key}, {value}").to_uppercase(),
        ScanCategory::Reagents => format!("{key}: {value} u."),
        ScanCategory::Access => format!("{key}: {value}"),
        ScanCategory::Fiber => value.to_owned(),
    }
}
