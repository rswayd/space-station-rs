//! Damage of gas containers: canisters (`portable/canister.dm`, `portable_atmospherics.dm`) and
//! stationary tanks (`components/tank.dm`), with tg's `gas_leaker` component, the
//! `volatile_gas_storage` element and the tank's `crackable` cracks.
//!
//! - A canister's gas over `pressure_limit` or `temp_limit` hurts it every fire
//!   (`take_atmos_damage`).
//! - Below 90% integrity a container leaks: `release_gas_to(turf, its pressure, rate)` with
//!   `rate = (1 - integrity / max) * leak_rate` (canisters 0.01, tanks 0.05).
//! - Breaking (`atom_break`) blows up a container holding over 5000 kPa (light impact up to
//!   range 9 at 100 MPa, `volatile_gas_storage`); a broken canister empties onto its tile,
//!   lets go of its port and tank and loses its density (`canister_break`).
// shortcut: cracks are drawn unrotated (tg turns each by a multiple of 10 degrees).

use bevy::prelude::*;
use ss13_atmos::Atmos;
use ss13_atmos::pipenet::Pipenet;
use ss13_atmos::pipenet::air::{Mix, PipeAir, release_moles};
use ss13_mapload::{DmObj, IconIndex};
use ss13_shared::{Appearance, Overlay, TilePos};
use ss13_world::Density;

use super::portable::{Canister, PortableAtmos};
use super::tank::StationaryTank;
use crate::health::DamageType;
use crate::obj_damage::{Integrity, take_damage};

/// `portable_atmospherics` `temp_limit`, `pressure_limit`.
pub const TEMP_LIMIT: f64 = 10_000.0;
pub const PRESSURE_LIMIT: f64 = 500_000.0;
/// `gas_leaker` `integrity_leak_percent` and the `leak_rate`s of canisters and tanks.
pub const LEAK_INTEGRITY: f32 = 0.9;
pub const CANISTER_LEAK_RATE: f64 = 0.01;
pub const TANK_LEAK_RATE: f64 = 0.05;
/// `volatile_gas_storage` defaults.
pub const MIN_EXPLOSIVE_PRESSURE: f64 = 5000.0;
pub const MAX_EXPLOSIVE_PRESSURE: f64 = 100_000.0;
pub const MAX_EXPLOSIVE_FORCE: f64 = 9.0;
const CRACKS: &str = "icons/effects/cracks.dmi";

/// `take_atmos_damage`: burn damage of a canister whose gas is too hot or pressurized, `None`
/// when both are within its limits.
pub fn atmos_damage(
    temperature: f64,
    pressure: f64,
    temp_limit: f64,
    pressure_limit: f64,
) -> Option<f32> {
    let temp = temperature / temp_limit;
    let pres = pressure / pressure_limit;
    (temp > 1.0 || pres > 1.0).then(|| (temp * pres).clamp(5.0, 50.0) as f32)
}

/// `volatile_gas_storage/Break`: the light impact range of a container breaking at `pressure`.
pub fn explosive_force(pressure: f64) -> Option<i32> {
    let pressure = pressure.min(MAX_EXPLOSIVE_PRESSURE);
    (pressure >= MIN_EXPLOSIVE_PRESSURE)
        .then(|| (pressure / MAX_EXPLOSIVE_PRESSURE * MAX_EXPLOSIVE_FORCE).ceil() as i32)
}

/// `gas_leaker/process_leak`: the leak rate at this integrity, `None` above 90%.
pub fn leak_rate(integrity: Integrity, max: f32, leak_rate: f64) -> Option<f64> {
    if max <= 0.0 || integrity.current > max * LEAK_INTEGRITY {
        return None;
    }
    Some((1.0 - f64::from(integrity.current / max)) * leak_rate)
}

/// Releases `rate` of `air` toward its own pressure into the tile (`release_gas_to`).
fn leak(air: &mut PipeAir, atmos: &mut Atmos, pos: TilePos, rate: f64) {
    let Some(turf) = atmos.air(pos).cloned() else {
        return;
    };
    let turf = ss13_atmos::pipenet::air::TurfMix(&turf);
    let target = air.pressure();
    let Some(moles) = release_moles(&*air, &turf as &dyn Mix, target, rate, None) else {
        return;
    };
    if let Some(removed) = air.remove(moles) {
        atmos.merge_air(pos, &removed);
    }
}

/// Every fire: canisters take their atmos damage and damaged containers leak.
pub fn process_container_damage(world: &mut World) {
    let mut q = world.query::<(Entity, &TilePos, &DmObj, &Integrity, Has<Canister>)>();
    let found: Vec<(Entity, TilePos, f32, Integrity, bool)> = q
        .iter(world)
        .filter(|(e, ..)| {
            world.get::<Canister>(*e).is_some() || world.get::<StationaryTank>(*e).is_some()
        })
        .map(|(e, &p, obj, &i, c)| (e, p, obj.max_integrity, i, c))
        .collect();
    for (e, pos, max, integrity, canister) in found {
        if integrity.broken && canister {
            continue;
        }
        // `canister/take_atmos_damage`: nothing while the shielding is powered.
        let limits = world
            .get::<Canister>(e)
            .filter(|c| !c.shielding)
            .map(|c| (c.temp_limit, c.pressure_limit));
        if let Some((temp_limit, pressure_limit)) = limits {
            let state = container_air(world, e).map(|a| (a.temperature(), a.pressure()));
            if let Some(damage) =
                state.and_then(|(t, p)| atmos_damage(t, p, temp_limit, pressure_limit))
            {
                take_damage(world, e, damage, DamageType::Burn);
            }
        }
        let Some(integrity) = world.get::<Integrity>(e).copied() else {
            continue;
        };
        let rate = if canister {
            CANISTER_LEAK_RATE
        } else {
            TANK_LEAK_RATE
        };
        // `process_machine`: a broken machine does not leak.
        if integrity.broken {
            continue;
        }
        if let Some(rate) = leak_rate(integrity, max, rate) {
            with_container_air(world, e, |air, atmos| leak(air, atmos, pos, rate));
        }
    }
}

/// The mixture a container holds: a canister's own, a tank's group air.
fn container_air(world: &World, e: Entity) -> Option<PipeAir> {
    let net = world.resource::<Pipenet>();
    if let Some(p) = world.get::<PortableAtmos>(e) {
        return p.air(net).cloned();
    }
    let tank = world.get::<StationaryTank>(e)?;
    net.tank_group(tank.group).map(|g| g.air.clone())
}

fn with_container_air(world: &mut World, e: Entity, f: impl FnOnce(&mut PipeAir, &mut Atmos)) {
    if let Some(port) = world.get::<PortableAtmos>(e).map(|p| p.port) {
        match port {
            Some((_, id)) => world.resource_scope(|world, mut net: Mut<Pipenet>| {
                if let Some(air) = net.attached_mut(id) {
                    f(air, &mut world.resource_mut::<Atmos>());
                }
            }),
            // The gas is no part of the look (see `process_portables`).
            None => world.resource_scope(|world, mut atmos: Mut<Atmos>| {
                if let Some(mut p) = world.get_mut::<PortableAtmos>(e)
                    && let Some(air) = p.bypass_change_detection().air.as_mut()
                {
                    f(air, &mut atmos);
                }
            }),
        }
        return;
    }
    let Some(group) = world.get::<StationaryTank>(e).map(|t| t.group) else {
        return;
    };
    world.resource_scope(|world, mut net: Mut<Pipenet>| {
        if let Some(g) = net.tank_group_mut(group) {
            f(&mut g.air, &mut world.resource_mut::<Atmos>());
        }
    });
}

/// `atom_break` of a gas container (from [`crate::obj_damage`]): the volatile explosion, then
/// `canister_break` for canisters. True when `target` is one.
pub fn atom_break(world: &mut World, target: Entity) -> bool {
    let canister = world.get::<Canister>(target).is_some();
    if !canister && world.get::<StationaryTank>(target).is_none() {
        return false;
    }
    let Some(mut integrity) = world.get_mut::<Integrity>(target) else {
        return true;
    };
    if integrity.broken {
        return true;
    }
    integrity.broken = true;
    let Some(&pos) = world.get::<TilePos>(target) else {
        return true;
    };
    // `AtmosComponentBreak` reads a tank's port mixtures, which reconcile to its air.
    let pressure = container_air(world, target).map_or(0.0, |a| a.pressure());
    if let Some(force) = explosive_force(pressure) {
        crate::events::explosion::explosion(world, pos, 0, 0, force, 0);
    }
    if canister && world.get_entity(target).is_ok() {
        canister_break(world, target, pos);
    }
    true
}

/// `canister_break`: off its port, all the gas onto the tile, the held tank dropped, no
/// density, a spray sound.
pub fn canister_break(world: &mut World, canister: Entity, pos: TilePos) {
    super::portable::disconnect(world, canister);
    with_container_air(world, canister, |air, atmos| {
        let all = air.total_moles();
        if let Some(gas) = air.remove(all) {
            atmos.merge_air(pos, &gas);
        }
    });
    let held = world
        .get_mut::<PortableAtmos>(canister)
        .and_then(|mut p| p.holding.take());
    if let Some(tank) = held {
        crate::storage::take_out(world, tank, pos);
    }
    world.entity_mut(canister).remove::<Density>();
    if let Some(mut c) = world.get_mut::<Canister>(canister) {
        c.broken = true;
    }
    // `update_icon_state` sets `"[base_icon_state]-1"`, which is `"-1"` (no base state) and
    // not in the GAGS icon, so BYOND keeps the default state; only the `broken` overlay shows.
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(pos, "sound/effects/spray.ogg", 10.0, true).extrarange(-3.0),
    );
}

/// `canister/on_deconstruction`: breaks first if whole, then iron sheets (10 taken apart, 5
/// smashed).
pub fn deconstruct(world: &mut World, target: Entity, disassembled: bool) -> bool {
    if super::pipe_scrubber::deconstruct(world, target) {
        return true;
    }
    if world.get::<Canister>(target).is_none() {
        return tank_deconstruct(world, target, disassembled);
    }
    let Some(&pos) = world.get::<TilePos>(target) else {
        return true;
    };
    if !world.get::<Integrity>(target).is_some_and(|i| i.broken) {
        canister_break(world, target, pos);
    }
    let sheets = if disassembled { 10 } else { 5 };
    crate::construction::spawn_stack(world, "/obj/item/stack/sheet/iron", pos, sheets);
    true
}

/// `tank/on_deconstruction`: `assume_air(air_contents)`, which is the whole group's shared
/// mixture, onto the tile, and the tank out of the pipe graph; cut open (`disassembled`), it
/// leaves a plated, unsecured frame.
fn tank_deconstruct(world: &mut World, target: Entity, disassembled: bool) -> bool {
    if world.get::<StationaryTank>(target).is_none() {
        // Pipes and devices (a vent worn out by overclocking, one wrenched off).
        return super::fitting::deconstruct(world, target);
    }
    let Some(&pos) = world.get::<TilePos>(target) else {
        return true;
    };
    with_container_air(world, target, |air, atmos| {
        let all = air.total_moles();
        if let Some(gas) = air.remove(all) {
            atmos.merge_air(pos, &gas);
        }
    });
    if let Some(device) = world.get::<super::AtmosDevice>(target).copied() {
        let volume = world
            .get::<ss13_mapload::DmAtmosDevice>(target)
            .map_or(0.0, |d| d.volume);
        // Its share of the (now emptied) shared air leaves with it, then its ports' gas.
        let released = world.resource_scope(|_, mut net: Mut<Pipenet>| {
            let share = net.leave_tank_group(device.id, volume);
            let ports = net.remove(device.id);
            [share, ports]
        });
        for gas in released.into_iter().flatten() {
            world.resource_mut::<Atmos>().merge_air(pos, &gas);
        }
    }
    if disassembled {
        super::tank_frame::spawn_frame(world, pos, true);
    }
    true
}

/// `crackable`: a damaged tank shows cracks, more the lower its integrity (one of
/// `crack1`..`crack10` per lost tenth).
pub fn tank_cracks(
    icons: Option<Res<IconIndex>>,
    mut tanks: Query<
        (&Integrity, &DmObj, &mut Appearance),
        (With<StationaryTank>, Changed<Integrity>),
    >,
) {
    let Some(icons) = icons else {
        return;
    };
    let crack_ids: Vec<_> = (1..=10)
        .filter_map(|i| icons.get(CRACKS, &format!("crack{i}")))
        .collect();
    for (integrity, obj, mut appearance) in &mut tanks {
        appearance
            .overlays
            .retain(|o| !crack_ids.contains(&o.icon_state));
        if obj.max_integrity <= 0.0 {
            continue;
        }
        let lost = 1.0 - integrity.current / obj.max_integrity;
        let count = ((lost * 10.0).ceil() as usize).min(crack_ids.len());
        appearance
            .overlays
            .extend(crack_ids.iter().take(count).map(|&id| Overlay::new(id)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_follow_tg() {
        // 600 MPa at room temperature: 1.2 * 0.029 = 0.035 -> at least 5.
        let damage = |t, p| atmos_damage(t, p, TEMP_LIMIT, PRESSURE_LIMIT);
        assert_eq!(damage(293.0, 600_000.0), Some(5.0));
        assert_eq!(damage(293.0, 100_000.0), None);
        assert_eq!(damage(40_000.0, 1_000_000.0), Some(8.0));
        // A canister of the best quality (+20%) holds 600 000 kPa.
        assert_eq!(atmos_damage(293.0, 600_000.0, TEMP_LIMIT, 600_000.0), None);
        assert_eq!(explosive_force(4999.0), None);
        assert_eq!(explosive_force(23_000.0), Some(3));
        assert_eq!(explosive_force(1e9), Some(9));
        let at = |current| Integrity {
            current,
            broken: false,
        };
        assert_eq!(leak_rate(at(300.0), 300.0, 0.01), None);
        assert!((leak_rate(at(150.0), 300.0, 0.01).unwrap() - 0.005).abs() < 1e-12);
    }
}
