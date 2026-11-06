//! Mob holders on a fixture station (`ss13_game::mob_holder`): a mouse grabbed aggressively
//! and dragged onto its grabber is picked up after 2 s into a hand, rides hidden inside the
//! holder following its carrier, and is let out where the holder lands on the floor.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::closets::InCloset;
use ss13_game::inventory::{CarriedBy, Inventory};
use ss13_game::mob_holder::MobHolder;
use ss13_game::pull::{GrabState, PullSlowdown, Pulling};
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::DmMob;
use ss13_shared::TilePos;
use ss13_shared::protocol::{ClientIntent, DragEnd};

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3, "anchored": 0 } },
    "/obj": { "parent": "/atom/movable" },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start" },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null, "attack_speed": 8 } },
    "/obj/item/mob_holder": { "parent": "/obj/item", "declared_vars": {
        "name": "bugged mob", "icon": null, "icon_state": "", "slot_flags": 64 } },
    "/obj/item/mob_holder/snail": { "parent": "/obj/item/mob_holder" },
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
    "/mob": { "parent": "/atom/movable", "declared_vars": { "layer": 4, "density": 1,
        "faction": { "expr": "list(FACTION_NEUTRAL)" } } },
    "/mob/living": { "parent": "/mob", "declared_vars": { "maxHealth": 100, "health": 100,
        "held_state": "", "held_lh": { "resource": "pl.dmi" }, "held_rh": { "resource": "pr.dmi" },
        "head_icon": { "resource": "ph.dmi" }, "worn_slot_flags": 0 } },
    "/mob/living/carbon": { "parent": "/mob/living" },
    "/mob/living/carbon/human": { "parent": "/mob/living/carbon", "declared_vars": {
        "icon": { "resource": "h.dmi" }, "icon_state": "human_basic" } },
    "/mob/living/basic": { "parent": "/mob/living", "declared_vars": {
        "maxHealth": 20, "health": 20, "speed": 1, "melee_attack_cooldown": 20,
        "attack_verb_continuous": "атакует", "attack_verb_simple": "атакует",
        "response_help_continuous": "тычет", "response_help_simple": "тычет",
        "response_harm_continuous": "пихает", "response_harm_simple": "пихает",
        "unsuitable_atmos_damage": 0, "minimum_survivable_temperature": 0,
        "maximum_survivable_temperature": 100000, "melee_damage_type": "brute",
        "icon": { "resource": "m.dmi" } } },
    "/mob/living/basic/mouse": { "parent": "/mob/living/basic", "declared_vars": {
        "name": "mouse", "density": 0, "mob_size": 0, "held_state": "mouse_gray",
        "icon_state": "mouse_gray", "icon_living": "mouse_gray", "icon_dead": "mouse_gray_dead" } },
    "/turf": { "parent": "/atom", "declared_vars": { "icon": { "resource": "t.dmi" } } },
    "/turf/open": { "parent": "/turf" },
    "/turf/open/floor": { "parent": "/turf/open", "declared_vars": { "icon_state": "floor" } },
    "/turf/open/space": { "parent": "/turf/open" },
    "/turf/open/space/basic": { "parent": "/turf/open/space" },
    "/turf/closed": { "parent": "/turf", "declared_vars": { "density": 1, "opacity": 1 } },
    "/turf/closed/wall": { "parent": "/turf/closed", "declared_vars": { "icon_state": "wall" } },
    "/area": { "parent": "/atom" },
    "/area/space": { "parent": "/area" },
    "/area/station": { "parent": "/area" },
    "/area/station/holder_test": { "parent": "/area/station" }
  },
  "element_calls": {
    "/datum/element/can_be_held": [
      { "type": "/mob/living/basic/mouse", "proc": "Initialize", "call": "AddElement",
        "line": "AddElement(/datum/element/can_be_held)" }
    ]
  }
}"##;

const FACTIONS: &str = "#define FACTION_NEUTRAL \"neutral\"\n";

const STATES: [(&str, &str); 8] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("m.dmi", "mouse_gray"),
    ("m.dmi", "mouse_gray_dead"),
    ("i.dmi", "toy"),
    ("pl.dmi", "mouse_gray"),
];

/// `s` the player (2,3), `M` a mouse east of it (3,3).
const MAP: &str = r##""w" = (/turf/closed/wall,/area/station/holder_test)
"f" = (/turf/open/floor,/area/station/holder_test)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/station/holder_test)
"M" = (/mob/living/basic/mouse,/turf/open/floor,/area/station/holder_test)

(1,1,1) = {"
wwwwww
wffffw
wsMffw
wffffw
wwwwww
"}
"##;

fn fixture() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("mob_holder_fixture");
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/item/mob_holder",
        "/obj/item/mob_holder/snail",
        "/mob/living/basic/mouse",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/station/holder_test",
        "/area/space",
    ];
    let assets = write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)]);
    let types = ss13_mapload::convert::DmTypes::parse(TYPES.as_bytes()).unwrap();
    let defines: std::collections::HashMap<String, String> = FACTIONS
        .lines()
        .filter_map(|l| {
            let mut parts = l.split_whitespace().skip(1);
            Some((
                parts.next()?.to_owned(),
                parts.next()?.trim_matches('"').to_owned(),
            ))
        })
        .collect();
    let mobs = ss13_mapload::mobs::resolve(&types, &Default::default(), &defines);
    assert!(
        mobs.mobs["/mob/living/basic/mouse"].can_be_held,
        "the converter reads `can_be_held`"
    );
    std::fs::write(assets.join(ss13_mapload::mobs::MOBS_FILE), mobs.to_ron()).unwrap();
    assets
}

struct Station(testing::Station);
ss13_game::drive_station!(Station, 0);

#[test]
fn a_grabbed_mouse_is_picked_up_carried_and_let_out_on_the_floor() {
    let mut s = Station(testing::Station::start(
        fixture_map(&fixture(), &["map.dmm"]),
        |_| {},
    ));
    let p = s.join("p0", |_| {});
    let me = s.players[p].mob;
    let mouse = {
        let world = s.0.server.world_mut();
        world
            .query::<(Entity, &DmMob)>()
            .iter(world)
            .find(|(_, m)| m.path == "/mob/living/basic/mouse")
            .map(|(e, _)| e)
            .unwrap()
    };
    s.run_for(0.5);
    // Not grabbed: dragging it onto oneself does nothing.
    let (from, to) = (s.net_id(p, mouse).unwrap(), s.net_id(p, me).unwrap());
    let drag = ClientIntent::Drag {
        from: DragEnd::Atom(from),
        to: DragEnd::Atom(to),
        modifiers: default(),
    };
    s.send(p, drag.clone());
    s.run_for(3.0);
    assert!(s.try_get::<InCloset>(mouse).is_none());

    // Grabbed aggressively: 2 s, then the mouse is in a hand.
    s.0.server.world_mut().entity_mut(me).insert((
        Pulling(mouse),
        GrabState::Aggressive,
        PullSlowdown(0.0),
    ));
    s.send(p, drag);
    s.run_until_for(5.0, "the mouse is picked up", |s| {
        s.try_get::<InCloset>(mouse).is_some()
    });
    let holder = s.get::<InCloset>(mouse).closet;
    assert_eq!(s.get::<MobHolder>(holder).mob, mouse);
    assert_eq!(s.get::<CarriedBy>(holder).0, me);
    assert!(s.get::<Inventory>(me).holds(holder));
    assert!(s.try_get::<Pulling>(me).is_none(), "the grab ends");

    // It follows the carrier.
    let here = *s.get::<TilePos>(me);
    assert_eq!(*s.get::<TilePos>(mouse), here);

    // The holder on the floor (dropped): the mouse is out on that tile, the holder gone.
    let floor = TilePos::new(4, 3, 1);
    {
        let world = s.0.server.world_mut();
        world.entity_mut(holder).remove::<CarriedBy>().insert(floor);
    }
    s.run_until_for(2.0, "the mouse is let out", |s| {
        s.try_get::<InCloset>(mouse).is_none()
    });
    assert_eq!(*s.get::<TilePos>(mouse), floor);
    s.run_for(0.2);
    assert!(
        s.0.server.world().get_entity(holder).is_err(),
        "the holder is gone"
    );
    assert!(!s.get::<Inventory>(me).holds(holder));
}
