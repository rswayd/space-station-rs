//! `emp_act` of power things: cells (`power_store`), APCs, SMES, light fixtures, every powered
//! machine's energy surge (`/obj/machinery/emp_act`), cell chargers and rechargers, canisters'
//! cells and shielding, exosuits.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_shared::chat::Span;

use super::{
    EmpAct, EmpRng, EmpSystems, Emped, STANDARD_BATTERY_CHARGE, STANDARD_CELL_CHARGE, emp_acts,
    emped_expired, path_of, protection_of, set_emped, sparks,
};
use crate::atmos_machinery::portable::Canister;
use crate::health::DamageType;
use crate::mecha::core::{ItemCell, Mech, MechaDb};
use crate::power::chargers::{CellCharger, Recharger};
use crate::power::model::{CHANNEL_AUTO_ON, CHANNEL_OFF};
use crate::power::{Apc, LightFixture, LightProcessing, PowerMachine, Smes, Unpowered};

/// `/obj/machinery/emp_act`: `use_energy(7.5 KILO JOULES / severity)`.
const MACHINE_EMP_ENERGY: f64 = 7_500.0;
/// `addtimer(CALLBACK(src, PROC_REF(reset), APC_RESET_EMP), 60 SECONDS)`.
const APC_RESET_SECS: f64 = 60.0;
/// `input_level_max`, `output_level_max` (200 kW with stock parts).
// shortcut: the SMES's `power_coefficient` from its capacitors is not modelled; stock levels.
const SMES_LEVEL_MAX: f64 = 200_000.0;
/// `MECH_EMP_DAMAGE_LOWER`, `MECH_EMP_DAMAGE_UPPER`.
const MECH_EMP_DAMAGE: (i64, i64) = (100, 180);
/// `addtimer(restore_equipment, 3 SECONDS)`.
const MECH_EQUIPMENT_SECS: f64 = 3.0;

/// `emp_damage_modifier` of the cell types (1 for the rest). Most specific first.
const CELL_EMP_MODIFIER: &[(&str, f64)] = &[
    ("/obj/item/stock_parts/power_store/cell/emproof", 0.0),
    ("/obj/item/stock_parts/power_store/cell/infinite", 0.0),
    ("/obj/item/stock_parts/power_store/cell/ethereal", 0.0),
    ("/obj/item/stock_parts/power_store/cell/pulse", 0.5),
    ("/obj/item/stock_parts/power_store/cell/ninja", 3.0),
    ("/obj/item/stock_parts/power_store/cell/high", 3.0),
    ("/obj/item/stock_parts/power_store/cell/lead", 4.0),
    ("/obj/item/stock_parts/power_store/cell/super", 5.0),
    ("/obj/item/stock_parts/power_store/cell/hyper", 5.0),
    ("/obj/item/stock_parts/power_store/cell/bluespace", 5.0),
];

pub(super) struct EmpPowerPlugin;

impl Plugin for EmpPowerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                (
                    cells, apcs, smes, lights, machines, chargers, canisters, mechs,
                )
                    .in_set(EmpSystems::Effects),
                (restore_apcs, restore_mechs).in_set(EmpSystems::Restore),
            ),
        );
    }
}

/// `emp_damage_modifier` of a cell type.
pub fn cell_modifier(path: &str) -> f64 {
    CELL_EMP_MODIFIER
        .iter()
        .find(|(t, _)| is_under(path, t))
        .map_or(1.0, |&(_, m)| m)
}

/// `/obj/item/stock_parts/power_store/emp_act`: `use(STANDARD_CELL_CHARGE / severity *
/// emp_damage_modifier, force = TRUE)`, joules.
pub fn cell_drain(severity: u8, modifier: f64) -> f64 {
    STANDARD_CELL_CHARGE / f64::from(severity.max(1)) * modifier
}

/// `cell.emp_act(severity)` of a cell item held by a machine or device: its own protection,
/// then the drain.
pub fn cell_emp_act(world: &mut World, cell: Entity, severity: u8) {
    if protection_of(world, cell) & super::PROTECT_SELF != 0 {
        return;
    }
    let modifier = path_of(world, cell).map_or(1.0, |p| cell_modifier(&p));
    if let Some(mut c) = world.get_mut::<ItemCell>(cell) {
        c.0.take(cell_drain(severity, modifier), true);
    }
}

/// Cell items hit directly (on the floor, in a bag or a hand).
fn cells(world: &mut World, mut cursor: Local<MessageCursor<EmpAct>>) {
    for ev in emp_acts(world, &mut cursor) {
        if ev.hits_self() && world.get::<ItemCell>(ev.target).is_some() {
            let modifier = path_of(world, ev.target).map_or(1.0, |p| cell_modifier(&p));
            if let Some(mut c) = world.get_mut::<ItemCell>(ev.target) {
                c.0.take(cell_drain(ev.severity, modifier), true);
            }
        }
    }
}

/// `/obj/machinery/power/apc/emp_act`: the cell drains; every channel goes off, equipment and
/// environment come back on auto after a minute (`reset(APC_RESET_EMP)`; the lights stay off
/// until someone turns them on).
fn apcs(world: &mut World, mut cursor: Local<MessageCursor<EmpAct>>) {
    for ev in emp_acts(world, &mut cursor) {
        let Some(mut apc) = world.get_mut::<Apc>(ev.target) else {
            continue;
        };
        if ev.hits_contents()
            && let Some(cell) = apc.cell.as_mut()
        {
            cell.take(cell_drain(ev.severity, 1.0), true);
        }
        if !ev.hits_self() {
            continue;
        }
        apc.channels = [CHANNEL_OFF; 3];
        crate::wires::apc_update(world, ev.target);
        set_emped(world, ev.target, APC_RESET_SECS);
    }
}

fn restore_apcs(world: &mut World) {
    for apc in emped_expired::<With<Apc>>(world) {
        world.entity_mut(apc).remove::<Emped>();
        if let Some(mut a) = world.get_mut::<Apc>(apc) {
            a.channels[crate::power::Channel::Equip as usize] = CHANNEL_AUTO_ON;
            a.channels[crate::power::Channel::Environ as usize] = CHANNEL_AUTO_ON;
        }
        crate::wires::apc_update(world, apc);
    }
}

/// `/obj/machinery/power/smes/emp_act`: random input and output switches and levels, a
/// `STANDARD_BATTERY_CHARGE / severity` drain.
fn smes(world: &mut World, mut cursor: Local<MessageCursor<EmpAct>>) {
    for ev in emp_acts(world, &mut cursor) {
        if !ev.hits_self() || world.get::<Smes>(ev.target).is_none() {
            continue;
        }
        let (input, output, out_level, in_level) = {
            let mut rng = world.resource_mut::<EmpRng>();
            (
                rng.range(0, 1) == 1,
                rng.range(0, 1) == 1,
                rng.range(0, SMES_LEVEL_MAX as i64) as f64,
                rng.range(0, SMES_LEVEL_MAX as i64) as f64,
            )
        };
        let mut s = world.get_mut::<Smes>(ev.target).expect("checked above");
        s.input_attempt = input;
        s.inputting = input;
        s.output_attempt = output;
        s.outputting = output;
        s.output_level = out_level;
        s.input_level = in_level;
        s.charge = (s.charge - ev.by_severity(STANDARD_BATTERY_CHARGE)).max(0.0);
    }
}

/// `/obj/machinery/light/emp_act`: the emergency cell drains; a working bulb burns out at
/// `150 / severity` percent.
fn lights(world: &mut World, mut cursor: Local<MessageCursor<EmpAct>>) {
    for ev in emp_acts(world, &mut cursor) {
        if !ev.hits_self() {
            continue;
        }
        let Some(mut fixture) = world.get_mut::<LightFixture>(ev.target) else {
            continue;
        };
        if let Some(charge) = fixture.emergency.as_mut() {
            *charge = (*charge - cell_drain(ev.severity, 1.0)).max(0.0);
        }
        let working = fixture.on_light.is_some();
        world.entity_mut(ev.target).insert(LightProcessing);
        if working
            && world
                .resource_mut::<EmpRng>()
                .prob(150.0 / f64::from(ev.severity))
        {
            crate::power::burn_out_light(world, ev.target);
        }
    }
}

/// `/obj/machinery/emp_act`: a working machine that uses power surges `7.5 kJ / severity`
/// from its area and sparks.
// shortcut: no random uncommon language (no languages).
fn machines(world: &mut World, mut cursor: Local<MessageCursor<EmpAct>>) {
    for ev in emp_acts(world, &mut cursor) {
        let Ok(e) = world.get_entity(ev.target) else {
            continue;
        };
        if !ev.hits_self()
            || !e.contains::<PowerMachine>()
            || e.contains::<Unpowered>()
            || e.contains::<Emped>()
        {
            continue;
        }
        crate::power::machine_use_energy(
            world,
            ev.target,
            ev.by_severity(MACHINE_EMP_ENERGY),
            false,
        );
        sparks(world, ev.target);
    }
}

/// `/obj/machinery/cell_charger/emp_act` and `/obj/machinery/recharger/emp_act`: a powered
/// charger passes the pulse to what it charges (a recharger empties a baton's cell).
fn chargers(world: &mut World, mut cursor: Local<MessageCursor<EmpAct>>) {
    for ev in emp_acts(world, &mut cursor) {
        if !ev.hits_contents() || world.get::<Unpowered>(ev.target).is_some() {
            continue;
        }
        if let Some(cell) = world.get::<CellCharger>(ev.target).and_then(|c| c.charging) {
            cell_emp_act(world, cell, ev.severity);
            continue;
        }
        let Some(item) = world.get::<Recharger>(ev.target).and_then(|r| r.charging) else {
            continue;
        };
        if let Some(mut baton) = world.get_mut::<crate::security::Baton>(item) {
            if let Some(charge) = baton.charge.as_mut() {
                *charge = 0.0;
            }
        } else if let Some(mut gun) = world.get_mut::<crate::guns::Gun>(item) {
            gun.charge = (gun.charge - cell_drain(ev.severity, 1.0)).max(0.0);
        }
    }
}

/// `/obj/machinery/portable_atmospherics/canister/emp_act`: its cell, and the shielding goes
/// off at `75 / severity` percent.
fn canisters(world: &mut World, mut cursor: Local<MessageCursor<EmpAct>>) {
    for ev in emp_acts(world, &mut cursor) {
        let Some(canister) = world.get::<Canister>(ev.target).cloned() else {
            continue;
        };
        if !ev.hits_self() {
            continue;
        }
        if let Some(cell) = canister.cell {
            cell_emp_act(world, cell, ev.severity);
        }
        if canister.shielding
            && world
                .resource_mut::<EmpRng>()
                .prob(75.0 / f64::from(ev.severity))
        {
            if let Some(mut c) = world.get_mut::<Canister>(ev.target) {
                c.shielding = false;
            }
            info!("canister {:?} shielding turned off due to EMP", ev.target);
        }
    }
}

/// On an exosuit whose equipment control is lost (tg `equipment_disabled`).
#[derive(Component, Clone, Copy, Debug)]
pub struct MechEquipmentDisabled;

/// `/obj/vehicle/sealed/mecha/emp_act`: half the cell over `severity * capacitor rating`, burn
/// damage `rand(100, 180)` over the same, equipment control lost for 3 s.
// shortcut: no chassis camera refocus or tracking beacons (not ported).
fn mechs(world: &mut World, mut cursor: Local<MessageCursor<EmpAct>>) {
    for ev in emp_acts(world, &mut cursor) {
        let Some(mech) = world.get::<Mech>(ev.target).cloned() else {
            continue;
        };
        if !ev.hits_self() {
            continue;
        }
        let rating = mech
            .capacitor
            .and_then(|c| path_of(world, c))
            .map_or(1, |p| world.resource::<MechaDb>().rating(&p));
        let vulnerability = f64::from(ev.severity) * f64::from(rating.max(1));
        let cell = mech
            .cell
            .and_then(|c| world.get::<ItemCell>(c).map(|i| i.0));
        if let Some(cell) = cell.filter(|c| c.charge > 0.0) {
            crate::mecha::core::use_energy(
                world,
                ev.target,
                (cell.maxcharge / 2.0 / vulnerability).round(),
            );
        }
        let damage = world
            .resource_mut::<EmpRng>()
            .range(MECH_EMP_DAMAGE.0, MECH_EMP_DAMAGE.1) as f64;
        crate::mecha::core::take_damage(
            world,
            ev.target,
            (damage / vulnerability) as f32,
            DamageType::Burn,
            "",
            None,
        );
        if world.get_entity(ev.target).is_err() {
            continue;
        }
        if world.get::<MechEquipmentDisabled>(ev.target).is_none() && mech.occupant.is_some() {
            crate::mecha::core::tell_occupant(
                world,
                ev.target,
                Span::Warning,
                "Error -- Connection to equipment control unit has been lost.",
            );
        }
        world.entity_mut(ev.target).insert(MechEquipmentDisabled);
        // `TIMER_OVERRIDE`: the last pulse sets the time.
        let until = world.resource::<Time>().elapsed_secs_f64() + MECH_EQUIPMENT_SECS;
        world.entity_mut(ev.target).insert(Emped { until });
    }
}

fn restore_mechs(world: &mut World) {
    for mech in emped_expired::<With<MechEquipmentDisabled>>(world) {
        world
            .entity_mut(mech)
            .remove::<(Emped, MechEquipmentDisabled)>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_drain_follows_severity_and_type() {
        assert_eq!(cell_drain(super::super::EMP_HEAVY, 1.0), 10_000.0);
        assert_eq!(cell_drain(super::super::EMP_LIGHT, 1.0), 5_000.0);
        assert_eq!(
            cell_modifier("/obj/item/stock_parts/power_store/cell/high/empty"),
            3.0
        );
        assert_eq!(cell_modifier("/obj/item/stock_parts/power_store/cell"), 1.0);
        assert_eq!(
            cell_drain(
                1,
                cell_modifier("/obj/item/stock_parts/power_store/cell/emproof")
            ),
            0.0
        );
    }
}
