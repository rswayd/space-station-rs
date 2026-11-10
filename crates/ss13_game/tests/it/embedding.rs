//! Embedding over a loopback server: a throwing star thrown fast lodges in B and is ripped out
//! by hand, a slow one bounces off; a bullet leaves `DROPDEL` shrapnel that vanishes when it
//! comes out; a severed limb drops what was lodged in it.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::breath::{AlertCategory, Alerts};
use ss13_game::embedding::{self, Embedded, EmbeddedObjects};
use ss13_game::health::Health;
use ss13_game::inventory::CarriedBy;
use ss13_game::server::PlayerMob;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_game::throw::Thrown;
use ss13_game::zone::BodyZone;
use ss13_mapload::DmEmbed;
use ss13_mapload::guns::ProjectileData;
use ss13_shared::TilePos;

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/datum/embedding": { "parent": "/datum", "declared_vars": { "embed_chance": 45,
        "fall_chance": 5, "pain_chance": 15, "pain_mult": 2, "impact_pain_mult": 4,
        "remove_pain_mult": 6, "rip_time": 30, "ignore_throwspeed_threshold": 0,
        "jostle_chance": 5, "jostle_pain_mult": 1, "pain_stam_pct": 0,
        "immune_traits": { "list": ["pierce_immunity"] }, "stealthy_embed": 0 } },
    "/datum/embedding/star": { "parent": "/datum/embedding", "declared_vars": {
        "embed_chance": 100, "fall_chance": 0, "pain_chance": 0, "jostle_chance": 0 } },
    "/datum/embedding/bullet": { "parent": "/datum/embedding", "declared_vars": {
        "embed_chance": 100, "fall_chance": 0, "pain_chance": 0, "jostle_chance": 0,
        "pain_mult": 3 } },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3, "anchored": 0,
        "throwforce": 0, "throw_speed": 2, "throw_range": 7 } },
    "/obj": { "parent": "/atom/movable", "declared_vars": { "force": 0, "damtype": "brute" } },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start" },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "card_grey", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null, "w_class": 1,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null, "attack_speed": 8 } },
    "/obj/item/throwing_star": { "parent": "/obj/item", "declared_vars": {
        "name": "throwing star", "throwforce": 10, "throw_speed": 4, "throw_range": 7,
        "w_class": 2, "sharpness": 1, "embed_type": { "path": "/datum/embedding/star" } } },
    "/obj/item/shrapnel": { "parent": "/obj/item", "declared_vars": {
        "name": "shrapnel shard", "item_flags": 32, "weak_against_armour": 1 } },
    "/obj/item/shrapnel/bullet": { "parent": "/obj/item/shrapnel", "declared_vars": {
        "name": "bullet", "embed_type": null } },
    "/obj/item/card": { "parent": "/obj/item" },
    "/obj/item/card/id": { "parent": "/obj/item/card", "declared_vars": {
        "access": { "list": [] }, "icon_state": "card_grey", "slot_flags": 256 } },
    "/obj/item/card/id/advanced": { "parent": "/obj/item/card/id" },
    "/obj/item/clothing": { "parent": "/obj/item" },
    "/obj/item/clothing/gloves": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 4 } },
    "/obj/item/clothing/gloves/boxing": { "parent": "/obj/item/clothing/gloves", "declared_vars": {
        "name": "boxing gloves" } },
    "/obj/item/clothing/under": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 2 } },
    "/obj/item/clothing/under/color": { "parent": "/obj/item/clothing/under" },
    "/obj/item/clothing/under/color/grey": { "parent": "/obj/item/clothing/under/color" },
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

/// A and B are moved to (2,3) and (4,3).
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwww
wffffw
wsffsw
wffffw
wwwwww
"}
"##;

const STATES: [(&str, &str); 5] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "card_grey"),
];

const STAR: &str = "/obj/item/throwing_star";
const SHRAPNEL: &str = "/obj/item/shrapnel/bullet";
const A_POS: TilePos = TilePos::new(2, 3, 1);
const B_POS: TilePos = TilePos::new(4, 3, 1);

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("embedding_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        STAR,
        SHRAPNEL,
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
    ];
    write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)])
}

struct Station(testing::Station);
ss13_game::drive_station!(Station, 0);

impl Station {
    fn start(test: &str) -> Self {
        let assets = fixture_assets(test);
        let mut s = testing::Station::start(fixture_map(&assets, &["map.dmm"]), |app| {
            app.add_plugins(ss13_chat::server::ChatServerPlugin);
        });
        s.join_all(2, |_| {});
        s.without_mood();
        let (a, b) = (s.players[0].mob, s.players[1].mob);
        let world = s.server.world_mut();
        world.entity_mut(a).insert(A_POS);
        world.entity_mut(b).insert(B_POS);
        Self(s)
    }

    /// A star on A's tile, thrown at B at `speed`.
    fn throw_star(&mut self, speed: f32) -> Entity {
        let world = self.server.world_mut();
        let star = ss13_game::construction::spawn_obj(world, STAR, A_POS).expect("a star");
        assert!(ss13_game::throw::throw_at(world, star, B_POS, 7, speed));
        star
    }
}

#[test]
fn fast_star_embeds_and_is_ripped_out() {
    let mut s = Station::start("fast_star_embeds_and_is_ripped_out");
    let b = s.players[1].mob;
    let star = s.throw_star(4.0);
    s.run_until("the star lodges in B", |s| {
        s.try_get::<Embedded>(star).is_some()
    });
    assert_eq!(s.get::<Embedded>(star).owner, b);
    assert!(s.try_get::<TilePos>(star).is_none(), "it is in the body");
    assert_eq!(s.get::<EmbeddedObjects>(b).0, [star]);
    // `throwforce` 10 + `w_class` 2 × `impact_pain_mult` 4, and no ordinary hit on top.
    assert_eq!(s.get::<Health>(b).brute, 18.0);
    assert!(
        s.get::<Alerts>(b)
            .get(AlertCategory::EmbeddedObject)
            .is_some()
    );
    assert!(embedding::bleed_rate(s.server.world(), b) > 0.0);

    // B pulls it out: `rip_time` 3 s × `w_class` 2, then `w_class` × `remove_pain_mult` 6.
    let client = s.get::<PlayerMob>(b).client;
    embedding::rip_out(s.server.world_mut(), client, b, star);
    s.run_for(3.0);
    assert!(
        s.try_get::<Embedded>(star).is_some(),
        "six seconds, not three"
    );
    s.run_until("the star is out", |s| s.try_get::<Embedded>(star).is_none());
    assert_eq!(s.get::<CarriedBy>(star).0, b, "into the ripper's hand");
    assert_eq!(s.get::<Health>(b).brute, 30.0);
    assert!(s.get::<EmbeddedObjects>(b).0.is_empty());
    assert!(
        s.get::<Alerts>(b)
            .get(AlertCategory::EmbeddedObject)
            .is_none()
    );
    assert_eq!(embedding::bleed_rate(s.server.world(), b), 0.0);
}

#[test]
fn slow_star_bounces_off() {
    let mut s = Station::start("slow_star_bounces_off");
    let b = s.players[1].mob;
    let star = s.throw_star(2.0);
    s.run_until("the star hits B", |s| s.get::<Health>(b).brute > 0.0);
    s.run_until("the star lands", |s| s.try_get::<Thrown>(star).is_none());
    // Below `EMBED_THROWSPEED_THRESHOLD`: the plain `throwforce`.
    assert_eq!(s.get::<Health>(b).brute, 10.0);
    assert!(s.try_get::<Embedded>(star).is_none());
    assert!(s.try_get::<TilePos>(star).is_some());
}

#[test]
fn bullet_shrapnel_is_deleted_when_it_comes_out() {
    let mut s = Station::start("bullet_shrapnel_is_deleted_when_it_comes_out");
    let b = s.players[1].mob;
    let world = s.server.world_mut();
    let star = ss13_game::construction::spawn_obj(world, STAR, A_POS).expect("a star");
    let data = world.get::<DmEmbed>(star).unwrap().data.clone();
    let bullet = ProjectileData {
        path: "/obj/projectile/bullet".into(),
        embed: ss13_mapload::EmbedData {
            path: "/datum/embedding/bullet".into(),
            pain_mult: 3.0,
            ..data
        },
        shrapnel_type: SHRAPNEL.into(),
        embed_falloff_tile: -3.0,
        ..Default::default()
    };
    // Fully blocked: nothing stays.
    embedding::try_embed_projectile(world, &bullet, "9mm bullet", b, BodyZone::Chest, 100.0, 2.0);
    assert!(world.get::<EmbeddedObjects>(b).is_none());
    embedding::try_embed_projectile(world, &bullet, "9mm bullet", b, BodyZone::Chest, 0.0, 0.0);
    let shard = world.get::<EmbeddedObjects>(b).expect("the bullet stays").0[0];
    assert_eq!(world.get::<Name>(shard).unwrap().as_str(), "9mm bullet");
    assert_eq!(world.get::<Embedded>(shard).unwrap().zone, BodyZone::Chest);
    // `w_class` 1 × `impact_pain_mult` 4: shrapnel has no `throwforce`.
    assert_eq!(world.get::<Health>(b).unwrap().brute, 4.0);
    // `DROPDEL`.
    embedding::remove_embedding(world, shard, None);
    assert!(world.get_entity(shard).is_err());
    assert!(world.get::<EmbeddedObjects>(b).unwrap().0.is_empty());
}

#[test]
fn a_severed_limb_drops_its_embeds() {
    let mut s = Station::start("a_severed_limb_drops_its_embeds");
    let b = s.players[1].mob;
    let world = s.server.world_mut();
    let star = ss13_game::construction::spawn_obj(world, STAR, A_POS).expect("a star");
    let other = ss13_game::construction::spawn_obj(world, STAR, A_POS).expect("a star");
    let data = world.get::<DmEmbed>(star).unwrap().data.clone();
    embedding::embed_into(world, star, b, BodyZone::LArm, data.clone());
    embedding::embed_into(world, other, b, BodyZone::Chest, data);
    // What `bodyparts::drop_limb` calls (the fixture has no bodypart items to drop).
    embedding::limb_removed(world, b, BodyZone::LArm);
    assert_eq!(world.get::<TilePos>(star), Some(&B_POS));
    assert!(world.get::<Embedded>(star).is_none());
    assert_eq!(world.get::<EmbeddedObjects>(b).unwrap().0, [other]);
    // A full heal takes the rest out.
    ss13_game::medical::heal_all(world, b);
    assert_eq!(world.get::<TilePos>(other), Some(&B_POS));
}
