//! The panel's own settings: tgui-panel's chat tabs (`chat/model.ts`, `use-chat-pages.ts`), font
//! size and line height (`settings/`), text highlights (`renderer.tsx` `setHighlight`), and
//! BandaStation's emote panel list (`custom_emote_panel.dm`).
//!
//! They are client settings (`chat.*` keys); `ss13_game` keeps them in its settings file, this
//! crate only encodes them ([`ChatPrefs::to_settings`], [`ChatPrefs::from_settings`]).
// shortcut: BandaStation keeps the emote panel in the player's preferences on the server; the
// port has no preferences yet, so the list lives in this client's settings (move it into the
// preferences when they exist).

use std::ops::Range;
use std::sync::LazyLock;

use bevy::prelude::*;
use regex::{Regex, RegexBuilder};
use ss13_shared::chat::{ChatChannel, ChatMessage, Span};
use ss13_shared::protocol::PanelEmote;

/// tgui-panel `fontSize` slider: 8-32 px, 13 by default.
pub const FONT_SIZE_MIN: f32 = 8.0;
pub const FONT_SIZE_MAX: f32 = 32.0;
pub const DEFAULT_FONT_SIZE: f32 = 13.0;
/// tgui-panel `lineHeight` slider: 0.8-5, 1.2 by default.
pub const LINE_HEIGHT_MIN: f32 = 0.8;
pub const LINE_HEIGHT_MAX: f32 = 5.0;
pub const DEFAULT_LINE_HEIGHT: f32 = 1.2;

/// `WARN_AFTER_HIGHLIGHT_AMT`.
pub const WARN_AFTER_HIGHLIGHT_AMT: usize = 10;
/// `defaultHighlightSetting.highlightColor`.
pub const DEFAULT_HIGHLIGHT_COLOR: &str = "#ffdd44";

/// BandaStation `TGUI_PANEL_MAX_EMOTES`.
pub const MAX_EMOTES: usize = 45;
/// `TGUI_PANEL_MAX_EMOTE_LENGTH`.
pub const MAX_EMOTE_LENGTH: usize = 128;
/// `TGUI_PANEL_MAX_EMOTE_NAME_LENGTH`.
pub const MAX_EMOTE_NAME_LENGTH: usize = 32;

/// Pixel height of one chat line.
pub fn row_height(font_size: f32, line_height: f32) -> f32 {
    (font_size * line_height).ceil().max(1.0)
}

/// The message types a tab shows (tgui-panel `acceptedTypes`), one bit per [`ChatChannel`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ChannelSet(pub u32);

impl ChannelSet {
    pub const NONE: Self = Self(0);
    pub const ALL: Self = Self(u32::MAX);

    fn bit(channel: ChatChannel) -> u32 {
        1 << (channel as u32)
    }

    pub fn contains(self, channel: ChatChannel) -> bool {
        self.0 & Self::bit(channel) != 0
    }

    pub fn toggle(&mut self, channel: ChatChannel) {
        self.0 ^= Self::bit(channel);
    }

    /// `canPageAcceptType`: system messages are `important`, always shown.
    pub fn accepts(self, channel: ChatChannel) -> bool {
        channel == ChatChannel::System || self.contains(channel)
    }
}

/// tgui-panel `MESSAGE_TYPES` the port's channels stand for, as the tab settings list them:
/// (channel, name, admin-only).
pub const PAGE_CHANNELS: [(ChatChannel, &str, bool); 9] = [
    (ChatChannel::Local, "Местный чат", false),
    (ChatChannel::Radio, "Рация", false),
    (ChatChannel::Info, "Информация", false),
    (ChatChannel::Warning, "Предупреждения", false),
    (ChatChannel::Ooc, "OOC", false),
    (ChatChannel::AdminPm, "Админ ЛС", false),
    (ChatChannel::Combat, "Журнал боя", false),
    (ChatChannel::AdminChat, "Админ-чат", true),
    (ChatChannel::AdminLog, "Админ-лог", true),
];

/// tgui-panel `Page`.
#[derive(Clone, Debug, PartialEq)]
pub struct ChatPage {
    pub name: String,
    pub channels: ChannelSet,
    /// `hideUnreadCount` ("Mute").
    pub hide_unread: bool,
    /// Not saved, as tgui-panel's `saveChatState` zeroes it.
    pub unread: u32,
}

impl ChatPage {
    /// `createMainPage`.
    pub fn main() -> Self {
        Self {
            name: "Основная".into(),
            channels: ChannelSet::ALL,
            hide_unread: false,
            unread: 0,
        }
    }

    /// `createPage`: only the `important` (system) messages.
    pub fn new_tab() -> Self {
        Self {
            name: "Новая вкладка".into(),
            channels: ChannelSet::NONE,
            hide_unread: false,
            unread: 0,
        }
    }

    /// The tab's label: its name and, unless muted, the unread count (`UnreadCountWidget`).
    pub fn label(&self) -> String {
        if self.hide_unread || self.unread == 0 {
            self.name.clone()
        } else {
            format!("{} ({})", self.name, self.unread.min(99))
        }
    }
}

/// tgui-panel's pages: the first is the main one, which never moves and is never removed.
#[derive(Clone, Debug, PartialEq)]
pub struct ChatPages {
    pub pages: Vec<ChatPage>,
    pub current: usize,
}

impl Default for ChatPages {
    fn default() -> Self {
        Self {
            pages: vec![ChatPage::main()],
            current: 0,
        }
    }
}

impl ChatPages {
    pub fn current(&self) -> &ChatPage {
        &self.pages[self.current.min(self.pages.len() - 1)]
    }

    pub fn current_mut(&mut self) -> &mut ChatPage {
        let i = self.current.min(self.pages.len() - 1);
        &mut self.pages[i]
    }

    /// `addChatPage`: a new tab, shown at once.
    pub fn add(&mut self) {
        self.pages.push(ChatPage::new_tab());
        self.current = self.pages.len() - 1;
    }

    /// `changeChatPage`: its unread count is read.
    pub fn select(&mut self, index: usize) {
        if let Some(page) = self.pages.get_mut(index) {
            page.unread = 0;
            self.current = index;
        }
    }

    /// `removeChatPage`: the shown tab, back to the first; the main tab stays.
    pub fn remove_current(&mut self) {
        if self.current == 0 || self.current >= self.pages.len() {
            return;
        }
        self.pages.remove(self.current);
        self.current = 0;
    }

    /// `moveChatLeft`: never the main tab, never onto its place.
    pub fn move_left(&mut self) {
        let from = self.current;
        if from <= 1 || from >= self.pages.len() {
            return;
        }
        self.pages.swap(from, from - 1);
        self.current = from - 1;
    }

    /// `moveChatRight`.
    pub fn move_right(&mut self) {
        let from = self.current;
        if from == 0 || from + 1 >= self.pages.len() {
            return;
        }
        self.pages.swap(from, from + 1);
        self.current = from + 1;
    }

    /// `updateMessageCount` for one new message: counted on every tab that shows it, except the
    /// shown one while it follows new messages, and except tabs whose messages the shown tab
    /// shows too.
    pub fn count_unread(&mut self, channel: ChatChannel, tracking: bool) {
        let current = self.current;
        let current_accepts = self.current().channels.accepts(channel);
        for (i, page) in self.pages.iter_mut().enumerate() {
            if !page.channels.accepts(channel) {
                continue;
            }
            if i == current && tracking {
                continue;
            }
            if i != current && current_accepts {
                continue;
            }
            page.unread += 1;
        }
    }
}

/// tgui-panel `HighlightSetting`.
// shortcut: no highlight sound, job or character filter (the client has no sound path for
// arbitrary files here, nor the own job and character); add them with the preferences menu.
#[derive(Clone, Debug, PartialEq)]
pub struct Highlight {
    /// Comma-separated words, or `/regex/` entries.
    pub text: String,
    /// `#rrggbb`.
    pub color: String,
    pub whole_message: bool,
    pub match_word: bool,
    pub match_case: bool,
    pub enabled: bool,
}

impl Default for Highlight {
    fn default() -> Self {
        Self {
            text: String::new(),
            color: DEFAULT_HIGHLIGHT_COLOR.into(),
            whole_message: true,
            match_word: false,
            match_case: false,
            enabled: true,
        }
    }
}

impl Highlight {
    pub fn color(&self) -> Color {
        parse_color(&self.color).unwrap_or_else(|| parse_color(DEFAULT_HIGHLIGHT_COLOR).unwrap())
    }

    /// Whether the text compiles to a pattern (the settings show a broken one in red).
    pub fn is_valid(&self) -> bool {
        self.text.trim().is_empty() || compile(self).is_some()
    }
}

/// `#rrggbb` (or `rrggbb`, `#rgb`).
pub fn parse_color(text: &str) -> Option<Color> {
    Srgba::hex(text.trim()).ok().map(Color::from)
}

/// `allowedRegex`: letters, digits and the few signs a plain entry may have.
static ALLOWED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^[a-zа-яё0-9_\-$/^\[\s\]\\]+$").unwrap());
/// `oneCharacterRegex`: a `/regex/` matching one character is refused.
static ONE_CHARACTER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\[.*\]|\\.|.)$").unwrap());
/// `RADIO_CHANNEL_LABEL`: a radio line's `[Channel] ` is never highlighted.
static RADIO_LABEL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\[[^\]]+\]\s*").unwrap());

/// `setHighlight` for one setting: the pattern of its entries (`/regex/` entries win over
/// words, as there), case-insensitive unless `match_case`.
fn compile(setting: &Highlight) -> Option<Regex> {
    let mut expressions = Vec::new();
    let mut words = Vec::new();
    for line in setting.text.split(',').map(str::trim) {
        let slashed = line.len() >= 2 && line.starts_with('/') && line.ends_with('/');
        if line.chars().count() <= 1 || !(ALLOWED.is_match(line) || slashed) {
            continue;
        }
        if slashed {
            let expression = &line[1..line.len() - 1];
            // An empty one would match everywhere (tgui-panel's loop guard drops it).
            if expression.is_empty() || ONE_CHARACTER.is_match(expression) {
                continue;
            }
            expressions.push(expression.to_owned());
        } else {
            words.push(regex::escape(line));
        }
    }
    let pattern = if !expressions.is_empty() {
        format!("({})", expressions.join("|"))
    } else if !words.is_empty() {
        let edge = if setting.match_word { r"\b" } else { "" };
        format!("{edge}({}){edge}", words.join("|"))
    } else {
        return None;
    };
    // A broken pattern highlights nothing, as tgui-panel's `try`.
    RegexBuilder::new(&pattern)
        .case_insensitive(!setting.match_case)
        .size_limit(1 << 20)
        .build()
        .ok()
}

/// What a message gets highlighted with.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Marks {
    /// Byte ranges of the message's plain text and their color; a later range wins where they
    /// overlap (a later setting's highlight nests inside an earlier one's).
    pub ranges: Vec<(Range<usize>, Color)>,
    /// `ChatMessage--highlighted`: the color of the last matching whole-message setting.
    pub whole: Option<Color>,
}

impl Marks {
    /// The color at byte `at`.
    pub fn at(&self, at: usize) -> Option<Color> {
        self.ranges
            .iter()
            .rev()
            .find(|(r, _)| r.contains(&at))
            .map(|(_, c)| *c)
    }
}

/// The compiled highlight settings (tgui-panel `highlightParsers`).
#[derive(Clone, Debug, Default)]
pub struct Highlighter {
    parsers: Vec<(Regex, Color, bool)>,
}

impl Highlighter {
    pub fn new(settings: &[Highlight]) -> Self {
        Self {
            parsers: settings
                .iter()
                .filter(|s| s.enabled)
                .filter_map(|s| Some((compile(s)?, s.color(), s.whole_message)))
                .collect(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.parsers.is_empty()
    }

    /// The highlights of `message`: matched within each segment (tgui-panel matches text nodes,
    /// never across them), never in a radio channel label.
    pub fn marks(&self, message: &ChatMessage) -> Marks {
        let mut marks = Marks::default();
        for (regex, color, whole) in &self.parsers {
            let mut hit = false;
            let mut start = 0;
            for segment in &message.segments {
                let text = segment.text.as_str();
                let skip = if segment.span == Span::Name {
                    RADIO_LABEL.find(text).map_or(0, |m| m.end())
                } else {
                    0
                };
                for m in regex.find_iter(&text[skip..]) {
                    if m.is_empty() {
                        continue;
                    }
                    let at = start + skip;
                    marks.ranges.push((at + m.start()..at + m.end(), *color));
                    hit = true;
                }
                start += text.len();
            }
            if hit && *whole {
                marks.whole = Some(*color);
            }
        }
        marks
    }
}

/// One emote of BandaStation's panel (`custom_emote_panel` entry: its name and what it runs).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CustomEmote {
    pub name: String,
    pub emote: PanelEmote,
}

impl CustomEmote {
    /// `EmotePanel` tooltip.
    pub fn tooltip(&self) -> String {
        match &self.emote {
            PanelEmote::Default { key } => format!("*{key}"),
            PanelEmote::Custom { key, text } => format!("*{key} | \"{text}\""),
            PanelEmote::Me { text } => format!("\"{text}\""),
        }
    }
}

/// All of the panel's settings.
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct ChatPrefs {
    pub pages: ChatPages,
    pub highlights: Vec<Highlight>,
    pub font_size: f32,
    pub line_height: f32,
    pub emotes: Vec<CustomEmote>,
}

impl Default for ChatPrefs {
    fn default() -> Self {
        Self {
            pages: ChatPages::default(),
            // `defaultHighlights`: one empty setting.
            highlights: vec![Highlight::default()],
            font_size: DEFAULT_FONT_SIZE,
            line_height: DEFAULT_LINE_HEIGHT,
            emotes: Vec::new(),
        }
    }
}

pub const KEY_FONT_SIZE: &str = "chat.font_size";
pub const KEY_LINE_HEIGHT: &str = "chat.line_height";
pub const KEY_PAGES: &str = "chat.pages";
pub const KEY_PAGE: &str = "chat.page";
pub const KEY_HIGHLIGHTS: &str = "chat.highlights";
pub const KEY_EMOTES: &str = "chat.emotes";

/// The separators of the encoded lists, and what the settings file trims or splits on.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '%' | '|' | ';' | ' ' | '\n' | '\r' | '\t' | '#' | '=' => {
                out.push_str(&format!("%{:02X}", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}

fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        let hex: String = chars.by_ref().take(2).collect();
        match u8::from_str_radix(&hex, 16) {
            Ok(b) => out.push(char::from(b)),
            Err(_) => {
                out.push('%');
                out.push_str(&hex);
            }
        }
    }
    out
}

fn flag(b: bool) -> &'static str {
    if b { "1" } else { "0" }
}

/// Items of an encoded list, their fields unescaped.
fn records(text: &str) -> impl Iterator<Item = Vec<String>> + '_ {
    text.split('|')
        .filter(|r| !r.is_empty())
        .map(|r| r.split(';').map(unescape).collect())
}

fn encode_pages(pages: &ChatPages) -> String {
    pages
        .pages
        .iter()
        .map(|p| {
            format!(
                "{};{};{}",
                escape(&p.name),
                p.channels.0,
                flag(p.hide_unread)
            )
        })
        .collect::<Vec<_>>()
        .join("|")
}

fn decode_pages(text: &str, current: Option<usize>) -> ChatPages {
    let pages: Vec<ChatPage> = records(text)
        .filter_map(|f| {
            Some(ChatPage {
                name: f.first()?.clone(),
                channels: ChannelSet(f.get(1)?.parse().ok()?),
                hide_unread: f.get(2).is_some_and(|v| v == "1"),
                unread: 0,
            })
        })
        .collect();
    if pages.is_empty() {
        return ChatPages::default();
    }
    let current = current.filter(|&c| c < pages.len()).unwrap_or(0);
    ChatPages { pages, current }
}

fn encode_highlights(highlights: &[Highlight]) -> String {
    highlights
        .iter()
        .map(|h| {
            format!(
                "{};{};{};{};{};{}",
                flag(h.enabled),
                flag(h.whole_message),
                flag(h.match_word),
                flag(h.match_case),
                escape(&h.color),
                escape(&h.text)
            )
        })
        .collect::<Vec<_>>()
        .join("|")
}

fn decode_highlights(text: &str) -> Vec<Highlight> {
    records(text)
        .filter(|f| f.len() == 6)
        .map(|f| Highlight {
            enabled: f[0] == "1",
            whole_message: f[1] == "1",
            match_word: f[2] == "1",
            match_case: f[3] == "1",
            color: f[4].clone(),
            text: f[5].clone(),
        })
        .collect()
}

fn encode_emotes(emotes: &[CustomEmote]) -> String {
    emotes
        .iter()
        .map(|e| {
            let (kind, key, text) = match &e.emote {
                PanelEmote::Default { key } => ("d", key.as_str(), ""),
                PanelEmote::Custom { key, text } => ("c", key.as_str(), text.as_str()),
                PanelEmote::Me { text } => ("m", "", text.as_str()),
            };
            format!(
                "{};{kind};{};{}",
                escape(&e.name),
                escape(key),
                escape(text)
            )
        })
        .collect::<Vec<_>>()
        .join("|")
}

fn decode_emotes(text: &str) -> Vec<CustomEmote> {
    records(text)
        .filter(|f| f.len() == 4 && !f[0].is_empty())
        .filter_map(|f| {
            let (key, text) = (f[2].clone(), f[3].clone());
            let emote = match f[1].as_str() {
                "d" => PanelEmote::Default { key },
                "c" => PanelEmote::Custom { key, text },
                "m" => PanelEmote::Me { text },
                _ => return None,
            };
            Some(CustomEmote {
                name: f[0].clone(),
                emote,
            })
        })
        .take(MAX_EMOTES)
        .collect()
}

impl ChatPrefs {
    /// The `chat.*` settings, as (key, value).
    pub fn to_settings(&self) -> Vec<(&'static str, String)> {
        vec![
            (KEY_FONT_SIZE, self.font_size.to_string()),
            (KEY_LINE_HEIGHT, format!("{:.2}", self.line_height)),
            (KEY_PAGES, encode_pages(&self.pages)),
            (KEY_PAGE, self.pages.current.to_string()),
            (KEY_HIGHLIGHTS, encode_highlights(&self.highlights)),
            (KEY_EMOTES, encode_emotes(&self.emotes)),
        ]
    }

    /// Read back from the settings; a missing or broken value is its default.
    pub fn from_settings(get: impl Fn(&str) -> Option<String>) -> Self {
        let defaults = Self::default();
        let number = |key: &str, min: f32, max: f32, default: f32| {
            get(key)
                .and_then(|v| v.parse::<f32>().ok())
                .filter(|v| v.is_finite())
                .map_or(default, |v| v.clamp(min, max))
        };
        Self {
            pages: get(KEY_PAGES).map_or(defaults.pages, |text| {
                decode_pages(&text, get(KEY_PAGE).and_then(|c| c.parse().ok()))
            }),
            highlights: get(KEY_HIGHLIGHTS)
                .map_or(defaults.highlights, |text| decode_highlights(&text)),
            font_size: number(
                KEY_FONT_SIZE,
                FONT_SIZE_MIN,
                FONT_SIZE_MAX,
                DEFAULT_FONT_SIZE,
            )
            .round(),
            line_height: number(
                KEY_LINE_HEIGHT,
                LINE_HEIGHT_MIN,
                LINE_HEIGHT_MAX,
                DEFAULT_LINE_HEIGHT,
            ),
            emotes: get(KEY_EMOTES).map_or_else(Vec::new, |text| decode_emotes(&text)),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use ss13_shared::chat::ChatSegment;

    use super::*;

    fn hl(text: &str) -> Highlight {
        Highlight {
            text: text.into(),
            ..default()
        }
    }

    fn texts(marks: &Marks, text: &str) -> Vec<String> {
        marks
            .ranges
            .iter()
            .map(|(r, _)| text[r.clone()].to_string())
            .collect()
    }

    fn say(text: &str) -> ChatMessage {
        ChatMessage::simple(ChatChannel::Local, Span::Say, text)
    }

    #[test]
    fn tabs_filter_count_unread_and_move_like_tgui_panel() {
        let mut pages = ChatPages::default();
        pages.add();
        assert_eq!(pages.current, 1);
        let radio = pages.current_mut();
        radio.channels.toggle(ChatChannel::Radio);
        assert!(radio.channels.accepts(ChatChannel::Radio));
        assert!(radio.channels.accepts(ChatChannel::System), "always");
        assert!(!radio.channels.accepts(ChatChannel::Local));
        pages.add();
        pages.current_mut().channels.toggle(ChatChannel::Ooc);

        // On the main tab, following: nothing unread anywhere (it shows everything).
        pages.select(0);
        pages.count_unread(ChatChannel::Radio, true);
        assert!(pages.pages.iter().all(|p| p.unread == 0));
        // Scrolled up: the main tab counts.
        pages.count_unread(ChatChannel::Radio, false);
        assert_eq!(pages.pages[0].unread, 1);
        assert_eq!(pages.pages[1].unread, 0, "the main tab shows it too");
        // On the OOC tab: radio counts on the radio and main tabs.
        pages.select(2);
        pages.count_unread(ChatChannel::Radio, true);
        assert_eq!((pages.pages[0].unread, pages.pages[1].unread), (2, 1));
        assert_eq!(pages.pages[1].label(), "Новая вкладка (1)");
        pages.pages[1].hide_unread = true;
        assert_eq!(pages.pages[1].label(), "Новая вкладка");
        pages.select(1);
        assert_eq!(pages.pages[1].unread, 0, "read when shown");

        // The main tab never moves, nothing moves onto its place.
        pages.move_left();
        assert_eq!(pages.current, 1);
        pages.move_right();
        assert_eq!(pages.current, 2);
        assert!(pages.pages[2].channels.contains(ChatChannel::Radio));
        pages.select(0);
        pages.move_right();
        assert_eq!(pages.current, 0);
        pages.remove_current();
        assert_eq!(pages.pages.len(), 3, "the main tab stays");
        pages.select(2);
        pages.remove_current();
        assert_eq!((pages.pages.len(), pages.current), (2, 0));
    }

    #[test]
    fn highlights_follow_tgui_panel_set_highlight() {
        // Words: case-insensitive, regex signs are literal, one-letter entries are dropped.
        let h = Highlighter::new(&[hl("Капитан, a, c++")]);
        let text = "капитан купил c++ и КАПИТАНА";
        let marks = h.marks(&say(text));
        assert_eq!(texts(&marks, text), ["капитан", "КАПИТАН"]);
        assert_eq!(
            marks.whole,
            Some(parse_color(DEFAULT_HIGHLIGHT_COLOR).unwrap())
        );
        // `c++` is not an allowed plain entry; a word boundary keeps "капитана" out.
        let word = Highlighter::new(&[Highlight {
            match_word: true,
            match_case: true,
            ..hl("капитан")
        }]);
        let marks = word.marks(&say(text));
        assert_eq!(texts(&marks, text), ["капитан"]);
        // `/regex/` entries win over words; a one-character one is refused.
        let re = Highlighter::new(&[hl("боб, /к[оа]т/, /x/")]);
        let text = "кот и кат и боб";
        assert_eq!(texts(&re.marks(&say(text)), text), ["кот", "кат"]);
        assert!(Highlighter::new(&[hl("/x/")]).is_empty());
        assert!(!hl("/(?<=a)b/").is_valid(), "a broken pattern");
        assert!(
            Highlighter::new(&[Highlight {
                enabled: false,
                ..hl("боб")
            }])
            .is_empty()
        );
        // Never across segments, never in a radio label.
        let radio = ChatMessage {
            channel: ChatChannel::Radio,
            segments: vec![
                ChatSegment::new(Span::Name, "[Инженерия] "),
                ChatSegment::new(Span::Name, "Инженерия Боб"),
                ChatSegment::new(Span::Radio, " говорит, \"инже"),
                ChatSegment::new(Span::Radio, "нерия\""),
            ],
        };
        let text = radio.plain_text();
        let marks = Highlighter::new(&[hl("инженерия")]).marks(&radio);
        assert_eq!(texts(&marks, &text), ["Инженерия"]);
        assert_eq!(marks.ranges[0].0.start, "[Инженерия] ".len());
        // A later setting's color wins where both match.
        let two = Highlighter::new(&[
            hl("боб"),
            Highlight {
                color: "#ff0000".into(),
                whole_message: false,
                ..hl("об")
            },
        ]);
        let marks = two.marks(&say("боб"));
        assert_eq!(marks.at(0), parse_color(DEFAULT_HIGHLIGHT_COLOR));
        assert_eq!(marks.at(2), parse_color("#ff0000"));
        assert_eq!(marks.whole, parse_color(DEFAULT_HIGHLIGHT_COLOR));
    }

    #[test]
    fn settings_round_trip_through_the_settings_file() {
        let mut prefs = ChatPrefs {
            font_size: 17.0,
            line_height: 1.55,
            ..ChatPrefs::default()
        };
        prefs.pages.add();
        prefs.pages.current_mut().name = "Рация; | %20 = #1".into();
        prefs
            .pages
            .current_mut()
            .channels
            .toggle(ChatChannel::Radio);
        prefs.pages.current_mut().hide_unread = true;
        prefs.pages.current_mut().unread = 7;
        prefs.highlights = vec![Highlight {
            text: " капитан, /к[оа]т/ ".into(),
            color: "#ff0000".into(),
            match_case: true,
            ..default()
        }];
        prefs.emotes = vec![
            CustomEmote {
                name: "Вздох".into(),
                emote: PanelEmote::Default { key: "sigh".into() },
            },
            CustomEmote {
                name: "Крик".into(),
                emote: PanelEmote::Custom {
                    key: "scream".into(),
                    text: "вопит; громко | очень".into(),
                },
            },
            CustomEmote {
                name: "Нос".into(),
                emote: PanelEmote::Me {
                    text: "чешет нос".into(),
                },
            },
        ];
        // Through the file's own `key = value` text, which trims and splits.
        let file: String = prefs
            .to_settings()
            .into_iter()
            .map(|(k, v)| format!("{k} = {v}\n"))
            .collect();
        let values: HashMap<String, String> = file
            .lines()
            .filter_map(|l| {
                let (k, v) = l.split_once('=')?;
                Some((k.trim().to_owned(), v.trim().to_owned()))
            })
            .collect();
        let back = ChatPrefs::from_settings(|k| values.get(k).cloned());
        prefs.pages.pages[1].unread = 0;
        assert_eq!(back, prefs);

        // Nothing saved: the defaults; broken values: theirs, clamped.
        assert_eq!(ChatPrefs::from_settings(|_| None), ChatPrefs::default());
        let broken = ChatPrefs::from_settings(|k| {
            Some(match k {
                KEY_FONT_SIZE => "99".into(),
                KEY_LINE_HEIGHT => "NaN".into(),
                KEY_PAGE => "5".into(),
                _ => "garbage;;|x".into(),
            })
        });
        assert_eq!(broken.font_size, FONT_SIZE_MAX);
        assert_eq!(broken.line_height, DEFAULT_LINE_HEIGHT);
        assert_eq!(broken.pages, ChatPages::default());
        assert!(broken.highlights.is_empty() && broken.emotes.is_empty());
    }
}
