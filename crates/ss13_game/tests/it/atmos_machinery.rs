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

    // Meters read the pipe on their tile and layer and draw what they read.
    let mut meters = server
        .world_mut()
        .query::<&ss13_game::atmos_machinery::meter::Meter>();
    let all: Vec<_> = meters.iter(server.world()).cloned().collect();
    // Pipes show their gas (`gas_visual`): most carry a colored overlay after a minute.
    let mut gas = server
        .world_mut()
        .query::<&ss13_game::atmos_machinery::gas_visuals::PipeGasOverlay>();
    let overlays: Vec<[u8; 3]> = gas.iter(server.world()).map(|g| g.color).collect();
    let lit = overlays.iter().filter(|c| **c != [0; 3]).count();
    println!(
        "pipe gas visuals: {} pipes, {lit} not black",
        overlays.len()
    );
    assert!(overlays.len() > 2000 && lit > 500);

    // Gas chamber consoles read their sensors and drive their injector and vent.
    let mut consoles = server
        .world_mut()
        .query::<&ss13_game::atmos_machinery::control::AtmosControl>();
    let controls: Vec<_> = consoles.iter(server.world()).cloned().collect();
    for c in &controls {
        for ch in &c.data.chambers {
            println!(
                "atmos control {}: gas {}, input {:?}, output {:?}",
                ch.name,
                ch.gasmix.as_ref().map_or(-1.0, |g| g.pressure),
                ch.input_info,
                ch.output_info
            );
        }
    }
    let o2 = controls
        .iter()
        .flat_map(|c| &c.data.chambers)
        .find(|ch| ch.id == "o2")
        .expect("the oxygen supply console");
    assert!(o2.gasmix.is_some() && o2.input_info.is_some() && o2.output_info.is_some());
    let distro = controls
        .iter()
        .flat_map(|c| &c.data.chambers)
        .find(|ch| ch.id == "distro")
        .expect("the master console");
    assert!(distro.gasmix.as_ref().is_some_and(|g| g.pressure > 50.0));

    // Airlock pumps: linked at round start; an external airlock asked to open cycles the
    // chamber down to vacuum, then opens.
    use ss13_game::atmos_machinery::airlock_pump::AirlockPump;
    let mut pumps = server
        .world_mut()
        .query::<(Entity, &AirlockPump, &TilePos)>();
    let all_pumps: Vec<(Entity, AirlockPump, TilePos)> = pumps
        .iter(server.world())
        .map(|(e, p, &pos)| (e, p.clone(), pos))
        .collect();
    let linked = all_pumps.iter().filter(|(_, p, _)| p.set_up).count();
    println!("airlock pumps: {} mapped, {linked} linked", all_pumps.len());
    // The labor shuttle's pump finds its far side only through `close_others` (cyclelink
    // helpers), which doors do not load yet.
    assert!(all_pumps.len() >= 20 && linked + 1 >= all_pumps.len());
    let (pump, p, pump_pos) = all_pumps
        .iter()
        .find(|(_, _, pos)| {
            server
                .world()
                .resource::<Atmos>()
                .pressure(*pos)
                .is_some_and(|kpa| kpa > 90.0)
        })
        .cloned()
        .expect("a pressurized airlock");
    let door = p.external[0];
    server
        .world_mut()
        .get_mut::<ss13_game::doors::Door>(door)
        .unwrap()
        .cycle_requested = true;
    run(&mut server, 15.0);
    let after = server
        .world()
        .resource::<Atmos>()
        .pressure(pump_pos)
        .unwrap();
    let state = server.world().get::<AirlockPump>(pump).unwrap().clone();
    println!(
        "cycled airlock at {pump_pos:?}: {after:.2} kPa, on {}",
        state.on
    );
    assert!(after < 5.0 && !state.on, "decompressed: {after}");
    let door_state = |server: &App, e: Entity| {
        let d = server.world().get::<ss13_game::doors::Door>(e).unwrap();
        (d.state, d.locked)
    };
    println!(
        "external after decompression: {:?}",
        door_state(&server, door)
    );
    assert!(matches!(
        door_state(&server, door),
        (ss13_game::doors::DoorState::Open, true)
    ));
    // Back in: the internal airlock pressurizes from distro and opens.
    let inner = state.internal[0];
    server
        .world_mut()
        .get_mut::<ss13_game::doors::Door>(inner)
        .unwrap()
        .cycle_requested = true;
    run(&mut server, 15.0);
    let back = server
        .world()
        .resource::<Atmos>()
        .pressure(pump_pos)
        .unwrap();
    println!(
        "pressurized: {back:.1} kPa, internal {:?}",
        door_state(&server, inner)
    );
    assert!(back > 95.0, "pressurized: {back}");
    assert!(matches!(
        door_state(&server, inner),
        (ss13_game::doors::DoorState::Open, true)
    ));
    // `unlink_airlock`: losing the only external airlock breaks the setup and unbolts the rest.
    for &d in &state.external {
        server.world_mut().despawn(d);
    }
    server.update();
    let broken = server.world().get::<AirlockPump>(pump).unwrap().clone();
    assert!(!broken.set_up && broken.internal.is_empty());
    assert!(!door_state(&server, inner).1, "internal unbolted");

    // Portable pumps and scrubbers draw `[initial(icon_state)]_[on]`.
    {
        use ss13_game::atmos_machinery::portable::{PortablePump, PortableScrubber};

        let mut q = server.world_mut().query::<(
            &ss13_mapload::DmAtom,
            &ss13_shared::Appearance,
            Option<&PortablePump>,
            Option<&PortableScrubber>,
        )>();
        let icons = server
            .world()
            .resource::<ss13_mapload::fidelity::IconIndex>();
        let mut drawn = 0;
        for (atom, look, pump, scrubber) in q.iter(server.world()) {
            let Some(on) = pump.map(|p| p.on).or(scrubber.map(|s| s.on)) else {
                continue;
            };
            let want = icons.get(&atom.icon, &format!("{}_{}", atom.icon_state, u8::from(on)));
            assert_eq!(Some(look.icon_state), want, "{}", atom.icon_state);
            drawn += 1;
        }
        println!("portable pumps and scrubbers drawn: {drawn}");
        assert!(drawn > 0);
    }

    // Mapped canisters: a charged high cell inside, `pressure_limit` up to 20% over 500 MPa,
    // and the shielding and hatch overlays exist.
    {
        use ss13_game::atmos_machinery::{canister, portable::Canister};

        let mut q = server.world_mut().query::<(Entity, &Canister)>();
        let found: Vec<(Entity, f64)> = q
            .iter(server.world())
            .map(|(e, c)| (e, c.pressure_limit))
            .collect();
        let world = server.world();
        let limits: Vec<f64> = found.iter().map(|&(_, l)| l).collect();
        for &(e, limit) in &found {
            assert_eq!(canister::cell_percent(world, e), 100.0, "{e:?}");
            assert!((500_000.0..=600_000.0).contains(&limit), "{limit}");
        }
        let spread = limits.iter().copied().fold(f64::NAN, f64::max)
            - limits.iter().copied().fold(f64::NAN, f64::min);
        println!(
            "canisters with cells: {}, limit spread {spread:.0} kPa",
            found.len()
        );
        assert!(found.len() > 10 && spread > 0.0);
        let icons = world.resource::<ss13_mapload::fidelity::IconIndex>();
        for state in ["shielding", "cell_hatch", "can-0-light"] {
            assert!(
                icons
                    .get("icons/obj/pipes_n_cables/canisters.dmi", state)
                    .is_some(),
                "{state}"
            );
        }
    }

    // Thermomachines: the panel sprite, the pipe stub and the emissive resolve; a wrench with the
    // panel open turns one clockwise and it stays in the pipenet.
    {
        use ss13_game::atmos_machinery::devices::ThermoMachine;
        use ss13_game::atmos_machinery::thermo;
        use ss13_game::tools::ToolKind;
        use ss13_mapload::DmAtom;
        use ss13_shared::Appearance;

        let mut q = server
            .world_mut()
            .query_filtered::<(Entity, &DmAtom), With<ThermoMachine>>();
        let (machine, icon) = q
            .iter(server.world())
            .map(|(e, a)| (e, a.icon.clone()))
            .next()
            .expect("a mapped thermomachine");
        let icons = server.world().resource::<ss13_mapload::IconIndex>();
        let open = icons.get(&icon, "thermo-open").expect("thermo-open");
        for state in ["pipe", "thermo-emissive"] {
            assert!(
                icons
                    .get("icons/obj/machines/atmospherics/thermomachine.dmi", state)
                    .is_some(),
                "{state}"
            );
        }
        let world = server.world_mut();
        world.get_mut::<ThermoMachine>(machine).unwrap().on = false;
        let client = Entity::PLACEHOLDER;
        assert!(thermo::item_interaction(
            world,
            client,
            machine,
            Some(ToolKind::Screwdriver),
            false
        ));
        assert!(world.get::<ThermoMachine>(machine).unwrap().panel_open);
        let before = *world.get::<AtmosDevice>(machine).unwrap();
        assert!(thermo::item_interaction(
            world,
            client,
            machine,
            Some(ToolKind::Wrench),
            false
        ));
        let after = *world.get::<AtmosDevice>(machine).unwrap();
        assert_eq!(after.dir, thermo::clockwise(before.dir));
        assert!(world.resource::<Pipenet>().device(after.id).is_some());
        server.update();
        let look = server.world().get::<Appearance>(machine).unwrap();
        assert_eq!(look.icon_state, open);
        assert!(!look.overlays.is_empty(), "the pipe stub");
    }

    // `relabel`: every label has its canister type and its own sprite; a plasma label copies the
    // name and sprite of a mapped plasma canister.
    {
        use ss13_game::atmos_machinery::{canister, portable::Canister};
        use ss13_mapload::{DmName, DmObj};
        use ss13_shared::Appearance;

        const PLASMA: &str = "/obj/machinery/portable_atmospherics/canister/plasma";
        let mut q = server
            .world_mut()
            .query_filtered::<(Entity, &DmObj, &Appearance, &DmName), With<Canister>>();
        let all: Vec<(Entity, String, Appearance, DmName)> = q
            .iter(server.world())
            .map(|(e, o, a, n)| (e, o.path.clone(), a.clone(), n.clone()))
            .collect();
        let (_, _, plasma_look, plasma_name) = all
            .iter()
            .find(|(_, p, ..)| p == PLASMA)
            .expect("a mapped plasma canister")
            .clone();
        let (target, ..) = all.iter().find(|(_, p, ..)| p != PLASMA).unwrap().clone();
        let world = server.world_mut();
        let mut sprites = std::collections::HashSet::new();
        for i in 0..canister::LABELS.len() {
            let (_, path) = canister::LABELS[i];
            assert!(canister::relabel(world, target, path), "{path}");
            sprites.insert(world.get::<Appearance>(target).unwrap().icon_state);
        }
        println!("relabel sprites: {}", sprites.len());
        // Helium and halon share their colors.
        assert!(sprites.len() >= canister::LABELS.len() - 1);
        assert!(canister::relabel(world, target, PLASMA));
        assert_eq!(world.get::<DmName>(target), Some(&plasma_name));
        assert_eq!(
            world.get::<Appearance>(target).unwrap().icon_state,
            plasma_look.icon_state
        );
    }

    // Overclocked vents: the fan wears 0.5 a fire without armor, and a dead one leaves the pipes.
    use ss13_game::atmos_machinery::overclock;
    let mut vents = server
        .world_mut()
        .query::<(Entity, &VentPump, &AtmosDevice)>();
    let (vent, device) = vents
        .iter(server.world())
        .find(|(_, v, _)| v.on && v.area.is_some())
        .map(|(e, _, d)| (e, *d))
        .expect("a running vent");
    overclock::toggle_vent_overclock(server.world_mut(), vent);
    run(&mut server, 10.0);
    let worn = overclock::integrity_percent(server.world(), vent);
    println!("overclocked vent after 10 s: {worn:.2}");
    assert!(worn < 1.0 && worn > 0.5, "worn: {worn}");
    server
        .world_mut()
        .get_mut::<ss13_game::obj_damage::Integrity>(vent)
        .unwrap()
        .current = 0.5;
    run(&mut server, 3.0);
    assert!(server.world().get_entity(vent).is_err(), "destroyed");
    assert!(
        server
            .world()
            .resource::<Pipenet>()
            .device(device.id)
            .is_none()
    );

    // Stationary tanks: merge groups sharing air, about 23 MPa at start (half of 46 MPa).
    let mut tanks = server
        .world_mut()
        .query::<&ss13_game::atmos_machinery::tank::StationaryTank>();
    let groups: std::collections::BTreeSet<u32> =
        tanks.iter(server.world()).map(|t| t.group).collect();
    let tank_count = tanks.iter(server.world()).count();
    {
        let net = server.world().resource::<Pipenet>();
        for &g in &groups {
            let group = net.tank_group(g).unwrap();
            println!(
                "tank group {g}: {} tanks, {:.0} L, {:.0} kPa",
                group.members.len(),
                group.air.volume,
                group.air.pressure()
            );
            assert!(group.air.pressure() > 1000.0, "tanks start full");
        }
    }
    assert!(
        tank_count >= 20 && groups.len() < tank_count,
        "{tank_count} tanks, {groups:?}"
    );

    // A mapped pipe scrubber: its canister's limit is 500-600 MPa; switched on with plasma in
    // its buffer, the plasma ends up in the canister and it draws `pipe_scrubber_active`.
    {
        use ss13_game::atmos_machinery::PortableAtmos;
        use ss13_game::atmos_machinery::pipe_scrubber::PipeScrubber;
        use ss13_mapload::DmAtom;
        use ss13_shared::Appearance;

        let mut q = server
            .world_mut()
            .query_filtered::<(Entity, &DmAtom), With<PipeScrubber>>();
        let (scrubber, icon) = q
            .iter(server.world())
            .map(|(e, a)| (e, a.icon.clone()))
            .next()
            .expect("a mapped pipe scrubber");
        let world = server.world_mut();
        let limit = world
            .get::<PipeScrubber>(scrubber)
            .unwrap()
            .tank
            .pressure_limit;
        assert!((500_000.0..=600_000.0).contains(&limit), "{limit}");
        world.get_mut::<PipeScrubber>(scrubber).unwrap().on = true;
        world.resource_scope(|world, mut net: Mut<Pipenet>| {
            let mut p = world.get_mut::<PortableAtmos>(scrubber).unwrap();
            let air = p.air_mut(&mut net).unwrap();
            air.gas.set(ss13_atmos::GasId::PLASMA, 5.0);
        });
        run(&mut server, 2.0);
        let world = server.world();
        let s = world.get::<PipeScrubber>(scrubber).unwrap();
        let in_tank = s.tank.air.gas.get(ss13_atmos::GasId::PLASMA);
        println!("pipe scrubber: {in_tank:.2} mol plasma in its tank");
        assert!(in_tank > 4.9, "{in_tank}");
        let active = world
            .resource::<ss13_mapload::IconIndex>()
            .get(&icon, "pipe_scrubber_active")
            .expect("pipe_scrubber_active");
        assert_eq!(
            world.get::<Appearance>(scrubber).unwrap().icon_state,
            active
        );
    }

    // Cutting a tank of a two-tank group open: the shared gas goes onto the tile, a plated
    // frame is left; fastened and sealed, it is an empty tank back in its neighbour's group.
    {
        use ss13_game::atmos_machinery::tank::StationaryTank;
        use ss13_game::atmos_machinery::tank_frame::{self, TankFrame};

        let mut q = server
            .world_mut()
            .query::<(Entity, &StationaryTank, &TilePos)>();
        let all: Vec<(Entity, u32, TilePos)> = q
            .iter(server.world())
            .map(|(e, t, &p)| (e, t.group, p))
            .collect();
        let (tank, group, pos) = *all
            .iter()
            .find(|(_, g, _)| {
                server
                    .world()
                    .resource::<Pipenet>()
                    .tank_group(*g)
                    .is_some_and(|t| t.members.len() == 2)
            })
            .expect("a two-tank group");
        let world = server.world_mut();
        let tile_before = world
            .resource::<Atmos>()
            .air(pos)
            .map_or(0.0, |a| a.total_moles());
        let group_moles = world
            .resource::<Pipenet>()
            .tank_group(group)
            .unwrap()
            .air
            .total_moles();
        ss13_game::obj_damage::deconstruct(world, tank, true);
        assert!(world.get_entity(tank).is_err());
        let tile_after = world
            .resource::<Atmos>()
            .air(pos)
            .map_or(0.0, |a| a.total_moles());
        println!("tank cut: tile {tile_before:.1} -> {tile_after:.1}, group {group_moles:.1} mol");
        // The group's air and what was left in its ports (`relocate_airs`).
        assert!(tile_after - tile_before >= group_moles * (1.0 - 1e-6));
        let frame = world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .find(|&e| world.get::<TankFrame>(e).is_some())
            .expect("a frame");
        assert!(world.get::<TankFrame>(frame).unwrap().plated);
        assert_eq!(
            world
                .resource::<Pipenet>()
                .tank_group(group)
                .unwrap()
                .members
                .len(),
            1
        );
        world.despawn(frame);
        let new = tank_frame::build_tank(world, pos).expect("a tank");
        let t = world.get::<StationaryTank>(new).unwrap().clone();
        assert_eq!(t.group, group, "it joins its neighbour");
        let g = world.resource::<Pipenet>().tank_group(group).unwrap();
        assert_eq!((g.members.len(), g.air.volume), (2, 5000.0));
        server.update();
    }

    // Wrenching a straight smart pipe with gas off: its share goes onto the tile and a fitting
    // is left, which rebuilds the same joints; the same fitting again crosses the rebuilt pipe,
    // narrowing it to its axis; a third has nowhere to go.
    {
        use ss13_atmos::pipenet::DeviceKind;
        use ss13_game::atmos_machinery::fitting::{self, FittingKind, PipeFitting};

        let world = server.world_mut();
        let mut q = world.query::<(Entity, &AtmosDevice, &TilePos)>();
        let candidates: Vec<(Entity, AtmosDevice, TilePos)> =
            q.iter(world).map(|(e, d, &p)| (e, *d, p)).collect();
        let net = world.resource::<Pipenet>();
        let (pipe, device, pos) = candidates
            .into_iter()
            .find(|(_, d, p)| {
                net.devices_on(*p).len() == 1
                    && net.device(d.id).is_some_and(|dev| {
                        matches!(dev.kind, DeviceKind::Pipe { smart: true, .. })
                            && dev.init_dirs == 15
                            && matches!(dev.connections(net), 3 | 12)
                    })
                    && net
                        .pipeline_air(d.id, 0)
                        .is_some_and(|a| a.total_moles() > 1.0)
            })
            .expect("a straight smart pipe with gas, alone on its tile");
        let axis = net.device(device.id).unwrap().connections(net);
        let neighbour = net
            .device(device.id)
            .unwrap()
            .nodes
            .iter()
            .flatten()
            .copied()
            .next()
            .unwrap();
        let moles = |world: &World| {
            world.resource::<Pipenet>().total_moles()
                + world
                    .resource::<Atmos>()
                    .air(pos)
                    .map_or(0.0, |a| a.total_moles())
        };
        let before = moles(world);
        ss13_game::obj_damage::deconstruct(world, pipe, true);
        assert!(world.get_entity(pipe).is_err());
        assert!(
            (moles(world) - before).abs() < 1e-3,
            "gas kept on unwrenching"
        );
        let found = world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .find(|&e| world.get::<PipeFitting>(e).is_some())
            .expect("a fitting");
        let f = world.get::<PipeFitting>(found).unwrap().clone();
        assert_eq!(
            (f.kind, f.layer, f.init_dir),
            (FittingKind::Quaternary, device.layer, 15)
        );
        world.despawn(found);
        let rebuilt = fitting::build(world, pos, f.clone()).expect("rebuilt");
        let id = world.get::<AtmosDevice>(rebuilt).unwrap().id;
        let net = world.resource::<Pipenet>();
        assert_eq!(net.device(id).unwrap().connections(net), axis);
        assert_eq!(net.pipeline_of(id, 0), net.pipeline_of(neighbour, 0));
        let before = moles(world);
        let crossing = fitting::build(world, pos, f.clone()).expect("crosses");
        let net = world.resource::<Pipenet>();
        let first = world.get::<AtmosDevice>(rebuilt).unwrap().id;
        let second = world.get::<AtmosDevice>(crossing).unwrap().id;
        assert_eq!(net.device(first).unwrap().init_dirs, axis);
        assert_eq!(net.device(second).unwrap().init_dirs, 15 & !axis);
        assert_eq!(net.pipeline_of(first, 0), net.pipeline_of(neighbour, 0));
        assert!(
            (moles(world) - before).abs() < 1e-3,
            "gas kept on narrowing"
        );
        assert!(fitting::build(world, pos, f).is_err());
        println!("pipe {pos:?}: wrenched off, rebuilt, crossed");
        server.update();

        // A pipe dispenser set to red, layer 2, north-south pipes: its fittings carry that; a
        // layer adapter stays on layer 3; the meter recipe is a meter item; the red pipe builds
        // red next to the wrenched pipe's layer.
        use ss13_game::atmos_machinery::dispenser::{PipeDispenser, dispense};
        let world = server.world_mut();
        let machine = ss13_game::construction::spawn_obj(
            world,
            ss13_mapload::atmos_machine::PIPE_DISPENSER,
            pos,
        )
        .expect("a dispenser");
        server.update();
        let world = server.world_mut();
        assert!(world.get::<PipeDispenser>(machine).is_some());
        let red = PipeDispenser {
            layer: 2,
            color: 3,
            init_dir: 1 | 2,
            ready_at: 0.0,
        };
        let pipe = dispense(world, pos, &red, 0, 0, 1).expect("a pipe fitting");
        let f = world.get::<PipeFitting>(pipe).unwrap().clone();
        assert_eq!(
            (f.pipe_type.as_str(), f.kind, f.layer, f.init_dir, f.color),
            (
                "/obj/machinery/atmospherics/pipe/smart",
                FittingKind::Quaternary,
                2,
                1 | 2,
                [0xFF, 0, 0]
            )
        );
        let adapter = dispense(world, pos, &red, 0, 1, 1).expect("an adapter");
        assert_eq!(world.get::<PipeFitting>(adapter).unwrap().layer, 3);
        let meter = dispense(world, pos, &red, 2, 7, 1).expect("a meter item");
        assert!(
            world
                .get::<ss13_game::atmos_machinery::fitting::MeterFitting>(meter)
                .is_some()
        );
        let built = fitting::build(world, pos, f).expect("a red pipe on layer 2");
        assert_eq!(
            world.get::<ss13_shared::Appearance>(built).unwrap().color,
            Some([0xFF, 0, 0, 255])
        );
        let id = world.get::<AtmosDevice>(built).unwrap().id;
        let d = world.resource::<Pipenet>().device(id).unwrap();
        assert_eq!((d.layer, d.init_dirs, d.color), (2, 1 | 2, [0xFF, 0, 0]));
        println!("dispenser: red layer 2 pipe built at {pos:?}");
        server.update();

        // An RPD: its catalog has the fitting icons; one layer of a pressure pump facing east
        // on layer 3 in Connect mode builds the pump, switched off; a heat exchanging pipe
        // refuses layer 1.
        use ss13_game::atmos_machinery::rpd::{self, Rpd, RpdJob};
        let world = server.world_mut();
        let item = ss13_game::construction::spawn_obj(world, rpd::RPD, pos).expect("an RPD");
        server.update();
        let world = server.world_mut();
        assert!(world.get::<Rpd>(item).is_some());
        let catalog = rpd::catalog_of(world, item);
        let with_icons = catalog
            .iter()
            .flat_map(|c| &c.recipes)
            .filter(|r| r.icon.is_some())
            .count();
        assert_eq!(with_icons, 26, "every recipe has its pipe_item.dmi state");
        let turf = ss13_game::construction::turf_at(world, pos).expect("a turf");
        let action = |job: RpdJob| ss13_game::tools::DoAfter {
            client: Entity::PLACEHOLDER,
            target: turf,
            target_pos: pos,
            user_pos: pos,
            item: Some(item),
            amount: 0,
            ends_at: 0.0,
            job: ss13_game::construction::Job::Atmos(
                ss13_game::atmos_machinery::windows::AtmosJob::Rpd(job),
            ),
        };
        let pumps = |world: &mut World| {
            let mut q =
                world.query::<(&ss13_game::atmos_machinery::GasPump, &AtmosDevice, &TilePos)>();
            q.iter(world)
                .filter(|(p, d, at)| **at == pos && d.layer == 3 && d.dir == 4 && !p.on)
                .count()
        };
        // The pipes there are on layers 2 and 4.
        let job = RpdJob {
            layers: 1 << 2,
            category: 1,
            index: 2,
            dir: 4,
            flipped: false,
        };
        rpd::complete(world, Entity::PLACEHOLDER, &action(job), job);
        assert_eq!(pumps(world), 1, "a pump on layer 3 facing east");
        let he = RpdJob {
            layers: 1,
            category: 3,
            index: 0,
            dir: 1,
            flipped: false,
        };
        let before = world.resource::<SpatialIndex>().on_tile(pos).len();
        rpd::complete(world, Entity::PLACEHOLDER, &action(he), he);
        assert_eq!(world.resource::<SpatialIndex>().on_tile(pos).len(), before);
        println!("rpd: pump built, layer 1 refused for heat exchange");
        server.update();

        // An electrolyzer: the panel opens, a cell goes in, the panel closes, alt-click turns
        // it on; over humid air it makes hydrogen and oxygen, draws on the cell and looks
        // busy. The mapped one is set up too.
        use ss13_game::atmos_machinery::electrolyzer::{self as electro, Electrolyzer};
        use ss13_game::mecha::core::ItemCell;
        use ss13_game::tools::{Actor, ToolKind};
        let world = server.world_mut();
        let machine = ss13_game::construction::spawn_obj(world, electro::ELECTROLYZER, pos)
            .expect("an electrolyzer");
        let cell = ss13_game::construction::spawn_obj(
            world,
            "/obj/item/stock_parts/power_store/cell/high",
            pos,
        )
        .expect("a cell");
        server.update();
        let world = server.world_mut();
        let mut q = world.query::<&Electrolyzer>();
        assert!(q.iter(world).count() >= 2, "the mapped one and ours");
        let nobody = Entity::PLACEHOLDER;
        let actor = Actor {
            client: nobody,
            mob: nobody,
            item: None,
        };
        let screwdriver = |world: &mut World| {
            electro::item_interaction(world, actor, machine, machine, Some(ToolKind::Screwdriver))
        };
        assert!(electro::item_interaction(world, actor, cell, machine, None));
        assert_eq!(electro::cell_of(world, machine), None, "the panel is shut");
        screwdriver(world);
        electro::item_interaction(world, actor, cell, machine, None);
        assert_eq!(electro::cell_of(world, machine), Some(cell));
        electro::interact(world, nobody, nobody, machine, true);
        assert!(
            !world.get::<Electrolyzer>(machine).unwrap().on,
            "not with the panel open"
        );
        screwdriver(world);
        electro::interact(world, nobody, nobody, machine, true);
        assert!(world.get::<Electrolyzer>(machine).unwrap().on);
        world
            .resource_mut::<Atmos>()
            .modify_air(pos, |a| a.set(ss13_atmos::GasId::WATER_VAPOR, 50.0));
        let charge = |world: &World| world.get::<ItemCell>(cell).unwrap().0.charge;
        let before = charge(server.world());
        run(&mut server, 2.0);
        let world = server.world_mut();
        let hydrogen: f64 = std::iter::once(pos)
            .chain(world.resource::<Atmos>().adjacent_turfs(pos, true))
            .filter_map(|t| {
                world
                    .resource::<Atmos>()
                    .moles(t, ss13_atmos::GasId::HYDROGEN)
            })
            .sum();
        let used = before - charge(world);
        let m = world.get::<Electrolyzer>(machine).unwrap();
        assert!(m.on && m.working);
        assert!(hydrogen > 1.0, "hydrogen around: {hydrogen}");
        assert!(used > 0.0 && used < 1000.0, "cell used {used} J");
        println!("electrolyzer: {hydrogen:.2} mol of hydrogen for {used:.0} J of the cell");
        server.update();

        // The mapped crystallizer set to a standard fuel pellet (any temperature) with its
        // input port holding twice the recipe: it draws the gases in, and a pellet drops south
        // named after its quality.
        use ss13_game::atmos_machinery::crystallizer::{self, Crystallizer};
        let world = server.world_mut();
        let mut q = world.query::<(Entity, &Crystallizer, &AtmosDevice, &TilePos)>();
        let (machine, id, at) = q
            .iter(world)
            .map(|(e, _, d, &p)| (e, d.id, p))
            .next()
            .expect("MetaStation has a crystallizer");
        let fuel = crystallizer::CRYSTALLIZER_RECIPES
            .iter()
            .position(|r| r.id == "fuel_basic")
            .unwrap();
        crystallizer::select_recipe(world, machine, Some(fuel));
        {
            let mut c = world.get_mut::<Crystallizer>(machine).unwrap();
            c.on = true;
            c.gas_input = 1.0;
        }
        {
            let mut net = world.resource_mut::<Pipenet>();
            let input = &mut net.airs_mut(id).unwrap()[1];
            input.gas.set(ss13_atmos::GasId::O2, 100.0);
            input.gas.set(ss13_atmos::GasId::PLASMA, 200.0);
            input.gas.temperature = 300.0;
            net.update_parents(id);
        }
        let south = at.step(ss13_shared::Dir::South);
        let pellets = |world: &mut World| {
            let mut q =
                world.query::<(&ss13_mapload::item::DmItem, &ss13_mapload::DmName, &TilePos)>();
            q.iter(world)
                .filter(|(o, _, p)| **p == south && o.path == "/obj/item/fuel_pellet")
                .map(|(_, n, _)| n.name.clone())
                .collect::<Vec<_>>()
        };
        assert!(pellets(world).is_empty());
        run(&mut server, 6.0);
        let world = server.world_mut();
        let made = pellets(world);
        let c = world.get::<Crystallizer>(machine).unwrap();
        println!(
            "crystallizer {at:?}: {made:?}, progress {:.1}, {:.1} mol inside",
            c.progress,
            c.internal.total_moles()
        );
        assert_eq!(made.len(), 1, "one pellet south of the crystallizer");
        assert!(made[0].contains(' '), "a quality word in front: {made:?}");
        server.update();

        // Chamber sensors: the three `link` helpers tie their alarms to the ordnance freezer,
        // ordnance burn and engine sensors, read there and locked to them; plain outlet vents
        // left their areas' alarm lists. A multitool: right-click resets a sensor (the alarm
        // lets go and may relink), the sensor goes into the buffer, the unlocked alarm links it
        // again.
        use ss13_game::atmos_machinery::control::{self, AirSensor};
        use ss13_game::atmos_machinery::{AirAlarm, AreaAtmos, alarm};
        let world = server.world_mut();
        let mut q = world.query::<(Entity, &AirAlarm, &TilePos)>();
        let linked: Vec<(Entity, Entity, TilePos)> = q
            .iter(world)
            .filter_map(|(e, a, &p)| Some((e, a.sensor?, p)))
            .collect();
        assert_eq!(linked.len(), 3, "the three link helpers");
        for &(e, sensor, _) in &linked {
            let a = world.get::<AirAlarm>(e).unwrap();
            let sensor_pos = *world.get::<TilePos>(sensor).unwrap();
            assert!(!a.allow_link_change);
            assert_eq!(a.area, world.resource::<Areas>().area_at(sensor_pos));
            assert!(world.get::<AirSensor>(sensor).is_some());
        }
        let mut sensors = world.query::<&AirSensor>();
        let outlets: Vec<Entity> = sensors.iter(world).filter_map(|s| s.outlet).collect();
        let plain = "/obj/machinery/atmospherics/components/unary/vent_pump";
        let plain_outlets: Vec<Entity> = outlets
            .into_iter()
            .filter(|&v| world.get::<ss13_mapload::DmAtmosDevice>(v).unwrap().path == plain)
            .collect();
        println!("sensors: {} plain outlet vents", plain_outlets.len());
        for vent in plain_outlets {
            let area = world.get::<VentPump>(vent).unwrap().area;
            assert_eq!(area, None, "a plain outlet vent leaves its area");
            assert!(
                world
                    .resource::<AreaAtmos>()
                    .0
                    .values()
                    .all(|d| !d.vents.contains(&vent)),
                "and its alarms' lists"
            );
        }
        let (alarm_e, sensor, alarm_pos) = linked[0];
        let tool = ss13_game::construction::spawn_obj(world, "/obj/item/multitool", alarm_pos)
            .expect("a multitool");
        let nobody = Entity::PLACEHOLDER;
        assert!(control::multitool_act(world, nobody, tool, sensor, true));
        let a = world.get::<AirAlarm>(alarm_e).unwrap();
        assert!(a.sensor.is_none() && a.allow_link_change);
        assert_eq!(a.area, world.resource::<Areas>().area_at(alarm_pos));
        assert!(control::multitool_act(world, nobody, tool, sensor, false));
        assert_eq!(
            ss13_game::tools::multitool_buffer(world, tool),
            Some(sensor)
        );
        world.get_mut::<AirAlarm>(alarm_e).unwrap().locked = true;
        assert!(
            !control::multitool_act(world, nobody, tool, alarm_e, false),
            "locked"
        );
        world.get_mut::<AirAlarm>(alarm_e).unwrap().locked = false;
        assert!(control::multitool_act(world, nobody, tool, alarm_e, false));
        assert_eq!(world.get::<AirAlarm>(alarm_e).unwrap().sensor, Some(sensor));
        assert_eq!(alarm::alarm_of(world, sensor), Some(alarm_e));
        // The buffered sensor onto a plain station vent: it becomes the outlet, a siphon off
        // its area's alarms.
        // Plain vents only come from fittings (the maps place subtypes): build one by the
        // alarm, on the first free layer.
        let vent = (1..=5)
            .find_map(|layer| {
                let d = ss13_game::atmos_machinery::dispenser::PipeDispenser {
                    layer,
                    ..Default::default()
                };
                let fitting =
                    ss13_game::atmos_machinery::dispenser::dispense(world, alarm_pos, &d, 2, 5, 1)?;
                let f = world.get::<PipeFitting>(fitting)?.clone();
                world.despawn(fitting);
                fitting::build(world, alarm_pos, f).ok()
            })
            .expect("a plain vent by the alarm");
        assert_eq!(
            world.get::<ss13_mapload::DmAtmosDevice>(vent).unwrap().path,
            plain
        );
        assert!(world.get::<VentPump>(vent).unwrap().area.is_some());
        assert!(control::multitool_act(world, nobody, tool, vent, false));
        assert_eq!(world.get::<AirSensor>(sensor).unwrap().outlet, Some(vent));
        let v = world.get::<VentPump>(vent).unwrap();
        assert!(v.area.is_none() && !v.releasing);
        assert!(
            world
                .resource::<AreaAtmos>()
                .0
                .values()
                .all(|d| !d.vents.contains(&vent))
        );
        assert_eq!(
            ss13_game::tools::multitool_buffer(world, tool),
            Some(sensor)
        );
        println!(
            "sensors: {} linked alarms, relinked {alarm_e:?} by multitool",
            linked.len()
        );
        server.update();
    }

    // A meter without a pipe on its layer stays unattached, as in tg (MetaStation has one
    // over a layer 2/4/5 manifold).
    let mut q = server.world_mut().query::<(
        &ss13_game::atmos_machinery::meter::Meter,
        &TilePos,
        &ss13_mapload::DmAtmosDevice,
    )>();
    let world = server.world();
    let net = world.resource::<Pipenet>();
    let wrongly_unattached = q
        .iter(world)
        .filter(|(m, pos, dm)| {
            m.target.is_none()
                && net.devices_on(**pos).iter().any(|&id| {
                    net.device(id)
                        .is_some_and(|d| d.is_pipe() && f64::from(d.layer) == dm.piping_layer)
                })
        })
        .count();
    let attached = all.iter().filter(|m| m.target.is_some()).count();
    let drawn = all.iter().filter(|m| m.look.is_some()).count();
    println!(
        "meters: {} on the map, {attached} on a pipe, {drawn} drawn",
        all.len()
    );
    assert!(all.len() >= 50 && wrongly_unattached == 0 && drawn == all.len());
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
    // The station's alert consoles list the vacuumed room as a priority alarm.
    run(&mut server, 5.0);
    let mut consoles = server
        .world_mut()
        .query::<&ss13_game::atmos_machinery::consoles::AtmosAlertConsole>();
    let lists: Vec<Vec<String>> = consoles
        .iter(server.world())
        .map(|c| c.priority.clone())
        .collect();
    println!("alert consoles: {lists:?}");
    assert!(lists.len() >= 3 && lists.iter().all(|p| !p.is_empty()));
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

/// Cyberiad: a two-deck station (one `.dmm`, levels stacked as tg's `height_autosetup` does).
/// The multi-deck adapters join each deck's distro and scrubber loops, so after a minute the
/// vents of both decks get distro pressure; the chamber sensors link their alarms.
#[test]
#[ignore = "needs generated assets and Cyberiad; run with --ignored"]
fn cyberiad() {
    use ss13_game::atmos_machinery::looks::MultiZAdapter;
    let map = MapLoadPlugin {
        maps: vec!["Cyberiad".into()],
        ..metastation_map()
    };
    let mut server = server_app(map, |app| {
        app.add_plugins(AtmosPlugin);
    });
    let report = server.world().resource::<AtmosMachineryReport>().clone();
    println!(
        "Cyberiad atmos machinery: {} devices, {} pipelines, {} vents, {} scrubbers, {} alarms",
        report.devices, report.pipelines, report.vents, report.scrubbers, report.alarms
    );
    let levels = ss13_game::events::controller::station_levels(server.world());
    assert_eq!(levels, 1..=2, "both decks are station levels");

    let mut adapters = server
        .world_mut()
        .query_filtered::<(&AtmosDevice, &TilePos), With<MultiZAdapter>>();
    let world = server.world();
    let net = world.resource::<Pipenet>();
    let all: Vec<(TilePos, [bool; 3])> = adapters
        .iter(world)
        .map(|(d, &p)| {
            let n = &net.device(d.id).unwrap().nodes;
            (p, [n[0].is_some(), n[1].is_some(), n[2].is_some()])
        })
        .collect();
    let vertical = all.iter().filter(|(_, n)| n[1] || n[2]).count();
    let lonely: Vec<&TilePos> = all
        .iter()
        .filter(|(_, n)| !n[1] && !n[2])
        .map(|(p, _)| p)
        .collect();
    println!(
        "multi-deck adapters: {} on the map, {vertical} joined to another deck, alone: {lonely:?}",
        all.len()
    );
    assert!(all.len() >= 10 && vertical == all.len());

    let (p, m, max) = run(&mut server, 60.0);
    println!("first minute: pipenets {p:.1} µs, machinery {m:.1} µs per fire, max {max:.1} µs");
    let mut vents = server
        .world_mut()
        .query::<(&AtmosDevice, &VentPump, &TilePos)>();
    let world = server.world();
    let net = world.resource::<Pipenet>();
    for z in levels {
        let on_deck: Vec<f64> = vents
            .iter(world)
            .filter(|(_, v, p)| p.z == z && v.area.is_some())
            .map(|(d, ..)| net.pipeline_air(d.id, 0).map_or(0.0, |a| a.pressure()))
            .collect();
        let pressurized = on_deck.iter().filter(|&&p| p > 50.0).count();
        println!(
            "deck {z}: {pressurized}/{} area vents over 50 kPa",
            on_deck.len()
        );
        let dry: Vec<(TilePos, String, Option<String>)> = vents
            .iter(world)
            .filter(|(d, v, p)| {
                p.z == z
                    && v.area.is_some()
                    && net.pipeline_air(d.id, 0).map_or(0.0, |a| a.pressure()) <= 50.0
            })
            .map(|(d, v, &p)| {
                let area = v
                    .area
                    .and_then(|a| world.get::<ss13_mapload::DmName>(a))
                    .map(|n| n.name.clone())
                    .unwrap_or_default();
                let path = world
                    .get::<ss13_mapload::DmAtmosDevice>(net.device(d.id).unwrap().entity)
                    .map(|dm| dm.path.clone());
                (p, area, path)
            })
            .collect();
        if !dry.is_empty() {
            println!("  without pressure: {dry:#?}");
        }
        assert!(
            pressurized * 10 >= on_deck.len() * 9,
            "deck {z}: most vents get distro pressure"
        );
    }
    let mut alarms = server
        .world_mut()
        .query::<&ss13_game::atmos_machinery::AirAlarm>();
    let linked = alarms
        .iter(server.world())
        .filter(|a| a.sensor.is_some())
        .count();
    println!("alarms linked to chamber sensors: {linked}");
}

/// Every station map through atmos start-up and a minute of air: what it has, what joins
/// across decks, which alarms found their sensors. A survey (`--nocapture`): it fails only on
/// a crash or on a multi-deck adapter left alone on a stacked level.
#[test]
#[ignore = "needs generated assets and every station map; run with --ignored"]
fn station_maps() {
    use ss13_game::atmos_machinery::looks::MultiZAdapter;
    let maps = [
        "_maps/map_files/MetaStation/MetaStation.dmm",
        "_maps/map_files/Cyberiad/Cyberiad.dmm",
        "_maps/map_files/Deltastation/DeltaStation2.dmm",
        "_maps/map_files/IceBoxStation/IceBoxStation.dmm",
        "_maps/map_files/KiloStation/KiloStation.dmm",
        "_maps/map_files/NebulaStation/NebulaStation.dmm",
        "_maps/map_files/tramstation/tramstation.dmm",
        "_maps/map_files/wawastation/wawastation.dmm",
        "_maps/map_files/CatwalkStation/CatwalkStation_2023.dmm",
        "_maps/map_files/debug/runtimestation.dmm",
        "_maps/map_files/debug/multiz.dmm",
    ];
    let mut lonely_total = 0;
    for path in maps {
        let map = MapLoadPlugin {
            maps: vec![path.into()],
            ..metastation_map()
        };
        let mut server = server_app(map, |app| {
            app.add_plugins(AtmosPlugin);
        });
        let report = server.world().resource::<AtmosMachineryReport>().clone();
        let levels = ss13_game::events::controller::station_levels(server.world());
        let mut adapters = server
            .world_mut()
            .query_filtered::<(&AtmosDevice, &TilePos), With<MultiZAdapter>>();
        let world = server.world();
        let net = world.resource::<Pipenet>();
        let all: Vec<(TilePos, bool)> = adapters
            .iter(world)
            .map(|(d, &p)| {
                let n = &net.device(d.id).unwrap().nodes;
                (p, n[1].is_some() || n[2].is_some())
            })
            .collect();
        let lonely: Vec<TilePos> = all.iter().filter(|(_, j)| !j).map(|(p, _)| *p).collect();
        lonely_total += lonely.len();
        run(&mut server, 60.0);
        let mut vents = server.world_mut().query::<(&AtmosDevice, &VentPump)>();
        let world = server.world();
        let net = world.resource::<Pipenet>();
        let area_vents: Vec<f64> = vents
            .iter(world)
            .filter(|(_, v)| v.area.is_some())
            .map(|(d, _)| net.pipeline_air(d.id, 0).map_or(0.0, |a| a.pressure()))
            .collect();
        let pressurized = area_vents.iter().filter(|&&p| p > 50.0).count();
        let mut alarms = server
            .world_mut()
            .query::<&ss13_game::atmos_machinery::AirAlarm>();
        let linked = alarms
            .iter(server.world())
            .filter(|a| a.sensor.is_some())
            .count();
        println!(
            "{path}: levels {levels:?}, {} devices, {} pipelines, {} alarms ({linked} on a \
             sensor), adapters {} (alone {lonely:?}), vents over 50 kPa after 60 s: \
             {pressurized}/{}",
            report.devices,
            report.pipelines,
            report.alarms,
            all.len(),
            area_vents.len()
        );
    }
    assert_eq!(
        lonely_total, 0,
        "every multi-deck adapter finds its other deck"
    );
}
