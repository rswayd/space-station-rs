//! Basic mobs and their AI on a fixture station over real WebTransport: a carp hunts and bites
//! a player and is punched back; a cornered mouse is killed and leaves its corpse; Ian and a
//! cat wander for minutes and stay healthy; station mobs always plan, others sleep until a
//! player is near; the carp migration event brings a school to the carp landmark.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::health::{Health, Stat};
use ss13_game::mobs::BasicMob;
use ss13_game::mobs::ai::AiStats;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::DmMob;
use ss13_shared::TilePos;
use ss13_shared::protocol::ClientIntent;

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
    "/obj/effect/landmark/carpspawn": { "parent": "/obj/effect/landmark" },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null, "attack_speed": 8 } },
    "/obj/item/food": { "parent": "/obj/item" },
    "/obj/item/food/deadmouse": { "parent": "/obj/item/food", "declared_vars": {
        "name": "dead mouse", "icon": { "resource": "m.dmi" }, "icon_state": "mouse_gray_dead" } },
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
        "habitable_atmos": { "list": [ { "assoc": ["min_oxy", 5] }, { "assoc": ["max_oxy", 0] },
            { "assoc": ["min_plas", 0] }, { "assoc": ["max_plas", 1] }, { "assoc": ["min_co2", 0] },
            { "assoc": ["max_co2", 5] }, { "assoc": ["min_n2", 0] }, { "assoc": ["max_n2", 0] } ] },
        "unsuitable_atmos_damage": 1, "minimum_survivable_temperature": 250,
        "maximum_survivable_temperature": 350, "unsuitable_cold_damage": 1,
        "unsuitable_heat_damage": 1, "melee_damage_type": "brute",
        "icon": { "resource": "m.dmi" } } },
    "/mob/living/basic/carp": { "parent": "/mob/living/basic", "declared_vars": {
        "name": "space carp", "maxHealth": 25, "health": 25, "habitable_atmos": null,
        "melee_damage_lower": 20, "melee_damage_upper": 20, "melee_attack_cooldown": 15,
        "attack_verb_continuous": "bites", "attack_verb_simple": "bite",
        "faction": { "expr": "list(FACTION_CARP)" }, "minimum_survivable_temperature": 0,
        "maximum_survivable_temperature": 1500, "pass_flags": 17,
        "ai_controller": { "path": "/datum/ai_controller/basic_controller/carp" },
        "icon_state": "base", "icon_living": "base", "icon_dead": "base_dead" } },
    "/mob/living/basic/carp/mega": { "parent": "/mob/living/basic/carp" },
    "/mob/living/basic/pet": { "parent": "/mob/living/basic", "declared_vars": {
        "melee_damage_lower": 5, "melee_damage_upper": 5 } },
    "/mob/living/basic/pet/dog": { "parent": "/mob/living/basic/pet", "declared_vars": {
        "ai_controller": { "path": "/datum/ai_controller/basic_controller/dog" },
        "response_help_continuous": "гладит", "response_help_simple": "гладите" } },
    "/mob/living/basic/pet/dog/corgi": { "parent": "/mob/living/basic/pet/dog", "declared_vars": {
        "ai_controller": { "path": "/datum/ai_controller/basic_controller/dog/corgi" },
        "icon_state": "corgi", "icon_living": "corgi", "icon_dead": "corgi_dead" } },
    "/mob/living/basic/pet/dog/corgi/ian": { "parent": "/mob/living/basic/pet/dog/corgi",
        "declared_vars": { "name": "Ian" } },
    "/mob/living/basic/pet/cat": { "parent": "/mob/living/basic/pet", "declared_vars": {
        "name": "cat", "ai_controller": { "path": "/datum/ai_controller/basic_controller/cat" },
        "icon_state": "cat2", "icon_living": "cat2", "icon_dead": "cat2_dead" } },
    "/mob/living/basic/mouse": { "parent": "/mob/living/basic", "declared_vars": {
        "name": "mouse", "maxHealth": 5, "health": 5, "density": 0,
        "faction": { "expr": "list(FACTION_RAT, FACTION_MAINT_CREATURES)" },
        "ai_controller": { "path": "/datum/ai_controller/basic_controller/mouse" },
        "death_sound": { "resource": "rat_death.ogg" },
        "icon_state": "mouse_gray", "body_icon_state": "mouse", "body_color": "gray" } },
    "/mob/living/basic/mouse/hamster": { "parent": "/mob/living/basic/mouse", "declared_vars": {
        "name": "хомяк", "icon_state": "mouse_gray", "body_icon_state": null, "body_color": "",
        "icon_living": "mouse_gray", "icon_dead": "mouse_gray_dead", "gender": "male" } },
    "/mob/living/basic/mouse/hamster/baby": { "parent": "/mob/living/basic/mouse/hamster",
        "declared_vars": { "name": "хомячок", "maxHealth": 3, "health": 3 } },
    "/obj/structure": { "parent": "/obj", "declared_vars": { "anchored": 1 } },
    "/obj/structure/fixture_table": { "parent": "/obj/structure", "declared_vars": {
        "name": "table", "icon": { "resource": "i.dmi" }, "icon_state": "toy", "density": 1,
        "pass_flags_self": 65 } },
    "/mob/living/basic/cockroach": { "parent": "/mob/living/basic", "declared_vars": {
        "name": "cockroach", "maxHealth": 1, "health": 1, "density": 0, "basic_mob_flags": 1,
        "ai_controller": { "path": "/datum/ai_controller/basic_controller/cockroach" },
        "faction": { "expr": "list(FACTION_HOSTILE, FACTION_MAINT_CREATURES)" },
        "icon_state": "cockroach" } },
    "/mob/living/basic/cow": { "parent": "/mob/living/basic", "declared_vars": {
        "name": "корова", "maxHealth": 50, "health": 50, "icon_state": "cow",
        "icon_living": "cow", "icon_dead": "cow_dead" } },
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
    "/area/station/mobs_test": { "parent": "/area/station" }
  }
}"##;

const FACTIONS: &str = "#define FACTION_NEUTRAL \"neutral\"\n#define FACTION_CARP \"carp\"\n\
#define FACTION_RAT \"rats\"\n#define FACTION_MAINT_CREATURES \"maint_creatures\"\n\
#define FACTION_HOSTILE \"hostile\"\n";

/// Rows top-down, the last line is y = 1. `s` the player (3,3); `C` a carp (10,3); `L` the
/// carp landmark (19,3).
const MAP: &str = r##""w" = (/turf/closed/wall,/area/station/mobs_test)
"f" = (/turf/open/floor,/area/station/mobs_test)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/station/mobs_test)
"C" = (/mob/living/basic/carp,/turf/open/floor,/area/station/mobs_test)
"L" = (/obj/effect/landmark/carpspawn,/turf/open/floor,/area/station/mobs_test)

(1,1,1) = {"
wwwwwwwwwwwwwwwwwwww
wffffffffffffffffffw
wfsffffffCffffffffLw
wffffffffffffffffffw
wwwwwwwwwwwwwwwwwwww
"}
"##;

/// `s` the player (2,2) next to `M`, a mouse in a pocket of walls (3,2).
const MOUSE_MAP: &str = r##""w" = (/turf/closed/wall,/area/station/mobs_test)
"f" = (/turf/open/floor,/area/station/mobs_test)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/station/mobs_test)
"M" = (/mob/living/basic/mouse,/turf/open/floor,/area/station/mobs_test)

(1,1,1) = {"
wwwwww
wfwwww
wsMwww
wwwwww
"}
"##;

/// A carp-free room for the pets, far from everyone (the player spawns on `s`).
const PETS_MAP: &str = r##""w" = (/turf/closed/wall,/area/station/mobs_test)
"f" = (/turf/open/floor,/area/station/mobs_test)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/station/mobs_test)
"I" = (/mob/living/basic/pet/dog/corgi/ian,/turf/open/floor,/area/station/mobs_test)
"K" = (/mob/living/basic/pet/cat,/turf/open/floor,/area/station/mobs_test)
"R" = (/mob/living/basic/cockroach,/turf/open/floor,/area/station/mobs_test)

(1,1,1) = {"
wwwwwwwwwwwwwwwwwwww
wsfffffffffffffffffw
wffffffffffffffffffw
wfffffffffIffffffffw
wffffffffffffKfffffw
wffffffffffffffffRfw
wffffffffffffffffffw
wwwwwwwwwwwwwwwwwwww
"}
"##;

/// The pets off the station (`/area/space`), within 10 tiles of the player's spot.
const WILD_MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"I" = (/mob/living/basic/pet/dog/corgi/ian,/turf/open/floor,/area/space)
"K" = (/mob/living/basic/pet/cat,/turf/open/floor,/area/space)
"R" = (/mob/living/basic/cockroach,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwwwwwww
wsfffffffw
wffIfKfRfw
wwwwwwwwww
"}
"##;

/// Two carp on one tile (2,2), a free tile, then a table (4,2).
const SWARM_MAP: &str = r##""w" = (/turf/closed/wall,/area/station/mobs_test)
"f" = (/turf/open/floor,/area/station/mobs_test)
"D" = (/mob/living/basic/carp,/mob/living/basic/carp,/turf/open/floor,/area/station/mobs_test)
"T" = (/obj/structure/fixture_table,/turf/open/floor,/area/station/mobs_test)

(1,1,1) = {"
wwwwww
wDfTfw
wwwwww
"}
"##;

/// `s` the player (2,2) next to `H`, a hamster pup walled in (3,2).
const PUP_MAP: &str = r##""w" = (/turf/closed/wall,/area/station/mobs_test)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/station/mobs_test)
"H" = (/mob/living/basic/mouse/hamster/baby,/turf/open/floor,/area/station/mobs_test)

(1,1,1) = {"
wwwwww
wsHwww
wwwwww
"}
"##;

/// `s` the player (2,2) next to `K`, a cow (3,2), room to ride east.
const COW_MAP: &str = r##""w" = (/turf/closed/wall,/area/station/mobs_test)
"f" = (/turf/open/floor,/area/station/mobs_test)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/station/mobs_test)
"K" = (/mob/living/basic/cow,/turf/open/floor,/area/station/mobs_test)

(1,1,1) = {"
wwwwwww
wsKfffw
wwwwwww
"}
"##;

const STATES: [(&str, &str); 16] = [
    ("m.dmi", "cow"),
    ("m.dmi", "cow_dead"),
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("m.dmi", "base"),
    ("m.dmi", "base_dead"),
    ("m.dmi", "corgi"),
    ("m.dmi", "corgi_dead"),
    ("m.dmi", "cat2"),
    ("m.dmi", "cat2_dead"),
    ("m.dmi", "mouse_gray"),
    ("m.dmi", "mouse_gray_dead"),
    ("m.dmi", "cockroach"),
    ("i.dmi", "toy"),
];

fn fixture(test: &str, map: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("mobs_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/effect/landmark/carpspawn",
        "/mob/living/basic/carp",
        "/mob/living/basic/carp/mega",
        "/mob/living/basic/mouse",
        "/mob/living/basic/pet/dog/corgi/ian",
        "/mob/living/basic/pet/cat",
        "/mob/living/basic/cockroach",
        "/mob/living/basic/mouse/hamster/baby",
        "/mob/living/basic/cow",
        "/obj/structure/fixture_table",
        "/obj/item/food/deadmouse",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/station/mobs_test",
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
            .query::<(Entity, &DmMob)>()
            .iter(world)
            .find(|(_, m)| m.path == path)
            .map(|(e, _)| e)
    }

    fn pos(&self, e: Entity) -> TilePos {
        *self.get::<TilePos>(e)
    }
}

#[test]
fn carp_bites_a_player_and_is_punched_back() {
    let mut s = Station::start("carp", MAP);
    let carp = s.mob_of("/mob/living/basic/carp").expect("carp");
    assert!(
        s.try_get::<BasicMob>(carp).is_some(),
        "the map carp is a basic mob"
    );
    assert_eq!(s.get::<Health>(carp).max, 25.0);
    let p = s.join("p0", |_| {});
    let me = s.players[p].mob;
    s.run_until_for(20.0, "the carp bites", |s| {
        s.get::<Health>(me).brute >= 20.0
    });
    assert!(
        s.pos(carp).chebyshev(s.pos(me)).is_some_and(|d| d <= 1),
        "it came to bite"
    );
    // Combat mode and punches until the carp is hurt.
    s.send(p, ClientIntent::ToggleCombatMode);
    s.run_for(0.5);
    for _ in 0..20 {
        if s.get::<Health>(carp).health() < 25.0 {
            break;
        }
        let carp_now = carp;
        s.click(p, carp_now);
        s.run_for(1.0);
    }
    let hp = s.get::<Health>(carp).health();
    assert!((5.0..25.0).contains(&hp), "punched for 5-10 per hit: {hp}");
}

#[test]
fn a_cornered_mouse_dies_and_leaves_its_corpse() {
    let mut s = Station::start("mouse", MOUSE_MAP);
    let mouse = s.mob_of("/mob/living/basic/mouse").expect("mouse");
    let at = s.pos(mouse);
    let p = s.join("p0", |_| {});
    let me = s.players[p].mob;
    assert_eq!(
        s.pos(me).chebyshev(at),
        Some(1),
        "the player spawns next to the mouse"
    );
    s.send(p, ClientIntent::ToggleCombatMode);
    s.run_for(0.5);
    assert_eq!(s.pos(mouse), at, "nowhere to flee");
    s.click(p, mouse);
    s.run_until("the mouse dies", |s| {
        s.0.server.world().get_entity(mouse).is_err()
    });
    let world = s.0.server.world_mut();
    let corpse = world
        .query::<(&ss13_mapload::DmItem, &TilePos, &Name)>()
        .iter(world)
        .find(|(i, ..)| i.path == "/obj/item/food/deadmouse")
        .map(|(_, p, n)| (*p, n.as_str().to_owned()));
    assert_eq!(corpse, Some((at, "mouse".to_owned())));
}

#[test]
fn pets_wander_for_minutes_and_stay_healthy() {
    let mut s = Station::start("pets", PETS_MAP);
    let ian = s
        .mob_of("/mob/living/basic/pet/dog/corgi/ian")
        .expect("Ian");
    let cat = s.mob_of("/mob/living/basic/pet/cat").expect("cat");
    let roach = s.mob_of("/mob/living/basic/cockroach").expect("cockroach");
    let (ian_at, cat_at) = (s.pos(ian), s.pos(cat));
    s.join("p0", |_| {});
    let mut ian_moved = false;
    let mut cat_moved = false;
    for _ in 0..120 {
        s.run_for(1.0);
        ian_moved |= s.pos(ian) != ian_at;
        cat_moved |= s.pos(cat) != cat_at;
    }
    assert!(ian_moved && cat_moved, "Ian {ian_moved}, cat {cat_moved}");
    for pet in [ian, cat] {
        assert_eq!(*s.get::<Stat>(pet), Stat::Conscious);
        assert_eq!(
            s.get::<Health>(pet).health(),
            20.0,
            "station air hurts nobody"
        );
    }
    // The cat hunts the cockroach (hostile faction) and kills it: it vanishes on death.
    s.run_until_for(120.0, "the cat kills the cockroach", |s| {
        s.0.server.world().get_entity(roach).is_err()
    });
}

#[test]
fn ai_runs_on_the_station_and_sleeps_off_it_until_a_player_comes() {
    // tg `get_active_ai_status`: station areas always plan.
    let mut s = Station::start("station_awake", PETS_MAP);
    s.run_for(1.0);
    let stats = *s.0.server.world().resource::<AiStats>();
    // The cat may already have caught the cockroach.
    assert!(
        stats.controllers >= 2 && stats.on == stats.controllers,
        "{stats:?}"
    );
    // Off the station nothing plans until a player is within 10 tiles.
    let mut s = Station::start("wild_sleep", WILD_MAP);
    s.run_for(3.0);
    let stats = *s.0.server.world().resource::<AiStats>();
    assert_eq!(stats.controllers, 3, "{stats:?}");
    assert_eq!((stats.on, stats.ticked), (0, 0), "no player: nothing plans");
    s.join("p0", |_| {});
    s.run_for(1.0);
    let stats = *s.0.server.world().resource::<AiStats>();
    assert_eq!(stats.on, 3, "a player nearby wakes them");
    assert!(stats.ticked >= 3);
}

#[test]
fn carp_migration_brings_a_school() {
    let mut s = Station::start("migration", MAP);
    s.join("p0", |_| {});
    ss13_game::events::force_event(s.0.server.world_mut(), "Carp Migration", Some(true)).unwrap();
    let landmark = TilePos::new(19, 3, 1);
    s.run_until_for(150.0, "a carp at the landmark", |s| {
        let world = s.0.server.world_mut();
        world
            .query::<(&DmMob, &TilePos)>()
            .iter(world)
            .any(|(m, p)| {
                m.path.starts_with("/mob/living/basic/carp")
                    && p.chebyshev(landmark).is_some_and(|d| d <= 3)
            })
    });
}

/// `SS13_ASSETS` must hold MetaStation assets converted with this version (`mobs.ron`).
/// Prints the basic mobs, how many plan, and the AI cost per pass, with the round-start mice
/// and a carp school (~30 mobs).
#[test]
#[ignore = "needs generated assets and MetaStation; run with --ignored --nocapture"]
fn metastation_mobs() {
    let map = ss13_mapload::MapLoadPlugin {
        source: std::env::var("SS13_SOURCE")
            .unwrap_or_else(|_| env!("SS13_SOURCE").into())
            .into(),
        maps: vec!["MetaStation".into()],
        assets: std::env::var("SS13_ASSETS")
            .unwrap_or_else(|_| "assets/generated".into())
            .into(),
    };
    let mut s = Station(testing::Station::start(map, |_| {}));
    let ian = s
        .mob_of("/mob/living/basic/pet/dog/corgi/ian")
        .expect("Ian is mapped");
    let fish = ss13_game::events::carp_migration::spawn_school(s.0.server.world_mut());
    let count = |s: &mut Station| {
        let world = s.0.server.world_mut();
        world.query::<&BasicMob>().iter(world).count()
    };
    let mobs = count(&mut s);
    println!("{mobs} basic mobs ({} carp from the landmarks)", fish.len());
    s.run_for(10.0);
    let idle = *s.0.server.world().resource::<AiStats>();
    println!(
        "no player: {} controllers, {} on, {:.1} µs per pass",
        idle.controllers,
        idle.on,
        idle.pass_nanos as f64 / idle.passes.max(1) as f64 / 1000.0
    );
    // Station mobs always plan (tg); migrating carp run unwatched (`RUN_WHILE_UNWATCHED`).
    assert!(idle.on >= fish.len(), "{idle:?}");
    s.join("p0", |_| {});
    let before = *s.0.server.world().resource::<AiStats>();
    let ian_at = s.pos(ian);
    s.run_for(60.0);
    let after = *s.0.server.world().resource::<AiStats>();
    let passes = after.passes - before.passes;
    println!(
        "one player: {} controllers, {} on, {:.1} µs per pass ({passes} passes), Ian {:?} -> {:?}",
        after.controllers,
        after.on,
        (after.pass_nanos - before.pass_nanos) as f64 / passes.max(1) as f64 / 1000.0,
        ian_at,
        s.pos(ian)
    );
    assert!(after.on > 0);
    assert_eq!(
        *s.get::<Stat>(ian),
        Stat::Conscious,
        "Ian survives a minute"
    );
}

/// tg `/datum/component/swarming` and BandaStation's carp `pass_flags`: two carp sharing a tile
/// spread by their offsets and gather back once apart; a carp swims over a table.
#[test]
fn carp_swarm_on_one_tile_and_cross_tables() {
    use ss13_game::mobs::swarming::Swarming;
    let mut s = Station::start("swarm", SWARM_MAP);
    let carps: Vec<Entity> = {
        let world = s.0.server.world_mut();
        world
            .query::<(Entity, &DmMob)>()
            .iter(world)
            .filter(|(_, m)| m.path == "/mob/living/basic/carp")
            .map(|(e, _)| e)
            .collect()
    };
    let [a, b] = carps[..] else {
        panic!("two carp: {carps:?}");
    };
    for carp in [a, b] {
        s.0.server
            .world_mut()
            .entity_mut(carp)
            .remove::<ss13_game::mobs::ai::AiController>();
    }
    s.run_for(0.2);
    assert_eq!(s.pos(a), s.pos(b), "still together");
    for carp in [a, b] {
        let swarm = *s.get::<Swarming>(carp);
        assert!(swarm.active, "two on one tile swarm");
        assert_eq!(
            s.get::<ss13_shared::Appearance>(carp).pixel_offset,
            swarm.offset
        );
        assert!(swarm.offset.iter().all(|o| (-20..=20).contains(o)));
    }
    let start = s.pos(a);
    let east = |p: TilePos| p.step(ss13_shared::Dir::East);
    let step = |s: &mut Station, dir| {
        s.0.server
            .world_mut()
            .write_message(ss13_world::MoveRequest { entity: a, dir });
        s.run_for(0.5);
    };
    step(&mut s, ss13_shared::Dir::East);
    assert_eq!(s.pos(a), east(start));
    for carp in [a, b] {
        assert!(!s.get::<Swarming>(carp).active, "alone again");
        assert_eq!(s.get::<ss13_shared::Appearance>(carp).pixel_offset, [0, 0]);
    }
    step(&mut s, ss13_shared::Dir::East);
    assert_eq!(s.pos(a), east(east(start)), "PASSTABLE crosses the table");
}

/// BandaStation hamster pup `on_entered`: a human stepping on it crushes it.
#[test]
fn a_hamster_pup_is_crushed_underfoot() {
    let mut s = Station::start("pup", PUP_MAP);
    let pup = s
        .mob_of("/mob/living/basic/mouse/hamster/baby")
        .expect("pup");
    let at = s.pos(pup);
    let p = s.join("p0", |_| {});
    let me = s.players[p].mob;
    s.run_until("the pup is crushed", |s| {
        s.try_step(p, ss13_shared::Dir::East);
        s.0.server.world().get_entity(pup).is_err()
    });
    assert_eq!(s.pos(me), at, "stepped onto it");
    let world = s.0.server.world_mut();
    let corpse = world
        .query::<(&ss13_mapload::DmItem, &TilePos)>()
        .iter(world)
        .any(|(i, p)| i.path == "/obj/item/food/deadmouse" && *p == at);
    assert!(corpse, "it leaves its corpse");
}

/// tg `riding/creature/cow`: a tamed cow is ridable; the rider drives it, its AI pauses, and
/// resisting gets off.
#[test]
fn a_tamed_cow_is_ridden() {
    use ss13_game::medical::surgery::Buckled;
    use ss13_game::mobs::ai::AiPaused;
    use ss13_game::vehicles::{Rider, Vehicle};
    use ss13_shared::Dir;
    use ss13_shared::protocol::DragEnd;

    let mut s = Station::start("cow", COW_MAP);
    let cow = s.mob_of("/mob/living/basic/cow").expect("cow");
    let p = s.join("p0", |_| {});
    let me = s.players[p].mob;
    let drag = |s: &mut Station| {
        let from = s.net_id(p, me).unwrap();
        let to = s.net_id(p, cow).unwrap();
        s.send(
            p,
            ClientIntent::Drag {
                from: DragEnd::Atom(from),
                to: DragEnd::Atom(to),
                modifiers: Default::default(),
            },
        );
    };
    // Untamed, it is no mount.
    drag(&mut s);
    s.run_for(0.5);
    assert!(
        s.try_get::<Buckled>(me).is_none(),
        "a wild cow is not ridden"
    );

    ss13_game::mobs::pets::tame(s.0.server.world_mut(), cow, me);
    let start = s.pos(cow);
    drag(&mut s);
    s.run_until("on the cow", |s| s.try_get::<Rider>(me).is_some());
    assert_eq!(s.pos(me), start);
    assert_eq!(s.get::<Vehicle>(cow).rider, Some(me));
    s.run_until("its AI pauses", |s| s.try_get::<AiPaused>(cow).is_some());

    let east = start.step(Dir::East);
    s.run_until("rode east", |s| {
        s.try_step(p, Dir::East);
        s.pos(cow) == east
    });
    s.run_until("the rider came along", |s| s.pos(me) == s.pos(cow));

    s.send(p, ClientIntent::Resist);
    s.run_until("off the cow", |s| s.try_get::<Buckled>(me).is_none());
    s.run_until("the cow is free again", |s| {
        s.get::<Vehicle>(cow).rider.is_none() && s.try_get::<AiPaused>(cow).is_none()
    });
}
