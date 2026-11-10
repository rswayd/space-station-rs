//! Shuttle consoles and the ordinary shuttles they fly (server), after tg
//! `/obj/machinery/computer/shuttle` (`code/modules/shuttle/shuttle_consoles/shuttle_console.dm`)
//! and the modes of `/obj/docking_port/mobile` it drives (`mobile_port.dm`: `request`, `check`).
//!
//! - A shuttle flown by consoles has a [`NavShuttle`] on its mobile port, given by whoever loads
//!   it (the nuclear operatives' infiltrator, the escape pods). A request ignites it (5.5 s, tg
//!   `ignitionTime`), it then waits in its transit for the call time (10 s, `callTime`) and docks
//!   at the destination; a docking that fails puts it back where it left from.
//! - Consoles ([`ShuttleConsole`]): the console types of [`CONSOLES`] get their shuttle id,
//!   destinations and access; one without a shuttle id flies the shuttle it stands on
//!   (`connect_to_shuttle`; an escape pod's console also gets `<pod>_lavaland`). The window
//!   (`ShuttleConsole.jsx`) lists the destinations other than the dock the shuttle is on;
//!   "move" checks access (none once emagged), the lock, that the shuttle is idle and that the
//!   destination is one of those at the moment of the act, then requests the flight;
//!   "set_destination" picks one.
//! - Escape pods refuse a request below red alert unless their console is emagged, and launch
//!   once (`crate::escape_pods::request`).
// shortcut: no admin-controlled consoles (authorization request, `admin_controlled`), no
// illiterate random launch, no head revolutionary warning, no remote control, no `check_dock`
// (docks here never overlap), no recharge time, no `no_destination_swap` mid-flight changes (a
// flying shuttle refuses new requests), no silicon refusal of the ferry console (`attack_ai`).
// Consoles of shuttles not loaded (monastery) are not claimed here; [`ShuttleConsole::new`] and
// [`NavShuttle::new`] are the way in. Mapped shuttle lifts (`/obj/docking_port/mobile/elevator`)
// register themselves ([`register_lift_shuttles`]) and dock at once on a request.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::DmObj;
use ss13_mapload::fidelity::is_under;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};

use crate::atmos_machinery::machine_say;
use crate::click::{ClickResolved, ResolveClicks};
use crate::inventory::{IdCard, Inventory, check_access};
use crate::security_level::{SecLevel, SecurityLevel};
use crate::shuttle::{MobileDock, StationaryDock, initiate_docking, ordered_turfs};
pub use crate::shuttle_console::{
    DESTINATION, MOVE, SET_DESTINATION, SHUTTLE_ID, ShuttleConsoleData, ShuttleConsoleUi,
};
use crate::tools::tell;

/// tg `/obj/docking_port/mobile` `ignitionTime`, `callTime`.
pub const IGNITION_SECS: f64 = 5.5;
pub const CALL_SECS: f64 = 10.0;
const SHUTTLE_CONSOLE: &str = "/obj/machinery/computer/shuttle";
const POD_CONSOLE: &str = "/obj/machinery/computer/shuttle/pod";
/// `/obj/machinery/computer/shuttle/ferry` (and `ferry/request`): `allow_emag = FALSE`.
const FERRY_CONSOLE: &str = "/obj/machinery/computer/shuttle/ferry";
const WHITE_SHIP_CONSOLE: &str = "/obj/machinery/computer/shuttle/white_ship";
const DEPARTING: &str = "Shuttle departing. Please stand away from the doors.";
/// The infiltrator's `shuttle_id` (`crate::antag::nukeop::war::launch_check`).
const INFILTRATOR_ID: &str = "syndicate";

/// A console type: `(path and subtypes, shuttleId, possible_destinations, req_access)`;
/// `None` for the shuttle the console stands on.
type ConsoleKind = (
    &'static str,
    Option<&'static str>,
    &'static [&'static str],
    &'static [&'static str],
);

/// The console types claimed here, most specific first.
pub const CONSOLES: [ConsoleKind; 8] = [
    // `/obj/machinery/computer/shuttle/pod`; `_lavaland` is added when it connects.
    (POD_CONSOLE, None, &["pod_asteroid"], &[]),
    // `code/modules/mining/mine_items.dm` (`crate::mining_shuttles`).
    (
        "/obj/machinery/computer/shuttle/mining/common",
        Some("mining_common"),
        &[
            "commonmining_home",
            "lavaland_common_away",
            "landing_zone_dock",
            "mining_public",
        ],
        &[],
    ),
    (
        "/obj/machinery/computer/shuttle/mining",
        Some("mining"),
        &[
            "mining_home",
            "mining_away",
            "landing_zone_dock",
            "mining_public",
        ],
        &[],
    ),
    // `code/modules/mining/laborcamp/laborshuttle.dm`; the one-way console's
    // "already at the outpost" check is its only destination being the dock it is on.
    (
        "/obj/machinery/computer/shuttle/labor/one_way",
        Some("laborcamp"),
        &["laborcamp_away"],
        &[],
    ),
    (
        "/obj/machinery/computer/shuttle/labor",
        Some("laborcamp"),
        &["laborcamp_home", "laborcamp_away"],
        &["brig"],
    ),
    // `/obj/machinery/computer/shuttle/syndicate`.
    (
        "/obj/machinery/computer/shuttle/syndicate",
        Some("syndicate"),
        &[
            "syndicate_away",
            "syndicate_z5",
            "syndicate_ne",
            "syndicate_nw",
            "syndicate_n",
            "syndicate_se",
            "syndicate_sw",
            "syndicate_s",
            "syndicate_custom",
        ],
        &["syndicate"],
    ),
    // `shuttle_consoles/white_ship.dm` (and its bridge console); `crate::away_shuttles`.
    (
        WHITE_SHIP_CONSOLE,
        Some("whiteship"),
        &[
            "whiteship_away",
            "whiteship_home",
            "whiteship_z4",
            "whiteship_waystation",
            "whiteship_lavaland",
            "whiteship_custom",
        ],
        &[],
    ),
    // `mobile_port/variants/ferry.dm`; `crate::away_shuttles`.
    (
        FERRY_CONSOLE,
        Some("ferry"),
        &["ferry_home", "ferry_away"],
        &["cent_general"],
    ),
];

/// A mobile port's flight (tg `mode` and `timer`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Flight {
    #[default]
    Idle,
    Igniting {
        dest: Entity,
        until: f64,
    },
    Transit {
        dest: Entity,
        until: f64,
    },
}

/// A shuttle consoles fly, on its mobile port (which has a [`MobileDock`]).
#[derive(Component, Clone, Debug)]
pub struct NavShuttle {
    /// Its transit dock.
    pub transit: Entity,
    pub flight: Flight,
    pub ignition_secs: f64,
    pub call_secs: f64,
    /// The dock it left from (tg `previous`), where a failed docking puts it back.
    pub previous: Option<Entity>,
    /// `/obj/docking_port/mobile/elevator`: a request docks it at once, no ignition or transit.
    pub instant: bool,
}

impl NavShuttle {
    /// tg's mobile port defaults.
    pub fn new(transit: Entity) -> Self {
        Self {
            transit,
            flight: Flight::Idle,
            ignition_secs: IGNITION_SECS,
            call_secs: CALL_SECS,
            previous: None,
            instant: false,
        }
    }
}

/// `/obj/machinery/computer/shuttle`.
#[derive(Component, Clone, Debug, Default)]
pub struct ShuttleConsole {
    /// tg `shuttleId`; `None` until it connects to the shuttle it stands on.
    pub shuttle_id: Option<String>,
    pub possible_destinations: Vec<String>,
    pub destination: Option<String>,
    pub req_access: Vec<String>,
    /// tg `locked` (an escape pod's follows the alert level, see [`console_locked`]).
    pub locked: bool,
    /// `obj_flags & EMAGGED`.
    pub emagged: bool,
    /// `/obj/machinery/computer/shuttle/pod`.
    pub pod: bool,
    /// The ferry's `allow_emag = FALSE`: an emag does nothing.
    pub firewall: bool,
    /// tg `may_be_remote_controlled` (the white ship's).
    pub may_be_remote_controlled: bool,
    /// tg `remote_ref`: its shuttle remote (`crate::shuttle_remote`).
    pub remote: Option<Entity>,
}

impl ShuttleConsole {
    pub fn new(shuttle_id: Option<&str>, destinations: &[&str], access: &[&str]) -> Self {
        Self {
            shuttle_id: shuttle_id.map(str::to_owned),
            possible_destinations: destinations.iter().map(|d| (*d).to_owned()).collect(),
            req_access: access.iter().map(|a| (*a).to_owned()).collect(),
            ..default()
        }
    }
}

/// Why [`send_shuttle`] did not send it (tg `SHUTTLE_CONSOLE_*`), with tg's message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SendError {
    AccessDenied,
    Locked,
    InTransit,
    DestInvalid,
    /// The shuttle refused (`request`): an escape pod below red alert or already launched.
    Refused(&'static str),
    Missing,
}

impl SendError {
    pub fn text(self) -> &'static str {
        match self {
            SendError::AccessDenied => "Access denied.",
            SendError::Locked => "The console is locked.",
            SendError::InTransit => "Shuttle already in transit.",
            SendError::DestInvalid => "Invalid destination.",
            SendError::Refused(why) => why,
            SendError::Missing => "Unable to comply.",
        }
    }
}

pub struct ShuttleNavPlugin;

impl Plugin for ShuttleNavPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // An emag on the console (`emag_act`).
        app.add_click_handler(ClickPhase::HeldOnTarget, 340, |w, c| {
            !c.modifiers.right
                && c.held
                    .is_some_and(|h| emag_interaction(w, c.client, h, c.target))
        });
        app.register_ui::<ShuttleConsoleUi>()
            // After every Startup system has registered its shuttles.
            .add_systems(
                ss13_world::MapInitLate,
                (register_lift_shuttles, setup_consoles).chain(),
            )
            .add_systems(
                Update,
                (
                    console_clicks.after(ResolveClicks).before(UiSystems::Open),
                    console_acts.in_set(UiSystems::Handle),
                    tick_flights,
                ),
            );
    }
}

/// `/obj/docking_port/mobile/elevator` (`mobile_port/variants/elevator.dm`): the old
/// shuttle-mechanism lift, mapped in place.
pub const LIFT_PORT: &str = "/obj/docking_port/mobile/elevator";

/// The mapped shuttle lifts: registered where they stand (ids `elevator`, `elevator_2`, ... as
/// tg's `Check_id` counter) and flown by consoles like any shuttle, docking at once.
pub(crate) fn register_lift_shuttles(world: &mut World) {
    let mut q =
        world.query_filtered::<(Entity, &ss13_mapload::DmDockingPort), With<ss13_world::Mapped>>();
    let mut lifts: Vec<(Entity, String)> = q
        .iter(world)
        .filter(|(_, d)| is_under(&d.path, LIFT_PORT))
        .map(|(e, d)| {
            let id = if d.shuttle_id.is_empty() {
                "elevator"
            } else {
                d.shuttle_id.as_str()
            };
            (e, id.to_owned())
        })
        .collect();
    lifts.sort_by_key(|(e, _)| world.get::<TilePos>(*e).map(|p| (p.z, p.y, p.x)));
    let mut seen: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    for (port, base) in lifts {
        let n = seen.entry(base.clone()).or_default();
        *n += 1;
        let id = if *n > 1 { format!("{base}_{n}") } else { base };
        if !crate::shuttle::register_mapped_port(world, port, &id) {
            warn!("shuttle lift {port} ({id}) has no size or no shuttle area");
            continue;
        }
        let mut nav = NavShuttle::new(port);
        nav.instant = true;
        world.entity_mut(port).insert(nav);
    }
}

/// The console types on the maps, connected to the shuttles they stand on.
pub(crate) fn setup_consoles(world: &mut World) {
    let mut objs = world.query_filtered::<(Entity, &DmObj), With<ss13_world::Mapped>>();
    let found: Vec<(Entity, String)> = objs
        .iter(world)
        .filter(|(_, o)| is_under(&o.path, SHUTTLE_CONSOLE))
        .map(|(e, o)| (e, o.path.clone()))
        .collect();
    for (e, path) in found {
        let Some(&(root, id, destinations, access)) =
            CONSOLES.iter().find(|(root, ..)| is_under(&path, root))
        else {
            continue;
        };
        let mut console = ShuttleConsole::new(id, destinations, access);
        console.pod = root == POD_CONSOLE;
        console.locked = console.pod;
        console.firewall = root == FERRY_CONSOLE;
        console.may_be_remote_controlled = root == WHITE_SHIP_CONSOLE;
        if console.shuttle_id.is_none()
            && let Some(port) = world
                .get::<TilePos>(e)
                .copied()
                .and_then(|pos| containing_shuttle(world, pos))
            && let Some(id) = world.get::<MobileDock>(port).map(|m| m.id.clone())
        {
            // `/obj/machinery/computer/shuttle/pod/connect_to_shuttle`.
            if console.pod {
                console.possible_destinations.push(format!("{id}_lavaland"));
            }
            console.shuttle_id = Some(id);
        }
        world.entity_mut(e).insert(console);
    }
}

/// The mobile port with a [`NavShuttle`] whose rectangle holds `pos` (tg
/// `SSshuttle.get_containing_shuttle`).
pub fn containing_shuttle(world: &mut World, pos: TilePos) -> Option<Entity> {
    let mut q = world.query_filtered::<(Entity, &MobileDock, &TilePos), With<NavShuttle>>();
    q.iter(world)
        .find(|(_, m, p)| p.z == pos.z && ordered_turfs(**p, m.dir, m.size).contains(&pos))
        .map(|(e, ..)| e)
}

/// The port of the [`NavShuttle`] with `id` (tg `SSshuttle.getShuttle`).
pub fn get_shuttle(world: &mut World, id: &str) -> Option<Entity> {
    let mut q = world.query_filtered::<(Entity, &MobileDock), With<NavShuttle>>();
    q.iter(world).find(|(_, m)| m.id == id).map(|(e, _)| e)
}

/// The flight of the shuttle at `port`, `Idle` without one.
pub fn flight(world: &World, port: Entity) -> Flight {
    world
        .get::<NavShuttle>(port)
        .map_or(Flight::Idle, |n| n.flight)
}

/// tg `locked`, with an escape pod's `check_lock`: below red alert unless emagged.
pub fn console_locked(console: &ShuttleConsole, level: SecLevel) -> bool {
    if console.pod {
        !console.emagged && level < SecLevel::Red
    } else {
        console.locked
    }
}

fn console_clicks(
    mut clicks: MessageReader<ClickResolved>,
    consoles: Query<&ss13_mapload::DmName, With<ShuttleConsole>>,
    inventories: Query<&Inventory>,
    mut open: MessageWriter<OpenUi>,
) {
    for click in clicks.read() {
        let Ok(name) = consoles.get(click.target) else {
            continue;
        };
        if inventories
            .get(click.mob)
            .is_ok_and(|inv| inv.active_item().is_none())
        {
            open.write(OpenUi::new::<ShuttleConsoleUi>(
                click.client,
                click.mob,
                click.target,
                crate::vending::plain_name(&name.name),
            ));
        }
    }
}

/// tg `getTimerStr`.
pub fn timer_str(left: f64) -> String {
    if left > 3600.0 {
        return "--:--".into();
    }
    let left = left.max(0.0).round() as u64;
    format!("{:02}:{:02}", left / 60, left % 60)
}

/// `get_valid_destinations`: docks named by the console, not the one the shuttle is on;
/// `(dock, id, name)` sorted by id.
pub fn valid_destinations(
    world: &mut World,
    console: &ShuttleConsole,
    port: Entity,
) -> Vec<(Entity, String, String)> {
    let docked = world.get::<MobileDock>(port).and_then(|m| m.docked);
    let mut q = world.query::<(Entity, &StationaryDock)>();
    let mut out: Vec<(Entity, String, String)> = q
        .iter(world)
        .filter(|(e, d)| console.possible_destinations.contains(&d.id) && Some(*e) != docked)
        .map(|(e, d)| (e, d.id.clone(), d.name.clone()))
        .collect();
    out.sort_by(|a, b| a.1.cmp(&b.1));
    out
}

impl ServerUi for ShuttleConsoleUi {
    type Source = &'static ShuttleConsole;
    type UserParam = (
        Query<'static, 'static, (&'static MobileDock, &'static NavShuttle)>,
        Query<'static, 'static, (Entity, &'static StationaryDock)>,
        Option<Res<'static, SecurityLevel>>,
        Res<'static, Time>,
    );
    type Act = ConsoleAct;

    fn static_data(_: &ShuttleConsole) {}

    fn data(
        console: &ShuttleConsole,
        _: Entity,
        (shuttles, docks, level, time): &bevy::ecs::system::SystemParamItem<
            '_,
            '_,
            Self::UserParam,
        >,
    ) -> ShuttleConsoleData {
        let shuttle = console
            .shuttle_id
            .as_ref()
            .and_then(|id| shuttles.iter().find(|(m, _)| &m.id == id));
        let Some((mobile, nav)) = shuttle else {
            return ShuttleConsoleData {
                docked_location: "Unknown".into(),
                timer_str: "00:00".into(),
                status: "Missing".into(),
                destination: console.destination.clone(),
                ..default()
            };
        };
        let now = time.elapsed_secs_f64();
        let docked = mobile.docked;
        // `get_status_text_tgui`.
        let docked_location = docked
            .and_then(|d| docks.get(d).ok())
            .map_or("In transit".to_owned(), |(_, d)| d.name.clone());
        let (status, left) = match nav.flight {
            Flight::Idle => ("Idle", 0.0),
            Flight::Igniting { until, .. } => ("Igniting", until - now),
            Flight::Transit { until, .. } => ("In Transit", until - now),
        };
        let locked = console_locked(
            console,
            level.as_deref().map_or(SecLevel::Green, |l| l.current),
        );
        let mut locations: Vec<(String, String)> = docks
            .iter()
            .filter(|(e, d)| console.possible_destinations.contains(&d.id) && Some(*e) != docked)
            .map(|(_, d)| (d.id.clone(), d.name.clone()))
            .collect();
        locations.sort();
        let mut data = ShuttleConsoleData {
            docked_location,
            locked,
            authorization_required: false,
            timer_str: timer_str(left),
            destination: console.destination.clone(),
            status: if locked { "Locked" } else { status }.into(),
            locations,
        };
        if data.locations.len() == 1 {
            data.destination = Some(data.locations[0].0.clone());
        }
        if data.locations.is_empty() {
            data.locked = true;
            data.status = "Locked".into();
        }
        data
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<ConsoleAct, UiReject> {
        let id = |key: &str| {
            params
                .get(key)
                .filter(|v| !v.is_empty() && v.len() <= 64)
                .map(str::to_owned)
                .ok_or_else(|| UiReject::BadParams(format!("{key}: missing")))
        };
        match action {
            MOVE => Ok(ConsoleAct::Move(id(SHUTTLE_ID)?)),
            SET_DESTINATION => Ok(ConsoleAct::SetDestination(id(DESTINATION)?)),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConsoleAct {
    Move(String),
    SetDestination(String),
}

/// The access on the actor's ID (`get_idcard(hand_first = TRUE)`).
fn actor_access(world: &World, actor: Entity) -> Vec<String> {
    world
        .get::<Inventory>(actor)
        .and_then(|inv| {
            inv.id_search().find_map(|i| {
                let card = crate::inventory::get_id(world, i)?;
                world.get::<IdCard>(card).map(|c| c.access.clone())
            })
        })
        .unwrap_or_default()
}

/// tg `allowed(user)` for the console: its `req_access` (emptied by an emag).
fn allowed(world: &World, console: &ShuttleConsole, actor: Entity) -> bool {
    console.emagged || check_access(&console.req_access, &[], &actor_access(world, actor))
}

/// `send_shuttle(dest_id, user)`: every check at the moment of the act, then the request.
pub fn send_shuttle(
    world: &mut World,
    console_e: Entity,
    actor: Entity,
    dest_id: &str,
) -> Result<(), SendError> {
    let console = world
        .get::<ShuttleConsole>(console_e)
        .cloned()
        .ok_or(SendError::Missing)?;
    if !allowed(world, &console, actor) {
        return Err(SendError::AccessDenied);
    }
    let port = console
        .shuttle_id
        .as_deref()
        .and_then(|id| get_shuttle(world, id))
        .ok_or(SendError::Missing)?;
    // An escape pod's lock is its `request` check below, with its own message.
    if !console.pod && console.locked {
        return Err(SendError::Locked);
    }
    if flight(world, port) != Flight::Idle {
        return Err(SendError::InTransit);
    }
    let valid = valid_destinations(world, &console, port);
    let Some(&(dest, ..)) = valid.iter().find(|(_, id, _)| id == dest_id) else {
        warn!(
            "{actor} tried to send shuttle {port} from {console_e} to an invalid dock {dest_id:?}"
        );
        return Err(SendError::DestInvalid);
    };
    if console.pod {
        crate::escape_pods::request(world, port, dest, console.emagged)
            .map_err(SendError::Refused)?;
    }
    // `/obj/machinery/computer/shuttle/syndicate/launch_check`: not while a declared war
    // gives the station time to prepare.
    if console.shuttle_id.as_deref() == Some(INFILTRATOR_ID) {
        crate::antag::nukeop::war::launch_check(world).map_err(SendError::Refused)?;
    }
    request(world, port, dest);
    Ok(())
}

/// tg `request(dest)` of an idle shuttle: ignition towards `dest`.
pub fn request(world: &mut World, port: Entity, dest: Entity) -> bool {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let docked = world.get::<MobileDock>(port).and_then(|m| m.docked);
    let Some(mut nav) = world.get_mut::<NavShuttle>(port) else {
        return false;
    };
    if nav.flight != Flight::Idle {
        return false;
    }
    if nav.instant {
        // `/obj/docking_port/mobile/elevator/request`: `initiate_docking(S, force = TRUE)`.
        nav.previous = docked;
        let moved = initiate_docking(world, port, dest);
        info!("lift shuttle {port} docked: {moved:?}");
        return moved.is_ok();
    }
    nav.flight = Flight::Igniting {
        dest,
        until: now + nav.ignition_secs,
    };
    nav.previous = docked;
    true
}

/// `/obj/machinery/computer/shuttle/ui_act`.
fn console_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<ShuttleConsoleUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, ConsoleAct)> = acts
        .read(world.resource::<Messages<UiActed<ShuttleConsoleUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, console_e, act) in pending {
        let Some(console) = world.get::<ShuttleConsole>(console_e).cloned() else {
            continue;
        };
        if !allowed(world, &console, actor) {
            tell(world, client, Span::Danger, "Access denied.");
            continue;
        }
        match act {
            // The window's dropdown sends the dock's name (tgui maps it back to the id).
            ConsoleAct::SetDestination(value) => {
                let Some(port) = console
                    .shuttle_id
                    .as_deref()
                    .and_then(|id| get_shuttle(world, id))
                else {
                    continue;
                };
                let valid = valid_destinations(world, &console, port);
                if let Some((_, id, _)) = valid.iter().find(|(_, d, n)| *d == value || *n == value)
                    && let Some(mut c) = world.get_mut::<ShuttleConsole>(console_e)
                {
                    c.destination = Some(id.clone());
                }
            }
            ConsoleAct::Move(id) => match send_shuttle(world, console_e, actor, &id) {
                Ok(()) => {
                    machine_say(world, console_e, DEPARTING);
                    info!("{actor} sent shuttle {:?} towards {id}", console.shuttle_id);
                }
                Err(e) => tell(world, client, Span::Warning, e.text()),
            },
        }
    }
}

/// `/obj/machinery/computer/shuttle/emag_act`: no more access checks; an escape pod's console
/// also stops following the alert level.
fn emag_interaction(world: &mut World, client: Entity, held: Entity, target: Entity) -> bool {
    if world.get::<ShuttleConsole>(target).is_none() || !crate::emag::is_emag(world, held) {
        return false;
    }
    let mut console = world.get_mut::<ShuttleConsole>(target).expect("checked");
    if console.firewall {
        // `/obj/machinery/computer/shuttle/ferry/emag_act`.
        tell(world, client, Span::Warning, "firewall too powerful!");
        return true;
    }
    if console.emagged {
        return false;
    }
    console.emagged = true;
    let pod = console.pod;
    if pod {
        console.locked = false;
    }
    tell(
        world,
        client,
        Span::Notice,
        if pod {
            "alert level checking disabled"
        } else {
            "id checking system fried"
        },
    );
    true
}

/// The mobile ports' timers: ignition, transit, arrival.
fn tick_flights(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut q = world.query::<(Entity, &NavShuttle)>();
    let due: Vec<(Entity, NavShuttle)> = q
        .iter(world)
        .filter(|(_, n)| match n.flight {
            Flight::Igniting { until, .. } | Flight::Transit { until, .. } => now >= until,
            Flight::Idle => false,
        })
        .map(|(e, n)| (e, n.clone()))
        .collect();
    for (port, nav) in due {
        let next = match nav.flight {
            Flight::Igniting { dest, .. } => {
                let moved = initiate_docking(world, port, nav.transit);
                info!("shuttle {port} into transit: {moved:?}");
                if moved.is_ok() {
                    Flight::Transit {
                        dest,
                        until: now + nav.call_secs,
                    }
                } else {
                    Flight::Idle
                }
            }
            Flight::Transit { dest, .. } => {
                let moved = initiate_docking(world, port, dest);
                info!("shuttle {port} docked: {moved:?}");
                if moved.is_err()
                    && let Some(previous) = nav.previous
                {
                    // tg would keep it in transit; back where it came from rather than stranded.
                    let _ = initiate_docking(world, port, previous);
                }
                Flight::Idle
            }
            Flight::Idle => Flight::Idle,
        };
        if let Some(mut n) = world.get_mut::<NavShuttle>(port) {
            n.flight = next;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pod_console_lock_follows_red_alert_and_the_emag() {
        let mut pod = ShuttleConsole::new(None, &["pod_asteroid"], &[]);
        pod.pod = true;
        assert!(console_locked(&pod, SecLevel::Blue));
        assert!(!console_locked(&pod, SecLevel::Red));
        assert!(!console_locked(&pod, SecLevel::Delta));
        pod.emagged = true;
        assert!(!console_locked(&pod, SecLevel::Green));
        let plain = ShuttleConsole::new(Some("syndicate"), &[], &["syndicate"]);
        assert!(!console_locked(&plain, SecLevel::Green));
    }

    #[test]
    fn timer_str_follows_get_timer_str() {
        assert_eq!(timer_str(65.4), "01:05");
        assert_eq!(timer_str(-3.0), "00:00");
        assert_eq!(timer_str(4000.0), "--:--");
    }
}
