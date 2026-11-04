//! Fire alarms and the firelocks they drive (tg `firealarm.dm`, `firedoor.dm` `activate`,
//! `crack_open`, `add_as_source`, `remove_as_source`, `correct_state`; `areas.dm`
//! `set_fire_effect`).
//!
//! A left click pulls the alarm (`alarm()`: powered, the area not already alarmed): every
//! firelock serving the area activates and closes, the first active firelock of an area sets its
//! `fire` (lights red, every fire alarm there flashes red), and the pulled alarm rings. A right
//! click resets (`reset()`): the area's firelocks crack open, the area's `fire` clears with its
//! last active firelock, the alarm stops ringing. A firelock serves its own area and the areas
//! of the open turfs next to it (`CalculateAffectingAreas`).
// shortcut: firelocks do not watch the air (`check_atmos`, `issue_turfs`): only alarms activate
// them, so `crack_open`'s `release_constraints` never re-closes one; no alarm manager (consoles,
// AI), no fault location examine, no `fire_detect` toggle (silicon ctrl-click), no security
// level (the alarm shows green), no active power use while ringing. Add with those systems.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{Areas, DmTurfBuild, DoorKind};
use ss13_shared::chat::Span;
use ss13_shared::{Dir, EMISSIVE_PLANE, Overlay, TilePos};

use super::{Look, light_of, refresh_look};
use crate::doors::{Door, DoorState, with_door};
use crate::power::{AreaPower, Unpowered};
use crate::sound::{PlaySound, SoundRng, playsound};
use crate::tools::tell;

/// `/datum/looping_sound/firealarm`: `mid_sounds`, `mid_length`, `volume`.
const SIREN: [&str; 4] = [
    "sound/machines/fire_alarm/fire_alarm1.ogg",
    "sound/machines/fire_alarm/fire_alarm2.ogg",
    "sound/machines/fire_alarm/fire_alarm3.ogg",
    "sound/machines/fire_alarm/fire_alarm4.ogg",
];
const SIREN_SECS: f64 = 2.4;
const SIREN_VOLUME: f32 = 30.0;
/// tg light colors: `LIGHT_COLOR_BLUEGREEN` (green security level), `COLOR_WHITE` (detection
/// off), `LIGHT_COLOR_INTENSE_RED` (alarm).
const BLUEGREEN: [u8; 3] = [0x7D, 0xE1, 0xAF];
const WHITE: [u8; 3] = [0xFF, 0xFF, 0xFF];
const INTENSE_RED: [u8; 3] = [0xC8, 0x19, 0x19];
/// `firedoor/update_overlays`: the hazard lights of an alarmed firelock.
const FIRELOCK_LIGHTS: &str = "firelock_alarm_type_generic";

/// `/obj/machinery/firealarm`: its `my_area` and its siren (`soundloop`).
#[derive(Component, Clone, Copy, Debug)]
pub struct FireAlarm {
    pub area: Entity,
    /// The next siren sound while it rings.
    pub ringing: Option<f64>,
}

/// An area's fire alarm state (tg `/area` `firedoors`, `active_firelocks`, `firealarms`,
/// `fire_detect`, `fault_status`); its `fire` is [`AreaPower::fire`].
#[derive(Component, Clone, Debug)]
pub struct AreaFire {
    pub firedoors: Vec<Entity>,
    pub active_firelocks: Vec<Entity>,
    pub firealarms: Vec<Entity>,
    pub fire_detect: bool,
    /// `fault_status == AREA_FAULT_MANUAL`: an alarm was pulled.
    pub manual: bool,
}

impl Default for AreaFire {
    fn default() -> Self {
        Self {
            firedoors: Vec::new(),
            active_firelocks: Vec::new(),
            firealarms: Vec::new(),
            fire_detect: true,
            manual: false,
        }
    }
}

/// `/obj/machinery/door/firedoor`: `active`, `affecting_areas`.
// shortcut: no `ignore_alarms` window after `crack_open` (it only delays atmos re-activation
// and hides the hazard lights of a generic alarm pulled within 3 s); add with `check_atmos`.
#[derive(Component, Clone, Debug)]
pub struct Firelock {
    pub active: bool,
    pub areas: Vec<Entity>,
}

/// A firelock whose door does not match `active` yet (tg `correct_state` and its re-checks after
/// a move).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct FirelockPending;

fn is_open_turf(world: &World, pos: TilePos) -> bool {
    crate::construction::turf_at(world, pos)
        .and_then(|t| world.get::<DmTurfBuild>(t))
        .is_some_and(|b| is_under(&b.path, "/turf/open"))
}

/// tg `CalculateAffectingAreas` of every firelock: its area and those of the open turfs next to
/// it.
pub(super) fn init_firelocks(world: &mut World) {
    let mut query = world.query::<(Entity, &Door, &TilePos)>();
    let firelocks: Vec<(Entity, TilePos)> = query
        .iter(world)
        .filter(|(_, d, _)| d.kind == DoorKind::Firedoor)
        .map(|(e, _, &p)| (e, p))
        .collect();
    for (e, pos) in firelocks {
        let mut areas: Vec<Entity> = Vec::new();
        let around = Dir::CARDINALS
            .into_iter()
            .map(|d| pos.step(d))
            .filter(|&p| is_open_turf(world, p));
        for p in std::iter::once(pos).chain(around) {
            if let Some(area) = world.resource::<Areas>().area_at(p)
                && !areas.contains(&area)
            {
                areas.push(area);
            }
        }
        for &area in &areas {
            area_fire(world, area).firedoors.push(e);
        }
        world.entity_mut(e).insert(Firelock {
            active: false,
            areas,
        });
    }
}

fn area_fire(world: &mut World, area: Entity) -> Mut<'_, AreaFire> {
    if world.get::<AreaFire>(area).is_none() {
        world.entity_mut(area).insert(AreaFire::default());
    }
    world.get_mut::<AreaFire>(area).expect("inserted above")
}

pub(super) fn add_alarm(world: &mut World, alarm: Entity, area: Entity) {
    area_fire(world, area).firealarms.push(alarm);
    world.entity_mut(alarm).insert(FireAlarm {
        area,
        ringing: None,
    });
}

/// `my_area.fire || LAZYLEN(my_area.active_firelocks)`.
fn alerting(world: &World, area: Entity) -> bool {
    world.get::<AreaPower>(area).is_some_and(|a| a.fire)
        || world
            .get::<AreaFire>(area)
            .is_some_and(|f| !f.active_firelocks.is_empty())
}

/// tg `update_icon_state`, `update_overlays` and `update_appearance` (light) of a secured,
/// unbroken alarm on a station level.
pub(super) fn look(world: &World, e: Entity, alarm: &FireAlarm, powered: bool) -> Look {
    let mut look = Look {
        icon_state: "fire0".to_owned(),
        overlays: Vec::new(),
        light: None,
    };
    if !powered {
        return look;
    }
    let detect = world
        .get::<AreaFire>(alarm.area)
        .is_none_or(|f| f.fire_detect);
    let (overlay, emissive, range, power, color) = if alerting(world, alarm.area) {
        ("fire_alerting", "fire_alerting_e", 2.5, 1.5, INTENSE_RED)
    } else if detect {
        // `fire_[SSsecurity_level.get_current_level_as_number()]`: green.
        ("fire_0", "fire_level_e", 1.6, 1.0, BLUEGREEN)
    } else {
        ("fire_disabled", "fire_level_e", 1.6, 1.0, WHITE)
    };
    look.overlays = vec![(overlay.to_owned(), false), (emissive.to_owned(), true)];
    look.light = light_of(world, e, range, power, color);
    look
}

fn now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

/// tg `firealarm/alarm(user)`: closes every firelock of the area and rings. `client`: the user
/// who pulled it, told so.
pub fn alarm(world: &mut World, alarm: Entity, client: Option<Entity>) {
    let Some(&FireAlarm { area, .. }) = world.get::<FireAlarm>(alarm) else {
        return;
    };
    // `is_operational`, `my_area.fire`.
    if world.get::<Unpowered>(alarm).is_some()
        || world.get::<AreaPower>(area).is_some_and(|a| a.fire)
    {
        return;
    }
    let firelocks = world
        .get::<AreaFire>(area)
        .map(|f| f.firedoors.clone())
        .unwrap_or_default();
    for firelock in firelocks {
        activate(world, firelock);
    }
    if let Some(client) = client {
        tell(world, client, Span::Notice, "triggered alarm!");
    }
    area_fire(world, area).manual = true;
    let at = now(world);
    if let Some(mut a) = world.get_mut::<FireAlarm>(alarm) {
        a.ringing = Some(at);
    }
    refresh_look(world, alarm);
}

/// tg `firealarm/reset(user)`: cracks every firelock of the area open and stops ringing.
pub fn reset(world: &mut World, alarm: Entity, client: Option<Entity>) {
    let Some(&FireAlarm { area, .. }) = world.get::<FireAlarm>(alarm) else {
        return;
    };
    if world.get::<Unpowered>(alarm).is_some() {
        return;
    }
    let firelocks = world
        .get::<AreaFire>(area)
        .map(|f| f.firedoors.clone())
        .unwrap_or_default();
    for firelock in firelocks {
        crack_open(world, firelock);
    }
    if let Some(client) = client {
        tell(world, client, Span::Notice, "reset alarm");
    }
    if let Some(mut a) = world.get_mut::<FireAlarm>(alarm) {
        a.ringing = None;
    }
    refresh_look(world, alarm);
}

/// tg `firedoor/activate(FIRELOCK_ALARM_TYPE_GENERIC)`.
pub fn activate(world: &mut World, firelock: Entity) {
    let Some(mut lock) = world.get_mut::<Firelock>(firelock) else {
        return;
    };
    if lock.active {
        return;
    }
    lock.active = true;
    let areas = lock.areas.clone();
    for area in areas {
        let mut fire = area_fire(world, area);
        if !fire.active_firelocks.contains(&firelock) {
            fire.active_firelocks.push(firelock);
        }
        // The first active firelock of the area bathes it in red.
        if fire.active_firelocks.len() == 1 {
            set_fire_effect(world, area, true);
        }
    }
    world.entity_mut(firelock).insert(FirelockPending);
    refresh_firelock(world, firelock);
}

/// tg `firedoor/crack_open()`: inactive, open.
pub fn crack_open(world: &mut World, firelock: Entity) {
    let Some(mut lock) = world.get_mut::<Firelock>(firelock) else {
        return;
    };
    lock.active = false;
    let areas = lock.areas.clone();
    for area in areas {
        let Some(mut fire) = world.get_mut::<AreaFire>(area) else {
            continue;
        };
        let Some(i) = fire.active_firelocks.iter().position(|&f| f == firelock) else {
            continue;
        };
        fire.active_firelocks.swap_remove(i);
        if fire.active_firelocks.is_empty() {
            set_fire_effect(world, area, false);
        }
    }
    world.entity_mut(firelock).insert(FirelockPending);
    refresh_firelock(world, firelock);
}

/// tg `/area/proc/set_fire_effect` and the `COMSIG_AREA_FIRE_CHANGED` handlers: the lights, and
/// every fire alarm's `set_status()`.
fn set_fire_effect(world: &mut World, area: Entity, fire: bool) {
    if !crate::power::set_area_fire(world, area, fire) {
        return;
    }
    if !fire {
        area_fire(world, area).manual = false;
    }
    let alarms = world
        .get::<AreaFire>(area)
        .map(|f| f.firealarms.clone())
        .unwrap_or_default();
    for alarm in alarms {
        set_status(world, alarm);
    }
}

/// tg `firealarm/set_status()`: the siren stops when the area calmed down.
fn set_status(world: &mut World, alarm: Entity) {
    let Some(area) = world.get::<FireAlarm>(alarm).map(|a| a.area) else {
        return;
    };
    if !alerting(world, area)
        && let Some(mut a) = world.get_mut::<FireAlarm>(alarm)
    {
        a.ringing = None;
    }
    refresh_look(world, alarm);
}

/// tg `firedoor/update_overlays`: the hazard lights while alarmed and powered.
pub(super) fn refresh_firelock(world: &mut World, firelock: Entity) {
    let lit = world.get::<Firelock>(firelock).is_some_and(|l| l.active)
        && world.get::<Unpowered>(firelock).is_none();
    let id = world
        .get::<ss13_mapload::DmAtom>(firelock)
        .map(|a| a.icon.clone())
        .and_then(|icon| {
            world
                .get_resource::<ss13_mapload::IconIndex>()?
                .get(&icon, FIRELOCK_LIGHTS)
        });
    let Some(id) = id else {
        return;
    };
    let Some(mut appearance) = world.get_mut::<ss13_shared::Appearance>(firelock) else {
        return;
    };
    let lights = [
        Overlay::new(id),
        Overlay {
            plane: Some(EMISSIVE_PLANE),
            ..Overlay::new(id)
        },
    ];
    let has = appearance.overlays.iter().any(|o| o.icon_state == id);
    if lit && !has {
        appearance.overlays.extend(lights);
    } else if !lit && has {
        appearance.overlays.retain(|o| o.icon_state != id);
    }
}

/// tg `correct_state()` of the firelocks still moving to their state: an active one closes, an
/// inactive one opens; a firelock that cannot (welded) stays as it is.
pub(super) fn correct_firelocks(world: &mut World) {
    let mut query = world.query_filtered::<(Entity, &Firelock), With<FirelockPending>>();
    let pending: Vec<(Entity, bool)> = query.iter(world).map(|(e, l)| (e, l.active)).collect();
    for (e, active) in pending {
        let settled = with_door(world, e, |door, appearance, ctx| {
            if door.is_operating() {
                return (false, false);
            }
            let moved = match (active, door.state) {
                (true, DoorState::Open) => ctx.close(door, appearance, false),
                (false, DoorState::Closed) => ctx.open(door, appearance, false),
                _ => false,
            };
            (!moved, moved)
        });
        if settled != Some(false) {
            world.entity_mut(e).remove::<FirelockPending>();
        }
    }
}

/// The sirens of the ringing alarms (`soundloop`), one random sound per 2.4 s.
pub(super) fn ring_alarms(world: &mut World) {
    let t = now(world);
    let mut query = world.query::<(Entity, &FireAlarm, &TilePos)>();
    let due: Vec<(Entity, TilePos)> = query
        .iter(world)
        .filter(|(_, a, _)| a.ringing.is_some_and(|next| t >= next))
        .map(|(e, _, &p)| (e, p))
        .collect();
    for (e, pos) in due {
        let i = world
            .resource_mut::<SoundRng>()
            .0
            .range(0, SIREN.len() as u32 - 1) as usize;
        playsound(world, PlaySound::new(pos, SIREN[i], SIREN_VOLUME, false));
        if let Some(mut a) = world.get_mut::<FireAlarm>(e) {
            a.ringing = Some(t + SIREN_SECS);
        }
    }
}
