//! The arrivals shuttle (server), after tg `/obj/docking_port/mobile/arrivals`
//! (`code/modules/shuttle/mobile_port/variants/arrivals.dm`) and `SSjob.latejoin_trackers`.
//!
//! - Round start: the station map's arrivals dock (`arrival_stationary`, its
//!   `roundstart_template`: MetaStation `arrival/box`) loads the template and the shuttle docks
//!   there. Its chairs are the latejoin spawn points ([`latejoin_tiles`]), in place of the
//!   latejoin landmarks.
//! - With nobody alive aboard (and no nuclear disk) it leaves the station: 5 s of ignition, then
//!   it waits in transit with no timer. A latejoiner spawns on its chairs; anyone alive aboard in
//!   transit brings it to the station within 5.5 s (`arrivals_shuttle_dock_window`), where it
//!   says its welcome. A launch is cancelled while someone or the disk is aboard, unless a
//!   latejoin forced it (`force_depart`).
//! - Hull breach: when a chair's tile is outside the hazard pressures it stays (or comes back)
//!   docked for repairs, and leaves again once the pressure is back.
//! - Hyperspace sounds (`crate::shuttle::hyperspace_sound`): the warm-up when it ignites, the
//!   launch when it heads for the station with someone aboard, the arrival before it docks.
//! - Arrival announcements ([`queue_announce`], tg `QueueAnnounce` -> `announce_arrival`): the
//!   ghosts hear where a latejoiner arrived; the automated announcement system (the first powered
//!   `/obj/machinery/announcement_system`) announces them on the common channel, unless their job
//!   lacks `JOB_ANNOUNCE_ARRIVAL`. While the shuttle is away from the station the announcements
//!   wait for its (undamaged) docking.
// shortcut: a latejoin while it is docked forces the departure and spawns the player aboard at
// once (tg `RequireUndocked` holds the player until the shuttle is in transit), unbuckled; no AAS
// breach announcement, no admin `perma_docked`. The AAS here is only its arrival entry: no
// window, templates, emag or EMP (`docs/parity/comms/station-info.md`); the rank is the job
// title untranslated (tg `job_title_ru` only in the ghosts' line, here the title too).

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{Areas, DmObj};
use ss13_shared::TilePos;

use crate::atmos_machinery::machine_say;
use crate::breath::{HAZARD_HIGH_PRESSURE, HAZARD_LOW_PRESSURE};
use crate::health::Stat;
use crate::server::PlayerMob;
use crate::shuttle::{
    HYPERSPACE_END_SECS, Hyperspace, MobileDock, RoundstartPorts, ShuttleMode, ShuttleTimer,
    dock_at_map_pos, hyperspace_sound, initiate_docking, push_template, setup_template_shuttle,
};

/// The arrivals port's `ignitionTime`.
pub const IGNITION_SECS: f64 = 5.0;
/// `arrivals_shuttle_dock_window` (55 deciseconds).
pub const DOCK_WINDOW_SECS: f64 = 5.5;
/// tg `setTimer(20)` after a failed docking.
const RETRY_SECS: f64 = 2.0;
/// `SSshuttle` `wait`.
const CHECK_SECS: f64 = 1.0;
const ARRIVALS_PORT: &str = "/obj/docking_port/mobile/arrivals";
const CHAIR: &str = "/obj/structure/chair";
const REQUESTS_CONSOLE: &str = "/obj/machinery/requests_console";
const PERSON_ABOARD: &str = "обнаружена форма жизни на борту";
const DISK_ABOARD: &str = "на борту обнаружено критически важное устройство станции";
const ANNOUNCEMENT_SYSTEM: &str = "/obj/machinery/announcement_system";
/// `/obj/machinery/announcement_system` `name`.
const AAS_NAME: &str = "Автоматическая система оповещений";

/// What the shuttle's requests console says (`console.say`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Say {
    Repaired,
    Breach,
    Departing,
    PickingUp,
    Arriving,
    EmergencyDock,
    Welcome,
    Cancelled(&'static str),
}

impl Say {
    pub fn text(self, station: &str) -> String {
        match self {
            Say::Repaired => "Ремонт завершен. Скоро запуск шаттла.".into(),
            Say::Breach => "Тревога! Обнаружена пробоина в корпусе!".into(),
            Say::Departing => "Шаттл отправляется.".into(),
            Say::PickingUp => "Немедленная отстыковка шаттла для нового сотрудника.".into(),
            Say::Arriving => format!("Прибываем к: {station}."),
            Say::EmergencyDock => "Аварийная стыковка для ремонта!".into(),
            Say::Welcome => "Добро пожаловать в вашу новую жизнь, сотрудники!".into(),
            Say::Cancelled(reason) => format!("Запуск отменён, {reason}."),
        }
    }
}

/// What [`ArrivalsState::check`] asks the world to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArrivalsStep {
    /// Ignition is over: into transit, then [`ArrivalsState::entered_transit`].
    EnterTransit,
    /// The way back ran out: dock at the station, then [`ArrivalsState::docked`].
    DockStation,
}

/// The arrivals shuttle's modes without the world: `Idle` docked at the station, `Igniting`,
/// `Call` in transit (no timer: waiting for someone, tg `callTime = INFINITY`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ArrivalsState {
    pub mode: ShuttleMode,
    pub timer: ShuttleTimer,
    pub force_depart: bool,
    pub damaged: bool,
}

impl ArrivalsState {
    /// `Launch(pickingup)`: from the station, ignition starts.
    pub fn launch(&mut self, now: f64, picking_up: bool, said: &mut Vec<Say>) {
        if picking_up {
            self.force_depart = true;
        }
        if self.mode == ShuttleMode::Idle {
            said.push(if picking_up {
                Say::PickingUp
            } else {
                Say::Departing
            });
            self.mode = ShuttleMode::Igniting;
            self.timer.set(now, IGNITION_SECS);
        }
    }

    /// `SendToStation`: in transit, it docks within the dock window.
    pub fn send_to_station(&mut self, now: f64, said: &mut Vec<Say>) {
        if self.mode == ShuttleMode::Call
            && self
                .timer
                .left(now)
                .is_none_or(|left| left > DOCK_WINDOW_SECS)
        {
            said.push(if self.damaged {
                Say::EmergencyDock
            } else {
                Say::Arriving
            });
            self.timer.set(now, DOCK_WINDOW_SECS);
        }
    }

    /// `RequireUndocked` for a latejoiner: a docked shuttle leaves for them.
    pub fn require_undocked(&mut self, now: f64, said: &mut Vec<Say>) {
        if self.mode == ShuttleMode::Call || self.damaged {
            return;
        }
        self.launch(now, true, said);
    }

    /// The mobile port's `check` and the arrivals `check`, once per second. `awake`: someone
    /// alive or the disk aboard; `breach`: a chair's pressure is out of the safe range.
    pub fn check(
        &mut self,
        now: f64,
        awake: bool,
        breach: bool,
        said: &mut Vec<Say>,
    ) -> Option<ArrivalsStep> {
        if self.timer.left(now).is_some_and(|left| left <= 0.0) {
            match self.mode {
                ShuttleMode::Call => return Some(ArrivalsStep::DockStation),
                ShuttleMode::Igniting => {
                    self.mode = ShuttleMode::Call;
                    self.timer.clear();
                    return Some(ArrivalsStep::EnterTransit);
                }
                _ => {
                    self.mode = ShuttleMode::Idle;
                    self.timer.clear();
                }
            }
        }
        if self.damaged {
            if !breach {
                self.damaged = false;
                said.push(Say::Repaired);
            }
            return None;
        }
        if breach {
            self.damaged = true;
            said.push(Say::Breach);
            if self.mode == ShuttleMode::Call {
                self.send_to_station(now, said);
            } else {
                self.mode = ShuttleMode::Idle;
                self.timer.clear();
            }
            return None;
        }
        match self.mode {
            ShuttleMode::Call => {
                if awake {
                    self.send_to_station(now, said);
                }
            }
            ShuttleMode::Igniting => {
                if awake && !self.force_depart {
                    self.mode = ShuttleMode::Idle;
                    self.timer.clear();
                }
            }
            _ => {
                if !awake {
                    self.launch(now, false, said);
                }
            }
        }
        None
    }

    /// The arrivals `initiate_docking` to transit: cancelled by `aboard` (a reason) unless
    /// forced. True when the shuttle should move.
    pub fn entered_transit(&mut self, aboard: Option<&'static str>, said: &mut Vec<Say>) -> bool {
        if !self.force_depart
            && let Some(reason) = aboard
        {
            self.mode = ShuttleMode::Idle;
            said.push(Say::Cancelled(reason));
            return false;
        }
        self.force_depart = false;
        true
    }

    /// After [`ArrivalsStep::DockStation`].
    pub fn docked(&mut self, now: f64, success: bool, said: &mut Vec<Say>) {
        if !success {
            self.timer.set(now, RETRY_SECS);
            return;
        }
        if !self.damaged {
            said.push(Say::Welcome);
        }
        self.mode = ShuttleMode::Idle;
        self.timer.clear();
    }
}

/// The round's arrivals shuttle (tg `SSshuttle.arrivals`).
#[derive(Resource, Debug)]
pub struct ArrivalsShuttle {
    pub state: ArrivalsState,
    pub port: Entity,
    pub station: Entity,
    pub transit: Entity,
    /// tg `latejoin_trackers`: its chairs.
    pub chairs: Vec<Entity>,
    /// Its requests console, which talks.
    pub console: Option<Entity>,
    /// tg `sound_played`.
    pub sound_played: bool,
    /// tg `queued_announces`: `(mob, rank)` announced when it docks at the station.
    pub queued: Vec<(Entity, String)>,
}

#[derive(Resource)]
struct ArrivalsTemplate(crate::shuttle::RoundstartPort, std::path::PathBuf);

pub struct ArrivalsPlugin;

impl Plugin for ArrivalsPlugin {
    fn build(&self, app: &mut App) {
        let found = app
            .world()
            .get_resource::<RoundstartPorts>()
            .and_then(|r| r.of("arrival").into_iter().next())
            .filter(|(_, path)| ss13_data::exists(path));
        if let Some((port, path)) = found {
            info!("arrivals shuttle template {}", path.display());
            push_template(app, path.clone());
            app.insert_resource(ArrivalsTemplate(port, path))
                .add_systems(Startup, setup_arrivals.after(crate::shuttle::setup_shuttle))
                .add_systems(Update, tick);
        }
    }
}

fn setup_arrivals(world: &mut World) {
    let Some(ArrivalsTemplate(port, path)) = world.remove_resource::<ArrivalsTemplate>() else {
        return;
    };
    let Some(station) = dock_at_map_pos(world, port.x, port.y, port.z) else {
        warn!(
            "arrivals dock at {},{},{} is not a dock",
            port.x, port.y, port.z
        );
        return;
    };
    let Some(shuttle) = setup_template_shuttle(
        world,
        &path,
        ARRIVALS_PORT,
        "arrival",
        "arrival_away",
        "CentCom arrivals dock",
    ) else {
        return;
    };
    let moved = initiate_docking(world, shuttle.port, station);
    info!("arrivals shuttle at the station: {moved:?}");
    if moved.is_err() {
        return;
    }
    let areas = world
        .get::<MobileDock>(shuttle.port)
        .map(|m| m.areas.clone())
        .unwrap_or_default();
    let mut objs = world.query::<(Entity, &DmObj, &TilePos)>();
    let aboard: Vec<(Entity, String)> = objs
        .iter(world)
        .filter(|(_, _, p)| {
            world
                .resource::<Areas>()
                .area_at(**p)
                .is_some_and(|a| areas.contains(&a))
        })
        .map(|(e, o, _)| (e, o.path.clone()))
        .collect();
    let chairs: Vec<Entity> = aboard
        .iter()
        .filter(|(_, p)| is_under(p, CHAIR))
        .map(|(e, _)| *e)
        .collect();
    let console = aboard
        .iter()
        .find(|(_, p)| is_under(p, REQUESTS_CONSOLE))
        .map(|(e, _)| *e);
    info!("arrivals shuttle: {} latejoin chairs", chairs.len());
    world.insert_resource(ArrivalsShuttle {
        state: ArrivalsState::default(),
        port: shuttle.port,
        station,
        transit: shuttle.transit,
        chairs,
        console,
        sound_played: false,
        queued: Vec::new(),
    });
}

/// The shuttle's areas hold `pos`.
fn aboard(world: &World, areas: &[Entity], pos: TilePos) -> bool {
    world
        .resource::<Areas>()
        .area_at(pos)
        .is_some_and(|a| areas.contains(&a))
}

/// `PersonCheck` then `NukeDiskCheck`: why it may not leave, if anything.
fn aboard_reason(world: &mut World, areas: &[Entity]) -> Option<&'static str> {
    let mut players = world.query_filtered::<(Entity, &TilePos, Option<&Stat>), With<PlayerMob>>();
    let person = players.iter(world).any(|(e, &pos, stat)| {
        stat != Some(&Stat::Dead)
            && world.get::<crate::ghost::Ghost>(e).is_none()
            && aboard(world, areas, pos)
    });
    if person {
        return Some(PERSON_ABOARD);
    }
    let mut disks = world.query_filtered::<Entity, With<crate::antag::nukeop::server::NukeDisk>>();
    let disks: Vec<Entity> = disks.iter(world).collect();
    disks
        .into_iter()
        .any(|d| crate::modpc::pda::turf_of(world, d).is_some_and(|p| aboard(world, areas, p)))
        .then_some(DISK_ABOARD)
}

/// `CheckTurfsPressure` over the chairs.
fn breach(world: &World, chairs: &[Entity]) -> bool {
    let Some(atmos) = world.get_resource::<ss13_atmos::Atmos>() else {
        return false;
    };
    chairs.iter().any(|&c| {
        world
            .get::<TilePos>(c)
            .and_then(|&p| atmos.pressure(p))
            .is_some_and(|kpa| !(HAZARD_LOW_PRESSURE..=HAZARD_HIGH_PRESSURE).contains(&kpa))
    })
}

fn say_all(world: &mut World, said: Vec<Say>) {
    let Some(console) = world
        .get_resource::<ArrivalsShuttle>()
        .and_then(|a| a.console)
    else {
        return;
    };
    if said.is_empty() {
        return;
    }
    let station = crate::events::station_name(world);
    for line in said {
        machine_say(world, console, &line.text(&station));
    }
}

fn tick(world: &mut World, mut next: Local<f64>) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    if now < *next {
        return;
    }
    *next = now + CHECK_SECS;
    let Some(shuttle) = world.get_resource::<ArrivalsShuttle>() else {
        return;
    };
    let (port, station, transit, chairs) = (
        shuttle.port,
        shuttle.station,
        shuttle.transit,
        shuttle.chairs.clone(),
    );
    let areas = world
        .get::<MobileDock>(port)
        .map(|m| m.areas.clone())
        .unwrap_or_default();
    let reason = aboard_reason(world, &areas);
    let breached = breach(world, &chairs);
    let mut said = Vec::new();
    let mut state = world.resource::<ArrivalsShuttle>().state.clone();
    let mut sound_played = world.resource::<ArrivalsShuttle>().sound_played;
    // `check_effects`, before the docking of this same check.
    if state.mode == ShuttleMode::Call
        && !sound_played
        && state
            .timer
            .left(now)
            .is_some_and(|left| left <= HYPERSPACE_END_SECS)
    {
        sound_played = true;
        hyperspace_sound(world, port, Hyperspace::End);
    }
    let mut welcomed = false;
    match state.check(now, reason.is_some(), breached, &mut said) {
        Some(ArrivalsStep::EnterTransit) => {
            sound_played = false;
            if state.entered_transit(reason, &mut said) {
                let moved = initiate_docking(world, port, transit);
                debug!("arrivals shuttle into transit: {moved:?}");
            }
        }
        Some(ArrivalsStep::DockStation) => {
            sound_played = false;
            let moved = initiate_docking(world, port, station);
            debug!("arrivals shuttle at the station: {moved:?}");
            state.docked(now, moved.is_ok(), &mut said);
            welcomed = said.contains(&Say::Welcome);
        }
        None => {}
    }
    // `SendToStation`: "for the new guy".
    if said
        .iter()
        .any(|s| matches!(s, Say::Arriving | Say::EmergencyDock))
    {
        hyperspace_sound(world, port, Hyperspace::Launch);
    }
    match state.mode {
        ShuttleMode::Igniting if !sound_played => {
            sound_played = true;
            hyperspace_sound(world, port, Hyperspace::Warmup);
        }
        ShuttleMode::Idle => sound_played = false,
        _ => {}
    }
    let queued = {
        let mut shuttle = world.resource_mut::<ArrivalsShuttle>();
        shuttle.state = state;
        shuttle.sound_played = sound_played;
        if welcomed {
            std::mem::take(&mut shuttle.queued)
        } else {
            Vec::new()
        }
    };
    say_all(world, said);
    for (mob, rank) in queued {
        announce_arrival(world, mob, &rank, true);
    }
}

/// tg `SSshuttle.arrivals.QueueAnnounce(mob, rank)`: announced now while the shuttle is docked
/// at the station (or without one), else when it docks there.
pub fn queue_announce(world: &mut World, mob: Entity, rank: &str) {
    let away = world
        .get_resource::<ArrivalsShuttle>()
        .is_some_and(|shuttle| shuttle.state.mode != ShuttleMode::Idle);
    if away {
        world
            .resource_mut::<ArrivalsShuttle>()
            .queued
            .push((mob, rank.to_owned()));
    } else {
        announce_arrival(world, mob, rank, true);
    }
}

/// tg `announce_arrival(character, rank, announce_to_ghosts)`.
pub fn announce_arrival(world: &mut World, mob: Entity, rank: &str, announce_to_ghosts: bool) {
    if world.resource::<crate::round::Ticker>().state() != crate::lobby::GameState::Playing
        || world.get_entity(mob).is_err()
    {
        return;
    }
    let name = world
        .get::<Name>(mob)
        .map(|n| n.to_string())
        .unwrap_or_default();
    if announce_to_ghosts {
        let area = world
            .get::<TilePos>(mob)
            .and_then(|&p| world.resource::<Areas>().area_at(p))
            .map(|a| crate::shuttle::area_name(world, a))
            .unwrap_or_default();
        let mut ghosts = world.query_filtered::<&PlayerMob, With<crate::ghost::Ghost>>();
        let clients: Vec<Entity> = ghosts.iter(world).map(|p| p.client).collect();
        if !clients.is_empty() {
            world.write_message(ss13_net::chat::server::SendChat {
                clients,
                message: ss13_shared::chat::ChatMessage::simple(
                    ss13_shared::chat::ChatChannel::Info,
                    ss13_shared::chat::Span::Info,
                    format!("{name} ({rank}) прибыл на станцию у {area}."),
                ),
            });
        }
    }
    let quiet = world
        .get::<crate::jobs::PlayerJob>(mob)
        .and_then(|j| {
            world
                .resource::<crate::jobs::Jobs>()
                .data
                .jobs
                .get(j.0)
                .map(|d| d.quiet_arrival)
        })
        .unwrap_or(false);
    if !quiet {
        aas_announce(
            world,
            &format!("{name} прибывает на станцию в должности {rank}"),
        );
    }
}

/// `aas_config_announce` without channels: a powered `/obj/machinery/announcement_system` of
/// the base type (`has_supported_channels`) says `text` on the common channel.
fn aas_announce(world: &mut World, text: &str) {
    let mut objs = world.query::<(Entity, &DmObj, &TilePos)>();
    let found = objs
        .iter(world)
        .filter(|(e, o, _)| {
            o.path == ANNOUNCEMENT_SYSTEM && world.get::<crate::power::Unpowered>(*e).is_none()
        })
        .map(|(_, _, p)| p.z)
        .next();
    let Some(z) = found else {
        return;
    };
    world.write_message(crate::radio::MachineRadio {
        freq: ss13_chat::radio::FREQ_COMMON,
        z,
        name: AAS_NAME.to_owned(),
        text: text.to_owned(),
    });
}

/// Where latejoiners spawn: the arrivals shuttle's chairs (tg `latejoin_trackers`), when it has
/// any.
pub fn latejoin_tiles(world: &World) -> Option<Vec<TilePos>> {
    let shuttle = world.get_resource::<ArrivalsShuttle>()?;
    let tiles: Vec<TilePos> = shuttle
        .chairs
        .iter()
        .filter_map(|&c| world.get::<TilePos>(c).copied())
        .collect();
    (!tiles.is_empty()).then_some(tiles)
}

/// `SSshuttle.arrivals.RequireUndocked` before a latejoin spawn.
pub fn require_undocked(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut said = Vec::new();
    let Some(mut shuttle) = world.get_resource_mut::<ArrivalsShuttle>() else {
        return;
    };
    shuttle.state.require_undocked(now, &mut said);
    say_all(world, said);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaves_empty_waits_in_transit_and_brings_a_latejoiner() {
        let mut s = ArrivalsState::default();
        let mut said = Vec::new();
        // Docked and empty: it departs.
        assert_eq!(s.check(0.0, false, false, &mut said), None);
        assert_eq!(s.mode, ShuttleMode::Igniting);
        assert_eq!(said, vec![Say::Departing]);
        assert_eq!(s.check(4.0, false, false, &mut said), None);
        assert_eq!(
            s.check(5.0, false, false, &mut said),
            Some(ArrivalsStep::EnterTransit)
        );
        assert!(s.entered_transit(None, &mut said));
        assert_eq!(s.mode, ShuttleMode::Call);
        // Waits in transit with nobody aboard.
        assert_eq!(s.check(500.0, false, false, &mut said), None);
        assert_eq!(s.timer.left(500.0), None);
        // A latejoiner aboard: back within the dock window.
        said.clear();
        s.require_undocked(501.0, &mut said);
        assert!(said.is_empty(), "already in transit");
        assert_eq!(s.check(501.0, true, false, &mut said), None);
        assert_eq!(said, vec![Say::Arriving]);
        assert_eq!(s.timer.left(501.0), Some(DOCK_WINDOW_SECS));
        assert_eq!(
            s.check(506.5, true, false, &mut said),
            Some(ArrivalsStep::DockStation)
        );
        s.docked(506.5, true, &mut said);
        assert_eq!(s.mode, ShuttleMode::Idle);
        assert_eq!(said.last(), Some(&Say::Welcome));
        // Someone aboard keeps it docked.
        said.clear();
        assert_eq!(s.check(510.0, true, false, &mut said), None);
        assert_eq!(s.mode, ShuttleMode::Idle);
        assert!(said.is_empty());
    }

    #[test]
    fn launch_cancels_with_someone_aboard_unless_forced() {
        let mut s = ArrivalsState::default();
        let mut said = Vec::new();
        s.check(0.0, false, false, &mut said);
        // Someone boards during ignition: it stays.
        s.check(1.0, true, false, &mut said);
        assert_eq!(s.mode, ShuttleMode::Idle);
        // A latejoin forces the departure with them aboard.
        said.clear();
        s.require_undocked(2.0, &mut said);
        assert_eq!(said, vec![Say::PickingUp]);
        assert_eq!(s.check(3.0, true, false, &mut said), None);
        assert_eq!(s.mode, ShuttleMode::Igniting, "forced: not cancelled");
        assert_eq!(
            s.check(7.0, true, false, &mut said),
            Some(ArrivalsStep::EnterTransit)
        );
        assert!(s.entered_transit(Some(PERSON_ABOARD), &mut said));
        assert!(!s.force_depart);
        // Unforced with the disk aboard: cancelled at the last moment.
        let mut t = ArrivalsState {
            mode: ShuttleMode::Igniting,
            ..default()
        };
        t.timer.set(0.0, IGNITION_SECS);
        assert_eq!(
            t.check(5.0, false, false, &mut said),
            Some(ArrivalsStep::EnterTransit)
        );
        said.clear();
        assert!(!t.entered_transit(Some(DISK_ABOARD), &mut said));
        assert_eq!(t.mode, ShuttleMode::Idle);
        assert_eq!(said, vec![Say::Cancelled(DISK_ABOARD)]);
    }

    #[test]
    fn a_breach_brings_it_home_and_holds_it() {
        let mut s = ArrivalsState {
            mode: ShuttleMode::Call,
            ..default()
        };
        let mut said = Vec::new();
        assert_eq!(s.check(0.0, false, true, &mut said), None);
        assert!(s.damaged);
        assert_eq!(said, vec![Say::Breach, Say::EmergencyDock]);
        assert_eq!(
            s.check(5.5, false, true, &mut said),
            Some(ArrivalsStep::DockStation)
        );
        said.clear();
        s.docked(5.5, true, &mut said);
        assert!(said.is_empty(), "no welcome while damaged");
        assert_eq!(s.check(10.0, false, true, &mut said), None);
        assert_eq!(s.mode, ShuttleMode::Idle, "held while breached");
        assert_eq!(s.check(11.0, false, false, &mut said), None);
        assert_eq!(said, vec![Say::Repaired]);
        assert!(!s.damaged);
        said.clear();
        s.check(12.0, false, false, &mut said);
        assert_eq!(said, vec![Say::Departing]);
    }
}
