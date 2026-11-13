//! BandaStation's AI law change request (server), after
//! `modular_bandastation/ai_law_change/code/ai_law_change.dm`:
//!
//! - The AI's "Request Law Change" verb (Status panel, AI tab) opens `AiLawChangeRequest`:
//!   three laws to draft. Refused while incapacitated, after a rejection, after an approved
//!   request, 10 minutes into the round, or while a request waits.
//! - Submitting (three non-empty laws, each cut to `MAX_LAW_LEN`) tells the admins (`R_ADMIN`)
//!   with three chat links: approve, review, reject.
//! - Review opens the same window to the admin, editable, with the crew notification toggle;
//!   confirming approves the edited laws, rejecting tells the AI "ВАШ ЗАПРОС БЫЛ ОТКЛОНЁН.".
//! - Approval replaces the laws of the core module of the AI's base rack and syncs the rack
//!   (`super::law_rack::set_core_laws`); with the notification on, the crew gets a priority
//!   announcement. One approved request per AI.
// shortcut: the review window has no VV, PM, player panel or follow buttons (they are
// `ss13_admin`'s tools); no window flash or sound for the admins.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_net::chat::server::SendChat;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_tgui::UiInterface as _;
use ss13_tgui::UiStatus;
use ss13_tgui::server::{
    AlwaysState, AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems, UiWindow,
};

use super::ai::{AiCore, AiEye};
use super::notice;
use super::ui::*;
use crate::examine::{ChatLinkClicked, ChatLinkSystems, chat_link};
use crate::health::Stat;

/// `AI_LAW_CHANGE_SET_AMOUNT`.
pub const LAW_AMOUNT: usize = 3;
/// `AI_LAW_CHANGE_TIME_LIMIT`: 10 minutes after the round start.
pub const TIME_LIMIT_SECS: f64 = 600.0;
/// `CONFIG_GET(number/max_law_len)`.
pub const MAX_LAW_LEN: usize = 1024;
/// The verb (`GAME_VERB_DESC(..., "Request Law Change", ...)`).
pub const VERB: &str = "Request Law Change";
const TOPIC: &str = "ai_law_change";
const TITLE: &str = "Запрос на смену законов";
const ANNOUNCE_TITLE: &str = "Отдел работы с искуственным интеллектом";
const ANNOUNCE_TEXT: &str = "В рамках эксперимента по анализу влияния различных сводов законов \
     станционного ИИ на продуктивность смены на объекте, свод законов станционного ИИ был изменён. \
     Пожалуйста, сообщайте о нетипичных или опасных для активов и экипажа объекта поведенческих \
     проявлениях посредством факсимильной связи.";

/// On the AI core: tg `law_change_used`, `law_change_rejected`, `law_change_request`.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct LawChangeState {
    pub used: bool,
    pub rejected: bool,
    pub request: Option<Entity>,
}

/// `/datum/ai_law_change_request`, the window source (a child of the core).
#[derive(Component, Clone, Debug)]
pub struct LawChangeRequest {
    pub requester: Entity,
    pub laws: Vec<String>,
    pub submitted: bool,
    pub reviewing: bool,
    pub notify_crew: bool,
    pub resolved: bool,
}

pub struct LawChangePlugin;

impl Plugin for LawChangePlugin {
    fn build(&self, app: &mut App) {
        app.register_ui::<AiLawChangeUi>().add_systems(
            Update,
            (
                verb.in_set(UiSystems::Handle),
                acts.in_set(UiSystems::Handle),
                link_clicks.after(ChatLinkSystems),
                window_status.in_set(UiSystems::StatusHook),
            ),
        );
    }
}

/// The client's active `R_ADMIN` (kept by `ss13_admin`).
fn is_admin(world: &World, client: Entity) -> bool {
    world
        .get::<crate::votes::VoteRights>(client)
        .is_some_and(|r| r.admin)
}

/// tg `incapacitated` of the AI: dead, stunned or depowered.
fn incapacitated(world: &World, core: Entity) -> bool {
    world.get::<Stat>(core) != Some(&Stat::Conscious)
        || crate::status_effects::incapacitation(world, core).incapacitated
        || super::ai_power::depowered(world, core)
}

fn round_time(world: &World) -> f64 {
    let now = crate::tools::now(world);
    world
        .get_resource::<crate::round::Ticker>()
        .map_or(0.0, |t| t.round_duration(now))
}

/// Why the verb refuses, in the verb's order.
pub fn refusal(state: &LawChangeState, submitted: bool, round_time: f64) -> Option<&'static str> {
    if state.rejected {
        Some("ЗАПРОС БЫЛ ОТКЛОНЁН.")
    } else if state.used {
        Some("КОЛИЧЕСТВО ЗАПРОСОВ ПРЕВЫШЕНО.")
    } else if round_time > TIME_LIMIT_SECS {
        Some("ВРЕМЯ НА ЗАПРОС СМЕНЫ ЗАКОНОВ ИСТЕКЛО.")
    } else if submitted {
        Some("ОЖИДАЙТЕ РАССМОТРЕНИЯ.")
    } else {
        None
    }
}

/// `copytext_char(trim(law), 1, max_law_len + 1)` of each law; `law_request_valid` checks the
/// count and that none is blank.
pub fn clean_laws(laws: &[String]) -> (Vec<String>, bool) {
    let cleaned: Vec<String> = laws
        .iter()
        .map(|l| l.trim().chars().take(MAX_LAW_LEN).collect())
        .collect();
    let valid = cleaned.len() == LAW_AMOUNT && cleaned.iter().all(|l| !l.trim().is_empty());
    (cleaned, valid)
}

/// `request_law_change`.
fn verb(world: &mut World, mut acts: Local<MessageCursor<UiActed<crate::statpanel::StatPanelUi>>>) {
    use crate::statpanel::{AI_TAB, StatPanelHost};
    let pending: Vec<(Entity, Entity)> = acts
        .read(world.resource::<Messages<UiActed<crate::statpanel::StatPanelUi>>>())
        .filter(|a| a.act.tab == AI_TAB && a.act.name == VERB)
        .map(|a| (a.client, a.source))
        .collect();
    for (client, source) in pending {
        if world.get::<StatPanelHost>(source).map(|h| h.client) != Some(client) {
            continue;
        }
        let Some(eye) = world.resource::<crate::server::Players>().mob(client) else {
            continue;
        };
        let Some(core) = world.get::<AiEye>(eye).map(|e| e.ai) else {
            continue;
        };
        if world.get::<AiCore>(core).is_none() || incapacitated(world, core) {
            continue;
        }
        let state = world
            .get::<LawChangeState>(core)
            .copied()
            .unwrap_or_default();
        let submitted = state
            .request
            .and_then(|r| world.get::<LawChangeRequest>(r))
            .is_some_and(|r| r.submitted);
        if let Some(text) = refusal(&state, submitted, round_time(world)) {
            notice(world, client, Span::Warning, text);
            continue;
        }
        let request = match state
            .request
            .filter(|&r| world.get::<LawChangeRequest>(r).is_some())
        {
            Some(r) => r,
            None => {
                let r = world
                    .spawn((
                        LawChangeRequest {
                            requester: core,
                            laws: Vec::new(),
                            submitted: false,
                            reviewing: false,
                            notify_crew: true,
                            resolved: false,
                        },
                        AlwaysState,
                        ChildOf(core),
                    ))
                    .id();
                world.entity_mut(core).insert(LawChangeState {
                    request: Some(r),
                    ..state
                });
                r
            }
        };
        if let Some(mut r) = world.get_mut::<LawChangeRequest>(request) {
            r.reviewing = false;
        }
        world.write_message(OpenUi::new::<AiLawChangeUi>(client, eye, request, TITLE));
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LawChangeAct {
    Submit(Vec<String>),
    Confirm(Vec<String>),
    Reject,
    ToggleNotify,
}

fn laws_param(params: &UiParams) -> Result<Vec<String>, UiReject> {
    (0..LAW_AMOUNT)
        .map(|i| params.text(&law_change_param(i)))
        .collect()
}

impl ServerUi for AiLawChangeUi {
    type Source = &'static LawChangeRequest;
    type UserParam = (
        Query<'static, 'static, &'static Name>,
        Query<'static, 'static, &'static AiCore>,
        Query<'static, 'static, &'static crate::server::PlayerMob>,
        Query<'static, 'static, &'static ss13_net::server::ClientToken>,
    );
    type Act = LawChangeAct;

    fn static_data(_: &LawChangeRequest) {}

    fn data(
        r: &LawChangeRequest,
        _: Entity,
        (names, cores, players, tokens): &bevy::ecs::system::SystemParamItem<
            '_,
            '_,
            Self::UserParam,
        >,
    ) -> AiLawChangeData {
        let key = cores
            .get(r.requester)
            .ok()
            .and_then(|c| players.get(c.eye).ok())
            .and_then(|p| tokens.get(p.client).ok())
            .map(|t| t.0.clone())
            .unwrap_or_default();
        AiLawChangeData {
            reviewing: r.reviewing,
            laws: r.laws.clone(),
            notify_crew: r.notify_crew,
            max_law_length: MAX_LAW_LEN as u32,
            law_amount: LAW_AMOUNT as u32,
            requester_key: key,
            requester_name: names
                .get(r.requester)
                .map(|n| n.as_str().to_owned())
                .unwrap_or_default(),
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<LawChangeAct, UiReject> {
        match action {
            LAW_CHANGE_SUBMIT => laws_param(params).map(LawChangeAct::Submit),
            LAW_CHANGE_CONFIRM => laws_param(params).map(LawChangeAct::Confirm),
            LAW_CHANGE_REJECT => Ok(LawChangeAct::Reject),
            LAW_CHANGE_TOGGLE_NOTIFY => Ok(LawChangeAct::ToggleNotify),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

/// `ui_status`: closed for an incapacitated AI; the AI's own window only while drafting, an
/// admin's only while reviewing.
fn window_status(
    mut windows: Query<&mut UiWindow>,
    requests: Query<&LawChangeRequest>,
    eyes: Query<&AiEye>,
    rights: Query<&crate::votes::VoteRights>,
    cores: Query<(&Stat, Option<&super::ai_power::AiPower>), With<AiCore>>,
) {
    for mut window in &mut windows {
        if window.interface != AiLawChangeUi::NAME {
            continue;
        }
        let Ok(r) = requests.get(window.source) else {
            continue;
        };
        let alive = cores.get(r.requester).is_ok_and(|(stat, power)| {
            *stat == Stat::Conscious && !power.is_some_and(|p| p.incapacitated())
        });
        let requester = eyes.get(window.actor).is_ok_and(|e| e.ai == r.requester);
        let admin = rights.get(window.client).is_ok_and(|r| r.admin);
        let open = alive && ((requester && !r.reviewing) || (r.reviewing && admin && !requester));
        if !open {
            window.cap_status(UiStatus::Close);
        }
    }
}

fn acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<AiLawChangeUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, LawChangeAct)> = acts
        .read(world.resource::<Messages<UiActed<AiLawChangeUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, request, act) in pending {
        let Some(r) = world.get::<LawChangeRequest>(request).cloned() else {
            continue;
        };
        if incapacitated(world, r.requester) {
            continue;
        }
        let by_requester = world
            .get::<AiEye>(actor)
            .is_some_and(|e| e.ai == r.requester);
        let reviewer = r.reviewing && is_admin(world, client);
        match act {
            LawChangeAct::Submit(laws) => {
                if !by_requester || r.submitted {
                    continue;
                }
                if round_time(world) > TIME_LIMIT_SECS {
                    notice(
                        world,
                        client,
                        Span::Warning,
                        "ВРЕМЯ НА ЗАПРОС СМЕНЫ ЗАКОНОВ ИСТЕКЛО.",
                    );
                    continue;
                }
                let (laws, valid) = clean_laws(&laws);
                if let Some(mut req) = world.get_mut::<LawChangeRequest>(request) {
                    req.laws = laws;
                    req.submitted = valid;
                }
                if !valid {
                    notice(world, client, Span::Warning, "ПЕРЕПРОВЕРЬТЕ ВАШ ЗАПРОС.");
                    continue;
                }
                notice(world, client, Span::Notice, "ПОЖАЛУЙСТА, ОЖИДАЙТЕ.");
                notify_admins(world, request);
                close_windows(world, request);
            }
            LawChangeAct::ToggleNotify => {
                if reviewer && let Some(mut req) = world.get_mut::<LawChangeRequest>(request) {
                    req.notify_crew = !req.notify_crew;
                }
            }
            LawChangeAct::Reject => {
                if reviewer {
                    reject(world, client, request);
                    close_windows(world, request);
                }
            }
            LawChangeAct::Confirm(laws) => {
                if !reviewer {
                    continue;
                }
                let (laws, _) = clean_laws(&laws);
                if let Some(mut req) = world.get_mut::<LawChangeRequest>(request) {
                    req.laws = laws;
                }
                approve(world, client, request);
                close_windows(world, request);
            }
        }
    }
}

/// `SStgui.close_uis(src)` / `ui.close()`.
fn close_windows(world: &mut World, request: Entity) {
    let windows: Vec<Entity> = world
        .query::<(Entity, &UiWindow)>()
        .iter(world)
        .filter(|(_, w)| w.source == request && w.interface == AiLawChangeUi::NAME)
        .map(|(e, _)| e)
        .collect();
    for window in windows {
        world.try_despawn(window).ok();
    }
}

fn requester_name(world: &World, core: Entity) -> String {
    super::chassis::name_of(world, core)
}

/// `notify_admins`: the fax-like notice with its three links, per admin.
fn notify_admins(world: &mut World, request: Entity) {
    let Some(core) = world.get::<LawChangeRequest>(request).map(|r| r.requester) else {
        return;
    };
    let who = {
        let key = world
            .get::<AiCore>(core)
            .and_then(|c| crate::tools::client_of(world, c.eye))
            .and_then(|client| world.get::<ss13_net::server::ClientToken>(client))
            .map(|t| t.0.clone())
            .unwrap_or_default();
        format!("{key}/({})", requester_name(world, core))
    };
    let admins: Vec<Entity> = world
        .query::<(Entity, &crate::votes::VoteRights)>()
        .iter(world)
        .filter(|(_, r)| r.admin)
        .map(|(e, _)| e)
        .collect();
    info!("AI law change: {who} requested a law change");
    for admin in admins {
        let bits = request.to_bits();
        let mut link = |action: &str, text: &str| {
            ChatSegment::new(
                Span::Link(chat_link(world, admin, format!("{TOPIC}:{action}:{bits}"))),
                text,
            )
        };
        let segments = vec![
            ChatSegment::new(Span::AdminNotice, "СМЕНА ЗАКОНОВ ИИ: "),
            ChatSegment::new(
                Span::AdminNotice,
                format!("{who} запросил смену законов. ("),
            ),
            link("approve", "Одобрить"),
            ChatSegment::new(Span::AdminNotice, "/"),
            link("review", "Рассмотреть"),
            ChatSegment::new(Span::AdminNotice, "/"),
            link("reject", "Отклонить"),
            ChatSegment::new(Span::AdminNotice, ")"),
        ];
        world.write_message(SendChat {
            clients: vec![admin],
            message: ChatMessage {
                channel: ChatChannel::AdminLog,
                segments,
            },
        });
    }
}

/// `Topic`: the admin's links, `R_ADMIN` checked again.
fn link_clicks(world: &mut World, mut cursor: Local<MessageCursor<ChatLinkClicked>>) {
    let Some(messages) = world.get_resource::<Messages<ChatLinkClicked>>() else {
        return;
    };
    let clicks: Vec<ChatLinkClicked> = cursor.read(messages).cloned().collect();
    for click in clicks {
        let mut parts = click.topic.split(':');
        let (Some(TOPIC), Some(action), Some(bits)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let Some(request) = bits.parse::<u64>().ok().and_then(Entity::try_from_bits) else {
            continue;
        };
        if world.get::<LawChangeRequest>(request).is_none() || !is_admin(world, click.client) {
            continue;
        }
        match action {
            "approve" => approve(world, click.client, request),
            "review" => start_review(world, click.client, request),
            "reject" => reject(world, click.client, request),
            _ => {}
        }
    }
}

fn admin_log(world: &World, admin: Entity, text: &str) {
    let key = world
        .get::<ss13_net::server::ClientToken>(admin)
        .map(|t| t.0.clone())
        .unwrap_or_default();
    info!(target: "admin", "{key} {text}");
}

/// `start_review`: the window opens for the admin, editable.
fn start_review(world: &mut World, admin: Entity, request: Entity) {
    let Some(mut r) = world.get_mut::<LawChangeRequest>(request) else {
        return;
    };
    if !r.submitted || r.resolved {
        return;
    }
    r.reviewing = true;
    admin_log(
        world,
        admin,
        "начал рассматривать запрос на смену законов ИИ.",
    );
    // The AI's own window closes (`ui_status`) while the admin reviews.
    let actor = world
        .resource::<crate::server::Players>()
        .mob(admin)
        .unwrap_or(admin);
    world.write_message(OpenUi::new::<AiLawChangeUi>(admin, actor, request, TITLE));
}

/// `approve`: the laws go to the core module, the crew is told (`notify_crew`), the AI has
/// used its request.
fn approve(world: &mut World, admin: Entity, request: Entity) {
    let Some(mut r) = world.get_mut::<LawChangeRequest>(request) else {
        return;
    };
    if !r.submitted || r.resolved {
        return;
    }
    r.resolved = true;
    r.reviewing = false;
    let (core, notify) = (r.requester, r.notify_crew);
    let laws: Vec<String> = r
        .laws
        .iter()
        .filter(|l| !l.trim().is_empty())
        .cloned()
        .collect();
    if world.get_entity(core).is_ok()
        && super::law_rack::set_core_laws(world, core, &laws)
        && notify
    {
        crate::events::priority_announce(world, ANNOUNCE_TITLE, ANNOUNCE_TEXT);
    }
    if let Some(mut state) = world.get_mut::<LawChangeState>(core) {
        state.used = true;
    }
    let name = requester_name(world, core);
    admin_log(
        world,
        admin,
        &format!("одобрил запрос на смену законов ИИ {name}."),
    );
}

/// `reject`: the AI is told and may not ask again.
fn reject(world: &mut World, admin: Entity, request: Entity) {
    let Some(mut r) = world.get_mut::<LawChangeRequest>(request) else {
        return;
    };
    if !r.submitted || r.resolved {
        return;
    }
    r.resolved = true;
    r.reviewing = false;
    let core = r.requester;
    if let Some(mut state) = world.get_mut::<LawChangeState>(core) {
        state.rejected = true;
    }
    if let Some(eye) = world.get::<AiCore>(core).map(|c| c.eye)
        && let Some(client) = crate::tools::client_of(world, eye)
    {
        notice(world, client, Span::Warning, "ВАШ ЗАПРОС БЫЛ ОТКЛОНЁН.");
    }
    let name = requester_name(world, core);
    admin_log(
        world,
        admin,
        &format!("отклонил запрос на смену законов ИИ {name}."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_verb_refuses_in_order() {
        let fresh = LawChangeState::default();
        assert_eq!(refusal(&fresh, false, 0.0), None);
        assert_eq!(refusal(&fresh, true, 0.0), Some("ОЖИДАЙТЕ РАССМОТРЕНИЯ."));
        assert_eq!(
            refusal(&fresh, false, TIME_LIMIT_SECS + 1.0),
            Some("ВРЕМЯ НА ЗАПРОС СМЕНЫ ЗАКОНОВ ИСТЕКЛО.")
        );
        let used = LawChangeState {
            used: true,
            ..default()
        };
        assert_eq!(
            refusal(&used, true, 0.0),
            Some("КОЛИЧЕСТВО ЗАПРОСОВ ПРЕВЫШЕНО.")
        );
        let rejected = LawChangeState {
            rejected: true,
            used: true,
            ..default()
        };
        assert_eq!(refusal(&rejected, false, 0.0), Some("ЗАПРОС БЫЛ ОТКЛОНЁН."));
    }

    #[test]
    fn laws_are_trimmed_cut_and_all_required() {
        let long = "я".repeat(MAX_LAW_LEN + 5);
        let (laws, valid) = clean_laws(&[" a ".into(), long, "c".into()]);
        assert!(valid);
        assert_eq!(laws[0], "a");
        assert_eq!(laws[1].chars().count(), MAX_LAW_LEN);
        assert!(!clean_laws(&["a".into(), "  ".into(), "c".into()]).1);
        assert!(!clean_laws(&["a".into(), "b".into()]).1);
    }
}
