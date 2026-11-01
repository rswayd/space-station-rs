//! Breaks a message into visual lines with parley, the engine Bevy draws text with, so each
//! line can be a fixed-height `VirtualList` row that is drawn without wrapping.

use std::ops::Range;

use bevy::text::{FontCx, LayoutCx};
use parley::{FontFamily, FontStyle, FontWeight, OverflowWrap, StyleProperty};
use ss13_shared::chat::ChatSegment;

use super::style::span_style;

/// Byte ranges of `text` (the segments concatenated), one per line, for lines at most `width`
/// logical px wide; a `\n` ends a line and belongs to none (a row drawn with it would be two
/// lines tall in a one-line row). Without fonts (headless apps) only `\n` breaks lines.
pub(crate) fn break_lines(
    fonts: Option<(&mut FontCx, &mut LayoutCx)>,
    segments: &[ChatSegment],
    text: &str,
    family: &str,
    font_size: f32,
    width: f32,
) -> Vec<Range<usize>> {
    let Some((font_cx, layout_cx)) = fonts else {
        return hard_lines(text).collect();
    };
    let mut builder = layout_cx
        .0
        .ranged_builder(&mut font_cx.context, text, 1.0, true);
    builder.push_default(StyleProperty::FontFamily(FontFamily::named(family)));
    builder.push_default(StyleProperty::FontSize(font_size));
    // tgui-panel `.ChatMessage { word-wrap: break-word }`: long words break anywhere.
    builder.push_default(StyleProperty::OverflowWrap(OverflowWrap::Anywhere));
    let mut start = 0;
    for segment in segments {
        let range = start..start + segment.text.len();
        start = range.end;
        let style = span_style(segment.span);
        if style.bold {
            builder.push(StyleProperty::FontWeight(FontWeight::BOLD), range.clone());
        }
        if style.italic {
            builder.push(StyleProperty::FontStyle(FontStyle::Italic), range);
        }
    }
    let mut layout = builder.build(text);
    layout.break_all_lines(Some(width.max(1.0)));
    let lines: Vec<Range<usize>> = layout
        .lines()
        .map(|line| {
            let range = line.text_range();
            let kept = text[range.clone()].trim_end_matches(['\n', '\r']).len();
            range.start..range.start + kept
        })
        .collect();
    if lines.is_empty() {
        hard_lines(text).collect()
    } else {
        lines
    }
}

/// The ranges between `\n`s.
fn hard_lines(text: &str) -> impl Iterator<Item = Range<usize>> + '_ {
    let mut start = 0;
    text.split('\n').map(move |line| {
        let range = start..start + line.trim_end_matches('\r').len();
        start += line.len() + 1;
        range
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hard_breaks_without_fonts() {
        let text = "a\nbc\r\n\nd";
        let lines: Vec<&str> = break_lines(None, &[], text, "", 13.0, 100.0)
            .into_iter()
            .map(|r| &text[r])
            .collect();
        assert_eq!(lines, ["a", "bc", "", "d"]);
    }
}
