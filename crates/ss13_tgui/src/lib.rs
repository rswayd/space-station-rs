//! tgui on ss13-rs: server-owned windows, client-rendered.
//!
//! Like BYOND tgui, all logic is on the server. A window binds one client (and its acting body)
//! to a source entity; the server produces typed `ui_data`/`ui_static_data`, re-checks `ui_status`
//! every tick, sends updates on open, on change and on a timer, and validates every act. The
//! client only renders the data through a builder registered under the tgui interface name and
//! turns button presses into [`ClientIntent::UiAct`](ss13_shared::protocol::ClientIntent::UiAct).
//!
//! Features: `server` ([`server`]), `client` ([`client`]). Interfaces ([`interfaces`]) carry the
//! data types both halves share, plus each half's implementation behind the same features.

use serde::Serialize;
use serde::de::DeserializeOwned;

pub use ss13_shared::ui::UiStatus;

#[cfg(feature = "client")]
pub mod client;
pub mod interfaces;
#[cfg(feature = "server")]
pub mod prompt;
#[cfg(feature = "client")]
pub mod samples;
#[cfg(feature = "server")]
pub mod server;

/// A tgui interface: the name and data shapes both sides agree on.
pub trait UiInterface: Send + Sync + 'static {
    /// tgui interface name (`new(user, src, "ThermoMachine")`); the client registry key.
    const NAME: &'static str;
    /// `ui_data`: re-sent whenever it changes.
    type Data: Serialize + DeserializeOwned + Send + Sync + 'static;
    /// `ui_static_data`: sent once on open. `()` costs zero bytes.
    type StaticData: Serialize + DeserializeOwned + Send + Sync + 'static;
}

/// Wire encoding of window data. Full state, not a diff: see `server` docs for the measurement.
pub fn encode<T: Serialize>(value: &T) -> Vec<u8> {
    postcard::to_stdvec(value).expect("ui data types serialize infallibly")
}

pub fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, postcard::Error> {
    postcard::from_bytes(bytes)
}
