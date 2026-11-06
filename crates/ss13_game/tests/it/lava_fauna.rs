//! Lavaland fauna on a fixture station (`ss13_game::mobs::lavaland`): a goliath digs its
//! tentacles under a player and grabs them; a legion launches a skull that lands next to the
//! player and bites.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::health::Health;
use ss13_game::mobs::lavaland::abilities::Grabbed;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::DmMob;

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
    "/mob/living": { "parent": "/mob", "declared_vars": { "maxHealth": 100, "health": 100 } },
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
    "/mob/living/basic/mining": { "parent": "/mob/living/basic", "declared_vars": {
        "faction": { "expr": "list(FACTION_MINING, FACTION_ASHWALKER)" } } },
    "/mob/living/basic/mining/goliath": { "parent": "/mob/living/basic/mining", "declared_vars": {
        "name": "goliath", "maxHealth": 300, "health": 300, "speed": 12,
        "melee_damage_lower": 25, "melee_damage_upper": 25,
        "ai_controller": { "path": "/datum/ai_controller/basic_controller/goliath" },
        "icon_state": "goliath", "icon_living": "goliath", "icon_dead": "goliath_dead" } },
    "/mob/living/basic/mining/legion": { "parent": "/mob/living/basic/mining", "declared_vars": {
        "name": "legion", "maxHealth": 75, "health": 75, "speed": 3, "basic_mob_flags": 1,
        "melee_damage_lower": 15, "melee_damage_upper": 15,
        "ai_controller": { "path": "/datum/ai_controller/basic_controller/legion" },
        "icon_state": "legion", "icon_living": "legion", "icon_dead": "legion" } },
    "/mob/living/basic/mining/legion_brood": { "parent": "/mob/living/basic/mining",
        "declared_vars": {
        "name": "legion", "maxHealth": 1, "health": 1, "speed": 1, "basic_mob_flags": 1,
        "density": 0, "melee_damage_lower": 12, "melee_damage_upper": 12,
        "faction": { "expr": "list(FACTION_MINING)" },
        "ai_controller": { "path": "/datum/ai_controller/basic_controller/legion_brood" },
        "icon_state": "legion_head", "icon_living": "legion_head", "icon_dead": "legion_head" } },
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
    "/area/station/lava_test": { "parent": "/area/station" }
  }
}"##;

const FACTIONS: &str = "#define FACTION_NEUTRAL \"neutral\"\n#define FACTION_MINING \"mining\"\n\
#define FACTION_ASHWALKER \"ashwalker\"\n";

const STATES: [(&str, &str); 10] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("m.dmi", "goliath"),
    ("m.dmi", "goliath_dead"),
    ("m.dmi", "legion"),
    ("m.dmi", "legion_head"),
    ("i.dmi", "toy"),
    ("m.dmi", "toy"),
];

/// `s` the player (2,3), `G` a goliath five tiles east (7,3).
const GOLIATH_MAP: &str = r##""w" = (/turf/closed/wall,/area/station/lava_test)
"f" = (/turf/open/floor,/area/station/lava_test)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/station/lava_test)
"G" = (/mob/living/basic/mining/goliath,/turf/open/floor,/area/station/lava_test)

(1,1,1) = {"
wwwwwwwwwwww
wffffffffffw
wsffffGffffw
wffffffffffw
wwwwwwwwwwww
"}
"##;

/// `s` the player (2,3), `L` a legion four tiles east (6,3).
const LEGION_MAP: &str = r##""w" = (/turf/closed/wall,/area/station/lava_test)
"f" = (/turf/open/floor,/area/station/lava_test)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/station/lava_test)
"L" = (/mob/living/basic/mining/legion,/turf/open/floor,/area/station/lava_test)

(1,1,1) = {"
wwwwwwwwwwww
wffffffffffw
wsfffLfffffw
wffffffffffw
wwwwwwwwwwww
"}
"##;

fn fixture(test: &str, map: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("lava_fauna_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/mob/living/basic/mining/goliath",
        "/mob/living/basic/mining/legion",
        "/mob/living/basic/mining/legion_brood",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/station/lava_test",
        "/area/space",
    ];
    let assets = write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", map)]);
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
    std::fs::write(assets.join(ss13_mapload::mobs::MOBS_FILE), mobs.to_ron()).unwrap();
    assets
}

struct Station(testing::Station);
ss13_game::drive_station!(Station, 0);

impl Station {
    fn start(test: &str, map: &str) -> Self {
        let assets = fixture(test, map);
        Self(testing::Station::start(
            fixture_map(&assets, &["map.dmm"]),
            |_| {},
        ))
    }

    fn mobs_of(&mut self, path: &str) -> Vec<Entity> {
        let world = self.0.server.world_mut();
        world
            .query::<(Entity, &DmMob)>()
            .iter(world)
            .filter(|(_, m)| m.path == path)
            .map(|(e, _)| e)
            .collect()
    }
}

#[test]
fn a_goliath_grabs_a_player_with_its_tentacles() {
    let mut s = Station::start("goliath", GOLIATH_MAP);
    assert_eq!(s.mobs_of("/mob/living/basic/mining/goliath").len(), 1);
    let p = s.join("p0", |_| {});
    let me = s.players[p].mob;
    // 3 s on the target, then 1 s for the tentacle to rise and grab.
    s.run_until_for(15.0, "a tentacle grabs the player", |s| {
        s.try_get::<Grabbed>(me).is_some()
    });
    let brute = s.get::<Health>(me).brute;
    assert!(brute >= 10.0, "the grab hurts the leg: {brute}");
    assert!(
        ss13_game::status_effects::incapacitation(s.0.server.world(), me).immobilized,
        "held in place"
    );
}

#[test]
fn a_legion_launches_a_skull_that_bites() {
    let mut s = Station::start("legion", LEGION_MAP);
    let p = s.join("p0", |_| {});
    let me = s.players[p].mob;
    s.run_until_for(10.0, "a skull lands", |s| {
        !s.mobs_of("/mob/living/basic/mining/legion_brood")
            .is_empty()
    });
    s.run_until_for(10.0, "the skull bites", |s| {
        s.get::<Health>(me).brute >= 12.0
    });
    // Skulls live 10 s.
    s.run_for(12.0);
    let left = s.mobs_of("/mob/living/basic/mining/legion_brood").len();
    assert!(left <= 3, "old skulls die off: {left}");
}
