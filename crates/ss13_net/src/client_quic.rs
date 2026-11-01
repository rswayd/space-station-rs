//! The WebTransport (QUIC) half of the client, native only: one task per session on the transport
//! runtime, reporting to Bevy through the session's event channel.

use core::time::Duration;

use bevy::platform::time::Instant;
use tokio::sync::mpsc;
use wtransport::tls::Sha256Digest;
use wtransport::{ClientConfig, Connection, Endpoint, RecvStream, SendStream};

use crate::client::{CONNECT_TIMEOUT, Event, Events, NetClientConfig};
use crate::protocol::{
    Frame, FrameError, Hello, HelloReply, IDLE_TIMEOUT, KEEP_ALIVE, MAX_HELLO_FRAME,
    MAX_SERVER_FRAME, close, close_reason, code, decode_motion, encode_frame, lost_reason,
    read_frame, runtime,
};
use crate::tap::NetLinkStats;

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

/// Starts a session task; returns its outgoing frame queue and handle.
pub(crate) fn spawn(
    config: &NetClientConfig,
    version: u32,
    events: Events,
) -> (mpsc::UnboundedSender<Frame>, tokio::task::JoinHandle<()>) {
    let (out_tx, out_rx) = mpsc::unbounded_channel();
    let task = runtime().spawn(run_session(config.clone(), version, events, out_rx));
    (out_tx, task)
}

pub(crate) fn link_stats(conn: &Connection) -> NetLinkStats {
    let quic = conn.quic_connection();
    let stats = quic.stats();
    NetLinkStats {
        rtt: Some(quic.rtt()),
        udp_tx_bytes: stats.udp_tx.bytes,
        udp_rx_bytes: stats.udp_rx.bytes,
        udp_tx_datagrams: stats.udp_tx.datagrams,
        udp_rx_datagrams: stats.udp_rx.datagrams,
        lost_packets: stats.path.lost_packets,
    }
}

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
        .send((Instant::now(), Event::Accepted(Some(conn.clone()))))
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
