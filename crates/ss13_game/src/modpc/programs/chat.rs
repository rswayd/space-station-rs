//! NTNet Relay Chat (`/datum/computer_file/program/chatclient`, `/datum/ntnet_conversation`):
//! station-wide channels any chat client can create and join. The creator operates the channel
//! (rename, delete, password, mute); a password channel is joined by saying its password; members
//! ping each other (3 s cooldown) with an alert on the pinged computer. A client goes offline in
//! its channels when the program is closed and comes back when it is started again; minimized,
//! the header shows whether its channel has new messages. With network access (`ACCESS_NETWORK`)
//! the admin mode reads any channel unannounced and operates every one.
// shortcut: no log saving (`PRG_savelog`: no text files on the computer), no admin chat log
// (`log_talk`), no department chat consoles (`/preset/cargochat`).
// tg lets a non-member speak into a password channel after a wrong password (the message falls
// through to `add_message`); here only members speak.

use bevy::prelude::*;
use ss13_net::chat::server::SendChat;
use ss13_shared::chat::{ChatChannel, Span};
use ss13_tgui::server::{UiParams, UiReject};

use super::ProgramKind;
use crate::modpc::pda::ModularComputer;
use crate::modpc::{
    ChatClientData, ChatMember, ChatMessageUi, ChatRoom, PRG_CHANGE_NAME, PRG_DELETE_CHANNEL,
    PRG_JOIN_CHANNEL, PRG_LEAVE_CHANNEL, PRG_MUTE_USER, PRG_NEW_CHANNEL, PRG_PING_USER,
    PRG_RENAME_CHANNEL, PRG_SET_PASSWORD, PRG_SPEAK, PRG_TOGGLE_ADMIN,
};

/// `USERNAME_SIZE`, `CHANNELNAME_SIZE`, `MESSAGE_SIZE`.
pub const USERNAME_SIZE: usize = 32;
pub const CHANNELNAME_SIZE: usize = 18;
pub const MESSAGE_SIZE: usize = 2048;
/// `PING_COOLDOWN_TIME`.
const PING_COOLDOWN: f64 = 3.0;
/// `MAX_CHANNELS`: channel ids past it are not made.
const MAX_CHANNELS: u32 = 1000;
/// `trim_message_list`.
const MAX_MESSAGES: usize = 50;
/// `ACCESS_NETWORK`.
const ACCESS_NETWORK: &str = "network";
const PING_SOUND: &str = "sound/machines/ping.ogg";
const ALERT_SOUND: &str = "sound/machines/beep/twobeep_high.ogg";

/// A `/datum/ntnet_conversation`. Clients are their computers.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Channel {
    pub id: u32,
    pub title: String,
    /// `strong`: cannot be renamed or deleted.
    pub strong: bool,
    /// `password`; empty is none.
    pub password: String,
    /// `messages`: (id, line), oldest first, at most [`MAX_MESSAGES`].
    pub messages: Vec<(u32, String)>,
    pub next_message_id: u32,
    pub operator: Option<Entity>,
    pub active: Vec<Entity>,
    pub offline: Vec<Entity>,
    pub muted: Vec<Entity>,
}

impl Channel {
    pub fn is_member(&self, pc: Entity) -> bool {
        self.active.contains(&pc) || self.offline.contains(&pc)
    }

    fn push(&mut self, line: String) {
        self.messages.push((self.next_message_id, line));
        self.next_message_id += 1;
        if self.messages.len() > MAX_MESSAGES {
            let extra = self.messages.len() - MAX_MESSAGES;
            self.messages.drain(..extra);
        }
    }

    /// `add_message`.
    pub fn add_message(&mut self, ts: &str, username: &str, message: &str) {
        self.push(format!("{ts} {username}: {message}"));
    }

    /// `add_status_message`.
    pub fn add_status(&mut self, ts: &str, message: &str) {
        self.push(format!("{ts} -!- {message}"));
    }

    /// `add_client`: the first member becomes the operator silently.
    pub fn add_client(&mut self, ts: &str, pc: Entity, username: &str, silent: bool) {
        if !self.active.contains(&pc) {
            self.active.push(pc);
        }
        self.offline.retain(|&c| c != pc);
        if !silent {
            self.add_status(ts, &format!("{username} has joined the channel."));
        }
        if self.operator.is_none() {
            self.operator = Some(pc);
        }
    }

    /// `remove_client`: an operator who leaves hands over to an active member (`pick`).
    pub fn remove_client(
        &mut self,
        ts: &str,
        pc: Entity,
        username: &str,
        names: &dyn Fn(Entity) -> String,
        pick: &mut dyn FnMut(usize) -> usize,
    ) {
        if self.active.contains(&pc) {
            self.active.retain(|&c| c != pc);
            self.add_status(ts, &format!("{username} has left the channel."));
        }
        self.muted.retain(|&c| c != pc);
        self.offline.retain(|&c| c != pc);
        if self.operator == Some(pc) {
            self.operator = None;
            if !self.active.is_empty() {
                let new_op = self.active[pick(self.active.len())];
                self.operator = Some(new_op);
                self.add_status(
                    ts,
                    &format!("Channel operator status transferred to {}.", names(new_op)),
                );
            }
        }
    }
}

/// `SSmodular_computers.chat_channels` and `ntnrc_uid`.
#[derive(Resource, Clone, Debug, Default)]
pub struct ChatChannels {
    pub channels: Vec<Channel>,
    pub next_id: u32,
}

impl ChatChannels {
    pub fn get(&self, id: u32) -> Option<&Channel> {
        self.channels.iter().find(|c| c.id == id)
    }

    pub fn get_mut(&mut self, id: u32) -> Option<&mut Channel> {
        self.channels.iter_mut().find(|c| c.id == id)
    }

    /// `new /datum/ntnet_conversation`; `None` past [`MAX_CHANNELS`].
    pub fn create(&mut self, title: &str, strong: bool) -> Option<u32> {
        let id = self.next_id;
        self.next_id += 1;
        if id > MAX_CHANNELS {
            return None;
        }
        self.channels.push(Channel {
            id,
            title: title.to_owned(),
            strong,
            ..default()
        });
        Some(id)
    }
}

/// The program's state on its computer.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct ChatClient {
    pub username: String,
    /// `last_message_id`: the newest message id when the program went to the background.
    pub last_message_id: Option<u32>,
    pub active_channel: Option<u32>,
    pub netadmin: bool,
    /// The ping cooldown's end (`Time::elapsed_secs_f64`).
    pub ping_ready: f64,
    /// The program was running (in front or background) and in front at the last sync, to
    /// catch `on_start`, `kill_program` and `background_program`.
    pub was_running: bool,
    pub was_front: bool,
}

/// `reject_bad_chattext`: no control characters, not only spaces, at most `max` characters.
pub fn reject_bad_chattext(text: &str, max: usize) -> Option<String> {
    if text.chars().count() > max || text.chars().any(|c| (c as u32) < 32) {
        return None;
    }
    text.chars().any(|c| c != ' ').then(|| text.to_owned())
}

/// The acts, well-formed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChatAct {
    Speak(String),
    Join(u32),
    Leave,
    NewChannel(String),
    ToggleAdmin,
    ChangeName(String),
    RenameChannel(String),
    DeleteChannel,
    SetPassword(String),
    Mute(Entity),
    Ping(Entity),
}

/// A member's `ref` (`REF(channel_client)`): its computer.
pub fn member_ref(pc: Entity) -> String {
    pc.to_bits().to_string()
}

fn parse_ref(params: &UiParams) -> Result<Entity, UiReject> {
    params
        .get("ref")
        .and_then(|r| r.parse::<u64>().ok())
        .and_then(Entity::try_from_bits)
        .ok_or_else(|| UiReject::BadParams("ref: expected a member".into()))
}

/// `None` for an action that is not the chat client's.
pub fn parse(action: &str, params: &UiParams) -> Option<Result<ChatAct, UiReject>> {
    let text = |key: &str| params.get(key).unwrap_or_default().to_owned();
    Some(match action {
        PRG_SPEAK => Ok(ChatAct::Speak(text("message"))),
        PRG_JOIN_CHANNEL => params.number("id").and_then(|n| {
            (n >= 0.0 && n <= f64::from(MAX_CHANNELS))
                .then_some(ChatAct::Join(n as u32))
                .ok_or_else(|| UiReject::BadParams("id: out of range".into()))
        }),
        PRG_LEAVE_CHANNEL => Ok(ChatAct::Leave),
        PRG_NEW_CHANNEL => Ok(ChatAct::NewChannel(text("new_channel_name"))),
        PRG_TOGGLE_ADMIN => Ok(ChatAct::ToggleAdmin),
        PRG_CHANGE_NAME => Ok(ChatAct::ChangeName(text("new_name"))),
        PRG_RENAME_CHANNEL => Ok(ChatAct::RenameChannel(text("new_name"))),
        PRG_DELETE_CHANNEL => Ok(ChatAct::DeleteChannel),
        PRG_SET_PASSWORD => Ok(ChatAct::SetPassword(text("new_password"))),
        PRG_MUTE_USER => parse_ref(params).map(ChatAct::Mute),
        PRG_PING_USER => parse_ref(params).map(ChatAct::Ping),
        _ => return None,
    })
}

fn timestamp(world: &World) -> String {
    let secs = super::messenger::round_secs(world);
    format!("{:02}:{:02}", secs / 3600, secs / 60 % 60)
}

fn username(world: &World, pc: Entity) -> String {
    world
        .get::<ChatClient>(pc)
        .map(|c| c.username.clone())
        .unwrap_or_default()
}

fn pick(world: &mut World, n: usize) -> usize {
    let mut rng = world.get_resource_or_insert_with(crate::combat::CombatRng::default);
    rng.range(0, n as u32 - 1) as usize
}

/// `remove_client` of `pc` from channel `id`, with the names and the operator pick it needs.
fn remove_from(world: &mut World, id: u32, pc: Entity) {
    let ts = timestamp(world);
    let name = username(world, pc);
    let Some(channel) = world.resource::<ChatChannels>().get(id) else {
        return;
    };
    let names: Vec<(Entity, String)> = channel
        .active
        .iter()
        .map(|&c| (c, username(world, c)))
        .collect();
    let lookup = move |e: Entity| {
        names
            .iter()
            .find(|(c, _)| *c == e)
            .map(|(_, n)| n.clone())
            .unwrap_or_default()
    };
    let after = channel.active.iter().filter(|&&c| c != pc).count().max(1);
    let roll = pick(world, after);
    let mut picker = move |n: usize| roll.min(n.saturating_sub(1));
    if let Some(channel) = world.resource_mut::<ChatChannels>().get_mut(id) {
        channel.remove_client(&ts, pc, &name, &lookup, &mut picker);
    }
}

/// `alert_call` of the chat client on `pc`: a sound and the notice to everyone who sees it.
fn alert_call(world: &mut World, pc: Entity, text: &str, sound: &str) {
    let Some(at) = crate::modpc::pda::turf_of(world, pc) else {
        return;
    };
    crate::sound::playsound(world, crate::sound::PlaySound::new(at, sound, 50.0, true));
    let name = world
        .get::<ModularComputer>(pc)
        .map(ModularComputer::display_name)
        .unwrap_or_default();
    let line = format!(
        "Монитор {name} показывает уведомление {}: {text}",
        ProgramKind::ChatClient.filedesc()
    );
    for send in
        crate::pull::visible_message(world, at, ChatChannel::Local, (Span::Notice, line), &[])
            .into_iter()
            .filter(|s: &SendChat| !s.clients.is_empty())
    {
        world.write_message(send);
    }
}

fn has_network_access(world: &World, computer: &ModularComputer) -> bool {
    computer
        .stored_id
        .and_then(|c| world.get::<crate::inventory::IdCard>(c))
        .is_some_and(|c| c.access.iter().any(|a| a == ACCESS_NETWORK))
}

/// `ui_act` of the chat client in front of `pc`.
pub fn apply(
    world: &mut World,
    actor: Entity,
    pc: Entity,
    computer: &ModularComputer,
    act: ChatAct,
) -> Result<(), &'static str> {
    if computer.active != Some(ProgramKind::ChatClient) {
        return Err("chat client not in front");
    }
    world.init_resource::<ChatChannels>();
    let mut me = world.get::<ChatClient>(pc).cloned().unwrap_or_default();
    let ts = timestamp(world);
    let channel_id = me.active_channel;
    let authed = channel_id
        .and_then(|id| world.resource::<ChatChannels>().get(id))
        .is_some_and(|c| c.operator == Some(pc) || me.netadmin);
    macro_rules! channel {
        () => {
            channel_id.and_then(|id| {
                world
                    .resource_mut::<ChatChannels>()
                    .into_inner()
                    .get_mut(id)
            })
        };
    }
    match act {
        ChatAct::Speak(raw) => {
            let message = reject_bad_chattext(&raw, MESSAGE_SIZE).ok_or("bad text")?;
            let name = me.username.clone();
            let channel = channel!().ok_or("no channel")?;
            if channel.muted.contains(&pc) {
                return Err("muted");
            }
            if !channel.is_member(pc) {
                if !channel.password.is_empty() && channel.password == message {
                    channel.add_client(&ts, pc, &name, false);
                    return Ok(());
                }
                if !me.netadmin {
                    return Err("not a member");
                }
            }
            info!("{actor} as {name} to channel {}: {message}", channel.title);
            channel.add_message(&ts, &name, &message);
            return Ok(());
        }
        ChatAct::Join(id) => {
            if Some(id) == me.active_channel {
                return Ok(());
            }
            let name = me.username.clone();
            let netadmin = me.netadmin;
            let channel = world
                .resource_mut::<ChatChannels>()
                .into_inner()
                .get_mut(id)
                .ok_or("no such channel")?;
            // Admin mode bypasses joining and passwords, unseen by the others.
            if !netadmin && !channel.is_member(pc) && channel.password.is_empty() {
                channel.add_client(&ts, pc, &name, false);
            }
            me.active_channel = Some(id);
        }
        ChatAct::Leave => {
            let id = channel_id.ok_or("no channel")?;
            remove_from(world, id, pc);
            me.active_channel = None;
        }
        ChatAct::NewChannel(raw) => {
            let title = reject_bad_chattext(&raw, CHANNELNAME_SIZE).ok_or("bad title")?;
            let name = me.username.clone();
            let mut channels = world.resource_mut::<ChatChannels>();
            let id = channels.create(&title, false).ok_or("too many channels")?;
            channels
                .get_mut(id)
                .expect("just made")
                .add_client(&ts, pc, &name, false);
            me.active_channel = Some(id);
        }
        ChatAct::ToggleAdmin => {
            if me.netadmin {
                me.netadmin = false;
                let name = me.username.clone();
                if let Some(channel) = channel!() {
                    channel.add_client(&ts, pc, &name, false);
                }
            } else {
                if !has_network_access(world, computer) {
                    return Err("no network access");
                }
                let ids: Vec<u32> = world
                    .resource::<ChatChannels>()
                    .channels
                    .iter()
                    .map(|c| c.id)
                    .collect();
                for id in ids {
                    remove_from(world, id, pc);
                }
                me.netadmin = true;
            }
        }
        ChatAct::ChangeName(raw) => {
            let new_name = reject_bad_chattext(&raw, USERNAME_SIZE)
                .ok_or("bad name")?
                .replace(' ', "_");
            if new_name == me.username {
                return Ok(());
            }
            let old = me.username.clone();
            for channel in &mut world.resource_mut::<ChatChannels>().channels {
                if channel.active.contains(&pc) {
                    channel.add_status(&ts, &format!("{old} is now known as {new_name}."));
                }
            }
            me.username = new_name;
        }
        ChatAct::RenameChannel(raw) => {
            if !authed {
                return Err("not the operator");
            }
            let title = reject_bad_chattext(&raw, CHANNELNAME_SIZE).ok_or("bad title")?;
            let channel = channel!().ok_or("no channel")?;
            // `change_title`: a strong channel keeps its name.
            if channel.strong {
                return Err("strong channel");
            }
            let line = format!(
                "Channel renamed from {} to {title} by operator.",
                channel.title
            );
            channel.add_status(&ts, &line);
            channel.title = title;
            return Ok(());
        }
        ChatAct::DeleteChannel => {
            if !authed {
                return Err("not the operator");
            }
            let id = channel_id.ok_or("no channel")?;
            let mut channels = world.resource_mut::<ChatChannels>();
            if channels.get(id).is_some_and(|c| c.strong) {
                return Err("strong channel");
            }
            channels.channels.retain(|c| c.id != id);
            me.active_channel = None;
        }
        ChatAct::SetPassword(raw) => {
            if !authed {
                return Err("not the operator");
            }
            let password =
                super::messenger::sanitize_message(&raw, MESSAGE_SIZE).unwrap_or_default();
            channel!().ok_or("no channel")?.password = password;
            return Ok(());
        }
        ChatAct::Mute(target) => {
            if !authed {
                return Err("not the operator");
            }
            let channel = channel!().ok_or("no channel")?;
            if !channel.is_member(target) {
                return Err("not a member");
            }
            let title = channel.title.clone();
            let unmuted = channel.muted.contains(&target);
            if unmuted {
                channel.muted.retain(|&c| c != target);
                alert_call(
                    world,
                    target,
                    &format!("You have been unmuted from {title}!"),
                    PING_SOUND,
                );
            } else {
                channel.muted.push(target);
                alert_call(
                    world,
                    target,
                    &format!("You have been muted from {title}!"),
                    ALERT_SOUND,
                );
            }
            return Ok(());
        }
        ChatAct::Ping(target) => {
            let now = world.resource::<Time>().elapsed_secs_f64();
            if now < me.ping_ready {
                return Err("ping cooling down");
            }
            let name = me.username.clone();
            let target_name = username(world, target);
            let channel = channel!().ok_or("no channel")?;
            if channel.muted.contains(&pc) || !channel.is_member(target) {
                return Err("cannot ping");
            }
            let title = channel.title.clone();
            channel.add_status(&ts, &format!("{name} pinged {target_name}."));
            alert_call(
                world,
                target,
                &format!("You have been pinged in {title} by {name}!"),
                PING_SOUND,
            );
            me.ping_ready = now + PING_COOLDOWN;
        }
    }
    world.entity_mut(pc).insert(me);
    Ok(())
}

/// `ui_header` while in the background: idle, new messages, or none outside a channel.
pub fn header(
    client: Option<&ChatClient>,
    channels: Option<&ChatChannels>,
) -> Option<&'static str> {
    let client = client?;
    let channel = channels?.get(client.active_channel?)?;
    let Some(&(last, _)) = channel.messages.last() else {
        return Some("ntnrc_idle.gif");
    };
    Some(if client.last_message_id == Some(last) {
        "ntnrc_idle.gif"
    } else {
        "ntnrc_new.gif"
    })
}

/// `ui_data` of the client on `pc`; `status` gives a member's computer state.
pub fn ui_data(
    pc: Entity,
    client: Option<&ChatClient>,
    channels: Option<&ChatChannels>,
    can_admin: bool,
    status: &dyn Fn(Entity) -> (bool, bool),
    names: &dyn Fn(Entity) -> String,
) -> ChatClientData {
    let fallback = ChatClient::default();
    let client = client.unwrap_or(&fallback);
    let mut data = ChatClientData {
        username: client.username.clone(),
        adminmode: client.netadmin,
        can_admin,
        active_channel: client.active_channel,
        all_channels: channels
            .map(|c| {
                c.channels
                    .iter()
                    .filter(|c| !c.title.is_empty())
                    .map(|c| ChatRoom {
                        chan: c.title.clone(),
                        id: c.id,
                    })
                    .collect()
            })
            .unwrap_or_default(),
        ..default()
    };
    let Some(channel) = client
        .active_channel
        .and_then(|id| channels.and_then(|c| c.get(id)))
    else {
        return data;
    };
    data.title = channel.title.clone();
    // No fishing: a password channel shows its members and messages to members only.
    data.authed = channel.password.is_empty() || client.netadmin || channel.is_member(pc);
    if data.authed {
        data.strong = channel.strong;
        data.is_operator = channel.operator == Some(pc) || client.netadmin;
        data.clients = channel
            .active
            .iter()
            .chain(&channel.offline)
            .map(|&c| {
                let (online, away) = status(c);
                ChatMember {
                    name: names(c),
                    online,
                    away,
                    muted: channel.muted.contains(&c),
                    operator: channel.operator == Some(c),
                    is_self: c == pc,
                    member_ref: member_ref(c),
                }
            })
            .collect();
        data.messages = channel
            .messages
            .iter()
            .rev()
            .map(|(key, msg)| ChatMessageUi {
                key: *key,
                msg: msg.clone(),
            })
            .collect();
    }
    data
}

/// `on_install` (`DefaultUser###`), then `on_start`, `background_program` and `kill_program`
/// as the computer's programs change.
fn sync_clients(
    world: &mut World,
    changed: &mut QueryState<
        (
            Entity,
            &'static ModularComputer,
            Option<&'static ChatClient>,
        ),
        Changed<ModularComputer>,
    >,
) {
    let changes: Vec<(Entity, bool, bool, bool, Option<ChatClient>)> = changed
        .iter(world)
        .filter(|(_, c, client)| c.programs.contains(&ProgramKind::ChatClient) || client.is_some())
        .map(|(e, c, client)| {
            let front = c.enabled && c.active == Some(ProgramKind::ChatClient);
            let running = front || (c.enabled && c.idle.contains(&ProgramKind::ChatClient));
            let installed = c.programs.contains(&ProgramKind::ChatClient);
            (e, installed, running, front, client.cloned())
        })
        .collect();
    if changes.is_empty() {
        return;
    }
    world.init_resource::<ChatChannels>();
    for (pc, installed, running, front, client) in changes {
        let mut client = match client {
            Some(c) => c,
            None if installed => {
                let n = {
                    let mut rng =
                        world.get_resource_or_insert_with(crate::combat::CombatRng::default);
                    rng.range(100, 999)
                };
                ChatClient {
                    username: format!("DefaultUser{n}"),
                    ..default()
                }
            }
            None => continue,
        };
        if running && !client.was_running {
            // `on_start`: back online wherever it went offline.
            for channel in &mut world.resource_mut::<ChatChannels>().channels {
                if channel.offline.contains(&pc) {
                    channel.offline.retain(|&c| c != pc);
                    channel.active.push(pc);
                }
            }
        } else if !running && client.was_running {
            // `kill_program`: offline in every channel.
            for channel in &mut world.resource_mut::<ChatChannels>().channels {
                if channel.active.contains(&pc) {
                    channel.active.retain(|&c| c != pc);
                    channel.offline.push(pc);
                }
            }
            client.active_channel = None;
        }
        if running && client.was_front && !front {
            // `background_program`: the newest message seen.
            client.last_message_id = client
                .active_channel
                .and_then(|id| world.resource::<ChatChannels>().get(id))
                .and_then(|c| c.messages.last())
                .map(|(id, _)| *id);
        }
        client.was_running = running;
        client.was_front = front;
        if world.get::<ChatClient>(pc) != Some(&client) {
            world.entity_mut(pc).insert(client);
        }
    }
}

/// `Destroy` of a client: out of every channel (no references to a gone computer).
fn purge_client(remove: On<Remove<ChatClient>>, mut channels: Option<ResMut<ChatChannels>>) {
    let pc = remove.entity;
    let Some(channels) = channels.as_mut() else {
        return;
    };
    for channel in &mut channels.channels {
        channel.active.retain(|&c| c != pc);
        channel.offline.retain(|&c| c != pc);
        channel.muted.retain(|&c| c != pc);
        if channel.operator == Some(pc) {
            channel.operator = channel.active.first().copied();
        }
    }
}

pub fn build(app: &mut App) {
    app.init_resource::<ChatChannels>()
        .add_observer(purge_client)
        .add_systems(Update, sync_clients);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world_with_clients() -> (World, Entity, Entity) {
        let mut world = World::new();
        world.init_resource::<Time>();
        world.init_resource::<ChatChannels>();
        world.init_resource::<Messages<SendChat>>();
        world.add_observer(purge_client);
        let mut spawn = |name: &str| {
            let mut pc = ModularComputer::pda("/obj/item/modular_computer/pda/crew", "pda");
            pc.enabled = true;
            pc.programs.push(ProgramKind::ChatClient);
            assert!(pc.open_program(ProgramKind::ChatClient));
            world
                .spawn((
                    pc,
                    ChatClient {
                        username: name.to_owned(),
                        was_running: true,
                        was_front: true,
                        ..default()
                    },
                ))
                .id()
        };
        let a = spawn("alice");
        let b = spawn("bob");
        (world, a, b)
    }

    fn act(world: &mut World, pc: Entity, act: ChatAct) -> Result<(), &'static str> {
        let computer = world.get::<ModularComputer>(pc).unwrap().clone();
        apply(world, pc, pc, &computer, act)
    }

    fn channel(world: &World) -> Channel {
        world.resource::<ChatChannels>().channels[0].clone()
    }

    #[test]
    fn channels_join_speak_password_and_operator() {
        let (mut world, a, b) = world_with_clients();
        act(&mut world, a, ChatAct::NewChannel("#ops".into())).unwrap();
        let c = channel(&world);
        assert_eq!(c.operator, Some(a));
        // Too long a title is refused.
        assert!(act(&mut world, a, ChatAct::NewChannel("x".repeat(19))).is_err());
        act(&mut world, a, ChatAct::SetPassword("hunter2".into())).unwrap();
        // Bob opens it: a password channel is not joined, and he cannot speak in it.
        act(&mut world, b, ChatAct::Join(c.id)).unwrap();
        assert!(!channel(&world).is_member(b));
        assert!(act(&mut world, b, ChatAct::Speak("hi".into())).is_err());
        // Not the operator: no rename.
        assert!(act(&mut world, b, ChatAct::RenameChannel("#mine".into())).is_err());
        // The password joins.
        act(&mut world, b, ChatAct::Speak("hunter2".into())).unwrap();
        assert!(channel(&world).is_member(b));
        act(&mut world, b, ChatAct::Speak("hello".into())).unwrap();
        assert!(
            channel(&world)
                .messages
                .last()
                .unwrap()
                .1
                .ends_with("bob: hello")
        );
        // Muted: silent.
        act(&mut world, a, ChatAct::Mute(b)).unwrap();
        assert!(act(&mut world, b, ChatAct::Speak("hey".into())).is_err());
        // The operator leaves: bob takes over.
        act(&mut world, a, ChatAct::Leave).unwrap();
        assert_eq!(channel(&world).operator, Some(b));
        act(&mut world, b, ChatAct::DeleteChannel).unwrap();
        assert!(world.resource::<ChatChannels>().channels.is_empty());
    }

    #[test]
    fn pings_cool_down_and_names_change() {
        let (mut world, a, b) = world_with_clients();
        act(&mut world, a, ChatAct::NewChannel("#gen".into())).unwrap();
        let id = channel(&world).id;
        act(&mut world, b, ChatAct::Join(id)).unwrap();
        act(&mut world, a, ChatAct::Ping(b)).unwrap();
        assert!(act(&mut world, a, ChatAct::Ping(b)).is_err(), "cooldown");
        act(&mut world, a, ChatAct::ChangeName("al ice".into())).unwrap();
        assert_eq!(world.get::<ChatClient>(a).unwrap().username, "al_ice");
        assert!(
            channel(&world)
                .messages
                .iter()
                .any(|(_, m)| m.ends_with("alice is now known as al_ice."))
        );
        // A computer that is gone leaves no member behind.
        world.entity_mut(b).despawn();
        world.flush();
        let c = channel(&world);
        assert!(!c.is_member(b));
        // Admin mode needs network access.
        assert!(act(&mut world, a, ChatAct::ToggleAdmin).is_err());
    }

    #[test]
    fn hostile_params_are_refused() {
        let p = |pairs: &[(&str, &str)]| -> Vec<(String, String)> {
            pairs
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect()
        };
        let bad = p(&[("id", "-1")]);
        assert!(matches!(
            parse(PRG_JOIN_CHANNEL, &UiParams::new(&bad)),
            Some(Err(_))
        ));
        let bad = p(&[("ref", "abc")]);
        assert!(matches!(
            parse(PRG_PING_USER, &UiParams::new(&bad)),
            Some(Err(_))
        ));
        assert_eq!(reject_bad_chattext("   ", 10), None);
        assert_eq!(reject_bad_chattext("a\u{7}b", 10), None);
        assert_eq!(reject_bad_chattext("привет", 10), Some("привет".into()));
    }
}
