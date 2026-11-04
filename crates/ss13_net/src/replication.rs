//! Entity replication with per-client interest: wire format shared by client and server.
//!
//! Not built on a generic entity replication such as bevy_replicon: replicon keeps per-client
//! visibility as a "hidden" map that defaults to visible and walks every entity of a changed
//! archetype for every client each tick (`bevy_replicon::server::collect_changes`,
//! `needs_replication`), so a station of 200k entities with 100 clients costs
//! ~20M lookups per tick and 20M map entries. Here interest is per chunk: a client only ever
//! costs work for the entities in its view box.
//!
//! Delivery:
//! - [`ReplicationBatch`] on the reliable stream (ordered, exactly once): spawns with full state (appearance by interned
//!   [`AppearanceId`]), despawns, appearance changes, control changes, and "settle" snapshots of
//!   entities that stopped moving.
//! - [`MotionBatch`] in QUIC datagrams (unordered, unreliable): positions and facings of moving entities, every
//!   tick they change. A lost motion is repaired by the next motion of the entity or, when it
//!   stops, by the reliable settle sent [`SETTLE_TICKS`] after its last change.
//!
//! The client's appearance cache mirrors the server's bookkeeping of it, and [`NetId`]s are per
//! client and reused (after a delay), so the reliable stream must deliver every batch exactly
//! once and in order; any transport change has to keep that.
//!
//! Every batch carries the server replication tick; the client applies an entity's state only
//! if it is newer than the state it has, so a late motion never overwrites a newer spawn or
//! settle.

// Each side uses its half of the wire format.
#![cfg_attr(not(all(feature = "client", feature = "server")), allow(dead_code))]

use serde::{Deserialize, Serialize};
use ss13_shared::protocol::NetId;
use ss13_shared::{Appearance, Dir, LightSource, Optics, TilePos};

/// Ticks without change after which a moved entity's final state is resent reliably.
pub(crate) const SETTLE_TICKS: u32 = 4;

/// Server-assigned id of a distinct [`Appearance`] value, as BYOND appearances: most station
/// objects share a few thousand appearances, so spawns carry this id and a client receives each
/// value once, in [`ReplicationBatch::appearances`], while it has entities that use it.
///
/// Per-client cache rule, applied identically by both sides to the same op stream: an id is
/// cached from its definition until a batch ends with no entity of the client using it, then it
/// is evicted and is defined again before its next use.
// shortcut: never reused, so it wraps after 2^32 distinct interned values (~50 days at
// 1000 new appearances/s); reuse needs every client's cache to have evicted the id first.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct AppearanceId(pub u32);

/// The value an [`AppearanceId`] stands for: what an entity looks like and how its tile is lit.
/// Interned together because they go together: floors and space tiles of one area share both.
/// `optics` carries only [`Optics::tile`]: light sources and opacity go per light block
/// ([`ReplicationOp::LightBlock`]), so a light switching or a door opening does not touch the
/// entity's appearance.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub(crate) struct Look {
    pub appearance: Appearance,
    pub optics: Option<Optics>,
}

/// What an entity does to walking through its tile, for the client's movement prediction
/// (`ss13_world::Density`, `Border`, `ClimbWalkable`). Movers (mobs) are never sent: they are
/// pushed, swapped and walk away, which the client cannot foresee.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Blocks {
    Dense,
    /// A table: dense, but walked across from another one.
    Climbable,
    /// A dense border object: the tile edges it blocks (BYOND dir bits).
    Edges(u8),
    /// The last stair of a flight going up toward this dir (BYOND dir bits): its far edge
    /// blocks, and a step off it there climbs (`ss13_world::ClimbsUp`).
    Stairs(u8),
    /// Open space a walker falls through (`ss13_world::FallsThrough`), on its turf.
    Hole,
}

/// One entity of a [`ReplicationOp::Spawn`] tile group.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub(crate) struct Spawned {
    pub id: NetId,
    pub facing: Option<Dir>,
    pub appearance: Option<AppearanceId>,
    pub blocks: Option<Blocks>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub(crate) enum ReplicationOp {
    /// Entities that entered the client's view, grouped by tile: a strip of a moved view is
    /// tile after tile of static objects, so the position is sent once per tile.
    Spawn {
        pos: TilePos,
        entities: Vec<Spawned>,
    },
    Despawn(Vec<NetId>),
    Appearance(NetId, Option<AppearanceId>),
    /// A known entity started or stopped blocking (a door closed or opened).
    Blocks(NetId, Option<Blocks>),
    /// Current position and facing of an entity that stopped changing.
    Settle {
        id: NetId,
        pos: TilePos,
        facing: Option<Dir>,
    },
    /// The entity this client controls; always sent after the entity's `Spawn`.
    Control(Option<NetId>),
    /// A light block entered the client's light interest: its whole state.
    LightBlock {
        origin: TilePos,
        opaque: [u8; 32],
        lights: BlockLights,
    },
    /// New opacity of a known light block (layout of `ss13_shared::LightUpdate::Opacity`).
    Opacity {
        origin: TilePos,
        opaque: [u8; 32],
    },
    /// New light sources of some tiles of a known light block.
    Lights {
        origin: TilePos,
        lights: BlockLights,
    },
    /// Light blocks that left the client's light interest.
    ForgetLight(Vec<TilePos>),
    /// Whether the client draws only what its eye sees (BYOND `view()`) and within which
    /// Chebyshev radius; it computes that itself from its predicted position and the light
    /// blocks' opacity, so its view never lags its own step.
    LineOfSight(Option<u32>),
    /// The tiles of a known light block a game view mask (camera static) shows (bit layout of
    /// `Opacity`); after the block's `LightBlock` and when it changes.
    Seen {
        origin: TilePos,
        seen: [u8; 32],
    },
    /// Which levels stand over which (`ss13_shared::LevelStack::links`): sent once, before the
    /// first spawn on a level below the viewer's.
    Stack(Vec<(u16, u16)>),
}

/// Light sources of some tiles of one light block. Most lights of a block share a few values
/// (a room's tubes, starlight on space, an area's emergency lights), so each value goes once.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub(crate) struct BlockLights {
    pub palette: Vec<LightSource>,
    /// Tile index in the block (`ss13_shared::light_block`) and the palette indexes of all its
    /// lights; a listed tile's lights replace the known ones, an empty list clears them.
    pub tiles: Vec<(u8, Vec<u16>)>,
}

impl ReplicationOp {
    pub(crate) fn is_light(&self) -> bool {
        matches!(
            self,
            Self::LightBlock { .. }
                | Self::Opacity { .. }
                | Self::Lights { .. }
                | Self::ForgetLight(_)
                | Self::Seen { .. }
                | Self::LineOfSight(_)
        )
    }
}

/// Everything reliable for one client in one server tick.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub(crate) struct ReplicationBatch {
    pub tick: u32,
    /// Applied before `ops`: every id an op of this batch uses is cached by then.
    pub appearances: Vec<(AppearanceId, Look)>,
    pub ops: Vec<ReplicationOp>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub(crate) struct Motion {
    pub id: NetId,
    pub pos: TilePos,
    pub facing: Option<Dir>,
}

/// Motion of entities the client already has, for one server tick.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub(crate) struct MotionBatch {
    pub tick: u32,
    pub moves: Vec<Motion>,
}
