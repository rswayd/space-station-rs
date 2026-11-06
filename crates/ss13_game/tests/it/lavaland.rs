//! The mining level: MetaStation with Lavaland (`ss13_mapload::MiningLevel`), its planetary air
//! and caves, the mining and labour shuttles flying there, and the escape pods' random docks;
//! its environment: lava rivers and ruins, lava burning what is in it, ash storms.
//!
//! `cargo test -p ss13_game --all-features --test it lavaland -- --ignored --nocapture` (needs
//! generated assets in `SS13_ASSETS`).

use bevy::prelude::*;
use ss13_atmos::{Atmos, AtmosPlugin};
use ss13_game::escape_pods::{EscapePods, place_random_dock};
use ss13_game::fire::FireStacks;
use ss13_game::health::Health;
use ss13_game::shuttle::{MobileDock, StationaryDock};
use ss13_game::shuttle_nav::{Flight, NavShuttle, get_shuttle, request};
use ss13_game::testing::server_app;
use ss13_game::weather::{ActiveWeather, WeatherKind};
use ss13_mapload::{
    Areas, DmArea, DmMineral, DmPower, DmTurfBuild, LAVALAND_MAP, MapLoadPlugin, MiningLevel, Turf,
    mining_z,
};
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

const CORGI: &str = "/mob/living/basic/pet/dog/corgi";

fn run(app: &mut App, secs: f64) {
    for _ in 0..ss13_game::testing::steps(secs) {
        app.update();
    }
}

fn turf_path(world: &World, pos: TilePos) -> Option<String> {
    let index = world.resource::<ss13_world::SpatialIndex>();
    index
        .on_tile(pos)
        .iter()
        .find_map(|&e| {
            world
                .get::<DmTurfBuild>(e)
                .filter(|_| world.get::<Turf>(e).is_some())
        })
        .map(|t| t.path.clone())
}

fn free(world: &World, pos: TilePos) -> bool {
    world
        .resource::<ss13_world::SpatialIndex>()
        .dense_on_tile(pos)
        .is_empty()
}

fn outdoors(world: &World, pos: TilePos) -> Option<bool> {
    let area = world.resource::<Areas>().area_at(pos)?;
    Some(world.get::<DmArea>(area).is_some_and(|a| a.outdoors))
}

#[test]
#[ignore = "needs generated assets (with the lava ruins), MetaStation and Lavaland; run with --ignored"]
fn lavaland_environment() {
    let map = metastation_map();
    let lavaland = map.source.join(LAVALAND_MAP);
    let mut app = server_app(map, |app| {
        app.insert_resource(MiningLevel(lavaland))
            .add_plugins(AtmosPlugin);
    });
    app.update();
    let world = app.world_mut();
    let z = mining_z(world).expect("the mining level loaded");
    assert!(
        world
            .resource::<ss13_game::lava::LavaLevels>()
            .0
            .contains(&z),
        "lavaland has lava"
    );

    // Rivers: lava well beyond the few mapped tiles.
    let mut turfs = world.query_filtered::<(&TilePos, &DmTurfBuild), With<Turf>>();
    let tiles: Vec<(TilePos, String)> = turfs
        .iter(world)
        .filter(|(p, _)| p.z == z)
        .map(|(p, t)| (*p, t.path.clone()))
        .collect();
    let lava: Vec<TilePos> = tiles
        .iter()
        .filter(|(_, t)| t.starts_with("/turf/open/lava/"))
        .map(|(p, _)| *p)
        .collect();
    println!("lavaland z{z}: {} lava tiles", lava.len());
    assert!(lava.len() > 200, "{} lava tiles", lava.len());

    // Ruins: some ruin areas on the level.
    let ruin_tiles = tiles
        .iter()
        .filter(|(p, _)| {
            let areas = world.resource::<Areas>();
            areas.area_at(*p).is_some_and(|a| {
                world
                    .get::<DmPower>(a)
                    .is_some_and(|d| d.path.starts_with("/area/ruin"))
            })
        })
        .count();
    println!("lavaland z{z}: {ruin_tiles} ruin tiles");
    assert!(ruin_tiles > 0, "no ruins placed");

    // A corgi and an ID card in lava: the corgi burns, the card burns away.
    let spot = *lava
        .iter()
        .find(|&&p| free(world, p) && ss13_game::lava::burning_lava_at(world, p))
        .expect("an open lava tile");
    let dog = ss13_game::mobs::spawn::spawn_mob(world, CORGI, spot).expect("a corgi");
    let card = ss13_game::construction::spawn_obj(world, "/obj/item/card/id/advanced", spot)
        .expect("an id card");
    let full = app.world().get::<Health>(dog).unwrap().health();
    run(&mut app, 1.5);
    let world = app.world();
    assert!(world.get::<Health>(dog).unwrap().health() < full - 15.0);
    assert!(world.get::<FireStacks>(dog).is_some_and(|f| f.on_fire));
    run(&mut app, 60.0);
    assert!(
        app.world().get_entity(card).is_err(),
        "the card burned away"
    );

    // An ash storm: after its 30 s warning it burns a corgi outdoors, not one in the base.
    let world = app.world_mut();
    // Outdoors on basalt; indoors on a floor of the (pressurised) mining base.
    let spot = |world: &World, outside: bool| {
        tiles
            .iter()
            .filter(|(_, t)| {
                t.starts_with(if outside {
                    "/turf/open/misc/asteroid/basalt"
                } else {
                    "/turf/open/floor/iron"
                })
            })
            .map(|(p, _)| *p)
            .find(|&p| {
                let area = world.resource::<Areas>().area_at(p);
                outdoors(world, p) == Some(outside)
                    && (outside
                        || area
                            .and_then(|a| world.get::<DmPower>(a))
                            .is_some_and(|d| d.path.starts_with("/area/mine/")))
                    && free(world, p)
                    && turf_path(world, p).is_some()
            })
    };
    let out = spot(&*world, true).expect("an outdoor tile");
    let inside = spot(&*world, false).expect("a tile in the mining base");
    let exposed = ss13_game::mobs::spawn::spawn_mob(world, CORGI, out).unwrap();
    let sheltered = ss13_game::mobs::spawn::spawn_mob(world, CORGI, inside).unwrap();
    ss13_game::weather::run_weather(world, WeatherKind::AshStorm, None);
    // (The scheduler may have started one of its own at round start.)
    let started = world.resource::<ActiveWeather>().0.last().unwrap();
    assert_eq!(started.kind, WeatherKind::AshStorm);
    assert_eq!(started.levels, vec![z]);
    // A corgi has 20 health and the ash does 4 a second: measure from the spawn, since a storm
    // the scheduler started at round start may already be burning.
    let hp = |app: &App, e| app.world().get::<Health>(e).unwrap().health();
    let (a, b) = (hp(&app, exposed), hp(&app, sheltered));
    run(&mut app, 35.0);
    assert!(hp(&app, exposed) < a - 10.0, "the ash burns outdoors");
    assert_eq!(hp(&app, sheltered), b, "the base shelters");
}
