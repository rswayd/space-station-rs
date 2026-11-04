//! Station alarms (server), after tg `code/datums/alarm.dm` (`/datum/alarm_handler`,
//! `/datum/alarm_listener`), `code/datums/station_alert.dm`, the station alert console
//! (`code/game/machinery/computer/station_alert.dm`) and the silicons' `queueAlarm` /
//! `show_alarms` (`silicon.dm`), `alarm_triggered` / `alarm_cleared` (`ai.dm`, `robot.dm`).
//!
//! - [`StationAlarms`] is every handler's `sent_alarms` at once: per alarm type and area, the
//!   sources alarming it. An area is alarmed while it has a source (tg listeners count the
//!   handlers per area); its first source raises and its last one clears an [`AlarmChanged`].
//! - Sources follow their state: an air alarm in danger (`ALARM_ATMOS`), an area with an active
//!   fire alarm (`ALARM_FIRE`, the area's own handler: fire alarms and firelocks), an APC with
//!   its `ALARM_POWER` (`crate::power::Apc::power_alarm`).
//! - An [`AlertListener`] (the station alert console, the AI's eye, a cyborg) lists the alarms
//!   of its types in its scope (station and mining areas, or one z-level) in the
//!   `StationAlertConsole` window. The AI's window shows each area's cameras and sources, and
//!   picking an area jumps its eye to one of the area's cameras (a list when several).
//! - The AI and cyborgs are told in chat (tg `queueAlarm`): changes are gathered for 3 seconds,
//!   then shown one by one, or as counts per type past 4 raised / 2 cleared lines.
// shortcut: no camera, motion or burglar alarms: cameras never break or detect motion here and
// display cases are not ported; their types are listed (nominal) as in tg.
// shortcut: listeners see every current alarm, not only those raised after they were created
// (tg's own comment doubts that this is a feature); a broken console or a dead cyborg keeps
// updating (`prevent_alarm_changes`); no `[Show Alerts]` / camera links in the chat lines.

use std::collections::{BTreeMap, HashMap};

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::{Areas, DmAtom, DmName, DmPower, IconIndex};
use ss13_net::chat::server::SendChat;
use ss13_shared::chat::{ChatChannel, ChatMessage, Span};
use ss13_shared::{Appearance, Overlay, TilePos};
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, PromptSystems};
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};

use super::ai::{AiCore, AiEye};
use super::cameras::Camera;
use super::ui::{AlarmAlert, AlarmCategory, SELECT_CAMERA, StationAlertData, StationAlertUi};
use crate::atmos_machinery::alarm::{ALERT_NONE, AirAlarm};
use crate::health::Stat;
use crate::obj_damage::is_under;
use crate::power::{Apc, AreaPower, Unpowered};
use crate::server::PlayerMob;

/// `/obj/machinery/computer/station_alert`.
pub const STATION_ALERT_CONSOLE: &str = "/obj/machinery/computer/station_alert";
/// `silicon/queueAlarm`'s `addtimer(show_alarms, 3 SECONDS)`.
const SHOW_DELAY: f64 = 3.0;
/// The console's screen with alarms (`update_overlays`).
const ALERT_SCREEN: &str = "alert:2";

/// tg `ALARM_*`, in the alert window's sorting order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AlarmKind {
    Fire,
    Atmosphere,
    Power,
    Burglar,
    Motion,
    Camera,
}

impl AlarmKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Fire => "Fire",
            Self::Atmosphere => "Atmosphere",
            Self::Power => "Power",
            Self::Burglar => "Burglar",
            Self::Motion => "Motion",
            Self::Camera => "Camera",
        }
    }

    /// What silicons listen to (tg AI and robot `alert_control`).
    pub const SILICON: [Self; 6] = [
        Self::Atmosphere,
        Self::Fire,
        Self::Power,
        Self::Camera,
        Self::Burglar,
        Self::Motion,
    ];
    /// The station alert console's.
    pub const CONSOLE: [Self; 3] = [Self::Atmosphere, Self::Fire, Self::Power];
}

/// An alarmed area of one type.
#[derive(Clone, Debug, PartialEq)]
pub struct ActiveAlarm {
    pub sources: Vec<Entity>,
    /// The area's cameras when it was raised (tg passes `source_area.cameras`).
    pub cameras: Vec<Entity>,
    pub name: String,
    /// A station or mining area (tg `GLOB.the_station_areas + /area/mine`).
    pub station: bool,
    pub z: u16,
}

/// Every alarm handler's sent alarms.
#[derive(Resource, Default, Debug)]
pub struct StationAlarms {
    /// (type, source) -> the area it alarms.
    sent: HashMap<(AlarmKind, Entity), Entity>,
    pub active: BTreeMap<(AlarmKind, Entity), ActiveAlarm>,
}

/// What the area's alarm needs when it is raised.
pub struct AreaInfo {
    pub name: String,
    pub station: bool,
    pub z: u16,
    pub cameras: Vec<Entity>,
}

impl StationAlarms {
    /// tg `send_alarm` (`on`) / `clear_alarm` of `source`'s handler: the change when the area
    /// became alarmed or its last source cleared.
    pub fn set(
        &mut self,
        kind: AlarmKind,
        source: Entity,
        area: Option<Entity>,
        on: bool,
        info: impl FnOnce(Entity) -> AreaInfo,
    ) -> Option<AlarmChanged> {
        let sent = self.sent.get(&(kind, source)).copied();
        match (on, area) {
            (true, Some(area)) if sent == Some(area) => None,
            (true, Some(area)) => {
                // shortcut: a source moving to another area clears the old one silently;
                // sources never move here.
                if let Some(old) = sent {
                    self.clear(kind, source, old);
                }
                self.sent.insert((kind, source), area);
                match self.active.get_mut(&(kind, area)) {
                    Some(alarm) => {
                        alarm.sources.push(source);
                        None
                    }
                    None => {
                        let i = info(area);
                        let changed = AlarmChanged {
                            kind,
                            area,
                            raised: true,
                            station: i.station,
                            z: i.z,
                        };
                        self.active.insert(
                            (kind, area),
                            ActiveAlarm {
                                sources: vec![source],
                                cameras: i.cameras,
                                name: i.name,
                                station: i.station,
                                z: i.z,
                            },
                        );
                        Some(changed)
                    }
                }
            }
            _ => self.clear(kind, source, sent?),
        }
    }

    fn clear(&mut self, kind: AlarmKind, source: Entity, area: Entity) -> Option<AlarmChanged> {
        self.sent.remove(&(kind, source));
        let alarm = self.active.get_mut(&(kind, area))?;
        alarm.sources.retain(|&s| s != source);
        if !alarm.sources.is_empty() {
            return None;
        }
        let alarm = self.active.remove(&(kind, area))?;
        Some(AlarmChanged {
            kind,
            area,
            raised: false,
            station: alarm.station,
            z: alarm.z,
        })
    }

    /// The alarms `listener` lists, by type.
    pub fn listed<'a>(
        &'a self,
        listener: &'a AlertListener,
    ) -> impl Iterator<Item = (AlarmKind, Entity, &'a ActiveAlarm)> + 'a {
        self.active
            .iter()
            .filter(|((kind, _), alarm)| listener.kinds.contains(kind) && listener.hears(alarm))
            .map(|(&(kind, area), alarm)| (kind, area, alarm))
    }
}

/// Which alarms a listener takes (tg `allowed_z_levels`, `allowed_areas`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlertScope {
    Station,
    Level(u16),
}

/// tg `/datum/station_alert`: on its window's source.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct AlertListener {
    pub kinds: Vec<AlarmKind>,
    pub scope: AlertScope,
    pub camera_view: bool,
}

impl AlertListener {
    pub fn silicon(camera_view: bool) -> Self {
        Self {
            kinds: AlarmKind::SILICON.to_vec(),
            scope: AlertScope::Station,
            camera_view,
        }
    }

    pub fn hears(&self, alarm: &ActiveAlarm) -> bool {
        match self.scope {
            AlertScope::Station => alarm.station,
            AlertScope::Level(z) => alarm.z == z,
        }
    }
}

/// An area's first source raised (`raised`) or its last one cleared an alarm; where the area
/// is, for the listeners' scopes.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct AlarmChanged {
    pub kind: AlarmKind,
    pub area: Entity,
    pub raised: bool,
    pub station: bool,
    pub z: u16,
}

/// tg `queueAlarm`'s pending lines and counts, on a silicon listener.
#[derive(Component, Clone, Debug, Default)]
pub struct AlarmQueue {
    show: Vec<String>,
    clear: Vec<String>,
    show_counts: BTreeMap<AlarmKind, u32>,
    clear_counts: BTreeMap<AlarmKind, u32>,
    due: Option<f64>,
}

/// On the station alert console.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct StationAlertConsole;

pub struct AlarmsPlugin;

impl Plugin for AlarmsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<StationAlarms>()
            .add_message::<AlarmChanged>()
            .register_ui::<StationAlertWindow>()
            .add_systems(PostStartup, init_consoles)
            .add_systems(
                Update,
                (
                    (sync_sources, queue_alarms, show_alarms, console_screens).chain(),
                    select_camera.in_set(UiSystems::Handle),
                    camera_chosen
                        .in_set(UiSystems::Handle)
                        .after(PromptSystems::Answer),
                ),
            );
    }
}

fn area_info(
    world_areas: &Query<(Option<&DmName>, Option<&DmPower>)>,
    cameras: &Query<(Entity, &TilePos), With<Camera>>,
    areas: Option<&Areas>,
    area: Entity,
    z: u16,
) -> AreaInfo {
    let (name, path) = world_areas.get(area).unwrap_or((None, None));
    AreaInfo {
        name: name.map_or_else(String::new, |n| {
            crate::vending::plain_name(&n.name).to_owned()
        }),
        station: path
            .is_some_and(|p| is_under(&p.path, "/area/station") || is_under(&p.path, "/area/mine")),
        z,
        // Cost: every camera per raised area (a few hundred position lookups, rarely).
        cameras: cameras
            .iter()
            .filter(|(_, p)| areas.and_then(|a| a.area_at(**p)) == Some(area))
            .map(|(e, _)| e)
            .collect(),
    }
}

/// The sources' handlers follow their state.
// Cost: only changed air alarms, areas and APCs (each APC changes every machine tick: ~150
// comparisons per 2 s).
fn sync_sources(
    mut alarms: ResMut<StationAlarms>,
    air: Query<(Entity, &AirAlarm, &TilePos), Changed<AirAlarm>>,
    fire: Query<(Entity, &AreaPower), Changed<AreaPower>>,
    apcs: Query<(Entity, &Apc, &TilePos), Changed<Apc>>,
    positions: Query<&TilePos>,
    mut gone_air: RemovedComponents<AirAlarm>,
    mut gone_apcs: RemovedComponents<Apc>,
    world_areas: Query<(Option<&DmName>, Option<&DmPower>)>,
    cameras: Query<(Entity, &TilePos), With<Camera>>,
    areas: Option<Res<Areas>>,
    mut changed: MessageWriter<AlarmChanged>,
) {
    let mut updates: Vec<(AlarmKind, Entity, Option<Entity>, bool, u16)> = Vec::new();
    for (e, alarm, pos) in &air {
        updates.push((
            AlarmKind::Atmosphere,
            e,
            alarm.area,
            alarm.danger_level != ALERT_NONE,
            pos.z,
        ));
    }
    for (area, power) in &fire {
        let z = power
            .apc
            .and_then(|a| positions.get(a).ok())
            .map_or(0, |p| p.z);
        updates.push((AlarmKind::Fire, area, Some(area), power.fire, z));
    }
    for (e, apc, pos) in &apcs {
        updates.push((AlarmKind::Power, e, Some(apc.area), apc.power_alarm, pos.z));
    }
    for e in gone_air.read() {
        updates.push((AlarmKind::Atmosphere, e, None, false, 0));
    }
    for e in gone_apcs.read() {
        updates.push((AlarmKind::Power, e, None, false, 0));
    }
    for (kind, source, area, on, z) in updates {
        let info = |area| area_info(&world_areas, &cameras, areas.as_deref(), area, z);
        if let Some(change) = alarms.set(kind, source, area, on, info) {
            changed.write(change);
        }
    }
}

/// tg AI `alarm_triggered` / robot `alarm_triggered` and `alarm_cleared` -> `queueAlarm`.
fn queue_alarms(
    mut changed: MessageReader<AlarmChanged>,
    alarms: Res<StationAlarms>,
    mut listeners: Query<(&AlertListener, &mut AlarmQueue)>,
    cameras: Query<&Camera>,
    names: Query<&DmName>,
    time: Res<Time>,
) {
    let now = time.elapsed_secs_f64();
    for &AlarmChanged {
        kind,
        area,
        raised,
        station,
        z,
    } in changed.read()
    {
        let alarm = alarms.active.get(&(kind, area));
        let name = alarm.map_or_else(
            || {
                names.get(area).map_or_else(
                    |_| String::new(),
                    |n| crate::vending::plain_name(&n.name).to_owned(),
                )
            },
            |a| a.name.clone(),
        );
        for (listener, mut queue) in &mut listeners {
            let in_scope = match listener.scope {
                AlertScope::Station => station,
                AlertScope::Level(level) => level == z,
            };
            if !listener.kinds.contains(&kind) || !in_scope {
                continue;
            }
            if raised {
                let Some(alarm) = alarm.filter(|a| listener.hears(a)) else {
                    continue;
                };
                let line = if !listener.camera_view {
                    format!("--- {} alarm detected in {name}!", kind.name())
                } else {
                    // tg: the first camera if usable, else every camera's tag.
                    let tags: Vec<&Camera> = alarm
                        .cameras
                        .iter()
                        .filter_map(|&c| cameras.get(c).ok())
                        .collect();
                    let where_ = match tags.first() {
                        None => "No Camera".to_owned(),
                        Some(first) if first.enabled => first.c_tag.clone(),
                        Some(_) => tags
                            .iter()
                            .map(|c| c.c_tag.as_str())
                            .collect::<Vec<_>>()
                            .join(" | "),
                    };
                    format!("--- {} alarm detected in {name}! ({where_})", kind.name())
                };
                queue.show.push(line);
                *queue.show_counts.entry(kind).or_default() += 1;
            } else {
                queue.clear.push(format!(
                    "--- {} alarm in {name} has been cleared.",
                    kind.name()
                ));
                *queue.clear_counts.entry(kind).or_default() += 1;
            }
            if queue.due.is_none() {
                queue.due = Some(now + SHOW_DELAY);
            }
        }
    }
}

/// tg `show_alarms`: the gathered lines, or the counts per type when there are many.
pub fn alarm_lines(
    show: &[String],
    clear: &[String],
    show_counts: &BTreeMap<AlarmKind, u32>,
    clear_counts: &BTreeMap<AlarmKind, u32>,
) -> Vec<String> {
    let summary = |counts: &BTreeMap<AlarmKind, u32>, what: &str| {
        let mut msg = String::from("--- ");
        for (kind, n) in counts {
            msg.push_str(&format!(
                "{}: {n} alarms {what}. - ",
                kind.name().to_uppercase()
            ));
        }
        msg.push_str("[Show Alerts]");
        msg
    };
    let mut lines = Vec::new();
    if show.len() < 5 {
        lines.extend(show.iter().cloned());
    } else {
        lines.push(summary(show_counts, "detected"));
    }
    if clear.len() < 3 {
        lines.extend(clear.iter().cloned());
    } else {
        lines.push(summary(clear_counts, "cleared"));
    }
    lines
}

fn show_alarms(
    mut queues: Query<(&mut AlarmQueue, Option<&PlayerMob>)>,
    time: Res<Time>,
    mut chat: MessageWriter<SendChat>,
) {
    let now = time.elapsed_secs_f64();
    for (mut queue, player) in &mut queues {
        if queue.due.is_none_or(|due| now < due) {
            continue;
        }
        let q = std::mem::take(&mut *queue);
        let Some(client) = player.map(|p| p.client) else {
            continue;
        };
        for line in alarm_lines(&q.show, &q.clear, &q.show_counts, &q.clear_counts) {
            chat.write(SendChat {
                clients: vec![client],
                message: ChatMessage::simple(ChatChannel::Local, Span::Plain, line),
            });
        }
    }
}

/// tg `station_alert/Initialize`: station levels list station and mining areas; elsewhere a
/// "local alert console" lists its own level.
fn init_consoles(world: &mut World) {
    let mut q = world.query::<(Entity, &DmPower, &TilePos)>();
    let found: Vec<(Entity, u16)> = q
        .iter(world)
        .filter(|(_, dm, _)| is_under(&dm.path, STATION_ALERT_CONSOLE))
        .map(|(e, _, p)| (e, p.z))
        .collect();
    let station = crate::events::controller::station_levels(world);
    for (e, z) in found {
        setup_console(world, e, (!station.contains(&z)).then_some(z));
    }
}

/// A station alert console's listener; `local` is its z-level off the station.
pub(crate) fn setup_console(world: &mut World, e: Entity, local: Option<u16>) {
    let mut entity = world.entity_mut(e);
    entity.insert((
        StationAlertConsole,
        AlertListener {
            kinds: AlarmKind::CONSOLE.to_vec(),
            scope: local.map_or(AlertScope::Station, AlertScope::Level),
            camera_view: false,
        },
    ));
    if local.is_some()
        && let Some(mut name) = entity.get_mut::<DmName>()
    {
        name.name = "local alert console".to_owned();
    }
}

/// `update_overlays`: `alert:2` while powered with alarms listed.
fn console_screens(
    icons: Option<Res<IconIndex>>,
    alarms: Res<StationAlarms>,
    mut consoles: Query<
        (&AlertListener, &DmAtom, &mut Appearance, Has<Unpowered>),
        With<StationAlertConsole>,
    >,
) {
    let Some(icons) = icons else { return };
    if !alarms.is_changed() {
        return;
    }
    for (listener, atom, mut appearance, unpowered) in &mut consoles {
        let Some(id) = icons.get(&atom.icon, ALERT_SCREEN) else {
            continue;
        };
        let want = !unpowered && alarms.listed(listener).next().is_some();
        let has = appearance.overlays.iter().any(|o| o.icon_state == id);
        if want && !has {
            appearance.overlays.push(Overlay::new(id));
        } else if !want && has {
            appearance.overlays.retain(|o| o.icon_state != id);
        }
    }
}

/// A powered console's click opens its window (tg `ui_interact`).
pub fn interact(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if world.get::<StationAlertConsole>(target).is_none() {
        return false;
    }
    if world.get::<Unpowered>(target).is_none() {
        open(world, client, mob, target);
    }
    true
}

/// Opens `source`'s alert window (its [`AlertListener`]'s) for `client`.
pub fn open(world: &mut World, client: Entity, actor: Entity, source: Entity) {
    let title = match world.get::<DmName>(source) {
        Some(n) => crate::vending::plain_name(&n.name).to_owned(),
        None => "Station Alerts".to_owned(),
    };
    world.write_message(OpenUi::new::<StationAlertWindow>(
        client, actor, source, title,
    ));
}

pub struct StationAlertWindow;

impl ss13_tgui::UiInterface for StationAlertWindow {
    const NAME: &'static str = StationAlertUi::NAME;
    type Data = StationAlertData;
    type StaticData = ();
}

/// The `select_camera` act's area.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelectCamera(pub Entity);

/// `ui_data` of a listener.
pub fn alert_data(alarms: &StationAlarms, listener: &AlertListener) -> StationAlertData {
    let mut kinds = listener.kinds.clone();
    kinds.sort();
    kinds.dedup();
    let alarms = kinds
        .into_iter()
        .map(|kind| AlarmCategory {
            name: kind.name().to_owned(),
            alerts: alarms
                .listed(listener)
                .filter(|(k, ..)| *k == kind)
                .map(|(_, area, alarm)| AlarmAlert {
                    name: alarm.name.clone(),
                    cameras: listener.camera_view.then_some(alarm.cameras.len() as u32),
                    sources: listener.camera_view.then_some(alarm.sources.len() as u32),
                    alert_ref: listener.camera_view.then(|| area.to_bits().to_string()),
                })
                .collect(),
        })
        .collect();
    StationAlertData {
        camera_view: listener.camera_view,
        alarms,
    }
}

impl ServerUi for StationAlertWindow {
    type Source = &'static AlertListener;
    type UserParam = Res<'static, StationAlarms>;
    type Act = SelectCamera;

    fn static_data(_: &AlertListener) {}

    fn data(source: &AlertListener, _: Entity, alarms: &Res<StationAlarms>) -> StationAlertData {
        alert_data(alarms, source)
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<SelectCamera, UiReject> {
        if action != SELECT_CAMERA {
            return Err(UiReject::UnknownAction);
        }
        params
            .get("alert")
            .and_then(|r| r.parse::<u64>().ok())
            .and_then(Entity::try_from_bits)
            .map(SelectCamera)
            .ok_or_else(|| UiReject::BadParams("alert: not a ref".into()))
    }
}

/// The AI at `eye` can act now (not dead, depowered or with control disabled).
pub(crate) fn ai_can_act(world: &World, eye: Entity) -> bool {
    let Some(core) = world.get::<AiEye>(eye).map(|e| e.ai) else {
        return false;
    };
    world.get::<Stat>(core) == Some(&Stat::Conscious)
        && world
            .get::<AiCore>(core)
            .is_some_and(|c| !c.control_disabled)
        && !super::ai_power::depowered(world, core)
}

/// tg `switchCamera`: the eye moves to a usable camera.
pub(crate) fn switch_camera(world: &mut World, client: Entity, eye: Entity, camera: Entity) {
    if !ai_can_act(world, eye) {
        return;
    }
    let Some((cam, &pos)) = world
        .get::<Camera>(camera)
        .zip(world.get::<TilePos>(camera))
    else {
        return;
    };
    if !cam.enabled {
        super::notice(world, client, Span::Warning, "Camera is unavailable!");
        return;
    }
    world.entity_mut(eye).insert(pos);
}

/// On a prompt: the AI eye choosing which camera to jump to.
#[derive(Component, Clone, Debug)]
pub(crate) struct CameraChoice {
    pub eye: Entity,
    pub cameras: Vec<Entity>,
}

/// tg `station_alert/ui_act("select_camera")`: AIs only; one camera jumps there, several ask.
fn select_camera(world: &mut World, mut acts: Local<MessageCursor<UiActed<StationAlertWindow>>>) {
    let pending: Vec<(Entity, Entity, Entity, SelectCamera)> = acts
        .read(world.resource::<Messages<UiActed<StationAlertWindow>>>())
        .map(|a| (a.client, a.actor, a.source, a.act))
        .collect();
    for (client, actor, source, SelectCamera(area)) in pending {
        if world.get::<AiEye>(actor).is_none() {
            continue;
        }
        let Some(listener) = world.get::<AlertListener>(source) else {
            continue;
        };
        let cameras: Vec<Entity> = {
            let alarms = world.resource::<StationAlarms>();
            let Some((.., alarm)) = alarms.listed(listener).find(|(_, a, _)| *a == area) else {
                continue;
            };
            alarm.cameras.clone()
        };
        ask_camera(world, client, actor, cameras, false);
    }
}

/// Jumps to the only camera, or asks which (tg `tgui_input_list` of the cameras by tag;
/// `list_all`: the camera list, which always asks and marks unusable cameras).
pub(crate) fn ask_camera(
    world: &mut World,
    client: Entity,
    eye: Entity,
    cameras: Vec<Entity>,
    list_all: bool,
) {
    let mut named: Vec<(String, Entity)> = cameras
        .into_iter()
        .filter_map(|c| {
            let cam = world.get::<Camera>(c)?;
            let name = if list_all && !cam.enabled {
                format!("{} (Deactivated)", cam.c_tag)
            } else {
                cam.c_tag.clone()
            };
            Some((name, c))
        })
        .collect();
    named.sort_by(|a, b| a.0.cmp(&b.0));
    match named.as_slice() {
        [] => {}
        [(_, only)] if !list_all => switch_camera(world, client, eye, *only),
        _ => {
            let prompt = ss13_tgui::prompt::ask_world(
                world,
                Ask::new(
                    client,
                    eye,
                    "Camera Selection",
                    "Choose a camera to jump to",
                    PromptKind::List {
                        items: named.iter().map(|(n, _)| n.clone()).collect(),
                        default: None,
                    },
                ),
            );
            world.entity_mut(prompt).insert(CameraChoice {
                eye,
                cameras: named.into_iter().map(|(_, c)| c).collect(),
            });
        }
    }
}

fn camera_chosen(world: &mut World, mut answers: Local<MessageCursor<PromptAnswered>>) {
    let pending: Vec<(Entity, Entity, Option<PromptAnswer>)> = answers
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.answer.clone()))
        .collect();
    for (prompt, client, answer) in pending {
        let Some(choice) = world.get::<CameraChoice>(prompt).cloned() else {
            continue;
        };
        if let Some(PromptAnswer::Item(i)) = answer
            && let Some(&camera) = choice.cameras.get(i)
        {
            switch_camera(world, client, choice.eye, camera);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raised(change: Option<AlarmChanged>) -> Option<bool> {
        change.map(|c| c.raised)
    }

    fn info(z: u16) -> impl FnOnce(Entity) -> AreaInfo {
        move |_| AreaInfo {
            name: "Bridge".into(),
            station: true,
            z,
            cameras: vec![],
        }
    }

    /// tg listeners: the first handler in an area raises, the last one clears; a handler sends
    /// once per area.
    #[test]
    fn areas_raise_on_first_source_and_clear_on_last() {
        let mut alarms = StationAlarms::default();
        let (a, b, area) = (
            Entity::from_bits(1),
            Entity::from_bits(2),
            Entity::from_bits(10),
        );
        let k = AlarmKind::Atmosphere;
        assert_eq!(
            raised(alarms.set(k, a, Some(area), true, info(1))),
            Some(true)
        );
        assert_eq!(
            raised(alarms.set(k, a, Some(area), true, info(1))),
            None,
            "sent once"
        );
        assert_eq!(raised(alarms.set(k, b, Some(area), true, info(1))), None);
        assert_eq!(alarms.active[&(k, area)].sources.len(), 2);
        assert_eq!(raised(alarms.set(k, a, Some(area), false, info(1))), None);
        assert_eq!(
            raised(alarms.set(k, a, Some(area), false, info(1))),
            None,
            "not sent"
        );
        assert_eq!(
            raised(alarms.set(k, b, None, false, info(1))),
            Some(false),
            "a removed source clears its area"
        );
        assert!(alarms.active.is_empty());
        // Other types of the same area are separate.
        alarms.set(k, a, Some(area), true, info(1));
        assert_eq!(
            raised(alarms.set(AlarmKind::Power, b, Some(area), true, info(1))),
            Some(true)
        );
    }

    #[test]
    fn listeners_filter_by_scope_and_type_and_list_nominal_types() {
        let mut alarms = StationAlarms::default();
        let area = Entity::from_bits(10);
        alarms.set(
            AlarmKind::Power,
            Entity::from_bits(1),
            Some(area),
            true,
            info(2),
        );
        alarms.set(
            AlarmKind::Camera,
            Entity::from_bits(2),
            Some(area),
            true,
            info(2),
        );
        let console = AlertListener {
            kinds: AlarmKind::CONSOLE.to_vec(),
            scope: AlertScope::Station,
            camera_view: false,
        };
        let data = alert_data(&alarms, &console);
        let names: Vec<&str> = data.alarms.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["Fire", "Atmosphere", "Power"]);
        assert_eq!(data.alarms[2].alerts.len(), 1);
        assert_eq!(data.alarms[2].alerts[0].alert_ref, None);
        let local = AlertListener {
            scope: AlertScope::Level(1),
            ..console
        };
        assert!(alarms.listed(&local).next().is_none());
        let ai = AlertListener::silicon(true);
        let data = alert_data(&alarms, &ai);
        assert_eq!(data.alarms.len(), 6);
        let power = &data.alarms[2].alerts[0];
        assert_eq!((power.cameras, power.sources), (Some(0), Some(1)));
        assert_eq!(power.alert_ref, Some(area.to_bits().to_string()));
    }

    /// tg `show_alarms`: up to 4 raised and 2 cleared lines as they are, else counts.
    #[test]
    fn many_alarms_are_summed_up() {
        let lines = |n: usize| (0..n).map(|i| format!("line {i}")).collect::<Vec<_>>();
        let counts = BTreeMap::from([(AlarmKind::Fire, 2), (AlarmKind::Power, 3)]);
        let out = alarm_lines(&lines(4), &lines(2), &counts, &counts);
        assert_eq!(out.len(), 6);
        let out = alarm_lines(&lines(5), &lines(3), &counts, &counts);
        assert_eq!(
            out,
            [
                "--- FIRE: 2 alarms detected. - POWER: 3 alarms detected. - [Show Alerts]",
                "--- FIRE: 2 alarms cleared. - POWER: 3 alarms cleared. - [Show Alerts]",
            ]
        );
    }
}
