//! The editing model is Bevy's `EditableText` (parley `PlainEditor`). These tests pin the
//! behaviour we rely on for Russian players: grapheme-aware deletion, word jumps over Cyrillic,
//! selection, and max length counted in chars.

use std::borrow::Cow;

use bevy::clipboard::Clipboard;
use bevy::prelude::*;
use bevy::text::{DEFAULT_FONT_DATA, EditableText, FontCx, LayoutCx, TextEdit};
use parley::FontFamilyName;

use super::text::single_line_char;

struct Harness {
    fonts: FontCx,
    layout: LayoutCx,
    // Never used for copy/paste here: with `system_clipboard` that would touch the OS clipboard.
    clipboard: Clipboard,
}

impl Harness {
    fn new() -> Self {
        let mut fonts = FontCx::default();
        let font = Font::from_bytes(DEFAULT_FONT_DATA.to_vec());
        fonts.collection.register_fonts(font.data, None);
        Self {
            fonts,
            layout: LayoutCx::default(),
            clipboard: Clipboard::default(),
        }
    }

    fn field(&mut self, text: &str) -> EditableText {
        let mut field = EditableText::new(text);
        field
            .editor
            .edit_styles()
            .insert(FontFamilyName::Named(Cow::Borrowed("Fira Mono")).into());
        self.apply(&mut field, []);
        field
    }

    fn apply(&mut self, field: &mut EditableText, edits: impl IntoIterator<Item = TextEdit>) {
        self.apply_filtered(field, edits, |_| true);
    }

    fn apply_filtered(
        &mut self,
        field: &mut EditableText,
        edits: impl IntoIterator<Item = TextEdit>,
        filter: impl Fn(char) -> bool,
    ) {
        field.pending_edits.extend(edits);
        field.apply_pending_edits(
            &mut self.fonts,
            &mut self.layout.0,
            &mut self.clipboard,
            filter,
        );
    }
}

fn value(field: &EditableText) -> String {
    field.value().to_string()
}

fn selected(field: &EditableText) -> Option<String> {
    field.editor().selected_text().map(str::to_owned)
}

#[test]
fn cyrillic_insert_and_delete_by_character() {
    let mut h = Harness::new();
    let mut f = h.field("Привет");
    h.apply(&mut f, [TextEdit::Backspace]);
    assert_eq!(value(&f), "Приве");
    h.apply(
        &mut f,
        [
            TextEdit::Left(false),
            TextEdit::Left(false),
            TextEdit::Insert("ё".into()),
        ],
    );
    assert_eq!(value(&f), "Приёве");
    h.apply(&mut f, [TextEdit::Delete]);
    assert_eq!(value(&f), "Приёе");
    h.apply(&mut f, [TextEdit::TextStart(false), TextEdit::Delete]);
    assert_eq!(value(&f), "риёе");
}

#[test]
fn cursor_moves_by_whole_clusters_over_emoji() {
    let mut h = Harness::new();
    // Thumbs up + skin tone is 2 scalars / 8 bytes. Cursor stops come from parley's shaping
    // clusters, so they are font-dependent: a decomposed "е\u{308}" or a ZWJ family emoji is one
    // stop only if the font ligates it (the embedded test font does not). Keyboards and IMEs
    // produce precomposed "ё", so this does not affect Russian input.
    let mut f = h.field("а👍🏽б😀вёг");
    // From the end: Left over "г", over "ё"; Delete removes "ё".
    h.apply(
        &mut f,
        [
            TextEdit::Left(false),
            TextEdit::Left(false),
            TextEdit::Delete,
        ],
    );
    assert_eq!(value(&f), "а👍🏽б😀вг");
    // Two more Lefts land before the 4-byte emoji; forward Delete removes all of it.
    h.apply(
        &mut f,
        [
            TextEdit::Left(false),
            TextEdit::Left(false),
            TextEdit::Delete,
        ],
    );
    assert_eq!(value(&f), "а👍🏽бвг");
    // Shift+Left from before "б" moves by whole scalars, never into a UTF-8 sequence; with the
    // test font the skin-tone modifier is its own stop.
    h.apply(&mut f, [TextEdit::Left(false), TextEdit::Left(true)]);
    assert_eq!(selected(&f).as_deref(), Some("🏽"));
    h.apply(&mut f, [TextEdit::Left(true)]);
    assert_eq!(selected(&f).as_deref(), Some("👍🏽"));
    h.apply(&mut f, [TextEdit::Backspace]);
    assert_eq!(value(&f), "абвг");
}

#[test]
fn shift_selection_replace_and_select_all() {
    let mut h = Harness::new();
    let mut f = h.field("Привет мир");
    h.apply(
        &mut f,
        [
            TextEdit::Left(true),
            TextEdit::Left(true),
            TextEdit::Left(true),
        ],
    );
    assert_eq!(selected(&f).as_deref(), Some("мир"));
    h.apply(&mut f, [TextEdit::Insert("всем".into())]);
    assert_eq!(value(&f), "Привет всем");
    h.apply(&mut f, [TextEdit::SelectAll, TextEdit::Insert("Ок".into())]);
    assert_eq!(value(&f), "Ок");
    assert_eq!(selected(&f), None);
}

#[test]
fn word_jumps_and_word_deletes_over_cyrillic() {
    let mut h = Harness::new();
    let mut f = h.field("съешь ещё этих булок");
    h.apply(
        &mut f,
        [TextEdit::TextStart(false), TextEdit::WordRight(true)],
    );
    assert_eq!(selected(&f).as_deref().map(str::trim_end), Some("съешь"));
    h.apply(&mut f, [TextEdit::TextEnd(false), TextEdit::WordLeft(true)]);
    assert_eq!(selected(&f).as_deref(), Some("булок"));
    h.apply(&mut f, [TextEdit::TextEnd(false), TextEdit::BackspaceWord]);
    assert_eq!(value(&f), "съешь ещё этих ");
    h.apply(&mut f, [TextEdit::TextStart(false), TextEdit::DeleteWord]);
    assert_eq!(value(&f).trim_start(), "ещё этих ");
}

#[test]
fn max_length_counts_chars_not_bytes() {
    let mut h = Harness::new();
    let mut f = h.field("абв");
    f.max_characters = Some(5);
    // 3 + 3 > 5: the whole insert is rejected (Bevy does not truncate).
    h.apply(&mut f, [TextEdit::Insert("где".into())]);
    assert_eq!(value(&f), "абв");
    // 5 chars = 10 bytes of UTF-8: allowed.
    h.apply(&mut f, [TextEdit::Insert("гд".into())]);
    assert_eq!(value(&f), "абвгд");
    h.apply(&mut f, [TextEdit::Insert("е".into())]);
    assert_eq!(value(&f), "абвгд");
    // Replacing a selection counts the removed chars first.
    h.apply(&mut f, [TextEdit::Left(true), TextEdit::Insert("ё".into())]);
    assert_eq!(value(&f), "абвгё");
}

#[test]
fn single_line_filter_rejects_newlines() {
    let mut h = Harness::new();
    let mut f = h.field("а");
    h.apply_filtered(&mut f, [TextEdit::Insert("б\nв".into())], single_line_char);
    assert_eq!(value(&f), "а");
    h.apply_filtered(&mut f, [TextEdit::Insert("бв".into())], single_line_char);
    assert_eq!(value(&f), "абв");
}

#[test]
fn ime_preedit_is_shown_but_not_part_of_value() {
    let mut h = Harness::new();
    let mut f = h.field("а");
    h.apply(
        &mut f,
        [TextEdit::ImeSetCompose {
            value: "日本".into(),
            cursor: None,
        }],
    );
    assert!(f.is_composing());
    assert_eq!(value(&f), "а");
    assert_eq!(f.editor().raw_text(), "а日本");
    h.apply(
        &mut f,
        [TextEdit::ImeCommit {
            value: "日本".into(),
        }],
    );
    assert!(!f.is_composing());
    assert_eq!(value(&f), "а日本");
}
