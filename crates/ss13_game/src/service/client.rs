//! The paper and book windows (client): tgui `PaperSheet` and `MarkdownViewer`. The data is
//! `crate::service`, the server half `crate::service::server`.
// shortcut: plain text instead of rendered markdown/HTML and fonts (each input keeps its colour
// and boldness on a white sheet); stamps are listed by name under the text, not drawn.

use bevy::app::Propagate;
use ss13_tgui::client::{AppTguiClientExt, ClientUi};
use ss13_ui_kit::prelude::*;
use ss13_ui_kit::theme::Bold;

use super::{MarkdownViewerData, MarkdownViewerUi, PaperSheetData, PaperSheetUi, plain_text};

pub struct ServiceClientPlugin;

impl Plugin for ServiceClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_view::<PaperSheetUi>()
            .register_ui_view::<MarkdownViewerUi>();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pen_colours_parse() {
        assert_eq!(parse_color("#FF0000"), Color::srgb_u8(255, 0, 0));
        assert_eq!(parse_color("nope"), Color::BLACK);
    }
}
