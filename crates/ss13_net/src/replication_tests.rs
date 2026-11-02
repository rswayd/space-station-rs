//! In-process replication check over real WebTransport on 127.0.0.1.

use core::net::{Ipv4Addr, SocketAddr};
use core::time::Duration;
use std::time::Instant;

use bevy::prelude::*;
use ss13_shared::{
    Appearance, Dir, Facing, IconStateId, Layer, LightSource, Optics, TileLighting, TilePos,
};
use ss13_world::{SpatialIndex, WorldPlugin};

use crate::client::{
    LightBlocks, LocalControl, NetClientConfig, NetClientPlugin, NetClientState, NetEntity,
    NetEntityMap,
};
use crate::server::{
    ClientConnected, ControlledEntity, Interest, KEEP_MARGIN, LIGHT_MARGIN, NetServerConfig,
    NetServerPlugin, Networked, ReplicationServerPlugin, ReplicationStats, ServerCertificateDigest,
    ViewMask, ViewMasks, Viewer,
};
use crate::view_mask::MaskId;

#[derive(Resource, Default)]
struct Connected(Vec<Entity>);

/// What a client should draw: position, icon, facing.
type Seen = (i32, i32, u16, u32, Option<u8>);

fn appearance(icon: u32) -> Appearance {
    Appearance {
        icon_state: IconStateId(icon),
        layer: Layer(3.0),
        plane: 0,
        color: None,
        pixel_offset: [0, 0],
        rotation: 0,
        mouse_opacity: Default::default(),
        overlays: vec![],
    }
}

fn server_app(bind_addr: SocketAddr) -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, WorldPlugin))
        .insert_resource(NetServerConfig {
            bind_addr,
            ..default()
        })
        .add_plugins((NetServerPlugin, ReplicationServerPlugin))
        .init_resource::<Connected>()
        .add_systems(
            Update,
            |mut connected: MessageReader<ClientConnected>, mut log: ResMut<Connected>| {
                log.0.extend(connected.read().map(|c| c.client));
            },
        );
    let mut index = app.world_mut().resource_mut::<SpatialIndex>();
    index.create_level(1, 64, 64).unwrap();
    index.create_level(2, 64, 64).unwrap();
    // Every other tile on z 1, a sparse grid on z 2; the icon encodes the spawn tile.
    for x in (1..=63).step_by(2) {
        for y in (1..=63).step_by(2) {
            app.world_mut().spawn((
                Networked,
                TilePos::new(x, y, 1),
                Facing(Dir::South),
                appearance((x * 100 + y) as u32),
            ));
        }
    }
    for x in (1..=63).step_by(8) {
        app.world_mut().spawn((
            Networked,
            TilePos::new(x, 10, 2),
            appearance(100_000 + x as u32),
        ));
    }
    // Not networked: never replicated.
    app.world_mut()
        .spawn((TilePos::new(10, 10, 1), appearance(1)));
    app.finish();
    app.cleanup();
    app
}

fn client_app(config: NetClientConfig) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(config)
        .add_plugins(NetClientPlugin);
    app.finish();
    app.cleanup();
    app
}

fn run_until(apps: &mut [&mut App], what: &str, mut done: impl FnMut(&mut [&mut App]) -> bool) {
    let start = Instant::now();
    while !done(apps) {
        assert!(
            start.elapsed() < Duration::from_secs(15),
            "timed out: {what}"
        );
        for app in apps.iter_mut() {
            app.update();
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn within(viewer: Viewer, margin: u32) -> impl Fn(&TilePos) -> bool {
    move |pos| {
        pos.chebyshev(viewer.center)
            .is_some_and(|d| d <= viewer.radius + margin)
    }
}

fn state(server: &mut App, keep: impl Fn(Entity, &TilePos) -> bool) -> Vec<Seen> {
    let mut query = server
        .world_mut()
        .query_filtered::<(Entity, &TilePos, &Appearance, Option<&Facing>), With<Networked>>();
    let mut seen: Vec<Seen> = query
        .iter(server.world())
        .filter(|(e, pos, ..)| keep(*e, pos))
        .map(|(_, p, a, f)| (p.x, p.y, p.z, a.icon_state.0, f.map(|f| f.0 as u8)))
        .collect();
    seen.sort();
    seen
}

/// What the server has replicated to `link`.
fn expected(server: &mut App, link: Entity) -> Vec<Seen> {
    let all: Vec<Entity> = server
        .world_mut()
        .query_filtered::<Entity, With<Networked>>()
        .iter(server.world())
        .collect();
    let interest = server.world().resource::<Interest>();
    let known: Vec<Entity> = all
        .into_iter()
        .filter(|&e| interest.sees(link, e))
        .collect();
    state(server, |e, _| known.contains(&e))
}

/// Hysteresis bounds: everything in the view box, nothing beyond the keep box.
fn in_bounds(server: &mut App, got: &[Seen], viewer: Viewer) -> bool {
    let inner = state(server, |_, p| within(viewer, 0)(p));
    let keep = within(viewer, KEEP_MARGIN);
    inner.iter().all(|s| got.contains(s))
        && got
            .iter()
            .all(|&(x, y, z, ..)| keep(&TilePos::new(x, y, z)))
}

fn seen(client: &mut App) -> Vec<Seen> {
    let mut query = client
        .world_mut()
        .query_filtered::<(&TilePos, &Appearance, Option<&Facing>), With<NetEntity>>();
    let mut seen: Vec<Seen> = query
        .iter(client.world())
        .map(|(p, a, f)| (p.x, p.y, p.z, a.icon_state.0, f.map(|f| f.0 as u8)))
        .collect();
    seen.sort();
    seen
}

fn on_tile(server: &mut App, pos: TilePos) -> Entity {
    server
        .world()
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .find(|&e| server.world().get::<Networked>(e).is_some())
        .unwrap()
}

/// Runs until both clients show exactly what the server replicated to them, and that is within
/// the hysteresis bounds of their viewers.
fn converge(
    server: &mut App,
    a: &mut App,
    b: &mut App,
    links: [Entity; 2],
    viewers: [Viewer; 2],
    what: &str,
) -> [Vec<Seen>; 2] {
    let want = |server: &mut App| links.map(|link| expected(server, link));
    run_until(&mut [server, a, b], what, |apps| {
        let want = want(apps[0]);
        want == [seen(apps[1]), seen(apps[2])]
            && (0..2).all(|i| in_bounds(apps[0], &want[i], viewers[i]))
    });
    let want = want(server);
    for (client, want) in [&*a, &*b].into_iter().zip(&want) {
        // One local entity per replicated entity, no leftovers without components.
        assert_eq!(client.world().resource::<NetEntityMap>().len(), want.len());
    }
    want
}

#[test]
fn clients_see_exactly_their_view_and_follow_changes() {
    let server_addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 0));
    let mut server = server_app(server_addr);
    server.update();
    let server_addr = server.world().resource::<NetServerConfig>().bind_addr;
    let digest = server
        .world()
        .resource::<ServerCertificateDigest>()
        .0
        .clone();
    let config = |t: &str| NetClientConfig {
        server_addr,
        certificate_digest: digest.clone(),
        token: t.into(),
    };

    // Connect one at a time to know which link is which client.
    let mut a = client_app(config("a"));
    run_until(&mut [&mut server, &mut a], "client a connects", |apps| {
        !apps[0].world().resource::<Connected>().0.is_empty()
            && *apps[1].world().resource::<NetClientState>() == NetClientState::Connected
    });
    let mut b = client_app(config("b"));
    run_until(
        &mut [&mut server, &mut a, &mut b],
        "client b connects",
        |apps| {
            apps[0].world().resource::<Connected>().0.len() == 2
                && *apps[2].world().resource::<NetClientState>() == NetClientState::Connected
        },
    );
    let [link_a, link_b] = server.world().resource::<Connected>().0[..] else {
        unreachable!()
    };
    assert!(
        seen(&mut a).is_empty(),
        "nothing replicates without a viewer"
    );

    // Adjacent boxes: a covers x 6..=14, b covers x 16..=24.
    let mut va = Viewer {
        center: TilePos::new(10, 10, 1),
        radius: 4,
    };
    let vb = Viewer {
        center: TilePos::new(20, 10, 1),
        radius: 4,
    };
    server.world_mut().entity_mut(link_a).insert(va);
    server.world_mut().entity_mut(link_b).insert(vb);
    let [seen_a, seen_b] = converge(
        &mut server,
        &mut a,
        &mut b,
        [link_a, link_b],
        [va, vb],
        "initial views",
    );
    assert_eq!((seen_a.len(), seen_b.len()), (16, 16));

    // Across the boundary: leaves a, enters b.
    let crossing = on_tile(&mut server, TilePos::new(13, 11, 1));
    server
        .world_mut()
        .entity_mut(crossing)
        .insert(TilePos::new(18, 12, 1));
    let [seen_a, seen_b] = converge(
        &mut server,
        &mut a,
        &mut b,
        [link_a, link_b],
        [va, vb],
        "crossing",
    );
    assert_eq!((seen_a.len(), seen_b.len()), (15, 17));

    // Facing and appearance of an entity inside a's view.
    let turned = on_tile(&mut server, TilePos::new(9, 9, 1));
    server
        .world_mut()
        .entity_mut(turned)
        .insert((Facing(Dir::North), appearance(7)));
    // A step inside the view goes the motion path.
    let walker = on_tile(&mut server, TilePos::new(11, 11, 1));
    server
        .world_mut()
        .entity_mut(walker)
        .insert((TilePos::new(12, 12, 1), Facing(Dir::NorthEast)));
    converge(
        &mut server,
        &mut a,
        &mut b,
        [link_a, link_b],
        [va, vb],
        "facing and appearance",
    );
    let id = server
        .world()
        .resource::<Interest>()
        .net_id(link_a, turned)
        .unwrap();
    let local = a.world().resource::<NetEntityMap>().get(id).unwrap();
    assert_eq!(a.world().get::<Facing>(local), Some(&Facing(Dir::North)));
    assert_eq!(a.world().get::<NetEntity>(local), Some(&NetEntity(id)));

    // Optics: added, changed and removed in view. Entities carry only the tile lighting; the
    // opacity goes with the light blocks (doors toggle it at runtime).
    let door = on_tile(&mut server, TilePos::new(7, 7, 1));
    let door_id = server
        .world()
        .resource::<Interest>()
        .net_id(link_a, door)
        .unwrap();
    let optics_on = |client: &App, id| {
        let local = client.world().resource::<NetEntityMap>().get(id)?;
        client.world().get::<Optics>(local).cloned()
    };
    let closed = Optics {
        opaque: true,
        tile: Some(TileLighting::Space),
        light: None,
    };
    let seen_closed = Optics {
        tile: Some(TileLighting::Space),
        ..default()
    };
    let opaque_on =
        |client: &App| (client.world().resource::<LightBlocks>()).opaque(TilePos::new(7, 7, 1));
    assert_eq!(opaque_on(&a), Some(false));
    server.world_mut().entity_mut(door).insert(closed.clone());
    run_until(&mut [&mut server, &mut a], "optics added", |apps| {
        optics_on(apps[1], door_id).as_ref() == Some(&seen_closed)
            && opaque_on(apps[1]) == Some(true)
    });
    server.world_mut().get_mut::<Optics>(door).unwrap().opaque = false;
    run_until(&mut [&mut server, &mut a], "opacity changed", |apps| {
        opaque_on(apps[1]) == Some(false)
    });
    assert_eq!(optics_on(&a, door_id).as_ref(), Some(&seen_closed));
    let lit = TileLighting::new(true, [1, 2, 3, 4]);
    server.world_mut().get_mut::<Optics>(door).unwrap().tile = Some(lit);
    run_until(
        &mut [&mut server, &mut a],
        "tile lighting changed",
        |apps| optics_on(apps[1], door_id).and_then(|o| o.tile) == Some(lit),
    );
    server.world_mut().entity_mut(door).remove::<Optics>();
    run_until(&mut [&mut server, &mut a], "optics removed", |apps| {
        optics_on(apps[1], door_id).is_none()
    });
    // Outside a's view now; arrives with its spawn when the view moves over it.
    let wall = on_tile(&mut server, TilePos::new(17, 13, 1));
    server.world_mut().entity_mut(wall).insert(closed.clone());

    // Control hook.
    server
        .world_mut()
        .entity_mut(link_a)
        .insert(ControlledEntity(turned));
    run_until(&mut [&mut server, &mut a], "control", |apps| {
        apps[1].world().resource::<NetEntityMap>().controlled() == Some(local)
    });
    assert!(a.world().get::<LocalControl>(local).is_some());

    // The viewer moves a few tiles (strip update), then jumps to another z-level.
    va.center = TilePos::new(14, 12, 1);
    server.world_mut().entity_mut(link_a).insert(va);
    converge(
        &mut server,
        &mut a,
        &mut b,
        [link_a, link_b],
        [va, vb],
        "viewer step",
    );
    assert!(server.world().resource::<Interest>().sees(link_a, crossing));
    let wall_id = server
        .world()
        .resource::<Interest>()
        .net_id(link_a, wall)
        .unwrap();
    assert_eq!(optics_on(&a, wall_id), Some(seen_closed));
    va = Viewer {
        center: TilePos::new(30, 10, 2),
        radius: 12,
    };
    server.world_mut().entity_mut(link_a).insert(va);
    let [seen_a, _] = converge(
        &mut server,
        &mut a,
        &mut b,
        [link_a, link_b],
        [va, vb],
        "other z",
    );
    assert_eq!(seen_a.len(), 3);

    // Despawn on the server despawns on the client.
    let doomed = on_tile(&mut server, TilePos::new(21, 9, 1));
    server.world_mut().despawn(doomed);
    let [_, seen_b] = converge(
        &mut server,
        &mut a,
        &mut b,
        [link_a, link_b],
        [va, vb],
        "despawn",
    );
    assert_eq!(seen_b.len(), 16);

    let stats = server.world().resource::<ReplicationStats>();
    assert!(stats.clients.contains_key(&link_a));
}

fn stats(server: &App) -> (u64, u64, u64) {
    let s = server.world().resource::<ReplicationStats>();
    (s.spawned_total, s.despawned_total, s.appearances_sent_total)
}

/// Runs a few ticks so that everything sent has arrived.
fn settle(apps: &mut [&mut App]) {
    for _ in 0..20 {
        for app in apps.iter_mut() {
            app.update();
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn appearances_are_interned_per_client_and_view_edges_have_hysteresis() {
    let server_addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 0));
    let mut server = server_app(server_addr);
    // z 3: every tile filled, three appearances shared by all (a floor-like station).
    server
        .world_mut()
        .resource_mut::<SpatialIndex>()
        .create_level(3, 64, 64)
        .unwrap();
    for x in 1..=64 {
        for y in 1..=64 {
            server.world_mut().spawn((
                Networked,
                TilePos::new(x, y, 3),
                appearance(500 + (x % 3) as u32),
            ));
        }
    }
    server.update();
    let server_addr = server.world().resource::<NetServerConfig>().bind_addr;
    let digest = server
        .world()
        .resource::<ServerCertificateDigest>()
        .0
        .clone();
    let config = |t: &str| NetClientConfig {
        server_addr,
        certificate_digest: digest.clone(),
        token: t.into(),
    };
    let mut a = client_app(config("a"));
    run_until(&mut [&mut server, &mut a], "client a connects", |apps| {
        !apps[0].world().resource::<Connected>().0.is_empty()
            && *apps[1].world().resource::<NetClientState>() == NetClientState::Connected
    });
    let link_a = server.world().resource::<Connected>().0[0];
    let mut va = Viewer {
        center: TilePos::new(20, 20, 3),
        radius: 4,
    };
    server.world_mut().entity_mut(link_a).insert(va);
    run_until(&mut [&mut server, &mut a], "a sees 81 tiles", |apps| {
        seen(apps[1]).len() == 81
    });
    // 81 spawns, 3 appearance values.
    assert_eq!(stats(&server), (81, 0, 3));
    assert_eq!(a.world().resource::<NetEntityMap>().cached_appearances(), 3);
    let interest = server.world().resource::<Interest>();
    assert_eq!(interest.cached_appearances(link_a), 3);
    assert_eq!(interest.interned_appearances(), 3);

    // A late joiner gets the values of ids the first client already has.
    let mut b = client_app(config("b"));
    run_until(
        &mut [&mut server, &mut a, &mut b],
        "client b connects",
        |apps| {
            apps[0].world().resource::<Connected>().0.len() == 2
                && *apps[2].world().resource::<NetClientState>() == NetClientState::Connected
        },
    );
    let link_b = server.world().resource::<Connected>().0[1];
    let vb = va;
    server.world_mut().entity_mut(link_b).insert(vb);
    run_until(
        &mut [&mut server, &mut a, &mut b],
        "b sees the same",
        |apps| seen(apps[2]) == seen(apps[1]),
    );
    assert_eq!(stats(&server), (162, 0, 6));

    // One step east spawns one column and despawns nothing (margin); stepping back costs nothing.
    va.center = TilePos::new(21, 20, 3);
    server.world_mut().entity_mut(link_a).insert(va);
    settle(&mut [&mut server, &mut a]);
    assert_eq!(stats(&server), (162 + 9, 0, 6));
    for _ in 0..3 {
        for x in [20, 21] {
            va.center = TilePos::new(x, 20, 3);
            server.world_mut().entity_mut(link_a).insert(va);
            settle(&mut [&mut server, &mut a]);
        }
    }
    assert_eq!(stats(&server), (162 + 9, 0, 6), "oscillation is free");
    // Walking on: the trailing column leaves only once it is beyond the margin (column 16 at
    // the third step), then one column in and one out per step.
    va.center = TilePos::new(22, 20, 3);
    server.world_mut().entity_mut(link_a).insert(va);
    settle(&mut [&mut server, &mut a]);
    assert_eq!(stats(&server), (162 + 18, 0, 6));
    for (x, out) in [(23, 9), (24, 18)] {
        va.center = TilePos::new(x, 20, 3);
        server.world_mut().entity_mut(link_a).insert(va);
        settle(&mut [&mut server, &mut a]);
        assert_eq!(stats(&server), (162 + 9 * (x - 20) as u64, out, 6));
    }
    converge(
        &mut server,
        &mut a,
        &mut b,
        [link_a, link_b],
        [va, vb],
        "after walking",
    );

    // Leaving the level empties a's cache on both sides; b keeps its own.
    va.center = TilePos::new(30, 10, 2);
    server.world_mut().entity_mut(link_a).insert(va);
    run_until(&mut [&mut server, &mut a, &mut b], "a left", |apps| {
        let got = seen(apps[1]);
        !got.is_empty() && got.iter().all(|s| s.2 == 2)
    });
    // One distinct appearance per z 2 entity, nothing left from z 3.
    let on_z2 = seen(&mut a).len();
    assert_eq!(
        a.world().resource::<NetEntityMap>().cached_appearances(),
        on_z2
    );
    let interest = server.world().resource::<Interest>();
    assert_eq!(interest.cached_appearances(link_a), on_z2);
    assert_eq!(interest.cached_appearances(link_b), 3);

    // Despawning every user of one appearance evicts it from the server table and b's cache.
    let mut q = server
        .world_mut()
        .query::<(Entity, &TilePos, &Appearance)>();
    let doomed: Vec<Entity> = q
        .iter(server.world())
        .filter(|(_, p, a)| p.z == 3 && a.icon_state == IconStateId(501))
        .map(|(e, ..)| e)
        .collect();
    let before = server.world().resource::<Interest>().interned_appearances();
    for e in doomed {
        server.world_mut().despawn(e);
    }
    run_until(&mut [&mut server, &mut a, &mut b], "b drops 501", |apps| {
        apps[2]
            .world()
            .resource::<NetEntityMap>()
            .cached_appearances()
            == 2
    });
    let interest = server.world().resource::<Interest>();
    assert_eq!(interest.cached_appearances(link_b), 2);
    assert_eq!(interest.interned_appearances(), before - 1);
}

fn lamp(range: f32, color: [u8; 3]) -> Optics {
    Optics {
        light: Some(LightSource {
            range,
            power: 1.0,
            color,
            height: 1.0,
            angle: 360,
            dir: Dir::South,
            offset: [0, 0],
        }),
        ..default()
    }
}

/// Light sources and opacity beyond the entity view, over real WebTransport: a lamp and a door
/// 15 tiles away are known to the client without their entities, an outage and the door
/// opening reach it, and leaving the level forgets the blocks.
#[test]
fn light_blocks_reach_beyond_the_view_and_follow_changes() {
    let server_addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 0));
    let mut server = server_app(server_addr);
    server
        .world_mut()
        .resource_mut::<ReplicationStats>()
        .count_payload = true;
    let center = TilePos::new(20, 20, 1);
    let (lamp_pos, door_pos) = (TilePos::new(35, 20, 1), TilePos::new(35, 22, 1));
    let lamp_e = (server.world_mut())
        .spawn((
            Networked,
            lamp_pos,
            appearance(1),
            lamp(8.0, [255, 255, 255]),
        ))
        .id();
    let door = (server.world_mut())
        .spawn((
            Networked,
            door_pos,
            appearance(2),
            Optics {
                opaque: true,
                ..default()
            },
        ))
        .id();
    server.update();
    let server_addr = server.world().resource::<NetServerConfig>().bind_addr;
    let digest = (server.world().resource::<ServerCertificateDigest>().0).clone();
    let mut a = client_app(NetClientConfig {
        server_addr,
        certificate_digest: digest,
        token: "a".into(),
    });
    run_until(&mut [&mut server, &mut a], "client connects", |apps| {
        !apps[0].world().resource::<Connected>().0.is_empty()
            && *apps[1].world().resource::<NetClientState>() == NetClientState::Connected
    });
    let link = server.world().resource::<Connected>().0[0];
    server
        .world_mut()
        .entity_mut(link)
        .insert(Viewer::new(center));

    let lights_at = |client: &App, pos| {
        (client.world().resource::<LightBlocks>())
            .lights(pos)
            .map(<[LightSource]>::to_vec)
    };
    let opaque_at = |client: &App, pos| client.world().resource::<LightBlocks>().opaque(pos);
    let white = lamp(8.0, [255, 255, 255]).light.unwrap();
    run_until(
        &mut [&mut server, &mut a],
        "the far lamp and door",
        |apps| {
            lights_at(apps[1], lamp_pos) == Some(vec![white])
                && opaque_at(apps[1], door_pos) == Some(true)
        },
    );
    let interest = server.world().resource::<Interest>();
    assert!(!interest.sees(link, lamp_e) && !interest.sees(link, door));
    // 21x21 view + 11 tiles each side = 43x43 tiles: 3 or 4 blocks per axis.
    let blocks = interest.light_blocks(link);
    assert!((9..=16).contains(&blocks), "{blocks} blocks");
    assert_eq!(a.world().resource::<LightBlocks>().len(), blocks);
    let join = server.world().resource::<ReplicationStats>().clients[&link].light_bytes_total;
    println!("light blocks at join: {blocks} blocks, {join} bytes");
    assert!(join < 2000, "{join} bytes");

    // An outage turns the lamp into an emergency light; the door opens.
    let emergency = lamp(2.0, [255, 50, 50]);
    server
        .world_mut()
        .entity_mut(lamp_e)
        .insert(emergency.clone());
    server.world_mut().get_mut::<Optics>(door).unwrap().opaque = false;
    run_until(&mut [&mut server, &mut a], "outage and door", |apps| {
        lights_at(apps[1], lamp_pos) == Some(vec![emergency.light.unwrap()])
            && opaque_at(apps[1], door_pos) == Some(false)
    });
    let after = server.world().resource::<ReplicationStats>().clients[&link].light_bytes_total;
    println!("outage of one lamp + one door: {} bytes", after - join);
    // Off, and a light block of an untouched lamp did not change.
    server.world_mut().entity_mut(lamp_e).remove::<Optics>();
    run_until(&mut [&mut server, &mut a], "lamp off", |apps| {
        lights_at(apps[1], lamp_pos) == Some(vec![])
    });
    assert_eq!(a.world().resource::<LightBlocks>().sources(), 0);

    // Beyond the light margin: nothing is known there.
    let far = TilePos::new(
        20 + (crate::server::DEFAULT_VIEW_RADIUS + LIGHT_MARGIN) as i32 + 17,
        20,
        1,
    );
    assert_eq!(opaque_at(&a, far), None);

    // Another level: every block of z 1 is forgotten.
    server
        .world_mut()
        .entity_mut(link)
        .insert(Viewer::new(TilePos::new(30, 10, 2)));
    run_until(&mut [&mut server, &mut a], "z 1 forgotten", |apps| {
        opaque_at(apps[1], door_pos).is_none()
            && opaque_at(apps[1], TilePos::new(30, 10, 2)).is_some()
    });
}

/// A client with a [`ViewMask`] (an AI eye under camera static) gets only the entities on tiles
/// its mask shows, plus the entity it controls; the light blocks carry every light and the
/// tiles the mask shows; mask changes spawn and despawn by tile; dropping the mask shows the box.
#[test]
fn masked_view_replicates_only_visible_tiles() {
    let server_addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 0));
    let mut server = server_app(server_addr);
    let center = TilePos::new(20, 20, 1);
    let lamp_pos = TilePos::new(24, 24, 1);
    let lamp_e = (server.world_mut())
        .spawn((
            Networked,
            lamp_pos,
            appearance(7),
            lamp(4.0, [255, 255, 255]),
        ))
        .id();
    // The controlled entity (the eye) stands under static.
    let eye_pos = TilePos::new(20, 26, 1);
    let eye = (server.world_mut())
        .spawn((Networked, eye_pos, appearance(8)))
        .id();
    let mask = MaskId(1);
    // Visible: x 15-25, y 15-19 (the room a camera sees).
    let room = |extra_rows: i32| {
        (15..=25).flat_map(move |x| (15..=19 + extra_rows).map(move |y| TilePos::new(x, y, 1)))
    };
    server
        .world_mut()
        .resource_mut::<ViewMasks>()
        .set_tiles(mask, room(0));
    server.update();
    let server_addr = server.world().resource::<NetServerConfig>().bind_addr;
    let digest = (server.world().resource::<ServerCertificateDigest>().0).clone();
    let mut a = client_app(NetClientConfig {
        server_addr,
        certificate_digest: digest,
        token: "a".into(),
    });
    run_until(&mut [&mut server, &mut a], "client connects", |apps| {
        !apps[0].world().resource::<Connected>().0.is_empty()
            && *apps[1].world().resource::<NetClientState>() == NetClientState::Connected
    });
    let link = server.world().resource::<Connected>().0[0];
    server.world_mut().entity_mut(link).insert((
        Viewer::new(center),
        ViewMask(mask),
        ControlledEntity(eye),
    ));

    // `rows`: extra visible rows, `None` unmasked.
    let visible = move |rows: Option<i32>, e: Entity, p: &TilePos| {
        e == eye || rows.is_none_or(|rows| room(rows).any(|r| r == *p))
    };
    let check = |server: &mut App, a: &mut App, rows: Option<i32>, what: &str| {
        let viewer = Viewer::new(center);
        run_until(&mut [server, a], what, |apps| {
            let want = state(apps[0], |e, p| within(viewer, 0)(p) && visible(rows, e, p));
            seen(apps[1]) == want && expected(apps[0], link) == want
        });
    };
    check(&mut server, &mut a, Some(0), "only the room and the eye");
    let interest = server.world().resource::<Interest>();
    assert!(interest.sees(link, eye), "the eye under static is shown");
    assert!(!interest.sees(link, lamp_e));
    let lights = |a: &App| {
        a.world()
            .resource::<LightBlocks>()
            .lights(lamp_pos)
            .map(<[_]>::to_vec)
    };
    let opaque = |a: &App, p| a.world().resource::<LightBlocks>().opaque(p);
    let seen_tile = |a: &App, p| a.world().resource::<LightBlocks>().seen(p);
    // tg lights a seen tile from sources it cannot see: lights and opacity go unmasked, the
    // client only learns which tiles to draw.
    assert_eq!(
        lights(&a).map(|l| l.len()),
        Some(1),
        "a hidden lamp still lights"
    );
    assert_eq!(
        opaque(&a, lamp_pos),
        Some(false),
        "hidden floors stay clear"
    );
    assert_eq!(seen_tile(&a, lamp_pos), Some(false));
    assert_eq!(seen_tile(&a, TilePos::new(17, 17, 1)), Some(true));
    run_until(&mut [&mut server, &mut a], "the eye arrives", |apps| {
        apps[1]
            .world()
            .resource::<NetEntityMap>()
            .controlled()
            .is_some()
    });

    // An entity walks out of the room: it is gone at once (no hysteresis under static).
    let walker = on_tile(&mut server, TilePos::new(17, 19, 1));
    server
        .world_mut()
        .entity_mut(walker)
        .insert(TilePos::new(17, 21, 1));
    check(&mut server, &mut a, Some(0), "the walker left the room");
    assert!(!server.world().resource::<Interest>().sees(link, walker));

    // A door opens: the camera sees five more rows, the lamp's tile included.
    server
        .world_mut()
        .resource_mut::<ViewMasks>()
        .set_tiles(mask, room(5));
    check(&mut server, &mut a, Some(5), "the room grew");
    assert!(server.world().resource::<Interest>().sees(link, walker));
    assert!(server.world().resource::<Interest>().sees(link, lamp_e));
    run_until(&mut [&mut server, &mut a], "the lamp's tile seen", |apps| {
        seen_tile(apps[1], lamp_pos) == Some(true)
    });
    assert_eq!(lights(&a).map(|l| l.len()), Some(1));

    // It closes again: back to the room.
    server
        .world_mut()
        .resource_mut::<ViewMasks>()
        .set_tiles(mask, room(0));
    check(&mut server, &mut a, Some(0), "the room shrank");
    run_until(
        &mut [&mut server, &mut a],
        "the lamp's tile hidden",
        |apps| seen_tile(apps[1], lamp_pos) == Some(false),
    );

    // The eye walks into the room and out to the hidden lamp: still shown.
    server
        .world_mut()
        .entity_mut(eye)
        .insert(TilePos::new(23, 24, 1));
    check(&mut server, &mut a, Some(0), "the eye moved under static");

    // No mask: the whole box.
    server.world_mut().entity_mut(link).remove::<ViewMask>();
    check(&mut server, &mut a, None, "unmasked");
    run_until(&mut [&mut server, &mut a], "the lamp unmasked", |apps| {
        seen_tile(apps[1], lamp_pos) == Some(true) && lights(apps[1]).is_some_and(|l| l.len() == 1)
    });
}

/// Line of sight over real WebTransport: behind a wall row (y 25) with one door (21, 25) a
/// player gets nothing; the door opening shows what is in line with it in the same batch as
/// its opacity, closing hides it again; an x-ray client (an observer) gets the whole box; a
/// step lets the player see from the next tile until it stands still.
#[test]
fn line_of_sight_hides_what_walls_hide() {
    use crate::server::Sight;
    use ss13_shared::line_of_sight::ViewGrid;

    let server_addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 0));
    let mut server = server_app(server_addr);
    let door_pos = TilePos::new(21, 25, 1);
    let wall = Optics {
        opaque: true,
        ..default()
    };
    let mut door = None;
    for x in 1..=63 {
        let e = (server.world_mut())
            .spawn((
                Networked,
                TilePos::new(x, 25, 1),
                appearance(9),
                wall.clone(),
            ))
            .id();
        if x == door_pos.x {
            door = Some(e);
        }
    }
    let door = door.unwrap();
    // A lamp behind the wall: its entity is hidden, its light is not (tg lights the seen side
    // of the wall from it).
    let lamp_pos = TilePos::new(26, 28, 1);
    server.world_mut().spawn((
        Networked,
        lamp_pos,
        appearance(7),
        lamp(3.0, [255, 255, 255]),
    ));
    server.update();
    let server_addr = server.world().resource::<NetServerConfig>().bind_addr;
    let digest = (server.world().resource::<ServerCertificateDigest>().0).clone();
    let config = |token: &str| NetClientConfig {
        server_addr,
        certificate_digest: digest.clone(),
        token: token.into(),
    };
    // One after the other, so `links[i]` is the i-th client.
    let mut a = client_app(config("a"));
    run_until(&mut [&mut server, &mut a], "a connects", |apps| {
        apps[0].world().resource::<Connected>().0.len() == 1
    });
    let mut b = client_app(config("b"));
    run_until(&mut [&mut server, &mut a, &mut b], "b connects", |apps| {
        apps[0].world().resource::<Connected>().0.len() == 2
            && [1, 2].iter().all(|&i| {
                *apps[i].world().resource::<NetClientState>() == NetClientState::Connected
            })
    });
    let links: Vec<Entity> = server.world().resource::<Connected>().0.clone();
    let center = TilePos::new(21, 20, 1);
    server
        .world_mut()
        .entity_mut(links[0])
        .insert(Viewer::new(center));
    server
        .world_mut()
        .entity_mut(links[1])
        .insert((Viewer::new(center), Sight::XRAY));
    let viewer = Viewer::new(center);
    let check = |server: &mut App,
                 a: &mut App,
                 b: &mut App,
                 want_a: &dyn Fn(&TilePos) -> bool,
                 what: &str| {
        run_until(&mut [server, a, b], what, |apps| {
            let want = state(apps[0], |_, p| within(viewer, 0)(p) && want_a(p));
            let all = state(apps[0], |_, p| within(viewer, 0)(p));
            seen(apps[1]) == want && expected(apps[0], links[0]) == want && seen(apps[2]) == all
        });
    };
    check(
        &mut server,
        &mut a,
        &mut b,
        &|p| p.y <= 25,
        "nothing behind the wall",
    );
    let opaque = |a: &App, p| a.world().resource::<LightBlocks>().opaque(p);
    let lights = |a: &App, p| {
        (a.world().resource::<LightBlocks>())
            .lights(p)
            .map(<[_]>::to_vec)
    };
    // The client draws its own line of sight from the true opacity: no seen bits, no masking.
    let los = |a: &App| a.world().resource::<LightBlocks>().line_of_sight();
    assert_eq!(los(&a), Some(12));
    assert_eq!(los(&b), None, "x-ray sees all");
    assert_eq!(opaque(&a, TilePos::new(21, 27, 1)), Some(false));
    assert_eq!(opaque(&a, TilePos::new(22, 25, 1)), Some(true));
    let seen_tile = |a: &App, p| a.world().resource::<LightBlocks>().seen(p);
    assert_eq!(
        seen_tile(&a, TilePos::new(21, 27, 1)),
        Some(true),
        "no seen bits"
    );
    assert_eq!(lights(&a, lamp_pos).map(|l| l.len()), Some(1));

    // The door opens: the column in line with it shows the frame the door does.
    let behind = on_tile(&mut server, TilePos::new(21, 29, 1));
    server.world_mut().get_mut::<Optics>(door).unwrap().opaque = false;
    let start = Instant::now();
    loop {
        server.update();
        a.update();
        b.update();
        let open = opaque(&a, door_pos) == Some(false);
        let shown = a
            .world_mut()
            .query::<&TilePos>()
            .iter(a.world())
            .any(|p| *p == TilePos::new(21, 29, 1));
        assert_eq!(
            open, shown,
            "the door and what is behind it arrive together"
        );
        if open {
            break;
        }
        assert!(start.elapsed() < Duration::from_secs(15), "the door opens");
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(server.world().resource::<Interest>().sees(links[0], behind));
    let mut grid = ViewGrid::default();
    let walls = |closed: bool| {
        move |x: i32, y: i32| y == 25 && (closed || x != door_pos.x) || !(1..=64).contains(&x)
    };
    let sees_from = |grid: &mut ViewGrid, eye: TilePos, p: &TilePos, closed: bool| {
        let opaque = walls(closed);
        grid.compute(12, |dx, dy| opaque(eye.x + dx, eye.y + dy));
        // Sent: the seen tiles and the glimpsed ring past them.
        let (dx, dy) = (p.x - eye.x, p.y - eye.y);
        grid.visible(dx, dy) || grid.glimpsed(dx, dy)
    };
    let open_view: Vec<TilePos> = state(&mut server, |_, p| within(viewer, 0)(p))
        .iter()
        .map(|s| TilePos::new(s.0, s.1, s.2))
        .filter(|p| sees_from(&mut grid, center, p, false))
        .collect();
    assert!(
        open_view.iter().any(|p| p.y > 26),
        "the door shows the far side"
    );
    check(
        &mut server,
        &mut a,
        &mut b,
        &|p| open_view.contains(p),
        "the door open",
    );

    // It closes: hidden again.
    server.world_mut().get_mut::<Optics>(door).unwrap().opaque = true;
    check(
        &mut server,
        &mut a,
        &mut b,
        &|p| p.y <= 25,
        "the door closed",
    );
    assert!(!server.world().resource::<Interest>().sees(links[0], behind));

    // Open again and step east: for a few ticks the player also sees from the tile beyond
    // the step, then from its own tile only.
    server.world_mut().get_mut::<Optics>(door).unwrap().opaque = false;
    check(
        &mut server,
        &mut a,
        &mut b,
        &|p| open_view.contains(p),
        "reopened",
    );
    // From beside the door's column BYOND shows a wedge to one side of it, from in line with
    // it both sides: stepping toward that line sees the other side a step early.
    let (here, next) = (TilePos::new(20, 20, 1), TilePos::new(21, 20, 1));
    server
        .world_mut()
        .entity_mut(links[0])
        .insert(Viewer::new(TilePos::new(19, 20, 1)));
    for _ in 0..=crate::replication_server::LOOKAHEAD_TICKS_FOR_TESTS {
        server.update();
    }
    let ahead_only = (26..=31)
        .flat_map(|y| (11..=31).map(move |x| TilePos::new(x, y, 1)))
        .find(|p| sees_from(&mut grid, next, p, false) && !sees_from(&mut grid, here, p, false))
        .expect("a tile seen from the next tile only");
    let entity = (server.world_mut())
        .spawn((Networked, ahead_only, appearance(5)))
        .id();
    server.update();
    server
        .world_mut()
        .entity_mut(links[0])
        .insert(Viewer::new(here));
    server.update();
    assert!(
        server.world().resource::<Interest>().sees(links[0], entity),
        "the step sees ahead at once"
    );
    for _ in 0..crate::replication_server::LOOKAHEAD_TICKS_FOR_TESTS {
        server.update();
    }
    assert!(
        !server.world().resource::<Interest>().sees(links[0], entity),
        "standing still sees from its own tile"
    );
}
