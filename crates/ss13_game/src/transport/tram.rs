//! Trams (server): tg `/datum/transport_controller/linear/tram` (`tram/tram_controller.dm`),
//! `SStransport`'s requests (`incoming_request`, `pre_departure`, `validate_and_dispatch`), its
//! navigation landmarks (`transport_navigation.dm`), the controls console
//! `/obj/machinery/computer/tram_controls` (`tram_controls.dm`, window `TramControl.tsx`), the
//! platform call buttons (`/obj/item/assembly/control/transport/call_button`,
//! `tram_machinery.dm`), the controller cabinet on the tram and the tram doors
//! (`/obj/machinery/door/airlock/tram`, `tram_doors.dm`).
//!
//! - The tram knows its nav beacon (the `tram/nav` landmark riding on it) and the platform it
//!   idles at (the `tram/platform` landmark under it). A request names a platform code; it fails
//!   (and the caller is told) when the tram is gone, unpowered or without its cabinet, faulted or
//!   emergency stopped, busy, the platform is unknown or already where the tram is. Otherwise
//!   the route is the direction and distance from the nav beacon to the platform.
//! - Departure: the controls lock, a chime, 1.4 s later the tram doors close, and every 3 s the
//!   doors are checked: still open ones are told "Пожалуйста, отойдите от дверей!" and closed
//!   again, from the third try forced; after four tries it leaves anyway.
//! - It moves a tile per `internal_movement_delay` (0.05 s, the slow tram 0.3 s), announcing the
//!   next station from 39 tiles out (every 4 s). On arrival the doors open, the controls unlock
//!   after 2 s and the platform's jingle plays. Arriving unpowered it stops hard
//!   (`degraded_stop`): a crash and every passenger is thrown forward. A fault or an emergency
//!   stop halts it at once the same way and it forgets its platform; `reset_position` drives it
//!   to the nearest platform ahead to recover.
//! - The controls console lists the platforms of its line and sends the tram; it closes while
//!   the tram is busy. A call button sends the tram to its platform (`id`) and speaks the answer.
// shortcut: no controller cabinet window (`TramController`: emergency stop, reset, door and
// sensor switches; [`estop`] and [`reset_position`] are the way in), no cover, lock, emag or
// repairs of the cabinet, no home controller in telecoms (`COMM_ERROR`), no malfunction event,
// no immovable rod pushes, no tram registration plaque or persistence, no TTS voice, no rapid
// mode, no platform icons in the console, no multitool re-pointing of call buttons, no
// emergency release of the doors, no crossing signals, sensors, destination signs,
// power rectifiers or energized plates (the cabinet's own power is the tram's).

use std::collections::HashMap;

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmAtom, DmName, DmObj, DmTransport};
use ss13_shared::TilePos;
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};
use ss13_world::{Density, Mapped, SpatialIndex};

use super::{
    BYPASS_SENSORS, CONTROLS_LOCKED, DOORS_READY, EMERGENCY_STOP, PRE_DEPARTURE, SYSTEM_FAULT,
    Step, TYPE_TRAM, Transport, move_transport,
};
use crate::atmos_machinery::machine_say;
use crate::click::{ClickResolved, ResolveClicks};
use crate::doors::with_door;
use crate::health::Health;
use crate::inventory::Inventory;
use crate::power::Unpowered;
use crate::sound::{PlaySound, playsound};
use crate::tools::now;
pub use crate::transport_ui::{
    DESTINATION, SEND, TramControlData, TramControlStatic, TramControlUi,
};

pub const BEACON: &str = "/obj/effect/landmark/transport/nav_beacon/tram";
pub const NAV: &str = "/obj/effect/landmark/transport/nav_beacon/tram/nav";
pub const PLATFORM: &str = "/obj/effect/landmark/transport/nav_beacon/tram/platform";
const SLOW_TRAM: &str = "/obj/structure/transport/linear/tram/slow";
const CABINET: &str = "/obj/machinery/transport/tram_controller";
const HOME_CONTROLLER: &str = "/obj/machinery/transport/tram_controller/tcomms";
const CONSOLE: &str = "/obj/machinery/computer/tram_controls";
const TRAM_DOOR: &str = "/obj/machinery/door/airlock/tram";
/// `TRAMSTATION_LINE_1`: the line of a console or button whose circuit names none.
pub const DEFAULT_LINE: &str = "tram_1";
/// `set_tram_speed(tram_max_speed)`: `internal_movement_delay` of 100% and the slow tram's 16.5%.
const DELAY_SECS: f64 = 0.05;
const SLOW_DELAY_SECS: f64 = 0.3;
/// `reset_position`'s crawl.
const RESET_DELAY_SECS: f64 = 0.15;
const ANNOUNCE_TILES: u32 = 39;
const ANNOUNCE_COOLDOWN: f64 = 4.0;
const THROW_RANGE: u32 = 30;
const THROW_SPEED: f32 = 7.0;
const CHIME: &str = "sound/machines/tram/door_chime.ogg";
const ARRIVAL: &str = "sound/machines/tram/other_line_processed.ogg";
const CRASH: &str = "sound/vehicles/car_crash.ogg";
const SYNTH_YES: &str = "sound/machines/synth/synth_yes.ogg";
const BUZZ_SIGH: &str = "sound/machines/buzz/buzz-sigh.ogg";
const BUZZ_TWO: &str = "sound/machines/buzz/buzz-two.ogg";
const RESET_SAY: &str = "Контроллер сброшен.";
const FAULT_SAY: &str = "Ошибка контроллера. Пожалуйста, свяжитесь с инженерным отделом.";
const CONTROLLER_ERROR: &str = "Ошибка контроллера трамвая. Пожалуйста, обратитесь к техническому специалисту или члену экипажа, имеющему доступ к телекоммуникационным системам трамвая, для сброса контроллера.";

/// A tram landmark (`/obj/effect/landmark/transport/nav_beacon/tram`).
#[derive(Component, Clone, Debug)]
pub struct TramBeacon {
    /// `specific_transport_id`: the line (platforms) or `tram_nav` (the trams' own beacons).
    pub line: String,
    /// `platform_code`; empty on nav beacons.
    pub code: String,
    pub name: String,
}

/// `SStransport.nav_beacons`: the beacons by line.
#[derive(Resource, Default, Debug)]
pub struct NavBeacons(pub HashMap<String, Vec<Entity>>);

/// `SStransport.doors`: the tram doors and their `transport_linked_id`.
#[derive(Resource, Default, Debug)]
pub struct TramDoors(pub Vec<(Entity, String)>);

/// What a tram does later (tg's timers).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TramEvent {
    CloseDoors,
    Validate(u32),
    Dispatch,
    Unlock,
    OpenDoors,
    Jingle,
}

/// The tram half of the controller, on the [`Transport`].
#[derive(Component, Clone, Debug)]
pub struct Tram {
    /// `controller_active`: busy, no requests taken.
    pub active: bool,
    /// `controller_operational`: its cabinet has power.
    pub operational: bool,
    /// `paired_cabinet`.
    pub cabinet: Option<Entity>,
    pub nav_beacon: Option<Entity>,
    pub idle_platform: Option<Entity>,
    pub destination: Option<Entity>,
    /// `travel_direction` as a step, `travel_remaining`, `travel_trip_length`.
    pub travel: (i32, i32),
    pub travel_remaining: u32,
    pub trip_length: u32,
    /// `internal_movement_delay`, and the speed it goes back to (`set_tram_speed`).
    pub delay_secs: f64,
    pub base_delay_secs: f64,
    /// Moving: the time of the next tile.
    pub next_move: Option<f64>,
    pub announce_ready_at: f64,
    pub timers: Vec<(f64, TramEvent)>,
}

/// Why a request failed (tg `REQUEST_FAIL` codes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    BrokenBeyondRepair,
    NotInService,
    InternalError,
    InUse,
    InvalidPlatform,
    NoCallRequired,
}

/// `/obj/machinery/computer/tram_controls`.
#[derive(Component, Clone, Debug, Default)]
pub struct TramConsole {
    pub line: String,
    pub tram: Option<Entity>,
    /// `get_destinations`: `(name, code)`.
    pub destinations: Vec<(String, String)>,
}

/// A call button's line (`specific_transport_id`); its platform is the button's `id`.
#[derive(Component, Clone, Debug)]
pub struct TramCallButton {
    pub line: String,
}

pub struct TramPlugin;

impl Plugin for TramPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NavBeacons>()
            .init_resource::<TramDoors>()
            .register_ui::<TramControlUi>()
            .add_systems(
                ss13_world::MapInitLate,
                link_trams.after(super::link_transports),
            )
            .add_systems(
                Update,
                (
                    console_clicks.after(ResolveClicks).before(UiSystems::Open),
                    console_acts.in_set(UiSystems::Handle),
                    tick_trams,
                ),
            );
    }
}

fn plain_name(world: &World, e: Entity) -> String {
    world
        .get::<DmName>(e)
        .map(|n| crate::vending::plain_name(&n.name).to_owned())
        .unwrap_or_default()
}

fn line_or_default(id: &str) -> String {
    if id.is_empty() {
        DEFAULT_LINE.to_owned()
    } else {
        id.to_owned()
    }
}

/// The beacons, consoles, buttons, cabinets and doors of the trams; each tram's nav beacon,
/// starting platform and cabinet.
pub fn link_trams(world: &mut World) {
    let mut q = world.query_filtered::<(Entity, &DmTransport), With<Mapped>>();
    let found: Vec<(Entity, DmTransport)> = q.iter(world).map(|(e, t)| (e, t.clone())).collect();
    let mut beacons: HashMap<String, Vec<Entity>> = HashMap::new();
    let mut doors = Vec::new();
    let mut consoles = Vec::new();
    for (e, dm) in &found {
        if is_under(&dm.path, BEACON) {
            let beacon = TramBeacon {
                line: dm.specific_transport_id.clone(),
                code: dm.platform_code.clone(),
                name: plain_name(world, *e),
            };
            beacons.entry(beacon.line.clone()).or_default().push(*e);
            world.entity_mut(*e).insert(beacon);
        } else if is_under(&dm.path, TRAM_DOOR) && !dm.transport_linked_id.is_empty() {
            doors.push((*e, dm.transport_linked_id.clone()));
        } else if is_under(&dm.path, CONSOLE) {
            consoles.push((*e, line_or_default(&dm.specific_transport_id)));
        } else if is_under(&dm.path, "/obj/machinery/button/transport") {
            world.entity_mut(*e).insert(TramCallButton {
                line: line_or_default(&dm.specific_transport_id),
            });
        }
    }
    world.insert_resource(NavBeacons(beacons));
    world.insert_resource(TramDoors(doors.clone()));

    let mut q = world.query::<(Entity, &Transport)>();
    let trams: Vec<(Entity, Transport)> = q
        .iter(world)
        .filter(|(_, t)| t.transport_id == TYPE_TRAM)
        .map(|(e, t)| (e, t.clone()))
        .collect();
    for (tram_e, transport) in trams {
        let mut tram = Tram {
            active: false,
            operational: true,
            cabinet: None,
            nav_beacon: None,
            idle_platform: None,
            destination: None,
            travel: (0, 0),
            travel_remaining: 0,
            trip_length: 0,
            delay_secs: DELAY_SECS,
            base_delay_secs: DELAY_SECS,
            next_move: None,
            announce_ready_at: 0.0,
            timers: Vec::new(),
        };
        if transport.modules.iter().any(|&m| {
            world
                .get::<DmObj>(m)
                .is_some_and(|o| is_under(&o.path, SLOW_TRAM))
        }) {
            tram.delay_secs = SLOW_DELAY_SECS;
            tram.base_delay_secs = SLOW_DELAY_SECS;
        }
        // `check_for_landmarks`, `find_controller` of the cabinet on the tram.
        for pos in super::tiles(world, &transport) {
            let here: Vec<Entity> = world.resource::<SpatialIndex>().on_tile(pos).to_vec();
            for e in here {
                let Some(path) = world.get::<DmTransport>(e).map(|t| t.path.clone()) else {
                    continue;
                };
                if is_under(&path, PLATFORM) {
                    tram.idle_platform = Some(e);
                    tram.destination = Some(e);
                } else if is_under(&path, NAV) {
                    tram.nav_beacon = Some(e);
                } else if is_under(&path, CABINET) && !is_under(&path, HOME_CONTROLLER) {
                    tram.cabinet = Some(e);
                }
            }
        }
        if tram.idle_platform.is_none() || tram.nav_beacon.is_none() {
            error!(
                "tram {:?} has no starting platform or nav beacon",
                transport.specific_id
            );
        }
        world.entity_mut(tram_e).insert(tram);
        // `post_machine_initialize`: the tram doors start open.
        cycle_doors(world, &transport.specific_id, true, false);
    }

    for (e, line) in consoles {
        let tram = super::find(world, TYPE_TRAM, &line);
        let destinations = destinations(world, &line);
        world.entity_mut(e).insert(TramConsole {
            line,
            tram,
            destinations,
        });
    }
}

/// `detailed_destination_list`: the platforms of `line`, `(name, code)`.
pub fn destinations(world: &World, line: &str) -> Vec<(String, String)> {
    world
        .resource::<NavBeacons>()
        .0
        .get(line)
        .into_iter()
        .flatten()
        .filter_map(|&b| world.get::<TramBeacon>(b))
        .filter(|b| !b.code.is_empty())
        .map(|b| (b.name.clone(), b.code.clone()))
        .collect()
}

fn platform_of(world: &World, line: &str, code: &str) -> Option<Entity> {
    world
        .resource::<NavBeacons>()
        .0
        .get(line)?
        .iter()
        .copied()
        .find(|&b| world.get::<TramBeacon>(b).is_some_and(|t| t.code == code))
}

fn set_status(world: &mut World, tram: Entity, code: u32, on: bool) {
    if let Some(mut t) = world.get_mut::<Transport>(tram) {
        if on {
            t.status |= code;
        } else {
            t.status &= !code;
        }
    }
}

fn status(world: &World, tram: Entity) -> u32 {
    world.get::<Transport>(tram).map_or(0, |t| t.status)
}

fn schedule(world: &mut World, tram: Entity, after: f64, event: TramEvent) {
    let at = now(world) + after;
    if let Some(mut t) = world.get_mut::<Tram>(tram) {
        t.timers.push((at, event));
    }
}

fn cabinet_say(world: &mut World, tram: Entity, sound: &str, text: &str) {
    let Some(cabinet) = world.get::<Tram>(tram).and_then(|t| t.cabinet) else {
        return;
    };
    if let Some(&pos) = world.get::<TilePos>(cabinet) {
        playsound(world, PlaySound::new(pos, sound, 40.0, false));
        machine_say(world, cabinet, text);
    }
}

/// `SStransport.incoming_request`: validates the request of `source` and sends the tram.
/// The destination platform's name on success.
pub fn request(world: &mut World, line: &str, code: &str) -> Result<String, Refusal> {
    let Some(tram_e) = super::find(world, TYPE_TRAM, line) else {
        return Err(Refusal::BrokenBeyondRepair);
    };
    let Some(tram) = world.get::<Tram>(tram_e).cloned() else {
        return Err(Refusal::BrokenBeyondRepair);
    };
    if !tram.operational || tram.cabinet.is_none() {
        return Err(Refusal::NotInService);
    }
    if status(world, tram_e) & (SYSTEM_FAULT | EMERGENCY_STOP) != 0 {
        return Err(Refusal::InternalError);
    }
    if tram.active {
        return Err(Refusal::InUse);
    }
    let Some(dest) = platform_of(world, line, code) else {
        return Err(Refusal::InvalidPlatform);
    };
    if tram.idle_platform == Some(dest) {
        return Err(Refusal::NoCallRequired);
    }
    if !calculate_route(world, tram_e, dest) {
        return Err(Refusal::InternalError);
    }
    let name = world
        .get::<TramBeacon>(dest)
        .map(|b| b.name.clone())
        .unwrap_or_default();
    info!("tram {line} dispatched to {name}");
    dispatch(world, tram_e);
    Ok(name)
}

/// `calculate_route`: the direction and distance from the nav beacon to `dest`.
fn calculate_route(world: &mut World, tram_e: Entity, dest: Entity) -> bool {
    let Some(tram) = world.get::<Tram>(tram_e) else {
        return false;
    };
    if tram.idle_platform == Some(dest) {
        return false;
    }
    let (Some(&from), Some(&to)) = (
        tram.nav_beacon.and_then(|n| world.get::<TilePos>(n)),
        world.get::<TilePos>(dest),
    ) else {
        return false;
    };
    let Some(dist) = from.chebyshev(to) else {
        return false;
    };
    let travel = ((to.x - from.x).signum(), (to.y - from.y).signum());
    let mut tram = world.get_mut::<Tram>(tram_e).expect("checked");
    tram.destination = Some(dest);
    tram.travel = travel;
    tram.travel_remaining = dist;
    tram.trip_length = dist;
    true
}

/// `dispatch_transport` + `pre_departure` of the subsystem.
fn dispatch(world: &mut World, tram_e: Entity) {
    if let Some(mut t) = world.get_mut::<Tram>(tram_e) {
        t.active = true;
    }
    set_status(world, tram_e, PRE_DEPARTURE | CONTROLS_LOCKED, true);
    if status(world, tram_e) & BYPASS_SENSORS != 0 {
        let id = specific_id(world, tram_e);
        cycle_doors(world, &id, false, true);
        schedule(world, tram_e, 3.0, TramEvent::Validate(0));
        return;
    }
    if let Some(pos) = nav_pos(world, tram_e) {
        playsound(world, PlaySound::new(pos, CHIME, 45.0, false));
    }
    schedule(world, tram_e, 1.4, TramEvent::CloseDoors);
    schedule(world, tram_e, 1.4 + 3.0, TramEvent::Validate(0));
}

fn specific_id(world: &World, tram_e: Entity) -> String {
    world
        .get::<Transport>(tram_e)
        .map(|t| t.specific_id.clone())
        .unwrap_or_default()
}

fn nav_pos(world: &World, tram_e: Entity) -> Option<TilePos> {
    world
        .get::<Tram>(tram_e)
        .and_then(|t| t.nav_beacon)
        .and_then(|n| world.get::<TilePos>(n))
        .copied()
}

/// `cycle_doors(CYCLE_OPEN / CYCLE_CLOSED)` of the tram doors of `line`.
pub fn cycle_doors(world: &mut World, line: &str, open: bool, forced: bool) {
    let doors: Vec<Entity> = world
        .resource::<TramDoors>()
        .0
        .iter()
        .filter(|(_, id)| id == line)
        .map(|(e, _)| *e)
        .collect();
    for door in doors {
        with_door(world, door, |d, appearance, ctx| {
            let moved = if open {
                ctx.open(d, appearance, forced)
            } else {
                ctx.close(d, appearance, forced)
            };
            ((), moved)
        });
    }
}

/// The tram doors of `line` that are not shut.
fn open_doors(world: &World, line: &str) -> Vec<Entity> {
    world
        .resource::<TramDoors>()
        .0
        .iter()
        .filter(|(e, id)| id == line && world.get::<Density>(*e).is_none())
        .map(|(e, _)| *e)
        .collect()
}

/// `validate_and_dispatch(attempt)`: leaves with the doors shut, or after four tries anyway.
fn validate(world: &mut World, tram_e: Entity, attempt: u32) {
    if attempt >= 4 {
        start_moving(world, tram_e);
        return;
    }
    let line = specific_id(world, tram_e);
    let stuck = open_doors(world, &line);
    set_status(world, tram_e, DOORS_READY, stuck.is_empty());
    if stuck.is_empty() {
        start_moving(world, tram_e);
        return;
    }
    // The doors' own retries (`verify_status`): from the third the door closes on whoever is
    // in the way.
    let forced = attempt + 1 >= 2;
    for door in stuck {
        if let Some(&pos) = world.get::<TilePos>(door) {
            if forced {
                playsound(world, PlaySound::new(pos, BUZZ_TWO, 60.0, false));
                machine_say(world, door, "ТЫ ЗАДЕРЖИВАЕШЬ ТРАМВАЙ, ПРИДУРОК!");
            } else {
                playsound(world, PlaySound::new(pos, BUZZ_SIGH, 60.0, false));
                machine_say(world, door, "Пожалуйста, отойдите от дверей!");
            }
        }
        with_door(world, door, |d, appearance, ctx| {
            let moved = ctx.close(d, appearance, forced);
            ((), moved)
        });
    }
    schedule(world, tram_e, 3.0, TramEvent::Validate(attempt + 1));
}

/// The controller's `dispatch_transport`: off it goes.
fn start_moving(world: &mut World, tram_e: Entity) {
    set_status(world, tram_e, PRE_DEPARTURE, false);
    if status(world, tram_e) & EMERGENCY_STOP != 0 {
        set_status(world, tram_e, EMERGENCY_STOP, false);
        cabinet_say(world, tram_e, SYNTH_YES, RESET_SAY);
    }
    let t = now(world);
    if let Some(mut tram) = world.get_mut::<Tram>(tram_e) {
        tram.next_move = Some(t + tram.delay_secs);
    }
    travel_loop(world, tram_e, true);
}

/// `nav_beacon.tram_loop.start()` / `stop()`: the travel sound rides the nav beacon.
fn travel_loop(world: &mut World, tram_e: Entity, on: bool) {
    if let Some(nav) = world.get::<Tram>(tram_e).and_then(|t| t.nav_beacon) {
        crate::looping_sound::set(world, nav, &crate::looping_sound::defs::TRAM, on);
    }
}

/// `estop`: the cabinet's emergency stop.
pub fn estop(world: &mut World, tram_e: Entity) {
    cabinet_say(
        world,
        tram_e,
        BUZZ_SIGH,
        "Активирована аварийная остановка!",
    );
    set_status(world, tram_e, EMERGENCY_STOP, true);
}

/// The trams' timers and travel (tg `process`).
pub fn tick_trams(world: &mut World) {
    let t = now(world);
    let mut q = world.query::<(Entity, &Tram)>();
    let trams: Vec<Entity> = q.iter(world).map(|(e, _)| e).collect();
    for tram_e in trams {
        update_power(world, tram_e);
        let due: Vec<TramEvent> = {
            let Some(mut tram) = world.get_mut::<Tram>(tram_e) else {
                continue;
            };
            let due = tram
                .timers
                .iter()
                .filter(|(at, _)| t >= *at)
                .map(|(_, e)| *e)
                .collect();
            tram.timers.retain(|(at, _)| t < *at);
            due
        };
        for event in due {
            run_event(world, tram_e, event);
        }
        travel(world, tram_e, t);
    }
}

/// The cabinet's power is the tram's (`power_lost`, `power_restored`); a lost cabinet faults it.
fn update_power(world: &mut World, tram_e: Entity) {
    let Some(tram) = world.get::<Tram>(tram_e) else {
        return;
    };
    let cabinet = tram.cabinet.filter(|&c| world.get_entity(c).is_ok());
    let was = tram.operational;
    let lost = tram.cabinet.is_some() && cabinet.is_none();
    let powered = cabinet.is_some_and(|c| world.get::<Unpowered>(c).is_none());
    if let Some(mut tram) = world.get_mut::<Tram>(tram_e) {
        tram.cabinet = cabinet;
        tram.operational = powered;
    }
    if lost {
        set_status(world, tram_e, SYSTEM_FAULT, true);
    }
    if powered && !was {
        let line = specific_id(world, tram_e);
        cycle_doors(world, &line, true, false);
    }
}

fn run_event(world: &mut World, tram_e: Entity, event: TramEvent) {
    let line = specific_id(world, tram_e);
    match event {
        TramEvent::CloseDoors => {
            set_status(world, tram_e, DOORS_READY, false);
            cycle_doors(world, &line, false, false);
        }
        TramEvent::Validate(attempt) => validate(world, tram_e, attempt),
        TramEvent::Dispatch => start_moving(world, tram_e),
        TramEvent::Unlock => {
            set_status(world, tram_e, CONTROLS_LOCKED, false);
            if let Some(mut t) = world.get_mut::<Tram>(tram_e) {
                t.active = false;
            }
        }
        TramEvent::OpenDoors => cycle_doors(world, &line, true, false),
        TramEvent::Jingle => {
            if let Some(&pos) = world
                .get::<Tram>(tram_e)
                .and_then(|t| t.idle_platform)
                .and_then(|p| world.get::<TilePos>(p))
            {
                playsound(world, PlaySound::new(pos, ARRIVAL, 60.0, false));
            }
        }
    }
}

/// `process(seconds_per_tick)` while moving.
fn travel(world: &mut World, tram_e: Entity, t: f64) {
    let Some(tram) = world.get::<Tram>(tram_e).cloned() else {
        return;
    };
    let Some(mut next) = tram.next_move else {
        return;
    };
    if tram.cabinet.is_none() {
        set_status(world, tram_e, SYSTEM_FAULT, true);
    }
    if status(world, tram_e) & (SYSTEM_FAULT | EMERGENCY_STOP) != 0 {
        halt_and_catch_fire(world, tram_e);
        return;
    }
    let mut remaining = tram.travel_remaining;
    let mut announce_ready_at = tram.announce_ready_at;
    // Frame independent: every tile whose time has come this tick.
    while remaining > 0 && t >= next {
        remaining -= 1;
        if !move_transport(world, tram_e, Step::Flat(tram.travel.0, tram.travel.1)) {
            remaining = 0;
            break;
        }
        if remaining < ANNOUNCE_TILES && t >= announce_ready_at {
            let name = tram
                .destination
                .and_then(|d| world.get::<TramBeacon>(d))
                .map(|b| b.name.clone())
                .unwrap_or_default();
            if let Some(nav) = tram.nav_beacon {
                if let Some(&pos) = world.get::<TilePos>(nav) {
                    playsound(
                        world,
                        PlaySound::new(pos, "sound/machines/tram/info_chime.ogg", 100.0, false),
                    );
                }
                machine_say(world, nav, &format!("The next station is: {name}."));
            }
            announce_ready_at = t + ANNOUNCE_COOLDOWN;
        }
        next += tram.delay_secs;
    }
    if let Some(mut live) = world.get_mut::<Tram>(tram_e) {
        live.travel_remaining = remaining;
        live.announce_ready_at = announce_ready_at;
        live.next_move = Some(next);
    }
    if remaining == 0 && t >= next {
        if tram.operational {
            normal_stop(world, tram_e);
        } else {
            degraded_stop(world, tram_e);
        }
    }
}

fn end_trip(world: &mut World, tram_e: Entity, platform: Option<Entity>) {
    if let Some(mut tram) = world.get_mut::<Tram>(tram_e) {
        tram.next_move = None;
        tram.idle_platform = platform;
        tram.travel_remaining = 0;
        tram.trip_length = 0;
        tram.delay_secs = tram.base_delay_secs;
    }
}

/// `normal_stop`.
fn normal_stop(world: &mut World, tram_e: Entity) {
    let line = specific_id(world, tram_e);
    cycle_doors(world, &line, true, false);
    travel_loop(world, tram_e, false);
    schedule(world, tram_e, 2.0, TramEvent::Unlock);
    schedule(world, tram_e, 2.5, TramEvent::Jingle);
    let dest = world.get::<Tram>(tram_e).and_then(|t| t.destination);
    let dest_pos = dest.and_then(|d| world.get::<TilePos>(d)).copied();
    if status(world, tram_e) & SYSTEM_FAULT != 0
        && dest_pos.is_some()
        && nav_pos(world, tram_e) == dest_pos
    {
        set_status(world, tram_e, SYSTEM_FAULT, false);
        cabinet_say(world, tram_e, SYNTH_YES, RESET_SAY);
    }
    end_trip(world, tram_e, dest);
}

/// `degraded_stop`: arriving without power.
fn degraded_stop(world: &mut World, tram_e: Entity) {
    crash_fx(world, tram_e);
    travel_loop(world, tram_e, false);
    schedule(world, tram_e, 4.0, TramEvent::Unlock);
    if status(world, tram_e) & SYSTEM_FAULT != 0 {
        set_status(world, tram_e, SYSTEM_FAULT, false);
        cabinet_say(world, tram_e, SYNTH_YES, RESET_SAY);
    }
    let dest = world.get::<Tram>(tram_e).and_then(|t| t.destination);
    end_trip(world, tram_e, dest);
    estop_throw(world, tram_e);
}

/// `halt_and_catch_fire`: a fault or an emergency stop mid-trip.
fn halt_and_catch_fire(world: &mut World, tram_e: Entity) {
    if status(world, tram_e) & SYSTEM_FAULT != 0 {
        cabinet_say(world, tram_e, BUZZ_SIGH, FAULT_SAY);
    }
    let remaining = world.get::<Tram>(tram_e).map_or(0, |t| t.travel_remaining);
    if remaining > 0 {
        crash_fx(world, tram_e);
        estop_throw(world, tram_e);
    }
    travel_loop(world, tram_e, false);
    schedule(world, tram_e, 4.0, TramEvent::Unlock);
    schedule(world, tram_e, 2.0, TramEvent::OpenDoors);
    end_trip(world, tram_e, None);
}

fn crash_fx(world: &mut World, tram_e: Entity) {
    if let Some(pos) = nav_pos(world, tram_e) {
        playsound(world, PlaySound::new(pos, CRASH, 100.0, false));
    }
}

/// `estop_throw`: every passenger flies on in the travel direction.
fn estop_throw(world: &mut World, tram_e: Entity) {
    let Some(transport) = world.get::<Transport>(tram_e).cloned() else {
        return;
    };
    let (dx, dy) = world.get::<Tram>(tram_e).map_or((0, 0), |t| t.travel);
    let passengers: Vec<(Entity, TilePos)> = super::tiles(world, &transport)
        .into_iter()
        .flat_map(|p| {
            world
                .resource::<SpatialIndex>()
                .on_tile(p)
                .iter()
                .map(move |&e| (e, p))
                .collect::<Vec<_>>()
        })
        .filter(|&(e, _)| {
            world.get::<Health>(e).is_some() && world.get::<crate::ghost::Ghost>(e).is_none()
        })
        .collect();
    for (e, p) in passengers {
        let to = TilePos::new(p.x + dx * 50, p.y + dy * 50, p.z);
        crate::throw::throw_at(world, e, to, THROW_RANGE, THROW_SPEED);
    }
}

/// `reset_position`: back in sync at its platform, or a crawl to the nearest platform ahead.
pub fn reset_position(world: &mut World, tram_e: Entity) {
    let Some(tram) = world.get::<Tram>(tram_e).cloned() else {
        return;
    };
    let nav = nav_pos(world, tram_e);
    if let Some(idle) = tram.idle_platform
        && world.get::<TilePos>(idle).copied() == nav
    {
        set_status(world, tram_e, SYSTEM_FAULT | EMERGENCY_STOP, false);
        cabinet_say(world, tram_e, SYNTH_YES, RESET_SAY);
        return;
    }
    let Some(nav) = nav else {
        return;
    };
    let line = specific_id(world, tram_e);
    let outbound = if tram.travel.1 != 0 {
        tram.travel.1 > 0
    } else {
        tram.travel.0 > 0
    };
    let facing = tram
        .nav_beacon
        .and_then(|n| world.get::<DmAtom>(n))
        .map_or(8, |a| a.dir);
    let Some(target) = closest_in_travel_dir(world, nav, facing, outbound, &line) else {
        cabinet_say(
            world,
            tram_e,
            BUZZ_SIGH,
            "Ошибка сброса контроллера. Пожалуйста, свяжитесь с производителем.",
        );
        error!("tram {line} cannot find a platform to reset at from {nav:?}");
        return;
    };
    let Some(&to) = world.get::<TilePos>(target) else {
        return;
    };
    if let Some(mut t) = world.get_mut::<Tram>(tram_e) {
        t.travel = ((to.x - nav.x).signum(), (to.y - nav.y).signum());
        t.travel_remaining = nav.chebyshev(to).unwrap_or(0);
        t.trip_length = t.travel_remaining;
        t.destination = Some(target);
        t.delay_secs = RESET_DELAY_SECS;
        t.active = true;
    }
    cabinet_say(
        world,
        tram_e,
        "sound/machines/ping.ogg",
        "Выполняется сброс контроллера... Переход к точке сброса.",
    );
    cycle_doors(world, &line, false, false);
    set_status(world, tram_e, CONTROLS_LOCKED, true);
    schedule(world, tram_e, 3.0, TramEvent::Dispatch);
}

/// `closest_nav_in_travel_dir`: the closest platform of `line` on the nav beacon's line of
/// travel (its `dir`'s axis), ahead (`outbound`: increasing x or y) or behind.
fn closest_in_travel_dir(
    world: &World,
    nav: TilePos,
    facing: u8,
    outbound: bool,
    line: &str,
) -> Option<Entity> {
    let horizontal = facing & (4 | 8) != 0;
    world
        .resource::<NavBeacons>()
        .0
        .get(line)?
        .iter()
        .filter_map(|&b| world.get::<TilePos>(b).map(|&p| (b, p)))
        .filter(|(_, p)| p.z == nav.z)
        .filter(|(_, p)| {
            if horizontal {
                p.y == nav.y && (p.x >= nav.x) == outbound
            } else {
                p.x == nav.x && (p.y >= nav.y) == outbound
            }
        })
        .min_by_key(|(_, p)| nav.chebyshev(*p).unwrap_or(u32::MAX))
        .map(|(b, _)| b)
}

/// tg `REQUEST_FAIL` answers of a call button (`call_response`).
pub fn button_answer(result: Result<String, Refusal>) -> &'static str {
    match result {
        Ok(_) => "Трамвай вызван на платформу.",
        Err(Refusal::BrokenBeyondRepair) => {
            "Трамвай потерпел катастрофическую аварию. Пожалуйста, воспользуйтесь альтернативными способами передвижения."
        }
        Err(Refusal::NotInService) => {
            "Трамвай не работает из-за отключения электроэнергии или системных проблем. Обратитесь к техническому специалисту для проверки питания и контроллера."
        }
        Err(Refusal::InvalidPlatform) => {
            "Ошибка конфигурации кнопки. Пожалуйста, свяжитесь с техническим специалистом."
        }
        Err(Refusal::InUse) => "Трамвай в пути, пожалуйста, подождите.",
        Err(Refusal::NoCallRequired) => {
            "Трамвай уже в пункте назначения. Пожалуйста, займите место и выберите пункт назначения."
        }
        Err(Refusal::InternalError) => CONTROLLER_ERROR,
    }
}

/// `/obj/item/assembly/control/transport/call_button/activate`: the tram to the button's
/// platform `code`, and the button speaks the answer.
pub fn call_button(world: &mut World, button: Entity, code: &str) {
    let line = world
        .get::<TramCallButton>(button)
        .map_or_else(|| DEFAULT_LINE.to_owned(), |b| b.line.clone());
    let answer = button_answer(request(world, &line, code));
    machine_say(world, button, answer);
}

fn console_clicks(
    mut clicks: MessageReader<ClickResolved>,
    consoles: Query<(&DmName, &TramConsole)>,
    trams: Query<&Tram>,
    inventories: Query<&Inventory>,
    mut open: MessageWriter<OpenUi>,
) {
    for click in clicks.read() {
        let Ok((name, console)) = consoles.get(click.target) else {
            continue;
        };
        // `ui_status`: closed while the tram is busy.
        if console
            .tram
            .and_then(|t| trams.get(t).ok())
            .is_some_and(|t| t.active)
        {
            continue;
        }
        if inventories
            .get(click.mob)
            .is_ok_and(|inv| inv.active_item().is_none())
        {
            open.write(OpenUi::new::<TramControlUi>(
                click.client,
                click.mob,
                click.target,
                crate::vending::plain_name(&name.name),
            ));
        }
    }
}

impl ServerUi for TramControlUi {
    type Source = &'static TramConsole;
    type UserParam = (
        Query<'static, 'static, &'static Tram>,
        Query<'static, 'static, &'static TramBeacon>,
    );
    type Act = String;

    fn static_data(console: &TramConsole) -> TramControlStatic {
        TramControlStatic {
            destinations: console.destinations.clone(),
        }
    }

    fn data(
        console: &TramConsole,
        _: Entity,
        (trams, beacons): &bevy::ecs::system::SystemParamItem<'_, '_, Self::UserParam>,
    ) -> TramControlData {
        let tram = console.tram.and_then(|t| trams.get(t).ok());
        TramControlData {
            moving: tram.is_some_and(|t| t.active),
            broken: tram.is_none_or(|t| t.cabinet.is_none()),
            tram_location: tram
                .and_then(|t| t.idle_platform)
                .and_then(|p| beacons.get(p).ok())
                .map(|b| b.name.clone()),
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<String, UiReject> {
        match action {
            SEND => params
                .get(DESTINATION)
                .filter(|d| !d.is_empty() && d.len() <= 32)
                .map(str::to_owned)
                .ok_or_else(|| UiReject::BadParams("destination: missing".into())),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

/// `/obj/machinery/computer/tram_controls/ui_act` "send", and `call_response` on failure.
fn console_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<TramControlUi>>>) {
    let pending: Vec<(Entity, String)> = acts
        .read(world.resource::<Messages<UiActed<TramControlUi>>>())
        .map(|a| (a.source, a.act.clone()))
        .collect();
    for (console_e, code) in pending {
        let Some(console) = world.get::<TramConsole>(console_e).cloned() else {
            continue;
        };
        if !console.destinations.iter().any(|(_, c)| *c == code) {
            continue;
        }
        let refusal = match request(world, &console.line, &code) {
            Ok(_) => continue,
            Err(r) => r,
        };
        let text = match refusal {
            Refusal::NotInService => {
                "Трамвай в данный момент недоступен. Пожалуйста, свяжитесь с техническим специалистом."
            }
            Refusal::InvalidPlatform => {
                "Ошибка конфигурации. Пожалуйста, свяжитесь с техническим специалистом."
            }
            Refusal::InternalError => CONTROLLER_ERROR,
            _ => continue,
        };
        if let Some(nav) = console
            .tram
            .and_then(|t| world.get::<Tram>(t))
            .and_then(|t| t.nav_beacon)
        {
            machine_say(world, nav, text);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn button_answers_follow_the_response_codes() {
        assert_eq!(
            button_answer(Ok("Medical".into())),
            "Трамвай вызван на платформу."
        );
        assert_eq!(
            button_answer(Err(Refusal::InUse)),
            "Трамвай в пути, пожалуйста, подождите."
        );
        assert_eq!(button_answer(Err(Refusal::InternalError)), CONTROLLER_ERROR);
    }
}
