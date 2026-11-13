//! Guardians on a fixture room: a player uses a dusty shard (a random type), a ghost signs up
//! and becomes the guardian hidden inside them; it manifests, the damage it takes goes to its
//! summoner, it is pulled back past its range, and it recalls.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::closets::InCloset;
use ss13_game::ghost::ghostize_client;
use ss13_game::ghost_poll::finish_all;
use ss13_game::health::{DamageType, Health};
use ss13_game::mobs::guardian::{self, Guardian};
use ss13_game::server::Players;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_shared::TilePos;
use ss13_tgui::prompt::{Prompt, PromptAnswer, PromptAnswered};

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
        "slot_flags": 0, "inhand_icon_state": null, "attack_speed": 8,
        "tool_behaviour": null, "toolspeed": 1, "heat": 0 } },
    "/obj/item/guardian_creator": { "parent": "/obj/item", "declared_vars": {
        "name": "enchanted deck of tarot cards" } },
    "/obj/item/guardian_creator/miner": { "parent": "/obj/item/guardian_creator",
        "declared_vars": { "name": "dusty shard" } },
    "/obj/item/card": { "parent": "/obj/item" },
    "/obj/item/card/id": { "parent": "/obj/item/card", "declared_vars": { "slot_flags": 256,
        "access": { "list": [] } } },
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
        "unsuitable_atmos_damage": 1, "minimum_survivable_temperature": 250,
        "maximum_survivable_temperature": 350, "unsuitable_cold_damage": 1,
        "unsuitable_heat_damage": 1, "melee_damage_type": "brute",
        "icon": { "resource": "m.dmi" } } },
    "/mob/living/basic/guardian": { "parent": "/mob/living/basic", "declared_vars": {
        "name": "Guardian Spirit", "icon": { "resource": "g.dmi" }, "icon_state": "magicbase",
        "basic_mob_flags": 1, "speed": 0, "unsuitable_atmos_damage": 0,
        "minimum_survivable_temperature": 0, "maximum_survivable_temperature": 1e31,
        "melee_damage_lower": 15, "melee_damage_upper": 15 } },
    "/mob/living/basic/guardian/charger": { "parent": "/mob/living/basic/guardian" },
    "/mob/living/basic/guardian/protector": { "parent": "/mob/living/basic/guardian" },
    "/mob/living/basic/guardian/ranged": { "parent": "/mob/living/basic/guardian" },
    "/mob/living/basic/guardian/standard": { "parent": "/mob/living/basic/guardian" },
    "/mob/living/basic/guardian/support": { "parent": "/mob/living/basic/guardian" },
    "/turf": { "parent": "/atom", "declared_vars": { "icon": { "resource": "t.dmi" } } },
    "/turf/open": { "parent": "/turf" },
    "/turf/open/floor": { "parent": "/turf/open", "declared_vars": { "icon_state": "floor" } },
    "/turf/closed": { "parent": "/turf", "declared_vars": { "density": 1, "opacity": 1 } },
    "/turf/closed/wall": { "parent": "/turf/closed", "declared_vars": { "icon_state": "wall" } },
    "/area": { "parent": "/atom" },
    "/area/space": { "parent": "/area" },
    "/area/station": { "parent": "/area" },
    "/area/station/guardian_test": { "parent": "/area/station" }
  }
}"##;

/// A long hall: the spawns at the west end, room to stray past every type's range.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/station/guardian_test)
"f" = (/turf/open/floor,/area/station/guardian_test)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/station/guardian_test)

(1,1,1) = {"
wwwwwwwwwwwwwwwwwwwwww
wssffffffffffffffffffw
wwwwwwwwwwwwwwwwwwwwww
"}
"##;

const STATES: [(&str, &str); 7] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
    ("g.dmi", "magicbase"),
    ("g.dmi", "minerbase"),
];

fn fixture() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("guardians_fixture");
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/item/guardian_creator/miner",
        "/mob/living/basic/guardian/charger",
        "/mob/living/basic/guardian/protector",
        "/mob/living/basic/guardian/ranged",
        "/mob/living/basic/guardian/standard",
        "/mob/living/basic/guardian/support",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/area/station/guardian_test",
        "/area/space",
    ];
    let assets = write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)]);
    let types = ss13_mapload::convert::DmTypes::parse(TYPES.as_bytes()).unwrap();
    let defines: std::collections::HashMap<String, String> =
        [("FACTION_NEUTRAL".to_owned(), "neutral".to_owned())].into();
    let mobs = ss13_mapload::mobs::resolve(&types, &Default::default(), &defines);
    std::fs::write(assets.join(ss13_mapload::mobs::MOBS_FILE), mobs.to_ron()).unwrap();
    assets
}

#[test]
fn a_dusty_shard_binds_a_ghost_as_a_guardian() {
    let mut s = testing::Station::start(fixture_map(&fixture(), &["map.dmm"]), |_| {});
    let a = s.join("p0", |_| {});
    let b = s.join("p1", |_| {});
    let (summoner, client_a, link_b) = (s.players[a].mob, s.players[a].link, s.players[b].link);
    let world = s.server.world_mut();
    ghostize_client(world, link_b).expect("B ghosts");
    let at = *world.get::<TilePos>(summoner).unwrap();
    let shard =
        ss13_game::construction::spawn_obj(world, "/obj/item/guardian_creator/miner", at).unwrap();

    // The shard picks a type and polls the ghosts.
    assert!(guardian::attack_self(world, client_a, summoner, shard));
    assert!(world.get::<guardian::CreatorUsed>(shard).is_some());
    let prompts: Vec<(Entity, Entity, Entity)> = world
        .query::<(Entity, &Prompt)>()
        .iter(world)
        .map(|(e, p)| (e, p.client, p.actor))
        .collect();
    assert_eq!(prompts.len(), 1, "the ghost is asked");
    let (prompt, client, actor) = prompts[0];
    world.write_message(PromptAnswered {
        prompt,
        client,
        actor,
        answer: Some(PromptAnswer::Button(0)),
    });
    s.tick();
    let world = s.server.world_mut();
    finish_all(world);
    let spirit = world.resource::<Players>().mob(link_b).unwrap();
    let bond = world
        .get::<Guardian>(spirit)
        .expect("B is the guardian")
        .clone();
    assert_eq!(bond.summoner, Some(summoner));
    assert_eq!(
        world.get::<InCloset>(spirit).map(|c| c.closet),
        Some(summoner)
    );
    assert!(
        !guardian::deployed(world, spirit),
        "hidden inside its summoner"
    );

    // Manifested, it takes a hit for its summoner.
    assert!(guardian::manifest(world, spirit, true));
    assert!(guardian::deployed(world, spirit));
    world
        .get_mut::<Health>(spirit)
        .unwrap()
        .adjust(DamageType::Brute, 10.0);
    s.tick();
    let world = s.server.world_mut();
    assert_eq!(
        world.get::<Health>(spirit).unwrap().brute,
        0.0,
        "invincible"
    );
    assert!(
        world.get::<Health>(summoner).unwrap().brute >= 9.9,
        "the summoner bleeds"
    );

    // Past its range it is pulled back to the summoner.
    let far = TilePos::new(at.x + bond.range as i32 + 3, at.y, at.z);
    world.entity_mut(spirit).insert(far);
    s.tick();
    let world = s.server.world_mut();
    assert_eq!(world.get::<TilePos>(spirit), world.get::<TilePos>(summoner));

    // A recall hides it again (no cooldown when forced).
    assert!(guardian::recall(world, spirit, true));
    assert!(!guardian::deployed(world, spirit));
    // A second shard use is refused.
    assert!(guardian::attack_self(world, client_a, summoner, shard));
    s.run_for(0.5);
}
