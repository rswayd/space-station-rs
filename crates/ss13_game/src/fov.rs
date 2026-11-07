//! Field of view (server), tg `code/modules/mob/living/living_fov.dm`.
//!
//! A mob's [`FovTraits`] is tg `fov_traits` (source -> `FOV_*_DEGREES`); its `fov_view` is the
//! highest of them (a forward cone beats a reversed one). [`in_fov`] is tg `in_fov`: blind sees
//! nothing, the 3x3 box around the mob is always seen, otherwise the angle between the facing
//! and the target must be inside `(360 - |fov|) / 2` (outside it for a reversed cone).
// shortcut: server side only. The cone drawn over the client's view (`fov_handler`'s
// `FIELD_OF_VISION_BLOCKER_PLANE` mask) and `play_fov_effect` sound cues need a client render
// pass and a protocol field for the angle; until then nothing hides on screen. No
// nearsightedness radius, no `TRAIT_EXPANDED_FOV`; no source sets a trait yet (visors, eyes,
// fear's tunnel vision, newspapers add theirs through [`add_fov_trait`]).

use bevy::prelude::*;
use ss13_shared::{Dir, Facing, TilePos};

/// `FOV_90_DEGREES` .. `FOV_REVERSE_270_DEGRESS`.
pub const FOV_90: i32 = 90;
pub const FOV_180: i32 = 180;
pub const FOV_270: i32 = 270;
pub const FOV_REVERSE_270: i32 = -270;

/// tg `fov_traits`.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct FovTraits(pub Vec<(&'static str, i32)>);

impl FovTraits {
    /// `update_fov`: the highest angle, 0 for none.
    pub fn view(&self) -> i32 {
        self.0.iter().map(|&(_, a)| a).max().unwrap_or(0)
    }
}

/// tg `add_fov_trait(source, type)`.
pub fn add_fov_trait(world: &mut World, mob: Entity, source: &'static str, angle: i32) {
    let Ok(mut entity) = world.get_entity_mut(mob) else {
        return;
    };
    let mut traits = entity.take::<FovTraits>().unwrap_or_default();
    traits.0.retain(|&(s, _)| s != source);
    traits.0.push((source, angle));
    entity.insert(traits);
}

/// tg `remove_fov_trait(source)`.
pub fn remove_fov_trait(world: &mut World, mob: Entity, source: &str) {
    if let Some(mut traits) = world.get_mut::<FovTraits>(mob) {
        traits.0.retain(|&(s, _)| s != source);
    }
}

/// The cone test of tg `in_fov` for a mob facing `facing` with `fov_view` `fov`, the target
/// `rel` tiles away.
pub fn cone_allows(fov: i32, facing: Dir, rel: (i32, i32)) -> bool {
    if fov == 0 || (rel.0.abs() <= 1 && rel.1.abs() <= 1) {
        return true;
    }
    let (dx, dy) = facing.offset();
    let (dir_x, dir_y) = (f64::from(dx), f64::from(dy));
    let (rx, ry) = (f64::from(rel.0), f64::from(rel.1));
    let cos = (dir_x * rx + dir_y * ry) / ((dir_x.hypot(dir_y)) * rx.hypot(ry));
    let angle = cos.clamp(-1.0, 1.0).acos().to_degrees();
    let vision = f64::from(360 - fov.abs()) / 2.0;
    if fov > 0 {
        angle < vision
    } else {
        angle > vision
    }
}

/// tg `/mob/living/in_fov(observed)`.
pub fn in_fov(world: &World, mob: Entity, observed: TilePos) -> bool {
    if crate::vision::is_blind(world, mob) {
        return false;
    }
    let Some(&at) = world.get::<TilePos>(mob) else {
        return true;
    };
    let fov = world.get::<FovTraits>(mob).map_or(0, FovTraits::view);
    let facing = world.get::<Facing>(mob).map_or(Dir::South, |f| f.0);
    cone_allows(fov, facing, (observed.x - at.x, observed.y - at.y))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cone_hides_what_is_behind() {
        // Facing north with a 90 degree blind spot behind (`FOV_90_DEGREES`: 135 each side).
        assert!(cone_allows(FOV_90, Dir::North, (0, 5)));
        assert!(cone_allows(FOV_90, Dir::North, (5, 0)));
        assert!(!cone_allows(FOV_90, Dir::North, (0, -5)));
        // 270: only a 45 degree cone forward.
        assert!(!cone_allows(FOV_270, Dir::North, (5, 0)));
        assert!(cone_allows(FOV_270, Dir::North, (1, 5)));
        // Reversed 270 (reading a newspaper): the front is hidden, the back is seen.
        assert!(!cone_allows(FOV_REVERSE_270, Dir::North, (0, 5)));
        assert!(cone_allows(FOV_REVERSE_270, Dir::North, (0, -5)));
        // The 3x3 box and no FOV are always seen.
        assert!(cone_allows(FOV_270, Dir::North, (0, -1)));
        assert!(cone_allows(0, Dir::North, (0, -5)));
    }

    #[test]
    fn the_highest_trait_wins_and_sources_replace() {
        let mut world = World::new();
        let mob = world.spawn(TilePos::new(5, 5, 1)).id();
        add_fov_trait(&mut world, mob, "newspaper", FOV_REVERSE_270);
        add_fov_trait(&mut world, mob, "visor", FOV_180);
        assert_eq!(world.get::<FovTraits>(mob).unwrap().view(), FOV_180);
        remove_fov_trait(&mut world, mob, "visor");
        assert_eq!(world.get::<FovTraits>(mob).unwrap().view(), FOV_REVERSE_270);
    }
}
