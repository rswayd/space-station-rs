//! tg span classes → text style, from tgui-panel's dark chat theme (`styles/tgchat/chat-dark.scss`).

use bevy::prelude::*;
use ss13_shared::chat::Span;

/// `--chat-text-color`.
pub const CHAT_TEXT: Color = Color::hsl(210.0, 0.5, 0.8);
/// `--color-base` of tgui-panel.
pub const PANEL_BACKGROUND: Color = Color::hsl(0.0, 0.0, 0.125);
/// `.Chat__badge` background.
pub const BADGE_BACKGROUND: Color = Color::hsl(348.0, 0.83, 0.47);

const RED: Color = Color::hsl(0.0, 0.736, 0.445);
const NOTICE: Color = Color::hsl(227.0, 0.877, 0.68);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpanStyle {
    pub color: Color,
    pub bold: bool,
    pub italic: bool,
}

pub fn span_style(span: Span) -> SpanStyle {
    let (color, bold, italic) = match span {
        Span::Plain | Span::Say | Span::Emote => (CHAT_TEXT, false, false),
        Span::Name | Span::Bold => (CHAT_TEXT, true, false),
        Span::Italics => (CHAT_TEXT, false, true),
        Span::Radio => (Color::hsl(132.8, 0.744, 0.459), false, false),
        Span::Info => (Color::hsl(226.9, 1.0, 0.802), false, false),
        Span::Notice => (NOTICE, false, false),
        Span::BoldNotice => (NOTICE, true, false),
        Span::Warning => (RED, false, true),
        Span::BoldWarning => (RED, true, true),
        Span::Danger => (RED, false, false),
        Span::BoldAnnounce => (RED, true, false),
        Span::Ooc => (Color::hsl(47.9, 1.0, 0.4), true, false),
        Span::SciRadio => (Color::hsl(271.6, 0.917, 0.765), false, false),
        Span::ComRadio => (Color::hsl(53.0, 0.976, 0.5), false, false),
        Span::SecRadio => (Color::hsl(0.0, 0.712, 0.537), false, false),
        Span::MedRadio => (Color::hsl(202.0, 0.836, 0.641), false, false),
        Span::EngRadio => (Color::hsl(17.0, 0.878, 0.614), false, false),
        Span::SuppRadio => (Color::hsl(33.7, 0.449, 0.498), false, false),
        Span::ServRadio => (Color::hsl(88.1, 0.606, 0.408), false, false),
        Span::EnteRadio => (Color::hsl(157.1, 0.396, 0.624), false, false),
        // `hsl(359.1, 31.8%, 42.5%)`.
        Span::SyndRadio => (Color::srgb_u8(0x8f, 0x4a, 0x4b), false, false),
        Span::CentcomRadio => (Color::hsl(197.0, 0.626, 0.398), false, false),
        Span::AiPrivRadio => (Color::hsl(332.2, 0.596, 0.602), false, false),
        // BandaStation `.justiceradio { color: #df3e71 }`.
        Span::JusticeRadio => (Color::srgb_u8(0xdf, 0x3e, 0x71), false, false),
        // shortcut: tg also sets `font-size: 185%`; chat text has one size here.
        Span::GreenText => (Color::hsl(132.8, 0.934, 0.296), true, false),
        Span::RedText => (RED, true, false),
        Span::BinarySay => (Color::hsl(209.6, 1.0, 0.559), false, false),
        Span::AdminSay => (Color::hsl(16.2, 1.0, 0.5), true, false),
        Span::Admin => (Color::hsl(227.0, 0.635, 0.602), true, false),
        Span::AdminHelp => (Color::hsl(0.0, 1.0, 0.5), true, false),
        Span::AdminNotice => (NOTICE, false, false),
        Span::Changeling => (Color::hsl(132.8, 0.934, 0.296), false, true),
        Span::Cult => (Color::hsl(2.0, 0.438, 0.412), false, false),
        Span::CultItalic => (Color::hsl(2.0, 0.438, 0.412), false, true),
        // shortcut: tg also sets `font-size: 185%`; chat text has one size here.
        Span::CultLarge => (Color::hsl(2.0, 0.438, 0.412), true, false),
        // shortcut: no `text-shadow` glow.
        Span::Mansus => (Color::srgb_u8(0x0d, 0x9b, 0x00), false, false),
        // shortcut: the `hypnocolor` animation is drawn in its 50% purple.
        // BandaStation `.looc { color: #6699cc }`, bold with `.ooc`.
        Span::Looc => (Color::srgb_u8(0x66, 0x99, 0xcc), true, false),
        // `.adminooc`.
        Span::AdminOoc => (Color::hsl(226.6, 0.528, 0.502), true, false),
        Span::Hypnophrase => (Color::hsl(272.0, 0.863, 0.6), true, false),
        // Drawn as an icon (`super::panel`); its `:name:` text measures the line.
        Span::Emoji(_) => (CHAT_TEXT, false, false),
        // `a { color: var(--color-hyperlink) }`.
        Span::Link(_) => (Color::hsl(207.0, 0.9, 0.65), false, false),
    };
    SpanStyle {
        color,
        bold,
        italic,
    }
}

/// tg radio classes: the whole radio line is wrapped in one (`compose_message`).
pub fn is_radio(span: Span) -> bool {
    matches!(
        span,
        Span::Radio
            | Span::SciRadio
            | Span::ComRadio
            | Span::SecRadio
            | Span::MedRadio
            | Span::EngRadio
            | Span::SuppRadio
            | Span::ServRadio
            | Span::EnteRadio
            | Span::SyndRadio
            | Span::CentcomRadio
            | Span::AiPrivRadio
            | Span::JusticeRadio
            | Span::BinarySay
    )
}

/// [`span_style`] of a span nested in `outer`: classes without a color of their own (`.name`,
/// `.italics`, plain text) take the enclosing one's, as in the browser.
pub fn span_style_in(span: Span, outer: Option<Span>) -> SpanStyle {
    let mut style = span_style(span);
    if let Some(outer) = outer
        && style.color == CHAT_TEXT
    {
        style.color = span_style(outer).color;
    }
    style
}

/// The span a whole message is nested in: the radio class of a radio line.
pub fn outer_span(message: &ss13_shared::chat::ChatMessage) -> Option<Span> {
    (message.channel == ss13_shared::chat::ChatChannel::Radio)
        .then(|| {
            message
                .segments
                .iter()
                .map(|s| s.span)
                .find(|&s| is_radio(s))
        })
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ss13_shared::chat::{ChatChannel, ChatMessage};

    #[test]
    fn radio_lines_color_their_name_and_text() {
        let line = crate::radio::radio_message(1357, "Боб", "шепчет", "т*х*", true);
        let outer = outer_span(&line);
        assert_eq!(outer, Some(Span::EngRadio));
        let eng = span_style(Span::EngRadio).color;
        let name = span_style_in(Span::Name, outer);
        assert!(name.bold && name.color == eng);
        let text = span_style_in(Span::Italics, outer);
        assert!(text.italic && text.color == eng);
        // Spans with a color of their own keep it; other channels nest nothing.
        assert_eq!(span_style_in(Span::Warning, outer).color, RED);
        let local = ChatMessage::simple(ChatChannel::Local, Span::Radio, "x");
        assert_eq!(outer_span(&local), None);
    }
}
