//! The panel's tab bar, settings pane and BandaStation's emote panel (tgui-panel `ChatTabs`,
//! `SettingsPanel`/`SettingsGeneral`/`ChatPageSettings`/`TextHighlight`, `emotes/EmotePanel`).
//!
//! Each part is rebuilt only when what it shows changes structurally (a click that adds,
//! removes or toggles something); sliders and text fields write the settings in place, so a
//! drag or a key never rebuilds the field under the pointer or the cursor.

use bevy::app::Propagate;
use bevy::picking::events::PointerClick;
use bevy::picking::pointer::PointerButton;
use bevy::scene::EntityScene;
use ss13_net::client::SendIntent;
use ss13_shared::chat::{ChatChannel, ChatMessage, Span};
use ss13_shared::protocol::{ClientIntent, PanelEmote};
use ss13_tgui::slider::{LocalSlider, slider_with};
use ss13_ui_kit::prelude::*;
use ss13_ui_kit::theme::{self, UiColor};

use super::history;
use super::log::ChatLog;
use super::panel::{ChatPanel, ChatView};
use super::prefs::{
    self, CustomEmote, MAX_EMOTE_LENGTH, MAX_EMOTE_NAME_LENGTH, MAX_EMOTES, PAGE_CHANNELS,
    WARN_AFTER_HIGHLIGHT_AMT,
};
use crate::emote_names;

/// "Save chat log": the page to write (`ss13_game` puts it beside the client settings).
#[derive(Message, Clone, Debug, PartialEq)]
pub struct ChatExported {
    pub file_name: String,
    pub html: String,
}

/// tgui-panel `SETTINGS_TABS` the port has.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SettingsTab {
    #[default]
    General,
    Page,
    Highlights,
}

/// What kind of panel emote the form makes (BandaStation's "Какую эмоцию добавить в панель?").
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EmoteKind {
    /// "Обычная".
    #[default]
    Default,
    /// "С кастомным текстом".
    Custom,
    /// "*me".
    Me,
}

/// The add/edit form of the emote panel: BandaStation's chain of `tgui_alert`/`tgui_input_*`
/// prompts (`emotes/create`, `emotes/contextAction`) as one inline form.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EmoteForm {
    /// The emote being edited (right click), or a new one.
    pub editing: Option<usize>,
    pub kind: EmoteKind,
    pub name: String,
    pub key: String,
    pub text: String,
    /// The filter of the emote list (`tgui_input_list`'s search).
    pub search: String,
}

impl EmoteForm {
    pub fn edit(index: usize, emote: &CustomEmote) -> Self {
        let (kind, key, text) = match &emote.emote {
            PanelEmote::Default { key } => (EmoteKind::Default, key.clone(), String::new()),
            PanelEmote::Custom { key, text } => (EmoteKind::Custom, key.clone(), text.clone()),
            PanelEmote::Me { text } => (EmoteKind::Me, String::new(), text.clone()),
        };
        Self {
            editing: Some(index),
            kind,
            name: emote.name.clone(),
            key,
            text,
            search: String::new(),
        }
    }

    /// The emote the form describes, with BandaStation's checks and warnings. The key is picked
    /// from [`emote_names::NAMES`], as BandaStation's `all_emotes`; whether the mob may run it
    /// is checked by the server when it runs.
    pub fn build(&self, emotes: &[CustomEmote]) -> Result<CustomEmote, String> {
        const CANCEL: &str = "Добавление эмоции отменено.";
        if self.editing.is_none() && emotes.len() >= MAX_EMOTES {
            return Err(format!("Достигнут максимум эмоций: {MAX_EMOTES}."));
        }
        let key = self.key.trim().trim_start_matches('*').to_lowercase();
        let text = self.text.trim().to_owned();
        let needs_key = self.kind != EmoteKind::Me;
        if needs_key && key.is_empty() {
            return Err(CANCEL.into());
        }
        if needs_key && emote_names::name_of(&key).is_none() {
            return Err(format!("Эмоция {key} не существует!"));
        }
        if self.kind != EmoteKind::Default
            && (text.is_empty() || text.chars().count() > MAX_EMOTE_LENGTH)
        {
            return Err(CANCEL.into());
        }
        let name = match self.name.trim() {
            "" if self.kind == EmoteKind::Me => text.chars().take(MAX_EMOTE_NAME_LENGTH).collect(),
            "" => emote_names::name_of(&key).unwrap_or(&key).to_owned(),
            name => name.to_owned(),
        };
        if name.chars().count() > MAX_EMOTE_NAME_LENGTH {
            return Err(CANCEL.into());
        }
        if emotes
            .iter()
            .enumerate()
            .any(|(i, e)| Some(i) != self.editing && e.name == name)
        {
            return Err(format!("Эмоция \"{name}\" уже существует!"));
        }
        let emote = match self.kind {
            EmoteKind::Default => PanelEmote::Default { key },
            EmoteKind::Custom => PanelEmote::Custom { key, text },
            EmoteKind::Me => PanelEmote::Me { text },
        };
        Ok(CustomEmote { name, emote })
    }
}

/// The panel's view state besides the settings themselves.
#[derive(Resource, Debug, Default)]
pub struct PanelUi {
    pub settings_open: bool,
    pub settings_tab: SettingsTab,
    pub emotes_open: bool,
    pub emote_form: Option<EmoteForm>,
    /// The settings pane / emote panel are rebuilt on the next frame.
    pub settings_dirty: bool,
    pub emotes_dirty: bool,
    /// `Button.Confirm` of "Clear chat": the first click asks.
    pub confirm_clear: bool,
}

/// The tab bar's tabs live here.
#[derive(Component, Clone, Debug, Default)]
pub struct PageTabs;

/// A tab of the bar: its page.
#[derive(Component, Clone, Debug, Default)]
pub struct PageTab(pub usize);

/// The cog button.
#[derive(Component, Clone, Debug, Default)]
pub struct SettingsToggle;

/// The emote panel button.
#[derive(Component, Clone, Debug, Default)]
pub struct EmotesToggle;

/// The settings pane.
#[derive(Component, Clone, Debug, Default)]
pub struct SettingsRoot;

/// The emote panel.
#[derive(Component, Clone, Debug, Default)]
pub struct EmotesRoot;

/// An emote button of the panel: its index in [`prefs::ChatPrefs::emotes`].
#[derive(Component, Clone, Debug, Default)]
pub struct EmoteButton(pub usize);

/// What a text field of the pane or the emote form edits.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub enum PrefInput {
    #[default]
    PageName,
    HighlightText(usize),
    HighlightColor(usize),
    EmoteName,
    EmoteSearch,
    EmoteText,
}

/// An emote of the form's list: its key.
#[derive(Component, Clone, Debug, Default)]
pub struct EmotePick(pub &'static str);

pub(crate) fn plugin(app: &mut App) {
    ss13_tgui::slider::plugin(app);
    app.init_resource::<PanelUi>()
        .init_resource::<prefs::ChatPrefs>()
        .add_message::<ChatExported>()
        .add_observer(on_settings_act)
        .add_observer(on_pref_input)
        .add_observer(on_emote_right_click)
        .add_observer(on_emote_pick);
}

/// The top bar's tabs and buttons, put in the panel by [`super::panel`].
pub(crate) fn top_bar() -> impl Scene {
    bsn! {
        Node {
            flex_shrink: 0.0,
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: px(theme::SPACE_XS),
            padding: UiRect::right(px(theme::SPACE_M)),
        }
        BackgroundColor(theme::COLOR_SECTION)
        @theme::root_text_style()
        Children [
            Node { flex_grow: 1.0, min_width: px(0), overflow: Overflow::clip() }
            PageTabs
            --
            @Button { @icon: "plus", @tooltip: "Новая вкладка" }
            Button { color: UiColor::Transparent }
            Act("page_add")
            --
            @Button { @icon: "table-list", @tooltip: "Статпанель" }
            Button { color: UiColor::Grey }
            Act({super::stat_dock::TOGGLE_ACT.to_owned()})
            super::stat_dock::StatToggle
            --
            @Button { @icon: "face-grin-beam", @tooltip: "Панель эмоций" }
            Button { color: UiColor::Grey }
            Act("emotes")
            EmotesToggle
            --
            @Button { @icon: "cog", @tooltip: "Открыть настройки" }
            Act("settings")
            SettingsToggle
        ]
    }
}

/// The emote panel, hidden until its button opens it.
pub(crate) fn emotes_root() -> impl Scene {
    bsn! {
        Node {
            flex_shrink: 0.0,
            display: Display::None,
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::Wrap,
            align_items: AlignItems::Center,
            padding: UiRect::all(px(theme::SPACE_M)),
            border: UiRect::bottom(px(theme::BORDER_TINY)),
        }
        BorderColor::all(theme::COLOR_BORDER_DARK)
        @theme::root_text_style()
        EmotesRoot
    }
}

/// The settings pane, hidden until the cog opens it.
pub(crate) fn settings_root() -> impl Scene {
    bsn! {
        Node {
            flex_shrink: 0.0,
            display: Display::None,
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(px(theme::SPACE_M)),
            row_gap: px(theme::SPACE_M),
            border: UiRect::bottom(px(theme::BORDER_TINY)),
        }
        BorderColor::all(theme::COLOR_BORDER_DARK)
        @theme::root_text_style()
        SettingsRoot
    }
}

fn boxed(scene: impl Scene) -> Box<dyn Scene> {
    Box::new(scene)
}

/// `Button.Checkbox`.
fn checkbox(label: &str, checked: bool, act: String) -> Box<dyn Scene> {
    let icon = if checked {
        "check-square-o"
    } else {
        "square-o"
    };
    let label = label.to_owned();
    boxed(bsn! {
        @Button { @content: label, @icon: icon }
        Button { selected: checked }
        Act(act)
    })
}

fn label(text: &str) -> Box<dyn Scene> {
    let text = text.to_owned();
    let color = UiColor::Label.text().unwrap_or(theme::COLOR_TEXT);
    boxed(bsn! {
        Node { flex_shrink: 0.0 }
        Propagate::<TextColor>(TextColor(color))
        Children [ Text(text) ]
    })
}

fn row(children: Vec<Box<dyn Scene>>) -> Box<dyn Scene> {
    boxed(bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::Wrap,
            align_items: AlignItems::Center,
            column_gap: px(theme::SPACE_M),
        }
        Children [ {children} ]
    })
}

/// A labelled slider row: the label column, then the slider taking the rest.
fn slider_row(
    text: &str,
    key: &str,
    value: f32,
    min: f32,
    max: f32,
    decimals: u8,
) -> Box<dyn Scene> {
    let slider = EntityScene(slider_with(key.to_owned(), value, min, max, Some(decimals)));
    let text = text.to_owned();
    let color = UiColor::Label.text().unwrap_or(theme::COLOR_TEXT);
    boxed(bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: px(8),
        }
        Children [
            Node { width: px(170), flex_shrink: 0.0 }
            Propagate::<TextColor>(TextColor(color))
            Children [ Text(text) ]
            --
            {slider}
        ]
    })
}

fn input(value: String, field: PrefInput, max: usize, placeholder: &str) -> Box<dyn Scene> {
    let placeholder = placeholder.to_owned();
    let max_length = Some(max);
    boxed(bsn! {
        Input { value: value, max_length: max_length, placeholder: placeholder, fluid: true }
        Node { flex_grow: 1.0, flex_basis: px(0), min_width: px(80) }
        field
    })
}

/// `SettingsGeneral`: font size, line height, saving and clearing the log.
fn general(prefs: &prefs::ChatPrefs, ui: &PanelUi) -> Vec<Box<dyn Scene>> {
    let clear = if ui.confirm_clear {
        "Точно очистить?"
    } else {
        "Очистить чат"
    };
    let clear = clear.to_owned();
    vec![
        slider_row(
            "Размер шрифта",
            prefs::KEY_FONT_SIZE,
            prefs.font_size,
            prefs::FONT_SIZE_MIN,
            prefs::FONT_SIZE_MAX,
            0,
        ),
        slider_row(
            "Межстрочный интервал",
            prefs::KEY_LINE_HEIGHT,
            prefs.line_height,
            prefs::LINE_HEIGHT_MIN,
            prefs::LINE_HEIGHT_MAX,
            2,
        ),
        row(vec![
            boxed(bsn! {
                @Button { @icon: "save", @content: "Сохранить лог чата", @tooltip: "Экспорт истории текущей вкладки в HTML-файл" }
                Act("export")
            }),
            boxed(bsn! {
                @Button { @icon: "trash", @content: clear, @tooltip: "Стереть историю текущей вкладки" }
                Button { color: UiColor::Red }
                Act("clear")
            }),
        ]),
    ]
}

/// `ChatPageSettings`: the shown tab's name, order, mute, removal and message types.
fn page(prefs: &prefs::ChatPrefs) -> Vec<Box<dyn Scene>> {
    let pages = &prefs.pages;
    let page = pages.current();
    let main = pages.current == 0;
    let mut head = Vec::new();
    if !main {
        head.push(boxed(bsn! {
            @Button { @icon: "angles-left", @tooltip: "Сдвинуть вкладку влево" }
            Button { color: UiColor::Blue }
            Act("page_left")
        }));
    }
    head.push(input(
        page.name.clone(),
        PrefInput::PageName,
        32,
        "Название",
    ));
    if !main {
        head.push(boxed(bsn! {
            @Button { @icon: "angles-right", @tooltip: "Сдвинуть вкладку вправо" }
            Button { color: UiColor::Blue }
            Act("page_right")
        }));
    }
    head.push(checkbox(
        "Без счётчика",
        page.hide_unread,
        "page_mute".into(),
    ));
    if !main {
        head.push(boxed(bsn! {
            @Button { @icon: "times", @content: "Удалить" }
            Button { color: UiColor::Red }
            Act("page_remove")
        }));
    }
    let types = |admin: bool| -> Vec<Box<dyn Scene>> {
        PAGE_CHANNELS
            .iter()
            .enumerate()
            .filter(|(_, (_, _, a))| *a == admin)
            .map(|(i, (channel, name, _))| {
                checkbox(
                    name,
                    page.channels.contains(*channel),
                    format!("channel:{i}"),
                )
            })
            .collect()
    };
    vec![
        row(head),
        label("Показывать сообщения:"),
        row(types(false)),
        label("Админские:"),
        row(types(true)),
    ]
}

/// `TextHighlightSettings`.
fn highlights(prefs: &prefs::ChatPrefs) -> Vec<Box<dyn Scene>> {
    let mut out = Vec::new();
    for (i, h) in prefs.highlights.iter().enumerate() {
        let swatch = h.color();
        let color_field = PrefInput::HighlightColor(i);
        let color_value = h.color.clone();
        let remove = format!("hl:{i}:remove");
        let text_field = input(
            h.text.clone(),
            PrefInput::HighlightText(i),
            1024,
            "Слова для подсветки через запятую: слово1, слово2; /regex/",
        );
        let invalid = if h.is_valid() {
            Color::NONE
        } else {
            Color::srgb(1.0, 0.0, 0.0)
        };
        out.push(row(vec![
            checkbox("Вкл.", h.enabled, format!("hl:{i}:enabled")),
            boxed(bsn! {
                Node { width: px(14), height: px(14), flex_shrink: 0.0 }
                BackgroundColor(swatch)
            }),
            boxed(bsn! {
                Input { value: color_value, max_length: {Some(7)} }
                Node { width: px(70), flex_shrink: 0.0 }
                color_field
            }),
            checkbox("Всё сообщение", h.whole_message, format!("hl:{i}:whole")),
            checkbox("Слово целиком", h.match_word, format!("hl:{i}:word")),
            checkbox("Регистр", h.match_case, format!("hl:{i}:case")),
            boxed(bsn! {
                @Button { @icon: "times", @content: "Удалить" }
                Button { color: UiColor::Transparent }
                Act(remove)
            }),
        ]));
        out.push(boxed(bsn! {
            Node { width: percent(100), border: UiRect::all(px(theme::BORDER_TINY)) }
            BorderColor::all(invalid)
            Children [ @{text_field} ]
        }));
    }
    let mut add = vec![boxed(bsn! {
        @Button { @icon: "plus", @content: "Добавить подсветку" }
        Button { color: UiColor::Transparent }
        Act("hl_add")
    })];
    if prefs.highlights.len() >= WARN_AFTER_HIGHLIGHT_AMT {
        add.push(boxed(bsn! {
            Node
            Propagate::<TextColor>(TextColor(Color::srgb(1.0, 0.3, 0.3)))
            Children [ Text("Много подсветок может замедлять чат!") ]
        }));
    }
    out.push(row(add));
    out.push(row(vec![
        boxed(bsn! {
            @Button { @icon: "check", @content: "Применить сейчас" }
            Act("hl_apply")
        }),
        label("Подсветка новых сообщений меняется сразу; эта кнопка перерисует весь чат."),
    ]));
    out
}

fn settings_scene(prefs: &prefs::ChatPrefs, ui: &PanelUi) -> impl Scene + use<> {
    let tab = |name: &str, which: SettingsTab, act: &str| {
        let name = name.to_owned();
        let selected = ui.settings_tab == which;
        let act = act.to_owned();
        bsn! { @TabsTab { @content: name } TabsTab { selected: selected } Act(act) }
    };
    let tabs = vec![
        tab("Основное", SettingsTab::General, "set:general"),
        tab("Вкладка", SettingsTab::Page, "set:page"),
        tab("Подсветка", SettingsTab::Highlights, "set:highlights"),
    ];
    let content = match ui.settings_tab {
        SettingsTab::General => general(prefs, ui),
        SettingsTab::Page => page(prefs),
        SettingsTab::Highlights => highlights(prefs),
    };
    bsn! {
        Node { flex_direction: FlexDirection::Column, row_gap: px(theme::SPACE_M) }
        Children [
            @Tabs
            Children [ {tabs} ]
            --
            Node { flex_direction: FlexDirection::Column, row_gap: px(theme::SPACE_M) }
            Children [ {content} ]
        ]
    }
}

/// `EmotePanel`: the emotes by name (standard blue, own-text purple, `*me` orange) and the add
/// button; the form under them while adding or editing.
fn emotes_scene(prefs: &prefs::ChatPrefs, ui: &PanelUi) -> impl Scene + use<> {
    let mut order: Vec<usize> = (0..prefs.emotes.len()).collect();
    order.sort_by(|&a, &b| prefs.emotes[a].name.cmp(&prefs.emotes[b].name));
    let mut items: Vec<Box<dyn Scene>> = order
        .into_iter()
        .map(|i| {
            let emote = &prefs.emotes[i];
            let color = match emote.emote {
                PanelEmote::Default { .. } => UiColor::Blue,
                PanelEmote::Custom { .. } => UiColor::Purple,
                PanelEmote::Me { .. } => UiColor::Orange,
            };
            let name = emote.name.clone();
            let tooltip = format!("{} (ПКМ — изменить)", emote.tooltip());
            let act = format!("emote:{i}");
            boxed(bsn! {
                @Button { @content: name, @tooltip: tooltip }
                Button { color: color }
                Act(act)
                EmoteButton(i)
            })
        })
        .collect();
    items.push(boxed(bsn! {
        @Button { @icon: "plus", @tooltip: "Добавить эмоцию" }
        Button { color: UiColor::Green }
        Act("emote_new")
    }));
    if let Some(form) = &ui.emote_form {
        items.push(emote_form(form));
    }
    bsn! {
        Node { width: percent(100), flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, align_items: AlignItems::Center }
        Children [ {items} ]
    }
}

fn emote_form(form: &EmoteForm) -> Box<dyn Scene> {
    let kind = |name: &str, which: EmoteKind, act: &str| {
        let selected = form.kind == which;
        let name = name.to_owned();
        let act = act.to_owned();
        boxed(bsn! { @Button { @content: name } Button { selected: selected } Act(act) })
    };
    let mut rows = vec![row(vec![
        label("Тип:"),
        kind("Обычная", EmoteKind::Default, "emote_kind:default"),
        kind(
            "С кастомным текстом",
            EmoteKind::Custom,
            "emote_kind:custom",
        ),
        kind("*me", EmoteKind::Me, "emote_kind:me"),
    ])];
    if form.kind != EmoteKind::Me {
        rows.push(row(vec![
            label("Эмоция:"),
            input(form.search.clone(), PrefInput::EmoteSearch, 32, "поиск"),
        ]));
        rows.push(emote_list(form));
    }
    if form.kind != EmoteKind::Default {
        rows.push(row(vec![
            label("Текст:"),
            input(
                form.text.clone(),
                PrefInput::EmoteText,
                MAX_EMOTE_LENGTH,
                "кастомный текст эмоции",
            ),
        ]));
    }
    rows.push(row(vec![
        label("Название:"),
        input(
            form.name.clone(),
            PrefInput::EmoteName,
            MAX_EMOTE_NAME_LENGTH,
            "название в панели",
        ),
    ]));
    let mut buttons = vec![
        boxed(bsn! {
            @Button { @icon: "check", @content: "Сохранить" }
            Button { color: UiColor::Green }
            Act("emote_save")
        }),
        boxed(bsn! { @Button { @content: "Отмена" } Act("emote_cancel") }),
    ];
    if form.editing.is_some() {
        buttons.push(boxed(bsn! {
            @Button { @icon: "trash", @content: "Удалить" }
            Button { color: UiColor::Red }
            Act("emote_delete")
        }));
    }
    rows.push(row(buttons));
    boxed(bsn! {
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: px(theme::SPACE_S),
            margin: UiRect::top(px(theme::SPACE_M)),
        }
        Children [ {rows} ]
    })
}

/// Rebuilds the settings pane and the emote panel when marked dirty, and shows or hides them.
pub(crate) fn sync_panes(
    prefs: Res<prefs::ChatPrefs>,
    mut ui: ResMut<PanelUi>,
    mut settings: Query<
        (Entity, &mut Node, Option<&Children>),
        (With<SettingsRoot>, Without<EmotesRoot>),
    >,
    mut emotes: Query<(Entity, &mut Node, Option<&Children>), With<EmotesRoot>>,
    mut toggles: Query<(&mut Button, Has<SettingsToggle>, Has<EmotesToggle>)>,
    mut commands: Commands,
) {
    if !ui.is_changed() {
        return;
    }
    let display = |open: bool| if open { Display::Flex } else { Display::None };
    for (mut button, settings_toggle, emotes_toggle) in &mut toggles {
        let selected = (settings_toggle && ui.settings_open) || (emotes_toggle && ui.emotes_open);
        if (settings_toggle || emotes_toggle) && button.selected != selected {
            button.selected = selected;
        }
    }
    if let Ok((root, mut node, children)) = settings.single_mut() {
        node.display = display(ui.settings_open);
        if ui.settings_dirty && ui.settings_open {
            for &child in children.into_iter().flatten() {
                commands.entity(child).despawn();
            }
            commands
                .spawn_scene(settings_scene(&prefs, &ui))
                .insert(ChildOf(root));
        }
    }
    if let Ok((root, mut node, children)) = emotes.single_mut() {
        node.display = display(ui.emotes_open);
        if ui.emotes_dirty && ui.emotes_open {
            for &child in children.into_iter().flatten() {
                commands.entity(child).despawn();
            }
            commands
                .spawn_scene(emotes_scene(&prefs, &ui))
                .insert(ChildOf(root));
        }
    }
    let ui = ui.bypass_change_detection();
    ui.settings_dirty &= !ui.settings_open;
    ui.emotes_dirty &= !ui.emotes_open;
}

/// Rebuilds the tab bar when the tabs, their names or the shown one change; the unread counts
/// change only the labels.
pub(crate) fn sync_tabs(
    prefs: Res<prefs::ChatPrefs>,
    bars: Query<(Entity, Option<&Children>), With<PageTabs>>,
    tabs: Query<(Entity, &PageTab)>,
    children: Query<&Children>,
    mut texts: Query<&mut Text>,
    mut built: Local<Option<(Vec<String>, usize)>>,
    mut commands: Commands,
) {
    if !prefs.is_changed() && built.is_some() {
        return;
    }
    let pages = &prefs.pages;
    let shape = (
        pages
            .pages
            .iter()
            .map(|p| p.name.clone())
            .collect::<Vec<_>>(),
        pages.current,
    );
    if built.as_ref() != Some(&shape) {
        let Ok((bar, old)) = bars.single() else {
            return;
        };
        for &child in old.into_iter().flatten() {
            commands.entity(child).despawn();
        }
        let items: Vec<_> = pages
            .pages
            .iter()
            .enumerate()
            .map(|(i, page)| {
                let label = page.label();
                let selected = i == pages.current;
                let act = format!("page:{i}");
                bsn! {
                    @TabsTab { @content: label }
                    TabsTab { selected: selected }
                    Act(act)
                    PageTab(i)
                }
            })
            .collect();
        commands
            .spawn_scene(bsn! { @Tabs Children [ {items} ] })
            .insert(ChildOf(bar));
        *built = Some(shape);
        return;
    }
    for (tab, PageTab(i)) in &tabs {
        let Some(page) = pages.pages.get(*i) else {
            continue;
        };
        let label = page.label();
        for part in children.iter_descendants(tab) {
            if let Ok(mut text) = texts.get_mut(part)
                && text.0 != label
            {
                text.0.clone_from(&label);
            }
        }
    }
}

/// tgui-panel's `updateMessageCount` for new messages, and the shown tab read again once it
/// follows new messages (`ChatPanel`'s scroll tracking effect).
pub(crate) fn count_unread(
    log: Res<ChatLog>,
    view: Res<ChatView>,
    mut prefs: ResMut<prefs::ChatPrefs>,
    mut seen: Local<Option<u64>>,
) {
    let next = log.entries().back().map_or(0, |e| e.id + 1);
    // Messages restored from the last session are not new.
    let from = *seen.get_or_insert(next);
    let tracking = view.autoscroll.tracking;
    if from < next {
        let start = log.entries().partition_point(|e| e.id < from);
        for entry in log.entries().range(start..) {
            prefs.pages.count_unread(entry.message.channel, tracking);
        }
        *seen = Some(next);
    }
    if tracking && prefs.pages.current().unread > 0 {
        prefs.pages.current_mut().unread = 0;
    }
}

/// The settings sliders write the font size (whole pixels) and the line height.
pub(crate) fn read_sliders(
    sliders: Query<&LocalSlider, Changed<LocalSlider>>,
    mut prefs: ResMut<prefs::ChatPrefs>,
) {
    for slider in &sliders {
        match slider.key.as_str() {
            prefs::KEY_FONT_SIZE => {
                let size = slider.value.round();
                if prefs.font_size != size {
                    prefs.font_size = size;
                }
            }
            prefs::KEY_LINE_HEIGHT => {
                let height = (slider.value * 100.0).round() / 100.0;
                if prefs.line_height != height {
                    prefs.line_height = height;
                }
            }
            _ => {}
        }
    }
}

/// The settings into the view: the shown tab's types, the font, new highlights.
pub(crate) fn apply_prefs(
    prefs: Res<prefs::ChatPrefs>,
    mut view: ResMut<ChatView>,
    mut highlights: Local<Option<Vec<prefs::Highlight>>>,
) {
    if !prefs.is_changed() {
        return;
    }
    let filter = prefs.pages.current().channels;
    if view.filter() != filter {
        view.set_filter(filter);
    }
    view.set_font(prefs.font_size, prefs.line_height);
    if highlights.as_ref() != Some(&prefs.highlights) {
        view.set_highlighter(prefs::Highlighter::new(&prefs.highlights));
        *highlights = Some(prefs.highlights.clone());
    }
}

fn warn(log: &mut ChatLog, text: impl Into<String>, now: f64) {
    log.push(
        ChatMessage::simple(ChatChannel::Warning, Span::Warning, text),
        now,
    );
}

fn unix_now() -> u64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[allow(clippy::too_many_arguments)]
fn on_settings_act(
    act: On<UiAct>,
    panels: Query<(), With<ChatPanel>>,
    mut prefs: ResMut<prefs::ChatPrefs>,
    mut ui: ResMut<PanelUi>,
    mut view: ResMut<ChatView>,
    mut log: ResMut<ChatLog>,
    time: Res<Time<Real>>,
    mut intents: MessageWriter<SendIntent>,
    mut exported: MessageWriter<ChatExported>,
) {
    if !panels.contains(act.scope) {
        return;
    }
    let now = time.elapsed_secs_f64();
    let action = act.action.as_str();
    let (verb, arg) = action.split_once(':').unwrap_or((action, ""));
    let index = arg.split(':').next().and_then(|i| i.parse::<usize>().ok());
    let mut settings = true;
    match (verb, index) {
        ("settings", _) => {
            ui.settings_open = !ui.settings_open;
        }
        ("emotes", _) => {
            ui.emotes_open = !ui.emotes_open;
            ui.emotes_dirty = true;
        }
        // `addChatPage`, then the settings open on the new tab.
        ("page_add", _) => {
            prefs.pages.add();
            ui.settings_open = true;
            ui.settings_tab = SettingsTab::Page;
        }
        ("page", Some(i)) => prefs.pages.select(i),
        ("set", _) => {
            ui.settings_tab = match arg {
                "page" => SettingsTab::Page,
                "highlights" => SettingsTab::Highlights,
                _ => SettingsTab::General,
            };
        }
        ("page_left", _) => prefs.pages.move_left(),
        ("page_right", _) => prefs.pages.move_right(),
        ("page_mute", _) => {
            let page = prefs.pages.current_mut();
            page.hide_unread = !page.hide_unread;
        }
        ("page_remove", _) => prefs.pages.remove_current(),
        ("channel", Some(i)) => {
            if let Some(&(channel, ..)) = PAGE_CHANNELS.get(i) {
                prefs.pages.current_mut().channels.toggle(channel);
            }
        }
        ("export", _) => {
            let html = history::export_html(
                &log,
                prefs.pages.current().channels,
                prefs.font_size,
                prefs.line_height,
            );
            exported.write(ChatExported {
                file_name: history::export_file_name(unix_now()),
                html,
            });
            settings = false;
        }
        // `Button.Confirm` + `clearChat`: the shown tab's messages.
        ("clear", _) => {
            if ui.confirm_clear {
                let filter = prefs.pages.current().channels;
                log.remove_where(|e| filter.accepts(e.message.channel));
                ui.confirm_clear = false;
            } else {
                ui.confirm_clear = true;
            }
        }
        ("hl_add", _) => prefs.highlights.push(prefs::Highlight::default()),
        ("hl_apply", _) => {
            view.rebuild();
            settings = false;
        }
        ("hl", Some(i)) if i < prefs.highlights.len() => {
            let field = arg.split(':').nth(1).unwrap_or("");
            if field == "remove" {
                prefs.highlights.remove(i);
            } else {
                let h = &mut prefs.highlights[i];
                match field {
                    "enabled" => h.enabled = !h.enabled,
                    "whole" => h.whole_message = !h.whole_message,
                    "word" => h.match_word = !h.match_word,
                    "case" => h.match_case = !h.match_case,
                    _ => {}
                }
            }
        }
        // `emotes/execute`.
        ("emote", Some(i)) => {
            if let Some(emote) = prefs.emotes.get(i) {
                intents.write(SendIntent(ClientIntent::PanelEmote(emote.emote.clone())));
            }
            settings = false;
        }
        // `emotes/create`.
        ("emote_new", _) => {
            if prefs.emotes.len() >= MAX_EMOTES {
                warn(
                    &mut log,
                    format!("Достигнут максимум эмоций: {MAX_EMOTES}."),
                    now,
                );
            } else {
                ui.emote_form = Some(EmoteForm::default());
            }
            ui.emotes_dirty = true;
            settings = false;
        }
        ("emote_kind", _) => {
            if let Some(form) = ui.emote_form.as_mut() {
                form.kind = match arg {
                    "custom" => EmoteKind::Custom,
                    "me" => EmoteKind::Me,
                    _ => EmoteKind::Default,
                };
            }
            ui.emotes_dirty = true;
            settings = false;
        }
        ("emote_save", _) => {
            if let Some(form) = ui.emote_form.clone() {
                match form.build(&prefs.emotes) {
                    Ok(emote) => {
                        match form.editing {
                            Some(i) if i < prefs.emotes.len() => prefs.emotes[i] = emote,
                            _ => prefs.emotes.push(emote),
                        }
                        ui.emote_form = None;
                    }
                    Err(warning) => warn(&mut log, warning, now),
                }
            }
            ui.emotes_dirty = true;
            settings = false;
        }
        ("emote_cancel", _) => {
            if ui.emote_form.take().is_some_and(|f| f.editing.is_none()) {
                warn(&mut log, "Добавление эмоции отменено.", now);
            }
            ui.emotes_dirty = true;
            settings = false;
        }
        ("emote_delete", _) => {
            if let Some(Some(i)) = ui.emote_form.take().map(|f| f.editing)
                && i < prefs.emotes.len()
            {
                prefs.emotes.remove(i);
            }
            ui.emotes_dirty = true;
            settings = false;
        }
        _ => return,
    }
    if verb != "clear" {
        ui.confirm_clear = false;
    }
    if settings {
        ui.settings_dirty = true;
    }
}

/// The pane's text fields write their setting as typed (tgui-panel writes them on blur); the
/// pane is not rebuilt, so the field keeps its cursor.
fn on_pref_input(
    changed: On<InputChanged>,
    fields: Query<&PrefInput>,
    mut picks: Query<(&EmotePick, &mut Node)>,
    mut prefs: ResMut<prefs::ChatPrefs>,
    mut ui: ResMut<PanelUi>,
) {
    let Ok(field) = fields.get(changed.entity) else {
        return;
    };
    let value = changed.value.clone();
    match field {
        PrefInput::PageName => prefs.pages.current_mut().name = value,
        PrefInput::HighlightText(i) => {
            if let Some(h) = prefs.highlights.get_mut(*i) {
                h.text = value;
            }
        }
        PrefInput::HighlightColor(i) => {
            if prefs::parse_color(&value).is_some()
                && let Some(h) = prefs.highlights.get_mut(*i)
            {
                h.color = value;
            }
        }
        // The form is view state: changing it must not rebuild the panel under the cursor.
        PrefInput::EmoteName | PrefInput::EmoteText => {
            let ui = ui.bypass_change_detection();
            if let Some(form) = ui.emote_form.as_mut() {
                match field {
                    PrefInput::EmoteName => form.name = value,
                    _ => form.text = value,
                }
            }
        }
        PrefInput::EmoteSearch => {
            let ui = ui.bypass_change_detection();
            if let Some(form) = ui.emote_form.as_mut() {
                form.search = value;
                for (pick, mut node) in &mut picks {
                    node.display = display_if(matches(&form.search, pick.0));
                }
            }
        }
    }
}

/// Picking an emote of the form's list (`tgui_input_list`): selects it and, while the name is
/// still the previous pick's, names it after the emote (`suggested_name`). The form is not
/// rebuilt, so the list keeps its scroll and the search field its text.
fn on_emote_pick(
    mut click: On<PointerClick>,
    picks: Query<&EmotePick>,
    mut buttons: Query<(&EmotePick, &mut Button)>,
    mut fields: Query<(&PrefInput, &mut Input)>,
    mut ui: ResMut<PanelUi>,
) {
    if click.button != PointerButton::Primary {
        return;
    }
    let Ok(&EmotePick(key)) = picks.get(click.entity) else {
        return;
    };
    click.propagate(false);
    let ui = ui.bypass_change_detection();
    let Some(form) = ui.emote_form.as_mut() else {
        return;
    };
    let old_name = emote_names::name_of(&form.key).unwrap_or(&form.key);
    let rename = form.name.trim().is_empty() || form.name == old_name;
    form.key = key.to_owned();
    for (pick, mut button) in &mut buttons {
        let selected = pick.0 == key;
        if button.selected != selected {
            button.selected = selected;
        }
    }
    if rename {
        form.name = emote_names::name_of(key).unwrap_or(key).to_owned();
        for (field, mut input) in &mut fields {
            if *field == PrefInput::EmoteName {
                input.value.clone_from(&form.name);
            }
        }
    }
}

/// Whether the list shows `key` under the filter `search` (by name or key, any case).
fn matches(search: &str, key: &str) -> bool {
    let search = search.trim().to_lowercase();
    search.is_empty()
        || key.contains(&search)
        || emote_names::name_of(key).is_some_and(|n| n.to_lowercase().contains(&search))
}

fn display_if(shown: bool) -> Display {
    if shown { Display::Flex } else { Display::None }
}

/// BandaStation's "Выбор эмоции" list: every emote by name, the picked one selected.
fn emote_list(form: &EmoteForm) -> Box<dyn Scene> {
    let mut names: Vec<_> = emote_names::NAMES.to_vec();
    names.sort_by(|a, b| a.1.cmp(b.1));
    let items: Vec<Box<dyn Scene>> = names
        .into_iter()
        .map(|(key, name)| {
            let selected = form.key == key;
            let display = display_if(matches(&form.search, key));
            let content = name.to_owned();
            let tooltip = format!("*{key}");
            boxed(bsn! {
                @Button { @content: content, @tooltip: tooltip }
                Button { selected: selected }
                Node { display: display }
                EmotePick(key)
            })
        })
        .collect();
    boxed(bsn! {
        Node {
            width: percent(100),
            max_height: px(160),
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::Wrap,
            align_content: AlignContent::FlexStart,
            overflow: Overflow::scroll_y(),
        }
        Children [ {items} ]
    })
}

/// BandaStation `emotes/contextAction` (right click): the emote's edit form.
fn on_emote_right_click(
    mut click: On<PointerClick>,
    buttons: Query<&EmoteButton>,
    prefs: Res<prefs::ChatPrefs>,
    mut ui: ResMut<PanelUi>,
) {
    if click.button != PointerButton::Secondary {
        return;
    }
    let Ok(EmoteButton(i)) = buttons.get(click.entity) else {
        return;
    };
    click.propagate(false);
    if let Some(emote) = prefs.emotes.get(*i) {
        ui.emote_form = Some(EmoteForm::edit(*i, emote));
        ui.emotes_dirty = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn emote(name: &str) -> CustomEmote {
        CustomEmote {
            name: name.into(),
            emote: PanelEmote::Default { key: "sigh".into() },
        }
    }

    #[test]
    fn the_emote_form_checks_like_bandastation() {
        let mut form = EmoteForm {
            key: " *Sigh ".into(),
            ..default()
        };
        // The name defaults to the key; the key is cleaned.
        assert_eq!(
            form.build(&[]),
            Ok(CustomEmote {
                name: "вздох".into(),
                emote: PanelEmote::Default { key: "sigh".into() },
            })
        );
        assert_eq!(
            form.build(&[emote("вздох")]),
            Err("Эмоция \"вздох\" уже существует!".into())
        );
        // Editing it keeps its own name.
        form.editing = Some(0);
        assert!(form.build(&[emote("вздох")]).is_ok());
        form.editing = None;
        form.key = "вфвф".into();
        assert_eq!(form.build(&[]), Err("Эмоция вфвф не существует!".into()));
        // A custom text needs the text; `*me` is named after it.
        form.kind = EmoteKind::Custom;
        form.key = "scream".into();
        assert_eq!(form.build(&[]), Err("Добавление эмоции отменено.".into()));
        form.text = "вопит".into();
        assert_eq!(
            form.build(&[]).unwrap().emote,
            PanelEmote::Custom {
                key: "scream".into(),
                text: "вопит".into()
            }
        );
        form.kind = EmoteKind::Me;
        form.text = "очень долго и задумчиво чешет затылок".into();
        let me = form.build(&[]).unwrap();
        assert_eq!(me.name.chars().count(), MAX_EMOTE_NAME_LENGTH);
        // At most 45: a new one is refused, an edit is not.
        let full: Vec<_> = (0..MAX_EMOTES).map(|i| emote(&i.to_string())).collect();
        assert_eq!(
            form.build(&full),
            Err(format!("Достигнут максимум эмоций: {MAX_EMOTES}."))
        );
        form.editing = Some(3);
        assert!(form.build(&full).is_ok());
    }
}
