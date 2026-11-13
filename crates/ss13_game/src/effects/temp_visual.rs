//! Temporary visuals (server), after tg `/obj/effect/temp_visual`
//! (`code/game/objects/effects/temporary_visuals/temporary_visual.dm`): an unclickable sprite
//! (`MOUSE_OPACITY_TRANSPARENT`, `ABOVE_MOB_LAYER`) that deletes itself after its `duration`
//! (`QDEL_IN`), turned to a random cardinal (`randomdir`) or to a set direction
//! (`dir_setting`).
//!
//! [`spawn`] is the `new /obj/effect/temp_visual/...(loc)` of game code; the kinds ported here:
//! - [`blood_splatter`]: `dir_setting/bloodsplatter` of `/mob/living/create_splatter`, on a
//!   brute projectile hit of a mob that has blood (`create_projectile_hit_effects`).
//!
//! - [`telegraph`]: `/obj/effect/temp_visual/telegraphing` and its subtypes, the warning box,
//!   circle or line on a tile an attack is about to land on (`code/datums/ai/telegraph_effects.dm`).
//!
//! The other visuals belong to their mechanics and are spawned there with [`spawn`] (older
//! ones keep their own copies: `crate::point`, `crate::emp`, `crate::mobs::megafauna`).
// shortcut: the splatter does not slide 16 px and fade over its 0.5 s (`animate`): it shows
// still and vanishes; it is always red (no blood type colours).

use bevy::prelude::*;
use ss13_mapload::IconIndex;
use ss13_net::server::Networked;
use ss13_shared::{Appearance, Dir, Facing, IconStateId, Layer, MouseOpacity, TilePos};

/// `ABOVE_MOB_LAYER`, `BELOW_MOB_LAYER`; `GAME_PLANE`.
pub const ABOVE_MOB_LAYER: f32 = 4.1;
pub const BELOW_MOB_LAYER: f32 = 3.7;
pub const GAME_PLANE: i16 = -4;
/// `BLOOD_COLOR_RED`.
const BLOOD_RED: [u8; 4] = [0xC8, 0x00, 0x00, 0xFF];
const BLOOD_DMI: &str = "icons/effects/blood.dmi";

/// A temporary visual, gone at `until`.
#[derive(Component, Clone, Copy, Debug)]
pub struct TempVisual {
    pub until: f64,
}

/// What a temporary visual looks like and how long it stays.
#[derive(Clone, Copy, Debug)]
pub struct Visual {
    pub icon: &'static str,
    pub state: &'static str,
    /// `duration`, seconds.
    pub secs: f64,
    pub layer: f32,
    pub plane: i16,
    pub offset: [i16; 2],
    pub color: Option<[u8; 4]>,
    /// `dir_setting`'s `set_dir`; `None` picks a random cardinal (`randomdir`).
    pub dir: Option<Dir>,
}

impl Visual {
    /// `/obj/effect/temp_visual` defaults: `ABOVE_MOB_LAYER` on the game plane, random dir.
    pub const fn new(icon: &'static str, state: &'static str, secs: f64) -> Self {
        Self {
            icon,
            state,
            secs,
            layer: ABOVE_MOB_LAYER,
            plane: GAME_PLANE,
            offset: [0, 0],
            color: None,
            dir: None,
        }
    }
}

/// `new /obj/effect/temp_visual/...(pos)`; `None` without its icon state.
pub fn spawn(world: &mut World, pos: TilePos, v: Visual) -> Option<Entity> {
    let icon: IconStateId = world.get_resource::<IconIndex>()?.get(v.icon, v.state)?;
    let now = world.resource::<Time>().elapsed_secs_f64();
    let dir = v.dir.unwrap_or_else(|| {
        let i = world.resource_mut::<crate::combat::CombatRng>().range(0, 3);
        Dir::CARDINALS[i as usize]
    });
    Some(
        world
            .spawn((
                Name::new(v.state),
                pos,
                Facing(dir),
                Appearance {
                    icon_state: icon,
                    layer: Layer(v.layer),
                    plane: v.plane,
                    color: v.color,
                    pixel_offset: v.offset,
                    rotation: 0,
                    overlays: vec![],
                    mouse_opacity: MouseOpacity::Transparent,
                    glide: 0,
                },
                Networked,
                TempVisual {
                    until: now + v.secs,
                },
            ))
            .id(),
    )
}

/// `/obj/effect/temp_visual/telegraphing` subtypes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Telegraph {
    /// `telegraphing`: the holographic target box, 2 s.
    Box,
    /// `telegraphing/vending_machine_tilt`: the box, 1 s.
    VendingTilt,
    /// `telegraphing/lift_travel` and `vending_machine_tilt` with a set duration (deciseconds
    /// in tg, seconds here).
    BoxFor(f64),
    /// `telegraphing/circle`, 2 s, and `circle/short`, 1 s.
    Circle,
    CircleShort,
    /// `telegraphing/line`, 1.2 s, and `line/short`, 0.5 s.
    Line,
    LineShort,
}

impl Telegraph {
    /// The visual: `BELOW_MOB_LAYER` on the game plane, turned at random (`randomdir`).
    pub fn visual(self) -> Visual {
        const HOLO: &str = "icons/mob/telegraphing/telegraph_holographic.dmi";
        const PLAIN: &str = "icons/mob/telegraphing/telegraph.dmi";
        let (icon, state, secs) = match self {
            Self::Box => (HOLO, "target_box", 2.0),
            Self::VendingTilt => (HOLO, "target_box", 1.0),
            Self::BoxFor(secs) => (HOLO, "target_box", secs),
            Self::Circle => (PLAIN, "target_circle", 2.0),
            Self::CircleShort => (PLAIN, "target_circle", 1.0),
            Self::Line => (PLAIN, "line", 1.2),
            Self::LineShort => (PLAIN, "line", 0.5),
        };
        Visual {
            layer: BELOW_MOB_LAYER,
            ..Visual::new(icon, state, secs)
        }
    }
}

/// `new /obj/effect/temp_visual/telegraphing/...(pos)`.
// shortcut: no `light_range = 1` and no emissive overlay (alpha 90): the box does not glow in
// the dark; add both when temp visuals get lights.
pub fn telegraph(world: &mut World, pos: TilePos, kind: Telegraph) -> Option<Entity> {
    spawn(world, pos, kind.visual())
}

/// `QDEL_IN(src, duration)`.
pub(super) fn expire_visuals(
    time: Res<Time>,
    visuals: Query<(Entity, &TempVisual)>,
    mut commands: Commands,
) {
    let now = time.elapsed_secs_f64();
    for (e, v) in &visuals {
        if now >= v.until {
            commands.entity(e).despawn();
        }
    }
}

/// `angle2dir(degrees)`: the direction nearest a clockwise-from-north angle.
pub fn angle_dir(degrees: f32) -> Dir {
    const BY_OCTANT: [Dir; 8] = [
        Dir::North,
        Dir::NorthEast,
        Dir::East,
        Dir::SouthEast,
        Dir::South,
        Dir::SouthWest,
        Dir::West,
        Dir::NorthWest,
    ];
    let octant = ((degrees.rem_euclid(360.0) + 22.5) / 45.0) as usize % 8;
    BY_OCTANT[octant]
}

/// `bloodsplatter/Initialize(set_dir)`: `splatter1|2|6` for a diagonal, `3|4|5` for a cardinal;
/// below mobs, above them when it flies south (toward the viewer).
pub fn splatter_look(dir: Dir, roll: u32) -> (&'static str, f32) {
    let diagonal = !Dir::CARDINALS.contains(&dir);
    let states = if diagonal {
        ["splatter1", "splatter2", "splatter6"]
    } else {
        ["splatter3", "splatter4", "splatter5"]
    };
    let layer = if matches!(dir, Dir::South | Dir::SouthEast | Dir::SouthWest) {
        ABOVE_MOB_LAYER
    } else {
        BELOW_MOB_LAYER
    };
    (states[roll as usize % 3], layer)
}

/// `/mob/living/create_splatter(dir)`: a 0.5 s blood splatter on the mob's tile, if it bleeds.
pub fn blood_splatter(world: &mut World, mob: Entity, dir: Dir) {
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    // `can_bleed()`: blood in the veins, not `TRAIT_NOBLOOD`.
    let bleeds = world
        .get::<crate::medical::blood::Blood>(mob)
        .is_some_and(|b| b.volume > 0.0)
        && !crate::species::has_trait(world, mob, crate::species::Trait::NoBlood);
    if !bleeds {
        return;
    }
    let roll = world.resource_mut::<crate::combat::CombatRng>().range(0, 2);
    let (state, layer) = splatter_look(dir, roll);
    let mut v = Visual::new(BLOOD_DMI, state, 0.5);
    v.layer = layer;
    v.color = Some(BLOOD_RED);
    v.dir = Some(dir);
    spawn(world, pos, v);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn angles_round_to_the_nearest_direction() {
        assert_eq!(angle_dir(0.0), Dir::North);
        assert_eq!(angle_dir(350.0), Dir::North);
        assert_eq!(angle_dir(40.0), Dir::NorthEast);
        assert_eq!(angle_dir(90.0), Dir::East);
        assert_eq!(angle_dir(-90.0), Dir::West);
        assert_eq!(angle_dir(200.0), Dir::South);
    }

    #[test]
    fn telegraphs_keep_tg_icons_and_durations() {
        let v = Telegraph::Box.visual();
        assert_eq!(
            (v.icon, v.state, v.secs, v.layer),
            (
                "icons/mob/telegraphing/telegraph_holographic.dmi",
                "target_box",
                2.0,
                BELOW_MOB_LAYER
            )
        );
        assert_eq!(Telegraph::VendingTilt.visual().secs, 1.0);
        assert_eq!(Telegraph::BoxFor(0.7).visual().secs, 0.7);
        let c = Telegraph::CircleShort.visual();
        assert_eq!((c.state, c.secs), ("target_circle", 1.0));
        assert_eq!(Telegraph::Line.visual().secs, 1.2);
        assert_eq!(Telegraph::LineShort.visual().state, "line");
        assert_eq!(c.plane, GAME_PLANE);
    }

    #[test]
    fn splatter_states_follow_the_direction() {
        assert_eq!(splatter_look(Dir::North, 0), ("splatter3", BELOW_MOB_LAYER));
        assert_eq!(splatter_look(Dir::South, 2), ("splatter5", ABOVE_MOB_LAYER));
        assert_eq!(
            splatter_look(Dir::NorthEast, 1),
            ("splatter2", BELOW_MOB_LAYER)
        );
        assert_eq!(
            splatter_look(Dir::SouthWest, 2),
            ("splatter6", ABOVE_MOB_LAYER)
        );
    }
}
