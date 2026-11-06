//! A template loaded mid-round comes up initialized like the station
//! (`ss13_mapload::load_template` + `ss13_world::run_map_init`): its airlock is a door that the
//! area's power opens, its APC and RTG share a powernet of their own in the station's grid, its
//! closet took the item mapped under it; the station's machines are left as they were.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::closets::Closet;
use ss13_game::doors::{Door, DoorState};
use ss13_game::power::{Apc, AreaPower, Grid, Unpowered};
use ss13_game::storage::Stored;
use ss13_game::testing::{self, fixture_map, steps, write_fixture};
use ss13_mapload::DoorKind;
use ss13_shared::TilePos;
use ss13_world::{Mapped, SpatialIndex};

/// Rows top-down, y = 1 at the bottom: an RTG (2,3) cabled to an APC (4,3), a closet over a toy
/// (3,2) and an airlock (5,2).
const TEMPLATE: &str = r##""w" = (/turf/closed/wall,/area/station)
"f" = (/turf/open/floor,/area/station)
"R" = (/obj/structure/cable,/obj/machinery/power/rtg,/turf/open/floor,/area/station)
"c" = (/obj/structure/cable,/turf/open/floor,/area/station)
"A" = (/obj/structure/cable,/obj/machinery/power/apc{dir = 1; pixel_y = 25},/turf/open/floor,/area/station)
"C" = (/obj/structure/closet,/obj/item/toy,/turf/open/floor,/area/station)
"D" = (/obj/machinery/door/airlock{normalspeed = 0},/turf/open/floor,/area/station)

(1,1,1) = {"
wwwwwww
wRcAffw
wfCfDfw
wwwwwww
"}
"##;

const CLOSET_TYPES: &str = r##"
    "/obj/item/toy": { "parent": "/obj/item", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "w_class": 1 } },
    "/obj/structure/closet": { "parent": "/obj/structure", "declared_vars": {
        "name": "closet", "icon": { "resource": "k.dmi" }, "icon_state": "generic",
        "density": 1, "opened": 0, "locked": 0, "welded": 0, "secure": 0, "icon_door": null,
        "icon_welded": "welded", "enable_door_overlay": 1, "has_opened_overlay": 1,
        "has_closed_overlay": 1, "dense_when_open": 0, "wall_mounted": 0, "horizontal": 0,
        "storage_capacity": 30 } },
    "/mob": {"##;

/// The power fixture's station (`crate::power`) and [`TEMPLATE`] next to it.
fn fixture_assets() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("map_init_fixture");
    let types = crate::power::TYPES
        .replacen(r#"    "/mob": {"#, CLOSET_TYPES, 1)
        .replacen(
            r#""element_calls":"#,
            r#""populate": { "/obj/structure/closet": { "PopulateContents": ["return"] } },
  "element_calls":"#,
            1,
        );
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/machinery/door/airlock",
        "/obj/machinery/light",
        "/obj/machinery/power/apc",
        "/obj/machinery/power/smes/engineering",
        "/obj/machinery/power/terminal",
        "/obj/machinery/power/rtg",
        "/obj/structure/cable",
        "/obj/structure/closet",
        "/obj/item/toy",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
        "/area/station",
    ];
    let states = [
        ("t.dmi", "floor"),
        ("t.dmi", "wall"),
        ("t.dmi", ""),
        ("l.dmi", "x2"),
        ("h.dmi", "human_basic"),
        ("a.dmi", "closed"),
        ("a.dmi", "opening"),
        ("a.dmi", "open"),
        ("a.dmi", "closing"),
        ("apc.dmi", "apc0"),
        ("k.dmi", "generic"),
        ("k.dmi", "generic_open"),
        ("k.dmi", "generic_door"),
        ("i.dmi", "toy"),
    ];
    write_fixture(
        &dir,
        &types,
        &used,
        &states,
        &[("map.dmm", crate::power::MAP), ("template.dmm", TEMPLATE)],
    )
}

fn find<T: Component>(world: &World, pos: TilePos) -> Entity {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .find(|&e| world.get::<T>(e).is_some())
        .unwrap_or_else(|| panic!("nothing with {} at {pos:?}", std::any::type_name::<T>()))
}

#[test]
fn a_template_loaded_mid_round_works() {
    let dir = fixture_assets();
    let mut app = testing::server_app(fixture_map(&dir, &["map.dmm"]), |_| {});
    for _ in 0..steps(1.0) {
        app.update();
    }
    let world = app.world_mut();
    let station_apc = find::<Apc>(world, TilePos::new(7, 5, 1));
    let station_area = world.get::<Apc>(station_apc).unwrap().area;
    let station_net = world.resource::<Grid>().node_net(station_apc);
    assert!(station_net.is_some());

    let z = ss13_mapload::load_template(world, &dir.join("template.dmm")).unwrap();
    assert!(z > 1);
    assert!(ss13_world::map_init_open(world));
    ss13_world::run_map_init(world);
    assert!(!ss13_world::map_init_open(world));
    let mut marked = world.query_filtered::<Entity, With<Mapped>>();
    assert_eq!(marked.iter(world).count(), 0, "the pass left markers");

    // Power: the template's APC runs its own area, on the RTG's net, in the station's grid.
    let apc = find::<Apc>(world, TilePos::new(4, 3, z));
    let rtg = world
        .resource::<SpatialIndex>()
        .on_tile(TilePos::new(2, 3, z))
        .iter()
        .copied()
        .find(|&e| world.resource::<Grid>().node_net(e).is_some())
        .expect("the RTG is a grid node");
    let area = world.get::<Apc>(apc).unwrap().area;
    assert_ne!(area, station_area);
    assert_eq!(world.get::<AreaPower>(area).unwrap().apc, Some(apc));
    assert_eq!(
        world.get::<AreaPower>(station_area).unwrap().apc,
        Some(station_apc)
    );
    let grid = world.resource::<Grid>();
    assert!(grid.node_net(apc).is_some());
    assert_eq!(grid.node_net(apc), grid.node_net(rtg));
    assert_ne!(grid.node_net(apc), station_net);
    assert_eq!(grid.node_net(station_apc), station_net);

    // The closet took the toy mapped under it, and gives it back when opened.
    let closet = find::<Closet>(world, TilePos::new(3, 2, z));
    let inside: Vec<Entity> = world
        .get::<Stored>(closet)
        .map(|s| s.items().to_vec())
        .unwrap_or_default();
    assert_eq!(inside.len(), 1, "the mapped toy");
    assert!(ss13_game::closets::open(world, closet));
    assert_eq!(
        world.get::<TilePos>(inside[0]),
        Some(&TilePos::new(3, 2, z))
    );

    // The airlock is a door, powered by its area, and opens.
    let door = find::<Door>(world, TilePos::new(5, 2, z));
    assert_eq!(world.get::<Door>(door).unwrap().kind, DoorKind::Airlock);
    for _ in 0..steps(3.0) {
        app.update();
    }
    let world = app.world_mut();
    assert!(
        world.get::<Unpowered>(door).is_none(),
        "the template's area powers it"
    );
    let opener = world.spawn(TilePos::new(4, 2, z)).id();
    ss13_game::doors::try_activate(world, door, opener, false);
    let mut opened = false;
    for _ in 0..steps(2.0) {
        app.update();
        if app.world().get::<Door>(door).unwrap().state != DoorState::Closed {
            opened = true;
            break;
        }
    }
    assert!(opened, "the template's airlock opens");
}
