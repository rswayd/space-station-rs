//! Client half of the revolution: the `AntagInfoRevolution` panel, drawn like the generic one.

use ss13_tgui::client::{AppTguiClientExt, ClientUi};
use ss13_ui_kit::prelude::*;

use super::AntagInfoRevolutionUi;
use crate::antag::BrotherInfo;
use crate::antag::cult::AntagInfoGenericUi;

pub struct RevolutionClientPlugin;

impl Plugin for RevolutionClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_view::<AntagInfoRevolutionUi>();
    }
}

impl ClientUi for AntagInfoRevolutionUi {
    const WIDTH: f32 = AntagInfoGenericUi::WIDTH;
    const HEIGHT: f32 = AntagInfoGenericUi::HEIGHT;

    fn content(info: &BrotherInfo, data: &()) -> impl Scene {
        AntagInfoGenericUi::content(info, data)
    }
}
