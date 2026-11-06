//! The paper and book windows (client): tgui `PaperSheet` and `MarkdownViewer`. The data is
//! `crate::service`, the server half `crate::service::server`.
// shortcut: plain text instead of rendered markdown/HTML and fonts (each input keeps its colour
// and boldness on a white sheet); stamps are listed by name under the text, not drawn.

use bevy::app::Propagate;
use bevy::scene::{EntityScene, SceneList};
use ss13_tgui::client::{ActOnSubmit, AppTguiClientExt, ClientUi};
use ss13_ui_kit::ActParams;
use ss13_ui_kit::prelude::*;
use ss13_ui_kit::theme::Bold;

use super::{
    DsmBookData, DsmBookUi, IdcBookData, IdcBookUi, LIBRARY_ARCHIVE, LIBRARY_CHECKOUT,
    LIBRARY_INVENTORY, LIBRARY_PRINT, LIBRARY_TOP_SNEAKY, LIBRARY_UPLOAD, LibraryConsoleData,
    LibraryConsoleUi, MarkdownViewerData, MarkdownViewerUi, PaperSheetData, PaperSheetUi,
    plain_text,
};

pub struct ServiceClientPlugin;

impl Plugin for ServiceClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_view::<PaperSheetUi>()
            .register_ui_view::<MarkdownViewerUi>()
            .register_ui_view::<DsmBookUi>()
            .register_ui_view::<IdcBookUi>()
            .register_ui_view::<LibraryConsoleUi>();
    }
}

/// `#rrggbb` (tg pen colours), black when malformed.
fn parse_color(hex: &str) -> Color {
    let hex = hex.trim_start_matches('#');
    let byte = |i: usize| {
        hex.get(i..i + 2)
            .and_then(|b| u8::from_str_radix(b, 16).ok())
            .unwrap_or(0)
    };
    if hex.len() < 6 {
        return Color::BLACK;
    }
    Color::srgb_u8(byte(0), byte(2), byte(4))
}

impl ClientUi for PaperSheetUi {
    const WIDTH: f32 = 400.0;
    const HEIGHT: f32 = 500.0;

    fn content(data: &PaperSheetData, _: &()) -> impl Scene {
        paper_view(data)
    }
}

fn paper_view(data: &PaperSheetData) -> impl Scene + use<> {
    let inputs: Vec<_> = data
        .raw_text_input
        .iter()
        .map(|input| {
            let (text, color, bold) = (
                plain_text(&input.raw_text),
                parse_color(&input.color),
                input.bold,
            );
            bsn! {
                Node { margin: UiRect::bottom(px(6)) }
                Propagate::<TextColor>(TextColor(color))
                Propagate::<Bold>(Bold(bold))
                Children [ Text(text) ]
            }
        })
        .collect();
    let stamps: Vec<_> = data
        .raw_stamp_input
        .iter()
        .map(|stamp| {
            let text = format!("[{stamp}]");
            bsn! {
                Node
                Propagate::<TextColor>(TextColor(Color::srgb(0.6, 0.1, 0.1)))
                Propagate::<Bold>(Bold(true))
                Children [ Text(text) ]
            }
        })
        .collect();
    bsn! {
        @WindowContent
        Children [
            @Section { @fill: true, @scrollable: true }
            Children [
                Node {
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::all(px(8)),
                    min_height: percent(100),
                }
                BackgroundColor(Color::WHITE)
                Children [ {inputs} -- {stamps} ]
            ]
        ]
    }
}

impl ClientUi for MarkdownViewerUi {
    const WIDTH: f32 = 600.0;
    const HEIGHT: f32 = 800.0;

    fn content(data: &MarkdownViewerData, _: &()) -> impl Scene {
        book_view(data)
    }
}

fn book_view(data: &MarkdownViewerData) -> impl Scene + use<> {
    let title = if data.author.is_empty() {
        data.title.clone()
    } else {
        format!("{} — {}", data.title, data.author)
    };
    let content = plain_text(&data.content);
    bsn! {
        @WindowContent
        Children [
            @Section { @title: title, @fill: true, @scrollable: true }
            Children [ Node Children [ Text(content) ] ]
        ]
    }
}

// shortcut: the manuals are one scrolling list sorted by name, not tgui's paginated
// `BookUI` with a table of contents and page-flip acts; upgrade with a paginated book widget.
impl ClientUi for DsmBookUi {
    const WIDTH: f32 = 600.0;
    const HEIGHT: f32 = 800.0;

    fn content(data: &DsmBookData, _: &()) -> impl Scene {
        let mut traumas = data.traumas.clone();
        traumas.sort_by(|a, b| a.full_name.cmp(&b.full_name));
        let entries: Vec<_> = traumas
            .into_iter()
            .map(|t| {
                // `checkIdenticalTraumaNames`.
                let title = if t.full_name.to_lowercase() == t.scan_name.to_lowercase() {
                    t.full_name
                } else {
                    format!("{} (aka \"{}\")", t.full_name, t.scan_name)
                };
                let lines = vec![
                    format!("Description: {}", t.desc),
                    format!("Diagnosis: {}", t.symptoms),
                ];
                manual_entry(title, lines)
            })
            .collect();
        let title = "SDSM-35".to_owned();
        bsn! {
            @WindowContent
            Children [
                @Section { @title: title, @fill: true, @scrollable: true }
                Children [
                    Node { flex_direction: FlexDirection::Column }
                    Children [ {entries} ]
                ]
            ]
        }
    }
}

impl ClientUi for IdcBookUi {
    const WIDTH: f32 = 600.0;
    const HEIGHT: f32 = 800.0;

    fn content(data: &IdcBookData, _: &()) -> impl Scene {
        let mut diseases = data.diseases.clone();
        diseases.sort_by(|a, b| a.name.cmp(&b.name));
        let entries: Vec<_> = diseases
            .into_iter()
            .map(|d| {
                let mut lines = vec![format!("{} — {}", d.form, d.agent)];
                if d.form == "Symptom" {
                    lines.push(format!("Illness: {} (level {})", d.illness, d.level));
                } else {
                    lines.push(format!("Spread by: {}", d.spread_by));
                }
                lines.push(format!("Description: {}", d.desc));
                lines.push(format!("Cured by: {}", d.cured_by));
                manual_entry(d.name, lines)
            })
            .collect();
        let title = "IDC-27".to_owned();
        bsn! {
            @WindowContent
            Children [
                @Section { @title: title, @fill: true, @scrollable: true }
                Children [
                    Node { flex_direction: FlexDirection::Column }
                    Children [ {entries} ]
                ]
            ]
        }
    }
}

fn manual_entry(title: String, lines: Vec<String>) -> impl Scene + use<> {
    let lines: Vec<_> = lines
        .into_iter()
        .map(|line| bsn! { Node { margin: UiRect::bottom(px(4)) } Children [ Text(line) ] })
        .collect();
    bsn! {
        @Section { @title: title }
        Children [
            Node { flex_direction: FlexDirection::Column, padding: UiRect::all(px(4)) }
            Children [ {lines} ]
        ]
    }
}

type Part = Box<dyn SceneList>;

/// A button sending `action` with `params`.
fn act_button(
    content: String,
    action: &str,
    params: Vec<(String, String)>,
    selected: bool,
) -> Part {
    let (action, params) = (action.to_owned(), ActParams(params));
    Box::new(EntityScene(bsn! {
        @ss13_ui_kit::Button { @content: content }
        ss13_ui_kit::Button { selected: selected }
        Act(action)
        params
    }))
}

fn line(text: String) -> Part {
    Box::new(EntityScene(
        bsn! { Node { margin: UiRect::bottom(px(4)) } Children [ Text(text) ] },
    ))
}

/// A text field: Enter sends `action` with the text as `param` (and `params`).
fn field(action: &str, param: &str, params: Vec<(String, String)>) -> Part {
    let (action, param) = (action.to_owned(), param.to_owned());
    Box::new(EntityScene(bsn! {
        Input { value: {String::new()}, max_length: {Some(256)} }
        ActOnSubmit { action: action, param: param, params: params }
    }))
}

/// A row: `text` and its buttons.
fn row(text: String, buttons: Vec<Part>) -> Part {
    let mut children: Vec<Part> = vec![Box::new(EntityScene(bsn! {
        Node { margin: UiRect::right(px(6)) }
        Children [ Text(text) ]
    }))];
    children.extend(buttons);
    Box::new(EntityScene(bsn! {
        Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, align_items: AlignItems::Center, margin: UiRect::bottom(px(2)) }
        Children [ {children} ]
    }))
}

fn param(key: &str, value: impl ToString) -> Vec<(String, String)> {
    vec![(key.to_owned(), value.to_string())]
}

// shortcut: tgui's `LibraryConsole` screens as plain rows of text, fields and buttons (no
// pages, no bible sprite, no dropdown); the checkout time is a fixed 15 minutes field default.
impl ClientUi for LibraryConsoleUi {
    const WIDTH: f32 = 600.0;
    const HEIGHT: f32 = 600.0;

    fn content(_: &(), data: &LibraryConsoleData) -> impl Scene {
        library_console_view(data)
    }
}

fn library_console_view(data: &LibraryConsoleData) -> impl Scene + use<> {
    let mut screens = vec![
        (LIBRARY_INVENTORY, "Inventory"),
        (LIBRARY_CHECKOUT, "Checkout"),
        (LIBRARY_ARCHIVE, "Archive"),
        (LIBRARY_UPLOAD, "Upload"),
        (LIBRARY_PRINT, "Print"),
    ];
    if data.display_lore {
        screens.push((LIBRARY_TOP_SNEAKY, "Forbidden Lore"));
    }
    let tabs: Vec<Part> = screens
        .into_iter()
        .map(|(n, label)| {
            act_button(
                label.to_owned(),
                "set_screen",
                param("screen_index", n),
                data.screen_state == n,
            )
        })
        .collect();
    let body: Vec<Part> = match data.screen_state {
        LIBRARY_CHECKOUT => {
            let mut out = vec![line(format!(
                "Checking out: {}",
                data.checkout_title.as_deref().unwrap_or("nothing")
            ))];
            out.extend(data.inventory.iter().map(|b| {
                row(
                    format!("{} by {}", b.title, b.author),
                    vec![act_button(
                        "Select".into(),
                        "set_checkout",
                        param("book_name", &b.title),
                        false,
                    )],
                )
            }));
            out.push(line("Lend to (Enter):".into()));
            out.push(field("checkout", "loaned_to", param("checkout_time", 15)));
            out.extend(data.checkouts.iter().map(|l| {
                let due = if l.overdue {
                    "OVERDUE".to_owned()
                } else {
                    format!("due in {} min", l.due_in_minutes)
                };
                row(
                    format!("{} by {} — {} — {due}", l.title, l.author, l.borrower),
                    vec![act_button(
                        "Check in".into(),
                        "checkin",
                        param("checked_out_id", l.id),
                        false,
                    )],
                )
            }));
            out
        }
        LIBRARY_ARCHIVE => data
            .pages
            .iter()
            .map(|b| {
                row(
                    format!("#{} {} by {} ({})", b.id, b.title, b.author, b.category),
                    vec![act_button(
                        "Print".into(),
                        "print_book",
                        param("book_id", b.id),
                        false,
                    )],
                )
            })
            .collect(),
        LIBRARY_UPLOAD => {
            if !data.has_scanner {
                vec![line("No nearby scanner detected.".into())]
            } else if !data.has_cache {
                vec![line("No cached book found.".into())]
            } else {
                let mut out = vec![
                    line(format!("Title: {}", data.cache_title)),
                    field("set_cache_title", "title", Vec::new()),
                    line(format!("Author: {}", data.cache_author)),
                    field("set_cache_author", "author", Vec::new()),
                ];
                let uploads = data
                    .upload_categories
                    .iter()
                    .map(|c| {
                        act_button(
                            format!("Upload as {c}"),
                            "upload",
                            param("category", c),
                            false,
                        )
                    })
                    .collect();
                out.push(row(String::new(), uploads));
                out
            }
        }
        LIBRARY_PRINT => vec![
            line(format!(
                "{} — {} ({})",
                data.bible_name, data.deity, data.religion
            )),
            act_button("Print the bible".into(), "print_bible", Vec::new(), false),
        ],
        LIBRARY_TOP_SNEAKY => vec![
            line("The vault's browsing window is open.".into()),
            act_button("Browse".into(), "lore_spawn", Vec::new(), false),
            act_button("Close the tab".into(), "lore_deny", Vec::new(), false),
        ],
        _ => data
            .inventory
            .iter()
            .map(|b| {
                row(
                    format!("{} by {}", b.title, b.author),
                    vec![act_button(
                        "Remove".into(),
                        "inventory_remove",
                        param("book_id", b.id),
                        false,
                    )],
                )
            })
            .collect(),
    };
    bsn! {
        @WindowContent
        Children [
            Node { flex_direction: FlexDirection::Row, column_gap: px(4), margin: UiRect::bottom(px(6)) }
            Children [ {tabs} ]
            --
            @Section { @fill: true, @scrollable: true }
            Children [
                Node { flex_direction: FlexDirection::Column }
                Children [ {body} ]
            ]
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pen_colours_parse() {
        assert_eq!(parse_color("#FF0000"), Color::srgb_u8(255, 0, 0));
        assert_eq!(parse_color("nope"), Color::BLACK);
    }
}
