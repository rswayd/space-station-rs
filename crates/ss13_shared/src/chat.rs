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
    /// tg `emoji_parse`: the segment is one emoji, drawn as this icon state of `EMOJI_SET`; its
    /// text (`:name:`) is what a client without the icon shows and what logs keep.
    Emoji(crate::IconStateId),
    /// A clickable chat link (BYOND `<a href='?src=...'>` in `to_chat`): a click sends
    /// `ClientIntent::ChatLink` with this id, which only the server knows the meaning of.
    Link(u32),
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
