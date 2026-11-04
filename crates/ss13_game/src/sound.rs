//! Positional sound (server): tg `playsound` / `playsound_local` / `sound_to_playing_players`
//! (`code/game/sound/sound.dm`) and `get_sfx` (`sound_keys/`).
//!
//! A system writes a [`PlaySound`]; at the end of the tick each player mob within
//! `SOUND_RANGE + extrarange` tiles (euclidean, same z) that is not deaf (and, for
//! `ignore_walls = FALSE`, is a ghost or stands on a tile BYOND `view()` sees from the source:
//! tg `get_hearers_in_view`) gets a
//! [`SoundEvent`] with its own volume: distance falloff (`CALCULATE_SOUND_VOLUME_RATIO`), times
//! the pressure factor of the thinner of the two tiles' air (space 0, at least 0.15 next to the
//! source), and its offset for stereo panning. `vary` picks `rand(32000, 55000)` Hz once per
//! sound, as tg. SFX keys pick one of their files (weighted) once per sound.
//!
//! Cost: one pass over the player mobs per sound (players × sounds per tick, a distance check
//! each) — a few hundred mobs × a few sounds per tick; a sound that does not carry through walls
//! adds one `view()` of up to 31×31 tiles (an opacity lookup each), and only suppressed or
//! otherwise rare sounds ask for it.
// shortcut: no sounds from the z-levels above/below through open space (tg `GET_TURF_ABOVE`,
// `istransparentturf`): z-levels here are not stacked; add with multi-z. No reverb/environment
// and no `volume_preference`.

use std::collections::HashSet;

use bevy::prelude::*;
use ss13_mapload::sound::{SFX_FILE, SfxData};
use ss13_mapload::{DmItem, MapLoadPlugin, Opaque};
use ss13_net::server::NetServerSystems;
use ss13_net::sound::server::SendSound;
use ss13_shared::TilePos;
use ss13_shared::protocol::SoundEvent;
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
    /// BYOND `sound.pitch` of a sound datum (tg energy guns), a multiplier.
    // shortcut: applied as playback speed, so it also changes the length (BYOND shifts the
    // pitch only); upgrade when the client gets a pitch-shift filter.
    pub pitch: f32,
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
            .add_systems(PreStartup, load_sfx)
            .add_systems(
                PostUpdate,
                (
                    crate::footsteps::play_footsteps.before(SoundSystems),
                    deliver.in_set(SoundSystems).before(NetServerSystems::Send),
                ),
            );
    }
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

/// tg `TRAIT_DEAF` sources ported: the deafness mutation and earmuffs (the ears' own in
/// `crate::medical::organs::ear_deaf`).
// shortcut: no deafness quirk or disease.
fn is_deaf(dna: Option<&Dna>, inventory: Option<&Inventory>, items: &Query<&DmItem>) -> bool {
    dna.is_some_and(|d| mutation_id(DEAF_MUTATION).is_some_and(|id| d.get_mutation(id).is_some()))
        || inventory
            .and_then(|i| i.ears)
            .and_then(|e| items.get(e).ok())
            .is_some_and(|i| ss13_mapload::fidelity::is_under(&i.path, EARMUFFS))
}

fn pick_sfx(sfx: &Sfx, rng: &mut CombatRng, sound: &str) -> String {
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

#[allow(clippy::too_many_arguments)]
fn deliver(
    mut sounds: MessageReader<PlaySound>,
    listeners: Query<Listener>,
    locs: Query<(Option<&TilePos>, Option<&CarriedBy>, Option<&StoredIn>)>,
    items: Query<&DmItem>,
    organs: Query<&crate::medical::organs::Organ>,
    sfx: Res<Sfx>,
    mut rng: ResMut<SoundRng>,
    atmos: Option<Res<ss13_atmos::Atmos>>,
    (index, opaque): (Res<SpatialIndex>, Query<(), With<Opaque>>),
    mut out: MessageWriter<SendSound>,
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
        } * sound.pitch;
        let max_distance = SOUND_RANGE + sound.extrarange;
        let view = match sound.source {
            Some(source) if !sound.ignore_walls => Some(hearing_view(
                source,
                max_distance,
                |p| index.on_tile(p).iter().any(|&e| opaque.contains(e)),
                |p| index.in_bounds(p),
            )),
            _ => None,
        };
        for (player, mob, dna, inventory, ghost, (owned, host)) in &listeners {
            if is_deaf(dna, inventory, &items)
                || crate::medical::organs::ear_deaf(owned, host, |e| organs.get(e).ok())
            {
                continue;
            }
            let Some(at) = turf(mob) else {
                continue;
            };
            let (volume, offset) = match sound.source {
                None => (sound.vol, None),
                Some(source) => {
                    if source.z != at.z {
                        continue;
                    }
                    let (dx, dy) = (source.x - at.x, source.y - at.y);
                    let distance = ((dx * dx + dy * dy) as f32).sqrt();
                    // Ghosts on the z-level hear it anyway (`dead_players_by_zlevel`).
                    let unseen = view.as_ref().is_some_and(|v| !ghost && !v.contains(&at));
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
                    (volume, Some([dx as i16, dy as i16]))
                }
            };
            // A silent sound is not sent: the client would play nothing.
            if volume <= 0.0 {
                continue;
            }
            out.write(SendSound {
                client: player.client,
                sound: SoundEvent {
                    file: file.clone(),
                    volume,
                    speed,
                    offset,
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
