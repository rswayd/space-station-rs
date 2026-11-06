//! Moving between Cyberiad's two decks, server side, mobs moved with `MoveRequest` (no client):
//! up the last stair of a flight, back down through the open space over it (the stairs catch
//! the fall), a fall elsewhere that hurts, a dropped item that falls, a ladder climb.
//!
//! `cargo test -p ss13_game --all-features --test it multiz::cyberiad -- --ignored --nocapture`
//! (needs generated assets in `SS13_ASSETS`).

use bevy::prelude::*;
use ss13_game::health::Health;
use ss13_game::multiz::{self, Climb, Ladder, Stairs, ZLevels};
use ss13_game::testing::{server_app, steps};
use ss13_mapload::MapLoadPlugin;
use ss13_shared::{Dir, TilePos};
use ss13_world::{MoveRequest, SpatialIndex};

/// A dog: walks, does not fly, survives one fall.
const DOG: &str = "/mob/living/basic/pet/dog/corgi";

fn cyberiad() -> MapLoadPlugin {
    MapLoadPlugin {
        maps: vec!["Cyberiad".into()],
        ..MapLoadPlugin::from_env().unwrap_or(MapLoadPlugin {
            source: env!("SS13_SOURCE").into(),
            maps: vec![],
            assets: std::env::var("SS13_ASSETS")
                .unwrap_or_else(|_| "assets/generated".into())
                .into(),
        })
    }
}

fn run(app: &mut App, secs: f64) {
    for _ in 0..steps(secs) {
        app.update();
    }
}

fn pos(app: &App, e: Entity) -> TilePos {
    *app.world().get::<TilePos>(e).unwrap()
}

fn health(app: &App, e: Entity) -> f32 {
    app.world().get::<Health>(e).unwrap().health()
}

fn free(world: &World, p: TilePos) -> bool {
    world.resource::<SpatialIndex>().dense_on_tile(p).is_empty()
}

#[test]
#[ignore = "needs generated assets and Cyberiad; run with --ignored"]
fn cyberiad_decks() {
    let mut app = server_app(cyberiad(), |app| {
        app.add_plugins(ss13_atmos::AtmosPlugin);
    });
    app.update();
    let world = app.world_mut();
    let levels = world.resource::<ZLevels>().clone();
    let links = world.resource::<ss13_atmos::Atmos>().vertical_links();
    println!("air links between the decks: {links}");
    assert!(links > 0);
    assert_eq!(
        levels.step(TilePos::new(10, 10, 1), true),
        Some(TilePos::new(10, 10, 2))
    );

    // A flight whose last stair leads onto a floor upstairs, with open space over that stair.
    let mut q = world.query::<(&Stairs, &TilePos)>();
    let stairs: Vec<(Stairs, TilePos)> = q.iter(world).map(|(s, &p)| (*s, p)).collect();
    println!(
        "stairs: {}, last of a flight: {}",
        stairs.len(),
        stairs.iter().filter(|(s, _)| s.terminator).count()
    );
    let (flight, at) = stairs
        .iter()
        .copied()
        .find(|(s, p)| {
            let Some(above) = levels.step(*p, true) else {
                return false;
            };
            let target = above.step(s.dir);
            s.terminator
                && multiz::is_openspace(world, above)
                && !multiz::is_openspace(world, target)
                && multiz::z_pass_in(world, above, true)
                && free(world, target)
                && free(world, *p)
        })
        .expect("a climbable flight");
    let above = levels.step(at, true).unwrap();
    let landing = above.step(flight.dir);
    println!(
        "climbing the stairs at {at:?} ({:?}) to {landing:?}",
        flight.dir
    );

    let dog = ss13_game::mobs::spawn::spawn_mob(app.world_mut(), DOG, at).expect("a corgi");
    run(&mut app, 0.5);
    let full = health(&app, dog);
    app.world_mut().write_message(MoveRequest {
        entity: dog,
        dir: flight.dir,
    });
    run(&mut app, 0.5);
    assert_eq!(pos(&app, dog), landing, "up the stairs");

    // Back onto the open space over the stairs: down onto the last stair, unhurt.
    app.world_mut().write_message(MoveRequest {
        entity: dog,
        dir: flight.dir.reverse(),
    });
    run(&mut app, 0.5);
    assert_eq!(
        pos(&app, dog),
        at,
        "down through the open space onto the stairs"
    );
    assert_eq!(health(&app, dog), full, "the stairs catch the fall");

    // Open space over a plain floor: a corgi spawned there falls and is hurt.
    let world = app.world_mut();
    let mut turfs = world.query_filtered::<&TilePos, With<ss13_mapload::Turf>>();
    let all: Vec<TilePos> = turfs.iter(world).copied().collect();
    let drop = all
        .into_iter()
        .filter(|&p| {
            p.z == 2
                && multiz::is_openspace(world, p)
                && multiz::z_pass_out(world, p, false)
                && free(world, p)
        })
        .find(|&p| {
            let Some(below) = levels.step(p, false) else {
                return false;
            };
            multiz::z_pass_in(world, below, false)
                && !multiz::is_openspace(world, below)
                && free(world, below)
                && !stairs.iter().any(|(_, s)| *s == below)
                && multiz::has_gravity(world, p)
        })
        .expect("open space over a floor");
    let below = levels.step(drop, false).unwrap();
    let faller = ss13_game::mobs::spawn::spawn_mob(app.world_mut(), DOG, drop).unwrap();
    // A wandering dog can step off the landing tile before it is checked.
    app.world_mut()
        .entity_mut(faller)
        .remove::<ss13_game::mobs::ai::AiController>();
    run(&mut app, 0.5);
    let fell = health(&app, faller);
    println!(
        "a corgi dropped at {drop:?}: at {:?}, health {full} -> {fell}",
        pos(&app, faller)
    );
    assert_eq!(pos(&app, faller), below);
    // `(5 * 1) ^ 1.5` = 11.18, standing: `round(11.18 / 2)` to each leg.
    assert_eq!(full - fell, 12.0);

    // A dropped item falls too.
    let item = ss13_game::construction::spawn_obj(app.world_mut(), "/obj/item/crowbar", drop)
        .expect("a crowbar");
    run(&mut app, 0.5);
    assert_eq!(pos(&app, item), below, "the crowbar falls");

    // A ladder going up: a click and a second later the climber is on the one above.
    let world = app.world_mut();
    let mut ladders = world.query::<(Entity, &Ladder, &TilePos)>();
    let all: Vec<(Entity, Ladder, TilePos)> =
        ladders.iter(world).map(|(e, l, &p)| (e, *l, p)).collect();
    println!(
        "ladders: {}, linked: {}",
        all.len(),
        all.iter()
            .filter(|(_, l, _)| l.up.is_some() || l.down.is_some())
            .count()
    );
    let (ladder, link, at) = all
        .into_iter()
        .find(|(_, l, p)| l.up.is_some() && free(world, *p))
        .expect("a ladder going up");
    let top = *world.get::<TilePos>(link.up.unwrap()).unwrap();
    let climber = ss13_game::mobs::spawn::spawn_mob(app.world_mut(), DOG, at).unwrap();
    // A wandering dog would walk off the ladder mid-climb.
    app.world_mut()
        .entity_mut(climber)
        .remove::<ss13_game::mobs::ai::AiController>();
    run(&mut app, 0.2);
    assert!(multiz::interact(
        app.world_mut(),
        Entity::PLACEHOLDER,
        climber,
        None,
        ladder,
        false
    ));
    run(&mut app, 0.5);
    assert_eq!(pos(&app, climber), at, "still climbing");
    run(&mut app, 0.7);
    assert_eq!(pos(&app, climber), top, "up the ladder");
    multiz::complete_climb(
        app.world_mut(),
        climber,
        link.up.unwrap(),
        Climb { up: false },
    );
    assert_eq!(pos(&app, climber), at, "and down again");
    let _ = Dir::North;
}

/// What a player upstairs is sent of the deck below: only what lies under open space and glass
/// floors (tg's z-mimic), with the level stack to draw it.
#[test]
#[ignore = "needs generated assets and Cyberiad; run with --ignored"]
fn cyberiad_view_down() {
    use ss13_game::testing::{Drive, Station};
    use ss13_net::loopback;
    let mut s = Station::start(cyberiad(), |app| {
        let (listener, connector) = loopback::listener();
        app.insert_resource(listener).insert_resource(connector);
    });
    let p = s.join("p0", |_| {});
    let mob = s.players[p].mob;
    // A floor upstairs with open space over a furnished room in view.
    let world = s.server.world_mut();
    let levels = world.resource::<ZLevels>().clone();
    let mut turfs = world.query_filtered::<&TilePos, With<ss13_mapload::Turf>>();
    let all: Vec<TilePos> = turfs.iter(world).copied().collect();
    let index = world.resource::<SpatialIndex>();
    let floor_next_to = |w: TilePos| {
        all.iter().copied().find(|&q| {
            q.z == 2
                && !index.transparent(q)
                && index.dense_on_tile(q).is_empty()
                && q.chebyshev(w) == Some(1)
                && !multiz::is_openspace(world, q)
        })
    };
    let (window, stand) = all
        .iter()
        .copied()
        .filter(|&p| p.z == 2 && index.transparent(p))
        .filter_map(|w| Some((w, floor_next_to(w)?)))
        .max_by_key(|&(w, _)| levels.step(w, false).map_or(0, |b| index.on_tile(b).len()))
        .expect("a transparent tile upstairs beside a floor");
    println!("standing at {stand:?}, looking down through {window:?}");
    world.entity_mut(mob).insert(stand);
    s.run_for(2.0);

    let client = s.players[p].app.world_mut();
    let stack = client
        .get_resource::<ss13_shared::LevelStack>()
        .cloned()
        .expect("the level stack");
    assert_eq!(stack.below(2), Some(1));
    let mut q = client.query::<&TilePos>();
    let all_client: Vec<TilePos> = q.iter(client).copied().collect();
    let mut by_z = std::collections::BTreeMap::new();
    for p in &all_client {
        *by_z.entry(p.z).or_insert(0) += 1;
    }
    println!("client entities by z: {by_z:?}");
    let below: Vec<TilePos> = all_client.iter().copied().filter(|p| p.z == 1).collect();
    let index = s.server.world().resource::<SpatialIndex>();
    let wrong: Vec<&TilePos> = below
        .iter()
        .filter(|p| !index.transparent(TilePos { z: 2, ..**p }))
        .collect();
    println!(
        "{} entities below, {} of them not under a transparent tile",
        below.len(),
        wrong.len()
    );
    assert!(below.iter().any(|b| b.x == window.x && b.y == window.y));
    assert!(
        wrong.is_empty(),
        "only under open space and glass: {wrong:?}"
    );
}

/// One key press on the last stair: the player lands on the floor beyond the open space
/// upstairs, once (no extra step), on the server and on its own client alike.
#[test]
#[ignore = "needs generated assets and Cyberiad; run with --ignored"]
fn cyberiad_one_step_up() {
    use ss13_game::testing::{Drive, Station};
    use ss13_net::loopback;
    let mut s = Station::start(cyberiad(), |app| {
        let (listener, connector) = loopback::listener();
        app.insert_resource(listener).insert_resource(connector);
    });
    let p = s.join("p0", |_| {});
    let mob = s.players[p].mob;
    // The library's wooden flight (south, up).
    let world = s.server.world_mut();
    let levels = world.resource::<ZLevels>().clone();
    let mut q = world.query::<(&Stairs, &TilePos)>();
    let (flight, at) = q
        .iter(world)
        .map(|(s, &p)| (*s, p))
        .find(|(s, p)| s.terminator && *p == TilePos::new(174, 139, 1))
        .expect("the library flight");
    let landing = levels.step(at, true).unwrap().step(flight.dir);
    world.entity_mut(mob).insert(at);
    s.run_for(1.0);
    let seq_before = s.players[p].seq;
    s.try_step(p, flight.dir);
    assert_eq!(s.players[p].seq, seq_before + 1);
    s.run_for(2.0);
    let server = *s.get::<TilePos>(mob);
    let copy = s.client_copy(p, mob).expect("own mob");
    let client = *s.players[p].app.world().get::<TilePos>(copy).unwrap();
    println!(
        "stair {at:?} {:?}: landing {landing:?}, server {server:?}, client {client:?}",
        flight.dir
    );
    assert_eq!(server, landing, "one step up, no more");
    assert_eq!(client, landing, "the client agrees");
}

/// Walking up and down the library's flight with a held key, in real time with 50 ms of
/// latency each way, as a player does: every tile change of the server, the ack and the
/// prediction is logged with its time, and steps may not come faster than the walk delay.
#[test]
#[ignore = "needs generated assets and Cyberiad; real time; run with --ignored --nocapture"]
fn cyberiad_stairs_walk() {
    use bevy::time::TimeUpdateStrategy;
    use ss13_game::prediction::{MoveInput, Prediction, PredictionPlugin};
    use ss13_game::testing::Station;
    use ss13_net::{SimLatency, loopback};
    use std::time::{Duration, Instant};
    let real_time = |app: &mut App| {
        app.insert_resource(TimeUpdateStrategy::Automatic)
            .insert_resource(SimLatency(Duration::from_millis(50)));
    };
    let mut st = Station::start(cyberiad(), |app| {
        let (listener, connector) = loopback::listener();
        app.insert_resource(listener).insert_resource(connector);
        real_time(app);
    });
    let p = st.join("a", |client| {
        real_time(client);
        client.add_plugins(PredictionPlugin);
    });
    let mob = st.players[p].mob;
    // Three tiles before the last stair (174,139), on the flight going south (up).
    st.server
        .world_mut()
        .entity_mut(mob)
        .insert(TilePos::new(174, 140, 1));
    let start = Instant::now();
    let mut log: Vec<(f64, TilePos, TilePos, TilePos)> = Vec::new();
    let mut own = Vec::new();
    let mut frame = |st: &mut Station| {
        st.tick();
        let world = st.players[p].app.world();
        let controlled = world
            .resource::<ss13_net::client::NetEntityMap>()
            .controlled();
        if own.last() != Some(&controlled) {
            own.push(controlled);
        }
        let pred = world.resource::<Prediction>();
        let (Some(predicted), Some(acked)) = (pred.predicted(), pred.authoritative()) else {
            return;
        };
        let server = *st.get::<TilePos>(mob);
        let t = start.elapsed().as_secs_f64();
        if log
            .last()
            .is_none_or(|l| (l.1, l.2, l.3) != (predicted, acked.pos, server))
        {
            log.push((t, predicted, acked.pos, server));
        }
    };
    let hold = |st: &mut Station, dir: Option<Dir>| {
        st.players[p].app.world_mut().resource_mut::<MoveInput>().0 = dir;
    };
    for _ in 0..30 {
        frame(&mut st);
    }
    hold(&mut st, Some(Dir::South));
    let until = Instant::now() + Duration::from_secs_f64(1.6);
    while Instant::now() < until {
        frame(&mut st);
    }
    hold(&mut st, None);
    for _ in 0..30 {
        frame(&mut st);
    }
    hold(&mut st, Some(Dir::North));
    let until = Instant::now() + Duration::from_secs_f64(1.6);
    while Instant::now() < until {
        frame(&mut st);
    }
    hold(&mut st, None);
    for _ in 0..30 {
        frame(&mut st);
    }
    // The mob stays the same entity on the client across levels: one despawned and spawned
    // anew is drawn popping onto its tile, not stepping.
    let own: Vec<_> = own.into_iter().flatten().collect();
    assert_eq!(own.len(), 1, "the client respawned its own mob: {own:?}");
    for (t, predicted, acked, server) in &log {
        println!(
            "{t:6.3}  predicted {:>3},{:>3},{}  acked {:>3},{:>3},{}  server {:>3},{:>3},{}",
            predicted.x,
            predicted.y,
            predicted.z,
            acked.x,
            acked.y,
            acked.z,
            server.x,
            server.y,
            server.z
        );
    }
    // The drawn position moves a tile at a time, also across the climb and the fall.
    // (The first change is the test putting the mob in place.)
    for w in log.windows(2).skip(1) {
        let (a, b) = (w[0].1, w[1].1);
        let jump = (a.x - b.x).abs().max((a.y - b.y).abs());
        assert!(
            jump <= 1,
            "the prediction jumped {a:?} -> {b:?} at {:.3}",
            w[1].0
        );
    }
    // Server tile changes: no two closer than most of a step.
    let mut changes: Vec<(f64, TilePos)> = Vec::new();
    for &(t, _, _, s) in &log {
        if changes.last().is_none_or(|c| c.1 != s) {
            changes.push((t, s));
        }
    }
    for w in changes.windows(2) {
        println!(
            "server {:?} -> {:?} after {:.3} s",
            w[0].1,
            w[1].1,
            w[1].0 - w[0].0
        );
    }
}
