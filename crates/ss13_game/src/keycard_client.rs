//! The keycard authentication device window (client): tgui `KeycardAuth.jsx`. The data is
//! `crate::keycard`, the server half `crate::security_level::keycard`.
// shortcut: the Authorize button keeps the plain button height (tgui sets `lineHeight="60px"`).

use bevy::scene::EntityScene;
use ss13_tgui::client::{AppTguiClientExt, ClientUi};
use ss13_ui_kit::prelude::*;

use crate::keycard::{
    AUTH_SWIPE, BSA_UNLOCK, EMERGENCY_MAINT, GIVE_JANITOR_ACCESS, KeycardAuthData, KeycardAuthUi,
    RED_ALERT,
};

pub struct KeycardClientPlugin;

impl Plugin for KeycardClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_view::<KeycardAuthUi>();
    }
}

impl ClientUi for KeycardAuthUi {
    const WIDTH: f32 = 375.0;
    const HEIGHT: f32 = 145.0;

    fn content(_: &(), data: &KeycardAuthData) -> impl Scene {
        view(data)
    }
}

fn button(icon: &str, content: &str, action: &str) -> impl Scene + use<> {
    let (icon, content, action) = (icon.to_owned(), content.to_owned(), action.to_owned());
    bsn! {
        @ss13_ui_kit::Button { @icon: icon, @content: content, @fluid: true }
        Act(action)
    }
}

fn view(data: &KeycardAuthData) -> impl Scene + use<> {
    let waiting = data.waiting.then(|| {
        EntityScene(bsn! {
            Node Children [ Text({"Waiting for another device to confirm your request...".to_owned()}) ]
        })
    });
    let authorize = (!data.waiting && data.auth_required.is_some()).then(|| {
        EntityScene(bsn! {
            @ss13_ui_kit::Button { @icon: "check-square", @content: "Authorize", @fluid: true }
            ss13_ui_kit::Button { color: UiColor::Bad }
            Act({AUTH_SWIPE.to_owned()})
        })
    });
    let events = (!data.waiting && data.auth_required.is_none()).then(|| {
        EntityScene(bsn! {
            Node { flex_direction: FlexDirection::Column }
            Children [
                {EntityScene(button("exclamation-triangle", "Red Alert", RED_ALERT))}
                --
                {EntityScene(button("wrench", "Emergency Maintenance Access", EMERGENCY_MAINT))}
                --
                {EntityScene(button("meteor", "Bluespace Artillery Unlock", BSA_UNLOCK))}
                --
                {EntityScene(button("key", "Grant Janitor Access", GIVE_JANITOR_ACCESS))}
            ]
        })
    });
    bsn! {
        @WindowContent
        Children [
            @Section
            Children [
                {waiting}
                --
                {authorize}
                --
                {events}
            ]
        ]
    }
}
