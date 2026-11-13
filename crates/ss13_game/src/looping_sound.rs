//! Looping sounds (server): tg `/datum/looping_sound` (`code/datums/looping_sounds/`) and the
//! sound tokens its `use_sound_tokens` loops play on (`/datum/sound_token`, `SSsound_tokens`).
//!
//! A machine starts a [`LoopDef`] on its entity ([`start`], [`stop`], [`set`], or the
//! [`sync`] system on a state component): the `start_sound` plays, after `start_length` the
//! loop plays a `mid_sounds` pick every `mid_length` (± `mid_length_vary`, at `chance`%), and
//! [`stop`] ends it with the `end_sound`. `each_once` plays every mid sound once per round of
//! the list (shuffled, or `in_order`).
//!
//! Without tokens each sound is a [`PlaySound`] from the parent (`direct`: to the parent mob's
//! client only). With tokens the loop holds a reserved channel: every player within
//! `SOUND_RANGE + extra_range` tiles hears the current sound on it, and as the source or the
//! listener moves only its volume and offset change (`SOUND_UPDATE`); one leaving the range is
//! stopped. A loop with one mid sound and nothing random repeats natively on the client
//! (`can_native_repeat`).
//!
//! Cost: the schedule touches only the loops that are due; the tokens follow their listeners
//! once per tick: active tokens × player mobs distance checks (a few dozen machines × a few
//! hundred players at worst), a message only when a listener's volume or offset changed.
// shortcut: a listener joining mid-sound hears it from the start (tg seeks to the elapsed
// `offset`; the server knows no clip lengths), and a token loop's `end_sound` is a plain
// positional sound that does not follow its source. No `max_loops` (no loop uses it), no
// reverb/area environment, no `reserve_random_channel` for non-token loops (their sounds are
// one-shots here).

use std::collections::HashMap;

use bevy::prelude::*;
use ss13_net::sound::server::{SendChannelSound, SendSound};
use ss13_shared::TilePos;
use ss13_shared::protocol::{ChannelSound, SoundEvent};

use crate::combat::CombatRng;
use crate::server::PlayerMob;
use crate::sound::{
    MIN_VOLUME, ONE_ATMOSPHERE, PlaySound, SOUND_FALLOFF_EXPONENT, SOUND_RANGE, SoundRng,
    falloff_volume, playsound, pressure_factor,
};

/// The vars of a `/datum/looping_sound` subtype. Times in seconds (tg deciseconds / 10).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LoopDef {
    /// The tg type; identifies the loop on its parent.
    pub path: &'static str,
    /// Files with their `pick_weight` weights (1 for an unweighted list entry).
    pub mid_sounds: &'static [(&'static str, u32)],
    pub mid_length: f64,
    pub mid_length_vary: f64,
    pub each_once: bool,
    pub in_order: bool,
    pub start_sound: Option<&'static str>,
    pub start_length: f64,
    pub start_volume: Option<f32>,
    pub end_sound: Option<&'static str>,
    pub end_volume: Option<f32>,
    /// Percent per loop; 0 plays every loop.
    pub chance: f32,
    pub volume: f32,
    pub vary: bool,
    pub extra_range: f32,
    pub falloff_exponent: f32,
    pub falloff_distance: f32,
    pub pressure_affected: bool,
    pub ignore_walls: bool,
    pub use_sound_tokens: bool,
    pub never_native_repeat: bool,
}

/// `/datum/looping_sound`'s defaults (a `null` falloff is `playsound`'s default).
pub const BASE: LoopDef = LoopDef {
    path: "/datum/looping_sound",
    mid_sounds: &[],
    mid_length: 1.0,
    mid_length_vary: 0.0,
    each_once: false,
    in_order: false,
    start_sound: None,
    start_length: 0.0,
    start_volume: None,
    end_sound: None,
    end_volume: None,
    chance: 0.0,
    volume: 100.0,
    vary: false,
    extra_range: 0.0,
    falloff_exponent: SOUND_FALLOFF_EXPONENT,
    falloff_distance: 0.0,
    pressure_affected: true,
    ignore_walls: true,
    use_sound_tokens: false,
    never_native_repeat: false,
};

impl LoopDef {
    /// `can_native_repeat`: a token loop with one mid sound and nothing random.
    pub fn native_repeat(&self, mids: &[(&'static str, u32)]) -> bool {
        self.use_sound_tokens
            && !self.never_native_repeat
            && self.chance == 0.0
            && self.mid_length_vary == 0.0
            && !self.each_once
            && !self.in_order
            && mids.len() == 1
    }
}

/// tg's subtypes that have a machine here, with BandaStation's volumes
/// (`modular_bandastation/sounds/code/looping_sound.dm`, `supermatter.dm`).
pub mod defs {
    use super::{BASE, LoopDef};
    use crate::sound::MEDIUM_RANGE_SOUND_EXTRARANGE;

    pub const SHOWERING: LoopDef = LoopDef {
        path: "/datum/looping_sound/showering",
        start_sound: Some("sound/machines/shower/shower_start.ogg"),
        start_length: 0.2,
        mid_sounds: &[
            ("sound/machines/shower/shower_mid1.ogg", 1),
            ("sound/machines/shower/shower_mid2.ogg", 1),
            ("sound/machines/shower/shower_mid3.ogg", 1),
        ],
        mid_length: 1.0,
        end_sound: Some("sound/machines/shower/shower_end.ogg"),
        volume: 20.0,
        ..BASE
    };
    /// BandaStation: volume 20 (tg 40).
    pub const SUPERMATTER: LoopDef = LoopDef {
        path: "/datum/looping_sound/supermatter",
        start_sound: Some("sound/machines/sm/loops/calm_start.ogg"),
        start_length: 1.6,
        mid_sounds: &[(SM_CALM, 1)],
        mid_length: 4.0,
        end_sound: Some("sound/machines/sm/loops/calm_end.ogg"),
        volume: 20.0,
        extra_range: 25.0,
        falloff_exponent: 10.0,
        falloff_distance: 5.0,
        vary: true,
        use_sound_tokens: true,
        ..BASE
    };
    /// BandaStation's `/datum/looping_sound/supermatter/hugbox`.
    pub const SUPERMATTER_HUGBOX: LoopDef = LoopDef {
        path: "/datum/looping_sound/supermatter/hugbox",
        volume: 10.0,
        extra_range: -5.0,
        ..SUPERMATTER
    };
    pub const SM_CALM: &str = "sound/machines/sm/loops/calm.ogg";
    pub const SM_DELAMMING: &str = "sound/machines/sm/loops/delamming.ogg";
    /// BandaStation: volume 30 (tg 55).
    pub const HYPERTORUS: LoopDef = LoopDef {
        path: "/datum/looping_sound/hypertorus",
        mid_sounds: &[("sound/machines/hypertorus/loops/hypertorus_nominal.ogg", 1)],
        mid_length: 6.0,
        volume: 30.0,
        extra_range: 15.0,
        vary: true,
        use_sound_tokens: true,
        ..BASE
    };
    pub const GENERATOR: LoopDef = LoopDef {
        path: "/datum/looping_sound/generator",
        start_sound: Some("sound/machines/generator/generator_start.ogg"),
        start_length: 0.4,
        mid_sounds: &[
            ("sound/machines/generator/generator_mid1.ogg", 1),
            ("sound/machines/generator/generator_mid2.ogg", 1),
            ("sound/machines/generator/generator_mid3.ogg", 1),
        ],
        mid_length: 0.4,
        end_sound: Some("sound/machines/generator/generator_end.ogg"),
        volume: 40.0,
        ..BASE
    };
    pub const DEEP_FRYER: LoopDef = LoopDef {
        path: "/datum/looping_sound/deep_fryer",
        start_sound: Some("sound/machines/fryer/deep_fryer_immerse.ogg"),
        start_length: 1.0,
        mid_sounds: &[
            ("sound/machines/fryer/deep_fryer_1.ogg", 1),
            ("sound/machines/fryer/deep_fryer_2.ogg", 1),
        ],
        mid_length: 1.0,
        end_sound: Some("sound/machines/fryer/deep_fryer_emerge.ogg"),
        volume: 15.0,
        use_sound_tokens: true,
        ..BASE
    };
    /// BandaStation: volume 30 (tg 50).
    pub const CLOCK: LoopDef = LoopDef {
        path: "/datum/looping_sound/clock",
        mid_sounds: &[("sound/ambience/misc/ticking_clock.ogg", 1)],
        mid_length: 4.0,
        volume: 30.0,
        ignore_walls: false,
        ..BASE
    };
    pub const GRILL: LoopDef = LoopDef {
        path: "/datum/looping_sound/grill",
        mid_sounds: &[("sound/machines/grill/grillsizzle.ogg", 1)],
        mid_length: 1.8,
        volume: 50.0,
        use_sound_tokens: true,
        ..BASE
    };
    pub const OVEN: LoopDef = LoopDef {
        path: "/datum/looping_sound/oven",
        start_sound: Some("sound/machines/oven/oven_loop_start.ogg"),
        start_length: 1.2,
        mid_sounds: &[("sound/machines/oven/oven_loop_mid.ogg", 1)],
        mid_length: 1.3,
        end_sound: Some("sound/machines/oven/oven_loop_end.ogg"),
        volume: 100.0,
        falloff_exponent: 4.0,
        use_sound_tokens: true,
        ..BASE
    };
    pub const MICROWAVE: LoopDef = LoopDef {
        path: "/datum/looping_sound/microwave",
        start_sound: Some("sound/machines/microwave/microwave-start.ogg"),
        start_length: 1.0,
        mid_sounds: &[
            ("sound/machines/microwave/microwave-mid1.ogg", 10),
            ("sound/machines/microwave/microwave-mid2.ogg", 1),
        ],
        mid_length: 1.0,
        end_sound: Some("sound/machines/microwave/microwave-end.ogg"),
        volume: 90.0,
        use_sound_tokens: true,
        ..BASE
    };
    pub const LATHE_PRINT: LoopDef = LoopDef {
        path: "/datum/looping_sound/lathe_print",
        mid_sounds: &[("sound/machines/lathe/lathe_print.ogg", 1)],
        mid_length: 2.0,
        volume: 50.0,
        vary: true,
        ignore_walls: false,
        falloff_distance: 1.0,
        mid_length_vary: 1.0,
        use_sound_tokens: true,
        ..BASE
    };
    pub const COMPUTER: LoopDef = LoopDef {
        path: "/datum/looping_sound/computer",
        start_sound: Some("sound/machines/computer/computer_start.ogg"),
        start_length: 7.2,
        start_volume: Some(10.0),
        mid_sounds: &[
            ("sound/machines/computer/computer_mid1.ogg", 1),
            ("sound/machines/computer/computer_mid2.ogg", 1),
        ],
        mid_length: 1.8,
        end_sound: Some("sound/machines/computer/computer_end.ogg"),
        // tg `end_volume = 1 SECONDS`: 10.
        end_volume: Some(10.0),
        volume: 3.0,
        falloff_exponent: 4.0,
        extra_range: -12.0,
        falloff_distance: 0.0,
        use_sound_tokens: true,
        ..BASE
    };
    pub const GRAVGEN: LoopDef = LoopDef {
        path: "/datum/looping_sound/gravgen",
        start_sound: Some("sound/machines/gravgen/grav_gen_start.ogg"),
        start_length: 1.0,
        mid_sounds: &[("sound/machines/gravgen/grav_gen_mid1.ogg", 1)],
        mid_length: 2.4,
        end_sound: Some("sound/machines/gravgen/grav_gen_end.ogg"),
        extra_range: 8.0,
        vary: true,
        volume: 70.0,
        falloff_distance: 5.0,
        falloff_exponent: 10.0,
        use_sound_tokens: true,
        ..BASE
    };
    pub const FIREALARM: LoopDef = LoopDef {
        path: "/datum/looping_sound/firealarm",
        mid_sounds: &[
            ("sound/machines/fire_alarm/fire_alarm1.ogg", 1),
            ("sound/machines/fire_alarm/fire_alarm2.ogg", 1),
            ("sound/machines/fire_alarm/fire_alarm3.ogg", 1),
            ("sound/machines/fire_alarm/fire_alarm4.ogg", 1),
        ],
        mid_length: 2.4,
        volume: 30.0,
        use_sound_tokens: true,
        ..BASE
    };
    pub const BOILING: LoopDef = LoopDef {
        path: "/datum/looping_sound/boiling",
        mid_sounds: &[("sound/effects/bubbles/bubbles2.ogg", 1)],
        mid_length: 7.0,
        volume: 25.0,
        use_sound_tokens: true,
        ..BASE
    };
    pub const SOUP: LoopDef = LoopDef {
        path: "/datum/looping_sound/soup",
        mid_sounds: &[
            ("sound/effects/soup_boil/soup_boil1.ogg", 1),
            ("sound/effects/soup_boil/soup_boil2.ogg", 1),
            ("sound/effects/soup_boil/soup_boil3.ogg", 1),
            ("sound/effects/soup_boil/soup_boil4.ogg", 1),
            ("sound/effects/soup_boil/soup_boil5.ogg", 1),
        ],
        mid_length: 3.0,
        volume: 80.0,
        end_sound: Some("sound/effects/soup_boil/soup_boil_end.ogg"),
        end_volume: Some(60.0),
        extra_range: MEDIUM_RANGE_SOUND_EXTRARANGE,
        falloff_exponent: 3.0,
        use_sound_tokens: true,
        ..BASE
    };
    pub const CRYO_CELL: LoopDef = LoopDef {
        path: "/datum/looping_sound/cryo_cell",
        mid_sounds: &[
            ("sound/machines/cryo/cryo_1.ogg", 1),
            ("sound/machines/cryo/cryo_2.ogg", 1),
            ("sound/machines/cryo/cryo_3.ogg", 1),
            ("sound/machines/cryo/cryo_4.ogg", 1),
            ("sound/machines/cryo/cryo_5.ogg", 1),
            ("sound/machines/cryo/cryo_6.ogg", 1),
            ("sound/machines/cryo/cryo_7.ogg", 1),
            ("sound/machines/cryo/cryo_8.ogg", 1),
            ("sound/machines/cryo/cryo_9.ogg", 1),
            ("sound/machines/cryo/cryo_10.ogg", 1),
        ],
        mid_length: 5.0,
        volume: 50.0,
        use_sound_tokens: true,
        ..BASE
    };
    pub const BURNING: LoopDef = LoopDef {
        path: "/datum/looping_sound/burning",
        start_sound: Some("sound/items/match_strike.ogg"),
        start_length: 3.0,
        mid_sounds: &[("sound/effects/comfyfire.ogg", 1)],
        mid_length: 5.0,
        volume: 50.0,
        vary: true,
        extra_range: MEDIUM_RANGE_SOUND_EXTRARANGE,
        use_sound_tokens: true,
        ..BASE
    };
    pub const VENT_PUMP_OVERCLOCK: LoopDef = LoopDef {
        path: "/datum/looping_sound/vent_pump_overclock",
        start_sound: Some("sound/machines/fan/fan_start.ogg"),
        start_length: 1.5,
        end_sound: Some("sound/machines/fan/fan_stop.ogg"),
        mid_sounds: &[("sound/machines/fan/fan_loop.ogg", 1)],
        mid_length: 1.9,
        use_sound_tokens: true,
        ..BASE
    };
    /// tg leaves `mid_length` unset (a timer every tick); the hiss clip is 0.25 s long.
    pub const TAPE_RECORDER_HISS: LoopDef = LoopDef {
        path: "/datum/looping_sound/tape_recorder_hiss",
        mid_sounds: &[("sound/items/taperecorder/taperecorder_hiss_mid.ogg", 1)],
        mid_length: 0.25,
        start_sound: Some("sound/items/taperecorder/taperecorder_hiss_start.ogg"),
        volume: 10.0,
        ..BASE
    };
    /// tg leaves `mid_length` unset (a timer every tick); the clip is 4 s long.
    pub const BEESMOKE: LoopDef = LoopDef {
        path: "/datum/looping_sound/beesmoke",
        mid_sounds: &[("sound/items/weapons/beesmoke.ogg", 1)],
        mid_length: 4.0,
        volume: 5.0,
        ..BASE
    };
    /// `/datum/looping_sound/fire` of a hot group (`crate::fire`).
    pub const FIRE: LoopDef = LoopDef {
        path: "/datum/looping_sound/fire",
        mid_sounds: &[
            ("sound/effects/fireclip1.ogg", 1),
            ("sound/effects/fireclip2.ogg", 1),
            ("sound/effects/fireclip3.ogg", 1),
            ("sound/effects/fireclip4.ogg", 1),
            ("sound/effects/fireclip5.ogg", 1),
            ("sound/effects/fireclip6.ogg", 1),
            ("sound/effects/fireclip7.ogg", 1),
        ],
        volume: 30.0,
        mid_length: 2.0,
        falloff_distance: 1.0,
        ..BASE
    };
}

/// A listener of a token, by client.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Heard {
    volume: f32,
    offset: [i16; 2],
}

/// `/datum/sound_token`: the sound on a reserved channel that follows its listeners.
#[derive(Clone, Debug)]
struct Token {
    channel: u16,
    file: String,
    volume: f32,
    repeat: bool,
    range: f32,
    falloff_exponent: f32,
    falloff_distance: f32,
    listeners: HashMap<Entity, Heard>,
    /// The sound changed (`update_sound(start_playing = TRUE)`): every listener gets it anew.
    fresh: bool,
}

/// One running loop.
#[derive(Clone, Debug)]
struct LoopState {
    def: &'static LoopDef,
    /// `mid_sounds` set at runtime (the supermatter's calm/delamming).
    mids: &'static [(&'static str, u32)],
    direct: bool,
    /// `loop_started`: the mid sounds play.
    looping: bool,
    native: bool,
    /// When the loop starts (`start_length`) or plays its next mid sound.
    next: Option<f64>,
    cut_list: Vec<(&'static str, u32)>,
    audio_index: usize,
    token: Option<Token>,
}

/// The loops running on an atom.
#[derive(Component, Clone, Debug, Default)]
pub struct SoundLoops(Vec<LoopState>);

impl SoundLoops {
    /// `is_active()` of the loop of type `path`.
    pub fn is_active(&self, path: &str) -> bool {
        self.0.iter().any(|l| l.def.path == path)
    }
}

/// `SSsounds` channels reserved by tokens: from `CHANNEL_HIGHEST_AVAILABLE` (1008) down, as
/// tg's `reserve_channel`.
#[derive(Resource, Debug)]
pub struct TokenChannels {
    next: u16,
    free: Vec<u16>,
}

/// `CHANNEL_HIGHEST_AVAILABLE`.
pub const CHANNEL_HIGHEST_AVAILABLE: u16 = 1008;

impl Default for TokenChannels {
    fn default() -> Self {
        Self {
            next: CHANNEL_HIGHEST_AVAILABLE,
            free: Vec::new(),
        }
    }
}

impl TokenChannels {
    fn reserve(&mut self) -> Option<u16> {
        if let Some(c) = self.free.pop() {
            return Some(c);
        }
        if self.next == 0 {
            return None;
        }
        self.next -= 1;
        Some(self.next + 1)
    }

    fn release(&mut self, channel: u16) {
        self.free.push(channel);
    }
}

pub struct LoopingSoundPlugin;

impl Plugin for LoopingSoundPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TokenChannels>()
            .add_message::<SendChannelSound>()
            .add_message::<SendSound>()
            .add_observer(parent_removed)
            .add_systems(Update, drive);
    }
}

fn now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

fn rng(world: &mut World) -> CombatRng {
    std::mem::take(&mut world.resource_mut::<SoundRng>().0)
}

/// `new /datum/looping_sound(parent, start_immediately = TRUE)`.
pub fn start(world: &mut World, parent: Entity, def: &'static LoopDef) {
    start_with(world, parent, def, false, false);
}

/// `start()` with the `_skip_starting_sounds` and `_direct` args. Does nothing if the loop runs.
pub fn start_with(
    world: &mut World,
    parent: Entity,
    def: &'static LoopDef,
    skip_starting_sounds: bool,
    direct: bool,
) {
    let Ok(mut entity) = world.get_entity_mut(parent) else {
        return;
    };
    if def.mid_sounds.is_empty() {
        return;
    }
    if entity
        .get::<SoundLoops>()
        .is_some_and(|l| l.is_active(def.path))
    {
        return;
    }
    let state = LoopState {
        def,
        mids: def.mid_sounds,
        direct,
        looping: false,
        native: false,
        next: None,
        cut_list: Vec::new(),
        audio_index: 0,
        token: None,
    };
    if entity.contains::<SoundLoops>() {
        if let Some(mut loops) = entity.get_mut::<SoundLoops>() {
            loops.0.push(state);
        }
    } else {
        entity.insert(SoundLoops(vec![state]));
    }
    // `on_start`.
    let t = now(world);
    match def.start_sound.filter(|_| !skip_starting_sounds) {
        Some(sound) => {
            play(world, parent, def.path, sound, def.start_volume, false);
            if def.start_length > 0.0 {
                with_state(world, parent, def.path, |s| {
                    s.next = Some(t + def.start_length)
                });
                return;
            }
            start_sound_loop(world, parent, def.path, t);
        }
        None => start_sound_loop(world, parent, def.path, t),
    }
}

/// `stop()`: the token falls silent and the `end_sound` plays if the mid sounds had started.
pub fn stop(world: &mut World, parent: Entity, path: &str) {
    let Some(state) = world.get_mut::<SoundLoops>(parent).and_then(|mut loops| {
        let i = loops.0.iter().position(|l| l.def.path == path)?;
        Some(loops.0.remove(i))
    }) else {
        return;
    };
    let def = state.def;
    let direct = state.direct;
    if let Some(token) = state.token {
        silence(world, &token);
    }
    if state.looping
        && let Some(end) = def.end_sound
    {
        play_once(world, parent, def, direct, end, def.end_volume);
    }
}

/// [`start`] or [`stop`] to match `on`.
pub fn set(world: &mut World, parent: Entity, def: &'static LoopDef, on: bool) {
    if on {
        start(world, parent, def);
    } else {
        stop(world, parent, def.path);
    }
}

pub fn is_active(world: &World, parent: Entity, path: &str) -> bool {
    world
        .get::<SoundLoops>(parent)
        .is_some_and(|l| l.is_active(path))
}

/// `soundloop.mid_sounds = ...` while it runs; a natively repeating token switches to the new
/// file at once.
pub fn set_mid_sounds(
    world: &mut World,
    parent: Entity,
    path: &str,
    mids: &'static [(&'static str, u32)],
) {
    let mut restart = None;
    with_state(world, parent, path, |s| {
        if s.mids == mids {
            return;
        }
        s.mids = mids;
        s.cut_list.clear();
        if s.native && s.looping {
            restart = Some(mids[0].0);
        }
    });
    if let Some(file) = restart {
        play(world, parent, path, file, None, true);
    }
}

/// The clients that hear a token loop now (tests, admin views).
pub fn listeners(world: &World, parent: Entity, path: &str) -> Vec<Entity> {
    world
        .get::<SoundLoops>(parent)
        .and_then(|l| l.0.iter().find(|s| s.def.path == path))
        .and_then(|s| s.token.as_ref())
        .map(|t| t.listeners.keys().copied().collect())
        .unwrap_or_default()
}

/// A system starting and stopping `def` on each entity whose `C` changed, as `on` says (a
/// machine's own on/off state; the observer below stops it when the entity goes).
pub fn sync<C: Component>(
    def: &'static LoopDef,
    on: fn(&C) -> bool,
) -> impl FnMut(Query<(Entity, &C, Option<&SoundLoops>), Changed<C>>, Commands) {
    move |query, mut commands| {
        for (e, c, loops) in &query {
            let want = on(c);
            if want != loops.is_some_and(|l| l.is_active(def.path)) {
                commands.queue(move |world: &mut World| set(world, e, def, want));
            }
        }
    }
}

/// One `sound_loop()` of a loop whose parent is a turf, not an entity (a hot group's center,
/// `crate::fire`), with `extra_range` and `falloff_distance` as the parent set them.
pub fn play_mid_at(
    world: &mut World,
    pos: TilePos,
    def: &LoopDef,
    extra_range: f32,
    falloff_distance: f32,
) {
    let mut rng = rng(world);
    let pass = def.chance == 0.0 || rng.chance(def.chance);
    let i = pick_weight(def.mid_sounds, &mut rng);
    world.resource_mut::<SoundRng>().0 = rng;
    let Some(&(file, _)) = def.mid_sounds.get(i).filter(|_| pass) else {
        return;
    };
    let mut sound = PlaySound::new(pos, file, def.volume, def.vary)
        .extrarange(extra_range)
        .falloff_exponent(def.falloff_exponent)
        .falloff_distance(falloff_distance)
        .ignore_walls(def.ignore_walls);
    sound.pressure_affected = def.pressure_affected;
    playsound(world, sound);
}

fn with_state<R>(
    world: &mut World,
    parent: Entity,
    path: &str,
    f: impl FnOnce(&mut LoopState) -> R,
) -> Option<R> {
    let mut loops = world.get_mut::<SoundLoops>(parent)?;
    let state = loops.0.iter_mut().find(|l| l.def.path == path)?;
    Some(f(state))
}

/// `start_sound_loop`: native repeat, or the first mid sound and the timer.
fn start_sound_loop(world: &mut World, parent: Entity, path: &str, t: f64) {
    let Some((def, mids)) = with_state(world, parent, path, |s| {
        s.looping = true;
        s.native = s.def.native_repeat(s.mids);
        s.next = (!s.native).then_some(t + s.def.mid_length);
        (s.def, s.mids)
    }) else {
        return;
    };
    if def.native_repeat(mids) {
        play(world, parent, path, mids[0].0, None, true);
    } else {
        sound_loop(world, parent, path);
    }
}

/// `sound_loop`: one mid sound, at `chance`%.
fn sound_loop(world: &mut World, parent: Entity, path: &str) {
    let mut rng = rng(world);
    let file = with_state(world, parent, path, |s| {
        if s.def.chance > 0.0 && !rng.chance(s.def.chance) {
            return None;
        }
        get_sound(s, &mut rng)
    })
    .flatten();
    world.resource_mut::<SoundRng>().0 = rng;
    if let Some(file) = file {
        play(world, parent, path, file, None, false);
    }
}

/// `get_sound()`: a weighted pick; with `each_once` every sound once per round of the list,
/// shuffled or `in_order`.
fn get_sound(s: &mut LoopState, rng: &mut CombatRng) -> Option<&'static str> {
    if s.mids.is_empty() {
        return None;
    }
    if !s.def.each_once {
        return Some(s.mids[pick_weight(s.mids, rng)].0);
    }
    if s.def.in_order {
        s.audio_index = s.audio_index % s.mids.len() + 1;
        return Some(s.mids[s.audio_index - 1].0);
    }
    if s.cut_list.is_empty() {
        s.cut_list = s.mids.to_vec();
    }
    let i = pick_weight(&s.cut_list, rng);
    Some(s.cut_list.remove(i).0)
}

/// `pick_weight`: an index by weight; all zero picks the first.
fn pick_weight(list: &[(&'static str, u32)], rng: &mut CombatRng) -> usize {
    let total: u32 = list.iter().map(|(_, w)| w).sum();
    if total == 0 {
        return 0;
    }
    let mut roll = rng.range(1, total);
    for (i, (_, w)) in list.iter().enumerate() {
        if roll <= *w {
            return i;
        }
        roll -= w;
    }
    list.len() - 1
}

/// `play()`: on the loop's token, or a one-shot from (or `direct` to) the parent.
fn play(
    world: &mut World,
    parent: Entity,
    path: &str,
    file: &'static str,
    volume_override: Option<f32>,
    repeat: bool,
) {
    let Some((def, direct, has_token)) = with_state(world, parent, path, |s| {
        (s.def, s.direct, s.token.is_some())
    }) else {
        return;
    };
    if !def.use_sound_tokens {
        play_once(world, parent, def, direct, file, volume_override);
        return;
    }
    let channel = if has_token {
        None
    } else {
        let Some(c) = world.resource_mut::<TokenChannels>().reserve() else {
            warn!("looping sound {path}: no sound channels left");
            return;
        };
        Some(c)
    };
    let volume = volume_override.unwrap_or(def.volume);
    with_state(world, parent, path, |s| {
        let token = s.token.get_or_insert_with(|| Token {
            channel: channel.unwrap_or_default(),
            file: String::new(),
            volume,
            repeat,
            range: SOUND_RANGE + def.extra_range,
            falloff_exponent: def.falloff_exponent,
            falloff_distance: def.falloff_distance,
            listeners: HashMap::new(),
            fresh: true,
        });
        token.file = file.to_owned();
        token.volume = volume;
        token.repeat = repeat;
        token.fresh = true;
    });
}

/// A sound that does not loop or follow: `playsound` from the parent, or `SEND_SOUND` to it.
fn play_once(
    world: &mut World,
    parent: Entity,
    def: &LoopDef,
    direct: bool,
    file: &str,
    volume_override: Option<f32>,
) {
    let volume = volume_override.unwrap_or(def.volume);
    if direct {
        if let Some(client) = world.get::<PlayerMob>(parent).map(|p| p.client) {
            world.write_message(SendSound {
                client,
                sound: SoundEvent {
                    file: file.to_owned(),
                    volume,
                    speed: 1.0,
                    offset: None,
                },
            });
        }
        return;
    }
    let Some(pos) = crate::explosions::turf_of(world, parent) else {
        return;
    };
    let mut sound = PlaySound::new(pos, file, volume, def.vary && !def.use_sound_tokens)
        .extrarange(def.extra_range)
        .falloff_exponent(def.falloff_exponent)
        .falloff_distance(def.falloff_distance)
        .ignore_walls(def.ignore_walls);
    sound.pressure_affected = def.pressure_affected || def.use_sound_tokens;
    playsound(world, sound);
}

/// `remove_listener` of everyone and the channel back to the pool.
fn silence(world: &mut World, token: &Token) {
    for &client in token.listeners.keys() {
        world.write_message(stop_message(client, token.channel));
    }
    world.resource_mut::<TokenChannels>().release(token.channel);
}

fn stop_message(client: Entity, channel: u16) -> SendChannelSound {
    SendChannelSound {
        client,
        sound: ChannelSound {
            channel,
            sound: None,
            update: false,
            repeat: false,
        },
    }
}

/// The parent is gone (`handle_parent_del` + `QDEL_NULL(soundloop)`): its tokens fall silent,
/// without end sounds.
fn parent_removed(
    remove: On<Remove<SoundLoops>>,
    loops: Query<&SoundLoops>,
    mut channels: ResMut<TokenChannels>,
    mut out: MessageWriter<SendChannelSound>,
) {
    let Ok(loops) = loops.get(remove.entity) else {
        return;
    };
    for token in loops.0.iter().filter_map(|l| l.token.as_ref()) {
        for &client in token.listeners.keys() {
            out.write(stop_message(client, token.channel));
        }
        channels.release(token.channel);
    }
}

/// The timers (`SSsound_loops`), then the tokens follow their listeners (`SSsound_tokens`).
fn drive(world: &mut World) {
    let t = now(world);
    let mut query = world.query::<(Entity, &SoundLoops)>();
    let due: Vec<(Entity, &'static str, bool)> = query
        .iter(world)
        .flat_map(|(e, loops)| {
            loops
                .0
                .iter()
                .filter(|s| s.next.is_some_and(|n| n <= t))
                .map(move |s| (e, s.def.path, s.looping))
        })
        .collect();
    for (e, path, looping) in due {
        if !looping {
            start_sound_loop(world, e, path, t);
            continue;
        }
        let mut rng = rng(world);
        with_state(world, e, path, |s| {
            let vary = s.def.mid_length_vary;
            let jitter = if vary > 0.0 {
                let ds = (vary * 10.0).round() as u32;
                (f64::from(rng.range(0, 2 * ds)) - f64::from(ds)) / 10.0
            } else {
                0.0
            };
            let next = s.next.unwrap_or(t) + (s.def.mid_length + jitter).max(0.1);
            // A long stall does not replay every missed loop.
            s.next = Some(if next <= t {
                t + s.def.mid_length
            } else {
                next
            });
        });
        world.resource_mut::<SoundRng>().0 = rng;
        sound_loop(world, e, path);
    }
    follow_tokens(world);
}

/// One player as a listener this tick.
struct Ear {
    client: Entity,
    pos: TilePos,
    deaf: bool,
    pressure: Option<f64>,
}

/// What a listener at `ear` hears of a token at `source` (`update_listener` and
/// `playsound_local`): `None` out of range, `Some(0)` muted.
fn heard(token: &Token, source: TilePos, source_pressure: Option<f64>, ear: &Ear) -> Option<Heard> {
    if source.z != ear.pos.z {
        return None;
    }
    let (dx, dy) = (source.x - ear.pos.x, source.y - ear.pos.y);
    let distance = ((dx * dx + dy * dy) as f32).sqrt();
    if distance > token.range {
        return None;
    }
    let volume = if ear.deaf {
        0.0
    } else {
        falloff_volume(
            token.volume,
            distance,
            token.range,
            token.falloff_distance,
            token.falloff_exponent,
            MIN_VOLUME,
        ) * pressure_factor(ear.pressure, source_pressure, distance)
    };
    Some(Heard {
        volume: volume.max(0.0),
        offset: [dx.clamp(-999, 999) as i16, dy.clamp(-999, 999) as i16],
    })
}

/// A tile's pressure for `playsound_local` (no atmos simulation at all: air everywhere).
fn pressure(world: &World, pos: TilePos) -> Option<f64> {
    match world.get_resource::<ss13_atmos::Atmos>() {
        Some(a) => a.pressure(pos),
        None => Some(ONE_ATMOSPHERE),
    }
}

fn follow_tokens(world: &mut World) {
    let mut query = world.query::<(Entity, &SoundLoops)>();
    let sources: Vec<Entity> = query
        .iter(world)
        .filter(|(_, l)| l.0.iter().any(|s| s.token.is_some()))
        .map(|(e, _)| e)
        .collect();
    if sources.is_empty() {
        return;
    }
    let mobs: Vec<(Entity, Entity)> = world
        .query::<(Entity, &PlayerMob)>()
        .iter(world)
        .map(|(e, p)| (e, p.client))
        .collect();
    let w: &World = world;
    let ears: Vec<Ear> = mobs
        .into_iter()
        .filter_map(|(mob, client)| {
            let pos = crate::explosions::turf_of(w, mob)?;
            Some(Ear {
                client,
                pos,
                deaf: !crate::sound::can_hear(w, mob),
                pressure: pressure(w, pos),
            })
        })
        .collect();
    let mut out = Vec::new();
    for source in sources {
        let at = crate::explosions::turf_of(world, source);
        let source_pressure = at.and_then(|p| pressure(world, p));
        let Some(mut loops) = world.get_mut::<SoundLoops>(source) else {
            continue;
        };
        for token in loops.0.iter_mut().filter_map(|s| s.token.as_mut()) {
            let fresh = std::mem::take(&mut token.fresh);
            let mut listeners = HashMap::new();
            for ear in &ears {
                let Some(now) = at.and_then(|at| heard(token, at, source_pressure, ear)) else {
                    continue;
                };
                let known = token.listeners.get(&ear.client).copied();
                if fresh || known != Some(now) {
                    let update = known.is_some() && !fresh;
                    out.push(SendChannelSound {
                        client: ear.client,
                        sound: ChannelSound {
                            channel: token.channel,
                            sound: Some(SoundEvent {
                                file: token.file.clone(),
                                volume: now.volume,
                                speed: 1.0,
                                offset: Some(now.offset),
                            }),
                            update,
                            repeat: token.repeat,
                        },
                    });
                }
                listeners.insert(ear.client, now);
            }
            for &client in token.listeners.keys() {
                if !listeners.contains_key(&client) {
                    out.push(stop_message(client, token.channel));
                }
            }
            token.listeners = listeners;
        }
    }
    for message in out {
        world.write_message(message);
    }
}

#[cfg(test)]
mod tests {
    use bevy::ecs::message::Messages;

    use super::defs::*;
    use super::*;

    fn world() -> World {
        let mut world = World::new();
        world.init_resource::<Time>();
        world.init_resource::<SoundRng>();
        world.init_resource::<TokenChannels>();
        world.init_resource::<Messages<PlaySound>>();
        world.init_resource::<Messages<SendSound>>();
        world.init_resource::<Messages<SendChannelSound>>();
        world.add_observer(parent_removed);
        world
    }

    fn advance(world: &mut World, secs: f64) {
        world
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_secs_f64(secs));
        drive(world);
    }

    fn played(world: &mut World) -> Vec<String> {
        world
            .resource_mut::<Messages<PlaySound>>()
            .drain()
            .map(|s| s.sound)
            .collect()
    }

    fn spawn(world: &mut World) -> Entity {
        world.spawn(TilePos::new(5, 5, 1)).id()
    }

    #[test]
    fn start_then_mids_every_mid_length_then_end() {
        let mut world = world();
        let e = spawn(&mut world);
        start(&mut world, e, &GENERATOR);
        assert_eq!(
            played(&mut world),
            ["sound/machines/generator/generator_start.ogg"]
        );
        // Nothing until `start_length` (0.4 s), then a mid sound each 0.4 s.
        advance(&mut world, 0.2);
        assert!(played(&mut world).is_empty());
        advance(&mut world, 0.25);
        let first = played(&mut world);
        assert_eq!(first.len(), 1);
        assert!(first[0].contains("generator_mid"));
        advance(&mut world, 0.2);
        assert!(played(&mut world).is_empty());
        advance(&mut world, 0.25);
        assert_eq!(played(&mut world).len(), 1);
        // A second start does nothing while it runs.
        start(&mut world, e, &GENERATOR);
        assert!(played(&mut world).is_empty());
        stop(&mut world, e, GENERATOR.path);
        assert_eq!(
            played(&mut world),
            ["sound/machines/generator/generator_end.ogg"]
        );
        assert!(!is_active(&world, e, GENERATOR.path));
        advance(&mut world, 1.0);
        assert!(played(&mut world).is_empty());
    }

    #[test]
    fn stopped_before_the_loop_started_has_no_end_sound() {
        let mut world = world();
        let e = spawn(&mut world);
        start(&mut world, e, &GENERATOR);
        played(&mut world);
        stop(&mut world, e, GENERATOR.path);
        assert!(played(&mut world).is_empty());
    }

    const CHANCY: LoopDef = LoopDef {
        path: "/datum/looping_sound/test_chance",
        mid_sounds: &[("a.ogg", 1)],
        chance: 50.0,
        ..BASE
    };

    #[test]
    fn chance_skips_some_loops() {
        let mut world = world();
        let e = spawn(&mut world);
        start(&mut world, e, &CHANCY);
        played(&mut world);
        let mut heard = 0;
        for _ in 0..200 {
            advance(&mut world, 1.0);
            heard += played(&mut world).len();
        }
        assert!((60..140).contains(&heard), "about half: {heard}");
    }

    const ONCE: LoopDef = LoopDef {
        path: "/datum/looping_sound/test_once",
        mid_sounds: &[("a.ogg", 5), ("b.ogg", 1), ("c.ogg", 1)],
        each_once: true,
        ..BASE
    };
    const ORDER: LoopDef = LoopDef {
        path: "/datum/looping_sound/test_order",
        in_order: true,
        ..ONCE
    };

    #[test]
    fn each_once_plays_every_sound_per_round_and_in_order_keeps_the_order() {
        let mut world = world();
        let e = spawn(&mut world);
        start(&mut world, e, &ONCE);
        let mut got = played(&mut world);
        for _ in 0..5 {
            advance(&mut world, 1.0);
            got.extend(played(&mut world));
        }
        for round in got.chunks(3) {
            let mut round = round.to_vec();
            round.sort();
            assert_eq!(round, ["a.ogg", "b.ogg", "c.ogg"]);
        }
        let o = spawn(&mut world);
        start(&mut world, o, &ORDER);
        let mut got = played(&mut world);
        for _ in 0..3 {
            advance(&mut world, 1.0);
            got.extend(played(&mut world));
        }
        assert_eq!(got, ["a.ogg", "b.ogg", "c.ogg", "a.ogg"]);
    }

    #[test]
    fn weighted_mids_follow_their_weights() {
        let mut rng = CombatRng(3);
        let list = MICROWAVE.mid_sounds;
        let first = (0..1100)
            .filter(|_| pick_weight(list, &mut rng) == 0)
            .count();
        assert!((900..1080).contains(&first), "10 to 1: {first}");
    }

    #[test]
    fn native_repeat_only_for_one_plain_sound_on_a_token() {
        assert!(SUPERMATTER.native_repeat(SUPERMATTER.mid_sounds));
        assert!(GRAVGEN.native_repeat(GRAVGEN.mid_sounds));
        assert!(!MICROWAVE.native_repeat(MICROWAVE.mid_sounds));
        assert!(!LATHE_PRINT.native_repeat(LATHE_PRINT.mid_sounds));
        assert!(!CLOCK.native_repeat(CLOCK.mid_sounds));
        // BandaStation's volumes.
        assert_eq!(
            (
                SUPERMATTER.volume,
                SUPERMATTER_HUGBOX.volume,
                HYPERTORUS.volume,
                CLOCK.volume
            ),
            (20.0, 10.0, 30.0, 30.0)
        );
        assert_eq!(SUPERMATTER_HUGBOX.extra_range, -5.0);
    }

    #[test]
    fn a_token_follows_its_listener_and_frees_its_channel() {
        let mut world = world();
        let e = spawn(&mut world);
        let client = world.spawn_empty().id();
        let mob = world
            .spawn((PlayerMob { client }, TilePos::new(8, 5, 1)))
            .id();
        start(&mut world, e, &GRAVGEN);
        // The start sound plays on the token at once.
        advance(&mut world, 0.0);
        let sent: Vec<SendChannelSound> = world
            .resource_mut::<Messages<SendChannelSound>>()
            .drain()
            .collect();
        assert_eq!(sent.len(), 1);
        let first = sent[0].sound.clone();
        assert_eq!(first.channel, CHANNEL_HIGHEST_AVAILABLE);
        assert!(!first.update && !first.repeat);
        let s = first.sound.unwrap();
        assert_eq!(s.file, "sound/machines/gravgen/grav_gen_start.ogg");
        assert_eq!(s.offset, Some([-3, 0]));
        // After `start_length` the single mid sound repeats natively.
        advance(&mut world, 1.0);
        let sent: Vec<SendChannelSound> = world
            .resource_mut::<Messages<SendChannelSound>>()
            .drain()
            .collect();
        assert_eq!(sent.len(), 1);
        assert!(sent[0].sound.repeat && !sent[0].sound.update);
        // No new sound later: the client loops it.
        advance(&mut world, 5.0);
        assert!(
            world
                .resource_mut::<Messages<SendChannelSound>>()
                .drain()
                .next()
                .is_none()
        );
        // The listener moves: only an update.
        *world.get_mut::<TilePos>(mob).unwrap() = TilePos::new(9, 5, 1);
        advance(&mut world, 0.1);
        let sent: Vec<SendChannelSound> = world
            .resource_mut::<Messages<SendChannelSound>>()
            .drain()
            .collect();
        assert_eq!(sent.len(), 1);
        assert!(sent[0].sound.update);
        assert_eq!(listeners(&world, e, GRAVGEN.path), [client]);
        // Out of range (15 + 8): stopped.
        *world.get_mut::<TilePos>(mob).unwrap() = TilePos::new(40, 5, 1);
        advance(&mut world, 0.1);
        let sent: Vec<SendChannelSound> = world
            .resource_mut::<Messages<SendChannelSound>>()
            .drain()
            .collect();
        assert_eq!(sent.len(), 1);
        assert!(sent[0].sound.sound.is_none());
        assert!(listeners(&world, e, GRAVGEN.path).is_empty());
        // Back in range, then the machine goes: stopped, the channel is free again.
        *world.get_mut::<TilePos>(mob).unwrap() = TilePos::new(6, 5, 1);
        advance(&mut world, 0.1);
        world.resource_mut::<Messages<SendChannelSound>>().clear();
        world.despawn(e);
        let sent: Vec<SendChannelSound> = world
            .resource_mut::<Messages<SendChannelSound>>()
            .drain()
            .collect();
        assert_eq!(sent.len(), 1);
        assert!(sent[0].sound.sound.is_none());
        assert_eq!(
            world.resource_mut::<TokenChannels>().reserve(),
            Some(CHANNEL_HIGHEST_AVAILABLE)
        );
    }
}
