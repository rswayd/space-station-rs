//! Password doors (server): tg `/obj/machinery/door/password` (`passworddoor.dm`), for ruins
//! and events. No hand, ID or bump opens one (`crate::doors` refuses it: `Door::password_lock`).
//!
//! - A click on the closed door (`interaction_activated`) asks for the password
//!   (`tgui_input_text`); the answer, re-checked when it comes (the door still there, closed and
//!   still, the asker next to it), opens it when it is the password exactly, else the deny sound.
//! - A voice door (`/voice`, `voice_activated`) opens when someone within hearing says a line
//!   containing the password (DM `findtext`: ignoring case); whispers carry one tile.
//! - Blast door frames and timings (`crate::doors`), `door_open`/`door_close`/`door_deny`
//!   sounds; it stays open (no autoclose), as in tg.
// tg asks for the password even with `interaction_activated` off (the var is never read); it
// documents the var as what enables the text box, so a door with it off just denies.
// shortcut: no EMP immunity element (doors take no EMP here yet) and no `ex_act` immunity beyond
// the door's `resistance_flags`.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_chat::server::{HEARING_RANGE, Spoke};
use ss13_mapload::DmDoor;
use ss13_shared::TilePos;
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, PromptSystems};

use crate::doors::{Door, DoorState, with_door};

/// tg `MAX_MESSAGE_LEN`.
const MAX_PASSWORD_LEN: u32 = 1024;

/// A password prompt open for `door`.
#[derive(Component, Clone, Copy, Debug)]
pub struct PasswordFor {
    pub door: Entity,
}

pub struct PasswordDoorPlugin;

impl Plugin for PasswordDoorPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                hear,
                password_answers
                    .in_set(ss13_tgui::server::UiSystems::Handle)
                    .after(PromptSystems::Answer),
            ),
        );
    }
}

fn closed_and_still(world: &World, door: Entity) -> bool {
    world
        .get::<Door>(door)
        .is_some_and(|d| d.password_lock && d.state == DoorState::Closed && !d.is_operating())
}

fn password_of(world: &World, door: Entity) -> Option<(String, bool, bool)> {
    let dm = world.get::<DmDoor>(door)?;
    Some((
        dm.password.clone(),
        dm.interaction_activated,
        dm.voice_activated,
    ))
}

/// `try_to_activate_door` by `mob` on the closed door: the password prompt, or the deny
/// animation when the door takes no typed password.
pub fn ask_for_pass(world: &mut World, client: Entity, mob: Entity, door: Entity) {
    if !closed_and_still(world, door) {
        return;
    }
    let Some((_, interaction, _)) = password_of(world, door) else {
        return;
    };
    if !interaction {
        deny(world, door);
        return;
    }
    let prompt = ss13_tgui::prompt::ask_world(
        world,
        Ask::new(
            client,
            mob,
            "Password",
            "Enter the password",
            PromptKind::Text {
                default: String::new(),
                max_length: MAX_PASSWORD_LEN,
                multiline: false,
            },
        ),
    );
    world.entity_mut(prompt).insert(PasswordFor { door });
}

/// `open(opener)`: the closed door opens.
pub fn open(world: &mut World, door: Entity) -> bool {
    with_door(world, door, |d, appearance, ctx| {
        let moved = d.state == DoorState::Closed && ctx.open(d, appearance, false);
        (moved, moved)
    })
    .unwrap_or(false)
}

fn deny(world: &mut World, door: Entity) {
    with_door(world, door, |d, appearance, ctx| {
        let shown = ctx.deny(d, appearance);
        ((), shown)
    });
}

/// The typed guess for `door` by `actor`, checked at the moment of effect. True when it opened.
pub fn try_password(world: &mut World, actor: Entity, door: Entity, guess: &str) -> bool {
    if !closed_and_still(world, door) || !crate::storage::in_reach(world, actor, door) {
        return false;
    }
    let Some((password, ..)) = password_of(world, door) else {
        return false;
    };
    if guess == password {
        open(world, door)
    } else {
        deny(world, door);
        false
    }
}

fn password_answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answers: Vec<(Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.actor, a.answer.clone()))
        .collect();
    for (prompt, actor, answer) in answers {
        let Some(&PasswordFor { door }) = world.get::<PasswordFor>(prompt) else {
            continue;
        };
        // A closed window is no guess (tg's `null` never equals the password).
        let Some(PromptAnswer::Text(guess)) = answer else {
            continue;
        };
        try_password(world, actor, door, &guess);
    }
}

/// `password/Hear`: a closed voice door within hearing opens on a line containing its
/// password.
pub fn hear_line(world: &mut World, pos: TilePos, text: &str, whisper: bool) {
    let range = if whisper { 1 } else { HEARING_RANGE };
    let said = text.to_lowercase();
    let mut q = world.query::<(Entity, &Door, &DmDoor, &TilePos)>();
    let doors: Vec<Entity> = q
        .iter(world)
        .filter(|(_, d, dm, at)| {
            d.password_lock
                && dm.voice_activated
                && !dm.password.is_empty()
                && d.state == DoorState::Closed
                && at.chebyshev(pos).is_some_and(|r| r <= range)
                && said.contains(&dm.password.to_lowercase())
        })
        .map(|(e, ..)| e)
        .collect();
    for door in doors {
        open(world, door);
    }
}

fn hear(world: &mut World, mut cursor: Local<MessageCursor<Spoke>>) {
    let lines: Vec<(TilePos, String, bool)> = cursor
        .read(world.resource::<Messages<Spoke>>())
        .map(|s| (s.pos, s.text.clone(), s.heard_whisper))
        .collect();
    for (pos, text, whisper) in lines {
        hear_line(world, pos, &text, whisper);
    }
}
