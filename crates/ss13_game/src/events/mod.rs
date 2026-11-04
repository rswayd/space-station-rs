//! Random events (server): tg `SSevents` and the round events that act on systems ss13-rs has
//! (`code/modules/events/`), with BandaStation's Russian announcement texts.
//!
//! - [`controller`]: the scheduler, the event lifecycle, forced events.
//! - One file per event family; [`controls`] lists them with tg's control vars (weight,
//!   `earliest_start`, `min_players`, `max_occurrences`).
//! - [`explosion`]: `ex_act` and a radius stand-in for `explosion()`, used by meteors.
//! - [`carp_migration`]: spawns basic mobs (`crate::mobs`).
//!
//! Announcements are tg `priority_announce` from Central Command (`crate::announce`), with the
//! announcer's `ANNOUNCER_*` sound where tg names one, else its random alert sound.

pub mod brand_intelligence;
pub mod carp_migration;
pub mod comms_blackout;
pub mod controller;
pub mod electrical_storm;
pub mod explosion;
pub mod false_alarm;
pub mod grid_check;
pub mod market_crash;
pub mod meteors;
pub mod radiation_storm;

use bevy::prelude::*;

pub use controller::{
    EventControl, EventRng, EventState, Events, EventsConfig, RoundEvent, force_event,
};

/// tg `command_name()`: the default sender of `priority_announce`, with `": Сообщение"`.
pub const COMMAND_NAME: &str = "Центральное Командование";

/// tg `priority_announce(text, title)` with no sender override.
pub fn priority_announce(world: &mut World, title: &str, text: &str) {
    crate::announce::Priority::new(text, title).send(world);
}

/// tg `priority_announce(text, title, ANNOUNCER_*)`.
pub fn priority_announce_sound(world: &mut World, title: &str, text: &str, sound: &str) {
    crate::announce::Priority::new(text, title)
        .sound(sound)
        .send(world);
}

/// tg `station_name()`.
pub fn station_name(world: &World) -> String {
    crate::round::station_name(world)
}

const MINUTE: f64 = 60.0;

/// The ported controls with tg's vars (`code/modules/events/*.dm`); BandaStation changes none
/// of them in code (only `/datum/round_event_control/operative`, not ported).
pub fn controls() -> Vec<EventControl> {
    let c = |name, weight, earliest_min: f64, min_players, max_occurrences, make| EventControl {
        name,
        weight,
        earliest_start: earliest_min * MINUTE,
        min_players,
        max_occurrences,
        occurrences: 0,
        fakeable: true,
        admin_disabled: false,
        make,
    };
    let unfakeable = |control: EventControl| EventControl {
        fakeable: false,
        ..control
    };
    vec![
        c(grid_check::NAME, 10, 20.0, 0, 3, grid_check::make),
        c(
            electrical_storm::NAME,
            20,
            10.0,
            5,
            20,
            electrical_storm::make,
        ),
        c(comms_blackout::NAME, 30, 20.0, 0, 20, comms_blackout::make),
        c(radiation_storm::NAME, 10, 20.0, 0, 1, radiation_storm::make),
        c(
            brand_intelligence::NAME,
            5,
            20.0,
            15,
            1,
            brand_intelligence::make,
        ),
        c(market_crash::NAME, 10, 20.0, 0, 20, market_crash::make),
        c(carp_migration::NAME, 15, 10.0, 12, 6, carp_migration::make),
        unfakeable(c(false_alarm::NAME, 20, 20.0, 0, 5, false_alarm::make)),
        c(meteors::WAVE_NORMAL, 4, 25.0, 15, 3, meteors::make_normal),
        c(
            meteors::WAVE_THREATENING,
            5,
            35.0,
            20,
            3,
            meteors::make_threatening,
        ),
        c(
            meteors::WAVE_CATASTROPHIC,
            7,
            45.0,
            25,
            3,
            meteors::make_catastrophic,
        ),
        c(meteors::WAVE_MEATY, 2, 25.0, 15, 1, meteors::make_meaty),
        c(
            meteors::DUST_STORM,
            14,
            15.0,
            15,
            3,
            meteors::make_dust_storm,
        ),
        unfakeable(c(
            meteors::SPACE_DUST,
            200,
            0.0,
            0,
            1000,
            meteors::make_space_dust,
        )),
        unfakeable(c(
            meteors::STRAY_METEOR,
            15,
            20.0,
            15,
            3,
            meteors::make_stray,
        )),
    ]
}

pub struct EventsServerPlugin;

impl Plugin for EventsServerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EventsConfig>()
            .init_resource::<grid_check::GridCheckCooldown>()
            .add_systems(Startup, controller::init_events)
            .add_systems(
                Update,
                (
                    controller::events_fire,
                    meteors::move_meteors,
                    radiation_storm::process_storm,
                )
                    .chain(),
            );
    }
}
