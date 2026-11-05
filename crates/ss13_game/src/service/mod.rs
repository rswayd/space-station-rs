//! Service jobs' items (tg `code/modules/paperwork`, `code/modules/library`,
//! `code/game/objects/items/dice.dm`, `clown_items.dm`, `datums/components/squeak.dm`):
//! paper with text and stamps, pens, paper bins, the hand labeler, the desk bell, books and
//! bookcases, the bible, dice, squeaky things and the mime's vow.
//!
//! This module holds what both halves share: the tgui `PaperSheet` and `MarkdownViewer` window
//! data. [`server`] runs the items, [`client`] draws the windows.

use serde::{Deserialize, Serialize};
use ss13_tgui::UiInterface;

#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "server")]
pub mod server;

#[cfg(feature = "server")]
mod clown;
#[cfg(feature = "server")]
mod games;
#[cfg(feature = "server")]
mod library;
#[cfg(feature = "server")]
mod paper;

#[cfg(feature = "server")]
pub use clown::MimeVow;
#[cfg(feature = "server")]
pub use games::Die;
#[cfg(feature = "server")]
pub use paper::{Paper, write_printed};

/// tgui `PaperSheet` (`/obj/item/paper/ui_interact`), read-only here: writing goes through a
/// text prompt (see [`server`]).
pub struct PaperSheetUi;

impl UiInterface for PaperSheetUi {
    const NAME: &'static str = "PaperSheet";
    type Data = ();
    type StaticData = PaperSheetData;
}

/// One `/datum/paper_input` (`to_list`: `raw_text`, `font`, `color`, `bold`).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct PaperInput {
    pub raw_text: String,
    pub font: String,
    /// `#rrggbb`.
    pub color: String,
    pub bold: bool,
}

/// `/obj/item/paper/convert_to_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct PaperSheetData {
    pub paper_name: String,
    pub raw_text_input: Vec<PaperInput>,
    /// The stamps' names, in stamping order (tg sends their class and position).
    pub raw_stamp_input: Vec<String>,
}

/// tgui `MarkdownViewer` (`/obj/item/book/ui_static_data`).
pub struct MarkdownViewerUi;

impl UiInterface for MarkdownViewerUi {
    const NAME: &'static str = "MarkdownViewer";
    type Data = ();
    type StaticData = MarkdownViewerData;
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct MarkdownViewerData {
    pub title: String,
    pub author: String,
    pub content: String,
}

/// The text of raw paper/book HTML as plain lines: `<br>`, `<p>` and list items break lines,
/// other tags go, the common entities are decoded.
// shortcut: no markdown or HTML rendering (tgui `marked` + DOMPurify); upgrade with a rich text
// widget in `ss13_ui_kit`.
pub fn plain_text(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(start) = rest.find('<') {
        out.push_str(&rest[..start]);
        let Some(len) = rest[start..].find('>') else {
            out.push_str(&rest[start..]);
            rest = "";
            break;
        };
        let tag = rest[start + 1..start + len]
            .trim()
            .trim_start_matches('/')
            .split(|c: char| c.is_whitespace() || c == '/')
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        match tag.as_str() {
            "br" | "p" | "ul" | "ol" | "hr" | "h1" | "h2" | "h3" | "h4" | "div" => out.push('\n'),
            "li" if !rest[start + 1..].starts_with('/') => out.push_str("\n• "),
            _ => {}
        }
        rest = &rest[start + len + 1..];
    }
    out.push_str(rest);
    let out = out
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&amp;", "&");
    // Runs of breaks from nested tags collapse to one blank line.
    let mut lines: Vec<&str> = Vec::new();
    for line in out.lines() {
        let line = line.trim_end();
        if line.trim().is_empty() && lines.last().is_none_or(|l| l.trim().is_empty()) {
            continue;
        }
        lines.push(line);
    }
    while lines.last().is_some_and(|l| l.trim().is_empty()) {
        lines.pop();
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_breaks_lines_and_drops_tags() {
        assert_eq!(
            plain_text("How to play<br><ul><li>Roll</li><li>Win &amp; <b>gloat</b></li></ul>"),
            "How to play\n• Roll\n• Win & gloat"
        );
        assert_eq!(plain_text("# Title\n\nText"), "# Title\n\nText");
        assert_eq!(plain_text("a < b"), "a < b");
    }
}
