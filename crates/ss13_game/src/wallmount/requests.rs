//! Requests consoles (server), after tg `/obj/machinery/requests_console`
//! (`code/game/machinery/requests_console.dm`, `code/datums/request_message.dm`, the mapping
//! helpers of `mapping_helpers.dm`) with BandaStation's Russian announcement title.
//!
//! - Map init: the department (`auto_name`: the area's name; else `department`, the map's name,
//!   or "Unknown") names the console "<department> requests console"; the helpers on its tile
//!   let it announce or list it as a recipient of assistance, supply or information requests.
//! - An empty hand opens the `RequestsConsole` window (not with the panel open, not unpowered);
//!   an ID verifies the next message (and allows announcements with `ACCESS_RC_ANNOUNCE`), a
//!   stamp stamps it; a crowbar opens the panel, a screwdriver inside toggles the hack (extreme
//!   priority).
//! - A message goes through the telecomms message servers (`crate::telecomms::deliver_message`)
//!   to every console of the recipient department: its screen shows the priority, it beeps and
//!   says who sent it (unless silent; an extreme message unmutes it), and messages to the
//!   bridge, medbay, science, engineering, security, cargo bay or mining go out on that radio
//!   channel through the announcement system.
//! - The emergency buttons call security, engineering or medical through the announcement
//!   system for 5 minutes; announcements (authenticated, every 30 s) go to every player and the
//!   station announcements news channel.
//! - Every act is checked here at the moment of effect, and the quick reply again when it is
//!   answered: power, panel, the actor's reach and consciousness, the hack for extreme priority,
//!   known departments and request types.
// shortcut: no RETA (`code/modules/reta`, BandaStation's temporary emergency access: the calls
// use tg's path without it), ore updates (no ore redemption reports), speech filters on
// announcements (`treat_message`), silicon or ghost use, wall frames or damage; the texts are
// typed in the window's fields, cut to the act limits.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_chat::radio::{
    FREQ_COMMAND, FREQ_ENGINEERING, FREQ_MEDICAL, FREQ_SCIENCE, FREQ_SECURITY, FREQ_SUPPLY,
};
use ss13_mapload::fidelity::is_under;
use ss13_mapload::wallmount::REQUESTS_HELPER_ROOT;
use ss13_mapload::{DmItem, DmName, DmWallMount};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, PromptSystems};
use ss13_tgui::server::{
    AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems, UiWindow,
};
use ss13_tgui::{UiInterface as _, UiStatus};

use super::{Look, light_of, refresh_look};
use crate::health::{Health, Stat};
use crate::inventory::{IdCard, Inventory};
use crate::power::Unpowered;
use crate::requests_console::*;
use crate::sound::{PlaySound, playsound};
use crate::tools::{ToolKind, tell, tool_of};

/// `ANNOUNCEMENT_COOLDOWN_TIME`.
const ANNOUNCEMENT_COOLDOWN: f64 = 30.0;
/// `addtimer(CALLBACK(src, PROC_REF(clear_emergency)), 5 MINUTES)`.
const EMERGENCY_SECS: f64 = 300.0;
/// `MAX_MESSAGE_LEN`.
const MAX_MESSAGE_LEN: usize = 1024;
const ACCESS_RC_ANNOUNCE: &str = "rc_announce";
const STAMP: &str = "/obj/item/stamp";
const SOUND_SENT: &str = "sound/machines/beep/twobeep.ogg";
const SOUND_ERROR: &str = "sound/machines/buzz/buzz-two.ogg";
const SOUND_RECEIVED: &str = "sound/machines/beep/twobeep_high.ogg";
const SOUND_ANNOUNCE: &str = "sound/announcer/announcement/announce_dig.ogg";
const CROWBAR_SOUND: &str = "sound/items/tools/crowbar.ogg";
const SCREWDRIVER_SOUND: &str = "sound/items/tools/screwdriver.ogg";
/// `set_light(1.5, 0.7, "#34D352")`.
const LIGHT_COLOR: [u8; 3] = [0x34, 0xD3, 0x52];

/// One received message (`/datum/request_message`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConsoleMessage {
    pub sender: String,
    pub time: String,
    pub content: String,
    pub verified_by: String,
    pub stamped_by: String,
    pub priority: u8,
    pub request_type: String,
    /// `notify_channel`: the radio frequency the AAS relays it on.
    pub radio: Option<u16>,
}

/// A requests console's state (tg machine vars).
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct RequestsConsole {
    pub department: String,
    /// Newest first.
    pub messages: Vec<ConsoleMessage>,
    pub new_message_priority: u8,
    pub silent: bool,
    pub hack_state: bool,
    pub can_send_announcements: bool,
    /// The maintenance panel is pried open.
    pub open: bool,
    pub announcement_authenticated: bool,
    pub message_verified_by: String,
    pub message_stamped_by: String,
    /// The emergency called from here and when it clears.
    pub emergency: Option<(String, f64)>,
    pub has_mail_send_error: bool,
    pub announcement_ready_at: f64,
    /// `ui_static_data`: the request lists without this department.
    pub targets: RequestsStatic,
}

/// `GLOB.req_console_assistance`, `_supplies`, `_information` and the known departments
/// (`req_console_ckey_departments`), filled at map init.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct RequestsDirectory {
    pub assistance: Vec<String>,
    pub supplies: Vec<String>,
    pub information: Vec<String>,
    pub departments: Vec<String>,
}

/// On a quick reply prompt: the console and the department answered.
#[derive(Component, Clone, Debug)]
struct ReplyPrompt {
    console: Entity,
    recipient: String,
}

pub(super) fn build(app: &mut App) {
    app.init_resource::<RequestsDirectory>()
        .register_ui::<RequestsConsoleUi>()
        .add_systems(
            Update,
            (
                acts.in_set(UiSystems::Handle),
                answers
                    .in_set(UiSystems::Handle)
                    .after(PromptSystems::Answer)
                    .after(acts),
                window_status.in_set(UiSystems::StatusHook),
                clear_emergencies,
            ),
        );
}

/// BYOND `ckey()`: lowercase letters and digits only.
pub fn ckey(text: &str) -> String {
    text.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// `send_message`'s `radio_channel` for a recipient department.
pub fn notify_freq(recipient: &str) -> Option<u16> {
    Some(match ckey(recipient).as_str() {
        "bridge" => FREQ_COMMAND,
        "medbay" => FREQ_MEDICAL,
        "science" => FREQ_SCIENCE,
        "engineering" => FREQ_ENGINEERING,
        "security" => FREQ_SECURITY,
        "cargobay" | "mining" => FREQ_SUPPLY,
        _ => return None,
    })
}

/// `Initialize`'s naming: the department and the console's name.
pub fn department_of(dm: &DmWallMount, map_name: &str, area_name: &str) -> (String, String) {
    let department = if dm.auto_name {
        area_name.to_owned()
    } else if !dm.department.is_empty() {
        dm.department.clone()
    } else if map_name != "requests console" && !map_name.is_empty() {
        map_name.to_owned()
    } else {
        "Unknown".to_owned()
    };
    let name = format!("{department} requests console");
    (department, name)
}

/// `Initialize` of a mapped console; [`finish_init`] applies the helpers.
pub(super) fn add_console(world: &mut World, e: Entity, dm: &DmWallMount, pos: TilePos) {
    let map_name = world
        .get::<DmName>(e)
        .map(|n| n.name.clone())
        .unwrap_or_default();
    let area = crate::bots::area_name(world, pos);
    let (department, name) = department_of(dm, &map_name, &area);
    if let Some(mut n) = world.get_mut::<DmName>(e) {
        n.name = name;
    }
    {
        let mut dir = world.resource_mut::<RequestsDirectory>();
        if !dir.departments.contains(&department) {
            dir.departments.push(department.clone());
        }
    }
    world.entity_mut(e).insert(RequestsConsole {
        department,
        ..default()
    });
}

/// The mapping helpers' `payload` on the console of their tile, then every console's request
/// lists (`ui_static_data`).
pub(super) fn finish_init(world: &mut World, helpers: &[(String, TilePos)]) {
    let mut q = world.query::<(Entity, &RequestsConsole, &TilePos)>();
    let consoles: Vec<(Entity, String, TilePos)> = q
        .iter(world)
        .map(|(e, c, p)| (e, c.department.clone(), *p))
        .collect();
    for (path, pos) in helpers {
        let Some((console, department, _)) = consoles.iter().find(|(_, _, p)| p == pos) else {
            warn!("{path} at {pos:?} finds no requests console");
            continue;
        };
        let kind = path.strip_prefix(REQUESTS_HELPER_ROOT).unwrap_or_default();
        if kind == "/announcement" {
            world
                .get_mut::<RequestsConsole>(*console)
                .expect("listed")
                .can_send_announcements = true;
            continue;
        }
        let mut dir = world.resource_mut::<RequestsDirectory>();
        let list = match kind {
            "/assistance" => &mut dir.assistance,
            "/supplies" => &mut dir.supplies,
            "/information" => &mut dir.information,
            // shortcut: `ore_update` (no ore redemption reports).
            _ => continue,
        };
        if !list.contains(department) {
            list.push(department.clone());
        }
    }
    let dir = world.resource::<RequestsDirectory>().clone();
    for (console, department, _) in consoles {
        let without = |list: &[String]| -> Vec<String> {
            let mut out: Vec<String> = list.iter().filter(|d| **d != department).cloned().collect();
            out.sort();
            out
        };
        world
            .get_mut::<RequestsConsole>(console)
            .expect("listed")
            .targets = RequestsStatic {
            assistance_consoles: without(&dir.assistance),
            supply_consoles: without(&dir.supplies),
            information_consoles: without(&dir.information),
        };
    }
}

pub fn is_helper(path: &str) -> bool {
    is_under(path, REQUESTS_HELPER_ROOT)
}

/// `update_overlays` and `update_appearance` (light).
pub(super) fn look(world: &World, e: Entity, console: &RequestsConsole, powered: bool) -> Look {
    let mut look = Look {
        icon_state: "req_comp_off".to_owned(),
        overlays: Vec::new(),
        light: None,
    };
    if console.open {
        look.overlays.push(("req_comp_open".to_owned(), false));
    }
    if !powered {
        return look;
    }
    look.light = light_of(world, e, 1.5, 0.7, LIGHT_COLOR);
    if console.open {
        return look;
    }
    let screen = if console.emergency.is_some() || console.new_message_priority == PRIORITY_EXTREME
    {
        3
    } else {
        console.new_message_priority.min(PRIORITY_HIGH)
    };
    let state = format!("req_comp{screen}");
    look.overlays.push((state.clone(), false));
    look.overlays.push((state, true));
    look
}

/// tg `examine`: the panel and the hack.
pub(super) fn examine_lines(world: &World, target: Entity, lines: &mut Vec<crate::examine::Line>) {
    let Some(console) = world.get::<RequestsConsole>(target) else {
        return;
    };
    lines.push(if console.open {
        (
            Span::Warning,
            "Панель обслуживания вскрыта, её можно закрыть ломом.".to_owned(),
        )
    } else {
        (
            Span::Notice,
            "Похоже, панель обслуживания можно вскрыть ломом.".to_owned(),
        )
    });
    if console.hack_state {
        lines.push((
            Span::Warning,
            "Похоже, консоль кто-то модифицировал!".to_owned(),
        ));
    }
}

/// `ui_interact`: the window, unless the panel is open or there is no power.
pub(super) fn interact(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    let Some(console) = world.get::<RequestsConsole>(target) else {
        return false;
    };
    if console.open || world.get::<Unpowered>(target).is_some() {
        return true;
    }
    let title = format!("{} Requests Console", console.department);
    world.write_message(OpenUi::new::<RequestsConsoleUi>(client, mob, target, title));
    true
}

/// `crowbar_act`, `screwdriver_act`, `item_interaction` (an ID, a stamp).
pub(super) fn item_interaction(
    world: &mut World,
    client: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if world.get::<RequestsConsole>(target).is_none() {
        return false;
    }
    match tool_of(world, held).map(|t| t.0) {
        Some(ToolKind::Crowbar) => {
            sound(world, target, CROWBAR_SOUND);
            let mut console = world.get_mut::<RequestsConsole>(target).expect("checked");
            console.open = !console.open;
            let text = if console.open {
                "Вы вскрываете панель обслуживания."
            } else {
                "Вы закрываете панель обслуживания."
            };
            tell(world, client, Span::Notice, text);
            refresh_look(world, target);
            return true;
        }
        Some(ToolKind::Screwdriver) => {
            let mut console = world.get_mut::<RequestsConsole>(target).expect("checked");
            if !console.open {
                tell(
                    world,
                    client,
                    Span::Warning,
                    "Сначала нужно вскрыть панель обслуживания!",
                );
                return true;
            }
            console.hack_state = !console.hack_state;
            let text = if console.hack_state {
                "Вы модифицируете проводку."
            } else {
                "Вы возвращаете проводку в исходное состояние."
            };
            tell(world, client, Span::Notice, text);
            sound(world, target, SCREWDRIVER_SOUND);
            refresh_look(world, target);
            return true;
        }
        _ => {}
    }
    if let Some(card) = crate::inventory::get_id(world, held) {
        let registration = world
            .get::<crate::outfit::IdRegistration>(card)
            .map(|r| (r.registered_name.clone(), r.assignment.clone()))
            .unwrap_or_default();
        let authenticated = world
            .get::<IdCard>(card)
            .is_some_and(|c| c.access.iter().any(|a| a == ACCESS_RC_ANNOUNCE));
        let mut console = world.get_mut::<RequestsConsole>(target).expect("checked");
        console.message_verified_by = format!("{} ({})", registration.0, registration.1);
        console.announcement_authenticated = authenticated;
        return true;
    }
    if world
        .get::<DmItem>(held)
        .is_some_and(|i| is_under(&i.path, STAMP))
    {
        let name = crate::examine::name_of(world, held);
        world
            .get_mut::<RequestsConsole>(target)
            .expect("checked")
            .message_stamped_by = name;
        return true;
    }
    false
}

fn sound(world: &mut World, console: Entity, file: &str) {
    if let Some(&pos) = world.get::<TilePos>(console) {
        playsound(world, PlaySound::new(pos, file, 50.0, true));
    }
}

/// An unpowered console is update-only.
fn window_status(
    mut windows: Query<&mut UiWindow>,
    consoles: Query<(&RequestsConsole, Has<Unpowered>)>,
) {
    for mut window in &mut windows {
        if window.interface != RequestsConsoleUi::NAME {
            continue;
        }
        if let Ok((console, unpowered)) = consoles.get(window.source)
            && (unpowered || console.open)
        {
            window.cap_status(UiStatus::Update);
        }
    }
}

impl ServerUi for RequestsConsoleUi {
    type Source = &'static RequestsConsole;
    type UserParam = ();
    type Act = RequestsAct;

    fn static_data(console: &RequestsConsole) -> RequestsStatic {
        console.targets.clone()
    }

    fn data(console: &RequestsConsole, _: Entity, _: &()) -> RequestsData {
        ui_data(console)
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<RequestsAct, UiReject> {
        let text = |key: &str| params.text(key);
        Ok(match action {
            CLEAR_MESSAGE_STATUS => RequestsAct::ClearMessageStatus,
            CLEAR_AUTHENTICATION => RequestsAct::ClearAuthentication,
            TOGGLE_SILENT => RequestsAct::ToggleSilent,
            SET_EMERGENCY => RequestsAct::SetEmergency(text(EMERGENCY)?),
            SEND_ANNOUNCEMENT => RequestsAct::SendAnnouncement(text(MESSAGE)?),
            QUICK_REPLY => RequestsAct::QuickReply(text(REPLY_RECIPIENT)?),
            SEND_MESSAGE => RequestsAct::SendMessage {
                recipient: text(RECIPIENT)?,
                priority: params
                    .get(PRIORITY)
                    .and_then(|p| p.trim().parse::<u8>().ok())
                    .ok_or_else(|| UiReject::BadParams(format!("{PRIORITY}: a number")))?,
                request_type: text(REQUEST_TYPE)?,
                message: text(MESSAGE)?,
            },
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RequestsAct {
    ClearMessageStatus,
    ClearAuthentication,
    ToggleSilent,
    SetEmergency(String),
    SendAnnouncement(String),
    QuickReply(String),
    SendMessage {
        recipient: String,
        priority: u8,
        request_type: String,
        message: String,
    },
}

pub fn ui_data(console: &RequestsConsole) -> RequestsData {
    RequestsData {
        can_send_announcements: console.can_send_announcements,
        department: console.department.clone(),
        emergency: console
            .emergency
            .as_ref()
            .map(|e| e.0.clone())
            .unwrap_or_default(),
        hack_state: console.hack_state,
        new_message_priority: console.new_message_priority,
        silent: console.silent,
        has_mail_send_error: console.has_mail_send_error,
        authentication_data: AuthenticationData {
            message_verified_by: console.message_verified_by.clone(),
            message_stamped_by: console.message_stamped_by.clone(),
            announcement_authenticated: console.announcement_authenticated,
        },
        messages: console
            .messages
            .iter()
            .map(|m| RequestMessage {
                sender_department: m.sender.clone(),
                received_time: m.time.clone(),
                content: m.content.clone(),
                message_verified_by: m.verified_by.clone(),
                message_stamped_by: m.stamped_by.clone(),
                priority: m.priority,
                request_type: m.request_type.clone(),
            })
            .collect(),
    }
}

/// The actor still works this console: powered, panel closed, conscious and in reach.
fn can_use(world: &World, actor: Entity, console: Entity) -> bool {
    let Some(state) = world.get::<RequestsConsole>(console) else {
        return false;
    };
    if state.open || world.get::<Unpowered>(console).is_some() {
        return false;
    }
    let conscious =
        world.get::<Health>(actor).is_none() || world.get::<Stat>(actor) == Some(&Stat::Conscious);
    conscious && crate::storage::in_reach(world, actor, console)
}

/// `reject_bad_text(trim(html_encode(text), MAX_MESSAGE_LEN))`.
fn clean(text: &str) -> Option<String> {
    let trimmed: String = text.trim().chars().take(MAX_MESSAGE_LEN).collect();
    crate::comms_server::reject_bad_text(&trimmed, MAX_MESSAGE_LEN)
}

fn acts(world: &mut World, mut cursor: Local<MessageCursor<UiActed<RequestsConsoleUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, RequestsAct)> = cursor
        .read(world.resource::<Messages<UiActed<RequestsConsoleUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, console, act) in pending {
        if !can_use(world, actor, console) {
            continue;
        }
        match act {
            RequestsAct::ClearMessageStatus => clear_message_status(world, console),
            RequestsAct::ClearAuthentication => {
                let mut c = world.get_mut::<RequestsConsole>(console).expect("checked");
                c.message_stamped_by.clear();
                c.message_verified_by.clear();
                c.announcement_authenticated = false;
            }
            RequestsAct::ToggleSilent => {
                let mut c = world.get_mut::<RequestsConsole>(console).expect("checked");
                c.silent = !c.silent;
            }
            RequestsAct::SetEmergency(kind) => set_emergency(world, actor, console, &kind),
            RequestsAct::SendAnnouncement(text) => {
                send_announcement(world, client, actor, console, &text);
            }
            RequestsAct::QuickReply(recipient) => {
                if !world
                    .resource::<RequestsDirectory>()
                    .departments
                    .contains(&recipient)
                {
                    continue;
                }
                let ask = Ask::new(
                    client,
                    actor,
                    "Awaiting Input",
                    format!("Write a quick reply to {recipient}"),
                    PromptKind::Text {
                        default: String::new(),
                        max_length: MAX_MESSAGE_LEN as u32,
                        multiline: false,
                    },
                );
                let prompt = ss13_tgui::prompt::ask_world(world, ask);
                world
                    .entity_mut(prompt)
                    .insert(ReplyPrompt { console, recipient });
            }
            RequestsAct::SendMessage {
                recipient,
                priority,
                request_type,
                message,
            } => {
                let hacked = world
                    .get::<RequestsConsole>(console)
                    .is_some_and(|c| c.hack_state);
                let priority_ok = matches!(priority, PRIORITY_NORMAL | PRIORITY_HIGH)
                    || (priority == PRIORITY_EXTREME && hacked);
                let type_ok = [ASSISTANCE_REQUEST, SUPPLIES_REQUEST, INFORMATION_REQUEST]
                    .contains(&request_type.as_str());
                let known = world
                    .resource::<RequestsDirectory>()
                    .departments
                    .contains(&recipient);
                if !priority_ok || !type_ok || !known {
                    continue;
                }
                let Some(message) = clean(&message) else {
                    tell(world, client, Span::Warning, "Invalid message.");
                    world
                        .get_mut::<RequestsConsole>(console)
                        .expect("checked")
                        .has_mail_send_error = true;
                    continue;
                };
                send_message(
                    world,
                    console,
                    &recipient,
                    &message,
                    priority,
                    &request_type,
                );
            }
        }
    }
}

/// `clear_message_status`: every console of the department forgets its unread priority.
fn clear_message_status(world: &mut World, console: Entity) {
    let Some(department) = world
        .get::<RequestsConsole>(console)
        .map(|c| c.department.clone())
    else {
        return;
    };
    world
        .get_mut::<RequestsConsole>(console)
        .expect("checked")
        .has_mail_send_error = false;
    let mut q = world.query::<(Entity, &RequestsConsole)>();
    let same: Vec<Entity> = q
        .iter(world)
        .filter(|(_, c)| c.department == department)
        .map(|(e, _)| e)
        .collect();
    for e in same {
        world
            .get_mut::<RequestsConsole>(e)
            .expect("listed")
            .new_message_priority = PRIORITY_NONE;
        refresh_look(world, e);
    }
}

/// `set_emergency` without RETA: the call on its department's channel, for 5 minutes.
fn set_emergency(world: &mut World, actor: Entity, console: Entity, kind: &str) {
    let freq = match kind {
        EMERGENCY_SECURITY => FREQ_SECURITY,
        EMERGENCY_ENGINEERING => FREQ_ENGINEERING,
        EMERGENCY_MEDICAL => FREQ_MEDICAL,
        _ => return,
    };
    let state = world
        .get::<RequestsConsole>(console)
        .cloned()
        .expect("checked");
    if state.emergency.is_some() {
        return;
    }
    let card = world
        .get::<Inventory>(actor)
        .and_then(|i| i.id_card(world))
        .and_then(|c| world.get::<crate::outfit::IdRegistration>(c))
        .map(|r| (r.registered_name.clone(), r.assignment.clone()));
    let used_swipe = card.is_none() && !state.message_verified_by.is_empty();
    let caller = match card {
        Some((name, assignment)) => format!("(Called by {name}, {assignment})"),
        None if !state.message_verified_by.is_empty() => {
            format!("(Last authentication: {})", state.message_verified_by)
        }
        None => "(Identification not provided)".to_owned(),
    };
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut c = world.get_mut::<RequestsConsole>(console).expect("checked");
    if used_swipe {
        c.message_stamped_by.clear();
        c.message_verified_by.clear();
    }
    c.emergency = Some((kind.to_owned(), now + EMERGENCY_SECS));
    let text = emergency_line(kind, &state.department, &caller);
    crate::arrivals::aas_announce_on(world, freq, &text);
    refresh_look(world, console);
}

/// `/datum/aas_config_entry/rc_emergency` without RETA.
pub fn emergency_line(kind: &str, location: &str, caller: &str) -> String {
    let what = match kind {
        EMERGENCY_SECURITY => "SECURITY",
        EMERGENCY_ENGINEERING => "ENGINEERING",
        _ => "MEDICAL",
    };
    format!("{what} EMERGENCY in {location} {caller}!!!")
}

fn send_announcement(
    world: &mut World,
    client: Entity,
    actor: Entity,
    console: Entity,
    text: &str,
) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let state = world
        .get::<RequestsConsole>(console)
        .cloned()
        .expect("checked");
    if state.announcement_ready_at > now {
        tell(
            world,
            client,
            Span::Warning,
            "Intercomms recharging. Please stand by.",
        );
        return;
    }
    if !state.can_send_announcements || !state.announcement_authenticated {
        return;
    }
    let Some(message) = clean(text) else {
        tell(world, client, Span::Warning, "Invalid message.");
        return;
    };
    let title = format!("{} объявляет", state.department);
    crate::security_level::minor_announce_with_sound(world, &title, &message, SOUND_ANNOUNCE);
    crate::newscaster_server::submit_article(
        world,
        crate::newscaster_server::STATION_ANNOUNCEMENTS,
        &message,
        &state.department,
        true,
        true,
    );
    let name = crate::examine::name_of(world, actor);
    info!(
        "requests console: {name} made a station announcement from {}: {message:?}",
        state.department
    );
    let mut c = world.get_mut::<RequestsConsole>(console).expect("checked");
    c.announcement_ready_at = now + ANNOUNCEMENT_COOLDOWN;
    c.announcement_authenticated = false;
}

/// `send_message`: through the message servers to every console of `recipient`.
pub fn send_message(
    world: &mut World,
    console: Entity,
    recipient: &str,
    message: &str,
    priority: u8,
    request_type: &str,
) {
    let Some(state) = world.get::<RequestsConsole>(console).cloned() else {
        return;
    };
    let from = world.get::<TilePos>(console).copied();
    let delivered = crate::telecomms::deliver_message(world, from).unwrap_or(true);
    if delivered {
        let received = ConsoleMessage {
            sender: state.department.clone(),
            time: round_timestamp(world),
            content: message.to_owned(),
            verified_by: state.message_verified_by.clone(),
            stamped_by: state.message_stamped_by.clone(),
            priority,
            request_type: request_type.to_owned(),
            radio: notify_freq(recipient),
        };
        // `/datum/signal/subspace/messaging/rc/broadcast`: every console of the department.
        let target = ckey(recipient);
        let mut q = world.query::<(Entity, &RequestsConsole)>();
        let consoles: Vec<Entity> = q
            .iter(world)
            .filter(|(_, c)| ckey(&c.department) == target)
            .map(|(e, _)| e)
            .collect();
        for e in consoles {
            create_message(world, e, received.clone());
        }
    }
    let mut c = world.get_mut::<RequestsConsole>(console).expect("checked");
    c.has_mail_send_error = !delivered;
    c.message_stamped_by.clear();
    c.message_verified_by.clear();
    if !state.silent {
        sound(
            world,
            console,
            if delivered { SOUND_SENT } else { SOUND_ERROR },
        );
    }
}

/// `create_message`: the new message on `console`.
fn create_message(world: &mut World, console: Entity, message: ConsoleMessage) {
    let department = {
        let mut c = world.get_mut::<RequestsConsole>(console).expect("listed");
        if message.priority == PRIORITY_EXTREME {
            c.silent = false;
        }
        if c.new_message_priority < message.priority {
            c.new_message_priority = message.priority;
        }
        c.messages.insert(0, message.clone());
        c.department.clone()
    };
    refresh_look(world, console);
    if !world
        .get::<RequestsConsole>(console)
        .expect("listed")
        .silent
    {
        sound(world, console, SOUND_RECEIVED);
        crate::atmos_machinery::machine_say(world, console, &alert_of(&message));
    }
    if let Some(freq) = message.radio {
        let text = aas_line(&message, &department);
        crate::arrivals::aas_announce_on(world, freq, &text);
    }
}

/// `/datum/request_message/get_alert`.
pub fn alert_of(message: &ConsoleMessage) -> String {
    let authenticated = if !message.verified_by.is_empty() {
        format!(", Verified by {} (Authenticated)", message.verified_by)
    } else if !message.stamped_by.is_empty() {
        format!(", Stamped by {} (Authenticated)", message.stamped_by)
    } else {
        String::new()
    };
    format!("Message from {}{authenticated}", message.sender)
}

/// `/datum/aas_config_entry/rc_new_message` (chat is plain text: no `<i>`).
pub fn aas_line(message: &ConsoleMessage, receiver: &str) -> String {
    let sender = &message.sender;
    let content = &message.content;
    if !message.verified_by.is_empty() {
        format!(
            "Message from {sender} to {receiver}, Verified by {} (Authenticated): {content}",
            message.verified_by
        )
    } else if !message.stamped_by.is_empty() {
        format!(
            "Message from {sender} to {receiver}, Stamped by {} (Authenticated): {content}",
            message.stamped_by
        )
    } else {
        format!("Message from {sender} to {receiver}: {content}")
    }
}

/// `round_timestamp()`.
fn round_timestamp(world: &World) -> String {
    let secs = crate::modpc::programs::messenger::round_secs(world);
    format!("{:02}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
}

/// The quick replies: `reject_bad_text` of the answer, sent as a normal priority reply; a
/// blank answer is a send error.
fn answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answers: Vec<(Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.actor, a.answer.clone()))
        .collect();
    for (prompt, actor, answer) in answers {
        let Some(ReplyPrompt { console, recipient }) = world.get::<ReplyPrompt>(prompt).cloned()
        else {
            continue;
        };
        if !can_use(world, actor, console) {
            continue;
        }
        let text = match &answer {
            Some(PromptAnswer::Text(t)) => clean(t),
            _ => None,
        };
        let Some(text) = text else {
            world
                .get_mut::<RequestsConsole>(console)
                .expect("checked")
                .has_mail_send_error = true;
            sound(world, console, SOUND_ERROR);
            continue;
        };
        send_message(
            world,
            console,
            &recipient,
            &text,
            PRIORITY_NORMAL,
            REPLY_REQUEST,
        );
    }
}

/// `clear_emergency` when its 5 minutes are up.
fn clear_emergencies(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut q = world.query::<(Entity, &RequestsConsole)>();
    let due: Vec<Entity> = q
        .iter(world)
        .filter(|(_, c)| c.emergency.as_ref().is_some_and(|e| e.1 <= now))
        .map(|(e, _)| e)
        .collect();
    for e in due {
        world
            .get_mut::<RequestsConsole>(e)
            .expect("listed")
            .emergency = None;
        refresh_look(world, e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn naming_follows_initialize() {
        let dm = |department: &str, auto_name: bool| DmWallMount {
            department: department.into(),
            auto_name,
            ..default()
        };
        assert_eq!(
            department_of(
                &dm("Genetics", false),
                "Genetics Requests Console",
                "Genetics Lab"
            ),
            (
                "Genetics".to_owned(),
                "Genetics requests console".to_owned()
            )
        );
        assert_eq!(
            department_of(&dm("", false), "Bar Console", "Bar").0,
            "Bar Console"
        );
        assert_eq!(
            department_of(&dm("", false), "requests console", "Bar").0,
            "Unknown"
        );
        assert_eq!(department_of(&dm("X", true), "", "Bar").0, "Bar");
    }

    #[test]
    fn channels_and_lines() {
        assert_eq!(notify_freq("Medbay"), Some(FREQ_MEDICAL));
        assert_eq!(notify_freq("Cargo Bay"), Some(FREQ_SUPPLY));
        assert_eq!(notify_freq("Genetics"), None);
        let message = ConsoleMessage {
            sender: "Genetics".into(),
            time: "00:01:00".into(),
            content: "Нужен врач".into(),
            verified_by: "Bob (Geneticist)".into(),
            stamped_by: "stamp".into(),
            priority: PRIORITY_HIGH,
            request_type: ASSISTANCE_REQUEST.into(),
            radio: Some(FREQ_MEDICAL),
        };
        assert_eq!(
            alert_of(&message),
            "Message from Genetics, Verified by Bob (Geneticist) (Authenticated)"
        );
        assert_eq!(
            aas_line(&message, "Medbay"),
            "Message from Genetics to Medbay, Verified by Bob (Geneticist) (Authenticated): \
             Нужен врач"
        );
        assert_eq!(
            emergency_line(EMERGENCY_MEDICAL, "Bar", "(Identification not provided)"),
            "MEDICAL EMERGENCY in Bar (Identification not provided)!!!"
        );
    }
}
