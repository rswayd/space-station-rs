//! Soulstones on a fixture station (`ss13_game::antag::cult::server::soulstone`): a cultist's
//! stone takes a victim in crit as a cult shade (the body is dust, the victim's player is the
//! shade), the stone in hand lets it out and takes it back, and the stone on a construct shell
//! makes the chosen construct for the shade's player; shell and stone are used up.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::antag::cult::server::{
    ConstructPrompt, CultForm, CultTeam, Cultist, SHELL, SOULSTONE, Soulstone, add_cultist,
    soulstone_attack_self, soulstone_interaction,
};
use ss13_game::closets::InCloset;
use ss13_game::health::Stat;
use ss13_game::inventory::{CarriedBy, Inventory};
use ss13_game::mobs::BasicMob;
use ss13_game::server::{PlayerMob, Players};
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_shared::TilePos;
use ss13_tgui::prompt::{PromptAnswer, PromptAnswered};

const SHADE: &str = "/mob/living/basic/shade";
const WRAITH: &str = "/mob/living/basic/construct/wraith";

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
    "/obj/structure/constructshell": { "parent": "/obj/structure", "declared_vars": {
        "name": "empty shell", "icon": { "resource": "s.dmi" }, "icon_state": "construct_cult" } },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "soulstone", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null, "attack_speed": 8 } },
    "/obj/item/soulstone": { "parent": "/obj/item", "declared_vars": {
        "name": "soulstone shard" } },
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
    "/mob/living/basic/shade": { "parent": "/mob/living/basic", "declared_vars": {
        "name": "Shade", "maxHealth": 40, "health": 40, "basic_mob_flags": 1,
        "icon_state": "shade_cult", "icon_living": "shade_cult" } },
    "/mob/living/basic/construct": { "parent": "/mob/living/basic", "declared_vars": {
        "basic_mob_flags": 1 } },
    "/mob/living/basic/construct/wraith": { "parent": "/mob/living/basic/construct",
        "declared_vars": { "name": "Wraith", "maxHealth": 65, "health": 65,
        "icon_state": "wraith", "icon_living": "wraith" } },
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
    "/area/station/stone_test": { "parent": "/area/station" }
  }
}"##;

const FACTIONS: &str = "#define FACTION_NEUTRAL \"neutral\"\n";

const STATES: [(&str, &str); 9] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "soulstone"),
    ("s.dmi", "construct_cult"),
    ("c.dmi", "shade_cult"),
    ("c.dmi", "wraith"),
    ("i.dmi", "soulstone2"),
];

/// Two players side by side (2,3) and (3,3).
const MAP: &str = r##""w" = (/turf/closed/wall,/area/station/stone_test)
"f" = (/turf/open/floor,/area/station/stone_test)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/station/stone_test)

(1,1,1) = {"
wwwwww
wffffw
wssffw
wffffw
wwwwww
"}
"##;

fn fixture() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("soulstones_fixture");
    let used = [
        "/obj/effect/landmark/start/assistant",
        SOULSTONE,
        SHELL,
        SHADE,
        WRAITH,
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/station/stone_test",
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

fn mob_of(s: &mut Station, path: &str) -> Option<Entity> {
    let world = s.0.server.world_mut();
    world
        .query::<(Entity, &BasicMob)>()
        .iter(world)
        .find(|(_, m)| m.def.path == path)
        .map(|(e, _)| e)
}

#[test]
fn a_cult_soulstone_takes_a_soul_and_makes_a_construct() {
    let mut s = Station(testing::Station::start(
        fixture_map(&fixture(), &["map.dmm"]),
        |_| {},
    ));
    let (a, b) = (s.join("p0", |_| {}), s.join("p1", |_| {}));
    let (cultist, victim) = (s.players[a].mob, s.players[b].mob);
    let victim_client = s.players[b].link;
    s.run_for(0.2);
    let stone = {
        let world = s.0.server.world_mut();
        assert!(add_cultist(world, cultist));
        // No sacrifice target to refuse.
        world.resource_mut::<CultTeam>().objectives.clear();
        let pos = *world.get::<TilePos>(cultist).unwrap();
        let stone = ss13_game::construction::spawn_obj(world, SOULSTONE, pos).unwrap();
        world
            .entity_mut(stone)
            .remove::<TilePos>()
            .insert(CarriedBy(cultist));
        world.get_mut::<Inventory>(cultist).unwrap().hands[0] = Some(stone);
        stone
    };

    // Awake, the victim cannot be taken.
    let world = s.0.server.world_mut();
    assert!(soulstone_interaction(world, cultist, stone, victim));
    assert!(world.get_entity(victim).is_ok(), "still standing");
    // In crit: the soul goes into the stone, the body is dust.
    world.entity_mut(victim).insert(Stat::HardCrit);
    assert!(soulstone_interaction(world, cultist, stone, victim));
    assert!(world.get_entity(victim).is_err(), "dusted");
    let shade = world.get::<Soulstone>(stone).unwrap().shade.unwrap();
    assert_eq!(world.get::<InCloset>(shade).unwrap().closet, stone);
    assert_eq!(world.get::<PlayerMob>(shade).unwrap().client, victim_client);
    assert_eq!(
        world.get::<Cultist>(shade).unwrap().form,
        CultForm::Shade { released_at: None }
    );
    assert!(
        world
            .get::<Name>(stone)
            .unwrap()
            .as_str()
            .contains(": Shade of "),
        "the stone names its shade"
    );

    // Let out by the stone in hand, taken back by the stone.
    assert!(soulstone_attack_self(world, cultist, stone));
    assert!(world.get::<InCloset>(shade).is_none());
    assert!(matches!(
        world.get::<Cultist>(shade).unwrap().form,
        CultForm::Shade {
            released_at: Some(_)
        }
    ));
    assert!(soulstone_interaction(world, cultist, stone, shade));
    assert_eq!(world.get::<InCloset>(shade).unwrap().closet, stone);

    // The stone on a shell: the cultist picks the Wraith.
    let pos = *world.get::<TilePos>(cultist).unwrap();
    let shell_at = TilePos::new(pos.x, pos.y + 1, pos.z);
    let shell = ss13_game::construction::spawn_obj(world, SHELL, shell_at).unwrap();
    assert!(soulstone_interaction(world, cultist, stone, shell));
    let prompt = world
        .query_filtered::<Entity, With<ConstructPrompt>>()
        .iter(world)
        .next()
        .expect("the construct list");
    let client = world.get::<PlayerMob>(cultist).unwrap().client;
    world.write_message(PromptAnswered {
        prompt,
        client,
        actor: cultist,
        answer: Some(PromptAnswer::Item(1)),
    });
    s.run_for(0.5);
    let wraith = mob_of(&mut s, WRAITH).expect("a wraith");
    let world = s.0.server.world_mut();
    assert_eq!(
        world.resource::<Players>().mob(victim_client),
        Some(wraith),
        "the shade's player is the wraith"
    );
    assert_eq!(
        world.get::<Cultist>(wraith).unwrap().form,
        CultForm::Construct
    );
    assert!(world.get_entity(shell).is_err() && world.get_entity(stone).is_err());
    assert!(world.get_entity(shade).is_err(), "the empty shade is gone");
}
