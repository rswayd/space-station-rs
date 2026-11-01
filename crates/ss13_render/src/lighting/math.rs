//! tg lighting math on plain data (`code/modules/lighting/lighting_source.dm`,
//! `lighting_corner.dm`), independent of Bevy so it can be tested on tiny grids.
//!
//! Corner `(cx, cy)` is the north-east corner of tile `(cx, cy)`: tg's `/datum/lighting_corner`
//! at `x + 0.5, y + 0.5`. A tile is drawn with a bilinear blend of its four corners.

use ss13_shared::{Dir, LightSource};

/// tg `LIGHTING_ROUND_VALUE`.
const ROUND: f32 = 1.0 / 64.0;

/// tg `dir2angle`.
fn dir_angle(dir: Dir) -> f32 {
    match dir {
        Dir::North => 0.0,
        Dir::NorthEast => 45.0,
        Dir::East => 90.0,
        Dir::SouthEast => 135.0,
        Dir::South => 180.0,
        Dir::SouthWest => 225.0,
        Dir::West => 270.0,
        Dir::NorthWest => 315.0,
    }
}

/// tg `delta_to_angle`: degrees clockwise from north.
fn delta_angle(x: f32, y: f32) -> f32 {
    if y == 0.0 {
        return if x >= 0.0 { 90.0 } else { 270.0 };
    }
    let a = (x / y).atan().to_degrees();
    if y < 0.0 {
        a + 180.0
    } else if x < 0.0 {
        a + 360.0
    } else {
        a
    }
}

/// tg `falloff_at_coord` (z = 0): brightness 0..1 at offset `(x, y)` tiles from the source.
pub fn falloff(light: &LightSource, x: f32, y: f32) -> f32 {
    let range = light.range.max(1.0);
    // tg takes sqrt of a negative for space height right under the source; it is 0 there.
    let d = (x * x + y * y + light.height).max(0.0).sqrt();
    let multiplier = 1.0 - (d / range).clamp(0.0, 1.0);
    let angle = f32::from(light.angle);
    if angle >= 360.0 || angle <= 0.0 {
        return multiplier;
    }
    let mut delta = (dir_angle(light.dir) - delta_angle(x, y)).abs();
    if delta > 180.0 {
        delta = 360.0 - delta;
    }
    (multiplier * (1.0 - (delta - angle / 2.0).max(0.0) / 30.0)).max(0.0)
}

/// The source offset in tiles (tg `offset_x`, `offset_y`).
pub fn offset(light: &LightSource) -> (f32, f32) {
    (
        f32::from(light.offset[0]) / 32.0,
        f32::from(light.offset[1]) / 32.0,
    )
}

/// tg `impacted_corners` view range: `ceil(light_range + visual_offset)`.
pub fn reach(light: &LightSource) -> i32 {
    let (ox, oy) = offset(light);
    let visual = ox.abs().ceil().max(oy.abs().ceil());
    (light.range + visual).ceil().max(0.0) as i32
}

/// Exact `num / den` slope of symmetric shadowcasting (`den > 0`).
#[derive(Clone, Copy)]
struct Slope {
    num: i32,
    den: i32,
}

/// Tiles visible from `(0, 0)` within Chebyshev distance `radius`, by symmetric shadowcasting
/// (Albert Ford). Opaque tiles are visible but block; `visit` gets every visible tile once,
/// with whether it is opaque.
///
/// shortcut: BYOND `view()` uses its own (unpublished) algorithm; symmetric shadowcasting
/// agrees on rooms and corridors and may differ by a tile at oblique wall corners.
pub fn visible(
    radius: i32,
    opaque: impl Fn(i32, i32) -> bool,
    mut visit: impl FnMut(i32, i32, bool),
) {
    visit(0, 0, opaque(0, 0));
    if radius <= 0 {
        return;
    }
    // Diagonals belong to two quadrants.
    let side = (2 * radius + 1) as usize;
    let mut seen = vec![false; side * side];
    let mut visit = |x: i32, y: i32, wall: bool| {
        let i = (y + radius) as usize * side + (x + radius) as usize;
        if !std::mem::replace(&mut seen[i], true) {
            visit(x, y, wall);
        }
    };
    // (depth, col) of a quadrant -> offset from the origin.
    let quadrants: [fn(i32, i32) -> (i32, i32); 4] = [
        |d, c| (c, d),
        |d, c| (d, -c),
        |d, c| (-c, -d),
        |d, c| (-d, c),
    ];
    let mut stack = Vec::new();
    for transform in quadrants {
        let wall = |d, c| {
            let (x, y) = transform(d, c);
            opaque(x, y)
        };
        stack.push((1, Slope { num: -1, den: 1 }, Slope { num: 1, den: 1 }));
        while let Some((depth, mut start, end)) = stack.pop() {
            // round_ties_up(depth * start) ..= round_ties_down(depth * end)
            let min_col = (2 * depth * start.num + start.den).div_euclid(2 * start.den);
            let max_col = -((-2 * depth * end.num + end.den).div_euclid(2 * end.den));
            let mut prev: Option<bool> = None;
            for col in min_col..=max_col {
                let is_wall = wall(depth, col);
                let symmetric =
                    col * start.den >= depth * start.num && col * end.den <= depth * end.num;
                if is_wall || symmetric {
                    let (x, y) = transform(depth, col);
                    visit(x, y, is_wall);
                }
                let tile_slope = Slope {
                    num: 2 * col - 1,
                    den: 2 * depth,
                };
                if prev == Some(true) && !is_wall {
                    start = tile_slope;
                }
                if prev == Some(false) && is_wall && depth < radius {
                    stack.push((depth + 1, start, tile_slope));
                }
                prev = Some(is_wall);
            }
            if prev == Some(false) && depth < radius {
                stack.push((depth + 1, start, end));
            }
        }
    }
}

/// What one source adds to each corner it reaches: tg `update_corners` with `APPLY_CORNER`.
/// `opaque(dx, dy)` is relative to the source tile; corners come relative to it too.
pub fn contribution(
    light: &LightSource,
    opaque: impl Fn(i32, i32) -> bool,
) -> Vec<(i32, i32, [f32; 3])> {
    let r = reach(light);
    let side = (2 * r + 2) as usize;
    // Corners `-r-1..=r` on each axis, deduplicated: four tiles share a corner.
    let mut seen = vec![false; side * side];
    let mut corners = Vec::new();
    visible(r, opaque, |x, y, is_opaque| {
        if is_opaque {
            return; // tg `IS_OPAQUE_TURF`: an opaque tile gets light only through its neighbours.
        }
        for (cx, cy) in [(x - 1, y - 1), (x, y - 1), (x - 1, y), (x, y)] {
            let i = (cy + r + 1) as usize * side + (cx + r + 1) as usize;
            if !std::mem::replace(&mut seen[i], true) {
                corners.push((cx, cy));
            }
        }
    });
    let (ox, oy) = offset(light);
    let color = light.color.map(|c| f32::from(c) / 255.0);
    corners
        .into_iter()
        .filter_map(|(cx, cy)| {
            let f = falloff(light, cx as f32 + 0.5 + ox, cy as f32 + 0.5 + oy) * light.power;
            (f != 0.0).then(|| (cx, cy, color.map(|c| c * f)))
        })
        .collect()
}

/// tg `lighting_corner/update_objects`: the corner colour drawn from its summed luminosity,
/// scaled down so the brightest channel is at most 1, rounded to 1/64.
pub fn corner_color(lum: [f32; 3]) -> [f32; 3] {
    let largest = lum[0].max(lum[1]).max(lum[2]);
    let factor = if largest > 1.0 { 1.0 / largest } else { 1.0 };
    lum.map(|l| (l * factor / ROUND).round() * ROUND)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn light(range: f32) -> LightSource {
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

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }

    #[test]
    fn falloff_matches_tg_formula() {
        let l = light(8.0);
        // 1 - sqrt(0.5^2 + 0.5^2 + 1) / 8
        assert!(close(falloff(&l, 0.5, 0.5), 1.0 - 1.5f32.sqrt() / 8.0));
        assert!(close(
            falloff(&l, 3.5, -2.5),
            1.0 - (12.25f32 + 6.25 + 1.0).sqrt() / 8.0
        ));
        assert_eq!(falloff(&l, 8.5, 0.5), 0.0);
        // Ranges below 1 divide by 1.
        assert!(close(falloff(&light(0.5), 0.0, 0.0), 0.0));
        // Starlight sits under the floor: full brightness at the nearest corners.
        let star = LightSource {
            height: -0.5,
            ..light(2.0)
        };
        assert!(close(falloff(&star, 0.5, 0.5), 1.0));
        assert!(close(falloff(&star, 0.0, 0.0), 1.0));
    }

    #[test]
    fn cone_dims_outside_its_angle() {
        // A wall fixture facing south: 170 degrees, 30 more degrees of fade.
        let l = LightSource {
            angle: 170,
            dir: Dir::South,
            ..light(8.0)
        };
        let full = 1.0 - (4.0f32 + 1.0).sqrt() / 8.0;
        assert!(close(falloff(&l, 0.0, -2.0), full));
        // Straight sideways is 90 degrees off axis: 5 degrees past the half angle.
        assert!(close(falloff(&l, 2.0, 0.0), full * (1.0 - 5.0 / 30.0)));
        // Behind the fixture: dark.
        assert_eq!(falloff(&l, 0.0, 2.0), 0.0);
    }

    fn grid<'a>(rows: &'a [&'a str]) -> impl Fn(i32, i32) -> bool + 'a {
        // `rows[0]` is the top row; the source `S` is the origin.
        let (sy, sx) = rows
            .iter()
            .enumerate()
            .find_map(|(y, r)| r.find('S').map(|x| (y as i32, x as i32)))
            .unwrap();
        move |x, y| {
            let (col, row) = (sx + x, sy - y);
            row >= 0
                && col >= 0
                && rows
                    .get(row as usize)
                    .and_then(|r| r.as_bytes().get(col as usize))
                    == Some(&b'#')
        }
    }

    fn seen(radius: i32, opaque: impl Fn(i32, i32) -> bool) -> Vec<(i32, i32)> {
        let mut out = Vec::new();
        visible(radius, opaque, |x, y, _| out.push((x, y)));
        out.sort();
        let n = out.len();
        out.dedup();
        assert_eq!(n, out.len(), "a tile was visited twice");
        out
    }

    #[test]
    fn open_space_sees_the_whole_box() {
        assert_eq!(seen(3, |_, _| false).len(), 49);
    }

    #[test]
    fn walls_are_seen_and_cast_shadows() {
        let rows = [
            ".......", //
            ".......", //
            "...#...", //
            "...S...", //
            ".......", //
        ];
        let tiles = seen(3, grid(&rows));
        assert!(tiles.contains(&(0, 1)), "the wall itself is visible");
        assert!(!tiles.contains(&(0, 2)), "right behind the wall is not");
        assert!(!tiles.contains(&(0, 3)));
        assert!(tiles.contains(&(2, 2)), "diagonal past the wall is");
        assert!(tiles.contains(&(0, -3)));
    }

    #[test]
    fn closed_room_keeps_light_in() {
        let rows = [
            ".........", //
            ".#######.", //
            ".#.....#.", //
            ".#..S..#.", //
            ".#.....#.", //
            ".#######.", //
            ".........", //
        ];
        let tiles = seen(4, grid(&rows));
        assert!(tiles.contains(&(3, 0)), "walls are seen");
        assert!(!tiles.contains(&(4, 0)), "nothing outside the walls");
        assert!(!tiles.contains(&(0, 3)));
        assert!(!tiles.contains(&(-4, -3)));
    }

    #[test]
    fn corners_light_walls_from_their_open_side_only() {
        // A corridor 1 tile wide running east-west, walls above and below.
        let rows = [
            "#########", //
            "....S....", //
            "#########", //
        ];
        let c = contribution(&light(4.0), grid(&rows));
        let at = |x, y| {
            c.iter()
                .find(|&&(cx, cy, _)| (cx, cy) == (x, y))
                .map(|v| v.2[0])
        };
        // Corners on the corridor's edges are lit; the far edges of the walls are not.
        assert!(at(0, 0).is_some() && at(0, -1).is_some());
        assert_eq!(at(0, 1), None);
        assert_eq!(at(0, -2), None);
        // Nearest corners: 1 - sqrt(0.5 + 1) / 4.
        assert!(close(at(0, 0).unwrap(), 1.0 - 1.5f32.sqrt() / 4.0));
        assert!(at(3, 0).unwrap() < at(1, 0).unwrap());
    }

    #[test]
    fn opening_a_door_lets_light_through() {
        let closed = ["#####", "..#..", "S.#..", "..#..", "#####"];
        let open = ["#####", "..#..", "S....", "..#..", "#####"];
        let lit = |rows: &[&str]| {
            contribution(&light(5.0), grid(rows))
                .iter()
                .filter(|&&(cx, _, _)| cx >= 3)
                .count()
        };
        assert_eq!(lit(&closed), 0);
        assert!(lit(&open) > 0);
    }

    #[test]
    fn colors_are_capped_like_tg() {
        assert_eq!(corner_color([2.0, 1.0, 0.5]), [1.0, 0.5, 0.25]);
        assert_eq!(corner_color([0.5, 0.25, 0.0]), [0.5, 0.25, 0.0]);
        assert_eq!(
            corner_color([0.3, 0.0, 0.0])[0],
            (0.3f32 * 64.0).round() / 64.0
        );
    }
}
