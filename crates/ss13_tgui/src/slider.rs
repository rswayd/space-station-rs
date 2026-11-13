//! A horizontal slider over a window's [`UiLocalState`]: a track with a fill, a knob and its
//! value beside it. Pressing or dragging anywhere on the track (the drag keeps going outside it)
//! and the mouse wheel over it set a continuous value under its key; the slider moves its own
//! fill, knob and label at once, without re-rendering the window, so the dragged node stays the
//! same entity and the drag never drops. The view reads the value back from the local state on
//! its next render, like any other local entry.

use bevy::input::mouse::MouseScrollUnit;
use bevy::picking::Pickable;
use bevy::picking::events::{
    PointerDrag, PointerDragEnd, PointerOut, PointerOver, PointerPress, PointerScroll,
};
use bevy::scene::{Scene, bsn};
use bevy::ui::{ComputedNode, UiGlobalTransform};
use ss13_ui_kit::prelude::*;
use ss13_ui_kit::theme;

use crate::client::{ClientWindow, UiLocalState};

/// The track of a slider: the local key it writes, its range and its current value.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct LocalSlider {
    pub key: String,
    pub min: f32,
    pub max: f32,
    /// The value shown now (also written under `key`).
    pub value: f32,
    /// Change per wheel notch.
    pub wheel_step: f32,
}

impl LocalSlider {
    /// The value at `fraction` (0..1) of the track.
    pub fn value_at(&self, fraction: f32) -> f32 {
        let (lo, hi) = (self.min.min(self.max), self.max.max(self.min));
        (self.min + fraction.clamp(0.0, 1.0) * (self.max - self.min)).clamp(lo, hi)
    }

    /// Where `value` sits on the track, 0..1.
    pub fn fraction_of(&self, value: f32) -> f32 {
        let span = self.max - self.min;
        if span == 0.0 {
            return 0.0;
        }
        ((value - self.min) / span).clamp(0.0, 1.0)
    }

    /// `value` moved by `notches` of the wheel, within the range.
    pub fn wheeled(&self, notches: f32) -> f32 {
        let (lo, hi) = (self.min.min(self.max), self.max.max(self.min));
        (self.value + notches * self.wheel_step).clamp(lo, hi)
    }
}

/// The text a value is stored as in the local state (two decimals: continuous, short).
pub fn stored(value: f32) -> String {
    format!("{value:.2}")
}

/// The fill of a slider's track.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct SliderFill;

/// The knob of a slider.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct SliderKnob;

/// The value text beside a slider.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct SliderLabel;

/// The narrowest a track gets when its row is short of room.
pub const SLIDER_MIN_WIDTH: f32 = 60.0;
const TRACK_HEIGHT: f32 = 6.0;
const KNOB: f32 = 14.0;
const LABEL_WIDTH: f32 = 40.0;

/// The value as shown: a percent-like integer.
fn label(value: f32) -> String {
    format!("{}%", value.round() as i64)
}

fn knob_color(active: bool) -> Color {
    if active {
        theme::lighten(theme::COLOR_PRIMARY, theme::ADJUST_HOVER * 2.0)
    } else {
        theme::COLOR_TEXT
    }
}

/// A slider writing `key` in `min..=max`, drawn at `value`, its value text beside it.
pub fn slider(key: String, value: f32, min: f32, max: f32) -> impl Scene + use<> {
    let track = LocalSlider {
        key,
        min,
        max,
        value,
        wheel_step: (max - min).abs() / 100.0,
    };
    let at = track.fraction_of(value) * 100.0;
    let text = label(value);
    bsn! {
        // Takes the room its row leaves it; the track stretches, the value text does not.
        Node {
            flex_grow: 1.0,
            flex_basis: px(0),
            min_width: px(SLIDER_MIN_WIDTH + LABEL_WIDTH + 8.0),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: px(8),
        }
        Children [
            // The hit area is taller than the visible track, so it is easy to grab.
            Node {
                flex_grow: 1.0,
                flex_basis: px(0),
                min_width: px(SLIDER_MIN_WIDTH),
                height: px(KNOB + 6.0),
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
                    SliderFill
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
                SliderKnob
                BackgroundColor({knob_color(false)})
                BorderColor::all(theme::COLOR_BORDER_DARK)
                Pickable::IGNORE
            ]
            --
            Node { width: px(LABEL_WIDTH), flex_shrink: 0.0, justify_content: JustifyContent::FlexEnd }
            Children [ Text(text) SliderLabel ]
        ]
    }
}

type Windows<'w, 's> = Query<'w, 's, &'static mut UiLocalState, With<ClientWindow>>;
type Parts<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut Node,
        Option<&'static SliderFill>,
        Option<&'static SliderKnob>,
    ),
    Or<(With<SliderFill>, With<SliderKnob>)>,
>;

/// Where `pointer` (window logical px) is along `node`, 0..1.
fn fraction(pointer: Vec2, scale: f32, node: &ComputedNode, transform: &UiGlobalTransform) -> f32 {
    let width = node.size.x.max(1.0);
    let left = transform.translation.x - width / 2.0;
    ((pointer.x * scale - left) / width).clamp(0.0, 1.0)
}

fn window_scale(screens: &Query<&bevy::window::Window>) -> f32 {
    screens.iter().next().map_or(1.0, |w| w.scale_factor())
}

/// Sets the slider `entity` to `value`: its fill, knob and label at once, and the window's local
/// state (no re-render).
#[allow(clippy::too_many_arguments)]
fn set_value(
    entity: Entity,
    value: f32,
    sliders: &mut Query<&mut LocalSlider>,
    children: &Query<&Children>,
    parents: &Query<&ChildOf>,
    parts: &mut Parts,
    labels: &mut Query<&mut Text, With<SliderLabel>>,
    windows: &mut Windows,
) {
    let Ok(mut slider) = sliders.get_mut(entity) else {
        return;
    };
    if (slider.value - value).abs() < f32::EPSILON {
        return;
    }
    slider.value = value;
    let at = slider.fraction_of(value) * 100.0;
    let key = slider.key.clone();
    for part in children.iter_descendants(entity) {
        if let Ok((mut node, fill, knob)) = parts.get_mut(part) {
            if fill.is_some() {
                node.width = Val::Percent(at);
            }
            if knob.is_some() {
                node.left = Val::Percent(at);
            }
        }
    }
    // The label is the slider's sibling (its row's second child).
    if let Ok(row) = parents.get(entity).map(ChildOf::parent) {
        for part in children.iter_descendants(row) {
            if let Ok(mut text) = labels.get_mut(part) {
                text.0 = label(value);
            }
        }
    }
    if let Some(root) = parents
        .iter_ancestors(entity)
        .find(|e| windows.contains(*e))
        && let Ok(mut local) = windows.get_mut(root)
    {
        local.set(&key, &stored(value));
    }
}

fn knob_highlight(
    entity: Entity,
    active: bool,
    children: &Query<&Children>,
    knobs: &mut Query<&mut BackgroundColor, With<SliderKnob>>,
) {
    for part in children.iter_descendants(entity) {
        if let Ok(mut color) = knobs.get_mut(part) {
            color.0 = knob_color(active);
        }
    }
}

/// The slider being dragged (its knob stays lit when the pointer leaves the track).
#[derive(Resource, Default)]
struct Dragging(Option<Entity>);

#[allow(clippy::too_many_arguments)]
fn press(
    mut press: On<PointerPress>,
    geometry: Query<(&ComputedNode, &UiGlobalTransform)>,
    mut sliders: Query<&mut LocalSlider>,
    screens: Query<&bevy::window::Window>,
    children: Query<&Children>,
    parents: Query<&ChildOf>,
    mut parts: Parts,
    mut labels: Query<&mut Text, With<SliderLabel>>,
    mut windows: Windows,
    mut dragging: ResMut<Dragging>,
) {
    let entity = press.event_target();
    let (Ok(slider), Ok((node, transform))) = (sliders.get(entity), geometry.get(entity)) else {
        return;
    };
    press.propagate(false);
    let value = slider.value_at(fraction(
        press.pointer.position,
        window_scale(&screens),
        node,
        transform,
    ));
    dragging.0 = Some(entity);
    set_value(
        entity,
        value,
        &mut sliders,
        &children,
        &parents,
        &mut parts,
        &mut labels,
        &mut windows,
    );
}

#[allow(clippy::too_many_arguments)]
fn drag(
    mut drag: On<PointerDrag>,
    geometry: Query<(&ComputedNode, &UiGlobalTransform)>,
    mut sliders: Query<&mut LocalSlider>,
    screens: Query<&bevy::window::Window>,
    children: Query<&Children>,
    parents: Query<&ChildOf>,
    mut parts: Parts,
    mut labels: Query<&mut Text, With<SliderLabel>>,
    mut windows: Windows,
) {
    let entity = drag.event_target();
    let (Ok(slider), Ok((node, transform))) = (sliders.get(entity), geometry.get(entity)) else {
        return;
    };
    // A drag on a slider moves the knob, not the window.
    drag.propagate(false);
    let value = slider.value_at(fraction(
        drag.pointer.position,
        window_scale(&screens),
        node,
        transform,
    ));
    set_value(
        entity,
        value,
        &mut sliders,
        &children,
        &parents,
        &mut parts,
        &mut labels,
        &mut windows,
    );
}

#[allow(clippy::too_many_arguments)]
fn wheel(
    mut scroll: On<PointerScroll>,
    mut sliders: Query<&mut LocalSlider>,
    children: Query<&Children>,
    parents: Query<&ChildOf>,
    mut parts: Parts,
    mut labels: Query<&mut Text, With<SliderLabel>>,
    mut windows: Windows,
) {
    let entity = scroll.event_target();
    let Ok(slider) = sliders.get(entity) else {
        return;
    };
    // The wheel over a slider turns it instead of scrolling the window.
    scroll.propagate(false);
    let notches = match scroll.unit {
        MouseScrollUnit::Line => scroll.y,
        MouseScrollUnit::Pixel => scroll.y / 40.0,
    };
    if notches == 0.0 {
        return;
    }
    let value = slider.wheeled(notches);
    set_value(
        entity,
        value,
        &mut sliders,
        &children,
        &parents,
        &mut parts,
        &mut labels,
        &mut windows,
    );
}

fn over(
    over: On<PointerOver>,
    sliders: Query<(), With<LocalSlider>>,
    children: Query<&Children>,
    mut knobs: Query<&mut BackgroundColor, With<SliderKnob>>,
) {
    let entity = over.event_target();
    if sliders.contains(entity) {
        knob_highlight(entity, true, &children, &mut knobs);
    }
}

fn out(
    out: On<PointerOut>,
    sliders: Query<(), With<LocalSlider>>,
    children: Query<&Children>,
    mut knobs: Query<&mut BackgroundColor, With<SliderKnob>>,
    dragging: Res<Dragging>,
) {
    let entity = out.event_target();
    if sliders.contains(entity) && dragging.0 != Some(entity) {
        knob_highlight(entity, false, &children, &mut knobs);
    }
}

fn drag_end(
    end: On<PointerDragEnd>,
    sliders: Query<(), With<LocalSlider>>,
    children: Query<&Children>,
    mut knobs: Query<&mut BackgroundColor, With<SliderKnob>>,
    mut dragging: ResMut<Dragging>,
) {
    let entity = end.event_target();
    if sliders.contains(entity) {
        dragging.0 = None;
        knob_highlight(entity, false, &children, &mut knobs);
    }
}

pub(crate) fn plugin(app: &mut App) {
    app.init_resource::<Dragging>()
        .add_observer(press)
        .add_observer(drag)
        .add_observer(drag_end)
        .add_observer(wheel)
        .add_observer(over)
        .add_observer(out);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_are_continuous_within_the_range_and_the_wheel_nudges_them() {
        let s = LocalSlider {
            key: "v".into(),
            min: 0.0,
            max: 100.0,
            value: 50.0,
            wheel_step: 1.0,
        };
        assert_eq!(s.value_at(0.0), 0.0);
        assert_eq!(s.value_at(1.0), 100.0);
        assert!((s.value_at(0.423) - 42.3).abs() < 1e-4);
        assert_eq!(s.value_at(2.0), 100.0);
        assert_eq!(s.fraction_of(25.0), 0.25);
        assert_eq!(s.fraction_of(-10.0), 0.0);
        assert_eq!(s.wheeled(3.0), 53.0);
        assert_eq!(s.wheeled(-80.0), 0.0);
        assert_eq!(stored(42.345), "42.35");
        assert_eq!(label(42.6), "43%");
    }
}
