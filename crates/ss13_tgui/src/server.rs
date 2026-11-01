//! Server half: the window model (`datum/tgui` + `SStgui`).
//!
//! A game plugin declares a window by implementing [`ServerUi`] for its interface, calling
//! [`AppTguiExt::register_ui`], handling [`UiActed<T>`] in [`UiSystems::Handle`] and writing
//! [`OpenUi`] when a player interacts with the source:
//!
//! ```ignore
//! app.register_ui::<ThermoMachineUi>()
//!     .add_systems(Update, apply_thermo_acts.in_set(UiSystems::Handle));
//! open.write(OpenUi::new::<ThermoMachineUi>(client, mob, machine, "thermomachine"));
//! ```
//!
//! Per tick (`Update`, sets in order): windows the clients closed go first (before
//! `WorldSystems::Movement`); [`UiSystems::Open`] opens windows;
//! [`UiSystems::Status`] (after `WorldSystems::Movement`) computes tgui's default `ui_status` (distance between the actor and the
//! source); [`UiSystems::StatusHook`] is where game systems lower it ([`UiWindow::cap_status`]:
//! incapacitation, power, panel open...); [`UiSystems::Receive`] validates acts against that
//! status; [`UiSystems::Handle`] is the game's `ui_act`; [`UiSystems::Send`] sends updates and
//! closes windows whose status fell to [`UiStatus::Close`].
//!
//! Updates are full `ui_data`, skipped when the encoded bytes did not change. Measured
//! (`interfaces::thermo_machine` tests): ThermoMachine data is 25 bytes; a field diff would save
//! ~16 bytes per update, less than one packet header, at ~1 update/s per window. Revisit for
//! interfaces with large lists (Vending stock), where a keyed diff pays off.

use core::fmt::Debug;
use core::marker::PhantomData;
use std::collections::HashMap;

use bevy::ecs::query::{QueryItem, ReadOnlyQueryData};
use bevy::ecs::system::{ReadOnlySystemParam, StaticSystemParam, SystemParamItem};
use bevy::prelude::*;
use ss13_net::server::{ClientDisconnected, IntentReceived};
use ss13_net::ui::server::{SendUi, UiMessageReceived};
use ss13_shared::TilePos;
use ss13_shared::protocol::ClientIntent;
use ss13_shared::ui::{UiClientMessage, UiServerMessage, UiStatus};
use ss13_world::WorldSystems;

use crate::{UiInterface, encode};

/// `SStgui` `wait = 9` deciseconds: the period of the autoupdate.
pub const REFRESH_SECS: f64 = 0.9;
/// BYOND `SECOND_TOPIC_LIMIT` / `MINUTE_TOPIC_LIMIT` from the tg config, applied to UI acts.
pub const ACTS_PER_SECOND: u32 = 10;
pub const ACTS_PER_MINUTE: u32 = 200;
/// Hostile-input caps; real acts are a word and a couple of short params.
pub const MAX_ACTION_LEN: usize = 64;
pub const MAX_PARAMS: usize = 16;
pub const MAX_PARAM_LEN: usize = 256;

/// The server side of a tgui interface: `ui_static_data`, `ui_data`, and the parsing half of
/// `ui_act`. The effect half is a game system reading [`UiActed<Self>`].
pub trait ServerUi: UiInterface {
    /// What `ui_data` reads from the source entity. A source without it cannot have this window.
    type Source: ReadOnlyQueryData + 'static;
    /// What `ui_data(mob/user)` reads about the user, as read-only system params (queries over
    /// the user's body and what it carries); `()` when the data does not depend on the user.
    type UserParam: ReadOnlySystemParam + 'static;
    /// A parsed, well-formed act.
    type Act: Send + Sync + Debug + 'static;

    fn static_data(source: QueryItem<'_, '_, Self::Source>) -> Self::StaticData;
    /// `user` is the window's actor (its body); it may have despawned this tick.
    fn data(
        source: QueryItem<'_, '_, Self::Source>,
        user: Entity,
        params: &SystemParamItem<'_, '_, Self::UserParam>,
    ) -> Self::Data;
    /// Validates the action name and params shape. State-dependent checks (ranges, ownership,
    /// stock) belong in the handler, at the moment of effect.
    fn parse_act(action: &str, params: &UiParams) -> Result<Self::Act, UiReject>;
}

/// Why an act was dropped. Reported in [`UiActRejected`]; the client gets no answer, as in tgui.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiReject {
    RateLimited,
    /// Over a size cap or duplicate param keys.
    Malformed,
    /// No such window, or it belongs to another client.
    UnknownWindow,
    NotInteractive(UiStatus),
    UnknownAction,
    BadParams(String),
}

/// Validated act, written after the window's status was checked this tick.
pub struct UiActed<T: ServerUi> {
    pub window: Entity,
    pub client: Entity,
    pub actor: Entity,
    pub source: Entity,
    pub act: T::Act,
}

impl<T: ServerUi> Message for UiActed<T> {}

/// A dropped act, for logs and abuse detection.
#[derive(Message, Clone, Debug)]
pub struct UiActRejected {
    pub client: Entity,
    pub window: u32,
    pub action: String,
    pub reason: UiReject,
}

/// Opens `interface` on `source` for `client`, whose body is `actor` (tgui `ui_interact(user)`).
/// If that window is already open it is refreshed instead (`SStgui.try_update_ui`).
#[derive(Message, Clone, Debug)]
pub struct OpenUi {
    pub client: Entity,
    pub actor: Entity,
    pub source: Entity,
    pub interface: &'static str,
    pub title: String,
}

impl OpenUi {
    pub fn new<T: ServerUi>(
        client: Entity,
        actor: Entity,
        source: Entity,
        title: impl Into<String>,
    ) -> Self {
        Self {
            client,
            actor,
            source,
            interface: T::NAME,
            title: title.into(),
        }
    }
}

/// Act params as sent by the client. Keys are unique (checked before parsing).
pub struct UiParams<'a>(&'a [(String, String)]);

impl<'a> UiParams<'a> {
    pub fn new(params: &'a [(String, String)]) -> Self {
        Self(params)
    }

    pub fn get(&self, key: &str) -> Option<&'a str> {
        self.0
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    /// A finite number (DM `text2num` on a tgui param).
    pub fn number(&self, key: &str) -> Result<f64, UiReject> {
        self.get(key)
            .and_then(|v| v.trim().parse::<f64>().ok())
            .filter(|v| v.is_finite())
            .ok_or_else(|| UiReject::BadParams(format!("{key}: expected a number")))
    }
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UiSystems {
    Open,
    Status,
    /// Game systems lowering [`UiWindow::cap_status`].
    StatusHook,
    Receive,
    /// Game systems applying [`UiActed`].
    Handle,
    Send,
}

/// One open window (`datum/tgui`). Lives on its own entity.
#[derive(Component, Debug)]
pub struct UiWindow {
    pub id: u32,
    pub client: Entity,
    pub actor: Entity,
    pub source: Entity,
    pub interface: &'static str,
    title: String,
    status: UiStatus,
    /// `None` until the `Open` message went out.
    sent_status: Option<UiStatus>,
    last_data: Vec<u8>,
    next_refresh: f64,
    /// Send this tick regardless of the timer (opened again, act applied).
    dirty: bool,
    pending: Vec<(String, Vec<(String, String)>)>,
}

impl UiWindow {
    pub fn status(&self) -> UiStatus {
        self.status
    }

    /// tgui combines status checks with `min`: a hook can only lower the status.
    pub fn cap_status(&mut self, cap: UiStatus) {
        self.status = self.status.min(cap);
    }
}

/// The server type of a window. Several types may serve one interface (`ListInputWindow`), so
/// hooks that act on their own windows filter by this, not by [`UiWindow::interface`].
#[derive(Component)]
pub struct UiKind<T>(PhantomData<fn() -> T>);

#[derive(Resource, Default)]
struct UiWindows {
    by_id: HashMap<u32, Entity>,
    by_key: HashMap<(Entity, Entity, &'static str), Entity>,
    next_id: u32,
}

/// Fixed one-second and one-minute windows, as BYOND `client/Topic` counts topics.
#[derive(Default)]
struct ActLimiter {
    second: (f64, u32),
    minute: (f64, u32),
}

impl ActLimiter {
    fn allow(&mut self, now: f64) -> bool {
        for (window, len, max) in [
            (&mut self.second, 1.0, ACTS_PER_SECOND),
            (&mut self.minute, 60.0, ACTS_PER_MINUTE),
        ] {
            if now - window.0 >= len {
                *window = (now, 0);
            }
            window.1 += 1;
            if window.1 > max {
                return false;
            }
        }
        true
    }
}

#[derive(Resource, Default)]
struct ActLimits(HashMap<Entity, ActLimiter>);

pub struct TguiServerPlugin;

impl Plugin for TguiServerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<UiWindows>()
            .init_resource::<ActLimits>()
            .init_resource::<Time<Real>>()
            .add_message::<OpenUi>()
            .add_message::<UiActRejected>()
            .configure_sets(
                Update,
                (
                    UiSystems::Open,
                    UiSystems::Status,
                    UiSystems::StatusHook,
                    UiSystems::Receive,
                    UiSystems::Handle,
                    UiSystems::Send,
                )
                    .chain(),
            )
            // Status is judged where the actor is after this tick's moves, not before them.
            .configure_sets(Update, UiSystems::Status.after(WorldSystems::Movement))
            .add_systems(
                Update,
                (
                    // Closes are applied before this tick's game systems: a client closes the
                    // window it sees before it clicks something that reopens it, so a close and
                    // a reopen of the same window arriving in one tick leave it open.
                    client_closes.before(WorldSystems::Movement),
                    disconnects.in_set(UiSystems::Open),
                    default_status.in_set(UiSystems::Status),
                    receive_acts.in_set(UiSystems::Receive),
                ),
            )
            .add_observer(forget_window);
    }
}

pub trait AppTguiExt {
    /// Declares interface `T` (tgui `ui_interact` + `ui_data` + `ui_act` of one type).
    fn register_ui<T: ServerUi>(&mut self) -> &mut Self;
}

impl AppTguiExt for App {
    fn register_ui<T: ServerUi>(&mut self) -> &mut Self {
        self.add_message::<UiActed<T>>().add_systems(
            Update,
            (
                open::<T>.in_set(UiSystems::Open),
                route_acts::<T>
                    .in_set(UiSystems::Receive)
                    .after(receive_acts),
                send::<T>.in_set(UiSystems::Send),
            ),
        )
    }
}

fn open<T: ServerUi>(
    mut requests: MessageReader<OpenUi>,
    sources: Query<T::Source>,
    mut windows: ResMut<UiWindows>,
    mut existing: Query<&mut UiWindow>,
    mut commands: Commands,
) {
    // Windows opened by this run: their spawn commands are not applied until the system ends, so
    // a second request for the same key in one tick must find them here, not in `existing`.
    let mut opened: HashMap<(Entity, Entity, &'static str), UiWindow> = HashMap::new();
    for request in requests.read().filter(|r| r.interface == T::NAME) {
        let key = (request.client, request.source, T::NAME);
        if let Some(&entity) = windows.by_key.get(&key)
            && let Ok(mut window) = existing.get_mut(entity)
        {
            window.actor = request.actor;
            window.dirty = true;
            continue;
        }
        if let Some(window) = opened.get_mut(&key) {
            window.actor = request.actor;
            continue;
        }
        if !sources.contains(request.source) {
            warn!(
                "OpenUi {}: source {} lacks the interface's components",
                T::NAME,
                request.source
            );
            continue;
        }
        windows.next_id = windows.next_id.wrapping_add(1).max(1);
        opened.insert(
            key,
            UiWindow {
                id: windows.next_id,
                client: request.client,
                actor: request.actor,
                source: request.source,
                interface: T::NAME,
                title: request.title.clone(),
                status: UiStatus::Close,
                sent_status: None,
                last_data: Vec::new(),
                next_refresh: 0.0,
                dirty: true,
                pending: Vec::new(),
            },
        );
    }
    for (key, window) in opened {
        let id = window.id;
        let entity = commands
            .spawn((
                Name::new(format!("ui {} #{id}", T::NAME)),
                window,
                UiKind::<T>(PhantomData),
            ))
            .id();
        windows.by_id.insert(id, entity);
        windows.by_key.insert(key, entity);
    }
}

fn forget_window(
    remove: On<Remove<UiWindow>>,
    all: Query<&UiWindow>,
    mut windows: ResMut<UiWindows>,
) {
    if let Ok(window) = all.get(remove.entity) {
        // Only this window's entries: an id or key may already map to another window.
        let key = (window.client, window.source, window.interface);
        if windows.by_id.get(&window.id) == Some(&remove.entity) {
            windows.by_id.remove(&window.id);
        }
        if windows.by_key.get(&key) == Some(&remove.entity) {
            windows.by_key.remove(&key);
        }
    }
}

fn client_closes(
    mut received: MessageReader<UiMessageReceived>,
    windows: Res<UiWindows>,
    all: Query<&UiWindow>,
    mut commands: Commands,
) {
    for UiMessageReceived { client, message } in received.read() {
        let UiClientMessage::Close { window } = message;
        let entity = windows.by_id.get(window).copied();
        if let Some(entity) = entity
            && all.get(entity).is_ok_and(|w| w.client == *client)
        {
            commands.entity(entity).despawn();
        }
    }
}

fn disconnects(
    mut gone: MessageReader<ClientDisconnected>,
    all: Query<(Entity, &UiWindow)>,
    mut limits: ResMut<ActLimits>,
    mut commands: Commands,
) {
    for ClientDisconnected { client, .. } in gone.read() {
        limits.0.remove(client);
        for (entity, _) in all.iter().filter(|(_, w)| w.client == *client) {
            commands.entity(entity).despawn();
        }
    }
}

/// tgui `shared_living_ui_distance` without the line-of-sight check: interactive at 0-1 tiles,
/// update-only at 2, disabled at 3-5, closed beyond 5 or on another z.
// shortcut: no `view()` occlusion and no "source inside a container" (`get_turf`); add both when
// the world has visibility and inventories.
pub fn distance_status(actor: TilePos, source: TilePos) -> UiStatus {
    match actor.chebyshev(source) {
        Some(0..=1) => UiStatus::Interactive,
        Some(2) => UiStatus::Update,
        Some(3..=5) => UiStatus::Disabled,
        _ => UiStatus::Close,
    }
}

/// On a window source: tgui `always_state` instead of the distance check, for sources that are
/// nowhere on the map (the lobby's new player, tg `new_player_state`). Hooks may still lower it.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct AlwaysState;

/// On a window actor whose reach is not the distance check (tg silicons' overridden
/// `default_can_use_topic`: the AI through its cameras, cyborgs by view): the default status
/// is interactive and a game hook in [`UiSystems::StatusHook`] lowers it.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct RemoteActor;

fn default_status(
    mut windows: Query<&mut UiWindow>,
    positions: Query<&TilePos>,
    always: Query<(), With<AlwaysState>>,
    remote: Query<(), With<RemoteActor>>,
) {
    for mut window in &mut windows {
        let status = match (positions.get(window.actor), positions.get(window.source)) {
            _ if always.contains(window.source) => UiStatus::Interactive,
            (Ok(_), Ok(_)) if remote.contains(window.actor) => UiStatus::Interactive,
            (Ok(actor), Ok(source)) => distance_status(*actor, *source),
            // Actor or source despawned or left the map.
            _ => UiStatus::Close,
        };
        // Avoid tripping change detection for hooks that use `Changed<UiWindow>`.
        if window.status != status {
            window.status = status;
        }
    }
}

fn receive_acts(
    mut intents: MessageReader<IntentReceived>,
    windows: Res<UiWindows>,
    mut all: Query<&mut UiWindow>,
    mut limits: ResMut<ActLimits>,
    time: Res<Time<Real>>,
    mut rejected: MessageWriter<UiActRejected>,
) {
    let now = time.elapsed_secs_f64();
    for IntentReceived { client, intent } in intents.read() {
        let ClientIntent::UiAct {
            window: id,
            action,
            params,
        } = intent
        else {
            continue;
        };
        let now_ok = limits.0.entry(*client).or_default().allow(now);
        let verdict = validate_act(now_ok, *client, *id, action, params, &windows, &all);
        match verdict {
            Ok(entity) => {
                if let Ok(mut window) = all.get_mut(entity) {
                    window.pending.push((action.clone(), params.clone()));
                }
            }
            Err(reason) => {
                // A rejected action may be any size the transport let through.
                let action = clip_action(action);
                debug!("ui act {action:?} on #{id} from {client} rejected: {reason:?}");
                rejected.write(UiActRejected {
                    client: *client,
                    window: *id,
                    action,
                    reason,
                });
            }
        }
    }
}

fn validate_act(
    within_rate: bool,
    client: Entity,
    id: u32,
    action: &str,
    params: &[(String, String)],
    windows: &UiWindows,
    all: &Query<&mut UiWindow>,
) -> Result<Entity, UiReject> {
    if !within_rate {
        return Err(UiReject::RateLimited);
    }
    check_shape(action, params)?;
    let entity = *windows.by_id.get(&id).ok_or(UiReject::UnknownWindow)?;
    let window = all.get(entity).map_err(|_| UiReject::UnknownWindow)?;
    // Same answer as for a missing id: a client learns nothing about others' windows.
    if window.client != client || window.sent_status.is_none() {
        return Err(UiReject::UnknownWindow);
    }
    if window.status != UiStatus::Interactive {
        return Err(UiReject::NotInteractive(window.status));
    }
    Ok(entity)
}

/// Size caps before the duplicate scan, so the quadratic scan is bounded by `MAX_PARAMS`².
fn check_shape(action: &str, params: &[(String, String)]) -> Result<(), UiReject> {
    let oversized = action.len() > MAX_ACTION_LEN
        || params.len() > MAX_PARAMS
        || params
            .iter()
            .any(|(k, v)| k.len() > MAX_PARAM_LEN || v.len() > MAX_PARAM_LEN);
    if oversized
        || params
            .iter()
            .enumerate()
            .any(|(i, (k, _))| params[..i].iter().any(|(other, _)| other == k))
    {
        return Err(UiReject::Malformed);
    }
    Ok(())
}

/// The first [`MAX_ACTION_LEN`] bytes of `action`, cut at a char boundary.
fn clip_action(action: &str) -> String {
    let mut end = action.len().min(MAX_ACTION_LEN);
    while !action.is_char_boundary(end) {
        end -= 1;
    }
    action[..end].to_owned()
}

fn route_acts<T: ServerUi>(
    mut windows: Query<(Entity, &mut UiWindow), With<UiKind<T>>>,
    mut acted: MessageWriter<UiActed<T>>,
    mut rejected: MessageWriter<UiActRejected>,
) {
    let mut acted_sources = Vec::new();
    for (entity, mut window) in &mut windows {
        if window.pending.is_empty() {
            continue;
        }
        for (action, params) in std::mem::take(&mut window.pending) {
            match T::parse_act(&action, &UiParams(&params)) {
                Ok(act) => {
                    acted_sources.push(window.source);
                    acted.write(UiActed {
                        window: entity,
                        client: window.client,
                        actor: window.actor,
                        source: window.source,
                        act,
                    });
                }
                Err(reason) => {
                    debug!("ui act {action:?} on #{} rejected: {reason:?}", window.id);
                    rejected.write(UiActRejected {
                        client: window.client,
                        window: window.id,
                        action,
                        reason,
                    });
                }
            }
        }
    }
    // tgui `SStgui.update_uis(src_object)` after a handled act: every window on that source,
    // not only the actor's (others at the same vendor see the new stock this tick).
    if !acted_sources.is_empty() {
        for (_, mut window) in &mut windows {
            if acted_sources.contains(&window.source) {
                window.dirty = true;
            }
        }
    }
}

fn send<T: ServerUi>(
    mut windows: Query<(Entity, &mut UiWindow), With<UiKind<T>>>,
    sources: Query<T::Source>,
    user: StaticSystemParam<T::UserParam>,
    time: Res<Time<Real>>,
    mut out: MessageWriter<SendUi>,
    mut commands: Commands,
) {
    let now = time.elapsed_secs_f64();
    for (entity, mut window) in &mut windows {
        let status = window.status;
        // tgui `open()` refuses below UI_UPDATE; a later fall to UI_CLOSE closes. A source that
        // lost the interface's components is gone for this window.
        if status == UiStatus::Close
            || (window.sent_status.is_none() && status < UiStatus::Update)
            || !sources.contains(window.source)
        {
            close(entity, &window, &mut out, &mut commands);
            continue;
        }
        let status_changed = window.sent_status != Some(status);
        if !(window.dirty || status_changed || now >= window.next_refresh) {
            continue;
        }
        window.dirty = false;
        window.next_refresh = now + REFRESH_SECS;
        let id = window.id;
        if window.sent_status.is_none() {
            let (Ok(for_static), Ok(for_data)) =
                (sources.get(window.source), sources.get(window.source))
            else {
                continue;
            };
            let static_data = encode(&T::static_data(for_static));
            let data = encode(&T::data(for_data, window.actor, &user));
            out.write(SendUi {
                client: window.client,
                message: UiServerMessage::Open {
                    window: id,
                    interface: T::NAME.into(),
                    title: window.title.clone(),
                    status,
                    static_data,
                    data: data.clone(),
                },
            });
            window.last_data = data;
            window.sent_status = Some(status);
            continue;
        }
        let data = (status >= UiStatus::Update)
            .then(|| {
                let source = sources.get(window.source).ok()?;
                Some(encode(&T::data(source, window.actor, &user)))
            })
            .flatten()
            .filter(|data| *data != window.last_data);
        if data.is_none() && !status_changed {
            continue;
        }
        if let Some(data) = &data {
            window.last_data.clone_from(data);
        }
        window.sent_status = Some(status);
        out.write(SendUi {
            client: window.client,
            message: UiServerMessage::Update {
                window: id,
                status,
                data,
            },
        });
    }
}

fn close(
    entity: Entity,
    window: &UiWindow,
    out: &mut MessageWriter<SendUi>,
    commands: &mut Commands,
) {
    if window.sent_status.is_some() {
        out.write(SendUi {
            client: window.client,
            message: UiServerMessage::Close { window: window.id },
        });
    }
    commands.entity(entity).despawn();
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::interfaces::thermo_machine::{self, ThermoMachine, ThermoMachineUi};

    #[derive(Resource, Default)]
    struct Log {
        acted: usize,
        rejected: Vec<UiReject>,
    }

    /// Server-side tgui with the net messages it reads, no network. Machine at (5,5), actor next
    /// to it at (5,6).
    fn app() -> (App, Entity, Entity, Entity) {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<IntentReceived>()
            .add_message::<ClientDisconnected>()
            .add_message::<UiMessageReceived>()
            .add_message::<SendUi>()
            .add_plugins((TguiServerPlugin, thermo_machine::plugin))
            .init_resource::<Log>()
            .add_systems(
                Update,
                (|mut acted: MessageReader<UiActed<ThermoMachineUi>>,
                  mut rejected: MessageReader<UiActRejected>,
                  mut log: ResMut<Log>| {
                    log.acted += acted.read().count();
                    log.rejected
                        .extend(rejected.read().map(|r| r.reason.clone()));
                })
                .after(UiSystems::Handle),
            );
        let world = app.world_mut();
        let machine = world
            .spawn((TilePos::new(5, 5, 1), ThermoMachine::default()))
            .id();
        let actor = world.spawn(TilePos::new(5, 6, 1)).id();
        let client = world.spawn_empty().id();
        (app, client, actor, machine)
    }

    fn windows(app: &mut App) -> Vec<u32> {
        let mut q = app.world_mut().query::<&UiWindow>();
        q.iter(app.world()).map(|w| w.id).collect()
    }

    #[test]
    fn same_window_opened_twice_in_one_tick_is_one_window() {
        let (mut app, client, actor, machine) = app();
        for _ in 0..2 {
            app.world_mut()
                .write_message(OpenUi::new::<ThermoMachineUi>(client, actor, machine, "t"));
        }
        app.update();
        let open = windows(&mut app);
        assert_eq!(open.len(), 1);

        app.world_mut().write_message(UiMessageReceived {
            client,
            message: UiClientMessage::Close { window: open[0] },
        });
        app.update();
        assert!(windows(&mut app).is_empty());
        assert!(app.world().resource::<UiWindows>().by_key.is_empty());

        app.world_mut()
            .write_message(OpenUi::new::<ThermoMachineUi>(client, actor, machine, "t"));
        app.update();
        assert_eq!(windows(&mut app).len(), 1);
    }

    #[test]
    fn forgetting_a_window_keeps_entries_of_another_window() {
        let (mut app, client, actor, machine) = app();
        app.world_mut()
            .write_message(OpenUi::new::<ThermoMachineUi>(client, actor, machine, "t"));
        app.update();
        let live = *app
            .world()
            .resource::<UiWindows>()
            .by_key
            .values()
            .next()
            .unwrap();
        let id = app.world().get::<UiWindow>(live).unwrap().id;
        // A stale window with the same key and id, as two opens in one tick used to leave.
        let stale = app
            .world_mut()
            .spawn(UiWindow {
                id,
                client,
                actor,
                source: machine,
                interface: ThermoMachineUi::NAME,
                title: String::new(),
                status: UiStatus::Close,
                sent_status: None,
                last_data: Vec::new(),
                next_refresh: 0.0,
                dirty: false,
                pending: Vec::new(),
            })
            .id();
        app.world_mut().despawn(stale);
        let windows = app.world().resource::<UiWindows>();
        assert_eq!(windows.by_id.get(&id), Some(&live));
        assert_eq!(windows.by_key.values().next(), Some(&live));
    }

    #[derive(Resource)]
    struct StepAway(Option<Entity>);

    #[test]
    fn act_in_the_tick_the_actor_walks_away_is_not_interactive() {
        let (mut app, client, actor, machine) = app();
        // Added after the tgui plugin, so without an ordering constraint it runs after Status.
        app.insert_resource(StepAway(None)).add_systems(
            Update,
            // Exclusive and immediate, like `ss13_world`'s movement.
            (|world: &mut World| {
                if let Some(actor) = world.resource_mut::<StepAway>().0.take() {
                    world.entity_mut(actor).insert(TilePos::new(5, 8, 1));
                }
            })
            .in_set(WorldSystems::Movement),
        );
        app.world_mut()
            .write_message(OpenUi::new::<ThermoMachineUi>(client, actor, machine, "t"));
        app.update();
        app.update();
        let id = windows(&mut app)[0];

        app.world_mut().resource_mut::<StepAway>().0 = Some(actor);
        app.world_mut().write_message(IntentReceived {
            client,
            intent: ClientIntent::UiAct {
                window: id,
                action: "power".into(),
                params: Vec::new(),
            },
        });
        app.update();
        let log = app.world().resource::<Log>();
        assert_eq!(log.acted, 0);
        assert_eq!(log.rejected, [UiReject::NotInteractive(UiStatus::Disabled)]);
    }

    /// tgui `update_uis(src)`: an act refreshes every window on that source in the same tick,
    /// not only the actor's.
    #[test]
    fn act_refreshes_every_window_on_the_source() {
        let (mut app, client, actor, machine) = app();
        let world = app.world_mut();
        let other_client = world.spawn_empty().id();
        let other_actor = world.spawn(TilePos::new(4, 5, 1)).id();
        world.write_message(OpenUi::new::<ThermoMachineUi>(client, actor, machine, "t"));
        world.write_message(OpenUi::new::<ThermoMachineUi>(
            other_client,
            other_actor,
            machine,
            "t",
        ));
        app.update();
        app.update();
        let mut q = app.world_mut().query::<&UiWindow>();
        let id = q.iter(app.world()).find(|w| w.client == client).unwrap().id;
        app.world_mut().resource_mut::<Messages<SendUi>>().clear();
        app.world_mut().write_message(IntentReceived {
            client,
            intent: ClientIntent::UiAct {
                window: id,
                action: "power".into(),
                params: Vec::new(),
            },
        });
        app.update();
        let updated: Vec<Entity> = app
            .world_mut()
            .resource_mut::<Messages<SendUi>>()
            .drain()
            .filter(|m| matches!(m.message, UiServerMessage::Update { data: Some(_), .. }))
            .map(|m| m.client)
            .collect();
        assert!(updated.contains(&client), "{updated:?}");
        assert!(updated.contains(&other_client), "{updated:?}");
    }

    #[test]
    fn distance_matches_tgui() {
        let at = |x| TilePos::new(x, 0, 1);
        let actor = at(0);
        assert_eq!(distance_status(actor, at(1)), UiStatus::Interactive);
        assert_eq!(distance_status(actor, at(2)), UiStatus::Update);
        assert_eq!(distance_status(actor, at(5)), UiStatus::Disabled);
        assert_eq!(distance_status(actor, at(6)), UiStatus::Close);
        assert_eq!(
            distance_status(actor, TilePos::new(0, 0, 2)),
            UiStatus::Close
        );
    }

    #[test]
    fn oversized_params_are_rejected_before_the_duplicate_scan() {
        let many: Vec<_> = (0..200_000)
            .map(|i| (i.to_string(), String::new()))
            .collect();
        let start = bevy::platform::time::Instant::now();
        assert_eq!(check_shape("x", &many), Err(UiReject::Malformed));
        // Unbounded, the duplicate scan is ~2e10 string comparisons here.
        assert!(start.elapsed() < core::time::Duration::from_secs(1));

        let pair = |k: &str| (k.to_owned(), String::new());
        assert_eq!(check_shape("x", &[pair("a"), pair("b")]), Ok(()));
        assert_eq!(
            check_shape("x", &[pair("a"), pair("a")]),
            Err(UiReject::Malformed)
        );
    }

    #[test]
    fn rejected_actions_are_clipped_at_a_char_boundary() {
        assert_eq!(clip_action("power"), "power");
        let long = "я".repeat(MAX_ACTION_LEN);
        let clipped = clip_action(&long);
        assert!(clipped.len() <= MAX_ACTION_LEN && long.starts_with(&clipped));
        assert_eq!(clipped.chars().count(), MAX_ACTION_LEN / 2);
    }

    #[test]
    fn limiter_counts_per_second_and_per_minute() {
        let mut limiter = ActLimiter::default();
        let allowed = (0..15).filter(|_| limiter.allow(0.5)).count();
        assert_eq!(allowed, ACTS_PER_SECOND as usize);
        // New second, same minute.
        assert!(limiter.allow(1.6));
        let mut limiter = ActLimiter::default();
        let total: usize = (0..60)
            .map(|s| {
                (0..10)
                    .filter(|_| limiter.allow(f64::from(s) + 0.1))
                    .count()
            })
            .sum();
        assert_eq!(total, ACTS_PER_MINUTE as usize);
    }
}
