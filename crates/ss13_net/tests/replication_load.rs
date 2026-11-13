//! Replication load measurement over real WebTransport on 127.0.0.1, ignored by default:
//!
//! ```text
//! cargo test -p ss13_net --all-features --test replication_load -- --ignored --nocapture
//! LOAD_CLIENTS=100 cargo test ... (same)
//! ```
//! Env: `LOAD_CLIENTS` (20), `LOAD_ENTITIES` (50000 static), `LOAD_MOVERS` (1000),
//! `LOAD_SECS` (10, measured after warm-up), `LOAD_VIEWER_EVERY` (1: viewers step every tick),
//! `LOAD_APPEARANCES` (0: every entity random; N: drawn from a pool of N distinct appearances,
//! as on a real station where most objects share a few thousand).
//! Server and clients share this process: the server on the test thread, clients on up to 16
//! threads, so the machine's load is server + clients.

use core::net::{Ipv4Addr, SocketAddr};
use core::time::Duration;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Instant;

use bevy::prelude::*;
use ss13_net::client::{NetClientConfig, NetClientPlugin, NetClientState, NetEntityMap};
use ss13_net::server::{
    ClientConnected, Interest, NetServerConfig, NetServerPlugin, Networked,
    ReplicationServerPlugin, ReplicationStats, ServerCertificateDigest, Viewer,
};
use ss13_shared::{Appearance, Dir, Facing, IconStateId, Layer, Overlay, TilePos};
use ss13_world::{SpatialIndex, WorldPlugin};

const SIZE: i32 = 255;

fn env(key: &str, default: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// xorshift64*: deterministic and dependency-free.
#[derive(Resource)]
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    fn dir(&mut self) -> Dir {
        Dir::CARDINALS[self.below(4) as usize]
    }

    fn tile(&mut self) -> TilePos {
        TilePos::new(
            1 + self.below(SIZE as u64) as i32,
            1 + self.below(SIZE as u64) as i32,
            1,
        )
    }
}

#[derive(Component)]
struct Walker;

#[derive(Resource, Default)]
struct Connected(Vec<Entity>);

/// A typical station object: base sprite plus 0-2 overlays.
fn appearance(rng: &mut Rng) -> Appearance {
    let overlays = (0..rng.below(3))
        .map(|_| Overlay::new(IconStateId(rng.below(40_000) as u32)))
        .collect();
    Appearance {
        icon_state: IconStateId(rng.below(40_000) as u32),
        layer: Layer(2.0 + rng.below(4) as f32),
        plane: 0,
        color: None,
        pixel_offset: [0, 0],
        rotation: 0,
        mouse_opacity: Default::default(),
        glide: 0,
        overlays,
    }
}

fn step(pos: TilePos, dir: Dir) -> TilePos {
    let next = pos.step(dir);
    if (1..=SIZE).contains(&next.x) && (1..=SIZE).contains(&next.y) {
        next
    } else {
        pos.step(dir.reverse())
    }
}

fn walk(
    walkers: Query<(Entity, &TilePos), With<Walker>>,
    mut rng: ResMut<Rng>,
    mut commands: Commands,
) {
    for (entity, &pos) in &walkers {
        let dir = rng.dir();
        commands
            .entity(entity)
            .insert((step(pos, dir), Facing(dir)));
    }
}

fn percentile(sorted: &[Duration], p: f64) -> Duration {
    sorted[((sorted.len() - 1) as f64 * p).round() as usize]
}

/// User + system CPU seconds of this process, from `/proc/self/stat` (Linux only; 0 elsewhere).
fn cpu_secs() -> f64 {
    let stat = std::fs::read_to_string("/proc/self/stat").unwrap_or_default();
    let fields: Vec<&str> = stat
        .rsplit_once(')')
        .map(|(_, rest)| rest.split_whitespace().collect())
        .unwrap_or_default();
    // utime and stime are fields 14 and 15 of the line; `rest` starts at field 3.
    let ticks = |i: usize| {
        fields
            .get(i)
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0)
    };
    (ticks(11) + ticks(12)) / 100.0
}

/// Bytes and packets through the loopback interface since boot, from `/proc/net/dev` (Linux only;
/// 0 elsewhere): the real wire cost of everything on 127.0.0.1, IP and UDP headers included.
fn loopback_totals() -> (u64, u64) {
    let dev = std::fs::read_to_string("/proc/net/dev").unwrap_or_default();
    dev.lines()
        .find_map(|line| line.trim_start().strip_prefix("lo:"))
        .map(|rest| {
            let fields: Vec<u64> = rest
                .split_whitespace()
                .map(|v| v.parse().unwrap_or(0))
                .collect();
            (fields[0], fields[1])
        })
        .unwrap_or_default()
}

/// Runs `count` headless clients at the tick rate until `stop`; returns the number of
/// replicated entities they hold at the end and the time of each client update.
fn run_clients(
    count: usize,
    server_addr: SocketAddr,
    digest: String,
    stop: &AtomicBool,
    ready: &AtomicUsize,
) -> (usize, Vec<Duration>, Vec<String>) {
    let mut apps: Vec<App> = (0..count)
        .map(|_| {
            let mut app = App::new();
            app.add_plugins(MinimalPlugins)
                .insert_resource(NetClientConfig {
                    server_addr,
                    certificate_digest: digest.clone(),
                    token: "load".into(),
                })
                .add_plugins(NetClientPlugin);
            app.finish();
            app.cleanup();
            app
        })
        .collect();
    let mut connected = vec![false; count];
    let mut times = Vec::new();
    while !stop.load(Ordering::Relaxed) {
        let frame = Instant::now();
        for (app, connected) in apps.iter_mut().zip(&mut connected) {
            let t = Instant::now();
            app.update();
            times.push(t.elapsed());
            if !*connected && *app.world().resource::<NetClientState>() == NetClientState::Connected
            {
                *connected = true;
                ready.fetch_add(1, Ordering::Relaxed);
            }
        }
        if let Some(rest) = ss13_shared::protocol::TICK_DURATION.checked_sub(frame.elapsed()) {
            std::thread::sleep(rest);
        }
    }
    let local = apps
        .iter()
        .map(|a| a.world().resource::<NetEntityMap>().len())
        .sum();
    let failed = apps
        .iter()
        .filter_map(|a| match a.world().resource::<NetClientState>() {
            NetClientState::Connected => None,
            other => Some(format!("{other:?}")),
        })
        .collect();
    (local, times, failed)
}

#[test]
#[ignore = "load measurement; run with --ignored --nocapture"]
fn replication_load() {
    let clients = env("LOAD_CLIENTS", 20);
    let statics = env("LOAD_ENTITIES", 50_000);
    let movers = env("LOAD_MOVERS", 1000);
    let secs = env("LOAD_SECS", 10) as f64;
    let viewer_every = env("LOAD_VIEWER_EVERY", 1).max(1);

    let server_addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 0));
    let mut server = App::new();
    server
        .add_plugins((MinimalPlugins, WorldPlugin))
        .insert_resource(NetServerConfig {
            bind_addr: server_addr,
            // Every load client comes from 127.0.0.1.
            max_clients_per_ip: usize::MAX,
            ..default()
        })
        .add_plugins((NetServerPlugin, ReplicationServerPlugin))
        .insert_resource(Rng(0x5eed))
        .init_resource::<Connected>()
        .add_systems(Update, walk)
        .add_systems(
            Update,
            |mut connected: MessageReader<ClientConnected>, mut log: ResMut<Connected>| {
                log.0.extend(connected.read().map(|c| c.client));
            },
        );
    server
        .world_mut()
        .resource_mut::<SpatialIndex>()
        .create_level(1, SIZE as u32, SIZE as u32)
        .unwrap();
    let load = Instant::now();
    let mut rng = Rng(42);
    let pool: Vec<Appearance> = (0..env("LOAD_APPEARANCES", 0))
        .map(|_| appearance(&mut rng))
        .collect();
    let pick = |rng: &mut Rng| match pool.len() {
        0 => appearance(rng),
        n => pool[rng.below(n as u64) as usize].clone(),
    };
    let batch: Vec<_> = (0..statics)
        .map(|_| (Networked, rng.tile(), pick(&mut rng)))
        .collect();
    server.world_mut().spawn_batch(batch);
    for _ in 0..movers {
        let bundle = (
            Networked,
            Walker,
            rng.tile(),
            Facing(Dir::South),
            pick(&mut rng),
        );
        server.world_mut().spawn(bundle);
    }
    server.finish();
    server.cleanup();
    server
        .world_mut()
        .resource_mut::<ReplicationStats>()
        .count_payload = true;
    let first = Instant::now();
    server.update();
    println!(
        "spawned {statics} static + {movers} moving entities in {:?}; first update {:?}",
        first - load,
        first.elapsed()
    );
    let server_addr = server.world().resource::<NetServerConfig>().bind_addr;
    let digest = server
        .world()
        .resource::<ServerCertificateDigest>()
        .0
        .clone();

    // Clients run on their own threads so that their cost does not stretch server frames.
    let stop = Arc::new(AtomicBool::new(false));
    let ready = Arc::new(AtomicUsize::new(0));
    let threads = clients.clamp(1, 16);
    let workers: Vec<_> = (0..threads)
        .map(|t| {
            let count = clients / threads + usize::from(t < clients % threads);
            let (digest, stop, ready) = (digest.clone(), stop.clone(), ready.clone());
            std::thread::spawn(move || run_clients(count, server_addr, digest, &stop, &ready))
        })
        .collect();
    let connect = Instant::now();
    // A connect storm can time out a few handshakes (clients then retry after a backoff):
    // measure with whoever made it in.
    while (server.world().resource::<Connected>().0.len() < clients
        || ready.load(Ordering::Relaxed) < clients)
        && connect.elapsed() < Duration::from_secs(30)
    {
        server.update();
        std::thread::sleep(Duration::from_millis(5));
    }
    let connected = server.world().resource::<Connected>().0.len();
    assert!(connected > 0, "no client connected");
    println!(
        "{connected} of {clients} clients connected in {:?}",
        connect.elapsed()
    );

    let links = server.world().resource::<Connected>().0.clone();
    let mut viewers: Vec<Viewer> = links.iter().map(|_| Viewer::new(rng.tile())).collect();
    let mut server_times = Vec::new();
    let mut replicate_times = Vec::new();
    let warmup = 2.0;
    let start = Instant::now();
    let mut measured: Option<(Instant, f64, Vec<(u64, u64, u64)>, (u64, u64), (u64, u64))> = None;
    let mut tick = 0usize;
    // Byte totals advance once per second: run whole windows past the measured span.
    while start.elapsed().as_secs_f64() < warmup + secs + 1.1 {
        let frame = Instant::now();
        if tick.is_multiple_of(viewer_every) {
            for (viewer, &link) in viewers.iter_mut().zip(&links) {
                viewer.center = step(viewer.center, rng.dir());
                server.world_mut().entity_mut(link).insert(*viewer);
            }
        }
        tick += 1;

        let t = Instant::now();
        server.update();
        let server_time = t.elapsed();

        if (frame - start).as_secs_f64() > warmup {
            let stats = server.world().resource::<ReplicationStats>();
            server_times.push(server_time);
            replicate_times.push(stats.replicate_time);
            if measured.is_none() {
                let totals = links
                    .iter()
                    .map(|l| {
                        let t = stats.clients.get(l).copied().unwrap_or_default();
                        (t.bytes_total, t.packets_total, t.replication_bytes_total)
                    })
                    .collect();
                measured = Some((
                    Instant::now(),
                    cpu_secs(),
                    totals,
                    (stats.spawned_total, stats.appearances_sent_total),
                    loopback_totals(),
                ));
            }
        }
        if let Some(rest) = ss13_shared::protocol::TICK_DURATION.checked_sub(frame.elapsed()) {
            std::thread::sleep(rest);
        }
    }

    let (since, cpu_start, before, (spawned0, sent0), (lo_bytes0, lo_packets0)) = measured.unwrap();
    let wall = since.elapsed().as_secs_f64();
    let (lo_bytes, lo_packets) = loopback_totals();
    let cpu = cpu_secs() - cpu_start;
    let stats = server.world().resource::<ReplicationStats>();
    let interest = server.world().resource::<Interest>();
    let mut rates: Vec<(f64, f64, f64)> = links
        .iter()
        .zip(&before)
        .map(|(l, &(b0, p0, r0))| {
            let t = stats.clients.get(l).copied().unwrap_or_default();
            (
                (t.bytes_total - b0) as f64 / wall,
                (t.packets_total - p0) as f64 / wall,
                (t.replication_bytes_total - r0) as f64 / wall,
            )
        })
        .collect();
    rates.sort_by(|a, b| a.0.total_cmp(&b.0));
    let visible: usize = links.iter().map(|&l| interest.visible_count(l)).sum();
    stop.store(true, Ordering::Relaxed);
    let (local, mut client_times, failed) = workers.into_iter().map(|w| w.join().unwrap()).fold(
        (0, Vec::new(), Vec::new()),
        |(n, mut times, mut failed), (local, t, f)| {
            times.extend(t);
            failed.extend(f);
            (n + local, times, failed)
        },
    );
    server_times.sort();
    replicate_times.sort();
    client_times.sort();
    let ticks = server_times.len();
    let n = rates.len() as f64;
    println!(
        "--- {clients} clients, {statics} static + {movers} moving entities, \
         viewer step every {viewer_every} tick(s)"
    );
    println!(
        "ticks {ticks} in {wall:.1}s ({:.1}/s); process CPU {:.0}% of one core; loadavg {}",
        ticks as f64 / wall,
        cpu / wall * 100.0,
        std::fs::read_to_string("/proc/loadavg")
            .unwrap_or_default()
            .trim()
    );
    println!(
        "per-client bytes/s sent by the server (UDP payload; IP+UDP add 28 B/packet): \
         avg {:.0} min {:.0} max {:.0}; packets/s avg {:.1}",
        rates.iter().map(|r| r.0).sum::<f64>() / n,
        rates.first().unwrap().0,
        rates.last().unwrap().0,
        rates.iter().map(|r| r.1).sum::<f64>() / n,
    );
    println!(
        "loopback wire (both directions, IP+UDP headers included, whole machine): \
         {:.0} B/s and {:.1} packets/s per client",
        (lo_bytes - lo_bytes0) as f64 / wall / n,
        (lo_packets - lo_packets0) as f64 / wall / n,
    );
    println!(
        "server update p50 {:?} p99 {:?} max {:?}; replicate pass p50 {:?} p99 {:?} max {:?}",
        percentile(&server_times, 0.5),
        percentile(&server_times, 0.99),
        server_times.last().unwrap(),
        percentile(&replicate_times, 0.5),
        percentile(&replicate_times, 0.99),
        replicate_times.last().unwrap(),
    );
    println!(
        "one client update (on {threads} client threads) p50 {:?} p99 {:?}",
        percentile(&client_times, 0.5),
        percentile(&client_times, 0.99),
    );
    println!(
        "visible per client: server {:.0}, client-side entities {:.0}",
        visible as f64 / n,
        local as f64 / n,
    );
    println!(
        "replication messages alone (postcard, no transport): avg {:.0} B/s per client",
        rates.iter().map(|r| r.2).sum::<f64>() / n,
    );
    println!(
        "per client per second: {:.0} entities spawned, {:.1} appearance values sent",
        (stats.spawned_total - spawned0) as f64 / wall / n,
        (stats.appearances_sent_total - sent0) as f64 / wall / n,
    );
    // A client that never connected: its last state says why.
    for reason in &failed {
        println!("client not connected: {reason}");
    }
}
