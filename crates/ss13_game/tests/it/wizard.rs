//! The wizard and its spells over real WebTransport on 127.0.0.1, on a fixture station with a
//! fixture den (`WizardConfig`): a dev wizard (`DevTraitor::Wizard`) joins in the den in its
//! robes with the staff, the scroll (4 uses) and its bound spellbook, no job and objectives. It
//! buys Teleport, Magic Missile, Ethereal Jaunt and Blink, refunds Blink in the den, buys
//! Fireball and levels Teleport up, then has no points left. Holding a null rod it cannot cast.
//! It teleports into the station's area; a crewman joins: the magic missile paralyzes him, a null
//! rod in his hand makes the next one fizzle; the smoke spell makes a cloud; the fireball hurts
//! him; the jaunt carries the wizard north through the wall into space and ends there. A third
//! player becomes its apprentice of destruction; the round-end report lists the wizard and its
//! purchases.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_chat::server::ChatServerPlugin;
use ss13_game::actions::ActionButtons;
use ss13_game::antag::dynamic::{DevTraitor, DynamicConfig};
use ss13_game::antag::objectives::RoundEnd;
use ss13_game::antag::wizard::server::{
    School, Spellbook, Wizard, WizardConfig, WizardDen, add_apprentice, purchase, refund,
    roundend_report,
};
use ss13_game::grenades::Smoke;
use ss13_game::health::Health;
use ss13_game::inventory::{CarriedBy, Inventory};
use ss13_game::jobs::{PlayerJob, fallback_jobs};
use ss13_game::spells::effects::TeleportScroll;
use ss13_game::spells::{
    CastOn, JauntStep, Jaunting, MAGIC_RESISTANCE, Spell, Spells, can_block_magic, can_cast, cast,
    knows, teleport_areas,
};
use ss13_game::status_effects::StatusEffects;
use ss13_game::storage::Stored;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::DmItem;
use ss13_mapload::antag::{ANTAG_FILE, AntagData};
use ss13_mapload::jobs::JOBS_FILE;
use ss13_net::server::Networked;
use ss13_shared::{Dir, TilePos};

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/datum/storage": { "parent": "/datum", "declared_vars": {
        "max_slots": 7, "max_specific_storage": 3, "max_total_storage": 14 } },
    "/datum/storage/backpack": { "parent": "/datum/storage", "declared_vars": {
        "max_slots": 21, "max_total_storage": 21 } },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 }, "declared_vars": { "can_atmos_pass": 1 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3, "anchored": 0 } },
    "/obj": { "parent": "/atom/movable" },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark",
        "vars": { "name": "start" }, "declared_vars": { "name": "start" } },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start",
        "vars": { "name": "Assistant" }, "declared_vars": { "name": "Assistant" } },
    "/obj/effect/landmark/start/wizard": { "parent": "/obj/effect/landmark/start",
        "vars": { "name": "wizard" }, "declared_vars": { "name": "wizard" } },
    "/obj/effect/landmark/latejoin": { "parent": "/obj/effect/landmark" },
    "/obj/effect/particle_effect": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "anchored": 1 } },
    "/obj/effect/particle_effect/fluid": { "parent": "/obj/effect/particle_effect" },
    "/obj/effect/particle_effect/fluid/smoke": { "parent": "/obj/effect/particle_effect/fluid",
        "declared_vars": { "name": "smoke", "opacity": 1 } },
    "/obj/effect/particle_effect/fluid/smoke/bad": {
        "parent": "/obj/effect/particle_effect/fluid/smoke" },
    "/obj/projectile": { "parent": "/obj", "declared_vars": { "name": "projectile",
        "icon": { "resource": "p.dmi" }, "icon_state": "bullet", "anchored": 1 } },
    "/obj/projectile/magic": { "parent": "/obj/projectile", "declared_vars": {
        "name": "bolt" } },
    "/obj/projectile/magic/aoe": { "parent": "/obj/projectile/magic" },
    "/obj/projectile/magic/aoe/magic_missile": { "parent": "/obj/projectile/magic/aoe",
        "declared_vars": { "name": "magic missile" } },
    "/obj/projectile/magic/fireball": { "parent": "/obj/projectile/magic",
        "declared_vars": { "name": "bolt of fireball" } },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null, "w_class": 2,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null } },
    "/obj/item/spellbook": { "parent": "/obj/item", "declared_vars": { "name": "spell book" } },
    "/obj/item/teleportation_scroll": { "parent": "/obj/item", "declared_vars": {
        "name": "scroll of teleportation" } },
    "/obj/item/teleportation_scroll/apprentice": { "parent": "/obj/item/teleportation_scroll",
        "declared_vars": { "name": "lesser scroll of teleportation" } },
    "/obj/item/staff": { "parent": "/obj/item", "declared_vars": { "name": "wizard staff" } },
    "/obj/item/nullrod": { "parent": "/obj/item", "declared_vars": { "name": "null rod" } },
    "/obj/item/card": { "parent": "/obj/item" },
    "/obj/item/card/id": { "parent": "/obj/item/card", "declared_vars": {
        "access": { "list": [] }, "slot_flags": 256 } },
    "/obj/item/card/id/advanced": { "parent": "/obj/item/card/id" },
    "/obj/item/clothing": { "parent": "/obj/item" },
    "/obj/item/clothing/under": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 2 } },
    "/obj/item/clothing/under/color": { "parent": "/obj/item/clothing/under" },
    "/obj/item/clothing/under/color/grey": { "parent": "/obj/item/clothing/under/color" },
    "/obj/item/clothing/under/color/lightpurple": { "parent": "/obj/item/clothing/under/color" },
    "/obj/item/clothing/suit": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 1 } },
    "/obj/item/clothing/suit/wizrobe": { "parent": "/obj/item/clothing/suit", "declared_vars": {
        "name": "wizard robe" } },
    "/obj/item/clothing/head": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 64 } },
    "/obj/item/clothing/head/wizard": { "parent": "/obj/item/clothing/head", "declared_vars": {
        "name": "wizard hat" } },
    "/obj/item/clothing/shoes": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 128 } },
    "/obj/item/clothing/shoes/sneakers": { "parent": "/obj/item/clothing/shoes" },
    "/obj/item/clothing/shoes/sneakers/black": { "parent": "/obj/item/clothing/shoes/sneakers" },
    "/obj/item/clothing/shoes/sandal": { "parent": "/obj/item/clothing/shoes" },
    "/obj/item/clothing/shoes/sandal/magic": { "parent": "/obj/item/clothing/shoes/sandal" },
    "/obj/item/radio": { "parent": "/obj/item", "declared_vars": { "name": "station bounced radio",
        "on": 1, "frequency": 1459, "broadcasting": 0, "listening": 1, "canhear_range": 3,
        "subspace_transmission": 0, "freerange": 0, "keyslot": null, "special_channels": 0,
        "slot_flags": 512 } },
    "/obj/item/radio/headset": { "parent": "/obj/item/radio", "declared_vars": {
        "name": "radio headset", "canhear_range": 0, "subspace_transmission": 1,
        "keyslot2": null, "slot_flags": 16 } },
    "/obj/item/storage": { "parent": "/obj/item", "declared_vars": {
        "storage_type": { "path": "/datum/storage" } } },
    "/obj/item/storage/backpack": { "parent": "/obj/item/storage", "declared_vars": {
        "slot_flags": 1024, "w_class": 4, "storage_type": { "path": "/datum/storage/backpack" } } },
    "/obj/item/storage/box": { "parent": "/obj/item/storage" },
    "/obj/item/storage/box/survival": { "parent": "/obj/item/storage/box" },
    "/mob": { "parent": "/atom/movable", "declared_vars": { "layer": 4, "density": 1 } },
    "/mob/living": { "parent": "/mob" },
    "/mob/living/carbon": { "parent": "/mob/living" },
    "/mob/living/carbon/human": { "parent": "/mob/living/carbon", "declared_vars": {
        "icon": { "resource": "h.dmi" }, "icon_state": "human_basic" } },
    "/turf": { "parent": "/atom", "declared_vars": { "icon": { "resource": "t.dmi" },
        "blocks_air": 0, "can_atmos_pass": 0, "initial_gas_mix": "o2=22;n2=82;TEMP=293.15",
        "temperature": 293.15, "baseturfs": { "path": "/turf/baseturf_bottom" } } },
    "/turf/open": { "parent": "/turf", "declared_vars": { "can_atmos_pass": -1,
        "planetary_atmos": 0 } },
    "/turf/open/floor": { "parent": "/turf/open", "declared_vars": { "icon_state": "floor",
        "baseturfs": { "path": "/turf/open/floor/plating" } } },
    "/turf/open/floor/plating": { "parent": "/turf/open/floor", "declared_vars": {
        "icon_state": "plating", "baseturfs": { "path": "/turf/baseturf_bottom" } } },
    "/turf/open/space": { "parent": "/turf/open", "declared_vars": { "temperature": 2.7,
        "initial_gas_mix": "TEMP=2.7", "icon_state": "space",
        "baseturfs": { "path": "/turf/open/space" } } },
    "/turf/open/space/basic": { "parent": "/turf/open/space" },
    "/turf/closed": { "parent": "/turf", "declared_vars": { "density": 1, "opacity": 1,
        "blocks_air": 1 } },
    "/turf/closed/wall": { "parent": "/turf/closed", "declared_vars": { "icon_state": "wall",
        "baseturfs": { "path": "/turf/open/floor/plating" } } },
    "/area": { "parent": "/atom" },
    "/area/space": { "parent": "/area", "declared_vars": { "name": "Space" } },
    "/area/station": { "parent": "/area", "declared_vars": { "name": "Мостик" } },
    "/area/centcom": { "parent": "/area", "declared_vars": { "name": "CentCom" } },
    "/area/centcom/wizard_station": { "parent": "/area/centcom", "declared_vars": {
        "name": "Wizard's Den" } }
  }
}"##;

/// Rows top-down, the last line is y = 1: the room's floor x 2..8, y 2..4 (every tile within 6
/// of every other), the wall row y = 5, space above it.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/station)
"f" = (/turf/open/floor,/area/station)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/station)
"L" = (/obj/effect/landmark/latejoin,/turf/open/floor,/area/station)
"e" = (/turf/open/space/basic,/area/space)

(1,1,1) = {"
eeeeeeeee
eeeeeeeee
wwwwwwwww
wfffffsLw
wfffffffw
wfffffffw
wwwwwwwww
"}
"##;

/// The den: the wizard start `W` at (3,3).
const DEN: &str = r##""w" = (/turf/closed/wall,/area/centcom/wizard_station)
"f" = (/turf/open/floor,/area/centcom/wizard_station)
"W" = (/obj/effect/landmark/start/wizard,/turf/open/floor,/area/centcom/wizard_station)

(1,1,1) = {"
wwwww
wfWfw
wfffw
wwwww
"}
"##;

const STATES: [(&str, &str); 9] = [
    ("t.dmi", "floor"),
    ("t.dmi", "plating"),
    ("t.dmi", "wall"),
    ("t.dmi", "space"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
    ("p.dmi", "bullet"),
    ("icons/mob/huds/antag_hud.dmi", "wizard"),
];

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("wizard_fixture")
        .join(test);
    let mut used = vec![
        "/obj/effect/landmark/start/assistant",
        "/obj/effect/landmark/start/wizard",
        "/obj/effect/landmark/latejoin",
        "/obj/effect/particle_effect/fluid/smoke/bad",
        "/obj/projectile/magic/aoe/magic_missile",
        "/obj/projectile/magic/fireball",
        "/obj/item/spellbook",
        "/obj/item/teleportation_scroll",
        "/obj/item/teleportation_scroll/apprentice",
        "/obj/item/staff",
        "/obj/item/nullrod",
        "/obj/item/clothing/under/color/lightpurple",
        "/obj/item/clothing/suit/wizrobe",
        "/obj/item/clothing/head/wizard",
        "/obj/item/clothing/shoes/sandal/magic",
        "/obj/item/radio/headset",
        "/turf/open/floor",
        "/turf/open/floor/plating",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
        "/area/station",
        "/area/centcom/wizard_station",
    ];
    used.extend(ss13_mapload::RUNTIME_CONTAINER_TYPES);
    write_fixture(
        &dir,
        TYPES,
        &used,
        &STATES,
        &[("map.dmm", MAP), ("den.dmm", DEN)],
    );
    std::fs::write(dir.join(JOBS_FILE), fallback_jobs().to_ron()).unwrap();
    std::fs::write(dir.join(ANTAG_FILE), AntagData::default().to_ron()).unwrap();
    dir
}

fn item_path(world: &World, e: Option<Entity>) -> Option<String> {
    world.get::<DmItem>(e?).map(|i| i.path.clone())
}

fn spellbook(world: &mut World, mob: Entity) -> Entity {
    let pack = world.get::<Inventory>(mob).unwrap().back.unwrap();
    let items = world.get::<Stored>(pack).unwrap().items().to_vec();
    items
        .into_iter()
        .find(|&e| world.get::<Spellbook>(e).is_some())
        .expect("the spellbook in the backpack")
}

/// A new `path` item in a free hand of `mob`.
fn hold(world: &mut World, mob: Entity, path: &str) -> Entity {
    let item = ss13_game::construction::spawn_detached(world, path).unwrap();
    world.entity_mut(item).insert((Networked, CarriedBy(mob)));
    let mut inv = world.get_mut::<Inventory>(mob).unwrap();
    let hand = inv
        .hands
        .iter()
        .position(Option::is_none)
        .expect("a free hand");
    inv.hands[hand] = Some(item);
    item
}

fn drop_held(world: &mut World, mob: Entity, item: Entity) {
    world.entity_mut(item).despawn();
    let mut inv = world.get_mut::<Inventory>(mob).unwrap();
    for h in &mut inv.hands {
        if *h == Some(item) {
            *h = None;
        }
    }
}

fn paralyzed(s: &testing::Station, mob: Entity) -> bool {
    s.try_get::<StatusEffects>(mob)
        .is_some_and(|e| e.paralyze > 0.0)
}

#[test]
fn wizard_buys_teleports_casts_and_takes_an_apprentice() {
    let assets = fixture_assets("main");
    let den_path = assets.join("den.dmm");
    let mut s = testing::Station::start(fixture_map(&assets, &["map.dmm"]), |app| {
        app.insert_resource(DynamicConfig {
            forced_rulesets: None,
            protect_roles: true,
            seed: None,
            dev_traitor: DevTraitor::Wizard,
        })
        .insert_resource(WizardConfig {
            den: Some(den_path),
        })
        .add_plugins(ChatServerPlugin);
    });
    let w = s.join("wizard", |_| {});
    let wizard = s.players[w].mob;
    let client = s.players[w].link;

    // In the den, dressed, with the bound book and the scroll; no job, objectives.
    let book = {
        let world = s.server.world_mut();
        let den = world.resource::<WizardDen>().clone();
        assert_eq!(den.spawns.len(), 1, "one wizard start in the den");
        let z = den.z.expect("the den is loaded");
        assert!(z > 1, "the den loads after the station");
        assert_eq!(den.spawns[0], TilePos::new(3, 3, z));
        assert_eq!(world.get::<TilePos>(wizard), Some(&den.spawns[0]));
        let w = world.get::<Wizard>(wizard).expect("a wizard");
        assert!(w.master.is_none());
        assert!(!w.objectives.is_empty());
        assert!(world.get::<PlayerJob>(wizard).is_none());
        let inv = world.get::<Inventory>(wizard).unwrap().clone();
        assert_eq!(
            item_path(world, inv.suit).as_deref(),
            Some("/obj/item/clothing/suit/wizrobe")
        );
        assert_eq!(
            item_path(world, inv.head).as_deref(),
            Some("/obj/item/clothing/head/wizard")
        );
        assert_eq!(
            item_path(world, inv.hands[0]).as_deref(),
            Some("/obj/item/staff")
        );
        let scroll = inv.pockets[1].expect("the scroll in the right pocket");
        assert_eq!(world.get::<TeleportScroll>(scroll).map(|s| s.uses), Some(4));
        let book = spellbook(world, wizard);
        let state = world.get::<Spellbook>(book).unwrap();
        assert_eq!((state.owner, state.uses), (Some(wizard), 10));
        book
    };

    // The book: points, levels, a refund in the den.
    {
        let world = s.server.world_mut();
        for entry in ["teleport", "magicm", "jaunt", "blink"] {
            let id = format!("/datum/spellbook_entry/{entry}");
            assert!(purchase(world, wizard, book, &id), "{entry}");
        }
        assert_eq!(world.get::<Spellbook>(book).unwrap().uses, 2);
        assert!(refund(world, wizard, book, "/datum/spellbook_entry/blink"));
        assert!(!knows(world, wizard, Spell::Blink));
        assert_eq!(world.get::<Spellbook>(book).unwrap().uses, 4);
        assert!(purchase(
            world,
            wizard,
            book,
            "/datum/spellbook_entry/fireball"
        ));
        assert!(purchase(
            world,
            wizard,
            book,
            "/datum/spellbook_entry/teleport"
        ));
        assert_eq!(world.get::<Spellbook>(book).unwrap().uses, 0);
        assert!(!purchase(
            world,
            wizard,
            book,
            "/datum/spellbook_entry/smoke"
        ));
        let teleport = world
            .get::<Spells>(wizard)
            .unwrap()
            .get(Spell::Teleport)
            .unwrap()
            .clone();
        assert_eq!(teleport.level, 2);
        assert_eq!(teleport.cooldown(), 40.0);
        let buttons = &world.get::<ActionButtons>(wizard).unwrap().0;
        assert!(
            buttons
                .iter()
                .any(|(b, _)| b.id == "spell:/datum/action/cooldown/spell/aoe/magic_missile"),
            "{buttons:?}"
        );
        // A null rod in the free hand stops the casting.
        let rod = hold(world, wizard, "/obj/item/nullrod");
        assert!(
            can_cast(world, wizard, Spell::MagicMissile)
                .unwrap_err()
                .is_some_and(|t| t.contains("antimagic"))
        );
        drop_held(world, wizard, rod);
        assert!(can_cast(world, wizard, Spell::MagicMissile).is_ok());
    }

    // Teleport onto the station.
    {
        let world = s.server.world_mut();
        let (_, area) = teleport_areas(world)
            .into_iter()
            .next()
            .expect("the station's area");
        assert!(cast(
            world,
            client,
            wizard,
            Spell::Teleport,
            CastOn::Area(area)
        ));
        let pos = *world.get::<TilePos>(wizard).unwrap();
        assert_eq!(pos.z, 1, "on the station");
        assert!(
            (2..=8).contains(&pos.x) && (2..=4).contains(&pos.y),
            "{pos:?}"
        );
        assert!(
            !cast(world, client, wizard, Spell::Teleport, CastOn::Area(area)),
            "on cooldown"
        );
        world.resource_mut::<DynamicConfig>().dev_traitor = DevTraitor::Off;
    }
    let c = s.join("crew", |_| {});
    let crew = s.players[c].mob;
    assert!(s.try_get::<Wizard>(crew).is_none());

    // Magic missile: the crewman is paralyzed.
    assert!(cast(
        s.server.world_mut(),
        client,
        wizard,
        Spell::MagicMissile,
        CastOn::Caster
    ));
    s.run_until("the missile paralyzes the crewman", |s| paralyzed(s, crew));
    s.run_until_for(10.0, "the paralysis wears off", |s| !paralyzed(s, crew));

    // A null rod in his hand: the next missile fizzles.
    let rod = {
        let world = s.server.world_mut();
        let rod = hold(world, crew, "/obj/item/nullrod");
        assert!(can_block_magic(world, crew, MAGIC_RESISTANCE));
        let mut spells = world.get_mut::<Spells>(wizard).unwrap();
        spells.get_mut(Spell::MagicMissile).unwrap().ready_at = 0.0;
        assert!(cast(
            world,
            client,
            wizard,
            Spell::MagicMissile,
            CastOn::Caster
        ));
        rod
    };
    s.hold(5.0, "the missile fizzles on the null rod", |s| {
        !paralyzed(s, crew)
    });
    drop_held(s.server.world_mut(), crew, rod);

    // The fireball hurts him.
    let before = s.get::<Health>(crew).health();
    {
        let world = s.server.world_mut();
        let at = *world.get::<TilePos>(crew).unwrap();
        let on = CastOn::Atom {
            target: crew,
            pos: at,
            right: false,
        };
        assert!(cast(world, client, wizard, Spell::Fireball, on));
    }
    s.run_until("the fireball burns the crewman", |s| {
        s.get::<Health>(crew).health() < before
    });

    // The jaunt: north through the wall into space, then solid again there.
    let start = *s.get::<TilePos>(wizard);
    assert!(cast(
        s.server.world_mut(),
        client,
        wizard,
        Spell::EtherealJaunt,
        CastOn::Caster
    ));
    assert!(s.try_get::<Jaunting>(wizard).is_some());
    assert!(
        can_cast(s.server.world(), wizard, Spell::Fireball).is_err(),
        "no casting while phased"
    );
    for _ in 0..testing::steps(3.0) {
        s.server.world_mut().write_message(JauntStep {
            mob: wizard,
            dir: Dir::North,
        });
        s.tick();
    }
    let phased = *s.get::<TilePos>(wizard);
    assert!(phased.y >= 6, "through the wall: {start:?} -> {phased:?}");
    s.run_until_for(10.0, "the jaunt ends", |s| {
        s.try_get::<Jaunting>(wizard).is_none()
    });
    let out = *s.get::<TilePos>(wizard);
    assert!(out.y >= 6, "out in space where it went: {out:?}");

    // An apprentice of destruction.
    let a = s.join("apprentice", |_| {});
    let apprentice = s.players[a].mob;
    {
        let world = s.server.world_mut();
        assert!(add_apprentice(
            world,
            apprentice,
            wizard,
            School::Destruction
        ));
        let w = world.get::<Wizard>(apprentice).unwrap();
        assert_eq!(w.master, Some(wizard));
        assert!(knows(world, apprentice, Spell::MagicMissile));
        assert!(knows(world, apprentice, Spell::Fireball));
        let inv = world.get::<Inventory>(apprentice).unwrap().clone();
        let scroll = inv.pockets[1].unwrap();
        assert_eq!(world.get::<TeleportScroll>(scroll).map(|s| s.uses), Some(1));
        assert!(inv.hands[0].is_none(), "no staff");
    }

    // The smoke spell (no robes needed) makes a cloud.
    {
        let world = s.server.world_mut();
        assert!(
            ss13_game::spells::learn(world, wizard, Spell::Smoke)
                == ss13_game::spells::Learned::New
        );
        assert!(cast(world, client, wizard, Spell::Smoke, CastOn::Caster));
        let clouds = world.query::<&Smoke>().iter(world).count();
        assert!(clouds > 0, "smoke");
    }

    let report = roundend_report(s.server.world_mut(), RoundEnd::default());
    let text: Vec<String> = report
        .iter()
        .map(|m| m.segments.iter().map(|g| g.text.as_str()).collect())
        .collect();
    assert!(
        text.iter().any(|l| l.contains("Волшебники/Ведьмы")),
        "{text:?}"
    );
    assert!(
        text.iter()
            .any(|l| l.contains("Teleport") && l.contains("Magic Missile")),
        "{text:?}"
    );
}
