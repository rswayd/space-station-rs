//! Positional sound (server): tg `playsound` / `playsound_local` / `sound_to_playing_players`
//! (`code/game/sound/sound.dm`), `get_sfx` (`sound_keys/`) and the channel reservations of
//! `SSsounds` (`code/controllers/subsystem/sounds.dm`).
//!
//! A system writes a [`PlaySound`]; at the end of the tick each player mob within
//! `SOUND_RANGE + extrarange` tiles (euclidean over x and y) that is not deaf gets a
//! [`ChannelSound`] with its own volume: distance falloff (`CALCULATE_SOUND_VOLUME_RATIO`), times
//! the pressure factor of the thinner of the two tiles' air (space 0, at least 0.15 next to the
//! source), and its offset for stereo panning. `vary` picks `rand(32000, 55000)` Hz once per
//! sound, as tg. SFX keys pick one of their files (weighted) once per sound.
//!
//! - Levels: a sound is also heard on the level above when the turf above the source shows the
//!   level below (open space, glass floors: `istransparentturf`), and on the level below when the
//!   source's own turf does; BYOND places those listeners 5 tiles away vertically, so near the
//!   edge of the range its own rolloff ([`z_rolloff`]) makes them quieter.
//! - `ignore_walls = FALSE`: only mobs on tiles BYOND `view()` sees from the source (or from the
//!   turf above/below it) hear it (`get_hearers_in_view`); ghosts on the source's level hear it
//!   anyway.
//! - The listener's room reverb (`sound.environment`): the mob's override (drugginess, being
//!   drunk), else its area's `sound_environment`; none without `use_reverb` or for sounds without
//!   a source.
//! - `volume_preference` travels as the sound's [`SoundCategory`]; the client scales it by the
//!   player's volume of that kind (`crate::sound_client`).
//! - `channel`: 0 plays alongside everything else; a channel number replaces what plays there
//!   for that client ([`SoundChannels`] hands out reserved ones, [`stop_sound_channel`] stops
//!   one).
//!
//! Cost: one pass over the player mobs per sound (players × sounds per tick, a distance check
//! each) — a few hundred mobs × a few sounds per tick; a sound that does not carry through walls
//! adds one `view()` of up to 31×31 tiles (an opacity lookup each) per hearing level, and only
//! suppressed or otherwise rare sounds ask for it; one area lookup per listener that hears it.
// shortcut: tg hands every sound a rotating free channel (`random_available_channel`); here
// channel 0 is "any free channel" on the client, which is what those channels are for. No
// `TRAIT_SOUND_DEBUGGED` admin readout, no `wait` (BandaStation's queued sounds).

use std::collections::{HashMap, HashSet};

use bevy::prelude::*;
use ss13_mapload::sound::{SFX_FILE, SfxData};
use ss13_mapload::{Areas, DmArea, DmItem, MapLoadPlugin, Opaque};
use ss13_net::server::{ClientToken, NetServerSystems};
use ss13_net::sound::server::{SendChannelSound, SendSound};
use ss13_shared::protocol::{
    CHANNEL_HIGHEST_AVAILABLE, ChannelSound, SOUND_ENVIRONMENT_DRUGGED, SOUND_ENVIRONMENT_NONE,
    SOUND_ENVIRONMENT_PSYCHOTIC, SoundCategory, SoundEvent, SoundFx,
};
use ss13_shared::{LevelStack, TilePos};
use ss13_world::SpatialIndex;

use crate::combat::CombatRng;
use crate::genetics::{Dna, mutation_id};
use crate::ghost::Ghost;
use crate::inventory::{CarriedBy, Inventory};
use crate::server::PlayerMob;
use crate::storage::StoredIn;

/// `SOUND_RANGE`.
pub const SOUND_RANGE: f32 = 15.0;
/// `SOUND_FALLOFF_EXPONENT`.
pub const SOUND_FALLOFF_EXPONENT: f32 = 2.5;
/// `SOUND_MINIMUM_PRESSURE`, `ONE_ATMOSPHERE` (kPa).
pub const SOUND_MINIMUM_PRESSURE: f64 = 10.0;
pub const ONE_ATMOSPHERE: f64 = 101.325;
/// `MEDIUM_RANGE_SOUND_EXTRARANGE`, `SHORT_RANGE_SOUND_EXTRARANGE`.
pub const MEDIUM_RANGE_SOUND_EXTRARANGE: f32 = -3.0;
pub const SHORT_RANGE_SOUND_EXTRARANGE: f32 = -7.0;
/// `playsound_local`'s `min_volume`: `playsound` does not pass its own (3) on, so every
/// listener gets this one.
pub const MIN_VOLUME: f32 = 5.0;
const EARMUFFS: &str = "/obj/item/clothing/ears/earmuffs";
const DEAF_MUTATION: &str = "/datum/mutation/deaf";

/// tg `playsound(source, soundin, vol, vary, extrarange, falloff_exponent, frequency, ...)`.
/// `source: None` is `sound_to_playing_players`: every player mob, no distance or pressure.
#[derive(Message, Clone, Debug, PartialEq)]
pub struct PlaySound {
    pub source: Option<TilePos>,
    /// A file (`sound/...ogg`) or an SFX key (`"swing_hit"`).
    pub sound: String,
    pub vol: f32,
    pub vary: bool,
    pub extrarange: f32,
    pub falloff_exponent: f32,
    /// BYOND `frequency`: Hz, or a speed multiplier within ±100; only used with `vary`.
    pub frequency: Option<f32>,
    pub pressure_affected: bool,
    pub ignore_walls: bool,
    pub falloff_distance: f32,
    pub min_volume: f32,
    /// BYOND `sound.pitch` of a sound datum (tg energy guns): a pitch multiplier that keeps the
    /// length.
    pub pitch: f32,
    /// BYOND `sound.frequency` set on the sound datum itself (instrument notes): a playback
    /// speed multiplier that applies with or without `vary`.
    pub speed: f32,
    /// tg `channel`: 0 for any free one.
    pub channel: u16,
    /// tg `use_reverb`: the listener's room reverb applies.
    pub use_reverb: bool,
    /// tg `volume_preference`.
    pub category: SoundCategory,
}

impl PlaySound {
    /// `playsound(source, sound, vol, vary)` with tg's defaults for the rest.
    pub fn new(source: TilePos, sound: impl Into<String>, vol: f32, vary: bool) -> Self {
        Self {
            source: Some(source),
            sound: sound.into(),
            vol,
            vary,
            extrarange: 0.0,
            falloff_exponent: SOUND_FALLOFF_EXPONENT,
            frequency: None,
            pressure_affected: true,
            ignore_walls: true,
            falloff_distance: 0.0,
            min_volume: MIN_VOLUME,
            pitch: 1.0,
            speed: 1.0,
            channel: 0,
            use_reverb: true,
            category: SoundCategory::Game,
        }
    }

    /// `sound_to_playing_players(sound, volume, vary)`.
    pub fn to_playing_players(sound: impl Into<String>, vol: f32, vary: bool) -> Self {
        Self {
            source: None,
            pressure_affected: false,
            ..Self::new(TilePos::new(0, 0, 0), sound, vol, vary)
        }
    }

    pub fn extrarange(mut self, extrarange: f32) -> Self {
        self.extrarange = extrarange;
        self
    }

    pub fn falloff_exponent(mut self, exponent: f32) -> Self {
        self.falloff_exponent = exponent;
        self
    }

    pub fn frequency(mut self, frequency: f32) -> Self {
        self.frequency = Some(frequency);
        self
    }

    pub fn ignore_walls(mut self, ignore_walls: bool) -> Self {
        self.ignore_walls = ignore_walls;
        self
    }

    pub fn falloff_distance(mut self, distance: f32) -> Self {
        self.falloff_distance = distance;
        self
    }

    pub fn pitch(mut self, pitch: f32) -> Self {
        self.pitch = pitch;
        self
    }

    pub fn speed(mut self, speed: f32) -> Self {
        self.speed = speed;
        self
    }

    pub fn channel(mut self, channel: u16) -> Self {
        self.channel = channel;
        self
    }

    pub fn use_reverb(mut self, use_reverb: bool) -> Self {
        self.use_reverb = use_reverb;
        self
    }

    pub fn pressure_affected(mut self, pressure_affected: bool) -> Self {
        self.pressure_affected = pressure_affected;
        self
    }

    pub fn volume_preference(mut self, category: SoundCategory) -> Self {
        self.category = category;
        self
    }
}

/// `playsound(...)` from an exclusive system: delivered at the end of this tick.
pub fn playsound(world: &mut World, sound: PlaySound) {
    if !sound.sound.is_empty() {
        world.write_message(sound);
    }
}

/// Turns this tick's [`PlaySound`]s into per-client sounds (`PostUpdate`): systems writing
/// sounds in `PostUpdate` run before it.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SoundSystems;

/// tg `get_sfx` table, from `ss13-mapload convert` (`<assets>/sfx.ron`).
#[derive(Resource, Clone, Debug, Default)]
pub struct Sfx(pub SfxData);

/// Sound rolls (`vary`, SFX picks): own stream, so sounds never shift combat rolls.
#[derive(Resource, Debug, Default)]
pub struct SoundRng(pub CombatRng);

pub struct SoundServerPlugin;

impl Plugin for SoundServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<PlaySound>()
            .init_resource::<Sfx>()
            .init_resource::<SoundRng>()
            .init_resource::<SoundChannels>()
            .add_systems(PreStartup, load_sfx)
            .add_systems(
                PostUpdate,
                (
                    crate::footsteps::play_footsteps.before(SoundSystems),
                    deliver.in_set(SoundSystems).before(NetServerSystems::Send),
                    free_dead_owners,
                ),
            );
    }
}

/// tg `random_channels_min`: the lowest channels are never reserved.
pub const RANDOM_CHANNELS_MIN: u16 = 50;

/// tg `SSsounds` channel reservations: a looping or long sound that must be stopped or updated
/// later gets a channel of its own, from [`CHANNEL_HIGHEST_AVAILABLE`] down, never into the
/// lowest [`RANDOM_CHANNELS_MIN`]. A channel reserved for an entity is freed when it is gone
/// (tg `tracked_datum_deleted`).
#[derive(Resource, Debug, Default)]
pub struct SoundChannels {
    /// Channel -> its owner (`None`: a `reserve_sound_channel` without one).
    reserved: HashMap<u16, Option<Entity>>,
}

impl SoundChannels {
    /// `reserve_sound_channel` (`owner: None`) / `reserve_sound_channel_for_datum`; `None` once
    /// every reservable channel is taken.
    pub fn reserve(&mut self, owner: Option<Entity>) -> Option<u16> {
        let channel = (RANDOM_CHANNELS_MIN + 1..=CHANNEL_HIGHEST_AVAILABLE)
            .rev()
            .find(|c| !self.reserved.contains_key(c))?;
        self.reserved.insert(channel, owner);
        Some(channel)
    }

    /// `free_sound_channel`.
    pub fn free(&mut self, channel: u16) -> bool {
        self.reserved.remove(&channel).is_some()
    }

    /// `free_datum_channels`.
    pub fn free_owner(&mut self, owner: Entity) {
        self.reserved.retain(|_, o| *o != Some(owner));
    }

    /// `free_datumless_channels`.
    pub fn free_datumless(&mut self) {
        self.reserved.retain(|_, o| o.is_some());
    }

    /// Who reserved `channel`: `Some(None)` for a reservation without an owner.
    pub fn owner(&self, channel: u16) -> Option<Option<Entity>> {
        self.reserved.get(&channel).copied()
    }
}

/// tg `tracked_datum_deleted`: the channels of despawned owners are free again.
fn free_dead_owners(mut channels: ResMut<SoundChannels>, alive: Query<Entity>) {
    let dead = |o: &Option<Entity>| o.is_some_and(|e| !alive.contains(e));
    if channels.reserved.values().any(dead) {
        channels.reserved.retain(|_, o| !dead(o));
    }
}

/// tg `/mob/proc/stop_sound_channel(chan)` for the mob's `client`; channel 0 stops every sound.
pub fn stop_sound_channel(world: &mut World, client: Entity, channel: u16) {
    world.write_message(SendChannelSound {
        client,
        sound: ChannelSound {
            channel,
            sound: None,
            ..default()
        },
    });
}

/// tg `/mob/proc/set_sound_channel_volume(channel, volume)` (`SOUND_UPDATE`).
pub fn set_sound_channel_volume(world: &mut World, client: Entity, channel: u16, volume: f32) {
    world.write_message(SendChannelSound {
        client,
        sound: ChannelSound {
            channel,
            sound: Some(SoundEvent {
                file: String::new(),
                volume,
                speed: 1.0,
                offset: None,
            }),
            update: true,
            ..default()
        },
    });
}

/// tg `SEND_SOUND(world, sound(file, volume = volume))`: every connected client, the lobby
/// included, deaf or not.
pub fn send_sound_to_world(world: &mut World, file: &str, volume: f32) {
    let clients: Vec<Entity> = world
        .query_filtered::<Entity, With<ClientToken>>()
        .iter(world)
        .collect();
    for client in clients {
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
}

/// tg `/datum/antagonist/proc/play_stinger`: `playsound_local` of the antagonist's stinger on
/// its own turf at 100, no pressure, no reverb.
pub fn play_stinger(world: &mut World, mob: Entity, file: &str) {
    let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) else {
        return;
    };
    if !can_hear(world, mob) {
        return;
    }
    world.write_message(SendSound {
        client,
        sound: SoundEvent {
            file: file.to_owned(),
            volume: 100.0,
            speed: 1.0,
            offset: Some([0, 0]),
        },
    });
}

/// DM `pick(files)` from the sound rolls.
pub fn pick_file<'a>(world: &mut World, files: &[&'a str]) -> &'a str {
    let Some(mut rng) = world.get_resource_mut::<SoundRng>() else {
        return files.first().copied().unwrap_or_default();
    };
    if files.is_empty() {
        return "";
    }
    files[rng.0.range(1, files.len() as u32) as usize - 1]
}

fn load_sfx(world: &mut World) {
    let Some(path) = world
        .get_resource::<MapLoadPlugin>()
        .map(|m| m.assets.join(SFX_FILE))
    else {
        return;
    };
    match SfxData::read(&path) {
        Ok(data) => world.insert_resource(Sfx(data)),
        // Fixture assets have no table: keys then play as given, like tg's `get_sfx` fallback.
        Err(e) => info!("no sfx table: {e:#}"),
    }
}

/// `CALCULATE_SOUND_VOLUME_RATIO` applied as in `playsound_local`.
pub fn falloff_volume(
    vol: f32,
    distance: f32,
    max_distance: f32,
    falloff_distance: f32,
    falloff_exponent: f32,
    min_volume: f32,
) -> f32 {
    let ratio = ((distance - falloff_distance).max(0.0)
        / (max_distance.max(distance) - falloff_distance))
        .powf(1.0 / falloff_exponent);
    vol - ratio * (vol - min_volume)
}

/// `playsound_local`'s pressure factor: kPa on the hearer's and the source's tiles (`None`: no
/// air, space in tg's sense).
pub fn pressure_factor(hearer: Option<f64>, source: Option<f64>, distance: f32) -> f32 {
    let factor = match (hearer, source) {
        (Some(h), Some(s)) => {
            let pressure = h.min(s);
            if pressure < ONE_ATMOSPHERE {
                ((pressure - SOUND_MINIMUM_PRESSURE) / (ONE_ATMOSPHERE - SOUND_MINIMUM_PRESSURE))
                    .max(0.0)
            } else {
                1.0
            }
        }
        _ => 0.0,
    };
    let factor = factor as f32;
    if distance <= 1.0 {
        factor.max(0.15)
    } else {
        factor
    }
}

/// BYOND `sound.frequency` as a playback speed: within ±100 a multiplier, else Hz.
pub fn speed_of(frequency: f32) -> f32 {
    if frequency.abs() <= 100.0 {
        frequency
    } else {
        frequency / 44100.0
    }
}

/// tg `get_clamped_volume` of an item hit.
pub fn clamped_volume(force: f32, w_class: u8) -> f32 {
    let w = f32::from(w_class);
    if w_class == 0 {
        0.0
    } else if force > 0.0 {
        ((force + w) * 4.0).clamp(30.0, 100.0)
    } else {
        (w * 6.0).clamp(10.0, 100.0)
    }
}

/// An item's `hitsound` after `/obj/item/Initialize`: its own, else by `damtype`.
pub fn hitsound(item: &DmItem) -> &str {
    use ss13_mapload::sound::files;
    match item.damtype.as_str() {
        _ if !item.hitsound.is_empty() => &item.hitsound,
        "fire" => files::WELDER,
        "brute" => files::SFX_SWING_HIT,
        _ => "",
    }
}

/// tg `TRAIT_DEAF` sources ported: the deafness mutation, the Deaf quirk and earmuffs (the
/// ears' own in `crate::medical::organs::ear_deaf`).
// shortcut: no deafness disease.
fn is_deaf(
    dna: Option<&Dna>,
    inventory: Option<&Inventory>,
    quirks: Option<&crate::quirks::Quirks>,
    items: &Query<&DmItem>,
) -> bool {
    quirks.is_some_and(|q| q.has(crate::quirks::Quirk::Deaf))
        || dna.is_some_and(|d| {
            mutation_id(DEAF_MUTATION).is_some_and(|id| d.get_mutation(id).is_some())
        })
        || inventory
            .and_then(|i| i.ears)
            .and_then(|e| items.get(e).ok())
            .is_some_and(|i| ss13_mapload::fidelity::is_under(&i.path, EARMUFFS))
}

/// [`is_deaf`] of a mob, from an exclusive system.
pub fn mob_is_deaf(world: &World, mob: Entity) -> bool {
    let dna = world.get::<Dna>(mob);
    crate::quirks::has(world, mob, crate::quirks::Quirk::Deaf)
        || dna.is_some_and(|d| {
            mutation_id(DEAF_MUTATION).is_some_and(|id| d.get_mutation(id).is_some())
        })
        || world
            .get::<Inventory>(mob)
            .and_then(|i| i.ears)
            .and_then(|e| world.get::<DmItem>(e))
            .is_some_and(|i| ss13_mapload::fidelity::is_under(&i.path, EARMUFFS))
}

/// tg `can_hear()` of a mob, from an exclusive system: not deaf ([`mob_is_deaf`], the ears).
pub fn can_hear(world: &World, mob: Entity) -> bool {
    use crate::medical::organs::{Organ, OrganHost, Organs, ear_deaf};
    !mob_is_deaf(world, mob)
        && !ear_deaf(
            world.get::<Organs>(mob),
            world.get::<OrganHost>(mob).is_some(),
            |e| world.get::<Organ>(e),
        )
}

pub(crate) fn pick_sfx(sfx: &Sfx, rng: &mut CombatRng, sound: &str) -> String {
    let Some(files) = sfx.0.sfx.get(sound) else {
        return sound.to_owned();
    };
    let total: u32 = files.iter().map(|(_, w)| w).sum();
    if total == 0 {
        return files[0].0.clone();
    }
    let mut roll = rng.range(1, total);
    for (file, weight) in files {
        if roll <= *weight {
            return file.clone();
        }
        roll -= weight;
    }
    files[files.len() - 1].0.clone()
}

type Listener<'a> = (
    &'a PlayerMob,
    Entity,
    Option<&'a Dna>,
    Option<&'a Inventory>,
    Has<Ghost>,
    (
        Option<&'a crate::medical::organs::Organs>,
        Has<crate::medical::organs::OrganHost>,
        Option<&'a crate::quirks::Quirks>,
        Option<&'a crate::status_effects::StatusEffects>,
        Option<&'a crate::chem::MobReagents>,
    ),
);

/// tg `get_hearers_in_view(max_distance, source)`: the tiles BYOND `view()` sees from `source`
/// (opaque tiles are seen and stop sight; off the map nothing is seen through).
pub fn hearing_view(
    source: TilePos,
    max_distance: f32,
    opaque: impl Fn(TilePos) -> bool,
    in_bounds: impl Fn(TilePos) -> bool,
) -> HashSet<TilePos> {
    let range = max_distance.ceil().clamp(0.0, f32::from(u8::MAX)) as u8;
    let mut seen = HashSet::new();
    crate::silicon::cameras::camera_sees(source, range, &opaque, &in_bounds, |p| {
        seen.insert(p);
    });
    seen
}

/// Where `playsound` looks for hearers: the source's turf, the turf above it when that one shows
/// the level below (`istransparentturf(above_turf)`), the turf below when the source's own turf
/// shows it.
pub fn hearing_origins(
    source: TilePos,
    levels: Option<&LevelStack>,
    transparent: impl Fn(TilePos) -> bool,
) -> Vec<TilePos> {
    let mut origins = vec![source];
    let Some(levels) = levels else {
        return origins;
    };
    if let Some(above) = levels.step(source, true)
        && transparent(above)
    {
        origins.push(above);
    }
    if let Some(below) = levels.step(source, false)
        && transparent(source)
    {
        origins.push(below);
    }
    origins
}

/// BYOND's own rolloff of a 3D sound (FMOD's inverse distance past `sound.falloff`, which tg sets
/// to the sound's range): tg puts a listener `levels` z-levels away 5 tiles per level above or
/// below the source, so only those can be past it.
pub fn z_rolloff(distance: f32, levels: u32, falloff: f32) -> f32 {
    let height = levels as f32 * 5.0;
    let d = (distance * distance + height * height).sqrt();
    if d > falloff && falloff > 0.0 {
        falloff / d
    } else {
        1.0
    }
}

/// `playsound_local`'s `sound.environment`: the mob's `sound_environment_override` (drugginess
/// sets `SOUND_ENVIRONMENT_DRUGGED`, being drunk past tipsy `SOUND_ENVIRONMENT_PSYCHOTIC`), else
/// the area's `sound_environment`.
pub fn listener_environment(drugged: bool, drunk: f32, area: i8) -> i8 {
    if drugged {
        SOUND_ENVIRONMENT_DRUGGED
    } else if drunk >= ss13_reagents::metabolism::TIPSY_THRESHOLD {
        SOUND_ENVIRONMENT_PSYCHOTIC
    } else {
        area
    }
}

#[allow(clippy::too_many_arguments)]
fn deliver(
    mut sounds: MessageReader<PlaySound>,
    listeners: Query<Listener>,
    locs: Query<(Option<&TilePos>, Option<&CarriedBy>, Option<&StoredIn>)>,
    items: Query<&DmItem>,
    organs: Query<&crate::medical::organs::Organ>,
    (sfx, mut rng): (Res<Sfx>, ResMut<SoundRng>),
    atmos: Option<Res<ss13_atmos::Atmos>>,
    (index, opaque, levels): (
        Res<SpatialIndex>,
        Query<(), With<Opaque>>,
        Option<Res<LevelStack>>,
    ),
    (areas, area_vars): (Option<Res<Areas>>, Query<&DmArea>),
    mut out: MessageWriter<SendChannelSound>,
) {
    let turf = |mut e: Entity| {
        for _ in 0..8 {
            let (pos, carried, stored) = locs.get(e).ok()?;
            if let Some(pos) = pos {
                return Some(*pos);
            }
            e = carried.map(|c| c.0).or(stored.map(|s| s.0))?;
        }
        None
    };
    let pressure = |pos: TilePos| match &atmos {
        Some(a) => a.pressure(pos),
        // No atmos simulation at all (some test servers): air everywhere.
        None => Some(ONE_ATMOSPHERE),
    };
    let area_environment = |pos: TilePos| {
        areas
            .as_ref()
            .and_then(|a| a.area_at(pos))
            .and_then(|a| area_vars.get(a).ok())
            .map_or(SOUND_ENVIRONMENT_NONE, |a| a.sound_environment)
    };
    for sound in sounds.read() {
        if sound.sound.is_empty() {
            continue;
        }
        let rng = &mut rng.0;
        let file = pick_sfx(&sfx, rng, &sound.sound);
        let speed = if sound.vary {
            speed_of(
                sound
                    .frequency
                    .unwrap_or_else(|| rng.range(32000, 55000) as f32),
            )
        } else {
            1.0
        } * sound.speed;
        let max_distance = SOUND_RANGE + sound.extrarange;
        // Each level that hears it, with the tiles seen from there when walls stop it.
        let origins: Vec<(TilePos, Option<HashSet<TilePos>>)> = match sound.source {
            None => Vec::new(),
            Some(source) => hearing_origins(source, levels.as_deref(), |p| index.transparent(p))
                .into_iter()
                .map(|origin| {
                    let view = (!sound.ignore_walls).then(|| {
                        hearing_view(
                            origin,
                            max_distance,
                            |p| index.on_tile(p).iter().any(|&e| opaque.contains(e)),
                            |p| index.in_bounds(p),
                        )
                    });
                    (origin, view)
                })
                .collect(),
        };
        for (player, mob, dna, inventory, ghost, (owned, host, quirks, status, reagents)) in
            &listeners
        {
            if is_deaf(dna, inventory, quirks, &items)
                || crate::medical::organs::ear_deaf(owned, host, |e| organs.get(e).ok())
            {
                continue;
            }
            let Some(at) = turf(mob) else {
                continue;
            };
            let (volume, offset, environment) = match sound.source {
                None => (sound.vol, None, SOUND_ENVIRONMENT_NONE),
                Some(source) => {
                    let Some((_, view)) = origins.iter().find(|(o, _)| o.z == at.z) else {
                        continue;
                    };
                    let (dx, dy) = (source.x - at.x, source.y - at.y);
                    let distance = ((dx * dx + dy * dy) as f32).sqrt();
                    // Ghosts on the source's level hear it anyway (`dead_players_by_zlevel`).
                    let unseen = view
                        .as_ref()
                        .is_some_and(|v| !(ghost && at.z == source.z) && !v.contains(&at));
                    if distance > max_distance || unseen {
                        continue;
                    }
                    let mut volume = falloff_volume(
                        sound.vol,
                        distance,
                        max_distance,
                        sound.falloff_distance,
                        sound.falloff_exponent,
                        sound.min_volume,
                    );
                    if sound.pressure_affected {
                        volume *= pressure_factor(pressure(at), pressure(source), distance);
                    }
                    if at.z != source.z {
                        volume *= z_rolloff(distance, 1, max_distance);
                    }
                    let environment = if sound.use_reverb {
                        listener_environment(
                            status.is_some_and(|s| s.drugginess > 0.0),
                            reagents.map_or(0.0, |r| r.drunk),
                            area_environment(at),
                        )
                    } else {
                        SOUND_ENVIRONMENT_NONE
                    };
                    (volume, Some([dx as i16, dy as i16]), environment)
                }
            };
            // A silent sound is not sent: the client would play nothing.
            if volume <= 0.0 {
                continue;
            }
            out.write(SendChannelSound {
                client: player.client,
                sound: ChannelSound {
                    channel: sound.channel,
                    sound: Some(SoundEvent {
                        file: file.clone(),
                        volume,
                        speed,
                        offset,
                    }),
                    fx: SoundFx {
                        environment,
                        pitch: sound.pitch,
                        category: sound.category,
                    },
                    ..default()
                },
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.01
    }

    #[test]
    fn falloff_matches_tg() {
        // vol 50 (pistol), max 15: at the source full volume, at the edge min_volume.
        assert!(close(falloff_volume(50.0, 0.0, 15.0, 0.0, 2.5, 5.0), 50.0));
        assert!(close(falloff_volume(50.0, 15.0, 15.0, 0.0, 2.5, 5.0), 5.0));
        // 5 tiles: ratio (5/15)^(1/2.5) = 0.6444; 50 - 0.6444 * 45 = 21.0.
        assert!(close(falloff_volume(50.0, 5.0, 15.0, 0.0, 2.5, 5.0), 21.0));
        // Airlock deny: vol 50, extrarange 3 -> max 18; 9 tiles: 0.5^0.4 = 0.7579 -> 15.89.
        assert!(close(falloff_volume(50.0, 9.0, 18.0, 0.0, 2.5, 5.0), 15.89));
    }

    #[test]
    fn pressure_matches_tg() {
        assert_eq!(pressure_factor(Some(101.325), Some(200.0), 5.0), 1.0);
        // Half an atmosphere: (50.66 - 10) / 91.325 = 0.4452.
        assert!(close(
            pressure_factor(Some(50.6625), Some(101.325), 5.0),
            0.4452
        ));
        // Vacuum or no air at all: silent, unless touching the source.
        assert_eq!(pressure_factor(Some(0.0), Some(101.325), 5.0), 0.0);
        assert_eq!(pressure_factor(None, Some(101.325), 3.0), 0.0);
        assert_eq!(pressure_factor(Some(0.0), Some(0.0), 1.0), 0.15);
        assert_eq!(pressure_factor(Some(5.0), Some(101.325), 0.0), 0.15);
    }

    #[test]
    fn frequency_and_item_volumes() {
        assert!(close(speed_of(44100.0), 1.0));
        assert!(close(speed_of(0.5), 0.5));
        // Toolbox: force 12, w_class 4 -> 64; a forceless pen (w 1) -> 10.
        assert_eq!(clamped_volume(12.0, 4), 64.0);
        assert_eq!(clamped_volume(0.0, 1), 10.0);
        assert_eq!(clamped_volume(30.0, 5), 100.0);
    }

    #[test]
    fn walls_stop_sounds_that_do_not_carry_through_them() {
        // A wall along y = 2 from x = -5 to 5: the tile behind it is not heard, the tile beside
        // the source and the wall face itself are.
        let source = TilePos::new(0, 0, 1);
        let wall = |p: TilePos| p.y == 2 && (-5..=5).contains(&p.x);
        let seen = hearing_view(source, 15.0, wall, |_| true);
        assert!(seen.contains(&TilePos::new(3, 0, 1)));
        assert!(seen.contains(&TilePos::new(0, 2, 1)));
        assert!(!seen.contains(&TilePos::new(0, 3, 1)));
        assert!(!seen.contains(&TilePos::new(0, 16, 1)));
    }

    #[test]
    fn sounds_reach_the_levels_open_space_shows() {
        let mut levels = LevelStack::default();
        levels.link(1, 2);
        levels.link(2, 3);
        let source = TilePos::new(5, 5, 2);
        // Nothing transparent: only the source's level.
        assert_eq!(hearing_origins(source, Some(&levels), |_| false), [source]);
        // Open space above the source: heard upstairs.
        let above = TilePos::new(5, 5, 3);
        let up = hearing_origins(source, Some(&levels), |p| p == above);
        assert_eq!(up, [source, above]);
        // The source stands on glass: heard downstairs.
        let down = hearing_origins(source, Some(&levels), |p| p == source);
        assert_eq!(down, [source, TilePos::new(5, 5, 1)]);
        // No stack (unit-test worlds) or no level there: the source's level alone.
        assert_eq!(hearing_origins(source, None, |_| true), [source]);
        let top = TilePos::new(5, 5, 3);
        assert_eq!(
            hearing_origins(top, Some(&levels), |_| true),
            [top, TilePos::new(5, 5, 2)]
        );
    }

    #[test]
    fn another_level_rolls_off_only_near_the_edge() {
        // 5 tiles up and 3 across: 5.83 < 15, BYOND keeps it.
        assert_eq!(z_rolloff(3.0, 1, 15.0), 1.0);
        // 14 across: sqrt(196 + 25) = 14.87 < 15; 15 across: 15.81 -> 15 / 15.81.
        assert_eq!(z_rolloff(14.0, 1, 15.0), 1.0);
        assert!(close(z_rolloff(15.0, 1, 15.0), 0.9487));
        assert_eq!(z_rolloff(15.0, 0, 15.0), 1.0);
    }

    #[test]
    fn the_room_reverb_follows_the_mob_then_the_area() {
        let parking_lot = 20;
        assert_eq!(listener_environment(false, 0.0, parking_lot), parking_lot);
        assert_eq!(
            listener_environment(false, 0.0, SOUND_ENVIRONMENT_NONE),
            SOUND_ENVIRONMENT_NONE
        );
        assert_eq!(
            listener_environment(true, 0.0, parking_lot),
            SOUND_ENVIRONMENT_DRUGGED
        );
        // Tipsy (below 6) keeps the room; drunk turns it psychotic.
        assert_eq!(listener_environment(false, 5.9, parking_lot), parking_lot);
        assert_eq!(
            listener_environment(false, 6.0, parking_lot),
            SOUND_ENVIRONMENT_PSYCHOTIC
        );
    }

    #[test]
    fn channels_are_reserved_from_the_top_and_freed() {
        let mut channels = SoundChannels::default();
        let owner = Entity::PLACEHOLDER;
        assert_eq!(
            channels.reserve(Some(owner)),
            Some(CHANNEL_HIGHEST_AVAILABLE)
        );
        assert_eq!(channels.reserve(None), Some(CHANNEL_HIGHEST_AVAILABLE - 1));
        assert_eq!(channels.owner(CHANNEL_HIGHEST_AVAILABLE), Some(Some(owner)));
        // A freed channel is the next one handed out.
        assert!(channels.free(CHANNEL_HIGHEST_AVAILABLE));
        assert!(!channels.free(CHANNEL_HIGHEST_AVAILABLE));
        assert_eq!(
            channels.reserve(Some(owner)),
            Some(CHANNEL_HIGHEST_AVAILABLE)
        );
        channels.free_owner(owner);
        assert_eq!(channels.owner(CHANNEL_HIGHEST_AVAILABLE), None);
        channels.free_datumless();
        assert_eq!(channels.owner(CHANNEL_HIGHEST_AVAILABLE - 1), None);
        // The lowest `random_channels_min` are never reserved.
        let reservable = usize::from(CHANNEL_HIGHEST_AVAILABLE - RANDOM_CHANNELS_MIN);
        for _ in 0..reservable {
            assert!(
                channels
                    .reserve(None)
                    .is_some_and(|c| c > RANDOM_CHANNELS_MIN)
            );
        }
        assert_eq!(channels.reserve(None), None);
    }

    #[test]
    fn a_despawned_owner_frees_its_channels() {
        let mut app = App::new();
        app.init_resource::<SoundChannels>()
            .add_systems(Update, free_dead_owners);
        let owner = app.world_mut().spawn_empty().id();
        let channel = app
            .world_mut()
            .resource_mut::<SoundChannels>()
            .reserve(Some(owner))
            .unwrap();
        app.update();
        assert!(
            app.world()
                .resource::<SoundChannels>()
                .owner(channel)
                .is_some()
        );
        app.world_mut().despawn(owner);
        app.update();
        assert_eq!(app.world().resource::<SoundChannels>().owner(channel), None);
    }

    #[test]
    fn a_sound_scales_by_its_own_preference_or_its_channels() {
        use ss13_shared::protocol::{CHANNEL_JUKEBOX, CHANNEL_LOBBYMUSIC};
        let plain = SoundFx::default();
        assert_eq!(plain.category_on(0), SoundCategory::Game);
        assert_eq!(
            plain.category_on(CHANNEL_LOBBYMUSIC),
            SoundCategory::LobbyMusic
        );
        let own = SoundFx {
            category: SoundCategory::Instruments,
            ..plain
        };
        assert_eq!(own.category_on(CHANNEL_JUKEBOX), SoundCategory::Instruments);
        assert_eq!(SoundCategory::TtsRadio.default_percent(), 75);
    }

    #[test]
    fn sfx_picks_by_weight() {
        let mut data = SfxData::default();
        data.sfx
            .insert("k".into(), vec![("a.ogg".into(), 0), ("b.ogg".into(), 3)]);
        let sfx = Sfx(data);
        let mut rng = CombatRng(7);
        for _ in 0..20 {
            assert_eq!(pick_sfx(&sfx, &mut rng, "k"), "b.ogg");
        }
        assert_eq!(pick_sfx(&sfx, &mut rng, "x.ogg"), "x.ogg");
    }
}
