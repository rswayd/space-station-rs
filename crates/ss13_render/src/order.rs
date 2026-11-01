//! Draw order. Bevy 2D sorts transparent sprites only by `GlobalTransform` z (stable sort),
//! so `(plane, layer, tiebreak, overlay index)` is packed into an integer z that `f32`
//! holds exactly (< 2^24):
//!
//! `z = rank(plane, layer) * 2^14 + (entity index % 512) * 32 + overlay slot`
//!
//! `rank` is the position of `(plane, layer)` among all pairs seen so far, so arbitrary
//! BYOND layers (`2.01`, `4.11`, ...) keep their order without quantization.

use bevy::prelude::*;

const RANKS: u32 = 1 << 10;
const TIEBREAKS: u32 = 1 << 9;
/// Slot 0 is the base sprite, overlays take `1..OVERLAY_SLOTS`.
pub const OVERLAY_SLOTS: u32 = 1 << 5;
/// Exclusive upper bound of every z produced here; the camera's depth range covers it.
pub const Z_RANGE: f32 = (RANKS * TIEBREAKS * OVERLAY_SLOTS) as f32;

#[derive(Resource, Default, Debug)]
pub struct DrawOrder {
    /// Sorted, distinct `(plane, layer)` pairs.
    keys: Vec<(i16, f32)>,
}

fn cmp(a: &(i16, f32), b: &(i16, f32)) -> std::cmp::Ordering {
    a.0.cmp(&b.0).then(a.1.total_cmp(&b.1))
}

impl DrawOrder {
    /// Registers a pair; `true` when it is new, which shifts the ranks after it,
    /// so every z must be recomputed (rare: new layer values stop appearing after map load).
    pub fn insert(&mut self, plane: i16, layer: f32) -> bool {
        match self.keys.binary_search_by(|k| cmp(k, &(plane, layer))) {
            Ok(_) => false,
            Err(i) => {
                self.keys.insert(i, (plane, layer));
                if self.keys.len() as u32 == RANKS + 1 {
                    warn!(
                        "more than {RANKS} distinct (plane, layer) pairs; the top ones share a rank"
                    );
                }
                true
            }
        }
    }

    fn rank(&self, plane: i16, layer: f32) -> u32 {
        let i = match self.keys.binary_search_by(|k| cmp(k, &(plane, layer))) {
            Ok(i) | Err(i) => i as u32,
        };
        // shortcut: ranks above 1023 collapse; widen z packing (or sort ourselves) if a codebase needs more.
        i.min(RANKS - 1)
    }

    /// z between the sprites of planes below `plane` and those of `plane` and above: where a
    /// full-screen plane effect (lighting) is drawn.
    pub fn plane_z(&self, plane: i16) -> f32 {
        let rank = self.keys.partition_point(|k| k.0 < plane) as u32;
        (rank.min(RANKS) * TIEBREAKS * OVERLAY_SLOTS) as f32 - 0.5
    }

    /// z of a base sprite. `tiebreak` (the entity index) keeps equal layers in a fixed order;
    /// two sprites collide only with the same rank and the same index modulo 512.
    pub fn z(&self, plane: i16, layer: f32, tiebreak: u32) -> f32 {
        let rank = self.rank(plane, layer);
        ((rank * TIEBREAKS + tiebreak % TIEBREAKS) * OVERLAY_SLOTS) as f32
    }
}

/// Local z of overlay `i` relative to its base sprite: just above the base, in list order.
pub fn overlay_z(i: usize) -> f32 {
    (i as u32 + 1).min(OVERLAY_SLOTS - 1) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plane_then_layer_then_tiebreak_then_overlay() {
        let mut o = DrawOrder::default();
        let pairs = [
            (0, 4.11),
            (-2, 10.0),
            (0, 2.0),
            (0, 2.01),
            (5, 1.0),
            (0, 4.1),
        ];
        for (p, l) in pairs {
            assert!(o.insert(p, l));
        }
        assert!(!o.insert(0, 2.0));
        let mut sorted = pairs.to_vec();
        sorted.sort_by(|a, b| o.z(a.0, a.1, 0).total_cmp(&o.z(b.0, b.1, 0)));
        assert_eq!(
            sorted,
            vec![
                (-2, 10.0),
                (0, 2.0),
                (0, 2.01),
                (0, 4.1),
                (0, 4.11),
                (5, 1.0)
            ]
        );

        // Tiebreak and overlays never cross into the next rank.
        let top_of_rank = o.z(0, 2.0, TIEBREAKS - 1) + overlay_z(1000);
        assert!(top_of_rank < o.z(0, 2.01, 0));
        assert!(o.z(0, 2.0, 7) < o.z(0, 2.0, 8));
        assert!(o.z(0, 2.0, 7) + overlay_z(0) < o.z(0, 2.0, 7) + overlay_z(1));
    }

    #[test]
    fn plane_z_splits_planes() {
        let mut o = DrawOrder::default();
        for (p, l) in [(-6, 2.0), (-4, 4.1), (10, 1.0), (25, 3.0)] {
            o.insert(p, l);
        }
        let z = o.plane_z(10);
        assert!(o.z(-4, 4.1, TIEBREAKS - 1) + overlay_z(1000) < z);
        assert!(z < o.z(10, 1.0, 0));
    }

    #[test]
    fn z_is_exact_in_f32_and_in_range() {
        let mut o = DrawOrder::default();
        for i in 0..2000 {
            o.insert(0, i as f32);
        }
        let max = o.z(0, 1999.0, u32::MAX) + overlay_z(usize::MAX >> 40);
        assert!(max < Z_RANGE);
        assert_eq!(max as u32 as f32, max);
        assert_eq!(max + 1.0 - 1.0, max);
    }
}
