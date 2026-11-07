//! The boombox and music tapes (server), after tg `code/game/objects/items/devices/boombox/`.
//!
//! - A music tape ([`MusicTape`]) holds one track (`/datum/looping_sound/boombox` subtypes:
//!   file, volume, length) and its label for examine.
//! - A tape goes in by clicking the boombox with it ("eject first!" when one is in; a recorder
//!   tape "won't fit!"). Used in hand, clicked on the floor or in the other hand with an empty
//!   hand, the boombox offers tg's radial menu as a list: play or stop, eject.
//! - Playing ([`super::music`], `Reach::Token`): the tape's track loops natively (a sound token,
//!   `use_sound_tokens`) for everyone within 12 tiles (`SOUND_RANGE` + medium extra range,
//!   falloff exponent 1), following them; stop silences it, eject stops it and drops the tape.
//! - Dragged onto its user it is picked up (`/datum/element/drag_pickup`).
// shortcut: no shoulder (swag) mode, no music note particles, no examine lines (tape inside,
// track label); the robocop tape has no track (see
// `ss13_mapload::instruments::BOOMBOX_TRACKS`) and plays nothing.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_shared::TilePos;
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, ask_world};

use super::music::{self, MusicPlayer, Reach};
use crate::inventory::{CarriedBy, Inventory};
use crate::modpc::pda::turf_of;
use crate::obj_damage::is_under;
use crate::sound::{MEDIUM_RANGE_SOUND_EXTRARANGE, PlaySound, SOUND_RANGE, playsound};

pub const BOOMBOX: &str = "/obj/item/boombox";
pub const MUSIC_TAPE: &str = "/obj/item/music_tape";
/// `/obj/item/tape` (the recorder's).
const RECORDER_TAPE: &str = "/obj/item/tape";
const SOUND_CLOSE: &str = "sound/items/taperecorder/taperecorder_close.ogg";
const SOUND_CLICK: &str = "sound/machines/click.ogg";
/// `/datum/looping_sound/boombox` `falloff_exponent`.
const FALLOFF_EXPONENT: f32 = 1.0;

/// tape type, track (`None`: not shipped), volume, `song_name`.
pub const TAPES: [(&str, Option<&str>, f32, &str); 6] = [
    (
        "/obj/item/music_tape/rock",
        Some("sound/music/boombox/bainmack-rock.ogg"),
        50.0,
        "Bainmack",
    ),
    (
        "/obj/item/music_tape/hiphop",
        Some("sound/music/boombox/groovepad-hiphop.ogg"),
        50.0,
        "Groovepad",
    ),
    (
        "/obj/item/music_tape/jazz",
        Some("sound/music/boombox/frankyboomer-jazz.ogg"),
        60.0,
        "Franky's Gonna Boom",
    ),
    (
        "/obj/item/music_tape/pop",
        Some("sound/music/boombox/bubblegum-pop.ogg"),
        50.0,
        "Bubblegum",
    ),
    (
        "/obj/item/music_tape/robocop",
        None,
        20.0,
        "Nanotrasen Company Approved Ambiance",
    ),
    (
        MUSIC_TAPE,
        Some("sound/music/boombox/industrial-crush.ogg"),
        50.0,
        "Industrial Crush",
    ),
];

/// A music tape's `song_inside` and `song_name`.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct MusicTape {
    pub track: Option<String>,
    pub volume: f32,
    pub song_name: String,
}

impl MusicTape {
    /// The tape of type `path` (the most specific entry of [`TAPES`]).
    pub fn of(path: &str) -> Option<Self> {
        TAPES
            .iter()
            .find(|(p, ..)| is_under(path, p))
            .map(|(_, track, volume, name)| Self {
                track: track.map(str::to_owned),
                volume: *volume,
                song_name: (*name).to_owned(),
            })
    }
}

/// `/obj/item/boombox`: `tapedeck`, `active`.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct Boombox {
    pub tape: Option<Entity>,
    pub active: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Choice {
    Play,
    Stop,
    Eject,
}

impl Choice {
    fn label(self) -> &'static str {
        match self {
            Self::Play => "Играть",
            Self::Stop => "Стоп",
            Self::Eject => "Извлечь",
        }
    }
}

/// On a prompt: the boombox it is the menu of, and its options.
#[derive(Component, Clone, Debug)]
struct MenuFor(Entity, Vec<Choice>);

pub(super) fn add_item(commands: &mut Commands, e: Entity, path: &str) {
    if is_under(path, BOOMBOX) {
        commands
            .entity(e)
            .try_insert_if_new((Boombox::default(), MusicPlayer::default()));
    } else if let Some(tape) = MusicTape::of(path) {
        commands.entity(e).try_insert_if_new(tape);
    }
}

fn sound(world: &mut World, e: Entity, file: &str, volume: f32, vary: bool) {
    if let Some(pos) = turf_of(world, e) {
        playsound(world, PlaySound::new(pos, file, volume, vary));
    }
}

/// `update_available_icons` + `display_radial_menu`.
fn open_menu(world: &mut World, client: Entity, mob: Entity, e: Entity) -> bool {
    let Some(b) = world.get::<Boombox>(e).cloned() else {
        return false;
    };
    // `check_menu`.
    if crate::status_effects::incapacitation(world, mob).incapacitated {
        return true;
    }
    let mut choices = vec![if b.active { Choice::Stop } else { Choice::Play }];
    if b.tape.is_some() {
        choices.push(Choice::Eject);
    }
    let title = crate::medical::name_of(world, e);
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            mob,
            title,
            "Выберите действие",
            PromptKind::List {
                items: choices.iter().map(|c| c.label().to_owned()).collect(),
                default: None,
            },
        ),
    );
    world.entity_mut(prompt).insert(MenuFor(e, choices));
    true
}

/// `attack_self`.
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    open_menu(world, client, mob, item)
}

/// `attack_hand` of a boombox on the floor (it is not picked up: no
/// `INTERACT_ITEM_ATTACK_HAND_PICKUP`).
pub fn attack_hand(world: &mut World, client: Entity, mob: Entity, e: Entity) -> bool {
    if world.get::<Boombox>(e).is_none() || world.get::<CarriedBy>(e).is_some() {
        return false;
    }
    open_menu(world, client, mob, e)
}

/// The boombox in the other hand: a tape from the active hand goes in, an empty hand opens the
/// menu.
pub fn other_hand(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Option<Entity>,
    target: Entity,
) -> bool {
    if world.get::<Boombox>(target).is_none() {
        return false;
    }
    match held {
        Some(h) => item_interaction(world, client, mob, h, target),
        None => open_menu(world, client, mob, target),
    }
}

/// `item_interaction`: a music tape goes in, a recorder tape does not fit.
pub fn item_interaction(
    world: &mut World,
    _client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let Some(b) = world.get::<Boombox>(target).cloned() else {
        return false;
    };
    if world.get::<MusicTape>(held).is_some() {
        if b.tape.is_some() {
            crate::balloon::balloon_alert(world, mob, target, "сначала извлеките!");
            return true;
        }
        world.entity_mut(held).remove::<CarriedBy>();
        if let Some(mut inventory) = world.get_mut::<Inventory>(mob)
            && let Some(slot) = inventory.hands.iter_mut().find(|h| **h == Some(held))
        {
            *slot = None;
        }
        crate::storage::put_inside(world, held, target);
        crate::inventory::refresh_look(world, mob);
        if let Some(mut b) = world.get_mut::<Boombox>(target) {
            b.tape = Some(held);
        }
        crate::balloon::balloon_alert(world, mob, target, "кассета вставлена");
        sound(world, target, SOUND_CLOSE, 50.0, false);
        return true;
    }
    if world
        .get::<DmItem>(held)
        .is_some_and(|i| is_under(&i.path, RECORDER_TAPE))
    {
        crate::balloon::balloon_alert(world, mob, target, "не влезает!");
        return true;
    }
    false
}

/// `Play`: the tape's track loops around the boombox.
fn play(world: &mut World, mob: Entity, e: Entity) {
    let Some(tape) = world.get::<Boombox>(e).and_then(|b| b.tape) else {
        crate::balloon::balloon_alert(world, mob, e, "нет кассеты!");
        return;
    };
    let Some(t) = world.get::<MusicTape>(tape).cloned() else {
        return;
    };
    if let Some(track) = &t.track {
        let reach = Reach::Token {
            max_distance: SOUND_RANGE + MEDIUM_RANGE_SOUND_EXTRARANGE,
            exponent: FALLOFF_EXPONENT,
        };
        music::start(world, e, track, t.volume, true, reach);
    }
    set_active(world, e, true);
}

fn set_active(world: &mut World, e: Entity, active: bool) {
    if let Some(mut b) = world.get_mut::<Boombox>(e) {
        b.active = active;
    }
    let state = if active { "boombox_on" } else { "boombox" };
    crate::obj_damage::set_icon_state(world, e, state);
}

/// `stop_music`.
fn stop(world: &mut World, e: Entity) {
    music::stop(world, e);
    set_active(world, e, false);
}

fn choose(world: &mut World, mob: Entity, e: Entity, choice: Choice) {
    match choice {
        Choice::Play => play(world, mob, e),
        Choice::Stop => {
            if !world.get::<Boombox>(e).is_some_and(|b| b.active) {
                crate::balloon::balloon_alert(world, mob, e, "ничего не играет!");
                return;
            }
            stop(world, e);
        }
        Choice::Eject => {
            stop(world, e);
            let tape = world.get_mut::<Boombox>(e).and_then(|mut b| b.tape.take());
            if let (Some(tape), Some(pos)) = (tape, turf_of(world, e))
                && world.get_entity(tape).is_ok()
            {
                crate::storage::take_out(world, tape, pos);
            }
        }
    }
    sound(world, e, SOUND_CLICK, 40.0, true);
}

/// The chosen option (`require_near`: the boombox at hand).
pub(super) fn menu_answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answers: Vec<(Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.actor, a.answer.clone()))
        .collect();
    for (prompt, mob, answer) in answers {
        let Some(MenuFor(e, choices)) = world.get::<MenuFor>(prompt).cloned() else {
            continue;
        };
        let Some(PromptAnswer::Item(index)) = answer else {
            continue;
        };
        let Some(&choice) = choices.get(index) else {
            continue;
        };
        if world.get_entity(e).is_err() || !near(world, mob, e) {
            continue;
        }
        choose(world, mob, e, choice);
    }
}

fn near(world: &World, mob: Entity, e: Entity) -> bool {
    if world.get::<Inventory>(mob).is_some_and(|i| i.holds(e)) {
        return true;
    }
    match (world.get::<TilePos>(e), turf_of(world, mob)) {
        (Some(&at), Some(user)) => user.chebyshev(at).is_some_and(|d| d <= 1),
        _ => false,
    }
}

/// A boombox whose tape was taken away by other means stops (`Destroy`/`handle_deconstruct`).
pub(super) fn tick(world: &mut World) {
    let lost: Vec<Entity> = world
        .query::<(Entity, &Boombox)>()
        .iter(world)
        .filter(|(_, b)| b.tape.is_some_and(|t| world.get_entity(t).is_err()))
        .map(|(e, _)| e)
        .collect();
    for e in lost {
        stop(world, e);
        if let Some(mut b) = world.get_mut::<Boombox>(e) {
            b.tape = None;
        }
    }
}

/// `/datum/element/drag_pickup`: a boombox dragged onto its user goes into a free hand.
pub fn drag_pickup(world: &mut World, mob: Entity, dragged: Entity) -> bool {
    if world.get::<Boombox>(dragged).is_none() || world.get::<TilePos>(dragged).is_none() {
        return false;
    }
    if let Some(inventory) = world.get::<Inventory>(mob).cloned()
        && let Some(hand) = [inventory.active, 1 - inventory.active]
            .into_iter()
            .find(|&h| inventory.hands[h].is_none())
    {
        crate::inventory::pick_up(world, mob, dragged, hand);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tapes_hold_their_tracks() {
        let rock = MusicTape::of("/obj/item/music_tape/rock").unwrap();
        assert_eq!(
            rock.track.as_deref(),
            Some("sound/music/boombox/bainmack-rock.ogg")
        );
        assert_eq!(rock.song_name, "Bainmack");
        let jazz = MusicTape::of("/obj/item/music_tape/jazz").unwrap();
        assert_eq!(jazz.volume, 60.0);
        assert_eq!(
            MusicTape::of(MUSIC_TAPE).unwrap().song_name,
            "Industrial Crush"
        );
        assert!(
            MusicTape::of("/obj/item/music_tape/robocop")
                .unwrap()
                .track
                .is_none()
        );
        assert!(MusicTape::of("/obj/item/tape").is_none());
    }
}
