//! Debug tap on the client's traffic, for the debug overlay (`ss13_debug`). Off by default and
//! then free: nothing is formatted or recorded unless [`NetTap::enabled`] is set.
//!
//! While enabled, every message the client sends or receives is reported once as a [`NetPacket`]
//! (in `PreUpdate`, [`NetClientSystems::Receive`](crate::client::NetClientSystems::Receive):
//! received messages of this frame and the messages sent last frame), and [`NetLinkStats`] is
//! refreshed from QUIC.

use core::fmt::{self, Debug, Write};
use core::time::Duration;

use bevy::prelude::*;

use crate::protocol::{ClientMessage, ServerMessage};
use crate::replication::{MotionBatch, ReplicationBatch, ReplicationOp};

/// Longest [`NetPacket::summary`]; a whole-view replication batch would be tens of KiB of text.
const SUMMARY_MAX: usize = 400;

#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct NetTap {
    pub enabled: bool,
}

/// One message on the wire.
#[derive(Message, Clone, Debug, PartialEq)]
pub struct NetPacket {
    pub outgoing: bool,
    /// Message kind: `Intent::Move`, `Ui::Open`, `Replication`, `Motion`, ...
    pub kind: String,
    /// Encoded bytes: the frame with its length prefix, or the datagram payload.
    pub size: usize,
    /// `Debug` of the message, cut at a few hundred bytes.
    pub summary: String,
}

/// QUIC's view of the session; refreshed while the tap is enabled.
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct NetLinkStats {
    pub rtt: Option<Duration>,
    pub udp_tx_bytes: u64,
    pub udp_rx_bytes: u64,
    pub udp_tx_datagrams: u64,
    pub udp_rx_datagrams: u64,
    pub lost_packets: u64,
}

impl NetPacket {
    pub(crate) fn sent(message: &ClientMessage, size: usize) -> Self {
        let (kind, summary) = match message {
            ClientMessage::Intent(i) => (format!("Intent::{}", variant(i)), bounded(i)),
            ClientMessage::Ui(m) => (format!("Ui::{}", variant(m)), bounded(m)),
            ClientMessage::Debug(r) => (format!("Debug::{}", variant(r)), bounded(r)),
            ClientMessage::Admin(r) => (format!("Admin::{}", variant(r)), bounded(r)),
        };
        Self {
            outgoing: true,
            kind,
            size,
            summary,
        }
    }

    pub(crate) fn received(message: &ServerMessage, size: usize) -> Self {
        let (kind, summary) = match message {
            ServerMessage::Notice(n) => ("Notice".into(), bounded(n)),
            ServerMessage::Ui(m) => (format!("Ui::{}", variant(m)), bounded(m)),
            ServerMessage::Chat(c) => ("Chat".into(), bounded(c)),
            ServerMessage::MoveAck(a) => ("MoveAck".into(), bounded(a)),
            ServerMessage::Replication(b) => ("Replication".into(), replication_summary(b)),
            ServerMessage::Debug(r) => (format!("Debug::{}", variant(r)), bounded(r)),
            ServerMessage::Admin(r) => (format!("Admin::{}", variant(r)), bounded(r)),
            ServerMessage::Camera(c) => ("Camera".into(), bounded(c)),
            ServerMessage::Sound(s) => ("Sound".into(), bounded(s)),
            ServerMessage::Animation(a) => ("Animation".into(), bounded(a)),
            ServerMessage::ChannelSound(s) => ("ChannelSound".into(), bounded(s)),
            ServerMessage::CloseSay => ("CloseSay".into(), String::new()),
        };
        Self {
            outgoing: false,
            kind,
            size,
            summary,
        }
    }

    pub(crate) fn motion(batch: &MotionBatch, size: usize) -> Self {
        Self {
            outgoing: false,
            kind: "Motion".into(),
            size,
            summary: format!("tick {}, {} moves", batch.tick, batch.moves.len()),
        }
    }
}

/// Op counts instead of the ops: the interesting part of a batch is what it does.
fn replication_summary(batch: &ReplicationBatch) -> String {
    let mut counts = [0usize; 13];
    let mut spawned = 0;
    for op in &batch.ops {
        let i = match op {
            ReplicationOp::Spawn { entities, .. } => {
                spawned += entities.len();
                0
            }
            ReplicationOp::Despawn(_) => 1,
            ReplicationOp::Appearance(..) => 2,
            ReplicationOp::Settle { .. } => 3,
            ReplicationOp::Control(_) => 4,
            ReplicationOp::LightBlock { .. } => 5,
            ReplicationOp::Opacity { .. } => 6,
            ReplicationOp::Lights { .. } => 7,
            ReplicationOp::ForgetLight(_) => 8,
            ReplicationOp::Seen { .. } => 9,
            ReplicationOp::LineOfSight(_) => 10,
            ReplicationOp::Blocks(..) => 11,
            ReplicationOp::Stack(_) => 12,
        };
        counts[i] += 1;
    }
    const NAMES: [&str; 13] = [
        "spawn tiles",
        "despawn",
        "appearance",
        "settle",
        "control",
        "light block",
        "opacity",
        "lights",
        "forget light",
        "seen",
        "line of sight",
        "blocks",
        "level stack",
    ];
    let mut out = format!(
        "tick {}, {} appearances, {} entities spawned",
        batch.tick,
        batch.appearances.len(),
        spawned
    );
    for (name, count) in NAMES.iter().zip(counts) {
        if count > 0 {
            let _ = write!(out, ", {name} {count}");
        }
    }
    out
}

/// The variant name at the start of a `Debug` output (`Move { .. }` → `Move`).
fn variant(value: &impl Debug) -> String {
    let text = bounded_to(value, 64);
    let end = text
        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
        .unwrap_or(text.len());
    text[..end].to_owned()
}

fn bounded(value: &impl Debug) -> String {
    bounded_to(value, SUMMARY_MAX)
}

/// `Debug` of `value` cut at `max` bytes; formatting stops there instead of running to the end.
pub(crate) fn bounded_to(value: &impl Debug, max: usize) -> String {
    struct Bounded {
        out: String,
        max: usize,
    }
    impl Write for Bounded {
        fn write_str(&mut self, s: &str) -> fmt::Result {
            let room = self.max - self.out.len();
            if s.len() <= room {
                self.out.push_str(s);
                return Ok(());
            }
            let mut end = room;
            while !s.is_char_boundary(end) {
                end -= 1;
            }
            self.out.push_str(&s[..end]);
            Err(fmt::Error)
        }
    }
    let mut w = Bounded {
        out: String::new(),
        max,
    };
    if write!(w, "{value:?}").is_err() {
        w.out.push('…');
    }
    w.out
}

#[cfg(test)]
mod tests {
    use ss13_shared::protocol::ClientIntent;
    use ss13_shared::{Dir, TilePos};

    use super::*;
    use crate::replication::Motion;

    #[test]
    fn kinds_and_bounded_summaries() {
        let packet = NetPacket::sent(
            &ClientMessage::Intent(ClientIntent::Move {
                dir: Dir::North,
                seq: 7,
            }),
            12,
        );
        assert_eq!(packet.kind, "Intent::Move");
        assert_eq!(packet.summary, "Move { dir: North, seq: 7 }");
        assert!(packet.outgoing);

        let long = ClientMessage::Intent(ClientIntent::Say("я".repeat(1000)));
        let packet = NetPacket::sent(&long, 2000);
        assert_eq!(packet.kind, "Intent::Say");
        assert!(packet.summary.len() <= SUMMARY_MAX + '…'.len_utf8());
        assert!(packet.summary.ends_with('…'));

        let batch = MotionBatch {
            tick: 3,
            moves: vec![Motion {
                id: ss13_shared::protocol::NetId(1),
                pos: TilePos::new(1, 2, 1),
                facing: None,
            }],
        };
        assert_eq!(NetPacket::motion(&batch, 20).summary, "tick 3, 1 moves");
    }
}
