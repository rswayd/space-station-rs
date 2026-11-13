//! Anomaly events (`code/modules/events/anomaly/`) and their anomalies
//! (`code/game/objects/effects/anomalies/`): the event picks a station area outside the
//! "safe" ones (`/datum/anomaly_placer`), announces it and drops an [`Anomaly`] on a free tile
//! of it. The anomaly processes every 2 s (`SSobj`): drifts a tile 45%/s and does its effect;
//! after 120 s it detonates and is gone.
//!
//! - Hyper-Energetic Flux (dense): shocks the first living thing touching it after each
//!   process (20 burn, no gloves); detonates in an EMP (4/16) and an explosion (0/1/4, flash 6).
//! - Gravitational: pulls loose objects and mobs within 4 tiles a step towards it, knocks down
//!   whoever steps on it, throws the loose items on it at someone within 4 tiles 20% of the
//!   time.
//! - Bluespace (dense): teleports the living within 1 tile up to 4 tiles away, a bumper up to
//!   8; detonates by moving everything loose within 12 tiles of a random tile of its area to
//!   the same offsets around a random teleport beacon on a station level.
//! - Pyroclastic: 5 mol of oxygen and plasma at 1000 K every 10 s; detonates in 500 mol of
//!   each.
//! - Vortex (`bhole`): pulls what is on a random ring around it (radius 0-3) a step closer
//!   half the time and blasts a quarter of those tiles (heavy or devastating), throws the loose
//!   objects within 2 at someone within 4 and blasts the anchored ones.
//! - Hallucination: a hallucination pulse (radius 5, 50 s, up to 300 s) every 5 s, one of
//!   radius 15 when it detonates; it comes with core-less decoys on 35% of the tiles around it,
//!   which only drift and mislead analyzers.
//!
//! Every anomaly but a decoy holds an anomaly core (`anomaly_core`): a signaller on a random
//! free frequency and code, which an analyzer reads off the anomaly. A signal on that
//! frequency and code neutralizes it (`anomalyNeutralize`): the core drops (a typed
//! `/obj/item/assembly/signaler/anomaly/*` on the same frequency and code) while the round's
//! cap of that type (`MAX_CORES_*`) is not reached, an inert core otherwise.
//!
//! Cost: one anomaly at a time in practice; a process looks at the tiles within 4 (the
//! gravitational one) every 2 s; a signal looks at the anomalies (a handful).
//
// The anomaly neutralizer (`/obj/item/anomaly_neutralizer`, its `effect_remover`) neutralizes
// an anomaly on hit and burns out; the anomaly releaser (`/obj/item/anomaly_releaser`) turns a
// core back into a stable (immortal) anomaly of its type after 3 s, once.
// shortcut: no countdown, light, wibble filter or emissive overlay; no neutralization smoke;
// explosions do not destroy anomalies; the cores' own `signal()` effects are not ported; the
// gravitational one does not throw mobs (knockdown only) or check magboots; the pyroclastic
// slime does not pick its colour's sprite variant; the bluespace one has no sparkle flash; the
// hallucination pulse ignores line of sight (tg `visible_hallucination_pulse`).

pub mod exotic;

use std::collections::HashMap;

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::{DmItem, DmName, DmObj, HiddenUnderTile, IconIndex, Turf};
use ss13_net::server::Networked;
use ss13_shared::appearance::{Layer, MouseOpacity};
use ss13_shared::{Appearance, IconStateId, TilePos};
use ss13_world::{Bumped, Density, Moved, SpatialIndex};

use super::controller::{EventRng, EventState, RoundEvent, station_levels};
use crate::construction::{path_of, turf_at};
use crate::health::{Health, Stat};
use crate::inventory::CarriedBy;
use crate::obj_damage::is_under;

pub const FLUX: &str = "Anomaly: Hyper-Energetic Flux";
pub const BLUESPACE: &str = "Anomaly: Bluespace";
pub const GRAV: &str = "Anomaly: Gravitational";
pub const PYRO: &str = "Anomaly: Pyroclastic";
pub const VORTEX: &str = "Anomaly: Vortex";
pub const HALLUCINATION: &str = "Anomaly: Hallucination";
pub const BIOSCRAMBLER: &str = "Anomaly: Bioscrambler";
pub const DIMENSIONAL: &str = "Anomaly: Dimensional";
pub const ECTOPLASM: &str = "Anomaly: Ectoplasmic Outburst";
pub const WEATHER: &str = "Anomaly: Weather";
pub const WEATHER_THUNDERING: &str = "Anomaly: Thundering Weather";

/// `ANOMALY_COUNTDOWN_TIMER`.
const LIFESPAN_SECS: f64 = 120.0;
/// `SSobj` wait.
const PROCESS_SECS: f64 = 2.0;
/// `ANOMALY_MOVECHANCE`, percent per second.
const MOVE_CHANCE: f64 = 45.0;
/// `ANOMALY_ANNOUNCE_MEDIUM_TEXT`, `ANOMALY_ANNOUNCE_HARMFUL_TEXT` (also `..._DANGEROUS_TEXT`).
const MEDIUM_TEXT: &str = "сканерах дальнего действия. Предполагаемая локация:";
const HARMFUL_TEXT: &str = "локальных сканерах. Зафиксированная локация:";
/// `/obj/effect`: `OBJ_LAYER` on `GAME_PLANE`.
const OBJ_LAYER: f32 = 3.0;
const GAME_PLANE: i16 = -4;
const COSMIC_ENERGY: &str = "sound/effects/magic/cosmic_energy.ogg";
const BEACON: &str = "/obj/item/beacon";
const EFFECT: &str = "/obj/effect";
/// `/datum/anomaly_placer` `safe_area_types` and the subtypes that are not safe after all.
/// `MIN_FREE_FREQ`, `MAX_FREE_FREQ`.
const MIN_FREE_FREQ: i64 = 1201;
const MAX_FREE_FREQ: i64 = 1599;
const INERT_CORE: &str = "/obj/item/inert_anomaly";
const CORE: &str = "/obj/item/assembly/signaler/anomaly";
const NEUTRALIZER: &str = "/obj/item/anomaly_neutralizer";
const RELEASER: &str = "/obj/item/anomaly_releaser";
/// The releaser's `used_icon_state`.
const RELEASER_USED_STATE: &str = "anomaly_releaser_used";
/// The releaser's `do_after`, deciseconds.
const RELEASE_DS: f32 = 30.0;
/// The hallucination anomaly's `messages`.
const HALLUCINATION_MESSAGES: [&str; 4] = [
    "Вы чувствуете, как ваш разум разваливается на части!",
    "Реальность искажается вокруг вас!",
    "Что-то шепчет вокруг вас!",
    "Вы сходите с ума!",
];
/// The decoy's `report_text` (with `%TOOL%` for the analyzer).
const DECOY_REPORTS: [&str; 6] = [
    "нестабильное поле колеблется на частоте 9999999.99999, код 9999999.99999. Нет, нет, этого не может быть?",
    "Он ничего не обнаруживает. Он ждёт ввода, будто вы направили его в пустоту. Что?",
    "Ничего не происходит?",
    "Он предлагает попробовать ещё раз, потому что вы всё делаете неправильно. Что?",
    "Вам приходит в голову, что сканируемой аномалии на самом деле нет.",
    "%TOOL% смотрит на вас с разочарованием: «Вы промахнулись мимо аномалии. Там ничего нет!»",
];
const SAFE_AREAS: [&str; 7] = [
    "/area/station/ai/satellite/chamber",
    "/area/station/ai/upload/chamber",
    "/area/station/engineering",
    "/area/station/science/ordnance/bomb",
    "/area/station/solars",
    "/area/station/holodeck",
    "/area/station/maintenance",
];
const UNSAFE_SUBTYPES: [&str; 1] = ["/area/station/engineering/break_room"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AnomalyKind {
    Flux,
    Bluespace,
    Grav,
    Pyro,
    Vortex,
    Hallucination,
    Bioscrambler,
    Dimensional,
    Ectoplasm,
    Weather,
}

impl AnomalyKind {
    pub const ALL: [Self; 10] = [
        Self::Flux,
        Self::Bluespace,
        Self::Grav,
        Self::Pyro,
        Self::Vortex,
        Self::Hallucination,
        Self::Bioscrambler,
        Self::Dimensional,
        Self::Ectoplasm,
        Self::Weather,
    ];

    /// The core's `anomaly_type`: none for the base core and the cores of unported anomalies.
    pub fn of_core(path: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|k| is_under(path, k.core_path()))
    }

    fn path(self) -> &'static str {
        match self {
            Self::Flux => "/obj/effect/anomaly/flux",
            Self::Bluespace => "/obj/effect/anomaly/bluespace",
            Self::Grav => "/obj/effect/anomaly/grav",
            Self::Pyro => "/obj/effect/anomaly/pyro",
            Self::Vortex => "/obj/effect/anomaly/bhole",
            Self::Hallucination => "/obj/effect/anomaly/hallucination",
            Self::Bioscrambler => "/obj/effect/anomaly/bioscrambler",
            Self::Dimensional => "/obj/effect/anomaly/dimensional",
            Self::Ectoplasm => "/obj/effect/anomaly/ectoplasm",
            Self::Weather => "/obj/effect/anomaly/weather",
        }
    }

    /// `anomaly_core`.
    pub fn core_path(self) -> &'static str {
        match self {
            Self::Flux => "/obj/item/assembly/signaler/anomaly/flux",
            Self::Bluespace => "/obj/item/assembly/signaler/anomaly/bluespace",
            Self::Grav => "/obj/item/assembly/signaler/anomaly/grav",
            Self::Pyro => "/obj/item/assembly/signaler/anomaly/pyro",
            Self::Vortex => "/obj/item/assembly/signaler/anomaly/vortex",
            Self::Hallucination => "/obj/item/assembly/signaler/anomaly/hallucination",
            Self::Bioscrambler => "/obj/item/assembly/signaler/anomaly/bioscrambler",
            Self::Dimensional => "/obj/item/assembly/signaler/anomaly/dimensional",
            Self::Ectoplasm => "/obj/item/assembly/signaler/anomaly/ectoplasm",
            Self::Weather => "/obj/item/assembly/signaler/anomaly/weather",
        }
    }

    /// `MAX_CORES_*` (`SSresearch.anomaly_hard_limit_by_type`).
    pub fn max_cores(self) -> u32 {
        match self {
            Self::Bluespace | Self::Vortex => 3,
            Self::Flux | Self::Weather => 5,
            Self::Grav => 6,
            Self::Pyro
            | Self::Hallucination
            | Self::Bioscrambler
            | Self::Dimensional
            | Self::Ectoplasm => 8,
        }
    }

    fn desc(self) -> &'static str {
        match self {
            Self::Vortex => {
                "That's a nice station you have there. It'd be a shame if something happened to it."
            }
            Self::Ectoplasm => {
                "It looks like the souls of the damned are trying to break into the realm of the \
                 living again. How upsetting."
            }
            _ => {
                "A mysterious anomaly, seen commonly only in the region of space that the station \
                 orbits..."
            }
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Flux => "flux wave anomaly",
            Self::Bluespace => "bluespace anomaly",
            Self::Grav => "gravitational anomaly",
            Self::Pyro => "pyroclastic anomaly",
            Self::Vortex => "vortex anomaly",
            Self::Hallucination => "hallucination anomaly",
            Self::Bioscrambler => "bioscrambler anomaly",
            Self::Dimensional => "dimensional anomaly",
            Self::Ectoplasm => "ectoplasm anomaly",
            Self::Weather => "weather anomaly",
        }
    }

    fn icon(self) -> (&'static str, &'static str) {
        match self {
            Self::Flux => ("icons/effects/anomalies.dmi", "flux"),
            Self::Bluespace => ("icons/obj/weapons/guns/projectiles.dmi", "bluespace"),
            Self::Grav => ("icons/effects/effects.dmi", "shield2"),
            Self::Pyro => ("icons/effects/anomalies.dmi", "pyroclastic"),
            Self::Vortex => ("icons/effects/anomalies.dmi", "vortex"),
            Self::Hallucination => ("icons/effects/anomalies.dmi", "hallucination"),
            Self::Bioscrambler => ("icons/effects/anomalies.dmi", "bioscrambler"),
            Self::Dimensional => ("icons/effects/anomalies.dmi", "dimensional"),
            Self::Ectoplasm => ("icons/effects/anomalies.dmi", "ectoplasm"),
            // The base `vortex` state (tg inverts its colors).
            Self::Weather => ("icons/effects/anomalies.dmi", "vortex"),
        }
    }

    fn dense(self) -> bool {
        matches!(self, Self::Flux | Self::Bluespace)
    }

    /// The event's `start_when` and `announce_when` (`ANOMALY_*_TIME`, in fires).
    fn timing(self) -> (u32, u32) {
        match self {
            Self::Bluespace | Self::Hallucination | Self::Bioscrambler | Self::Dimensional => {
                (3, 1)
            }
            Self::Flux
            | Self::Grav
            | Self::Pyro
            | Self::Vortex
            | Self::Ectoplasm
            | Self::Weather => (1, 15),
        }
    }

    /// `lifespan`, seconds (`ANOMALY_COUNTDOWN_TIMER` scaled by the type).
    fn lifespan(self) -> f64 {
        match self {
            Self::Bioscrambler => LIFESPAN_SECS * 2.0,
            Self::Dimensional => LIFESPAN_SECS * 20.0,
            Self::Ectoplasm => LIFESPAN_SECS + 2.0,
            Self::Weather => LIFESPAN_SECS * 2.5,
            _ => LIFESPAN_SECS,
        }
    }

    /// `move_chance`, percent per second.
    fn move_chance(self) -> f64 {
        match self {
            Self::Dimensional | Self::Ectoplasm => 0.0,
            _ => MOVE_CHANCE,
        }
    }
}

/// `/obj/effect/anomaly`.
#[derive(Component, Debug)]
pub struct Anomaly {
    pub kind: AnomalyKind,
    /// `impact_area`.
    pub area: Option<Entity>,
    pub death_time: f64,
    /// `move_chance`, percent per second (0 when stabilized with an anchor).
    pub move_chance: f64,
    /// `immortal`: never detonates (a stabilized anomaly).
    pub immortal: bool,
    next_process: f64,
    /// The flux's `canshock`, the gravitational one's `boing`.
    armed: bool,
    /// The pyroclastic and hallucination ones' `ticks`: seconds towards the next release.
    ticks: f64,
    /// `anomaly_core`: the frequency and code that neutralize it (`None`: a decoy, or the
    /// core is gone).
    pub core: Option<CoreSignal>,
    /// `/obj/effect/anomaly/hallucination/decoy`: drifts, misleads analyzers, nothing else.
    pub decoy: bool,
    /// The pyroclastic one's `already_polling` and `chosen_one`.
    polling: bool,
    chosen: Option<crate::ghost_poll::Candidate>,
}

/// On a pyroclastic anomaly's slime spawned before its poll ended: the poll's winner takes it.
#[derive(Component, Clone, Copy, Debug)]
pub struct PyroSlimeOf(pub Entity);

/// `/obj/effect/anomaly/pyro` `poll_time`.
const PYRO_POLL_SECS: f64 = 10.0;
/// `POLL_IGNORE_PYROSLIME`.
const POLL_IGNORE_PYROSLIME: &str = "pyroslime";

/// The `frequency` and `code` of an anomaly's core signaller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CoreSignal {
    pub frequency: u16,
    pub code: i32,
}

/// `SSresearch.created_anomaly_types`: cores of each type dropped this round.
#[derive(Resource, Default, Debug)]
pub struct CoresMade(pub HashMap<AnomalyKind, u32>);

// --- Events -------------------------------------------------------------------------------

struct AnomalyEvent {
    kind: AnomalyKind,
    /// `/obj/effect/anomaly/weather/thundering`.
    thundering: bool,
    /// `impact_area` and its tiles.
    area: Option<(Entity, Vec<TilePos>)>,
}

fn event(kind: AnomalyKind) -> Box<dyn RoundEvent> {
    Box::new(AnomalyEvent {
        kind,
        thundering: false,
        area: None,
    })
}

pub fn make_weather(_: &mut World) -> Box<dyn RoundEvent> {
    event(AnomalyKind::Weather)
}

pub fn make_weather_thundering(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(AnomalyEvent {
        kind: AnomalyKind::Weather,
        thundering: true,
        area: None,
    })
}

/// The weather events' `can_spawn_event`: no weather running (`SSweather.processing`).
pub fn no_weather_running(world: &mut World) -> bool {
    world
        .get_resource::<crate::weather::ActiveWeather>()
        .is_none_or(|w| w.0.is_empty())
}

pub fn make_flux(_: &mut World) -> Box<dyn RoundEvent> {
    event(AnomalyKind::Flux)
}

pub fn make_bluespace(_: &mut World) -> Box<dyn RoundEvent> {
    event(AnomalyKind::Bluespace)
}

pub fn make_grav(_: &mut World) -> Box<dyn RoundEvent> {
    event(AnomalyKind::Grav)
}

pub fn make_pyro(_: &mut World) -> Box<dyn RoundEvent> {
    event(AnomalyKind::Pyro)
}

pub fn make_vortex(_: &mut World) -> Box<dyn RoundEvent> {
    event(AnomalyKind::Vortex)
}

pub fn make_hallucination(_: &mut World) -> Box<dyn RoundEvent> {
    event(AnomalyKind::Hallucination)
}

pub fn make_bioscrambler(_: &mut World) -> Box<dyn RoundEvent> {
    event(AnomalyKind::Bioscrambler)
}

pub fn make_dimensional(_: &mut World) -> Box<dyn RoundEvent> {
    event(AnomalyKind::Dimensional)
}

pub fn make_ectoplasm(_: &mut World) -> Box<dyn RoundEvent> {
    event(AnomalyKind::Ectoplasm)
}

impl RoundEvent for AnomalyEvent {
    fn setup(&mut self, world: &mut World, ev: &mut EventState) {
        (ev.start_when, ev.announce_when) = self.kind.timing();
        self.area = find_valid_area(world);
        if self.area.is_none() {
            ev.killed = true;
        }
    }

    fn announce(&mut self, world: &mut World, _: &mut EventState, _: bool) {
        if self.area.is_none() {
            self.area = find_valid_area(world);
        }
        let Some((area, _)) = &self.area else {
            return;
        };
        let name = crate::shuttle::area_name(world, *area);
        let title = "Обнаружена аномалия";
        match self.kind {
            AnomalyKind::Flux => super::priority_announce(
                world,
                title,
                &format!(
                    "Поточная гиперэнергетическая аномалия обнаружена на {HARMFUL_TEXT} {name}."
                ),
            ),
            AnomalyKind::Bluespace => super::priority_announce(
                world,
                title,
                &format!("Нестабильная блюспейс-аномалия обнаружена на {MEDIUM_TEXT} {name}."),
            ),
            AnomalyKind::Grav => super::priority_announce_sound(
                world,
                title,
                &format!("Гравитационная аномалия обнаружена на {HARMFUL_TEXT} {name}."),
                crate::announce::ANNOUNCER_GRANOMALIES,
            ),
            AnomalyKind::Pyro => super::priority_announce(
                world,
                title,
                &format!("Пирокластическая аномалия обнаружена на {HARMFUL_TEXT} {name}."),
            ),
            AnomalyKind::Vortex => super::priority_announce(
                world,
                title,
                &format!(
                    "Вихревая аномалия высокой интенсивности обнаружена на {HARMFUL_TEXT} {name}"
                ),
            ),
            AnomalyKind::Hallucination => super::priority_announce(
                world,
                title,
                &format!("Галлюцинаторное событие обнаружено на {MEDIUM_TEXT} {name}."),
            ),
            AnomalyKind::Bioscrambler => super::priority_announce(
                world,
                title,
                &format!(
                    "Биологический агент замены конечностей обнаружен на {MEDIUM_TEXT} {name}. \
                     Используйте биокостюмы или другое защитное снаряжение, чтобы противостоять \
                     последствиям."
                ),
            ),
            AnomalyKind::Dimensional => super::priority_announce(
                world,
                title,
                &format!("Пространственная аномалия обнаружена на {MEDIUM_TEXT} {name}."),
            ),
            AnomalyKind::Ectoplasm => super::priority_announce(
                world,
                title,
                &format!("Паранормальный выброс эктоплазмы обнаружен на {HARMFUL_TEXT} {name}."),
            ),
            AnomalyKind::Weather if self.thundering => super::priority_announce(
                world,
                title,
                &format!("Серьезная барометрическая аномалия обнаружена на {HARMFUL_TEXT} {name}."),
            ),
            AnomalyKind::Weather => super::priority_announce(
                world,
                title,
                &format!("Барометрическая аномалия обнаружена на {HARMFUL_TEXT} {name}."),
            ),
        }
    }

    fn start(&mut self, world: &mut World, _: &mut EventState) {
        let Some((area, tiles)) = self.area.clone() else {
            return;
        };
        let valid: Vec<TilePos> = tiles
            .into_iter()
            .filter(|&p| valid_destination(world, p))
            .collect();
        let Some(&pos) = world.resource_mut::<EventRng>().pick(&valid) else {
            return;
        };
        if self.kind == AnomalyKind::Weather {
            spawn_weather(world, pos, Some(area), self.thundering);
        } else {
            spawn_anomaly(world, self.kind, pos, Some(area));
        }
    }
}

// --- Placement ----------------------------------------------------------------------------

/// `findValidArea()`: a random station area outside [`SAFE_AREAS`] that has tiles, with them.
pub fn find_valid_area(world: &mut World) -> Option<(Entity, Vec<TilePos>)> {
    let mut grouped: HashMap<Entity, Vec<TilePos>> = HashMap::new();
    for (area, pos) in super::station_tiles(world) {
        grouped.entry(area).or_default().push(pos);
    }
    let mut by_area: Vec<(Entity, Vec<TilePos>)> = grouped
        .into_iter()
        .filter(|(area, _)| {
            super::area_type(world, *area).is_some_and(|path| {
                UNSAFE_SUBTYPES.iter().any(|u| is_under(path, u))
                    || !SAFE_AREAS.iter().any(|s| is_under(path, s))
            })
        })
        .collect();
    if by_area.is_empty() {
        return None;
    }
    by_area.sort_by_key(|(area, _)| *area);
    let i = world
        .resource_mut::<EventRng>()
        .0
        .range(0, by_area.len() as i64 - 1);
    Some(by_area.swap_remove(i as usize))
}

/// `is_valid_destination()`: not space, nothing dense but mobs.
// shortcut: no lava or chasms are ported.
fn valid_destination(world: &World, pos: TilePos) -> bool {
    let space = turf_at(world, pos)
        .and_then(|t| path_of(world, t))
        .is_none_or(|p| is_under(&p, "/turf/open/space"));
    !space
        && world
            .resource::<SpatialIndex>()
            .dense_on_tile(pos)
            .iter()
            .all(|&e| world.get::<Health>(e).is_some())
}

/// `new anomaly_path(pos)`; a hallucination anomaly brings its decoys (`generate_decoys`).
pub fn spawn_anomaly(
    world: &mut World,
    kind: AnomalyKind,
    pos: TilePos,
    area: Option<Entity>,
) -> Entity {
    if kind == AnomalyKind::Weather {
        return spawn_weather(world, pos, area, false);
    }
    let e = spawn_one(world, kind, pos, area, false);
    if kind == AnomalyKind::Hallucination {
        for dx in -1..=1 {
            for dy in -1..=1 {
                let at = TilePos::new(pos.x + dx, pos.y + dy, pos.z);
                if (dx, dy) == (0, 0) || !world.resource::<SpatialIndex>().in_bounds(at) {
                    continue;
                }
                let wall = turf_at(world, at).is_some_and(|t| world.get::<Density>(t).is_some());
                if !wall && world.resource_mut::<EventRng>().0.prob(35.0) {
                    spawn_one(world, kind, at, area, true);
                }
            }
        }
    }
    e
}

/// `new /obj/effect/anomaly/weather(pos)` (`thundering`: its `thundering` subtype): the
/// weather starts with it.
pub fn spawn_weather(
    world: &mut World,
    pos: TilePos,
    area: Option<Entity>,
    thundering: bool,
) -> Entity {
    let e = spawn_one(world, AnomalyKind::Weather, pos, area, false);
    exotic::start_weather(world, e, thundering);
    e
}

/// tg `anomaly_core.code = rand(1, 100)` and `sanitize_frequency(rand(MIN_FREE_FREQ,
/// MAX_FREE_FREQ), free = TRUE)` (odd, so never the even syndicate frequency).
fn roll_core(world: &mut World) -> CoreSignal {
    let rng = &mut world.resource_mut::<EventRng>().0;
    let code = rng.range(1, 100) as i32;
    let mut frequency = rng.range(MIN_FREE_FREQ, MAX_FREE_FREQ);
    if frequency % 2 == 0 {
        frequency += 1;
    }
    CoreSignal {
        frequency: frequency.min(MAX_FREE_FREQ) as u16,
        code,
    }
}

fn spawn_one(
    world: &mut World,
    kind: AnomalyKind,
    pos: TilePos,
    area: Option<Entity>,
    decoy: bool,
) -> Entity {
    let core = (!decoy).then(|| roll_core(world));
    let (dmi, state) = kind.icon();
    let icon = world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(dmi, state))
        .unwrap_or(IconStateId(u32::MAX));
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut e = world.spawn((
        Name::new(kind.path()),
        DmName {
            name: kind.name().to_owned(),
            desc: kind.desc().to_owned(),
        },
        pos,
        Appearance {
            icon_state: icon,
            layer: Layer(OBJ_LAYER),
            plane: GAME_PLANE,
            color: None,
            pixel_offset: [0, 0],
            rotation: 0,
            overlays: vec![],
            mouse_opacity: MouseOpacity::Icon,
            glide: 0,
        },
        Networked,
        Anomaly {
            kind,
            area,
            death_time: now + kind.lifespan(),
            move_chance: kind.move_chance(),
            immortal: false,
            next_process: now + PROCESS_SECS,
            armed: false,
            ticks: 0.0,
            core,
            decoy,
            polling: false,
            chosen: None,
        },
    ));
    if kind.dense() {
        e.insert(Density);
    }
    let e = e.id();
    exotic::on_spawn(world, e, kind);
    e
}

// --- Processing ---------------------------------------------------------------------------

/// `/obj/effect/anomaly/process()` of every anomaly whose `SSobj` fire has come.
pub(super) fn process_anomalies(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let secs = f64::from(world.resource::<Time>().delta_secs());
    exotic::process_portals(world, now, secs);
    let mut q = world.query::<(Entity, &Anomaly)>();
    let due: Vec<Entity> = q
        .iter(world)
        .filter(|(_, a)| a.next_process <= now)
        .map(|(e, _)| e)
        .collect();
    for e in due {
        if let Some(mut a) = world.get_mut::<Anomaly>(e) {
            a.next_process += PROCESS_SECS;
        }
        anomaly_effect(world, e, PROCESS_SECS);
        let Some(a) = world.get::<Anomaly>(e) else {
            continue;
        };
        if a.death_time < now && !a.immortal {
            detonate(world, e);
            world.despawn(e);
        }
    }
}

/// `anomalyEffect(seconds_per_tick)`.
fn anomaly_effect(world: &mut World, e: Entity, secs: f64) {
    let Some((kind, move_chance, decoy)) = world
        .get::<Anomaly>(e)
        .map(|a| (a.kind, a.move_chance, a.decoy))
    else {
        return;
    };
    // `SPT_PROB(move_chance, seconds_per_tick)`.
    let chance = 100.0 * (1.0 - (1.0 - move_chance / 100.0).powf(secs));
    if world.resource_mut::<EventRng>().0.prob(chance as f32)
        && !(kind == AnomalyKind::Bioscrambler && exotic::bioscrambler_move(world, e))
    {
        drift(world, e, kind);
    }
    let Some(&pos) = world.get::<TilePos>(e) else {
        return;
    };
    if decoy {
        return;
    }
    match kind {
        AnomalyKind::Flux => {
            arm(world, e);
            for mob in living_on(world, pos) {
                flux_shock(world, e, mob);
            }
        }
        AnomalyKind::Grav => grav_effect(world, e, pos),
        AnomalyKind::Bluespace => {
            let mobs: Vec<Entity> = world
                .resource::<SpatialIndex>()
                .in_radius(pos, 1)
                .filter(|&m| world.get::<Health>(m).is_some())
                .collect();
            for mob in mobs {
                teleport_near(world, mob, 4);
            }
        }
        AnomalyKind::Pyro => {
            let now = crate::tools::now(world);
            let Some(mut a) = world.get_mut::<Anomaly>(e) else {
                return;
            };
            // tg asks `poll_time` after the death time, when the anomaly has long detonated
            // (its slime is always mindless); the poll runs in the last `poll_time` instead.
            let poll = !a.immortal && !a.polling && a.death_time - PYRO_POLL_SECS < now;
            a.polling |= poll;
            // `releasedelay`.
            a.ticks += secs;
            let release = a.ticks >= 10.0;
            if release {
                a.ticks -= 10.0;
            }
            if poll {
                start_pyro_poll(world, e);
            }
            if release {
                spawn_air(world, pos, "o2=5;plasma=5;TEMP=1000");
            }
        }
        AnomalyKind::Vortex => vortex_effect(world, e, pos),
        AnomalyKind::Hallucination => {
            let Some(mut a) = world.get_mut::<Anomaly>(e) else {
                return;
            };
            // `release_delay`.
            a.ticks += secs;
            if a.ticks < 5.0 {
                return;
            }
            a.ticks -= 5.0;
            crate::hallucination::hallucination_pulse(
                world,
                pos,
                5,
                50.0,
                300.0,
                &HALLUCINATION_MESSAGES,
            );
        }
        AnomalyKind::Bioscrambler => exotic::bioscrambler_effect(world, e, false),
        AnomalyKind::Dimensional => exotic::dimensional_effect(world, e),
        AnomalyKind::Ectoplasm => exotic::ectoplasm_effect(world, e, pos),
        AnomalyKind::Weather => {}
    }
}

/// tg `EXPLODE_*` force from `rand(lo, hi)` (0: none).
fn severity_of(force: u32) -> Option<crate::explosions::wave::Severity> {
    use crate::explosions::wave::Severity;
    match force {
        1 => Some(Severity::Light),
        2 => Some(Severity::Heavy),
        3 => Some(Severity::Devastate),
        _ => None,
    }
}

/// The vortex `anomalyEffect`: `grav(rand(0, 3), rand(2, 3), 50, 25)`, then the objects within
/// 2: loose ones are thrown at the first conscious living thing within 4, anchored ones are
/// blasted (`med_mov_atom`).
fn vortex_effect(world: &mut World, e: Entity, pos: TilePos) {
    let (r, force) = {
        let rng = &mut world.resource_mut::<EventRng>().0;
        (rng.range(0, 3) as i32, rng.range(2, 3) as u32)
    };
    let severity = severity_of(force);
    // `grav()`: the ring of radius `r` (empty for 0: tg's `t < r` loop).
    let mut ring = Vec::new();
    for t in -r..r {
        ring.push((pos.x + t, pos.y - r));
        ring.push((pos.x - t, pos.y + r));
        ring.push((pos.x + r, pos.y + t));
        ring.push((pos.x - r, pos.y - t));
    }
    let mut blasted_turfs = Vec::new();
    let mut blasted = Vec::new();
    for (x, y) in ring {
        let at = TilePos::new(x, y, pos.z);
        if !world.resource::<SpatialIndex>().in_bounds(at) {
            continue;
        }
        if world.resource_mut::<EventRng>().0.prob(50.0) {
            let here: Vec<Entity> = world
                .resource::<SpatialIndex>()
                .on_tile(at)
                .iter()
                .copied()
                .filter(|&t| t != e && world.get::<Turf>(t).is_none())
                .collect();
            for t in here {
                let obj = world.get::<DmObj>(t).is_some() || world.get::<DmItem>(t).is_some();
                if world.get::<Health>(t).is_some() || (obj && loose_obj(world, t)) {
                    step_towards(world, t, pos);
                } else if obj && anchored(world, t) {
                    blasted.push(t);
                }
            }
        }
        if world.resource_mut::<EventRng>().0.prob(25.0) {
            blasted_turfs.push(at);
        }
    }
    // `for(var/obj/O in range(2, src))` (tg returns at itself, the intent is skipping it).
    let target = world
        .resource::<SpatialIndex>()
        .in_radius(pos, 4)
        .find(|&m| {
            world.get::<Health>(m).is_some()
                && world.get::<Stat>(m).is_some_and(|s| *s == Stat::Conscious)
        })
        .and_then(|m| world.get::<TilePos>(m).copied());
    let near: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .in_radius(pos, 2)
        .filter(|&t| {
            t != e
                && world.get::<Turf>(t).is_none()
                && world.get::<CarriedBy>(t).is_none()
                && (world.get::<DmObj>(t).is_some() || world.get::<DmItem>(t).is_some())
                && world.get::<Health>(t).is_none()
                && path_of(world, t).is_none_or(|p| !is_under(&p, EFFECT))
        })
        .collect();
    for t in near {
        if !anchored(world, t) {
            if let Some(to) = target {
                crate::throw::throw_at(world, t, to, 7, 5.0);
            }
        } else {
            blasted.push(t);
        }
    }
    if !blasted.is_empty() {
        crate::explosions::queue_movables(
            world,
            &blasted,
            crate::explosions::wave::Severity::Heavy,
        );
    }
    if let Some(severity) = severity
        && !blasted_turfs.is_empty()
    {
        crate::explosions::queue_turfs(world, &blasted_turfs, severity);
    }
}

/// `move_anomaly()`: `step(src, pick(GLOB.alldirs))`. A dense anomaly stops at anything dense
/// (the flux shocks a mob it runs into, `Bump`); the others only at walls.
fn drift(world: &mut World, e: Entity, kind: AnomalyKind) {
    let Some(&pos) = world.get::<TilePos>(e) else {
        return;
    };
    let rng = &mut world.resource_mut::<EventRng>().0;
    let (dx, dy) = loop {
        let d = (rng.range(-1, 1) as i32, rng.range(-1, 1) as i32);
        if d != (0, 0) {
            break d;
        }
    };
    let next = TilePos::new(pos.x + dx, pos.y + dy, pos.z);
    if !world.resource::<SpatialIndex>().in_bounds(next) {
        return;
    }
    let blockers: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .dense_on_tile(next)
        .iter()
        .copied()
        .filter(|&b| b != e)
        .collect();
    let wall = blockers.iter().any(|&b| turf_at(world, next) == Some(b));
    if wall || (kind.dense() && !blockers.is_empty()) {
        if kind == AnomalyKind::Flux {
            for b in blockers {
                flux_shock(world, e, b);
            }
        }
        return;
    }
    world.entity_mut(e).insert(next);
}

fn arm(world: &mut World, e: Entity) {
    if let Some(mut a) = world.get_mut::<Anomaly>(e) {
        a.armed = true;
    }
}

/// Takes the armed charge; false when there was none.
fn disarm(world: &mut World, e: Entity) -> bool {
    world
        .get_mut::<Anomaly>(e)
        .is_some_and(|mut a| std::mem::replace(&mut a.armed, false))
}

fn living_on(world: &World, pos: TilePos) -> Vec<Entity> {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .filter(|&m| world.get::<Health>(m).is_some())
        .collect()
}

/// Flux `mobShock(M)`: `electrocute_act(shockdamage, flags = SHOCK_NOGLOVES)` once per arming.
fn flux_shock(world: &mut World, anomaly: Entity, mob: Entity) {
    if world.get::<Health>(mob).is_none() || !disarm(world, anomaly) {
        return;
    }
    crate::wires::electrocute_act(world, mob, 20.0, anomaly, 1.0);
}

/// Gravitational `anomalyEffect`.
fn grav_effect(world: &mut World, e: Entity, pos: TilePos) {
    arm(world, e);
    let near: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .in_radius(pos, 4)
        .filter(|&t| t != e && world.get::<Turf>(t).is_none())
        .collect();
    // `for(var/obj/O in orange(4, src))`.
    for &t in &near {
        let at = world.get::<TilePos>(t).copied();
        if at.is_some_and(|p| p != pos) && loose_obj(world, t) {
            step_towards(world, t, pos);
        }
    }
    for mob in living_on(world, pos) {
        grav_shock(world, e, mob);
    }
    // `for(var/mob/living/M in orange(4, src))`.
    for &t in &near {
        let at = world.get::<TilePos>(t).copied();
        if at.is_some_and(|p| p != pos) && world.get::<Health>(t).is_some() {
            step_towards(world, t, pos);
        }
    }
    // `for(var/obj/O in range(0, src))`: thrown at the first living thing within 4.
    let target = world
        .resource::<SpatialIndex>()
        .in_radius(pos, 4)
        .find(|&m| world.get::<Health>(m).is_some());
    let Some(target) = target else {
        return;
    };
    let conscious = world
        .get::<Stat>(target)
        .is_some_and(|s| *s == Stat::Conscious);
    let Some(&target_pos) = world.get::<TilePos>(target) else {
        return;
    };
    let here: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .filter(|&t| {
            world.get::<DmItem>(t).is_some()
                && world.get::<HiddenUnderTile>(t).is_none()
                && loose_obj(world, t)
        })
        .collect();
    for item in here {
        if conscious && world.resource_mut::<EventRng>().0.prob(20.0) {
            crate::throw::throw_at(world, item, target_pos, 5, 10.0);
        }
    }
}

/// Gravitational `gravShock(M)`: a conscious mob is knocked down 4 s, once per arming.
// shortcut: the mob is not thrown away (mobs are not thrown yet).
fn grav_shock(world: &mut World, anomaly: Entity, mob: Entity) {
    let conscious = world
        .get::<Stat>(mob)
        .is_some_and(|s| *s == Stat::Conscious);
    if !conscious || world.get::<Health>(mob).is_none() || !disarm(world, anomaly) {
        return;
    }
    crate::status_effects::knockdown(world, mob, 4.0, 0.0);
}

/// An unanchored object or item on the map, not an effect.
fn loose_obj(world: &World, t: Entity) -> bool {
    (world.get::<DmObj>(t).is_some() || world.get::<DmItem>(t).is_some())
        && world.get::<Health>(t).is_none()
        && world.get::<CarriedBy>(t).is_none()
        && !anchored(world, t)
        && path_of(world, t).is_none_or(|p| !is_under(&p, EFFECT))
}

/// tg `anchored`: the runtime flag, else the type's.
pub(crate) fn anchored(world: &World, e: Entity) -> bool {
    world.get::<crate::obj_damage::Anchored>(e).map_or_else(
        || world.get::<DmObj>(e).is_some_and(|o| o.anchored),
        |a| a.0,
    )
}

/// `step_towards(thing, target)`: one step (diagonal allowed) unless something dense is there.
pub(crate) fn step_towards(world: &mut World, thing: Entity, target: TilePos) {
    let Some(&pos) = world.get::<TilePos>(thing) else {
        return;
    };
    let next = TilePos::new(
        pos.x + (target.x - pos.x).signum(),
        pos.y + (target.y - pos.y).signum(),
        pos.z,
    );
    if next == pos
        || !world
            .resource::<SpatialIndex>()
            .dense_on_tile(next)
            .iter()
            .all(|&b| b == thing)
    {
        return;
    }
    world.entity_mut(thing).insert(next);
}

/// `do_teleport(M, get_turf(M), precision)`.
fn teleport_near(world: &mut World, mob: Entity, precision: i32) {
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    if let Some(dest) = super::wormholes::teleport_turf(world, pos, precision) {
        world.entity_mut(mob).insert(dest);
    }
}

fn spawn_air(world: &mut World, pos: TilePos, gas: &str) {
    let failed = world
        .get_resource_mut::<ss13_atmos::Atmos>()
        .is_some_and(|mut atmos| atmos.spawn_air(pos, gas).is_err());
    if failed {
        warn!("anomaly gas {gas:?} does not parse");
    }
}

// --- Contact ------------------------------------------------------------------------------

/// `Bumped` (the flux and bluespace anomalies are dense) and `COMSIG_ATOM_ENTERED` (stepping
/// onto the gravitational one).
pub(super) fn anomaly_contacts(
    world: &mut World,
    mut bumps: Local<MessageCursor<Bumped>>,
    mut moves: Local<MessageCursor<Moved>>,
) {
    let bumped: Vec<Bumped> = bumps
        .read(world.resource::<Messages<Bumped>>())
        .copied()
        .collect();
    let moved: Vec<Moved> = moves
        .read(world.resource::<Messages<Moved>>())
        .copied()
        .collect();
    for Bumped {
        entity, obstacle, ..
    } in bumped
    {
        match world.get::<Anomaly>(obstacle).map(|a| a.kind) {
            Some(AnomalyKind::Flux) => flux_shock(world, obstacle, entity),
            Some(AnomalyKind::Grav) => grav_shock(world, obstacle, entity),
            Some(AnomalyKind::Bluespace) if world.get::<Health>(entity).is_some() => {
                teleport_near(world, entity, 8);
            }
            _ => {}
        }
    }
    for Moved { entity, to, .. } in moved {
        let anomalies: Vec<(Entity, AnomalyKind)> = world
            .resource::<SpatialIndex>()
            .on_tile(to)
            .iter()
            .filter_map(|&a| world.get::<Anomaly>(a).map(|an| (a, an.kind)))
            .collect();
        for (anomaly, kind) in anomalies {
            match kind {
                AnomalyKind::Flux => flux_shock(world, anomaly, entity),
                AnomalyKind::Grav => grav_shock(world, anomaly, entity),
                _ => {}
            }
        }
    }
}

// --- Detonation ---------------------------------------------------------------------------

/// `detonate()`.
fn detonate(world: &mut World, e: Entity) {
    let (Some(&pos), Some(a)) = (world.get::<TilePos>(e), world.get::<Anomaly>(e)) else {
        return;
    };
    let (kind, area) = (a.kind, a.area);
    if a.decoy {
        return;
    }
    match kind {
        AnomalyKind::Flux => {
            world.write_message(crate::emp::Empulse {
                epicenter: pos,
                heavy: 4,
                light: 16,
            });
            crate::explosions::explosion(
                world,
                pos,
                crate::explosions::Explosion::new(0, 1, 4)
                    .flash(6)
                    .cause("flux anomaly"),
            );
        }
        AnomalyKind::Grav => cosmic_sound(world, pos),
        AnomalyKind::Bluespace => {
            cosmic_sound(world, pos);
            bluespace_mass_teleport(world, area);
        }
        AnomalyKind::Pyro => {
            spawn_air(world, pos, "o2=500;plasma=500;TEMP=1000");
            make_pyro_slime(world, e, pos);
        }
        AnomalyKind::Vortex => crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(
                pos,
                "sound/effects/hallucinations/far_noise.ogg",
                50.0,
                false,
            ),
        ),
        AnomalyKind::Hallucination => crate::hallucination::hallucination_pulse(
            world,
            pos,
            15,
            50.0,
            300.0,
            &HALLUCINATION_MESSAGES,
        ),
        AnomalyKind::Bioscrambler => exotic::bioscrambler_effect(world, e, true),
        AnomalyKind::Dimensional => {}
        AnomalyKind::Ectoplasm => exotic::ectoplasm_detonate(world, e, pos),
        AnomalyKind::Weather => exotic::weather_detonate(world, e, pos),
    }
}

// --- Neutralization -----------------------------------------------------------------------

/// A signaller sent `code` on `frequency` (`post_signal`): every anomaly whose core listens
/// there is neutralized (the core's `receive_signal`). `range`: the sender's tile and range on
/// its level (low-range signallers), `None` everywhere.
pub fn signal_received(
    world: &mut World,
    frequency: u16,
    code: i32,
    range: Option<(TilePos, u32)>,
) {
    let mut q = world.query::<(Entity, &Anomaly, &TilePos)>();
    let hit: Vec<Entity> = q
        .iter(world)
        .filter(|(_, a, pos)| {
            a.core == Some(CoreSignal { frequency, code })
                && range.is_none_or(|(from, r)| from.chebyshev(**pos).is_some_and(|d| d <= r))
        })
        .map(|(e, ..)| e)
        .collect();
    for e in hit {
        neutralize(world, e);
    }
}

/// `anomalyNeutralize()`: the core drops while the round's cap of its type allows, an inert
/// core with a warning otherwise; the anomaly is gone.
pub fn neutralize(world: &mut World, e: Entity) {
    let (Some(&pos), Some(a)) = (world.get::<TilePos>(e), world.get::<Anomaly>(e)) else {
        return;
    };
    let (kind, core) = (a.kind, a.core);
    // The weather anomaly's `anomalyNeutralize`: its weathers wind down.
    if kind == AnomalyKind::Weather {
        crate::weather::wind_down_from(world, e);
    }
    world.despawn(e);
    let Some(core) = core else {
        return;
    };
    let available = {
        let mut made = world.get_resource_or_insert_with(CoresMade::default);
        let n = made.0.entry(kind).or_default();
        let ok = *n < kind.max_cores();
        if ok {
            *n += 1;
        }
        ok
    };
    if !available {
        let clients = crate::chem::clients_near(world, pos, None);
        if !clients.is_empty() {
            world.write_message(ss13_net::chat::server::SendChat {
                clients,
                message: crate::chem::line(
                    ss13_shared::chat::Span::Warning,
                    "Ядро аномалии тускнеет, падая на пол: для ещё одного ядра этого типа \
                     слишком мало фоновой энергии.",
                ),
            });
        }
        crate::construction::spawn_obj(world, INERT_CORE, pos);
        return;
    }
    spawn_core_on(world, kind, pos, core);
}

/// A finished core of `kind` on `pos` listening on `core`'s frequency and code.
fn spawn_core_on(
    world: &mut World,
    kind: AnomalyKind,
    pos: TilePos,
    core: CoreSignal,
) -> Option<Entity> {
    let item = crate::construction::spawn_obj(world, kind.core_path(), pos)?;
    world.flush();
    if let Some(mut asm) = world.get_mut::<crate::assemblies::server::Assembly>(item)
        && let crate::assemblies::server::Device::Signaler {
            frequency, code, ..
        } = &mut asm.device
    {
        *frequency = core.frequency;
        *code = core.code;
    }
    Some(item)
}

/// `new anomaly_type(loc)`: a finished core of `kind` on a random free frequency and code (the
/// refinery's `create_core`). Not counted towards the round's limit here.
pub fn spawn_core(world: &mut World, kind: AnomalyKind, pos: TilePos) -> Option<Entity> {
    let core = roll_core(world);
    spawn_core_on(world, kind, pos, core)
}

/// tg `analyzer_act` of an anomaly (or of an anomaly core): its core's frequency and code.
/// True when `target` is one.
pub fn analyzer_act(world: &mut World, client: Entity, tool: Entity, target: Entity) -> bool {
    use ss13_shared::chat::Span;
    if !crate::atmos::is_analyzer(world, tool) {
        return false;
    }
    let name = crate::chem::name_of(world, target);
    let tool_name = crate::chem::name_of(world, tool);
    if let Some(a) = world.get::<Anomaly>(target) {
        let text = if a.decoy {
            let i = (target.to_bits() % DECOY_REPORTS.len() as u64) as usize;
            format!(
                "Вы включаете {tool_name}. {}",
                DECOY_REPORTS[i].replace("%TOOL%", &tool_name)
            )
        } else if let Some(core) = a.core {
            format!(
                "Анализ... нестабильное поле {name} колеблется на частоте {}, код {}.",
                crate::assemblies::format_frequency(core.frequency),
                core.code
            )
        } else {
            format!("Анализ... нестабильное поле {name} не колеблется на стабильной частоте.")
        };
        crate::tools::tell(world, client, Span::Notice, text);
        return true;
    }
    let core =
        path_of(world, target).is_some_and(|p| is_under(&p, "/obj/item/assembly/signaler/anomaly"));
    if !core {
        return false;
    }
    let Some(crate::assemblies::server::Device::Signaler {
        frequency, code, ..
    }) = world
        .get::<crate::assemblies::server::Assembly>(target)
        .map(|a| a.device.clone())
    else {
        return false;
    };
    crate::tools::tell(
        world,
        client,
        Span::Notice,
        format!(
            "Анализ... стабилизированное поле {name} колеблется на частоте {}, код {code}.",
            crate::assemblies::format_frequency(frequency)
        ),
    );
    true
}

/// `stabilize(anchor, has_core)`: it lives until neutralized; `anchor` holds it in place, a
/// hollow one has no core to signal.
pub fn stabilize(world: &mut World, e: Entity, anchor: bool, has_core: bool) {
    let Some(mut a) = world.get_mut::<Anomaly>(e) else {
        return;
    };
    a.immortal = true;
    if !has_core {
        a.core = None;
    }
    if anchor {
        a.move_chance = 0.0;
    }
    if let Some(mut name) = world.get_mut::<DmName>(e) {
        let prefix = if has_core { "stable" } else { "hollow" };
        name.name = format!("{prefix} {}", name.name);
    }
}

// --- Neutralizer and releaser -------------------------------------------------------------

/// `/obj/item/anomaly_neutralizer`'s `effect_remover` (`try_remove_effect`): a hit neutralizes
/// the anomaly (`anomalyNeutralize`), the neutralizer sparks and is gone. A decoy
/// (`TRAIT_ILLUSORY_EFFECT`) is left alone. True when `actor.item` is a neutralizer used on an
/// anomaly.
pub fn neutralizer_act(world: &mut World, actor: crate::tools::Actor, target: Entity) -> bool {
    use ss13_shared::chat::Span;
    let Some(tool) = actor.item else {
        return false;
    };
    if !path_of(world, tool).is_some_and(|p| is_under(&p, NEUTRALIZER))
        || world.get::<Health>(actor.mob).is_none()
    {
        return false;
    }
    let Some(decoy) = world.get::<Anomaly>(target).map(|a| a.decoy) else {
        return false;
    };
    let (tool_name, name) = (
        crate::chem::name_of(world, tool),
        crate::chem::name_of(world, target),
    );
    if decoy {
        // tg returns NONE here; nothing else in the chain acts on an anomaly.
        crate::tools::tell(
            world,
            actor.client,
            Span::Notice,
            format!(
                "Вы проводите {tool_name} сквозь {name}, но ничего не происходит. А оно вообще \
                 здесь есть?"
            ),
        );
        return true;
    }
    crate::tools::tell(
        world,
        actor.client,
        Span::Notice,
        format!("Вы нейтрализуете {name} с помощью {tool_name}, выжигая его схемы."),
    );
    neutralize(world, target);
    if let Some(pos) = crate::explosions::turf_of(world, actor.mob) {
        crate::effects::particles::do_sparks(world, 3, false, pos);
    }
    world.despawn(tool);
    true
}

/// `/obj/item/anomaly_releaser/interact_with_atom` on an anomaly core: a 3 s `do_after`, then
/// [`complete_release`]. A used releaser blocks. True when it is a releaser on a core.
pub fn releaser_act(world: &mut World, actor: crate::tools::Actor, target: Entity) -> bool {
    let Some(tool) = actor.item else {
        return false;
    };
    if !path_of(world, tool).is_some_and(|p| is_under(&p, RELEASER))
        || !path_of(world, target).is_some_and(|p| is_under(&p, CORE))
    {
        return false;
    }
    if world.get::<ReleaserUsed>(tool).is_none() {
        crate::tools::do_after(
            world,
            actor,
            target,
            RELEASE_DS,
            0,
            crate::construction::Job::AnomalyRelease,
        );
    }
    true
}

/// A used `/obj/item/anomaly_releaser` (`used`).
#[derive(Component, Debug)]
pub struct ReleaserUsed;

/// The releaser's `do_after` is over: rechecks it is unused and the core still has an
/// `anomaly_type`, releases a stable anomaly of that type on the core's tile and uses up the
/// releaser and the core.
pub fn complete_release(world: &mut World, action: &crate::tools::DoAfter) {
    let (Some(tool), core) = (action.item, action.target) else {
        return;
    };
    if world.get::<ReleaserUsed>(tool).is_some()
        || !path_of(world, tool).is_some_and(|p| is_under(&p, RELEASER))
    {
        return;
    }
    let Some(kind) = path_of(world, core).and_then(|p| AnomalyKind::of_core(&p)) else {
        return;
    };
    let Some(pos) = crate::explosions::turf_of(world, core) else {
        return;
    };
    let area = world
        .get_resource::<ss13_mapload::Areas>()
        .and_then(|a| a.area_at(pos));
    let anomaly = spawn_anomaly(world, kind, pos, area);
    stabilize(world, anomaly, false, true);
    world.entity_mut(tool).insert(ReleaserUsed);
    if let Some(mut name) = world.get_mut::<DmName>(tool) {
        name.name = format!("used {}", name.name);
    }
    crate::obj_damage::set_icon_state(world, tool, RELEASER_USED_STATE);
    world.despawn(core);
}

fn cosmic_sound(world: &mut World, pos: TilePos) {
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(pos, COSMIC_ENERGY, 50.0, false),
    );
}

/// The bluespace `detonate()`: everything loose within 12 tiles of a random tile of its area
/// goes to the same offset from a random beacon (or a random tile within 12 of it off the
/// map).
// shortcut: beacons count on station levels only (tg: anywhere but CentCom and away levels).
fn bluespace_mass_teleport(world: &mut World, area: Option<Entity>) {
    let tiles: Vec<TilePos> = super::station_tiles(world)
        .into_iter()
        .filter(|&(a, _)| Some(a) == area)
        .map(|(_, p)| p)
        .collect();
    let Some(&impact) = world.resource_mut::<EventRng>().pick(&tiles) else {
        return;
    };
    let levels = station_levels(world);
    let mut q = world.query::<(&DmItem, Option<&TilePos>, Option<&CarriedBy>)>();
    let beacons: Vec<TilePos> = q
        .iter(world)
        .filter(|(item, ..)| is_under(&item.path, BEACON))
        .filter_map(|(_, pos, carrier)| {
            pos.copied()
                .or_else(|| carrier.and_then(|c| world.get::<TilePos>(c.0).copied()))
        })
        .filter(|p| levels.contains(&p.z))
        .collect();
    let Some(&beacon) = world.resource_mut::<EventRng>().pick(&beacons) else {
        return;
    };
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(beacon, "sound/effects/phasein.ogg", 100.0, true),
    );
    super::priority_announce(
        world,
        "Обнаружена аномалия",
        "Зафиксирован массивный выброс блюспейс энергии.",
    );
    let (dx, dy) = (beacon.x - impact.x, beacon.y - impact.y);
    let movables: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .in_radius(impact, 12)
        .filter(|&m| {
            let thing = world.get::<Health>(m).is_some()
                || world.get::<DmItem>(m).is_some()
                || world.get::<DmObj>(m).is_some();
            thing
                && world.get::<Turf>(m).is_none()
                && world.get::<CarriedBy>(m).is_none()
                && !anchored(world, m)
                && path_of(world, m).is_none_or(|p| !is_under(&p, EFFECT) && !is_under(&p, BEACON))
        })
        .collect();
    for m in movables {
        let Some(&at) = world.get::<TilePos>(m) else {
            continue;
        };
        let shifted = TilePos::new(at.x + dx, at.y + dy, beacon.z);
        let dest = if world.resource::<SpatialIndex>().in_bounds(shifted) {
            shifted
        } else {
            let rng = &mut world.resource_mut::<EventRng>().0;
            TilePos::new(
                beacon.x + rng.range(-12, 12) as i32,
                beacon.y + rng.range(-12, 12) as i32,
                beacon.z,
            )
        };
        if world.resource::<SpatialIndex>().in_bounds(dest) {
            world.entity_mut(m).insert(dest);
        }
    }
}

/// `start_poll`: the ghosts are asked for the slime.
fn start_pyro_poll(world: &mut World, anomaly: Entity) {
    if !world.contains_resource::<crate::ghost_poll::Polls>() {
        return;
    }
    let spec = crate::ghost_poll::PollSpec {
        question: "Вы хотите сыграть за pyroclastic anomaly slime?".to_owned(),
        role_name: "pyroclastic anomaly slime".to_owned(),
        secs: PYRO_POLL_SECS,
        ignore: Some(POLL_IGNORE_PYROSLIME),
        ..default()
    };
    let group = crate::ghost_poll::ghosts(world);
    crate::ghost_poll::poll_candidates(world, spec, group, move |world, mut signed_up| {
        let Some(chosen) = crate::ghost_poll::pick_candidate(world, &mut signed_up) else {
            return;
        };
        if let Some(mut a) = world.get_mut::<Anomaly>(anomaly) {
            a.chosen = Some(chosen);
            return;
        }
        // Already detonated: its slime waits for a player.
        let mut q = world.query::<(Entity, &PyroSlimeOf)>();
        let slime = q
            .iter(world)
            .find(|(_, of)| of.0 == anomaly)
            .map(|(e, _)| e);
        if let Some(slime) = slime {
            possess_pyro_slime(world, slime, chosen);
        }
    });
}

/// `makepyroslime`: an enraged adult red or orange slime, the poll's winner in it.
fn make_pyro_slime(world: &mut World, anomaly: Entity, pos: TilePos) {
    let colour = if world.resource_mut::<EventRng>().0.prob(50.0) {
        "red"
    } else {
        "orange"
    };
    let Some(slime) = crate::science::xenobio::spawn_slime(world, pos, colour, true) else {
        warn!("no slime prototype for the pyroclastic anomaly");
        return;
    };
    // `set_enraged_behaviour`.
    if let Some(mut s) = world.get_mut::<crate::science::xenobio::Slime>(slime) {
        s.rabid = true;
    }
    match world.get::<Anomaly>(anomaly).and_then(|a| a.chosen) {
        Some(chosen) => possess_pyro_slime(world, slime, chosen),
        None => {
            world.entity_mut(slime).insert(PyroSlimeOf(anomaly));
        }
    }
}

/// `PossessByPlayer` + `/datum/antagonist/pyro_slime`, if the winner is still a ghost.
fn possess_pyro_slime(world: &mut World, slime: Entity, chosen: crate::ghost_poll::Candidate) {
    let still_ghost = world.get::<crate::ghost::Ghost>(chosen.mob).is_some()
        && world
            .get::<crate::server::PlayerMob>(chosen.mob)
            .is_some_and(|p| p.client == chosen.client);
    if !still_ghost
        || world.get::<crate::server::PlayerMob>(slime).is_some()
        || world.get::<Stat>(slime).is_none_or(|s| *s == Stat::Dead)
    {
        return;
    }
    if !crate::medical::brainmob::possess(world, chosen.client, chosen.mob, slime) {
        return;
    }
    world.try_despawn(chosen.mob).ok();
    world.entity_mut(slime).remove::<PyroSlimeOf>();
    crate::antag::minions::add_minion(
        world,
        slime,
        crate::antag::minions::MinionKind::PyroSlime,
        None,
    );
    info!("{slime} was made into a slime by pyroclastic anomaly");
}
