//! OOC, LOOC and prayers: tg `/client/verb/ooc`, `select_ignore`, `/client/verb/motd`
//! (`code/modules/client/verbs/ooc.dm`), BandaStation's `/client/proc/looc_message`
//! (`modular_bandastation/communication/code/LOOC.dm`), tg `/mob/verb/pray`
//! (`code/modules/admin/verbs/pray.dm`).
//!
//! Every message is hostile until checked, here and not on the client: the sender's rights and
//! mutes, the text (control characters and length), the shared [`SayRates`] counter (admins are
//! exempt, as tg's `handle_spam_prevention` skips holders). The global switches are admin
//! verbs ([`toggle`]); an ignored player's OOC never reaches the client that ignored it.
// shortcut: no `is_ooc_filtered` word lists, no `byond://` server-whitelist, no OOC ban (only the
// global mute), the MOTD is the plain text of `motd.txt`, ignores live in memory (a reconnect
// forgets them; they belong to the preferences' SQL); the prayer's type (chaplain, cult,
// heretic), its icon and deity signal are not ported.

use std::collections::{HashMap, HashSet};

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_chat::badges::{ChatBadges, DonorChatPrefs, MAX_DONATOR_LEVEL, ooc_badged_name};
use ss13_chat::emoji::{Emojis, emoji_parse};
use ss13_chat::server::{Muted, Overhead, OverheadKind, SayRates};
use ss13_game::ghost::Ghost;
use ss13_game::health::Stat;
use ss13_game::lobby::GameState;
use ss13_game::round::Ticker;
use ss13_game::statpanel::{
    OOC_TAB, OOC_VERB_ADMIN_NOTICE, OOC_VERB_IGNORE, OOC_VERB_MOTD, OOC_VERB_UNIGNORE,
    StatPanelHost, StatPanelUi,
};
use ss13_mapload::Opaque;
use ss13_net::server::{ClientConnected, ClientToken, ControlledEntity, IntentReceived};
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, MAX_MESSAGE_LEN, Span};
use ss13_shared::protocol::ClientIntent;
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, PromptSystems, ask_world};
use ss13_tgui::server::{UiActed, UiSystems};
use ss13_world::SpatialIndex;

use crate::server::{
    AdminConfig, AdminSession, admin_action, admins_with, chat, client_ckey, log_admin,
    sanitize_text, tell,
};
use ss13_shared::admin::rights::{R_ADMIN, R_EVERYTHING};

/// `#define LOOC_RANGE 7`.
const LOOC_RANGE: u32 = 7;

/// `GLOB.ooc_allowed`, `GLOB.dooc_allowed`, `GLOB.looc_allowed`.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct OocPolicy {
    pub ooc: bool,
    pub dead_ooc: bool,
    pub looc: bool,
}

impl Default for OocPolicy {
    fn default() -> Self {
        Self {
            ooc: true,
            dead_ooc: true,
            looc: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Switch {
    Ooc,
    DeadOoc,
    Looc,
}

/// `config.motd`: the lines of `motd.txt`.
#[derive(Resource, Clone, Debug, Default)]
pub struct Motd(pub Vec<String>);

/// `GLOB.admin_notice`: shown to everyone who joins, for this round (a new round is a new `App`).
#[derive(Resource, Clone, Debug, Default)]
pub struct AdminNotice(pub String);

/// `prefs.ignoring`, on a connection.
#[derive(Component, Clone, Debug, Default)]
pub struct OocIgnored(pub HashSet<String>);

/// On a prompt: the keys the list offered, ignoring (or forgiving) the picked one.
#[derive(Component, Clone, Debug)]
struct IgnoreAsk {
    keys: Vec<String>,
    add: bool,
}

/// On a prompt: tg's "Meant for OOC?": the message waits for the answer.
#[derive(Component, Clone, Debug)]
struct OocConfirm(String);

pub(crate) fn build(app: &mut App) {
    let motd: Vec<String> = app
        .world()
        .resource::<AdminConfig>()
        .config_dir
        .as_deref()
        .and_then(|dir| ss13_data::read_to_string(&dir.join("motd.txt")).ok())
        .map(|text| text.lines().map(str::to_owned).collect())
        .unwrap_or_default();
    app.insert_resource(Motd(motd))
        .init_resource::<OocPolicy>()
        .init_resource::<AdminNotice>()
        .init_resource::<DonatorLevels>()
        .add_systems(
            Update,
            (
                (send_motd, handle_intents, run_verbs).chain(),
                answers.after(PromptSystems::Answer),
            )
                .in_set(UiSystems::Handle),
        );
}

/// Whether `client` has an active admin rank (tg `holder`, and `R_ADMIN` for the admin OOC
/// style).
fn holder(world: &World, client: Entity) -> Option<&AdminSession> {
    world
        .get::<AdminSession>(client)
        .filter(|s| s.active_rights() != 0)
}

/// tg `toggle_ooc` / `toggle_dooc` / the LOOC switch: the announcement, the admin log.
pub(crate) fn toggle(world: &mut World, key: &str, which: Switch) -> String {
    let mut policy = world.resource_mut::<OocPolicy>();
    let (flag, name) = match which {
        Switch::Ooc => (&mut policy.ooc, "OOC"),
        Switch::DeadOoc => (&mut policy.dead_ooc, "OOC мёртвых"),
        Switch::Looc => (&mut policy.looc, "LOOC"),
    };
    *flag = !*flag;
    let enabled = *flag;
    let state = if enabled {
        "включён"
    } else {
        "выключен"
    };
    if which != Switch::DeadOoc {
        let text = format!("Канал {name} был глобально {state}.");
        let everyone = all_clients(world);
        chat(
            world,
            everyone,
            ChatMessage::simple(ChatChannel::Ooc, Span::Ooc, text),
        );
    }
    log_admin(world, "OOC", &format!("{key} set {name}: {state}"));
    format!("{name}: {state}")
}

/// tg `set_admin_notice`: a new notice goes to everyone online.
pub(crate) fn set_admin_notice(world: &mut World, key: &str, raw: &str) -> String {
    let text = sanitize_text(raw, MAX_MESSAGE_LEN);
    let verb = if text.is_empty() { "removed" } else { "set" };
    admin_action(world, &format!("{key} {verb} the admin notice: {text}"));
    world.resource_mut::<AdminNotice>().0.clone_from(&text);
    if text.is_empty() {
        return "уведомление снято".to_owned();
    }
    let everyone = all_clients(world);
    chat(world, everyone, admin_notice_line(&text));
    "уведомление задано".to_owned()
}

fn admin_notice_line(text: &str) -> ChatMessage {
    ChatMessage::simple(
        ChatChannel::System,
        Span::AdminNotice,
        format!("Уведомление администрации: {text}"),
    )
}

/// tg `/client/verb/admin_notice`.
fn show_admin_notice(world: &mut World, client: Entity) {
    let text = world.resource::<AdminNotice>().0.clone();
    if text.is_empty() {
        tell(
            world,
            client,
            ChatChannel::System,
            Span::Notice,
            "Сейчас нет уведомлений администрации.",
        );
    } else {
        chat(world, vec![client], admin_notice_line(&text));
    }
}

fn all_clients(world: &mut World) -> Vec<Entity> {
    let mut q = world.query_filtered::<Entity, With<ClientToken>>();
    q.iter(world).collect()
}

fn now(world: &World) -> f64 {
    world.resource::<Time<Real>>().elapsed_secs_f64()
}

fn warn_to(world: &mut World, client: Entity, text: &str) {
    tell(world, client, ChatChannel::Warning, Span::Danger, text);
}

fn sender_body(world: &World, client: Entity) -> Option<Entity> {
    world.get::<ControlledEntity>(client).map(|c| c.0)
}

/// tg `mob.stat == DEAD`: a dead body or a ghost.
fn sender_is_dead(world: &World, body: Option<Entity>) -> bool {
    body.is_some_and(|b| {
        world.get::<Ghost>(b).is_some() || world.get::<Stat>(b) == Some(&Stat::Dead)
    })
}

/// Whether `/client/verb/ooc` lets it through, with tg's messages.
fn may_ooc(world: &mut World, client: Entity) -> bool {
    let is_holder = holder(world, client).is_some();
    if is_holder {
        return true;
    }
    let policy = *world.resource::<OocPolicy>();
    if !policy.ooc {
        warn_to(world, client, "OOC глобально заглушён.");
        return false;
    }
    if !policy.dead_ooc && sender_is_dead(world, sender_body(world, client)) {
        warn_to(world, client, "OOC для мёртвых отключён.");
        return false;
    }
    if world.get::<Muted>(client).is_some() {
        warn_to(world, client, "Вы не можете использовать OOC (мут).");
        return false;
    }
    true
}

/// tg's `handle_spam_prevention`: silent over the limit, `true` when it may go on.
fn pass_spam_check(world: &mut World, client: Entity) -> bool {
    if holder(world, client).is_some() {
        return true;
    }
    let at = now(world);
    world.resource_mut::<SayRates>().admit(client, at).is_ok()
}

/// tg's "Meant for OOC?": a message starting like speech after the round started.
fn looks_like_speech(text: &str) -> bool {
    let first_chars: String = text.chars().take(4).collect();
    text.starts_with(['.', ';', ':', '#']) || first_chars.contains("say")
}

/// `/client/verb/ooc`, the checks before the broadcast.
fn ooc_intent(world: &mut World, client: Entity, raw: &str) {
    if !may_ooc(world, client) {
        return;
    }
    let text = sanitize_text(raw, MAX_MESSAGE_LEN);
    if text.is_empty() {
        return;
    }
    let started = matches!(world.resource::<Ticker>().state(), GameState::Playing);
    if started && looks_like_speech(&text) {
        let actor = sender_body(world, client).unwrap_or(client);
        let message = format!(
            "Ваше сообщение \"{text}\" похоже на сказанное по рации или вслух. Сказать это в OOC?"
        );
        let prompt = ask_world(
            world,
            Ask::new(
                client,
                actor,
                "Сказать в OOC?",
                message,
                PromptKind::Alert {
                    buttons: vec!["Да".to_owned(), "Нет".to_owned()],
                    autofocus: true,
                },
            ),
        );
        world.entity_mut(prompt).insert(OocConfirm(text));
        return;
    }
    ooc_broadcast(world, client, &text);
}

/// `/client/verb/ooc`, from the spam check on (the confirmation prompt comes back here).
fn ooc_broadcast(world: &mut World, client: Entity, text: &str) {
    // The mute or the switch may have changed while the alert was open.
    if !may_ooc(world, client) || !pass_spam_check(world, client) {
        return;
    }
    let is_holder = holder(world, client).is_some();
    if !is_holder && text.contains("byond://") {
        let key = client_ckey(world, client);
        tell(
            world,
            client,
            ChatChannel::Warning,
            Span::BoldAnnounce,
            "Рекламировать другие серверы запрещено.",
        );
        log_admin(
            world,
            "OOC",
            &format!("{key} has attempted to advertise in OOC: {text}"),
        );
        return;
    }
    let key = client_ckey(world, client);
    info!(target: "say", "{client} OOC {key}: {text}");
    let span = if holder(world, client).is_some_and(|s| s.active_rights() & R_ADMIN != 0) {
        Span::AdminOoc
    } else {
        Span::Ooc
    };
    let mut segments = vec![ChatSegment::new(span, "OOC: ")];
    segments.extend(ooc_name(world, client, &key, span));
    segments.push(ChatSegment::new(span, ": "));
    segments.extend(emoji_parse(text, span, world.get_resource::<Emojis>()));
    let receivers: Vec<Entity> = all_clients(world)
        .into_iter()
        .filter(|&c| {
            c == client
                || !world
                    .get::<OocIgnored>(c)
                    .is_some_and(|i| i.0.contains(&key))
        })
        .collect();
    chat(
        world,
        receivers,
        ChatMessage {
            channel: ChatChannel::Ooc,
            segments,
        },
    );
}

/// `client.donator_level` by ckey (BandaStation `budget` table).
// shortcut: nothing fills it until the SQL layer is ported; it then loads the levels here.
#[derive(Resource, Clone, Debug, Default)]
pub struct DonatorLevels(pub HashMap<String, u8>);

/// BandaStation `/client/proc/get_donator_level`: the stored level, or the maximum for a holder
/// (deadminned too) of every right (`get_donator_level_from_host`).
// shortcut: no `enable_localhost_rank`/`localhost_donate_tier` (no localhost rank here).
pub fn donator_level(world: &World, client: Entity) -> u8 {
    let ckey = client_ckey(world, client);
    let stored = world
        .get_resource::<DonatorLevels>()
        .and_then(|d| d.0.get(&ckey).copied())
        .unwrap_or(0);
    let host = world
        .get::<AdminSession>(client)
        .is_some_and(|s| s.rights == R_EVERYTHING);
    if host {
        stored.max(MAX_DONATOR_LEVEL)
    } else {
        stored
    }
}

/// BandaStation `get_ooc_badged_name` of `client`: its badges and its key. The staff badge is
/// of the first rank of an active holder (tg `holder?.ranks[1]`; deadminned, there is none).
fn ooc_name(world: &World, client: Entity, key: &str, span: Span) -> Vec<ChatSegment> {
    let prefs = world
        .get::<DonorChatPrefs>(client)
        .copied()
        .unwrap_or_default();
    let rank = holder(world, client).and_then(|s| s.rank.split('+').next());
    ooc_badged_name(
        key,
        donator_level(world, client),
        rank,
        &prefs,
        world.get_resource::<ChatBadges>(),
        span,
    )
}

/// BandaStation `/client/proc/looc_message`.
fn looc_intent(world: &mut World, client: Entity, raw: &str, wall_pierce: bool) {
    let Some(body) = sender_body(world, client) else {
        return;
    };
    let text = sanitize_text(raw, MAX_MESSAGE_LEN);
    if text.is_empty() {
        return;
    }
    if holder(world, client).is_none() {
        if !world.resource::<OocPolicy>().looc {
            warn_to(world, client, "LOOC глобально заглушён.");
            return;
        }
        if !pass_spam_check(world, client) {
            return;
        }
        if text.contains("byond://") {
            tell(
                world,
                client,
                ChatChannel::Warning,
                Span::BoldAnnounce,
                "Рекламировать другие серверы запрещено.",
            );
            return;
        }
        if world.get::<Muted>(client).is_some() {
            warn_to(world, client, "Вы не можете использовать LOOC (мут).");
            return;
        }
    }
    let Some(&at) = world.get::<TilePos>(body) else {
        return;
    };
    let name = world
        .get::<Name>(body)
        .map_or_else(String::new, |n| n.as_str().to_owned());
    let key = client_ckey(world, client);
    let suffix = if wall_pierce {
        " (СКВОЗЬ СТЕНЫ)"
    } else {
        ""
    };
    info!(target: "say", "{client} LOOC{suffix} {key}/{name} at {at:?}: {text}");
    let parsed = emoji_parse(&text, Span::Looc, world.get_resource::<Emojis>());
    let segments_of = |who: &str, prefix: &str| {
        let mut segments = vec![ChatSegment::new(
            Span::Looc,
            format!("{prefix}LOOC{suffix}: {who}: "),
        )];
        segments.extend(parsed.iter().cloned());
        segments
    };
    let (mut players, mut staff) = (Vec::new(), Vec::new());
    let hearers = hearers_of(world, client, at, wall_pierce);
    // tg `isliving(mob)`.
    let speaker_alive = world.get::<Stat>(body).is_some() && world.get::<Ghost>(body).is_none();
    let mut overheads = Vec::new();
    for (hearer_client, hearer_body) in &hearers {
        if speaker_alive {
            overheads.push(*hearer_client);
        }
        if holder(world, *hearer_client).is_some() {
            staff.push(*hearer_client);
        } else if world.get::<Ghost>(*hearer_body).is_none() {
            players.push(*hearer_client);
        }
    }
    let mut messages = vec![(
        players,
        ChatMessage {
            channel: ChatChannel::Ooc,
            segments: segments_of(&name, ""),
        },
    )];
    let admin_name = format!("{key}/{name}");
    let admins: Vec<Entity> = admins_with(world, R_ADMIN);
    let (near, remote): (Vec<Entity>, Vec<Entity>) =
        admins.into_iter().partition(|a| staff.contains(a));
    messages.push((
        near,
        ChatMessage {
            channel: ChatChannel::Ooc,
            segments: segments_of(&admin_name, ""),
        },
    ));
    messages.push((
        remote,
        ChatMessage {
            channel: ChatChannel::Ooc,
            segments: segments_of(&admin_name, "(R)"),
        },
    ));
    for (clients, message) in messages {
        chat(world, clients, message);
    }
    if !speaker_alive {
        return;
    }
    for hearer_client in overheads {
        world.write_message(Overhead {
            client: hearer_client,
            speaker: body,
            name: name.clone(),
            text: text.clone(),
            kind: OverheadKind::Looc,
        });
    }
}

/// `get_hearers_in_view(LOOC_RANGE, mob)` or `get_hearers_in_range`: clients and the bodies they
/// control. `client`'s own client is always included, the way a mob hears itself.
fn hearers_of(
    world: &World,
    client: Entity,
    at: TilePos,
    wall_pierce: bool,
) -> Vec<(Entity, Entity)> {
    let index = world.resource::<SpatialIndex>();
    let seen = (!wall_pierce).then(|| {
        ss13_game::sound::hearing_view(
            at,
            LOOC_RANGE as f32,
            |p| {
                index
                    .on_tile(p)
                    .iter()
                    .any(|&e| world.get::<Opaque>(e).is_some())
            },
            |p| index.in_bounds(p),
        )
    });
    let listeners = world.resource::<ss13_chat::server::Listeners>();
    let mut out: Vec<(Entity, Entity)> = index
        .in_radius(at, LOOC_RANGE)
        .filter(|&e| {
            seen.as_ref()
                .is_none_or(|seen| world.get::<TilePos>(e).is_some_and(|p| seen.contains(p)))
        })
        .filter_map(|e| Some((listeners.client_of(e)?, e)))
        .collect();
    if !out.iter().any(|&(c, _)| c == client)
        && let Some(body) = world.get::<ControlledEntity>(client).map(|c| c.0)
    {
        out.push((client, body));
    }
    out
}

/// tg `/mob/verb/pray`: the words to the admins and back to the sender, through BandaStation's
/// IC filter.
fn pray_intent(world: &mut World, client: Entity, raw: &str) {
    let Some(body) = sender_body(world, client) else {
        return;
    };
    let text = sanitize_text(&ss13_shared::text::ic_filter(raw), MAX_MESSAGE_LEN);
    if text.is_empty() {
        return;
    }
    if world.get::<Muted>(client).is_some() {
        warn_to(world, client, "Вы не можете молиться (мут).");
        return;
    }
    if !pass_spam_check(world, client) {
        return;
    }
    let key = client_ckey(world, client);
    let name = world
        .get::<Name>(body)
        .map_or_else(String::new, |n| n.as_str().to_owned());
    info!(target: "say", "{client} PRAY {key}/{name}: {text}");
    let admins = admins_with(world, R_ADMIN);
    chat(
        world,
        admins,
        ChatMessage::simple(
            ChatChannel::AdminLog,
            Span::AdminNotice,
            format!("МОЛИТВА: {key}/({name}): {text}"),
        ),
    );
    tell(
        world,
        client,
        ChatChannel::Info,
        Span::Info,
        &format!("Вы молитесь богам: \"{text}\""),
    );
}

fn handle_intents(world: &mut World, mut cursor: Local<MessageCursor<IntentReceived>>) {
    let intents: Vec<(Entity, ClientIntent)> = cursor
        .read(world.resource::<Messages<IntentReceived>>())
        .filter(|i| {
            matches!(
                i.intent,
                ClientIntent::Ooc(_) | ClientIntent::Looc { .. } | ClientIntent::Pray(_)
            )
        })
        .map(|i| (i.client, i.intent.clone()))
        .collect();
    for (client, intent) in intents {
        match intent {
            ClientIntent::Ooc(text) => ooc_intent(world, client, &text),
            ClientIntent::Looc { text, wall_pierce } => {
                looc_intent(world, client, &text, wall_pierce);
            }
            ClientIntent::Pray(text) => pray_intent(world, client, &text),
            _ => {}
        }
    }
}

/// The statpanel's OOC tab.
fn run_verbs(world: &mut World, mut acts: Local<MessageCursor<UiActed<StatPanelUi>>>) {
    let pending: Vec<(Entity, Entity, String, String)> = acts
        .read(world.resource::<Messages<UiActed<StatPanelUi>>>())
        .map(|a| (a.client, a.source, a.act.tab.clone(), a.act.name.clone()))
        .collect();
    for (client, source, tab, name) in pending {
        if tab != OOC_TAB || world.get::<StatPanelHost>(source).map(|h| h.client) != Some(client) {
            continue;
        }
        match name.as_str() {
            OOC_VERB_IGNORE => ask_ignore(world, client, true),
            OOC_VERB_UNIGNORE => ask_ignore(world, client, false),
            OOC_VERB_MOTD => show_motd(world, client),
            OOC_VERB_ADMIN_NOTICE => show_admin_notice(world, client),
            _ => {}
        }
    }
}

/// tg `select_ignore` / `select_unignore`: the choice list.
fn ask_ignore(world: &mut World, client: Entity, add: bool) {
    let own = client_ckey(world, client);
    let ignored = world
        .get::<OocIgnored>(client)
        .map(|i| i.0.clone())
        .unwrap_or_default();
    let mut keys: Vec<String> = if add {
        let others = all_clients(world);
        others
            .into_iter()
            .map(|c| client_ckey(world, c))
            .filter(|k| *k != own && !ignored.contains(k))
            .collect()
    } else {
        ignored.into_iter().collect()
    };
    if keys.is_empty() {
        let text = if add {
            "Нет других игроков, которых можно игнорировать!"
        } else {
            "Вы никого не игнорируете!"
        };
        tell(world, client, ChatChannel::Info, Span::Plain, text);
        return;
    }
    keys.sort_unstable();
    let actor = sender_body(world, client).unwrap_or(client);
    let (title, message) = if add {
        ("Игнорировать в OOC", "Кого игнорировать?")
    } else {
        ("Перестать игнорировать", "Кого перестать игнорировать?")
    };
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            actor,
            title,
            message,
            PromptKind::List {
                items: keys.clone(),
                default: None,
            },
        ),
    );
    world.entity_mut(prompt).insert(IgnoreAsk { keys, add });
}

/// tg `/client/verb/motd`: also shown to every new client.
fn show_motd(world: &mut World, client: Entity) {
    let lines = world.resource::<Motd>().0.clone();
    if lines.is_empty() {
        tell(
            world,
            client,
            ChatChannel::System,
            Span::Notice,
            "Сообщение дня не задано.",
        );
        return;
    }
    for line in lines {
        tell(world, client, ChatChannel::System, Span::Plain, &line);
    }
}

fn send_motd(world: &mut World, mut connected: Local<MessageCursor<ClientConnected>>) {
    let joined: Vec<Entity> = connected
        .read(world.resource::<Messages<ClientConnected>>())
        .map(|c| c.client)
        .collect();
    let has_motd = !world.resource::<Motd>().0.is_empty();
    let has_notice = !world.resource::<AdminNotice>().0.is_empty();
    for client in joined {
        if has_motd {
            show_motd(world, client);
        }
        if has_notice {
            show_admin_notice(world, client);
        }
    }
}

fn answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answers: Vec<(Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.answer.clone()))
        .collect();
    for (prompt, client, answer) in answers {
        if let Some(OocConfirm(text)) = world.get::<OocConfirm>(prompt).cloned() {
            if answer == Some(PromptAnswer::Button(0)) {
                ooc_broadcast(world, client, &text);
            }
            continue;
        }
        let Some(ask) = world.get::<IgnoreAsk>(prompt).cloned() else {
            continue;
        };
        let Some(PromptAnswer::Item(i)) = answer else {
            continue;
        };
        let Some(key) = ask.keys.get(i).cloned() else {
            continue;
        };
        apply_ignore(world, client, key, ask.add);
    }
}

/// tg `select_ignore`/`select_unignore` after the choice, with their messages.
fn apply_ignore(world: &mut World, client: Entity, key: String, add: bool) {
    if world.get::<OocIgnored>(client).is_none() {
        world.entity_mut(client).insert(OocIgnored::default());
    }
    let mut ignored = world.get_mut::<OocIgnored>(client).expect("just inserted");
    let text = if add {
        ignored.0.insert(key.clone());
        format!("Теперь вы игнорируете {key} в канале OOC.")
    } else {
        ignored.0.remove(&key);
        format!("Вы больше не игнорируете {key} в канале OOC.")
    };
    tell(world, client, ChatChannel::Info, Span::Plain, &text);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speech_prefixes_are_caught_outside_the_round_they_are_not_special() {
        assert!(looks_like_speech("; привет"));
        assert!(looks_like_speech(":e кто тут"));
        assert!(looks_like_speech("#шепот"));
        assert!(looks_like_speech("say вот"));
        assert!(!looks_like_speech("обычный текст"));
    }

    #[test]
    fn donor_levels_come_from_the_store_or_every_right() {
        let mut world = World::new();
        world.insert_resource(DonatorLevels(HashMap::from([("bob".to_owned(), 2)])));
        world.insert_resource(ChatBadges(HashMap::from([
            ("Tier_5".to_owned(), ss13_shared::IconStateId(1)),
            ("Host".to_owned(), ss13_shared::IconStateId(2)),
        ])));
        let bob = world.spawn(ClientToken("Bob".into())).id();
        let session = AdminSession {
            ckey: "host".into(),
            rank: "Хост+Ментор".into(),
            rights: R_EVERYTHING,
            active: true,
        };
        let host = world.spawn((ClientToken("Host".into()), session)).id();
        assert_eq!(donator_level(&world, bob), 2);
        assert_eq!(donator_level(&world, host), MAX_DONATOR_LEVEL);
        let texts = |segments: Vec<ChatSegment>| -> Vec<String> {
            segments.into_iter().map(|s| s.text).collect()
        };
        assert_eq!(
            texts(ooc_name(&world, host, "host", Span::AdminOoc)),
            [":Tier_5:", ":Host:", "host"]
        );
        // Deadminned: still the top level (tg counts `GLOB.deadmins`), no staff badge.
        world.get_mut::<AdminSession>(host).unwrap().active = false;
        assert_eq!(
            texts(ooc_name(&world, host, "host", Span::Ooc)),
            [":Tier_5:", "host"]
        );
    }
}
