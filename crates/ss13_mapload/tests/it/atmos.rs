//! Atmos on a fixture station loaded from `.dmm` through the converter: an airlock between a
//! pressurized room and an airless one (or space), opened and closed by its density as the
//! door system does. Also the MetaStation checks (`--ignored`, needs `SS13_ASSETS`).

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use ss13_assets::manifest::AssetManifest;
use ss13_atmos::{Atmos, AtmosBlocker, AtmosPlugin, GasId, PressureDifference};
use ss13_mapload::MapLoadPlugin;
use ss13_mapload::convert::{DmTypes, convert};
use ss13_shared::TilePos;
use ss13_world::{Density, SpatialIndex, WorldPlugin};

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 }, "declared_vars": { "can_atmos_pass": 1, "flags_1": 0 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3 } },
    "/obj": { "parent": "/atom/movable" },
    "/obj/machinery": { "parent": "/obj" },
    "/obj/machinery/door": { "parent": "/obj/machinery", "declared_vars": {
        "density": 1, "opacity": 1, "can_atmos_pass": -2 } },
    "/obj/machinery/door/airlock": { "parent": "/obj/machinery/door" },
    "/obj/structure": { "parent": "/obj" },
    "/obj/structure/window": { "parent": "/obj/structure", "declared_vars": {
        "density": 1, "can_atmos_pass": -1, "flags_1": 2 } },
    "/obj/structure/window/fulltile": { "parent": "/obj/structure/window",
        "declared_vars": { "flags_1": 0 } },
    "/turf": { "parent": "/atom", "declared_vars": { "blocks_air": 0, "can_atmos_pass": 0,
        "initial_gas_mix": "o2=22;n2=82;TEMP=293.15", "temperature": 293.15 } },
    "/turf/open": { "parent": "/turf", "declared_vars": { "can_atmos_pass": -1,
        "planetary_atmos": 0 } },
    "/turf/open/floor": { "parent": "/turf/open" },
    "/turf/open/floor/airless": { "parent": "/turf/open/floor",
        "declared_vars": { "initial_gas_mix": "TEMP=2.7" } },
    "/turf/open/space": { "parent": "/turf/open", "declared_vars": { "temperature": 2.7,
        "initial_gas_mix": "TEMP=2.7" } },
    "/turf/open/space/basic": { "parent": "/turf/open/space" },
    "/turf/closed": { "parent": "/turf", "declared_vars": { "density": 1, "blocks_air": 1 } },
    "/turf/closed/wall": { "parent": "/turf/closed" },
    "/area": { "parent": "/atom" },
    "/area/space": { "parent": "/area" }
  }
}"##;

/// Rows top-down (the last line is y = 1). Room L is x 2..6, the airlock `D` (7,3), room R
/// x 8..12 is airless floor (`a`) or space (`s`). `W` (7,2) is a full-tile window, `E` (7,4) a
/// window facing east on a floor tile: both keep the rooms apart.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"a" = (/turf/open/floor/airless,/area/space)
"s" = (/turf/open/space/basic,/area/space)
"D" = (/obj/machinery/door/airlock,/turf/open/floor,/area/space)
"W" = (/obj/structure/window/fulltile,/turf/open/floor,/area/space)
"E" = (/obj/structure/window{dir = 4},/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwwwwwwwwww
wfffffwRRRRRw
wfffffERRRRRw
wfffffDRRRRRw
wfffffWRRRRRw
wwwwwwwwwwwww
"}
"##;

const DOOR: TilePos = TilePos::new(7, 3, 1);
const FIRE: Duration = Duration::from_millis(500);

fn fixture(test: &str, right: char) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("atmos_fixture")
        .join(test);
    std::fs::create_dir_all(dir.join("prototypes")).unwrap();
    let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
    let used = [
        "/turf/closed/wall",
        "/turf/open/floor",
        "/turf/open/floor/airless",
        "/turf/open/space/basic",
        "/obj/machinery/door/airlock",
        "/obj/structure/window",
        "/obj/structure/window/fulltile",
        "/area/space",
    ];
    let conversion = convert(&types, used).unwrap();
    std::fs::write(dir.join("prototypes/dm_types.ron"), conversion.ron).unwrap();
    let manifest = AssetManifest {
        atlases: vec![],
        states: vec![],
    };
    std::fs::write(
        dir.join(ss13_assets::import::MANIFEST_FILE),
        postcard::to_stdvec(&manifest).unwrap(),
    )
    .unwrap();
    std::fs::write(dir.join("map.dmm"), MAP.replace('R', &right.to_string())).unwrap();
    dir
}

fn app(map: MapLoadPlugin) -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, WorldPlugin))
        .insert_resource(TimeUpdateStrategy::ManualDuration(FIRE))
        .add_plugins((
            ss13_proto::ProtoPlugin {
                dir: map.prototypes_dir(),
            },
            map,
            AtmosPlugin,
        ));
    app.finish();
    app.cleanup();
    // Startup, plus the first frame (zero delta: no fire yet).
    app.update();
    app
}

fn fixture_app(test: &str, right: char) -> App {
    let dir = fixture(test, right);
    app(MapLoadPlugin {
        source: dir.clone(),
        maps: vec![dir.join("map.dmm").to_string_lossy().into_owned()],
        assets: dir,
    })
}

fn door(app: &mut App) -> Entity {
    let index = app.world().resource::<SpatialIndex>();
    let on_tile = index.on_tile(DOOR).to_vec();
    on_tile
        .into_iter()
        .find(|&e| app.world().get::<AtmosBlocker>(e).is_some())
        .expect("the airlock")
}

fn moles(app: &App, xs: std::ops::RangeInclusive<i32>) -> f64 {
    let atmos = app.world().resource::<Atmos>();
    xs.flat_map(|x| (2..=5).map(move |y| TilePos::new(x, y, 1)))
        .filter_map(|p| atmos.air(p).map(|a| a.total_moles()))
        .sum()
}

fn pressure(app: &App, x: i32) -> f64 {
    app.world()
        .resource::<Atmos>()
        .pressure(TilePos::new(x, 3, 1))
        .unwrap()
}

#[test]
fn airlock_between_rooms_equalizes_and_closing_stops_it() {
    let mut app = fixture_app("rooms", 'a');
    let atmos = app.world().resource::<Atmos>();
    assert_eq!(atmos.active_turfs(), 0, "a closed station is stable");
    assert!((atmos.pressure(TilePos::new(2, 3, 1)).unwrap() - 101.34).abs() < 0.01);
    assert_eq!(atmos.pressure(TilePos::new(10, 3, 1)), Some(0.0));
    assert_eq!(atmos.air(TilePos::new(1, 1, 1)), None, "walls have no air");
    // The east-facing window tile joins room L, the full-tile window tile neither room.
    let level = atmos.level(1).unwrap();
    let e = level.tile(7, 4).unwrap();
    assert!(
        level.adjacent(e, ss13_shared::Dir::West) && !level.adjacent(e, ss13_shared::Dir::East)
    );
    let w = level.tile(7, 2).unwrap();
    assert!(
        ss13_shared::Dir::CARDINALS
            .iter()
            .all(|&d| !level.adjacent(w, d))
    );

    let door = door(&mut app);
    let all = |app: &App| moles(app, 2..=12);
    let start = all(&app);
    let before = (moles(&app, 2..=6), moles(&app, 8..=12));
    app.world_mut().entity_mut(door).remove::<Density>();
    for _ in 0..3 {
        app.update();
    }
    let moved = (moles(&app, 2..=6), moles(&app, 8..=12));
    assert!(
        moved.1 > 1.0,
        "gas reached room R: {moved:?} from {before:?}"
    );
    // Closed again: nothing crosses any more.
    app.world_mut().entity_mut(door).insert(Density);
    app.update();
    let closed = (moles(&app, 2..=6), moles(&app, 8..=12));
    for _ in 0..30 {
        app.update();
    }
    let later = (moles(&app, 2..=6), moles(&app, 8..=12));
    assert!(
        (later.0 - closed.0).abs() < 1e-6 && (later.1 - closed.1).abs() < 1e-6,
        "{closed:?} -> {later:?}"
    );

    app.world_mut().entity_mut(door).remove::<Density>();
    let mut fires = 0;
    while (pressure(&app, 2) - pressure(&app, 12)).abs() > 1.0 {
        app.update();
        fires += 1;
        assert!(
            fires < 400,
            "equalized: {} vs {}",
            pressure(&app, 2),
            pressure(&app, 12)
        );
    }
    let total = all(&app);
    println!(
        "rooms equalized within 1 kPa in {fires} fires ({:.1} s): L {:.2} kPa, R {:.2} kPa, \
         {:.2} of {:.2} mol",
        fires as f64 * 0.5,
        pressure(&app, 2),
        pressure(&app, 12),
        total,
        start
    );
    assert!(
        (total - start).abs() < 0.01,
        "moles are kept: {total} vs {start}"
    );
    let atmos = app.world().resource::<Atmos>();
    assert!(atmos.active_turfs() > 0);
    for _ in 0..60 {
        app.update();
    }
    let atmos = app.world().resource::<Atmos>();
    assert_eq!(atmos.active_turfs(), 0, "equal rooms fall asleep");
    assert_eq!(atmos.excited_groups(), 0);
}

#[derive(Resource, Default)]
struct Deltas(Vec<PressureDifference>);

fn record(mut reader: MessageReader<PressureDifference>, mut log: ResMut<Deltas>) {
    log.0.extend(reader.read().copied());
}

#[test]
fn airlock_to_space_vents_the_room() {
    let mut app = fixture_app("space", 's');
    app.init_resource::<Deltas>().add_systems(Last, record);
    assert_eq!(app.world().resource::<Atmos>().active_turfs(), 0);
    let start = moles(&app, 2..=6);
    let door = door(&mut app);
    app.world_mut().entity_mut(door).remove::<Density>();
    let mut fires = 0;
    // LINDA vents a room through one tile slowly, and stalls where neighbours differ by less
    // than MINIMUM_MOLES_DELTA_TO_MOVE: measure to 10% at the far wall.
    while pressure(&app, 2) > 10.134 {
        app.update();
        fires += 1;
        assert!(fires < 400, "vented: {} kPa left", pressure(&app, 2));
    }
    let atmos = app.world().resource::<Atmos>();
    let t = atmos.temperature(TilePos::new(2, 3, 1)).unwrap();
    println!(
        "room vented to 10% at the far wall in {fires} fires ({:.1} s): {:.1} of {start:.1} mol \
         left, {t:.1} K",
        fires as f64 * 0.5,
        moles(&app, 2..=6)
    );
    let deltas = &app.world().resource::<Deltas>().0;
    assert!(
        deltas.iter().any(|d| d.pos == DOOR && d.difference > 10.0),
        "spacewind at the door"
    );
    // Expanding into the 7000 J/K "cold vacuum" chills what is left (tg's deliberate hack).
    assert!(t < 200.0 && atmos.moles(TilePos::new(2, 3, 1), GasId::O2).unwrap() < 22.0);
}

/// `SS13_ASSETS=... cargo test -p ss13_mapload --test it atmos:: -- --ignored --nocapture`
#[test]
#[ignore = "needs generated assets in SS13_ASSETS"]
fn metastation_atmos() {
    let assets = PathBuf::from(std::env::var("SS13_ASSETS").expect("SS13_ASSETS"));
    let source =
        PathBuf::from(std::env::var("SS13_SOURCE").unwrap_or_else(|_| env!("SS13_SOURCE").into()));
    let started = Instant::now();
    let mut app = app(MapLoadPlugin {
        source,
        maps: vec!["MetaStation".into()],
        assets,
    });
    println!("loaded with atmos in {:.2?}", started.elapsed());
    let atmos = app.world().resource::<Atmos>();
    let level = atmos.level(1).expect("station level");
    println!(
        "setup {:?}, tiles (blocked, open, space) {:?}",
        atmos.setup,
        level.kind_counts()
    );
    println!(
        "cells {}, active after setup {}, groups {}",
        level.cell_count(),
        atmos.active_turfs(),
        atmos.excited_groups()
    );
    let mut worst = 0;
    let idle = Instant::now();
    for i in 1..=20 {
        app.update();
        let active = app.world().resource::<Atmos>().active_turfs();
        worst = worst.max(active);
        if i % 5 == 0 {
            println!("fire {i}: active {active}");
        }
    }
    let idle = idle.elapsed() / 20;
    let atmos = app.world().resource::<Atmos>();
    println!(
        "idle: {:.3?} per frame (fire {:.3?}), worst active {worst}",
        idle, atmos.last_fire
    );
    assert!(
        worst < 50,
        "station stable at round start: {worst} active turfs"
    );

    // Breach: the first full-tile window (by position) with space on one side and station air
    // on the other is removed, as if smashed.
    let (window, pos) = {
        let world = app.world_mut();
        let mut q = world.query::<(Entity, &AtmosBlocker, &TilePos, Option<&ss13_world::Border>)>();
        let mut found: Vec<(Entity, TilePos)> = q
            .iter(world)
            .filter(|(_, b, _, border)| b.0 == ss13_atmos::AtmosPass::Proc && border.is_none())
            .map(|(e, _, &p, _)| (e, p))
            .collect();
        found.sort_by_key(|(_, p)| (p.y, p.x));
        let atmos = world.resource::<Atmos>();
        let level = atmos.level(1).unwrap();
        let kind = |x: i32, y: i32| level.tile(x, y).map(|t| level.kind(t));
        let pick = found.into_iter().find(|(_, p)| {
            ss13_shared::Dir::CARDINALS.iter().any(|d| {
                let (dx, dy) = d.offset();
                let inside = level.tile(p.x - dx, p.y - dy);
                kind(p.x + dx, p.y + dy) == Some(ss13_atmos::TileKind::Space)
                    && atmos
                        .pressure(TilePos::new(p.x - dx, p.y - dy, 1))
                        .is_some_and(|p| (95.0..110.0).contains(&p))
                    && inside.is_some_and(|t| {
                        ss13_shared::Dir::CARDINALS
                            .iter()
                            .filter(|&&d| level.adjacent(t, d))
                            .count()
                            >= 3
                    })
            })
        });
        pick.expect("a window between space and the station")
    };
    // Every blocker on the tile: the window spawners also leave their map-editor frame (a
    // second full-tile window, `MAP_SWITCH` resolved to its map branch by the extractor).
    let _ = window;
    let blockers: Vec<Entity> = app
        .world()
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .filter(|&e| app.world().get::<AtmosBlocker>(e).is_some())
        .collect();
    for e in blockers {
        app.world_mut().despawn(e);
    }
    let station = |app: &App| {
        let level = app.world().resource::<Atmos>().level(1).unwrap();
        let (w, h) = level.size();
        (0..w * h)
            .filter_map(|t| level.air(t))
            .map(|a| a.total_moles())
            .sum::<f64>()
    };
    let before = station(&app);
    let mut fire_times = Vec::new();
    let mut peak_active = 0;
    for i in 1..=120 {
        app.update();
        let atmos = app.world().resource::<Atmos>();
        fire_times.push(atmos.last_fire);
        peak_active = peak_active.max(atmos.active_turfs());
        if i % 20 == 0 {
            println!(
                "breach at {pos:?}, fire {i}: active {}, groups {}, station lost {:.0} mol, \
                 fire {:.3?}",
                atmos.active_turfs(),
                atmos.excited_groups(),
                before - station(&app),
                atmos.last_fire
            );
        }
    }
    fire_times.sort();
    let mean = fire_times.iter().sum::<Duration>() / fire_times.len() as u32;
    println!(
        "breach: peak active {peak_active}, fire mean {mean:.3?}, p99 {:.3?}, max {:.3?} \
         (~{:.0} fires/s)",
        fire_times[fire_times.len() * 99 / 100],
        fire_times[fire_times.len() - 1],
        1.0 / mean.as_secs_f64()
    );
    assert!(peak_active > 0 && before - station(&app) > 100.0);
}
