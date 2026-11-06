//! The mining level: MetaStation with Lavaland (`ss13_mapload::MiningLevel`), its planetary air
//! and caves, the mining and labour shuttles flying there, and the escape pods' random docks.
//!
//! `cargo test -p ss13_game --all-features --test it lavaland -- --ignored --nocapture` (needs
//! generated assets in `SS13_ASSETS`).

use bevy::prelude::*;
use ss13_atmos::{Atmos, AtmosPlugin};
use ss13_game::escape_pods::{EscapePods, place_random_dock};
use ss13_game::shuttle::{MobileDock, StationaryDock};
use ss13_game::shuttle_nav::{Flight, NavShuttle, get_shuttle, request};
use ss13_game::testing::server_app;
use ss13_mapload::{DmMineral, LAVALAND_MAP, MapLoadPlugin, MiningLevel, mining_z};
use ss13_shared::TilePos;

fn metastation_map() -> MapLoadPlugin {
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

fn dock(world: &mut World, id: &str) -> Entity {
    let mut q = world.query::<(Entity, &StationaryDock)>();
    q.iter(world)
        .find(|(_, d)| d.id == id)
        .map(|(e, _)| e)
        .unwrap_or_else(|| panic!("no dock {id}"))
}

fn docked_at(world: &mut World, shuttle: &str) -> String {
    let port = get_shuttle(world, shuttle).unwrap_or_else(|| panic!("no {shuttle} shuttle"));
    let at = world.get::<MobileDock>(port).unwrap().docked.unwrap();
    world.get::<StationaryDock>(at).unwrap().id.clone()
}

#[test]
#[ignore = "needs generated assets, MetaStation and Lavaland; run with --ignored"]
fn metastation_with_lavaland() {
    let map = metastation_map();
    let lavaland = map.source.join(LAVALAND_MAP);
    let mut app = server_app(map, |app| {
        app.insert_resource(MiningLevel(lavaland))
            .add_plugins(AtmosPlugin);
    });
    let world = app.world_mut();
    let z = mining_z(world).expect("the mining level loaded");
    assert!(
        !ss13_game::events::controller::station_levels(world).contains(&z),
        "lavaland is no station level"
    );
    // Caves: volcanic rock where the map had genturf placeholders.
    let mut minerals = world.query_filtered::<&TilePos, With<DmMineral>>();
    let rock = minerals.iter(world).filter(|p| p.z == z).count();
    println!("lavaland z{z}: {rock} mineral turfs");
    assert!(rock > 5000, "{rock} mineral turfs");

    // The mining and labour shuttles start at the station; their lavaland docks exist.
    assert_eq!(docked_at(world, "mining"), "mining_home");
    assert_eq!(docked_at(world, "mining_common"), "commonmining_home");
    assert_eq!(docked_at(world, "laborcamp"), "laborcamp_home");
    let away = dock(world, "mining_away");
    let away_pos = *world.get::<TilePos>(away).unwrap();
    assert_eq!(away_pos.z, z);
    // The outdoor dock has lavaland's planetary air: thin, never station pressure.
    let air = world.resource::<Atmos>().air(away_pos).expect("air");
    let kpa = air.pressure();
    assert!((25.0..55.0).contains(&kpa), "{kpa} kPa at mining_away");

    // The mining shuttle flies there: ignition, transit, docking.
    let port = get_shuttle(world, "mining").unwrap();
    assert!(request(world, port, away));
    for _ in 0..(20.0 / ss13_game::testing::TICK.as_secs_f64()) as usize {
        app.update();
        if app.world().get::<NavShuttle>(port).unwrap().flight == Flight::Idle {
            break;
        }
    }
    let world = app.world_mut();
    assert_eq!(docked_at(world, "mining"), "mining_away");
    assert_eq!(world.get::<TilePos>(port).unwrap().z, z);

    // An escape pod's random dock lands on the lavaland wastes.
    let pod = world.resource::<EscapePods>().0[0].port;
    let random = dock(world, "pod_lavaland");
    assert!(place_random_dock(world, pod, random));
    let pos = *world.get::<TilePos>(random).unwrap();
    assert_eq!(pos.z, z);
    assert!(
        pos.x >= 16 && pos.y >= 16 && pos.x <= 240 && pos.y <= 240,
        "{pos:?}"
    );
}
