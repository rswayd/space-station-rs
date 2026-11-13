//! The repairbot on a fixture room: it takes the iron sheets and floor tiles lying around, tiles
//! the plating, plates the girder into a wall with two sheets, and its window hands the rest of
//! the sheets back to a player.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::bots::repair::Slot;
use ss13_game::bots::{self, Bot};
use ss13_game::inventory::Inventory;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_game::tools::Stack;
use ss13_mapload::{DmItem, DmMob, DmTurfBuild};
use ss13_shared::TilePos;

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/datum/armor": { "parent": "/datum", "declared_vars": { "melee": { "expr": "0" } } },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 }, "declared_vars": { "can_atmos_pass": 1,
        "flags_1": 0, "max_integrity": 500, "armor_type": { "expr": "/datum/armor" },
        "integrity_failure": 0, "damage_deflection": 0, "resistance_flags": 0 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3, "anchored": 0 } },
    "/obj": { "parent": "/atom/movable", "declared_vars": { "force": 0, "damtype": "brute",
        "obj_flags": 2 } },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start" },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null, "attack_speed": 8,
        "tool_behaviour": null, "toolspeed": 1, "heat": 0 } },
    "/obj/item/stack": { "parent": "/obj/item", "declared_vars": { "amount": 1,
        "max_amount": 50, "merge_type": null, "singular_name": null, "turf_type": null } },
    "/obj/item/stack/sheet": { "parent": "/obj/item/stack" },
    "/obj/item/stack/sheet/iron": { "parent": "/obj/item/stack/sheet", "declared_vars": {
        "name": "iron", "icon_state": "sheet-metal",
        "merge_type": { "path": "/obj/item/stack/sheet/iron" } } },
    "/obj/item/stack/tile": { "parent": "/obj/item/stack", "declared_vars": {
        "max_amount": 60 } },
    "/obj/item/stack/tile/iron": { "parent": "/obj/item/stack/tile", "declared_vars": {
        "name": "floor tile", "icon_state": "tile",
        "turf_type": { "path": "/turf/open/floor/iron" },
        "merge_type": { "path": "/obj/item/stack/tile/iron" } } },
    "/obj/item/storage": { "parent": "/obj/item" },
    "/obj/item/storage/toolbox": { "parent": "/obj/item/storage", "declared_vars": {
        "name": "toolbox", "icon_state": "toolbox" } },
    "/obj/item/storage/toolbox/mechanical": { "parent": "/obj/item/storage/toolbox" },
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
    "/obj/structure": { "parent": "/obj", "declared_vars": { "anchored": 1,
        "max_integrity": 300, "icon": { "resource": "s.dmi" } } },
    "/obj/structure/girder": { "parent": "/obj/structure", "declared_vars": {
        "name": "girder", "icon_state": "girder", "density": 1, "state": 0,
        "stack_type": { "path": "/obj/item/stack/sheet/iron" }, "stack_amount": 2,
        "can_displace": 1, "max_integrity": 200 } },
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
    "/mob/living/basic/bot/repairbot": { "parent": "/mob/living/basic/bot", "declared_vars": {
        "name": "\\improper Repairbot", "icon_state": "repairbot1",
        "base_icon_state": "repairbot", "maxHealth": 35, "health": 35,
        "ai_controller": { "path": "/datum/ai_controller/basic_controller/bot/repairbot" } } },
    "/turf": { "parent": "/atom", "declared_vars": { "icon": { "resource": "t.dmi" },
        "blocks_air": 0, "can_atmos_pass": 0, "initial_gas_mix": "o2=22;n2=82;TEMP=293.15",
        "temperature": 293.15, "baseturfs": { "path": "/turf/baseturf_bottom" } } },
    "/turf/open": { "parent": "/turf", "declared_vars": { "can_atmos_pass": -1,
        "planetary_atmos": 0 } },
    "/turf/open/floor": { "parent": "/turf/open", "declared_vars": { "icon_state": "floor",
        "baseturfs": { "path": "/turf/open/floor/plating" }, "overfloor_placed": 1,
        "floor_tile": null } },
    "/turf/open/floor/iron": { "parent": "/turf/open/floor" },
    "/turf/open/floor/plating": { "parent": "/turf/open/floor", "declared_vars": {
        "icon_state": "plating", "overfloor_placed": 0,
        "baseturfs": { "path": "/turf/baseturf_bottom" } } },
    "/turf/open/space": { "parent": "/turf/open", "declared_vars": { "temperature": 2.7,
        "initial_gas_mix": "TEMP=2.7" } },
    "/turf/open/space/basic": { "parent": "/turf/open/space" },
    "/turf/closed": { "parent": "/turf", "declared_vars": { "density": 1, "opacity": 1,
        "blocks_air": 1 } },
    "/turf/closed/wall": { "parent": "/turf/closed", "declared_vars": { "icon_state": "wall",
        "baseturfs": { "path": "/turf/open/floor/plating" },
        "sheet_type": { "path": "/obj/item/stack/sheet/iron" }, "sheet_amount": 2,
        "girder_type": { "path": "/obj/structure/girder" }, "slicing_duration": 100 } },
    "/area": { "parent": "/atom" },
    "/area/space": { "parent": "/area" },
    "/area/station": { "parent": "/area" },
    "/area/station/repair_test": { "parent": "/area/station", "declared_vars": {
        "name": "Repair Lab" } }
  }
}"##;

const FACTIONS: &str = "#define FACTION_NEUTRAL \"neutral\"\n#define FACTION_SILICON \"silicon\"\n\
#define FACTION_TURRET \"turret\"\n";

/// Rows top-down, the last line is y = 1: the repairbot `R` (2,3), ten iron sheets `I` (4,3), a
/// girder `G` (6,3), ten floor tiles `T` (3,2), plating `p` (5,2), the spawn `s` (6,2).
const MAP: &str = r##""w" = (/turf/closed/wall,/area/station/repair_test)
"f" = (/turf/open/floor/iron,/area/station/repair_test)
"p" = (/turf/open/floor/plating,/area/station/repair_test)
"R" = (/mob/living/basic/bot/repairbot,/turf/open/floor/iron,/area/station/repair_test)
"I" = (/obj/item/stack/sheet/iron{amount = 10},/turf/open/floor/iron,/area/station/repair_test)
"T" = (/obj/item/stack/tile/iron{amount = 10},/turf/open/floor/iron,/area/station/repair_test)
"G" = (/obj/structure/girder,/turf/open/floor/iron,/area/station/repair_test)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor/iron,/area/station/repair_test)

(1,1,1) = {"
wwwwwww
wRfIfGw
wfTfpsw
wwwwwww
"}
"##;

const GIRDER_AT: TilePos = TilePos::new(6, 3, 1);
const PLATING_AT: TilePos = TilePos::new(5, 2, 1);
const IRON: &str = "/obj/item/stack/sheet/iron";
const TILE: &str = "/obj/item/stack/tile/iron";

const STATES: [(&str, &str); 12] = [
    ("t.dmi", "floor"),
    ("t.dmi", "plating"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
    ("i.dmi", "sheet-metal"),
    ("i.dmi", "tile"),
    ("i.dmi", "toolbox"),
    ("s.dmi", "girder"),
    ("b.dmi", "repairbot0"),
    ("b.dmi", "repairbot1"),
];

fn fixture(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("repairbot_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/item/stack/sheet/iron",
        "/obj/item/stack/tile/iron",
        "/obj/item/storage/toolbox/mechanical",
        "/obj/structure/girder",
        "/mob/living/basic/bot/repairbot",
        "/turf/open/floor/iron",
        "/turf/open/floor/plating",
        "/turf/closed/wall",
        "/area/station/repair_test",
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

impl Station {
    fn start(test: &str) -> Self {
        Self(testing::Station::start(
            fixture_map(&fixture(test), &["map.dmm"]),
            |_| {},
        ))
    }

    fn repairbot(&mut self) -> Entity {
        let world = self.0.server.world_mut();
        world
            .query::<(Entity, &DmMob)>()
            .iter(world)
            .find(|(_, m)| m.path == "/mob/living/basic/bot/repairbot")
            .map(|(e, _)| e)
            .expect("the repairbot")
    }

    fn turf_path(&self, pos: TilePos) -> String {
        let world = self.0.server.world();
        ss13_game::construction::turf_at(world, pos)
            .and_then(|t| world.get::<DmTurfBuild>(t))
            .map(|b| b.path.clone())
            .unwrap_or_default()
    }

    fn loose(&mut self, path: &str) -> usize {
        let world = self.0.server.world_mut();
        world
            .query_filtered::<&DmItem, With<TilePos>>()
            .iter(world)
            .filter(|i| i.path == path)
            .count()
    }
}

#[test]
fn a_repairbot_gathers_stacks_tiles_plating_and_plates_a_girder() {
    let mut s = Station::start("repair");
    let bot = s.repairbot();
    let stock = s.get::<Bot>(bot).repair.clone().expect("a repairbot stock");
    assert_eq!(stock.rods, 50, "a full stack of rods to start");
    s.run_until_for(60.0, "the girder becomes a wall", |s| {
        s.turf_path(GIRDER_AT) == "/turf/closed/wall"
    });
    s.run_until_for(30.0, "the plating gets a tile", |s| {
        s.turf_path(PLATING_AT) == "/turf/open/floor/iron"
    });
    assert_eq!(s.loose(IRON), 0, "the sheets were taken");
    assert_eq!(s.loose(TILE), 0, "the tiles were taken");
    let stock = s.get::<Bot>(bot).repair.clone().unwrap();
    assert_eq!(stock.amount(Slot::Iron), 8, "two sheets went into the wall");
    assert_eq!(
        stock.amount(Slot::Tiles),
        9,
        "one tile went onto the plating"
    );

    // Its window hands the sheets back to a player once the controls are unlocked.
    let p = s.join("p0", |_| {});
    let me = s.players[p].mob;
    s.0.server.world_mut().get_mut::<Bot>(bot).unwrap().cover &= !bots::COVER_LOCKED;
    bots::ui::act_on(
        s.0.server.world_mut(),
        me,
        bot,
        bots::ui::BotAct::RemoveItem("iron".to_owned()),
    );
    let held = s
        .get::<Inventory>(me)
        .active_item()
        .expect("the sheets in hand");
    assert_eq!(s.get::<DmItem>(held).path, IRON);
    assert_eq!(s.get::<Stack>(held).amount, 8);
    assert!(s.get::<Bot>(bot).repair.as_ref().unwrap().iron.is_none());
}
