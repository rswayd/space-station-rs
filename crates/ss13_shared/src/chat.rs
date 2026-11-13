//! Chat wire contract: server-built, structured messages. No HTML and no markup parsing on
//! either side: a message is a list of text segments, each tagged with a tg span class, so
//! player text can never inject styling.

use serde::{Deserialize, Serialize};

/// tg `MAX_MESSAGE_LEN`, in characters (Unicode scalar values).
pub const MAX_MESSAGE_LEN: usize = 1024;

/// tg `MAX_NAME_LEN`, in characters.
pub const MAX_NAME_LEN: usize = 42;

/// tgui-panel `MESSAGE_TYPE_*`: what a message is about, for tabs and filters.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ChatChannel {
    /// Server and client notices (MOTD, kicks, connection).
    System,
    /// In-character local speech and emotes.
    Local,
    Radio,
    /// Non-urgent game messages.
    Info,
    /// Urgent game messages.
    Warning,
    Ooc,
    Combat,
    /// tgui-panel `MESSAGE_TYPE_ADMINPM`: tickets and admin PMs.
    AdminPm,
    /// `MESSAGE_TYPE_ADMINCHAT`: asay.
    AdminChat,
    /// `MESSAGE_TYPE_ADMINLOG`: `message_admins`.
    AdminLog,
}

/// tg span classes (`code/__DEFINES/span.dm`) the client knows how to draw.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Span {
    /// No class: default chat text.
    Plain,
    /// `.name`: speaker name.
    Name,
    /// `.say`.
    Say,
    Emote,
    Radio,
    Info,
    Notice,
    BoldNotice,
    Warning,
    BoldWarning,
    Danger,
    BoldAnnounce,
    Ooc,
    Bold,
    Italics,
    /// Radio channel classes (`GLOB.freqtospan`); [`Span::Radio`] is `.radio` (Common and
    /// frequencies without a class).
    SciRadio,
    ComRadio,
    SecRadio,
    MedRadio,
    EngRadio,
    SuppRadio,
    ServRadio,
    EnteRadio,
    SyndRadio,
    CentcomRadio,
    AiPrivRadio,
    /// BandaStation `.justiceradio`.
    JusticeRadio,
    /// `.greentext` (`span_greentext`: a round-end success).
    GreenText,
    /// `.redtext` (`span_redtext`: a round-end failure).
    RedText,
    /// `.binarysay` (`span_binarysay`: silicons' binary chat).
    BinarySay,
    /// `.adminsay` (asay).
    AdminSay,
    /// `.admin` (`message_admins`).
    Admin,
    /// `.adminhelp` (tickets).
    AdminHelp,
    /// `.adminnotice`.
    AdminNotice,
    /// `.changeling` (`span_changeling`: the changeling hivemind and its powers).
    Changeling,
    /// `.cult` (`span_cult`: the blood cult's runes and magic).
    Cult,
    /// `.cult_italic` (`span_cult_italic`: cult communion, rune feedback).
    CultItalic,
    /// `.cult_large` (`span_cult_large`: Nar'Sie's words, the cult master).
    CultLarge,
    /// `.mansus` (`span_mansus`: the heretic's Mansus whispers).
    Mansus,
    /// `.hypnophrase` (`span_hypnophrase`: influences drained, sacrifices accepted).
    Hypnophrase,
    /// BandaStation `.looc`.
    Looc,
    /// `.adminooc`: an admin in OOC.
    AdminOoc,
    /// `.blue`: a traitor's code phrases in heard speech (`codeword_hearing`).
    Blue,
    /// `.red`: a traitor's code responses in heard speech.
    Red,
    /// tg `emoji_parse`: the segment is one emoji, drawn as this icon state of `EMOJI_SET`; its
    /// text (`:name:`) is what a client without the icon shows and what logs keep.
    Emoji(crate::IconStateId),
    /// A clickable chat link (BYOND `<a href='?src=...'>` in `to_chat`): a click sends
    /// `ClientIntent::ChatLink` with this id, which only the server knows the meaning of.
    Link(u32),
    /// BandaStation `get_ooc_badged_name`: a donor's key in OOC, in their `ooc_color`, with the
    /// name effect of their tier (`None`: only the color).
    DonorKey {
        rgb: [u8; 3],
        shine: Option<NameShine>,
    },
}

/// BandaStation's donor name effects (`chat-dark.scss`, `.tier-3`..`.tier-5` with `.metal`,
/// `.glowing` or `.auto`), resolved by the server from the tier and the player's choice.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NameShine {
    /// `@mixin metalic`: a light band sweeping across the name every 5 s.
    #[default]
    Metallic,
    /// `@mixin glowing`: a hue-shifted light band every 3 s.
    Glowing,
    /// `glowing` with `@mixin shine`'s blurred halo (tier 5).
    GlowingShine,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChatSegment {
    pub span: Span,
    pub text: String,
}

impl ChatSegment {
    pub fn new(span: Span, text: impl Into<String>) -> Self {
        Self {
            span,
            text: text.into(),
        }
    }
}

/// Server → client, reliable and ordered.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChatMessage {
    pub channel: ChatChannel,
    pub segments: Vec<ChatSegment>,
}

impl ChatMessage {
    /// A one-segment message.
    pub fn simple(channel: ChatChannel, span: Span, text: impl Into<String>) -> Self {
        Self {
            channel,
            segments: vec![ChatSegment::new(span, text)],
        }
    }

    /// The text without styling.
    pub fn plain_text(&self) -> String {
        self.segments.iter().map(|s| s.text.as_str()).collect()
    }
}

/// What a player's emphasis mark makes of the text between a pair of it (tg
/// `GLOB.emphasis_types`: `|` italics, `+` bold, `_` underline, `^` small).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Emphasis {
    Italic,
    Bold,
    Underline,
    Small,
}

impl Emphasis {
    fn of(mark: char) -> Option<Self> {
        match mark {
            '|' => Some(Self::Italic),
            '+' => Some(Self::Bold),
            '_' => Some(Self::Underline),
            '^' => Some(Self::Small),
            _ => None,
        }
    }
}

/// tg `apply_message_emphasis`: `(\\?(?:\||\+|_|\^))(.+?)\1` in one pass. A mark, some text
/// (on one line) and the same mark again make an emphasized run; an escaped pair (`\|x\|`)
/// leaves the marks in as plain text, unescaped. Marks inside a run stay as they are.
pub fn emphasis_runs(text: &str) -> Vec<(Option<Emphasis>, String)> {
    let chars: Vec<char> = text.chars().collect();
    let mut runs: Vec<(Option<Emphasis>, String)> = Vec::new();
    let mut plain = String::new();
    let mut i = 0;
    while i < chars.len() {
        let escaped =
            chars[i] == '\\' && chars.get(i + 1).is_some_and(|&c| Emphasis::of(c).is_some());
        let open: &[char] = if escaped {
            &chars[i..i + 2]
        } else if Emphasis::of(chars[i]).is_some() {
            &chars[i..i + 1]
        } else {
            plain.push(chars[i]);
            i += 1;
            continue;
        };
        let body_from = i + open.len();
        // `.+?`: at least one character, no line break, up to the first closing mark.
        let close = (body_from + 1..=chars.len().saturating_sub(open.len()))
            .take_while(|&j| chars[j - 1] != '\n')
            .find(|&j| chars[j..].starts_with(open));
        let Some(close) = close else {
            plain.push(chars[i]);
            i += 1;
            continue;
        };
        let body: String = chars[body_from..close].iter().collect();
        if escaped {
            let mark = open[1];
            plain.push(mark);
            plain.push_str(&body);
            plain.push(mark);
        } else {
            if !plain.is_empty() {
                runs.push((None, std::mem::take(&mut plain)));
            }
            runs.push((Emphasis::of(open[0]), body));
        }
        i = close + open.len();
    }
    if !plain.is_empty() {
        runs.push((None, plain));
    }
    runs
}

#[cfg(test)]
mod emphasis_tests {
    use super::*;

    fn runs(text: &str) -> Vec<(Option<Emphasis>, &str)> {
        // Leaked for the comparison's lifetimes; a test only.
        emphasis_runs(text)
            .into_iter()
            .map(|(e, t)| (e, &*Box::leak(t.into_boxed_str())))
            .collect()
    }

    /// tg `code/modules/unit_tests/say.dm` `emphasis_characters`.
    #[test]
    fn emphasis_follows_tg() {
        assert_eq!(runs("Blah Blah Blah"), [(None, "Blah Blah Blah")]);
        assert_eq!(
            runs("Blah |Blah| Blah"),
            [
                (None, "Blah "),
                (Some(Emphasis::Italic), "Blah"),
                (None, " Blah")
            ]
        );
        assert_eq!(
            runs("+Blah+ _Blah_"),
            [
                (Some(Emphasis::Bold), "Blah"),
                (None, " "),
                (Some(Emphasis::Underline), "Blah")
            ]
        );
        // Escaped pairs stay, unescaped; a lone or empty pair is plain text.
        assert_eq!(runs(r"Blah \|Blah\| Blah"), [(None, "Blah |Blah| Blah")]);
        assert_eq!(runs("a | b"), [(None, "a | b")]);
        assert_eq!(
            runs("Blah + Blah = Blah :)"),
            [(None, "Blah + Blah = Blah :)")]
        );
        assert_eq!(
            runs(r"\|Blah\| \+Blah\+ Blah"),
            [(None, "|Blah| +Blah+ Blah")]
        );
        assert_eq!(runs("||"), [(None, "||")]);
        // One pass: marks inside a run are left alone.
        assert_eq!(runs("|a _b_ c|"), [(Some(Emphasis::Italic), "a _b_ c")]);
        assert_eq!(runs("^тихо^"), [(Some(Emphasis::Small), "тихо")]);
    }
}
