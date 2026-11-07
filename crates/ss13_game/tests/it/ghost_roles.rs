//! Map mob spawners (`ss13_game::ghost_roles`): a placed human corpse spawner leaves a dead,
//! damaged human in its outfit at map init; a placed ghost role sleeper is listed for ghosts,
//! and a ghost that takes it controls a new human in the role's outfit, the sleeper keeping one
//! use less.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::ghost_verbs::{GhostRoleSpawner, take_role};
use ss13_game::health::{Health, Stat};
use ss13_game::inventory::Inventory;
use ss13_game::server::Players;
use ss13_game::testing::{Station, fixture_map, write_fixture};
use ss13_mapload::DmItem;
use ss13_mapload::convert::DmTypes;
use ss13_mapload::mob_spawn::{DmMobSpawn, MOB_SPAWN_FILE};

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/datum/outfit": { "parent": "/datum", "declared_vars": { "uniform": null, "shoes": null,
        "back": null, "box": null, "backpack_contents": null, "accessory": null } },
    "/datum/outfit/beachbum": { "parent": "/datum/outfit", "declared_vars": {
        "uniform": { "path": "/obj/item/clothing/under/pants/jeans" } } },
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
    "/obj/effect/mob_spawn": { "parent": "/obj/effect", "declared_vars": {
        "name": "Mob Spawner", "icon": { "resource": "s.dmi" }, "icon_state": "sleeper",
        "mob_name": null, "outfit": null, "mob_species": null, "faction": null } },
    "/obj/effect/mob_spawn/ghost_role": { "parent": "/obj/effect/mob_spawn", "declared_vars": {
        "uses": 1, "you_are_text": "", "flavour_text": "", "important_text": "",
        "prompt_name": "", "temp_body": 0, "infinite_use": 0, "deletes_on_zero_uses_left": 1 } },
    "/obj/effect/mob_spawn/ghost_role/human": { "parent": "/obj/effect/mob_spawn/ghost_role" },
    "/obj/effect/mob_spawn/ghost_role/human/beach": {
        "parent": "/obj/effect/mob_spawn/ghost_role/human", "declared_vars": {
        "name": "beach bum sleeper", "you_are_text": "You're a beach bum.", "uses": 2,
        "outfit": { "path": "/datum/outfit/beachbum" } } },
    "/obj/effect/mob_spawn/corpse": { "parent": "/obj/effect/mob_spawn", "declared_vars": {
        "brute_damage": 0, "oxy_damage": 0, "burn_damage": 0 } },
    "/obj/effect/mob_spawn/corpse/human": { "parent": "/obj/effect/mob_spawn/corpse",
        "declared_vars": { "husk": 0 } },
    "/obj/effect/mob_spawn/corpse/human/bum": { "parent": "/obj/effect/mob_spawn/corpse/human",
        "declared_vars": { "mob_name": "Dead Bum", "brute_damage": 50,
        "outfit": { "path": "/datum/outfit/beachbum" } } },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "card_grey", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null, "w_class": 1,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null } },
    "/obj/item/card": { "parent": "/obj/item" },
    "/obj/item/card/id": { "parent": "/obj/item/card", "declared_vars": {
        "access": { "list": [] }, "icon_state": "card_grey", "slot_flags": 256 } },
    "/obj/item/card/id/advanced": { "parent": "/obj/item/card/id" },
    "/obj/item/clothing": { "parent": "/obj/item" },
    "/obj/item/clothing/under": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 2 } },
    "/obj/item/clothing/under/color": { "parent": "/obj/item/clothing/under" },
    "/obj/item/clothing/under/color/grey": { "parent": "/obj/item/clothing/under/color" },
    "/obj/item/clothing/under/pants": { "parent": "/obj/item/clothing/under" },
    "/obj/item/clothing/under/pants/jeans": { "parent": "/obj/item/clothing/under/pants" },
    "/obj/item/clothing/shoes": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 128 } },
    "/obj/item/clothing/shoes/sneakers": { "parent": "/obj/item/clothing/shoes" },
    "/obj/item/clothing/shoes/sneakers/black": { "parent": "/obj/item/clothing/shoes/sneakers" },
    "/mob": { "parent": "/atom/movable", "declared_vars": { "layer": 4, "density": 1 } },
    "/mob/living": { "parent": "/mob" },
    "/mob/living/carbon": { "parent": "/mob/living" },
    "/mob/living/carbon/human": { "parent": "/mob/living/carbon", "declared_vars": {
        "icon": { "resource": "h.dmi" }, "icon_state": "human_basic" } },
    "/turf": { "parent": "/atom", "declared_vars": { "icon": { "resource": "t.dmi" } } },
    "/turf/open": { "parent": "/turf" },
    "/turf/open/floor": { "parent": "/turf/open", "declared_vars": { "icon_state": "floor" } },
    "/turf/open/space": { "parent": "/turf/open" },
    "/turf/open/space/basic": { "parent": "/turf/open/space" },
    "/turf/closed": { "parent": "/turf", "declared_vars": { "density": 1, "opacity": 1 } },
    "/turf/closed/wall": { "parent": "/turf/closed", "declared_vars": { "icon_state": "wall" } },
    "/area": { "parent": "/atom" },
    "/area/space": { "parent": "/area" }
  }
}"##;

const BEACH: &str = "/obj/effect/mob_spawn/ghost_role/human/beach";
const CORPSE: &str = "/obj/effect/mob_spawn/corpse/human/bum";
const JEANS: &str = "/obj/item/clothing/under/pants/jeans";

const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"b" = (/obj/effect/mob_spawn/ghost_role/human/beach,/turf/open/floor,/area/space)
"c" = (/obj/effect/mob_spawn/corpse/human/bum,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwww
wsfbw
wfffw
wcffw
wwwww
"}
"##;

const STATES: [(&str, &str); 6] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "card_grey"),
    ("s.dmi", "sleeper"),
];

fn fixture_assets() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("ghost_roles_fixture");
    let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
    let data = ss13_mapload::mob_spawn::resolve(&types).to_ron();
    let used = [
        "/obj/effect/landmark/start/assistant",
        BEACH,
        CORPSE,
        JEANS,
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
    ];
    write_fixture(
        &dir,
        TYPES,
        &used,
        &STATES,
        &[("map.dmm", MAP), (MOB_SPAWN_FILE, data.as_str())],
    )
}

fn uniform_path(world: &World, mob: Entity) -> Option<String> {
    let uniform = world.get::<Inventory>(mob)?.uniform?;
    world.get::<DmItem>(uniform).map(|i| i.path.clone())
}

#[test]
fn corpses_are_made_and_ghost_roles_spawn_humans_in_their_outfit() {
    let assets = fixture_assets();
    let mut s = Station::start(fixture_map(&assets, &["map.dmm"]), |_| {});
    let world = s.server.world_mut();

    // The corpse: dead, damaged, in jeans; its spawner is gone.
    let mut names = world.query::<(Entity, &Name)>();
    let corpse = names
        .iter(world)
        .find(|(_, n)| n.as_str() == "Dead Bum")
        .map(|(e, _)| e)
        .expect("a corpse");
    assert_eq!(world.get::<Stat>(corpse), Some(&Stat::Dead));
    assert!(world.get::<Health>(corpse).unwrap().brute > 0.0);
    assert_eq!(uniform_path(world, corpse).as_deref(), Some(JEANS));
    let mut spawns = world.query::<&DmMobSpawn>();
    assert!(spawns.iter(world).all(|s| s.path != CORPSE));

    // The sleeper is a ghost role with its uses.
    let mut roles = world.query::<(Entity, &GhostRoleSpawner)>();
    let (sleeper, role) = roles
        .iter(world)
        .map(|(e, r)| (e, r.clone()))
        .next()
        .expect("a ghost role");
    assert_eq!(role.name, "beach bum sleeper");
    assert_eq!(role.uses, 2);
    assert_eq!(role.def.as_deref(), Some(BEACH));

    // A ghost takes it.
    let p = s.join("p0", |_| {});
    let link = s.players[p].link;
    let world = s.server.world_mut();
    let ghost = ss13_game::ghost::ghostize_client(world, link).expect("a ghost");
    let mob = take_role(world, ghost, sleeper).expect("a role mob");
    assert_eq!(world.resource::<Players>().mob(link), Some(mob));
    assert!(world.get_entity(ghost).is_err());
    assert_eq!(uniform_path(world, mob).as_deref(), Some(JEANS));
    assert_eq!(
        world.get::<GhostRoleSpawner>(sleeper).map(|r| r.uses),
        Some(1)
    );
}
