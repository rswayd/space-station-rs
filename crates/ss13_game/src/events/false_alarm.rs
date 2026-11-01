//! False Alarm (`code/modules/events/false_alarm.dm`): announces another event that could
//! spawn now (`fakeable` ones) without running it.
// shortcut: dynamic midround rulesets are not fakeable yet (no midround antagonists).

use bevy::prelude::*;

use super::controller::{EventRng, EventState, Events, RoundEvent, active_players};

pub const NAME: &str = "False Alarm";

pub fn make(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(FalseAlarm)
}

struct FalseAlarm;

impl RoundEvent for FalseAlarm {
    fn setup(&mut self, _: &mut World, ev: &mut EventState) {
        ev.announce_when = 0;
        ev.end_when = 1;
    }

    fn announce(&mut self, world: &mut World, _: &mut EventState, fake: bool) {
        if fake {
            return;
        }
        let now = world.resource::<Time>().elapsed_secs_f64();
        let players = active_players(world);
        let round = world
            .get_resource::<crate::round::Ticker>()
            .map_or(0.0, |t| t.round_duration(now));
        let evacuated = crate::shuttle::evacuated(world);
        let mut pool = world.resource::<Events>().false_alarm_pool();
        // `pick_n_take` until one can spawn.
        while !pool.is_empty() {
            let i = world
                .resource_mut::<EventRng>()
                .0
                .range(0, pool.len() as i64 - 1) as usize;
            let control = pool.swap_remove(i);
            if !world
                .resource::<Events>()
                .can_spawn(control, players, round, evacuated)
            {
                continue;
            }
            let (make, name) = {
                let c = &world.resource::<Events>().controls[control];
                (c.make, c.name)
            };
            info!("False Alarm: {name}");
            let mut fake_event = make(world);
            fake_event.announce(world, &mut EventState::new(control), true);
            return;
        }
    }
}
