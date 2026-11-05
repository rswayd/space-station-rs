//! Random events of the second wave (`ss13_game::events`) on the power fixture room
//! (`super::power`) without clients: Grey Tide, Camera Failure, Bureaucratic Error, the
//! Immovable Rod, Wormholes, the Ventilation Clog's vent pick and the anomalies.

use bevy::prelude::*;
use ss13_game::doors::{Door, DoorState};
use ss13_game::events::anomalies::{Anomaly, AnomalyKind, spawn_anomaly};
use ss13_game::events::controller::{EventRng, fire};
use ss13_game::events::wormholes::{Wormhole, spawn_wormhole, teleport};
use ss13_game::events::{Events, force_event};
use ss13_game::health::{Health, Stat};
use ss13_game::jobs::Jobs;
use ss13_game::power::model::{CHANNEL_AUTO_ON, CHANNEL_OFF};
use ss13_game::power::{Apc, Channel};
use ss13_game::silicon::cameras::Camera;
use ss13_game::status_effects::{Incap, StatusEffects};
use ss13_game::testing::{self, fixture_map, steps};
use ss13_mapload::DmItem;
use ss13_mapload::jobs::{Job, JobsData};
use ss13_mapload::spawner::Rng;
use ss13_shared::TilePos;
use ss13_world::SpatialIndex;

const APC: TilePos = TilePos::new(7, 5, 1);
const DOOR: TilePos = TilePos::new(5, 3, 1);

fn server(test: &str) -> App {
    let assets = super::power::fixture_assets(&format!("events2_{test}"));
    let mut app = testing::server_app(fixture_map(&assets, &["map.dmm"]), |_| {});
    app.update();
    app
}

fn run(app: &mut App, secs: f64) {
    for _ in 0..steps(secs) {
        app.update();
    }
}

fn find<T: Component>(app: &mut App, pos: TilePos) -> Entity {
    let world = app.world();
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .find(|&e| world.get::<T>(e).is_some())
        .unwrap()
}

fn mob(app: &mut App, pos: TilePos) -> Entity {
    app.world_mut()
        .spawn((pos, Health::default(), Stat::Conscious))
        .id()
}

/// Grey Tide's end (`COMSIG_GLOB_GREY_TIDE`): nothing outside the struck departments; in them
/// the powered airlock opens and is bolted, the APC turns its lighting off.
#[test]
fn grey_tide_opens_and_bolts_airlocks_and_darkens_lights() {
    let mut app = server("grey_tide");
    run(&mut app, 3.0);
    let (door, apc) = (find::<Door>(&mut app, DOOR), find::<Apc>(&mut app, APC));
    let light = Channel::Light as usize;
    assert_eq!(
        app.world().get::<Apc>(apc).unwrap().channels[light],
        CHANNEL_AUTO_ON
    );

    ss13_game::events::grey_tide::grey_tide(app.world_mut(), &["/area/station/command"]);
    let d = app.world().get::<Door>(door).unwrap();
    assert!(
        !d.locked && d.state == DoorState::Closed,
        "another department"
    );

    ss13_game::events::grey_tide::grey_tide(app.world_mut(), &["/area/station"]);
    let d = app.world().get::<Door>(door).unwrap();
    assert!(d.locked, "bolted");
    assert!(
        matches!(d.state, DoorState::Opening { .. } | DoorState::Open),
        "{:?}",
        d.state
    );
    assert_eq!(
        app.world().get::<Apc>(apc).unwrap().channels[light],
        CHANNEL_OFF
    );
    run(&mut app, 2.0);
    let d = app.world().get::<Door>(door).unwrap();
    assert_eq!(d.state, DoorState::Open, "bolted open: it stays open");
}

/// Camera Failure: the first pick always goes dark; a camera off the station network never
/// does (picking it costs no iteration, so the station camera is still reached).
#[test]
fn camera_failure_turns_off_station_cameras() {
    let mut app = server("cameras");
    for seed in 0..20 {
        let world = app.world_mut();
        let mut camera = |network: &str, x| {
            world
                .spawn((
                    TilePos::new(x, 2, 1),
                    Camera {
                        c_tag: network.into(),
                        network: vec![network.into()],
                        view_range: 7,
                        enabled: true,
                    },
                ))
                .id()
        };
        let station = camera("ss13", 3);
        let other = camera("minisat", 4);
        world.insert_resource(EventRng(Rng::new(seed)));
        let off = ss13_game::events::camera_failure::fail_cameras(world);
        assert_eq!(off, vec![station], "seed {seed}");
        assert!(!world.get::<Camera>(station).unwrap().enabled);
        assert!(world.get::<Camera>(other).unwrap().enabled);
        world.despawn(station);
        world.despawn(other);
    }
}

/// Bureaucratic Error: the overflow role and the AI never change; otherwise either one job is
/// the only one open (unlimited) or each moves by -2..+4 slots, not below 0. Both outcomes
/// come up over 100 rolls.
#[test]
fn bureaucratic_error_shuffles_slots() {
    let job = |path: &str, title: &str, total| Job {
        path: path.into(),
        title: title.into(),
        total_positions: total,
        spawn_positions: total,
        ..default()
    };
    let data = JobsData {
        jobs: vec![
            job("/datum/job/assistant", "Assistant", -1),
            job("/datum/job/ai", "AI", 1),
            job("/datum/job/station_engineer", "Station Engineer", 5),
            job("/datum/job/janitor", "Janitor", 1),
            job("/datum/job/chemist", "Chemist", 0),
        ],
        ..default()
    };
    let (mut overflow, mut shuffled) = (0, 0);
    for seed in 0..100 {
        let mut jobs = Jobs::new(data.clone(), 1);
        let mut rng = EventRng(Rng::new(seed));
        ss13_game::events::bureaucratic_error::shuffle_slots(&mut jobs, &mut rng);
        let totals: Vec<i32> = jobs.data.jobs.iter().map(|j| j.total_positions).collect();
        assert_eq!(totals[0], -1, "the overflow role");
        assert_eq!(totals[1], 1, "the AI");
        let rest = &totals[2..];
        if rest.contains(&-1) {
            overflow += 1;
            assert_eq!(rest.iter().filter(|&&t| t == -1).count(), 1, "{rest:?}");
            assert!(rest.iter().all(|&t| t == -1 || t == 0), "{rest:?}");
        } else {
            shuffled += 1;
            for (&after, before) in rest.iter().zip([5, 1, 0]) {
                assert!(
                    after >= (before - 2).max(0) && after <= before + 4,
                    "{before} -> {after}"
                );
            }
        }
    }
    assert!(overflow > 10 && shuffled > 40, "{overflow} / {shuffled}");
}

/// An immovable rod through the room's middle row: it smashes the airlock in its way,
/// penetrates the mob on its line (at least the 100 brute over the body) and is gone at its
/// destination.
#[test]
fn immovable_rod_goes_through_everything() {
    let mut app = server("rod");
    let door = find::<Door>(&mut app, DOOR);
    let victim = mob(&mut app, TilePos::new(8, 3, 1));
    let rod = ss13_game::events::immovable_rod::spawn_rod(
        app.world_mut(),
        TilePos::new(1, 3, 1),
        TilePos::new(12, 3, 1),
    );
    run(&mut app, 3.0);
    let world = app.world();
    assert!(world.get_entity(rod).is_err(), "gone at its destination");
    assert!(
        world.get::<Door>(door).is_none(),
        "the airlock is destroyed"
    );
    let brute = world.get::<Health>(victim).unwrap().brute;
    assert!(brute >= 100.0, "penetrated: {brute} brute");
}

/// A bump into a wormhole lands next to one of the wormholes on a free tile; the event puts
/// 400 of them on the room's floors and removes them all when it ends.
#[test]
fn wormholes_teleport_and_vanish() {
    let mut app = server("wormholes");
    let holes = [TilePos::new(9, 2, 1), TilePos::new(10, 4, 1)];
    for pos in holes {
        spawn_wormhole(app.world_mut(), pos);
    }
    let walker = mob(&mut app, TilePos::new(3, 2, 1));
    assert!(teleport(app.world_mut(), walker));
    let at = *app.world().get::<TilePos>(walker).unwrap();
    assert!(
        holes
            .iter()
            .any(|h| (h.x - at.x).abs() <= 1 && (h.y - at.y).abs() <= 1),
        "{at:?}"
    );
    assert!(!holes.contains(&at), "not onto a (dense) wormhole");
    let mut q = app.world_mut().query_filtered::<Entity, With<Wormhole>>();
    let placed: Vec<Entity> = q.iter(app.world()).collect();
    for hole in placed {
        app.world_mut().despawn(hole);
    }

    let world = app.world_mut();
    world.resource_mut::<Events>().scheduled = f64::MAX;
    force_event(world, ss13_game::events::wormholes::NAME, Some(false)).unwrap();
    let mut most = 0;
    for _ in 0..90 {
        fire(world);
        let mut q = world.query_filtered::<Entity, With<Wormhole>>();
        most = most.max(q.iter(world).count());
    }
    assert_eq!(most, 400);
    let mut q = world.query_filtered::<Entity, With<Wormhole>>();
    assert_eq!(q.iter(world).count(), 0, "closed when the event ends");
    assert_eq!(world.resource::<Events>().running().count(), 0);
}

/// The Ventilation Clog needs an unwelded station vent: with one the event runs, with it
/// welded the event dies in `setup`.
#[test]
fn vent_clog_needs_an_open_vent() {
    use ss13_game::atmos_machinery::VentPump;
    use ss13_game::events::vent_clog::MINOR;
    let mut app = server("vent_clog");
    let world = app.world_mut();
    world.resource_mut::<Events>().scheduled = f64::MAX;
    let vent = world
        .spawn((
            TilePos::new(9, 2, 1),
            VentPump {
                on: true,
                releasing: true,
                pressure_checks: 1,
                external_pressure_bound: 101.325,
                internal_pressure_bound: 0.0,
                welded: false,
                area: None,
                overclocked: false,
            },
        ))
        .id();
    fn clogs(world: &World) -> usize {
        world
            .resource::<Events>()
            .running()
            .filter(|&n| n == MINOR)
            .count()
    }
    force_event(world, MINOR, Some(false)).unwrap();
    assert_eq!(clogs(world), 1);
    world.get_mut::<VentPump>(vent).unwrap().welded = true;
    force_event(world, MINOR, Some(false)).unwrap();
    assert_eq!(clogs(world), 1, "no open vent: killed");
}

/// A gravitational anomaly (held in place, `move_chance = 0`) knocks down the mob standing on
/// it and pulls a loose item a step closer; a flux anomaly past its lifespan detonates and is
/// gone.
#[test]
fn anomalies_pull_knock_down_and_detonate() {
    let mut app = server("anomalies");
    let at = TilePos::new(8, 2, 1);
    let world = app.world_mut();
    let grav = spawn_anomaly(world, AnomalyKind::Grav, at, None);
    world.get_mut::<Anomaly>(grav).unwrap().move_chance = 0.0;
    let item = world
        .spawn((
            TilePos::new(11, 2, 1),
            DmItem {
                path: "/obj/item/test".into(),
                ..default()
            },
        ))
        .id();
    let victim = mob(&mut app, at);
    let flux = spawn_anomaly(
        app.world_mut(),
        AnomalyKind::Flux,
        TilePos::new(3, 4, 1),
        None,
    );
    app.world_mut().get_mut::<Anomaly>(flux).unwrap().death_time = 0.0;

    run(&mut app, 2.5);
    let world = app.world();
    let knocked = world
        .get::<StatusEffects>(victim)
        .map_or(0.0, |e| e.remaining(Incap::Knockdown));
    assert!(knocked > 0.0, "knocked down");
    assert_eq!(
        *world.get::<TilePos>(item).unwrap(),
        TilePos::new(10, 2, 1),
        "pulled a step"
    );
    assert!(world.get_entity(flux).is_err(), "detonated");
    assert!(world.get_entity(grav).is_ok());
}

/// A signal on another code leaves an anomaly alone; its core's frequency and code neutralize
/// it and count a core of its type for the round (`anomalyNeutralize`); a decoy has no core.
#[test]
fn signaller_neutralizes_anomaly() {
    use ss13_game::events::anomalies::{CoresMade, signal_received};
    let mut app = server("neutralize");
    let world = app.world_mut();
    let grav = spawn_anomaly(world, AnomalyKind::Grav, TilePos::new(8, 2, 1), None);
    world.get_mut::<Anomaly>(grav).unwrap().move_chance = 0.0;
    let core = world.get::<Anomaly>(grav).unwrap().core.expect("a core");
    assert_eq!(core.frequency % 2, 1, "odd free frequency");
    assert!((1201..=1599).contains(&core.frequency));
    assert!((1..=100).contains(&core.code));

    signal_received(world, core.frequency, core.code % 100 + 1, None);
    assert!(world.get_entity(grav).is_ok(), "wrong code");
    // Out of a low-range signaller's reach.
    signal_received(
        world,
        core.frequency,
        core.code,
        Some((TilePos::new(30, 30, 1), 5)),
    );
    assert!(world.get_entity(grav).is_ok(), "out of range");
    signal_received(world, core.frequency, core.code, None);
    assert!(world.get_entity(grav).is_err(), "neutralized");
    assert_eq!(
        world.resource::<CoresMade>().0.get(&AnomalyKind::Grav),
        Some(&1)
    );
}
