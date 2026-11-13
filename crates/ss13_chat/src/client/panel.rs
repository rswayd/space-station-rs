//! The docked chat panel: tgui-panel's tabs + message list + "scroll to bottom", with tgui-say's
//! input line under it. The tab bar, the settings pane and the emote panel are in
//! [`super::settings_ui`]; what they set is [`super::prefs::ChatPrefs`].
//!
//! The say line is tgui-say (`tgui/packages/tgui-say`): the channel button and Tab cycle the
//! channels, a radio prefix typed first (`:h `) moves onto the button, Up/Down walk the last
//! five sent lines, and the line tells the server when the player thinks or types
//! ([`TypingTracker`], tg `start_thinking`/`start_typing`).
//!
//! Messages are broken into visual lines once (on arrival, on a combine, on a width change) and
//! every line is one fixed-height [`VirtualList`] row, so only the rows in view exist as
//! entities whatever the history length. A row is drawn without wrapping from the pieces of its
//! line; each piece keeps the span class of the segment it came from.

use std::sync::{Arc, RwLock};

use bevy::ecs::schedule::SystemCondition;
use bevy::input::ButtonState;
use bevy::input::keyboard::{KeyCode, KeyboardInput};
use bevy::input_focus::{FocusedInput, InputFocus};
use bevy::platform::collections::HashMap;
use bevy::text::{
    EditableText, FontCx, FontSize, FontSourceTemplate, FontStyle, FontWeight, LayoutCx,
    TextBackgroundColor,
};
use bevy::ui::ComputedNode;
use ss13_net::admin::client::{AdminResponseReceived, SendAdminRequest};
use ss13_net::chat::client::CloseSayReceived;
use ss13_net::client::SendIntent;
use ss13_shared::IconStateId;
use ss13_shared::admin::rights::R_ADMIN;
use ss13_shared::admin::{AdminRequest, AdminResponse};
use ss13_shared::chat::{MAX_MESSAGE_LEN, Span};
use ss13_shared::protocol::{ClientIntent, TypingState};
use ss13_ui_kit::prelude::*;
use ss13_ui_kit::scroll::{SCROLLBAR_WIDTH, ScrollContent};
use ss13_ui_kit::theme::{self, root_text_style};
use ss13_ui_kit::virtual_list::VirtualListState;

use super::log::{COMBINE_MAX_MESSAGES, ChatEntry, ChatLog};
use super::prefs::{self, ChannelSet, Highlighter};
use super::settings_ui;
use super::style;
use super::wrap::break_lines;

/// tgui-panel default `fontSize: 13`.
pub const FONT_SIZE: f32 = prefs::DEFAULT_FONT_SIZE;
/// `lineHeight: 1.2` of 13 px, rounded up to whole pixels so rows never drift.
pub const LINE_HEIGHT: f32 = 16.0;
/// Docked width. tgui-panel's split is user-resizable; this one is fixed, so lines are broken
/// once and never again (a resize re-breaks every message, see [`ChatView`]).
pub const PANEL_WIDTH: f32 = 480.0;
const ROW_PADDING: f32 = 6.0;
/// Room kept on the last line of a combined message for its `xN` badge.
const BADGE_RESERVE: f32 = 36.0;
/// Rounding of shaped advances differs slightly between this layout and the drawn one.
const WRAP_SLACK: f32 = 2.0;
/// tgui-panel uses 24 px against browser wheel steps of ~100 px; one Bevy wheel line is 20 px,
/// and a single notch up must already stop the view from following new messages.
pub const SCROLL_TRACKING_TOLERANCE: f32 = LINE_HEIGHT / 2.0;

/// One visual line of a message.
#[derive(Debug, PartialEq)]
pub struct ChatLine {
    /// The [`ChatEntry`] it belongs to.
    pub id: u64,
    pub pieces: Vec<(Span, String)>,
    /// The highlight color of each piece (tgui-panel `Chat__highlight`).
    pub marks: Vec<Option<Color>>,
    /// `ChatMessage--highlighted`: the whole message's highlight color.
    pub highlight: Option<Color>,
    pub font_size: f32,
    /// The span the whole message is nested in (a radio line's class), see
    /// [`style::span_style_in`].
    pub outer: Option<Span>,
    /// `xN` on the last line of a combined message.
    pub badge: Option<u32>,
}

/// tgui-panel scroll tracking: follow new messages while the view is at the bottom, keep the
/// view still once the player scrolls up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AutoScroll {
    pub tracking: bool,
    /// Messages that arrived while not tracking.
    pub unread: usize,
    prev_max: f32,
    jump: bool,
}

impl Default for AutoScroll {
    fn default() -> Self {
        Self {
            tracking: true,
            unread: 0,
            prev_max: 0.0,
            jump: false,
        }
    }
}

impl AutoScroll {
    /// The "scroll to bottom" button: track again from the next update.
    pub fn jump_to_bottom(&mut self) {
        self.jump = true;
    }

    /// One frame. `scroll` is the offset the player sees (last frame's content), `content` and
    /// `view` the new content height and the view height, `shifted` the height evicted above
    /// the view since last frame, `appended` the number of new messages. Returns the offset to
    /// apply, if it changes.
    pub fn update(
        &mut self,
        scroll: f32,
        content: f32,
        view: f32,
        shifted: f32,
        appended: usize,
    ) -> Option<f32> {
        if view <= 0.0 {
            return None;
        }
        let max = (content - view).max(0.0);
        let at = (scroll - shifted).max(0.0);
        let prev_max = (self.prev_max - shifted).max(0.0);
        self.tracking = std::mem::take(&mut self.jump) || prev_max - at < SCROLL_TRACKING_TOLERANCE;
        let target = if self.tracking {
            self.unread = 0;
            max
        } else {
            self.unread += appended;
            at.min(max)
        };
        self.prev_max = max;
        (target != scroll).then_some(target)
    }
}

/// What the rows were built for: the shown types, font size, line height and the log's
/// removal count; any change rebuilds every row.
type Built = (ChannelSet, f32, f32, u64);

/// Lines of the shown tab, shared with the list's row builder, and the per-message line cache.
#[derive(Resource)]
pub struct ChatView {
    rows: Arc<RwLock<Vec<Arc<ChatLine>>>>,
    /// Entry id → (`times` the lines were built for, lines).
    cache: HashMap<u64, (u32, Vec<Arc<ChatLine>>)>,
    width: f32,
    filter: ChannelSet,
    font_size: f32,
    line_height: f32,
    highlighter: Highlighter,
    built: Option<Built>,
    /// Id after the newest entry already turned into lines.
    next_id: u64,
    pub autoscroll: AutoScroll,
}

impl Default for ChatView {
    fn default() -> Self {
        Self {
            rows: Arc::default(),
            cache: HashMap::new(),
            width: 0.0,
            filter: ChannelSet::ALL,
            font_size: prefs::DEFAULT_FONT_SIZE,
            line_height: prefs::DEFAULT_LINE_HEIGHT,
            highlighter: Highlighter::default(),
            built: None,
            next_id: 0,
            autoscroll: AutoScroll::default(),
        }
    }
}

impl ChatView {
    /// The lines the list shows, top to bottom.
    pub fn lines(&self) -> Vec<Arc<ChatLine>> {
        self.rows.read().map(|r| r.clone()).unwrap_or_default()
    }

    /// The message types the list shows (the shown tab's).
    pub fn filter(&self) -> ChannelSet {
        self.filter
    }

    /// `changePage`: the list shows these types, from the bottom.
    pub fn set_filter(&mut self, filter: ChannelSet) {
        if self.filter != filter {
            self.filter = filter;
            self.autoscroll.jump_to_bottom();
        }
    }

    /// The chat's font size and line height (tgui-panel `assignStyle`).
    pub fn set_font(&mut self, font_size: f32, line_height: f32) {
        if self.font_size != font_size {
            self.font_size = font_size;
        }
        if self.line_height != line_height {
            self.line_height = line_height;
        }
    }

    /// The highlights of messages from now on (tgui-panel `setHighlight`).
    pub fn set_highlighter(&mut self, highlighter: Highlighter) {
        self.highlighter = highlighter;
    }

    /// `rebuildChat` ("Apply now"): every message is laid out again, with today's highlights.
    pub fn rebuild(&mut self) {
        self.cache.clear();
        self.built = None;
    }

    /// The height of one row.
    pub fn row_height(&self) -> f32 {
        prefs::row_height(self.font_size, self.line_height)
    }
}

/// The panel root.
#[derive(Component, Clone, Debug, Default)]
pub struct ChatPanel;

/// The message list ([`VirtualList`]).
#[derive(Component, Clone, Debug, Default)]
pub struct ChatMessages;

/// The "scroll to bottom" button, shown while the view does not follow new messages.
#[derive(Component, Clone, Debug, Default)]
pub struct JumpButton;

/// The say line ([`Input`]).
#[derive(Component, Clone, Debug, Default)]
pub struct SayInput;

/// The channel button before the say line.
#[derive(Component, Clone, Debug, Default)]
pub struct ChannelButton;

/// tgui-say's channel: what the say line sends. Back to [`Self::Say`] after every message, as
/// tgui-say closes after one: the next line never goes to OOC by accident.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SayChannel {
    #[default]
    Say,
    Radio,
    Whisper,
    Me,
    Ooc,
    Looc,
    LoocWallpierce,
    Pray,
    /// tgui-say `Admin` (asay), opened by its key only (F5) and only by an admin.
    Admin,
}

impl SayChannel {
    /// tgui-say's `ChannelIterator` order (the button, Tab), without its blacklisted `Admin`
    /// and `Mentor`. BandaStation's "LOOC (Wallpierce)" verb has no tgui-say channel; it
    /// follows LOOC here.
    const CYCLE: [SayChannel; 8] = [
        SayChannel::Say,
        SayChannel::Whisper,
        SayChannel::Radio,
        SayChannel::Ooc,
        SayChannel::Looc,
        SayChannel::LoocWallpierce,
        SayChannel::Me,
        SayChannel::Pray,
    ];

    /// `ChannelIterator.next`: a blacklisted channel (admin) stays.
    pub fn next(self) -> Self {
        let Some(i) = Self::CYCLE.iter().position(|c| *c == self) else {
            return self;
        };
        Self::CYCLE[(i + 1) % Self::CYCLE.len()]
    }

    /// `ChannelIterator.isVisible`: not one of tgui-say's `quiet` channels, so the mob shows
    /// the typing indicator.
    pub fn in_character(self) -> bool {
        matches!(
            self,
            SayChannel::Say | SayChannel::Radio | SayChannel::Whisper | SayChannel::Me
        )
    }

    pub fn label(self) -> &'static str {
        match self {
            SayChannel::Say => "Сказать",
            SayChannel::Radio => "Рация",
            SayChannel::Whisper => "Шёпот",
            SayChannel::Me => "Эмоция",
            SayChannel::Ooc => "OOC",
            SayChannel::Looc => "LOOC",
            SayChannel::LoocWallpierce => "LOOC (сквозь стены)",
            SayChannel::Pray => "Молитва",
            SayChannel::Admin => "Админ",
        }
    }

    /// tg `tgui_say/delegate_speech`: the intent of a line sent in this channel; `None` for
    /// admin chat, which goes as an [`AdminRequest::Asay`].
    pub fn intent(self, text: &str) -> Option<ClientIntent> {
        let text = text.to_string();
        Some(match self {
            SayChannel::Say => ClientIntent::Say(text),
            SayChannel::Radio => ClientIntent::Say(format!("; {text}")),
            SayChannel::Whisper => ClientIntent::Say(format!("#{text}")),
            SayChannel::Me => ClientIntent::Say(format!("*me {text}")),
            SayChannel::Ooc => ClientIntent::Ooc(text),
            SayChannel::Looc => ClientIntent::Looc {
                text,
                wall_pierce: false,
            },
            SayChannel::LoocWallpierce => ClientIntent::Looc {
                text,
                wall_pierce: true,
            },
            SayChannel::Pray => ClientIntent::Pray(text),
            SayChannel::Admin => return None,
        })
    }
}

/// tgui-say `RADIO_PREFIXES`: a prefix typed first moves onto the channel button.
pub const RADIO_PREFIXES: [(&str, &str); 30] = [
    (":a ", "Рой"),
    (":b ", "Бин"),
    (":c ", "Ком"),
    (":e ", "Инж"),
    (":g ", "Генка"),
    (":m ", "Мед"),
    (":n ", "Иссл"),
    (":o ", "ИИ"),
    (":p ", "Развл"),
    (":s ", "Безоп"),
    (":l ", "Юр"),
    (":t ", "Синд"),
    (":u ", "Снаб"),
    (":v ", "Обсл"),
    (":y ", "ЦК"),
    (":ф ", "Рой"),
    (":и ", "Бин"),
    (":с ", "Ком"),
    (":у ", "Инж"),
    (":п ", "Генка"),
    (":ь ", "Мед"),
    (":т ", "Иссл"),
    (":щ ", "ИИ"),
    (":з ", "Развл"),
    (":ы ", "Безоп"),
    (":д ", "Юр"),
    (":е ", "Синд"),
    (":г ", "Снаб"),
    (":м ", "Обсл"),
    (":н ", "ЦК"),
];

/// The binary prefixes: speech to silicons shows no indicator (tgui-say sends `thinking` off).
const BINARY_PREFIXES: [&str; 2] = [":b ", ":и "];

/// tgui-say `getPrefix`: the radio prefix `value` starts with (`/^[:.][\wА-Яа-яёЁ]\s/`, the
/// dot read as a colon, case-insensitive), as its [`RADIO_PREFIXES`] key.
pub fn radio_prefix(value: &str) -> Option<&'static str> {
    let mut chars = value.chars();
    let (mark, key, space) = (chars.next()?, chars.next()?, chars.next()?);
    if !matches!(mark, ':' | '.') || space != ' ' {
        return None;
    }
    let wanted: String = std::iter::once(':')
        .chain(key.to_lowercase())
        .chain([' '])
        .collect();
    RADIO_PREFIXES
        .iter()
        .find(|(prefix, _)| *prefix == wanted)
        .map(|&(prefix, _)| prefix)
}

/// tgui-say `ChatHistory`: the last five sent lines, newest first, walked with Up/Down; the
/// draft is kept while walking.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SayHistory {
    messages: Vec<String>,
    /// `-1` in tgui-say: at the draft, not on a sent line.
    index: Option<usize>,
    temp: Option<String>,
}

impl SayHistory {
    const MAX: usize = 5;

    pub fn add(&mut self, message: &str) {
        self.messages.insert(0, message.to_string());
        self.messages.truncate(Self::MAX);
        self.index = None;
    }

    /// The button's number while on a sent line (1 = the newest).
    pub fn position(&self) -> Option<usize> {
        self.index.map(|i| i + 1)
    }

    pub fn is_at_latest(&self) -> bool {
        self.index.is_none()
    }

    pub fn older(&mut self) -> Option<&str> {
        let next = self.index.map_or(0, |i| i + 1);
        if next >= self.messages.len() {
            return None;
        }
        self.index = Some(next);
        Some(&self.messages[next])
    }

    pub fn newer(&mut self) -> Option<&str> {
        match self.index {
            None | Some(0) => {
                self.index = None;
                None
            }
            Some(i) => {
                self.index = Some(i - 1);
                Some(&self.messages[i - 1])
            }
        }
    }

    pub fn save_temp(&mut self, draft: &str) {
        self.temp = Some(draft.to_string());
    }

    pub fn take_temp(&mut self) -> Option<String> {
        self.temp.take()
    }

    pub fn reset(&mut self) {
        self.index = None;
        self.temp = None;
    }
}

/// The say line's state besides its text and channel.
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct SayLine {
    /// tgui-say `currentPrefix`: a [`RADIO_PREFIXES`] key taken off the text.
    pub prefix: Option<&'static str>,
    pub history: SayHistory,
}

impl SayLine {
    /// The channel button's text (tgui-say `buttonContent`).
    pub fn button_label(&self, channel: SayChannel) -> String {
        if let Some(n) = self.history.position() {
            return n.to_string();
        }
        if let Some(prefix) = self.prefix
            && let Some((_, name)) = RADIO_PREFIXES.iter().find(|(p, _)| *p == prefix)
        {
            return (*name).to_string();
        }
        channel.label().to_string()
    }

    /// Whether the mob shows the typing indicator for what is being written.
    pub fn visible(&self, channel: SayChannel) -> bool {
        channel.in_character() && !self.prefix.is_some_and(|p| BINARY_PREFIXES.contains(&p))
    }

    /// tgui-say `handleEnter`'s entry: the prefix before the text in the say channel.
    pub fn compose(&self, channel: SayChannel, text: &str) -> String {
        match self.prefix {
            Some(prefix) if channel == SayChannel::Say && !text.is_empty() => {
                format!("{prefix}{text}")
            }
            _ => text.to_string(),
        }
    }

    /// tgui-say `handleClose`'s reset.
    fn reset(&mut self) {
        self.prefix = None;
        self.history.reset();
    }
}

/// tg `stop_typing` timer: typing falls back to thinking 5 s after the last key.
pub const TYPING_TIMEOUT_SECS: f64 = 5.0;

/// tgui-say `saveText` throttle.
pub const DRAFT_INTERVAL_SECS: f64 = 1.0;

/// What the say line last told the server ([`ClientIntent::Typing`], [`ClientIntent::SayDraft`]).
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct TypingTracker {
    sent: TypingState,
    last_key: Option<f64>,
    draft: String,
    draft_at: Option<f64>,
}

impl TypingTracker {
    /// A key changed the text of a visible line (tgui-say `typingMsg`).
    pub fn edited(&mut self, now: f64) {
        self.last_key = Some(now);
    }

    /// The state to send now, when it differs from the last one sent: off while the line is
    /// closed or in a quiet channel, typing within [`TYPING_TIMEOUT_SECS`] of a key, else
    /// thinking.
    pub fn update(&mut self, open: bool, visible: bool, now: f64) -> Option<TypingState> {
        if !open {
            self.last_key = None;
        }
        let state = if !open || !visible {
            TypingState::Off
        } else if self.last_key.is_some_and(|t| now - t < TYPING_TIMEOUT_SECS) {
            TypingState::Typing
        } else {
            TypingState::Thinking
        };
        (state != self.sent).then(|| {
            self.sent = state;
            state
        })
    }

    /// The draft to send now (tgui-say `saveText`, at most once per [`DRAFT_INTERVAL_SECS`],
    /// the latest text winning), when it differs from the last one sent. A closed line forgets
    /// it (the server drops it with the indicator).
    pub fn draft(&mut self, open: bool, text: &str, now: f64) -> Option<String> {
        if !open {
            self.draft.clear();
            return None;
        }
        if text == self.draft || self.draft_at.is_some_and(|t| now - t < DRAFT_INTERVAL_SECS) {
            return None;
        }
        self.draft = text.to_string();
        self.draft_at = Some(now);
        Some(self.draft.clone())
    }
}

/// The say line's text field (the [`SayInput`]'s `EditableText` child).
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct SayField(pub Option<Entity>);

/// The client's admin rights (`AdminResponse::Rights`), for the admin channel's key.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct AdminRights(pub u32);

/// tg's `/datum/keybinding/client/communication` defaults (with BandaStation's edits), and
/// BandaStation's `admin_say` (F5).
pub const CHANNEL_KEYS: [(KeyCode, SayChannel); 9] = [
    (KeyCode::KeyT, SayChannel::Say),
    (KeyCode::F3, SayChannel::Say),
    (KeyCode::KeyY, SayChannel::Radio),
    (KeyCode::KeyO, SayChannel::Ooc),
    (KeyCode::F2, SayChannel::Ooc),
    (KeyCode::KeyM, SayChannel::Me),
    (KeyCode::KeyP, SayChannel::Pray),
    (KeyCode::KeyL, SayChannel::Looc),
    (KeyCode::F5, SayChannel::Admin),
];

pub(crate) fn plugin(app: &mut App) {
    app.init_resource::<ChatView>()
        .init_resource::<SayChannel>()
        .init_resource::<SayLine>()
        .init_resource::<TypingTracker>()
        .init_resource::<SayField>()
        .init_resource::<AdminRights>()
        .add_message::<AdminResponseReceived>()
        .add_message::<SendAdminRequest>()
        .add_message::<CloseSayReceived>()
        .add_systems(Startup, spawn_panel)
        .add_systems(
            Update,
            (
                settings_ui::read_sliders,
                settings_ui::apply_prefs,
                sync_view,
                settings_ui::count_unread,
                settings_ui::sync_tabs,
                settings_ui::sync_panes,
                show_jump_button,
                attach_say_keys,
                close_say,
                admin_rights,
                channel_keys,
                show_channel
                    .run_if(resource_changed::<SayChannel>.or_else(resource_changed::<SayLine>)),
                send_typing,
            )
                .chain(),
        )
        .add_observer(on_act)
        .add_observer(on_say_submitted)
        .add_observer(on_say_changed);
    settings_ui::plugin(app);
}

/// `TextFont` of a span, as a scene patch (`TextFont` holds asset handles and is a template).
fn span_font(span: Span, outer: Option<Span>, size: f32, mark: Option<Color>) -> impl Scene {
    let style = style::span_style_in(span, outer);
    let weight = if style.bold {
        FontWeight::BOLD
    } else {
        FontWeight::NORMAL
    };
    let color = mark.unwrap_or(style.color);
    let family = FontSourceTemplate::Family(theme::FONT_FAMILY.into());
    let slant = if style.italic {
        FontStyle::Italic
    } else {
        FontStyle::Normal
    };
    bsn! {
        TextFont {
            font: family,
            font_size: FontSize::Px(size),
            weight: weight,
            style: slant,
        }
        TextColor(color)
    }
}

/// An emoji of a chat line ([`Span::Emoji`]): the game's renderer gives the node its
/// `ImageNode` from the icon atlas.
#[derive(Component, Clone, Debug, Default, PartialEq)]
#[require(Node)]
pub struct ChatEmoji {
    pub icon_state: Option<IconStateId>,
}

/// A run of a row: its text pieces, or one emoji; a link's run is a button whose click is
/// `link:<id>` ([`on_act`]).
fn run_scene(
    texts: Vec<(Span, String, Option<Color>)>,
    emoji: Option<IconStateId>,
    link: Option<u32>,
    outer: Option<Span>,
    size: f32,
) -> impl Scene {
    let spans: Vec<_> = texts
        .into_iter()
        .map(|(span, text, mark)| {
            let shine = super::donor::shine_patch(span);
            // `.Chat__highlight`: the highlight color over a 15% wash of it.
            let wash = mark.map(|c| bsn! { TextBackgroundColor(c.with_alpha(0.15)) });
            bsn! { TextSpan(text) @span_font(span, outer, size, mark) @{shine} @{wash} }
        })
        .collect();
    let text = (!spans.is_empty()).then(|| {
        bsn! {
            Children [
                Text("") @span_font(Span::Plain, None, size, None) TextLayout::no_wrap()
                Children [ {spans} ]
            ]
        }
    });
    let emoji_size = (size * 1.2).ceil();
    let icon = emoji.map(|id| {
        bsn! {
            Children [
                Node { width: px(emoji_size), height: px(emoji_size) }
                ChatEmoji { icon_state: {Some(id)} }
            ]
        }
    });
    let link = link.map(|id| {
        let action = format!("link:{id}");
        bsn! { bevy::ui_widgets::Button Act(action) }
    });
    bsn! {
        Node { align_items: AlignItems::Center }
        @{text}
        @{icon}
        @{link}
    }
}

fn row_scene(line: Option<Arc<ChatLine>>) -> impl Scene {
    let size = line.as_ref().map_or(FONT_SIZE, |l| l.font_size);
    let whole = line.as_ref().and_then(|l| l.highlight);
    let (pieces, badge) = line.map_or((Vec::new(), None), |line| {
        let outer = line.outer;
        let mut runs = Vec::new();
        let mut texts = Vec::new();
        for (i, (span, text)) in line.pieces.iter().enumerate() {
            let mark = line.marks.get(i).copied().flatten();
            match *span {
                // A whole `:name:`; half of one broken over two lines stays text.
                Span::Emoji(id)
                    if text.len() > 2 && text.starts_with(':') && text.ends_with(':') =>
                {
                    if !texts.is_empty() {
                        runs.push(run_scene(
                            std::mem::take(&mut texts),
                            None,
                            None,
                            outer,
                            size,
                        ));
                    }
                    runs.push(run_scene(Vec::new(), Some(id), None, outer, size));
                }
                Span::Link(id) => {
                    if !texts.is_empty() {
                        runs.push(run_scene(
                            std::mem::take(&mut texts),
                            None,
                            None,
                            outer,
                            size,
                        ));
                    }
                    runs.push(run_scene(
                        vec![(*span, text.clone(), mark)],
                        None,
                        Some(id),
                        outer,
                        size,
                    ));
                }
                _ => texts.push((*span, text.clone(), mark)),
            }
        }
        if !texts.is_empty() {
            runs.push(run_scene(texts, None, None, outer, size));
        }
        (runs, line.badge)
    });
    // `.ChatMessage--highlighted`: an 8% wash and a bar on the left.
    let whole = whole.map(|c| {
        bsn! {
            Node { border: UiRect::left(px(2)) }
            BackgroundColor(c.with_alpha(0.08))
            BorderColor::all(c)
        }
    });
    let badge = badge.map(|times| {
        let label = format!("x{times}");
        bsn! {
            Children [
                Node {
                    margin: UiRect::left(px(4)),
                    padding: UiRect::axes(px(size * 0.3), px(size * 0.1)),
                    border_radius: BorderRadius::all(px(10)),
                }
                BackgroundColor(style::BADGE_BACKGROUND)
                Children [
                    Text(label) @span_font(Span::Plain, None, size * 0.7, Some(Color::WHITE)) TextLayout::no_wrap()
                ]
            ]
        }
    });
    bsn! {
        Node {
            height: percent(100),
            padding: UiRect::horizontal(px(ROW_PADDING)),
            align_items: AlignItems::Center,
            overflow: Overflow::clip(),
        }
        Children [ {pieces} ]
        @{badge}
        @{whole}
    }
}

fn spawn_panel(view: Res<ChatView>, mut commands: Commands) {
    let rows = view.rows.clone();
    let row = VirtualRow::new(move |index| {
        let line = rows.read().ok().and_then(|r| r.get(index).cloned());
        row_scene(line)
    });
    let max_length = Some(MAX_MESSAGE_LEN);
    let row_height = view.row_height();
    commands.spawn_scene(bsn! {
        Node {
            position_type: PositionType::Absolute,
            top: px(0),
            right: px(0),
            bottom: px(0),
            width: px(PANEL_WIDTH),
            flex_direction: FlexDirection::Column,
            border: UiRect::left(px(theme::BORDER_TINY)),
        }
        ChatPanel
        ActScope
        BackgroundColor(style::PANEL_BACKGROUND)
        BorderColor::all(theme::COLOR_BORDER_DARK)
        Children [
            @{settings_ui::top_bar()}
            --
            @{settings_ui::emotes_root()}
            --
            @{settings_ui::settings_root()}
            --
            Node { flex_grow: 1.0, min_height: px(0), flex_direction: FlexDirection::Column }
            Children [
                @VirtualList { @item_height: row_height, @row: row }
                VirtualList { len: 0 }
                ChatMessages
                Node { flex_grow: 1.0 }
                --
                @Button { @icon: "arrow-down", @content: "Прокрутить вниз" }
                Act("scroll_bottom")
                JumpButton
                Node { position_type: PositionType::Absolute, right: px(24), bottom: px(12), display: Display::None }
                @root_text_style()
            ]
            --
            Node {
                flex_shrink: 0.0,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: px(theme::SPACE_M),
                padding: UiRect::all(px(theme::SPACE_M)),
                border: UiRect::top(px(theme::BORDER_TINY)),
            }
            BorderColor::all(theme::COLOR_BORDER_DARK)
            @root_text_style()
            Children [
                @Button { @content: "Сказать" }
                Act("cycle_channel")
                ChannelButton
                --
                Input {
                    placeholder: "Сообщение... (Enter — отправить, Esc — очистить)",
                    max_length: max_length,
                    self_clear: true,
                    fluid: true,
                }
                SayInput
                Node { flex_grow: 1.0 }
            ]
        ]
    });
}

fn wrap_entry(
    entry: &ChatEntry,
    fonts: Option<(&mut FontCx, &mut LayoutCx)>,
    width: f32,
    font_size: f32,
    highlighter: &Highlighter,
) -> Vec<Arc<ChatLine>> {
    let segments = &entry.message.segments;
    let text = entry.message.plain_text();
    let width = if entry.times > 1 {
        width - BADGE_RESERVE * font_size / FONT_SIZE
    } else {
        width
    };
    let marks = if highlighter.is_empty() {
        prefs::Marks::default()
    } else {
        highlighter.marks(&entry.message)
    };
    let ranges = break_lines(fonts, segments, &text, theme::FONT_FAMILY, font_size, width);
    let last = ranges.len() - 1;
    ranges
        .into_iter()
        .enumerate()
        .map(|(i, range)| {
            let mut pieces = Vec::new();
            let mut piece_marks = Vec::new();
            let mut start = 0;
            for segment in segments {
                let (a, b) = (
                    range.start.max(start),
                    range.end.min(start + segment.text.len()),
                );
                start += segment.text.len();
                // Cut where a highlight starts or ends, so each piece has one color.
                let mut cut = a;
                while cut < b {
                    let next = marks
                        .ranges
                        .iter()
                        .flat_map(|(r, _)| [r.start, r.end])
                        .filter(|&p| p > cut && p < b)
                        .min()
                        .unwrap_or(b);
                    pieces.push((segment.span, text[cut..next].to_string()));
                    piece_marks.push(marks.at(cut));
                    cut = next;
                }
            }
            Arc::new(ChatLine {
                id: entry.id,
                pieces,
                marks: piece_marks,
                highlight: marks.whole,
                font_size,
                outer: style::outer_span(&entry.message),
                badge: (i == last && entry.times > 1).then_some(entry.times),
            })
        })
        .collect()
}

/// Rebuilds the shown lines when messages, the tab or the width change, then scrolls.
///
/// Cost per change: one parley layout per new or combined message and a rebuild of the last few
/// messages' lines; rendered rows are respawned only when an index changes what it shows
/// (eviction, combine, tab switch), not for appends. Width and tab changes rebuild everything.
fn sync_view(
    log: Res<ChatLog>,
    mut view: ResMut<ChatView>,
    mut font_cx: Option<ResMut<FontCx>>,
    mut layout_cx: Option<ResMut<LayoutCx>>,
    mut lists: Query<
        (
            &mut VirtualList,
            &mut VirtualListState,
            &ComputedNode,
            &Children,
        ),
        With<ChatMessages>,
    >,
    mut contents: Query<(&mut ScrollPosition, &ComputedNode), With<ScrollContent>>,
    mut commands: Commands,
) {
    let Ok((mut list, mut state, node, children)) = lists.single_mut() else {
        return;
    };
    let width =
        node.size.x * node.inverse_scale_factor - SCROLLBAR_WIDTH - 2.0 * ROW_PADDING - WRAP_SLACK;
    if width <= 0.0 {
        // Not laid out yet.
        return;
    }
    let view = &mut *view;
    let mut shifted = 0;
    let mut appended = 0;
    let resized = (width - view.width).abs() > 0.5;
    let wanted = (
        view.filter,
        view.font_size,
        view.line_height,
        log.removals(),
    );
    let refont = view.built.is_some_and(|b| b.1 != view.font_size);
    let full = resized || view.built != Some(wanted);
    let row_height = view.row_height();
    if log.is_changed() || full {
        if resized || refont {
            // shortcut: re-breaks every message (~2500 layouts) on a width or font size change;
            // fine for a fixed-width panel and a whole-pixel font slider, debounce it if the
            // panel becomes resizable.
            view.cache.clear();
            view.width = width;
        }
        if full {
            state.item_height = row_height;
        }
        let ChatView {
            rows,
            cache,
            filter,
            font_size,
            highlighter,
            next_id,
            ..
        } = view;
        let Ok(mut rows) = rows.write() else {
            return;
        };
        let entries = log.entries();
        let first = entries.front().map_or(u64::MAX, |e| e.id);
        if full {
            rows.clear();
            *next_id = 0;
        } else {
            shifted = rows.partition_point(|l| l.id < first);
            if shifted > 0 {
                rows.drain(..shifted);
                cache.retain(|id, _| *id >= first);
            }
        }
        // Rebuilt from the oldest entry a repeat may have combined into, or the oldest new one.
        let new_from = usize::try_from(next_id.saturating_sub(first)).unwrap_or(usize::MAX);
        let from = new_from
            .min(entries.len().saturating_sub(COMBINE_MAX_MESSAGES))
            .min(entries.len());
        let from_id = entries.get(from).map_or(u64::MAX, |e| e.id);
        let cut = rows.partition_point(|l| l.id < from_id);
        let old_tail = rows.split_off(cut);
        let mut fonts = font_cx.as_deref_mut().zip(layout_cx.as_deref_mut());
        for entry in entries.range(from..) {
            if !filter.accepts(entry.message.channel) {
                continue;
            }
            let cached = cache.entry(entry.id).or_insert((0, Vec::new()));
            if cached.0 != entry.times {
                let fonts = fonts.as_mut().map(|(f, l)| (&mut **f, &mut **l));
                *cached = (
                    entry.times,
                    wrap_entry(entry, fonts, width, *font_size, highlighter),
                );
            }
            rows.extend(cached.1.iter().cloned());
            if !full && entry.id >= *next_id {
                appended += 1;
            }
        }
        *next_id = entries.back().map_or(*next_id, |e| e.id + 1);
        let tail_kept = rows.len() >= cut + old_tail.len()
            && old_tail
                .iter()
                .zip(&rows[cut..])
                .all(|(a, b)| Arc::ptr_eq(a, b));
        // Rows are keyed by index: anything but an append changes what an index shows.
        if full || shifted > 0 || !tail_kept {
            for (_, row) in std::mem::take(&mut state.rendered) {
                commands.entity(row).try_despawn();
            }
        }
        list.len = rows.len();
        drop(rows);
        view.built = Some(wanted);
    }
    for &child in children {
        if let Ok((mut scroll, content)) = contents.get_mut(child) {
            let view_height = content.size.y * content.inverse_scale_factor;
            let content_height = list.len as f32 * row_height;
            if let Some(y) = view.autoscroll.update(
                scroll.y,
                content_height,
                view_height,
                shifted as f32 * row_height,
                appended,
            ) {
                scroll.y = y;
            }
        }
    }
}

fn show_jump_button(
    view: Res<ChatView>,
    mut buttons: Query<(&mut Node, &Children), With<JumpButton>>,
    mut texts: Query<&mut Text>,
) {
    let scroll = view.autoscroll;
    for (mut node, children) in &mut buttons {
        let display = if scroll.tracking {
            Display::None
        } else {
            Display::Flex
        };
        if node.display != display {
            node.display = display;
        }
        let label = if scroll.unread > 0 {
            format!("Новые сообщения: {}", scroll.unread)
        } else {
            "Прокрутить вниз".to_string()
        };
        // The button's own text is its direct `Text` child; the icon sits one level deeper.
        for &child in children {
            if let Ok(mut text) = texts.get_mut(child)
                && text.0 != label
            {
                text.0.clone_from(&label);
            }
        }
    }
}

fn on_act(
    act: On<UiAct>,
    panels: Query<(), With<ChatPanel>>,
    mut view: ResMut<ChatView>,
    mut channel: ResMut<SayChannel>,
    mut line: ResMut<SayLine>,
    mut intents: MessageWriter<SendIntent>,
) {
    if !panels.contains(act.scope) {
        return;
    }
    match act.action.as_str() {
        "scroll_bottom" => view.autoscroll.jump_to_bottom(),
        // tgui-say `handleIncrementChannel`.
        "cycle_channel" => {
            *channel = channel.next();
            line.prefix = None;
        }
        action if action.starts_with("link:") => {
            if let Ok(id) = action["link:".len()..].parse() {
                intents.write(SendIntent(ClientIntent::ChatLink(id)));
            }
        }
        _ => {}
    }
}

/// tgui-say `handleEnter`: the line goes out (a prefix before it in the say channel), into the
/// history, and the line closes.
fn on_say_submitted(
    submitted: On<InputSubmitted>,
    inputs: Query<(), With<SayInput>>,
    mut intents: MessageWriter<SendIntent>,
    mut admin: MessageWriter<SendAdminRequest>,
    mut channel: ResMut<SayChannel>,
    mut line: ResMut<SayLine>,
    mut focus: ResMut<InputFocus>,
) {
    if !inputs.contains(submitted.entity) {
        return;
    }
    // The server checks everything again; this only avoids sending nothing.
    let text = submitted.value.trim();
    if !text.is_empty() {
        line.history.add(&submitted.value);
        let text = line.compose(*channel, text);
        match channel.intent(&text) {
            Some(intent) => {
                intents.write(SendIntent(intent));
            }
            None => {
                admin.write(SendAdminRequest(AdminRequest::Asay(text)));
            }
        }
    }
    *channel = SayChannel::Say;
    line.reset();
    focus.clear();
}

/// The say line's text field gets its keys (an entity observer runs after the text widget's
/// global ones, so its own edits win).
fn attach_say_keys(
    fields: Query<(Entity, &ChildOf), Added<EditableText>>,
    inputs: Query<(), With<SayInput>>,
    mut field: ResMut<SayField>,
    mut commands: Commands,
) {
    for (entity, parent) in &fields {
        if inputs.contains(parent.parent()) {
            field.0 = Some(entity);
            commands.entity(entity).observe(on_say_key);
        }
    }
}

/// Replaces the text of the focused field (the edit is not reported as typed).
fn set_line(editable: &mut EditableText, input: &mut Input, text: &str) {
    input.value = text.to_string();
    editable.clear();
    editable.editor_mut().set_text(text);
    editable.queue_edit(bevy::text::TextEdit::TextEnd(false));
}

/// tgui-say `handleKeyDown`: Up/Down walk the history, Backspace/Delete leave it or drop an
/// empty line's prefix, Tab cycles the channel, Escape drops the draft and closes the line.
fn on_say_key(
    mut key: On<FocusedInput<KeyboardInput>>,
    mut fields: Query<(&mut EditableText, &ChildOf)>,
    mut inputs: Query<&mut Input, With<SayInput>>,
    mut focus: ResMut<InputFocus>,
    mut channel: ResMut<SayChannel>,
    mut line: ResMut<SayLine>,
) {
    if key.input.state != ButtonState::Pressed {
        return;
    }
    let Ok((mut editable, parent)) = fields.get_mut(key.focused_entity) else {
        return;
    };
    let Ok(mut input) = inputs.get_mut(parent.parent()) else {
        return;
    };
    if editable.is_composing() {
        return;
    }
    match key.input.key_code {
        KeyCode::ArrowUp => {
            let value = input.value.clone();
            if line.history.is_at_latest() && !value.is_empty() {
                line.history.save_temp(&value);
            }
            if let Some(older) = line.history.older().map(str::to_string) {
                set_line(&mut editable, &mut input, &older);
            }
        }
        // tgui-say also empties a line never walked back; here Down only walks forward.
        KeyCode::ArrowDown if !line.history.is_at_latest() => {
            let newer = line
                .history
                .newer()
                .map(str::to_string)
                .or_else(|| line.history.take_temp())
                .unwrap_or_default();
            set_line(&mut editable, &mut input, &newer);
        }
        KeyCode::Backspace | KeyCode::Delete => {
            if !line.history.is_at_latest() {
                line.history.reset();
            } else if line.prefix.is_some() && *channel == SayChannel::Say && input.value.is_empty()
            {
                line.prefix = None;
            }
            return;
        }
        KeyCode::Tab => {
            *channel = channel.next();
            line.prefix = None;
        }
        KeyCode::Escape => {
            input.value.clear();
            editable.clear();
            focus.clear();
            *channel = SayChannel::Say;
            line.reset();
        }
        _ => return,
    }
    key.propagate(false);
}

/// tgui-say `handleInput`: a radio prefix typed first moves onto the button (the say channel
/// again), and a key in a visible channel is typing.
fn on_say_changed(
    changed: On<InputChanged>,
    mut inputs: Query<&mut Input, With<SayInput>>,
    mut fields: Query<&mut EditableText>,
    field: Res<SayField>,
    mut channel: ResMut<SayChannel>,
    mut line: ResMut<SayLine>,
    mut typing: ResMut<TypingTracker>,
    time: Res<Time>,
) {
    let Ok(mut input) = inputs.get_mut(changed.entity) else {
        return;
    };
    if let Some(prefix) = radio_prefix(&changed.value)
        && line.prefix != Some(prefix)
    {
        line.prefix = Some(prefix);
        *channel = SayChannel::Say;
        let rest = changed
            .value
            .char_indices()
            .nth(3)
            .map_or("", |(i, _)| &changed.value[i..]);
        if let Some(mut editable) = field.0.and_then(|f| fields.get_mut(f).ok()) {
            set_line(&mut editable, &mut input, rest);
        }
    }
    if line.visible(*channel) {
        typing.edited(time.elapsed_secs_f64());
    }
}

/// The client's admin rights, for the admin channel's key.
fn admin_rights(
    mut responses: MessageReader<AdminResponseReceived>,
    mut rights: ResMut<AdminRights>,
) {
    for AdminResponseReceived(response) in responses.read() {
        if let AdminResponse::Rights { rights: r, .. } = response {
            rights.0 = *r;
        }
    }
}

/// tgui-say `open`/`close`/`thinking`: the line's state to the server, on a change only.
fn send_typing(
    focus: Res<InputFocus>,
    field: Res<SayField>,
    channel: Res<SayChannel>,
    line: Res<SayLine>,
    inputs: Query<&Input, With<SayInput>>,
    time: Res<Time>,
    mut typing: ResMut<TypingTracker>,
    mut intents: MessageWriter<SendIntent>,
) {
    let now = time.elapsed_secs_f64();
    let open = field.0.is_some() && focus.get() == field.0;
    let visible = line.visible(*channel);
    // tgui-say `save` (`handle_entry`: only say and radio text is kept; whisper here too, as a
    // forced whisper keeps its channel): the text as it would be said.
    let draft = inputs
        .iter()
        .next()
        .filter(|_| visible && *channel != SayChannel::Me)
        .map(|input| line.compose(*channel, input.value.trim()))
        .and_then(|text| match channel.intent(&text) {
            Some(ClientIntent::Say(said)) if !text.is_empty() => Some(said),
            _ => None,
        })
        .unwrap_or_default();
    if let Some(draft) = typing.draft(open, &draft, now) {
        intents.write(SendIntent(ClientIntent::SayDraft(draft)));
    }
    if let Some(state) = typing.update(open, visible, now) {
        intents.write(SendIntent(ClientIntent::Typing(state)));
    }
}

/// tgui-say `close` from the server (a forced say): the line drops its text and closes.
fn close_say(
    mut closes: MessageReader<CloseSayReceived>,
    field: Res<SayField>,
    mut fields: Query<&mut EditableText>,
    mut inputs: Query<&mut Input, With<SayInput>>,
    mut focus: ResMut<InputFocus>,
    mut channel: ResMut<SayChannel>,
    mut line: ResMut<SayLine>,
) {
    if closes.read().count() == 0 {
        return;
    }
    for mut input in &mut inputs {
        input.value.clear();
    }
    if let Some(mut editable) = field.0.and_then(|f| fields.get_mut(f).ok()) {
        editable.clear();
    }
    if focus.get().is_some() && focus.get() == field.0 {
        focus.clear();
    }
    *channel = SayChannel::Say;
    line.reset();
}

/// tg `keybinding/client/communication`: the key picks the channel and opens the say line.
fn channel_keys(
    keys: Res<ButtonInput<KeyCode>>,
    mut focus: ResMut<InputFocus>,
    fields: Query<(Entity, &ChildOf), With<EditableText>>,
    inputs: Query<(), With<SayInput>>,
    mut channel: ResMut<SayChannel>,
    mut line: ResMut<SayLine>,
    rights: Res<AdminRights>,
) {
    if focus.get().is_some()
        || keys.any_pressed([
            KeyCode::ShiftLeft,
            KeyCode::ShiftRight,
            KeyCode::ControlLeft,
            KeyCode::ControlRight,
            KeyCode::AltLeft,
            KeyCode::AltRight,
        ])
    {
        return;
    }
    let Some(picked) = CHANNEL_KEYS
        .into_iter()
        .find_map(|(key, picked)| keys.just_pressed(key).then_some(picked))
    else {
        return;
    };
    // `required_permissions = R_ADMIN`.
    if picked == SayChannel::Admin && rights.0 & R_ADMIN == 0 {
        return;
    }
    let Some((field, _)) = fields.iter().find(|(_, p)| inputs.contains(p.parent())) else {
        return;
    };
    *channel = picked;
    line.prefix = None;
    focus.set(field, bevy::input_focus::FocusCause::Navigated);
}

fn show_channel(
    channel: Res<SayChannel>,
    line: Res<SayLine>,
    buttons: Query<&Children, With<ChannelButton>>,
    mut texts: Query<&mut Text>,
) {
    let label = line.button_label(*channel);
    for children in &buttons {
        for &child in children {
            if let Ok(mut text) = texts.get_mut(child)
                && text.0 != label
            {
                text.0.clone_from(&label);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIEW: f32 = 160.0;

    #[test]
    fn channels_send_their_intents() {
        let say = |c: SayChannel, text: &str| c.intent(text);
        assert_eq!(
            say(SayChannel::Say, "да"),
            Some(ClientIntent::Say("да".into()))
        );
        assert_eq!(
            say(SayChannel::Radio, "да"),
            Some(ClientIntent::Say("; да".into()))
        );
        assert_eq!(
            say(SayChannel::Whisper, "да"),
            Some(ClientIntent::Say("#да".into()))
        );
        assert_eq!(
            say(SayChannel::Me, "кивает"),
            Some(ClientIntent::Say("*me кивает".into()))
        );
        assert_eq!(
            say(SayChannel::Ooc, "да"),
            Some(ClientIntent::Ooc("да".into()))
        );
        assert_eq!(
            say(SayChannel::LoocWallpierce, "да"),
            Some(ClientIntent::Looc {
                text: "да".into(),
                wall_pierce: true
            })
        );
        assert_eq!(
            say(SayChannel::Admin, "да"),
            None,
            "asay is an admin request"
        );
        // tgui-say's order, every channel once, back to say; admin chat is never cycled into
        // nor out of.
        let mut c = SayChannel::Say;
        let mut seen = Vec::new();
        for _ in 0..8 {
            c = c.next();
            assert_ne!(c, SayChannel::Admin);
            seen.push(c);
        }
        assert_eq!(c, SayChannel::Say);
        assert_eq!(
            &seen[..3],
            [SayChannel::Whisper, SayChannel::Radio, SayChannel::Ooc]
        );
        assert_eq!(SayChannel::Pray.next(), SayChannel::Say);
        assert_eq!(SayChannel::Admin.next(), SayChannel::Admin);
    }

    #[test]
    fn radio_prefixes_are_read_like_tgui_say() {
        assert_eq!(radio_prefix(":h привет"), None, "no :h in tgui-say's list");
        assert_eq!(radio_prefix(":e привет"), Some(":e "));
        assert_eq!(radio_prefix(".E привет"), Some(":e "));
        assert_eq!(radio_prefix(":у привет"), Some(":у "));
        assert_eq!(radio_prefix(":У привет"), Some(":у "));
        assert_eq!(radio_prefix(":e"), None);
        assert_eq!(radio_prefix(":eпривет"), None);
        assert_eq!(radio_prefix("e: привет"), None);
        let mut line = SayLine::default();
        assert!(line.visible(SayChannel::Say));
        assert!(!line.visible(SayChannel::Ooc));
        assert!(!line.visible(SayChannel::Looc));
        assert!(!line.visible(SayChannel::Pray));
        line.prefix = Some(":и ");
        assert!(
            !line.visible(SayChannel::Say),
            "binary speech shows no bubble"
        );
        assert_eq!(line.button_label(SayChannel::Say), "Бин");
    }

    #[test]
    fn history_keeps_five_lines_and_the_draft() {
        let mut h = SayHistory::default();
        assert_eq!(h.older(), None);
        for i in 1..=6 {
            h.add(&format!("l{i}"));
        }
        h.save_temp("черновик");
        assert_eq!(h.older(), Some("l6"));
        assert_eq!(h.position(), Some(1));
        for want in ["l5", "l4", "l3", "l2"] {
            assert_eq!(h.older(), Some(want));
        }
        assert_eq!(h.older(), None, "five kept: l1 is gone");
        assert_eq!(h.position(), Some(5));
        assert_eq!(h.newer(), Some("l3"));
        h.newer();
        h.newer();
        assert_eq!(h.newer(), Some("l6"));
        assert_eq!(h.newer(), None);
        assert!(h.is_at_latest());
        assert_eq!(h.take_temp().as_deref(), Some("черновик"));
    }

    #[test]
    fn drafts_are_throttled_and_forgotten_on_close() {
        let mut t = TypingTracker::default();
        assert_eq!(t.draft(true, "", 0.0), None, "nothing typed, nothing sent");
        assert_eq!(t.draft(true, "п", 0.1), Some("п".into()));
        assert_eq!(t.draft(true, "пр", 0.5), None, "within a second");
        assert_eq!(
            t.draft(true, "при", 1.1),
            Some("при".into()),
            "the latest text"
        );
        assert_eq!(t.draft(true, "при", 3.0), None, "unchanged");
        assert_eq!(t.draft(false, "при", 3.5), None);
        assert_eq!(
            t.draft(true, "при", 4.0),
            Some("при".into()),
            "reopened: sent again"
        );
        let line = SayLine {
            prefix: Some(":e "),
            ..default()
        };
        assert_eq!(line.compose(SayChannel::Say, "да"), ":e да");
        assert_eq!(line.compose(SayChannel::Say, ""), "");
    }

    #[test]
    fn typing_state_is_sent_only_on_a_change() {
        let mut t = TypingTracker::default();
        assert_eq!(
            t.update(false, true, 0.0),
            None,
            "closed: off, already sent"
        );
        assert_eq!(t.update(true, true, 1.0), Some(TypingState::Thinking));
        assert_eq!(t.update(true, true, 1.1), None);
        t.edited(2.0);
        assert_eq!(t.update(true, true, 2.0), Some(TypingState::Typing));
        t.edited(3.0);
        assert_eq!(t.update(true, true, 3.0), None, "more keys: no resend");
        assert_eq!(t.update(true, true, 7.9), None);
        assert_eq!(
            t.update(true, true, 8.0),
            Some(TypingState::Thinking),
            "5 s without a key"
        );
        // A quiet channel (OOC) is off; back in say, thinking.
        assert_eq!(t.update(true, false, 8.1), Some(TypingState::Off));
        assert_eq!(t.update(true, true, 8.2), Some(TypingState::Thinking));
        t.edited(9.0);
        assert_eq!(t.update(true, true, 9.0), Some(TypingState::Typing));
        // Sent or closed: off, and the next opening thinks again.
        assert_eq!(t.update(false, true, 9.5), Some(TypingState::Off));
        assert_eq!(t.update(true, true, 10.0), Some(TypingState::Thinking));
    }

    #[test]
    fn follows_bottom_until_scrolled_up_then_holds_and_counts() {
        let mut s = AutoScroll::default();
        // Short content: no scrolling needed.
        assert_eq!(s.update(0.0, 100.0, VIEW, 0.0, 1), None);
        assert!(s.tracking);
        // Content grows past the view: follow it.
        assert_eq!(s.update(0.0, 400.0, VIEW, 0.0, 1), Some(240.0));
        assert_eq!(s.update(240.0, 416.0, VIEW, 0.0, 1), Some(256.0));
        // The player scrolls up one wheel line: stop following, count new messages.
        assert_eq!(s.update(236.0, 432.0, VIEW, 0.0, 1), None);
        assert!(!s.tracking);
        assert_eq!(s.unread, 1);
        assert_eq!(s.update(236.0, 448.0, VIEW, 0.0, 2), None);
        assert_eq!(s.unread, 3);
        // Eviction above the view: the same text stays in view.
        assert_eq!(s.update(236.0, 432.0, VIEW, 32.0, 0), Some(204.0));
        assert!(!s.tracking);
        // Back at the bottom by hand: follow again.
        assert_eq!(s.update(272.0, 448.0, VIEW, 0.0, 1), Some(288.0));
        assert!(s.tracking);
        assert_eq!(s.unread, 0);
        // Far up, then the button.
        assert_eq!(s.update(0.0, 448.0, VIEW, 0.0, 0), None);
        assert!(!s.tracking);
        s.jump_to_bottom();
        assert_eq!(s.update(0.0, 448.0, VIEW, 0.0, 0), Some(288.0));
        assert!(s.tracking);
        // Not laid out yet: nothing is decided.
        assert_eq!(s.update(0.0, 448.0, 0.0, 0.0, 0), None);
        assert!(s.tracking);
    }
}
