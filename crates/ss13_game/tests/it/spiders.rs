//! Spiders on a fixture station (`ss13_game::antag::spiders`): a ready midwife egg cluster
//! hatches for a ghost as a spider antagonist with its directive, the spiderling grows into a
//! young and then a giant broodmother keeping its player, the broodmother lays eggs carrying her
//! directive and wraps a human into a cocoon, eating it for an enriched egg.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::antag::spiders::{
    Cocoon, EggKind, Spider, SpiderAbilities, SpiderEggs, SpiderGrowth, SpiderJob, complete,
    lay_cluster, take_egg,
};
use ss13_game::closets::InCloset;
use ss13_game::construction::Job;
use ss13_game::ghost_verbs::GhostRoleSpawner;
use ss13_game::health::Stat;
use ss13_game::mobs::BasicMob;
use ss13_game::server::Players;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_game::tools::DoAfter;
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
    "/obj/structure": { "parent": "/obj", "declared_vars": { "density": 1, "anchored": 1 } },
    "/obj/structure/spider": { "parent": "/obj/structure", "declared_vars": {
        "density": 0, "icon": { "resource": "s.dmi" }, "icon_state": "eggs" } },
    "/obj/structure/spider/eggcluster": { "parent": "/obj/structure/spider", "declared_vars": {
        "name": "egg cluster" } },
    "/obj/structure/spider/eggcluster/midwife": { "parent": "/obj/structure/spider/eggcluster" },
    "/obj/structure/spider/eggcluster/enriched": { "parent": "/obj/structure/spider/eggcluster" },
    "/obj/structure/spider/cocoon": { "parent": "/obj/structure/spider", "declared_vars": {
        "name": "cocoon", "icon_state": "cocoon1" } },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "soulstone", "interaction_flags_item": 1,
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
        "basic_mob_flags": 0, "icon": { "resource": "c.dmi" } } },
    "/mob/living/basic/spider": { "parent": "/mob/living/basic", "declared_vars": {
        "name": "spider", "icon_state": "spider", "icon_living": "spider" } },
    "/mob/living/basic/spider/growing": { "parent": "/mob/living/basic/spider" },
    "/mob/living/basic/spider/growing/spiderling": { "parent": "/mob/living/basic/spider/growing",
        "declared_vars": { "name": "spiderling", "maxHealth": 5, "health": 5 } },
    "/mob/living/basic/spider/growing/spiderling/midwife": {
        "parent": "/mob/living/basic/spider/growing/spiderling" },
    "/mob/living/basic/spider/growing/young": { "parent": "/mob/living/basic/spider/growing",
        "declared_vars": { "name": "young spider", "maxHealth": 60, "health": 60 } },
    "/mob/living/basic/spider/growing/young/midwife": {
        "parent": "/mob/living/basic/spider/growing/young" },
    "/mob/living/basic/spider/giant": { "parent": "/mob/living/basic/spider", "declared_vars": {
        "name": "giant spider", "maxHealth": 200, "health": 200 } },
    "/mob/living/basic/spider/giant/midwife": { "parent": "/mob/living/basic/spider/giant",
        "declared_vars": { "name": "broodmother" } },
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
    "/area/station/spider_test": { "parent": "/area/station" }
  }
}"##;

const FACTIONS: &str = "#define FACTION_NEUTRAL \"neutral\"\n";

const STATES: [(&str, &str); 8] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "soulstone"),
    ("s.dmi", "eggs"),
    ("s.dmi", "cocoon1"),
    ("c.dmi", "spider"),
];

/// Two players side by side (2,3) and (3,3).
const MAP: &str = r##""w" = (/turf/closed/wall,/area/station/spider_test)
"f" = (/turf/open/floor,/area/station/spider_test)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/station/spider_test)

(1,1,1) = {"
wwwwww
wffffw
wssffw
wffffw
wwwwww
"}
"##;

fn fixture() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("spiders_fixture");
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/structure/spider/eggcluster",
        "/obj/structure/spider/eggcluster/midwife",
        "/obj/structure/spider/eggcluster/enriched",
        "/obj/structure/spider/cocoon",
        "/mob/living/basic/spider/growing/spiderling/midwife",
        "/mob/living/basic/spider/growing/young/midwife",
        "/mob/living/basic/spider/giant/midwife",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/station/spider_test",
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

const MIDWIFE: &str = "/mob/living/basic/spider/giant/midwife";

fn path_of(s: &Station, mob: Entity) -> String {
    s.0.server
        .world()
        .get::<BasicMob>(mob)
        .map(|b| b.def.path.clone())
        .unwrap_or_default()
}

#[test]
fn a_midwife_egg_hatches_grows_lays_and_wraps() {
    let mut s = Station(testing::Station::start(
        fixture_map(&fixture(), &["map.dmm"]),
        |_| {},
    ));
    let (a, b) = (s.join("p0", |_| {}), s.join("p1", |_| {}));
    let (client, human) = (s.players[a].link, s.players[b].mob);
    let body = s.players[a].mob;
    let pos = *s.get::<TilePos>(body);
    let egg = lay_cluster(
        s.0.server.world_mut(),
        EggKind::Midwife,
        pos,
        "Guard the nest.".to_owned(),
        98.0,
    )
    .unwrap();
    assert_eq!(s.get::<GhostRoleSpawner>(egg).uses, 0, "not ready");
    s.run_until_for(2.0, "the eggs are ready", |s| {
        s.get::<SpiderEggs>(egg).ready
    });
    assert_eq!(s.get::<GhostRoleSpawner>(egg).uses, 1);

    // The player's ghost takes the cluster: its only spiderling hatches for it.
    let ghost = {
        let world = s.0.server.world_mut();
        ss13_game::ghost::ghostize_client(world, client).unwrap()
    };
    take_egg(s.0.server.world_mut(), client, ghost, egg);
    let spider =
        s.0.server
            .world()
            .resource::<Players>()
            .mob(client)
            .unwrap();
    assert_eq!(
        path_of(&s, spider),
        "/mob/living/basic/spider/growing/spiderling/midwife"
    );
    assert_eq!(s.get::<Spider>(spider).directive, "Guard the nest.");
    assert!(
        s.0.server.world().get_entity(egg).is_err(),
        "the cluster is gone"
    );

    // It grows twice, the player and the antagonist along.
    let mut mob = spider;
    for _ in 0..2 {
        s.0.server
            .world_mut()
            .get_mut::<SpiderGrowth>(mob)
            .unwrap()
            .at = 0.0;
        s.run_for(0.2);
        mob =
            s.0.server
                .world()
                .resource::<Players>()
                .mob(client)
                .unwrap();
    }
    assert_eq!(path_of(&s, mob), MIDWIFE);
    assert!(s.try_get::<Spider>(mob).is_some());
    assert!(s.try_get::<SpiderGrowth>(mob).is_none());

    // The broodmother lays eggs with her directive.
    let world = s.0.server.world_mut();
    world
        .get_mut::<SpiderAbilities>(mob)
        .unwrap()
        .current_directive = "Eat the crew.".to_owned();
    let at = *world.get::<TilePos>(mob).unwrap();
    let action = |job| DoAfter {
        client,
        target: mob,
        target_pos: at,
        user_pos: at,
        item: None,
        amount: 0,
        ends_at: 0.0,
        job: Job::Spider(job),
    };
    complete(
        world,
        mob,
        &action(SpiderJob::Eggs(EggKind::Normal)),
        SpiderJob::Eggs(EggKind::Normal),
    );
    let laid: Vec<SpiderEggs> = world.query::<&SpiderEggs>().iter(world).cloned().collect();
    assert_eq!(laid.len(), 1);
    assert_eq!(
        (laid[0].kind, laid[0].directive.as_str()),
        (EggKind::Normal, "Eat the crew.")
    );

    // Wrapping a human: a cocoon, the human dead inside, one enriched egg.
    let wrap = DoAfter {
        target: human,
        ..action(SpiderJob::Wrap)
    };
    complete(world, mob, &wrap, SpiderJob::Wrap);
    let cocoon = world.get::<InCloset>(human).unwrap().closet;
    assert!(world.get::<Cocoon>(cocoon).is_some());
    assert_eq!(world.get::<Stat>(human), Some(&Stat::Dead));
    assert_eq!(
        world.get::<SpiderAbilities>(mob).unwrap().enriched_charges,
        1
    );
    // Broken open, it lets the body out.
    world.despawn(cocoon);
    world.flush();
    assert!(world.get::<InCloset>(human).is_none());
}
