//! The blast wave's arithmetic, after tg `code/controllers/subsystem/explosions.dm`
//! (`prepare_explosion_turfs`, `propagate_blastwave`, `dyn_explosion`, the bomb cap) and the
//! `get_angle`/`get_ranged_target_turf_direct` helpers. No world access: the server half
//! ([`super`]) feeds it tiles and blocks.

use ss13_shared::TilePos;

/// `EXPLOSION_BLOCK_DEV`, `EXPLOSION_BLOCK_HEAVY`, `EXPLOSION_BLOCK_LIGHT`: how many tiles of
/// range one point of `explosive_resistance` takes from each zone.
pub const BLOCK_DEV: f32 = 1.0;
pub const BLOCK_HEAVY: f32 = 1.5;
pub const BLOCK_LIGHT: f32 = 2.5;
/// `GLOB.DYN_EX_SCALE`.
pub const DYN_EX_SCALE: f32 = 0.5;

/// tg `EXPLODE_LIGHT` 1, `EXPLODE_HEAVY` 2, `EXPLODE_DEVASTATE` 3.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    Light = 1,
    Heavy = 2,
    Devastate = 3,
}

impl Severity {
    /// One step milder (`severity--`); `None` below light.
    pub fn milder(self) -> Option<Self> {
        match self {
            Self::Devastate => Some(Self::Heavy),
            Self::Heavy => Some(Self::Light),
            Self::Light => None,
        }
    }

    /// One step worse, capped at devastation (tg `/obj/machinery/door/ex_act`).
    pub fn worse(self) -> Self {
        match self {
            Self::Light => Self::Heavy,
            Self::Heavy | Self::Devastate => Self::Devastate,
        }
    }
}

/// The three impact ranges (tiles).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ranges {
    pub devastation: f32,
    pub heavy: f32,
    pub light: f32,
}

/// tg `GLOB.MAX_EX_*` from `CONFIG_GET(number/bombcap)`: devastation `round(cap / 4)`, heavy
/// `round(cap / 2)`, light, flash and flame `cap` (DM one-argument `round` is `floor`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BombCap {
    pub devastation: f32,
    pub heavy: f32,
    pub light: f32,
    pub flash: f32,
    pub flame: f32,
}

impl BombCap {
    /// `bombcap` has `min_val = 4`.
    pub fn new(bombcap: f32) -> Self {
        let cap = bombcap.max(4.0);
        Self {
            devastation: (cap / 4.0).floor(),
            heavy: (cap / 2.0).floor(),
            light: cap,
            flash: cap,
            flame: cap,
        }
    }
}

/// tg `dyn_explosion` ranges for `power`: `range = round((2 * power) ** DYN_EX_SCALE)`,
/// devastation a quarter of it and heavy a half (floored).
pub fn dyn_ranges(power: f32) -> (Ranges, f32) {
    let range = (2.0 * power).max(0.0).powf(DYN_EX_SCALE).floor();
    (
        Ranges {
            devastation: (range * 0.25).floor(),
            heavy: (range * 0.5).floor(),
            light: range,
        },
        range,
    )
}

/// The severity at `dist` tiles behind `block` resistance (`EXPLODE_NONE`: `None`).
pub fn severity(dist: f32, block: f32, r: Ranges) -> Option<Severity> {
    if dist + block * BLOCK_DEV < r.devastation {
        Some(Severity::Devastate)
    } else if dist + block * BLOCK_HEAVY < r.heavy {
        Some(Severity::Heavy)
    } else if dist + block * BLOCK_LIGHT < r.light {
        Some(Severity::Light)
    } else {
        None
    }
}

/// `CHEAP_HYPOTENUSE`.
pub fn hypotenuse(dx: i32, dy: i32) -> f32 {
    ((dx * dx + dy * dy) as f32).sqrt()
}

/// tg `delta_to_angle`: degrees clockwise from north.
pub fn delta_to_angle(dx: f32, dy: f32) -> f32 {
    if dy == 0.0 {
        return if dx >= 0.0 { 90.0 } else { 270.0 };
    }
    let mut a = (dx / dy).atan().to_degrees();
    if dy < 0.0 {
        a += 180.0;
    } else if dx < 0.0 {
        a += 360.0;
    }
    a
}

/// DM `WRAP(val, min, max)`.
fn wrap(v: f32, min: f32, max: f32) -> f32 {
    let d = max - min;
    let r = v - d * ((v - min) / d).floor();
    if r == max { min } else { r }
}

/// tg `prepare_explosion_turfs`: the epicenter (unless protected), then the square rings out
/// to `range` in tg's order (each ring's top row, right column, bottom row, left column, as
/// BYOND `block()` lists them), clipped to the `max_x` x `max_y` map; a directional blast keeps
/// the tiles whose `get_angle` from the epicenter falls in its arc. Every tile comes after the
/// tile one step closer to the epicenter, which the block cache relies on.
pub fn prepare_turfs(
    range: i32,
    center: TilePos,
    protect_epicenter: bool,
    direction: f32,
    arc: f32,
    max_x: i32,
    max_y: i32,
) -> Vec<TilePos> {
    let mut out = Vec::new();
    if !protect_epicenter {
        out.push(center);
    }
    let first = wrap(direction - arc * 0.5, 0.0, 360.0);
    let second = wrap(direction + arc * 0.5, 0.0, 360.0);
    // `(lower, upper, reverse)` of a directional arc; `None`: the full circle.
    let arc_limits = if first == second {
        None
    } else if first < second {
        Some((first, second, false))
    } else {
        Some((second, first, true))
    };
    let (x0, y0, z) = (center.x, center.y, center.z);
    let in_x = |x: i32| (1..=max_x).contains(&x);
    let in_y = |y: i32| (1..=max_y).contains(&y);
    let mut push = |x: i32, y: i32| {
        if !in_x(x) || !in_y(y) {
            return;
        }
        if let Some((lower, upper, reverse)) = arc_limits {
            let angle = delta_to_angle((x - x0) as f32, (y - y0) as f32);
            if ((lower..=upper).contains(&angle)) == reverse {
                return;
            }
        }
        out.push(TilePos::new(x, y, z));
    };
    for i in 1..=range.max(0) {
        let (lx, ly, hx, hy) = (x0 - i, y0 - i, x0 + i, y0 + i);
        if hy <= max_y {
            for x in lx..hx {
                push(x, hy);
            }
        }
        if hx <= max_x {
            for y in ly + 1..=hy {
                push(hx, y);
            }
        }
        if ly >= 1 {
            for x in lx + 1..=hx {
                push(x, ly);
            }
        }
        if lx >= 1 {
            for y in ly..hy {
                push(lx, y);
            }
        }
    }
    out
}

/// BYOND `get_step_towards(tile, center)`: one step, diagonal unless aligned.
pub fn step_towards(tile: TilePos, center: TilePos) -> TilePos {
    TilePos::new(
        tile.x - (tile.x - center.x).signum(),
        tile.y - (tile.y - center.y).signum(),
        tile.z,
    )
}

/// tg `get_ranged_target_turf_direct(start, target, range, offset)`: the last map tile on the
/// line from `start` at `ATAN2` towards `target` plus `offset` degrees (counter-clockwise from
/// east), up to `range` tiles.
pub fn ranged_target_direct(
    start: TilePos,
    target: TilePos,
    range: i32,
    offset: f32,
    in_bounds: impl Fn(TilePos) -> bool,
) -> TilePos {
    let (dx, dy) = ((target.x - start.x) as f32, (target.y - start.y) as f32);
    // tg `ATAN2(x, y)`: 0 for a zero vector.
    let mut angle = if dx == 0.0 && dy == 0.0 {
        0.0
    } else {
        dy.atan2(dx).to_degrees()
    };
    angle += offset;
    let (sin, cos) = angle.to_radians().sin_cos();
    let mut at = start;
    for i in 1..=range {
        // BYOND `locate()` drops the fraction of its coordinates.
        let check = TilePos::new(
            (start.x as f32 + cos * i as f32).trunc() as i32,
            (start.y as f32 + sin * i as f32).trunc() as i32,
            start.z,
        );
        if !in_bounds(check) {
            break;
        }
        at = check;
    }
    at
}

#[cfg(test)]
mod tests {
    use super::*;

    const R: Ranges = Ranges {
        devastation: 1.0,
        heavy: 3.0,
        light: 6.0,
    };

    #[test]
    fn severity_by_distance_and_block() {
        assert_eq!(severity(0.0, 0.0, R), Some(Severity::Devastate));
        assert_eq!(severity(2.0, 0.0, R), Some(Severity::Heavy));
        assert_eq!(severity(5.9, 0.0, R), Some(Severity::Light));
        assert_eq!(severity(6.0, 0.0, R), None);
        // A wall (1) behind the epicenter: heavy needs dist + 1.5 < 3, light dist + 2.5 < 6.
        assert_eq!(severity(1.0, 1.0, R), Some(Severity::Heavy));
        assert_eq!(severity(2.0, 1.0, R), Some(Severity::Light));
        assert_eq!(severity(3.5, 1.0, R), None);
        // Even the epicenter is spared behind an infinite block.
        assert_eq!(severity(0.0, 1e31, R), None);
    }

    #[test]
    fn bombcap_and_dyn_ranges_match_tg() {
        let cap = BombCap::new(14.0);
        assert_eq!((cap.devastation, cap.heavy, cap.light), (3.0, 7.0, 14.0));
        let cap = BombCap::new(20.0);
        assert_eq!((cap.devastation, cap.heavy, cap.light), (5.0, 10.0, 20.0));
        assert_eq!(BombCap::new(1.0).light, 4.0);
        // `sqrt(2 * power)` floored, then a quarter and a half.
        let (r, range) = dyn_ranges(100.0);
        assert_eq!(
            (r.devastation, r.heavy, r.light, range),
            (3.0, 7.0, 14.0, 14.0)
        );
        let (r, _) = dyn_ranges(5.0);
        assert_eq!((r.devastation, r.heavy, r.light), (0.0, 1.0, 3.0));
        let (r, _) = dyn_ranges(1.0);
        assert_eq!((r.devastation, r.heavy, r.light), (0.0, 0.0, 1.0));
    }

    #[test]
    fn rings_spiral_out_and_stay_on_the_map() {
        let c = TilePos::new(10, 10, 1);
        let tiles = prepare_turfs(2, c, false, 0.0, 360.0, 100, 100);
        assert_eq!(tiles.len(), 25);
        assert_eq!(tiles[0], c);
        // Ring 1 starts at its top-left corner, as tg's first `block()`.
        assert_eq!(tiles[1], TilePos::new(9, 11, 1));
        let unique: std::collections::HashSet<_> = tiles.iter().collect();
        assert_eq!(unique.len(), 25);
        // Each tile's step towards the epicenter came earlier.
        for (i, &t) in tiles.iter().enumerate().skip(1) {
            let prev = step_towards(t, c);
            assert!(tiles[..i].contains(&prev), "{t:?} before {prev:?}");
        }
        // A corner epicenter keeps only the map's tiles; a protected one leaves itself out.
        let corner = prepare_turfs(2, TilePos::new(1, 1, 1), true, 0.0, 360.0, 100, 100);
        assert_eq!(corner.len(), 8);
        assert!(corner.iter().all(|t| t.x >= 1 && t.y >= 1));
    }

    #[test]
    fn a_directional_blast_keeps_its_arc() {
        let c = TilePos::new(10, 10, 1);
        // East, 90 degrees wide: (11,10) is in, (9,10) and (10,11) are out.
        let tiles = prepare_turfs(3, c, false, 90.0, 90.0, 100, 100);
        assert!(tiles.contains(&TilePos::new(11, 10, 1)));
        assert!(tiles.contains(&TilePos::new(13, 12, 1)));
        assert!(!tiles.contains(&TilePos::new(9, 10, 1)));
        assert!(!tiles.contains(&TilePos::new(10, 11, 1)));
        // North across 0 degrees: (10,11) in, (10,9) out.
        let north = prepare_turfs(3, c, false, 0.0, 120.0, 100, 100);
        assert!(north.contains(&TilePos::new(10, 11, 1)));
        assert!(north.contains(&TilePos::new(9, 12, 1)));
        assert!(!north.contains(&TilePos::new(10, 9, 1)));
        assert!(!north.contains(&TilePos::new(13, 10, 1)));
    }

    #[test]
    fn thrown_away_from_the_epicenter() {
        let all = |_| true;
        // An item east of the epicenter flies further east.
        let at = ranged_target_direct(TilePos::new(5, 5, 1), TilePos::new(3, 5, 1), 4, 180.0, all);
        assert_eq!(at, TilePos::new(9, 5, 1));
        // On the epicenter: `ATAN2(0, 0)` is 0, so west.
        let at = ranged_target_direct(TilePos::new(5, 5, 1), TilePos::new(5, 5, 1), 2, 180.0, all);
        assert_eq!(at, TilePos::new(3, 5, 1));
        // The map edge stops it.
        let inside = |p: TilePos| p.x >= 1;
        let at = ranged_target_direct(
            TilePos::new(2, 5, 1),
            TilePos::new(4, 5, 1),
            5,
            180.0,
            inside,
        );
        assert_eq!(at, TilePos::new(1, 5, 1));
    }

    #[test]
    fn angles_clockwise_from_north() {
        assert_eq!(delta_to_angle(0.0, 1.0), 0.0);
        assert_eq!(delta_to_angle(1.0, 0.0), 90.0);
        assert_eq!(delta_to_angle(0.0, -1.0), 180.0);
        assert_eq!(delta_to_angle(-1.0, 0.0), 270.0);
        assert!((delta_to_angle(-1.0, 1.0) - 315.0).abs() < 1e-4);
    }
}
