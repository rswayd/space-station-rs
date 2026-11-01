//! Line-of-sight interest on the real game: what a player's client receives through walls and
//! doors, and the replication cost of it on MetaStation.
//!
//! ```text
//! SS13_ASSETS=$ASSETS LOS_CLIENTS=50 cargo test -p ss13_game --all-features --test it los:: -- --ignored --nocapture
//! ```
//! `LOS_XRAY=1` gives every client x-ray sight: the whole view box, the cost before line of sight.

use core::net::{Ipv4Addr, SocketAddr};
use core::time::Duration;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Instant;

use bevy::prelude::*;
use ss13_game::server::{GameServerPlugin, Players};
use ss13_mapload::{MapLoadPlugin, Turf};
use ss13_net::client::{
    NetClientConfig, NetClientPlugin, NetClientState, NetEntityMap, SendIntent,
};
use ss13_net::server::{
    ControlledEntity, NetServerConfig, NetServerPlugin, ReplicationServerPlugin, ReplicationStats,
    ServerCertificateDigest, Sight,
};
use ss13_shared::protocol::ClientIntent;
use ss13_shared::{Dir, Optics, TileLighting, TilePos};
use ss13_world::{Density, SpatialIndex, WorldPlugin};

fn server_app(map: MapLoadPlugin) -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, WorldPlugin))
        .insert_resource(ss13_game::round::RoundConfig::skip_lobby())
        .insert_resource(ss13_game::events::EventsConfig::forced_only())
        .insert_resource(NetServerConfig {
            bind_addr: SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
            max_clients_per_ip: 1024,
            ..default()
        })
        .add_plugins((
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
    app
}

fn client_app(server_addr: SocketAddr, digest: &str, token: &str) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(NetClientConfig {
            server_addr,
            certificate_digest: digest.into(),
            token: token.into(),
        })
        .add_plugins(NetClientPlugin);
    app.finish();
    app.cleanup();
    app
}

fn step(server: &mut App, clients: &mut [App]) {
    server.update();
    for c in clients.iter_mut() {
        c.update();
    }
    std::thread::sleep(Duration::from_millis(1));
}

fn send(client: &mut App, dir: Dir) {
    static SEQ: AtomicU32 = AtomicU32::new(0);
    let seq = SEQ.fetch_add(1, Ordering::Relaxed) + 1;
    client
        .world_mut()
        .write_message(SendIntent(ClientIntent::Move { dir, seq }));
}

fn metastation() -> MapLoadPlugin {
    MapLoadPlugin {
        maps: vec!["MetaStation".into()],
        ..MapLoadPlugin::from_env().unwrap_or(MapLoadPlugin {
            source: env!("SS13_SOURCE").into(),
            maps: vec![],
            assets: std::env::var("SS13_ASSETS")
                .unwrap_or_else(|_| "assets/generated".into())
                .into(),
        })
    }
}

/// Station interior tiles a mob can stand on: lit by the station's lighting, nothing dense.
fn interior_tiles(server: &mut App) -> Vec<TilePos> {
    let world = server.world_mut();
    let mut q = world.query_filtered::<(&TilePos, Option<&Optics>), With<Turf>>();
    let mut tiles: Vec<TilePos> = q
        .iter(world)
        .filter(|(p, o)| {
            p.z == 1
                && o.is_none_or(|o| {
                    !o.opaque
                        && matches!(
                            o.tile,
                            None | Some(TileLighting::Area { dynamic: true, .. })
                        )
                })
        })
        .map(|(p, _)| *p)
        .collect();
    let index = world.resource::<SpatialIndex>();
    tiles.retain(|&p| {
        index.on_tile(p).iter().all(|&e| {
            world.get::<Density>(e).is_none() && world.get::<Optics>(e).is_none_or(|o| !o.opaque)
        })
    });
    tiles.sort_by_key(|p| (p.x, p.y));
    tiles.dedup();
    tiles
}

/// N clients on random interior tiles of MetaStation, each walking a random direction that
/// changes every ~0.5 s: replication pass time and replication bytes per client.
#[test]
#[ignore = "needs generated assets and MetaStation; measurement, run with --ignored --nocapture"]
fn metastation_load() {
    let n: usize = std::env::var("LOS_CLIENTS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10);
    let secs: f64 = std::env::var("LOS_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10.0);
    let mut server = server_app(metastation());
    server.update();
    let addr = server.world().resource::<NetServerConfig>().bind_addr;
    let digest = server
        .world()
        .resource::<ServerCertificateDigest>()
        .0
        .clone();
    server
        .world_mut()
        .resource_mut::<ReplicationStats>()
        .count_payload = true;
    let mut clients: Vec<App> = (0..n)
        .map(|i| client_app(addr, &digest, &format!("los{i}")))
        .collect();
    let start = Instant::now();
    loop {
        step(&mut server, &mut clients);
        let ready = clients.iter().all(|c| {
            *c.world().resource::<NetClientState>() == NetClientState::Connected
                && c.world().resource::<NetEntityMap>().controlled().is_some()
        });
        if ready {
            break;
        }
        if start.elapsed() > Duration::from_secs(120) {
            let states: Vec<String> = clients
                .iter()
                .map(|c| {
                    format!(
                        "{:?}/{}",
                        c.world().resource::<NetClientState>(),
                        c.world().resource::<NetEntityMap>().controlled().is_some()
                    )
                })
                .collect();
            panic!("clients join: {states:?}");
        }
    }
    // Spread the mobs over the station (xorshift, fixed seed).
    let tiles = interior_tiles(&mut server);
    assert!(tiles.len() > 1000, "{} interior tiles", tiles.len());
    let mut rng = 0x9E37_79B9_7F4A_7C15u64;
    let mut next = move || {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        rng
    };
    let links: Vec<Entity> = {
        let world = server.world_mut();
        let mut q = world.query_filtered::<Entity, With<ControlledEntity>>();
        q.iter(world).collect()
    };
    // `LOS_XRAY=1`: every client sees the whole view box, as before line of sight (A/B).
    let xray = std::env::var_os("LOS_XRAY").is_some();
    for &link in &links {
        if xray {
            server.world_mut().entity_mut(link).insert(Sight::XRAY);
        }
        let mob = server.world().resource::<Players>().mob(link).unwrap();
        let tile = tiles[(next() % tiles.len() as u64) as usize];
        server.world_mut().entity_mut(mob).insert(tile);
    }
    let warm = Instant::now();
    while warm.elapsed() < Duration::from_secs(2) {
        step(&mut server, &mut clients);
    }

    let totals = |server: &App| -> (u64, u64) {
        let s = server.world().resource::<ReplicationStats>();
        let bytes = links
            .iter()
            .map(|l| s.clients.get(l).map_or(0, |c| c.replication_bytes_total))
            .sum();
        let light = links
            .iter()
            .map(|l| s.clients.get(l).map_or(0, |c| c.light_bytes_total))
            .sum();
        (bytes, light)
    };
    let mobs: Vec<Entity> = links
        .iter()
        .map(|&l| server.world().resource::<Players>().mob(l).unwrap())
        .collect();
    let mut steps = 0u64;
    let mut last: Vec<TilePos> = mobs
        .iter()
        .map(|&m| *server.world().get::<TilePos>(m).unwrap())
        .collect();
    let (bytes0, light0) = totals(&server);
    let los0 = server
        .world()
        .resource::<ReplicationStats>()
        .los_computed_total;
    let mut los_times = Vec::new();
    let mut passes = Vec::new();
    let mut dirs: Vec<Dir> = (0..n)
        .map(|_| Dir::CARDINALS[(next() % 4) as usize])
        .collect();
    let measure = Instant::now();
    let mut last_turn = 0.0;
    while measure.elapsed().as_secs_f64() < secs {
        let t = measure.elapsed().as_secs_f64();
        if t - last_turn > 0.5 {
            last_turn = t;
            for d in &mut dirs {
                if next() % 2 == 0 {
                    *d = Dir::CARDINALS[(next() % 4) as usize];
                }
            }
        }
        for (c, &d) in clients.iter_mut().zip(&dirs) {
            send(c, d);
        }
        step(&mut server, &mut clients);
        let stats = server.world().resource::<ReplicationStats>();
        passes.push(stats.replicate_time);
        los_times.push(stats.los_time);
        for (m, l) in mobs.iter().zip(&mut last) {
            let p = *server.world().get::<TilePos>(*m).unwrap();
            if p != *l {
                steps += 1;
                *l = p;
            }
        }
    }
    let elapsed = measure.elapsed().as_secs_f64();
    let (bytes1, light1) = totals(&server);
    let los = server
        .world()
        .resource::<ReplicationStats>()
        .los_computed_total
        - los0;
    let los_avg = los_times.iter().sum::<Duration>() / los_times.len() as u32;
    let los_per = los_times.iter().sum::<Duration>() / (los.max(1) as u32);
    passes.sort();
    let avg = passes.iter().sum::<Duration>() / passes.len() as u32;
    let p99 = passes[(passes.len() * 99 / 100).min(passes.len() - 1)];
    let max = *passes.last().unwrap();
    let per_client = |b: u64| (b as f64) / elapsed / n as f64;
    let seen: usize = clients
        .iter()
        .map(|c| c.world().resource::<NetEntityMap>().len())
        .sum();
    println!(
        "MetaStation, {n} clients walking, {} passes in {elapsed:.1} s: replicate pass avg {avg:?} \
         p99 {p99:?} max {max:?}; replication B/s per client {:.0} (light {:.0}); \
         entities known per client {:.0}; {:.1} steps/s per mob; line of sight: {los} \
         computations ({:.0}/s), {los_per:?} each, {los_avg:?} per pass",
        passes.len(),
        per_client(bytes1 - bytes0),
        per_client(light1 - light0),
        seen as f64 / n as f64,
        steps as f64 / elapsed / n as f64,
        los as f64 / elapsed,
    );
}
