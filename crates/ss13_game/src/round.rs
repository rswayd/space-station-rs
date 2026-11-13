//! The lobby and the round (server), after tg `SSticker`, `/mob/dead/new_player` and
//! `SSjob.divide_occupations` (BandaStation's job priority staffing).
//!
//! A connecting client is a new player: a [`NewPlayer`] on its connection entity, no body, nothing
//! replicated, the lobby screen ([`LobbyUi`]) open. [`Ticker`] runs tg's states: `PREGAME` counts
//! [`RoundConfig::lobby_secs`] down; `SETTING_UP` divides the jobs among the ready players
//! ([`divide_occupations`]) and spawns them all in the same tick at their jobs' start landmarks;
//! `PLAYING` lets lobby clients latejoin (`crate::jobs`) or observe (a ghost at
//! `/obj/effect/landmark/observer_start`); the round ends when [`Ticker::force_ending`] is set
//! (tg's admin "End Round", or `SS13_ROUND_SECONDS` in debug builds): `FINISHED` prints tg's
//! round-end report and, after `round_end_countdown`, reboots the world.
//!
//! Job preferences are the jobs page of tg's preferences menu ([`JobPrefsUi`]): high, medium, low
//! or never per job (one high at most) and the jobless role, kept for the connection.
// shortcut: the reboot is a new server `App` in the same process (`REBOOT_EXIT_CODE`, see
// `ss13_server`): the map is loaded fresh and clients reconnect to the same certificate on their
// own (~1 s), like BYOND clients on `world.Reboot()`. An in-process reset that keeps the sessions
// needs every plugin's startup to be rerunnable; do it once reconnects become a problem.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::MapLoadPlugin;
use ss13_net::chat::server::SendChat;
use ss13_net::server::ClientToken;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_shared::{Facing, TilePos};
use ss13_tgui::interfaces::job_selection::JobSelectionStatic;
use ss13_tgui::server::{
    AlwaysState, AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems,
};

use crate::antag::dynamic::AntagPreferences;
use crate::ghost::spawn_ghost;
use crate::health::Stat;
use crate::human::{
    AppearancePrefs, EYE_COLORS, HAIR_COLORS, HAIR_STYLE_NAMES, SKIN_TONES, hex_rgb,
};
use crate::jobs::{CharacterName, Jobs, JobsStartup, PlayerJob, character_name, display_time_text};
use crate::lobby::{
    APPEARANCE_EYE_COLOR, APPEARANCE_FEMALE, APPEARANCE_HAIR_COLOR, APPEARANCE_HAIR_STYLE,
    APPEARANCE_MUTANT_COLOR, APPEARANCE_SKIN_TONE, APPEARANCE_SPECIES, AppearanceData,
    CHARACTER_SETUP, GIVE_QUIRK, GameState, JOBLESS_ROLE, JobPrefsData, JobPrefsStatic, JobPrefsUi,
    JobPriority, JoblessRole, LobbyData, LobbyUi, OBSERVE, QuirkInfo, RANDOMIZE_APPEARANCE,
    REMOVE_QUIRK, SET_ANTAGS, SET_APPEARANCE, SET_JOB_PREFERENCE, SET_PREFERENCE,
    TOGGLE_ANTAG_ROLL, TOGGLE_READY,
};
use crate::lobby::{CLEAR_PERSONALITIES, HANDLE_PERSONALITY, PersonalityInfo};
use crate::lobby::{
    CrewManifestData, CrewManifestUi, DELAY, DepartmentPositions, LobbyTrait, MANIFEST, NOTICE,
    RESET_HUD, START_NOW, TRAIT_PARAM, TRAIT_SIGNUP,
};
use crate::personality::PersonalityPrefs;
use crate::quirks::QuirkPrefs;
use crate::server::{MoveQueue, Players, SpawnPoints};

/// The server process builds a new `App` when one exits with this code (`ss13_server`).
pub const REBOOT_EXIT_CODE: u8 = 75;
/// tg `start_now` cancelled: `SetTimeLeft(3 MINUTES)`.
const START_NOW_CANCEL_SECS: f64 = 180.0;
/// tg `declare_completion`: `sleep(5 SECONDS)` before `standard_reboot`.
const REPORT_TO_REBOOT_SECS: f64 = 5.0;
/// BandaStation lobby "delay": `static/time` toggles between 1.5 minutes and 1984 days.
const DELAY_SECS: f64 = 90.0;
const DELAY_FOREVER_SECS: f64 = 1984.0 * 86_400.0;
/// tg `RESET_HUD_INTERVAL`.
const RESET_HUD_SECS: f64 = 15.0;
/// BandaStation `change_title_screen_notice` `max_length`.
pub const NOTICE_MAX_LEN: u32 = 2048;
/// From the round end song to the reboot.
const ROUND_END_SOUND_LEAD_SECS: f64 = 1.0;
/// tg `security_scaling_coeff`, BandaStation `security_min_positions`/`security_max_positions`
/// (config defaults).
const SECURITY_SCALING_COEFF: f64 = 8.0;
const SECURITY_MIN_POSITIONS: i32 = 5;
const SECURITY_MAX_POSITIONS: i32 = 12;
const SECURITY_OFFICER: &str = "Security Officer";

/// Round timing. Insert before `GameServerPlugin` to override; otherwise read from the
/// environment:
/// - `SS13_LOBBY_SECONDS` (default 120, tg `lobby_countdown`): the lobby countdown; `0` starts the
///   round at once (tg `start_immediately`), clients still land in the lobby;
/// - `SS13_ROUND_END_SECONDS` (default 25, tg `round_end_countdown`): from the reboot notice to
///   the reboot;
/// - `SS13_SKIP_LOBBY=1` (dev): the round is in progress from the start and every connecting
///   client joins at once as the overflow role, as before the lobby existed;
/// - `SS13_ROUND_SECONDS` (dev, debug builds only): the round ends that long after it started;
/// - `SS13_SOURCE_URL`: where players get this server's source code (AGPL-3.0 §13), shown in
///   the lobby; unset logs a warning at startup.
/// - `SS13_REOPEN_SUICIDE_ROLES=1` (tg `REOPEN_ROUNDSTART_SUICIDE_ROLES`, off by default):
///   [`ReopenSuicideRoles`], with `SS13_REOPEN_SUICIDE_ROLES_DELAY` (seconds, at least 30,
///   default 240), `SS13_REOPEN_SUICIDE_ROLES_COMMAND=1` and `SS13_REOPEN_SUICIDE_ROLES_REPORT=1`;
/// - `SS13_HARD_POPCAP`, `SS13_EXTREME_POPCAP` (tg `HARD_POPCAP`, `EXTREME_POPCAP`, unset by
///   default): living players at which latejoiners queue (`crate::join_queue`);
/// - `SS13_JOB_RESTRICTIONS` (job titles), `SS13_ALLOWED_SPECIES` (`/datum/species/*` paths),
///   comma separated (BandaStation `JOB_RESTRICTIONS`, `ALLOWED_SPECIES`, unset by default):
///   only the allowed species may take the restricted jobs (`crate::jobs::species_restricted`).
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct RoundConfig {
    pub lobby_secs: f64,
    pub round_end_secs: f64,
    pub skip_lobby: bool,
    pub round_secs: Option<f64>,
    pub source_url: Option<String>,
    pub reopen_suicide_roles: Option<ReopenSuicideRoles>,
    pub hard_popcap: Option<u32>,
    pub extreme_popcap: Option<u32>,
    /// BandaStation `job_restrictions`: titles only [`Self::allowed_species`] may take.
    pub job_restrictions: Vec<String>,
    /// BandaStation `allowed_species`: species paths; empty restricts nobody.
    pub allowed_species: Vec<String>,
}

/// tg `reopen_roundstart_suicide_roles*` config: the jobs of the players who killed themselves
/// early in the round open again for latejoiners ([`reopen_roundstart_suicide_roles`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReopenSuicideRoles {
    /// `reopen_roundstart_suicide_roles_delay`: after the round start (min 30 s; unset 4 min).
    pub delay_secs: f64,
    /// `reopen_roundstart_suicide_roles_command_positions`: command jobs reopen too.
    pub command_positions: bool,
    /// `reopen_roundstart_suicide_roles_command_report`: CentCom reports the reopened jobs.
    pub command_report: bool,
}

/// tg `reopen_roundstart_suicide_roles_delay` when unset (`|| 4 MINUTES`) and its `min_val`.
const REOPEN_DEFAULT_SECS: f64 = 240.0;
const REOPEN_MIN_SECS: f64 = 30.0;

impl RoundConfig {
    /// tg's defaults, no environment.
    pub fn tg() -> Self {
        Self {
            lobby_secs: 120.0,
            round_end_secs: 25.0,
            skip_lobby: false,
            round_secs: None,
            source_url: None,
            reopen_suicide_roles: None,
            hard_popcap: None,
            extreme_popcap: None,
            job_restrictions: Vec::new(),
            allowed_species: Vec::new(),
        }
    }

    /// Dev and tests: no lobby, a new client gets a body at once.
    pub fn skip_lobby() -> Self {
        Self {
            skip_lobby: true,
            ..Self::tg()
        }
    }
}

impl Default for RoundConfig {
    fn default() -> Self {
        let secs = |key: &str| {
            std::env::var(key)
                .ok()
                .and_then(|v| v.trim().parse::<f64>().ok())
                .filter(|v| v.is_finite() && *v >= 0.0)
        };
        let tg = Self::tg();
        let flag = |key: &str| std::env::var(key).is_ok_and(|v| v == "1");
        let reopen_suicide_roles = flag("SS13_REOPEN_SUICIDE_ROLES").then(|| ReopenSuicideRoles {
            delay_secs: secs("SS13_REOPEN_SUICIDE_ROLES_DELAY")
                .map_or(REOPEN_DEFAULT_SECS, |s| s.max(REOPEN_MIN_SECS)),
            command_positions: flag("SS13_REOPEN_SUICIDE_ROLES_COMMAND"),
            command_report: flag("SS13_REOPEN_SUICIDE_ROLES_REPORT"),
        });
        // tg `min_val = 0`; 0 is no cap (`if(hard_popcap)`).
        let cap = |key: &str| {
            std::env::var(key)
                .ok()
                .and_then(|v| v.trim().parse::<u32>().ok())
                .filter(|&v| v > 0)
        };
        // tg `str_list`: one entry per line there, comma separated here.
        let list = |key: &str| -> Vec<String> {
            std::env::var(key)
                .map(|v| {
                    v.split(',')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default()
        };
        let round_secs = secs("SS13_ROUND_SECONDS");
        if round_secs.is_some() && !cfg!(debug_assertions) {
            warn!("SS13_ROUND_SECONDS is ignored: it exists in debug builds only");
        }
        Self {
            lobby_secs: secs("SS13_LOBBY_SECONDS").unwrap_or(tg.lobby_secs),
            round_end_secs: secs("SS13_ROUND_END_SECONDS").unwrap_or(tg.round_end_secs),
            skip_lobby: std::env::var("SS13_SKIP_LOBBY").is_ok_and(|v| v == "1"),
            round_secs: round_secs.filter(|_| cfg!(debug_assertions)),
            source_url: std::env::var("SS13_SOURCE_URL")
                .ok()
                .map(|v| v.trim().to_owned())
                .filter(|v| !v.is_empty()),
            reopen_suicide_roles,
            hard_popcap: cap("SS13_HARD_POPCAP"),
            extreme_popcap: cap("SS13_EXTREME_POPCAP"),
            job_restrictions: list("SS13_JOB_RESTRICTIONS"),
            allowed_species: list("SS13_ALLOWED_SPECIES"),
        }
    }
}

/// tg `SSticker`.
#[derive(Resource, Debug, Default)]
pub struct Ticker {
    state: GameState,
    /// tg `timeLeft`, seconds.
    time_left: f64,
    /// `Time::elapsed_secs_f64` at `SETTING_UP` -> `PLAYING`.
    round_start: f64,
    finished_at: f64,
    reboot_at: Option<f64>,
    /// tg `round_end_sound_sent`: the round end song went out; the reboot follows.
    round_end_sound_sent: bool,
    /// When [`reopen_roundstart_suicide_roles`] runs (tg's `addtimer` in `PostSetup`).
    reopen_at: Option<f64>,
    /// tg `force_ending`: an admin ended the round; it ends on the next tick.
    pub force_ending: bool,
    /// tg `start_immediately`: an admin started the round; the lobby ends on the next tick.
    start_immediately: bool,
    /// BandaStation's lobby "delay" toggle is on the long wait (its `static/time`).
    delayed: bool,
    total_players: u32,
    ready_players: u32,
    in_lobby: u32,
}

impl Ticker {
    pub fn state(&self) -> GameState {
        self.state
    }

    /// tg `totalPlayersReady`: the ready players when the lobby last counted them.
    pub fn ready_players(&self) -> u32 {
        self.ready_players
    }

    /// tg `start_now` verb: `Ok(true)` starts the round as soon as possible, `Ok(false)` cancels an
    /// earlier start and counts down 3 minutes; refused once the round is setting up.
    pub fn start_now(&mut self) -> Result<bool, &'static str> {
        if self.state > GameState::Pregame {
            return Err("Игра уже началась!");
        }
        if self.start_immediately {
            self.start_immediately = false;
            self.time_left = START_NOW_CANCEL_SECS;
            return Ok(false);
        }
        self.start_immediately = true;
        Ok(true)
    }

    /// Seconds since the round started (tg `world.time - SSticker.round_start_time`).
    pub fn round_duration(&self, now: f64) -> f64 {
        if self.state >= GameState::Playing {
            now - self.round_start
        } else {
            0.0
        }
    }
}

/// On a client connection in the lobby: tg `/mob/dead/new_player`. Removed when it joins or
/// observes.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NewPlayer {
    /// tg `ready == PLAYER_READY_TO_PLAY`.
    pub ready: bool,
}

/// On a client connection: tg `prefs.job_preferences` and the jobless role, kept for the
/// connection. Keys are indexes into `JobsData::jobs`.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct JobPreferences {
    pub levels: HashMap<usize, JobPriority>,
    pub jobless: JoblessRole,
    /// tg `SSjob.prevented_occupations`: jobs this player may not get this round (a roundstart
    /// antagonist's blacklisted roles). Set on the round start copy only.
    pub prevented: Vec<usize>,
}

impl JobPreferences {
    /// tg `set_job_preference_level`: a new high demotes the old one to medium, and the
    /// overflow role (a toggle) to never.
    pub fn set(&mut self, job: usize, level: Option<JobPriority>, overflow: usize) {
        if level == Some(JobPriority::High) {
            self.levels
                .retain(|&j, l| !(j == overflow && *l == JobPriority::High));
            for l in self.levels.values_mut() {
                if *l == JobPriority::High {
                    *l = JobPriority::Medium;
                }
            }
        }
        match level {
            Some(level) => self.levels.insert(job, level),
            None => self.levels.remove(&job),
        };
    }
}

/// On a client connection whose active admin rights include `R_ADMIN` or `R_DEBUG`
/// (BandaStation `check_rights_for(client, R_ADMIN|R_DEBUG)`): the lobby shows the admin buttons.
/// Kept in sync by `ss13_admin`, which also handles the admin acts and checks the rights again.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct LobbyAdmin;

/// BandaStation `SStitle.notice`: the lobby's admin notice.
// shortcut: not persisted between rounds (BandaStation's `TITLE_NOTICE_FILE`); the reboot is a
// new `App`, add a file next to the admin config when admins ask for it.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct LobbyNotice(pub Option<String>);

/// A lobby admin button pressed, for `ss13_admin` to check the client's rights, apply and log
/// (BandaStation `new_player/Topic` `picture`/`notice`/`start_now`/`delay`).
#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct LobbyAdminAct {
    pub client: Entity,
    pub request: LobbyAdminRequest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LobbyAdminRequest {
    StartNow,
    Delay,
    /// The notice the admin typed (`None`: the button of a client without [`LobbyAdmin`],
    /// asked nothing, for the exploit warning).
    Notice(Option<String>),
}

/// On the prompt of BandaStation `change_title_screen_notice`.
#[derive(Component)]
struct NoticePrompt;

/// tg `reset_hud_cooldown`: when the client may reset the lobby HUD again.
#[derive(Component, Clone, Copy, Debug)]
struct ResetHudCooldown(f64);

/// The preferences window's static half, on every client connection.
#[derive(Component, Clone, Debug)]
pub struct PrefsBoard(Arc<JobPrefsStatic>);

#[derive(Resource)]
struct PrefsStatic(Arc<JobPrefsStatic>);

pub struct RoundServerPlugin;

impl Plugin for RoundServerPlugin {
    fn build(&self, app: &mut App) {
        if !app.world().contains_resource::<RoundConfig>() {
            app.init_resource::<RoundConfig>();
        }
        let config = app.world().resource::<RoundConfig>().clone();
        info!("round: {config:?}");
        if config.source_url.is_none() {
            warn!(
                "SS13_SOURCE_URL is not set: AGPL-3.0 section 13 requires offering players this \
                 server's source code; the lobby shows no source link"
            );
        }
        app.insert_resource(Ticker {
            // No lobby: the round is in progress from the first tick.
            state: if config.skip_lobby {
                GameState::Playing
            } else {
                GameState::Startup
            },
            ..default()
        })
        .register_ui::<LobbyUi>()
        .register_ui::<JobPrefsUi>()
        .register_ui::<CrewManifestUi>()
        .init_resource::<LobbyNotice>()
        .add_message::<LobbyAdminAct>();
        crate::lobby_music::build(app);
        app.init_resource::<crate::join_queue::JoinQueue>()
            .add_systems(Startup, build_prefs_static.after(JobsStartup))
            .add_systems(
                Update,
                (
                    tick.before(UiSystems::Open),
                    crate::join_queue::fire.after(tick).before(UiSystems::Open),
                    (lobby_acts, preference_acts).in_set(UiSystems::Handle),
                    notice_answers
                        .in_set(UiSystems::Handle)
                        .after(ss13_tgui::prompt::PromptSystems::Answer),
                ),
            );
    }
}

fn build_prefs_static(world: &mut World) {
    let jobs = world.resource::<Jobs>();
    let st: &JobSelectionStatic = &jobs.static_data;
    let prefs = JobPrefsStatic {
        departments: st.departments.clone(),
        overflow: jobs.data.jobs[jobs.default_job].title.clone(),
        unavailable: jobs
            .data
            .jobs
            .iter()
            .filter(|j| !j.unresolved.is_empty())
            .map(|j| j.title.clone())
            .collect(),
        skin_tones: SKIN_TONES
            .iter()
            .map(|(name, rgb)| ((*name).to_owned(), *rgb))
            .collect(),
        hair_styles: HAIR_STYLE_NAMES.iter().map(|n| (*n).to_owned()).collect(),
        hair_colors: HAIR_COLORS.iter().map(|&c| hex_rgb(c)).collect(),
        eye_colors: EYE_COLORS.iter().map(|&c| hex_rgb(c)).collect(),
        quirks: crate::quirks::QUIRKS
            .iter()
            .map(|q| QuirkInfo {
                name: q.name.to_owned(),
                description: q.desc.to_owned(),
                icon: q.icon.to_owned(),
                value: q.value,
            })
            .collect(),
        quirk_blacklist: crate::quirks::BLACKLIST
            .iter()
            .map(|list| list.iter().map(|q| q.def().name.to_owned()).collect())
            .collect(),
        default_quirk_balance: crate::quirks::DEFAULT_QUIRK_POINTS,
        max_positive_quirks: crate::quirks::MAX_POSITIVE_QUIRKS as u8,
        personalities: crate::personality::PERSONALITIES
            .iter()
            .map(|p| PersonalityInfo {
                key: p.key.to_owned(),
                name: p.name.to_owned(),
                description: p.desc.to_owned(),
                pos_gameplay_description: p.pos.to_owned(),
                neg_gameplay_description: p.neg.to_owned(),
                neut_gameplay_description: p.neut.to_owned(),
                groups: p.groups.iter().map(|g| g.key().to_owned()).collect(),
            })
            .collect(),
        // tg: -1 when the limit is not below the number of personalities.
        max_personalities: if crate::personality::MAX_PERSONALITIES
            >= crate::personality::PERSONALITIES.len()
        {
            -1
        } else {
            crate::personality::MAX_PERSONALITIES as i32
        },
        species: crate::human::roundstart_species()
            .into_iter()
            .map(|s| s.def().name.to_owned())
            .collect(),
        mutant_colors: crate::human::MUTANT_COLORS
            .iter()
            .map(|&c| hex_rgb(c))
            .collect(),
    };
    world.insert_resource(PrefsStatic(Arc::new(prefs)));
}

/// A client that passed the handshake: tg `new_player/Login`, or with the lobby skipped a body of
/// the overflow role at once.
pub(crate) fn new_player(world: &mut World, client: Entity) {
    if world.resource::<RoundConfig>().skip_lobby {
        // Dev, debug builds: `SS13_DEV_JOB=AI` gives every skip-lobby client that job.
        let dev_job = std::env::var("SS13_DEV_JOB")
            .ok()
            .filter(|_| cfg!(debug_assertions))
            .and_then(|title| world.resource::<Jobs>().data.job(&title));
        let job = dev_job.unwrap_or(world.resource::<Jobs>().default_job);
        let mob = crate::jobs::join(world, client, job, false);
        crate::antag::traitor::dev_traitor(world, client, mob);
        return;
    }
    character_name(world, client);
    let board = PrefsBoard(world.resource::<PrefsStatic>().0.clone());
    world.entity_mut(client).insert((
        NewPlayer::default(),
        JobPreferences::default(),
        AntagPreferences::default(),
        AppearancePrefs::random(client.to_bits()),
        QuirkPrefs::default(),
        PersonalityPrefs::default(),
        board,
        AlwaysState,
    ));
    world.write_message(OpenUi::new::<LobbyUi>(client, client, client, ""));
    let ticker = world.resource::<Ticker>();
    if ticker.state < GameState::SettingUp {
        let when = if ticker.time_left > 0.0 {
            format!("через {}", display_time_text(ticker.time_left))
        } else {
            "скоро".to_owned()
        };
        let text = format!(
            "Пожалуйста, настройте своего персонажа и нажмите \"Готов\". Игра будет запущена {when}."
        );
        chat(world, vec![client], Span::Plain, text);
    }
}

fn chat(world: &mut World, clients: Vec<Entity>, span: Span, text: impl Into<String>) {
    world.write_message(SendChat {
        clients,
        message: ChatMessage::simple(ChatChannel::System, span, text),
    });
}

fn all_clients(world: &mut World) -> Vec<Entity> {
    let mut query = world.query_filtered::<Entity, With<ClientToken>>();
    query.iter(world).collect()
}

/// tg `to_chat(world, ...)`.
/// The admin `Start Now` verb ([`Ticker::start_now`]); a cancel is announced to everyone.
pub fn start_now(world: &mut World) -> Result<bool, &'static str> {
    let started = world.resource_mut::<Ticker>().start_now()?;
    if !started {
        to_world(world, Span::BoldNotice, "Игра начнётся через 3 минуты.");
    }
    Ok(started)
}

/// BandaStation's lobby "delay" button: the countdown is set to 1.5 minutes, or to 1984 days
/// when it was 1.5 minutes (a toggle), and an earlier "start now" is cancelled. Returns the new
/// countdown; refused once the round is setting up.
pub fn delay_start(world: &mut World) -> Result<f64, &'static str> {
    let mut ticker = world.resource_mut::<Ticker>();
    if ticker.state > GameState::Pregame {
        return Err("Слишком поздно... Игра уже началась!");
    }
    ticker.delayed = !ticker.delayed;
    let secs = if ticker.delayed {
        DELAY_FOREVER_SECS
    } else {
        DELAY_SECS
    };
    ticker.time_left = secs;
    ticker.start_immediately = false;
    let text = format!("Игра начнётся через {}.", display_time_text(secs));
    to_world(world, Span::Bold, text);
    Ok(secs)
}

/// BandaStation `SStitle.set_notice` with `change_title_screen_notice`'s announcement to the
/// lobby: an empty text removes the notice.
pub fn set_notice(world: &mut World, text: &str) {
    let text: String = text.trim().chars().take(NOTICE_MAX_LEN as usize).collect();
    let announce = if text.is_empty() {
        "УВЕДОМЛЕНИЕ В ЛОББИ УДАЛЕНО.".to_owned()
    } else {
        format!("УВЕДОМЛЕНИЕ В ЛОББИ ОБНОВЛЕНО: {text}")
    };
    world.resource_mut::<LobbyNotice>().0 = (!text.is_empty()).then_some(text);
    let mut lobby = world.query_filtered::<Entity, With<NewPlayer>>();
    let clients: Vec<Entity> = lobby.iter(world).collect();
    chat(world, clients, Span::BoldAnnounce, announce);
}

pub fn to_world(world: &mut World, span: Span, text: impl Into<String>) {
    let clients = all_clients(world);
    chat(world, clients, span, text);
}

/// tg `station_name()`: the name a charter gave (`crate::misc_items::charter`), else the map's.
pub(crate) fn station_name(world: &World) -> String {
    if let Some(name) = world
        .get_resource::<crate::misc_items::charter::StationName>()
        .filter(|n| !n.0.is_empty())
    {
        return name.0.clone();
    }
    world
        .get_resource::<MapLoadPlugin>()
        .and_then(|m| m.maps.first())
        .map(|m| {
            let name = m.rsplit('/').next().unwrap_or(m);
            name.trim_end_matches(".dmm").to_owned()
        })
        .unwrap_or_else(|| "станции".to_owned())
}

/// tg `SSticker/fire`, once per tick.
fn tick(world: &mut World) {
    let time = world.resource::<Time>();
    let (now, dt) = (time.elapsed_secs_f64(), time.delta_secs_f64());
    let mut lobby = world.query::<&NewPlayer>();
    let (in_lobby, ready) = lobby
        .iter(world)
        .fold((0, 0), |(n, r), p| (n + 1, r + u32::from(p.ready)));
    let total = world
        .query_filtered::<(), With<ClientToken>>()
        .iter(world)
        .count() as u32;
    let config = world.resource::<RoundConfig>().clone();
    // tg `check_finished`: the shuttle reached CentCom or the station was nuked.
    let shuttle_home =
        crate::shuttle::endgame(world) || crate::antag::nukeop::server::station_destroyed(world);
    let mut ticker = world.resource_mut::<Ticker>();
    ticker.in_lobby = in_lobby;
    ticker.ready_players = ready;
    ticker.total_players = total;
    match ticker.state {
        GameState::Startup => {
            ticker.state = GameState::Pregame;
            ticker.time_left = config.lobby_secs;
            info!("lobby: the round starts in {} s", config.lobby_secs);
        }
        GameState::Pregame => {
            ticker.time_left -= dt;
            if ticker.time_left <= 0.0 || ticker.start_immediately {
                ticker.state = GameState::SettingUp;
            }
        }
        GameState::SettingUp => setup(world, now),
        GameState::Playing => {
            let reopen_due = ticker.reopen_at.is_some_and(|at| now >= at);
            if reopen_due {
                ticker.reopen_at = None;
            }
            let timed_out = config
                .round_secs
                .is_some_and(|secs| now - ticker.round_start >= secs);
            let ending = ticker.force_ending || timed_out || shuttle_home;
            if ending {
                ticker.state = GameState::Finished;
                ticker.finished_at = now;
            }
            if reopen_due && let Some(reopen) = config.reopen_suicide_roles {
                reopen_roundstart_suicide_roles(world, reopen);
            }
            if ending {
                declare_completion(world, now);
            }
        }
        GameState::Finished => {
            let reboot_at = ticker.reboot_at;
            if reboot_at.is_none() && now - ticker.finished_at >= REPORT_TO_REBOOT_SECS {
                ticker.reboot_at = Some(now + config.round_end_secs);
                let text = format!(
                    "Перезагрузка мира через {}. Раунд окончен.",
                    display_time_text(config.round_end_secs)
                );
                to_world(world, Span::BoldAnnounce, text);
            } else if reboot_at.is_some_and(|at| now >= at) {
                if !ticker.round_end_sound_sent {
                    // `Shutdown`'s round end song, a moment before the exit so that the
                    // streams deliver it (the client keeps playing it while it reconnects).
                    ticker.round_end_sound_sent = true;
                    ticker.reboot_at = Some(now + ROUND_END_SOUND_LEAD_SECS);
                    let song = crate::lobby_music::play_round_end_song(world);
                    info!("round end song: {song}");
                } else {
                    info!("round over: rebooting the world");
                    world.write_message(AppExit::from_code(REBOOT_EXIT_CODE));
                }
            }
        }
    }
}

/// tg `SSticker/setup`: jobs for the ready players, everyone spawned at once.
fn setup(world: &mut World, now: f64) {
    to_world(world, Span::BoldAnnounce, "Игра начинается...");
    let mut query = world.query::<(Entity, &NewPlayer, &JobPreferences)>();
    let mut ready: Vec<(Entity, JobPreferences)> = query
        .iter(world)
        .filter(|(_, p, _)| p.ready)
        .map(|(e, _, prefs)| (e, prefs.clone()))
        .collect();
    // The order `divide_occupations` shuffles from: the same seed gives the same jobs.
    ready.sort_by_key(|(e, _)| *e);
    // `SSdynamic.select_roundstart_antagonists`: before the jobs, which it restricts.
    let antags = crate::antag::dynamic::prepare_roundstart(world, &mut ready);
    // BandaStation's species job restrictions. Its ready check keeps them out of the wanted
    // jobs only; the random job fallback could still land on one, so they are prevented too.
    for (client, prefs) in &mut ready {
        prefs
            .prevented
            .extend(crate::jobs::restricted_jobs(world, *client));
    }
    // The nuclear operatives and the wizard get no station job (tg `forced_occupations`).
    let invaders = |ruleset: &str| -> Vec<Entity> {
        antags
            .iter()
            .filter(|(_, r)| *r == ruleset)
            .map(|(c, _)| *c)
            .collect()
    };
    let operatives = invaders(crate::antag::dynamic::NUKEOPS_RULESET);
    let clowns = invaders(crate::antag::dynamic::CLOWNOPS_RULESET);
    let wizards = invaders(crate::antag::dynamic::WIZARD_RULESET);
    ready.retain(|(c, _)| !operatives.contains(c) && !clowns.contains(c) && !wizards.contains(c));
    // `COMSIG_GLOB_PRE_JOBS_ASSIGNED`: the job traits' sign-ups take their jobs first.
    let trait_jobs = crate::station_traits::pre_jobs_assigned(world, &mut ready);
    let prefs: Vec<&JobPreferences> = ready.iter().map(|(_, p)| p).collect();
    let assigned = divide_occupations(&mut world.resource_mut::<Jobs>(), &prefs);
    // `setup_officer_positions`: lockers for the officer slots the map lacks.
    crate::jobs::security::spawn_extra_lockers(world, SECURITY_MAX_POSITIONS);
    let mut spawned = Vec::new();
    for (client, job) in trait_jobs {
        let mob = crate::jobs::join(world, client, job, false);
        spawned.push((client, mob, job));
    }
    for ((client, _), assignment) in ready.iter().zip(assigned) {
        match assignment {
            Assignment::Job(job) => {
                let mob = crate::jobs::join(world, *client, job, false);
                spawned.push((*client, mob, job));
            }
            rejected => {
                if let Some(mut player) = world.get_mut::<NewPlayer>(*client) {
                    player.ready = false;
                }
                let text = if rejected == Assignment::NoPreferences {
                    "You have no jobs enabled, along with return to lobby if job is unavailable. \
                     This makes you ineligible for any round start role, please update your job \
                     preferences."
                } else {
                    "You have failed to qualify for any job you desired."
                };
                chat(world, vec![*client], Span::Danger, text);
            }
        }
    }
    crate::antag::nukeop::server::spawn_operatives(world, &operatives);
    crate::antag::nukeop::server::spawn_operatives_as(world, &clowns, true);
    crate::antag::wizard::server::spawn_wizards(world, &wizards);
    crate::antag::dynamic::execute_roundstart(world, &antags);
    // `equip_characters`: the spare ID's captain, announced `OnRoundstart`.
    crate::jobs::roundstart_captaincy(world, &spawned);
    let reopen = world.resource::<RoundConfig>().reopen_suicide_roles;
    let mut ticker = world.resource_mut::<Ticker>();
    ticker.state = GameState::Playing;
    ticker.round_start = now;
    // `PostSetup`: `addtimer(reopen_roundstart_suicide_roles, delay)`.
    ticker.reopen_at = reopen.map(|r| now + r.delay_secs);
    let station = station_name(world);
    let text = format!("Приветствуем вас на {station}. Приятного вам пребывания!");
    to_world(world, Span::BoldNotice, text);
    // "и..." and the holidays' greetings (tg sends them after the welcome sound).
    crate::events::holidays::greet(world);
    // `SEND_SOUND(world, SSstation.announcer.get_rand_welcome_sound())`.
    if let Some(welcome) = crate::announce::welcome_sound(world) {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::to_playing_players(welcome, 100.0, false),
        );
    }
    // `COMSIG_TICKER_ROUND_STARTING`, after the characters: the station traits' round start.
    crate::station_traits::on_round_start(world);
    // `PostSetup`: `queue_roundstart_report`.
    crate::announce::queue_roundstart_report(world);
    info!(
        "round started: {} players spawned",
        ready.len() + operatives.len()
    );
}

/// tg `TRAIT_SUICIDED` (`GLOB.suicided_mob_list`): this body's player killed it with the suicide
/// verb. The verb is `docs/parity/mobs/life-and-health.md`'s (not ported yet); it inserts this.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Suicided;

/// On a body whose job slot [`reopen_roundstart_suicide_roles`] freed: its `PlayerJob` no longer
/// holds a slot (`crate::jobs` does not free it a second time).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SlotReopened;

/// Jobs without `JOB_REOPEN_ON_ROUNDSTART_LOSS`: every other station job has it
/// (`STATION_JOB_FLAGS`).
const NO_REOPEN_JOBS: [&str; 2] = ["AI", "Cyborg"];

/// tg `reopen_roundstart_suicide_roles`: each suicided body's job gets its slot back (command jobs
/// only with [`ReopenSuicideRoles::command_positions`]), and with
/// [`ReopenSuicideRoles::command_report`] CentCom prints the list. Returns the reopened titles.
pub fn reopen_roundstart_suicide_roles(
    world: &mut World,
    config: ReopenSuicideRoles,
) -> Vec<String> {
    let mut quitters: Vec<(Entity, usize)> = world
        .query_filtered::<(Entity, &PlayerJob), (With<Suicided>, Without<SlotReopened>)>()
        .iter(world)
        .map(|(e, j)| (e, j.0))
        .collect();
    quitters.sort_by_key(|(e, _)| *e);
    let mut reopened = Vec::new();
    for (body, job) in quitters {
        let Some(mut jobs) = world.get_resource_mut::<Jobs>() else {
            return reopened;
        };
        let (title, command) = match jobs.data.jobs.get(job) {
            Some(j) => (j.title.clone(), j.command),
            None => continue,
        };
        if NO_REOPEN_JOBS.contains(&title.as_str()) || (command && !config.command_positions) {
            continue;
        }
        // `current_positions = max(current_positions - 1, 0)`.
        jobs.free_position(job);
        world.entity_mut(body).insert(SlotReopened);
        reopened.push(title);
    }
    if config.command_report && !reopened.is_empty() {
        let text = suicide_report(&station_name(world), &reopened);
        crate::announce::print_command_report(
            world,
            &text,
            Some(&format!(
                "{}: Кадровые изменения",
                crate::events::COMMAND_NAME
            )),
            true,
        );
    }
    if !reopened.is_empty() {
        info!("reopened roundstart suicide roles: {reopened:?}");
    }
    reopened
}

/// tg `reopen_roundstart_suicide_roles`' `suicide_command_report`.
pub fn suicide_report(station: &str, titles: &[String]) -> String {
    let command = crate::events::COMMAND_NAME;
    format!(
        "{command}: Отдел кадров\n\
         Уведомление о кадровых изменениях\n\n\
         Кадровому составу {station}:\n\n\
         Наш медицинский персонал зафиксировал ряд аномалий в показаниях жизненных датчиков \
         некоторых сотрудников вашей станции.\n\n\
         Дальнейшее расследование с нашей стороны выявило ряд весьма... неудачных решений, \
         принятых этими сотрудниками.\n\n\
         Поэтому мы взяли на себя смелость автоматически вновь открыть вакансии сотрудников, \
         решивших не участвовать в наших исследованиях. Их дела будут переданы комиссии по \
         найму для оценки пригодности к дальнейшей службе в компании (и, разумеется, \
         дальнейшего хранения записей клонирования на центральном медицинском сервере).\n\n\
         Следующие должности открыты повторно:\n\n{}",
        titles.join("\n")
    )
}

/// tg `declare_completion`: `personal_report` + `survivor_report` per client, into the chat.
fn declare_completion(world: &mut World, now: f64) {
    let station = station_name(world);
    let duration = display_time_text(world.resource::<Ticker>().round_duration(now));
    let evacuated = crate::shuttle::evacuated(world);
    let mut crew = world.query_filtered::<(Entity, Option<&Stat>), With<PlayerJob>>();
    let crew: Vec<(Entity, bool)> = crew
        .iter(world)
        .map(|(e, stat)| (e, stat != Some(&Stat::Dead)))
        .collect();
    let total = crew.len() as u32;
    let (mut survivors, mut escapees, mut shuttle_escapees) = (0u32, 0u32, 0u32);
    let mut escaped_names = Vec::new();
    for &(mob, alive) in &crew {
        if !alive {
            continue;
        }
        survivors += 1;
        let (escaped, on_shuttle) = crate::shuttle::escape_status(world, mob);
        escapees += u32::from(escaped);
        shuttle_escapees += u32::from(on_shuttle);
        if escaped {
            escaped_names.push(
                world
                    .get::<Name>(mob)
                    .map(|n| n.to_string())
                    .unwrap_or_default(),
            );
        }
    }
    let percent = |n: u32| (f64::from(n) / f64::from(total) * 1000.0).round() / 10.0;
    let mut lines = vec![
        report_line("Карта: ", station.clone()),
        report_line("Длительность смены: ", duration.clone()),
    ];
    if total > 0 {
        lines.push(report_line(
            "Общая численность экипажа: ",
            total.to_string(),
        ));
        if evacuated {
            lines.push(report_line(
                "Коэффициент эвакуированных: ",
                format!("{escapees} ({}%)", percent(escapees)),
            ));
            lines.push(report_line(
                "На эвакуационном шаттле: ",
                format!("{shuttle_escapees} ({}%)", percent(shuttle_escapees)),
            ));
        }
        lines.push(report_line(
            "Коэффициент выживаемости: ",
            format!("{survivors} ({}%)", percent(survivors)),
        ));
    }
    for client in all_clients(world) {
        let body = world.resource::<Players>().mob(client);
        // A ghost reports on the body it left.
        let body = body.and_then(|mob| match world.get::<crate::ghost::Ghost>(mob) {
            Some(ghost) => ghost.body,
            None => Some(mob),
        });
        let personal = body
            .filter(|&b| world.get::<PlayerJob>(b).is_some())
            .map(|b| {
                let name = world
                    .get::<Name>(b)
                    .map(|n| n.to_string())
                    .unwrap_or_default();
                match world.get::<Stat>(b) {
                    Some(Stat::Dead) => ChatMessage::simple(
                        ChatChannel::System,
                        Span::Danger,
                        format!("Вы не пережили события на {station}..."),
                    ),
                    // tg `personal_report`: alive but not on CentCom after the evacuation.
                    _ if evacuated && !crate::shuttle::escape_status(world, b).0 => {
                        ChatMessage::simple(
                            ChatChannel::System,
                            Span::Warning,
                            format!("Вам удалось выжить, но вы были брошены на {station}..."),
                        )
                    }
                    _ => ChatMessage::simple(
                        ChatChannel::System,
                        Span::Notice,
                        format!("Вам удалось пережить события на {station} как {name}."),
                    ),
                }
            });
        for message in personal.into_iter().chain(lines.iter().cloned()) {
            world.write_message(SendChat {
                clients: vec![client],
                message,
            });
        }
    }
    let end = crate::antag::objectives::RoundEnd {
        force_ending: world.resource::<Ticker>().force_ending,
    };
    let mut antags = crate::antag::traitor::roundend_report(world, end);
    antags.extend(crate::antag::brother::roundend_report(world, end));
    antags.extend(crate::antag::vampire::server::roundend_report(world, end));
    antags.extend(crate::antag::changeling::server::roundend_report(
        world, end,
    ));
    antags.extend(crate::antag::nukeop::server::roundend_report(world, end));
    antags.extend(crate::antag::nukeop::server::lone_roundend_report(
        world, end,
    ));
    antags.extend(crate::antag::nukeop::battlecruiser::roundend_report(
        world, end,
    ));
    antags.extend(crate::antag::cult::server::roundend_report(world, end));
    antags.extend(crate::antag::heretic::server::roundend_report(world, end));
    antags.extend(crate::antag::wizard::server::roundend_report(world, end));
    antags.extend(crate::antag::abductor::server::roundend_report(world, end));
    antags.extend(crate::antag::paradox_clone::roundend_report(world, end));
    antags.extend(crate::antag::nightmare::roundend_report(world, end));
    antags.extend(crate::antag::raiders::roundend_report(world, end));
    antags.extend(crate::antag::morph::roundend_report(world, end));
    antags.extend(crate::antag::xeno::roundend_report(world, end));
    antags.extend(crate::antag::space_dragon::roundend_report(world, end));
    antags.extend(crate::antag::revenant::roundend_report(world, end));
    antags.extend(crate::antag::slaughter::roundend_report(world, end));
    antags.extend(crate::antag::blob::roundend_report(world, end));
    antags.extend(crate::antag::brainwashed::roundend_report(world, end));
    antags.extend(crate::antag::valentine::roundend_report(world, end));
    antags.extend(crate::events::wizard_items::roundend_report(world));
    antags.extend(crate::antag::survivalist::roundend_report(world, end));
    antags.extend(crate::antag::ashwalker::roundend_report(world, end));
    antags.extend(crate::antag::voidwalker::roundend_report(world, end));
    antags.extend(crate::antag::spy::roundend_report(world, end));
    antags.extend(crate::antag::separatist::roundend_report(world, end));
    antags.extend(crate::antag::revolution::server::roundend_report(
        world, end,
    ));
    // `goal_report`, then `market_report`.
    antags.extend(crate::station_goals::goal_report(world));
    antags.extend(crate::cargo::economy::market_report(world));
    let clients = all_clients(world);
    for message in antags {
        world.write_message(SendChat {
            clients: clients.clone(),
            message,
        });
    }
    to_world(world, Span::BoldAnnounce, "Раунд закончился.");
    // `SSdynamic.set_round_result` + `send_news_report`.
    crate::round_news::send_news_report(world, end);
    // `C?.playtitlemusic(volume_multiplier = 0.5)`.
    crate::lobby_music::round_end_title_music(world);
    info!(
        "round ended after {duration}: {survivors}/{total} crew survived, {escapees} escaped \
         ({shuttle_escapees} on the shuttle): {escaped_names:?}"
    );
}

fn report_line(label: &str, value: String) -> ChatMessage {
    ChatMessage {
        channel: ChatChannel::System,
        segments: vec![
            ChatSegment::new(Span::Plain, format!("    {label}")),
            ChatSegment::new(Span::Bold, value),
        ],
    }
}

/// A round-start job assignment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Assignment {
    Job(usize),
    /// tg `try_reject_player`: no wanted job had room; back to not ready.
    Rejected,
    /// tg `check_job_preferences`: no job enabled and "return to lobby".
    NoPreferences,
}

/// BYOND `shuffle`.
pub(crate) fn shuffle<T>(items: &mut [T], rng: &mut ss13_mapload::spawner::Rng) {
    for i in (1..items.len()).rev() {
        let j = rng.range(0, i as i64) as usize;
        items.swap(i, j);
    }
}

/// tg `SSjob.divide_occupations` for ready players' preferences, BandaStation's version (job
/// priority staffing; no head-of-staff or AI forcing, no dynamic rulesets, popcaps, bans or
/// playtime here). Fills `jobs`' officer slots for the population (`setup_officer_positions`);
/// the slots themselves are taken when the players spawn.
pub fn divide_occupations(jobs: &mut Jobs, players: &[&JobPreferences]) -> Vec<Assignment> {
    let mut result = vec![Assignment::Rejected; players.len()];
    let mut unassigned: Vec<usize> = Vec::new();
    for (i, prefs) in players.iter().enumerate() {
        if prefs.jobless == JoblessRole::ReturnToLobby && prefs.levels.is_empty() {
            result[i] = Assignment::NoPreferences;
        } else {
            unassigned.push(i);
        }
    }
    if let Some(officer) = jobs.data.job(SECURITY_OFFICER) {
        let job = &mut jobs.data.jobs[officer];
        if job.spawn_positions > 0 {
            // DM's one-argument `round` is a floor.
            let n = (unassigned.len() as f64 / SECURITY_SCALING_COEFF).floor() as i32;
            let n = n.clamp(SECURITY_MIN_POSITIONS, SECURITY_MAX_POSITIONS);
            job.total_positions = n;
            job.spawn_positions = n;
        }
    }
    shuffle(&mut unassigned, &mut jobs.rng);
    let count = jobs.data.jobs.len();
    let mut current: Vec<u32> = (0..count).map(|j| jobs.positions(j)).collect();
    let spawn = |j: usize| jobs.data.jobs[j].spawn_positions;
    let full =
        |j: usize, current: &[u32]| spawn(j) != -1 && i64::from(current[j]) >= i64::from(spawn(j));
    let eligible = |p: usize, j: usize| {
        jobs.data.jobs[j].unresolved.is_empty() && !players[p].prevented.contains(&j)
    };
    let overflow = jobs.default_job;

    let mut assign = |p: usize, j: usize, current: &mut [u32], unassigned: &mut Vec<usize>| {
        result[p] = Assignment::Job(j);
        current[j] += 1;
        unassigned.retain(|&u| u != p);
    };

    // `assign_all_overflow_positions`: the overflow role at any level.
    for p in unassigned.clone() {
        if full(overflow, &current) {
            break;
        }
        if players[p].levels.contains_key(&overflow) && eligible(p, overflow) {
            assign(p, overflow, &mut current, &mut unassigned);
        }
    }

    // Standard assignment, highest preference level first, the most needed job first.
    let mut available: Vec<usize> = (0..count).filter(|&j| !full(j, &current)).collect();
    let rng = &mut jobs.rng;
    for level in [JobPriority::High, JobPriority::Medium, JobPriority::Low] {
        for p in unassigned.clone() {
            let mut possible: Vec<usize> = available
                .iter()
                .copied()
                .filter(|&j| players[p].levels.get(&j) == Some(&level) && eligible(p, j))
                .collect();
            if possible.is_empty() {
                continue;
            }
            sort_by_staffing_priority(&mut possible, &jobs.data, &current, rng);
            let picked = possible[0];
            assign(p, picked, &mut current, &mut unassigned);
            if full(picked, &current) {
                available.retain(|&j| j != picked);
            }
        }
    }

    // `handle_unassigned`: the jobless role decides.
    for p in unassigned.clone() {
        match players[p].jobless {
            JoblessRole::BeOverflow if !full(overflow, &current) && eligible(p, overflow) => {
                assign(p, overflow, &mut current, &mut unassigned);
            }
            JoblessRole::BeRandomJob => {
                // `give_priority_job`: the most needed open job, never the overflow or command.
                let mut all: Vec<usize> = (0..count).collect();
                sort_by_staffing_priority(&mut all, &jobs.data, &current, rng);
                let pick = all.into_iter().find(|&j| {
                    !full(j, &current)
                        && j != overflow
                        && !jobs.data.jobs[j].command
                        && eligible(p, j)
                });
                if let Some(j) = pick {
                    assign(p, j, &mut current, &mut unassigned);
                }
            }
            _ => {}
        }
    }
    result
}

/// BandaStation `shuffle_and_sort_jobs_by_staffing_priority`: shuffled, then stably sorted by
/// `cmp_job_staffing_priority` (priority, then the least filled share of spawn slots; jobs
/// without spawn slots last).
fn sort_by_staffing_priority(
    jobs: &mut [usize],
    data: &ss13_mapload::jobs::JobsData,
    current: &[u32],
    rng: &mut ss13_mapload::spawner::Rng,
) {
    shuffle(jobs, rng);
    jobs.sort_by(|&a, &b| {
        let (ja, jb) = (&data.jobs[a], &data.jobs[b]);
        ja.staffing_priority
            .cmp(&jb.staffing_priority)
            .then_with(
                || match (ja.spawn_positions <= 0, jb.spawn_positions <= 0) {
                    (true, true) => std::cmp::Ordering::Equal,
                    (true, false) => std::cmp::Ordering::Greater,
                    (false, true) => std::cmp::Ordering::Less,
                    (false, false) => {
                        let fa = f64::from(current[a]) / f64::from(ja.spawn_positions);
                        let fb = f64::from(current[b]) / f64::from(jb.spawn_positions);
                        fa.total_cmp(&fb)
                    }
                },
            )
    });
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LobbyAct {
    ToggleReady,
    Observe,
    CharacterSetup,
    ToggleAntagRoll,
    Manifest,
    /// A station trait's name.
    TraitSignup(String),
    ResetHud,
    /// Admin buttons, handled by `ss13_admin` (rights, log).
    StartNow,
    Delay,
    Notice,
}

impl ServerUi for LobbyUi {
    type Source = (
        &'static NewPlayer,
        &'static CharacterName,
        &'static AntagPreferences,
        Has<LobbyAdmin>,
    );
    type UserParam = (
        Res<'static, Ticker>,
        Res<'static, RoundConfig>,
        Res<'static, LobbyNotice>,
        Option<Res<'static, crate::station_traits::StationTraits>>,
        Option<Res<'static, crate::station_traits::LobbyCandidates>>,
    );
    type Act = LobbyAct;

    fn static_data(_: (&NewPlayer, &CharacterName, &AntagPreferences, bool)) {}

    fn data(
        (player, name, antag, admin): (&NewPlayer, &CharacterName, &AntagPreferences, bool),
        client: Entity,
        (ticker, config, notice, traits, candidates): &(
            Res<Ticker>,
            Res<RoundConfig>,
            Res<LobbyNotice>,
            Option<Res<crate::station_traits::StationTraits>>,
            Option<Res<crate::station_traits::LobbyCandidates>>,
        ),
    ) -> LobbyData {
        // tg `can_display_lobby_button`: `sign_up_button && !SSticker.HasRoundStarted()`.
        let traits = match traits {
            Some(traits) if ticker.state < GameState::SettingUp => traits
                .active
                .iter()
                .filter_map(|t| {
                    let desc = crate::station_traits::lobby_button_desc(t.path)?;
                    Some(LobbyTrait {
                        name: t.name.to_owned(),
                        desc: desc.to_owned(),
                        signed_up: candidates
                            .as_ref()
                            .is_some_and(|c| c.signed_up(t.path, client)),
                    })
                })
                .collect(),
            _ => Vec::new(),
        };
        LobbyData {
            state: ticker.state,
            time_left: (ticker.state == GameState::Pregame)
                .then(|| ticker.time_left.max(0.0).ceil() as u32),
            ready: player.ready,
            name: name.0.clone(),
            players: ticker.total_players,
            ready_players: ticker.ready_players,
            in_lobby: ticker.in_lobby,
            antag_roll: antag.antag_roll,
            source_url: config.source_url.clone(),
            notice: notice.0.clone(),
            admin,
            traits,
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<LobbyAct, UiReject> {
        match action {
            TOGGLE_READY => Ok(LobbyAct::ToggleReady),
            OBSERVE => Ok(LobbyAct::Observe),
            CHARACTER_SETUP => Ok(LobbyAct::CharacterSetup),
            TOGGLE_ANTAG_ROLL => Ok(LobbyAct::ToggleAntagRoll),
            MANIFEST => Ok(LobbyAct::Manifest),
            TRAIT_SIGNUP => Ok(LobbyAct::TraitSignup(params.text(TRAIT_PARAM)?)),
            RESET_HUD => Ok(LobbyAct::ResetHud),
            START_NOW => Ok(LobbyAct::StartNow),
            DELAY => Ok(LobbyAct::Delay),
            NOTICE => Ok(LobbyAct::Notice),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PrefAct {
    Job(String, Option<JobPriority>),
    Jobless(JoblessRole),
    /// tg `set_antags`: one antagonist role on or off.
    Antag(String, bool),
    /// `set_appearance`: one field; the index is checked against its table when applied.
    Appearance(AppearanceField),
    RandomAppearance,
    /// tg `give_quirk` (true) / `remove_quirk` (false) by name.
    Quirk(String, bool),
    /// tg `handle_personality` by savefile key.
    Personality(String),
    /// tg `clear_personalities`.
    ClearPersonalities,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppearanceField {
    Species(usize),
    MutantColor(usize),
    Female(bool),
    SkinTone(usize),
    HairStyle(Option<usize>),
    HairColor(usize),
    EyeColor(usize),
}

impl ServerUi for JobPrefsUi {
    /// `NewPlayer`: the window closes on joining the round, as tg's closes with the new player
    /// mob (`SStgui.close_user_uis`).
    type Source = (
        &'static JobPreferences,
        &'static PrefsBoard,
        &'static AntagPreferences,
        &'static AppearancePrefs,
        &'static NewPlayer,
        &'static QuirkPrefs,
        &'static PersonalityPrefs,
    );
    type UserParam = Option<Res<'static, Jobs>>;
    type Act = PrefAct;

    fn static_data(
        (_, board, ..): (
            &JobPreferences,
            &PrefsBoard,
            &AntagPreferences,
            &AppearancePrefs,
            &NewPlayer,
            &QuirkPrefs,
            &PersonalityPrefs,
        ),
    ) -> JobPrefsStatic {
        (*board.0).clone()
    }

    fn data(
        (prefs, _, antag, look, _, quirks, personalities): (
            &JobPreferences,
            &PrefsBoard,
            &AntagPreferences,
            &AppearancePrefs,
            &NewPlayer,
            &QuirkPrefs,
            &PersonalityPrefs,
        ),
        _: Entity,
        jobs: &Option<Res<Jobs>>,
    ) -> JobPrefsData {
        let mut levels: Vec<(String, JobPriority)> = jobs
            .as_ref()
            .map(|jobs| {
                prefs
                    .levels
                    .iter()
                    .filter_map(|(&j, &l)| Some((jobs.data.jobs.get(j)?.title.clone(), l)))
                    .collect()
            })
            .unwrap_or_default();
        levels.sort();
        JobPrefsData {
            levels,
            jobless_role: prefs.jobless,
            be_special: antag.be_special.iter().cloned().collect(),
            appearance: AppearanceData {
                species: look.species as u8,
                mutant_color: look.mutant_color as u8,
                female: look.female,
                skin_tone: look.skin_tone as u8,
                hair_style: look.hair_style.map(|h| h as u8),
                hair_color: look.hair_color as u8,
                eye_color: look.eye_color as u8,
            },
            quirks: quirks.0.clone(),
            quirk_balance: quirks.balance(),
            personalities: personalities.0.clone(),
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<PrefAct, UiReject> {
        let bad = |what: &str| UiReject::BadParams(what.to_owned());
        match action {
            SET_JOB_PREFERENCE => {
                let job = params.get("job").ok_or_else(|| bad("job: missing"))?;
                let level = JobPriority::from_param(params.get("level").unwrap_or(""))
                    .ok_or_else(|| bad("level: expected 1, 2, 3 or empty"))?;
                Ok(PrefAct::Job(job.to_owned(), level))
            }
            SET_PREFERENCE if params.get("preference") == Some(JOBLESS_ROLE) => params
                .get("value")
                .and_then(JoblessRole::from_param)
                .map(PrefAct::Jobless)
                .ok_or_else(|| bad("value: expected 1, 2 or 3")),
            SET_ANTAGS => {
                let antag = params.get("antags").ok_or_else(|| bad("antags: missing"))?;
                let toggled = match params.get("toggled") {
                    Some("1") => true,
                    Some("0") => false,
                    _ => return Err(bad("toggled: expected 1 or 0")),
                };
                Ok(PrefAct::Antag(antag.to_owned(), toggled))
            }
            SET_APPEARANCE => {
                let value = params.get("value").unwrap_or("");
                let index = || {
                    value
                        .parse::<usize>()
                        .map_err(|_| bad("value: expected an index"))
                };
                let field = match params.get("key") {
                    Some(APPEARANCE_FEMALE) => AppearanceField::Female(value == "1"),
                    Some(APPEARANCE_SPECIES) => AppearanceField::Species(index()?),
                    Some(APPEARANCE_MUTANT_COLOR) => AppearanceField::MutantColor(index()?),
                    Some(APPEARANCE_SKIN_TONE) => AppearanceField::SkinTone(index()?),
                    Some(APPEARANCE_HAIR_STYLE) if value.is_empty() => {
                        AppearanceField::HairStyle(None)
                    }
                    Some(APPEARANCE_HAIR_STYLE) => AppearanceField::HairStyle(Some(index()?)),
                    Some(APPEARANCE_HAIR_COLOR) => AppearanceField::HairColor(index()?),
                    Some(APPEARANCE_EYE_COLOR) => AppearanceField::EyeColor(index()?),
                    _ => return Err(bad("key: unknown appearance field")),
                };
                Ok(PrefAct::Appearance(field))
            }
            RANDOMIZE_APPEARANCE => Ok(PrefAct::RandomAppearance),
            GIVE_QUIRK | REMOVE_QUIRK => {
                let quirk = params.get("quirk").ok_or_else(|| bad("quirk: missing"))?;
                Ok(PrefAct::Quirk(quirk.to_owned(), action == GIVE_QUIRK))
            }
            HANDLE_PERSONALITY => params
                .get("personality_type")
                .map(|key| PrefAct::Personality(key.to_owned()))
                .ok_or_else(|| bad("personality_type: missing")),
            CLEAR_PERSONALITIES => Ok(PrefAct::ClearPersonalities),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

/// tg `abandon_mob` -> `new /mob/dead/new_player`: the client is in the lobby again, its
/// preferences kept (made fresh when it never was in the lobby: a skip-lobby client).
pub(crate) fn return_to_lobby(world: &mut World, client: Entity) {
    if world.get_entity(client).is_err() {
        return;
    }
    character_name(world, client);
    let board = world
        .get_resource::<PrefsStatic>()
        .map(|p| PrefsBoard(p.0.clone()));
    let mut entity = world.entity_mut(client);
    if let Some(board) = board {
        entity.insert(board);
    }
    if !entity.contains::<JobPreferences>() {
        entity.insert((
            JobPreferences::default(),
            AntagPreferences::default(),
            AppearancePrefs::random(client.to_bits()),
            QuirkPrefs::default(),
            PersonalityPrefs::default(),
        ));
    }
    entity.insert((NewPlayer::default(), AlwaysState));
    world.write_message(OpenUi::new::<LobbyUi>(client, client, client, ""));
}

/// BandaStation `new_player/Topic`: ready, observe, character setup; checked when applied.
fn lobby_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<LobbyUi>>>) {
    let acted: Vec<(Entity, LobbyAct)> = acts
        .read(world.resource::<Messages<UiActed<LobbyUi>>>())
        .map(|a| (a.client, a.act.clone()))
        .collect();
    for (client, act) in acted {
        let Some(player) = world.get::<NewPlayer>(client).copied() else {
            continue;
        };
        let state = world.resource::<Ticker>().state;
        match act {
            LobbyAct::ToggleReady => {
                if state >= GameState::SettingUp {
                    chat(world, vec![client], Span::Warning, "Игра уже начинается!");
                    continue;
                }
                let has_prefs = world
                    .get::<JobPreferences>(client)
                    .is_some_and(|p| !p.levels.is_empty());
                if !player.ready && !has_prefs {
                    chat(
                        world,
                        vec![client],
                        Span::BoldWarning,
                        "Ошибка настроек персонажа. Выберите предпочитаемую должность.",
                    );
                    continue;
                }
                // BandaStation: a wanted job the chosen species may not take.
                if !player.ready && crate::jobs::prefs_restricted(world, client) {
                    let text = crate::jobs::SPECIES_INCOMPATIBLE;
                    chat(world, vec![client], Span::Warning, text);
                    continue;
                }
                world.entity_mut(client).insert(NewPlayer {
                    ready: !player.ready,
                });
            }
            LobbyAct::Observe => {
                if state <= GameState::Startup {
                    chat(
                        world,
                        vec![client],
                        Span::Warning,
                        "Сервер ещё не загрузился!",
                    );
                    continue;
                }
                make_observer(world, client);
            }
            // BandaStation `toggle_antag_roll`: any time in the lobby.
            LobbyAct::ToggleAntagRoll => {
                if let Some(mut prefs) = world.get_mut::<AntagPreferences>(client) {
                    prefs.antag_roll = !prefs.antag_roll;
                }
            }
            LobbyAct::CharacterSetup => {
                world.write_message(OpenUi::new::<JobPrefsUi>(
                    client,
                    client,
                    client,
                    "Настройка персонажа",
                ));
            }
            // tg `ViewManifest`: `GLOB.manifest.ui_interact(new_player)`.
            LobbyAct::Manifest => {
                world.write_message(OpenUi::new::<CrewManifestUi>(
                    client,
                    client,
                    client,
                    "Манифест экипажа",
                ));
            }
            LobbyAct::TraitSignup(name) => trait_signup(world, client, &name),
            LobbyAct::ResetHud => reset_hud(world, client),
            LobbyAct::StartNow => {
                world.write_message(LobbyAdminAct {
                    client,
                    request: LobbyAdminRequest::StartNow,
                });
            }
            LobbyAct::Delay => {
                world.write_message(LobbyAdminAct {
                    client,
                    request: LobbyAdminRequest::Delay,
                });
            }
            LobbyAct::Notice if world.get::<LobbyAdmin>(client).is_some() => {
                use ss13_tgui::prompt::{Ask, PromptKind, ask_world};
                let prompt = ask_world(
                    world,
                    Ask::new(
                        client,
                        client,
                        "Уведомление в лобби",
                        "Введи то что должно отображаться в лобби:",
                        PromptKind::Text {
                            default: String::new(),
                            max_length: NOTICE_MAX_LEN,
                            multiline: false,
                        },
                    ),
                );
                world.entity_mut(prompt).insert(NoticePrompt);
            }
            LobbyAct::Notice => {
                world.write_message(LobbyAdminAct {
                    client,
                    request: LobbyAdminRequest::Notice(None),
                });
            }
        }
    }
}

/// The typed notices, passed on to `ss13_admin` (which checks the rights again).
fn notice_answers(
    world: &mut World,
    mut cursor: Local<MessageCursor<ss13_tgui::prompt::PromptAnswered>>,
) {
    let answers: Vec<(Entity, Entity, Option<ss13_tgui::prompt::PromptAnswer>)> = cursor
        .read(world.resource::<Messages<ss13_tgui::prompt::PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.answer.clone()))
        .collect();
    for (prompt, client, answer) in answers {
        if world.get::<NoticePrompt>(prompt).is_none() {
            continue;
        }
        // tg `isnull(new_notice)`: a cancel changes nothing.
        let Some(ss13_tgui::prompt::PromptAnswer::Text(text)) = answer else {
            continue;
        };
        world.write_message(LobbyAdminAct {
            client,
            request: LobbyAdminRequest::Notice(Some(text)),
        });
    }
}

/// BandaStation `/datum/station_trait/job/on_lobby_button_click`: in or out of the trait's
/// candidates, before the round starts.
fn trait_signup(world: &mut World, client: Entity, name: &str) {
    let Some(path) = world
        .get_resource::<crate::station_traits::StationTraits>()
        .and_then(|t| t.active.iter().find(|t| t.name == name))
        .map(|t| t.path)
        .filter(|path| crate::station_traits::lobby_button_desc(path).is_some())
    else {
        return;
    };
    if world.resource::<Ticker>().state >= GameState::SettingUp {
        chat(world, vec![client], Span::RedText, "Раунд уже начался!");
        return;
    }
    let signed_up = world
        .get_resource_or_init::<crate::station_traits::LobbyCandidates>()
        .toggle(path, client);
    let (span, text) = if signed_up {
        (
            Span::GreenText,
            format!("Вы были добавлены в список кандидатов на роль {name}."),
        )
    } else {
        (
            Span::RedText,
            format!("Вы были убраны из списка кандидатов на роль {name}."),
        )
    };
    chat(world, vec![client], span, text);
}

/// tg `reset_menu_hud`: the lobby window is sent again (the client redraws its screen from
/// scratch), once per 15 seconds.
fn reset_hud(world: &mut World, client: Entity) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    if let Some(&ResetHudCooldown(at)) = world.get::<ResetHudCooldown>(client)
        && now < at
    {
        let text = format!(
            "Вы должны подождать {} перед повторным сбросом HUD лобби!",
            display_time_text(at - now)
        );
        chat(world, vec![client], Span::Warning, text);
        return;
    }
    world
        .entity_mut(client)
        .insert(ResetHudCooldown(now + RESET_HUD_SECS));
    world.write_message(OpenUi::new::<LobbyUi>(client, client, client, ""));
    let text = format!(
        "HUD лобби сброшен. Повторно сбросить его можно через {}.",
        display_time_text(RESET_HUD_SECS)
    );
    chat(world, vec![client], Span::Info, text);
}

impl ServerUi for CrewManifestUi {
    /// tg `ui_status`: interactive for new players (observers have their own verb), else closed.
    type Source = &'static NewPlayer;
    type UserParam = (
        Option<Res<'static, crate::records::manifest::Manifest>>,
        Option<Res<'static, Jobs>>,
    );
    type Act = ();

    fn static_data(_: &NewPlayer) {}

    fn data(
        _: &NewPlayer,
        _: Entity,
        (manifest, jobs): &(
            Option<Res<crate::records::manifest::Manifest>>,
            Option<Res<Jobs>>,
        ),
    ) -> CrewManifestData {
        let Some(jobs) = jobs else {
            return CrewManifestData::default();
        };
        crew_manifest(manifest.as_deref(), jobs)
    }
}

/// `/datum/manifest/ui_data`: the records by department, and per joinable department the open
/// slots and the unlimited jobs.
pub fn crew_manifest(
    manifest: Option<&crate::records::manifest::Manifest>,
    jobs: &Jobs,
) -> CrewManifestData {
    let positions = jobs
        .data
        .departments
        .iter()
        .map(|d| {
            let mut open = 0;
            let mut exceptions = Vec::new();
            for &j in &d.jobs {
                let job = &jobs.data.jobs[usize::from(j)];
                if job.total_positions < 0 {
                    exceptions.push(job.title.clone());
                    continue;
                }
                open += (job.total_positions as u32).saturating_sub(jobs.positions(usize::from(j)));
            }
            DepartmentPositions {
                name: d.name.clone(),
                open,
                exceptions,
                color: d.color.clone(),
            }
        })
        .collect();
    CrewManifestData {
        manifest: manifest
            .map(|m| m.departments(&jobs.data))
            .unwrap_or_default(),
        positions,
    }
}

/// tg `make_me_an_observer` (the confirmation is asked by the client).
fn make_observer(world: &mut World, client: Entity) {
    let spawns = world.resource::<SpawnPoints>();
    let pos: TilePos = crate::server::dev_spawn(world, client)
        .or(spawns.forced)
        .or(spawns.observer)
        .unwrap_or_else(|| spawns.tiles[0]);
    let name = character_name(world, client);
    world.entity_mut(client).remove::<NewPlayer>();
    chat(world, vec![client], Span::Notice, "Now teleporting.");
    let ghost = spawn_ghost(
        world,
        client,
        None,
        pos,
        Facing::default(),
        Name::new(name),
        MoveQueue::default(),
    );
    info!("client {client} observes as {ghost} at {pos:?}");
}

/// The jobs page's acts (tg `preference_middleware/jobs`).
fn preference_acts(
    mut acts: MessageReader<UiActed<JobPrefsUi>>,
    jobs: Option<Res<Jobs>>,
    mut prefs: Query<(
        &mut JobPreferences,
        &mut AntagPreferences,
        &mut AppearancePrefs,
        &mut QuirkPrefs,
        &mut PersonalityPrefs,
    )>,
    mut randomized: Local<u64>,
) {
    let Some(jobs) = jobs else {
        return;
    };
    for act in acts.read() {
        let Ok((mut prefs, mut antag, mut look, mut quirks, mut personalities)) =
            prefs.get_mut(act.client)
        else {
            continue;
        };
        match &act.act {
            PrefAct::Appearance(field) => {
                let mut new = *look;
                match *field {
                    AppearanceField::Female(female) => new.female = female,
                    AppearanceField::Species(i) => new.species = i,
                    AppearanceField::MutantColor(i) => new.mutant_color = i,
                    AppearanceField::SkinTone(i) => new.skin_tone = i,
                    AppearanceField::HairStyle(i) => new.hair_style = i,
                    AppearanceField::HairColor(i) => new.hair_color = i,
                    AppearanceField::EyeColor(i) => new.eye_color = i,
                }
                // An index outside its table is refused.
                if new.is_valid() {
                    *look = new;
                }
            }
            // `change_prefs` refuses unknown names and any set that is not valid as a whole.
            PrefAct::Quirk(name, give) => {
                let mut new = (*quirks).clone();
                if crate::quirks::change_prefs(&mut new, name, *give) {
                    *quirks = new;
                }
            }
            // `toggle_prefs` refuses unknown keys, a full set and incompatible picks.
            PrefAct::Personality(key) => {
                let mut new = (*personalities).clone();
                if crate::personality::toggle_prefs(&mut new, key) {
                    personalities.set_if_neq(new);
                }
            }
            PrefAct::ClearPersonalities => {
                personalities.set_if_neq(PersonalityPrefs::default());
            }
            PrefAct::RandomAppearance => {
                *randomized += 1;
                *look = AppearancePrefs::random(act.client.to_bits() ^ (*randomized << 32));
            }
            PrefAct::Job(title, level) => {
                // tg: an unknown job or one of another faction is refused.
                let Some(job) = jobs.data.job(title) else {
                    continue;
                };
                if !jobs.data.jobs[job].unresolved.is_empty() {
                    continue;
                }
                prefs.set(job, *level, jobs.default_job);
            }
            PrefAct::Jobless(role) => prefs.jobless = *role,
            // tg checks the role against the antagonists the menu knows.
            PrefAct::Antag(role, on)
                if [
                    crate::antag::ROLE_TRAITOR,
                    crate::antag::ROLE_BROTHER,
                    crate::antag::vampire::ROLE_VAMPIRE,
                    crate::antag::changeling::ROLE_CHANGELING,
                    crate::antag::nukeop::ROLE_OPERATIVE,
                    crate::antag::cult::ROLE_CULTIST,
                    crate::antag::heretic::ROLE_HERETIC,
                    crate::antag::ROLE_SYNDICATE_INFILTRATOR,
                    crate::antag::ROLE_SLEEPER_AGENT,
                    crate::antag::ROLE_STOWAWAY_CHANGELING,
                    crate::antag::ROLE_OPERATIVE_MIDROUND,
                    crate::antag::wizard::ROLE_WIZARD,
                    crate::antag::wizard::ROLE_WIZARD_MIDROUND,
                    crate::antag::revolution::ROLE_REV_HEAD,
                    crate::antag::revolution::ROLE_PROVOCATEUR,
                    crate::antag::ROLE_MALF,
                    crate::antag::ROLE_PARADOX_CLONE,
                    crate::antag::ROLE_NIGHTMARE,
                    crate::antag::ROLE_SPACE_DRAGON,
                    crate::antag::ROLE_REVENANT,
                    crate::antag::ROLE_BLOB,
                    crate::antag::ROLE_BLOB_INFECTION,
                    crate::antag::ROLE_ALIEN,
                    crate::antag::ROLE_NINJA,
                    crate::antag::ROLE_FUGITIVE,
                    crate::antag::changeling::ROLE_CHANGELING_MIDROUND,
                    crate::antag::ROLE_SPY,
                    crate::antag::ROLE_VOIDWALKER,
                    crate::antag::ROLE_BLOOD_WORM_INFESTATION,
                ]
                .contains(&role.as_str()) =>
            {
                if *on {
                    antag.be_special.insert(role.clone());
                } else {
                    antag.be_special.remove(role);
                }
            }
            PrefAct::Antag(..) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use ss13_mapload::jobs::{Department, JobsData};

    use super::*;

    #[test]
    fn start_now_toggles_and_is_refused_after_the_lobby() {
        let mut ticker = Ticker {
            state: GameState::Pregame,
            time_left: 10_800.0,
            ..default()
        };
        assert_eq!(ticker.start_now(), Ok(true));
        assert!(ticker.start_immediately);
        // Pressed again before the round starts: cancelled, 3 minutes left (tg).
        assert_eq!(ticker.start_now(), Ok(false));
        assert!(!ticker.start_immediately);
        assert_eq!(ticker.time_left, START_NOW_CANCEL_SECS);
        ticker.state = GameState::Startup;
        assert_eq!(ticker.start_now(), Ok(true));
        ticker.state = GameState::Playing;
        assert!(ticker.start_now().is_err());
    }

    #[test]
    fn the_lobby_manifest_counts_open_slots_and_unlimited_jobs() {
        let mut d = data();
        d.departments
            .last_mut()
            .unwrap()
            .jobs
            .push(ASSISTANT as u16);
        let mut jobs = Jobs::new(d, 1);
        jobs.add_position(OFFICER);
        let mut manifest = crate::records::manifest::Manifest::default();
        manifest.inject(crate::records::manifest::Joiner {
            name: "Боб",
            assignment: SECURITY_OFFICER,
            job: Some(OFFICER),
            mob: Entity::PLACEHOLDER,
        });
        let m = crew_manifest(Some(&manifest), &jobs);
        let station = m.positions.iter().find(|p| p.name == "Station").unwrap();
        // Captain 1 + officers 5 - 1 + engineers 2 + clown 1 + AI 1; the assistant is unlimited.
        assert_eq!(station.open, 9);
        assert_eq!(station.exceptions, vec!["Assistant".to_owned()]);
        let dept = m.manifest.iter().find(|d| d.name == "Station").unwrap();
        assert_eq!(
            dept.entries,
            vec![("Боб".to_owned(), SECURITY_OFFICER.to_owned())]
        );
        assert!(crew_manifest(None, &jobs).manifest.is_empty());
    }

    fn lobby_world() -> World {
        let mut world = World::new();
        world.init_resource::<Messages<SendChat>>();
        world.init_resource::<Messages<OpenUi>>();
        world.init_resource::<LobbyNotice>();
        world.insert_resource(Time::<()>::default());
        world.insert_resource(Ticker {
            state: GameState::Pregame,
            time_left: 120.0,
            ..default()
        });
        world
    }

    #[test]
    fn delay_toggles_between_ninety_seconds_and_forever_until_the_round_starts() {
        let mut world = lobby_world();
        world.resource_mut::<Ticker>().start_immediately = true;
        assert_eq!(delay_start(&mut world), Ok(DELAY_FOREVER_SECS));
        assert!(!world.resource::<Ticker>().start_immediately);
        assert_eq!(world.resource::<Ticker>().time_left, DELAY_FOREVER_SECS);
        assert_eq!(delay_start(&mut world), Ok(DELAY_SECS));
        assert_eq!(world.resource::<Ticker>().time_left, DELAY_SECS);
        world.resource_mut::<Ticker>().state = GameState::SettingUp;
        assert!(delay_start(&mut world).is_err());
        assert_eq!(world.resource::<Ticker>().time_left, DELAY_SECS);
    }

    #[test]
    fn the_notice_is_set_capped_announced_to_the_lobby_and_removed_by_empty_text() {
        let mut world = lobby_world();
        let player = world.spawn(NewPlayer::default()).id();
        set_notice(&mut world, &"а".repeat(3000));
        assert_eq!(
            world
                .resource::<LobbyNotice>()
                .0
                .as_ref()
                .map(|n| n.chars().count()),
            Some(NOTICE_MAX_LEN as usize)
        );
        let sent: Vec<_> = world
            .resource::<Messages<SendChat>>()
            .iter_current_update_messages()
            .map(|m| m.clients.clone())
            .collect();
        assert_eq!(sent, vec![vec![player]]);
        set_notice(&mut world, "  ");
        assert_eq!(world.resource::<LobbyNotice>().0, None);
    }

    #[test]
    fn the_lobby_hud_resets_once_per_fifteen_seconds() {
        let mut world = lobby_world();
        let client = world.spawn(NewPlayer::default()).id();
        let opens = |world: &World| {
            world
                .resource::<Messages<OpenUi>>()
                .iter_current_update_messages()
                .filter(|o| {
                    o.interface == <LobbyUi as ss13_tgui::UiInterface>::NAME && o.client == client
                })
                .count()
        };
        reset_hud(&mut world, client);
        assert_eq!(opens(&world), 1);
        reset_hud(&mut world, client);
        assert_eq!(opens(&world), 1, "on cooldown");
        world
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_secs_f64(RESET_HUD_SECS));
        reset_hud(&mut world, client);
        assert_eq!(opens(&world), 2);
    }

    #[test]
    fn trait_signup_toggles_candidates_of_lobby_traits_only_before_the_round() {
        static GORILLA: crate::station_traits::TraitDef = crate::station_traits::TraitDef {
            path: "/datum/station_trait/job/cargorilla",
            name: "Cargo Gorilla",
            kind: crate::station_traits::TraitKind::Neutral,
            weight: 1,
            cost: 0.0,
            show_in_report: false,
            report: "",
            blacklist: &[],
        };
        let mut world = lobby_world();
        world.insert_resource(crate::station_traits::StationTraits {
            active: vec![&GORILLA],
        });
        let client = world.spawn(NewPlayer::default()).id();
        let signed = |world: &World| {
            world
                .get_resource::<crate::station_traits::LobbyCandidates>()
                .is_some_and(|c| c.signed_up(GORILLA.path, client))
        };
        trait_signup(&mut world, client, "Nope");
        assert!(!signed(&world));
        trait_signup(&mut world, client, "Cargo Gorilla");
        assert!(signed(&world));
        trait_signup(&mut world, client, "Cargo Gorilla");
        assert!(!signed(&world));
        world.resource_mut::<Ticker>().state = GameState::Playing;
        trait_signup(&mut world, client, "Cargo Gorilla");
        assert!(!signed(&world));
    }

    const ASSISTANT: usize = 0;
    const CAPTAIN: usize = 1;
    const OFFICER: usize = 2;
    const ENGINEER: usize = 3;
    const CLOWN: usize = 4;
    const AI: usize = 5;

    /// Assistant (overflow, unlimited), Captain (1, critical, command), Security Officer (5,
    /// moderate), Station Engineer (2, moderate), Clown (1, minor), AI (unplayable).
    fn data() -> JobsData {
        let mut data = crate::jobs::fallback_jobs();
        let base = data.jobs[0].clone();
        let job = |title: &str, slots: i32, priority: u8, command: bool| {
            let mut j = base.clone();
            j.title = title.into();
            j.total_positions = slots;
            j.spawn_positions = slots;
            j.staffing_priority = priority;
            j.command = command;
            j
        };
        data.jobs.push(job("Captain", 1, 1, true));
        data.jobs.push(job(SECURITY_OFFICER, 5, 4, false));
        data.jobs.push(job("Station Engineer", 2, 4, false));
        data.jobs.push(job("Clown", 1, 5, false));
        let mut ai = job("AI", 1, 2, false);
        ai.unresolved = vec!["no outfit".into()];
        data.jobs.push(ai);
        data.departments.push(Department {
            name: "Station".into(),
            jobs: vec![1, 2, 3, 4, 5],
            ..default()
        });
        data
    }

    fn prefs(levels: &[(usize, JobPriority)], jobless: JoblessRole) -> JobPreferences {
        JobPreferences {
            levels: levels.iter().copied().collect(),
            jobless,
            prevented: Vec::new(),
        }
    }

    fn divide(jobs: &mut Jobs, players: &[JobPreferences]) -> Vec<Assignment> {
        let refs: Vec<&JobPreferences> = players.iter().collect();
        divide_occupations(jobs, &refs)
    }

    use JobPriority::{High, Low, Medium};

    #[test]
    fn suicides_reopen_station_slots_once_and_command_slots_only_when_configured() {
        let mut world = World::new();
        let mut jobs = Jobs::new(data(), 1);
        for job in [CAPTAIN, ENGINEER, ENGINEER, CLOWN] {
            jobs.add_position(job);
        }
        world.insert_resource(jobs);
        let engineer = world.spawn((PlayerJob(ENGINEER), Suicided)).id();
        world.spawn(PlayerJob(ENGINEER));
        world.spawn((PlayerJob(CAPTAIN), Suicided));
        world.spawn(PlayerJob(CLOWN));
        let mut config = ReopenSuicideRoles {
            delay_secs: REOPEN_DEFAULT_SECS,
            command_positions: false,
            command_report: false,
        };
        let reopened = reopen_roundstart_suicide_roles(&mut world, config);
        assert_eq!(reopened, vec!["Station Engineer".to_owned()]);
        let jobs = world.resource::<Jobs>();
        assert_eq!(jobs.positions(ENGINEER), 1);
        assert_eq!(jobs.positions(CAPTAIN), 1, "command stays filled");
        assert_eq!(jobs.positions(CLOWN), 1);
        assert!(world.get::<SlotReopened>(engineer).is_some());
        // With command positions: the captain reopens, the engineer is not freed twice.
        config.command_positions = true;
        let reopened = reopen_roundstart_suicide_roles(&mut world, config);
        assert_eq!(reopened, vec!["Captain".to_owned()]);
        let jobs = world.resource::<Jobs>();
        assert_eq!(jobs.positions(CAPTAIN), 0);
        assert_eq!(jobs.positions(ENGINEER), 1);
    }

    #[test]
    fn the_suicide_report_lists_the_reopened_jobs() {
        let text = suicide_report("Box", &["Clown".into(), "Station Engineer".into()]);
        assert!(text.contains("Кадровому составу Box"));
        assert!(text.ends_with("Clown\nStation Engineer"));
    }

    #[test]
    fn slots_cap_jobs_and_the_rest_overflow_to_assistant() {
        for seed in 0..20 {
            let mut jobs = Jobs::new(data(), seed);
            let players = vec![prefs(&[(CLOWN, High)], JoblessRole::BeOverflow); 3];
            let got = divide(&mut jobs, &players);
            let clowns = got.iter().filter(|a| **a == Assignment::Job(CLOWN)).count();
            let assistants = got
                .iter()
                .filter(|a| **a == Assignment::Job(ASSISTANT))
                .count();
            assert_eq!((clowns, assistants), (1, 2), "seed {seed}: {got:?}");
        }
    }

    #[test]
    fn higher_levels_go_first_and_needed_jobs_before_others() {
        for seed in 0..20 {
            let mut jobs = Jobs::new(data(), seed);
            // Both want the captain; one at high, one at medium (with the clown at low).
            let players = vec![
                prefs(&[(CLOWN, Low), (CAPTAIN, Medium)], JoblessRole::BeOverflow),
                prefs(&[(CAPTAIN, High)], JoblessRole::BeOverflow),
            ];
            let got = divide(&mut jobs, &players);
            assert_eq!(
                got,
                [Assignment::Job(CLOWN), Assignment::Job(CAPTAIN)],
                "seed {seed}"
            );
            // Two jobs at the same level: the critical one is filled first.
            let mut jobs = Jobs::new(data(), seed);
            let players = vec![prefs(
                &[(ENGINEER, Medium), (CAPTAIN, Medium)],
                JoblessRole::BeOverflow,
            )];
            assert_eq!(divide(&mut jobs, &players), [Assignment::Job(CAPTAIN)]);
        }
    }

    #[test]
    fn same_priority_fills_the_emptier_job() {
        // An engineer is already on the station (1 of 2); officers have 0 of 5.
        let mut jobs = Jobs::new(data(), 3);
        jobs.add_position(ENGINEER);
        let players = vec![prefs(
            &[(ENGINEER, High), (OFFICER, Low)],
            JoblessRole::BeOverflow,
        )];
        assert_eq!(divide(&mut jobs, &players), [Assignment::Job(ENGINEER)]);
        let players = vec![prefs(&[(ENGINEER, Medium)], JoblessRole::BeOverflow); 2];
        let got = divide(&mut jobs, &players);
        assert!(got.contains(&Assignment::Job(ENGINEER)), "{got:?}");
        assert!(got.contains(&Assignment::Job(ASSISTANT)), "{got:?}");
    }

    #[test]
    fn overflow_pref_wins_and_jobless_roles_apply() {
        let mut jobs = Jobs::new(data(), 9);
        let players = vec![
            // The assistant toggle is taken before any other level.
            prefs(
                &[(ASSISTANT, High), (CAPTAIN, Low)],
                JoblessRole::BeOverflow,
            ),
            // Nothing they want is open: random job, never command, never the overflow.
            prefs(&[(AI, High)], JoblessRole::BeRandomJob),
            prefs(&[(AI, High)], JoblessRole::ReturnToLobby),
            prefs(&[], JoblessRole::ReturnToLobby),
        ];
        let got = divide(&mut jobs, &players);
        assert_eq!(got[0], Assignment::Job(ASSISTANT));
        // Officer and engineer share the best priority with open spawn slots.
        assert!(
            matches!(got[1], Assignment::Job(OFFICER | ENGINEER)),
            "{got:?}"
        );
        assert_eq!(got[2], Assignment::Rejected);
        assert_eq!(got[3], Assignment::NoPreferences);
    }

    #[test]
    fn officer_slots_scale_with_ready_players() {
        let mut jobs = Jobs::new(data(), 1);
        let players = vec![prefs(&[], JoblessRole::BeOverflow); 100];
        divide(&mut jobs, &players);
        let officer = &jobs.data.jobs[OFFICER];
        // round(100 / 8) = 12.
        assert_eq!((officer.total_positions, officer.spawn_positions), (12, 12));
        // DM `round(55 / 8)` floors 6.875 to 6.
        let mut jobs = Jobs::new(data(), 1);
        divide(&mut jobs, &vec![prefs(&[], JoblessRole::BeOverflow); 55]);
        assert_eq!(jobs.data.jobs[OFFICER].spawn_positions, 6);
        let mut jobs = Jobs::new(data(), 1);
        divide(&mut jobs, &[prefs(&[], JoblessRole::BeOverflow)]);
        assert_eq!(jobs.data.jobs[OFFICER].spawn_positions, 5);
    }

    #[test]
    fn prevented_jobs_are_never_assigned() {
        // A roundstart traitor who wants security gets the overflow role instead, and a random
        // job is never a prevented one.
        for seed in 0..20 {
            let mut jobs = Jobs::new(data(), seed);
            let mut traitor = prefs(&[(OFFICER, High)], JoblessRole::BeOverflow);
            traitor.prevented = vec![OFFICER, CAPTAIN];
            let mut random = prefs(&[(AI, High)], JoblessRole::BeRandomJob);
            random.prevented = vec![OFFICER, ENGINEER];
            let got = divide(&mut jobs, &[traitor, random]);
            assert_eq!(got[0], Assignment::Job(ASSISTANT), "seed {seed}");
            assert_eq!(got[1], Assignment::Job(CLOWN), "seed {seed}");
        }
    }

    #[test]
    fn a_new_high_demotes_the_old_one() {
        let mut p = JobPreferences::default();
        p.set(ASSISTANT, Some(High), ASSISTANT);
        p.set(CLOWN, Some(High), ASSISTANT);
        assert_eq!(p.levels.get(&ASSISTANT), None);
        p.set(ENGINEER, Some(High), ASSISTANT);
        assert_eq!(p.levels[&CLOWN], Medium);
        assert_eq!(p.levels[&ENGINEER], High);
        p.set(ENGINEER, None, ASSISTANT);
        assert_eq!(p.levels.len(), 1);
    }
}
