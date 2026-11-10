//! BandaStation `/datum/computer_file/program/navigator` (`NtosNavigator`): where the user is
//! (tile, facing, area) by the device's NTNet signal, and the ladders and stairs
//! (`get_map_ui_data`'s `stairs`) of the user's level.
// shortcut: the map is a list (the nanomap view, `crate::nanomap`, has no zoom or stairs yet): the
// user's position and the
// nearest [`MAX_STAIRS`] ladders and stairs of the level with their distance and direction. A
// low signal rounds the position to [`LOW_SIGNAL_STEP`] tiles server-side instead of the
// client's ±5 tile jitter (which handed the exact position to the client).

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use ss13_mapload::{Areas, DmName};
use ss13_shared::{Dir, Facing, TilePos};

use crate::modpc::{NavLocation, NavStair, NavigatorData};
use crate::multiz::{Ladder, Stairs};

/// Ladders and stairs listed, nearest first.
pub const MAX_STAIRS: usize = 10;
/// The grid a low signal's position snaps to.
pub const LOW_SIGNAL_STEP: i32 = 5;

/// `get_ntnet_status()` values the program knows (`SIGNAL` of the interface).
pub use crate::modpc::{SIGNAL_GOOD, SIGNAL_LOST, SIGNAL_LOW};

/// What `ui_data` reads.
#[derive(SystemParam)]
pub struct NavigatorParams<'w, 's> {
    users: Query<'w, 's, (&'static TilePos, Option<&'static Facing>)>,
    areas: Option<Res<'w, Areas>>,
    names: Query<'w, 's, &'static DmName>,
    stairs: Query<'w, 's, (&'static TilePos, Has<Ladder>), Or<(With<Ladder>, With<Stairs>)>>,
}

/// `ui_data` for `user` with the device's `signal` (0-3; ethernet counts as good).
/// Cost: one pass over the ladders and stairs (a few hundred tiles) per refresh of an open
/// navigator.
pub fn ui_data(params: &NavigatorParams, user: Entity, signal: u8) -> NavigatorData {
    let signal = signal.min(SIGNAL_GOOD);
    let mut data = NavigatorData {
        signal,
        ..default()
    };
    if signal == SIGNAL_LOST {
        return data;
    }
    let Ok((&at, facing)) = params.users.get(user) else {
        return data;
    };
    let area = params
        .areas
        .as_ref()
        .and_then(|a| a.area_at(at))
        .and_then(|a| params.names.get(a).ok())
        .map(|n| crate::vending::plain_name(&n.name).to_owned())
        .unwrap_or_default();
    let good = signal == SIGNAL_GOOD;
    let snap = |v: i32| {
        if good {
            v
        } else {
            (v + LOW_SIGNAL_STEP / 2).div_euclid(LOW_SIGNAL_STEP) * LOW_SIGNAL_STEP
        }
    };
    let shown = TilePos::new(snap(at.x), snap(at.y), at.z);
    data.location = Some(NavLocation {
        x: shown.x,
        y: shown.y,
        z: shown.z,
        dir: good.then(|| dir_ru(facing.map_or(Dir::South, |f| f.0)).to_owned()),
        area,
    });
    data.stairs = nearest_stairs(shown, params.stairs.iter().map(|(p, l)| (*p, l)));
    data
}

/// The [`MAX_STAIRS`] nearest of `stairs` on `from`'s level.
pub fn nearest_stairs(
    from: TilePos,
    stairs: impl Iterator<Item = (TilePos, bool)>,
) -> Vec<NavStair> {
    let mut found: Vec<NavStair> = stairs
        .filter_map(|(pos, ladder)| {
            Some(NavStair {
                x: pos.x,
                y: pos.y,
                distance: from.chebyshev(pos)?,
                toward: toward(from, pos).map(dir_ru).unwrap_or_default().to_owned(),
                ladder,
            })
        })
        .collect();
    found.sort_by_key(|s| (s.distance, s.x, s.y));
    found.truncate(MAX_STAIRS);
    found
}

/// BYOND `get_dir(from, to)`; `None` on the same tile.
fn toward(from: TilePos, to: TilePos) -> Option<Dir> {
    Some(match ((to.x - from.x).signum(), (to.y - from.y).signum()) {
        (0, 1) => Dir::North,
        (0, -1) => Dir::South,
        (1, 0) => Dir::East,
        (-1, 0) => Dir::West,
        (1, 1) => Dir::NorthEast,
        (-1, 1) => Dir::NorthWest,
        (1, -1) => Dir::SouthEast,
        (-1, -1) => Dir::SouthWest,
        _ => return None,
    })
}

pub fn dir_ru(dir: Dir) -> &'static str {
    match dir {
        Dir::North => "север",
        Dir::South => "юг",
        Dir::East => "восток",
        Dir::West => "запад",
        Dir::NorthEast => "северо-восток",
        Dir::NorthWest => "северо-запад",
        Dir::SouthEast => "юго-восток",
        Dir::SouthWest => "юго-запад",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stairs_are_listed_nearest_first_on_the_users_level() {
        let me = TilePos::new(10, 10, 2);
        let stairs = [
            (TilePos::new(20, 10, 2), false),
            (TilePos::new(12, 13, 2), true),
            (TilePos::new(10, 11, 3), false),
        ];
        let got = nearest_stairs(me, stairs.into_iter());
        assert_eq!(got.len(), 2);
        assert_eq!((got[0].distance, got[0].ladder), (3, true));
        assert_eq!(got[0].toward, "северо-восток");
        assert_eq!((got[1].distance, got[1].toward.as_str()), (10, "восток"));
        let many = (0..30).map(|i| (TilePos::new(i, 0, 2), false));
        assert_eq!(nearest_stairs(me, many).len(), MAX_STAIRS);
    }

    #[test]
    fn the_signal_decides_what_the_user_sees() {
        let mut world = World::new();
        let user = world.spawn((TilePos::new(7, 8, 1), Facing(Dir::East))).id();
        world.spawn((TilePos::new(9, 8, 1), Ladder::default()));
        let mut state = bevy::ecs::system::SystemState::<NavigatorParams>::new(&mut world);
        let params = state.get(&world).unwrap();
        // A console's ethernet (3) is a good signal.
        let good = ui_data(&params, user, 3);
        assert_eq!(good.signal, SIGNAL_GOOD);
        let here = good.location.unwrap();
        assert_eq!(
            (here.x, here.y, here.dir.as_deref()),
            (7, 8, Some("восток"))
        );
        assert_eq!(good.stairs.len(), 1);
        assert_eq!(good.stairs[0].toward, "восток");
        let low = ui_data(&params, user, SIGNAL_LOW).location.unwrap();
        assert_eq!((low.x, low.y, low.dir), (5, 10, None));
        assert_eq!(ui_data(&params, user, SIGNAL_LOST).location, None);
    }
}
