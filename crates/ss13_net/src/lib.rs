//! Networking: maps `ss13_shared::protocol` onto WebTransport (QUIC, `wtransport`). Swapping the
//! network library touches only this crate.
//!
//! After the WebTransport session opens, the client sends a handshake with
//! [`ss13_shared::protocol::PROTOCOL_VERSION`] and an auth token, and the server answers accept or
//! reject-with-reason before any intent is delivered to game code. Framing, limits and channel
//! semantics: `protocol.rs`; admission and backpressure: `server.rs`.
//!
//! World state reaches clients through our own interest-managed replication (`replication*`
//! modules).

pub mod admin;
pub mod camera;
pub mod chat;
pub mod debug;
#[cfg(feature = "server")]
mod light_server;
#[cfg(feature = "server")]
mod line_of_sight;
pub mod move_ack;
mod protocol;
mod replication;
#[cfg(feature = "client")]
mod replication_client;
#[cfg(feature = "server")]
mod replication_server;
mod sim_latency;
pub mod sound;
#[cfg(feature = "client")]
pub mod tap;
pub mod ui;
#[cfg(feature = "server")]
mod view_mask;

#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "server")]
pub mod server;

#[cfg(all(test, feature = "client", feature = "server"))]
mod replication_tests;
#[cfg(all(test, feature = "client", feature = "server"))]
mod tests;
#[cfg(all(test, feature = "client", feature = "server"))]
mod transport_tests;

pub use protocol::{DEFAULT_PORT, ServerNotice, TICK_DURATION};
pub use sim_latency::SimLatency;
