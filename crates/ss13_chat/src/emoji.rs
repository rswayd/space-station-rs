//! tg `emoji_parse` (`code/modules/emoji/emoji_parse.dm`): `:name:` in player text becomes an
//! emoji segment ([`Span::Emoji`]) when `name` is an icon state of `EMOJI_SET` (BandaStation's
//! `modular_bandastation/emojis220/icons/emoji220.dmi`).

use std::collections::HashMap;

use bevy::prelude::*;
use ss13_shared::IconStateId;
use ss13_shared::chat::{ChatSegment, Span};

/// tg `EMOJI_SET` (BandaStation edit).
pub const EMOJI_SET: &str = "modular_bandastation/emojis220/icons/emoji220.dmi";

/// The emoji names (lower case) and their icon states; filled by the game at startup. Empty:
/// no emojis (tg `CONFIG_GET(flag/emojis)` off).
#[derive(Resource, Clone, Debug, Default)]
pub struct Emojis(pub HashMap<String, IconStateId>);

/// `text` as segments of `span`, every known `:name:` (any case) cut out as an emoji segment
/// whose text is `:name:` as written. A colon that does not open a known emoji stays text and
/// may close the next one, as tg's scan.
pub fn emoji_parse(text: &str, span: Span, emojis: Option<&Emojis>) -> Vec<ChatSegment> {
    let mut out = Vec::new();
    let Some(emojis) = emojis.filter(|e| !e.0.is_empty()) else {
        if !text.is_empty() {
            out.push(ChatSegment::new(span, text));
        }
        return out;
    };
    let mut plain = String::new();
    let mut pos = 0;
    while let Some(open) = text[pos..].find(':').map(|i| pos + i) {
        plain.push_str(&text[pos..open]);
        let Some(close) = text[open + 1..].find(':').map(|i| open + 1 + i) else {
            pos = open;
            break;
        };
        let name = text[open + 1..close].to_lowercase();
        match emojis.0.get(&name) {
            Some(&id) => {
                if !plain.is_empty() {
                    out.push(ChatSegment::new(span, std::mem::take(&mut plain)));
                }
                out.push(ChatSegment::new(Span::Emoji(id), &text[open..=close]));
                pos = close + 1;
            }
            None => {
                plain.push_str(&text[open..close]);
                pos = close;
            }
        }
    }
    plain.push_str(&text[pos..]);
    if !plain.is_empty() {
        out.push(ChatSegment::new(span, plain));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn emojis() -> Emojis {
        Emojis(HashMap::from([
            ("ai".to_owned(), IconStateId(7)),
            ("fire".to_owned(), IconStateId(9)),
        ]))
    }

    #[test]
    fn known_names_become_emojis_and_the_rest_stays_text() {
        let e = emojis();
        let got = emoji_parse("привет :AI: и :нет: :fire:!", Span::Ooc, Some(&e));
        assert_eq!(
            got,
            [
                ChatSegment::new(Span::Ooc, "привет "),
                ChatSegment::new(Span::Emoji(IconStateId(7)), ":AI:"),
                ChatSegment::new(Span::Ooc, " и :нет: "),
                ChatSegment::new(Span::Emoji(IconStateId(9)), ":fire:"),
                ChatSegment::new(Span::Ooc, "!"),
            ]
        );
        // An unknown name's closing colon opens the next emoji: "a:b:ai:" has `:ai:`.
        let got = emoji_parse("a:b:ai:", Span::Ooc, Some(&e));
        assert_eq!(
            got[1],
            ChatSegment::new(Span::Emoji(IconStateId(7)), ":ai:")
        );
        assert_eq!(got[0].text, "a:b");
        // A lone colon and no emojis at all: the text as is.
        assert_eq!(
            emoji_parse("10:30", Span::Ooc, Some(&e)),
            [ChatSegment::new(Span::Ooc, "10:30")]
        );
        assert_eq!(
            emoji_parse(":ai:", Span::Ooc, None),
            [ChatSegment::new(Span::Ooc, ":ai:")]
        );
    }
}
