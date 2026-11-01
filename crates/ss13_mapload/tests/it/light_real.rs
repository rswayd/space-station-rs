//! MetaStation lighting data at round start: what the server replicates for lighting.
//!
//! Needs `SS13_ASSETS` like `real_maps.rs`. `--ignored --nocapture` prints the numbers.

use std::collections::BTreeSet;
use std::path::PathBuf;

use bevy::prelude::*;
use ss13_assets::dmm::DmmMap;
use ss13_mapload::convert::{DmTypes, convert};
use ss13_mapload::{
    DEFAULT_AREA, DEFAULT_TURF, DmProtoPlugin, MapLoadPlugin, Turf, load_maps, read_manifest,
};
use ss13_proto::{ProtoComponents, Prototypes};
use ss13_shared::{Optics, TilePos};
use ss13_world::WorldPlugin;

#[test]
#[ignore = "needs the BYOND source checkout and generated assets (SS13_ASSETS)"]
fn metastation_lights() {
    let config = MapLoadPlugin {
        source: std::env::var_os("SS13_SOURCE")
            .map_or_else(|| env!("SS13_SOURCE").into(), PathBuf::from),
        maps: vec!["MetaStation".into()],
        assets: std::env::var_os("SS13_ASSETS")
            .map(PathBuf::from)
            .expect("SS13_ASSETS"),
    };
    let types = DmTypes::read(&config.assets.join("dm_types.json")).unwrap();
    let manifest = read_manifest(&config.assets).unwrap();
    let mut used = BTreeSet::from([DEFAULT_TURF.to_owned(), DEFAULT_AREA.to_owned()]);
    for path in config.map_paths() {
        for prefab in &DmmMap::from_file(&path).unwrap().prefabs {
            used.extend(prefab.objects.iter().map(|o| o.path.clone()));
        }
    }
    let conversion = convert(&types, used.iter().map(String::as_str)).unwrap();
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, WorldPlugin, DmProtoPlugin));
    let world = app.world();
    let protos = Prototypes::from_sources(
        &[(PathBuf::from("dm_types.ron"), conversion.ron)],
        world.resource::<ProtoComponents>(),
        world.resource::<AppTypeRegistry>(),
    )
    .unwrap();
    app.insert_resource(protos);
    load_maps(app.world_mut(), &config.map_paths(), &manifest).unwrap();

    let world = app.world_mut();
    let mut q = world.query::<(&Optics, &TilePos, Has<Turf>, Option<&ss13_mapload::DmName>)>();
    let (mut lights, mut starlight, mut opaque, mut tiles, mut unlit, mut bytes) =
        (0, 0, 0, 0, 0, 0);
    let mut by_name = std::collections::BTreeMap::<String, usize>::new();
    for (optics, _, turf, name) in q.iter(world) {
        opaque += usize::from(optics.opaque);
        if let Some(tile) = optics.tile {
            tiles += 1;
            unlit += usize::from(!tile.dynamic() && tile.ambient()[3] == 0);
        }
        if optics.light.is_some() {
            if turf {
                starlight += 1;
            } else {
                lights += 1;
                let name = name.map_or("?".into(), |n| n.name.clone());
                *by_name.entry(name).or_default() += 1;
            }
        }
        bytes += postcard::to_stdvec(optics).unwrap().len();
    }
    let mut top: Vec<_> = by_name.into_iter().collect();
    top.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    println!(
        "MetaStation: {lights} light sources + {starlight} starlight tiles, {opaque} opaque, \
         {tiles} tiles with area lighting ({unlit} pitch black); Optics total {bytes} bytes"
    );
    println!("top sources: {:?}", &top[..top.len().min(15)]);
    // Reliable bytes of a join (everything in a 21x21 view box) laid out as `ss13_net` sends
    // them: `Spawn { pos, entities: [Spawned { id, facing, appearance id }] }` per tile, and each
    // distinct interned value once; with lighting the value is (appearance, optics).
    let mut q = world.query::<(
        &TilePos,
        Option<&ss13_shared::Facing>,
        Option<&ss13_shared::Appearance>,
        Option<&Optics>,
    )>();
    for center in [TilePos::new(108, 66, 1), TilePos::new(40, 122, 1)] {
        let mut tiles = std::collections::BTreeMap::<(i32, i32), Vec<_>>::new();
        for (pos, facing, appearance, optics) in q.iter(world) {
            if appearance.is_none() || pos.chebyshev(center).is_none_or(|d| d > 10) {
                continue;
            }
            tiles
                .entry((pos.x, pos.y))
                .or_default()
                .push((*pos, facing, appearance, optics));
        }
        let (mut spawns, mut n, mut with_optics) = (0, 0, 0);
        let (mut plain, mut lit) = (BTreeSet::new(), BTreeSet::new());
        for list in tiles.values() {
            spawns += 1 + postcard::to_stdvec(&(list[0].0, list.len() as u32))
                .unwrap()
                .len();
            for &(_, facing, appearance, optics) in list {
                n += 1;
                with_optics += usize::from(optics.is_some());
                let value = postcard::to_stdvec(appearance.unwrap()).unwrap();
                plain.insert(value.clone());
                lit.insert((value, postcard::to_stdvec(&optics).unwrap()));
                // Ids: 2 varint bytes each for a per-client NetId and an AppearanceId.
                spawns += postcard::to_stdvec(&(300u16, facing.map(|f| f.0), Some(300u16)))
                    .unwrap()
                    .len();
            }
        }
        let id = 2;
        let base = spawns + plain.iter().map(|v| id + v.len()).sum::<usize>();
        let with = spawns
            + lit
                .iter()
                .map(|(v, o)| id + v.len() + o.len())
                .sum::<usize>();
        println!(
            "view at {center:?}: {n} entities ({with_optics} with optics), join {base} B; \
             with lighting {with} B (+{} B, +{:.1}%); {} -> {} interned values",
            with - base,
            100.0 * (with - base) as f64 / base as f64,
            plain.len(),
            lit.len()
        );
    }
    assert!(lights > 500, "fixtures, lamps and consoles");
    assert!(starlight > 500, "space along the hull");
    assert!(opaque > 3000, "walls");
}
