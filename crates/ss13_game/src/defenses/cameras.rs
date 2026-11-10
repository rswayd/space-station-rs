//! Building, taking apart and upgrading security cameras, and motion-sensitive cameras
//! (server), after tg `code/game/machinery/camera/camera_construction.dm`, `camera.dm`
//! (`toggle_cam`, `on_deconstruction`), `presets.dm` (`upgradeMotion`) and `motion.dm`, and
//! `/obj/item/wallframe/camera`.
//!
//! - The camera assembly (`/obj/item/wallframe/camera`) used on a wall from the next tile
//!   becomes a disabled camera frame on the user's tile facing away from the wall
//!   (`/obj/machinery/camera/autoname/deconstructed`, [`CameraFrame`] wrenched). A welder welds
//!   it (2 s) or unwelds it, two lengths of cable wire a welded one (wirecutters take them back
//!   out), a screwdriver on a wired one asks for its networks (`SS13` by default) and finishes
//!   it, switched on. A wrench detaches a wrenched frame (the assembly back in hand's reach).
//! - A finished camera: a screwdriver opens or closes its panel; with it open wirecutters
//!   switch it off or on, a multitool messes up or restores its focus (`short_range` 2), a
//!   proximity sensor adds the motion upgrade and a crowbar takes it out, a welder (10 s, 2 fuel)
//!   takes it off the wall (the assembly, the cable and the sensor drop).
//! - A motion camera ([`MotionSensor`]: the `motion` presets and upgraded cameras) notices the
//!   living mobs next to it (in a `motion_monitored` area, anywhere in its area it sees) and 3 s
//!   later raises the area's motion alarm (`ALARM_MOTION`, `crate::silicon::alarms`) with "A red
//!   light flashes"; the alarm clears once nobody is left.
//! - Upgrades go into an open camera: a gas analyzer gives x-ray ([`XRay`]: it sees through
//!   walls), a plasma sheet EMP protection (`EMP_PROTECT_ALL`), a proximity sensor the motion
//!   sensor; the presets come with them. A crowbar asks which part to take out.
//! - A camera switched on, or back from an EMP, raises its area's camera alarm
//!   (`ALARM_CAMERA`) for 10 s; an EMPed camera shows its `emp` light.
// shortcut: no notes held up to the camera, no
// damage, breaking or repairing; a camera taken apart drops the assembly at full integrity; the
// monitored areas are tg's `motion_monitored` areas by type (AI, armory, EVA, vault); tg's
// motion group only tells its first camera (`track_mob` returns after one), here every camera
// of the area watches.
// Cost: every second, per motion camera (tens on a station), one `in_radius` of the spatial
// index (1 tile, or its view range in a monitored area).

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::fidelity::{IconIndex, is_under};
use ss13_mapload::{Areas, DmAtom, DmCamera, DmItem, DmName, DmPower};
use ss13_net::server::Networked;
use ss13_shared::chat::Span;
use ss13_shared::protocol::ClickModifiers;
use ss13_shared::{Appearance, Facing, Overlay, TilePos};
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, PromptSystems};
use ss13_tgui::server::UiSystems;
use ss13_world::SpatialIndex;

use crate::construction::{Job, PanelOpen};
use crate::emp::{EmpProtection, PROTECT_ALL};
use crate::health::{Health, Stat};
use crate::medical::{name_of, visible};
use crate::silicon::alarms::{AlarmKind, AreaInfo, StationAlarms};
use crate::silicon::cameras::{Camera, XRay};
use crate::silicon::mob_cameras::MobCamera;
use crate::tools::{Actor, DoAfter, ToolKind, tell, tool_of};

pub const CAMERA_ROOT: &str = "/obj/machinery/camera";
/// `/obj/item/wallframe/camera`'s `result_path`.
pub const CAMERA_FRAME: &str = "/obj/machinery/camera/autoname/deconstructed";
pub const CAMERA_ASSEMBLY: &str = "/obj/item/wallframe/camera";
const CABLE: &str = "/obj/item/stack/cable_coil";
const PROX_SENSOR: &str = "/obj/item/assembly/prox_sensor";
const ANALYZER: &str = "/obj/item/analyzer";
const PLASMA: &str = "/obj/item/stack/sheet/mineral/plasma";
/// `triggerCameraAlarm` then `addtimer(cancelCameraAlarm, 10 SECONDS)`.
const CAMERA_ALARM_SECS: f64 = 10.0;
/// The presets that call `upgradeMotion()` in `Initialize` (the thunderdome camera is the
/// entertainment network's own).
const MOTION_PRESETS: [&str; 4] = [
    "/obj/machinery/camera/motion",
    "/obj/machinery/camera/emp_proof/motion",
    "/obj/machinery/camera/all",
    "/obj/machinery/camera/autoname/motion",
];
const THUNDERDOME: &str = "/obj/machinery/camera/motion/thunderdome";
/// Areas with `motion_monitored = TRUE` (and their subtypes).
const MONITORED_AREAS: [&str; 4] = [
    "/area/station/ai",
    "/area/station/security/armory",
    "/area/station/command/eva",
    "/area/station/command/vault",
];
/// `CAMERA_STATE_WRENCHED`, `_WELDED`, `_WIRED` (`_FINISHED` is no [`CameraFrame`]).
pub const STATE_WRENCHED: u8 = 1;
pub const STATE_WELDED: u8 = 2;
pub const STATE_WIRED: u8 = 3;
/// `use_tool(src, user, 2 SECONDS)` (frame welding), `use_tool(src, user, 100, amount = 2)`
/// (taking a finished camera off), deciseconds.
const WELD_DS: f32 = 20.0;
const UNWELD_DS: f32 = 100.0;
/// `short_range`.
const SHORT_RANGE: u8 = 2;
/// `alarm_delay` (3 s; tg's comment adds `queueAlarm`'s 3 s on top).
pub const ALARM_DELAY_SECS: f64 = 3.0;
const MOTION_TICK_SECS: f64 = 1.0;

/// tg `camera_construction_state` of an unfinished camera.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct CameraFrame(pub u8);

/// `CAMERA_UPGRADE_MOTION` with its `detectTime` (`None`: 0; set: the time it noticed someone)
/// and whether its alarm is up (`detectTime == -1`).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct MotionSensor {
    pub detect_since: Option<f64>,
    pub alarmed: bool,
}

/// A camera alarm up until this time (`cancelCameraAlarm` timer).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct CameraAlarmUntil(pub f64);

/// The crowbar's part prompt (`tgui_input_list` of `crowbar_act`): the parts offered.
#[derive(Component, Clone, Debug)]
struct PartPrompt {
    camera: Entity,
    mob: Entity,
    parts: Vec<Part>,
}

/// An upgrade part inside a camera.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Part {
    XRay,
    EmpProof,
    Motion,
}

impl Part {
    fn name(self) -> &'static str {
        match self {
            Self::XRay => "газовый анализатор",
            Self::EmpProof => "лист плазмы",
            Self::Motion => "датчик движения",
        }
    }
}

/// The camera's `initial(view_range)`, for the multitool's focus.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
struct FocusRange(u8);

/// A timed camera step ([`Job::Camera`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CameraJob {
    /// Welding or unwelding a frame in this state.
    Weld { from: u8 },
    /// Taking a finished camera off the wall.
    Detach,
}

/// The networks prompt of a wired frame (`tgui_input_text` of `screwdriver_act`).
#[derive(Component, Clone, Copy, Debug)]
struct NetworkPrompt {
    camera: Entity,
    mob: Entity,
}

pub struct CameraBuildPlugin;

impl Plugin for CameraBuildPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(ss13_world::MapInitLate, init_motion)
            .add_systems(
                Update,
                (
                    (networks_chosen, part_chosen)
                        .in_set(UiSystems::Handle)
                        .after(PromptSystems::Answer),
                    motion_sensors,
                    clear_camera_alarms,
                ),
            );
    }
}

fn init_motion(world: &mut World) {
    let mut query = world.query_filtered::<(Entity, &DmCamera), With<ss13_world::Mapped>>();
    let found: Vec<(Entity, String, f64)> = query
        .iter(world)
        .map(|(e, d)| (e, d.path.clone(), d.view_range))
        .collect();
    for (e, path, range) in found {
        world
            .entity_mut(e)
            .insert(FocusRange(range.clamp(0.0, 32.0) as u8));
        if MOTION_PRESETS.iter().any(|p| is_under(&path, p)) && !is_under(&path, THUNDERDOME) {
            world.entity_mut(e).insert(MotionSensor::default());
        }
    }
}

use crate::tools::now;

fn path_of(world: &World, e: Entity) -> String {
    crate::construction::path_of(world, e).unwrap_or_default()
}

fn is_camera(world: &World, e: Entity) -> bool {
    world.get::<Camera>(e).is_some()
}

use crate::construction::panel_open;

fn held_path(world: &World, item: Entity) -> String {
    world
        .get::<DmItem>(item)
        .map(|i| i.path.clone())
        .unwrap_or_default()
}

/// `camera/update_icon_state`, `update_overlays`: `camera` or `camera_off`, the open panel,
/// the light of an enabled camera.
pub fn refresh_look(world: &mut World, e: Entity) {
    let Some(enabled) = world.get::<Camera>(e).map(|c| c.enabled) else {
        return;
    };
    // `isXRay(TRUE)`: the `xray` sprites; `machine_stat & EMPED`: the `emp` light.
    let xray = if world.get::<XRay>(e).is_some() {
        "xray"
    } else {
        ""
    };
    let emped = world.get::<crate::emp::Emped>(e).is_some();
    let open = panel_open(world, e);
    let Some(dmi) = world.get::<DmAtom>(e).map(|a| a.icon.clone()) else {
        return;
    };
    let Some(icons) = world.get_resource::<IconIndex>() else {
        return;
    };
    let state = if enabled && !emped {
        icons.get(&dmi, &format!("{xray}camera"))
    } else {
        icons.get(&dmi, &format!("{xray}camera_off"))
    };
    let mut overlays = Vec::new();
    if open {
        overlays.extend(icons.get(&dmi, "camera_panel").map(Overlay::new));
    }
    if emped {
        overlays.extend(
            icons
                .get(&dmi, &format!("{xray}camera_emp"))
                .map(Overlay::new),
        );
    } else if enabled {
        overlays.extend(
            icons
                .get(&dmi, &format!("{xray}camera_on"))
                .map(Overlay::new),
        );
    }
    if let Some(mut look) = world.get_mut::<Appearance>(e) {
        if let Some(state) = state {
            look.icon_state = state;
        }
        look.overlays = overlays;
    }
}

/// `toggle_cam`: on or off, on or off the network, "<user> switches <camera> on/off".
pub fn toggle_cam(world: &mut World, e: Entity, user: Option<Entity>, message: bool) {
    let Some(enabled) = world.get::<Camera>(e).map(|c| !c.enabled) else {
        return;
    };
    crate::emp::machines::set_camera(world, e, enabled);
    if enabled {
        camera_alarm(world, e);
    }
    if message {
        let change = if enabled {
            "включается"
        } else {
            "отключается"
        };
        let camera = name_of(world, e);
        let text = match user {
            Some(user) => format!("{} {change} {camera}!", name_of(world, user)),
            None => format!("{camera} {change}!"),
        };
        visible(world, e, &text, &text, Span::Danger);
    }
    refresh_look(world, e);
}

/// tg `item_interaction` and the tool acts of cameras and frames, and the assembly on a wall.
/// True when the click was theirs.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
    _modifiers: ClickModifiers,
) -> bool {
    if is_under(&held_path(world, held), CAMERA_ASSEMBLY) {
        return place_assembly(world, client, mob, held, target);
    }
    if !is_camera(world, target) {
        return false;
    }
    // `item_interaction`: `ITEM_INTERACT_SKIP_TO_ATTACK` in combat mode.
    if world
        .get::<crate::combat::CombatMode>(mob)
        .is_some_and(|c| c.0)
    {
        return false;
    }
    let actor = Actor {
        client,
        mob,
        item: Some(held),
    };
    let state = world.get::<CameraFrame>(target).map(|f| f.0);
    let tool = tool_of(world, held).map(|(k, _)| k);
    let path = held_path(world, held);
    match (state, tool) {
        (Some(from @ (STATE_WRENCHED | STATE_WELDED)), Some(ToolKind::Welder)) => {
            let text = if from == STATE_WELDED {
                "Вы начинаете отваривать каркас камеры..."
            } else {
                "Вы начинаете приваривать каркас камеры..."
            };
            if crate::tools::use_tool(world, actor, target, WELD_DS, 1, weld_job(from)) {
                tell(world, client, Span::Notice, text);
            }
        }
        (Some(STATE_WRENCHED), Some(ToolKind::Wrench)) => {
            tell(
                world,
                client,
                Span::Notice,
                format!("Вы снимаете {} с места.", name_of(world, target)),
            );
            deconstruct(world, target);
        }
        (Some(STATE_WIRED), Some(ToolKind::Wirecutters)) => {
            if let Some(&pos) = world.get::<TilePos>(target) {
                crate::construction::spawn_stack(world, CABLE, pos, 2);
            }
            world.entity_mut(target).insert(CameraFrame(STATE_WELDED));
            tell(world, client, Span::Notice, "Вы перерезаете провода.");
        }
        (Some(STATE_WIRED), Some(ToolKind::Screwdriver)) => {
            let prompt = ss13_tgui::prompt::ask_world(
                world,
                Ask::new(
                    client,
                    mob,
                    "Сеть камеры",
                    "К каким сетям подключить эту камеру? Разделяйте сети запятыми, без \
                     пробелов!\nНапример: SS13,Security,Secret",
                    PromptKind::Text {
                        default: "SS13".to_owned(),
                        max_length: 42,
                        multiline: false,
                    },
                ),
            );
            world.entity_mut(prompt).insert(NetworkPrompt {
                camera: target,
                mob,
            });
        }
        (Some(STATE_WELDED), _) if is_under(&path, CABLE) => {
            if crate::tools::use_stack(world, held, 2) {
                world.entity_mut(target).insert(CameraFrame(STATE_WIRED));
                tell(
                    world,
                    client,
                    Span::Notice,
                    format!("Вы подключаете провода к {}.", name_of(world, target)),
                );
            } else {
                tell(
                    world,
                    client,
                    Span::Warning,
                    "Нужно два отрезка кабеля, чтобы подключить камеру!",
                );
            }
        }
        (Some(_), _) => return false,
        (None, Some(ToolKind::Screwdriver)) => {
            let open = !panel_open(world, target);
            world.entity_mut(target).insert(PanelOpen(open));
            let text = if open {
                "Вы откручиваете панель камеры."
            } else {
                "Вы закручиваете панель камеры."
            };
            tell(world, client, Span::Notice, text);
            refresh_look(world, target);
        }
        (None, _) if !panel_open(world, target) => return false,
        // `gas_analyzer_act`, `plasma_act`.
        (None, _) if is_under(&path, ANALYZER) || is_under(&path, PLASMA) => {
            let xray = is_under(&path, ANALYZER);
            let has = if xray {
                world.get::<XRay>(target).is_some()
            } else {
                world.get::<EmpProtection>(target).is_some()
            };
            if has {
                tell(
                    world,
                    client,
                    Span::Warning,
                    "На этой камере уже есть такое улучшение!",
                );
                return true;
            }
            if xray {
                if crate::inventory::no_drop(world, held) {
                    return true;
                }
                world.despawn(held);
                world.entity_mut(target).insert(XRay);
                crate::silicon::cameras::sync_camera(world, target);
                refresh_look(world, target);
            } else {
                if !crate::tools::use_stack(world, held, 1) {
                    return true;
                }
                world.entity_mut(target).insert(EmpProtection(PROTECT_ALL));
            }
            tell(
                world,
                client,
                Span::Notice,
                "Вы подключаете улучшение к внутренним схемам камеры.",
            );
        }
        (None, Some(ToolKind::Wirecutters)) => toggle_cam(world, target, Some(mob), true),
        (None, Some(ToolKind::Multitool)) => {
            let initial = world.get::<FocusRange>(target).map_or(7, |f| f.0);
            let restored = world
                .get::<Camera>(target)
                .is_some_and(|c| c.view_range != initial);
            let range = if restored { initial } else { SHORT_RANGE };
            set_view_range(world, target, range);
            let text = if restored {
                "Вы восстанавливаете фокус камеры."
            } else {
                "Вы сбиваете фокус камеры."
            };
            tell(world, client, Span::Notice, text);
        }
        (None, Some(ToolKind::Crowbar)) => {
            let parts = parts_of(world, target);
            if parts.is_empty() {
                return true;
            }
            let prompt = ss13_tgui::prompt::ask_world(
                world,
                Ask::new(
                    client,
                    mob,
                    "Извлечение детали",
                    "Выберите деталь для извлечения",
                    PromptKind::List {
                        items: parts.iter().map(|p| p.name().to_owned()).collect(),
                        default: None,
                    },
                ),
            );
            world.entity_mut(prompt).insert(PartPrompt {
                camera: target,
                mob,
                parts,
            });
        }
        (None, Some(ToolKind::Welder)) => {
            crate::tools::use_tool(
                world,
                actor,
                target,
                UNWELD_DS,
                2,
                Job::Camera(CameraJob::Detach),
            );
        }
        (None, _) if is_under(&path, PROX_SENSOR) => {
            if world.get::<MotionSensor>(target).is_some() {
                tell(
                    world,
                    client,
                    Span::Warning,
                    "На этой камере уже есть такое улучшение!",
                );
                return true;
            }
            world.despawn(held);
            world.entity_mut(target).insert(MotionSensor::default());
            tell(
                world,
                client,
                Span::Notice,
                "Вы подключаете датчик к внутренним схемам камеры.",
            );
        }
        (None, _) => return false,
    }
    true
}

/// The upgrade parts inside a camera (`droppable_parts`).
fn parts_of(world: &World, camera: Entity) -> Vec<Part> {
    let mut parts = Vec::new();
    if world.get::<XRay>(camera).is_some() {
        parts.push(Part::XRay);
    }
    if world.get::<EmpProtection>(camera).is_some() {
        parts.push(Part::EmpProof);
    }
    if world.get::<MotionSensor>(camera).is_some() {
        parts.push(Part::Motion);
    }
    parts
}

/// `drop_upgrade` + `removeXRay` / `removeEmpProof` / `removeMotion`.
fn drop_part(world: &mut World, camera: Entity, part: Part, pos: TilePos) {
    match part {
        Part::XRay => {
            world.entity_mut(camera).remove::<XRay>();
            crate::silicon::cameras::sync_camera(world, camera);
            refresh_look(world, camera);
            crate::construction::spawn_obj(world, ANALYZER, pos);
        }
        Part::EmpProof => {
            world.entity_mut(camera).remove::<EmpProtection>();
            crate::construction::spawn_stack(world, PLASMA, pos, 1);
        }
        Part::Motion => {
            remove_motion(world, camera);
            crate::construction::spawn_obj(world, PROX_SENSOR, pos);
        }
    }
}

/// The part picked for the crowbar, at the moment of effect: the camera still finished and
/// open with that part, the user conscious next to it (`can_perform_action`).
fn part_chosen(world: &mut World, mut answers: Local<MessageCursor<PromptAnswered>>) {
    let pending: Vec<(Entity, Entity, Option<PromptAnswer>)> = answers
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.answer.clone()))
        .collect();
    for (prompt, client, answer) in pending {
        let Some(ask) = world.get::<PartPrompt>(prompt).cloned() else {
            continue;
        };
        let Some(PromptAnswer::Item(i)) = answer else {
            continue;
        };
        let Some(&part) = ask.parts.get(i) else {
            continue;
        };
        let Some(&pos) = world.get::<TilePos>(ask.camera) else {
            continue;
        };
        if world.get::<CameraFrame>(ask.camera).is_some()
            || !panel_open(world, ask.camera)
            || !parts_of(world, ask.camera).contains(&part)
            || world.get::<Stat>(ask.mob) != Some(&Stat::Conscious)
            || !crate::storage::in_reach(world, ask.mob, ask.camera)
        {
            continue;
        }
        drop_part(world, ask.camera, part, pos);
        let text = format!(
            "Вы извлекаете {} из {}.",
            part.name(),
            name_of(world, ask.camera)
        );
        tell(world, client, Span::Notice, text);
    }
}

/// `triggerCameraAlarm` with its `cancelCameraAlarm` 10 s later.
pub fn camera_alarm(world: &mut World, camera: Entity) {
    let until = now(world) + CAMERA_ALARM_SECS;
    set_kind_alarm(world, camera, AlarmKind::Camera, true);
    world.entity_mut(camera).insert(CameraAlarmUntil(until));
}

fn clear_camera_alarms(world: &mut World) {
    let t = now(world);
    let mut q = world.query::<(Entity, &CameraAlarmUntil)>();
    let due: Vec<Entity> = q
        .iter(world)
        .filter(|(_, u)| t >= u.0)
        .map(|(e, _)| e)
        .collect();
    for camera in due {
        world.entity_mut(camera).remove::<CameraAlarmUntil>();
        set_kind_alarm(world, camera, AlarmKind::Camera, false);
    }
}

fn weld_job(from: u8) -> Job {
    Job::Camera(CameraJob::Weld { from })
}

/// `setViewRange`.
fn set_view_range(world: &mut World, e: Entity, range: u8) {
    if let Some(mut cam) = world.get_mut::<Camera>(e) {
        cam.view_range = range;
    }
}

/// `removeMotion`: the sensor out, its alarm cleared.
fn remove_motion(world: &mut World, e: Entity) {
    if let Some(sensor) = world.entity_mut(e).take::<MotionSensor>()
        && sensor.alarmed
    {
        set_alarm(world, e, false);
    }
}

/// A timed camera step ended: its state still holds, the welder's fuel is used.
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: CameraJob) {
    let camera = action.target;
    let state = world.get::<CameraFrame>(camera).map(|f| f.0);
    match job {
        CameraJob::Weld { from } if state == Some(from) => {
            if !action
                .item
                .is_none_or(|i| crate::tools::consume(world, i, action.amount))
            {
                return;
            }
            let to = if from == STATE_WELDED {
                STATE_WRENCHED
            } else {
                STATE_WELDED
            };
            world.entity_mut(camera).insert(CameraFrame(to));
            let text = if to == STATE_WELDED {
                "Каркас приварен."
            } else {
                "Каркас отварен."
            };
            tell(world, action.client, Span::Notice, text);
        }
        CameraJob::Detach if state.is_none() && panel_open(world, camera) => {
            if !action
                .item
                .is_none_or(|i| crate::tools::consume(world, i, action.amount))
            {
                return;
            }
            let (user, name) = (name_of(world, mob), name_of(world, camera));
            visible(
                world,
                mob,
                &format!("Вы отвариваете {name}, оставляя от неё лишь раму на стене."),
                &format!("{user} отваривает {name}, оставляя от неё лишь раму на стене."),
                Span::Warning,
            );
            deconstruct(world, camera);
        }
        _ => {}
    }
}

/// `on_deconstruction(disassembled = TRUE)`: the assembly, the cable of a wired camera and the
/// sensor drop; the camera leaves the network.
fn deconstruct(world: &mut World, camera: Entity) {
    let Some(&pos) = world.get::<TilePos>(camera) else {
        return;
    };
    let wired = world
        .get::<CameraFrame>(camera)
        .is_none_or(|f| f.0 >= STATE_WIRED);
    crate::construction::spawn_obj(world, CAMERA_ASSEMBLY, pos);
    if wired {
        crate::construction::spawn_stack(world, CABLE, pos, 2);
    }
    for part in parts_of(world, camera) {
        drop_part(world, camera, part, pos);
    }
    world
        .resource_mut::<crate::silicon::cameras::CameraNet>()
        .remove_camera(camera);
    world.despawn(camera);
    crate::construction::tile_changed(world, pos);
}

/// `/obj/item/wallframe/try_build` + `attack_atom` of the camera assembly: on a wall next to the
/// user (in a cardinal direction), a camera frame on the user's tile facing away from it.
fn place_assembly(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if !is_under(&path_of(world, target), "/turf/closed/wall") {
        return false;
    }
    let (Some(&wall), Some(&at)) = (world.get::<TilePos>(target), world.get::<TilePos>(mob)) else {
        return false;
    };
    let Some(dir) =
        crate::pull::dir_between(wall, at).filter(|d| ss13_shared::Dir::CARDINALS.contains(d))
    else {
        tell(
            world,
            client,
            Span::Warning,
            "Нужно стоять прямо перед стеной!",
        );
        return true;
    };
    let mut frames = world.query_filtered::<&TilePos, With<CameraFrame>>();
    if frames.iter(world).any(|p| *p == at)
        || world
            .resource::<SpatialIndex>()
            .on_tile(at)
            .iter()
            .any(|&e| is_camera(world, e) && world.get::<MobCamera>(e).is_none())
    {
        tell(world, client, Span::Warning, "Здесь уже есть камера!");
        return true;
    }
    let Some(camera) = crate::construction::spawn_obj(world, CAMERA_FRAME, at) else {
        return true;
    };
    world.despawn(held);
    let dm = world.get::<DmCamera>(camera).cloned().unwrap_or_default();
    let c_tag = autoname(world, camera, at);
    world.entity_mut(camera).insert((
        Camera {
            c_tag,
            network: dm.network.iter().map(|n| n.to_lowercase()).collect(),
            view_range: dm.view_range.clamp(0.0, 32.0) as u8,
            enabled: false,
        },
        CameraFrame(STATE_WRENCHED),
        FocusRange(dm.view_range.clamp(0.0, 32.0) as u8),
        Facing(dir),
        Networked,
    ));
    tell(
        world,
        client,
        Span::Notice,
        "Вы закрепляете каркас камеры на стене.",
    );
    refresh_look(world, camera);
    true
}

/// `/obj/machinery/camera/autoname`: `"<area> #<n>"`, numbered after the area's cameras.
fn autoname(world: &mut World, camera: Entity, at: TilePos) -> String {
    let Some(area) = world.resource::<Areas>().area_at(at) else {
        return String::new();
    };
    let mut cams = world.query_filtered::<(Entity, &TilePos), (With<Camera>, Without<MobCamera>)>();
    let count = cams
        .iter(world)
        .filter(|(e, p)| *e != camera && world.resource::<Areas>().area_at(**p) == Some(area))
        .count();
    let name = world
        .get::<DmName>(area)
        .map(|d| crate::vending::plain_name(&d.name).to_owned())
        .unwrap_or_default();
    format!("{name} #{}", count + 1)
}

/// The networks typed for a wired frame, at the moment of effect: the frame is still wired and
/// the user next to it.
fn networks_chosen(world: &mut World, mut answers: Local<MessageCursor<PromptAnswered>>) {
    let pending: Vec<(Entity, Entity, Option<PromptAnswer>)> = answers
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.answer.clone()))
        .collect();
    for (prompt, client, answer) in pending {
        let Some(ask) = world.get::<NetworkPrompt>(prompt).copied() else {
            continue;
        };
        let Some(PromptAnswer::Text(text)) = answer else {
            continue;
        };
        let near = match (
            world.get::<TilePos>(ask.camera),
            world.get::<TilePos>(ask.mob),
        ) {
            (Some(a), Some(b)) => a.chebyshev(*b).is_some_and(|d| d <= 1),
            _ => false,
        };
        if !near
            || world.get::<CameraFrame>(ask.camera) != Some(&CameraFrame(STATE_WIRED))
            || world.get::<Stat>(ask.mob) != Some(&Stat::Conscious)
        {
            continue;
        }
        let networks: Vec<String> = text
            .split(',')
            .map(|n| n.trim().to_lowercase())
            .filter(|n| !n.is_empty())
            .collect();
        if networks.is_empty() {
            tell(
                world,
                client,
                Span::Warning,
                "Сеть не найдена, повесьте трубку и попробуйте ещё раз!",
            );
            continue;
        }
        world.entity_mut(ask.camera).remove::<CameraFrame>();
        if let Some(mut cam) = world.get_mut::<Camera>(ask.camera) {
            cam.network = networks;
        }
        toggle_cam(world, ask.camera, Some(ask.mob), false);
    }
}

// ---------------------------------------------------------------------------------------------
// Motion

/// The living mobs a motion camera notices (`HasProximity` within 1 tile; in a monitored area,
/// `motion_group` targets: anyone in the area it can see).
fn motion_targets(world: &World, camera: Entity, pos: TilePos, range: u8) -> bool {
    let areas = world.resource::<Areas>();
    let area = areas.area_at(pos);
    let monitored = area
        .and_then(|a| world.get::<DmPower>(a))
        .is_some_and(|p| MONITORED_AREAS.iter().any(|r| is_under(&p.path, r)));
    let radius = if monitored {
        u32::from(range.max(1))
    } else {
        1
    };
    world
        .resource::<SpatialIndex>()
        .in_radius(pos, radius)
        .filter(|&e| e != camera)
        .filter(|&e| {
            world.get::<Health>(e).is_some()
                && world.get::<Stat>(e) != Some(&Stat::Dead)
                && world.get::<crate::silicon::ai::AiCore>(e).is_none()
        })
        .any(|e| {
            let Some(&p) = world.get::<TilePos>(e) else {
                return false;
            };
            !monitored
                || (areas.area_at(p) == area
                    && crate::mobs::ai::behaviors::can_see(world, pos, p, radius))
        })
}

/// tg `camera/process` of motion cameras, every second.
fn motion_sensors(world: &mut World, mut next: Local<f64>) {
    let t = now(world);
    if t < *next {
        return;
    }
    *next = t + MOTION_TICK_SECS;
    let mut query = world.query::<(Entity, &MotionSensor, &Camera, &TilePos)>();
    let cams: Vec<(Entity, MotionSensor, bool, u8, TilePos)> = query
        .iter(world)
        .map(|(e, s, c, &p)| (e, *s, c.enabled, c.view_range, p))
        .collect();
    for (e, sensor, enabled, range, pos) in cams {
        if world.get::<crate::emp::Emped>(e).is_some() {
            continue;
        }
        let seen = motion_targets(world, e, pos, range);
        let mut next_state = sensor;
        match (sensor.detect_since, sensor.alarmed) {
            (_, true) if !seen => {
                // `lost_target` -> `cancelAlarm`.
                if enabled {
                    set_alarm(world, e, false);
                }
                next_state = MotionSensor::default();
            }
            (Some(since), false) if t - since > ALARM_DELAY_SECS => {
                // `triggerAlarm`.
                if enabled && set_alarm(world, e, true) {
                    let text = format!("Красный огонёк мигает на {}!", name_of(world, e));
                    visible(world, e, &text, &text, Span::Warning);
                }
                next_state.alarmed = true;
            }
            (None, false) if seen => next_state.detect_since = Some(t),
            _ => {}
        }
        if next_state != sensor
            && let Some(mut s) = world.get_mut::<MotionSensor>(e)
        {
            *s = next_state;
        }
    }
}

/// `alarm_manager.send_alarm(ALARM_MOTION)` / `clear_alarm`: true when the area's alarm changed.
fn set_alarm(world: &mut World, camera: Entity, on: bool) -> bool {
    set_kind_alarm(world, camera, AlarmKind::Motion, on)
}

/// `alarm_manager.send_alarm(kind)` / `clear_alarm(kind)` of a camera.
fn set_kind_alarm(world: &mut World, camera: Entity, kind: AlarmKind, on: bool) -> bool {
    let Some(&pos) = world.get::<TilePos>(camera) else {
        return false;
    };
    let area = world.get_resource::<Areas>().and_then(|a| a.area_at(pos));
    let info = area.map(|a| area_info(world, a, pos.z));
    let Some(mut alarms) = world.get_resource_mut::<StationAlarms>() else {
        return false;
    };
    let change = alarms.set(kind, camera, area, on, |_| {
        info.unwrap_or(AreaInfo {
            name: String::new(),
            station: false,
            z: pos.z,
            cameras: Vec::new(),
        })
    });
    match change {
        Some(change) => {
            world.write_message(change);
            true
        }
        None => false,
    }
}

fn area_info(world: &mut World, area: Entity, z: u16) -> AreaInfo {
    let name = world
        .get::<DmName>(area)
        .map(|n| crate::vending::plain_name(&n.name).to_owned())
        .unwrap_or_default();
    let station = world
        .get::<DmPower>(area)
        .is_some_and(|p| is_under(&p.path, "/area/station") || is_under(&p.path, "/area/mine"));
    let mut cams = world.query_filtered::<(Entity, &TilePos), (With<Camera>, Without<MobCamera>)>();
    let cameras = cams
        .iter(world)
        .filter(|(_, p)| world.resource::<Areas>().area_at(**p) == Some(area))
        .map(|(e, _)| e)
        .collect();
    AreaInfo {
        name,
        station,
        z,
        cameras,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn motion_presets_and_monitored_areas() {
        let motion =
            |p: &str| MOTION_PRESETS.iter().any(|r| is_under(p, r)) && !is_under(p, THUNDERDOME);
        assert!(motion("/obj/machinery/camera/autoname/motion"));
        assert!(motion("/obj/machinery/camera/motion"));
        assert!(!motion("/obj/machinery/camera/motion/thunderdome"));
        assert!(!motion("/obj/machinery/camera/autoname"));
        assert!(
            MONITORED_AREAS
                .iter()
                .any(|r| is_under("/area/station/ai/satellite/interior", r))
        );
    }

    #[test]
    fn welding_toggles_between_wrenched_and_welded() {
        assert_eq!(
            weld_job(STATE_WRENCHED),
            Job::Camera(CameraJob::Weld {
                from: STATE_WRENCHED
            })
        );
    }
}
