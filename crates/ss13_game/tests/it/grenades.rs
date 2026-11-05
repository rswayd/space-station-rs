//! Grenades (`ss13_game::grenades`) on a fixture floor 25x11 with a wall column at x = 20, no
//! clients: a minibomb's fuse, an EMP grenade's `empulse(4, 10)`, a flashbang flooring the mob
//! next to it and staggering one further out (not one behind the wall), a smoke cloud spreading
//! over `DIAMOND_AREA(4)` tiles that chokes a mob and clears after 16 s, a clusterbang scattering
//! armed payloads that all go off, and a grenade destroyed by a blast going off.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::explosions::{Explosion, ExplosionQueue, explosion};
use ss13_game::grenades::{self, Armed, Grenade, Smoke};
use ss13_game::health::{Health, Stat};
use ss13_game::inventory::Inventory;
use ss13_game::mecha::core::ItemCell;
use ss13_game::power::Cell;
use ss13_game::status_effects::StatusEffects;
use ss13_game::testing::{self, fixture_map, steps, write_fixture};
use ss13_mapload::DmItem;
use ss13_shared::TilePos;

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
    "/obj/effect/particle_effect": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "anchored": 1 } },
    "/obj/effect/particle_effect/fluid": { "parent": "/obj/effect/particle_effect" },
    "/obj/effect/particle_effect/fluid/smoke": { "parent": "/obj/effect/particle_effect/fluid",
        "declared_vars": { "name": "smoke", "opacity": 1 } },
    "/obj/effect/particle_effect/fluid/smoke/bad": {
        "parent": "/obj/effect/particle_effect/fluid/smoke" },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null, "max_integrity": 200,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null, "attack_speed": 8,
        "tool_behaviour": null, "toolspeed": 1, "heat": 0 } },
    "/obj/item/grenade": { "parent": "/obj/item", "declared_vars": { "name": "grenade",
        "max_integrity": 40 } },
    "/obj/item/grenade/flashbang": { "parent": "/obj/item/grenade", "declared_vars": {
        "name": "flashbang" } },
    "/obj/item/grenade/flashbang/cluster": { "parent": "/obj/item/grenade/flashbang" },
    "/obj/item/grenade/clusterbuster": { "parent": "/obj/item/grenade", "declared_vars": {
        "name": "clusterbang" } },
    "/obj/item/grenade/clusterbuster/segment": { "parent": "/obj/item/grenade/clusterbuster" },
    "/obj/item/card": { "parent": "/obj/item" },
    "/obj/item/card/id": { "parent": "/obj/item/card", "declared_vars": { "slot_flags": 256 } },
    "/obj/item/card/id/advanced": { "parent": "/obj/item/card/id" },
    "/obj/item/clothing": { "parent": "/obj/item" },
    "/obj/item/clothing/under": { "parent": "/obj/item/clothing" },
    "/obj/item/clothing/under/color": { "parent": "/obj/item/clothing/under" },
    "/obj/item/clothing/under/color/grey": { "parent": "/obj/item/clothing/under/color" },
    "/obj/item/clothing/shoes": { "parent": "/obj/item/clothing" },
    "/obj/item/clothing/shoes/sneakers": { "parent": "/obj/item/clothing/shoes" },
    "/obj/item/clothing/shoes/sneakers/black": { "parent": "/obj/item/clothing/shoes/sneakers" },
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
        "baseturfs": { "path": "/turf/open/floor/plating" }, "explosive_resistance": 1 } },
    "/area": { "parent": "/atom" },
    "/area/space": { "parent": "/area" }
  }
}"##;

const STATES: [(&str, &str); 6] = [
    ("t.dmi", "floor"),
    ("t.dmi", "plating"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
];

const W: i32 = 25;
const H: i32 = 11;
const WALL_X: i32 = 20;
const SPAWN: TilePos = TilePos::new(2, 2, 1);

const MINIBOMB: &str = "/obj/item/grenade/syndieminibomb";
const EMP: &str = "/obj/item/grenade/empgrenade";
const FLASHBANG: &str = "/obj/item/grenade/flashbang";
const SMOKE: &str = "/obj/item/grenade/smokebomb";
const CLUSTER: &str = "/obj/item/grenade/clusterbuster";

fn map() -> String {
    let mut rows = Vec::new();
    for y in (1..=H).rev() {
        let row: String = (1..=W)
            .map(|x| {
                if TilePos::new(x, y, 1) == SPAWN {
                    'A'
                } else if x == WALL_X {
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
"A" = (/obj/effect/landmark/start/assistant,/turf/open/floor/iron,/area/space)

(1,1,1) = {{"
{}
"}}
"#,
        rows.join("\n")
    )
}

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("grenades_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/effect/particle_effect/fluid/smoke/bad",
        "/obj/item/grenade/flashbang/cluster",
        "/obj/item/grenade/clusterbuster/segment",
        "/turf/open/floor/iron",
        "/turf/open/floor/plating",
        "/turf/open/space/basic",
        "/turf/closed/wall",
        "/area/space",
    ];
    write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", &map())])
}

fn server(test: &str) -> App {
    let assets = fixture_assets(test);
    let mut app = testing::server_app(fixture_map(&assets, &["map.dmm"]), |app| {
        app.add_plugins(ss13_atmos::AtmosPlugin);
    });
    app.update();
    app
}

const fn at(x: i32, y: i32) -> TilePos {
    TilePos::new(x, y, 1)
}

/// A grenade of `path` on `pos` (no prototype needed: the grenade's state comes from its type).
fn grenade(app: &mut App, path: &str, pos: TilePos) -> Entity {
    let e = app
        .world_mut()
        .spawn((
            DmItem {
                path: path.into(),
                ..default()
            },
            pos,
        ))
        .id();
    app.update();
    e
}

/// A living mob with no organs or client on `pos`.
fn mob(app: &mut App, pos: TilePos) -> Entity {
    app.world_mut()
        .spawn((pos, Health::default(), Stat::Conscious))
        .id()
}

fn run(app: &mut App, secs: f64) {
    for _ in 0..steps(secs) {
        app.update();
    }
}

fn exists(app: &App, e: Entity) -> bool {
    app.world().get_entity(e).is_ok()
}

fn explosions(app: &App) -> u32 {
    app.world().resource::<ExplosionQueue>().count
}

fn effects(app: &App, e: Entity) -> StatusEffects {
    app.world()
        .get::<StatusEffects>(e)
        .cloned()
        .unwrap_or_default()
}

#[test]
fn a_minibomb_goes_off_after_its_fuse() {
    let mut app = server("a_minibomb_goes_off_after_its_fuse");
    let bomb = grenade(&mut app, MINIBOMB, at(5, 6));
    grenades::arm(app.world_mut(), bomb, None, None);
    assert!(app.world().get::<Armed>(bomb).is_some());
    assert_eq!(app.world().get::<Grenade>(bomb).unwrap().det_time, 5.0);
    run(&mut app, 4.8);
    assert!(exists(&app, bomb), "not before the 5 s fuse");
    assert_eq!(explosions(&app), 0);
    run(&mut app, 0.4);
    assert!(!exists(&app, bomb), "gone after the fuse");
    assert_eq!(explosions(&app), 1, "one explosion (1, 2, 4, flame 2)");
}

#[test]
fn an_emp_grenade_pulses_heavy_4_light_10() {
    let mut app = server("an_emp_grenade_pulses_heavy_4_light_10");
    const FULL: f64 = 100_000.0;
    let cell = || {
        ItemCell(Cell {
            charge: FULL,
            maxcharge: FULL,
            chargerate: 1000.0,
        })
    };
    let near = app.world_mut().spawn((at(7, 6), cell())).id();
    let mid = app.world_mut().spawn((at(12, 6), cell())).id();
    let far = app.world_mut().spawn((at(16, 6), cell())).id();
    let emp = grenade(&mut app, EMP, at(5, 6));
    grenades::arm(app.world_mut(), emp, None, Some(0.0));
    run(&mut app, 0.2);
    assert!(!exists(&app, emp));
    let charge = |e| app.world().get::<ItemCell>(e).unwrap().0.charge;
    assert_eq!(charge(near), FULL - 10_000.0, "heavy at 2 tiles");
    assert_eq!(charge(mid), FULL - 5_000.0, "light at 7 tiles");
    assert_eq!(charge(far), FULL, "beyond 10 tiles");
}

#[test]
fn a_flashbang_floors_the_near_and_staggers_the_far() {
    let mut app = server("a_flashbang_floors_the_near_and_staggers_the_far");
    let near = mob(&mut app, at(16, 6));
    let far = mob(&mut app, at(9, 6));
    let walled = mob(&mut app, at(21, 6));
    let bang = grenade(&mut app, FLASHBANG, at(15, 6));
    assert_eq!(
        grenades::grenade(app.world_mut(), bang).map(|g| g.fuses.to_vec()),
        Some(vec![3.0, 4.0, 5.0])
    );
    grenades::arm(app.world_mut(), bang, None, Some(0.0));
    app.update();
    assert!(!exists(&app, bang));
    // 1 tile: the flash paralyzes 2 s and knocks down 20 s; the bang adds 3 s knockdown.
    let e = effects(&app, near);
    assert!(e.knockdown > 19.0, "{e:?}");
    assert!(e.paralyze > 1.5, "{e:?}");
    // 6 tiles, past the 3-tile sweet spot: no flash stun, staggered max(200 / 6, 5) ds.
    let e = effects(&app, far);
    assert_eq!(e.knockdown, 0.0, "{e:?}");
    assert!(e.staggered > 3.0 && e.staggered <= 3.4, "{e:?}");
    // Behind the wall: unseen, unheard.
    assert_eq!(effects(&app, walled), StatusEffects::default());
}

#[test]
fn smoke_fills_a_diamond_chokes_and_clears() {
    let mut app = server("smoke_fills_a_diamond_chokes_and_clears");
    let center = at(10, 6);
    let victim = app
        .world_mut()
        .spawn((
            at(11, 6),
            Health::default(),
            Stat::Conscious,
            Inventory::default(),
        ))
        .id();
    let smoke = grenade(&mut app, SMOKE, center);
    grenades::arm(app.world_mut(), smoke, None, Some(0.0));
    // A generation of spreading every 0.1 s.
    run(&mut app, 1.0);
    let tiles: Vec<TilePos> = {
        let world = app.world_mut();
        world
            .query_filtered::<&TilePos, With<Smoke>>()
            .iter(world)
            .copied()
            .collect()
    };
    // `DIAMOND_AREA(4)` = 41 tiles, cardinal spreading: the diamond of radius 4.
    assert_eq!(tiles.len(), 41);
    assert!(
        tiles
            .iter()
            .all(|t| (t.x - center.x).abs() + (t.y - center.y).abs() <= 4)
    );
    assert!(app.world().get::<Health>(victim).unwrap().oxy >= 1.0);
    run(&mut app, 16.5);
    let left = {
        let world = app.world_mut();
        world
            .query_filtered::<(), With<Smoke>>()
            .iter(world)
            .count()
    };
    assert_eq!(left, 0, "the smoke clears after 16 s");
}

#[test]
fn a_clusterbang_scatters_armed_payloads_that_all_go_off() {
    let mut app = server("a_clusterbang_scatters_armed_payloads_that_all_go_off");
    let cluster = grenade(&mut app, CLUSTER, at(10, 6));
    grenades::arm(app.world_mut(), cluster, None, Some(0.0));
    app.update();
    assert!(!exists(&app, cluster));
    let armed: Vec<(Grenade, Armed)> = {
        let world = app.world_mut();
        world
            .query::<(&Grenade, &Armed)>()
            .iter(world)
            .map(|(g, a)| (g.clone(), *a))
            .collect()
    };
    assert!(
        (4..=8).contains(&armed.len()),
        "rand(4, 8) payloads and segments: {}",
        armed.len()
    );
    assert!(armed.iter().all(|(g, _)| g.cluster));
    // Payloads and segments go off within 6 s; a segment's payloads within 6 s more.
    run(&mut app, 12.5);
    let left = {
        let world = app.world_mut();
        world
            .query_filtered::<(), With<Armed>>()
            .iter(world)
            .count()
    };
    assert_eq!(left, 0);
}

#[test]
fn a_grenade_destroyed_by_a_blast_goes_off() {
    let mut app = server("a_grenade_destroyed_by_a_blast_goes_off");
    let bomb = grenade(&mut app, MINIBOMB, at(5, 6));
    explosion(app.world_mut(), at(5, 6), Explosion::new(1, 1, 1));
    assert_eq!(explosions(&app), 1);
    run(&mut app, 0.3);
    assert!(!exists(&app, bomb));
    assert_eq!(explosions(&app), 2, "the minibomb's own explosion");
}
