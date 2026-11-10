//! The emergency shuttle (server), after tg `SSshuttle` (`code/controllers/subsystem/shuttle.dm`),
//! `/obj/docking_port` (`code/modules/shuttle/`), `/obj/docking_port/mobile/emergency`
//! (`variants/emergency/emergency.dm`) and the communications console
//! (`code/game/machinery/computer/communications.dm`).
//!
//! - Templates: the map json's `"emergency"` shuttle (`_maps/map_jsons/*.json`, MetaStation:
//!   `emergency_meta`) loads with the station onto its own z-level ([`ShuttleTemplates`]); its
//!   turfs get tg's `/turf/baseturf_skipover/shuttle` under their own layers (`dispatch`), its
//!   `/obj/docking_port/mobile/emergency` gets width/height/dwidth/dheight from the template
//!   (`calculate_docking_port_information`) and its `/area/shuttle` areas. Stationary ports on
//!   the map register with their `shuttle_id` and size ([`StationaryDock`]).
//! - Moving ([`initiate_docking`]): every shuttle turf (one with the skipover) and what stands on
//!   it moves from one dock's ordered turfs to the other's (`return_ordered_turfs`): the turf
//!   entity moves and covers the destination's turf (whose type joins its baseturfs), the tile it
//!   left is scraped down to what was under the skipover, areas swap
//!   (`underlying_areas_by_turf`), the air moves with the tile, lighting is recomputed around both
//!   rectangles. Entities keep their identity, so clients see a block of entity moves.
//! - Modes ([`Emergency`]): idle at CentCom -> called (`request`) -> after the call time docked at
//!   `emergency_home` -> igniting 10 s before the dock time ends -> escape (transit) -> endgame at
//!   `emergency_away`, which ends the round (`SSticker.check_finished`). The call time is the
//!   base time times the security level's `shuttle_call_time_mod`, rescaled when the level changes
//!   during a call or a recall (`alert_coeff_change`). A call is recalled by the console while
//!   enough of the call is left for the level (`past_restriction_point`).
//! - The communications console calls and recalls it (`crate::comms_server`).
//! - Announcements: tg's priority announcements with BandaStation's Russian texts and the
//!   announcer's shuttle sounds (`crate::announce`).
// shortcut: there is no CentCom level: the template's own level stands in for it, with
// `emergency_away` placed where the template put the shuttle, and the transit dock is a fixed
// space level made at round start (tg reserves one per flight with hyperspace turfs). Both have
// the shuttle's own direction, so the emergency shuttle never rotates there (tg turns it at
// CentCom and in transit); a dock of another direction turns `Facing` only.
//! - Hyperspace sounds ([`hyperspace_sound`]): the engines rev up 5 s before the launch, the
//!   launch, and the arrival half a second before the shuttle docks at CentCom.
// shortcut: no hostile environments, ripples, shuttle events or airlock cycling on docking. A hijacked shuttle (`crate::shuttle_console_server`) docks at the CentCom
// stand-in too (no Syndicate base level, `emergency_syndicate`); `syndicate_landing` records it.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Duration;

use bevy::platform::time::Instant;
use bevy::prelude::*;
use ss13_assets::dmm::{DmValue, DmmMap, DmmObject};
use ss13_atmos::{Atmos, GasMixture, GasOverlay, HotspotEffect, TurfAir};
use ss13_mapload::dock::{
    SHUTTLE_SKIPOVER, STATIONARY_PORT_ROOT, map_shuttle, shuttle_template_path,
};
use ss13_mapload::spawner::Rng;
use ss13_mapload::{
    Areas, DEFAULT_AREA, DEFAULT_TURF, DmAtom, DmDockingPort, DmName, DmObj, DmPower,
    MapLoadPlugin, MapLoadReport, MapSeed, ShuttleTemplates, Turf,
};
use ss13_shared::{Dir, Facing, TilePos};
use ss13_world::SpatialIndex;

use crate::announce::{
    ANNOUNCER_SHUTTLECALLED, ANNOUNCER_SHUTTLEDOCK, ANNOUNCER_SHUTTLERECALLED, Priority,
};
use crate::construction::{Baseturfs, baseturfs_of, change_turf, path_of, turf_at};
use crate::health::{DamageType, Health, Stat, apply_damage};
use crate::inventory::Inventory;
use crate::jobs::{PlayerJob, display_time_text};
use crate::lobby::GameState;
use crate::obj_damage::{Anchored, is_under};
use crate::round::Ticker;
use crate::security_level::{ALERT_COEFF_GREEN, SecLevel};

/// tg `ENGINE_START_TIME`: the docked shuttle starts its engines this long before leaving.
pub const ENGINE_START_SECS: f64 = 10.0;
/// tg `ALERT_COEFF_AUTOEVAC_CRITICAL`.
pub const ALERT_COEFF_AUTOEVAC_CRITICAL: f64 = 0.4;
/// tg `CALL_SHUTTLE_REASON_LENGTH`.
pub const CALL_SHUTTLE_REASON_LENGTH: usize = 12;
/// tg `ACCESS_COMMAND`, the console's `req_access`.
pub const ACCESS_COMMAND: &str = "command";
/// tg `ACCESS_CAPTAIN`: the console's alert level section.
pub const ACCESS_CAPTAIN: &str = "captain";
/// tg `setTimer(20)` after a failed docking.
const RETRY_SECS: f64 = 2.0;
pub(crate) const EMERGENCY_PORT: &str = "/obj/docking_port/mobile/emergency";
/// tg `SHUTTLE_DEFAULT_SHUTTLE_AREA_TYPE`.
const SHUTTLE_AREA: &str = "/area/shuttle";
const SPACE_TURF: &str = "/turf/open/space";
const BASETURF_BOTTOM: &str = "/turf/baseturf_bottom";
const HOME_DOCK: &str = "emergency_home";
const AWAY_DOCK: &str = "emergency_away";
/// Tiles of space around the shuttle on the transit level.
const TRANSIT_MARGIN: i32 = 3;
const SENDER: &str = "Аварийное оповещение эвакуационного шаттла";
/// tg `command_name()`.
const COMMAND_NAME: &str = "Центральное Командование";

/// Emergency shuttle timing (tg `SSshuttle` vars and config). Insert before `GameServerPlugin`
/// to override; otherwise tg's defaults with these environment overrides (seconds):
/// `SS13_SHUTTLE_CALL_SECONDS` (`emergency_call_time`, 600; times the security level's
/// `shuttle_call_time_mod`: green doubles it),
/// `SS13_SHUTTLE_DOCK_SECONDS` (`emergency_dock_time`, 180), `SS13_SHUTTLE_ESCAPE_SECONDS`
/// (`emergency_escape_time`, 120), `SS13_SHUTTLE_REFUEL_SECONDS` (`SHUTTLE_REFUEL_DELAY`, 1200:
/// no call before that much round time), `SS13_SHUTTLE_AUTOCALL_THRESHOLD`
/// (`EMERGENCY_SHUTTLE_AUTOCALL_THRESHOLD`, off: the shuttle is sent when that share of the
/// crew is alive or less).
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct ShuttleConfig {
    pub call_secs: f64,
    pub dock_secs: f64,
    pub escape_secs: f64,
    pub refuel_secs: f64,
    pub autocall_threshold: Option<f64>,
    /// The emergency shuttle `.dmm` (absolute); `None`: the map json's.
    pub template: Option<PathBuf>,
}

impl ShuttleConfig {
    pub fn tg() -> Self {
        Self {
            call_secs: 600.0,
            dock_secs: 180.0,
            escape_secs: 120.0,
            refuel_secs: 1200.0,
            autocall_threshold: None,
            template: None,
        }
    }
}

impl Default for ShuttleConfig {
    fn default() -> Self {
        let secs = |key: &str| {
            std::env::var(key)
                .ok()
                .and_then(|v| v.trim().parse::<f64>().ok())
                .filter(|v| v.is_finite() && *v >= 0.0)
        };
        let tg = Self::tg();
        Self {
            call_secs: secs("SS13_SHUTTLE_CALL_SECONDS").unwrap_or(tg.call_secs),
            dock_secs: secs("SS13_SHUTTLE_DOCK_SECONDS").unwrap_or(tg.dock_secs),
            escape_secs: secs("SS13_SHUTTLE_ESCAPE_SECONDS").unwrap_or(tg.escape_secs),
            refuel_secs: secs("SS13_SHUTTLE_REFUEL_SECONDS").unwrap_or(tg.refuel_secs),
            autocall_threshold: secs("SS13_SHUTTLE_AUTOCALL_THRESHOLD").filter(|t| *t <= 1.0),
            template: None,
        }
    }
}

/// tg `SHUTTLE_*` modes the emergency shuttle uses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ShuttleMode {
    #[default]
    Idle,
    Igniting,
    Recall,
    Call,
    Docked,
    Escape,
    Endgame,
}

/// tg's mobile port timer (`timer`, `last_timer_length`), in seconds of server time.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ShuttleTimer {
    at: Option<f64>,
    last_length: f64,
}

impl ShuttleTimer {
    /// tg `setTimer`.
    pub fn set(&mut self, now: f64, wait: f64) {
        self.at = Some(now + wait);
        self.last_length = wait;
    }

    pub fn clear(&mut self) {
        self.at = None;
    }

    /// tg `timeLeft(1)` in seconds; `None` without a timer.
    pub fn left(&self, now: f64) -> Option<f64> {
        self.at.map(|at| (at - now).max(0.0))
    }

    /// tg `invertTimer`: a recall takes as long as the shuttle has flown.
    pub fn invert(&mut self, now: f64) {
        let Some(at) = self.at else {
            return;
        };
        if self.last_length <= 0.0 {
            return;
        }
        let remaining = at - now;
        if remaining > 0.0 {
            self.set(now, self.last_length - remaining);
        }
    }
}

/// What [`Emergency::check`] asks the world to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShuttleStep {
    /// The call ran out: dock at `emergency_home`, then [`Emergency::docked_home`].
    DockHome,
    /// Ignition is over: enter transit, then [`Emergency::launched`].
    Launch,
    /// The escape ran out: dock at `emergency_away`, then [`Emergency::arrived`].
    DockAway,
}

/// The emergency shuttle's state machine (tg `/obj/docking_port/mobile/emergency` `check`,
/// `request`, `cancel` and `SSshuttle` call bookkeeping), without the world.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Emergency {
    pub mode: ShuttleMode,
    pub timer: ShuttleTimer,
    /// tg `alert_coeff` of the current call.
    pub alert_coeff: f64,
    /// tg `emergencyCallAmount`.
    pub call_count: u32,
    /// tg `emergency_last_call_loc`: the traced area's name.
    pub last_call_area: Option<String>,
    /// tg `emergency_no_recall`.
    pub no_recall: bool,
    /// tg `hijack_status`: `HIJACK_NOT_BEGUN` (0) to [`HIJACK_COMPLETED`].
    pub hijack_status: u8,
}

/// tg `HIJACK_COMPLETED`: the fifth stage of the console's navigation override.
pub const HIJACK_COMPLETED: u8 = 5;
/// The console's `hijack_flight_time_increase`: a stage during the flight lengthens it.
pub const HIJACK_FLIGHT_TIME_INCREASE: f64 = 30.0;
/// The console's `hijack_completion_flight_time_set`: the flight left after the last stage.
pub const HIJACK_COMPLETION_FLIGHT_SECS: f64 = 10.0;

impl Emergency {
    /// tg `request`: from idle, called or recalling the shuttle is (re)called; false otherwise.
    pub fn request(&mut self, now: f64, call_secs: f64, coeff: f64) -> bool {
        if !matches!(
            self.mode,
            ShuttleMode::Recall | ShuttleMode::Idle | ShuttleMode::Call
        ) {
            return false;
        }
        self.alert_coeff = coeff;
        self.mode = ShuttleMode::Call;
        self.timer.set(now, call_secs * coeff);
        self.call_count += 1;
        true
    }

    /// tg `cancel`: a called shuttle turns back.
    pub fn cancel(&mut self, now: f64) -> bool {
        if self.mode != ShuttleMode::Call {
            return false;
        }
        self.timer.invert(now);
        self.mode = ShuttleMode::Recall;
        true
    }

    /// tg `check`, once per tick.
    pub fn check(&mut self, now: f64) -> Option<ShuttleStep> {
        let left = self.timer.left(now)?;
        match self.mode {
            ShuttleMode::Recall if left <= 0.0 => {
                self.mode = ShuttleMode::Idle;
                self.timer.clear();
                None
            }
            ShuttleMode::Call if left <= 0.0 => Some(ShuttleStep::DockHome),
            ShuttleMode::Docked if left <= ENGINE_START_SECS => {
                self.mode = ShuttleMode::Igniting;
                None
            }
            ShuttleMode::Igniting if left <= 0.0 => Some(ShuttleStep::Launch),
            ShuttleMode::Escape if left <= 0.0 => Some(ShuttleStep::DockAway),
            _ => None,
        }
    }

    /// After [`ShuttleStep::DockHome`]: docked, or another try in 2 s.
    pub fn docked_home(&mut self, now: f64, success: bool, dock_secs: f64) {
        if success {
            self.mode = ShuttleMode::Docked;
            self.timer.set(now, dock_secs);
        } else {
            self.timer.set(now, RETRY_SECS);
        }
    }

    /// After [`ShuttleStep::Launch`].
    pub fn launched(&mut self, now: f64, escape_secs: f64) {
        self.mode = ShuttleMode::Escape;
        self.timer.set(now, escape_secs);
    }

    /// After [`ShuttleStep::DockAway`].
    pub fn arrived(&mut self) {
        self.mode = ShuttleMode::Endgame;
        self.timer.clear();
    }

    /// tg `SSshuttle.canEvac`: `Err` with the reason a call is refused.
    pub fn can_evac(&self, round_secs: f64, refuel_secs: f64) -> Result<(), String> {
        if round_secs < refuel_secs {
            return Err(format!(
                "Эвакуационный шаттл на дозаправке. Следующий вызов будет доступен в течении {}.",
                display_time_text(refuel_secs - round_secs)
            ));
        }
        let reason = match self.mode {
            ShuttleMode::Recall => {
                "Эвакуационный шаттл не может быть отозван на пути к Центральному Командованию."
            }
            ShuttleMode::Call => "Эвакуационный шаттл уже в пути.",
            ShuttleMode::Docked => "Эвакуационный шаттл уже состыкован со станцией.",
            ShuttleMode::Igniting => "Эвакуационный шаттл производит запуск двигателей для отлёта.",
            ShuttleMode::Escape => "Эвакуационный шаттл удаляется на безопасное расстояние.",
            ShuttleMode::Idle | ShuttleMode::Endgame => return Ok(()),
        };
        Err(reason.to_owned())
    }

    /// tg `can_recall` for a non-admin: a call with enough of it left for the security level
    /// (`past_restriction_point`: the base call time at green, half of it at blue, a quarter
    /// above).
    pub fn can_recall(&self, now: f64, call_secs: f64, level: SecLevel) -> bool {
        let needed = match level {
            SecLevel::Green => call_secs,
            SecLevel::Blue => call_secs * 0.5,
            _ => call_secs * 0.25,
        };
        self.mode == ShuttleMode::Call
            && !self.no_recall
            && self.timer.left(now).is_some_and(|left| left >= needed)
    }

    /// tg `alert_coeff_change`: the time left (and the length a recall inverts) scale by the new
    /// coefficient over the old.
    // tg's `setTimer` here also overwrites `last_timer_length` with the time left, so a recall
    // after a level change would return at once; the scaled length is kept instead.
    pub fn alert_coeff_change(&mut self, now: f64, coeff: f64) {
        let Some(at) = self.timer.at else {
            return;
        };
        let remaining = at - now;
        if remaining < 0.0 || self.timer.last_length <= 0.0 || self.alert_coeff <= 0.0 {
            return;
        }
        let multiplier = coeff / self.alert_coeff;
        self.timer.at = Some(now + remaining * multiplier);
        self.timer.last_length *= multiplier;
        self.alert_coeff = coeff;
    }

    /// `EMERGENCY_ESCAPED_OR_ENDGAMED`.
    pub fn escaped_or_endgamed(&self) -> bool {
        matches!(self.mode, ShuttleMode::Escape | ShuttleMode::Endgame)
    }

    /// The console's `IS_DOCKED && !ENGINES_STARTED`: docked, engines not started.
    pub fn docked_idle(&self) -> bool {
        self.mode == ShuttleMode::Docked
    }

    /// The console's early launch (`setTimer(ENGINE_START_TIME)`): the engines start at once and
    /// the shuttle leaves 10 s later. False unless [`docked_idle`](Self::docked_idle).
    pub fn early_launch(&mut self, now: f64) -> bool {
        // Already launching early: the console's `process` runs again before the mode turns.
        if !self.docked_idle() || self.timer.left(now).is_some_and(|l| l <= ENGINE_START_SECS) {
            return false;
        }
        self.timer.set(now, ENGINE_START_SECS);
        true
    }

    /// tg `is_hijacked`.
    pub fn is_hijacked(&self) -> bool {
        self.hijack_status >= HIJACK_COMPLETED
    }

    /// The console's `increase_hijack_stage` on the shuttle: the next stage; in flight the last
    /// stage cuts the flight to 10 s, the others lengthen it by 30 s.
    pub fn hijack_stage(&mut self, now: f64) -> u8 {
        self.hijack_status = (self.hijack_status + 1).min(HIJACK_COMPLETED);
        if self.mode == ShuttleMode::Escape {
            let wait = if self.is_hijacked() {
                HIJACK_COMPLETION_FLIGHT_SECS
            } else {
                self.timer.left(now).unwrap_or(0.0) + HIJACK_FLIGHT_TIME_INCREASE
            };
            self.timer.set(now, wait);
        }
        self.hijack_status
    }
}

/// tg `width`, `height`, `dwidth`, `dheight` of a docking port.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PortSize {
    pub width: i32,
    pub height: i32,
    pub dwidth: i32,
    pub dheight: i32,
}

/// A registered `/obj/docking_port/stationary` (or a dock made at round start).
#[derive(Component, Clone, Debug)]
pub struct StationaryDock {
    pub id: String,
    pub name: String,
    pub dir: Dir,
    pub size: PortSize,
    /// tg `area_type`: the area under the dock, given back to tiles a shuttle leaves.
    pub area: Option<Entity>,
}

/// A registered `/obj/docking_port/mobile`.
#[derive(Component, Clone, Debug)]
pub struct MobileDock {
    pub id: String,
    pub dir: Dir,
    pub size: PortSize,
    /// tg `shuttle_areas`.
    pub areas: Vec<Entity>,
    /// tg `underlying_areas_by_turf`.
    pub underlying: HashMap<TilePos, Entity>,
    /// The stationary dock it sits on.
    pub docked: Option<Entity>,
}

/// The round's emergency shuttle (tg `SSshuttle.emergency`), when the map has one.
#[derive(Resource, Debug)]
pub struct EmergencyShuttle {
    pub state: Emergency,
    /// The mobile port.
    pub port: Entity,
    pub name: String,
    pub home: Option<Entity>,
    pub away: Entity,
    pub transit: Entity,
    /// The level standing in for CentCom.
    pub centcom_z: u16,
    pub transit_z: u16,
    /// The last [`initiate_docking`].
    pub last_move: Option<MoveReport>,
    /// It flew to the Syndicate instead of CentCom (hijacked, or the revolution won).
    pub syndicate_landing: bool,
    /// tg `sound_played`: the warm-up sound of this launch played.
    pub sound_played: bool,
}

#[derive(Resource)]
struct ShuttleRng(Rng);

/// What one shuttle move did.
#[derive(Clone, Debug, PartialEq)]
pub struct MoveReport {
    pub turfs: usize,
    pub movables: usize,
    /// Destination things crushed or pushed away (tg `toShuttleMove`).
    pub crushed: usize,
    pub elapsed: Duration,
}

/// Why [`initiate_docking`] did not move the shuttle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DockingError {
    /// tg `DOCKING_NULL_SOURCE` / `DOCKING_NULL_DESTINATION`: a turf off the map.
    NullSource,
    NullDestination,
    /// Not a registered port.
    NotAPort,
    /// The two rectangles overlap on one level (unsupported here).
    Overlap,
}

pub struct ShuttleServerPlugin;

impl Plugin for ShuttleServerPlugin {
    fn build(&self, app: &mut App) {
        if !app.world().contains_resource::<ShuttleConfig>() {
            app.init_resource::<ShuttleConfig>();
        }
        let config = app.world().resource::<ShuttleConfig>().clone();
        let template = config.template.clone().or_else(|| {
            let map = app.world().get_resource::<MapLoadPlugin>()?;
            let name = map.maps.first().filter(|m| !m.ends_with(".dmm"))?;
            let id = map_shuttle(&map.source, name, "emergency")?;
            Some(shuttle_template_path(&map.source, "emergency", &id))
        });
        if let Some(template) = template {
            info!("emergency shuttle template {}", template.display());
            push_template(app, template.clone());
            app.insert_resource(EmergencyTemplate(template));
        }
        let roundstart = app
            .world()
            .get_resource::<MapLoadPlugin>()
            .and_then(|map| {
                // Only a real station map: fixture maps name `.dmm` files.
                map.maps.first().filter(|m| !m.ends_with(".dmm"))?;
                let path = map.map_paths().into_iter().next()?;
                let dmm = DmmMap::from_file(&path)
                    .map_err(|e| warn!("roundstart shuttles of {}: {e}", path.display()))
                    .ok()?;
                Some(RoundstartPorts {
                    source: map.source.clone(),
                    ports: roundstart_ports(&dmm),
                })
            })
            .unwrap_or_default();
        app.insert_resource(roundstart);
        app.add_systems(Startup, setup_shuttle)
            .add_systems(Update, tick)
            .add_plugins((
                crate::comms_server::CommsServerPlugin,
                crate::shuttle_console_server::ShuttleConsoleServerPlugin,
                crate::escape_pods::EscapePodsPlugin,
                crate::arrivals::ArrivalsPlugin,
                crate::shuttle_nav::ShuttleNavPlugin,
                crate::shuttle_catalog::ShuttleCatalogPlugin,
                crate::mining_shuttles::MiningShuttlesPlugin,
                crate::away_shuttles::AwayShuttlesPlugin,
                crate::shuttle_remote::ShuttleRemotePlugin,
                crate::transport::TransportPlugin,
                crate::shuttle_docker::ShuttleDockerPlugin,
            ));
    }
}

/// The map's emergency shuttle template.
#[derive(Resource)]
pub(crate) struct EmergencyTemplate(pub(crate) PathBuf);

/// Adds `template` to the [`ShuttleTemplates`] the station loads with (plugin build time).
pub fn push_template(app: &mut App, template: PathBuf) {
    let world = app.world_mut();
    match world.get_resource_mut::<ShuttleTemplates>() {
        Some(mut templates) => templates.0.push(template),
        None => {
            world.insert_resource(ShuttleTemplates(vec![template]));
        }
    }
}

/// tg `/obj/docking_port/stationary/load_roundstart`: a stationary port of the station map with
/// a `roundstart_template` (an escape pod loader has `escape_pod/default` by type).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoundstartPort {
    /// 1-based map coordinates (the map's own z).
    pub x: u32,
    pub y: u32,
    pub z: u32,
    /// The template's `port_id` (`arrival`, `escape_pod`) and id (`arrival_box`).
    pub port_id: String,
    pub id: String,
}

/// The station map's [`RoundstartPort`]s in map order (x fastest, then y, then z), read when the
/// plugin builds, before the map loads.
#[derive(Resource, Clone, Debug, Default)]
pub struct RoundstartPorts {
    /// The BYOND checkout the templates come from.
    pub source: PathBuf,
    pub ports: Vec<RoundstartPort>,
}

impl RoundstartPorts {
    /// The ports of templates with `port_id`, with each template's `.dmm`.
    pub fn of(&self, port_id: &str) -> Vec<(RoundstartPort, PathBuf)> {
        self.ports
            .iter()
            .filter(|p| p.port_id == port_id)
            .map(|p| {
                let path = shuttle_template_path(&self.source, &p.port_id, &p.id);
                (p.clone(), path)
            })
            .collect()
    }
}

const ESCAPE_POD_LOADER: &str = "/obj/docking_port/stationary/escape_pod";
const TEMPLATE_ROOT: &str = "/datum/map_template/shuttle/";
/// Stationary port types whose `roundstart_template` is a type default, not a map edit
/// (`code/modules/mining/mine_items.dm`, `laborcamp/laborshuttle.dm`).
const TYPE_TEMPLATES: [(&str, &str); 10] = [
    (
        "/obj/docking_port/stationary/mining_home",
        "/datum/map_template/shuttle/mining/delta",
    ),
    (
        "/obj/docking_port/stationary/mining_home/kilo",
        "/datum/map_template/shuttle/mining/kilo",
    ),
    (
        "/obj/docking_port/stationary/mining_home/northstar",
        "/datum/map_template/shuttle/mining/northstar",
    ),
    (
        "/obj/docking_port/stationary/mining_home/nebula",
        "/datum/map_template/shuttle/mining/nebula",
    ),
    (
        "/obj/docking_port/stationary/mining_home/common",
        "/datum/map_template/shuttle/mining_common/meta",
    ),
    (
        "/obj/docking_port/stationary/mining_home/common/kilo",
        "/datum/map_template/shuttle/mining_common/kilo",
    ),
    (
        "/obj/docking_port/stationary/mining_home/common/northstar",
        "/datum/map_template/shuttle/mining_common/northstar",
    ),
    (
        "/obj/docking_port/stationary/laborcamp_home",
        "/datum/map_template/shuttle/labour/delta",
    ),
    (
        "/obj/docking_port/stationary/laborcamp_home/kilo",
        "/datum/map_template/shuttle/labour/kilo",
    ),
    (
        "/obj/docking_port/stationary/laborcamp_home/nebula",
        "/datum/map_template/shuttle/labour/nebula",
    ),
];

/// `/datum/map_template/shuttle/<port_id>/<suffix>` -> `(port_id, "<port_id>_<suffix>")`.
pub fn template_id(path: &str) -> Option<(String, String)> {
    let rest = path.trim().strip_prefix(TEMPLATE_ROOT)?;
    let (port_id, suffix) = rest.rsplit_once('/')?;
    Some((
        port_id.to_owned(),
        format!("{}_{suffix}", port_id.replace('/', "_")),
    ))
}

fn port_template(obj: &DmmObject) -> Option<(String, String)> {
    if !is_under(&obj.path, STATIONARY_PORT_ROOT) {
        return None;
    }
    match obj.var("roundstart_template") {
        Some(DmValue::Path(path)) => template_id(path),
        Some(_) => None,
        None => {
            if let Some((_, template)) = TYPE_TEMPLATES.iter().find(|(t, _)| *t == obj.path) {
                return template_id(template);
            }
            // shortcut: no station traits, so never the cramped or luxury pods.
            is_under(&obj.path, ESCAPE_POD_LOADER)
                .then(|| ("escape_pod".to_owned(), "escape_pod_default".to_owned()))
        }
    }
}

/// The [`RoundstartPort`]s of a parsed station map.
pub fn roundstart_ports(map: &DmmMap) -> Vec<RoundstartPort> {
    let templates: Vec<Option<(String, String)>> = map
        .prefabs
        .iter()
        .map(|p| p.objects.iter().find_map(port_template))
        .collect();
    let mut out = Vec::new();
    if templates.iter().all(Option::is_none) {
        return out;
    }
    let (sx, sy, sz) = map.size;
    for z in 1..=sz {
        for y in 1..=sy {
            for x in 1..=sx {
                if let Some(i) = map.index(x, y, z)
                    && let Some(Some((port_id, id))) = templates.get(i as usize)
                {
                    out.push(RoundstartPort {
                        x,
                        y,
                        z,
                        port_id: port_id.clone(),
                        id: id.clone(),
                    });
                }
            }
        }
    }
    out
}

/// The registered stationary dock at map coordinates `(x, y, z)` of the station map.
pub fn dock_at_map_pos(world: &mut World, x: u32, y: u32, z: u32) -> Option<Entity> {
    let first_z = world.get_resource::<MapLoadReport>()?.maps.first()?.first_z;
    let pos = TilePos::new(x as i32, y as i32, first_z + z as u16 - 1);
    let mut q = world.query_filtered::<(Entity, &TilePos), With<StationaryDock>>();
    q.iter(world).find(|(_, p)| **p == pos).map(|(e, _)| e)
}

fn area_path(world: &World, area: Entity) -> Option<&str> {
    world.get::<DmPower>(area).map(|p| p.path.as_str())
}

pub(crate) fn area_named(world: &World, path: &str) -> Option<Entity> {
    let areas = world.resource::<Areas>();
    areas
        .entities()
        .iter()
        .copied()
        .find(|&a| area_path(world, a) == Some(path))
}

pub(crate) fn area_name(world: &World, area: Entity) -> String {
    world
        .get::<DmName>(area)
        .map(|n| plain(&n.name).to_owned())
        .unwrap_or_default()
}

fn plain(name: &str) -> &str {
    name.trim_start_matches("\\improper ")
        .trim_start_matches("\\proper ")
}

/// BYOND `dir2angle`.
fn angle(dir: Dir) -> i32 {
    match dir {
        Dir::North => 0,
        Dir::NorthEast => 45,
        Dir::East => 90,
        Dir::SouthEast => 135,
        Dir::South => 180,
        Dir::SouthWest => 225,
        Dir::West => 270,
        Dir::NorthWest => 315,
    }
}

/// BYOND `angle2dir` of a multiple of 45.
fn angle_dir(degrees: i32) -> Dir {
    match degrees.rem_euclid(360) {
        0 => Dir::North,
        45 => Dir::NorthEast,
        90 => Dir::East,
        135 => Dir::SouthEast,
        180 => Dir::South,
        225 => Dir::SouthWest,
        270 => Dir::West,
        _ => Dir::NorthWest,
    }
}

/// tg `return_ordered_turfs(x, y, z, dir)`: the port's rectangle, always in the same order, so
/// turf `i` of one dock lands on turf `i` of another whatever their directions.
pub fn ordered_turfs(origin: TilePos, dir: Dir, size: PortSize) -> Vec<TilePos> {
    let (cos, sin) = match dir {
        Dir::West => (0, 1),
        Dir::South => (-1, 0),
        Dir::East => (0, -1),
        _ => (1, 0),
    };
    let mut out = Vec::with_capacity((size.width * size.height).max(0) as usize);
    for dx in 0..size.width {
        let cx = dx - size.dwidth;
        for dy in 0..size.height {
            let cy = dy - size.dheight;
            out.push(TilePos::new(
                origin.x + cx * cos - cy * sin,
                origin.y + cy * cos + cx * sin,
                origin.z,
            ));
        }
    }
    out
}

/// tg `calculate_docking_port_information` from a template of `width` x `height` with the port
/// on tile `(px, py)` of it (1-based).
pub fn port_size(dir: Dir, width: i32, height: i32, px: i32, py: i32) -> PortSize {
    let (w, h) = if matches!(dir, Dir::East | Dir::West) {
        (height, width)
    } else {
        (width, height)
    };
    let (dwidth, dheight) = match dir {
        Dir::East => (height - py, px - 1),
        Dir::South => (width - px, height - py),
        Dir::West => (py - 1, width - px),
        _ => (px - 1, py - 1),
    };
    PortSize {
        width: w,
        height: h,
        dwidth,
        dheight,
    }
}

fn dir_of(world: &World, e: Entity) -> Dir {
    world
        .get::<DmAtom>(e)
        .and_then(|a| Dir::from_bits(a.dir))
        .unwrap_or(Dir::North)
}

/// tg `SSshuttle.Initialize` -> `setup_shuttles` for the emergency shuttle: registers the
/// stationary ports, the template's mobile port, the CentCom stand-in and the transit dock.
pub(crate) fn setup_shuttle(world: &mut World) {
    // Stationary ports on any map, with tg's `_<n>` suffix for repeated ids.
    let mut query = world.query::<(Entity, &DmDockingPort, &TilePos)>();
    let ports: Vec<(Entity, DmDockingPort, TilePos)> = query
        .iter(world)
        .map(|(e, d, p)| (e, d.clone(), *p))
        .collect();
    let mut counters: HashMap<String, u32> = HashMap::new();
    let mut docks = Vec::new();
    for (e, dm, pos) in ports.iter().filter(|(_, d, _)| !d.mobile()) {
        let base = if dm.shuttle_id.is_empty() {
            "dock".to_owned()
        } else {
            dm.shuttle_id.clone()
        };
        let n = counters.entry(base.clone()).or_default();
        *n += 1;
        let id = if *n > 1 { format!("{base}_{n}") } else { base };
        let name = world
            .get::<DmName>(*e)
            .map(|n| plain(&n.name).to_owned())
            .unwrap_or_else(|| "dock".into());
        let area = world.resource::<Areas>().area_at(*pos);
        docks.push((
            *e,
            StationaryDock {
                id,
                name,
                dir: dir_of(world, *e),
                size: PortSize {
                    width: dm.width,
                    height: dm.height,
                    dwidth: dm.dwidth,
                    dheight: dm.dheight,
                },
                area,
            },
        ));
    }
    let home = docks
        .iter()
        .find(|(_, d)| d.id == HOME_DOCK)
        .map(|(e, _)| *e);
    world.insert_batch(docks);

    let Some(template) = world
        .get_resource::<EmergencyTemplate>()
        .map(|t| t.0.clone())
    else {
        return;
    };
    let Some(shuttle) = setup_template_shuttle(
        world,
        &template,
        EMERGENCY_PORT,
        "emergency",
        AWAY_DOCK,
        "CentCom Emergency Shuttle Dock",
    ) else {
        return;
    };
    let seed = world.get_resource::<MapSeed>().map_or(0, |s| s.0);
    world.insert_resource(ShuttleRng(Rng::new(seed ^ 0x5348_5554)));
    let size = world
        .get::<MobileDock>(shuttle.port)
        .map(|m| m.size)
        .unwrap_or_default();
    info!(
        "emergency shuttle {}: {}x{} at z{}, home dock {}, transit z{}",
        shuttle.name,
        size.width,
        size.height,
        shuttle.level_z,
        if home.is_some() { "found" } else { "missing" },
        shuttle.transit_z
    );
    world.insert_resource(EmergencyShuttle {
        state: Emergency {
            alert_coeff: ALERT_COEFF_GREEN,
            ..default()
        },
        port: shuttle.port,
        name: shuttle.name,
        home,
        away: shuttle.away,
        transit: shuttle.transit,
        centcom_z: shuttle.level_z,
        transit_z: shuttle.transit_z,
        last_move: None,
        syndicate_landing: false,
        sound_played: false,
    });
}

/// A shuttle loaded from a template onto its own level ([`setup_template_shuttle`]).
#[derive(Clone, Debug)]
pub struct TemplateShuttle {
    /// The mobile port, with its [`MobileDock`].
    pub port: Entity,
    pub name: String,
    /// The dock where the template put it (the level stands in for CentCom).
    pub away: Entity,
    pub transit: Entity,
    pub level_z: u16,
    pub transit_z: u16,
}

/// tg `/datum/map_template/shuttle` loading + `register()` for the template at `template`
/// (loaded with the station, `ShuttleTemplates`): its turfs get the skipover baseturf, the
/// mobile port under `port_root` gets its size and areas and sits on a new stationary dock
/// `away_id` where the template placed it, and a transit level is made for it.
pub fn setup_template_shuttle(
    world: &mut World,
    template: &std::path::Path,
    port_root: &str,
    id: &str,
    away_id: &str,
    away_name: &str,
) -> Option<TemplateShuttle> {
    setup_template_copy(world, template, 0, port_root, id, away_id, away_name)
}

/// [`setup_template_shuttle`] for the `nth` (0-based) copy of `template` in
/// [`ShuttleTemplates`] (one template loaded several times, as escape pods are).
pub fn setup_template_copy(
    world: &mut World,
    template: &std::path::Path,
    nth: usize,
    port_root: &str,
    id: &str,
    away_id: &str,
    away_name: &str,
) -> Option<TemplateShuttle> {
    let Some((level_z, (tw, th, _))) = world.get_resource::<MapLoadReport>().and_then(|r| {
        r.maps
            .iter()
            .filter(|m| m.path == template)
            .nth(nth)
            .map(|m| (m.first_z, m.size))
    }) else {
        warn!(
            "{id} shuttle template {} was not loaded",
            template.display()
        );
        return None;
    };
    let mut query = world.query::<(Entity, &DmDockingPort, &TilePos)>();
    let Some((port, port_pos)) = query
        .iter(world)
        .find(|(_, d, p)| p.z == level_z && is_under(&d.path, port_root))
        .map(|(e, _, p)| (e, *p))
    else {
        warn!("{}: no {port_root}", template.display());
        return None;
    };
    let dir = dir_of(world, port);
    let size = port_size(dir, tw as i32, th as i32, port_pos.x, port_pos.y);
    let turfs = ordered_turfs(port_pos, dir, size);
    let mut areas = Vec::new();
    for &pos in &turfs {
        let area = world.resource::<Areas>().area_at(pos);
        if let Some(area) = area
            && area_path(world, area).is_some_and(|p| is_under(p, SHUTTLE_AREA))
            && !areas.contains(&area)
        {
            areas.push(area);
        }
        // tg loads templates with `place_on_top` over the reservation's space, so a turf's
        // stack starts with that space; `dispatch` then puts the skipover third.
        let Some(turf) = turf_at(world, pos) else {
            continue;
        };
        if path_of(world, turf).is_none_or(|p| is_under(&p, SPACE_TURF)) {
            continue;
        }
        let mut stack = vec![SPACE_TURF.to_owned(), DEFAULT_TURF.to_owned()];
        stack.extend(
            baseturfs_of(world, turf)
                .into_iter()
                .filter(|b| b != BASETURF_BOTTOM),
        );
        stack.insert(2, SHUTTLE_SKIPOVER.to_owned());
        world.entity_mut(turf).insert(Baseturfs(stack));
    }
    let space = area_named(world, DEFAULT_AREA).or_else(|| {
        let areas = world.resource::<Areas>();
        areas.entities().first().copied()
    });
    let away = world
        .spawn((
            StationaryDock {
                id: away_id.into(),
                name: away_name.into(),
                dir,
                size,
                area: space,
            },
            port_pos,
        ))
        .id();
    let name = world
        .get::<DmName>(port)
        .map(|n| plain(&n.name).to_owned())
        .unwrap_or_else(|| format!("{id} shuttle"));
    world.entity_mut(port).insert(MobileDock {
        id: id.into(),
        dir,
        size,
        areas,
        underlying: HashMap::new(),
        docked: Some(away),
    });
    let (transit, transit_z) = make_transit(world, &format!("{id}_transit"), dir, size, space)?;
    Some(TemplateShuttle {
        port,
        name,
        away,
        transit,
        level_z,
        transit_z,
    })
}

/// tg `/obj/docking_port/mobile/Initialize(mapload)` of a port mapped in place rather than
/// loaded from a template: its size from its own vars, `shuttle_areas` from the `/area/shuttle`
/// areas of its rectangle (`area_type`), docked on the stationary port under it (`get_docked`).
/// False when it has no size or no shuttle area.
// shortcut: its turfs in those areas get the shuttle skipover on top of their baseturfs, as a
// template's do, so they move with it (tg moves only turfs mapped with a skipover baseturf).
pub fn register_mapped_port(world: &mut World, port: Entity, id: &str) -> bool {
    let (Some(dm), Some(&pos)) = (
        world.get::<DmDockingPort>(port).cloned(),
        world.get::<TilePos>(port),
    ) else {
        return false;
    };
    let size = PortSize {
        width: dm.width,
        height: dm.height,
        dwidth: dm.dwidth,
        dheight: dm.dheight,
    };
    if size.width <= 0 || size.height <= 0 {
        return false;
    }
    let dir = dir_of(world, port);
    let mut areas = Vec::new();
    for p in ordered_turfs(pos, dir, size) {
        let Some(area) = world.resource::<Areas>().area_at(p) else {
            continue;
        };
        if !area_path(world, area).is_some_and(|a| is_under(a, SHUTTLE_AREA)) {
            continue;
        }
        if !areas.contains(&area) {
            areas.push(area);
        }
        if let Some(turf) = turf_at(world, p) {
            let mut stack = baseturfs_of(world, turf);
            if !stack.iter().any(|b| b == SHUTTLE_SKIPOVER) {
                stack.push(SHUTTLE_SKIPOVER.to_owned());
                world.entity_mut(turf).insert(Baseturfs(stack));
            }
        }
    }
    if areas.is_empty() {
        return false;
    }
    let docked = {
        let mut q = world.query_filtered::<(Entity, &TilePos), With<StationaryDock>>();
        q.iter(world).find(|(_, p)| **p == pos).map(|(e, _)| e)
    };
    world.entity_mut(port).insert(MobileDock {
        id: id.into(),
        dir,
        size,
        areas,
        underlying: HashMap::new(),
        docked,
    });
    true
}

/// A space level just big enough for the shuttle, with its transit dock.
fn make_transit(
    world: &mut World,
    id: &str,
    dir: Dir,
    size: PortSize,
    space: Option<Entity>,
) -> Option<(Entity, u16)> {
    let rel = ordered_turfs(TilePos::new(0, 0, 0), dir, size);
    let (min_x, max_x) = rel
        .iter()
        .fold((i32::MAX, i32::MIN), |(a, b), p| (a.min(p.x), b.max(p.x)));
    let (min_y, max_y) = rel
        .iter()
        .fold((i32::MAX, i32::MIN), |(a, b), p| (a.min(p.y), b.max(p.y)));
    let w = (max_x - min_x + 1 + 2 * TRANSIT_MARGIN) as u32;
    let h = (max_y - min_y + 1 + 2 * TRANSIT_MARGIN) as u32;
    let z = {
        let mut index = world.resource_mut::<SpatialIndex>();
        let z = (1..=u16::MAX).find(|&z| index.level_size(z).is_none())?;
        index.create_level(z, w, h).ok()?;
        z
    };
    if let Some(space) = space {
        world.resource_mut::<Areas>().add_level(z, w, h, space);
    }
    for y in 1..=h as i32 {
        for x in 1..=w as i32 {
            change_turf(
                world,
                TilePos::new(x, y, z),
                DEFAULT_TURF,
                Vec::new(),
                false,
            );
        }
    }
    ss13_mapload::light::light_tiles(world, [z]);
    let pos = TilePos::new(TRANSIT_MARGIN + 1 - min_x, TRANSIT_MARGIN + 1 - min_y, z);
    let dock = world
        .spawn((
            StationaryDock {
                id: id.into(),
                name: "In Transit".into(),
                dir,
                size,
                area: space,
            },
            pos,
        ))
        .id();
    Some((dock, z))
}

/// tg `initiate_docking(new_dock)`: moves the shuttle of mobile port `port` onto `dock` at once.
pub fn initiate_docking(
    world: &mut World,
    port: Entity,
    dock: Entity,
) -> Result<MoveReport, DockingError> {
    let start = Instant::now();
    let (Some(mobile), Some(&port_pos)) = (
        world.get::<MobileDock>(port).cloned(),
        world.get::<TilePos>(port),
    ) else {
        return Err(DockingError::NotAPort);
    };
    let (Some(target), Some(&dock_pos)) = (
        world.get::<StationaryDock>(dock).cloned(),
        world.get::<TilePos>(dock),
    ) else {
        return Err(DockingError::NotAPort);
    };
    if mobile.docked == Some(dock) {
        return Ok(MoveReport {
            turfs: 0,
            movables: 0,
            crushed: 0,
            elapsed: start.elapsed(),
        });
    }
    let old_turfs = ordered_turfs(port_pos, mobile.dir, mobile.size);
    let new_turfs = ordered_turfs(dock_pos, target.dir, mobile.size);
    {
        let index = world.resource::<SpatialIndex>();
        if !old_turfs.iter().all(|&p| index.in_bounds(p)) {
            return Err(DockingError::NullSource);
        }
        if !new_turfs.iter().all(|&p| index.in_bounds(p)) {
            return Err(DockingError::NullDestination);
        }
    }
    // shortcut: tg moves overlapping rectangles in order; no dock here overlaps another.
    if port_pos.z == dock_pos.z {
        let old: HashSet<TilePos> = old_turfs.iter().copied().collect();
        if new_turfs.iter().any(|p| old.contains(p)) {
            return Err(DockingError::Overlap);
        }
    }
    let fallback = mobile
        .docked
        .and_then(|d| world.get::<StationaryDock>(d))
        .and_then(|d| d.area)
        .or_else(|| area_named(world, DEFAULT_AREA));
    let rotation = (angle(target.dir) - angle(mobile.dir)).rem_euclid(360);
    let shuttle_areas: HashSet<Entity> = mobile.areas.iter().copied().collect();

    // preflight_check: which tiles move their area, and which their turf and contents.
    struct Plan {
        old: TilePos,
        new: TilePos,
        area: Option<Entity>,
        turf: Option<Entity>,
    }
    let mut plans = Vec::new();
    for (&old, &new) in old_turfs.iter().zip(&new_turfs) {
        let area = world
            .resource::<Areas>()
            .area_at(old)
            .filter(|a| shuttle_areas.contains(a));
        if area.is_none() {
            continue;
        }
        let turf = turf_at(world, old)
            .filter(|&t| baseturfs_of(world, t).iter().any(|b| b == SHUTTLE_SKIPOVER));
        plans.push(Plan {
            old,
            new,
            area,
            turf,
        });
    }

    // toShuttleMove: what stands where the shuttle's turfs land is crushed or pushed.
    let mut crushed = 0;
    for plan in plans.iter().filter(|p| p.turf.is_some()) {
        let here: Vec<Entity> = world.resource::<SpatialIndex>().on_tile(plan.new).to_vec();
        for e in here {
            if stays(world, e) {
                continue;
            }
            crushed += 1;
            if let (Some(mut health), Some(mut stat)) = (
                world.get::<Health>(e).cloned(),
                world.get::<Stat>(e).copied(),
            ) {
                // shortcut: tg gibs the mob; here it dies where it stood.
                apply_damage(&mut health, &mut stat, DamageType::Brute, 1000.0);
                world.entity_mut(e).insert((health, stat));
            } else if world.get::<Anchored>(e).is_some_and(|a| !a.0)
                || world.get::<Inventory>(e).is_none()
                    && world.get::<ss13_mapload::DmItem>(e).is_some()
            {
                let to = plan.new.step(mobile.dir);
                if world.resource::<SpatialIndex>().in_bounds(to) {
                    world.entity_mut(e).insert(to);
                }
            } else if world.get::<crate::ghost::Ghost>(e).is_none() {
                world.despawn(e);
            }
        }
    }

    // tg airlock `beforeShuttleMove`.
    let airlocks = crate::atmos_machinery::airlock_pump::airlocks_on(world, &old_turfs);
    crate::atmos_machinery::airlock_pump::before_shuttle_move(world, &airlocks);

    // takeoff: turfs, areas, contents.
    let mut origins = Vec::new();
    let mut movables = 0;
    let mut touched = Vec::new();
    let mut mobile = mobile;
    for plan in &plans {
        if let Some(turf) = plan.turf {
            let old_air = world
                .get_resource::<Atmos>()
                .and_then(|a| a.air(plan.old).cloned());
            let mut stack = baseturfs_of(world, turf);
            let k = stack
                .iter()
                .rposition(|b| b == SHUTTLE_SKIPOVER)
                .expect("planned turfs have the skipover");
            let above = stack.split_off(k);
            let under = stack;
            let mut new_stack = Vec::new();
            if let Some(dest) = turf_at(world, plan.new) {
                new_stack = baseturfs_of(world, dest);
                if let Some(path) = path_of(world, dest) {
                    new_stack.push(path);
                }
                world.despawn(dest);
            }
            new_stack.extend(above);
            world
                .entity_mut(turf)
                .insert((Baseturfs(new_stack), plan.new));
            move_air(world, turf, plan.new, old_air);
            origins.push((plan.old, under));
        }
        if let Some(area) = plan.area {
            let under = mobile.underlying.remove(&plan.old).or(fallback);
            let mut areas = world.resource_mut::<Areas>();
            if let Some(under) = under {
                areas.set_area(plan.old, under);
            }
            if let Some(prior) = areas.area_at(plan.new) {
                mobile.underlying.insert(plan.new, prior);
            }
            areas.set_area(plan.new, area);
        }
        if plan.turf.is_some() {
            let here: Vec<Entity> = world.resource::<SpatialIndex>().on_tile(plan.old).to_vec();
            for e in here {
                if world.get::<StationaryDock>(e).is_some()
                    || world.get::<Turf>(e).is_some()
                    || world.get::<GasOverlay>(e).is_some()
                    || world.get::<HotspotEffect>(e).is_some()
                {
                    continue;
                }
                let mut entity = world.entity_mut(e);
                entity.insert(plan.new);
                if rotation != 0
                    && let Some(mut facing) = entity.get_mut::<Facing>()
                {
                    facing.0 = angle_dir(angle(facing.0) + rotation);
                }
                movables += 1;
            }
            touched.push(plan.old);
            touched.push(plan.new);
        }
    }

    // cleanup_runway: the tiles left behind get what was under the shuttle.
    let turfs = origins.len();
    for (pos, mut under) in origins {
        let path = under.pop().unwrap_or_else(|| DEFAULT_TURF.to_owned());
        change_turf(world, pos, &path, under, false);
    }
    if let Some(mut atmos) = world.get_resource_mut::<Atmos>() {
        for pos in touched {
            atmos.blocker_changed(pos, false);
        }
    }
    relight(world, &old_turfs);
    relight(world, &new_turfs);
    // tg airlock `afterShuttleMove`.
    crate::atmos_machinery::airlock_pump::after_shuttle_move(world, &airlocks);
    mobile.docked = Some(dock);
    mobile.dir = target.dir;
    world.entity_mut(port).insert(mobile);
    Ok(MoveReport {
        turfs,
        movables,
        crushed,
        elapsed: start.elapsed(),
    })
}

/// Not touched by a landing shuttle: turfs, docking ports, atmos visuals.
fn stays(world: &World, e: Entity) -> bool {
    world.get::<Turf>(e).is_some()
        || world.get::<DmDockingPort>(e).is_some()
        || world.get::<StationaryDock>(e).is_some()
        || world.get::<GasOverlay>(e).is_some()
        || world.get::<HotspotEffect>(e).is_some()
}

/// tg `copy_air_with_tile`: the moved turf's tile takes its kind and the air it had.
fn move_air(world: &mut World, turf: Entity, to: TilePos, air: Option<GasMixture>) {
    let Some(kind) = world.get::<TurfAir>(turf).map(|a| a.kind) else {
        return;
    };
    let Some(mut atmos) = world.get_resource_mut::<Atmos>() else {
        return;
    };
    if let Some(level) = atmos.level_mut(to.z)
        && let Some(tile) = level.tile(to.x, to.y)
    {
        level.change_tile(tile, kind, air);
    }
}

fn relight(world: &mut World, turfs: &[TilePos]) {
    let Some(first) = turfs.first() else {
        return;
    };
    let (mut min, mut max) = ((first.x, first.y), (first.x, first.y));
    for p in turfs {
        min = (min.0.min(p.x), min.1.min(p.y));
        max = (max.0.max(p.x), max.1.max(p.y));
    }
    ss13_mapload::light::relight_rect(world, first.z, min, max);
}

/// tg `priority_announce(text, title, sound, sender_override = SENDER)` to every player.
pub fn announce(world: &mut World, title: &str, text: &str, sound: Option<&str>) {
    let mut priority = Priority::new(text, title).sender(SENDER);
    priority.sound = sound.map(str::to_owned);
    priority.send(world);
}

/// tg `priority_announce(text, title, sender_override = sender)` to every player not in the
/// lobby: the sender header, the title (when not empty) and the text, with the announcer's
/// alert sound.
pub fn priority_announce(world: &mut World, sender: &str, title: &str, text: &str) {
    Priority::new(text, title).sender(sender).send(world);
}

fn declension_minutes(n: u64) -> &'static str {
    crate::jobs::declension_ru(n, "минуты", "минут", "минут")
}

/// tg `timeLeft(60 SECONDS)`: whole minutes, rounded.
fn minutes_left(state: &Emergency, now: f64) -> u64 {
    (state.timer.left(now).unwrap_or(0.0) / 60.0).round() as u64
}

/// The `prob(70)` trace of a call or recall to the caller's area.
fn trace(world: &mut World, area: Option<String>) -> Option<String> {
    let traced = world
        .get_resource_mut::<ShuttleRng>()
        .is_some_and(|mut r| r.0.prob(70.0));
    area.filter(|_| traced)
}

/// tg `SSshuttle.call_evac_shuttle`: called with the security level's coefficient, announced
/// as a priority shuttle at red alert and above.
pub fn call_evac_shuttle(world: &mut World, reason: &str, origin: Option<String>) -> bool {
    let level = crate::security_level::current(world);
    request(
        world,
        reason,
        origin,
        level.shuttle_call_time_mod(),
        level >= SecLevel::Red,
    )
}

/// tg `emergency.request(set_coefficient = coeff)`: the shuttle is called and announced.
pub fn call_shuttle(world: &mut World, reason: &str, origin: Option<String>, coeff: f64) -> bool {
    request(world, reason, origin, coeff, false)
}

fn request(
    world: &mut World,
    reason: &str,
    origin: Option<String>,
    coeff: f64,
    red_alert: bool,
) -> bool {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let call_secs = world.resource::<ShuttleConfig>().call_secs;
    let traced = trace(world, origin);
    let Some(mut shuttle) = world.get_resource_mut::<EmergencyShuttle>() else {
        return false;
    };
    if !shuttle.state.request(now, call_secs, coeff) {
        return false;
    }
    shuttle.state.last_call_area = traced.clone();
    let minutes = minutes_left(&shuttle.state, now);
    let reason = if reason.trim().is_empty() {
        String::new()
    } else {
        format!("\n\nХарактер происшествия:\n{}", reason.trim())
    };
    let traced = if traced.is_some() {
        "\n\nСигнал вызова отслежен. Результаты можно просмотреть на любой консоли коммуникаций."
    } else {
        ""
    };
    let red = if red_alert {
        "Красный уровень угрозы подтверждён: Отправлен приоритетный шаттл. "
    } else {
        ""
    };
    let text = format!(
        "Эвакуационный шаттл вызван. {red}Ожидаемое время прибытия: {minutes} {}.{reason}{traced}",
        declension_minutes(minutes)
    );
    announce(
        world,
        "Вызван эвакуационный шаттл",
        &text,
        Some(ANNOUNCER_SHUTTLECALLED),
    );
    true
}

/// tg `SSshuttle.cancel_evac` -> `emergency.cancel`, after `can_recall` was checked.
pub fn recall_shuttle(world: &mut World, origin: Option<String>) -> bool {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let traced = trace(world, origin);
    let Some(mut shuttle) = world.get_resource_mut::<EmergencyShuttle>() else {
        return false;
    };
    if !shuttle.state.cancel(now) {
        return false;
    }
    shuttle.state.last_call_area = traced.clone();
    let traced = if traced.is_some() {
        " Сигнал отзыва шаттла отслежен. Результаты можно просмотреть на любой консоли коммуникаций."
    } else {
        ""
    };
    announce(
        world,
        "Эвакуационный шаттл отозван",
        &format!("Эвакуационный шаттл был отозван.{traced}"),
        Some(ANNOUNCER_SHUTTLERECALLED),
    );
    true
}

/// `/obj/docking_port/mobile/emergency/on_security_level_changed`: a call or a recall in
/// progress takes the new level's coefficient.
pub fn on_security_level_changed(world: &mut World, level: SecLevel) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    if let Some(mut shuttle) = world.get_resource_mut::<EmergencyShuttle>()
        && matches!(shuttle.state.mode, ShuttleMode::Call | ShuttleMode::Recall)
    {
        shuttle
            .state
            .alert_coeff_change(now, level.shuttle_call_time_mod());
    }
}

/// `SSshuttle.fire` for the emergency shuttle: `CheckAutoEvac` and `emergency.check`.
fn tick(world: &mut World) {
    if !world.contains_resource::<EmergencyShuttle>() {
        return;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    let config = world.resource::<ShuttleConfig>().clone();
    check_auto_evac(world, &config, now);
    let step = world.resource_mut::<EmergencyShuttle>().state.check(now);
    emergency_sounds(world, now);
    let Some(step) = step else {
        return;
    };
    let shuttle = world.resource::<EmergencyShuttle>();
    let (port, home, transit, away, name) = (
        shuttle.port,
        shuttle.home,
        shuttle.transit,
        shuttle.away,
        shuttle.name.clone(),
    );
    match step {
        ShuttleStep::DockHome => {
            let result = home.map(|home| initiate_docking(world, port, home));
            let success = matches!(result, Some(Ok(_)));
            log_move(world, "dock at the station", result);
            world.resource_mut::<EmergencyShuttle>().state.docked_home(
                now,
                success,
                config.dock_secs,
            );
            if success {
                let text = format!(
                    "{name} состыковался со станцией. У вас есть {}, чтобы прибыть на борт шаттла.",
                    display_time_text(config.dock_secs)
                );
                announce(
                    world,
                    "Эвакуационный шаттл прибыл",
                    &text,
                    Some(ANNOUNCER_SHUTTLEDOCK),
                );
            }
        }
        ShuttleStep::Launch => {
            // Every escape pod still at the station follows it (`on_emergency_launch`).
            crate::escape_pods::on_emergency_launch(world);
            hyperspace_sound(world, port, Hyperspace::Launch);
            let result = initiate_docking(world, port, transit);
            log_move(world, "enter transit", Some(result));
            let mut shuttle = world.resource_mut::<EmergencyShuttle>();
            shuttle.state.launched(now, config.escape_secs);
            let minutes = minutes_left(&shuttle.state, now);
            let text = format!(
                "Эвакуационный шаттл покинул станцию. Приблизительное время ожидания: {minutes} {}, \
                 прежде чем шаттл прибудет на {COMMAND_NAME}.",
                declension_minutes(minutes)
            );
            announce(
                world,
                "Произведена отстыковка эвакуационного шаттла",
                &text,
                None,
            );
        }
        ShuttleStep::DockAway => {
            crate::escape_pods::on_emergency_dock(world);
            // BandaStation: a won revolution sends it to the Syndicate too.
            let revolution = world
                .get_resource::<crate::antag::revolution::server::RevolutionHandler>()
                .is_some_and(|r| {
                    r.result == Some(crate::antag::revolution::server::RevResult::Revolution)
                });
            let redirect = revolution || world.resource::<EmergencyShuttle>().state.is_hijacked();
            if redirect {
                crate::security_level::minor_announce(
                    world,
                    "КРИТИЧЕСКАЯ НЕИСПРАВНОСТЬ",
                    "Обнаружен сбой в навигационных протоколах. Пожалуйста, свяжитесь с \
                     руководством.",
                    true,
                );
            }
            let result = initiate_docking(world, port, away);
            log_move(world, "dock at CentCom", Some(result));
            let mut shuttle = world.resource_mut::<EmergencyShuttle>();
            shuttle.syndicate_landing = redirect;
            shuttle.state.arrived();
        }
    }
}

/// The emergency shuttle's `check` sounds: the warm-up 5 s before the launch, the arrival half a
/// second before it docks at CentCom (tg checks once a second; this plays each once per flight).
fn emergency_sounds(world: &mut World, now: f64) {
    let shuttle = world.resource::<EmergencyShuttle>();
    let left = shuttle.state.timer.left(now);
    let phase = match (shuttle.state.mode, left) {
        (ShuttleMode::Igniting, Some(left))
            if left <= HYPERSPACE_WARMUP_SECS && !shuttle.sound_played =>
        {
            Hyperspace::Warmup
        }
        (ShuttleMode::Escape, Some(left))
            if left <= HYPERSPACE_END_SECS && shuttle.sound_played =>
        {
            Hyperspace::End
        }
        _ => return,
    };
    let port = shuttle.port;
    world.resource_mut::<EmergencyShuttle>().sound_played = phase == Hyperspace::Warmup;
    hyperspace_sound(world, port, phase);
}

/// tg `HYPERSPACE_WARMUP`, `HYPERSPACE_LAUNCH`, `HYPERSPACE_END`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hyperspace {
    Warmup,
    Launch,
    End,
}

impl Hyperspace {
    fn sound(self) -> &'static str {
        match self {
            Hyperspace::Warmup => "hyperspace_begin",
            Hyperspace::Launch => "hyperspace_progress",
            Hyperspace::End => "hyperspace_end",
        }
    }
}

/// The emergency shuttle revs its engines this long before the launch (tg `time_left <= 50`).
pub const HYPERSPACE_WARMUP_SECS: f64 = 5.0;
/// tg `HYPERSPACE_END_TIME` (5 deciseconds).
pub const HYPERSPACE_END_SECS: f64 = 0.5;
const SHUTTLE_ENGINE: &str = "/obj/machinery/power/shuttle_engine";
const HYPERSPACE_DIR: &str = "sound/runtime/hyperspace/";

/// tg `/obj/docking_port/mobile/hyperspace_sound(phase)`: players on the shuttle's level within
/// `max(width, height)` tiles of its first engine hear the sound from their nearest engine,
/// within 2.5 times that its distant version from the first engine; a shuttle without engines
/// plays it from one of its doors, one without either plays nothing.
pub fn hyperspace_sound(world: &mut World, port: Entity, phase: Hyperspace) {
    let (Some(mobile), Some(&origin)) = (
        world.get::<MobileDock>(port).cloned(),
        world.get::<TilePos>(port),
    ) else {
        return;
    };
    let (mut engines, mut doors) = (Vec::new(), Vec::new());
    {
        let index = world.resource::<SpatialIndex>();
        for tile in ordered_turfs(origin, mobile.dir, mobile.size) {
            for &e in index.on_tile(tile) {
                if world
                    .get::<DmObj>(e)
                    .is_some_and(|o| is_under(&o.path, SHUTTLE_ENGINE))
                {
                    engines.push(tile);
                } else if world.get::<crate::doors::Door>(e).is_some() {
                    doors.push(tile);
                }
            }
        }
    }
    let Some(&distant) = engines.first().or(doors.first()) else {
        return;
    };
    let range = mobile.size.width.max(mobile.size.height).max(0) as u32;
    let long_range = range as f32 * 2.5;
    let mut players = world.query::<(Entity, &crate::server::PlayerMob, &TilePos)>();
    let listeners: Vec<(Entity, Entity, TilePos)> = players
        .iter(world)
        .filter(|(_, _, p)| p.z == origin.z)
        .map(|(e, p, pos)| (e, p.client, *pos))
        .collect();
    for (mob, client, at) in listeners {
        let Some(far) = at.chebyshev(distant) else {
            continue;
        };
        let (source, file) = if (far as f32) <= long_range && far > range {
            (
                distant,
                format!("{HYPERSPACE_DIR}{}_distance.ogg", phase.sound()),
            )
        } else if far <= range {
            let nearest = engines
                .iter()
                .copied()
                .min_by_key(|e| at.chebyshev(*e).unwrap_or(u32::MAX))
                .unwrap_or(distant);
            (nearest, format!("{HYPERSPACE_DIR}{}.ogg", phase.sound()))
        } else {
            continue;
        };
        if !crate::sound::can_hear(world, mob) {
            continue;
        }
        let (dx, dy) = (source.x - at.x, source.y - at.y);
        let distance = ((dx * dx + dy * dy) as f32).sqrt();
        let volume = world.get_resource::<Atmos>().map_or(100.0, |atmos| {
            100.0
                * crate::sound::pressure_factor(
                    atmos.pressure(at),
                    atmos.pressure(source),
                    distance,
                )
        });
        if volume <= 0.0 {
            continue;
        }
        world.write_message(ss13_net::sound::server::SendSound {
            client,
            sound: ss13_shared::protocol::SoundEvent {
                file,
                volume,
                speed: 1.0,
                offset: Some([dx as i16, dy as i16]),
            },
        });
    }
}

fn log_move(world: &mut World, what: &str, result: Option<Result<MoveReport, DockingError>>) {
    match result {
        Some(Ok(report)) => {
            info!(
                "emergency shuttle: {what}: {} turfs, {} movables, {} crushed in {:.2?}",
                report.turfs, report.movables, report.crushed, report.elapsed
            );
            world.resource_mut::<EmergencyShuttle>().last_move = Some(report);
        }
        Some(Err(e)) => warn!("emergency shuttle could not {what}: {e:?}"),
        None => warn!("emergency shuttle could not {what}: no {HOME_DOCK} dock"),
    }
}

/// tg `SSshuttle.CheckAutoEvac`: with the threshold configured, a crew mostly dead calls the
/// shuttle and blocks recalls.
fn check_auto_evac(world: &mut World, config: &ShuttleConfig, now: f64) {
    let Some(threshold) = config.autocall_threshold.filter(|t| *t > 0.0) else {
        return;
    };
    if world.resource::<EmergencyShuttle>().state.no_recall
        || world.resource::<Ticker>().state() != GameState::Playing
    {
        return;
    }
    let mut crew = world.query_filtered::<Option<&Stat>, With<PlayerJob>>();
    let (total, alive) = crew.iter(world).fold((0u32, 0u32), |(t, a), stat| {
        (t + 1, a + u32::from(stat != Some(&Stat::Dead)))
    });
    if total == 0 || f64::from(alive) / f64::from(total) > threshold {
        return;
    }
    let state = &mut world.resource_mut::<EmergencyShuttle>().state;
    state.no_recall = true;
    let idle_or_recalled = matches!(state.mode, ShuttleMode::Idle | ShuttleMode::Recall);
    let late = state
        .timer
        .left(now)
        .is_some_and(|left| left > config.call_secs * ALERT_COEFF_AUTOEVAC_CRITICAL);
    let call = idle_or_recalled || (state.mode == ShuttleMode::Call && late);
    announce(
        world,
        "Отправлен эвакуационный шаттл",
        "Обнаружены катастрофические потери: активированы протоколы кризисного шаттла - глушение \
         сигналов отзыва на всех частотах.",
        None,
    );
    if call {
        call_shuttle(world, "", None, ALERT_COEFF_AUTOEVAC_CRITICAL);
    }
}

/// tg `SSticker.check_finished`: the emergency shuttle reached CentCom.
pub fn endgame(world: &World) -> bool {
    world
        .get_resource::<EmergencyShuttle>()
        .is_some_and(|s| s.state.mode == ShuttleMode::Endgame)
}

/// tg `SSshuttle.emergency.is_hijacked()`.
pub fn hijacked(world: &World) -> bool {
    world
        .get_resource::<EmergencyShuttle>()
        .is_some_and(|s| s.state.is_hijacked())
}

/// `EMERGENCY_ESCAPED_OR_ENDGAMED`.
pub fn evacuated(world: &World) -> bool {
    world
        .get_resource::<EmergencyShuttle>()
        .is_some_and(|s| s.state.escaped_or_endgamed())
}

/// tg round end popcount for one living mob: `(escaped, on the shuttle)`: on CentCom (the
/// shuttle's arrival level) once evacuated, and in the shuttle's areas there.
pub fn escape_status(world: &World, mob: Entity) -> (bool, bool) {
    let Some(shuttle) = world.get_resource::<EmergencyShuttle>() else {
        return (false, false);
    };
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return (false, false);
    };
    // An escape pod docked at its own CentCom stand-in counts as CentCom.
    let escaped = shuttle.state.escaped_or_endgamed()
        && (pos.z == shuttle.centcom_z
            || pos.z == shuttle.transit_z && shuttle.state.mode == ShuttleMode::Endgame
            || crate::escape_pods::at_centcom(world, pos));
    let area = world.resource::<Areas>().area_at(pos);
    let on_shuttle = escaped
        && area.is_some_and(|a| {
            world
                .get::<MobileDock>(shuttle.port)
                .is_some_and(|m| m.areas.contains(&a))
        });
    (escaped, on_shuttle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn call_dock_ignite_escape_arrive() {
        let mut e = Emergency::default();
        assert_eq!(e.check(0.0), None, "idle without a timer");
        assert!(e.request(0.0, 600.0, ALERT_COEFF_GREEN));
        assert_eq!(e.mode, ShuttleMode::Call);
        assert_eq!(e.timer.left(0.0), Some(1200.0));
        assert_eq!(e.check(1199.0), None);
        assert_eq!(e.check(1200.0), Some(ShuttleStep::DockHome));
        e.docked_home(1200.0, false, 180.0);
        assert_eq!(e.mode, ShuttleMode::Call, "a failed docking retries");
        assert_eq!(e.check(1201.0), None);
        assert_eq!(e.check(1202.0), Some(ShuttleStep::DockHome));
        e.docked_home(1202.0, true, 180.0);
        assert_eq!(e.mode, ShuttleMode::Docked);
        assert_eq!(e.check(1371.0), None);
        assert_eq!(e.check(1372.0), None, "engines start 10 s before leaving");
        assert_eq!(e.mode, ShuttleMode::Igniting);
        assert_eq!(e.check(1381.9), None);
        assert_eq!(e.check(1382.0), Some(ShuttleStep::Launch));
        e.launched(1382.0, 120.0);
        assert_eq!(e.mode, ShuttleMode::Escape);
        assert!(e.escaped_or_endgamed());
        assert_eq!(e.check(1501.0), None);
        assert_eq!(e.check(1502.0), Some(ShuttleStep::DockAway));
        e.arrived();
        assert_eq!(e.mode, ShuttleMode::Endgame);
        assert_eq!(e.check(2000.0), None);
        assert!(!e.request(2000.0, 600.0, 2.0), "no call after the escape");
    }

    #[test]
    fn recall_takes_as_long_as_the_flight_and_only_early() {
        let mut e = Emergency::default();
        e.request(100.0, 600.0, ALERT_COEFF_GREEN);
        // Green: recallable while at least the base call time (600 s) is left.
        assert!(e.can_recall(100.0 + 599.0, 600.0, SecLevel::Green));
        assert!(!e.can_recall(100.0 + 601.0, 600.0, SecLevel::Green));
        assert!(e.cancel(400.0));
        assert_eq!(e.mode, ShuttleMode::Recall);
        assert_eq!(
            e.timer.left(400.0),
            Some(300.0),
            "flew 300 s, returns in 300 s"
        );
        assert!(!e.cancel(400.0));
        assert!(!e.can_recall(400.0, 600.0, SecLevel::Green));
        assert_eq!(e.check(699.0), None);
        assert_eq!(e.mode, ShuttleMode::Recall);
        assert_eq!(e.check(700.0), None);
        assert_eq!(e.mode, ShuttleMode::Idle);
        // A new call while recalling restarts the full call time.
        e.request(800.0, 600.0, ALERT_COEFF_GREEN);
        e.cancel(900.0);
        assert!(e.request(950.0, 600.0, ALERT_COEFF_GREEN));
        assert_eq!(e.timer.left(950.0), Some(1200.0));
        assert_eq!(e.call_count, 3);
        e.no_recall = true;
        assert!(!e.can_recall(950.0, 600.0, SecLevel::Green));
    }

    #[test]
    fn recall_window_follows_the_level() {
        let mut e = Emergency::default();
        e.request(0.0, 600.0, ALERT_COEFF_GREEN);
        // 400 s left: past green's 600 s, within blue's 300 s and red's 150 s.
        assert!(!e.can_recall(800.0, 600.0, SecLevel::Green));
        assert!(e.can_recall(800.0, 600.0, SecLevel::Blue));
        assert!(!e.can_recall(1000.0, 600.0, SecLevel::Blue));
        assert!(e.can_recall(1000.0, 600.0, SecLevel::Red));
        assert!(!e.can_recall(1100.0, 600.0, SecLevel::Gamma));
    }

    #[test]
    fn level_change_rescales_the_call_and_the_recall() {
        let mut e = Emergency::default();
        e.request(0.0, 600.0, ALERT_COEFF_GREEN);
        // Green -> red at 200 s: 1000 s left become 250 s; the 1200 s flight counts as 300 s.
        e.alert_coeff_change(200.0, SecLevel::Red.shuttle_call_time_mod());
        assert_eq!(e.timer.left(200.0), Some(250.0));
        assert_eq!(e.alert_coeff, 0.5);
        // A recall 50 s later returns after the 50 s flown at the red rate (tg's `setTimer`
        // would make it return at once).
        assert!(e.cancel(250.0));
        assert_eq!(e.timer.left(250.0), Some(100.0));
        // Red -> green during the recall: the way back doubles.
        e.alert_coeff_change(250.0, ALERT_COEFF_GREEN);
        assert_eq!(e.timer.left(250.0), Some(400.0));
        // Nothing to rescale without a timer.
        let mut idle = Emergency::default();
        idle.alert_coeff_change(0.0, 1.0);
        assert_eq!(idle.timer.left(0.0), None);
    }

    #[test]
    fn can_evac_reasons() {
        let mut e = Emergency::default();
        let refuel = e.can_evac(60.0, 1200.0).unwrap_err();
        assert!(refuel.contains("19 минут"), "{refuel}");
        assert_eq!(e.can_evac(1200.0, 1200.0), Ok(()));
        e.request(0.0, 600.0, 2.0);
        assert_eq!(
            e.can_evac(1300.0, 1200.0),
            Err("Эвакуационный шаттл уже в пути.".into())
        );
    }

    #[test]
    fn ordered_turfs_match_tg() {
        // MetaStation: the template's port at (20, 14) of 25x14, facing south.
        let size = port_size(Dir::South, 25, 14, 20, 14);
        assert_eq!(
            size,
            PortSize {
                width: 25,
                height: 14,
                dwidth: 5,
                dheight: 0
            }
        );
        let turfs = ordered_turfs(TilePos::new(20, 14, 2), Dir::South, size);
        assert_eq!(turfs.len(), 25 * 14);
        assert_eq!(turfs[0], TilePos::new(25, 14, 2));
        assert!(turfs.contains(&TilePos::new(20, 14, 2)));
        assert!(turfs.contains(&TilePos::new(1, 1, 2)));
        // The same port facing north covers the mirrored rectangle in the same order.
        let north = ordered_turfs(TilePos::new(50, 50, 1), Dir::North, size);
        assert_eq!(north[0], TilePos::new(45, 50, 1));
        assert_eq!(north[size.height as usize], TilePos::new(46, 50, 1));
        // East: x and y swap (tg cos 0, sin -1).
        let east = ordered_turfs(TilePos::new(50, 50, 1), Dir::East, size);
        assert_eq!(east[0], TilePos::new(50, 55, 1));
        assert_eq!(angle_dir(angle(Dir::South) + 90), Dir::West);
    }

    #[test]
    fn early_launch_and_hijack_stages() {
        let mut e = Emergency::default();
        assert!(!e.early_launch(0.0), "not docked");
        e.request(0.0, 600.0, 1.0);
        e.check(600.0);
        e.docked_home(600.0, true, 180.0);
        assert!(e.early_launch(610.0));
        assert_eq!(e.timer.left(610.0), Some(ENGINE_START_SECS));
        assert!(!e.early_launch(611.0), "already launching");
        assert_eq!(e.check(611.0), None);
        assert_eq!(e.mode, ShuttleMode::Igniting);
        assert!(!e.early_launch(612.0), "engines started");
        assert_eq!(e.check(620.0), Some(ShuttleStep::Launch));
        e.launched(620.0, 120.0);
        // In flight: a stage adds 30 s, the last one leaves 10 s.
        e.hijack_status = 3;
        assert_eq!(e.hijack_stage(640.0), 4);
        assert_eq!(e.timer.left(640.0), Some(130.0));
        assert!(!e.is_hijacked());
        assert_eq!(e.hijack_stage(650.0), HIJACK_COMPLETED);
        assert!(e.is_hijacked());
        assert_eq!(e.timer.left(650.0), Some(HIJACK_COMPLETION_FLIGHT_SECS));
        assert_eq!(e.hijack_stage(651.0), HIJACK_COMPLETED, "capped");
        // Docked: stages leave the timer alone.
        let mut d = Emergency {
            mode: ShuttleMode::Docked,
            ..default()
        };
        d.timer.set(0.0, 100.0);
        d.hijack_stage(10.0);
        assert_eq!(d.timer.left(10.0), Some(90.0));
    }

    #[test]
    fn roundstart_templates_from_the_map() {
        assert_eq!(
            template_id("/datum/map_template/shuttle/arrival/box"),
            Some(("arrival".into(), "arrival_box".into()))
        );
        assert_eq!(
            template_id("/datum/map_template/shuttle/mining_common/meta"),
            Some(("mining_common".into(), "mining_common_meta".into()))
        );
        assert_eq!(template_id("/datum/map_template/ruin/x"), None);
        let map = DmmMap::parse(
            r#""a" = (/turf/open/space,/area/space)
"b" = (/obj/docking_port/stationary/escape_pod,/turf/open/space,/area/space)
"c" = (/obj/docking_port/stationary{roundstart_template = /datum/map_template/shuttle/arrival/box; shuttle_id = "arrival_stationary"},/turf/open/space,/area/space)
"d" = (/obj/docking_port/stationary{shuttle_id = "x"},/turf/open/space,/area/space)

(1,1,1) = {"
bab
cdb
"}
"#,
        )
        .unwrap();
        let ports = roundstart_ports(&map);
        let found: Vec<(u32, u32, &str)> =
            ports.iter().map(|p| (p.x, p.y, p.id.as_str())).collect();
        assert_eq!(
            found,
            vec![
                (1, 1, "arrival_box"),
                (3, 1, "escape_pod_default"),
                (1, 2, "escape_pod_default"),
                (3, 2, "escape_pod_default"),
            ]
        );
    }

    #[test]
    fn mining_and_labour_docks_load_their_type_templates() {
        let map = DmmMap::parse(
            r#""a" = (/obj/docking_port/stationary/mining_home{dir = 8},/turf/open/space,/area/space)
"b" = (/obj/docking_port/stationary/mining_home/common,/turf/open/space,/area/space)
"c" = (/obj/docking_port/stationary/laborcamp_home/kilo,/turf/open/space,/area/space)

(1,1,1) = {"
abc
"}
"#,
        )
        .unwrap();
        let found: Vec<(String, String)> = roundstart_ports(&map)
            .into_iter()
            .map(|p| (p.port_id, p.id))
            .collect();
        assert_eq!(
            found,
            vec![
                ("mining".into(), "mining_delta".into()),
                ("mining_common".into(), "mining_common_meta".into()),
                ("labour".into(), "labour_kilo".into()),
            ]
        );
    }

    #[test]
    fn invert_without_length_is_a_noop() {
        let mut t = ShuttleTimer::default();
        t.invert(5.0);
        assert_eq!(t.left(5.0), None);
        t.set(0.0, 10.0);
        t.invert(20.0);
        assert_eq!(t.left(20.0), Some(0.0), "an expired timer stays expired");
    }
}
