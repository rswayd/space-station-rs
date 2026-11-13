//! The modular computer as a circuit shell, after tg `computer_circuit.dm` and
//! `add_shell_component`.
//!
//! Every carried computer is a medium shell and every console a large one
//! (`crate::wiremod::server::ShellKind`); an attached circuit gets the "Modular Computer"
//! component (`Kind::Modpc`): `Turn On/Off` shuts the computer down or turns it on, `Toggle
//! Lights` switches its flashlight, `Print` prints `Print Text`; it sends `Turned On` and `Shut
//! Down` when the computer's power state changes by any path. A circuit without a cell of its
//! own runs on the computer's power (`use_energy_for_circuits`). Silicon PDAs are no shells.
// shortcut: no paper in computers, so `Print` only says the printer is out of paper; the
// flashlight colour (`Set Color`) is not kept (the light is always white); the circuit's
// programs components (messenger, signaler, status display and others) are not ported.

use bevy::prelude::*;

use super::pda::ModularComputer;
use super::power::ComputerCell;
use crate::wiremod::circuit::{ComputerEffect, Kind};
use crate::wiremod::server::Shell;

/// `/obj/item/modular_computer`: carried computers are medium shells.
pub const COMPUTER_ITEM: &str = "/obj/item/modular_computer";

/// The computer's power state the circuit last saw.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
struct SeenEnabled(bool);

/// The shell is a computer whose power a cell-less circuit uses.
pub fn powers_circuit(world: &World, shell: Entity) -> bool {
    world.get::<ModularComputer>(shell).is_some()
}

/// The charge a circuit in computer `shell` may spend: the computer's cell, or plenty for a
/// console on area power (`processor/use_energy`).
pub fn circuit_charge(world: &World, shell: Entity) -> Option<f64> {
    let console = world
        .get::<ModularComputer>(shell)
        .is_some_and(|c| super::power::is_console(&c.path));
    if console && world.get::<crate::power::Unpowered>(shell).is_none() {
        return Some(f64::MAX / 4.0);
    }
    world
        .get::<ComputerCell>(shell)
        .and_then(|c| c.0)
        .map(|c| c.charge)
}

/// `shutdown_computer`.
fn shutdown(world: &mut World, pc: Entity) {
    let card_mod = world.get::<ModularComputer>(pc).is_some_and(|c| {
        c.active == Some(super::programs::ProgramKind::CardMod)
            || c.idle.contains(&super::programs::ProgramKind::CardMod)
    });
    if card_mod {
        super::programs::card_mod::on_kill(world, pc);
    }
    if let Some(mut c) = world.get_mut::<ModularComputer>(pc) {
        c.shutdown();
    }
}

/// The component's effect on its computer.
pub fn apply(world: &mut World, pc: Entity, effect: ComputerEffect) {
    let Some(computer) = world.get::<ModularComputer>(pc) else {
        return;
    };
    let (enabled, has_light) = (computer.enabled, computer.has_light);
    match effect {
        ComputerEffect::TogglePower if enabled => shutdown(world, pc),
        ComputerEffect::TogglePower => {
            if super::power::can_turn_on(world, pc)
                && let Some(mut c) = world.get_mut::<ModularComputer>(pc)
            {
                c.enabled = true;
            }
        }
        // `toggle_flashlight`: needs the light and a charged cell.
        ComputerEffect::ToggleLight => {
            let charged = world
                .get::<ComputerCell>(pc)
                .and_then(|c| c.0)
                .is_some_and(|c| c.charge > 0.0);
            if has_light
                && charged
                && let Some(mut c) = world.get_mut::<ModularComputer>(pc)
            {
                c.light_on = !c.light_on;
            }
        }
        ComputerEffect::Print(_) if enabled => {
            // `print_text` without `stored_paper`.
            debug!("modular computer {pc}: circuit print with no paper");
        }
        ComputerEffect::Print(_) | ComputerEffect::LightColor(_) => {}
    }
}

/// `COMSIG_MODULAR_COMPUTER_TURNED_ON` / `_SHUT_DOWN` to the attached circuit: `Turned On` or
/// `Shut Down` when the power state changed since the last frame.
// Cost: per frame, the computers with a circuit attached (a handful).
fn power_events(world: &mut World) {
    let mut q = world.query::<(Entity, &ModularComputer, &Shell, Option<&SeenEnabled>)>();
    let changed: Vec<(Entity, Entity, bool, bool)> = q
        .iter(world)
        .filter_map(|(e, c, s, seen)| {
            let circuit = s.circuit?;
            (seen.map(|s| s.0) != Some(c.enabled)).then_some((
                e,
                circuit,
                c.enabled,
                seen.is_some(),
            ))
        })
        .collect();
    let now = world.resource::<Time>().elapsed_secs_f64();
    for (pc, circuit, enabled, seen) in changed {
        world.entity_mut(pc).insert(SeenEnabled(enabled));
        // The state at attach time is only recorded.
        if seen {
            let port = if enabled { 0 } else { 1 };
            crate::wiremod::server::with_circuit(world, circuit, |c, _| {
                c.shell_output(Kind::Modpc, port, now)
            });
        }
    }
}

pub fn build(app: &mut App) {
    app.add_systems(Update, power_events);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::power::Cell;

    #[test]
    fn the_component_powers_and_lights_its_computer() {
        let mut world = World::new();
        let pda = world
            .spawn((
                ModularComputer::pda("/obj/item/modular_computer/pda/crew", "pda"),
                ComputerCell(Some(Cell {
                    charge: 100.0,
                    maxcharge: 100.0,
                    chargerate: 1.0,
                })),
            ))
            .id();
        apply(&mut world, pda, ComputerEffect::TogglePower);
        assert!(world.get::<ModularComputer>(pda).unwrap().enabled);
        assert_eq!(
            circuit_charge(&world, pda),
            Some(98.0),
            "turning on costs 2 J"
        );
        apply(&mut world, pda, ComputerEffect::ToggleLight);
        assert!(world.get::<ModularComputer>(pda).unwrap().light_on);
        apply(&mut world, pda, ComputerEffect::TogglePower);
        assert!(!world.get::<ModularComputer>(pda).unwrap().enabled);
        // A console on area power never runs its circuit dry.
        let console = world
            .spawn(ModularComputer::console(
                "/obj/machinery/modular_computer/preset/civilian",
                "c",
                vec![],
            ))
            .id();
        assert!(circuit_charge(&world, console).is_some_and(|c| c > 1e9));
    }

    #[test]
    fn the_circuit_hears_power_changes() {
        use crate::wiremod::circuit::{Circuit, Ctx, Value};
        let mut c = Circuit::default();
        let mut rng = crate::combat::CombatRng(1);
        let names = |_: Entity| None;
        let mut ctx = Ctx {
            now: 0.0,
            charge: Some(1000.0),
            has_shell: true,
            rng: &mut rng,
            name_of: &names,
            id_of: &crate::wiremod::circuit::no_id,
            id_card: &crate::wiremod::circuit::no_card,
            ntnet: false,
            effects: Vec::new(),
        };
        c.attach_shell(&[Kind::Modpc], &mut ctx);
        assert!(c.shell_output(Kind::Modpc, 1, 5.0));
        let node = c.nodes.iter().find(|n| n.kind == Kind::Modpc).unwrap();
        assert_eq!(node.outputs[1].value, Value::Num(5.0));
        assert_eq!(node.inputs[0].name, "Turn On/Off");
    }
}
