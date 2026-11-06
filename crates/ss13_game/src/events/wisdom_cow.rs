//! Wisdom Cow (`code/modules/events/wisdomcow.dm`): a wisdom cow (`crate::mobs::farm`, its
//! wisdom, experience and random milk rolled at spawn) appears in smoke on a random safe
//! station floor, announced by the cow breeding agency.
// shortcut: no admin setup (location, wisdom, experience, milk); no ghost notification.

use bevy::prelude::*;

use super::controller::{EventState, RoundEvent};
use crate::mobs::farm::WISDOM_COW;

pub const NAME: &str = "Wisdom Cow";

pub fn make(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(WisdomCowEvent)
}

struct WisdomCowEvent;

impl RoundEvent for WisdomCowEvent {
    fn announce(&mut self, world: &mut World, _: &mut EventState, _: bool) {
        super::priority_announce(
            world,
            "Агентство по разведению коров Нанотрейзен",
            "В окрестностях станции была замечена мудрая корова. Не забудьте спросить у нее \
             совета.",
        );
    }

    fn start(&mut self, world: &mut World, _: &mut EventState) {
        let Some(pos) = super::electrical_storm::random_station_turf(world) else {
            return;
        };
        let Some(cow) = crate::mobs::spawn::spawn_mob(world, WISDOM_COW, pos) else {
            warn!("{NAME}: no {WISDOM_COW} prototype");
            return;
        };
        crate::effects::smoke::do_smoke(
            world,
            1,
            pos,
            None,
            crate::effects::smoke::SmokeKind::Basic,
        );
        info!("{NAME}: {cow} at {pos:?}");
    }
}
