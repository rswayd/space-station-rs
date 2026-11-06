use std::time::{Duration, Instant};

use bevy::app::ScheduleRunnerPlugin;
use bevy::log::LogPlugin;
use bevy::prelude::*;

fn main() -> AppExit {
    // Set by the client's single-player launch: the parent holds our stdin open, so its end, even
    // when the parent is killed, ends this server too instead of leaving a hidden process behind.
    if std::env::var_os("SS13_EXIT_WITH_PARENT").is_some() {
        std::thread::spawn(|| {
            let _ = std::io::copy(&mut std::io::stdin(), &mut std::io::sink());
            std::process::exit(0);
        });
    }
    let mut first = true;
    loop {
        let exit = server(first).run();
        // `ss13_game::round`: the round is over, the world reboots into a new lobby.
        if exit != AppExit::from_code(ss13_game::round::REBOOT_EXIT_CODE) {
            return exit;
        }
        first = false;
        wait_for_port(ss13_net::server::NetServerConfig::default().bind_addr);
    }
}

fn server(first: bool) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(
        ss13_shared::protocol::TICK_DURATION,
    )));
    // The global logger is set once per process.
    if first {
        app.add_plugins(LogPlugin {
            // `SS13_PROFILE=1`: per-system time every 10 s (a `--features profile` build).
            custom_layer: ss13_debug::profile::profile_layer,
            ..Default::default()
        });
    }
    app.add_plugins((
        ss13_world::WorldPlugin,
        ss13_net::server::NetServerPlugin,
        ss13_net::server::ReplicationServerPlugin,
        ss13_tgui::server::TguiServerPlugin,
        ss13_chat::server::ChatServerPlugin,
        // Debug queries and dev tools: gated inside (debug builds / `SS13_DEBUG_QUERIES`).
        ss13_debug::server::DebugServerPlugin,
        // Admins (`SS13_CONFIG_DIR`), verbs, tickets, admin log: every build.
        ss13_admin::server::AdminServerPlugin,
    ));
    // SS13_METRICS_ADDR: Prometheus `/metrics` (off when unset).
    #[cfg(not(target_arch = "wasm32"))]
    app.add_plugins(ss13_debug::metrics::MetricsPlugin);
    // SS13_MAP=MetaStation (see `MapLoadPlugin::from_env`); unset = empty world.
    if let Some(map) = ss13_mapload::MapLoadPlugin::from_env() {
        // The station's mining level (lavaland); `SS13_MINING_LEVEL=0` loads none.
        if let Some(mining) = ss13_mapload::MiningLevel::from_env(&map) {
            app.insert_resource(mining);
        }
        app.add_plugins((
            ss13_proto::ProtoPlugin {
                dir: map.prototypes_dir(),
            },
            map,
            ss13_atmos::AtmosPlugin,
            ss13_game::server::GameServerPlugin::default(),
        ));
    }
    app
}

/// The old endpoint closes on the transport runtime after its `App` is dropped; the next one
/// binds the same port once the socket is free.
fn wait_for_port(addr: std::net::SocketAddr) {
    let start = Instant::now();
    while std::net::UdpSocket::bind(addr).is_err() && start.elapsed() < Duration::from_secs(10) {
        std::thread::sleep(Duration::from_millis(100));
    }
}
