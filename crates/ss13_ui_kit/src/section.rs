//! tgui `Section`.

use bevy::app::Propagate;
use bevy::prelude::{
    AlignItems, App, BackgroundColor, BorderColor, Changed, ChildOf, Children, Color, Component,
    Entity, FlexDirection, IntoScheduleConfigs, Node, PositionType, PostUpdate, Query, UiRect, Val,
    With, percent, px,
};
use bevy::scene::{Scene, SceneComponent, SceneList, bsn};
use bevy::ui::UiSystems;
use bevy::ui::widget::Text;

use crate::button::TrailingMargin;
use crate::layout::{HasSlot, Part, Slot, block};
use crate::scroll::{scroll_children, scroll_row};
use crate::theme::{self, Bold, FontPx};

/// tgui `Section`. User children go into the padded content area.
#[derive(SceneComponent, Clone, Debug, Default)]
#[scene(section(SectionProps))]
pub struct Section;

#[derive(Default)]
pub struct SectionProps {
    pub title: String,
    /// Right side of the title bar: `@buttons: bsn! { @Button {...} }`.
    pub buttons: Option<Box<dyn SceneList>>,
    /// `fill`: take the full height of the parent.
    pub fill: bool,
    /// `fitted`: no content padding.
    pub fitted: bool,
    pub no_top_padding: bool,
    /// `scrollable`: the content scrolls vertically (with `fill`, inside the section's height).
    pub scrollable: bool,
    // TODO: `scrollableHorizontal`, `stretchContents`; no ported window needs them yet.
}

/// `.Section--fitted`: tabs and other edge-to-edge children skip their negative margins.
#[derive(Component, Clone, Debug, Default)]
pub struct SectionFitted;

#[derive(Component, Clone, Debug, Default)]
pub struct SectionTitle;

#[derive(Component, Clone, Debug, Default)]
pub struct SectionTitleText;

fn section(props: SectionProps) -> impl Scene {
    let has_title = !props.title.is_empty() || props.buttons.is_some();
    let content_padding = if props.fitted {
        UiRect::ZERO
    } else {
        UiRect {
            top: px(if props.no_top_padding {
                0.0
            } else {
                theme::SPACE_ML
            }),
            bottom: px(theme::SPACE_ML),
            left: px(theme::SPACE_M),
            right: px(theme::SPACE_M),
        }
    };
    let title = has_title.then(|| section_title(props.title, props.buttons));
    let grow = if props.fill { 1.0_f32 } else { 0.0_f32 };
    let content: Box<dyn Scene> = if props.scrollable {
        // `.Section--fill.Section--scrollable > ... > .Section__content { position: absolute }`:
        // a filled scrolling section never grows to its content.
        let basis = if props.fill { px(0) } else { Val::Auto };
        Box::new(bsn! {
            Children [
                @scroll_row()
                Node { flex_grow: grow, flex_basis: basis }
                Part
                @scroll_children(bsn! { @block() Node { padding: content_padding } Part Slot }, false)
            ]
        })
    } else {
        Box::new(bsn! {
            Children [
                @block()
                Node {
                    padding: content_padding,
                    flex_grow: grow,
                }
                Part
                Slot
            ]
        })
    };
    (
        block(),
        bsn! {
            Section
            HasSlot
            TrailingMargin(UiRect::bottom(px(theme::SPACE_M)))
            BackgroundColor(theme::COLOR_SECTION)
        },
        props
            .fill
            .then(|| bsn! { Node { height: percent(100), flex_grow: 1.0, min_height: px(0) } }),
        props.fitted.then_some(bsn! { SectionFitted }),
        title,
        content,
    )
}

fn section_title(title: String, buttons: Option<Box<dyn SceneList>>) -> impl Scene {
    let border = UiRect::bottom(px(theme::BORDER_SMALL));
    bsn! {
        Children [
            Node {
                padding: UiRect::all(px(theme::SPACE_M)),
                border: border,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
            }
            Part
            SectionTitle
            BorderColor::all(theme::COLOR_PRIMARY)
            Children [
                Node
                SectionTitleText
                Propagate::<FontPx>(FontPx(theme::FONT_SIZE_TITLE))
                Propagate::<Bold>(Bold(true))
                Children [ Text(title) ]
                --
                // `.Section__buttons`: absolutely positioned, so they never grow the title.
                Node {
                    position_type: PositionType::Absolute,
                    right: px(theme::SPACE_M),
                    top: px(theme::SPACE_M - theme::SPACE_XXS),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                }
                Children [ {buttons} ]
            ]
        ]
    }
}

/// `.Section .Section`: nested sections drop their background, pull out over the parent padding
/// and use smaller titles (13px, then 12px).
fn style_nested_sections(
    moved: Query<(Entity, &ChildOf), (With<Section>, Changed<ChildOf>)>,
    parents: Query<&ChildOf>,
    sections: Query<(), With<Section>>,
    siblings: Query<&Children>,
    titles: Query<&Children, With<SectionTitle>>,
    mut title_texts: Query<&mut Propagate<FontPx>, With<SectionTitleText>>,
    mut nodes: Query<(&mut Node, &mut BackgroundColor), With<Section>>,
) {
    for (section, child_of) in &moved {
        let depth = parents
            .iter_ancestors(section)
            .filter(|e| sections.contains(*e))
            .count();
        if depth == 0 {
            continue;
        }
        let first = siblings
            .get(child_of.parent())
            .is_ok_and(|c| c.first() == Some(&section));
        if let Ok((mut node, mut bg)) = nodes.get_mut(section) {
            let m = px(-theme::SPACE_M);
            node.margin.left = m;
            node.margin.right = m;
            if first {
                node.margin.top = m;
            }
            bg.0 = Color::NONE;
        }
        let size = if depth == 1 { 13.0 } else { theme::FONT_SIZE };
        let Ok(children) = siblings.get(section) else {
            continue;
        };
        for title in children.iter().filter_map(|c| titles.get(*c).ok()) {
            for &text in title.iter() {
                if let Ok(mut font) = title_texts.get_mut(text) {
                    font.0 = FontPx(size);
                }
            }
        }
    }
}

pub(crate) fn plugin(app: &mut App) {
    app.add_systems(
        PostUpdate,
        style_nested_sections.before(UiSystems::Propagate),
    );
}
