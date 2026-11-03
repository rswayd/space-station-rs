//! Layout primitives: `Box` (named [`UiBox`]: `Box` would shadow `std::boxed::Box`), `Stack`,
//! `Flex`, `Divider`, plus the child-slot mechanism shared by every widget.
//!
//! Containers are "block" boxes: a column that stretches its children, which is what tgui's
//! `display: block` divs look like. Inline-level widgets (buttons, icons, inline boxes) inside a
//! block are gathered into wrapping rows, see [`wrap_inline_runs`].

use bevy::app::Propagate;
use bevy::prelude::{
    AlignItems, AlignSelf, App, BackgroundColor, BorderColor, Changed, Children, Commands,
    Component, Entity, FlexDirection, FlexWrap, Has, IntoScheduleConfigs, JustifyContent, Node, On,
    PostUpdate, Query, UiRect, Val, With, Without, percent, px,
};
use bevy::scene::{Ready, Scene, SceneComponent, bsn};
use bevy::text::{Justify, TextColor};
use bevy::ui::UiSystems;

use crate::theme::{self, Bold, FontPx, Italic, TextAlign, UiColor};

/// tgui `unit(n)`: numeric sizes (`width={2}`) in rem.
pub fn unit(n: f32) -> Val {
    px(n * theme::UNIT)
}

/// tgui `halfUnit(n)`: numeric spacing (`m={1}`, `mt={0.5}`) in half-rem.
pub fn half_unit(n: f32) -> Val {
    px(n * theme::HALF_UNIT)
}

/// An entity created by a widget's own scene (title bar, label cell, ...). Never moved into the
/// slot.
#[derive(Component, Default, Clone)]
pub struct Part;

/// The part that receives the children the user wrote under the widget (`Children [...]` after
/// `@Section {...}`), like `props.children` placement in JSX.
#[derive(Component, Default, Clone)]
pub struct Slot;

/// On a widget root whose user children belong in a [`Slot`] part.
#[derive(Component, Default, Clone)]
pub struct HasSlot;

/// Moves user children into the slot once the widget and all its children are spawned.
// shortcut: runs once on scene `Ready`; children added later must target the `Slot` entity
// directly. Upgrade if wave-2 re-rendering patches widgets in place instead of respawning.
fn fill_slots(
    ready: On<Ready>,
    roots: Query<&Children, With<HasSlot>>,
    parts: Query<Has<Slot>, With<Part>>,
    descendants: Query<&Children>,
    mut commands: Commands,
) {
    let Ok(children) = roots.get(ready.entity) else {
        return;
    };
    // The slot is a direct part, or nested inside one (a scrolling section's content).
    let is_slot = |e: &Entity| parts.get(*e) == Ok(true);
    let own_parts = || children.iter().copied().filter(|c| parts.contains(*c));
    let Some(slot) = own_parts()
        .find(is_slot)
        .or_else(|| own_parts().find_map(|part| descendants.iter_descendants(part).find(is_slot)))
    else {
        return;
    };
    let user: Vec<Entity> = children
        .iter()
        .copied()
        .filter(|c| !parts.contains(*c))
        .collect();
    if !user.is_empty() {
        commands.entity(slot).add_children(&user);
    }
}

/// The base node of a block container.
pub fn block() -> impl Scene {
    bsn! {
        Node {
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Stretch,
        }
        Block
    }
}

/// A CSS block container: runs of [`Inline`] children are wrapped into an [`InlineLine`].
#[derive(Component, Default, Clone)]
pub struct Block;

/// An inline-level box (`Button`, `Icon`, inline `Box`, `Dropdown`): flows left to right inside
/// a [`Block`] instead of stacking.
#[derive(Component, Default, Clone)]
pub struct Inline;

/// The anonymous box that holds a run of inline siblings, like the line boxes a browser builds
/// for inline content in a block: a wrapping row. Created by [`wrap_inline_runs`].
#[derive(Component, Default, Clone)]
pub struct InlineLine;

/// `bevy_ui` has no inline formatting context. A block's consecutive inline children are moved
/// into one wrapping flex row, which lays them out like CSS line boxes do for inline-blocks.
// shortcut: text nodes stay block-level (a `Text` between buttons breaks the line), since a
// flex row cannot wrap text across items the way inline text flows. Upgrade with rich-text
// paragraphs if a ported window needs text and buttons on one wrapping line.
fn wrap_inline_runs(
    blocks: Query<(Entity, &Children), (With<Block>, Without<HasSlot>, Changed<Children>)>,
    inline: Query<(), With<Inline>>,
    lines: Query<(), With<InlineLine>>,
    mut commands: Commands,
) {
    for (block, children) in &blocks {
        // (index of the first child of the run, run members, existing line before the run)
        let mut runs: Vec<(usize, Vec<Entity>, Option<Entity>)> = Vec::new();
        let mut prev: Option<Entity> = None;
        for (i, &child) in children.iter().enumerate() {
            if inline.contains(child) {
                match runs.last_mut() {
                    Some((start, members, _)) if *start + members.len() == i => {
                        members.push(child);
                    }
                    _ => runs.push((i, vec![child], prev.filter(|p| lines.contains(*p)))),
                }
            }
            prev = Some(child);
        }
        // Back to front, so earlier insertion indices stay valid.
        for (start, members, line) in runs.into_iter().rev() {
            let line = line.unwrap_or_else(|| {
                let line = commands
                    .spawn((
                        Node {
                            flex_direction: FlexDirection::Row,
                            flex_wrap: FlexWrap::Wrap,
                            align_items: AlignItems::Center,
                            ..Node::DEFAULT
                        },
                        InlineLine,
                    ))
                    .id();
                commands.entity(block).insert_children(start, &[line]);
                line
            });
            commands.entity(line).add_children(&members);
        }
    }
}

/// `text-align` of a block also places its inline boxes.
fn align_inline_lines(
    mut lines: Query<(&mut Node, &TextAlign), (With<InlineLine>, Changed<TextAlign>)>,
) {
    for (mut node, align) in &mut lines {
        let justify = match align.0 {
            Justify::Center => JustifyContent::Center,
            Justify::Right | Justify::End => JustifyContent::FlexEnd,
            _ => JustifyContent::FlexStart,
        };
        if node.justify_content != justify {
            node.justify_content = justify;
        }
    }
}

/// tgui `Box`: a block container with the common text props. Spacing/size props (`m`, `p`,
/// `width`, ...) are plain `Node` patches next to it: `@UiBox Node { margin: UiRect::top(half_unit(1.)) }`.
#[derive(SceneComponent, Default, Clone)]
#[scene(ui_box(UiBoxProps))]
pub struct UiBox;

#[derive(Default)]
pub struct UiBoxProps {
    /// Text color (atomic `color-<name>`, inherited by descendants).
    pub color: UiColor,
    /// Background color (atomic `color-bg-<name>`).
    pub background_color: UiColor,
    pub bold: bool,
    pub italic: bool,
    /// Font size in px; `0` inherits.
    pub font_size: f32,
    pub text_align: Option<Justify>,
    /// `inline`: hug content instead of stretching to the parent width.
    pub inline: bool,
}

fn ui_box(props: UiBoxProps) -> impl Scene {
    (
        block(),
        text_props(
            props.color,
            props.bold,
            props.italic,
            (props.font_size > 0.0).then_some(props.font_size),
            props.text_align,
        ),
        props
            .background_color
            .background()
            .map(|c| bsn! { BackgroundColor(c) }),
        props
            .inline
            .then_some(bsn! { Node { align_self: AlignSelf::FlexStart } Inline }),
    )
}

/// Inherited text style overrides shared by `Box`-like widgets.
pub(crate) fn text_props(
    color: UiColor,
    bold: bool,
    italic: bool,
    font_size: Option<f32>,
    text_align: Option<Justify>,
) -> impl Scene {
    (
        color
            .text()
            .map(|c| bsn! { Propagate::<TextColor>(TextColor(c)) }),
        bold.then_some(bsn! { Propagate::<Bold>(Bold(true)) }),
        italic.then_some(bsn! { Propagate::<Italic>(Italic(true)) }),
        font_size.map(|s| bsn! { Propagate::<FontPx>(FontPx(s)) }),
        text_align.map(|a| bsn! { Propagate::<TextAlign>(TextAlign(a)) }),
    )
}

/// tgui `Stack`: a flex container with a 0.5em gap.
#[derive(SceneComponent, Default, Clone)]
#[scene(stack(StackProps))]
pub struct Stack;

#[derive(Default)]
pub struct StackProps {
    pub vertical: bool,
    /// `fill`: take the full height of the parent.
    pub fill: bool,
    pub reverse: bool,
    /// Candystripe odd items.
    pub zebra: bool,
    pub align: Option<AlignItems>,
    pub justify: Option<JustifyContent>,
    pub wrap: bool,
}

/// Marks a zebra stack; its odd items get the candystripe background.
#[derive(Component, Default, Clone)]
pub struct Zebra;

fn stack(props: StackProps) -> impl Scene {
    let direction = match (props.vertical, props.reverse) {
        (false, false) => FlexDirection::Row,
        (false, true) => FlexDirection::RowReverse,
        (true, false) => FlexDirection::Column,
        (true, true) => FlexDirection::ColumnReverse,
    };
    let gap = px(theme::SPACE_M);
    (
        bsn! {
            Node {
                flex_direction: direction,
                row_gap: gap,
                column_gap: gap,
                align_items: {props.align.unwrap_or(AlignItems::Stretch)},
                justify_content: {props.justify.unwrap_or_default()},
                flex_wrap: {if props.wrap { FlexWrap::Wrap } else { FlexWrap::NoWrap }},
            }
        },
        props
            .fill
            .then(|| bsn! { Node { height: percent(100), flex_grow: 1.0 } }),
        props.zebra.then_some(bsn! { Zebra }),
    )
}

fn stripe_zebra(ready: On<Ready>, stacks: Query<&Children, With<Zebra>>, mut commands: Commands) {
    let Ok(children) = stacks.get(ready.entity) else {
        return;
    };
    // `:nth-child(odd)` is 1-based.
    for item in children.iter().copied().step_by(2) {
        commands
            .entity(item)
            .insert(BackgroundColor(theme::CANDYSTRIPE_ODD));
    }
}

/// `Stack.Item` / `Flex.Item` props.
#[derive(Default)]
pub struct FlexItemProps {
    /// `grow`: flex-grow; `true` in tgui is `1.0`.
    pub grow: f32,
    /// Defaults to 1, as in CSS.
    pub shrink: Option<f32>,
    pub basis: Option<Val>,
    pub align: Option<AlignSelf>,
}

fn flex_item(props: FlexItemProps) -> impl Scene {
    // `computeFlexItemProps`: a grown item without an explicit basis gets `flex-basis: 0`.
    let basis = props
        .basis
        .unwrap_or(if props.grow > 0.0 { px(0) } else { Val::Auto });
    // A grown item takes the height left, never its content's: tgui's scrolling `Section--fill`
    // positions its content absolutely, so it adds nothing to the item's `min-height: auto`;
    // here the content counts, and without a zero minimum the item stays as tall as the list
    // inside, which then never scrolls and is cut off by the window.
    let min_height = if props.grow > 0.0 { px(0) } else { Val::Auto };
    (
        block(),
        bsn! {
            Node {
                flex_grow: {props.grow},
                flex_shrink: {props.shrink.unwrap_or(1.0)},
                flex_basis: basis,
                align_self: {props.align.unwrap_or_default()},
                min_height: min_height,
            }
        },
    )
}

/// tgui `Stack.Item`.
#[derive(SceneComponent, Default, Clone)]
#[scene(flex_item(FlexItemProps))]
pub struct StackItem;

/// tgui `Stack.Divider`: an item with a 2px divider line across the stack direction.
#[derive(SceneComponent, Default, Clone)]
#[scene(stack_divider(StackDividerProps))]
pub struct StackDivider;

#[derive(Default)]
pub struct StackDividerProps {
    /// Must match the parent stack: draws a horizontal line in a vertical stack.
    pub vertical: bool,
    pub hidden: bool,
}

fn stack_divider(props: StackDividerProps) -> impl Scene {
    let border = match (props.hidden, props.vertical) {
        (true, _) => UiRect::ZERO,
        (false, true) => UiRect::top(px(theme::BORDER_SMALL)),
        (false, false) => UiRect::left(px(theme::BORDER_SMALL)),
    };
    bsn! {
        Node { border: border }
        BorderColor::all(theme::COLOR_BORDER)
    }
}

/// tgui `Flex`.
#[derive(SceneComponent, Default, Clone)]
#[scene(flex(FlexProps))]
pub struct Flex;

#[derive(Default)]
pub struct FlexProps {
    pub direction: FlexDirection,
    pub align: Option<AlignItems>,
    pub justify: Option<JustifyContent>,
    pub wrap: bool,
}

fn flex(props: FlexProps) -> impl Scene {
    bsn! {
        Node {
            flex_direction: {props.direction},
            align_items: {props.align.unwrap_or_default()},
            justify_content: {props.justify.unwrap_or_default()},
            flex_wrap: {if props.wrap { FlexWrap::Wrap } else { FlexWrap::NoWrap }},
        }
    }
}

/// tgui `Flex.Item`.
#[derive(SceneComponent, Default, Clone)]
#[scene(flex_item(FlexItemProps))]
pub struct FlexItem;

/// tgui `Divider`.
#[derive(SceneComponent, Default, Clone)]
#[scene(divider(DividerProps))]
pub struct Divider;

#[derive(Default)]
pub struct DividerProps {
    pub vertical: bool,
    pub hidden: bool,
}

fn divider(props: DividerProps) -> impl Scene {
    let line = if props.hidden {
        0.0
    } else {
        theme::BORDER_SMALL
    };
    let m = px(theme::SPACE_M);
    let node = if props.vertical {
        Node {
            height: percent(100),
            margin: UiRect::horizontal(m),
            border: UiRect::left(px(line)),
            ..Node::DEFAULT
        }
    } else {
        Node {
            margin: UiRect::vertical(m),
            border: UiRect::top(px(line)),
            ..Node::DEFAULT
        }
    };
    bsn! {
        node
        BorderColor::all(theme::COLOR_BORDER)
    }
}

pub(crate) fn plugin(app: &mut App) {
    app.add_observer(fill_slots)
        .add_observer(stripe_zebra)
        .add_systems(
            PostUpdate,
            (
                wrap_inline_runs.before(UiSystems::Propagate),
                align_inline_lines
                    .after(UiSystems::Propagate)
                    .before(UiSystems::Layout),
            ),
        );
}
