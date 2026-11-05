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
//!
//! Cost: one anomaly at a time in practice; a process looks at the tiles within 4 (the
//! gravitational one) every 2 s.
// shortcut: no countdown, light, wibble filter or emissive overlay; no anomaly core and no
// neutralizing by signaller or analyzer (science); explosions do not destroy anomalies; the
// gravitational one does not throw mobs (knockdown only) or check magboots; the pyroclastic
// one makes no slime (no slimes or ghost polls); the bluespace one has no sparkle flash.

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnomalyKind {
    Flux,
    Bluespace,
    Grav,
    Pyro,
}

impl AnomalyKind {
    fn path(self) -> &'static str {
        match self {
            Self::Flux => "/obj/effect/anomaly/flux",
            Self::Bluespace => "/obj/effect/anomaly/bluespace",
            Self::Grav => "/obj/effect/anomaly/grav",
            Self::Pyro => "/obj/effect/anomaly/pyro",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Flux => "flux wave anomaly",
            Self::Bluespace => "bluespace anomaly",
            Self::Grav => "gravitational anomaly",
            Self::Pyro => "pyroclastic anomaly",
        }
    }

    fn icon(self) -> (&'static str, &'static str) {
        match self {
            Self::Flux => ("icons/effects/anomalies.dmi", "flux"),
            Self::Bluespace => ("icons/obj/weapons/guns/projectiles.dmi", "bluespace"),
            Self::Grav => ("icons/effects/effects.dmi", "shield2"),
            Self::Pyro => ("icons/effects/anomalies.dmi", "pyroclastic"),
        }
    }

    fn dense(self) -> bool {
        matches!(self, Self::Flux | Self::Bluespace)
    }

    /// The event's `start_when` and `announce_when` (`ANOMALY_*_TIME`, in fires).
    fn timing(self) -> (u32, u32) {
        match self {
            Self::Bluespace => (3, 1),
            Self::Flux | Self::Grav | Self::Pyro => (1, 15),
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
    next_process: f64,
    /// The flux's `canshock`, the gravitational one's `boing`.
    armed: bool,
    /// The pyroclastic one's `ticks`: seconds towards the next gas release.
    ticks: f64,
}

// --- Events -------------------------------------------------------------------------------

struct AnomalyEvent {
    kind: AnomalyKind,
    /// `impact_area` and its tiles.
    area: Option<(Entity, Vec<TilePos>)>,
}

fn event(kind: AnomalyKind) -> Box<dyn RoundEvent> {
    Box::new(AnomalyEvent { kind, area: None })
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
        spawn_anomaly(world, self.kind, pos, Some(area));
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

/// `new anomaly_path(pos)`.
pub fn spawn_anomaly(
    world: &mut World,
    kind: AnomalyKind,
    pos: TilePos,
    area: Option<Entity>,
) -> Entity {
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
            desc: "A mysterious anomaly, seen commonly only in the region of space that the \
                   station orbits..."
                .to_owned(),
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
        },
        Networked,
        Anomaly {
            kind,
            area,
            death_time: now + LIFESPAN_SECS,
            move_chance: MOVE_CHANCE,
            next_process: now + PROCESS_SECS,
            armed: false,
            ticks: 0.0,
        },
    ));
    if kind.dense() {
        e.insert(Density);
    }
    e.id()
}

// --- Processing ---------------------------------------------------------------------------

/// `/obj/effect/anomaly/process()` of every anomaly whose `SSobj` fire has come.
pub(super) fn process_anomalies(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
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
        if a.death_time < now {
            detonate(world, e);
            world.despawn(e);
        }
    }
}

/// `anomalyEffect(seconds_per_tick)`.
fn anomaly_effect(world: &mut World, e: Entity, secs: f64) {
    let Some((kind, move_chance)) = world.get::<Anomaly>(e).map(|a| (a.kind, a.move_chance)) else {
        return;
    };
    // `SPT_PROB(move_chance, seconds_per_tick)`.
    let chance = 100.0 * (1.0 - (1.0 - move_chance / 100.0).powf(secs));
    if world.resource_mut::<EventRng>().0.prob(chance as f32) {
        drift(world, e, kind);
    }
    let Some(&pos) = world.get::<TilePos>(e) else {
        return;
    };
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
            let Some(mut a) = world.get_mut::<Anomaly>(e) else {
                return;
            };
            // `releasedelay`.
            a.ticks += secs;
            if a.ticks < 10.0 {
                return;
            }
            a.ticks -= 10.0;
            spawn_air(world, pos, "o2=5;plasma=5;TEMP=1000");
        }
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

fn anchored(world: &World, e: Entity) -> bool {
    world.get::<crate::obj_damage::Anchored>(e).map_or_else(
        || world.get::<DmObj>(e).is_some_and(|o| o.anchored),
        |a| a.0,
    )
}

/// `step_towards(thing, target)`: one step (diagonal allowed) unless something dense is there.
fn step_towards(world: &mut World, thing: Entity, target: TilePos) {
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
    for Bumped { entity, obstacle } in bumped {
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
        AnomalyKind::Pyro => spawn_air(world, pos, "o2=500;plasma=500;TEMP=1000"),
    }
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
