//! Timed scale check, not a correctness test. Run in release:
//! `cargo test --release -p ss13_world --test bench -- --ignored --nocapture`

use std::hint::black_box;
use std::time::{Duration, Instant};

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use ss13_shared::{Dir, TilePos};
use ss13_world::{Density, MoveRequest, Mover, SpatialIndex, WorldPlugin};

const SIZE: u32 = 255;
const STATIC: usize = 200_000;
const MOVERS: usize = 1_000;
const VIEWERS: usize = 500;
const VIEW_RADIUS: u32 = 9;
const TICKS: usize = 200;

/// xorshift64: deterministic and dependency-free.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn pos(&mut self) -> TilePos {
        let x = (self.next() % u64::from(SIZE)) as i32 + 1;
        let y = (self.next() % u64::from(SIZE)) as i32 + 1;
        TilePos::new(x, y, 1)
    }
}

fn percentile(sorted: &[Duration], p: f64) -> Duration {
    sorted[((sorted.len() - 1) as f64 * p).round() as usize]
}

#[test]
#[ignore = "timing run; use --release --ignored"]
fn bench_world_core() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, WorldPlugin))
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            50,
        )));
    app.world_mut()
        .resource_mut::<SpatialIndex>()
        .create_level(1, SIZE, SIZE)
        .unwrap();

    // Static: ~3 per tile on one z (worst case for view queries); 5% dense, like walls/machines.
    let start = Instant::now();
    let statics: Vec<_> = (0..STATIC)
        .map(|i| {
            let pos = rng.pos();
            if i % 20 == 0 {
                app.world_mut().spawn((pos, Density)).id()
            } else {
                app.world_mut().spawn(pos).id()
            }
        })
        .collect();
    let spawn = start.elapsed();
    black_box(&statics);

    let movers: Vec<_> = (0..MOVERS)
        .map(|_| {
            let pos = rng.pos();
            app.world_mut().spawn((pos, Mover::new(0.0), Density)).id()
        })
        .collect();
    app.update();

    // Baseline tick with no requests: schedule overhead only.
    let mut idle = Vec::with_capacity(TICKS);
    for _ in 0..TICKS {
        let t = Instant::now();
        app.update();
        idle.push(t.elapsed());
    }

    let mut ticks = Vec::with_capacity(TICKS);
    for _ in 0..TICKS {
        for &entity in &movers {
            let dir = Dir::DMI_ORDER[(rng.next() % 8) as usize];
            app.world_mut().write_message(MoveRequest { entity, dir });
        }
        let t = Instant::now();
        app.update();
        ticks.push(t.elapsed());
    }
    idle.sort();
    ticks.sort();

    // Index update alone: replace TilePos through the hooks, as the move system does.
    let mut update = Duration::ZERO;
    for _ in 0..TICKS {
        let t = Instant::now();
        for &e in &movers {
            let pos = *app.world().get::<TilePos>(e).unwrap();
            let to = TilePos::new((pos.x % SIZE as i32) + 1, pos.y, pos.z);
            app.world_mut()
                .modify_component(e, |p: &mut TilePos| *p = to)
                .unwrap();
        }
        update += t.elapsed();
    }
    let per_update = update / (TICKS * MOVERS) as u32;

    // Interest: VIEWERS viewers, Chebyshev radius VIEW_RADIUS, entity lists and chunk lists.
    let viewers: Vec<_> = movers[..VIEWERS]
        .iter()
        .map(|&e| *app.world().get::<TilePos>(e).unwrap())
        .collect();
    let index = app.world().resource::<SpatialIndex>();
    let mut radius_times = Vec::with_capacity(TICKS);
    let mut seen = 0usize;
    for _ in 0..TICKS {
        let t = Instant::now();
        seen = viewers
            .iter()
            .map(|&c| index.in_radius(c, VIEW_RADIUS).count())
            .sum();
        radius_times.push(t.elapsed());
    }
    radius_times.sort();
    let t = Instant::now();
    let mut chunks = 0usize;
    for _ in 0..TICKS {
        chunks = viewers
            .iter()
            .map(|&c| index.chunks_in_radius(c, VIEW_RADIUS).count())
            .sum();
    }
    let chunk_pass = t.elapsed() / TICKS as u32;
    black_box((seen, chunks));

    println!("spawn {STATIC} static (index insert via hooks): {spawn:?}");
    println!(
        "idle tick: p50 {:?} p99 {:?}",
        percentile(&idle, 0.5),
        percentile(&idle, 0.99)
    );
    println!(
        "tick with {MOVERS} move requests: p50 {:?} p99 {:?} max {:?}",
        percentile(&ticks, 0.5),
        percentile(&ticks, 0.99),
        ticks[ticks.len() - 1]
    );
    println!("index update (modify_component TilePos): {per_update:?} per move");
    println!(
        "{VIEWERS} viewers in_radius({VIEW_RADIUS}): p50 {:?} p99 {:?}, {} entities/pass ({} per viewer)",
        percentile(&radius_times, 0.5),
        percentile(&radius_times, 0.99),
        seen,
        seen / VIEWERS
    );
    println!(
        "{VIEWERS} viewers chunks_in_radius({VIEW_RADIUS}): {chunk_pass:?} per pass, {} chunks/viewer",
        chunks / VIEWERS
    );
}
