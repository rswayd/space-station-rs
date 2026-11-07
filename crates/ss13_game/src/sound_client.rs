//! Positional sound (client): plays each [`SoundEvent`] from the server with `bevy_audio` at
//! the server's volume and speed, panned by its offset from the listener.
//!
//! Panning uses Bevy's spatial audio (left/right stereo only) with a [`SpatialListener`] at the
//! origin and the sound at its tile offset, scaled down so rodio's own `1/d²` attenuation never
//! applies: the server already attenuated the volume as tg does.
//!
//! `SS13_VOLUME` (0-100, default 100) is the master volume.
//!
//! A channel sound ([`ChannelSoundReceived`]: the lobby music, jukeboxes, boomboxes) replaces
//! the one playing on its channel, or stops it; an update only changes its volume and offset.
//!
//! Every sound file (13 MB on MetaStation's assets) is read in the background at startup
//! ([`preload`]); a sound played before that finishes is read on the spot, as before (a cold
//! read took ~10 ms of the frame).
// shortcut: the master volume is an environment variable (like `SS13_LIGHTING`); move it to a
// settings window when the client has one.

use std::collections::HashMap;
use std::path::PathBuf;

use bevy::audio::{AudioPlayer, AudioSource, GlobalVolume, PlaybackSettings, SpatialListener};
use bevy::audio::{AudioSink, AudioSinkPlayback, SpatialAudioSink, SpatialScale, Volume};
use bevy::prelude::*;
use bevy::tasks::futures_lite::future::poll_once;
use bevy::tasks::{IoTaskPool, Task, block_on};
use ss13_net::sound::client::{ChannelSoundReceived, SoundReceived};

/// Tiles between the ears: a source this far to one side plays from that side only.
const EAR_GAP_TILES: f32 = 8.0;
/// Keeps every offset (at most ~30 tiles) under one unit, where rodio does not attenuate.
const SPATIAL_SCALE: f32 = 0.01;
/// `ss13_mapload::sound::SOUND_DIR` (the client does not link the converter).
const SOUND_DIR: &str = "sound";
/// A frame this long after the previous one ends a pause (a browser tab in the background gets
/// no animation frames while the server worker runs on): the sounds that queued up meanwhile are
/// stale, and playing them all at once is a burst of noise.
const STALE_GAP_SECS: f32 = 0.5;
/// Sounds stay dropped this long after a pause, real seconds: the client reads at most a few
/// hundred queued messages per frame, so a long backlog takes several frames to catch up.
const CATCH_UP_SECS: f64 = 1.0;

/// Real time until which received sounds are dropped (the catch-up after a pause).
#[derive(Resource, Default)]
struct MutedUntil(f64);

/// The player of each BYOND channel's sound.
#[derive(Resource, Default)]
struct ChannelPlayers(HashMap<u16, Entity>);

/// Whether a sound received at `now` (real seconds), in a frame `gap` seconds after the previous
/// one, is dropped; updates `muted_until`.
pub fn stale_sound(now: f64, gap: f32, muted_until: &mut f64) -> bool {
    if gap > STALE_GAP_SECS {
        *muted_until = now + CATCH_UP_SECS;
    }
    now < *muted_until
}

pub struct SoundClientPlugin {
    /// The generated assets directory (`SS13_ASSETS`); files are under its `sound/`.
    pub asset_root: PathBuf,
}

#[derive(Resource)]
struct SoundFiles {
    root: PathBuf,
    /// `None`: the file is missing or unreadable (logged once).
    loaded: HashMap<String, Option<Handle<AudioSource>>>,
}

/// The background read of every sound file: (path under the sound directory, bytes).
#[derive(Resource)]
struct Preloading(Task<Vec<(String, Vec<u8>)>>);

impl Plugin for SoundClientPlugin {
    fn build(&self, app: &mut App) {
        let master = std::env::var("SS13_VOLUME")
            .ok()
            .and_then(|v| v.parse::<f32>().ok())
            .map_or(1.0, |v| (v / 100.0).clamp(0.0, 1.0));
        app.insert_resource(GlobalVolume::new(Volume::Linear(master)))
            .init_resource::<MutedUntil>()
            .init_resource::<ChannelPlayers>()
            .insert_resource(SoundFiles {
                root: self.asset_root.join(SOUND_DIR),
                loaded: HashMap::new(),
            })
            .add_systems(Startup, (spawn_listener, start_preload))
            .add_systems(Update, (preload, play, play_channels).chain());
    }
}

/// Every file under `dir`, as paths relative to `root` with `/` separators.
fn sound_files(root: &std::path::Path, dir: &std::path::Path, out: &mut Vec<String>) {
    let Ok(entries) = ss13_data::read_dir(dir) else {
        return;
    };
    for path in entries {
        if ss13_data::is_dir(&path) {
            sound_files(root, &path, out);
        } else if let Ok(rel) = path.strip_prefix(root)
            && path
                .extension()
                .is_some_and(|e| matches!(e.to_str(), Some("ogg" | "wav" | "mp3" | "flac")))
        {
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
}

fn start_preload(files: Res<SoundFiles>, mut commands: Commands) {
    let root = files.root.clone();
    let task = IoTaskPool::get().spawn(async move {
        let mut names = Vec::new();
        sound_files(&root, &root, &mut names);
        names
            .into_iter()
            .filter_map(|name| Some((name.clone(), ss13_data::read(&root.join(&name)).ok()?)))
            .collect()
    });
    commands.insert_resource(Preloading(task));
}

/// Takes the preloaded files once they are read.
fn preload(
    task: Option<ResMut<Preloading>>,
    mut files: ResMut<SoundFiles>,
    mut sources: ResMut<Assets<AudioSource>>,
    mut commands: Commands,
) {
    let Some(mut task) = task else {
        return;
    };
    let Some(read) = block_on(poll_once(&mut task.0)) else {
        return;
    };
    commands.remove_resource::<Preloading>();
    let count = read.len();
    for (name, bytes) in read {
        files.loaded.entry(name).or_insert_with(|| {
            Some(sources.add(AudioSource {
                bytes: bytes.into(),
            }))
        });
    }
    info!("sound: {count} files preloaded");
}

fn spawn_listener(mut commands: Commands) {
    commands.spawn((
        Name::new("sound listener"),
        Transform::default(),
        SpatialListener::new(EAR_GAP_TILES),
    ));
}

fn play(
    mut received: MessageReader<SoundReceived>,
    time: Res<Time<Real>>,
    mut muted: ResMut<MutedUntil>,
    mut files: ResMut<SoundFiles>,
    mut sources: ResMut<Assets<AudioSource>>,
    mut commands: Commands,
) {
    if stale_sound(time.elapsed_secs_f64(), time.delta_secs(), &mut muted.0) {
        received.clear();
        return;
    }
    for SoundReceived(sound) in received.read() {
        let Some(handle) = sound_handle(&mut files, &mut sources, &sound.file) else {
            continue;
        };
        let [dx, dy] = sound.offset.unwrap_or([0, 0]);
        commands.spawn((
            AudioPlayer(handle),
            PlaybackSettings {
                volume: Volume::Linear((sound.volume / 100.0).clamp(0.0, 1.0)),
                speed: sound.speed.max(0.01),
                spatial: sound.offset.is_some(),
                spatial_scale: Some(SpatialScale(Vec3::splat(SPATIAL_SCALE))),
                ..PlaybackSettings::DESPAWN
            },
            Transform::from_xyz(f32::from(dx), f32::from(dy), 0.0),
        ));
    }
}

/// The loaded file, read on the spot if the preload has not got it; `None` for a missing file or
/// one outside the sound directory.
fn sound_handle(
    files: &mut SoundFiles,
    sources: &mut Assets<AudioSource>,
    file: &str,
) -> Option<Handle<AudioSource>> {
    // The server names files; never leave the sound directory.
    if file.contains("..") || file.starts_with('/') {
        return None;
    }
    let root = files.root.clone();
    files
        .loaded
        .entry(file.to_owned())
        .or_insert_with(|| match ss13_data::read(&root.join(file)) {
            Ok(bytes) => Some(sources.add(AudioSource {
                bytes: bytes.into(),
            })),
            Err(e) => {
                warn!("sound {file}: {e}");
                None
            }
        })
        .clone()
}

/// Channel sounds: the new one replaces the channel's player (not dropped after a pause: the
/// lobby music is sent once); an update (`SOUND_UPDATE`) only moves and re-levels it.
fn play_channels(
    mut received: MessageReader<ChannelSoundReceived>,
    mut channels: ResMut<ChannelPlayers>,
    mut files: ResMut<SoundFiles>,
    mut sources: ResMut<Assets<AudioSource>>,
    global: Res<GlobalVolume>,
    mut players: Query<(
        &mut PlaybackSettings,
        &mut Transform,
        Option<&mut AudioSink>,
        Option<&mut SpatialAudioSink>,
    )>,
    mut commands: Commands,
) {
    for ChannelSoundReceived(channel_sound) in received.read() {
        if channel_sound.update {
            let (Some(sound), Some(&player)) =
                (&channel_sound.sound, channels.0.get(&channel_sound.channel))
            else {
                continue;
            };
            let Ok((mut settings, mut transform, sink, spatial)) = players.get_mut(player) else {
                continue;
            };
            let volume = (sound.volume / 100.0).clamp(0.0, 1.0);
            settings.volume = Volume::Linear(volume);
            let [dx, dy] = sound.offset.unwrap_or([0, 0]);
            transform.translation = Vec3::new(f32::from(dx), f32::from(dy), 0.0);
            let level = Volume::Linear(volume * global.volume.to_linear());
            if let Some(mut sink) = sink {
                sink.set_volume(level);
            }
            if let Some(mut sink) = spatial {
                sink.set_volume(level);
            }
            continue;
        }
        if let Some(old) = channels.0.remove(&channel_sound.channel) {
            commands.entity(old).try_despawn();
        }
        let Some(sound) = &channel_sound.sound else {
            continue;
        };
        let Some(handle) = sound_handle(&mut files, &mut sources, &sound.file) else {
            continue;
        };
        let [dx, dy] = sound.offset.unwrap_or([0, 0]);
        let base = if channel_sound.repeat {
            PlaybackSettings::LOOP
        } else {
            PlaybackSettings::DESPAWN
        };
        let player = commands
            .spawn((
                AudioPlayer(handle),
                PlaybackSettings {
                    volume: Volume::Linear((sound.volume / 100.0).clamp(0.0, 1.0)),
                    speed: sound.speed.max(0.01),
                    spatial: sound.offset.is_some(),
                    spatial_scale: Some(SpatialScale(Vec3::splat(SPATIAL_SCALE))),
                    ..base
                },
                Transform::from_xyz(f32::from(dx), f32::from(dy), 0.0),
            ))
            .id();
        channels.0.insert(channel_sound.channel, player);
    }
}

#[cfg(test)]
mod tests {
    use bevy::audio::{AudioSource, Decodable};

    #[test]
    fn sounds_after_a_pause_are_dropped() {
        // 10 fps still plays; a backgrounded tab coming back does not replay its backlog.
        let mut muted = 0.0;
        assert!(!super::stale_sound(10.0, 0.1, &mut muted));
        assert!(super::stale_sound(20.0, 3.0, &mut muted));
        // The backlog still arriving in the next ordinary frames is dropped too, then sound is back.
        assert!(super::stale_sound(20.5, 0.016, &mut muted));
        assert!(!super::stale_sound(21.1, 0.016, &mut muted));
    }

    /// tg's `.ogg` files decode with the client's Bevy features (`audio` = `vorbis`); skipped
    /// without the BYOND checkout.
    #[test]
    fn tg_ogg_decodes() {
        let source = std::env::var("SS13_SOURCE").unwrap_or(env!("SS13_SOURCE").into());
        let path = std::path::Path::new(&source).join("sound/items/weapons/gun/pistol/shot.ogg");
        let Ok(bytes) = std::fs::read(&path) else {
            return;
        };
        let samples = AudioSource {
            bytes: bytes.into(),
        }
        .decoder()
        .take(1000)
        .count();
        assert_eq!(samples, 1000);
    }
}
