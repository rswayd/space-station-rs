//! The docked chat panel: tgui-panel's tabs + message list + "scroll to bottom", with tgui-say's
//! input line under it.
//!
//! Messages are broken into visual lines once (on arrival, on a combine, on a width change) and
//! every line is one fixed-height [`VirtualList`] row, so only the rows in view exist as
//! entities whatever the history length. A row is drawn without wrapping from the pieces of its
//! line; each piece keeps the span class of the segment it came from.

use std::sync::{Arc, RwLock};

use bevy::input::ButtonState;
use bevy::input::keyboard::{KeyCode, KeyboardInput};
use bevy::input_focus::{FocusedInput, InputFocus};
use bevy::platform::collections::HashMap;
use bevy::text::{
    EditableText, FontCx, FontSize, FontSourceTemplate, FontStyle, FontWeight, LayoutCx,
};
use bevy::ui::ComputedNode;
use ss13_net::client::SendIntent;
use ss13_shared::IconStateId;
use ss13_shared::chat::{ChatChannel, MAX_MESSAGE_LEN, Span};
use ss13_shared::protocol::ClientIntent;
use ss13_ui_kit::prelude::*;
use ss13_ui_kit::scroll::{SCROLLBAR_WIDTH, ScrollContent};
use ss13_ui_kit::theme::{self, root_text_style};
use ss13_ui_kit::virtual_list::VirtualListState;

use super::log::{COMBINE_MAX_MESSAGES, ChatEntry, ChatLog};
use super::style;
use super::wrap::break_lines;

/// tgui-panel default `fontSize: 13`.
pub const FONT_SIZE: f32 = 13.0;
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

/// Which messages the list shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChatTab {
    #[default]
    All,
    System,
}

impl ChatTab {
    fn shows(self, channel: ChatChannel) -> bool {
        match self {
            ChatTab::All => true,
            ChatTab::System => channel == ChatChannel::System,
        }
    }
}

/// One visual line of a message.
#[derive(Debug, PartialEq)]
pub struct ChatLine {
    /// The [`ChatEntry`] it belongs to.
    pub id: u64,
    pub pieces: Vec<(Span, String)>,
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

/// Lines of the shown tab, shared with the list's row builder, and the per-message line cache.
#[derive(Resource)]
pub struct ChatView {
    rows: Arc<RwLock<Vec<Arc<ChatLine>>>>,
    /// Entry id → (`times` the lines were built for, lines).
    cache: HashMap<u64, (u32, Vec<Arc<ChatLine>>)>,
    width: f32,
    tab: ChatTab,
    built_tab: Option<ChatTab>,
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
            tab: ChatTab::All,
            built_tab: None,
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

    pub fn tab(&self) -> ChatTab {
        self.tab
    }

    pub fn set_tab(&mut self, tab: ChatTab) {
        if self.tab != tab {
            self.tab = tab;
            self.autoscroll.jump_to_bottom();
        }
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

pub(crate) fn plugin(app: &mut App) {
    app.init_resource::<ChatView>()
        .add_systems(Startup, spawn_panel)
        .add_systems(Update, (sync_view, show_jump_button).chain())
        .add_observer(on_act)
        .add_observer(on_say_submitted)
        .add_observer(on_say_escape);
}

/// `TextFont` of a span, as a scene patch (`TextFont` holds asset handles and is a template).
fn span_font(span: Span, outer: Option<Span>, size: f32) -> impl Scene {
    let style = style::span_style_in(span, outer);
    let weight = if style.bold {
        FontWeight::BOLD
    } else {
        FontWeight::NORMAL
    };
    let color = style.color;
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

/// A run of a row: its text pieces, or one emoji.
fn run_scene(
    texts: Vec<(Span, String)>,
    emoji: Option<IconStateId>,
    outer: Option<Span>,
) -> impl Scene {
    let spans: Vec<_> = texts
        .into_iter()
        .map(|(span, text)| bsn! { TextSpan(text) @span_font(span, outer, FONT_SIZE) })
        .collect();
    let text = (!spans.is_empty()).then(|| {
        bsn! {
            Children [
                Text("") @span_font(Span::Plain, None, FONT_SIZE) TextLayout::no_wrap()
                Children [ {spans} ]
            ]
        }
    });
    let icon = emoji.map(|id| {
        bsn! {
            Children [
                Node { width: px(LINE_HEIGHT), height: px(LINE_HEIGHT) }
                ChatEmoji { icon_state: {Some(id)} }
            ]
        }
    });
    bsn! {
        Node { align_items: AlignItems::Center }
        @{text}
        @{icon}
    }
}

fn row_scene(line: Option<Arc<ChatLine>>) -> impl Scene {
    let (pieces, badge) = line.map_or((Vec::new(), None), |line| {
        let outer = line.outer;
        let mut runs = Vec::new();
        let mut texts = Vec::new();
        for (span, text) in &line.pieces {
            match *span {
                // A whole `:name:`; half of one broken over two lines stays text.
                Span::Emoji(id)
                    if text.len() > 2 && text.starts_with(':') && text.ends_with(':') =>
                {
                    if !texts.is_empty() {
                        runs.push(run_scene(std::mem::take(&mut texts), None, outer));
                    }
                    runs.push(run_scene(Vec::new(), Some(id), outer));
                }
                _ => texts.push((*span, text.clone())),
            }
        }
        if !texts.is_empty() {
            runs.push(run_scene(texts, None, outer));
        }
        (runs, line.badge)
    });
    let badge = badge.map(|times| {
        let label = format!("x{times}");
        bsn! {
            Children [
                Node {
                    margin: UiRect::left(px(4)),
                    padding: UiRect::axes(px(FONT_SIZE * 0.3), px(FONT_SIZE * 0.1)),
                    border_radius: BorderRadius::all(px(10)),
                }
                BackgroundColor(style::BADGE_BACKGROUND)
                Children [
                    Text(label) @span_font(Span::Plain, None, FONT_SIZE * 0.7) TextColor(Color::WHITE) TextLayout::no_wrap()
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
    }
}

fn spawn_panel(view: Res<ChatView>, mut commands: Commands) {
    let rows = view.rows.clone();
    let row = VirtualRow::new(move |index| {
        let line = rows.read().ok().and_then(|r| r.get(index).cloned());
        row_scene(line)
    });
    let max_length = Some(MAX_MESSAGE_LEN);
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
            Node { flex_shrink: 0.0 }
            @root_text_style()
            Children [
                @Tabs
                Children [
                    @TabsTab { @icon: "list", @content: "Все" } TabsTab { selected: true } Act("tab:all")
                    --
                    @TabsTab { @icon: "cog", @content: "Система" } Act("tab:system")
                ]
            ]
            --
            Node { flex_grow: 1.0, min_height: px(0), flex_direction: FlexDirection::Column }
            Children [
                @VirtualList { @item_height: LINE_HEIGHT, @row: row }
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
                Text("Сказать")
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
) -> Vec<Arc<ChatLine>> {
    let segments = &entry.message.segments;
    let text = entry.message.plain_text();
    let width = if entry.times > 1 {
        width - BADGE_RESERVE
    } else {
        width
    };
    let ranges = break_lines(fonts, segments, &text, theme::FONT_FAMILY, FONT_SIZE, width);
    let last = ranges.len() - 1;
    ranges
        .into_iter()
        .enumerate()
        .map(|(i, range)| {
            let mut pieces = Vec::new();
            let mut start = 0;
            for segment in segments {
                let (a, b) = (
                    range.start.max(start),
                    range.end.min(start + segment.text.len()),
                );
                start += segment.text.len();
                if a < b {
                    pieces.push((segment.span, text[a..b].to_string()));
                }
            }
            Arc::new(ChatLine {
                id: entry.id,
                pieces,
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
    let full = resized || view.built_tab != Some(view.tab);
    if log.is_changed() || full {
        if resized {
            // shortcut: re-breaks every message (~2500 layouts) on a width change; fine for a
            // fixed-width panel, debounce it if the panel becomes resizable.
            view.cache.clear();
            view.width = width;
        }
        let ChatView {
            rows,
            cache,
            tab,
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
            if !tab.shows(entry.message.channel) {
                continue;
            }
            let cached = cache.entry(entry.id).or_insert((0, Vec::new()));
            if cached.0 != entry.times {
                let fonts = fonts.as_mut().map(|(f, l)| (&mut **f, &mut **l));
                *cached = (entry.times, wrap_entry(entry, fonts, width));
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
        view.built_tab = Some(view.tab);
    }
    for &child in children {
        if let Ok((mut scroll, content)) = contents.get_mut(child) {
            let view_height = content.size.y * content.inverse_scale_factor;
            let content_height = list.len as f32 * LINE_HEIGHT;
            if let Some(y) = view.autoscroll.update(
                scroll.y,
                content_height,
                view_height,
                shifted as f32 * LINE_HEIGHT,
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
    mut tabs: Query<(&mut TabsTab, &Act)>,
    mut view: ResMut<ChatView>,
) {
    if !panels.contains(act.scope) {
        return;
    }
    match act.action.as_str() {
        "scroll_bottom" => view.autoscroll.jump_to_bottom(),
        action => {
            let tab = match action {
                "tab:all" => ChatTab::All,
                "tab:system" => ChatTab::System,
                _ => return,
            };
            view.set_tab(tab);
            for (mut t, a) in &mut tabs {
                let selected = a.0 == action;
                if a.0.starts_with("tab:") && t.selected != selected {
                    t.selected = selected;
                }
            }
        }
    }
}

fn on_say_submitted(
    submitted: On<InputSubmitted>,
    inputs: Query<(), With<SayInput>>,
    mut intents: MessageWriter<SendIntent>,
) {
    if !inputs.contains(submitted.entity) {
        return;
    }
    // The server checks everything again; this only avoids sending nothing.
    let text = submitted.value.trim();
    if !text.is_empty() {
        intents.write(SendIntent(ClientIntent::Say(text.to_string())));
    }
}

/// tgui-say: Escape drops the draft and leaves the field.
fn on_say_escape(
    key: On<FocusedInput<KeyboardInput>>,
    mut fields: Query<(&mut EditableText, &ChildOf)>,
    mut inputs: Query<&mut Input, With<SayInput>>,
    mut focus: ResMut<InputFocus>,
) {
    if key.input.state != ButtonState::Pressed || key.input.key_code != KeyCode::Escape {
        return;
    }
    let Ok((mut editable, parent)) = fields.get_mut(key.focused_entity) else {
        return;
    };
    let Ok(mut input) = inputs.get_mut(parent.parent()) else {
        return;
    };
    input.value.clear();
    editable.clear();
    focus.clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIEW: f32 = 160.0;

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
