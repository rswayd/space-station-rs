//! Acceptance: MetaStation and tramstation load fully from the source checkout.
//!
//! Needs `SS13_ASSETS` with `manifest.postcard` (`ss13-import dmi --out <dir>`) and
//! `dm_types.json` (`ss13-dm-extract --out <dir>/dm_types.json`); `SS13_SOURCE` defaults to
//! `../BandaStation`. Run with `--nocapture` to see the numbers.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::Instant;

use bevy::prelude::*;
use ss13_assets::dmm::DmmMap;
use ss13_mapload::convert::{DmTypes, convert};
use ss13_mapload::spawner::dmm_loot_paths;
use ss13_mapload::{
    Areas, DEFAULT_AREA, DEFAULT_TURF, DmProtoPlugin, MapLoadPlugin, Turf, load_maps, read_manifest,
};
use ss13_proto::{ProtoComponents, Prototypes};
use ss13_shared::TilePos;
use ss13_world::{SpatialIndex, WorldPlugin};

#[test]
#[ignore = "needs the BYOND source checkout and generated assets (SS13_ASSETS)"]
fn metastation_and_tramstation_load_fully() {
    let config = MapLoadPlugin {
        source: std::env::var_os("SS13_SOURCE")
            .map_or_else(|| env!("SS13_SOURCE").into(), PathBuf::from),
        maps: vec!["MetaStation".into(), "tramstation".into()],
        assets: std::env::var_os("SS13_ASSETS")
            .map(PathBuf::from)
            .expect("SS13_ASSETS"),
    };
    let start = Instant::now();
    let types = DmTypes::read(&config.assets.join("dm_types.json")).unwrap();
    let manifest = read_manifest(&config.assets).unwrap();
    let mut used = BTreeSet::from([DEFAULT_TURF.to_owned(), DEFAULT_AREA.to_owned()]);
    for path in config.map_paths() {
        let map = DmmMap::from_file(&path).unwrap();
        for prefab in &map.prefabs {
            used.extend(prefab.objects.iter().map(|o| o.path.clone()));
            used.extend(prefab.objects.iter().flat_map(dmm_loot_paths));
        }
    }
    // Fails on any map type that is not a DM type under /atom: zero unresolved types.
    let conversion = convert(&types, used.iter().map(String::as_str)).unwrap();
    println!(
        "{} used types -> {} prototypes, unsupported declared values {:?} ({:.2?})",
        used.len(),
        conversion.prototypes,
        conversion.unsupported,
        start.elapsed()
    );

    for map in config.map_paths() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, WorldPlugin, DmProtoPlugin));
        let started = Instant::now();
        let world = app.world();
        let protos = Prototypes::from_sources(
            &[(PathBuf::from("dm_types.ron"), conversion.ron.clone())],
            world.resource::<ProtoComponents>(),
            world.resource::<AppTypeRegistry>(),
        )
        .unwrap_or_else(|e| panic!("{e}"));
        let proto_time = started.elapsed();
        app.insert_resource(protos);

        // Errors on any type without a prototype.
        let report = load_maps(app.world_mut(), std::slice::from_ref(&map), &manifest).unwrap();
        println!("--- prototypes parsed in {proto_time:.2?}\n{report}");
        for (key, n) in &report.missing_icons {
            println!("  missing {key}: {n}");
        }

        let world = app.world();
        let index = world.resource::<SpatialIndex>();
        let areas = world.resource::<Areas>();
        let (sx, sy, sz) = report.maps[0].size;
        assert_eq!(
            sz,
            if map.ends_with("tramstation.dmm") {
                2
            } else {
                1
            }
        );
        for z in 1..=sz as u16 {
            assert_eq!(index.level_size(z), Some((sx, sy)));
            for y in 1..=sy as i32 {
                for x in 1..=sx as i32 {
                    let pos = TilePos::new(x, y, z);
                    let turfs = index
                        .on_tile(pos)
                        .iter()
                        .filter(|&&e| world.entity(e).contains::<Turf>())
                        .count();
                    assert_eq!(turfs, 1, "{pos:?}");
                    assert!(areas.area_at(pos).is_some(), "{pos:?}");
                }
            }
        }
        let turfs = report.counts[&ss13_mapload::Category::Turf];
        assert_eq!(turfs, (sx * sy * sz) as usize);
    }
}

/// Every techweb design (all build types) and every job outfit item has a drawable icon: no "?"
/// in the R&D console and no GAGS "ERROR" placeholder on the HUD.
#[test]
#[ignore = "needs the BYOND source checkout and generated assets (SS13_ASSETS)"]
fn design_and_outfit_icons_resolve() {
    use ss13_mapload::{IconResolution, Template};
    let assets = std::env::var_os("SS13_ASSETS")
        .map(PathBuf::from)
        .expect("SS13_ASSETS");
    let types = DmTypes::read(&assets.join("dm_types.json")).unwrap();
    let manifest = read_manifest(&assets).unwrap();
    let research = ss13_mapload::research::resolve(&types);
    let jobs = ss13_mapload::jobs::resolve(&types);
    let outfit: BTreeSet<&str> = jobs
        .outfit_items()
        .into_iter()
        .filter(|p| types.types.contains_key(*p))
        .collect();
    let designs = research.prototype_types();
    let conversion = convert(&types, designs.iter().chain(&outfit).copied()).unwrap();
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, WorldPlugin, DmProtoPlugin));
    let world = app.world();
    let protos = Prototypes::from_sources(
        &[(PathBuf::from("dm_types.ron"), conversion.ron)],
        world.resource::<ProtoComponents>(),
        world.resource::<AppTypeRegistry>(),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let index = manifest.index();
    let resolve = |path: &str| Template::new(protos.get(path).unwrap().clone(), &index).icon;
    let undrawn: Vec<&str> = designs
        .iter()
        .copied()
        .filter(|p| resolve(p) == IconResolution::Missing)
        .collect();
    assert!(undrawn.is_empty(), "designs without an icon: {undrawn:?}");
    let placeholders: Vec<&str> = designs
        .iter()
        .chain(&outfit)
        .copied()
        .filter(|p| {
            let icon = protos.get(p).unwrap().get::<ss13_mapload::DmAtom>();
            icon.is_some_and(|a| a.icon.starts_with("icons/map_icons/"))
                && resolve(p) == IconResolution::DefaultState
        })
        .collect();
    println!(
        "{} designs, {} outfit items; GAGS placeholders: {placeholders:?}",
        designs.len(),
        outfit.len()
    );
    assert!(
        placeholders.is_empty(),
        "map icon placeholders: {placeholders:?}"
    );
}
