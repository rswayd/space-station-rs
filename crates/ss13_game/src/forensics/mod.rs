//! Forensics after tg `code/modules/forensics` (`/datum/forensics`: fingerprints, hidden prints,
//! blood DNA, fibers) and `code/modules/detectivework` (the detective's forensic scanner, its scan
//! log and printed report, evidence bags).
//!
//! [`server`] keeps the traces on atoms and runs the scanner and the bags, [`spoofer`] the
//! syndicate forensics spoofer (`/obj/item/forensics_spoofer`), [`board`] the detective's notice
//! board (`/obj/structure/detectiveboard`); [`client`] draws their tgui windows
//! (`ForensicScanner`, `ForensicsSpoofer`, `DetectiveBoard`). This module holds what both halves
//! share: the windows' data and acts.

use serde::{Deserialize, Serialize};
use ss13_tgui::UiInterface;

#[cfg(feature = "server")]
pub mod board;
#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "server")]
pub mod server;
#[cfg(feature = "server")]
pub mod spoofer;

pub struct ForensicScannerUi;

impl UiInterface for ForensicScannerUi {
    const NAME: &'static str = "ForensicScanner";
    type Data = ForensicScannerData;
    // tg sends `categories` as static data; both halves share [`ScanCategory`] instead.
    type StaticData = ();
}

/// `/datum/detective_scan_category` subtypes the ported scan fills, in `display_order`.
// Settings, holy data, illegal tech and notes come from `COMSIG_DETECTIVE_SCANNED` handlers of
// items that are not ported; add their categories with them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ScanCategory {
    Fingers,
    Blood,
    Fiber,
    Reagents,
    Access,
}

impl ScanCategory {
    /// `name`: the window's and the report's label.
    pub fn name(self) -> &'static str {
        match self {
            Self::Fingers => "Fingerprints",
            Self::Blood => "Blood DNA, Type",
            Self::Fiber => "Fibers",
            Self::Reagents => "Reagents",
            Self::Access => "ID Access",
        }
    }

    /// `format_report_entry` of the category.
    pub fn report_entry(self, key: &str, value: &str) -> String {
        match self {
            Self::Blood => format!("{key}, {value}"),
            Self::Reagents => format!("{key}: {value} u."),
            Self::Access => format!("{key}: {value}"),
            Self::Fingers | Self::Fiber => key.to_owned(),
        }
    }
}

/// `/datum/detective_scanner_data_entry/ui_data`: tg's assoc `data` as key, value pairs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanEntry {
    pub category: ScanCategory,
    pub data: Vec<(String, String)>,
}

/// `/datum/detective_scanner_log/ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanLog {
    pub scan_target: String,
    pub scan_time: String,
    /// Sorted by category (`sort_data_entries`).
    pub data_entries: Vec<ScanEntry>,
}

impl ScanLog {
    /// `add_data_entry`: merges into the category's entry; a key already there keeps its place
    /// and takes the new value (DM assoc lists).
    pub fn add(
        &mut self,
        category: ScanCategory,
        data: impl IntoIterator<Item = (String, String)>,
    ) {
        let index = match self
            .data_entries
            .iter()
            .position(|e| e.category == category)
        {
            Some(i) => i,
            None => {
                self.data_entries.push(ScanEntry {
                    category,
                    data: Vec::new(),
                });
                self.data_entries.sort_by_key(|e| e.category);
                self.data_entries
                    .iter()
                    .position(|e| e.category == category)
                    .expect("just pushed")
            }
        };
        let entry = &mut self.data_entries[index];
        for (key, value) in data {
            match entry.data.iter_mut().find(|(k, _)| *k == key) {
                Some(slot) => slot.1 = value,
                None => entry.data.push((key, value)),
            }
        }
    }

    /// `generate_report_text`, as plain lines.
    pub fn report_text(&self) -> String {
        let mut text = format!(
            "{} scan at {}\n",
            capitalize(&self.scan_target),
            self.scan_time
        );
        if self.data_entries.is_empty() {
            text.push_str("No forensic traces found.\n");
        }
        for entry in &self.data_entries {
            let category = entry.category;
            text.push_str(category.name());
            text.push('\n');
            for (key, value) in &entry.data {
                text.push_str("  ");
                text.push_str(&category.report_entry(key, value));
                text.push('\n');
            }
        }
        text.push_str("---\n");
        text
    }
}

/// `/obj/item/detective_scanner/ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForensicScannerData {
    pub logs: Vec<ScanLog>,
}

/// `ui_act` actions; `delete` takes the 0-based log `index`.
pub const CLEAR: &str = "clear";
pub const DELETE: &str = "delete";
pub const PRINT: &str = "print";
pub const INDEX: &str = "index";

pub use ss13_shared::capitalize;

// ---------------------------------------------------------------------------------------------
// The forensics spoofer (`ForensicsSpoofer.tsx`).

pub struct ForensicsSpooferUi;

impl UiInterface for ForensicsSpooferUi {
    const NAME: &'static str = "ForensicsSpoofer";
    type Data = SpooferData;
    type StaticData = SpooferStatic;
}

/// `/obj/item/forensics_spoofer/ui_static_data`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpooferStatic {
    pub max_storage: u32,
}

/// `/obj/item/forensics_spoofer/ui_data`: tg's assoc `fingerprints` as (print, owner's name).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpooferData {
    pub scanmode: bool,
    pub silent: bool,
    pub fibers: Vec<String>,
    pub fingerprints: Vec<(String, String)>,
    pub chosen_fiber: Option<String>,
    pub chosen_fingerprint: Option<String>,
}

/// `ui_act` actions; `choose` and `delete` take the print or fiber as `chosen`.
pub const SPOOF_SCANMODE: &str = "scanmode";
pub const SPOOF_STEALTH: &str = "stealth";
pub const SPOOF_CHOOSE: &str = "choose";
pub const SPOOF_DELETE: &str = "delete";
pub const SPOOF_CHOSEN: &str = "chosen";

// ---------------------------------------------------------------------------------------------
// The detective board (`DetectiveBoard/`).

pub struct DetectiveBoardUi;

impl UiInterface for DetectiveBoardUi {
    const NAME: &'static str = "DetectiveBoard";
    type Data = DetectiveBoardData;
    type StaticData = ();
}

/// `/obj/structure/detectiveboard/ui_data`; `current_case` is 0-based (tg's is 1-based).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetectiveBoardData {
    pub cases: Vec<BoardCase>,
    pub current_case: usize,
}

/// `/datum/case`: `ref` is its id on the board.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoardCase {
    pub id: u32,
    pub name: String,
    pub color: String,
    pub evidences: Vec<BoardEvidence>,
}

/// `/datum/evidence`: a pinned photo or paper; `connections` are evidence ids of its case.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoardEvidence {
    pub id: u32,
    pub name: String,
    pub description: String,
    pub photo: bool,
    /// A paper's first text input (`raw_text_inputs[1].raw_text`).
    pub text: String,
    pub connections: Vec<u32>,
}

/// `ui_act` actions and their params (ids as numbers).
pub const BOARD_ADD_CASE: &str = "add_case";
pub const BOARD_SET_CASE: &str = "set_case";
pub const BOARD_REMOVE_CASE: &str = "remove_case";
pub const BOARD_RENAME_CASE: &str = "rename_case";
pub const BOARD_LOOK: &str = "look_evidence";
pub const BOARD_REMOVE_EVIDENCE: &str = "remove_evidence";
pub const BOARD_CONNECT: &str = "add_connection";
pub const BOARD_CASE: &str = "case";
pub const BOARD_CASE_REF: &str = "case_ref";
pub const BOARD_EVIDENCE_REF: &str = "evidence_ref";
pub const BOARD_FROM_REF: &str = "from_ref";
pub const BOARD_TO_REF: &str = "to_ref";
