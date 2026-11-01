//! Client side: connects at `Startup` over WebTransport, performs the handshake, sends intents,
//! and reconnects with backoff when the session is lost (not after a rejection).
//!
//! Game code reads [`NetClientState`], writes [`SendIntent`] and reads [`NoticeReceived`].
//! Replicated world entities appear as local entities with [`NetEntity`] (see
//! `replication_client`).

use core::net::{Ipv4Addr, SocketAddr};
use core::time::Duration;
use std::time::Instant;

use bevy::prelude::*;
use ss13_shared::protocol::{ClientIntent, PROTOCOL_VERSION};
use tokio::sync::mpsc;
use wtransport::tls::Sha256Digest;
use wtransport::{ClientConfig, Connection, Endpoint, RecvStream, SendStream};

pub use crate::replication_client::{LightBlocks, LocalControl, NetEntity, NetEntityMap};

use crate::chat::client::ChatReceived;
use crate::move_ack::client::MoveAckReceived;
use crate::protocol::{
    ClientMessage, DEFAULT_PORT, Frame, FrameError, Hello, HelloReply, IDLE_TIMEOUT, KEEP_ALIVE,
    MAX_CLIENT_FRAME, MAX_HELLO_FRAME, MAX_SERVER_FRAME, ServerMessage, ServerNotice, close,
    close_reason, code, decode_motion, encode_frame, lost_reason, read_frame, runtime,
};
use crate::replication_client::ReplicationInbox;
use crate::sim_latency::{Inbox, SimLatency};
use crate::tap::{NetLinkStats, NetPacket, NetTap};
use crate::ui::client::UiMessageReceived;

/// QUIC handshake, WebTransport session and our handshake together.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
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

enum Event {
    Accepted(Connection),
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
    conn: Option<Connection>,
    // Unbounded: only this player's own input goes in.
    out: mpsc::UnboundedSender<Frame>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Session {
    fn drop(&mut self) {
        // Dropping the task drops the connection, which closes it.
        self.task.abort();
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
                let _ = session.out.send(frame);
            }
            Err(e) => warn!("not sent to the server: {e}"),
        }
    }
}

impl Plugin for NetClientPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            crate::ui::client::plugin,
            crate::chat::client::plugin,
            crate::debug::client::plugin,
            crate::admin::client::plugin,
            crate::move_ack::client::plugin,
            crate::camera::client::plugin,
            crate::sound::client::plugin,
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
    mut transport: ResMut<ClientTransport>,
    mut state: ResMut<NetClientState>,
) {
    let digest = &config.certificate_digest;
    if parse_digest(digest).is_none() {
        *state = NetClientState::Disconnected {
            reason: format!("invalid certificate digest {digest:?}: expected 64 hex digits"),
        };
        return;
    }
    connect(&config, version.0, &mut transport, &mut state);
}

/// 64 hex digits, as the server logs it.
pub(crate) fn parse_digest(hex: &str) -> Option<Sha256Digest> {
    if hex.len() != 64 || !hex.is_ascii() {
        return None;
    }
    let mut bytes = [0; 32];
    for (byte, pair) in bytes.iter_mut().zip(hex.as_bytes().chunks(2)) {
        *byte = u8::from_str_radix(core::str::from_utf8(pair).ok()?, 16).ok()?;
    }
    Some(Sha256Digest::new(bytes))
}

fn connect(
    config: &NetClientConfig,
    version: u32,
    transport: &mut ClientTransport,
    state: &mut NetClientState,
) {
    let (events_tx, events_rx) = mpsc::channel(INBOX_ITEMS);
    let (out_tx, out_rx) = mpsc::unbounded_channel();
    let task = runtime().spawn(run_session(config.clone(), version, events_tx, out_rx));
    transport.session = Some(Session {
        events: Inbox::new(events_rx),
        conn: None,
        out: out_tx,
        task,
    });
    transport.retry_at = None;
    *state = NetClientState::Connecting;
}

type Events = mpsc::Sender<(Instant, Event)>;

async fn run_session(
    config: NetClientConfig,
    version: u32,
    events: Events,
    out: mpsc::UnboundedReceiver<Frame>,
) {
    let event = match session(&config, version, &events, out).await {
        Ok(Some(rejected)) => Event::Rejected(rejected),
        Ok(None) => return,
        Err(reason) => Event::Lost(reason),
    };
    let _ = events.send((Instant::now(), event)).await;
}

/// `Ok(Some(reason))`: rejected; `Ok(None)`: Bevy dropped the session; `Err`: lost.
async fn session(
    config: &NetClientConfig,
    version: u32,
    events: &Events,
    out: mpsc::UnboundedReceiver<Frame>,
) -> Result<Option<String>, String> {
    let digest = parse_digest(&config.certificate_digest).ok_or("invalid certificate digest")?;
    let client_config = ClientConfig::builder()
        .with_bind_default()
        .with_server_certificate_hashes([digest])
        .keep_alive_interval(Some(KEEP_ALIVE))
        .max_idle_timeout(Some(IDLE_TIMEOUT))
        .map_err(|e| e.to_string())?
        .build();
    let endpoint = Endpoint::client(client_config).map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    let handshake = async {
        let conn = endpoint
            .connect(format!("https://{}", config.server_addr))
            .await
            .map_err(|e| e.to_string())?;
        let opening = conn.open_bi().await.map_err(|e| close_reason(&e))?;
        let (mut send, mut recv) = opening.await.map_err(|e| e.to_string())?;
        let hello = Hello {
            version,
            token: config.token.clone(),
        };
        let frame = encode_frame(&hello, MAX_HELLO_FRAME).map_err(|e| e.to_string())?;
        send.write_all(&frame).await.map_err(|e| e.to_string())?;
        let reply = read_frame::<HelloReply>(&mut recv, MAX_HELLO_FRAME, &mut buf)
            .await
            .map_err(|e| e.to_string());
        let reply = match reply {
            Ok(reply) => reply,
            // The server closed before answering (shutting down, timed out): its reason if any.
            Err(how) => return Err(lost_reason(&conn, how).await),
        };
        Ok::<_, String>((conn, send, recv, reply))
    };
    let (conn, send, recv, reply) = tokio::time::timeout(CONNECT_TIMEOUT, handshake)
        .await
        .map_err(|_| {
            format!(
                "no answer from {} in {CONNECT_TIMEOUT:?}",
                config.server_addr
            )
        })??;
    if let HelloReply::Rejected(reason) = reply {
        conn.close(code(close::NORMAL), b"");
        return Ok(Some(reason));
    }
    if events
        .send((Instant::now(), Event::Accepted(conn.clone())))
        .await
        .is_err()
    {
        return Ok(None);
    }
    let reason = tokio::select! {
        reason = read_loop(&conn, recv, events) => reason,
        reason = read_datagrams(&conn, events) => reason,
        reason = write_loop(send, out) => reason,
        error = conn.closed() => Some(close_reason(&error)),
    };
    let Some(reason) = reason else {
        conn.close(code(close::NORMAL), b"");
        return Ok(None);
    };
    // A kick is final, like a rejection: reconnecting would undo it.
    let kicked = matches!(
        tokio::time::timeout(Duration::from_millis(100), conn.closed()).await,
        Ok(wtransport::error::ConnectionError::ApplicationClosed(c)) if c.code() == code(close::KICKED)
    );
    conn.close(code(close::NORMAL), b"");
    if kicked {
        Ok(Some(reason))
    } else {
        Err(reason)
    }
}

/// `None` when Bevy dropped the session.
async fn read_loop(conn: &Connection, mut recv: RecvStream, events: &Events) -> Option<String> {
    let mut buf = Vec::new();
    loop {
        match read_frame(&mut recv, MAX_SERVER_FRAME, &mut buf).await {
            Ok(message) => {
                let size = buf.len() + 4;
                events
                    .send((Instant::now(), Event::Message(message, size)))
                    .await
                    .ok()?;
            }
            Err(FrameError::Closed(how)) => return Some(lost_reason(conn, how).await),
            Err(e) => {
                let reason = format!("server sent a {e}");
                conn.close(code(close::PROTOCOL), reason.as_bytes());
                return Some(reason);
            }
        }
    }
}

async fn read_datagrams(conn: &Connection, events: &Events) -> Option<String> {
    loop {
        let datagram = match conn.receive_datagram().await {
            Ok(datagram) => datagram,
            Err(e) => return Some(close_reason(&e)),
        };
        let payload = datagram.payload();
        let Some(batch) = decode_motion(&payload) else {
            continue; // Unreliable anyway; a bad one is just lost.
        };
        match events.try_send((Instant::now(), Event::Motion(batch, payload.len()))) {
            Ok(()) | Err(mpsc::error::TrySendError::Full(_)) => {}
            Err(mpsc::error::TrySendError::Closed(_)) => return None,
        }
    }
}

async fn write_loop(
    mut send: SendStream,
    mut out: mpsc::UnboundedReceiver<Frame>,
) -> Option<String> {
    while let Some(frame) = out.recv().await {
        if let Err(e) = send.write_all(&frame).await {
            return Some(e.to_string());
        }
    }
    None
}

fn receive(
    mut transport: ResMut<ClientTransport>,
    mut state: ResMut<NetClientState>,
    config: Res<NetClientConfig>,
    version: Res<HandshakeVersion>,
    latency: Res<SimLatency>,
    mut inbox: ResMut<ReplicationInbox>,
    mut notices: MessageWriter<NoticeReceived>,
    mut ui: MessageWriter<UiMessageReceived>,
    mut chat: MessageWriter<ChatReceived>,
    (mut acks, mut camera, mut sounds): (
        MessageWriter<MoveAckReceived>,
        MessageWriter<crate::camera::client::CameraEffectReceived>,
        MessageWriter<crate::sound::client::SoundReceived>,
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
        if let Some(conn) = transport.session.as_ref().and_then(|s| s.conn.as_ref()) {
            let quic = conn.quic_connection();
            let stats = quic.stats();
            link_stats.set_if_neq(NetLinkStats {
                rtt: Some(quic.rtt()),
                udp_tx_bytes: stats.udp_tx.bytes,
                udp_rx_bytes: stats.udp_rx.bytes,
                udp_tx_datagrams: stats.udp_tx.datagrams,
                udp_rx_datagrams: stats.udp_rx.datagrams,
                lost_packets: stats.path.lost_packets,
            });
        }
    } else {
        // Sent while the tap was on; stale by the time it is turned on again.
        transport.tapped.get_mut().expect("tap lock").clear();
    }
    if transport.session.is_none() {
        if transport.retry_at.is_some_and(|at| Instant::now() >= at) {
            info!("reconnecting to {}", config.server_addr);
            connect(&config, version.0, transport, &mut state);
        }
        return;
    }
    let mut ended = None;
    let session = transport.session.as_mut().expect("checked");
    session.events.drain(latency.0, |event| match event {
        Event::Accepted(conn) => {
            info!("handshake accepted");
            session.conn = Some(conn);
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
                ServerMessage::Camera(effect) => {
                    camera.write(crate::camera::client::CameraEffectReceived(effect));
                }
                ServerMessage::Sound(sound) => {
                    sounds.write(crate::sound::client::SoundReceived(sound));
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
