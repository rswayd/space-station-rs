//! tgui-panel's saved chat (`saveChatMessages`: the last [`MAX_PERSISTED_MESSAGES`], restored on
//! the next start) and "Save chat log" (`saveToDisk`: the shown tab as an HTML page). The bytes
//! and the page are made here; `ss13_game` keeps the files beside the client settings.

use ss13_shared::chat::{ChatMessage, ChatSegment, Span};
use ss13_shared::protocol::PROTOCOL_VERSION;

use super::log::ChatLog;
use super::prefs::ChannelSet;
use super::style;

/// tgui-panel `MAX_PERSISTED_MESSAGES`.
pub const MAX_PERSISTED_MESSAGES: usize = 250;
/// tgui-panel `MESSAGE_SAVE_INTERVAL` (30 s).
pub const MESSAGE_SAVE_INTERVAL_SECS: f64 = 30.0;

/// A message as it is kept: links and emoji become their text. Link ids belong to the session
/// that sent them (tgui-panel strips `<a>` from restored messages), emoji ids to the asset build.
fn persisted(message: &ChatMessage) -> ChatMessage {
    ChatMessage {
        channel: message.channel,
        segments: message
            .segments
            .iter()
            .map(|s| match s.span {
                Span::Link(_) | Span::Emoji(_) => ChatSegment::new(Span::Plain, s.text.clone()),
                _ => s.clone(),
            })
            .collect(),
    }
}

/// The last [`MAX_PERSISTED_MESSAGES`] messages of `log` with their repeat counts.
pub fn encode(log: &ChatLog) -> Vec<u8> {
    let from = log.len().saturating_sub(MAX_PERSISTED_MESSAGES);
    let messages: Vec<(ChatMessage, u32)> = log
        .entries()
        .range(from..)
        .map(|e| (persisted(&e.message), e.times))
        .collect();
    postcard::to_stdvec(&(PROTOCOL_VERSION, messages)).unwrap_or_default()
}

/// The messages [`encode`] kept; nothing when they are from another protocol version or
/// unreadable (tgui-panel discards an incompatible saved state).
pub fn decode(bytes: &[u8]) -> Vec<(ChatMessage, u32)> {
    match postcard::from_bytes::<(u32, Vec<(ChatMessage, u32)>)>(bytes) {
        Ok((version, mut messages)) if version == PROTOCOL_VERSION => {
            let from = messages.len().saturating_sub(MAX_PERSISTED_MESSAGES);
            messages.drain(..from);
            messages
        }
        _ => Vec::new(),
    }
}

fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
    out
}

fn css_color(color: bevy::color::Color) -> String {
    bevy::color::Srgba::from(color).to_hex()
}

/// `saveToDisk`: the messages `filter` shows, styled as the panel draws them.
pub fn export_html(log: &ChatLog, filter: ChannelSet, font_size: f32, line_height: f32) -> String {
    let mut body = String::new();
    for entry in log.entries() {
        if !filter.accepts(entry.message.channel) {
            continue;
        }
        let outer = style::outer_span(&entry.message);
        body.push_str("<div class=\"ChatMessage\">");
        for segment in &entry.message.segments {
            if segment.text.is_empty() {
                continue;
            }
            let s = style::span_style_in(segment.span, outer);
            let mut css = format!("color:{}", css_color(s.color));
            if s.bold {
                css.push_str(";font-weight:bold");
            }
            if s.italic {
                css.push_str(";font-style:italic");
            }
            body.push_str(&format!(
                "<span style=\"{css}\">{}</span>",
                escape_html(&segment.text)
            ));
        }
        if entry.times > 1 {
            body.push_str(&format!(
                "<span class=\"Chat__badge\">{}</span>",
                entry.times
            ));
        }
        body.push_str("</div>\n");
    }
    format!(
        "<!doctype html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n<title>SS13 Chat Log</title>\n\
         <style>\nbody, html {{ background-color: #141414 }}\n\
         .Chat {{ color: {text}; font-family: Verdana, sans-serif; font-size: {font_size}px; \
         line-height: {line_height}; white-space: pre-wrap; word-wrap: break-word }}\n\
         .Chat__badge {{ display: inline-block; margin-left: 4px; padding: 0 4px; \
         border-radius: 10px; font-size: 0.7em; color: #fff; background-color: {badge} }}\n\
         </style>\n</head>\n<body>\n<div class=\"Chat\">\n{body}</div>\n</body>\n</html>\n",
        text = css_color(style::CHAT_TEXT),
        badge = css_color(style::BADGE_BACKGROUND),
    )
}

/// `ss13-chatlog-YYYYMMDD-HHMMSS.html` (UTC) for `unix_secs`.
pub fn export_file_name(unix_secs: u64) -> String {
    let days = (unix_secs / 86_400) as i64;
    let secs = unix_secs % 86_400;
    // Howard Hinnant's `civil_from_days`.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "ss13-chatlog-{year:04}{month:02}{day:02}-{:02}{:02}{:02}.html",
        secs / 3600,
        secs / 60 % 60,
        secs % 60
    )
}

#[cfg(test)]
mod tests {
    use ss13_shared::chat::ChatChannel;

    use super::*;

    #[test]
    fn history_keeps_the_last_messages_without_session_ids() {
        let mut log = ChatLog::default();
        for i in 0..300 {
            log.push(
                ChatMessage::simple(ChatChannel::Local, Span::Say, i.to_string()),
                f64::from(i) * 10.0,
            );
        }
        log.push(
            ChatMessage {
                channel: ChatChannel::Ooc,
                segments: vec![
                    ChatSegment::new(Span::Link(7), "ссылка"),
                    ChatSegment::new(Span::Emoji(ss13_shared::IconStateId(3)), ":smile:"),
                ],
            },
            4000.0,
        );
        log.push(
            ChatMessage::simple(ChatChannel::Local, Span::Say, "x"),
            4000.0,
        );
        log.push(
            ChatMessage::simple(ChatChannel::Local, Span::Say, "x"),
            4001.0,
        );
        let back = decode(&encode(&log));
        assert_eq!(back.len(), MAX_PERSISTED_MESSAGES);
        assert_eq!(back[0].0.plain_text(), "52");
        let (link, _) = &back[MAX_PERSISTED_MESSAGES - 2];
        assert!(link.segments.iter().all(|s| s.span == Span::Plain));
        assert_eq!(link.plain_text(), "ссылка:smile:");
        assert_eq!(back.last().unwrap().1, 2, "the repeat count is kept");

        // Restored: before anything new, never combined with it.
        let mut restored = ChatLog::default();
        restored.restore(back);
        assert_eq!(restored.len(), MAX_PERSISTED_MESSAGES);
        let id = restored.push(ChatMessage::simple(ChatChannel::Local, Span::Say, "x"), 0.0);
        assert_eq!(id, MAX_PERSISTED_MESSAGES as u64);

        // Another version, or garbage: nothing.
        let other =
            postcard::to_stdvec(&(PROTOCOL_VERSION + 1, Vec::<(ChatMessage, u32)>::new())).unwrap();
        assert!(decode(&other).is_empty());
        assert!(decode(b"\xff\xff\xff").is_empty());
    }

    #[test]
    fn export_is_the_shown_tab_escaped() {
        let mut log = ChatLog::default();
        log.push(
            ChatMessage::simple(ChatChannel::Local, Span::Bold, "<b>&"),
            0.0,
        );
        log.push(ChatMessage::simple(ChatChannel::Ooc, Span::Ooc, "ooc"), 0.0);
        let mut filter = ChannelSet::NONE;
        filter.toggle(ChatChannel::Local);
        let html = export_html(&log, filter, 13.0, 1.2);
        assert!(html.contains("&lt;b&gt;&amp;"));
        assert!(html.contains("font-weight:bold"));
        assert!(!html.contains("ooc</span>"));
        assert_eq!(
            export_file_name(1_700_000_000),
            "ss13-chatlog-20231114-221320.html"
        );
    }
}
