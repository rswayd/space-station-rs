//! tg input modals in a headless client (real layout and text, no GPU): each opens from server
//! data and its buttons and keys send the acts `ss13_tgui::prompt` reads.

use std::time::Duration;

use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input_focus::InputFocus;
use bevy::render::RenderPlugin;
use bevy::render::settings::{RenderCreation, WgpuSettings};
use bevy::time::TimeUpdateStrategy;
use bevy::ui_widgets::Activate;
use bevy::winit::WinitPlugin;
use ss13_net::client::{NetClientState, SendIntent};
use ss13_net::ui::client::{SendUi, UiMessageReceived};
use ss13_shared::protocol::ClientIntent;
use ss13_shared::ui::{UiServerMessage, UiStatus};
use ss13_tgui::client::{ClientWindows, TguiClientPlugin};
use ss13_tgui::interfaces::alert::{AlertStatic, AlertUi, PromptTimer};
use ss13_tgui::interfaces::checkbox_input::{CheckboxInputStatic, CheckboxInputUi};
use ss13_tgui::interfaces::color_picker::{ColorPickerStatic, ColorPickerUi};
use ss13_tgui::interfaces::key_combo::{KeyComboStatic, KeyComboUi};
use ss13_tgui::interfaces::list_input::{ListInputData, ListInputUi};
use ss13_tgui::interfaces::number_input::{NumberInputStatic, NumberInputUi};
use ss13_tgui::interfaces::text_input::{TextInputStatic, TextInputUi};
use ss13_tgui::{UiInterface, encode};
use ss13_ui_kit::UiKitPlugin;
use ss13_ui_kit::prelude::*;

const WINDOW: u32 = 3;

/// Intents the client sent (messages only live two frames).
#[derive(Resource, Default)]
struct Sent(Vec<ClientIntent>);

fn headless() -> App {
    let mut app = App::new();
    app.add_plugins((
        DefaultPlugins
            .build()
            .disable::<WinitPlugin>()
            .disable::<bevy::log::LogPlugin>()
            .set(RenderPlugin {
                render_creation: RenderCreation::Automatic(Box::new(WgpuSettings {
                    backends: None,
                    ..default()
                })),
                ..default()
            }),
        UiKitPlugin,
        TguiClientPlugin,
    ))
    .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
        16,
    )))
    .insert_resource(NetClientState::Connected)
    .add_message::<UiMessageReceived>()
    .add_message::<SendUi>()
    .add_message::<SendIntent>()
    .init_resource::<Sent>()
    .add_systems(
        Last,
        |mut read: MessageReader<SendIntent>, mut sent: ResMut<Sent>| {
            sent.0.extend(read.read().map(|SendIntent(i)| i.clone()));
        },
    );
    app.world_mut().spawn(Camera2d);
    for _ in 0..3 {
        app.update();
    }
    app
}

fn open<T: UiInterface<Data = PromptTimer>>(app: &mut App, static_data: &T::StaticData) {
    open_with::<T>(app, static_data, &PromptTimer { timeout: Some(0.5) });
}

fn open_with<T: UiInterface>(app: &mut App, static_data: &T::StaticData, data: &T::Data) {
    app.world_mut()
        .write_message(UiMessageReceived(UiServerMessage::Open {
            window: WINDOW,
            interface: T::NAME.into(),
            title: "Вопрос".into(),
            status: UiStatus::Interactive,
            static_data: encode(static_data),
            data: encode(data),
        }));
    settle(app);
}

fn settle(app: &mut App) {
    for _ in 0..8 {
        app.update();
    }
}

fn root(app: &App) -> Entity {
    app.world().resource::<ClientWindows>().0[&WINDOW]
}

fn named(app: &mut App, name: &str) -> Entity {
    let root = root(app);
    let world = app.world_mut();
    let mut q = world.query::<(Entity, &Name)>();
    let found: Vec<Entity> = q
        .iter(world)
        .filter(|(e, n)| n.as_str() == name && is_in(world, *e, root))
        .map(|(e, _)| e)
        .collect();
    // The shown render is the last one spawned.
    *found.last().unwrap_or_else(|| panic!("no {name:?}"))
}

fn is_in(world: &World, mut e: Entity, root: Entity) -> bool {
    while let Some(parent) = world.get::<ChildOf>(e) {
        e = parent.parent();
        if e == root {
            return true;
        }
    }
    false
}

fn shows(app: &mut App, needle: &str) -> bool {
    let root = root(app);
    let world = app.world_mut();
    let mut q = world.query::<(Entity, &Text, &InheritedVisibility)>();
    q.iter(world)
        .any(|(e, t, v)| v.get() && t.0.contains(needle) && is_in(world, e, root))
}

fn click(app: &mut App, name: &str) {
    let entity = named(app, name);
    app.world_mut().trigger(Activate { entity });
    settle(app);
}

fn key(app: &mut App, code: KeyCode, logical: Key) {
    let window = app.world_mut().spawn_empty().id();
    for state in [ButtonState::Pressed, ButtonState::Released] {
        app.world_mut().write_message(KeyboardInput {
            key_code: code,
            logical_key: logical.clone(),
            state,
            text: None,
            repeat: false,
            window,
        });
        app.update();
    }
    settle(app);
}

/// Acts sent since the last call.
fn acts(app: &mut App) -> Vec<(String, Vec<(String, String)>)> {
    std::mem::take(&mut app.world_mut().resource_mut::<Sent>().0)
        .into_iter()
        .filter_map(|intent| match intent {
            ClientIntent::UiAct { action, params, .. } => Some((action, params)),
            _ => None,
        })
        .collect()
}

fn act(action: &str, params: &[(&str, &str)]) -> (String, Vec<(String, String)>) {
    (
        action.into(),
        params
            .iter()
            .map(|(k, v)| ((*k).into(), (*v).into()))
            .collect(),
    )
}

#[test]
fn alert_buttons_and_keys_choose() {
    let mut app = headless();
    open::<AlertUi>(
        &mut app,
        &AlertStatic {
            message: "Вы уверены?".into(),
            buttons: vec!["Да".into(), "Нет".into()],
            autofocus: true,
            large_buttons: false,
            swapped_buttons: true,
        },
    );
    assert!(shows(&mut app, "Вы уверены?"));
    assert!(shows(&mut app, "Нет"));
    // Autofocus: the window takes the keyboard.
    let focus = app.world().resource::<InputFocus>().get();
    let name = focus
        .and_then(|f| app.world().get::<Name>(f))
        .map(|n| n.to_string());
    assert_eq!(focus, Some(root(&app)), "focus on {name:?}");
    acts(&mut app);
    click(&mut app, "alert button 2");
    assert_eq!(acts(&mut app), [act("choose", &[("choice", "Нет")])]);
    // Right moves the selection to the second button, Enter picks it, Escape cancels.
    key(&mut app, KeyCode::ArrowRight, Key::ArrowRight);
    key(&mut app, KeyCode::Enter, Key::Enter);
    assert_eq!(acts(&mut app), [act("choose", &[("choice", "Нет")])]);
    key(&mut app, KeyCode::Escape, Key::Escape);
    assert_eq!(acts(&mut app), [act("cancel", &[])]);
}

#[test]
fn text_input_submits_the_default_until_edited() {
    let mut app = headless();
    open::<TextInputUi>(
        &mut app,
        &TextInputStatic {
            message: "Новое имя".into(),
            placeholder: "Боб".into(),
            max_length: 42,
            ..Default::default()
        },
    );
    assert!(shows(&mut app, "Новое имя"));
    assert!(shows(&mut app, "3/42"));
    acts(&mut app);
    click(&mut app, "input: submit");
    assert_eq!(acts(&mut app), [act("submit", &[("entry", "Боб")])]);
    click(&mut app, "input: cancel");
    assert_eq!(acts(&mut app), [act("cancel", &[])]);
}

#[test]
fn number_input_steps_and_submits() {
    let mut app = headless();
    open::<NumberInputUi>(
        &mut app,
        &NumberInputStatic {
            message: "Сколько?".into(),
            init_value: 5.0,
            min_value: 0.0,
            max_value: 10.0,
            round_value: true,
            ..Default::default()
        },
    );
    acts(&mut app);
    let up = {
        let root = root(&app);
        let world = app.world_mut();
        let mut q = world.query::<(Entity, &ss13_tgui::client::SetLocal)>();
        let ups: Vec<Entity> = q
            .iter(world)
            .filter(|(e, s)| s.value == "6" && is_in(world, *e, root))
            .map(|(e, _)| e)
            .collect();
        *ups.last().expect("a +1 button")
    };
    app.world_mut().trigger(Activate { entity: up });
    settle(&mut app);
    click(&mut app, "input: submit");
    assert_eq!(acts(&mut app), [act("submit", &[("entry", "6")])]);
}

#[test]
fn checkboxes_toggle_and_submit_indices() {
    let mut app = headless();
    open::<CheckboxInputUi>(
        &mut app,
        &CheckboxInputStatic {
            message: "Выберите".into(),
            items: vec!["Альфа".into(), "Бета".into(), "Гамма".into()],
            min_checked: 1,
            max_checked: 2,
            ..Default::default()
        },
    );
    assert!(shows(&mut app, "(Min: 1) (Max: 2)"));
    acts(&mut app);
    click(&mut app, "checkbox 3");
    click(&mut app, "checkbox 1");
    click(&mut app, "input: submit");
    assert_eq!(acts(&mut app), [act("submit", &[("entry", "2,0")])]);
}

#[test]
fn key_combo_catches_the_next_key() {
    let mut app = headless();
    open::<KeyComboUi>(
        &mut app,
        &KeyComboStatic {
            message: "Нажмите клавишу".into(),
            init_value: "F1".into(),
            ..Default::default()
        },
    );
    assert!(shows(&mut app, "Ожидание ввода..."));
    acts(&mut app);
    key(&mut app, KeyCode::KeyG, Key::Character("g".into()));
    assert!(shows(&mut app, "G"));
    key(&mut app, KeyCode::Enter, Key::Enter);
    assert_eq!(acts(&mut app), [act("submit", &[("entry", "G")])]);
}

#[test]
fn list_click_selects_and_submit_or_arrows_pick() {
    let mut app = headless();
    let items: Vec<String> = (1..=12).map(|i| format!("пункт {i}")).collect();
    let data = ListInputData {
        title: String::new(),
        message: "Куда?".into(),
        init_value: items[0].clone(),
        items,
        timeout: None,
    };
    open_with::<ListInputUi>(&mut app, &(), &data);
    // Long lists get a search field; labels start with a capital.
    named(&mut app, "list input: search");
    assert!(shows(&mut app, "Пункт 3"));
    acts(&mut app);
    click(&mut app, "list item 3");
    assert!(acts(&mut app).is_empty(), "a click only selects");
    click(&mut app, "input: submit");
    assert_eq!(acts(&mut app), [act("submit", &[("entry", "пункт 3")])]);
    // The window has the keyboard after a click on it.
    let root = root(&app);
    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(root, bevy::input_focus::FocusCause::Pressed);
    key(&mut app, KeyCode::ArrowDown, Key::ArrowDown);
    key(&mut app, KeyCode::Enter, Key::Enter);
    assert_eq!(acts(&mut app), [act("submit", &[("entry", "пункт 4")])]);
}

#[test]
fn color_picker_edits_by_hex_preset_and_channel() {
    let mut app = headless();
    open::<ColorPickerUi>(
        &mut app,
        &ColorPickerStatic {
            message: "Цвет волос".into(),
            default_color: "#336699".into(),
            autofocus: true,
            ..Default::default()
        },
    );
    assert!(shows(&mut app, "Цвет волос"));
    acts(&mut app);
    click(&mut app, "input: submit");
    assert_eq!(acts(&mut app), [act("submit", &[("entry", "#336699")])]);

    // Typing a hex code.
    let hex = named(&mut app, "color: hex");
    app.world_mut().trigger(ss13_ui_kit::InputChanged {
        entity: hex,
        value: "ff0000".into(),
    });
    settle(&mut app);
    click(&mut app, "input: submit");
    assert_eq!(acts(&mut app), [act("submit", &[("entry", "#ff0000")])]);

    // The value channel: red at half brightness.
    let value = {
        let root = root(&app);
        let world = app.world_mut();
        let mut q = world.query::<(Entity, &ss13_tgui::interfaces::color_picker::ColorChannel)>();
        let found: Vec<Entity> = q
            .iter(world)
            .filter(|(e, c)| {
                **c == ss13_tgui::interfaces::color_picker::ColorChannel::V
                    && is_in(world, *e, root)
            })
            .map(|(e, _)| e)
            .collect();
        *found.last().unwrap()
    };
    app.world_mut().trigger(ss13_ui_kit::NumberInputChanged {
        entity: value,
        value: 50.0,
    });
    settle(&mut app);
    click(&mut app, "input: submit");
    assert_eq!(acts(&mut app), [act("submit", &[("entry", "#800000")])]);
}

#[test]
fn list_arrows_follow_the_search_and_letters_jump_in_hotkey_mode() {
    let mut app = headless();
    let items: Vec<String> = ["Бар", "Мостик", "Медблок", "Морг", "Бриг"]
        .map(String::from)
        .to_vec();
    let data = ListInputData {
        title: String::new(),
        message: "Куда?".into(),
        init_value: items[0].clone(),
        items,
        timeout: None,
    };
    open_with::<ListInputUi>(&mut app, &(), &data);
    // Five items: no search bar, the window takes the keyboard and letters jump.
    assert_eq!(app.world().resource::<InputFocus>().get(), Some(root(&app)));
    acts(&mut app);
    key(&mut app, KeyCode::KeyM, Key::Character("м".into()));
    key(&mut app, KeyCode::Enter, Key::Enter);
    assert_eq!(acts(&mut app), [act("submit", &[("entry", "Мостик")])]);
    // A double click submits the row at once.
    let row = named(&mut app, "list item 4");
    app.world_mut()
        .trigger(bevy::picking::events::PointerClick {
            entity: row,
            pointer: bevy::picking::events::Pointer::new(
                bevy::picking::pointer::PointerId::Mouse,
                bevy::picking::pointer::Location {
                    target: bevy::camera::NormalizedRenderTarget::None {
                        width: 1,
                        height: 1,
                    },
                    position: Vec2::ZERO,
                },
            ),
            button: bevy::picking::pointer::PointerButton::Primary,
            hit: bevy::picking::backend::HitData::new(row, 0.0, None, None),
            duration: Duration::from_millis(80),
            count: 2,
        });
    settle(&mut app);
    assert_eq!(acts(&mut app), [act("submit", &[("entry", "Морг")])]);
    // Search mode: arrows skip what the query hides.
    click(&mut app, "list input: mode");
    let search = named(&mut app, "list input: search");
    app.world_mut().trigger(ss13_ui_kit::InputChanged {
        entity: search,
        value: "б".into(),
    });
    settle(&mut app);
    let root = root(&app);
    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(root, bevy::input_focus::FocusCause::Pressed);
    key(&mut app, KeyCode::ArrowDown, Key::ArrowDown);
    key(&mut app, KeyCode::Enter, Key::Enter);
    assert_eq!(acts(&mut app), [act("submit", &[("entry", "Бар")])]);
}
