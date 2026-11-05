//! The forensic scanner window (client): tgui `ForensicScanner/index.tsx` and
//! `ForensicLogs.tsx`. The data is `crate::forensics`, the server half
//! `crate::forensics::server`.
// shortcut: no category icons (tgui draws a FontAwesome icon per line), "Clear logs" sends at once
// (tgui `Button.Confirm` asks first).

use bevy::scene::{EntityScene, SceneList};
use ss13_tgui::client::{AppTguiClientExt, ClientUi};
use ss13_ui_kit::prelude::*;

use super::{
    CLEAR, DELETE, ForensicScannerData, ForensicScannerUi, INDEX, PRINT, ScanCategory, ScanLog,
    capitalize,
};

pub struct ForensicsClientPlugin;

impl Plugin for ForensicsClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_view::<ForensicScannerUi>();
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
