use super::*;
use crate::gas::{GasId, T20C};

const RED: [u8; 3] = [0xFF, 0, 0];
const BLUE: [u8; 3] = [0, 0, 0xFF];

fn pos(x: i32, y: i32) -> TilePos {
    TilePos::new(x, y, 1)
}

/// A map-placed smart pipe: all four directions allowed (tg `pipe/smart` on a map).
fn pipe(x: i32, y: i32, layer: u8, color: [u8; 3]) -> DeviceSpec {
    DeviceSpec {
        entity: Entity::PLACEHOLDER,
        pos: pos(x, y),
        init_dirs: 15,
        node_dirs: default_node_dirs(4, 15),
        layer,
        color,
        flags: 0,
        kind: DeviceKind::Pipe {
            volume: 0.0,
            smart: true,
            he: HeatExchange::None,
        },
        port_volumes: Vec::new(),
        custom: Custom::None,
    }
}

/// A unary machine facing `dir` (vent, connector).
fn unary(x: i32, y: i32, dir: u8, custom: Custom, volume: f64) -> DeviceSpec {
    DeviceSpec {
        entity: Entity::PLACEHOLDER,
        pos: pos(x, y),
        init_dirs: dir,
        node_dirs: vec![dir],
        layer: 3,
        color: COLOR_OMNI,
        flags: flags::ONE_PER_TURF,
        kind: DeviceKind::Component,
        port_volumes: vec![volume],
        custom,
    }
}

/// A binary machine facing east: node 1 west (input), node 2 east (output).
fn binary_east(x: i32, y: i32, custom: Custom) -> DeviceSpec {
    DeviceSpec {
        entity: Entity::PLACEHOLDER,
        pos: pos(x, y),
        init_dirs: 4 | 8,
        node_dirs: vec![8, 4],
        layer: 3,
        color: COLOR_OMNI,
        flags: flags::BRIDGE,
        kind: DeviceKind::Component,
        port_volumes: vec![PORT_VOLUME, PORT_VOLUME],
        custom,
    }
}

fn o2(moles: f64) -> GasMixture {
    let mut g = GasMixture::vacuum();
    g.set(GasId::O2, moles);
    g.temperature = T20C;
    g
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

#[test]
fn smart_pipes_merge_into_one_pipeline_of_summed_volume() {
    let mut net = Pipenet::default();
    let ids: Vec<_> = (0..4).map(|x| net.add(pipe(x, 0, 3, BLUE))).collect();
    // A pipe on another layer next to the run stays apart.
    let other = net.add(pipe(4, 0, 2, BLUE));
    net.build();
    let line = net.pipeline_of(ids[0], 0).unwrap();
    assert!(ids.iter().all(|&i| net.pipeline_of(i, 0) == Some(line)));
    assert_ne!(net.pipeline_of(other, 0), Some(line));
    // Ends have one connection, the middle two: every pipe is at least 70 L.
    assert_eq!(net.pipeline(line).unwrap().air.volume, 4.0 * 70.0);
    assert_eq!(net.device(ids[1]).unwrap().connections(&net), 4 | 8);
    assert_eq!(net.device(ids[0]).unwrap().connections(&net), 4);
}

#[test]
fn colors_connect_only_to_their_own_or_omni() {
    let mut net = Pipenet::default();
    let red = net.add(pipe(0, 0, 3, RED));
    let blue = net.add(pipe(1, 0, 3, BLUE));
    let omni = net.add(pipe(0, 1, 3, COLOR_OMNI));
    net.build();
    assert_ne!(net.pipeline_of(red, 0), net.pipeline_of(blue, 0));
    assert_eq!(net.pipeline_of(red, 0), net.pipeline_of(omni, 0));
}

#[test]
fn removing_a_pipe_splits_and_adding_it_back_merges_with_gas_kept() {
    let mut net = Pipenet::default();
    let ids: Vec<_> = (0..5).map(|x| net.add(pipe(x, 0, 3, BLUE))).collect();
    net.build();
    let line = net.pipeline_of(ids[0], 0).unwrap();
    net.lines[line.0 as usize].as_mut().unwrap().air.gas = o2(35.0);
    // 5 pipes * 70 L = 350 L; the middle pipe holds 70/350 of the gas.
    let released = net.remove(ids[2]).unwrap();
    assert!(close(released.get(GasId::O2), 7.0));
    let (left, right) = (
        net.pipeline_of(ids[0], 0).unwrap(),
        net.pipeline_of(ids[4], 0).unwrap(),
    );
    assert_ne!(left, right);
    for l in [left, right] {
        let pl = net.pipeline(l).unwrap();
        assert_eq!(pl.air.volume, 140.0);
        assert!(close(pl.air.gas.get(GasId::O2), 14.0));
    }
    assert!(close(net.total_moles(), 28.0));
    // A new pipe in the gap joins both halves into one pipeline, gas summed.
    let back = net.add(pipe(2, 0, 3, BLUE));
    let joined = net.pipeline_of(back, 0).unwrap();
    assert!(
        ids.iter()
            .filter(|&&i| i != ids[2])
            .all(|&i| net.pipeline_of(i, 0) == Some(joined))
    );
    let pl = net.pipeline(joined).unwrap();
    assert_eq!(pl.air.volume, 350.0);
    assert!(close(pl.air.gas.get(GasId::O2), 28.0));
}

#[test]
fn machine_ports_reconcile_by_volume_when_updated() {
    let mut net = Pipenet::default();
    let vent = net.add(unary(0, 0, 4, Custom::None, PORT_VOLUME));
    let a = net.add(pipe(1, 0, 3, BLUE));
    let b = net.add(pipe(2, 0, 3, BLUE));
    net.build();
    let line = net.pipeline_of(a, 0).unwrap();
    assert_eq!(net.pipeline_of(b, 0), Some(line));
    assert_eq!(net.pipeline_of(vent, 0), Some(line));
    assert_eq!(net.pipeline(line).unwrap().others, vec![(vent, 0)]);
    // 140 L of pipe + 200 L of port; 34 mol put in the port spread 200/340 : 140/340.
    net.process();
    net.air_mut(vent, 0).unwrap().merge(&o2(34.0));
    net.process();
    // Not updated: nothing moved.
    assert!(close(net.air(vent, 0).unwrap().gas.get(GasId::O2), 34.0));
    net.update_parents(vent);
    net.process();
    assert!(close(net.air(vent, 0).unwrap().gas.get(GasId::O2), 20.0));
    assert!(close(
        net.pipeline(line).unwrap().air.gas.get(GasId::O2),
        14.0
    ));
}

#[test]
fn machines_connected_directly_share_a_pipeline_without_pipes() {
    let mut net = Pipenet::default();
    let pump = net.add(binary_east(0, 0, Custom::None));
    let vent = net.add(unary(1, 0, 8, Custom::None, PORT_VOLUME));
    net.build();
    let line = net.pipeline_of(vent, 0).unwrap();
    assert_eq!(net.pipeline_of(pump, 1), Some(line));
    // The pump's unconnected input still has a pipeline of its own (tg `get_rebuild_target`).
    assert!(net.pipeline_of(pump, 0).is_some());
    assert_ne!(net.pipeline_of(pump, 0), Some(line));
    assert_eq!(net.pipeline(line).unwrap().air.volume, 0.0);
}

#[test]
fn open_valve_joins_its_pipelines() {
    let mut net = Pipenet::default();
    let left = net.add(pipe(0, 0, 3, BLUE));
    let valve = net.add(binary_east(1, 0, Custom::Valve { open: false }));
    let right = net.add(pipe(2, 0, 3, BLUE));
    net.build();
    let (l, r) = (
        net.pipeline_of(left, 0).unwrap(),
        net.pipeline_of(right, 0).unwrap(),
    );
    assert_ne!(l, r);
    net.lines[l.0 as usize].as_mut().unwrap().air.gas = o2(54.0);
    net.process();
    assert_eq!(net.pipeline(r).unwrap().air.total_moles(), 0.0);
    // Open: 70 L + 200 + 200 + 70 L, the 54 mol spread over 540 L.
    net.set_valve_open(valve, true);
    assert!(close(net.pipeline(r).unwrap().air.gas.get(GasId::O2), 7.0));
    assert!(close(net.air(valve, 1).unwrap().gas.get(GasId::O2), 20.0));
    assert!(close(net.total_moles(), 54.0));
}

#[test]
fn connector_reconciles_the_attached_portable() {
    let mut net = Pipenet::default();
    let connector = net.add(unary(0, 0, 4, Custom::Connector { attached: None }, 0.0));
    let p = net.add(pipe(1, 0, 3, BLUE));
    net.build();
    let canister = PipeAir::with_gas(o2(100.0), 2000.0);
    net.attach(connector, canister).unwrap();
    // 2000 L canister + 0 L connector + 70 L pipe.
    let line = net.pipeline_of(p, 0).unwrap();
    let share = 100.0 * 70.0 / 2070.0;
    assert!(close(
        net.pipeline(line).unwrap().air.gas.get(GasId::O2),
        share
    ));
    let back = net.detach(connector).unwrap();
    assert!(close(back.gas.get(GasId::O2), 100.0 - share));
}

#[test]
fn layer_manifold_bridges_layers() {
    let mut net = Pipenet::default();
    let l2 = net.add(pipe(0, 0, 2, BLUE));
    let manifold = net.add(DeviceSpec {
        entity: Entity::PLACEHOLDER,
        pos: pos(1, 0),
        init_dirs: 4 | 8,
        node_dirs: Vec::new(),
        layer: 3,
        color: BLUE,
        flags: flags::ALL_LAYER
            | flags::DEFAULT_LAYER_ONLY
            | flags::CARDINAL_AUTONORMALIZE
            | flags::BRIDGE,
        kind: DeviceKind::LayerManifold,
        port_volumes: Vec::new(),
        custom: Custom::None,
    });
    let l4 = net.add(pipe(2, 0, 4, BLUE));
    net.build();
    let line = net.pipeline_of(manifold, 0).unwrap();
    assert_eq!(net.pipeline_of(l2, 0), Some(line));
    assert_eq!(net.pipeline_of(l4, 0), Some(line));
    assert_eq!(net.pipeline(line).unwrap().air.volume, 70.0 + 200.0 + 70.0);
}

#[test]
fn removing_a_machine_relocates_its_gas() {
    let mut net = Pipenet::default();
    let vent = net.add(unary(0, 0, 4, Custom::None, PORT_VOLUME));
    let p = net.add(pipe(1, 0, 3, BLUE));
    let lone = net.add(unary(5, 5, 4, Custom::None, PORT_VOLUME));
    net.build();
    net.air_mut(vent, 0).unwrap().merge(&o2(10.0));
    net.air_mut(lone, 0).unwrap().merge(&o2(3.0));
    // Connected: the port's gas goes into the pipeline; unconnected: onto the tile.
    assert!(net.remove(vent).is_none());
    let line = net.pipeline_of(p, 0).unwrap();
    assert!(close(
        net.pipeline(line).unwrap().air.gas.get(GasId::O2),
        10.0
    ));
    assert!(net.pipeline(line).unwrap().others.is_empty());
    assert!(close(net.remove(lone).unwrap().get(GasId::O2), 3.0));
}

/// A heat exchanging binary pipe along an axis (or a diagonal bend), 70 L.
fn he_pipe(x: i32, y: i32, init_dirs: u8, he: HeatExchange) -> DeviceSpec {
    DeviceSpec {
        entity: Entity::PLACEHOLDER,
        pos: pos(x, y),
        init_dirs,
        node_dirs: default_node_dirs(2, init_dirs),
        layer: 3,
        color: COLOR_OMNI,
        flags: flags::CARDINAL_AUTONORMALIZE,
        kind: DeviceKind::Pipe {
            volume: 70.0,
            smart: false,
            he,
        },
        port_volumes: Vec::new(),
        custom: Custom::None,
    }
}

#[test]
fn heat_exchanging_pipes_connect_only_through_a_junction() {
    let mut net = Pipenet::default();
    // plain (0,0) - junction (1,0), plain side west - HE (2,0) - HE bend (3,0) north-west -
    // HE (3,1); a plain pipe (4,0) east of the bend never joins it, an HE pipe (0,1) never joins
    // the plain one.
    let plain = net.add(pipe(0, 0, 3, COLOR_OMNI));
    let junction = net.add(he_pipe(
        1,
        0,
        4 | 8,
        HeatExchange::Junction { plain_side: 8 },
    ));
    let he = net.add(he_pipe(2, 0, 4 | 8, HeatExchange::Pipe));
    let bend = net.add(he_pipe(3, 0, 1 | 8, HeatExchange::Pipe));
    let up = net.add(he_pipe(3, 1, 1 | 2, HeatExchange::Pipe));
    let stray = net.add(pipe(4, 0, 3, COLOR_OMNI));
    let he_by_plain = net.add(he_pipe(0, 1, 1 | 2, HeatExchange::Pipe));
    net.build();
    let line = net.pipeline_of(plain, 0).unwrap();
    for id in [junction, he, bend, up] {
        assert_eq!(net.pipeline_of(id, 0), Some(line));
    }
    assert_ne!(net.pipeline_of(stray, 0), Some(line));
    assert_ne!(net.pipeline_of(he_by_plain, 0), Some(line));
}

#[test]
fn temperature_interact_follows_tg_conduction() {
    let mut net = Pipenet::default();
    let ids: Vec<_> = (0..2)
        .map(|x| net.add(he_pipe(x, 0, 4 | 8, HeatExchange::Pipe)))
        .collect();
    net.build();
    let line = net.pipeline_of(ids[0], 0).unwrap();
    let mut hot = o2(100.0);
    hot.temperature = 1000.0;
    net.lines[line.0 as usize].as_mut().unwrap().air.gas = hot;
    let total = net.pipeline(line).unwrap().air.heat_capacity();
    // Space: 2.7 K, `HEAT_CAPACITY_VACUUM`; half of the pipeline (70 of 140 L)
    // touches it with `WINDOW_HEAT_TRANSFER_COEFFICIENT`.
    let partial = total * 0.5;
    let space = crate::gas::HEAT_CAPACITY_VACUUM;
    let heat = 0.1 * (2.7 - 1000.0) * (partial * space / (partial + space));
    let turf_delta = net
        .temperature_interact(ids[0], 70.0, 0.1, 2.7, space)
        .unwrap();
    let pl = net.pipeline(line).unwrap();
    assert!(close(pl.air.gas.temperature, 1000.0 + heat / total));
    assert!(close(turf_delta, -heat / space));
    assert!(pl.update);
    assert!(pl.air.gas.temperature < 1000.0);
}

#[test]
fn reconfiguring_a_smart_pipe_keeps_its_gas_and_connections() {
    let mut net = Pipenet::default();
    let ids: Vec<_> = (0..3).map(|x| net.add(pipe(x, 0, 3, BLUE))).collect();
    net.build();
    let line = net.pipeline_of(ids[0], 0).expect("one pipeline");
    net.pipeline_air_mut(line).expect("line").merge(&o2(30.0));
    let before = net.total_moles();
    // The middle pipe keeps east-west (its connections) and drops north-south, as
    // `try_smart_reconfiguration` does to a straight pipe crossed by a new one.
    let mid = net
        .reconnect(ids[1], 4 | 8, default_node_dirs(4, 4 | 8), 3, BLUE)
        .expect("reconnected");
    assert!(
        close(net.total_moles(), before),
        "{} vs {before}",
        net.total_moles()
    );
    let line = net.pipeline_of(mid, 0).expect("still in a pipeline");
    assert_eq!(net.pipeline_of(ids[0], 0), Some(line));
    assert_eq!(net.pipeline_of(ids[2], 0), Some(line));
    assert_eq!(net.device(mid).expect("mid").init_dirs, 4 | 8);
}

/// A multi-deck adapter at `(x, y, z)` facing `dir` on `layer`.
fn multiz(x: i32, y: i32, z: u16, dir: u8, layer: u8, color: [u8; 3]) -> DeviceSpec {
    DeviceSpec {
        entity: Entity::PLACEHOLDER,
        pos: TilePos::new(x, y, z),
        init_dirs: dir,
        node_dirs: vec![dir, 0, 0],
        layer,
        color,
        flags: 0,
        kind: DeviceKind::Pipe {
            volume: 0.0,
            smart: false,
            he: HeatExchange::None,
        },
        port_volumes: Vec::new(),
        custom: Custom::MultiZ,
    }
}

#[test]
fn multi_deck_adapters_join_pipelines_across_linked_levels_only() {
    let build = |linked: bool, upper_layer: u8| {
        let mut net = Pipenet::default();
        if linked {
            net.link_levels(1, 2);
        }
        // Level 1: a pipe south of the adapter; level 2: the same.
        let lower = net.add(multiz(5, 5, 1, 2, 3, BLUE));
        let below = net.add(DeviceSpec {
            pos: TilePos::new(5, 4, 1),
            ..pipe(0, 0, 3, BLUE)
        });
        let upper = net.add(multiz(5, 5, 2, 2, upper_layer, BLUE));
        let above = net.add(DeviceSpec {
            pos: TilePos::new(5, 4, 2),
            ..pipe(0, 0, upper_layer, BLUE)
        });
        net.build();
        net.process();
        let same = net.pipeline_of(below, 0) == net.pipeline_of(above, 0);
        let nodes = net.device(lower).unwrap().nodes.clone();
        (same, nodes, upper)
    };
    let (same, nodes, upper) = build(true, 3);
    assert!(same, "one pipeline through both decks");
    assert_eq!(nodes[1], Some(upper));
    assert_eq!(nodes[2], None, "nothing below level 1");
    assert!(!build(false, 3).0, "levels not stacked");
    assert!(!build(true, 2).0, "another layer");
}
