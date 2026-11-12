//! Suits with their own mechanics, over a loopback server with two humans: reactive armor
//! blocks a hit and goes on cooldown, a straight jacket restrains until it comes off, laser tag
//! vests make teams for pins and beams, the ablative coat reflects reflectable shots, the EVA
//! plasma envirosuit puts its burning wearer out, a collar bomb sticks to the neck and goes off
//! five seconds after its button (a dud off the neck).

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::clothing::{ablative, collar_bomb, envirosuit, lasertag, reactive};
use ss13_game::inventory::{CarriedBy, Inventory};
use ss13_game::status_effects::Incapacitation;
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
    "/obj/item/card": { "parent": "/obj/item" },
    "/obj/item/card/id": { "parent": "/obj/item/card", "declared_vars": {
        "access": { "list": [] }, "icon_state": "card_grey", "slot_flags": 256 } },
    "/obj/item/card/id/advanced": { "parent": "/obj/item/card/id" },
    "/obj/item/clothing": { "parent": "/obj/item", "declared_vars": {
        "body_parts_covered": 0, "clothing_flags": 0 } },
    "/obj/item/clothing/suit": { "parent": "/obj/item/clothing", "declared_vars": {
        "slot_flags": 1, "body_parts_covered": 2 } },
    "/obj/item/clothing/suit/armor": { "parent": "/obj/item/clothing/suit" },
    "/obj/item/clothing/suit/armor/reactive": { "parent": "/obj/item/clothing/suit/armor" },
    "/obj/item/clothing/suit/armor/reactive/teleport": { "parent": "/obj/item/clothing/suit/armor/reactive" },
    "/obj/item/clothing/suit/jacket": { "parent": "/obj/item/clothing/suit" },
    "/obj/item/clothing/suit/jacket/straight_jacket": { "parent": "/obj/item/clothing/suit/jacket" },
    "/obj/item/clothing/suit/bluetag": { "parent": "/obj/item/clothing/suit" },
    "/obj/item/clothing/suit/hooded": { "parent": "/obj/item/clothing/suit" },
    "/obj/item/clothing/suit/hooded/ablative": { "parent": "/obj/item/clothing/suit/hooded" },
    "/obj/item/clothing/suit/space": { "parent": "/obj/item/clothing/suit" },
    "/obj/item/clothing/suit/space/eva": { "parent": "/obj/item/clothing/suit/space" },
    "/obj/item/clothing/suit/space/eva/plasmaman": { "parent": "/obj/item/clothing/suit/space/eva",
        "declared_vars": { "body_parts_covered": 2047, "clothing_flags": 1024 } },
    "/obj/item/clothing/under": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 2 } },
    "/obj/item/clothing/under/color": { "parent": "/obj/item/clothing/under" },
    "/obj/item/clothing/under/color/grey": { "parent": "/obj/item/clothing/under/color" },
    "/obj/item/clothing/shoes": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 128 } },
    "/obj/item/clothing/neck": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 2048 } },
    "/obj/item/clothing/neck/collar_bomb": { "parent": "/obj/item/clothing/neck" },
    "/obj/item/collar_bomb_button": { "parent": "/obj/item" },
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

const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwwww
wfffffw
wfsfsfw
wfffffw
wwwwwww
"}
"##;

const STATES: [(&str, &str); 5] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "card_grey"),
];

const TELEPORT_ARMOR: &str = "/obj/item/clothing/suit/armor/reactive/teleport";
const JACKET: &str = "/obj/item/clothing/suit/jacket/straight_jacket";
const BLUE_VEST: &str = "/obj/item/clothing/suit/bluetag";
const COAT: &str = "/obj/item/clothing/suit/hooded/ablative";
const EVA: &str = "/obj/item/clothing/suit/space/eva/plasmaman";
const COLLAR: &str = "/obj/item/clothing/neck/collar_bomb";
const BUTTON: &str = "/obj/item/collar_bomb_button";

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("suits_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        TELEPORT_ARMOR,
        JACKET,
        BLUE_VEST,
        COAT,
        EVA,
        COLLAR,
        BUTTON,
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
        Self(s)
    }

    /// Puts `path` on `p` as its suit.
    fn wear_suit(&mut self, p: usize, path: &str) -> Entity {
        let mob = self.players[p].mob;
        let world = self.0.server.world_mut();
        let pos = *world.get::<TilePos>(mob).unwrap();
        let item = ss13_game::construction::spawn_obj(world, path, pos).unwrap();
        world
            .entity_mut(item)
            .remove::<TilePos>()
            .insert(CarriedBy(mob));
        world.get_mut::<Inventory>(mob).unwrap().suit = Some(item);
        item
    }

    fn client(&self, p: usize) -> Entity {
        self.0
            .server
            .world()
            .get::<ss13_game::server::PlayerMob>(self.players[p].mob)
            .unwrap()
            .client
    }
}

#[test]
fn reactive_armor_blocks_then_recharges() {
    let mut s = Station::start("reactive_armor_blocks_then_recharges");
    let (a, b) = (s.players[0].mob, s.players[1].mob);
    let armor = s.wear_suit(1, TELEPORT_ARMOR);
    s.run_for(0.2);
    let client = s.client(1);
    let world = s.server.world_mut();
    // Off: never reacts.
    assert!((0..30).all(|_| !reactive::hit_reaction(world, b, Some(a), "атаку")));
    assert!(reactive::attack_self(world, client, b, armor));
    assert!(world.get::<reactive::ReactiveArmor>(armor).unwrap().active);
    // On: about half the hits trigger it; the first one blocks and starts the cooldown.
    let blocked = (0..40).any(|_| reactive::hit_reaction(world, b, Some(a), "атаку"));
    assert!(blocked, "a 50% reaction in 40 hits");
    let now = world.resource::<Time>().elapsed_secs_f64();
    let state = *world.get::<reactive::ReactiveArmor>(armor).unwrap();
    assert!(state.cooldown_until > now + 9.0, "{state:?}");
    // Recharging: it only says so.
    assert!((0..30).all(|_| !reactive::hit_reaction(world, b, Some(a), "атаку")));
    assert!(
        world.get::<TilePos>(b).is_some(),
        "the wearer was teleported, not lost"
    );
}

#[test]
fn straight_jacket_restrains_until_removed() {
    let mut s = Station::start("straight_jacket_restrains_until_removed");
    let b = s.players[1].mob;
    let jacket = s.wear_suit(1, JACKET);
    s.run_until("the jacket restrains", |s| {
        let i = s.get::<Incapacitation>(b);
        i.restrained && i.hands_blocked
    });
    let client = s.client(1);
    ss13_game::restraints::resist_restraints(s.server.world_mut(), client, b);
    assert!(
        s.server
            .world()
            .get::<ss13_game::tools::DoAfter>(b)
            .is_some(),
        "resisting starts the 5 minute breakout"
    );
    // Off by any path: free again.
    let world = s.server.world_mut();
    let pos = *world.get::<TilePos>(b).unwrap();
    world.get_mut::<Inventory>(b).unwrap().suit = None;
    world.entity_mut(jacket).remove::<CarriedBy>().insert(pos);
    s.run_until("free again", |s| !s.get::<Incapacitation>(b).restrained);
}

#[test]
fn laser_tag_vests_make_teams() {
    let mut s = Station::start("laser_tag_vests_make_teams");
    let (a, b) = (s.players[0].mob, s.players[1].mob);
    s.wear_suit(1, BLUE_VEST);
    let world = s.server.world();
    assert_eq!(lasertag::team_of(world, b), Some(lasertag::Team::Blue));
    assert_eq!(lasertag::team_of(world, a), None);
    // Pins fire for their own team only.
    assert_eq!(
        lasertag::pin_check(world, "/obj/item/firing_pin/tag/blue", b),
        Some(Ok(()))
    );
    assert!(matches!(
        lasertag::pin_check(world, "/obj/item/firing_pin/tag/red", b),
        Some(Err(_))
    ));
    assert!(matches!(
        lasertag::pin_check(world, "/obj/item/firing_pin/tag/blue", a),
        Some(Err(_))
    ));
    assert_eq!(lasertag::pin_check(world, "/obj/item/firing_pin", b), None);
    // Beams of the other team drain stamina; own beams and non-players do nothing.
    assert_eq!(
        lasertag::hit_stamina(world, "/obj/projectile/beam/lasertag/redtag", b),
        Some(lasertag::LASERTAG_DAMAGE)
    );
    assert_eq!(
        lasertag::hit_stamina(world, "/obj/projectile/beam/lasertag/bluetag", b),
        None
    );
    assert_eq!(
        lasertag::hit_stamina(world, "/obj/projectile/beam/lasertag/redtag", a),
        None
    );
}

#[test]
fn ablative_coat_reflects_lasers() {
    let mut s = Station::start("ablative_coat_reflects_lasers");
    let (a, b) = (s.players[0].mob, s.players[1].mob);
    s.wear_suit(1, COAT);
    let world = s.server.world_mut();
    let start = *world.get::<TilePos>(a).unwrap();
    let projectile = |reflectable: bool, zone| ss13_game::projectile::Projectile {
        data: ss13_mapload::ProjectileData {
            path: "/obj/projectile/beam/laser".into(),
            reflectable,
            ..default()
        },
        firer: a,
        original: Some(b),
        zone,
        hit_prone: false,
        start,
        x: start.x as f32,
        y: start.y as f32,
        dx: 1.0,
        dy: 0.0,
        moved: 0.0,
        angle: 90.0,
        fired_from: None,
        ricochets: 0,
    };
    use ss13_game::zone::BodyZone;
    // A bullet is not reflectable; the coat does not cover the head.
    let mut bullet = projectile(false, BodyZone::Chest);
    assert!((0..30).all(|_| !ablative::reflect_projectile(world, b, b, &mut bullet)));
    let mut head = projectile(true, BodyZone::Head);
    assert!((0..30).all(|_| !ablative::reflect_projectile(world, b, b, &mut head)));
    let mut laser = projectile(true, BodyZone::Chest);
    assert!(
        (0..40).any(|_| ablative::reflect_projectile(world, b, b, &mut laser)),
        "a 50% reflection in 40 shots"
    );
    assert_eq!(laser.firer, b);
    assert_eq!(laser.original, None);
    assert!(
        (210.0..=330.0).contains(&laser.angle),
        "turned back: {}",
        laser.angle
    );
}

#[test]
fn plasma_envirosuit_puts_the_wearer_out() {
    let mut s = Station::start("plasma_envirosuit_puts_the_wearer_out");
    let b = s.players[1].mob;
    let suit = s.wear_suit(1, EVA);
    s.run_for(0.2);
    let world = s.server.world_mut();
    assert_eq!(
        world.get::<envirosuit::Envirosuit>(suit).map(|e| e.left),
        Some(10)
    );
    ss13_game::fire::adjust_fire_stacks(world, b, 5.0);
    ss13_game::fire::ignite_mob(world, b);
    s.run_until("the suit spends a charge", |s| {
        s.get::<envirosuit::Envirosuit>(suit).left == 9
    });
    assert!(!ss13_game::fire::on_fire(s.server.world(), b));
}

#[test]
fn collar_bomb_sticks_and_goes_off() {
    let mut s = Station::start("collar_bomb_sticks_and_goes_off");
    let b = s.players[1].mob;
    let world = s.server.world_mut();
    let pos = *world.get::<TilePos>(b).unwrap();
    let collar = ss13_game::construction::spawn_obj(world, COLLAR, pos).unwrap();
    let spare = ss13_game::construction::spawn_obj(world, COLLAR, pos).unwrap();
    let button = ss13_game::construction::spawn_obj(world, BUTTON, pos).unwrap();
    s.run_for(0.2);
    let world = s.server.world_mut();
    collar_bomb::link(world, button, collar);
    world
        .entity_mut(collar)
        .remove::<TilePos>()
        .insert(CarriedBy(b));
    world.get_mut::<Inventory>(b).unwrap().neck = Some(collar);
    s.run_until("the collar grips the neck", |s| {
        s.try_get::<ss13_game::inventory::NoDrop>(collar).is_some()
    });
    assert_eq!(
        s.get::<collar_bomb::CollarButton>(button).collar,
        Some(collar)
    );

    // Off the neck it is a dud and can go again.
    let world = s.server.world_mut();
    assert!(collar_bomb::trigger(world, spare));
    assert!(!collar_bomb::trigger(world, spare), "already counting down");
    s.run_until_for(10.0, "the spare collar fizzles", |s| {
        !s.get::<collar_bomb::CollarBomb>(spare).active
    });
    assert!(s.server.world().get_entity(spare).is_ok());

    // On the neck: five seconds, then the head.
    let start = s.now();
    assert!(collar_bomb::trigger(s.server.world_mut(), collar));
    s.run_until_for(10.0, "the collar goes off", |s| {
        s.server.world().get_entity(collar).is_err()
    });
    assert!(s.now() - start >= 5.0, "the countdown takes five seconds");
    assert_eq!(s.get::<Inventory>(b).neck, None);
    assert_eq!(
        s.get::<ss13_game::health::Stat>(b),
        &ss13_game::health::Stat::Dead
    );
    assert_eq!(s.get::<collar_bomb::CollarButton>(button).collar, None);
}
