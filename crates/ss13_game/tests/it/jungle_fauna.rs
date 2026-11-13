//! Jungle fauna on a fixture station (`ss13_game::mobs::jungle`): a leaper's bubble knocks a
//! player down; a venus human trap off its vines withers.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::health::Health;
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
    "/mob/living/basic/leaper": { "parent": "/mob/living/basic", "declared_vars": {
        "name": "leaper", "maxHealth": 350, "health": 350, "speed": 10,
        "melee_damage_lower": 15, "melee_damage_upper": 20,
        "faction": { "expr": "list(FACTION_JUNGLE)" },
        "ai_controller": { "path": "/datum/ai_controller/basic_controller/leaper" },
        "icon_state": "leaper", "icon_living": "leaper", "icon_dead": "leaper_dead" } },
    "/mob/living/basic/venus_human_trap": { "parent": "/mob/living/basic", "declared_vars": {
        "name": "venus human trap", "maxHealth": 50, "health": 50,
        "melee_damage_lower": 10, "melee_damage_upper": 20,
        "faction": { "expr": "list(FACTION_HOSTILE, FACTION_VINES, FACTION_PLANTS)" },
        "icon_state": "venus_human_trap", "icon_living": "venus_human_trap",
        "icon_dead": "venus_human_trap" } },
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
    "/area/station/jungle_test": { "parent": "/area/station" }
  }
}"##;

const FACTIONS: &str = "#define FACTION_NEUTRAL \"neutral\"\n#define FACTION_JUNGLE \"jungle\"\n\
#define FACTION_HOSTILE \"hostile\"\n#define FACTION_VINES \"vines\"\n\
#define FACTION_PLANTS \"plants\"\n";

const STATES: [(&str, &str); 11] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("m.dmi", "leaper"),
    ("m.dmi", "leaper_dead"),
    ("m.dmi", "venus_human_trap"),
    ("p.dmi", "leaper"),
    ("i.dmi", "toy"),
    ("m.dmi", "toy"),
    ("p.dmi", "toy"),
];

/// `s` the player (2,3), `L` a leaper five tiles east (7,3).
const LEAPER_MAP: &str = r##""w" = (/turf/closed/wall,/area/station/jungle_test)
"f" = (/turf/open/floor,/area/station/jungle_test)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/station/jungle_test)
"L" = (/mob/living/basic/leaper,/turf/open/floor,/area/station/jungle_test)

(1,1,1) = {"
wwwwwwwwwwww
wffffffffffw
wsffffLffffw
wffffffffffw
wwwwwwwwwwww
"}
"##;

/// `V` a venus human trap on a bare floor (no kudzu), the player out of its sight behind a wall.
const VENUS_MAP: &str = r##""w" = (/turf/closed/wall,/area/station/jungle_test)
"f" = (/turf/open/floor,/area/station/jungle_test)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/station/jungle_test)
"V" = (/mob/living/basic/venus_human_trap,/turf/open/floor,/area/station/jungle_test)

(1,1,1) = {"
wwwwwwwww
wsfwfffVw
wwwwwwwww
"}
"##;

fn fixture(test: &str, map: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("jungle_fauna_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/projectile/leaper",
        "/mob/living/basic/leaper",
        "/mob/living/basic/venus_human_trap",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/station/jungle_test",
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

    fn mob_of(&mut self, path: &str) -> Option<Entity> {
        let world = self.0.server.world_mut();
        world
            .query::<(Entity, &ss13_mapload::DmMob)>()
            .iter(world)
            .find(|(_, m)| m.path == path)
            .map(|(e, _)| e)
    }
}

#[test]
fn a_leaper_bubble_knocks_a_player_down() {
    let mut s = Station::start("leaper", LEAPER_MAP);
    let p = s.join("p0", |_| {});
    let me = s.players[p].mob;
    // The bubble is its first ability: 5 s of paralysis.
    s.run_until_for(10.0, "the bubble hits the player", |s| {
        ss13_game::status_effects::incapacitation(s.0.server.world(), me).incapacitated
    });
}

#[test]
fn a_venus_trap_off_its_vines_withers() {
    let mut s = Station::start("venus", VENUS_MAP);
    let trap = s
        .mob_of("/mob/living/basic/venus_human_trap")
        .expect("venus trap");
    // `Life` every 2 s: 12.5 brute away from kudzu.
    s.run_until_for(5.0, "the trap withers", |s| {
        s.get::<Health>(trap).brute >= 12.5
    });
}
