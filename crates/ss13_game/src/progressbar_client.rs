//! Client half of the timed action visuals ([`crate::progressbar`]): tg's progress bar over the
//! target of the player's own action and the cog over other busy mobs, as world sprites from
//! `icons/effects/progressbar.dmi`.
//!
//! - A bar rises from the target's tile to `ICON_SIZE_Y + PROGRESSBAR_HEIGHT * (index - 1)`
//!   while it fades in over `PROGRESSBAR_ANIMATION_TIME` (`SINE_EASING`), and shows
//!   `prog_bar_[round(progress * 100, 5)]`. The client runs the clock itself (tg updates the
//!   state every tick from the server), so it advances the same way without a message per step.
//! - On its end it keeps its state (`_fail` when the action stopped early) and fades out over
//!   the same time, then goes; the bars above it on that target slide down a row.
//! - A cog fades in over a busy mob at `ICON_SIZE_Y`, and out again when the action ends.
// shortcut: no `get_oversized_icon_offsets` (bars over a large icon sit at 32 px too), until
// big mobs get timed actions on them.
// shortcut: a 0 % or 100 % bar that fails keeps its plain state (the dmi has no `_fail` of
// them; BYOND then shows nothing).
// tg differences on purpose: the bar fills over the action's real time (tg's goal is the delay
// before `cached_multiplicative_actions_slowdown`, so a slowed action's bar sits full early).

use bevy::prelude::*;
use bevy::sprite::Anchor;
use ss13_net::animation::client::AnimationReceived;
use ss13_net::client::NetEntityMap;
use ss13_render::IconAtlas;
use ss13_render::animate::ease;
use ss13_render::atlas::frame_rect;
use ss13_render::order::Z_RANGE;
use ss13_shared::animation::{Animation, EaseCurve, Easing};
use ss13_shared::{Dir, IconStateId, TILE_SIZE};

pub const PROGRESSBAR_DMI: &str = "icons/effects/progressbar.dmi";
/// `PROGRESSBAR_HEIGHT`, pixels between stacked bars.
pub const HEIGHT_PX: f32 = 6.0;
/// `PROGRESSBAR_ANIMATION_TIME` and `COGBAR_ANIMATION_TIME`, seconds.
pub const ANIM_SECS: f64 = 0.5;
/// Bars draw over everything on the map (`ABOVE_HUD_PLANE`); the cog just under them
/// (`HIGH_GAME_PLANE`, over every mob).
// shortcut: the cog is not lit (tg draws it on a lit game plane).
const BAR_Z: f32 = Z_RANGE - 1.0;
const COG_Z: f32 = Z_RANGE - 2.0;

pub(crate) fn plugin(app: &mut App) {
    app.init_resource::<BarStates>()
        .add_systems(Update, (receive, draw).chain());
}

/// `prog_bar_0` … `prog_bar_100`, their `_fail` twins and `cog`, looked up once the atlas is
/// open.
#[derive(Resource, Default, Debug)]
pub struct BarStates {
    looked: bool,
    bar: [Option<IconStateId>; 21],
    fail: [Option<IconStateId>; 21],
    cog: Option<IconStateId>,
}

impl BarStates {
    fn look_up(&mut self, atlas: &IconAtlas) {
        if self.looked {
            return;
        }
        self.looked = true;
        for (i, s) in atlas.manifest.states.iter().enumerate() {
            if s.dmi != PROGRESSBAR_DMI || s.movement {
                continue;
            }
            let id = Some(IconStateId(i as u32));
            if s.state == "cog" {
                self.cog = self.cog.or(id);
                continue;
            }
            let Some(rest) = s.state.strip_prefix("prog_bar_") else {
                continue;
            };
            let (num, fail) = match rest.strip_suffix("_fail") {
                Some(num) => (num, true),
                None => (rest, false),
            };
            let Some(step) = num
                .parse::<usize>()
                .ok()
                .filter(|n| n % 5 == 0 && *n <= 100)
            else {
                continue;
            };
            let slot = if fail { &mut self.fail } else { &mut self.bar };
            slot[step / 5] = slot[step / 5].or(id);
        }
    }

    fn bar(&self, step: usize, failed: bool) -> Option<IconStateId> {
        failed.then(|| self.fail[step]).flatten().or(self.bar[step])
    }
}

/// A progress bar on screen.
#[derive(Component, Debug)]
pub struct Bar {
    /// The local entity it is over.
    pub target: Entity,
    pub born: f64,
    pub duration: f64,
    /// 1-based row on its target (tg `listindex`).
    pub index: u32,
    /// Height and time the last row change started from (tg's slide down in `Destroy`).
    pub slide: Option<(f32, f64)>,
    /// When it ended, its step then and whether it failed.
    pub ended: Option<(f64, usize, bool)>,
}

/// A cog over a busy mob.
#[derive(Component, Debug)]
pub struct Cog {
    pub target: Entity,
    pub born: f64,
    pub ended: Option<f64>,
}

/// tg `update`: `round(progress / goal * 100, 5)` as a step of 5 % (0..=20).
pub fn step(elapsed: f64, duration: f64) -> usize {
    if duration <= 0.0 {
        return 20;
    }
    ((elapsed / duration).clamp(0.0, 1.0) * 20.0).round() as usize
}

fn sine(t: f64) -> f32 {
    ease(Easing::of(EaseCurve::Sine), t as f32)
}

/// The resting height of row `index`, pixels over the target's tile.
pub fn row_height(index: u32) -> f32 {
    TILE_SIZE as f32 + HEIGHT_PX * (index.max(1) - 1) as f32
}

impl Bar {
    /// Height (pixels over the target's tile) and alpha (0-1) at `now`.
    pub fn pose(&self, now: f64) -> (f32, f32) {
        let rest = row_height(self.index);
        let height = match self.slide {
            Some((from, at)) => from + (rest - from) * sine((now - at) / ANIM_SECS),
            None => rest * sine((now - self.born) / ANIM_SECS),
        };
        let alpha = match self.ended {
            Some((at, ..)) => 1.0 - ((now - at) / ANIM_SECS).clamp(0.0, 1.0) as f32,
            None => sine((now - self.born) / ANIM_SECS),
        };
        (height, alpha)
    }

    fn gone(&self, now: f64) -> bool {
        self.ended.is_some_and(|(at, ..)| now - at >= ANIM_SECS)
    }
}

fn receive(
    mut received: MessageReader<AnimationReceived>,
    map: Res<NetEntityMap>,
    time: Res<Time>,
    mut bars: Query<(Entity, &mut Bar)>,
    mut cogs: Query<&mut Cog>,
    mut commands: Commands,
) {
    let now = time.elapsed_secs_f64();
    for AnimationReceived(event) in received.read() {
        let Some(target) = map.get(event.target) else {
            continue;
        };
        match event.animation {
            Animation::Progress { duration_ds } => {
                let index = bars.iter().filter(|(_, b)| b.target == target).count() as u32 + 1;
                commands.spawn((
                    Bar {
                        target,
                        born: now,
                        duration: f64::from(duration_ds) / 10.0,
                        index,
                        slide: None,
                        ended: None,
                    },
                    Name::new("progress bar"),
                    Sprite::default(),
                    Anchor::BOTTOM_LEFT,
                    Transform::default(),
                    Visibility::Hidden,
                ));
            }
            Animation::ProgressEnd { success } => {
                let oldest = bars
                    .iter_mut()
                    .filter(|(_, b)| b.target == target && b.ended.is_none())
                    .min_by(|(_, a), (_, b)| a.born.total_cmp(&b.born));
                if let Some((_, mut bar)) = oldest {
                    let s = if success {
                        20
                    } else {
                        step(now - bar.born, bar.duration)
                    };
                    bar.ended = Some((now, s, !success));
                }
            }
            Animation::Cog { on: true } => {
                // The player's own mob shows no cog (tg's blank override image).
                if map.controlled() == Some(target) {
                    continue;
                }
                commands.spawn((
                    Cog {
                        target,
                        born: now,
                        ended: None,
                    },
                    Name::new("cog"),
                    Sprite::default(),
                    Anchor::BOTTOM_LEFT,
                    Transform::default(),
                    Visibility::Hidden,
                ));
            }
            Animation::Cog { on: false } => {
                for mut cog in &mut cogs {
                    if cog.target == target && cog.ended.is_none() {
                        cog.ended = Some(now);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Cost: per bar or cog on screen (a handful), one transform lookup and a sprite write.
#[allow(clippy::type_complexity)]
fn draw(
    time: Res<Time>,
    mut states: ResMut<BarStates>,
    mut atlas: Option<ResMut<IconAtlas>>,
    targets: Query<&GlobalTransform, (Without<Bar>, Without<Cog>)>,
    mut bars: Query<
        (
            Entity,
            &mut Bar,
            &mut Sprite,
            &mut Transform,
            &mut Visibility,
        ),
        Without<Cog>,
    >,
    mut cogs: Query<(Entity, &Cog, &mut Sprite, &mut Transform, &mut Visibility), Without<Bar>>,
    mut commands: Commands,
) {
    let now = time.elapsed_secs_f64();
    let Some(atlas) = atlas.as_deref_mut() else {
        return;
    };
    states.look_up(atlas);
    // Bars that finished fading go; the ones above them on that target slide down a row.
    let gone: Vec<(Entity, Entity, u32)> = bars
        .iter()
        .filter(|(_, b, ..)| b.gone(now) || targets.get(b.target).is_err())
        .map(|(e, b, ..)| (e, b.target, b.index))
        .collect();
    for &(entity, target, index) in &gone {
        commands.entity(entity).despawn();
        for (other, mut bar, ..) in &mut bars {
            if other != entity && bar.target == target && bar.index > index {
                let (height, _) = bar.pose(now);
                bar.index -= 1;
                bar.slide = Some((height, now));
            }
        }
    }
    for (entity, bar, mut sprite, mut transform, mut visibility) in &mut bars {
        if gone.iter().any(|g| g.0 == entity) {
            continue;
        }
        let Ok(at) = targets.get(bar.target) else {
            continue;
        };
        let (s, failed) = match bar.ended {
            Some((_, s, failed)) => (s, failed),
            None => (step(now - bar.born, bar.duration), false),
        };
        let (height, alpha) = bar.pose(now);
        let drawn = states.bar(s, failed).and_then(|id| {
            atlas
                .state(id)
                .map(|(info, img)| (frame_rect(info, Dir::South, 0), img))
        });
        place(
            &mut sprite,
            &mut transform,
            &mut visibility,
            drawn,
            at,
            height,
            alpha,
            BAR_Z,
        );
    }
    for (entity, cog, mut sprite, mut transform, mut visibility) in &mut cogs {
        let ended = cog.ended.is_some_and(|at| now - at >= ANIM_SECS);
        let Some(at) = targets.get(cog.target).ok().filter(|_| !ended) else {
            commands.entity(entity).despawn();
            continue;
        };
        let alpha = match cog.ended {
            Some(at) => 1.0 - ((now - at) / ANIM_SECS).clamp(0.0, 1.0) as f32,
            None => ((now - cog.born) / ANIM_SECS).clamp(0.0, 1.0) as f32,
        };
        let drawn = states.cog.and_then(|id| {
            atlas
                .state(id)
                .map(|(info, img)| (frame_rect(info, Dir::South, 0), img))
        });
        let height = TILE_SIZE as f32;
        place(
            &mut sprite,
            &mut transform,
            &mut visibility,
            drawn,
            at,
            height,
            alpha,
            COG_Z,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn place(
    sprite: &mut Sprite,
    transform: &mut Transform,
    visibility: &mut Visibility,
    drawn: Option<(Option<Rect>, Handle<Image>)>,
    at: &GlobalTransform,
    height: f32,
    alpha: f32,
    z: f32,
) {
    let Some((rect, image)) = drawn else {
        *visibility = Visibility::Hidden;
        return;
    };
    sprite.image = image;
    sprite.rect = rect;
    sprite.color = Color::srgba(1.0, 1.0, 1.0, alpha);
    let base = at.translation();
    transform.translation = Vec3::new(base.x, base.y + height, z);
    *visibility = Visibility::Inherited;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bar_fills_in_steps_of_five_rises_into_its_row_and_fades_out() {
        assert_eq!(step(0.0, 2.0), 0);
        assert_eq!(step(1.0, 2.0), 10, "half way: prog_bar_50");
        assert_eq!(step(0.96, 2.0), 10, "rounded to the nearest 5 %");
        assert_eq!(step(5.0, 2.0), 20);
        assert_eq!(step(0.0, 0.0), 20);

        let mut bar = Bar {
            target: Entity::PLACEHOLDER,
            born: 10.0,
            duration: 3.0,
            index: 2,
            slide: None,
            ended: None,
        };
        assert_eq!(bar.pose(10.0), (0.0, 0.0), "starts on the tile, unseen");
        let (h, a) = bar.pose(10.0 + ANIM_SECS);
        assert!(
            (h - 38.0).abs() < 1e-3 && (a - 1.0).abs() < 1e-6,
            "row 2: 32 + 6 px"
        );
        bar.ended = Some((12.0, 13, true));
        assert_eq!(bar.pose(12.0 + ANIM_SECS).1, 0.0);
        assert!(bar.gone(12.0 + ANIM_SECS));

        // The row below went: it slides from where it was down to row 1.
        bar.index = 1;
        bar.slide = Some((38.0, 20.0));
        assert!((bar.pose(20.0).0 - 38.0).abs() < 1e-3);
        assert!((bar.pose(20.0 + ANIM_SECS).0 - 32.0).abs() < 1e-3);
    }
}
