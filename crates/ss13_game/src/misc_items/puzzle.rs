//! Ruin puzzle keycards and their doors (tg `code/game/objects/items/puzzle_pieces.dm`):
//!
//! - A keycard door (`/obj/machinery/door/puzzle/keycard`) opens for good when a keycard with
//!   its `puzzle_id` (any keycard when it has none) is swiped on it; a wrong one buzzes. No hand
//!   opens it (`can_open_with_hands`, `crate::doors`).
//! - A keycard panel (`/obj/machinery/puzzle/keycardpad`) takes the keycard of its `id` once,
//!   and a puzzle button (`/obj/machinery/puzzle/button`) is pressed (once when
//!   `single_use`): both complete their puzzle, opening every puzzle door that waits on it (tg
//!   `SSqueuelinks`: doors with `uses_queuelinks` and that `puzzle_id`).
// shortcut: the queue links pair every waiting door of an id, not `queue_size` partners in map
// load order; no password panels, light puzzles, pressure plates or holoboxes (their own ruin
// entries).

use bevy::prelude::*;
use ss13_mapload::fidelity::{IconIndex, is_under};
use ss13_mapload::puzzle::{KEYCARD_DOOR_ROOT, KEYCARD_ROOT, PUZZLE_DOOR_ROOT};
use ss13_mapload::{DmAtom, DmPuzzle};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, TilePos};

use crate::doors::{Door, DoorState};
use crate::sound::{PlaySound, playsound};
use crate::tools::tell;

const KEYCARD_PAD: &str = "/obj/machinery/puzzle/keycardpad";
const PUZZLE_BUTTON: &str = "/obj/machinery/puzzle/button";
const CARD_SLIDE: &str = "sound/machines/card_slide.ogg";
const BEEP: &str = "sound/machines/beep/beep.ogg";
const BUTTON: &str = "sound/machines/terminal/terminal_button07.ogg";

/// `/obj/machinery/puzzle`: `used`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PuzzleUsed;

/// A puzzle door whose link fired (`try_signal`: `puzzle_id = null`).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PuzzleSolved;

pub(super) fn build(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    // `puzzle/keycard/item_interaction`, `keycardpad/attackby`.
    app.add_click_handler(ClickPhase::HeldOnTarget, 317, |w, c| {
        c.held
            .is_some_and(|h| swipe(w, c.client, c.mob, h, c.target))
    });
    // `puzzle/button/attack_hand`.
    app.add_click_handler(ClickPhase::EmptyHand, 2017, |w, c| {
        press(w, c.mob, c.target)
    });
}

fn puzzle(world: &World, e: Entity) -> Option<&DmPuzzle> {
    world.get::<DmPuzzle>(e)
}

fn path_is(world: &World, e: Entity, root: &str) -> bool {
    puzzle(world, e).is_some_and(|p| is_under(&p.path, root))
}

/// A keycard on a keycard door or panel. True when it was a keycard on one of them.
fn swipe(world: &mut World, client: Entity, mob: Entity, card: Entity, target: Entity) -> bool {
    if !path_is(world, card, KEYCARD_ROOT) {
        return false;
    }
    let key = puzzle(world, card)
        .map(|p| p.puzzle_id.clone())
        .unwrap_or_default();
    if path_is(world, target, KEYCARD_DOOR_ROOT) {
        if !try_puzzle_open(world, target, Some(&key)) {
            let name = crate::medical::name_of(world, target);
            tell(
                world,
                client,
                Span::Notice,
                format!("{name} пищит. Похоже, это не тот ключ."),
            );
        }
        return true;
    }
    if path_is(world, target, KEYCARD_PAD) {
        swipe_pad(world, mob, target, &key);
        return true;
    }
    false
}

/// `keycardpad/attackby`.
fn swipe_pad(world: &mut World, mob: Entity, pad: Entity, key: &str) {
    if world.get::<PuzzleUsed>(pad).is_some() {
        return;
    }
    let Some(&pos) = world.get::<TilePos>(pad) else {
        return;
    };
    let id = puzzle(world, pad).map(|p| p.id.clone()).unwrap_or_default();
    let correct = key == id;
    let text = if correct {
        "карта подошла"
    } else {
        "неверная карта!"
    };
    crate::medical::visible(world, mob, text, text, Span::Notice);
    playsound(world, PlaySound::new(pos, CARD_SLIDE, 45.0, true));
    if !correct {
        return;
    }
    use_up(world, pad);
    playsound(world, PlaySound::new(pos, BEEP, 45.0, true));
    complete(world, &id);
}

/// `puzzle/button/attack_hand`. True when `target` is a puzzle button.
fn press(world: &mut World, mob: Entity, button: Entity) -> bool {
    if !path_is(world, button, PUZZLE_BUTTON) {
        return false;
    }
    let single_use = puzzle(world, button).is_some_and(|p| p.single_use);
    if world.get::<PuzzleUsed>(button).is_some() && single_use {
        return true;
    }
    let Some(&pos) = world.get::<TilePos>(button) else {
        return true;
    };
    if single_use {
        use_up(world, button);
    }
    let (user, name) = (
        crate::medical::name_of(world, mob),
        crate::medical::name_of(world, button),
    );
    crate::medical::visible(
        world,
        mob,
        &format!("Вы нажимаете кнопку на {name}."),
        &format!("{user} нажимает кнопку на {name}."),
        Span::Notice,
    );
    playsound(world, PlaySound::new(pos, BUTTON, 45.0, true));
    let id = puzzle(world, button)
        .map(|p| p.id.clone())
        .unwrap_or_default();
    complete(world, &id);
    true
}

/// `used = TRUE` and `update_icon_state`: `[base_icon_state][used]`.
fn use_up(world: &mut World, gizmo: Entity) {
    world.entity_mut(gizmo).insert(PuzzleUsed);
    let base = puzzle(world, gizmo)
        .map(|p| p.base_icon_state.clone())
        .unwrap_or_default();
    let icon = world.get::<DmAtom>(gizmo).map(|a| a.icon.clone());
    let state = icon.and_then(|icon| {
        world
            .get_resource::<IconIndex>()?
            .get(&icon, &format!("{base}1"))
    });
    if let (Some(state), Some(mut look)) = (state, world.get_mut::<Appearance>(gizmo)) {
        look.icon_state = state;
    }
}

/// `on_puzzle_complete` -> `COMSIG_PUZZLE_COMPLETED` -> the linked doors' `try_signal`. A panel
/// without an `id` never queued, so it links nothing.
fn complete(world: &mut World, id: &str) {
    if id.is_empty() {
        return;
    }
    let mut doors = world.query::<(Entity, &DmPuzzle)>();
    let linked: Vec<Entity> = doors
        .iter(world)
        .filter(|(_, p)| {
            is_under(&p.path, PUZZLE_DOOR_ROOT) && p.uses_queuelinks && p.puzzle_id == id
        })
        .map(|(e, _)| e)
        .collect();
    for door in linked {
        if world.get::<PuzzleSolved>(door).is_some() {
            continue;
        }
        world.entity_mut(door).insert(PuzzleSolved);
        try_puzzle_open(world, door, None);
    }
}

/// `door/puzzle/try_puzzle_open(try_id)`: a wrong id is refused; an open door stays open.
fn try_puzzle_open(world: &mut World, door: Entity, try_id: Option<&str>) -> bool {
    let solved = world.get::<PuzzleSolved>(door).is_some();
    let own = puzzle(world, door)
        .map(|p| p.puzzle_id.clone())
        .unwrap_or_default();
    if !solved && !own.is_empty() && Some(own.as_str()) != try_id {
        return false;
    }
    let closed = world
        .get::<Door>(door)
        .is_some_and(|d| d.state == DoorState::Closed);
    if !closed {
        crate::medical::visible(
            world,
            door,
            "",
            "Дверь, кажется, не закрыть.",
            Span::Warning,
        );
        return true;
    }
    let message = puzzle(world, door)
        .map(|p| p.open_message.clone())
        .unwrap_or_default();
    if !message.is_empty() {
        crate::medical::visible(world, door, "", &message, Span::Notice);
    }
    crate::password_door::open(world, door);
    true
}
