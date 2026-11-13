//! A shuttle docking at a port of another direction turns with it (tg `initiate_docking` ->
//! `shuttleRotate`), as the mining shuttle does landing on Lavaland (`mining_home` dir 8 ->
//! `mining_away` dir 2): the template's port faces south, the test dock east, so the shuttle
//! turns 270 degrees clockwise. Positions, dirs, border edges, pixel offsets and wall smoothing
//! follow, and docking back at the template's spot restores all of them.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::construction::turf_at;
use ss13_game::shuttle::{EmergencyShuttle, ShuttleConfig, StationaryDock, initiate_docking};
use ss13_game::testing::{self, fixture_map, write_fixture};
use ss13_mapload::{DmObj, IconIndex};
use ss13_shared::{Appearance, Dir, Facing, TilePos};
use ss13_world::{Border, SpatialIndex};

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 }, "declared_vars": { "can_atmos_pass": 1,
        "flags_1": 0 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3, "anchored": 0 } },
    "/obj": { "parent": "/atom/movable" },
    "/obj/docking_port": { "parent": "/obj", "declared_vars": { "invisibility": 101,
        "icon": { "resource": "s.dmi" }, "icon_state": "mark", "anchored": 1, "dir": 1,
        "shuttle_id": null, "width": 0, "height": 0, "dwidth": 0, "dheight": 0 } },
    "/obj/docking_port/stationary": { "parent": "/obj/docking_port", "declared_vars": {
        "json_key": null } },
    "/obj/docking_port/mobile": { "parent": "/obj/docking_port" },
    "/obj/docking_port/mobile/emergency": { "parent": "/obj/docking_port/mobile",
        "declared_vars": { "shuttle_id": "emergency", "dir": 4 } },
    "/obj/structure": { "parent": "/obj", "declared_vars": { "anchored": 1 } },
    "/obj/structure/testmark": { "parent": "/obj/structure", "declared_vars": {
        "icon": { "resource": "s.dmi" }, "icon_state": "mark" } },
    "/obj/structure/testmount": { "parent": "/obj/structure", "declared_vars": {
        "icon": { "resource": "s.dmi" }, "icon_state": "mount" } },
    "/obj/structure/window": { "parent": "/obj/structure", "declared_vars": {
        "icon": { "resource": "s.dmi" }, "icon_state": "win", "density": 1, "flags_1": 2 } },
    "/turf": { "parent": "/atom", "declared_vars": { "icon": { "resource": "t.dmi" },
        "blocks_air": 0, "can_atmos_pass": 0, "initial_gas_mix": "o2=22;n2=82;TEMP=293.15",
        "temperature": 293.15, "baseturfs": { "path": "/turf/baseturf_bottom" } } },
    "/turf/open": { "parent": "/turf", "declared_vars": { "can_atmos_pass": -1,
        "planetary_atmos": 0 } },
    "/turf/open/floor": { "parent": "/turf/open", "declared_vars": { "icon_state": "floor",
        "baseturfs": { "path": "/turf/open/floor/plating" } } },
    "/turf/open/floor/plating": { "parent": "/turf/open/floor", "declared_vars": {
        "icon_state": "plating", "baseturfs": { "path": "/turf/baseturf_bottom" } } },
    "/turf/open/space": { "parent": "/turf/open", "declared_vars": { "temperature": 2.7,
        "initial_gas_mix": "TEMP=2.7", "icon_state": "space",
        "baseturfs": { "path": "/turf/open/space" } } },
    "/turf/open/space/basic": { "parent": "/turf/open/space" },
    "/turf/closed": { "parent": "/turf", "declared_vars": { "density": 1, "opacity": 1,
        "blocks_air": 1 } },
    "/turf/closed/wall": { "parent": "/turf/closed", "declared_vars": { "icon_state": "swall",
        "base_icon_state": "swall", "smoothing_flags": 1, "smoothing_groups": "5,",
        "canSmoothWith": "5,", "baseturfs": { "path": "/turf/open/floor/plating" } } },
    "/area": { "parent": "/atom" },
    "/area/space": { "parent": "/area", "declared_vars": { "name": "Space" } },
    "/area/shuttle": { "parent": "/area" },
    "/area/shuttle/escape": { "parent": "/area/shuttle", "declared_vars": {
        "name": "Emergency Shuttle" } }
  }
}"##;

/// Space with the east-facing test dock `D` at (3,3).
const MAP: &str = r##""e" = (/turf/open/space/basic,/area/space)
"D" = (/obj/docking_port/stationary{dir = 4; dwidth = 1; height = 2; name = "Rotated bay"; shuttle_id = "rot_test"; width = 3},/turf/open/space/basic,/area/space)

(1,1,1) = {"
eeeeee
eeeeee
eeeeee
eeDeee
eeeeee
eeeeee
"}
"##;

/// A row of three smoothing walls (the south-facing port on the middle one) over a row of floor:
/// a north-facing mark, a north-facing floor, a north border window with a north wall mount
/// (`pixel_y = 32`).
const SHUTTLE: &str = r##""w" = (/turf/closed/wall,/area/shuttle/escape)
"p" = (/obj/docking_port/mobile/emergency{dir = 2; name = "Rot"},/turf/closed/wall,/area/shuttle/escape)
"c" = (/obj/structure/testmark{dir = 1},/turf/open/floor,/area/shuttle/escape)
"f" = (/turf/open/floor{dir = 1},/area/shuttle/escape)
"x" = (/obj/structure/window{dir = 1},/obj/structure/testmount{pixel_y = 32},/turf/open/floor,/area/shuttle/escape)

(1,1,1) = {"
wpw
cfx
"}
"##;

const STATES: [(&str, &str); 14] = [
    ("t.dmi", "floor"),
    ("t.dmi", "plating"),
    ("t.dmi", "space"),
    ("t.dmi", "swall"),
    ("t.dmi", "swall-1"),
    ("t.dmi", "swall-2"),
    ("t.dmi", "swall-3"),
    ("t.dmi", "swall-4"),
    ("t.dmi", "swall-8"),
    ("t.dmi", "swall-12"),
    ("s.dmi", "mark"),
    ("s.dmi", "mount"),
    ("s.dmi", "win"),
    ("t.dmi", "swall-0"),
];

fn fixture_assets() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("shuttle_rotate_fixture");
    let used = [
        "/obj/docking_port/stationary",
        "/obj/docking_port/mobile/emergency",
        "/obj/structure/testmark",
        "/obj/structure/testmount",
        "/obj/structure/window",
        "/turf/open/floor",
        "/turf/open/floor/plating",
        "/turf/open/space/basic",
        "/turf/closed/wall",
        "/area/space",
        "/area/shuttle/escape",
    ];
    write_fixture(
        &dir,
        TYPES,
        &used,
        &STATES,
        &[("map.dmm", MAP), ("shuttle.dmm", SHUTTLE)],
    )
}

fn obj(world: &World, pos: TilePos, path: &str) -> Entity {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .find(|&e| world.get::<DmObj>(e).is_some_and(|o| o.path == path))
        .unwrap_or_else(|| panic!("no {path} on {pos:?}"))
}

fn facing(world: &World, e: Entity) -> Dir {
    world.get::<Facing>(e).map_or(Dir::South, |f| f.0)
}

fn state(world: &World, e: Entity) -> String {
    let id = world.get::<Appearance>(e).expect("drawn").icon_state;
    let icons = world.resource::<IconIndex>();
    STATES
        .iter()
        .find(|(dmi, s)| icons.get(dmi, s) == Some(id))
        .map(|(_, s)| (*s).to_owned())
        .unwrap_or_default()
}

/// The shuttle's tiles as `[wall a, port wall, wall b, mark, floor, window + mount]` for the
/// template rows `wpw` / `cfx`.
struct Layout([TilePos; 6]);

/// Checks every carried atom: `dir` the north of the template now points to, `walls` the
/// junction states of `[a, port wall, b]`, `pixel` the mount's offset.
fn check(world: &World, at: &Layout, north: Dir, walls: [&str; 3], pixel: [i16; 2]) {
    let [a, p, b, c, f, x] = at.0;
    for (pos, want) in [(a, walls[0]), (p, walls[1]), (b, walls[2])] {
        let wall = turf_at(world, pos).expect("wall");
        assert_eq!(state(world, wall), want, "wall on {pos:?}");
    }
    let mark = obj(world, c, "/obj/structure/testmark");
    assert_eq!(facing(world, mark), north);
    let floor = turf_at(world, f).expect("floor");
    assert_eq!(facing(world, floor), north);
    let window = obj(world, x, "/obj/structure/window");
    assert_eq!(facing(world, window), north);
    assert_eq!(
        world.get::<Border>(window).map(|b| b.edges),
        Some(Border::facing(north, false).edges)
    );
    let mount = obj(world, x, "/obj/structure/testmount");
    assert_eq!(world.get::<Appearance>(mount).unwrap().pixel_offset, pixel);
}

#[test]
fn a_shuttle_turns_with_a_dock_of_another_direction_and_back() {
    let assets = fixture_assets();
    let config = ShuttleConfig {
        call_secs: 5.0,
        dock_secs: 12.0,
        escape_secs: 1.0,
        refuel_secs: 0.0,
        autocall_threshold: None,
        template: Some(assets.join("shuttle.dmm")),
    };
    let mut app = testing::server_app(fixture_map(&assets, &["map.dmm"]), |app| {
        app.insert_resource(config);
    });
    let world = app.world_mut();
    let (port, away) = {
        let shuttle = world.resource::<EmergencyShuttle>();
        (shuttle.port, shuttle.away)
    };
    let home = *world.get::<TilePos>(port).unwrap();
    let z = home.z;
    // The template: port at (2,2) of a 3x2 template, facing south.
    let template = Layout([
        TilePos::new(home.x - 1, home.y, z),
        home,
        TilePos::new(home.x + 1, home.y, z),
        TilePos::new(home.x - 1, home.y - 1, z),
        TilePos::new(home.x, home.y - 1, z),
        TilePos::new(home.x + 1, home.y - 1, z),
    ]);
    check(
        world,
        &template,
        Dir::North,
        ["swall-4", "swall-12", "swall-8"],
        [0, 32],
    );

    let dock = {
        let mut q = world.query::<(Entity, &StationaryDock)>();
        q.iter(world)
            .find(|(_, d)| d.id == "rot_test")
            .map(|(e, _)| e)
            .expect("test dock")
    };
    let d = *world.get::<TilePos>(dock).unwrap();
    initiate_docking(world, port, dock).expect("docks");
    // tg `return_ordered_turfs` of the east dock: template (dx, dy) lands on
    // (Dx + dy, Dy + 1 - dx); the template's north is now west, its west south.
    let turned = Layout([
        TilePos::new(d.x, d.y - 1, d.z),
        d,
        TilePos::new(d.x, d.y + 1, d.z),
        TilePos::new(d.x + 1, d.y - 1, d.z),
        TilePos::new(d.x + 1, d.y, d.z),
        TilePos::new(d.x + 1, d.y + 1, d.z),
    ]);
    assert_eq!(*world.get::<TilePos>(port).unwrap(), d);
    assert_eq!(facing(world, port), Dir::East);
    check(
        world,
        &turned,
        Dir::West,
        ["swall-1", "swall-3", "swall-2"],
        [-32, 0],
    );

    initiate_docking(world, port, away).expect("docks back");
    assert_eq!(*world.get::<TilePos>(port).unwrap(), home);
    assert_eq!(facing(world, port), Dir::South);
    check(
        world,
        &template,
        Dir::North,
        ["swall-4", "swall-12", "swall-8"],
        [0, 32],
    );
}
