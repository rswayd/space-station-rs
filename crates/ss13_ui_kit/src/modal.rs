//! tgui `Dimmer` and `Modal`.
//!
//! The dimmer covers its parent node (tgui: the nearest positioned ancestor, usually
//! `Window.Content`) and, being an ordinary pickable node on top, takes every click and hover
//! meant for what it covers.

use bevy::prelude::{
    AlignItems, BackgroundColor, Children, Color, FlexDirection, JustifyContent, Node,
    PositionType, UiRect, ZIndex, px,
};
use bevy::scene::{Scene, SceneComponent, bsn};

use crate::layout::{HasSlot, Part, Slot, block};
use crate::theme;

/// `--dimmer-background`: base at 5% lightness, 0.75 alpha.
pub const DIMMER_BACKGROUND: Color = Color::hsla(0.0, 0.0, 0.05, 0.75);
/// `.Dimmer { z-index: 100 }`.
const DIMMER_Z: i32 = 100;

/// tgui `Dimmer`: children are centered over the dimmed area.
#[derive(SceneComponent, Clone, Debug, Default)]
#[scene(dimmer)]
pub struct Dimmer;

/// tgui `Modal`: a dimmer with a padded panel.
#[derive(SceneComponent, Clone, Debug, Default)]
#[scene(modal)]
pub struct Modal;

fn dimmer() -> impl Scene {
    bsn! {
        Node {
            position_type: PositionType::Absolute,
            left: px(0),
            right: px(0),
            top: px(0),
            bottom: px(0),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
        }
        ZIndex(DIMMER_Z)
        BackgroundColor(DIMMER_BACKGROUND)
    }
}

fn modal() -> impl Scene {
    bsn! {
        @dimmer()
        HasSlot
        Children [
            @block()
            Node {
                padding: UiRect::all(px(theme::SPACE_XL)),
                max_width: bevy::ui::Val::Percent(100.0),
                max_height: bevy::ui::Val::Percent(100.0),
            }
            BackgroundColor(theme::COLOR_BASE)
            Part
            Slot
        ]
    }
}
