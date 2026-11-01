//! Timed scale check, not a correctness test:
//! `cargo test -p ss13_atmos --test bench -- --ignored --nocapture` (add `--release` for
//! release numbers).

use std::time::{Duration, Instant};

use ss13_atmos::{Atmos, LevelAir, TileKind};
use ss13_shared::{Dir, TilePos};

const AIR: &str = "o2=22;n2=82;TEMP=293.15";
const ROOM: u32 = 100;
const FIRES: usize = 300;

/// A ROOM x ROOM room of standard air in a level of space, its west wall closed for now.
fn room() -> LevelAir {
    let size = ROOM + 2;
    let mut level = LevelAir::new(size, size);
    let air = ss13_atmos::GasMixture::parse(AIR).unwrap();
    for y in 1..=size as i32 {
        for x in 1..=size as i32 {
            let t = level.tile(x, y).unwrap();
            let edge = x == 1 || y == 1 || x == size as i32 || y == size as i32;
            if !edge {
                level.set_tile(t, TileKind::Open, Some(air.clone()), None);
            } else if x != 1 {
                level.set_tile(t, TileKind::Blocked, None, None);
            }
        }
    }
    for y in 2..size as i32 {
        let t = level.tile(2, y).unwrap();
        level.set_edge_block(t, Dir::West as u8);
    }
    assert_eq!(level.setup().differing, 0);
    level
}

/// The whole west wall opens to space.
fn breach(level: &mut LevelAir) {
    for y in 2..=ROOM as i32 + 1 {
        let t = level.tile(2, y).unwrap();
        level.set_edge_block(t, 0);
        level.recalculate_adjacency(t);
        level.activate(t, false);
    }
}

fn report(name: &str, mut times: Vec<Duration>, atmos: &Atmos) {
    times.sort();
    let mean = times.iter().sum::<Duration>() / times.len() as u32;
    println!(
        "{name}: fire mean {mean:.3?}, p50 {:.3?}, p99 {:.3?}, max {:.3?} -> {:.0} fires/s \
         (real time needs 2/s); active now {}, groups {}",
        times[times.len() / 2],
        times[times.len() * 99 / 100],
        times[times.len() - 1],
        1.0 / mean.as_secs_f64(),
        atmos.active_turfs(),
        atmos.excited_groups(),
    );
}

fn run(levels: u16) {
    let mut atmos = Atmos::default();
    for z in 1..=levels {
        let mut level = room();
        breach(&mut level);
        atmos.insert_level(z, level);
    }
    let start_moles: f64 = (1..=levels).map(|z| moles(&atmos, z)).sum();
    let mut times = Vec::with_capacity(FIRES);
    let mut peak = 0;
    for _ in 0..FIRES {
        let t = Instant::now();
        atmos.fire();
        times.push(t.elapsed());
        peak = peak.max(atmos.active_turfs());
    }
    let left: f64 = (1..=levels).map(|z| moles(&atmos, z)).sum();
    println!(
        "{levels} level(s) of a {ROOM}x{ROOM} room, full west wall breach, {FIRES} fires: peak \
         active {peak}, {:.1}% of the air left, far corner {:.1} kPa",
        left / start_moles * 100.0,
        atmos
            .pressure(TilePos::new(ROOM as i32 + 1, ROOM as i32 + 1, 1))
            .unwrap()
    );
    report(&format!("{levels} level(s)"), times, &atmos);
}

fn moles(atmos: &Atmos, z: u16) -> f64 {
    let level = atmos.level(z).unwrap();
    let (w, h) = level.size();
    (0..w * h)
        .filter_map(|t| level.air(t))
        .map(|a| a.total_moles())
        .sum()
}

#[test]
#[ignore = "timing, run with --nocapture"]
fn breach_of_a_large_room() {
    run(1);
    run(8);
    // Idle: the same rooms closed cost nothing per fire.
    let mut atmos = Atmos::default();
    for z in 1..=8 {
        atmos.insert_level(z, room());
    }
    let times = (0..FIRES)
        .map(|_| {
            let t = Instant::now();
            atmos.fire();
            t.elapsed()
        })
        .collect();
    report("8 idle levels", times, &atmos);
}

/// Reaction checks on air that cannot react (every processed station turf, every oxygen tank):
/// `cargo test -p ss13_atmos --test bench react -- --ignored --nocapture`.
#[test]
#[ignore = "timing, run by hand"]
fn react_early_out() {
    use ss13_atmos::reactions::{Exposures, Holder};
    const CALLS: u32 = 1_000_000;
    for text in [AIR, "o2=6;TEMP=293.15", "plasma=20;o2=20;TEMP=293.15"] {
        let mut air = ss13_atmos::GasMixture::parse(text).unwrap();
        let start = Instant::now();
        let mut flags = 0;
        for _ in 0..CALLS {
            flags |= std::hint::black_box(&mut air).react(
                Holder::Turf,
                2500.0,
                &mut Exposures::default(),
            );
        }
        let per = start.elapsed() / CALLS;
        println!("react {text:?}: {per:?} per call, flags {flags}");
    }
}
