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
//!   `emergency_away`, which ends the round (`SSticker.check_finished`). A call is recalled by the
//!   console while more than the base call time is left (green alert, `past_restriction_point`).
//! - The console ([`CommsConsoleUi`]): log in with an ID that has `ACCESS_COMMAND`, call with a
//!   reason of at least 12 characters, recall; everything checked at the moment of effect.
//! - Announcements: tg's priority announcements with BandaStation's Russian texts to every
//!   player's chat.
// shortcut: there is no CentCom level: the template's own level stands in for it, with
// `emergency_away` placed where the template put the shuttle, and the transit dock is a fixed
// space level made at round start (tg reserves one per flight with hyperspace turfs). Both have
// the shuttle's own direction, so the emergency shuttle never rotates there (tg turns it at
// CentCom and in transit); a dock of another direction turns `Facing` only.
// shortcut: no security levels (always green: call time x2), no hostile environments, ripples,
// hyperspace sounds, shuttle events, hijacks or airlock cycling on docking.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_atmos::{Atmos, GasMixture, GasOverlay, HotspotEffect, TurfAir};
use ss13_mapload::dock::{SHUTTLE_SKIPOVER, map_shuttle, shuttle_template_path};
use ss13_mapload::spawner::Rng;
use ss13_mapload::{
    Areas, DEFAULT_AREA, DEFAULT_TURF, DmAtom, DmDockingPort, DmName, DmObj, DmPower,
    MapLoadPlugin, MapLoadReport, MapSeed, ShuttleTemplates, Turf,
};
use ss13_net::chat::server::SendChat;
use ss13_net::server::ClientToken;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_shared::{Dir, Facing, TilePos};
use ss13_tgui::server::{
    AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems, UiWindow,
};
use ss13_tgui::{UiInterface as _, UiStatus};
use ss13_world::{SpatialIndex, WorldSystems};

use crate::click::{ClickResolved, ResolveClicks};
use crate::comms::{
    CALL_SHUTTLE, CommsConsoleData, CommsConsoleUi, REASON, RECALL_SHUTTLE, TOGGLE_AUTHENTICATION,
};
use crate::construction::{Baseturfs, baseturfs_of, change_turf, path_of, turf_at};
use crate::health::{DamageType, Health, Stat, apply_damage};
use crate::inventory::{IdCard, Inventory, check_access};
use crate::jobs::{PlayerJob, display_time_text};
use crate::lobby::GameState;
use crate::obj_damage::{Anchored, is_under};
use crate::outfit::IdRegistration;
use crate::power::Unpowered;
use crate::round::{NewPlayer, Ticker};

/// tg `ENGINE_START_TIME`: the docked shuttle starts its engines this long before leaving.
pub const ENGINE_START_SECS: f64 = 10.0;
/// tg `ALERT_COEFF_GREEN` (`/datum/security_level/green`): the call time multiplier.
pub const ALERT_COEFF_GREEN: f64 = 2.0;
/// tg `ALERT_COEFF_AUTOEVAC_CRITICAL`.
pub const ALERT_COEFF_AUTOEVAC_CRITICAL: f64 = 0.4;
/// tg `CALL_SHUTTLE_REASON_LENGTH`.
pub const CALL_SHUTTLE_REASON_LENGTH: usize = 12;
/// tg `ACCESS_COMMAND`, the console's `req_access`.
pub const ACCESS_COMMAND: &str = "command";
/// tg `setTimer(20)` after a failed docking.
const RETRY_SECS: f64 = 2.0;
const EMERGENCY_PORT: &str = "/obj/docking_port/mobile/emergency";
const COMMS_CONSOLE: &str = "/obj/machinery/computer/communications";
const COMMS_SYNDICATE: &str = "/obj/machinery/computer/communications/syndicate";
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
/// `SS13_SHUTTLE_CALL_SECONDS` (`emergency_call_time`, 600; green alert doubles it),
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
}

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

    /// tg `can_recall` for a non-admin at green alert: a call with more than the base call time
    /// left (`past_restriction_point`).
    pub fn can_recall(&self, now: f64, call_secs: f64) -> bool {
        self.mode == ShuttleMode::Call
            && !self.no_recall
            && self.timer.left(now).is_some_and(|left| left >= call_secs)
    }

    /// `EMERGENCY_ESCAPED_OR_ENDGAMED`.
    pub fn escaped_or_endgamed(&self) -> bool {
        matches!(self.mode, ShuttleMode::Escape | ShuttleMode::Endgame)
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
}

/// A communications console's login (tg `authorize_name`, `authorize_access`).
#[derive(Component, Clone, Debug, Default)]
pub struct CommsConsole {
    pub authorize_name: Option<String>,
    pub authorize_access: Vec<String>,
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
            app.insert_resource(ShuttleTemplates(vec![template.clone()]));
            app.insert_resource(EmergencyTemplate(template));
        }
        app.register_ui::<CommsConsoleUi>()
            .add_systems(Startup, (setup_shuttle, setup_consoles))
            .add_systems(
                Update,
                (
                    tick,
                    open_console
                        .after(WorldSystems::Movement)
                        .after(ResolveClicks)
                        .before(UiSystems::Open),
                    console_acts.in_set(UiSystems::Handle),
                    unpowered_windows.in_set(UiSystems::StatusHook),
                ),
            );
    }
}

#[derive(Resource)]
struct EmergencyTemplate(PathBuf);

fn area_path(world: &World, area: Entity) -> Option<&str> {
    world.get::<DmPower>(area).map(|p| p.path.as_str())
}

fn area_named(world: &World, path: &str) -> Option<Entity> {
    let areas = world.resource::<Areas>();
    areas
        .entities()
        .iter()
        .copied()
        .find(|&a| area_path(world, a) == Some(path))
}

fn area_name(world: &World, area: Entity) -> String {
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
    let Some((level_z, (tw, th, _))) = world.get_resource::<MapLoadReport>().and_then(|r| {
        r.maps
            .iter()
            .find(|m| m.path == template)
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

/// tg `priority_announce(text, title, sender_override = SENDER)` to every player.
pub fn announce(world: &mut World, title: &str, text: &str) {
    priority_announce(world, SENDER, title, text);
}

/// tg `priority_announce(text, title, sender_override = sender)` to every player not in the
/// lobby: the sender header, the title (when not empty) and the text.
pub fn priority_announce(world: &mut World, sender: &str, title: &str, text: &str) {
    let mut query = world.query_filtered::<Entity, (With<ClientToken>, Without<NewPlayer>)>();
    let clients: Vec<Entity> = query.iter(world).collect();
    info!("announcement: {title}: {text}");
    let mut segments = vec![ChatSegment::new(Span::BoldAnnounce, format!("{sender}\n"))];
    if !title.is_empty() {
        segments.push(ChatSegment::new(Span::Bold, format!("{title}\n")));
    }
    segments.push(ChatSegment::new(Span::Plain, text));
    world.write_message(SendChat {
        clients,
        message: ChatMessage {
            channel: ChatChannel::System,
            segments,
        },
    });
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

/// tg `SSshuttle.call_evac_shuttle` + `emergency.request`: the shuttle is called and announced.
pub fn call_shuttle(world: &mut World, reason: &str, origin: Option<String>, coeff: f64) -> bool {
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
    let text = format!(
        "Эвакуационный шаттл вызван. Ожидаемое время прибытия: {minutes} {}.{reason}{traced}",
        declension_minutes(minutes)
    );
    announce(world, "Вызван эвакуационный шаттл", &text);
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
    );
    true
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
                announce(world, "Эвакуационный шаттл прибыл", &text);
            }
        }
        ShuttleStep::Launch => {
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
            announce(world, "Произведена отстыковка эвакуационного шаттла", &text);
        }
        ShuttleStep::DockAway => {
            let result = initiate_docking(world, port, away);
            log_move(world, "dock at CentCom", Some(result));
            world.resource_mut::<EmergencyShuttle>().state.arrived();
        }
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
    let escaped = shuttle.state.escaped_or_endgamed()
        && (pos.z == shuttle.centcom_z
            || pos.z == shuttle.transit_z && shuttle.state.mode == ShuttleMode::Endgame);
    let area = world.resource::<Areas>().area_at(pos);
    let on_shuttle = escaped
        && area.is_some_and(|a| {
            world
                .get::<MobileDock>(shuttle.port)
                .is_some_and(|m| m.areas.contains(&a))
        });
    (escaped, on_shuttle)
}

fn setup_consoles(world: &mut World) {
    let mut query = world.query::<(Entity, &DmObj)>();
    let consoles: Vec<Entity> = query
        .iter(world)
        .filter(|(_, o)| is_under(&o.path, COMMS_CONSOLE) && !is_under(&o.path, COMMS_SYNDICATE))
        .map(|(e, _)| e)
        .collect();
    info!("{} communications consoles", consoles.len());
    for e in consoles {
        world.entity_mut(e).insert(CommsConsole::default());
    }
}

/// tg `attack_hand` (empty hand) or `item_interaction` with an ID card: opens the window.
fn open_console(
    mut clicks: MessageReader<ClickResolved>,
    consoles: Query<&DmName, (With<CommsConsole>, Without<Unpowered>)>,
    inventories: Query<&Inventory>,
    cards: Query<(), With<IdCard>>,
    mut open: MessageWriter<OpenUi>,
) {
    for click in clicks.read() {
        let Ok(name) = consoles.get(click.target) else {
            continue;
        };
        let Ok(inventory) = inventories.get(click.mob) else {
            continue;
        };
        if inventory
            .active_item()
            .is_none_or(|item| cards.contains(item))
        {
            open.write(OpenUi::new::<CommsConsoleUi>(
                click.client,
                click.mob,
                click.target,
                plain(&name.name),
            ));
        }
    }
}

/// An unpowered console is update-only (`can_interact` fails).
fn unpowered_windows(
    mut windows: Query<&mut UiWindow>,
    unpowered: Query<(), (With<CommsConsole>, With<Unpowered>)>,
) {
    for mut window in &mut windows {
        if window.interface == CommsConsoleUi::NAME && unpowered.contains(window.source) {
            window.cap_status(UiStatus::Update);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommsAct {
    ToggleAuthentication,
    CallShuttle(String),
    RecallShuttle,
}

type ConsoleParams = (
    Option<Res<'static, EmergencyShuttle>>,
    Res<'static, Time>,
    Res<'static, Ticker>,
    Res<'static, ShuttleConfig>,
);

impl ServerUi for CommsConsoleUi {
    type Source = &'static CommsConsole;
    type UserParam = ConsoleParams;
    type Act = CommsAct;

    fn static_data(_: &CommsConsole) {}

    fn data(
        console: &CommsConsole,
        _: Entity,
        (shuttle, time, ticker, config): &bevy::ecs::system::SystemParamItem<'_, '_, ConsoleParams>,
    ) -> CommsConsoleData {
        let now = time.elapsed_secs_f64();
        let authenticated = console.authorize_name.is_some();
        let mut data = CommsConsoleData {
            authenticated,
            can_log_out: true,
            authorize_name: console.authorize_name.clone(),
            can_recall_shuttles: true,
            call_shuttle_reason_min_length: CALL_SHUTTLE_REASON_LENGTH as u32,
            max_message_length: ss13_tgui::server::MAX_PARAM_LEN as u32,
            ..default()
        };
        match shuttle.as_deref() {
            Some(shuttle) => {
                let state = &shuttle.state;
                data.shuttle_can_evac_or_fail_reason = state
                    .can_evac(ticker.round_duration(now), config.refuel_secs)
                    .err();
                data.shuttle_called =
                    !matches!(state.mode, ShuttleMode::Idle | ShuttleMode::Recall);
                data.shuttle_recallable = state.can_recall(now, config.call_secs);
                data.shuttle_called_previously = state.call_count > 0;
                data.shuttle_last_called = state.last_call_area.clone();
            }
            None => {
                data.shuttle_can_evac_or_fail_reason =
                    Some("Эвакуационный шаттл не найден.".into());
            }
        }
        data
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<CommsAct, UiReject> {
        match action {
            TOGGLE_AUTHENTICATION => Ok(CommsAct::ToggleAuthentication),
            CALL_SHUTTLE => Ok(CommsAct::CallShuttle(
                params
                    .get(REASON)
                    .ok_or_else(|| UiReject::BadParams("reason: missing".into()))?
                    .to_owned(),
            )),
            RECALL_SHUTTLE => Ok(CommsAct::RecallShuttle),
            // shortcut: messages, status displays, alert levels, shuttle purchases,
            // announcements and the rest of the console are not ported.
            _ => Err(UiReject::UnknownAction),
        }
    }
}

/// tg `ui_act`, one exclusive pass: logins read the actor's ID, calls and recalls change the
/// shuttle; every check runs here, at the moment of effect.
fn console_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<CommsConsoleUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, CommsAct)> = acts
        .read(world.resource::<Messages<UiActed<CommsConsoleUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, console, act) in pending {
        if world.get::<Unpowered>(console).is_some() {
            continue;
        }
        let Some(state) = world.get::<CommsConsole>(console).cloned() else {
            continue;
        };
        match act {
            CommsAct::ToggleAuthentication => {
                let mut state = state;
                if state.authorize_name.is_some() {
                    state = CommsConsole::default();
                } else if let Some((access, name)) = id_card(world, actor)
                    && check_access(&[ACCESS_COMMAND.to_owned()], &[], &access)
                {
                    state.authorize_access = access;
                    state.authorize_name = Some(name);
                }
                world.entity_mut(console).insert(state);
            }
            CommsAct::CallShuttle(reason) => {
                if state.authorize_name.is_none() {
                    continue;
                }
                let reason: String = reason
                    .trim()
                    .chars()
                    .take(ss13_shared::chat::MAX_MESSAGE_LEN)
                    .collect();
                if reason.chars().count() < CALL_SHUTTLE_REASON_LENGTH {
                    continue;
                }
                request_evac(world, client, actor, &reason);
            }
            CommsAct::RecallShuttle => {
                if state.authorize_name.is_none() {
                    continue;
                }
                let now = world.resource::<Time>().elapsed_secs_f64();
                let call_secs = world.resource::<ShuttleConfig>().call_secs;
                let can = world
                    .get_resource::<EmergencyShuttle>()
                    .is_some_and(|s| s.state.can_recall(now, call_secs));
                if can {
                    let origin = actor_area(world, actor);
                    recall_shuttle(world, origin);
                    info!("{actor} recalled the emergency shuttle");
                }
            }
        }
    }
}

/// tg `get_idcard(hand_first = TRUE)`: the access and `"<name> - <assignment>"` of the first ID
/// in the active hand, the other hand, then the ID slot.
fn id_card(world: &World, mob: Entity) -> Option<(Vec<String>, String)> {
    let inventory = world.get::<Inventory>(mob)?;
    [
        inventory.hands[inventory.active],
        inventory.hands[1 - inventory.active],
        inventory.id,
    ]
    .into_iter()
    .flatten()
    .find_map(|item| {
        let item = crate::inventory::get_id(world, item)?;
        let card = world.get::<IdCard>(item)?;
        let name = world
            .get::<IdRegistration>(item)
            .map(|r| format!("{} - {}", r.registered_name, r.assignment))
            .unwrap_or_else(|| "Unknown".into());
        Some((card.access.clone(), name))
    })
}

fn actor_area(world: &World, actor: Entity) -> Option<String> {
    let pos = *world.get::<TilePos>(actor)?;
    let area = world.resource::<Areas>().area_at(pos)?;
    Some(area_name(world, area))
}

/// tg `SSshuttle.requestEvac`.
fn request_evac(world: &mut World, client: Entity, actor: Entity, reason: &str) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let round = world.resource::<Ticker>().round_duration(now);
    let refuel = world.resource::<ShuttleConfig>().refuel_secs;
    let Some(check) = world
        .get_resource::<EmergencyShuttle>()
        .map(|s| s.state.can_evac(round, refuel))
    else {
        return;
    };
    if let Err(why) = check {
        world.write_message(SendChat {
            clients: vec![client],
            message: ChatMessage::simple(ChatChannel::System, Span::Warning, why),
        });
        return;
    }
    let origin = actor_area(world, actor);
    if call_shuttle(world, reason, origin, ALERT_COEFF_GREEN) {
        info!("{actor} called the emergency shuttle: {reason}");
    }
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
        assert!(e.can_recall(100.0 + 599.0, 600.0));
        assert!(!e.can_recall(100.0 + 601.0, 600.0));
        assert!(e.cancel(400.0));
        assert_eq!(e.mode, ShuttleMode::Recall);
        assert_eq!(
            e.timer.left(400.0),
            Some(300.0),
            "flew 300 s, returns in 300 s"
        );
        assert!(!e.cancel(400.0));
        assert!(!e.can_recall(400.0, 600.0));
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
        assert!(!e.can_recall(950.0, 600.0));
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
    fn invert_without_length_is_a_noop() {
        let mut t = ShuttleTimer::default();
        t.invert(5.0);
        assert_eq!(t.left(5.0), None);
        t.set(0.0, 10.0);
        t.invert(20.0);
        assert_eq!(t.left(20.0), Some(0.0), "an expired timer stays expired");
    }
}
