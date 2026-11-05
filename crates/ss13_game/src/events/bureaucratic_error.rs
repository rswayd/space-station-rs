//! Bureaucratic Error (`code/modules/events/bureaucratic_error.dm`): the latejoin slots of the
//! jobs that allow it (`SSjob.get_valid_overflow_jobs()`) change: a third of the time one of
//! them becomes unlimited and every other one closes, otherwise each gains `rand(-2, 4)`
//! slots (not below 0).

use bevy::prelude::*;

use super::controller::{EventRng, EventState, RoundEvent};
use crate::jobs::Jobs;

pub const NAME: &str = "Bureaucratic Error";
/// The jobs with `allow_bureaucratic_error = FALSE` (the overflow role is excluded too, by
/// `set_overflow_role`).
const PROTECTED: [&str; 6] = [
    "/datum/job/ai",
    "/datum/job/pun_pun",
    "/datum/job/bridge_assistant",
    "/datum/job/veteran_advisor",
    "/datum/job/human_ai",
    "/datum/job/cargo_gorilla",
];

pub fn make(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(BureaucraticError)
}

struct BureaucraticError;

impl RoundEvent for BureaucraticError {
    fn setup(&mut self, _: &mut World, ev: &mut EventState) {
        ev.announce_when = 1;
    }

    fn announce(&mut self, world: &mut World, _: &mut EventState, _: bool) {
        super::priority_announce(
            world,
            "Предупреждение о бюрократической ошибке.",
            "Недавняя бюрократическая ошибка в отделе органических ресурсов может привести к \
             нехватке кадров в одних отделах и избытку в других.",
        );
    }

    fn start(&mut self, world: &mut World, _: &mut EventState) {
        world.resource_scope(|world, mut rng: Mut<EventRng>| {
            if let Some(mut jobs) = world.get_resource_mut::<Jobs>() {
                shuffle_slots(&mut jobs, &mut rng);
            }
        });
    }
}

/// `/datum/round_event/bureaucratic_error/start()`.
pub fn shuffle_slots(jobs: &mut Jobs, rng: &mut EventRng) {
    let overflow = jobs.default_job;
    let mut valid: Vec<usize> = (0..jobs.data.jobs.len())
        .filter(|&j| j != overflow && !PROTECTED.contains(&jobs.data.jobs[j].path.as_str()))
        .collect();
    if valid.is_empty() {
        return;
    }
    if rng.0.prob(33.0) {
        let i = rng.0.range(0, valid.len() as i64 - 1) as usize;
        let only = valid.swap_remove(i);
        let job = &mut jobs.data.jobs[only];
        job.spawn_positions = -1;
        job.total_positions = -1;
        for j in valid {
            jobs.data.jobs[j].total_positions = 0;
        }
        return;
    }
    for j in valid {
        let job = &mut jobs.data.jobs[j];
        job.total_positions = (job.total_positions + rng.0.range(-2, 4) as i32).max(0);
    }
}
