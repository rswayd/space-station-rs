//! Masked views: clients that see only some tiles of their view box, as BYOND's AI eye and
//! camera consoles under camera static (tg `cameranet`: mobs and objects under static are not
//! shown). A masked client gets no entity on a hidden tile (its own controlled entity excepted),
//! and its light blocks carry no light sources of hidden tiles and show them opaque, so a
//! modified client learns nothing about what moves or opens under the static.
//!
//! Game code owns the masks ([`ViewMasks`]: 8x8 chunk bit sets, the layout of tg camera chunks)
//! and puts a [`ViewMask`] on a client's connection entity. Replication follows chunk changes
//! only: each pass walks the tiles whose bit flipped, for the clients using that mask.

use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use ss13_shared::{LIGHT_BLOCK, TilePos};

use crate::replication::{BlockLights, ReplicationOp};

/// Side of a mask chunk (tg `CHUNK_SIZE`).
pub const MASK_CHUNK: i32 = 8;

/// A mask chunk: `(x, y, z)` with `x = tile x div MASK_CHUNK` (Euclidean).
pub type MaskChunk = (i32, i32, u16);

/// The chunk holding `pos` and the bit of `pos` in it (bit `y * 8 + x` of the chunk's tiles).
pub fn mask_chunk(pos: TilePos) -> (MaskChunk, u64) {
    let key = (
        pos.x.div_euclid(MASK_CHUNK),
        pos.y.div_euclid(MASK_CHUNK),
        pos.z,
    );
    let (x, y) = (pos.x.rem_euclid(MASK_CHUNK), pos.y.rem_euclid(MASK_CHUNK));
    (key, 1 << (y * MASK_CHUNK + x))
}

/// Names one mask in [`ViewMasks`]; game code picks the values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MaskId(pub u64);

/// On a client's connection entity next to its `Viewer`: the client sees only the tiles this
/// mask marks visible. A mask that does not exist shows nothing.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewMask(pub MaskId);

/// Visible tiles of every mask, and the chunks that changed since the last replication pass.
#[derive(Resource, Default)]
pub struct ViewMasks {
    masks: HashMap<MaskId, HashMap<MaskChunk, u64>>,
    /// Changed chunks with their bits as of the last pass.
    changed: HashMap<(MaskId, MaskChunk), u64>,
}

impl ViewMasks {
    /// Sets the visible tiles of one chunk.
    pub fn set_chunk(&mut self, mask: MaskId, chunk: MaskChunk, bits: u64) {
        let chunks = self.masks.entry(mask).or_default();
        let old = chunks.get(&chunk).copied().unwrap_or(0);
        if old == bits {
            return;
        }
        if bits == 0 {
            chunks.remove(&chunk);
        } else {
            chunks.insert(chunk, bits);
        }
        self.changed.entry((mask, chunk)).or_insert(old);
    }

    /// Replaces the whole mask with `tiles`.
    pub fn set_tiles(&mut self, mask: MaskId, tiles: impl IntoIterator<Item = TilePos>) {
        let mut new: HashMap<MaskChunk, u64> = HashMap::default();
        for pos in tiles {
            let (key, bit) = mask_chunk(pos);
            *new.entry(key).or_default() |= bit;
        }
        let old: Vec<MaskChunk> = self
            .masks
            .get(&mask)
            .map(|m| m.keys().copied().filter(|k| !new.contains_key(k)).collect())
            .unwrap_or_default();
        for key in old {
            self.set_chunk(mask, key, 0);
        }
        for (key, bits) in new {
            self.set_chunk(mask, key, bits);
        }
    }

    pub fn visible(&self, mask: MaskId, pos: TilePos) -> bool {
        let (key, bit) = mask_chunk(pos);
        self.bits(mask, key) & bit != 0
    }

    pub(crate) fn bits(&self, mask: MaskId, chunk: MaskChunk) -> u64 {
        self.masks
            .get(&mask)
            .and_then(|m| m.get(&chunk))
            .copied()
            .unwrap_or(0)
    }

    /// Changed chunks since the last call: `(mask, chunk, bits before, bits now)`, flipped only.
    pub(crate) fn take_changes(&mut self) -> Vec<(MaskId, MaskChunk, u64)> {
        let changed = std::mem::take(&mut self.changed);
        changed
            .into_iter()
            .filter_map(|((mask, chunk), old)| {
                let flipped = old ^ self.bits(mask, chunk);
                (flipped != 0).then_some((mask, chunk, flipped))
            })
            .collect()
    }

    /// Visibility of the tiles of the light block at `origin`, by block tile index.
    fn block_visibility(&self, mask: MaskId, origin: TilePos) -> [bool; TILES] {
        let mut out = [false; TILES];
        let mut last: Option<(MaskChunk, u64)> = None;
        for (i, seen) in out.iter_mut().enumerate() {
            let pos = block_tile(origin, i);
            let (key, bit) = mask_chunk(pos);
            let bits = match last {
                Some((k, b)) if k == key => b,
                _ => {
                    let b = self.bits(mask, key);
                    last = Some((key, b));
                    b
                }
            };
            *seen = bits & bit != 0;
        }
        out
    }
}

const TILES: usize = (LIGHT_BLOCK * LIGHT_BLOCK) as usize;

fn block_tile(origin: TilePos, i: usize) -> TilePos {
    let i = i as i32;
    TilePos::new(
        origin.x + i % LIGHT_BLOCK,
        origin.y + i / LIGHT_BLOCK,
        origin.z,
    )
}

/// A light op as a client with `mask` may see it: hidden tiles have no lights and are opaque.
/// `None` when nothing of it is left to send.
pub(crate) fn mask_light(
    masks: &ViewMasks,
    mask: MaskId,
    op: ReplicationOp,
) -> Option<ReplicationOp> {
    let opacity = |seen: &[bool; TILES], mut opaque: [u8; 32]| {
        for (i, _) in seen.iter().enumerate().filter(|(_, s)| !**s) {
            opaque[i / 8] |= 1 << (i % 8);
        }
        opaque
    };
    match op {
        ReplicationOp::LightBlock {
            origin,
            opaque,
            lights,
        } => {
            let seen = masks.block_visibility(mask, origin);
            Some(ReplicationOp::LightBlock {
                origin,
                opaque: opacity(&seen, opaque),
                lights: visible_lights(&seen, lights).unwrap_or_default(),
            })
        }
        ReplicationOp::Opacity { origin, opaque } => {
            let seen = masks.block_visibility(mask, origin);
            Some(ReplicationOp::Opacity {
                origin,
                opaque: opacity(&seen, opaque),
            })
        }
        ReplicationOp::Lights { origin, lights } => {
            let seen = masks.block_visibility(mask, origin);
            visible_lights(&seen, lights).map(|lights| ReplicationOp::Lights { origin, lights })
        }
        other => Some(other),
    }
}

/// The entries of `lights` on seen tiles, with a palette of only the values they use.
fn visible_lights(seen: &[bool; TILES], lights: BlockLights) -> Option<BlockLights> {
    let mut out = BlockLights::default();
    for (tile, indexes) in lights.tiles {
        if !seen[usize::from(tile)] {
            continue;
        }
        let indexes = indexes
            .into_iter()
            .map(|i| {
                let light = lights.palette[usize::from(i)];
                let at = out.palette.iter().position(|p| *p == light);
                at.unwrap_or_else(|| {
                    out.palette.push(light);
                    out.palette.len() - 1
                }) as u16
            })
            .collect();
        out.tiles.push((tile, indexes));
    }
    (!out.tiles.is_empty()).then_some(out)
}

#[cfg(test)]
mod tests {
    use ss13_shared::{Dir, LightSource};

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

    #[test]
    fn chunks_record_flips_once_per_pass() {
        let mut masks = ViewMasks::default();
        let m = MaskId(1);
        let a = TilePos::new(3, 3, 1);
        let b = TilePos::new(-1, 9, 1);
        assert!(!masks.visible(m, a));
        masks.set_tiles(m, [a, b]);
        assert!(masks.visible(m, a) && masks.visible(m, b));
        assert!(!masks.visible(m, TilePos::new(3, 4, 1)));
        assert!(!masks.visible(MaskId(2), a), "masks are separate");
        let mut changes = masks.take_changes();
        changes.sort_by_key(|c| c.1);
        assert_eq!(changes.len(), 2);
        assert!(masks.take_changes().is_empty());

        // On and off again within one pass: nothing flipped.
        masks.set_tiles(m, [a]);
        masks.set_tiles(m, [a, b]);
        assert!(masks.take_changes().is_empty());
        masks.set_tiles(m, [b]);
        let changes = masks.take_changes();
        assert_eq!(changes, vec![(m, mask_chunk(a).0, mask_chunk(a).1)]);
        assert!(!masks.visible(m, a));
    }

    #[test]
    fn hidden_tiles_lose_lights_and_turn_opaque() {
        let mut masks = ViewMasks::default();
        let m = MaskId(1);
        let origin = TilePos::new(1, 1, 1);
        // Only the block's tile 0 (1,1) and tile 1 (2,1) are seen.
        masks.set_tiles(m, [TilePos::new(1, 1, 1), TilePos::new(2, 1, 1)]);
        let lights = BlockLights {
            palette: vec![lamp(2.0), lamp(5.0)],
            tiles: vec![(1, vec![1]), (17, vec![0])],
        };
        let mut opaque = [0u8; 32];
        opaque[0] = 0b10; // tile 1 is a wall
        let Some(ReplicationOp::LightBlock {
            opaque: got,
            lights: got_lights,
            ..
        }) = mask_light(
            &masks,
            m,
            ReplicationOp::LightBlock {
                origin,
                opaque,
                lights: lights.clone(),
            },
        )
        else {
            panic!()
        };
        assert_eq!(got[0], 0b1111_1110, "tile 0 stays clear, tile 1 its wall");
        assert!(got[1..].iter().all(|&b| b == 0xff), "hidden tiles opaque");
        assert_eq!(got_lights.tiles, vec![(1, vec![0])]);
        assert_eq!(got_lights.palette, vec![lamp(5.0)], "unused values dropped");
        // A change of hidden lights only sends nothing.
        let hidden = BlockLights {
            palette: vec![lamp(2.0)],
            tiles: vec![(17, vec![0])],
        };
        assert!(
            mask_light(
                &masks,
                m,
                ReplicationOp::Lights {
                    origin,
                    lights: hidden
                }
            )
            .is_none()
        );
    }
}
