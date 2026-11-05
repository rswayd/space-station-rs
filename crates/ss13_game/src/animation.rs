//! Server side of BYOND `flick` and `animate` (`ss13_shared::animation`): game code plays a
//! visual on an entity, `ss13_net` sends it to the clients that see the entity now.
//!
//! - [`flick`]: tg `flick(state, atom)` of a state of the atom's own icon.
//! - [`do_attack_animation`]: tg `/atom/movable/do_attack_animation`'s lunge and tilt.
//! - [`do_jitter_animation`]: tg `/mob/living/do_jitter_animation`.
// shortcut: no `do_item_attack_animation` (the weapon's image flying at the target), no
// floating bob (`DO_FLOATING_ANIM` loops forever: a client that sees the mob later would need
// it as replicated state, not as an event), no `Shake`, spin or fade animations yet.

use bevy::ecs::message::Messages;
use bevy::prelude::*;
use ss13_mapload::DmAtom;
use ss13_mapload::fidelity::IconIndex;
use ss13_net::animation::server::PlayAnimation;
use ss13_shared::animation::{
    Animation, AnimationStep, EaseCurve, EaseMode, Easing, TransformAnimation,
};
use ss13_shared::{Dir, IconStateId, TilePos};

/// Rolls of purely visual randomness (jitter offsets, the attack tilt), kept off the game's RNG
/// streams so that animations never shift a combat or status roll.
#[derive(Resource, Clone, Copy, Debug)]
pub struct AnimationRng(pub u32);

impl Default for AnimationRng {
    fn default() -> Self {
        Self(0x9E37_79B9)
    }
}

impl AnimationRng {
    /// Uniform in `[-1, 1]` (xorshift32).
    pub fn signed_unit(&mut self) -> f32 {
        let mut x = self.0 | 1;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x as f32 / u32::MAX as f32 * 2.0 - 1.0
    }
}

/// Plays `animation` on `entity` for the clients that see it. Nothing without the network
/// (unit-test worlds).
pub fn play(world: &mut World, entity: Entity, animation: Animation) {
    if world.contains_resource::<Messages<PlayAnimation>>() {
        world.write_message(PlayAnimation { entity, animation });
    }
}

/// tg `flick(state, atom)`: `state` of the atom's own icon, for one cycle of its frames.
/// False when the atom has no such state.
pub fn flick(world: &mut World, entity: Entity, state: &str) -> bool {
    let Some(id) = own_state(world, entity, state) else {
        return false;
    };
    play(
        world,
        entity,
        Animation::Flick {
            state: id,
            duration_ds: None,
        },
    );
    true
}

fn own_state(world: &World, entity: Entity, state: &str) -> Option<IconStateId> {
    let icon = &world.get::<DmAtom>(entity)?.icon;
    world.get_resource::<IconIndex>()?.get(icon, state)
}

/// tg `do_attack_animation`'s pixel offset toward the target, 8 pixels per axis.
const ATTACK_LUNGE_PX: f32 = 8.0;
/// `transform.Turn(15 * turn_dir)`.
const ATTACK_TILT_DEGREES: f32 = 15.0;

/// tg `do_attack_animation(attacked_atom)` of `attacker` toward `target`: nothing on itself.
pub fn do_attack_animation(world: &mut World, attacker: Entity, target: Entity) {
    if attacker == target {
        return;
    }
    let (Some(&from), Some(&to)) = (world.get::<TilePos>(attacker), world.get::<TilePos>(target))
    else {
        return;
    };
    let flip = world
        .get_resource_or_init::<AnimationRng>()
        .signed_unit()
        .is_sign_negative();
    let anim = attack_animation(dir_bits(from, to), flip);
    play(world, attacker, Animation::Transform(anim));
}

/// BYOND `get_dir` bits from `from` to `to`; 0 on the same tile.
fn dir_bits(from: TilePos, to: TilePos) -> u8 {
    let (dx, dy) = ((to.x - from.x).signum(), (to.y - from.y).signum());
    let mut bits = 0;
    if dy > 0 {
        bits |= Dir::North as u8;
    } else if dy < 0 {
        bits |= Dir::South as u8;
    }
    if dx > 0 {
        bits |= Dir::East as u8;
    } else if dx < 0 {
        bits |= Dir::West as u8;
    }
    bits
}

/// The lunge of tg `do_attack_animation` toward `dir` (BYOND bits): out over 1 decisecond
/// (`BACK_EASING|EASE_IN`) tilted 15 degrees, back over 2 (`SINE_EASING`). `flip` is tg's
/// `prob(50)` tilt the other way on a north or south attack.
pub fn attack_animation(dir: u8, flip: bool) -> TransformAnimation {
    let (mut x, mut y, mut turn_dir) = (0.0, 0.0, 1.0);
    let vertical = dir & (Dir::North as u8 | Dir::South as u8);
    if vertical != 0 {
        y = if dir & Dir::North as u8 != 0 {
            ATTACK_LUNGE_PX
        } else {
            -ATTACK_LUNGE_PX
        };
        if flip {
            turn_dir = -1.0;
        }
    }
    if dir & Dir::East as u8 != 0 {
        x = ATTACK_LUNGE_PX;
    } else if dir & Dir::West as u8 != 0 {
        x = -ATTACK_LUNGE_PX;
        turn_dir = -1.0;
    }
    TransformAnimation {
        steps: vec![
            AnimationStep {
                offset: [x, y],
                turn: ATTACK_TILT_DEGREES * turn_dir,
                scale: [1.0, 1.0],
                time_ds: 1.0,
                easing: Easing::with(EaseCurve::Back, EaseMode::In),
            },
            AnimationStep {
                easing: Easing::of(EaseCurve::Sine),
                ..AnimationStep::rest(2.0)
            },
        ],
        loops: 1,
    }
}

/// tg `do_jitter_animation(jitter_amount)` on `mob`.
pub fn do_jitter_animation(world: &mut World, mob: Entity, jitter_amount: f32) {
    let rolls = {
        let mut rng = world.get_resource_or_init::<AnimationRng>();
        [rng.signed_unit(), rng.signed_unit()]
    };
    play(
        world,
        mob,
        Animation::Transform(jitter_animation(jitter_amount, rolls)),
    );
}

/// tg `do_jitter_animation`: `amplitude = min(4, jitter_amount / 100 + 1)`, a random shake up to
/// it sideways and a third of it vertically, out and back over 0.2 s each, 6 times
/// (`ANIMATION_RELATIVE|ANIMATION_PARALLEL`). `rolls` are the two `rand` draws in `[-1, 1]`.
// BYOND draws at whole pixels anyway, so the fractional `rand` of a fractional amplitude is
// kept as is.
pub fn jitter_animation(jitter_amount: f32, rolls: [f32; 2]) -> TransformAnimation {
    let amplitude = (jitter_amount / 100.0 + 1.0).min(4.0);
    let offset = [rolls[0] * amplitude, rolls[1] * amplitude / 3.0];
    TransformAnimation {
        steps: vec![
            AnimationStep {
                offset,
                ..AnimationStep::rest(2.0)
            },
            AnimationStep::rest(2.0),
        ],
        loops: 6,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_attack_lunges_toward_the_target_and_comes_back() {
        let east = attack_animation(Dir::East as u8, true);
        assert_eq!(east.steps[0].offset, [8.0, 0.0]);
        assert_eq!(east.steps[0].turn, 15.0, "east never flips");
        assert_eq!(east.steps[1].offset, [0.0, 0.0]);
        assert_eq!(
            (east.steps[0].time_ds, east.steps[1].time_ds, east.loops),
            (1.0, 2.0, 1)
        );
        let west = attack_animation(Dir::West as u8, false);
        assert_eq!(
            (west.steps[0].offset, west.steps[0].turn),
            ([-8.0, 0.0], -15.0)
        );
        let north = attack_animation(Dir::North as u8, true);
        assert_eq!(
            (north.steps[0].offset, north.steps[0].turn),
            ([0.0, 8.0], -15.0)
        );
        let south_west = attack_animation(Dir::SouthWest as u8, false);
        assert_eq!(
            (south_west.steps[0].offset, south_west.steps[0].turn),
            ([-8.0, -8.0], -15.0)
        );
        assert_eq!(
            dir_bits(TilePos::new(5, 5, 1), TilePos::new(6, 4, 1)),
            Dir::SouthEast as u8
        );
    }

    #[test]
    fn jitter_grows_with_the_time_left_up_to_four_pixels() {
        let mild = jitter_animation(20.0, [1.0, -1.0]);
        assert!((mild.steps[0].offset[0] - 1.2).abs() < 1e-5);
        assert!((mild.steps[0].offset[1] + 0.4).abs() < 1e-5);
        assert_eq!(mild.steps[1].offset, [0.0, 0.0]);
        assert_eq!(mild.loops, 6);
        let wild = jitter_animation(1000.0, [1.0, 1.0]);
        assert_eq!(wild.steps[0].offset[0], 4.0);
    }

    #[test]
    fn visual_rolls_stay_in_range() {
        let mut rng = AnimationRng::default();
        for _ in 0..1000 {
            let r = rng.signed_unit();
            assert!((-1.0..=1.0).contains(&r));
        }
    }
}
