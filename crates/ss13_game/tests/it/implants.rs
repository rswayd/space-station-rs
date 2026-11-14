//! Implants and turrets over real WebTransport on 127.0.0.1.
//!
//! - A player implants itself with an EMP implanter (no delay on yourself) and gets the
//!   implant's action button; an empty implanter takes the implant out of a tracking case; a
//!   macrobomb arms when its host dies.
//! - An AI turret on the map pops up and shoots a player in view with its stun beam; the turret
//!   control panel of its area turns it off; a panel's turret ignores players outside its area.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::actions::ActionButtons;
use ss13_game::defenses::server::{ControlZone, Turret, TurretControl, targets, update_turrets};
use ss13_game::health::{DamageType, Health, Stat, apply_damage};
use ss13_game::implants::{
    Detonation, Implant, ImplantHolder, ImplantKind, Implants, MACROBOMB, implant, item_interaction,
};
use ss13_game::inventory::{CarriedBy, Inventory};
use ss13_game::status_effects::StatusEffects;
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
    "/obj": { "parent": "/atom/movable" },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start" },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null } },
    "/obj/item/implanter": { "parent": "/obj/item", "declared_vars": {
        "name": "implanter", "icon": { "resource": "icons/obj/medical/syringe.dmi" },
        "icon_state": "implanter0" } },
    "/obj/item/implanter/emp": { "parent": "/obj/item/implanter", "declared_vars": {
        "name": "implanter (EMP)" } },
    "/obj/item/implantcase": { "parent": "/obj/item", "declared_vars": {
        "name": "implant case", "icon": { "resource": "icons/obj/medical/syringe.dmi" },
        "icon_state": "implantcase-0" } },
    "/obj/item/implantcase/tracking": { "parent": "/obj/item/implantcase", "declared_vars": {
        "name": "implant case - 'Tracking'" } },
    "/obj/item/card": { "parent": "/obj/item" },
    "/obj/item/card/id": { "parent": "/obj/item/card", "declared_vars": {
        "access": { "list": [] }, "slot_flags": 256 } },
    "/obj/item/card/id/advanced": { "parent": "/obj/item/card/id" },
    "/obj/item/clothing": { "parent": "/obj/item" },
    "/obj/item/clothing/under": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 2 } },
    "/obj/item/clothing/under/color": { "parent": "/obj/item/clothing/under" },
    "/obj/item/clothing/under/color/grey": { "parent": "/obj/item/clothing/under/color" },
    "/obj/item/clothing/shoes": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 128 } },
    "/obj/item/clothing/shoes/sneakers": { "parent": "/obj/item/clothing/shoes" },
    "/obj/item/clothing/shoes/sneakers/black": { "parent": "/obj/item/clothing/shoes/sneakers" },
    "/obj/projectile": { "parent": "/obj", "declared_vars": { "damage": 10,
        "damage_type": "brute", "armor_flag": "bullet", "speed": 1.25, "range": 50,
        "pass_flags": 1, "accurate_range": 100, "accuracy_falloff": 7,
        "icon": { "resource": "i.dmi" }, "icon_state": "toy" } },
    "/obj/projectile/beam": { "parent": "/obj/projectile", "declared_vars": {
        "damage": 20, "damage_type": "burn", "armor_flag": "laser", "pass_flags": 7 } },
    "/obj/projectile/beam/disabler": { "parent": "/obj/projectile/beam", "declared_vars": {
        "damage": 30, "damage_type": "stamina", "armor_flag": "energy", "speed": 1.6 } },
    "/obj/machinery": { "parent": "/obj", "declared_vars": { "anchored": 1 } },
    "/obj/machinery/porta_turret": { "parent": "/obj/machinery", "declared_vars": {
        "name": "turret", "icon": { "resource": "icons/obj/weapons/turrets.dmi" },
        "icon_state": "turretCover", "density": 1, "invisibility": 60, "scan_range": 7,
        "shot_delay": 15, "reqpower": 500, "always_up": 0, "has_cover": 1, "mode": 0,
        "turret_flags": 20, "req_access": { "list": ["security"] },
        "base_icon_state": "standard", "installation": null,
        "stun_projectile": { "path": "/obj/projectile/beam/disabler" },
        "lethal_projectile": { "path": "/obj/projectile/beam" } } },
    "/obj/machinery/porta_turret/ai": { "parent": "/obj/machinery/porta_turret",
        "declared_vars": { "scan_range": 6, "turret_flags": 148 } },
    "/obj/machinery/turretid": { "parent": "/obj/machinery", "declared_vars": {
        "name": "turret control panel", "icon": { "resource": "icons/obj/machines/turret_control.dmi" },
        "icon_state": "control", "enabled": 1, "lethal": 0, "locked": 1, "ailock": 0,
        "shoot_cyborgs": 0, "control_area": null, "req_access": { "list": ["ai_upload"] } } },
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
    "/area/space": { "parent": "/area" },
    "/area/hall": { "parent": "/area" }
  }
}"##;

const PREFABS: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"t" = (/obj/machinery/porta_turret/ai,/turf/open/floor,/area/space)
"p" = (/obj/machinery/turretid,/turf/open/floor,/area/space)
"h" = (/turf/open/floor,/area/hall)
"S" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/hall)
"##;

/// A player at (3,3).
const IMPLANT_MAP: &str = r##"
(1,1,1) = {"
wwwwwww
wfffffw
wfsfffw
wfffffw
wwwwwww
"}
"##;

/// A player at (3,3), the AI turret at (7,3) four tiles east, its panel at (7,4).
const TURRET_MAP: &str = r##"
(1,1,1) = {"
wwwwwwwwww
wfffffpffw
wfsffftffw
wffffffffw
wwwwwwwwww
"}
"##;

/// As [`TURRET_MAP`], but the player and the tiles west of x = 5 are another area (`/area/hall`).
const ZONE_MAP: &str = r##"
(1,1,1) = {"
wwwwwwwwww
whhhffpffw
whShfftffw
whhhfffffw
wwwwwwwwww
"}
"##;

const STATES: [(&str, &str); 8] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
    ("icons/obj/medical/syringe.dmi", "implanter1"),
    ("icons/obj/weapons/turrets.dmi", "turretCover"),
    ("icons/obj/weapons/turrets.dmi", "openTurretCover"),
];

fn fixture_assets(test: &str, map: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("implants_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/item/implanter",
        "/obj/item/implanter/emp",
        "/obj/item/implantcase/tracking",
        "/obj/machinery/porta_turret/ai",
        "/obj/machinery/turretid",
        "/obj/projectile/beam/disabler",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
        "/area/hall",
    ];
    let map = format!("{PREFABS}{map}");
    write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", &map)])
}

struct Station(testing::Station);

ss13_game::drive_station!(Station, 0);

impl Station {
    fn start(test: &str, map: &str) -> Self {
        let assets = fixture_assets(test, map);
        let mut s = testing::Station::start(fixture_map(&assets, &["map.dmm"]), |_| {});
        s.join_all(1, |_| {});
        s.without_mood();
        Self(s)
    }

    fn spawn(&mut self, path: &str, pos: TilePos) -> Entity {
        let e = ss13_game::construction::spawn_obj(self.server.world_mut(), path, pos)
            .unwrap_or_else(|| panic!("a {path}"));
        // The holder's implant is put in by a queued command.
        self.tick();
        e
    }

    fn hold(&mut self, mob: Entity, item: Entity) {
        let world = self.server.world_mut();
        let mut inventory = world.get_mut::<Inventory>(mob).unwrap();
        let active = inventory.active;
        inventory.hands[active] = Some(item);
        world
            .entity_mut(item)
            .remove::<TilePos>()
            .insert(CarriedBy(mob));
    }

    fn kinds(&self, mob: Entity) -> Vec<ImplantKind> {
        self.try_get::<Implants>(mob)
            .map(|i| i.0.iter().map(|i| i.kind).collect())
            .unwrap_or_default()
    }
}

#[test]
fn implanting_cases_and_bombs() {
    let mut s = Station::start("implanting_cases_and_bombs", IMPLANT_MAP);
    let a = s.players[0].mob;
    let at = *s.get::<TilePos>(a);

    // An EMP implanter on yourself: implanted at once, with its button.
    let implanter = s.spawn("/obj/item/implanter/emp", at);
    assert!(s.get::<ImplantHolder>(implanter).imp.is_some());
    s.hold(a, implanter);
    s.run_until("replicated", |s| s.net_id(0, a).is_some());
    s.click(0, a);
    s.run_until("the EMP implant is in", |s| {
        s.kinds(a).contains(&ImplantKind::Emp)
    });
    assert!(s.get::<ImplantHolder>(implanter).imp.is_none());
    s.run_until("the implant's button", |s| {
        s.try_get::<ActionButtons>(a)
            .is_some_and(|b| b.0.iter().any(|(b, _)| b.id.starts_with("implant:")))
    });

    // An empty implanter on a tracking case takes the implant.
    let empty = s.spawn("/obj/item/implanter", at);
    let case = s.spawn("/obj/item/implantcase/tracking", at);
    let link = s.players[0].link;
    assert!(item_interaction(s.server.world_mut(), link, a, empty, case));
    let moved = s.get::<ImplantHolder>(empty).imp.as_ref().map(|i| i.kind);
    assert_eq!(moved, Some(ImplantKind::Tracking));
    assert!(s.get::<ImplantHolder>(case).imp.is_none());

    // A macrobomb arms when its host dies (and is defused before it goes off).
    let bomb = Implant::new(MACROBOMB, 1000).unwrap();
    assert!(implant(s.server.world_mut(), a, None, bomb).is_ok());
    {
        let world = s.server.world_mut();
        let mut health = world.get::<Health>(a).unwrap().clone();
        let mut stat = *world.get::<Stat>(a).unwrap();
        apply_damage(&mut health, &mut stat, DamageType::Brute, 500.0);
        world.entity_mut(a).insert((health, stat));
    }
    s.run_until("the bomb arms", |s| s.try_get::<Detonation>(a).is_some());
    s.server.world_mut().entity_mut(a).remove::<Detonation>();
}

#[test]
fn turret_shoots_and_its_panel_turns_it_off() {
    let mut s = Station::start("turret_shoots_and_its_panel_turns_it_off", TURRET_MAP);
    let a = s.players[0].mob;
    let (turret, panel) = {
        let world = s.server.world_mut();
        let mut turrets = world.query_filtered::<Entity, With<Turret>>();
        let turret = turrets.iter(world).next().expect("the turret");
        let mut panels = world.query_filtered::<Entity, With<TurretControl>>();
        let panel = panels.iter(world).next().expect("the panel");
        (turret, panel)
    };
    assert!(s.get::<TurretControl>(panel).turrets.contains(&turret));
    s.run_until("the turret's stun beam hits", |s| {
        s.try_get::<StatusEffects>(a)
            .is_some_and(|e| e.stamina > 0.0)
    });
    assert!(s.get::<Turret>(turret).raised);

    // The panel turns its turrets off: it pops down.
    s.server
        .world_mut()
        .get_mut::<TurretControl>(panel)
        .unwrap()
        .enabled = false;
    update_turrets(s.server.world_mut(), panel);
    assert!(!s.get::<Turret>(turret).on);
    s.run_until("the turret pops down", |s| !s.get::<Turret>(turret).raised);
}

#[test]
fn a_panels_turret_keeps_to_its_area() {
    let mut s = Station::start("a_panels_turret_keeps_to_its_area", ZONE_MAP);
    let a = s.players[0].mob;
    let turret = {
        let world = s.server.world_mut();
        let mut turrets = world.query_filtered::<Entity, With<Turret>>();
        turrets.iter(world).next().expect("the turret")
    };
    assert!(s.try_get::<ControlZone>(turret).is_some());
    // In view four tiles west, but in the hall: no target.
    assert!(targets(s.server.world_mut(), turret).is_empty());

    // Into the turret's area: shot.
    s.server
        .world_mut()
        .entity_mut(a)
        .insert(TilePos::new(5, 3, 1));
    s.tick();
    assert_eq!(targets(s.server.world_mut(), turret), [a]);
    s.run_until("the turret's stun beam hits", |s| {
        s.try_get::<StatusEffects>(a)
            .is_some_and(|e| e.stamina > 0.0)
    });
}
