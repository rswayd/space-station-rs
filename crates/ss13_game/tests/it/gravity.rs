//! The gravity generator and level gravity (`ss13_game::power::gravity_generator`,
//! `ss13_game::gravity`) on a room of its own without clients.
//!
//! - Mapped on, the generator spawns its eight parts, gives its level gravity and crushes the
//!   tiles within 2 of it with 6 G.
//! - A blackout drains it: the level loses gravity, so a mob on a floor away from it floats;
//!   with the breaker closed again it charges 2% a machine tick and comes back on at 100%.
//! - A devastating blast on a part breaks it: off, drained, and it needs the repair.
//! - The Gravity Generator Blackout event does the blackout on the station generator.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::gravity::{LevelGravity, field_gravity, turf_gravity};
use ss13_game::power::gravity_generator::{self, GravityGenerator, GravityGeneratorPart};
use ss13_game::testing::{self, fixture_map, steps, write_fixture};
use ss13_shared::TilePos;

const GEN_TYPES: &str = r##"
    "/obj/machinery/gravity_generator": { "parent": "/obj/machinery", "declared_vars": {
        "icon": { "resource": "g.dmi" }, "density": 1, "use_power": 0 } },
    "/obj/machinery/gravity_generator/main": { "parent": "/obj/machinery/gravity_generator",
        "declared_vars": { "icon_state": "on_8", "use_power": 1, "idle_power_usage": 0,
        "active_power_usage": 300, "power_channel": 3 } },
    "/obj/machinery/gravity_generator/part": { "parent": "/obj/machinery/gravity_generator" },
    "/area/engine": { "parent": "/area", "declared_vars": { "requires_power": 0 } },
"##;

/// The generator at (6, 3): its parts fill x 5..7, y 3..5.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/engine)
"f" = (/turf/open/floor,/area/engine)
"G" = (/obj/machinery/gravity_generator/main,/turf/open/floor,/area/engine)

(1,1,1) = {"
wwwwwwwwww
wffffffffw
wffffffffw
wffffffffw
wffffGfffw
wffffffffw
wwwwwwwwww
"}
"##;

const GEN: TilePos = TilePos::new(6, 3, 1);
/// Four tiles off: no field, the level's gravity.
const FAR: TilePos = TilePos::new(2, 6, 1);

fn assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("gravity_fixture")
        .join(test);
    let types = super::power::TYPES.replacen(
        "    \"/mob\": {",
        &format!("{GEN_TYPES}    \"/mob\": {{"),
        1,
    );
    let used = [
        "/obj/machinery/gravity_generator/main",
        "/obj/machinery/gravity_generator/part",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/area/engine",
    ];
    let mut states = vec![("t.dmi", "floor"), ("t.dmi", "wall"), ("t.dmi", "")];
    for s in [
        "on_1",
        "on_2",
        "on_3",
        "on_4",
        "on_5",
        "on_6",
        "on_7",
        "on_8",
        "on_9",
        "off_8",
        "fix0_8",
        "activated",
    ] {
        states.push(("g.dmi", s));
    }
    write_fixture(&dir, &types, &used, &states, &[("map.dmm", MAP)])
}

fn run(app: &mut App, secs: f64) {
    for _ in 0..steps(secs) {
        app.update();
    }
}

fn generator(app: &mut App) -> Entity {
    let world = app.world_mut();
    let mut q = world.query_filtered::<Entity, With<GravityGenerator>>();
    q.single(world).expect("one generator")
}

#[test]
fn the_generator_gives_its_level_gravity_until_it_goes_down() {
    let assets = assets("level");
    let mut app = testing::server_app(fixture_map(&assets, &["map.dmm"]), |_| {});
    run(&mut app, 3.0);
    let g = generator(&mut app);
    let parts: Vec<Entity> = {
        let world = app.world_mut();
        let mut q = world.query::<(Entity, &GravityGeneratorPart)>();
        q.iter(world).map(|(e, _)| e).collect()
    };
    assert_eq!(parts.len(), 8, "the 3x3 machine");
    {
        let world = app.world();
        assert!(world.get::<GravityGenerator>(g).unwrap().on);
        assert_eq!(world.resource::<LevelGravity>().level(1), Some(1.0));
        assert_eq!(turf_gravity(world, FAR), 1.0);
        assert_eq!(field_gravity(world, TilePos::new(8, 5, 1)), Some(6.0));
        assert_eq!(field_gravity(world, TilePos::new(9, 5, 1)), None);
    }
    let mob = app.world_mut().spawn(FAR).id();
    assert!(!ss13_game::spacemove::weightless_at(app.world(), mob, FAR));

    // Blackout: drained, breaker open, the level without gravity — the floor floats.
    gravity_generator::blackout(app.world_mut(), g);
    {
        let world = app.world();
        let gen_state = world.get::<GravityGenerator>(g).unwrap();
        assert!(!gen_state.on && !gen_state.breaker && gen_state.charge_count == 0);
        assert_eq!(world.resource::<LevelGravity>().level(1), Some(0.0));
        assert!(!ss13_game::multiz::has_gravity(world, FAR));
        assert_eq!(field_gravity(world, GEN), None, "the field goes with it");
        assert!(ss13_game::spacemove::weightless_at(world, mob, FAR));
    }

    // Breaker closed near the top: it charges 2% a machine tick and switches on at 100%.
    {
        let mut gen_state = app.world_mut().get_mut::<GravityGenerator>(g).unwrap();
        gen_state.breaker = true;
        gen_state.charge_count = 94;
        gen_state.set_power(true);
    }
    run(&mut app, 3.0);
    assert!(
        !app.world().get::<GravityGenerator>(g).unwrap().on,
        "still charging"
    );
    run(&mut app, 8.0);
    assert!(app.world().get::<GravityGenerator>(g).unwrap().on);
    assert!(ss13_game::multiz::has_gravity(app.world(), FAR));

    // A devastating blast on a part breaks the whole machine.
    let world = app.world_mut();
    assert!(gravity_generator::ex_act(
        world,
        parts[0],
        ss13_game::explosions::Severity::Devastate
    ));
    let gen_state = world.get::<GravityGenerator>(g).unwrap();
    assert!(gen_state.broken && !gen_state.on && gen_state.charge_count == 0);
    assert!(!ss13_game::multiz::has_gravity(world, FAR));
}

/// Gravity Generator Blackout: available with a station generator, and its start (the first
/// event tick) drains the generator and opens the breaker.
#[test]
fn gravity_blackout_event_drains_the_station_generator() {
    use ss13_game::events::{Events, force_event, gravity_blackout};
    let assets = assets("blackout_event");
    let mut app = testing::server_app(fixture_map(&assets, &["map.dmm"]), |_| {});
    run(&mut app, 3.0);
    let g = generator(&mut app);
    let world = app.world_mut();
    assert!(gravity_blackout::has_station_generator(world));
    world.resource_mut::<Events>().scheduled = f64::MAX;
    force_event(world, gravity_blackout::NAME, Some(false)).unwrap();
    for _ in 0..3 {
        ss13_game::events::controller::fire(world);
    }
    let gen_state = world.get::<GravityGenerator>(g).unwrap();
    assert!(!gen_state.on && !gen_state.breaker && gen_state.charge_count == 0);
    assert_eq!(world.resource::<LevelGravity>().level(1), Some(0.0));
}
