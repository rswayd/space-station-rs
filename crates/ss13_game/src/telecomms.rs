//! Telecommunications (server): the `/obj/machinery/telecomms` network a subspace signal crosses
//! (`code/game/machinery/telecomms/`), after tg's `relay_information` / `receive_information`
//! chain.
//!
//! - Mapped telecomms machines get a [`Telecomms`] from their type ([`preset`]: id, autolinkers,
//!   listened frequencies, `toggled`) and link to every machine sharing an autolinker in reach
//!   (`post_machine_initialize`: both long range, within 20 tiles or on a connected level).
//! - [`Telecomms::on`] follows `update_power`: toggled and neither unpowered, `EMPED` nor broken.
//! - [`Net::route`] runs a signal through the machines the way tg's procs pass it: receivers
//!   (`send_to_receivers`) → hub or bus → processor (decompresses) → bus → server (or the message
//!   server for PDA and request console messages) → hub → relays (add their levels) and
//!   broadcasters (add theirs and broadcast). A processor that is off or missing leaves the voice
//!   compressed: the words are garbled ([`gibberish`]). The all-in-one mainframe receives and
//!   broadcasts on its own.
//! - `crate::radio` routes every radio transmission through [`Net::route`]; a map without any
//!   telecomms machine keeps the radio's whole-level network (see [`Net::is_empty`]).
//!
//! Cost: a route visits each linked machine at most a handful of times (MetaStation: 25 machines,
//! ~60 links); the [`Net`] snapshot is rebuilt per radio tick that has transmissions.
// shortcut: the presets' vars come from the type table in [`preset`] (map var edits of
// `autolinkers`, `freq_listening` and `id` are not read; MetaStation has none). No traffic and
// network lag (`traffic`, `netspeed`, `data["slow"]`), no compressing processors;
// `recent_messages` dedupes only within one signal. Configuration, monitoring and server logs:
// `crate::telecomms_config`.

use std::ops::RangeInclusive;

use bevy::prelude::*;
use ss13_chat::radio::{
    FREQ_COMMAND, FREQ_ENGINEERING, FREQ_ENTERTAINMENT, FREQ_JUSTICE, FREQ_MEDICAL, FREQ_SCIENCE,
    FREQ_SECURITY, FREQ_SERVICE, FREQ_SUPPLY, FREQ_SYNDICATE, MAX_FREQ, MIN_FREQ,
};
use ss13_mapload::DmObj;
use ss13_mapload::fidelity::is_under;
use ss13_shared::TilePos;

use crate::combat::CombatRng;
use crate::emp::Emped;
use crate::obj_damage::Integrity;
use crate::power::Unpowered;

pub const TELECOMMS_ROOT: &str = "/obj/machinery/telecomms";
pub const PROCESSOR_ROOT: &str = "/obj/machinery/telecomms/processor";
/// `post_machine_initialize`: machines link within this `GET_CARDINAL_DIST`.
const LINK_DISTANCE: i32 = 20;
/// `relay_information(amount = 20)`.
const MAX_SENDS: usize = 20;
/// `COMPRESSION_VOCAL_SIGNAL_MIN`, `COMPRESSION_VOCAL_SIGNAL_MAX`.
pub const COMPRESSION_MIN: u32 = 35;
pub const COMPRESSION_MAX: u32 = 65;
/// `COMPRESSION_REPLACE_CHARACTER_THRESHOLD`.
pub const COMPRESSION_REPLACE_THRESHOLD: u8 = 30;
/// `/obj/machinery/telecomms/message_server` `calibrating = 15 MINUTES` (presets: 0).
pub const CALIBRATION_SECS: f64 = 15.0 * 60.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MachineKind {
    Receiver,
    Bus,
    Processor,
    Server,
    Hub,
    Relay,
    Broadcaster,
    MessageServer,
    AllInOne,
}

impl MachineKind {
    /// The machine's `telecomms_type` from its path.
    pub fn of(path: &str) -> Option<Self> {
        const ROOTS: [(&str, MachineKind); 9] = [
            ("/obj/machinery/telecomms/receiver", MachineKind::Receiver),
            ("/obj/machinery/telecomms/bus", MachineKind::Bus),
            (PROCESSOR_ROOT, MachineKind::Processor),
            ("/obj/machinery/telecomms/server", MachineKind::Server),
            ("/obj/machinery/telecomms/hub", MachineKind::Hub),
            ("/obj/machinery/telecomms/relay", MachineKind::Relay),
            (
                "/obj/machinery/telecomms/broadcaster",
                MachineKind::Broadcaster,
            ),
            (
                "/obj/machinery/telecomms/message_server",
                MachineKind::MessageServer,
            ),
            ("/obj/machinery/telecomms/allinone", MachineKind::AllInOne),
        ];
        ROOTS
            .iter()
            .find(|(root, _)| is_under(path, root))
            .map(|&(_, kind)| kind)
    }
}

/// A telecomms machine's live state (tg `/obj/machinery/telecomms` vars).
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Telecomms {
    pub kind: MachineKind,
    pub id: String,
    pub autolinkers: Vec<&'static str>,
    /// `freq_listening`: empty listens to every frequency.
    pub freq_listening: Vec<u16>,
    /// Meant to be active (`toggled`); [`Self::on`] is whether it is (`on`).
    pub toggled: bool,
    pub on: bool,
    /// Links across levels and any distance (hubs and relays).
    pub long_range_link: bool,
    /// `links`, both ways, in link order.
    pub links: Vec<Entity>,
    /// Relays: `broadcasting`, `receiving`.
    pub relay_broadcasting: bool,
    pub relay_receiving: bool,
    /// The all-in-one `syndicate` mainframe: hears its frequencies on every level and broadcasts
    /// everywhere.
    pub syndicate: bool,
    /// Message servers: working from this server time (`calibrating`).
    pub calibrated_at: f64,
    /// `network`: what the monitoring consoles probe for (`"tcommsat"` for the presets).
    pub network: String,
    /// `tempfreq`: the frequency the configuration window adds to `freq_listening`.
    pub tempfreq: u16,
    /// Buses: `change_frequency`, `0` for none.
    pub change_frequency: u16,
}

/// `banned_frequencies`: never listened to, rewritten to or logged.
pub const BANNED_FREQUENCIES: [u16; 6] = [
    FREQ_SYNDICATE,
    ss13_chat::radio::FREQ_CENTCOM,
    ss13_chat::radio::FREQ_CTF_RED,
    ss13_chat::radio::FREQ_CTF_YELLOW,
    ss13_chat::radio::FREQ_CTF_GREEN,
    ss13_chat::radio::FREQ_CTF_BLUE,
];

/// The type's vars (tg presets) for `path`.
pub fn preset(path: &str) -> Option<Telecomms> {
    let kind = MachineKind::of(path)?;
    let under = |root: &str| is_under(path, root);
    let free: Vec<u16> = (MIN_FREQ..=MAX_FREQ).step_by(2).collect();
    let with_free = |mut list: Vec<u16>| {
        for f in &free {
            if !list.contains(f) {
                list.push(*f);
            }
        }
        list
    };
    let mut m = Telecomms {
        kind,
        id: "NULL".to_owned(),
        autolinkers: Vec::new(),
        freq_listening: Vec::new(),
        toggled: true,
        on: true,
        long_range_link: matches!(kind, MachineKind::Hub | MachineKind::Relay),
        links: Vec::new(),
        relay_broadcasting: true,
        relay_receiving: true,
        syndicate: false,
        calibrated_at: 0.0,
        network: if path.contains("/preset") {
            "tcommsat"
        } else {
            "NULL"
        }
        .to_owned(),
        tempfreq: ss13_chat::radio::FREQ_COMMON,
        change_frequency: 0,
    };
    let mut set = |id: &str, links: &[&'static str], freqs: Vec<u16>| {
        id.clone_into(&mut m.id);
        m.autolinkers = links.to_vec();
        m.freq_listening = freqs;
    };
    let rec = "/obj/machinery/telecomms/receiver";
    let bus = "/obj/machinery/telecomms/bus";
    let srv = "/obj/machinery/telecomms/server/presets";
    if under(&format!("{rec}/preset_left/birdstation")) {
        set("Receiver A", &["receiverA"], Vec::new());
    } else if under(&format!("{rec}/preset_left")) {
        let freqs = vec![
            FREQ_SCIENCE,
            FREQ_MEDICAL,
            FREQ_SUPPLY,
            FREQ_SERVICE,
            FREQ_ENTERTAINMENT,
        ];
        set("Receiver A", &["receiverA"], freqs);
    } else if under(&format!("{rec}/preset_right")) {
        let freqs = vec![FREQ_COMMAND, FREQ_ENGINEERING, FREQ_SECURITY, FREQ_JUSTICE];
        set("Receiver B", &["receiverB"], with_free(freqs));
    } else if under(&format!("{bus}/preset_one/birdstation")) {
        set("Bus 1", &["processor1", "common", "messaging"], Vec::new());
    } else if under(&format!("{bus}/preset_one")) {
        let links = ["processor1", "science", "medical"];
        set("Bus 1", &links, vec![FREQ_SCIENCE, FREQ_MEDICAL]);
    } else if under(&format!("{bus}/preset_two")) {
        let links = ["processor2", "supply", "service", "entertainment"];
        let freqs = vec![FREQ_SUPPLY, FREQ_SERVICE, FREQ_ENTERTAINMENT];
        set("Bus 2", &links, freqs);
    } else if under(&format!("{bus}/preset_three")) {
        let links = ["processor3", "security", "command", "justice"];
        let freqs = vec![FREQ_SECURITY, FREQ_COMMAND, FREQ_JUSTICE];
        set("Bus 3", &links, freqs);
    } else if under(&format!("{bus}/preset_four")) {
        let links = ["processor4", "engineering", "common", "messaging"];
        set("Bus 4", &links, with_free(vec![FREQ_ENGINEERING]));
    } else if let Some(n) = ["one", "two", "three", "four"]
        .iter()
        .position(|n| under(&format!("{PROCESSOR_ROOT}/preset_{n}")))
    {
        const LINKS: [&str; 4] = ["processor1", "processor2", "processor3", "processor4"];
        set(&format!("Processor {}", n + 1), &[LINKS[n]], Vec::new());
    } else if under(&format!("{srv}/common/birdstation")) {
        set("Common & AI Server", &["common"], Vec::new());
    } else if under(&format!("{srv}/common")) {
        set("Common & AI Server", &["common"], free.clone());
    } else if under(&format!("{srv}/science")) {
        set("Science Server", &["science"], vec![FREQ_SCIENCE]);
    } else if under(&format!("{srv}/medical")) {
        set("Medical Server", &["medical"], vec![FREQ_MEDICAL]);
    } else if under(&format!("{srv}/supply")) {
        set("Supply Server", &["supply"], vec![FREQ_SUPPLY]);
    } else if under(&format!("{srv}/service")) {
        let freqs = vec![FREQ_SERVICE, FREQ_ENTERTAINMENT];
        set(
            "Service & Entertainment Server",
            &["service", "entertainment"],
            freqs,
        );
    } else if under(&format!("{srv}/command")) {
        set("Command Server", &["command"], vec![FREQ_COMMAND]);
    } else if under(&format!("{srv}/engineering")) {
        set(
            "Engineering Server",
            &["engineering"],
            vec![FREQ_ENGINEERING],
        );
    } else if under(&format!("{srv}/security")) {
        let freqs = vec![FREQ_SECURITY, FREQ_JUSTICE];
        set("Security Server", &["security", "justice"], freqs);
    } else if under("/obj/machinery/telecomms/hub/preset") {
        const HUB: [&str; 20] = [
            "hub",
            "relay",
            "s_relay",
            "m_relay",
            "r_relay",
            "science",
            "medical",
            "supply",
            "service",
            "common",
            "command",
            "engineering",
            "entertainment",
            "security",
            "receiverA",
            "receiverB",
            "broadcasterA",
            "broadcasterB",
            "autorelay",
            "messaging",
        ];
        set("Hub", &HUB, Vec::new());
    } else if under("/obj/machinery/telecomms/relay/preset/station") {
        set("Station Relay", &["s_relay"], Vec::new());
    } else if under("/obj/machinery/telecomms/relay/preset/telecomms") {
        set("Telecomms Relay", &["relay"], Vec::new());
    } else if under("/obj/machinery/telecomms/relay/preset/mining") {
        set("Mining Relay", &["m_relay"], Vec::new());
    } else if under("/obj/machinery/telecomms/relay/preset/ruskie") {
        set("Ruskie Relay", &["r_relay"], Vec::new());
        m.toggled = false;
        m.on = false;
    } else if under("/obj/machinery/telecomms/relay/preset/auto") {
        set("NULL", &["autorelay"], Vec::new());
    } else if under("/obj/machinery/telecomms/broadcaster/preset_left") {
        set("Broadcaster A", &["broadcasterA"], Vec::new());
    } else if under("/obj/machinery/telecomms/broadcaster/preset_right") {
        set("Broadcaster B", &["broadcasterB"], Vec::new());
    } else if under("/obj/machinery/telecomms/message_server/preset") {
        set("Messaging Server", &["messaging"], Vec::new());
    } else if under("/obj/machinery/telecomms/allinone/nuclear") {
        m.freq_listening = vec![FREQ_SYNDICATE];
        m.syndicate = true;
    }
    if kind == MachineKind::MessageServer
        && !under("/obj/machinery/telecomms/message_server/preset")
    {
        // Made relative to the server's start in [`setup_machine`].
        m.calibrated_at = CALIBRATION_SECS;
    }
    Some(m)
}

impl Telecomms {
    /// `is_freq_listening`.
    pub fn listens(&self, freq: u16) -> bool {
        self.freq_listening.is_empty() || self.freq_listening.contains(&freq)
    }
}

/// The station's levels (tg `ZTRAIT_STATION`), for `get_connected_levels` and the relays.
#[derive(Resource, Clone, Debug, PartialEq, Eq)]
pub struct TelecommsLevels(pub RangeInclusive<u16>);

impl Default for TelecommsLevels {
    fn default() -> Self {
        #[allow(clippy::reversed_empty_ranges)]
        Self(1..=0)
    }
}

impl TelecommsLevels {
    /// `SSmapping.get_connected_levels(z)`: the station's levels form one stack.
    // shortcut: off-station levels are each their own stack (no multi-z away sites).
    pub fn connected(&self, z: u16) -> Vec<u16> {
        if self.0.contains(&z) {
            self.0.clone().collect()
        } else {
            vec![z]
        }
    }

    pub fn same_stack(&self, a: u16, b: u16) -> bool {
        a == b || (self.0.contains(&a) && self.0.contains(&b))
    }
}

pub struct TelecommsPlugin;

impl Plugin for TelecommsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            crate::telecomms_config::TelecommsConfigPlugin,
            crate::blackbox::BlackboxPlugin,
            crate::ntnet_relay::NtnetRelayPlugin,
        ));
        app.init_resource::<TelecommsLevels>()
            .add_observer(setup_machine)
            .add_systems(
                Update,
                (refresh_levels, autolink, update_power)
                    .chain()
                    .before(TelecommsSystems),
            );
    }
}

/// Runs after the machines' links and `on` are up to date this tick.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TelecommsSystems;

fn setup_machine(
    add: On<Add<DmObj>>,
    objs: Query<&DmObj>,
    time: Res<Time>,
    mut commands: Commands,
) {
    let Ok(obj) = objs.get(add.entity) else {
        return;
    };
    if !is_under(&obj.path, TELECOMMS_ROOT) {
        return;
    }
    if let Some(mut machine) = preset(&obj.path) {
        if machine.calibrated_at > 0.0 {
            machine.calibrated_at += time.elapsed_secs_f64();
        }
        commands.entity(add.entity).try_insert((machine, Unlinked));
    }
}

/// A machine that has not autolinked yet (tg `INITIALIZE_HINT_LATELOAD`).
#[derive(Component)]
struct Unlinked;

fn refresh_levels(world: &mut World) {
    let levels = crate::events::controller::station_levels(world);
    if world.resource::<TelecommsLevels>().0 != levels {
        world.insert_resource(TelecommsLevels(levels));
    }
}

/// `post_machine_initialize` of new machines: a link (both ways) to every machine sharing an
/// autolinker, if both link long range, or it is within 20 tiles, or on a connected level.
fn autolink(
    new: Query<Entity, (With<Unlinked>, With<TilePos>)>,
    mut machines: Query<(Entity, &mut Telecomms, &TilePos)>,
    levels: Res<TelecommsLevels>,
    mut commands: Commands,
) {
    if new.is_empty() {
        return;
    }
    let mut all: Vec<(Entity, Vec<&'static str>, bool, TilePos)> = machines
        .iter()
        .map(|(e, m, p)| (e, m.autolinkers.clone(), m.long_range_link, *p))
        .collect();
    all.sort_by_key(|(e, ..)| *e);
    let mut pairs: Vec<(Entity, Entity)> = Vec::new();
    for e in &new {
        commands.entity(e).remove::<Unlinked>();
        let Some((_, links, long, pos)) = all.iter().find(|a| a.0 == e) else {
            continue;
        };
        if links.is_empty() {
            continue;
        }
        for (other, other_links, other_long, other_pos) in &all {
            if *other == e {
                continue;
            }
            let near = (pos.x - other_pos.x).abs().max((pos.y - other_pos.y).abs())
                <= LINK_DISTANCE
                || levels.same_stack(pos.z, other_pos.z);
            if !(*long && *other_long) && !near {
                continue;
            }
            if links.iter().any(|l| other_links.contains(l)) {
                pairs.push((e, *other));
            }
        }
    }
    for (a, b) in pairs {
        for (from, to) in [(a, b), (b, a)] {
            if let Ok((_, mut m, _)) = machines.get_mut(from)
                && !m.links.contains(&to)
            {
                m.links.push(to);
            }
        }
    }
}

/// `update_power`: on while toggled and neither broken, unpowered nor `EMPED`; message servers
/// finish calibrating (`process`).
fn update_power(
    mut machines: Query<(
        &mut Telecomms,
        Has<Unpowered>,
        Option<&Emped>,
        Option<&Integrity>,
    )>,
    time: Res<Time>,
) {
    let now = time.elapsed_secs_f64();
    for (mut m, unpowered, emped, integrity) in &mut machines {
        let on = m.toggled
            && !unpowered
            && emped.is_none_or(|e| e.until <= now)
            && integrity.is_none_or(|i| !i.broken);
        if m.on != on {
            m.on = on;
        }
    }
}

/// `get_connected_levels` plus tg's `0` (every level).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Levels {
    pub all: bool,
    pub z: Vec<u16>,
}

impl Levels {
    pub fn of(z: impl IntoIterator<Item = u16>) -> Self {
        let mut levels = Self::default();
        levels.add(z);
        levels
    }

    pub fn contains(&self, z: u16) -> bool {
        self.all || self.z.contains(&z)
    }

    fn add(&mut self, z: impl IntoIterator<Item = u16>) {
        for z in z {
            if !self.z.contains(&z) {
                self.z.push(z);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignalKind {
    /// `/datum/signal/subspace/vocal`: radio speech, logged by servers.
    Vocal,
    /// `/datum/signal/subspace/messaging`: PDA and request console messages, refused until a
    /// message server accepts them (`data["reject"]`).
    Message,
}

/// A subspace signal on its way (`/datum/signal/subspace`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Signal {
    pub kind: SignalKind,
    pub freq: u16,
    /// Where it was sent from, then (after a receiver) where it will be heard.
    pub levels: Levels,
    /// `data["compression"]`.
    pub compression: u8,
    reject: bool,
}

impl Signal {
    pub fn vocal(freq: u16, levels: Levels, compression: u8) -> Self {
        Self {
            kind: SignalKind::Vocal,
            freq,
            levels,
            compression,
            reject: false,
        }
    }

    /// `/datum/signal/subspace/messaging` (`frequency = FREQ_COMMON`).
    pub fn message(levels: Levels) -> Self {
        Self {
            kind: SignalKind::Message,
            freq: ss13_chat::radio::FREQ_COMMON,
            levels,
            compression: 0,
            reject: true,
        }
    }

    fn server(&self) -> MachineKind {
        match self.kind {
            SignalKind::Vocal => MachineKind::Server,
            SignalKind::Message => MachineKind::MessageServer,
        }
    }
}

/// What a broadcaster sent out (`signal.broadcast()`), the first one only (`recent_messages`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Broadcast {
    pub levels: Levels,
    pub compression: u8,
    /// The frequency it went out on: a bus's `change_frequency` rewrites it.
    pub freq: u16,
}

/// A route's outcome: the broadcast and the servers that logged the signal, with its
/// compression at that point (`/obj/machinery/telecomms/server/receive_information`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Routed {
    pub broadcast: Option<Broadcast>,
    pub logged: Vec<(Entity, u8)>,
}

struct Node {
    entity: Entity,
    m: Telecomms,
    pos: TilePos,
    links: Vec<usize>,
    calibrating: bool,
}

/// A snapshot of the machines for routing.
pub struct Net {
    nodes: Vec<Node>,
    levels: TelecommsLevels,
}

impl Net {
    pub fn new<'a>(
        machines: impl IntoIterator<Item = (Entity, &'a Telecomms, &'a TilePos)>,
        levels: &TelecommsLevels,
        now: f64,
    ) -> Self {
        let mut list: Vec<(Entity, &Telecomms, TilePos)> =
            machines.into_iter().map(|(e, m, p)| (e, m, *p)).collect();
        list.sort_by_key(|(e, ..)| *e);
        let index = |e: Entity| list.iter().position(|(x, ..)| *x == e);
        let nodes = list
            .iter()
            .map(|(e, m, pos)| Node {
                entity: *e,
                links: m.links.iter().filter_map(|&l| index(l)).collect(),
                calibrating: m.calibrated_at > now,
                m: (*m).clone(),
                pos: *pos,
            })
            .collect();
        Self {
            nodes,
            levels: levels.clone(),
        }
    }

    /// The map has no telecomms machines: radios keep the whole-level network of
    /// `crate::radio` (test maps, maps without a telecomms room).
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// `get_connected_levels(z)`.
    pub fn connected(&self, z: u16) -> Levels {
        Levels::of(self.levels.connected(z))
    }

    /// `send_to_receivers()` and everything after it: the broadcast, if any machine broadcast
    /// the signal (`data["done"]`).
    pub fn route(&self, signal: &Signal) -> Option<Broadcast> {
        self.route_logged(signal).broadcast
    }

    /// [`Self::route`] with the servers that logged the signal.
    pub fn route_logged(&self, signal: &Signal) -> Routed {
        let mut router = Router {
            net: self,
            out: None,
            logged: Vec::new(),
        };
        for (i, node) in self.nodes.iter().enumerate() {
            match node.m.kind {
                MachineKind::Receiver => router.receiver(i, signal),
                MachineKind::AllInOne => router.all_in_one(i, signal),
                _ => {}
            }
        }
        Routed {
            broadcast: router.out,
            logged: router.logged,
        }
    }
}

struct Router<'a> {
    net: &'a Net,
    out: Option<Broadcast>,
    logged: Vec<(Entity, u8)>,
}

impl Router<'_> {
    /// `/obj/machinery/telecomms/receiver/receive_signal`.
    fn receiver(&mut self, i: usize, signal: &Signal) {
        let net = self.net;
        let node = &net.nodes[i];
        if !node.m.on || !node.m.listens(signal.freq) || !self.receives_level(i, signal) {
            return;
        }
        let mut copy = signal.clone();
        copy.levels = Levels::default();
        if self.relay(i, &mut copy, MachineKind::Hub, false) == 0 {
            self.relay(i, &mut copy, MachineKind::Bus, false);
        }
    }

    /// `check_receive_level`: on the signal's level, or a linked hub has a linked relay that
    /// receives it there.
    fn receives_level(&self, i: usize, signal: &Signal) -> bool {
        let net = self.net;
        if signal.levels.contains(net.nodes[i].pos.z) {
            return true;
        }
        net.nodes[i]
            .links
            .iter()
            .filter(|&&h| net.nodes[h].m.kind == MachineKind::Hub)
            .flat_map(|&h| net.nodes[h].links.iter())
            .any(|&r| {
                let relay = &net.nodes[r];
                relay.m.kind == MachineKind::Relay
                    && relay.m.on
                    && relay.m.listens(signal.freq)
                    && relay.m.relay_receiving
                    && signal.levels.contains(relay.pos.z)
            })
    }

    /// `/obj/machinery/telecomms/allinone/receive_signal`.
    fn all_in_one(&mut self, i: usize, signal: &Signal) {
        let net = self.net;
        let node = &net.nodes[i];
        if !node.m.on
            || !node.m.listens(signal.freq)
            || (!node.m.syndicate && !signal.levels.contains(node.pos.z))
        {
            return;
        }
        let mut copy = signal.clone();
        if node.m.syndicate {
            copy.levels = Levels {
                all: true,
                z: Vec::new(),
            };
        }
        copy.compression = 0;
        if self.out.is_none() {
            self.out = Some(Broadcast {
                levels: copy.levels,
                compression: 0,
                freq: copy.freq,
            });
        }
    }

    /// `relay_information(signal, filter, copysig)`: the number of machines it went to.
    fn relay(
        &mut self,
        from: usize,
        signal: &mut Signal,
        filter: MachineKind,
        copy: bool,
    ) -> usize {
        let net = self.net;
        let source = &net.nodes[from];
        if !source.m.on {
            return 0;
        }
        let mut sent = 0;
        for &to in &source.links {
            let target = &net.nodes[to];
            if target.m.kind != filter || !target.m.on {
                continue;
            }
            if sent >= MAX_SENDS {
                break;
            }
            if !net.levels.same_stack(source.pos.z, target.pos.z)
                && !source.m.long_range_link
                && !target.m.long_range_link
            {
                continue;
            }
            sent += 1;
            if copy {
                let mut c = signal.clone();
                self.receive(to, &mut c, from);
            } else {
                self.receive(to, signal, from);
            }
        }
        sent
    }

    /// `receive_information(signal, machine_from)` of machine `i`.
    fn receive(&mut self, i: usize, signal: &mut Signal, from: usize) {
        let net = self.net;
        let node = &net.nodes[i];
        let from_kind = net.nodes[from].m.kind;
        match node.m.kind {
            MachineKind::Bus => {
                if !node.m.listens(signal.freq) {
                    return;
                }
                if node.m.change_frequency != 0 && !BANNED_FREQUENCIES.contains(&signal.freq) {
                    signal.freq = node.m.change_frequency;
                }
                if from_kind != MachineKind::Processor
                    && from != i
                    && self.relay(i, signal, MachineKind::Processor, false) > 0
                {
                    return;
                }
                for kind in [signal.server(), MachineKind::Hub, MachineKind::Broadcaster] {
                    if self.relay(i, signal, kind, false) > 0 {
                        break;
                    }
                }
            }
            MachineKind::Processor => {
                if !node.m.listens(signal.freq) {
                    return;
                }
                // Decompresses (`COMPRESSION_AMOUNT_DECOMPRESSING`).
                signal.compression = 0;
                if from_kind == MachineKind::Bus {
                    self.receive(from, signal, i);
                } else {
                    self.relay(i, signal, signal.server(), false);
                }
            }
            MachineKind::Server => {
                if signal.kind != SignalKind::Vocal || !node.m.listens(signal.freq) {
                    return;
                }
                if !BANNED_FREQUENCIES.contains(&signal.freq) {
                    self.logged.push((node.entity, signal.compression));
                }
                if self.relay(i, signal, MachineKind::Hub, false) == 0 {
                    self.relay(i, signal, MachineKind::Broadcaster, false);
                }
            }
            MachineKind::MessageServer => {
                if signal.kind != SignalKind::Message || !node.m.on || node.calibrating {
                    return;
                }
                signal.reject = false;
                if self.relay(i, signal, MachineKind::Hub, false) == 0 {
                    self.relay(i, signal, MachineKind::Broadcaster, false);
                }
            }
            MachineKind::Hub => {
                if !node.m.listens(signal.freq) {
                    return;
                }
                if from_kind == MachineKind::Receiver {
                    self.relay(i, signal, MachineKind::Bus, true);
                } else {
                    self.relay(i, signal, MachineKind::Relay, false);
                    self.relay(i, signal, MachineKind::Broadcaster, false);
                }
            }
            MachineKind::Relay => {
                if node.m.on && node.m.listens(signal.freq) && node.m.relay_broadcasting {
                    // Relays on the station send to every station level.
                    signal.levels.add(net.levels.connected(node.pos.z));
                }
            }
            MachineKind::Broadcaster => {
                if signal.reject || self.out.is_some() {
                    return;
                }
                signal.levels.add(net.levels.connected(node.pos.z));
                self.out = Some(Broadcast {
                    levels: signal.levels.clone(),
                    compression: signal.compression,
                    freq: signal.freq,
                });
            }
            MachineKind::Receiver | MachineKind::AllInOne => {}
        }
    }
}

/// The routing snapshot of `world`'s machines.
pub fn net(world: &mut World) -> Net {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let levels = world
        .get_resource::<TelecommsLevels>()
        .cloned()
        .unwrap_or_default();
    let mut q = world.query::<(Entity, &Telecomms, &TilePos)>();
    Net::new(q.iter(world), &levels, now)
}

/// A PDA or request console message sent from `from`: `None` without telecomms machines (it
/// always goes through), else whether a broadcaster sent it out (`data["done"]`).
pub fn deliver_message(world: &mut World, from: Option<TilePos>) -> Option<bool> {
    let net = net(world);
    if net.is_empty() {
        return None;
    }
    let levels = from.map_or_else(Levels::default, |p| net.connected(p.z));
    Some(net.route(&Signal::message(levels)).is_some())
}

/// A vocal signal's starting compression (`rand(35, 65)`).
pub fn vocal_compression(rng: &mut CombatRng) -> u8 {
    rng.range(COMPRESSION_MIN, COMPRESSION_MAX) as u8
}

/// tg `Gibberish(text, replace_characters, chance = 50)`.
pub fn gibberish(text: &str, replace: bool, rng: &mut CombatRng) -> String {
    const NOISE: [char; 17] = [
        '#', '@', '*', '&', '%', '$', '/', '<', '>', ';', '*', '*', '*', '*', '*', '*', '*',
    ];
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if !rng.chance(50.0) {
            out.push(c);
            continue;
        }
        if !replace {
            out.push(c);
        }
        for _ in 0..rng.range(0, 2) {
            out.push(NOISE[rng.range(0, NOISE.len() as u32 - 1) as usize]);
        }
    }
    out
}

/// The words of a broadcast (`/datum/signal/subspace/vocal/broadcast`): garbled while the
/// signal is still compressed.
pub fn broadcast_text(text: &str, compression: u8, rng: &mut CombatRng) -> String {
    if compression == 0 {
        return text.to_owned();
    }
    gibberish(text, compression >= COMPRESSION_REPLACE_THRESHOLD, rng)
}

/// `/obj/machinery/telecomms/emp_act`: at `100 / severity` percent, unless already `EMPED`, out
/// for `300 s / severity` ±2 s. Returns whether it took.
pub fn emp_machine(world: &mut World, machine: Entity, severity: u8) -> bool {
    const EMP_SECS: f64 = 300.0;
    let now = world.resource::<Time>().elapsed_secs_f64();
    if world.get::<Emped>(machine).is_some_and(|e| e.until > now) {
        return false;
    }
    let sev = f64::from(severity.max(1));
    let mut rng = world.resource_mut::<crate::emp::EmpRng>();
    if !rng.prob(100.0 / sev) {
        return false;
    }
    let base = (EMP_SECS / sev).round() as i64;
    let secs = rng.range(base - 2, base + 2);
    crate::emp::set_emped(world, machine, secs as f64);
    true
}

/// Every telecomms machine of a kind (`GLOB.telecomm_machines`), in entity order.
pub fn machines_of(world: &mut World, kind: Option<MachineKind>) -> Vec<Entity> {
    let mut q = world.query::<(Entity, &Telecomms)>();
    let mut out: Vec<Entity> = q
        .iter(world)
        .filter(|(_, m)| kind.is_none_or(|k| m.kind == k))
        .map(|(e, _)| e)
        .collect();
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use ss13_chat::radio::FREQ_COMMON;

    use super::*;

    const ROOT: &str = "/obj/machinery/telecomms";

    /// MetaStation's telecomms room on level 2 (all within reach), station levels 2..=2,
    /// autolinked like `post_machine_initialize`; `except` paths are left out.
    fn station(except: &[&str]) -> (Vec<(Entity, Telecomms, TilePos)>, TelecommsLevels) {
        let paths = [
            "receiver/preset_left",
            "receiver/preset_right",
            "bus/preset_one",
            "bus/preset_two",
            "bus/preset_three",
            "bus/preset_four",
            "processor/preset_one",
            "processor/preset_two",
            "processor/preset_three",
            "processor/preset_four",
            "server/presets/science",
            "server/presets/medical",
            "server/presets/supply",
            "server/presets/service",
            "server/presets/common",
            "server/presets/command",
            "server/presets/engineering",
            "server/presets/security",
            "hub/preset",
            "broadcaster/preset_left",
            "broadcaster/preset_right",
            "message_server/preset",
            "relay/preset/mining",
        ];
        let mut world = World::new();
        let mut machines: Vec<(Entity, Telecomms, TilePos)> = paths
            .iter()
            .filter(|p| !except.contains(p))
            .enumerate()
            .map(|(i, p)| {
                let m = preset(&format!("{ROOT}/{p}")).unwrap();
                // The mining relay sits on level 5.
                let z = if p.starts_with("relay") { 5 } else { 2 };
                (world.spawn_empty().id(), m, TilePos::new(i as i32, 1, z))
            })
            .collect();
        let n = machines.len();
        for a in 0..n {
            for b in 0..n {
                if a == b {
                    continue;
                }
                let shared = machines[a]
                    .1
                    .autolinkers
                    .iter()
                    .any(|l| machines[b].1.autolinkers.contains(l));
                let long = machines[a].1.long_range_link && machines[b].1.long_range_link;
                let near = machines[a].2.z == machines[b].2.z;
                if shared && (long || near) {
                    let to = machines[b].0;
                    machines[a].1.links.push(to);
                }
            }
        }
        (machines, TelecommsLevels(2..=2))
    }

    fn route(
        machines: &[(Entity, Telecomms, TilePos)],
        levels: &TelecommsLevels,
        s: &Signal,
    ) -> Option<Broadcast> {
        Net::new(machines.iter().map(|(e, m, p)| (*e, m, p)), levels, 0.0).route(s)
    }

    fn vocal(freq: u16, z: u16) -> Signal {
        Signal::vocal(freq, Levels::of([z]), 50)
    }

    #[test]
    fn a_working_network_decompresses_and_reaches_the_station_and_relays() {
        let (m, levels) = station(&[]);
        for freq in [
            FREQ_COMMON,
            FREQ_SECURITY,
            FREQ_SCIENCE,
            FREQ_ENGINEERING,
            FREQ_SERVICE,
        ] {
            let out = route(&m, &levels, &vocal(freq, 2)).unwrap_or_else(|| panic!("{freq}"));
            assert_eq!(out.compression, 0, "{freq}");
            assert!(
                out.levels.contains(2) && out.levels.contains(5),
                "{freq}: {:?}",
                out.levels
            );
        }
        // Heard on the mining level through the relay.
        assert!(route(&m, &levels, &vocal(FREQ_COMMON, 5)).is_some());
        // Not the Syndicate channel, and not from a level with no relay.
        assert!(route(&m, &levels, &vocal(FREQ_SYNDICATE, 2)).is_none());
        assert!(route(&m, &levels, &vocal(FREQ_COMMON, 7)).is_none());
    }

    #[test]
    fn a_missing_or_dead_processor_garbles_its_channels() {
        let (mut m, levels) = station(&[]);
        // Processor 3 serves bus 3: security and command.
        let p3 = m.iter().position(|x| x.1.id == "Processor 3").unwrap();
        m[p3].1.on = false;
        let sec = route(&m, &levels, &vocal(FREQ_SECURITY, 2)).unwrap();
        assert_eq!(sec.compression, 50);
        let common = route(&m, &levels, &vocal(FREQ_COMMON, 2)).unwrap();
        assert_eq!(common.compression, 0);
        // Hub down: nothing passes.
        let hub = m.iter().position(|x| x.1.id == "Hub").unwrap();
        m[hub].1.on = false;
        assert!(route(&m, &levels, &vocal(FREQ_COMMON, 2)).is_none());
    }

    #[test]
    fn messages_need_the_message_server() {
        let (mut m, levels) = station(&[]);
        let msg = Signal::message(Levels::of([2]));
        assert!(route(&m, &levels, &msg).is_some());
        let server = m
            .iter()
            .position(|x| x.1.kind == MachineKind::MessageServer)
            .unwrap();
        m[server].1.on = false;
        assert!(route(&m, &levels, &msg).is_none());
        // Speech does not care.
        assert!(route(&m, &levels, &vocal(FREQ_COMMON, 2)).is_some());
        // A dead broadcaster pair silences everything.
        m[server].1.on = true;
        for x in m
            .iter_mut()
            .filter(|x| x.1.kind == MachineKind::Broadcaster)
        {
            x.1.on = false;
        }
        assert!(route(&m, &levels, &msg).is_none());
    }

    #[test]
    fn servers_log_and_a_bus_rewrites_the_frequency() {
        let (mut m, levels) = station(&[]);
        let net = |m: &[(Entity, Telecomms, TilePos)]| {
            Net::new(m.iter().map(|(e, t, p)| (*e, t, p)), &levels, 0.0)
        };
        let science = m.iter().find(|x| x.1.id == "Science Server").unwrap().0;
        let routed = net(&m).route_logged(&vocal(FREQ_SCIENCE, 2));
        assert_eq!(routed.logged, vec![(science, 0)]);
        assert_eq!(routed.broadcast.unwrap().freq, FREQ_SCIENCE);
        // Bus 1 (science, medical) rewrites science to medical: the medical server logs it.
        let bus = m.iter().position(|x| x.1.id == "Bus 1").unwrap();
        m[bus].1.change_frequency = FREQ_MEDICAL;
        let medical = m.iter().find(|x| x.1.id == "Medical Server").unwrap().0;
        let routed = net(&m).route_logged(&vocal(FREQ_SCIENCE, 2));
        assert_eq!(routed.logged, vec![(medical, 0)]);
        assert_eq!(routed.broadcast.unwrap().freq, FREQ_MEDICAL);
        // A new network id on a preset does not matter to routing; the presets carry tcommsat.
        assert_eq!(m[bus].1.network, "tcommsat");
    }

    #[test]
    fn the_syndicate_mainframe_broadcasts_everywhere() {
        let mut nuke = preset(&format!("{ROOT}/allinone/nuclear")).unwrap();
        nuke.links.clear();
        let e = World::new().spawn_empty().id();
        let machines = [(e, nuke, TilePos::new(1, 1, 9))];
        let levels = TelecommsLevels(2..=2);
        let out = route(&machines, &levels, &vocal(FREQ_SYNDICATE, 2)).unwrap();
        assert!(out.levels.all && out.compression == 0);
        assert!(route(&machines, &levels, &vocal(FREQ_COMMON, 9)).is_none());
    }

    #[test]
    fn presets() {
        let right = preset(&format!("{ROOT}/receiver/preset_right")).unwrap();
        assert!(right.listens(FREQ_COMMON) && right.listens(FREQ_JUSTICE));
        assert!(!right.listens(FREQ_SCIENCE));
        let ruskie = preset(&format!("{ROOT}/relay/preset/ruskie")).unwrap();
        assert!(!ruskie.toggled && ruskie.long_range_link);
        assert_eq!(
            MachineKind::of("/obj/machinery/telecomms/processor/preset_one/birdstation"),
            Some(MachineKind::Processor)
        );
        let plain = preset("/obj/machinery/telecomms/message_server").unwrap();
        assert_eq!(plain.calibrated_at, CALIBRATION_SECS);
    }

    #[test]
    fn gibberish_keeps_or_replaces() {
        let mut rng = CombatRng(7);
        let kept = gibberish("абвгдежзик", false, &mut rng);
        let letters: String = kept.chars().filter(|c| c.is_alphabetic()).collect();
        assert_eq!(letters, "абвгдежзик");
        let replaced = gibberish("абвгдежзиклмнопрст", true, &mut rng);
        assert!(replaced.chars().filter(|c| c.is_alphabetic()).count() < 18);
        assert_eq!(broadcast_text("тест", 0, &mut rng), "тест");
    }
}
