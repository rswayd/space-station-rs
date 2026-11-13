//! A horizontal slider over a window's [`UiLocalState`]: a track with a fill and a knob. A press
//! or a drag on the track sets the value under its key (snapped to `step`, within `min..=max`)
//! and re-renders the window, so the view reads the value back as any other local entry.

use bevy::picking::Pickable;
use bevy::picking::events::{PointerDrag, PointerPress};
use bevy::scene::{Scene, bsn};
use bevy::ui::{ComputedNode, UiGlobalTransform};
use ss13_ui_kit::prelude::*;
use ss13_ui_kit::theme;

use crate::client::{ClientWindow, UiLocalState};

/// The track of a slider: the local key it writes and its range.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct LocalSlider {
    pub key: String,
    pub min: f32,
    pub max: f32,
    pub step: f32,
}

impl LocalSlider {
    /// The value at `fraction` (0..1) of the track, snapped to `step`.
    pub fn value_at(&self, fraction: f32) -> f32 {
        let raw = self.min + fraction.clamp(0.0, 1.0) * (self.max - self.min);
        let snapped = if self.step > 0.0 {
            self.min + ((raw - self.min) / self.step).round() * self.step
        } else {
            raw
        };
        snapped.clamp(self.min.min(self.max), self.max.max(self.min))
    }

    /// Where `value` sits on the track, 0..1.
    pub fn fraction_of(&self, value: f32) -> f32 {
        let span = self.max - self.min;
        if span == 0.0 {
            return 0.0;
        }
        ((value - self.min) / span).clamp(0.0, 1.0)
    }
}

/// Track width in logical pixels.
pub const SLIDER_WIDTH: f32 = 140.0;
const TRACK_HEIGHT: f32 = 6.0;
const KNOB: f32 = 12.0;

/// A slider writing `key` in `min..=max` by `step`, drawn at `value`.
pub fn slider(key: String, value: f32, min: f32, max: f32, step: f32) -> impl Scene + use<> {
    let track = LocalSlider {
        key,
        min,
        max,
        step,
    };
    let at = track.fraction_of(value) * 100.0;
    bsn! {
        // The hit area is taller than the visible track, so it is easy to grab.
        Node {
            width: px(SLIDER_WIDTH),
            height: px(KNOB + 4.0),
            align_items: AlignItems::Center,
        }
        track
        Children [
            Node {
                width: percent(100),
                height: px(TRACK_HEIGHT),
                border_radius: BorderRadius::all(px(TRACK_HEIGHT / 2.0)),
            }
            BackgroundColor({theme::COLOR_SECONDARY})
            Pickable::IGNORE
            Children [
                Node {
                    width: percent(at),
                    height: percent(100),
                    border_radius: BorderRadius::all(px(TRACK_HEIGHT / 2.0)),
                }
                BackgroundColor({theme::COLOR_PRIMARY})
                Pickable::IGNORE
            ]
            --
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(at),
                margin: UiRect::left(px(-KNOB / 2.0)),
                width: px(KNOB),
                height: px(KNOB),
                border: UiRect::all(px(theme::BORDER_TINY)),
                border_radius: BorderRadius::all(px(KNOB / 2.0)),
            }
            BackgroundColor({theme::COLOR_TEXT})
            BorderColor::all(theme::COLOR_BORDER_DARK)
            Pickable::IGNORE
        ]
    }
}

type Windows<'w, 's> = Query<'w, 's, (&'static mut ClientWindow, &'static mut UiLocalState)>;

/// Where `pointer` (window logical px) is along `node`, 0..1.
fn fraction(pointer: Vec2, scale: f32, node: &ComputedNode, transform: &UiGlobalTransform) -> f32 {
    let width = node.size.x.max(1.0);
    let left = transform.translation.x - width / 2.0;
    ((pointer.x * scale - left) / width).clamp(0.0, 1.0)
}

fn window_scale(screens: &Query<&bevy::window::Window>) -> f32 {
    screens.iter().next().map_or(1.0, |w| w.scale_factor())
}

/// Writes the slider's value for `pointer` into the window it is in.
fn set_from(
    entity: Entity,
    pointer: Vec2,
    sliders: &Query<(&LocalSlider, &ComputedNode, &UiGlobalTransform)>,
    screens: &Query<&bevy::window::Window>,
    parents: &Query<&ChildOf>,
    windows: &mut Windows,
) {
    let Ok((slider, node, transform)) = sliders.get(entity) else {
        return;
    };
    let value = slider.value_at(fraction(pointer, window_scale(screens), node, transform));
    let Some(root) = parents
        .iter_ancestors(entity)
        .find(|e| windows.contains(*e))
    else {
        return;
    };
    let Ok((mut window, mut local)) = windows.get_mut(root) else {
        return;
    };
    let text = format!("{}", value.round() as i64);
    if local.value(&slider.key) != Some(text.as_str()) {
        local.set(&slider.key, &text);
        window.mark_dirty();
    }
}

fn press(
    mut press: On<PointerPress>,
    sliders: Query<(&LocalSlider, &ComputedNode, &UiGlobalTransform)>,
    screens: Query<&bevy::window::Window>,
    parents: Query<&ChildOf>,
    mut windows: Windows,
) {
    let entity = press.event_target();
    if sliders.contains(entity) {
        press.propagate(false);
        let at = press.pointer.position;
        set_from(entity, at, &sliders, &screens, &parents, &mut windows);
    }
}

fn drag(
    mut drag: On<PointerDrag>,
    sliders: Query<(&LocalSlider, &ComputedNode, &UiGlobalTransform)>,
    screens: Query<&bevy::window::Window>,
    parents: Query<&ChildOf>,
    mut windows: Windows,
) {
    let entity = drag.event_target();
    if sliders.contains(entity) {
        // A drag on a slider moves the knob, not the window.
        drag.propagate(false);
        let at = drag.pointer.position;
        set_from(entity, at, &sliders, &screens, &parents, &mut windows);
    }
}

pub(crate) fn plugin(app: &mut App) {
    app.add_observer(press).add_observer(drag);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_snap_to_the_step_within_the_range() {
        let s = LocalSlider {
            key: "v".into(),
            min: 0.0,
            max: 100.0,
            step: 5.0,
        };
        assert_eq!(s.value_at(0.0), 0.0);
        assert_eq!(s.value_at(1.0), 100.0);
        assert_eq!(s.value_at(0.42), 40.0);
        assert_eq!(s.value_at(0.43), 45.0);
        assert_eq!(s.value_at(2.0), 100.0);
        assert_eq!(s.fraction_of(25.0), 0.25);
        assert_eq!(s.fraction_of(-10.0), 0.0);
    }
}
