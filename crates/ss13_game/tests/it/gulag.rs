//! The labor camp teleporter and the item reclaimer (`ss13_game::brig::gulag`): a prisoner shut
//! in the teleporter is processed by a console with a prisoner ID inserted (refused without brig
//! access): stripped into the reclaimer, dressed in the prisoner jumpsuit with the ID (goal
//! defaulted to 200) and moved onto the beacon; the teleporter opens. The prisoner gets their
//! items back from the reclaimer only once their ID reaches its goal.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::brig::gulag::{
    DEFAULT_GOAL, GulagAct, GulagBeacon, GulagConsole, GulagReclaimer, GulagTeleporter, JUMPSUIT,
    SHOES, console_act, release_items,
};
use ss13_game::brig::prisoner::{PRISONER_CARD, PrisonerCard, PrisonerConsole};
use ss13_game::closets::InCloset;
use ss13_game::inventory::{IdCard, Inventory};
use ss13_game::medical::pods::{Pod, close_pod};
use ss13_game::storage::StoredIn;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::DmItem;
use ss13_shared::TilePos;
use ss13_world::SpatialIndex;

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 }, "declared_vars": { "light_power": 1 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3, "anchored": 0 } },
    "/obj": { "parent": "/atom/movable" },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start" },
    "/obj/item": { "parent": "/obj", "declared_vars": { "icon": { "resource": "i.dmi" },
        "icon_state": "toy", "interaction_flags_item": 1 } },
    "/obj/item/card": { "parent": "/obj/item" },
    "/obj/item/card/id": { "parent": "/obj/item/card", "declared_vars": { "slot_flags": 256 } },
    "/obj/item/card/id/advanced": { "parent": "/obj/item/card/id" },
    "/obj/item/card/id/advanced/prisoner": { "parent": "/obj/item/card/id/advanced",
        "declared_vars": { "name": "prisoner ID card" } },
    "/obj/item/clothing": { "parent": "/obj/item" },
    "/obj/item/clothing/under": { "parent": "/obj/item/clothing", "declared_vars": {
        "slot_flags": 2 } },
    "/obj/item/clothing/under/color": { "parent": "/obj/item/clothing/under" },
    "/obj/item/clothing/under/color/grey": { "parent": "/obj/item/clothing/under/color" },
    "/obj/item/clothing/under/rank": { "parent": "/obj/item/clothing/under" },
    "/obj/item/clothing/under/rank/prisoner": { "parent": "/obj/item/clothing/under/rank" },
    "/obj/item/clothing/shoes": { "parent": "/obj/item/clothing", "declared_vars": {
        "slot_flags": 128 } },
    "/obj/item/clothing/shoes/sneakers": { "parent": "/obj/item/clothing/shoes" },
    "/obj/item/clothing/shoes/sneakers/black": { "parent": "/obj/item/clothing/shoes/sneakers" },
    "/obj/item/clothing/shoes/sneakers/orange": { "parent": "/obj/item/clothing/shoes/sneakers" },
    "/obj/machinery": { "parent": "/obj", "declared_vars": { "anchored": 1,
        "use_power": 1, "idle_power_usage": 100, "active_power_usage": 1000, "power_channel": 1,
        "req_access": null, "req_one_access": null } },
    "/obj/machinery/gulag_teleporter": { "parent": "/obj/machinery", "declared_vars": {
        "name": "labor camp teleporter", "icon": { "resource": "ic.dmi" },
        "icon_state": "implantchair", "density": 1 } },
    "/obj/machinery/gulag_item_reclaimer": { "parent": "/obj/machinery", "declared_vars": {
        "name": "equipment reclaimer station", "icon": { "resource": "wm.dmi" },
        "icon_state": "gulag_off" } },
    "/obj/machinery/computer": { "parent": "/obj/machinery", "declared_vars": {
        "icon": { "resource": "c.dmi" }, "icon_state": "computer", "density": 1 } },
    "/obj/machinery/computer/prisoner": { "parent": "/obj/machinery/computer" },
    "/obj/machinery/computer/prisoner/gulag_teleporter_computer": {
        "parent": "/obj/machinery/computer/prisoner", "declared_vars": {
        "name": "labor camp teleporter console", "req_access": { "list": ["brig"] } } },
    "/obj/structure": { "parent": "/obj", "declared_vars": { "anchored": 1 } },
    "/obj/structure/gulag_beacon": { "parent": "/obj/structure", "declared_vars": {
        "name": "labor camp bluespace beacon", "icon": { "resource": "t.dmi" },
        "icon_state": "floor" } },
    "/mob": { "parent": "/atom/movable", "declared_vars": { "layer": 4, "density": 1 } },
    "/mob/living": { "parent": "/mob" },
    "/mob/living/carbon": { "parent": "/mob/living" },
    "/mob/living/carbon/human": { "parent": "/mob/living/carbon", "declared_vars": {
        "icon": { "resource": "h.dmi" }, "icon_state": "human_basic" } },
    "/turf": { "parent": "/atom", "declared_vars": { "icon": { "resource": "t.dmi" } } },
    "/turf/open": { "parent": "/turf" },
    "/turf/open/floor": { "parent": "/turf/open", "declared_vars": { "icon_state": "floor" } },
    "/turf/open/space": { "parent": "/turf/open" },
    "/turf/open/space/basic": { "parent": "/turf/open/space" },
    "/turf/closed": { "parent": "/turf", "declared_vars": { "density": 1, "opacity": 1 } },
    "/turf/closed/wall": { "parent": "/turf/closed", "declared_vars": { "icon_state": "wall" } },
    "/area": { "parent": "/atom", "declared_vars": { "requires_power": 0, "always_unpowered": 0,
        "power_equip": 1, "power_light": 1, "power_environ": 1, "lightswitch": 1,
        "static_lighting": 1 } },
    "/area/space": { "parent": "/area" },
    "/area/station": { "parent": "/area" }
  }
}"##;

/// The player at (2,3), the console at (3,3), the teleporter at (4,3), the beacon at (6,3),
/// the reclaimer at (3,2).
const MAP: &str = r##""w" = (/turf/closed/wall,/area/station)
"f" = (/turf/open/floor,/area/station)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/station)
"C" = (/obj/machinery/computer/prisoner/gulag_teleporter_computer,/turf/open/floor,/area/station)
"T" = (/obj/machinery/gulag_teleporter,/turf/open/floor,/area/station)
"B" = (/obj/structure/gulag_beacon,/turf/open/floor,/area/station)
"R" = (/obj/machinery/gulag_item_reclaimer,/turf/open/floor,/area/station)

(1,1,1) = {"
wwwwwww
wfffffw
wsCTfBw
wfRfffw
wwwwwww
"}
"##;

const SPAWN: TilePos = TilePos::new(2, 3, 1);
const CONSOLE: TilePos = TilePos::new(3, 3, 1);
const TELEPORTER: TilePos = TilePos::new(4, 3, 1);
const BEACON: TilePos = TilePos::new(6, 3, 1);
const RECLAIMER: TilePos = TilePos::new(3, 2, 1);

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("gulag_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/machinery/computer/prisoner/gulag_teleporter_computer",
        "/obj/machinery/gulag_teleporter",
        "/obj/machinery/gulag_item_reclaimer",
        "/obj/structure/gulag_beacon",
        PRISONER_CARD,
        JUMPSUIT,
        SHOES,
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
        "/area/station",
    ];
    let states = [
        ("t.dmi", "floor"),
        ("t.dmi", "wall"),
        ("l.dmi", "x2"),
        ("h.dmi", "human_basic"),
        ("i.dmi", "toy"),
        ("c.dmi", "computer"),
        ("ic.dmi", "implantchair"),
        ("ic.dmi", "implantchair_open"),
        ("ic.dmi", "implantchair_occupied"),
        ("wm.dmi", "gulag_off"),
    ];
    write_fixture(&dir, TYPES, &used, &states, &[("map.dmm", MAP)])
}

fn find<T: Component>(s: &testing::Station, pos: TilePos) -> Entity {
    let world = s.server.world();
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .find(|&e| world.get::<T>(e).is_some())
        .unwrap_or_else(|| panic!("nothing with {} on {pos:?}", std::any::type_name::<T>()))
}

fn path_of(s: &testing::Station, e: Option<Entity>) -> Option<String> {
    e.and_then(|e| s.try_get::<DmItem>(e))
        .map(|d| d.path.clone())
}

#[test]
fn prisoner_is_processed_and_reclaims_items_after_the_goal() {
    let mut s = testing::Station::start(
        fixture_map(&fixture_assets("process"), &["map.dmm"]),
        |_| {},
    );
    s.join("p0", |_| {});
    let (mob, client) = (s.players[0].mob, s.players[0].link);
    assert_eq!(*s.get::<TilePos>(mob), SPAWN);
    let console = find::<GulagConsole>(&s, CONSOLE);
    let teleporter = find::<GulagTeleporter>(&s, TELEPORTER);
    let beacon = find::<GulagBeacon>(&s, BEACON);
    let reclaimer = find::<GulagReclaimer>(&s, RECLAIMER);
    assert_eq!(s.get::<GulagConsole>(console).teleporter, Some(teleporter));
    assert_eq!(s.get::<GulagConsole>(console).beacon, Some(beacon));
    assert_eq!(
        s.get::<GulagTeleporter>(teleporter).reclaimer,
        Some(reclaimer)
    );
    let old_id = s.get::<Inventory>(mob).id.expect("an assistant ID");
    let old_uniform = s.get::<Inventory>(mob).uniform.expect("a jumpsuit");

    // Shut in, a prisoner ID in the console.
    let card = {
        let world = s.server.world_mut();
        assert_eq!(close_pod(world, teleporter, Some(mob)), Some(mob));
        let card = ss13_game::construction::spawn_obj(world, PRISONER_CARD, CONSOLE)
            .expect("the prisoner ID prototype");
        ss13_game::storage::put_inside(world, card, console);
        world
            .get_mut::<PrisonerConsole>(console)
            .unwrap()
            .contained_id = Some(card);
        card
    };
    s.run_for(0.2);
    assert_eq!(s.get::<PrisonerCard>(card).goal, 0);

    // No brig access: refused.
    console_act(
        s.server.world_mut(),
        client,
        mob,
        console,
        GulagAct::Teleport,
    );
    assert!(s.get::<GulagConsole>(console).pending.is_none());

    {
        let world = s.server.world_mut();
        world
            .get_mut::<IdCard>(old_id)
            .unwrap()
            .access
            .push("brig".to_owned());
    }
    console_act(
        s.server.world_mut(),
        client,
        mob,
        console,
        GulagAct::Teleport,
    );
    assert!(s.get::<GulagConsole>(console).pending.is_some());
    s.run_until("the prisoner arrives at the beacon", |s| {
        *s.get::<TilePos>(mob) == BEACON
    });
    assert!(s.try_get::<InCloset>(mob).is_none());
    let inventory = s.get::<Inventory>(mob).clone();
    assert_eq!(inventory.id, Some(card), "the prisoner ID is worn");
    assert_eq!(path_of(&s, inventory.uniform).as_deref(), Some(JUMPSUIT));
    assert_eq!(path_of(&s, inventory.shoes).as_deref(), Some(SHOES));
    assert_eq!(s.get::<PrisonerCard>(card).goal, DEFAULT_GOAL);
    assert!(s.get::<Pod>(teleporter).open, "the teleporter opens");
    assert!(!s.get::<GulagTeleporter>(teleporter).locked);
    assert!(s.get::<PrisonerConsole>(console).contained_id.is_none());
    let stored = s.get::<GulagReclaimer>(reclaimer).stored.clone();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].mob, mob);
    for item in [old_id, old_uniform] {
        assert!(stored[0].items.contains(&item));
        assert_eq!(s.get::<StoredIn>(item).0, reclaimer);
    }

    // The sentence is not served: no items.
    release_items(s.server.world_mut(), client, mob, reclaimer, mob);
    assert_eq!(s.get::<GulagReclaimer>(reclaimer).stored.len(), 1);
    s.server
        .world_mut()
        .get_mut::<PrisonerCard>(card)
        .unwrap()
        .points = DEFAULT_GOAL;
    release_items(s.server.world_mut(), client, mob, reclaimer, mob);
    assert!(s.get::<GulagReclaimer>(reclaimer).stored.is_empty());
    s.run_for(0.2);
    for item in [old_id, old_uniform] {
        assert!(s.try_get::<StoredIn>(item).is_none());
        assert_eq!(*s.get::<TilePos>(item), RECLAIMER);
    }
}
