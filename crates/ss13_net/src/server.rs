//! Server side: WebTransport endpoint, admission, handshake, and delivery between sessions and
//! Bevy.
//!
//! Game plugins see only the Bevy messages defined here. A client is identified by its server-side
//! connection entity, spawned when its handshake is accepted ([`ClientConnected`]) and valid until
//! [`ClientDisconnected`].
//!
//! Every session runs as a task on the transport runtime ([`crate::protocol::runtime`]); Bevy
//! systems exchange data with it through bounded channels only:
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

use core::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use core::time::Duration;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use ss13_shared::protocol::{ClientIntent, PROTOCOL_VERSION};
use tokio::sync::{mpsc, oneshot};
use wtransport::config::QuicTransportConfig;
use wtransport::endpoint::IncomingSession;
use wtransport::endpoint::endpoint_side::Server;
use wtransport::{Connection, Endpoint, Identity, RecvStream, SendStream, ServerConfig};

pub use crate::light_server::LIGHT_MARGIN;
pub use crate::line_of_sight::ViewGrid;
pub use crate::replication_server::{
    ClientTraffic, ControlledEntity, DEFAULT_VIEW_RADIUS, Interest, Invisibility, KEEP_MARGIN,
    Networked, ReplicationServerPlugin, ReplicationStats, SeeGroups, SeeInvisible, Sight, Viewer,
    VisibleToGroups, los_mask,
};
pub use crate::view_mask::{MASK_CHUNK, MaskChunk, MaskId, ViewMask, ViewMasks, mask_chunk};

use crate::protocol::{
    ClientMessage, DEFAULT_PORT, Frame, FrameError, Hello, HelloReply, IDLE_TIMEOUT, KEEP_ALIVE,
    MAX_CLIENT_FRAME, MAX_HELLO_FRAME, MAX_SERVER_FRAME, ServerMessage, ServerNotice, close,
    close_reason, code, encode_frame, lost_reason, pack_motion, read_frame, runtime,
};
use crate::replication::Motion;
use crate::sim_latency::{Inbox, SimLatency};
use crate::ui::server::UiMessageReceived;

/// From the first QUIC packet to a decoded [`Hello`].
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
/// Time for a rejection reason to reach the client before the server closes the session itself.
const REJECT_GRACE: Duration = Duration::from_secs(1);
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
/// Handshakes in progress over all clients; past half of it, unvalidated addresses get a QUIC
/// retry first, so spoofed source addresses cannot hold handshake slots.
const MAX_PENDING: usize = 256;
const RETRY_ABOVE_PENDING: usize = MAX_PENDING / 2;
/// QUIC buffers per client. The client sends small frames: a 128 KiB stream window holds two
/// maximal frames. Unacknowledged server data is capped by `send_window` (10 MB/s at 100 ms RTT).
const STREAM_RECEIVE_WINDOW: u32 = 128 * 1024;
const RECEIVE_WINDOW: u32 = 256 * 1024;
const SEND_WINDOW: u64 = 1024 * 1024;
/// Old motion is worthless: queued datagrams past this are dropped oldest first.
const DATAGRAM_SEND_BUFFER: usize = 32 * 1024;

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
    conn: Connection,
    out: mpsc::UnboundedSender<Frame>,
    /// Bytes in `out` not yet handed to QUIC; bounds `out`.
    queued: Arc<AtomicUsize>,
    inbox: Inbox<ClientMessage>,
    closed: oneshot::Receiver<String>,
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
        let Some(max) = self.conn.max_datagram_size() else {
            return; // Peer without datagram support; settles still bring the final state.
        };
        for datagram in pack_motion(tick, moves, max) {
            // Unreliable by contract: a full buffer or a shrunk path drops it.
            let _ = self.conn.send_datagram(datagram);
        }
    }

    pub fn kick(&self, code_: u32, reason: &str) {
        self.conn.close(code(code_), reason.as_bytes());
    }

    /// UDP payload bytes and packets QUIC has sent on this session.
    pub fn sent(&self) -> (u64, u64) {
        let stats = self.conn.quic_connection().stats();
        (stats.udp_tx.bytes, stats.udp_tx.datagrams)
    }

    #[cfg(test)]
    pub fn queued(&self) -> usize {
        self.queued.load(Ordering::Relaxed)
    }
}

/// `None` (and an error log) when the encoded message is over [`MAX_SERVER_FRAME`].
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
struct PendingHello {
    remote: SocketAddr,
    hello: Hello,
    reply: oneshot::Sender<HelloReply>,
    link: ClientLink,
}

#[derive(Resource)]
struct ServerTransport {
    // Unbounded but bounded in practice: at most one hello per admitted session.
    hellos: mpsc::UnboundedReceiver<PendingHello>,
    /// Dropped with the app: the accept loop then closes the endpoint and every session.
    _shutdown: oneshot::Sender<()>,
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
        .add_message::<ClientConnected>()
        .add_message::<ClientDisconnected>()
        .add_message::<IntentReceived>()
        .add_message::<SendNotice>()
        .add_message::<KickClient>()
        .add_systems(Startup, start_server)
        .add_systems(
            PreUpdate,
            (judge_hellos, receive)
                .chain()
                .in_set(NetServerSystems::Receive)
                .run_if(resource_exists::<ServerTransport>),
        )
        .add_systems(PostUpdate, send_notices.in_set(NetServerSystems::Send))
        .add_systems(Last, kick.before(despawn_gone))
        .add_systems(Last, despawn_gone);
    }
}

fn start_server(mut commands: Commands, mut config: ResMut<NetServerConfig>) -> Result {
    let identity = match &config.tls {
        Some(files) => runtime().block_on(Identity::load_pemfiles(
            &files.cert_chain,
            &files.private_key,
        ))?,
        None => self_signed()?,
    };
    let digest: String = identity.certificate_chain().as_slice()[0]
        .hash()
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();

    let mut transport = QuicTransportConfig::default();
    // The WebTransport CONNECT stream and ours; HTTP/3 control and two QPACK streams.
    transport
        .max_concurrent_bidi_streams(2u32.into())
        .max_concurrent_uni_streams(3u32.into())
        .stream_receive_window(STREAM_RECEIVE_WINDOW.into())
        .receive_window(RECEIVE_WINDOW.into())
        .send_window(SEND_WINDOW)
        // Clients send no datagrams; a tiny buffer drops any.
        .datagram_receive_buffer_size(Some(2048))
        .datagram_send_buffer_size(DATAGRAM_SEND_BUFFER);
    let mut server_config = ServerConfig::builder()
        .with_bind_address(config.bind_addr)
        .with_custom_transport(identity, transport)
        .keep_alive_interval(Some(KEEP_ALIVE))
        .max_idle_timeout(Some(IDLE_TIMEOUT))?
        .build();
    server_config
        .quic_config_mut()
        .max_incoming(MAX_PENDING * 2);

    let endpoint = {
        let _runtime = runtime().enter();
        Endpoint::server(server_config)?
    };
    config.bind_addr = endpoint.local_addr()?;
    info!(
        "WebTransport server on {}, certificate digest {digest}",
        config.bind_addr
    );
    commands.insert_resource(ServerCertificateDigest(digest));

    let admission = Arc::new(Admission {
        max_total: config.max_clients,
        max_per_ip: config.max_clients_per_ip,
        state: Mutex::default(),
    });
    let (hellos_tx, hellos) = mpsc::unbounded_channel();
    let (shutdown_tx, shutdown) = oneshot::channel();
    runtime().spawn(accept_loop(endpoint, admission, hellos_tx, shutdown));
    commands.insert_resource(ServerTransport {
        hellos,
        _shutdown: shutdown_tx,
    });
    Ok(())
}

/// One self-signed identity per process: a round restart builds a new server `App` in the same
/// process (`ss13_game::round`), and reconnecting clients still pin the digest they had.
// shortcut: the certificate is valid 14 days from the process start; a process living longer
// needs `SS13_TLS_CERT`/`SS13_TLS_KEY` or a fresh certificate per restart with a digest exchange.
fn self_signed() -> Result<Identity, wtransport::tls::error::InvalidSan> {
    static IDENTITY: std::sync::OnceLock<Identity> = std::sync::OnceLock::new();
    if let Some(identity) = IDENTITY.get() {
        return Ok(identity.clone_identity());
    }
    let identity = Identity::self_signed(["localhost", "127.0.0.1", "::1"])?;
    Ok(IDENTITY.get_or_init(|| identity).clone_identity())
}

async fn accept_loop(
    endpoint: Endpoint<Server>,
    admission: Arc<Admission>,
    hellos: mpsc::UnboundedSender<PendingHello>,
    mut shutdown: oneshot::Receiver<()>,
) {
    loop {
        // Closed only here, after the last `accept`: wtransport panics in an `accept` racing a
        // close. QUIC keeps sending the close to every session after the loop ends.
        let incoming = tokio::select! {
            incoming = endpoint.accept() => incoming,
            _ = &mut shutdown => {
                return endpoint.close(code(close::SHUTDOWN), b"server shutting down");
            }
        };
        match admission.admit(&incoming) {
            Admit::Yes(permit) => {
                tokio::spawn(session(incoming, permit, hellos.clone()));
            }
            Admit::Retry => incoming.retry(),
            Admit::Refuse(why) => {
                debug!("refused {}: {why}", incoming.remote_address());
                incoming.refuse();
            }
        }
    }
}

/// One client from its first packet to the end. The permit is held for the whole session.
async fn session(
    incoming: IncomingSession,
    mut permit: Permit,
    hellos: mpsc::UnboundedSender<PendingHello>,
) {
    let remote = incoming.remote_address();
    let deadline = tokio::time::Instant::now() + HANDSHAKE_TIMEOUT;
    // A timeout here drops the half-open QUIC connection.
    let conn =
        match tokio::time::timeout_at(deadline, async { incoming.await?.accept().await }).await {
            Ok(Ok(conn)) => conn,
            Ok(Err(e)) => return debug!("session from {remote} failed: {e}"),
            Err(_) => return debug!("session from {remote}: QUIC handshake timed out"),
        };

    let mut buf = Vec::new();
    let opened = tokio::time::timeout_at(deadline, async {
        let (send, mut recv) = conn
            .accept_bi()
            .await
            .map_err(|e| FrameError::Closed(close_reason(&e)))?;
        let hello: Hello = read_frame(&mut recv, MAX_HELLO_FRAME, &mut buf).await?;
        Ok::<_, FrameError>((send, recv, hello))
    })
    .await;
    let (mut send, recv, hello) = match opened {
        Ok(Ok(opened)) => opened,
        Ok(Err(FrameError::Closed(how))) => return debug!("session from {remote} ended: {how}"),
        Ok(Err(e)) => {
            debug!("session from {remote}: bad hello: {e}");
            return conn.close(code(close::PROTOCOL), e.to_string().as_bytes());
        }
        Err(_) => {
            debug!("session from {remote}: no hello in {HANDSHAKE_TIMEOUT:?}");
            return conn.close(code(close::HANDSHAKE_TIMEOUT), b"handshake timed out");
        }
    };

    let (inbox_tx, inbox_rx) = mpsc::channel(INBOX_FRAMES);
    let (out_tx, out_rx) = mpsc::unbounded_channel();
    let (closed_tx, closed_rx) = oneshot::channel();
    let (reply_tx, reply_rx) = oneshot::channel();
    let queued = Arc::new(AtomicUsize::new(0));
    let link = ClientLink {
        conn: conn.clone(),
        out: out_tx,
        queued: queued.clone(),
        inbox: Inbox::new(inbox_rx),
        closed: closed_rx,
    };
    let pending = PendingHello {
        remote,
        hello,
        reply: reply_tx,
        link,
    };
    if hellos.send(pending).is_err() {
        return;
    }
    // Bevy answers within a frame; a server stuck longer (map load) lets the client retry.
    let Ok(Ok(reply)) = tokio::time::timeout(HANDSHAKE_TIMEOUT, reply_rx).await else {
        return conn.close(code(close::HANDSHAKE_TIMEOUT), b"server did not answer");
    };
    let frame = encode_frame(&reply, MAX_HELLO_FRAME).expect("reasons are truncated");
    if let HelloReply::Rejected(reason) = reply {
        let _ = tokio::time::timeout(REJECT_GRACE, async {
            if send.write_all(&frame).await.is_ok() && send.finish().await.is_ok() {
                // The client closes as soon as it has read the reason.
                conn.closed().await;
            }
        })
        .await;
        return conn.close(code(close::REJECTED), reason.as_bytes());
    }
    if send.write_all(&frame).await.is_err() {
        return;
    }
    permit.promote();

    let reason = tokio::select! {
        reason = read_loop(&conn, recv, inbox_tx) => reason,
        reason = write_loop(&conn, send, out_rx, &queued) => reason,
        error = conn.closed() => close_reason(&error),
    };
    conn.close(code(close::NORMAL), b"");
    let _ = closed_tx.send(reason);
}

async fn read_loop(
    conn: &Connection,
    mut recv: RecvStream,
    inbox: mpsc::Sender<(Instant, ClientMessage)>,
) -> String {
    let mut buf = Vec::new();
    loop {
        match read_frame(&mut recv, MAX_CLIENT_FRAME, &mut buf).await {
            Ok(message) => {
                if inbox.send((Instant::now(), message)).await.is_err() {
                    return "server dropped the client".into();
                }
            }
            Err(FrameError::Closed(how)) => return lost_reason(conn, how).await,
            Err(e) => {
                let reason = e.to_string();
                conn.close(code(close::PROTOCOL), reason.as_bytes());
                return reason;
            }
        }
    }
}

async fn write_loop(
    conn: &Connection,
    mut send: SendStream,
    mut out: mpsc::UnboundedReceiver<Frame>,
    queued: &AtomicUsize,
) -> String {
    while let Some(frame) = out.recv().await {
        match tokio::time::timeout(SEND_STALL_TIMEOUT, send.write_all(&frame)).await {
            Ok(Ok(())) => {
                queued.fetch_sub(frame.len(), Ordering::Relaxed);
            }
            Ok(Err(e)) => return e.to_string(),
            Err(_) => {
                let reason = format!("client did not read for {SEND_STALL_TIMEOUT:?}");
                conn.close(code(close::SLOW_READER), reason.as_bytes());
                return reason;
            }
        }
    }
    // Bevy dropped the link: the client entity is gone or the server app is shutting down.
    let reason = "server closed the session";
    conn.close(code(close::SHUTDOWN), reason.as_bytes());
    reason.into()
}

struct Admission {
    max_total: usize,
    max_per_ip: usize,
    state: Mutex<AdmissionState>,
}

#[derive(Default)]
struct AdmissionState {
    total: usize,
    pending: usize,
    per_ip: HashMap<IpAddr, usize>,
}

enum Admit {
    Yes(Permit),
    Retry,
    Refuse(&'static str),
}

/// One admitted session; frees its slots when dropped, however the session ends.
struct Permit {
    admission: Arc<Admission>,
    ip: IpAddr,
    pending: bool,
}

/// An IPv4 address, or the /64 of an IPv6 one: a single host usually owns a whole /64.
fn ip_key(ip: IpAddr) -> IpAddr {
    match ip.to_canonical() {
        IpAddr::V6(v6) => IpAddr::V6(Ipv6Addr::from_bits(v6.to_bits() & !((1u128 << 64) - 1))),
        v4 => v4,
    }
}

impl Admission {
    fn admit(self: &Arc<Self>, incoming: &IncomingSession) -> Admit {
        let ip = ip_key(incoming.remote_address().ip());
        let mut state = self.state.lock().expect("admission lock");
        if state.total >= self.max_total || state.pending >= MAX_PENDING {
            return Admit::Refuse("server full");
        }
        if state.per_ip.get(&ip).copied().unwrap_or(0) >= self.max_per_ip {
            return Admit::Refuse("too many sessions from this address");
        }
        if state.pending >= RETRY_ABOVE_PENDING && !incoming.remote_address_validated() {
            return Admit::Retry;
        }
        state.total += 1;
        state.pending += 1;
        *state.per_ip.entry(ip).or_default() += 1;
        Admit::Yes(Permit {
            admission: self.clone(),
            ip,
            pending: true,
        })
    }
}

impl Permit {
    fn promote(&mut self) {
        if std::mem::take(&mut self.pending) {
            self.admission.state.lock().expect("admission lock").pending -= 1;
        }
    }
}

impl Drop for Permit {
    fn drop(&mut self) {
        let mut state = self.admission.state.lock().expect("admission lock");
        state.total -= 1;
        if self.pending {
            state.pending -= 1;
        }
        if let Some(count) = state.per_ip.get_mut(&self.ip) {
            *count -= 1;
            if *count == 0 {
                state.per_ip.remove(&self.ip);
            }
        }
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

    #[test]
    fn ipv6_hosts_are_counted_per_64() {
        let a: IpAddr = "2001:db8:1:2:aaaa::1".parse().unwrap();
        let b: IpAddr = "2001:db8:1:2:bbbb::9".parse().unwrap();
        let c: IpAddr = "2001:db8:1:3::1".parse().unwrap();
        assert_eq!(ip_key(a), ip_key(b));
        assert_ne!(ip_key(a), ip_key(c));
        let mapped: IpAddr = "::ffff:10.0.0.1".parse().unwrap();
        assert_eq!(ip_key(mapped), "10.0.0.1".parse::<IpAddr>().unwrap());
    }
}
