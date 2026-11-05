//! Radiation Storm (`code/modules/events/radiation_storm.dm`): the announcement, then
//! `SSweather.run_weather(/datum/weather/rad_storm)` (`crate::weather`).

use bevy::prelude::*;

use super::controller::{EventState, RoundEvent};
use crate::weather::WeatherKind;

pub const NAME: &str = "Radiation Storm";

pub fn make(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(RadiationStormEvent)
}

struct RadiationStormEvent;

impl RoundEvent for RadiationStormEvent {
    fn setup(&mut self, _: &mut World, ev: &mut EventState) {
        ev.start_when = 3;
        ev.end_when = ev.start_when + 1;
        ev.announce_when = 1;
    }

    fn announce(&mut self, world: &mut World, _: &mut EventState, _: bool) {
        super::priority_announce_sound(
            world,
            "Обнаружена аномалия",
            "Вблизи станции обнаружено радиационное поле высокой интенсивности. Всему персоналу \
             надлежит проследовать в технические тоннели.",
            crate::announce::ANNOUNCER_RADIATION,
        );
    }

    fn start(&mut self, world: &mut World, _: &mut EventState) {
        run_weather(world);
    }
}

/// `SSweather.run_weather(/datum/weather/rad_storm)` on the station's levels.
pub fn run_weather(world: &mut World) {
    crate::weather::run_weather(world, WeatherKind::RadStorm, None);
}
