//! Smoke test of every tgui view the client registers (`TguiClientPlugin` and the game's client
//! plugins): each opens in a headless Bevy app (real UI layout and text, no GPU, no OS window)
//! without a panic, its window root spawns and its render has child nodes. Server tests check the
//! data only; a broken scene was otherwise found by a player.
//!
//! The views are read from the registry the real plugins fill, so a new view is covered without
//! touching this file. Data per view: its gallery sample (`ss13_tgui::samples`) where it has one,
//! plus two generic postcard streams that decode as almost any data type without a sample:
//! all zero bytes (every number 0, every list and string empty, `None`, the first enum variant: the
//! `Default`-like empty state) and all `0x01` bytes (every list one long, `Some`, `true`, the
//! second enum variant: the non-empty paths). Static data goes through the same streams.

use std::any::Any;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::Duration;

use bevy::prelude::*;
use bevy::render::RenderPlugin;
use bevy::render::settings::{RenderCreation, WgpuSettings};
use bevy::time::TimeUpdateStrategy;
use bevy::winit::WinitPlugin;
use ss13_net::client::{NetClientState, SendIntent};
use ss13_net::ui::client::{SendUi, UiMessageReceived};
use ss13_shared::ui::{UiServerMessage, UiStatus};
use ss13_tgui::client::{
    ClientWindow, ClientWindows, TguiClientPlugin, copy_views, view_decodes, view_names,
};
use ss13_tgui::samples::sample;
use ss13_ui_kit::UiKitPlugin;

/// Long enough for any data type of a window (each field takes at least one byte).
const STREAM_LEN: usize = 16 * 1024;
/// Frames an open may take to show its first render (the scene, its slot, settling).
const FRAMES: usize = 20;

/// The client's tgui half with every view the game registers, without the game's systems (they
/// need the renderer, the net client and the map).
fn smoke_app() -> App {
    let mut game = App::new();
    game.add_plugins(ss13_game::client::GameClientPlugin);

    let mut app = App::new();
    app.add_plugins((
        DefaultPlugins
            .build()
            .disable::<WinitPlugin>()
            .disable::<bevy::log::LogPlugin>()
            .disable::<bevy::audio::AudioPlugin>()
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
    .add_message::<SendIntent>();
    copy_views(game.world(), app.world_mut());
    app.world_mut().spawn(Camera2d);
    for _ in 0..3 {
        app.update();
    }
    app
}

/// One way to open a view.
struct Case {
    label: &'static str,
    static_data: Vec<u8>,
    data: Vec<u8>,
}

/// The cases of `name` whose data decodes, and the labels of those that do not.
fn cases(world: &World, name: &str) -> (Vec<Case>, Vec<&'static str>) {
    let mut all = Vec::new();
    if let Some(s) = sample(name) {
        all.push(Case {
            label: "sample",
            static_data: s.static_data,
            data: s.data,
        });
    }
    for (label, byte) in [("zeroed", 0u8), ("ones", 1u8)] {
        all.push(Case {
            label,
            static_data: vec![byte; STREAM_LEN],
            data: vec![byte; STREAM_LEN],
        });
    }
    let (ok, bad): (Vec<Case>, Vec<Case>) = all
        .into_iter()
        .partition(|c| view_decodes(world, name, &c.static_data, &c.data));
    (ok, bad.into_iter().map(|c| c.label).collect())
}

fn window_root(app: &App, window: u32) -> Option<Entity> {
    app.world()
        .resource::<ClientWindows>()
        .0
        .get(&window)
        .copied()
}

fn send(app: &mut App, message: UiServerMessage) {
    app.world_mut().write_message(UiMessageReceived(message));
}

/// Opens `name` with the case's data as window `window` and checks what it shows.
fn check(app: &mut App, window: u32, name: &str, case: &Case) -> Result<(), String> {
    send(
        app,
        UiServerMessage::Open {
            window,
            interface: name.into(),
            title: name.into(),
            status: UiStatus::Interactive,
            static_data: case.static_data.clone(),
            data: case.data.clone(),
        },
    );
    let mut shown = None;
    for _ in 0..FRAMES {
        app.update();
        shown = window_root(app, window)
            .and_then(|root| app.world().get::<ClientWindow>(root))
            .and_then(ClientWindow::shown);
        if shown.is_some() {
            break;
        }
    }
    let world = app.world();
    let root = window_root(app, window).ok_or("the window root never spawned")?;
    if world.get::<Node>(root).is_none() {
        return Err("the window root is not a UI node".into());
    }
    let holder = shown.ok_or(format!("no render shown within {FRAMES} frames"))?;
    let content = world
        .get::<Children>(holder)
        .and_then(|c| c.first().copied())
        .ok_or("the render spawned nothing (a scene error is logged, not raised)")?;
    let mut nodes = 0;
    let mut stack: Vec<Entity> = world
        .get::<Children>(content)
        .map(|c| c.iter().collect())
        .unwrap_or_default();
    while let Some(e) = stack.pop() {
        if world.get::<Node>(e).is_some() {
            nodes += 1;
        }
        if let Some(children) = world.get::<Children>(e) {
            stack.extend(children.iter());
        }
    }
    if nodes == 0 {
        return Err("the view's root has no child nodes".into());
    }
    Ok(())
}

fn panic_text(payload: &(dyn Any + Send)) -> String {
    payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_owned()))
        .unwrap_or_else(|| "non-string panic".to_owned())
}

#[test]
fn every_registered_view_opens_headless_with_content() {
    let mut app = smoke_app();
    let names = view_names(app.world());
    // The tgui crate's views and the game's; a drop means a client plugin stopped registering.
    assert!(names.len() >= 90, "{} views: {names:?}", names.len());
    let mut failures = Vec::new();
    let mut undecodable = Vec::new();
    let mut window = 100;
    for name in names {
        let (runs, bad) = cases(app.world(), name);
        if runs.is_empty() {
            failures.push(format!(
                "{name}: neither a sample nor the generic streams decode; add a sample to \
                 ss13_tgui::samples"
            ));
            continue;
        }
        if !bad.is_empty() {
            undecodable.push(format!("{name}: {bad:?}"));
        }
        for case in &runs {
            window += 1;
            match catch_unwind(AssertUnwindSafe(|| check(&mut app, window, name, case))) {
                Ok(result) => {
                    if let Err(e) = result {
                        failures.push(format!("{name} ({}): {e}", case.label));
                    }
                    send(&mut app, UiServerMessage::Close { window });
                    app.update();
                }
                Err(payload) => {
                    failures.push(format!(
                        "{name} ({}): panicked: {}",
                        case.label,
                        panic_text(payload.as_ref())
                    ));
                    // The world a system panicked in is not trusted further.
                    app = smoke_app();
                }
            }
        }
    }
    // Not failures: the other cases of these views ran.
    if !undecodable.is_empty() {
        eprintln!(
            "generic data streams that do not decode (covered by the other cases):\n{}",
            undecodable.join("\n")
        );
    }
    assert!(
        failures.is_empty(),
        "{} view cases failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
