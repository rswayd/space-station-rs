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
//! - Storm sounds (`area_sound_manager` of every player's mob, `weather_listener`): a player in
//!   an impacted area of an ash, sand, snow or rain storm hears the stage's loop for that area
//!   ([`weather_sounds`]; weather radios: `crate::weather_announcer`).
// shortcut: no area overlays (needs a client contract: tg draws the stage's icon state over
// every impacted area), no particles, no turf or thunder effects (no rain on the station); the
// base `weather_act_mob` pull of body temperature towards 20 °C is not ported; no status display
// radiation alert (the displays have no emergency override); no icemoon, sand or rain weathers;
// the ash storm has no thunder, and does not refill dug basalt (`refill_dug`), and its damage
// ignores `required_bodytype`.

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
use crate::looping_sound::LoopDef;
use crate::server::PlayerMob;

/// `SSweather.wait`.
const FIRE_SECS: f64 = 1.0;
/// `SSweather.fire`: `rand(5 MINUTES, 10 MINUTES)` before a level is eligible again.
const REST_SECS: (u32, u32) = (300, 600);

/// `weather_flags`.
const WEATHER_MOBS: u8 = 1 << 0;
const WEATHER_INDOORS: u8 = 1 << 1;
const WEATHER_ENDLESS: u8 = 1 << 2;
/// `WEATHER_BAROMETER`: weather radios and barometers warn of it.
const WEATHER_BAROMETER: u8 = 1 << 3;

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
    /// `/datum/weather/particle/rain_storm`.
    RainStorm,
    /// `/datum/weather/snow_storm`.
    SnowStorm,
    /// `/datum/weather/sand_storm`.
    SandStorm,
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
    pub const ALL: [Self; 9] = [
        Self::RadStorm,
        Self::RadStormNebula,
        Self::FloorIsLava,
        Self::VoidStorm,
        Self::AshStorm,
        Self::Emberfall,
        Self::RainStorm,
        Self::SnowStorm,
        Self::SandStorm,
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
            flags: WEATHER_MOBS | WEATHER_BAROMETER,
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
                flags: WEATHER_INDOORS | WEATHER_BAROMETER | WEATHER_ENDLESS,
                probability: 0,
                target: Target::Station,
            },
            Self::AshStorm => ash,
            // The planetary storms: no level here has their traits (`ZTRAIT_RAINSTORM`,
            // `ZTRAIT_SNOWSTORM`, `ZTRAIT_SANDSTORM`), so they only run when started (the
            // weather anomaly).
            Self::RainStorm => Def {
                name: "rain",
                telegraph_message: Some((
                    Span::Warning,
                    "Thunder rumbles from above. You hear droplets hitting the floor around you.",
                )),
                telegraph_duration: 30.0,
                telegraph_sound: None,
                weather_message: Some((Span::BoldWarning, "На вас выпадает дождь!")),
                duration: (180, 300),
                weather_sound: None,
                end_message: Some((Span::Danger, "Стук дождя постепенно затихает.")),
                end_duration: 30.0,
                area_type: "/area",
                protected_areas: &[],
                flags: WEATHER_MOBS | WEATHER_BAROMETER,
                probability: 0,
                target: Target::Station,
            },
            Self::SnowStorm => Def {
                name: "snow storm",
                telegraph_message: Some((
                    Span::Warning,
                    "В воздухе начинают скапливаться крупные снежинки...",
                )),
                telegraph_duration: 30.0,
                telegraph_sound: Some("sound/ambience/weather/snowstorm/snow_start.ogg"),
                weather_message: Some((
                    Span::BoldWarning,
                    "Сильная метель начинает разносить падающий с неба снег! Немедленно \
                     прячьтесь внутрь!",
                )),
                duration: (60, 150),
                weather_sound: None,
                end_message: Some((
                    Span::Danger,
                    "Метель успокаивается, теперь можно безопасно выходить наружу.",
                )),
                end_duration: 10.0,
                area_type: "/area",
                protected_areas: &[],
                flags: WEATHER_MOBS | WEATHER_BAROMETER,
                probability: 0,
                target: Target::Station,
            },
            Self::SandStorm => Def {
                name: "severe sandstorm",
                telegraph_message: Some((
                    Span::Danger,
                    "Вы видите песчаные облака, поднимающиеся из-за горизонта. Это не кажется \
                     чем-то хорошим...",
                )),
                telegraph_duration: 30.0,
                telegraph_sound: Some("sound/effects/siren.ogg"),
                weather_message: Some((
                    Span::BoldWarning,
                    "Ветер ударяет в вас горячим песком. Немедленно прячьтесь внутрь!",
                )),
                duration: (60, 120),
                weather_sound: None,
                end_message: Some((
                    Span::Danger,
                    "Ветер уносит от вас остатки песка и возвращается к своему привычному тихому \
                     гулу. Теперь можно безопасно выходить наружу.",
                )),
                end_duration: 30.0,
                area_type: "/area",
                protected_areas: &[],
                flags: WEATHER_MOBS | WEATHER_BAROMETER,
                probability: 0,
                target: Target::Station,
            },
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
    /// `end_duration`, seconds (`WEATHER_FORCED_END` may set it).
    pub end_duration: f64,
    /// `turf_thunder_chance` per impacted tile per fire (`WEATHER_THUNDER`; 0: none).
    pub thunder_chance: f64,
    /// The impacted tiles thunder may strike (filled for a thundering weather only).
    pub tiles: Vec<TilePos>,
    /// What started it (the weather anomaly), to wind it down.
    pub source: Option<Entity>,
    next_thunder: f64,
}

/// `weather_data` of `SSweather.run_weather`: the `WEATHER_FORCED_*` overrides.
#[derive(Clone, Debug, Default)]
pub struct Forced {
    /// `WEATHER_FORCED_AREAS` (indoors too: `WEATHER_INDOORS`).
    pub areas: HashSet<Entity>,
    /// `WEATHER_FORCED_TELEGRAPH`, `WEATHER_FORCED_DURATION`, `WEATHER_FORCED_END`, seconds.
    pub telegraph: f64,
    pub duration: f64,
    pub end: f64,
    /// `WEATHER_FORCED_THUNDER`.
    pub thunder: f64,
    pub source: Option<Entity>,
}

/// `THUNDER_CHANCE_HIGH`.
pub const THUNDER_CHANCE_HIGH: f64 = 0.001;
/// `MAX_TURFS_PER_TICK`.
const MAX_TURFS_PER_TICK: f64 = 500.0;
/// `/datum/weather/snow_storm` `weather_temperature`: `ICEBOX_MIN_TEMPERATURE - 40`.
const SNOW_TEMPERATURE: f64 = 180.0 - 40.0;
const LIGHTNING_SOUND: &str = "sound/effects/magic/lightningbolt.ogg";

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
            .add_systems(Update, (fire, weather_sounds).chain())
            .add_plugins(crate::weather_announcer::WeatherAnnouncerPlugin);
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
        end_duration: def.end_duration,
        thunder_chance: 0.0,
        tiles: Vec::new(),
        source: None,
        next_thunder: now,
    };
    info!("{} started on levels {:?}", def.name, weather.levels);
    // `telegraph`: the alert only with a telegraph.
    if def.telegraph_duration > 0.0 {
        send_alert(world, &weather, def.telegraph_message, def.telegraph_sound);
    }
    world.resource_mut::<ActiveWeather>().0.push(weather);
}

/// `SSweather.run_weather(type, z_levels, weather_data)` with forced areas, stage durations,
/// thunder and a source.
pub fn run_weather_forced(world: &mut World, kind: WeatherKind, levels: Vec<u16>, forced: Forced) {
    let def = kind.def();
    let now = world.resource::<Time>().elapsed_secs_f64();
    let tiles = if forced.thunder > 0.0 {
        area_tiles(world, &forced.areas, &levels)
    } else {
        Vec::new()
    };
    let weather = Weather {
        kind,
        stage: Stage::Telegraph,
        stage_end: now + forced.telegraph,
        duration: forced.duration,
        levels,
        areas: forced.areas,
        next_fire: now,
        end_duration: forced.end,
        thunder_chance: forced.thunder,
        tiles,
        source: forced.source,
        next_thunder: now,
    };
    info!(
        "{} started (forced) on levels {:?}",
        def.name, weather.levels
    );
    if forced.telegraph > 0.0 {
        send_alert(world, &weather, def.telegraph_message, def.telegraph_sound);
    }
    world.resource_mut::<ActiveWeather>().0.push(weather);
}

/// The tiles of `areas` on `levels` (`get_turfs_from_all_zlevels`).
fn area_tiles(world: &World, areas: &HashSet<Entity>, levels: &[u16]) -> Vec<TilePos> {
    let Some(index) = world.get_resource::<Areas>() else {
        return Vec::new();
    };
    let spatial = world.resource::<ss13_world::SpatialIndex>();
    let mut out = Vec::new();
    for &z in levels {
        let Some((w, h)) = spatial.level_size(z) else {
            continue;
        };
        for y in 1..=h as i32 {
            for x in 1..=w as i32 {
                let pos = TilePos::new(x, y, z);
                if index.area_at(pos).is_some_and(|a| areas.contains(&a)) {
                    out.push(pos);
                }
            }
        }
    }
    out
}

/// `wind_down()` of the weathers `source` started (the weather anomaly neutralized).
pub fn wind_down_from(world: &mut World, source: Entity) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut ended = Vec::new();
    for w in world.resource_mut::<ActiveWeather>().0.iter_mut() {
        if w.source == Some(source) && w.stage != Stage::WindDown {
            w.stage = Stage::WindDown;
            w.stage_end = now + w.end_duration;
            ended.push(w.clone());
        }
    }
    for w in ended {
        send_alert(world, &w, w.kind.def().end_message, None);
    }
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

impl Weather {
    /// `weather_flags & WEATHER_BAROMETER`: weather radios warn of it.
    pub fn barometer(&self) -> bool {
        self.kind.def().flags & WEATHER_BAROMETER != 0
    }

    /// `weather_flags & FUNCTIONAL_WEATHER`: it acts on mobs or strikes with thunder.
    pub fn functional(&self) -> bool {
        self.kind.def().flags & WEATHER_MOBS != 0 || self.thunder_chance > 0.0
    }
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
        thunder(world, now);
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
                weather.stage_end = now + weather.end_duration;
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

/// The `SSWEATHER_THUNDER` task of every thundering weather in its main stage, once a fire:
/// `ROUND_PROB(tiles × chance)` strikes on random impacted tiles that are not walls, space or
/// full-tile windows.
fn thunder(world: &mut World, now: f64) {
    let due: Vec<(usize, f64, usize)> = world
        .resource_mut::<ActiveWeather>()
        .0
        .iter_mut()
        .enumerate()
        .filter(|(_, w)| {
            w.stage == Stage::Main
                && w.thunder_chance > 0.0
                && !w.tiles.is_empty()
                && w.next_thunder <= now
        })
        .map(|(i, w)| {
            w.next_thunder = now + FIRE_SECS;
            (i, w.thunder_chance, w.tiles.len())
        })
        .collect();
    for (i, chance, count) in due {
        let per_tick = (count as f64 * chance).min(MAX_TURFS_PER_TICK);
        let mut strikes = per_tick.floor() as u32;
        let frac = (per_tick - per_tick.floor()) as f32 * 100.0;
        if world.resource_mut::<CombatRng>().chance(frac) {
            strikes += 1;
        }
        for _ in 0..strikes {
            let pick = world.resource_mut::<CombatRng>().range(0, count as u32 - 1) as usize;
            let pos = world.resource::<ActiveWeather>().0[i].tiles[pick];
            if can_weather_act_turf(world, pos) {
                thunder_act_turf(world, pos);
            }
        }
    }
}

/// `can_weather_act_turf`: open, not space, no full-tile window.
fn can_weather_act_turf(world: &World, pos: TilePos) -> bool {
    let Some(path) = crate::construction::turf_at(world, pos)
        .and_then(|t| crate::construction::path_of(world, t))
    else {
        return false;
    };
    if !is_under(&path, "/turf/open") || is_under(&path, "/turf/open/space") {
        return false;
    }
    !world
        .resource::<ss13_world::SpatialIndex>()
        .on_tile(pos)
        .iter()
        .any(|&e| {
            crate::construction::path_of(world, e)
                .is_some_and(|p| is_under(&p, "/obj/structure/window") && p.contains("fulltile"))
        })
}

/// `thunder_act_turf`: the living there are shocked (50, no gloves), the bolt's sound, a fire.
// shortcut: no thunderbolt flash effect, no ore smelting.
pub fn thunder_act_turf(world: &mut World, pos: TilePos) {
    let mobs: Vec<Entity> = world
        .resource::<ss13_world::SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .filter(|&m| world.get::<Health>(m).is_some())
        .collect();
    for mob in mobs {
        if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
            crate::tools::tell(
                world,
                client,
                Span::BoldWarning,
                "You've been struck by lightning!",
            );
        }
        crate::wires::electrocute_act(world, mob, 50.0, mob, 1.0);
    }
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(pos, LIGHTNING_SOUND, 100.0, true).extrarange(10.0),
    );
    if let Some(mut atmos) = world.get_resource_mut::<ss13_atmos::Atmos>() {
        atmos.hotspot_expose(pos, 1000.0, 2500.0, false);
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
        || world
            .get::<crate::modsuit::supply::AshImmune>(mob)
            .is_some()
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
        WeatherKind::SandStorm => {
            // `adjust_brute_loss(5, required_bodytype = BODYTYPE_ORGANIC)`.
            if let Ok(mut entity) = world.get_entity_mut(mob)
                && let Ok((mut health, mut stat)) =
                    entity.get_components_mut::<(&mut Health, &mut Stat)>()
            {
                crate::health::apply_damage(
                    &mut health,
                    &mut stat,
                    crate::health::DamageType::Brute,
                    5.0,
                );
            }
        }
        WeatherKind::SnowStorm => {
            // The base `weather_act_mob` at the storm's `weather_temperature`, through any
            // clothing (`WEATHER_TEMPERATURE_BYPASS_CLOTHING`), in steps.
            let body = world
                .get::<crate::breath::Respiration>(mob)
                .map(|r| r.bodytemperature);
            if let Some(body) = body {
                let delta = SNOW_TEMPERATURE - body;
                let step = if delta > 0.0 {
                    (delta / 15.0).min(30.0)
                } else {
                    (delta / 15.0).max(-30.0)
                };
                crate::medical::diseases::adjust_bodytemperature(world, mob, step, 0.0);
            }
        }
        WeatherKind::VoidStorm | WeatherKind::Emberfall | WeatherKind::RainStorm => {}
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

/// `/datum/component/area_sound_manager` on a player's mob while a storm with a playlist runs on
/// its level: the loop of its area, if any.
#[derive(Component, Clone, Copy, Debug)]
pub struct WeatherTrack {
    def: Option<&'static LoopDef>,
    /// The storm stage it follows: a new one is `handle_change` (the start sound plays).
    stage: (WeatherKind, Stage),
    /// `next_loop_time`: the loop waits for the old one's next sound (and `skip_start`).
    start_at: Option<(f64, bool)>,
}

/// The weathers with a sound playlist (`get_playlist_ref`).
fn has_playlist(kind: WeatherKind) -> bool {
    matches!(
        kind,
        WeatherKind::AshStorm
            | WeatherKind::Emberfall
            | WeatherKind::SandStorm
            | WeatherKind::SnowStorm
            | WeatherKind::RainStorm
    )
}

/// The playlists (`GLOB.ash_storm_sounds`, `sand_storm_sounds`, `snowstorm_sounds`,
/// `rain_storm_sounds`): the loop of an impacted area at a stage.
fn playlist(kind: WeatherKind, stage: Stage, outdoors: bool) -> Option<&'static LoopDef> {
    use crate::looping_sound::defs::*;
    let pick = |a: &'static LoopDef, b: &'static LoopDef| Some(if outdoors { a } else { b });
    match (kind, stage) {
        (WeatherKind::AshStorm | WeatherKind::Emberfall, Stage::Main) => {
            pick(&ACTIVE_OUTSIDE_ASHSTORM, &ACTIVE_INSIDE_ASHSTORM)
        }
        (WeatherKind::AshStorm | WeatherKind::Emberfall, _) => {
            pick(&WEAK_OUTSIDE_ASHSTORM, &WEAK_INSIDE_ASHSTORM)
        }
        (WeatherKind::SandStorm, Stage::Main) => Some(&ACTIVE_OUTSIDE_ASHSTORM),
        (WeatherKind::SandStorm, _) => Some(&WEAK_OUTSIDE_ASHSTORM),
        // Filled from `start()` and kept through the wind-down.
        (WeatherKind::SnowStorm, Stage::Telegraph) => None,
        (WeatherKind::SnowStorm, _) => Some(&SNOWSTORM),
        (WeatherKind::RainStorm, Stage::Telegraph) => Some(&RAIN_START),
        (WeatherKind::RainStorm, Stage::Main) => Some(&RAIN_MIDDLE),
        (WeatherKind::RainStorm, Stage::WindDown) => Some(&RAIN_END),
        _ => None,
    }
}

/// A storm with a playlist on `mob`'s level: the stage followed (the one whose playlist has
/// the mob's area, else the first) and the loop of the area.
fn storm_here(
    world: &World,
    mob: Entity,
) -> Option<((WeatherKind, Stage), Option<&'static LoopDef>)> {
    let pos = crate::explosions::turf_of(world, mob)?;
    let storms: Vec<&Weather> = world
        .get_resource::<ActiveWeather>()?
        .0
        .iter()
        .filter(|w| has_playlist(w.kind) && w.levels.contains(&pos.z))
        .collect();
    let first = storms.first()?;
    let area = world.get_resource::<Areas>().and_then(|a| a.area_at(pos));
    let outdoors = area.is_some_and(|a| world.get::<DmArea>(a).is_some_and(|d| d.outdoors));
    let playing = area.and_then(|area| {
        storms.iter().find_map(|w| {
            let def = playlist(w.kind, w.stage, outdoors).filter(|_| w.areas.contains(&area))?;
            Some(((w.kind, w.stage), def))
        })
    });
    Some(match playing {
        Some((stage, def)) => (stage, Some(def)),
        None => ((first.kind, first.stage), None),
    })
}

/// The area sound managers: each player's mob on a storm's level plays its area's loop to its
/// client (`direct`). Moving to an area with another loop changes it without the start sound
/// (`react_to_move`), a new stage changes it with it (`handle_change`); either way the new loop
/// waits for the old one's next sound (`next_loop_time`). An area out of the storm stops the
/// loop; the storm's end or leaving its level or logging out ends the manager
/// (`handle_removal`, `react_to_z_move`, `COMSIG_MOB_LOGOUT`). A stopped loop cuts its channel
/// sound.
// Cost: while a storm runs (or a manager lives), per tick one turf, area and weather lookup per
// player; nothing otherwise.
pub fn weather_sounds(world: &mut World) {
    use crate::looping_sound::{cancel, start_with, time_left};
    let storm = world
        .get_resource::<ActiveWeather>()
        .is_some_and(|a| a.0.iter().any(|w| has_playlist(w.kind)));
    let mut tracked = world.query_filtered::<Entity, With<WeatherTrack>>();
    let mut mobs: Vec<Entity> = tracked.iter(world).collect();
    if !storm && mobs.is_empty() {
        return;
    }
    if storm {
        let mut players =
            world.query_filtered::<Entity, (With<PlayerMob>, Without<WeatherTrack>)>();
        mobs.extend(players.iter(world));
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    for mob in mobs {
        let here = if world.get::<PlayerMob>(mob).is_some() {
            storm_here(world, mob)
        } else {
            None
        };
        let track = world.get::<WeatherTrack>(mob).copied();
        match (track, here) {
            (None, None) => {}
            (Some(t), None) => {
                if let Some(old) = t.def {
                    cancel(world, mob, old.path, true);
                }
                world.entity_mut(mob).remove::<WeatherTrack>();
            }
            (None, Some((stage, def))) => {
                if let Some(def) = def {
                    start_with(world, mob, def, false, true);
                }
                world.entity_mut(mob).insert(WeatherTrack {
                    def,
                    stage,
                    start_at: None,
                });
            }
            (Some(t), Some((stage, def))) => {
                let changed = t.stage != stage;
                if !changed && t.def.map(|d| d.path) == def.map(|d| d.path) {
                    if let (Some(def), Some((at, skip))) = (def, t.start_at)
                        && at <= now
                    {
                        start_with(world, mob, def, skip, true);
                        world.entity_mut(mob).insert(WeatherTrack {
                            start_at: None,
                            ..t
                        });
                    }
                    continue;
                }
                // `change_the_track`: a pending start keeps its time.
                let skip = !changed;
                let at = t
                    .start_at
                    .map(|(at, _)| at)
                    .or_else(|| {
                        t.def
                            .and_then(|old| time_left(world, mob, old.path))
                            .map(|left| now + left)
                    })
                    .filter(|&at| at > now);
                if let Some(old) = t.def {
                    // shortcut: with a next loop the old sound plays out (tg cuts a channel
                    // loop's sound at once, then waits silent for its timer).
                    cancel(world, mob, old.path, def.is_none());
                }
                if let Some(def) = def
                    && at.is_none()
                {
                    start_with(world, mob, def, skip, true);
                }
                world.entity_mut(mob).insert(WeatherTrack {
                    def,
                    stage,
                    start_at: def.and(at).map(|at| (at, skip)),
                });
            }
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
