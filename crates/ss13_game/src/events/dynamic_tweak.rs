//! Dynamic Tweak (`code/modules/events/dynamic_tweak.dm`): a hidden event giving dynamic one
//! more light midround, heavy midround or latejoin ruleset to spawn (at least one heavy or
//! latejoin; `crate::antag::midround::Midround::to_spawn`). Not before 20 minutes, never past
//! the emergency shuttle's point of no return.
// shortcut: `max_occurrences` stays 1 (tg's control `New()` adds `rand(-1, 1)`; the controls
// are built without a random stream); `SSdynamic.antag_events_enabled` has no admin toggle
// here and is always on.

use bevy::prelude::*;

use super::controller::{EventRng, EventState, RoundEvent};
use crate::antag::midround::Midround;

pub const NAME: &str = "Dynamic Tweak";

pub fn make(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(DynamicTweak)
}

/// `can_spawn_event`: dynamic runs and the shuttle has not passed the point of no return.
pub fn can_run(world: &mut World) -> bool {
    world.get_resource::<Midround>().is_some_and(|m| m.started)
        && !crate::antag::midround::shuttle_past_no_return(world)
}

/// `start()`'s rolls: light, heavy, latejoin; one of the last two at least.
pub fn roll(rng: &mut EventRng) -> [u32; 3] {
    let lights = rng.0.range(0, 1);
    let heavies = rng.0.range(1 - lights, 1);
    let latejoins = rng.0.range(1 - heavies, 1);
    [lights as u32, heavies as u32, latejoins as u32]
}

struct DynamicTweak;

impl RoundEvent for DynamicTweak {
    fn setup(&mut self, _: &mut World, ev: &mut EventState) {
        ev.start_when = 1;
        ev.end_when = 2;
    }

    fn start(&mut self, world: &mut World, _: &mut EventState) {
        let added = roll(&mut world.resource_mut::<EventRng>());
        let Some(mut midround) = world.get_resource_mut::<Midround>() else {
            return;
        };
        for (to_spawn, add) in midround.to_spawn.iter_mut().zip(added) {
            *to_spawn += add;
        }
        info!(
            "Event: Dynamic Tweak added {} latejoin(s), {} light midround(s) and {} heavy \
             midround(s).",
            added[2], added[0], added[1]
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_heavy_or_a_latejoin_always_comes() {
        let mut rng = EventRng(ss13_mapload::spawner::Rng::new(3));
        for _ in 0..500 {
            let [light, heavy, latejoin] = roll(&mut rng);
            assert!(light <= 1 && heavy <= 1 && latejoin <= 1);
            assert!(light + heavy >= 1, "a light or a heavy");
            assert!(heavy + latejoin >= 1, "a heavy or a latejoin");
        }
    }
}
