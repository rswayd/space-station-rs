//! Positional sound (client): plays each [`SoundEvent`] from the server with `bevy_audio` at
//! the server's volume and speed, panned by its offset from the listener.
//!
//! Panning uses Bevy's spatial audio (left/right stereo only) with a [`SpatialListener`] at the
//! origin and the sound at its tile offset, scaled down so rodio's own `1/d²` attenuation never
//! applies: the server already attenuated the volume as tg does.
//!
//! `SS13_VOLUME` (0-100, default 100) is the master volume.
//!
//! Every sound file (13 MB on MetaStation's assets) is read in the background at startup
//! ([`preload`]); a sound played before that finishes is read on the spot, as before (a cold
//! read took ~10 ms of the frame).
// shortcut: the master volume is an environment variable (like `SS13_LIGHTING`); move it to a
// settings window when the client has one.

use std::collections::HashMap;
use std::path::PathBuf;

use bevy::audio::{AudioPlayer, AudioSource, GlobalVolume, PlaybackSettings, SpatialListener};
use bevy::audio::{SpatialScale, Volume};
use bevy::prelude::*;
use bevy::tasks::futures_lite::future::poll_once;
use bevy::tasks::{IoTaskPool, Task, block_on};
use ss13_net::sound::client::SoundReceived;

/// Tiles between the ears: a source this far to one side plays from that side only.
const EAR_GAP_TILES: f32 = 8.0;
/// Keeps every offset (at most ~30 tiles) under one unit, where rodio does not attenuate.
const SPATIAL_SCALE: f32 = 0.01;
/// `ss13_mapload::sound::SOUND_DIR` (the client does not link the converter).
const SOUND_DIR: &str = "sound";

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
            .insert_resource(SoundFiles {
                root: self.asset_root.join(SOUND_DIR),
                loaded: HashMap::new(),
            })
            .add_systems(Startup, (spawn_listener, start_preload))
            .add_systems(Update, (preload, play).chain());
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
    mut files: ResMut<SoundFiles>,
    mut sources: ResMut<Assets<AudioSource>>,
    mut commands: Commands,
) {
    for SoundReceived(sound) in received.read() {
        // The server names files; never leave the sound directory.
        if sound.file.contains("..") || sound.file.starts_with('/') {
            continue;
        }
        let root = files.root.clone();
        let handle = files
            .loaded
            .entry(sound.file.clone())
            .or_insert_with(|| match ss13_data::read(&root.join(&sound.file)) {
                Ok(bytes) => Some(sources.add(AudioSource {
                    bytes: bytes.into(),
                })),
                Err(e) => {
                    warn!("sound {}: {e}", sound.file);
                    None
                }
            })
            .clone();
        let Some(handle) = handle else {
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

#[cfg(test)]
mod tests {
    use bevy::audio::{AudioSource, Decodable};

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
