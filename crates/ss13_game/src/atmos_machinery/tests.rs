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
        overclocked: false,
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
        &vent(true, EXTERNAL_BOUND),
        1.0
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
        &vent(true, EXTERNAL_BOUND),
        1.0
    ));
}

#[test]
fn vent_siphons_to_the_internal_bound() {
    let (mut net, id) = vent_net();
    let mut atmos = atmos_row(3, "o2=22;n2=82;TEMP=293.15");
    let mut siphon = vent(false, INTERNAL_BOUND);
    siphon.internal_pressure_bound = 50.0;
    assert!(vent_step(&mut net, &mut atmos, id, pos(1, 1), &siphon, 1.0));
    // (50 - 0) * 200 L / (T * R) moles into the empty 200 L port: it reaches 50 kPa.
    let air = net.air(id, 0).unwrap();
    assert!((air.total_moles() - 50.0 * 200.0 / (T20C * R)).abs() < 1e-3);
    assert!((air.pressure() - 50.0).abs() < 0.01);
}

#[test]
fn worn_vents_move_less_unless_overclocked() {
    // Half integrity: half of the 50 kPa worth of moles.
    let full = 50.0 * 200.0 / (T20C * R);
    let (mut net, id) = vent_net();
    let mut atmos = atmos_row(3, "o2=22;n2=82;TEMP=293.15");
    let mut siphon = vent(false, INTERNAL_BOUND);
    siphon.internal_pressure_bound = 50.0;
    assert!(vent_step(&mut net, &mut atmos, id, pos(1, 1), &siphon, 0.5));
    assert!((net.air(id, 0).unwrap().total_moles() - full / 2.0).abs() < 1e-3);
    // Overclocked: the full amount, and past the 50 atm port limit.
    let (mut net, id) = vent_net();
    let mut atmos = atmos_row(3, "o2=22;n2=82;TEMP=293.15");
    siphon.overclocked = true;
    assert!(vent_step(&mut net, &mut atmos, id, pos(1, 1), &siphon, 0.5));
    assert!((net.air(id, 0).unwrap().total_moles() - full).abs() < 1e-3);
    let (mut net, id) = vent_net();
    let mut atmos = atmos_row(3, "o2=22;n2=82;TEMP=293.15");
    let packed = 60.0 * ONE_ATMOSPHERE * 200.0 / (T20C * R);
    net.air_mut(id, 0).unwrap().merge(&o2(packed, T20C));
    let mut siphon = vent(false, 0);
    assert!(!vent_step(
        &mut net,
        &mut atmos,
        id,
        pos(1, 1),
        &siphon,
        1.0
    ));
    siphon.overclocked = true;
    assert!(vent_step(&mut net, &mut atmos, id, pos(1, 1), &siphon, 1.0));
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
    let leaked = volume_pump_step(&mut net, id, 50.0, false).unwrap();
    assert_eq!(leaked.total_moles(), 0.0);
    assert!(close(net.air(id, 1).unwrap().total_moles(), 25.0));
    // Overclocked: a tenth of the 25 mol leaks, and a full output no longer stops it.
    let (mut net, id) = pump_net();
    net.air_mut(id, 0).unwrap().merge(&o2(100.0, T20C));
    let n = 9500.0 * 200.0 / (R * T20C);
    net.air_mut(id, 1).unwrap().merge(&o2(n, T20C));
    assert!(volume_pump_step(&mut net, id, 50.0, false).is_none());
    let leaked = volume_pump_step(&mut net, id, 50.0, true).unwrap();
    assert!(close(leaked.total_moles(), 2.5));
    assert!(close(net.air(id, 1).unwrap().total_moles(), n + 22.5));
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
    thermo_step(&mut net, id, &freezer).unwrap();
    // O2: 20 J/K/mol; heat = dT * (200 * 5000 / 5200); T += heat / 200.
    let c = 200.0;
    let heat = (73.15 - T20C) * (c * 5000.0 / (c + 5000.0));
    let expected = (T20C * c + heat) / c;
    assert!(close(net.air(id, 0).unwrap().temperature(), expected));
}

#[test]
fn thermomachine_draws_idle_plus_its_heat_curve() {
    // An empty port moves nothing and draws nothing (tg returns before `use_energy`).
    let mut net = Pipenet::default();
    let id = net.add(spec(THERMOMACHINE, 1, 2, 1.0));
    net.build();
    let freezer = ThermoMachine::tier1(140.0, 170.0, true);
    assert_eq!(thermo_step(&mut net, id, &freezer), None);
    assert_eq!(thermo_energy(0.0, 100.0), 100.0);
    // Below 5e7 J the exponent is 1.05 - 0.16 = 0.89.
    let heat = 1e6;
    assert!((thermo_energy(heat, 100.0) - (100.0 + 5e4_f64.powf(0.89))).abs() < 1e-6);
    // Above it the exponent climbs toward 1.05.
    let big = 1e8 * 0.01 * 100.0;
    let exp = 1.05 - 5e7 * 0.16 / big;
    assert!((thermo_energy(big, 0.0) - (big * 0.05).powf(exp)).abs() < 1e-3);
}

#[test]
fn a_turned_thermomachine_reconnects_keeping_its_port_gas() {
    use super::thermo::{clockwise, next_layer};
    // Facing west at x=1 (nothing there); the pipe is at x=2.
    let mut net = Pipenet::default();
    let id = net.add(spec(THERMOMACHINE, 1, 8, 1.0));
    net.add(spec(
        "/obj/machinery/atmospherics/pipe/smart/simple",
        2,
        2,
        4.0,
    ));
    net.build();
    net.air_mut(id, 0).unwrap().merge(&o2(10.0, T20C));
    let node = |net: &Pipenet, id| net.device(id).unwrap().nodes[0];
    assert_eq!(node(&net, id), None);
    // West -> north -> east: a wrench turns it clockwise.
    assert_eq!(clockwise(8), 1);
    assert_eq!(clockwise(clockwise(8)), 4);
    let id = net.reconnect(id, 4, vec![4], 3, COLOR_OMNI).unwrap();
    assert!(node(&net, id).is_some());
    assert!(close(net.air(id, 0).unwrap().gas.get(GasId::O2), 10.0));
    // The pipe is on layer 3; layer 4 is free of it.
    assert_eq!(next_layer(3), 4);
    assert_eq!(next_layer(5), 1);
    let id = net.reconnect(id, 4, vec![4], 4, COLOR_OMNI).unwrap();
    assert_eq!(node(&net, id), None);
    assert!(close(net.air(id, 0).unwrap().gas.get(GasId::O2), 10.0));
    assert!(close(net.total_moles(), 10.0));
}

#[test]
fn tank_groups_merge_on_joining_and_share_on_leaving() {
    // Two separate groups of one tank each; a third tank between them joins both.
    let mut net = Pipenet::default();
    let ids: Vec<DeviceId> = (1..=3)
        .map(|x| net.add(spec(super::tank::TANK, x, 2, 4.0)))
        .collect();
    net.build();
    let full = |moles| PipeAir::with_gas(o2(moles, T20C), 2500.0);
    let a = net.add_tank_group(full(30.0), vec![ids[0]]);
    let b = net.add_tank_group(full(10.0), vec![ids[2]]);
    let g = net.join_tank_groups(ids[1], PipeAir::new(2500.0), &[a, b, a]);
    assert_eq!(g, a);
    assert!(net.tank_group(b).is_none());
    let group = net.tank_group(g).unwrap();
    assert_eq!((group.members.len(), group.air.volume), (3, 7500.0));
    assert!(close(group.air.gas.get(GasId::O2), 40.0));
    assert_eq!(net.tank_group_of(ids[2]), Some(g));
    // A leaver takes a third of the gas and its 2500 L.
    let share = net.leave_tank_group(ids[1], 2500.0).unwrap();
    assert!(close(share.get(GasId::O2), 40.0 / 3.0));
    let group = net.tank_group(g).unwrap();
    assert_eq!((group.members.len(), group.air.volume), (2, 5000.0));
}

#[test]
fn pipe_scrubber_moves_scrubbed_gases_into_its_tank() {
    use super::pipe_scrubber::{InternalTank, scrub};
    // A 200 L buffer with N2 and plasma; 1000 L/fire takes all of it (`min(1, 1000 / 200)`).
    let mut buffer = PipeAir::new(200.0);
    buffer.gas.set(GasId::N2, 10.0);
    buffer.gas.set(GasId::PLASMA, 4.0);
    buffer.gas.temperature = T20C;
    let mut tank = InternalTank::new(0.0);
    let plasma_only = 1 << GasId::PLASMA.0;
    scrub(&mut buffer, &mut tank.air, plasma_only, 1000.0);
    assert!(close(tank.air.gas.get(GasId::PLASMA), 4.0));
    assert!(close(tank.air.gas.get(GasId::N2), 0.0));
    assert!(close(buffer.gas.get(GasId::N2), 10.0));
    assert!(close(buffer.gas.get(GasId::PLASMA), 0.0));
    // A smaller rate takes its share of the buffer: 100 / 200 of it.
    buffer.gas.set(GasId::PLASMA, 4.0);
    scrub(&mut buffer, &mut tank.air, plasma_only, 100.0);
    assert!(close(tank.air.gas.get(GasId::PLASMA), 6.0));
    // At `PUMP_MAX_PRESSURE` the tank takes nothing more.
    tank.air.gas.set(GasId::PLASMA, 1e6);
    scrub(&mut buffer, &mut tank.air, plasma_only, 1000.0);
    assert!(close(buffer.gas.get(GasId::PLASMA), 2.0));
    // The internal canister breaks at 40% of 300.
    assert!(!tank.broken());
    tank.integrity = 120.0;
    assert!(tank.broken());
    assert_eq!(InternalTank::new(1.0).pressure_limit, 600_000.0);
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
            Canister::new(true, ONE_ATMOSPHERE),
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
    he_step(&mut net, &mut atmos, ids[0], pos(1, 1), HePipe::PIPE, false);
    let after_space = net.pipeline_air(ids[0], 0).unwrap().temperature();
    assert!(after_space < 1000.0);
    assert_eq!(atmos.temperature(pos(1, 1)), Some(TCMB));
    // A room tile: the pipeline loses heat to the air, which warms up.
    he_step(&mut net, &mut atmos, ids[1], pos(2, 1), HePipe::PIPE, false);
    assert!(net.pipeline_air(ids[0], 0).unwrap().temperature() < after_space);
    assert!(atmos.temperature(pos(2, 1)).unwrap() > room_before);
    // A minute of fires on space alone brings it toward 2.7 K.
    for _ in 0..120 {
        he_step(&mut net, &mut atmos, ids[0], pos(1, 1), HePipe::PIPE, false);
    }
    let cooled = net.pipeline_air(ids[0], 0).unwrap().temperature();
    assert!(cooled < 600.0, "space cools the loop: {cooled}");
    // Within 20 K of the tile nothing happens.
    let mut tepid = GasMixture::vacuum();
    tepid.set(GasId::N2, 50.0);
    tepid.temperature = room_before + 10.0;
    net.pipeline_air_mut(line).unwrap().gas = tepid;
    he_step(&mut net, &mut atmos, ids[1], pos(2, 1), HePipe::PIPE, false);
    assert_eq!(
        net.pipeline_air(ids[0], 0).unwrap().temperature(),
        room_before + 10.0
    );
}

#[test]
fn a_heat_exchanging_pipe_under_a_wall_shares_with_the_wall() {
    use ss13_atmos::level::conduct::Thermal;
    const HE: &str = "/obj/machinery/atmospherics/pipe/heat_exchanging/simple";
    // x=1 a wall (`/turf/closed/wall`: 0.04, 312500 J/K), x=2 open.
    let mut level = LevelAir::new(2, 1);
    level.set_tile(0, TileKind::Blocked, None, None);
    level.set_thermal(
        0,
        Thermal {
            conductivity: 0.04,
            heat_capacity: 312_500.0,
            temperature: T20C,
        },
    );
    level.set_tile(
        1,
        TileKind::Open,
        GasMixture::parse("n2=100;TEMP=293.15").ok(),
        None,
    );
    level.setup();
    let mut atmos = Atmos::default();
    atmos.insert_level(1, level);
    let mut net = Pipenet::default();
    let mut d = dm(HE);
    d.device_type = 2.0;
    d.pipe_flags = flags::CARDINAL_AUTONORMALIZE as f64;
    let (spec, _) = device_spec(Entity::PLACEHOLDER, pos(1, 1), &d, 4).unwrap();
    let id = net.add(spec);
    net.build();
    let line = net.pipeline_of(id, 0).unwrap();
    let mut hot = GasMixture::vacuum();
    hot.set(GasId::N2, 50.0);
    hot.temperature = 1000.0;
    net.pipeline_air_mut(line).unwrap().gas = hot;
    let heat = |net: &Pipenet, atmos: &Atmos| {
        let p = net.pipeline_air(id, 0).unwrap();
        let (t, c) = atmos.solid_heat(pos(1, 1)).unwrap();
        p.gas.heat_capacity() * p.temperature() + c * t
    };
    let before = heat(&net, &atmos);
    he_step(&mut net, &mut atmos, id, pos(1, 1), HePipe::PIPE, false);
    let (wall, _) = atmos.solid_heat(pos(1, 1)).unwrap();
    assert!(wall > T20C, "the wall warms: {wall}");
    assert!(net.pipeline_air(id, 0).unwrap().temperature() < 1000.0);
    assert!((heat(&net, &atmos) - before).abs() / before < 1e-9);
    // An open tile has no solid heat of its own to give.
    assert_eq!(atmos.solid_heat(pos(2, 1)), None);
    // Lava: 5000 K against the pipeline decides, even with the turf within 20 K of it.
    let mut tepid = GasMixture::vacuum();
    tepid.set(GasId::N2, 50.0);
    tepid.temperature = wall + 10.0;
    net.pipeline_air_mut(line).unwrap().gas = tepid;
    he_step(&mut net, &mut atmos, id, pos(1, 1), HePipe::PIPE, false);
    assert_eq!(net.pipeline_air(id, 0).unwrap().temperature(), wall + 10.0);
    he_step(&mut net, &mut atmos, id, pos(1, 1), HePipe::PIPE, true);
    assert!(net.pipeline_air(id, 0).unwrap().temperature() < wall + 10.0);
}

/// `pressure_valve` and `temperature_gate`: closed until their condition holds, then
/// `release_gas_to` the input's own pressure.
#[test]
fn conditional_gates_open_on_their_condition() {
    let gate = |condition| ConditionalGate {
        on: true,
        condition,
        flowing: false,
    };
    let n = 200.0 * PORT_VOLUME / (R * T20C);
    let filled = || {
        let (mut net, id) = pump_net();
        net.air_mut(id, 0).unwrap().merge(&o2(n, T20C));
        (net, id)
    };
    // 200 kPa in: a 300 kPa valve stays shut, a 150 kPa one lets half the difference out.
    let (mut net, id) = filled();
    let high = gate(GateCondition::Pressure { target: 300.0 });
    assert!(!conditional_gate_step(&mut net, id, &high));
    assert_eq!(net.air(id, 1).unwrap().total_moles(), 0.0);
    let low = gate(GateCondition::Pressure { target: 150.0 });
    assert!(conditional_gate_step(&mut net, id, &low));
    assert!(net.air(id, 1).unwrap().total_moles() > 0.0);
    // 293 K in: a gate at 273 K passes only colder gas, unless inverted.
    for (inverted, flows) in [(false, false), (true, true)] {
        let (mut net, id) = filled();
        let g = gate(GateCondition::Temperature {
            target: 273.15,
            inverted,
        });
        assert_eq!(
            conditional_gate_step(&mut net, id, &g),
            flows,
            "inverted {inverted}"
        );
    }
}

/// `temperature_pump`: 90% of each side, `rate`% of the conduction energy, input to output.
#[test]
fn temperature_pump_moves_heat_one_way() {
    let (mut net, id) = pump_net();
    net.air_mut(id, 0).unwrap().merge(&o2(10.0, 400.0));
    net.air_mut(id, 1).unwrap().merge(&o2(10.0, 300.0));
    assert!(temperature_pump_step(&mut net, id, 100.0));
    // Equal capacities: the 90% parts meet at 350 K, then mix back with the 10% left.
    assert!((net.air(id, 0).unwrap().temperature() - 355.0).abs() < 1e-6);
    assert!((net.air(id, 1).unwrap().temperature() - 345.0).abs() < 1e-6);
    // A colder input moves nothing.
    let (mut net, id) = pump_net();
    net.air_mut(id, 0).unwrap().merge(&o2(10.0, 300.0));
    net.air_mut(id, 1).unwrap().merge(&o2(10.0, 400.0));
    assert!(!temperature_pump_step(&mut net, id, 100.0));
    assert!((net.air(id, 1).unwrap().temperature() - 400.0).abs() < 1e-9);
}

/// `dp_vent_pump`: releases the input port up to the external bound; siphons the tile into
/// the output port up to `output_pressure_max`.
#[test]
fn dual_port_vent_releases_and_siphons() {
    let vent = |releasing, checks| DpVentPump {
        on: true,
        releasing,
        pressure_checks: checks,
        external_pressure_bound: ONE_ATMOSPHERE,
        input_pressure_min: 0.0,
        output_pressure_max: 50.0,
    };
    let (mut net, id) = pump_net();
    net.air_mut(id, 0).unwrap().merge(&o2(1000.0, T20C));
    let mut atmos = atmos_row(3, "TEMP=293.15");
    assert!(dp_vent_step(
        &mut net,
        &mut atmos,
        id,
        pos(2, 1),
        &vent(true, EXTERNAL_BOUND)
    ));
    let released = ONE_ATMOSPHERE * CELL_VOLUME / (T20C * R);
    assert!(close(atmos.air(pos(2, 1)).unwrap().total_moles(), released));
    // Siphon: the room is at one atmosphere, the bound is 0 kPa, the port fills to 50 kPa.
    let (mut net, id) = pump_net();
    let mut atmos = atmos_row(3, "o2=22;n2=82;TEMP=293.15");
    let mut siphon = vent(false, EXTERNAL_BOUND | INTERNAL_BOUND);
    siphon.external_pressure_bound = 0.0;
    assert!(dp_vent_step(&mut net, &mut atmos, id, pos(2, 1), &siphon));
    assert!((net.air(id, 1).unwrap().pressure() - 50.0).abs() < 0.05);
}

/// A tank group's air reconciles with the pipeline its open port is in, by volume.
#[test]
fn tank_group_shares_with_its_pipeline() {
    let mut net = Pipenet::default();
    let mut tank_spec = spec(super::tank::TANK, 1, 4, 4.0);
    tank_spec.init_dirs = 4;
    tank_spec.node_dirs = super::tank::tank_nodes(4);
    let tank = net.add(tank_spec);
    let pipe = net.add(spec(
        "/obj/machinery/atmospherics/pipe/smart/simple",
        2,
        2,
        4.0,
    ));
    let mut air = PipeAir::new(2500.0);
    air.gas = o2(1000.0, T20C);
    let group = net.add_tank_group(air, vec![tank]);
    net.build();
    net.mark_parents(tank);
    net.process();
    let line = net.pipeline_air(pipe, 0).unwrap();
    // tg gives every port a pipeline, connected or not, and a tank reconciles the pipelines
    // of all its group: all four 200 L ports take part.
    let total = 2500.0 + 4.0 * PORT_VOLUME + line.volume;
    let tank_air = &net.tank_group(group).unwrap().air;
    assert!(close(tank_air.total_moles(), 1000.0 * 2500.0 / total));
    assert!(close(line.total_moles(), 1000.0 * line.volume / total));
    assert!(close(net.total_moles(), 1000.0));
}

/// `heat_exchanger`: the two ports meet at their heat-capacity weighted temperature.
#[test]
fn heat_exchangers_share_one_temperature() {
    let mut net = Pipenet::default();
    let a = net.add(spec(HEAT_EXCHANGER_PATH, 1, 8, 1.0));
    let b = net.add(spec(HEAT_EXCHANGER_PATH, 2, 4, 1.0));
    net.build();
    net.air_mut(a, 0).unwrap().merge(&o2(10.0, 400.0));
    net.air_mut(b, 0).unwrap().merge(&o2(30.0, 200.0));
    heat_exchange_step(&mut net, a, b);
    for id in [a, b] {
        assert!((net.air(id, 0).unwrap().temperature() - 250.0).abs() < 1e-9);
    }
}

const HEAT_EXCHANGER_PATH: &str = "/obj/machinery/atmospherics/components/unary/heat_exchanger";

/// A canister below 90% integrity leaks into its tile; broken, it empties and loses density.
#[test]
fn damaged_canisters_leak_and_break_open() {
    let mut world = World::new();
    world.insert_resource(Pipenet::default());
    world.insert_resource(atmos_row(1, "TEMP=293.15"));
    world.init_resource::<bevy::ecs::message::Messages<crate::sound::PlaySound>>();
    world.init_resource::<ss13_world::SpatialIndex>();
    let can = world
        .spawn((
            pos(1, 1),
            PortableAtmos {
                air: Some(PipeAir::with_gas(o2(1000.0, T20C), 1000.0)),
                port: None,
                holding: None,
            },
            Canister::new(false, ONE_ATMOSPHERE),
            ss13_mapload::DmObj {
                path: "/obj/machinery/portable_atmospherics/canister".into(),
                max_integrity: 300.0,
                integrity_failure: 0.4,
                ..default()
            },
            crate::obj_damage::Integrity {
                current: 150.0,
                broken: false,
            },
            ss13_world::Density,
        ))
        .id();
    let held = |world: &World| {
        world
            .get::<PortableAtmos>(can)
            .unwrap()
            .air
            .as_ref()
            .unwrap()
            .total_moles()
    };
    damage::process_container_damage(&mut world);
    let tile = world
        .resource::<Atmos>()
        .air(pos(1, 1))
        .unwrap()
        .total_moles();
    assert!(held(&world) < 1000.0 && tile > 0.0, "it leaked {tile}");
    assert!(damage::atom_break(&mut world, can));
    assert_eq!(held(&world), 0.0);
    assert!(world.get::<Canister>(can).unwrap().broken);
    assert!(world.get::<ss13_world::Density>(can).is_none());
}

#[test]
fn huge_scrubbers_need_anchoring_and_scrub_around_twice() {
    use super::portable::{PortableScrubber, process_portables};
    use bevy::ecs::system::RunSystemOnce;
    let mut world = World::new();
    world.insert_resource(atmos_row(3, "co2=100;TEMP=293.15"));
    world.insert_resource(Pipenet::default());
    let scrubber = PortableScrubber {
        on: true,
        filter: gas_mask(&["co2".into()]),
        volume_rate: 1500.0,
        overpressure_m: 200.0,
        huge: true,
        movable: false,
    };
    let mut air = PipeAir::new(50000.0);
    air.gas.temperature = T20C;
    let portable = PortableAtmos {
        air: Some(air),
        port: None,
        holding: None,
    };
    let loose = world
        .spawn((
            pos(2, 1),
            portable.clone(),
            scrubber.clone(),
            crate::obj_damage::Anchored(false),
        ))
        .id();
    world.run_system_once(process_portables).unwrap();
    assert!(!world.get::<PortableScrubber>(loose).unwrap().on);
    world
        .entity_mut(loose)
        .insert(crate::obj_damage::Anchored(true));
    world.get_mut::<PortableScrubber>(loose).unwrap().on = true;
    world.run_system_once(process_portables).unwrap();
    // removal_ratio 1500 / 2500 = 0.6: its own tile once (40 left), each neighbour twice (16).
    let atmos = world.resource::<Atmos>();
    assert!(close(atmos.moles(pos(2, 1), GasId::CO2).unwrap(), 40.0));
    assert!(close(atmos.moles(pos(1, 1), GasId::CO2).unwrap(), 16.0));
    assert!(close(atmos.moles(pos(3, 1), GasId::CO2).unwrap(), 16.0));
}

#[test]
fn he_pipes_glow_from_500_kelvin_in_10_kelvin_steps() {
    let mut glow = HeGlow::default();
    assert_eq!(he_glow(&mut glow, 450.0), None, "no glow under 500 K");
    // 1250 K: heat2colour (255, 90.1, 0) halfway from the #404040 grey.
    assert_eq!(he_glow(&mut glow, 1250.0), Some([160, 77, 32, 255]));
    assert_eq!(he_glow(&mut glow, 1255.0), None, "10 K hysteresis");
    // From 2000 K the color is heat2colour itself.
    assert_eq!(he_glow(&mut glow, 3000.0), Some([255, 177, 110, 255]));
    // Cooling under 500 K redraws once with tg's negative scale (past the grey), then stops.
    assert_eq!(he_glow(&mut glow, 300.0), Some([39, 73, 73, 255]));
    assert_eq!(he_glow(&mut glow, 200.0), None);
}

#[test]
fn canister_shielding_runs_off_its_cell_and_blocks_gas_damage() {
    use super::canister::{process_canister_shielding, shield_energy};
    use crate::mecha::core::ItemCell;
    let mut world = World::new();
    world.insert_resource(atmos_row(1, "TEMP=293.15"));
    world.insert_resource(Pipenet::default());
    world.insert_resource(crate::power::MachineTicks(1));
    // 1000 mol O2 in 1000 L: ~2437 kPa, against a limit lowered to 100 kPa.
    let mut canister = Canister::new(false, ONE_ATMOSPHERE);
    canister.pressure_limit = 100.0;
    canister.shielding = true;
    let can = world
        .spawn((
            pos(1, 1),
            PortableAtmos {
                air: Some(PipeAir::with_gas(o2(1000.0, T20C), 1000.0)),
                port: None,
                holding: None,
            },
            canister,
            ss13_mapload::DmObj {
                path: "/obj/machinery/portable_atmospherics/canister".into(),
                max_integrity: 300.0,
                integrity_failure: 0.4,
                ..default()
            },
            crate::obj_damage::Integrity {
                current: 300.0,
                broken: false,
            },
        ))
        .id();
    // floor(log10(~2337)) = 3: 3 * 250 J/s over the 2 s tick.
    let c = world.get::<Canister>(can).unwrap().clone();
    let pressure = 1000.0 * R * T20C / 1000.0;
    assert_eq!(shield_energy(pressure, T20C, &c, 2.0), 1500.0);
    let mut within = c.clone();
    within.pressure_limit = super::damage::PRESSURE_LIMIT;
    assert_eq!(shield_energy(pressure, T20C, &within, 2.0), 0.0);
    let cell = world
        .spawn((
            ItemCell(crate::power::Cell {
                charge: 100.0,
                maxcharge: 1000.0,
                chargerate: 0.0,
            }),
            crate::storage::StoredIn(can),
        ))
        .id();
    world.get_mut::<Canister>(can).unwrap().cell = Some(cell);
    // Shielded: no damage over the limit.
    damage::process_container_damage(&mut world);
    assert_eq!(
        world
            .get::<crate::obj_damage::Integrity>(can)
            .unwrap()
            .current,
        300.0
    );
    let shield = world.register_system(process_canister_shielding);
    // No area power: a fortieth of 1500 J from the cell each tick.
    let charge = |world: &World| world.get::<ItemCell>(cell).unwrap().0.charge;
    world.run_system(shield).unwrap();
    assert_eq!(charge(&world), 62.5);
    // The same tick does not draw twice.
    world.run_system(shield).unwrap();
    assert_eq!(charge(&world), 62.5);
    for tick in 2..=3 {
        world.resource_mut::<crate::power::MachineTicks>().0 = tick;
        world.run_system(shield).unwrap();
    }
    // 25 J left is short of 37.5 J: the shielding drops and the gas hurts again.
    assert_eq!(charge(&world), 25.0);
    assert!(!world.get::<Canister>(can).unwrap().shielding);
    damage::process_container_damage(&mut world);
    assert!(
        world
            .get::<crate::obj_damage::Integrity>(can)
            .unwrap()
            .current
            < 300.0
    );
}

#[test]
fn a_suppressing_nob_crystal_stops_reactions_in_portables() {
    use super::portable::{NobCrystal, process_portables};
    use bevy::ecs::system::RunSystemOnce;
    let mut world = World::new();
    world.insert_resource(atmos_row(1, "TEMP=293.15"));
    world.insert_resource(Pipenet::default());
    let burning = || {
        let mut gas = GasMixture::vacuum();
        gas.set(GasId::PLASMA, 100.0);
        gas.set(GasId::O2, 500.0);
        gas.temperature = 1000.0;
        PortableAtmos {
            air: Some(PipeAir::with_gas(gas, 1000.0)),
            port: None,
            holding: None,
        }
    };
    let quiet = world
        .spawn((
            pos(1, 1),
            burning(),
            Canister::new(false, ONE_ATMOSPHERE),
            NobCrystal { suppress: true },
        ))
        .id();
    let lit = world
        .spawn((
            pos(1, 1),
            burning(),
            Canister::new(false, ONE_ATMOSPHERE),
            NobCrystal { suppress: false },
        ))
        .id();
    world.run_system_once(process_portables).unwrap();
    let plasma = |world: &World, e| {
        world
            .get::<PortableAtmos>(e)
            .unwrap()
            .air
            .as_ref()
            .unwrap()
            .gas
            .get(GasId::PLASMA)
    };
    assert_eq!(plasma(&world, quiet), 100.0);
    assert!(plasma(&world, lit) < 100.0, "{}", plasma(&world, lit));
}
