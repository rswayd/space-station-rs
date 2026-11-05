//! Effect systems (`ss13_game::effects`) and weather (`ss13_game::weather`) on a fixture floor
//! 25x11 (a hallway, a maintenance strip at x >= 22) with a wall column at x = 20, no clients:
//! sparks flying off and fading, metal foam spreading over the diamond, stopping at the wall
//! and leaving foamed metal, a foam reaction in a container making foam, sleeping smoke putting
//! a mob to sleep, the floor is lava burning a player on the floor but not one on a table (two
//! clients), and the radiation storm sparing maintenance.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::chem::{Chem, Reagents};
use ss13_game::effects::foam::{Foam, FoamKind, FoamOptions, do_foam};
use ss13_game::effects::particles::{Particle, do_sparks};
use ss13_game::effects::smoke::{SmokeKind, do_smoke};
use ss13_game::health::{Health, Stat};
use ss13_game::inventory::Inventory;
use ss13_game::status_effects::{Incap, StatusEffects};
use ss13_game::testing::{self, Drive, fixture_map, steps, write_fixture};
use ss13_game::weather::{self, ActiveWeather, WeatherKind};
use ss13_mapload::DmObj;
use ss13_mapload::convert::DmTypes;
use ss13_reagents::{Holder, Owner};
use ss13_shared::TilePos;

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/datum/armor": { "parent": "/datum", "declared_vars": { "melee": { "expr": "0" },
        "bomb": { "expr": "0" } } },
    "/datum/reagent": { "parent": "/datum", "declared_vars": {
        "abstract_type": { "path": "/datum/reagent" }, "name": "", "description": "",
        "color": "#000000", "taste_description": "metaphorical salt", "taste_mult": 1,
        "metabolization_rate": 0.2, "overdose_threshold": 0, "ph": 7, "specific_heat": 200,
        "chemical_flags": 0, "purity": 1, "creation_purity": 1, "inverse_chem": null,
        "inverse_chem_val": 0.25, "purge_multiplier": 1 } },
    "/datum/reagent/water": { "parent": "/datum/reagent", "declared_vars": {
        "name": "Water", "color": "#AAAAAA77" } },
    "/datum/reagent/fluorosurfactant": { "parent": "/datum/reagent", "declared_vars": {
        "name": "Fluorosurfactant", "color": "#9E6B38" } },
    "/datum/chemical_reaction": { "parent": "/datum", "declared_vars": {
        "H_ion_release": 0, "determin_ph_range": 4, "is_cold_recipe": 0,
        "mix_message": "The solution begins to bubble.", "mob_react": 1, "optimal_ph_max": 9,
        "optimal_ph_min": 5, "optimal_temp": 500, "overheat_temp": 900, "ph_exponent_factor": 2,
        "purity_min": 0.15, "rate_up_lim": 30, "reaction_flags": 0,
        "required_catalysts": { "list": [] }, "required_container": null,
        "required_container_accepts_subtypes": 0, "required_reagents": { "list": [] },
        "required_temp": 0, "results": { "list": [] }, "temp_exponent_factor": 2,
        "thermic_constant": 0 } },
    "/datum/chemical_reaction/foam": { "parent": "/datum/chemical_reaction",
        "declared_vars": { "mob_react": 0, "reaction_flags": 2,
        "required_reagents": { "list": [
            { "assoc": [{ "path": "/datum/reagent/fluorosurfactant" }, 1] },
            { "assoc": [{ "path": "/datum/reagent/water" }, 1] }] } } },
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
    "/obj/effect/particle_effect/sparks": { "parent": "/obj/effect/particle_effect",
        "declared_vars": { "name": "sparks" } },
    "/obj/effect/particle_effect/fluid": { "parent": "/obj/effect/particle_effect" },
    "/obj/effect/particle_effect/fluid/smoke": { "parent": "/obj/effect/particle_effect/fluid",
        "declared_vars": { "name": "smoke", "opacity": 1 } },
    "/obj/effect/particle_effect/fluid/smoke/sleeping": {
        "parent": "/obj/effect/particle_effect/fluid/smoke" },
    "/obj/effect/particle_effect/fluid/foam": { "parent": "/obj/effect/particle_effect/fluid",
        "declared_vars": { "name": "foam" } },
    "/obj/effect/particle_effect/fluid/foam/metal": {
        "parent": "/obj/effect/particle_effect/fluid/foam" },
    "/obj/structure": { "parent": "/obj", "declared_vars": { "icon": { "resource": "i.dmi" },
        "icon_state": "toy" } },
    "/obj/structure/foamedmetal": { "parent": "/obj/structure", "declared_vars": {
        "name": "foamed metal", "density": 1, "opacity": 1, "anchored": 1,
        "max_integrity": 20, "can_atmos_pass": -2 } },
    "/obj/structure/table": { "parent": "/obj/structure", "declared_vars": { "density": 1,
        "anchored": 1 } },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null, "max_integrity": 200,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null, "attack_speed": 8,
        "tool_behaviour": null, "toolspeed": 1, "heat": 0 } },
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
    "/area/space": { "parent": "/area" },
    "/area/station": { "parent": "/area" },
    "/area/station/hallway": { "parent": "/area/station" },
    "/area/station/maintenance": { "parent": "/area/station" }
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
const MAINT_X: i32 = 22;
const SPAWN: TilePos = TilePos::new(2, 2, 1);
const TABLE: TilePos = TilePos::new(5, 9, 1);

fn map() -> String {
    let mut rows = Vec::new();
    for y in (1..=H).rev() {
        let row: String = (1..=W)
            .map(|x| {
                let pos = TilePos::new(x, y, 1);
                if pos == SPAWN {
                    'A'
                } else if pos == TABLE {
                    't'
                } else if x == WALL_X {
                    'w'
                } else if x >= MAINT_X {
                    'm'
                } else {
                    'f'
                }
            })
            .collect();
        rows.push(row);
    }
    format!(
        r#""w" = (/turf/closed/wall,/area/station/hallway)
"f" = (/turf/open/floor/iron,/area/station/hallway)
"t" = (/obj/structure/table,/turf/open/floor/iron,/area/station/hallway)
"m" = (/turf/open/floor/iron,/area/station/maintenance)
"A" = (/obj/effect/landmark/start/assistant,/turf/open/floor/iron,/area/station/hallway)

(1,1,1) = {{"
{}
"}}
"#,
        rows.join("\n")
    )
}

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("effects_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/effect/particle_effect/sparks",
        "/obj/effect/particle_effect/fluid/smoke/sleeping",
        "/obj/effect/particle_effect/fluid/foam",
        "/obj/effect/particle_effect/fluid/foam/metal",
        "/obj/structure/foamedmetal",
        "/obj/structure/table",
        "/turf/open/floor/iron",
        "/turf/open/floor/plating",
        "/turf/open/space/basic",
        "/turf/closed/wall",
        "/area/space",
        "/area/station/hallway",
        "/area/station/maintenance",
    ];
    let dir = write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", &map())]);
    let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
    let (chem, _) = ss13_mapload::reagents::chem_data(&types);
    std::fs::write(dir.join(ss13_reagents::CHEM_FILE), chem.to_ron()).unwrap();
    dir
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

fn run(app: &mut App, secs: f64) {
    for _ in 0..steps(secs) {
        app.update();
    }
}

fn foam_tiles(app: &mut App) -> Vec<TilePos> {
    let world = app.world_mut();
    world
        .query_filtered::<&TilePos, With<Foam>>()
        .iter(world)
        .copied()
        .collect()
}

fn count_objs(app: &mut App, path: &str) -> usize {
    let world = app.world_mut();
    world
        .query::<&DmObj>()
        .iter(world)
        .filter(|o| o.path == path)
        .count()
}

#[test]
fn sparks_fly_off_and_fade() {
    let mut app = server("sparks_fly_off_and_fade");
    let origin = at(10, 6);
    do_sparks(app.world_mut(), 3, true, origin);
    fn particles(app: &mut App) -> Vec<TilePos> {
        let world = app.world_mut();
        world
            .query_filtered::<&TilePos, With<Particle>>()
            .iter(world)
            .copied()
            .collect()
    }
    assert_eq!(particles(&mut app).len(), 3);
    run(&mut app, 0.5);
    let moved = particles(&mut app);
    assert!(
        moved
            .iter()
            .all(|p| p.chebyshev(origin).is_some_and(|d| d >= 1))
    );
    // 2-3 steps of 0.35 s at most.
    run(&mut app, 1.2);
    assert!(particles(&mut app).is_empty(), "the sparks fade");
}

#[test]
fn metal_foam_spreads_stops_at_walls_and_sets() {
    let mut app = server("metal_foam_spreads_stops_at_walls_and_sets");
    let open = at(10, 6);
    let by_wall = at(19, 3);
    do_foam(
        app.world_mut(),
        1,
        open,
        None,
        None,
        FoamKind::Metal,
        FoamOptions::default(),
    );
    do_foam(
        app.world_mut(),
        1,
        by_wall,
        None,
        None,
        FoamKind::Metal,
        FoamOptions::default(),
    );
    run(&mut app, 1.0);
    let tiles = foam_tiles(&mut app);
    // `DIAMOND_AREA(1)` = 5 in the open; the wall at x = 20 takes the east tile, the flood
    // grows on the other side instead.
    let near = |c: TilePos| {
        tiles
            .iter()
            .filter(|t| (t.x - c.x).abs() + (t.y - c.y).abs() <= 2)
            .count()
    };
    assert_eq!(tiles.len(), 10, "{tiles:?}");
    assert_eq!(near(open), 5);
    assert!(tiles.iter().all(|t| t.x < WALL_X), "{tiles:?}");
    // 8 s of lifetime, then foamed metal on every tile and the foam gone.
    run(&mut app, 9.0);
    assert!(foam_tiles(&mut app).is_empty());
    assert_eq!(count_objs(&mut app, "/obj/structure/foamedmetal"), 10);
}

#[test]
fn a_foam_reaction_spews_foam_and_empties_the_container() {
    let mut app = server("a_foam_reaction_spews_foam_and_empties_the_container");
    let chem = app.world().resource::<Chem>().0.clone();
    let water = chem.id("/datum/reagent/water").unwrap();
    let surfactant = chem.id("/datum/reagent/fluorosurfactant").unwrap();
    let mut holder = Holder::new(100.0, 0, Owner::default());
    holder.add_reagent(&chem, water, 5.0, 300.0, None, None, true, false);
    let beaker = app.world_mut().spawn((at(10, 6), Reagents(holder))).id();
    app.update();
    app.world_mut()
        .get_mut::<Reagents>(beaker)
        .unwrap()
        .0
        .add_reagent(&chem, surfactant, 5.0, 300.0, None, None, false, false);
    run(&mut app, 2.0);
    // `create_foam(2 * created_volume)`: 5 reactions, 10 nodes.
    assert_eq!(foam_tiles(&mut app).len(), 10);
    assert_eq!(
        app.world().get::<Reagents>(beaker).unwrap().0.total_volume,
        0.0
    );
}

#[test]
fn sleeping_smoke_puts_a_mob_to_sleep() {
    let mut app = server("sleeping_smoke_puts_a_mob_to_sleep");
    let mob = app
        .world_mut()
        .spawn((
            at(11, 6),
            Health::default(),
            Stat::Conscious,
            Inventory::default(),
        ))
        .id();
    do_smoke(app.world_mut(), 1, at(10, 6), None, SmokeKind::Sleeping);
    run(&mut app, 0.5);
    let effects = app
        .world()
        .get::<StatusEffects>(mob)
        .cloned()
        .unwrap_or_default();
    assert!(effects.remaining(Incap::Sleeping) > 15.0, "{effects:?}");
}

/// A living mob with no organs or client on `pos`.
fn mob(app: &mut App, pos: TilePos) -> Entity {
    app.world_mut()
        .spawn((pos, Health::default(), Stat::Conscious))
        .id()
}

fn burn(s: &testing::Station, mob: Entity) -> f32 {
    s.server.world().get::<Health>(mob).unwrap().burn
}

#[test]
fn the_floor_is_lava_burns_who_stands_on_the_floor() {
    let assets = fixture_assets("the_floor_is_lava_burns_who_stands_on_the_floor");
    let mut s = testing::Station::start(fixture_map(&assets, &["map.dmm"]), |app| {
        app.add_plugins(ss13_atmos::AtmosPlugin);
    });
    s.join_all(2, |_| {});
    let walker = s.players[0].mob;
    let on_table = s.players[1].mob;
    s.server.world_mut().entity_mut(on_table).insert(TABLE);
    s.run_for(0.2);
    weather::run_weather(
        s.server.world_mut(),
        WeatherKind::FloorIsLava,
        Some(vec![1]),
    );
    assert!(weather::running(s.server.world(), WeatherKind::FloorIsLava));
    // 15 s of telegraph: nothing yet.
    s.run_for(14.5);
    assert_eq!(burn(&s, walker), 0.0);
    // Then 3 burn a second to who stands on the floor, not on the table.
    s.run_for(2.0);
    assert!(burn(&s, walker) >= 3.0);
    assert_eq!(burn(&s, on_table), 0.0);
    // 30-60 s, no wind-down.
    s.run_for(61.0);
    assert!(!weather::running(
        s.server.world(),
        WeatherKind::FloorIsLava
    ));
}

#[test]
fn the_radiation_storm_spares_maintenance() {
    let mut app = server("the_radiation_storm_spares_maintenance");
    let hall = mob(&mut app, at(8, 6));
    let maint = mob(&mut app, at(23, 6));
    weather::run_weather(app.world_mut(), WeatherKind::RadStorm, None);
    let storm = app.world().resource::<ActiveWeather>().0[0].clone();
    assert!(weather::can_weather_act_mob(
        app.world(),
        &storm,
        hall,
        at(8, 6)
    ));
    assert!(!weather::can_weather_act_mob(
        app.world(),
        &storm,
        maint,
        at(23, 6)
    ));
    // Another level is not impacted.
    assert!(!weather::can_weather_act_mob(
        app.world(),
        &storm,
        hall,
        TilePos::new(8, 6, 2)
    ));
    // 40 s telegraph, 60-150 s storm, 10 s wind-down.
    run(&mut app, 201.0);
    assert!(!weather::running(app.world(), WeatherKind::RadStorm));
}
