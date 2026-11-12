//! The requests console window (client): tgui `RequestsConsole/*` with the header (send error,
//! unread messages, the emergency buttons or the called emergency), the tabs (view messages,
//! write a message, make an announcement) and the speaker toggle. The data is
//! `crate::requests_console`, the server half `crate::wallmount::requests`.
// shortcut: the recipients are a list of buttons instead of a `Dropdown`; opening the write or
// announcement tab does not clear the unread status or the authentication (the view tab and the
// discard buttons do); sending keeps the typed text (a clear would run before the act reads it;
// Discard clears it); no appended lists (no ore updates).

use bevy::scene::{EntityScene, SceneList};
use ss13_tgui::client::{ActWithLocal, ClearLocal, ClientUi, LocalOnInput, SetLocal, UiLocalState};
use ss13_ui_kit::prelude::*;

use crate::requests_console::*;

/// `useState` keys.
const TAB: &str = "tab";
const KIND: &str = "request_type";
const TO: &str = "recipient";
const PRIO: &str = "priority";
const TEXT: &str = "messageText";
const ANNOUNCE_TEXT: &str = "announceText";
/// `RequestTabs`.
const TAB_VIEW: &str = "1";
const TAB_WRITE: &str = "2";
const TAB_ANNOUNCE: &str = "3";

impl ClientUi for RequestsConsoleUi {
    const WIDTH: f32 = 500.0;
    const HEIGHT: f32 = 600.0;

    fn content(statics: &RequestsStatic, data: &RequestsData) -> impl Scene {
        view(statics, data, &UiLocalState::default())
    }

    fn content_local(
        statics: &RequestsStatic,
        data: &RequestsData,
        local: &UiLocalState,
    ) -> impl Scene {
        view(statics, data, local)
    }
}

fn act_button(
    icon: &'static str,
    content: &str,
    action: &str,
    params: ActParams,
) -> impl Scene + use<> {
    let (content, action) = (content.to_owned(), action.to_owned());
    bsn! {
        @ss13_ui_kit::Button { @icon: icon, @content: content }
        Act(action)
        params
    }
}

fn notice(text: String, danger: bool) -> impl Scene + use<> {
    bsn! { @NoticeBox { @danger: danger } Children [ Text(text) ] }
}

fn view(statics: &RequestsStatic, data: &RequestsData, local: &UiLocalState) -> impl Scene + use<> {
    let header = EntityScene(header(data));
    let tab = match local.get(TAB) {
        TAB_WRITE => TAB_WRITE,
        TAB_ANNOUNCE if data.can_send_announcements => TAB_ANNOUNCE,
        _ => TAB_VIEW,
    };
    let tab_button = |label: &str, value: &str| {
        let (label, value) = (label.to_owned(), value.to_owned());
        let selected = tab == value;
        let clear = value == TAB_VIEW;
        let scene = bsn! {
            @TabsTab { @content: label }
            TabsTab { selected: selected }
            SetLocal { key: {TAB.to_owned()}, value: value, rerender: true }
        };
        // The view tab reads the messages (`clear_message_status`).
        let act = clear.then(|| bsn! { Act({CLEAR_MESSAGE_STATUS.to_owned()}) });
        (scene, act)
    };
    let mut tabs: Vec<_> = vec![
        tab_button("View Messages", TAB_VIEW),
        tab_button("Write Message", TAB_WRITE),
    ];
    if data.can_send_announcements {
        tabs.push(tab_button("Make Announcement", TAB_ANNOUNCE));
    }
    let body: Box<dyn SceneList> = match tab {
        TAB_WRITE => Box::new(EntityScene(write_tab(statics, data, local))),
        TAB_ANNOUNCE => Box::new(EntityScene(announce_tab(data))),
        _ => messages_tab(data),
    };
    let speaker = if data.silent {
        "square-o"
    } else {
        "check-square-o"
    };
    let footer = EntityScene(act_button(
        speaker,
        "Speaker",
        TOGGLE_SILENT,
        ActParams(Vec::new()),
    ));
    bsn! {
        @WindowContent
        Children [
            Node { flex_direction: FlexDirection::Column, flex_grow: 1.0, min_height: px(0) }
            Children [
                {header}
                --
                @Tabs { @fluid: true }
                Children [ {tabs} ]
                --
                Node { flex_direction: FlexDirection::Column, flex_grow: 1.0, min_height: px(0) }
                Children [ {body} ]
                --
                {footer}
            ]
        ]
    }
}

/// `RequestsConsoleHeader`.
fn header(data: &RequestsData) -> impl Scene + use<> {
    let error = data.has_mail_send_error.then(|| {
        EntityScene(notice(
            "Error occured while sending a message!".to_owned(),
            true,
        ))
    });
    let unread = (data.new_message_priority != PRIORITY_NONE).then(|| {
        let level = match data.new_message_priority {
            PRIORITY_HIGH => "PRIORITY ",
            PRIORITY_EXTREME => "EXTREME PRIORITY ",
            _ => "",
        };
        EntityScene(notice(
            format!("You have new unread {level}messages"),
            false,
        ))
    });
    let emergency: Box<dyn SceneList> = if data.emergency.is_empty() {
        let call = |icon: &'static str, label: &str, kind: &str| {
            let params = ActParams(vec![(EMERGENCY.to_owned(), kind.to_owned())]);
            let (label, action) = (label.to_owned(), SET_EMERGENCY.to_owned());
            bsn! {
                @ss13_ui_kit::Button { @icon: icon, @content: label, @fluid: true }
                ss13_ui_kit::Button { color: UiColor::Red }
                Act(action)
                params
            }
        };
        let buttons = vec![
            call("shield", "Call Security", EMERGENCY_SECURITY),
            call(
                "screwdriver-wrench",
                "Call Engineering",
                EMERGENCY_ENGINEERING,
            ),
            call("suitcase-medical", "Call Medical", EMERGENCY_MEDICAL),
        ];
        Box::new(EntityScene(bsn! {
            Node { flex_direction: FlexDirection::Row, column_gap: px(4) }
            Children [ {buttons} ]
        }))
    } else {
        Box::new(EntityScene(notice(
            format!("{} called!", data.emergency),
            true,
        )))
    };
    bsn! {
        Node { flex_direction: FlexDirection::Column, margin: UiRect::bottom(px(6)) }
        Children [ {error} -- {unread} -- {emergency} ]
    }
}

/// `MessageViewTab`.
fn messages_tab(data: &RequestsData) -> Box<dyn SceneList> {
    let messages: Vec<_> = data
        .messages
        .iter()
        .map(|m| {
            let title = format!(
                "{} from {}, {}",
                m.request_type, m.sender_department, m.received_time
            );
            let priority: Option<EntityScene<_>> = match m.priority {
                PRIORITY_HIGH => Some(EntityScene(notice("High Priority".to_owned(), false))),
                PRIORITY_EXTREME => Some(EntityScene(notice(
                    "!!!Extreme Priority!!!".to_owned(),
                    true,
                ))),
                _ => None,
            };
            let verified = if m.message_verified_by.is_empty() {
                "Not Verified".to_owned()
            } else {
                m.message_verified_by.clone()
            };
            let stamped = if m.message_stamped_by.is_empty() {
                "Not Stamped".to_owned()
            } else {
                m.message_stamped_by.clone()
            };
            let reply = (m.request_type != ORE_UPDATE_REQUEST).then(|| {
                let params = ActParams(vec![(
                    REPLY_RECIPIENT.to_owned(),
                    m.sender_department.clone(),
                )]);
                EntityScene(act_button("reply", "Quick Reply", QUICK_REPLY, params))
            });
            let content = m.content.clone();
            bsn! {
                @Section { @title: title }
                Children [
                    {priority}
                    --
                    Node { margin: UiRect::bottom(px(4)) }
                    Children [ Text(content) ]
                    --
                    @LabeledList
                    Children [
                        @LabeledListItem { @label: "Message Verified By" }
                        Children [ Text(verified) ]
                        --
                        @LabeledListItem { @label: "Message Stamped By" }
                        Children [ Text(stamped) ]
                    ]
                    --
                    {reply}
                ]
            }
        })
        .collect();
    Box::new(EntityScene(bsn! {
        @Section { @fill: true, @scrollable: true }
        Children [
            Node { flex_direction: FlexDirection::Column }
            Children [ {messages} ]
        ]
    }))
}

/// `MessageWriteTab`.
fn write_tab(
    statics: &RequestsStatic,
    data: &RequestsData,
    local: &UiLocalState,
) -> impl Scene + use<> {
    let kind = local.value(KIND).unwrap_or(ASSISTANCE_REQUEST).to_owned();
    let recipient = local.value(TO).unwrap_or_default().to_owned();
    let priority = local
        .value(PRIO)
        .unwrap_or("1")
        .parse::<u8>()
        .unwrap_or(PRIORITY_NORMAL);
    let kind_button = |icon: &'static str, label: &str, value: &str| {
        let selected = kind == value;
        let (label, value) = (label.to_owned(), value.to_owned());
        bsn! {
            @ss13_ui_kit::Button { @icon: icon, @content: label, @fluid: true }
            ss13_ui_kit::Button { selected: selected }
            SetLocal { key: {KIND.to_owned()}, value: value, rerender: true }
            ClearLocal({vec![TO.to_owned()]})
        }
    };
    let kinds = vec![
        kind_button("handshake-angle", "Request Assistance", ASSISTANCE_REQUEST),
        kind_button("boxes-stacked", "Request Supplies", SUPPLIES_REQUEST),
        kind_button("upload", "Relay Information", INFORMATION_REQUEST),
    ];
    let list = match kind.as_str() {
        SUPPLIES_REQUEST => &statics.supply_consoles,
        INFORMATION_REQUEST => &statics.information_consoles,
        _ => &statics.assistance_consoles,
    };
    let recipients: Vec<_> = list
        .iter()
        .map(|department| {
            let selected = *department == recipient;
            let department = department.clone();
            bsn! {
                @ss13_ui_kit::Button { @content: {department.clone()} }
                ss13_ui_kit::Button { selected: selected }
                SetLocal { key: {TO.to_owned()}, value: department, rerender: true }
            }
        })
        .collect();
    let priority_button = |icon: &'static str, label: &str, value: u8| {
        let selected = priority == value;
        let label = label.to_owned();
        bsn! {
            @ss13_ui_kit::Button { @icon: icon, @content: label, @fluid: true }
            ss13_ui_kit::Button { selected: selected }
            SetLocal { key: {PRIO.to_owned()}, value: {value.to_string()}, rerender: true }
        }
    };
    let mut priorities = vec![
        priority_button("envelope", "Normal Priority", PRIORITY_NORMAL),
        priority_button("exclamation", "High Priority", PRIORITY_HIGH),
    ];
    if data.hack_state {
        priorities.push(priority_button(
            "burst",
            "EXTREME PRIORITY",
            PRIORITY_EXTREME,
        ));
    }
    let text = local.value(TEXT).unwrap_or_default().to_owned();
    let send_params = ActParams(vec![
        (RECIPIENT.to_owned(), recipient.clone()),
        (PRIORITY.to_owned(), priority.to_string()),
        (REQUEST_TYPE.to_owned(), kind.clone()),
    ]);
    let send_disabled = recipient.is_empty();
    let auth = &data.authentication_data;
    let verified = if auth.message_verified_by.is_empty() {
        "Not verified".to_owned()
    } else {
        auth.message_verified_by.clone()
    };
    let stamped = if auth.message_stamped_by.is_empty() {
        "Not stamped".to_owned()
    } else {
        auth.message_stamped_by.clone()
    };
    let reset = vec![
        TEXT.to_owned(),
        TO.to_owned(),
        PRIO.to_owned(),
        KIND.to_owned(),
    ];
    bsn! {
        @Section
        Children [
            Node { flex_direction: FlexDirection::Row, column_gap: px(4), margin: UiRect::bottom(px(6)) }
            Children [ {kinds} ]
            --
            @Section { @title: "Pick a Recipient" }
            Children [
                Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap }
                Children [ {recipients} ]
            ]
            --
            Node { flex_direction: FlexDirection::Row, column_gap: px(4), margin: UiRect::vertical(px(6)) }
            Children [ {priorities} ]
            --
            Node { margin: UiRect::bottom(px(6)) }
            Input {
                value: text,
                placeholder: {"Type your message...".to_owned()},
                max_length: {Some(1024)},
                fluid: true,
            }
            ss13_ui_kit::TextArea { rows: 6.0 }
            LocalOnInput({TEXT.to_owned()})
            --
            Node { flex_direction: FlexDirection::Row, column_gap: px(4) }
            Children [
                @ss13_ui_kit::Button { @icon: "paper-plane", @content: "Send message" }
                ss13_ui_kit::Button { disabled: send_disabled }
                Act({SEND_MESSAGE.to_owned()})
                send_params
                ActWithLocal { param: {MESSAGE.to_owned()}, key: {TEXT.to_owned()}, default: {String::new()} }
                --
                @ss13_ui_kit::Button { @icon: "id-card", @content: verified }
                --
                @ss13_ui_kit::Button { @icon: "stamp", @content: stamped }
            ]
            --
            @ss13_ui_kit::Button { @icon: "trash-can", @content: "Discard message" }
            Act({CLEAR_AUTHENTICATION.to_owned()})
            ClearLocal(reset)
        ]
    }
}

/// `AnnouncementTab`.
fn announce_tab(data: &RequestsData) -> impl Scene + use<> {
    let authenticated = data.authentication_data.announcement_authenticated;
    let notice_box = if authenticated {
        EntityScene(bsn! {
            @NoticeBox { @info: true } Children [ Text({"Successfully authenticated".to_owned()}) ]
        })
    } else {
        EntityScene(bsn! {
            @NoticeBox Children [ Text({"Swipe your card to authenticate yourself".to_owned()}) ]
        })
    };
    let reset = vec![ANNOUNCE_TEXT.to_owned()];
    bsn! {
        @Section
        Children [
            Node { margin: UiRect::bottom(px(6)) }
            Input {
                placeholder: {"Type your announcement...".to_owned()},
                max_length: {Some(1024)},
                fluid: true,
            }
            ss13_ui_kit::TextArea { rows: 6.0 }
            LocalOnInput({ANNOUNCE_TEXT.to_owned()})
            --
            {notice_box}
            --
            Node { flex_direction: FlexDirection::Row, column_gap: px(4) }
            Children [
                @ss13_ui_kit::Button { @icon: "bullhorn", @content: "Send announcement" }
                ss13_ui_kit::Button { disabled: {!authenticated} }
                Act({SEND_ANNOUNCEMENT.to_owned()})
                ActWithLocal { param: {MESSAGE.to_owned()}, key: {ANNOUNCE_TEXT.to_owned()}, default: {String::new()} }
                --
                @ss13_ui_kit::Button { @icon: "trash-can", @content: "Discard announcement" }
                Act({CLEAR_AUTHENTICATION.to_owned()})
                ClearLocal(reset)
            ]
        ]
    }
}
