//! Wire format shared by client and server, and the framing of the reliable stream.
//!
//! One WebTransport session per client. The client opens one bidirectional stream; both
//! directions carry length-prefixed postcard frames (`u32` little-endian length, then the
//! payload), in order:
//! - client → server: [`Hello`], then [`ClientMessage`]s;
//! - server → client: [`HelloReply`], then (if accepted) [`ServerMessage`]s.
//!
//! Every reliable message shares this one stream, so messages arrive exactly once and in the
//! order the server wrote them, across kinds: a UI message or chat line is never overtaken by an
//! older replication batch. The cost is head-of-line blocking across kinds on a lost packet (one
//! RTT); replication, the only bulky kind, needs in-order delivery anyway.
//! Motion goes in QUIC datagrams ([`MotionBatch`], one decodable batch per datagram).
//!
//! Limits are checked from the length prefix before anything is allocated; a frame over the
//! limit or one that does not decode closes the session.

// Each side uses its half of the wire format.
#![cfg_attr(not(all(feature = "client", feature = "server")), allow(dead_code))]

#[cfg(not(target_arch = "wasm32"))]
use core::time::Duration;
use std::sync::Arc;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use ss13_shared::admin::{AdminRequest, AdminResponse};
use ss13_shared::chat::ChatMessage;
use ss13_shared::debug::{DebugRequest, DebugResponse};
use ss13_shared::protocol::{ClientIntent, MoveAck};
pub use ss13_shared::protocol::{ServerNotice, TICK_DURATION};
use ss13_shared::ui::{UiClientMessage, UiServerMessage};
#[cfg(not(target_arch = "wasm32"))]
use wtransport::{RecvStream, VarInt};

use crate::replication::{Motion, MotionBatch, ReplicationBatch};

pub const DEFAULT_PORT: u16 = 7777;

/// [`Hello`] and [`HelloReply`]: a token and a rejection reason.
pub(crate) const MAX_HELLO_FRAME: usize = 4 * 1024;
/// Client → server: an intent or UI message. `Say` is at most 4 KiB of UTF-8; 64 KiB leaves room
/// for UI acts carrying text (paper, PDA) without letting a client make the server buffer much.
pub(crate) const MAX_CLIENT_FRAME: usize = 64 * 1024;
/// Server → client: a replication batch of a whole view (a z-level jump on a dense station is
/// tens of KiB) or a tgui window's static data. A replication batch cannot be split (the
/// appearance cache evicts at batch end), so this is generous; the client trusts the server only
/// this far.
pub(crate) const MAX_SERVER_FRAME: usize = 4 * 1024 * 1024;

/// QUIC idle timeout on both ends; keep-alives keep a quiet but live session open.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) const IDLE_TIMEOUT: Duration = Duration::from_secs(10);
#[cfg(not(target_arch = "wasm32"))]
pub(crate) const KEEP_ALIVE: Duration = Duration::from_secs(3);

/// Application close codes (QUIC `CONNECTION_CLOSE`); the reason bytes are human-readable.
pub(crate) mod close {
    pub const NORMAL: u32 = 0;
    /// Frame over the size limit or not decodable.
    pub const PROTOCOL: u32 = 1;
    pub const REJECTED: u32 = 2;
    pub const HANDSHAKE_TIMEOUT: u32 = 3;
    /// The client does not read what the server sends.
    pub const SLOW_READER: u32 = 4;
    pub const SHUTDOWN: u32 = 5;
    /// An admin kicked or banned the client: final, the client does not reconnect.
    pub const KICKED: u32 = 6;
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn code(code: u32) -> VarInt {
    VarInt::from_u32(code)
}

/// Client → server, first frame. Its layout and that of [`HelloReply`] must never change, so that
/// a client of any version reads why it was rejected.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub(crate) struct Hello {
    pub version: u32,
    pub token: String,
}

/// Server → client answer to [`Hello`].
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub(crate) enum HelloReply {
    Accepted,
    Rejected(String),
}

/// Client → server after the handshake.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub(crate) enum ClientMessage {
    Intent(ClientIntent),
    Ui(UiClientMessage),
    Debug(DebugRequest),
    Admin(AdminRequest),
}

/// Server → client after the handshake.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub(crate) enum ServerMessage {
    Notice(ServerNotice),
    Ui(UiServerMessage),
    Chat(ChatMessage),
    MoveAck(MoveAck),
    Replication(ReplicationBatch),
    Debug(DebugResponse),
    Admin(AdminResponse),
    Camera(ss13_shared::protocol::CameraEffect),
    Sound(ss13_shared::protocol::SoundEvent),
    Animation(ss13_shared::animation::AnimationEvent),
}

/// An encoded frame, length prefix included; shared when one message goes to many clients.
pub(crate) type Frame = Arc<[u8]>;

#[derive(Debug)]
pub(crate) enum FrameError {
    TooLarge(usize),
    Malformed(postcard::Error),
    /// An in-memory frame whose prefix does not match its payload.
    Length {
        prefix: usize,
        payload: usize,
    },
    /// The stream or connection ended; the string says how.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    Closed(String),
}

impl core::fmt::Display for FrameError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::TooLarge(len) => write!(f, "frame of {len} bytes is over the limit"),
            Self::Malformed(e) => write!(f, "malformed frame: {e}"),
            Self::Length { prefix, payload } => {
                write!(f, "frame prefix says {prefix} bytes, payload has {payload}")
            }
            Self::Closed(how) => f.write_str(how),
        }
    }
}

pub(crate) fn encode_frame<T: Serialize>(message: &T, max: usize) -> Result<Frame, FrameError> {
    let mut buf = postcard::to_extend(message, vec![0; 4]).map_err(FrameError::Malformed)?;
    let len = buf.len() - 4;
    if len > max {
        return Err(FrameError::TooLarge(len));
    }
    buf[..4].copy_from_slice(&(len as u32).to_le_bytes());
    Ok(buf.into())
}

/// Decodes one whole frame (prefix included) of at most `max` payload bytes: the in-memory
/// transport's counterpart of [`read_frame`], with the same limit.
pub(crate) fn decode_frame<T: DeserializeOwned>(frame: &[u8], max: usize) -> Result<T, FrameError> {
    let Some((prefix, payload)) = frame.split_first_chunk::<4>() else {
        return Err(FrameError::Length {
            prefix: 4,
            payload: frame.len(),
        });
    };
    let len = u32::from_le_bytes(*prefix) as usize;
    if len > max {
        return Err(FrameError::TooLarge(len));
    }
    if len != payload.len() {
        return Err(FrameError::Length {
            prefix: len,
            payload: payload.len(),
        });
    }
    postcard::from_bytes(payload).map_err(FrameError::Malformed)
}

/// Reads one frame of at most `max` payload bytes, using `buf` as scratch between calls.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn read_frame<T: DeserializeOwned>(
    recv: &mut RecvStream,
    max: usize,
    buf: &mut Vec<u8>,
) -> Result<T, FrameError> {
    let closed = |e: wtransport::error::StreamReadExactError| FrameError::Closed(e.to_string());
    let mut prefix = [0; 4];
    recv.read_exact(&mut prefix).await.map_err(closed)?;
    let len = u32::from_le_bytes(prefix) as usize;
    if len > max {
        return Err(FrameError::TooLarge(len));
    }
    buf.clear();
    buf.resize(len, 0);
    recv.read_exact(buf).await.map_err(closed)?;
    postcard::from_bytes(buf).map_err(FrameError::Malformed)
}

/// Splits `moves` into datagrams of at most `max` bytes, each a complete [`MotionBatch`]: postcard
/// encodes a `Vec` as its length then its elements, so a batch is `(tick, count)` followed by the
/// concatenated motions. One motion is far below any QUIC datagram limit.
pub(crate) fn pack_motion(tick: u32, moves: &[Motion], max: usize) -> Vec<Vec<u8>> {
    // Largest `(u32, usize)` header: 5 + 10 varint bytes.
    const HEADER: usize = 15;
    let mut datagrams = Vec::new();
    let mut body = Vec::new();
    let mut count = 0usize;
    let flush = |body: &[u8], count: usize, out: &mut Vec<Vec<u8>>| {
        let mut datagram = postcard::to_stdvec(&(tick, count)).expect("encodes");
        datagram.extend_from_slice(body);
        out.push(datagram);
    };
    for motion in moves {
        let start = body.len();
        body = postcard::to_extend(motion, body).expect("encodes");
        if count > 0 && HEADER + body.len() > max {
            flush(&body[..start], count, &mut datagrams);
            body.drain(..start);
            count = 0;
        }
        count += 1;
    }
    if count > 0 {
        flush(&body, count, &mut datagrams);
    }
    datagrams
}

pub(crate) fn decode_motion(datagram: &[u8]) -> Option<MotionBatch> {
    postcard::from_bytes(datagram).ok()
}

#[cfg(not(target_arch = "wasm32"))]
/// The process-wide async runtime of the transport: QUIC I/O never runs on Bevy threads, and
/// Bevy systems only touch bounded channels and non-blocking calls.
pub(crate) fn runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();
    RUNTIME.get_or_init(|| {
        let threads = std::thread::available_parallelism().map_or(2, |n| n.get().clamp(2, 8));
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(threads)
            .thread_name("ss13-net")
            .enable_all()
            .build()
            .expect("tokio runtime")
    })
}

#[cfg(not(target_arch = "wasm32"))]
/// A connection close as a reason fit to show: the peer's own reason when it gave one.
pub(crate) fn close_reason(error: &wtransport::error::ConnectionError) -> String {
    match error {
        wtransport::error::ConnectionError::ApplicationClosed(close)
            if !close.reason().is_empty() =>
        {
            String::from_utf8_lossy(close.reason()).into_owned()
        }
        other => other.to_string(),
    }
}

#[cfg(not(target_arch = "wasm32"))]
/// The connection's close reason when the stream ended because the connection did, else `how`.
pub(crate) async fn lost_reason(conn: &wtransport::Connection, how: String) -> String {
    match tokio::time::timeout(Duration::from_millis(100), conn.closed()).await {
        Ok(error) => close_reason(&error),
        Err(_) => how,
    }
}

#[cfg(test)]
mod tests {
    use ss13_shared::protocol::NetId;
    use ss13_shared::{Dir, TilePos};

    use super::*;

    #[test]
    fn motion_is_split_into_decodable_datagrams_under_the_limit() {
        let moves: Vec<Motion> = (0..500u64)
            .map(|i| Motion {
                id: NetId(i * 1000),
                pos: TilePos::new(i as i32, 255 - i as i32, 1),
                facing: (i % 2 == 0).then_some(Dir::North),
            })
            .collect();
        let datagrams = pack_motion(123_456, &moves, 1200);
        assert!(datagrams.len() > 1);
        let mut decoded = Vec::new();
        for d in &datagrams {
            assert!(d.len() <= 1200, "{}", d.len());
            let batch = decode_motion(d).unwrap();
            assert_eq!(batch.tick, 123_456);
            decoded.extend(batch.moves);
        }
        assert_eq!(decoded, moves);
        assert!(pack_motion(1, &[], 1200).is_empty());
    }

    #[test]
    fn oversized_frames_are_refused_on_encode() {
        let notice = ServerNotice {
            text: "x".repeat(100),
        };
        assert!(matches!(
            encode_frame(&notice, 50),
            Err(FrameError::TooLarge(_))
        ));
        let frame = encode_frame(&notice, 1000).unwrap();
        assert_eq!(
            u32::from_le_bytes(frame[..4].try_into().unwrap()) as usize,
            frame.len() - 4
        );
    }
}
