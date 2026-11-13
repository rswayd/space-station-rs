//! The chat panel in a headless Bevy app with real UI layout and text shaping (no GPU, no OS
//! window): line breaking, styled rows, scroll tracking against real layout, eviction, and the
//! frame cost with 10k messages.

use std::time::{Duration, Instant};

use bevy::camera::NormalizedRenderTarget;
use bevy::input::mouse::MouseScrollUnit;
use bevy::input::touch::TouchPhase;
use bevy::picking::pointer::{Location, PointerAction, PointerId, PointerInput};
use bevy::render::RenderPlugin;
use bevy::render::settings::{RenderCreation, WgpuSettings};
use bevy::time::TimeUpdateStrategy;
use bevy::ui::{ComputedNode, UiGlobalTransform};
use bevy::ui_widgets::Activate;
use bevy::window::PrimaryWindow;
use bevy::winit::WinitPlugin;
use ss13_chat::client::panel::{ChatMessages, JumpButton, LINE_HEIGHT};
use ss13_chat::client::prefs::{Highlight, row_height};
use ss13_chat::client::settings_ui::PageTab;
use ss13_chat::client::{ChatLog, ChatPanelPlugin, ChatPrefs, ChatView};
use ss13_net::client::SendIntent;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_ui_kit::UiKitPlugin;
use ss13_ui_kit::prelude::*;
use ss13_ui_kit::scroll::ScrollContent;
use ss13_ui_kit::virtual_list::VirtualListRow;
use ss13_ui_kit::virtual_list::VirtualListState;

fn headless(capacity: usize) -> App {
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
        ChatPanelPlugin,
    ))
    .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
        50,
    )))
    .insert_resource(ChatLog::with_capacity(capacity))
    .add_message::<SendIntent>();
    app.world_mut().spawn(Camera2d);
    settle(&mut app, 3);
    app
}

fn settle(app: &mut App, frames: usize) {
    for _ in 0..frames {
        app.update();
    }
}

fn single<C: Component>(app: &mut App) -> Entity {
    app.world_mut()
        .query_filtered::<Entity, With<C>>()
        .single(app.world())
        .unwrap()
}

fn push(app: &mut App, message: ChatMessage) {
    let now = app.world().resource::<Time>().elapsed_secs_f64();
    app.world_mut().resource_mut::<ChatLog>().push(message, now);
}

fn say(name: &str, text: &str) -> ChatMessage {
    ChatMessage {
        channel: ChatChannel::Local,
        segments: vec![
            ChatSegment::new(Span::Name, name),
            ChatSegment::new(Span::Say, " говорит, \""),
            ChatSegment::new(Span::Say, text),
            ChatSegment::new(Span::Say, "\""),
        ],
    }
}

/// (scroll offset, view height) of the list.
fn scroll(app: &mut App) -> (f32, f32) {
    let content = single::<ScrollContent>(app);
    let y = app.world().get::<ScrollPosition>(content).unwrap().y;
    let node = app.world().get::<ComputedNode>(content).unwrap();
    (y, node.size.y * node.inverse_scale_factor)
}

fn lines(app: &App) -> usize {
    app.world().resource::<ChatView>().lines().len()
}

fn rows(app: &mut App) -> usize {
    app.world_mut()
        .query::<&VirtualListRow>()
        .iter(app.world())
        .count()
}

fn spans(app: &mut App) -> Vec<String> {
    app.world_mut()
        .query::<&TextSpan>()
        .iter(app.world())
        .map(|t| t.0.clone())
        .collect()
}

fn jump_shown(app: &mut App) -> bool {
    let button = single::<JumpButton>(app);
    app.world().get::<Node>(button).unwrap().display != Display::None
}

fn wheel(app: &mut App, lines: f32) {
    let list = single::<ChatMessages>(app);
    let node = app.world().get::<ComputedNode>(list).unwrap();
    let transform = app.world().get::<UiGlobalTransform>(list).unwrap();
    let position = transform.translation * node.inverse_scale_factor;
    let window = single::<PrimaryWindow>(app);
    let target = NormalizedRenderTarget::Window(
        bevy::window::WindowRef::Primary
            .normalize(Some(window))
            .unwrap(),
    );
    for action in [
        PointerAction::Move { delta: Vec2::ZERO },
        PointerAction::Scroll {
            unit: MouseScrollUnit::Line,
            x: 0.0,
            y: lines,
            phase: TouchPhase::Moved,
        },
    ] {
        app.world_mut().write_message(PointerInput::new(
            PointerId::Mouse,
            Location {
                target: target.clone(),
                position,
            },
            action,
        ));
        app.update();
    }
}

#[test]
fn long_cyrillic_message_wraps_into_styled_rows() {
    let mut app = headless(2500);
    let text = "Внимание экипажу станции: ".repeat(12) + &"Ы".repeat(150);
    push(&mut app, say("Капитан", &text));
    settle(&mut app, 3);
    let shown = app.world().resource::<ChatView>().lines();
    assert!(shown.len() >= 4, "{} lines", shown.len());
    // The lines put back together are the message; spans survive the breaks.
    let joined: String = shown
        .iter()
        .flat_map(|l| l.pieces.iter().map(|(_, t)| t.as_str()))
        .collect();
    assert_eq!(joined, format!("Капитан говорит, \"{text}\""));
    assert_eq!(shown[0].pieces[0], (Span::Name, "Капитан".to_string()));
    assert!(
        shown[1..]
            .iter()
            .all(|l| l.pieces.iter().all(|(s, _)| *s == Span::Say))
    );
    // An unbreakable run of 150 letters is split anywhere (`word-wrap: break-word`).
    assert!(
        shown
            .last()
            .unwrap()
            .pieces
            .iter()
            .all(|(_, t)| t.chars().count() < 150)
    );
    // Every line is drawn: one row per line, its pieces as `TextSpan`s.
    assert_eq!(rows(&mut app), shown.len());
    assert!(spans(&mut app).contains(&"Капитан".to_string()));

    // A repeat combines: the last line gets an `x2` badge, no new message.
    push(&mut app, say("Капитан", &text));
    settle(&mut app, 2);
    let after = app.world().resource::<ChatView>().lines();
    assert_eq!(after.last().unwrap().badge, Some(2));
    assert!(after.iter().all(|l| l.id == shown[0].id));
    let badges = app
        .world_mut()
        .query::<&Text>()
        .iter(app.world())
        .filter(|t| t.0 == "x2")
        .count();
    assert_eq!(badges, 1);

    // A new tab shows only system messages: local speech is hidden.
    app.world_mut().resource_mut::<ChatPrefs>().pages.add();
    settle(&mut app, 2);
    assert_eq!(lines(&app), 0);
    assert_eq!(rows(&mut app), 0);
    let tabs = app
        .world_mut()
        .query::<&PageTab>()
        .iter(app.world())
        .count();
    assert_eq!(tabs, 2);
    // New speech there is unread on the main tab, and its label says so.
    push(&mut app, say("Боцман", "эй"));
    settle(&mut app, 2);
    assert_eq!(app.world().resource::<ChatPrefs>().pages.pages[0].unread, 1);
    assert!(
        app.world_mut()
            .query::<&Text>()
            .iter(app.world())
            .any(|t| t.0 == "Основная (1)")
    );
    // Back on the main tab: read, and the speech is shown again.
    app.world_mut().resource_mut::<ChatPrefs>().pages.select(0);
    settle(&mut app, 2);
    assert_eq!(app.world().resource::<ChatPrefs>().pages.pages[0].unread, 0);
    assert!(lines(&app) > 0);
}

#[test]
fn highlights_and_font_size_reach_the_rows() {
    let mut app = headless(2500);
    app.world_mut().resource_mut::<ChatPrefs>().highlights = vec![Highlight {
        text: "капитан".into(),
        ..Default::default()
    }];
    settle(&mut app, 1);
    push(&mut app, say("Боб", "где капитан?"));
    settle(&mut app, 3);
    let shown = app.world().resource::<ChatView>().lines();
    let line = &shown[0];
    assert!(line.highlight.is_some(), "the whole message");
    let marked: Vec<&str> = line
        .pieces
        .iter()
        .zip(&line.marks)
        .filter(|(_, m)| m.is_some())
        .map(|((_, t), _)| t.as_str())
        .collect();
    assert_eq!(marked, ["капитан"]);
    // A bigger font: rows are laid out again, taller.
    app.world_mut().resource_mut::<ChatPrefs>().font_size = 20.0;
    settle(&mut app, 3);
    let list = single::<ChatMessages>(&mut app);
    let state = app.world().get::<VirtualListState>(list).unwrap();
    assert_eq!(state.item_height, row_height(20.0, 1.2));
    assert!(
        app.world()
            .resource::<ChatView>()
            .lines()
            .iter()
            .all(|l| l.font_size == 20.0)
    );
}

#[test]
fn multi_line_messages_get_one_row_per_line() {
    let mut app = headless(2500);
    // Health and gas analyzer readouts: one message, lines separated by `\n` inside segments.
    let scan = ChatMessage {
        channel: ChatChannel::Info,
        segments: vec![
            ChatSegment::new(Span::Info, "Анализ результатов для "),
            ChatSegment::new(Span::Bold, "Боб"),
            ChatSegment::new(Span::Info, ":"),
            ChatSegment::new(Span::Info, "\nОбщее состояние: 70% здоровья"),
            ChatSegment::new(Span::Warning, "\nГрудь: 30 / 0 / 0 / 0"),
            ChatSegment::new(Span::Info, "\n\nТемпература тела: 37 °C (98.6 °F)"),
        ],
    };
    push(&mut app, scan);
    settle(&mut app, 3);
    let shown = app.world().resource::<ChatView>().lines();
    let texts: Vec<String> = shown
        .iter()
        .map(|l| l.pieces.iter().map(|(_, t)| t.as_str()).collect())
        .collect();
    assert_eq!(
        texts,
        [
            "Анализ результатов для Боб:",
            "Общее состояние: 70% здоровья",
            "Грудь: 30 / 0 / 0 / 0",
            "",
            "Температура тела: 37 °C (98.6 °F)",
        ]
    );
    assert_eq!(shown[1].pieces[0].0, Span::Info);
    assert_eq!(shown[2].pieces[0].0, Span::Warning);
    // Every row's text fits its fixed row height: no line break left inside a row.
    settle(&mut app, 2);
    let heights: Vec<f32> = app
        .world_mut()
        .query_filtered::<&ComputedNode, With<Text>>()
        .iter(app.world())
        .map(|n| n.size.y * n.inverse_scale_factor)
        .collect();
    assert!(!heights.is_empty());
    assert!(
        heights.iter().all(|&h| h <= LINE_HEIGHT + 0.5),
        "{heights:?}"
    );
}

#[test]
fn follows_new_messages_until_scrolled_up_and_jump_returns() {
    let mut app = headless(2500);
    for i in 0..200 {
        push(&mut app, say("Боб", &format!("сообщение {i}")));
    }
    settle(&mut app, 3);
    let (y, view) = scroll(&mut app);
    assert_eq!(lines(&app), 200);
    assert_eq!(y, 200.0 * LINE_HEIGHT - view);
    assert!(!jump_shown(&mut app));

    // New messages keep the bottom in view.
    push(&mut app, say("Боб", "ещё"));
    settle(&mut app, 2);
    assert_eq!(scroll(&mut app).0, 201.0 * LINE_HEIGHT - view);

    // Scrolled up: the view holds still and the button offers the way back.
    wheel(&mut app, 5.0);
    settle(&mut app, 2);
    let held = scroll(&mut app).0;
    assert!(held < 201.0 * LINE_HEIGHT - view - LINE_HEIGHT, "{held}");
    for i in 0..3 {
        push(&mut app, say("Алиса", &format!("пока вы читали {i}")));
        settle(&mut app, 1);
    }
    settle(&mut app, 2);
    assert_eq!(scroll(&mut app).0, held);
    assert!(jump_shown(&mut app));
    assert_eq!(app.world().resource::<ChatView>().autoscroll.unread, 3);

    let button = single::<JumpButton>(&mut app);
    app.world_mut().trigger(Activate { entity: button });
    settle(&mut app, 3);
    assert_eq!(scroll(&mut app).0, 204.0 * LINE_HEIGHT - view);
    assert!(!jump_shown(&mut app));
}

#[test]
fn eviction_keeps_the_cap_and_redraws_shifted_rows() {
    let mut app = headless(50);
    for i in 0..120 {
        push(&mut app, say("Боб", &format!("номер {i}")));
        settle(&mut app, 1);
    }
    settle(&mut app, 2);
    // At most the cap; crossing it evicts down to 45.
    let kept = app.world().resource::<ChatLog>().len();
    assert!((45..=50).contains(&kept), "{kept}");
    assert_eq!(lines(&app), kept);
    let shown = spans(&mut app);
    assert!(shown.contains(&"номер 119".to_string()));
    // Rows drawn before the evictions were respawned with their new content.
    let evicted = |t: &String| {
        t.strip_prefix("номер ")
            .is_some_and(|n| n.parse::<usize>().unwrap() < 120 - kept)
    };
    assert!(!shown.iter().any(evicted), "{shown:?}");
}

#[test]
fn ten_thousand_messages_frame_cost() {
    let frames = 60;
    let mut empty = headless(10);
    let start = Instant::now();
    settle(&mut empty, frames);
    let baseline = start.elapsed() / frames as u32;

    let mut app = headless(10_000);
    for i in 0..10_000 {
        let text = format!("сообщение номер {i} с текстом средней длины");
        push(&mut app, say("Боб", &text));
    }
    let start = Instant::now();
    app.update();
    let first = start.elapsed();
    settle(&mut app, 2);
    assert_eq!(lines(&app), 10_000);
    let drawn = rows(&mut app);
    assert!(drawn < 80, "{drawn} rows");

    let start = Instant::now();
    settle(&mut app, frames);
    let idle = start.elapsed() / frames as u32;

    // One new message every frame: the view follows the bottom.
    let frame_of = |app: &mut App, i: usize| {
        push(app, say("Алиса", &format!("новое {i}")));
        let start = Instant::now();
        app.update();
        start.elapsed()
    };
    let mut times: Vec<Duration> = (0..frames).map(|i| frame_of(&mut app, i)).collect();
    // The frame that crosses the cap evicts a thousand messages and redraws every row.
    let eviction = times.iter().max().copied().unwrap();
    times.sort();
    let busy = times[frames / 2];
    assert!(rows(&mut app) < 80);
    println!(
        "10k messages: first build {first:?}; per frame: idle {idle:?}, one new message per \
         frame {busy:?} (median), eviction frame {eviction:?}; empty panel {baseline:?}; \
         {drawn} rows drawn"
    );
}
