//! Client half of runechat ([`crate::runechat`], tg `/datum/chatmessage`): every
//! [`Animation::Runechat`] becomes a line of text over its speaker. A speaker's lines stack, the
//! newest at the bottom, and a new line shortens the older ones' lives (`CHAT_MESSAGE_EXP_DECAY`,
//! `CHAT_MESSAGE_HEIGHT_DECAY`).
// shortcut: a UI text node projected onto the map every frame, as `crate::balloon_client`, with
// Bevy's default font (no bold for `yell`, no italics: the server already darkens the color);
// the prefix `[LOOC]` is in the line's own color.

use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::text::{FontSize, Justify, TextLayout};
use ss13_net::animation::client::AnimationReceived;
use ss13_net::client::NetEntityMap;
use ss13_render::WorldCamera;
use ss13_shared::TILE_SIZE;
use ss13_shared::animation::Animation;

/// `CHAT_MESSAGE_SPAWN_TIME`, seconds.
const SPAWN_SECS: f64 = 0.2;
/// `CHAT_MESSAGE_LIFESPAN`.
const LIFESPAN_SECS: f64 = 5.0;
/// `CHAT_MESSAGE_EOL_FADE`.
const FADE_SECS: f64 = 0.7;
/// `CHAT_MESSAGE_EXP_DECAY`, `CHAT_MESSAGE_HEIGHT_DECAY`.
const EXP_DECAY: f64 = 0.7;
const HEIGHT_DECAY: f64 = 0.9;
/// `CHAT_MESSAGE_WIDTH`, icon pixels.
const WIDTH_PX: f32 = 112.0;
/// The maptext font's height, icon pixels (`small`: 6).
const FONT_PX: f32 = 7.0;
const SMALL_FONT_PX: f32 = 6.0;
/// Characters of the font on a line, what `MeasureText` gives tg as `approx_lines`.
const CHARS_PER_LINE: f32 = 22.0;
/// How fast a line eases to its place in the stack (`CHAT_MESSAGE_SPAWN_TIME` per slide), per
/// second.
const SLIDE_RATE: f32 = 12.0;

pub struct RunechatClientPlugin;

impl Plugin for RunechatClientPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (spawn_lines, place_lines).chain());
    }
}

/// A line on screen.
#[derive(Component, Debug)]
pub struct Runechat {
    /// The local entity it is over.
    pub target: Entity,
    pub born: f64,
    /// When it starts fading.
    pub fade_at: f64,
    pub lines: f32,
    pub color: [u8; 3],
    pub small: bool,
    /// Current rise above the speaker, logical pixels.
    pub lifted: f32,
}

/// tg `approx_lines`.
fn approx_lines(text: &str) -> f32 {
    (text.chars().count() as f32 / CHARS_PER_LINE)
        .ceil()
        .max(1.0)
}

/// The alpha of a line `now` (`get_current_alpha`).
pub fn alpha_at(line: &Runechat, now: f64) -> f32 {
    let t = now - line.born;
    let alpha = if t < SPAWN_SECS {
        t / SPAWN_SECS
    } else if now < line.fade_at {
        1.0
    } else {
        1.0 - (now - line.fade_at) / FADE_SECS
    };
    alpha.clamp(0.0, 1.0) as f32
}

/// tg `finish_image_generation`: the older lines of the speaker, oldest first, get shorter lives;
/// those already fading keep theirs.
pub fn age_older<'a>(older: impl Iterator<Item = &'a mut Runechat>, new_lines: f32, now: f64) {
    let mut idx = 1;
    let mut combined = new_lines;
    for line in older {
        combined += line.lines;
        let before_fade = line.fade_at - line.born;
        if now - line.born >= before_fade {
            continue;
        }
        let remaining = before_fade * EXP_DECAY.powi(idx) * HEIGHT_DECAY.powf(f64::from(combined));
        idx += 1;
        line.fade_at = now + remaining.max(0.0);
    }
}

fn spawn_lines(
    mut received: MessageReader<AnimationReceived>,
    map: Res<NetEntityMap>,
    time: Res<Time>,
    mut lines: Query<&mut Runechat>,
    mut commands: Commands,
) {
    let now = time.elapsed_secs_f64();
    for AnimationReceived(event) in received.read() {
        let Animation::Runechat { text, color, style } = &event.animation else {
            continue;
        };
        let Some(target) = map.get(event.target) else {
            continue;
        };
        let shown = if style.looc {
            format!("[LOOC] {text}")
        } else {
            text.clone()
        };
        let new_lines = approx_lines(&shown);
        let mut older: Vec<Mut<Runechat>> =
            lines.iter_mut().filter(|l| l.target == target).collect();
        older.sort_by(|a, b| a.born.total_cmp(&b.born));
        age_older(older.iter_mut().map(|l| &mut **l), new_lines, now);
        let [r, g, b] = *color;
        commands.spawn((
            Runechat {
                target,
                born: now,
                fade_at: now + LIFESPAN_SECS - SPAWN_SECS - FADE_SECS,
                lines: new_lines,
                color: *color,
                small: style.small,
                lifted: 0.0,
            },
            Name::new("runechat"),
            Node {
                position_type: PositionType::Absolute,
                ..default()
            },
            Text::new(shown),
            TextFont {
                font_size: FontSize::Px(FONT_PX * 2.0),
                ..default()
            },
            TextColor(Color::srgba_u8(r, g, b, 0)),
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

/// Cost: per line on screen (a dozen at a crowded bar), one projection and a few writes a frame.
fn place_lines(
    time: Res<Time>,
    cameras: Query<(&Camera, &GlobalTransform), With<WorldCamera>>,
    targets: Query<&GlobalTransform, Without<Runechat>>,
    mut lines: Query<(
        Entity,
        &mut Runechat,
        &mut Node,
        &mut TextFont,
        &mut TextColor,
        &mut TextShadow,
        &ComputedNode,
    )>,
    mut commands: Commands,
) {
    let now = time.elapsed_secs_f64();
    let dt = time.delta_secs();
    let camera = cameras.iter().next();
    // A speaker's lines, newest first: each stands on the ones born after it.
    let mut stacks: HashMap<Entity, Vec<(f64, Entity, f32)>> = HashMap::new();
    for (entity, line, _, _, _, _, computed) in &lines {
        let height = computed.size().y * computed.inverse_scale_factor();
        stacks
            .entry(line.target)
            .or_default()
            .push((line.born, entity, height));
    }
    let mut lifts: HashMap<Entity, f32> = HashMap::new();
    for stack in stacks.values_mut() {
        stack.sort_by(|a, b| b.0.total_cmp(&a.0));
        let mut under = 0.0;
        for &(_, entity, height) in stack.iter() {
            lifts.insert(entity, under);
            under += height;
        }
    }
    for (entity, mut line, mut node, mut font, mut color, mut shadow, computed) in &mut lines {
        let Ok(target) = targets.get(line.target) else {
            commands.entity(entity).despawn();
            continue;
        };
        // `CHAT_MESSAGE_GRACE_PERIOD` after the fade ends.
        if now > line.fade_at + FADE_SECS + 0.2 {
            commands.entity(entity).despawn();
            continue;
        }
        let want = lifts.get(&entity).copied().unwrap_or(0.0);
        let step = (dt * SLIDE_RATE).min(1.0);
        line.lifted += (want - line.lifted) * step;
        let alpha = alpha_at(&line, now);
        let [r, g, b] = line.color;
        color.0 = Color::srgba_u8(r, g, b, (alpha * 255.0) as u8);
        shadow.color = Color::srgba(0.0, 0.0, 0.0, alpha * 0.8);
        let Some((camera, view)) = camera else {
            continue;
        };
        let base =
            target.translation().truncate() + Vec2::new(TILE_SIZE as f32 / 2.0, TILE_SIZE as f32);
        let (Ok(at), Ok(right)) = (
            camera.world_to_viewport(view, base.extend(0.0)),
            camera.world_to_viewport(view, (base + Vec2::X).extend(0.0)),
        ) else {
            continue;
        };
        let scale = (right.x - at.x).abs().max(1.0);
        let size = (if line.small { SMALL_FONT_PX } else { FONT_PX }) * scale;
        if !matches!(font.font_size, FontSize::Px(cur) if cur == size) {
            font.font_size = FontSize::Px(size);
        }
        let width = WIDTH_PX * scale;
        let height = computed.size().y * computed.inverse_scale_factor();
        node.width = px(width);
        node.left = px(at.x - width / 2.0);
        node.top = px(at.y - height - line.lifted);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(born: f64) -> Runechat {
        Runechat {
            target: Entity::PLACEHOLDER,
            born,
            fade_at: born + 4.1,
            lines: 1.0,
            color: [255; 3],
            small: false,
            lifted: 0.0,
        }
    }

    #[test]
    fn a_line_fades_in_stays_and_fades_out() {
        let l = line(10.0);
        assert_eq!(alpha_at(&l, 10.0), 0.0);
        assert_eq!(alpha_at(&l, 11.0), 1.0);
        assert!(alpha_at(&l, l.fade_at + FADE_SECS / 2.0) < 1.0);
        assert!(alpha_at(&l, l.fade_at + FADE_SECS) < 1e-6);
    }

    #[test]
    fn older_lines_lose_time_to_newer_ones() {
        let (mut a, mut b) = (line(0.0), line(1.0));
        let (a_fade, b_fade) = (a.fade_at, b.fade_at);
        age_older([&mut a, &mut b].into_iter(), 1.0, 1.5);
        assert!(a.fade_at < a_fade && b.fade_at < b_fade);
        // A line past its fade moment is left alone.
        let mut c = line(0.0);
        let fade = c.fade_at;
        age_older([&mut c].into_iter(), 1.0, 5.0);
        assert_eq!(c.fade_at, fade);
    }

    #[test]
    fn approx_lines_grow_with_text() {
        assert_eq!(approx_lines("ok"), 1.0);
        assert_eq!(approx_lines(&"я".repeat(23)), 2.0);
    }
}
