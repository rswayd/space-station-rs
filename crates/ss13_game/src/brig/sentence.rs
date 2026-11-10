//! Timed sentences on prisoner IDs (server), after tg `/obj/item/card/id/advanced/prisoner`
//! (`set_sentence_time`, `start_timer`, `process`, `attack_self`, `examine`) and
//! `prisongate/allow_prisoner_id` (`prisongate.dm`).
//!
//! - An ID with `ACCESS_BRIG` used on a prisoner ID held in the other hand asks for the sentence
//!   in seconds; a card that already has one is reset instead.
//! - The sentence starts when the prisoner passes a brig prison gate ([`gate_check`]: the gate
//!   lets them in and the card announces it); while it runs the gates refuse the card; once it
//!   has run out the card says so, and the next gate lets the prisoner out and clears the
//!   sentence. The labor camp's gate reads the points instead (`crate::brig::labor`).
//! - Used in hand, the card tells its points and goal.
// shortcut: the reset keeps the card's name (tg also resets `registered_name`); no locker
// linking by swiping the card on a brig locker (BandaStation's hint on examine).

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_shared::chat::Span;
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, PromptSystems, ask_world};
use ss13_tgui::server::UiSystems;

use super::prisoner::PrisonerCard;
use crate::inventory::{IdCard, Inventory};
use crate::tools::tell;

/// `ACCESS_BRIG`.
const ACCESS_BRIG: &str = "brig";
/// `tgui_input_number` defaults: `max_value = 10000`, `min_value = 0`.
const MAX_SENTENCE_SECS: f64 = 10000.0;

/// What a brig prison gate does with a timed prisoner ID (`allow_prisoner_id`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SentenceVerdict {
    /// `time_to_assign` set: in, and the sentence starts.
    Start,
    /// Run out: out, and the card is no longer timed.
    Served,
    /// Still running: refused.
    Ongoing,
}

/// `allow_prisoner_id` of a station gate; `None` for a card without a sentence (`SKIP_EXIT`).
pub fn gate_check(card: &PrisonerCard, now_ms: u64) -> Option<SentenceVerdict> {
    if !card.timed {
        return None;
    }
    if card.time_to_assign > 0 {
        return Some(SentenceVerdict::Start);
    }
    match card.sentence_end_ms {
        Some(end) if now_ms < end => Some(SentenceVerdict::Ongoing),
        _ => Some(SentenceVerdict::Served),
    }
}

pub fn now_ms(world: &World) -> u64 {
    (world.resource::<Time>().elapsed_secs_f64() * 1000.0) as u64
}

/// On a prompt: the card whose sentence is being set.
#[derive(Component, Clone, Copy, Debug)]
struct SentenceAsk {
    card: Entity,
}

pub struct SentencePlugin;

impl Plugin for SentencePlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // An ID on the prisoner ID in the other hand: the sentence.
        app.add_click_handler(ClickPhase::OtherHand, 1009, |w, c| {
            c.held.is_some_and(|h| {
                crate::brig::sentence::set_sentence(w, c.client, c.mob, h, c.target)
            })
        });
        // The prisoner ID used in hand: its points.
        app.add_click_handler(ClickPhase::AttackSelf, 3329, |w, c| {
            crate::brig::sentence::attack_self(w, c.client, c.target)
        });
        app.add_systems(
            Update,
            (
                run_sentences,
                answers
                    .in_set(UiSystems::Handle)
                    .after(PromptSystems::Answer),
            ),
        );
    }
}

/// `set_sentence_time`: `id` (an ID in the active hand) on `card` (a prisoner ID in the other
/// hand). True when the click was the card's.
pub fn set_sentence(
    world: &mut World,
    client: Entity,
    mob: Entity,
    id: Entity,
    card: Entity,
) -> bool {
    let Some(state) = world.get::<PrisonerCard>(card).copied() else {
        return false;
    };
    let Some(id_card) = crate::inventory::get_id(world, id) else {
        return false;
    };
    let allowed = world
        .get::<IdCard>(id_card)
        .is_some_and(|c| c.access.iter().any(|a| a == ACCESS_BRIG));
    if !allowed {
        crate::balloon::balloon_alert(world, mob, card, "в доступе отказано!");
        return true;
    }
    if !holding(world, mob, card) {
        tell(
            world,
            client,
            Span::Warning,
            "Вы должны держать ID-карту, чтобы продолжить!",
        );
        return true;
    }
    if state.timed {
        if let Some(mut c) = world.get_mut::<PrisonerCard>(card) {
            c.timed = false;
            c.time_to_assign = 0;
            c.sentence_end_ms = None;
        }
        tell(
            world,
            client,
            Span::Plain,
            "Сброс параметров ID-карты заключённого.",
        );
        return true;
    }
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            mob,
            "Приговор",
            "Время приговора в секундах",
            PromptKind::Number {
                default: 0.0,
                min: 0.0,
                max: MAX_SENTENCE_SECS,
                round: true,
            },
        ),
    );
    world.entity_mut(prompt).insert(SentenceAsk { card });
    true
}

fn holding(world: &World, mob: Entity, card: Entity) -> bool {
    world
        .get::<Inventory>(mob)
        .is_some_and(|i| i.hands.contains(&Some(card)))
}

/// The sentence answered, after the prompt's sleep: still holding the card.
fn answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answered: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, mob, answer) in answered {
        let Some(SentenceAsk { card }) = world.get::<SentenceAsk>(prompt).copied() else {
            continue;
        };
        let Some(PromptAnswer::Number(secs)) = answer else {
            continue;
        };
        let conscious = world
            .get::<crate::health::Stat>(mob)
            .is_none_or(|s| *s == crate::health::Stat::Conscious);
        if !conscious || !holding(world, mob, card) {
            continue;
        }
        let secs = secs.clamp(0.0, MAX_SENTENCE_SECS) as u32;
        let Some(mut c) = world.get_mut::<PrisonerCard>(card) else {
            continue;
        };
        c.time_to_assign = secs;
        c.timed = true;
        tell(
            world,
            client,
            Span::Plain,
            format!(
                "Вы установили время приговора на {}.",
                crate::restraints::display_time(secs as f32)
            ),
        );
    }
}

/// `start_timer` (from a gate, [`crate::brig::labor`]): the sentence counts from now.
pub fn start(world: &mut World, card: Entity) {
    let now = now_ms(world);
    let Some(mut c) = world.get_mut::<PrisonerCard>(card) else {
        return;
    };
    c.sentence_end_ms = Some(now + u64::from(c.time_to_assign) * 1000);
    c.time_to_assign = 0;
    crate::cargo::console::machine_say(
        world,
        card,
        "Приговор вступил в силу. Добро пожаловать в корпоративный реабилитационный центр!",
    );
}

/// `process()`: a sentence that ran out says so once.
// Cost: one pass over the prisoner IDs (a few per map) per tick.
fn run_sentences(world: &mut World) {
    let now = now_ms(world);
    let mut q = world.query::<(Entity, &PrisonerCard)>();
    let done: Vec<Entity> = q
        .iter(world)
        .filter(|(_, c)| c.timed && c.sentence_end_ms.is_some_and(|end| now >= end))
        .map(|(e, _)| e)
        .collect();
    for card in done {
        if let Some(mut c) = world.get_mut::<PrisonerCard>(card) {
            c.sentence_end_ms = None;
        }
        crate::cargo::console::machine_say(
            world,
            card,
            "Срок наказания отбыт. Благодарим вас за сотрудничество в рамках нашей корпоративной \
             программы реабилитации!",
        );
    }
}

/// `attack_self`: the points so far. True when `item` is a prisoner ID.
pub fn attack_self(world: &mut World, client: Entity, item: Entity) -> bool {
    let Some(c) = world.get::<PrisonerCard>(item).copied() else {
        return false;
    };
    tell(
        world,
        client,
        Span::Notice,
        format!(
            "Вы набрали {} из {} очков, необходимых вам, для обретения свободы.",
            c.points, c.goal
        ),
    );
    true
}

/// `examine` of a timed card.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<crate::examine::Line>) {
    let Some(c) = world.get::<PrisonerCard>(target) else {
        return;
    };
    if c.timed {
        let text = if c.time_to_assign > 0 {
            format!(
                "Цифровой таймер на карте установлен на {}. Таймер запустится, когда заключённый \
                 пройдёт через сканеры тюремных ворот.",
                crate::restraints::display_time(c.time_to_assign as f32)
            )
        } else {
            let left = c
                .sentence_end_ms
                .map_or(0, |end| end.saturating_sub(now_ms(world)) / 1000);
            if left == 0 {
                "На цифровом таймере карты осталось ноль секунд. Вы выходите другим, но свободным \
                 человеком."
                    .to_owned()
            } else {
                format!(
                    "На цифровом таймере карты осталось {}. Не совершай преступление, если не \
                     можешь отбыть срок.",
                    crate::restraints::display_time(left as f32)
                )
            }
        };
        lines.push((Span::Notice, text));
    }
    lines.push((
        Span::Notice,
        "Проведите ID-картой СБ по карте, чтобы назначить/сбросить время приговора.".to_owned(),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sentence_lets_in_then_holds_then_frees() {
        let mut card = PrisonerCard::default();
        assert_eq!(
            gate_check(&card, 0),
            None,
            "no sentence: the gate skips the card"
        );
        card.timed = true;
        card.time_to_assign = 60;
        assert_eq!(gate_check(&card, 0), Some(SentenceVerdict::Start));
        card.time_to_assign = 0;
        card.sentence_end_ms = Some(60_000);
        assert_eq!(gate_check(&card, 59_999), Some(SentenceVerdict::Ongoing));
        assert_eq!(gate_check(&card, 60_000), Some(SentenceVerdict::Served));
        card.sentence_end_ms = None;
        assert_eq!(gate_check(&card, 0), Some(SentenceVerdict::Served));
    }
}
