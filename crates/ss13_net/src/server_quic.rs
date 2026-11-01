//! The WebTransport (QUIC) half of the server, native only: endpoint, admission, and one task
//! per session on the transport runtime that hands a [`ClientLink`] to Bevy after the handshake.

use core::net::{IpAddr, Ipv6Addr};
use core::time::Duration;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use bevy::platform::collections::HashMap;
use bevy::platform::time::Instant;
use bevy::prelude::*;
use tokio::sync::{mpsc, oneshot};
use wtransport::config::QuicTransportConfig;
use wtransport::endpoint::IncomingSession;
use wtransport::endpoint::endpoint_side::Server;
use wtransport::{Connection, Endpoint, Identity, RecvStream, SendStream, ServerConfig};

use crate::protocol::{
    ClientMessage, Frame, FrameError, Hello, HelloReply, IDLE_TIMEOUT, KEEP_ALIVE,
    MAX_CLIENT_FRAME, MAX_HELLO_FRAME, close, close_reason, code, encode_frame, lost_reason,
    read_frame, runtime,
};
use crate::server::{
    ClientLink, HANDSHAKE_TIMEOUT, INBOX_FRAMES, LinkConn, NetServerConfig, PendingHello,
    SEND_STALL_TIMEOUT, ServerCertificateDigest,
};
use crate::sim_latency::Inbox;

/// Time for a rejection reason to reach the client before the server closes the session itself.
const REJECT_GRACE: Duration = Duration::from_secs(1);
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

/// Binds the endpoint and starts accepting; returns the shutdown handle the server transport
/// holds (dropping it closes the endpoint and every session).
pub(crate) fn start(
    commands: &mut Commands,
    config: &mut NetServerConfig,
    hellos_tx: mpsc::UnboundedSender<PendingHello>,
) -> Result<oneshot::Sender<()>> {
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
    let (shutdown_tx, shutdown) = oneshot::channel();
    runtime().spawn(accept_loop(endpoint, admission, hellos_tx, shutdown));
    Ok(shutdown_tx)
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
        conn: LinkConn::Quic(conn.clone()),
        out: out_tx,
        queued: queued.clone(),
        inbox: Inbox::new(inbox_rx),
        closed: closed_rx,
    };
    let pending = PendingHello {
        remote: remote.to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;

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
