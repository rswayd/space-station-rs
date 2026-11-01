//! Atmos machinery on MetaStation.
//!
//! MetaStation (`--ignored`): pipe network build time and per-fire cost, the distro refills a
//! vacuumed room to ~101 kPa, the supermatter cooling loop moves gas once set up (nitrogen on
//! the ports, the loop pumps and filters on) and its space radiator (heat exchanging pipes)
//! cools it. The same over
//! real WebTransport with a player (a canister wrenched to a port, a vent refilling a room, the
//! canister window) is the bot case `tests/cases/atmos.case`.
//!
//! `SS13_ASSETS=<dir> cargo test -p ss13_game --all-features --test it atmos_machinery:: -- --ignored --nocapture`

use bevy::prelude::*;
use ss13_atmos::pipenet::Pipenet;
use ss13_atmos::{AirTick, Atmos, AtmosPlugin, GasMixture};
use ss13_game::atmos_machinery::{AtmosDevice, AtmosMachineryReport, VentPump};
use ss13_game::testing::{server_app, steps};
use ss13_mapload::{Areas, MapLoadPlugin};
use ss13_shared::TilePos;
use ss13_world::SpatialIndex;

/// An open turf with air of its own (not space, not a wall).
fn open(atmos: &Atmos, pos: TilePos) -> bool {
    atmos.level(pos.z).is_some_and(|l| {
        l.tile(pos.x, pos.y)
            .is_some_and(|t| l.kind(t) == ss13_atmos::TileKind::Open)
    })
}

/// Mean pressure over the open tiles of `area` on z.
fn area_pressure(world: &World, area: Entity, z: u16) -> (f64, usize) {
    let (w, h) = world.resource::<SpatialIndex>().level_size(z).unwrap();
    let areas = world.resource::<Areas>();
    let atmos = world.resource::<Atmos>();
    let (mut sum, mut n) = (0.0, 0);
    for x in 1..=w as i32 {
        for y in 1..=h as i32 {
            let pos = TilePos::new(x, y, z);
            if areas.area_at(pos) == Some(area)
                && open(atmos, pos)
                && let Some(p) = atmos.pressure(pos)
            {
                sum += p;
                n += 1;
            }
        }
    }
    (if n > 0 { sum / n as f64 } else { 0.0 }, n)
}

fn area_tiles(world: &World, area: Entity, z: u16) -> Vec<TilePos> {
    let (w, h) = world.resource::<SpatialIndex>().level_size(z).unwrap();
    let areas = world.resource::<Areas>();
    let atmos = world.resource::<Atmos>();
    let mut out = Vec::new();
    for x in 1..=w as i32 {
        for y in 1..=h as i32 {
            let pos = TilePos::new(x, y, z);
            // The room's air: airless tiles mapped into the area (outside a window) stay so.
            if areas.area_at(pos) == Some(area) && atmos.pressure(pos).is_some_and(|p| p > 50.0) {
                out.push(pos);
            }
        }
    }
    out
}

fn mean_pressure(world: &World, tiles: &[TilePos]) -> f64 {
    let atmos = world.resource::<Atmos>();
    tiles.iter().filter_map(|&t| atmos.pressure(t)).sum::<f64>() / tiles.len().max(1) as f64
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

/// Runs `secs` of game time; returns (mean pipenet µs, mean machinery µs, max of both) per
/// SSair fire.
fn run(server: &mut App, secs: f64) -> (f64, f64, f64) {
    let frames = steps(secs);
    let (mut pipes, mut machines, mut max, mut fires) = (0.0, 0.0, 0.0f64, 0usize);
    for _ in 0..frames {
        server.update();
        let world = server.world();
        if world.resource::<AirTick>().due {
            let p = world.resource::<Pipenet>().last_process.as_secs_f64() * 1e6;
            let m = world
                .resource::<AtmosMachineryReport>()
                .last_machinery
                .as_secs_f64()
                * 1e6;
            pipes += p;
            machines += m;
            max = max.max(p + m);
            fires += 1;
        }
    }
    let fires = fires.max(1) as f64;
    (pipes / fires, machines / fires, max)
}

#[test]
#[ignore = "needs generated assets and MetaStation; run with --ignored"]
fn metastation() {
    let mut server = server_app(metastation_map(), |app| {
        app.add_plugins(AtmosPlugin);
    });
    let report = server.world().resource::<AtmosMachineryReport>().clone();
    println!("MetaStation atmos machinery: {report:#?}");
    assert!(report.devices > 4000 && report.vents > 200 && report.alarms > 150);

    // Round start: the distro fills from the air chamber through "Air to Distro".
    let (p, m, max) = run(&mut server, 60.0);
    println!("first minute: pipenets {p:.1} µs, machinery {m:.1} µs per fire, max {max:.1} µs");
    let mut vents = server
        .world_mut()
        .query::<(Entity, &AtmosDevice, &VentPump, &TilePos)>();
    let world = server.world();
    let net = world.resource::<Pipenet>();
    let vent_list: Vec<(Entity, f64, Option<Entity>, TilePos)> = vents
        .iter(world)
        .map(|(e, d, v, &pos)| {
            let p = net.pipeline_air(d.id, 0).map_or(0.0, |a| a.pressure());
            (e, p, v.area, pos)
        })
        .collect();
    let pressurized = vent_list.iter().filter(|v| v.1 > 50.0).count();
    println!(
        "vents with distro pressure over 50 kPa: {pressurized}/{}",
        vent_list.len()
    );

    // Vacuum a room with a vent, then let the distro refill it.
    let (_, _, area, pos) = *vent_list
        .iter()
        .filter(|v| v.1 > 50.0 && v.2.is_some())
        .find(|v| {
            let (_, n) = area_pressure(server.world(), v.2.unwrap(), v.3.z);
            (6..=20).contains(&n)
        })
        .expect("a small room with a pressurized vent");
    let area = area.unwrap();
    // The room: the area's tiles air reaches from the vent (not those under closed doors).
    let in_area = area_tiles(server.world(), area, pos.z);
    let mut tiles = vec![pos];
    let mut i = 0;
    while i < tiles.len() {
        let next = server
            .world()
            .resource::<Atmos>()
            .adjacent_turfs(tiles[i], false);
        for t in next {
            if in_area.contains(&t) && !tiles.contains(&t) {
                tiles.push(t);
            }
        }
        i += 1;
    }
    {
        let mut atmos = server.world_mut().resource_mut::<Atmos>();
        for &t in &tiles {
            atmos.modify_air(t, |air| *air = GasMixture::vacuum());
        }
    }
    let (before, n) = (mean_pressure(server.world(), &tiles), tiles.len());
    // tg numbers: a vent passes what its 200 L port holds per fire (~8 mol at 101 kPa), so a
    // room of n tiles needs ~n * 104 / 8 fires.
    let (p, m, max) = run(&mut server, 180.0);
    let after = mean_pressure(server.world(), &tiles);
    println!(
        "room of {n} tiles at {pos:?}: {before:.1} kPa after the breach, {after:.1} kPa three \
         minutes later; pipenets {p:.1} µs, machinery {m:.1} µs per fire, max {max:.1} µs"
    );
    assert!(after > 95.0, "the vent refills the room: {after}");

    // The supermatter's cooling loop: what the chamber's machines do with its gas.
    sm_loop(&mut server);

    // Idle station.
    let (p, m, max) = run(&mut server, 30.0);
    println!("idle: pipenets {p:.1} µs, machinery {m:.1} µs per fire, max {max:.1} µs");
}

/// Prints the atmos machines around the supermatter, their pipelines and the chamber air, a
/// minute apart.
fn sm_loop(server: &mut App) {
    let mut q = server
        .world_mut()
        .query_filtered::<&TilePos, With<ss13_game::power::supermatter::Supermatter>>();
    let Some(&sm) = q.iter(server.world()).next() else {
        println!("no supermatter");
        return;
    };
    let dump = |server: &mut App| {
        let mut q = server
            .world_mut()
            .query::<(&AtmosDevice, &ss13_mapload::DmAtmosDevice, &TilePos)>();
        let world = server.world();
        let net = world.resource::<Pipenet>();
        let mut lines: Vec<String> = q
            .iter(world)
            .filter(|(_, _, p)| p.chebyshev(sm).is_some_and(|d| d <= 4))
            .filter(|(_, dm, _)| dm.path.contains("components"))
            .map(|(d, dm, p)| {
                let ports: Vec<String> = (0..3)
                    .filter_map(|i| net.pipeline_air(d.id, i))
                    .map(|a| format!("{:.0} kPa {:.0} K", a.pressure(), a.temperature()))
                    .collect();
                format!(
                    "{} {:?}: {}",
                    dm.path
                        .trim_start_matches("/obj/machinery/atmospherics/components/"),
                    (p.x - sm.x, p.y - sm.y),
                    ports.join(" | ")
                )
            })
            .collect();
        lines.sort();
        let air = world.resource::<Atmos>().air(sm).cloned();
        println!(
            "SM chamber: {:.1} kPa {:.0} K; machines:\n  {}",
            air.as_ref().map_or(0.0, |a| a.pressure()),
            air.as_ref().map_or(0.0, |a| a.temperature),
            lines.join("\n  ")
        );
    };
    dump(server);
    // An engineer's setup: nitrogen canisters on the four ports, the loop pumps on at max.
    let mut q = server.world_mut().query::<(
        Entity,
        &AtmosDevice,
        &TilePos,
        Option<&ss13_game::atmos_machinery::PortsConnector>,
        Option<&mut ss13_game::atmos_machinery::GasPump>,
    )>();
    let near: Vec<(Entity, ss13_atmos::pipenet::DeviceId, bool)> = q
        .iter(server.world())
        .filter(|(_, _, p, ..)| p.chebyshev(sm).is_some_and(|d| d <= 6))
        .filter(|(_, _, _, c, p)| c.is_some() || p.is_some())
        .map(|(e, d, _, c, _)| (e, d.id, c.is_some()))
        .collect();
    for (e, id, connector) in near {
        if connector {
            let mut n2 = GasMixture::vacuum();
            n2.set(ss13_atmos::GasId::N2, 3740.0);
            n2.temperature = ss13_atmos::gas::T20C;
            let air = ss13_atmos::pipenet::air::PipeAir::with_gas(n2, 2000.0);
            let _ = server.world_mut().resource_mut::<Pipenet>().attach(id, air);
        } else if let Some(mut pump) = server
            .world_mut()
            .get_mut::<ss13_game::atmos_machinery::GasPump>(e)
        {
            pump.on = true;
            pump.target_pressure = 4500.0;
        }
    }
    // ... the loop's filters on (MetaStation maps them off) ...
    let mut q = server
        .world_mut()
        .query::<(&TilePos, &mut ss13_game::atmos_machinery::GasFilter)>();
    for (p, mut filter) in q.iter_mut(server.world_mut()) {
        if p.chebyshev(sm).is_some_and(|d| d <= 8) {
            filter.on = true;
        }
    }
    // ... and the chamber scrubbers set to take every gas (the engine's air alarm).
    let mut q = server
        .world_mut()
        .query::<(&TilePos, &mut ss13_game::atmos_machinery::VentScrubber)>();
    for (p, mut scrubber) in q.iter_mut(server.world_mut()) {
        if p.chebyshev(sm).is_some_and(|d| d <= 2) {
            scrubber.filter = (1 << ss13_atmos::GAS_COUNT) - 1;
        }
    }
    run(server, 60.0);
    dump(server);
    let n2 = server
        .world()
        .resource::<Atmos>()
        .moles(sm, ss13_atmos::GasId::N2)
        .unwrap_or(0.0);
    println!("SM tile nitrogen after setup: {n2:.1} mol");
    assert!(n2 > 1.0, "the loop carries nitrogen into the chamber");
    he_report(server);
}

/// The heat exchanging pipes (tg's space radiators): how many, their pipelines and what the
/// loop's gas does in them over a minute.
fn he_report(server: &mut App) {
    use std::collections::BTreeMap;
    let lines = |server: &mut App| {
        let mut q = server.world_mut().query::<(
            &AtmosDevice,
            &ss13_game::atmos_machinery::devices::HePipe,
            &TilePos,
        )>();
        let world = server.world();
        let net = world.resource::<Pipenet>();
        let atmos = world.resource::<Atmos>();
        let mut by_line: BTreeMap<u32, (usize, usize, f64, f64, TilePos)> = BTreeMap::new();
        for (d, _, &pos) in q.iter(world) {
            let Some(line) = net.pipeline_of(d.id, 0) else {
                continue;
            };
            let air = net.pipeline_air(d.id, 0).unwrap();
            let e = by_line
                .entry(line.0)
                .or_insert((0, 0, air.pressure(), air.temperature(), pos));
            e.0 += 1;
            if atmos.space_exposed(pos) {
                e.1 += 1;
            }
        }
        by_line
    };
    let before = lines(server);
    let radiator = before.values().max_by_key(|v| v.0).copied();
    assert!(
        radiator.is_some_and(|(_, space, p, t, _)| space > 40 && p > 10.0 && t < 200.0),
        "the space radiator carries the loop's gas and cools it: {radiator:?}"
    );
    let total: usize = before.values().map(|v| v.0).sum();
    println!("HE pipes: {total} in {} pipelines", before.len());
    for (line, (n, space, p, t, pos)) in &before {
        println!("  line {line}: {n} pipes ({space} on space), {p:.0} kPa {t:.0} K, e.g. {pos:?}");
    }
    // What feeds the radiator: the machine ports on the biggest HE pipeline.
    if let Some((&line, _)) = before.iter().max_by_key(|(_, v)| v.0) {
        let mut q = server
            .world_mut()
            .query::<(&AtmosDevice, &ss13_mapload::DmAtmosDevice, &TilePos)>();
        let world = server.world();
        let net = world.resource::<Pipenet>();
        let all: Vec<_> = q.iter(world).collect();
        for &(d, dm, pos) in &all {
            for port in 0..3 {
                if dm.path.contains("components")
                    && net.pipeline_of(d.id, port).is_some_and(|l| l.0 == line)
                {
                    let other = 1 - port;
                    let other_line = net.pipeline_of(d.id, other);
                    let air = net.pipeline_air(d.id, other);
                    println!(
                        "  feeds line {line}: {} port {port} at {pos:?}; its other port: line {:?} \
                         {:.0} kPa {:.0} K",
                        dm.path,
                        other_line.map(|l| l.0),
                        air.map_or(0.0, |a| a.pressure()),
                        air.map_or(0.0, |a| a.temperature())
                    );
                    if let Some(other_line) = other_line {
                        for &(d2, dm2, pos2) in &all {
                            if dm2.path.contains("components")
                                && (0..3).any(|p| net.pipeline_of(d2.id, p) == Some(other_line))
                            {
                                println!("    on it: {} at {pos2:?}", dm2.path);
                            }
                        }
                    }
                }
            }
        }
    }
    run(server, 60.0);
    for (line, (n, _, p, t, _)) in lines(server) {
        println!("  a minute later, line {line} ({n} pipes): {p:.0} kPa {t:.0} K");
    }
}
