//! Browser demo, worker side (`docs/WEB.md`): the server app of `ss13_server` on MetaStation,
//! without sockets: a [`LoopbackListener`] whose connector opens one connection per page `open`,
//! ticked every 50 ms by its own timer (Bevy's schedule runner needs a `window`, a worker has
//! none). Exports `start(files, post, source_url?)` and `on_page_message(msg)`.
#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;
use std::path::PathBuf;

use bevy::app::PluginsState;
use bevy::log::LogPlugin;
use bevy::platform::time::Instant;
use bevy::prelude::*;
use js_sys::{Function, Map};
use ss13_net::loopback::{LoopbackConnector, LoopbackListener, listener};
use ss13_web_common::{Bridge, Post};
use wasm_bindgen::prelude::*;

/// The bundle's BYOND map files live under this prefix (`scripts/demo-package.sh` layout).
const SOURCE_DIR: &str = "BandaStation";
/// Ticks per logged timing line.
const STATS_TICKS: usize = 200;

struct Server {
    app: App,
    connector: LoopbackConnector,
    bridge: Bridge,
    ticks: Vec<f64>,
    source_url: Option<String>,
}

thread_local! {
    static SERVER: RefCell<Option<Server>> = const { RefCell::new(None) };
}

/// `files`: path → `Uint8Array` (server and shared files of `data/files.json`); `post(msg,
/// transfer)` posts to the page; `source_url` (optional): where players get the source (AGPL-3.0
/// §13), shown in the lobby; none: no source line. Returns at once; the build and map load run in the next task
/// (`progress` messages, then `ready`, or `error`).
#[wasm_bindgen]
pub fn start(files: Map, post: Function, source_url: Option<String>) {
    let source_url = source_url.filter(|u| !u.trim().is_empty());
    let post = Post::new(post);
    ss13_web_common::set_panic_hook(Some(post.clone()));
    ss13_web_common::mount_files(files, admin_config());
    post.send("progress", &[("stage", "server: prototypes".into())], None);
    ss13_web_common::after(0, move || {
        let built = Instant::now();
        let (listener, connector) = listener();
        let mut app = server_app(listener, source_url.clone(), true);
        post.send("progress", &[("stage", "server: map".into())], None);
        let Some(()) = finish(&mut app, &post) else {
            return;
        };
        let loaded = Instant::now();
        app.update();
        info!(
            "web server: built in {:.2} s, first update (map load, round start) {:.2} s",
            (loaded - built).as_secs_f64(),
            loaded.elapsed().as_secs_f64()
        );
        let bridge = Bridge::new(post.clone());
        SERVER.with(|s| {
            *s.borrow_mut() = Some(Server {
                app,
                connector,
                bridge,
                ticks: Vec::with_capacity(STATS_TICKS),
                source_url,
            })
        });
        post.send("ready", &[], None);
        let tick_ms = ss13_shared::protocol::TICK_DURATION.as_millis() as i32;
        ss13_web_common::every(tick_ms, tick);
    });
}

/// A page message: `open` connects a new loopback client; `frame`/`dgram`/`close` go to it.
#[wasm_bindgen]
pub fn on_page_message(msg: JsValue) {
    SERVER.with(|s| {
        let mut s = s.borrow_mut();
        let Some(server) = s.as_mut() else {
            return;
        };
        if server.bridge.apply(&msg) {
            return;
        }
        if ss13_web_common::msg_type(&msg).as_deref() != Some("open") {
            return;
        }
        let Some(id) = ss13_web_common::msg_id(&msg) else {
            return;
        };
        match server.connector.connect() {
            Some(end) => server.bridge.insert(id, end),
            None => server.bridge.post().send(
                "close",
                &[
                    ("id", id.into()),
                    // `protocol::close::NORMAL` (crate-private in ss13_net).
                    ("code", 0.into()),
                    ("reason", "server gone".into()),
                ],
                None,
            ),
        }
    });
}

fn tick() {
    SERVER.with(|s| {
        let mut s = s.borrow_mut();
        let Some(server) = s.as_mut() else {
            return;
        };
        let start = Instant::now();
        server.app.update();
        server.ticks.push(start.elapsed().as_secs_f64() * 1000.0);
        if let Some(exit) = server.app.should_exit() {
            if exit != AppExit::from_code(ss13_game::round::REBOOT_EXIT_CODE) {
                server.bridge.post().send(
                    "error",
                    &[("message", "the server stopped".into())],
                    None,
                );
                *s = None;
                return;
            }
            reboot(server);
        }
        server.bridge.pump();
        if server.ticks.len() >= STATS_TICKS {
            log_ticks(&mut server.ticks);
        }
    });
}

/// `ss13_game::round`: the round is over, a new lobby on a fresh world, as `ss13_server`'s
/// reboot loop. The listener moves over; the old connections end with the old app and the client
/// reconnects through the page bridge.
fn reboot(server: &mut Server) {
    let listener = server
        .app
        .world_mut()
        .remove_resource::<LoopbackListener>()
        .expect("the listener stays in the server app");
    let mut app = server_app(listener, server.source_url.clone(), false);
    if finish(&mut app, server.bridge.post()).is_some() {
        server.app = app;
        info!("web server: rebooted");
    }
}

fn log_ticks(ticks: &mut Vec<f64>) {
    ticks.sort_by(f64::total_cmp);
    let n = ticks.len();
    let avg = ticks.iter().sum::<f64>() / n as f64;
    let p95 = ticks[(n * 95 / 100).min(n - 1)];
    info!(
        "web server: {n} ticks, avg {avg:.2} ms, p95 {p95:.2} ms, max {:.2} ms",
        ticks[n - 1]
    );
    ticks.clear();
}

/// `App::run`'s preparation without its runner: we drive `update` ourselves.
fn finish(app: &mut App, post: &Post) -> Option<()> {
    if app.plugins_state() == PluginsState::Adding {
        post.send(
            "error",
            &[("message", "a server plugin is still loading".into())],
            None,
        );
        return None;
    }
    app.finish();
    app.cleanup();
    Some(())
}

/// The plugins of `ss13_server` with `SS13_MAP=MetaStation`, the single-player demo settings
/// (`ss13_client::local_server`) and a loopback listener instead of a socket.
fn server_app(listener: LoopbackListener, source_url: Option<String>, first: bool) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    // The global logger is set once per worker.
    if first {
        app.add_plugins(LogPlugin::default());
    }
    let map = ss13_mapload::MapLoadPlugin {
        source: SOURCE_DIR.into(),
        maps: vec!["MetaStation".into()],
        assets: PathBuf::new(),
    };
    app.insert_resource(listener)
        .insert_resource(ss13_admin::server::AdminConfig {
            config_dir: Some("config".into()),
            log_file: None,
            // Every client is this page's player (the bridge is the only way in).
            insecure_admins: true,
        })
        .insert_resource(ss13_game::round::RoundConfig {
            // The round starts at once; the player joins it from the lobby (latejoin).
            lobby_secs: 0.0,
            source_url,
            ..ss13_game::round::RoundConfig::tg()
        })
        .add_plugins((
            ss13_world::WorldPlugin,
            ss13_net::server::NetServerPlugin,
            ss13_net::server::ReplicationServerPlugin,
            ss13_tgui::server::TguiServerPlugin,
            ss13_chat::server::ChatServerPlugin,
            ss13_debug::server::DebugServerPlugin,
            ss13_admin::server::AdminServerPlugin,
            ss13_proto::ProtoPlugin {
                dir: map.prototypes_dir(),
            },
            map,
            ss13_atmos::AtmosPlugin,
            ss13_game::server::GameServerPlugin::default(),
        ));
    app
}

/// `config/admin_ranks.txt` as shipped, and the page's player (default token `dev`) as host.
fn admin_config() -> Vec<(String, Vec<u8>)> {
    vec![
        (
            "config/admin_ranks.txt".into(),
            include_bytes!("../../../config/admin_ranks.txt").to_vec(),
        ),
        ("config/admins.txt".into(), b"dev = Host\n".to_vec()),
    ]
}
