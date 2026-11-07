//! Client half of balloon alerts ([`crate::balloon`]): each [`Animation::Balloon`] becomes a
//! line of text over its entity that rises 1.2 tiles and fades, as tg `balloon_alert_perform`'s
//! `animate` chain (`pixel_y` over the whole lifetime with `SINE_EASING|EASE_OUT`; alpha in over
//! 0.2 s `CUBIC|EASE_OUT`, out over 0.7 s × length `CUBIC|EASE_IN`).
// shortcut: a UI text node projected onto the map every frame instead of a maptext image on the
// balloon plane; Bevy's default font with a drop shadow instead of the 1px `#0005` outline.

use bevy::prelude::*;
use bevy::text::{FontSize, Justify, TextLayout};
use ss13_net::animation::client::AnimationReceived;
use ss13_net::client::NetEntityMap;
use ss13_render::WorldCamera;
use ss13_shared::TILE_SIZE;
use ss13_shared::animation::Animation;

/// `BALLOON_TEXT_SPAWN_TIME`, seconds.
const SPAWN_SECS: f64 = 0.2;
/// `BALLOON_TEXT_FULLY_VISIBLE_TIME`.
const VISIBLE_SECS: f64 = 0.7;
/// `BALLOON_TEXT_FADE_TIME`.
const FADE_SECS: f64 = 0.1;
/// `BALLOON_TEXT_CHAR_LIFETIME_INCREASE_MIN` / `_MULT`.
const CHAR_MIN: usize = 10;
const CHAR_MULT: f64 = 0.05;
/// `BALLOON_TEXT_WIDTH`, icon pixels.
const WIDTH_PX: f32 = 200.0;
/// `pixel_y = ICON_SIZE_Y * 1.2`.
const RISE_PX: f32 = TILE_SIZE as f32 * 1.2;
/// The maptext font's height, icon pixels.
const FONT_PX: f32 = 7.0;

pub struct BalloonClientPlugin;

impl Plugin for BalloonClientPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (spawn_balloons, move_balloons).chain());
    }
}

/// A balloon on screen.
#[derive(Component, Debug)]
pub struct Balloon {
    /// The local entity it rises from.
    pub target: Entity,
    pub born: f64,
    pub length_mult: f64,
}

/// tg `length_mult`: longer texts stay longer.
pub fn length_mult(text: &str) -> f64 {
    1.0 + text.chars().count().saturating_sub(CHAR_MIN) as f64 * CHAR_MULT
}

/// `BALLOON_TEXT_TOTAL_LIFETIME(mult)`, seconds.
pub fn lifetime(mult: f64) -> f64 {
    SPAWN_SECS + VISIBLE_SECS * mult + FADE_SECS
}

/// The rise (icon pixels) and alpha (0-1) `t` seconds after the balloon appeared.
pub fn pose(t: f64, mult: f64) -> (f32, f32) {
    let total = lifetime(mult);
    let p = (t / total).clamp(0.0, 1.0);
    let rise = RISE_PX * (p * std::f64::consts::FRAC_PI_2).sin() as f32;
    let alpha = if t < SPAWN_SECS {
        1.0 - (1.0 - t / SPAWN_SECS).powi(3)
    } else {
        let p = ((t - SPAWN_SECS) / (VISIBLE_SECS * mult)).clamp(0.0, 1.0);
        1.0 - p.powi(3)
    };
    (rise, alpha.clamp(0.0, 1.0) as f32)
}

fn spawn_balloons(
    mut received: MessageReader<AnimationReceived>,
    map: Res<NetEntityMap>,
    time: Res<Time>,
    mut commands: Commands,
) {
    let now = time.elapsed_secs_f64();
    for AnimationReceived(event) in received.read() {
        let Animation::Balloon { text } = &event.animation else {
            continue;
        };
        let Some(target) = map.get(event.target) else {
            continue;
        };
        commands.spawn((
            Balloon {
                target,
                born: now,
                length_mult: length_mult(text),
            },
            Name::new("balloon alert"),
            Node {
                position_type: PositionType::Absolute,
                ..default()
            },
            Text::new(text.clone()),
            TextFont {
                font_size: FontSize::Px(FONT_PX * 2.0),
                ..default()
            },
            TextColor(Color::srgba(1.0, 1.0, 1.0, 0.0)),
            TextLayout::justify(Justify::Center),
            TextShadow {
                offset: Vec2::splat(1.0),
                color: Color::srgba(0.0, 0.0, 0.0, 0.0),
            },
            Pickable::IGNORE,
            GlobalZIndex(0),
        ));
    }
}

/// Cost: per balloon on screen (a handful), one projection and a few node writes per frame.
fn move_balloons(
    time: Res<Time>,
    cameras: Query<(&Camera, &GlobalTransform), With<WorldCamera>>,
    targets: Query<&GlobalTransform, Without<Balloon>>,
    mut balloons: Query<(
        Entity,
        &Balloon,
        &mut Node,
        &mut TextFont,
        &mut TextColor,
        &mut TextShadow,
        &ComputedNode,
    )>,
    mut commands: Commands,
) {
    let now = time.elapsed_secs_f64();
    let camera = cameras.iter().next();
    for (entity, balloon, mut node, mut font, mut color, mut shadow, computed) in &mut balloons {
        let t = now - balloon.born;
        let Ok(target) = targets.get(balloon.target) else {
            commands.entity(entity).despawn();
            continue;
        };
        if t >= lifetime(balloon.length_mult) {
            commands.entity(entity).despawn();
            continue;
        }
        let (rise, alpha) = pose(t, balloon.length_mult);
        color.0 = Color::srgba(1.0, 1.0, 1.0, alpha);
        shadow.color = Color::srgba(0.0, 0.0, 0.0, alpha * 0.33);
        let Some((camera, view)) = camera else {
            continue;
        };
        let base = target.translation().truncate() + Vec2::new(TILE_SIZE as f32 / 2.0, rise);
        let (Ok(at), Ok(right)) = (
            camera.world_to_viewport(view, base.extend(0.0)),
            camera.world_to_viewport(view, (base + Vec2::X).extend(0.0)),
        ) else {
            continue;
        };
        // Window pixels per icon pixel: the map zoom.
        let scale = (right.x - at.x).abs().max(1.0);
        let size = FONT_PX * scale;
        if !matches!(font.font_size, FontSize::Px(cur) if cur == size) {
            font.font_size = FontSize::Px(size);
        }
        let width = WIDTH_PX * scale;
        let height = computed.size().y * computed.inverse_scale_factor();
        node.width = px(width);
        node.left = px(at.x - width / 2.0);
        node.top = px(at.y - height);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn balloons_fade_in_rise_and_fade_out_like_tg() {
        let mult = length_mult("нет места!");
        assert_eq!(mult, 1.0);
        assert!((lifetime(mult) - 1.0).abs() < 1e-9);
        let (rise, alpha) = pose(0.0, mult);
        assert_eq!((rise, alpha), (0.0, 0.0));
        let (_, alpha) = pose(SPAWN_SECS, mult);
        assert!((alpha - 1.0).abs() < 1e-6);
        let (rise, alpha) = pose(lifetime(mult), mult);
        assert!((rise - RISE_PX).abs() < 1e-3);
        assert_eq!(alpha, 0.0);
        // 20 characters: 10 over the minimum, half again as long.
        assert!((length_mult(&"a".repeat(20)) - 1.5).abs() < 1e-9);
    }
}
