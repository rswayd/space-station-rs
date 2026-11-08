//! Explosions (`ss13_game::explosions`, tg `SSexplosions`) on a fixture station 31x13 tiles:
//! - the blast wave: a wall weakens it behind itself, a directional blast keeps its arc, the bomb
//!   cap limits the ranges unless ignored, `dyn_explosion` sizes by power (server only);
//! - over real WebTransport: a light blast hurts the player, rings its ears, shakes its view and
//!   throws loose items away; devastation gibs the player and destroys what lay with it, and a
//!   heavy blast dismantles a wall; a C-4 planted on a wall takes it down when its timer ends.
//!
//! `cargo test -p ss13_game --all-features --test it explosions::metastation_cap -- --ignored
//! --nocapture` times a bomb-cap explosion on MetaStation (needs `SS13_ASSETS`).

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::explosions::{
    Explosion, ExplosionConfig, ExplosionQueue, Severity, dyn_explosion, explosion,
};
use ss13_game::health::Health;
use ss13_game::inventory::Inventory;
use ss13_game::medical::organs::{self, Slot};
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_game::tools::DoAfter;
use ss13_mapload::construct::DmTurfBuild;
use ss13_mapload::{DmItem, MapLoadPlugin, Turf};
use ss13_net::camera::client::CameraEffectReceived;
use ss13_net::sound::client::SoundReceived;
use ss13_shared::TilePos;
use ss13_shared::protocol::CameraEffect;
use ss13_world::SpatialIndex;

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/datum/armor": { "parent": "/datum", "declared_vars": { "melee": { "expr": "0" },
        "bomb": { "expr": "0" } } },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 }, "declared_vars": { "can_atmos_pass": 1,
        "flags_1": 0, "max_integrity": 500, "armor_type": { "expr": "/datum/armor" },
        "integrity_failure": 0, "damage_deflection": 0, "resistance_flags": 0 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3, "anchored": 0,
        "explosion_block": 0 } },
    "/obj": { "parent": "/atom/movable", "declared_vars": { "force": 0, "damtype": "brute",
        "obj_flags": 2 } },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start" },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null, "max_integrity": 200,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null, "attack_speed": 8,
        "tool_behaviour": null, "toolspeed": 1, "heat": 0 } },
    "/obj/item/storage": { "parent": "/obj/item" },
    "/obj/item/storage/toolbox": { "parent": "/obj/item/storage", "declared_vars": {
        "name": "toolbox", "icon_state": "toolbox", "force": 13 } },
    "/obj/item/grenade": { "parent": "/obj/item" },
    "/obj/item/grenade/c4": { "parent": "/obj/item/grenade", "declared_vars": {
        "name": "C-4 charge", "icon_state": "plastic-explosive0",
        "inhand_icon_state": "plastic-explosive" } },
    "/obj/item/stack": { "parent": "/obj/item", "declared_vars": { "amount": 1,
        "max_amount": 50, "merge_type": null, "singular_name": null, "turf_type": null } },
    "/obj/item/stack/sheet": { "parent": "/obj/item/stack" },
    "/obj/item/stack/sheet/iron": { "parent": "/obj/item/stack/sheet", "declared_vars": {
        "name": "iron", "icon_state": "sheet-metal",
        "merge_type": { "path": "/obj/item/stack/sheet/iron" } } },
    "/obj/item/stack/rods": { "parent": "/obj/item/stack", "declared_vars": {
        "icon_state": "rods", "merge_type": { "path": "/obj/item/stack/rods" } } },
    "/obj/item/card": { "parent": "/obj/item", "declared_vars": { "inhand_icon_state": "card-id" } },
    "/obj/item/card/id": { "parent": "/obj/item/card", "declared_vars": {
        "access": { "list": [] }, "icon_state": "card_grey", "slot_flags": 256,
        "worn_icon_state": "nothing" } },
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
    "/obj/structure/lattice": { "parent": "/obj/structure", "declared_vars": {
        "name": "lattice", "icon_state": "lattice", "max_integrity": 50 } },
    "/mob": { "parent": "/atom/movable", "declared_vars": { "layer": 4, "density": 1 } },
    "/mob/living": { "parent": "/mob" },
    "/mob/living/carbon": { "parent": "/mob/living" },
    "/mob/living/carbon/human": { "parent": "/mob/living/carbon", "declared_vars": {
        "icon": { "resource": "h.dmi" }, "icon_state": "human_basic" } },
    "/turf": { "parent": "/atom", "declared_vars": { "icon": { "resource": "t.dmi" },
        "blocks_air": 0, "can_atmos_pass": 0, "initial_gas_mix": "o2=22;n2=82;TEMP=293.15",
        "temperature": 293.15, "baseturfs": { "path": "/turf/baseturf_bottom" },
        "explosive_resistance": 0 } },
    "/turf/open": { "parent": "/turf", "declared_vars": { "can_atmos_pass": -1,
        "planetary_atmos": 0 } },
    "/turf/open/floor": { "parent": "/turf/open", "declared_vars": { "icon_state": "floor",
        "baseturfs": { "path": "/turf/open/floor/plating" }, "overfloor_placed": 1,
        "floor_tile": null } },
    "/turf/open/floor/iron": { "parent": "/turf/open/floor" },
    "/turf/open/floor/plating": { "parent": "/turf/open/floor", "declared_vars": {
        "icon_state": "plating", "overfloor_placed": 0,
        "baseturfs": { "path": "/turf/open/space/basic" } } },
    "/turf/open/space": { "parent": "/turf/open", "declared_vars": { "temperature": 2.7,
        "initial_gas_mix": "TEMP=2.7" } },
    "/turf/open/space/basic": { "parent": "/turf/open/space" },
    "/turf/closed": { "parent": "/turf", "declared_vars": { "density": 1, "opacity": 1,
        "blocks_air": 1 } },
    "/turf/closed/wall": { "parent": "/turf/closed", "declared_vars": { "icon_state": "wall",
        "baseturfs": { "path": "/turf/open/floor/plating" }, "explosive_resistance": 1,
        "sheet_type": { "path": "/obj/item/stack/sheet/iron" }, "sheet_amount": 2,
        "girder_type": { "path": "/obj/structure/girder" }, "slicing_duration": 100 } },
    "/area": { "parent": "/atom" },
    "/area/space": { "parent": "/area" }
  }
}"##;

const STATES: [(&str, &str); 14] = [
    ("t.dmi", "floor"),
    ("t.dmi", "plating"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toolbox"),
    ("i.dmi", "plastic-explosive0"),
    ("i.dmi", "plastic-explosive2"),
    ("i.dmi", "sheet-metal"),
    ("i.dmi", "rods"),
    ("s.dmi", "girder"),
    ("s.dmi", "lattice"),
    ("l.dmi", "card-id"),
    ("i.dmi", "toy"),
];

/// Width and height of the fixture.
const W: i32 = 31;
const H: i32 = 13;
/// The wall column (y 2..=12) that blocks the wave.
const WALL_X: i32 = 16;
const SPAWN: TilePos = TilePos::new(4, 7, 1);
/// A wall next to the spawn, east.
const NEAR_WALL: TilePos = TilePos::new(5, 7, 1);
/// A C-4 charge north of the spawn.
const C4_AT: TilePos = TilePos::new(4, 8, 1);

/// Floors everywhere, the wall column, a wall by the spawn; the spawn holds a toolbox.
fn map() -> String {
    let mut rows = Vec::new();
    for y in (1..=H).rev() {
        let row: String = (1..=W)
            .map(|x| {
                let pos = TilePos::new(x, y, 1);
                if pos == SPAWN {
                    'A'
                } else if pos == C4_AT {
                    'C'
                } else if pos == NEAR_WALL || (x == WALL_X && (2..H).contains(&y)) {
                    'w'
                } else {
                    'f'
                }
            })
            .collect();
        rows.push(row);
    }
    format!(
        r#""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor/iron,/area/space)
"A" = (/obj/effect/landmark/start/assistant,/obj/item/storage/toolbox,/turf/open/floor/iron,/area/space)
"C" = (/obj/item/grenade/c4,/turf/open/floor/iron,/area/space)

(1,1,1) = {{"
{}
"}}
"#,
        rows.join("\n")
    )
}

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("explosions_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/item/storage/toolbox",
        "/obj/item/grenade/c4",
        "/obj/item/stack/sheet/iron",
        "/obj/item/stack/rods",
        "/obj/structure/girder",
        "/obj/structure/lattice",
        "/turf/open/floor/iron",
        "/turf/open/floor/plating",
        "/turf/open/space/basic",
        "/turf/closed/wall",
        "/area/space",
    ];
    write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", &map())])
}

/// The fixture station, shared with `crate::ordnance`.
pub(crate) fn server(test: &str) -> App {
    let assets = fixture_assets(test);
    testing::server_app(fixture_map(&assets, &["map.dmm"]), |app| {
        app.insert_resource(ExplosionConfig {
            bombcap: 14.0,
            reactionary: true,
        });
    })
}

fn queued(app: &App, x: i32, y: i32) -> Option<Severity> {
    app.world()
        .resource::<ExplosionQueue>()
        .queued(TilePos::new(x, y, 1))
}

#[test]
fn a_wall_weakens_the_blast_behind_it() {
    let mut app = server("a_wall_weakens_the_blast_behind_it");
    explosion(
        app.world_mut(),
        TilePos::new(12, 7, 1),
        Explosion::new(0, 0, 7),
    );
    // West, nothing in the way: 5 tiles is light.
    assert_eq!(queued(&app, 7, 7), Some(Severity::Light));
    // The wall 4 tiles east is hit; 5 tiles east, behind it, 5 + 2.5 is out of range.
    assert_eq!(queued(&app, WALL_X, 7), Some(Severity::Light));
    assert_eq!(queued(&app, WALL_X + 1, 7), None);
    // Without reactionary explosions it would be hit.
    let mut app = server("a_wall_weakens_the_blast_behind_it_off");
    app.insert_resource(ExplosionConfig {
        bombcap: 14.0,
        reactionary: false,
    });
    explosion(
        app.world_mut(),
        TilePos::new(12, 7, 1),
        Explosion::new(0, 0, 7),
    );
    assert_eq!(queued(&app, WALL_X + 1, 7), Some(Severity::Light));
}

#[test]
fn a_directional_blast_keeps_its_arc() {
    let mut app = server("a_directional_blast_keeps_its_arc");
    let ex = Explosion::new(0, 0, 4).directional(270.0, 90.0);
    explosion(app.world_mut(), TilePos::new(12, 7, 1), ex);
    assert_eq!(queued(&app, 9, 7), Some(Severity::Light));
    assert_eq!(queued(&app, 15, 7), None);
    assert_eq!(queued(&app, 12, 10), None);
}

#[test]
fn the_bomb_cap_limits_ranges_unless_ignored() {
    let mut app = server("the_bomb_cap_limits_ranges_unless_ignored");
    // Bomb cap 14: light under 14 tiles.
    explosion(
        app.world_mut(),
        TilePos::new(1, 1, 1),
        Explosion::new(0, 0, 20),
    );
    assert_eq!(queued(&app, 14, 1), Some(Severity::Light));
    assert_eq!(queued(&app, 15, 1), None);
    let mut app = server("the_bomb_cap_limits_ranges_unless_ignored_2");
    explosion(
        app.world_mut(),
        TilePos::new(1, 1, 1),
        Explosion::new(0, 0, 20).ignorecap(),
    );
    assert_eq!(queued(&app, 15, 1), Some(Severity::Light));
    assert_eq!(queued(&app, 20, 1), Some(Severity::Light));
    // `dyn_explosion(power 50)`: range 10 -> (2, 5, 10).
    let mut app = server("the_bomb_cap_limits_ranges_unless_ignored_3");
    dyn_explosion(
        app.world_mut(),
        TilePos::new(1, 1, 1),
        50.0,
        None,
        None,
        true,
    );
    assert_eq!(queued(&app, 2, 1), Some(Severity::Devastate));
    assert_eq!(queued(&app, 5, 1), Some(Severity::Heavy));
    assert_eq!(queued(&app, 6, 1), Some(Severity::Light));
    assert_eq!(queued(&app, 11, 1), None);
}

#[derive(Resource, Default)]
struct Heard {
    sounds: Vec<String>,
    shakes: Vec<CameraEffect>,
}

fn record(
    mut sounds: MessageReader<SoundReceived>,
    mut camera: MessageReader<CameraEffectReceived>,
    mut out: ResMut<Heard>,
) {
    out.sounds.extend(sounds.read().map(|s| s.0.file.clone()));
    out.shakes.extend(camera.read().map(|c| c.0));
}

struct Station(testing::Station);
ss13_game::drive_station!(Station, 0);

impl Station {
    fn start(test: &str) -> Self {
        let assets = fixture_assets(test);
        let mut s = testing::Station::start(fixture_map(&assets, &["map.dmm"]), |app| {
            // Air to carry the sound.
            app.add_plugins(ss13_atmos::AtmosPlugin);
            app.insert_resource(ExplosionConfig {
                bombcap: 14.0,
                reactionary: true,
            });
        });
        s.join("bomber", |app| {
            app.init_resource::<Heard>().add_systems(Update, record);
        });
        let s = Self(s);
        assert_eq!(s.pos(s.mob()), SPAWN);
        s
    }

    fn mob(&self) -> Entity {
        self.players[0].mob
    }

    fn world(&self) -> &World {
        self.server.world()
    }

    fn pos(&self, e: Entity) -> TilePos {
        *self.world().get::<TilePos>(e).unwrap()
    }

    fn heard(&self) -> &Heard {
        self.players[0].app.world().resource::<Heard>()
    }

    fn item_at(&self, pos: TilePos, path: &str) -> Option<Entity> {
        let world = self.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .find(|&e| world.get::<DmItem>(e).is_some_and(|i| i.path == path))
    }

    fn turf_path(&self, pos: TilePos) -> String {
        let world = self.world();
        let turf = world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .find(|&e| world.get::<Turf>(e).is_some())
            .unwrap();
        world.get::<DmTurfBuild>(turf).unwrap().path.clone()
    }

    fn explode(&mut self, at: TilePos, ex: Explosion) {
        explosion(self.0.server.world_mut(), at, ex);
    }
}

#[test]
fn a_light_blast_hurts_deafens_shakes_and_throws() {
    let mut s = Station::start("a_light_blast_hurts_deafens_shakes_and_throws");
    let mob = s.mob();
    let toolbox = s.item_at(SPAWN, "/obj/item/storage/toolbox").unwrap();
    // 4 tiles south of the epicenter, a light blast (6 + 7 - 2 = 11: the near sound).
    s.explode(TilePos::new(4, 11, 1), Explosion::new(0, 0, 6));
    s.run_until("the player is hurt", |s| {
        s.get::<Health>(mob).health() < 100.0
    });
    // `/mob/living/carbon/human/ex_act` light: 30 brute over the six bodyparts; a limb blown
    // off (no bomb armor: dismemberment may roll even at light) takes its share with it and
    // hurts the chest by 15 (`dismember`: half its 5 damage, at least 15).
    let health = s.get::<Health>(mob).clone();
    let lost = health.missing.count_ones() as f32;
    let present = 6.0 - lost;
    assert!(
        (health.brute - 5.0 * present - 15.0 * lost).abs() < 0.05,
        "{health:?}"
    );
    assert_eq!(health.burn, 0.0);
    let ears = organs::state(s.world(), mob, Slot::Ears).expect("ears");
    assert!(
        ears.deafness > 100.0,
        "ringing for ~120 s: {}",
        ears.deafness
    );
    assert!(ears.damage >= 14.0, "15 ear damage: {}", ears.damage);
    // Thrown south, away from the epicenter.
    s.run_until("the toolbox lands south", |s| {
        s.world()
            .get::<TilePos>(toolbox)
            .is_some_and(|p| p.y < SPAWN.y)
    });
    s.run_until("the client shakes", |s| {
        s.heard()
            .shakes
            .iter()
            .any(|e| matches!(e, CameraEffect::Shake { strength, .. } if *strength > 0.0))
    });
    // No SFX table in the fixture: the key itself is played.
    s.run_until("the client hears the blast", |s| {
        s.heard().sounds.iter().any(|f| f == "explosion")
    });
}

#[test]
fn devastation_gibs_and_heavy_dismantles_walls() {
    let mut s = Station::start("devastation_gibs_and_heavy_dismantles_walls");
    let mob = s.mob();
    let toolbox = s.item_at(SPAWN, "/obj/item/storage/toolbox").unwrap();
    // The spawn devastated, the wall next to it (1 tile) heavy.
    s.explode(SPAWN, Explosion::new(1, 2, 3));
    s.run_until("the player is gibbed", |s| {
        s.world().get_entity(mob).is_err()
    });
    s.run_until("the toolbox is destroyed", |s| {
        s.world().get_entity(toolbox).is_err()
    });
    s.run_until("the wall is dismantled", |s| {
        s.turf_path(NEAR_WALL) != "/turf/closed/wall"
    });
}

#[test]
fn c4_planted_on_a_wall_takes_it_down() {
    let mut s = Station::start("c4_planted_on_a_wall_takes_it_down");
    let mob = s.mob();
    let c4 = s.item_at(C4_AT, "/obj/item/grenade/c4").unwrap();
    s.run_until("the charge is replicated", |s| s.net_id(0, c4).is_some());
    s.click(0, c4);
    s.run_until("picked up", |s| {
        s.world().get::<Inventory>(mob).unwrap().active_item() == Some(c4)
    });
    let wall = {
        let world = s.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(NEAR_WALL)
            .iter()
            .copied()
            .find(|&e| world.get::<Turf>(e).is_some())
            .unwrap()
    };
    s.run_until("the wall is replicated", |s| s.net_id(0, wall).is_some());
    s.click(0, wall);
    s.run_until("planting starts", |s| {
        s.world().get::<DoAfter>(mob).is_some()
    });
    s.run_until_for(5.0, "the charge is planted", |s| {
        s.world()
            .get::<ss13_game::explosions::sources::Charge>(c4)
            .is_some_and(|c| c.planted.is_some())
    });
    assert!(
        s.world()
            .get::<Inventory>(mob)
            .unwrap()
            .active_item()
            .is_none(),
        "the charge left the hand"
    );
    // The default 10 s timer: the wall takes a heavy blast, then the charge explodes.
    s.run_until_for(15.0, "the wall comes down", |s| {
        s.turf_path(NEAR_WALL) != "/turf/closed/wall"
    });
    s.run_until("the charge is gone", |s| s.world().get_entity(c4).is_err());
}

fn metastation_map() -> MapLoadPlugin {
    MapLoadPlugin {
        maps: vec!["MetaStation".into()],
        ..MapLoadPlugin::from_env().unwrap_or(MapLoadPlugin {
            source: env!("SS13_SOURCE").into(),
            maps: vec![],
            assets: std::env::var("SS13_ASSETS")
                .unwrap_or_else(|_| "assets/generated".into())
                .into(),
        })
    }
}

/// A bomb-cap explosion (3, 7, 14) on MetaStation's bridge (the communications console): how
/// long the wave takes to queue (`explosion()`) and the subsystem's round to blast it
/// (`fire()`, one tick), after the station has settled.
#[test]
#[ignore = "needs generated assets and MetaStation; run with --ignored"]
fn metastation_cap() {
    let mut app = testing::server_app(metastation_map(), |_| {});
    for _ in 0..20 {
        app.update();
    }
    let center = {
        let world = app.world_mut();
        let mut q = world.query::<(&ss13_mapload::DmObj, &TilePos)>();
        q.iter(world)
            .find(|(o, _)| o.path == "/obj/machinery/computer/communications")
            .map(|(_, p)| *p)
            .expect("a communications console")
    };
    let count = |app: &mut App| {
        let world = app.world_mut();
        world.query::<&TilePos>().iter(world).count()
    };
    let before = count(&mut app);
    let started = std::time::Instant::now();
    explosion(
        app.world_mut(),
        center,
        Explosion::new(3, 7, 14).cause("test"),
    );
    let wave = started.elapsed();
    let started = std::time::Instant::now();
    ss13_game::explosions::fire(app.world_mut());
    let blast = started.elapsed();
    let started = std::time::Instant::now();
    app.update();
    let tick = started.elapsed();
    let started = std::time::Instant::now();
    app.update();
    let next = started.elapsed();
    // A second blast 20 tiles west: the debris templates are cached by now.
    let second = TilePos::new(center.x - 20, center.y, center.z);
    explosion(
        app.world_mut(),
        second,
        Explosion::new(3, 7, 14).cause("test"),
    );
    let started = std::time::Instant::now();
    ss13_game::explosions::fire(app.world_mut());
    let blast2 = started.elapsed();
    println!(
        "second blast at {second:?}: {:.2} ms",
        blast2.as_secs_f64() * 1000.0
    );
    println!(
        "MetaStation bomb cap at {center:?}: wave {:.2} ms, blast {:.2} ms, the tick after \
         {:.2} ms, the next {:.2} ms; entities on tiles {before} -> {}",
        wave.as_secs_f64() * 1000.0,
        blast.as_secs_f64() * 1000.0,
        tick.as_secs_f64() * 1000.0,
        next.as_secs_f64() * 1000.0,
        count(&mut app),
    );
}
