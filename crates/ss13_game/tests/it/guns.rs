//! Guns over real WebTransport on 127.0.0.1: A's energy gun stamcrits B with disabler shots,
//! then (switched to kill) burns B through an armor vest for 25 × 70%; a pistol fires its
//! magazine and the chambered round dry, takes a new magazine and fires again; the energy gun
//! charges back in a recharger; a shot at a wall stops there and never reaches B behind it.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::combat::NextMove;
use ss13_game::guns::Gun;
use ss13_game::health::Health;
use ss13_game::inventory::{CarriedBy, Inventory};
use ss13_game::projectile::Projectile;
use ss13_game::status_effects::StatusEffects;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::DmItem;
use ss13_net::camera::client::CameraEffectReceived;
use ss13_net::chat::client::ChatReceived;
use ss13_shared::TilePos;
use ss13_shared::protocol::{CameraEffect, ClickModifiers, ClientIntent, InventorySlot};
use ss13_world::SpatialIndex;

/// tg values (`gun.dm`, `energy.dm`, `ballistic.dm`, `pistol.dm`, `beams.dm`, `bullets`,
/// `suit_armor`); the pistol's magazine holds 12 (`m9mm`).
const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/datum/armor": { "parent": "/datum", "declared_vars": { "melee": 0, "bullet": 0,
        "laser": 0, "energy": 0 } },
    "/datum/armor/suit_armor": { "parent": "/datum/armor", "declared_vars": {
        "melee": { "expr": "35" }, "bullet": { "expr": "30" }, "laser": { "expr": "30" },
        "energy": { "expr": "40" } } },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3, "anchored": 0 } },
    "/obj": { "parent": "/atom/movable", "declared_vars": { "force": 0, "damtype": "brute",
        "armor_type": { "expr": "/datum/armor" } } },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start" },
    "/obj/projectile": { "parent": "/obj", "declared_vars": { "name": "projectile",
        "icon": { "resource": "p.dmi" }, "icon_state": "bullet", "anchored": 1, "damage": 10,
        "damage_type": "brute", "armor_flag": "bullet", "armour_penetration": 0, "stamina": 0,
        "knockdown": 0, "paralyze": 0, "stun": 0, "immobilize": 0, "speed": 1.25, "range": 50,
        "pass_flags": 1, "accurate_range": 100, "accuracy_falloff": 7, "ricochets_max": 0,
        "ricochet_chance": 0, "ricochet_decay_chance": 0.7, "ricochet_decay_damage": 0.7,
        "ricochet_incidence_leeway": 40, "wound_bonus": -100, "exposed_wound_bonus": 0,
        "wound_falloff_tile": 0, "sharpness": 0, "damage_falloff_tile": 0 } },
    "/obj/projectile/energy": { "parent": "/obj/projectile", "declared_vars": {
        "damage": 0, "damage_type": "burn", "armor_flag": "energy" } },
    "/obj/projectile/energy/electrode": { "parent": "/obj/projectile/energy",
        "declared_vars": { "name": "electrode", "icon_state": "spark", "range": 5,
        "tase_stamina": 60 } },
    "/obj/projectile/bullet/pellet": { "parent": "/obj/projectile/bullet", "declared_vars": {
        "icon_state": "pellet_tracer", "damage_falloff_tile": -0.25 } },
    "/obj/projectile/bullet/pellet/shotgun_buckshot": { "parent": "/obj/projectile/bullet/pellet",
        "declared_vars": { "name": "buckshot pellet", "damage": 5, "wound_bonus": 5,
        "exposed_wound_bonus": 5, "speed": 1.1, "wound_falloff_tile": -0.5, "sharpness": 1 } },
    "/obj/projectile/bullet/c38": { "parent": "/obj/projectile/bullet", "declared_vars": {
        "name": ".38 bullet", "damage": 25, "ricochets_max": 2, "ricochet_chance": 50 } },
    "/obj/projectile/bullet/c38/match": { "parent": "/obj/projectile/bullet/c38" },
    "/obj/projectile/bullet/c38/match/bouncy": { "parent": "/obj/projectile/bullet/c38/match",
        "declared_vars": { "name": ".38 Rubber bullet", "damage": 10, "stamina": 30,
        "ricochets_max": 6, "ricochet_incidence_leeway": 0, "ricochet_chance": 130,
        "ricochet_decay_damage": 0.8, "sharpness": 0 } },
    "/obj/projectile/beam": { "parent": "/obj/projectile", "declared_vars": { "name": "laser",
        "icon_state": "laser", "damage": 20, "damage_type": "burn", "armor_flag": "laser",
        "pass_flags": 7 } },
    "/obj/projectile/beam/laser": { "parent": "/obj/projectile/beam", "declared_vars": {
        "damage": 25 } },
    "/obj/projectile/beam/disabler": { "parent": "/obj/projectile/beam", "declared_vars": {
        "name": "disabler beam", "icon_state": "omnilaser", "damage": 30,
        "damage_type": "stamina", "armor_flag": "energy", "speed": 1.6 } },
    "/obj/projectile/bullet": { "parent": "/obj/projectile", "declared_vars": {
        "name": "bullet", "damage": 60, "sharpness": 2, "wound_bonus": 0,
        "wound_falloff_tile": -5 } },
    "/obj/projectile/bullet/c9mm": { "parent": "/obj/projectile/bullet", "declared_vars": {
        "name": "9mm bullet", "damage": 30 } },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null, "body_parts_covered": 0,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null, "attack_speed": 8 } },
    "/obj/item/firing_pin": { "parent": "/obj/item", "declared_vars": {
        "default_pin_auth": 1, "fail_message": "недопустимый пользователь!" } },
    "/obj/item/stock_parts": { "parent": "/obj/item" },
    "/obj/item/stock_parts/power_store": { "parent": "/obj/item/stock_parts" },
    "/obj/item/stock_parts/power_store/cell": { "parent": "/obj/item/stock_parts/power_store",
        "declared_vars": { "maxcharge": 10000, "chargerate": 1000 } },
    "/obj/item/ammo_casing": { "parent": "/obj/item", "declared_vars": { "name": "casing",
        "icon_state": "s-casing", "caliber": null, "projectile_type": null, "harmful": 1,
        "click_cooldown_override": 0, "delay": 0, "pellets": 1, "variance": 0,
        "randomspread": 0 } },
    "/obj/item/ammo_casing/shotgun": { "parent": "/obj/item/ammo_casing", "declared_vars": {
        "name": "shotgun slug", "icon_state": "blshell", "caliber": "shotgun" } },
    "/obj/item/ammo_casing/shotgun/buckshot": { "parent": "/obj/item/ammo_casing/shotgun",
        "declared_vars": { "name": "buckshot shell", "icon_state": "gshell", "pellets": 6,
        "variance": 15, "randomspread": 1,
        "projectile_type": { "path": "/obj/projectile/bullet/pellet/shotgun_buckshot" } } },
    "/obj/item/ammo_casing/c38": { "parent": "/obj/item/ammo_casing", "declared_vars": {
        "caliber": "9mm", "projectile_type": { "path": "/obj/projectile/bullet/c38/match/bouncy" } } },
    "/obj/item/ammo_casing/energy/electrode": { "parent": "/obj/item/ammo_casing/energy",
        "declared_vars": { "e_cost": 2000, "harmful": 0, "select_name": "stun",
        "projectile_type": { "path": "/obj/projectile/energy/electrode" } } },
    "/obj/item/ammo_casing/c9mm": { "parent": "/obj/item/ammo_casing", "declared_vars": {
        "caliber": "9mm", "projectile_type": { "path": "/obj/projectile/bullet/c9mm" } } },
    "/obj/item/ammo_casing/energy": { "parent": "/obj/item/ammo_casing", "declared_vars": {
        "caliber": "energy", "e_cost": 1000, "select_name": "energy" } },
    "/obj/item/ammo_casing/energy/disabler": { "parent": "/obj/item/ammo_casing/energy",
        "declared_vars": { "e_cost": 500, "harmful": 0, "select_name": "disable",
        "projectile_type": { "path": "/obj/projectile/beam/disabler" } } },
    "/obj/item/ammo_casing/energy/laser": { "parent": "/obj/item/ammo_casing/energy",
        "declared_vars": { "select_name": "kill",
        "projectile_type": { "path": "/obj/projectile/beam/laser" } } },
    "/obj/item/ammo_box": { "parent": "/obj/item", "declared_vars": { "max_ammo": 7,
        "start_empty": 0, "caliber": null, "ammo_type": { "path": "/obj/item/ammo_casing" } } },
    "/obj/item/ammo_box/magazine": { "parent": "/obj/item/ammo_box" },
    "/obj/item/ammo_box/magazine/internal": { "parent": "/obj/item/ammo_box/magazine" },
    "/obj/item/ammo_box/magazine/internal/shot": { "parent": "/obj/item/ammo_box/magazine/internal",
        "declared_vars": { "max_ammo": 4, "caliber": "shotgun",
        "ammo_type": { "path": "/obj/item/ammo_casing/shotgun/buckshot" } } },
    "/obj/item/ammo_box/magazine/bouncy": { "parent": "/obj/item/ammo_box/magazine",
        "declared_vars": { "max_ammo": 6, "caliber": "9mm",
        "ammo_type": { "path": "/obj/item/ammo_casing/c38" } } },
    "/obj/item/ammo_box/magazine/m9mm": { "parent": "/obj/item/ammo_box/magazine",
        "declared_vars": { "name": "pistol magazine (9mm)", "icon_state": "9x19p",
        "max_ammo": 12, "caliber": "9mm", "ammo_type": { "path": "/obj/item/ammo_casing/c9mm" } } },
    "/obj/item/gun": { "parent": "/obj/item", "declared_vars": { "name": "gun",
        "fire_delay": 0, "burst_size": 1, "burst_delay": 2, "pinless": 0,
        "pin": { "path": "/obj/item/firing_pin" }, "spread": 0, "randomspread": 1,
        "dual_wield_spread": 24, "weapon_weight": 1, "recoil": 0,
        "recoil_backtime_multiplier": 1.5, "recoil_deviation": 20,
        "projectile_damage_multiplier": 1 } },
    "/obj/item/gun/ballistic": { "parent": "/obj/item/gun", "declared_vars": {
        "bolt_type": 1, "casing_ejector": 1, "semi_auto": 1, "spawnwithmagazine": 1,
        "internal_magazine": 0, "spawn_magazine_type": null,
        "accepted_magazine_type": { "path": "/obj/item/ammo_box/magazine/m9mm" } } },
    "/obj/item/gun/ballistic/automatic": { "parent": "/obj/item/gun/ballistic" },
    "/obj/item/gun/ballistic/automatic/pistol": { "parent": "/obj/item/gun/ballistic/automatic",
        "declared_vars": { "name": "Makarov pistol", "icon_state": "pistol", "bolt_type": 4 } },
    "/obj/item/gun/ballistic/automatic/pistol/bouncy": {
        "parent": "/obj/item/gun/ballistic/automatic/pistol", "declared_vars": {
        "accepted_magazine_type": { "path": "/obj/item/ammo_box/magazine/bouncy" } } },
    "/obj/item/gun/ballistic/shotgun": { "parent": "/obj/item/gun/ballistic", "declared_vars": {
        "name": "shotgun", "icon_state": "shotgun", "semi_auto": 0, "internal_magazine": 1,
        "casing_ejector": 0, "weapon_weight": 3, "recoil": 1,
        "accepted_magazine_type": { "path": "/obj/item/ammo_box/magazine/internal/shot" } } },
    "/obj/item/gun/energy": { "parent": "/obj/item/gun", "declared_vars": { "can_charge": 1,
        "cell_type": { "path": "/obj/item/stock_parts/power_store/cell" },
        "ammo_type": { "list": [ { "path": "/obj/item/ammo_casing/energy" } ] } } },
    "/obj/item/gun/energy/taser": { "parent": "/obj/item/gun/energy", "declared_vars": {
        "name": "taser gun", "icon_state": "taser", "ammo_type": { "list": [
            { "path": "/obj/item/ammo_casing/energy/electrode" } ] } } },
    "/obj/item/gun/energy/e_gun": { "parent": "/obj/item/gun/energy", "declared_vars": {
        "name": "energy gun", "icon_state": "energy", "ammo_type": { "list": [
            { "path": "/obj/item/ammo_casing/energy/disabler" },
            { "path": "/obj/item/ammo_casing/energy/laser" } ] } } },
    "/obj/item/clothing": { "parent": "/obj/item", "declared_vars": { "flash_protect": 0 } },
    "/obj/item/clothing/suit": { "parent": "/obj/item/clothing", "declared_vars": {
        "slot_flags": 1 } },
    "/obj/item/clothing/suit/armor": { "parent": "/obj/item/clothing/suit", "declared_vars": {
        "body_parts_covered": 3, "armor_type": { "path": "/datum/armor/suit_armor" } } },
    "/obj/item/clothing/suit/armor/vest": { "parent": "/obj/item/clothing/suit/armor",
        "declared_vars": { "name": "armor vest", "icon_state": "armoralt" } },
    "/obj/item/card": { "parent": "/obj/item", "declared_vars": { "inhand_icon_state": "card-id" } },
    "/obj/item/card/id": { "parent": "/obj/item/card", "declared_vars": {
        "access": { "list": [] }, "icon_state": "card_grey", "slot_flags": 256,
        "worn_icon_state": "nothing" } },
    "/obj/item/card/id/advanced": { "parent": "/obj/item/card/id" },
    "/obj/item/clothing/under": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 2 } },
    "/obj/item/clothing/under/color": { "parent": "/obj/item/clothing/under" },
    "/obj/item/clothing/under/color/grey": { "parent": "/obj/item/clothing/under/color" },
    "/obj/item/clothing/shoes": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 128 } },
    "/obj/item/clothing/shoes/sneakers": { "parent": "/obj/item/clothing/shoes" },
    "/obj/item/clothing/shoes/sneakers/black": { "parent": "/obj/item/clothing/shoes/sneakers" },
    "/obj/machinery": { "parent": "/obj", "declared_vars": { "anchored": 1, "density": 1 } },
    "/obj/machinery/recharger": { "parent": "/obj/machinery", "declared_vars": {
        "name": "recharger", "icon": { "resource": "icons/obj/machines/sec.dmi" },
        "icon_state": "recharger", "density": 0 } },
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

/// A at (2,3), B at (7,3); a recharger (2,2); a wall (5,5) in the floor.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"R" = (/obj/machinery/recharger,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwwwwww
wfffffffw
wfffwfffw
wfffffffw
wsffffsfw
wRffffffw
wwwwwwwww
"}
"##;

const STATES: [(&str, &str); 20] = [
    ("i.dmi", "shotgun"),
    ("i.dmi", "gshell"),
    ("i.dmi", "taser"),
    ("p.dmi", "pellet_tracer"),
    ("p.dmi", "spark"),
    ("i.dmi", "toy"),
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "energy"),
    ("i.dmi", "pistol"),
    ("i.dmi", "9x19p"),
    ("i.dmi", "s-casing"),
    ("i.dmi", "armoralt"),
    ("p.dmi", "bullet"),
    ("p.dmi", "laser"),
    ("p.dmi", "omnilaser"),
    ("icons/obj/machines/sec.dmi", "recharger"),
    ("icons/obj/machines/sec.dmi", "recharger-charging"),
];

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("guns_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/item/gun/energy/e_gun",
        "/obj/item/gun/ballistic/automatic/pistol",
        "/obj/item/ammo_box/magazine/m9mm",
        "/obj/item/gun/ballistic/shotgun",
        "/obj/item/gun/ballistic/automatic/pistol/bouncy",
        "/obj/item/gun/energy/taser",
        "/obj/item/clothing/suit/armor/vest",
        "/obj/machinery/recharger",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
    ];
    write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)])
}

#[derive(Resource, Default)]
struct Chat(Vec<String>);

fn record(mut chat: MessageReader<ChatReceived>, mut out: ResMut<Chat>) {
    for ChatReceived(message) in chat.read() {
        out.0.push(message.plain_text());
    }
}

#[derive(Resource, Default)]
struct Kicks(Vec<CameraEffect>);

fn record_kicks(mut kicks: MessageReader<CameraEffectReceived>, mut out: ResMut<Kicks>) {
    out.0.extend(kicks.read().map(|k| k.0));
}

struct Station(testing::Station);

ss13_game::drive_station!(Station, 0);

impl Station {
    fn start(test: &str) -> Self {
        let assets = fixture_assets(test);
        let mut s = testing::Station::start(fixture_map(&assets, &["map.dmm"]), |_| {});
        s.join_all(2, |app| {
            app.init_resource::<Chat>()
                .init_resource::<Kicks>()
                .add_systems(Update, (record, record_kicks));
        });
        let s = Self(s);
        let (a, b) = (s.players[0].mob, s.players[1].mob);
        assert_eq!(
            (s.pos(a), s.pos(b)),
            (TilePos::new(2, 3, 1), TilePos::new(7, 3, 1))
        );
        s
    }

    fn pos(&self, e: Entity) -> TilePos {
        *self.get::<TilePos>(e)
    }

    /// Spawns `path` into `p`'s active hand (emptied first).
    fn give(&mut self, p: usize, path: &str) -> Entity {
        let mob = self.players[p].mob;
        let world = self.0.server.world_mut();
        let hands = world.get::<Inventory>(mob).unwrap().hands;
        for item in hands.into_iter().flatten() {
            world.despawn(item);
        }
        world.get_mut::<Inventory>(mob).unwrap().hands = [None, None];
        let pos = *world.get::<TilePos>(mob).unwrap();
        let item = ss13_game::construction::spawn_obj(world, path, pos).unwrap();
        let hand = world.get::<Inventory>(mob).unwrap().active;
        world
            .entity_mut(item)
            .remove::<TilePos>()
            .insert(CarriedBy(mob));
        world.get_mut::<Inventory>(mob).unwrap().hands[hand] = Some(item);
        item
    }

    /// Puts `path` on `p` (suit slot).
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

    /// A left click on `target` once `p` is off its click cooldown and sees it.
    fn shoot(&mut self, p: usize, target: Entity) {
        let mob = self.players[p].mob;
        self.run_until("off the click cooldown", |s| {
            s.now() >= s.get::<NextMove>(mob).0
        });
        self.run_until("replicated", |s| s.net_id(p, target).is_some());
        self.0.click(p, target);
        self.run_until("the click is used", |s| s.now() < s.get::<NextMove>(mob).0);
    }

    /// A click on the active hand's slot (tg `attack_self`) once off the click cooldown.
    fn use_in_hand(&mut self, p: usize) {
        let mob = self.players[p].mob;
        self.run_until("off the click cooldown", |s| {
            s.now() >= s.get::<NextMove>(mob).0
        });
        let slot = if self.get::<Inventory>(mob).active == 0 {
            InventorySlot::LeftHand
        } else {
            InventorySlot::RightHand
        };
        self.send(p, ClientIntent::ClickSlot(slot));
    }

    fn said(&self, p: usize, text: &str) -> bool {
        self.players[p]
            .app
            .world()
            .resource::<Chat>()
            .0
            .iter()
            .any(|l| l.contains(text))
    }

    fn projectiles(&mut self) -> usize {
        let world = self.0.server.world_mut();
        world.query::<&Projectile>().iter(world).count()
    }

    fn turf_at(&self, pos: TilePos) -> Entity {
        let world = self.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .find(|&e| world.get::<ss13_mapload::Turf>(e).is_some())
            .unwrap()
    }

    fn angles(&mut self) -> Vec<f32> {
        let world = self.0.server.world_mut();
        world
            .query::<&Projectile>()
            .iter(world)
            .map(|p| p.angle)
            .collect()
    }

    fn effects(&self, e: Entity) -> StatusEffects {
        self.try_get::<StatusEffects>(e)
            .cloned()
            .unwrap_or_default()
    }
}

#[test]
fn disabler_stamcrits_and_laser_burns_through_a_vest() {
    let mut s = Station::start("disabler_stamcrits_and_laser_burns_through_a_vest");
    let b = s.players[1].mob;
    let egun = s.give(0, "/obj/item/gun/energy/e_gun");
    assert_eq!(s.get::<Gun>(egun).charge, 10_000.0);
    // Disabler shots (30 stamina, `energy` flag) until B is in stamina crit: four.
    let mut shots = 0;
    while !s.effects(b).stamcrit {
        s.shoot(0, b);
        shots += 1;
        s.run_until_for(3.0, "the beam lands", |s| s.projectiles() == 0);
        assert!(shots <= 6, "stamcrit after {shots} shots");
    }
    assert_eq!(shots, 4);
    s.run_until("B is told", |s| s.said(1, "попадает по вам в"));
    assert_eq!(s.get::<Health>(b).burn, 0.0);
    assert_eq!(s.get::<Gun>(egun).charge, 10_000.0 - 4.0 * 500.0);

    // Kill mode, a vest on B: 25 burn x (100 - 30)% = 17.5 to whatever zone it hits.
    s.use_in_hand(0);
    s.run_until("kill mode", |s| s.get::<Gun>(egun).select == 1);
    s.run_until("A is told", |s| s.said(0, "выбран режим: летальный"));
    s.wear_suit(1, "/obj/item/clothing/suit/armor/vest");
    let before = s.get::<Health>(b).burn;
    s.shoot(0, b);
    s.run_until_for(3.0, "the laser lands", |s| s.get::<Health>(b).burn > before);
    let dealt = s.get::<Health>(b).burn - before;
    // The chest is selected; a miss of the zone (7% falloff over 5 tiles: 65%) may hit an
    // unarmored limb for the full 25.
    assert!(
        (dealt - 17.5).abs() < 1e-3 || (dealt - 25.0).abs() < 1e-3,
        "dealt {dealt}"
    );
    if (dealt - 17.5).abs() < 1e-3 {
        s.run_until("the vest softens it", |s| {
            s.said(1, "Ваша броня смягчает удар!")
        });
    }
}

#[test]
fn pistol_empties_and_reloads_energy_gun_recharges() {
    let mut s = Station::start("pistol_empties_and_reloads_energy_gun_recharges");
    let b = s.players[1].mob;
    let a = s.players[0].mob;
    let pistol = s.give(0, "/obj/item/gun/ballistic/automatic/pistol");
    // 12 in the magazine + 1 chambered (`replace_new_round`).
    assert_eq!(
        s.get::<Gun>(pistol).magazine.as_ref().unwrap().rounds.len(),
        12
    );
    assert!(s.get::<Gun>(pistol).chambered.is_some());
    // At the floor (7,5), past B's shoulder: the bullets fly on into the wall.
    let floor = s.turf_at(TilePos::new(7, 5, 1));
    for _ in 0..13 {
        s.shoot(0, floor);
    }
    s.run_until_for(5.0, "the bullets land", |s| s.projectiles() == 0);
    let gun = s.get::<Gun>(pistol).clone();
    assert!(gun.chambered.is_none() && gun.bolt_locked);
    assert!(gun.magazine.as_ref().unwrap().rounds.is_empty());
    // The 13 casings on A's tile.
    let casings = {
        let world = s.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(TilePos::new(2, 3, 1))
            .iter()
            .filter(|&&e| {
                world
                    .get::<DmItem>(e)
                    .is_some_and(|i| i.path == "/obj/item/ammo_casing/c9mm")
            })
            .count()
    };
    assert_eq!(casings, 13);
    assert_eq!(s.get::<Health>(b).brute, 0.0);
    // Dry: a click.
    s.run_until("off cooldown", |s| s.now() >= s.get::<NextMove>(a).0);
    s.0.click(0, floor);
    s.run_until("it clicks", |s| s.said(0, "*щёлк*"));

    // Z ejects the empty magazine into the free hand; a new one goes in; Z drops the bolt.
    s.use_in_hand(0);
    s.run_until("ejected", |s| s.get::<Gun>(pistol).magazine.is_none());
    let empty = s
        .get::<Inventory>(a)
        .hands
        .into_iter()
        .flatten()
        .find(|&e| e != pistol);
    assert!(empty.is_some(), "the empty magazine is in a hand");
    let world = s.0.server.world_mut();
    world.despawn(empty.unwrap());
    let other = 1 - world.get::<Inventory>(a).unwrap().active;
    world.get_mut::<Inventory>(a).unwrap().hands[other] = None;
    let pos = *world.get::<TilePos>(a).unwrap();
    let mag =
        ss13_game::construction::spawn_obj(world, "/obj/item/ammo_box/magazine/m9mm", pos).unwrap();
    world
        .entity_mut(mag)
        .remove::<TilePos>()
        .insert(CarriedBy(a));
    // The magazine in the active hand, the pistol in the other.
    let inv = world.get::<Inventory>(a).unwrap().clone();
    let active = inv.active;
    let mut inv = inv;
    inv.hands[other] = Some(pistol);
    inv.hands[active] = Some(mag);
    world.entity_mut(a).insert(inv);
    let slot = if other == 0 {
        InventorySlot::LeftHand
    } else {
        InventorySlot::RightHand
    };
    s.send(0, ClientIntent::ClickSlot(slot));
    s.run_until("loaded", |s| s.get::<Gun>(pistol).magazine.is_some());
    s.run_until("A is told", |s| s.said(0, "вставка магазина"));
    // Back to the pistol's hand; Z drops the locked bolt and chambers.
    s.send(0, ClientIntent::SwapHands);
    s.run_until("swapped", |s| s.get::<Inventory>(a).active == other);
    s.use_in_hand(0);
    s.run_until("chambered", |s| s.get::<Gun>(pistol).chambered.is_some());
    assert!(!s.get::<Gun>(pistol).bolt_locked);
    s.shoot(0, b);
    s.run_until_for(3.0, "it fires again", |s| s.get::<Health>(b).brute > 0.0);
    assert_eq!(
        s.get::<Gun>(pistol).magazine.as_ref().unwrap().rounds.len(),
        10
    );

    // An energy gun with shots spent charges back in the recharger at (2,2).
    let egun = s.give(0, "/obj/item/gun/energy/e_gun");
    s.0.server.world_mut().get_mut::<Gun>(egun).unwrap().charge = 1000.0;
    let recharger = {
        let world = s.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(TilePos::new(2, 2, 1))
            .iter()
            .copied()
            .find(|&e| {
                world
                    .get::<ss13_game::power::chargers::Recharger>(e)
                    .is_some()
            })
            .unwrap()
    };
    s.run_until("replicated", |s| s.net_id(0, recharger).is_some());
    s.run_until("off cooldown", |s| s.now() >= s.get::<NextMove>(a).0);
    s.0.click(0, recharger);
    s.run_until("in the recharger", |s| {
        s.get::<ss13_game::power::chargers::Recharger>(recharger)
            .charging
            == Some(egun)
    });
    s.run_until_for(10.0, "it charges", |s| s.get::<Gun>(egun).charge > 1000.0);
}

#[test]
fn a_shot_stops_at_a_wall() {
    let mut s = Station::start("a_shot_stops_at_a_wall");
    let (a, b) = (s.players[0].mob, s.players[1].mob);
    // A at (2,5) shoots the wall (5,5); B stands behind it at (6,5).
    let world = s.0.server.world_mut();
    world.entity_mut(a).insert(TilePos::new(2, 5, 1));
    world.entity_mut(b).insert(TilePos::new(6, 5, 1));
    s.give(0, "/obj/item/gun/energy/e_gun");
    let wall = s.turf_at(TilePos::new(5, 5, 1));
    s.shoot(0, wall);
    s.run_until("the beam flies", |s| s.projectiles() == 1);
    s.run_until_for(3.0, "it ends at the wall", |s| s.projectiles() == 0);
    s.hold(1.0, "B is not hit", |s| s.effects(b).stamina == 0.0);
}

#[test]
fn a_shot_aimed_at_a_tile_corner_flies_to_that_pixel() {
    let mut s = Station::start("a_shot_aimed_at_a_tile_corner_flies_to_that_pixel");
    let a = s.players[0].mob;
    s.give(0, "/obj/item/gun/energy/e_gun");
    // A at (2,3) clicks the north-east corner of the floor at (6,5): 4.5 tiles east, 2.5 north.
    let floor = s.turf_at(TilePos::new(6, 5, 1));
    s.run_until("replicated", |s| s.net_id(0, floor).is_some());
    let corner = ClickModifiers {
        pixel: Some([16, 16]),
        ..default()
    };
    s.0.click_with(0, floor, corner);
    s.run_until("the beam flies", |s| s.projectiles() == 1);
    let expected = 144.0f32.atan2(80.0).to_degrees();
    let angle = s.angles()[0];
    assert!((angle - expected).abs() < 0.01, "{angle} vs {expected}");
    // A forged offset is clamped to one tile past the center.
    s.run_until_for(3.0, "it is gone", |s| s.projectiles() == 0);
    s.run_until("off cooldown", |s| s.now() >= s.get::<NextMove>(a).0 + 0.5);
    let forged = ClickModifiers {
        pixel: Some([127, -128]),
        ..default()
    };
    s.0.click_with(0, floor, forged);
    s.run_until("the beam flies", |s| s.projectiles() == 1);
    let expected = (4.0f32 * 32.0 + 32.0).atan2(2.0 * 32.0 - 32.0).to_degrees();
    let angle = s.angles()[0];
    assert!((angle - expected).abs() < 0.01, "{angle} vs {expected}");
}

#[test]
fn a_shotgun_fires_pellets_and_pumps() {
    let mut s = Station::start("a_shotgun_fires_pellets_and_pumps");
    let (a, b) = (s.players[0].mob, s.players[1].mob);
    let shotgun = s.give(0, "/obj/item/gun/ballistic/shotgun");
    // `WEAPON_HEAVY`: not with something in the other hand.
    let world = s.0.server.world_mut();
    let other = ss13_game::construction::spawn_obj(
        world,
        "/obj/item/ammo_casing/shotgun/buckshot",
        TilePos::new(2, 3, 1),
    )
    .unwrap();
    world
        .entity_mut(other)
        .remove::<TilePos>()
        .insert(CarriedBy(a));
    let active = world.get::<Inventory>(a).unwrap().active;
    world.get_mut::<Inventory>(a).unwrap().hands[1 - active] = Some(other);
    s.run_until("replicated", |s| s.net_id(0, b).is_some());
    s.0.click(0, b);
    s.run_until("refused", |s| s.said(0, "нужно обе руки!"));
    let world = s.0.server.world_mut();
    world.get_mut::<Inventory>(a).unwrap().hands[1 - active] = None;
    world.despawn(other);
    s.shoot(0, b);
    s.run_until("six pellets fly", |s| s.projectiles() == 6);
    // `recoil = 1`: A's view kicks a tile back from B (east), over 0.2 s out and 0.25 s back.
    s.run_until("the camera kicks", |s| {
        !s.players[0].app.world().resource::<Kicks>().0.is_empty()
    });
    let CameraEffect::Recoil {
        offset,
        out_ds,
        back_ds,
    } = s.players[0].app.world().resource::<Kicks>().0[0]
    else {
        panic!("a recoil kick");
    };
    assert!(offset[0] < -25 && offset[1].abs() <= 12, "{offset:?}");
    assert_eq!((out_ds, back_ds), (2.0, 2.5));
    s.run_until_for(3.0, "they land", |s| s.projectiles() == 0);
    assert!(s.get::<Health>(b).health() < 100.0, "pellets hit B");
    // Pump action: the spent shell stays chambered until racked.
    s.run_until("off cooldown", |s| s.now() >= s.get::<NextMove>(a).0);
    s.0.click(0, b);
    s.run_until("dry", |s| s.said(0, "*щёлк*"));
    s.use_in_hand(0);
    s.run_until("racked", |s| {
        s.get::<Gun>(shotgun)
            .chambered
            .as_ref()
            .is_some_and(|r| r.live)
    });
    s.shoot(0, b);
    s.run_until("six more", |s| s.projectiles() == 6);
}

#[test]
fn a_rubber_bullet_ricochets_back_at_the_shooter() {
    let mut s = Station::start("a_rubber_bullet_ricochets_back_at_the_shooter");
    let a = s.players[0].mob;
    // A at (2,5) shoots the wall (5,5) head on: it bounces straight back (leeway 0).
    s.0.server
        .world_mut()
        .entity_mut(a)
        .insert(TilePos::new(2, 5, 1));
    s.give(0, "/obj/item/gun/ballistic/automatic/pistol/bouncy");
    let wall = s.turf_at(TilePos::new(5, 5, 1));
    s.shoot(0, wall);
    s.run_until_for(3.0, "A is hit by the bounce", |s| {
        s.effects(a).stamina > 0.0
    });
    s.run_until("A reads it", |s| s.said(0, "попадает по вам"));
}

#[test]
fn a_bullet_wounds() {
    let mut s = Station::start("a_bullet_wounds");
    let b = s.players[1].mob;
    s.0.server
        .world_mut()
        .entity_mut(b)
        .insert(TilePos::new(3, 3, 1));
    s.give(0, "/obj/item/gun/ballistic/automatic/pistol");
    // 25 considered damage rolls 1..90 against 30 for a pierce wound: a few point blank
    // shots are enough.
    for _ in 0..10 {
        s.shoot(0, b);
        if s.try_get::<ss13_game::medical::wounds::Limbs>(b)
            .is_some_and(|l| l.any_wounds())
        {
            return;
        }
    }
    panic!("ten point blank 9mm shots never wounded");
}

#[test]
fn a_taser_hooks_and_drains_stamina_until_clicked_off() {
    let mut s = Station::start("a_taser_hooks_and_drains_stamina_until_clicked_off");
    let (a, b) = (s.players[0].mob, s.players[1].mob);
    s.0.server
        .world_mut()
        .entity_mut(b)
        .insert(TilePos::new(5, 3, 1));
    let taser = s.give(0, "/obj/item/gun/energy/taser");
    s.shoot(0, b);
    s.run_until_for(2.0, "electrodes in B", |s| {
        s.try_get::<ss13_game::taser::Tased>(b)
            .is_some_and(|t| !t.0.is_empty())
    });
    let charge = s.get::<Gun>(taser).charge;
    s.run_until_for(3.0, "60 stamina a second", |s| s.effects(b).stamina >= 90.0);
    assert!(s.get::<Gun>(taser).charge < charge, "the cell pays");
    // The firer clicks the victim: the electrodes let go.
    s.run_until("off cooldown", |s| s.now() >= s.get::<NextMove>(a).0);
    s.0.click(0, b);
    s.run_until("let go", |s| {
        s.try_get::<ss13_game::taser::Tased>(b)
            .is_some_and(|t| t.0.is_empty())
    });
}

#[test]
fn a_gun_in_the_mouth_fires_after_twelve_seconds_for_five_times_the_damage() {
    let mut s =
        Station::start("a_gun_in_the_mouth_fires_after_twelve_seconds_for_five_times_the_damage");
    let a = s.players[0].mob;
    let pistol = s.give(0, "/obj/item/gun/ballistic/automatic/pistol");
    // The chest selected: clicking yourself does not shoot.
    s.run_until("replicated", |s| s.net_id(0, a).is_some());
    s.0.click(0, a);
    s.hold(0.5, "no shot", |s| s.get::<Health>(a).brute == 0.0);
    assert!(!s.said(0, "Вы вставляете"));
    s.0.server
        .world_mut()
        .entity_mut(a)
        .insert(ss13_game::health::ZoneSelected(
            ss13_game::zone::TargetZone::Mouth,
        ));
    s.0.click(0, a);
    s.run_until("in the mouth", |s| {
        s.said(0, "Вы вставляете") && s.try_get::<ss13_game::tools::DoAfter>(a).is_some()
    });
    // Nothing for most of the 12 seconds (tg delays need no mood here: ×0.9 is 10.8 s).
    s.hold(10.0, "not yet", |s| s.get::<Health>(a).brute == 0.0);
    s.run_until_for(4.0, "the trigger is pulled", |s| {
        s.get::<Health>(a).brute > 0.0
    });
    assert!(s.said(0, "Вы спускаете курок!"));
    // 30 × 5 to the head.
    assert!(
        s.get::<Health>(a).brute >= 100.0,
        "{}",
        s.get::<Health>(a).brute
    );
    let gun = s.get::<Gun>(pistol);
    assert!(gun.chambered.as_ref().is_some_and(|r| r.live));
}

#[test]
fn a_held_up_mob_that_moves_is_shot_with_the_charged_round() {
    use ss13_game::breath::{AlertCategory, Alerts};
    use ss13_game::gunpoint::Gunpoint;
    let mut s = Station::start("a_held_up_mob_that_moves_is_shot_with_the_charged_round");
    let (a, b) = (s.players[0].mob, s.players[1].mob);
    s.0.server
        .world_mut()
        .entity_mut(b)
        .insert(TilePos::new(5, 3, 1));
    let pistol = s.give(0, "/obj/item/gun/ballistic/automatic/pistol");
    s.run_until("replicated", |s| s.net_id(0, b).is_some());
    let right = ClickModifiers {
        right: true,
        ..default()
    };
    s.0.click_with(0, b, right);
    s.run_until("aiming", |s| s.try_get::<Gunpoint>(a).is_some());
    assert_eq!(s.projectiles(), 0, "a right click does not fire");
    assert!(s.get::<Alerts>(a).get(AlertCategory::Holdup).is_some());
    assert!(s.get::<Alerts>(b).get(AlertCategory::Heldup).is_some());
    assert!(s.get::<Alerts>(b).get(AlertCategory::Surrender).is_some());
    s.run_until("B is told", |s| s.said(1, "целится из"));
    // 2.5 s later the aim steadies.
    s.run_until_for(4.0, "stage 2", |s| s.get::<Gunpoint>(a).stage == 2);
    assert_eq!(s.get::<Gunpoint>(a).damage_mult(), 1.75);
    // B steps away: the charged shot, 30 × 1.75.
    s.run_until("B steps", |s| {
        s.try_step(1, ss13_shared::Dir::North);
        s.pos(b) != TilePos::new(5, 3, 1)
    });
    s.run_until("the trigger is pulled", |s| {
        s.try_get::<Gunpoint>(a).is_none()
    });
    assert!(s.get::<Alerts>(b).get(AlertCategory::Heldup).is_none());
    s.run_until_for(3.0, "B is hit", |s| s.get::<Health>(b).brute > 0.0);
    assert!(
        s.get::<Health>(b).brute > 50.0,
        "{}",
        s.get::<Health>(b).brute
    );
    assert_eq!(
        s.get::<Gun>(pistol).magazine.as_ref().unwrap().rounds.len(),
        11
    );

    // Aimed again, the gun leaves A's hands: the aim is lowered without a shot.
    s.run_until("off cooldown", |s| s.now() >= s.get::<NextMove>(a).0);
    s.0.click_with(0, b, right);
    s.run_until("aiming", |s| s.try_get::<Gunpoint>(a).is_some());
    let world = s.0.server.world_mut();
    let hand = world.get::<Inventory>(a).unwrap().active;
    world.get_mut::<Inventory>(a).unwrap().hands[hand] = None;
    s.run_until("lowered", |s| s.try_get::<Gunpoint>(a).is_none());
    assert!(s.get::<Alerts>(b).get(AlertCategory::Heldup).is_none());
    assert_eq!(
        s.get::<Gun>(pistol).magazine.as_ref().unwrap().rounds.len(),
        11
    );
}
