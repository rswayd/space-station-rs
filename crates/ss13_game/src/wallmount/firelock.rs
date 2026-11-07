//! Firelocks (tg `code/game/machinery/doors/firedoor.dm`, with BandaStation's opening by hand).
//!
//! A firelock serves its own area and the areas of the open turfs next to it
//! (`CalculateAffectingAreas`). It watches its own tile and the four next to it in the air
//! ([`ss13_atmos::Atmos::watch`]: tg `COMSIG_TURF_EXPOSE` and
//! `COMSIG_TURF_CALCULATED_ADJACENT_ATMOS`): whenever such a tile processes or its adjacency
//! changes, `process_results` reads its air. Air at or above `FIRE_MINIMUM_TEMPERATURE_TO_EXIST`
//! or with antinoblium is hot, at or below `BODYTEMP_COLD_DAMAGE_LIMIT` cold; a tile that shares
//! air and is hot or cold becomes an issue of the firelock's row (the cardinally connected
//! firelocks, tg's merge group, sharing one issue list) and the row activates with that alarm
//! type: every firelock closes, shows its hazard lights, makes its areas `fire` (red lights, the
//! fire alarms alert) and the one that noticed rings. The last issue tile clearing resets the
//! row: inactive, open, quiet.
//!
//! By hand (BandaStation): a closed firelock opens after a 5 s `do_after`; an active one closes
//! again 2 s later. A crowbar (left click) holds a closed one open while its user stands next to
//! it, and holds off atmos re-activation for 3 s; a right click opens it (an active one closes
//! again after 2 s); on an open one either click closes it. A fire alarm's reset cracks the
//! area's firelocks open and ignores alarms for 3 s, then re-activates them if an issue remains.
//!
//! Cost: nothing per tick but the firelocks with something pending (moving to their state, a
//! timer, held open); the air side costs one flag test per processed tile, and a firelock reads
//! a tile's air only when it was reported.
// shortcut: no welding (`welded` overlays, deconstruction), emag, silicon or alien opening,
// bumping, `TRAIT_FIREDOOR_STOP`, alarm manager reports (consoles, AI) or random `id_tag` names;
// a held-open firelock lets go only when its user moves away or lies down (checked each tick),
// not on deletion of a non-mob holder. Turfs changing next to a firelock do not recompute its
// areas (`adjacent_change`).

use std::collections::HashMap;

use bevy::prelude::*;
use ss13_atmos::{Atmos, GasId};
use ss13_mapload::{Areas, DmAtom, DmName, DoorKind};
use ss13_shared::chat::{ChatChannel, Span};
use ss13_shared::{Appearance, Dir, EMISSIVE_PLANE, Overlay, TilePos};
use ss13_world::Border;

use super::WallJob;
use super::fire_alarm::{self, AreaFire, Fault};
use crate::construction::Job;
use crate::doors::{Door, DoorState, with_door};
use crate::health::BodyPosition;
use crate::power::Unpowered;
use crate::sound::{PlaySound, playsound};
use crate::tools::{Actor, tell};

/// `FIRE_MINIMUM_TEMPERATURE_TO_EXIST`, `BODYTEMP_COLD_DAMAGE_LIMIT` (`BODYTEMP_NORMAL - 40`).
const HOT: f64 = ss13_atmos::reactions::FIRE_MINIMUM_TEMPERATURE_TO_EXIST;
const COLD: f64 = 310.15 - 40.0;
/// `REACTIVATION_DELAY`, `crack_open`'s `release_constraints` delay, the `correct_state` timer
/// after opening an active firelock, BandaStation's hand opening `do_after` (deciseconds).
const REACTIVATION_DELAY: f64 = 3.0;
const RELEASE_DELAY: f64 = 3.0;
const RECHECK_DELAY: f64 = 2.0;
const HAND_OPEN_DS: f32 = 50.0;
const BASH_SOUND: &str = "sound/effects/glass/glassbash.ogg";

/// tg `FIRELOCK_ALARM_TYPE_*`: the hazard light icon state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlarmType {
    Hot,
    Cold,
    Generic,
}

/// The hazard lights of an emagged firelock.
const EMAG_LIGHTS: &str = "firelock_alarm_type_emag";

impl AlarmType {
    pub const ALL: [Self; 3] = [Self::Hot, Self::Cold, Self::Generic];

    pub fn icon_state(self) -> &'static str {
        match self {
            Self::Hot => "firelock_alarm_type_hot",
            Self::Cold => "firelock_alarm_type_cold",
            Self::Generic => "firelock_alarm_type_generic",
        }
    }
}

/// `/obj/machinery/door/firedoor`.
#[derive(Component, Clone, Debug)]
pub struct Firelock {
    pub active: bool,
    /// `affecting_areas`.
    pub areas: Vec<Entity>,
    /// `my_area`: the area it reports faults to.
    pub my_area: Option<Entity>,
    pub alarm_type: Option<AlarmType>,
    pub ignore_alarms: bool,
    /// Its row in [`Firelocks::groups`].
    pub group: usize,
    /// `soundloop` (`is_playing_alarm`): when the next siren sound plays.
    pub ringing: Option<f64>,
    /// `activation_cooldown`.
    pub cooldown_until: f64,
    /// A non-generic `activate` waiting for the cooldown.
    pub activate_at: Option<(f64, AlarmType)>,
    /// `release_constraints` after `crack_open`.
    pub release_at: Option<f64>,
    /// `correct_state` after an active firelock was opened.
    pub recheck_at: Option<f64>,
    /// `being_held_open` by this mob.
    pub held_by: Option<Entity>,
    /// `correct_state` pending: the door does not match `active` yet.
    pub settle: bool,
}

/// A firelock whose siren runs ([`Firelock::ringing`]), for the siren pass.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Ringing;

/// A firelock with something in [`Firelock`] pending; [`run_firelocks`] watches only these.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct FirelockPending;

/// A row of firelocks (tg `datum/merger` "firelocks"): its members and shared `issue_turfs`.
#[derive(Debug, Default, Clone)]
pub struct FirelockGroup {
    pub members: Vec<Entity>,
    pub issues: Vec<TilePos>,
}

/// The rows and which firelocks watch which tile.
// shortcut: firelocks exist only from map load and never move; rebuild rows and watches when
// construction makes them.
#[derive(Resource, Default, Debug)]
pub struct Firelocks {
    pub groups: Vec<FirelockGroup>,
    pub watch: HashMap<TilePos, Vec<Entity>>,
}

use crate::tools::now;

fn is_open_turf(world: &World, pos: TilePos) -> bool {
    use ss13_mapload::DmTurfBuild;
    use ss13_mapload::fidelity::is_under;
    crate::construction::turf_at(world, pos)
        .and_then(|t| world.get::<DmTurfBuild>(t))
        .is_some_and(|b| is_under(&b.path, "/turf/open"))
}

/// tg `Initialize`/`post_machine_initialize` of every firelock: `CalculateAffectingAreas`,
/// `my_area`, the rows, the watched tiles; a firelock mapped with an alarm (`/closed`) starts
/// its row's activation.
pub(super) fn init_firelocks(world: &mut World) {
    let mut query = world.query_filtered::<(Entity, &Door, &TilePos), With<ss13_world::Mapped>>();
    let firelocks: Vec<(Entity, TilePos)> = query
        .iter(world)
        .filter(|(_, d, _)| d.kind == DoorKind::Firedoor)
        .map(|(e, _, &p)| (e, p))
        .collect();
    let mut at: HashMap<TilePos, Vec<Entity>> = HashMap::new();
    for &(e, pos) in &firelocks {
        at.entry(pos).or_default().push(e);
    }
    // The rows: cardinally connected tiles with firelocks.
    let mut group_of: HashMap<TilePos, usize> = HashMap::new();
    let mut groups: Vec<FirelockGroup> = Vec::new();
    for &(_, start) in &firelocks {
        if group_of.contains_key(&start) {
            continue;
        }
        let g = groups.len();
        let mut group = FirelockGroup::default();
        let mut queue = vec![start];
        group_of.insert(start, g);
        while let Some(pos) = queue.pop() {
            group.members.extend(at[&pos].iter().copied());
            for d in Dir::CARDINALS {
                let n = pos.step(d);
                if at.contains_key(&n) && !group_of.contains_key(&n) {
                    group_of.insert(n, g);
                    queue.push(n);
                }
            }
        }
        groups.push(group);
    }
    // A later load's rows come after the station's.
    let base = world
        .get_resource::<Firelocks>()
        .map_or(0, |f| f.groups.len());
    let mut watch: HashMap<TilePos, Vec<Entity>> = HashMap::new();
    let mut preset = Vec::new();
    for &(e, pos) in &firelocks {
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
            fire_alarm::area_fire(world, area).firedoors.push(e);
        }
        for p in std::iter::once(pos).chain(Dir::CARDINALS.map(|d| pos.step(d))) {
            watch.entry(p).or_default().push(e);
        }
        // `/obj/machinery/door/firedoor/closed` and its border twin map an alarm.
        let mapped_alarm =
            crate::construction::path_of(world, e).is_some_and(|p| p.ends_with("/closed"));
        if mapped_alarm {
            preset.push(e);
        }
        let my_area = world.resource::<Areas>().area_at(pos);
        world.entity_mut(e).insert(Firelock {
            active: false,
            areas,
            my_area,
            alarm_type: None,
            ignore_alarms: false,
            group: base + group_of[&pos],
            ringing: None,
            cooldown_until: 0.0,
            activate_at: None,
            release_at: None,
            recheck_at: None,
            held_by: None,
            settle: false,
        });
    }
    if let Some(mut atmos) = world.get_resource_mut::<Atmos>() {
        for &pos in watch.keys() {
            atmos.watch(pos, true);
        }
    }
    let mut all = world.remove_resource::<Firelocks>().unwrap_or_default();
    all.groups.extend(groups);
    for (pos, list) in watch {
        all.watch.entry(pos).or_default().extend(list);
    }
    world.insert_resource(all);
    for e in preset {
        if let Some(mut lock) = world.get_mut::<Firelock>(e) {
            lock.alarm_type = Some(AlarmType::Generic);
        }
        start_activation_process(world, e, AlarmType::Generic);
    }
}

/// tg `Initialize` of a firelock built from a frame (`crate::door_structures`): its areas,
/// watched tiles and row as [`init_firelocks`] gives mapped ones. It joins the row of a firelock
/// next to it, or starts its own.
// shortcut: two rows a new firelock connects are not merged (tg `datum/merger` would); it joins
// the first one found.
pub fn setup_built(world: &mut World, e: Entity) {
    let Some(&pos) = world.get::<TilePos>(e) else {
        return;
    };
    let mut areas: Vec<Entity> = Vec::new();
    let around: Vec<TilePos> = Dir::CARDINALS
        .into_iter()
        .map(|d| pos.step(d))
        .filter(|&p| is_open_turf(world, p))
        .collect();
    for p in std::iter::once(pos).chain(around) {
        if let Some(area) = world.resource::<Areas>().area_at(p)
            && !areas.contains(&area)
        {
            areas.push(area);
        }
    }
    for &area in &areas {
        fire_alarm::area_fire(world, area).firedoors.push(e);
    }
    let watched: Vec<TilePos> = std::iter::once(pos)
        .chain(Dir::CARDINALS.map(|d| pos.step(d)))
        .collect();
    let neighbour_group = watched
        .iter()
        .flat_map(|&p| {
            world
                .resource::<ss13_world::SpatialIndex>()
                .on_tile(p)
                .to_vec()
        })
        .find_map(|n| world.get::<Firelock>(n).map(|f| f.group));
    let my_area = world.resource::<Areas>().area_at(pos);
    let mut all = world.remove_resource::<Firelocks>().unwrap_or_default();
    let group = match neighbour_group {
        Some(g) if g < all.groups.len() => g,
        _ => {
            all.groups.push(FirelockGroup::default());
            all.groups.len() - 1
        }
    };
    all.groups[group].members.push(e);
    for &p in &watched {
        all.watch.entry(p).or_default().push(e);
    }
    world.insert_resource(all);
    if let Some(mut atmos) = world.get_resource_mut::<Atmos>() {
        for &p in &watched {
            atmos.watch(p, true);
        }
    }
    world.entity_mut(e).insert(Firelock {
        active: false,
        areas,
        my_area,
        alarm_type: None,
        ignore_alarms: false,
        group,
        ringing: None,
        cooldown_until: 0.0,
        activate_at: None,
        release_at: None,
        recheck_at: None,
        held_by: None,
        settle: false,
    });
}

/// A firelock about to be taken apart (`crate::door_structures`): out of its row, its areas'
/// lists and the watch lists, so nothing keeps its entity.
pub fn teardown(world: &mut World, e: Entity) {
    let Some(lock) = world.get::<Firelock>(e).cloned() else {
        return;
    };
    for &area in &lock.areas {
        let mut fire = fire_alarm::area_fire(world, area);
        fire.firedoors.retain(|&d| d != e);
        fire.active_firelocks.retain(|&d| d != e);
    }
    if let Some(mut all) = world.get_resource_mut::<Firelocks>() {
        if let Some(group) = all.groups.get_mut(lock.group) {
            group.members.retain(|&m| m != e);
        }
        for list in all.watch.values_mut() {
            list.retain(|&m| m != e);
        }
    }
    world
        .entity_mut(e)
        .remove::<(Firelock, FirelockPending, Ringing)>();
}

/// tg `check_atmos(turf)`; `None` where there is no air (walls, off the map).
pub fn check_atmos(atmos: &Atmos, pos: TilePos) -> Option<AlarmType> {
    let air = atmos.air(pos)?;
    if air.temperature >= HOT
        || air.get(GasId::ANTINOBLIUM) > ss13_atmos::reactions::MINIMUM_MOLE_COUNT
    {
        return Some(AlarmType::Hot);
    }
    (air.temperature <= COLD).then_some(AlarmType::Cold)
}

/// Reported watched tiles: `process_results` of every firelock watching each.
pub(super) fn firelock_atmos(world: &mut World) {
    let Some(reports) = world
        .get_resource_mut::<Atmos>()
        .map(|mut a| a.drain_watch_reports())
    else {
        return;
    };
    for pos in reports {
        let watchers = world
            .get_resource::<Firelocks>()
            .and_then(|f| f.watch.get(&pos).cloned())
            .unwrap_or_default();
        for firelock in watchers {
            process_results(world, firelock, pos);
        }
    }
}

/// tg `process_results(turf)`.
// tg also resets a row whose alarm is generic when any watched tile without an issue processes
// while other issues remain; the alarm stays here until the last issue clears.
pub fn process_results(world: &mut World, firelock: Entity, pos: TilePos) {
    let Some(lock) = world.get::<Firelock>(firelock).cloned() else {
        return;
    };
    let detect = lock
        .areas
        .iter()
        .all(|&a| world.get::<AreaFire>(a).is_none_or(|f| f.fire_detect));
    if !detect {
        return;
    }
    let atmos = world.resource::<Atmos>();
    let result = check_atmos(atmos, pos);
    // `TURF_SHARES`.
    let shares = !atmos.adjacent_turfs(pos, false).is_empty();
    // `Some(alarm)`: the row's alarm changes to `alarm`.
    let change = {
        let mut firelocks = world.resource_mut::<Firelocks>();
        let issues = &mut firelocks.groups[lock.group].issues;
        if let (Some(result), true) = (result, shares) {
            if !issues.contains(&pos) {
                issues.push(pos);
            }
            lock.alarm_type.is_none().then_some(Some(result))
        } else if let Some(i) = issues.iter().position(|&p| p == pos) {
            issues.swap_remove(i);
            issues.is_empty().then_some(None)
        } else {
            None
        }
    };
    let Some(alarm) = change else {
        return;
    };
    set(world, firelock, |l| l.alarm_type = alarm);
    if lock.ignore_alarms {
        return;
    }
    match alarm {
        Some(code) => start_activation_process(world, firelock, code),
        None => start_deactivation_process(world, firelock),
    }
}

fn set(world: &mut World, firelock: Entity, f: impl FnOnce(&mut Firelock)) {
    let Some(mut lock) = world.get_mut::<Firelock>(firelock) else {
        return;
    };
    f(&mut lock);
    let ringing = lock.ringing.is_some();
    let mut e = world.entity_mut(firelock);
    if ringing != e.contains::<Ringing>() {
        if ringing {
            e.insert(Ringing);
        } else {
            e.remove::<Ringing>();
        }
    }
}

fn members(world: &World, firelock: Entity) -> Vec<Entity> {
    world
        .get::<Firelock>(firelock)
        .and_then(|l| world.resource::<Firelocks>().groups.get(l.group))
        .map(|g| g.members.clone())
        .unwrap_or_default()
}

fn name_of(world: &World, e: Entity) -> String {
    world.get::<DmName>(e).map_or_else(
        || "firelock".to_owned(),
        |n| crate::vending::plain_name(&n.name).to_owned(),
    )
}

/// tg `start_activation_process(code)`: this one rings, its area records the fault, the row
/// activates.
pub fn start_activation_process(world: &mut World, firelock: Entity, code: AlarmType) {
    let Some(lock) = world.get::<Firelock>(firelock).cloned() else {
        return;
    };
    if lock.active {
        return;
    }
    let t = now(world);
    set(world, firelock, |l| l.ringing = Some(t));
    if let Some(area) = lock.my_area {
        let name = name_of(world, firelock);
        let mut fire = fire_alarm::area_fire(world, area);
        fire.fault = Fault::Automatic;
        fire.fault_location = name;
    }
    for buddy in members(world, firelock) {
        activate(world, buddy, code);
    }
}

/// tg `start_deactivation_process()`: quiet, no fault, the row resets.
pub fn start_deactivation_process(world: &mut World, firelock: Entity) {
    let Some(my_area) = world.get::<Firelock>(firelock).map(|l| l.my_area) else {
        return;
    };
    set(world, firelock, |l| l.ringing = None);
    if let Some(area) = my_area {
        let mut fire = fire_alarm::area_fire(world, area);
        fire.fault = Fault::None;
        fire.fault_location.clear();
    }
    for buddy in members(world, firelock) {
        reset(world, buddy);
    }
}

/// tg `activate(code)`: closes, lights up and makes its areas `fire`. A non-generic code waits
/// out a crowbar's `activation_cooldown`.
pub fn activate(world: &mut World, firelock: Entity, code: AlarmType) {
    let Some(lock) = world.get::<Firelock>(firelock).cloned() else {
        return;
    };
    if lock.active || (lock.ignore_alarms && code != AlarmType::Generic) {
        return;
    }
    let t = now(world);
    if code != AlarmType::Generic && t < lock.cooldown_until {
        set(world, firelock, |l| {
            l.activate_at = Some((lock.cooldown_until, code))
        });
        world.entity_mut(firelock).insert(FirelockPending);
        return;
    }
    set(world, firelock, |l| {
        l.active = true;
        l.alarm_type = Some(code);
    });
    add_as_source(world, firelock);
    correct_state(world, firelock);
    refresh_firelock(world, firelock);
}

/// tg `reset()`.
pub fn reset(world: &mut World, firelock: Entity) {
    if world.get::<Firelock>(firelock).is_none() {
        return;
    }
    set(world, firelock, |l| {
        l.alarm_type = None;
        l.active = false;
        l.ringing = None;
    });
    remove_as_source(world, firelock);
    correct_state(world, firelock);
    refresh_firelock(world, firelock);
}

/// tg `crack_open()`: open without clearing an atmos alarm, which comes back 3 s later.
pub fn crack_open(world: &mut World, firelock: Entity) {
    let Some(group) = world.get::<Firelock>(firelock).map(|l| l.group) else {
        return;
    };
    let issues = !world.resource::<Firelocks>().groups[group]
        .issues
        .is_empty();
    let t = now(world);
    set(world, firelock, |l| {
        l.active = false;
        l.ignore_alarms = true;
        if !issues {
            l.alarm_type = None;
        }
        l.ringing = None;
        l.release_at = Some(t + RELEASE_DELAY);
    });
    remove_as_source(world, firelock);
    correct_state(world, firelock);
    world.entity_mut(firelock).insert(FirelockPending);
    refresh_firelock(world, firelock);
}

/// tg `release_constraints()`.
fn release_constraints(world: &mut World, firelock: Entity) {
    set(world, firelock, |l| l.ignore_alarms = false);
    refresh_firelock(world, firelock);
    let Some(lock) = world.get::<Firelock>(firelock).cloned() else {
        return;
    };
    // tg reactivates on the firelock's own `alarm_type` even when the row's issues cleared
    // while it ignored alarms (another firelock of the row saw the last one go), leaving the
    // row closed with nothing to reset it; only a remaining issue brings the alarm back.
    if world.resource::<Firelocks>().groups[lock.group]
        .issues
        .is_empty()
    {
        set(world, firelock, |l| l.alarm_type = None);
        refresh_firelock(world, firelock);
        return;
    }
    let Some(code) = lock.alarm_type.filter(|_| !lock.active) else {
        return;
    };
    let detect = lock
        .areas
        .iter()
        .all(|&a| world.get::<AreaFire>(a).is_none_or(|f| f.fire_detect));
    if detect {
        start_activation_process(world, firelock, code);
    }
}

/// tg `add_as_source()`: the first active firelock of an area bathes it in red.
fn add_as_source(world: &mut World, firelock: Entity) {
    let Some(areas) = world.get::<Firelock>(firelock).map(|l| l.areas.clone()) else {
        return;
    };
    let name = name_of(world, firelock);
    for area in areas {
        let mut fire = fire_alarm::area_fire(world, area);
        if !fire.active_firelocks.contains(&firelock) {
            fire.active_firelocks.push(firelock);
        }
        if fire.active_firelocks.len() == 1 {
            fire_alarm::set_fire_effect(world, area, true, Fault::Automatic, &name);
        }
    }
}

/// tg `remove_as_source()`: the last one going clears the area.
fn remove_as_source(world: &mut World, firelock: Entity) {
    let Some(areas) = world.get::<Firelock>(firelock).map(|l| l.areas.clone()) else {
        return;
    };
    for area in areas {
        let Some(mut fire) = world.get_mut::<AreaFire>(area) else {
            continue;
        };
        let Some(i) = fire.active_firelocks.iter().position(|&f| f == firelock) else {
            continue;
        };
        fire.active_firelocks.swap_remove(i);
        if fire.active_firelocks.is_empty() {
            fire_alarm::set_fire_effect(world, area, false, Fault::None, "");
        }
    }
}

/// tg `correct_state()`: schedules the move to `active` ([`run_firelocks`]); an emagged
/// firelock (`crate::emag`) stays as it is.
fn correct_state(world: &mut World, firelock: Entity) {
    if world.get::<Door>(firelock).is_some_and(|d| d.emagged) {
        return;
    }
    set(world, firelock, |l| l.settle = true);
    world.entity_mut(firelock).insert(FirelockPending);
}

/// tg `update_overlays`: the hazard lights of an alarm (`firelock_alarm_type_emag` once
/// emagged), powered and not ignoring alarms; a border firelock's lights sit 2 px towards its
/// edge (`adjust_lights_starting_offset`).
pub(crate) fn refresh_firelock(world: &mut World, firelock: Entity) {
    let Some(lock) = world.get::<Firelock>(firelock) else {
        return;
    };
    let lit = lock
        .alarm_type
        .filter(|_| !lock.ignore_alarms && world.get::<Unpowered>(firelock).is_none());
    let Some(icon) = world.get::<DmAtom>(firelock).map(|a| a.icon.clone()) else {
        return;
    };
    let offset = match world
        .get::<Border>(firelock)
        .and(world.get::<DmAtom>(firelock))
    {
        Some(atom) => Dir::from_bits(atom.dir).map_or([0, 0], |d| {
            let (dx, dy) = d.offset();
            [dx as i16 * 2, dy as i16 * 2]
        }),
        None => [0, 0],
    };
    let Some(index) = world.get_resource::<ss13_mapload::IconIndex>() else {
        return;
    };
    let emagged = world.get::<Door>(firelock).is_some_and(|d| d.emagged);
    let ids: Vec<_> = AlarmType::ALL
        .iter()
        .map(|t| t.icon_state())
        .chain([EMAG_LIGHTS])
        .filter_map(|state| index.get(&icon, state))
        .collect();
    let want: Vec<Overlay> = lit
        .and_then(|t| index.get(&icon, if emagged { EMAG_LIGHTS } else { t.icon_state() }))
        .map(|id| {
            let lights = Overlay {
                pixel_offset: offset,
                ..Overlay::new(id)
            };
            let emissive = Overlay {
                plane: Some(EMISSIVE_PLANE),
                ..lights.clone()
            };
            vec![lights, emissive]
        })
        .unwrap_or_default();
    let Some(mut appearance) = world.get_mut::<Appearance>(firelock) else {
        return;
    };
    let mut overlays: Vec<Overlay> = appearance
        .overlays
        .iter()
        .filter(|o| !ids.contains(&o.icon_state))
        .cloned()
        .collect();
    overlays.extend(want);
    if appearance.overlays != overlays {
        appearance.overlays = overlays;
    }
}

/// The pending firelocks: due timers, held-open users still there, doors moving to `active`
/// (a welded or bolted one cannot and stays).
pub(super) fn run_firelocks(world: &mut World) {
    let t = now(world);
    let mut query = world.query_filtered::<(Entity, &Firelock), With<FirelockPending>>();
    let pending: Vec<(Entity, Firelock)> = query.iter(world).map(|(e, l)| (e, l.clone())).collect();
    for (e, lock) in pending {
        if let Some((at, code)) = lock.activate_at
            && t >= at
        {
            set(world, e, |l| l.activate_at = None);
            activate(world, e, code);
        }
        if lock.release_at.is_some_and(|at| t >= at) {
            set(world, e, |l| l.release_at = None);
            release_constraints(world, e);
        }
        if lock.recheck_at.is_some_and(|at| t >= at) {
            set(world, e, |l| l.recheck_at = None);
            correct_state(world, e);
        }
        if let Some(holder) = lock.held_by
            && !holds_open(world, e, holder)
        {
            set(world, e, |l| l.held_by = None);
            correct_state(world, e);
            if let Some(client) = world
                .get::<crate::server::PlayerMob>(holder)
                .map(|p| p.client)
            {
                tell(world, client, Span::Notice, "released firelock");
            }
        }
        let Some(lock) = world.get::<Firelock>(e).cloned() else {
            continue;
        };
        let mut settle = lock.settle;
        if settle && lock.held_by.is_none() {
            let active = lock.active;
            let done = with_door(world, e, |door, appearance, ctx| {
                if door.emagged {
                    return (true, false);
                }
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
            settle = done == Some(false);
        } else if lock.held_by.is_some() {
            // `correct_state` returns while held open; the release asks again.
            settle = false;
        }
        set(world, e, |l| l.settle = settle);
        let busy = settle
            || lock.activate_at.is_some()
            || lock.release_at.is_some()
            || lock.recheck_at.is_some()
            || lock.held_by.is_some();
        if !busy {
            world.entity_mut(e).remove::<FirelockPending>();
        }
    }
}

/// tg `handle_held_open_adjacency`: the user is next to it and standing.
fn holds_open(world: &World, firelock: Entity, holder: Entity) -> bool {
    let (Some(&a), Some(&b)) = (world.get::<TilePos>(firelock), world.get::<TilePos>(holder))
    else {
        return false;
    };
    a.chebyshev(b).is_some_and(|d| d <= 1)
        && world.get::<BodyPosition>(holder).is_none_or(|p| !p.lying())
}

/// tg `attack_hand` with BandaStation's edit: out of combat mode a closed firelock opens after
/// 5 s (`do_after`); in combat mode it is bashed. True when `target` is a firelock.
pub(super) fn attack_hand(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if world.get::<Firelock>(target).is_none() {
        return false;
    }
    let Some(door) = world.get::<Door>(target) else {
        return true;
    };
    if door.is_operating() || door.state != DoorState::Closed {
        return true;
    }
    let welded = door.welded;
    let t = now(world);
    world
        .entity_mut(mob)
        .insert(crate::combat::NextMove(t + crate::combat::CLICK_CD_MELEE));
    let combat = world
        .get::<crate::combat::CombatMode>(mob)
        .is_some_and(|c| c.0);
    let Some(&pos) = world.get::<TilePos>(target) else {
        return true;
    };
    if combat {
        let (user, name) = (name_of(world, mob), name_of(world, target));
        let chat = crate::pull::visible_message(
            world,
            pos,
            ChatChannel::Combat,
            (Span::Warning, format!("{user} bashes {name}!")),
            &[(mob, Span::Warning, format!("You bash {name}!"))],
        );
        for message in chat {
            world.write_message(message);
        }
        playsound(world, PlaySound::new(pos, BASH_SOUND, 100.0, true));
        return true;
    }
    if !welded {
        crate::tools::do_after(
            world,
            Actor {
                client,
                mob,
                item: None,
            },
            target,
            HAND_OPEN_DS,
            0,
            Job::WallMount(WallJob::OpenFirelock),
        );
    }
    true
}

/// The end of the hand opening's `do_after`: still closed, it opens; an active one closes again
/// 2 s later.
pub(super) fn finish_hand_open(world: &mut World, mob: Entity, firelock: Entity) {
    let opened = with_door(world, firelock, |door, appearance, ctx| {
        if door.state != DoorState::Closed {
            return (false, false);
        }
        let moved = ctx.open(door, appearance, false);
        (moved, moved)
    });
    if opened != Some(true) {
        return;
    }
    let Some(&pos) = world.get::<TilePos>(firelock) else {
        return;
    };
    let (user, name) = (name_of(world, mob), name_of(world, firelock));
    let chat = crate::pull::visible_message(
        world,
        pos,
        ChatChannel::Local,
        (Span::Notice, format!("{user} opens {name}.")),
        &[(mob, Span::Notice, format!("You open {name}."))],
    );
    for message in chat {
        world.write_message(message);
    }
    schedule_recheck(world, firelock);
}

/// `if(active) addtimer(CALLBACK(src, PROC_REF(correct_state)), 2 SECONDS, TIMER_UNIQUE)`.
fn schedule_recheck(world: &mut World, firelock: Entity) {
    let t = now(world);
    let Some(mut lock) = world.get_mut::<Firelock>(firelock) else {
        return;
    };
    if lock.active && lock.recheck_at.is_none() {
        lock.recheck_at = Some(t + RECHECK_DELAY);
        world.entity_mut(firelock).insert(FirelockPending);
    }
}

/// tg `try_to_crowbar` (left click) and `try_to_crowbar_secondary` (right click) by `actor`.
/// True when `target` is a firelock.
pub fn crowbar_act(world: &mut World, actor: Actor, target: Entity, secondary: bool) -> bool {
    if world.get::<Firelock>(target).is_none() {
        return false;
    }
    let Some((state, operating, welded)) = world
        .get::<Door>(target)
        .map(|d| (d.state, d.is_operating(), d.welded))
    else {
        return true;
    };
    if welded || operating {
        return true;
    }
    if state != DoorState::Closed {
        with_door(world, target, |door, appearance, ctx| {
            let moved = ctx.close(door, appearance, false);
            ((), moved)
        });
        return true;
    }
    if secondary {
        with_door(world, target, |door, appearance, ctx| {
            let moved = ctx.open(door, appearance, false);
            ((), moved)
        });
        schedule_recheck(world, target);
        return true;
    }
    let t = now(world);
    set(world, target, |l| {
        l.held_by = Some(actor.mob);
        l.cooldown_until = t + REACTIVATION_DELAY;
    });
    world.entity_mut(target).insert(FirelockPending);
    tell(world, actor.client, Span::Notice, "holding firelock open");
    with_door(world, target, |door, appearance, ctx| {
        let moved = ctx.open(door, appearance, false);
        ((), moved)
    });
    true
}

/// tg `firedoor/examine`.
pub(super) fn examine_lines(world: &World, target: Entity, lines: &mut Vec<(Span, String)>) {
    if world.get::<Firelock>(target).is_none() {
        return;
    }
    let Some(door) = world.get::<Door>(target) else {
        return;
    };
    let dense = world.get::<ss13_world::Density>(target).is_some();
    let text: &[&str] = if !dense {
        &["It is open, but could be pried closed."]
    } else if !door.welded {
        &[
            "It is closed, but could be pried open.",
            "Hold the firelock temporarily open by prying it with left-click and standing next \
             to it.",
            "Prying by right-clicking the firelock will open it permanently.",
            "Deconstruction would require it to be welded shut.",
        ]
    } else {
        &["It is welded shut. The floor bolts have been locked by screws."]
    };
    lines.extend(text.iter().map(|t| (Span::Notice, (*t).to_owned())));
}

#[cfg(test)]
mod tests {
    use ss13_atmos::level::LevelAir;
    use ss13_atmos::{GasMixture, TileKind};

    use super::*;

    /// tg `check_atmos`: hot from 373.15 K or with antinoblium, cold at 270.15 K and below,
    /// nothing for a wall.
    #[test]
    fn check_atmos_thresholds() {
        let mixes = [
            "o2=22;n2=82;TEMP=293.15",
            "o2=22;n2=82;TEMP=373.15",
            "o2=22;n2=82;TEMP=373.0",
            "o2=22;n2=82;TEMP=270.15",
            "o2=22;n2=82;TEMP=271",
            "o2=22;n2=82;antinoblium=0.02;TEMP=293.15",
        ];
        let mut level = LevelAir::new(mixes.len() as u32 + 1, 1);
        for (i, mix) in mixes.iter().enumerate() {
            level.set_tile(i as u32, TileKind::Open, GasMixture::parse(mix).ok(), None);
        }
        level.set_tile(mixes.len() as u32, TileKind::Blocked, None, None);
        let mut atmos = Atmos::default();
        atmos.insert_level(1, level);
        let at = |x| check_atmos(&atmos, TilePos::new(x, 1, 1));
        use AlarmType::{Cold, Hot};
        assert_eq!(
            (1..=7).map(at).collect::<Vec<_>>(),
            [None, Some(Hot), None, Some(Cold), None, Some(Hot), None]
        );
    }
}
