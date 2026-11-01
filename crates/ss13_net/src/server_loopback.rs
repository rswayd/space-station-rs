//! The in-memory half of the server (`loopback`): Bevy systems in `PreUpdate` that take new
//! connections, decode their [`Hello`] into the same [`PendingHello`] the QUIC sessions produce
//! (judged by `judge_hellos`), answer it, and then feed each connection's frames to its
//! [`ClientLink`]. Limits and decoding are those of the QUIC path (`protocol`); the link sends
//! straight into the connection, and the peer's reads drain its counted send queue.

use bevy::platform::time::Instant;
use bevy::prelude::*;
use tokio::sync::{mpsc, oneshot};

use crate::loopback::{LoopbackEnd, LoopbackListener};
use crate::protocol::{
    ClientMessage, Hello, HelloReply, MAX_CLIENT_FRAME, MAX_HELLO_FRAME, close, decode_frame,
    encode_frame,
};
use crate::server::{
    ClientLink, HANDSHAKE_TIMEOUT, INBOX_FRAMES, LinkConn, NetServerConfig, PendingHello,
    ServerTransport,
};
use crate::sim_latency::Inbox;

/// Connections of the [`LoopbackListener`], from the first frame to the end.
#[derive(Resource, Default)]
pub(crate) struct LoopbackSessions {
    /// Waiting for a [`Hello`] until the deadline.
    hello: Vec<(LoopbackEnd, Instant)>,
    /// Handed to `judge_hellos`, which answers in the same frame.
    judged: Vec<(Session, oneshot::Receiver<HelloReply>)>,
    live: Vec<Session>,
}

/// The transport end of one connection (the `session` task of the QUIC path).
pub(crate) struct Session {
    end: LoopbackEnd,
    inbox: mpsc::Sender<(Instant, ClientMessage)>,
    closed: oneshot::Sender<String>,
}

pub(crate) fn listening(listener: Option<Res<LoopbackListener>>) -> bool {
    listener.is_some()
}

/// Takes new connections and decodes their [`Hello`].
pub(crate) fn accept(
    mut listener: ResMut<LoopbackListener>,
    mut sessions: ResMut<LoopbackSessions>,
    config: Res<NetServerConfig>,
    transport: Res<ServerTransport>,
) {
    let sessions = &mut *sessions;
    let now = Instant::now();
    while let Ok(end) = listener.0.try_recv() {
        // shortcut: only the total cap; per-address caps mean nothing for in-memory peers.
        if sessions.hello.len() + sessions.judged.len() + sessions.live.len() >= config.max_clients
        {
            end.close(close::REJECTED, "server full");
            continue;
        }
        sessions.hello.push((end, now + HANDSHAKE_TIMEOUT));
    }
    for (mut end, deadline) in std::mem::take(&mut sessions.hello) {
        let frame = match end.recv_frame() {
            Ok(Some(frame)) => frame,
            Ok(None) if now < deadline => {
                sessions.hello.push((end, deadline));
                continue;
            }
            Ok(None) => {
                end.close(close::HANDSHAKE_TIMEOUT, "handshake timed out");
                continue;
            }
            Err(_) => continue,
        };
        let hello: Hello = match decode_frame(&frame, MAX_HELLO_FRAME) {
            Ok(hello) => hello,
            Err(e) => {
                debug!("loopback session: bad hello: {e}");
                end.close(close::PROTOCOL, &e.to_string());
                continue;
            }
        };
        let (inbox_tx, inbox_rx) = mpsc::channel(INBOX_FRAMES);
        let (closed_tx, closed_rx) = oneshot::channel();
        let (reply_tx, reply_rx) = oneshot::channel();
        let link = ClientLink {
            conn: LinkConn::Loopback(end.tx.clone()),
            out: end.tx.frames.clone(),
            queued: end.tx.unread.clone(),
            inbox: Inbox::new(inbox_rx),
            closed: closed_rx,
        };
        let pending = PendingHello {
            remote: "loopback".into(),
            hello,
            reply: reply_tx,
            link,
        };
        if transport.hellos_tx.send(pending).is_err() {
            continue;
        }
        let session = Session {
            end,
            inbox: inbox_tx,
            closed: closed_tx,
        };
        sessions.judged.push((session, reply_rx));
    }
}

/// Sends `judge_hellos`' answers; a rejected connection is closed after its reason.
pub(crate) fn answer(mut sessions: ResMut<LoopbackSessions>) {
    let sessions = &mut *sessions;
    for (session, mut reply) in std::mem::take(&mut sessions.judged) {
        let reply = match reply.try_recv() {
            Ok(reply) => reply,
            Err(oneshot::error::TryRecvError::Empty) => {
                sessions.judged.push((session, reply));
                continue;
            }
            Err(oneshot::error::TryRecvError::Closed) => {
                session
                    .end
                    .close(close::HANDSHAKE_TIMEOUT, "server did not answer");
                continue;
            }
        };
        let frame = encode_frame(&reply, MAX_HELLO_FRAME).expect("reasons are truncated");
        session.end.tx.send_frame(frame);
        match reply {
            HelloReply::Accepted => sessions.live.push(session),
            HelloReply::Rejected(reason) => session.end.close(close::REJECTED, &reason),
        }
    }
}

/// Decodes what each live connection sent into its link's inbox, and ends sessions that closed.
pub(crate) fn pump(mut sessions: ResMut<LoopbackSessions>) {
    for mut session in std::mem::take(&mut sessions.live) {
        match pump_one(&mut session) {
            Ok(()) => sessions.live.push(session),
            Err(reason) => {
                let _ = session.closed.send(reason);
            }
        }
    }
}

/// `Err` with the reason when the session is over.
fn pump_one(session: &mut Session) -> Result<(), String> {
    if session.closed.is_closed() {
        // The link is gone: the client entity was despawned or the app is shutting down.
        let reason = "server closed the session";
        session.end.close(close::SHUTDOWN, reason);
        return Err(reason.into());
    }
    // Closed from either side (a kick, the client leaving): nothing more is delivered, as in QUIC.
    if let Some((_, reason)) = session.end.closed() {
        return Err(reason);
    }
    // A full inbox leaves frames in the connection, like QUIC flow control.
    while let Ok(permit) = session.inbox.try_reserve() {
        let frame = match session.end.recv_frame() {
            Ok(Some(frame)) => frame,
            Ok(None) => break,
            Err((_, reason)) => return Err(reason),
        };
        match decode_frame(&frame, MAX_CLIENT_FRAME) {
            Ok(message) => permit.send((Instant::now(), message)),
            Err(e) => {
                let reason = e.to_string();
                session.end.close(close::PROTOCOL, &reason);
                return Err(reason);
            }
        }
    }
    Ok(())
}
