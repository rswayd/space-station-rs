//! Server side of replication: which client sees which entity, and what to send it.
//!
//! Game code marks entities with [`Networked`] and gives every connected client a [`Viewer`]
//! (inserted on the client's connection entity from [`ClientConnected`](crate::server::ClientConnected)).
//! A client gets the [`Networked`] entities whose `TilePos` enters the Chebyshev box of its viewer
//! (same z-level, existing level) and keeps them until they are [`KEEP_MARGIN`] tiles outside it.
//! `TilePos`, `Facing`, `Appearance` and `Optics::tile` of those entities are replicated; a client never
//! learns a server `Entity`, only a [`NetId`] of its own ([`Interest::entity`] maps it back).
//! Light sources and opacity go per light block from a wider box (`light_server`).
//! A client with a [`ViewMask`] gets only the entities on tiles the mask shows (`view_mask`).
//! Every other client gets only what its eye sees (BYOND `view()`, `line_of_sight`): a mask of
//! its own, recomputed when the viewer moves or opacity changes near it, unless its [`Sight`]
//! sees through walls (observers), or sees turfs or mobs through them ([`SeenAsTurf`], [`SeenAsMob`]).
//! [`Invisibility`], [`VisibleToGroups`] and [`VisibleToMobs`] decide per client whether an
//! entity exists for it at all; [`AltAppearances`] which appearance it gets (tg alternate
//! appearances and override images), so the alternate value goes only to its seers.
//!
//! Cost per tick is proportional to what changed, never to the world size:
//! - a changed entity touches only the clients registered on its old and new chunk;
//! - a moved viewer walks only the tile strips that entered or left its box;
//! - a changed mask chunk walks only its flipped tiles, for the clients using that mask;
//! - line of sight is recomputed only for a viewer that moved or saw opacity flip within reach;
//! - change detection over `Networked` entities is the only per-entity pass (no data touched
//!   for unchanged entities);
//! - an appearance is hashed once when an entity is first replicated or changes it, never per
//!   client: clients get interned [`AppearanceId`]s (an alternate appearance is one more
//!   interned value; choosing it is a scan of the entity's few entries per client sent to).

use std::collections::VecDeque;
use std::hash::{BuildHasher, Hash, Hasher};
use std::time::Duration;

use bevy::platform::collections::{HashMap, HashSet, HashTable};
use bevy::platform::hash::FixedHasher;
use bevy::platform::time::Instant;
use bevy::prelude::*;
use ss13_shared::protocol::NetId;
use ss13_shared::{Appearance, Facing, LevelStack, Optics, TilePos, light_block};
use ss13_world::{
    Border, ChunkPos, ClimbWalkable, ClimbsUp, Density, FallsThrough, Mover, NonDenseBorder,
    Pushable, SeenAsMob, SeenAsTurf, SpatialIndex,
};

use crate::light_server::{LIGHT_MARGIN, LightGrid, block_overlaps, blocks_in};
use crate::protocol::ServerMessage;
use crate::replication::{
    AppearanceId, Blocks, Look, Motion, ReplicationBatch, ReplicationOp, SETTLE_TICKS, Spawned,
};
use crate::server::{ClientLink, NetServerSystems};
use crate::view_mask::{MASK_CHUNK, MaskId, ViewMask, ViewMasks, block_seen};
use ss13_shared::line_of_sight::ViewGrid;

/// BYOND tg widescreen view is 19x15 (half-width 9); one tile of margin so that entities
/// already exist when they scroll into view.
pub const DEFAULT_VIEW_RADIUS: u32 = 10;

/// Hysteresis: an entity leaves a client only once it is this many tiles outside the view box,
/// so a viewer stepping back and forth does not respawn the strip it just left.
pub const KEEP_MARGIN: u32 = 2;

/// A client's freed [`NetId`] is reused only after this many ticks, so a click still in flight
/// for a despawned entity resolves to nothing instead of to a new entity.
const NET_ID_REUSE_TICKS: u32 = 200;

/// How often [`ReplicationStats`] is logged.
const STATS_LOG_INTERVAL_SECS: f64 = 10.0;

/// Replicate this entity's `TilePos`, `Facing`, `Appearance` and `Optics::tile` (interned with
/// the appearance; dropped on entities without one) to the clients that see it. Its `Optics`
/// light source and opacity go to the clients whose light interest covers its tile.
/// A marker without data and without hooks, so map load pays nothing per entity for it.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Networked;

/// BYOND `invisibility` of a [`Networked`] entity: only clients whose [`SeeInvisible`] is at least
/// this see it (tg observers: `INVISIBILITY_OBSERVER` 60). Absent = 0, visible to everyone.
// shortcut: read when the entity is first replicated to a client; changing it on an entity that
// clients already know does not despawn or spawn it. Handle changes when something becomes
// visible or invisible at runtime (cloaks, revived ghosts).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Invisibility(pub u8);

/// BYOND `see_invisible` of a client: on the connection entity next to its [`Viewer`]. Absent = 0
/// (tg `SEE_INVISIBLE_LIVING` is 25, above nothing a living mob can see on the map). A change
/// re-evaluates every entity in the client's view.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SeeInvisible(pub u8);

/// tg alternate appearances shown to an antagonist's minds
/// (`/datum/atom_hud/alternate_appearance/basic/has_antagonist`, e.g. heretic influences): a
/// [`Networked`] entity with groups is also seen by the clients whose [`SeeGroups`] share one,
/// whatever its [`Invisibility`]. Same first-replication shortcut as [`Invisibility`].
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VisibleToGroups(pub u32);

/// The [`VisibleToGroups`] bits a client sees: on the connection entity next to its [`Viewer`].
/// A change re-evaluates every entity in the client's view.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SeeGroups(pub u32);

/// tg `client.images` of one mob (a hallucination's images): a [`Networked`] entity also seen
/// by the clients whose [`ControlledEntity`] is one of these mobs, whatever its [`Invisibility`].
/// It follows the mob, not the connection, as tg images follow `hallucinator.client` of the
/// mob. Same first-replication shortcut as [`Invisibility`]; a control change re-evaluates the
/// client's whole view.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct VisibleToMobs(pub Vec<Entity>);

/// What decides whether a client may see an entity at all.
type Hidden<'a> = (
    Option<&'a Invisibility>,
    Option<&'a VisibleToGroups>,
    Option<&'a VisibleToMobs>,
);

/// Who sees an [`AltAppearance`]: the clients controlling one of `mobs` ([`ControlledEntity`])
/// and those whose [`SeeGroups`] share a bit with `groups`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Seers {
    pub mobs: Vec<Entity>,
    pub groups: u32,
}

impl Seers {
    /// Only the client controlling `mob` (tg `alternate_appearance/basic/one_person`).
    pub fn mob(mob: Entity) -> Self {
        Self {
            mobs: vec![mob],
            groups: 0,
        }
    }

    fn include(&self, control: Option<Entity>, groups: u32) -> bool {
        self.groups & groups != 0 || control.is_some_and(|c| self.mobs.contains(&c))
    }
}

/// One alternate appearance of an entity.
#[derive(Clone, Debug, PartialEq)]
pub struct AltAppearance {
    /// tg's `key`: what removes it again.
    pub key: String,
    pub seers: Seers,
    pub appearance: Appearance,
}

/// tg alternate appearances (`add_alt_appearance`, `/datum/atom_hud/alternate_appearance`) and
/// override images (`image.override` in `client.images`): a [`Networked`] entity looks like an
/// entry's `appearance` to that entry's seers instead of its own `Appearance`, the last matching
/// entry winning. Only the seers get the value: the others are sent the entity's own appearance
/// and never learn an alternate exists. Position, facing, blocking and the tile lighting
/// (`Optics::tile`) stay the entity's own for everyone.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct AltAppearances(pub Vec<AltAppearance>);

/// [`Sight`] classes of an entity: [`SeenAsTurf`], [`SeenAsMob`], neither (an object).
type SightClasses<'w, 's> = Query<'w, 's, (Has<SeenAsTurf>, Has<SeenAsMob>)>;

type Alts<'w, 's> = Query<'w, 's, &'static AltAppearances, With<Networked>>;

/// BYOND `sight` flags of a client: on the connection entity next to its [`Viewer`]. Absent = 0:
/// the client gets only the tiles its eye sees (BYOND `view()` through opacity).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Sight(pub u16);

impl Sight {
    pub const SEE_MOBS: u16 = 1 << 2;
    pub const SEE_OBJS: u16 = 1 << 3;
    pub const SEE_TURFS: u16 = 1 << 4;
    /// tg observers and x-ray: everything in the view box, walls or not.
    pub const XRAY: Self = Self(Self::SEE_MOBS | Self::SEE_OBJS | Self::SEE_TURFS);

    fn sees_all(self) -> bool {
        self.0 & Self::XRAY.0 == Self::XRAY.0
    }
}

/// Line of sight reaches this many tiles beyond the view box: the client's lightmap margin
/// past the screen (`ss13_render` lighting `MARGIN`), so a glide never shows a tile's seen bit
/// missing. Lights do not need more: light blocks go unmasked (`view_mask`).
const LOS_MARGIN: u32 = 2;

/// A viewer that stepped also sees from the next tile in its direction this long (ticks), so
/// what its client's predicted step reveals is there when the step shows, not a round trip
/// later. After it stops it sees from its own tile only.
const LOOKAHEAD_TICKS: u32 = 8;
#[cfg(test)]
pub(crate) const LOOKAHEAD_TICKS_FOR_TESTS: u32 = LOOKAHEAD_TICKS;

/// The mask of a client's line of sight. Game masks never name a client connection entity.
pub fn los_mask(client: Entity) -> MaskId {
    MaskId(client.to_bits())
}

/// Where a client looks from. Insert on the client's connection entity; the client gets
/// [`Networked`] entities within Chebyshev distance `radius` of `center` on `center.z`, and
/// keeps them until they are farther than `radius + KEEP_MARGIN`.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Viewer {
    pub center: TilePos,
    pub radius: u32,
}

impl Viewer {
    pub fn new(center: TilePos) -> Self {
        Self {
            center,
            radius: DEFAULT_VIEW_RADIUS,
        }
    }
}

/// The entity a client controls (its mob). Insert on the client's connection entity; the
/// client marks its copy of the entity with `LocalControl` once the entity is in its view.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ControlledEntity(pub Entity);

/// Per-client interest state, and the server `Entity` behind every client's [`NetId`]s.
#[derive(Resource, Default)]
pub struct Interest {
    tick: u32,
    clients: HashMap<Entity, ClientView>,
    /// Clients whose keep box overlaps a chunk.
    chunk_viewers: HashMap<ChunkPos, Vec<Entity>>,
    /// Entities replicated to at least one client since they became `Networked`.
    tracked: HashMap<Entity, Tracked>,
    appearances: Appearances,
    lights: LightGrid,
    /// Scratch of line-of-sight computations.
    grid: ViewGrid,
}

struct Tracked {
    /// Position as of the last replication pass: tells which chunk's viewers may know the entity.
    pos: TilePos,
    /// Interned appearance as of the last replication pass.
    appearance: Option<AppearanceId>,
    /// Interned [`AltAppearances`] with their seers, in order, as of the last replication pass.
    alts: Vec<(Seers, AppearanceId)>,
}

impl Tracked {
    /// The appearance of a client controlling `control` with `groups`: the last alternate
    /// appearance it is a seer of, else the entity's own.
    fn look_for(&self, control: Option<Entity>, groups: u32) -> Option<AppearanceId> {
        match self
            .alts
            .iter()
            .rev()
            .find(|(s, _)| s.include(control, groups))
        {
            Some(&(_, id)) => Some(id),
            None => self.appearance,
        }
    }
}

/// Interns `alts` with the entity's `optics` (the tile lighting stays the entity's own).
fn intern_alts(
    appearances: &mut Appearances,
    alts: Option<&AltAppearances>,
    optics: Option<&Optics>,
) -> Vec<(Seers, AppearanceId)> {
    let Some(alts) = alts else {
        return Vec::new();
    };
    (alts.0.iter())
        .map(|alt| {
            (
                alt.seers.clone(),
                appearances.acquire(&alt.appearance, optics),
            )
        })
        .collect()
}

fn release_alts(appearances: &mut Appearances, alts: Vec<(Seers, AppearanceId)>) {
    for (_, id) in alts {
        appearances.release(id);
    }
}

impl Interest {
    /// The entity `client` knows as `id`. `Some` only while the client has it replicated, so a
    /// resolved intent target is one the client sees.
    pub fn entity(&self, client: Entity, id: NetId) -> Option<Entity> {
        self.clients.get(&client)?.by_net.get(&id).copied()
    }

    /// The id `client` knows `entity` by, while the client has it replicated.
    pub fn net_id(&self, client: Entity, entity: Entity) -> Option<NetId> {
        self.clients.get(&client)?.known.get(&entity).map(|k| k.id)
    }

    /// Whether `client` currently has `entity` replicated, i.e. sees it.
    pub fn sees(&self, client: Entity, entity: Entity) -> bool {
        self.clients
            .get(&client)
            .is_some_and(|view| view.known.contains_key(&entity))
    }

    /// The appearance `client` has for `entity` (its own or an alternate one), while it has the
    /// entity replicated.
    pub fn appearance(&self, client: Entity, entity: Entity) -> Option<&Appearance> {
        let id = self.clients.get(&client)?.known.get(&entity)?.appearance?;
        Some(&self.appearances.entries.get(&id)?.look.appearance)
    }

    /// Fills `out` with the clients that have `entity` replicated and the id each knows it by.
    pub(crate) fn viewers_of(&self, entity: Entity, out: &mut Vec<(Entity, NetId)>) {
        out.clear();
        let Some(tracked) = self.tracked.get(&entity) else {
            return;
        };
        // A client that knows the entity keeps its chunk in its keep box.
        let Some(clients) = self.chunk_viewers.get(&ChunkPos::of(tracked.pos)) else {
            return;
        };
        out.extend(
            clients
                .iter()
                .filter_map(|&client| Some((client, self.net_id(client, entity)?))),
        );
    }

    /// Number of entities replicated to `client`.
    pub fn visible_count(&self, client: Entity) -> usize {
        self.clients.get(&client).map_or(0, |view| view.known.len())
    }

    /// Number of appearances `client` has cached.
    pub fn cached_appearances(&self, client: Entity) -> usize {
        self.clients
            .get(&client)
            .map_or(0, |view| view.cached.len())
    }

    /// Number of light blocks `client` subscribes to.
    pub fn light_blocks(&self, client: Entity) -> usize {
        self.clients
            .get(&client)
            .map_or(0, |view| view.light_blocks.len())
    }

    /// Distinct appearances of the replicated entities.
    pub fn interned_appearances(&self) -> usize {
        self.appearances.entries.len()
    }
}

/// Interned appearances of tracked entities, refcounted by the entities using them.
#[derive(Default)]
struct Appearances {
    next: u32,
    /// Ids by value hash; the values live in `entries`.
    by_value: HashTable<AppearanceId>,
    entries: HashMap<AppearanceId, Interned>,
}

struct Interned {
    look: Look,
    hash: u64,
    refs: u32,
}

impl Appearances {
    /// Interns `appearance` with the part of `optics` that entities carry (`Look`).
    fn acquire(&mut self, appearance: &Appearance, optics: Option<&Optics>) -> AppearanceId {
        let optics = optics.and_then(|o| o.tile).map(|tile| Optics {
            tile: Some(tile),
            ..Default::default()
        });
        let optics = optics.as_ref();
        let hash = hash_appearance(appearance, optics);
        let entries = &mut self.entries;
        if let Some(&id) = self.by_value.find(hash, |id| {
            let look = &entries[id].look;
            same_appearance(&look.appearance, appearance) && look.optics.as_ref() == optics
        }) {
            entries.get_mut(&id).expect("indexed id").refs += 1;
            return id;
        }
        // After a u32 wrap the counter meets ids still in use; overwriting one would leave two
        // hashes indexing it and panic on a later lookup. Live ids are < 2^32, so this ends.
        // shortcut: an id released earlier in the same pass may still be cached by a client until
        // its eviction at pass end; reuse after a wrap could then skip its resend. Needs 2^32
        // interned values first; upgrade with a per-pass quarantine of freed ids if that is reached.
        let id = loop {
            self.next = self.next.wrapping_add(1);
            let id = AppearanceId(self.next);
            if !entries.contains_key(&id) {
                break id;
            }
        };
        entries.insert(
            id,
            Interned {
                look: Look {
                    appearance: appearance.clone(),
                    optics: optics.cloned(),
                },
                hash,
                refs: 1,
            },
        );
        self.by_value.insert_unique(hash, id, |id| entries[id].hash);
        id
    }

    fn release(&mut self, id: AppearanceId) {
        let Some(entry) = self.entries.get_mut(&id) else {
            return;
        };
        entry.refs -= 1;
        if entry.refs == 0 {
            let hash = entry.hash;
            if let Ok(found) = self.by_value.find_entry(hash, |&other| other == id) {
                found.remove();
            }
            self.entries.remove(&id);
        }
    }

    fn get(&self, id: AppearanceId) -> &Look {
        &self.entries[&id].look
    }
}

/// `Appearance` has an `f32` layer, so no `Hash`/`Eq`: interning compares it bit for bit.
fn hash_appearance(a: &Appearance, optics: Option<&Optics>) -> u64 {
    let mut h = FixedHasher.build_hasher();
    // Interned optics carry only `tile` (`Appearances::acquire`).
    optics.map(|o| o.tile).hash(&mut h);
    a.icon_state.hash(&mut h);
    a.layer.0.to_bits().hash(&mut h);
    a.plane.hash(&mut h);
    a.color.hash(&mut h);
    a.pixel_offset.hash(&mut h);
    a.rotation.hash(&mut h);
    a.mouse_opacity.hash(&mut h);
    a.glide.hash(&mut h);
    a.overlays.len().hash(&mut h);
    for o in &a.overlays {
        o.icon_state.hash(&mut h);
        o.dir.hash(&mut h);
        o.color.hash(&mut h);
        o.pixel_offset.hash(&mut h);
    }
    h.finish()
}

fn same_appearance(a: &Appearance, b: &Appearance) -> bool {
    a.icon_state == b.icon_state
        && a.layer.0.to_bits() == b.layer.0.to_bits()
        && a.plane == b.plane
        && a.color == b.color
        && a.pixel_offset == b.pixel_offset
        && a.rotation == b.rotation
        && a.mouse_opacity == b.mouse_opacity
        && a.glide == b.glide
        && a.overlays == b.overlays
}

#[derive(Default)]
struct ClientView {
    view: Option<ViewBox>,
    /// Entities replicated to this client. Invariant after each pass: every `Networked`, indexed
    /// entity inside `view` that `shows`, and none outside `keep()` or that it does not show.
    known: HashMap<Entity, Known>,
    by_net: HashMap<NetId, Entity>,
    /// Per-client ids: small varints on the wire, and they reveal nothing about what other
    /// clients see (global sequential ids would leak how many entities were seen elsewhere).
    next_id: u64,
    /// Freed ids with the tick they were freed, oldest first.
    free_ids: VecDeque<(NetId, u32)>,
    /// The client's appearance cache (see `AppearanceId`): id -> known entities using it.
    cached: HashMap<AppearanceId, u32>,
    /// Ids whose count reached zero this pass; evicted at its end unless used again.
    unused: Vec<AppearanceId>,
    /// Entities whose last change went out unreliably, with the tick of that change.
    unsettled: HashMap<Entity, u32>,
    control: Option<Entity>,
    /// The [`SeeInvisible`] this view was built with.
    see_invisible: u8,
    /// The [`SeeGroups`] this view was built with.
    see_groups: u32,
    /// The [`ViewMask`] this view was built with.
    mask: Option<MaskId>,
    /// Line of sight, while the client has it (no [`ViewMask`], no x-ray [`Sight`]).
    los: Option<Los>,
    /// The tiles seen from the eyes of the last line-of-sight computation, until opacity flips
    /// near them: a step onto the lookahead tile reuses what was seen from it.
    // Cost: ~1000 tiles per eye, two eyes per walking client (~24 KB).
    los_seen: Vec<(TilePos, Vec<TilePos>)>,
    /// Clients under a game [`ViewMask`]: the seen tiles last sent per light block
    /// (`ReplicationOp::Seen`).
    seen_sent: HashMap<TilePos, [u8; 32]>,
    /// The game mask whose seen tiles go with the light blocks; line of sight is not one (the
    /// client computes it from its own predicted position, `ReplicationOp::LineOfSight`).
    tile_mask: Option<MaskId>,
    /// The line-of-sight radius last sent.
    los_sent: Option<u32>,
    /// [`Sight`] flags of this pass.
    sight: u16,
    /// Origins of the light blocks this client subscribes to.
    light_blocks: Vec<TilePos>,
    /// The viewer's level and the one below it, from the last pass.
    levels: Option<(u16, Option<u16>)>,
    /// [`ReplicationOp::Stack`] went out.
    stack_sent: bool,
    appearances: Vec<(AppearanceId, Look)>,
    ops: Vec<ReplicationOp>,
    moves: Vec<Motion>,
}

/// What a client's line-of-sight mask was computed from.
#[derive(Clone, Copy)]
struct Los {
    center: TilePos,
    radius: u32,
    /// The step the viewer took last, seen through from `center + ahead`.
    ahead: Option<(i32, i32)>,
    moved_at: u32,
}

#[derive(Clone, Copy)]
struct Known {
    id: NetId,
    /// The appearance the client has for the entity.
    appearance: Option<AppearanceId>,
    /// What the client was told about it blocking walking.
    blocks: Option<Blocks>,
}

impl ClientView {
    fn keep(&self) -> Option<ViewBox> {
        self.view.map(|v| v.grow(KEEP_MARGIN))
    }

    /// Whether this client may see an entity of that invisibility and those groups at all.
    fn sees(&self, (invisibility, groups, mobs): Hidden<'_>) -> bool {
        invisibility.map_or(0, |i| i.0) <= self.see_invisible
            || groups.is_some_and(|g| g.0 & self.see_groups != 0)
            || mobs.is_some_and(|m| self.control.is_some_and(|c| m.0.contains(&c)))
    }

    /// The appearance this client gets for a tracked entity.
    fn look_of(&self, tracked: &Tracked) -> Option<AppearanceId> {
        tracked.look_for(self.control, self.see_groups)
    }

    /// Whether the mask lets `entity` on `pos` through: everything without a mask, the
    /// controlled entity always.
    fn shows(
        &self,
        masks: &ViewMasks,
        index: &SpatialIndex,
        classes: &SightClasses<'_, '_>,
        entity: Entity,
        pos: TilePos,
    ) -> bool {
        // The level below shows through a transparent tile over it, where that tile shows.
        let pos = match self.levels {
            Some((z, Some(below))) if pos.z == below => {
                let over = TilePos { z, ..pos };
                if !index.transparent(over) {
                    return false;
                }
                over
            }
            _ => pos,
        };
        self.mask.is_none_or(|m| {
            self.control == Some(entity)
                || masks.visible(m, pos)
                || self.sees_through_walls(classes, entity)
        })
    }

    /// One `sight` flag shows this class behind walls; a game mask (camera static) always hides.
    fn sees_through_walls(&self, classes: &SightClasses<'_, '_>, entity: Entity) -> bool {
        if self.tile_mask.is_some() {
            return false;
        }
        let flag = match classes.get(entity) {
            Ok((true, _)) => Sight::SEE_TURFS,
            Ok((_, true)) => Sight::SEE_MOBS,
            _ => Sight::SEE_OBJS,
        };
        self.sight & flag != 0
    }

    /// Queues a light op, masked for a masked client.
    /// Queues a light op; a masked client also gets the seen tiles of a block it now gets whole.
    fn push_light(&mut self, masks: &ViewMasks, op: ReplicationOp) {
        let whole = match &op {
            ReplicationOp::LightBlock { origin, .. } => Some(*origin),
            ReplicationOp::ForgetLight(origins) => {
                for origin in origins {
                    self.seen_sent.remove(origin);
                }
                None
            }
            _ => None,
        };
        self.ops.push(op);
        if let Some(origin) = whole {
            // The client forgets a block's seen tiles when it gets the block again.
            self.seen_sent.remove(&origin);
            self.push_seen(masks, origin);
        }
    }

    /// The seen tiles of the light block at `origin`, if the client is masked and they changed.
    fn push_seen(&mut self, masks: &ViewMasks, origin: TilePos) {
        let Some(mask) = self.tile_mask else {
            return;
        };
        let seen = block_seen(masks, mask, origin);
        if self.seen_sent.insert(origin, seen) != Some(seen) {
            self.ops.push(ReplicationOp::Seen { origin, seen });
        }
    }

    fn alloc_id(&mut self, tick: u32) -> NetId {
        if let Some(&(id, freed)) = self.free_ids.front()
            && tick.wrapping_sub(freed) >= NET_ID_REUSE_TICKS
        {
            self.free_ids.pop_front();
            return id;
        }
        self.next_id += 1;
        NetId(self.next_id)
    }

    fn use_appearance(&mut self, id: Option<AppearanceId>, table: &Appearances) {
        let Some(id) = id else { return };
        match self.cached.get_mut(&id) {
            Some(count) => *count += 1,
            None => {
                self.cached.insert(id, 1);
                self.appearances.push((id, table.get(id).clone()));
            }
        }
    }

    fn drop_appearance(&mut self, id: Option<AppearanceId>) {
        let Some(id) = id else { return };
        if let Some(count) = self.cached.get_mut(&id) {
            *count -= 1;
            if *count == 0 {
                self.unused.push(id);
            }
        }
    }

    fn evict_unused(&mut self) {
        for id in self.unused.drain(..) {
            if self.cached.get(&id) == Some(&0) {
                self.cached.remove(&id);
            }
        }
    }

    fn spawn(
        &mut self,
        tick: u32,
        entity: Entity,
        pos: TilePos,
        facing: Option<&Facing>,
        appearance: Option<AppearanceId>,
        blocks: Option<Blocks>,
        table: &Appearances,
    ) {
        let id = self.alloc_id(tick);
        self.known.insert(
            entity,
            Known {
                id,
                appearance,
                blocks,
            },
        );
        self.by_net.insert(id, entity);
        self.use_appearance(appearance, table);
        let spawned = Spawned {
            id,
            facing: facing.map(|f| f.0),
            appearance,
            blocks,
        };
        match self.ops.last_mut() {
            Some(ReplicationOp::Spawn {
                pos: last,
                entities,
            }) if *last == pos => entities.push(spawned),
            _ => self.ops.push(ReplicationOp::Spawn {
                pos,
                entities: vec![spawned],
            }),
        }
        if self.control == Some(entity) {
            self.ops.push(ReplicationOp::Control(Some(id)));
        }
    }

    fn despawn(&mut self, tick: u32, entity: Entity) -> bool {
        let Some(known) = self.known.remove(&entity) else {
            return false;
        };
        self.by_net.remove(&known.id);
        self.free_ids.push_back((known.id, tick));
        self.unsettled.remove(&entity);
        self.drop_appearance(known.appearance);
        match self.ops.last_mut() {
            Some(ReplicationOp::Despawn(ids)) => ids.push(known.id),
            _ => self.ops.push(ReplicationOp::Despawn(vec![known.id])),
        }
        true
    }

    /// Brings a known entity's appearance to `new`, if the client has another one.
    fn set_appearance(&mut self, entity: Entity, new: Option<AppearanceId>, table: &Appearances) {
        let Some(known) = self.known.get_mut(&entity) else {
            return;
        };
        if known.appearance == new {
            return;
        }
        let (id, old) = (known.id, known.appearance);
        known.appearance = new;
        self.use_appearance(new, table);
        self.drop_appearance(old);
        self.ops.push(ReplicationOp::Appearance(id, new));
    }

    /// Brings a known entity's blocking to `new`, if the client was told otherwise.
    fn set_blocks(&mut self, entity: Entity, new: Option<Blocks>) {
        let Some(known) = self.known.get_mut(&entity) else {
            return;
        };
        if known.blocks != new {
            known.blocks = new;
            self.ops.push(ReplicationOp::Blocks(known.id, new));
        }
    }
}

/// Inclusive tile box on one z-level and, on a multi-deck map, the same box on the level below
/// (seen through open space: tg's z-mimic).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ViewBox {
    z: u16,
    below: Option<u16>,
    min: (i32, i32),
    max: (i32, i32),
}

impl ViewBox {
    fn of(viewer: &Viewer, stack: Option<&LevelStack>) -> Self {
        let r = viewer.radius.min(i32::MAX as u32) as i32;
        let c = viewer.center;
        Self {
            z: c.z,
            below: stack.and_then(|s| s.below(c.z)),
            min: (c.x.saturating_sub(r), c.y.saturating_sub(r)),
            max: (c.x.saturating_add(r), c.y.saturating_add(r)),
        }
    }

    fn grow(&self, by: u32) -> Self {
        let by = by.min(i32::MAX as u32) as i32;
        Self {
            z: self.z,
            below: self.below,
            min: (self.min.0.saturating_sub(by), self.min.1.saturating_sub(by)),
            max: (self.max.0.saturating_add(by), self.max.1.saturating_add(by)),
        }
    }

    /// The box's levels: its own, then the one below.
    fn levels(&self) -> impl Iterator<Item = u16> + use<> {
        std::iter::once(self.z).chain(self.below)
    }

    fn contains(&self, pos: TilePos) -> bool {
        (pos.z == self.z || Some(pos.z) == self.below)
            && (self.min.0..=self.max.0).contains(&pos.x)
            && (self.min.1..=self.max.1).contains(&pos.y)
    }

    fn chunk_range(&self) -> (ChunkPos, ChunkPos) {
        (
            ChunkPos::of(TilePos::new(self.min.0, self.min.1, self.z)),
            ChunkPos::of(TilePos::new(self.max.0, self.max.1, self.z)),
        )
    }

    fn chunks(&self) -> impl Iterator<Item = ChunkPos> + use<> {
        let (lo, hi) = self.chunk_range();
        self.levels().flat_map(move |z| {
            (lo.y..=hi.y).flat_map(move |y| (lo.x..=hi.x).map(move |x| ChunkPos { x, y, z }))
        })
    }

    fn has_chunk(&self, chunk: ChunkPos) -> bool {
        let (lo, hi) = self.chunk_range();
        (chunk.z == self.z || Some(chunk.z) == self.below)
            && (lo.x..=hi.x).contains(&chunk.x)
            && (lo.y..=hi.y).contains(&chunk.y)
    }

    /// The part of `self` outside `other` on each of its levels, as up to four disjoint
    /// `(min, max)` rects.
    fn minus(&self, other: Option<&ViewBox>) -> Vec<((i32, i32), (i32, i32))> {
        let Some(other) = other.filter(|o| o.z == self.z && o.below == self.below) else {
            return vec![(self.min, self.max)];
        };
        let lo = (self.min.0.max(other.min.0), self.min.1.max(other.min.1));
        let hi = (self.max.0.min(other.max.0), self.max.1.min(other.max.1));
        if lo.0 > hi.0 || lo.1 > hi.1 {
            return vec![(self.min, self.max)];
        }
        [
            // Full-width rows below and above the overlap, then the sides of the overlap rows.
            ((self.min.0, self.min.1), (self.max.0, lo.1 - 1)),
            ((self.min.0, hi.1 + 1), (self.max.0, self.max.1)),
            ((self.min.0, lo.1), (lo.0 - 1, hi.1)),
            ((hi.0 + 1, lo.1), (self.max.0, hi.1)),
        ]
        .into_iter()
        .filter(|(min, max)| min.0 <= max.0 && min.1 <= max.1)
        .collect()
    }
}

/// Bytes the server put on the wire per client (UDP payloads: whole QUIC packets with our frames,
/// datagrams, acks, retransmits and keep-alives; IP and UDP headers add 28 bytes per packet), and
/// the cost of the replication pass.
#[derive(Resource, Default, Debug)]
pub struct ReplicationStats {
    pub clients: HashMap<Entity, ClientTraffic>,
    /// Wall time of the last replication pass.
    pub replicate_time: Duration,
    /// Longest replication pass since the last log line.
    pub replicate_time_max: Duration,
    /// Totals over all clients since start: entities spawned on and despawned from clients, and
    /// appearance values sent (a spawn whose appearance the client has cached sends none).
    pub spawned_total: u64,
    pub despawned_total: u64,
    pub appearances_sent_total: u64,
    /// Line of sight: time of the last pass's computations and computations since start.
    pub los_time: Duration,
    pub los_computed_total: u64,
    /// Set to count [`ClientTraffic::replication_bytes_total`]: one extra size pass per batch.
    pub count_payload: bool,
    replicate_time_sum: Duration,
    replicate_runs: u32,
    window_start: Option<f64>,
    last_log: f64,
}

#[derive(Default, Debug, Clone, Copy)]
pub struct ClientTraffic {
    pub bytes_total: u64,
    pub packets_total: u64,
    /// Over the last full one-second window.
    pub bytes_per_sec: f64,
    pub packets_per_sec: f64,
    /// Serialized size of the replication messages themselves (no transport framing, acks,
    /// resends or pings), when `ReplicationStats::count_payload` is set.
    pub replication_bytes_total: u64,
    /// The part of `replication_bytes_total` that is light blocks (ops only, batch framing not
    /// counted).
    pub light_bytes_total: u64,
}

/// Requires `ss13_world::WorldPlugin` (the spatial index) and [`NetServerPlugin`](crate::server::NetServerPlugin).
pub struct ReplicationServerPlugin;

impl Plugin for ReplicationServerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Interest>()
            .init_resource::<ReplicationStats>()
            .init_resource::<ViewMasks>()
            .add_systems(
                PostUpdate,
                replicate
                    .in_set(NetServerSystems::Send)
                    .run_if(resource_exists::<SpatialIndex>),
            )
            .add_systems(
                PostUpdate,
                crate::animation::server::send
                    .after(replicate)
                    .in_set(NetServerSystems::Send),
            )
            .add_systems(PostUpdate, measure_traffic.after(NetServerSystems::Send));
    }
}

type States<'w, 's> = Query<
    'w,
    's,
    (
        &'static TilePos,
        Option<&'static Facing>,
        Option<&'static Appearance>,
        Option<&'static Optics>,
        (
            Option<&'static Invisibility>,
            Option<&'static VisibleToGroups>,
            Option<&'static VisibleToMobs>,
        ),
        BlockData,
    ),
    With<Networked>,
>;

/// What [`Blocks`] is made of.
type BlockData = (
    Has<Density>,
    Option<&'static Border>,
    Has<ClimbWalkable>,
    Has<Mover>,
    Has<NonDenseBorder>,
    Option<&'static ClimbsUp>,
    Has<FallsThrough>,
    Has<Pushable>,
);

/// [`Blocks`] of an entity: dense things that do not move by themselves.
fn blocks_of(
    (dense, border, climbable, mover, edge_only, climbs, hole, pushable): (
        bool,
        Option<&Border>,
        bool,
        bool,
        bool,
        Option<&ClimbsUp>,
        bool,
        bool,
    ),
) -> Option<Blocks> {
    if let Some(ClimbsUp(dir)) = climbs {
        return Some(Blocks::Stairs(*dir as u8));
    }
    if hole {
        return Some(Blocks::Hole);
    }
    let dense = dense || (edge_only && border.is_some());
    if !dense || mover {
        return None;
    }
    // shortcut: a loose border object (an unanchored directional window) goes as its edges, not
    // pushable; the client predicts no shove of it and waits for the server.
    Some(match (border, climbable) {
        (Some(b), _) => Blocks::Edges(b.edges),
        (None, true) => Blocks::Climbable,
        (None, false) if pushable => Blocks::Pushable,
        (None, false) => Blocks::Dense,
    })
}

fn replicate(
    mut interest: ResMut<Interest>,
    mut stats: ResMut<ReplicationStats>,
    mut masks: ResMut<ViewMasks>,
    index: Res<SpatialIndex>,
    states: States,
    changed: Query<
        (
            Entity,
            Ref<TilePos>,
            Option<Ref<Facing>>,
            Option<Ref<Appearance>>,
            Option<Ref<Optics>>,
            Ref<Networked>,
            Hidden<'static>,
        ),
        Or<(
            Changed<TilePos>,
            Changed<Facing>,
            Changed<Appearance>,
            Changed<Optics>,
            Added<Networked>,
        )>,
    >,
    mut removed_networked: RemovedComponents<Networked>,
    mut removed_pos: RemovedComponents<TilePos>,
    mut removed_facing: RemovedComponents<Facing>,
    mut removed_appearance: RemovedComponents<Appearance>,
    mut removed_optics: RemovedComponents<Optics>,
    viewers: Query<
        (
            Entity,
            Ref<Viewer>,
            Option<&ControlledEntity>,
            (Option<&SeeInvisible>, Option<&SeeGroups>),
            Option<&ViewMask>,
            Option<&Sight>,
        ),
        With<ClientLink>,
    >,
    mut removed_viewers: RemovedComponents<Viewer>,
    links: Query<&ClientLink>,
    (
        blocking_changed,
        mut lost_density,
        mut lost_border,
        mut lost_climbable,
        mut lost_hole,
        mut lost_pushable,
    ): (
        Query<
            Entity,
            (
                With<Networked>,
                Or<(
                    Added<Density>,
                    Changed<Border>,
                    Added<ClimbWalkable>,
                    Added<Mover>,
                    Added<FallsThrough>,
                    Added<Pushable>,
                )>,
            ),
        >,
        RemovedComponents<Density>,
        RemovedComponents<Border>,
        RemovedComponents<ClimbWalkable>,
        RemovedComponents<FallsThrough>,
        RemovedComponents<Pushable>,
    ),
    (stack, alt_states, alt_changed, mut removed_alts, classes): (
        Option<Res<LevelStack>>,
        Alts,
        Query<Entity, (With<Networked>, Changed<AltAppearances>)>,
        RemovedComponents<AltAppearances>,
        SightClasses,
    ),
) {
    let start = Instant::now();
    let interest = &mut *interest;
    interest.tick = interest.tick.wrapping_add(1);
    let tick = interest.tick;
    let Interest {
        clients,
        chunk_viewers,
        tracked,
        appearances,
        lights,
        grid,
        ..
    } = interest;
    let mut candidates = Vec::new();
    let (mut spawned, mut despawned) = (0u64, 0u64);

    // 0. Opacity and lights of this tick first: line of sight needs the current opacity.
    let left: Vec<Entity> = (removed_networked.read().chain(removed_pos.read()))
        .filter(|&e| !states.contains(e)) // Re-added in the same tick: a change below.
        .collect();
    for &entity in &left {
        lights.set(entity, None, None);
    }
    let lost_optics: Vec<Entity> = removed_optics.read().collect();
    for &entity in &lost_optics {
        if let Ok((&pos, _, _, None, ..)) = states.get(entity) {
            lights.set(entity, index.in_bounds(pos).then_some(pos), None);
        }
    }
    for (entity, pos, _, _, optics, ..) in &changed {
        lights.set(
            entity,
            index.in_bounds(*pos).then_some(*pos),
            optics.as_deref(),
        );
    }
    let los_start = Instant::now();
    let los_computed =
        update_line_of_sight(clients, &viewers, lights, &index, grid, &mut masks, tick);
    stats.los_time = los_start.elapsed();
    stats.los_computed_total += los_computed;
    let mask_changes = masks.take_changes();
    let masks_res = masks;
    let masks = &*masks_res;

    // 1. Entities that left replication: despawn wherever they are known.
    for entity in left {
        let Some(gone) = tracked.remove(&entity) else {
            continue;
        };
        if let Some(id) = gone.appearance {
            appearances.release(id);
        }
        release_alts(appearances, gone.alts);
        collect_viewers(chunk_viewers, &mut candidates, [Some(gone.pos), None]);
        for client in &candidates {
            let view = clients.get_mut(client).expect("registered viewer");
            despawned += u64::from(view.despawn(tick, entity));
        }
    }
    for entity in removed_facing.read() {
        if let (Some(t), Ok((&pos, None, ..))) = (tracked.get(&entity), states.get(entity)) {
            collect_viewers(chunk_viewers, &mut candidates, [Some(t.pos), None]);
            for client in &candidates {
                let view = clients.get_mut(client).expect("registered viewer");
                if let Some(known) = view.known.get(&entity) {
                    view.moves.push(Motion {
                        id: known.id,
                        pos,
                        facing: None,
                    });
                    view.unsettled.insert(entity, tick);
                }
            }
        }
    }
    for entity in removed_appearance.read() {
        if let (Some(t), Ok((_, _, None, ..))) = (tracked.get_mut(&entity), states.get(entity)) {
            let Some(old) = t.appearance.take() else {
                continue;
            };
            appearances.release(old);
            collect_viewers(chunk_viewers, &mut candidates, [Some(t.pos), None]);
            for client in &candidates {
                let view = clients.get_mut(client).expect("registered viewer");
                let id = view.look_of(t);
                view.set_appearance(entity, id, appearances);
            }
        }
    }

    // Lost optics: the appearance is interned again without them.
    for entity in lost_optics {
        if let (Some(t), Ok((_, _, Some(appearance), None, ..))) =
            (tracked.get_mut(&entity), states.get(entity))
        {
            let new = appearances.acquire(appearance, None);
            if let Some(old) = t.appearance.replace(new) {
                appearances.release(old);
            }
            let alts = intern_alts(appearances, alt_states.get(entity).ok(), None);
            release_alts(appearances, std::mem::replace(&mut t.alts, alts));
            collect_viewers(chunk_viewers, &mut candidates, [Some(t.pos), None]);
            for client in &candidates {
                let view = clients.get_mut(client).expect("registered viewer");
                let id = view.look_of(t);
                view.set_appearance(entity, id, appearances);
            }
        }
    }

    // A viewer changing level this tick (its mob climbed or fell): its entities are judged
    // against the new box already, or the mob itself would leave the old one and come back as a
    // new entity in 3 (drawn popping onto its tile instead of stepping).
    let next_level: HashMap<Entity, ViewBox> = viewers
        .iter()
        .filter(|v| v.1.is_changed())
        .filter_map(|(client, viewer, ..)| {
            let new = ViewBox::of(&viewer, stack.as_deref());
            let old = clients.get(&client)?.view?;
            (old.z != new.z).then_some((client, new.grow(KEEP_MARGIN)))
        })
        .collect();

    // 2. Changed entities, against the current view boxes.
    for (entity, pos, facing, appearance, optics, networked, invisibility) in &changed {
        let motion = pos.is_changed() || facing.as_ref().is_some_and(|f| f.is_changed());
        let appearance_changed = networked.is_added()
            || appearance.as_ref().is_some_and(|a| a.is_changed())
            || optics.as_ref().is_some_and(|o| o.is_changed());
        let optics = optics.as_deref();
        let indexed = index.in_bounds(*pos);
        // Tracked entities are updated to the current values.
        let old_pos = match tracked.get_mut(&entity) {
            Some(t) => {
                let old = t.pos;
                t.pos = *pos;
                if appearance_changed {
                    // Acquire first: an unchanged value keeps its id instead of being re-interned.
                    let new = appearance
                        .as_deref()
                        .map(|a| appearances.acquire(a, optics));
                    if let Some(id) = t.appearance {
                        appearances.release(id);
                    }
                    t.appearance = new;
                    // The tile lighting they carry may have changed.
                    let alts = intern_alts(appearances, alt_states.get(entity).ok(), optics);
                    release_alts(appearances, std::mem::replace(&mut t.alts, alts));
                }
                Some(old)
            }
            None => None,
        };
        collect_viewers(chunk_viewers, &mut candidates, [old_pos, Some(*pos)]);
        let facing = facing.as_deref();
        let mut is_tracked = old_pos.is_some();
        for client in &candidates {
            let view = clients.get_mut(client).expect("registered viewer");
            let shown = indexed && view.shows(masks, &index, &classes, entity, *pos);
            let in_view =
                shown && view.sees(invisibility) && view.view.is_some_and(|v| v.contains(*pos));
            let keep = next_level.get(client).copied().or_else(|| view.keep());
            let in_keep = shown && keep.is_some_and(|v| v.contains(*pos));
            match view.known.get(&entity).copied() {
                None if in_view => {
                    if !is_tracked {
                        is_tracked = true;
                        track(
                            tracked,
                            appearances,
                            entity,
                            *pos,
                            appearance.as_deref(),
                            optics,
                            alt_states.get(entity).ok(),
                        );
                    }
                    let id = view.look_of(&tracked[&entity]);
                    let blocks = states.get(entity).ok().and_then(|s| blocks_of(s.5));
                    view.spawn(tick, entity, *pos, facing, id, blocks, appearances);
                    spawned += 1;
                }
                Some(known) if in_keep => {
                    if motion {
                        view.moves.push(Motion {
                            id: known.id,
                            pos: *pos,
                            facing: facing.map(|f| f.0),
                        });
                        view.unsettled.insert(entity, tick);
                    }
                    if is_tracked {
                        let id = view.look_of(&tracked[&entity]);
                        view.set_appearance(entity, id, appearances);
                    }
                }
                Some(_) => despawned += u64::from(view.despawn(tick, entity)),
                None => {}
            }
        }
    }

    // 2c. Alternate appearances that changed: the clients that know the entity. Entities
    // tracked above already have their current ones (re-interning them is a no-op).
    let mut realt: Vec<Entity> = alt_changed.iter().collect();
    realt.extend(removed_alts.read());
    realt.sort_unstable();
    realt.dedup();
    for entity in realt {
        let Some(t) = tracked.get_mut(&entity) else {
            continue;
        };
        let optics = states.get(entity).ok().and_then(|s| s.3);
        let alts = intern_alts(appearances, alt_states.get(entity).ok(), optics);
        release_alts(appearances, std::mem::replace(&mut t.alts, alts));
        collect_viewers(chunk_viewers, &mut candidates, [Some(t.pos), None]);
        for client in &candidates {
            let view = clients.get_mut(client).expect("registered viewer");
            let id = view.look_of(t);
            view.set_appearance(entity, id, appearances);
        }
    }

    // 2a. Blocking that changed (doors opening and closing): the clients that know the entity.
    // Entities spawned above already went with their current blocking (a no-op here).
    let mut reblocked: Vec<Entity> = blocking_changed.iter().collect();
    reblocked.extend(lost_density.read());
    reblocked.extend(lost_border.read());
    reblocked.extend(lost_climbable.read());
    reblocked.extend(lost_hole.read());
    reblocked.extend(lost_pushable.read());
    reblocked.sort_unstable();
    reblocked.dedup();
    for entity in reblocked {
        let Some(t) = tracked.get(&entity) else {
            continue;
        };
        let new = states.get(entity).ok().and_then(|s| blocks_of(s.5));
        collect_viewers(chunk_viewers, &mut candidates, [Some(t.pos), None]);
        for client in &candidates {
            let view = clients.get_mut(client).expect("registered viewer");
            view.set_blocks(entity, new);
        }
    }

    // 2b. Mask chunks that changed: the flipped tiles of each client using the mask. Its light
    // blocks there get their seen tiles again.
    let mut light_flips: HashSet<(Entity, TilePos)> = HashSet::default();
    let mut by_mask: HashMap<MaskId, Vec<Entity>> = HashMap::default();
    if !mask_changes.is_empty() {
        for (&client, view) in clients.iter() {
            if let Some(mask) = view.mask {
                by_mask.entry(mask).or_default().push(client);
            }
        }
    }
    for (mask, chunk, flipped) in mask_changes {
        for client in by_mask.get(&mask).into_iter().flatten() {
            let view = clients.get_mut(client).expect("indexed client");
            let Some(keep) = view.keep() else {
                continue;
            };
            let min = (chunk.0 * MASK_CHUNK, chunk.1 * MASK_CHUNK);
            for bit in (0..64).filter(|b| flipped & (1 << b) != 0) {
                let pos = TilePos::new(min.0 + bit % MASK_CHUNK, min.1 + bit / MASK_CHUNK, chunk.2);
                let (origin, _) = light_block(pos);
                if view.light_blocks.contains(&origin) {
                    light_flips.insert((*client, origin));
                }
                if !keep.contains(pos) {
                    continue;
                }
                // The tile below shows or hides with this one.
                let under = view
                    .levels
                    .and_then(|(z, below)| below.filter(|_| z == pos.z))
                    .map(|below| TilePos { z: below, ..pos });
                let on_tiles: Vec<Entity> = index
                    .on_tile(pos)
                    .iter()
                    .chain(under.iter().flat_map(|&u| index.on_tile(u)))
                    .copied()
                    .collect();
                for entity in on_tiles {
                    let (s, d) = refresh(
                        view,
                        tick,
                        entity,
                        (&states, &alt_states, &classes),
                        &index,
                        masks,
                        tracked,
                        appearances,
                    );
                    spawned += s;
                    despawned += d;
                }
            }
        }
    }

    // 3. Viewers: removed, then moved or new.
    let mut los_dropped: Vec<Entity> = Vec::new();
    for client in removed_viewers.read() {
        if let Some(view) = clients.get_mut(&client) {
            if view.los.take().is_some() {
                view.los_seen = Vec::new();
                los_dropped.push(client);
            }
            let old = view.keep();
            let known: Vec<Entity> = view.known.keys().copied().collect();
            for entity in known {
                despawned += u64::from(view.despawn(tick, entity));
            }
            view.view = None;
            if let Some(old) = old {
                unregister_chunks(chunk_viewers, client, &old, None);
            }
            update_light_interest(lights, view, client, masks);
        }
    }
    for (client, viewer, controlled, see, mask, sight) in &viewers {
        let new_view = ViewBox::of(&viewer, stack.as_deref());
        let view = clients.entry(client).or_default();
        view.levels = Some((new_view.z, new_view.below));
        // The level stack goes once, before anything of a level below is spawned.
        if !view.stack_sent
            && let Some(stack) = stack.as_deref().filter(|s| !s.links.is_empty())
        {
            view.stack_sent = true;
            view.ops.push(ReplicationOp::Stack(stack.links.clone()));
        }
        let (see, groups) = (see.0.map_or(0, |s| s.0), see.1.map_or(0, |g| g.0));
        let viewer_mask = mask;
        let mask = mask
            .map(|m| m.0)
            .or_else(|| view.los.is_some().then(|| los_mask(client)));
        // Line of sight: the client blacks out what its eye does not see, within this radius.
        let los = (viewer_mask.is_none())
            .then_some(view.los)
            .flatten()
            .map(|l| l.radius);
        if view.los_sent != los {
            view.los_sent = los;
            view.ops.push(ReplicationOp::LineOfSight(los));
        }
        let want = controlled.map(|c| c.0);
        let old_control = view.control;
        if want != view.control {
            view.control = want;
            match want {
                // Not yet in view: sent right after its spawn.
                Some(entity) => {
                    if let Some(known) = view.known.get(&entity) {
                        view.ops.push(ReplicationOp::Control(Some(known.id)));
                    }
                }
                None => view.ops.push(ReplicationOp::Control(None)),
            }
        }
        let sight = sight.map_or(0, |s| s.0);
        if view.see_invisible != see
            || view.see_groups != groups
            || view.sight != sight
            || view.mask != mask
            || old_control != want
        {
            // Walk the whole box again below: some known entities may hide, others appear, and
            // those with alternate appearances may look different.
            view.see_invisible = see;
            view.see_groups = groups;
            view.sight = sight;
            let mask_changed = view.mask != mask;
            view.mask = mask;
            view.tile_mask = viewer_mask.map(|m| m.0);
            let keep = new_view.grow(KEEP_MARGIN);
            let hidden: Vec<Entity> = view
                .known
                .keys()
                .copied()
                .filter(|&e| {
                    states.get(e).is_ok_and(|(pos, _, _, _, invisibility, _)| {
                        !view.sees(invisibility)
                            || !keep.contains(*pos)
                            || !view.shows(masks, &index, &classes, e, *pos)
                    })
                })
                .collect();
            for entity in hidden {
                despawned += u64::from(view.despawn(tick, entity));
            }
            let alternated: Vec<Entity> = (view.known.keys())
                .copied()
                .filter(|e| tracked.get(e).is_some_and(|t| !t.alts.is_empty()))
                .collect();
            for entity in alternated {
                let id = view.look_of(&tracked[&entity]);
                view.set_appearance(entity, id, appearances);
            }
            if mask_changed {
                // Every light block goes again, under the new mask (or none).
                let origins = std::mem::take(&mut view.light_blocks);
                for &origin in &origins {
                    lights.unsubscribe(origin, client);
                }
                view.seen_sent.clear();
                if !origins.is_empty() {
                    view.ops.push(ReplicationOp::ForgetLight(origins));
                }
            }
            if let Some(old) = view.keep() {
                unregister_chunks(chunk_viewers, client, &old, None);
            }
            view.view = None;
        }
        let old_view = view.view;
        if old_view != Some(new_view) {
            let old_keep = view.keep();
            let new_keep = new_view.grow(KEEP_MARGIN);
            match old_keep {
                // Another level: what shows in both boxes stays (the mob that climbed, the hall
                // under the open space it looked down through), the rest goes.
                Some(old) if (old.z, old.below) != (new_keep.z, new_keep.below) => {
                    let gone: Vec<Entity> = view
                        .known
                        .keys()
                        .copied()
                        .filter(|&e| {
                            states.get(e).is_ok_and(|(pos, ..)| {
                                !new_keep.contains(*pos)
                                    || !view.shows(masks, &index, &classes, e, *pos)
                            })
                        })
                        .collect();
                    for entity in gone {
                        despawned += u64::from(view.despawn(tick, entity));
                    }
                }
                Some(old) => {
                    for (min, max) in old.minus(Some(&new_keep)) {
                        for z in old.levels() {
                            for entity in index.in_rect(z, min, max) {
                                despawned += u64::from(view.despawn(tick, entity));
                            }
                        }
                    }
                }
                None => {}
            }
            let rects = new_view.minus(old_view.as_ref());
            let fresh = rects
                .iter()
                .flat_map(|&(min, max)| new_view.levels().map(move |z| (z, min, max)));
            for (z, min, max) in fresh {
                for entity in index.in_rect(z, min, max) {
                    if view.known.contains_key(&entity) {
                        continue;
                    }
                    if let Ok((&pos, facing, appearance, optics, invisibility, block)) =
                        states.get(entity)
                        && view.sees(invisibility)
                        && view.shows(masks, &index, &classes, entity, pos)
                    {
                        if !tracked.contains_key(&entity) {
                            let alts = alt_states.get(entity).ok();
                            track(tracked, appearances, entity, pos, appearance, optics, alts);
                        }
                        let interned = view.look_of(&tracked[&entity]);
                        view.spawn(
                            tick,
                            entity,
                            pos,
                            facing,
                            interned,
                            blocks_of(block),
                            appearances,
                        );
                        spawned += 1;
                    }
                }
            }
            view.view = Some(new_view);
            update_light_interest(lights, view, client, masks);
            if let Some(old) = old_keep {
                unregister_chunks(chunk_viewers, client, &old, Some(&new_keep));
            }
            for chunk in new_keep.chunks() {
                if old_keep.is_none_or(|old| !old.has_chunk(chunk)) {
                    chunk_viewers.entry(chunk).or_default().push(client);
                }
            }
        }

        // A masked view shows its controlled entity anywhere: both ends of a change.
        if view.mask.is_some() && old_control != want {
            for entity in [old_control, want].into_iter().flatten() {
                let (s, d) = refresh(
                    view,
                    tick,
                    entity,
                    (&states, &alt_states, &classes),
                    &index,
                    masks,
                    tracked,
                    appearances,
                );
                spawned += s;
                despawned += d;
            }
        }
    }

    // Light changes of this tick to the blocks' subscribers (a block subscribed this tick already
    // has them in its snapshot; the repeat is idempotent).
    lights.take_changes(|to, op| {
        for client in to {
            if let Some(view) = clients.get_mut(client) {
                view.push_light(masks, op.clone());
            }
        }
    });
    for (client, origin) in light_flips {
        if let Some(view) = clients.get_mut(&client)
            && view.light_blocks.contains(&origin)
        {
            view.push_seen(masks, origin);
        }
    }

    // 4. Settle entities whose last change went out unreliably and that stopped changing.
    for view in clients.values_mut() {
        let ClientView {
            unsettled,
            ops,
            known,
            ..
        } = view;
        unsettled.retain(|&entity, &mut changed_at| {
            if tick.wrapping_sub(changed_at) < SETTLE_TICKS {
                return true;
            }
            if let (Some(k), Ok((&pos, facing, ..))) = (known.get(&entity), states.get(entity)) {
                ops.push(ReplicationOp::Settle {
                    id: k.id,
                    pos,
                    facing: facing.map(|f| f.0),
                });
            }
            false
        });
    }

    // 5. Flush; forget clients whose connection is gone.
    let mut gone = Vec::new();
    let mut appearances_sent = 0;
    let count_payload = stats.count_payload;
    for (&client, view) in clients.iter_mut() {
        view.evict_unused();
        let Ok(link) = links.get(client) else {
            gone.push(client);
            continue;
        };
        let mut payload = 0;
        if !view.ops.is_empty() {
            appearances_sent += view.appearances.len() as u64;
            let batch = ReplicationBatch {
                tick,
                appearances: std::mem::take(&mut view.appearances),
                ops: std::mem::take(&mut view.ops),
            };
            if count_payload {
                payload += postcard::experimental::serialized_size(&batch).unwrap_or(0);
                let light: usize = (batch.ops.iter())
                    .filter(|op| op.is_light())
                    .map(|op| postcard::experimental::serialized_size(op).unwrap_or(0))
                    .sum();
                stats.clients.entry(client).or_default().light_bytes_total += light as u64;
            }
            if !link.send(&ServerMessage::Replication(batch)) {
                // The client's mirror of this batch is gone: it cannot stay in sync.
                link.kick(
                    crate::protocol::close::PROTOCOL,
                    "replication batch over the frame limit",
                );
            }
        }
        if !view.moves.is_empty() {
            if count_payload {
                // Same encoding as `MotionBatch`, without moving the moves.
                payload +=
                    postcard::experimental::serialized_size(&(tick, &view.moves)).unwrap_or(0);
            }
            link.send_motion(tick, &view.moves);
            view.moves.clear();
        }
        if payload > 0 {
            stats
                .clients
                .entry(client)
                .or_default()
                .replication_bytes_total += payload as u64;
        }
    }
    for client in gone {
        if let Some(view) = clients.remove(&client) {
            if view.los.is_some() {
                los_dropped.push(client);
            }
            if let Some(old) = view.keep() {
                unregister_chunks(chunk_viewers, client, &old, None);
            }
            for origin in view.light_blocks {
                lights.unsubscribe(origin, client);
            }
        }
    }

    let mut masks = masks_res;
    for client in los_dropped {
        masks.set_tiles(los_mask(client), []);
    }

    let elapsed = start.elapsed();
    stats.spawned_total += spawned;
    stats.despawned_total += despawned;
    stats.appearances_sent_total += appearances_sent;
    stats.replicate_time = elapsed;
    stats.replicate_time_max = stats.replicate_time_max.max(elapsed);
    stats.replicate_time_sum += elapsed;
    stats.replicate_runs += 1;
}

/// Subscribes `client` to the light blocks within [`LIGHT_MARGIN`] of its view box (sending
/// each new block's state) and forgets those beyond `LIGHT_MARGIN + KEEP_MARGIN` (hysteresis,
/// as entities). Cost: the ~16-25 blocks around the view, only when the view box changed.
fn update_light_interest(
    lights: &mut LightGrid,
    view: &mut ClientView,
    client: Entity,
    masks: &ViewMasks,
) {
    let keep = view.view.map(|v| v.grow(LIGHT_MARGIN + KEEP_MARGIN));
    let mut forgotten = Vec::new();
    view.light_blocks.retain(|&origin| {
        // Also the level below: its own lights light what shows through open space.
        let kept =
            keep.is_some_and(|k| k.levels().any(|z| block_overlaps(origin, z, k.min, k.max)));
        if !kept {
            lights.unsubscribe(origin, client);
            forgotten.push(origin);
        }
        kept
    });
    if !forgotten.is_empty() {
        view.push_light(masks, ReplicationOp::ForgetLight(forgotten));
    }
    let Some(want) = view.view.map(|v| v.grow(LIGHT_MARGIN)) else {
        return;
    };
    let origins: Vec<TilePos> = want
        .levels()
        .flat_map(|z| blocks_in(z, want.min, want.max))
        .collect();
    for origin in origins {
        if !view.light_blocks.contains(&origin) {
            view.light_blocks.push(origin);
            let op = lights.subscribe(origin, client);
            view.push_light(masks, op);
        }
    }
}

/// Recomputes the line-of-sight masks that are due: of new line-of-sight clients, of viewers
/// that moved, and of those within reach of a tile whose opacity flipped. Drops the mask of a
/// client that got a [`ViewMask`] or x-ray [`Sight`]. Returns the number of computations.
// Cost: an idle tick walks the viewers and this tick's opacity flips (doors: a few). A
// computation is BYOND's ring passes over (2 * (radius + LOS_MARGIN) + 1)^2 tiles, twice while
// the viewer walks (lookahead), on a step or a door flip in reach only.
fn update_line_of_sight(
    clients: &mut HashMap<Entity, ClientView>,
    viewers: &Query<
        (
            Entity,
            Ref<Viewer>,
            Option<&ControlledEntity>,
            (Option<&SeeInvisible>, Option<&SeeGroups>),
            Option<&ViewMask>,
            Option<&Sight>,
        ),
        With<ClientLink>,
    >,
    lights: &mut LightGrid,
    index: &SpatialIndex,
    grid: &mut ViewGrid,
    masks: &mut ViewMasks,
    tick: u32,
) -> u64 {
    let flips = lights.take_flips();
    let mut computed = 0;
    let mut tiles: Vec<TilePos> = Vec::new();
    let mut seen_now: Vec<(TilePos, Vec<TilePos>)> = Vec::new();
    for (client, viewer, _, _, mask, sight) in viewers {
        let view = clients.entry(client).or_default();
        if mask.is_some() || sight.is_some_and(|s| s.sees_all()) {
            if view.los.take().is_some() {
                view.los_seen = Vec::new();
                masks.set_tiles(los_mask(client), []);
            }
            continue;
        }
        let center = viewer.center;
        let radius = viewer.radius.saturating_add(LOS_MARGIN);
        // Cached eyes are within 2 tiles of `center`.
        let flipped = (flips.iter()).any(|&f| f.chebyshev(center).is_some_and(|d| d <= radius + 2));
        if flipped || view.los.is_some_and(|l| l.radius != radius) {
            view.los_seen.clear();
        }
        let new = match view.los {
            None => Los {
                center,
                radius,
                ahead: None,
                moved_at: tick,
            },
            Some(los) if los.center != center => {
                let step = (center.x - los.center.x, center.y - los.center.y);
                let one = los.center.chebyshev(center) == Some(1);
                Los {
                    center,
                    radius,
                    ahead: one.then_some(step),
                    moved_at: tick,
                }
            }
            Some(los) => {
                let expired =
                    los.ahead.is_some() && tick.wrapping_sub(los.moved_at) >= LOOKAHEAD_TICKS;
                if !expired && !flipped && los.radius == radius {
                    continue;
                }
                Los {
                    radius,
                    ahead: los.ahead.filter(|_| !expired),
                    ..los
                }
            }
        };
        view.los = Some(new);
        tiles.clear();
        let r = new.radius.min(64) as i32;
        let mut eyes = vec![center];
        if let Some((dx, dy)) = new.ahead {
            let next = TilePos::new(center.x + dx, center.y + dy, center.z);
            if !opaque_at(lights, index, next) {
                eyes.push(next);
            }
        }
        for eye in eyes {
            if let Some(at) = view.los_seen.iter().position(|(e, _)| *e == eye) {
                let cached = view.los_seen.swap_remove(at);
                tiles.extend_from_slice(&cached.1);
                seen_now.push(cached);
                continue;
            }
            let mut cache: Option<(TilePos, Option<&[u16]>)> = None;
            grid.compute(r, |dx, dy| {
                let pos = TilePos::new(eye.x + dx, eye.y + dy, eye.z);
                if !index.in_bounds(pos) {
                    return true;
                }
                let (origin, i) = light_block(pos);
                let block = match cache {
                    Some((o, b)) if o == origin => b,
                    _ => {
                        let b = lights.block_opacity(origin);
                        cache = Some((origin, b));
                        b
                    }
                };
                block.is_some_and(|b| b[i] > 0)
            });
            computed += 1;
            // The glimpsed ring too: the client draws it dimmed.
            let seen: Vec<TilePos> = (grid.seen().chain(grid.glimpses()))
                .map(|(dx, dy)| TilePos::new(eye.x + dx, eye.y + dy, eye.z))
                .filter(|&p| index.in_bounds(p))
                .collect();
            tiles.extend_from_slice(&seen);
            seen_now.push((eye, seen));
        }
        view.los_seen = std::mem::take(&mut seen_now);
        masks.set_tiles(los_mask(client), tiles.iter().copied());
    }
    computed
}

fn opaque_at(lights: &LightGrid, index: &SpatialIndex, pos: TilePos) -> bool {
    if !index.in_bounds(pos) {
        return true;
    }
    let (origin, i) = light_block(pos);
    lights.block_opacity(origin).is_some_and(|b| b[i] > 0)
}

/// Spawns or despawns `entity` on `view` by the current rules (box, keep, invisibility, mask);
/// returns (spawned, despawned).
#[allow(clippy::too_many_arguments)]
fn refresh(
    view: &mut ClientView,
    tick: u32,
    entity: Entity,
    (states, alt_states, classes): (&States<'_, '_>, &Alts<'_, '_>, &SightClasses<'_, '_>),
    index: &SpatialIndex,
    masks: &ViewMasks,
    tracked: &mut HashMap<Entity, Tracked>,
    appearances: &mut Appearances,
) -> (u64, u64) {
    let Ok((&pos, facing, appearance, optics, invisibility, block)) = states.get(entity) else {
        return (0, 0);
    };
    let shown = index.in_bounds(pos) && view.shows(masks, index, classes, entity, pos);
    if view.known.contains_key(&entity) {
        let kept = shown && view.keep().is_some_and(|k| k.contains(pos));
        return (0, u64::from(!kept && view.despawn(tick, entity)));
    }
    let in_view = shown && view.sees(invisibility) && view.view.is_some_and(|v| v.contains(pos));
    if !in_view {
        return (0, 0);
    }
    if !tracked.contains_key(&entity) {
        let alts = alt_states.get(entity).ok();
        track(tracked, appearances, entity, pos, appearance, optics, alts);
    }
    let interned = view.look_of(&tracked[&entity]);
    view.spawn(
        tick,
        entity,
        pos,
        facing,
        interned,
        blocks_of(block),
        appearances,
    );
    (1, 0)
}

/// Starts tracking an entity on its first replication.
fn track(
    tracked: &mut HashMap<Entity, Tracked>,
    appearances: &mut Appearances,
    entity: Entity,
    pos: TilePos,
    appearance: Option<&Appearance>,
    optics: Option<&Optics>,
    alts: Option<&AltAppearances>,
) {
    let appearance = appearance.map(|a| appearances.acquire(a, optics));
    let alts = intern_alts(appearances, alts, optics);
    tracked.insert(
        entity,
        Tracked {
            pos,
            appearance,
            alts,
        },
    );
}

/// Fills `out` with the clients registered on the chunks of `positions`, without duplicates.
fn collect_viewers(
    chunk_viewers: &HashMap<ChunkPos, Vec<Entity>>,
    out: &mut Vec<Entity>,
    positions: [Option<TilePos>; 2],
) {
    out.clear();
    let viewers = |chunk| chunk_viewers.get(&chunk).map_or(&[][..], Vec::as_slice);
    let [a, b] = positions.map(|p| p.map(ChunkPos::of));
    if let Some(a) = a {
        out.extend_from_slice(viewers(a));
    }
    if let Some(b) = b
        && Some(b) != a
    {
        let before = out.len();
        for &client in viewers(b) {
            if !out[..before].contains(&client) {
                out.push(client);
            }
        }
    }
}

/// Removes `client` from the chunks of `old` that `new` does not cover.
fn unregister_chunks(
    chunk_viewers: &mut HashMap<ChunkPos, Vec<Entity>>,
    client: Entity,
    old: &ViewBox,
    new: Option<&ViewBox>,
) {
    for chunk in old.chunks() {
        if new.is_some_and(|new| new.has_chunk(chunk)) {
            continue;
        }
        if let Some(list) = chunk_viewers.get_mut(&chunk) {
            list.retain(|&c| c != client);
            if list.is_empty() {
                chunk_viewers.remove(&chunk);
            }
        }
    }
}

/// Reads what QUIC sent per client once per window.
fn measure_traffic(
    links: Query<(Entity, &ClientLink)>,
    mut stats: ResMut<ReplicationStats>,
    time: Res<Time<Real>>,
) {
    let stats = &mut *stats;
    for (client, _) in &links {
        stats.clients.entry(client).or_default();
    }
    let now = time.elapsed_secs_f64();
    let window_start = *stats.window_start.get_or_insert(now);
    let window = now - window_start;
    if window < 1.0 {
        return;
    }
    stats.window_start = Some(now);
    stats.clients.retain(|&client, _| links.contains(client));
    for (client, link) in &links {
        let (bytes, packets) = link.sent();
        let traffic = stats.clients.entry(client).or_default();
        traffic.bytes_per_sec = bytes.saturating_sub(traffic.bytes_total) as f64 / window;
        traffic.packets_per_sec = packets.saturating_sub(traffic.packets_total) as f64 / window;
        traffic.bytes_total = bytes;
        traffic.packets_total = packets;
    }

    if now - stats.last_log >= STATS_LOG_INTERVAL_SECS {
        stats.last_log = now;
        let n = stats.clients.len();
        if n > 0 {
            let rates = stats.clients.values().map(|t| t.bytes_per_sec);
            let max = rates.clone().fold(0.0, f64::max);
            let avg = rates.sum::<f64>() / n as f64;
            let runs = stats.replicate_runs.max(1);
            info!(
                "replication: {n} clients, bytes/s per client avg {avg:.0} max {max:.0}; \
                 replicate pass avg {:?} max {:?}; spawned {} despawned {} appearances sent {}",
                stats.replicate_time_sum / runs,
                stats.replicate_time_max,
                stats.spawned_total,
                stats.despawned_total,
                stats.appearances_sent_total,
            );
        }
        stats.replicate_time_sum = Duration::ZERO;
        stats.replicate_time_max = Duration::ZERO;
        stats.replicate_runs = 0;
    }
}

#[cfg(test)]
mod tests {
    use bevy::platform::collections::HashSet;
    use ss13_shared::{Dir, IconStateId, Layer, Overlay};

    use super::*;

    fn bx(min: (i32, i32), max: (i32, i32)) -> ViewBox {
        ViewBox {
            z: 1,
            below: None,
            min,
            max,
        }
    }

    fn tiles(rects: &[((i32, i32), (i32, i32))]) -> HashSet<(i32, i32)> {
        let mut out = HashSet::new();
        for &(min, max) in rects {
            for x in min.0..=max.0 {
                for y in min.1..=max.1 {
                    assert!(out.insert((x, y)), "rects overlap at {x},{y}");
                }
            }
        }
        out
    }

    #[test]
    fn box_difference_is_exact_and_disjoint() {
        let a = bx((0, 0), (10, 10));
        for b in [
            bx((3, -2), (13, 8)),
            bx((2, 2), (5, 5)),
            bx((-5, -5), (20, 20)),
            bx((11, 0), (20, 10)),
            bx((0, 0), (10, 10)),
        ] {
            let got = tiles(&a.minus(Some(&b)));
            let want: HashSet<_> = (0..=10)
                .flat_map(|x| (0..=10).map(move |y| (x, y)))
                .filter(|&(x, y)| !b.contains(TilePos::new(x, y, 1)))
                .collect();
            assert_eq!(got, want, "{b:?}");
        }
        let other_z = ViewBox { z: 2, ..a };
        assert_eq!(a.minus(Some(&other_z)).len(), 1);
    }

    fn look(icon: u32, layer: f32, overlay: Option<u32>) -> Appearance {
        Appearance {
            icon_state: IconStateId(icon),
            layer: Layer(layer),
            plane: 0,
            color: None,
            pixel_offset: [0, 0],
            rotation: 0,
            mouse_opacity: Default::default(),
            glide: 0,
            overlays: overlay
                .map(|icon| Overlay::new(IconStateId(icon)))
                .into_iter()
                .collect(),
        }
    }

    fn entity(i: u32) -> Entity {
        Entity::from_raw_u32(i).unwrap()
    }

    #[test]
    fn interning_reuses_ids_and_evicts_unused_values() {
        let mut table = Appearances::default();
        let a = table.acquire(&look(1, 2.0, None), None);
        assert_eq!(
            table.acquire(&look(1, 2.0, None), None),
            a,
            "equal values share an id"
        );
        let b = table.acquire(&look(1, 2.0, Some(7)), None);
        let c = table.acquire(&look(1, 2.5, None), None);
        assert!(
            a != b && b != c && a != c,
            "any field difference is a new id"
        );
        assert_eq!(table.entries.len(), 3);
        table.release(a);
        assert_eq!(table.entries.len(), 3, "still used once");
        table.release(a);
        assert_eq!(table.entries.len(), 2, "evicted at zero refs");
        let a2 = table.acquire(&look(1, 2.0, None), None);
        assert_ne!(a2, a, "ids are never reused");
        assert_eq!(table.get(a2).appearance, look(1, 2.0, None));
    }

    #[test]
    fn wrapped_counter_skips_live_ids() {
        let mut table = Appearances::default();
        let a = table.acquire(&look(1, 2.0, None), None);
        table.next = u32::MAX; // the next id wraps to 0, then to `a`'s 1
        let zero = table.acquire(&look(2, 2.0, None), None);
        let b = table.acquire(&look(3, 2.0, None), None);
        assert_eq!(zero, AppearanceId(0));
        assert_ne!(b, a, "a live id is not reissued");
        assert_eq!(table.get(a).appearance, look(1, 2.0, None));
        assert_eq!(table.get(b).appearance, look(3, 2.0, None));
        for id in [a, zero, b] {
            table.release(id);
        }
        assert!(table.entries.is_empty() && table.by_value.is_empty());
        assert_eq!(table.acquire(&look(1, 2.0, None), None), AppearanceId(3));
    }

    #[test]
    fn client_cache_sends_each_value_once_and_evicts_at_batch_end() {
        let mut table = Appearances::default();
        let floor = table.acquire(&look(1, 2.0, None), None);
        let wall = table.acquire(&look(2, 2.0, None), None);
        let mut view = ClientView::default();
        let tile = TilePos::new(1, 1, 1);
        for i in 1..=3 {
            view.spawn(1, entity(i), tile, None, Some(floor), None, &table);
        }
        view.spawn(
            1,
            entity(4),
            tile.step(Dir::North),
            None,
            Some(wall),
            None,
            &table,
        );
        assert_eq!(
            view.appearances.len(),
            2,
            "one value per distinct appearance"
        );
        assert_eq!(view.ops.len(), 2, "spawns grouped by tile");
        view.evict_unused();
        view.appearances.clear();
        view.ops.clear();

        // The last floor users leave and a new one arrives in the same pass: no resend.
        for i in 1..=3 {
            view.despawn(2, entity(i));
        }
        view.spawn(2, entity(5), tile, None, Some(floor), None, &table);
        assert!(view.appearances.is_empty());
        assert_eq!(view.ops.len(), 2, "one despawn group, one spawn group");
        view.evict_unused();
        assert_eq!(view.cached.len(), 2);

        // Now it leaves for good: evicted, and sent again on its next use.
        view.despawn(3, entity(5));
        view.evict_unused();
        assert_eq!(view.cached.len(), 1);
        view.spawn(4, entity(6), tile, None, Some(floor), None, &table);
        assert_eq!(view.appearances.len(), 1);
    }

    #[test]
    fn freed_net_ids_wait_before_reuse() {
        let mut view = ClientView::default();
        let table = Appearances::default();
        view.spawn(
            1,
            entity(1),
            TilePos::new(1, 1, 1),
            None,
            None,
            None,
            &table,
        );
        let first = view.known[&entity(1)].id;
        view.despawn(1, entity(1));
        assert_ne!(view.alloc_id(NET_ID_REUSE_TICKS), first);
        assert_eq!(view.alloc_id(1 + NET_ID_REUSE_TICKS), first);
    }
}
