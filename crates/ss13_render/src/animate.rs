//! BYOND `flick` and `animate` on drawn entities ([`ss13_shared::animation`]).
//!
//! A [`Flick`] swaps the base sprite's state in [`crate::sprites::sync_appearance`] until it
//! ends. [`TransformAnims`] moves, turns and scales the whole entity (overlays are children):
//! its pose is put on the `Transform` after everything else placed the sprite this frame
//! (tile, glide, appearance turn, camera follow), and taken off again at the start of the next,
//! so no other system ever sees it and the camera does not shake with its target.

use bevy::prelude::*;
use ss13_shared::animation::{
    AnimationStep, EaseCurve, EaseMode, Easing, MAX_ANIMATION_STEPS, TransformAnimation,
};
use ss13_shared::{Appearance, IconStateId, TILE_SIZE};

/// tg `flick`: the base sprite shows `state` until `until` (`Time::elapsed_secs_f64`).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Flick {
    pub state: IconStateId,
    pub until: f64,
}

impl Flick {
    /// The state to draw at `now`, while it runs.
    pub fn state_at(&self, now: f64) -> Option<IconStateId> {
        (now < self.until).then_some(self.state)
    }
}

/// Most transform animations an entity runs at once; the oldest goes first.
const MAX_TRACKS: usize = 8;
/// Longest step the client plays, deciseconds.
const MAX_STEP_DS: f32 = 600.0;

/// Running transform animations of an entity.
#[derive(Component, Clone, Debug, Default)]
pub struct TransformAnims {
    tracks: Vec<Track>,
    /// The rest transform while a pose is on the entity's `Transform`.
    rest: Option<Transform>,
}

#[derive(Clone, Debug)]
struct Track {
    anim: TransformAnimation,
    start: f64,
}

impl TransformAnims {
    /// Starts `anim` at `now`, alongside the running ones. Over-long or empty animations are
    /// dropped and times are bounded: a bad message cannot freeze a sprite off its tile.
    pub fn play(&mut self, mut anim: TransformAnimation, now: f64) {
        if anim.steps.is_empty() || anim.steps.len() > MAX_ANIMATION_STEPS {
            return;
        }
        for step in &mut anim.steps {
            step.time_ds = if step.time_ds.is_finite() {
                step.time_ds.clamp(0.0, MAX_STEP_DS)
            } else {
                0.0
            };
        }
        if self.tracks.len() >= MAX_TRACKS {
            self.tracks.remove(0);
        }
        self.tracks.push(Track { anim, start: now });
    }

    /// BYOND `animate(atom)`: every animation ends, the entity is back at rest.
    pub fn stop(&mut self) {
        self.tracks.clear();
    }

    /// The summed pose of the running animations at `now`; finished ones are dropped.
    fn pose(&mut self, now: f64) -> Pose {
        let mut total = Pose::REST;
        self.tracks
            .retain(|t| match sample(&t.anim, (now - t.start) as f32) {
                Some(p) => {
                    total = total.then(p);
                    true
                }
                None => false,
            });
        total
    }
}

/// A pose relative to rest.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub offset: Vec2,
    /// Clockwise degrees.
    pub turn: f32,
    pub scale: Vec2,
}

impl Pose {
    pub const REST: Self = Self {
        offset: Vec2::ZERO,
        turn: 0.0,
        scale: Vec2::ONE,
    };

    fn of(step: &AnimationStep) -> Self {
        Self {
            offset: Vec2::from(step.offset),
            turn: step.turn,
            scale: Vec2::from(step.scale),
        }
    }

    fn lerp(self, to: Self, t: f32) -> Self {
        Self {
            offset: self.offset.lerp(to.offset, t),
            turn: self.turn + (to.turn - self.turn) * t,
            scale: self.scale.lerp(to.scale, t),
        }
    }

    /// Parallel animations: offsets and turns add, scales multiply.
    fn then(self, other: Self) -> Self {
        Self {
            offset: self.offset + other.offset,
            turn: self.turn + other.turn,
            scale: self.scale * other.scale,
        }
    }
}

/// The pose of `anim` `elapsed` seconds after it started; `None` once it is over.
pub fn sample(anim: &TransformAnimation, elapsed: f32) -> Option<Pose> {
    let cycle: f32 = anim.steps.iter().map(|s| s.time_ds).sum::<f32>() / 10.0;
    let elapsed = elapsed.max(0.0);
    if cycle <= 0.0 || (anim.loops > 0 && elapsed >= cycle * f32::from(anim.loops)) {
        return None;
    }
    let mut t = elapsed % cycle;
    let mut from = Pose::REST;
    for step in &anim.steps {
        let to = Pose::of(step);
        let secs = step.time_ds / 10.0;
        if t < secs {
            return Some(from.lerp(to, ease(step.easing, t / secs)));
        }
        t -= secs;
        from = to;
    }
    Some(from)
}

/// BYOND easing of progress `t` in `[0, 1]`.
pub fn ease(easing: Easing, t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let f = |t: f32| ease_in(easing.curve, t);
    match easing.mode {
        EaseMode::In => f(t),
        EaseMode::Out => 1.0 - f(1.0 - t),
        EaseMode::InOut if t < 0.5 => f(2.0 * t) / 2.0,
        EaseMode::InOut => 1.0 - f(2.0 - 2.0 * t) / 2.0,
    }
}

/// The `EASE_IN` form of each curve (Penner's equations); the others mirror it.
fn ease_in(curve: EaseCurve, t: f32) -> f32 {
    use core::f32::consts::{FRAC_PI_2, TAU};
    match curve {
        EaseCurve::Linear => t,
        EaseCurve::Sine => 1.0 - (t * FRAC_PI_2).cos(),
        EaseCurve::Quad => t * t,
        EaseCurve::Cubic => t * t * t,
        EaseCurve::Circular => 1.0 - (1.0 - t * t).max(0.0).sqrt(),
        EaseCurve::Back => {
            const C1: f32 = 1.70158;
            (C1 + 1.0) * t * t * t - C1 * t * t
        }
        EaseCurve::Bounce => 1.0 - bounce_out(1.0 - t),
        EaseCurve::Elastic if t <= 0.0 || t >= 1.0 => t,
        EaseCurve::Elastic => {
            -(2f32.powf(10.0 * t - 10.0)) * ((t * 10.0 - 10.75) * (TAU / 3.0)).sin()
        }
        // Stays at the start, then jumps to the end.
        EaseCurve::Jump => {
            if t >= 1.0 {
                1.0
            } else {
                0.0
            }
        }
    }
}

fn bounce_out(t: f32) -> f32 {
    const N: f32 = 7.5625;
    const D: f32 = 2.75;
    if t < 1.0 / D {
        N * t * t
    } else if t < 2.0 / D {
        let t = t - 1.5 / D;
        N * t * t + 0.75
    } else if t < 2.5 / D {
        let t = t - 2.25 / D;
        N * t * t + 0.9375
    } else {
        let t = t - 2.625 / D;
        N * t * t + 0.984375
    }
}

/// `rest` with `pose` on it: turned and scaled around the tile's center, then offset in screen
/// pixels. z stays, so draw order does not change.
// shortcut: turns and scales around the 32x32 tile center like `sprites::turn`; the scale is in
// the sprite's own frame, which for a sprite already turned by its appearance (a lying mob) and
// a non-uniform scale differs from BYOND's screen-space matrix. Compose full 2D matrices if
// squash animations of lying mobs ever show it.
pub fn posed(rest: &Transform, pose: Pose) -> Transform {
    let center = Vec3::splat(TILE_SIZE as f32 / 2.0).with_z(0.0);
    let turn = Quat::from_rotation_z(-pose.turn.to_radians());
    let rotation = turn * rest.rotation;
    let scale = rest.scale * pose.scale.extend(1.0);
    // The tile center in the entity's local frame at rest, and where it must land now.
    let pivot = (rest.rotation.inverse() * center) / rest.scale;
    let target = rest.translation.with_z(0.0) + center + pose.offset.extend(0.0);
    let translation = (target - rotation * (scale * pivot)).with_z(rest.translation.z);
    Transform {
        translation,
        rotation,
        scale,
    }
}

/// `First`: takes last frame's poses off, so this frame's systems see the rest transform.
pub fn unpose(mut posed: Query<(&mut TransformAnims, &mut Transform)>) {
    for (mut anims, mut transform) in &mut posed {
        if let Some(rest) = anims.rest.take() {
            *transform = rest;
        }
    }
}

/// `PostUpdate`, after the sprite is placed: puts the current pose on; drops the component
/// once nothing runs.
pub fn pose(
    time: Res<Time>,
    mut anims: Query<(Entity, &mut TransformAnims, &mut Transform)>,
    mut commands: Commands,
) {
    let now = time.elapsed_secs_f64();
    for (entity, mut anim, mut transform) in &mut anims {
        let pose = anim.pose(now);
        if anim.tracks.is_empty() {
            commands.entity(entity).try_remove::<TransformAnims>();
            continue;
        }
        anim.rest = Some(*transform);
        *transform = posed(&transform, pose);
    }
}

/// Ends flicks that ran out: the sprite is rebuilt with the appearance's own state.
pub fn end_flicks(
    time: Res<Time>,
    mut flicks: Query<(Entity, &Flick, Option<&mut Appearance>)>,
    mut commands: Commands,
) {
    let now = time.elapsed_secs_f64();
    for (entity, flick, appearance) in &mut flicks {
        if flick.state_at(now).is_none() {
            commands.entity(entity).try_remove::<Flick>();
            if let Some(mut a) = appearance {
                a.set_changed();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(offset: [f32; 2], turn: f32, time_ds: f32) -> AnimationStep {
        AnimationStep {
            offset,
            turn,
            scale: [1.0, 1.0],
            time_ds,
            easing: Easing::LINEAR,
        }
    }

    #[test]
    fn steps_go_from_pose_to_pose_and_loop_from_rest() {
        let anim = TransformAnimation {
            steps: vec![step([4.0, 0.0], 0.0, 2.0), step([0.0, 0.0], 0.0, 2.0)],
            loops: 2,
        };
        let at = |secs: f32| sample(&anim, secs).map(|p| p.offset.x);
        assert_eq!(at(0.0), Some(0.0));
        assert!((at(0.1).unwrap() - 2.0).abs() < 1e-4);
        assert!((at(0.2).unwrap() - 4.0).abs() < 1e-4);
        assert!((at(0.3).unwrap() - 2.0).abs() < 1e-4);
        // Second loop, from rest again.
        assert!((at(0.5).unwrap() - 2.0).abs() < 1e-4);
        assert_eq!(at(0.8), None);
        let forever = TransformAnimation { loops: 0, ..anim };
        assert!(sample(&forever, 1000.1).is_some());
    }

    #[test]
    fn parallel_animations_add_up_and_finished_ones_drop() {
        let mut anims = TransformAnims::default();
        anims.play(
            TransformAnimation {
                steps: vec![step([2.0, 0.0], 10.0, 1.0), step([2.0, 0.0], 10.0, 9.0)],
                loops: 1,
            },
            0.0,
        );
        anims.play(
            TransformAnimation {
                steps: vec![step([0.0, 3.0], 5.0, 1.0), step([0.0, 3.0], 5.0, 1.0)],
                loops: 1,
            },
            0.0,
        );
        let both = anims.pose(0.15);
        assert!((both.offset - Vec2::new(2.0, 3.0)).length() < 1e-4);
        assert!((both.turn - 15.0).abs() < 1e-4);
        let one = anims.pose(0.5);
        assert!((one.offset - Vec2::new(2.0, 0.0)).length() < 1e-4);
        assert_eq!(anims.tracks.len(), 1);
        anims.play(
            TransformAnimation {
                steps: vec![],
                loops: 1,
            },
            0.0,
        );
        assert_eq!(anims.tracks.len(), 1, "an empty animation is dropped");
    }

    #[test]
    fn easing_ends_where_it_should() {
        use EaseCurve::*;
        for curve in [
            Linear, Sine, Quad, Cubic, Circular, Back, Bounce, Elastic, Jump,
        ] {
            for mode in [EaseMode::In, EaseMode::Out, EaseMode::InOut] {
                let e = Easing::with(curve, mode);
                assert!(ease(e, 0.0).abs() < 1e-4, "{curve:?} {mode:?} starts at 0");
                assert!(
                    (ease(e, 1.0) - 1.0).abs() < 1e-4,
                    "{curve:?} {mode:?} ends at 1"
                );
            }
        }
        // `BACK_EASING|EASE_IN` first pulls back.
        assert!(ease(Easing::with(Back, EaseMode::In), 0.2) < 0.0);
        assert!((ease(Easing::of(Sine), 0.5) - 0.5).abs() < 1e-4);
    }

    #[test]
    fn a_pose_turns_around_the_tile_center_and_offsets_in_screen_space() {
        let center = Vec3::new(16.0, 16.0, 0.0);
        let rest = Transform::from_xyz(64.0, 32.0, 5.0);
        let pose = Pose {
            offset: Vec2::new(3.0, -2.0),
            turn: 90.0,
            scale: Vec2::splat(2.0),
        };
        let t = posed(&rest, pose);
        // The tile center lands offset, nothing else moves it; z is kept.
        let drawn = t.transform_point(center);
        assert!((drawn - Vec3::new(64.0 + 16.0 + 3.0, 32.0 + 16.0 - 2.0, 5.0)).length() < 1e-3);
        // Clockwise: the sprite's up points east.
        assert!((t.rotation * Vec3::Y - Vec3::X).length() < 1e-5);
        assert_eq!(t.scale, Vec3::new(2.0, 2.0, 1.0));
        assert_eq!(posed(&rest, Pose::REST), rest);
    }

    #[test]
    fn a_pose_on_a_turned_sprite_keeps_its_center() {
        // A lying mob: the entity sits on the tile corner, turned; its anchors put the local
        // point `rotation^-1 * center` on the tile center (`sprites::turn`).
        let rotation = Quat::from_rotation_z(-90f32.to_radians());
        let center = Vec3::new(16.0, 16.0, 0.0);
        let rest = Transform::from_xyz(0.0, 0.0, 1.0).with_rotation(rotation);
        let local_center = rotation.inverse() * center;
        let t = posed(
            &rest,
            Pose {
                offset: Vec2::new(1.0, 0.0),
                turn: 15.0,
                scale: Vec2::splat(1.5),
            },
        );
        let drawn = t.transform_point(local_center);
        assert!((drawn - Vec3::new(17.0, 16.0, 1.0)).length() < 1e-3);
    }
}
