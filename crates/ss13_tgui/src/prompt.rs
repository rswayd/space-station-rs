//! Server half of tg's input modals (`code/modules/tgui_input/`): `tgui_alert`,
//! `tgui_input_text`, `tgui_input_number`, `tgui_input_list`, `tgui_input_checkboxes`,
//! `tgui_input_keycombo`, `tgui_color_picker`.
//!
//! tg's procs sleep until the player answers. Here [`ask`] spawns a prompt entity (the window
//! source) and returns it; the caller puts its own component on it to remember what the
//! question was for, and reads [`PromptAnswered`] in `UiSystems::Handle` after
//! [`PromptSystems::Answer`]. The answer is `None` when the player cancelled, closed the window,
//! disconnected or ran out of time, as tg returns `null`. The prompt entity is despawned in
//! `UiSystems::Send` of the tick it was answered, which closes its window.
//!
//! As after any tg sleep, the caller re-checks at the moment of effect that the actor can still
//! do what it asked about: the prompt itself is always interactive (`always_state`).
// shortcut: `large_buttons`/`swapped_buttons` are tg's defaults (off/on) until game
// preferences exist; `encode` (html_encode) does not apply, chat text is not HTML.

use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;

use crate::UiInterface;
use crate::interfaces::alert::{AlertStatic, AlertUi, CANCEL, CHOOSE, PromptTimer, SUBMIT};
use crate::interfaces::checkbox_input::{CheckboxInputStatic, CheckboxInputUi, parse_indices};
use crate::interfaces::color_picker::{ColorPickerStatic, ColorPickerUi, sanitize_hex};
use crate::interfaces::key_combo::{KeyComboStatic, KeyComboUi, MAX_COMBO_LEN};
use crate::interfaces::list_input::{ListInputData, ListInputUi};
use crate::interfaces::number_input::{NumberInputStatic, NumberInputUi};
use crate::interfaces::radial::{RadialData, RadialFrame, RadialSlice, RadialUi};
use crate::interfaces::text_input::{TextInputStatic, TextInputUi};
use crate::server::{
    AlwaysState, AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems, UiWindow,
};

/// tg default of `tgui_input_swapped` (unset toggle: on).
const SWAPPED_BUTTONS: bool = true;
/// A prompt whose window never opened (no client view, refused open) gives up after this.
const OPEN_GRACE_SECS: f64 = 2.0;

/// What is asked, with tg's arguments.
#[derive(Clone, Debug, PartialEq)]
pub enum PromptKind {
    /// `tgui_alert(buttons, autofocus)`. More than three buttons become a list, as in tg.
    Alert {
        buttons: Vec<String>,
        autofocus: bool,
    },
    /// `tgui_input_text(default, max_length, multiline)`; `max_length` 0 is no limit.
    Text {
        default: String,
        max_length: u32,
        multiline: bool,
    },
    /// `tgui_input_number(default, max_value, min_value, round_value)`.
    Number {
        default: f64,
        min: f64,
        max: f64,
        round: bool,
    },
    /// `tgui_input_list(items, default)`.
    List {
        items: Vec<String>,
        default: Option<String>,
    },
    /// `tgui_input_checkboxes(items, min_checked, max_checked)`.
    Checkboxes {
        items: Vec<String>,
        min: u32,
        max: u32,
    },
    /// `tgui_input_keycombo(default)`.
    KeyCombo { default: String },
    /// `tgui_color_picker(default, autofocus)`; `default` is `#rrggbb`.
    Color { default: String, autofocus: bool },
    /// `show_radial_menu`: drawn around `anchor` by the client ([`RadialUi`]).
    Radial {
        items: Vec<RadialChoice>,
        anchor: Option<ss13_shared::protocol::NetId>,
        frame: RadialFrame,
    },
}

/// One choice of a [`PromptKind::Radial`].
#[derive(Clone, Debug, PartialEq)]
pub struct RadialChoice {
    pub name: String,
    pub icon: Option<ss13_shared::IconStateId>,
}

/// A valid answer, by the kind asked.
#[derive(Clone, Debug, PartialEq)]
pub enum PromptAnswer {
    /// The index of the alert button pressed.
    Button(usize),
    /// Trimmed to `max_length` characters.
    Text(String),
    /// In range, rounded when asked.
    Number(f64),
    /// The index of the chosen list item.
    Item(usize),
    /// Indices of the checked items, in the order they were checked.
    Items(Vec<usize>),
    KeyCombo(String),
    /// `#rrggbb`, lowercase.
    Color(String),
}

/// The question, on its own entity (the window source).
#[derive(Component, Clone, Debug)]
#[require(AlwaysState)]
pub struct Prompt {
    pub client: Entity,
    pub actor: Entity,
    pub message: String,
    pub kind: PromptKind,
    /// Seconds; `None` waits forever.
    pub timeout: Option<f64>,
    /// List items with duplicates told apart (tg `avoid_assoc_duplicate_keys`).
    keys: Vec<String>,
    started: Option<f64>,
    opened: bool,
}

/// Marks an answered prompt until it is despawned.
#[derive(Component)]
struct Answered;

/// The player's answer to [`ask`]. `prompt` is the entity [`ask`] returned, still alive
/// in `UiSystems::Handle` so the caller can read its components from it.
#[derive(Message, Clone, Debug)]
pub struct PromptAnswered {
    pub prompt: Entity,
    pub client: Entity,
    pub actor: Entity,
    pub answer: Option<PromptAnswer>,
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PromptSystems {
    /// Writes [`PromptAnswered`]; callers read it after this, in `UiSystems::Handle`.
    Answer,
}

/// A question for `client`, whose body is `actor`.
#[derive(Clone, Debug)]
pub struct Ask {
    pub client: Entity,
    pub actor: Entity,
    pub title: String,
    pub message: String,
    pub kind: PromptKind,
    pub timeout: Option<f64>,
}

impl Ask {
    pub fn new(
        client: Entity,
        actor: Entity,
        title: impl Into<String>,
        message: impl Into<String>,
        kind: PromptKind,
    ) -> Self {
        Self {
            client,
            actor,
            title: title.into(),
            message: message.into(),
            kind,
            timeout: None,
        }
    }

    pub fn timeout(mut self, secs: f64) -> Self {
        self.timeout = (secs > 0.0).then_some(secs);
        self
    }

    fn into_prompt(self) -> (Prompt, OpenUi) {
        let mut kind = self.kind;
        // `tgui_alert`: "use a list" past three buttons.
        if let PromptKind::Alert { buttons, .. } = &kind
            && buttons.len() > 3
        {
            kind = PromptKind::List {
                items: buttons.clone(),
                default: None,
            };
        }
        let keys = match &kind {
            PromptKind::List { items, .. } => unique_keys(items),
            PromptKind::Radial { items, .. } => {
                let names: Vec<String> = items.iter().map(|i| i.name.clone()).collect();
                unique_keys(&names)
            }
            _ => Vec::new(),
        };
        let interface = match kind {
            PromptKind::Alert { .. } => AlertUi::NAME,
            PromptKind::Text { .. } => TextInputUi::NAME,
            PromptKind::Number { .. } => NumberInputUi::NAME,
            PromptKind::List { .. } => ListInputUi::NAME,
            PromptKind::Checkboxes { .. } => CheckboxInputUi::NAME,
            PromptKind::KeyCombo { .. } => KeyComboUi::NAME,
            PromptKind::Color { .. } => ColorPickerUi::NAME,
            PromptKind::Radial { .. } => RadialUi::NAME,
        };
        let open = OpenUi {
            client: self.client,
            actor: self.actor,
            source: Entity::PLACEHOLDER,
            interface,
            title: self.title,
        };
        let prompt = Prompt {
            client: self.client,
            actor: self.actor,
            message: self.message,
            kind,
            timeout: self.timeout,
            keys,
            started: None,
            opened: false,
        };
        (prompt, open)
    }
}

/// Opens the question and returns its prompt entity (tg `tgui_input_*` before its sleep).
pub fn ask(commands: &mut Commands, ask: Ask) -> Entity {
    let (prompt, mut open) = ask.into_prompt();
    let entity = commands.spawn((Name::new("prompt"), prompt)).id();
    open.source = entity;
    commands.write_message(open);
    entity
}

/// [`ask`] for exclusive systems.
pub fn ask_world(world: &mut World, ask: Ask) -> Entity {
    let (prompt, mut open) = ask.into_prompt();
    let entity = world.spawn((Name::new("prompt"), prompt)).id();
    open.source = entity;
    world.write_message(open);
    entity
}

/// tg `avoid_assoc_duplicate_keys`: the second `X` is `X (2)`.
fn unique_keys(items: &[String]) -> Vec<String> {
    let mut seen: Vec<(&str, u32)> = Vec::new();
    items
        .iter()
        .map(
            |item| match seen.iter_mut().find(|(k, _)| *k == item.as_str()) {
                Some((_, n)) => {
                    *n += 1;
                    format!("{item} ({n})")
                }
                None => {
                    seen.push((item, 1));
                    item.clone()
                }
            },
        )
        .collect()
}

/// `CLAMP01((timeout - elapsed - 1 SECONDS) / (timeout - 1 SECONDS))`.
fn timer(prompt: &Prompt, now: f64) -> PromptTimer {
    let timeout = prompt.timeout.zip(prompt.started).map(|(timeout, start)| {
        let span = (timeout - 1.0).max(f64::EPSILON);
        ((timeout - (now - start) - 1.0) / span).clamp(0.0, 1.0) as f32
    });
    PromptTimer { timeout }
}

/// The parsed act of every modal: `choose`/`submit` with its raw entry, or `cancel`.
#[derive(Clone, Debug, PartialEq)]
pub enum PromptAct {
    Entry(String),
    Cancel,
}

fn parse(action: &str, params: &UiParams, choose: &str, key: &str) -> Result<PromptAct, UiReject> {
    match action {
        a if a == choose => params.text(key).map(PromptAct::Entry),
        CANCEL => Ok(PromptAct::Cancel),
        _ => Err(UiReject::UnknownAction),
    }
}

type Time = Res<'static, bevy::time::Time<bevy::time::Real>>;

macro_rules! prompt_ui {
    ($ui:ty, $choose:expr, $key:expr, |$p:ident| $static:expr, |$p2:ident, $now:ident| $data:expr) => {
        impl ServerUi for $ui {
            type Source = &'static Prompt;
            type UserParam = Time;
            type Act = PromptAct;

            fn static_data($p: &Prompt) -> <Self as UiInterface>::StaticData {
                $static
            }

            fn data(
                $p2: &Prompt,
                _: Entity,
                time: &SystemParamItem<'_, '_, Time>,
            ) -> <Self as UiInterface>::Data {
                let $now = time.elapsed_secs_f64();
                $data
            }

            fn parse_act(action: &str, params: &UiParams) -> Result<PromptAct, UiReject> {
                parse(action, params, $choose, $key)
            }
        }
    };
}

prompt_ui!(
    AlertUi,
    CHOOSE,
    "choice",
    |p| match &p.kind {
        PromptKind::Alert { buttons, autofocus } => AlertStatic {
            message: p.message.clone(),
            buttons: buttons.clone(),
            autofocus: *autofocus,
            large_buttons: false,
            swapped_buttons: SWAPPED_BUTTONS,
        },
        _ => AlertStatic::default(),
    },
    |p, now| timer(p, now)
);

prompt_ui!(
    TextInputUi,
    SUBMIT,
    "entry",
    |p| match &p.kind {
        PromptKind::Text {
            default,
            max_length,
            multiline,
        } => TextInputStatic {
            message: p.message.clone(),
            placeholder: default.clone(),
            max_length: *max_length,
            multiline: *multiline,
            large_buttons: false,
            swapped_buttons: SWAPPED_BUTTONS,
        },
        _ => TextInputStatic::default(),
    },
    |p, now| timer(p, now)
);

prompt_ui!(
    NumberInputUi,
    SUBMIT,
    "entry",
    |p| match &p.kind {
        PromptKind::Number {
            default,
            min,
            max,
            round,
        } => NumberInputStatic {
            message: p.message.clone(),
            init_value: *default,
            min_value: *min,
            max_value: *max,
            round_value: *round,
            large_buttons: false,
            swapped_buttons: SWAPPED_BUTTONS,
        },
        _ => NumberInputStatic::default(),
    },
    |p, now| timer(p, now)
);

prompt_ui!(
    CheckboxInputUi,
    SUBMIT,
    "entry",
    |p| match &p.kind {
        PromptKind::Checkboxes { items, min, max } => CheckboxInputStatic {
            message: p.message.clone(),
            items: items.clone(),
            min_checked: *min,
            max_checked: *max,
            large_buttons: false,
            swapped_buttons: SWAPPED_BUTTONS,
        },
        _ => CheckboxInputStatic::default(),
    },
    |p, now| timer(p, now)
);

prompt_ui!(
    KeyComboUi,
    SUBMIT,
    "entry",
    |p| match &p.kind {
        PromptKind::KeyCombo { default } => KeyComboStatic {
            message: p.message.clone(),
            init_value: default.clone(),
            large_buttons: false,
            swapped_buttons: SWAPPED_BUTTONS,
        },
        _ => KeyComboStatic::default(),
    },
    |p, now| timer(p, now)
);

prompt_ui!(
    ColorPickerUi,
    SUBMIT,
    "entry",
    |p| match &p.kind {
        PromptKind::Color { default, autofocus } => ColorPickerStatic {
            message: p.message.clone(),
            default_color: default.clone(),
            autofocus: *autofocus,
            // tg sends `!prefs || ...`, the same defaults as the other modals for a player.
            large_buttons: false,
            swapped_buttons: SWAPPED_BUTTONS,
        },
        _ => ColorPickerStatic::default(),
    },
    |p, now| timer(p, now)
);

prompt_ui!(ListInputUi, SUBMIT, "entry", |_p| (), |p, now| {
    let init_value = match &p.kind {
        PromptKind::List {
            default: Some(d), ..
        } if p.keys.contains(d) => d.clone(),
        _ => p.keys.first().cloned().unwrap_or_default(),
    };
    ListInputData {
        title: String::new(),
        message: p.message.clone(),
        init_value,
        items: p.keys.clone(),
        timeout: timer(p, now).timeout,
    }
});

prompt_ui!(
    RadialUi,
    SUBMIT,
    "entry",
    |_p| (),
    |p, _now| match &p.kind {
        PromptKind::Radial {
            items,
            anchor,
            frame,
        } => RadialData {
            anchor: *anchor,
            items: items
                .iter()
                .zip(&p.keys)
                .map(|(item, key)| RadialSlice {
                    key: key.clone(),
                    name: item.name.clone(),
                    icon: item.icon,
                })
                .collect(),
            frame: *frame,
        },
        _ => RadialData::default(),
    }
);

/// tg `ui_act` validation of an entry against the question; `None` drops the act (tg `CRASH`
/// or a bare `return`: the window stays open).
pub fn check_entry(prompt: &Prompt, entry: &str) -> Option<PromptAnswer> {
    match &prompt.kind {
        PromptKind::Alert { buttons, .. } => buttons
            .iter()
            .position(|b| b == entry)
            .map(PromptAnswer::Button),
        PromptKind::Text { max_length, .. } => {
            if *max_length > 0 && entry.chars().count() > *max_length as usize {
                return None;
            }
            Some(PromptAnswer::Text(entry.to_owned()))
        }
        PromptKind::Number {
            min, max, round, ..
        } => {
            let value = entry.trim().parse::<f64>().ok().filter(|v| v.is_finite())?;
            let value = if *round { value.round() } else { value };
            (*min..=*max)
                .contains(&value)
                .then_some(PromptAnswer::Number(value))
        }
        PromptKind::List { .. } | PromptKind::Radial { .. } => prompt
            .keys
            .iter()
            .position(|k| k == entry)
            .map(PromptAnswer::Item),
        PromptKind::Checkboxes { items, min, max } => {
            let picked = parse_indices(entry)?;
            if picked.iter().any(|&i| i >= items.len()) {
                return None;
            }
            // tg closes with no choices when the count is out of bounds.
            let count = picked.len() as u32;
            Some(PromptAnswer::Items(if (*min..=*max).contains(&count) {
                picked
            } else {
                Vec::new()
            }))
        }
        PromptKind::KeyCombo { .. } => {
            let ok = !entry.is_empty()
                && entry.len() <= MAX_COMBO_LEN
                && entry.chars().all(|c| c.is_ascii_graphic());
            ok.then(|| PromptAnswer::KeyCombo(entry.to_owned()))
        }
        // `sanitize_hexcolor(LOWER_TEXT(entry), 6, TRUE)`; a bad one keeps the window open.
        PromptKind::Color { .. } => sanitize_hex(entry).map(PromptAnswer::Color),
    }
}

fn answer_acts<T: ServerUi<Act = PromptAct>>(
    mut acts: MessageReader<UiActed<T>>,
    prompts: Query<&Prompt, Without<Answered>>,
    mut answered: MessageWriter<PromptAnswered>,
    mut commands: Commands,
) {
    for act in acts.read() {
        let Ok(prompt) = prompts.get(act.source) else {
            continue;
        };
        // A window is its client's: the act comes from the asked player.
        if act.client != prompt.client {
            continue;
        }
        let answer = match &act.act {
            PromptAct::Cancel => None,
            PromptAct::Entry(entry) => match check_entry(prompt, entry) {
                Some(answer) => Some(answer),
                None => {
                    debug!("prompt {}: entry {entry:?} rejected", act.source);
                    continue;
                }
            },
        };
        commands.entity(act.source).insert(Answered);
        answered.write(PromptAnswered {
            prompt: act.source,
            client: prompt.client,
            actor: prompt.actor,
            answer,
        });
    }
}

/// Closed windows, disconnects and timeouts answer `None` (tg `closed`, `QDEL_IN(timeout)`).
fn drop_unanswered(
    mut prompts: Query<(Entity, &mut Prompt), Without<Answered>>,
    windows: Query<&UiWindow>,
    time: Res<bevy::time::Time<bevy::time::Real>>,
    mut answered: MessageWriter<PromptAnswered>,
    mut commands: Commands,
) {
    if prompts.is_empty() {
        return;
    }
    let now = time.elapsed_secs_f64();
    for (entity, mut prompt) in &mut prompts {
        let started = *prompt.started.get_or_insert(now);
        let open = windows.iter().any(|w| w.source == entity);
        if open && !prompt.opened {
            prompt.opened = true;
        }
        let gone = if prompt.opened {
            !open
        } else {
            now - started > OPEN_GRACE_SECS
        };
        let timed_out = prompt.timeout.is_some_and(|t| now - started >= t);
        if gone || timed_out {
            commands.entity(entity).insert(Answered);
            answered.write(PromptAnswered {
                prompt: entity,
                client: prompt.client,
                actor: prompt.actor,
                answer: None,
            });
        }
    }
}

fn despawn_answered(done: Query<Entity, With<Answered>>, mut commands: Commands) {
    for entity in &done {
        commands.entity(entity).despawn();
    }
}

pub(crate) fn plugin(app: &mut App) {
    app.add_message::<PromptAnswered>()
        .register_ui::<AlertUi>()
        .register_ui::<TextInputUi>()
        .register_ui::<NumberInputUi>()
        .register_ui::<ListInputUi>()
        .register_ui::<CheckboxInputUi>()
        .register_ui::<KeyComboUi>()
        .register_ui::<ColorPickerUi>()
        .register_ui::<RadialUi>()
        .configure_sets(Update, PromptSystems::Answer.in_set(UiSystems::Handle))
        .add_systems(
            Update,
            (
                (
                    drop_unanswered,
                    answer_acts::<AlertUi>,
                    answer_acts::<TextInputUi>,
                    answer_acts::<NumberInputUi>,
                    answer_acts::<ListInputUi>,
                    answer_acts::<CheckboxInputUi>,
                    answer_acts::<KeyComboUi>,
                    answer_acts::<ColorPickerUi>,
                    answer_acts::<RadialUi>,
                )
                    .chain()
                    .in_set(PromptSystems::Answer),
                despawn_answered.in_set(UiSystems::Send),
            ),
        );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::TguiServerPlugin;
    use ss13_net::server::{ClientDisconnected, IntentReceived};
    use ss13_net::ui::server::{SendUi, UiMessageReceived};
    use ss13_shared::protocol::ClientIntent;
    use ss13_shared::ui::UiClientMessage;

    #[derive(Resource, Default)]
    struct Answers(Vec<PromptAnswered>);

    fn app() -> (App, Entity, Entity) {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<IntentReceived>()
            .add_message::<ClientDisconnected>()
            .add_message::<UiMessageReceived>()
            .add_message::<SendUi>()
            .add_plugins(TguiServerPlugin)
            .init_resource::<Answers>()
            .add_systems(
                Update,
                (|mut read: MessageReader<PromptAnswered>, mut log: ResMut<Answers>| {
                    log.0.extend(read.read().cloned());
                })
                .after(PromptSystems::Answer)
                .in_set(UiSystems::Handle),
            );
        let client = app.world_mut().spawn_empty().id();
        let actor = app.world_mut().spawn_empty().id();
        (app, client, actor)
    }

    #[test]
    fn radial_entries_are_the_slices_and_nothing_else() {
        let radial = Ask::new(
            Entity::PLACEHOLDER,
            Entity::PLACEHOLDER,
            "t",
            "",
            PromptKind::Radial {
                items: vec![
                    RadialChoice {
                        name: "a".into(),
                        icon: None,
                    },
                    RadialChoice {
                        name: "a".into(),
                        icon: None,
                    },
                ],
                anchor: None,
                frame: RadialFrame::default(),
            },
        );
        let (prompt, open) = radial.into_prompt();
        assert_eq!(open.interface, RadialUi::NAME);
        assert_eq!(check_entry(&prompt, "a"), Some(PromptAnswer::Item(0)));
        assert_eq!(check_entry(&prompt, "a (2)"), Some(PromptAnswer::Item(1)));
        assert_eq!(check_entry(&prompt, "b"), None);
    }

    fn open(app: &mut App, ask: Ask) -> (Entity, u32) {
        let prompt = ask_world(app.world_mut(), ask);
        app.update();
        app.update();
        let mut q = app.world_mut().query::<&UiWindow>();
        let id = q.iter(app.world()).find(|w| w.source == prompt).unwrap().id;
        (prompt, id)
    }

    fn act(app: &mut App, client: Entity, window: u32, action: &str, params: &[(&str, &str)]) {
        app.world_mut().write_message(IntentReceived {
            client,
            intent: ClientIntent::UiAct {
                window,
                action: action.into(),
                params: params
                    .iter()
                    .map(|(k, v)| ((*k).into(), (*v).into()))
                    .collect(),
            },
        });
        app.update();
    }

    fn yes_no(client: Entity, actor: Entity) -> Ask {
        let buttons = vec!["Да".into(), "Нет".into()];
        Ask::new(
            client,
            actor,
            "Вопрос",
            "Уверены?",
            PromptKind::Alert {
                buttons,
                autofocus: true,
            },
        )
    }

    #[test]
    fn answer_reaches_the_caller_and_closes_the_window() {
        let (mut app, client, actor) = app();
        let (prompt, id) = open(&mut app, yes_no(client, actor));
        // Another client cannot answer, an unknown button is dropped.
        let stranger = app.world_mut().spawn_empty().id();
        act(&mut app, stranger, id, CHOOSE, &[("choice", "Да")]);
        act(&mut app, client, id, CHOOSE, &[("choice", "Может")]);
        assert!(app.world().resource::<Answers>().0.is_empty());

        act(&mut app, client, id, CHOOSE, &[("choice", "Нет")]);
        let answers = &app.world().resource::<Answers>().0;
        assert_eq!(answers.len(), 1);
        assert_eq!(answers[0].prompt, prompt);
        assert_eq!(answers[0].answer, Some(PromptAnswer::Button(1)));
        assert!(app.world().get_entity(prompt).is_err());
        app.update();
        let mut q = app.world_mut().query::<&UiWindow>();
        assert_eq!(q.iter(app.world()).count(), 0);
        // A second press after the answer does nothing.
        act(&mut app, client, id, CHOOSE, &[("choice", "Да")]);
        assert_eq!(app.world().resource::<Answers>().0.len(), 1);
    }

    #[test]
    fn closing_the_window_or_timing_out_answers_none() {
        let (mut app, client, actor) = app();
        let (_, id) = open(&mut app, yes_no(client, actor));
        app.world_mut().write_message(UiMessageReceived {
            client,
            message: UiClientMessage::Close { window: id },
        });
        app.update();
        let answers = &app.world().resource::<Answers>().0;
        assert_eq!(answers.len(), 1);
        assert_eq!(answers[0].answer, None);

        let ask = yes_no(client, actor).timeout(0.05);
        let (prompt, _) = open(&mut app, ask);
        std::thread::sleep(std::time::Duration::from_millis(80));
        app.update();
        let answers = &app.world().resource::<Answers>().0;
        assert_eq!(answers.len(), 2);
        assert_eq!(
            (answers[1].prompt, answers[1].answer.clone()),
            (prompt, None)
        );
    }

    #[test]
    fn long_text_is_joined_and_checked_against_max_length() {
        let (mut app, client, actor) = app();
        let kind = PromptKind::Text {
            default: String::new(),
            max_length: 200,
            multiline: true,
        };
        let (_, id) = open(&mut app, Ask::new(client, actor, "Т", "", kind));
        let head = "я".repeat(128);
        let tail = "ю".repeat(10);
        act(
            &mut app,
            client,
            id,
            SUBMIT,
            &[("entry", &head), ("entry.1", &tail)],
        );
        let answers = &app.world().resource::<Answers>().0;
        assert_eq!(
            answers[0].answer,
            Some(PromptAnswer::Text(format!("{head}{tail}")))
        );
    }

    fn prompt(kind: PromptKind) -> Prompt {
        Ask::new(Entity::PLACEHOLDER, Entity::PLACEHOLDER, "t", "m", kind)
            .into_prompt()
            .0
    }

    #[test]
    fn entries_are_checked_like_tg_ui_act() {
        let alert = prompt(PromptKind::Alert {
            buttons: vec!["Да".into(), "Нет".into()],
            autofocus: true,
        });
        assert_eq!(check_entry(&alert, "Нет"), Some(PromptAnswer::Button(1)));
        assert_eq!(check_entry(&alert, "Может"), None);

        let text = prompt(PromptKind::Text {
            default: String::new(),
            max_length: 3,
            multiline: false,
        });
        assert_eq!(
            check_entry(&text, "абв"),
            Some(PromptAnswer::Text("абв".into()))
        );
        assert_eq!(check_entry(&text, "абвг"), None);

        let number = prompt(PromptKind::Number {
            default: 0.0,
            min: 0.0,
            max: 10.0,
            round: true,
        });
        assert_eq!(check_entry(&number, "4.6"), Some(PromptAnswer::Number(5.0)));
        assert_eq!(check_entry(&number, "11"), None);
        assert_eq!(check_entry(&number, "inf"), None);

        let boxes = prompt(PromptKind::Checkboxes {
            items: vec!["a".into(), "b".into(), "c".into()],
            min: 1,
            max: 2,
        });
        assert_eq!(
            check_entry(&boxes, "2,0"),
            Some(PromptAnswer::Items(vec![2, 0]))
        );
        assert_eq!(
            check_entry(&boxes, "0,1,2"),
            Some(PromptAnswer::Items(vec![]))
        );
        assert_eq!(check_entry(&boxes, "3"), None);

        let combo = prompt(PromptKind::KeyCombo {
            default: String::new(),
        });
        assert_eq!(
            check_entry(&combo, "CtrlShiftA"),
            Some(PromptAnswer::KeyCombo("CtrlShiftA".into()))
        );
        assert_eq!(check_entry(&combo, ""), None);

        let color = prompt(PromptKind::Color {
            default: "#000000".into(),
            autofocus: true,
        });
        assert_eq!(
            check_entry(&color, "#ABC"),
            Some(PromptAnswer::Color("#aabbcc".into()))
        );
        assert_eq!(check_entry(&color, "red"), None);
    }

    #[test]
    fn list_keys_tell_duplicates_apart_and_long_alerts_become_lists() {
        let list = prompt(PromptKind::List {
            items: vec!["Бар".into(), "Бар".into(), "Мостик".into(), "Бар".into()],
            default: None,
        });
        assert_eq!(list.keys, ["Бар", "Бар (2)", "Мостик", "Бар (3)"]);
        assert_eq!(check_entry(&list, "Бар (2)"), Some(PromptAnswer::Item(1)));

        let long = prompt(PromptKind::Alert {
            buttons: (1..=4).map(|i| i.to_string()).collect(),
            autofocus: true,
        });
        assert!(matches!(long.kind, PromptKind::List { .. }));
    }

    #[test]
    fn timer_runs_down_to_zero() {
        let mut p = prompt(PromptKind::KeyCombo {
            default: String::new(),
        });
        p.timeout = Some(11.0);
        p.started = Some(100.0);
        assert_eq!(timer(&p, 100.0).timeout, Some(1.0));
        assert_eq!(timer(&p, 105.0).timeout, Some(0.5));
        assert_eq!(timer(&p, 120.0).timeout, Some(0.0));
        p.timeout = None;
        assert_eq!(timer(&p, 105.0).timeout, None);
    }
}
