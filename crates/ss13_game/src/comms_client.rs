//! The communications console window (client): `CommunicationsConsole/index.tsx` with the
//! authentication section, and `Main.tsx`'s emergency shuttle section with the call reason
//! `MessageModal` and its alert level section (`AlertButton.tsx`) with the "Swipe ID" modal.
//! The data is `crate::comms`, the server half `crate::shuttle`.
// shortcut: the other main page sections (functions, sectors) and pages (messages, status
// display, shuttle purchase) are not ported; the server refuses their acts.
// The recall button sends at once (tgui `Button.Confirm` asks a second click), and the call
// button's "longer reason" check runs on submit (the window does not re-render on typing).

use bevy::scene::EntityScene;
use bevy::ui_widgets::Activate;
use ss13_net::client::SendIntent;
use ss13_shared::protocol::ClientIntent;
use ss13_tgui::client::{
    AppTguiClientExt, ClientUi, ClientWindow, LocalOnInput, SetLocal, UiLocalState,
};
use ss13_ui_kit::prelude::*;

use crate::comms::{
    CALL_SHUTTLE, CHANGE_SECURITY_LEVEL, CommsConsoleData, CommsConsoleUi, NEW_SECURITY_LEVEL,
    REASON, RECALL_SHUTTLE, SWIPE_NEEDED, TOGGLE_AUTHENTICATION,
};

/// `useState` keys.
const CALLING: &str = "callingShuttle";
const INPUT: &str = "input";
const NEW_ALERT_LEVEL: &str = "newAlertLevel";

pub struct CommsClientPlugin;

impl Plugin for CommsClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_view::<CommsConsoleUi>()
            .add_observer(submit_reason);
    }
}

/// On the modal's submit button: `onSubmit(input)` -> `act(action, { [param]: input })` when
/// the input has at least `min_length` characters.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct SubmitLocal {
    pub action: String,
    pub param: String,
    pub min_length: u32,
}

fn submit_reason(
    activate: On<Activate>,
    submits: Query<&SubmitLocal>,
    parents: Query<&ChildOf>,
    mut windows: Query<(&ClientWindow, &mut UiLocalState)>,
    mut intents: MessageWriter<SendIntent>,
) {
    let Ok(submit) = submits.get(activate.entity) else {
        return;
    };
    let Some(root) = parents
        .iter_ancestors(activate.entity)
        .find(|e| windows.contains(*e))
    else {
        return;
    };
    let Ok((window, mut local)) = windows.get_mut(root) else {
        return;
    };
    let input = local.get(INPUT).to_owned();
    if (input.chars().count() as u32) < submit.min_length {
        return;
    }
    intents.write(SendIntent(ClientIntent::UiAct {
        window: window.id,
        action: submit.action.clone(),
        params: vec![(submit.param.clone(), input)],
    }));
    local.set(INPUT, "");
}

impl ClientUi for CommsConsoleUi {
    const WIDTH: f32 = 400.0;
    const HEIGHT: f32 = 650.0;

    fn content(_: &(), data: &CommsConsoleData) -> impl Scene {
        view(data, &UiLocalState::default())
    }

    fn content_local(_: &(), data: &CommsConsoleData, local: &UiLocalState) -> impl Scene {
        view(data, local)
    }
}

fn view(data: &CommsConsoleData, local: &UiLocalState) -> impl Scene + use<> {
    let (icon, color, text) = if data.authenticated {
        let name = data
            .authorize_name
            .as_ref()
            .map(|n| format!(" ({n})"))
            .unwrap_or_default();
        ("sign-out-alt", UiColor::Bad, format!("Log Out{name}"))
    } else {
        ("sign-in-alt", UiColor::Good, "Log In".to_owned())
    };
    let auth = (data.can_log_out || !data.authenticated).then(|| {
        EntityScene(bsn! {
            @Section { @title: "Authentication" }
            Children [
                @ss13_ui_kit::Button { @icon: icon, @content: text }
                ss13_ui_kit::Button { color: color }
                Act({TOGGLE_AUTHENTICATION.to_owned()})
            ]
        })
    });
    let calling = data.authenticated && !data.shuttle_called && local.get(CALLING) == "1";
    let main = data
        .authenticated
        .then(|| EntityScene(shuttle_section(data)));
    let can_set_alert = data.authenticated && data.can_set_alert_level.is_some();
    let alert = can_set_alert.then(|| EntityScene(alert_section(data)));
    let new_level = local.get(NEW_ALERT_LEVEL).to_owned();
    let swipe = (can_set_alert && !new_level.is_empty() && new_level != data.alert_level)
        .then(|| EntityScene(swipe_modal(new_level)));
    let modal = calling.then(|| EntityScene(reason_modal(data)));
    bsn! {
        @WindowContent
        Children [
            {auth}
            --
            {main}
            --
            {alert}
            --
            {swipe}
            --
            {modal}
        ]
    }
}

/// tgui `capitalize`.
fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// `Main.tsx` `<Section title="Alert Level">` with green and blue `AlertButton`s: the current
/// one does nothing, the other asks for the swipe (`SWIPE_NEEDED`) or sets the level at once.
fn alert_section(data: &CommsConsoleData) -> impl Scene + use<> {
    let current = format!("Currently on {} Alert", capitalize(&data.alert_level));
    let swipe = data.can_set_alert_level.as_deref() == Some(SWIPE_NEEDED);
    let button = |level: &str| {
        let is_current = data.alert_level == level;
        let color = if is_current {
            UiColor::Good
        } else {
            UiColor::Default
        };
        let content = capitalize(level);
        let level = level.to_owned();
        // Exactly one of the three is built.
        let current = is_current.then(|| {
            let content = content.clone();
            EntityScene(bsn! {
                @ss13_ui_kit::Button { @icon: "exclamation-triangle", @content: content }
                ss13_ui_kit::Button { color: color }
            })
        });
        let ask = (!is_current && swipe).then(|| {
            let (content, level) = (content.clone(), level.clone());
            EntityScene(bsn! {
                @ss13_ui_kit::Button { @icon: "exclamation-triangle", @content: content }
                SetLocal { key: {NEW_ALERT_LEVEL.to_owned()}, value: level, rerender: true }
            })
        });
        let set = (!is_current && !swipe).then(|| {
            EntityScene(bsn! {
                @ss13_ui_kit::Button { @icon: "exclamation-triangle", @content: content }
                Act({CHANGE_SECURITY_LEVEL.to_owned()})
                ActParams({vec![(NEW_SECURITY_LEVEL.to_owned(), level)]})
            })
        });
        EntityScene(bsn! {
            Node
            Children [
                {current}
                --
                {ask}
                --
                {set}
            ]
        })
    };
    let (green, blue) = (button("green"), button("blue"));
    bsn! {
        @Section { @title: "Alert Level" }
        Children [
            Node { justify_content: JustifyContent::SpaceBetween }
            Children [
                Node Children [ Text(current) ]
                --
                Node
                Children [
                    {green}
                    --
                    {blue}
                ]
            ]
        ]
    }
}

/// `Main.tsx`'s "Swipe ID to confirm change" modal.
fn swipe_modal(level: String) -> impl Scene + use<> {
    bsn! {
        @Modal
        Children [
            Node { flex_direction: FlexDirection::Column, width: px(300) }
            Children [
                Node { margin: UiRect::bottom(px(12)) }
                Children [ Text({"Swipe ID to confirm change".to_owned()}) ]
                --
                Node
                Children [
                    @ss13_ui_kit::Button { @icon: "id-card-o", @content: "Swipe ID" }
                    ss13_ui_kit::Button { color: UiColor::Good }
                    Act({CHANGE_SECURITY_LEVEL.to_owned()})
                    ActParams({vec![(NEW_SECURITY_LEVEL.to_owned(), level)]})
                    SetLocal { key: {NEW_ALERT_LEVEL.to_owned()}, value: {String::new()}, rerender: true }
                    --
                    @ss13_ui_kit::Button { @icon: "times", @content: "Cancel" }
                    ss13_ui_kit::Button { color: UiColor::Bad }
                    SetLocal { key: {NEW_ALERT_LEVEL.to_owned()}, value: {String::new()}, rerender: true }
                ]
            ]
        ]
    }
}

/// `Main.tsx` `<Section title="Emergency Shuttle">`.
fn shuttle_section(data: &CommsConsoleData) -> impl Scene + use<> {
    let recall = data.shuttle_called.then(|| {
        let tooltip = if data.can_recall_shuttles && !data.shuttle_recallable {
            "It's too late for the emergency shuttle to be recalled."
        } else {
            "You do not have permission to recall the emergency shuttle."
        };
        let disabled = !data.can_recall_shuttles || !data.shuttle_recallable;
        EntityScene(bsn! {
            @ss13_ui_kit::Button {
                @icon: "space-shuttle",
                @content: "Recall Emergency Shuttle",
                @tooltip: tooltip,
                @tooltip_position: Placement::Top,
            }
            ss13_ui_kit::Button { color: UiColor::Bad, disabled: disabled }
            Act({RECALL_SHUTTLE.to_owned()})
        })
    });
    let call = (!data.shuttle_called).then(|| {
        let reason = data.shuttle_can_evac_or_fail_reason.clone();
        let disabled = reason.is_some();
        EntityScene(bsn! {
            @ss13_ui_kit::Button {
                @icon: "space-shuttle",
                @content: "Call Emergency Shuttle",
                @tooltip: {reason.unwrap_or_default()},
                @tooltip_position: Placement::Top,
            }
            ss13_ui_kit::Button { disabled: disabled }
            SetLocal { key: {CALLING.to_owned()}, value: {"1".to_owned()}, rerender: true }
        })
    });
    let traced = data.shuttle_called_previously.then(|| {
        let text = match &data.shuttle_last_called {
            Some(area) => format!("Most recent shuttle call/recall traced to: {area}"),
            None => "Unable to trace most recent shuttle/recall signal.".to_owned(),
        };
        EntityScene(bsn! { Node Children [ Text(text) ] })
    });
    bsn! {
        @Section { @title: "Emergency Shuttle" }
        Children [
            {recall}
            --
            {call}
            --
            {traced}
        ]
    }
}

/// `MessageModal` for `callShuttle`.
fn reason_modal(data: &CommsConsoleData) -> impl Scene + use<> {
    let max = data.max_message_length as usize;
    let min = data.call_shuttle_reason_min_length;
    bsn! {
        @Modal
        Children [
            Node { flex_direction: FlexDirection::Column }
            Children [
                Node { margin: UiRect::bottom(px(6)) }
                Children [ Text({"Nature of emergency:".to_owned()}) ]
                --
                Node { width: px(300), margin: UiRect::bottom(px(6)) }
                Input {
                    placeholder: {"Nature of emergency".to_owned()},
                    max_length: {Some(max)},
                    fluid: true,
                    auto_focus: true,
                }
                ss13_ui_kit::TextArea { rows: 5.0 }
                LocalOnInput({INPUT.to_owned()})
                --
                Node
                Children [
                    @ss13_ui_kit::Button { @icon: "space-shuttle", @content: "Call Shuttle" }
                    ss13_ui_kit::Button { color: UiColor::Good }
                    SubmitLocal {
                        action: {CALL_SHUTTLE.to_owned()},
                        param: {REASON.to_owned()},
                        min_length: min,
                    }
                    SetLocal { key: {CALLING.to_owned()}, value: {String::new()}, rerender: true }
                    --
                    @ss13_ui_kit::Button { @icon: "times", @content: "Cancel" }
                    ss13_ui_kit::Button { color: UiColor::Bad }
                    SetLocal { key: {CALLING.to_owned()}, value: {String::new()}, rerender: true }
                ]
            ]
        ]
    }
}
