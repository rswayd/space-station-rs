//! Market Crash (`code/modules/events/market_crash.dm`): vending prices go up for 50-100
//! fires (1.7-3.3 minutes), announced at both ends.
//!
//! While it runs, `SSeconomy.inflation_value = 5.5 * log(activeFor + 1)` multiplies the station
//! vendors' prices (`crate::vending::update_prices`), refreshed every 5 ticks; the end puts
//! them back. (tg's `tick_counter = tick_counter++` never counts and its start refreshes the
//! prices before the crash trait is set; the intended refresh is ported.)

use bevy::prelude::*;

use super::controller::{EventRng, EventState, RoundEvent};

pub const NAME: &str = "Market Crash";

const REASONS: [&str; 9] = [
    "выравнивания луны и солнца",
    "некоторых рискованных результаты на рынке жилья",
    "сильно преувеличенных сообщений об \"увольнении\" бухгалтерского состава Нанотрейзен",
    "\"огромного вложения\" в \"НФТ\" \"идиотом\"",
    "рейдов со стороны агентов Кооператива \"Тигр\"",
    "сокращения цепочек поставок",
    "падения акций социальной сети \"Нанотрейзен+\"",
    "неожиданного успеха социальной сети \"Нанотрейзен+\"",
    "неудачного стечения обстоятельств",
];
const SENDER_TITLE: &str = "Отдел бухгалтерского учета Нанотрейзен";

pub fn make(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(MarketCrash { ticks: 0 })
}

struct MarketCrash {
    ticks: u32,
}

/// `5.5 * log(activeFor + 1)` (DM `log` with one argument is natural), at least 1.
pub fn inflation(active_for: u32) -> f32 {
    (5.5 * (f64::from(active_for) + 1.0).ln()).max(1.0) as f32
}

fn set_prices(world: &mut World, value: f32) {
    world.insert_resource(crate::vending::VendingInflation(value));
    crate::vending::update_prices(world);
}

impl RoundEvent for MarketCrash {
    fn setup(&mut self, world: &mut World, ev: &mut EventState) {
        ev.start_when = 1;
        // `rand(100, 50)`.
        ev.end_when = world.resource_mut::<EventRng>().0.range(50, 100) as u32;
        ev.announce_when = 2;
    }

    fn announce(&mut self, world: &mut World, _: &mut EventState, _: bool) {
        let reason = *world
            .resource_mut::<EventRng>()
            .pick(&REASONS)
            .expect("non-empty");
        let text = format!(
            "Из-за {reason}, цены в торговых автоматах на станции будут повышены на короткий \
             период."
        );
        super::priority_announce(world, SENDER_TITLE, &text);
    }

    fn start(&mut self, world: &mut World, ev: &mut EventState) {
        set_prices(world, inflation(ev.active_for));
    }

    fn tick(&mut self, world: &mut World, ev: &mut EventState) {
        self.ticks += 1;
        if self.ticks.is_multiple_of(5) {
            set_prices(world, inflation(ev.active_for));
        }
    }

    fn end(&mut self, world: &mut World, _: &mut EventState) {
        set_prices(world, 1.0);
        super::priority_announce(
            world,
            SENDER_TITLE,
            "Цены в торговых автоматах на станции стабилизировались.",
        );
    }
}
