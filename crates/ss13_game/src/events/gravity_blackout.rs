//! Gravity Generator Blackout (`code/modules/events/gravity_generator_blackout.dm`): every main
//! gravity generator on a station level is blacked out
//! ([`crate::power::gravity_generator::blackout`]: breaker off, charge emptied) and needs a
//! manual restart. Announced 33% of the time with `ANNOUNCER_GRANOMALIES`.

use bevy::prelude::*;
use ss13_shared::TilePos;

use super::controller::{EventState, RoundEvent, station_levels};
use crate::power::gravity_generator::GravityGenerator;

pub const NAME: &str = "Gravity Generator Blackout";

pub fn make(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(GravityBlackout)
}

/// The main generators on station levels.
fn station_generators(world: &mut World) -> Vec<Entity> {
    let levels = station_levels(world);
    let mut query = world.query_filtered::<(Entity, &TilePos), With<GravityGenerator>>();
    query
        .iter(world)
        .filter(|(_, pos)| levels.contains(&pos.z))
        .map(|(e, _)| e)
        .collect()
}

/// The control's `can_spawn_event`: a main generator on a station level.
pub fn has_station_generator(world: &mut World) -> bool {
    !station_generators(world).is_empty()
}

struct GravityBlackout;

impl RoundEvent for GravityBlackout {
    fn setup(&mut self, _: &mut World, ev: &mut EventState) {
        ev.announce_when = 1;
        ev.start_when = 1;
        ev.announce_chance = 33.0;
    }

    fn announce(&mut self, world: &mut World, _: &mut EventState, _fake: bool) {
        let text = format!(
            "Гравносферные аномалии обнаружены вблизи {}. Требуется ручная перезагрузка \
             генераторов гравитации.",
            super::station_name(world)
        );
        super::priority_announce_sound(
            world,
            "Обнаружена аномалия",
            &text,
            crate::announce::ANNOUNCER_GRANOMALIES,
        );
    }

    fn start(&mut self, world: &mut World, _: &mut EventState) {
        for generator in station_generators(world) {
            crate::power::gravity_generator::blackout(world, generator);
        }
    }
}
