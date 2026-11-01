//! The playable loop over real WebTransport on 127.0.0.1: a fixture station loaded by the real map
//! loader, headless clients that join, walk, bump a wall, spam moves and see each other.
//!
//! `cargo test -p ss13_game --all-features --test it play::metastation -- --ignored` also runs the
//! join-and-walk part on real MetaStation (needs generated assets in `SS13_ASSETS`).

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_assets::manifest::{AssetManifest, IconStateInfo};
use ss13_game::RUN_DELAY_SECS;
use ss13_game::server::{PlayerMob, Players};
use ss13_game::testing::{Drive, Station, fixture_map};
use ss13_mapload::MapLoadPlugin;
use ss13_mapload::convert::{DmTypes, convert};
use ss13_net::client::NetEntityMap;
use ss13_net::server::{Networked, ReplicationStats, Viewer};
use ss13_shared::protocol::ClientIntent;
use ss13_shared::{Appearance, Dir, Facing, IconStateId, TilePos};
use ss13_world::SpatialIndex;

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3 } },
    "/obj": { "parent": "/atom/movable" },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start",
        "declared_vars": { "icon_state": "Assistant" } },
    "/mob": { "parent": "/atom/movable", "declared_vars": { "layer": 4, "density": 1 } },
    "/mob/living": { "parent": "/mob" },
    "/mob/living/carbon": { "parent": "/mob/living" },
    "/mob/living/carbon/human": { "parent": "/mob/living/carbon", "declared_vars": {
        "icon": { "resource": "h.dmi" }, "icon_state": "human_basic" } },
    "/turf": { "parent": "/atom", "declared_vars": { "icon": { "resource": "t.dmi" } } },
    "/turf/open": { "parent": "/turf" },
    "/turf/open/floor": { "parent": "/turf/open", "declared_vars": { "icon_state": "floor" } },
    "/turf/open/space": { "parent": "/turf/open" },
    "/turf/open/space/basic": { "parent": "/turf/open/space" },
    "/turf/closed": { "parent": "/turf", "declared_vars": { "density": 1, "opacity": 1 } },
    "/turf/closed/wall": { "parent": "/turf/closed", "declared_vars": { "icon_state": "wall" } },
    "/area": { "parent": "/atom" },
    "/area/space": { "parent": "/area" }
  }
}"##;

/// Rows top-down: the last line is y = 1. One spawn landmark at (3, 3) with a wall east of it.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwwwwwwwwwww
wffffffffffffw
wffffffffffffw
wfswfffffffffw
wffffffffffffw
wwwwwwwwwwwwww
"}
"##;

const SPAWN: TilePos = TilePos::new(3, 3, 1);

/// Manifest order = `IconStateId`.
const STATES: [(&str, &str); 6] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("t.dmi", ""),
    ("l.dmi", "x2"),
    ("l.dmi", "Assistant"),
    ("h.dmi", "human_basic"),
];
const LANDMARK_ICON: IconStateId = IconStateId(4);
const HUMAN_ICON: IconStateId = IconStateId(5);

/// Without `RUNTIME_TYPES` (no assistant kit): the mob spawns bare, as `write_fixture` cannot.
fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("play_fixture")
        .join(test);
    std::fs::create_dir_all(dir.join("prototypes")).unwrap();
    let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/mob/living/carbon/human",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
    ];
    let conversion = convert(&types, used).unwrap();
    std::fs::write(dir.join("prototypes/dm_types.ron"), conversion.ron).unwrap();
    let manifest = AssetManifest {
        atlases: vec![],
        states: STATES
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
    std::fs::write(dir.join("map.dmm"), MAP).unwrap();
    dir
}

/// One new step of `p`, sent `times` times (repeats of a `seq` are ignored by the server).
fn send(s: &mut Station, p: usize, dir: Dir, times: usize) {
    s.players[p].seq += 1;
    let seq = s.players[p].seq;
    for _ in 0..times {
        s.send(p, ClientIntent::Move { dir, seq });
    }
}

/// `p`'s own mob as its client sees it.
fn local_mob(s: &Station, p: usize) -> Option<TilePos> {
    let client = &s.players[p].app;
    let mob = client.world().resource::<NetEntityMap>().controlled()?;
    client.world().get::<TilePos>(mob).copied()
}

fn pos(s: &Station, entity: Entity) -> TilePos {
    *s.get::<TilePos>(entity)
}

/// Entities `p`'s client draws with `icon`.
fn drawn_with(s: &mut Station, p: usize, icon: IconStateId) -> usize {
    let world = s.players[p].app.world_mut();
    let mut q = world.query::<&Appearance>();
    q.iter(world).filter(|a| a.icon_state == icon).count()
}

#[test]
fn join_walk_bump_spam_and_see_each_other() {
    let assets = fixture_assets("join_walk");
    let mut s = Station::start(fixture_map(&assets, &["map.dmm"]), |_| {});

    // The landmark exists on the server but is never replicated.
    let landmark = {
        let world = s.server.world();
        let on_spawn = world.resource::<SpatialIndex>().on_tile(SPAWN).to_vec();
        on_spawn
            .into_iter()
            .find(|&e| {
                world
                    .get::<Appearance>(e)
                    .is_some_and(|a| a.icon_state == LANDMARK_ICON)
            })
            .expect("landmark spawned with an appearance")
    };
    assert!(s.server.world().get::<Networked>(landmark).is_none());

    let a = s.join("a", |_| {});
    let (link_a, mob_a) = (s.players[a].link, s.players[a].mob);
    assert_eq!(pos(&s, mob_a), SPAWN);
    assert_eq!(local_mob(&s, a), Some(SPAWN));
    // Walls and floors around it are drawn, the landmark under it is not.
    s.run_until("a sees the room", |s| {
        drawn_with(s, a, IconStateId(1)) > 0 && drawn_with(s, a, IconStateId(0)) > 0
    });
    assert_eq!(drawn_with(&mut s, a, LANDMARK_ICON), 0);
    assert_eq!(drawn_with(&mut s, a, HUMAN_ICON), 1);

    // A dense wall east of the spawn: the mob turns but stays.
    send(&mut s, a, Dir::East, 1);
    s.run_until("a turns east", |s| {
        s.try_get::<Facing>(mob_a) == Some(&Facing(Dir::East))
    });
    s.run_for(0.3);
    assert_eq!(pos(&s, mob_a), SPAWN);
    assert_eq!(local_mob(&s, a), Some(SPAWN));

    // A free step north reaches the server and the client.
    send(&mut s, a, Dir::North, 1);
    let north = SPAWN.step(Dir::North);
    s.run_until("a steps north", |s| {
        pos(s, mob_a) == north && local_mob(s, a) == Some(north)
    });
    let viewer = *s.get::<Viewer>(link_a);
    assert_eq!(viewer.center, north, "the view follows the mob");

    // One step repeated 100 times in one client frame: one step.
    s.run_for(0.4);
    send(&mut s, a, Dir::East, 100);
    let burst = north.step(Dir::East);
    s.run_until("the burst's step lands", |s| pos(s, mob_a) != north);
    s.hold(0.6, "a burst is one step", |s| pos(s, mob_a) == burst);
    let after_burst = pos(&s, mob_a);

    // Sustained spam (a new step per frame) walks no faster than the cooldown allows.
    s.run_for(0.4);
    let start = s.now();
    let mut last_move = start;
    let mut last_pos = after_burst;
    // 2 s of game time: a stall of the loaded network cannot starve the lower bound.
    while s.now() - start < 2.0 {
        send(&mut s, a, Dir::East, 100);
        s.step();
        let now_pos = pos(&s, mob_a);
        if now_pos != last_pos {
            last_pos = now_pos;
            last_move = s.now();
        }
    }
    let steps = (last_pos.x - after_burst.x) as f64;
    let allowed = ((last_move - start) / f64::from(RUN_DELAY_SECS)).floor() + 1.0;
    println!(
        "spam: {steps} steps in {:.2}s of server time (cooldown allows {allowed})",
        last_move - start
    );
    assert!(
        (3.0..=allowed).contains(&steps),
        "{steps} steps in {:.2}s, allowed {allowed}",
        last_move - start
    );
    s.run_until("a settles", |s| local_mob(s, a) == Some(pos(s, mob_a)));

    // A second player joins at the free spawn and sees the first one, and vice versa.
    let b = s.join("b", |_| {});
    let (link_b, mob_b) = (s.players[b].link, s.players[b].mob);
    assert_eq!(pos(&s, mob_b), SPAWN);
    s.run_until("both players see two humans", |s| {
        drawn_with(s, a, HUMAN_ICON) == 2 && drawn_with(s, b, HUMAN_ICON) == 2
    });
    assert!(
        s.server
            .world()
            .resource::<ReplicationStats>()
            .clients
            .contains_key(&link_b)
    );

    // a leaves: its body stays on the map without a client (tg SSD), b still sees it.
    drop(s.players.remove(a));
    let b = 0;
    s.run_until("a's client is gone", |s| {
        s.server.world().resource::<Players>().len() == 1
    });
    assert!(s.server.world().get::<PlayerMob>(mob_a).is_none());
    assert_eq!(drawn_with(&mut s, b, HUMAN_ICON), 2);
}

/// Join and walk on the real station; `SS13_ASSETS` must hold the converted MetaStation assets.
/// Rates are per second of game time (one frame is one 50 ms tick, as on the real server; the
/// traffic window of `ReplicationStats` counts the same manual time).
#[test]
#[ignore = "needs generated assets and MetaStation; run with --ignored"]
fn metastation() {
    let map = MapLoadPlugin {
        maps: vec!["MetaStation".into()],
        ..MapLoadPlugin::from_env().unwrap_or(MapLoadPlugin {
            source: env!("SS13_SOURCE").into(),
            maps: vec![],
            assets: std::env::var("SS13_ASSETS")
                .unwrap_or_else(|_| "assets/generated".into())
                .into(),
        })
    };
    let mut s = Station::start(map, |_| {});
    s.server
        .world_mut()
        .resource_mut::<ReplicationStats>()
        .count_payload = true;
    let a = s.join("a", |_| {});
    let (link, mob) = (s.players[a].link, s.players[a].mob);
    let start = pos(&s, mob);
    s.run_for(1.0);
    let seen = s.players[a].app.world().resource::<NetEntityMap>().len();
    assert!(seen > 200, "only {seen} entities around the spawn");
    let idle = bytes_per_sec(&mut s, link, 3, |_, _| {});
    // Some direction is free next to a spawn landmark.
    let free = Dir::CARDINALS
        .into_iter()
        .find(|&dir| {
            send(&mut s, a, dir, 1);
            s.run_for(0.4);
            pos(&s, mob) != start
        })
        .unwrap_or_else(|| panic!("boxed in at {start:?}"));
    let moved = pos(&s, mob);
    s.run_until("client follows", |s| local_mob(s, a) == Some(moved));
    let totals = |s: &Station| {
        let stats = s.server.world().resource::<ReplicationStats>();
        [
            stats.spawned_total,
            stats.despawned_total,
            stats.appearances_sent_total,
            stats.clients[&link].replication_bytes_total,
        ]
    };
    let started = s.now();
    let before = totals(&s);
    // Walk back and forth with a new step every frame (the server paces them by the cooldown).
    let walking = bytes_per_sec(&mut s, link, 5, |s, secs| {
        let dir = if (secs as u32).is_multiple_of(2) {
            free.reverse()
        } else {
            free
        };
        send(s, a, dir, 1);
    });
    let after = totals(&s);
    let secs = s.now() - started;
    let [spawned, despawned, appearances, payload] =
        [0, 1, 2, 3].map(|i| (after[i] - before[i]) as f64 / secs);
    println!(
        "MetaStation, 1 client: {seen} entities in view; bytes/s idle {idle:.0}, walking {walking:.0}; \
         per second walking: {spawned:.0} spawned, {despawned:.0} despawned, {appearances:.0} appearances sent, \
         {payload:.0} B of replication messages (no transport)"
    );
}

/// Average of the server's per-second byte rate to `link` over `secs` whole seconds of game time,
/// calling `each_frame` with the station and the elapsed time before every frame.
fn bytes_per_sec(
    s: &mut Station,
    link: Entity,
    secs: u32,
    mut each_frame: impl FnMut(&mut Station, f64),
) -> f64 {
    let start = s.now();
    let mut samples = Vec::new();
    let mut next = 1.5;
    loop {
        let t = s.now() - start;
        if t >= next {
            let stats = s.server.world().resource::<ReplicationStats>();
            samples.push(stats.clients[&link].bytes_per_sec);
            next += 1.0;
            if samples.len() == secs as usize {
                break;
            }
        }
        each_frame(s, t);
        s.step();
    }
    samples.iter().sum::<f64>() / samples.len() as f64
}
