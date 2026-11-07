//! Space events (`ss13_game::events`) on the power fixture room (`super::power`) without
//! clients: the sandstorm's space sand, the Dark Matt-eor's red alert.

use bevy::prelude::*;
use ss13_game::events::controller::fire;
use ss13_game::events::meteors::{self, DarkMatteor, Meteor, kind_by_path, spawn_meteor_at};
use ss13_game::events::{Events, force_event};
use ss13_game::security_level::{self, SecLevel};
use ss13_game::testing::{self, fixture_map, steps};
use ss13_shared::TilePos;

fn server(test: &str) -> App {
    let assets = super::power::fixture_assets(&format!("space_events_{test}"));
    let mut app = testing::server_app(fixture_map(&assets, &["map.dmm"]), |_| {});
    app.update();
    app
}

fn run(app: &mut App, secs: f64) {
    for _ in 0..steps(secs) {
        app.update();
    }
}

/// Space sand rams the room's plain walls (`hardness` 40) and floors, but nothing in an
/// `EVENT_PROTECTED` area.
#[test]
fn space_sand_spares_protected_areas() {
    let mut app = server("sand");
    let (wall, floor) = (TilePos::new(1, 3, 1), TilePos::new(3, 3, 1));
    assert!(!meteors::sand_spares(app.world(), wall));
    assert!(!meteors::sand_spares(app.world(), floor));
    let area = app
        .world()
        .resource::<ss13_mapload::Areas>()
        .area_at(floor)
        .unwrap();
    app.world_mut()
        .get_mut::<ss13_mapload::DmPower>(area)
        .unwrap()
        .area_flags = f64::from(1u32 << 9);
    assert!(meteors::sand_spares(app.world(), floor));
    assert!(meteors::sand_spares(app.world(), wall), "same area");
}

/// A dark matt-eor raises the level to red while it flies; reaching its goal unspent it has
/// missed: the level goes back to green.
#[test]
fn dark_matteor_raises_red_alert_until_it_misses() {
    let mut app = server("dark_matteor");
    assert_eq!(security_level::current(app.world()), SecLevel::Green);
    let kind = kind_by_path("/obj/effect/meteor/dark_matteor").unwrap();
    let meteor = spawn_meteor_at(
        app.world_mut(),
        kind,
        TilePos::new(3, 3, 1),
        TilePos::new(4, 3, 1),
    )
    .unwrap();
    assert_eq!(security_level::current(app.world()), SecLevel::Red);
    assert_eq!(
        app.world().get::<DarkMatteor>(meteor).unwrap().previous,
        Some(SecLevel::Green)
    );
    run(&mut app, 1.0);
    assert!(app.world().get::<Meteor>(meteor).is_none(), "gone");
    assert_eq!(security_level::current(app.world()), SecLevel::Green);
}

/// The forced Dark Matt-eor targets the living mob on the station level; without space 13+10
/// tiles in on this small level it finds no start and spawns nothing; the event ends at once.
#[test]
fn dark_matteor_event_targets_a_mob() {
    let mut app = server("dark_matteor_event");
    let pos = TilePos::new(5, 4, 1);
    app.world_mut()
        .spawn((pos, ss13_game::health::Health::default()));
    assert_eq!(meteors::dark_matteor_target(app.world_mut()), Some(pos));
    let world = app.world_mut();
    world.resource_mut::<Events>().scheduled = f64::MAX;
    force_event(world, meteors::DARK_MATTEOR, Some(false)).unwrap();
    fire(world);
    fire(world);
    assert_eq!(world.resource::<Events>().running().count(), 0);
}

/// Sandstorm: Directional runs 110-140 ticks after its 70-90 tick warning, then ends.
#[test]
fn sandstorm_runs_and_ends() {
    let mut app = server("sandstorm");
    let world = app.world_mut();
    world.resource_mut::<Events>().scheduled = f64::MAX;
    force_event(world, meteors::SANDSTORM, Some(true)).unwrap();
    for _ in 0..100 {
        fire(world);
    }
    assert_eq!(world.resource::<Events>().running().count(), 1);
    for _ in 0..50 {
        fire(world);
    }
    assert_eq!(world.resource::<Events>().running().count(), 0);
}

/// Aurora Caelus on a bare world: the fade walks only the starlit space turf's light (not a
/// floor's lamp) to the aurora's green over five 8 s steps, lights the kitchen floor, and is
/// done after the sixth; the aurora's colors then recolor starlight.
#[test]
fn aurora_caelus_recolors_starlight() {
    use ss13_game::events::aurora_caelus::{self, AuroraFade, FROM, Starlight, TO};
    use ss13_shared::{Dir, LightSource, Optics, TileLighting};
    use std::time::Duration;

    let mut world = World::new();
    world.insert_resource(Time::<()>::default());
    let light = |color| LightSource {
        range: 2.0,
        power: 1.0,
        color,
        height: 1.0,
        angle: 360,
        dir: Dir::North,
        offset: [0, 0],
    };
    let star_color = [0x85, 0x89, 0xfa];
    let space = world
        .spawn((
            ss13_mapload::Turf,
            Optics {
                opaque: false,
                tile: Some(TileLighting::Space),
                light: Some(light(star_color)),
            },
        ))
        .id();
    let lamp = world
        .spawn((
            ss13_mapload::Turf,
            Optics {
                opaque: false,
                tile: None,
                light: Some(light([255, 255, 255])),
            },
        ))
        .id();
    let kitchen = world.spawn(ss13_mapload::Turf).id();
    let base = aurora_caelus::base_starlight(&mut world).unwrap();
    assert_eq!(
        base,
        Starlight {
            color: star_color,
            range: 2.0,
            power: 1.0
        }
    );
    world.insert_resource(AuroraFade {
        base,
        fade_in: true,
        started: 0.0,
        next: 1,
        kitchen: vec![kitchen],
    });
    let star = |world: &World| world.get::<Optics>(space).unwrap().light.unwrap();
    for _ in 0..6 {
        aurora_caelus::process_fades(&mut world);
        world
            .resource_mut::<Time>()
            .advance_by(Duration::from_secs(8));
    }
    assert!(world.get_resource::<AuroraFade>().is_none(), "done");
    assert_eq!(star(&world).color, FROM);
    assert!((star(&world).range - 3.5).abs() < 1e-5);
    let lamp_light = world.get::<Optics>(lamp).unwrap().light.unwrap();
    assert_eq!(lamp_light.color, [255, 255, 255], "not starlight");
    let lit = world.get::<Optics>(kitchen).unwrap().light.unwrap();
    assert_eq!((lit.color, lit.power), (FROM, 0.75));

    aurora_caelus::set_starlight(&mut world, TO, None, None);
    assert_eq!(star(&world).color, TO);
    assert!((star(&world).range - 3.5).abs() < 1e-5, "range kept");
}

/// Portal Storm: Syndicate Shocktroops draws its 10 troopers on every other tick from tick 8
/// and its 2 stormtroopers on ticks 17 and 22, and ends with the last draw (tick 26), long
/// before its `end_when` of 999.
#[test]
fn portal_storm_ends_when_everything_came_out() {
    let mut app = server("portal_storm");
    let world = app.world_mut();
    world.resource_mut::<Events>().scheduled = f64::MAX;
    force_event(
        world,
        ss13_game::events::portal_storm::SYNDICATE,
        Some(false),
    )
    .unwrap();
    for _ in 0..20 {
        fire(world);
    }
    assert_eq!(world.resource::<Events>().running().count(), 1);
    for _ in 0..10 {
        fire(world);
    }
    assert_eq!(world.resource::<Events>().running().count(), 0);
}
