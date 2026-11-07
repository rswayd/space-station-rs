//! The communications console window (client): `CommunicationsConsole/index.tsx` with the
//! authentication section and its pages: `Main.tsx` (emergency shuttle, alert level with the
//! "Swipe ID" modal, functions, the `MessageModal`s), `Messages.tsx` and `ChangingStatus.tsx`
//! (`common/StatusDisplayControls.tsx`). The data is `crate::comms`, the server half
//! `crate::comms_server`. `BuyingShuttle.tsx` lists the emergency shuttle catalog.
// shortcut: no allied sectors (the server has none), no `syndicate` theme, no coin icon or
// colours on the shuttle cards, no emag-only or department-locked cards (the catalog has none). `Button.Confirm`s (recall, emergency access, delete) send at once, and a
// modal's "longer reason" check runs on submit (the window does not re-render on typing).

use bevy::scene::{EntityScene, SceneList};
use bevy::ui_widgets::Activate;
use ss13_net::client::SendIntent;
use ss13_shared::protocol::ClientIntent;
use ss13_tgui::client::{
    AppTguiClientExt, ClientUi, ClientWindow, LocalOnInput, SetLocal, UiLocalState,
};
use ss13_ui_kit::prelude::*;

use crate::comms::*;

/// `useState` keys.
const MODAL: &str = "modal";
const INPUT: &str = "input";
const NEW_ALERT_LEVEL: &str = "newAlertLevel";
/// `useSharedState` keys of `StatusDisplayControls`.
const STATUS_UPPER: &str = "statusUpperText";
const STATUS_LOWER: &str = "statusLowerText";

pub struct CommsClientPlugin;

impl Plugin for CommsClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_view::<CommsConsoleUi>()
            .register_ui_view::<crate::newscaster::NewscasterUi>()
            .register_ui_view::<crate::radio_ui::RadioUi>()
            .add_observer(submit_local);
    }
}

/// On a submit button: `act(action, { [param]: local value })` for each `(param, key, default)`
/// (the default when the key is unset), when the first value has at least `min_length`
/// characters; `clear` empties the keys after.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct SubmitLocal {
    pub action: String,
    pub params: Vec<(String, String, String)>,
    pub min_length: u32,
    pub clear: bool,
}

fn submit_local(
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
    let params: Vec<(String, String)> = submit
        .params
        .iter()
        .map(|(param, key, default)| {
            let value = local.value(key).unwrap_or(default).to_owned();
            (param.clone(), value)
        })
        .collect();
    let first = params.first().map_or(0, |(_, v)| v.chars().count() as u32);
    if first < submit.min_length {
        return;
    }
    intents.write(SendIntent(ClientIntent::UiAct {
        window: window.id,
        action: submit.action.clone(),
        params,
    }));
    if submit.clear {
        for (_, key, _) in &submit.params {
            local.set(key, "");
        }
    }
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

fn button(icon: &'static str, content: String, action: &str) -> impl Scene + use<> {
    let action = action.to_owned();
    bsn! {
        @ss13_ui_kit::Button { @icon: icon, @content: content }
        Act(action)
    }
}

fn set_state(page: &str) -> ActParams {
    ActParams(vec![(STATE.to_owned(), page.to_owned())])
}

fn back_section() -> impl Scene {
    let params = set_state(PAGE_MAIN);
    bsn! {
        @Section
        Children [
            @ss13_ui_kit::Button { @icon: "chevron-left", @content: "Back" }
            Act({SET_STATE.to_owned()})
            params
        ]
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
    let page = data.authenticated.then_some(data.page.as_str());
    let main = (page == Some(PAGE_MAIN)).then(|| EntityScene(main_page(data, local)));
    let messages = (page == Some(PAGE_MESSAGES)).then(|| EntityScene(messages_page(data)));
    let status =
        (page == Some(PAGE_CHANGING_STATUS)).then(|| EntityScene(status_page(data, local)));
    let buying = (page == Some(PAGE_BUYING_SHUTTLE)).then(|| EntityScene(buying_page(data)));
    bsn! {
        @WindowContent
        Children [
            {auth}
            --
            {main}
            --
            {messages}
            --
            {status}
            --
            {buying}
        ]
    }
}

/// `BuyingShuttle.tsx`: the budget, then a card per shuttle (cheapest first, as the server sends
/// them) whose button buys it.
fn buying_page(data: &CommsConsoleData) -> impl Scene + use<> {
    let budget = data.budget;
    let cards: Vec<_> = data
        .shuttles
        .iter()
        .map(|shuttle| {
            let short = budget < shuttle.credit_cost;
            let tooltip = if short {
                format!(
                    "You need {} more {}.",
                    shuttle.credit_cost - budget,
                    crate::cargo::MONEY_SYMBOL
                )
            } else {
                String::new()
            };
            let content = format!("{} {}", shuttle.credit_cost, crate::cargo::MONEY_SYMBOL);
            let params = ActParams(vec![(SHUTTLE.to_owned(), shuttle.id.clone())]);
            let buy: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
                @ss13_ui_kit::Button {
                    @content: content,
                    @tooltip: tooltip,
                    @tooltip_position: Placement::Left,
                }
                ss13_ui_kit::Button { disabled: short }
                Act({PURCHASE_SHUTTLE.to_owned()})
                params
            }));
            let (title, description) = (shuttle.name.clone(), shuttle.description.clone());
            let occupancy = format!("Occupancy Limit: {}", shuttle.occupancy_limit);
            bsn! {
                @Section { @title: title, @buttons: buy }
                Children [
                    Node Children [ Text(description) ]
                    --
                    Node Children [ Text(occupancy) ]
                ]
            }
        })
        .collect();
    let back = set_state(PAGE_MAIN);
    let funds = format!("{budget} {}", crate::cargo::MONEY_SYMBOL);
    bsn! {
        Node { flex_direction: FlexDirection::Column }
        Children [
            @Section
            Children [
                Node { justify_content: JustifyContent::SpaceBetween }
                Children [
                    @ss13_ui_kit::Button { @icon: "chevron-left", @content: "Back" }
                    Act({SET_STATE.to_owned()})
                    back
                    --
                    Node Children [ Text(funds) ]
                ]
            ]
            --
            {cards}
        ]
    }
}

/// `Main.tsx`.
fn main_page(data: &CommsConsoleData, local: &UiLocalState) -> impl Scene + use<> {
    let shuttle = (!data.syndicate).then(|| EntityScene(shuttle_section(data)));
    let can_set_alert = data.can_set_alert_level.is_some();
    let alert = can_set_alert.then(|| EntityScene(alert_section(data)));
    let new_level = local.get(NEW_ALERT_LEVEL).to_owned();
    let swipe = (can_set_alert && !new_level.is_empty() && new_level != data.alert_level)
        .then(|| EntityScene(swipe_modal(new_level)));
    let modal = match local.get(MODAL) {
        CALL_SHUTTLE if !data.shuttle_called => Some(ModalProps {
            label: "Nature of emergency".into(),
            notice: None,
            icon: "space-shuttle",
            button: "Call Shuttle",
            action: CALL_SHUTTLE,
            param: REASON,
            min_length: data.call_shuttle_reason_min_length,
        }),
        MESSAGE_ASSOCIATES if data.can_message_associates => Some(ModalProps {
            label: format!(
                "Message to transmit to {} via quantum entanglement",
                if data.emagged {
                    "[ABNORMAL ROUTING COORDINATES]"
                } else {
                    "CentCom"
                }
            ),
            notice: Some(
                "Please be aware that this process is very expensive, and abuse will lead \
                 to...termination. Transmission does not guarantee a response.",
            ),
            icon: "bullhorn",
            button: "Send",
            action: MESSAGE_ASSOCIATES,
            param: MESSAGE,
            min_length: 0,
        }),
        REQUEST_NUKE_CODES if data.can_request_nuke => Some(ModalProps {
            label: "Reason for requesting nuclear self-destruct codes".into(),
            notice: Some(
                "Misuse of the nuclear request system will not be tolerated under any \
                 circumstances. Transmission does not guarantee a response.",
            ),
            icon: "bomb",
            button: "Request Codes",
            action: REQUEST_NUKE_CODES,
            param: REASON,
            min_length: 0,
        }),
        REQUEST_ERT if data.can_request_ert => Some(ModalProps {
            label: "Reason for requesting Emergency Response Team".into(),
            notice: Some(
                "Misuse of the ERT system will not be tolerated under any circumstances. \
                 Transmission does not guarantee a response.",
            ),
            icon: "people-group",
            button: "Request ERT",
            action: REQUEST_ERT,
            param: REASON,
            min_length: 0,
        }),
        _ => None,
    }
    .map(|m| EntityScene(message_modal(m, data.max_message_length)));
    let functions = EntityScene(functions_section(data));
    bsn! {
        Node { flex_direction: FlexDirection::Column }
        Children [
            {shuttle}
            --
            {alert}
            --
            {functions}
            --
            {swipe}
            --
            {modal}
        ]
    }
}

/// `Main.tsx` `<Section title="Functions">`.
fn functions_section(data: &CommsConsoleData) -> impl Scene + use<> {
    let open_modal = |icon: &'static str, content: String, action: &str, disabled: bool| {
        let action = action.to_owned();
        EntityScene(bsn! {
            @ss13_ui_kit::Button { @icon: icon, @content: content }
            ss13_ui_kit::Button { disabled: disabled }
            SetLocal { key: {MODAL.to_owned()}, value: action, rerender: true }
        })
    };
    let announce = data.can_make_announcement.then(|| {
        EntityScene(button(
            "bullhorn",
            "Make Priority Announcement".into(),
            MAKE_PRIORITY_ANNOUNCEMENT,
        ))
    });
    let access = data.can_toggle_emergency_access.then(|| {
        let (verb, color) = if data.emergency_access {
            ("Disable", UiColor::Bad)
        } else {
            ("Enable", UiColor::Default)
        };
        let content = format!("{verb} Emergency Maintenance Access");
        EntityScene(bsn! {
            @ss13_ui_kit::Button { @icon: "id-card-o", @content: content }
            ss13_ui_kit::Button { color: color }
            Act({TOGGLE_EMERGENCY_ACCESS.to_owned()})
        })
    });
    let status = (!data.syndicate).then(|| {
        let params = set_state(PAGE_CHANGING_STATUS);
        EntityScene(bsn! {
            @ss13_ui_kit::Button { @icon: "desktop", @content: "Set Status Display" }
            Act({SET_STATE.to_owned()})
            params
        })
    });
    let list = set_state(PAGE_MESSAGES);
    // `canBuyShuttles !== 0`, disabled with the reason as its tooltip unless it is `1`.
    let purchase = data.can_buy_shuttles.then(|| {
        let reason = data.buy_shuttles_fail_reason.clone();
        let disabled = reason.is_some();
        let params = set_state(PAGE_BUYING_SHUTTLE);
        EntityScene(bsn! {
            @ss13_ui_kit::Button {
                @icon: "shopping-cart",
                @content: "Purchase Shuttle",
                @tooltip: {reason.unwrap_or_default()},
                @tooltip_position: Placement::Top,
            }
            ss13_ui_kit::Button { disabled: disabled }
            Act({SET_STATE.to_owned()})
            params
        })
    });
    let not_ready = !data.important_action_ready;
    let associates = data.can_message_associates.then(|| {
        let to = if data.emagged { "[UNKNOWN]" } else { "CentCom" };
        open_modal(
            "comment-o",
            format!("Send message to {to}"),
            MESSAGE_ASSOCIATES,
            not_ready,
        )
    });
    let nuke = data.can_request_nuke.then(|| {
        open_modal(
            "radiation",
            "Request Nuclear Authentication Codes".into(),
            REQUEST_NUKE_CODES,
            not_ready,
        )
    });
    let ert = data.can_request_ert.then(|| {
        open_modal(
            "people-group",
            "Request Emergency Response Team".into(),
            REQUEST_ERT,
            not_ready,
        )
    });
    let restore = (data.emagged && !data.syndicate).then(|| {
        EntityScene(button(
            "undo",
            "Restore Backup Routing Data".into(),
            RESTORE_BACKUP_ROUTING_DATA,
        ))
    });
    bsn! {
        @Section { @title: "Functions" }
        Children [
            Node { flex_direction: FlexDirection::Column }
            Children [
                {announce}
                --
                {access}
                --
                {status}
                --
                @ss13_ui_kit::Button { @icon: "envelope-o", @content: "Message List" }
                Act({SET_STATE.to_owned()})
                list
                --
                {purchase}
                --
                {associates}
                --
                {nuke}
                --
                {ert}
                --
                {restore}
            ]
        ]
    }
}

use ss13_chat::capitalize;

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
            SetLocal { key: {MODAL.to_owned()}, value: {CALL_SHUTTLE.to_owned()}, rerender: true }
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

/// `MessageModal`'s props.
struct ModalProps {
    label: String,
    notice: Option<&'static str>,
    icon: &'static str,
    button: &'static str,
    action: &'static str,
    param: &'static str,
    min_length: u32,
}

/// `MessageModal`: the text, then `act(action, { [param]: text })`.
fn message_modal(m: ModalProps, max: u32) -> impl Scene + use<> {
    let notice = m
        .notice
        .map(|n| EntityScene(bsn! { Node { max_width: px(300) } Children [ Text(n) ] }));
    let label = format!("{}:", m.label);
    let placeholder = m.label;
    let submit = SubmitLocal {
        action: m.action.to_owned(),
        params: vec![(m.param.to_owned(), INPUT.to_owned(), String::new())],
        min_length: m.min_length,
        clear: true,
    };
    let (icon, content) = (m.icon, m.button);
    bsn! {
        @Modal
        Children [
            Node { flex_direction: FlexDirection::Column }
            Children [
                Node { margin: UiRect::bottom(px(6)), max_width: px(300) }
                Children [ Text(label) ]
                --
                Node { width: px(300), margin: UiRect::bottom(px(6)) }
                Input {
                    placeholder: placeholder,
                    max_length: {Some(max as usize)},
                    fluid: true,
                    auto_focus: true,
                }
                ss13_ui_kit::TextArea { rows: 5.0 }
                LocalOnInput({INPUT.to_owned()})
                --
                Node
                Children [
                    @ss13_ui_kit::Button { @icon: icon, @content: content }
                    ss13_ui_kit::Button { color: UiColor::Good }
                    submit
                    SetLocal { key: {MODAL.to_owned()}, value: {String::new()}, rerender: true }
                    --
                    @ss13_ui_kit::Button { @icon: "times", @content: "Cancel" }
                    ss13_ui_kit::Button { color: UiColor::Bad }
                    SetLocal { key: {MODAL.to_owned()}, value: {String::new()}, rerender: true }
                ]
                --
                {notice}
            ]
        ]
    }
}

/// `Messages.tsx`: newest first; answers until one is chosen, delete.
fn messages_page(data: &CommsConsoleData) -> impl Scene + use<> {
    let sections: Vec<_> = data
        .messages
        .iter()
        .enumerate()
        .rev()
        .map(|(i, message)| {
            let index = (i + 1).to_string();
            let delete_params = ActParams(vec![(MESSAGE.to_owned(), index.clone())]);
            let delete: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
                @ss13_ui_kit::Button { @icon: "trash", @content: "Delete" }
                ss13_ui_kit::Button { color: UiColor::Red }
                Act({DELETE_MESSAGE.to_owned()})
                delete_params
            }));
            let answers: Vec<_> = message
                .possible_answers
                .iter()
                .enumerate()
                .map(|(a, answer)| {
                    let chosen = message.answered == a as u32 + 1;
                    let color = if chosen {
                        UiColor::Good
                    } else {
                        UiColor::Default
                    };
                    let content = answer.clone();
                    let open = message.answered == 0;
                    let pick = open.then(|| {
                        let params = ActParams(vec![
                            (MESSAGE.to_owned(), index.clone()),
                            (ANSWER.to_owned(), (a + 1).to_string()),
                        ]);
                        let content = content.clone();
                        EntityScene(bsn! {
                            @ss13_ui_kit::Button { @content: content }
                            Act({ANSWER_MESSAGE.to_owned()})
                            params
                        })
                    });
                    let shown = (!open).then(|| {
                        EntityScene(bsn! {
                            @ss13_ui_kit::Button { @content: content }
                            ss13_ui_kit::Button { color: color }
                        })
                    });
                    bsn! { Node Children [ {pick} -- {shown} ] }
                })
                .collect();
            let (title, content) = (message.title.clone(), message.content.clone());
            bsn! {
                @Section { @title: title, @buttons: delete }
                Children [
                    Node Children [ Text(content) ]
                    --
                    Node { margin: UiRect::top(px(6)) }
                    Children [ {answers} ]
                ]
            }
        })
        .collect();
    let back = EntityScene(back_section());
    bsn! {
        Node { flex_direction: FlexDirection::Column }
        Children [
            {back}
            --
            {sections}
        ]
    }
}

/// `ChangingStatus.tsx` with `StatusDisplayControls`.
fn status_page(data: &CommsConsoleData, local: &UiLocalState) -> impl Scene + use<> {
    let controls = EntityScene(status_display_controls(
        &data.upper_text,
        &data.lower_text,
        data.max_status_line_length as usize,
        local,
    ));
    let back = EntityScene(back_section());
    bsn! {
        Node { flex_direction: FlexDirection::Column }
        Children [ {back} -- {controls} ]
    }
}

/// `StatusDisplayControls` (also the PDA's Status Display, `crate::modpc`): the off and shuttle
/// buttons, the pictures, two lines and Send.
pub fn status_display_controls(
    upper_text: &str,
    lower_text: &str,
    max: usize,
    local: &UiLocalState,
) -> impl Scene + use<> {
    let picture = |icon: &'static str, content: &'static str, picture: &str| {
        let params = ActParams(vec![(PICTURE.to_owned(), picture.to_owned())]);
        bsn! {
            @ss13_ui_kit::Button { @icon: icon, @content: content }
            Act({SET_STATUS_PICTURE.to_owned()})
            params
        }
    };
    let line = |key: &str, initial: &str| {
        let value = local.value(key).unwrap_or(initial).to_owned();
        let key = key.to_owned();
        EntityScene(bsn! {
            Node { margin: UiRect::bottom(px(6)) }
            Input { value: value, max_length: {Some(max)}, fluid: true }
            LocalOnInput(key)
        })
    };
    let (upper, lower) = (
        line(STATUS_UPPER, upper_text),
        line(STATUS_LOWER, lower_text),
    );
    let send = SubmitLocal {
        action: SET_STATUS_MESSAGE.to_owned(),
        params: vec![
            (
                UPPER_TEXT.to_owned(),
                STATUS_UPPER.to_owned(),
                upper_text.to_owned(),
            ),
            (
                LOWER_TEXT.to_owned(),
                STATUS_LOWER.to_owned(),
                lower_text.to_owned(),
            ),
        ],
        min_length: 0,
        clear: false,
    };
    let (off, shuttle) = (
        EntityScene(picture("toggle-off", "Off", "blank")),
        EntityScene(picture("space-shuttle", "Shuttle ETA / Off", "shuttle")),
    );
    let graphics = vec![
        picture("flag", "Logo", "default"),
        picture("exclamation", "Security Alert Level", "currentalert"),
        picture("exclamation-triangle", "Lockdown", "lockdown"),
        picture("biohazard", "Biohazard", "biohazard"),
        picture("radiation", "Radiation", "radiation"),
    ];
    bsn! {
        Node { flex_direction: FlexDirection::Column }
        Children [
            @Section
            Children [ {off} -- {shuttle} ]
            --
            @Section { @title: "Graphics" }
            Children [ {graphics} ]
            --
            @Section { @title: "Message" }
            Children [
                Node { flex_direction: FlexDirection::Column }
                Children [
                    {upper}
                    --
                    {lower}
                    --
                    @ss13_ui_kit::Button { @icon: "comment-o", @content: "Send" }
                    send
                ]
            ]
        ]
    }
}
