//! Maintenance drones (`ss13_game::silicon::drones`, `ss13_game::dextrous`): a mapped derelict
//! shell is a ghost role that makes "derelict drone (N)" with its laws, ushanka and built-in
//! tools; a regular drone is shy (a pacifist through trauma refreshes) and wears only hats in
//! its head slot; a preloaded dispenser prints a drone shell from its iron and glass.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::dextrous::Dextrous;
use ss13_game::ghost_verbs::{GhostRoleSpawner, take_role};
use ss13_game::hud::HudLayout;
use ss13_game::inventory::{Inventory, NoDrop, can_equip};
use ss13_game::medical::traumas::Pacifist;
use ss13_game::server::Players;
use ss13_game::silicon::drone_dispenser::{DroneDispenser, Mode};
use ss13_game::silicon::drones::Drone;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::DmItem;
use ss13_mapload::convert::DmTypes;
use ss13_mapload::mob_spawn::{DmMobSpawn, MOB_SPAWN_FILE};
use ss13_shared::TilePos;
use ss13_shared::protocol::InventorySlot;

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
    "/obj/effect/mob_spawn": { "parent": "/obj/effect", "declared_vars": {
        "name": "Mob Spawner", "mob_name": null, "outfit": null, "mob_species": null,
        "faction": null, "mob_type": null, "icon": { "resource": "d.dmi" },
        "icon_state": "drone_maint_hat" } },
    "/obj/effect/mob_spawn/ghost_role": { "parent": "/obj/effect/mob_spawn", "declared_vars": {
        "uses": 1, "you_are_text": "", "flavour_text": "", "important_text": "",
        "prompt_name": "", "temp_body": 0, "infinite_use": 0, "deletes_on_zero_uses_left": 1 } },
    "/obj/effect/mob_spawn/ghost_role/drone": { "parent": "/obj/effect/mob_spawn/ghost_role",
        "declared_vars": { "name": "drone shell", "mob_name": "drone",
        "mob_type": { "path": "/mob/living/basic/drone" },
        "you_are_text": "You are a Maintenance Drone." } },
    "/obj/effect/mob_spawn/ghost_role/drone/derelict": {
        "parent": "/obj/effect/mob_spawn/ghost_role/drone", "declared_vars": {
        "name": "derelict drone shell", "mob_name": "derelict drone",
        "mob_type": { "path": "/mob/living/basic/drone/derelict" },
        "you_are_text": "Вы - дрон на Космической Станции 13." } },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null, "w_class": 2,
        "lefthand_file": { "resource": "lh.dmi" }, "righthand_file": { "resource": "rh.dmi" },
        "worn_icon": null, "worn_icon_state": null, "item_flags": 0 } },
    "/obj/item/storage": { "parent": "/obj/item" },
    "/obj/item/storage/drone_tools": { "parent": "/obj/item/storage", "declared_vars": {
        "name": "built-in tools", "item_flags": 128 } },
    "/obj/item/clothing": { "parent": "/obj/item" },
    "/obj/item/clothing/head": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 64 } },
    "/obj/item/clothing/head/costume": { "parent": "/obj/item/clothing/head" },
    "/obj/item/clothing/head/costume/ushanka": { "parent": "/obj/item/clothing/head/costume",
        "declared_vars": { "name": "ushanka" } },
    "/obj/item/clothing/head/hat": { "parent": "/obj/item/clothing/head" },
    "/obj/item/clothing/under": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 2 } },
    "/obj/item/clothing/under/color": { "parent": "/obj/item/clothing/under" },
    "/obj/item/clothing/under/color/grey": { "parent": "/obj/item/clothing/under/color" },
    "/obj/item/clothing/shoes": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 128 } },
    "/obj/item/clothing/shoes/sneakers": { "parent": "/obj/item/clothing/shoes" },
    "/obj/item/clothing/shoes/sneakers/black": { "parent": "/obj/item/clothing/shoes/sneakers" },
    "/obj/item/card": { "parent": "/obj/item" },
    "/obj/item/card/id": { "parent": "/obj/item/card", "declared_vars": {
        "access": { "list": [] }, "slot_flags": 256 } },
    "/obj/item/card/id/advanced": { "parent": "/obj/item/card/id" },
    "/obj/machinery": { "parent": "/obj", "declared_vars": { "anchored": 1, "density": 1,
        "icon": { "resource": "dd.dmi" } } },
    "/obj/machinery/drone_dispenser": { "parent": "/obj/machinery", "declared_vars": {
        "name": "drone shell dispenser", "icon_state": "on" } },
    "/obj/machinery/drone_dispenser/preloaded": { "parent": "/obj/machinery/drone_dispenser" },
    "/mob": { "parent": "/atom/movable", "declared_vars": { "layer": 4, "density": 1 } },
    "/mob/living": { "parent": "/mob", "declared_vars": { "maxHealth": 100, "health": 100 } },
    "/mob/living/carbon": { "parent": "/mob/living" },
    "/mob/living/carbon/human": { "parent": "/mob/living/carbon", "declared_vars": {
        "icon": { "resource": "h.dmi" }, "icon_state": "human_basic" } },
    "/mob/living/basic": { "parent": "/mob/living", "declared_vars": {
        "maxHealth": 20, "health": 20, "speed": 1, "melee_attack_cooldown": 20,
        "unsuitable_atmos_damage": 0, "unsuitable_cold_damage": 0, "unsuitable_heat_damage": 0,
        "minimum_survivable_temperature": 0, "maximum_survivable_temperature": 1500,
        "habitable_atmos": null, "melee_damage_type": "brute",
        "icon": { "resource": "d.dmi" } } },
    "/mob/living/basic/drone": { "parent": "/mob/living/basic", "declared_vars": {
        "name": "Drone", "maxHealth": 45, "health": 45, "density": 0, "speed": 0,
        "icon_state": "drone_maint_grey", "icon_living": "drone_maint_grey",
        "icon_dead": "drone_maint_dead" } },
    "/mob/living/basic/drone/derelict": { "parent": "/mob/living/basic/drone",
        "declared_vars": { "name": "derelict drone" } },
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

const DERELICT_SHELL: &str = "/obj/effect/mob_spawn/ghost_role/drone/derelict";
const SHELL: &str = "/obj/effect/mob_spawn/ghost_role/drone";
const DRONE: &str = "/mob/living/basic/drone";
const TOOLS: &str = "/obj/item/storage/drone_tools";
const USHANKA: &str = "/obj/item/clothing/head/costume/ushanka";
const HAT: &str = "/obj/item/clothing/head/hat";
const JUMPSUIT: &str = "/obj/item/clothing/under/color/grey";

/// `s` the player's spot, `d` a derelict drone shell, `D` a preloaded dispenser.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"d" = (/obj/effect/mob_spawn/ghost_role/drone/derelict,/turf/open/floor,/area/space)
"D" = (/obj/machinery/drone_dispenser/preloaded,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwww
wsfdfw
wfffDw
wwwwww
"}
"##;

const STATES: [(&str, &str); 9] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
    ("d.dmi", "drone_maint_grey"),
    ("d.dmi", "drone_maint_dead"),
    ("d.dmi", "drone_maint_hat"),
    ("dd.dmi", "on"),
];

fn fixture_assets() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("drones_fixture");
    let used = [
        "/obj/effect/landmark/start/assistant",
        SHELL,
        DERELICT_SHELL,
        DRONE,
        "/mob/living/basic/drone/derelict",
        TOOLS,
        USHANKA,
        HAT,
        JUMPSUIT,
        "/obj/machinery/drone_dispenser/preloaded",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
    ];
    let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
    let spawns = ss13_mapload::mob_spawn::resolve(&types).to_ron();
    let assets = write_fixture(
        &dir,
        TYPES,
        &used,
        &STATES,
        &[("map.dmm", MAP), (MOB_SPAWN_FILE, spawns.as_str())],
    );
    let mobs = ss13_mapload::mobs::resolve(&types, &Default::default(), &Default::default());
    std::fs::write(assets.join(ss13_mapload::mobs::MOBS_FILE), mobs.to_ron()).unwrap();
    assets
}

struct Station(testing::Station);
ss13_game::drive_station!(Station, 0);

fn start() -> Station {
    Station(testing::Station::start(
        fixture_map(&fixture_assets(), &["map.dmm"]),
        |_| {},
    ))
}

fn path_in(world: &World, inventory: &Inventory, slot: InventorySlot) -> Option<String> {
    let item = inventory.slot(slot)?;
    world.get::<DmItem>(item).map(|i| i.path.clone())
}

#[test]
fn a_derelict_shell_makes_a_drone_with_its_laws_hat_and_tools() {
    let mut s = start();
    let world = s.0.server.world_mut();
    let mut roles = world.query::<(Entity, &GhostRoleSpawner)>();
    let (shell, role) = roles
        .iter(world)
        .find(|(_, r)| r.def.as_deref() == Some(DERELICT_SHELL))
        .map(|(e, r)| (e, r.clone()))
        .expect("the mapped shell is a ghost role");
    assert_eq!(role.name, "derelict drone shell");
    assert_eq!(role.uses, 1);

    let p = s.0.join("p0", |_| {});
    let link = s.0.players[p].link;
    let world = s.0.server.world_mut();
    let ghost = ss13_game::ghost::ghostize_client(world, link).expect("a ghost");
    let drone = take_role(world, ghost, shell).expect("a drone");
    assert_eq!(world.resource::<Players>().mob(link), Some(drone));
    assert!(world.get_entity(shell).is_err(), "one use");
    let name = world.get::<Name>(drone).unwrap().as_str().to_owned();
    assert!(name.starts_with("derelict drone ("), "{name}");
    let laws = &world.get::<Drone>(drone).expect("a drone").laws;
    assert!(
        laws.starts_with("1. You may not involve yourself in the matters of another sentient"),
        "{laws}"
    );
    // Derelict drones are not shy.
    assert!(world.get::<Pacifist>(drone).is_none());
    assert_eq!(
        world.get::<Dextrous>(drone).map(|d| d.layout),
        Some(HudLayout::Drone)
    );
    let inventory = world.get::<Inventory>(drone).unwrap().clone();
    assert_eq!(
        path_in(world, &inventory, InventorySlot::Head).as_deref(),
        Some(USHANKA)
    );
    assert_eq!(
        path_in(world, &inventory, InventorySlot::SuitStorage).as_deref(),
        Some(TOOLS)
    );
    let tools = inventory.suit_storage.unwrap();
    assert!(
        world.get::<NoDrop>(tools).is_some(),
        "the tools are built in"
    );
}

#[test]
fn a_regular_drone_is_shy_and_wears_only_hats() {
    let mut s = start();
    let world = s.0.server.world_mut();
    let drone =
        ss13_game::mobs::spawn::spawn_mob(world, DRONE, TilePos::new(3, 2, 1)).expect("a drone");
    // Its gear comes after the spawn (the system does it next tick).
    ss13_game::silicon::drones::give_gear(world, drone);
    assert!(
        world
            .get::<Drone>(drone)
            .unwrap()
            .laws
            .starts_with("1. You may not involve yourself in the matters of another being")
    );
    assert!(world.get::<Pacifist>(drone).is_some(), "shy");
    // A trauma refresh keeps the shy drone's pacifism.
    ss13_game::medical::traumas::refresh(world, drone);
    assert!(world.get::<Pacifist>(drone).is_some());

    let hat = ss13_game::construction::spawn_detached(world, HAT).expect("a hat");
    let jumpsuit = ss13_game::construction::spawn_detached(world, JUMPSUIT).expect("a jumpsuit");
    let inventory = world.get::<Inventory>(drone).unwrap().clone();
    assert!(can_equip(
        world,
        drone,
        &inventory,
        hat,
        InventorySlot::Head
    ));
    assert!(!can_equip(
        world,
        drone,
        &inventory,
        jumpsuit,
        InventorySlot::Uniform
    ));
    assert!(!can_equip(
        world,
        drone,
        &inventory,
        jumpsuit,
        InventorySlot::Head
    ));
    // The internal storage already holds the tools.
    assert!(!can_equip(
        world,
        drone,
        &inventory,
        hat,
        InventorySlot::SuitStorage
    ));
}

#[test]
fn a_preloaded_dispenser_prints_a_drone_shell() {
    let mut s = start();
    let world = s.0.server.world_mut();
    let mut q = world.query::<(Entity, &DroneDispenser, &TilePos)>();
    let (dispenser, d, &pos) = q.iter(world).next().expect("a dispenser");
    assert_eq!((d.iron, d.glass), (250, 250));
    s.run_until("a shell is printed", |s| {
        let world = s.0.server.world_mut();
        // The fixture has no APC.
        world
            .entity_mut(dispenser)
            .remove::<ss13_game::power::Unpowered>();
        let mut shells = world.query::<(&DmMobSpawn, &TilePos, &GhostRoleSpawner)>();
        shells
            .iter(world)
            .any(|(m, p, _)| m.path == SHELL && *p == pos)
    });
    let d = s.0.get::<DroneDispenser>(dispenser);
    assert_eq!((d.iron, d.glass), (200, 200));
    assert_eq!(d.mode, Mode::Recharging);
}
