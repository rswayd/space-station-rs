//! The in-memory transport (`ss13_net::loopback`, what the browser build runs between its Web
//! Worker server and the page): a server and a client app in one process join over it, a move
//! intent walks the mob, and the client sees the step replicated. No QUIC session is opened.
//!
//! `metastation_tick_single_threaded` (ignored, needs `SS13_ASSETS`) measures what the browser
//! server would cost: MetaStation, one player joined over loopback, every schedule on the
//! single-threaded executor and task pools without worker threads, as on wasm32. Run it in a
//! release build: `cargo test --release -p ss13_game --all-features --test it
//! loopback::metastation -- --ignored --nocapture` (`docs/WEB.md`).

use std::path::{Path, PathBuf};
use std::time::Instant;

use bevy::ecs::schedule::SingleThreadedExecutor;
use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, ComputeTaskPool, IoTaskPool, TaskPoolBuilder};
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::MapLoadPlugin;
use ss13_net::loopback;
use ss13_shared::{Dir, TilePos};

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 }, "declared_vars": { "can_atmos_pass": 1 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3, "anchored": 0 } },
    "/obj": { "parent": "/atom/movable", "declared_vars": { "force": 0, "damtype": "brute" } },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start" },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null, "attack_speed": 8, "w_class": 3,
        "body_parts_covered": 0, "heat_protection": 0, "cold_protection": 0,
        "min_cold_protection_temperature": 0, "max_heat_protection_temperature": 0,
        "allowed": null } },
    "/obj/item/card": { "parent": "/obj/item" },
    "/obj/item/card/id": { "parent": "/obj/item/card", "declared_vars": {
        "access": { "list": [] }, "slot_flags": 256 } },
    "/obj/item/card/id/advanced": { "parent": "/obj/item/card/id" },
    "/obj/item/clothing": { "parent": "/obj/item", "declared_vars": { "clothing_flags": 0 } },
    "/obj/item/clothing/under": { "parent": "/obj/item/clothing", "declared_vars": {
        "slot_flags": 2, "body_parts_covered": 411 } },
    "/obj/item/clothing/under/color": { "parent": "/obj/item/clothing/under" },
    "/obj/item/clothing/under/color/grey": { "parent": "/obj/item/clothing/under/color" },
    "/obj/item/clothing/shoes": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 128 } },
    "/obj/item/clothing/shoes/sneakers": { "parent": "/obj/item/clothing/shoes" },
    "/obj/item/clothing/shoes/sneakers/black": { "parent": "/obj/item/clothing/shoes/sneakers" },
    "/obj/item/clothing/mask": { "parent": "/obj/item/clothing", "declared_vars": {
        "slot_flags": 32, "body_parts_covered": 4 } },
    "/obj/item/clothing/mask/breath": { "parent": "/obj/item/clothing/mask", "declared_vars": {
        "name": "breath mask", "icon_state": "breath", "body_parts_covered": 0,
        "clothing_flags": 8, "w_class": 2 } },
    "/obj/item/tank": { "parent": "/obj/item", "declared_vars": { "slot_flags": 1024,
        "volume": 70, "distribute_pressure": 101.325 } },
    "/obj/item/tank/internals": { "parent": "/obj/item/tank" },
    "/obj/item/tank/internals/emergency_oxygen": { "parent": "/obj/item/tank/internals",
        "declared_vars": { "name": "emergency oxygen tank", "icon_state": "emergency",
        "slot_flags": 512, "volume": 3, "distribute_pressure": 16, "w_class": 2 } },
    "/mob": { "parent": "/atom/movable", "declared_vars": { "layer": 4, "density": 1 } },
    "/mob/living": { "parent": "/mob" },
    "/mob/living/carbon": { "parent": "/mob/living" },
    "/mob/living/carbon/human": { "parent": "/mob/living/carbon", "declared_vars": {
        "icon": { "resource": "h.dmi" }, "icon_state": "human_basic" } },
    "/turf": { "parent": "/atom", "declared_vars": { "icon": { "resource": "t.dmi" },
        "blocks_air": 0, "can_atmos_pass": 0, "initial_gas_mix": "o2=22;n2=82;TEMP=293.15" } },
    "/turf/open": { "parent": "/turf", "declared_vars": { "can_atmos_pass": -1,
        "planetary_atmos": 0 } },
    "/turf/open/space": { "parent": "/turf/open", "declared_vars": { "icon_state": "space",
        "initial_gas_mix": "TEMP=2.7" } },
    "/turf/open/space/basic": { "parent": "/turf/open/space" },
    "/turf/closed": { "parent": "/turf", "declared_vars": { "density": 1, "opacity": 1,
        "blocks_air": 1 } },
    "/turf/closed/wall": { "parent": "/turf/closed", "declared_vars": { "icon_state": "wall" } },
    "/area": { "parent": "/atom" },
    "/area/space": { "parent": "/area" }
  }
}"##;

/// Rows top-down, the last line is y = 1: space inside walls, the spawn `A` at (2,3).
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"s" = (/turf/open/space/basic,/area/space)
"A" = (/obj/effect/landmark/start/assistant,/turf/open/space/basic,/area/space)

(1,1,1) = {"
wwwww
wsssw
wAssw
wsssw
wwwww
"}
"##;

const STATES: [(&str, &str); 4] = [
    ("t.dmi", "space"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
];

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("loopback_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
    ];
    write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)])
}

#[test]
fn join_move_and_replicate_over_the_in_memory_transport() {
    let assets = fixture_assets("join_move");
    let mut s = testing::Station::start(fixture_map(&assets, &["map.dmm"]), |app| {
        let (listener, connector) = loopback::listener();
        // The connector rides on the server app so that `testing::client_app` finds it.
        app.insert_resource(listener).insert_resource(connector);
    });
    let p = s.join("p0", |_| {});
    let mob = s.players[p].mob;
    let start = TilePos::new(2, 3, 1);
    assert_eq!(*s.get::<TilePos>(mob), start);
    let copy = s.client_copy(p, mob).expect("own mob replicated");
    assert_eq!(
        *s.players[p].app.world().get::<TilePos>(copy).unwrap(),
        start
    );

    let east = TilePos::new(3, 3, 1);
    s.run_until("the step is applied and replicated", |s| {
        s.try_step(p, Dir::East);
        let copy = s.client_copy(p, mob).unwrap();
        *s.get::<TilePos>(mob) == east
            && s.players[p].app.world().get::<TilePos>(copy) == Some(&east)
    });

    // Leaving closes the session on the server like a lost QUIC connection.
    s.players.pop();
    s.run_until("the server drops the link", |s| s.link_of("p0").is_none());
}

/// Average and p95 server tick time over a minute of an idle MetaStation round (`docs/WEB.md`).
#[test]
#[ignore = "needs generated assets and MetaStation; a timing, run in release with --ignored"]
fn metastation_tick_single_threaded() {
    // No worker threads: scoped tasks (atmos) run on the calling thread, as on wasm32. Pools are
    // global and set once per process; this test runs in its own process under nextest.
    ComputeTaskPool::get_or_init(|| TaskPoolBuilder::new().num_threads(0).build());
    AsyncComputeTaskPool::get_or_init(|| TaskPoolBuilder::new().num_threads(0).build());
    IoTaskPool::get_or_init(|| TaskPoolBuilder::new().num_threads(0).build());
    let map = MapLoadPlugin {
        source: env!("SS13_SOURCE").into(),
        maps: vec!["MetaStation".into()],
        assets: std::env::var("SS13_ASSETS")
            .unwrap_or_else(|_| "assets/generated".into())
            .into(),
    };
    let loaded = Instant::now();
    let mut s = testing::Station::start(map, |app| {
        let (listener, connector) = loopback::listener();
        // The gameplay plugins `ss13_server` adds next to the game (not the debug and admin ones).
        app.insert_resource(listener)
            .insert_resource(connector)
            .add_plugins((
                ss13_atmos::AtmosPlugin,
                ss13_tgui::server::TguiServerPlugin,
                ss13_chat::server::ChatServerPlugin,
            ));
    });
    let load_secs = loaded.elapsed().as_secs_f64();
    let mut schedules = s.server.world_mut().resource_mut::<Schedules>();
    for (_, schedule) in schedules.iter_mut() {
        schedule.set_executor(SingleThreadedExecutor::new());
    }
    s.join("p0", |_| {});
    s.run_for(5.0); // Warm-up: first views, caches.
    let mut ticks = Vec::new();
    for _ in 0..testing::steps(60.0) {
        let start = Instant::now();
        s.server.update();
        ticks.push(start.elapsed().as_secs_f64() * 1000.0);
        for p in &mut s.players {
            p.app.update();
        }
    }
    ticks.sort_by(f64::total_cmp);
    let avg = ticks.iter().sum::<f64>() / ticks.len() as f64;
    let pct = |p: f64| ticks[((ticks.len() - 1) as f64 * p).round() as usize];
    println!(
        "MetaStation idle, 1 player, single-threaded: {} ticks, avg {avg:.2} ms, p50 {:.2}, \
         p95 {:.2}, p99 {:.2}, max {:.2} ms; startup (map load) {load_secs:.1} s",
        ticks.len(),
        pct(0.5),
        pct(0.95),
        pct(0.99),
        ticks.last().unwrap(),
    );
}
