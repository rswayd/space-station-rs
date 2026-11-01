//! Market Crash (`code/modules/events/market_crash.dm`): vending prices go up for 50-100
//! fires (1.7-3.3 minutes), announced at both ends.
// shortcut: vending charges nothing yet (`ss13_game::vending`), so the crash only announces; add
// `SSeconomy.inflation_value = 5.5 * log(activeFor + 1)` with vending prices.

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
    Box::new(MarketCrash)
}

struct MarketCrash;

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

    fn end(&mut self, world: &mut World, _: &mut EventState) {
        super::priority_announce(
            world,
            SENDER_TITLE,
            "Цены в торговых автоматах на станции стабилизировались.",
        );
    }
}
