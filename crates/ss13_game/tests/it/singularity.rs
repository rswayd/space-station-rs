//! Singularities and tesla balls (`ss13_game::power::{singularity, tesla}`) on a 21x21 floor
//! room, without clients.
//!
//! - A stage-two singularity inside a ring of containment fields 3 tiles out stays within one
//!   tile of the centre and leaves the fields alone; without the ring it wanders off.
//! - Dark matter chases the nearest living mob.
//! - A singularity delamination leaves a supermatter-charged singularity of 900 energy that
//!   grows to stage three; a tesla delamination a reworked ball of 1500 MJ that decays and
//!   takes emitter hits.
//! - A bolt goes to a grounding rod before a mob and dies there; without the rod the mob burns.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::health::{Health, Stat};
use ss13_game::obj_damage::Anchored;
use ss13_game::power::accumulator::{Accumulator, AccumulatorKind};
use ss13_game::power::field_generator::ContainmentField;
use ss13_game::power::singularity::{self, STAGE_THREE, STAGE_TWO, SingKind, Singularity};
use ss13_game::power::tesla::{self, BallKind, TeslaBall};
use ss13_game::testing::{self, fixture_map, steps, write_fixture};
use ss13_shared::TilePos;

const CENTER: TilePos = TilePos::new(11, 11, 1);

const EXTRA_TYPES: &str = r##"
    "/obj/singularity": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "sing.dmi" }, "icon_state": "singularity_s1", "density": 1,
        "anchored": 1 } },
    "/obj/tesla_ball": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "eb.dmi" }, "icon_state": "energy_ball", "density": 1,
        "anchored": 1 } },
"##;

fn map() -> String {
    let mut rows = Vec::new();
    for y in 0..21 {
        let row: String = (0..21)
            .map(|x| {
                if x == 0 || y == 0 || x == 20 || y == 20 {
                    'w'
                } else if (x, y) == (1, 1) {
                    's'
                } else {
                    'f'
                }
            })
            .collect();
        rows.push(row);
    }
    format!(
        "\"w\" = (/turf/closed/wall,/area/station)\n\
         \"f\" = (/turf/open/floor,/area/station)\n\
         \"s\" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/station)\n\n\
         (1,1,1) = {{\"\n{}\n\"}}\n",
        rows.join("\n")
    )
}

fn assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("singularity_fixture")
        .join(test);
    let types = super::power::TYPES.replacen(
        "    \"/mob\": {",
        &format!("{EXTRA_TYPES}    \"/mob\": {{"),
        1,
    );
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/singularity",
        "/obj/tesla_ball",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
        "/area/station",
    ];
    let states = [
        ("t.dmi", "floor"),
        ("t.dmi", "wall"),
        ("t.dmi", ""),
        ("l.dmi", "x2"),
        ("l.dmi", "Assistant"),
        ("h.dmi", "human_basic"),
        ("sing.dmi", "singularity_s1"),
        ("eb.dmi", "energy_ball"),
    ];
    let map = map();
    write_fixture(&dir, &types, &used, &states, &[("map.dmm", &map)])
}

fn app(test: &str) -> App {
    let assets = assets(test);
    let mut app = testing::server_app(fixture_map(&assets, &["map.dmm"]), |_| {});
    app.update();
    app
}

fn run(app: &mut App, secs: f64) {
    for _ in 0..steps(secs) {
        app.update();
    }
}

fn now(app: &App) -> f64 {
    app.world().resource::<Time>().elapsed_secs_f64()
}

fn spawn_singularity(app: &mut App, kind: SingKind, energy: f64, pos: TilePos) -> Entity {
    let t = now(app);
    let mut s = Singularity::new(kind, t);
    s.energy = energy;
    app.world_mut().spawn((pos, s)).id()
}

fn pos_of(app: &App, e: Entity) -> TilePos {
    *app.world().get::<TilePos>(e).expect("still there")
}

#[test]
fn a_field_ring_holds_a_stage_two_singularity() {
    let mut app = app("ring");
    let mut ring = Vec::new();
    for dy in -3..=3i32 {
        for dx in -3..=3i32 {
            if dx.abs().max(dy.abs()) != 3 {
                continue;
            }
            let field = ContainmentField {
                gens: [Entity::PLACEHOLDER; 2],
                shocked_until: 0.0,
            };
            let at = TilePos::new(CENTER.x + dx, CENTER.y + dy, 1);
            ring.push(app.world_mut().spawn((at, field)).id());
        }
    }
    let sing = spawn_singularity(&mut app, SingKind::Gravity, 300.0, CENTER);
    for _ in 0..steps(15.0) {
        app.update();
        let at = pos_of(&app, sing);
        assert!(
            at.chebyshev(CENTER).unwrap() <= 1,
            "the ring holds it: {at:?}"
        );
    }
    let s = app.world().get::<Singularity>(sing).unwrap();
    assert_eq!(s.current_size, STAGE_TWO);
    assert!(s.roaming);
    assert!(ring.iter().all(|&f| app.world().get_entity(f).is_ok()));

    // The ring gone, it wanders off.
    for f in ring {
        app.world_mut().despawn(f);
    }
    let mut farthest = 0;
    for _ in 0..steps(30.0) {
        app.update();
        let Some(&at) = app.world().get::<TilePos>(sing) else {
            break;
        };
        farthest = farthest.max(at.chebyshev(CENTER).unwrap());
    }
    assert!(farthest >= 2, "loose, it moves on: {farthest}");
}

#[test]
fn dark_matter_hunts_the_nearest_mob() {
    let mut app = app("dark");
    let start = TilePos::new(3, 11, 1);
    let mob = app
        .world_mut()
        .spawn((TilePos::new(17, 11, 1), Health::default(), Stat::Conscious))
        .id();
    let sing = spawn_singularity(&mut app, SingKind::DarkMatter, 250.0, start);
    run(&mut app, 5.5);
    let s = app.world().get::<Singularity>(sing).unwrap();
    assert_eq!(s.current_size, STAGE_TWO);
    assert_eq!(s.target, Some(mob));
    let at = pos_of(&app, sing);
    assert!(at.x >= start.x + 3, "it closes in: {at:?}");
    assert_eq!(at.y, start.y, "straight at it");
}

#[test]
fn delaminations_leave_a_singularity_or_a_tesla_ball() {
    let mut app = app("delam");
    let sing = singularity::effect_singulo(app.world_mut(), CENTER).expect("a prototype");
    {
        let s = app.world().get::<Singularity>(sing).unwrap();
        assert_eq!(s.energy, 900.0);
        assert!(s.consumed_supermatter);
    }
    run(&mut app, 2.5);
    let s = app.world().get::<Singularity>(sing).unwrap();
    assert_eq!(s.current_size, STAGE_THREE);
    assert_eq!((s.grav_pull, s.consume_range), (8, 2));
    app.world_mut().despawn(sing);

    let ball = tesla::effect_tesla(app.world_mut(), TilePos::new(5, 5, 1)).expect("a prototype");
    {
        let b = app.world().get::<TeslaBall>(ball).unwrap();
        assert_eq!(
            (b.kind, b.energy),
            (BallKind::Reworked, tesla::DEFAULT_ENERGY)
        );
    }
    run(&mut app, 2.5);
    let b = app.world().get::<TeslaBall>(ball).unwrap().clone();
    // One tick of 4% decay; no plasma, no zaps.
    assert!(
        (b.energy - tesla::DEFAULT_ENERGY * 0.96).abs() < 1.0,
        "{}",
        b.energy
    );
    assert!(
        !b.pending_minis.is_empty() || b.miniballs > 0,
        "mini-balls coming"
    );
    assert!(tesla::emitter_hit(app.world_mut(), ball));
    let after = app.world().get::<TeslaBall>(ball).unwrap().energy;
    assert!((after - b.energy - b.hit_heal).abs() < 1.0);
}

#[test]
fn bolts_go_to_rods_before_mobs() {
    let mut app = app("zap");
    let source = app.world_mut().spawn(TilePos::new(5, 5, 1)).id();
    let rod = app
        .world_mut()
        .spawn((
            TilePos::new(7, 5, 1),
            Accumulator::new(AccumulatorKind::GroundingRod),
            Anchored(true),
        ))
        .id();
    let mob = app
        .world_mut()
        .spawn((TilePos::new(6, 5, 1), Health::default(), Stat::Conscious))
        .id();
    let world = app.world_mut();
    let mut shocked = Vec::new();
    tesla::tesla_zap(
        world,
        source,
        TilePos::new(5, 5, 1),
        3,
        1e7,
        tesla::ZAP_CUTOFF,
        tesla::ZAP_TESLA_FLAGS,
        &mut shocked,
        0,
    );
    assert_eq!(world.get::<Accumulator>(rod).unwrap().stored_energy, 1e7);
    assert_eq!(
        world.get::<Health>(mob).unwrap().burn,
        0.0,
        "the rod took it all"
    );

    world.despawn(rod);
    let mut shocked = Vec::new();
    tesla::tesla_zap(
        world,
        source,
        TilePos::new(5, 5, 1),
        3,
        1e7,
        tesla::ZAP_CUTOFF,
        tesla::ZAP_TESLA_FLAGS,
        &mut shocked,
        0,
    );
    // `min(round(1e7 / 600), 90) + rand(-5, 5)`.
    let burn = world.get::<Health>(mob).unwrap().burn;
    assert!((85.0..=95.0).contains(&burn), "{burn}");
    assert!(shocked.contains(&mob));
}
