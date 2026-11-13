//! Client side: connects at `Startup` over WebTransport (or the in-memory [`crate::loopback`]
//! transport when a [`LoopbackConnector`] is inserted), performs the handshake, sends intents,
//! and reconnects with backoff when the session is lost (not after a rejection).
//!
//! Game code reads [`NetClientState`], writes [`SendIntent`] and reads [`NoticeReceived`].
//! Replicated world entities appear as local entities with [`NetEntity`] (see
//! `replication_client`).

use core::net::{Ipv4Addr, SocketAddr};
use core::time::Duration;

use bevy::platform::time::Instant;
use bevy::prelude::*;
use ss13_shared::protocol::{ClientIntent, PROTOCOL_VERSION};
use tokio::sync::mpsc;

pub use crate::replication_client::{LightBlocks, LocalControl, NetEntity, NetEntityMap};

use crate::chat::client::ChatReceived;
use crate::loopback::{LoopbackConnector, LoopbackEnd};
use crate::move_ack::client::MoveAckReceived;
use crate::protocol::{
    ClientMessage, DEFAULT_PORT, Hello, HelloReply, MAX_CLIENT_FRAME, MAX_HELLO_FRAME,
    MAX_SERVER_FRAME, ServerMessage, ServerNotice, close, decode_frame, decode_motion,
    encode_frame,
};
use crate::replication_client::ReplicationInbox;
use crate::sim_latency::{Inbox, SimLatency};
use crate::tap::{NetLinkStats, NetPacket, NetTap};
use crate::ui::client::UiMessageReceived;

/// The established QUIC connection, kept for its stats; nothing on wasm32.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) type QuicConn = wtransport::Connection;
#[cfg(target_arch = "wasm32")]
pub(crate) type QuicConn = core::convert::Infallible;

/// QUIC handshake, WebTransport session and our handshake together.
pub(crate) const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Reconnect delays: doubled after every failed attempt up to the max, reset by an accepted
/// handshake.
const RECONNECT_MIN: Duration = Duration::from_secs(1);
const RECONNECT_MAX: Duration = Duration::from_secs(30);
/// Received frames and datagrams waiting for Bevy. A full queue stalls the stream (QUIC flow
/// control slows the server) and drops datagrams; a client draws faster than the server ticks.
const INBOX_ITEMS: usize = 256;

pub struct NetClientPlugin;

/// Insert before [`NetClientPlugin`] to override; otherwise read from the environment:
/// `SS13_SERVER_ADDR` (default `127.0.0.1:7777`), `SS13_CERT_DIGEST` (hex SHA-256 logged by the
/// server), `SS13_TOKEN` (default `dev`).
// shortcut: the server is authenticated only by a pinned digest; a CA-validated certificate needs
// a host name in the config (`with_native_certs`), add it with the server list.
#[derive(Resource, Clone, Debug)]
pub struct NetClientConfig {
    pub server_addr: SocketAddr,
    pub certificate_digest: String,
    pub token: String,
}

impl Default for NetClientConfig {
    fn default() -> Self {
        let env = |key: &str| std::env::var(key).ok();
        Self {
            server_addr: env("SS13_SERVER_ADDR")
                .and_then(|addr| addr.parse().ok())
                .unwrap_or(SocketAddr::new(Ipv4Addr::LOCALHOST.into(), DEFAULT_PORT)),
            certificate_digest: env("SS13_CERT_DIGEST").unwrap_or_default(),
            token: env("SS13_TOKEN").unwrap_or_else(|| "dev".into()),
        }
    }
}

#[derive(Resource, Clone, Debug, PartialEq, Eq)]
pub enum NetClientState {
    /// Transport or handshake in progress.
    Connecting,
    /// Handshake accepted; intents are delivered.
    Connected,
    /// Not connected; `reason` is fit to show to the player. A lost session is retried after a
    /// backoff (the state goes back to `Connecting`); a rejection is final.
    Disconnected { reason: String },
}

/// Write to send an intent; dropped unless [`NetClientState::Connected`].
#[derive(Message, Clone, Debug)]
pub struct SendIntent(pub ClientIntent);

#[derive(Message, Clone, Debug)]
pub struct NoticeReceived(pub ServerNotice);

/// Ordering anchors for game systems.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NetClientSystems {
    /// `PreUpdate`: updates [`NetClientState`], writes [`NoticeReceived`].
    Receive,
    /// `PostUpdate`: consumes [`SendIntent`].
    Send,
}

/// Version sent in the handshake; a resource only so tests can fake an old client.
#[derive(Resource)]
pub(crate) struct HandshakeVersion(pub u32);

pub(crate) enum Event {
    /// With the QUIC connection; `None` over loopback.
    Accepted(Option<QuicConn>),
    /// Final: the server refused this client.
    Rejected(String),
    /// With its encoded size.
    Message(ServerMessage, usize),
    Motion(crate::replication::MotionBatch, usize),
    Lost(String),
}

/// The current attempt or session, and when to try again.
#[derive(Resource, Default)]
pub(crate) struct ClientTransport {
    session: Option<Session>,
    retry_at: Option<Instant>,
    backoff: Duration,
    /// Mirrors [`NetTap::enabled`]; while set, [`Self::send`] records what it sends.
    tap: bool,
    tapped: std::sync::Mutex<Vec<NetPacket>>,
}

struct Session {
    events: Inbox<Event>,
    /// Set once the handshake is accepted.
    conn: Option<QuicConn>,
    driver: Driver,
}

enum Driver {
    #[cfg(not(target_arch = "wasm32"))]
    Quic {
        // Unbounded: only this player's own input goes in.
        out: mpsc::UnboundedSender<crate::protocol::Frame>,
        task: tokio::task::JoinHandle<()>,
    },
    Loopback(Box<LoopbackSession>),
}

impl Drop for Session {
    fn drop(&mut self) {
        // Dropping the task drops the connection, which closes it; a dropped loopback end closes
        // the connection for the server.
        #[cfg(not(target_arch = "wasm32"))]
        if let Driver::Quic { task, .. } = &self.driver {
            task.abort();
        }
    }
}

impl ClientTransport {
    pub(crate) fn send(&self, message: &ClientMessage) {
        let Some(session) = &self.session else {
            return;
        };
        match encode_frame(message, MAX_CLIENT_FRAME) {
            Ok(frame) => {
                if self.tap {
                    let packet = NetPacket::sent(message, frame.len());
                    self.tapped.lock().expect("tap lock").push(packet);
                }
                match &session.driver {
                    #[cfg(not(target_arch = "wasm32"))]
                    Driver::Quic { out, .. } => {
                        let _ = out.send(frame);
                    }
                    Driver::Loopback(lo) => lo.end.tx.send_frame(frame),
                }
            }
            Err(e) => warn!("not sent to the server: {e}"),
        }
    }
}

impl Plugin for NetClientPlugin {
    fn build(&self, app: &mut App) {
        // Replicated blockers are filed in a local index for movement prediction.
        // shortcut: levels of tg's 255 x 255; send the level bounds if bigger maps come.
        if !app.world().contains_resource::<ss13_world::SpatialIndex>() {
            app.insert_resource(ss13_world::SpatialIndex::auto_levels(255, 255))
                .add_plugins(ss13_world::SpatialIndexPlugin);
        }
        app.add_plugins((
            crate::ui::client::plugin,
            crate::chat::client::plugin,
            crate::debug::client::plugin,
            crate::admin::client::plugin,
            crate::move_ack::client::plugin,
            crate::camera::client::plugin,
            crate::sound::client::plugin,
            crate::animation::client::plugin,
            crate::sim_latency::plugin,
        ))
        .init_resource::<NetClientConfig>()
        .insert_resource(HandshakeVersion(PROTOCOL_VERSION))
        .insert_resource(NetClientState::Disconnected {
            reason: "not connected".into(),
        })
        .init_resource::<ClientTransport>()
        .init_resource::<ReplicationInbox>()
        .add_message::<SendIntent>()
        .add_message::<NoticeReceived>()
        .init_resource::<NetTap>()
        .init_resource::<NetLinkStats>()
        .add_message::<NetPacket>()
        .add_systems(Startup, start_client)
        .init_resource::<NetEntityMap>()
        .init_resource::<LightBlocks>()
        .add_message::<ss13_shared::LightUpdate>()
        .add_systems(
            PreUpdate,
            (
                receive,
                crate::replication_client::receive_replication,
                // After everything that may apply a batch this frame, or its entities outlive the
                // connection as ghosts.
                crate::replication_client::clear_on_disconnect,
            )
                .chain()
                .in_set(NetClientSystems::Receive),
        )
        .add_systems(PostUpdate, send_intents.in_set(NetClientSystems::Send));
    }
}

fn start_client(
    config: Res<NetClientConfig>,
    version: Res<HandshakeVersion>,
    connector: Option<Res<LoopbackConnector>>,
    mut transport: ResMut<ClientTransport>,
    mut state: ResMut<NetClientState>,
) {
    #[cfg(not(target_arch = "wasm32"))]
    if connector.is_none() {
        let digest = &config.certificate_digest;
        if crate::client_quic::parse_digest(digest).is_none() {
            *state = NetClientState::Disconnected {
                reason: format!("invalid certificate digest {digest:?}: expected 64 hex digits"),
            };
            return;
        }
    }
    connect(
        &config,
        version.0,
        connector.as_deref(),
        &mut transport,
        &mut state,
    );
}

fn connect(
    config: &NetClientConfig,
    version: u32,
    connector: Option<&LoopbackConnector>,
    transport: &mut ClientTransport,
    state: &mut NetClientState,
) {
    let (events_tx, events_rx) = mpsc::channel(INBOX_ITEMS);
    let driver = match connector {
        Some(connector) => Driver::Loopback(Box::new(LoopbackSession::start(
            connector, config, version, events_tx,
        ))),
        #[cfg(not(target_arch = "wasm32"))]
        None => {
            let (out, task) = crate::client_quic::spawn(config, version, events_tx);
            Driver::Quic { out, task }
        }
        #[cfg(target_arch = "wasm32")]
        None => {
            *state = NetClientState::Disconnected {
                reason: "no WebTransport client on wasm32: insert a LoopbackConnector".into(),
            };
            return;
        }
    };
    transport.session = Some(Session {
        events: Inbox::new(events_rx),
        conn: None,
        driver,
    });
    transport.retry_at = None;
    *state = NetClientState::Connecting;
}

pub(crate) type Events = mpsc::Sender<(Instant, Event)>;

/// An in-memory session, pumped from `receive` (no task): the counterpart of `client_quic`'s
/// session task, with the same frames, limits and outcomes.
struct LoopbackSession {
    end: LoopbackEnd,
    events: Events,
    accepted: bool,
    deadline: Instant,
    /// The final event, held while the event queue is full; `over` once it is queued.
    ending: Option<Event>,
    over: bool,
}

impl LoopbackSession {
    fn start(
        connector: &LoopbackConnector,
        config: &NetClientConfig,
        version: u32,
        events: Events,
    ) -> Self {
        // A server that is gone gives a closed end: the session is lost and retried.
        let end = connector
            .connect()
            .unwrap_or_else(|| crate::loopback::pipe().0);
        let hello = Hello {
            version,
            token: config.token.clone(),
        };
        let mut session = Self {
            end,
            events,
            accepted: false,
            deadline: Instant::now() + CONNECT_TIMEOUT,
            ending: None,
            over: false,
        };
        match encode_frame(&hello, MAX_HELLO_FRAME) {
            Ok(frame) => session.end.tx.send_frame(frame),
            Err(e) => session.ending = Some(Event::Lost(e.to_string())),
        }
        session
    }

    /// Moves what arrived into the event queue.
    fn pump(&mut self) {
        if self.over {
            return;
        }
        if self.ending.is_none() {
            self.ending = self.read();
        }
        if let Some(event) = self.ending.take() {
            match self.events.try_send((Instant::now(), event)) {
                Err(mpsc::error::TrySendError::Full((_, event))) => self.ending = Some(event),
                _ => self.over = true,
            }
        }
    }

    /// The final event once the session is over.
    fn read(&mut self) -> Option<Event> {
        let now = Instant::now();
        if !self.accepted {
            let frame = match self.end.recv_frame() {
                Ok(Some(frame)) => frame,
                Ok(None) if now < self.deadline => return None,
                Ok(None) => {
                    let reason =
                        format!("no answer from the loopback server in {CONNECT_TIMEOUT:?}");
                    return Some(Event::Lost(reason));
                }
                Err((_, reason)) => return Some(Event::Lost(reason)),
            };
            match decode_frame(&frame, MAX_HELLO_FRAME) {
                Ok(HelloReply::Accepted) => {
                    if self.events.try_send((now, Event::Accepted(None))).is_err() {
                        return Some(Event::Lost("client dropped the session".into()));
                    }
                    self.accepted = true;
                }
                Ok(HelloReply::Rejected(reason)) => {
                    self.end.close(close::NORMAL, "");
                    return Some(Event::Rejected(reason));
                }
                Err(e) => return Some(Event::Lost(e.to_string())),
            }
        }
        // A full queue leaves frames in the connection, as QUIC flow control would.
        while let Ok(permit) = self.events.try_reserve() {
            let frame = match self.end.recv_frame() {
                Ok(Some(frame)) => frame,
                Ok(None) => break,
                // A kick is final, like a rejection: reconnecting would undo it.
                Err((code, reason)) if code == close::KICKED => {
                    return Some(Event::Rejected(reason));
                }
                Err((_, reason)) => return Some(Event::Lost(reason)),
            };
            match decode_frame(&frame, MAX_SERVER_FRAME) {
                Ok(message) => permit.send((now, Event::Message(message, frame.len()))),
                Err(e) => {
                    let reason = format!("server sent a {e}");
                    self.end.close(close::PROTOCOL, &reason);
                    return Some(Event::Lost(reason));
                }
            }
        }
        while let Some(datagram) = self.end.recv_datagram() {
            // Unreliable anyway: a bad one or one over a full queue is just lost.
            if let Some(batch) = decode_motion(&datagram) {
                let _ = self
                    .events
                    .try_send((now, Event::Motion(batch, datagram.len())));
            }
        }
        None
    }
}

fn receive(
    mut transport: ResMut<ClientTransport>,
    mut state: ResMut<NetClientState>,
    config: Res<NetClientConfig>,
    version: Res<HandshakeVersion>,
    connector: Option<Res<LoopbackConnector>>,
    latency: Res<SimLatency>,
    mut inbox: ResMut<ReplicationInbox>,
    mut notices: MessageWriter<NoticeReceived>,
    mut ui: MessageWriter<UiMessageReceived>,
    (mut chat, mut close_say): (
        MessageWriter<ChatReceived>,
        MessageWriter<crate::chat::client::CloseSayReceived>,
    ),
    (mut acks, mut camera, mut sounds, mut animations, mut channel_sounds): (
        MessageWriter<MoveAckReceived>,
        MessageWriter<crate::camera::client::CameraEffectReceived>,
        MessageWriter<crate::sound::client::SoundReceived>,
        MessageWriter<crate::animation::client::AnimationReceived>,
        MessageWriter<crate::sound::client::ChannelSoundReceived>,
    ),
    mut debug: MessageWriter<crate::debug::client::DebugResponseReceived>,
    mut admin: MessageWriter<crate::admin::client::AdminResponseReceived>,
    tap: Res<NetTap>,
    mut packets: MessageWriter<NetPacket>,
    mut link_stats: ResMut<NetLinkStats>,
) {
    let transport = &mut *transport;
    transport.tap = tap.enabled;
    if tap.enabled {
        packets.write_batch(transport.tapped.get_mut().expect("tap lock").drain(..));
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(conn) = transport.session.as_ref().and_then(|s| s.conn.as_ref()) {
            link_stats.set_if_neq(crate::client_quic::link_stats(conn));
        }
        #[cfg(target_arch = "wasm32")]
        let _ = &mut link_stats;
    } else {
        // Sent while the tap was on; stale by the time it is turned on again.
        transport.tapped.get_mut().expect("tap lock").clear();
    }
    if transport.session.is_none() {
        if transport.retry_at.is_some_and(|at| Instant::now() >= at) {
            info!("reconnecting to {}", config.server_addr);
            connect(
                &config,
                version.0,
                connector.as_deref(),
                transport,
                &mut state,
            );
        }
        return;
    }
    let mut ended = None;
    let session = transport.session.as_mut().expect("checked");
    // The only driver on wasm32.
    #[allow(irrefutable_let_patterns)]
    if let Driver::Loopback(lo) = &mut session.driver {
        lo.pump();
    }
    session.events.drain(latency.0, |event| match event {
        Event::Accepted(conn) => {
            info!("handshake accepted");
            session.conn = conn;
            *state = NetClientState::Connected;
        }
        Event::Message(message, size) => {
            if tap.enabled {
                packets.write(NetPacket::received(&message, size));
            }
            match message {
                ServerMessage::Notice(notice) => {
                    notices.write(NoticeReceived(notice));
                }
                ServerMessage::Ui(message) => {
                    ui.write(UiMessageReceived(message));
                }
                ServerMessage::Chat(message) => {
                    chat.write(ChatReceived(message));
                }
                ServerMessage::CloseSay => {
                    close_say.write(crate::chat::client::CloseSayReceived);
                }
                ServerMessage::Camera(effect) => {
                    camera.write(crate::camera::client::CameraEffectReceived(effect));
                }
                ServerMessage::Sound(sound) => {
                    sounds.write(crate::sound::client::SoundReceived(sound));
                }
                ServerMessage::ChannelSound(sound) => {
                    channel_sounds.write(crate::sound::client::ChannelSoundReceived(sound));
                }
                ServerMessage::Animation(event) => {
                    animations.write(crate::animation::client::AnimationReceived(event));
                }
                ServerMessage::MoveAck(ack) => {
                    acks.write(MoveAckReceived(ack));
                }
                ServerMessage::Replication(batch) => inbox.batches.push(batch),
                ServerMessage::Debug(response) => {
                    debug.write(crate::debug::client::DebugResponseReceived(response));
                }
                ServerMessage::Admin(response) => {
                    admin.write(crate::admin::client::AdminResponseReceived(response));
                }
            }
        }
        Event::Motion(batch, size) => {
            if tap.enabled {
                packets.write(NetPacket::motion(&batch, size));
            }
            inbox.motion.push(batch);
        }
        Event::Rejected(reason) => ended = Some((format!("rejected by server: {reason}"), false)),
        Event::Lost(reason) => ended = Some((reason, true)),
    });
    if let Some((reason, retry)) = ended {
        let was_connected = *state == NetClientState::Connected;
        transport.session = None;
        if was_connected {
            transport.backoff = Duration::ZERO;
        }
        if retry {
            transport.backoff = (transport.backoff * 2).clamp(RECONNECT_MIN, RECONNECT_MAX);
            transport.retry_at = Some(Instant::now() + transport.backoff);
            warn!(
                "disconnected: {reason}; reconnecting in {:?}",
                transport.backoff
            );
        } else {
            warn!("{reason}");
        }
        *state = NetClientState::Disconnected { reason };
    }
}

fn send_intents(
    mut intents: MessageReader<SendIntent>,
    transport: Res<ClientTransport>,
    state: Res<NetClientState>,
) {
    if *state != NetClientState::Connected {
        intents.clear();
        return;
    }
    for SendIntent(intent) in intents.read() {
        transport.send(&ClientMessage::Intent(intent.clone()));
    }
}
