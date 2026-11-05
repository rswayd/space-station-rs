use bevy::ecs::error::{BevyError, ErrorContext};
use bevy::prelude::*;

mod local_server;

fn main() -> AppExit {
    // The release package run with no server address: a bundled server on loopback, stopped when
    // `local` drops after the app exits.
    let package = local_server::package_dir();
    let local = match package.as_deref().map(local_server::start) {
        Some(Ok(server)) => Some(server),
        Some(Err(e)) => {
            eprintln!("single-player launch failed: {e}");
            return AppExit::error();
        }
        None => None,
    };
    // `SS13_SCRIPT`: scripted run for visual checks (docs/RUNNING.md); absent, nothing changes.
    let script = ss13_devtools::ScriptPlugin::from_env();
    // Debug overlay (F9, docs/RUNNING.md "Debug UI"): debug builds, or `--features debug-ui`.
    let debug_ui = cfg!(debug_assertions) || cfg!(feature = "debug-ui");
    let defaults = DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "ss13-rs".into(),
            // BYOND widescreen view, 19x15 tiles at the world camera's zoom 2, left of the chat.
            resolution: (
                19 * 64 + ss13_chat::client::panel::PANEL_WIDTH as u32,
                15 * 64,
            )
                .into(),
            ..default()
        }),
        ..default()
    });
    // `SS13_PROFILE=1`: per-system time every 10 s (a `--features profile` build).
    let defaults = defaults.set(bevy::log::LogPlugin {
        custom_layer: if debug_ui {
            debug_layers
        } else {
            ss13_debug::profile::profile_layer
        },
        ..default()
    });
    let defaults = match &script {
        Some(script) => script.adjust_defaults(defaults),
        None => defaults,
    };
    // Same variable and default as the server's map loader.
    let asset_root: std::path::PathBuf = match (std::env::var_os("SS13_ASSETS"), &package) {
        (Some(root), _) => root.into(),
        (None, Some(dir)) => dir.join("assets"),
        (None, None) => ss13_render::DEFAULT_ASSET_ROOT.into(),
    };
    let mut app = App::new();
    if let Some(local) = &local {
        app.insert_resource(local.config.clone());
    }
    app.set_error_handler(log_errors_keep_panics)
        .add_plugins((
            defaults,
            ss13_net::client::NetClientPlugin,
            ss13_render::RenderPlugin {
                asset_root: asset_root.clone(),
            },
            // tg lighting from replicated `Optics`; `SS13_LIGHTING=0` turns it off.
            ss13_render::LightingPlugin,
            ss13_render::ParallaxPlugin,
            ss13_ui_kit::UiKitPlugin,
            ss13_tgui::client::TguiClientPlugin,
            ss13_game::client::GameClientPlugin,
            ss13_game::inventory_client::InventoryHudPlugin,
            ss13_game::storage_client::StorageClientPlugin,
            ss13_game::health_client::HealthHudPlugin,
            ss13_game::breath_client::AlertHudPlugin,
            ss13_game::interact_client::InteractHudPlugin,
            ss13_chat::client::ChatClientPlugin,
            ss13_chat::client::ChatPanelPlugin,
        ))
        .add_plugins(ss13_game::hunger_client::HungerHudPlugin)
        .add_plugins(ss13_game::mood_client::MoodHudPlugin)
        .add_plugins(ss13_game::fullscreen_client::FullscreenPlugin)
        .add_plugins(ss13_game::sound_client::SoundClientPlugin { asset_root })
        // F1 adminhelp and, for server-confirmed admins, the F7 admin panel: every build.
        .add_plugins(ss13_admin::client::AdminUiPlugin)
        .add_systems(Update, map_left_of_chat);
    // `SS13_LOG_FPS=1`: frame time to the log every second.
    if std::env::var_os("SS13_LOG_FPS").is_some() {
        app.add_plugins((
            bevy::diagnostic::FrameTimeDiagnosticsPlugin::default(),
            bevy::diagnostic::LogDiagnosticsPlugin::default(),
        ))
        .add_systems(Last, log_frame_max);
    }
    if debug_ui {
        app.add_plugins(ss13_debug::client::DebugUiPlugin);
    }
    if let Some(script) = script {
        app.add_plugins(script);
    }
    let exit = app.run();
    drop(local);
    exit
}

/// Wiring: the map viewport ends where the docked chat panel begins (ss13_render knows no chat).
fn map_left_of_chat(
    panels: Query<&ComputedNode, With<ss13_chat::client::ChatPanel>>,
    mut insets: ResMut<ss13_render::MapInsets>,
) {
    let right = panels
        .iter()
        .map(|node| node.size.x * node.inverse_scale_factor)
        .fold(0.0, f32::max);
    insets.set_if_neq(ss13_render::MapInsets { left: 0.0, right });
}

/// The longest frame of each second: the averages above hide a once-a-second hitch.
fn log_frame_max(time: Res<Time<Real>>, mut window: Local<(f64, f32, u32)>) {
    let (start, max, frames) = &mut *window;
    *max = max.max(time.delta_secs());
    *frames += 1;
    let now = time.elapsed_secs_f64();
    if now - *start >= 1.0 {
        info!("frame max {:.1} ms over {frames} frames", *max * 1000.0);
        *window = (now, 0.0, 0);
    }
}

/// A system error (the default handler panics on any) is logged so the client keeps running and
/// can show why. Real panics (they carry a payload) still unwind: swallowing them would hide bugs.
fn log_errors_keep_panics(mut error: BevyError, ctx: ErrorContext) {
    if let Some(payload) = error.take_payload() {
        std::panic::resume_unwind(payload);
    }
    bevy::ecs::error::error(error, ctx);
}

/// The debug overlay's log tail and, with `SS13_PROFILE`, the per-system profile.
fn debug_layers(app: &mut App) -> Option<bevy::log::BoxedLayer> {
    let layers: Vec<bevy::log::BoxedLayer> = [
        ss13_debug::client::log_layer(app),
        ss13_debug::profile::profile_layer(app),
    ]
    .into_iter()
    .flatten()
    .collect();
    Some(Box::new(layers))
}
