//! Megafauna on a fixture station over real WebTransport: the colossus shoots and strikes a
//! player who comes near and, killed, vanishes into its chest of loot; the ash drake burns,
//! swoops on or bites its target, shrugs off light explosions, is never gibbed and leaves its
//! corpse with a chest; Bubblegum charges its target, takes explosions as light ones and
//! leaves its sarcophagus. The ignored test places the bosses on the real Lavaland.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::explosions::Severity;
use ss13_game::explosions::ex_act::ex_act_mob;
use ss13_game::health::{Health, Stat};
use ss13_game::mobs::megafauna::{BUBBLEGUM, COLOSSUS, DRAKE, Megafauna};
use ss13_game::storage::StoredIn;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::{DmItem, DmMob, DmObj};
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
    "/obj/item/cain_and_abel": { "parent": "/obj/item", "declared_vars": { "name": "Cain and Abel" } },
    "/obj/item/organ": { "parent": "/obj/item" },
    "/obj/item/organ/vocal_cords": { "parent": "/obj/item/organ" },
    "/obj/item/organ/vocal_cords/colossus": { "parent": "/obj/item/organ/vocal_cords",
        "declared_vars": { "name": "divine vocal cords" } },
    "/obj/item/melee": { "parent": "/obj/item" },
    "/obj/item/melee/ghost_sword": { "parent": "/obj/item/melee", "declared_vars": { "name": "spectral blade" } },
    "/obj/item/lava_staff": { "parent": "/obj/item", "declared_vars": { "name": "staff of lava" } },
    "/obj/item/book": { "parent": "/obj/item" },
    "/obj/item/book/granter": { "parent": "/obj/item/book" },
    "/obj/item/book/granter/action": { "parent": "/obj/item/book/granter" },
    "/obj/item/book/granter/action/spell": { "parent": "/obj/item/book/granter/action" },
    "/obj/item/book/granter/action/spell/sacredflame": { "parent": "/obj/item/book/granter/action/spell" },
    "/obj/item/dragons_blood": { "parent": "/obj/item", "declared_vars": { "name": "bottle of dragons blood" } },
    "/obj/item/clothing/suit": { "parent": "/obj/item/clothing" },
    "/obj/item/clothing/suit/hooded": { "parent": "/obj/item/clothing/suit" },
    "/obj/item/clothing/suit/hooded/hostile_environment": { "parent": "/obj/item/clothing/suit/hooded" },
    "/obj/item/mayhem": { "parent": "/obj/item" },
    "/obj/item/soulscythe": { "parent": "/obj/item" },
    "/obj/item/food": { "parent": "/obj/item" },
    "/obj/item/food/bubblegum": { "parent": "/obj/item/food" },
    "/obj/item/food/bubblegum/bubblegum": { "parent": "/obj/item/food/bubblegum" },
    "/obj/projectile": { "parent": "/obj", "declared_vars": { "damage": 10,
        "damage_type": "brute", "armor_flag": "bullet", "speed": 1.25, "range": 50,
        "pass_flags": 1, "accurate_range": 100, "accuracy_falloff": 7,
        "icon": { "resource": "i.dmi" }, "icon_state": "toy" } },
    "/obj/projectile/colossus": { "parent": "/obj/projectile", "declared_vars": {
        "name": "death bolt", "damage": 25, "armour_penetration": 100, "speed": 0.5 } },
    "/obj/machinery": { "parent": "/obj", "declared_vars": { "anchored": 1, "density": 1 } },
    "/obj/machinery/anomalous_crystal": { "parent": "/obj/machinery", "declared_vars": {
        "name": "anomalous crystal", "icon": { "resource": "i.dmi" }, "icon_state": "toy" } },
    "/obj/machinery/anomalous_crystal/dark_reprise": { "parent": "/obj/machinery/anomalous_crystal" },
    "/obj/machinery/anomalous_crystal/emitter": { "parent": "/obj/machinery/anomalous_crystal" },
    "/obj/machinery/anomalous_crystal/helpers": { "parent": "/obj/machinery/anomalous_crystal" },
    "/obj/machinery/anomalous_crystal/honk": { "parent": "/obj/machinery/anomalous_crystal" },
    "/obj/machinery/anomalous_crystal/possessor": { "parent": "/obj/machinery/anomalous_crystal" },
    "/obj/machinery/anomalous_crystal/theme_warp": { "parent": "/obj/machinery/anomalous_crystal" },
    "/obj/structure": { "parent": "/obj", "declared_vars": { "anchored": 1 } },
    "/obj/structure/closet": { "parent": "/obj/structure", "declared_vars": {
        "icon": { "resource": "cr.dmi" }, "icon_state": "generic", "density": 1,
        "anchored": 0, "opened": 0, "locked": 0, "welded": 0, "secure": 0, "icon_door": null,
        "icon_welded": "welded", "enable_door_overlay": 1, "has_opened_overlay": 1,
        "has_closed_overlay": 1, "dense_when_open": 0, "wall_mounted": 0, "horizontal": 0,
        "storage_capacity": 30, "name": "closet" } },
    "/obj/structure/closet/crate": { "parent": "/obj/structure/closet", "declared_vars": {
        "icon_state": "crate", "base_icon_state": "crate", "dense_when_open": 1,
        "horizontal": 1, "name": "crate" } },
    "/obj/structure/closet/crate/necropolis": { "parent": "/obj/structure/closet/crate",
        "declared_vars": { "name": "necropolis chest", "icon_state": "necrocrate",
        "base_icon_state": "necrocrate" } },
    "/obj/structure/closet/crate/necropolis/colossus": {
        "parent": "/obj/structure/closet/crate/necropolis", "declared_vars": { "name": "colossus chest" } },
    "/obj/structure/closet/crate/necropolis/dragon": {
        "parent": "/obj/structure/closet/crate/necropolis", "declared_vars": { "name": "dragon chest" } },
    "/obj/structure/closet/crate/necropolis/bubblegum": {
        "parent": "/obj/structure/closet/crate/necropolis" },
    "/mob": { "parent": "/atom/movable", "declared_vars": { "layer": 4, "density": 1,
        "faction": { "expr": "list(FACTION_NEUTRAL)" } } },
    "/mob/living": { "parent": "/mob", "declared_vars": { "maxHealth": 100, "health": 100 } },
    "/mob/living/carbon": { "parent": "/mob/living" },
    "/mob/living/carbon/human": { "parent": "/mob/living/carbon", "declared_vars": {
        "icon": { "resource": "h.dmi" }, "icon_state": "human_basic" } },
    "/mob/living/simple_animal": { "parent": "/mob/living", "declared_vars": {
        "maxHealth": 20, "health": 20, "speed": 1, "minbodytemp": 250, "maxbodytemp": 350,
        "del_on_death": 0, "melee_damage_type": "brute",
        "attack_verb_continuous": "attacks", "attack_verb_simple": "attack",
        "response_harm_continuous": "hits", "response_harm_simple": "hit" } },
    "/mob/living/simple_animal/hostile": { "parent": "/mob/living/simple_animal" },
    "/mob/living/simple_animal/hostile/megafauna": { "parent": "/mob/living/simple_animal/hostile",
        "declared_vars": { "maxHealth": 1000, "health": 1000, "minbodytemp": 0, "maxbodytemp": 1e31,
        "faction": { "expr": "list(FACTION_MINING, FACTION_BOSS)" },
        "physiology": { "list": [ { "assoc": ["burn", 0.5] }, { "assoc": ["stamina", 0] } ] },
        "icon": { "resource": "m.dmi" } } },
    "/mob/living/simple_animal/hostile/megafauna/colossus": {
        "parent": "/mob/living/simple_animal/hostile/megafauna", "declared_vars": {
        "name": "colossus", "maxHealth": 2500, "health": 2500, "melee_damage_lower": 40,
        "melee_damage_upper": 40, "speed": 10, "del_on_death": 1, "attack_verb_continuous": "judges",
        "attack_verb_simple": "judge", "speak_emote": { "list": ["roars"] },
        "icon_state": "eva", "icon_living": "eva", "icon_dead": "" } },
    "/mob/living/simple_animal/hostile/megafauna/dragon": {
        "parent": "/mob/living/simple_animal/hostile/megafauna", "declared_vars": {
        "name": "ash drake", "maxHealth": 2500, "health": 2500, "melee_damage_lower": 40,
        "melee_damage_upper": 40, "speed": 5, "attack_verb_continuous": "chomps",
        "attack_verb_simple": "chomp", "icon_state": "dragon", "icon_living": "dragon",
        "icon_dead": "dragon_dead" } },
    "/mob/living/simple_animal/hostile/megafauna/bubblegum": {
        "parent": "/mob/living/simple_animal/hostile/megafauna", "declared_vars": {
        "name": "bubblegum", "maxHealth": 2500, "health": 2500, "melee_damage_lower": 40,
        "melee_damage_upper": 40, "speed": 5, "del_on_death": 1, "attack_verb_continuous": "rends",
        "attack_verb_simple": "rend", "speak_emote": { "list": ["gurgles"] },
        "icon_state": "bubblegum", "icon_living": "bubblegum", "icon_dead": "" } },
    "/mob/living/simple_animal/hostile/megafauna/legion": {
        "parent": "/mob/living/simple_animal/hostile/megafauna", "declared_vars": {
        "name": "Legion", "maxHealth": 700, "health": 700, "melee_damage_lower": 25,
        "melee_damage_upper": 25, "speed": 5, "del_on_death": 1, "icon_state": "mega_legion",
        "icon_living": "mega_legion", "icon_dead": "" } },
    "/mob/living/simple_animal/hostile/megafauna/legion/medium": {
        "parent": "/mob/living/simple_animal/hostile/megafauna/legion",
        "declared_vars": { "maxHealth": 350 } },
    "/mob/living/simple_animal/hostile/megafauna/legion/medium/left": {
        "parent": "/mob/living/simple_animal/hostile/megafauna/legion/medium" },
    "/mob/living/simple_animal/hostile/megafauna/legion/medium/right": {
        "parent": "/mob/living/simple_animal/hostile/megafauna/legion/medium" },
    "/mob/living/simple_animal/hostile/megafauna/legion/medium/eye": {
        "parent": "/mob/living/simple_animal/hostile/megafauna/legion/medium" },
    "/mob/living/simple_animal/hostile/megafauna/legion/small": {
        "parent": "/mob/living/simple_animal/hostile/megafauna/legion",
        "declared_vars": { "maxHealth": 200 } },
    "/mob/living/simple_animal/hostile/megafauna/clockwork_defender": {
        "parent": "/mob/living/simple_animal/hostile/megafauna", "declared_vars": {
        "name": "clockwork defender", "maxHealth": 300, "health": 300, "melee_damage_lower": 20,
        "melee_damage_upper": 20, "speed": 5, "del_on_death": 1,
        "icon_state": "clockwork_defender", "icon_living": "clockwork_defender", "icon_dead": "" } },
    "/mob/living/simple_animal/hostile/megafauna/hierophant": {
        "parent": "/mob/living/simple_animal/hostile/megafauna", "declared_vars": {
        "name": "hierophant", "maxHealth": 2500, "health": 2500, "melee_damage_lower": 15,
        "melee_damage_upper": 15, "speed": 10, "del_on_death": 1,
        "icon_state": "hierophant", "icon_living": "hierophant", "icon_dead": "" } },
    "/obj/effect/hierophant": { "parent": "/obj/effect", "declared_vars": {
        "name": "hierophant beacon", "icon": { "resource": "i.dmi" }, "icon_state": "toy",
        "anchored": 1 } },
    "/obj/item/hierophant_club": { "parent": "/obj/item", "declared_vars": { "name": "hierophant club" } },
    "/obj/item/storm_staff": { "parent": "/obj/item", "declared_vars": { "name": "staff of storms" } },
    "/obj/item/clockwork_alloy": { "parent": "/obj/item", "declared_vars": { "name": "clockwork alloy" } },
    "/obj/structure/closet/crate/necropolis/tendril": {
        "parent": "/obj/structure/closet/crate/necropolis" },
    "/turf": { "parent": "/atom", "declared_vars": { "icon": { "resource": "t.dmi" } } },
    "/turf/open": { "parent": "/turf" },
    "/turf/open/floor": { "parent": "/turf/open", "declared_vars": { "icon_state": "floor" } },
    "/turf/closed": { "parent": "/turf", "declared_vars": { "density": 1, "opacity": 1 } },
    "/turf/closed/wall": { "parent": "/turf/closed", "declared_vars": { "icon_state": "wall" } },
    "/area": { "parent": "/atom" },
    "/area/space": { "parent": "/area" },
    "/area/lavaland_test": { "parent": "/area" }
  }
}"##;

const FACTIONS: [(&str, &str); 3] = [
    ("FACTION_NEUTRAL", "neutral"),
    ("FACTION_MINING", "mining"),
    ("FACTION_BOSS", "boss"),
];

/// Rows top-down, the last line is y = 1. `s` the player (3,4), `C` the colossus (8,4), five
/// tiles away (its idle vision).
const COLOSSUS_MAP: &str = r##""w" = (/turf/closed/wall,/area/lavaland_test)
"f" = (/turf/open/floor,/area/lavaland_test)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/lavaland_test)
"C" = (/mob/living/simple_animal/hostile/megafauna/colossus,/turf/open/floor,/area/lavaland_test)

(1,1,1) = {"
wwwwwwwwwwwwwwww
wffffffffffffffw
wffffffffffffffw
wfsffffCfffffffw
wffffffffffffffw
wffffffffffffffw
wwwwwwwwwwwwwwww
"}
"##;

/// `s` the player (3,4), `D` the drake (6,4).
const DRAKE_MAP: &str = r##""w" = (/turf/closed/wall,/area/lavaland_test)
"f" = (/turf/open/floor,/area/lavaland_test)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/lavaland_test)
"D" = (/mob/living/simple_animal/hostile/megafauna/dragon,/turf/open/floor,/area/lavaland_test)

(1,1,1) = {"
wwwwwwwwwwwwwwww
wffffffffffffffw
wffffffffffffffw
wfsffDfffffffffw
wffffffffffffffw
wffffffffffffffw
wwwwwwwwwwwwwwww
"}
"##;

/// `s` the player (3,4), `B` Bubblegum (8,4).
const BUBBLEGUM_MAP: &str = r##""w" = (/turf/closed/wall,/area/lavaland_test)
"f" = (/turf/open/floor,/area/lavaland_test)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/lavaland_test)
"B" = (/mob/living/simple_animal/hostile/megafauna/bubblegum,/turf/open/floor,/area/lavaland_test)

(1,1,1) = {"
wwwwwwwwwwwwwwww
wffffffffffffffw
wffffffffffffffw
wfsffffBfffffffw
wffffffffffffffw
wffffffffffffffw
wwwwwwwwwwwwwwww
"}
"##;

/// `L` Legion (8,4), `K` the clockwork defender (12,2); no player.
const LEGION_MAP: &str = r##""w" = (/turf/closed/wall,/area/lavaland_test)
"f" = (/turf/open/floor,/area/lavaland_test)
"L" = (/mob/living/simple_animal/hostile/megafauna/legion,/turf/open/floor,/area/lavaland_test)
"K" = (/mob/living/simple_animal/hostile/megafauna/clockwork_defender,/turf/open/floor,/area/lavaland_test)

(1,1,1) = {"
wwwwwwwwwwwwwwww
wffffffffffffffw
wffffffffffffffw
wffffffLfffffffw
wffffffffffffffw
wfffffffffffKffw
wwwwwwwwwwwwwwww
"}
"##;

/// `s` the player (3,4), `H` the hierophant (7,4).
const HIEROPHANT_MAP: &str = r##""w" = (/turf/closed/wall,/area/lavaland_test)
"f" = (/turf/open/floor,/area/lavaland_test)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/lavaland_test)
"H" = (/mob/living/simple_animal/hostile/megafauna/hierophant,/turf/open/floor,/area/lavaland_test)

(1,1,1) = {"
wwwwwwwwwwwwwwww
wffffffffffffffw
wffffffffffffffw
wfsfffHffffffffw
wffffffffffffffw
wffffffffffffffw
wwwwwwwwwwwwwwww
"}
"##;

const STATES: [(&str, &str); 14] = [
    ("m.dmi", "hierophant"),
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("m.dmi", "eva"),
    ("m.dmi", "dragon"),
    ("m.dmi", "dragon_dead"),
    ("m.dmi", "bubblegum"),
    ("m.dmi", "mega_legion"),
    ("m.dmi", "clockwork_defender"),
    ("i.dmi", "toy"),
    ("cr.dmi", "necrocrate"),
    ("cr.dmi", "necrocrateopen"),
];

const USED: [&str; 44] = [
    "/mob/living/simple_animal/hostile/megafauna/hierophant",
    "/obj/effect/hierophant",
    "/obj/item/hierophant_club",
    "/obj/effect/landmark/start/assistant",
    "/turf/open/floor",
    "/turf/closed/wall",
    "/area/lavaland_test",
    "/area/space",
    COLOSSUS,
    DRAKE,
    "/obj/projectile/colossus",
    "/obj/structure/closet/crate/necropolis/colossus",
    "/obj/structure/closet/crate/necropolis/dragon",
    "/obj/item/organ/vocal_cords/colossus",
    "/obj/item/cain_and_abel",
    "/obj/machinery/anomalous_crystal/dark_reprise",
    "/obj/machinery/anomalous_crystal/emitter",
    "/obj/machinery/anomalous_crystal/helpers",
    "/obj/machinery/anomalous_crystal/honk",
    "/obj/machinery/anomalous_crystal/possessor",
    "/obj/machinery/anomalous_crystal/theme_warp",
    "/obj/item/melee/ghost_sword",
    "/obj/item/lava_staff",
    "/obj/item/book/granter/action/spell/sacredflame",
    "/obj/item/dragons_blood",
    BUBBLEGUM,
    "/obj/structure/closet/crate/necropolis/bubblegum",
    "/obj/item/clothing/suit/hooded/hostile_environment",
    "/obj/item/mayhem",
    "/obj/item/soulscythe",
    "/obj/item/food/bubblegum/bubblegum",
    "/mob/living/simple_animal/hostile/megafauna/legion",
    "/mob/living/simple_animal/hostile/megafauna/legion/medium/left",
    "/mob/living/simple_animal/hostile/megafauna/legion/medium/right",
    "/mob/living/simple_animal/hostile/megafauna/legion/medium/eye",
    "/mob/living/simple_animal/hostile/megafauna/legion/small",
    "/mob/living/simple_animal/hostile/megafauna/clockwork_defender",
    "/obj/item/storm_staff",
    "/obj/item/clockwork_alloy",
    "/obj/structure/closet/crate/necropolis/tendril",
    "/mob/living/carbon/human",
    "/obj/item/card/id/advanced",
    "/obj/item/clothing/under/color/grey",
    "/obj/item/clothing/shoes/sneakers/black",
];

fn fixture(test: &str, map: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("megafauna_fixture")
        .join(test);
    let assets = write_fixture(&dir, TYPES, &USED, &STATES, &[("map.dmm", map)]);
    let types = ss13_mapload::convert::DmTypes::parse(TYPES.as_bytes()).unwrap();
    let defines: std::collections::HashMap<String, String> = FACTIONS
        .iter()
        .map(|&(k, v)| (k.to_owned(), v.to_owned()))
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

    fn kill(&mut self, mob: Entity) {
        self.0.server.world_mut().entity_mut(mob).insert(Stat::Dead);
        self.run_for(0.5);
    }

    /// The chest of `path` and the paths of what it holds.
    fn chest(&mut self, path: &str) -> (TilePos, Vec<String>) {
        let world = self.0.server.world_mut();
        let (chest, at) = world
            .query::<(Entity, &DmObj, &TilePos)>()
            .iter(world)
            .find(|(_, o, _)| o.path == path)
            .map(|(e, _, p)| (e, *p))
            .unwrap_or_else(|| panic!("no {path}"));
        let inside = world
            .query::<(&StoredIn, Option<&DmItem>, Option<&DmObj>)>()
            .iter(world)
            .filter(|(s, ..)| s.0 == chest)
            .filter_map(|(_, i, o)| {
                i.map(|i| i.path.clone())
                    .or_else(|| o.map(|o| o.path.clone()))
            })
            .collect();
        (at, inside)
    }
}

#[test]
fn colossus_shoots_its_target_and_leaves_its_chest() {
    let mut s = Station::start("colossus", COLOSSUS_MAP);
    let colossus = s.mob_of(COLOSSUS);
    assert!(s.try_get::<Megafauna>(colossus).is_some(), "a megafauna");
    assert_eq!(s.get::<Health>(colossus).max, 2500.0);
    let p = s.join("p0", |_| {});
    let me = s.players[p].mob;
    s.run_until_for(30.0, "the colossus hurts its target", |s| {
        s.get::<Health>(me).brute > 0.0
    });
    assert_eq!(s.get::<Megafauna>(colossus).target, Some(me));
    let at = s.pos(colossus);
    s.kill(colossus);
    assert!(
        s.0.server.world().get_entity(colossus).is_err(),
        "del_on_death"
    );
    let (chest, inside) = s.chest("/obj/structure/closet/crate/necropolis/colossus");
    assert!(
        chest.chebyshev(at).is_some_and(|d| d <= 1),
        "{chest:?} {at:?}"
    );
    assert!(inside.contains(&"/obj/item/organ/vocal_cords/colossus".to_owned()));
    assert!(inside.contains(&"/obj/item/cain_and_abel".to_owned()));
    assert!(
        inside
            .iter()
            .any(|p| p.starts_with("/obj/machinery/anomalous_crystal/")),
        "{inside:?}"
    );
}

#[test]
fn ash_drake_burns_its_target_and_leaves_its_corpse_and_chest() {
    let mut s = Station::start("drake", DRAKE_MAP);
    let drake = s.mob_of(DRAKE);
    {
        let world = s.0.server.world_mut();
        ex_act_mob(world, drake, Severity::Light);
        assert_eq!(world.get::<Health>(drake).unwrap().health(), 2500.0);
        ex_act_mob(world, drake, Severity::Devastate);
        assert_eq!(world.get::<Health>(drake).unwrap().health(), 2250.0);
    }
    s.run_for(0.2);
    assert_eq!(s.get::<Stat>(drake), &Stat::Conscious, "never gibbed");
    let p = s.join("p0", |_| {});
    let me = s.players[p].mob;
    s.run_until_for(30.0, "the drake hurts its target", |s| {
        let h = s.get::<Health>(me);
        h.brute + h.burn > 0.0
    });
    s.kill(drake);
    assert_eq!(s.get::<Stat>(drake), &Stat::Dead, "the corpse stays");
    let at = s.pos(drake);
    let (chest, inside) = s.chest("/obj/structure/closet/crate/necropolis/dragon");
    assert!(
        chest.chebyshev(at).is_some_and(|d| d <= 1),
        "{chest:?} {at:?}"
    );
    assert_eq!(inside.len(), 1, "{inside:?}");
}

#[test]
fn bubblegum_charges_its_target_and_leaves_its_sarcophagus() {
    let mut s = Station::start("bubblegum", BUBBLEGUM_MAP);
    let gum = s.mob_of(BUBBLEGUM);
    {
        let world = s.0.server.world_mut();
        ex_act_mob(world, gum, Severity::Light);
        assert_eq!(world.get::<Health>(gum).unwrap().health(), 2500.0);
        // Puny mortals: a devastating blast counts as a light one.
        ex_act_mob(world, gum, Severity::Devastate);
        assert_eq!(world.get::<Health>(gum).unwrap().health(), 2450.0);
    }
    let p = s.join("p0", |_| {});
    let me = s.players[p].mob;
    s.run_until_for(30.0, "bubblegum hurts its target", |s| {
        s.get::<Health>(me).brute > 0.0
    });
    assert_eq!(s.get::<Megafauna>(gum).target, Some(me));
    let at = s.pos(gum);
    s.kill(gum);
    assert!(s.0.server.world().get_entity(gum).is_err(), "del_on_death");
    let (chest, inside) = s.chest("/obj/structure/closet/crate/necropolis/bubblegum");
    assert!(
        chest.chebyshev(at).is_some_and(|d| d <= 1),
        "{chest:?} {at:?}"
    );
    assert_eq!(inside.len(), 2, "{inside:?}");
    assert!(inside.contains(&"/obj/item/clothing/suit/hooded/hostile_environment".to_owned()));
}

#[test]
fn legion_splits_twice_and_the_last_skull_drops_the_storm_staff() {
    use ss13_game::mobs::megafauna::{Kind, legion};
    let mut s = Station::start("legion", LEGION_MAP);
    let big = s.mob_of(legion::LEGION);
    assert_eq!(s.get::<Megafauna>(big).kind, Kind::Legion);
    assert_eq!(s.get::<Megafauna>(big).legion.size, 3);
    let living = |s: &mut Station| -> Vec<(Entity, u8)> {
        let world = s.0.server.world_mut();
        world
            .query::<(Entity, &Megafauna, &Stat)>()
            .iter(world)
            .filter(|(_, m, st)| m.kind == Kind::Legion && **st != Stat::Dead)
            .map(|(e, m, _)| (e, m.legion.size))
            .collect()
    };
    s.kill(big);
    let medium = living(&mut s);
    assert_eq!(medium.len(), 3, "{medium:?}");
    assert!(medium.iter().all(|&(_, size)| size == 2));
    assert_eq!(s.get::<Health>(medium[0].0).max, 350.0);
    for (e, _) in medium {
        s.kill(e);
    }
    let small = living(&mut s);
    assert_eq!(small.len(), 9, "{small:?}");
    assert!(small.iter().all(|&(_, size)| size == 1));
    let objs = |s: &mut Station, path: &str| -> usize {
        let world = s.0.server.world_mut();
        world
            .query::<(Option<&DmItem>, Option<&DmObj>)>()
            .iter(world)
            .filter(|(i, o)| i.is_some_and(|i| i.path == path) || o.is_some_and(|o| o.path == path))
            .count()
    };
    assert_eq!(objs(&mut s, legion::STORM_STAFF), 0, "not the last one yet");
    for (e, _) in small {
        s.kill(e);
    }
    assert!(living(&mut s).is_empty());
    assert_eq!(
        objs(&mut s, legion::STORM_STAFF),
        1,
        "the last skull's staff"
    );
    // The clockwork defender crumbles into its alloy.
    let knight = s.mob_of("/mob/living/simple_animal/hostile/megafauna/clockwork_defender");
    assert_eq!(s.get::<Megafauna>(knight).kind, Kind::ClockworkDefender);
    s.kill(knight);
    assert!(
        s.0.server.world().get_entity(knight).is_err(),
        "del_on_death"
    );
    assert_eq!(objs(&mut s, "/obj/item/clockwork_alloy"), 1);
}

#[test]
#[ignore = "needs generated assets, MetaStation and Lavaland; run with --ignored"]
fn bosses_roam_lavaland() {
    use ss13_mapload::{LAVALAND_MAP, MapLoadPlugin, MiningLevel, mining_z};
    let map = MapLoadPlugin {
        maps: vec!["MetaStation".into()],
        ..MapLoadPlugin::from_env().unwrap_or(MapLoadPlugin {
            source: env!("SS13_SOURCE").into(),
            maps: vec![],
            assets: std::env::var("SS13_ASSETS")
                .unwrap_or_else(|_| "assets/generated".into())
                .into(),
        })
    };
    let lavaland = map.source.join(LAVALAND_MAP);
    let mut app = testing::server_app(map, |app| {
        app.insert_resource(MiningLevel(lavaland));
    });
    app.update();
    let world = app.world_mut();
    let z = mining_z(world).expect("the mining level loaded");
    let bosses: Vec<(TilePos, String)> = world
        .query::<(&TilePos, &DmMob, &Megafauna)>()
        .iter(world)
        .map(|(p, m, _)| (*p, m.path.clone()))
        .collect();
    println!("{} megafauna: {bosses:?}", bosses.len());
    assert!(!bosses.is_empty(), "no megafauna on lavaland");
    assert!(bosses.iter().all(|(p, _)| p.z == z));
}

/// tg `hierophant.dm`: its beacon where it spawned, vortex blasts burn its target, and dying it
/// leaves its club and takes the beacon along.
#[test]
fn hierophant_blasts_its_target_and_leaves_its_club() {
    use ss13_game::mobs::megafauna::hierophant::{CLUB, HIEROPHANT};
    let mut s = Station::start("hierophant", HIEROPHANT_MAP);
    let boss = s.mob_of(HIEROPHANT);
    assert!(s.try_get::<Megafauna>(boss).is_some(), "a megafauna");
    let start = s.pos(boss);
    // The beacon is an effect (no `DmObj`): found through the boss's own reference.
    let beacon_at = |s: &mut Station| {
        let beacon = s.try_get::<Megafauna>(boss)?.hierophant.beacon?;
        s.try_get::<TilePos>(beacon).copied()
    };
    assert_eq!(beacon_at(&mut s), Some(start), "its home beacon");
    let p = s.join("p0", |_| {});
    let me = s.players[p].mob;
    s.run_until_for(40.0, "a vortex blast burns its target", |s| {
        s.get::<Health>(me).burn > 0.0
    });
    let at = s.pos(boss);
    s.kill(boss);
    assert!(s.0.server.world().get_entity(boss).is_err(), "del_on_death");
    assert_eq!(beacon_at(&mut s), None, "the beacon goes with it");
    let world = s.0.server.world_mut();
    let club = world
        .query::<(&DmItem, &TilePos)>()
        .iter(world)
        .any(|(i, p)| i.path == CLUB && *p == at);
    assert!(club, "its club where it died");
}
