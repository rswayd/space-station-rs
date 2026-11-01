//! Smoothing, undertile and spawners on a tiny hand-written station.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_assets::manifest::{AssetManifest, IconStateInfo};
use ss13_mapload::convert::{DmTypes, convert};
use ss13_mapload::undertile::{BELOW_CATWALK_LAYER, FLOOR_PLANE};
use ss13_mapload::{
    DmName, DmProtoPlugin, DmSpawner, DmTurf, FidelityReport, HiddenUnderTile, MapLoadReport, Turf,
    fidelity, load_maps, tile_changed,
};
use ss13_proto::{ProtoComponents, Prototypes};
use ss13_shared::{Appearance, Dir, Facing, IconStateId, TilePos};
use ss13_world::{SpatialIndex, WorldPlugin};

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/atom": { "parent": "/datum", "vars": { "layer": 2, "plane": -4, "alpha": 255, "dir": 2,
        "density": 0, "opacity": 0, "pixel_x": 0, "pixel_y": 0, "invisibility": 0 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3, "anchored": 0 } },
    "/obj": { "parent": "/atom/movable" },
    "/obj/a": { "parent": "/obj", "declared_vars": { "icon": { "resource": "i/o.dmi" }, "icon_state": "a" } },
    "/obj/b": { "parent": "/obj", "declared_vars": { "icon": { "resource": "i/o.dmi" }, "icon_state": "b" } },
    "/obj/abstract": { "parent": "/obj", "declared_vars": { "abstract_type": { "path": "/obj/abstract" } } },
    "/obj/structure": { "parent": "/obj", "declared_vars": { "anchored": 1 } },
    "/obj/structure/table": { "parent": "/obj/structure", "declared_vars": {
        "icon": { "resource": "i/table.dmi" }, "icon_state": "table-0", "base_icon_state": "table",
        "smoothing_flags": 1, "smoothing_groups": "-51,", "canSmoothWith": "-51," } },
    "/obj/structure/cable": { "parent": "/obj/structure", "procs": ["Initialize"], "declared_vars": {
        "icon": { "resource": "i/o.dmi" }, "icon_state": "c", "layer": 10010, "plane": -4 } },
    "/obj/machinery": { "parent": "/obj" },
    "/obj/machinery/atmospherics": { "parent": "/obj/machinery", "declared_vars": { "hide": 1 } },
    "/obj/machinery/atmospherics/pipe": { "parent": "/obj/machinery/atmospherics", "procs": ["setup_hiding"],
        "declared_vars": { "icon": { "resource": "i/o.dmi" }, "icon_state": "p" } },
    "/obj/machinery/atmospherics/pipe/visible": { "parent": "/obj/machinery/atmospherics/pipe",
        "declared_vars": { "hide": 0 } },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/spawner": { "parent": "/obj/effect", "procs": ["Initialize"] },
    "/obj/effect/spawner/random": { "parent": "/obj/effect/spawner", "procs": ["Initialize", "spawn_loot"],
        "declared_vars": { "icon": { "resource": "i/o.dmi" }, "icon_state": "loot", "loot": null,
            "spawn_loot_count": 1 } },
    "/obj/effect/spawner/random/test": { "parent": "/obj/effect/spawner/random", "declared_vars": {
        "dir": 4,
        "loot": { "list": [ { "assoc": [ { "path": "/obj/a" }, 1 ] }, { "assoc": [ { "path": "/obj/b" }, 3 ] },
            { "path": "/obj/abstract" } ] } } },
    "/obj/effect/spawner/random/hollow": { "parent": "/obj/effect/spawner/random", "declared_vars": {
        "spawn_loot_count": 4,
        "loot": { "list": [ { "assoc": [ { "list": [ { "path": "/obj/abstract" } ] }, 1 ] },
            { "assoc": [ { "path": "/obj/a" }, 1 ] } ] } } },
    "/obj/effect/spawner/random/custom": { "parent": "/obj/effect/spawner/random", "procs": ["Initialize"],
        "declared_vars": { "loot": { "list": [ { "path": "/obj/a" } ] } } },
    "/obj/effect/spawner/structure": { "parent": "/obj/effect/spawner", "procs": ["Initialize"] },
    "/obj/effect/spawner/structure/window": { "parent": "/obj/effect/spawner/structure",
        "procs": ["Initialize"], "declared_vars": { "spawn_list": { "list": [
            { "path": "/obj/structure/table" }, { "path": "/obj/effect/spawner/random/test" } ] } } },
    "/turf": { "parent": "/atom", "declared_vars": { "icon": { "resource": "i/t.dmi" },
        "underfloor_accessibility": 0 } },
    "/turf/open": { "parent": "/turf" },
    "/turf/open/floor": { "parent": "/turf/open", "declared_vars": { "icon_state": "floor" } },
    "/turf/open/floor/plating": { "parent": "/turf/open/floor", "declared_vars": {
        "icon_state": "plating", "underfloor_accessibility": 2 } },
    "/turf/open/floor/glass": { "parent": "/turf/open/floor", "declared_vars": {
        "icon_state": "glass", "underfloor_accessibility": 1 } },
    "/turf/open/space": { "parent": "/turf/open" },
    "/turf/open/space/basic": { "parent": "/turf/open/space" },
    "/turf/closed": { "parent": "/turf" },
    "/turf/closed/wall": { "parent": "/turf/closed", "declared_vars": {
        "icon": { "resource": "i/wall.dmi" }, "icon_state": "wall-0", "base_icon_state": "wall",
        "smoothing_flags": 1, "smoothing_groups": { "list": ["-1,", "58,"] }, "canSmoothWith": "-1," } },
    "/area": { "parent": "/atom" },
    "/area/space": { "parent": "/area" }
  },
  "element_calls": { "/datum/element/undertile": [
    { "type": "/obj/structure/cable", "proc": "Initialize", "call": "AddElement", "line": "" },
    { "type": "/obj/machinery/atmospherics/pipe", "proc": "setup_hiding", "call": "AddElement", "line": "" }
  ] }
}"##;

/// Rows top-down: y = 3 first.
/// ```text
/// W W W .      W wall, . floor, T table on floor
/// W . T T
/// c p g s      c cable + pipes on floor, p on plating, g cable on glass, s spawner (3 picks)
/// ```
/// `x` holds a structure spawner: a table and a nested random spawner.
const MAP: &str = r##""W" = (/turf/closed/wall,/area/space)
"." = (/turf/open/floor,/area/space)
"T" = (/obj/structure/table,/turf/open/floor,/area/space)
"c" = (/obj/structure/cable,/obj/machinery/atmospherics/pipe,/obj/machinery/atmospherics/pipe/visible,/turf/open/floor,/area/space)
"p" = (/obj/structure/cable,/obj/machinery/atmospherics/pipe,/turf/open/floor/plating,/area/space)
"g" = (/obj/structure/cable,/turf/open/floor/glass,/area/space)
"s" = (/obj/effect/spawner/random/test{spawn_loot_count = 3},/turf/open/floor,/area/space)
"u" = (/obj/effect/spawner/random/custom,/turf/open/floor,/area/space)
"x" = (/obj/effect/spawner/structure/window,/turf/open/floor,/area/space)

(1,1,1) = {"
WWW.u
W.TTx
cpgs.
"}
"##;

const USED: [&str; 15] = [
    "/turf/closed/wall",
    "/turf/open/floor",
    "/turf/open/floor/plating",
    "/turf/open/floor/glass",
    "/obj/structure/table",
    "/obj/structure/cable",
    "/obj/machinery/atmospherics/pipe",
    "/obj/machinery/atmospherics/pipe/visible",
    "/obj/effect/spawner/random/test",
    "/obj/effect/spawner/random/custom",
    "/obj/effect/spawner/random/hollow",
    "/obj/effect/spawner/structure/window",
    "/area/space",
    "/turf/open/space/basic",
    "/obj/effect/spawner/random",
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
    let mut states = Vec::new();
    for j in 0..=255 {
        states.push(state("i/wall.dmi", &format!("wall-{j}")));
        states.push(state("i/table.dmi", &format!("table-{j}")));
    }
    for s in ["a", "b", "c", "p", "loot"] {
        states.push(state("i/o.dmi", s));
    }
    for s in ["floor", "plating", "glass", ""] {
        states.push(state("i/t.dmi", s));
    }
    AssetManifest {
        atlases: vec![],
        states,
    }
}

fn state_name(manifest: &AssetManifest, id: IconStateId) -> String {
    manifest.get(id).unwrap().state.clone()
}

fn loaded(test: &str, seed: u64) -> (App, AssetManifest, FidelityReport) {
    let (app, manifest, _, report) = loaded_map(test, seed, MAP);
    (app, manifest, report)
}

fn loaded_map(
    test: &str,
    seed: u64,
    map: &str,
) -> (App, AssetManifest, MapLoadReport, FidelityReport) {
    let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
    let conversion = convert(&types, USED).unwrap();
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, WorldPlugin, DmProtoPlugin));
    let world = app.world();
    let protos = Prototypes::from_sources(
        &[(PathBuf::from("dm_types.ron"), conversion.ron)],
        world.resource::<ProtoComponents>(),
        world.resource::<AppTypeRegistry>(),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    // Loot types are converted although no map places them.
    assert!(protos.get("/obj/a").is_some() && protos.get("/obj/b").is_some());
    app.insert_resource(protos);

    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("mapload_fidelity");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{test}.dmm"));
    std::fs::write(&path, map).unwrap();
    let manifest = manifest();
    let world = app.world_mut();
    let loaded = load_maps(world, &[path], &manifest).unwrap();
    let report = fidelity::apply(world, &manifest, seed);
    (app, manifest, loaded, report)
}

fn tile(world: &World, x: i32, y: i32) -> Vec<Entity> {
    world
        .resource::<SpatialIndex>()
        .on_tile(TilePos::new(x, y, 1))
        .to_vec()
}

fn named(world: &World, x: i32, y: i32, name: &str) -> Vec<Entity> {
    tile(world, x, y)
        .into_iter()
        .filter(|&e| world.get::<DmName>(e).is_some_and(|n| n.name == name))
        .collect()
}

fn drawn_state(app: &App, manifest: &AssetManifest, x: i32, y: i32, name: &str) -> String {
    let world = app.world();
    let e = named(world, x, y, name)[0];
    state_name(manifest, world.get::<Appearance>(e).unwrap().icon_state)
}

#[test]
fn walls_and_tables_smooth() {
    let (mut app, manifest, report) = loaded("smooth", 1);
    let s = |app: &App, x, y, n| drawn_state(app, &manifest, x, y, n);
    // Walls: S|E, W|E, W, then N for the one below the corner.
    assert_eq!(s(&app, 1, 3, "wall"), "wall-6");
    assert_eq!(s(&app, 2, 3, "wall"), "wall-12");
    assert_eq!(s(&app, 3, 3, "wall"), "wall-8");
    assert_eq!(s(&app, 1, 2, "wall"), "wall-1");
    // Tables smooth with tables (anchored movables), not with the wall above them.
    assert_eq!(s(&app, 3, 2, "table"), "table-4");
    // (4, 2) also has the table from the structure spawner at (5, 2).
    assert_eq!(s(&app, 4, 2, "table"), "table-12");
    assert_eq!(report.smooth.smoothed["wall"], 4);
    assert_eq!(report.smooth.smoothed["table"], 3);
    assert!(report.smooth.missing.is_empty());

    // Destroy the middle wall: its neighbors lose that junction.
    let world = app.world_mut();
    let middle = named(world, 2, 3, "wall")[0];
    world.despawn(middle);
    tile_changed(world, TilePos::new(2, 3, 1));
    assert_eq!(s(&app, 1, 3, "wall"), "wall-2");
    assert_eq!(s(&app, 3, 3, "wall"), "wall-0");
}

#[test]
fn undertile_hides_on_floor_not_on_plating() {
    let (mut app, _, report) = loaded("undertile", 1);
    let world = app.world();
    let hidden = |e: Entity| world.get::<HiddenUnderTile>(e).is_some();
    // Floor tile: cable and the hidden pipe are hidden, the `hide = 0` pipe is not.
    assert!(hidden(named(world, 1, 1, "cable")[0]));
    assert!(hidden(named(world, 1, 1, "pipe")[0]));
    assert!(!hidden(named(world, 1, 1, "visible")[0]));
    // Plating: both drawn as mapped.
    let cable = named(world, 2, 1, "cable")[0];
    assert!(!hidden(cable) && !hidden(named(world, 2, 1, "pipe")[0]));
    assert_eq!(world.get::<Appearance>(cable).unwrap().plane, -4);
    // Glass: visible, moved to the floor plane under catwalks.
    let glass = named(world, 3, 1, "cable")[0];
    assert!(!hidden(glass));
    let a = world.get::<Appearance>(glass).unwrap();
    assert_eq!((a.plane, a.layer.0), (FLOOR_PLANE, BELOW_CATWALK_LAYER));
    assert_eq!(
        (
            report.undertile.hidden,
            report.undertile.visible,
            report.undertile.exposed
        ),
        (2, 1, 2)
    );

    // Pry the floor tile up: what was under it shows.
    let world = app.world_mut();
    let turf = tile(world, 1, 1)
        .into_iter()
        .find(|&e| world.get::<Turf>(e).is_some())
        .unwrap();
    world
        .get_mut::<DmTurf>(turf)
        .unwrap()
        .underfloor_accessibility = 2;
    tile_changed(world, TilePos::new(1, 1, 1));
    let cable = named(app.world(), 1, 1, "cable")[0];
    assert!(app.world().get::<HiddenUnderTile>(cable).is_none());
}

#[test]
fn spawners_resolve_with_a_fixed_seed() {
    let picks = |seed| {
        let (app, _, report) = loaded(&format!("spawner_{seed}"), seed);
        let world = app.world();
        let mut names: Vec<String> = tile(world, 4, 1)
            .into_iter()
            .filter(|&e| world.get::<Turf>(e).is_none())
            .map(|e| {
                // Random spawners turn their loot like themselves.
                assert_eq!(world.get::<Facing>(e), Some(&Facing(Dir::East)));
                world.get::<DmName>(e).unwrap().name.clone()
            })
            .collect();
        names.sort();
        // The custom spawner is removed without loot; the structure spawner left its table and
        // one pick of its nested random spawner.
        assert!(
            tile(world, 5, 3)
                .iter()
                .all(|&e| world.get::<Turf>(e).is_some())
        );
        assert_eq!(named(world, 5, 2, "table").len(), 1);
        assert_eq!(tile(world, 5, 2).len(), 3);
        let mut q = app.world().try_query::<&DmSpawner>().unwrap();
        assert_eq!(q.iter(world).count(), 0, "no spawner survives");
        assert_eq!(report.spawners.resolved, 2);
        assert_eq!(report.spawners.nested, 1);
        assert_eq!(report.spawners.spawned, 3 + 2);
        assert_eq!(
            report.spawners.unresolved.keys().collect::<Vec<_>>(),
            ["/obj/effect/spawner/random/custom (/obj/effect/spawner/random/custom/Initialize())"]
        );
        names
    };
    let first = picks(3);
    assert_eq!(first.len(), 3, "spawn_loot_count = 3 from the map");
    assert!(first.iter().all(|n| n == "a" || n == "b"), "{first:?}");
    assert_eq!(picks(3), first, "same seed, same loot");
}

fn loot_names(world: &World, x: i32, y: i32) -> Vec<String> {
    let mut names: Vec<String> = tile(world, x, y)
        .into_iter()
        .filter(|&e| world.get::<Turf>(e).is_none())
        .map(|e| world.get::<DmName>(e).unwrap().name.clone())
        .collect();
    names.sort();
    names
}

#[test]
fn map_edited_loot_lists_replace_the_type_loot() {
    // The type's own loot is a = 1, b = 3: over 8 seeds of 3 picks, `a` would show up.
    const EDITED: &str = r##""l" = (/obj/effect/spawner/random/test{loot = list(/obj/b, /obj/b = 2); spawn_loot_count = 3},/turf/open/floor,/area/space)
"s" = (/obj/effect/spawner/structure/window{spawn_list = list(/obj/a)},/turf/open/floor,/area/space)

(1,1,1) = {"
ls
"}
"##;
    for seed in 0..8 {
        let (app, _, loaded, _) = loaded_map("edited_loot", seed, EDITED);
        assert!(
            !loaded.unknown_vars.contains_key("loot")
                && !loaded.unknown_vars.contains_key("spawn_list")
                && loaded.unsupported_values.is_empty(),
            "{loaded:?}"
        );
        assert_eq!(
            loot_names(app.world(), 1, 1),
            ["b", "b", "b"],
            "seed {seed}"
        );
        assert_eq!(loot_names(app.world(), 2, 1), ["a"], "seed {seed}");
    }
}

#[test]
fn empty_nested_list_picks_nothing_and_rolls_on() {
    // `hollow`: 4 picks from list(list(/obj/abstract) = 1, /obj/a = 1); the nested list is empty
    // after cleaning, so each pick is `a` with 1/2 and nothing otherwise (tg counts the null).
    const HOLLOW: &str = r##""h" = (/obj/effect/spawner/random/hollow,/turf/open/floor,/area/space)

(1,1,1) = {"
h
"}
"##;
    let total: usize = (0..32)
        .map(|seed| {
            let (app, ..) = loaded_map("hollow", seed, HOLLOW);
            let names = loot_names(app.world(), 1, 1);
            assert!(
                names.len() <= 4 && names.iter().all(|n| n == "a"),
                "{names:?}"
            );
            names.len()
        })
        .sum();
    // Expected 64 of 128 picks; stopping at the first null would give about 30.
    assert!((48..=80).contains(&total), "{total}");
}
