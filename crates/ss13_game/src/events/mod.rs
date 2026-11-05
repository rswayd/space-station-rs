//! Random events (server): tg `SSevents` and the round events that act on systems ss13-rs has
//! (`code/modules/events/`), with BandaStation's Russian announcement texts.
//!
//! - [`controller`]: the scheduler, the event lifecycle, forced events.
//! - One file per event family; [`controls`] lists them with tg's control vars (weight,
//!   `earliest_start`, `min_players`, `max_occurrences`).
//! - [`carp_migration`], [`mice_migration`], [`vent_clog`]: spawn basic mobs (`crate::mobs`).
//! - [`anomalies`], [`immovable_rod`], [`wormholes`]: their own objects, moved and processed
//!   by this plugin's systems.
//!
//! Announcements are tg `priority_announce` from Central Command (`crate::announce`), with the
//! announcer's `ANNOUNCER_*` sound where tg names one, else its random alert sound.

pub mod anomalies;
pub mod brand_intelligence;
pub mod bureaucratic_error;
pub mod camera_failure;
pub mod carp_migration;
pub mod comms_blackout;
pub mod controller;
pub mod electrical_storm;
pub mod false_alarm;
pub mod grey_tide;
pub mod grid_check;
pub mod immovable_rod;
pub mod market_crash;
pub mod meteors;
pub mod mice_migration;
pub mod processor_overload;
pub mod radiation_storm;
pub mod vent_clog;
pub mod wormholes;

use bevy::prelude::*;
use ss13_shared::TilePos;

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

/// The type of an area (areas carry it in [`ss13_mapload::DmPower`]).
pub fn area_type(world: &World, area: Entity) -> Option<&str> {
    world
        .get::<ss13_mapload::DmPower>(area)
        .map(|p| p.path.as_str())
}

/// The type of the area at `pos`.
pub fn area_type_at(world: &World, pos: TilePos) -> Option<&str> {
    let area = world.get_resource::<ss13_mapload::Areas>()?.area_at(pos)?;
    area_type(world, area)
}

/// The tiles of `/area/station` areas on station levels, with their area (tg
/// `GLOB.station_turfs`). One scan of the station levels (~65k tiles on MetaStation): for
/// events that pick from them once.
pub fn station_tiles(world: &World) -> Vec<(Entity, TilePos)> {
    let Some(areas) = world.get_resource::<ss13_mapload::Areas>() else {
        return Vec::new();
    };
    let index = world.resource::<ss13_world::SpatialIndex>();
    let mut tiles = Vec::new();
    for z in controller::station_levels(world) {
        let Some((w, h)) = index.level_size(z) else {
            continue;
        };
        for y in 1..=h as i32 {
            for x in 1..=w as i32 {
                let pos = TilePos::new(x, y, z);
                let Some(area) = areas.area_at(pos) else {
                    continue;
                };
                if area_type(world, area)
                    .is_some_and(|p| crate::obj_damage::is_under(p, "/area/station"))
                {
                    tiles.push((area, pos));
                }
            }
        }
    }
    tiles
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
        c(
            processor_overload::NAME,
            15,
            20.0,
            20,
            20,
            processor_overload::make,
        ),
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
        unfakeable(c(
            camera_failure::NAME,
            100,
            20.0,
            0,
            20,
            camera_failure::make,
        )),
        c(grey_tide::NAME, 10, 20.0, 5, 2, grey_tide::make),
        c(
            bureaucratic_error::NAME,
            5,
            20.0,
            0,
            1,
            bureaucratic_error::make,
        ),
        c(immovable_rod::NAME, 10, 20.0, 15, 5, immovable_rod::make),
        c(wormholes::NAME, 2, 20.0, 2, 3, wormholes::make),
        c(mice_migration::NAME, 10, 20.0, 0, 20, mice_migration::make),
        c(vent_clog::MINOR, 25, 5.0, 0, 20, vent_clog::make_minor),
        c(vent_clog::MAJOR, 12, 10.0, 0, 5, vent_clog::make_major),
        c(
            vent_clog::CRITICAL,
            8,
            25.0,
            15,
            3,
            vent_clog::make_critical,
        ),
        c(vent_clog::STRANGE, 5, 5.0, 0, 2, vent_clog::make_strange),
        c(anomalies::FLUX, 20, 20.0, 10, 5, anomalies::make_flux),
        c(
            anomalies::BLUESPACE,
            15,
            20.0,
            1,
            1,
            anomalies::make_bluespace,
        ),
        c(anomalies::GRAV, 25, 20.0, 1, 5, anomalies::make_grav),
        c(anomalies::PYRO, 20, 20.0, 1, 5, anomalies::make_pyro),
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
                    immovable_rod::move_rods,
                    anomalies::process_anomalies,
                    radiation_storm::process_storm,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                (wormholes::enter_wormholes, anomalies::anomaly_contacts)
                    .after(ss13_world::WorldSystems::Movement),
            );
    }
}
