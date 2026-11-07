//! Musical instruments (server), after tg `code/modules/instruments` with BandaStation's BPM
//! unlock, the jukebox (`dance_machine.dm`, `/datum/jukebox`) and the boombox.
//!
//! - Every `/obj/item/instrument` and `/obj/structure/musician` gets a [`Song`] (tg
//!   `/datum/song/handheld` / `stationary`) with its type's allowed instruments
//!   (`ss13_mapload::instruments`, the synthesizers: every non-admin one). Used in hand (an item)
//!   or clicked (a structure) it opens the song editor (`InstrumentEditor220`).
//! - Playing: the lines are compiled into chords ([`song`]); every tick (`SSinstruments` runs
//!   at `world.tick_lag`) the next chord plays when its delay is up (60 / BPM / tempo division,
//!   whole ticks with the remainder carried). A synthesized note is its nearest sample played
//!   at `2^(semitones / 12)` speed (BYOND `frequency`: pitch and speed together); a legacy note
//!   is its own file, when it exists. Every hearer in view within the song's range hears it at
//!   `volume * volume_multiplier`, without distance falloff (tg `playsound_local` without
//!   `max_distance`). It stops at the end (after the repeats), when the player can no longer play
//!   (incapacitated, not next to it, an item worn in its own slot unless playable worn, an
//!   unanchored structure, no hands) or on the play button.
//! - The harmonica stops when its wearer speaks; headphones show their `_on` state while playing.
//! - A wrench (4 s) fastens or unfastens a piano (`default_unfasten_wrench`).
// shortcut: notes are one-shot sounds: no per-channel sustain decay (the linear/exponential
// settings and "hold the last note" are kept and shown but the note fades on its own sample),
// no 128-channel limit and no instrument volume preference; no song sync (`id`, BandaStation's
// auto unison and multi-track), no musician trait's good music mood, no synthesizer circuit
// component, the drum kit does not fold into an item (a wrench fastens it like a piano). Add the
// sustain with per-channel volume updates (`ChannelSound::update`).

pub mod boombox;
pub mod jukebox;
pub mod music;
pub mod song;

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::instruments::{INSTRUMENTS_FILE, InstrumentHolder, Instruments, LEGACY_DIR};
use ss13_mapload::{DmAtom, DmItem, DmObj, MapLoadPlugin};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_shared::ui::UiStatus;
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, ask_world};
use ss13_tgui::server::{
    AlwaysState, AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems, UiWindow,
};

use crate::health::Stat;
use crate::instruments_ui::*;
use crate::inventory::{CarriedBy, Inventory, InventorySlot, slot_bit};
use crate::modpc::pda::turf_of;
use crate::obj_damage::is_under;
use crate::sound::{PlaySound, SOUND_RANGE, playsound};
use crate::tools::tell;
use song::Chord;

const ITEM_ROOT: &str = "/obj/item/instrument";
const STRUCTURE_ROOT: &str = "/obj/structure/musician";
const SYNTH: &str = "/obj/item/instrument/piano_synth";
const HEADPHONES: &str = "/obj/item/instrument/piano_synth/headphones";
const HARMONICA: &str = "/obj/item/instrument/harmonica";
/// `max_repeats`, `volume`, `min_volume`, `max_volume`, `note_shift_min/max`.
pub const MAX_REPEATS: u32 = 10;
pub const DEFAULT_VOLUME: u32 = 35;
pub const MIN_VOLUME: u32 = 1;
pub const MAX_VOLUME: u32 = 75;
pub const NOTE_SHIFT_MIN: i32 = -100;
pub const NOTE_SHIFT_MAX: i32 = 100;
/// `SUSTAIN_LINEAR`, `SUSTAIN_EXPONENTIAL`.
pub const SUSTAIN_LINEAR: &str = "Linear";
pub const SUSTAIN_EXPONENTIAL: &str = "Exponential";
/// `INSTRUMENT_MIN_TOTAL_SUSTAIN`, `INSTRUMENT_MAX_TOTAL_SUSTAIN` (ds),
/// `INSTRUMENT_EXP_FALLOFF_MIN/MAX`, `INSTRUMENT_MIN_SUSTAIN_DROPOFF`.
const MIN_TOTAL_SUSTAIN: f32 = 0.1;
const MAX_TOTAL_SUSTAIN: f32 = 50.0;
const EXP_FALLOFF_MIN: f32 = 1.025;
const EXP_FALLOFF_MAX: f32 = 10.0;
/// `default_unfasten_wrench(time = 4 SECONDS)` of `/obj/structure/musician`, deciseconds.
const MUSICIAN_WRENCH_DS: f32 = 40.0;
const RATCHET: &str = "sound/items/tools/ratchet.ogg";

/// `SSinstruments.instrument_data` and the holder types, from `<assets>/instruments.ron`.
#[derive(Resource, Clone, Debug, Default)]
pub struct InstrumentDb(pub Instruments);

impl InstrumentDb {
    /// `/datum/instrument/ready()`.
    pub fn ready(&self, id: &str) -> bool {
        self.0
            .instruments
            .get(id)
            .is_some_and(|i| match &i.legacy_dir {
                Some(_) => true,
                None => !i.samples.is_empty(),
            })
    }

    pub fn name(&self, id: &str) -> String {
        self.0
            .instruments
            .get(id)
            .map_or_else(|| id.to_owned(), |i| i.name.clone())
    }

    /// `get_allowed_instrument_ids()`: the synthesizers' list.
    pub fn synthesizer_ids(&self) -> Vec<String> {
        self.0
            .instruments
            .iter()
            .filter(|(_, i)| !i.admin_only)
            .map(|(id, _)| id.clone())
            .collect()
    }
}

/// tg `/datum/song` on its instrument.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Song {
    /// The sync id (`set_instrument_id`).
    pub id: String,
    pub lines: Vec<String>,
    pub bpm: u32,
    pub repeat: u32,
    pub volume: u32,
    pub range: f32,
    /// `allowed_instrument_ids` with their names.
    pub allowed: Vec<PossibleInstrument>,
    pub instrument: Option<String>,
    pub instrument_ready: bool,
    pub note_shift: i32,
    pub sustain_mode: String,
    pub sustain_dropoff_volume: f32,
    /// Deciseconds.
    pub sustain_linear_duration: f32,
    pub sustain_exponential_dropoff: f32,
    pub full_sustain_held_note: bool,
    pub stationary: bool,
    pub playable_while_worn: bool,
    pub can_play_unanchored: bool,
    pub playing: bool,
    pub player: Option<Entity>,
    /// `initial(icon_state)`, for the headphones' `_on`.
    pub base_icon: String,
    chords: Vec<Chord>,
    current: usize,
    elapsed: u32,
    delay_by: u32,
    residual: f64,
}

impl Song {
    pub fn new(holder: &InstrumentHolder, allowed: Vec<PossibleInstrument>) -> Self {
        Self {
            id: String::new(),
            lines: Vec::new(),
            bpm: song::DEFAULT_BPM,
            repeat: 0,
            volume: DEFAULT_VOLUME,
            range: holder.range,
            instrument: allowed.first().map(|i| i.id.clone()),
            allowed,
            instrument_ready: false,
            note_shift: 0,
            sustain_mode: SUSTAIN_LINEAR.to_owned(),
            sustain_dropoff_volume: 0.0,
            sustain_linear_duration: 5.0,
            sustain_exponential_dropoff: 1.4,
            full_sustain_held_note: true,
            stationary: holder.stationary,
            playable_while_worn: holder.playable_while_worn,
            can_play_unanchored: holder.can_play_unanchored,
            playing: false,
            player: None,
            base_icon: String::new(),
            chords: Vec::new(),
            current: 0,
            elapsed: 0,
            delay_by: 0,
            residual: 0.0,
        }
    }

    /// `process_song` for one tick: the chord to play now, if any; `Some(finished)` in the
    /// second slot when the song ended.
    pub fn step(&mut self) -> (Option<Chord>, Option<bool>) {
        if self.chords.is_empty() {
            return (None, Some(true));
        }
        self.elapsed += 1;
        if self.elapsed < self.delay_by {
            return (None, None);
        }
        let chord = self.chords[self.current].clone();
        self.elapsed = 0;
        self.delay_by = song::tempodiv_to_delay(self.bpm, chord.tempodiv, &mut self.residual);
        self.current += 1;
        if self.current < self.chords.len() {
            return (Some(chord), None);
        }
        if self.repeat == 0 {
            return (Some(chord), Some(true));
        }
        self.repeat -= 1;
        self.current = 0;
        (Some(chord), None)
    }

    /// `start_playing` past its checks: compiles and rewinds. False for an empty song.
    pub fn start(&mut self, player: Entity) -> bool {
        self.chords = song::parse(&self.lines);
        if self.chords.is_empty() {
            return false;
        }
        self.playing = true;
        self.player = Some(player);
        self.elapsed = 0;
        self.delay_by = 0;
        self.residual = 0.0;
        self.current = 0;
        true
    }

    pub fn stop(&mut self) {
        self.playing = false;
        self.player = None;
        self.chords.clear();
    }
}

pub struct InstrumentsPlugin;

impl Plugin for InstrumentsPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        app.add_click_handler(ClickPhase::AttackSelf, 1747, |w, c| {
            crate::instruments::attack_self(w, c.client, c.mob, c.target)
        });
        app.add_click_handler(ClickPhase::EmptyHand, 3547, |w, c| {
            crate::instruments::attack_hand(w, c.client, c.mob, c.target, c.modifiers.right)
        });
        app.add_click_handler(ClickPhase::HeldOnTarget, 3447, |w, c| {
            c.held.is_some_and(|h| {
                crate::instruments::item_interaction(w, c.client, c.mob, h, c.target)
            })
        });
        app.add_click_handler(ClickPhase::OtherHand, 477, |w, c| {
            crate::instruments::boombox::other_hand(w, c.client, c.mob, c.held, c.target)
        });
        app.init_resource::<InstrumentDb>()
            .init_resource::<jukebox::JukeboxTracks>()
            .register_ui::<InstrumentEditorUi>()
            .register_ui::<JukeboxUi>()
            .add_observer(add_item)
            .add_observer(add_obj)
            .add_systems(PreStartup, (load_db, jukebox::load_tracks))
            .add_systems(
                Update,
                (
                    process_songs,
                    harmonica_speech,
                    jukebox::tick,
                    boombox::tick,
                    music::follow_listeners,
                    (editor_status, jukebox::window_status).in_set(UiSystems::StatusHook),
                    (
                        editor_acts,
                        editor_answers,
                        jukebox::acts,
                        boombox::menu_answers,
                    )
                        .chain()
                        .in_set(UiSystems::Handle)
                        .after(ss13_tgui::prompt::PromptSystems::Answer),
                ),
            );
    }
}

fn load_db(world: &mut World) {
    let Some(path) = world
        .get_resource::<MapLoadPlugin>()
        .map(|m| m.assets.join(INSTRUMENTS_FILE))
    else {
        return;
    };
    match Instruments::read(&path) {
        Ok(data) => world.insert_resource(InstrumentDb(data)),
        Err(e) => info!("{e:#} (rerun `ss13-mapload convert`): no instruments"),
    }
}

fn add_item(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    let Ok(item) = items.get(add.entity) else {
        return;
    };
    let path = item.path.clone();
    let entity = add.entity;
    if is_under(&path, ITEM_ROOT) {
        commands.queue(move |world: &mut World| attach_song(world, entity, &path));
    } else {
        boombox::add_item(&mut commands, entity, &path);
    }
}

fn add_obj(add: On<Add<DmObj>>, objs: Query<&DmObj>, mut commands: Commands) {
    let Ok(obj) = objs.get(add.entity) else {
        return;
    };
    let path = obj.path.clone();
    let entity = add.entity;
    if is_under(&path, STRUCTURE_ROOT) {
        commands.queue(move |world: &mut World| attach_song(world, entity, &path));
    } else if is_under(&path, jukebox::JUKEBOX) {
        commands.queue(move |world: &mut World| jukebox::attach(world, entity, &path));
    }
}

/// `/obj/item/instrument/Initialize` / `/obj/structure/musician/Initialize`: `song = new(src,
/// allowed_instrument_ids, instrument_range)`.
pub fn attach_song(world: &mut World, e: Entity, path: &str) {
    if world.get_entity(e).is_err() || world.get::<Song>(e).is_some() {
        return;
    }
    let db = world.resource::<InstrumentDb>();
    let Some(holder) = db.0.holders.get(path) else {
        return;
    };
    let ids = if is_under(path, SYNTH) {
        db.synthesizer_ids()
    } else {
        holder.allowed.clone()
    };
    let allowed = ids
        .iter()
        .map(|id| PossibleInstrument {
            name: db.name(id),
            id: id.clone(),
        })
        .collect();
    let mut song = Song::new(holder, allowed);
    song.instrument_ready = song.instrument.as_deref().is_some_and(|id| db.ready(id));
    song.base_icon = world
        .get::<DmAtom>(e)
        .map(|a| a.icon_state.clone())
        .unwrap_or_default();
    let stationary = song.stationary;
    let mut entity = world.entity_mut(e);
    entity.insert(song);
    // An item's window follows `default_state` ([`editor_status`]), not the tile distance.
    if !stationary {
        entity.insert(AlwaysState);
    }
}

fn open_editor(world: &mut World, client: Entity, mob: Entity, e: Entity) {
    let title = crate::medical::name_of(world, e);
    world.write_message(OpenUi::new::<InstrumentEditorUi>(client, mob, e, title));
}

/// `attack_self`: a held instrument opens its editor, a boombox its menu.
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    if world.get::<Song>(item).is_some_and(|s| !s.stationary) {
        open_editor(world, client, mob, item);
        return true;
    }
    boombox::attack_self(world, client, mob, item)
}

/// An empty-hand click: a structure's editor, the jukebox, a boombox's menu.
pub fn attack_hand(world: &mut World, client: Entity, mob: Entity, e: Entity, right: bool) -> bool {
    if let Some(song) = world.get::<Song>(e)
        && song.stationary
    {
        // Drum kit `ui_interact`: nothing while unanchored or broken.
        let drums = world
            .get::<DmObj>(e)
            .is_some_and(|o| is_under(&o.path, "/obj/structure/musician/drumskit"));
        let broken = world
            .get::<crate::obj_damage::Integrity>(e)
            .is_some_and(|i| i.broken);
        if !drums || (crate::obj_damage::anchored(world, e) && !broken) {
            open_editor(world, client, mob, e);
        }
        return true;
    }
    if world.get::<jukebox::Jukebox>(e).is_some() {
        return jukebox::interact(world, client, mob, e, right);
    }
    boombox::attack_hand(world, client, mob, e)
}

/// Item clicks: a wrench on a structure or jukebox, a tape into a boombox.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if world.get::<Song>(target).is_some_and(|s| s.stationary)
        && matches!(
            crate::tools::tool_of(world, held),
            Some((crate::tools::ToolKind::Wrench, _))
        )
    {
        unfasten(world, client, mob, held, target, MUSICIAN_WRENCH_DS);
        return true;
    }
    if world.get::<jukebox::Jukebox>(target).is_some() {
        return jukebox::item_interaction(world, client, mob, held, target);
    }
    boombox::item_interaction(world, client, mob, held, target)
}

/// `default_unfasten_wrench(user, tool, time)`.
pub(crate) fn unfasten(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
    delay_ds: f32,
) {
    let anchored = crate::obj_damage::anchored(world, target);
    if !anchored && crate::construction::blocked_for(world, target) {
        let name = crate::medical::name_of(world, target);
        tell(
            world,
            client,
            Span::Warning,
            format!("Вам не удалось прикрутить {name}."),
        );
        return;
    }
    if let Some(&pos) = world.get::<TilePos>(target) {
        playsound(world, PlaySound::new(pos, RATCHET, 50.0, true));
    }
    let actor = crate::tools::Actor {
        client,
        mob,
        item: Some(held),
    };
    crate::tools::use_tool(
        world,
        actor,
        target,
        delay_ds,
        0,
        crate::construction::Job::Unfasten { anchored },
    );
}

fn incapacitated(world: &World, mob: Entity) -> bool {
    world
        .get::<Stat>(mob)
        .is_some_and(|s| *s != Stat::Conscious)
        || crate::status_effects::incapacitation(world, mob).incapacitated
}

/// `/obj/item/instrument/can_play` and `/obj/structure/musician/can_play`.
pub fn can_play(world: &World, e: Entity, player: Entity) -> bool {
    let Some(song) = world.get::<Song>(e) else {
        return false;
    };
    if world.get_entity(player).is_err() || incapacitated(world, player) {
        return false;
    }
    if song.stationary {
        if !crate::obj_damage::anchored(world, e) && !song.can_play_unanchored {
            return false;
        }
        // `ISADVANCEDTOOLUSER`.
        return world.get::<Inventory>(player).is_some()
            && crate::storage::in_reach(world, player, e);
    }
    if let Some(slot) = world.get::<Inventory>(player).and_then(|i| i.slot_of(e)) {
        let hand = matches!(slot, InventorySlot::LeftHand | InventorySlot::RightHand);
        let flags = world.get::<DmItem>(e).map_or(0, |i| i.slot_flags);
        // Can't play while wearing the instrument in its own slot.
        return hand || song.playable_while_worn || slot_bit(slot) & flags == 0;
    }
    if world.get::<CarriedBy>(e).is_some() {
        return false;
    }
    crate::storage::in_reach(world, player, e)
}

/// `in_range(parent, user)`: in the user's hands or on a tile next to it.
fn in_range(world: &World, mob: Entity, e: Entity) -> bool {
    if world.get::<Inventory>(mob).is_some_and(|i| i.holds(e)) {
        return true;
    }
    match (turf_of(world, mob), world.get::<TilePos>(e)) {
        (Some(at), Some(&pos)) => at.chebyshev(pos).is_some_and(|d| d <= 1),
        _ => false,
    }
}

/// `start_playing(user)`.
pub fn start_playing(world: &mut World, client: Entity, e: Entity, user: Entity) {
    let Some(mut song) = world.get::<Song>(e).cloned() else {
        return;
    };
    if song.playing {
        return;
    }
    let name = crate::medical::name_of(world, e);
    if !song.instrument_ready {
        tell(
            world,
            client,
            Span::Warning,
            format!("С {name} что-то не так. Сбросьте инструмент."),
        );
        return;
    }
    if !song.start(user) {
        tell(world, client, Span::Warning, "Песня пуста.");
        return;
    }
    world.entity_mut(e).insert(song);
    headphones_icon(world, e);
}

/// `stop_playing()`.
pub fn stop_playing(world: &mut World, e: Entity) {
    {
        let Some(mut song) = world.get_mut::<Song>(e) else {
            return;
        };
        if !song.playing {
            return;
        }
        song.stop();
    }
    headphones_icon(world, e);
}

/// `/obj/item/instrument/piano_synth/headphones/update_icon_state`.
fn headphones_icon(world: &mut World, e: Entity) {
    let headphones = world
        .get::<DmItem>(e)
        .is_some_and(|i| is_under(&i.path, HEADPHONES));
    let Some((base, playing)) = world
        .get::<Song>(e)
        .map(|s| (s.base_icon.clone(), s.playing))
    else {
        return;
    };
    if headphones {
        let state = if playing { format!("{base}_on") } else { base };
        crate::obj_damage::set_icon_state(world, e, &state);
    }
}

/// `play_chord`: every note to the hearers.
fn play_chord(world: &mut World, e: Entity, chord: &Chord) {
    let Some(pos) = turf_of(world, e) else {
        return;
    };
    let Some(song) = world.get::<Song>(e).cloned() else {
        return;
    };
    let Some(id) = song.instrument.clone() else {
        return;
    };
    let Some(data) = world
        .resource::<InstrumentDb>()
        .0
        .instruments
        .get(&id)
        .cloned()
    else {
        return;
    };
    let volume = song.volume as f32 * data.volume_multiplier;
    for note in &chord.notes {
        let (file, frequency) = match &data.legacy_dir {
            Some(dir) => {
                let Some(name) = song::legacy_name(note).filter(|n| data.legacy_notes.contains(n))
                else {
                    continue;
                };
                (format!("{LEGACY_DIR}/{dir}/{name}.ogg"), 1.0)
            }
            None => {
                let key =
                    (song::synth_key(note) + song.note_shift).clamp(song::MIN_KEY, song::MAX_KEY);
                let Some((sample, frequency)) = song::key_sample(&data.samples, key) else {
                    continue;
                };
                (sample.to_owned(), frequency)
            }
        };
        playsound(world, note_sound(pos, file, volume, song.range, frequency));
    }
}

/// A note to the hearers in view within `range`, at full `volume` (no distance falloff), at
/// `frequency` speed.
pub fn note_sound(
    pos: TilePos,
    file: String,
    volume: f32,
    range: f32,
    frequency: f32,
) -> PlaySound {
    // A range of 0 (space pods) is the own tile; 0.5 keeps the falloff ratio finite.
    let mut sound = PlaySound::new(pos, file, volume, false)
        .extrarange(range.max(0.5) - SOUND_RANGE)
        .ignore_walls(false)
        .pitch(frequency);
    sound.min_volume = volume;
    sound
}

/// `SSinstruments` `process`: every playing song one tick on.
fn process_songs(world: &mut World) {
    let playing: Vec<Entity> = world
        .query::<(Entity, &Song)>()
        .iter(world)
        .filter(|(_, s)| s.playing)
        .map(|(e, _)| e)
        .collect();
    for e in playing {
        let player = world.get::<Song>(e).and_then(|s| s.player);
        if !player.is_some_and(|p| can_play(world, e, p)) {
            stop_playing(world, e);
            continue;
        }
        let Some((chord, finished)) = world.get_mut::<Song>(e).map(|mut s| s.step()) else {
            continue;
        };
        if let Some(chord) = chord {
            play_chord(world, e, &chord);
        }
        if finished.is_some() {
            stop_playing(world, e);
        }
    }
}

/// `/obj/item/instrument/harmonica/handle_speech`: a worn harmonica stops when its wearer talks.
fn harmonica_speech(world: &mut World, mut cursor: Local<MessageCursor<ss13_chat::server::Spoke>>) {
    let Some(spoken) = world.get_resource::<Messages<ss13_chat::server::Spoke>>() else {
        return;
    };
    let speakers: Vec<(Entity, Entity)> =
        cursor.read(spoken).map(|s| (s.client, s.speaker)).collect();
    for (client, speaker) in speakers {
        let Some(mask) = world.get::<Inventory>(speaker).and_then(|i| i.mask) else {
            continue;
        };
        let harmonica = world
            .get::<DmItem>(mask)
            .is_some_and(|i| is_under(&i.path, HARMONICA));
        if harmonica && world.get::<Song>(mask).is_some_and(|s| s.playing) {
            tell(
                world,
                client,
                Span::Warning,
                "Вы перестаёте играть на губной гармошке, чтобы заговорить...",
            );
            stop_playing(world, mask);
        }
    }
}

/// `default_state` of an item's editor (`crate::devices::item_ui_status`).
fn editor_status(
    mut windows: Query<&mut UiWindow>,
    songs: Query<(&Song, Option<&TilePos>)>,
    users: Query<(&Inventory, Option<&Stat>, Option<&TilePos>)>,
) {
    for mut window in &mut windows {
        if window.interface != InstrumentEditorUi::NAME {
            continue;
        }
        let Ok((song, at)) = songs.get(window.source) else {
            window.cap_status(UiStatus::Close);
            continue;
        };
        if song.stationary {
            continue;
        }
        let status =
            crate::devices::item_ui_status(window.source, at, users.get(window.actor).ok());
        window.cap_status(status);
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum EditorAct {
    Play,
    SetId(String),
    ChangeInstrument(String),
    Tempo { faster: bool, step: f64 },
    SetBpm(f64),
    Import,
    NewSong,
    AddLine,
    DeleteLine(f64),
    ModifyLine(f64),
    SustainMode(String),
    NoteShift(f64),
    Volume(f64),
    Dropoff(f64),
    ToggleHold,
    Repeat(f64),
    EditSustain(f64),
}

impl ServerUi for InstrumentEditorUi {
    type Source = &'static Song;
    type UserParam = ();
    type Act = EditorAct;

    fn static_data(song: &Song) -> InstrumentEditorStatic {
        InstrumentEditorStatic {
            can_switch_instrument: song.allowed.len() > 1,
            possible_instruments: song.allowed.clone(),
            sustain_modes: vec![SUSTAIN_LINEAR.to_owned(), SUSTAIN_EXPONENTIAL.to_owned()],
            max_repeats: MAX_REPEATS,
            min_volume: MIN_VOLUME,
            max_volume: MAX_VOLUME,
            note_shift_min: NOTE_SHIFT_MIN,
            note_shift_max: NOTE_SHIFT_MAX,
            max_line_chars: song::MAX_LINE_CHARS as u32,
            max_lines: song::MAX_LINES as u32,
            min_bpm: song::MIN_BPM,
            max_bpm: song::MAX_BPM,
        }
    }

    fn data(song: &Song, _: Entity, _: &()) -> InstrumentEditorData {
        let linear = song.sustain_mode == SUSTAIN_LINEAR;
        InstrumentEditorData {
            id: song.id.clone(),
            using_instrument: song
                .instrument
                .as_ref()
                .and_then(|id| song.allowed.iter().find(|i| &i.id == id))
                .map_or_else(|| "No instrument loaded!".to_owned(), |i| i.name.clone()),
            note_shift: song.note_shift,
            sustain_mode: song.sustain_mode.clone(),
            sustain_mode_button: if linear {
                "Linear Sustain Duration (in seconds)"
            } else {
                "Exponential Falloff Factor (% per decisecond)"
            }
            .to_owned(),
            sustain_mode_duration: if linear {
                song.sustain_linear_duration / 10.0
            } else {
                song.sustain_exponential_dropoff
            },
            sustain_mode_min: if linear {
                MIN_TOTAL_SUSTAIN
            } else {
                EXP_FALLOFF_MIN
            },
            sustain_mode_max: if linear {
                MAX_TOTAL_SUSTAIN
            } else {
                EXP_FALLOFF_MAX
            },
            instrument_ready: song.instrument_ready,
            volume: song.volume,
            volume_dropoff_threshold: song.sustain_dropoff_volume,
            sustain_indefinitely: song.full_sustain_held_note,
            playing: song.playing,
            repeat: song.repeat,
            bpm: song.bpm,
            lines: song.lines.clone(),
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<EditorAct, UiReject> {
        let amount = || params.number("amount");
        Ok(match action {
            "play_music" => EditorAct::Play,
            "set_instrument_id" => EditorAct::SetId(params.text("id")?),
            "change_instrument" => EditorAct::ChangeInstrument(params.text("new_instrument")?),
            "tempo" | "tempo_big_step" => {
                let faster = params.get("tempo_change") == Some("increase_speed");
                let step = if action == "tempo" {
                    params.number("bpm_step").unwrap_or(1.0)
                } else {
                    10.0
                };
                EditorAct::Tempo { faster, step }
            }
            "set_bpm" | "set_bpm_slider" => EditorAct::SetBpm(amount()?),
            "import_song" => EditorAct::Import,
            "start_new_song" => EditorAct::NewSong,
            "add_new_line" => EditorAct::AddLine,
            "delete_line" => EditorAct::DeleteLine(params.number("line_deleted")?),
            "modify_line" => EditorAct::ModifyLine(params.number("line_editing")?),
            "set_sustain_mode" => EditorAct::SustainMode(params.text("new_mode")?),
            "set_note_shift" => EditorAct::NoteShift(amount()?),
            "set_volume" => EditorAct::Volume(amount()?),
            "set_dropoff_volume" => EditorAct::Dropoff(amount()?),
            "toggle_sustain_hold_indefinitely" => EditorAct::ToggleHold,
            "set_repeat_amount" => EditorAct::Repeat(amount()?),
            "edit_sustain_mode" => EditorAct::EditSustain(amount()?),
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

/// tg `reject_bad_name(lowertext(id), max_length = 20, allow_numbers = TRUE)` for a sync id.
pub fn sanitize_sync_id(id: &str) -> Option<String> {
    let id: String = id.trim().to_lowercase();
    let ok = !id.is_empty()
        && id.chars().count() <= 20
        && id
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.' | '\''));
    ok.then_some(id)
}

/// What a song editor prompt is for.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
enum SongPrompt {
    Import(Entity),
    AddLine(Entity),
    Modify(Entity, usize),
}

#[allow(clippy::too_many_arguments)]
fn ask_text(
    world: &mut World,
    client: Entity,
    actor: Entity,
    e: Entity,
    message: &str,
    default: String,
    max_length: usize,
    multiline: bool,
) -> Entity {
    let title = crate::medical::name_of(world, e);
    ask_world(
        world,
        Ask::new(
            client,
            actor,
            title,
            message,
            PromptKind::Text {
                default,
                max_length: max_length as u32,
                multiline,
            },
        ),
    )
}

/// `/datum/song/ui_act`, the state checks at the moment of effect.
pub fn editor_act(world: &mut World, client: Entity, actor: Entity, e: Entity, act: EditorAct) {
    let Some(mut song) = world.get::<Song>(e).cloned() else {
        return;
    };
    match act {
        EditorAct::Play => {
            if song.playing {
                stop_playing(world, e);
            } else {
                start_playing(world, client, e, actor);
            }
            return;
        }
        EditorAct::SetId(id) => {
            if let Some(id) = sanitize_sync_id(&id) {
                song.id = id;
            }
        }
        EditorAct::ChangeInstrument(id) => {
            if !song.allowed.iter().any(|i| i.id == id) {
                return;
            }
            let ready = world.resource::<InstrumentDb>().ready(&id);
            song.instrument = Some(id);
            song.instrument_ready = ready;
        }
        EditorAct::Tempo { faster, step } => {
            let step = step.round().clamp(1.0, 100.0);
            let bpm = f64::from(song.bpm) + if faster { step } else { -step };
            song.bpm = song::clamp_bpm(bpm);
        }
        EditorAct::SetBpm(bpm) => {
            if bpm <= 0.0 {
                return;
            }
            song.bpm = song::clamp_bpm(bpm);
        }
        EditorAct::Import => {
            let max = song::MAX_LINES * song::MAX_LINE_CHARS;
            let prompt = ask_text(
                world,
                client,
                actor,
                e,
                "Вставьте всю песню:",
                String::new(),
                max,
                true,
            );
            world.entity_mut(prompt).insert(SongPrompt::Import(e));
            return;
        }
        EditorAct::NewSong => {
            song.lines.clear();
            song.bpm = song::DEFAULT_BPM;
        }
        EditorAct::AddLine => {
            let prompt = ask_text(
                world,
                client,
                actor,
                e,
                "Введите строку",
                String::new(),
                song::MAX_LINE_CHARS,
                false,
            );
            world.entity_mut(prompt).insert(SongPrompt::AddLine(e));
            return;
        }
        EditorAct::DeleteLine(n) => {
            if n < 1.0 || n > song.lines.len() as f64 || n.fract() != 0.0 {
                return;
            }
            song.lines.remove(n as usize - 1);
        }
        EditorAct::ModifyLine(n) => {
            if n < 1.0 || n > song.lines.len() as f64 || n.fract() != 0.0 {
                return;
            }
            let index = n as usize - 1;
            let current = song.lines[index].clone();
            let prompt = ask_text(
                world,
                client,
                actor,
                e,
                "Введите строку",
                current,
                song::MAX_LINE_CHARS,
                false,
            );
            world
                .entity_mut(prompt)
                .insert(SongPrompt::Modify(e, index));
            return;
        }
        EditorAct::SustainMode(mode) => {
            if mode != SUSTAIN_LINEAR && mode != SUSTAIN_EXPONENTIAL {
                return;
            }
            song.sustain_mode = mode;
        }
        EditorAct::NoteShift(amount) => {
            song.note_shift = amount
                .round()
                .clamp(f64::from(NOTE_SHIFT_MIN), f64::from(NOTE_SHIFT_MAX))
                as i32;
        }
        EditorAct::Volume(amount) => {
            song.volume = amount
                .round()
                .clamp(f64::from(MIN_VOLUME), f64::from(MAX_VOLUME))
                as u32;
        }
        EditorAct::Dropoff(amount) => {
            song.sustain_dropoff_volume =
                ((amount * 100.0).round() / 100.0).clamp(0.0, 100.0) as f32;
        }
        EditorAct::ToggleHold => song.full_sustain_held_note = !song.full_sustain_held_note,
        EditorAct::Repeat(amount) => {
            // `set_repeats`: not while playing.
            if song.playing {
                return;
            }
            song.repeat = amount.round().clamp(0.0, f64::from(MAX_REPEATS)) as u32;
        }
        EditorAct::EditSustain(amount) => {
            let amount = amount as f32;
            if song.sustain_mode == SUSTAIN_LINEAR {
                // `set_linear_falloff_duration`: seconds in, deciseconds rounded to a tick.
                let ds = ((amount * 10.0) / 0.5).round() * 0.5;
                song.sustain_linear_duration = ds.clamp(0.5, MAX_TOTAL_SUSTAIN);
            } else {
                song.sustain_exponential_dropoff = amount.clamp(EXP_FALLOFF_MIN, EXP_FALLOFF_MAX);
            }
        }
    }
    // Prompts and acts never touch the playback state: keep the live one.
    if let Some(mut live) = world.get_mut::<Song>(e) {
        let live = &mut *live;
        song.playing = live.playing;
        song.player = live.player;
        song.chords = std::mem::take(&mut live.chords);
        song.current = live.current;
        song.elapsed = live.elapsed;
        song.delay_by = live.delay_by;
        song.residual = live.residual;
        *live = song;
    }
}

fn editor_acts(world: &mut World, mut cursor: Local<MessageCursor<UiActed<InstrumentEditorUi>>>) {
    let acts: Vec<(Entity, Entity, Entity, EditorAct)> = cursor
        .read(world.resource::<Messages<UiActed<InstrumentEditorUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, source, act) in acts {
        editor_act(world, client, actor, source, act);
    }
}

/// The editor's text prompts: applied if the user is still next to the instrument.
fn editor_answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answers: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, actor, answer) in answers {
        let Some(&kind) = world.get::<SongPrompt>(prompt) else {
            continue;
        };
        let Some(PromptAnswer::Text(text)) = answer else {
            continue;
        };
        let (SongPrompt::Import(e) | SongPrompt::AddLine(e) | SongPrompt::Modify(e, _)) = kind;
        if world.get::<Song>(e).is_none() || !in_range(world, actor, e) {
            continue;
        }
        match kind {
            SongPrompt::Import(_) => {
                let parsed = song::parse_song(&text);
                for warning in &parsed.warnings {
                    tell(world, client, Span::Notice, warning.clone());
                }
                if let Some(mut s) = world.get_mut::<Song>(e) {
                    s.lines = parsed.lines;
                    s.bpm = parsed.bpm;
                }
            }
            SongPrompt::AddLine(_) => {
                if text.is_empty() {
                    continue;
                }
                let line: String = text.chars().take(song::MAX_LINE_CHARS).collect();
                if let Some(mut s) = world.get_mut::<Song>(e)
                    && s.lines.len() <= song::MAX_LINES
                {
                    s.lines.push(line);
                }
            }
            SongPrompt::Modify(_, index) => {
                let line: String = text.chars().take(song::MAX_LINE_CHARS).collect();
                if let Some(mut s) = world.get_mut::<Song>(e)
                    && let Some(slot) = s.lines.get_mut(index)
                {
                    *slot = line;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn holder() -> InstrumentHolder {
        InstrumentHolder {
            allowed: vec!["r3grand".into()],
            range: 15.0,
            ..default()
        }
    }

    #[test]
    fn a_song_plays_its_chords_on_time_and_repeats() {
        let mut song = Song::new(&holder(), Vec::new());
        song.lines = vec!["C,D/2".into()];
        song.repeat = 1;
        assert!(song.start(Entity::PLACEHOLDER));
        // 120 BPM: the first chord at once, the second 10 ticks later, then 5 ticks (/2).
        let (chord, end) = song.step();
        assert!(chord.is_some() && end.is_none());
        for _ in 0..9 {
            assert_eq!(song.step(), (None, None));
        }
        let (chord, end) = song.step();
        assert_eq!(chord.map(|c| c.tempodiv), Some(2.0));
        assert!(end.is_none());
        for _ in 0..4 {
            assert_eq!(song.step(), (None, None));
        }
        // The repeat: C again, then D ends the song.
        assert!(song.step().0.is_some());
        for _ in 0..9 {
            song.step();
        }
        let (chord, end) = song.step();
        assert!(chord.is_some());
        assert_eq!(end, Some(true));
        let mut empty = Song::new(&holder(), Vec::new());
        assert!(!empty.start(Entity::PLACEHOLDER));
    }

    #[test]
    fn notes_are_heard_in_range_without_falloff() {
        let sound = note_sound(TilePos::new(0, 0, 1), "a.ogg".into(), 11.55, 15.0, 2.0);
        assert_eq!(sound.extrarange, 0.0);
        assert!(!sound.ignore_walls);
        assert_eq!(sound.pitch, 2.0);
        let at_edge = crate::sound::falloff_volume(
            sound.vol,
            14.0,
            SOUND_RANGE + sound.extrarange,
            sound.falloff_distance,
            sound.falloff_exponent,
            sound.min_volume,
        );
        assert!((at_edge - 11.55).abs() < 1e-4);
        // Space pods: only the own tile, and still a finite volume.
        let pods = note_sound(TilePos::new(0, 0, 1), "a.ogg".into(), 10.0, 0.0, 1.0);
        let own = crate::sound::falloff_volume(
            pods.vol,
            0.0,
            SOUND_RANGE + pods.extrarange,
            pods.falloff_distance,
            pods.falloff_exponent,
            pods.min_volume,
        );
        assert_eq!(own, 10.0);
    }

    #[test]
    fn sync_ids_are_sanitized() {
        assert_eq!(sanitize_sync_id(" Band1 ").as_deref(), Some("band1"));
        assert_eq!(sanitize_sync_id(""), None);
        assert_eq!(sanitize_sync_id(&"a".repeat(21)), None);
        assert_eq!(sanitize_sync_id("<b>"), None);
    }
}
