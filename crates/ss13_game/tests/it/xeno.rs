//! Xenomorphs on a fixture station (`ss13_game::antag::xeno`): a facehugger implants an embryo in
//! the human it leaps at, a weed node grows weeds, a larva evolves and keeps its player.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::antag::xeno::reproduction::{self, AlienEmbryo, FACEHUGGER, WEED_NODE, Weed};
use ss13_game::antag::xeno::{self, Caste, LARVA, Xeno};
use ss13_game::inventory::Inventory;
use ss13_game::server::{PlayerMob, Players};
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_shared::TilePos;
use ss13_shared::protocol::InventorySlot;

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3, "anchored": 0 } },
    "/obj": { "parent": "/atom/movable", "declared_vars": { "obj_flags": 2 } },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start" },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null, "attack_speed": 8 } },
    "/obj/item/card": { "parent": "/obj/item" },
    "/obj/item/card/id": { "parent": "/obj/item/card", "declared_vars": { "access": { "list": [] } } },
    "/obj/item/card/id/advanced": { "parent": "/obj/item/card/id" },
    "/obj/item/clothing": { "parent": "/obj/item" },
    "/obj/item/clothing/under": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 2 } },
    "/obj/item/clothing/under/color": { "parent": "/obj/item/clothing/under" },
    "/obj/item/clothing/under/color/grey": { "parent": "/obj/item/clothing/under/color" },
    "/obj/item/clothing/shoes": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 128 } },
    "/obj/item/clothing/shoes/sneakers": { "parent": "/obj/item/clothing/shoes" },
    "/obj/item/clothing/shoes/sneakers/black": { "parent": "/obj/item/clothing/shoes/sneakers" },
    "/obj/item/clothing/mask": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 16 } },
    "/obj/item/clothing/mask/facehugger": { "parent": "/obj/item/clothing/mask", "declared_vars": {
        "name": "alien", "icon": { "resource": "a.dmi" }, "icon_state": "facehugger" } },
    "/obj/structure": { "parent": "/obj", "declared_vars": { "anchored": 1 } },
    "/obj/structure/alien": { "parent": "/obj/structure", "declared_vars": {
        "icon": { "resource": "a.dmi" }, "max_integrity": 100 } },
    "/obj/structure/alien/weeds": { "parent": "/obj/structure/alien", "declared_vars": {
        "name": "resin floor", "icon_state": "weeds", "max_integrity": 15 } },
    "/obj/structure/alien/weeds/node": { "parent": "/obj/structure/alien/weeds", "declared_vars": {
        "name": "glowing resin", "icon_state": "weednode" } },
    "/mob": { "parent": "/atom/movable", "declared_vars": { "layer": 4, "density": 1 } },
    "/mob/living": { "parent": "/mob", "declared_vars": { "maxHealth": 100, "health": 100 } },
    "/mob/living/carbon": { "parent": "/mob/living" },
    "/mob/living/carbon/human": { "parent": "/mob/living/carbon", "declared_vars": {
        "icon": { "resource": "h.dmi" }, "icon_state": "human_basic" } },
    "/mob/living/carbon/alien": { "parent": "/mob/living/carbon", "declared_vars": {
        "name": "alien", "icon": { "resource": "a.dmi" } } },
    "/mob/living/carbon/alien/larva": { "parent": "/mob/living/carbon/alien", "declared_vars": {
        "name": "alien larva", "icon_state": "larva0", "maxHealth": 25, "health": 25, "density": 0 } },
    "/mob/living/carbon/alien/adult": { "parent": "/mob/living/carbon/alien", "declared_vars": {
        "melee_damage_lower": 20, "melee_damage_upper": 20 } },
    "/mob/living/carbon/alien/adult/drone": { "parent": "/mob/living/carbon/alien/adult",
        "declared_vars": { "name": "alien drone", "icon_state": "aliend", "maxHealth": 125,
        "health": 125 } },
    "/turf": { "parent": "/atom", "declared_vars": { "icon": { "resource": "t.dmi" } } },
    "/turf/open": { "parent": "/turf" },
    "/turf/open/floor": { "parent": "/turf/open", "declared_vars": { "icon_state": "floor" } },
    "/turf/closed": { "parent": "/turf", "declared_vars": { "density": 1, "opacity": 1 } },
    "/turf/closed/wall": { "parent": "/turf/closed", "declared_vars": { "icon_state": "wall" } },
    "/area": { "parent": "/atom", "declared_vars": { "area_flags": 0, "requires_power": 0 } },
    "/area/space": { "parent": "/area" },
    "/area/station": { "parent": "/area" },
    "/area/station/xeno_test": { "parent": "/area/station", "declared_vars": { "area_flags": 2 } }
  }
}"##;

const STATES: [(&str, &str); 12] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
    ("a.dmi", "facehugger"),
    ("a.dmi", "weeds"),
    ("a.dmi", "weednode"),
    ("a.dmi", "larva0"),
    ("a.dmi", "larva0_dead"),
    ("a.dmi", "aliend"),
    ("a.dmi", "aliend_dead"),
];

const MAP: &str = r##""w" = (/turf/closed/wall,/area/station/xeno_test)
"f" = (/turf/open/floor,/area/station/xeno_test)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/station/xeno_test)

(1,1,1) = {"
wwwwwwwwwwwwww
wffffffffffffw
wffffffffffffw
wsfffffffffffw
wffffffffffffw
wffffffffffffw
wwwwwwwwwwwwww
"}
"##;

const NODE_AT: TilePos = TilePos::new(7, 3, 1);

fn fixture(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("xeno_fixture")
        .join(test);
    let used = vec![
        "/obj/effect/landmark/start/assistant",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/area/station/xeno_test",
        "/area/space",
        "/mob/living/carbon/human",
        "/obj/item/card/id/advanced",
        "/obj/item/clothing/under/color/grey",
        "/obj/item/clothing/shoes/sneakers/black",
        "/obj/item/clothing/mask/facehugger",
        "/obj/structure/alien/weeds",
        "/obj/structure/alien/weeds/node",
        "/mob/living/carbon/alien/larva",
        "/mob/living/carbon/alien/adult/drone",
    ];
    let assets = write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)]);
    let parsed = ss13_mapload::convert::DmTypes::parse(TYPES.as_bytes()).unwrap();
    let mobs = ss13_mapload::mobs::resolve(&parsed, &Default::default(), &Default::default());
    std::fs::write(assets.join(ss13_mapload::mobs::MOBS_FILE), mobs.to_ron()).unwrap();
    assets
}

struct Station(testing::Station);
ss13_game::drive_station!(Station, 0);

impl Station {
    fn start(test: &str) -> Self {
        let mut s = Self(testing::Station::start(
            fixture_map(&fixture(test), &["map.dmm"]),
            |_| {},
        ));
        s.run_for(0.5);
        s
    }

    fn world(&mut self) -> &mut World {
        self.0.server.world_mut()
    }
}

#[test]
fn a_facehugger_implants_an_embryo() {
    let mut s = Station::start("hugger");
    let p = s.join("p0", |_| {});
    let me = s.players[p].mob;
    let at = *s.get::<TilePos>(me);
    let world = s.world();
    let hugger = ss13_game::construction::spawn_obj(world, FACEHUGGER, at).expect("a hugger");
    world.flush();
    assert!(reproduction::leap_to(world, hugger, me));
    let mask = world
        .get::<Inventory>(me)
        .unwrap()
        .slot(InventorySlot::Mask);
    assert_eq!(mask, Some(hugger), "on the face");
    assert!(world.get::<AlienEmbryo>(me).is_none(), "not at once");
    // `MAX_IMPREGNATION_TIME` is 15 s.
    s.run_for(16.0);
    let embryo = s.world().get::<AlienEmbryo>(me).copied();
    assert_eq!(embryo.map(|e| e.stage), Some(1));
    let state = s
        .world()
        .get::<reproduction::FaceHugger>(hugger)
        .unwrap()
        .state;
    assert_eq!(state, reproduction::HuggerState::Dead, "spent");
}

#[test]
fn weeds_grow_from_a_node() {
    let mut s = Station::start("weeds");
    let world = s.world();
    ss13_game::construction::spawn_obj(world, WEED_NODE, NODE_AT).expect("a node");
    world.flush();
    // `maximum_growtime` is 10 s; the tile tick is a second.
    s.run_for(12.0);
    let world = s.world();
    let weeds = world.query::<&Weed>().iter(world).count();
    assert!(weeds > 1, "only {weeds} weeds");
}

#[test]
fn a_larva_evolves_and_keeps_its_player() {
    let mut s = Station::start("evolve");
    let p = s.join("p0", |_| {});
    let (link, me) = (s.players[p].link, s.players[p].mob);
    let at = *s.get::<TilePos>(me);
    let world = s.world();
    let larva = ss13_game::mobs::spawn::spawn_mob(world, LARVA, at).expect("a larva");
    assert!(ss13_game::medical::brainmob::transfer_mind(
        world, me, larva
    ));
    xeno::make_xeno(world, larva, true);
    let identifier = world.get::<Xeno>(larva).unwrap().identifier;
    let drone = xeno::evolve(world, larva, Caste::Drone).expect("a drone");
    s.run_for(0.5);
    let world = s.world();
    let x = world.get::<Xeno>(drone).unwrap();
    assert_eq!((x.caste, x.identifier), (Caste::Drone, identifier));
    assert_eq!(x.plasma, Caste::Drone.vessel().stored);
    assert_eq!(world.get::<PlayerMob>(drone).map(|p| p.client), Some(link));
    assert_eq!(world.resource::<Players>().mob(link), Some(drone));
    assert!(world.get_entity(larva).is_err(), "the larva is gone");
}
