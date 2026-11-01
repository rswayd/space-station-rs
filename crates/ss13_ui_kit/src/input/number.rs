//! `NumberInput`: drag vertically to change by `step`, click to type. Port of tgui-core 6.1
//! `NumberInput` (drag starts after 250 ms or 4 px, value moves one `step` per
//! `step_pixel_size` px, typed values are clamped but not snapped).

use bevy::app::PropagateStop;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input_focus::tab_navigation::TabIndex;
use bevy::input_focus::{FocusCause, FocusLost, FocusedInput, InputFocus};
use bevy::picking::events::{PointerDrag, PointerDragEnd, PointerPress, PointerRelease};
use bevy::picking::pointer::PointerButton;
use bevy::prelude::*;
use bevy::text::{EditableText, TextCursorStyle, TextEdit};
use bevy::ui_widgets::TextInput;

use super::{root_font, style};

/// tgui `NumberInput`. `value` is written by the widget when the user commits a change (and
/// [`NumberInputChanged`] fires); game code may write it at any time.
#[derive(Component, Clone, Debug)]
#[require(Node)]
pub struct NumberInput {
    pub value: f64,
    pub min_value: f64,
    pub max_value: f64,
    /// Drag granularity. Typed values are not snapped to it (as in tgui).
    pub step: f64,
    /// Pointer travel, in logical px, per `step`.
    pub step_pixel_size: f32,
    pub unit: String,
    /// Also fire [`NumberInputChanged`] every 400 ms while dragging.
    pub tick_while_dragging: bool,
    pub format: Option<fn(f64) -> String>,
    /// Width 100% (only applied when the root `Node` width is `Auto`).
    pub fluid: bool,
}

impl Default for NumberInput {
    fn default() -> Self {
        Self {
            value: 0.0,
            min_value: f64::NEG_INFINITY,
            max_value: f64::INFINITY,
            step: 1.0,
            step_pixel_size: 1.0,
            unit: String::new(),
            tick_while_dragging: false,
            format: None,
            fluid: false,
        }
    }
}

/// Fired on the [`NumberInput`] root when a drag ends or a typed number is entered, only if the
/// value differs from the one before the gesture (tgui fires on every drag end; skipping no-op
/// changes saves a server round trip).
#[derive(EntityEvent, Clone, Debug)]
pub struct NumberInputChanged {
    pub entity: Entity,
    pub value: f64,
}

const DRAG_DELAY_SECS: f32 = 0.25;
const DRAG_THRESHOLD_PX: f32 = 4.0;
const TICK_SECS: f32 = 0.4;
/// Typed entry is parsed as a prefix (O(n^2)); a cap keeps it trivial.
const MAX_TYPED_CHARS: usize = 32;

/// One press-move-release gesture on the number display.
#[derive(Clone, Debug, PartialEq)]
struct DragGesture {
    origin_y: f32,
    current: f64,
    /// Last value reported (gesture start or last tick).
    reported: f64,
    dragging: bool,
    started_at: f32,
    last_tick: f32,
}

enum GestureEnd {
    Drag(f64),
    Click,
}

impl DragGesture {
    fn new(value: f64, y: f32, now: f32) -> Self {
        Self {
            origin_y: y,
            current: value,
            reported: value,
            dragging: false,
            started_at: now,
            last_tick: now,
        }
    }

    fn move_to(&mut self, y: f32, now: f32, props: &NumberInput) {
        if !self.dragging && now - self.started_at >= DRAG_DELAY_SECS {
            self.dragging = true;
        }
        // Screen y grows downward; moving up increases the value.
        let offset = self.origin_y - y;
        if !self.dragging {
            self.dragging = offset.abs() > DRAG_THRESHOLD_PX;
            return;
        }
        let step = props.step();
        let pixels = if props.step_pixel_size > 0.0 {
            props.step_pixel_size
        } else {
            1.0
        };
        let target = clamp(
            self.current + f64::from(offset) * step / f64::from(pixels),
            props.min_value - step,
            props.max_value + step,
        );
        if (target - self.current).abs() >= step {
            self.current = clamp(
                (target / step).round() * step,
                props.min_value,
                props.max_value,
            );
            self.origin_y = y;
        } else if offset.abs() > pixels {
            self.origin_y = y;
        }
    }

    /// `tick_while_dragging`: the value to report now, if any.
    fn tick(&mut self, now: f32) -> Option<f64> {
        if !self.dragging || now - self.last_tick < TICK_SECS {
            return None;
        }
        self.last_tick = now;
        (self.current != self.reported).then(|| {
            self.reported = self.current;
            self.current
        })
    }

    fn end(&self, now: f32) -> GestureEnd {
        if self.dragging || now - self.started_at >= DRAG_DELAY_SECS {
            GestureEnd::Drag(self.current)
        } else {
            GestureEnd::Click
        }
    }
}

impl NumberInput {
    fn step(&self) -> f64 {
        if self.step.is_finite() && self.step > 0.0 {
            self.step
        } else {
            1.0
        }
    }

    fn display(&self, value: f64) -> String {
        let text = self.format.map_or_else(|| value.to_string(), |f| f(value));
        if self.unit.is_empty() {
            text
        } else {
            format!("{text} {}", self.unit)
        }
    }

    /// Typed entry: tgui clamps but does not snap to `step`; unparsable text is ignored.
    fn parse_typed(&self, text: &str) -> Option<f64> {
        parse_number(text).map(|v| clamp(v, self.min_value, self.max_value))
    }

    /// Fill of the left bar, 0..=1.
    fn fill(&self, value: f64) -> f32 {
        let fill = (value - self.min_value) / (self.max_value - self.min_value);
        if fill.is_finite() {
            fill.clamp(0.0, 1.0) as f32
        } else {
            0.0
        }
    }
}

/// JS-style clamp: never panics, NaN passes through.
fn clamp(value: f64, min: f64, max: f64) -> f64 {
    if value < min {
        min
    } else if value > max {
        max
    } else {
        value
    }
}

/// `parseFloat`: longest numeric prefix after leading whitespace. Unlike JS, `,` is accepted as
/// the decimal separator because Russian layouts put it on the numpad.
fn parse_number(text: &str) -> Option<f64> {
    let text = text.trim_start().replace(',', ".");
    let end = text
        .find(|c: char| !(c.is_ascii_digit() || matches!(c, '+' | '-' | '.' | 'e' | 'E')))
        .unwrap_or(text.len());
    (1..=end)
        .rev()
        .find_map(|i| text[..i].parse::<f64>().ok())
        .filter(|v| v.is_finite())
}

/// Internal state and children of a `NumberInput` root.
#[derive(Component)]
pub(super) struct NumberParts {
    content: Entity,
    bar: Entity,
    pub(super) field: Entity,
    gesture: Option<DragGesture>,
    editing: bool,
}

pub(super) fn build(app: &mut App) {
    // The pointer observers are global; without `TimePlugin` (headless apps) they would error on
    // every press anywhere. `init_resource` keeps an existing clock.
    app.init_resource::<Time<Real>>()
        .add_observer(spawn_parts)
        .add_observer(on_press)
        .add_observer(on_drag)
        .add_observer(on_release)
        .add_observer(on_drag_end)
        .add_observer(on_field_blur)
        .add_systems(Update, (tick_drag, sync_display).chain());
}

fn spawn_parts(
    add: On<Add<NumberInput>>,
    mut roots: Query<(&NumberInput, Option<&TextFont>, &mut Node)>,
    mut commands: Commands,
) {
    let root = add.event_target();
    let Ok((props, font, mut node)) = roots.get_mut(root) else {
        return;
    };
    let font = root_font(font);

    node.position_type = PositionType::Relative;
    node.justify_content = JustifyContent::FlexEnd;
    node.border = UiRect::all(px(style::BORDER_PX));
    node.border_radius = BorderRadius::all(px(style::RADIUS_PX));
    node.padding = UiRect::horizontal(px(4.0));
    node.margin.right = px(2.0);
    node.min_height = px(17.0);
    if props.fluid && node.width == Val::Auto {
        node.width = percent(100);
    }

    let bar = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: px(2.0),
                bottom: px(2.0),
                left: px(2.0),
                width: px(3.0),
                ..default()
            },
            Pickable::IGNORE,
            ChildOf(root),
        ))
        .with_child((
            Node {
                position_type: PositionType::Absolute,
                bottom: px(0.0),
                left: px(0.0),
                width: px(3.0),
                top: bar_top(props.fill(props.value)),
                ..default()
            },
            BackgroundColor(style::BORDER_FOCUS),
            Pickable::IGNORE,
        ))
        .id();
    let content = commands
        .spawn((
            Node {
                margin: UiRect::left(px(6.0)),
                ..default()
            },
            Text::new(props.display(props.value)),
            font.clone(),
            TextColor(style::BORDER_FOCUS),
            TextLayout::no_wrap(),
            Pickable::IGNORE,
            ChildOf(root),
        ))
        .id();
    let field = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(0.0),
                right: px(0.0),
                top: px(0.0),
                bottom: px(0.0),
                padding: UiRect::horizontal(px(6.0)),
                display: Display::None,
                ..default()
            },
            TextInput,
            EditableText {
                max_characters: Some(MAX_TYPED_CHARS),
                ..default()
            },
            TextLayout {
                justify: Justify::Right,
                ..TextLayout::no_wrap()
            },
            font,
            TextColor(style::TEXT),
            TextCursorStyle {
                color: style::TEXT,
                selection_color: style::SELECTION,
                unfocused_selection_color: Color::NONE,
                ..default()
            },
            BackgroundColor(style::BACKGROUND),
            // Click-focusable while editing, never reached by Tab (tgui's input is hidden).
            TabIndex(-1),
            ChildOf(root),
        ))
        // Entity observer: runs after Bevy's global text-input one, so it can stop Enter.
        .observe(on_field_key)
        .id();

    commands.entity(root).insert((
        NumberParts {
            content,
            bar,
            field,
            gesture: None,
            editing: false,
        },
        // The parts pick their own colors; a window's inherited text color must not repaint them.
        PropagateStop::<TextColor>::default(),
        BackgroundColor(style::BACKGROUND),
        BorderColor::all(style::BORDER),
    ));
}

fn on_press(
    press: On<PointerPress>,
    mut roots: Query<(&NumberInput, &mut NumberParts)>,
    time: Res<Time<Real>>,
) {
    if press.button != PointerButton::Primary {
        return;
    }
    let Ok((props, mut parts)) = roots.get_mut(press.event_target()) else {
        return;
    };
    if parts.editing {
        return;
    }
    parts.gesture = Some(DragGesture::new(
        props.value,
        press.pointer.position.y,
        time.elapsed_secs(),
    ));
}

fn on_drag(
    drag: On<PointerDrag>,
    mut roots: Query<(&NumberInput, &mut NumberParts)>,
    time: Res<Time<Real>>,
) {
    if drag.button != PointerButton::Primary {
        return;
    }
    if let Ok((props, mut parts)) = roots.get_mut(drag.event_target())
        && let Some(gesture) = parts.gesture.as_mut()
    {
        gesture.move_to(drag.pointer.position.y, time.elapsed_secs(), props);
    }
}

fn on_release(
    release: On<PointerRelease>,
    roots: Query<(&mut NumberInput, &mut NumberParts)>,
    fields: Query<&mut EditableText>,
    time: Res<Time<Real>>,
    commands: Commands,
) {
    if release.button == PointerButton::Primary {
        finish_gesture(release.event_target(), roots, fields, &time, commands);
    }
}

/// A drag released outside the root only reports `DragEnd`.
fn on_drag_end(
    end: On<PointerDragEnd>,
    roots: Query<(&mut NumberInput, &mut NumberParts)>,
    fields: Query<&mut EditableText>,
    time: Res<Time<Real>>,
    commands: Commands,
) {
    if end.button == PointerButton::Primary {
        finish_gesture(end.event_target(), roots, fields, &time, commands);
    }
}

fn finish_gesture(
    root: Entity,
    mut roots: Query<(&mut NumberInput, &mut NumberParts)>,
    mut fields: Query<&mut EditableText>,
    time: &Time<Real>,
    mut commands: Commands,
) {
    let Ok((mut props, mut parts)) = roots.get_mut(root) else {
        return;
    };
    // Release and DragEnd both arrive for a drag that ends over the root; the first one wins.
    let Some(gesture) = parts.gesture.take() else {
        return;
    };
    match gesture.end(time.elapsed_secs()) {
        GestureEnd::Drag(value) => {
            props.value = value;
            if value != gesture.reported {
                commands.trigger(NumberInputChanged {
                    entity: root,
                    value,
                });
            }
        }
        GestureEnd::Click => {
            let Ok(mut editable) = fields.get_mut(parts.field) else {
                return;
            };
            start_editing(&mut parts, props.value, &mut editable);
            let field = parts.field;
            // Queued so it lands after the press's own `AcquireFocus`, which blurs on the root.
            commands.queue(move |world: &mut World| {
                world
                    .resource_mut::<InputFocus>()
                    .set(field, FocusCause::Pressed);
            });
        }
    }
}

/// tgui puts the raw value (not `format`ted) into the field, all selected.
fn start_editing(parts: &mut NumberParts, value: f64, editable: &mut EditableText) {
    parts.editing = true;
    editable.editor_mut().set_text(&value.to_string());
    editable.queue_edit(TextEdit::SelectAll);
}

#[cfg(test)]
pub(super) fn begin_editing_for_test(world: &mut World, root: Entity) {
    let value = world.get::<NumberInput>(root).unwrap().value;
    let field = world.get::<NumberParts>(root).unwrap().field;
    let mut editable = world.get_mut::<EditableText>(field).unwrap().clone();
    start_editing(
        &mut world.get_mut::<NumberParts>(root).unwrap(),
        value,
        &mut editable,
    );
    *world.get_mut::<EditableText>(field).unwrap() = editable;
}

fn on_field_key(
    mut key: On<FocusedInput<KeyboardInput>>,
    fields: Query<(&EditableText, &ChildOf)>,
    mut roots: Query<&mut NumberParts>,
    mut focus: ResMut<InputFocus>,
    mut commands: Commands,
) {
    if !key.input.state.is_pressed() {
        return;
    }
    let Ok((editable, &ChildOf(root))) = fields.get(key.event_target()) else {
        return;
    };
    let Ok(mut parts) = roots.get_mut(root) else {
        return;
    };
    if parts.field != key.event_target() || editable.is_composing() {
        return;
    }
    match key.input.logical_key {
        Key::Enter => {
            let text = editable.value().to_string();
            commands.queue(move |world: &mut World| commit_typed(world, root, &text));
            parts.editing = false;
            focus.clear();
            key.propagate(false);
        }
        // Bevy's text input blurs on Escape; dropping `editing` first makes the blur a cancel.
        Key::Escape => parts.editing = false,
        _ => {}
    }
}

fn on_field_blur(
    lost: On<FocusLost>,
    fields: Query<(&EditableText, &ChildOf)>,
    mut roots: Query<&mut NumberParts>,
    mut commands: Commands,
) {
    let Ok((editable, &ChildOf(root))) = fields.get(lost.event_target()) else {
        return;
    };
    let Ok(mut parts) = roots.get_mut(root) else {
        return;
    };
    if parts.field != lost.event_target() {
        return;
    }
    if parts.editing {
        parts.editing = false;
        let text = editable.value().to_string();
        commands.queue(move |world: &mut World| commit_typed(world, root, &text));
    }
}

fn commit_typed(world: &mut World, root: Entity, text: &str) {
    let Some(mut props) = world.get_mut::<NumberInput>(root) else {
        return;
    };
    let Some(value) = props.parse_typed(text) else {
        return;
    };
    if value != props.value {
        props.value = value;
        world.trigger(NumberInputChanged {
            entity: root,
            value,
        });
    }
}

fn tick_drag(
    mut roots: Query<(Entity, &NumberInput, &mut NumberParts)>,
    time: Res<Time<Real>>,
    mut commands: Commands,
) {
    let now = time.elapsed_secs();
    for (entity, props, mut parts) in &mut roots {
        if !props.tick_while_dragging {
            continue;
        }
        if let Some(value) = parts.gesture.as_mut().and_then(|g| g.tick(now)) {
            commands.trigger(NumberInputChanged { entity, value });
        }
    }
}

/// Redraws the value text, bar and field visibility when props or gesture state change.
fn sync_display(
    roots: Query<(&NumberInput, &NumberParts), Or<(Changed<NumberInput>, Changed<NumberParts>)>>,
    mut texts: Query<&mut Text>,
    mut nodes: Query<&mut Node>,
    children: Query<&Children>,
) {
    for (props, parts) in &roots {
        let shown = parts
            .gesture
            .as_ref()
            .filter(|g| g.dragging)
            .map_or(props.value, |g| g.current);
        if let Ok(mut text) = texts.get_mut(parts.content) {
            let display = props.display(shown);
            if text.0 != display {
                text.0 = display;
            }
        }
        // Percent heights do not resolve inside an inset-sized absolute parent; insets do.
        let fill = bar_top(props.fill(shown));
        for bar in children.iter_descendants(parts.bar) {
            if let Ok(mut node) = nodes.get_mut(bar)
                && node.top != fill
            {
                node.top = fill;
            }
        }
        for (entity, display) in [(parts.field, Display::Flex), (parts.content, Display::None)] {
            if let Ok(mut node) = nodes.get_mut(entity) {
                let display = if parts.editing {
                    display
                } else {
                    flip(display)
                };
                if node.display != display {
                    node.display = display;
                }
            }
        }
    }
}

fn bar_top(fill: f32) -> Val {
    percent((1.0 - fill) * 100.0)
}

fn flip(display: Display) -> Display {
    if display == Display::None {
        Display::Flex
    } else {
        Display::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn props(min: f64, max: f64, step: f64) -> NumberInput {
        NumberInput {
            min_value: min,
            max_value: max,
            step,
            ..default()
        }
    }

    #[test]
    fn small_moves_before_delay_are_a_click() {
        let p = props(0.0, 100.0, 1.0);
        let mut g = DragGesture::new(10.0, 100.0, 0.0);
        g.move_to(97.0, 0.1, &p);
        assert!(!g.dragging);
        assert!(matches!(g.end(0.1), GestureEnd::Click));
    }

    #[test]
    fn holding_past_delay_is_a_drag_without_change() {
        let g = DragGesture::new(10.0, 100.0, 0.0);
        assert!(matches!(g.end(0.3), GestureEnd::Drag(v) if v == 10.0));
    }

    #[test]
    fn drag_up_increases_by_step_per_pixel_and_down_decreases() {
        let p = props(0.0, 100.0, 1.0);
        let mut g = DragGesture::new(10.0, 100.0, 0.0);
        g.move_to(95.0, 0.05, &p); // threshold crossed: starts dragging, no change yet
        assert!(g.dragging);
        assert_eq!(g.current, 10.0);
        // Like tgui, the origin is not reset when dragging starts: the first 10 px all count.
        g.move_to(90.0, 0.06, &p);
        assert_eq!(g.current, 20.0);
        g.move_to(97.0, 0.07, &p);
        assert_eq!(g.current, 13.0);
    }

    #[test]
    fn drag_snaps_to_step_and_respects_step_pixel_size() {
        let p = NumberInput {
            step_pixel_size: 5.0,
            ..props(0.0, 10.0, 0.5)
        };
        let mut g = DragGesture::new(1.0, 100.0, 0.0);
        g.dragging = true;
        g.move_to(97.0, 0.0, &p); // 3 px < 5 px per step: no change, origin kept
        assert_eq!(g.current, 1.0);
        g.move_to(90.0, 0.0, &p); // 10 px from origin: two steps
        assert_eq!(g.current, 2.0);
    }

    #[test]
    fn drag_clamps_to_range() {
        let p = props(0.0, 20.0, 1.0);
        let mut g = DragGesture::new(18.0, 100.0, 0.0);
        g.dragging = true;
        g.move_to(0.0, 0.0, &p);
        assert_eq!(g.current, 20.0);
        g.move_to(500.0, 0.0, &p);
        assert_eq!(g.current, 0.0);
    }

    #[test]
    fn tick_reports_only_changes_every_400ms() {
        let p = props(0.0, 100.0, 1.0);
        let mut g = DragGesture::new(0.0, 100.0, 0.0);
        g.move_to(90.0, 0.3, &p);
        assert_eq!(g.tick(0.3), None, "interval not elapsed");
        assert_eq!(g.tick(0.45), Some(10.0));
        assert_eq!(g.tick(0.9), None, "unchanged since last report");
    }

    #[test]
    fn typed_entry_parses_like_parse_float_and_clamps() {
        let p = props(-5.0, 50.0, 1.0);
        assert_eq!(p.parse_typed("  12abc"), Some(12.0));
        assert_eq!(p.parse_typed("1e1"), Some(10.0));
        assert_eq!(p.parse_typed("2,5"), Some(2.5));
        assert_eq!(p.parse_typed("999"), Some(50.0));
        assert_eq!(p.parse_typed("-100"), Some(-5.0));
        assert_eq!(
            p.parse_typed("3.3"),
            Some(3.3),
            "typed values are not snapped"
        );
        assert_eq!(p.parse_typed("abc"), None);
        assert_eq!(p.parse_typed(""), None);
        assert_eq!(p.parse_typed("inf"), None);
    }

    #[test]
    fn display_and_fill() {
        let p = NumberInput {
            unit: "кПа".into(),
            ..props(0.0, 200.0, 1.0)
        };
        assert_eq!(p.display(101.0), "101 кПа");
        assert_eq!(p.fill(50.0), 0.25);
        assert_eq!(p.fill(-1.0), 0.0);
        assert_eq!(
            NumberInput::default().fill(5.0),
            0.0,
            "unbounded range has no bar"
        );
        let f = NumberInput {
            format: Some(|v| format!("{v:.1}")),
            ..default()
        };
        assert_eq!(f.display(1.0), "1.0");
    }
}
