//! In-process servers: the same plugins as `ss13_server`, on a free loopback port, with the debug
//! gate's dev tools open (debug builds) so the test hooks answer. A server loads a small staging
//! level first (where clients appear before a bot is placed) and then every case map it will
//! run, one level each.

use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use bevy::prelude::*;
use ss13_mapload::{MapLoadPlugin, MapLoadReport};
use ss13_net::server::{NetServerConfig, ServerCertificateDigest};
use ss13_shared::protocol::TICK_DURATION;

/// Where connecting clients spawn before `setup` moves them; real station types.
pub const STAGING_MAP: &str = r#""w" = (/turf/closed/wall,/area/misc/testroom)
"f" = (/turf/open/floor/iron,/area/misc/testroom)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor/iron,/area/misc/testroom)
"l" = (/obj/effect/landmark/latejoin,/turf/open/floor/iron,/area/misc/testroom)

(1,1,1) = {"
wwwwwww
wsssssw
wsffflw
wsffflw
wsssssw
wwwwwww
"}
"#;

/// Seed of pooled servers (a case with its own `seed` is exclusive).
pub const POOL_SEED: u64 = 1;

#[derive(Clone, Debug)]
pub struct ServerSpec {
    /// Generated assets (`manifest.postcard`, `prototypes/`).
    pub assets: PathBuf,
    /// BYOND checkout: station names resolve against it.
    pub source: PathBuf,
    /// The staging `.dmm`.
    pub staging: PathBuf,
    /// Case maps (`.dmm` paths or station names), one level each after staging.
    pub maps: Vec<String>,
    pub seed: u64,
}

pub struct Server {
    pub app: App,
    pub addr: SocketAddr,
    pub digest: String,
    /// First z of each of `spec.maps`.
    pub levels: Vec<u16>,
    next_tick: Instant,
    ticks: u64,
}

/// Writes the staging map `text` into `dir`.
pub fn write_staging(dir: &Path, text: &str) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join("staging.dmm");
    std::fs::write(&path, text)?;
    Ok(path)
}

impl Server {
    pub fn start(spec: &ServerSpec) -> anyhow::Result<Self> {
        let mut maps = vec![spec.staging.display().to_string()];
        maps.extend(spec.maps.iter().cloned());
        let map = MapLoadPlugin {
            source: spec.source.clone(),
            maps,
            assets: spec.assets.clone(),
        };
        let mut app = App::new();
        // Every game RNG from the case's seed, one tick of game time per update, random events
        // only when a case forces one (`event`).
        ss13_game::testing::deterministic(&mut app, spec.seed);
        app.add_plugins((MinimalPlugins, ss13_world::WorldPlugin))
            .insert_resource(ss13_game::round::RoundConfig::skip_lobby())
            .insert_resource(NetServerConfig {
                // Port 0: the bound port is read back after `Startup`.
                bind_addr: SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
                // Every bot connects from 127.0.0.1.
                max_clients_per_ip: 1024,
                ..default()
            })
            .insert_resource(ss13_debug::server::DebugGate {
                queries: true,
                dev_tools: true,
            })
            // Admins come from case files (`bot ... admin`), nothing is logged to files; the bots
            // are trusted clients.
            .insert_resource(ss13_admin::server::AdminConfig {
                config_dir: None,
                log_file: None,
                insecure_admins: true,
            })
            .add_plugins((
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
        app.finish();
        app.cleanup();
        app.update();
        let digest = app.world().resource::<ServerCertificateDigest>().0.clone();
        let addr = app.world().resource::<NetServerConfig>().bind_addr;
        let report = app.world().resource::<MapLoadReport>();
        anyhow::ensure!(
            report.maps.len() == spec.maps.len() + 1,
            "{} of {} maps loaded",
            report.maps.len(),
            spec.maps.len() + 1
        );
        let levels = report.maps[1..].iter().map(|m| m.first_z).collect();
        Ok(Self {
            app,
            addr,
            digest,
            levels,
            next_tick: Instant::now(),
            ticks: 0,
        })
    }

    /// One server tick when it is due (the real server's rate, [`TICK_DURATION`]).
    pub fn tick_if_due(&mut self) {
        let now = Instant::now();
        if now < self.next_tick {
            return;
        }
        self.app.update();
        self.ticks += 1;
        self.next_tick += TICK_DURATION;
        // Behind by more than a tick (a slow load, a debugger): do not burst to catch up.
        if self.next_tick + TICK_DURATION < now {
            self.next_tick = now + TICK_DURATION;
        }
    }

    /// Ticks run since the start.
    pub fn ticks(&self) -> u64 {
        self.ticks
    }

    pub fn until_next_tick(&self) -> Duration {
        self.next_tick.saturating_duration_since(Instant::now())
    }
}
