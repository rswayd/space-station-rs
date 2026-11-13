//! Area ambience (server), after tg `SSambience` (`code/controllers/subsystem/ambience.dm`):
//!
//! - Once a second ([`AMBIENCE_WAIT_SECS`]) each player mob that can hear (ghosts too, as tg:
//!   only the lobby and `TRAIT_DEAF` are skipped) whose cooldown is over gets a random sound of
//!   its area's `ambientsounds` on `CHANNEL_AMBIENCE` ([`play_ambience`]); the next one comes
//!   after the sound's length plus `rand(min_ambience_cooldown, max_ambience_cooldown)`. A
//!   `forced_ambience` area (maintenance) gives a newcomer one early, 5% a second. The first one
//!   comes 10 s after the player gets a mob (`update_ambience_pref`). In maintenance, a mob in the
//!   dark sometimes hears `minecraft_cave_noises` instead.
//! - The area's `ambient_buzz` loops on `CHANNEL_BUZZ` (tg `refresh_looping_ambience`): it
//!   changes when the mob moves into an area with another buzz and stops in an area without one,
//!   for a deaf mob, or where the APC is missing, off, empty or has environment power off (not on
//!   the mining level). Ghosts keep whatever they had (tg does not refresh observers).
//!
//! The client plays both channels as any channel sound (`crate::sound_client`).
//!
//! Cost: one pass over the player mobs per second (an area lookup and a few component reads
//! each), plus the mobs that moved this tick for the buzz; a sound file's length is read from
//! disk once.
// The server sends tg's base volumes; the player's `sound_ambience_volume` and
// `sound_ship_ambience_volume` are the client's category sliders (`SoundCategory::of_channel`
// maps `CHANNEL_AMBIENCE` and `CHANNEL_BUZZ`).
// shortcut: the deafness checks are polled each second instead of tg's `TRAIT_DEAF` signals.

use std::collections::HashMap;

use bevy::ecs::system::SystemState;
use bevy::prelude::*;
use ss13_mapload::{Areas, DmArea, MapLoadPlugin};
use ss13_net::sound::server::SendChannelSound;
use ss13_shared::TilePos;
use ss13_shared::protocol::{ChannelSound, SoundEvent};

use crate::combat::CombatRng;
use crate::ghost::Ghost;
use crate::health::Stat;
use crate::lobby_music::DEFAULT_VOLUME;
use crate::power::{Apc, AreaPower, Channel};
use crate::server::PlayerMob;
use crate::sound::SoundRng;

pub use ss13_shared::protocol::{CHANNEL_AMBIENCE, CHANNEL_BUZZ};
/// `SSambience.wait`.
pub const AMBIENCE_WAIT_SECS: f64 = 1.0;
/// `play_ambience`'s `volume`.
pub const AMBIENCE_VOLUME: f32 = 27.0;
/// `update_ambience_pref`: the first sound after login.
pub const FIRST_AMBIENCE_SECS: f64 = 10.0;
/// `play_ambience` of an area without sounds: tried again in a minute.
pub const NO_AMBIENCE_SECS: f64 = 60.0;
/// `fire`: a `forced_ambience` area's chance a second for a newcomer.
pub const FORCED_AMBIENCE_PROB: f32 = 5.0;
/// `/area/station/maintenance/play_ambience`: the chance of a cave noise in the dark.
pub const CAVE_NOISE_PROB: f32 = 0.5;
/// `LIGHTING_TILE_IS_DARK`.
const LIGHTING_TILE_IS_DARK: f32 = 0.2;
const MAINTENANCE: &str = "/area/station/maintenance";

/// `SSambience` lists and `client.current_ambient_sound`, by client entity.
#[derive(Resource, Debug, Default)]
pub struct Ambience {
    /// `ambience_listening_clients`: when the next sound may play (elapsed seconds).
    pub next: HashMap<Entity, f64>,
    /// `client_old_areas`: the area of the last sound.
    pub old_areas: HashMap<Entity, Entity>,
    /// `current_ambient_sound`: the buzz loop playing.
    pub buzz: HashMap<Entity, String>,
}

/// `SSsounds.get_sound_length`: seconds by file, read once (0 for a missing file).
#[derive(Resource, Debug, Default)]
pub struct SoundLengths(pub HashMap<String, f64>);

pub struct AmbiencePlugin;

impl Plugin for AmbiencePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Ambience>()
            .init_resource::<SoundLengths>()
            .init_resource::<SoundRng>()
            .add_systems(Update, (fire, refresh_moved));
    }
}

/// When the next sound may play after one of `length` seconds: `sound_length +
/// rand(min_ambience_cooldown, max_ambience_cooldown)`, rolled in deciseconds as tg.
pub fn next_cooldown(length: f64, area: &DmArea, rng: &mut CombatRng) -> f64 {
    let low = (area.min_ambience_cooldown * 10.0).max(0.0) as u32;
    let high = ((area.max_ambience_cooldown * 10.0).max(0.0) as u32).max(low);
    length + f64::from(rng.range(low, high)) / 10.0
}

/// `pick(ambientsounds)`, or `override_sound`.
pub fn pick_sound<'a>(
    area: &'a DmArea,
    override_sound: Option<&'a str>,
    rng: &mut CombatRng,
) -> Option<&'a str> {
    override_sound.or_else(|| {
        let n = area.ambientsounds.len() as u32;
        (n > 0).then(|| area.ambientsounds[rng.range(0, n - 1) as usize].as_str())
    })
}

/// `SSambience.fire`: whether the client's cooldown lets a sound play now (`forced` is the
/// newcomer roll of a `forced_ambience` area, made only when it matters).
pub fn ambience_due(
    next: f64,
    now: f64,
    forced_ambience: bool,
    new_area: bool,
    roll: impl FnOnce() -> bool,
) -> bool {
    next <= now || (forced_ambience && new_area && roll())
}

fn area_of(world: &World, mob: Entity) -> Option<(TilePos, Entity)> {
    let pos = crate::explosions::turf_of(world, mob)?;
    Some((pos, world.get_resource::<Areas>()?.area_at(pos)?))
}

/// `TRAIT_DEAF`: the ported sources plus hard crit and death (`set_stat`).
fn deaf(world: &World, mob: Entity) -> bool {
    !crate::sound::can_hear(world, mob)
        || world.get::<Stat>(mob).is_some_and(|s| *s >= Stat::HardCrit)
}

fn sound_length(world: &mut World, file: &str) -> f64 {
    if let Some(&secs) = world.resource::<SoundLengths>().0.get(file) {
        return secs;
    }
    let secs = world
        .get_resource::<MapLoadPlugin>()
        .and_then(|m| std::fs::read(m.assets.join(ss13_mapload::sound::SOUND_DIR).join(file)).ok())
        .and_then(|b| ss13_mapload::sound::ogg_duration(&b))
        .unwrap_or_else(|| {
            // tg: sounds then cut into each other.
            warn!("ambience {file}: no sound length");
            0.0
        });
    world
        .resource_mut::<SoundLengths>()
        .0
        .insert(file.to_owned(), secs);
    secs
}

fn send(world: &mut World, client: Entity, channel: u16, sound: Option<SoundEvent>, repeat: bool) {
    world.write_message(SendChannelSound {
        client,
        sound: ChannelSound {
            channel,
            sound,
            repeat,
            ..default()
        },
    });
}

/// `/area/proc/play_ambience`: plays a sound of `area` to the client, returns the cooldown in
/// seconds.
pub fn play_ambience(world: &mut World, client: Entity, pos: TilePos, area: Entity) -> f64 {
    let dm = world.get::<DmArea>(area).cloned().unwrap_or_default();
    let maintenance = crate::events::area_type(world, area)
        .is_some_and(|p| ss13_mapload::fidelity::is_under(p, MAINTENANCE));
    let mut rng = std::mem::take(&mut world.resource_mut::<SoundRng>().0);
    // `!M.has_light_nearby() && prob(0.5)`, the cheap roll first.
    let cave = maintenance
        && rng.chance(CAVE_NOISE_PROB)
        && !crate::antag::nightmare::is_lit(
            crate::botany::lumcount(world, pos),
            LIGHTING_TILE_IS_DARK,
        );
    let noises = &ss13_mapload::ambience::MAINT_CAVE_NOISES;
    let override_sound = cave.then(|| noises[rng.range(0, noises.len() as u32 - 1) as usize]);
    let file = pick_sound(&dm, override_sound, &mut rng).map(str::to_owned);
    let cooldown = match file {
        None => NO_AMBIENCE_SECS,
        Some(file) => {
            let volume = AMBIENCE_VOLUME * DEFAULT_VOLUME / 100.0;
            let event = SoundEvent {
                file: file.clone(),
                volume,
                speed: 1.0,
                offset: None,
            };
            send(world, client, CHANNEL_AMBIENCE, Some(event), false);
            let length = sound_length(world, &file);
            next_cooldown(length, &dm, &mut rng)
        }
    };
    world.resource_mut::<SoundRng>().0 = rng;
    cooldown
}

/// tg `refresh_looping_ambience` of a player mob.
pub fn refresh_buzz(world: &mut World, mob: Entity, client: Entity) {
    if world.get::<Ghost>(mob).is_some() {
        return;
    }
    let w: &World = world;
    let want = area_of(w, mob).and_then(|(pos, area)| {
        let dm = w.get::<DmArea>(area)?;
        let volume = DEFAULT_VOLUME;
        if dm.ambient_buzz.is_empty() || volume <= 0.0 || deaf(w, mob) {
            return None;
        }
        let mining = ss13_mapload::mining_z(w) == Some(pos.z);
        (mining || environ_powered(w, area)).then(|| {
            (
                dm.ambient_buzz.clone(),
                dm.ambient_buzz_vol * volume / 100.0,
            )
        })
    });
    let current = world.resource::<Ambience>().buzz.get(&client).cloned();
    match want {
        None => {
            if current.is_some() {
                world.resource_mut::<Ambience>().buzz.remove(&client);
                send(world, client, CHANNEL_BUZZ, None, false);
            }
        }
        // Don't reset current loops.
        Some((file, _)) if current.as_deref() == Some(file.as_str()) => {}
        Some((file, volume)) => {
            world
                .resource_mut::<Ambience>()
                .buzz
                .insert(client, file.clone());
            let event = SoundEvent {
                file,
                volume,
                speed: 1.0,
                offset: None,
            };
            send(world, client, CHANNEL_BUZZ, Some(event), true);
        }
    }
}

/// The buzz needs `area.apc`, `operating`, a charged cell (when the area `requires_power`) and
/// `power_environ`.
fn environ_powered(world: &World, area: Entity) -> bool {
    let Some(power) = world.get::<AreaPower>(area) else {
        return false;
    };
    let Some(apc) = power.apc.and_then(|a| world.get::<Apc>(a)) else {
        return false;
    };
    let charged = apc.cell.as_ref().is_some_and(|c| c.charge > 0.0);
    apc.operating && (charged || !power.requires_power) && power.power[Channel::Environ as usize]
}

fn players(world: &mut World) -> Vec<(Entity, Entity)> {
    world
        .query::<(Entity, &PlayerMob)>()
        .iter(world)
        .map(|(mob, p)| (mob, p.client))
        .collect()
}

/// `SSambience.fire` and the buzz of every player, once a second.
fn fire(world: &mut World, mut clock: Local<f64>) {
    *clock += world.resource::<Time>().delta_secs_f64();
    if *clock < AMBIENCE_WAIT_SECS {
        return;
    }
    *clock %= AMBIENCE_WAIT_SECS;
    let now = world.resource::<Time>().elapsed_secs_f64();
    let players = players(world);
    // A client without a player mob (gone, or back in the lobby) leaves the lists.
    let gone: Vec<Entity> = {
        let state = world.resource::<Ambience>();
        (state.next.keys().chain(state.buzz.keys()))
            .filter(|c| !players.iter().any(|(_, p)| p == *c))
            .copied()
            .collect()
    };
    for client in gone {
        let mut state = world.resource_mut::<Ambience>();
        state.next.remove(&client);
        state.old_areas.remove(&client);
        let playing = state.buzz.remove(&client).is_some();
        if playing && world.get_entity(client).is_ok() {
            send(world, client, CHANNEL_BUZZ, None, false);
        }
    }
    for (mob, client) in players {
        refresh_buzz(world, mob, client);
        tick_ambience(world, mob, client, now);
    }
}

fn tick_ambience(world: &mut World, mob: Entity, client: Entity, now: f64) {
    let Some(next) = world.resource::<Ambience>().next.get(&client).copied() else {
        world
            .resource_mut::<Ambience>()
            .next
            .insert(client, now + FIRST_AMBIENCE_SECS);
        return;
    };
    if deaf(world, mob) {
        return;
    }
    let Some((pos, area)) = area_of(world, mob) else {
        return;
    };
    let forced = world.get::<DmArea>(area).is_some_and(|a| a.forced_ambience);
    let new_area = world.resource::<Ambience>().old_areas.get(&client) != Some(&area);
    let due = ambience_due(next, now, forced, new_area, || {
        world
            .resource_mut::<SoundRng>()
            .0
            .chance(FORCED_AMBIENCE_PROB)
    });
    if !due {
        return;
    }
    let cooldown = play_ambience(world, client, pos, area);
    let mut state = world.resource_mut::<Ambience>();
    state.next.insert(client, now + cooldown);
    state.old_areas.insert(client, area);
}

/// `/area/Entered` -> `update_ambience_area`: a player mob that moved refreshes its buzz at once.
fn refresh_moved(
    world: &mut World,
    moved: &mut SystemState<Query<(Entity, &PlayerMob), Changed<TilePos>>>,
) {
    let Ok(query) = moved.get(world) else {
        return;
    };
    let moved: Vec<(Entity, Entity)> = query.iter().map(|(mob, p)| (mob, p.client)).collect();
    for (mob, client) in moved {
        refresh_buzz(world, mob, client);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(files: &[&str]) -> DmArea {
        DmArea {
            ambientsounds: files.iter().map(|f| (*f).to_owned()).collect(),
            ..default()
        }
    }

    #[test]
    fn cooldown_is_length_plus_tg_roll() {
        let dm = area(&[]);
        let mut rng = CombatRng(3);
        for _ in 0..50 {
            let c = next_cooldown(12.5, &dm, &mut rng);
            // rand(4 SECONDS, 10 SECONDS) after the sound.
            assert!((16.5..=22.5).contains(&c), "{c}");
            // Whole deciseconds, as tg's `rand`.
            assert!((((c - 12.5) * 10.0).round() - (c - 12.5) * 10.0).abs() < 1e-9);
        }
        let fixed = DmArea {
            min_ambience_cooldown: 30.0,
            max_ambience_cooldown: 30.0,
            ..default()
        };
        assert_eq!(next_cooldown(0.0, &fixed, &mut rng), 30.0);
    }

    #[test]
    fn sound_comes_from_the_area_or_the_override() {
        let mut rng = CombatRng(9);
        assert_eq!(pick_sound(&area(&[]), None, &mut rng), None);
        let two = area(&["a.ogg", "b.ogg"]);
        let mut seen = std::collections::HashSet::new();
        for _ in 0..40 {
            seen.insert(pick_sound(&two, None, &mut rng).unwrap());
        }
        assert_eq!(seen.len(), 2);
        assert_eq!(pick_sound(&two, Some("c.ogg"), &mut rng), Some("c.ogg"));
    }

    #[test]
    fn cooldown_gates_the_sound_unless_a_newcomer_is_forced() {
        assert!(ambience_due(5.0, 5.0, false, false, || false));
        assert!(!ambience_due(9.0, 5.0, false, true, || true));
        // Maintenance lets a newcomer roll; the same area does not.
        assert!(ambience_due(9.0, 5.0, true, true, || true));
        assert!(!ambience_due(9.0, 5.0, true, true, || false));
        assert!(!ambience_due(9.0, 5.0, true, false, || panic!(
            "no roll in the same area"
        )));
    }
}
