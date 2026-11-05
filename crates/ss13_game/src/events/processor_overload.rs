//! Processor Overload (`code/modules/events/processor_overload.dm`): every telecomms processor
//! takes a heavy EMP (`EMPED` for `rand(148, 152)` s at 50%), so the voice it would decompress
//! comes out garbled (`crate::telecomms`); one in ten pops instead: a light explosion of range 2
//! and a devastating blast on the machine itself. Announced 80% of the time, garbled.

use bevy::prelude::*;

use super::controller::{EventRng, EventState, RoundEvent};
use crate::explosions::ex_act::ex_act_movable;
use crate::explosions::{Explosion, Severity, explosion};
use crate::telecomms::MachineKind;

pub const NAME: &str = "Processor Overload";
const ALERTS: [&str; 6] = [
    "Приближается экзосферный пузырь. Возможна перегрузка процессора. Пожалуйста, свяжитесь с \
     ваши*%xp25)`6cq-BZZT",
    "Приближается экзосферный пузырь. Возможна перегрузка процl*1eta;c5;'1v¬-BZZZT",
    "Приближается экзосферный пузырь. Возможна пер#MCi46:5.;@63-BZZZZT",
    "Приближается экзосферн'Fz\\k55_@-BZZZZZT",
    "Приближается экз:%£ QCbyj^j</.3-BZZZZZZT",
    "!!hy%;f3l7e,<$^-BZZZZZZZT",
];

pub fn make(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(Overload)
}

struct Overload;

impl RoundEvent for Overload {
    fn setup(&mut self, _: &mut World, ev: &mut EventState) {
        ev.announce_when = 1;
    }

    // shortcut: no AIs to always tell (`GLOB.ai_list`).
    fn announce(&mut self, world: &mut World, _: &mut EventState, fake: bool) {
        let mut rng = world.resource_mut::<EventRng>();
        let alert = *rng.pick(&ALERTS).expect("non-empty");
        if rng.0.prob(80.0) || fake {
            super::priority_announce(world, "Обнаружена аномалия", alert);
        }
    }

    // shortcut: no `announce_to_ghosts` (no ghost orbit alerts).
    fn start(&mut self, world: &mut World, _: &mut EventState) {
        for processor in crate::telecomms::machines_of(world, Some(MachineKind::Processor)) {
            if !world.resource_mut::<EventRng>().0.prob(10.0) {
                crate::telecomms::emp_machine(world, processor, crate::emp::EMP_HEAVY);
                continue;
            }
            let Some(&pos) = world.get::<ss13_shared::TilePos>(processor) else {
                continue;
            };
            explosion(
                world,
                pos,
                Explosion::new(0, 0, 2).cause("processor overload"),
            );
            // `SSexplosions.high_mov_atom`: only this blast damages the machine itself.
            ex_act_movable(world, processor, Severity::Devastate);
        }
    }
}
