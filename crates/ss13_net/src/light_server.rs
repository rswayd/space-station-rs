//! Server side of light replication: light sources and opacity per light block
//! (`ss13_shared::LIGHT_BLOCK` tiles square), streamed from a wider box than entities.
//!
//! A client sees a source's light up to its range beyond the source, and the source's shadows
//! need the opacity of every tile in between, so the client needs light data
//! [`LIGHT_MARGIN`] tiles beyond its view box. Sending entities that far would cost their full
//! state; a block is 32 bytes of opacity bits plus its lights, each distinct light value once.
//!
//! Why blocks streamed by interest, not the whole z-level at connect plus global deltas:
//! - an outage flipping hundreds of lights reaches only the clients near them (whole-level
//!   deltas cost every client of the level every change: 500 clients x 600 lights x ~20 B
//!   = 6 MB per station-wide outage, here each client gets the ~50 lights around it);
//! - a client learns only lights and door states around itself (the whole level would tell
//!   every player which doors open anywhere on the station);
//! - per client the server keeps the list of its ~16 blocks; block state is shared by all.
//!
//! Every `Networked` entity on a level with `Optics` contributes (opaque, light) to its tile's
//! block, kept up to date from the same change detection as entity replication. Changes mark
//! their block dirty; once per tick each dirty block sends its new opacity (32 bytes) and/or the
//! lights of its changed tiles to the block's subscribers.

use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use ss13_shared::{LIGHT_BLOCK, LightSource, Optics, TilePos, light_block};

use crate::replication::{BlockLights, ReplicationOp};

/// Light interest reaches this many tiles beyond the view box: the drawn region's margin past
/// the view radius (1.5 tiles at 19x15 with 2 tiles of glide margin), the longest light reach
/// (8, tg `light_range` of tubes and most fixtures) and one tile for corner rounding.
// shortcut: sources with a reach above 8 pop in at the screen edge; raise it with them (cost:
// one more block row now and then, ~100 B).
pub const LIGHT_MARGIN: u32 = 11;

/// Tiles per block.
const TILES: usize = (LIGHT_BLOCK * LIGHT_BLOCK) as usize;

/// What one entity adds to its tile.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Contribution {
    pos: TilePos,
    opaque: bool,
    light: Option<LightSource>,
}

#[derive(Default)]
struct Block {
    /// Opaque entities per tile.
    opaque: Vec<u16>,
    lights: Vec<(Entity, u8, LightSource)>,
    viewers: Vec<Entity>,
    opacity_dirty: bool,
    dirty_tiles: Vec<u8>,
}

impl Block {
    fn bits(&self) -> [u8; 32] {
        let mut bits = [0; 32];
        for (i, &count) in self.opaque.iter().enumerate() {
            if count > 0 {
                bits[i / 8] |= 1 << (i % 8);
            }
        }
        bits
    }

    /// Lights of `tiles` (each listed, empty when it has none), palette-encoded.
    fn lights_of(&self, tiles: impl IntoIterator<Item = u8>) -> BlockLights {
        let mut out = BlockLights::default();
        for tile in tiles {
            let indexes = self
                .lights
                .iter()
                .filter(|(_, t, _)| *t == tile)
                .map(|(_, _, light)| palette_index(&mut out.palette, light))
                .collect();
            out.tiles.push((tile, indexes));
        }
        out
    }

    fn lit_tiles(&self) -> Vec<u8> {
        let mut tiles: Vec<u8> = self.lights.iter().map(|(_, t, _)| *t).collect();
        tiles.sort_unstable();
        tiles.dedup();
        tiles
    }
}

fn palette_index(palette: &mut Vec<LightSource>, light: &LightSource) -> u16 {
    let i = palette.iter().position(|p| p == light).unwrap_or_else(|| {
        palette.push(*light);
        palette.len() - 1
    });
    // A block has at most a few entities per tile; 65536 distinct values cannot occur.
    i as u16
}

/// Light state of every level, by block, and who subscribes to which block.
#[derive(Default)]
pub(crate) struct LightGrid {
    blocks: HashMap<TilePos, Block>,
    /// The contribution each entity made, to undo it on change.
    contributions: HashMap<Entity, Contribution>,
    /// Blocks with unsent changes.
    dirty: Vec<TilePos>,
    /// Tiles whose opacity flipped since the last [`LightGrid::take_flips`].
    flips: Vec<TilePos>,
}

impl LightGrid {
    /// Brings `entity`'s contribution to its current state: `pos` (`None`: off the levels or
    /// gone) and `optics`.
    pub(crate) fn set(&mut self, entity: Entity, pos: Option<TilePos>, optics: Option<&Optics>) {
        let new = pos.zip(optics).and_then(|(pos, o)| {
            (o.opaque || o.light.is_some()).then_some(Contribution {
                pos,
                opaque: o.opaque,
                light: o.light,
            })
        });
        let old = self.contributions.get(&entity).copied();
        if old == new {
            return;
        }
        if let Some(old) = old {
            self.contributions.remove(&entity);
            self.apply(entity, old, false);
        }
        if let Some(new) = new {
            self.contributions.insert(entity, new);
            self.apply(entity, new, true);
        }
    }

    fn apply(&mut self, entity: Entity, c: Contribution, add: bool) {
        let (origin, index) = light_block(c.pos);
        let block = self.blocks.entry(origin).or_default();
        let mut dirty = false;
        if c.opaque {
            if block.opaque.is_empty() {
                block.opaque = vec![0; TILES];
            }
            let count = &mut block.opaque[index];
            let was = *count > 0;
            *count = if add {
                count.saturating_add(1)
            } else {
                count.saturating_sub(1)
            };
            if was != (*count > 0) {
                block.opacity_dirty = true;
                dirty = true;
                self.flips.push(c.pos);
            }
        }
        if let Some(light) = c.light {
            let tile = index as u8;
            if add {
                block.lights.push((entity, tile, light));
            } else if let Some(i) = block.lights.iter().position(|(e, ..)| *e == entity) {
                block.lights.swap_remove(i);
            }
            if !block.dirty_tiles.contains(&tile) {
                block.dirty_tiles.push(tile);
            }
            dirty = true;
        }
        if dirty && !self.dirty.contains(&origin) {
            self.dirty.push(origin);
        }
    }

    /// Tiles whose opacity flipped since the last call (a tile may repeat).
    pub(crate) fn take_flips(&mut self) -> Vec<TilePos> {
        std::mem::take(&mut self.flips)
    }

    /// Opacity per tile of the block at `origin` (`ss13_shared::light_block` indexes), `None`
    /// for a block with nothing opaque.
    pub(crate) fn block_opacity(&self, origin: TilePos) -> Option<&[u16]> {
        let block = self.blocks.get(&origin)?;
        (!block.opaque.is_empty()).then_some(block.opaque.as_slice())
    }

    /// Adds `client` to the block's subscribers and returns the block's whole state.
    pub(crate) fn subscribe(&mut self, origin: TilePos, client: Entity) -> ReplicationOp {
        let block = self.blocks.entry(origin).or_default();
        if !block.viewers.contains(&client) {
            block.viewers.push(client);
        }
        ReplicationOp::LightBlock {
            origin,
            opaque: block.bits(),
            lights: block.lights_of(block.lit_tiles()),
        }
    }

    pub(crate) fn unsubscribe(&mut self, origin: TilePos, client: Entity) {
        if let Some(block) = self.blocks.get_mut(&origin) {
            block.viewers.retain(|&c| c != client);
        }
    }

    /// The changes of every dirty block since the last call, with the clients to send them to.
    pub(crate) fn take_changes(&mut self, mut send: impl FnMut(&[Entity], ReplicationOp)) {
        for origin in self.dirty.drain(..) {
            let Some(block) = self.blocks.get_mut(&origin) else {
                continue;
            };
            let opacity = std::mem::take(&mut block.opacity_dirty);
            let tiles = std::mem::take(&mut block.dirty_tiles);
            if block.viewers.is_empty() {
                continue;
            }
            if opacity {
                let opaque = block.bits();
                send(&block.viewers, ReplicationOp::Opacity { origin, opaque });
            }
            if !tiles.is_empty() {
                let lights = block.lights_of(tiles);
                send(&block.viewers, ReplicationOp::Lights { origin, lights });
            }
        }
    }

    #[cfg(test)]
    fn sources(&self) -> usize {
        self.blocks.values().map(|b| b.lights.len()).sum()
    }
}

/// Origins of the blocks overlapping the inclusive tile rect `min..=max` on `z`.
pub(crate) fn blocks_in(
    z: u16,
    min: (i32, i32),
    max: (i32, i32),
) -> impl Iterator<Item = TilePos> + use<> {
    let lo = light_block(TilePos::new(min.0, min.1, z)).0;
    let hi = light_block(TilePos::new(max.0, max.1, z)).0;
    let step = LIGHT_BLOCK as usize;
    (lo.y..=hi.y).step_by(step).flat_map(move |y| {
        (lo.x..=hi.x)
            .step_by(step)
            .map(move |x| TilePos::new(x, y, z))
    })
}

/// Whether the block at `origin` overlaps the inclusive rect `min..=max` on `z`.
pub(crate) fn block_overlaps(origin: TilePos, z: u16, min: (i32, i32), max: (i32, i32)) -> bool {
    origin.z == z
        && origin.x <= max.0
        && origin.x + LIGHT_BLOCK > min.0
        && origin.y <= max.1
        && origin.y + LIGHT_BLOCK > min.1
}

#[cfg(test)]
mod tests {
    use ss13_shared::Dir;

    use super::*;

    fn lamp(range: f32) -> LightSource {
        LightSource {
            range,
            power: 1.0,
            color: [255, 255, 255],
            height: 1.0,
            angle: 360,
            dir: Dir::North,
            offset: [0, 0],
        }
    }

    fn optics(opaque: bool, light: Option<LightSource>) -> Optics {
        Optics {
            opaque,
            tile: None,
            light,
        }
    }

    fn e(i: u32) -> Entity {
        Entity::from_raw_u32(i).unwrap()
    }

    fn changes(grid: &mut LightGrid) -> Vec<(Vec<Entity>, ReplicationOp)> {
        let mut out = Vec::new();
        grid.take_changes(|to, op| out.push((to.to_vec(), op)));
        out
    }

    fn bit(bits: &[u8; 32], i: usize) -> bool {
        bits[i / 8] & (1 << (i % 8)) != 0
    }

    #[test]
    fn opacity_counts_entities_and_sends_only_flips() {
        let mut grid = LightGrid::default();
        let client = e(100);
        let wall = TilePos::new(3, 2, 1); // index 16 + 2
        grid.set(e(1), Some(wall), Some(&optics(true, None)));
        let ReplicationOp::LightBlock { origin, opaque, .. } =
            grid.subscribe(TilePos::new(1, 1, 1), client)
        else {
            panic!()
        };
        assert_eq!(origin, TilePos::new(1, 1, 1));
        assert!(bit(&opaque, 18) && opaque.iter().map(|b| b.count_ones()).sum::<u32>() == 1);
        changes(&mut grid); // the add before the subscription

        // A second opaque thing on the tile, then the first goes: no flip, nothing sent.
        grid.set(e(2), Some(wall), Some(&optics(true, None)));
        grid.set(e(1), None, None);
        assert!(changes(&mut grid).is_empty());
        // The last one opens (a door): one opacity op to the subscriber.
        grid.set(e(2), Some(wall), Some(&optics(false, None)));
        let sent = changes(&mut grid);
        assert_eq!(sent.len(), 1);
        let (to, ReplicationOp::Opacity { opaque, .. }) = &sent[0] else {
            panic!("{sent:?}")
        };
        assert_eq!(to, &[client]);
        assert_eq!(opaque, &[0; 32]);
        // Unchanged state: no-op.
        grid.set(e(2), Some(wall), Some(&optics(false, None)));
        assert!(changes(&mut grid).is_empty());
    }

    #[test]
    fn lights_are_palette_encoded_per_tile_and_follow_moves() {
        let mut grid = LightGrid::default();
        let client = e(100);
        let a = TilePos::new(17, 1, 1); // block (17, 1), index 0
        for i in 0..5 {
            grid.set(
                e(i),
                Some(TilePos::new(17 + i as i32, 1, 1)),
                Some(&optics(false, Some(lamp(8.0)))),
            );
        }
        grid.set(e(9), Some(a), Some(&optics(false, Some(lamp(2.0)))));
        let ReplicationOp::LightBlock { lights, .. } =
            grid.subscribe(TilePos::new(17, 1, 1), client)
        else {
            panic!()
        };
        assert_eq!(
            lights.palette,
            vec![lamp(8.0), lamp(2.0)],
            "each value once"
        );
        assert_eq!(lights.tiles[0], (0, vec![0, 1]), "both lights of tile 0");
        assert_eq!(lights.tiles.len(), 5);
        changes(&mut grid);

        // An outage: every light of the block changes in one tick -> one op, one palette entry.
        for i in 0..5 {
            grid.set(
                e(i),
                Some(TilePos::new(17 + i as i32, 1, 1)),
                Some(&optics(false, Some(lamp(2.0)))),
            );
        }
        let sent = changes(&mut grid);
        let [(_, ReplicationOp::Lights { origin, lights })] = &sent[..] else {
            panic!("{sent:?}")
        };
        assert_eq!(*origin, TilePos::new(17, 1, 1));
        assert_eq!(lights.palette, vec![lamp(2.0)]);
        assert_eq!(lights.tiles.len(), 5);

        // A lamp moves into the next block: removed from one, added to the other.
        grid.subscribe(TilePos::new(33, 1, 1), client);
        grid.set(
            e(4),
            Some(TilePos::new(33, 1, 1)),
            Some(&optics(false, Some(lamp(2.0)))),
        );
        let sent = changes(&mut grid);
        assert_eq!(sent.len(), 2);
        assert!(sent.iter().any(|(_, op)| matches!(op,
            ReplicationOp::Lights { origin, lights } if origin.x == 17 && lights.tiles == vec![(4, vec![])])));
        assert_eq!(grid.sources(), 6);
        // Light off and the entity gone: its contribution is undone once.
        grid.set(
            e(4),
            Some(TilePos::new(33, 1, 1)),
            Some(&optics(false, None)),
        );
        grid.set(e(4), None, None);
        assert_eq!(grid.sources(), 5);
    }

    #[test]
    fn unsubscribed_blocks_send_nothing() {
        let mut grid = LightGrid::default();
        let client = e(100);
        let origin = TilePos::new(1, 1, 1);
        grid.subscribe(origin, client);
        grid.unsubscribe(origin, client);
        grid.set(e(1), Some(origin), Some(&optics(true, Some(lamp(3.0)))));
        assert!(changes(&mut grid).is_empty());
    }

    #[test]
    fn block_ranges() {
        let got: Vec<TilePos> = blocks_in(1, (10, 10), (40, 20)).collect();
        assert_eq!(
            got,
            [(1, 1), (17, 1), (33, 1), (1, 17), (17, 17), (33, 17)]
                .map(|(x, y)| TilePos::new(x, y, 1))
        );
        assert!(block_overlaps(
            TilePos::new(17, 1, 1),
            1,
            (32, 16),
            (40, 20)
        ));
        assert!(!block_overlaps(
            TilePos::new(17, 1, 1),
            1,
            (33, 17),
            (40, 20)
        ));
        assert!(!block_overlaps(TilePos::new(17, 1, 1), 2, (0, 0), (40, 20)));
    }
}
