//! Chat on ss13-rs, as tg does it: the server validates `say`, picks hearers by range and builds
//! styled messages; the client only shows them (tgui-panel) and sends what the player typed.
//!
//! Features: `server` ([`server`]), `client` ([`client`]). Wire types are in `ss13_shared::chat`.

#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "server")]
pub mod emote;
pub mod radio;
#[cfg(feature = "server")]
pub mod server;
#[cfg(feature = "server")]
pub mod speech;

pub use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, MAX_MESSAGE_LEN, Span};
