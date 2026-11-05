//! Forensics after tg `code/modules/forensics` (`/datum/forensics`: fingerprints, hidden prints,
//! blood DNA, fibers) and `code/modules/detectivework` (the detective's forensic scanner, its scan
//! log and printed report, evidence bags).
//!
//! [`server`] keeps the traces on atoms and runs the scanner and the bags, [`client`] draws the
//! scanner's tgui `ForensicScanner` window. This module holds what both halves share: the
//! window's data and acts.

use serde::{Deserialize, Serialize};
use ss13_tgui::UiInterface;

#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "server")]
pub mod server;

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

/// DM `capitalize`: the first letter upper case.
pub fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}
