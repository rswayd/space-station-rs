//! An ethereal's stomach battery and the APC (tg `/obj/item/organ/stomach/ethereal`,
//! `code/modules/power/apc/apc_attack.dm` `attack_hand_secondary`, `ethereal_interact`).
//!
//! - The ethereal stomach keeps a charge, joules ([`EtherealStomach`]; tg `cell`, an
//!   `/obj/item/stock_parts/power_store/cell/ethereal` of `ETHEREAL_CHARGE_DANGEROUS` that starts
//!   at `ETHEREAL_CHARGE_FULL`).
//! - A human's right click on an APC: without an ethereal stomach, or with one charged to at
//!   least `ETHEREAL_CHARGE_NORMAL`, the interface lock flips (`togglelock`); an ethereal then
//!   drains the APC's cell (combat mode) or charges it from the stomach, 3 s per step, a tenth of
//!   `STANDARD_ETHEREAL_CHARGE` each. Draining stops at half the APC's cell and repeats while
//!   both sides allow it; charging is one step.
// shortcut: the stomach's own life (discharge over time, hunger alerts and damage from low or
// high charge, shock absorption, multitool readout, cyborg chargers) belongs to the species
// (`docs/parity/mobs/species-list.md`); here the charge only moves to and from APCs.
// shortcut: tg delays the first balloon by 0.75 s so it does not collide with the lock's; it
// shows at once here.

use bevy::prelude::*;

use super::model::Apc;
use crate::construction::Job;
use crate::medical::organs::{self, Organ, Slot};
use crate::tools::{Actor, DoAfter};

/// `/obj/item/organ/stomach/ethereal`.
pub const ETHEREAL_STOMACH: &str = "/obj/item/organ/stomach/ethereal";
/// `STANDARD_ETHEREAL_CHARGE` (`STANDARD_CELL_CHARGE`, 10 kJ).
pub const STANDARD_ETHEREAL_CHARGE: f64 = 10_000.0;
/// `ETHEREAL_CHARGE_NORMAL`.
pub const ETHEREAL_CHARGE_NORMAL: f64 = STANDARD_ETHEREAL_CHARGE;
/// `ETHEREAL_CHARGE_FULL`: what a new stomach holds.
pub const ETHEREAL_CHARGE_FULL: f64 = 2.0 * STANDARD_ETHEREAL_CHARGE;
/// `ETHEREAL_CHARGE_DANGEROUS`: the cell's `maxcharge` and the clamp of `adjust_charge`.
pub const ETHEREAL_CHARGE_DANGEROUS: f64 = 3.0 * STANDARD_ETHEREAL_CHARGE;
/// `ETHEREAL_APC_DRAIN_TIME`, deciseconds.
const DRAIN_TIME_DS: f32 = 30.0;
/// `ETHEREAL_APC_POWER_GAIN`.
pub const APC_POWER_GAIN: f64 = 0.1 * STANDARD_ETHEREAL_CHARGE;

/// The stomach's `cell` charge and `drain_time` (seconds of game time).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct EtherealStomach {
    pub charge: f64,
    pub drain_until: f64,
}

impl Default for EtherealStomach {
    fn default() -> Self {
        Self {
            charge: ETHEREAL_CHARGE_FULL,
            drain_until: f64::NEG_INFINITY,
        }
    }
}

impl EtherealStomach {
    /// `cell.used_charge()`.
    pub fn used_charge(&self) -> f64 {
        ETHEREAL_CHARGE_DANGEROUS - self.charge
    }

    /// `adjust_charge(amount)`: the change actually made.
    pub fn adjust_charge(&mut self, amount: f64) -> f64 {
        let amount = amount.clamp(-self.charge, ETHEREAL_CHARGE_DANGEROUS - self.charge);
        self.charge += amount;
        amount
    }
}

/// A step of an ethereal at an APC.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EtherealJob {
    /// `discharge_to_ethereal`: the APC's cell into the stomach.
    Drain { stomach: Entity },
    /// `charge_from_ethereal`: the stomach into the APC's cell.
    Charge { stomach: Entity },
}

/// New ethereal stomachs get their cell (`Initialize`).
fn add_stomach_cell(add: On<Add<Organ>>, organs: Query<&Organ>, mut commands: Commands) {
    if organs
        .get(add.entity)
        .is_ok_and(|o| crate::obj_damage::is_under(&o.path, ETHEREAL_STOMACH))
    {
        commands
            .entity(add.entity)
            .insert_if_new(EtherealStomach::default());
    }
}

pub(super) struct EtherealPlugin;

impl Plugin for EtherealPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(add_stomach_cell);
    }
}

/// `get_organ_slot(ORGAN_SLOT_STOMACH)` when it is an ethereal's.
pub fn stomach(world: &World, mob: Entity) -> Option<Entity> {
    organs::organ(world, mob, Slot::Stomach).filter(|&s| world.get::<EtherealStomach>(s).is_some())
}

/// `attack_hand_secondary` of a human on an APC: the lock, then an ethereal's charge exchange.
pub fn attack_hand_secondary(world: &mut World, client: Entity, mob: Entity, apc: Entity) {
    let Some(stomach) = stomach(world, mob) else {
        super::apc_ui::toggle_lock(world, mob, apc);
        return;
    };
    if world
        .get::<EtherealStomach>(stomach)
        .is_some_and(|s| s.charge >= ETHEREAL_CHARGE_NORMAL)
    {
        super::apc_ui::toggle_lock(world, mob, apc);
    }
    ethereal_interact(world, client, mob, apc, stomach);
}

fn alert(world: &mut World, mob: Entity, apc: Entity, text: &str) {
    crate::balloon::balloon_alert(world, mob, apc, text);
}

/// `ethereal_interact`.
fn ethereal_interact(world: &mut World, client: Entity, mob: Entity, apc: Entity, stomach: Entity) {
    let Some(cell) = world.get::<Apc>(apc).and_then(|a| a.cell) else {
        return;
    };
    let now = crate::tools::now(world);
    let Some(state) = world.get::<EtherealStomach>(stomach).copied() else {
        return;
    };
    if state.drain_until > now {
        return;
    }
    let job = if crate::medical::surgery::combat_mode(world, mob) {
        // Ethereals cannot drain an APC under half charge: the station's low power sends them
        // to other sources.
        if cell.charge < cell.maxcharge / 2.0 {
            alert(
                world,
                mob,
                apc,
                "протоколы безопасности предотвращают выкачивание энергии!",
            );
            return;
        }
        alert(world, mob, apc, "выкачивание энергии...");
        EtherealJob::Drain { stomach }
    } else {
        if cell.charge >= cell.maxcharge {
            alert(world, mob, apc, "ЛКП полностью заряжен!");
            return;
        }
        if state.charge <= 0.0 {
            alert(world, mob, apc, "слишком мало заряда!");
            return;
        }
        alert(world, mob, apc, "передача энергии...");
        EtherealJob::Charge { stomach }
    };
    if let Some(mut s) = world.get_mut::<EtherealStomach>(stomach) {
        s.drain_until = now + f64::from(DRAIN_TIME_DS) / 10.0;
    }
    start(world, client, mob, apc, job);
}

fn start(world: &mut World, client: Entity, mob: Entity, apc: Entity, job: EtherealJob) {
    let actor = Actor {
        client,
        mob,
        item: None,
    };
    crate::tools::do_after(
        world,
        actor,
        apc,
        DRAIN_TIME_DS,
        0,
        Job::Power(super::PowerJob::Ethereal(job)),
    );
}

/// The end of a 3 s step, every check again at the moment of effect.
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: EtherealJob) {
    let apc = action.target;
    let (EtherealJob::Drain { stomach } | EtherealJob::Charge { stomach }) = job;
    if organs::organ(world, mob, Slot::Stomach) != Some(stomach)
        || world.get::<EtherealStomach>(stomach).is_none()
    {
        alert(world, mob, apc, "желудок извлечён!?");
        return;
    }
    let Some(mut cell) = world.get::<Apc>(apc).and_then(|a| a.cell) else {
        alert(world, mob, apc, "батарейка извлечена!");
        return;
    };
    let mut state = *world.get::<EtherealStomach>(stomach).expect("checked");
    let (text, again) = match job {
        EtherealJob::Drain { .. } => {
            let half = cell.maxcharge / 2.0;
            if cell.charge < half {
                alert(world, mob, apc, "активирован протокол безопасности!");
                return;
            }
            let potential = (cell.charge - half).min(state.used_charge());
            let drained = cell.take(APC_POWER_GAIN.min(potential), true);
            state.adjust_charge(drained);
            if state.used_charge() <= 0.0 {
                (Some("ваш заряд полон!"), false)
            } else if cell.charge <= 0.0 {
                (Some("ЛКП пуст!"), false)
            } else {
                // `while(do_after(...))`: the next step at once.
                (None, true)
            }
        }
        EtherealJob::Charge { .. } => {
            let potential = state.charge.min(cell.used_charge());
            let drained = state.adjust_charge(-APC_POWER_GAIN.min(potential));
            cell.give(-drained);
            if cell.used_charge() <= 0.0 {
                (Some("ЛКП полностью заряжен!"), false)
            } else if state.charge <= 0.0 {
                (Some("заряд закончился!"), false)
            } else {
                (None, false)
            }
        }
    };
    if let Some(mut a) = world.get_mut::<Apc>(apc) {
        a.cell = Some(cell);
    }
    if let Some(mut s) = world.get_mut::<EtherealStomach>(stomach) {
        s.charge = state.charge;
    }
    if let Some(text) = text {
        alert(world, mob, apc, text);
    }
    if again {
        start(world, action.client, mob, apc, job);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adjust_charge_clamps_to_the_cell() {
        let mut s = EtherealStomach::default();
        assert_eq!(s.charge, 20_000.0);
        assert_eq!(s.adjust_charge(15_000.0), 10_000.0);
        assert_eq!(s.used_charge(), 0.0);
        assert_eq!(s.adjust_charge(-40_000.0), -30_000.0);
        assert_eq!(s.charge, 0.0);
    }
}
