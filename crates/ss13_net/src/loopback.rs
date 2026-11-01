//! In-memory transport: the frames of the WebTransport stream (length prefix, postcard, the same
//! limits and decoding) and motion datagrams over channels, for a server and a client in one
//! process — natively in tests, in the browser across a Web Worker boundary once a bridge forwards
//! [`LoopbackEnd`] traffic over `postMessage` (`docs/WEB.md`).
//!
//! No async runtime: both ends are pumped by Bevy systems (`server::loopback`, `client`), so it
//! works on wasm32 where tokio has no runtime. Insert [`LoopbackListener`] before
//! `NetServerPlugin` and [`LoopbackConnector`] before `NetClientPlugin`; [`listener`] makes both.

// A client-only build uses only the client half.
#![cfg_attr(not(feature = "server"), allow(dead_code))]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use bevy::prelude::*;
use tokio::sync::mpsc;

use crate::protocol::Frame;

/// Motion datagrams are packed up to this size, like a typical QUIC path.
pub(crate) const MAX_DATAGRAM: usize = 1200;
/// Datagrams waiting for the peer; more are dropped, as QUIC drops old motion.
const DATAGRAM_QUEUE: usize = 64;

/// Who closed the connection first, with an application code (`protocol::close`) and reason.
type CloseState = Arc<Mutex<Option<(u32, String)>>>;

/// The sending half of an end; cloned into the server's `ClientLink`.
#[derive(Clone)]
pub(crate) struct LoopbackTx {
    /// A server's `ClientLink` sends on a clone of it, counting into `unread` itself.
    pub(crate) frames: mpsc::UnboundedSender<Frame>,
    datagrams: mpsc::Sender<Vec<u8>>,
    /// Bytes sent and not yet read by the peer: the send queue the server bounds.
    pub(crate) unread: Arc<AtomicUsize>,
    close: CloseState,
}

/// One end of an in-memory connection; dropping it closes the connection for the peer.
pub struct LoopbackEnd {
    pub(crate) tx: LoopbackTx,
    frames: mpsc::UnboundedReceiver<Frame>,
    datagrams: mpsc::Receiver<Vec<u8>>,
    /// The peer's `tx.unread`.
    peer_unread: Arc<AtomicUsize>,
}

/// Two connected ends.
pub fn pipe() -> (LoopbackEnd, LoopbackEnd) {
    let close = CloseState::default();
    let (a_frames, b_frames_rx) = mpsc::unbounded_channel();
    let (b_frames, a_frames_rx) = mpsc::unbounded_channel();
    let (a_datagrams, b_datagrams_rx) = mpsc::channel(DATAGRAM_QUEUE);
    let (b_datagrams, a_datagrams_rx) = mpsc::channel(DATAGRAM_QUEUE);
    let (a_unread, b_unread) = (Arc::<AtomicUsize>::default(), Arc::<AtomicUsize>::default());
    let a = LoopbackEnd {
        tx: LoopbackTx {
            frames: a_frames,
            datagrams: a_datagrams,
            unread: a_unread.clone(),
            close: close.clone(),
        },
        frames: a_frames_rx,
        datagrams: a_datagrams_rx,
        peer_unread: b_unread.clone(),
    };
    let b = LoopbackEnd {
        tx: LoopbackTx {
            frames: b_frames,
            datagrams: b_datagrams,
            unread: b_unread,
            close,
        },
        frames: b_frames_rx,
        datagrams: b_datagrams_rx,
        peer_unread: a_unread,
    };
    (a, b)
}

impl LoopbackTx {
    /// Queues an encoded frame (prefix included); dropped when the peer is gone.
    pub(crate) fn send_frame(&self, frame: Frame) {
        self.unread.fetch_add(frame.len(), Ordering::Relaxed);
        let _ = self.frames.send(frame);
    }

    /// Unreliable: dropped when the peer's queue is full.
    pub(crate) fn send_datagram(&self, datagram: Vec<u8>) {
        let _ = self.datagrams.try_send(datagram);
    }

    /// Closes for both ends; the first close wins, as in QUIC.
    pub(crate) fn close(&self, code: u32, reason: &str) {
        self.close
            .lock()
            .expect("close lock")
            .get_or_insert_with(|| (code, reason.into()));
    }

    /// The close code and reason once either end closed or dropped its end.
    pub(crate) fn closed(&self) -> Option<(u32, String)> {
        if let Some(close) = &*self.close.lock().expect("close lock") {
            return Some(close.clone());
        }
        self.frames
            .is_closed()
            .then(|| (crate::protocol::close::NORMAL, "connection lost".into()))
    }
}

impl LoopbackEnd {
    /// Sends an encoded frame, as produced by the other end's [`Self::recv_frame`].
    pub fn send_frame(&self, frame: &[u8]) {
        self.tx.send_frame(frame.into());
    }

    pub fn send_datagram(&self, datagram: Vec<u8>) {
        self.tx.send_datagram(datagram);
    }

    /// The next received frame, prefix included; frames sent before a close are still
    /// delivered (a rejection's reason precedes it). `Err` once nothing is left and the
    /// connection is closed: `(code, reason)`.
    pub fn recv_frame(&mut self) -> Result<Option<Frame>, (u32, String)> {
        match self.frames.try_recv() {
            Ok(frame) => {
                self.peer_unread.fetch_sub(frame.len(), Ordering::Relaxed);
                Ok(Some(frame))
            }
            Err(_) => match self.tx.closed() {
                Some(closed) => Err(closed),
                None if self.frames.is_closed() => {
                    Err((crate::protocol::close::NORMAL, "connection lost".into()))
                }
                None => Ok(None),
            },
        }
    }

    pub fn recv_datagram(&mut self) -> Option<Vec<u8>> {
        self.datagrams.try_recv().ok()
    }

    pub fn close(&self, code: u32, reason: &str) {
        self.tx.close(code, reason);
    }

    pub fn closed(&self) -> Option<(u32, String)> {
        self.tx.closed()
    }
}

/// The server's side of [`listener`]: new connections.
#[derive(Resource)]
pub struct LoopbackListener(pub(crate) mpsc::UnboundedReceiver<LoopbackEnd>);

/// The clients' side of [`listener`]: each [`Self::connect`] opens a connection.
#[derive(Resource, Clone)]
pub struct LoopbackConnector(mpsc::UnboundedSender<LoopbackEnd>);

pub fn listener() -> (LoopbackListener, LoopbackConnector) {
    let (tx, rx) = mpsc::unbounded_channel();
    (LoopbackListener(rx), LoopbackConnector(tx))
}

impl LoopbackListener {
    /// The next connection a client opened, if any: for a bridge that forwards connections to a
    /// server elsewhere (the browser page, `docs/WEB.md`) instead of `NetServerPlugin`.
    pub fn accept(&mut self) -> Option<LoopbackEnd> {
        self.0.try_recv().ok()
    }
}

impl LoopbackConnector {
    /// The client end of a new connection; `None` when the server is gone.
    pub fn connect(&self) -> Option<LoopbackEnd> {
        let (client, server) = pipe();
        self.0.send(server).ok()?;
        Some(client)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_before_a_close_are_delivered_then_the_close() {
        let (mut a, b) = pipe();
        b.send_frame(&[1, 2, 3]);
        assert_eq!(a.tx.unread.load(Ordering::Relaxed), 0);
        assert_eq!(b.tx.unread.load(Ordering::Relaxed), 3);
        b.close(7, "bye");
        assert_eq!(&*a.recv_frame().unwrap().unwrap(), &[1, 2, 3]);
        assert_eq!(b.tx.unread.load(Ordering::Relaxed), 0);
        assert_eq!(a.recv_frame().unwrap_err(), (7, "bye".into()));
        b.close(1, "later");
        assert_eq!(a.closed(), Some((7, "bye".into())));
    }

    #[test]
    fn dropping_an_end_closes_the_connection() {
        let (mut a, b) = pipe();
        assert_eq!(a.recv_frame().unwrap(), None);
        drop(b);
        assert!(a.recv_frame().is_err());
        assert!(a.closed().is_some());
    }
}
