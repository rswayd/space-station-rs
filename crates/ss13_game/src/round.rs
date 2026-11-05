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
    CHARACTER_SETUP, GameState, JOBLESS_ROLE, JobPrefsData, JobPrefsStatic, JobPrefsUi,
    JobPriority, JoblessRole, LobbyData, LobbyUi, OBSERVE, RANDOMIZE_APPEARANCE, SET_ANTAGS,
    SET_APPEARANCE, SET_JOB_PREFERENCE, SET_PREFERENCE, TOGGLE_ANTAG_ROLL, TOGGLE_READY,
};
use crate::server::{MoveQueue, Players, SpawnPoints};

/// The server process builds a new `App` when one exits with this code (`ss13_server`).
pub const REBOOT_EXIT_CODE: u8 = 75;
/// tg `start_now` cancelled: `SetTimeLeft(3 MINUTES)`.
const START_NOW_CANCEL_SECS: f64 = 180.0;
/// tg `declare_completion`: `sleep(5 SECONDS)` before `standard_reboot`.
const REPORT_TO_REBOOT_SECS: f64 = 5.0;
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
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct RoundConfig {
    pub lobby_secs: f64,
    pub round_end_secs: f64,
    pub skip_lobby: bool,
    pub round_secs: Option<f64>,
    pub source_url: Option<String>,
}

impl RoundConfig {
    /// tg's defaults, no environment.
    pub fn tg() -> Self {
        Self {
            lobby_secs: 120.0,
            round_end_secs: 25.0,
            skip_lobby: false,
            round_secs: None,
            source_url: None,
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
    /// tg `force_ending`: an admin ended the round; it ends on the next tick.
    pub force_ending: bool,
    /// tg `start_immediately`: an admin started the round; the lobby ends on the next tick.
    start_immediately: bool,
    total_players: u32,
    ready_players: u32,
    in_lobby: u32,
}

impl Ticker {
    pub fn state(&self) -> GameState {
        self.state
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
        .add_systems(Startup, build_prefs_static.after(JobsStartup))
        .add_systems(
            Update,
            (
                tick.before(UiSystems::Open),
                (lobby_acts, preference_acts).in_set(UiSystems::Handle),
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

fn to_world(world: &mut World, span: Span, text: impl Into<String>) {
    let clients = all_clients(world);
    chat(world, clients, span, text);
}

/// tg `station_name()`: the map's name here.
pub(crate) fn station_name(world: &World) -> String {
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
            let timed_out = config
                .round_secs
                .is_some_and(|secs| now - ticker.round_start >= secs);
            if ticker.force_ending || timed_out || shuttle_home {
                ticker.state = GameState::Finished;
                ticker.finished_at = now;
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
                info!("round over: rebooting the world");
                world.write_message(AppExit::from_code(REBOOT_EXIT_CODE));
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
    // The nuclear operatives get no station job (tg `forced_occupations`).
    let operatives: Vec<Entity> = antags
        .iter()
        .filter(|(_, r)| *r == crate::antag::dynamic::NUKEOPS_RULESET)
        .map(|(c, _)| *c)
        .collect();
    ready.retain(|(c, _)| !operatives.contains(c));
    let prefs: Vec<&JobPreferences> = ready.iter().map(|(_, p)| p).collect();
    let assigned = divide_occupations(&mut world.resource_mut::<Jobs>(), &prefs);
    for ((client, _), assignment) in ready.iter().zip(assigned) {
        match assignment {
            Assignment::Job(job) => {
                crate::jobs::join(world, *client, job, false);
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
    crate::antag::dynamic::execute_roundstart(world, &antags);
    let mut ticker = world.resource_mut::<Ticker>();
    ticker.state = GameState::Playing;
    ticker.round_start = now;
    let station = station_name(world);
    let text = format!("Приветствуем вас на {station}. Приятного вам пребывания!");
    to_world(world, Span::BoldNotice, text);
    // `SEND_SOUND(world, SSstation.announcer.get_rand_welcome_sound())`.
    if let Some(welcome) = crate::announce::welcome_sound(world) {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::to_playing_players(welcome, 100.0, false),
        );
    }
    // `PostSetup`: `queue_roundstart_report`.
    crate::announce::queue_roundstart_report(world);
    info!(
        "round started: {} players spawned",
        ready.len() + operatives.len()
    );
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
    antags.extend(crate::antag::cult::server::roundend_report(world, end));
    antags.extend(crate::antag::heretic::server::roundend_report(world, end));
    antags.extend(crate::antag::revolution::server::roundend_report(
        world, end,
    ));
    antags.extend(crate::cargo::economy::market_report(world));
    let clients = all_clients(world);
    for message in antags {
        world.write_message(SendChat {
            clients: clients.clone(),
            message,
        });
    }
    to_world(world, Span::BoldAnnounce, "Раунд закончился.");
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
fn shuffle<T>(items: &mut [T], rng: &mut ss13_mapload::spawner::Rng) {
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
            let n = (unassigned.len() as f64 / SECURITY_SCALING_COEFF).round() as i32;
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
}

impl ServerUi for LobbyUi {
    type Source = (
        &'static NewPlayer,
        &'static CharacterName,
        &'static AntagPreferences,
    );
    type UserParam = (Res<'static, Ticker>, Res<'static, RoundConfig>);
    type Act = LobbyAct;

    fn static_data(_: (&NewPlayer, &CharacterName, &AntagPreferences)) {}

    fn data(
        (player, name, antag): (&NewPlayer, &CharacterName, &AntagPreferences),
        _: Entity,
        (ticker, config): &(Res<Ticker>, Res<RoundConfig>),
    ) -> LobbyData {
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
        }
    }

    fn parse_act(action: &str, _: &UiParams) -> Result<LobbyAct, UiReject> {
        match action {
            TOGGLE_READY => Ok(LobbyAct::ToggleReady),
            OBSERVE => Ok(LobbyAct::Observe),
            CHARACTER_SETUP => Ok(LobbyAct::CharacterSetup),
            TOGGLE_ANTAG_ROLL => Ok(LobbyAct::ToggleAntagRoll),
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
        ),
    ) -> JobPrefsStatic {
        (*board.0).clone()
    }

    fn data(
        (prefs, _, antag, look, _): (
            &JobPreferences,
            &PrefsBoard,
            &AntagPreferences,
            &AppearancePrefs,
            &NewPlayer,
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
            _ => Err(UiReject::UnknownAction),
        }
    }
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
        }
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
    )>,
    mut randomized: Local<u64>,
) {
    let Some(jobs) = jobs else {
        return;
    };
    for act in acts.read() {
        let Ok((mut prefs, mut antag, mut look)) = prefs.get_mut(act.client) else {
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
                    crate::antag::revolution::ROLE_REV_HEAD,
                    crate::antag::revolution::ROLE_PROVOCATEUR,
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
        // round(100 / 8) = 13, capped at 12.
        assert_eq!((officer.total_positions, officer.spawn_positions), (12, 12));
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
