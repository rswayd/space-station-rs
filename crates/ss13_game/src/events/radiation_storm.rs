//! Radiation Storm (`code/modules/events/radiation_storm.dm`) and its weather
//! (`/datum/weather/rad_storm`, `code/datums/weather/`): a 40 s telegraph, 1-2.5 minutes of
//! storm over every area of the station levels except maintenance and the shelters, 10 s of
//! wind-down, then the all-clear.
//!
//! During the storm, every SSweather fire (1 s), each living human in an impacted area rolls
//! `prob(40)` to mutate (`weather_act_mob`).
// shortcut: no unique features to scramble; no area overlay, status display alert or sounds.

use bevy::prelude::*;
use ss13_mapload::{Areas, DmPower};
use ss13_net::chat::server::SendChat;
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};

use super::controller::{EventRng, EventState, RoundEvent, station_levels};
use crate::genetics::{self, Dna};
use crate::ghost::Ghost;
use crate::health::{Health, Stat};
use crate::obj_damage::is_under;
use crate::server::PlayerMob;

pub const NAME: &str = "Radiation Storm";
/// `/datum/weather/rad_storm`: `telegraph_duration`, `weather_duration_lower/upper`,
/// `end_duration`.
const TELEGRAPH_SECS: f64 = 40.0;
const DURATION_SECS: (i64, i64) = (60, 150);
const END_SECS: f64 = 10.0;
/// `SSweather.wait`.
const WEATHER_WAIT_SECS: f64 = 1.0;
/// `negative_mutation_chance`.
const NEGATIVE_MUTATION_CHANCE: f32 = 90.0;
/// `mutate_chance`.
const MUTATE_CHANCE: f32 = 40.0;
/// `protected_areas`.
const PROTECTED_AREAS: [&str; 12] = [
    "/area/station/maintenance",
    "/area/shuttle",
    "/area/station/ai/satellite/maintenance",
    "/area/station/ai/upload",
    "/area/station/ai/satellite/chamber",
    "/area/station/commons/storage/emergency/starboard",
    "/area/station/commons/storage/emergency/port",
    "/area/station/security/prison/safe",
    "/area/station/security/prison/toilet",
    "/area/mine/maintenance",
    "/area/ruin/comms_agent/maint",
    "/area/icemoon/underground",
];

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Telegraph,
    Main,
    WindDown,
}

/// A running `/datum/weather/rad_storm`.
#[derive(Resource, Clone, Debug)]
pub struct RadStorm {
    pub stage: Stage,
    /// When the stage ends (server seconds).
    pub stage_end: f64,
    pub duration: f64,
    pub levels: Vec<u16>,
    next_fire: f64,
}

/// `SSweather.run_weather(/datum/weather/rad_storm)`: `telegraph()`.
pub fn run_weather(world: &mut World) {
    if world.contains_resource::<RadStorm>() {
        return;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    let duration = world
        .resource_mut::<EventRng>()
        .0
        .range(DURATION_SECS.0, DURATION_SECS.1) as f64;
    let levels: Vec<u16> = station_levels(world).collect();
    world.insert_resource(RadStorm {
        stage: Stage::Telegraph,
        stage_end: now + TELEGRAPH_SECS,
        duration,
        levels,
        next_fire: now,
    });
    send_alert(
        world,
        Span::Danger,
        "Вам кажется, что воздух вокруг становится теплее.",
    );
}

/// The weather's timers and, in the main stage, its effect on mobs.
pub(super) fn process_storm(world: &mut World) {
    let Some(mut storm) = world.get_resource::<RadStorm>().cloned() else {
        return;
    };
    let now = world.resource::<Time>().elapsed_secs_f64();
    if storm.stage_end <= now {
        match storm.stage {
            // `start()`.
            Stage::Telegraph => {
                storm.stage = Stage::Main;
                storm.stage_end = now + storm.duration;
                storm.next_fire = now;
                world.insert_resource(storm.clone());
                send_alert(
                    world,
                    Span::BoldWarning,
                    "Вы чувствуете, как вас окутывают волны тепла! Найдите убежище!",
                );
            }
            // `wind_down()`.
            Stage::Main => {
                storm.stage = Stage::WindDown;
                storm.stage_end = now + END_SECS;
                world.insert_resource(storm.clone());
                send_alert(world, Span::Notice, "Кажется, воздух снова остывает.");
            }
            // `end()`.
            Stage::WindDown => {
                world.remove_resource::<RadStorm>();
                super::priority_announce(
                    world,
                    "Обнаружена аномалия",
                    "Радиационная угроза миновала. Пожалуйста, вернитесь на свои рабочие места.",
                );
                return;
            }
        }
    }
    if storm.stage != Stage::Main || storm.next_fire > now {
        return;
    }
    world.resource_mut::<RadStorm>().next_fire = now + WEATHER_WAIT_SECS;
    let mut q = world.query_filtered::<(Entity, &TilePos), (With<Health>, Without<Ghost>)>();
    let mobs: Vec<(Entity, TilePos)> = q.iter(world).map(|(e, p)| (e, *p)).collect();
    for (mob, pos) in mobs {
        if !storm.levels.contains(&pos.z) || !impacted(world, pos) {
            continue;
        }
        weather_act_mob(world, mob);
    }
}

/// `can_weather_act_mob`: the mob's area is one of `area_type` (`/area`) and not protected.
fn impacted(world: &World, pos: TilePos) -> bool {
    let Some(area) = world.resource::<Areas>().area_at(pos) else {
        return false;
    };
    let path = world.get::<DmPower>(area).map_or("", |p| p.path.as_str());
    !PROTECTED_AREAS.iter().any(|root| is_under(path, root))
}

/// `/datum/weather/rad_storm/weather_act_mob`: humans (`can_mutate`) that are not radiation
/// immune or covered in radiation-protected clothing get a unique identity block scrambled and half the time `do_mutate`: a random negative (90%) or positive mutation.
fn weather_act_mob(world: &mut World, mob: Entity) {
    let mut rng = world.resource_mut::<EventRng>();
    if !rng.0.prob(MUTATE_CHANCE) {
        return;
    }
    let do_mutate = rng.0.prob(50.0);
    let negative = rng.0.prob(NEGATIVE_MUTATION_CHANCE);
    if world.get::<Dna>(mob).is_none() || world.get::<Stat>(mob) == Some(&Stat::Dead) {
        return;
    }
    if crate::radiation::rad_immune(world, mob)
        || crate::radiation::wearing_rad_protected_clothing(world, mob)
    {
        return;
    }
    genetics::random_mutate_unique_identity(world, mob);
    if do_mutate {
        let quality = if negative {
            genetics::NEGATIVE | genetics::MINOR_NEGATIVE
        } else {
            genetics::POSITIVE
        };
        genetics::easy_random_mutate(world, mob, quality);
        genetics::domutcheck(world, mob);
    }
}

/// `send_alert(msg)`: every player on the storm's levels.
fn send_alert(world: &mut World, span: Span, text: &str) {
    let levels = world
        .get_resource::<RadStorm>()
        .map(|s| s.levels.clone())
        .unwrap_or_default();
    let mut q = world.query::<(&PlayerMob, &TilePos)>();
    let clients: Vec<Entity> = q
        .iter(world)
        .filter(|(_, pos)| levels.contains(&pos.z))
        .map(|(p, _)| p.client)
        .collect();
    world.write_message(SendChat {
        clients,
        message: ChatMessage {
            channel: ChatChannel::Warning,
            segments: vec![ChatSegment::new(span, text)],
        },
    });
}
