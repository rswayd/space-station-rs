//! The communications console (server), after tg `/obj/machinery/computer/communications`
//! (`code/game/machinery/computer/communications.dm`) with BandaStation's additions (the ERT
//! request, martial law on Gamma and Epsilon).
//!
//! - Login with an ID that has `ACCESS_COMMAND` (a Syndicate console: `ACCESS_SYNDICATE_LEADER`);
//!   silicons are always authenticated and cannot log out; an emagged console logs anyone in as
//!   "Unknown" with all station access.
//! - Main page: call (reason of at least 12 characters) and recall the emergency shuttle, set the
//!   green or blue alert (captain, swiping a captain's ID), make a priority announcement (captain
//!   or silicon, through a text prompt, 30 s cooldown), message CentCom (or the Syndicate when
//!   emagged), request the nuclear codes or an ERT (captain; 60 s cooldown shared by every
//!   console), toggle emergency maintenance access (three free uses, then 30 s), restore the
//!   routing data of an emagged console.
//! - Message list: the reports and messages CentCom sent (`crate::announce::send_message`),
//!   answered and deleted per console.
//! - Status display page: blank, shuttle, a picture or two lines on the evac displays
//!   (`crate::security_level::displays`).
//! - Requests to CentCom or the Syndicate go to the admins as [`CommsRequest`] (`ss13_admin`).
//!
//! Every act is checked here at the moment of effect: the console's power and connection, the
//! login, the actor's silicon access, the cooldowns, the text limits.
//! - Shuttle purchase page: the emergency shuttle catalog (`crate::shuttle_catalog`).
// shortcut: no messages to other sectors (no cross-server link), no spare ID safe codes
// (no spare ID safe), no `treat_message` speech modifiers or mute check on announcements, no
// console hacking by antagonists (no ninja or other caller), no battlecruiser emag.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmName, DmObj};
use ss13_net::chat::server::SendChat;
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, ChatMessage, Span};
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, PromptSystems};
use ss13_tgui::server::{
    AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems, UiWindow,
};
use ss13_tgui::{UiInterface as _, UiStatus};

use crate::announce::{CommsController, Priority};
use crate::click::{ClickResolved, ResolveClicks};
use crate::comms::*;
use crate::health::{Health, Stat};
use crate::inventory::{IdCard, Inventory, check_access};
use crate::outfit::IdRegistration;
use crate::power::Unpowered;
use crate::round::Ticker;
use crate::security_level::displays::StatusCommand;
use crate::security_level::{SecLevel, SecurityLevel, StationAccess};
use crate::shuttle::{
    ACCESS_CAPTAIN, ACCESS_COMMAND, CALL_SHUTTLE_REASON_LENGTH, EmergencyShuttle, ShuttleConfig,
    ShuttleMode,
};
use crate::silicon::interact::{SiliconUsers, has_silicon_access};

/// tg `IMPORTANT_ACTION_COOLDOWN`, `EMERGENCY_ACCESS_COOLDOWN`.
pub const IMPORTANT_ACTION_COOLDOWN: f64 = 60.0;
pub const EMERGENCY_ACCESS_COOLDOWN: f64 = 30.0;
/// `toggle_max_uses`.
const TOGGLE_MAX_USES: u32 = 3;
/// tg `MAX_MESSAGE_LEN`, `MAX_STATUS_LINE_LENGTH`.
pub const MAX_MESSAGE_LEN: usize = 1024;
pub const MAX_STATUS_LINE_LENGTH: usize = 40;
/// tg `ACCESS_SYNDICATE_LEADER`, the Syndicate console's `req_access`.
pub const ACCESS_SYNDICATE_LEADER: &str = "syndicate_leader";
const COMMS_CONSOLE: &str = "/obj/machinery/computer/communications";
const COMMS_SYNDICATE: &str = "/obj/machinery/computer/communications/syndicate";
const PROMPT_DENY: &str = "sound/machines/terminal/terminal_prompt_deny.ogg";
const PROMPT_CONFIRM: &str = "sound/machines/terminal/terminal_prompt_confirm.ogg";
const PROMPT: &str = "sound/machines/terminal/terminal_prompt.ogg";
const TERMINAL_ALERT: &str = "sound/machines/terminal/terminal_alert.ogg";
const TERMINAL_ON: &str = "sound/machines/terminal/terminal_on.ogg";
const TERMINAL_OFF: &str = "sound/machines/terminal/terminal_off.ogg";

/// A communications console (tg vars).
#[derive(Component, Clone, Debug, Default)]
pub struct CommsConsole {
    pub authenticated: bool,
    pub authorize_name: Option<String>,
    pub authorize_access: Vec<String>,
    /// `/obj/machinery/computer/communications/syndicate`.
    pub syndicate: bool,
    /// `obj_flags & EMAGGED`.
    pub emagged: bool,
    /// `state`, `cyborg_state`: one of [`PAGES`].
    pub state: &'static str,
    pub cyborg_state: &'static str,
    pub messages: Vec<CommsMessage>,
    pub toggle_uses: u32,
    pub last_toggled: f64,
}

impl CommsConsole {
    /// `authenticated(user)`.
    pub fn authenticated(&self, silicon: bool) -> bool {
        silicon || self.authenticated
    }

    /// `authenticated_as_silicon_or_captain(user)` (never on a Syndicate console).
    pub fn silicon_or_captain(&self, silicon: bool) -> bool {
        !self.syndicate && (silicon || self.authorize_access.iter().any(|a| a == ACCESS_CAPTAIN))
    }

    /// `authenticated_as_non_silicon_captain(user)`.
    pub fn non_silicon_captain(&self, silicon: bool) -> bool {
        !silicon && self.authorize_access.iter().any(|a| a == ACCESS_CAPTAIN)
    }

    /// tg `authenticated_as_silicon_or_captain` for a non-silicon.
    pub fn is_captain(&self) -> bool {
        self.silicon_or_captain(false)
    }

    fn page(&self, silicon: bool) -> &'static str {
        let page = if silicon {
            self.cyborg_state
        } else {
            self.state
        };
        if page.is_empty() { PAGE_MAIN } else { page }
    }
}

/// What a console sent to CentCom (or the Syndicate) for the admins: tg `message_centcom`,
/// `message_syndicate`, `nuke_request`, BandaStation `ert_request`.
#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct CommsRequest {
    pub kind: RequestKind,
    pub client: Entity,
    pub sender: String,
    pub text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestKind {
    Centcom,
    Syndicate,
    NukeCodes,
    Ert,
}

impl RequestKind {
    /// The admin message prefix.
    pub fn label(self) -> &'static str {
        match self {
            Self::Centcom => "CENTCOM",
            Self::Syndicate => "SYNDICATE",
            Self::NukeCodes => "NUKE CODE REQUEST",
            Self::Ert => "ERT REQUEST",
        }
    }
}

/// On a priority announcement prompt: the console it was asked from.
#[derive(Component, Clone, Copy, Debug)]
struct AnnouncementFor(Entity);

pub struct CommsServerPlugin;

impl Plugin for CommsServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(crate::newscaster_server::NewscasterPlugin)
            .add_message::<CommsRequest>()
            .register_ui::<CommsConsoleUi>()
            .add_systems(ss13_world::MapInit, setup_consoles)
            .add_systems(
                Update,
                (
                    open_console
                        .after(ss13_world::WorldSystems::Movement)
                        .after(ResolveClicks)
                        .before(UiSystems::Open),
                    console_acts.in_set(UiSystems::Handle),
                    announcement_answers
                        .in_set(UiSystems::Handle)
                        .after(PromptSystems::Answer),
                    unpowered_windows.in_set(UiSystems::StatusHook),
                ),
            );
    }
}

fn setup_consoles(world: &mut World) {
    let mut query = world.query_filtered::<(Entity, &DmObj), With<ss13_world::Mapped>>();
    let consoles: Vec<(Entity, bool)> = query
        .iter(world)
        .filter(|(_, o)| is_under(&o.path, COMMS_CONSOLE))
        .map(|(e, o)| (e, is_under(&o.path, COMMS_SYNDICATE)))
        .collect();
    info!("{} communications consoles", consoles.len());
    for (e, syndicate) in consoles {
        world.entity_mut(e).insert(CommsConsole {
            syndicate,
            ..default()
        });
    }
}

/// tg `attack_hand` (empty hand) or `item_interaction` with an ID card: opens the window.
fn open_console(
    mut clicks: MessageReader<ClickResolved>,
    consoles: Query<(), (With<CommsConsole>, Without<Unpowered>)>,
    inventories: Query<&Inventory>,
    cards: Query<(), With<IdCard>>,
    mut open: MessageWriter<OpenUi>,
    names: Query<&DmName>,
) {
    for click in clicks.read() {
        if !consoles.contains(click.target) {
            continue;
        }
        let Ok(inventory) = inventories.get(click.mob) else {
            continue;
        };
        if inventory
            .active_item()
            .is_none_or(|item| cards.contains(item))
        {
            let title = names
                .get(click.target)
                .map(|n| crate::vending::plain_name(&n.name).to_owned())
                .unwrap_or_default();
            open.write(OpenUi::new::<CommsConsoleUi>(
                click.client,
                click.mob,
                click.target,
                title,
            ));
        }
    }
}

/// tg `attack_ai` / `attack_robot`: the window for a silicon (`crate::silicon::interact`).
pub fn open(world: &mut World, client: Entity, user: Entity, console: Entity) {
    if world.get::<Unpowered>(console).is_some() {
        return;
    }
    let title = world
        .get::<DmName>(console)
        .map(|n| crate::vending::plain_name(&n.name).to_owned())
        .unwrap_or_default();
    world.write_message(OpenUi::new::<CommsConsoleUi>(client, user, console, title));
}

/// An unpowered console is update-only (`can_interact` fails).
fn unpowered_windows(
    mut windows: Query<&mut UiWindow>,
    unpowered: Query<(), (With<CommsConsole>, With<Unpowered>)>,
) {
    for mut window in &mut windows {
        if window.interface == CommsConsoleUi::NAME && unpowered.contains(window.source) {
            window.cap_status(UiStatus::Update);
        }
    }
}

/// tg `is_station_level(z)`.
pub fn on_station_level(world: &World, pos: TilePos) -> bool {
    crate::events::controller::station_levels(world).contains(&pos.z)
}

/// tg `has_communication()`: a station or CentCom level, or a Syndicate console anywhere.
pub fn has_communication(world: &World, console: Entity) -> bool {
    if world
        .get::<CommsConsole>(console)
        .is_some_and(|c| c.syndicate)
    {
        return true;
    }
    let Some(&pos) = world.get::<TilePos>(console) else {
        return false;
    };
    on_station_level(world, pos)
        || world
            .get_resource::<EmergencyShuttle>()
            .is_some_and(|s| s.centcom_z == pos.z)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommsAct {
    ToggleAuthentication,
    CallShuttle(String),
    RecallShuttle,
    ChangeSecurityLevel(SecLevel),
    MakePriorityAnnouncement,
    MessageAssociates(String),
    RequestNukeCodes(String),
    RequestErt(String),
    RestoreBackupRoutingData,
    ToggleEmergencyAccess,
    SetState(&'static str),
    SetStatusMessage(String, String),
    SetStatusPicture(&'static str),
    /// 1-based message and answer.
    AnswerMessage(usize, u32),
    DeleteMessage(usize),
    /// A catalog id (`crate::shuttle_catalog::CATALOG`).
    PurchaseShuttle(String),
}

type ConsoleParams = (
    Option<Res<'static, EmergencyShuttle>>,
    Res<'static, Time>,
    Res<'static, Ticker>,
    Res<'static, ShuttleConfig>,
    Res<'static, SecurityLevel>,
    Res<'static, CommsController>,
    Res<'static, StationAccess>,
    SiliconUsers<'static, 'static>,
    Option<Res<'static, crate::shuttle_catalog::ShuttleCatalog>>,
    Option<Res<'static, crate::cargo::economy::Economy>>,
);

impl ServerUi for CommsConsoleUi {
    type Source = &'static CommsConsole;
    type UserParam = ConsoleParams;
    type Act = CommsAct;

    fn static_data(_: &CommsConsole) {}

    fn data(
        console: &CommsConsole,
        user: Entity,
        (shuttle, time, ticker, config, level, controller, access, silicons, catalog, economy): &bevy::ecs::system::SystemParamItem<
            '_,
            '_,
            ConsoleParams,
        >,
    ) -> CommsConsoleData {
        let now = time.elapsed_secs_f64();
        let silicon = silicons.contains(user);
        let authenticated = console.authenticated(silicon);
        let captain = console.silicon_or_captain(silicon);
        let (upper_text, lower_text) = controller.last_status_display.clone().unwrap_or_default();
        let mut data = CommsConsoleData {
            authenticated,
            can_log_out: !silicon,
            authorize_name: console.authorize_name.clone(),
            can_recall_shuttles: !silicon,
            call_shuttle_reason_min_length: CALL_SHUTTLE_REASON_LENGTH as u32,
            max_message_length: ss13_tgui::server::MAX_PARAM_LEN as u32,
            alert_level: level.current.name().to_owned(),
            can_set_alert_level: captain.then(|| {
                if silicon {
                    NO_SWIPE_NEEDED
                } else {
                    SWIPE_NEEDED
                }
                .to_owned()
            }),
            emagged: console.emagged || console.syndicate,
            syndicate: console.syndicate,
            page: console.page(silicon).to_owned(),
            can_make_announcement: captain || console.syndicate,
            can_message_associates: console.non_silicon_captain(silicon),
            can_request_nuke: console.non_silicon_captain(silicon),
            can_request_ert: console.non_silicon_captain(silicon),
            can_toggle_emergency_access: captain,
            emergency_access: access.emergency_access,
            important_action_ready: now >= controller.important_action_until,
            messages: console.messages.clone(),
            upper_text,
            lower_text,
            max_status_line_length: MAX_STATUS_LINE_LENGTH as u32,
            ..default()
        };
        match shuttle.as_deref() {
            Some(shuttle) => {
                let state = &shuttle.state;
                data.shuttle_can_evac_or_fail_reason = state
                    .can_evac(ticker.round_duration(now), config.refuel_secs)
                    .err();
                data.shuttle_called =
                    !matches!(state.mode, ShuttleMode::Idle | ShuttleMode::Recall);
                data.shuttle_recallable =
                    state.can_recall(now, config.call_secs, level.current) || console.syndicate;
                data.shuttle_called_previously = state.call_count > 0;
                data.shuttle_last_called = state.last_call_area.clone();
            }
            None => {
                data.shuttle_can_evac_or_fail_reason =
                    Some("Эвакуационный шаттл не найден.".into());
            }
        }
        if console.syndicate {
            data.shuttle_can_evac_or_fail_reason =
                Some("You cannot summon the shuttle from this console!".into());
        }
        if authenticated {
            use crate::shuttle_catalog::{BuyStatus, can_buy_shuttles};
            let mode = shuttle.as_deref().map(|s| s.state.mode);
            match can_buy_shuttles(catalog.as_deref(), mode, console, silicon) {
                BuyStatus::No => {}
                BuyStatus::Refused(why) => {
                    data.can_buy_shuttles = true;
                    data.buy_shuttles_fail_reason = Some(why.to_owned());
                }
                BuyStatus::Yes => data.can_buy_shuttles = true,
            }
            if data.page == PAGE_BUYING_SHUTTLE
                && let Some(catalog) = catalog.as_deref()
            {
                data.shuttles = crate::shuttle_catalog::offers(catalog, console);
                data.budget = economy
                    .as_deref()
                    .and_then(|e| e.department(crate::cargo::economy::ACCOUNT_CAR))
                    .map_or(0, |a| a.balance);
            }
        }
        data
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<CommsAct, UiReject> {
        let text = |key: &str| {
            params
                .get(key)
                .map(str::to_owned)
                .ok_or_else(|| UiReject::BadParams(format!("{key}: missing")))
        };
        let index = |key: &str| {
            params
                .get(key)
                .and_then(|v| v.trim().parse::<usize>().ok())
                .filter(|i| *i >= 1)
                .ok_or_else(|| UiReject::BadParams(format!("{key}: a 1-based index")))
        };
        match action {
            TOGGLE_AUTHENTICATION => Ok(CommsAct::ToggleAuthentication),
            CALL_SHUTTLE => Ok(CommsAct::CallShuttle(text(REASON)?)),
            RECALL_SHUTTLE => Ok(CommsAct::RecallShuttle),
            CHANGE_SECURITY_LEVEL => params
                .get(NEW_SECURITY_LEVEL)
                .and_then(SecLevel::from_name)
                .map(CommsAct::ChangeSecurityLevel)
                .ok_or_else(|| UiReject::BadParams("newSecurityLevel: a level name".into())),
            MAKE_PRIORITY_ANNOUNCEMENT => Ok(CommsAct::MakePriorityAnnouncement),
            MESSAGE_ASSOCIATES => Ok(CommsAct::MessageAssociates(text(MESSAGE)?)),
            REQUEST_NUKE_CODES => Ok(CommsAct::RequestNukeCodes(text(REASON)?)),
            REQUEST_ERT => Ok(CommsAct::RequestErt(text(REASON)?)),
            RESTORE_BACKUP_ROUTING_DATA => Ok(CommsAct::RestoreBackupRoutingData),
            TOGGLE_EMERGENCY_ACCESS => Ok(CommsAct::ToggleEmergencyAccess),
            SET_STATE => params
                .get(STATE)
                .and_then(|s| PAGES.into_iter().find(|p| *p == s))
                .map(CommsAct::SetState)
                .ok_or_else(|| UiReject::BadParams("state: a page".into())),
            SET_STATUS_MESSAGE => Ok(CommsAct::SetStatusMessage(
                params.get(UPPER_TEXT).unwrap_or_default().to_owned(),
                params.get(LOWER_TEXT).unwrap_or_default().to_owned(),
            )),
            SET_STATUS_PICTURE => params
                .get(PICTURE)
                .and_then(|p| STATUS_PICTURES.into_iter().find(|s| *s == p))
                .map(CommsAct::SetStatusPicture)
                .ok_or_else(|| UiReject::BadParams("picture: an approved picture".into())),
            ANSWER_MESSAGE => {
                let answer = index(ANSWER)?;
                Ok(CommsAct::AnswerMessage(index(MESSAGE)?, answer as u32))
            }
            DELETE_MESSAGE => Ok(CommsAct::DeleteMessage(index(MESSAGE)?)),
            PURCHASE_SHUTTLE => Ok(CommsAct::PurchaseShuttle(
                text(SHUTTLE)?.chars().take(64).collect(),
            )),
            // shortcut: sendToOtherSector and requestSafeCodes (see the module).
            _ => Err(UiReject::UnknownAction),
        }
    }
}

/// tg `reject_bad_text(text, max_length)` with BandaStation's Cyrillic: `None` for text too
/// long, outside printable ASCII and Cyrillic, blank, or with `\ < > /`.
pub fn reject_bad_text(text: &str, max_length: usize) -> Option<String> {
    if text.chars().count() > max_length {
        return None;
    }
    let allowed = |c: char| matches!(c, '\u{20}'..='\u{7E}' | '\t' | '\n' | '\u{400}'..='\u{4FF}');
    let bad = |c: char| matches!(c, '\\' | '<' | '>' | '/');
    if !text.chars().all(allowed) || text.chars().any(bad) || text.trim().is_empty() {
        return None;
    }
    Some(text.to_owned())
}

/// tg `trim(html_encode(text), MAX_MESSAGE_LEN)`: chat is not HTML, so trimmed and cut.
fn trim_message(text: &str) -> String {
    text.trim().chars().take(MAX_MESSAGE_LEN).collect()
}

fn tell(world: &mut World, client: Entity, span: Span, text: &str) {
    world.write_message(SendChat {
        clients: vec![client],
        message: ChatMessage::simple(ChatChannel::System, span, text),
    });
}

fn beep(world: &mut World, console: Entity, sound: &str, volume: f32) {
    if let Some(&pos) = world.get::<TilePos>(console) {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, sound, volume, false),
        );
    }
}

/// The user's name as tg prints `[user]`: an AI's eye speaks for its core.
fn user_name(world: &World, user: Entity) -> String {
    let body = world
        .get::<crate::silicon::ai::AiEye>(user)
        .map_or(user, |e| e.ai);
    world
        .get::<Name>(body)
        .map(|n| n.to_string())
        .unwrap_or_else(|| "Unknown".to_owned())
}

/// tg `ui_act`, one exclusive pass: every check runs here, at the moment of effect.
fn console_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<CommsConsoleUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, CommsAct)> = acts
        .read(world.resource::<Messages<UiActed<CommsConsoleUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, console, act) in pending {
        if world.get::<Unpowered>(console).is_some() || !has_communication(world, console) {
            continue;
        }
        let Some(state) = world.get::<CommsConsole>(console).cloned() else {
            continue;
        };
        let silicon = has_silicon_access(world, actor);
        let now = world.resource::<Time>().elapsed_secs_f64();
        let important_ready = now >= world.resource::<CommsController>().important_action_until;
        match act {
            CommsAct::ToggleAuthentication => toggle_authentication(world, actor, console, state),
            CommsAct::CallShuttle(reason) => {
                if !state.authenticated(silicon) || state.syndicate {
                    continue;
                }
                let reason: String = reason.trim().chars().take(MAX_MESSAGE_LEN).collect();
                if reason.chars().count() < CALL_SHUTTLE_REASON_LENGTH {
                    continue;
                }
                request_evac(world, client, actor, &reason);
            }
            CommsAct::RecallShuttle => {
                // AIs cannot recall the shuttle.
                if !state.authenticated(silicon) || silicon || state.syndicate {
                    continue;
                }
                let call_secs = world.resource::<ShuttleConfig>().call_secs;
                let level = crate::security_level::current(world);
                let can = world
                    .get_resource::<EmergencyShuttle>()
                    .is_some_and(|s| s.state.can_recall(now, call_secs, level));
                if can {
                    let origin = actor_area(world, actor);
                    crate::shuttle::recall_shuttle(world, origin);
                    info!("{actor} recalled the emergency shuttle");
                }
            }
            CommsAct::ChangeSecurityLevel(level) => {
                if state.silicon_or_captain(silicon) {
                    change_security_level(world, client, actor, console, level, silicon);
                }
            }
            CommsAct::MakePriorityAnnouncement => {
                if !state.silicon_or_captain(silicon) && !state.syndicate {
                    continue;
                }
                if !world
                    .resource::<CommsController>()
                    .can_announce(now, silicon)
                {
                    tell(
                        world,
                        client,
                        Span::Warning,
                        "Intercomms recharging. Please stand by.",
                    );
                    continue;
                }
                let prompt = ss13_tgui::prompt::ask_world(
                    world,
                    Ask::new(
                        client,
                        actor,
                        "Announcement",
                        "Message to announce to the station crew",
                        PromptKind::Text {
                            default: String::new(),
                            max_length: MAX_MESSAGE_LEN as u32,
                            multiline: true,
                        },
                    ),
                );
                world.entity_mut(prompt).insert(AnnouncementFor(console));
            }
            CommsAct::MessageAssociates(message) => {
                if !state.non_silicon_captain(silicon) || !important_ready {
                    continue;
                }
                beep(world, console, PROMPT_CONFIRM, 50.0);
                let message = trim_message(&message);
                let (kind, reply) = if state.emagged {
                    (
                        RequestKind::Syndicate,
                        "SYSERR @l(19833)of(transmit.dm): !@$ MESSAGE TRANSMITTED TO SYNDICATE \
                         COMMAND.",
                    )
                } else if state.syndicate {
                    (
                        RequestKind::Syndicate,
                        "Message transmitted to Syndicate Command.",
                    )
                } else {
                    (
                        RequestKind::Centcom,
                        "Message transmitted to Central Command.",
                    )
                };
                request(world, kind, client, actor, &message);
                let span = if kind == RequestKind::Centcom {
                    Span::Notice
                } else {
                    Span::Danger
                };
                tell(world, client, span, reply);
                start_important_cooldown(world, now);
            }
            CommsAct::RequestNukeCodes(reason) => {
                if !state.non_silicon_captain(silicon) || !important_ready {
                    continue;
                }
                let reason = trim_message(&reason);
                request(world, RequestKind::NukeCodes, client, actor, &reason);
                tell(world, client, Span::Notice, "Request sent.");
                let name = user_name(world, actor);
                let mut announcement = Priority::new(
                    format!(
                        "{name} запросил коды для запуска механизма ядерного самоуничтожения \
                         станции. В ближайшее время будет отправлено уведомление о \
                         подтверждении или отклонении данного запроса."
                    ),
                    "Запрос кода самоуничтожения станции",
                );
                announcement.sound = crate::announce::report_sound(world);
                announcement.send(world);
                beep(world, console, PROMPT, 50.0);
                start_important_cooldown(world, now);
            }
            CommsAct::RequestErt(reason) => {
                if !state.non_silicon_captain(silicon) || !important_ready {
                    continue;
                }
                let reason = trim_message(&reason);
                request(world, RequestKind::Ert, client, actor, &reason);
                tell(world, client, Span::Notice, "ERT request sent.");
                let name = user_name(world, actor);
                let mut announcement = Priority::new(
                    format!(
                        "Отправлен запрос ОБР. Инициатор: {name}. Запрос принят к рассмотрению."
                    ),
                    format!(
                        "{}: Служба быстрого реагирования",
                        crate::events::COMMAND_NAME
                    ),
                );
                announcement.sound = crate::announce::report_sound(world);
                announcement.send(world);
                beep(world, console, PROMPT, 50.0);
                start_important_cooldown(world, now);
            }
            CommsAct::RestoreBackupRoutingData => {
                if !state.non_silicon_captain(silicon) || !state.emagged {
                    continue;
                }
                tell(world, client, Span::Notice, "Backup routing data restored.");
                beep(world, console, PROMPT_CONFIRM, 50.0);
                world
                    .get_mut::<CommsConsole>(console)
                    .expect("read")
                    .emagged = false;
            }
            CommsAct::ToggleEmergencyAccess => {
                // tg runs the cooldown first, so anyone spent the free uses.
                if !state.silicon_or_captain(silicon)
                    || emergency_access_cooldown(world, client, console, now)
                {
                    continue;
                }
                let on = world.resource::<StationAccess>().emergency_access;
                if on {
                    crate::security_level::revoke_maint_all_access(world);
                } else {
                    crate::security_level::make_maint_all_access(world);
                }
                info!("{actor} toggled emergency maintenance access: {}", !on);
            }
            CommsAct::SetState(page) => {
                let mode = world
                    .get_resource::<EmergencyShuttle>()
                    .map(|s| s.state.mode);
                let can_buy = crate::shuttle_catalog::can_buy_shuttles(
                    world.get_resource::<crate::shuttle_catalog::ShuttleCatalog>(),
                    mode,
                    &state,
                    silicon,
                ) == crate::shuttle_catalog::BuyStatus::Yes;
                if !state.authenticated(silicon) || (page == PAGE_BUYING_SHUTTLE && !can_buy) {
                    continue;
                }
                let mut console = world.get_mut::<CommsConsole>(console).expect("read");
                if silicon {
                    console.cyborg_state = page;
                } else {
                    console.state = page;
                }
            }
            CommsAct::SetStatusMessage(upper, lower) => {
                if !state.authenticated(silicon) || state.syndicate {
                    continue;
                }
                let upper = reject_bad_text(&upper, MAX_STATUS_LINE_LENGTH).unwrap_or_default();
                let lower = reject_bad_text(&lower, MAX_STATUS_LINE_LENGTH).unwrap_or_default();
                info!("{actor} set the status displays to \"{upper} {lower}\"");
                crate::security_level::displays::post_status(
                    world,
                    StatusCommand::Message(upper.clone(), lower.clone()),
                );
                world.resource_mut::<CommsController>().last_status_display = Some((upper, lower));
            }
            CommsAct::SetStatusPicture(picture) => {
                if !state.authenticated(silicon) || state.syndicate {
                    continue;
                }
                let command = match picture {
                    "blank" => StatusCommand::Blank,
                    "shuttle" => StatusCommand::Shuttle,
                    // You cannot set Code Blue display during Code Red and similar.
                    "currentalert" => StatusCommand::Alert(
                        crate::security_level::current(world)
                            .status_display_icon_state()
                            .to_owned(),
                    ),
                    picture => StatusCommand::Alert(picture.to_owned()),
                };
                crate::security_level::displays::post_status(world, command);
            }
            CommsAct::AnswerMessage(message, answer) => {
                if !state.authenticated(silicon) {
                    continue;
                }
                let mut console = world.get_mut::<CommsConsole>(console).expect("read");
                if let Some(m) = console.messages.get_mut(message - 1)
                    && m.answered == 0
                    && (answer as usize) <= m.possible_answers.len()
                {
                    m.answered = answer;
                }
            }
            CommsAct::DeleteMessage(message) => {
                if !state.authenticated(silicon) {
                    continue;
                }
                let mut console = world.get_mut::<CommsConsole>(console).expect("read");
                if message <= console.messages.len() {
                    console.messages.remove(message - 1);
                }
            }
            CommsAct::PurchaseShuttle(id) => {
                if !state.authenticated(silicon) {
                    continue;
                }
                crate::shuttle_catalog::purchase(world, client, actor, console, silicon, &id);
            }
        }
    }
}

fn start_important_cooldown(world: &mut World, now: f64) {
    world
        .resource_mut::<CommsController>()
        .important_action_until = now + IMPORTANT_ACTION_COOLDOWN;
}

/// tg `message_centcom` & co: the request for the admins.
fn request(world: &mut World, kind: RequestKind, client: Entity, actor: Entity, text: &str) {
    let sender = user_name(world, actor);
    info!("{actor} ({sender}) -> {}: {text}", kind.label());
    world.write_message(CommsRequest {
        kind,
        client,
        sender,
        text: text.to_owned(),
    });
}

/// `toggleAuthentication`: log out, or log in as "Unknown" when emagged, or with the actor's
/// ID if it has the console's access.
fn toggle_authentication(world: &mut World, actor: Entity, console: Entity, state: CommsConsole) {
    let mut state = state;
    if state.authorize_name.is_some() {
        state.authenticated = false;
        state.authorize_access.clear();
        state.authorize_name = None;
        world.entity_mut(console).insert(state);
        beep(world, console, TERMINAL_OFF, 50.0);
        return;
    }
    if state.emagged {
        state.authenticated = true;
        state.authorize_access = all_station_access();
        state.authorize_name = Some("Unknown".into());
        beep(world, console, TERMINAL_ALERT, 25.0);
    } else if world.get::<Health>(actor).is_some()
        && let Some((access, name)) = id_card(world, actor)
    {
        let required = if state.syndicate {
            ACCESS_SYNDICATE_LEADER
        } else {
            ACCESS_COMMAND
        };
        if check_access(&[required.to_owned()], &[], &access) {
            state.authenticated = true;
            state.authorize_access = access;
            state.authorize_name = Some(name);
        }
    }
    state.state = PAGE_MAIN;
    world.entity_mut(console).insert(state);
    beep(world, console, TERMINAL_ON, 50.0);
}

fn all_station_access() -> Vec<String> {
    crate::id_access::all_station_access()
        .into_iter()
        .map(str::to_owned)
        .collect()
}

/// tg `emag_act` (from `crate::emag`): the routing circuits scrambled (messages go to the Syndicate, announcements
/// are the Syndicate's); a logged-in console gets all station access. True when it took.
pub fn emag_act(world: &mut World, client: Entity, console: Entity) -> bool {
    let Some(mut state) = world.get_mut::<CommsConsole>(console) else {
        return false;
    };
    if state.syndicate || state.emagged {
        return false;
    }
    state.emagged = true;
    if state.authenticated {
        state.authorize_access = all_station_access();
    }
    tell(world, client, Span::Notice, "routing circuits scrambled");
    beep(world, console, TERMINAL_ALERT, 50.0);
    true
}

/// `emergency_access_cooldown(user)`: true while it refuses. Three toggles are free, the
/// fourth starts the 30 s cooldown; 30 s without a toggle resets the count.
fn emergency_access_cooldown(world: &mut World, client: Entity, console: Entity, now: f64) -> bool {
    let until = world.resource::<CommsController>().emergency_access_until;
    let Some(state) = world.get::<CommsConsole>(console) else {
        return true;
    };
    let (uses, last) = (state.toggle_uses, state.last_toggled);
    if uses == TOGGLE_MAX_USES {
        world
            .get_mut::<CommsConsole>(console)
            .expect("read")
            .toggle_uses += 1;
        world
            .resource_mut::<CommsController>()
            .emergency_access_until = now + EMERGENCY_ACCESS_COOLDOWN;
        tell(
            world,
            client,
            Span::Warning,
            "This was your last free use without cooldown, you will not be able to use this \
             again for 30 seconds.",
        );
        return false;
    }
    if now < until {
        let left = crate::jobs::display_time_text(until - now);
        tell(
            world,
            client,
            Span::Warning,
            &format!("Emergency Access is still in cooldown for {left}!"),
        );
        return true;
    }
    let mut state = world.get_mut::<CommsConsole>(console).expect("read");
    if last + EMERGENCY_ACCESS_COOLDOWN < now {
        state.toggle_uses = 0;
    }
    state.toggle_uses += 1;
    state.last_toggled = now;
    false
}

/// `make_announcement(user)` after the prompt: the console, the login and the reach are checked
/// again (`can_perform_action(src, ALLOW_SILICON_REACH)`), then the controller's cooldown.
fn announcement_answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answers: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, actor, answer) in answers {
        let Some(&AnnouncementFor(console)) = world.get::<AnnouncementFor>(prompt) else {
            continue;
        };
        let Some(PromptAnswer::Text(input)) = answer else {
            continue;
        };
        let input = trim_message(&input);
        let Some(state) = world.get::<CommsConsole>(console).cloned() else {
            continue;
        };
        let silicon = has_silicon_access(world, actor);
        let reach = silicon
            || (world.get::<Stat>(actor) == Some(&Stat::Conscious)
                && crate::storage::in_reach(world, actor, console));
        if input.is_empty()
            || !reach
            || world.get::<Unpowered>(console).is_some()
            || !has_communication(world, console)
            || !(state.silicon_or_captain(silicon) || state.syndicate)
        {
            continue;
        }
        let players = state.syndicate.then(|| syndicate_players(world, console));
        let syndicate = state.syndicate || state.emagged;
        let name = user_name(world, actor);
        if !crate::announce::make_announcement(
            world, actor, &name, silicon, &input, syndicate, players,
        ) {
            tell(
                world,
                client,
                Span::Warning,
                "Intercomms recharging. Please stand by.",
            );
        }
    }
}

/// The Syndicate console's `get_communication_players`: the dead and those on its level.
fn syndicate_players(world: &mut World, console: Entity) -> Vec<Entity> {
    let z = world.get::<TilePos>(console).map(|p| p.z);
    let mut query = world.query::<(
        Entity,
        &crate::server::PlayerMob,
        Option<&TilePos>,
        Option<&Stat>,
        Has<crate::ghost::Ghost>,
    )>();
    query
        .iter(world)
        .filter(|(_, _, pos, stat, ghost)| {
            *ghost || *stat == Some(&Stat::Dead) || pos.map(|p| p.z) == z
        })
        .map(|(e, ..)| e)
        .collect()
}

/// tg `ui_act "changeSecurityLevel"` after `authenticated_as_silicon_or_captain`: a captain's ID
/// swiped in the active hand (silicons need none) sets green or blue, unless CentCom locked the
/// level.
fn change_security_level(
    world: &mut World,
    client: Entity,
    actor: Entity,
    console: Entity,
    level: SecLevel,
    silicon: bool,
) {
    if !silicon {
        let held = world
            .get::<Inventory>(actor)
            .and_then(Inventory::active_item)
            .and_then(|item| crate::inventory::get_id(world, item));
        let Some(card) = held else {
            tell(world, client, Span::Warning, "You need to swipe your ID!");
            beep(world, console, PROMPT_DENY, 50.0);
            return;
        };
        let captain = world
            .get::<IdCard>(card)
            .is_some_and(|c| c.access.iter().any(|a| a == ACCESS_CAPTAIN));
        if !captain {
            tell(
                world,
                client,
                Span::Warning,
                "You are not authorized to do this!",
            );
            beep(world, console, PROMPT_DENY, 50.0);
            return;
        }
    }
    if !matches!(level, SecLevel::Green | SecLevel::Blue) {
        return;
    }
    let current = crate::security_level::current(world);
    if current >= SecLevel::Delta {
        tell(
            world,
            client,
            Span::Warning,
            "Central Command has placed a lock on the alert level due to a doomsday!",
        );
        return;
    }
    if current >= SecLevel::Gamma {
        tell(
            world,
            client,
            Span::Warning,
            "Центральным командованием установлено военное положение. Изменение кода невозможно.",
        );
        return;
    }
    if current == level {
        return;
    }
    crate::security_level::set_level(world, level, true);
    tell(
        world,
        client,
        Span::Notice,
        "Authorization confirmed. Modifying security level.",
    );
    beep(world, console, PROMPT_CONFIRM, 50.0);
    info!("{actor} changed the security level to {}", level.name());
}

/// tg `get_idcard(hand_first = TRUE)`: the access and `"<name> - <assignment>"` of the first ID
/// in the active hand, the other hand, then the ID slot.
fn id_card(world: &World, mob: Entity) -> Option<(Vec<String>, String)> {
    let inventory = world.get::<Inventory>(mob)?;
    [
        inventory.hands[inventory.active],
        inventory.hands[1 - inventory.active],
        inventory.id,
    ]
    .into_iter()
    .flatten()
    .find_map(|item| {
        let item = crate::inventory::get_id(world, item)?;
        let card = world.get::<IdCard>(item)?;
        let name = world
            .get::<IdRegistration>(item)
            .map(|r| format!("{} - {}", r.registered_name, r.assignment))
            .unwrap_or_else(|| "Unknown".into());
        Some((card.access.clone(), name))
    })
}

fn actor_area(world: &World, actor: Entity) -> Option<String> {
    let pos = *world.get::<TilePos>(actor)?;
    let area = world.resource::<ss13_mapload::Areas>().area_at(pos)?;
    Some(crate::shuttle::area_name(world, area))
}

/// tg `SSshuttle.canEvac()`; `None` without an emergency shuttle.
pub(crate) fn can_evac(world: &World) -> Option<Result<(), String>> {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let round = world.resource::<Ticker>().round_duration(now);
    let refuel = world.resource::<ShuttleConfig>().refuel_secs;
    world
        .get_resource::<EmergencyShuttle>()
        .map(|s| s.state.can_evac(round, refuel))
}

/// tg `SSshuttle.requestEvac`, then `post_status("shuttle")`.
pub(crate) fn request_evac(world: &mut World, client: Entity, actor: Entity, reason: &str) {
    let Some(check) = can_evac(world) else {
        return;
    };
    if let Err(why) = check {
        tell(world, client, Span::Warning, &why);
        return;
    }
    if reason.trim().chars().count() < CALL_SHUTTLE_REASON_LENGTH
        && crate::security_level::current(world) > SecLevel::Green
    {
        tell(world, client, Span::Warning, "You must provide a reason.");
        return;
    }
    let origin = actor_area(world, actor);
    if crate::shuttle::call_evac_shuttle(world, reason, origin) {
        info!("{actor} called the emergency shuttle: {reason}");
    }
    crate::security_level::displays::post_status(world, StatusCommand::Shuttle);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_lines_follow_reject_bad_text() {
        assert_eq!(reject_bad_text("ЭВАК", 40).as_deref(), Some("ЭВАК"));
        assert_eq!(
            reject_bad_text("Code 7: all ok", 40).as_deref(),
            Some("Code 7: all ok")
        );
        assert_eq!(
            reject_bad_text(&"я".repeat(40), 40).map(|s| s.len()),
            Some(80)
        );
        assert_eq!(reject_bad_text(&"я".repeat(41), 40), None);
        assert_eq!(reject_bad_text("<b>", 40), None);
        assert_eq!(reject_bad_text("a/b", 40), None);
        assert_eq!(reject_bad_text("   ", 40), None);
        assert_eq!(reject_bad_text("☢", 40), None);
    }

    #[test]
    fn authentication_kinds() {
        let mut c = CommsConsole::default();
        assert!(!c.authenticated(false) && c.authenticated(true));
        assert!(c.silicon_or_captain(true) && !c.non_silicon_captain(true));
        c.authorize_access = vec![ACCESS_CAPTAIN.into()];
        assert!(c.is_captain() && c.non_silicon_captain(false));
        c.syndicate = true;
        assert!(!c.silicon_or_captain(true));
        assert_eq!(c.page(false), PAGE_MAIN);
    }
}
