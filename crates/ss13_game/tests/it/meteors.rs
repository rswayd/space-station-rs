//! Meteors against people and shields (`ss13_game::events::meteors`, `ss13_game::station_goals`)
//! on a 30x30 level of open space: punching, batting and mining a meteor, the meteor mode,
//! emagged meteor shields and the dark matt-eor reflecting a shield's beam.

use std::path::Path;

use bevy::prelude::*;
use ss13_game::combat::CombatMode;
use ss13_game::events::meteors::{self, Meteor, kind_by_path, spawn_meteor_at};
use ss13_game::events::{Events, controller::station_levels};
use ss13_game::skills::{LEVEL_LEGENDARY, Skill, Skills};
use ss13_game::station_goals::{self, EmaggedShield, METEOR_SHIELD, ShieldEmags};
use ss13_game::testing::{fixture_map, server_app, write_fixture};
use ss13_mapload::{DmItem, DmObj, DmTool};
use ss13_shared::TilePos;

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 }, "declared_vars": { "flags_1": 0 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3 } },
    "/obj": { "parent": "/atom/movable" },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start" },
    "/mob": { "parent": "/atom/movable", "declared_vars": { "layer": 4, "density": 1 } },
    "/turf": { "parent": "/atom", "declared_vars": { "icon": { "resource": "t.dmi" } } },
    "/turf/open": { "parent": "/turf" },
    "/turf/open/space": { "parent": "/turf/open" },
    "/turf/open/space/basic": { "parent": "/turf/open/space" },
    "/area": { "parent": "/atom", "declared_vars": { "area_flags": 0, "requires_power": 0 } },
    "/area/space": { "parent": "/area" }
  }
}"##;

const STATES: [(&str, &str); 2] = [("t.dmi", "space"), ("l.dmi", "x2")];
const SIZE: usize = 30;

fn fixture(test: &str) -> App {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("meteors_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/turf/open/space/basic",
        "/area/space",
    ];
    let mut rows: Vec<String> = (0..SIZE).map(|_| "e".repeat(SIZE)).collect();
    rows[SIZE - 1].replace_range(0..1, "s");
    let map = format!(
        "\"e\" = (/turf/open/space/basic,/area/space)\n\
         \"s\" = (/obj/effect/landmark/start/assistant,/turf/open/space/basic,/area/space)\n\n\
         (1,1,1) = {{\"\n{}\n\"}}\n",
        rows.join("\n")
    );
    let assets = write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", &map)]);
    server_app(fixture_map(&assets, &["map.dmm"]), |_| {})
}

fn t(x: i32, y: i32) -> TilePos {
    TilePos::new(x, y, 1)
}

fn medium(app: &mut App, at: TilePos, goal: TilePos) -> Entity {
    let kind = kind_by_path("/obj/effect/meteor/medium").unwrap();
    spawn_meteor_at(app.world_mut(), kind, at, goal).unwrap()
}

fn athlete(app: &mut App, at: TilePos, level: u8, combat: bool) -> Entity {
    let mut skills = Skills::default();
    skills.set_level(Skill::Athletics, level);
    app.world_mut().spawn((at, skills, CombatMode(combat))).id()
}

fn shield(app: &mut App, at: TilePos) -> Entity {
    app.world_mut()
        .spawn((
            at,
            DmObj {
                path: METEOR_SHIELD.into(),
                ..default()
            },
        ))
        .id()
}

fn weight(app: &App, name: &str) -> i64 {
    let events = app.world().resource::<Events>();
    events.controls[events.index(name).unwrap()].weight
}

/// A legendary athlete in combat mode punches a meteor off to the edge away from them; out of
/// combat mode, or less fit, the punch does nothing.
#[test]
fn a_legendary_punch_sends_a_meteor_away() {
    let mut app = fixture("punch");
    let meteor = medium(&mut app, t(12, 15), t(12, 16));
    let weak = athlete(&mut app, t(10, 15), 3, true);
    let calm = athlete(&mut app, t(10, 15), LEVEL_LEGENDARY, false);
    let champ = athlete(&mut app, t(10, 15), LEVEL_LEGENDARY, true);
    assert!(!meteors::punch(app.world_mut(), weak, meteor));
    assert!(!meteors::punch(app.world_mut(), calm, meteor));
    assert!(meteors::punch(app.world_mut(), champ, meteor));
    // Due east of the athlete: off to the east edge, 13 tiles in.
    let dest = app.world().get::<Meteor>(meteor).unwrap().dest;
    assert_eq!(dest.x, SIZE as i32 - 13);
    assert!((13..=SIZE as i32 - 13).contains(&dest.y));
}

/// A baseball bat is too heavy below expert athletics; a mining tool breaks the meteor.
#[test]
fn a_pickaxe_breaks_a_meteor_and_a_bat_needs_fitness() {
    let mut app = fixture("pickaxe");
    let meteor = medium(&mut app, t(12, 15), t(12, 16));
    let user = athlete(&mut app, t(11, 15), 1, true);
    let client = app.world_mut().spawn_empty().id();
    let bat = app
        .world_mut()
        .spawn(DmItem {
            path: "/obj/item/melee/baseball_bat".into(),
            ..default()
        })
        .id();
    assert!(meteors::attacked(
        app.world_mut(),
        client,
        user,
        bat,
        meteor
    ));
    assert_eq!(
        app.world().get::<Meteor>(meteor).unwrap().dest,
        t(12, 16),
        "not budged"
    );
    let pickaxe = app
        .world_mut()
        .spawn((
            DmItem {
                path: "/obj/item/pickaxe".into(),
                ..default()
            },
            DmTool {
                behaviour: "mining".into(),
                ..default()
            },
        ))
        .id();
    assert!(meteors::attacked(
        app.world_mut(),
        client,
        user,
        pickaxe,
        meteor
    ));
    assert!(app.world().get_entity(meteor).is_err(), "broken up");
}

/// The meteor mode throws meteors every second once its delay is over, and stops when
/// stopped.
#[test]
fn meteor_mode_throws_meteors_after_its_delay() {
    let mut app = fixture("mode");
    assert!(station_levels(app.world()).contains(&1));
    let count = |app: &mut App| {
        let world = app.world_mut();
        world.query::<&Meteor>().iter(world).count()
    };
    meteors::start_meteor_mode(app.world_mut(), 1e9, 5.0);
    for _ in 0..5 {
        app.update();
    }
    assert_eq!(count(&mut app), 0, "still waiting");
    meteors::start_meteor_mode(app.world_mut(), 0.0, 5.0);
    app.update();
    assert!(count(&mut app) >= 1);
    meteors::stop_meteor_mode(app.world_mut());
    for _ in 0..ss13_game::testing::steps(20.0) {
        app.update();
    }
    assert_eq!(count(&mut app), 0, "the last ones flew off");
}

/// An emagged meteor shield counts while active: the meteor events' weights double, and come
/// back when it is switched off or destroyed; one emag a minute; the tenth active emagged
/// shield brings the dark matt-eor.
#[test]
fn emagged_meteor_shields_draw_meteors_and_the_dark_matteor() {
    let mut app = fixture("emag");
    let client = app.world_mut().spawn_empty().id();
    let base = weight(&app, meteors::WAVE_NORMAL);
    let stray = weight(&app, meteors::STRAY_METEOR);
    let first = shield(&mut app, t(5, 5));
    let second = shield(&mut app, t(6, 5));
    app.update();
    // Inactive: emagged, but nothing counts yet.
    assert!(station_goals::emag_act(
        app.world_mut(),
        client,
        client,
        first
    ));
    assert!(app.world().get::<EmaggedShield>(first).is_some());
    assert_eq!(weight(&app, meteors::WAVE_NORMAL), base);
    assert!(
        !station_goals::emag_act(app.world_mut(), client, client, first),
        "already"
    );
    assert!(
        !station_goals::emag_act(app.world_mut(), client, client, second),
        "cooldown"
    );
    assert!(station_goals::toggle(app.world_mut(), first, None));
    assert_eq!(weight(&app, meteors::WAVE_NORMAL), base * 2);
    assert_eq!(weight(&app, meteors::STRAY_METEOR), stray * 2);
    assert!(station_goals::toggle(app.world_mut(), first, None));
    assert_eq!(weight(&app, meteors::WAVE_NORMAL), base, "switched off");
    // A destroyed active one counts down.
    assert!(station_goals::toggle(app.world_mut(), first, None));
    assert_eq!(weight(&app, meteors::WAVE_NORMAL), base * 2);
    app.world_mut().despawn(first);
    app.update();
    assert_eq!(weight(&app, meteors::WAVE_NORMAL), base);
    assert!(app.world().resource::<ShieldEmags>().counted.is_empty());
    // Ten active emagged shields.
    let mut shields = vec![second];
    shields.extend((0..9).map(|i| shield(&mut app, t(5 + i, 6))));
    for &s in &shields {
        assert!(station_goals::toggle(app.world_mut(), s, None));
        app.world_mut().resource_mut::<ShieldEmags>().cooldown_until = 0.0;
        assert!(station_goals::emag_act(app.world_mut(), client, client, s));
    }
    assert_eq!(weight(&app, meteors::WAVE_NORMAL), base << 10);
    assert_eq!(app.world().resource::<ShieldEmags>().highest, 10);
    assert!(
        app.world()
            .resource::<Events>()
            .running()
            .any(|n| n == meteors::DARK_MATTEOR),
        "the dark matt-eor is coming"
    );
}

/// An active shield destroys a meteor in reach through space, but a dark matt-eor reflects the
/// beam and destroys the shield.
#[test]
fn a_dark_matteor_destroys_the_shield_that_shoots_it() {
    let mut app = fixture("dark_matteor");
    let sat = shield(&mut app, t(5, 15));
    app.update();
    assert!(station_goals::toggle(app.world_mut(), sat, None));
    let meteor = medium(&mut app, t(9, 15), t(20, 15));
    app.update();
    assert!(app.world().get_entity(meteor).is_err(), "shot down");
    let dark = kind_by_path("/obj/effect/meteor/dark_matteor").unwrap();
    let dark = spawn_meteor_at(app.world_mut(), dark, t(9, 15), t(20, 15)).unwrap();
    app.update();
    assert!(app.world().get::<Meteor>(dark).is_some(), "reflected");
    assert!(app.world().get_entity(sat).is_err(), "the shield is gone");
}
