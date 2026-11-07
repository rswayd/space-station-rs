//! The join queue (server), after tg's population caps: `hard_popcap`/`extreme_popcap`, the
//! lobby "Join Game" button (`/atom/movable/screen/lobby/button/join`), the latejoin menu's
//! `select_job` check, `AttemptLateSpawn`'s dequeue and `SSticker.check_queue`.
//!
//! With a cap set ([`crate::round::RoundConfig::hard_popcap`], `extreme_popcap`), a lobby
//! player pressing "Join Game" while the living players reach it (or while anyone waits) is
//! queued instead ([`join_button`]); only the first in line may pick a job, and only while a slot
//! is free ([`may_select`]). Once a second ([`check_queue`]): every 5 s the first in line is
//! told a slot opened (with the latejoin menu), 25 s without joining drops them; a cap removed
//! releases everyone.
// shortcut: admins wait in the queue too (tg lets `GLOB.admin_datums` through): the game crate
// does not know the admins; `extreme_popcap`'s connection refusal is the panic server redirect,
// not ported (no panic server); no soft popcap login notice.

use bevy::prelude::*;
use ss13_net::chat::server::SendChat;
use ss13_net::sound::server::SendSound;
use ss13_shared::chat::{ChatChannel, ChatMessage, Span};
use ss13_shared::protocol::SoundEvent;

use crate::ghost::Ghost;
use crate::health::Stat;
use crate::lobby::GameState;
use crate::round::{NewPlayer, RoundConfig, Ticker};
use crate::server::PlayerMob;

/// tg `hard_popcap_message` (config default).
pub const HARD_POPCAP_MESSAGE: &str = "Сервер сейчас обслуживает большое число игроков, \
     присоединиться пока нельзя. Вы можете подождать, пока число живых членов экипажа уменьшится, \
     понаблюдать или найти другой сервер.";
/// `check_queue`'s `SEND_SOUND`.
pub const NOTICE_SOUND: &str = "sound/announcer/notice/notice1.ogg";
/// `SSticker` fires once a second; `queue_delay` counts the fires.
const FIRE_SECS: f64 = 1.0;
/// `AttemptLateSpawn`: `queue_delay = 4`, the next check one fire later.
const DELAY_AFTER_JOIN: u32 = 4;

/// tg `SSticker.queued_players` and `queue_delay`.
#[derive(Resource, Debug, Default)]
pub struct JoinQueue {
    /// Lobby clients, first in line first.
    pub queued: Vec<Entity>,
    pub delay: u32,
    next_fire: f64,
}

/// tg `living_player_count()`: player bodies that are not dead.
pub fn living_player_count(world: &mut World) -> u32 {
    world
        .query_filtered::<Option<&Stat>, (With<PlayerMob>, Without<Ghost>)>()
        .iter(world)
        .filter(|s| s.copied() != Some(Stat::Dead))
        .count() as u32
}

/// The latejoin checks' `relevant_cap`: the lower of both caps when both are set.
pub fn relevant_cap(config: &RoundConfig) -> Option<u32> {
    match (config.hard_popcap, config.extreme_popcap) {
        (Some(h), Some(e)) => Some(h.min(e)),
        (h, e) => h.or(e),
    }
}

/// The "Join Game" button: `None` opens the latejoin menu; otherwise the client is (or stays)
/// in the queue and gets these lines.
pub fn join_button(
    queue: &mut JoinQueue,
    cap: Option<u32>,
    living: u32,
    client: Entity,
) -> Option<Vec<(Span, String)>> {
    let full = cap.is_some_and(|cap| living >= cap);
    if queue.queued.is_empty() && !full {
        return None;
    }
    let mut lines = vec![(Span::Danger, HARD_POPCAP_MESSAGE.to_owned())];
    lines.push(match queue.queued.iter().position(|&c| c == client) {
        Some(0) => (
            Span::Notice,
            "Вы следующий в очереди. Вам сообщат, когда освободится место.".to_owned(),
        ),
        Some(i) => (Span::Notice, format!("Перед вами в очереди игроков: {i}.")),
        None => {
            queue.queued.push(client);
            (
                Span::Notice,
                format!(
                    "Вы добавлены в очередь на вход в игру. Ваше место в очереди: {}.",
                    queue.queued.len()
                ),
            )
        }
    });
    Some(lines)
}

/// `select_job`: with a queue only its first may join, and only below the cap.
pub fn may_select(queue: &JoinQueue, cap: Option<u32>, living: u32, client: Entity) -> bool {
    queue.queued.is_empty() || (living < cap.unwrap_or(0) && queue.queued.first() == Some(&client))
}

/// `AttemptLateSpawn`: the joiner leaves the queue and the next check comes one fire later.
pub fn joined(world: &mut World, client: Entity) {
    if let Some(mut queue) = world.get_resource_mut::<JoinQueue>() {
        queue.queued.retain(|&c| c != client);
        queue.delay = DELAY_AFTER_JOIN;
    }
}

/// `check_queue` once a second while the round is in progress.
pub(crate) fn fire(world: &mut World) {
    if world.resource::<Ticker>().state() != GameState::Playing {
        return;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut queue = world.resource_mut::<JoinQueue>();
    if now < queue.next_fire {
        return;
    }
    queue.next_fire = now + FIRE_SECS;
    check_queue(world);
}

/// What one `check_queue` did to a client.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueueEvent {
    /// The cap is gone: everyone may join.
    Released(Entity),
    /// A slot opened for the first in line.
    SlotOpened(Entity),
    /// The first in line did not join in time.
    Dropped(Entity),
}

/// tg `check_queue` on the queue alone (`list_clear_nulls`: `in_lobby` says who is still there).
pub fn step(
    queue: &mut JoinQueue,
    hard_popcap: Option<u32>,
    living: u32,
    in_lobby: impl Fn(Entity) -> bool,
) -> Vec<QueueEvent> {
    if queue.queued.is_empty() {
        return Vec::new();
    }
    let Some(cap) = hard_popcap else {
        let released = queue
            .queued
            .drain(..)
            .filter(|&c| in_lobby(c))
            .map(QueueEvent::Released)
            .collect();
        queue.delay = 0;
        return released;
    };
    queue.delay += 1;
    let next = queue.queued[0];
    match queue.delay {
        5 => {
            queue.queued.retain(|&c| in_lobby(c));
            if living < cap {
                if in_lobby(next) {
                    return vec![QueueEvent::SlotOpened(next)];
                }
                // Disconnected: out of the line.
                queue.queued.retain(|&c| c != next);
            }
            queue.delay = 0;
            Vec::new()
        }
        d if d >= 25 => {
            queue.queued.retain(|&c| c != next);
            queue.delay = 0;
            if in_lobby(next) {
                vec![QueueEvent::Dropped(next)]
            } else {
                Vec::new()
            }
        }
        _ => Vec::new(),
    }
}

fn check_queue(world: &mut World) {
    let hard = world.resource::<RoundConfig>().hard_popcap;
    let living = living_player_count(world);
    let lobby: Vec<Entity> = world
        .query_filtered::<Entity, With<NewPlayer>>()
        .iter(world)
        .collect();
    let mut queue = std::mem::take(&mut *world.resource_mut::<JoinQueue>());
    let events = step(&mut queue, hard, living, |c| lobby.contains(&c));
    *world.resource_mut::<JoinQueue>() = queue;
    for event in events {
        let (client, span, text, menu) = match event {
            QueueEvent::Released(c) => (
                c,
                Span::Danger,
                "Ограничение числа живых игроков снято! Можно войти в игру.",
                true,
            ),
            QueueEvent::SlotOpened(c) => (
                c,
                Span::Danger,
                "Освободилось место! У вас примерно 20 секунд, чтобы войти в игру.",
                true,
            ),
            QueueEvent::Dropped(c) => (
                c,
                Span::Danger,
                "Ответа не получено. Вы удалены из очереди.",
                false,
            ),
        };
        world.write_message(SendChat {
            clients: vec![client],
            message: ChatMessage::simple(ChatChannel::System, span, text),
        });
        if menu {
            world.write_message(SendSound {
                client,
                sound: SoundEvent {
                    file: NOTICE_SOUND.to_owned(),
                    volume: 100.0,
                    speed: 1.0,
                    offset: None,
                },
            });
            crate::jobs::open_latejoin_menu(world, client);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clients() -> (Entity, Entity) {
        let mut world = World::new();
        (world.spawn_empty().id(), world.spawn_empty().id())
    }

    #[test]
    fn a_full_server_queues_the_join_button_in_order() {
        let (a, b) = clients();
        let mut queue = JoinQueue::default();
        assert_eq!(join_button(&mut queue, None, 100, a), None, "no cap");
        assert_eq!(
            join_button(&mut queue, Some(10), 9, a),
            None,
            "below the cap"
        );
        let lines = join_button(&mut queue, Some(10), 10, a).expect("full");
        assert!(lines[1].1.contains("место в очереди: 1"));
        join_button(&mut queue, Some(10), 10, b).expect("full");
        assert_eq!(queue.queued, vec![a, b]);
        // Pressing again tells the position; a queue alone keeps the button closed.
        let lines = join_button(&mut queue, Some(10), 3, b).expect("queued");
        assert!(lines[1].1.contains("игроков: 1"));
        assert_eq!(queue.queued, vec![a, b]);
        // Only the first may pick a job, below the cap.
        assert!(may_select(&queue, Some(10), 9, a));
        assert!(!may_select(&queue, Some(10), 9, b));
        assert!(!may_select(&queue, Some(10), 10, a));
    }

    #[test]
    fn check_queue_offers_the_slot_every_5_fires_and_drops_at_25() {
        let (a, b) = clients();
        let mut queue = JoinQueue {
            queued: vec![a, b],
            ..default()
        };
        let lobby = |_: Entity| true;
        // Full: nothing for 5 fires, then the timer restarts.
        for _ in 0..5 {
            assert!(step(&mut queue, Some(10), 10, lobby).is_empty());
        }
        assert_eq!(queue.delay, 0);
        // A slot: the fifth fire offers it to the first in line.
        for _ in 0..4 {
            assert!(step(&mut queue, Some(10), 9, lobby).is_empty());
        }
        assert_eq!(
            step(&mut queue, Some(10), 9, lobby),
            vec![QueueEvent::SlotOpened(a)]
        );
        // No join: dropped on the 25th fire, `b` is next.
        for _ in 6..25 {
            assert!(step(&mut queue, Some(10), 9, lobby).is_empty());
        }
        assert_eq!(
            step(&mut queue, Some(10), 9, lobby),
            vec![QueueEvent::Dropped(a)]
        );
        assert_eq!(queue.queued, vec![b]);
        // The cap removed: everyone still in the lobby is released.
        assert_eq!(
            step(&mut queue, None, 9, lobby),
            vec![QueueEvent::Released(b)]
        );
        assert!(queue.queued.is_empty());
    }

    #[test]
    fn a_disconnected_first_in_line_loses_the_place() {
        let (a, b) = clients();
        let mut queue = JoinQueue {
            queued: vec![a, b],
            delay: 4,
            ..default()
        };
        assert!(step(&mut queue, Some(10), 9, |c| c == b).is_empty());
        assert_eq!(queue.queued, vec![b]);
        assert_eq!(queue.delay, 0);
    }
}
