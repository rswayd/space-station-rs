//! Spontaneous Brain Trauma (`code/modules/events/brain_trauma.dm`): a random living crew human
//! with a client and a brain gains a random trauma (`gain_trauma_type`,
//! [`crate::medical::traumas::gain_type`]): mild 60%, severe 30%, special 10%; resilience basic
//! 50%, surgery 30%, lobotomy 15%, magic 5%. An unported pick gives nothing, so each ported
//! trauma comes as often as in tg.
// shortcut: no ghost notification.

use bevy::prelude::*;

use super::controller::{EventRng, EventState, RoundEvent};
use crate::combat::CombatRng;
use crate::health::Stat;
use crate::medical::organs::{self, Slot};
use crate::medical::traumas::{self, Category, Resilience};

pub const NAME: &str = "Spontaneous Brain Trauma";

pub fn make(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(BrainTrauma)
}

struct BrainTrauma;

/// The weighted pick of `traumatize()`: `(category, resilience)` from two rolls.
pub fn roll(rng: &mut CombatRng) -> (Category, Resilience) {
    let resilience = match rng.range(1, 100) {
        1..=50 => Resilience::Basic,
        51..=80 => Resilience::Surgery,
        81..=95 => Resilience::Lobotomy,
        _ => Resilience::Magic,
    };
    let category = match rng.range(1, 100) {
        1..=60 => Category::Mild,
        61..=90 => Category::Severe,
        _ => Category::Special,
    };
    (category, resilience)
}

impl RoundEvent for BrainTrauma {
    fn start(&mut self, world: &mut World, _: &mut EventState) {
        let mut query = world.query_filtered::<(Entity, &Stat), (
            With<crate::server::PlayerMob>,
            With<crate::jobs::PlayerJob>,
            With<crate::human::HumanLook>,
        )>();
        let mut pool: Vec<Entity> = query
            .iter(world)
            .filter(|(_, stat)| **stat != Stat::Dead)
            .map(|(e, _)| e)
            .collect();
        pool.sort();
        let mut rng = CombatRng(world.resource_mut::<EventRng>().0.next_u64());
        // `shuffle(GLOB.alive_mob_list)`, the first with a brain.
        while !pool.is_empty() {
            let mob = pool.swap_remove(rng.range(0, pool.len() as u32 - 1) as usize);
            let Some(brain) = organs::organ(world, mob, Slot::Brain) else {
                continue;
            };
            let (category, resilience) = roll(&mut rng);
            let gained =
                traumas::gain_type(world, brain, category, Some(resilience), false, &mut rng);
            info!("{NAME}: {mob} rolled {category:?} ({resilience:?}): {gained:?}");
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roll_follows_the_weights() {
        let mut rng = CombatRng(11);
        let n = 20_000;
        let (mut mild, mut magic) = (0, 0);
        for _ in 0..n {
            let (category, resilience) = roll(&mut rng);
            mild += u32::from(category == Category::Mild);
            magic += u32::from(resilience == Resilience::Magic);
        }
        let share = |x: u32| f64::from(x) / f64::from(n);
        assert!((share(mild) - 0.6).abs() < 0.02, "{mild}");
        assert!((share(magic) - 0.05).abs() < 0.01, "{magic}");
    }
}
