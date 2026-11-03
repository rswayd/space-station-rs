//! Debug and test tooling: an egui overlay in the client (`client` feature) showing what is on
//! screen and what goes over the network, and the server half (`server` feature) that answers
//! its debug queries and applies its dev tools behind a gate. See `docs/RUNNING.md`, "Debug UI".

pub mod netlog;
pub mod profile;
// Prometheus metrics of a server (`SS13_METRICS_ADDR`); not in the browser server.
#[cfg(all(feature = "server", not(target_arch = "wasm32")))]
pub mod metrics;

#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "server")]
pub mod server;
#[cfg(feature = "server")]
pub mod test_hooks;
