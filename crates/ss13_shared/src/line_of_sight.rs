//! BYOND `view()`: which tiles of a square around an eye it sees through opacity.
//!
//! A port of BYOND's own algorithm as Lummox JR described it
//! (<https://www.byond.com/forum/post/2130277#comment20659267>), step for step as OpenDream
//! implements it (`OpenDreamShared/Dream/ViewAlgorithm.cs`, including its fix of the "vis=d"
//! typo in step 4). It is not a raycaster; what players know from BYOND follows from it:
//! - sight spreads ring by ring (Chebyshev rings for `vis2`, then Manhattan diamonds for `vis`),
//!   a tile is reached from a reached clear neighbor of the previous ring;
//! - opaque tiles are seen and stop the spread;
//! - two opaque tiles touching at a corner block the diagonal between them (no peeking through
//!   diagonal gaps), and a single pillar casts a shadow one tile wide forever behind it;
//! - an opaque tile between two seen tiles (a wall face) or the corner of two seen walls around
//!   a seen clear tile is seen too (step 9), so room walls and their corners show whole.
//!
//! Steps 2, 6-8 (luminosity, infrared) are left out: tg mobs have `see_in_dark = 1e6`
//! (`mob_defines.dm`), so darkness never hides a turf from `view()`; tg draws darkness with its
//! lighting planes instead.

/// Visible tiles of one eye, by offset from it.
#[derive(Default)]
pub struct ViewGrid {
    radius: i32,
    side: usize,
    opaque: Vec<bool>,
    vis: Vec<i32>,
    vis2: Vec<i32>,
}

impl ViewGrid {
    /// Computes the tiles seen within Chebyshev distance `radius` of the eye at `(0, 0)`;
    /// `opaque(dx, dy)` is the opacity of the tile at that offset.
    pub fn compute(&mut self, radius: i32, mut opaque: impl FnMut(i32, i32) -> bool) {
        let radius = radius.max(0);
        let side = (2 * radius + 1) as usize;
        self.radius = radius;
        self.side = side;
        self.opaque.clear();
        self.opaque.resize(side * side, false);
        for y in 0..side {
            for x in 0..side {
                self.opaque[y * side + x] = opaque(x as i32 - radius, y as i32 - radius);
            }
        }
        self.vis.clear();
        self.vis.resize(side * side, 0);
        self.vis2.clear();
        self.vis2.resize(side * side, 0);
        let (w, r) = (side as i32, radius);
        let at = |x: i32, y: i32| (y * w + x) as usize;
        let inside = |x: i32, y: i32| (0..w).contains(&x) && (0..w).contains(&y);

        // Step 3, diagonal shadows: Chebyshev ring `d + 1` takes `vis2 = d + 1` (clear) or -1
        // (opaque) where a neighbor has `vis2 == d` (the eye's 0 at the first ring).
        for d in 0..=r {
            let k = d + 1;
            for (x, y) in ring(r, k) {
                if inside(x, y) && neighbor_has(&self.vis2, w, x, y, d) {
                    self.vis2[at(x, y)] = if self.opaque[at(x, y)] { -1 } else { k };
                }
            }
        }
        // Step 4, straight shadows: Manhattan diamond `d + 1` takes `vis = d + 1` (clear and
        // reached by step 3) or -1 (opaque) where a neighbor has `vis == d`.
        for d in 0..2 * (r + 1) {
            let k = d + 1;
            for (x, y) in diamond(r, k) {
                if inside(x, y) && neighbor_has(&self.vis, w, x, y, d) {
                    let i = at(x, y);
                    if self.opaque[i] {
                        self.vis[i] = -1;
                    } else if self.vis2[i] != 0 {
                        self.vis[i] = k;
                    }
                }
            }
        }
        // Step 5: the eye.
        self.vis[at(r, r)] = 1;
        // Step 9: unseen opaque tiles between two seen tiles (walls) or at the corner of two
        // seen opaque tiles around a seen clear one (corners) are seen.
        let mut boundary = Vec::new();
        for y in 0..w {
            for x in 0..w {
                let i = at(x, y);
                if !self.opaque[i] || self.vis[i] != 0 {
                    continue;
                }
                let seen = |x: i32, y: i32| self.vis[at(x, y)] != 0;
                let wall = (x != 0 && x != w - 1 && seen(x - 1, y) && seen(x + 1, y))
                    || (y != 0 && y != w - 1 && seen(x, y - 1) && seen(x, y + 1));
                let corner = [(1, 1), (1, -1), (-1, -1), (-1, 1)]
                    .iter()
                    .any(|&(dx, dy)| {
                        let (cx, cy) = (x + dx, y + dy);
                        inside(cx, cy)
                            && seen(cx, cy)
                            && !self.opaque[at(cx, cy)]
                            && seen(x, cy)
                            && self.opaque[at(x, cy)]
                            && seen(cx, y)
                            && self.opaque[at(cx, y)]
                    });
                if wall || corner {
                    boundary.push(i);
                }
            }
        }
        for i in boundary {
            self.vis[i] = -1;
        }
    }

    /// Whether the tile at offset `(dx, dy)` from the eye is seen.
    pub fn visible(&self, dx: i32, dy: i32) -> bool {
        let (x, y) = (dx + self.radius, dy + self.radius);
        let w = self.side as i32;
        (0..w).contains(&x) && (0..w).contains(&y) && self.vis[(y * w + x) as usize] != 0
    }

    /// Offsets of every seen tile.
    pub fn seen(&self) -> impl Iterator<Item = (i32, i32)> + '_ {
        let (w, r) = (self.side, self.radius);
        (self.vis.iter().enumerate())
            .filter(|(_, v)| **v != 0)
            .map(move |(i, _)| ((i % w) as i32 - r, (i / w) as i32 - r))
    }

    /// Whether the tile at `(dx, dy)` is glimpsed: not seen, but next to a seen clear tile (a
    /// diagonal one only past a clear side, as sight never slips between two touching walls).
    /// The dim ring around the view: the owner's choice; BYOND draws it black.
    pub fn glimpsed(&self, dx: i32, dy: i32) -> bool {
        if self.visible(dx, dy) {
            return false;
        }
        (-1..=1).any(|oy| {
            (-1..=1).any(|ox| {
                let (x, y) = (dx + ox, dy + oy);
                let corner_open =
                    ox == 0 || oy == 0 || !self.opaque_at(x, dy) || !self.opaque_at(dx, y);
                (ox, oy) != (0, 0) && self.visible(x, y) && !self.opaque_at(x, y) && corner_open
            })
        })
    }

    /// Offsets of every glimpsed tile; they reach one tile past the radius.
    pub fn glimpses(&self) -> impl Iterator<Item = (i32, i32)> + '_ {
        let r = self.radius + 1;
        (-r..=r)
            .flat_map(move |dy| (-r..=r).map(move |dx| (dx, dy)))
            .filter(|&(dx, dy)| self.glimpsed(dx, dy))
    }

    /// The opacity `compute` was given at `(dx, dy)`; clear outside the square.
    fn opaque_at(&self, dx: i32, dy: i32) -> bool {
        let (x, y) = (dx + self.radius, dy + self.radius);
        let w = self.side as i32;
        (0..w).contains(&x) && (0..w).contains(&y) && self.opaque[(y * w + x) as usize]
    }
}

/// Grid coordinates (eye at `(r, r)`) of Chebyshev ring `k`.
fn ring(r: i32, k: i32) -> impl Iterator<Item = (i32, i32)> {
    let rows = (-k..=k).flat_map(move |dx| [(dx, -k), (dx, k)]);
    let cols = (1 - k..k).flat_map(move |dy| [(-k, dy), (k, dy)]);
    rows.chain(cols).map(move |(dx, dy)| (r + dx, r + dy))
}

/// Grid coordinates (eye at `(r, r)`) of Manhattan diamond `k`.
fn diamond(r: i32, k: i32) -> impl Iterator<Item = (i32, i32)> {
    (-k..=k).flat_map(move |dx| {
        let dy = k - dx.abs();
        let both = dy != 0;
        [(dx, -dy), (dx, dy)]
            .into_iter()
            .take(1 + usize::from(both))
            .map(move |(dx, dy)| (r + dx, r + dy))
    })
}

/// Whether one of the 8 neighbors of `(x, y)` has `value`.
fn neighbor_has(values: &[i32], w: i32, x: i32, y: i32, value: i32) -> bool {
    for ny in (y - 1).max(0)..(y + 2).min(w) {
        for nx in (x - 1).max(0)..(x + 2).min(w) {
            if (nx, ny) != (x, y) && values[(ny * w + nx) as usize] == value {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Draws `radius` view of `map` (rows top-down, `@` the eye, `#` opaque, `.` clear) as
    /// the same rows with unseen tiles as spaces.
    fn view(map: &str) -> String {
        let rows: Vec<&str> = map.trim().lines().map(str::trim).collect();
        let h = rows.len() as i32;
        let (mut ex, mut ey) = (0, 0);
        for (row, line) in rows.iter().enumerate() {
            if let Some(col) = line.find('@') {
                (ex, ey) = (col as i32, h - 1 - row as i32);
            }
        }
        let opaque = |x: i32, y: i32| {
            let row = h - 1 - y;
            row < 0
                || row >= h
                || x < 0
                || rows[row as usize]
                    .as_bytes()
                    .get(x as usize)
                    .is_none_or(|&c| c == b'#')
        };
        let mut grid = ViewGrid::default();
        let radius = rows
            .iter()
            .map(|r| r.len() as i32)
            .chain([h])
            .max()
            .unwrap();
        grid.compute(radius, |dx, dy| opaque(ex + dx, ey + dy));
        rows.iter()
            .enumerate()
            .map(|(row, line)| {
                let y = h - 1 - row as i32;
                (line.char_indices())
                    .map(|(x, c)| {
                        if grid.visible(x as i32 - ex, y - ey) {
                            c
                        } else {
                            ' '
                        }
                    })
                    .collect::<String>()
                    .trim_end()
                    .to_owned()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn expect(map: &str, want: &str) {
        let want: Vec<&str> = (want.trim().lines())
            .map(|l| l.trim().strip_prefix('|').unwrap_or(l).trim_end())
            .collect();
        assert_eq!(view(map), want.join("\n"), "\n{map}");
    }

    /// The glimpsed tiles of `map` (as in [`view`]) inside the map, as `(x, y)`.
    fn glimpsed(map: &str) -> Vec<(i32, i32)> {
        let rows: Vec<&str> = map.trim().lines().map(str::trim).collect();
        let h = rows.len() as i32;
        let at = |x: i32, y: i32| {
            let row = h - 1 - y;
            (0..h)
                .contains(&row)
                .then(|| rows[row as usize].as_bytes().get(x as usize))
                .flatten()
        };
        let (ex, ey) = (0..h)
            .flat_map(|y| (0..20).map(move |x| (x, y)))
            .find(|&(x, y)| at(x, y) == Some(&b'@'))
            .unwrap();
        let mut grid = ViewGrid::default();
        grid.compute(20, |dx, dy| at(ex + dx, ey + dy).is_none_or(|&c| c == b'#'));
        let mut out: Vec<(i32, i32)> = (grid.glimpses())
            .map(|(dx, dy)| (ex + dx, ey + dy))
            .filter(|&(x, y)| at(x, y).is_some())
            .collect();
        out.sort();
        out
    }

    #[test]
    fn nothing_is_glimpsed_through_a_wall() {
        let map = "
            .........
            #########
            ....@....
            ";
        assert_eq!(glimpsed(map), []);
    }

    #[test]
    fn the_edge_of_a_shadow_is_glimpsed() {
        // The pillar's shadow (x >= 3 on the eye's row) borders seen floor above and below.
        let map = "
            ......
            @.#...
            ......
            ";
        assert_eq!(glimpsed(map), [(3, 1), (4, 1), (5, 1)]);
    }

    #[test]
    fn diagonal_gaps_are_not_glimpsed_through() {
        // (2, 1) touches the seen floor (1, 0) only across the gap between two walls.
        let map = "
            .#.
            @.#
            ";
        assert_eq!(glimpsed(map), []);
    }

    #[test]
    fn open_room_is_seen_with_its_walls_and_corners() {
        expect(
            "
            #######
            #.....#
            #..@..#
            #.....#
            #######
            ",
            "
            |#######
            |#.....#
            |#..@..#
            |#.....#
            |#######
            ",
        );
    }

    #[test]
    fn walls_hide_what_is_behind_them() {
        expect(
            "
            .........
            .........
            #########
            ....@....
            ",
            "
            |
            |
            |#########
            |....@....
            ",
        );
    }

    #[test]
    fn a_pillar_casts_a_shadow_one_tile_wide() {
        expect(
            "
            .........
            .........
            @.#......
            .........
            ",
            "
            |.........
            |.........
            |@.#
            |.........
            ",
        );
    }

    #[test]
    fn a_diagonal_pillar_shadows_its_diagonal() {
        expect(
            "
            ......
            ......
            ......
            ..#...
            ......
            @.....
            ",
            "
            |.....
            |.... .
            |... ..
            |..#...
            |......
            |@.....
            ",
        );
    }

    #[test]
    fn diagonal_gaps_between_walls_are_closed() {
        // Two walls touching at a corner next to the eye: nothing past the diagonal between them.
        expect(
            "
            ....
            #...
            @#..
            ",
            "
            |
            |#
            |@#
            ",
        );
        // Along a wall beside the eye: the open column and a widening strip past the wall's end,
        // but not the tile diagonally past the wall's corner (2, 3).
        expect(
            "
            .....
            .....
            .#...
            @#...
            #....
            ",
            "
            |...
            |..
            |.#
            |@#
            |#
            ",
        );
    }

    #[test]
    fn a_doorway_shows_a_cone_of_the_next_room() {
        let closed = "
            #########
            #.......#
            #.......#
            ####D####
            ....@....
            ";
        expect(
            &closed.replace('D', "#"),
            "
            |
            |
            |
            |#########
            |....@....
            ",
        );
        expect(
            &closed.replace('D', "."),
            "
            |  #####
            |  .....
            |   ...
            |####.####
            |....@....
            ",
        );
    }

    #[test]
    fn corridor_corner() {
        // Around an L corner: the far arm is seen only along the line of the near one.
        expect(
            "
            #######
            #.....#
            #.#####
            #.#
            #@#
            ###
            ",
            "
            |###
            |#..
            |#.#
            |#.#
            |#@#
            |###
            ",
        );
    }
}
