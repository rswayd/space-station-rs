//! tgui `Dropdown`: a button-like control that opens a menu of options above every window.
//!
//! Controlled like tgui: picking an option triggers [`DropdownSelected`] (and `act()` when the
//! dropdown has an [`Act`]) but does not change `selected`; the owner sets it from new data.

use bevy::app::Propagate;
use bevy::ecs::change_detection::DetectChangesMut;
use bevy::ecs::lifecycle::Remove;
use bevy::input::ButtonState;
use bevy::input::keyboard::{KeyCode, KeyboardInput};
use bevy::input_focus::FocusedInput;
use bevy::input_focus::tab_navigation::TabIndex;
use bevy::math::Rot2;
use bevy::picking::hover::Hovered;
use bevy::picking::pointer::{PointerAction, PointerInput};
use bevy::prelude::{
    Added, AlignItems, App, BackgroundColor, BorderColor, BorderRadius, Changed, ChildOf, Children,
    Color, Commands, Component, Entity, EntityEvent, FlexDirection, GlobalZIndex, Has,
    IntoScheduleConfigs, JustifyContent, MessageReader, Node, On, Or, Overflow, PositionType,
    PostUpdate, Query, UiRect, Update, Val, With, px,
};
use bevy::scene::{CommandsSceneExt, Scene, SceneComponent, bsn};
use bevy::text::TextColor;
use bevy::ui::widget::Text;
use bevy::ui::{InteractionDisabled, Pressed, UiSystems, UiTransform};
use bevy::ui_widgets::Activate;

use crate::button::{self as button_mod, Act, ActParams, ActScope, Button, TrailingMargin};
use crate::icon::Icon;
use crate::layout::Inline;
use crate::overlay::{Anchored, MENU_Z, Placement};
use crate::scroll::{scroll_children, scroll_row};
use crate::theme::{self, UiColor, root_text_style};

/// tgui `Dropdown`. Fields are live state: change them and the control follows.
#[derive(SceneComponent, Clone, Debug, Default, PartialEq)]
#[scene(dropdown(DropdownProps))]
pub struct Dropdown {
    pub options: Vec<String>,
    pub selected: String,
    /// `displayText`: shown instead of `selected`.
    pub display_text: String,
    pub disabled: bool,
}

pub struct DropdownProps {
    /// Shown when nothing is selected.
    pub placeholder: String,
    /// Control width in units (rem); tgui's default is 15.
    pub width: f32,
    /// Menu width in units; `0` matches the control.
    pub menu_width: f32,
    pub fluid: bool,
    /// `over`: the menu opens above the control.
    pub over: bool,
    pub icon: String,
    pub no_chevron: bool,
    pub color: UiColor,
    /// `buttons`: previous/next arrows beside the control.
    pub buttons: bool,
    /// Name of the `act()` param that carries the chosen option.
    pub value_param: String,
}

impl Default for DropdownProps {
    fn default() -> Self {
        Self {
            placeholder: "Select...".into(),
            width: 15.0,
            menu_width: 0.0,
            fluid: false,
            over: false,
            icon: String::new(),
            no_chevron: false,
            color: UiColor::Default,
            buttons: false,
            value_param: "value".into(),
        }
    }
}

/// An option was picked (click, Enter, or the arrow buttons). Targets the dropdown root.
#[derive(EntityEvent, Clone, Debug, PartialEq)]
pub struct DropdownSelected {
    #[event_target]
    pub entity: Entity,
    pub value: String,
}

/// Structure and open state of a dropdown, on its root.
#[derive(Component, Clone, Debug, Default)]
pub struct DropdownState {
    pub placeholder: String,
    pub over: bool,
    pub menu_width: f32,
    pub color: UiColor,
    pub value_param: String,
    /// The open menu overlay.
    pub menu: Option<Entity>,
    /// Option index highlighted with the arrow keys while the menu is open.
    pub highlight: Option<usize>,
}

/// The clickable box showing the selection.
#[derive(Component, Clone, Debug, Default)]
pub struct DropdownControl;

#[derive(Component, Clone, Debug, Default)]
pub struct DropdownSelectedText;

#[derive(Component, Clone, Debug, Default)]
pub struct DropdownChevron;

/// A previous (-1) / next (+1) arrow button.
#[derive(Component, Clone, Debug, Default)]
pub struct DropdownStep(pub i32);

/// The menu overlay root; `owner` is the dropdown root.
#[derive(Component, Clone, Debug)]
pub struct DropdownMenu {
    pub owner: Entity,
}

impl Default for DropdownMenu {
    fn default() -> Self {
        Self {
            owner: Entity::PLACEHOLDER,
        }
    }
}

/// One option row in an open menu.
#[derive(Component, Clone, Debug)]
pub struct DropdownEntry {
    pub owner: Entity,
    pub index: usize,
}

impl Default for DropdownEntry {
    fn default() -> Self {
        Self {
            owner: Entity::PLACEHOLDER,
            index: 0,
        }
    }
}

/// `--dropdown-menu-background` (base - 5.5 lightness) without tgui-core's 0.85 alpha: drawn
/// over sharp `bevy_ui` content, the translucent menu let the text below show through its options.
const MENU_BACKGROUND: Color = Color::hsl(0.0, 0.0, 0.095);
/// `--dropdown-menu-max-height`.
const MENU_MAX_HEIGHT: f32 = 200.0;
/// `.Dropdown__control { height: em(22px) }`.
const CONTROL_HEIGHT: f32 = 22.0;

fn dropdown(props: DropdownProps) -> impl Scene {
    let state = DropdownState {
        placeholder: props.placeholder,
        over: props.over,
        menu_width: props.menu_width,
        color: props.color,
        value_param: props.value_param,
        menu: None,
        highlight: None,
    };
    let width = if props.fluid {
        Val::Auto
    } else {
        px(props.width * theme::UNIT)
    };
    let grow = if props.fluid { 1.0_f32 } else { 0.0_f32 };
    let icon = (!props.icon.is_empty()).then(|| {
        let icon = props.icon;
        bsn! {
            Children [
                Node { width: px(CONTROL_HEIGHT), justify_content: JustifyContent::Center }
                Children [ @Icon { @name: icon } ]
            ]
        }
    });
    let chevron = (!props.no_chevron).then(|| {
        bsn! {
            Children [
                Node { width: px(CONTROL_HEIGHT), justify_content: JustifyContent::Center }
                Children [ @Icon { @name: "chevron-down" } DropdownChevron ]
            ]
        }
    });
    let left_pad = if icon.is_some() { 0.0 } else { theme::SPACE_M };
    let steps = props.buttons.then(|| {
        bsn! {
            Children [
                @Button { @icon: "chevron-left" } DropdownStep(-1)
                --
                @Button { @icon: "chevron-right" } DropdownStep(1)
            ]
        }
    });
    (
        bsn! {
            Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Stretch,
                column_gap: px(theme::SPACE_S),
                flex_grow: grow,
            }
            state
            TrailingMargin(UiRect { right: px(theme::SPACE_XS), bottom: px(theme::SPACE_XS), ..UiRect::DEFAULT })
            Children [
                Node {
                    width: width,
                    flex_grow: 1.0,
                    height: px(CONTROL_HEIGHT),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    overflow: Overflow::clip(),
                    border_radius: BorderRadius::all(px(theme::RADIUS_TINY)),
                }
                DropdownControl
                bevy::ui_widgets::Button
                Hovered
                TabIndex(0)
                BackgroundColor(Color::NONE)
                Propagate::<TextColor>(TextColor(theme::COLOR_TEXT))
                @{icon}
                Children [
                    // `.Dropdown__selected-text`.
                    Node {
                        flex_grow: 1.0,
                        min_width: px(0),
                        height: bevy::ui::Val::Percent(100.0),
                        padding: UiRect { left: px(left_pad), right: px(theme::SPACE_M), top: px(theme::SPACE_S), bottom: px(theme::SPACE_S) },
                        border: UiRect::right(px(theme::BORDER_TINY)),
                        overflow: Overflow::clip(),
                        flex_direction: FlexDirection::Column,
                        justify_content: JustifyContent::Center,
                    }
                    BorderColor::all(theme::COLOR_BORDER_DARK)
                    Children [ Text("") DropdownSelectedText ]
                ]
                @{chevron}
            ]
            @{steps}
        },
        (!props.fluid).then_some(bsn! { Inline }),
    )
}

/// Control colors (`button-color` mixin) and the shown text.
fn style_controls(
    roots: Query<(&Dropdown, &DropdownState, &Children), Changed<Dropdown>>,
    controls: Query<
        (Entity, &ChildOf, &Hovered, Has<Pressed>),
        (
            With<DropdownControl>,
            Or<(Changed<Hovered>, Added<Pressed>, Added<DropdownControl>)>,
        ),
    >,
    all_roots: Query<(&Dropdown, &DropdownState)>,
    descendants: Query<&Children>,
    mut texts: Query<&mut Text, With<DropdownSelectedText>>,
    mut colors: Query<(&mut BackgroundColor, &mut Propagate<TextColor>), With<DropdownControl>>,
    mut commands: Commands,
) {
    for (dropdown, state, children) in &roots {
        let shown = if !dropdown.display_text.is_empty() {
            &dropdown.display_text
        } else if !dropdown.selected.is_empty() {
            &dropdown.selected
        } else {
            &state.placeholder
        };
        for &child in children {
            for e in descendants.iter_descendants(child) {
                if let Ok(mut text) = texts.get_mut(e)
                    && text.0 != *shown
                {
                    text.0 = shown.clone();
                }
            }
            if colors.contains(child) {
                if dropdown.disabled {
                    commands.entity(child).insert(InteractionDisabled);
                } else {
                    commands.entity(child).remove::<InteractionDisabled>();
                }
                restyle(child, dropdown, state, false, false, &mut colors);
            }
        }
    }
    for (control, parent, hovered, pressed) in &controls {
        if let Ok((dropdown, state)) = all_roots.get(parent.parent()) {
            restyle(
                control,
                dropdown,
                state,
                hovered.get(),
                pressed,
                &mut colors,
            );
        }
    }
}

fn restyle(
    control: Entity,
    dropdown: &Dropdown,
    state: &DropdownState,
    hovered: bool,
    pressed: bool,
    colors: &mut Query<(&mut BackgroundColor, &mut Propagate<TextColor>), With<DropdownControl>>,
) {
    let look = Button {
        color: state.color,
        selected: false,
        disabled: dropdown.disabled,
    };
    let (bg, text) = button_mod::button_colors(&look, hovered, pressed);
    if let Ok((mut b, mut t)) = colors.get_mut(control) {
        b.set_if_neq(BackgroundColor(bg));
        t.set_if_neq(Propagate(TextColor(text)));
    }
}

fn root_of(control: Entity, parents: &Query<&ChildOf>) -> Option<Entity> {
    parents.get(control).ok().map(ChildOf::parent)
}

fn open_menu(
    root: Entity,
    control: Entity,
    dropdown: &Dropdown,
    state: &mut DropdownState,
    commands: &mut Commands,
) {
    let placement = if state.over {
        Placement::TopStart
    } else {
        Placement::BottomStart
    };
    let match_width = state.menu_width <= 0.0;
    let width = if match_width {
        Val::Auto
    } else {
        px(state.menu_width * theme::UNIT)
    };
    let anchored = Anchored {
        anchor: control,
        placement,
        offset: theme::SPACE_M,
        match_width,
    };
    let entries: Vec<_> = if dropdown.options.is_empty() {
        vec![Box::new(bsn! { @entry_node() Children [ Text("No options") ] }) as Box<dyn Scene>]
    } else {
        dropdown
            .options
            .iter()
            .enumerate()
            .map(|(index, option)| {
                let text = option.clone();
                let selected = *option == dropdown.selected;
                let bg = if selected {
                    theme::COLOR_SELECTED
                } else {
                    Color::NONE
                };
                Box::new(bsn! {
                    @entry_node()
                    DropdownEntry { owner: root, index: index }
                    bevy::ui_widgets::Button
                    Hovered
                    BackgroundColor(bg)
                    Children [ Text(text) ]
                }) as Box<dyn Scene>
            })
            .collect()
    };
    let menu = commands
        .spawn_scene(bsn! {
            Node {
                position_type: PositionType::Absolute,
                width: width,
                flex_direction: FlexDirection::Column,
                border: UiRect::all(px(theme::BORDER_TINY)),
                border_radius: BorderRadius::all(px(theme::RADIUS_LARGE)),
                overflow: Overflow::clip(),
            }
            DropdownMenu { owner: root }
            anchored
            GlobalZIndex(MENU_Z)
            Hovered
            BackgroundColor(MENU_BACKGROUND)
            BorderColor::all(theme::COLOR_BORDER)
            @root_text_style()
            Children [
                @scroll_row()
                Node { max_height: px(MENU_MAX_HEIGHT) }
                @scroll_children(bsn! {
                    Node { padding: UiRect::all(px(theme::SPACE_SM)) }
                    Children [ {entries} ]
                }, false)
            ]
        })
        .id();
    state.menu = Some(menu);
    state.highlight = None;
}

fn entry_node() -> impl Scene {
    bsn! {
        Node {
            padding: UiRect::axes(px(theme::SPACE_M), px(theme::SPACE_XS)),
            min_height: px(16.0 + 2.0 * theme::SPACE_XS),
            border_radius: BorderRadius::all(px(theme::RADIUS_SMALL)),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            overflow: Overflow::clip(),
        }
    }
}

fn close_menu(state: &mut DropdownState, commands: &mut Commands) {
    if let Some(menu) = state.menu.take() {
        commands.entity(menu).try_despawn();
    }
    state.highlight = None;
}

/// Sends the selection of option `index` and closes the menu.
fn select(
    root: Entity,
    index: usize,
    dropdown: &Dropdown,
    state: &mut DropdownState,
    act: Option<(&Act, Option<&ActParams>)>,
    parents: &Query<&ChildOf>,
    scopes: &Query<(), With<ActScope>>,
    commands: &mut Commands,
) {
    close_menu(state, commands);
    let Some(value) = dropdown.options.get(index).cloned() else {
        return;
    };
    if let Some((act, params)) = act {
        let mut params = params.map(|p| p.0.clone()).unwrap_or_default();
        params.push((state.value_param.clone(), value.clone()));
        commands.trigger(button_mod::act_event(root, act, params, parents, scopes));
    }
    commands.trigger(DropdownSelected {
        entity: root,
        value,
    });
}

fn on_activate(
    activate: On<Activate>,
    controls: Query<(), With<DropdownControl>>,
    entries: Query<&DropdownEntry>,
    steps: Query<&DropdownStep>,
    parents: Query<&ChildOf>,
    scopes: Query<(), With<ActScope>>,
    mut roots: Query<(
        &Dropdown,
        &mut DropdownState,
        Option<&Act>,
        Option<&ActParams>,
    )>,
    mut commands: Commands,
) {
    let target = activate.entity;
    let (root, pick) = if controls.contains(target) {
        let Some(root) = root_of(target, &parents) else {
            return;
        };
        (root, None)
    } else if let Ok(entry) = entries.get(target) {
        (entry.owner, Some(entry.index))
    } else if let Ok(step) = steps.get(target) {
        let Some(root) = root_of(target, &parents) else {
            return;
        };
        let Ok((dropdown, ..)) = roots.get(root) else {
            return;
        };
        let len = dropdown.options.len();
        if len == 0 || dropdown.disabled {
            return;
        }
        let current = dropdown
            .options
            .iter()
            .position(|o| *o == dropdown.selected);
        let next = match (current, step.0 > 0) {
            (None, true) => len - 1,
            (None, false) => 0,
            (Some(i), true) => (i + 1) % len,
            (Some(i), false) => (i + len - 1) % len,
        };
        (root, Some(next))
    } else {
        return;
    };
    let Ok((dropdown, mut state, act, params)) = roots.get_mut(root) else {
        return;
    };
    // Enter on the control while an option is highlighted picks it.
    let pick = pick.or(state.menu.and(state.highlight));
    if let Some(index) = pick {
        if dropdown.disabled {
            return;
        }
        let act = act.map(|a| (a, params));
        select(
            root,
            index,
            dropdown,
            &mut state,
            act,
            &parents,
            &scopes,
            &mut commands,
        );
    } else if state.menu.is_some() {
        close_menu(&mut state, &mut commands);
    } else if !dropdown.disabled {
        open_menu(root, target, dropdown, &mut state, &mut commands);
    }
}

/// Arrow keys open the menu and move the highlight; Escape closes it.
fn on_key(
    mut key: On<FocusedInput<KeyboardInput>>,
    controls: Query<(), With<DropdownControl>>,
    parents: Query<&ChildOf>,
    mut roots: Query<(&Dropdown, &mut DropdownState)>,
    mut commands: Commands,
) {
    let control = key.focused_entity;
    if !controls.contains(control) || key.input.state != ButtonState::Pressed {
        return;
    }
    let Some(root) = root_of(control, &parents) else {
        return;
    };
    let Ok((dropdown, mut state)) = roots.get_mut(root) else {
        return;
    };
    let step = match key.input.key_code {
        KeyCode::ArrowDown => 1,
        KeyCode::ArrowUp => -1,
        KeyCode::Escape if state.menu.is_some() => {
            key.propagate(false);
            close_menu(&mut state, &mut commands);
            return;
        }
        _ => return,
    };
    key.propagate(false);
    let len = dropdown.options.len();
    if dropdown.disabled || len == 0 {
        return;
    }
    if state.menu.is_none() {
        open_menu(root, control, dropdown, &mut state, &mut commands);
    }
    let from = state.highlight.or_else(|| {
        dropdown
            .options
            .iter()
            .position(|o| *o == dropdown.selected)
    });
    state.highlight = Some(match from {
        None if step > 0 => 0,
        None => len - 1,
        Some(i) => (i as i32 + step).rem_euclid(len as i32) as usize,
    });
}

/// floating-ui `useDismiss`: a press anywhere outside the menu and its control closes it.
// Reads raw pointer input rather than picking events: a press on empty space (the game view)
// hits no UI entity, so no entity event would report it.
fn close_on_outside_press(
    mut input: MessageReader<PointerInput>,
    menus: Query<(&DropdownMenu, &Hovered, &Anchored)>,
    hovered: Query<&Hovered>,
    mut roots: Query<&mut DropdownState>,
    mut commands: Commands,
) {
    let pressed = input
        .read()
        .any(|i| matches!(i.action, PointerAction::Press(_)));
    if !pressed {
        return;
    }
    for (menu, menu_hovered, anchored) in &menus {
        let on_control = hovered.get(anchored.anchor).is_ok_and(Hovered::get);
        if menu_hovered.get() || on_control {
            continue;
        }
        if let Ok(mut state) = roots.get_mut(menu.owner) {
            close_menu(&mut state, &mut commands);
        }
    }
}

/// Entry hover/highlight backgrounds and the chevron flip.
fn style_menus(
    roots: Query<(&Dropdown, &DropdownState, &Children), Changed<DropdownState>>,
    descendants: Query<&Children>,
    mut chevrons: Query<&mut UiTransform, With<DropdownChevron>>,
    mut entries: Query<(&DropdownEntry, &Hovered, &mut BackgroundColor)>,
    changed_entries: Query<(), (With<DropdownEntry>, Changed<Hovered>)>,
    states: Query<(&Dropdown, &DropdownState)>,
) {
    for (_, state, children) in &roots {
        let flipped = state.menu.is_some() != state.over;
        let rotation = if flipped {
            Rot2::degrees(180.0)
        } else {
            Rot2::IDENTITY
        };
        for &child in children {
            for e in descendants.iter_descendants(child) {
                if let Ok(mut t) = chevrons.get_mut(e)
                    && t.rotation != rotation
                {
                    t.rotation = rotation;
                }
            }
        }
    }
    let any_root_changed = !roots.is_empty();
    if !any_root_changed && changed_entries.is_empty() {
        return;
    }
    for (entry, hovered, mut bg) in &mut entries {
        let Ok((dropdown, state)) = states.get(entry.owner) else {
            continue;
        };
        let selected = dropdown.options.get(entry.index) == Some(&dropdown.selected);
        let color = if selected {
            theme::COLOR_SELECTED
        } else if hovered.get() || state.highlight == Some(entry.index) {
            theme::COLOR_HOVER
        } else {
            Color::NONE
        };
        bg.set_if_neq(BackgroundColor(color));
    }
}

/// A despawned dropdown takes its menu overlay with it: the menu is not its child.
fn despawn_orphan_menu(
    remove: On<Remove<DropdownState>>,
    roots: Query<&DropdownState>,
    mut commands: Commands,
) {
    if let Ok(state) = roots.get(remove.entity)
        && let Some(menu) = state.menu
    {
        commands.entity(menu).try_despawn();
    }
}

pub(crate) fn plugin(app: &mut App) {
    // Idempotent; lets the kit run in apps without the picking input plugins (headless tests).
    app.add_message::<PointerInput>()
        .add_observer(on_activate)
        .add_observer(on_key)
        .add_observer(despawn_orphan_menu)
        .add_systems(Update, close_on_outside_press)
        .add_systems(
            PostUpdate,
            (style_controls, style_menus).before(UiSystems::Layout),
        );
}
