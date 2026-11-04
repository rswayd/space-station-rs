//! Station bots on a fixture station over real WebTransport: Beepsky patrols from beacon to
//! beacon through a door his access opens; an emagged securitron stuns and cuffs a player; a
//! cleanbot cleans dirt; a medbot tends an injured player; a firebot puts a burning player out;
//! a medbot and a cleanbot are built from their parts; the window's controls need access.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::bots::{self, Bot};
use ss13_game::health::{DamageType, Health, Stat};
use ss13_game::inventory::{CarriedBy, Inventory};
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::{DmMob, DmNavBeacon};
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
    "/obj/effect/decal": { "parent": "/obj/effect" },
    "/obj/effect/decal/cleanable": { "parent": "/obj/effect/decal" },
    "/obj/effect/decal/cleanable/dirt": { "parent": "/obj/effect/decal/cleanable",
        "declared_vars": { "name": "dirt", "icon": { "resource": "e.dmi" }, "icon_state": "dirt" } },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null, "attack_speed": 8 } },
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
    "/obj/item/assembly": { "parent": "/obj/item" },
    "/obj/item/restraints": { "parent": "/obj/item" },
    "/obj/item/restraints/handcuffs": { "parent": "/obj/item/restraints", "declared_vars": {
        "name": "handcuffs", "handcuff_time": 40, "breakouttime": 600 } },
    "/obj/item/restraints/handcuffs/cable": { "parent": "/obj/item/restraints/handcuffs" },
    "/obj/item/restraints/handcuffs/cable/zipties": { "parent": "/obj/item/restraints/handcuffs/cable",
        "declared_vars": { "name": "zipties" } },
    "/obj/item/restraints/handcuffs/cable/zipties/used": {
        "parent": "/obj/item/restraints/handcuffs/cable/zipties" },
    "/obj/item/assembly/prox_sensor": { "parent": "/obj/item/assembly",
        "declared_vars": { "name": "proximity sensor" } },
    "/obj/item/bodypart": { "parent": "/obj/item" },
    "/obj/item/bodypart/arm": { "parent": "/obj/item/bodypart" },
    "/obj/item/bodypart/arm/right": { "parent": "/obj/item/bodypart/arm" },
    "/obj/item/bodypart/arm/right/robot": { "parent": "/obj/item/bodypart/arm/right",
        "declared_vars": { "name": "cyborg right arm" } },
    "/obj/item/reagent_containers": { "parent": "/obj/item" },
    "/obj/item/reagent_containers/cup": { "parent": "/obj/item/reagent_containers" },
    "/obj/item/reagent_containers/cup/bucket": { "parent": "/obj/item/reagent_containers/cup",
        "declared_vars": { "name": "bucket" } },
    "/obj/item/storage": { "parent": "/obj/item" },
    "/obj/item/storage/medkit": { "parent": "/obj/item/storage", "declared_vars": { "name": "medkit" } },
    "/obj/item/storage/medkit/fire": { "parent": "/obj/item/storage/medkit",
        "declared_vars": { "name": "burn treatment kit" } },
    "/obj/item/healthanalyzer": { "parent": "/obj/item", "declared_vars": { "name": "health analyzer" } },
    "/obj/item/bot_assembly": { "parent": "/obj/item", "declared_vars": { "icon": { "resource": "b.dmi" } } },
    "/obj/item/bot_assembly/cleanbot": { "parent": "/obj/item/bot_assembly",
        "declared_vars": { "name": "incomplete cleanbot assembly", "icon_state": "cleanbot_assembly" } },
    "/obj/item/bot_assembly/medbot": { "parent": "/obj/item/bot_assembly",
        "declared_vars": { "name": "incomplete medibot assembly", "icon_state": "medbot_assembly_generic" } },
    "/obj/machinery": { "parent": "/obj" },
    "/obj/machinery/navbeacon": { "parent": "/obj/machinery", "declared_vars": {
        "name": "navigation beacon", "icon": { "resource": "n.dmi" }, "icon_state": "navbeacon0",
        "location": "", "codes_txt": "" } },
    "/obj/machinery/door": { "parent": "/obj/machinery", "declared_vars": {
        "icon": { "resource": "d.dmi" }, "icon_state": "door_closed", "density": 1, "opacity": 1,
        "layer": 2.7, "base_icon_state": "door", "autoclose": 0, "can_open_with_hands": 1,
        "closingLayer": 3.1, "glass": 0, "locked": 0, "safe": 1, "visible": 1, "welded": 0 } },
    "/obj/machinery/door/airlock": { "parent": "/obj/machinery/door", "declared_vars": {
        "icon": { "resource": "a.dmi" }, "icon_state": "closed", "autoclose": 1,
        "base_icon_state": null, "normalspeed": 1, "overlays_file": { "resource": "o.dmi" },
        "airlock_material": null } },
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
    "/mob/living/basic/bot": { "parent": "/mob/living/basic", "declared_vars": {
        "icon": { "resource": "b.dmi" }, "basic_mob_flags": 1, "density": 0, "speed": 3,
        "habitable_atmos": null, "minimum_survivable_temperature": 0,
        "maximum_survivable_temperature": 1e31,
        "faction": { "expr": "list(FACTION_SILICON, FACTION_NEUTRAL, FACTION_TURRET)" },
        "ai_controller": { "path": "/datum/ai_controller/basic_controller/bot" },
        "bot_mode_flags": 29, "bot_access_flags": 2, "req_one_access": { "list": ["robotics"] },
        "radio_channel": "Общий", "additional_access": null } },
    "/mob/living/basic/bot/secbot": { "parent": "/mob/living/basic/bot", "declared_vars": {
        "name": "\\improper Securitron", "icon_state": "secbot", "base_icon_state": "secbot",
        "maxHealth": 25, "health": 25, "security_mode_flags": 25, "bot_mode_flags": 16777207,
        "req_one_access": { "list": ["security"] }, "radio_channel": "Безопасность",
        "commissioned": 0,
        "ai_controller": { "path": "/datum/ai_controller/basic_controller/bot/secbot" } } },
    "/mob/living/basic/bot/secbot/beepsky": { "parent": "/mob/living/basic/bot/secbot",
        "declared_vars": { "name": "Commander Beep O'sky", "bot_mode_flags": 7, "commissioned": 1 } },
    "/mob/living/basic/bot/cleanbot": { "parent": "/mob/living/basic/bot", "declared_vars": {
        "name": "\\improper Cleanbot", "icon_state": "cleanbot0", "maxHealth": 25, "health": 25,
        "janitor_mode_flags": 1,
        "ai_controller": { "path": "/datum/ai_controller/basic_controller/bot/cleanbot" } } },
    "/mob/living/basic/bot/medbot": { "parent": "/mob/living/basic/bot", "declared_vars": {
        "name": "\\improper Medibot", "icon_state": "medbot_generic_idle", "base_icon_state": "medbot",
        "speed": 2, "heal_amount": 2.5, "heal_threshold": 10, "damage_type_healer": "brute",
        "medical_mode_flags": 5, "skin": "generic", "radio_channel": "Медицина",
        "ai_controller": { "path": "/datum/ai_controller/basic_controller/bot/medbot" } } },
    "/mob/living/basic/bot/firebot": { "parent": "/mob/living/basic/bot", "declared_vars": {
        "name": "\\improper Firebot", "icon_state": "firebot1",
        "ai_controller": { "path": "/datum/ai_controller/basic_controller/bot/firebot" } } },
    "/turf": { "parent": "/atom", "declared_vars": { "icon": { "resource": "t.dmi" } } },
    "/turf/open": { "parent": "/turf" },
    "/turf/open/floor": { "parent": "/turf/open", "declared_vars": { "icon_state": "floor" } },
    "/turf/closed": { "parent": "/turf", "declared_vars": { "density": 1, "opacity": 1 } },
    "/turf/closed/wall": { "parent": "/turf/closed", "declared_vars": { "icon_state": "wall" } },
    "/area": { "parent": "/atom" },
    "/area/space": { "parent": "/area" },
    "/area/station": { "parent": "/area" },
    "/area/station/bots_test": { "parent": "/area/station", "declared_vars": { "name": "Bot Lab" } }
  },
  "element_calls": { "/datum/element/undertile": [
    { "type": "/obj/machinery/navbeacon", "proc": "Initialize", "call": "AddElement", "line": "" }
  ] }
}"##;

const FACTIONS: &str = "#define FACTION_NEUTRAL \"neutral\"\n#define FACTION_SILICON \"silicon\"\n\
#define FACTION_TURRET \"turret\"\n";

/// Beepsky (`B`) next to beacon `1`; beacon `2` is behind an airlock only robotics opens.
const PATROL_MAP: &str = r##""w" = (/turf/closed/wall,/area/station/bots_test)
"f" = (/turf/open/floor,/area/station/bots_test)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/station/bots_test)
"B" = (/mob/living/basic/bot/secbot/beepsky,/turf/open/floor,/area/station/bots_test)
"1" = (/obj/machinery/navbeacon{codes_txt = "patrol;next_patrol=Two"; location = "One"},/turf/open/floor,/area/station/bots_test)
"2" = (/obj/machinery/navbeacon{codes_txt = "patrol;next_patrol=One"; location = "Two"},/turf/open/floor,/area/station/bots_test)
"D" = (/obj/machinery/door/airlock{normalspeed = 0; req_access = list("robotics")},/turf/open/floor,/area/station/bots_test)

(1,1,1) = {"
wwwwwwwwwwwwwwwwww
wsfffffwwwwwffffff
wBf1ffffffDfffff2w
wfffffwwwwwwffffff
wwwwwwwwwwwwwwwwww
"}
"##;

/// A room with the player on `s` and one bot of each test.
fn room(bot: &str, extra: &str) -> String {
    format!(
        r##""w" = (/turf/closed/wall,/area/station/bots_test)
"f" = (/turf/open/floor,/area/station/bots_test)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/station/bots_test)
"X" = ({bot},/turf/open/floor,/area/station/bots_test)
"d" = (/obj/effect/decal/cleanable/dirt,/turf/open/floor,/area/station/bots_test)

(1,1,1) = {{"
wwwwwwwwww
wsfffffffw
wfff{extra}ffffw
wffffXfffw
wwwwwwwwww
"}}
"##
    )
}

const STATES: [(&str, &str); 24] = [
    ("b.dmi", "medbot_assembly_generic"),
    ("b.dmi", "cleanbot_assembly"),
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
    ("e.dmi", "dirt"),
    ("n.dmi", "navbeacon0"),
    ("d.dmi", "door_closed"),
    ("a.dmi", "closed"),
    ("a.dmi", "opening"),
    ("a.dmi", "open"),
    ("a.dmi", "closing"),
    ("b.dmi", "secbot"),
    ("b.dmi", "secbot0"),
    ("b.dmi", "secbot1"),
    ("b.dmi", "secbot-c"),
    ("b.dmi", "cleanbot0"),
    ("b.dmi", "cleanbot1"),
    ("b.dmi", "cleanbot-c"),
    ("b.dmi", "medbot_generic_idle"),
    ("b.dmi", "medbot_burn_idle"),
    ("b.dmi", "firebot1"),
];

fn fixture(test: &str, map: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("bots_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/effect/decal/cleanable/dirt",
        "/obj/machinery/navbeacon",
        "/obj/machinery/door/airlock",
        "/obj/item/assembly/prox_sensor",
        "/obj/item/restraints/handcuffs/cable/zipties/used",
        "/obj/item/bodypart/arm/right/robot",
        "/obj/item/reagent_containers/cup/bucket",
        "/obj/item/storage/medkit/fire",
        "/obj/item/healthanalyzer",
        "/obj/item/bot_assembly/cleanbot",
        "/obj/item/bot_assembly/medbot",
        "/mob/living/basic/bot/secbot",
        "/mob/living/basic/bot/secbot/beepsky",
        "/mob/living/basic/bot/cleanbot",
        "/mob/living/basic/bot/medbot",
        "/mob/living/basic/bot/firebot",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/area/station/bots_test",
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

    fn mob_of(&mut self, path: &str) -> Entity {
        let world = self.0.server.world_mut();
        world
            .query::<(Entity, &DmMob)>()
            .iter(world)
            .find(|(_, m)| m.path == path)
            .map(|(e, _)| e)
            .unwrap_or_else(|| panic!("no {path}"))
    }

    fn pos(&self, e: Entity) -> TilePos {
        *self.get::<TilePos>(e)
    }

    fn bot(&self, e: Entity) -> &Bot {
        self.get::<Bot>(e)
    }

    fn bot_mut(&mut self, e: Entity) -> Mut<'_, Bot> {
        self.0.server.world_mut().get_mut::<Bot>(e).unwrap()
    }

    fn beacon(&mut self, location: &str) -> TilePos {
        let world = self.0.server.world_mut();
        world
            .query::<(&DmNavBeacon, &TilePos)>()
            .iter(world)
            .find(|(b, _)| b.location == location)
            .map(|(_, p)| *p)
            .unwrap()
    }

    /// Spawns `path` on `pos`.
    fn spawn(&mut self, path: &str, pos: TilePos) -> Entity {
        let world = self.0.server.world_mut();
        let e = ss13_game::construction::spawn_obj(world, path, pos).unwrap();
        world.flush();
        e
    }

    /// Spawns `path` into `p`'s active hand (emptied first).
    fn give(&mut self, p: usize, path: &str) -> Entity {
        let mob = self.players[p].mob;
        let pos = self.pos(mob);
        let item = self.spawn(path, pos);
        let world = self.0.server.world_mut();
        let hands = world.get::<Inventory>(mob).unwrap().hands;
        for held in hands.into_iter().flatten() {
            world.despawn(held);
        }
        let hand = world.get::<Inventory>(mob).unwrap().active;
        world
            .entity_mut(item)
            .remove::<TilePos>()
            .insert(CarriedBy(mob));
        world.get_mut::<Inventory>(mob).unwrap().hands[hand] = Some(item);
        item
    }

    /// Steps until `p`'s client has `e`.
    fn seen(&mut self, p: usize, e: Entity) {
        self.run_until("replicated", |s| s.net_id(p, e).is_some());
    }

    fn items_at(&mut self, path: &str) -> Vec<Entity> {
        let world = self.0.server.world_mut();
        world
            .query::<(Entity, &ss13_mapload::DmItem)>()
            .iter(world)
            .filter(|(_, i)| i.path == path)
            .map(|(e, _)| e)
            .collect()
    }

    fn mobs_of(&mut self, path: &str) -> Vec<Entity> {
        let world = self.0.server.world_mut();
        world
            .query::<(Entity, &DmMob)>()
            .iter(world)
            .filter(|(_, m)| m.path == path)
            .map(|(e, _)| e)
            .collect()
    }
}

#[test]
fn beepsky_patrols_between_beacons_through_an_access_door() {
    let mut s = Station::start("patrol", PATROL_MAP);
    let beepsky = s.mob_of("/mob/living/basic/bot/secbot/beepsky");
    assert!(s.bot(beepsky).on(), "mapped on");
    assert!(s.bot(beepsky).commissioned);
    let (one, two) = (s.beacon("One"), s.beacon("Two"));
    s.run_until_for(30.0, "Beepsky reaches the far beacon", |s| {
        s.pos(beepsky) == two
    });
    assert_eq!(s.bot(beepsky).mode, bots::BOT_PATROL);
    s.run_until_for(30.0, "and comes back to the first", |s| {
        s.pos(beepsky) == one
    });
}

#[test]
fn a_bot_that_is_off_stays_put() {
    let mut s = Station::start("off", PATROL_MAP);
    let beepsky = s.mob_of("/mob/living/basic/bot/secbot/beepsky");
    let p = s.join("p0", |_| {});
    let me = s.players[p].mob;
    // Locked: a player without access is refused; the controls stay as they were.
    bots::ui::act_on(s.0.server.world_mut(), me, beepsky, bots::ui::BotAct::Power);
    assert!(s.bot(beepsky).on(), "no access, no change");
    s.bot_mut(beepsky).cover &= !bots::COVER_LOCKED;
    bots::ui::act_on(s.0.server.world_mut(), me, beepsky, bots::ui::BotAct::Power);
    assert!(!s.bot(beepsky).on(), "unlocked: anyone turns it off");
    let at = s.pos(beepsky);
    s.hold(5.0, "an inactive bot does not patrol", |s| {
        s.pos(beepsky) == at
    });
}

#[test]
fn an_emagged_securitron_stuns_and_cuffs_a_player() {
    let map = room("/mob/living/basic/bot/secbot", "f");
    let mut s = Station::start("secbot", &map);
    let secbot = s.mob_of("/mob/living/basic/bot/secbot");
    let p = s.join("p0", |_| {});
    let me = s.players[p].mob;
    s.run_for(1.0);
    assert!(
        s.try_get::<ss13_game::restraints::Handcuffed>(me).is_none(),
        "no threat: left alone"
    );
    s.bot_mut(secbot).cover |= bots::COVER_EMAGGED;
    s.run_until_for(20.0, "stunned", |s| {
        s.try_get::<ss13_game::status_effects::StatusEffects>(me)
            .is_some_and(|e| e.paralyze > 0.0)
    });
    s.run_until_for(20.0, "zip-tied", |s| {
        s.try_get::<ss13_game::restraints::Handcuffed>(me).is_some()
    });
    assert!(s.pos(secbot).chebyshev(s.pos(me)).is_some_and(|d| d <= 1));
}

#[test]
fn a_cleanbot_cleans_dirt_nearby() {
    let map = room("/mob/living/basic/bot/cleanbot", "d");
    let mut s = Station::start("cleanbot", &map);
    let cleanbot = s.mob_of("/mob/living/basic/bot/cleanbot");
    let dirt = {
        let world = s.0.server.world_mut();
        world
            .query_filtered::<Entity, With<ss13_mapload::DmCleanable>>()
            .iter(world)
            .next()
            .unwrap()
    };
    let at = s.pos(dirt);
    s.run_until_for(30.0, "the dirt is cleaned", |s| {
        s.0.server.world().get_entity(dirt).is_err()
    });
    assert!(s.pos(cleanbot).chebyshev(at).is_some_and(|d| d <= 1));
    assert_eq!(s.bot(cleanbot).mode, bots::BOT_IDLE);
}

#[test]
fn a_medbot_tends_an_injured_player() {
    let map = room("/mob/living/basic/bot/medbot", "f");
    let mut s = Station::start("medbot", &map);
    let medbot = s.mob_of("/mob/living/basic/bot/medbot");
    let p = s.join("p0", |_| {});
    let me = s.players[p].mob;
    s.0.server
        .world_mut()
        .get_mut::<Health>(me)
        .unwrap()
        .adjust(DamageType::Brute, 30.0);
    s.run_until_for(60.0, "healed down to the threshold", |s| {
        s.get::<Health>(me).brute <= 10.0
    });
    assert!(s.pos(medbot).chebyshev(s.pos(me)).is_some_and(|d| d <= 1));
    assert_eq!(s.get::<Stat>(me), &Stat::Conscious);
    s.run_for(5.0);
    assert!(s.get::<Health>(me).brute > 5.0, "it stops at the threshold");
}

#[test]
fn a_firebot_puts_out_a_burning_player() {
    let map = room("/mob/living/basic/bot/firebot", "f");
    let mut s = Station::start("firebot", &map);
    let p = s.join("p0", |_| {});
    let me = s.players[p].mob;
    s.run_for(0.5);
    {
        let world = s.0.server.world_mut();
        ss13_game::fire::adjust_fire_stacks(world, me, 20.0);
        ss13_game::fire::ignite_mob(world, me);
    }
    assert!(ss13_game::fire::on_fire(s.0.server.world(), me));
    s.run_until_for(10.0, "put out", |s| {
        !ss13_game::fire::on_fire(s.0.server.world(), me)
    });
    assert_eq!(
        s.try_get::<ss13_game::fire::FireStacks>(me)
            .map_or(0.0, |f| f.stacks),
        0.0,
        "sprayed, not burnt out"
    );
}

#[test]
fn medbot_and_cleanbot_are_built_from_parts() {
    let map = room("/mob/living/basic/bot/firebot", "f");
    let mut s = Station::start("build", &map);
    let p = s.join("p0", |_| {});
    let me = s.players[p].mob;
    let floor = TilePos::new(s.pos(me).x + 1, s.pos(me).y, s.pos(me).z);
    // A robot arm on a burn kit, a health analyzer, a proximity sensor.
    let kit = s.spawn("/obj/item/storage/medkit/fire", floor);
    s.give(p, "/obj/item/bodypart/arm/right/robot");
    s.seen(p, kit);
    s.click(p, kit);
    s.run_until("the medibot assembly", |s| {
        !s.items_at("/obj/item/bot_assembly/medbot").is_empty()
    });
    let assembly = s.items_at("/obj/item/bot_assembly/medbot")[0];
    assert!(
        s.0.server.world().get_entity(kit).is_err(),
        "the kit is used"
    );
    s.give(p, "/obj/item/healthanalyzer");
    s.seen(p, assembly);
    s.click(p, assembly);
    s.run_until("analyzer added", |s| {
        s.try_get::<bots::construction::BotAssembly>(assembly)
            .is_some_and(|a| a.step == 2)
    });
    s.give(p, "/obj/item/assembly/prox_sensor");
    s.click(p, assembly);
    s.run_until("a medibot", |s| {
        !s.mobs_of("/mob/living/basic/bot/medbot").is_empty()
    });
    let medbot = s.mobs_of("/mob/living/basic/bot/medbot")[0];
    s.run_for(0.2);
    assert_eq!(s.bot(medbot).skin, "burn", "the kit's skin");
    assert_eq!(
        s.bot(medbot).healer,
        bots::Healer::Type(DamageType::Burn),
        "and its healing"
    );
    assert_eq!(s.pos(medbot), floor);
    // A proximity sensor on a bucket, then a robot arm.
    let bucket = s.spawn("/obj/item/reagent_containers/cup/bucket", floor);
    s.give(p, "/obj/item/assembly/prox_sensor");
    s.seen(p, bucket);
    s.click(p, bucket);
    s.run_until("the cleanbot assembly", |s| {
        !s.items_at("/obj/item/bot_assembly/cleanbot").is_empty()
    });
    let assembly = s.items_at("/obj/item/bot_assembly/cleanbot")[0];
    s.give(p, "/obj/item/bodypart/arm/right/robot");
    s.seen(p, assembly);
    s.click(p, assembly);
    s.run_until("a cleanbot", |s| {
        !s.mobs_of("/mob/living/basic/bot/cleanbot").is_empty()
    });
    let cleanbot = s.mobs_of("/mob/living/basic/bot/cleanbot")[0];
    s.run_for(0.2);
    assert_eq!(s.get::<Name>(cleanbot).as_str(), "Cleanbot");
    assert!(s.bot(cleanbot).on());
}

/// `SS13_ASSETS` must hold assets converted with this version (`mobs.ron` bot vars, navbeacon
/// codes). Loads `SS13_BOTS_MAP` (default MetaStation), prints the bots and the patrol beacons,
/// runs two minutes with a player and prints how far each bot got and the AI cost per pass.
#[test]
#[ignore = "needs generated assets and a station map; run with --ignored --nocapture"]
fn station_bots() {
    let map_name = std::env::var("SS13_BOTS_MAP").unwrap_or_else(|_| "MetaStation".into());
    let map = ss13_mapload::MapLoadPlugin {
        source: std::env::var("SS13_SOURCE")
            .unwrap_or_else(|_| env!("SS13_SOURCE").into())
            .into(),
        maps: vec![map_name.clone()],
        assets: std::env::var("SS13_ASSETS")
            .unwrap_or_else(|_| "assets/generated".into())
            .into(),
    };
    let mut s = Station(testing::Station::start(map, |_| {}));
    let bots: Vec<(Entity, String, TilePos)> = {
        let world = s.0.server.world_mut();
        world
            .query::<(Entity, &Bot, &Name, &TilePos)>()
            .iter(world)
            .map(|(e, _, n, p)| (e, n.as_str().to_owned(), *p))
            .collect()
    };
    let beacons = s.0.server.world().resource::<bots::NavBeacons>().0.len();
    println!("{map_name}: {} bots, {beacons} beacons", bots.len());
    assert!(!bots.is_empty(), "the map has bots");
    assert!(beacons > 0, "and beacons");
    s.join("p0", |_| {});
    let before =
        *s.0.server
            .world()
            .resource::<ss13_game::mobs::ai::AiStats>();
    s.run_for(120.0);
    let after =
        *s.0.server
            .world()
            .resource::<ss13_game::mobs::ai::AiStats>();
    let passes = after.passes - before.passes;
    println!(
        "{} controllers, {} on, {:.1} µs per AI pass ({passes} passes)",
        after.controllers,
        after.on,
        (after.pass_nanos - before.pass_nanos) as f64 / passes.max(1) as f64 / 1000.0,
    );
    let mut moved = 0;
    for (bot, name, from) in bots {
        let Some(now) = s.try_get::<TilePos>(bot).copied() else {
            println!("  {name}: gone");
            continue;
        };
        let b = s.bot(bot);
        println!(
            "  {name}: {from:?} -> {now:?}, {} ({}), mode flags {:#x}",
            b.mode,
            if b.on() { "on" } else { "off" },
            b.mode_flags
        );
        if now != from && b.mode_flags & bots::MODE_AUTOPATROL != 0 {
            moved += 1;
        }
    }
    assert!(moved > 0, "a patrolling bot left its spot");
}
