//! `Input` and `TextArea`: a decorated root node holding a Bevy `EditableText` field.

use bevy::app::PropagateStop;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input_focus::tab_navigation::TabIndex;
use bevy::input_focus::{
    AcquireFocus, AutoFocus, FocusCause, FocusGained, FocusLost, FocusedInput, InputFocus,
};
use bevy::prelude::*;
use bevy::text::{EditableText, EditableTextFilter, TextCursorStyle, TextEditChange};
use bevy::ui_widgets::{SelectAllOnFocus, TextInput};

use super::{root_font, style};

/// tgui `Input`: single-line text field.
///
/// `value` is the current text. The widget writes it on every edit (and fires [`InputChanged`]);
/// game code may write it too, which updates the field only while it is not focused, as in tgui.
#[derive(Component, Clone, Debug, Default)]
#[require(Node)]
pub struct Input {
    pub value: String,
    pub placeholder: String,
    /// Maximum length in Unicode scalar values (not bytes).
    pub max_length: Option<usize>,
    /// Focus the field when spawned.
    pub auto_focus: bool,
    /// Focus the field when spawned and select all text whenever it gains focus.
    pub auto_select: bool,
    /// Clear the text after Enter.
    pub self_clear: bool,
    /// Width 100% instead of tgui's 9em (only applied when the root `Node` width is `Auto`).
    pub fluid: bool,
}

/// tgui `TextArea`: add next to an [`Input`] to make it multi-line.
///
/// Enter submits, Shift+Enter inserts a newline, Tab inserts `\t` unless `dont_use_tab_for_indent`.
#[derive(Component, Clone, Debug)]
#[require(Input)]
pub struct TextArea {
    pub dont_use_tab_for_indent: bool,
    /// Visible rows; more text scrolls.
    pub rows: f32,
}

impl Default for TextArea {
    fn default() -> Self {
        Self {
            dont_use_tab_for_indent: false,
            rows: 2.0,
        }
    }
}

/// Fired on the [`Input`] root whenever the text changes (tgui `onChange`). Not fired for IME
/// preedit or for changes made by game code.
#[derive(EntityEvent, Clone, Debug)]
pub struct InputChanged {
    pub entity: Entity,
    pub value: String,
}

/// Fired on the [`Input`] root when Enter is pressed (tgui `onEnter`); the field then blurs.
#[derive(EntityEvent, Clone, Debug)]
pub struct InputSubmitted {
    pub entity: Entity,
    pub value: String,
}

/// Internal: links an `Input` root to its children.
#[derive(Component)]
struct InputParts {
    field: Entity,
    placeholder: Entity,
}

pub(super) fn build(app: &mut App) {
    app.add_observer(spawn_parts)
        .add_observer(on_text_edit)
        .add_observer(redirect_focus_to_field)
        .add_observer(on_focus_gained)
        .add_observer(on_focus_lost)
        .add_systems(Update, sync_props);
}

fn spawn_parts(
    add: On<Add<Input>>,
    mut roots: Query<(&Input, Option<&TextArea>, Option<&TextFont>, &mut Node)>,
    mut commands: Commands,
) {
    let root = add.event_target();
    let Ok((input, area, font, mut node)) = roots.get_mut(root) else {
        return;
    };
    let font = root_font(font);

    node.border = UiRect::all(px(style::BORDER_PX));
    node.border_radius = BorderRadius::all(px(style::RADIUS_PX));
    // tgui padding: --space-xxs --space-sm (0.085em 0.33em).
    node.padding = UiRect::axes(px(4.0), px(1.0));
    if node.width == Val::Auto {
        node.width = if input.fluid {
            percent(100)
        } else {
            px(style::INPUT_WIDTH_PX)
        };
    }

    let mut editable = EditableText::new(&input.value);
    editable.max_characters = input.max_length;
    let (layout, filter) = match area {
        Some(area) => {
            editable.visible_lines = Some(area.rows);
            (TextLayout::default(), EditableTextFilter::default())
        }
        // `allow_newlines` only gates the Enter key; pasted newlines must be filtered too.
        // shortcut: a paste containing a newline is rejected whole (Bevy filters per edit),
        // strip newlines instead if players complain.
        None => (
            TextLayout::no_wrap(),
            EditableTextFilter::new(single_line_char),
        ),
    };

    let mut field = commands.spawn((
        Node {
            flex_grow: 1.0,
            ..default()
        },
        TextInput,
        editable,
        filter,
        layout,
        font.clone(),
        TextColor(style::TEXT),
        TextCursorStyle {
            color: style::TEXT,
            selection_color: style::SELECTION,
            unfocused_selection_color: Color::NONE,
            ..default()
        },
        TabIndex(0),
        ChildOf(root),
    ));
    // Entity observers run after global ones, so our `propagate(false)` overrides Bevy's text
    // input observer, which re-enables propagation for keys it ignores (Enter, Tab).
    field.observe(on_key);
    if input.auto_select {
        field.insert(SelectAllOnFocus);
    }
    if input.auto_focus || input.auto_select {
        field.insert(AutoFocus);
    }
    let field = field.id();

    let placeholder = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(4.0),
                top: px(1.0),
                display: display_if(input.value.is_empty()),
                ..default()
            },
            Text::new(input.placeholder.clone()),
            TextFont {
                style: FontStyle::Italic,
                ..font
            },
            // Inside a tgui root the theme rebuilds `TextFont` from inherited pieces.
            crate::theme::Italic(true),
            TextColor(style::PLACEHOLDER),
            TextLayout::no_wrap(),
            Pickable::IGNORE,
            ChildOf(root),
        ))
        .id();

    commands.entity(root).insert((
        InputParts { field, placeholder },
        // The parts pick their own colors; a window's inherited text color must not repaint them.
        PropagateStop::<TextColor>::default(),
        BackgroundColor(style::BACKGROUND),
        BorderColor::all(style::BORDER),
    ));
}

pub(super) fn single_line_char(c: char) -> bool {
    c != '\n' && c != '\r'
}

fn display_if(visible: bool) -> Display {
    if visible {
        Display::Flex
    } else {
        Display::None
    }
}

/// Mirrors user edits into `Input::value` and fires [`InputChanged`].
fn on_text_edit(
    change: On<TextEditChange>,
    fields: Query<(&EditableText, &ChildOf)>,
    mut roots: Query<(&mut Input, &InputParts)>,
    mut nodes: Query<&mut Node>,
    mut commands: Commands,
) {
    let Ok((editable, &ChildOf(root))) = fields.get(change.event_target()) else {
        return;
    };
    let Ok((mut input, parts)) = roots.get_mut(root) else {
        return;
    };
    if let Ok(mut node) = nodes.get_mut(parts.placeholder) {
        let display = display_if(editable.editor().raw_text().is_empty());
        if node.display != display {
            node.display = display;
        }
    }
    if editable.is_composing() || editable.value() == input.value.as_str() {
        return;
    }
    let value = editable.value().to_string();
    input.value.clone_from(&value);
    commands.trigger(InputChanged {
        entity: root,
        value,
    });
}

fn on_key(
    mut key: On<FocusedInput<KeyboardInput>>,
    mut fields: Query<(&mut EditableText, &ChildOf)>,
    mut roots: Query<(&mut Input, Option<&TextArea>)>,
    keys: Res<ButtonInput<Key>>,
    mut focus: ResMut<InputFocus>,
    mut commands: Commands,
) {
    if !key.input.state.is_pressed() {
        return;
    }
    let Ok((mut editable, &ChildOf(root))) = fields.get_mut(key.event_target()) else {
        return;
    };
    let Ok((mut input, area)) = roots.get_mut(root) else {
        return;
    };
    if editable.is_composing() {
        return;
    }
    let modifiers = [Key::Shift, Key::Control, Key::Alt, Key::Super];
    let no_modifiers = !keys.any_pressed(modifiers.clone());
    let only_shift = keys.pressed(Key::Shift) && !keys.any_pressed(modifiers[1..].iter().cloned());

    match (&key.input.logical_key, area) {
        (Key::Enter, Some(_)) if only_shift => {
            editable.queue_edit(bevy::text::TextEdit::Insert("\n".into()));
        }
        (Key::Tab, Some(area)) if no_modifiers && !area.dont_use_tab_for_indent => {
            editable.queue_edit(bevy::text::TextEdit::Insert("\t".into()));
        }
        (Key::Enter, _) if no_modifiers => {
            commands.trigger(InputSubmitted {
                entity: root,
                value: editable.value().to_string(),
            });
            if input.self_clear {
                // Clear the mirror first so the clear is not reported as a change, like tgui.
                input.value.clear();
                editable.clear();
            }
            focus.clear();
        }
        _ => return,
    }
    key.propagate(false);
}

/// Pushes game-side writes of `Input` (value, placeholder, max length) into the field.
fn sync_props(
    roots: Query<(&Input, &InputParts), Changed<Input>>,
    mut fields: Query<&mut EditableText>,
    mut texts: Query<(&mut Text, &mut Node)>,
    focus: Res<InputFocus>,
) {
    for (input, parts) in &roots {
        if let Ok(mut editable) = fields.get_mut(parts.field) {
            if editable.max_characters != input.max_length {
                editable.max_characters = input.max_length;
            }
            if focus.get() != Some(parts.field) && editable.value() != input.value.as_str() {
                editable.editor_mut().set_text(&input.value);
                editable.queue_edit(bevy::text::TextEdit::TextEnd(false));
            }
        }
        if let Ok((mut text, mut node)) = texts.get_mut(parts.placeholder) {
            if text.0 != input.placeholder {
                text.0.clone_from(&input.placeholder);
            }
            let display = display_if(input.value.is_empty());
            if node.display != display {
                node.display = display;
            }
        }
    }
}

/// A click on the root's padding/border focuses the field instead of blurring.
fn redirect_focus_to_field(
    mut acquire: On<AcquireFocus>,
    roots: Query<&InputParts>,
    mut focus: ResMut<InputFocus>,
) {
    if let Ok(parts) = roots.get(acquire.event_target()) {
        acquire.propagate(false);
        if focus.get() != Some(parts.field) {
            focus.set(parts.field, FocusCause::Pressed);
        }
    }
}

fn on_focus_gained(gained: On<FocusGained>, mut roots: Query<&mut BorderColor, With<InputParts>>) {
    if let Ok(mut border) = roots.get_mut(gained.event_target()) {
        *border = BorderColor::all(style::BORDER_FOCUS);
    }
}

fn on_focus_lost(lost: On<FocusLost>, mut roots: Query<&mut BorderColor, With<InputParts>>) {
    if let Ok(mut border) = roots.get_mut(lost.event_target()) {
        *border = BorderColor::all(style::BORDER);
    }
}

#[cfg(test)]
mod tests {
    use bevy::clipboard::Clipboard;
    use bevy::input::ButtonState;
    use bevy::input::keyboard::KeyCode;
    use bevy::input_focus::tab_navigation::{TabGroup, TabNavigationPlugin};
    use bevy::input_focus::{InputDispatchPlugin, InputFocusVisible};
    use bevy::text::{FontCx, LayoutCx, TextEdit, apply_text_edits};
    use bevy::window::PrimaryWindow;

    use super::super::{NumberInput, NumberInputChanged};
    use super::*;

    #[derive(Resource, Default)]
    struct Seen(Vec<String>);

    fn app() -> (App, Entity) {
        let mut app = App::new();
        app.add_plugins((bevy::input::InputPlugin, InputDispatchPlugin))
            .init_resource::<InputFocus>()
            .init_resource::<InputFocusVisible>()
            .init_resource::<FontCx>()
            .init_resource::<LayoutCx>()
            .init_resource::<Clipboard>()
            .init_resource::<Seen>()
            .add_systems(PostUpdate, apply_text_edits)
            // No `VisibilityPlugin` here, and tab navigation skips hidden entities.
            .add_systems(PostUpdate, |mut q: Query<&mut InheritedVisibility>| {
                q.iter_mut()
                    .for_each(|mut v| *v = InheritedVisibility::VISIBLE);
            })
            .add_observer(|e: On<InputChanged>, mut seen: ResMut<Seen>| {
                seen.0.push(format!("changed:{}", e.value));
            })
            .add_observer(|e: On<InputSubmitted>, mut seen: ResMut<Seen>| {
                seen.0.push(format!("submitted:{}", e.value));
            })
            .add_observer(|e: On<NumberInputChanged>, mut seen: ResMut<Seen>| {
                seen.0.push(format!("number:{}", e.value));
            });
        app.add_plugins(TabNavigationPlugin);
        build(&mut app);
        super::super::number::build(&mut app);
        let window = app
            .world_mut()
            .spawn((Window::default(), PrimaryWindow))
            .id();
        (app, window)
    }

    fn press(app: &mut App, window: Entity, key_code: KeyCode, logical_key: Key) {
        app.world_mut().write_message(KeyboardInput {
            key_code,
            logical_key,
            state: ButtonState::Pressed,
            text: None,
            repeat: false,
            window,
        });
    }

    fn field_of(app: &mut App, root: Entity) -> Entity {
        app.update();
        app.world().get::<InputParts>(root).unwrap().field
    }

    fn type_text(app: &mut App, field: Entity, text: &str) {
        app.world_mut()
            .get_mut::<EditableText>(field)
            .unwrap()
            .queue_edit(TextEdit::Insert(text.into()));
        app.update();
    }

    fn seen(app: &mut App) -> Vec<String> {
        std::mem::take(&mut app.world_mut().resource_mut::<Seen>().0)
    }

    #[test]
    fn typing_mirrors_value_and_fires_changed() {
        let (mut app, _) = app();
        let root = app.world_mut().spawn(Input::default()).id();
        let field = field_of(&mut app, root);
        type_text(&mut app, field, "Привет");
        assert_eq!(app.world().get::<Input>(root).unwrap().value, "Привет");
        assert_eq!(seen(&mut app), ["changed:Привет"]);
    }

    #[test]
    fn enter_submits_self_clears_and_blurs() {
        let (mut app, window) = app();
        let root = app
            .world_mut()
            .spawn(Input {
                self_clear: true,
                ..default()
            })
            .id();
        let field = field_of(&mut app, root);
        app.world_mut()
            .resource_mut::<InputFocus>()
            .set(field, FocusCause::Navigated);
        type_text(&mut app, field, "ура");
        seen(&mut app);
        press(&mut app, window, KeyCode::Enter, Key::Enter);
        app.update();
        assert_eq!(
            seen(&mut app),
            ["submitted:ура"],
            "self-clear is not a change"
        );
        assert_eq!(app.world().get::<Input>(root).unwrap().value, "");
        assert_eq!(app.world().resource::<InputFocus>().get(), None);
    }

    #[test]
    fn text_area_shift_enter_inserts_newline_and_tab_indents() {
        let (mut app, window) = app();
        let group = app.world_mut().spawn(TabGroup::new(0)).id();
        let root = app
            .world_mut()
            .spawn((TextArea::default(), ChildOf(group)))
            .id();
        let other = app
            .world_mut()
            .spawn((Input::default(), ChildOf(group)))
            .id();
        let field = field_of(&mut app, root);
        let other_field = field_of(&mut app, other);
        app.world_mut()
            .resource_mut::<InputFocus>()
            .set(field, FocusCause::Navigated);
        type_text(&mut app, field, "а");
        press(&mut app, window, KeyCode::ShiftLeft, Key::Shift);
        press(&mut app, window, KeyCode::Enter, Key::Enter);
        app.update();
        app.world_mut().write_message(KeyboardInput {
            key_code: KeyCode::ShiftLeft,
            logical_key: Key::Shift,
            state: ButtonState::Released,
            text: None,
            repeat: false,
            window,
        });
        press(&mut app, window, KeyCode::Tab, Key::Tab);
        app.update();
        assert_eq!(app.world().get::<Input>(root).unwrap().value, "а\n\t");
        assert_eq!(
            app.world().resource::<InputFocus>().get(),
            Some(field),
            "Tab in a text area indents instead of navigating"
        );
        // In a single-line input Tab still navigates.
        app.world_mut()
            .resource_mut::<InputFocus>()
            .set(other_field, FocusCause::Navigated);
        app.update();
        press(&mut app, window, KeyCode::Tab, Key::Tab);
        app.update();
        assert_eq!(app.world().resource::<InputFocus>().get(), Some(field));
    }

    #[test]
    fn game_writes_apply_only_while_unfocused() {
        let (mut app, _) = app();
        let root = app.world_mut().spawn(Input::default()).id();
        let field = field_of(&mut app, root);
        app.world_mut().get_mut::<Input>(root).unwrap().value = "сервер".into();
        app.update();
        let text = |app: &App| {
            app.world()
                .get::<EditableText>(field)
                .unwrap()
                .value()
                .to_string()
        };
        assert_eq!(text(&app), "сервер");
        assert!(
            seen(&mut app).is_empty(),
            "game writes are not user changes"
        );

        app.world_mut()
            .resource_mut::<InputFocus>()
            .set(field, FocusCause::Navigated);
        app.world_mut().get_mut::<Input>(root).unwrap().value = "другое".into();
        app.update();
        assert_eq!(text(&app), "сервер");
    }

    #[test]
    fn number_typed_entry_commits_clamped_value_on_enter() {
        let (mut app, window) = app();
        let root = app
            .world_mut()
            .spawn(NumberInput {
                value: 5.0,
                max_value: 10.0,
                ..default()
            })
            .id();
        app.update();
        let field = app
            .world()
            .get::<super::super::number::NumberParts>(root)
            .unwrap()
            .field;
        super::super::number::begin_editing_for_test(app.world_mut(), root);
        app.world_mut()
            .resource_mut::<InputFocus>()
            .set(field, FocusCause::Pressed);
        app.update();
        type_text(&mut app, field, "42");
        press(&mut app, window, KeyCode::Enter, Key::Enter);
        app.update();
        assert_eq!(seen(&mut app), ["number:10"]);
        assert_eq!(app.world().get::<NumberInput>(root).unwrap().value, 10.0);
        assert_eq!(app.world().resource::<InputFocus>().get(), None);
    }
}
