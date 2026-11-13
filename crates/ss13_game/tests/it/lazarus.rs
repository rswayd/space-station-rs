//! The lazarus injector (`ss13_game::mining::devices`): a dead goliath comes back friendly to
//! whoever injected it, the injector is spent; a living or boss creature is refused.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::health::{Health, Stat};
use ss13_game::mining::devices::{LazarusInjector, item_interaction};
use ss13_game::mobs::Faction;
use ss13_game::mobs::pets::Friends;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::DmMob;
use ss13_shared::TilePos;

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
        "icon_state": "goliath", "icon_living": "goliath", "icon_dead": "goliath_dead" } },
    "/mob/living/basic/mining/boss": { "parent": "/mob/living/basic/mining", "declared_vars": {
        "name": "boss", "sentience_type": 5,
        "icon_state": "goliath", "icon_living": "goliath", "icon_dead": "goliath_dead" } },
    "/obj/item/lazarus_injector": { "parent": "/obj/item", "declared_vars": {
        "name": "lazarus injector", "icon_state": "toy" } },
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

const STATES: [(&str, &str); 7] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("m.dmi", "goliath"),
    ("m.dmi", "goliath_dead"),
    ("i.dmi", "toy"),
];

/// `s` the player (2,3), `G` a goliath (3,3), `B` a boss (3,2).
const MAP: &str = r##""w" = (/turf/closed/wall,/area/station/lava_test)
"f" = (/turf/open/floor,/area/station/lava_test)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/station/lava_test)
"G" = (/mob/living/basic/mining/goliath,/turf/open/floor,/area/station/lava_test)
"B" = (/mob/living/basic/mining/boss,/turf/open/floor,/area/station/lava_test)

(1,1,1) = {"
wwwwww
wffffw
wsGffw
wfBffw
wwwwww
"}
"##;

fn fixture() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("lazarus_fixture");
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/mob/living/basic/mining/goliath",
        "/mob/living/basic/mining/boss",
        "/obj/item/lazarus_injector",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/station/lava_test",
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
    std::fs::write(assets.join(ss13_mapload::mobs::MOBS_FILE), mobs.to_ron()).unwrap();
    assets
}

struct Station(testing::Station);
ss13_game::drive_station!(Station, 0);

impl Station {
    fn mob_of(&mut self, path: &str) -> Entity {
        let world = self.0.server.world_mut();
        world
            .query::<(Entity, &DmMob)>()
            .iter(world)
            .find(|(_, m)| m.path == path)
            .map(|(e, _)| e)
            .unwrap_or_else(|| panic!("no {path}"))
    }

    fn kill(&mut self, mob: Entity) {
        let world = self.0.server.world_mut();
        let mut health = world.get::<Health>(mob).unwrap().clone();
        health.adjust(ss13_game::health::DamageType::Brute, 10_000.0);
        world.entity_mut(mob).insert(health);
        self.run_until_for(2.0, "dead", |s| *s.get::<Stat>(mob) == Stat::Dead);
    }
}

#[test]
fn a_lazarus_injector_revives_a_dead_goliath_as_a_friend() {
    let mut s = Station(testing::Station::start(
        fixture_map(&fixture(), &["map.dmm"]),
        |_| {},
    ));
    let p = s.join("p0", |_| {});
    let me = s.players[p].mob;
    let goliath = s.mob_of("/mob/living/basic/mining/goliath");
    let boss = s.mob_of("/mob/living/basic/mining/boss");
    let injector = ss13_game::construction::spawn_obj(
        s.0.server.world_mut(),
        "/obj/item/lazarus_injector",
        TilePos::new(4, 3, 1),
    )
    .unwrap();
    s.run_for(0.2);
    assert!(s.get::<LazarusInjector>(injector).loaded);

    // Alive: refused, still loaded.
    assert!(item_interaction(
        s.0.server.world_mut(),
        me,
        injector,
        goliath
    ));
    assert!(s.get::<LazarusInjector>(injector).loaded);

    // A boss is no organic creature.
    s.kill(boss);
    assert!(item_interaction(s.0.server.world_mut(), me, injector, boss));
    assert_eq!(*s.get::<Stat>(boss), Stat::Dead);

    s.kill(goliath);
    assert!(item_interaction(
        s.0.server.world_mut(),
        me,
        injector,
        goliath
    ));
    assert_eq!(*s.get::<Stat>(goliath), Stat::Conscious);
    assert_eq!(s.get::<Health>(goliath).brute, 0.0);
    assert!(s.get::<Friends>(goliath).0.contains(&me));
    assert!(s.get::<Faction>(goliath).0.iter().any(|f| f == "neutral"));
    assert!(!s.get::<LazarusInjector>(injector).loaded, "spent");
    // Spent: it does nothing more.
    s.kill(goliath);
    assert!(!item_interaction(
        s.0.server.world_mut(),
        me,
        injector,
        goliath
    ));
}
