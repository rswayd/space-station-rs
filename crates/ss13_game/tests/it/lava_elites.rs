//! Lavaland elites on a fixture station (`ss13_game::mobs::megafauna::elites`): a human's touch
//! wakes the pulsing tumor and an elite emerges to fight them.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::mobs::megafauna::elites::{ELITE_ROOT, EliteChallenger};
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};

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
    "/obj/structure": { "parent": "/obj", "declared_vars": { "anchored": 1 } },
    "/obj/structure/elite_tumor": { "parent": "/obj/structure", "declared_vars": {
        "name": "pulsing tumor", "icon": { "resource": "o.dmi" }, "icon_state": "tumor" } },
    "/obj/projectile": { "parent": "/obj", "declared_vars": { "name": "projectile",
        "icon": { "resource": "p.dmi" }, "icon_state": "leaper", "anchored": 1, "damage": 0,
        "damage_type": "brute", "armor_flag": "bullet", "armour_penetration": 0, "stamina": 0,
        "knockdown": 0, "paralyze": 0, "stun": 0, "immobilize": 0, "speed": 1.25, "range": 50,
        "pass_flags": 1, "accurate_range": 100, "accuracy_falloff": 7, "ricochets_max": 0,
        "ricochet_chance": 0, "ricochet_decay_chance": 0.7, "ricochet_decay_damage": 0.7,
        "ricochet_incidence_leeway": 40, "wound_bonus": -100, "exposed_wound_bonus": 0,
        "wound_falloff_tile": 0, "sharpness": 0, "damage_falloff_tile": 0 } },
    "/obj/projectile/leaper": { "parent": "/obj/projectile", "declared_vars": {
        "name": "leaper bubble", "paralyze": 50, "range": 7 } },
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
    "/mob/living/simple_animal": { "parent": "/mob/living", "declared_vars": {
        "maxHealth": 20, "health": 20, "speed": 1, "melee_damage_type": "brute",
        "icon": { "resource": "m.dmi" } } },
    "/mob/living/simple_animal/hostile": { "parent": "/mob/living/simple_animal" },
    "/mob/living/simple_animal/hostile/asteroid": { "parent": "/mob/living/simple_animal/hostile",
        "declared_vars": { "faction": { "expr": "list(FACTION_MINING)" } } },
    "/mob/living/simple_animal/hostile/asteroid/elite": {
        "parent": "/mob/living/simple_animal/hostile/asteroid", "declared_vars": {
        "faction": { "expr": "list(FACTION_MINING, FACTION_BOSS)" } } },
    "/mob/living/simple_animal/hostile/asteroid/elite/broodmother": {
        "parent": "/mob/living/simple_animal/hostile/asteroid/elite", "declared_vars": {
        "name": "goliath broodmother", "maxHealth": 1000, "health": 1000,
        "melee_damage_lower": 30, "melee_damage_upper": 30, "icon_state": "elite",
        "icon_living": "elite", "icon_dead": "elite" } },
    "/mob/living/simple_animal/hostile/asteroid/elite/pandora": {
        "parent": "/mob/living/simple_animal/hostile/asteroid/elite", "declared_vars": {
        "name": "pandora", "maxHealth": 1000, "health": 1000, "icon_state": "elite",
        "icon_living": "elite", "icon_dead": "elite" } },
    "/mob/living/simple_animal/hostile/asteroid/elite/legionnaire": {
        "parent": "/mob/living/simple_animal/hostile/asteroid/elite", "declared_vars": {
        "name": "legionnaire", "maxHealth": 1000, "health": 1000, "icon_state": "elite",
        "icon_living": "elite", "icon_dead": "elite" } },
    "/mob/living/simple_animal/hostile/asteroid/elite/herald": {
        "parent": "/mob/living/simple_animal/hostile/asteroid/elite", "declared_vars": {
        "name": "herald", "maxHealth": 1000, "health": 1000, "icon_state": "elite",
        "icon_living": "elite", "icon_dead": "elite" } },
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
    "/area/station/elite_test": { "parent": "/area/station" }
  }
}"##;

const FACTIONS: &str = "#define FACTION_NEUTRAL \"neutral\"\n#define FACTION_MINING \"mining\"\n\
#define FACTION_BOSS \"boss\"\n";

const STATES: [(&str, &str); 9] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("m.dmi", "elite"),
    ("o.dmi", "tumor"),
    ("i.dmi", "toy"),
    ("m.dmi", "toy"),
    ("p.dmi", "leaper"),
];

/// `s` the player (2,3), `T` the tumor next to them (3,3).
const TUMOR_MAP: &str = r##""w" = (/turf/closed/wall,/area/station/elite_test)
"f" = (/turf/open/floor,/area/station/elite_test)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/station/elite_test)
"T" = (/obj/structure/elite_tumor,/turf/open/floor,/area/station/elite_test)

(1,1,1) = {"
wwwwwwwwwwww
wffffffffffw
wsTffffffffw
wffffffffffw
wwwwwwwwwwww
"}
"##;

fn fixture(test: &str, map: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("lava_elites_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/structure/elite_tumor",
        "/mob/living/simple_animal/hostile/asteroid/elite/broodmother",
        "/mob/living/simple_animal/hostile/asteroid/elite/pandora",
        "/mob/living/simple_animal/hostile/asteroid/elite/legionnaire",
        "/mob/living/simple_animal/hostile/asteroid/elite/herald",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/station/elite_test",
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

    fn of_path(&mut self, root: &str) -> Option<Entity> {
        let world = self.0.server.world_mut();
        world
            .query::<(Entity, &ss13_mapload::DmMob)>()
            .iter(world)
            .find(|(_, m)| ss13_mapload::fidelity::is_under(&m.path, root))
            .map(|(e, _)| e)
    }

    fn tumor(&mut self) -> Entity {
        let world = self.0.server.world_mut();
        world
            .query::<(Entity, &ss13_mapload::DmObj)>()
            .iter(world)
            .find(|(_, o)| o.path == "/obj/structure/elite_tumor")
            .map(|(e, _)| e)
            .expect("the tumor")
    }
}

#[test]
fn a_touched_tumor_summons_an_elite_against_its_challenger() {
    let mut s = Station::start("tumor", TUMOR_MAP);
    let p = s.join("p0", |_| {});
    let me = s.players[p].mob;
    let tumor = s.tumor();
    s.run_until("the tumor is seen", |s| s.net_id(p, tumor).is_some());
    s.click(p, tumor);
    s.run_until("the player challenges the tumor", |s| {
        s.0.server.world().get::<EliteChallenger>(me).is_some()
    });
    // The elite emerges 3 s later.
    s.run_until_for(6.0, "an elite emerges", |s| s.of_path(ELITE_ROOT).is_some());
}
