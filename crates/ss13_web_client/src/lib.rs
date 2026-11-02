//! Browser demo, page side (`docs/WEB.md`): the client app of `ss13_client` on a WebGL2 canvas.
//! Its `NetClientPlugin` connects to a page-local [`listener`]; the bridge forwards every
//! connection to the worker's server over `postMessage`. Exports `start(canvas_selector, files,
//! post)` and `on_worker_message(msg)`.
#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;
use std::path::PathBuf;

use bevy::ecs::error::{BevyError, ErrorContext};
use bevy::prelude::*;
use js_sys::{Function, Map};
use ss13_net::loopback::{LoopbackListener, listener};
use ss13_web_common::{Bridge, Post};
use wasm_bindgen::prelude::*;

/// The page's half of the loopback: the listener the client connects to and the bridged ends.
struct PageBridge {
    listener: LoopbackListener,
    bridge: Bridge,
    next_id: u32,
}

thread_local! {
    static BRIDGE: RefCell<Option<PageBridge>> = const { RefCell::new(None) };
}

/// `canvas_selector`: the canvas to draw on (CSS selector); `files`: path → `Uint8Array` (client
/// and shared files of `data/files.json`); `post(msg, transfer)` posts to the worker. Call after
/// the worker's `ready`.
#[wasm_bindgen]
pub fn start(canvas_selector: String, files: Map, post: Function) {
    ss13_web_common::set_panic_hook(None);
    ss13_web_common::mount_files(files, Vec::new());
    let (listener, connector) = listener();
    BRIDGE.with(|b| {
        *b.borrow_mut() = Some(PageBridge {
            listener,
            bridge: Bridge::new(Post::new(post)),
            next_id: 0,
        })
    });
    let asset_root = PathBuf::new();
    let defaults = DefaultPlugins
        .set(WindowPlugin {
            primary_window: Some(Window {
                title: "ss13-rs".into(),
                canvas: Some(canvas_selector),
                fit_canvas_to_parent: true,
                // F1 adminhelp, F6/F7 admin keys and Tab must reach the game, not the browser.
                prevent_default_event_handling: true,
                ..default()
            }),
            ..default()
        })
        // The debug overlay (F9 or backtick, docs/RUNNING.md "Debug UI") with its log tail.
        .set(bevy::log::LogPlugin {
            custom_layer: ss13_debug::client::log_layer,
            ..default()
        });
    let mut app = App::new();
    app.set_error_handler(log_errors_keep_panics)
        .insert_resource(connector)
        .add_plugins((
            defaults,
            ss13_net::client::NetClientPlugin,
            ss13_render::RenderPlugin {
                asset_root: asset_root.clone(),
            },
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
        .add_plugins(ss13_game::sound_client::SoundClientPlugin { asset_root })
        .add_plugins(ss13_admin::client::AdminUiPlugin)
        .add_plugins(ss13_debug::client::DebugUiPlugin)
        .add_systems(Update, map_left_of_chat)
        .add_systems(Last, pump_bridge);
    // On wasm, winit's runner hands the app to the browser event loop and returns.
    app.run();
}

/// A worker message: `frame`/`dgram`/`close` for a bridged connection; the rest (`ready`,
/// `progress`, `error`) is for the page script.
#[wasm_bindgen]
pub fn on_worker_message(msg: JsValue) {
    BRIDGE.with(|b| {
        if let Some(page) = b.borrow_mut().as_mut() {
            page.bridge.apply(&msg);
        }
    });
}

/// Every frame: announces new client connections to the worker and forwards what the bridged
/// ends received.
fn pump_bridge() {
    BRIDGE.with(|b| {
        let mut b = b.borrow_mut();
        let Some(page) = b.as_mut() else {
            return;
        };
        while let Some(end) = page.listener.accept() {
            let id = page.next_id;
            page.next_id += 1;
            page.bridge.post().send("open", &[("id", id.into())], None);
            page.bridge.insert(id, end);
        }
        page.bridge.pump();
    });
}

/// As `ss13_client`: the map viewport ends where the docked chat panel begins.
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

/// As `ss13_client`: a system error is logged, real panics still unwind.
fn log_errors_keep_panics(mut error: BevyError, ctx: ErrorContext) {
    if let Some(payload) = error.take_payload() {
        std::panic::resume_unwind(payload);
    }
    bevy::ecs::error::error(error, ctx);
}
