//! Debug and test tooling: an egui overlay in the client (`client` feature) showing what is on
//! screen and what goes over the network, and the server half (`server` feature) that answers
//! its debug queries and applies its dev tools behind a gate. See `docs/RUNNING.md`, "Debug UI".

pub mod netlog;
pub mod profile;

#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "server")]
pub mod server;
#[cfg(feature = "server")]
pub mod test_hooks;
