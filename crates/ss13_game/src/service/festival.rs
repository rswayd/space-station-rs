//! The festival god (server), after tg `/datum/religion_sect/music`,
//! `code/modules/religion/festival` (`instrument_rites.dm`, `festival_violin.dm`),
//! `/datum/component/smooth_tunes` and `/datum/status_effect/song`.
//!
//! - A convert gets a music beacon (`/obj/item/choice_beacon/music`) on their tile.
//! - "Cogitandi Fidis" makes the holy violin, which tells its player the song's lines, tempo
//!   and length when a song starts.
//! - "Portable Song Tuning" turns an instrument on the altar into a [`PortableAltar`]: a bible
//!   on it offers the song rites, five times.
//! - A song rite tunes the priest's next song ([`SmoothTunes`]): once they start playing a
//!   handheld instrument the tune shows its line, and every 2 s (`SSobj`) each human hearing it
//!   (but the priest, and those with holy antimagic) feels its effect; a song longer than
//!   [`FESTIVAL_SONG_LONG_ENOUGH`] (lines × tempo) that ends on its own gives the finishing
//!   effect too. Changing the tempo mid-song, or a repeat for the tunes that forbid one, loses it.
//!   - Evangelical Hymn: 0.2 favor a tick per non-holy listener with a player; blessing mood.
//!   - Illuminating Solo: the bright aura (1 minute) at the end.
//!   - Nullwave Vibrato: the dull aura (antimagic, 10 s, stacking) every tick.
//!   - Murderous Chord: 1 brute a tick (half on the holy); a moderate cut at the end.
//!   - Spiritual Lullaby: blurred eyes and yawns (not on the holy); 5 s of sleep at the end.
// shortcut: no particles, glow outline or lights (the light song's aura is only its lines);
// stationary instruments are not tuned (tg's TODO too).

use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_world::SpatialIndex;

use super::religion::{
    BASE_RITE, PerformingRite, RiteDef, glob_deity, holy_role, religion, religion_mut, rite_def,
    tell_mob,
};
use super::server::{client_of, item_path, say_around, sound_at};
use crate::chem::name_of;
use crate::instruments::Song;
use crate::obj_damage::is_under;

/// `FESTIVAL_SONG_LONG_ENOUGH`: lines × tempo (deciseconds) for the finishing effect.
pub const FESTIVAL_SONG_LONG_ENOUGH: f32 = 170.0;
const FESTIVAL_VIOLIN: &str = "/obj/item/instrument/violin/festival";
const INSTRUMENT: &str = "/obj/item/instrument";
const MUSIC_BEACON: &str = "/obj/item/choice_beacon/music";
const PRAY: &str = "sound/effects/pray.ogg";
const FWOOSH: &str = "sound/items/weapons/fwoosh.ogg";
/// `SSobj` `wait`: the tunes' `process`.
const TICK_SECS: f64 = 2.0;
/// `portable_song_tuning`: `charges = 5`.
const PORTABLE_CHARGES: u32 = 5;
/// `/datum/status_effect/song/antimagic` and `/light` `duration`, seconds.
const ANTIMAGIC_SECS: f64 = 10.0;
const LIGHT_SECS: f64 = 60.0;

/// `/datum/religion_rites/song_tuner` subtypes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SongTune {
    Evangelism,
    Light,
    Nullwave,
    Pain,
    Lullaby,
}

impl SongTune {
    pub fn rite(self) -> &'static RiteDef {
        match self {
            SongTune::Evangelism => rite_def! {
                name: "Evangelical Hymn",
                desc: "Spreads the word of your god, gaining favor for each non-holy listener. At \
                       the end of the song, you'll bless all listeners, improving mood.",
                ..BASE_RITE
            },
            SongTune::Light => rite_def! {
                name: "Illuminating Solo",
                desc: "Sing a bright song, lighting up the area around you. At the end of the \
                       song, you'll give some illumination to listeners.",
                ..BASE_RITE
            },
            SongTune::Nullwave => rite_def! {
                name: "Nullwave Vibrato",
                desc: "Sing a dull song, protecting listeners from magic.",
                favor_cost: 10.0,
                ..BASE_RITE
            },
            SongTune::Pain => rite_def! {
                name: "Murderous Chord",
                desc: "Sing a sharp song, cutting those around you. Works less effectively on \
                       fellow priests. At the end of the song, you'll open dozens of cuts on all \
                       listeners.",
                favor_cost: 10.0,
                ..BASE_RITE
            },
            SongTune::Lullaby => rite_def! {
                name: "Spiritual Lullaby",
                desc: "Sing a lullaby, tiring and slowing those around you. At the end of the \
                       song, you'll lull all listeners to sleep.",
                favor_cost: 40.0,
                ..BASE_RITE
            },
        }
    }

    /// `repeats_okay`.
    pub fn repeats_okay(self) -> bool {
        self == SongTune::Evangelism
    }

    /// `song_invocation_message`.
    fn invocation_message(self) -> &'static str {
        match self {
            SongTune::Evangelism => "You've prepared a holy song!",
            SongTune::Light => "You've prepared a bright song!",
            SongTune::Nullwave => "You've prepared an antimagic song!",
            SongTune::Pain => "You've prepared a painful song!",
            SongTune::Lullaby => "You've prepared a sleepy song!",
        }
    }

    /// `song_start_message`.
    fn start_message(self) -> (Span, &'static str) {
        match self {
            SongTune::Evangelism => (Span::Notice, "This music sounds blessed!"),
            SongTune::Light => (Span::Notice, "This music simply glows!"),
            SongTune::Nullwave => (Span::Notice, "This music makes you feel protected!"),
            SongTune::Pain => (Span::Danger, "This music cuts like a knife!"),
            SongTune::Lullaby => (Span::Warning, "This music's making you feel drowsy..."),
        }
    }
}

/// `/datum/component/smooth_tunes` on the priest.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct SmoothTunes {
    pub tune: SongTune,
    /// The instrument whose song is tuned, once it started.
    pub song: Option<Entity>,
    /// `viable_for_final_effect`.
    pub viable: bool,
    /// The tempo it started at.
    pub bpm: u32,
    pub next_tick: f64,
}

/// An instrument empowered by portable song tuning (its `religious_tool` with `charges`).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PortableAltar {
    pub charges: u32,
}

/// `/datum/status_effect/song/antimagic` and `/light`: which aura and until when.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct SongAura {
    pub antimagic: bool,
    pub until: f64,
}

/// The festival violin told its player about this playing already.
#[derive(Component, Clone, Copy, Debug)]
struct Analyzed;

pub(super) fn build(app: &mut App) {
    app.add_systems(
        Update,
        (
            smooth_tunes.run_if(any_with_component::<SmoothTunes>),
            analyze_songs,
            fade_auras.run_if(any_with_component::<SongAura>),
        ),
    );
}

/// `/datum/religion_sect/music/on_conversion`.
pub(super) fn on_conversion(world: &mut World, mob: Entity) {
    if let Some(pos) = crate::chem::map_pos(world, mob) {
        crate::construction::spawn_obj(world, MUSIC_BEACON, pos);
    }
}

fn now(world: &World) -> f64 {
    world
        .get_resource::<Time>()
        .map_or(0.0, |t| t.elapsed_secs_f64())
}

/// The nullwave song's `TRAIT_ANTIMAGIC`.
pub(super) fn song_antimagic(world: &World, mob: Entity) -> bool {
    world
        .get::<SongAura>(mob)
        .is_some_and(|a| a.antimagic && a.until > now(world))
}

/// `song.lines.len * song.tempo`, tempo in deciseconds (`600 / BPM`).
pub fn song_length(lines: usize, bpm: u32) -> f32 {
    lines as f32 * 600.0 / bpm.max(1) as f32
}

/// `portable_song_tuning/perform_rite`: an instrument on the altar.
pub(super) fn rite_target(
    world: &mut World,
    user: Entity,
    altar: Entity,
) -> Option<Option<Entity>> {
    let instrument = world.get::<TilePos>(altar).and_then(|&pos| {
        world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .find(|&e| is_under(&item_path(world, e), INSTRUMENT))
    });
    if instrument.is_none() {
        let what = name_of(world, altar);
        tell_mob(
            world,
            user,
            Span::Warning,
            format!("You need to place an instrument on {what} to do this!"),
        );
        return None;
    }
    Some(instrument)
}

/// `invoke_effect` of the violin, portable tuning and the song tuners.
pub(super) fn invoke_effect(
    world: &mut World,
    user: Entity,
    state: PerformingRite,
    pos: TilePos,
) -> bool {
    match state.rite {
        super::religion::Rite::HolyViolin => {
            if let Some(fidis) = crate::construction::spawn_obj(world, FESTIVAL_VIOLIN, pos) {
                let name = name_of(world, fidis);
                say_around(world, fidis, Span::Notice, format!("{name} appears!"), &[]);
            }
            sound_at(world, state.altar, PRAY, 50.0, true);
            true
        }
        super::religion::Rite::PortableSongTuning => {
            let Some(instrument) = state
                .target
                .filter(|&i| world.get::<TilePos>(i) == Some(&pos))
            else {
                tell_mob(world, user, Span::Warning, "Your target left the altar!");
                return false;
            };
            let name = name_of(world, instrument);
            say_around(
                world,
                instrument,
                Span::Notice,
                format!("{name} glows for a moment."),
                &[],
            );
            sound_at(world, state.altar, PRAY, 50.0, true);
            world.entity_mut(instrument).insert(PortableAltar {
                charges: PORTABLE_CHARGES,
            });
            true
        }
        super::religion::Rite::Tune(tune) => {
            tell_mob(world, user, Span::Notice, tune.invocation_message());
            let next_tick = now(world);
            world.entity_mut(user).insert(SmoothTunes {
                tune,
                song: None,
                viable: false,
                bpm: 0,
                next_tick,
            });
            true
        }
        _ => false,
    }
}

/// A rite performed on a portable altar used one of its charges.
pub(super) fn use_charge(world: &mut World, tool: Entity) {
    let Some(mut altar) = world.get_mut::<PortableAltar>(tool) else {
        return;
    };
    altar.charges = altar.charges.saturating_sub(1);
    if altar.charges == 0 {
        world.entity_mut(tool).remove::<PortableAltar>();
    }
}

/// The handheld instrument `mob` is playing now (`COMSIG_ATOM_STARTING_INSTRUMENT`).
fn playing_instrument(world: &mut World, mob: Entity) -> Option<Entity> {
    world
        .query::<(Entity, &Song)>()
        .iter(world)
        .find(|(_, s)| s.playing && s.player == Some(mob) && !s.stationary)
        .map(|(e, _)| e)
}

/// `smooth_tunes`: `start_singing`, `tempo_change`, `process` and `stop_singing`.
/// Cost: the (rare) tuned priests; a song query each until they start playing.
fn smooth_tunes(world: &mut World) {
    let now = now(world);
    let tuned: Vec<(Entity, SmoothTunes)> = world
        .query::<(Entity, &SmoothTunes)>()
        .iter(world)
        .map(|(e, t)| (e, *t))
        .collect();
    for (mob, mut tunes) in tuned {
        let Some(instrument) = tunes.song else {
            if let Some(instrument) = playing_instrument(world, mob) {
                start_singing(world, mob, &mut tunes, instrument, now);
                world.entity_mut(mob).insert(tunes);
            }
            continue;
        };
        let Some(song) = world.get::<Song>(instrument).cloned() else {
            world.entity_mut(mob).remove::<SmoothTunes>();
            continue;
        };
        if !song.playing || song.player != Some(mob) {
            stop_singing(world, mob, tunes, instrument, song.ended_naturally);
            continue;
        }
        if !tunes.tune.repeats_okay() && song.repeats_done > 0 {
            stop_singing(world, mob, tunes, instrument, false);
            continue;
        }
        if song.bpm != tunes.bpm && tunes.viable {
            tunes.viable = false;
            tell_mob(
                world,
                mob,
                Span::Warning,
                "Modifying the song mid-performance has removed your ability to perform the \
                 song finishing effect.",
            );
        }
        tunes.bpm = song.bpm;
        if now >= tunes.next_tick {
            tunes.next_tick = now + TICK_SECS;
            for listener in listeners(world, mob, instrument, &song) {
                song_effect(world, tunes.tune, listener);
            }
        }
        world.entity_mut(mob).insert(tunes);
    }
}

fn start_singing(
    world: &mut World,
    mob: Entity,
    tunes: &mut SmoothTunes,
    instrument: Entity,
    now: f64,
) {
    let Some(song) = world.get::<Song>(instrument).cloned() else {
        return;
    };
    tunes.song = Some(instrument);
    tunes.bpm = song.bpm;
    tunes.next_tick = now;
    tunes.viable = song_length(song.lines.len(), song.bpm) > FESTIVAL_SONG_LONG_ENOUGH;
    if !tunes.viable {
        tell_mob(
            world,
            mob,
            Span::Warning,
            "This song is too short, so it won't include the song finishing effect.",
        );
    }
    let (span, text) = tunes.tune.start_message();
    say_around(world, instrument, span, text.to_owned(), &[]);
}

fn stop_singing(
    world: &mut World,
    mob: Entity,
    tunes: SmoothTunes,
    instrument: Entity,
    finished: bool,
) {
    world.entity_mut(mob).remove::<SmoothTunes>();
    if !tunes.viable {
        return;
    }
    if !finished {
        tell_mob(
            world,
            mob,
            Span::Warning,
            "The song was interrupted, you cannot activate the finishing ability!",
        );
        return;
    }
    let Some(song) = world.get::<Song>(instrument).cloned() else {
        return;
    };
    for listener in listeners(world, mob, instrument, &song) {
        finish_effect(world, tunes.tune, listener);
    }
}

/// `linked_song.hearing_mobs`: the humans hearing the song, without the priest and those who
/// block holy magic (`charge_cost` aside).
fn listeners(world: &World, priest: Entity, instrument: Entity, song: &Song) -> Vec<Entity> {
    let Some(pos) = crate::chem::map_pos(world, instrument) else {
        return Vec::new();
    };
    crate::grenades::hearers(world, pos, song.range.max(0.0).ceil() as u32)
        .into_iter()
        .filter(|&m| m != priest && super::religion::is_human(world, m))
        .filter(|&m| {
            !crate::spells::can_block_magic(world, m, crate::spells::MAGIC_RESISTANCE_HOLY)
        })
        .collect()
}

/// `song_effect` of the tune on one listener.
fn song_effect(world: &mut World, tune: SongTune, listener: Entity) {
    let holy = holy_role(world, listener).is_some();
    match tune {
        SongTune::Evangelism => {
            // A player is required, to stop monkey farms.
            if religion(world).sect.is_some() && !holy && client_of(world, listener).is_some() {
                religion_mut(world).adjust_favor(0.2);
            }
        }
        SongTune::Light => {}
        SongTune::Nullwave => apply_aura(world, listener, true),
        SongTune::Pain => {
            let amount = if holy { 0.5 } else { 1.0 };
            hurt(world, listener, amount);
        }
        SongTune::Lullaby => {
            if holy {
                return;
            }
            const SLEEPY: [&str; 3] = [
                "The music is putting you to sleep...",
                "The music makes you nod off for a moment.",
                "You try to focus on staying awake through the song.",
            ];
            if world
                .resource_mut::<crate::combat::CombatRng>()
                .chance(20.0)
            {
                let line = super::religion::pick(world, &SLEEPY);
                tell_mob(world, listener, Span::Warning, line);
                crate::medical::diseases::emote(world, listener, "yawn");
            }
            crate::vision::set_eye_blur_if_lower(world, listener, 4.0);
        }
    }
}

/// `finish_effect` of the tune on one listener.
fn finish_effect(world: &mut World, tune: SongTune, listener: Entity) {
    match tune {
        SongTune::Evangelism => {
            crate::mood::add_event(world, listener, "blessing", &super::library::BLESSING);
        }
        SongTune::Light => apply_aura(world, listener, false),
        SongTune::Nullwave => {}
        SongTune::Pain => cut(world, listener),
        SongTune::Lullaby => {
            tell_mob(
                world,
                listener,
                Span::Danger,
                "Wow, the ending of that song was... pretty...",
            );
            crate::status_effects::adjust(
                world,
                listener,
                crate::status_effects::Incap::Sleeping,
                5.0,
            );
        }
    }
}

/// `adjust_brute_loss(amount)` on a listener.
fn hurt(world: &mut World, mob: Entity, amount: f32) {
    let (Some(mut health), Some(mut stat)) = (
        world.get::<crate::health::Health>(mob).cloned(),
        world.get::<crate::health::Stat>(mob).copied(),
    ) else {
        return;
    };
    let before = stat;
    crate::health::apply_damage(
        &mut health,
        &mut stat,
        crate::health::DamageType::Brute,
        amount,
    );
    world.entity_mut(mob).insert(health);
    if stat != before {
        world.entity_mut(mob).insert(stat);
    }
}

/// `force_wound_upwards(/datum/wound/slash/flesh/moderate/many_cuts)` on a random bodypart.
fn cut(world: &mut World, mob: Entity) {
    use crate::medical::wounds::{Limbs, Series, WoundKind, apply_wound};
    let Some(parts) = world.get::<crate::health::Health>(mob).map(|h| {
        crate::zone::BodyZone::ALL
            .into_iter()
            .filter(|&z| h.has_part(z))
            .collect::<Vec<_>>()
    }) else {
        return;
    };
    if parts.is_empty() {
        return;
    }
    let i = world
        .resource_mut::<crate::combat::CombatRng>()
        .range(0, parts.len() as u32 - 1);
    if let Some(mut limbs) = world.get_mut::<Limbs>(mob) {
        let limb = limbs.limb_mut(parts[i as usize]);
        // "Upwards": only when no cut of the series is there yet.
        if !limb
            .wounds
            .iter()
            .any(|w| w.kind.def().series == Series::Slash)
        {
            apply_wound(limb, WoundKind::SlashModerate);
        }
    }
}

/// `apply_status_effect(/datum/status_effect/song/antimagic or /light)`: a new aura replaces the
/// other one; the same one lasts its duration longer (`refresh`).
fn apply_aura(world: &mut World, mob: Entity, antimagic: bool) {
    let now = now(world);
    let duration = if antimagic {
        ANTIMAGIC_SECS
    } else {
        LIGHT_SECS
    };
    let current = world.get::<SongAura>(mob).copied();
    match current {
        Some(aura) if aura.antimagic == antimagic && aura.until > now => {
            world.entity_mut(mob).insert(SongAura {
                antimagic,
                until: aura.until + duration,
            });
        }
        _ => {
            if let Some(old) = current.filter(|a| a.until > now) {
                aura_fades(world, mob, old.antimagic);
            }
            let name = name_of(world, mob);
            let desc = aura_desc(antimagic);
            say_around(
                world,
                mob,
                Span::Notice,
                format!("{name} is coated with a {desc} aura!"),
                &[],
            );
            sound_at(world, mob, FWOOSH, 75.0, false);
            world.entity_mut(mob).insert(SongAura {
                antimagic,
                until: now + duration,
            });
        }
    }
}

fn aura_desc(antimagic: bool) -> &'static str {
    if antimagic { "dull" } else { "bright" }
}

fn aura_fades(world: &mut World, mob: Entity, antimagic: bool) {
    let name = name_of(world, mob);
    let desc = aura_desc(antimagic);
    say_around(
        world,
        mob,
        Span::Warning,
        format!("{name}'s {desc} aura fades away..."),
        &[],
    );
}

fn fade_auras(world: &mut World) {
    let now = now(world);
    let done: Vec<(Entity, bool)> = world
        .query::<(Entity, &SongAura)>()
        .iter(world)
        .filter(|(_, a)| a.until <= now)
        .map(|(e, a)| (e, a.antimagic))
        .collect();
    for (mob, antimagic) in done {
        world.entity_mut(mob).remove::<SongAura>();
        aura_fades(world, mob, antimagic);
    }
}

/// `festival/on_instrument_start` -> `analyze_song`: once per playing of a holy violin.
fn analyze_songs(world: &mut World) {
    let songs: Vec<(Entity, bool, Option<Entity>, usize, u32, bool)> = world
        .query::<(Entity, &Song, &DmItem, Has<Analyzed>)>()
        .iter(world)
        .filter(|(_, _, i, _)| is_under(&i.path, FESTIVAL_VIOLIN))
        .map(|(e, s, _, done)| (e, s.playing, s.player, s.lines.len(), s.bpm, done))
        .collect();
    for (violin, playing, player, lines, bpm, done) in songs {
        match (playing, done) {
            (true, false) => {
                world.entity_mut(violin).insert(Analyzed);
                if let Some(player) = player {
                    analyze_song(world, violin, player, lines, bpm);
                }
            }
            (false, true) => {
                world.entity_mut(violin).remove::<Analyzed>();
            }
            _ => {}
        }
    }
}

fn analyze_song(world: &mut World, violin: Entity, player: Entity, lines: usize, bpm: u32) {
    let name = name_of(world, violin);
    let tempo = 600.0 / bpm.max(1) as f32;
    let length = song_length(lines, bpm);
    let deity = glob_deity(world);
    let text = format!(
        "{name} speaks to you...\n\"This song has {lines} lines and a tempo of {tempo}.\"\n\
         \"Multiplying these together gives a song length of {length}.\"\n\"To get a finishing \
         effect from {deity}, your songs must conclude with a length of \
         {FESTIVAL_SONG_LONG_ENOUGH}.\""
    );
    tell_mob(world, player, Span::Notice, text);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_song_needs_its_length_for_the_finish() {
        // 120 BPM is a tempo of 5 deciseconds: 34 lines make 170, not enough; 35 do.
        assert_eq!(song_length(34, 120), 170.0);
        assert!(song_length(34, 120) <= FESTIVAL_SONG_LONG_ENOUGH);
        assert!(song_length(35, 120) > FESTIVAL_SONG_LONG_ENOUGH);
    }

    #[test]
    fn only_the_hymn_takes_repeats() {
        assert!(SongTune::Evangelism.repeats_okay());
        for tune in [
            SongTune::Light,
            SongTune::Nullwave,
            SongTune::Pain,
            SongTune::Lullaby,
        ] {
            assert!(!tune.repeats_okay());
            assert_eq!(tune.rite().length, 10.0);
        }
        assert_eq!(SongTune::Lullaby.rite().favor_cost, 40.0);
    }

    #[test]
    fn auras_replace_each_other_and_stack_themselves() {
        let mut world = World::new();
        world.init_resource::<Time>();
        let mob = world.spawn_empty().id();
        apply_aura(&mut world, mob, true);
        apply_aura(&mut world, mob, true);
        let aura = *world.get::<SongAura>(mob).unwrap();
        assert!(aura.antimagic);
        assert_eq!(aura.until, 2.0 * ANTIMAGIC_SECS);
        assert!(song_antimagic(&world, mob));
        apply_aura(&mut world, mob, false);
        let aura = *world.get::<SongAura>(mob).unwrap();
        assert!(!aura.antimagic);
        assert_eq!(aura.until, LIGHT_SECS);
        assert!(!song_antimagic(&world, mob));
    }

    #[test]
    fn a_portable_altar_runs_out() {
        let mut world = World::new();
        let violin = world.spawn(PortableAltar { charges: 2 }).id();
        use_charge(&mut world, violin);
        assert_eq!(world.get::<PortableAltar>(violin).unwrap().charges, 1);
        use_charge(&mut world, violin);
        assert!(world.get::<PortableAltar>(violin).is_none());
    }
}
