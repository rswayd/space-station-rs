//! Client half of the PDA: the NtOS window (tg `NtosWindow` with `NtosMain`, `NtosMessenger`,
//! `NtosNotepad`, `NtosCrewManifest`, `NtosGasAnalyzer`).
// shortcut: one window size for every screen (tg sizes each interface), no device themes, no
// photos or search field in the messenger, Enter sends (no send button: buttons carry fixed
// params, the text is the input's), no "beginning of chat" divider or auto-scroll to the bottom.

use bevy::app::Propagate;
use bevy::scene::{EntityScene, SceneList};
use ss13_tgui::client::{ActOnSubmit, AppTguiClientExt, ClientUi, SetLocal, UiLocalState};
use ss13_tgui::interfaces::ru_jobs::department_ru;
use ss13_ui_kit::prelude::*;
use ss13_ui_kit::theme::{Bold, FontPx, Italic};

use super::*;

pub struct ModpcClientPlugin;

impl Plugin for ModpcClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_view::<NtosUi>();
    }
}

impl ClientUi for NtosUi {
    const WIDTH: f32 = 520.0;
    const HEIGHT: f32 = 640.0;

    fn content(_: &(), data: &NtosData) -> impl Scene {
        view(data, &UiLocalState::default())
    }

    fn content_local(_: &(), data: &NtosData, local: &UiLocalState) -> impl Scene {
        view(data, local)
    }
}

fn view(data: &NtosData, local: &UiLocalState) -> impl Scene + use<> {
    {
        let header = header(&data.header);
        let screen: Box<dyn SceneList> = match &data.screen {
            NtosScreen::Main(main) => Box::new(EntityScene(main_screen(main))),
            NtosScreen::Messenger(m) => Box::new(EntityScene(messenger_screen(m))),
            NtosScreen::Notepad(n) => Box::new(EntityScene(notepad_screen(n))),
            NtosScreen::CrewManifest(m) => Box::new(EntityScene(manifest_screen(m))),
            NtosScreen::GasAnalyzer(g) => Box::new(EntityScene(gas_screen(g))),
            NtosScreen::Card(c) => Box::new(EntityScene(card_screen(c, local))),
        };
        bsn! {
            @WindowContent
            Children [
                @Stack { @vertical: true, @fill: true }
                Children [
                    @StackItem
                    Children [ {EntityScene(header)} ]
                    --
                    @StackItem { @grow: 1.0 }
                    Node { min_height: px(0), flex_direction: FlexDirection::Column }
                    Children [ {screen} ]
                ]
            ]
        }
    }
}

fn icon_button(icon: &'static str, tooltip: &'static str, action: &'static str) -> impl Scene {
    bsn! {
        @ss13_ui_kit::Button { @icon: icon, @tooltip: tooltip }
        ss13_ui_kit::Button { color: UiColor::Transparent }
        Act({action.to_owned()})
        Name({format!("ntos: {action}")})
    }
}

/// `NtosWindow__header`: time, OS name and title on the left; background programs, signal,
/// battery and minimize/close or power off on the right.
fn header(h: &NtosHeader) -> impl Scene + use<> {
    let time = h.station_time.clone();
    let title = h.title.clone();
    let battery = h
        .battery_percent
        .map(|p| format!("{p}%"))
        .unwrap_or_default();
    let signal = if h.ntnet == 0 { "ban" } else { "signal" };
    let headers: Vec<_> = h
        .program_headers
        .iter()
        .map(|_| bsn! { Node { margin: UiRect::right(px(6)) } Children [ @Icon { @name: "comment-dots" } ] })
        .collect();
    let buttons: Box<dyn SceneList> = if h.show_exit {
        Box::new((
            EntityScene(icon_button("window-minimize-o", "Свернуть", PC_MINIMIZE)),
            EntityScene(icon_button("window-close-o", "Закрыть", PC_EXIT)),
        ))
    } else {
        Box::new(EntityScene(icon_button(
            "power-off",
            "Выключить",
            PC_SHUTDOWN,
        )))
    };
    bsn! {
        Node {
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Center,
            padding: UiRect::horizontal(px(4)),
        }
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.35))
        Children [
            Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(8) }
            Children [
                @Icon { @name: "calendar" }
                --
                Node Propagate::<Bold>(Bold(true)) Children [ Text(time) ]
                --
                Node
                Propagate::<Italic>(Italic(true))
                Propagate::<TextColor>(TextColor(Color::srgba(1.0, 1.0, 1.0, 0.33)))
                Children [ Text("NtOS") ]
                --
                Node Children [ Text(title) ]
            ]
            --
            Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(4) }
            Children [
                {headers}
                --
                @Icon { @name: signal }
                --
                @Icon { @name: "battery-full" }
                --
                Node Children [ Text(battery) ]
                --
                {buttons}
            ]
        ]
    }
}

fn run_params(name: &str) -> ActParams {
    ActParams(vec![("name".to_owned(), name.to_owned())])
}

/// `NtosMain`: header programs, the details section (light, ID), the program table.
fn main_screen(d: &NtosMainData) -> impl Scene + use<> {
    let header_programs: Vec<_> = d
        .programs
        .iter()
        .filter(|p| p.header_program)
        .map(|p| {
            let (desc, icon, params) = (p.desc.clone(), p.icon.clone(), run_params(&p.name));
            bsn! {
                @ss13_ui_kit::Button { @content: desc, @icon: icon }
                Act({PC_RUNPROGRAM.to_owned()})
                params
            }
        })
        .collect();
    let unknown = "Unknown".to_owned();
    let login_name = d.login.name.clone().unwrap_or_else(|| unknown.clone());
    let login_job = d.login.job.clone().unwrap_or(unknown);
    let proposed = |v: &Option<String>| v.as_ref().map(|v| format!(" ({v})")).unwrap_or_default();
    let name_line = format!(
        "Имя на ID: {login_name}{}",
        proposed(&d.proposed_login.name)
    );
    let job_line = format!("Назначение: {login_job}{}", proposed(&d.proposed_login.job));
    let imprint_disabled = d.proposed_login.name.is_none()
        || (d.proposed_login.name == d.login.name && d.proposed_login.job == d.login.job);
    let (light_on, has_light) = (d.light_on, d.has_light);
    let light = has_light.then(|| {
        let color = if light_on {
            UiColor::Good
        } else {
            UiColor::Bad
        };
        EntityScene(bsn! {
            @ss13_ui_kit::Button { @icon: "lightbulb", @tooltip: "Фонарик" }
            ss13_ui_kit::Button { color: color, selected: light_on }
            Act({PC_TOGGLE_LIGHT.to_owned()})
            Name("ntos: light")
        })
    });
    let id_inserted = d.id_inserted;
    let programs: Vec<_> = d
        .programs
        .iter()
        .filter(|p| !p.header_program)
        .map(|p| {
            let (desc, icon) = (p.desc.clone(), p.icon.clone());
            let (run, kill_params) = (run_params(&p.name), run_params(&p.name));
            let color = if p.alert {
                UiColor::Yellow
            } else {
                UiColor::Transparent
            };
            let kill = p.running.then(|| {
                EntityScene(bsn! {
                    @ss13_ui_kit::Button { @icon: "times", @tooltip: "Закрыть программу" }
                    ss13_ui_kit::Button { color: UiColor::Transparent }
                    Act({PC_KILLPROGRAM.to_owned()})
                    kill_params
                })
            });
            bsn! {
                Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center }
                Children [
                    Node { flex_grow: 1.0 }
                    Children [
                        @ss13_ui_kit::Button { @content: desc, @icon: icon, @fluid: true }
                        ss13_ui_kit::Button { color: color }
                        Act({PC_RUNPROGRAM.to_owned()})
                        run
                    ]
                    --
                    {kill}
                ]
            }
        })
        .collect();
    bsn! {
        @Section { @fill: true, @scrollable: true }
        Children [
            @Section
            Children [ @Stack Children [ {header_programs} ] ]
            --
            @Section {
                @title: "Детали",
                @buttons: bsn_list! {
                    {light}
                    --
                    @ss13_ui_kit::Button { @icon: "eject", @content: "Вытащить ID" }
                    ss13_ui_kit::Button { disabled: {!id_inserted} }
                    Act({PC_EJECT_DISK.to_owned()})
                    ActParams({vec![("name".to_owned(), "ID".to_owned())]})
                    --
                    @ss13_ui_kit::Button { @icon: "dna", @content: "Привязать ID" }
                    ss13_ui_kit::Button { disabled: imprint_disabled }
                    Act({PC_IMPRINT_ID.to_owned()})
                },
            }
            Children [
                Node Children [ Text(name_line) ]
                --
                Node Children [ Text(job_line) ]
            ]
            --
            @Section { @title: "Программы" }
            Children [ {programs} ]
        ]
    }
}

/// `NtosMessenger`: the open chat (a saved chat or a new one with a messenger), else contacts.
fn messenger_screen(m: &MessengerData) -> impl Scene + use<> {
    let open = m.open_chat.as_deref().and_then(|r| {
        if let Some(chat) = m.chat(r) {
            return Some((
                chat.recipient.clone(),
                chat.messages.clone(),
                chat.can_reply,
                r,
            ));
        }
        m.messenger(r).map(|p| (p.clone(), Vec::new(), true, r))
    });
    let body: Box<dyn SceneList> = match open {
        Some((recipient, messages, can_reply, chat_ref)) => Box::new(EntityScene(chat_screen(
            &recipient,
            &messages,
            can_reply,
            chat_ref,
            m.chat(chat_ref).is_some(),
        ))),
        None => Box::new(EntityScene(contacts_screen(m))),
    };
    bsn! {
        Node { flex_direction: FlexDirection::Column, flex_grow: 1.0, min_height: px(0) }
        Children [ {body} ]
    }
}

fn ref_params(r: &str) -> ActParams {
    ActParams(vec![("ref".to_owned(), r.to_owned())])
}

/// `ChatButton`; `node` names it for scripts (`pda chat` / `pda contact`).
fn chat_button(label: String, unread: u32, r: &str, node: &'static str) -> impl Scene + use<> {
    let text = if unread > 0 {
        let n = if unread <= 9 {
            unread.to_string()
        } else {
            "9+".to_owned()
        };
        let s = if unread != 1 { "s" } else { "" };
        format!("[{n} unread message{s}] {label}")
    } else {
        label.clone()
    };
    let icon = if unread > 0 { "envelope" } else { "" };
    let params = ref_params(r);
    bsn! {
        @ss13_ui_kit::Button { @content: text, @icon: icon, @fluid: true }
        Act({PDA_VIEW_MESSAGES.to_owned()})
        params
        Name(node)
    }
}

/// `ContactsScreen`.
fn contacts_screen(m: &MessengerData) -> impl Scene + use<> {
    let ringer = if m.alert_silenced {
        "Ringer: Off"
    } else {
        "Ringer: On"
    };
    let sending = if m.sending_and_receiving {
        "Send / Receive: On"
    } else {
        "Send / Receive: Off"
    };
    let sort = format!("Sort by: {}", if m.sort_by_job { "Job" } else { "Name" });
    let ringtone = m.ringtone.clone();
    let mut chats: Vec<&ChatData> = m.saved_chats.iter().collect();
    chats.sort_by_key(|c| c.unread_messages);
    let recent: Vec<_> = chats
        .iter()
        .filter(|c| c.visible)
        .map(|c| {
            chat_button(
                format!("{} ({})", c.recipient.name, c.recipient.job),
                c.unread_messages,
                &c.chat_ref,
                "pda chat",
            )
        })
        .collect();
    let mut contacts: Vec<_> = m
        .messengers
        .iter()
        .filter(|p| {
            chats
                .iter()
                .all(|c| c.recipient.messenger_ref != p.messenger_ref)
        })
        .map(|p| {
            chat_button(
                format!("{} ({})", p.name, p.job),
                0,
                &p.messenger_ref,
                "pda contact",
            )
        })
        .collect();
    contacts.extend(chats.iter().filter(|c| !c.visible).map(|c| {
        chat_button(
            format!("{} ({})", c.recipient.name, c.recipient.job),
            c.unread_messages,
            &c.chat_ref,
            "pda contact",
        )
    }));
    let no_users = contacts
        .is_empty()
        .then(|| EntityScene(bsn! { @NoticeBox Children [ Text("No users found.") ] }));
    let recent_section = (!recent.is_empty()).then(|| {
        EntityScene(bsn! {
            @Section { @title: "Previous Messages" }
            Children [ @Stack { @vertical: true } Children [ {recent} ] ]
        })
    });
    let no_id = m.owner.is_none().then(|| {
        EntityScene(bsn! {
            @NoticeBox { @color: UiColor::Red }
            Children [ Text("Please imprint an ID to continue.") ]
        })
    });
    bsn! {
        Node { flex_direction: FlexDirection::Column, flex_grow: 1.0, min_height: px(0) }
        Children [
            @Section
            Children [
                Node
                Propagate::<Bold>(Bold(true))
                Children [ Text("SpaceMessenger V6.5.4") ]
                --
                Node
                Propagate::<Italic>(Italic(true))
                Propagate::<TextColor>(TextColor(Color::srgba(1.0, 1.0, 1.0, 0.3)))
                Children [ Text("Bringing you spy-proof communications since 2467.") ]
                --
                Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, margin: UiRect::top(px(6)) }
                Children [
                    @ss13_ui_kit::Button { @icon: "bell", @content: ringer }
                    Act({PDA_TOGGLE_ALERTS.to_owned()})
                    --
                    @ss13_ui_kit::Button { @icon: "address-card", @content: sending }
                    Act({PDA_TOGGLE_SENDING.to_owned()})
                    --
                    @ss13_ui_kit::Button { @icon: "sort", @content: sort }
                    Act({PDA_CHANGE_SORT.to_owned()})
                ]
                --
                Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(6), margin: UiRect::top(px(6)) }
                Children [
                    @Icon { @name: "bell" }
                    --
                    Node Children [ Text("Set Ringtone") ]
                    --
                    Input { placeholder: ringtone, max_length: {Some(20)} }
                    ActOnSubmit {
                        action: {PDA_RING_SET.to_owned()},
                        param: {"ringtone".to_owned()},
                    }
                    Name("pda: ringtone")
                ]
            ]
            --
            {no_id}
            --
            {recent_section}
            --
            @Section { @title: "Detected Messengers", @fill: true, @scrollable: true }
            Children [ @Stack { @vertical: true } Children [ {no_users} -- {contacts} ] ]
        ]
    }
}

/// `ChatScreen`.
fn chat_screen(
    recipient: &MessengerEntry,
    messages: &[PdaMessage],
    can_reply: bool,
    chat_ref: &str,
    saved: bool,
) -> impl Scene + use<> {
    let title = format!("{} ({})", recipient.name, recipient.job);
    let lines: Vec<_> = messages
        .iter()
        .map(|msg| {
            let (align, bg) = if msg.outgoing {
                (AlignSelf::FlexEnd, Color::srgb(0.16, 0.32, 0.5))
            } else {
                (AlignSelf::FlexStart, Color::srgb(0.25, 0.25, 0.28))
            };
            let text = msg.message.clone();
            let ts = msg.timestamp.clone();
            bsn! {
                Node {
                    flex_direction: FlexDirection::Column,
                    align_self: align,
                    max_width: percent(80),
                    padding: UiRect::axes(px(8), px(4)),
                    margin: UiRect::bottom(px(4)),
                    border_radius: BorderRadius::all(px(6)),
                }
                BackgroundColor(bg)
                Children [
                    Node Children [ Text(text) ]
                    --
                    Node
                    Propagate::<FontPx>(FontPx(9.0))
                    Propagate::<TextColor>(TextColor(Color::srgba(1.0, 1.0, 1.0, 0.5)))
                    Children [ Text(ts) ]
                ]
            }
        })
        .collect();
    let chat_buttons = saved.then(|| {
        let (close, clear) = (ref_params(chat_ref), ref_params(chat_ref));
        (
            EntityScene(bsn! {
                @ss13_ui_kit::Button { @icon: "box-archive", @content: "Close chat" }
                Act({PDA_CLOSE_MESSAGES.to_owned()})
                close
            }),
            EntityScene(bsn! {
                @ss13_ui_kit::Button { @icon: "trash-can", @content: "Delete chat" }
                Act({PDA_CLEAR_MESSAGES.to_owned()})
                clear
            }),
        )
    });
    let placeholder = format!("Send message to {}...", recipient.name);
    let sending: Box<dyn SceneList> = if can_reply {
        Box::new(EntityScene(bsn! {
            @Section
            Children [
                Input { placeholder: placeholder, max_length: {Some(1024)}, self_clear: true, fluid: true }
                ActOnSubmit {
                    action: {PDA_SEND_MESSAGE.to_owned()},
                    param: {"message".to_owned()},
                    params: {vec![("ref".to_owned(), chat_ref.to_owned())]},
                }
                Name("pda: message")
            ]
        }))
    } else {
        Box::new(EntityScene(bsn! {
            @Section
            Children [
                Node
                Propagate::<Italic>(Italic(true))
                Propagate::<TextColor>(TextColor(Color::srgb(0.5, 0.5, 0.5)))
                Children [ Text("You cannot reply to this user.") ]
            ]
        }))
    };
    bsn! {
        Node { flex_direction: FlexDirection::Column, flex_grow: 1.0, min_height: px(0) }
        Children [
            @Section
            Children [
                Node { flex_direction: FlexDirection::Row }
                Children [
                    @ss13_ui_kit::Button { @icon: "arrow-left", @content: "Back" }
                    Act({PDA_VIEW_MESSAGES.to_owned()})
                    Name("pda: back")
                    --
                    {chat_buttons}
                ]
            ]
            --
            @Section { @title: title, @fill: true, @scrollable: true }
            Children [
                Node { flex_direction: FlexDirection::Column }
                Children [ {lines} ]
            ]
            --
            {sending}
        ]
    }
}

/// `NtosNotepad`: the note; Enter saves it (`UpdateNote`), Shift+Enter breaks the line.
fn notepad_screen(n: &NotepadData) -> impl Scene + use<> {
    let note = n.note.clone();
    bsn! {
        @Section { @title: "Notepad", @fill: true }
        Children [
            Input { value: note, fluid: true, max_length: {Some(1800)} }
            TextArea { rows: 16.0 }
            ActOnSubmit {
                action: {UPDATE_NOTE.to_owned()},
                param: {"newnote".to_owned()},
            }
            Name("pda: note")
            --
            Node
            Propagate::<FontPx>(FontPx(10.0))
            Propagate::<TextColor>(TextColor(Color::srgba(1.0, 1.0, 1.0, 0.5)))
            Children [ Text("Enter — сохранить, Shift+Enter — новая строка") ]
        ]
    }
}

/// `NtosCrewManifest`.
fn manifest_screen(departments: &[ManifestDepartment]) -> impl Scene + use<> {
    let sections: Vec<_> = departments
        .iter()
        .map(|dept| {
            let title = department_ru(&dept.name).unwrap_or(&dept.name).to_owned();
            let rows: Vec<_> = dept
                .entries
                .iter()
                .map(|(name, rank)| {
                    let (name, rank) = (name.clone(), format!("({rank})"));
                    bsn! {
                        @TableRow
                        Children [
                            @TableCell { @content: name, @bold: true }
                            --
                            @TableCell { @content: rank }
                        ]
                    }
                })
                .collect();
            bsn! {
                @Section { @title: title }
                Children [ @Table Children [ {rows} ] ]
            }
        })
        .collect();
    bsn! {
        @Section { @title: "Список экипажа", @fill: true, @scrollable: true }
        Children [ {sections} ]
    }
}

/// `NtosGasAnalyzer` (`GasAnalyzerContent`) of the device's tile.
fn gas_screen(mixes: &[GasMix]) -> impl Scene + use<> {
    let sections: Vec<_> = mixes
        .iter()
        .map(|mix| {
            let mut items = vec![
                ("Давление".to_owned(), format!("{:.2} кПа", mix.pressure)),
                (
                    "Температура".to_owned(),
                    format!(
                        "{:.2} K ({:.2} °C)",
                        mix.temperature,
                        mix.temperature - 273.15
                    ),
                ),
                ("Объём".to_owned(), format!("{:.0} л", mix.volume)),
                (
                    "Всего молей".to_owned(),
                    format!("{:.2} моль", mix.total_moles),
                ),
            ];
            items.extend(mix.gases.iter().map(|(gas, moles)| {
                let percent = if mix.total_moles > 0.0 {
                    moles / mix.total_moles * 100.0
                } else {
                    0.0
                };
                (gas.clone(), format!("{moles:.2} моль ({percent:.2} %)"))
            }));
            let items: Vec<_> = items
                .into_iter()
                .map(|(label, value)| bsn! { @LabeledListItem { @label: label, @content: value } })
                .collect();
            let name = mix.name.clone();
            bsn! {
                @Section { @title: name }
                Children [ @LabeledList Children [ {items} ] ]
            }
        })
        .collect();
    let empty = mixes
        .is_empty()
        .then(|| EntityScene(bsn! { @NoticeBox Children [ Text("Нет воздуха.") ] }));
    bsn! {
        @Section { @title: "AtmoZphere", @fill: true, @scrollable: true }
        Children [ {empty} -- {sections} ]
    }
}

/// `NtosCard` local state: the wildcard tab (`"None"`: the trim's basic access) and the region.
const WILDCARD_TAB: &str = "wildcardSelected";
const REGION_TAB: &str = "accessName";

fn act_button(
    content: String,
    icon: &'static str,
    action: &'static str,
    disabled: bool,
) -> impl Scene {
    bsn! {
        @ss13_ui_kit::Button { @content: content, @icon: icon }
        ss13_ui_kit::Button { disabled: disabled }
        Act({action.to_owned()})
        Name({format!("card: {action}")})
    }
}

/// `NtosCardContent`: login, the card's details, templates and `AccessList`.
fn card_screen(d: &CardModData, local: &UiLocalState) -> impl Scene + use<> {
    let authed = d.authed_user.is_some();
    let login_text = d
        .authed_user
        .as_ref()
        .map_or("Please log in to continue.".to_owned(), |u| {
            format!("Login: {u}")
        });
    let (main_label, main_action) = match &d.auth_card {
        Some(c) => (
            format!("{} ({})", c.id_owner, c.id_rank),
            PRG_REMOVE_MAIN_ID,
        ),
        None => ("Insert ID".to_owned(), PRG_INSERT_MAIN_ID),
    };
    let main_disabled = d.auth_card.is_none() && !d.is_holding_id;
    let (alt_label, alt_action) = match &d.modified_card {
        Some(c) => (format!("{} ({})", c.id_owner, c.id_rank), PRG_REMOVE_ALT_ID),
        None => ("Вставить ID".to_owned(), PRG_INSERT_ALT_ID),
    };
    let alt_disabled = d.modified_card.is_none() && !d.is_holding_id;
    let (log_label, log_action, log_color) = if authed {
        ("Log Out", PRG_LOGOUT, UiColor::Bad)
    } else {
        ("Log In", PRG_AUTHENTICATE, UiColor::Good)
    };
    let editing = d.modified_card.clone().filter(|_| authed);
    let details = editing.clone().map(|card| {
        let (owner, age, rank) = (card.id_owner.clone(), card.id_age.to_string(), card.id_rank.clone());
        EntityScene(bsn! {
            Node { flex_direction: FlexDirection::Column, row_gap: px(4), margin: UiRect::top(px(6)) }
            Children [
                Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(6) }
                Children [
                    Node Children [ Text("Детали:") ]
                    --
                    Node { flex_grow: 1.0 }
                    Children [
                        Input { value: owner, fluid: true, max_length: {Some(42)} }
                        ActOnSubmit { action: {PRG_EDIT.to_owned()}, param: {"name".to_owned()} }
                        Name("card: name")
                    ]
                    --
                    Node { width: px(70) }
                    Children [
                        Input { value: age, fluid: true, max_length: {Some(3)} }
                        ActOnSubmit { action: {PRG_AGE.to_owned()}, param: {"id_age".to_owned()} }
                        Name("card: age")
                    ]
                ]
                --
                Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(6) }
                Children [
                    Node Children [ Text("Назначение:") ]
                    --
                    Node { flex_grow: 1.0 }
                    Children [
                        Input { value: rank, fluid: true, max_length: {Some(42)} }
                        ActOnSubmit { action: {PRG_ASSIGN.to_owned()}, param: {"assignment".to_owned()} }
                        Name("card: assignment")
                    ]
                ]
            ]
        })
    });
    let templates = editing.as_ref().map(|card| {
        let body: Box<dyn SceneList> = if card.has_trim {
            let options = d.templates.clone();
            Box::new(EntityScene(bsn! {
                @Dropdown { @placeholder: "Выбрать шаблон...", @fluid: true, @value_param: "name" }
                Dropdown { options: options, selected: "None" }
                Act({PRG_TEMPLATE.to_owned()})
                Name("card: template")
            }))
        } else {
            Box::new(EntityScene(bsn! {
                Node Children [ Text("Templates require a trim already applied to the card. Please use an ID Painter to apply a trim.") ]
            }))
        };
        EntityScene(bsn! {
            @Section { @title: "Templates" }
            Children [ {body} ]
        })
    });
    let access = editing.map(|card| EntityScene(access_list(d, &card, local)));
    bsn! {
        @Section { @fill: true, @scrollable: true }
        Children [
            @Section
            Children [
                @NoticeBox { @info: authed } Children [ Text(login_text) ]
                --
                Node { flex_direction: FlexDirection::Row, column_gap: px(4) }
                Children [
                    Node { flex_grow: 1.0 }
                    Children [
                        @ss13_ui_kit::Button { @content: main_label, @icon: "eject", @fluid: true }
                        ss13_ui_kit::Button { disabled: main_disabled }
                        Act({main_action.to_owned()})
                        Name("card: main id")
                    ]
                    --
                    @ss13_ui_kit::Button { @content: log_label, @icon: "sign-in-alt" }
                    ss13_ui_kit::Button { color: log_color }
                    Act({log_action.to_owned()})
                    Name("card: login")
                ]
            ]
            --
            @Section
            Children [
                Node { flex_direction: FlexDirection::Row, column_gap: px(4) }
                Children [
                    Node { flex_grow: 1.0 }
                    Children [
                        @ss13_ui_kit::Button { @content: alt_label, @icon: "eject", @fluid: true }
                        ss13_ui_kit::Button { disabled: alt_disabled }
                        Act({alt_action.to_owned()})
                        Name("card: target id")
                    ]
                    --
                    {EntityScene(act_button("Распечатать отчет".to_owned(), "print", "PRG_print", true))}
                ]
                --
                {details}
            ]
            --
            {templates}
            --
            {access}
        ]
    }
}

/// `AccessList`: wildcard tabs, region tabs, the region's access checkboxes.
fn access_list(d: &CardModData, card: &CardUi, local: &UiLocalState) -> impl Scene + use<> {
    let slot_names: Vec<&str> = card
        .wildcard_slots
        .iter()
        .map(|s| s.name.as_str())
        .collect();
    let wildcard = match local.get(WILDCARD_TAB) {
        "" | "None" => "None",
        w if slot_names.contains(&w) => w,
        _ => "None",
    }
    .to_owned();
    let flag_of = |name: &str| {
        d.wildcard_flags
            .iter()
            .find(|(n, _)| n == name)
            .map_or(0, |(_, f)| *f)
    };
    let selected = &card.access_on_card;
    // `parsedRegions`: what each region lists under the chosen tab, and its selection summary.
    let regions: Vec<(String, Vec<AccessUi>, bool, bool)> = d
        .regions
        .iter()
        .filter_map(|r| {
            let accesses: Vec<AccessUi> = r
                .accesses
                .iter()
                .filter(|a| {
                    let basic = card.trim_access.contains(&a.access);
                    if wildcard == "None" {
                        basic
                    } else {
                        !basic && a.flag & flag_of(&wildcard) != 0
                    }
                })
                .cloned()
                .collect();
            if accesses.is_empty() {
                return None;
            }
            let any = accesses.iter().any(|a| selected.contains(&a.access));
            let all = accesses.iter().all(|a| selected.contains(&a.access));
            Some((r.name.clone(), accesses, any, all))
        })
        .collect();
    let basic_used = card
        .trim_access
        .iter()
        .filter(|a| selected.contains(a))
        .count();
    let mut tabs = vec![(
        "None".to_owned(),
        format!("Trim: {basic_used}/{}", card.trim_access.len()),
    )];
    tabs.extend(card.wildcard_slots.iter().map(|s| {
        let limit = if s.limit < 0 {
            "∞".to_owned()
        } else {
            s.limit.to_string()
        };
        (
            s.name.clone(),
            format!("{}: {}/{limit}", s.name, s.usage.len()),
        )
    }));
    let wildcard_tabs: Vec<_> = tabs
        .into_iter()
        .map(|(key, label)| {
            let sel = key == wildcard;
            let node = format!("wildcard: {key}");
            bsn! {
                @TabsTab { @content: label }
                TabsTab { selected: sel }
                SetLocal { key: {WILDCARD_TAB.to_owned()}, value: key, rerender: true }
                Name(node)
            }
        })
        .collect();
    let region = regions
        .iter()
        .find(|r| r.0 == local.get(REGION_TAB))
        .or(regions.first())
        .cloned();
    let region_name = region.as_ref().map(|r| r.0.clone()).unwrap_or_default();
    let region_tabs: Vec<_> = regions
        .iter()
        .map(|(name, _, any, all)| {
            let icon = if *all {
                "check"
            } else if *any {
                "minus"
            } else {
                "times"
            };
            let (label, key, sel) = (name.clone(), name.clone(), *name == region_name);
            let node = format!("region: {name}");
            bsn! {
                @TabsTab { @content: label, @icon: icon }
                TabsTab { selected: sel }
                SetLocal { key: {REGION_TAB.to_owned()}, value: key, rerender: true }
                Name(node)
            }
        })
        .collect();
    // Which slot holds each wildcard (`wcAccess`), and what the chosen slot has left.
    let holder = |access: &str| {
        card.wildcard_slots
            .iter()
            .find(|s| s.usage.iter().any(|u| u == access))
            .map(|s| s.name.clone())
    };
    let chosen = card.wildcard_slots.iter().find(|s| s.name == wildcard);
    let avail = chosen.map_or(-1, |s| s.limit - s.usage.len() as i32);
    let mut entries = region.map(|r| r.1).unwrap_or_default();
    entries.sort_by(|a, b| a.desc.cmp(&b.desc));
    let checkboxes: Vec<_> = entries
        .into_iter()
        .map(|a| {
            let held_in = holder(&a.access);
            let disabled = (avail == 0 && held_in.as_deref() != Some(wildcard.as_str()))
                || (avail > 0 && held_in.as_ref().is_some_and(|h| *h != wildcard));
            let label = if held_in.is_none() && card.trim_access.contains(&a.access) {
                a.desc.clone()
            } else {
                let flag = d
                    .access_flag_names
                    .iter()
                    .find(|(f, _)| *f == a.flag)
                    .map_or("", |(_, n)| n.as_str());
                format!("{} ({flag})", a.desc)
            };
            let checked = selected.contains(&a.access);
            let icon = if checked {
                "check-square-o"
            } else {
                "square-o"
            };
            let mut params = vec![("access_target".to_owned(), a.access.clone())];
            if wildcard != "None" {
                params.push(("access_wildcard".to_owned(), wildcard.clone()));
            }
            let node = format!("access: {}", a.access);
            bsn! {
                @ss13_ui_kit::Button { @content: label, @icon: icon, @fluid: true }
                ss13_ui_kit::Button { selected: checked, disabled: disabled }
                Act({PRG_ACCESS.to_owned()})
                ActParams(params)
                Name(node)
            }
        })
        .collect();
    bsn! {
        @Section {
            @title: "Access",
            @buttons: bsn_list! {
                @ss13_ui_kit::Button { @content: "Terminate Employment" }
                ss13_ui_kit::Button { color: UiColor::Bad }
                Act({PRG_TERMINATE.to_owned()})
                Name("card: terminate")
            },
        }
        Children [
            @Tabs Children [ {wildcard_tabs} ]
            --
            Node { flex_direction: FlexDirection::Row, column_gap: px(6) }
            Children [
                Node { min_width: px(150) }
                Children [ @Tabs { @vertical: true } Children [ {region_tabs} ] ]
                --
                Node { flex_grow: 1.0, flex_direction: FlexDirection::Column }
                Children [ {checkboxes} ]
            ]
        ]
    }
}
