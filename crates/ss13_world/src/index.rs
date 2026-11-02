//! Z-levels and the `TilePos -> entities` spatial index.
//!
//! Coordinates follow BYOND: `x` in `1..=width`, `y` in `1..=height`, so `.dmm` and DM positions
//! map 1:1. The index is kept in sync by `TilePos` component hooks (see `WorldPlugin`), never by
//! scanning.

use bevy::ecs::lifecycle::HookContext;
use bevy::ecs::world::DeferredWorld;
use bevy::prelude::*;
use ss13_shared::{Dir, TilePos};

use crate::movement::{Border, Density};

/// Chunk side in tiles. 16 keeps a radius-9 view (19x19 tiles) within 4 chunks at most
/// (2x2 when the view box is chunk-aligned), while one chunk's 256 tile lists stay small enough
/// to walk whole for chunk-level interest diffs.
pub const CHUNK_SIZE: i32 = 16;
const CHUNK_TILES: usize = (CHUNK_SIZE * CHUNK_SIZE) as usize;

/// A chunk on a z-level, in chunk coordinates (tile `(x - 1) / CHUNK_SIZE`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ChunkPos {
    pub x: i32,
    pub y: i32,
    pub z: u16,
}

impl ChunkPos {
    pub fn of(pos: TilePos) -> Self {
        Self {
            x: (pos.x - 1).div_euclid(CHUNK_SIZE),
            y: (pos.y - 1).div_euclid(CHUNK_SIZE),
            z: pos.z,
        }
    }
}

#[derive(Debug)]
pub enum LevelError {
    AlreadyExists(u16),
    EmptyBounds,
}

impl std::fmt::Display for LevelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyExists(z) => write!(f, "z-level {z} already exists"),
            Self::EmptyBounds => write!(f, "z-level width and height must be at least 1"),
        }
    }
}

impl std::error::Error for LevelError {}

struct Level {
    width: i32,
    height: i32,
    chunks_x: i32,
    chunks_y: i32,
    /// Chunk-major: all 256 tiles of a chunk are contiguous, so view queries touch few cache lines
    /// of `Vec` headers. Index = chunk_index * CHUNK_TILES + local_y * CHUNK_SIZE + local_x.
    tiles: Vec<Tile>,
}

#[derive(Default)]
struct Tile {
    entities: Vec<Entity>,
    /// Subset of `entities` with `Density` and no [`Border`]: full-tile blockers. Kept apart so
    /// blocking checks never touch other entities' storage (random entity lookups were the
    /// dominant cost of a step).
    dense: Vec<Entity>,
    /// Subset of `entities` with `Density` and [`Border`], with its edges.
    border: Vec<(Entity, u8)>,
}

impl Level {
    fn tile_index(&self, x: i32, y: i32) -> Option<usize> {
        if x < 1 || y < 1 || x > self.width || y > self.height {
            return None;
        }
        let (x0, y0) = (x - 1, y - 1);
        let chunk = (y0 / CHUNK_SIZE) * self.chunks_x + x0 / CHUNK_SIZE;
        let local = (y0 % CHUNK_SIZE) * CHUNK_SIZE + x0 % CHUNK_SIZE;
        Some(chunk as usize * CHUNK_TILES + local as usize)
    }
}

/// Z-levels with their bounds and the per-tile entity lists.
///
/// Entities whose `TilePos` is outside every level are not indexed: create a level before
/// spawning anything on it.
#[derive(Resource, Default)]
pub struct SpatialIndex {
    levels: Vec<Option<Level>>,
}

impl SpatialIndex {
    pub fn create_level(&mut self, z: u16, width: u32, height: u32) -> Result<(), LevelError> {
        if width == 0 || height == 0 {
            return Err(LevelError::EmptyBounds);
        }
        let zi = usize::from(z);
        if self.levels.len() <= zi {
            self.levels.resize_with(zi + 1, || None);
        }
        if self.levels[zi].is_some() {
            return Err(LevelError::AlreadyExists(z));
        }
        let (width, height) = (width as i32, height as i32);
        let chunks_x = (width + CHUNK_SIZE - 1) / CHUNK_SIZE;
        let chunks_y = (height + CHUNK_SIZE - 1) / CHUNK_SIZE;
        let mut tiles = Vec::new();
        tiles.resize_with((chunks_x * chunks_y) as usize * CHUNK_TILES, Tile::default);
        self.levels[zi] = Some(Level {
            width,
            height,
            chunks_x,
            chunks_y,
            tiles,
        });
        Ok(())
    }

    /// `(width, height)` of level `z`, if it exists.
    pub fn level_size(&self, z: u16) -> Option<(u32, u32)> {
        self.level(z).map(|l| (l.width as u32, l.height as u32))
    }

    pub fn in_bounds(&self, pos: TilePos) -> bool {
        self.level(pos.z)
            .is_some_and(|l| l.tile_index(pos.x, pos.y).is_some())
    }

    /// Entities on one tile, in no particular order.
    pub fn on_tile(&self, pos: TilePos) -> &[Entity] {
        self.tile(pos).map_or(&[], |t| t.entities.as_slice())
    }

    /// Full-tile blockers on one tile: entities with `Density` and no [`Border`] (tg
    /// `is_blocked_turf` also skips border objects).
    pub fn dense_on_tile(&self, pos: TilePos) -> &[Entity] {
        self.tile(pos).map_or(&[], |t| t.dense.as_slice())
    }

    /// Dense border objects on one tile, with the edges they block.
    pub fn border_on_tile(&self, pos: TilePos) -> &[(Entity, u8)] {
        self.tile(pos).map_or(&[], |t| t.border.as_slice())
    }

    /// tg `turf/Exit`: dense border objects on `from` that refuse `mover` leaving through
    /// its `dir` edge.
    pub fn exit_blockers(
        &self,
        from: TilePos,
        dir: Dir,
        mover: Entity,
    ) -> impl Iterator<Item = Entity> + '_ {
        self.border_on_tile(from)
            .iter()
            .filter(move |&&(e, edges)| e != mover && Border { edges }.blocks(dir))
            .map(|&(e, _)| e)
    }

    /// tg `turf/Enter` for a step in `dir` onto `to`: its full-tile blockers and the dense
    /// border objects facing the side the mover comes from.
    pub fn enter_blockers(
        &self,
        to: TilePos,
        dir: Dir,
        mover: Entity,
    ) -> impl Iterator<Item = Entity> + '_ {
        let side = dir.reverse();
        let border = self
            .border_on_tile(to)
            .iter()
            .filter(move |&&(_, edges)| Border { edges }.blocks(side))
            .map(|&(e, _)| e);
        self.dense_on_tile(to)
            .iter()
            .copied()
            .chain(border)
            .filter(move |&e| e != mover)
    }

    /// BYOND `turf/Adjacent` for clicks: the same tile; an orthogonal neighbor when no border
    /// object on either tile faces the shared edge; a diagonal neighbor through one of the two
    /// tiles beside both, which must have no full-tile blocker and whose edges on the way must
    /// be free (`ClickCross`). `ignore`: dense entities that do not block a reach (tg only checks
    /// objects and turfs, not mobs, and skips the target itself).
    // `LETPASSTHROW`/`LETPASSCLICKS` objects (tables) are the caller's `ignore` (`ss13_game`).
    // shortcut: diagonal border objects block nothing (tg treats them as full-tile for clicks);
    // add with those objects.
    pub fn adjacent(&self, from: TilePos, to: TilePos, ignore: impl Fn(Entity) -> bool) -> bool {
        let crosses = |tile: TilePos, d: Dir| {
            !self
                .border_on_tile(tile)
                .iter()
                .any(|&(e, edges)| Border { edges }.blocks(d) && !ignore(e))
        };
        let dir = match from.chebyshev(to) {
            Some(0) => return true,
            Some(1) => dir_of(to.x - from.x, to.y - from.y),
            _ => return false,
        };
        let (dx, dy) = dir.offset();
        if dx == 0 || dy == 0 {
            return crosses(from, dir) && crosses(to, dir.reverse());
        }
        let v = dir_of(0, dy);
        let h = dir_of(dx, 0);
        [(v, h), (h, v)].into_iter().any(|(first, second)| {
            let side = from.step(first);
            self.in_bounds(side)
                && crosses(from, first)
                && self.dense_on_tile(side).iter().all(|&e| ignore(e))
                && crosses(side, first.reverse())
                && crosses(side, second)
                && crosses(to, second.reverse())
        })
    }

    /// Entities in the inclusive rectangle `min..=max` on level `z`, clipped to the level.
    /// Walks chunk by chunk so each chunk's tile lists are read contiguously.
    pub fn in_rect(
        &self,
        z: u16,
        min: (i32, i32),
        max: (i32, i32),
    ) -> impl Iterator<Item = Entity> + '_ {
        self.rect_tiles(z, min, max)
            .flat_map(|tile| tile.entities.iter().copied())
    }

    /// Entities within Chebyshev distance `radius` of `center` (BYOND `range`/view box).
    pub fn in_radius(&self, center: TilePos, radius: u32) -> impl Iterator<Item = Entity> + '_ {
        let r = radius.min(i32::MAX as u32) as i32;
        self.in_rect(
            center.z,
            (center.x.saturating_sub(r), center.y.saturating_sub(r)),
            (center.x.saturating_add(r), center.y.saturating_add(r)),
        )
    }

    /// Chunks that overlap the Chebyshev box of `radius` around `center`, clipped to the level.
    pub fn chunks_in_radius(
        &self,
        center: TilePos,
        radius: u32,
    ) -> impl Iterator<Item = ChunkPos> + use<> {
        let r = radius.min(i32::MAX as u32) as i32;
        let clip = self.level(center.z).map(|l| {
            let lo = ChunkPos::of(TilePos::new(
                center.x.saturating_sub(r).max(1),
                center.y.saturating_sub(r).max(1),
                center.z,
            ));
            let hi = ChunkPos::of(TilePos::new(
                center.x.saturating_add(r).min(l.width),
                center.y.saturating_add(r).min(l.height),
                center.z,
            ));
            (lo, hi)
        });
        let z = center.z;
        clip.into_iter()
            .filter(|(lo, hi)| lo.x <= hi.x && lo.y <= hi.y)
            .flat_map(move |(lo, hi)| {
                (lo.y..=hi.y).flat_map(move |y| (lo.x..=hi.x).map(move |x| ChunkPos { x, y, z }))
            })
    }

    /// All entities in one chunk.
    pub fn in_chunk(&self, chunk: ChunkPos) -> impl Iterator<Item = Entity> + '_ {
        let tiles: &[Tile] = self
            .level(chunk.z)
            .filter(|l| {
                chunk.x >= 0 && chunk.y >= 0 && chunk.x < l.chunks_x && chunk.y < l.chunks_y
            })
            .map(|l| {
                let start = (chunk.y * l.chunks_x + chunk.x) as usize * CHUNK_TILES;
                &l.tiles[start..start + CHUNK_TILES]
            })
            .unwrap_or(&[]);
        tiles.iter().flat_map(|t| t.entities.iter().copied())
    }

    fn level(&self, z: u16) -> Option<&Level> {
        self.levels.get(usize::from(z)).and_then(Option::as_ref)
    }

    fn tile(&self, pos: TilePos) -> Option<&Tile> {
        let level = self.level(pos.z)?;
        level.tile_index(pos.x, pos.y).map(|i| &level.tiles[i])
    }

    fn tile_mut(&mut self, pos: TilePos) -> Option<&mut Tile> {
        let level = self.levels.get_mut(usize::from(pos.z))?.as_mut()?;
        level.tile_index(pos.x, pos.y).map(|i| &mut level.tiles[i])
    }

    fn rect_tiles(
        &self,
        z: u16,
        min: (i32, i32),
        max: (i32, i32),
    ) -> impl Iterator<Item = &Tile> + '_ {
        let level = self.level(z);
        let bounds = level.map(|l| {
            (
                min.0.max(1),
                min.1.max(1),
                max.0.min(l.width),
                max.1.min(l.height),
            )
        });
        level
            .zip(bounds)
            .filter(|(_, (x0, y0, x1, y1))| x0 <= x1 && y0 <= y1)
            .into_iter()
            .flat_map(|(l, (x0, y0, x1, y1))| {
                let (cx0, cy0) = ((x0 - 1) / CHUNK_SIZE, (y0 - 1) / CHUNK_SIZE);
                let (cx1, cy1) = ((x1 - 1) / CHUNK_SIZE, (y1 - 1) / CHUNK_SIZE);
                (cy0..=cy1).flat_map(move |cy| {
                    (cx0..=cx1).flat_map(move |cx| {
                        // Clip the rect to this chunk, then walk its rows.
                        let tx0 = x0.max(cx * CHUNK_SIZE + 1);
                        let tx1 = x1.min((cx + 1) * CHUNK_SIZE);
                        let ty0 = y0.max(cy * CHUNK_SIZE + 1);
                        let ty1 = y1.min((cy + 1) * CHUNK_SIZE);
                        (ty0..=ty1).flat_map(move |y| {
                            let row = l.tile_index(tx0, y).expect("clipped to level");
                            l.tiles[row..=row + (tx1 - tx0) as usize].iter()
                        })
                    })
                })
            })
    }

    fn insert(&mut self, entity: Entity, pos: TilePos, dense: bool, border: Option<Border>) {
        let Some(tile) = self.tile_mut(pos) else {
            warn!("{entity} has TilePos {pos:?} outside every z-level; not indexed");
            return;
        };
        tile.entities.push(entity);
        if dense {
            tile.push_blocker(entity, border);
        }
    }

    fn remove(&mut self, entity: Entity, pos: TilePos) {
        if let Some(tile) = self.tile_mut(pos) {
            swap_remove_item(&mut tile.entities, entity);
            tile.remove_blocker(entity);
        }
    }

    /// Files `entity` as a blocker of the right kind, or as none. Idempotent, and a no-op for
    /// entities no longer on the tile, so despawn (which discards `Density`, `Border` and
    /// `TilePos` in any order while all are still present) cannot leave a stale entry.
    fn set_blocker(&mut self, entity: Entity, pos: TilePos, dense: bool, border: Option<Border>) {
        let Some(tile) = self.tile_mut(pos) else {
            return;
        };
        tile.remove_blocker(entity);
        if dense && tile.entities.contains(&entity) {
            tile.push_blocker(entity, border);
        }
    }
}

impl Tile {
    fn push_blocker(&mut self, entity: Entity, border: Option<Border>) {
        match border {
            Some(b) => self.border.push((entity, b.edges)),
            None => self.dense.push(entity),
        }
    }

    fn remove_blocker(&mut self, entity: Entity) {
        swap_remove_item(&mut self.dense, entity);
        if let Some(at) = self.border.iter().position(|&(e, _)| e == entity) {
            self.border.swap_remove(at);
        }
    }
}

/// The direction of a unit offset (`(0, 0)` gives south; callers never pass it).
fn dir_of(dx: i32, dy: i32) -> Dir {
    Dir::DMI_ORDER
        .into_iter()
        .find(|d| d.offset() == (dx.signum(), dy.signum()))
        .unwrap_or_default()
}

fn swap_remove_item(list: &mut Vec<Entity>, entity: Entity) {
    if let Some(at) = list.iter().position(|&e| e == entity) {
        list.swap_remove(at);
    }
}

pub(crate) fn on_tile_pos_insert(mut world: DeferredWorld, ctx: HookContext) {
    let entity = world.entity(ctx.entity);
    let pos = *entity
        .get::<TilePos>()
        .expect("insert hook runs with the component present");
    let dense = entity.contains::<Density>();
    let border = entity.get::<Border>().copied();
    world
        .resource_mut::<SpatialIndex>()
        .insert(ctx.entity, pos, dense, border);
}

/// Runs on replace, remove and despawn, with the old value still readable.
pub(crate) fn on_tile_pos_discard(mut world: DeferredWorld, ctx: HookContext) {
    let pos = *world
        .get::<TilePos>(ctx.entity)
        .expect("discard hook runs before the component is dropped");
    world.resource_mut::<SpatialIndex>().remove(ctx.entity, pos);
}

pub(crate) fn on_density_insert(world: DeferredWorld, ctx: HookContext) {
    let border = world.get::<Border>(ctx.entity).copied();
    set_blocker(world, ctx, true, border);
}

pub(crate) fn on_density_discard(world: DeferredWorld, ctx: HookContext) {
    set_blocker(world, ctx, false, None);
}

pub(crate) fn on_border_insert(world: DeferredWorld, ctx: HookContext) {
    let dense = world.entity(ctx.entity).contains::<Density>();
    let border = world.get::<Border>(ctx.entity).copied();
    set_blocker(world, ctx, dense, border);
}

/// Also runs before a replacing insert, which then files the entity again.
pub(crate) fn on_border_discard(world: DeferredWorld, ctx: HookContext) {
    let dense = world.entity(ctx.entity).contains::<Density>();
    set_blocker(world, ctx, dense, None);
}

fn set_blocker(mut world: DeferredWorld, ctx: HookContext, dense: bool, border: Option<Border>) {
    if let Some(&pos) = world.get::<TilePos>(ctx.entity) {
        world
            .resource_mut::<SpatialIndex>()
            .set_blocker(ctx.entity, pos, dense, border);
    }
}
