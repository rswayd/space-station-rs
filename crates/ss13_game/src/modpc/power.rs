//! Power of modular computers after tg `computer_power.dm` and `/obj/machinery/modular_computer`.
//!
//! Every computer has an internal cell ([`ComputerCell`]): a carried computer a standard cell, a
//! console's processor a cheap AA cell (`cell/crap`). Every `SSobj` tick (2 s) an enabled
//! computer pays `handle_power`: its base usage (screen on or off), ×1.1 with the flashlight, the
//! program in front's `power_cell_use` and half of each background program's. A console draws
//! from its area while the area is powered (`processor/use_energy`) and tops its cell up by its
//! `idle_power_usage` (`modular_computer/process`); without area power, or for a carried
//! computer, the cell pays. A cell that cannot pay shuts the computer down (`power_failure`), and
//! an empty one does not turn on. A recharger charges a carried computer's cell
//! (`crate::power::chargers`).
//!
//! Cost: one pass over the computers every 2 s (MetaStation: ~150 PDAs and consoles), O(1)
//! each.
// shortcut: the cell is not a separate item: no screwdriver removal or `cell_act` insertion,
// no EMP on it; add with a cell slot once the computer is disassemblable.

use bevy::prelude::*;
use ss13_mapload::DmPower;
use ss13_mapload::fidelity::is_under;

use super::pda::ModularComputer;
use super::programs::ProgramKind;
use crate::power::{Cell, PowerMachine, Unpowered};

/// `SSobj` `wait`: modular computers process every 2 seconds.
pub const PROCESS_SECS: f64 = 2.0;
/// `/obj/item/modular_computer` `base_active_power_usage`, `base_idle_power_usage` (2 W, 1 W).
pub const ITEM_ACTIVE_POWER: f64 = 2.0;
pub const ITEM_IDLE_POWER: f64 = 1.0;
/// `/obj/machinery/modular_computer` `base_active_power_usage`, `base_idle_power_usage`.
pub const CONSOLE_ACTIVE_POWER: f64 = 500.0;
pub const CONSOLE_IDLE_POWER: f64 = 100.0;
/// `FLASHLIGHT_DRAIN_MULTIPLIER`.
const FLASHLIGHT_DRAIN_MULTIPLIER: f64 = 1.1;
/// `PROGRAM_BASIC_CELL_USE`.
const PROGRAM_BASIC_CELL_USE: f64 = 2.0;
/// `/obj/item/stock_parts/power_store/cell`: `STANDARD_CELL_CHARGE`, `STANDARD_CELL_RATE * 0.05`.
const CELL_CHARGE: f64 = 10_000.0;
const CELL_RATE: f64 = 500.0;
/// `/obj/machinery` root: a console, not a carried computer.
const MACHINE_ROOT: &str = "/obj/machinery";

/// `internal_cell`: `None` without one.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct ComputerCell(pub Option<Cell>);

impl ComputerCell {
    /// The default cell of a computer of type `path`.
    pub fn of_type(path: &str) -> Self {
        let maxcharge = if is_console(path) {
            CELL_CHARGE * 0.5
        } else {
            CELL_CHARGE
        };
        Self(Some(Cell {
            charge: maxcharge,
            maxcharge,
            chargerate: CELL_RATE,
        }))
    }

    /// `PC_batterypercent`: `round(percent())`.
    pub fn percent(&self) -> Option<u8> {
        self.0.map(|c| c.percent().round().clamp(0.0, 100.0) as u8)
    }
}

/// `/obj/machinery/modular_computer`: powered by its area.
pub fn is_console(path: &str) -> bool {
    is_under(path, MACHINE_ROOT)
}

/// `power_cell_use` of a program: the messenger is free (`NONE`), the rest basic.
pub fn program_power(program: ProgramKind) -> f64 {
    match program {
        ProgramKind::Messenger => 0.0,
        _ => PROGRAM_BASIC_CELL_USE,
    }
}

/// `base_active_power_usage` of the computer.
pub fn active_power(computer: &ModularComputer) -> f64 {
    if is_console(&computer.path) {
        CONSOLE_ACTIVE_POWER
    } else {
        ITEM_ACTIVE_POWER
    }
}

/// `handle_power`'s `power_usage`, watts.
pub fn power_usage(computer: &ModularComputer, screen_on: bool) -> f64 {
    let mut usage = match (is_console(&computer.path), screen_on) {
        (true, true) => CONSOLE_ACTIVE_POWER,
        (true, false) => CONSOLE_IDLE_POWER,
        (false, true) => ITEM_ACTIVE_POWER,
        (false, false) => ITEM_IDLE_POWER,
    };
    if computer.light_on {
        usage *= FLASHLIGHT_DRAIN_MULTIPLIER;
    }
    if let Some(front) = computer.active {
        usage += program_power(front);
    }
    usage
        + computer
            .idle
            .iter()
            .map(|&p| program_power(p) / 2.0)
            .sum::<f64>()
}

/// `use_energy(amount, check_programs)`: a powered console draws from its area, anything else
/// from its cell (all or nothing). A cell that cannot pay closes every program with
/// `check_programs`. True when paid.
pub fn use_energy(world: &mut World, pc: Entity, amount: f64, check_programs: bool) -> bool {
    let Some(path) = world.get::<ModularComputer>(pc).map(|c| c.path.clone()) else {
        return false;
    };
    if is_console(&path) && world.get::<Unpowered>(pc).is_none() {
        // A machine with no `PowerMachine` uses no power (tg `NO_POWER_USE`).
        if world.get::<PowerMachine>(pc).is_some()
            && world.contains_resource::<crate::power::Grid>()
        {
            crate::power::machine_use_energy(world, pc, amount, false);
        }
        return true;
    }
    let Some(mut cell) = world.get_mut::<ComputerCell>(pc) else {
        return false;
    };
    let Some(cell) = cell.0.as_mut() else {
        return false;
    };
    if amount <= 0.0 || cell.charge >= amount {
        cell.take(amount, false);
        return true;
    }
    if check_programs {
        close_all_programs(world, pc);
    }
    false
}

/// `close_all_programs`.
fn close_all_programs(world: &mut World, pc: Entity) {
    let Some(computer) = world.get::<ModularComputer>(pc) else {
        return;
    };
    if computer.active == Some(ProgramKind::CardMod)
        || computer.idle.contains(&ProgramKind::CardMod)
    {
        super::programs::card_mod::on_kill(world, pc);
    }
    if let Some(mut computer) = world.get_mut::<ModularComputer>(pc) {
        computer.active = None;
        computer.idle.clear();
    }
}

/// `turn_on`'s power check: `use_energy(base_active_power_usage)`.
pub fn can_turn_on(world: &mut World, pc: Entity) -> bool {
    let Some(amount) = world.get::<ModularComputer>(pc).map(active_power) else {
        return false;
    };
    use_energy(world, pc, amount, true)
}

/// `power_failure`: the flashlight goes out and the computer shuts down.
pub fn power_failure(world: &mut World, pc: Entity) {
    if !world.get::<ModularComputer>(pc).is_some_and(|c| c.enabled) {
        return;
    }
    close_all_programs(world, pc);
    if let Some(mut computer) = world.get_mut::<ModularComputer>(pc) {
        computer.light_on = false;
        computer.shutdown();
    }
}

/// One `SSobj` tick of `dt` seconds: `handle_power` of every enabled computer, then
/// `modular_computer/process` (the cell top-up) of every powered console.
pub fn tick(world: &mut World, dt: f64) {
    // `screen_on`: false for a closed laptop.
    let mut q = world.query::<(Entity, &ModularComputer, Option<&super::laptop::Laptop>)>();
    let computers: Vec<(Entity, bool, f64)> = q
        .iter(world)
        .map(|(e, c, lid)| (e, c.enabled, power_usage(c, lid.is_none_or(|l| l.open))))
        .collect();
    for (pc, enabled, usage) in computers {
        if enabled {
            super::ntnet::network_check(world, pc);
        }
        if enabled && !use_energy(world, pc, usage * dt, true) {
            power_failure(world, pc);
        }
        recharge_console(world, pc, dt);
    }
}

/// `/obj/machinery/modular_computer/process`: `charge_cell(idle_power_usage * dt)` while the
/// area is powered.
fn recharge_console(world: &mut World, pc: Entity, dt: f64) {
    let console = world
        .get::<ModularComputer>(pc)
        .is_some_and(|c| is_console(&c.path));
    if !console
        || world.get::<Unpowered>(pc).is_some()
        || world.get::<PowerMachine>(pc).is_none()
        || !world.contains_resource::<crate::power::Grid>()
    {
        return;
    }
    let Some(mut cell) = world.get::<ComputerCell>(pc).and_then(|c| c.0) else {
        return;
    };
    if cell.percent() >= 100.0 {
        return;
    }
    let rate = world.get::<DmPower>(pc).map_or(0.0, |d| d.idle_power_usage);
    crate::power::charge_cell(world, pc, rate * dt, &mut cell, false);
    world.entity_mut(pc).insert(ComputerCell(Some(cell)));
}

fn process_power(world: &mut World, mut since: Local<f64>) {
    *since += world.resource::<Time>().delta_secs_f64();
    if *since < PROCESS_SECS {
        return;
    }
    // A stall catches up at most three ticks, like the machines do.
    let dt = since.min(PROCESS_SECS * 3.0);
    *since = 0.0;
    tick(world, dt);
}

/// `Initialize`: `internal_cell = new internal_cell(src)`.
fn give_cell(
    add: On<Add<ModularComputer>>,
    computers: Query<&ModularComputer>,
    mut commands: Commands,
) {
    if let Ok(computer) = computers.get(add.entity) {
        commands
            .entity(add.entity)
            .insert_if_new(ComputerCell::of_type(&computer.path));
    }
}

pub fn build(app: &mut App) {
    app.add_observer(give_cell)
        .add_systems(Update, process_power);
}

#[cfg(test)]
mod tests {
    use super::*;

    const PDA: &str = "/obj/item/modular_computer/pda/crew";
    const CONSOLE: &str = "/obj/machinery/modular_computer/preset/engineering";

    fn cell(charge: f64) -> ComputerCell {
        ComputerCell(Some(Cell {
            charge,
            maxcharge: CELL_CHARGE,
            chargerate: CELL_RATE,
        }))
    }

    #[test]
    fn usage_follows_screen_light_and_programs() {
        let mut pc = ModularComputer::pda(PDA, "pda");
        assert_eq!(power_usage(&pc, true), 2.0);
        assert_eq!(power_usage(&pc, false), 1.0);
        // The messenger is free; the notepad in front costs 2 W, in the background half.
        assert!(pc.open_program(ProgramKind::Messenger));
        assert_eq!(power_usage(&pc, true), 2.0);
        assert!(pc.open_program(ProgramKind::Notepad));
        assert_eq!(power_usage(&pc, true), 4.0);
        assert!(pc.open_program(ProgramKind::CrewManifest));
        assert_eq!(power_usage(&pc, true), 5.0);
        pc.light_on = true;
        assert!((power_usage(&pc, true) - 5.2).abs() < 1e-9);
        let console = ModularComputer::console(CONSOLE, "c", vec![]);
        assert_eq!(power_usage(&console, true), 500.0);
    }

    #[test]
    fn an_empty_cell_shuts_the_pda_down_and_keeps_it_off() {
        let mut world = World::new();
        let mut pc = ModularComputer::pda(PDA, "pda");
        pc.enabled = true;
        pc.light_on = true;
        assert!(pc.open_program(ProgramKind::Notepad));
        // 2 W × 1.1 + 2 W for 2 s: 8.4 J per tick.
        let pda = world.spawn((pc, cell(10.0))).id();
        tick(&mut world, 2.0);
        let charge = |w: &World| w.get::<ComputerCell>(pda).unwrap().0.unwrap().charge;
        assert!((charge(&world) - 1.6).abs() < 1e-9);
        assert!(world.get::<ModularComputer>(pda).unwrap().enabled);
        // The next tick cannot be paid: everything closes, the light goes out.
        tick(&mut world, 2.0);
        let pc = world.get::<ModularComputer>(pda).unwrap();
        assert!(!pc.enabled && !pc.light_on && pc.active.is_none());
        assert!((charge(&world) - 1.6).abs() < 1e-9, "all or nothing");
        assert!(!can_turn_on(&mut world, pda));
        // Charged again (a recharger), it turns on and pays the turn-on cost.
        world.entity_mut(pda).insert(cell(100.0));
        assert!(can_turn_on(&mut world, pda));
        assert_eq!(charge(&world), 98.0);
        assert_eq!(
            ComputerCell::of_type(PDA).percent(),
            Some(100),
            "a fresh PDA is full"
        );
    }

    #[test]
    fn a_console_runs_on_its_area_then_its_cell() {
        let mut world = World::new();
        let mut pc = ModularComputer::console(CONSOLE, "c", vec![]);
        pc.enabled = true;
        let console = world.spawn((pc, ComputerCell::of_type(CONSOLE))).id();
        // Powered (no `PowerMachine`: free): the cell is untouched.
        tick(&mut world, 2.0);
        assert_eq!(
            world
                .get::<ComputerCell>(console)
                .unwrap()
                .0
                .unwrap()
                .charge,
            5_000.0
        );
        // The area lost power: 1 kJ per tick from the AA cell, off after five ticks.
        world.entity_mut(console).insert(Unpowered);
        for _ in 0..5 {
            tick(&mut world, 2.0);
        }
        assert!(world.get::<ModularComputer>(console).unwrap().enabled);
        tick(&mut world, 2.0);
        assert!(!world.get::<ModularComputer>(console).unwrap().enabled);
        assert!(!can_turn_on(&mut world, console));
        world.entity_mut(console).remove::<Unpowered>();
        assert!(can_turn_on(&mut world, console), "area power turns it on");
    }
}
