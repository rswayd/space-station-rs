//! tgui `ProgressBar` and `AnimatedNumber`.

use bevy::ecs::change_detection::DetectChangesMut;
use bevy::prelude::{
    AlignItems, App, BackgroundColor, BorderColor, BorderRadius, Changed, Children, Color,
    Component, Display, FlexDirection, IntoScheduleConfigs, JustifyContent, Node, PositionType,
    PostUpdate, Query, Res, Time, UiRect, Update, With, percent, px,
};
use bevy::scene::{Scene, SceneComponent, bsn};
use bevy::ui::UiSystems;
use bevy::ui::widget::Text;

use crate::button::TrailingMargin;
use crate::layout::{HasSlot, Part, Slot};
use crate::theme::{self, UiColor};

/// tgui `ProgressBar`. Fields are live state: `ProgressBar { value: 0.4, .. }`; change them and
/// the bar follows. User children replace the percentage text.
#[derive(SceneComponent, Clone, Debug, PartialEq)]
#[scene(progress_bar(ProgressBarProps))]
pub struct ProgressBar {
    pub value: f32,
    pub min_value: f32,
    pub max_value: f32,
    /// Fixed color; `Default` picks from `ranges`, then primary.
    pub color: UiColor,
    /// `ranges={{ good: [0.5, Infinity], bad: [-Infinity, 0.5] }}`: the first range holding the
    /// raw (unscaled) value wins, in the order given.
    pub ranges: Vec<(UiColor, f32, f32)>,
}

impl Default for ProgressBar {
    fn default() -> Self {
        Self {
            value: 0.0,
            min_value: 0.0,
            max_value: 1.0,
            color: UiColor::Default,
            ranges: Vec::new(),
        }
    }
}

#[derive(Default)]
pub struct ProgressBarProps {
    /// `empty`: no percentage text.
    pub empty: bool,
    /// Digits after the point in the percentage text.
    pub fraction_digits: usize,
}

/// The fill node of a progress bar.
#[derive(Component, Clone, Debug, Default)]
pub struct ProgressBarFill;

/// The auto-generated `NN%` text; absent when `empty` or when children replace it.
#[derive(Component, Clone, Debug, Default)]
pub struct ProgressBarText {
    pub fraction_digits: usize,
}

impl ProgressBar {
    /// `scale(value, min, max)` clamped to 0..1.
    pub fn fraction(&self) -> f32 {
        let span = self.max_value - self.min_value;
        if span == 0.0 {
            return 0.0;
        }
        ((self.value - self.min_value) / span).clamp(0.0, 1.0)
    }

    /// `color || keyOfMatchingRange(value, ranges) || 'default'`.
    pub fn effective_color(&self) -> UiColor {
        if self.color != UiColor::Default {
            return self.color;
        }
        self.ranges
            .iter()
            .find(|(_, min, max)| self.value >= *min && self.value <= *max)
            .map_or(UiColor::Default, |(c, _, _)| *c)
    }

    /// `--progress-bar-color`: the color 5 points darker (`--progress-bar-fill` is primary).
    pub fn bar_color(&self) -> Color {
        theme::lighten(self.effective_color().base(), -theme::ADJUST_COLOR)
    }
}

fn progress_bar(props: ProgressBarProps) -> impl Scene {
    let digits = props.fraction_digits;
    let text = (!props.empty).then(|| {
        bsn! { Children [ Text("0%") ProgressBarText { fraction_digits: digits } ] }
    });
    bsn! {
            Node {
                min_height: px(theme::BUTTON_HEIGHT),
                padding: UiRect::horizontal(px(theme::SPACE_M)),
                border: UiRect::all(px(theme::BORDER_TINY)),
                border_radius: BorderRadius::all(px(theme::RADIUS_TINY)),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
            }
            HasSlot
            TrailingMargin(UiRect::DEFAULT)
            BorderColor::all(Color::NONE)
            Children [
                Node {
                    position_type: PositionType::Absolute,
                    left: px(0),
                    top: px(0),
                    bottom: px(0),
                    width: percent(0),
                }
                Part
                ProgressBarFill
                BackgroundColor(Color::NONE)
                --
                // `.ProgressBar__content { text-align: right }`.
                Node {
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::FlexEnd,
                    align_items: AlignItems::Center,
                }
                Part
                Slot
                @{text}
            ]
    }
}

/// Restyles a bar and its parts when its live state changes.
fn update_progress_bars(
    bars: Query<(&ProgressBar, &Children, &mut BorderColor), Changed<ProgressBar>>,
    mut fills: Query<(&mut Node, &mut BackgroundColor), With<ProgressBarFill>>,
    descendants: Query<&Children>,
    mut texts: Query<
        (&mut Text, &mut Node, &ProgressBarText),
        bevy::prelude::Without<ProgressBarFill>,
    >,
) {
    for (bar, children, mut border) in bars {
        let color = bar.bar_color();
        border.set_if_neq(BorderColor::all(color));
        let fraction = bar.fraction();
        for &part in children {
            if let Ok((mut node, mut bg)) = fills.get_mut(part) {
                node.width = percent(100.0 * fraction);
                bg.set_if_neq(BackgroundColor(color));
            }
            let Ok(content) = descendants.get(part) else {
                continue;
            };
            // User children in the content slot replace the percentage text.
            let replaced = content.len() > 1;
            for &e in content {
                if let Ok((mut text, mut node, digits)) = texts.get_mut(e) {
                    let label = format!("{:.*}%", digits.fraction_digits, 100.0 * fraction);
                    if text.0 != label {
                        text.0 = label;
                    }
                    let display = if replaced {
                        Display::None
                    } else {
                        Display::Flex
                    };
                    if node.display != display {
                        node.display = display;
                    }
                }
            }
        }
    }
}

/// tgui `AnimatedNumber`: a text that eases toward `value` each frame. Set `value` to animate.
#[derive(SceneComponent, Clone, Debug, Default, PartialEq)]
#[scene(animated_number(AnimatedNumberProps))]
pub struct AnimatedNumber {
    pub value: f64,
}

#[derive(Default)]
pub struct AnimatedNumberProps {
    /// `initial`: the value the animation starts from; defaults to `value`.
    pub initial: Option<f64>,
    /// `format`; defaults to as many decimals as `value` has.
    pub format: Option<fn(f64) -> String>,
}

/// The number currently shown by an [`AnimatedNumber`].
#[derive(Component, Clone, Debug, Default)]
pub struct AnimatedNumberState {
    pub current: Option<f64>,
    pub format: Option<fn(f64) -> String>,
}

fn animated_number(props: AnimatedNumberProps) -> impl Scene {
    let state = AnimatedNumberState {
        current: props.initial,
        format: props.format,
    };
    bsn! { Text state }
}

/// `toFixed(value, decimals of target)`, decimals clamped to 0..=8 as tgui does.
fn format_like(target: f64, value: f64) -> String {
    let decimals = target
        .to_string()
        .split_once('.')
        .map_or(0, |(_, frac)| frac.len().min(8));
    format!("{value:.decimals$}")
}

/// tgui steps `current = current * e + target * (1 - e)` per 60 Hz frame with `e = 0.8333`;
/// scaled here by the real frame time.
fn animate_numbers(
    time: Res<Time>,
    mut numbers: Query<(&AnimatedNumber, &mut AnimatedNumberState, &mut Text)>,
) {
    let frames = time.delta_secs_f64() * 60.0;
    let e = 0.8333_f64.powf(frames);
    for (number, mut state, mut text) in &mut numbers {
        let target = number.value;
        let current = state.current.unwrap_or(target);
        let mut next = current * e + target * (1.0 - e);
        if (target - next).abs() < (0.001 * target.abs()).max(0.001) || !next.is_finite() {
            next = target;
        }
        if state.current != Some(next) {
            state.current = Some(next);
        }
        let label = match state.format {
            Some(format) => format(next),
            None => format_like(target, next),
        };
        if text.0 != label {
            text.0 = label;
        }
    }
}

pub(crate) fn plugin(app: &mut App) {
    app.add_systems(Update, animate_numbers)
        .add_systems(PostUpdate, update_progress_bars.before(UiSystems::Layout));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_pick_color_in_order() {
        let bar = ProgressBar {
            value: 0.3,
            ranges: vec![
                (UiColor::Good, 0.5, f32::INFINITY),
                (UiColor::Average, 0.25, 0.5),
                (UiColor::Bad, f32::NEG_INFINITY, 0.25),
            ],
            ..Default::default()
        };
        assert_eq!(bar.effective_color(), UiColor::Average);
        assert!((bar.fraction() - 0.3).abs() < 1e-6);
        let over = ProgressBar {
            value: 150.0,
            max_value: 100.0,
            ..Default::default()
        };
        assert_eq!(over.fraction(), 1.0);
        assert_eq!(over.effective_color(), UiColor::Default);
    }

    #[test]
    fn number_keeps_target_decimals() {
        assert_eq!(format_like(12.5, 7.123), "7.1");
        assert_eq!(format_like(3.0, 2.6), "3");
    }
}
