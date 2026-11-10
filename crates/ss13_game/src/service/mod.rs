//! Service jobs' items (tg `code/modules/paperwork`, `code/modules/library`,
//! `code/game/objects/items/dice.dm`, `clown_items.dm`, `datums/components/squeak.dm`):
//! paper with text and stamps, pens, paper bins, the hand labeler, the desk bell, books and
//! bookcases, the bible, dice, squeaky things, the mime's vow and the chaplain's religion;
//! clipboards, folders, paper planes, the photocopier and the fax (`office`), the library's
//! archive, consoles, scanner and binder (`archive`), easels and canvases (`art`), cameras,
//! photos and picture frames (`photo`).
//!
//! This module holds what both halves share: the tgui `PaperSheet` and `MarkdownViewer` window
//! data, the photo window's picture. [`server`] runs the items, [`client`] draws the windows.

use serde::{Deserialize, Serialize};
use ss13_tgui::UiInterface;

#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "server")]
pub mod server;

#[cfg(feature = "server")]
mod archive;
#[cfg(feature = "server")]
mod art;
#[cfg(feature = "server")]
pub mod barber;
#[cfg(feature = "server")]
mod barcode;
#[cfg(feature = "server")]
mod burden;
#[cfg(feature = "server")]
pub mod cards;
#[cfg(feature = "server")]
mod clown;
#[cfg(feature = "server")]
mod dream_sect;
#[cfg(feature = "server")]
mod festival;
#[cfg(feature = "server")]
mod games;
#[cfg(feature = "server")]
mod library;
#[cfg(feature = "server")]
mod library_console;
#[cfg(feature = "server")]
mod manual_data;
#[cfg(feature = "server")]
mod manuals;
#[cfg(feature = "server")]
mod office;
#[cfg(feature = "server")]
mod paper;
#[cfg(feature = "server")]
mod photo;
#[cfg(feature = "server")]
mod religion;
#[cfg(feature = "server")]
pub mod slots;
#[cfg(feature = "server")]
mod sparring;
#[cfg(feature = "server")]
mod stationery;
#[cfg(feature = "server")]
mod statues;

#[cfg(feature = "server")]
pub use archive::{ArchivedBook, BookBinder, BookScanner, LibraryArchive};
#[cfg(feature = "server")]
pub use art::Canvas;
#[cfg(feature = "server")]
pub use barber::{BarberJob, complete as barber_complete};
#[cfg(feature = "server")]
pub use barcode::{BarcodeScanner, Ledger, Loan, ScanMode};
#[cfg(feature = "server")]
pub use burden::{Burdened, Psyker};
#[cfg(feature = "server")]
pub use clown::MimeVow;
#[cfg(feature = "server")]
pub use dream_sect::{DreamProtection, SlumberParty, dream_pool, dream_started};
#[cfg(feature = "server")]
pub use festival::{PortableAltar, SmoothTunes, SongTune};
#[cfg(feature = "server")]
pub use games::Die;
#[cfg(feature = "server")]
pub use library::Book;
#[cfg(feature = "server")]
pub use library_console::{ConsoleAct as LibraryConsoleAct, emag_act as library_emag_act};
#[cfg(feature = "server")]
pub use office::{Clipboard, Fax, Photocopier, Toner};
#[cfg(feature = "server")]
pub use paper::{Paper, PaperBin, write_printed};
#[cfg(feature = "server")]
pub use photo::{
    Camera, Photo, PhotoJob, Picture, PictureFrame, capture, complete as photo_complete,
    examined as photo_examined, frame_moved as picture_frame_moved, portrait, show as photo_show,
};
#[cfg(feature = "server")]
pub use religion::{
    BIBLE_SKINS, BibleDeity, HolyRole, PerformingRite, Religion, ReligionJob, Rite, Sect,
    antimagic_flags as religion_antimagic, chaplain_spawned, complete as religion_complete,
    holy_role,
};
#[cfg(feature = "server")]
pub use sparring::{SparMatch, Sparring, Unbreakable};
#[cfg(feature = "server")]
pub use stationery::RapsheetCount;
#[cfg(feature = "server")]
pub use stationery::{Biscuit, CabinetKind, CarbonPaper, FilingCabinet, PaperCutter, Paperwork};
#[cfg(feature = "server")]
pub use statues::{
    ArtJob, CarvingBlock, Chisel, CombustibleFlooder, HippocraticBust, HippocraticOath,
    Radioactive, complete as art_complete, fire_act as flooder_fire_act,
    interrupted as art_interrupted, sworn as hippocratic_sworn,
};

/// tgui `PaperSheet` (`/obj/item/paper/ui_interact`), read-only here: writing goes through a
/// text prompt (see [`server`]).
pub struct PaperSheetUi;

impl UiInterface for PaperSheetUi {
    const NAME: &'static str = "PaperSheet";
    type Data = ();
    type StaticData = PaperSheetData;
}

/// One `/datum/paper_input` (`to_list`: `raw_text`, `font`, `color`, `bold`, `children`).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct PaperInput {
    pub raw_text: String,
    pub font: String,
    /// `#rrggbb`.
    pub color: String,
    pub bold: bool,
    /// What was written into the input fields, each at its `[child_N]` key (1-based) in
    /// `raw_text`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<PaperInput>,
}

/// tg's `[input_field]` key (`\[input_field(?:\]|\s+autofill_type=\w+\])`).
const FIELD_KEY: &str = "[input_field";
const CHILD_KEY: &str = "[child_";

impl PaperInput {
    /// `get_raw_text`: the text with the children in their keys.
    pub fn full_text(&self) -> String {
        if self.children.is_empty() {
            return self.raw_text.clone();
        }
        let mut out = String::with_capacity(self.raw_text.len());
        let mut rest = self.raw_text.as_str();
        while let Some(at) = rest.find(CHILD_KEY) {
            out.push_str(&rest[..at]);
            let tail = &rest[at + CHILD_KEY.len()..];
            let digits = tail
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(tail.len());
            let child = tail[..digits]
                .parse::<usize>()
                .ok()
                .filter(|_| tail[digits..].starts_with(']'))
                .and_then(|n| n.checked_sub(1))
                .and_then(|i| self.children.get(i));
            match child {
                Some(child) => {
                    out.push_str(&child.full_text());
                    rest = &tail[digits + 1..];
                }
                None => {
                    out.push_str(CHILD_KEY);
                    rest = tail;
                }
            }
        }
        out.push_str(rest);
        out
    }

    /// `get_raw_text_length`.
    pub fn text_length(&self) -> usize {
        self.full_text().chars().count()
    }

    /// The byte ranges of the empty input fields of `raw_text`, in order (field id = index + 1).
    pub fn fields(&self) -> Vec<std::ops::Range<usize>> {
        input_fields(&self.raw_text)
    }

    /// `add_child_to_field`: `child` into field `id` (1-based). False without that field.
    pub fn fill_field(&mut self, id: usize, child: PaperInput) -> bool {
        let Some(range) = id
            .checked_sub(1)
            .and_then(|i| self.fields().get(i).cloned())
        else {
            return false;
        };
        self.children.push(child);
        let key = format!("{CHILD_KEY}{}]", self.children.len());
        self.raw_text.replace_range(range, &key);
        true
    }

    /// What the window shows: the children in place, empty fields as a blank line.
    pub fn display_text(&self) -> String {
        let text = self.full_text();
        let mut out = String::with_capacity(text.len());
        let mut last = 0;
        for range in input_fields(&text) {
            out.push_str(&text[last..range.start]);
            out.push_str("__________");
            last = range.end;
        }
        out.push_str(&text[last..]);
        out
    }
}

/// The `[input_field]` / `[input_field autofill_type=word]` keys of `text`.
pub fn input_fields(text: &str) -> Vec<std::ops::Range<usize>> {
    const AUTOFILL: &str = "autofill_type=";
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(at) = text[from..].find(FIELD_KEY) {
        let start = from + at;
        let after = start + FIELD_KEY.len();
        let rest = &text[after..];
        let len = if rest.starts_with(']') {
            Some(1)
        } else {
            let trimmed = rest.trim_start();
            let spaces = rest.len() - trimmed.len();
            trimmed.strip_prefix(AUTOFILL).and_then(|t| {
                let word = t
                    .find(|c: char| !(c.is_alphanumeric() || c == '_'))
                    .unwrap_or(t.len());
                (spaces > 0 && word > 0 && t[word..].starts_with(']'))
                    .then_some(spaces + AUTOFILL.len() + word + 1)
            })
        };
        match len {
            Some(len) => {
                out.push(start..after + len);
                from = after + len;
            }
            None => from = after,
        }
    }
    out
}

/// `/obj/item/paper/convert_to_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct PaperSheetData {
    pub paper_name: String,
    pub raw_text_input: Vec<PaperInput>,
    /// The stamps' names, in stamping order (tg sends their class and position).
    pub raw_stamp_input: Vec<String>,
}

/// The photo window (tg `/obj/item/photo/show`, a browser window with the picture). The
/// server keeps no pixels: a picture is the sprites the camera saw, which the client draws.
pub struct PhotoUi;

impl UiInterface for PhotoUi {
    const NAME: &'static str = "Photo";
    type Data = ();
    type StaticData = PhotoData;
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct PhotoData {
    pub name: String,
    /// Tiles.
    pub width: u8,
    pub height: u8,
    /// Back to front.
    pub sprites: Vec<PhotoSprite>,
    /// "Written on the back".
    pub scribble: String,
}

/// One sprite of a picture: an atom's base sprite or one of its overlays as it was drawn.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct PhotoSprite {
    pub icon_state: ss13_shared::IconStateId,
    pub dir: ss13_shared::Dir,
    pub color: Option<[u8; 4]>,
    /// Pixels from the picture's bottom-left corner to the sprite's, +y up.
    pub x: i16,
    pub y: i16,
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

/// tgui `DSMBook` (`/obj/item/tgui_book/manual/dsm/ui_static_data`).
pub struct DsmBookUi;

impl UiInterface for DsmBookUi {
    const NAME: &'static str = "DSMBook";
    type Data = ();
    type StaticData = DsmBookData;
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct DsmBookData {
    pub traumas: Vec<DsmTrauma>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct DsmTrauma {
    pub full_name: String,
    pub scan_name: String,
    pub desc: String,
    pub symptoms: String,
    pub id: String,
}

/// tgui `IDCBook` (`/obj/item/tgui_book/manual/idc/ui_static_data`).
pub struct IdcBookUi;

impl UiInterface for IdcBookUi {
    const NAME: &'static str = "IDCBook";
    type Data = ();
    type StaticData = IdcBookData;
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct IdcBookData {
    pub diseases: Vec<IdcDisease>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct IdcDisease {
    pub form: String,
    pub agent: String,
    pub name: String,
    pub desc: String,
    pub illness: String,
    pub spread_by: String,
    pub cured_by: String,
    pub id: String,
    pub level: String,
}

/// tgui `LibraryConsole` (`/obj/machinery/computer/libraryconsole/bookmanagement`). Every
/// list is sent whole (tg pages them).
pub struct LibraryConsoleUi;

impl UiInterface for LibraryConsoleUi {
    const NAME: &'static str = "LibraryConsole";
    type Data = LibraryConsoleData;
    type StaticData = ();
}

/// The console's screens (`LIBRARY_INVENTORY`..`LIBRARY_TOP_SNEAKY`).
pub const LIBRARY_INVENTORY: u8 = 1;
pub const LIBRARY_CHECKOUT: u8 = 2;
pub const LIBRARY_ARCHIVE: u8 = 3;
pub const LIBRARY_UPLOAD: u8 = 4;
pub const LIBRARY_PRINT: u8 = 5;
pub const LIBRARY_TOP_SNEAKY: u8 = 6;

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct LibraryConsoleData {
    pub screen_state: u8,
    /// Emagged and the forbidden lore not taken yet.
    pub display_lore: bool,
    pub inventory: Vec<LibraryBookRow>,
    pub checkouts: Vec<LibraryLoanRow>,
    pub checkout_title: Option<String>,
    /// The archive (tg's search results).
    pub pages: Vec<LibraryArchiveRow>,
    pub upload_categories: Vec<String>,
    pub has_scanner: bool,
    pub has_cache: bool,
    pub cache_title: String,
    pub cache_author: String,
    pub deity: String,
    pub religion: String,
    pub bible_name: String,
}

/// An inventory row; `id` is its 1-based place in the list.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct LibraryBookRow {
    pub id: u32,
    pub title: String,
    pub author: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct LibraryLoanRow {
    pub id: u32,
    pub borrower: String,
    pub overdue: bool,
    pub due_in_minutes: f32,
    pub title: String,
    pub author: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct LibraryArchiveRow {
    pub id: u32,
    pub title: String,
    pub author: String,
    pub category: String,
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
            "How to play\n\n• Roll\n• Win & gloat"
        );
        assert_eq!(plain_text("# Title\n\nText"), "# Title\n\nText");
        assert_eq!(plain_text("a < b"), "a < b");
    }

    fn line(text: &str) -> PaperInput {
        PaperInput {
            raw_text: text.to_owned(),
            ..Default::default()
        }
    }

    #[test]
    fn input_fields_fill_in_order() {
        let mut form =
            line("Name: [input_field] Job: [input_field autofill_type=job] [input_field x]");
        assert_eq!(form.fields().len(), 2, "a malformed key is not a field");
        assert!(form.fill_field(2, line("Clown")));
        assert_eq!(form.fields().len(), 1);
        assert!(form.fill_field(1, line("Bob")));
        assert!(!form.fill_field(1, line("again")), "no field left");
        assert_eq!(form.full_text(), "Name: Bob Job: Clown [input_field x]");
        assert_eq!(form.text_length(), form.full_text().chars().count());
        let blank = line("Sign: [input_field]");
        assert_eq!(blank.display_text(), "Sign: __________");
    }
}
