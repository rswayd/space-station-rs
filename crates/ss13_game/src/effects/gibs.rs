//! Gib spawners (server), after tg `/obj/effect/gibspawner`
//! (`code/game/objects/effects/spawners/gibspawner.dm`): a gibbed body throws its gib decals
//! about. Each kind spawns its `gibtypes` on the tile with the `blobattack` squelch; each gib
//! with directions streaks off (`gibs/streak`): it takes `pick(0 = 200, 1 = 150, 2 = 50,
//! 3 = 17, 50 = 1)` more steps one way among its directions, leaving a blood splatter on every
//! tile it passes, and stops at what blocks it (a step that cannot even start leaves it home).
//! Robot debris throws sparks.
//!
//! The decals are runtime types (`ss13_mapload::runtime::GAME_RUNTIME_TYPES`). Callers:
//! `crate::health::gib` (human gibs for a carbon, generic ones for other mobs).
// shortcut: the streak lands at once (tg glides it a step per 0.2 s); no blood DNA or
// diseases on the gibs; splatters also fall on groundless tiles.

use bevy::prelude::*;
use ss13_shared::{Dir, TilePos};

use super::{Pass, step_blocked};
use crate::combat::CombatRng;

const GIBS: &str = "/obj/effect/decal/cleanable/blood/gibs";
const GIBS_UP: &str = "/obj/effect/decal/cleanable/blood/gibs/up";
const GIBS_DOWN: &str = "/obj/effect/decal/cleanable/blood/gibs/down";
const GIBS_BODY: &str = "/obj/effect/decal/cleanable/blood/gibs/body";
const GIBS_LIMB: &str = "/obj/effect/decal/cleanable/blood/gibs/limb";
const GIBS_CORE: &str = "/obj/effect/decal/cleanable/blood/gibs/core";
const ROBOT: &str = "/obj/effect/decal/cleanable/blood/gibs/robot_debris";
const ROBOT_UP: &str = "/obj/effect/decal/cleanable/blood/gibs/robot_debris/up";
const ROBOT_DOWN: &str = "/obj/effect/decal/cleanable/blood/gibs/robot_debris/down";
const ROBOT_LIMB: &str = "/obj/effect/decal/cleanable/blood/gibs/robot_debris/limb";
const SPLATTER: &str = "/obj/effect/decal/cleanable/blood/splatter";
const SOUND: &str = "sound/effects/blob/blobattack.ogg";

const N: Dir = Dir::North;
const S: Dir = Dir::South;
const E: Dir = Dir::East;
const W: Dir = Dir::West;
const NE: Dir = Dir::NorthEast;
const NW: Dir = Dir::NorthWest;
const SE: Dir = Dir::SouthEast;
const SW: Dir = Dir::SouthWest;
const ALL: &[Dir] = &Dir::DMI_ORDER;
const NORTHS: &[Dir] = &[N, NE, NW];
const SOUTHS: &[Dir] = &[S, SE, SW];
const WESTS: &[Dir] = &[W, NW, SW];
const EASTS: &[Dir] = &[E, NE, SE];
const GENERIC_WEST: &[Dir] = &[W, NW, SW, N];
const GENERIC_EAST: &[Dir] = &[E, NE, SE, S];

/// A gib spawner type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gibspawner {
    /// `gibspawner/generic` (and `generic/animal`).
    Generic,
    /// `gibspawner/human`.
    Human,
    /// `gibspawner/robot`.
    Robot,
}

impl Gibspawner {
    /// `gibtypes` (one of each, in order) with their `gibdirections`, and `sound_vol`.
    fn table(self, rng: &mut CombatRng) -> (Vec<(&'static str, &'static [Dir])>, f32) {
        match self {
            // `list(gibs = 2, gibs = 2, core = 1)`: the duplicate key keeps one entry of 2.
            Self::Generic => (
                vec![
                    (GIBS, GENERIC_WEST),
                    (GIBS, GENERIC_WEST),
                    (GIBS_CORE, GENERIC_EAST),
                ],
                40.0,
            ),
            // The assoc list dedupes `gibs` too: up, down, gibs, body, limb, core.
            Self::Human => (
                vec![
                    (GIBS_UP, NORTHS),
                    (GIBS_DOWN, SOUTHS),
                    (GIBS, WESTS),
                    (GIBS_BODY, EASTS),
                    (GIBS_LIMB, ALL),
                    (GIBS_CORE, ALL),
                ],
                50.0,
            ),
            Self::Robot => {
                let mut list = vec![(ROBOT_UP, NORTHS), (ROBOT_DOWN, SOUTHS), (ROBOT, WESTS)];
                // `gibtypes[limb] = pick(0, 1, 2)`.
                for _ in 0..rng.range(0, 2) {
                    list.push((ROBOT_LIMB, EASTS));
                }
                (list, 60.0)
            }
        }
    }
}

/// `pick(0;200, 1;150, 2;50, 3;17, 50;1)` from a roll in `0..418`.
pub fn streak_range(roll: u32) -> u32 {
    match roll {
        0..200 => 0,
        200..350 => 1,
        350..400 => 2,
        400..417 => 3,
        _ => 50,
    }
}

/// `new /obj/effect/gibspawner/<kind>(pos)`.
pub fn spawn_gibs(world: &mut World, kind: Gibspawner, pos: TilePos) {
    let (gibs, vol) = kind.table(&mut world.resource_mut::<CombatRng>());
    crate::sound::playsound(world, crate::sound::PlaySound::new(pos, SOUND, vol, true));
    for (path, dirs) in gibs {
        let mut at = pos;
        if !dirs.is_empty() {
            let (dir, range) = {
                let mut rng = world.resource_mut::<CombatRng>();
                let dir = dirs[rng.range(0, dirs.len() as u32 - 1) as usize];
                (dir, streak_range(rng.range(0, 417)))
            };
            at = streak(world, pos, dir, range);
        }
        crate::construction::spawn_obj(world, path, at);
    }
    if kind == Gibspawner::Robot {
        super::particles::do_sparks(world, 2, true, pos);
    }
}

/// `gibs/streak`: the first step (none if blocked), then `range` more, a splatter left on each
/// tile before stepping off it. Where the gib ends up.
fn streak(world: &mut World, from: TilePos, dir: Dir, range: u32) -> TilePos {
    if step_blocked(world, from, dir, Pass::Particle) {
        return from;
    }
    let mut at = from.step(dir);
    for _ in 0..range {
        crate::construction::spawn_obj(world, SPLATTER, at);
        if step_blocked(world, at, dir, Pass::Particle) {
            break;
        }
        at = at.step(dir);
    }
    at
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streak_ranges_follow_the_weights() {
        assert_eq!(streak_range(0), 0);
        assert_eq!(streak_range(199), 0);
        assert_eq!(streak_range(200), 1);
        assert_eq!(streak_range(349), 1);
        assert_eq!(streak_range(399), 2);
        assert_eq!(streak_range(416), 3);
        assert_eq!(streak_range(417), 50);
    }

    #[test]
    fn human_gibs_are_the_deduplicated_list() {
        let (list, vol) = Gibspawner::Human.table(&mut CombatRng(1));
        assert_eq!(list.len(), 6);
        assert_eq!(vol, 50.0);
        assert!(list.iter().all(|(_, d)| !d.is_empty()));
    }
}
