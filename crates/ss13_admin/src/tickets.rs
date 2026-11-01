//! Adminhelp tickets after BandaStation's ticket manager (`modular_bandastation/ticket_manager`:
//! `/datum/help_ticket`, `/datum/ticket_manager`): a player's adminhelp opens a ticket that every
//! admin sees; an admin's reply links that admin to it; from then on the player's adminhelps are
//! replies (one per 5 s); admins close, resolve or reopen it; an idle ticket closes itself after
//! 20 minutes. An admin PM to a player without a ticket opens a PM ticket.
// shortcut: admin tickets only (no mentor tickets or conversion), no Discord webhooks, no
// typing indicator, no ticket history across rounds; the whole ticket list is resent to every
// admin on any change (fine for tens of tickets a round).

use std::collections::BTreeMap;

use bevy::prelude::*;
use ss13_chat::server::Muted;
use ss13_net::admin::server::SendAdminResponse;
use ss13_net::server::ClientToken;
use ss13_shared::admin::rights::R_ADMIN;
use ss13_shared::admin::{AdminResponse, Ticket, TicketMessage, TicketState};
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, MAX_MESSAGE_LEN, Span};

use crate::rights::ckey;
use crate::server::{
    AdminSession, admins_with, chat, client_by_ckey, client_ckey, key_name, log_admin,
    message_admins, now_secs, sanitize_text, tell,
};

/// BandaStation `TICKET_REPLY_COOLDOWN`.
pub const REPLY_COOLDOWN_SECS: f64 = 5.0;
/// BandaStation `TICKET_AUTOCLOSE_TIMER`.
pub const AUTOCLOSE_SECS: f64 = 20.0 * 60.0;

#[derive(Clone, Debug)]
pub struct TicketData {
    pub ticket: Ticket,
    pub admin_replied: bool,
    /// Last message, for the autoclose timer.
    pub last_activity: f64,
}

/// BandaStation `GLOB.ticket_manager`.
#[derive(Resource, Default, Debug)]
pub struct Tickets {
    next_id: u32,
    pub all: BTreeMap<u32, TicketData>,
    /// The player's `current_help_ticket` by ckey.
    current: std::collections::HashMap<String, u32>,
    /// Player reply cooldowns by ckey.
    last_reply: std::collections::HashMap<String, f64>,
    /// Someone has to be sent the tickets again.
    pub dirty: bool,
}

impl Tickets {
    pub fn current(&self, ckey: &str) -> Option<&TicketData> {
        self.current.get(ckey).and_then(|id| self.all.get(id))
    }

    fn open(&mut self, initiator: &str, pm: bool, sender: &str, text: &str, now: f64) -> u32 {
        self.next_id += 1;
        let id = self.next_id;
        self.all.insert(
            id,
            TicketData {
                ticket: Ticket {
                    id,
                    initiator: initiator.to_owned(),
                    state: TicketState::Open,
                    pm,
                    linked_admin: pm.then(|| sender.to_owned()),
                    messages: vec![message(sender, text, now)],
                },
                admin_replied: pm,
                last_activity: now,
            },
        );
        self.current.insert(initiator.to_owned(), id);
        self.dirty = true;
        id
    }
}

fn message(sender: &str, text: &str, now: f64) -> TicketMessage {
    TicketMessage {
        sender: sender.to_owned(),
        text: text.to_owned(),
        time: now as u32,
    }
}

fn pm_message(title: &str, from: &str, text: &str, footer: Option<&str>) -> ChatMessage {
    let mut segments = vec![
        ChatSegment::new(Span::AdminHelp, format!("{title}\n")),
        ChatSegment::new(Span::Bold, format!("{from}\n")),
        ChatSegment::new(Span::AdminHelp, text),
    ];
    if let Some(footer) = footer {
        segments.push(ChatSegment::new(Span::AdminHelp, format!("\n{footer}")));
    }
    ChatMessage {
        channel: ChatChannel::AdminPm,
        segments,
    }
}

/// The other admins see every ticket message as a log line (`span_notice`).
fn log_to_admins(world: &mut World, except: Option<Entity>, prefix: &str, text: &str) {
    let to: Vec<Entity> = admins_with(world, R_ADMIN)
        .into_iter()
        .filter(|&a| Some(a) != except)
        .collect();
    chat(
        world,
        to,
        ChatMessage {
            channel: ChatChannel::AdminPm,
            segments: vec![
                ChatSegment::new(Span::Bold, format!("{prefix} ")),
                ChatSegment::new(Span::Notice, text),
            ],
        },
    );
    log_admin(world, "ADMINPRIVATE", &format!("{prefix} {text}"));
}

/// BandaStation `adminhelp` verb, `help_ui_handler` `create_ticket` and `add_ticket_message`
/// from the ticket's initiator.
pub fn adminhelp(world: &mut World, client: Entity, raw: &str) -> crate::server::AnswerResult {
    if world.get::<Muted>(client).is_some() {
        return Err("Ошибка: Вы не можете создавать тикеты (Заглушен).".into());
    }
    let text = sanitize_text(raw, MAX_MESSAGE_LEN);
    if text.is_empty() {
        return Ok(None);
    }
    let player = client_ckey(world, client);
    let now = now_secs(world);
    let current = world
        .resource::<Tickets>()
        .current(&player)
        .map(|t| t.ticket.id);
    let Some(id) = current else {
        let id = world
            .resource_mut::<Tickets>()
            .open(&player, false, &player, &text, now);
        let from = key_name(world, client);
        log_admin(
            world,
            "ADMINPRIVATE",
            &format!("Ticket #{id}: {from}: {text}"),
        );
        let to = admins_with(world, R_ADMIN);
        chat(
            world,
            to,
            pm_message(&format!("Тикет #{id}"), &from, &text, None),
        );
        tell(
            world,
            client,
            ChatChannel::AdminPm,
            Span::AdminHelp,
            &format!("Тикет #{id} был создан! Ожидайте ответ. Вы можете открыть тикет нажав F1"),
        );
        return Ok(Some(AdminResponse::Done(format!("ticket #{id} opened"))));
    };
    // `can_send_message` for the initiator.
    let (replied, linked, pm) = {
        let t = &world.resource::<Tickets>().all[&id];
        (t.admin_replied, t.ticket.linked_admin.clone(), t.ticket.pm)
    };
    if !replied {
        return Err("На ваш тикет ещё не ответил ни один администратор! Ожидайте ответа.".into());
    }
    let Some(linked) = linked else {
        return Err(
            "У вашего тикета нет привязанного администратора! Ожидайте пока кто-то вам ответит."
                .into(),
        );
    };
    let last = world.resource::<Tickets>().last_reply.get(&player).copied();
    if last.is_some_and(|at| now - at < REPLY_COOLDOWN_SECS) {
        return Err(format!(
            "Слишком быстро! Разрешено писать 1 сообщение в {REPLY_COOLDOWN_SECS} секунд."
        ));
    }
    {
        let mut tickets = world.resource_mut::<Tickets>();
        tickets.last_reply.insert(player.clone(), now);
        let t = tickets.all.get_mut(&id).expect("current ticket");
        t.ticket.messages.push(message(&player, &text, now));
        t.last_activity = now;
        tickets.dirty = true;
    }
    let from = key_name(world, client);
    let admin = client_by_ckey(world, &linked);
    let title = if pm {
        "Ответ на личное сообщение".to_owned()
    } else {
        format!("Ответ на тикет #{id}")
    };
    if let Some(admin) = admin {
        chat(world, vec![admin], pm_message(&title, &from, &text, None));
    }
    let prefix = if pm {
        format!("PM: {from} → {linked}:")
    } else {
        format!("Ticket #{id}: {from} → {linked}:")
    };
    log_to_admins(world, admin, &prefix, &text);
    tell(
        world,
        client,
        ChatChannel::AdminPm,
        Span::Notice,
        &format!("Сообщение в тикет #{id}: {text}"),
    );
    Ok(Some(AdminResponse::Done(format!(
        "replied to ticket #{id}"
    ))))
}

/// An admin's message into a ticket: links the admin (`add_ticket_message` from staff).
pub fn admin_reply(
    world: &mut World,
    admin: Entity,
    id: u32,
    raw: &str,
) -> crate::server::AnswerResult {
    let text = sanitize_text(raw, MAX_MESSAGE_LEN);
    if text.is_empty() {
        return Ok(None);
    }
    let state = world
        .resource::<Tickets>()
        .all
        .get(&id)
        .map(|t| t.ticket.state)
        .ok_or_else(|| format!("Нет тикета #{id}."))?;
    if state != TicketState::Open {
        return Err("Этот тикет уже закрыт! Создайте новый при необходимости.".into());
    }
    let admin_ckey = client_ckey(world, admin);
    let now = now_secs(world);
    let (player, pm) = {
        let mut tickets = world.resource_mut::<Tickets>();
        let t = tickets.all.get_mut(&id).expect("checked");
        t.ticket.linked_admin = Some(admin_ckey.clone());
        t.admin_replied = true;
        t.ticket.messages.push(message(&admin_ckey, &text, now));
        t.last_activity = now;
        let out = (t.ticket.initiator.clone(), t.ticket.pm);
        tickets.dirty = true;
        out
    };
    let admin_key = key_name(world, admin);
    let player_client = client_by_ckey(world, &player);
    let title = if pm {
        "Личное сообщение от администратора".to_owned()
    } else {
        format!("Ответ на тикет #{id}")
    };
    let player_key = match player_client {
        Some(c) => {
            chat(
                world,
                vec![c],
                pm_message(
                    &title,
                    &admin_ckey,
                    &text,
                    Some("Чтобы ответить, откройте тикет клавишей F1."),
                ),
            );
            key_name(world, c)
        }
        None => player.clone(),
    };
    let prefix = if pm {
        format!("PM: {admin_key} → {player_key}:")
    } else {
        format!("Ticket #{id}: {admin_key} → {player_key}:")
    };
    log_to_admins(world, None, &prefix, &text);
    Ok(Some(AdminResponse::Done(format!(
        "replied to ticket #{id}"
    ))))
}

/// Admin PM (BandaStation `open_ticket` / `new /datum/help_ticket(admin = ...)`): into the
/// player's open ticket, else a new PM ticket.
pub fn admin_pm(
    world: &mut World,
    admin: Entity,
    target: &str,
    raw: &str,
) -> crate::server::AnswerResult {
    let player = ckey(target);
    if client_by_ckey(world, &player).is_none() {
        return Err(format!("Игрок {player} не в сети."));
    }
    let current = world
        .resource::<Tickets>()
        .current(&player)
        .map(|t| t.ticket.id);
    if let Some(id) = current {
        return admin_reply(world, admin, id, raw);
    }
    let text = sanitize_text(raw, MAX_MESSAGE_LEN);
    if text.is_empty() {
        return Ok(None);
    }
    let admin_ckey = client_ckey(world, admin);
    let now = now_secs(world);
    let id = world
        .resource_mut::<Tickets>()
        .open(&player, true, &admin_ckey, &text, now);
    let admin_key = key_name(world, admin);
    let player_client = client_by_ckey(world, &player).expect("checked");
    chat(
        world,
        vec![player_client],
        pm_message(
            "Личное сообщение от администратора",
            &admin_ckey,
            &text,
            Some("Чтобы ответить, откройте тикет клавишей F1."),
        ),
    );
    let player_key = key_name(world, player_client);
    message_admins(world, &format!("Тикет #{id} создан"));
    log_to_admins(
        world,
        None,
        &format!("PM: {admin_key} → {player_key}:"),
        &text,
    );
    Ok(Some(AdminResponse::Done(format!("PM ticket #{id} opened"))))
}

/// BandaStation `set_ticket_state`; `admin: None` is the autoclose.
pub fn set_state(
    world: &mut World,
    admin: Option<Entity>,
    id: u32,
    state: TicketState,
) -> crate::server::AnswerResult {
    let (old, player) = world
        .resource::<Tickets>()
        .all
        .get(&id)
        .map(|t| (t.ticket.state, t.ticket.initiator.clone()))
        .ok_or_else(|| format!("Нет тикета #{id}."))?;
    if old == state {
        return Err(format!("Тикет #{id} уже в этом состоянии."));
    }
    let admin_key = admin.map(|a| key_name(world, a));
    let (admin_text, player_text) = if state == TicketState::Open {
        if world.resource::<Tickets>().current.contains_key(&player) {
            return Err(format!("{player} уже имеет открытый тикет!"));
        }
        let now = now_secs(world);
        let mut tickets = world.resource_mut::<Tickets>();
        tickets.current.insert(player.clone(), id);
        if let Some(t) = tickets.all.get_mut(&id) {
            t.last_activity = now;
        }
        (
            format!(
                "{} открыл тикет #{id}!",
                admin_key.clone().unwrap_or_default()
            ),
            format!("Ваш тикет #{id} был снова открыт! Проверьте Тикет Менеджер нажав F1!"),
        )
    } else {
        let mut tickets = world.resource_mut::<Tickets>();
        if tickets.current.get(&player) == Some(&id) {
            tickets.current.remove(&player);
        }
        let closed = state == TicketState::Closed;
        match &admin_key {
            None => (
                format!("Тикет #{id} был автоматически закрыт!"),
                "Ваш тикет был автоматически закрыт! Вы можете создать новый если вам всё ещё \
                 нужна помощь."
                    .to_owned(),
            ),
            Some(key) => (
                format!(
                    "{key} {} тикет #{id}!",
                    if closed { "закрыл" } else { "решил" }
                ),
                format!(
                    "Ваш тикет #{id} был {}!",
                    if closed { "закрыт" } else { "решён" }
                ),
            ),
        }
    };
    {
        let mut tickets = world.resource_mut::<Tickets>();
        if let Some(t) = tickets.all.get_mut(&id) {
            t.ticket.state = state;
        }
        tickets.dirty = true;
    }
    if let Some(player_client) = client_by_ckey(world, &player) {
        tell(
            world,
            player_client,
            ChatChannel::AdminPm,
            Span::AdminHelp,
            &player_text,
        );
    }
    message_admins(world, &admin_text);
    log_admin(world, "ADMINPRIVATE", &admin_text);
    Ok(Some(AdminResponse::Done(admin_text)))
}

/// Idle open tickets close after [`AUTOCLOSE_SECS`] (PM tickets never do). Checked once a
/// second.
pub(crate) fn autoclose(world: &mut World, mut next: Local<f64>) {
    let now = now_secs(world);
    if now < *next {
        return;
    }
    *next = now + 1.0;
    let stale: Vec<u32> = world
        .resource::<Tickets>()
        .all
        .values()
        .filter(|t| {
            t.ticket.state == TicketState::Open
                && !t.ticket.pm
                && now - t.last_activity >= AUTOCLOSE_SECS
        })
        .map(|t| t.ticket.id)
        .collect();
    for id in stale {
        let _ = set_state(world, None, id, TicketState::Closed);
    }
}

/// Sends every admin all tickets and every player their current or last one.
pub(crate) fn push(world: &mut World) {
    world.resource_mut::<Tickets>().dirty = false;
    let mut q = world.query::<(Entity, &ClientToken, Option<&AdminSession>)>();
    let clients: Vec<(Entity, String, bool)> = q
        .iter(world)
        .map(|(e, t, s)| {
            let admin = s.is_some_and(|s| s.active_rights() & R_ADMIN != 0);
            (e, ckey(&t.0), admin)
        })
        .collect();
    let tickets = world.resource::<Tickets>();
    let all: Vec<Ticket> = tickets.all.values().map(|t| t.ticket.clone()).collect();
    let mut out = Vec::new();
    for (client, ckey, admin) in clients {
        let mine: Vec<Ticket> = if admin {
            all.clone()
        } else {
            all.iter()
                .rev()
                .find(|t| t.initiator == ckey)
                .cloned()
                .into_iter()
                .collect()
        };
        // A player who never had a ticket gets nothing.
        if admin || !mine.is_empty() {
            out.push(SendAdminResponse {
                client,
                response: AdminResponse::Tickets(mine),
            });
        }
    }
    world.write_message_batch(out);
}
