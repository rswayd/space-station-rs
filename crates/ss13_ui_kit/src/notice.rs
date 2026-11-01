//! tgui `NoticeBox`.

use bevy::app::Propagate;
use bevy::prelude::{BackgroundColor, UiRect, px};
use bevy::scene::{Scene, SceneComponent, bsn};
use bevy::text::TextColor;

use crate::button::TrailingMargin;
use crate::layout::block;
use crate::theme::{self, Bold, Italic, UiColor, notice_background};

/// tgui `NoticeBox`. At most one of `info`/`success`/`warning`/`danger` is meant to be set;
/// `color` overrides them all, as the CSS order does.
// TODO(wave 2): the diagonal stripes (`repeating-linear-gradient`) have no `bevy_ui` equivalent.
#[derive(SceneComponent, Clone, Debug, Default)]
#[scene(notice_box(NoticeBoxProps))]
pub struct NoticeBox;

#[derive(Default)]
pub struct NoticeBoxProps {
    pub color: UiColor,
    pub info: bool,
    pub success: bool,
    pub warning: bool,
    pub danger: bool,
}

/// Background and text color of a notice box.
pub fn notice_colors(props: &NoticeBoxProps) -> (bevy::color::Color, bevy::color::Color) {
    let base = if props.color != UiColor::Default {
        props.color.base()
    } else if props.danger {
        UiColor::Red.base()
    } else if props.warning {
        UiColor::Orange.base()
    } else if props.success {
        UiColor::Green.base()
    } else if props.info {
        UiColor::Blue.base()
    } else {
        theme::NOTICE_BOX_BACKGROUND
    };
    let text = if props.color.is_low_contrast() {
        UiColor::Black.base()
    } else {
        theme::COLOR_TEXT
    };
    (notice_background(base), text)
}

fn notice_box(props: NoticeBoxProps) -> impl Scene {
    let (bg, text) = notice_colors(&props);
    let padding = UiRect::axes(px(theme::SPACE_M), px(theme::SPACE_SM));
    (
        block(),
        bsn! {
            bevy::prelude::Node { padding: padding }
            BackgroundColor(bg)
            TrailingMargin(UiRect::bottom(px(theme::SPACE_M)))
            Propagate::<TextColor>(TextColor(text))
            Propagate::<Bold>(Bold(true))
            Propagate::<Italic>(Italic(true))
        },
    )
}
