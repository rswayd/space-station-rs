//! Server side of BYOND `flick` and `animate` (`ss13_shared::animation`): game code plays a
//! visual on an entity, `ss13_net` sends it to the clients that see the entity now.
//!
//! - [`flick`]: tg `flick(state, atom)` of a state of the atom's own icon.
//! - [`do_attack_animation`]: tg `/atom/movable/do_attack_animation`'s lunge and tilt.
//! - [`do_jitter_animation`]: tg `/mob/living/do_jitter_animation`.
//! - [`do_attack_animation_with`]: the lunge plus `do_item_attack_animation`: an
//!   `ATTACK_EFFECT_*` state of `icons/effects/effects.dmi` (a punch, a kick) or the weapon's own
//!   sprite (`animate_attack`: blunt, pierce or slash by its sharpness) swept over the target,
//!   as a temporary visual on the target's tile ([`crate::effects::temp_visual`]).
// shortcut: no floating bob (`DO_FLOATING_ANIM` loops forever: a client that sees the mob later
// would need it as replicated state, not as an event), no `Shake`, spin or fade animations
// yet. The attack visual does not fade (no alpha in `TransformAnimation`): it vanishes when
// tg's fade ends; items' `icon_angle` and `attack_icon` are not imported (sprites are turned as
// if drawn facing east).

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

/// `ATTACK_EFFECT_PUNCH`, `ATTACK_EFFECT_KICK` (`icons/effects/effects.dmi`).
pub const ATTACK_EFFECT_PUNCH: &str = "punch";
pub const ATTACK_EFFECT_KICK: &str = "kick";
const EFFECTS_DMI: &str = "icons/effects/effects.dmi";

/// What `do_item_attack_animation` shows over the target.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AttackVisual {
    /// `visual_effect_icon`: a state of `icons/effects/effects.dmi`.
    Effect(&'static str),
    /// `used_item`: the item's own sprite.
    Item(Entity),
}

/// `ATTACK_ANIMATION_BLUNT`, `PIERCE`, `SLASH` (`animate_attack` picks it by `get_sharpness`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttackSweep {
    Blunt,
    Pierce,
    Slash,
}

impl AttackSweep {
    pub fn of_sharpness(sharpness: u32) -> Self {
        use crate::medical::wounds::{SHARP_EDGED, SHARP_POINTY};
        if sharpness & SHARP_EDGED != 0 {
            Self::Slash
        } else if sharpness & SHARP_POINTY != 0 {
            Self::Pierce
        } else {
            Self::Blunt
        }
    }
}

/// tg `do_attack_animation(target, visual_effect_icon, used_item)`: the visual over the target
/// (also on oneself), then the lunge.
pub fn do_attack_animation_with(
    world: &mut World,
    attacker: Entity,
    target: Entity,
    visual: AttackVisual,
) {
    do_item_attack_animation(world, attacker, target, visual);
    do_attack_animation(world, attacker, target);
}

/// tg `do_item_attack_animation`.
fn do_item_attack_animation(
    world: &mut World,
    attacker: Entity,
    target: Entity,
    visual: AttackVisual,
) {
    use crate::effects::temp_visual::{ABOVE_MOB_LAYER, GAME_PLANE, TempVisual};
    let (Some(&from), Some(&to)) = (world.get::<TilePos>(attacker), world.get::<TilePos>(target))
    else {
        return;
    };
    let rolls = {
        let mut rng = world.get_resource_or_init::<AnimationRng>();
        [rng.signed_unit(), rng.signed_unit(), rng.signed_unit()]
    };
    let (look, dir, anims) = match visual {
        AttackVisual::Effect(state) => {
            let Some(icon) = world
                .get_resource::<IconIndex>()
                .and_then(|i| i.get(EFFECTS_DMI, state))
            else {
                return;
            };
            let look = ss13_shared::Appearance {
                icon_state: icon,
                layer: ss13_shared::Layer(0.0),
                plane: 0,
                color: None,
                pixel_offset: [0, 0],
                rotation: 0,
                overlays: vec![],
                mouse_opacity: ss13_shared::MouseOpacity::Transparent,
                glide: 0,
            };
            (look, Dir::South, vec![effect_sweep()])
        }
        AttackVisual::Item(item) => {
            let Some(look) = world.get::<ss13_shared::Appearance>(item).cloned() else {
                return;
            };
            let sharpness = world
                .get::<ss13_mapload::DmWounding>(item)
                .map_or(0, |w| w.sharpness);
            let dir = world
                .get::<ss13_shared::Facing>(item)
                .map_or(Dir::South, |f| f.0);
            let anims = item_sweep(
                AttackSweep::of_sharpness(sharpness),
                dir_bits(from, to),
                rolls,
            );
            (look, dir, anims)
        }
    };
    // The visual lasts until the last of its animations ends (tg's fade to alpha 0).
    let secs = anims
        .iter()
        .map(|a| a.steps.iter().map(|s| s.time_ds).sum::<f32>())
        .fold(0.0f32, f32::max) as f64
        / 10.0;
    let now = world.resource::<Time>().elapsed_secs_f64();
    let visual = world
        .spawn((
            Name::new("attack"),
            to,
            ss13_shared::Facing(dir),
            ss13_shared::Appearance {
                layer: ss13_shared::Layer(ABOVE_MOB_LAYER),
                plane: GAME_PLANE,
                pixel_offset: [0, 0],
                rotation: 0,
                mouse_opacity: ss13_shared::MouseOpacity::Transparent,
                ..look
            },
            ss13_net::server::Networked,
            TempVisual { until: now + secs },
        ))
        .id();
    for anim in anims {
        play(world, visual, Animation::Transform(anim));
    }
}

/// `do_item_attack_animation` of a `visual_effect_icon`: at 0.4 scale, grows to 0.75 over
/// 0.3 s, holds 0.1 s, fades over 0.3 s.
pub fn effect_sweep() -> TransformAnimation {
    let at = |scale: f32, time_ds: f32| AnimationStep {
        scale: [scale, scale],
        ..AnimationStep::rest(time_ds)
    };
    TransformAnimation {
        steps: vec![at(0.4, 0.0), at(0.75, 3.0), at(0.75, 1.0), at(0.75, 3.0)],
        loops: 1,
    }
}

/// BYOND `dir2angle` of `get_dir` bits: clockwise from north.
fn dir_angle(bits: u8) -> f32 {
    let (n, s, e, w) = (
        bits & Dir::North as u8 != 0,
        bits & Dir::South as u8 != 0,
        bits & Dir::East as u8 != 0,
        bits & Dir::West as u8 != 0,
    );
    match (n, s, e, w) {
        (true, _, true, _) => 45.0,
        (_, true, true, _) => 135.0,
        (_, true, _, true) => 225.0,
        (true, _, _, true) => 315.0,
        (_, _, true, _) => 90.0,
        (_, true, _, _) => 180.0,
        (_, _, _, true) => 270.0,
        _ => 0.0,
    }
}

/// tg `/obj/item/proc/animate_attack` toward `dir` (`get_dir(attacker, target)` bits): the
/// parallel animations of the item's image, from rest at the target's center. `rolls` are three
/// draws in `[-1, 1]` (tg's `prob(50)` and `rand(-7, 7)`).
pub fn item_sweep(sweep: AttackSweep, dir: u8, rolls: [f32; 3]) -> Vec<TransformAnimation> {
    let (mut x, mut y) = (0.0f32, 0.0f32);
    if dir & Dir::North as u8 != 0 {
        y = -1.0;
    } else if dir & Dir::South as u8 != 0 {
        y = 1.0;
    }
    if dir & Dir::East as u8 != 0 {
        x = -1.0;
    } else if dir & Dir::West as u8 != 0 {
        x = 1.0;
    }
    let mut dir = dir;
    // Attacking oneself or something on the same tile.
    if dir == 0 {
        y = 1.0;
        x = if rolls[0] < 0.0 { -0.25 } else { 0.25 };
        dir = Dir::South as u8;
    }
    let diagonal = x != 0.0 && y != 0.0;
    let step =
        |offset: [f32; 2], turn: f32, scale: f32, time_ds: f32, easing: Easing| AnimationStep {
            offset,
            turn,
            scale: [scale, scale],
            time_ds,
            easing,
        };
    let circ = |mode| Easing::with(EaseCurve::Circular, mode);
    match sweep {
        AttackSweep::Blunt => vec![TransformAnimation {
            steps: vec![
                step([14.0 * x, 12.0 * y], 0.0, 0.5, 0.0, Easing::LINEAR),
                step([4.0 * x, 3.0 * y], 0.0, 0.75, 2.0, Easing::LINEAR),
                step([4.0 * x, 3.0 * y], 0.0, 0.75, 1.0, Easing::LINEAR),
                step([4.0 * x, 3.0 * y], 0.0, 0.75, 1.0, Easing::LINEAR),
            ],
            loops: 1,
        }],
        AttackSweep::Pierce => {
            let angle = dir_angle(dir) + (rolls[1] * 7.0).round();
            let turn = angle - 90.0;
            let m = if diagonal { 1.4 } else { 1.0 };
            let (sin, cos) = (angle.to_radians().sin(), angle.to_radians().cos());
            let cubic = |mode| Easing::with(EaseCurve::Cubic, mode);
            vec![TransformAnimation {
                steps: vec![
                    step([22.0 * x * m, 18.0 * y * m], turn, 0.5, 0.0, Easing::LINEAR),
                    step(
                        [(22.0 * x - 12.0 * sin) * m, (18.0 * y - 8.0 * cos) * m],
                        turn,
                        0.5,
                        1.0,
                        cubic(EaseMode::In),
                    ),
                    step(
                        [(22.0 * x + 26.0 * sin) * m, (18.0 * y + 22.0 * cos) * m],
                        turn,
                        0.75,
                        3.0,
                        cubic(EaseMode::Out),
                    ),
                    step(
                        [3.0 * (x + sin), 2.0 * (y + cos)],
                        turn,
                        0.75,
                        1.0,
                        circ(EaseMode::Out),
                    ),
                ],
                loops: 1,
            }]
        }
        AttackSweep::Slash => {
            let mut attack_dir: f32 = if rolls[2] < 0.0 { -1.0 } else { 1.0 };
            let angle = dir_angle(dir) - 90.0;
            let (mut x_rot, mut y_rot) = (0.0f32, 0.0f32);
            if x != 0.0 {
                y_rot = attack_dir;
            }
            if y != 0.0 {
                x_rot = attack_dir;
            }
            if x > 0.0 || y < 0.0 {
                attack_dir = -attack_dir;
            }
            let mut anim_dir = attack_dir;
            if diagonal {
                if attack_dir < 0.0 {
                    x_rot = -x * 1.4;
                    y_rot = 0.0;
                } else {
                    x_rot = 0.0;
                    y_rot = -y * 1.4;
                }
                if (x < 0.0 && y > 0.0) || (x > 0.0 && y < 0.0) {
                    anim_dir = -anim_dir;
                }
            }
            let start = [18.0 * x + 10.0 * x_rot, 14.0 * y + 8.0 * y_rot];
            let mut x_return = if x_rot == 0.0 {
                18.0 * x
            } else {
                -10.0 * x_rot
            };
            let mut y_return = if y_rot == 0.0 { 14.0 * y } else { -8.0 * y_rot };
            let mut m = 1.0;
            if diagonal {
                m = 1.4;
                if attack_dir > 0.0 {
                    (x_return, y_return) = (8.0 * x, 14.0 * y);
                } else {
                    (x_return, y_return) = (18.0 * x, 6.0 * y);
                }
            }
            let (from_turn, to_turn) = (angle - 45.0 * anim_dir, angle + 45.0 * anim_dir);
            vec![
                // The swing: turned through 90 degrees while it grows, then held to the fade.
                TransformAnimation {
                    steps: vec![
                        step([0.0, 0.0], from_turn, 0.5, 0.0, Easing::LINEAR),
                        step([0.0, 0.0], to_turn, 0.75, 3.0, Easing::LINEAR),
                        step([0.0, 0.0], to_turn, 0.75, 2.0, Easing::LINEAR),
                    ],
                    loops: 1,
                },
                // The path: in toward the center, then out on the other side.
                TransformAnimation {
                    steps: vec![
                        step(start, 0.0, 1.0, 0.0, Easing::LINEAR),
                        step(
                            [4.0 * x * m, 3.0 * y * m],
                            0.0,
                            1.0,
                            2.0,
                            circ(EaseMode::In),
                        ),
                        step([x_return, y_return], 0.0, 1.0, 2.0, circ(EaseMode::Out)),
                        step([x_return, y_return], 0.0, 1.0, 1.0, Easing::LINEAR),
                    ],
                    loops: 1,
                },
            ]
        }
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
    fn item_sweeps_follow_animate_attack() {
        // A club from the west: from 14 px west of center to 4 px, growing 0.5 -> 0.75.
        let blunt = item_sweep(AttackSweep::Blunt, Dir::East as u8, [1.0; 3]);
        assert_eq!(blunt.len(), 1);
        assert_eq!(blunt[0].steps[0].offset, [-14.0, 0.0]);
        assert_eq!(blunt[0].steps[0].scale, [0.5, 0.5]);
        assert_eq!(blunt[0].steps[1].offset, [-4.0, 0.0]);
        assert_eq!(blunt[0].steps[1].scale, [0.75, 0.75]);
        // A spear thrust north, no jitter: turned 90 degrees anticlockwise (an east sprite),
        // from below the target through it.
        let pierce = item_sweep(AttackSweep::Pierce, Dir::North as u8, [1.0, 0.0, 1.0]);
        let steps = &pierce[0].steps;
        assert_eq!(steps[0].turn, -90.0);
        assert_eq!(steps[0].offset, [0.0, -18.0]);
        assert!((steps[1].offset[1] + 26.0).abs() < 1e-4);
        assert!((steps[2].offset[1] - 4.0).abs() < 1e-4);
        // A slash: a swing turning through 90 degrees and a path, in parallel.
        let slash = item_sweep(AttackSweep::Slash, Dir::South as u8, [1.0; 3]);
        assert_eq!(slash.len(), 2);
        let swing = (slash[0].steps[1].turn - slash[0].steps[0].turn).abs();
        assert_eq!(swing, 90.0);
        assert_eq!(slash[1].steps[0].offset[1], 14.0);
        // On oneself: from above.
        let own = item_sweep(AttackSweep::Blunt, 0, [-1.0; 3]);
        assert_eq!(own[0].steps[0].offset, [-3.5, 12.0]);
        assert_eq!(
            AttackSweep::of_sharpness(crate::medical::wounds::SHARP_EDGED),
            AttackSweep::Slash
        );
        assert_eq!(effect_sweep().steps[1].scale, [0.75, 0.75]);
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
