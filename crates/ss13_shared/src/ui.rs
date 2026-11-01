//! tgui wire contract: server-owned windows whose state the client only renders.
//!
//! Window data travels as opaque bytes: each interface (tgui `interface` name, e.g.
//! `"ThermoMachine"`) has its own typed `ui_data`/`ui_static_data` structs that both sides share
//! outside this crate. Acts go client → server as [`ClientIntent::UiAct`](crate::protocol::ClientIntent::UiAct).

use serde::{Deserialize, Serialize};

/// tgui `UI_CLOSE`/`UI_DISABLED`/`UI_UPDATE`/`UI_INTERACTIVE`, ordered the same way, so the
/// combination of several checks is `min`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum UiStatus {
    /// The window must close.
    Close,
    /// Visible, neither updated nor interactive.
    Disabled,
    /// Updated, not interactive.
    Update,
    Interactive,
}

/// Server → client, reliable and ordered.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum UiServerMessage {
    /// Open a window (tgui `ui.open()` + the first `update`).
    Open {
        window: u32,
        interface: String,
        title: String,
        status: UiStatus,
        static_data: Vec<u8>,
        /// Empty when `status` is below [`UiStatus::Update`].
        data: Vec<u8>,
    },
    /// tgui `update`: new status, and new data unless the status forbids updates or the data
    /// did not change.
    Update {
        window: u32,
        status: UiStatus,
        data: Option<Vec<u8>>,
    },
    /// The server closed the window (distance, state loss, source gone).
    Close { window: u32 },
}

/// Client → server, reliable and ordered. Acts are [`ClientIntent::UiAct`](crate::protocol::ClientIntent::UiAct).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum UiClientMessage {
    /// The player closed the window (tgui `close`).
    Close { window: u32 },
}
