//! Tank dispensers and tank holders over real WebTransport on 127.0.0.1: a plasma tank comes out
//! of the dispenser and goes back in, an oxygen tank comes out by right click; a mapped holder's
//! extinguisher comes out and goes back, an emptied holder is screwed apart into rods.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::inventory::{CarriedBy, Inventory};
use ss13_game::tank_storage::{TankDispenser, TankHolder};
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::{DmItem, DmObj};
use ss13_shared::TilePos;
use ss13_shared::protocol::ClickModifiers;
use ss13_world::{Density, SpatialIndex};

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3, "anchored": 0 } },
    "/obj": { "parent": "/atom/movable", "declared_vars": { "force": 0, "damtype": "brute",
        "obj_flags": 2, "max_integrity": 100 } },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start" },
    "/obj/structure": { "parent": "/obj", "declared_vars": { "anchored": 1 } },
    "/obj/structure/tank_dispenser": { "parent": "/obj/structure", "declared_vars": {
        "name": "tank dispenser", "icon": { "resource": "s.dmi" }, "icon_state": "dispenser",
        "density": 1, "max_integrity": 300 } },
    "/obj/structure/tank_holder": { "parent": "/obj/structure", "declared_vars": {
        "name": "tank holder", "icon": { "resource": "c.dmi" }, "icon_state": "holder",
        "anchored": 0, "max_integrity": 20 } },
    "/obj/structure/tank_holder/extinguisher": { "parent": "/obj/structure/tank_holder",
        "declared_vars": { "icon_state": "holder_extinguisher" } },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null, "attack_speed": 8,
        "tool_behaviour": null, "toolspeed": 1 } },
    "/obj/item/screwdriver": { "parent": "/obj/item", "declared_vars": {
        "tool_behaviour": "screwdriver" } },
    "/obj/item/extinguisher": { "parent": "/obj/item", "declared_vars": {
        "name": "fire extinguisher" } },
    "/obj/item/tank": { "parent": "/obj/item", "declared_vars": { "slot_flags": 1024,
        "volume": 70, "distribute_pressure": 101.325 } },
    "/obj/item/tank/internals": { "parent": "/obj/item/tank" },
    "/obj/item/tank/internals/oxygen": { "parent": "/obj/item/tank/internals" },
    "/obj/item/tank/internals/plasma": { "parent": "/obj/item/tank/internals" },
    "/obj/item/stack": { "parent": "/obj/item", "declared_vars": { "amount": 1, "max_amount": 50 } },
    "/obj/item/stack/rods": { "parent": "/obj/item/stack" },
    "/obj/item/card": { "parent": "/obj/item" },
    "/obj/item/card/id": { "parent": "/obj/item/card", "declared_vars": {
        "access": { "list": [] }, "slot_flags": 256 } },
    "/obj/item/card/id/advanced": { "parent": "/obj/item/card/id" },
    "/obj/item/clothing": { "parent": "/obj/item" },
    "/obj/item/clothing/under": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 2 } },
    "/obj/item/clothing/under/color": { "parent": "/obj/item/clothing/under" },
    "/obj/item/clothing/under/color/grey": { "parent": "/obj/item/clothing/under/color" },
    "/obj/item/clothing/shoes": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 128 } },
    "/obj/item/clothing/shoes/sneakers": { "parent": "/obj/item/clothing/shoes" },
    "/obj/item/clothing/shoes/sneakers/black": { "parent": "/obj/item/clothing/shoes/sneakers" },
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
    "/area": { "parent": "/atom" },
    "/area/space": { "parent": "/area" }
  }
}"##;

/// Rows top-down, the last line is y = 1. The spawn `s` (3,2) between a dispenser `D` (2,2) and
/// an extinguisher holder `H` (4,2).
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"D" = (/obj/structure/tank_dispenser,/turf/open/floor,/area/space)
"H" = (/obj/structure/tank_holder/extinguisher,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwww
wfffw
wDsHw
wwwww
"}
"##;

const DISPENSER: TilePos = TilePos::new(2, 2, 1);
const HOLDER: TilePos = TilePos::new(4, 2, 1);

const STATES: [(&str, &str); 13] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
    ("s.dmi", "dispenser"),
    ("s.dmi", "oxygen-4"),
    ("s.dmi", "plasma-5"),
    ("c.dmi", "holder"),
    ("c.dmi", "holder_extinguisher"),
    ("c.dmi", "holder_oxygen"),
    ("c.dmi", "holder_generic"),
    ("c.dmi", "holder_foam_extinguisher"),
];

fn fixture_assets() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("tank_storage_fixture");
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/structure/tank_dispenser",
        "/obj/structure/tank_holder",
        "/obj/structure/tank_holder/extinguisher",
        "/obj/item/screwdriver",
        "/obj/item/extinguisher",
        "/obj/item/tank/internals/oxygen",
        "/obj/item/tank/internals/plasma",
        "/obj/item/stack/rods",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
    ];
    write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)])
}

struct Station(testing::Station);

ss13_game::drive_station!(Station, 0);

impl Station {
    fn start() -> Self {
        let assets = fixture_assets();
        let mut s = testing::Station::start(fixture_map(&assets, &["map.dmm"]), |_| {});
        s.join_all(1, |_| {});
        Self(s)
    }

    fn mob(&self) -> Entity {
        self.players[0].mob
    }

    fn obj_on(&self, pos: TilePos, root: &str) -> Option<Entity> {
        let world = self.server.world();
        (world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied())
        .find(|&e| {
            world
                .get::<DmObj>(e)
                .is_some_and(|o| o.path.starts_with(root))
        })
    }

    fn items_on(&self, pos: TilePos, path: &str) -> usize {
        let world = self.server.world();
        (world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied())
        .filter(|&e| world.get::<DmItem>(e).is_some_and(|i| i.path == path))
        .count()
    }

    fn held(&self) -> Option<Entity> {
        let inventory = self.get::<Inventory>(self.mob());
        inventory.hands[inventory.active]
    }

    fn held_path(&self) -> Option<String> {
        self.held().map(|h| self.get::<DmItem>(h).path.clone())
    }

    /// A new `path` item straight into the active hand, the old one gone.
    fn give_new(&mut self, path: &str) -> Entity {
        let mob = self.mob();
        let old = self.held();
        let world = self.server.world_mut();
        if let Some(old) = old {
            world.despawn(old);
        }
        let item = ss13_game::construction::spawn_obj(world, path, TilePos::new(3, 3, 1)).unwrap();
        world
            .entity_mut(item)
            .remove::<TilePos>()
            .insert(CarriedBy(mob));
        let mut inventory = world.get_mut::<Inventory>(mob).unwrap();
        let hand = inventory.active;
        inventory.hands[hand] = Some(item);
        item
    }
}

fn right() -> ClickModifiers {
    ClickModifiers {
        right: true,
        ..Default::default()
    }
}

#[test]
fn take_and_return_dispenser_tanks() {
    let mut s = Station::start();
    let dispenser = s
        .obj_on(DISPENSER, "/obj/structure/tank_dispenser")
        .unwrap();
    assert_eq!(
        *s.get::<TankDispenser>(dispenser),
        TankDispenser {
            oxygen: 10,
            plasma: 10
        }
    );
    // An empty hand takes a plasma tank.
    s.run_for(1.0);
    s.click(0, dispenser);
    s.run_until("plasma tank taken", |s| s.held().is_some());
    assert_eq!(
        s.held_path().as_deref(),
        Some("/obj/item/tank/internals/plasma")
    );
    assert_eq!(s.get::<TankDispenser>(dispenser).plasma, 9);
    // It goes back in.
    s.run_for(1.0);
    s.click(0, dispenser);
    s.run_until("plasma tank returned", |s| s.held().is_none());
    assert_eq!(s.get::<TankDispenser>(dispenser).plasma, 10);
    // A right click takes an oxygen tank.
    s.run_for(1.0);
    s.click_with(0, dispenser, right());
    s.run_until("oxygen tank taken", |s| s.held().is_some());
    assert_eq!(
        s.held_path().as_deref(),
        Some("/obj/item/tank/internals/oxygen")
    );
    assert_eq!(s.get::<TankDispenser>(dispenser).oxygen, 9);
    // A full dispenser refuses another plasma tank.
    s.give_new("/obj/item/tank/internals/plasma");
    s.run_for(1.0);
    s.click(0, dispenser);
    s.run_for(1.0);
    assert!(s.held().is_some(), "a full dispenser keeps it out");
    assert_eq!(s.get::<TankDispenser>(dispenser).plasma, 10);
}

#[test]
fn holder_gives_and_takes_its_extinguisher() {
    let mut s = Station::start();
    let holder = s.obj_on(HOLDER, "/obj/structure/tank_holder").unwrap();
    let mapped = s
        .get::<TankHolder>(holder)
        .tank
        .expect("mapped extinguisher");
    assert!(
        s.try_get::<Density>(holder).is_some(),
        "a full holder is dense"
    );
    s.run_for(1.0);
    s.click(0, holder);
    s.run_until("extinguisher taken", |s| s.held() == Some(mapped));
    assert!(s.get::<TankHolder>(holder).tank.is_none());
    assert!(s.try_get::<Density>(holder).is_none());
    // The extinguisher goes back in.
    s.run_for(1.0);
    s.click(0, holder);
    s.run_until("extinguisher returned", |s| s.held().is_none());
    assert_eq!(s.get::<TankHolder>(holder).tank, Some(mapped));
    // Emptied again, a screwdriver takes it apart into 2 rods.
    s.run_for(1.0);
    s.click(0, holder);
    s.run_until("extinguisher taken again", |s| s.held() == Some(mapped));
    s.give_new("/obj/item/screwdriver");
    s.run_for(1.0);
    s.click(0, holder);
    s.run_until("taken apart", |s| {
        s.obj_on(HOLDER, "/obj/structure/tank_holder").is_none()
    });
    assert_eq!(s.items_on(HOLDER, "/obj/item/stack/rods"), 1);
}
