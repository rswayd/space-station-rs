//! Ventilation Clog: Minor / Major / Critical / Strange (`code/modules/events/vent_clog.dm`):
//! a random unwelded station vent on a free tile spits dirt around itself, then every
//! `spawn_delay` fires a mob of the severity's list crawls out (while fewer than
//! `maximum_spawns` of them live and the vent is not welded) onto the vent or a free tile next
//! to it, with a splat of filth. A destroyed vent moves the clog to another one.
// shortcut: no plunger to clear the clog (not ported): it lasts until the event ends;
// "nothing to clog" kills the event in `setup` instead of keeping it out of the roll
// (`can_spawn_event`); dirt is spread by range, not `view(2)`. Mobs without a ported
// prototype or AI simply do not come or stand still.

use bevy::prelude::*;
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, Span};
use ss13_world::SpatialIndex;

use super::controller::{EventRng, EventState, RoundEvent, station_levels};
use crate::atmos_machinery::VentPump;
use crate::health::{DEFAULT_MESSAGE_RANGE, Stat};
use crate::obj_damage::is_under;

pub const MINOR: &str = "Ventilation Clog: Minor";
pub const MAJOR: &str = "Ventilation Clog: Major";
pub const CRITICAL: &str = "Ventilation Clog: Critical";
pub const STRANGE: &str = "Ventilation Clog: Strange";
/// `MOB_SPAWN_MINIMUM`.
const MOB_SPAWN_MINIMUM: i64 = 3;
const DIRT: &str = "/obj/effect/decal/cleanable/dirt";
const TABLE: &str = "/obj/structure/table";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Minor,
    Major,
    Critical,
    Strange,
}

impl Severity {
    /// `get_mob()`'s list.
    fn mobs(self) -> &'static [&'static str] {
        match self {
            Self::Minor => &[
                "/mob/living/basic/butterfly",
                "/mob/living/basic/cockroach",
                "/mob/living/basic/cockroach/bloodroach",
                "/mob/living/basic/spider/maintenance",
                "/mob/living/basic/mouse",
                "/mob/living/basic/snail",
            ],
            Self::Major => &[
                "/mob/living/basic/bee",
                "/mob/living/basic/cockroach/hauberoach",
                "/mob/living/basic/spider/giant",
                "/mob/living/basic/mouse/rat",
            ],
            Self::Critical => &[
                "/mob/living/basic/bee/toxin",
                "/mob/living/basic/carp",
                "/mob/living/basic/cockroach/glockroach",
            ],
            Self::Strange => &[
                "/mob/living/basic/bear",
                "/mob/living/basic/cockroach/glockroach/mobroach",
                "/mob/living/basic/goose",
                "/mob/living/basic/lightgeist",
                "/mob/living/basic/mothroach",
                "/mob/living/basic/mushroom",
                "/mob/living/basic/viscerator",
                "/mob/living/basic/pet/gondola",
            ],
        }
    }

    /// `filth_spawn_types`.
    fn filth(self) -> &'static [&'static str] {
        match self {
            Self::Minor => &[
                "/obj/effect/decal/cleanable/vomit",
                "/obj/effect/decal/cleanable/insectguts",
                "/obj/effect/decal/cleanable/blood/oil",
            ],
            Self::Major => &[
                "/obj/effect/decal/cleanable/blood",
                "/obj/effect/decal/cleanable/insectguts",
                "/obj/effect/decal/cleanable/fuel_pool",
                "/obj/effect/decal/cleanable/blood/oil",
            ],
            Self::Critical => &[
                "/obj/effect/decal/cleanable/blood",
                "/obj/effect/decal/cleanable/blood/splatter",
            ],
            Self::Strange => &[
                "/obj/effect/decal/cleanable/blood/xeno",
                "/obj/effect/decal/cleanable/fuel_pool",
                "/obj/effect/decal/cleanable/greenglow",
                "/obj/effect/decal/cleanable/vomit",
            ],
        }
    }
}

fn make(severity: Severity) -> Box<dyn RoundEvent> {
    Box::new(VentClog {
        severity,
        vent: None,
        mob: "",
        maximum_spawns: MOB_SPAWN_MINIMUM,
        spawn_delay: 10,
        living: Vec::new(),
    })
}

pub fn make_minor(_: &mut World) -> Box<dyn RoundEvent> {
    make(Severity::Minor)
}

pub fn make_major(_: &mut World) -> Box<dyn RoundEvent> {
    make(Severity::Major)
}

pub fn make_critical(_: &mut World) -> Box<dyn RoundEvent> {
    make(Severity::Critical)
}

pub fn make_strange(_: &mut World) -> Box<dyn RoundEvent> {
    make(Severity::Strange)
}

/// `/datum/round_event/vent_clog` and its subtypes.
pub struct VentClog {
    pub severity: Severity,
    pub vent: Option<Entity>,
    mob: &'static str,
    maximum_spawns: i64,
    spawn_delay: u32,
    /// `living_mobs`.
    living: Vec<Entity>,
}

impl RoundEvent for VentClog {
    fn setup(&mut self, world: &mut World, ev: &mut EventState) {
        ev.announce_when = 10;
        ev.announce_chance = 90.0;
        self.vent = get_vent(world);
        if self.vent.is_none() {
            ev.killed = true;
            return;
        }
        let rng = &mut world.resource_mut::<EventRng>().0;
        let mobs = self.severity.mobs();
        self.mob = mobs[rng.range(0, mobs.len() as i64 - 1) as usize];
        let (end, max, delay) = match self.severity {
            Severity::Minor => ((300, 600), 10, (10, 15)),
            Severity::Major => ((300, 600), 5, (15, 20)),
            Severity::Critical => ((300, 600), 6, (15, 25)),
            Severity::Strange => ((600, 900), 10, (6, 25)),
        };
        ev.end_when = rng.range(end.0, end.1) as u32;
        self.maximum_spawns = rng.range(MOB_SPAWN_MINIMUM, max);
        self.spawn_delay = rng.range(delay.0, delay.1) as u32;
    }

    fn announce(&mut self, world: &mut World, _: &mut EventState, fake: bool) {
        // A false alarm (or a lost vent) names a random station area (`GLOB.teleportlocs`).
        let area = match self.vent.filter(|_| !fake) {
            Some(vent) => vent_area_name(world, vent),
            None => random_area_name(world),
        };
        let (title, text) = match self.severity {
            Severity::Minor => (
                "Оповещение о загрязнении",
                format!(
                    "Обнаружено незначительное биологическое препятствие в вентиляции. \
                     Предполагается, что засор находится в {area}."
                ),
            ),
            Severity::Major => (
                "Заражение вредителями",
                format!(
                    "Обнаружено значительное биологическое препятствие в вентиляции. \
                     Предполагается, что засор находится в {area}."
                ),
            ),
            Severity::Critical => (
                "Отчет о безопасности",
                format!("Потенциально опасные формы жизни обнаружены в вентиляции {area}."),
            ),
            Severity::Strange => (
                "Неопознанные формы жизни",
                format!("Обнаружены необычные формы жизни в вентиляции {area}."),
            ),
        };
        if self.severity == Severity::Strange {
            super::priority_announce_sound(world, title, &text, crate::announce::ANNOUNCER_ALIENS);
        } else {
            super::priority_announce(world, title, &text);
        }
    }

    fn start(&mut self, world: &mut World, _: &mut EventState) {
        self.clog_vent(world);
    }

    fn tick(&mut self, world: &mut World, ev: &mut EventState) {
        if self
            .vent
            .is_some_and(|v| world.get::<VentPump>(v).is_none())
        {
            self.vent_move(world);
        }
        if !ev.active_for.is_multiple_of(self.spawn_delay.max(1)) {
            return;
        }
        // `life_check()`.
        self.living
            .retain(|&m| world.get::<Stat>(m).is_some_and(|s| *s != Stat::Dead));
        if (self.living.len() as i64) < self.maximum_spawns {
            self.produce_mob(world);
        }
    }

    fn end(&mut self, _: &mut World, _: &mut EventState) {
        self.vent = None;
        self.living.clear();
    }
}

impl VentClog {
    /// `clog_vent()`: dirt around the vent (85% per open tile within 2), then a first mob.
    fn clog_vent(&mut self, world: &mut World) {
        let Some(pos) = self.vent.and_then(|v| world.get::<TilePos>(v).copied()) else {
            return;
        };
        for dy in -2..=2 {
            for dx in -2..=2 {
                let tile = TilePos::new(pos.x + dx, pos.y + dy, pos.z);
                if !open_turf(world, tile) || !world.resource_mut::<EventRng>().0.prob(85.0) {
                    continue;
                }
                crate::construction::spawn_obj(world, DIRT, tile);
            }
        }
        self.produce_mob(world);
    }

    /// `vent_move()`: the vent is gone; the clog moves to another one.
    fn vent_move(&mut self, world: &mut World) {
        self.vent = get_vent(world);
        let Some(vent) = self.vent else {
            return;
        };
        self.clog_vent(world);
        let place = if world.resource_mut::<EventRng>().0.prob(50.0) {
            "Неизвестно.".to_owned()
        } else {
            format!("{}.", vent_area_name(world, vent))
        };
        let text = format!(
            "Показатели биосигнатур переместились в новый сегмент вентиляционной сети. Текущее \
             положение: {place}"
        );
        super::priority_announce(world, "Неопознанные формы жизни", &text);
    }

    /// `produce_mob()`.
    fn produce_mob(&mut self, world: &mut World) {
        let Some(vent) = self.vent else {
            return;
        };
        let (Some(&pos), Some(welded)) = (
            world.get::<TilePos>(vent),
            world.get::<VentPump>(vent).map(|v| v.welded),
        ) else {
            return;
        };
        if welded || blocked(world, pos) {
            return;
        }
        let Some(mob) = crate::mobs::spawn::spawn_mob(world, self.mob, pos) else {
            return;
        };
        self.living.push(mob);
        let text = format!(
            "{} crawls out of {}!",
            crate::security::name_of(world, mob),
            crate::security::name_of(world, vent)
        );
        crate::security::show(
            world,
            pos,
            DEFAULT_MESSAGE_RANGE,
            ChatChannel::Warning,
            (Span::Warning, text),
            &[],
        );
        let mut spots = vec![pos];
        for dy in -1..=1 {
            for dx in -1..=1 {
                let tile = TilePos::new(pos.x + dx, pos.y + dy, pos.z);
                if (dx, dy) != (0, 0) && open_turf(world, tile) && !blocked(world, tile) {
                    spots.push(tile);
                }
            }
        }
        let (spot, filth) = {
            let mut rng = world.resource_mut::<EventRng>();
            let spot = *rng.pick(&spots).expect("the vent's tile");
            (spot, *rng.pick(self.severity.filth()).expect("non-empty"))
        };
        if spot != pos {
            world.entity_mut(mob).insert(spot);
        }
        crate::construction::spawn_obj(world, filth, spot);
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(spot, "sound/effects/splat.ogg", 30.0, true),
        );
    }
}

/// `get_vent()`: a random unwelded vent of a station area on a station level, its tile not
/// blocked (tables do not block).
fn get_vent(world: &mut World) -> Option<Entity> {
    let levels = station_levels(world);
    let mut q = world.query::<(Entity, &VentPump, &TilePos)>();
    let mut vents: Vec<(Entity, TilePos)> = q
        .iter(world)
        .filter(|(_, v, p)| !v.welded && levels.contains(&p.z))
        .map(|(e, _, &p)| (e, p))
        .collect();
    vents.sort_by_key(|&(e, _)| e);
    vents.retain(|&(_, p)| {
        super::area_type_at(world, p).is_some_and(|a| is_under(a, "/area/station"))
            && !blocked(world, p)
    });
    world
        .resource_mut::<EventRng>()
        .pick(&vents)
        .map(|&(e, _)| e)
}

/// `is_blocked_turf_ignore_climbable()`: a dense thing other than a table.
fn blocked(world: &World, pos: TilePos) -> bool {
    world
        .resource::<SpatialIndex>()
        .dense_on_tile(pos)
        .iter()
        .any(|&e| crate::construction::path_of(world, e).is_none_or(|p| !is_under(&p, TABLE)))
}

/// `isopenturf()`: an open turf (a dense turf, a wall, is closed).
fn open_turf(world: &World, pos: TilePos) -> bool {
    crate::construction::turf_at(world, pos)
        .and_then(|t| crate::construction::path_of(world, t))
        .is_some_and(|p| is_under(&p, "/turf/open"))
}

/// `get_area_name(vent)`.
fn vent_area_name(world: &World, vent: Entity) -> String {
    world
        .get::<TilePos>(vent)
        .and_then(|&p| world.resource::<ss13_mapload::Areas>().area_at(p))
        .map(|a| crate::shuttle::area_name(world, a))
        .unwrap_or_default()
}

/// `pick(GLOB.teleportlocs)`: a random station area.
fn random_area_name(world: &mut World) -> String {
    let areas: Vec<Entity> = world
        .resource::<ss13_mapload::Areas>()
        .entities()
        .iter()
        .copied()
        .filter(|&a| {
            world
                .get::<ss13_mapload::DmPower>(a)
                .is_some_and(|p| is_under(&p.path, "/area/station"))
        })
        .collect();
    let picked = world.resource_mut::<EventRng>().pick(&areas).copied();
    picked.map_or_else(String::new, |a| crate::shuttle::area_name(world, a))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mobs_and_filth_are_runtime_types() {
        let registry = ss13_mapload::runtime::runtime_types();
        let severities = [
            Severity::Minor,
            Severity::Major,
            Severity::Critical,
            Severity::Strange,
        ];
        let missing: Vec<&str> = severities
            .into_iter()
            .flat_map(|s| s.mobs().iter().chain(s.filth()).copied())
            .filter(|p| !registry.contains(p))
            .collect();
        assert!(
            missing.is_empty(),
            "not in ss13_mapload::runtime: {missing:?}"
        );
    }
}
