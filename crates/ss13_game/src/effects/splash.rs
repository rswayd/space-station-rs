//! `spread_reagents` of `chem_splash.dm`: what a chemical bomb leaves behind, over the atmos-connected
//! tiles in range and in sight of the epicenter, half as much per tile of distance.
// shortcut: the things inside containers on the tiles are not exposed (`get_all_contents`).

use bevy::prelude::*;
use ss13_reagents::Holder;
use ss13_shared::TilePos;

use super::on_tile;
use super::particles::{Kind, start};

/// `spread_reagents`: the cap on `spread_range`, and the temperature that lights fires.
const MAX_RANGE: u32 = 20;
const HOT_CHEM: f32 = 300.0;

/// `0.5 / 2 ** max(1, distance)`.
pub fn fraction_at(distance: u32) -> f32 {
    0.5 / 2f32.powi(distance.max(1) as i32)
}

/// Exposes (`TOUCH`) everything within `range` of `epicenter`; `source` stays as it is.
pub fn spread_reagents(world: &mut World, source: &Holder, epicenter: TilePos, range: u32) {
    let range = range.min(MAX_RANGE);
    start(world, Kind::Steam, 10, false, epicenter);
    let mut tiles = vec![epicenter];
    let mut next = 0;
    while next < tiles.len() {
        let at = tiles[next];
        next += 1;
        if at.chebyshev(epicenter).is_none_or(|d| d >= range) {
            continue;
        }
        for to in super::smoke::spread_from(world, at) {
            if !tiles.contains(&to) {
                tiles.push(to);
            }
        }
    }
    let hot = source.chem_temp >= HOT_CHEM;
    for &tile in &tiles {
        if hot && let Some(mut atmos) = world.get_resource_mut::<ss13_atmos::Atmos>() {
            atmos.hotspot_expose(tile, f64::from(source.chem_temp) * 2.0, 5.0, false);
        }
        if !crate::mobs::ai::behaviors::can_see(world, epicenter, tile, range) {
            continue;
        }
        let fraction = fraction_at(tile.chebyshev(epicenter).unwrap_or(0));
        for thing in on_tile(world, tile) {
            if world.get_entity(thing).is_ok() {
                crate::janitor::expose(world, thing, source, fraction);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splashes_halve_with_distance() {
        assert_eq!(fraction_at(0), 0.25);
        assert_eq!(fraction_at(1), 0.25);
        assert_eq!(fraction_at(2), 0.125);
    }
}
