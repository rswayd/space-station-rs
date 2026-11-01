//! Deterministic in-process servers for integration tests (feature `testing`).
//!
//! Every test server runs on game time, not on the machine's clock: one `App::update` is one
//! tick ([`TICK`]) of `Time` (`TimeUpdateStrategy::ManualDuration`), so timers, cooldowns and
//! per-second rates advance per step whatever the load. Every game RNG derives from one seed
//! ([`MapSeed`], see [`crate::server::seed_rngs`] for the list); [`test_seed`] picks it
//! (`SS13_SEED`, default [`DEFAULT_SEED`]). Random events only run when a test forces one.
//!
//! The network still runs on real time (QUIC tasks on their own threads), so a test never
//! sleeps for a result: it steps the server and its clients until a condition holds
//! ([`Drive::run_until`]). A wait gives up only after both its game-time budget and the same
//! number of wall-clock seconds are spent: a fast machine does not fail a wait before the
//! network had real time to deliver, a loaded one does not fail it before the game had time.
//!
//! ```ignore
//! let assets = write_fixture(&dir, TYPES, &USED, &STATES, &[("map.dmm", MAP)]);
//! let mut s = Station::start(fixture_map(&assets, &["map.dmm"]), |app| {
//!     app.add_plugins(ss13_chat::server::ChatServerPlugin);
//! });
//! let a = s.join("p0", |client| { client.init_resource::<Received>(); });
//! s.send(a, ClientIntent::ToggleCombatMode);
//! s.run_until("combat mode", |s| s.get::<CombatMode>(s.players[a].mob).0);
//! ```

use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use ss13_assets::manifest::{AssetManifest, IconStateInfo};
use ss13_mapload::convert::{DmTypes, convert};
use ss13_mapload::{MapLoadPlugin, MapSeed};
use ss13_net::client::{
    NetClientConfig, NetClientPlugin, NetClientState, NetEntity, NetEntityMap, SendIntent,
};
use ss13_net::server::{
    ClientToken, Interest, NetServerConfig, NetServerPlugin, ReplicationServerPlugin,
    ServerCertificateDigest,
};
use ss13_shared::Dir;
use ss13_shared::protocol::{ClickModifiers, ClientIntent, NetId, TICK_DURATION};

use crate::server::{GameServerPlugin, MoveQueue, Players};

/// Game time per step.
pub const TICK: Duration = TICK_DURATION;

/// Seed of test servers unless `SS13_SEED` sets another (to reproduce or explore a failure).
pub const DEFAULT_SEED: u64 = 1;

/// Default budget of a wait, seconds of game time (and of wall time, see the module docs).
pub const WAIT_SECS: f64 = 30.0;

/// `SS13_SEED`, else [`DEFAULT_SEED`].
pub fn test_seed() -> u64 {
    std::env::var("SS13_SEED")
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(DEFAULT_SEED)
}

/// Steps covering `secs` of game time.
pub fn steps(secs: f64) -> u32 {
    (secs / TICK.as_secs_f64()).ceil() as u32
}

/// Makes a server app deterministic; call before adding the game plugins. `seed` becomes the
/// [`MapSeed`] every game RNG derives from; `Time` advances [`TICK`] per update; random events
/// run only when forced.
pub fn deterministic(app: &mut App, seed: u64) {
    app.insert_resource(MapSeed(seed))
        .insert_resource(TimeUpdateStrategy::ManualDuration(TICK))
        .insert_resource(crate::events::EventsConfig::forced_only());
}

/// A fixture's map plugin: `maps` relative to `assets`, which is also the BYOND "source".
pub fn fixture_map(assets: &Path, maps: &[&str]) -> MapLoadPlugin {
    MapLoadPlugin {
        source: assets.to_path_buf(),
        maps: maps.iter().map(|&m| m.into()).collect(),
        assets: assets.to_path_buf(),
    }
}

/// Writes fixture assets into `dir`: prototypes converted from the `ss13-dm-extract` JSON
/// `types` for the types in `used` (plus [`ss13_mapload::RUNTIME_TYPES`]), a manifest of
/// one-direction 32x32 `states` (manifest order = `IconStateId`) and `files` (maps) by name.
pub fn write_fixture(
    dir: &Path,
    types: &str,
    used: &[&str],
    states: &[(&str, &str)],
    files: &[(&str, &str)],
) -> PathBuf {
    std::fs::create_dir_all(dir.join("prototypes")).unwrap();
    let types = DmTypes::parse(types.as_bytes()).unwrap();
    let mut used = used.to_vec();
    used.extend(ss13_mapload::RUNTIME_TYPES);
    let conversion = convert(&types, used).unwrap();
    std::fs::write(dir.join("prototypes/dm_types.ron"), conversion.ron).unwrap();
    let manifest = AssetManifest {
        atlases: vec![],
        states: states
            .iter()
            .map(|&(dmi, state)| IconStateInfo {
                dmi: dmi.into(),
                state: state.into(),
                atlas: 0,
                width: 32,
                height: 32,
                dirs: 1,
                rects: vec![[0, 0, 32, 32]],
                delays: vec![],
                loop_count: 0,
                rewind: false,
                movement: false,
            })
            .collect(),
    };
    std::fs::write(
        dir.join(ss13_assets::import::MANIFEST_FILE),
        postcard::to_stdvec(&manifest).unwrap(),
    )
    .unwrap();
    for (name, text) in files {
        std::fs::write(dir.join(name), text).unwrap();
    }
    dir.to_path_buf()
}

/// A started server app: [`deterministic`] with [`test_seed`], lobby skipped, a free loopback
/// port; `setup` adds resources and plugins (they override these defaults) before the net,
/// prototype, map and game plugins; one update has run (`Startup`, maps loaded).
pub fn server_app(map: MapLoadPlugin, setup: impl FnOnce(&mut App)) -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, ss13_world::WorldPlugin))
        .insert_resource(crate::round::RoundConfig::skip_lobby())
        .insert_resource(NetServerConfig {
            // Port 0: the bound port is read back after `Startup`.
            bind_addr: SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
            ..default()
        });
    deterministic(&mut app, test_seed());
    setup(&mut app);
    app.add_plugins((
        NetServerPlugin,
        ReplicationServerPlugin,
        ss13_proto::ProtoPlugin {
            dir: map.prototypes_dir(),
        },
        map,
        GameServerPlugin::default(),
    ));
    app.finish();
    app.cleanup();
    app.update();
    app
}

/// A client app connecting to `server` with `token`, on game time like the server; `setup` adds
/// the test's recorders.
pub fn client_app(server: &App, token: &str, setup: impl FnOnce(&mut App)) -> App {
    let world = server.world();
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(TimeUpdateStrategy::ManualDuration(TICK))
        .insert_resource(NetClientConfig {
            server_addr: world.resource::<NetServerConfig>().bind_addr,
            certificate_digest: world.resource::<ServerCertificateDigest>().0.clone(),
            token: token.into(),
        })
        .add_plugins(NetClientPlugin);
    setup(&mut app);
    app.finish();
    app.cleanup();
    app
}

/// A connected client: its app, its connection entity on the server and its mob.
pub struct Player {
    pub app: App,
    pub link: Entity,
    /// `Entity::PLACEHOLDER` until it controls one (a lobby).
    pub mob: Entity,
    /// `seq` of the last `Move` sent ([`Station::try_step`]).
    pub seq: u32,
}

/// A test server and its clients.
pub struct Station {
    pub server: App,
    pub players: Vec<Player>,
}

impl Station {
    /// [`server_app`] without clients.
    pub fn start(map: MapLoadPlugin, setup: impl FnOnce(&mut App)) -> Self {
        Self::new(server_app(map, setup))
    }

    pub fn new(server: App) -> Self {
        Self {
            server,
            players: Vec::new(),
        }
    }

    /// Connects a client with `token` (see [`client_app`]) and waits until the server has its
    /// connection; returns its index. Its mob is set when it already controls one. Game time
    /// stands still meanwhile.
    pub fn connect(&mut self, token: &str, setup: impl FnOnce(&mut App)) -> usize {
        let app = client_app(&self.server, token, setup);
        let i = self.players.len();
        self.players.push(Player {
            app,
            link: Entity::PLACEHOLDER,
            mob: Entity::PLACEHOLDER,
            seq: 0,
        });
        self.wait_frozen(&format!("{token} connects"), |s| {
            *s.players[i].app.world().resource::<NetClientState>() == NetClientState::Connected
                && s.link_of(token).is_some()
        });
        let link = self.link_of(token).unwrap();
        self.players[i].link = link;
        if let Some(mob) = self.server.world().resource::<Players>().mob(link) {
            self.players[i].mob = mob;
        }
        i
    }

    /// [`Station::connect`], then waits (game time standing still) until the client controls
    /// its mob.
    pub fn join(&mut self, token: &str, setup: impl FnOnce(&mut App)) -> usize {
        let i = self.connect(token, setup);
        self.wait_frozen(&format!("{token} controls a mob"), |s| {
            s.players[i]
                .app
                .world()
                .resource::<NetEntityMap>()
                .controlled()
                .is_some()
        });
        let link = self.players[i].link;
        self.players[i].mob = self.server.world().resource::<Players>().mob(link).unwrap();
        i
    }

    /// `n` clients joined in order, tokens `p0`, `p1`, ..., each set up by `setup`.
    pub fn join_all(&mut self, n: usize, setup: impl Fn(&mut App)) {
        for i in 0..n {
            self.join(&format!("p{i}"), &setup);
        }
    }

    /// Steps with game time stopped until `done` holds (at most [`WAIT_SECS`] of wall time): a
    /// join is a network handshake, and the mobs already in the world (breathing vacuum,
    /// bleeding) must not age by however long the handshake took on this machine.
    fn wait_frozen(&mut self, what: &str, mut done: impl FnMut(&mut Self) -> bool) {
        let frozen = (0..=self.players.len())
            .map(|_| TimeUpdateStrategy::ManualDuration(Duration::ZERO))
            .collect();
        let saved = self.swap_time_strategies(frozen);
        let start = Instant::now();
        while !done(self) {
            assert!(
                start.elapsed().as_secs_f64() < WAIT_SECS,
                "timed out: {what}"
            );
            self.tick();
        }
        self.swap_time_strategies(saved);
    }

    /// Sets the server's (first) and each client's time strategy; returns the previous ones.
    fn swap_time_strategies(&mut self, new: Vec<TimeUpdateStrategy>) -> Vec<TimeUpdateStrategy> {
        let apps =
            std::iter::once(&mut self.server).chain(self.players.iter_mut().map(|p| &mut p.app));
        apps.zip(new)
            .map(|(app, strategy)| {
                std::mem::replace(
                    &mut *app.world_mut().resource_mut::<TimeUpdateStrategy>(),
                    strategy,
                )
            })
            .collect()
    }

    /// The server connection of the client with `token`.
    pub fn link_of(&mut self, token: &str) -> Option<Entity> {
        let world = self.server.world_mut();
        let mut q = world.query::<(Entity, &ClientToken)>();
        q.iter(world).find(|(_, t)| t.0 == token).map(|(e, _)| e)
    }

    /// One tick: the server, then every client.
    pub fn tick(&mut self) {
        self.server.update();
        for p in &mut self.players {
            p.app.update();
        }
        // The QUIC tasks need a core too when every test thread is busy.
        std::thread::sleep(Duration::from_millis(1));
    }

    /// Game time since the server started, seconds.
    pub fn now(&self) -> f64 {
        self.server.world().resource::<Time>().elapsed_secs_f64()
    }

    pub fn get<T: Component>(&self, e: Entity) -> &T {
        self.server.world().get::<T>(e).unwrap()
    }

    pub fn try_get<T: Component>(&self, e: Entity) -> Option<&T> {
        self.server.world().get::<T>(e)
    }

    /// The entity `p` controls now (its mob, a ghost, an AI eye ...).
    pub fn controlled(&self, p: usize) -> Entity {
        let link = self.players[p].link;
        self.server.world().resource::<Players>().mob(link).unwrap()
    }

    pub fn send(&mut self, p: usize, intent: ClientIntent) {
        self.players[p]
            .app
            .world_mut()
            .write_message(SendIntent(intent));
    }

    /// A tgui act on `window`.
    pub fn act(&mut self, p: usize, window: u32, action: &str, params: &[(&str, &str)]) {
        let params = params
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        self.send(
            p,
            ClientIntent::UiAct {
                window,
                action: action.into(),
                params,
            },
        );
    }

    /// `entity`'s id for `p`, when replicated to it.
    pub fn net_id(&self, p: usize, entity: Entity) -> Option<NetId> {
        self.server
            .world()
            .resource::<Interest>()
            .net_id(self.players[p].link, entity)
    }

    /// `p`'s client copy of `entity`, when replicated.
    pub fn client_copy(&mut self, p: usize, entity: Entity) -> Option<Entity> {
        let id = self.net_id(p, entity)?;
        let world = self.players[p].app.world_mut();
        let mut q = world.query::<(Entity, &NetEntity)>();
        q.iter(world).find(|(_, n)| n.0 == id).map(|(e, _)| e)
    }

    /// A plain left click by `p` on `entity` (replicated to `p`).
    pub fn click(&mut self, p: usize, entity: Entity) {
        self.click_with(p, entity, ClickModifiers::default());
    }

    pub fn click_with(&mut self, p: usize, entity: Entity, modifiers: ClickModifiers) {
        let target = self.net_id(p, entity).expect("replicated to the clicker");
        self.send(p, ClientIntent::Click { target, modifiers });
    }

    /// One step of `p`'s controlled entity: the next `seq` once the previous one was applied,
    /// nothing while it is pending. Call it from a [`Drive::run_until`] condition to walk.
    pub fn try_step(&mut self, p: usize, dir: Dir) {
        let controlled = self.controlled(p);
        let applied = self.get::<MoveQueue>(controlled).applied();
        if self.players[p].seq == applied {
            self.players[p].seq += 1;
            let seq = self.players[p].seq;
            self.send(p, ClientIntent::Move { dir, seq });
        }
    }
}

/// Stepping and waiting, for [`Station`] and for a test's own type that wraps one (its
/// conditions then see the test's helpers): implement [`Drive::station`].
pub trait Drive: Sized {
    fn station(&mut self) -> &mut Station;

    /// Appended to a failed wait's message (e.g. the clients' chat).
    fn context(&self) -> String {
        String::new()
    }

    /// One tick of the server and its clients.
    fn step(&mut self) {
        self.station().tick();
    }

    /// Steps until `done` holds, at most [`WAIT_SECS`] (see the module docs).
    fn run_until(&mut self, what: &str, done: impl FnMut(&mut Self) -> bool) {
        self.run_until_for(WAIT_SECS, what, done);
    }

    /// Steps until `done` holds; fails after `secs` of game time and `secs` of wall time.
    fn run_until_for(&mut self, secs: f64, what: &str, mut done: impl FnMut(&mut Self) -> bool) {
        let game_start = self.station().now();
        let wall_start = Instant::now();
        while !done(self) {
            let game = self.station().now() - game_start;
            if game >= secs && wall_start.elapsed().as_secs_f64() >= secs {
                panic!(
                    "timed out after {game:.1} s of game time: {what}{}",
                    self.context()
                );
            }
            self.step();
        }
    }

    /// Runs `secs` of game time, asserting `invariant` before every step.
    fn hold(&mut self, secs: f64, what: &str, mut invariant: impl FnMut(&Self) -> bool) {
        for _ in 0..steps(secs) {
            assert!(invariant(self), "{what}{}", self.context());
            self.step();
        }
    }

    /// Runs `secs` of game time.
    fn run_for(&mut self, secs: f64) {
        for _ in 0..steps(secs) {
            self.step();
        }
    }
}

impl Drive for Station {
    fn station(&mut self) -> &mut Station {
        self
    }
}

/// Makes a test's type that holds a [`Station`] in `$field` act as one: `Deref`/`DerefMut` to
/// it (its helpers) and [`Drive`] (waits whose conditions see the test type's helpers).
///
/// ```ignore
/// struct Station(testing::Station);
/// ss13_game::drive_station!(Station, 0);
/// ```
#[macro_export]
macro_rules! drive_station {
    ($ty:ty, $field:tt) => {
        impl ::core::ops::Deref for $ty {
            type Target = $crate::testing::Station;
            fn deref(&self) -> &$crate::testing::Station {
                &self.$field
            }
        }
        impl ::core::ops::DerefMut for $ty {
            fn deref_mut(&mut self) -> &mut $crate::testing::Station {
                &mut self.$field
            }
        }
        impl $crate::testing::Drive for $ty {
            fn station(&mut self) -> &mut $crate::testing::Station {
                &mut self.$field
            }
        }
    };
}
