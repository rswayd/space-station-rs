//! Martial arts over a loopback server: A learns CQC and its punch and kick combo hit B; boxing
//! gloves worn by A teach boxing (stamina-only punches against a non-boxer) and taking them off
//! forgets it.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::combat::{CombatMode, NextMove};
use ss13_game::health::Health;
use ss13_game::inventory::{CarriedBy, Inventory};
use ss13_game::martial::{self, MartialArts, Origin, Style};
use ss13_game::status_effects::StatusEffects;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::DmItem;
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

/// A and B are moved to (2,3) and (3,3); boxing gloves lie at (2,2).
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"g" = (/obj/item/clothing/gloves/boxing,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwww
wfffw
wsfsw
wgffw
wwwww
"}
"##;

const STATES: [(&str, &str); 5] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "card_grey"),
];

const GLOVES: &str = "/obj/item/clothing/gloves/boxing";

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("martial_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        GLOVES,
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
        let (a, b) = (s.players[0].mob, s.players[1].mob);
        let world = s.server.world_mut();
        world.entity_mut(a).insert(TilePos::new(2, 3, 1));
        world.entity_mut(b).insert(TilePos::new(3, 3, 1));
        world.entity_mut(a).insert(CombatMode(true));
        Self(s)
    }

    /// A clicks B once A is off its click cooldown.
    fn punch(&mut self) {
        let (a, b) = (self.players[0].mob, self.players[1].mob);
        self.run_until("off cooldown", |s| {
            s.server.world().resource::<Time>().elapsed_secs_f64() >= s.get::<NextMove>(a).0
        });
        self.0.click(0, b);
    }

    fn gloves(&mut self) -> Entity {
        let world = self.server.world_mut();
        let mut q = world.query::<(Entity, &DmItem)>();
        q.iter(world)
            .find(|(_, i)| i.path == GLOVES)
            .map(|(e, _)| e)
            .expect("gloves on the map")
    }
}

#[test]
fn cqc_punch_then_kick_combo() {
    let mut s = Station::start("cqc_punch_then_kick_combo");
    let (a, b) = (s.players[0].mob, s.players[1].mob);
    assert!(martial::teach(
        s.server.world_mut(),
        a,
        Style::Cqc,
        Origin::Learned
    ));
    assert_eq!(martial::active_style(s.server.world(), a), Some(Style::Cqc));

    // H: a CQC hit, 13 brute, never a miss.
    s.punch();
    s.run_until("the CQC hit lands", |s| s.get::<Health>(b).brute >= 13.0);
    assert!(s.get::<Health>(b).brute < 20.0, "one hit only");

    // H H: the kick throws B a tile back and deals 10 more.
    s.punch();
    s.run_until("the kick throws B back", |s| {
        *s.get::<TilePos>(b) == TilePos::new(4, 3, 1)
    });
    s.run_until("the kick hurts", |s| s.get::<Health>(b).brute >= 23.0);
    let streak = &s.get::<MartialArts>(a).0[0].streak;
    assert!(streak.is_empty(), "a combo resets the streak: {streak:?}");
}

#[test]
fn boxing_gloves_teach_while_worn() {
    let mut s = Station::start("boxing_gloves_teach_while_worn");
    let (a, b) = (s.players[0].mob, s.players[1].mob);
    let gloves = s.gloves();

    // Worn in the gloves slot: boxing.
    {
        let world = s.server.world_mut();
        world
            .entity_mut(gloves)
            .remove::<TilePos>()
            .insert(CarriedBy(a));
        world.get_mut::<Inventory>(a).unwrap().gloves = Some(gloves);
    }
    s.run_until("the gloves teach boxing", |s| {
        martial::active_style(s.server.world(), a) == Some(Style::Boxing)
    });

    // A boxer's punch on a non-boxer: stamina only.
    s.punch();
    s.run_until("the punch tires B", |s| {
        s.try_get::<StatusEffects>(b)
            .is_some_and(|e| e.stamina > 0.0)
    });
    assert_eq!(s.get::<Health>(b).brute, 0.0, "boxing deals no brute");

    // Taken off: forgotten.
    {
        let world = s.server.world_mut();
        world
            .entity_mut(gloves)
            .remove::<CarriedBy>()
            .insert(TilePos::new(2, 2, 1));
    }
    s.run_until("the style is forgotten", |s| {
        s.try_get::<MartialArts>(a).is_none()
    });
}

/// Kaza Ruk's neck chop button readies the next punch: 10 brute to the head, 20 s of silence,
/// the readied move used up.
#[test]
fn kaza_ruk_neck_chop_silences() {
    let mut s = Station::start("kaza_ruk_neck_chop_silences");
    let (a, b) = (s.players[0].mob, s.players[1].mob);
    assert!(martial::teach(
        s.server.world_mut(),
        a,
        Style::KazaRuk,
        Origin::Learned
    ));
    s.server.world_mut().get_mut::<MartialArts>(a).unwrap().0[0].streak = "neck_chop".into();
    s.punch();
    s.run_until("the neck chop silences B", |s| {
        s.try_get::<StatusEffects>(b)
            .is_some_and(|e| e.silence > 0.0)
    });
    assert!(s.get::<Health>(b).brute >= 10.0);
    assert!(s.get::<MartialArts>(a).0[0].streak.is_empty());
}

/// Plasma fist: punch, punch, shove is the tornado sweep, which throws B away from A.
#[test]
fn plasma_fist_tornado_throws_back() {
    let mut s = Station::start("plasma_fist_tornado_throws_back");
    let (a, b) = (s.players[0].mob, s.players[1].mob);
    assert!(martial::teach(
        s.server.world_mut(),
        a,
        Style::PlasmaFist,
        Origin::Learned
    ));
    s.punch();
    // The first punch lands (and starts its cooldown) before the second is sent.
    s.run_until("one punch in the streak", |s| {
        s.get::<MartialArts>(a).0[0].streak == "H"
    });
    s.punch();
    s.run_until("two punches in the streak", |s| {
        s.get::<MartialArts>(a).0[0].streak == "HH"
    });
    s.run_until("off cooldown", |s| {
        s.server.world().resource::<Time>().elapsed_secs_f64() >= s.get::<NextMove>(a).0
    });
    s.0.click_with(
        0,
        b,
        ss13_shared::protocol::ClickModifiers {
            right: true,
            ..Default::default()
        },
    );
    s.run_until("the tornado throws B", |s| s.get::<TilePos>(b).x > 3);
    assert!(s.get::<MartialArts>(a).0[0].streak.is_empty());
}
