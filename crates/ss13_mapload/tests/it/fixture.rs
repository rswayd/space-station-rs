//! Conversion and loading on a tiny hand-written DM type dump, manifest and map.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_assets::manifest::{AssetManifest, IconStateInfo};
use ss13_mapload::convert::{DmTypes, convert};
use ss13_mapload::{
    Areas, Category, DmAtom, DmName, DmProtoPlugin, MapLoadPlugin, MapLoadReport, Opaque, Turf,
    fold_color, load_maps,
};
use ss13_proto::{ProtoComponents, Prototypes};
use ss13_shared::{Appearance, Dir, Facing, IconStateId, TilePos};
use ss13_world::{Density, SpatialIndex, WorldPlugin};

const TYPES: &str = r##"{
  "schema": 3,
  "vars": [],
  "types": {
    "/datum": { "parent": null, "vars": {}, "declared_vars": {} },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 }, "declared_vars": { "unrelated": 5 } },
    "/atom/movable": { "parent": "/atom", "vars": { "name": null }, "declared_vars": { "layer": 3 } },
    "/obj": { "parent": "/atom/movable", "vars": { "name": null }, "declared_vars": {} },
    "/obj/a": { "parent": "/obj", "vars": { "name": "thing A" }, "declared_vars": {
        "name": "thing A", "icon": { "resource": "i/a.dmi" }, "icon_state": "a",
        "color": "#ff000080", "alpha": 128, "req_access": { "list": [] } } },
    "/obj/a/b": { "parent": "/obj/a", "vars": { "name": "thing A" }, "declared_vars": {
        "dir": 4, "density": 1, "desc": "bee" } },
    "/obj/a/b/c": { "parent": "/obj/a/b", "vars": { "name": "thing A" }, "declared_vars": {
        "icon_state": "nope", "color": { "list": [1, 0, 0] } } },
    "/obj/m": { "parent": "/obj", "vars": { "name": null }, "declared_vars": {
        "icon": { "resource": "i/none.dmi" } } },
    "/obj/unused": { "parent": "/obj", "vars": { "name": null }, "declared_vars": {} },
    "/turf": { "parent": "/atom", "vars": { "name": null }, "declared_vars": {
        "icon": { "resource": "i/t.dmi" } } },
    "/turf/x": { "parent": "/turf", "vars": { "name": null }, "declared_vars": {
        "icon_state": "t", "opacity": 1, "density": 1 } },
    "/turf/open": { "parent": "/turf", "vars": { "name": null }, "declared_vars": {} },
    "/turf/open/space": { "parent": "/turf/open", "vars": { "name": null }, "declared_vars": {} },
    "/turf/open/space/basic": { "parent": "/turf/open/space", "vars": { "name": null },
        "declared_vars": { "icon": { "resource": "i/s.dmi" } } },
    "/area": { "parent": "/atom", "vars": { "name": "Space" }, "declared_vars": { "name": "Space" } },
    "/area/space": { "parent": "/area", "vars": { "name": "Space" }, "declared_vars": {} },
    "/area/y": { "parent": "/area", "vars": { "name": "Y" }, "declared_vars": { "name": "Y" } }
  }
}"##;

/// Classic format; rows are listed top-down, so the last line is y = 1.
const MAP: &str = r##""a" = (/obj/a/b{dir = 8; pixel_x = 5; id = "x"},/turf/x,/area/y)
"b" = (/obj/a/b/c,/obj/a{alpha = 255; color = "#00ff00"},/turf/x{icon_state = "other"},/area/y)
"c" = (/obj/m,/area/y)

(1,1,1) = {"
ab
ca
"}
"##;

const USED: [&str; 8] = [
    "/obj/a",
    "/obj/a/b",
    "/obj/a/b/c",
    "/obj/m",
    "/turf/x",
    "/area/y",
    "/turf/open/space/basic",
    "/area/space",
];

fn state(dmi: &str, state: &str) -> IconStateInfo {
    IconStateInfo {
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
    }
}

fn manifest() -> AssetManifest {
    AssetManifest {
        atlases: vec![],
        states: vec![
            state("i/a.dmi", "a"),
            state("i/a.dmi", ""),
            state("i/t.dmi", "t"),
            state("i/t.dmi", "other"),
            state("i/s.dmi", ""),
        ],
    }
}

fn app() -> App {
    let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
    let conversion = convert(&types, USED).unwrap();
    // `req_access` is not a supported var; the color matrix is.
    assert_eq!(
        conversion.unsupported.into_iter().collect::<Vec<_>>(),
        [("color".to_owned(), 1)]
    );
    // Ancestors are pulled in, unused types are not.
    assert_eq!(conversion.prototypes, USED.len() + 7);

    let mut app = App::new();
    app.add_plugins((MinimalPlugins, WorldPlugin, DmProtoPlugin));
    let world = app.world();
    let protos = Prototypes::from_sources(
        &[(PathBuf::from("dm_types.ron"), conversion.ron)],
        world.resource::<ProtoComponents>(),
        world.resource::<AppTypeRegistry>(),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    app.insert_resource(protos);
    app
}

/// One file per test: tests run in parallel and would truncate each other's map.
fn map_file(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("mapload_fixture");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{test}.dmm"));
    std::fs::write(&path, MAP).unwrap();
    path
}

#[test]
fn schema_is_checked() {
    let err = DmTypes::parse(br#"{"schema": 1, "types": {}}"#)
        .err()
        .unwrap();
    assert!(err.to_string().contains("schema 1"), "{err}");
}

#[test]
fn prototypes_follow_dm_inheritance() {
    let app = app();
    let protos = app.world().resource::<Prototypes>();
    let atom = |id: &str| protos.get(id).unwrap().get::<DmAtom>().unwrap().clone();
    let name = |id: &str| protos.get(id).unwrap().get::<DmName>().unwrap().clone();

    assert_eq!(protos.get("/obj/a/b").unwrap().parent(), Some("/obj/a"));
    let b = atom("/obj/a/b");
    assert_eq!((b.icon.as_str(), b.icon_state.as_str()), ("i/a.dmi", "a"));
    assert_eq!((b.dir, b.density, b.layer), (4, true, 3.0));
    assert_eq!((b.color, b.alpha), (Some([255, 0, 0, 128]), 128));
    assert_eq!(name("/obj/a/b").name, "thing A");
    assert_eq!(name("/obj/a/b").desc, "bee");
    // Unrepresentable color matrix: inherited instead.
    assert_eq!(atom("/obj/a/b/c").color, Some([255, 0, 0, 128]));
    assert_eq!(atom("/obj/a/b/c").icon_state, "nope");
    // No name anywhere in the chain: BYOND's default from the type path.
    assert_eq!(name("/turf/x").name, "x");
    assert_eq!(name("/turf/open/space/basic").name, "basic");
    assert_eq!(atom("/turf/x").layer, 2.0);
    assert_eq!(name("/area/y").name, "Y");
}

#[test]
fn alpha_folds_into_color() {
    assert_eq!(fold_color(None, 255), None);
    assert_eq!(fold_color(None, 100), Some([255, 255, 255, 100]));
    assert_eq!(fold_color(Some([1, 2, 3, 255]), 255), Some([1, 2, 3, 255]));
    assert_eq!(
        fold_color(Some([255, 0, 0, 128]), 128),
        Some([255, 0, 0, 64])
    );
}

fn on_tile<'a>(world: &'a mut World, x: i32, y: i32) -> Vec<(String, EntityRef<'a>)> {
    let ids = world
        .resource::<SpatialIndex>()
        .on_tile(TilePos::new(x, y, 1))
        .to_vec();
    let world: &World = world;
    let mut out: Vec<_> = ids
        .into_iter()
        .map(|e| {
            let e = world.entity(e);
            (e.get::<DmName>().unwrap().name.clone(), e)
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

#[test]
fn loads_map_with_var_edits() {
    let mut app = app();
    let world = app.world_mut();
    let report = load_maps(world, &[map_file("var_edits")], &manifest()).unwrap();

    assert_eq!(world.resource::<SpatialIndex>().level_size(1), Some((2, 2)));
    let counts: Vec<_> = report.counts.clone().into_iter().collect();
    assert_eq!(
        counts,
        [(Category::Turf, 4), (Category::Obj, 5), (Category::Area, 1)]
    );
    assert_eq!(report.unknown_vars.get("id"), Some(&1));
    assert_eq!(report.default_turfs, 1);
    assert_eq!(report.default_state_fallbacks, 1);
    assert_eq!(
        report.missing_icons.clone().into_iter().collect::<Vec<_>>(),
        [("i/none.dmi:\"\"".to_owned(), 1)]
    );
    // Prefab "a" is used twice but resolved once: a/b{..}, turf/x, area/y, a/b/c, a{..},
    // turf/x{..}, m, default turf.
    assert_eq!(report.templates, 8);

    // Every tile: exactly one turf, and the one area.
    let area = world.resource::<Areas>().entities()[0];
    for (x, y) in [(1, 1), (1, 2), (2, 1), (2, 2)] {
        let turfs = on_tile(world, x, y)
            .iter()
            .filter(|(_, e)| e.contains::<Turf>())
            .count();
        assert_eq!(turfs, 1, "({x}, {y})");
        let at = world.resource::<Areas>().area_at(TilePos::new(x, y, 1));
        assert_eq!(at, Some(area));
    }
    assert_eq!(world.get::<DmName>(area).unwrap().name, "Y");
    assert_eq!(
        world.resource::<Areas>().area_at(TilePos::new(3, 1, 1)),
        None
    );

    let appearance = |e: &EntityRef| e.get::<Appearance>().cloned();
    // (1, 1): only an area and a movable -> world.turf.
    let tile = on_tile(world, 1, 1);
    let (turf, m) = (&tile[0].1, &tile[1].1);
    assert_eq!(tile[0].0, "basic");
    assert_eq!(appearance(turf).unwrap().icon_state, IconStateId(4));
    assert!(
        turf.get::<Facing>().is_none(),
        "south turfs carry no Facing"
    );
    assert!(appearance(m).is_none(), "missing icon is not drawn");
    assert_eq!(m.get::<Facing>(), Some(&Facing(Dir::South)));

    // (2, 1) = "a": the dir/pixel edits override the type, color folds with the type's alpha.
    let tile = on_tile(world, 2, 1);
    let (obj, turf) = (&tile[0].1, &tile[1].1);
    assert_eq!(obj.get::<Facing>(), Some(&Facing(Dir::West)));
    let a = appearance(obj).unwrap();
    assert_eq!(a.icon_state, IconStateId(0));
    assert_eq!(a.pixel_offset, [5, 0]);
    assert_eq!(a.color, Some([255, 0, 0, 64]));
    assert_eq!(a.layer.0, 3.0);
    assert_eq!(a.plane, -4);
    assert!(obj.contains::<Density>());
    assert!(turf.contains::<Density>() && turf.contains::<Opaque>());
    assert_eq!(appearance(turf).unwrap().icon_state, IconStateId(2));
    assert_eq!(
        world
            .resource::<SpatialIndex>()
            .dense_on_tile(TilePos::new(2, 1, 1))
            .len(),
        2
    );

    // (2, 2) = "b": missing state -> default state; edits on an instance of /obj/a.
    let tile = on_tile(world, 2, 2);
    assert_eq!(tile.len(), 3);
    let mut tile: Vec<_> = tile.into_iter().map(|(_, e)| e).collect();
    tile.sort_by_key(|e| e.get::<DmAtom>().unwrap().icon_state.clone());
    let (a, c, turf) = (&tile[0], &tile[1], &tile[2]);
    assert_eq!(c.get::<DmAtom>().unwrap().icon_state, "nope");
    assert_eq!(appearance(c).unwrap().icon_state, IconStateId(1));
    assert_eq!(appearance(a).unwrap().color, Some([0, 255, 0, 255]));
    assert!(!a.contains::<Density>());
    assert_eq!(appearance(turf).unwrap().icon_state, IconStateId(3));
}

/// The startup path the server uses: prototypes from a directory, manifest from disk.
#[test]
fn plugin_loads_at_startup() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("mapload_plugin");
    let assets = dir.join("assets");
    std::fs::create_dir_all(assets.join("prototypes")).unwrap();
    let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
    let ron = convert(&types, USED).unwrap().ron;
    std::fs::write(assets.join("prototypes/dm_types.ron"), ron).unwrap();
    std::fs::write(
        assets.join(ss13_assets::import::MANIFEST_FILE),
        postcard::to_allocvec(&manifest()).unwrap(),
    )
    .unwrap();
    let plugin = MapLoadPlugin {
        source: dir.clone(),
        maps: vec![map_file("plugin").to_string_lossy().into_owned()],
        assets,
    };

    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        WorldPlugin,
        ss13_proto::ProtoPlugin {
            dir: plugin.prototypes_dir(),
        },
        plugin,
    ));
    app.finish();
    app.update();
    let report = app.world().resource::<MapLoadReport>();
    assert_eq!(report.entities(), 10);
    let mut turfs = app.world_mut().query_filtered::<&TilePos, With<Turf>>();
    assert_eq!(turfs.iter(app.world()).count(), 4);
}

#[test]
fn unconverted_types_fail_before_spawning() {
    let mut app = app();
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("mapload_fixture/missing.dmm");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        &path,
        "\"a\" = (/obj/gone,/obj/unused,/turf/x,/area/y)\n(1,1,1) = {\"\na\n\"}\n",
    )
    .unwrap();
    let err = load_maps(app.world_mut(), &[path], &manifest()).unwrap_err();
    let text = format!("{err:#}");
    assert!(
        text.contains("/obj/gone") && text.contains("/obj/unused"),
        "{text}"
    );
    assert_eq!(
        app.world_mut()
            .query::<&TilePos>()
            .iter(app.world())
            .count(),
        0
    );
}
