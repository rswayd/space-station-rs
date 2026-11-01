//! Debug queries and dev tools (`ss13_debug`): the client's debug overlay asks, the server answers
//! when its debug gate is open (debug builds, or `SS13_DEBUG_QUERIES=1`); dev tools (`Teleport`
//! and below) additionally need a debug build of the server. Everything here is untrusted on the
//! server and refused or rate-limited there.

use serde::{Deserialize, Serialize};

use crate::TilePos;
use crate::protocol::NetId;

/// Prototype search results per [`DebugRequest::FindPrototypes`].
pub const MAX_PROTOTYPE_MATCHES: usize = 50;

/// Client → server.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum DebugRequest {
    /// Turf, area, air, power and entities of a tile.
    Tile(TilePos),
    ServerStats,
    /// Prototype ids (DM paths) containing the text, at most [`MAX_PROTOTYPE_MATCHES`].
    FindPrototypes(String),
    /// Dev: move the own mob to the tile.
    Teleport(TilePos),
    /// Dev: a new object of the prototype on the tile (tg `new path(loc)`).
    Spawn {
        path: String,
        pos: TilePos,
    },
    /// Dev: the own mob takes no damage (tg `GODMODE`).
    GodMode(bool),
    /// Dev: tg `SSticker.force_ending`.
    EndRound,
    /// Dev: speed of the server's virtual time (1.0 = normal).
    TimeScale(f32),
    /// Dev, functional tests (`ss13_bots`): a JSON test hook command, answered by
    /// [`DebugResponse::Test`] with the same `id`. The vocabulary lives with the hooks
    /// (`ss13_debug::test_hooks`), so new test queries need no protocol change.
    Test {
        id: u32,
        command: String,
    },
}

/// Server → client.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum DebugResponse {
    Tile(TileReport),
    ServerStats(ServerStats),
    Prototypes {
        query: String,
        ids: Vec<String>,
    },
    /// A dev request was applied; what happened.
    Done(String),
    /// Why a request was not answered or applied.
    Refused(String),
    /// Answer to [`DebugRequest::Test`]: JSON on success, the reason otherwise.
    Test {
        id: u32,
        result: Result<String, String>,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TileReport {
    pub pos: TilePos,
    /// Name of the tile's turf, empty when there is none.
    pub turf: String,
    pub area: Option<AreaReport>,
    /// `None` without simulated air on the tile (walls, space without atmos, no atmos at all).
    pub air: Option<AirReport>,
    pub entities: Vec<EntityReport>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AreaReport {
    pub name: String,
    pub requires_power: bool,
    /// tg `power_equip`, `power_light`, `power_environ` as `powered(chan)` answers them.
    pub powered: [bool; 3],
    /// Static plus this tick's dynamic load per channel, watts.
    pub load: [f64; 3],
    pub has_apc: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AirReport {
    pub pressure_kpa: f64,
    pub temperature_k: f64,
    pub total_moles: f64,
    /// Gases present: tg id (`o2`), moles, partial pressure in kPa.
    pub gases: Vec<(String, f64, f64)>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct EntityReport {
    /// The asking client's id of it, when replicated to that client.
    pub net_id: Option<NetId>,
    pub name: String,
    /// DM `icon` and `icon_state` it was built from.
    pub icon: String,
    pub icon_state: String,
    /// Server component type names.
    pub components: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ServerStats {
    /// Work time of one server tick (`First` to `Last`) over the recent ticks, milliseconds.
    pub tick_p50_ms: f32,
    pub tick_p99_ms: f32,
    pub tick_max_ms: f32,
    pub entities: u32,
    pub time_scale: f32,
    /// tg ticker state.
    pub round_state: String,
    pub clients: Vec<ClientStats>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ClientStats {
    /// Handshake token (a dev identity until there is auth).
    pub name: String,
    pub bytes_per_sec: f64,
    pub packets_per_sec: f64,
    /// Entities replicated to it.
    pub visible: u32,
}
