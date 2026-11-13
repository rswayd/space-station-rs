//! A track on `CHANNEL_JUKEBOX` that follows its listeners: tg `/datum/jukebox` (the jukebox)
//! and the boombox's `/datum/sound_token` loop (`use_sound_tokens`, native repeat).
//!
//! While a [`MusicPlayer`] plays, every player mob that comes within reach is registered once:
//! it gets the track (silent when muted, BYOND `SOUND_MUTE`), and after that only updates of
//! volume and offset (`SOUND_UPDATE`) when they change; stopping sends every listener the
//! channel's stop.
//!
//! - Jukebox reach: registered within 10 tiles on each axis (`x_cutoff`/`z_cutoff` of a 15x15
//!   view × 1.25), muted beyond them, on another z or deaf; BYOND's own distance rolloff
//!   (`falloff = 2`) is applied here as FMOD's inverse rolloff, `2 / distance`.
//! - Token reach (boombox): `SOUND_RANGE + extra_range` tiles, tg's falloff curve with the
//!   loop's `falloff_exponent`.
// shortcut: no pressure or area environment, no jukebox volume preference; the jukebox registers
// everyone within the cutoff (tg starts with `hearers(view)` and adds mobs that walk in).
// Cost: one pass over the player mobs per playing source per tick (a few hundred distance checks
// for the handful of jukeboxes and boomboxes playing at once).

use std::collections::HashMap;

use bevy::prelude::*;
use ss13_net::sound::server::SendChannelSound;
use ss13_shared::TilePos;
use ss13_shared::protocol::{CHANNEL_JUKEBOX, ChannelSound, SoundEvent};

use crate::modpc::pda::turf_of;
use crate::server::PlayerMob;

/// The jukebox's cutoff: `ceil(15 * 1.25 / 2)`.
pub const JUKEBOX_CUTOFF: i32 = 10;
/// The jukebox sound's `falloff`.
const JUKEBOX_FALLOFF: f32 = 2.0;

/// How a track reaches its listeners.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Reach {
    Jukebox,
    /// A sound token: `max_distance` tiles and the falloff exponent.
    Token {
        max_distance: f32,
        exponent: f32,
    },
}

/// What one listener hears now.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Heard {
    client: Entity,
    volume: f32,
    offset: [i16; 2],
}

#[derive(Clone, Debug, PartialEq)]
pub struct Playing {
    pub file: String,
    pub volume: f32,
    pub repeat: bool,
    pub reach: Reach,
    listeners: HashMap<Entity, Heard>,
}

/// A source of channel music; `None` while silent.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct MusicPlayer(pub Option<Playing>);

impl MusicPlayer {
    pub fn playing(&self) -> bool {
        self.0.is_some()
    }

    /// The mobs registered as listeners.
    pub fn listeners(&self) -> Vec<Entity> {
        self.0
            .as_ref()
            .map(|p| p.listeners.keys().copied().collect())
            .unwrap_or_default()
    }
}

/// Whether a listener at `at` is registered by a source at `source` (same z).
fn in_reach(reach: Reach, source: TilePos, at: TilePos) -> bool {
    if source.z != at.z {
        return false;
    }
    let (dx, dy) = ((at.x - source.x).abs(), (at.y - source.y).abs());
    match reach {
        Reach::Jukebox => dx <= JUKEBOX_CUTOFF && dy <= JUKEBOX_CUTOFF,
        Reach::Token { max_distance, .. } => ((dx * dx + dy * dy) as f32).sqrt() <= max_distance,
    }
}

/// The volume a listener at `at` hears (0: muted).
pub fn heard_volume(reach: Reach, volume: f32, source: TilePos, at: TilePos, deaf: bool) -> f32 {
    if deaf || !in_reach(reach, source, at) {
        return 0.0;
    }
    let (dx, dy) = (at.x - source.x, at.y - source.y);
    let distance = ((dx * dx + dy * dy) as f32).sqrt();
    match reach {
        Reach::Jukebox => volume * (JUKEBOX_FALLOFF / distance.max(JUKEBOX_FALLOFF)),
        Reach::Token {
            max_distance,
            exponent,
        } => crate::sound::falloff_volume(
            volume,
            distance,
            max_distance,
            0.0,
            exponent,
            crate::sound::MIN_VOLUME,
        ),
    }
}

fn send(world: &mut World, client: Entity, sound: Option<SoundEvent>, update: bool, repeat: bool) {
    world.write_message(SendChannelSound {
        client,
        sound: ChannelSound {
            channel: CHANNEL_JUKEBOX,
            sound,
            update,
            repeat,
            ..default()
        },
    });
}

/// Starts `file` on `source` (replacing what it played).
pub fn start(
    world: &mut World,
    source: Entity,
    file: &str,
    volume: f32,
    repeat: bool,
    reach: Reach,
) {
    stop(world, source);
    let playing = Playing {
        file: file.to_owned(),
        volume,
        repeat,
        reach,
        listeners: HashMap::new(),
    };
    world.entity_mut(source).insert(MusicPlayer(Some(playing)));
    follow(world, source);
}

/// `unlisten_all`: every listener's channel stops.
pub fn stop(world: &mut World, source: Entity) {
    let Some(playing) = world
        .get_mut::<MusicPlayer>(source)
        .and_then(|mut m| m.0.take())
    else {
        return;
    };
    for heard in playing.listeners.values() {
        send(world, heard.client, None, false, false);
    }
}

/// `set_new_volume` while playing: everyone is updated.
pub fn set_volume(world: &mut World, source: Entity, volume: f32) {
    if let Some(mut m) = world.get_mut::<MusicPlayer>(source)
        && let Some(p) = m.0.as_mut()
    {
        p.volume = volume;
    }
    follow(world, source);
}

/// Registers new listeners and updates the ones whose volume or offset changed.
fn follow(world: &mut World, source: Entity) {
    let Some(playing) = world.get::<MusicPlayer>(source).and_then(|m| m.0.clone()) else {
        return;
    };
    let Some(at) = turf_of(world, source) else {
        return;
    };
    let mobs: Vec<(Entity, Entity)> = world
        .query::<(Entity, &PlayerMob)>()
        .iter(world)
        .map(|(e, p)| (e, p.client))
        .collect();
    let mut listeners = playing.listeners.clone();
    listeners.retain(|mob, _| world.get_entity(*mob).is_ok());
    for (mob, client) in mobs {
        let Some(pos) = turf_of(world, mob) else {
            continue;
        };
        let known = listeners.get(&mob).copied();
        if known.is_none() && !in_reach(playing.reach, at, pos) {
            continue;
        }
        let deaf = !crate::sound::can_hear(world, mob);
        let volume = heard_volume(playing.reach, playing.volume, at, pos, deaf);
        let offset = [
            (at.x - pos.x).clamp(-999, 999) as i16,
            (at.y - pos.y).clamp(-999, 999) as i16,
        ];
        let heard = Heard {
            client,
            volume,
            offset,
        };
        // A new body for the client (`listener_login`): the track starts again on it.
        let restart = known.is_some_and(|k| k.client != client);
        if known == Some(heard) {
            continue;
        }
        let event = SoundEvent {
            file: playing.file.clone(),
            volume,
            speed: 1.0,
            offset: Some(offset),
        };
        let update = known.is_some() && !restart;
        send(world, client, Some(event), update, playing.repeat);
        listeners.insert(mob, heard);
    }
    if let Some(mut m) = world.get_mut::<MusicPlayer>(source)
        && let Some(p) = m.0.as_mut()
    {
        p.listeners = listeners;
    }
}

/// Every tick: each playing source follows its listeners.
pub(super) fn follow_listeners(world: &mut World) {
    let sources: Vec<Entity> = world
        .query::<(Entity, &MusicPlayer)>()
        .iter(world)
        .filter(|(_, m)| m.playing())
        .map(|(e, _)| e)
        .collect();
    for source in sources {
        follow(world, source);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reach_and_volume() {
        let at = TilePos::new(0, 0, 1);
        // Jukebox: full volume within 2 tiles, 2/d beyond, muted past 10 on an axis.
        assert_eq!(
            heard_volume(Reach::Jukebox, 50.0, at, TilePos::new(1, 1, 1), false),
            50.0
        );
        assert_eq!(
            heard_volume(Reach::Jukebox, 50.0, at, TilePos::new(4, 0, 1), false),
            25.0
        );
        assert_eq!(
            heard_volume(Reach::Jukebox, 50.0, at, TilePos::new(11, 0, 1), false),
            0.0
        );
        assert_eq!(
            heard_volume(Reach::Jukebox, 50.0, at, TilePos::new(1, 0, 2), false),
            0.0
        );
        assert_eq!(
            heard_volume(Reach::Jukebox, 50.0, at, TilePos::new(1, 0, 1), true),
            0.0
        );
        // Token: 12 tiles, exponent 1 (linear): 6 tiles -> halfway between 50 and 5.
        let token = Reach::Token {
            max_distance: 12.0,
            exponent: 1.0,
        };
        assert!((heard_volume(token, 50.0, at, TilePos::new(6, 0, 1), false) - 27.5).abs() < 0.01);
        assert_eq!(
            heard_volume(token, 50.0, at, TilePos::new(13, 0, 1), false),
            0.0
        );
    }
}
