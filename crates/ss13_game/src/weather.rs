//! Weather (server), after tg `SSweather` (`code/controllers/subsystem/weather.dm`),
//! `/datum/weather` (`code/datums/weather/weather.dm`) and the station weathers of
//! `weather_types/`: the radiation storm (and its endless nebula kind), the floor is lava, the
//! void storm, and lavaland's ash storm and emberfall.
//!
//! - [`run_weather`] (`SSweather.run_weather`) starts a weather on z-levels (by default the
//!   levels of its target trait: the station's): it picks its impacted areas once (every area
//!   of its `area_type`, not of a `protected_areas` type, outdoors unless the weather reaches
//!   indoors) and telegraphs.
//! - Stages (`telegraph`, `start`, `wind_down`, `end`) follow their durations (the main stage
//!   `rand(lower, upper)`; an endless weather stays until [`end_weather`]); each sends its
//!   message and sound to every player on the impacted levels (`send_alert`).
//! - Every `SSweather` fire (1 s) a weather in its main stage acts on each living mob it can
//!   (`can_weather_act_mob`: on an impacted level, in an impacted area, not inside something
//!   weather-proof, not immune): a radiation storm mutates humans, the nebula's also irradiates
//!   (5%), the lava floor burns those standing on the floor, an ash storm burns everyone outdoors
//!   on lavaland (4 burn) but the ash-storm immune (silicons, the mining fauna, mech pilots) and
//!   humans in full fire gear (`get_thermal_protection() >= FIRE_IMMUNITY_MAX_TEMP_PROTECT`).
//! - The scheduler (`SSweather.fire`, `make_eligible`): a level made eligible for weathers
//!   ([`make_eligible`], tg levels with a weather trait) gets one by `probability`, and is
//!   eligible again 5-10 minutes after that weather's longest duration. The mining level
//!   (`ZTRAIT_ASHSTORM`, [`ss13_mapload::mining_z`]) gets the ash storm (90) and emberfall (10).
// shortcut: no area overlays (needs a client contract: tg draws the stage's icon state over
// every impacted area), no particles, no turf or thunder effects (no rain on the station), no
// barometers, weather radios or storm sound managers (`area_sound_manager`); the base
// `weather_act_mob` pull of body temperature towards 20 °C is not ported; no status display
// radiation alert (the displays have no emergency override); no icemoon, sand or rain weathers;
// the ash storm has no area sound loops (`GLOB.ash_storm_sounds`), thunder, barometer, and does
// not refill dug basalt (`refill_dug`), and its damage ignores `required_bodytype`.

use std::collections::HashSet;

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{Areas, DmArea, DmPower};
use ss13_net::chat::server::SendChat;
use ss13_net::sound::server::SendSound;
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_shared::protocol::SoundEvent;

use crate::combat::CombatRng;
use crate::ghost::Ghost;
use crate::health::{Health, Stat};
use crate::server::PlayerMob;

/// `SSweather.wait`.
const FIRE_SECS: f64 = 1.0;
/// `SSweather.fire`: `rand(5 MINUTES, 10 MINUTES)` before a level is eligible again.
const REST_SECS: (u32, u32) = (300, 600);

/// `weather_flags`.
const WEATHER_MOBS: u8 = 1 << 0;
const WEATHER_INDOORS: u8 = 1 << 1;
const WEATHER_ENDLESS: u8 = 1 << 2;

/// `/datum/weather/rad_storm` `protected_areas`.
const RAD_PROTECTED: &[&str] = &[
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
/// `/datum/weather/rad_storm/nebula` `protected_areas`.
const NEBULA_PROTECTED: &[&str] = &["/area/shuttle", "/area/station/maintenance/radshelter"];
/// The containers with `TRAIT_WEATHER_IMMUNE` (`weather_protection` of the environmental body
/// bags that seal fully).
const WEATHERPROOF: &[&str] = &[
    "/obj/structure/closet/body_bag/environmental/nanotrasen",
    "/obj/structure/closet/body_bag/environmental/prisoner/syndicate",
];
const BED: &str = "/obj/structure/bed";
/// Mobs with `TRAIT_ASHSTORM_IMMUNE` from their type: the lava-immune ones
/// ([`crate::lava::LAVA_IMMUNE_MOBS`]) and raptors, node drones, guardians, star gazers.
const ASH_IMMUNE_MOBS: [&str; 4] = [
    "/mob/living/basic/raptor",
    "/mob/living/basic/node_drone",
    "/mob/living/basic/guardian",
    "/mob/living/basic/heretic_summon/star_gazer",
];
const STRUCTURE: &str = "/obj/structure";

/// The ported `/datum/weather` subtypes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WeatherKind {
    /// `/datum/weather/rad_storm`.
    RadStorm,
    /// `/datum/weather/rad_storm/nebula`.
    RadStormNebula,
    /// `/datum/weather/floor_is_lava`.
    FloorIsLava,
    /// `/datum/weather/void_storm`.
    VoidStorm,
    /// `/datum/weather/particle/ash_storm`.
    AshStorm,
    /// `/datum/weather/particle/ash_storm/emberfall`.
    Emberfall,
}

/// `target_trait`: the levels a weather runs on by default and is scheduled for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Target {
    /// `ZTRAIT_STATION`.
    Station,
    /// `ZTRAIT_ASHSTORM`: the mining level.
    AshStorm,
}

/// The static vars of a weather type.
struct Def {
    name: &'static str,
    telegraph_message: Option<(Span, &'static str)>,
    telegraph_duration: f64,
    telegraph_sound: Option<&'static str>,
    weather_message: Option<(Span, &'static str)>,
    duration: (u32, u32),
    weather_sound: Option<&'static str>,
    end_message: Option<(Span, &'static str)>,
    end_duration: f64,
    area_type: &'static str,
    protected_areas: &'static [&'static str],
    flags: u8,
    /// `probability` on the levels of its trait (0: never by itself).
    probability: u32,
    target: Target,
}

const WIND_UP: (Span, &str) = (Span::Warning, "The wind begins to pick up.");
const WIND_DOWN: (Span, &str) = (Span::Danger, "The wind relents its assault.");

impl WeatherKind {
    pub const ALL: [Self; 6] = [
        Self::RadStorm,
        Self::RadStormNebula,
        Self::FloorIsLava,
        Self::VoidStorm,
        Self::AshStorm,
        Self::Emberfall,
    ];

    fn def(self) -> Def {
        let rad = Def {
            name: "radiation storm",
            telegraph_message: Some((
                Span::Danger,
                "Вам кажется, что воздух вокруг становится теплее.",
            )),
            telegraph_duration: 40.0,
            telegraph_sound: None,
            weather_message: Some((
                Span::BoldWarning,
                "Вы чувствуете, как вас окутывают волны тепла! Найдите убежище!",
            )),
            duration: (60, 150),
            weather_sound: Some("sound/announcer/alarm/bloblarm.ogg"),
            end_message: Some((Span::Notice, "Кажется, воздух снова остывает.")),
            end_duration: 10.0,
            area_type: "/area",
            protected_areas: RAD_PROTECTED,
            flags: WEATHER_MOBS | WEATHER_INDOORS,
            probability: 0,
            target: Target::Station,
        };
        let ash = Def {
            name: "ash storm",
            telegraph_message: Some((
                Span::BoldWarning,
                "Далёкий ветер разносит жуткий стон. Горизонт застилают тучи пепла и чёрного \
                 дыма. Поскорее найдите укрытие.",
            )),
            telegraph_duration: 30.0,
            telegraph_sound: None,
            weather_message: Some((
                Span::Danger,
                "Вокруг вас сгущаются облака жгучего пепла! Немедленно прячьтесь внутрь!",
            )),
            duration: (60, 120),
            weather_sound: None,
            end_message: Some((
                Span::Danger,
                "Ветер уносит от вас остатки пепла и возвращается к своему привычному тихому \
                 гулу. Теперь можно безопасно выходить наружу.",
            )),
            end_duration: 30.0,
            area_type: "/area",
            protected_areas: &[],
            flags: WEATHER_MOBS,
            probability: 90,
            target: Target::AshStorm,
        };
        match self {
            Self::RadStorm => rad,
            Self::RadStormNebula => Def {
                end_message: None,
                protected_areas: NEBULA_PROTECTED,
                flags: rad.flags | WEATHER_ENDLESS,
                ..rad
            },
            Self::FloorIsLava => Def {
                name: "the floor is lava",
                telegraph_message: Some((
                    Span::Warning,
                    "Вы чувствуете, как земля под вами становится всё горячее. Волны жара \
                     создают миражи в воздухе.",
                )),
                telegraph_duration: 15.0,
                telegraph_sound: None,
                weather_message: Some((Span::BoldWarning, "Пол это лава! Взбирайтесь повыше!")),
                duration: (30, 60),
                weather_sound: None,
                end_message: Some((
                    Span::Danger,
                    "Пол остывает и возвращается в свое обыкновенное состояние.",
                )),
                end_duration: 0.0,
                area_type: "/area",
                protected_areas: &["/area/space"],
                flags: WEATHER_MOBS | WEATHER_INDOORS,
                probability: 0,
                target: Target::Station,
            },
            Self::VoidStorm => Def {
                name: "void storm",
                telegraph_message: Some(WIND_UP),
                telegraph_duration: 2.0,
                telegraph_sound: None,
                weather_message: Some((
                    Span::Hypnophrase,
                    "Вы чувствуете, как воздух становится холоднее... и как вас окутывает \
                     пустота...",
                )),
                duration: (60, 120),
                weather_sound: None,
                end_message: Some(WIND_DOWN),
                end_duration: 10.0,
                area_type: "/area",
                protected_areas: &[],
                flags: WEATHER_INDOORS | WEATHER_ENDLESS,
                probability: 0,
                target: Target::Station,
            },
            Self::AshStorm => ash,
            Self::Emberfall => Def {
                name: "emberfall",
                weather_message: Some((
                    Span::Notice,
                    "Тлеющие угольки медленно осыпаются вокруг вас, словно странный снег. \
                     Кажется, в этот раз шторм обошёл вас стороной...",
                )),
                end_message: Some((
                    Span::Notice,
                    "Пеплопад постепенно замедляется и угасает. Очередной слой сажи оседает на \
                     землю под вами...",
                )),
                flags: ash.flags & !WEATHER_MOBS,
                probability: 10,
                ..ash
            },
        }
    }

    /// tg `initial(name)`: `run_weather` also takes a weather by name.
    pub fn name(self) -> &'static str {
        self.def().name
    }

    pub fn by_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.name() == name)
    }

    /// `weather_duration_upper`, seconds.
    fn duration_upper(self) -> f64 {
        f64::from(self.def().duration.1)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    /// `STARTUP_STAGE`.
    Telegraph,
    /// `MAIN_STAGE`.
    Main,
    /// `WIND_DOWN_STAGE`.
    WindDown,
}

/// A running `/datum/weather`.
#[derive(Clone, Debug)]
pub struct Weather {
    pub kind: WeatherKind,
    pub stage: Stage,
    /// When the stage ends (game seconds; never for an endless main stage).
    pub stage_end: f64,
    /// `weather_duration`, seconds.
    pub duration: f64,
    /// `impacted_z_levels`.
    pub levels: Vec<u16>,
    /// `impacted_areas`.
    pub areas: HashSet<Entity>,
    next_fire: f64,
}

/// `SSweather.processing`.
#[derive(Resource, Clone, Debug, Default)]
pub struct ActiveWeather(pub Vec<Weather>);

/// `SSweather.eligible_zlevels` and `next_hit_by_zlevel`: per level, the weathers it may get by
/// weight, and when it is eligible (`None`: now).
#[derive(Resource, Clone, Debug, Default)]
pub struct WeatherSchedule {
    pub levels: Vec<(u16, Vec<(WeatherKind, u32)>, Option<f64>)>,
}

pub struct WeatherPlugin;

impl Plugin for WeatherPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ActiveWeather>()
            .init_resource::<WeatherSchedule>()
            .add_systems(Startup, init_schedule)
            .add_systems(Update, fire);
    }
}

/// `SSweather.Initialize`: every weather with a `probability` on the levels of its trait.
fn init_schedule(world: &mut World) {
    for kind in WeatherKind::ALL {
        let def = kind.def();
        if def.probability == 0 {
            continue;
        }
        for z in target_levels(world, def.target) {
            make_eligible(world, z, kind, def.probability);
        }
    }
}

/// `SSmapping.levels_by_trait(target_trait)`.
fn target_levels(world: &World, target: Target) -> Vec<u16> {
    match target {
        Target::Station => crate::events::controller::station_levels(world).collect(),
        Target::AshStorm => ss13_mapload::mining_z(world).into_iter().collect(),
    }
}

/// `eligible_zlevels["[z]"][weather] = probability`: `z` may get `kind`.
pub fn make_eligible(world: &mut World, z: u16, kind: WeatherKind, probability: u32) {
    let mut schedule = world.resource_mut::<WeatherSchedule>();
    match schedule.levels.iter_mut().find(|(l, _, _)| *l == z) {
        Some((_, kinds, _)) => match kinds.iter_mut().find(|(k, _)| *k == kind) {
            Some((_, p)) => *p = probability,
            None => kinds.push((kind, probability)),
        },
        None => schedule.levels.push((z, vec![(kind, probability)], None)),
    }
}

/// `pick_weight`: by weight, `roll` in `0..total`.
fn pick_weight(kinds: &[(WeatherKind, u32)], mut roll: u32) -> Option<WeatherKind> {
    for &(kind, weight) in kinds {
        if roll < weight {
            return Some(kind);
        }
        roll -= weight;
    }
    None
}

/// tg `SSweather.run_weather(type, z_levels)`: a new weather of `kind` on `levels` (default:
/// the levels of its trait, the station's or the mining level), telegraphing at once.
pub fn run_weather(world: &mut World, kind: WeatherKind, levels: Option<Vec<u16>>) {
    let def = kind.def();
    let levels = levels.unwrap_or_else(|| target_levels(world, def.target));
    let areas = impacted_areas(world, &def);
    let (lo, hi) = def.duration;
    let duration = f64::from(world.resource_mut::<CombatRng>().range(lo, hi));
    let now = world.resource::<Time>().elapsed_secs_f64();
    let weather = Weather {
        kind,
        stage: Stage::Telegraph,
        stage_end: now + def.telegraph_duration,
        duration,
        levels,
        areas,
        next_fire: now,
    };
    info!("{} started on levels {:?}", def.name, weather.levels);
    // `telegraph`: the alert only with a telegraph.
    if def.telegraph_duration > 0.0 {
        send_alert(world, &weather, def.telegraph_message, def.telegraph_sound);
    }
    world.resource_mut::<ActiveWeather>().0.push(weather);
}

/// `setup_weather_areas`.
fn impacted_areas(world: &World, def: &Def) -> HashSet<Entity> {
    let Some(areas) = world.get_resource::<Areas>() else {
        return HashSet::new();
    };
    areas
        .entities()
        .iter()
        .copied()
        .filter(|&a| {
            let path = world.get::<DmPower>(a).map_or("", |p| p.path.as_str());
            is_under(path, def.area_type)
                && !def.protected_areas.iter().any(|root| is_under(path, root))
                && (def.flags & WEATHER_INDOORS != 0
                    || world.get::<DmArea>(a).is_some_and(|d| d.outdoors))
        })
        .collect()
}

/// A weather of `kind` is running.
pub fn running(world: &World, kind: WeatherKind) -> bool {
    world
        .get_resource::<ActiveWeather>()
        .is_some_and(|w| w.0.iter().any(|w| w.kind == kind))
}

/// tg `end()` of every running weather of `kind` (an endless one too): no wind-down.
pub fn end_weather(world: &mut World, kind: WeatherKind) {
    let ended: Vec<Weather> = {
        let mut active = world.resource_mut::<ActiveWeather>();
        let (ended, kept) = std::mem::take(&mut active.0)
            .into_iter()
            .partition(|w| w.kind == kind);
        active.0 = kept;
        ended
    };
    for weather in ended {
        on_end(world, &weather);
    }
}

/// `end()`'s type-specific part.
fn on_end(world: &mut World, weather: &Weather) {
    info!("{} ended", weather.kind.name());
    if matches!(
        weather.kind,
        WeatherKind::RadStorm | WeatherKind::RadStormNebula
    ) {
        crate::events::priority_announce(
            world,
            "Обнаружена аномалия",
            "Радиационная угроза миновала. Пожалуйста, вернитесь на свои рабочие места.",
        );
    }
}

/// `SSweather.fire`: the stage timers, the main stage's effect on mobs every second, then the
/// scheduler.
// Cost: per tick, the running weathers (usually none); per second of a main stage, one pass
// over living mobs.
fn fire(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    if !world.resource::<ActiveWeather>().0.is_empty() {
        advance_stages(world, now);
        act_on_mobs(world, now);
    }
    schedule(world, now);
}

fn advance_stages(world: &mut World, now: f64) {
    let mut i = 0;
    while i < world.resource::<ActiveWeather>().0.len() {
        let mut weather = world.resource::<ActiveWeather>().0[i].clone();
        if weather.stage_end > now {
            i += 1;
            continue;
        }
        let def = weather.kind.def();
        match weather.stage {
            // `start()`.
            Stage::Telegraph => {
                weather.stage = Stage::Main;
                weather.stage_end = if def.flags & WEATHER_ENDLESS != 0 {
                    f64::INFINITY
                } else {
                    now + weather.duration
                };
                weather.next_fire = now;
                send_alert(world, &weather, def.weather_message, def.weather_sound);
            }
            // `wind_down()`.
            Stage::Main => {
                weather.stage = Stage::WindDown;
                weather.stage_end = now + def.end_duration;
                send_alert(world, &weather, def.end_message, None);
            }
            // `end()`.
            Stage::WindDown => {
                world.resource_mut::<ActiveWeather>().0.remove(i);
                on_end(world, &weather);
                continue;
            }
        }
        world.resource_mut::<ActiveWeather>().0[i] = weather;
        // An instant stage (`end_duration = 0`) ends on the same tick.
    }
}

fn act_on_mobs(world: &mut World, now: f64) {
    let due: Vec<Weather> = {
        let mut active = world.resource_mut::<ActiveWeather>();
        active
            .0
            .iter_mut()
            .filter(|w| w.stage == Stage::Main && w.next_fire <= now)
            .filter(|w| w.kind.def().flags & WEATHER_MOBS != 0)
            .map(|w| {
                w.next_fire = now + FIRE_SECS;
                w.clone()
            })
            .collect()
    };
    if due.is_empty() {
        return;
    }
    let mobs: Vec<(Entity, TilePos)> = world
        .query_filtered::<(Entity, &TilePos), (With<Health>, Without<Ghost>)>()
        .iter(world)
        .map(|(e, p)| (e, *p))
        .collect();
    for weather in &due {
        for &(mob, pos) in &mobs {
            if world.get_entity(mob).is_ok() && can_weather_act_mob(world, weather, mob, pos) {
                weather_act_mob(world, weather.kind, mob);
            }
        }
    }
}

/// `can_weather_act_mob` with the type's own checks.
pub fn can_weather_act_mob(world: &World, weather: &Weather, mob: Entity, pos: TilePos) -> bool {
    if !weather.levels.contains(&pos.z) {
        return false;
    }
    let Some(area) = world.get_resource::<Areas>().and_then(|a| a.area_at(pos)) else {
        return false;
    };
    if !weather.areas.contains(&area) {
        return false;
    }
    // `recursive_weather_protection_check` of the mob's containers.
    if let Some(closet) = world.get::<crate::closets::InCloset>(mob).map(|c| c.closet)
        && crate::construction::path_of(world, closet)
            .is_some_and(|p| WEATHERPROOF.iter().any(|root| is_under(&p, root)))
    {
        return false;
    }
    match weather.kind {
        WeatherKind::FloorIsLava => floor_is_lava_reaches(world, mob, pos),
        WeatherKind::AshStorm | WeatherKind::Emberfall => !ash_storm_immune(world, mob),
        _ => true,
    }
}

/// `TRAIT_ASHSTORM_IMMUNE` of the mob or what holds it (a mech), and
/// `/datum/weather/particle/ash_storm/recursive_weather_protection_check`: a human whose suit and
/// helmet reach `FIRE_IMMUNITY_MAX_TEMP_PROTECT`.
pub fn ash_storm_immune(world: &World, mob: Entity) -> bool {
    world.get::<crate::silicon::Silicon>(mob).is_some()
        || world.get::<crate::mecha::core::Pilot>(mob).is_some()
        || world.get::<crate::mobs::BasicMob>(mob).is_some_and(|m| {
            crate::lava::LAVA_IMMUNE_MOBS
                .iter()
                .chain(&ASH_IMMUNE_MOBS)
                .any(|root| is_under(&m.def.path, root))
        })
        || crate::fire::thermal_protection(world, mob)
            >= crate::fire::FIRE_IMMUNITY_MAX_TEMP_PROTECT
}

/// `/datum/weather/floor_is_lava/can_weather_act_mob`: sentient, not a silicon, not in a bed,
/// not lava immune, standing on a floor without a dense structure, not flying.
fn floor_is_lava_reaches(world: &World, mob: Entity, pos: TilePos) -> bool {
    if world.get::<PlayerMob>(mob).is_none()
        || world.get::<crate::silicon::Silicon>(mob).is_some()
        || crate::species::has_trait(world, mob, crate::species::Trait::LavaImmune)
        || crate::multiz::flies(world, mob)
    {
        return false;
    }
    if let Some(b) = world.get::<crate::medical::surgery::Buckled>(mob)
        && crate::construction::path_of(world, b.0).is_some_and(|p| is_under(&p, BED))
    {
        return false;
    }
    !world
        .resource::<ss13_world::SpatialIndex>()
        .dense_on_tile(pos)
        .iter()
        .any(|&e| {
            e != mob
                && (world.get::<ss13_mapload::Turf>(e).is_some()
                    || crate::construction::path_of(world, e)
                        .is_some_and(|p| is_under(&p, STRUCTURE)))
        })
}

/// `weather_act_mob` of each type.
fn weather_act_mob(world: &mut World, kind: WeatherKind, mob: Entity) {
    match kind {
        WeatherKind::RadStorm => rad_storm_act(world, mob, 40.0),
        WeatherKind::RadStormNebula => {
            // `mutate_chance = 0.1`, then a 5% `radiation_pulse` on the mob itself.
            rad_storm_act(world, mob, 0.1);
            if world.resource_mut::<CombatRng>().chance(5.0)
                && crate::radiation::can_irradiate_basic(world, mob)
                && !crate::radiation::wearing_rad_protected_clothing(world, mob)
            {
                crate::radiation::radiation_pulse(
                    world,
                    mob,
                    0.0,
                    crate::radiation::RAD_LIGHT_INSULATION,
                    crate::radiation::URANIUM_IRRADIATION_CHANCE,
                    0.0,
                );
            }
        }
        WeatherKind::FloorIsLava => {
            if let Ok(mut entity) = world.get_entity_mut(mob)
                && let Ok((mut health, mut stat)) =
                    entity.get_components_mut::<(&mut Health, &mut Stat)>()
            {
                crate::health::apply_damage(
                    &mut health,
                    &mut stat,
                    crate::health::DamageType::Burn,
                    3.0,
                );
            }
        }
        WeatherKind::AshStorm => {
            // `adjust_fire_loss(4, required_bodytype = BODYTYPE_ORGANIC)`.
            if let Ok(mut entity) = world.get_entity_mut(mob)
                && let Ok((mut health, mut stat)) =
                    entity.get_components_mut::<(&mut Health, &mut Stat)>()
            {
                crate::health::apply_damage(
                    &mut health,
                    &mut stat,
                    crate::health::DamageType::Burn,
                    4.0,
                );
            }
        }
        WeatherKind::VoidStorm | WeatherKind::Emberfall => {}
    }
}

/// `/datum/weather/rad_storm/weather_act_mob`: `prob(mutate_chance)`, then a human that can
/// mutate (DNA, alive) and is neither radiation immune nor in radiation-protected clothing
/// gets a unique identity block scrambled and half the time `do_mutate`: a random negative (90%)
/// or positive mutation.
// shortcut: no unique features to scramble (`random_mutate_unique_features`).
fn rad_storm_act(world: &mut World, mob: Entity, mutate_chance: f32) {
    let mut rng = world.resource_mut::<CombatRng>();
    if !rng.chance(mutate_chance) {
        return;
    }
    let do_mutate = rng.chance(50.0);
    let negative = rng.chance(90.0);
    if world.get::<crate::genetics::Dna>(mob).is_none()
        || world.get::<Stat>(mob) == Some(&Stat::Dead)
        || crate::radiation::rad_immune(world, mob)
        || crate::radiation::wearing_rad_protected_clothing(world, mob)
    {
        return;
    }
    crate::genetics::random_mutate_unique_identity(world, mob);
    if do_mutate {
        let quality = if negative {
            crate::genetics::NEGATIVE | crate::genetics::MINOR_NEGATIVE
        } else {
            crate::genetics::POSITIVE
        };
        crate::genetics::easy_random_mutate(world, mob, quality);
        crate::genetics::domutcheck(world, mob);
    }
}

/// `send_alert(msg, sfx)`: every player on the impacted levels (`clients_by_zlevel`).
fn send_alert(
    world: &mut World,
    weather: &Weather,
    message: Option<(Span, &str)>,
    sound: Option<&str>,
) {
    if message.is_none() && sound.is_none() {
        return;
    }
    let clients: Vec<Entity> = world
        .query::<(&PlayerMob, &TilePos)>()
        .iter(world)
        .filter(|(_, pos)| weather.levels.contains(&pos.z))
        .map(|(p, _)| p.client)
        .collect();
    if clients.is_empty() {
        return;
    }
    if let Some(file) = sound {
        for &client in &clients {
            world.write_message(SendSound {
                client,
                sound: SoundEvent {
                    file: file.to_owned(),
                    volume: 100.0,
                    speed: 1.0,
                    offset: None,
                },
            });
        }
    }
    if let Some((span, text)) = message {
        world.write_message(SendChat {
            clients,
            message: ChatMessage {
                channel: ChatChannel::Warning,
                segments: vec![ChatSegment::new(span, text)],
            },
        });
    }
}

/// The scheduler half of `SSweather.fire`: each eligible level gets a weather by weight and
/// rests until 5-10 minutes after its longest duration.
fn schedule(world: &mut World, now: f64) {
    let due: Vec<(u16, Vec<(WeatherKind, u32)>)> = world
        .resource::<WeatherSchedule>()
        .levels
        .iter()
        .filter(|(_, kinds, at)| !kinds.is_empty() && at.is_none_or(|t| t <= now))
        .map(|(z, kinds, _)| (*z, kinds.clone()))
        .collect();
    for (z, kinds) in due {
        let total: u32 = kinds.iter().map(|(_, p)| p).sum();
        if total == 0 {
            continue;
        }
        let roll = world.resource_mut::<CombatRng>().range(0, total - 1);
        let Some(kind) = pick_weight(&kinds, roll) else {
            continue;
        };
        run_weather(world, kind, Some(vec![z]));
        let rest = f64::from(
            world
                .resource_mut::<CombatRng>()
                .range(REST_SECS.0, REST_SECS.1),
        );
        if let Some(level) = world
            .resource_mut::<WeatherSchedule>()
            .levels
            .iter_mut()
            .find(|(l, _, _)| *l == z)
        {
            level.2 = Some(now + rest + kind.duration_upper());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weights_pick_in_order() {
        let kinds = [(WeatherKind::RadStorm, 3), (WeatherKind::FloorIsLava, 1)];
        assert_eq!(pick_weight(&kinds, 0), Some(WeatherKind::RadStorm));
        assert_eq!(pick_weight(&kinds, 2), Some(WeatherKind::RadStorm));
        assert_eq!(pick_weight(&kinds, 3), Some(WeatherKind::FloorIsLava));
        assert_eq!(pick_weight(&kinds, 4), None);
    }

    #[test]
    fn kinds_carry_their_tg_vars() {
        let rad = WeatherKind::RadStorm.def();
        assert_eq!(rad.telegraph_duration, 40.0);
        assert_eq!(rad.duration, (60, 150));
        assert_eq!(rad.end_duration, 10.0);
        let nebula = WeatherKind::RadStormNebula.def();
        assert!(nebula.flags & WEATHER_ENDLESS != 0);
        assert!(nebula.end_message.is_none());
        assert_eq!(WeatherKind::FloorIsLava.def().end_duration, 0.0);
        assert_eq!(
            WeatherKind::by_name("void storm"),
            Some(WeatherKind::VoidStorm)
        );
    }

    #[test]
    fn ash_storms_hit_the_outdoors_of_the_mining_level() {
        let ash = WeatherKind::AshStorm.def();
        assert_eq!(ash.telegraph_duration, 30.0);
        assert_eq!(ash.duration, (60, 120));
        assert_eq!(ash.end_duration, 30.0);
        assert_eq!(ash.probability, 90);
        assert_eq!(ash.target, Target::AshStorm);
        assert!(ash.flags & WEATHER_MOBS != 0 && ash.flags & WEATHER_INDOORS == 0);
        let ember = WeatherKind::Emberfall.def();
        assert_eq!(ember.probability, 10);
        assert!(ember.flags & WEATHER_MOBS == 0);
        assert_eq!(ember.duration, ash.duration);
        assert_eq!(
            WeatherKind::by_name("emberfall"),
            Some(WeatherKind::Emberfall)
        );
    }
}
