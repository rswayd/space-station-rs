//! Airlock chamber controllers (server).
//!
//! - Access buttons and access consoles, tg `/obj/machinery/door_buttons`
//!   (`code/game/machinery/embedded_controller/access_controller.dm`): a console (`idSelf`) holds
//!   an interior and an exterior airlock (`idInterior`, `idExterior` = airlock `id_tag`); a
//!   button with the same `idSelf` works one of them (`idDoor`). Pressed by someone with the
//!   access, a button opens its closed door (cycling: when the other one is open, that one
//!   closes and bolts first, then 2 s later ours opens and bolts) or closes and bolts its open
//!   one, and rests for 2 s. The console's window (`AirlockButtonController`) does the same per
//!   door. An emag clears the access.
//! - Embedded airlock consoles, tg `/obj/machinery/airlock_controller` (`airlock_controller.dm`)
//!   with their sensors (`airlock_control.dm`): the incinerator chambers. Each machine tick the
//!   console steps its state (`inopen`, `pressurize`, `closed`, `depressurize`, `outopen`)
//!   toward the target the window (`AirlockController`) or a sensor press asked for: doors
//!   secure-closed and opened, the dual-port vent pump filling the chamber to 95% of an
//!   atmosphere or siphoning it to 5% (1% when sanitizing) by the sensor's reading. A sensor
//!   shows alert under 80% of an atmosphere.
// shortcut: no silicon access to the consoles, no NOPOWER sprites of the buttons (`_off`), no
// shuttle id_tag prefixes (`connect_to_shuttle`).

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_atmos::Atmos;
use ss13_atmos::gas::ONE_ATMOSPHERE;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmAirlockControl, DmAtmosDevice, DmDoor, DoorKind};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::airlock_controller::{
    AirlockButtonControllerData, AirlockButtonControllerUi, AirlockControllerData,
    AirlockControllerUi,
};
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};

use crate::atmos_machinery::devices::{DpVentPump, EXTERNAL_BOUND};
use crate::doors::{Door, DoorState, with_door};
use crate::power::{MachineTicks, Unpowered};
use crate::tools::tell;

use ss13_mapload::airlock_control::{
    ACCESS_BUTTON, ACCESS_CONSOLE, AIRLOCK_CONTROLLER_ROOT, AIRLOCK_SENSOR_ROOT,
};

/// `addtimer(not_busy, 2 SECONDS)`, `addtimer(cycle_open, 2 SECONDS)`.
const BUSY_SECS: f64 = 2.0;

/// An access button or console (`door_buttons`): its doors, `busy`, the access it asks for.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct AccessControl {
    pub kind: AccessKind,
    pub busy: bool,
    pub emagged: bool,
    pub req_access: Vec<String>,
    pub req_one_access: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum AccessKind {
    /// `access_button`: its airlock and its console; `busy` ends at `idle_at`.
    Button {
        door: Option<Entity>,
        controller: Option<Entity>,
        idle_at: f64,
    },
    /// `door_buttons/airlock_controller`; `cycle_open` pending: (when, the door to open).
    Console {
        interior: Option<Entity>,
        exterior: Option<Entity>,
        cycle_open: Option<(f64, Entity)>,
    },
}

/// `AIRLOCK_STATE_*`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChamberState {
    InOpen,
    Pressurize,
    #[default]
    Closed,
    Depressurize,
    OutOpen,
}

impl ChamberState {
    fn name(self) -> &'static str {
        match self {
            Self::InOpen => "inopen",
            Self::Pressurize => "pressurize",
            Self::Closed => "closed",
            Self::Depressurize => "depressurize",
            Self::OutOpen => "outopen",
        }
    }
}

/// An embedded airlock console (`/obj/machinery/airlock_controller`).
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct AirlockConsole {
    pub interior: Option<Entity>,
    pub exterior: Option<Entity>,
    pub pump: Option<Entity>,
    pub sensor: Option<Entity>,
    pub sanitize_external: bool,
    pub state: ChamberState,
    pub target: ChamberState,
}

/// An airlock sensor: the console a press cycles (`master_tag`), `alert`.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct AirlockSensor {
    pub master: Option<Entity>,
    pub alert: bool,
}

pub struct AirlockControllersPlugin;

impl Plugin for AirlockControllersPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        app.add_click_handler(ClickPhase::EmptyHand, 2703, |w, c| {
            attack_hand(w, c.client, c.mob, c.target)
        });
        // `door_buttons/attackby`: any item presses it as a hand would.
        app.add_click_handler(ClickPhase::HeldOnTarget, 2703, |w, c| {
            w.get::<AccessControl>(c.target).is_some() && attack_hand(w, c.client, c.mob, c.target)
        });
        app.register_ui::<AccessConsoleWindow>()
            .register_ui::<ChamberConsoleWindow>()
            .add_systems(ss13_world::MapInitLate, link_controls)
            .add_systems(
                Update,
                (
                    (access_console_acts, chamber_console_acts).in_set(UiSystems::Handle),
                    access_timers,
                    process_chambers,
                ),
            );
    }
}

fn find_door(world: &mut World, tag: &str) -> Option<Entity> {
    if tag.is_empty() {
        return None;
    }
    let mut q = world.query::<(Entity, &DmDoor)>();
    q.iter(world)
        .find(|(_, d)| d.kind == DoorKind::Airlock && d.id_tag == tag)
        .map(|(e, _)| e)
}

/// The first control under `root` that `f` accepts.
fn find_control(
    list: &[(Entity, DmAirlockControl)],
    root: &str,
    f: &dyn Fn(&DmAirlockControl) -> bool,
) -> Option<Entity> {
    list.iter()
        .find(|(_, dm)| is_under(&dm.path, root) && f(dm))
        .map(|&(e, _)| e)
}

/// `find_objects_by_tag` / `post_machine_initialize` of every mapped control.
fn link_controls(world: &mut World) {
    let mut q = world.query_filtered::<(Entity, &DmAirlockControl), Without<AccessControl>>();
    let controls: Vec<(Entity, DmAirlockControl)> = q
        .iter(world)
        .filter(|(e, _)| {
            world.get::<AirlockConsole>(*e).is_none() && world.get::<AirlockSensor>(*e).is_none()
        })
        .map(|(e, dm)| (e, dm.clone()))
        .collect();
    let mut pumps = world.query::<(Entity, &DmAtmosDevice, &DpVentPump)>();
    let pumps: Vec<(Entity, String)> = pumps
        .iter(world)
        .map(|(e, dm, _)| (e, dm.id_tag.clone()))
        .collect();
    for (e, dm) in &controls {
        let access = |kind| AccessControl {
            kind,
            busy: false,
            emagged: false,
            req_access: dm.req_access.clone(),
            req_one_access: dm.req_one_access.clone(),
        };
        if is_under(&dm.path, ACCESS_CONSOLE) {
            // `find_objects_by_tag`: nothing unless both ids are set.
            let both = !dm.id_interior.is_empty() && !dm.id_exterior.is_empty();
            let (interior, exterior) = if both {
                (
                    find_door(world, &dm.id_interior),
                    find_door(world, &dm.id_exterior),
                )
            } else {
                (None, None)
            };
            world.entity_mut(*e).insert(access(AccessKind::Console {
                interior,
                exterior,
                cycle_open: None,
            }));
        } else if is_under(&dm.path, ACCESS_BUTTON) {
            let controller = (!dm.id_self.is_empty())
                .then(|| find_control(&controls, ACCESS_CONSOLE, &|c| c.id_self == dm.id_self))
                .flatten();
            let door = find_door(world, &dm.id_door);
            world.entity_mut(*e).insert(access(AccessKind::Button {
                door,
                controller,
                idle_at: 0.0,
            }));
        } else if is_under(&dm.path, AIRLOCK_CONTROLLER_ROOT) {
            let tagged = |tag: &str, root: &str| {
                (!tag.is_empty())
                    .then(|| find_control(&controls, root, &|c| c.id_tag == tag))
                    .flatten()
            };
            let console = AirlockConsole {
                interior: find_door(world, &dm.interior_door_tag),
                exterior: find_door(world, &dm.exterior_door_tag),
                pump: pumps
                    .iter()
                    .find(|(_, tag)| !dm.airpump_tag.is_empty() && *tag == dm.airpump_tag)
                    .map(|&(p, _)| p),
                sensor: tagged(&dm.sensor_tag, AIRLOCK_SENSOR_ROOT),
                sanitize_external: dm.sanitize_external,
                ..default()
            };
            world.entity_mut(*e).insert(console);
        } else if is_under(&dm.path, AIRLOCK_SENSOR_ROOT) {
            let master = (!dm.master_tag.is_empty())
                .then(|| {
                    find_control(&controls, AIRLOCK_CONTROLLER_ROOT, &|c| {
                        c.id_tag == dm.master_tag
                    })
                })
                .flatten();
            world.entity_mut(*e).insert(AirlockSensor {
                master,
                alert: false,
            });
            set_state(world, *e, "airlock_sensor_standby");
        }
    }
}

fn set_state(world: &mut World, e: Entity, state: &str) {
    crate::obj_damage::set_icon_state(world, e, state);
}

/// A door that is gone (despawned, no longer a door) reads as missing.
fn live(world: &World, door: Option<Entity>) -> Option<Entity> {
    door.filter(|&d| world.get::<Door>(d).is_some())
}

/// tg `density`: shut, or not yet passable while it moves.
fn closed(world: &World, door: Entity) -> bool {
    world.get::<ss13_world::Density>(door).is_some()
}

/// Clicks with a hand (or any item) on a control.
pub fn attack_hand(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if let Some(control) = world.get::<AccessControl>(target).cloned() {
        match control.kind {
            AccessKind::Button { .. } => press_button(world, client, mob, target, &control),
            AccessKind::Console { .. } => {
                let title = crate::security::name_of(world, target);
                world.write_message(OpenUi::new::<AccessConsoleWindow>(
                    client, mob, target, title,
                ));
            }
        }
        return true;
    }
    if world.get::<AirlockConsole>(target).is_some() {
        let title = crate::security::name_of(world, target);
        world.write_message(OpenUi::new::<ChamberConsoleWindow>(
            client, mob, target, title,
        ));
        return true;
    }
    if let Some(sensor) = world.get::<AirlockSensor>(target).cloned() {
        // `airlock_sensor/attack_hand`: the console cycles.
        if let Some(master) = sensor.master
            && let Some(mut console) = world.get_mut::<AirlockConsole>(master)
        {
            console.target = if matches!(
                console.state,
                ChamberState::InOpen | ChamberState::Pressurize
            ) {
                ChamberState::OutOpen
            } else {
                ChamberState::InOpen
            };
        }
        crate::animation::flick(world, target, "airlock_sensor_cycle");
        return true;
    }
    false
}

/// `access_button/interact`.
fn press_button(world: &mut World, client: Entity, mob: Entity, button: Entity, c: &AccessControl) {
    let AccessKind::Button {
        door, controller, ..
    } = c.kind
    else {
        return;
    };
    if c.busy {
        return;
    }
    if !crate::inventory::id_allowed(world, mob, &c.req_access, &c.req_one_access) {
        tell(world, client, Span::Warning, "Доступ запрещён.");
        return;
    }
    let (Some(door), Some(controller)) = (live(world, door), controller) else {
        return;
    };
    let Some(console) = world.get::<AccessControl>(controller).cloned() else {
        return;
    };
    let AccessKind::Console {
        interior, exterior, ..
    } = console.kind
    else {
        return;
    };
    if console.busy || world.get::<Unpowered>(controller).is_some() {
        return;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    if let Some(mut b) = world.get_mut::<AccessControl>(button) {
        b.busy = true;
        if let AccessKind::Button { idle_at, .. } = &mut b.kind {
            *idle_at = now + BUSY_SECS;
        }
    }
    set_state(world, button, "access_button_cycle");
    let (interior, exterior) = (live(world, interior), live(world, exterior));
    if closed(world, door) {
        match (interior, exterior) {
            (Some(i), Some(e)) if !(closed(world, i) && closed(world, e)) => {
                cycle_close(world, controller, door);
            }
            _ => only_open(world, controller, door),
        }
    } else {
        only_close(world, controller, door);
    }
}

fn set_busy(world: &mut World, console: Entity, busy: bool) {
    if let Some(mut c) = world.get_mut::<AccessControl>(console) {
        c.busy = busy;
    }
    let state = if busy {
        "access_control_process"
    } else {
        "access_control_standby"
    };
    set_state(world, console, state);
}

/// `go_idle`.
fn go_idle(world: &mut World, console: Entity) {
    set_busy(world, console, false);
}

/// `only_open` -> `open_door`: unbolt, open, bolt.
fn only_open(world: &mut World, console: Entity, door: Entity) {
    set_busy(world, console, true);
    open_door(world, console, door);
}

/// `only_close` -> `close_door`.
fn only_close(world: &mut World, console: Entity, door: Entity) {
    set_busy(world, console, true);
    close_door(world, console, door, true);
}

/// `open_door`: a closed door is unbolted, opened and bolted again.
fn open_door(world: &mut World, console: Entity, door: Entity) {
    if closed(world, door) {
        crate::wires::set_bolts(world, door, false);
        let opened = with_door(world, door, |d, appearance, ctx| {
            let moved = ctx.open(d, appearance, false);
            (moved, moved)
        }) == Some(true);
        if opened && world.get::<Unpowered>(console).is_none() {
            crate::wires::set_bolts(world, door, true);
        }
    }
    go_idle(world, console);
}

/// `close_door`: an open door is made unsafe (it crushes), unbolted, closed and bolted. False
/// when it did not close.
fn close_door(world: &mut World, console: Entity, door: Entity, idle_after: bool) -> bool {
    if closed(world, door) {
        go_idle(world, console);
        return false;
    }
    if let Some(mut d) = world.get_mut::<Door>(door) {
        d.safe = false;
    }
    crate::wires::set_bolts(world, door, false);
    let shut = with_door(world, door, |d, appearance, ctx| {
        let moved = ctx.close(d, appearance, false);
        (moved, moved)
    }) == Some(true);
    if !shut || world.get::<Unpowered>(console).is_some() {
        go_idle(world, console);
        return false;
    }
    crate::wires::set_bolts(world, door, true);
    if idle_after {
        go_idle(world, console);
    }
    true
}

fn opposite(world: &World, console: Entity, door: Entity) -> Option<Entity> {
    match world.get::<AccessControl>(console)?.kind {
        AccessKind::Console {
            interior, exterior, ..
        } => {
            let other = if Some(door) == exterior {
                interior
            } else {
                exterior
            };
            live(world, other)
        }
        AccessKind::Button { .. } => None,
    }
}

/// `cycle_close`: the other door closes and bolts, then in 2 s `cycle_open` opens this one.
fn cycle_close(world: &mut World, console: Entity, door: Entity) {
    let Some(AccessKind::Console {
        interior, exterior, ..
    }) = world.get::<AccessControl>(console).map(|c| c.kind.clone())
    else {
        return;
    };
    let (Some(i), Some(e)) = (live(world, interior), live(world, exterior)) else {
        return;
    };
    if closed(world, i) == closed(world, e) || !closed(world, door) {
        return;
    }
    set_busy(world, console, true);
    let Some(other) = opposite(world, console, door) else {
        return go_idle(world, console);
    };
    if !close_door(world, console, other, false) {
        return go_idle(world, console);
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    // tg's `close()` sleeps through the animation before the timer starts.
    let at = now + crate::doors::Timings::of(DoorKind::Airlock).close_done + BUSY_SECS;
    if let Some(mut c) = world.get_mut::<AccessControl>(console)
        && let AccessKind::Console { cycle_open, .. } = &mut c.kind
    {
        *cycle_open = Some((at, door));
    }
}

/// `cycle_open`: the other door must still be shut and bolted.
fn cycle_open(world: &mut World, console: Entity, door: Entity) {
    if world.get::<Door>(door).is_none() {
        return go_idle(world, console);
    }
    let ok = opposite(world, console, door)
        .and_then(|o| world.get::<Door>(o))
        .is_some_and(|o| o.state == DoorState::Closed && o.locked);
    if !ok {
        return go_idle(world, console);
    }
    set_busy(world, console, true);
    open_door(world, console, door);
}

/// Buttons resting after a press (`not_busy`), consoles' pending `cycle_open`.
fn access_timers(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut q = world.query::<(Entity, &mut AccessControl)>();
    let mut idle = Vec::new();
    let mut opens = Vec::new();
    for (e, mut c) in q.iter_mut(world) {
        match &mut c.bypass_change_detection().kind {
            AccessKind::Button { idle_at, .. } if *idle_at > 0.0 && now >= *idle_at => {
                *idle_at = 0.0;
                idle.push(e);
            }
            AccessKind::Console { cycle_open, .. } => {
                if let Some((at, door)) = *cycle_open
                    && now >= at
                {
                    *cycle_open = None;
                    opens.push((e, door));
                }
            }
            _ => {}
        }
    }
    for e in idle {
        if let Some(mut c) = world.get_mut::<AccessControl>(e) {
            c.busy = false;
        }
        set_state(world, e, "access_button_standby");
    }
    for (console, door) in opens {
        cycle_open(world, console, door);
    }
}

/// `door_buttons/emag_act`: the access is gone. False when already emagged.
pub fn emag_act(world: &mut World, mob: Entity, target: Entity) -> bool {
    let Some(mut c) = world.get_mut::<AccessControl>(target) else {
        return false;
    };
    if c.emagged {
        return false;
    }
    c.emagged = true;
    c.req_access.clear();
    c.req_one_access.clear();
    if let Some(&pos) = world.get::<TilePos>(target) {
        let sound = crate::sound::PlaySound::new(pos, "sparks", 100.0, true);
        crate::sound::playsound(world, sound);
    }
    crate::balloon::balloon_alert(world, mob, target, "контроллер доступа замкнут");
    true
}

/// The access console's window.
pub struct AccessConsoleWindow;

impl UiInterface for AccessConsoleWindow {
    const NAME: &'static str = AirlockButtonControllerUi::NAME;
    type Data = AirlockButtonControllerData;
    type StaticData = ();
}

/// `open`/`close` of the interior (`true`) or exterior door.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccessConsoleAct {
    Open(bool),
    Close(bool),
}

impl ServerUi for AccessConsoleWindow {
    type Source = &'static AccessControl;
    type UserParam = Query<'static, 'static, &'static Door>;
    type Act = AccessConsoleAct;

    fn static_data(_: &AccessControl) {}

    fn data(
        control: &AccessControl,
        _: Entity,
        doors: &SystemParamItem<'_, '_, Self::UserParam>,
    ) -> AirlockButtonControllerData {
        let AccessKind::Console {
            interior, exterior, ..
        } = control.kind
        else {
            return default();
        };
        let door = |e: Option<Entity>| e.and_then(|e| doors.get(e).ok());
        let shut = |d: Option<&Door>| {
            d.is_some_and(|d| !matches!(d.state, DoorState::Open | DoorState::Opening { .. }))
        };
        AirlockButtonControllerData {
            interior_door: door(interior).is_some(),
            exterior_door: door(exterior).is_some(),
            interior_door_closed: shut(door(interior)),
            exterior_door_closed: shut(door(exterior)),
            busy: control.busy,
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<AccessConsoleAct, UiReject> {
        let interior = params.get("requested_door") == Some("interior");
        if params.get("requested_door").is_none() {
            return Err(UiReject::BadParams("requested_door: missing".into()));
        }
        match action {
            "open" => Ok(AccessConsoleAct::Open(interior)),
            "close" => Ok(AccessConsoleAct::Close(interior)),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

/// `door_buttons/airlock_controller/ui_act`, re-checked now: nothing while busy.
fn access_console_acts(
    world: &mut World,
    mut acts: Local<MessageCursor<UiActed<AccessConsoleWindow>>>,
) {
    let pending: Vec<(Entity, AccessConsoleAct)> = acts
        .read(world.resource::<Messages<UiActed<AccessConsoleWindow>>>())
        .map(|a| (a.source, a.act))
        .collect();
    for (console, act) in pending {
        let Some(c) = world.get::<AccessControl>(console).cloned() else {
            continue;
        };
        let AccessKind::Console {
            interior, exterior, ..
        } = c.kind
        else {
            continue;
        };
        if c.busy {
            continue;
        }
        let (requested, other) = match act {
            AccessConsoleAct::Open(true) | AccessConsoleAct::Close(true) => (interior, exterior),
            _ => (exterior, interior),
        };
        let Some(requested) = live(world, requested) else {
            continue;
        };
        match act {
            AccessConsoleAct::Open(_) => {
                if live(world, other).is_some_and(|o| !closed(world, o)) {
                    cycle_close(world, console, requested);
                } else {
                    only_open(world, console, requested);
                }
            }
            AccessConsoleAct::Close(_) => only_close(world, console, requested),
        }
    }
}

/// The embedded console's window.
pub struct ChamberConsoleWindow;

impl UiInterface for ChamberConsoleWindow {
    const NAME: &'static str = AirlockControllerUi::NAME;
    type Data = AirlockControllerData;
    type StaticData = ();
}

impl ServerUi for ChamberConsoleWindow {
    type Source = &'static AirlockConsole;
    type UserParam = (
        Query<'static, 'static, &'static Door>,
        Query<'static, 'static, &'static DpVentPump>,
        Query<'static, 'static, &'static TilePos>,
        Option<Res<'static, Atmos>>,
    );
    type Act = ChamberState;

    fn static_data(_: &AirlockConsole) {}

    fn data(
        console: &AirlockConsole,
        _: Entity,
        (doors, pumps, positions, atmos): &SystemParamItem<'_, '_, Self::UserParam>,
    ) -> AirlockControllerData {
        let status = |e: Option<Entity>| match e.and_then(|e| doors.get(e).ok()) {
            None => "----".to_owned(),
            Some(d) if d.state == DoorState::Open => "open".to_owned(),
            Some(_) => "closed".to_owned(),
        };
        let pump_status = match console.pump.and_then(|p| pumps.get(p).ok()) {
            None => "----",
            Some(p) if p.releasing => "release",
            Some(_) => "siphon",
        };
        let pressure = console
            .sensor
            .and_then(|s| positions.get(s).ok())
            .and_then(|&pos| atmos.as_ref()?.pressure(pos));
        AirlockControllerData {
            airlock_state: console.state.name().to_owned(),
            sensor_pressure: pressure.map(|p| p as f32),
            pump_status: pump_status.to_owned(),
            interior_status: status(console.interior),
            exterior_status: status(console.exterior),
        }
    }

    fn parse_act(action: &str, _: &UiParams) -> Result<ChamberState, UiReject> {
        match action {
            "cycleClosed" | "abort" => Ok(ChamberState::Closed),
            "cycleExterior" => Ok(ChamberState::OutOpen),
            "cycleInterior" => Ok(ChamberState::InOpen),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

fn chamber_console_acts(
    world: &mut World,
    mut acts: Local<MessageCursor<UiActed<ChamberConsoleWindow>>>,
) {
    let pending: Vec<(Entity, ChamberState)> = acts
        .read(world.resource::<Messages<UiActed<ChamberConsoleWindow>>>())
        .map(|a| (a.source, a.act))
        .collect();
    for (console, target) in pending {
        if let Some(mut c) = world.get_mut::<AirlockConsole>(console) {
            c.target = target;
        }
    }
}

/// `secure_open` / `secure_close`: unbolted, moved forced, bolted again.
fn secure(world: &mut World, door: Entity, open: bool) {
    with_door(world, door, |d, appearance, ctx| {
        d.locked = false;
        let moved = if open {
            ctx.open(d, appearance, true)
        } else {
            ctx.close(d, appearance, true)
        };
        d.locked = true;
        d.apply_look(appearance);
        ((), moved)
    });
}

/// What one console sees this tick.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Reading {
    /// Door densities (`None`: missing): closed or moving counts as dense until it is open.
    interior_dense: Option<bool>,
    exterior_dense: Option<bool>,
    pressure: Option<f64>,
    /// The pump's `on` and releasing direction.
    pump: Option<(bool, bool)>,
}

/// What the console does to the world in a step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Effect {
    SecureOpen(bool),
    SecureClose(bool),
    PumpOn(bool),
    /// Releasing (`true`, with the external bound) or siphoning (without).
    PumpDirection(bool),
}

/// `airlock_controller/process`: steps `state` toward `target` with what it reads, returning
/// the effects (doors: `true` interior). A missing door, pump or sensor stops the step
/// (`continue` in tg ends the loop).
fn step(state: &mut ChamberState, target: ChamberState, sanitize: bool, r: Reading) -> Vec<Effect> {
    use ChamberState::*;
    let mut out = Vec::new();
    loop {
        match *state {
            InOpen => {
                if target != InOpen {
                    let Some(dense) = r.interior_dense else { break };
                    if dense {
                        *state = Closed;
                        continue;
                    }
                    out.push(Effect::SecureClose(true));
                } else if r.pump.is_some_and(|(on, _)| on) {
                    out.push(Effect::PumpOn(false));
                }
            }
            Pressurize => {
                if target == InOpen {
                    let Some(p) = r.pressure else { break };
                    if p >= ONE_ATMOSPHERE * 0.95 {
                        let Some(dense) = r.interior_dense else { break };
                        if dense {
                            out.push(Effect::SecureOpen(true));
                        } else {
                            *state = InOpen;
                            continue;
                        }
                    } else {
                        let Some((on, releasing)) = r.pump else { break };
                        if !releasing {
                            out.push(Effect::PumpDirection(true));
                        } else if !on {
                            out.push(Effect::PumpOn(true));
                        }
                    }
                } else {
                    *state = Closed;
                    continue;
                }
            }
            Closed => {
                if target == OutOpen {
                    let Some(dense) = r.interior_dense else { break };
                    if dense {
                        *state = Depressurize;
                        continue;
                    }
                    out.push(Effect::SecureClose(true));
                } else if target == InOpen {
                    let Some(dense) = r.exterior_dense else { break };
                    if dense {
                        *state = Pressurize;
                        continue;
                    }
                    out.push(Effect::SecureClose(false));
                } else {
                    let Some((on, _)) = r.pump else { break };
                    if !on {
                        out.push(Effect::PumpOn(true));
                    }
                }
            }
            Depressurize => {
                let goal = ONE_ATMOSPHERE * if sanitize { 0.01 } else { 0.05 };
                let Some(p) = r.pressure else { break };
                if p <= goal {
                    if target == OutOpen {
                        let Some(dense) = r.exterior_dense else { break };
                        if dense {
                            out.push(Effect::SecureOpen(false));
                        } else {
                            *state = OutOpen;
                        }
                    } else {
                        *state = Closed;
                        continue;
                    }
                } else if target != OutOpen && !sanitize {
                    *state = Closed;
                    continue;
                } else {
                    let Some((on, releasing)) = r.pump else { break };
                    if releasing {
                        out.push(Effect::PumpDirection(false));
                    } else if !on {
                        out.push(Effect::PumpOn(true));
                    }
                }
            }
            OutOpen => {
                if target != OutOpen {
                    let Some(dense) = r.exterior_dense else { break };
                    if dense {
                        *state = if sanitize { Depressurize } else { Closed };
                        continue;
                    }
                    out.push(Effect::SecureClose(false));
                } else {
                    let Some((on, _)) = r.pump else { break };
                    if on {
                        out.push(Effect::PumpOn(false));
                    }
                }
            }
        }
        break;
    }
    out
}

/// Every machine tick: sensors read their tile, consoles step.
fn process_chambers(world: &mut World, mut last: Local<Option<u64>>) {
    let ticks = world.resource::<MachineTicks>().0;
    let from = last.unwrap_or(ticks);
    *last = Some(ticks);
    if from == ticks {
        return;
    }
    let pressure_at = |world: &World, e: Option<Entity>| {
        let pos = *world.get::<TilePos>(e?)?;
        world.get_resource::<Atmos>()?.pressure(pos)
    };
    let mut sensors = world.query_filtered::<Entity, With<AirlockSensor>>();
    let sensors: Vec<Entity> = sensors.iter(world).collect();
    for s in sensors {
        let alert = pressure_at(world, Some(s)).is_some_and(|p| p < ONE_ATMOSPHERE * 0.8);
        if let Some(mut sensor) = world.get_mut::<AirlockSensor>(s)
            && sensor.alert != alert
        {
            sensor.alert = alert;
        }
        let state = if alert {
            "airlock_sensor_alert"
        } else {
            "airlock_sensor_standby"
        };
        set_state(world, s, state);
    }
    let mut consoles = world.query::<(Entity, &AirlockConsole)>();
    let consoles: Vec<(Entity, AirlockConsole)> =
        consoles.iter(world).map(|(e, c)| (e, c.clone())).collect();
    for (e, mut c) in consoles {
        let dense = |world: &World, d: Option<Entity>| {
            let d = d.filter(|&d| world.get::<Door>(d).is_some())?;
            Some(closed(world, d))
        };
        let reading = Reading {
            interior_dense: dense(world, c.interior),
            exterior_dense: dense(world, c.exterior),
            pressure: pressure_at(world, c.sensor),
            pump: c
                .pump
                .and_then(|p| world.get::<DpVentPump>(p))
                .map(|p| (p.on, p.releasing)),
        };
        let effects = step(&mut c.state, c.target, c.sanitize_external, reading);
        for effect in effects {
            match effect {
                Effect::SecureOpen(interior) | Effect::SecureClose(interior) => {
                    let door = if interior { c.interior } else { c.exterior };
                    if let Some(door) = door {
                        secure(world, door, matches!(effect, Effect::SecureOpen(_)));
                    }
                }
                Effect::PumpOn(on) => {
                    if let Some(mut p) = c.pump.and_then(|p| world.get_mut::<DpVentPump>(p)) {
                        p.on = on;
                    }
                }
                Effect::PumpDirection(release) => {
                    if let Some(mut p) = c.pump.and_then(|p| world.get_mut::<DpVentPump>(p)) {
                        p.releasing = release;
                        if release {
                            p.pressure_checks |= EXTERNAL_BOUND;
                        } else {
                            p.pressure_checks &= !EXTERNAL_BOUND;
                        }
                    }
                }
            }
        }
        let processing = c.state != c.target;
        let changed = world.get::<AirlockConsole>(e) != Some(&c);
        if changed {
            world.entity_mut(e).insert(c);
        }
        let look = if processing {
            "airlock_control_process"
        } else {
            "airlock_control_standby"
        };
        set_state(world, e, look);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ChamberState::*;

    fn reading(interior: bool, exterior: bool, pressure: f64, pump: (bool, bool)) -> Reading {
        Reading {
            interior_dense: Some(interior),
            exterior_dense: Some(exterior),
            pressure: Some(pressure),
            pump: Some(pump),
        }
    }

    /// From rest toward the exterior: the chamber siphons, then opens outside once at 1%.
    #[test]
    fn cycle_out_siphons_then_opens() {
        let mut state = Closed;
        let fx = step(
            &mut state,
            OutOpen,
            true,
            reading(true, true, 101.3, (true, true)),
        );
        assert_eq!(state, Depressurize);
        assert_eq!(fx, [Effect::PumpDirection(false)]);
        let fx = step(
            &mut state,
            OutOpen,
            true,
            reading(true, true, 0.5, (true, false)),
        );
        assert_eq!(fx, [Effect::SecureOpen(false)]);
        assert_eq!(state, Depressurize);
        let fx = step(
            &mut state,
            OutOpen,
            true,
            reading(true, false, 0.5, (true, false)),
        );
        assert!(fx.is_empty());
        assert_eq!(state, OutOpen);
        let fx = step(
            &mut state,
            OutOpen,
            true,
            reading(true, false, 0.5, (true, false)),
        );
        assert_eq!(fx, [Effect::PumpOn(false)]);
    }

    /// Back inside: the exterior closes, the sanitizing chamber goes through depressurize
    /// to rest, then pressurizes and opens the interior.
    #[test]
    fn cycle_in_closes_then_fills() {
        let mut state = OutOpen;
        let fx = step(
            &mut state,
            InOpen,
            true,
            reading(true, false, 0.5, (false, false)),
        );
        assert_eq!(fx, [Effect::SecureClose(false)]);
        let fx = step(
            &mut state,
            InOpen,
            true,
            reading(true, true, 0.5, (false, false)),
        );
        assert_eq!(state, Pressurize);
        assert_eq!(fx, [Effect::PumpDirection(true)]);
        let fx = step(
            &mut state,
            InOpen,
            true,
            reading(true, true, 99.0, (true, true)),
        );
        assert_eq!(fx, [Effect::SecureOpen(true)]);
        let fx = step(
            &mut state,
            InOpen,
            true,
            reading(false, true, 99.0, (true, true)),
        );
        assert_eq!(state, InOpen);
        assert_eq!(fx, [Effect::PumpOn(false)]);
    }

    #[test]
    fn missing_sensor_stops_the_step() {
        let mut state = Depressurize;
        let r = Reading {
            pressure: None,
            ..reading(true, true, 0.0, (true, false))
        };
        assert!(step(&mut state, OutOpen, true, r).is_empty());
        assert_eq!(state, Depressurize);
    }
}
