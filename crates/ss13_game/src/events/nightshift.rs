//! Night Shift (`code/modules/events/nightshift.dm`): for ~23 minutes (700 fires) every station
//! APC's night shift is on (`crate::power::set_nightshift`: dimmer, warmer lights), announced
//! by the automated lighting system at both ends. At red alert or above it is off until the
//! level drops again, with an announcement each way.
//!
//! BandaStation's config leaves `ENABLE_NIGHT_SHIFTS` off, so the control has no occurrences
//! (tg `max_occurrences = 0`): it runs only when forced.
//!
//! tg switches the lights from `announce` (none without the announcement) and its
//! `update_machines` sets every APC to `!nightshift_disabled` whatever `active` is, so the end
//! never turned the lights back; the intended behaviour is ported: on at the start, back to
//! normal at the end.
// shortcut: all APCs switch at once (tg spreads them over ticks for lag); no grey announcement
// colour.

use bevy::prelude::*;

use super::controller::{EventState, RoundEvent};
use crate::power::Apc;
use crate::security_level::SecLevel;

pub const NAME: &str = "Night Shift";

const SENDER: &str = "Automated Lighting System Announcement";
const SOUND: &str = "sound/announcer/notice/notice2.ogg";

pub fn make(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(NightShift { disabled: false })
}

struct NightShift {
    /// tg `nightshift_disabled`: off for an emergency (red alert or above).
    disabled: bool,
}

fn send_announcement(world: &mut World, text: &str) {
    crate::announce::Priority::new(text, "")
        .sender(SENDER)
        .sound(SOUND)
        .send(world);
}

/// The APCs of `/area/station` areas.
fn station_apcs(world: &mut World) -> Vec<Entity> {
    let mut query = world.query::<(Entity, &Apc)>();
    let apcs: Vec<(Entity, Entity)> = query.iter(world).map(|(e, a)| (e, a.area)).collect();
    apcs.into_iter()
        .filter(|&(_, area)| {
            super::area_type(world, area)
                .is_some_and(|p| crate::obj_damage::is_under(p, "/area/station"))
        })
        .map(|(e, _)| e)
        .collect()
}

/// `update_machines` with the intended state.
fn set_station_nightshift(world: &mut World, on: bool) {
    for apc in station_apcs(world) {
        crate::power::set_nightshift(world, apc, on);
    }
}

/// `SSsecurity_level.get_current_level_as_number() >= SEC_LEVEL_RED`.
pub fn emergency(world: &World) -> bool {
    crate::security_level::current(world).number() >= SecLevel::Red.number()
}

impl RoundEvent for NightShift {
    fn setup(&mut self, _: &mut World, ev: &mut EventState) {
        ev.announce_when = 1;
        ev.start_when = 1;
        ev.end_when = 700;
    }

    fn announce(&mut self, world: &mut World, _: &mut EventState, _: bool) {
        send_announcement(
            world,
            "Добрый вечер, экипаж. Для снижения энергопотребления и поддержания циркадных \
             ритмов некоторых видов всё освещение на станции было приглушено на ночь.",
        );
    }

    fn start(&mut self, world: &mut World, _: &mut EventState) {
        set_station_nightshift(world, !self.disabled);
    }

    fn tick(&mut self, world: &mut World, _: &mut EventState) {
        let emergency = emergency(world);
        if self.disabled == emergency {
            return;
        }
        self.disabled = emergency;
        if emergency {
            send_announcement(
                world,
                "Отключение ночного освещения: станция в режиме чрезвычайной ситуации.",
            );
        } else {
            send_announcement(
                world,
                "Восстановление штатного режима работы ночного освещения.",
            );
        }
        set_station_nightshift(world, !emergency);
    }

    fn end(&mut self, world: &mut World, _: &mut EventState) {
        send_announcement(
            world,
            "Доброе утро, экипаж. С наступлением дневного времени всё освещение на станции \
             возвращено к обычной яркости.",
        );
        set_station_nightshift(world, false);
    }
}
