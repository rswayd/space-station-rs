//! Server side: WebTransport endpoint, admission, handshake, and delivery between sessions and
//! Bevy.
//!
//! Game plugins see only the Bevy messages defined here. A client is identified by its server-side
//! connection entity, spawned when its handshake is accepted ([`ClientConnected`]) and valid until
//! [`ClientDisconnected`].
//!
//! Every QUIC session runs as a task on the transport runtime (`server_quic`); Bevy systems
//! exchange data with it through bounded channels only:
//! - client → server: at most [`INBOX_FRAMES`] decoded messages wait per client; a client sending
//!   faster is slowed by QUIC flow control, not buffered;
//! - server → client: the bytes queued for a client are counted; over [`SEND_QUEUE_CAP`], or when
//!   one write is blocked for [`SEND_STALL_TIMEOUT`] (the client stopped reading), the session is
//!   closed. QUIC's own buffer per client is bounded by `send_window`.
//!
//! Admission: [`NetServerConfig::max_clients`] sessions in total (handshakes included) and
//! [`NetServerConfig::max_clients_per_ip`] per address (per /64 for IPv6), checked on the first
//! QUIC packet before any TLS work; handshakes not finished in [`HANDSHAKE_TIMEOUT`] are closed.
//!
//! Certificates: by default a self-signed certificate (valid 14 days, the WebTransport limit for
//! certificate hashes) is generated once per process (a server `App` rebuilt in the same process
//! keeps it); with `SS13_TLS_CERT`/`SS13_TLS_KEY` (or
//! [`NetServerConfig::tls`]) a PEM chain and key are loaded instead. Either way the SHA-256 digest
//! of the leaf is logged and stored in [`ServerCertificateDigest`], and clients pin it.
//!
//! With a [`crate::loopback::LoopbackListener`] inserted the server accepts in-memory connections
//! instead (`server_loopback`, no socket): the same handshake, frames, limits and messages, pumped
//! by Bevy systems. On wasm32 it is the only transport.

use core::net::{Ipv4Addr, SocketAddr};
use core::time::Duration;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use bevy::prelude::*;
use ss13_shared::protocol::{ClientIntent, PROTOCOL_VERSION};
use tokio::sync::{mpsc, oneshot};

pub use crate::light_server::LIGHT_MARGIN;
pub use crate::replication_server::{
    AltAppearance, AltAppearances, ClientTraffic, ControlledEntity, DEFAULT_VIEW_RADIUS, Interest,
    Invisibility, KEEP_MARGIN, Networked, ReplicationServerPlugin, ReplicationStats, SeeGroups,
    SeeInvisible, Seers, Sight, Viewer, VisibleToGroups, VisibleToMobs, los_mask,
};
pub use crate::view_mask::{MASK_CHUNK, MaskChunk, MaskId, ViewMask, ViewMasks, mask_chunk};
pub use ss13_shared::line_of_sight::ViewGrid;

use crate::loopback::LoopbackTx;
use crate::protocol::{
    ClientMessage, DEFAULT_PORT, Frame, Hello, HelloReply, MAX_SERVER_FRAME, ServerMessage,
    ServerNotice, close, encode_frame, pack_motion,
};
use crate::replication::Motion;
use crate::sim_latency::{Inbox, SimLatency};
use crate::ui::server::UiMessageReceived;

/// From the first QUIC packet to a decoded [`Hello`].
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
/// Intents delivered per client per tick (50 ms); the rest are dropped. A player makes a few per
/// tick; 32 still passes ~1.5 s of input bunched up by a lag spike.
pub const MAX_INTENTS_PER_TICK: usize = 32;
/// Decoded client messages waiting for Bevy, per client: two ticks at the intent cap.
pub const INBOX_FRAMES: usize = 64;
/// Bytes queued for one client and not yet taken by QUIC. A whole view is tens of KiB and the
/// steady state a few KiB per tick, so only a client that does not read gets near it.
pub const SEND_QUEUE_CAP: usize = 1024 * 1024;
/// A write to a client blocked this long (its flow-control window is full: it does not read)
/// closes the session.
pub const SEND_STALL_TIMEOUT: Duration = Duration::from_secs(10);

pub struct NetServerPlugin;

/// Insert before [`NetServerPlugin`] to override; otherwise read from the environment
/// (`SS13_BIND_ADDR`, default `127.0.0.1:7777`: without real auth any client takes a slot with any
/// token, so a public bind is an explicit choice, see `docs/RUNNING.md`; `SS13_TLS_CERT` and
/// `SS13_TLS_KEY`: PEM certificate chain and private key instead of a self-signed certificate).
#[derive(Resource, Clone, Debug)]
pub struct NetServerConfig {
    /// After `Startup`, the address actually bound: port 0 picks a free port, which tests read
    /// back instead of probing for one (a probed port can be taken before the bind).
    pub bind_addr: SocketAddr,
    /// Sessions in total, handshakes included.
    pub max_clients: usize,
    /// Concurrent sessions from one IPv4 address or IPv6 /64, handshakes included. Players behind
    /// one NAT (a dormitory, a club) share an address, hence more than one.
    pub max_clients_per_ip: usize,
    pub tls: Option<TlsFiles>,
}

#[derive(Clone, Debug)]
pub struct TlsFiles {
    pub cert_chain: PathBuf,
    pub private_key: PathBuf,
}

impl Default for NetServerConfig {
    fn default() -> Self {
        let env = |key: &str| std::env::var_os(key).map(PathBuf::from);
        let bind_addr = std::env::var("SS13_BIND_ADDR")
            .ok()
            .and_then(|addr| addr.parse().ok())
            .unwrap_or(SocketAddr::new(Ipv4Addr::LOCALHOST.into(), DEFAULT_PORT));
        Self {
            bind_addr,
            max_clients: 1024,
            max_clients_per_ip: 8,
            tls: env("SS13_TLS_CERT")
                .zip(env("SS13_TLS_KEY"))
                .map(|(cert_chain, private_key)| TlsFiles {
                    cert_chain,
                    private_key,
                }),
        }
    }
}

/// Validates the token from the client handshake. Replace the resource to plug in a real auth
/// service; the default accepts any non-empty token.
// shortcut: synchronous check; an auth service over HTTP needs a pending state on the session.
#[derive(Resource, Clone, Copy)]
pub struct TokenValidator {
    /// `Err` is the reason shown to the client.
    pub check: fn(&str) -> Result<(), String>,
    /// Whether an accepted token proves who the client is (an auth service issued it), so that
    /// privileges may hang on it (`ss13_admin`). The default validator does not.
    pub authenticates: bool,
}

impl Default for TokenValidator {
    fn default() -> Self {
        Self {
            check: |token| {
                if token.is_empty() {
                    Err("empty auth token".into())
                } else {
                    Ok(())
                }
            },
            authenticates: false,
        }
    }
}

/// Hex SHA-256 digest of the certificate the server presents; clients pin it.
#[derive(Resource, Clone, Debug)]
pub struct ServerCertificateDigest(pub String);

/// On a connection entity: the token its handshake carried, as the validator accepted it. Until an
/// auth service exists it is only a dev identity (`SS13_DEV_SPAWNS` in `ss13_game`).
#[derive(Component, Clone, Debug)]
pub struct ClientToken(pub String);

/// A client passed the handshake; `client` is its connection entity.
#[derive(Message, Clone, Copy, Debug)]
pub struct ClientConnected {
    pub client: Entity,
}

/// A client that had passed the handshake is gone. The entity is despawned at the end of the next
/// frame, after every reader of this message.
#[derive(Message, Clone, Debug)]
pub struct ClientDisconnected {
    pub client: Entity,
    pub reason: String,
}

/// Untrusted: validate at the moment of effect.
#[derive(Message, Clone, Debug)]
pub struct IntentReceived {
    pub client: Entity,
    pub intent: ClientIntent,
}

/// Write to send a notice to one connected client.
#[derive(Message, Clone, Debug)]
pub struct SendNotice {
    pub client: Entity,
    pub notice: ServerNotice,
}

/// Write to close a connected client's session for good (an admin kick or ban): the client shows
/// `reason` and does not reconnect.
#[derive(Message, Clone, Debug)]
pub struct KickClient {
    pub client: Entity,
    pub reason: String,
}

/// Ordering anchors for game systems.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NetServerSystems {
    /// `PreUpdate`: writes [`ClientConnected`], [`ClientDisconnected`] and [`IntentReceived`].
    Receive,
    /// `PostUpdate`: consumes [`SendNotice`] and the other send messages.
    Send,
}

/// Intents this client sent over [`MAX_INTENTS_PER_TICK`], for logs and abuse detection.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DroppedIntents(pub u64);

/// The live session of a connection entity that passed the handshake. Removed (and with it the
/// session closed) when the client disconnects.
#[derive(Component)]
pub(crate) struct ClientLink {
    pub(crate) conn: LinkConn,
    pub(crate) out: mpsc::UnboundedSender<Frame>,
    /// Bytes in `out` not yet handed to QUIC (or read by a loopback peer); bounds `out`.
    pub(crate) queued: Arc<AtomicUsize>,
    pub(crate) inbox: Inbox<ClientMessage>,
    pub(crate) closed: oneshot::Receiver<String>,
}

pub(crate) enum LinkConn {
    #[cfg(not(target_arch = "wasm32"))]
    Quic(wtransport::Connection),
    Loopback(LoopbackTx),
}

impl ClientLink {
    /// Queues an encoded frame; closes the session instead when the client lets its queue grow
    /// past [`SEND_QUEUE_CAP`].
    pub fn send_frame(&self, frame: Frame) {
        let before = self.queued.fetch_add(frame.len(), Ordering::Relaxed);
        if before > SEND_QUEUE_CAP {
            self.queued.fetch_sub(frame.len(), Ordering::Relaxed);
            self.kick(
                close::SLOW_READER,
                &format!("over {SEND_QUEUE_CAP} bytes queued: client not reading"),
            );
            return;
        }
        // Fails only when the session task is gone; the disconnect is picked up in `receive`.
        let _ = self.out.send(frame);
    }

    /// Encodes and queues `message`; `false` when it is over [`MAX_SERVER_FRAME`] and was dropped.
    pub fn send(&self, message: &ServerMessage) -> bool {
        match encode_message(message) {
            Some(frame) => {
                self.send_frame(frame);
                true
            }
            None => false,
        }
    }

    /// Sends motion as datagrams below the path's current datagram limit.
    pub fn send_motion(&self, tick: u32, moves: &[Motion]) {
        match &self.conn {
            #[cfg(not(target_arch = "wasm32"))]
            LinkConn::Quic(conn) => {
                let Some(max) = conn.max_datagram_size() else {
                    return; // Peer without datagram support; settles still bring the final state.
                };
                for datagram in pack_motion(tick, moves, max) {
                    // Unreliable by contract: a full buffer or a shrunk path drops it.
                    let _ = conn.send_datagram(datagram);
                }
            }
            LinkConn::Loopback(tx) => {
                for datagram in pack_motion(tick, moves, crate::loopback::MAX_DATAGRAM) {
                    tx.send_datagram(datagram);
                }
            }
        }
    }

    pub fn kick(&self, code: u32, reason: &str) {
        match &self.conn {
            #[cfg(not(target_arch = "wasm32"))]
            LinkConn::Quic(conn) => conn.close(crate::protocol::code(code), reason.as_bytes()),
            LinkConn::Loopback(tx) => tx.close(code, reason),
        }
    }

    /// UDP payload bytes and packets QUIC has sent on this session.
    pub fn sent(&self) -> (u64, u64) {
        match &self.conn {
            #[cfg(not(target_arch = "wasm32"))]
            LinkConn::Quic(conn) => {
                let stats = conn.quic_connection().stats();
                (stats.udp_tx.bytes, stats.udp_tx.datagrams)
            }
            // shortcut: no traffic stats in memory; count frames here if a web build needs them.
            LinkConn::Loopback(_) => (0, 0),
        }
    }

    #[cfg(test)]
    pub fn queued(&self) -> usize {
        self.queued.load(Ordering::Relaxed)
    }
}

/// `None` (and an error log) when the encoded message is over [`MAX_SERVER_FRAME`].
/// QUIC unless a loopback listener is inserted: an in-memory server opens no socket.
fn start_server(
    mut commands: Commands,
    mut config: ResMut<NetServerConfig>,
    listener: Option<Res<crate::loopback::LoopbackListener>>,
) -> Result {
    let (hellos_tx, hellos) = mpsc::unbounded_channel();
    #[cfg(not(target_arch = "wasm32"))]
    let shutdown = match listener {
        Some(_) => None,
        None => Some(crate::server_quic::start(
            &mut commands,
            &mut config,
            hellos_tx.clone(),
        )?),
    };
    #[cfg(target_arch = "wasm32")]
    let _ = (&mut config, listener);
    commands.insert_resource(ServerTransport {
        hellos,
        hellos_tx,
        #[cfg(not(target_arch = "wasm32"))]
        _shutdown: shutdown,
    });
    Ok(())
}

pub(crate) fn encode_message(message: &ServerMessage) -> Option<Frame> {
    match encode_frame(message, MAX_SERVER_FRAME) {
        Ok(frame) => Some(frame),
        Err(e) => {
            error!("server message not sent: {e}");
            None
        }
    }
}

/// Disconnected; despawned when the count reaches 0 in `Last`.
#[derive(Component)]
struct Gone(u8);

/// A decoded [`Hello`] waiting for Bevy to judge it.
pub(crate) struct PendingHello {
    /// For logs: the peer address, or `loopback`.
    pub remote: String,
    pub hello: Hello,
    pub reply: oneshot::Sender<HelloReply>,
    pub link: ClientLink,
}

#[derive(Resource)]
pub(crate) struct ServerTransport {
    // Unbounded but bounded in practice: at most one hello per admitted session.
    hellos: mpsc::UnboundedReceiver<PendingHello>,
    pub(crate) hellos_tx: mpsc::UnboundedSender<PendingHello>,
    /// Dropped with the app: the accept loop then closes the endpoint and every session.
    #[cfg(not(target_arch = "wasm32"))]
    _shutdown: Option<oneshot::Sender<()>>,
}

impl Plugin for NetServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            crate::ui::server::plugin,
            crate::chat::server::plugin,
            crate::debug::server::plugin,
            crate::admin::server::plugin,
            crate::move_ack::server::plugin,
            crate::camera::server::plugin,
            crate::sound::server::plugin,
            crate::sim_latency::plugin,
        ))
        .init_resource::<NetServerConfig>()
        .init_resource::<TokenValidator>()
        .init_resource::<crate::server_loopback::LoopbackSessions>()
        .add_message::<ClientConnected>()
        .add_message::<ClientDisconnected>()
        .add_message::<IntentReceived>()
        .add_message::<SendNotice>()
        .add_message::<KickClient>()
        .add_systems(Startup, start_server)
        .add_systems(
            PreUpdate,
            (
                crate::server_loopback::accept.run_if(crate::server_loopback::listening),
                judge_hellos,
                crate::server_loopback::answer.run_if(crate::server_loopback::listening),
                crate::server_loopback::pump.run_if(crate::server_loopback::listening),
                receive,
            )
                .chain()
                .in_set(NetServerSystems::Receive)
                .run_if(resource_exists::<ServerTransport>),
        )
        .add_systems(PostUpdate, send_notices.in_set(NetServerSystems::Send))
        .add_systems(Last, kick.before(despawn_gone))
        .add_systems(Last, despawn_gone);
    }
}

/// `Err` carries the reason shown to the client.
fn check_hello(hello: &Hello, validator: TokenValidator) -> Result<(), String> {
    if hello.version != PROTOCOL_VERSION {
        return Err(format!(
            "protocol version mismatch: server {PROTOCOL_VERSION}, client {}",
            hello.version
        ));
    }
    (validator.check)(&hello.token)
}

fn judge_hellos(
    mut transport: ResMut<ServerTransport>,
    validator: Res<TokenValidator>,
    mut connected: MessageWriter<ClientConnected>,
    mut commands: Commands,
) {
    while let Ok(PendingHello {
        remote,
        hello,
        reply,
        link,
    }) = transport.hellos.try_recv()
    {
        match check_hello(&hello, *validator) {
            Ok(()) => {
                let client = commands
                    .spawn((
                        Name::new(format!("Client {remote}")),
                        link,
                        ClientToken(hello.token),
                    ))
                    .id();
                info!("client {client} from {remote} accepted");
                let _ = reply.send(HelloReply::Accepted);
                connected.write(ClientConnected { client });
            }
            Err(mut reason) => {
                info!("client from {remote} rejected: {reason}");
                if reason.len() > 1024 {
                    let mut end = 1024;
                    while !reason.is_char_boundary(end) {
                        end -= 1;
                    }
                    reason.truncate(end);
                }
                let _ = reply.send(HelloReply::Rejected(reason));
            }
        }
    }
}

fn receive(
    mut clients: Query<(Entity, &mut ClientLink, Option<&mut DroppedIntents>)>,
    latency: Res<SimLatency>,
    mut intents: MessageWriter<IntentReceived>,
    mut ui: MessageWriter<UiMessageReceived>,
    mut debug: MessageWriter<crate::debug::server::DebugRequestReceived>,
    mut admin: MessageWriter<crate::admin::server::AdminRequestReceived>,
    mut disconnected: MessageWriter<ClientDisconnected>,
    mut commands: Commands,
) {
    for (client, mut link, counter) in &mut clients {
        let gone = match link.closed.try_recv() {
            Ok(reason) => Some(reason),
            Err(oneshot::error::TryRecvError::Closed) => Some("connection lost".into()),
            Err(oneshot::error::TryRecvError::Empty) => None,
        };
        if let Some(reason) = gone {
            info!("client {client} disconnected: {reason}");
            disconnected.write(ClientDisconnected { client, reason });
            commands
                .entity(client)
                .remove::<ClientLink>()
                .insert(Gone(1));
            continue;
        }
        // Intents from the same frame stay in order; past the cap they are dropped.
        let (mut delivered, mut dropped) = (0, 0u64);
        link.inbox.drain(latency.0, |message| match message {
            ClientMessage::Intent(intent) if delivered < MAX_INTENTS_PER_TICK => {
                delivered += 1;
                debug!("intent from {client}: {intent:?}");
                intents.write(IntentReceived { client, intent });
            }
            ClientMessage::Intent(_) => dropped += 1,
            ClientMessage::Ui(message) => {
                ui.write(UiMessageReceived { client, message });
            }
            ClientMessage::Debug(request) => {
                debug.write(crate::debug::server::DebugRequestReceived { client, request });
            }
            ClientMessage::Admin(request) => {
                admin.write(crate::admin::server::AdminRequestReceived { client, request });
            }
        });
        if dropped > 0 {
            debug!("dropped {dropped} intents from {client}: over {MAX_INTENTS_PER_TICK} per tick");
            match counter {
                Some(mut counter) => counter.0 += dropped,
                None => {
                    commands.entity(client).try_insert(DroppedIntents(dropped));
                }
            }
        }
    }
}

fn despawn_gone(mut gone: Query<(Entity, &mut Gone)>, mut commands: Commands) {
    for (entity, mut gone) in &mut gone {
        match gone.0.checked_sub(1) {
            Some(left) => gone.0 = left,
            None => commands.entity(entity).despawn(),
        }
    }
}

/// In `Last`, after every send of the frame was queued, so a kicked client may still get the
/// messages that explain it (the close can overtake them in flight).
fn kick(mut kicks: MessageReader<KickClient>, clients: Query<&ClientLink>) {
    for KickClient { client, reason } in kicks.read() {
        if let Ok(link) = clients.get(*client) {
            link.kick(close::KICKED, reason);
        }
    }
}

fn send_notices(mut notices: MessageReader<SendNotice>, clients: Query<&ClientLink>) {
    for SendNotice { client, notice } in notices.read() {
        match clients.get(*client) {
            Ok(link) => {
                link.send(&ServerMessage::Notice(notice.clone()));
            }
            Err(_) => debug!("notice to {client} dropped: not a connected client"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_checks_version_then_token() {
        let validator = TokenValidator::default();
        let hello = |version, token: &str| Hello {
            version,
            token: token.into(),
        };
        assert_eq!(
            check_hello(&hello(PROTOCOL_VERSION, "dev"), validator),
            Ok(())
        );
        assert!(
            check_hello(&hello(PROTOCOL_VERSION + 1, "dev"), validator)
                .unwrap_err()
                .contains("protocol version mismatch")
        );
        assert!(check_hello(&hello(PROTOCOL_VERSION, ""), validator).is_err());
    }
}
