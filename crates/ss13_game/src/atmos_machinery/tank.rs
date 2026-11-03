//! Stationary pressure tanks: tg `/obj/machinery/atmospherics/components/tank`
//! (`machinery/components/tank.dm`). Tanks touching each other are one merge group sharing a
//! single mixture (`/datum/merger` `stationary_tanks`) of their summed volume, which joins the
//! reconciliation of every pipeline a member's open port is in ([`Pipenet::add_tank_group`]).
//! A mapped tank opens a port in its `dir`, closed where it faces another member. Each member
//! reacts the shared air every fire and takes damage while it is over `max_pressure`; a wrench
//! opens or closes the port toward the user.
// shortcut: no gas visuals behind the window (tg alpha-masks the gas overlays), no pipe
// underlays at the ports; a group cut in two by a removed tank stays one group (tg's merger
// splits it and hands each part a share of the gas).

use std::collections::{HashMap, HashSet};

use bevy::prelude::*;
use ss13_atmos::GasId;
use ss13_atmos::gas::{N2STANDARD, O2STANDARD, R_IDEAL_GAS_EQUATION, T20C};
use ss13_atmos::pipenet::air::PipeAir;
use ss13_atmos::pipenet::{CARDINALS, DeviceId, DeviceSpec, Pipenet, default_node_dirs};
use ss13_atmos::reactions::{Exposures, Holder, NO_REACTION};
use ss13_mapload::{DmAtmosDevice, IconIndex};
use ss13_shared::{Appearance, Dir, Overlay, TilePos};

use super::AtmosDevice;
use crate::health::DamageType;
use crate::obj_damage::{is_under, take_damage};

pub const TANK: &str = "/obj/machinery/atmospherics/components/tank";
const AIR_TANK: &str = "/obj/machinery/atmospherics/components/tank/air";
const CRACKS_DMI: &str = "icons/effects/cracks.dmi";
const MISC_DMI: &str = "icons/obj/pipes_n_cables/stationary_canisters_misc.dmi";
const BREAKING_SOUNDS: [&str; 3] = [
    "sound/effects/structure_stress/pop1.ogg",
    "sound/effects/structure_stress/pop2.ogg",
    "sound/effects/structure_stress/pop3.ogg",
];

/// One member tank.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct StationaryTank {
    pub group: u32,
    /// `max_pressure`, kPa.
    pub max_pressure: f64,
}

/// `fill_to_pressure`: moles of `gas` that bring `volume` litres at T20C to `max_pressure`
/// times `margin`.
pub fn fill_moles(max_pressure: f64, margin: f64, volume: f64) -> f64 {
    max_pressure * margin * volume / (R_IDEAL_GAS_EQUATION * T20C)
}

/// A tank's own `air_contents` after `Initialize`.
pub fn initial_air(dm: &DmAtmosDevice) -> PipeAir {
    let mut air = PipeAir::new(dm.volume);
    air.gas.temperature = T20C;
    let percent = dm.starting_pressure_percent;
    if percent <= 0.0 {
        return air;
    }
    if is_under(&dm.path, AIR_TANK) {
        let o2 = fill_moles(dm.max_pressure, O2STANDARD * percent, dm.volume);
        let n2 = fill_moles(dm.max_pressure, N2STANDARD * percent, dm.volume);
        air.gas.add(GasId::O2, o2);
        air.gas.add(GasId::N2, n2);
    } else if let Some(gas) = GasId::parse(&dm.gas_type) {
        air.gas
            .add(gas, fill_moles(dm.max_pressure, percent, dm.volume));
    }
    air
}

/// The node directions of a tank whose open ports are `ports` (`QUATERNARY`).
pub fn tank_nodes(ports: u8) -> Vec<u8> {
    default_node_dirs(4, ports)
}

/// Directions from `pos` to the tanks in `tanks` next to it.
fn neighbour_dirs(pos: TilePos, tanks: &HashSet<TilePos>) -> u8 {
    CARDINALS
        .into_iter()
        .filter(|&d| {
            let (dx, dy) = Dir::from_bits(d).expect("cardinal").offset();
            tanks.contains(&TilePos::new(pos.x + dx, pos.y + dy, pos.z))
        })
        .fold(0, |m, d| m | d)
}

/// The open ports of a mapped tank: its `dir` (`set_portdir_relative(dir, TRUE)` then
/// `set_init_directions`), closed toward another member (`merger_refresh_complete`).
pub fn mapped_ports(pos: TilePos, dir: u8, tanks: &HashSet<TilePos>) -> u8 {
    let dir = if matches!(dir, 1 | 2 | 4 | 8) { dir } else { 2 };
    dir & !neighbour_dirs(pos, tanks)
}

/// The merge groups: tanks connected through cardinal neighbours.
pub fn merge_groups(tanks: &[TilePos]) -> Vec<Vec<usize>> {
    let at: HashMap<TilePos, usize> = tanks.iter().enumerate().map(|(i, &p)| (p, i)).collect();
    let mut seen = vec![false; tanks.len()];
    let mut out = Vec::new();
    for start in 0..tanks.len() {
        if seen[start] {
            continue;
        }
        seen[start] = true;
        let mut group = vec![start];
        let mut i = 0;
        while i < group.len() {
            let p = tanks[group[i]];
            i += 1;
            for d in CARDINALS {
                let (dx, dy) = Dir::from_bits(d).expect("cardinal").offset();
                if let Some(&n) = at.get(&TilePos::new(p.x + dx, p.y + dy, p.z))
                    && !seen[n]
                {
                    seen[n] = true;
                    group.push(n);
                }
            }
        }
        out.push(group);
    }
    out
}

/// After the tanks are in the pipe graph: their groups, each with the merged air of its
/// members (`merger_refresh_complete` merges every joiner into the origin's mixture).
pub fn init_tank_groups(
    world: &mut World,
    net: &mut Pipenet,
    tanks: &[(Entity, DeviceId, TilePos, DmAtmosDevice)],
) {
    let positions: Vec<TilePos> = tanks.iter().map(|t| t.2).collect();
    for group in merge_groups(&positions) {
        let mut air = PipeAir::new(0.0);
        air.gas.temperature = T20C;
        let mut members = Vec::new();
        for &i in &group {
            let (_, id, _, dm) = &tanks[i];
            let own = initial_air(dm);
            if members.is_empty() {
                air = own;
            } else {
                air.merge(&own.gas);
                air.volume += own.volume;
            }
            members.push(*id);
        }
        let g = net.add_tank_group(air, members);
        for &i in &group {
            let (e, _, _, dm) = &tanks[i];
            world.entity_mut(*e).insert(StationaryTank {
                group: g,
                max_pressure: dm.max_pressure,
            });
        }
    }
}

/// `process_atmos` of every member: the shared air reacts (once per member, as in tg), and
/// over `max_pressure` the tank takes 0.1 brute with a 40% chance of a stress pop.
pub fn process_tanks(world: &mut World) {
    let mut q = world.query::<(Entity, &AtmosDevice, &StationaryTank, &TilePos)>();
    let tanks: Vec<(Entity, DeviceId, StationaryTank, TilePos)> = q
        .iter(world)
        .map(|(e, d, t, &p)| (e, d.id, t.clone(), p))
        .collect();
    for (e, id, tank, pos) in tanks {
        let pressure = {
            let mut net = world.resource_mut::<Pipenet>();
            let Some(group) = net.tank_group_mut(tank.group) else {
                continue;
            };
            let volume = group.air.volume;
            let mut exposures = Exposures::default();
            let reacted = group.air.gas.react(Holder::Other, volume, &mut exposures) != NO_REACTION;
            let pressure = group.air.pressure();
            if reacted {
                net.mark_parents(id);
            }
            pressure
        };
        if pressure > tank.max_pressure {
            take_damage(world, e, 0.1, DamageType::Brute);
            let mut rng = world.resource_mut::<crate::combat::CombatRng>();
            if rng.chance(40.0) {
                let sound = BREAKING_SOUNDS[rng.range(0, 2) as usize];
                crate::sound::playsound(
                    world,
                    crate::sound::PlaySound::new(pos, sound, 30.0, true),
                );
            }
        }
    }
}

/// `update_overlays`: the window (`window-bg`, the glass `window-fg`) and a knob per open port.
pub fn tank_looks(
    icons: Option<Res<IconIndex>>,
    net: Res<Pipenet>,
    mut tanks: Query<(&AtmosDevice, &mut Appearance), (With<StationaryTank>, Changed<AtmosDevice>)>,
) {
    let Some(icons) = icons else {
        return;
    };
    for (device, mut appearance) in &mut tanks {
        let ports = net.device(device.id).map_or(0, |d| d.init_dirs);
        let mut overlays: Vec<Overlay> = ["window-bg", "window-fg"]
            .into_iter()
            .filter_map(|s| icons.get(MISC_DMI, s))
            .map(Overlay::new)
            .collect();
        if let Some(knob) = icons.get(MISC_DMI, "knob") {
            for d in CARDINALS.into_iter().filter(|d| ports & d != 0) {
                overlays.push(Overlay {
                    dir: Dir::from_bits(d),
                    ..Overlay::new(knob)
                });
            }
        }
        // Cracks (`damage::tank_cracks`) stay.
        let cracks: Vec<Overlay> = appearance
            .overlays
            .iter()
            .filter(|o| icons.get(CRACKS_DMI, "crack1").is_some() && is_crack(&icons, o))
            .cloned()
            .collect();
        overlays.extend(cracks);
        appearance.overlays = overlays;
    }
}

fn is_crack(icons: &IconIndex, o: &Overlay) -> bool {
    (1..=10).any(|i| icons.get(CRACKS_DMI, &format!("crack{i}")) == Some(o.icon_state))
}

/// `toggle_side_port(dir)` of a tank's `wrench_act` (the port toward the user).
pub fn toggle_port(world: &mut World, tank: Entity, dir: u8) -> bool {
    let Some(device) = world.get::<AtmosDevice>(tank).copied() else {
        return false;
    };
    let Some(&pos) = world.get::<TilePos>(tank) else {
        return false;
    };
    let mut net = world.resource_mut::<Pipenet>();
    let Some(d) = net.device(device.id) else {
        return false;
    };
    let ports = d.init_dirs ^ dir;
    let spec = DeviceSpec {
        entity: tank,
        pos,
        init_dirs: ports,
        node_dirs: tank_nodes(ports),
        layer: d.layer,
        color: d.color,
        flags: d.flags,
        kind: d.kind,
        port_volumes: vec![ss13_atmos::pipenet::PORT_VOLUME; 4],
        custom: d.custom.clone(),
    };
    let Some(new) = net.retarget_tank(device.id, spec) else {
        return false;
    };
    world
        .entity_mut(tank)
        .insert(AtmosDevice { id: new, ..device });
    true
}

/// `examine`: the wrench hint and the safe pressure on the sticker.
pub fn examine_lines(world: &World, tank: Entity) -> Vec<String> {
    let (Some(t), Some(device)) = (
        world.get::<StationaryTank>(tank),
        world.get::<AtmosDevice>(tank),
    ) else {
        return Vec::new();
    };
    let open = world
        .resource::<Pipenet>()
        .device(device.id)
        .is_some_and(|d| d.init_dirs != 0);
    vec![
        if open {
            "Порт для трубы можно переставить или закрыть гаечным ключом."
        } else {
            "Гаечным ключом можно открыть порт для трубы."
        }
        .to_owned(),
        format!(
            "Голографическая наклейка гласит, что максимальное безопасное давление: {}.",
            siunit_pressure(t.max_pressure)
        ),
    ]
}

/// `siunit_pressure(kpa, 0)`: kPa scaled to the nearest SI prefix, no decimals.
pub(crate) fn siunit_pressure(kpa: f64) -> String {
    let pa = kpa * 1000.0;
    let (value, prefix) = if pa >= 1e9 {
        (pa / 1e9, "G")
    } else if pa >= 1e6 {
        (pa / 1e6, "M")
    } else if pa >= 1e3 {
        (pa / 1e3, "k")
    } else {
        (pa, "")
    };
    format!("{} {prefix}Pa", value.round())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pos(x: i32, y: i32) -> TilePos {
        TilePos::new(x, y, 1)
    }

    /// Adjacent tanks merge; a mapped tank's port in its dir closes toward a member.
    #[test]
    fn groups_and_ports() {
        let tanks = [pos(1, 1), pos(2, 1), pos(3, 1), pos(10, 10)];
        let groups = merge_groups(&tanks);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].len(), 3);
        let set: HashSet<TilePos> = tanks.into_iter().collect();
        // Facing east into the next tank: closed; facing north: open.
        assert_eq!(mapped_ports(pos(1, 1), 4, &set), 0);
        assert_eq!(mapped_ports(pos(1, 1), 1, &set), 1);
        assert_eq!(tank_nodes(1), [1, 0, 0, 0]);
    }

    /// `fill_to_pressure(gas, 0.5)` of a 2500 L tank: half of 46000 kPa; air splits by
    /// O2STANDARD / N2STANDARD.
    #[test]
    fn initial_fill_follows_tg() {
        let dm = |path: &str, gas: &str| DmAtmosDevice {
            path: path.into(),
            volume: 2500.0,
            max_pressure: 46000.0,
            starting_pressure_percent: 0.5,
            gas_type: gas.into(),
            ..default()
        };
        let o2 = initial_air(&dm(
            "/obj/machinery/atmospherics/components/tank/oxygen",
            "o2",
        ));
        assert!((o2.pressure() - 23000.0).abs() < 1e-6);
        let air = initial_air(&dm(AIR_TANK, ""));
        assert!((air.pressure() - 23000.0).abs() < 1e-6);
        assert!((air.gas.get(GasId::O2) / air.total_moles() - O2STANDARD).abs() < 1e-9);
        assert_eq!(siunit_pressure(46000.0), "46 MPa");
    }
}
