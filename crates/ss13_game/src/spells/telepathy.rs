//! Telepathy (server) after tg `spell_types/list_target/telepathy.dm` (`_list_target.dm`).
//!
//! The button lists the living in view (radius 7, not the caster); the chosen one gets a text
//! prompt (`MAX_MESSAGE_LEN`), then the message: the caster sees it, the target hears it (mind
//! antimagic blocks it, the caster is told), ghosts see both ends.
// shortcut: no TTS (`cast_tts`: `crate` has no TTS yet) and targets in range, not `view()`.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, ask_world};
use ss13_world::SpatialIndex;

use super::{
    CastOn, MAGIC_RESISTANCE_MIND, Spell, Spells, can_block_magic, can_cast, cast, chat_to,
};
use crate::health::Health;
use crate::server::PlayerMob;

const RADIUS: u32 = 7;
const MAX_MESSAGE_LEN: u32 = 1024;

/// The step of the telepathy prompts.
#[derive(Component, Clone, Debug)]
pub enum TelepathyPrompt {
    Target(Vec<Entity>),
    Message(Entity),
}

/// `get_list_targets`.
fn targets(world: &World, mob: Entity, pos: TilePos) -> Vec<Entity> {
    let mut near: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .in_radius(pos, RADIUS)
        .filter(|&e| e != mob && world.get::<Health>(e).is_some())
        .collect();
    near.sort();
    near
}

/// `PreActivate`: the target list, or "no targets nearby!".
pub(super) fn open_list(world: &mut World, client: Entity, mob: Entity) {
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    let list = targets(world, mob, pos);
    if list.is_empty() {
        chat_to(world, mob, Span::Warning, "no targets nearby!");
        return;
    }
    let items: Vec<String> = list
        .iter()
        .map(|&e| crate::medical::name_of(world, e))
        .collect();
    let ask = Ask::new(
        client,
        mob,
        Spell::Telepathy.def().name,
        "Выберите цель, которой хотите прошептать.",
        PromptKind::List {
            items,
            default: None,
        },
    );
    let prompt = ask_world(world, ask);
    world
        .entity_mut(prompt)
        .insert(TelepathyPrompt::Target(list));
}

/// The wait's end: still the caster's player, the spell known, the target in range
/// (`QDELETED`, `can_cast_spell`, `get_dist > target_radius`).
fn still_valid(world: &World, client: Entity, actor: Entity, target: Entity) -> bool {
    world.get::<PlayerMob>(actor).map(|p| p.client) == Some(client)
        && super::knows(world, actor, Spell::Telepathy)
        && can_cast(world, actor, Spell::Telepathy).is_ok()
        && world
            .get::<TilePos>(actor)
            .zip(world.get::<TilePos>(target))
            .is_some_and(|(a, b)| a.chebyshev(*b).is_some_and(|d| d <= RADIUS))
}

pub(super) fn answers(world: &mut World, mut answers: Local<MessageCursor<PromptAnswered>>) {
    let pending: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = answers
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, actor, answer) in pending {
        let Some(asked) = world.get::<TelepathyPrompt>(prompt).cloned() else {
            continue;
        };
        match (asked, answer) {
            (TelepathyPrompt::Target(list), Some(PromptAnswer::Item(i))) => {
                let Some(&target) = list.get(i) else {
                    continue;
                };
                if !still_valid(world, client, actor, target) {
                    continue;
                }
                let name = crate::medical::name_of(world, target);
                let ask = Ask::new(
                    client,
                    actor,
                    Spell::Telepathy.def().name,
                    format!("Что бы вы хотели шепнуть {name}?"),
                    PromptKind::Text {
                        default: String::new(),
                        max_length: MAX_MESSAGE_LEN,
                        multiline: false,
                    },
                );
                let next = ask_world(world, ask);
                world
                    .entity_mut(next)
                    .insert(TelepathyPrompt::Message(target));
            }
            (TelepathyPrompt::Message(target), Some(PromptAnswer::Text(text))) => {
                if text.is_empty() || !still_valid(world, client, actor, target) {
                    continue;
                }
                if let Some(mut spells) = world.get_mut::<Spells>(actor) {
                    spells.message = Some(text);
                }
                let Some(&pos) = world.get::<TilePos>(target) else {
                    continue;
                };
                cast(
                    world,
                    client,
                    actor,
                    Spell::Telepathy,
                    CastOn::Atom {
                        target,
                        pos,
                        right: false,
                    },
                );
            }
            _ => {}
        }
    }
}

/// `telepathy/cast`.
pub(super) fn send(world: &mut World, mob: Entity, target: Entity) -> bool {
    let Some(message) = world
        .get_mut::<Spells>(mob)
        .and_then(|mut s| s.message.take())
    else {
        return false;
    };
    let caster_name = crate::medical::name_of(world, mob);
    let target_name = crate::medical::name_of(world, target);
    info!("{mob} telepathy to {target}: {message}");
    chat_to(
        world,
        mob,
        Span::Notice,
        format!("Вы передаете {target_name}: {message}"),
    );
    let blocked = can_block_magic(world, target, MAGIC_RESISTANCE_MIND);
    if blocked {
        chat_to(
            world,
            mob,
            Span::Warning,
            "Что-то заблокировало вашу передачу!",
        );
    } else {
        chat_to(
            world,
            target,
            Span::Notice,
            format!("Вы слышите голос в своей голове... {message}"),
        );
    }
    let failure = if blocked {
        " (заблокировано антимагией)"
    } else {
        ""
    };
    let ghosts: Vec<Entity> = world
        .query_filtered::<&PlayerMob, With<crate::ghost::Ghost>>()
        .iter(world)
        .map(|p| p.client)
        .collect();
    let line = format!(
        "{caster_name} {}{failure}: {message} -> {target_name}",
        Spell::Telepathy.def().name
    );
    for client in ghosts {
        crate::tools::tell(world, client, Span::Notice, line.clone());
    }
    true
}
