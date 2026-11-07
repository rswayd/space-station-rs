//! The jukebox (server): tg `/obj/machinery/jukebox` (`dance_machine.dm`) with
//! `/datum/jukebox` (`code/datums/components/jukebox.dm`) and BandaStation's `Jukebox220`
//! window.
//!
//! - The tracks are the deployment's `config/jukebox_music/sounds/` (`load_songs_from_config`),
//!   read from the assets' `sound/config/jukebox_music/sounds/` so the client has them too:
//!   `Name+beat.ogg` (legacy `Name+length+beat`), the length from the file.
//! - A click opens the window: anchored, with access (`req_access`, the bar's), with tracks
//!   (otherwise a balloon or the CentCom error and the compiler buzz); a right click with access
//!   toggles the music.
//! - Playing ([`super::music`], `Reach::Jukebox`): the selected track at the jukebox's volume
//!   (0-50), looping or until its length is over; stopping starts a 10 s cooldown (a toggle
//!   meanwhile: "on cooldown", the buzz at most every 15 s). Power loss stops it. The window
//!   picks a track (not while playing), sets the volume and the loop.
//! - A wrench (2 s, `default_unfasten_wrench`) fastens or unfastens it while silent.
// shortcut: no disco machine lights, sparkles or dancing, no "Now playing" examine line, no
// BandaStation track upload; only `.ogg` tracks (their length is read from the file), and no
// default track (tg's `title3.ogg` is a remix we do not ship, see `crate::lobby_music`).

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::ecs::query::QueryItem;
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_mapload::MapLoadPlugin;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_shared::ui::UiStatus;
use ss13_tgui::UiInterface;
use ss13_tgui::server::{OpenUi, ServerUi, UiActed, UiParams, UiReject, UiWindow};

use super::music::{self, MusicPlayer, Reach};
use crate::instruments_ui::{JukeboxData, JukeboxSong, JukeboxUi};
use crate::obj_damage::is_under;
use crate::sound::{PlaySound, playsound};
use crate::tools::{now, tell};

pub const JUKEBOX: &str = "/obj/machinery/jukebox";
const DISCO: &str = "/obj/machinery/jukebox/disco";
/// `CONFIG_JUKEBOX_SOUNDS` under the client's sound root.
pub const TRACKS_DIR: &str = "config/jukebox_music/sounds";
/// `/datum/jukebox` `volume` (also the maximum).
pub const MAX_VOLUME: f32 = 50.0;
/// `jukebox_song_cd` after a stop, `jukebox_error_cd` between buzzes.
const SONG_COOLDOWN: f64 = 10.0;
const ERROR_COOLDOWN: f64 = 15.0;
/// `default_unfasten_wrench` default time, deciseconds.
const WRENCH_DS: f32 = 20.0;
const COMPILER_FAILURE: &str = "sound/machines/compiler/compiler-failure.ogg";
const TERMINAL_OFF: &str = "sound/machines/terminal/terminal_off.ogg";

/// A `/datum/track`.
#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    pub name: String,
    /// Under the client's sound root.
    pub file: String,
    /// Seconds.
    pub length: f64,
    /// `song_beat_deciseconds`, 0 unknown.
    pub beat: f32,
}

/// `load_songs_from_config`'s list, by name.
#[derive(Resource, Clone, Debug, Default)]
pub struct JukeboxTracks(pub Vec<Track>);

/// `/obj/machinery/jukebox` with its `/datum/jukebox` settings.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Jukebox {
    pub req_access: Vec<String>,
    pub selection: Option<String>,
    pub volume: f32,
    pub loops: bool,
    pub song_ready_at: f64,
    pub error_ready_at: f64,
    /// When the current (not looping) track ends.
    pub ends_at: Option<f64>,
    pub base_icon: String,
}

/// A track file name: `Name+beat.ext` or the legacy `Name+length+beat.ext`.
pub fn parse_track_name(file: &str) -> Option<(String, f32)> {
    let stem = file.rsplit_once('.').map_or(file, |(s, _)| s);
    let parts: Vec<&str> = stem.split('+').collect();
    let name = parts.first().filter(|n| !n.is_empty())?.to_string();
    let beat = match parts.len() {
        0 | 1 => 0.0,
        2 => super::song::text2num(parts[1]).unwrap_or(0.0),
        _ => super::song::text2num(parts[2]).unwrap_or(0.0),
    };
    Some((name, beat as f32))
}

pub(super) fn load_tracks(world: &mut World) {
    let Some(dir) = world.get_resource::<MapLoadPlugin>().map(|m| {
        m.assets
            .join(ss13_mapload::sound::SOUND_DIR)
            .join(TRACKS_DIR)
    }) else {
        return;
    };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    let mut files: Vec<String> = entries
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.to_lowercase().ends_with(".ogg"))
        .collect();
    files.sort();
    let mut tracks: Vec<Track> = Vec::new();
    for file in files {
        let Some((name, beat)) = parse_track_name(&file) else {
            continue;
        };
        let Some(length) = std::fs::read(dir.join(&file))
            .ok()
            .and_then(|b| ss13_mapload::sound::ogg_duration(&b))
        else {
            warn!("jukebox track {file}: unreadable length, skipped");
            continue;
        };
        // Names are unique (an assoc list by name): a later file replaces an earlier one.
        tracks.retain(|t| t.name != name);
        tracks.push(Track {
            name,
            file: format!("{TRACKS_DIR}/{file}"),
            length,
            beat,
        });
    }
    info!("{} jukebox tracks", tracks.len());
    world.insert_resource(JukeboxTracks(tracks));
}

/// `Initialize`: `music_player = new(src)` with a random selection.
pub(super) fn attach(world: &mut World, e: Entity, path: &str) {
    if world.get_entity(e).is_err() || world.get::<Jukebox>(e).is_some() {
        return;
    }
    let req_access = world
        .resource::<super::InstrumentDb>()
        .0
        .jukeboxes
        .get(path)
        .cloned()
        .unwrap_or_default();
    let tracks = world.resource::<JukeboxTracks>().0.clone();
    let selection = if tracks.is_empty() {
        None
    } else {
        let i = world
            .resource_mut::<crate::combat::CombatRng>()
            .range(0, tracks.len() as u32 - 1) as usize;
        Some(tracks[i].name.clone())
    };
    let base_icon = if is_under(path, DISCO) {
        "disco"
    } else {
        "jukebox"
    }
    .to_owned();
    world.entity_mut(e).insert((
        Jukebox {
            req_access,
            selection,
            volume: MAX_VOLUME,
            loops: false,
            song_ready_at: 0.0,
            error_ready_at: 0.0,
            ends_at: None,
            base_icon,
        },
        MusicPlayer::default(),
    ));
}

/// `allowed(user)` by ID; silicons pass (`HAS_SILICON_ACCESS`).
fn allowed(world: &World, e: Entity, mob: Entity) -> bool {
    let Some(j) = world.get::<Jukebox>(e) else {
        return false;
    };
    crate::silicon::interact::has_silicon_access(world, mob)
        || crate::inventory::id_allowed(world, mob, &j.req_access, &[])
}

fn playing(world: &World, e: Entity) -> bool {
    world
        .get::<MusicPlayer>(e)
        .is_some_and(MusicPlayer::playing)
}

/// `ui_status`'s refusals when the window would open; true when it may.
fn may_open(world: &mut World, client: Entity, mob: Entity, e: Entity) -> bool {
    if world.get::<crate::ghost::Ghost>(mob).is_some() {
        return true;
    }
    if !crate::obj_damage::anchored(world, e) {
        crate::balloon::balloon_alert(world, mob, e, "нужно закрепить!");
        return false;
    }
    let pos = world.get::<TilePos>(e).copied();
    if !allowed(world, e, mob) {
        crate::balloon::balloon_alert(world, mob, e, "доступ запрещён!");
        crate::hallucination::play_local(world, mob, pos, COMPILER_FAILURE, 20.0, true);
        return false;
    }
    if world.resource::<JukeboxTracks>().0.is_empty() {
        tell(
            world,
            client,
            Span::Warning,
            "Ошибка: для вашей станции не одобрено ни одного музыкального трека. \
             Обратитесь к Центральному командованию для решения этой проблемы.",
        );
        crate::hallucination::play_local(world, mob, pos, COMPILER_FAILURE, 25.0, true);
        return false;
    }
    true
}

/// An empty-hand click: the window (left), the music on or off (right, with access).
pub fn interact(world: &mut World, client: Entity, mob: Entity, e: Entity, right: bool) -> bool {
    if right {
        if allowed(world, e, mob) {
            toggle_playing(world, mob, e);
        }
        return true;
    }
    if may_open(world, client, mob, e) {
        let title = crate::medical::name_of(world, e);
        world.write_message(OpenUi::new::<JukeboxUi>(client, mob, e, title));
    }
    true
}

/// `wrench_act`: only while silent.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    e: Entity,
) -> bool {
    let wrench = matches!(
        crate::tools::tool_of(world, held),
        Some((crate::tools::ToolKind::Wrench, _))
    );
    if !wrench || playing(world, e) {
        return false;
    }
    super::unfasten(world, client, mob, held, e, WRENCH_DS);
    true
}

/// `toggle_playing(user)`.
pub fn toggle_playing(world: &mut World, mob: Entity, e: Entity) {
    if playing(world, e) {
        stop_music(world, e);
        return;
    }
    let Some(j) = world.get::<Jukebox>(e).cloned() else {
        return;
    };
    let t = now(world);
    if t >= j.song_ready_at {
        activate_music(world, e);
        return;
    }
    let left = (j.song_ready_at - t).ceil();
    crate::balloon::balloon_alert(world, mob, e, &format!("перезарядка {left} сек.!"));
    if t >= j.error_ready_at {
        if let Some(&pos) = world.get::<TilePos>(e) {
            playsound(world, PlaySound::new(pos, COMPILER_FAILURE, 25.0, true));
        }
        if let Some(mut j) = world.get_mut::<Jukebox>(e) {
            j.error_ready_at = t + ERROR_COOLDOWN;
        }
    }
}

/// `activate_music`.
pub fn activate_music(world: &mut World, e: Entity) {
    let Some(j) = world.get::<Jukebox>(e).cloned() else {
        return;
    };
    let Some(track) = j.selection.as_ref().and_then(|name| {
        world
            .resource::<JukeboxTracks>()
            .0
            .iter()
            .find(|t| &t.name == name)
            .cloned()
    }) else {
        return;
    };
    music::start(world, e, &track.file, j.volume, j.loops, Reach::Jukebox);
    let ends_at = (!j.loops).then(|| now(world) + track.length);
    if let Some(mut j) = world.get_mut::<Jukebox>(e) {
        j.ends_at = ends_at;
    }
    update_icon(world, e);
}

/// `stop_music`.
pub fn stop_music(world: &mut World, e: Entity) {
    music::stop(world, e);
    let t = now(world);
    if let Some(mut j) = world.get_mut::<Jukebox>(e) {
        j.ends_at = None;
        j.song_ready_at = t + SONG_COOLDOWN;
    }
    if let Some(&pos) = world.get::<TilePos>(e) {
        playsound(world, PlaySound::new(pos, TERMINAL_OFF, 50.0, true));
    }
    update_icon(world, e);
}

fn update_icon(world: &mut World, e: Entity) {
    let Some(base) = world.get::<Jukebox>(e).map(|j| j.base_icon.clone()) else {
        return;
    };
    let state = if playing(world, e) {
        format!("{base}-active")
    } else {
        base
    };
    crate::obj_damage::set_icon_state(world, e, &state);
}

/// The track's end and `on_set_is_operational` (power loss stops it).
pub(super) fn tick(world: &mut World) {
    let t = now(world);
    let due: Vec<Entity> = world
        .query::<(Entity, &Jukebox, &MusicPlayer)>()
        .iter(world)
        .filter(|(_, _, m)| m.playing())
        .filter(|&(e, j, _)| {
            j.ends_at.is_some_and(|end| t >= end)
                || world.get::<crate::power::Unpowered>(e).is_some()
        })
        .map(|(e, _, _)| e)
        .collect();
    for e in due {
        stop_music(world, e);
    }
}

/// `ui_status` while open: closed once unanchored or without access.
pub(super) fn window_status(world: &mut World) {
    let windows: Vec<(Entity, Entity, Entity)> = world
        .query::<(Entity, &UiWindow)>()
        .iter(world)
        .filter(|(_, w)| w.interface == JukeboxUi::NAME)
        .map(|(e, w)| (e, w.source, w.actor))
        .collect();
    for (window, source, actor) in windows {
        let ghost = world.get::<crate::ghost::Ghost>(actor).is_some();
        let ok = world.get::<Jukebox>(source).is_some()
            && (ghost
                || (crate::obj_damage::anchored(world, source) && allowed(world, source, actor)));
        if !ok && let Some(mut w) = world.get_mut::<UiWindow>(window) {
            w.cap_status(UiStatus::Close);
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum JukeboxAct {
    Toggle,
    Select(String),
    /// `None`: `reset`/`max`.
    Volume(Option<f64>),
    Loop(bool),
}

impl ServerUi for JukeboxUi {
    type Source = (&'static Jukebox, &'static MusicPlayer);
    type UserParam = Res<'static, JukeboxTracks>;
    type Act = JukeboxAct;

    fn static_data(_: QueryItem<'_, '_, Self::Source>) {}

    fn data(
        (j, m): QueryItem<'_, '_, Self::Source>,
        _: Entity,
        tracks: &SystemParamItem<'_, '_, Self::UserParam>,
    ) -> JukeboxData {
        JukeboxData {
            active: m.playing(),
            songs: tracks
                .0
                .iter()
                .map(|t| JukeboxSong {
                    name: t.name.clone(),
                    length: (t.length * 10.0).round() as u32,
                    beat: t.beat,
                })
                .collect(),
            track_selected: j.selection.clone(),
            looping: j.loops,
            volume: j.volume,
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<JukeboxAct, UiReject> {
        Ok(match action {
            "toggle" => JukeboxAct::Toggle,
            "select_track" => JukeboxAct::Select(params.text("track")?),
            "set_volume" => match params.get("volume") {
                Some("reset" | "max") => JukeboxAct::Volume(None),
                Some("min") => JukeboxAct::Volume(Some(0.0)),
                _ => JukeboxAct::Volume(Some(params.number("volume")?)),
            },
            "loop" => JukeboxAct::Loop(
                params
                    .get("looping")
                    .is_some_and(|v| v == "1" || v == "true"),
            ),
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

/// `ui_act`.
pub(super) fn acts(world: &mut World, mut cursor: Local<MessageCursor<UiActed<JukeboxUi>>>) {
    let acts: Vec<(Entity, Entity, Entity, JukeboxAct)> = cursor
        .read(world.resource::<Messages<UiActed<JukeboxUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, e, act) in acts {
        if world.get::<Jukebox>(e).is_none() {
            continue;
        }
        match act {
            JukeboxAct::Toggle => toggle_playing(world, actor, e),
            JukeboxAct::Select(name) => {
                if playing(world, e) {
                    tell(
                        world,
                        client,
                        Span::Warning,
                        "Ошибка: нельзя сменить трек, пока не закончился текущий.",
                    );
                    continue;
                }
                let known = world
                    .resource::<JukeboxTracks>()
                    .0
                    .iter()
                    .any(|t| t.name == name);
                if known && let Some(mut j) = world.get_mut::<Jukebox>(e) {
                    j.selection = Some(name);
                }
            }
            JukeboxAct::Volume(volume) => {
                let volume = volume.map_or(MAX_VOLUME, |v| (v as f32).clamp(0.0, MAX_VOLUME));
                if let Some(mut j) = world.get_mut::<Jukebox>(e) {
                    j.volume = volume;
                }
                music::set_volume(world, e, volume);
            }
            JukeboxAct::Loop(looping) => {
                if let Some(mut j) = world.get_mut::<Jukebox>(e) {
                    j.loops = looping;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_names_carry_their_beat() {
        assert_eq!(
            parse_track_name("Space Jazz+5.ogg"),
            Some(("Space Jazz".into(), 5.0))
        );
        // Legacy `name+length+beat`.
        assert_eq!(
            parse_track_name("Old+2400+8.ogg"),
            Some(("Old".into(), 8.0))
        );
        assert_eq!(parse_track_name("Plain.ogg"), Some(("Plain".into(), 0.0)));
        assert_eq!(parse_track_name("+3.ogg"), None);
    }
}
