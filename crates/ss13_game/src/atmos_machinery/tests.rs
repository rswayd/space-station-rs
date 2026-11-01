//! Component math against tg's formulas, worked by hand on small pipe networks.

use ss13_atmos::gas::{CELL_VOLUME, ONE_ATMOSPHERE, R_IDEAL_GAS_EQUATION as R, T20C, TCMB};
use ss13_atmos::pipenet::air::PipeAir;
use ss13_atmos::pipenet::{COLOR_OMNI, DeviceId, DeviceSpec};
use ss13_atmos::{Atmos, GasId, GasMixture, LevelAir, TileKind};

use super::devices::*;
use super::portable::{canister_gas, pressure_light, release};
use super::*;

fn pos(x: i32, y: i32) -> TilePos {
    TilePos::new(x, y, 1)
}

/// One level of open tiles in a row, each with `air`.
fn atmos_row(width: u32, air: &str) -> Atmos {
    let mut level = LevelAir::new(width, 1);
    for t in 0..width {
        level.set_tile(t, TileKind::Open, GasMixture::parse(air).ok(), None);
    }
    level.setup();
    let mut atmos = Atmos::default();
    atmos.insert_level(1, level);
    atmos
}

fn dm(path: &str) -> DmAtmosDevice {
    DmAtmosDevice {
        path: path.into(),
        piping_layer: 3.0,
        pipe_color: "#EEEEEE".into(),
        device_type: 1.0,
        initialize_directions: 15.0,
        ..default()
    }
}

fn spec(path: &str, x: i32, dir: u8, device_type: f64) -> DeviceSpec {
    let mut d = dm(path);
    d.device_type = device_type;
    device_spec(Entity::PLACEHOLDER, pos(x, 1), &d, dir)
        .unwrap()
        .0
}

fn o2(moles: f64, temperature: f64) -> GasMixture {
    let mut g = GasMixture::vacuum();
    g.set(GasId::O2, moles);
    g.temperature = temperature;
    g
}

/// Within tg's `QUANTIZE` rounding (1e-4 mol).
fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 2e-4
}

/// A vent at x=1 facing east into a pipe at x=2.
fn vent_net() -> (Pipenet, DeviceId) {
    let mut net = Pipenet::default();
    let vent = net.add(spec(VENT_PUMP, 1, 4, 1.0));
    net.add(spec(
        "/obj/machinery/atmospherics/pipe/smart/simple",
        2,
        2,
        4.0,
    ));
    net.build();
    (net, vent)
}

fn vent(releasing: bool, checks: u8) -> VentPump {
    VentPump {
        on: true,
        releasing,
        pressure_checks: checks,
        external_pressure_bound: ONE_ATMOSPHERE,
        internal_pressure_bound: 0.0,
        welded: false,
        area: None,
    }
}

#[test]
fn family_specs_follow_set_init_directions() {
    // Binary facing east: node 1 west (input), node 2 east (output).
    let pump = spec(PUMP, 1, 4, 2.0);
    assert_eq!(
        (pump.init_dirs, pump.node_dirs.clone()),
        (4 | 8, vec![8, 4])
    );
    // Trinary facing south: SOUTH|WEST|NORTH; nodes north (in), west (side), south (out).
    let filter = spec(FILTER, 1, 2, 3.0);
    assert_eq!(filter.init_dirs, 2 | 8 | 1);
    assert_eq!(filter.node_dirs, vec![1, 8, 2]);
    // Flipped: in and out swap.
    let mut d = dm(FILTER);
    d.device_type = 3.0;
    d.flipped = true;
    let flipped = device_spec(Entity::PLACEHOLDER, pos(1, 1), &d, 2)
        .unwrap()
        .0;
    assert_eq!(flipped.node_dirs, vec![2, 8, 1]);
    // A mixer's output holds 300 L, a high volume vent 1000 L, a connector nothing.
    assert_eq!(
        spec(MIXER, 1, 2, 3.0).port_volumes,
        vec![200.0, 200.0, 300.0]
    );
    assert_eq!(spec(HIGH_VOLUME_VENT, 1, 2, 1.0).port_volumes, vec![1000.0]);
    assert_eq!(spec(CONNECTOR, 1, 2, 1.0).port_volumes, vec![0.0]);
    assert_eq!(spec(VENT_PUMP, 1, 2, 1.0).color, COLOR_OMNI);
}

#[test]
fn vent_releases_to_the_external_bound() {
    let (mut net, id) = vent_net();
    // 50 kPa room, 1000 mol at T20C in the port.
    let room = format!("o2={};TEMP=293.15", 50.0 * CELL_VOLUME / (R * T20C));
    let mut atmos = atmos_row(3, &room);
    net.air_mut(id, 0).unwrap().merge(&o2(1000.0, T20C));
    assert!(vent_step(
        &mut net,
        &mut atmos,
        id,
        pos(1, 1),
        &vent(true, EXTERNAL_BOUND)
    ));
    // tg: (101.325 - 50) * 2500 / (T * R) moles leave the port.
    let moved = (ONE_ATMOSPHERE - 50.0) * CELL_VOLUME / (T20C * R);
    let left = net.air(id, 0).unwrap().total_moles();
    assert!((1000.0 - moved - left).abs() < 1e-3, "{left}");
    assert!((atmos.pressure(pos(1, 1)).unwrap() - ONE_ATMOSPHERE).abs() < 0.01);
    // At the bound nothing moves.
    assert!(!vent_step(
        &mut net,
        &mut atmos,
        id,
        pos(1, 1),
        &vent(true, EXTERNAL_BOUND)
    ));
}

#[test]
fn vent_siphons_to_the_internal_bound() {
    let (mut net, id) = vent_net();
    let mut atmos = atmos_row(3, "o2=22;n2=82;TEMP=293.15");
    let mut siphon = vent(false, INTERNAL_BOUND);
    siphon.internal_pressure_bound = 50.0;
    assert!(vent_step(&mut net, &mut atmos, id, pos(1, 1), &siphon));
    // (50 - 0) * 200 L / (T * R) moles into the empty 200 L port: it reaches 50 kPa.
    let air = net.air(id, 0).unwrap();
    assert!((air.total_moles() - 50.0 * 200.0 / (T20C * R)).abs() < 1e-3);
    assert!((air.pressure() - 50.0).abs() < 0.01);
}

#[test]
fn scrubber_takes_its_gases_by_volume_rate() {
    let (mut net, id) = vent_net();
    let mut atmos = atmos_row(3, "o2=22;n2=82;co2=10;TEMP=293.15");
    let scrubber = VentScrubber {
        on: true,
        scrubbing: true,
        filter: gas_mask(&["co2".into()]),
        widenet: false,
        volume_rate: 200.0,
        welded: false,
        area: None,
    };
    assert!(scrub(&mut net, &mut atmos, id, pos(1, 1), &scrubber));
    // removal_ratio = 200 / 2500; one filtered gas: 10 * 0.08 * 1 = 0.8 mol.
    assert!(close(net.air(id, 0).unwrap().gas.get(GasId::CO2), 0.8));
    assert!(close(atmos.moles(pos(1, 1), GasId::CO2).unwrap(), 9.2));
    assert_eq!(net.air(id, 0).unwrap().gas.get(GasId::O2), 0.0);
    // Siphoning takes 8% of everything.
    let mut siphon = scrubber.clone();
    siphon.scrubbing = false;
    let before = atmos.air(pos(2, 1)).unwrap().total_moles();
    assert!(scrub(&mut net, &mut atmos, id, pos(2, 1), &siphon));
    assert!((atmos.air(pos(2, 1)).unwrap().total_moles() - before * 0.92).abs() < 1e-2);
}

/// A pump at x=2 facing east between pipes at x=1 and x=3.
fn pump_net() -> (Pipenet, DeviceId) {
    let mut net = Pipenet::default();
    net.add(spec(
        "/obj/machinery/atmospherics/pipe/smart/simple",
        1,
        2,
        4.0,
    ));
    let pump = net.add(spec(PUMP, 2, 4, 2.0));
    net.add(spec(
        "/obj/machinery/atmospherics/pipe/smart/simple",
        3,
        2,
        4.0,
    ));
    net.build();
    (net, pump)
}

#[test]
fn pump_fills_the_bigger_of_port_and_pipeline() {
    let (mut net, id) = pump_net();
    net.air_mut(id, 0).unwrap().merge(&o2(500.0, T20C));
    assert!(pump_step(&mut net, id, ONE_ATMOSPHERE));
    // Both empty: n = P V / (T R) for the bigger side: the 200 L port, over the pipeline's
    // own 70 L of pipe (tg `parents[2].air` holds only the pipes' volume).
    let expected = ONE_ATMOSPHERE * 200.0 / (T20C * R);
    assert!((net.air(id, 1).unwrap().total_moles() - expected).abs() < 1e-3);
    // Fire after fire the reconciled output side (270 L) climbs to the target and stops.
    for _ in 0..20 {
        net.process();
        pump_step(&mut net, id, ONE_ATMOSPHERE);
    }
    net.process();
    let line = net.pipeline_air(id, 1).unwrap().pressure();
    assert!((line - ONE_ATMOSPHERE).abs() < 0.5, "{line}");
}

#[test]
fn volume_pump_and_passive_gate() {
    let (mut net, id) = pump_net();
    net.air_mut(id, 0).unwrap().merge(&o2(100.0, T20C));
    // 50 L/s of a 200 L port: a quarter.
    assert!(volume_pump_step(&mut net, id, 50.0));
    assert!(close(net.air(id, 1).unwrap().total_moles(), 25.0));
    // A passive gate releases at most half the pressure difference.
    let (mut net, id) = pump_net();
    let n = 200.0 * 200.0 / (R * T20C);
    net.air_mut(id, 0).unwrap().merge(&o2(n, T20C));
    assert!(passive_gate_step(&mut net, id, 1000.0));
    net.update_parents(id);
    net.process();
    assert!(net.air(id, 1).unwrap().pressure() <= 100.0 + 0.01);
}

#[test]
fn filter_splits_and_mixer_mixes() {
    let mut net = Pipenet::default();
    let filter = net.add(spec(FILTER, 1, 2, 3.0));
    net.build();
    let mut gas = o2(50.0, T20C);
    gas.set(GasId::PLASMA, 50.0);
    net.air_mut(filter, 0).unwrap().merge(&gas);
    // 200 L/s of 200 L: all of it; plasma to the side, oxygen out.
    assert!(filter_step(&mut net, filter, 200.0, 1 << GasId::PLASMA.0));
    assert!(close(
        net.air(filter, 1).unwrap().gas.get(GasId::PLASMA),
        50.0
    ));
    assert!(close(net.air(filter, 2).unwrap().gas.get(GasId::O2), 50.0));
    assert_eq!(net.air(filter, 0).unwrap().total_moles(), 0.0);

    let mut net = Pipenet::default();
    let mixer = net.add(spec(MIXER, 1, 2, 3.0));
    net.build();
    let mut n2 = GasMixture::vacuum();
    n2.set(GasId::N2, 1000.0);
    n2.temperature = T20C;
    net.air_mut(mixer, 0).unwrap().merge(&n2);
    net.air_mut(mixer, 1).unwrap().merge(&o2(1000.0, T20C));
    let airmix = GasMixer {
        on: true,
        target_pressure: ONE_ATMOSPHERE,
        node1_concentration: 0.79,
        node2_concentration: 0.21,
        flipped: false,
    };
    assert!(mixer_step(&mut net, mixer, &airmix));
    // general = 101.325 * 300 / R; moles = c * general / T.
    let general = ONE_ATMOSPHERE * 300.0 / R;
    let out = net.air(mixer, 2).unwrap();
    assert!(close(out.gas.get(GasId::N2), 0.79 * general / T20C));
    assert!(close(out.gas.get(GasId::O2), 0.21 * general / T20C));
    assert!((out.pressure() - ONE_ATMOSPHERE).abs() < 0.01);
}

#[test]
fn thermomachine_conducts_toward_its_target() {
    let mut net = Pipenet::default();
    let id = net.add(spec(THERMOMACHINE, 1, 2, 1.0));
    net.build();
    net.air_mut(id, 0).unwrap().merge(&o2(10.0, T20C));
    let freezer = ThermoMachine {
        target_temperature: 73.15,
        ..ThermoMachine::tier1(140.0, 170.0, true)
    };
    assert!((freezer.min_temperature - 73.15).abs() < 1e-9);
    assert_eq!(
        (freezer.max_temperature, freezer.heat_capacity),
        (573.15, 5000.0)
    );
    thermo_step(&mut net, id, &freezer);
    // O2: 20 J/K/mol; heat = dT * (200 * 5000 / 5200); T += heat / 200.
    let c = 200.0;
    let heat = (73.15 - T20C) * (c * 5000.0 / (c + 5000.0));
    let expected = (T20C * c + heat) / c;
    assert!(close(net.air(id, 0).unwrap().temperature(), expected));
}

#[test]
fn canisters_fill_and_release_like_tg() {
    let mut air = dm("/obj/machinery/portable_atmospherics/canister/air");
    air.volume = 2000.0;
    air.maximum_pressure = 90.0 * ONE_ATMOSPHERE;
    air.filled = 0.5;
    let gas = canister_gas(&air);
    let per = 90.0 * ONE_ATMOSPHERE * 0.5 * 2000.0 / (R * T20C);
    assert!(close(gas.get(GasId::O2), 0.21 * per));
    assert!(close(gas.get(GasId::N2), 0.79 * per));
    let mut can = PipeAir::with_gas(gas, 2000.0);
    // 45 atmospheres: the "can-3" light starts at 40.
    assert_eq!(pressure_light(can.pressure()), Some("can-3"));
    // The valve lets out what makes 101.325 kPa at its temperature; the vacuum turf's 7000 J/K
    // (tg `/datum/gas_mixture/turf/heat_capacity`) cools it on arrival.
    let mut atmos = atmos_row(1, "TEMP=2.7");
    assert!(release(
        &mut can,
        None,
        &mut atmos,
        pos(1, 1),
        ONE_ATMOSPHERE
    ));
    let moles = ONE_ATMOSPHERE * CELL_VOLUME / (T20C * R);
    let c = moles * 20.0;
    let t = (T20C * c + 2.7 * 7000.0) / (c + 7000.0);
    let p = atmos.pressure(pos(1, 1)).unwrap();
    assert!((p - moles * R * t / CELL_VOLUME).abs() < 0.05, "{p}");
}

#[test]
fn processing_a_canister_leaves_its_look_alone() {
    use bevy::ecs::system::RunSystemOnce;
    let mut world = World::new();
    world.insert_resource(Pipenet::default());
    world.insert_resource(atmos_row(1, "TEMP=2.7"));
    let can = world
        .spawn((
            pos(1, 1),
            PortableAtmos {
                air: Some(PipeAir::with_gas(o2(100.0, T20C), 1000.0)),
                port: None,
                holding: None,
            },
            Canister {
                valve_open: true,
                release_pressure: ONE_ATMOSPHERE,
                light: None,
            },
        ))
        .id();
    let changed = |world: &World| {
        world
            .entity(can)
            .get_change_ticks::<PortableAtmos>()
            .unwrap()
            .changed
    };
    let before = changed(&world);
    let moles = |world: &World| {
        world
            .get::<PortableAtmos>(can)
            .unwrap()
            .air
            .as_ref()
            .unwrap()
            .total_moles()
    };
    let full = moles(&world);
    world.increment_change_tick();
    world.run_system_once(portable::process_portables).unwrap();
    assert!(moles(&world) < full, "the valve let gas out");
    assert_eq!(
        changed(&world),
        before,
        "only the port and the tank redraw it"
    );
}

#[test]
fn heat_exchanging_pipes_radiate_to_space_and_warm_room_air() {
    const HE: &str = "/obj/machinery/atmospherics/pipe/heat_exchanging/simple";
    // x=1 space, x=2 and x=3 open (station air). Two HE pipes along the row, a bend at x=3
    // (dir 9: north-west) that connects to x=2 only.
    let mut level = LevelAir::new(3, 1);
    level.set_tile(0, TileKind::Space, None, None);
    for t in 1..3 {
        level.set_tile(
            t,
            TileKind::Open,
            GasMixture::parse("o2=22;n2=82;TEMP=293.15").ok(),
            None,
        );
    }
    level.setup();
    let mut atmos = Atmos::default();
    atmos.insert_level(1, level);
    let mut net = Pipenet::default();
    let mut d = dm(HE);
    d.device_type = 2.0;
    d.pipe_flags = flags::CARDINAL_AUTONORMALIZE as f64;
    let mut ids = Vec::new();
    for (x, dir) in [(1, 4), (2, 4), (3, 9)] {
        let (spec, _) = device_spec(Entity::PLACEHOLDER, pos(x, 1), &d, dir).unwrap();
        assert_eq!(
            spec.kind,
            DeviceKind::Pipe {
                volume: 0.0,
                smart: false,
                he: HeatExchange::Pipe,
            }
        );
        ids.push(net.add(spec));
    }
    net.build();
    let line = net.pipeline_of(ids[0], 0).unwrap();
    assert!(ids.iter().all(|&i| net.pipeline_of(i, 0) == Some(line)));
    // Hot coolant: 50 mol N2 at 1000 K in 210 L.
    let mut hot = GasMixture::vacuum();
    hot.set(GasId::N2, 50.0);
    hot.temperature = 1000.0;
    net.pipeline_air_mut(line).unwrap().gas = hot;
    let room_before = atmos.temperature(pos(2, 1)).unwrap();
    // Space alone: the pipeline loses heat, space stays at TCMB.
    he_step(&mut net, &mut atmos, ids[0], pos(1, 1), HePipe::PIPE);
    let after_space = net.pipeline_air(ids[0], 0).unwrap().temperature();
    assert!(after_space < 1000.0);
    assert_eq!(atmos.temperature(pos(1, 1)), Some(TCMB));
    // A room tile: the pipeline loses heat to the air, which warms up.
    he_step(&mut net, &mut atmos, ids[1], pos(2, 1), HePipe::PIPE);
    assert!(net.pipeline_air(ids[0], 0).unwrap().temperature() < after_space);
    assert!(atmos.temperature(pos(2, 1)).unwrap() > room_before);
    // A minute of fires on space alone brings it toward 2.7 K.
    for _ in 0..120 {
        he_step(&mut net, &mut atmos, ids[0], pos(1, 1), HePipe::PIPE);
    }
    let cooled = net.pipeline_air(ids[0], 0).unwrap().temperature();
    assert!(cooled < 600.0, "space cools the loop: {cooled}");
    // Within 20 K of the tile nothing happens.
    let mut tepid = GasMixture::vacuum();
    tepid.set(GasId::N2, 50.0);
    tepid.temperature = room_before + 10.0;
    net.pipeline_air_mut(line).unwrap().gas = tepid;
    he_step(&mut net, &mut atmos, ids[1], pos(2, 1), HePipe::PIPE);
    assert_eq!(
        net.pipeline_air(ids[0], 0).unwrap().temperature(),
        room_before + 10.0
    );
}
