//! Communications Blackout (`code/modules/events/communications_blackout.dm`): a heavy EMP on
//! every telecomms machine (`emp_act(EMP_HEAVY)`: `EMPED` for `rand(298, 302)` s), so headsets
//! go silent ([`crate::radio::TelecommsOutage`]). The announcement comes only 30% of the time,
//! garbled.

use bevy::prelude::*;

use super::controller::{EventRng, EventState, RoundEvent};
use crate::radio::TelecommsOutage;

pub const NAME: &str = "Communications Blackout";
/// `/obj/machinery/telecomms/emp_act`: `(300 SECONDS) / severity` (`EMP_HEAVY` = 1), ±2 s.
const EMP_SECS: i64 = 300;

const ALERTS: [&str; 6] = [
    "Обнаружены ионосферные аномалии. Временный сбой в телекоммуникации неизбежен. Пожалуйста, \
     свяжитесь с вашим*%fj00)`5vc-BZZT",
    "Обнаружены ионосферные аномалии. Временный сбой св*3mga;b4;'1v¬-BZZZT",
    "Обнаружены ионосферные аномалии. Вре#MCi46:5.;@63-BZZZZT",
    "Обнаружены ионосферные ано'fZ\\kg5_0-BZZZZZT",
    "Ионосфе:%£ MCayj^j<.3-BZZZZZZT",
    "#4nd%;f4y6,>£%-BZZZZZZZT",
];

pub fn make(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(Blackout)
}

struct Blackout;

impl RoundEvent for Blackout {
    fn setup(&mut self, _: &mut World, ev: &mut EventState) {
        ev.announce_when = 1;
    }

    // shortcut: no AIs to always tell (`GLOB.ai_list`).
    fn announce(&mut self, world: &mut World, _: &mut EventState, fake: bool) {
        let mut rng = world.resource_mut::<EventRng>();
        let alert = *rng.pick(&ALERTS).expect("non-empty");
        if rng.0.prob(30.0) || fake {
            super::priority_announce(world, "Обнаружена аномалия", alert);
        }
    }

    fn start(&mut self, world: &mut World, _: &mut EventState) {
        let now = world.resource::<Time>().elapsed_secs_f64();
        let secs = world
            .resource_mut::<EventRng>()
            .0
            .range(EMP_SECS - 2, EMP_SECS + 2);
        let mut outage = world.resource_mut::<TelecommsOutage>();
        // An EMPed machine is not EMPed again (`!(machine_stat & EMPED)`).
        if outage.until <= now {
            outage.until = now + secs as f64;
        }
    }
}
