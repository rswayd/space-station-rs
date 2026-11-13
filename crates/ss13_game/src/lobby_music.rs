//! Lobby and round end music (server), after tg `SSticker` (`Initialize`'s title music,
//! `set_lobby_music`, `choose_round_end_song`, `Shutdown`), `/client/proc/playtitlemusic` and the
//! April Fools' `celebrate()`.
//!
//! - The lobby music ([`login_music`]) is picked once per round from
//!   `strings/round_start_sounds.txt` ([`ROUND_START_MUSIC`]), the clown's song on April Fools'.
//!   A client gets it on the lobby channel (`CHANNEL_LOBBYMUSIC`) when it enters the lobby, and
//!   it stops when it joins or observes (`stop_sound_channel`).
//! - At round end every client hears it again at half volume (`declare_completion`'s
//!   `playtitlemusic(volume_multiplier = 0.5)`).
//! - Right before the reboot every client gets a random round end song from
//!   `config/reboot_themes/` ([`REBOOT_THEMES`]).
// shortcut: no deployment title music (`config/title_music/sounds/`, rare and map-specific
// tracks) and the last round's track is not excluded (`data/last_round_lobby_music.txt`); the
// volumes sent are the preferences' defaults (100: `sound_lobby_volume`, `sound_midi`): the
// player's lobby music volume applies on the client (`crate::sound_client`, by the channel); the
// round end song is not scaled by the admin music volume (it plays on no channel).
// Deviation: `title1.mod` is left out of the list: the client's audio decodes no tracker
// modules (BYOND does).

use bevy::prelude::*;
use ss13_net::server::ClientToken;
use ss13_net::sound::server::{SendChannelSound, SendSound};
use ss13_shared::protocol::{CHANNEL_LOBBYMUSIC, ChannelSound, SoundEvent};

use crate::round::NewPlayer;

/// `strings/round_start_sounds.txt` without `title1.mod` (a tracker module) and without
/// `title2`/`title3`: remixes of copyrighted themes (Robocop, Tintin) with no licence we can
/// redistribute under (sound/music/lobby_music/license.txt).
pub const ROUND_START_MUSIC: [&str; 2] = ["sound/music/lobby_music/title0.ogg", CLOWN_MUSIC];
/// April Fools' `celebrate()`: `set_lobby_music(clown.ogg, override = TRUE)`.
pub const CLOWN_MUSIC: &str = "sound/music/lobby_music/clown.ogg";
/// `config/reboot_themes/`: `choose_round_end_song` picks one.
pub const REBOOT_THEMES: [&str; 11] = [
    "config/reboot_themes/anyoneheard.ogg",
    "config/reboot_themes/apcdestroyed.ogg",
    "config/reboot_themes/bangindonk.ogg",
    "config/reboot_themes/bully2.ogg",
    "config/reboot_themes/disappointed.ogg",
    "config/reboot_themes/its_only_game.ogg",
    "config/reboot_themes/leavingtg.ogg",
    "config/reboot_themes/newroundsexy.ogg",
    "config/reboot_themes/petersondisappointed.ogg",
    "config/reboot_themes/scrunglartiy.ogg",
    "config/reboot_themes/yeehaw.ogg",
];
/// The volume preferences' default (`/datum/preference/numeric/volume`: `maximum`).
pub const DEFAULT_VOLUME: f32 = 100.0;

/// tg `SSticker.login_music`: picked on first use.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct LobbyMusic(pub Option<String>);

pub(crate) fn build(app: &mut App) {
    app.init_resource::<LobbyMusic>()
        .add_observer(lobby_entered)
        .add_observer(lobby_left);
}

/// tg `SSticker.login_music`: the round's lobby track.
pub fn login_music(world: &mut World) -> String {
    if let Some(music) = world.get_resource::<LobbyMusic>().and_then(|m| m.0.clone()) {
        return music;
    }
    let music =
        if crate::events::holidays::check_holidays(world, crate::events::holidays::APRIL_FOOLS) {
            CLOWN_MUSIC.to_owned()
        } else {
            pick(world, &ROUND_START_MUSIC).to_owned()
        };
    world.insert_resource(LobbyMusic(Some(music.clone())));
    music
}

fn pick<'a>(world: &mut World, from: &[&'a str]) -> &'a str {
    let i = world
        .get_resource_mut::<crate::sound::SoundRng>()
        .map_or(0, |mut rng| rng.0.range(0, from.len() as u32 - 1) as usize);
    from[i]
}

fn event(file: String, volume: f32) -> SoundEvent {
    SoundEvent {
        file,
        volume,
        speed: 1.0,
        offset: None,
    }
}

/// `/client/proc/playtitlemusic(volume_multiplier)` for one client.
pub fn play_title_music(world: &mut World, client: Entity, volume_multiplier: f32) {
    let music = login_music(world);
    world.write_message(SendChannelSound {
        client,
        sound: ChannelSound {
            channel: CHANNEL_LOBBYMUSIC,
            sound: Some(event(music, DEFAULT_VOLUME * volume_multiplier)),
            ..default()
        },
    });
}

/// `new_player/Login`: a client in the lobby (connected, or back from a body) hears the music.
fn lobby_entered(add: On<Add<NewPlayer>>, mut commands: Commands) {
    let client = add.entity;
    commands.queue(move |world: &mut World| play_title_music(world, client, 1.0));
}

/// `stop_sound_channel(CHANNEL_LOBBYMUSIC)` when the new player joins or observes.
fn lobby_left(remove: On<Remove<NewPlayer>>, mut out: MessageWriter<SendChannelSound>) {
    out.write(SendChannelSound {
        client: remove.entity,
        sound: ChannelSound {
            channel: CHANNEL_LOBBYMUSIC,
            sound: None,
            ..default()
        },
    });
}

fn clients(world: &mut World) -> Vec<Entity> {
    world
        .query_filtered::<Entity, With<ClientToken>>()
        .iter(world)
        .collect()
}

/// `declare_completion`: `playtitlemusic(volume_multiplier = 0.5)` for every client.
pub fn round_end_title_music(world: &mut World) {
    for client in clients(world) {
        play_title_music(world, client, 0.5);
    }
}

/// `Shutdown`: `choose_round_end_song` to every client. Returns the song.
pub fn play_round_end_song(world: &mut World) -> String {
    let song = pick(world, &REBOOT_THEMES).to_owned();
    for client in clients(world) {
        world.write_message(SendSound {
            client,
            sound: event(song.clone(), DEFAULT_VOLUME),
        });
    }
    song
}

#[cfg(test)]
mod tests {
    use bevy::ecs::message::Messages;

    use super::*;

    fn world() -> World {
        let mut world = World::new();
        world.init_resource::<Messages<SendChannelSound>>();
        world.init_resource::<Messages<SendSound>>();
        world.init_resource::<LobbyMusic>();
        world.add_observer(lobby_entered);
        world.add_observer(lobby_left);
        world
    }

    fn channel_sounds(world: &mut World) -> Vec<(Entity, Option<String>)> {
        world
            .resource_mut::<Messages<SendChannelSound>>()
            .drain()
            .map(|m| {
                assert_eq!(m.sound.channel, CHANNEL_LOBBYMUSIC);
                (m.client, m.sound.sound.map(|s| s.file))
            })
            .collect()
    }

    #[test]
    fn the_lobby_plays_one_track_and_stops_it_on_leaving() {
        let mut world = world();
        let client = world.spawn(NewPlayer::default()).id();
        world.flush();
        let music = world.resource::<LobbyMusic>().0.clone().expect("picked");
        assert!(ROUND_START_MUSIC.contains(&music.as_str()));
        assert_eq!(
            channel_sounds(&mut world),
            vec![(client, Some(music.clone()))]
        );
        // Joining the round: the music stops.
        world.entity_mut(client).remove::<NewPlayer>();
        assert_eq!(channel_sounds(&mut world), vec![(client, None)]);
        // Back in the lobby: the same track.
        world.entity_mut(client).insert(NewPlayer::default());
        world.flush();
        assert_eq!(channel_sounds(&mut world), vec![(client, Some(music))]);
    }

    #[test]
    fn the_round_end_song_is_a_reboot_theme_for_every_client() {
        let mut world = world();
        let client = world.spawn(ClientToken("bob".into())).id();
        let song = play_round_end_song(&mut world);
        assert!(REBOOT_THEMES.contains(&song.as_str()));
        let sent: Vec<_> = world
            .resource_mut::<Messages<SendSound>>()
            .drain()
            .collect();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].client, client);
        assert_eq!(sent[0].sound.file, song);
    }
}
