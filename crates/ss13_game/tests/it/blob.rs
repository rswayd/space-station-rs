//! The blob on a fixture station (`ss13_game::antag::blob`): an overmind places its core and its
//! blob grows by the core's pulses; by hand it grows a tile (4 resources), hardens it (15) and
//! takes the strong blob back (4); the structures take damage through their resistances; a
//! factory makes a spore of the blob; the core destroyed, the overmind dies and its blob goes
//! dead; a host pops into an overmind with its core where it stood.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::antag::blob::minions::BlobMinion;
use ss13_game::antag::blob::strains::{Flag, Strain};
use ss13_game::antag::blob::structures::{self, Blob, Kind};
use ss13_game::antag::blob::{self, BlobOvermind, BlobRecords, Placement};
use ss13_game::health::DamageType;
use ss13_game::mobs::Faction;
use ss13_game::obj_damage::Integrity;
use ss13_game::server::{PlayerMob, Players};
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
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
    "/obj": { "parent": "/atom/movable", "declared_vars": { "obj_flags": 2 } },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start" },
    "/obj/effect/landmark/blobstart": { "parent": "/obj/effect/landmark" },
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
    "/obj/structure": { "parent": "/obj", "declared_vars": { "anchored": 1 } },
    "/obj/structure/blob": { "parent": "/obj/structure", "declared_vars": {
        "name": "blob", "icon": { "resource": "blob.dmi" }, "icon_state": "blob", "density": 1,
        "max_integrity": 25 } },
    "/obj/structure/blob/normal": { "parent": "/obj/structure/blob", "declared_vars": {
        "name": "normal blob" } },
    "/obj/structure/blob/shield": { "parent": "/obj/structure/blob", "declared_vars": {
        "name": "strong blob", "icon_state": "blob_shield", "max_integrity": 150 } },
    "/obj/structure/blob/shield/core": { "parent": "/obj/structure/blob/shield" },
    "/obj/structure/blob/shield/reflective": { "parent": "/obj/structure/blob/shield",
        "declared_vars": { "name": "reflective blob", "icon_state": "blob_glow" } },
    "/obj/structure/blob/shield/reflective/core": {
        "parent": "/obj/structure/blob/shield/reflective" },
    "/obj/structure/blob/special": { "parent": "/obj/structure/blob" },
    "/obj/structure/blob/special/core": { "parent": "/obj/structure/blob/special",
        "declared_vars": { "name": "blob core", "icon_state": "blank_blob", "max_integrity": 400 } },
    "/obj/structure/blob/special/node": { "parent": "/obj/structure/blob/special",
        "declared_vars": { "name": "blob node", "icon_state": "blank_blob", "max_integrity": 200 } },
    "/obj/structure/blob/special/factory": { "parent": "/obj/structure/blob/special",
        "declared_vars": { "name": "factory blob", "icon_state": "blob_factory",
        "max_integrity": 200 } },
    "/obj/structure/blob/special/resource": { "parent": "/obj/structure/blob/special",
        "declared_vars": { "name": "resource blob", "icon_state": "blob_resource",
        "max_integrity": 60 } },
    "/mob": { "parent": "/atom/movable", "declared_vars": { "layer": 4, "density": 1 } },
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
        "icon": { "resource": "blob.dmi" } } },
    "/mob/living/basic/blob_minion": { "parent": "/mob/living/basic", "declared_vars": {
        "name": "Blob Error", "icon_state": "blob_head", "icon_living": "blob_head" } },
    "/mob/living/basic/blob_minion/spore": { "parent": "/mob/living/basic/blob_minion",
        "declared_vars": { "name": "blob spore", "icon_state": "blobpod",
        "icon_living": "blobpod", "maxHealth": 30, "health": 30, "basic_mob_flags": 1,
        "melee_damage_lower": 4, "melee_damage_upper": 8 } },
    "/mob/living/basic/blob_minion/spore/minion": {
        "parent": "/mob/living/basic/blob_minion/spore" },
    "/turf": { "parent": "/atom", "declared_vars": { "icon": { "resource": "t.dmi" } } },
    "/turf/open": { "parent": "/turf" },
    "/turf/open/floor": { "parent": "/turf/open", "declared_vars": { "icon_state": "floor" } },
    "/turf/closed": { "parent": "/turf", "declared_vars": { "density": 1, "opacity": 1 } },
    "/turf/closed/wall": { "parent": "/turf/closed", "declared_vars": { "icon_state": "wall" } },
    "/area": { "parent": "/atom", "declared_vars": { "area_flags": 0, "requires_power": 0 } },
    "/area/space": { "parent": "/area" },
    "/area/station": { "parent": "/area" },
    "/area/station/blob_test": { "parent": "/area/station", "declared_vars": {
        "area_flags": 2 } }
  }
}"##;

const STATES: [(&str, &str); 14] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
    ("blob.dmi", "blob"),
    ("blob.dmi", "blob_damaged"),
    ("blob.dmi", "blob_shield"),
    ("blob.dmi", "blob_glow"),
    ("blob.dmi", "blank_blob"),
    ("blob.dmi", "blob_factory"),
    ("blob.dmi", "blob_resource"),
    ("blob.dmi", "blobpod"),
    ("blob.dmi", "blob_head"),
];

/// `s` the player (2,4); the core goes at (7,3), away from the walls.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/station/blob_test)
"f" = (/turf/open/floor,/area/station/blob_test)
"b" = (/obj/effect/landmark/blobstart,/turf/open/floor,/area/station/blob_test)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/station/blob_test)

(1,1,1) = {"
wwwwwwwwwwwwww
wffffffffffffw
wfffffffffffbw
wsfffffffffffw
wffffffffffffw
wffffffffffffw
wwwwwwwwwwwwww
"}
"##;

const CORE_AT: TilePos = TilePos::new(7, 3, 1);
const EAST: TilePos = TilePos::new(8, 3, 1);

fn fixture(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("blob_fixture")
        .join(test);
    let mut used = vec![
        "/obj/effect/landmark/start/assistant",
        "/obj/effect/landmark/blobstart",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/area/station/blob_test",
        "/area/space",
        "/mob/living/carbon/human",
        "/obj/item/card/id/advanced",
        "/obj/item/clothing/under/color/grey",
        "/obj/item/clothing/shoes/sneakers/black",
        "/mob/living/basic/blob_minion/spore",
        "/mob/living/basic/blob_minion/spore/minion",
    ];
    used.extend(structures::BLOB_RUNTIME_TYPES);
    let assets = write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)]);
    let parsed = ss13_mapload::convert::DmTypes::parse(TYPES.as_bytes()).unwrap();
    let mobs = ss13_mapload::mobs::resolve(&parsed, &Default::default(), &Default::default());
    std::fs::write(assets.join(ss13_mapload::mobs::MOBS_FILE), mobs.to_ron()).unwrap();
    assets
}

struct Station(testing::Station);
ss13_game::drive_station!(Station, 0);

impl Station {
    fn start(test: &str) -> Self {
        let mut s = Self(testing::Station::start(
            fixture_map(&fixture(test), &["map.dmm"]),
            |_| {},
        ));
        s.run_for(0.5);
        s
    }

    fn world(&mut self) -> &mut World {
        self.0.server.world_mut()
    }

    /// An overmind of reactive spines (no strain moves or adds blobs) with its core on
    /// [`CORE_AT`].
    fn overmind(&mut self) -> Entity {
        let world = self.world();
        let o = blob::spawn_overmind(world, CORE_AT, blob::STARTING_POINTS);
        blob::set_strain(world, o, Strain::ReactiveSpines);
        assert!(blob::place_core(world, o, Placement::Force, true));
        o
    }

    fn state(&mut self, o: Entity) -> BlobOvermind {
        self.world()
            .get::<BlobOvermind>(o)
            .cloned()
            .expect("an overmind")
    }

    fn blob_on(&mut self, pos: TilePos) -> Option<(Entity, Kind)> {
        let world = self.world();
        structures::blob_at(world, pos).map(|e| (e, world.get::<Blob>(e).unwrap().kind))
    }

    fn blobs_of(&mut self, o: Entity) -> usize {
        let world = self.world();
        world
            .query::<&Blob>()
            .iter(world)
            .filter(|b| b.overmind == Some(o))
            .count()
    }
}

#[test]
fn an_overmind_places_its_core_and_its_blob_grows() {
    let mut s = Station::start("grows");
    let o = s.overmind();
    let core = s.state(o).core.expect("a core");
    assert_eq!(s.blob_on(CORE_AT), Some((core, Kind::Core)));
    assert!(s.state(o).placed);
    s.run_for(12.0);
    assert!(s.blobs_of(o) > 1, "the core's pulses grew the blob");
    assert!(
        s.state(o).points > blob::STARTING_POINTS,
        "the core gives resources"
    );
}

#[test]
fn powers_cost_resources_and_removing_pays_back() {
    let mut s = Station::start("powers");
    let o = s.overmind();
    blob::expand_blob(s.world(), o, EAST);
    assert_eq!(s.blob_on(EAST).map(|(_, k)| k), Some(Kind::Normal));
    assert_eq!(s.state(o).points, 56.0);
    blob::create_shield(s.world(), o, EAST);
    assert_eq!(s.blob_on(EAST).map(|(_, k)| k), Some(Kind::Strong));
    assert_eq!(s.state(o).points, 41.0);
    blob::remove_blob(s.world(), o, EAST);
    assert_eq!(s.blob_on(EAST), None);
    assert_eq!(s.state(o).points, 45.0);
    // The core cannot be removed; nothing to grow from two tiles away.
    blob::remove_blob(s.world(), o, CORE_AT);
    assert!(s.blob_on(CORE_AT).is_some());
    let far = TilePos::new(10, 5, 1);
    let t = s.now() + 1.0;
    s.run_until_for(2.0, "the expansion cooldown passes", |s| s.now() >= t);
    let before = s.state(o).points;
    blob::expand_blob(s.world(), o, far);
    assert_eq!(s.blob_on(far), None);
    assert_eq!(s.state(o).points, before, "nothing bought");
}

#[test]
fn structures_take_damage_through_their_resistances() {
    let mut s = Station::start("damage");
    let o = s.overmind();
    blob::expand_blob(s.world(), o, EAST);
    let (normal, _) = s.blob_on(EAST).expect("a normal blob");
    let world = s.world();
    assert_eq!(world.get::<Integrity>(normal).unwrap().current, 21.0);
    // Brute through a quarter, a laser through the 50 laser armor.
    structures::damage(world, normal, 10.0, DamageType::Brute, Flag::Melee).unwrap();
    assert_eq!(world.get::<Integrity>(normal).unwrap().current, 18.5);
    structures::damage(world, normal, 10.0, DamageType::Burn, Flag::Laser).unwrap();
    assert_eq!(world.get::<Integrity>(normal).unwrap().current, 13.5);
    // Fragile now: half the brute, through the generic object path.
    let hit = ss13_game::obj_damage::take_damage(world, normal, 30.0, DamageType::Brute);
    assert!(hit.destroyed, "{hit:?}");
    assert_eq!(s.blob_on(EAST), None);
}

#[test]
fn a_factory_makes_spores_and_a_dead_core_kills_the_overmind() {
    let mut s = Station::start("factory");
    let o = s.overmind();
    blob::expand_blob(s.world(), o, EAST);
    let (normal, _) = s.blob_on(EAST).expect("a normal blob");
    let world = s.world();
    let factory = structures::change_to(world, normal, Kind::Factory).expect("a factory");
    structures::be_pulsed(world, factory);
    let spores: Vec<(Entity, BlobMinion)> = world
        .query::<(Entity, &BlobMinion)>()
        .iter(world)
        .map(|(e, m)| (e, m.clone()))
        .collect();
    assert_eq!(spores.len(), 1, "one spore");
    let (spore, minion) = spores[0].clone();
    assert_eq!(minion.overmind, Some(o));
    assert_eq!(minion.factory, Some(factory));
    assert!(blob::is_ally(world, spore));
    assert!(
        world
            .get::<Faction>(spore)
            .is_some_and(|f| f.0.iter().any(|x| x == blob::FACTION_BLOB))
    );
    let core = s.state(o).core.unwrap();
    let hit = ss13_game::obj_damage::take_damage(s.world(), core, 10_000.0, DamageType::Brute);
    assert!(hit.destroyed);
    s.run_until_for(5.0, "the overmind dies", |s| {
        s.world().get::<BlobOvermind>(o).is_none()
    });
    s.run_for(0.2);
    let world = s.world();
    assert_eq!(world.get::<Blob>(factory).unwrap().overmind, None);
    assert_eq!(world.resource::<BlobRecords>().0.len(), 1);
    if let Some(m) = world.get::<BlobMinion>(spore) {
        assert_eq!(m.overmind, None);
    }
}

#[test]
fn a_host_pops_into_an_overmind_with_its_core() {
    let mut s = Station::start("pop");
    let p = s.join("p0", |_| {});
    let (link, me) = (s.players[p].link, s.players[p].mob);
    let at = *s.get::<TilePos>(me);
    let world = s.world();
    assert!(blob::make_host(world, me));
    assert!(blob::pop(world, me, false));
    s.run_for(0.5);
    let world = s.world();
    let overminds: Vec<Entity> = world
        .query_filtered::<Entity, With<BlobOvermind>>()
        .iter(world)
        .collect();
    assert_eq!(overminds.len(), 1);
    let o = overminds[0];
    assert_eq!(world.resource::<Players>().mob(link), Some(o));
    assert_eq!(world.get::<PlayerMob>(o).map(|p| p.client), Some(link));
    let state = world.get::<BlobOvermind>(o).cloned().unwrap();
    assert!(state.placed);
    let core = state.core.unwrap();
    assert_eq!(world.get::<TilePos>(core), Some(&at));
}
