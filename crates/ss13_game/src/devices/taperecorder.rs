//! Universal recorders and tapes (server), after tg `taperecorder.dm` and BandaStation's
//! `/datum/tape_message`.
//!
//! - A recorder ([`TapeRecorder`]) starts with a random tape (`/obj/item/taperecorder/empty`:
//!   none). Used in hand it offers tg's radial menu as a list: record, play, print a
//!   transcript, stop, eject. A tape goes in by clicking the recorder with it; an empty hand on
//!   a held recorder takes the tape out.
//! - Recording: every second takes a second of the tape (10 minutes a side), with a warning a
//!   minute before the end; speech heard meanwhile is stored with the tape time and the
//!   speaker's name. Playback says the stored lines with their timestamps at the recorded
//!   pace, a silence longer than 14 s announced and skipped.
//! - A transcript is printed on papers of up to 5000 characters, the last into the hands; then
//!   the recorder needs 30 s.
//! - A tape ([`Tape`]) gets a colour suffix and may start flipped; used in hand: unwind it or
//!   flip it (two sides); an unwound tape plays and records nothing until a screwdriver
//!   respools it (12 s).
// shortcut: hearing ignores walls and radios (every local line within 7 tiles, 1 for
// whispers); no TTS voice, hiss sound loop, tape recorder speech span, alt-click play, fire
// unspooling, music tapes or pre-recorded ruin tapes, and the greyscale tape is not tinted.
// Cost: one pass over the recording recorders per spoken line; the recording and playing ones
// once a second.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_chat::server::{HEARING_RANGE, Spoke};
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmAtom, DmItem, DmName, IconIndex};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Overlay, TilePos};
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, ask_world};

use crate::health::Stat;
use crate::inventory::{CarriedBy, Inventory};
use crate::modpc::pda::turf_of;
use crate::sound::{PlaySound, playsound};
use crate::tools::tell;

const RECORDER: &str = "/obj/item/taperecorder";
const EMPTY_RECORDER: &str = "/obj/item/taperecorder/empty";
const TAPE: &str = "/obj/item/tape";
const RANDOM_TAPE: &str = "/obj/item/tape/random";
const PAPER: &str = "/obj/item/paper";
const RANDOM_COLOURS: [&str; 6] = ["white", "blue", "red", "yellow", "purple", "greyscale"];
/// `max_capacity = 10 MINUTES`, in seconds.
pub const TAPE_CAPACITY: u32 = 600;
/// `time_left_warning = 60 SECONDS`.
const WARNING_SECS: u32 = 60;
/// `addtimer(VARSET_CALLBACK(src, canprint, TRUE), 30 SECONDS)`.
const PRINT_COOLDOWN: f64 = 30.0;
/// `MAX_PAPER_LENGTH`.
const MAX_PAPER_LENGTH: usize = 5000;
/// `use_tool(src, user, 12 SECONDS)`, deciseconds.
const RESPOOL_DS: f32 = 120.0;
const SOUND_PLAY: &str = "sound/items/taperecorder/taperecorder_play.ogg";
const SOUND_STOP: &str = "sound/items/taperecorder/taperecorder_stop.ogg";
const SOUND_OPEN: &str = "sound/items/taperecorder/taperecorder_open.ogg";
const SOUND_CLOSE: &str = "sound/items/taperecorder/taperecorder_close.ogg";
const SOUND_PRINT: &str = "sound/items/taperecorder/taperecorder_print.ogg";
const SOUND_FLIP: &str = "sound/items/taperecorder/tape_flip.ogg";

/// `/datum/tape_message`: the tape time (s), the speaker's voice name and the words.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TapeMessage {
    pub at: u32,
    pub speaker: String,
    pub text: String,
}

impl TapeMessage {
    /// `get_composed_message`: "[mm:ss] Имя: текст".
    pub fn composed(&self) -> String {
        format!(
            "[{:02}:{:02}] {}: {}",
            self.at / 60,
            self.at % 60,
            self.speaker,
            self.text
        )
    }
}

/// One side of a tape: `storedinfo`/`timestamp` and `used_capacity`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TapeSide {
    pub messages: Vec<TapeMessage>,
    pub used: u32,
}

/// `/obj/item/tape`.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct Tape {
    pub side: TapeSide,
    pub other: TapeSide,
    pub unspooled: bool,
    pub max: u32,
    /// `initial_icon_state` (a random tape's chosen colour).
    base_state: String,
    reversed: bool,
    ready: bool,
}

impl Tape {
    /// `tapeflip`.
    pub fn flip(&mut self) {
        std::mem::swap(&mut self.side, &mut self.other);
        self.reversed = !self.reversed;
    }
}

/// `/obj/item/taperecorder`.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct TapeRecorder {
    pub tape: Option<Entity>,
    pub recording: bool,
    pub playing: bool,
    /// Game seconds printing is possible again (`canprint`).
    pub print_ready_at: f64,
    /// `time_warned`.
    warned: bool,
    /// The next record or playback step.
    next_step: f64,
    /// The next stored line to say.
    play_index: usize,
    /// A silence (s) to announce at the next step.
    skip: Option<u32>,
    /// `usr` of the running record or playback loop (its alerts).
    client: Option<Entity>,
    /// `starting_tape_type` still to spawn.
    needs_tape: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Choice {
    Record,
    Play,
    Print,
    Stop,
    Eject,
    Unwind,
    Flip,
}

impl Choice {
    fn label(self) -> &'static str {
        match self {
            Self::Record => "Записать",
            Self::Play => "Воспроизвести",
            Self::Print => "Распечатать расшифровку",
            Self::Stop => "Стоп",
            Self::Eject => "Извлечь",
            Self::Unwind => "Размотать плёнку",
            Self::Flip => "Перевернуть кассету",
        }
    }
}

/// On a prompt: the recorder or tape it is the radial menu of, and its options.
#[derive(Component, Clone, Debug)]
struct MenuFor(Entity, Vec<Choice>);

pub(super) fn build(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    // The radial menus of recorders and tapes.
    app.add_click_handler(ClickPhase::AttackSelf, 1740, |w, c| {
        crate::devices::taperecorder::attack_self(w, c.client, c.mob, c.target)
    });
    // A tape into a recorder, a screwdriver on an unwound tape (on the floor).
    app.add_click_handler(ClickPhase::HeldOnTarget, 1565, |w, c| {
        c.held.is_some_and(|h| {
            crate::devices::taperecorder::item_interaction(w, c.client, c.mob, h, c.target)
        })
    });
    // The same in the other hand; an empty hand ejects a held recorder's tape.
    app.add_click_handler(ClickPhase::OtherHand, 465, |w, c| match c.held {
        Some(h) => crate::devices::taperecorder::item_interaction(w, c.client, c.mob, h, c.target),
        None => crate::devices::taperecorder::attack_hand(w, c.client, c.mob, c.target),
    });
    app.add_observer(add_parts).add_systems(
        Update,
        (
            setup,
            hear,
            tick,
            menu_answers
                .in_set(ss13_tgui::server::UiSystems::Handle)
                .after(ss13_tgui::prompt::PromptSystems::Answer),
        ),
    );
}

fn add_parts(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    let Ok(item) = items.get(add.entity) else {
        return;
    };
    if is_under(&item.path, RECORDER) {
        commands.entity(add.entity).try_insert_if_new(TapeRecorder {
            needs_tape: !is_under(&item.path, EMPTY_RECORDER),
            ..default()
        });
    } else if is_under(&item.path, TAPE) {
        commands.entity(add.entity).try_insert_if_new(Tape {
            max: TAPE_CAPACITY,
            ..default()
        });
    }
}

/// `Initialize` of new recorders (the starting tape) and tapes (the colour suffix, a random
/// tape's colour, `prob(50)` flipped).
fn setup(world: &mut World) {
    let tapes: Vec<Entity> = world
        .query::<(Entity, &Tape)>()
        .iter(world)
        .filter(|(_, t)| !t.ready)
        .map(|(e, _)| e)
        .collect();
    for e in tapes {
        setup_tape(world, e);
    }
    let recorders: Vec<Entity> = world
        .query::<(Entity, &TapeRecorder)>()
        .iter(world)
        .filter(|(_, r)| r.needs_tape)
        .map(|(e, _)| e)
        .collect();
    for e in recorders {
        let Some(pos) = turf_of(world, e) else {
            continue;
        };
        if let Some(mut r) = world.get_mut::<TapeRecorder>(e) {
            r.needs_tape = false;
        }
        if let Some(tape) = crate::construction::spawn_obj(world, RANDOM_TAPE, pos) {
            crate::storage::put_inside(world, tape, e);
            set_recorder(world, e, |r| r.tape = Some(tape));
        }
    }
}

fn setup_tape(world: &mut World, e: Entity) {
    let Some(path) = world.get::<DmItem>(e).map(|i| i.path.clone()) else {
        return;
    };
    if world.get::<DmName>(e).is_none() {
        // Prototype parts arrive one at a time: wait for the name.
        return;
    }
    let (colour, suffix, flipped) = {
        let mut rng = world.resource_mut::<crate::combat::CombatRng>();
        let colour = is_under(&path, RANDOM_TAPE)
            .then(|| RANDOM_COLOURS[rng.range(0, RANDOM_COLOURS.len() as u32 - 1) as usize]);
        // `random_short_color()`: three hex digits.
        let suffix = format!("{:03X}", rng.range(0, 0xFFF));
        (colour, suffix, rng.chance(50.0))
    };
    if let Some(colour) = colour {
        crate::obj_damage::set_icon_state(world, e, &format!("tape_{colour}"));
    }
    let base_state = match colour {
        Some(c) => format!("tape_{c}"),
        None => world
            .get::<DmAtom>(e)
            .map(|a| a.icon_state.clone())
            .unwrap_or_default(),
    };
    if let Some(mut name) = world.get_mut::<DmName>(e) {
        name.name = format!("{} ({suffix})", name.name);
    }
    if let Some(mut tape) = world.get_mut::<Tape>(e) {
        tape.ready = true;
        tape.base_state = base_state;
        if flipped {
            tape.flip();
        }
    }
    refresh_tape(world, e);
}

fn set_recorder(world: &mut World, e: Entity, f: impl FnOnce(&mut TapeRecorder)) {
    if let Some(mut r) = world.get_mut::<TapeRecorder>(e) {
        f(&mut r);
    }
    refresh_recorder(world, e);
}

/// `update_icon_state`.
fn refresh_recorder(world: &mut World, e: Entity) {
    let Some(r) = world.get::<TapeRecorder>(e) else {
        return;
    };
    let state = match (r.tape, r.recording, r.playing) {
        (None, ..) => "taperecorder_empty",
        (_, true, _) => "taperecorder_recording",
        (_, _, true) => "taperecorder_playing",
        _ => "taperecorder_idle",
    };
    crate::obj_damage::set_icon_state(world, e, state);
}

/// The flipped side's `_reverse` state and the `ribbonoverlay` of an unwound tape.
fn refresh_tape(world: &mut World, e: Entity) {
    let Some(tape) = world.get::<Tape>(e).cloned() else {
        return;
    };
    let state = if tape.reversed {
        format!("{}_reverse", tape.base_state)
    } else {
        tape.base_state.clone()
    };
    crate::obj_damage::set_icon_state(world, e, &state);
    let icon = world
        .get::<DmAtom>(e)
        .map(|a| a.icon.clone())
        .unwrap_or_default();
    let overlays: Vec<Overlay> = tape
        .unspooled
        .then(|| {
            world
                .get_resource::<IconIndex>()?
                .get(&icon, "ribbonoverlay")
        })
        .flatten()
        .map(Overlay::new)
        .into_iter()
        .collect();
    if let Some(mut appearance) = world.get_mut::<Appearance>(e)
        && appearance.overlays != overlays
    {
        appearance.overlays = overlays;
    }
}

fn sound(world: &mut World, e: Entity, file: &str, volume: f32) {
    if let Some(pos) = turf_of(world, e) {
        playsound(world, PlaySound::new(pos, file, volume, false));
    }
}

fn alert(world: &mut World, client: Option<Entity>, text: impl Into<String>) {
    if let Some(client) = client {
        tell(world, client, Span::Notice, text);
    }
}

/// `can_use`: a conscious, able user.
fn can_use(world: &World, mob: Entity) -> bool {
    world.get::<Stat>(mob) == Some(&Stat::Conscious)
        && !crate::status_effects::incapacitation(world, mob).incapacitated
}

/// The item's `say(text)`: its name and the words to every player within hearing of its turf.
fn item_say(world: &mut World, item: Entity, text: &str) {
    let Some(pos) = turf_of(world, item) else {
        return;
    };
    let name = world
        .get::<DmName>(item)
        .map(|n| crate::vending::plain_name(&n.name).to_owned())
        .unwrap_or_default();
    let mut query = world.query_filtered::<Entity, With<ss13_net::server::ClientToken>>();
    let clients: Vec<Entity> = query
        .iter(world)
        .filter(|&c| {
            let mob = world.resource::<crate::server::Players>().mob(c);
            mob.and_then(|m| turf_of(world, m))
                .and_then(|p| p.chebyshev(pos))
                .is_some_and(|d| d <= HEARING_RANGE)
        })
        .collect();
    if !clients.is_empty() {
        world.write_message(ss13_net::chat::server::SendChat {
            clients,
            message: ss13_chat::server::say_message(&name, text),
        });
    }
}

fn tape_name(world: &World, tape: Entity) -> String {
    crate::medical::name_of(world, tape)
}

/// `attack_self` of a recorder or a tape: the radial menu. True when `item` is one.
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    let choices = if let Some(r) = world.get::<TapeRecorder>(item).cloned() {
        let Some(tape) = r.tape else {
            tell(world, client, Span::Notice, "Он пуст!");
            return true;
        };
        let mut choices = Vec::new();
        if !r.playing && !r.recording {
            choices.extend([Choice::Record, Choice::Play]);
            let stored = world
                .get::<Tape>(tape)
                .is_some_and(|t| !t.side.messages.is_empty());
            if stored && now(world) >= r.print_ready_at {
                choices.push(Choice::Print);
            }
        } else {
            choices.push(Choice::Stop);
        }
        choices.push(Choice::Eject);
        choices
    } else if let Some(tape) = world.get::<Tape>(item) {
        let mut choices = Vec::new();
        if !tape.unspooled {
            choices.push(Choice::Unwind);
        }
        choices.push(Choice::Flip);
        choices
    } else {
        return false;
    };
    let title = crate::medical::name_of(world, item);
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
    world.entity_mut(prompt).insert(MenuFor(item, choices));
    true
}

fn now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

/// The chosen option, if the user still can act and has the item at hand (`require_near`).
fn menu_answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answers: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, mob, answer) in answers {
        let Some(MenuFor(item, choices)) = world.get::<MenuFor>(prompt).cloned() else {
            continue;
        };
        let Some(PromptAnswer::Item(index)) = answer else {
            continue;
        };
        let Some(&choice) = choices.get(index) else {
            continue;
        };
        if world.get_entity(item).is_err() || !near(world, mob, item) {
            continue;
        }
        choose(world, client, mob, item, choice);
    }
}

/// The item is in the mob's hands or on a tile next to it.
fn near(world: &World, mob: Entity, item: Entity) -> bool {
    if world.get::<Inventory>(mob).is_some_and(|i| i.holds(item)) {
        return true;
    }
    match (world.get::<TilePos>(item), world.get::<TilePos>(mob)) {
        (Some(&at), Some(&user)) => user.chebyshev(at).is_some_and(|d| d <= 1),
        _ => false,
    }
}

fn held_by(world: &World, mob: Entity, item: Entity) -> bool {
    world.get::<CarriedBy>(item).is_some_and(|c| c.0 == mob)
}

fn choose(world: &mut World, client: Entity, mob: Entity, item: Entity, choice: Choice) {
    match choice {
        Choice::Unwind | Choice::Flip => {
            // `if(loc != user) return`.
            if !held_by(world, mob, item) {
                return;
            }
            if choice == Choice::Flip {
                if let Some(mut tape) = world.get_mut::<Tape>(item) {
                    tape.flip();
                }
                tell(world, client, Span::Notice, "кассета перевёрнута");
                sound(world, item, SOUND_FLIP, 70.0);
            } else {
                if let Some(mut tape) = world.get_mut::<Tape>(item) {
                    tape.unspooled = true;
                }
                tell(world, client, Span::Notice, "плёнка размотана");
            }
            refresh_tape(world, item);
        }
        Choice::Eject => eject(world, client, mob, item),
        // The verbs check `can_use` (eject does not).
        _ if !can_use(world, mob) => tell(world, client, Span::Notice, "не получается!"),
        Choice::Record => record(world, client, item),
        Choice::Play => play(world, client, item),
        Choice::Stop => stop(world, Some(client), item),
        Choice::Print => print_transcript(world, client, mob, item),
    }
}

/// The recorder's spooled tape, or the `"no spooled tape!"` alert.
fn spooled_tape(world: &mut World, client: Entity, r: &TapeRecorder) -> Option<Entity> {
    let tape = r
        .tape
        .filter(|&t| world.get::<Tape>(t).is_some_and(|t| !t.unspooled));
    if tape.is_none() {
        tell(world, client, Span::Notice, "нет намотанной кассеты!");
    }
    tape
}

/// `busy` checks shared by record, play and print.
fn busy(world: &mut World, client: Entity, r: &TapeRecorder) -> bool {
    if r.recording {
        tell(world, client, Span::Notice, "сначала остановите запись!");
    } else if r.playing {
        tell(world, client, Span::Notice, "уже проигрывается!");
    }
    r.recording || r.playing
}

/// `record()`: starts the per-second loop (`tick`).
fn record(world: &mut World, client: Entity, e: Entity) {
    let Some(r) = world.get::<TapeRecorder>(e).cloned() else {
        return;
    };
    let Some(tape) = spooled_tape(world, client, &r) else {
        return;
    };
    if busy(world, client, &r) {
        return;
    }
    sound(world, e, SOUND_PLAY, 50.0);
    let full = world.get::<Tape>(tape).is_none_or(|t| t.side.used >= t.max);
    if full {
        tell(world, client, Span::Notice, "кассета заполнена!");
        sound(world, e, SOUND_STOP, 50.0);
        return;
    }
    tell(world, client, Span::Notice, "запись начата");
    let at = now(world);
    set_recorder(world, e, |r| {
        r.recording = true;
        r.next_step = at;
        r.client = Some(client);
    });
}

/// `play()`: starts the playback loop (`tick`).
fn play(world: &mut World, client: Entity, e: Entity) {
    let Some(r) = world.get::<TapeRecorder>(e).cloned() else {
        return;
    };
    let Some(tape) = spooled_tape(world, client, &r) else {
        return;
    };
    if busy(world, client, &r) {
        return;
    }
    if world
        .get::<Tape>(tape)
        .is_none_or(|t| t.side.messages.is_empty())
    {
        let name = tape_name(world, tape);
        tell(world, client, Span::Notice, format!("{name} пуста!"));
        return;
    }
    tell(world, client, Span::Notice, "начало проигрыша записи");
    sound(world, e, SOUND_PLAY, 50.0);
    let at = now(world);
    set_recorder(world, e, |r| {
        r.playing = true;
        r.next_step = at;
        r.play_index = 0;
        r.skip = None;
        r.client = Some(client);
    });
}

/// `stop()`.
fn stop(world: &mut World, client: Option<Entity>, e: Entity) {
    let Some(r) = world.get::<TapeRecorder>(e).cloned() else {
        return;
    };
    if r.recording {
        sound(world, e, SOUND_STOP, 50.0);
        alert(world, client, "запись остановлена");
    } else if r.playing {
        sound(world, e, SOUND_STOP, 50.0);
        alert(world, client, "проигрыш остановлен");
    }
    set_recorder(world, e, |r| {
        r.recording = false;
        r.playing = false;
        r.warned = false;
        r.skip = None;
    });
}

/// `eject(user)`: the tape into the user's hands.
fn eject(world: &mut World, client: Entity, mob: Entity, e: Entity) {
    let Some(r) = world.get::<TapeRecorder>(e).cloned() else {
        return;
    };
    let Some(tape) = r.tape.filter(|&t| world.get_entity(t).is_ok()) else {
        tell(world, client, Span::Notice, "кассеты нет!");
        return;
    };
    if r.playing {
        tell(world, client, Span::Notice, "сначала остановите кассету!");
        return;
    }
    sound(world, e, SOUND_OPEN, 50.0);
    let name = tape_name(world, tape);
    tell(world, client, Span::Notice, format!("извлечена {name}"));
    stop(world, Some(client), e);
    set_recorder(world, e, |r| r.tape = None);
    give(world, mob, e, tape);
}

/// `user.put_in_hands(item)`, else onto the source's turf.
fn give(world: &mut World, mob: Entity, source: Entity, item: Entity) {
    let Some(pos) = turf_of(world, source) else {
        return;
    };
    crate::storage::take_out(world, item, pos);
    if let Some(inventory) = world.get::<Inventory>(mob).cloned()
        && let Some(hand) = [inventory.active, 1 - inventory.active]
            .into_iter()
            .find(|&h| inventory.hands[h].is_none())
    {
        crate::inventory::pick_up(world, mob, item, hand);
    }
}

/// `attack_hand` of a held recorder with a tape: ejects it. True when it did.
pub fn attack_hand(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    let has_tape = world
        .get::<TapeRecorder>(target)
        .is_some_and(|r| r.tape.is_some());
    if !has_tape || !held_by(world, mob, target) {
        return false;
    }
    eject(world, client, mob, target);
    true
}

/// A tape into a recorder (`item_interaction`), a screwdriver on an unwound tape
/// (`screwdriver_act`). True when the click was ours.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if let Some(r) = world.get::<TapeRecorder>(target) {
        if r.tape.is_some() || world.get::<Tape>(held).is_none() {
            return false;
        }
        world.entity_mut(held).remove::<CarriedBy>();
        if let Some(mut inventory) = world.get_mut::<Inventory>(mob)
            && let Some(slot) = inventory.hands.iter_mut().find(|h| **h == Some(held))
        {
            *slot = None;
        }
        crate::storage::put_inside(world, held, target);
        let name = tape_name(world, held);
        tell(world, client, Span::Notice, format!("вставлена {name}"));
        sound(world, target, SOUND_CLOSE, 50.0);
        set_recorder(world, target, |r| r.tape = Some(held));
        crate::inventory::refresh_look(world, mob);
        return true;
    }
    let unspooled = world.get::<Tape>(target).is_some_and(|t| t.unspooled);
    let screwdriver = matches!(
        crate::tools::tool_of(world, held),
        Some((crate::tools::ToolKind::Screwdriver, _))
    );
    if !unspooled || !screwdriver {
        return false;
    }
    tell(world, client, Span::Notice, "перемотка плёнки...");
    let actor = crate::tools::Actor {
        client,
        mob,
        item: Some(held),
    };
    // The tape may be in a hand (no tile of its own): the do_after is on the user.
    crate::tools::use_tool(
        world,
        actor,
        mob,
        RESPOOL_DS,
        0,
        crate::construction::Job::TapeRespool(target),
    );
    true
}

/// The respool do_after's end: the tape is still at hand.
pub fn respool_complete(world: &mut World, mob: Entity, client: Entity, tape: Entity) {
    if world.get_entity(tape).is_err() || !near(world, mob, tape) {
        return;
    }
    if let Some(mut t) = world.get_mut::<Tape>(tape) {
        t.unspooled = false;
    }
    refresh_tape(world, tape);
    tell(world, client, Span::Notice, "плёнка перемотана");
}

/// `print_transcript()`: papers of up to [`MAX_PAPER_LENGTH`] characters on the recorder's
/// turf, the last one into the hands.
fn print_transcript(world: &mut World, client: Entity, mob: Entity, e: Entity) {
    let Some(r) = world.get::<TapeRecorder>(e).cloned() else {
        return;
    };
    let messages = r
        .tape
        .and_then(|t| world.get::<Tape>(t))
        .map(|t| t.side.messages.clone())
        .unwrap_or_default();
    if messages.is_empty() {
        tell(world, client, Span::Notice, "кассета пуста!");
        return;
    }
    if now(world) < r.print_ready_at {
        tell(world, client, Span::Notice, "нельзя печатать так быстро!");
        return;
    }
    let Some(tape) = spooled_tape(world, client, &r) else {
        return;
    };
    if busy(world, client, &r) {
        return;
    }
    let Some(pos) = turf_of(world, e) else {
        return;
    };
    let tape_label = world
        .get::<DmName>(tape)
        .map(|n| n.name.clone())
        .unwrap_or_default();
    let pages = transcript_pages(&messages);
    let count = pages.len();
    let mut last = None;
    for (n, text) in pages.into_iter().enumerate() {
        let Some(paper) = crate::construction::spawn_obj(world, PAPER, pos) else {
            return;
        };
        world.entity_mut(paper).insert(DmName {
            name: format!("бумага — расшифровка «{tape_label}», стр. {}", n + 1),
            desc: String::new(),
        });
        crate::service::write_printed(world, paper, text);
        last = Some(paper);
    }
    tell(
        world,
        client,
        Span::Notice,
        format!("расшифровка напечатана, страниц: {count}"),
    );
    sound(world, e, SOUND_PRINT, 50.0);
    if let Some(paper) = last {
        give(world, mob, e, paper);
    }
    let ready = now(world) + PRINT_COOLDOWN;
    set_recorder(world, e, |r| r.print_ready_at = ready);
}

/// The transcript split into pages of at most [`MAX_PAPER_LENGTH`] characters.
pub fn transcript_pages(messages: &[TapeMessage]) -> Vec<String> {
    let mut pages = Vec::new();
    let mut text = "<b>Расшифровка:</b><br><br>".to_owned();
    for message in messages {
        let line = format!("{}<br>", message.composed());
        if text.chars().count() + line.chars().count() > MAX_PAPER_LENGTH {
            pages.push(std::mem::take(&mut text));
        }
        text.push_str(&line);
    }
    pages.push(text);
    pages
}

/// `Hear()` of the recording recorders: the line at the tape's current time.
fn hear(world: &mut World, mut cursor: Local<MessageCursor<Spoke>>) {
    let lines: Vec<(TilePos, String, String, bool)> = cursor
        .read(world.resource::<Messages<Spoke>>())
        .map(|s| (s.pos, s.name.clone(), s.text.clone(), s.heard_whisper))
        .collect();
    if lines.is_empty() {
        return;
    }
    let recording: Vec<(Entity, Entity)> = world
        .query::<(Entity, &TapeRecorder)>()
        .iter(world)
        .filter(|(_, r)| r.recording)
        .filter_map(|(e, r)| Some((e, r.tape?)))
        .collect();
    for (pos, speaker, text, whisper) in lines {
        let range = if whisper { 1 } else { HEARING_RANGE };
        for &(recorder, tape) in &recording {
            let heard = turf_of(world, recorder)
                .and_then(|at| at.chebyshev(pos))
                .is_some_and(|d| d <= range);
            if !heard {
                continue;
            }
            if let Some(mut t) = world.get_mut::<Tape>(tape) {
                let at = t.side.used;
                t.side.messages.push(TapeMessage {
                    at,
                    speaker: speaker.clone(),
                    text: text.clone(),
                });
            }
        }
    }
}

/// The record and playback loops, one step a second (`sleep(1 SECONDS)`).
fn tick(world: &mut World) {
    let at = now(world);
    let due: Vec<Entity> = world
        .query::<(Entity, &TapeRecorder)>()
        .iter(world)
        .filter(|(_, r)| (r.recording || r.playing) && at >= r.next_step)
        .map(|(e, _)| e)
        .collect();
    for e in due {
        let Some(r) = world.get::<TapeRecorder>(e).cloned() else {
            continue;
        };
        let tape = r.tape.filter(|&t| world.get::<Tape>(t).is_some());
        let Some(tape) = tape else {
            stop(world, r.client, e);
            continue;
        };
        if r.recording {
            record_step(world, e, tape, &r, at);
        } else {
            play_step(world, e, tape, &r, at);
        }
    }
}

fn record_step(world: &mut World, e: Entity, tape: Entity, r: &TapeRecorder, at: f64) {
    let Some((used, max)) = world.get::<Tape>(tape).map(|t| (t.side.used, t.max)) else {
        return;
    };
    if used >= max {
        alert(world, r.client, "кассета заполнена!");
        stop(world, r.client, e);
        return;
    }
    let used = used + 1;
    if let Some(mut t) = world.get_mut::<Tape>(tape) {
        t.side.used = used;
    }
    let warn = max - used < WARNING_SECS && !r.warned;
    if warn {
        alert(world, r.client, format!("осталось {} с", max - used));
    }
    if let Some(mut r) = world.get_mut::<TapeRecorder>(e) {
        r.warned |= warn;
        r.next_step = at + 1.0;
    }
}

fn play_step(world: &mut World, e: Entity, tape: Entity, r: &TapeRecorder, at: f64) {
    if let Some(silence) = r.skip {
        item_say(world, e, &format!("Пропуск {silence} секунд тишины."));
        if let Some(mut r) = world.get_mut::<TapeRecorder>(e) {
            r.skip = None;
            r.next_step = at + 1.0;
        }
        return;
    }
    let Some(messages) = world.get::<Tape>(tape).map(|t| t.side.messages.clone()) else {
        return;
    };
    let i = r.play_index;
    let Some(message) = messages.get(i) else {
        alert(world, r.client, "запись закончена");
        stop(world, r.client, e);
        return;
    };
    item_say(world, e, &message.composed());
    let delay = messages
        .get(i + 1)
        .map_or(1, |next| next.at.saturating_sub(message.at).max(1));
    if let Some(mut r) = world.get_mut::<TapeRecorder>(e) {
        r.play_index = i + 1;
        if delay > 14 {
            r.skip = Some(delay);
            r.next_step = at + 1.0;
        } else {
            r.next_step = at + f64::from(delay);
        }
    }
}

/// `examine`: the recorder's display, an unwound tape's hint.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<crate::examine::Line>) {
    if let Some(r) = world.get::<TapeRecorder>(target) {
        lines.push((Span::Notice, "На дисплее:".to_owned()));
        let readout = match r.tape.and_then(|t| world.get::<Tape>(t)) {
            None => "КАССЕТА НЕ ВСТАВЛЕНА".to_owned(),
            Some(_) if r.playing => "ВОСПРОИЗВЕДЕНИЕ".to_owned(),
            Some(t) => format!("{} мин {} с", t.side.used / 60, t.side.used % 60),
        };
        lines.push((Span::Notice, readout));
    } else if world.get::<Tape>(target).is_some_and(|t| t.unspooled) {
        lines.push((
            Span::Notice,
            "Похоже, плёнка размотана. Это можно исправить отвёрткой.".to_owned(),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(at: u32, text: &str) -> TapeMessage {
        TapeMessage {
            at,
            speaker: "Ян".to_owned(),
            text: text.to_owned(),
        }
    }

    #[test]
    fn composed_lines_carry_the_tape_time() {
        assert_eq!(message(75, "привет").composed(), "[01:15] Ян: привет");
    }

    #[test]
    fn flipping_swaps_the_sides() {
        let mut tape = Tape {
            max: TAPE_CAPACITY,
            ..default()
        };
        tape.side.messages.push(message(0, "а"));
        tape.side.used = 3;
        tape.flip();
        assert!(tape.side.messages.is_empty());
        assert_eq!(tape.other.used, 3);
        tape.flip();
        assert_eq!(tape.side.messages.len(), 1);
    }

    #[test]
    fn long_transcripts_split_into_pages() {
        let long = "х".repeat(1000);
        let messages: Vec<TapeMessage> = (0..12).map(|i| message(i, &long)).collect();
        let pages = transcript_pages(&messages);
        assert!(pages.len() >= 3, "{}", pages.len());
        assert!(pages.iter().all(|p| p.chars().count() <= MAX_PAPER_LENGTH));
        assert!(pages[0].starts_with("<b>Расшифровка:</b>"));
    }
}
