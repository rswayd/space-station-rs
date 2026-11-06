//! Bitrunning on a fixture den, with a client over real WebTransport on 127.0.0.1: the quantum
//! server refuses a domain it cannot pay for and loads the free one mid-round onto a level of
//! its own, the safehouse pasted in (two exits, a goal) and the encrypted cache locked on its
//! spawn; the runner shut into the netpod comes out in an avatar at an exit, the avatar's
//! wounds reach the body; the cache on the goal tile pays the server and the byteforge puts
//! out the decrypted crate; a safe disconnect returns the mind and the pod opens; the reset
//! scrubs the domain.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::closets::{Closet, InCloset};
use ss13_game::health::{DamageType, Health};
use ss13_game::science::bitrunning::avatar::{self, AvatarConnection, Hololadder, Netpod};
use ss13_game::science::bitrunning::server::{
    DECRYPTED_CACHE, DOMAINS_DIR, EncryptedCache, QuantumConsole, QuantumServer, SAFEHOUSES_DIR,
    cold_boot, reset,
};
use ss13_game::server::Players;
use ss13_game::storage::Stored;
use ss13_game::testing::{Drive, Station, fixture_map, write_fixture};
use ss13_game::zone::BodyZone;
use ss13_mapload::DmObj;
use ss13_shared::TilePos;
use ss13_world::SpatialIndex;

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3, "anchored": 0 } },
    "/obj": { "parent": "/atom/movable", "declared_vars": { "req_access": null } },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start" },
    "/obj/effect/landmark/bitrunning": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/bitrunning/hololadder_spawn": {
        "parent": "/obj/effect/landmark/bitrunning" },
    "/obj/effect/landmark/bitrunning/cache_goal_turf": {
        "parent": "/obj/effect/landmark/bitrunning" },
    "/obj/effect/landmark/bitrunning/cache_spawn": {
        "parent": "/obj/effect/landmark/bitrunning" },
    "/obj/modular_map_root": { "parent": "/obj", "declared_vars": { "key": null,
        "invisibility": 101, "anchored": 1 } },
    "/obj/modular_map_root/safehouse": { "parent": "/obj/modular_map_root" },
    "/obj/machinery": { "parent": "/obj", "declared_vars": { "anchored": 1, "density": 1,
        "use_power": 0, "idle_power_usage": 0, "active_power_usage": 0, "power_channel": 1,
        "icon": { "resource": "m.dmi" }, "icon_state": "qserver" } },
    "/obj/machinery/quantum_server": { "parent": "/obj/machinery", "declared_vars": {
        "name": "quantum server" } },
    "/obj/machinery/computer": { "parent": "/obj/machinery" },
    "/obj/machinery/computer/quantum_console": { "parent": "/obj/machinery/computer",
        "declared_vars": { "name": "quantum console" } },
    "/obj/machinery/netpod": { "parent": "/obj/machinery", "declared_vars": {
        "name": "netpod", "icon_state": "netpod" } },
    "/obj/machinery/byteforge": { "parent": "/obj/machinery", "declared_vars": {
        "name": "byteforge", "icon_state": "byteforge" } },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null, "w_class": 3,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null } },
    "/obj/item/paper": { "parent": "/obj/item", "declared_vars": { "name": "paper",
        "icon_state": "paper", "w_class": 1 } },
    "/obj/item/stack": { "parent": "/obj/item", "declared_vars": { "amount": 1,
        "max_amount": 50, "merge_type": null, "singular_name": null, "mats_per_unit": null } },
    "/obj/item/stack/ore": { "parent": "/obj/item/stack" },
    "/obj/item/stack/ore/iron": { "parent": "/obj/item/stack/ore", "declared_vars": {
        "name": "iron ore", "merge_type": { "path": "/obj/item/stack/ore/iron" } } },
    "/obj/item/stack/ore/glass": { "parent": "/obj/item/stack/ore", "declared_vars": {
        "name": "sand", "merge_type": { "path": "/obj/item/stack/ore/glass" } } },
    "/obj/item/card": { "parent": "/obj/item" },
    "/obj/item/card/id": { "parent": "/obj/item/card", "declared_vars": {
        "access": { "list": [] }, "icon_state": "card_grey", "slot_flags": 256 } },
    "/obj/item/card/id/advanced": { "parent": "/obj/item/card/id" },
    "/obj/item/clothing": { "parent": "/obj/item" },
    "/obj/item/clothing/under": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 2 } },
    "/obj/item/clothing/under/color": { "parent": "/obj/item/clothing/under" },
    "/obj/item/clothing/under/color/grey": { "parent": "/obj/item/clothing/under/color" },
    "/obj/item/clothing/shoes": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 128 } },
    "/obj/item/clothing/shoes/sneakers": { "parent": "/obj/item/clothing/shoes" },
    "/obj/item/clothing/shoes/sneakers/black": { "parent": "/obj/item/clothing/shoes/sneakers" },
    "/obj/structure": { "parent": "/obj", "declared_vars": { "anchored": 1 } },
    "/obj/structure/hololadder": { "parent": "/obj/structure", "declared_vars": {
        "name": "hololadder", "icon": { "resource": "m.dmi" }, "icon_state": "ladder11" } },
    "/obj/structure/closet": { "parent": "/obj/structure", "declared_vars": {
        "name": "closet", "icon": { "resource": "k.dmi" }, "icon_state": "crate",
        "density": 1, "opened": 0, "locked": 0, "welded": 0, "secure": 0, "icon_door": null,
        "icon_welded": "welded", "enable_door_overlay": 1, "has_opened_overlay": 1,
        "has_closed_overlay": 1, "dense_when_open": 0, "wall_mounted": 0, "horizontal": 0,
        "storage_capacity": 30, "anchored": 0 } },
    "/obj/structure/closet/crate": { "parent": "/obj/structure/closet", "declared_vars": {
        "name": "crate", "horizontal": 1 } },
    "/obj/structure/closet/crate/secure": { "parent": "/obj/structure/closet/crate",
        "declared_vars": { "secure": 1 } },
    "/obj/structure/closet/crate/secure/bitrunning": {
        "parent": "/obj/structure/closet/crate/secure" },
    "/obj/structure/closet/crate/secure/bitrunning/encrypted": {
        "parent": "/obj/structure/closet/crate/secure/bitrunning", "declared_vars": {
        "name": "encrypted cache", "locked": 1 } },
    "/obj/structure/closet/crate/secure/bitrunning/decrypted": {
        "parent": "/obj/structure/closet/crate/secure/bitrunning", "declared_vars": {
        "name": "decrypted cache" } },
    "/mob": { "parent": "/atom/movable", "declared_vars": { "layer": 4, "density": 1 } },
    "/mob/living": { "parent": "/mob" },
    "/mob/living/carbon": { "parent": "/mob/living" },
    "/mob/living/carbon/human": { "parent": "/mob/living/carbon", "declared_vars": {
        "icon": { "resource": "h.dmi" }, "icon_state": "human_basic" } },
    "/turf": { "parent": "/atom", "declared_vars": { "icon": { "resource": "t.dmi" } } },
    "/turf/open": { "parent": "/turf" },
    "/turf/open/floor": { "parent": "/turf/open", "declared_vars": { "icon_state": "floor" } },
    "/turf/closed": { "parent": "/turf", "declared_vars": { "density": 1, "opacity": 1 } },
    "/turf/closed/wall": { "parent": "/turf/closed", "declared_vars": { "icon_state": "wall" } },
    "/area": { "parent": "/atom" },
    "/area/station": { "parent": "/area" },
    "/area/virtual_domain": { "parent": "/area" },
    "/area/virtual_domain/safehouse": { "parent": "/area/virtual_domain" }
  },
  "populate": {
    "/obj/structure/closet": { "PopulateContents": ["return"] }
  }
}"##;

/// Rows top-down, the last line is y = 1: the console `C` (2,4) next to the quantum server `Q`
/// (3,4), the byteforge `B` (5,4), the runner's spawn (3,2) next to the netpod `N` (4,2).
const MAP: &str = r##""w" = (/turf/closed/wall,/area/station)
"f" = (/turf/open/floor,/area/station)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/station)
"Q" = (/obj/machinery/quantum_server,/turf/open/floor,/area/station)
"C" = (/obj/machinery/computer/quantum_console,/turf/open/floor,/area/station)
"N" = (/obj/machinery/netpod,/turf/open/floor,/area/station)
"B" = (/obj/machinery/byteforge,/turf/open/floor,/area/station)

(1,1,1) = {"
wwwwwww
wCQfBfw
wfffffw
wfsNffw
wwwwwww
"}
"##;

/// The domain: the safehouse root at (1,1), the cache spawn at (3,3).
const DOMAIN: &str = r##""a" = (/turf/open/floor,/area/virtual_domain)
"c" = (/obj/effect/landmark/bitrunning/cache_spawn,/turf/open/floor,/area/virtual_domain)
"r" = (/obj/modular_map_root/safehouse{key = "test_only"},/turf/open/floor,/area/virtual_domain)

(1,1,1) = {"
aaaaa
aacaa
aaaaa
raaaa
"}
"##;

/// From its bottom-left corner: the goal (1,1), exits (1,2) and (1,3).
const SAFEHOUSE: &str = r##""e" = (/obj/effect/landmark/bitrunning/hololadder_spawn,/turf/open/floor,/area/virtual_domain/safehouse)
"g" = (/obj/effect/landmark/bitrunning/cache_goal_turf,/turf/open/floor,/area/virtual_domain/safehouse)

(1,1,1) = {"
e
e
g
"}
"##;

const SERVER: TilePos = TilePos::new(3, 4, 1);
const CONSOLE: TilePos = TilePos::new(2, 4, 1);
const POD: TilePos = TilePos::new(4, 2, 1);
const FORGE: TilePos = TilePos::new(5, 4, 1);

fn fixture_assets() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("bitrunning_fixture");
    std::fs::create_dir_all(dir.join(DOMAINS_DIR)).unwrap();
    std::fs::create_dir_all(dir.join(SAFEHOUSES_DIR)).unwrap();
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/effect/landmark/bitrunning/hololadder_spawn",
        "/obj/effect/landmark/bitrunning/cache_goal_turf",
        "/obj/effect/landmark/bitrunning/cache_spawn",
        "/obj/modular_map_root/safehouse",
        "/obj/machinery/quantum_server",
        "/obj/machinery/computer/quantum_console",
        "/obj/machinery/netpod",
        "/obj/machinery/byteforge",
        "/obj/item/paper",
        "/obj/item/stack/ore/iron",
        "/obj/item/stack/ore/glass",
        "/obj/structure/hololadder",
        "/obj/structure/closet/crate/secure/bitrunning/encrypted",
        "/obj/structure/closet/crate/secure/bitrunning/decrypted",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/area/station",
        "/area/virtual_domain",
        "/area/virtual_domain/safehouse",
    ];
    let states = [
        ("t.dmi", "floor"),
        ("t.dmi", "wall"),
        ("l.dmi", "x2"),
        ("h.dmi", "human_basic"),
        ("i.dmi", "card_grey"),
        ("i.dmi", "toy"),
        ("i.dmi", "paper"),
        ("m.dmi", "qserver"),
        ("m.dmi", "netpod"),
        ("m.dmi", "netpod_open_active"),
        ("m.dmi", "netpod_closed"),
        ("m.dmi", "netpod_closed_active"),
        ("m.dmi", "byteforge"),
        ("m.dmi", "ladder11"),
        ("k.dmi", "crate"),
        ("k.dmi", "crateopen"),
    ];
    let domain = format!("{DOMAINS_DIR}/test_only.dmm");
    let house = format!("{SAFEHOUSES_DIR}/test_only_safehouse.dmm");
    write_fixture(
        &dir,
        TYPES,
        &used,
        &states,
        &[
            ("map.dmm", MAP),
            (domain.as_str(), DOMAIN),
            (house.as_str(), SAFEHOUSE),
        ],
    )
}

fn find<T: Component>(world: &World, pos: TilePos) -> Entity {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .find(|&e| world.get::<T>(e).is_some())
        .unwrap_or_else(|| panic!("nothing with {} at {pos:?}", std::any::type_name::<T>()))
}

fn server_state(s: &Station, server: Entity) -> QuantumServer {
    s.server
        .world()
        .get::<QuantumServer>(server)
        .unwrap()
        .clone()
}

#[test]
fn a_domain_loads_connects_pays_out_and_scrubs() {
    let dir = fixture_assets();
    let mut s = Station::start(fixture_map(&dir, &["map.dmm"]), |_| {});
    let p = s.join("runner", |_| {});
    let (link, body) = (s.players[p].link, s.players[p].mob);
    let world = s.server.world_mut();
    let server = find::<QuantumServer>(world, SERVER);
    let pod = find::<Netpod>(world, POD);
    let console = find::<QuantumConsole>(world, CONSOLE);
    assert_eq!(
        world.get::<QuantumConsole>(console).unwrap().server,
        Some(server)
    );

    // `test_only_expensive` costs 3 points; `test_only` is free.
    assert!(!cold_boot(
        world,
        server,
        Some("test_only_expensive"),
        false,
        None
    ));
    assert!(cold_boot(world, server, Some("test_only"), false, None));
    let domain = world
        .get::<QuantumServer>(server)
        .unwrap()
        .domain
        .clone()
        .expect("loaded");
    let z = domain.levels[0];
    assert!(z > 1, "a level of its own");
    assert_eq!(domain.goal_turfs, vec![TilePos::new(1, 1, z)]);
    let mut exits = domain.exit_turfs.clone();
    exits.sort_by_key(|t| t.y);
    assert_eq!(exits, vec![TilePos::new(1, 2, z), TilePos::new(1, 3, z)]);
    let mut caches = world.query_filtered::<Entity, With<EncryptedCache>>();
    let caches: Vec<Entity> = caches.iter(world).collect();
    assert_eq!(caches.len(), 1);
    let cache = caches[0];
    assert_eq!(world.get::<TilePos>(cache), Some(&TilePos::new(3, 3, z)));
    assert!(world.get::<Closet>(cache).unwrap().locked);
    // Busy: a second boot is refused.
    assert!(!cold_boot(world, server, Some("test_only"), false, None));

    // Into the netpod: after 2 s the runner plays an avatar at an exit.
    assert!(avatar::mouse_drop_receive(world, pod, body));
    assert_eq!(world.get::<InCloset>(body).map(|c| c.closet), Some(pod));
    s.run_until("the runner is in the avatar", |s| {
        s.server.world().resource::<Players>().mob(link) != Some(body)
    });
    let world = s.server.world_mut();
    let avatar_mob = world.resource::<Players>().mob(link).unwrap();
    let conn = world.get::<AvatarConnection>(avatar_mob).unwrap().clone();
    assert_eq!((conn.body, conn.pod, conn.server), (body, pod, server));
    let at = *world.get::<TilePos>(avatar_mob).unwrap();
    assert!(exits.contains(&at), "at an exit: {at:?}");
    assert!(
        world
            .resource::<SpatialIndex>()
            .on_tile(at)
            .iter()
            .any(|&e| world.get::<Hololadder>(e).is_some()),
        "its hololadder"
    );
    assert_eq!(server_state(&s, server).connections, vec![avatar_mob]);

    // The avatar's wound reaches the body (less the pod's healing).
    let world = s.server.world_mut();
    let before = world.get::<Health>(body).unwrap().brute;
    world.get_mut::<Health>(avatar_mob).unwrap().damage_zone(
        BodyZone::Chest,
        DamageType::Brute,
        20.0,
    );
    s.tick();
    s.tick();
    let world = s.server.world();
    let after = world.get::<Health>(body).unwrap().brute;
    assert!(after > before + 15.0, "linked damage: {before} -> {after}");
    assert!(!world.get::<AvatarConnection>(avatar_mob).unwrap().nohit);

    // The cache on the goal: the server is paid, the byteforge puts out the reward.
    s.server
        .world_mut()
        .entity_mut(cache)
        .insert(TilePos::new(1, 1, z));
    s.tick();
    let state = server_state(&s, server);
    assert_eq!(state.points, 1, "BITRUNNER_REWARD_MIN");
    assert!(state.complete);
    let world = s.server.world();
    assert!(world.get_entity(cache).is_err(), "the cache is gone");
    let reward = world
        .resource::<SpatialIndex>()
        .on_tile(FORGE)
        .iter()
        .copied()
        .find(|&e| {
            world
                .get::<DmObj>(e)
                .is_some_and(|o| o.path == DECRYPTED_CACHE)
        })
        .expect("the decrypted crate on the byteforge");
    let inside = world.get::<Stored>(reward).map_or(0, |s| s.items().len());
    assert!(inside >= 3, "iron, sand and the certificate: {inside}");

    // A safe disconnect: the mind is back, the hurt body heals 10 s, then the pod opens.
    avatar::full_disconnect(s.server.world_mut(), avatar_mob, false);
    let world = s.server.world();
    assert_eq!(world.resource::<Players>().mob(link), Some(body));
    assert!(world.get::<AvatarConnection>(avatar_mob).is_none());
    assert!(!world.get::<Netpod>(pod).unwrap().connected);
    s.run_for(11.0);
    let world = s.server.world();
    assert!(world.get::<InCloset>(body).is_none(), "out of the pod");
    assert!(world.get::<Netpod>(pod).unwrap().open);

    // The reset: the domain is scrubbed after 15 s, the avatar with it.
    reset(s.server.world_mut(), server);
    assert!(!server_state(&s, server).is_ready);
    s.run_for(16.0);
    let state = server_state(&s, server);
    assert!(state.domain.is_none());
    assert!(!state.is_ready, "cooling down");
    let world = s.server.world();
    assert!(world.get_entity(avatar_mob).is_err(), "the avatar is gone");
    let left = world
        .resource::<SpatialIndex>()
        .in_rect(z, (0, 0), (10, 10))
        .filter(|&e| world.get::<DmObj>(e).is_some() || world.get::<Health>(e).is_some())
        .count();
    assert_eq!(left, 0, "no objects or mobs stay on the domain's level");
}
