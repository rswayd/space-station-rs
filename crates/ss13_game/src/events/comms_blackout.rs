//! Communications Blackout (`code/modules/events/communications_blackout.dm`): a heavy EMP on
//! every telecomms machine ([`crate::emp::machines::telecomms_emp`]: `EMPED` for
//! `rand(298, 302)` s), so headsets go silent ([`crate::radio::TelecommsOutage`]). The announcement comes only 30% of the time,
//! garbled.

use bevy::prelude::*;

use super::controller::{EventRng, EventState, RoundEvent};

pub const NAME: &str = "Communications Blackout";
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
        crate::emp::machines::telecomms_emp(world, crate::emp::EMP_HEAVY);
    }
}
