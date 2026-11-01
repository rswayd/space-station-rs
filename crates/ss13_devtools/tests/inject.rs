//! A script drives the real input paths in a headless app (no GPU, no OS window): held keys reach
//! `ButtonInput<KeyCode>` and the game's direction mapping, a UI click focuses the chat's say
//! line through picking, typed Cyrillic lands in the field and Enter sends it as a Say intent.

use std::time::Duration;

use bevy::render::RenderPlugin;
use bevy::render::settings::{RenderCreation, WgpuSettings};
use bevy::time::TimeUpdateStrategy;
use bevy::winit::WinitPlugin;
use ss13_chat::client::{ChatLog, ChatPanelPlugin};
use ss13_devtools::{ScriptPlugin, script};
use ss13_net::client::{NetClientState, NetEntityMap, SendIntent};
use ss13_shared::Dir;
use ss13_shared::protocol::ClientIntent;
use ss13_ui_kit::UiKitPlugin;
use ss13_ui_kit::prelude::*;

#[derive(Resource, Default)]
struct Seen {
    north_frames: usize,
    says: Vec<String>,
}

fn record(
    keys: Res<ButtonInput<KeyCode>>,
    mut intents: MessageReader<SendIntent>,
    mut seen: ResMut<Seen>,
) {
    if ss13_game::client::held_dir(&keys) == Some(Dir::North) {
        seen.north_frames += 1;
    }
    for SendIntent(intent) in intents.read() {
        if let ClientIntent::Say(text) = intent {
            seen.says.push(text.clone());
        }
    }
}

fn run(source: &str) -> (App, AppExit) {
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
        ScriptPlugin {
            lines: script::parse(source).unwrap(),
            out_dir: std::env::temp_dir(),
            offscreen: false,
        },
    ))
    .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
        50,
    )))
    .insert_resource(ChatLog::default())
    .insert_resource(NetClientState::Connected)
    .init_resource::<NetEntityMap>()
    .init_resource::<Seen>()
    .add_message::<SendIntent>()
    .add_systems(Last, record);
    app.world_mut().spawn(Camera2d);
    for _ in 0..400 {
        app.update();
        if let Some(exit) = app.should_exit() {
            return (app, exit);
        }
    }
    panic!("script did not finish in 400 frames");
}

#[test]
fn keys_clicks_and_cyrillic_text_take_the_real_input_paths() {
    let (app, exit) = run(r#"
        wait_connected timeout=1
        wait_secs 0.2           # panel laid out
        key_down W
        wait_secs 0.5
        key_up W
        click_ui "Сообщение"
        type "Привет, мир"
        tap Enter
        log_state
        quit
    "#);
    assert_eq!(exit, AppExit::Success);
    let seen = app.world().resource::<Seen>();
    // 0.5 s at 50 ms per frame, give or take the frames around the key events.
    assert!(
        (9..=12).contains(&seen.north_frames),
        "W held for {} frames",
        seen.north_frames
    );
    assert_eq!(seen.says, ["Привет, мир"]);
}

#[test]
fn a_timed_out_wait_fails_the_run() {
    let (app, exit) = run(r#"
        wait_until chat "never" timeout=0.3
        quit
    "#);
    assert_eq!(exit, AppExit::from_code(1));
    assert!(app.world().resource::<Seen>().says.is_empty());
}

#[test]
fn typing_without_a_focused_field_says_nothing() {
    let (app, exit) = run(r#"
        wait_secs 0.2
        type "Привет"
        tap Enter
        quit
    "#);
    assert_eq!(exit, AppExit::Success);
    assert!(app.world().resource::<Seen>().says.is_empty());
}
