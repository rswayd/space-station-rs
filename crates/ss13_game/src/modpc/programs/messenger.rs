//! `/datum/computer_file/program/messenger` (server): chats, sending and receiving.
//!
//! tg routes a message as a subspace signal through the telecomms message server to every
//! messenger in `GLOB.pda_messengers`; the messenger list is the PDAs with an imprint. Here the
//! signal goes through [`crate::telecomms::deliver_message`]: without a working receiver, hub,
//! bus, message server and broadcaster on the sender's levels the message is not sent ("Server
//! is not responding"); a map without telecomms machines always delivers.
// shortcut: no photos, viruses, rigged messages, send-to-all (`spam_mode`), ghost relay or radio
// jammers.

use bevy::prelude::*;
use ss13_net::chat::server::SendChat;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};

use super::super::pda::{ModularComputer, holder_mob, turf_of};
use super::super::{ChatData, MessengerData, MessengerEntry, PdaMessage};
use crate::health::Stat;
use crate::server::PlayerMob;

/// `MAX_PDA_MESSAGE_LEN`.
pub const MAX_PDA_MESSAGE_LEN: usize = 1024;
/// `MESSENGER_RINGTONE_DEFAULT`, `MESSENGER_RINGTONE_MAX_LENGTH`.
pub const RINGTONE_DEFAULT: &str = "beep";
pub const RINGTONE_MAX_LENGTH: usize = 20;
/// `COOLDOWN_START(src, last_text, 1 SECONDS)`.
pub const SEND_COOLDOWN_SECS: f64 = 1.0;

/// The messenger program's state, on the computer entity.
#[derive(Component, Clone, Debug)]
pub struct Messenger {
    pub ringtone: String,
    pub alert_silenced: bool,
    pub sending_and_receiving: bool,
    pub sort_by_job: bool,
    /// Hidden from the messenger list (nuclear PDAs).
    pub invisible: bool,
    pub chats: Vec<Chat>,
    /// `viewing_messages_of`.
    pub viewing: Option<ViewRef>,
    /// `last_text`: game time of the last message sent.
    pub last_text: Option<f64>,
    next_chat: u32,
}

impl Default for Messenger {
    fn default() -> Self {
        Self {
            ringtone: RINGTONE_DEFAULT.to_owned(),
            alert_silenced: false,
            sending_and_receiving: true,
            sort_by_job: true,
            invisible: false,
            chats: Vec::new(),
            viewing: None,
            last_text: None,
            next_chat: 1,
        }
    }
}

/// `/datum/pda_chat`.
#[derive(Clone, Debug, PartialEq)]
pub struct Chat {
    pub id: u32,
    /// The other messenger's computer; `None` once it is gone.
    pub recipient: Option<Entity>,
    pub cached_name: String,
    pub cached_job: String,
    pub messages: Vec<PdaMessage>,
    pub visible: bool,
    pub can_reply: bool,
    pub unread: u32,
}

/// What a client ref names: one of this messenger's chats, or another messenger.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewRef {
    Chat(u32),
    Messenger(Entity),
}

impl ViewRef {
    /// Hostile input: only the two shapes [`Self::encode`] makes.
    pub fn parse(text: &str) -> Option<Self> {
        if let Some(id) = text.strip_prefix("chat:") {
            return id.parse().ok().map(Self::Chat);
        }
        let bits = text.strip_prefix("pda:")?.parse().ok()?;
        Entity::try_from_bits(bits).map(Self::Messenger)
    }

    pub fn encode(self) -> String {
        match self {
            Self::Chat(id) => format!("chat:{id}"),
            Self::Messenger(e) => format!("pda:{}", e.to_bits()),
        }
    }
}

impl Messenger {
    pub fn chat(&self, id: u32) -> Option<&Chat> {
        self.chats.iter().find(|c| c.id == id)
    }

    fn chat_mut(&mut self, id: u32) -> Option<&mut Chat> {
        self.chats.iter_mut().find(|c| c.id == id)
    }

    /// `find_chat_by_recipient`, else `create_chat`.
    fn chat_with(&mut self, peer: Entity, name: &str, job: &str) -> &mut Chat {
        if let Some(i) = self.chats.iter().position(|c| c.recipient == Some(peer)) {
            return &mut self.chats[i];
        }
        let id = self.next_chat;
        self.next_chat += 1;
        self.chats.push(Chat {
            id,
            recipient: Some(peer),
            cached_name: name.to_owned(),
            cached_job: job.to_owned(),
            messages: Vec::new(),
            visible: false,
            can_reply: true,
            unread: 0,
        });
        self.chats.last_mut().expect("just pushed")
    }

    /// The sender's half of `send_message`: the message in the chat with `peer`, which the screen
    /// switches to. Returns the chat id.
    pub fn outgoing(&mut self, peer: Entity, name: &str, job: &str, text: &str, ts: &str) -> u32 {
        let chat = self.chat_with(peer, name, job);
        chat.messages.push(message(text, true, ts));
        chat.visible = true;
        chat.unread = 0;
        let id = chat.id;
        self.viewing = Some(ViewRef::Chat(id));
        id
    }

    /// `receive_message`: the message in the chat with `peer`, one more unread.
    pub fn incoming(&mut self, peer: Entity, name: &str, job: &str, text: &str, ts: &str) -> u32 {
        let chat = self.chat_with(peer, name, job);
        chat.messages.push(message(text, false, ts));
        chat.visible = true;
        chat.unread += 1;
        let id = chat.id;
        // The receiver had a new chat with the sender open: it is this chat now.
        if self.viewing == Some(ViewRef::Messenger(peer)) {
            self.viewing = Some(ViewRef::Chat(id));
        }
        id
    }

    /// `receive_message` of a fake sender (`automated`, `fakename`, `fakejob`): the chat with
    /// no messenger behind it under that name (no reply), one more unread.
    pub fn incoming_fake(&mut self, name: &str, job: &str, text: &str, ts: &str) -> u32 {
        let i = match self
            .chats
            .iter()
            .position(|c| c.recipient.is_none() && c.cached_name == name)
        {
            Some(i) => i,
            None => {
                let id = self.next_chat;
                self.next_chat += 1;
                self.chats.push(Chat {
                    id,
                    recipient: None,
                    cached_name: name.to_owned(),
                    cached_job: job.to_owned(),
                    messages: Vec::new(),
                    visible: false,
                    can_reply: false,
                    unread: 0,
                });
                self.chats.len() - 1
            }
        };
        let chat = &mut self.chats[i];
        chat.messages.push(message(text, false, ts));
        chat.visible = true;
        chat.unread += 1;
        chat.id
    }

    /// `COOLDOWN_FINISHED(src, last_text)`.
    pub fn off_cooldown(&self, now: f64) -> bool {
        self.last_text
            .is_none_or(|last| now - last >= SEND_COOLDOWN_SECS)
    }

    /// `PDA_viewMessages`: the old chat's unreads are read, a chat shown becomes visible.
    pub fn view(&mut self, target: Option<ViewRef>) {
        if let Some(ViewRef::Chat(old)) = self.viewing
            && let Some(chat) = self.chat_mut(old)
        {
            chat.unread = 0;
        }
        if let Some(ViewRef::Chat(id)) = target
            && let Some(chat) = self.chat_mut(id)
        {
            chat.visible = true;
        }
        self.viewing = target;
    }

    /// `PDA_closeMessages`.
    pub fn close(&mut self, id: u32) {
        if let Some(chat) = self.chat_mut(id) {
            chat.visible = false;
            if self.viewing == Some(ViewRef::Chat(id)) {
                self.viewing = None;
            }
        }
    }

    /// `PDA_clearMessages`: one chat, or every chat.
    pub fn clear(&mut self, id: Option<u32>) {
        match id {
            Some(id) => self.chats.retain(|c| c.id != id),
            None => self.chats.clear(),
        }
        self.viewing = None;
    }

    /// `PDA_clearUnreads`.
    pub fn clear_unreads(&mut self, id: u32) {
        if let Some(chat) = self.chat_mut(id) {
            chat.unread = 0;
        }
    }
}

fn message(text: &str, outgoing: bool, ts: &str) -> PdaMessage {
    PdaMessage {
        message: text.to_owned(),
        outgoing,
        everyone: false,
        timestamp: ts.to_owned(),
    }
}

/// `sanitize(trim(message, MAX_PDA_MESSAGE_LEN))`: control characters dropped (line breaks become
/// spaces), trimmed, cut at [`MAX_PDA_MESSAGE_LEN`] characters. `None` when nothing is left.
pub fn sanitize_message(raw: &str, max: usize) -> Option<String> {
    let text: String = raw
        .chars()
        .filter_map(|c| match c {
            '\t' | '\n' | '\r' => Some(' '),
            c if c.is_control() => None,
            c => Some(c),
        })
        .take(max)
        .collect();
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

/// Why a message was not sent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SendReject {
    NoMessenger,
    SendingDisabled,
    /// The sender has no imprint (`owner` is null: "Please imprint an ID to continue").
    NotRegistered,
    Empty,
    UnknownRecipient,
    CannotReply,
    RecipientDisabled,
    Cooldown,
    /// No broadcaster sent the signal (`!signal.data["done"]`).
    ServerDown,
}

/// A messenger in the list (`GLOB.pda_messengers`): an imprinted computer with a messenger.
pub fn registered(world: &World, pda: Entity) -> Option<(String, String)> {
    let computer = world.get::<ModularComputer>(pda)?;
    world.get::<Messenger>(pda)?;
    Some((
        computer.saved_identification.clone()?,
        computer.saved_job.clone()?,
    ))
}

/// `STRINGIFY_PDA_TARGET`.
pub fn pda_target(name: &str, job: &str) -> String {
    format!("{name} ({job})")
}

/// `PDA_sendMessage` from the messenger on `pda`, used by `sender`, to `target`: every check at
/// the moment of effect, then both chats, the chat lines and the recipient's ring.
pub fn send_message(
    world: &mut World,
    pda: Entity,
    sender: Entity,
    target: ViewRef,
    raw: &str,
) -> Result<(), SendReject> {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let client = world.get::<PlayerMob>(sender).map(|p| p.client);
    let messenger = world.get::<Messenger>(pda).ok_or(SendReject::NoMessenger)?;
    if !messenger.sending_and_receiving {
        notify(world, client, "ERROR: This device has sending disabled.");
        return Err(SendReject::SendingDisabled);
    }
    let (own_name, own_job) = registered(world, pda).ok_or(SendReject::NotRegistered)?;
    let text = sanitize_message(raw, MAX_PDA_MESSAGE_LEN).ok_or(SendReject::Empty)?;
    let peer = match target {
        ViewRef::Chat(id) => {
            let chat = messenger.chat(id).ok_or(SendReject::UnknownRecipient)?;
            if !chat.can_reply {
                notify(world, client, "ERROR: Recipient has receiving disabled.");
                return Err(SendReject::CannotReply);
            }
            chat.recipient
        }
        ViewRef::Messenger(peer) => Some(peer),
    };
    let Some((peer, (peer_name, peer_job))) = peer
        .filter(|&p| p != pda)
        .and_then(|p| Some((p, registered(world, p)?)))
    else {
        if let ViewRef::Chat(id) = target
            && let Some(mut messenger) = world.get_mut::<Messenger>(pda)
            && let Some(chat) = messenger.chat_mut(id)
        {
            chat.can_reply = false;
            chat.recipient = None;
            notify(world, client, "ERROR: Recipient no longer exists.");
        }
        return Err(SendReject::UnknownRecipient);
    };
    if !world
        .get::<Messenger>(peer)
        .is_some_and(|m| m.sending_and_receiving)
    {
        notify(world, client, "ERROR: Recipient has receiving disabled.");
        return Err(SendReject::RecipientDisabled);
    }
    if !world
        .get::<Messenger>(pda)
        .is_some_and(|m| m.off_cooldown(now))
    {
        return Err(SendReject::Cooldown);
    }
    let from = turf_of(world, pda);
    if crate::telecomms::deliver_message(world, from) == Some(false) {
        notify(world, client, "ERROR: Server is not responding.");
        return Err(SendReject::ServerDown);
    }
    let ts = timestamp(world);
    let mut own = world.get_mut::<Messenger>(pda).expect("checked above");
    own.outgoing(peer, &peer_name, &peer_job, &text, &ts);
    own.last_text = Some(now);
    let mut theirs = world.get_mut::<Messenger>(peer).expect("checked above");
    theirs.incoming(pda, &own_name, &own_job, &text, &ts);
    let (ringtone, silenced) = (theirs.ringtone.clone(), theirs.alert_silenced);
    let target_title = pda_target(&peer_name, &peer_job);
    info!("PDA: {own_name} ({own_job}) to {target_title}: {text:?}");
    if let Some(client) = client {
        world.write_message(SendChat {
            clients: vec![client],
            message: ChatMessage::simple(
                ChatChannel::System,
                Span::Info,
                format!("PDA message sent to {target_title}: \"{text}\""),
            ),
        });
    }
    // `receive_message`: the living holder reads it, then the PDA rings.
    let reader =
        holder_mob(world, peer).filter(|&m| world.get::<Stat>(m) == Some(&Stat::Conscious));
    if let Some(reader_client) = reader
        .and_then(|m| world.get::<PlayerMob>(m))
        .map(|p| p.client)
    {
        let title = pda_target(&own_name, &own_job);
        world.write_message(SendChat {
            clients: vec![reader_client],
            message: ChatMessage {
                channel: ChatChannel::System,
                segments: vec![
                    ChatSegment::new(Span::Bold, format!("PDA message from {title}, ")),
                    ChatSegment::new(Span::Plain, format!("\"{text}\"")),
                ],
            },
        });
    }
    if !silenced {
        ring(world, peer, &ringtone);
    }
    Ok(())
}

/// An automated message (`tablet_message` with `automated`) from `source` to the messenger
/// imprinted for `target_name` (the first one, as tg's `break`): through telecomms like any
/// message, then the holder's chat line and the ring. False when nothing received it.
pub fn send_automated(
    world: &mut World,
    source: Entity,
    target_name: &str,
    fake_name: &str,
    fake_job: &str,
    text: &str,
) -> bool {
    let mut q = world.query::<(Entity, &ModularComputer, &Messenger)>();
    let mut pdas: Vec<Entity> = q
        .iter(world)
        .filter(|(_, c, _)| c.saved_identification.as_deref() == Some(target_name))
        .map(|(e, _, _)| e)
        .collect();
    pdas.sort_unstable();
    let Some(pda) = pdas.first().copied() else {
        return false;
    };
    let from = turf_of(world, source);
    if crate::telecomms::deliver_message(world, from) == Some(false) {
        return false;
    }
    let ts = timestamp(world);
    let Some(mut messenger) = world.get_mut::<Messenger>(pda) else {
        return false;
    };
    if !messenger.sending_and_receiving {
        return false;
    }
    messenger.incoming_fake(fake_name, fake_job, text, &ts);
    let (ringtone, silenced) = (messenger.ringtone.clone(), messenger.alert_silenced);
    let reader = holder_mob(world, pda).filter(|&m| world.get::<Stat>(m) == Some(&Stat::Conscious));
    if let Some(client) = reader
        .and_then(|m| world.get::<PlayerMob>(m))
        .map(|p| p.client)
    {
        let title = pda_target(fake_name, fake_job);
        world.write_message(SendChat {
            clients: vec![client],
            message: ChatMessage {
                channel: ChatChannel::System,
                segments: vec![
                    ChatSegment::new(Span::Bold, format!("PDA message from {title}, ")),
                    ChatSegment::new(Span::Plain, format!("\"{text}\" [Automated Message]")),
                ],
            },
        });
    }
    if !silenced {
        ring(world, pda, &ringtone);
    }
    true
}

/// `/obj/item/modular_computer/proc/ring`: `*ringtone*` to everyone who hears the PDA.
fn ring(world: &mut World, pda: Entity, ringtone: &str) {
    let Some(at) = turf_of(world, pda) else {
        return;
    };
    let sends = crate::pull::visible_message(
        world,
        at,
        ChatChannel::Local,
        (Span::Plain, format!("*{ringtone}*")),
        &[],
    );
    for send in sends.into_iter().filter(|s| !s.clients.is_empty()) {
        world.write_message(send);
    }
}

fn notify(world: &mut World, client: Option<Entity>, text: &str) {
    if let Some(client) = client {
        world.write_message(SendChat {
            clients: vec![client],
            message: ChatMessage::simple(ChatChannel::System, Span::Notice, text),
        });
    }
}

/// `round_timestamp("hh:mm")`.
fn timestamp(world: &World) -> String {
    let secs = round_secs(world);
    format!("{:02}:{:02}", secs / 3600, secs / 60 % 60)
}

/// Whole seconds since the round started.
pub fn round_secs(world: &World) -> u64 {
    let now = world.resource::<Time>().elapsed_secs_f64();
    world
        .get_resource::<crate::round::Ticker>()
        .map_or(0.0, |t| t.round_duration(now))
        .max(0.0) as u64
}

/// `set_ringtone`: `trim(html_encode(new_ringtone), MESSENGER_RINGTONE_MAX_LENGTH)`.
pub fn sanitize_ringtone(raw: &str) -> Option<String> {
    sanitize_message(raw, RINGTONE_MAX_LENGTH)
}

/// One entry of the messenger list.
#[derive(Clone, Copy)]
pub struct Peer<'a> {
    pub entity: Entity,
    pub name: &'a str,
    pub job: &'a str,
    pub invisible: bool,
}

/// `ui_data` of the messenger on `pda`. `peers` is every imprinted messenger; the list skips
/// this one and the invisible ones, sorted like `GLOB.pda_messengers_by_job` / `_by_name`.
pub fn ui_data<'a>(
    pda: Entity,
    computer: &ModularComputer,
    messenger: &Messenger,
    peers: impl Iterator<Item = Peer<'a>>,
) -> MessengerData {
    let peers: Vec<Peer> = peers.collect();
    let live = |e: Option<Entity>| e.and_then(|e| peers.iter().find(|p| p.entity == e));
    let mut messengers: Vec<MessengerEntry> = peers
        .iter()
        .filter(|p| p.entity != pda && !p.invisible)
        .map(|p| MessengerEntry {
            name: p.name.to_owned(),
            job: p.job.to_owned(),
            messenger_ref: ViewRef::Messenger(p.entity).encode(),
        })
        .collect();
    if messenger.sort_by_job {
        messengers.sort_by(|a, b| a.job.cmp(&b.job).then_with(|| a.name.cmp(&b.name)));
    } else {
        messengers.sort_by(|a, b| a.name.cmp(&b.name));
    }
    let saved_chats = messenger
        .chats
        .iter()
        .map(|chat| {
            // `get_recipient_name`: the live imprint while the recipient is registered.
            let peer = live(chat.recipient);
            ChatData {
                chat_ref: ViewRef::Chat(chat.id).encode(),
                recipient: MessengerEntry {
                    name: peer.map_or(chat.cached_name.clone(), |p| p.name.to_owned()),
                    job: peer.map_or(chat.cached_job.clone(), |p| p.job.to_owned()),
                    messenger_ref: peer
                        .map(|p| ViewRef::Messenger(p.entity).encode())
                        .unwrap_or_default(),
                },
                messages: chat.messages.clone(),
                visible: chat.visible,
                can_reply: chat.can_reply,
                unread_messages: chat.unread,
            }
        })
        .collect();
    let owner = computer
        .saved_identification
        .clone()
        .zip(computer.saved_job.clone())
        .map(|(name, job)| MessengerEntry {
            name,
            job,
            messenger_ref: ViewRef::Messenger(pda).encode(),
        });
    MessengerData {
        owner,
        saved_chats,
        messengers,
        sort_by_job: messenger.sort_by_job,
        alert_silenced: messenger.alert_silenced,
        sending_and_receiving: messenger.sending_and_receiving,
        open_chat: messenger.viewing.map(ViewRef::encode),
        ringtone: messenger.ringtone.clone(),
    }
}

#[cfg(test)]
mod tests {
    use core::time::Duration;

    use bevy::ecs::message::Messages;

    use ss13_shared::TilePos;

    use super::*;
    use crate::inventory::CarriedBy;

    /// Two imprinted PDAs, the first carried by a conscious player mob.
    fn world() -> (World, Entity, Entity, Entity) {
        let mut world = World::new();
        world.insert_resource(Time::<()>::default());
        world.init_resource::<Messages<SendChat>>();
        let pda = |world: &mut World, name: &str| {
            let mut computer = ModularComputer::pda("/obj/item/modular_computer/pda", "pda");
            computer.saved_identification = Some(name.to_owned());
            computer.saved_job = Some("Ассистент".to_owned());
            world.spawn((computer, Messenger::default())).id()
        };
        let a = pda(&mut world, "Алиса");
        let b = pda(&mut world, "Боб");
        let client = world.spawn_empty().id();
        let mob = world
            .spawn((PlayerMob { client }, Stat::Conscious, TilePos::new(1, 1, 1)))
            .id();
        world.entity_mut(a).insert(CarriedBy(mob));
        (world, a, b, mob)
    }

    fn advance(world: &mut World, secs: f64) {
        world
            .resource_mut::<Time>()
            .advance_by(Duration::from_secs_f64(secs));
    }

    #[test]
    fn a_message_lands_in_both_chats() {
        let (mut world, a, b, mob) = world();
        advance(&mut world, 5.0);
        send_message(&mut world, a, mob, ViewRef::Messenger(b), "  Привет,\nБоб ").unwrap();
        let mine = world.get::<Messenger>(a).unwrap();
        assert_eq!(mine.chats.len(), 1);
        assert_eq!(mine.chats[0].messages[0].message, "Привет, Боб");
        assert!(mine.chats[0].messages[0].outgoing);
        assert_eq!(mine.viewing, Some(ViewRef::Chat(mine.chats[0].id)));
        let theirs = world.get::<Messenger>(b).unwrap();
        assert_eq!(theirs.chats[0].recipient, Some(a));
        assert_eq!(theirs.chats[0].cached_name, "Алиса");
        assert_eq!(theirs.chats[0].unread, 1);
        assert!(!theirs.chats[0].messages[0].outgoing);
        // The reply goes through the receiver's chat, into the same chat on both sides.
        let chat = theirs.chats[0].id;
        send_message(&mut world, b, mob, ViewRef::Chat(chat), "Привет").unwrap();
        assert_eq!(world.get::<Messenger>(a).unwrap().chats.len(), 1);
        assert_eq!(
            world.get::<Messenger>(a).unwrap().chats[0].messages.len(),
            2
        );
        assert_eq!(world.get::<Messenger>(a).unwrap().chats[0].unread, 1);
    }

    #[test]
    fn sending_is_rate_limited_and_validated() {
        let (mut world, a, b, mob) = world();
        advance(&mut world, 5.0);
        let send = |world: &mut World, to, text: &str| send_message(world, a, mob, to, text);
        send(&mut world, ViewRef::Messenger(b), "1").unwrap();
        assert_eq!(
            send(&mut world, ViewRef::Messenger(b), "2"),
            Err(SendReject::Cooldown)
        );
        advance(&mut world, 1.0);
        send(&mut world, ViewRef::Messenger(b), "3").unwrap();
        advance(&mut world, 1.0);
        assert_eq!(
            send(&mut world, ViewRef::Messenger(b), " \n "),
            Err(SendReject::Empty)
        );
        assert_eq!(
            send(&mut world, ViewRef::Messenger(a), "me"),
            Err(SendReject::UnknownRecipient)
        );
        assert_eq!(
            send(&mut world, ViewRef::Chat(99), "x"),
            Err(SendReject::UnknownRecipient)
        );
        let stranger = world.spawn_empty().id();
        assert_eq!(
            send(&mut world, ViewRef::Messenger(stranger), "x"),
            Err(SendReject::UnknownRecipient)
        );
        world.get_mut::<Messenger>(b).unwrap().sending_and_receiving = false;
        assert_eq!(
            send(&mut world, ViewRef::Messenger(b), "x"),
            Err(SendReject::RecipientDisabled)
        );
        world.get_mut::<Messenger>(b).unwrap().sending_and_receiving = true;
        // No imprint: not in the list, cannot send.
        world
            .get_mut::<ModularComputer>(a)
            .unwrap()
            .saved_identification = None;
        assert_eq!(
            send(&mut world, ViewRef::Messenger(b), "x"),
            Err(SendReject::NotRegistered)
        );
        let b_msgs = world.get::<Messenger>(b).unwrap().chats[0].messages.len();
        assert_eq!(b_msgs, 2);
        // A gone recipient makes the chat read-only.
        world
            .get_mut::<ModularComputer>(a)
            .unwrap()
            .saved_identification = Some("Алиса".into());
        let chat = world.get::<Messenger>(a).unwrap().chats[0].id;
        world.despawn(b);
        assert_eq!(
            send(&mut world, ViewRef::Chat(chat), "x"),
            Err(SendReject::UnknownRecipient)
        );
        assert!(!world.get::<Messenger>(a).unwrap().chats[0].can_reply);
        assert_eq!(
            send(&mut world, ViewRef::Chat(chat), "x"),
            Err(SendReject::CannotReply)
        );
    }

    #[test]
    fn refs_round_trip_and_reject_garbage() {
        let e = Entity::from_raw_u32(7).unwrap();
        for r in [ViewRef::Chat(3), ViewRef::Messenger(e)] {
            assert_eq!(ViewRef::parse(&r.encode()), Some(r));
        }
        for bad in ["", "chat:", "chat:-1", "pda:x", "7", "pda:"] {
            assert_eq!(ViewRef::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn the_list_hides_self_and_invisible_and_sorts() {
        let computer = ModularComputer::pda("/obj/item/modular_computer/pda", "pda");
        let me = Entity::from_raw_u32(1).unwrap();
        let peer = |i: u32, name: &'static str, job: &'static str, invisible| Peer {
            entity: Entity::from_raw_u32(i).unwrap(),
            name,
            job,
            invisible,
        };
        let peers = [
            peer(1, "Я", "Ассистент", false),
            peer(2, "Яна", "Врач", false),
            peer(3, "Борис", "Инженер", true),
            peer(4, "Анна", "Шеф", false),
        ];
        let mut messenger = Messenger::default();
        let data = ui_data(me, &computer, &messenger, peers.iter().copied());
        let names: Vec<_> = data.messengers.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, ["Яна", "Анна"]);
        assert!(data.owner.is_none());
        messenger.sort_by_job = false;
        let data = ui_data(me, &computer, &messenger, peers.iter().copied());
        let names: Vec<_> = data.messengers.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, ["Анна", "Яна"]);
    }
}
