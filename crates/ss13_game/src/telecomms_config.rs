//! Telecomms configuration and monitoring (server): the machines' `Telecomms` window
//! (`machine_interactions.dm`), the telecommunications monitoring console
//! (`/obj/machinery/computer/telecomms/monitor`, `TelecommsMonitor`) and the server log browser
//! (`/obj/machinery/computer/telecomms/server`, `ServerMonitor`) with the servers' logs
//! (`/datum/comm_log_entry`).
//!
//! - A click on a telecomms machine (empty hand, or a multitool: `multitool_act` ->
//!   `attack_hand`) opens its window. Its acts need a multitool in the active hand or silicon
//!   access: power toggle, ID (≤32), network (≤15, drops every link), filtered frequencies
//!   (`tempfreq`, `freq`, `delete`; `banned_frequencies` refused), links through the multitool
//!   buffer (`buffer`, `flush`, `link`, `unlink`), relays' `broadcast`/`receive` and buses'
//!   `change_freq`.
//! - Servers log every vocal signal they pass (`crate::radio`), at most 400, garbled while the
//!   signal is still compressed ("Corrupt File").
//! - The monitor probes a network id for its machines and shows one's links; the log browser
//!   scans servers of a network within 25 tiles and reads their logs in the viewer's
//!   understanding of the language; deleting a packet needs telecomms access.
// shortcut: no server `frequency_infos` editing (custom channel names and colours: needs the
// tgui text and colour inputs), no traffic (`total_traffic` stays 0), no emag on the log browser,
// silicons get no multitool proxy for `buffer`/`link` (no `aiMulti`).

use bevy::ecs::message::MessageCursor;
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_chat::language::LanguageId;
use ss13_chat::radio::{MAX_FREE_FREQ, MIN_FREE_FREQ};
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmAtom, DmName, DmObj, DmTool};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};

use crate::combat::CombatRng;
use crate::inventory::Inventory;
use crate::power::Unpowered;
use crate::telecomms::{BANNED_FREQUENCIES, MachineKind, Telecomms, gibberish};
use crate::telecomms_ui::*;
use crate::tools::{MultitoolBuffer, ToolKind, multitool_buffer, set_multitool_buffer, tell};

pub const MONITOR_PATH: &str = "/obj/machinery/computer/telecomms/monitor";
pub const LOG_BROWSER_PATH: &str = "/obj/machinery/computer/telecomms/server";
/// `MAX_LOG_ENTRIES`.
pub const MAX_LOG_ENTRIES: usize = 400;
/// `MAX_NETWORK_ID_LENGTH`.
const MAX_NETWORK_ID: usize = 15;
const MAX_MACHINE_ID: usize = 32;
/// `urange(25, src)` of the log browser's scan.
const SCAN_RANGE: i32 = 25;
const ACCESS_TCOMMS: &str = "tcomms";
const BUZZ: &str = "sound/machines/buzz/buzz-sigh.ogg";

/// `/datum/comm_log_entry`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogEntry {
    /// `input_type`: "Speech File" or "Corrupt File".
    pub kind: &'static str,
    pub name: String,
    pub job: String,
    pub message: String,
    pub language: Option<LanguageId>,
    pub race: &'static str,
}

impl LogEntry {
    /// The log of a signal from `name` saying `message`, garbled while `compression > 0`.
    pub fn new(
        name: &str,
        message: &str,
        language: Option<LanguageId>,
        race: &'static str,
        compression: u8,
        rng: &mut CombatRng,
    ) -> Self {
        // shortcut: no `job` in this port's signal (the speaker's ID job); logged as empty.
        let mut entry = Self {
            kind: "Speech File",
            name: name.to_owned(),
            job: String::new(),
            message: message.to_owned(),
            language,
            race,
        };
        if compression > 0 {
            let replace = compression >= 20;
            entry.kind = "Corrupt File";
            entry.name = gibberish(name, replace, rng);
            entry.message = gibberish(message, replace, rng);
        }
        entry
    }
}

/// A telecomms server's `log_entries` and `total_traffic`.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct ServerLog {
    pub entries: Vec<LogEntry>,
    pub total_traffic: u32,
}

impl ServerLog {
    /// Adds an entry, dropping the oldest past [`MAX_LOG_ENTRIES`].
    pub fn push(&mut self, entry: LogEntry) {
        if self.entries.len() >= MAX_LOG_ENTRIES {
            self.entries.remove(0);
        }
        self.entries.push(entry);
    }
}

/// The monitoring console's state.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct TelecommsMonitor {
    pub machines: Vec<Entity>,
    pub selected: Option<Entity>,
    pub network: String,
    pub status: Option<String>,
}

/// The log browser's state.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct ServerMonitor {
    pub servers: Vec<Entity>,
    pub selected: Option<Entity>,
    pub network: String,
    pub error: String,
}

pub struct TelecommsConfigPlugin;

impl Plugin for TelecommsConfigPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        app.register_ui::<TelecommsUi>()
            .register_ui::<TelecommsMonitorUi>()
            .register_ui::<ServerMonitorUi>()
            .add_observer(setup_server_log)
            .add_observer(setup_console)
            .add_systems(
                Update,
                (machine_acts, monitor_acts, browser_acts).in_set(UiSystems::Handle),
            );
        app.add_click_handler(ClickPhase::EmptyHand, 3497, |w, c| {
            interact(w, c.client, c.mob, c.target)
        });
        // `multitool_act` -> `attack_hand`.
        app.add_click_handler(ClickPhase::HeldOnTarget, 1557, |w, c| {
            c.held.is_some_and(|h| is_multitool(w, h))
                && w.get::<Telecomms>(c.target).is_some()
                && interact(w, c.client, c.mob, c.target)
        });
    }
}

fn setup_server_log(add: On<Add<Telecomms>>, machines: Query<&Telecomms>, mut commands: Commands) {
    if machines
        .get(add.entity)
        .is_ok_and(|m| m.kind == MachineKind::Server)
    {
        commands.entity(add.entity).try_insert(ServerLog::default());
    }
}

fn setup_console(add: On<Add<DmObj>>, objs: Query<&DmObj>, mut commands: Commands) {
    let Ok(obj) = objs.get(add.entity) else {
        return;
    };
    if is_under(&obj.path, MONITOR_PATH) {
        commands.entity(add.entity).try_insert(TelecommsMonitor {
            network: "NULL".to_owned(),
            ..default()
        });
    } else if is_under(&obj.path, LOG_BROWSER_PATH) {
        commands
            .entity(add.entity)
            .try_insert(ServerMonitor::default());
    }
}

fn is_multitool(world: &World, item: Entity) -> bool {
    crate::tools::tool_of(world, item).is_some_and(|(k, _)| k == ToolKind::Multitool)
}

fn plain(world: &World, e: Entity) -> String {
    world
        .get::<DmName>(e)
        .map(|n| crate::vending::plain_name(&n.name).to_owned())
        .unwrap_or_default()
}

/// `ui_interact` of a telecomms machine or console that is powered.
pub fn interact(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    let title = plain(world, target);
    let open = if world.get::<Telecomms>(target).is_some() {
        OpenUi::new::<TelecommsUi>(client, mob, target, title)
    } else if world.get::<TelecommsMonitor>(target).is_some() {
        OpenUi::new::<TelecommsMonitorUi>(client, mob, target, title)
    } else if world.get::<ServerMonitor>(target).is_some() {
        OpenUi::new::<ServerMonitorUi>(client, mob, target, title)
    } else {
        return false;
    };
    if world.get::<Unpowered>(target).is_none() {
        world.write_message(open);
    }
    true
}

fn index(params: &UiParams, key: &str) -> Result<usize, UiReject> {
    params
        .get(key)
        .and_then(|v| v.parse::<usize>().ok())
        .ok_or_else(|| UiReject::BadParams(format!("{key}: expected an index")))
}

fn text(params: &UiParams, key: &str) -> Result<String, UiReject> {
    params
        .get(key)
        .map(str::to_owned)
        .ok_or_else(|| UiReject::BadParams(format!("{key}: expected text")))
}

/// tg `text2num(value) * 10` of a kHz value, `None` when it is not a number.
fn tenths(value: &str) -> Option<u16> {
    let khz: f64 = value.trim().parse().ok()?;
    let freq = (khz * 10.0).round();
    (0.0..=f64::from(u16::MAX))
        .contains(&freq)
        .then_some(freq as u16)
}

// ----- the machine window -----

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MachineAct {
    Toggle,
    Id(String),
    Network(String),
    TempFreq(String),
    Freq,
    Delete(u16),
    Unlink(usize),
    Link,
    Buffer,
    Flush,
    Broadcast,
    Receive,
    ChangeFreq(String),
}

type MachineParams = (
    Query<'static, 'static, (&'static Telecomms, Option<&'static DmName>)>,
    Query<'static, 'static, &'static Inventory>,
    Query<'static, 'static, &'static DmTool>,
    Query<'static, 'static, &'static MultitoolBuffer>,
);

impl ServerUi for TelecommsUi {
    type Source = &'static Telecomms;
    type UserParam = MachineParams;
    type Act = MachineAct;

    fn static_data(_: &Telecomms) {}

    fn data(
        m: &Telecomms,
        user: Entity,
        (machines, inventories, tools, buffers): &SystemParamItem<'_, '_, MachineParams>,
    ) -> TelecommsData {
        let name = |e: Entity| {
            machines.get(e).ok().map(|(m, n)| {
                let name = n.map_or("", |n| crate::vending::plain_name(&n.name));
                (name.to_owned(), m.id.clone())
            })
        };
        let held = inventories
            .get(user)
            .ok()
            .and_then(Inventory::active_item)
            .filter(|&h| {
                tools
                    .get(h)
                    .is_ok_and(|t| ToolKind::from_dm(&t.behaviour) == Some(ToolKind::Multitool))
            });
        let multibuff = held
            .and_then(|h| buffers.get(h).ok())
            .and_then(|b| name(b.0))
            .map(|(name, id)| format!("{name} ({id})"));
        TelecommsData {
            kind: match m.kind {
                MachineKind::Bus => "bus",
                MachineKind::Relay => "relay",
                MachineKind::Server => "server",
                _ => "",
            }
            .to_owned(),
            changefrequency: m.change_frequency,
            broadcasting: m.relay_broadcasting,
            receiving: m.relay_receiving,
            minfreq: MIN_FREE_FREQ,
            maxfreq: MAX_FREE_FREQ,
            frequency: m.tempfreq,
            multitool: held.is_some(),
            multibuff,
            toggled: m.toggled,
            id: m.id.clone(),
            network: m.network.clone(),
            prefab: !m.autolinkers.is_empty(),
            linked: m
                .links
                .iter()
                .enumerate()
                .filter_map(|(i, &e)| {
                    let (name, id) = name(e)?;
                    Some(LinkedMachine {
                        index: i + 1,
                        name,
                        id,
                    })
                })
                .collect(),
            frequencies: m.freq_listening.clone(),
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<MachineAct, UiReject> {
        Ok(match action {
            TOGGLE => MachineAct::Toggle,
            ID => MachineAct::Id(text(params, VALUE)?),
            NETWORK => MachineAct::Network(text(params, VALUE)?),
            TEMPFREQ => MachineAct::TempFreq(text(params, VALUE)?),
            FREQ => MachineAct::Freq,
            DELETE => MachineAct::Delete(
                params
                    .get(VALUE)
                    .and_then(|v| v.parse::<u16>().ok())
                    .ok_or_else(|| UiReject::BadParams("value: expected a frequency".into()))?,
            ),
            UNLINK => MachineAct::Unlink(index(params, VALUE)?),
            LINK => MachineAct::Link,
            BUFFER => MachineAct::Buffer,
            FLUSH => MachineAct::Flush,
            BROADCAST => MachineAct::Broadcast,
            RECEIVE => MachineAct::Receive,
            CHANGE_FREQ => MachineAct::ChangeFreq(text(params, VALUE)?),
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

/// `add_new_link`: both ways; `false` for itself, a non-telecomms target or an existing link.
pub fn add_link(world: &mut World, a: Entity, b: Entity) -> bool {
    if a == b || world.get::<Telecomms>(a).is_none() || world.get::<Telecomms>(b).is_none() {
        return false;
    }
    let linked = |world: &World, x: Entity, y: Entity| {
        world
            .get::<Telecomms>(x)
            .is_some_and(|m| m.links.contains(&y))
    };
    if linked(world, a, b) && linked(world, b, a) {
        return false;
    }
    for (from, to) in [(a, b), (b, a)] {
        let mut m = world.get_mut::<Telecomms>(from).expect("checked");
        if !m.links.contains(&to) {
            m.links.push(to);
        }
    }
    true
}

/// `remove_link`: both ways.
pub fn remove_link(world: &mut World, a: Entity, b: Entity) -> bool {
    if a == b {
        return false;
    }
    for (from, to) in [(a, b), (b, a)] {
        if let Some(mut m) = world.get_mut::<Telecomms>(from) {
            m.links.retain(|&l| l != to);
        }
    }
    true
}

/// `ui_act` of `machine` by a user holding `multitool` (none for a silicon). `Err` is tg's
/// error message (said with a buzz).
pub fn apply_machine_act(
    world: &mut World,
    machine: Entity,
    multitool: Option<Entity>,
    act: &MachineAct,
) -> Result<(), String> {
    let Some(m) = world.get::<Telecomms>(machine) else {
        return Ok(());
    };
    let kind = m.kind;
    match act {
        MachineAct::Toggle => {
            let mut m = world.get_mut::<Telecomms>(machine).expect("checked");
            m.toggled = !m.toggled;
            // `update_power` runs next tick (`crate::telecomms`).
        }
        MachineAct::Id(value) => {
            if value.is_empty() {
                return Ok(());
            }
            if value.chars().count() > MAX_MACHINE_ID {
                return Err("Error: Machine ID too long!".to_owned());
            }
            value.clone_into(&mut world.get_mut::<Telecomms>(machine).expect("checked").id);
        }
        MachineAct::Network(value) => {
            if value.is_empty() {
                return Ok(());
            }
            if value.chars().count() > MAX_NETWORK_ID {
                return Err("Error: Network name too long!".to_owned());
            }
            for other in m.links.clone() {
                remove_link(world, machine, other);
            }
            let mut m = world.get_mut::<Telecomms>(machine).expect("checked");
            m.links.clear();
            value.clone_into(&mut m.network);
        }
        MachineAct::TempFreq(value) => {
            if let Some(freq) = tenths(value).filter(|&f| f > 0) {
                world
                    .get_mut::<Telecomms>(machine)
                    .expect("checked")
                    .tempfreq = freq;
            }
        }
        MachineAct::Freq => {
            let freq = m.tempfreq;
            if BANNED_FREQUENCIES.contains(&freq) {
                return Err(format!(
                    "Error: Interference preventing filtering frequency: \"{:.1} kHz\"",
                    f64::from(freq) / 10.0
                ));
            }
            let mut m = world.get_mut::<Telecomms>(machine).expect("checked");
            if !m.freq_listening.contains(&freq) {
                m.freq_listening.push(freq);
            }
        }
        MachineAct::Delete(freq) => {
            world
                .get_mut::<Telecomms>(machine)
                .expect("checked")
                .freq_listening
                .retain(|f| f != freq);
        }
        MachineAct::Unlink(n) => {
            if let Some(&other) = n.checked_sub(1).and_then(|i| m.links.get(i)) {
                remove_link(world, machine, other);
            }
        }
        MachineAct::Link => {
            if let Some(target) = multitool.and_then(|t| multitool_buffer(world, t)) {
                add_link(world, machine, target);
            }
        }
        MachineAct::Buffer => {
            if let Some(tool) = multitool {
                set_multitool_buffer(world, tool, machine);
            }
        }
        MachineAct::Flush => {
            if let Some(tool) = multitool {
                world.entity_mut(tool).remove::<MultitoolBuffer>();
            }
        }
        MachineAct::Broadcast if kind == MachineKind::Relay => {
            let mut m = world.get_mut::<Telecomms>(machine).expect("checked");
            m.relay_broadcasting = !m.relay_broadcasting;
        }
        MachineAct::Receive if kind == MachineKind::Relay => {
            let mut m = world.get_mut::<Telecomms>(machine).expect("checked");
            m.relay_receiving = !m.relay_receiving;
        }
        MachineAct::ChangeFreq(value) if kind == MachineKind::Bus => {
            // `if(newfreq) { if(newfreq < 10000) change_frequency = newfreq else 0 }`.
            if let Some(freq) = tenths(value).filter(|&f| f > 0) {
                world
                    .get_mut::<Telecomms>(machine)
                    .expect("checked")
                    .change_frequency = if freq < 10000 { freq } else { 0 };
            }
        }
        MachineAct::Broadcast | MachineAct::Receive | MachineAct::ChangeFreq(_) => {}
    }
    Ok(())
}

fn machine_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<TelecommsUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, MachineAct)> = acts
        .read(world.resource::<Messages<UiActed<TelecommsUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, machine, act) in pending {
        let held = world
            .get::<Inventory>(actor)
            .and_then(Inventory::active_item)
            .filter(|&h| is_multitool(world, h));
        // `HAS_SILICON_ACCESS` or a multitool in the active hand.
        if held.is_none() && !crate::silicon::interact::has_silicon_access(world, actor) {
            continue;
        }
        if let Err(message) = apply_machine_act(world, machine, held, &act) {
            tell(world, client, Span::Warning, message);
            if let Some(&pos) = world.get::<TilePos>(machine) {
                crate::sound::playsound(world, crate::sound::PlaySound::new(pos, BUZZ, 50.0, true));
            }
        } else {
            info!("telecomms: {actor:?} {act:?} on {machine:?}");
        }
    }
}

// ----- the monitoring console -----

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MonitorAct {
    Flush,
    Home,
    View(String),
    Probe(String),
}

type MonitorParams = (
    Query<
        'static,
        'static,
        (
            &'static Telecomms,
            Option<&'static DmName>,
            Option<&'static DmAtom>,
        ),
    >,
);

fn monitor_machine(
    machines: &Query<(&Telecomms, Option<&DmName>, Option<&DmAtom>)>,
    e: Entity,
) -> Option<MonitorMachine> {
    let (m, name, atom) = machines.get(e).ok()?;
    Some(MonitorMachine {
        id: m.id.clone(),
        name: name
            .map_or("", |n| crate::vending::plain_name(&n.name))
            .to_owned(),
        icon: atom.map(|a| a.icon_state.clone()).unwrap_or_default(),
    })
}

impl ServerUi for TelecommsMonitorUi {
    type Source = &'static TelecommsMonitor;
    type UserParam = MonitorParams;
    type Act = MonitorAct;

    fn static_data(_: &TelecommsMonitor) {}

    fn data(
        c: &TelecommsMonitor,
        _: Entity,
        (machines,): &SystemParamItem<'_, '_, MonitorParams>,
    ) -> TelecommsMonitorData {
        let selected = c.selected.and_then(|e| {
            let (m, ..) = machines.get(e).ok()?;
            let shown = monitor_machine(machines, e)?;
            Some(MonitorSelected {
                id: shown.id,
                name: shown.name,
                network: m.network.clone(),
                linked_machines: m
                    .links
                    .iter()
                    .filter_map(|&l| monitor_machine(machines, l))
                    .collect(),
            })
        });
        TelecommsMonitorData {
            screen: u8::from(selected.is_some()),
            status_message: c.status.clone(),
            network: c.network.clone(),
            machines: c
                .machines
                .iter()
                .filter_map(|&e| monitor_machine(machines, e))
                .collect(),
            machine: selected,
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<MonitorAct, UiReject> {
        Ok(match action {
            FLUSH => MonitorAct::Flush,
            HOME => MonitorAct::Home,
            VIEW => MonitorAct::View(text(params, MACHINE_ID)?),
            PROBE => MonitorAct::Probe(text(params, MACHINE_ID)?),
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

/// `/obj/machinery/computer/telecomms/monitor/ui_act`.
pub fn apply_monitor_act(world: &mut World, console: Entity, act: &MonitorAct) {
    let Some(mut c) = world.get::<TelecommsMonitor>(console).cloned() else {
        return;
    };
    c.status = None;
    // `list_clear_empty_weakrefs`.
    c.machines.retain(|&e| world.get::<Telecomms>(e).is_some());
    match act {
        MonitorAct::Flush => {
            c.machines.clear();
            c.network.clear();
        }
        MonitorAct::Home => c.selected = None,
        MonitorAct::View(id) => {
            if let Some(&e) = c
                .machines
                .iter()
                .rfind(|&&e| world.get::<Telecomms>(e).is_some_and(|m| &m.id == id))
            {
                c.selected = Some(e);
            }
            if c.selected.is_none() {
                c.status = Some("OPERATION FAILED: UNABLE TO LOCATE MACHINERY.".to_owned());
            }
        }
        MonitorAct::Probe(network) => {
            if network.chars().count() > MAX_NETWORK_ID {
                c.status = Some("OPERATION FAILED: NETWORK ID TOO LONG.".to_owned());
            } else if !c.machines.is_empty() {
                c.status = Some("OPERATION FAILED: CANNOT PROBE WHEN BUFFER FULL.".to_owned());
            } else {
                network.clone_into(&mut c.network);
                let mut q = world.query::<(Entity, &Telecomms)>();
                let mut found: Vec<Entity> = q
                    .iter(world)
                    .filter(|(_, m)| &m.network == network)
                    .map(|(e, _)| e)
                    .collect();
                found.sort();
                c.status = Some(if found.is_empty() {
                    format!("OPERATION FAILED: UNABLE TO LOCATE NETWORK ENTITIES IN {network}.")
                } else {
                    format!("{} ENTITIES LOCATED & BUFFERED", found.len())
                });
                c.machines = found;
            }
        }
    }
    if c.selected
        .is_some_and(|e| world.get::<Telecomms>(e).is_none())
    {
        c.selected = None;
    }
    world.entity_mut(console).insert(c);
}

fn monitor_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<TelecommsMonitorUi>>>) {
    let pending: Vec<(Entity, MonitorAct)> = acts
        .read(world.resource::<Messages<UiActed<TelecommsMonitorUi>>>())
        .map(|a| (a.source, a.act.clone()))
        .collect();
    for (console, act) in pending {
        if world.get::<Unpowered>(console).is_none() {
            apply_monitor_act(world, console, &act);
        }
    }
}

// ----- the server log browser -----

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BrowserAct {
    Scan(String),
    ClearBuffer,
    ViewServer(usize),
    ReturnHome,
    DeletePacket(usize),
}

type BrowserParams = (
    Query<
        'static,
        'static,
        (
            &'static Telecomms,
            Option<&'static DmName>,
            Option<&'static ServerLog>,
        ),
    >,
    ss13_chat::server::Understanding<'static, 'static>,
);

impl ServerUi for ServerMonitorUi {
    type Source = &'static ServerMonitor;
    type UserParam = BrowserParams;
    type Act = BrowserAct;

    fn static_data(_: &ServerMonitor) {}

    fn data(
        c: &ServerMonitor,
        user: Entity,
        (servers, understanding): &SystemParamItem<'_, '_, BrowserParams>,
    ) -> ServerMonitorData {
        let name = |n: Option<&DmName>| {
            n.map_or("", |n| crate::vending::plain_name(&n.name))
                .to_owned()
        };
        let server = c.selected.and_then(|e| {
            let (_, n, log) = servers.get(e).ok()?;
            let log = log?;
            let packets = log
                .entries
                .iter()
                .enumerate()
                .map(|(i, entry)| {
                    let message = match entry.language {
                        Some(lang) => {
                            let level = understanding.of_body(user, lang);
                            ss13_chat::language::scramble_paragraph(
                                understanding.seed(),
                                lang,
                                &entry.message,
                                level,
                            )
                        }
                        None => entry.message.clone(),
                    };
                    Packet {
                        name: "data packet (#)".to_owned(),
                        language: entry
                            .language
                            .map_or_else(String::new, |l| format!("{l:?}")),
                        message: format!("\"{message}\""),
                        race: entry.race.to_owned(),
                        source: entry.name.clone(),
                        job: entry.job.clone(),
                        kind: entry.kind.to_owned(),
                        packet_ref: i.to_string(),
                    }
                })
                .collect();
            Some(ServerView {
                name: name(n),
                traffic: log.total_traffic,
                packets,
            })
        });
        ServerMonitorData {
            screen: u8::from(server.is_some()),
            network: c.network.clone(),
            error: c.error.clone(),
            servers: c
                .servers
                .iter()
                .enumerate()
                .filter_map(|(i, &e)| {
                    let (m, n, _) = servers.get(e).ok()?;
                    Some(MonitoredServer {
                        server_ref: i.to_string(),
                        name: name(n),
                        id: m.id.clone(),
                    })
                })
                .collect(),
            server,
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<BrowserAct, UiReject> {
        Ok(match action {
            SCAN_NETWORK => BrowserAct::Scan(text(params, NETWORK_ID)?),
            CLEAR_BUFFER => BrowserAct::ClearBuffer,
            VIEW_SERVER => BrowserAct::ViewServer(index(params, SERVER)?),
            RETURN_HOME => BrowserAct::ReturnHome,
            DELETE_PACKET => BrowserAct::DeletePacket(index(params, PACKET_REF)?),
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

/// `/obj/machinery/computer/telecomms/server/ui_act`; `allowed` is the user's telecomms access.
pub fn apply_browser_act(world: &mut World, console: Entity, act: &BrowserAct, allowed: bool) {
    let Some(mut c) = world.get::<ServerMonitor>(console).cloned() else {
        return;
    };
    c.error.clear();
    c.servers.retain(|&e| world.get::<ServerLog>(e).is_some());
    if c.selected
        .is_some_and(|e| world.get::<ServerLog>(e).is_none())
    {
        c.selected = None;
    }
    match act {
        BrowserAct::Scan(network) => {
            if network.chars().count() > MAX_NETWORK_ID {
                c.error = "OPERATION FAILED: NETWORK ID TOO LONG.".to_owned();
            } else if !c.servers.is_empty() {
                c.error =
                    "OPERATION FAILED: BUFFER ALREADY POPULATED. PLEASE CLEAR THE BUFFER.".into();
            } else {
                network.clone_into(&mut c.network);
                let here = world.get::<TilePos>(console).copied();
                let mut q = world.query::<(Entity, &Telecomms, &TilePos)>();
                let mut found: Vec<Entity> = q
                    .iter(world)
                    .filter(|(_, m, p)| {
                        m.kind == MachineKind::Server
                            && &m.network == network
                            && here.is_some_and(|h| {
                                h.z == p.z
                                    && (h.x - p.x).abs() <= SCAN_RANGE
                                    && (h.y - p.y).abs() <= SCAN_RANGE
                            })
                    })
                    .map(|(e, ..)| e)
                    .collect();
                found.sort();
                if found.is_empty() {
                    c.error = format!(
                        "OPERATION FAILED: UNABLE TO LOCATE ANY SERVERS IN NETWORK {network}."
                    );
                }
                c.servers = found;
            }
        }
        BrowserAct::ClearBuffer => {
            c.servers.clear();
            c.network.clear();
        }
        BrowserAct::ViewServer(n) => match c.servers.get(*n) {
            Some(&e) => c.selected = Some(e),
            None => c.error = "OPERATION FAILED: UNABLE TO LOCATE SERVER.".to_owned(),
        },
        BrowserAct::ReturnHome => c.selected = None,
        BrowserAct::DeletePacket(n) => {
            if let Some(server) = c.selected {
                let exists = world
                    .get::<ServerLog>(server)
                    .is_some_and(|l| *n < l.entries.len());
                if !exists {
                    c.error = "OPERATION FAILED: PACKET NOT FOUND.".to_owned();
                } else if !allowed {
                    c.error = "OPERATION FAILED: ACCESS DENIED.".to_owned();
                } else {
                    world
                        .get_mut::<ServerLog>(server)
                        .expect("checked")
                        .entries
                        .remove(*n);
                    c.error = "SUCCESSFULLY DELETED data packet (#).".to_owned();
                }
            }
        }
    }
    world.entity_mut(console).insert(c);
}

fn browser_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<ServerMonitorUi>>>) {
    let pending: Vec<(Entity, Entity, BrowserAct)> = acts
        .read(world.resource::<Messages<UiActed<ServerMonitorUi>>>())
        .map(|a| (a.actor, a.source, a.act.clone()))
        .collect();
    for (actor, console, act) in pending {
        if world.get::<Unpowered>(console).is_some() {
            continue;
        }
        // `allowed(usr)`: `req_access = list(ACCESS_TCOMMS)`.
        let allowed = crate::silicon::interact::has_silicon_access(world, actor)
            || crate::inventory::check_access(
                &[ACCESS_TCOMMS.to_owned()],
                &[],
                &crate::bots::user_access(world, actor),
            );
        apply_browser_act(world, console, &act, allowed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telecomms::preset;

    const ROOT: &str = "/obj/machinery/telecomms";

    fn machine(world: &mut World, path: &str, pos: TilePos) -> Entity {
        let m = preset(&format!("{ROOT}/{path}")).unwrap();
        let log = (m.kind == MachineKind::Server).then(ServerLog::default);
        let e = world.spawn((m, pos)).id();
        if let Some(log) = log {
            world.entity_mut(e).insert(log);
        }
        e
    }

    #[test]
    fn the_window_configures_and_links_through_the_multitool() {
        let mut world = World::new();
        let bus = machine(&mut world, "bus/preset_one", TilePos::new(1, 1, 1));
        let server = machine(&mut world, "server/presets/science", TilePos::new(2, 1, 1));
        let tool = world.spawn_empty().id();
        let act = |w: &mut World, e, a| apply_machine_act(w, e, Some(tool), &a);
        // Buffer the server, link it from the bus.
        act(&mut world, server, MachineAct::Buffer).unwrap();
        act(&mut world, bus, MachineAct::Link).unwrap();
        assert_eq!(world.get::<Telecomms>(bus).unwrap().links, vec![server]);
        assert_eq!(world.get::<Telecomms>(server).unwrap().links, vec![bus]);
        // Unlink by 1-based index.
        act(&mut world, bus, MachineAct::Unlink(1)).unwrap();
        assert!(world.get::<Telecomms>(server).unwrap().links.is_empty());
        // Frequencies: 145.9 kHz added, the Syndicate frequency refused.
        act(&mut world, bus, MachineAct::TempFreq("145.9".into())).unwrap();
        act(&mut world, bus, MachineAct::Freq).unwrap();
        assert!(
            world
                .get::<Telecomms>(bus)
                .unwrap()
                .freq_listening
                .contains(&1459)
        );
        act(&mut world, bus, MachineAct::TempFreq("121.3".into())).unwrap();
        assert!(act(&mut world, bus, MachineAct::Freq).is_err());
        act(&mut world, bus, MachineAct::Delete(1459)).unwrap();
        assert!(
            !world
                .get::<Telecomms>(bus)
                .unwrap()
                .freq_listening
                .contains(&1459)
        );
        // ID and network limits; a new network drops the links.
        act(&mut world, server, MachineAct::Buffer).unwrap();
        act(&mut world, bus, MachineAct::Link).unwrap();
        assert!(act(&mut world, bus, MachineAct::Id("x".repeat(33))).is_err());
        act(&mut world, bus, MachineAct::Id("Bus X".into())).unwrap();
        assert!(act(&mut world, bus, MachineAct::Network("x".repeat(16))).is_err());
        act(&mut world, bus, MachineAct::Network("hacked".into())).unwrap();
        let m = world.get::<Telecomms>(bus).unwrap();
        assert_eq!((m.id.as_str(), m.network.as_str()), ("Bus X", "hacked"));
        assert!(m.links.is_empty() && world.get::<Telecomms>(server).unwrap().links.is_empty());
        // Bus frequency change; the power toggle.
        act(&mut world, bus, MachineAct::ChangeFreq("135.9".into())).unwrap();
        assert_eq!(world.get::<Telecomms>(bus).unwrap().change_frequency, 1359);
        act(&mut world, bus, MachineAct::Toggle).unwrap();
        assert!(!world.get::<Telecomms>(bus).unwrap().toggled);
        // Flush empties the buffer: linking does nothing.
        act(&mut world, bus, MachineAct::Flush).unwrap();
        act(&mut world, server, MachineAct::Link).unwrap();
        assert!(world.get::<Telecomms>(server).unwrap().links.is_empty());
    }

    #[test]
    fn the_monitor_probes_a_network_and_shows_a_machine() {
        let mut world = World::new();
        let hub = machine(&mut world, "hub/preset", TilePos::new(1, 1, 1));
        let bus = machine(&mut world, "bus/preset_one", TilePos::new(2, 1, 1));
        add_link(&mut world, hub, bus);
        let console = world
            .spawn(TelecommsMonitor {
                network: "NULL".into(),
                ..default()
            })
            .id();
        apply_monitor_act(&mut world, console, &MonitorAct::Probe("nothing".into()));
        let c = world.get::<TelecommsMonitor>(console).unwrap();
        assert!(c.machines.is_empty() && c.status.as_deref().unwrap().contains("UNABLE"));
        apply_monitor_act(&mut world, console, &MonitorAct::Probe("tcommsat".into()));
        let c = world.get::<TelecommsMonitor>(console).unwrap();
        assert_eq!(c.machines, vec![hub, bus]);
        assert_eq!(c.status.as_deref(), Some("2 ENTITIES LOCATED & BUFFERED"));
        apply_monitor_act(&mut world, console, &MonitorAct::Probe("tcommsat".into()));
        assert!(
            world
                .get::<TelecommsMonitor>(console)
                .unwrap()
                .status
                .as_deref()
                .unwrap()
                .contains("BUFFER FULL")
        );
        apply_monitor_act(&mut world, console, &MonitorAct::View("Hub".into()));
        assert_eq!(
            world.get::<TelecommsMonitor>(console).unwrap().selected,
            Some(hub)
        );
        apply_monitor_act(&mut world, console, &MonitorAct::Home);
        apply_monitor_act(&mut world, console, &MonitorAct::Flush);
        assert!(
            world
                .get::<TelecommsMonitor>(console)
                .unwrap()
                .machines
                .is_empty()
        );
    }

    #[test]
    fn the_log_browser_scans_reads_and_deletes_with_access() {
        let mut world = World::new();
        let near = machine(&mut world, "server/presets/common", TilePos::new(10, 10, 2));
        machine(
            &mut world,
            "server/presets/science",
            TilePos::new(40, 10, 2),
        );
        let mut rng = CombatRng(3);
        let entry = LogEntry::new("Капитан", "Привет", None, "Humanoid", 0, &mut rng);
        assert_eq!(entry.kind, "Speech File");
        world.get_mut::<ServerLog>(near).unwrap().push(entry);
        let corrupt = LogEntry::new("Капитан", "Привет всем", None, "Humanoid", 50, &mut rng);
        assert_eq!(corrupt.kind, "Corrupt File");
        let console = world
            .spawn((ServerMonitor::default(), TilePos::new(12, 12, 2)))
            .id();
        let act = |w: &mut World, a, allowed| apply_browser_act(w, console, &a, allowed);
        act(&mut world, BrowserAct::Scan("tcommsat".into()), false);
        // Only the server within 25 tiles.
        assert_eq!(
            world.get::<ServerMonitor>(console).unwrap().servers,
            vec![near]
        );
        act(&mut world, BrowserAct::ViewServer(0), false);
        assert_eq!(
            world.get::<ServerMonitor>(console).unwrap().selected,
            Some(near)
        );
        act(&mut world, BrowserAct::DeletePacket(0), false);
        assert!(
            world
                .get::<ServerMonitor>(console)
                .unwrap()
                .error
                .contains("ACCESS DENIED")
        );
        assert_eq!(world.get::<ServerLog>(near).unwrap().entries.len(), 1);
        act(&mut world, BrowserAct::DeletePacket(0), true);
        assert!(world.get::<ServerLog>(near).unwrap().entries.is_empty());
        act(&mut world, BrowserAct::DeletePacket(0), true);
        assert!(
            world
                .get::<ServerMonitor>(console)
                .unwrap()
                .error
                .contains("NOT FOUND")
        );
        // The log keeps the newest 400.
        let mut log = ServerLog::default();
        for i in 0..=MAX_LOG_ENTRIES {
            log.push(LogEntry::new(
                &i.to_string(),
                "",
                None,
                "Humanoid",
                0,
                &mut rng,
            ));
        }
        assert_eq!(log.entries.len(), MAX_LOG_ENTRIES);
        assert_eq!(log.entries[0].name, "1");
    }
}
