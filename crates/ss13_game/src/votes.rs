//! Votes, after tg `SSvote` (`code/controllers/subsystem/vote.dm`), `/datum/vote`
//! (`code/datums/votes/`) and BandaStation's `modular_bandastation/automatic_crew_transfer`.
//!
//! - One vote runs at a time ([`Votes`]); the [`VotePanelUi`] window is on a host off the map and
//!   opens from the OOC verb or the action button the start of a vote gives every player's mob.
//! - Votes are counted per client token (tg `ckey`): one choice for a single-choice vote, each
//!   option at most once for a multiple-choice one.
//! - Kinds ([`VoteKind`]): restart (only when no active admin with `R_SERVER` is online), custom
//!   (an admin's dialog of prompts, [`CustomDraft`]) and crew transfer: BandaStation's automatic
//!   vote after `automatic_crew_transfer_vote_delay`, repeated every `..._interval`, whose win
//!   calls the emergency shuttle without recall.
//! - Everything a window sends is checked again when it is applied ([`apply_acts`]).
//!
//! Admin rights live in `ss13_admin`, which depends on this crate: it keeps [`VoteRights`] on the
//! clients and turns [`AdminNotice`] into `message_admins`.
// shortcut: no map vote (the port cannot change the map, docs/parity/map/map-loading.md); no
// purple chat box or `sound_announcements` preference; a client's button stays on the mob it had
// at the start of the vote (tg `player_actions` follow the player), add with a player action list.
// Lobby clients have no mob and so no button: they use the OOC verb.

use serde::{Deserialize, Serialize};
use ss13_tgui::UiInterface;

/// The vote window (`VotePanel.tsx`).
pub struct VotePanelUi;

impl UiInterface for VotePanelUi {
    const NAME: &'static str = "VotePanel";
    type Data = VotePanelData;
    type StaticData = ();
}

/// `act('cancel')` and the rest of `ui_act`.
pub const CANCEL: &str = "cancel";
pub const END_NOW: &str = "endNow";
pub const TOGGLE_DEAD_VOTE: &str = "toggleDeadVote";
pub const TOGGLE_VOTE: &str = "toggleVote";
pub const CALL_VOTE: &str = "callVote";
pub const VOTE_SINGLE: &str = "voteSingle";
pub const VOTE_MULTI: &str = "voteMulti";
pub const RESET_COOLDOWN: &str = "resetCooldown";
pub const VOTE_NAME: &str = "voteName";
pub const VOTE_OPTION: &str = "voteOption";

/// `ui_data` (and `ui_static_data`: the toggle changes with the data here).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct VotePanelData {
    pub user: VoteUser,
    pub current_vote: Option<ActiveVote>,
    pub possible_votes: Vec<VoteEntry>,
    /// Whole seconds until a new vote can be called (`LastVoteTime + VoteCD`).
    pub cooldown: u32,
    /// `deadVoteEnabled`: tg's name for `no_dead_vote`, true when the dead may NOT vote.
    pub no_dead_vote: bool,
}

/// `data["user"]`. Only the viewer's own selections are sent.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct VoteUser {
    pub is_ghost: bool,
    pub is_lower_admin: bool,
    pub is_upper_admin: bool,
    pub selections: Vec<String>,
}

/// One entry of `possibleVotes`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct VoteEntry {
    pub name: String,
    pub can_be_initiated: bool,
    /// `is_config_enabled`: `None` for a vote without a config (`-1`).
    pub config: Option<bool>,
    pub message: String,
}

/// `currentVote`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ActiveVote {
    pub name: String,
    pub question: Option<String>,
    pub time_remaining: i32,
    pub multi: bool,
    pub options: Vec<VoteOption>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct VoteOption {
    pub name: String,
    /// `None` when the viewer may not see the count: hidden on the server, not in the window.
    pub votes: Option<u32>,
}

#[cfg(feature = "server")]
pub use server::*;

#[cfg(feature = "server")]
mod server {
    use std::collections::{HashMap, HashSet};

    use bevy::ecs::message::{MessageCursor, Messages};
    use bevy::ecs::system::SystemParamItem;
    use bevy::prelude::*;
    use ss13_mapload::IconIndex;
    use ss13_mapload::spawner::Rng;
    use ss13_net::chat::server::SendChat;
    use ss13_net::server::{ClientConnected, ClientDisconnected, ClientToken, IntentReceived};
    use ss13_net::sound::server::SendSound;
    use ss13_shared::capitalize;
    use ss13_shared::chat::{ChatChannel, ChatMessage, Span};
    use ss13_shared::protocol::{ClientIntent, SoundEvent};
    use ss13_shared::ui::UiStatus;
    use ss13_tgui::prompt::{
        Ask, PromptAnswer, PromptAnswered, PromptKind, PromptSystems, ask_world,
    };
    use ss13_tgui::server::{
        AlwaysState, AppTguiExt, OpenUi, ServerUi, UiActed, UiKind, UiParams, UiReject, UiSystems,
        UiWindow,
    };

    use super::*;
    use crate::actions::{ActionButton, ActionButtons, ActionButtonsUi, set_buttons};
    use crate::ghost::Ghost;
    use crate::health::Stat;
    use crate::jobs::display_time_text;
    use crate::lobby::GameState;
    use crate::round::Ticker;
    use crate::server::{PlayerMob, Players};
    use crate::shuttle::{EmergencyShuttle, ShuttleMode};
    use crate::statpanel::{OOC_TAB, OOC_VERB_VOTE, StatPanelHost, StatPanelUi};

    pub const WINDOW_TITLE: &str = "Голосование";
    /// The action button's id; tg `/datum/action/vote`.
    pub const VOTE_BUTTON: &str = "vote";
    /// tg `vote_sound`.
    const VOTE_SOUND: &str = "sound/misc/bloop.ogg";
    /// tg `inactivity_period` default: `is_afk`.
    const INACTIVITY_SECS: f64 = 300.0;
    /// `MAX_CUSTOM_VOTE_OPTIONS`.
    const MAX_CUSTOM_OPTIONS: usize = 10;
    const MAX_NAME_LEN: u32 = 42;
    const MAX_MESSAGE_LEN: u32 = 1024;
    const CHOICE_RESTART: &str = "Перезапустить раунд";
    const CHOICE_TRANSFER: &str = "Начать смену экипажа";
    const CHOICE_CONTINUE: &str = "Продолжить игру";
    const TRANSFER_REASON: &str = " Автоматическое окончание смены";

    /// tg's config entries, from the environment (seconds), as `crate::shuttle::ShuttleConfig`.
    #[derive(Resource, Clone, Debug, PartialEq)]
    pub struct VoteConfig {
        /// `vote_delay`.
        pub delay_secs: f64,
        /// `vote_period`.
        pub period_secs: f64,
        pub no_dead_vote: bool,
        pub default_no_vote: bool,
        pub allow_restart: bool,
        pub allow_crew_transfer: bool,
        /// `enable_automatic_crew_transfer`.
        pub automatic_transfer: bool,
        /// `automatic_crew_transfer_vote_delay`.
        pub transfer_delay_secs: f64,
        /// `automatic_crew_transfer_vote_interval`.
        pub transfer_interval_secs: f64,
    }

    impl VoteConfig {
        /// tg's defaults (`config.txt`, the module's entries).
        pub fn tg() -> Self {
            Self {
                delay_secs: 600.0,
                period_secs: 60.0,
                no_dead_vote: false,
                default_no_vote: false,
                allow_restart: false,
                allow_crew_transfer: true,
                automatic_transfer: true,
                transfer_delay_secs: 5400.0,
                transfer_interval_secs: 1800.0,
            }
        }
    }

    impl Default for VoteConfig {
        fn default() -> Self {
            let secs = |key: &str| {
                std::env::var(key)
                    .ok()
                    .and_then(|v| v.trim().parse::<f64>().ok())
                    .filter(|v| v.is_finite() && *v >= 0.0)
            };
            let flag = |key: &str, default: bool| {
                std::env::var(key).map_or(default, |v| matches!(v.trim(), "1" | "true" | "yes"))
            };
            let tg = Self::tg();
            Self {
                delay_secs: secs("SS13_VOTE_DELAY_SECONDS").unwrap_or(tg.delay_secs),
                period_secs: secs("SS13_VOTE_PERIOD_SECONDS").unwrap_or(tg.period_secs),
                no_dead_vote: flag("SS13_NO_DEAD_VOTE", tg.no_dead_vote),
                default_no_vote: flag("SS13_DEFAULT_NO_VOTE", tg.default_no_vote),
                allow_restart: flag("SS13_ALLOW_VOTE_RESTART", tg.allow_restart),
                allow_crew_transfer: flag("SS13_ALLOW_CREW_TRANSFER_VOTE", tg.allow_crew_transfer),
                automatic_transfer: flag("SS13_AUTOMATIC_CREW_TRANSFER", tg.automatic_transfer),
                // tg `min_val` 30 and 15 minutes.
                transfer_delay_secs: secs("SS13_CREW_TRANSFER_DELAY_SECONDS")
                    .map_or(tg.transfer_delay_secs, |s| s.max(1800.0)),
                transfer_interval_secs: secs("SS13_CREW_TRANSFER_INTERVAL_SECONDS")
                    .map_or(tg.transfer_interval_secs, |s| s.max(900.0)),
            }
        }
    }

    /// The `/datum/vote` subtypes, in `subtypesof` order.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum VoteKind {
        Custom,
        Restart,
        CrewTransfer,
    }

    impl VoteKind {
        pub const ALL: [Self; 3] = [Self::Custom, Self::Restart, Self::CrewTransfer];

        pub fn name(self) -> &'static str {
            match self {
                Self::Custom => "Особое",
                Self::Restart => "Рестарт",
                Self::CrewTransfer => "Смена экипажа",
            }
        }

        fn by_name(name: &str) -> Option<Self> {
            Self::ALL.into_iter().find(|k| k.name() == name)
        }

        fn default_message(self) -> &'static str {
            match self {
                Self::Custom => "Нажмите, чтобы начать особое голосование.",
                Self::Restart => {
                    "Голосование за перезапуск текущего раунда. Сработает, только если на сервере \
                     нет активных администраторов."
                }
                Self::CrewTransfer => "Голосование за вызов шаттла",
            }
        }

        /// `is_config_enabled`; `None` is tg's `-1`.
        fn config(self, config: &VoteConfig) -> Option<bool> {
            match self {
                Self::Custom => None,
                Self::Restart => Some(config.allow_restart),
                Self::CrewTransfer => Some(config.allow_crew_transfer),
            }
        }

        fn default_choices(self) -> Vec<(String, u32)> {
            let names: &[&str] = match self {
                Self::Custom => &[],
                Self::Restart => &[CHOICE_RESTART, CHOICE_CONTINUE],
                Self::CrewTransfer => &[CHOICE_TRANSFER, CHOICE_CONTINUE],
            };
            names.iter().map(|n| ((*n).to_owned(), 0)).collect()
        }
    }

    /// `can_be_initiated`: the reason why not, as the window's tooltip shows it.
    pub fn can_initiate(
        kind: VoteKind,
        forced: bool,
        config: &VoteConfig,
        state: GameState,
        shuttle_idle: bool,
    ) -> Result<(), String> {
        if !forced && !kind.config(config).unwrap_or(true) {
            return Err("Это голосование отключено настройками сервера.".to_owned());
        }
        match kind {
            VoteKind::Custom if !forced => {
                Err("Только администраторы могут создавать особые голосования.".to_owned())
            }
            VoteKind::CrewTransfer => match state {
                GameState::Playing if shuttle_idle => Ok(()),
                GameState::Playing => Err("Шаттл не может быть вызван.".to_owned()),
                GameState::Finished => Err("Раунд уже закончен.".to_owned()),
                _ => Err("Раунд еще не начался.".to_owned()),
            },
            _ => Ok(()),
        }
    }

    /// `VOTE_WINNER_METHOD_*`.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum WinnerMethod {
        Simple,
        WeightedRandom,
        None,
    }

    /// A running vote (the `/datum/vote` singleton's "internal values").
    #[derive(Clone, Debug, PartialEq)]
    pub struct Vote {
        pub kind: VoteKind,
        /// `override_question`.
        pub question: Option<String>,
        pub multi: bool,
        pub winner_method: WinnerMethod,
        pub display_statistics: bool,
        pub print_results: bool,
        /// Option and its votes, in order.
        pub choices: Vec<(String, u32)>,
        /// Client token to what it chose (`choices_by_ckey`).
        pub by_key: HashMap<String, Vec<String>>,
        pub started_at: f64,
        pub time_remaining: i32,
    }

    impl Vote {
        pub fn new(kind: VoteKind, started_at: f64, period_secs: f64) -> Self {
            Self {
                kind,
                question: None,
                multi: false,
                winner_method: WinnerMethod::Simple,
                display_statistics: true,
                print_results: true,
                choices: kind.default_choices(),
                by_key: HashMap::new(),
                started_at,
                time_remaining: period_secs.round() as i32,
            }
        }

        fn slot(&mut self, option: &str) -> Option<&mut u32> {
            self.choices
                .iter_mut()
                .find(|(n, _)| n == option)
                .map(|(_, v)| v)
        }

        /// `submit_single_vote`: the token's choice is `option`; its old one loses the vote.
        pub fn submit_single(&mut self, key: &str, option: &str) -> bool {
            if self.multi || self.slot(option).is_none() {
                return false;
            }
            let old = self
                .by_key
                .insert(key.to_owned(), vec![option.to_owned()])
                .and_then(|o| o.into_iter().next());
            if let Some(slot) = old.and_then(|o| self.slot(&o)) {
                *slot -= 1;
            }
            *self.slot(option).expect("checked above") += 1;
            true
        }

        /// `submit_multi_vote`: toggles `option` for the token.
        pub fn submit_multi(&mut self, key: &str, option: &str) -> bool {
            if !self.multi || self.slot(option).is_none() {
                return false;
            }
            let chosen = self.by_key.entry(key.to_owned()).or_default();
            let remove = chosen.iter().position(|o| o == option);
            match remove {
                Some(i) => {
                    chosen.remove(i);
                    *self.slot(option).expect("checked above") -= 1;
                }
                None => {
                    chosen.push(option.to_owned());
                    *self.slot(option).expect("checked above") += 1;
                }
            }
            true
        }

        pub fn total(&self) -> u32 {
            self.choices.iter().map(|(_, v)| v).sum()
        }

        /// `get_simple_winner` as documented: no votes at all is no winner. tg's loop returns
        /// every option then, and `finalize_vote` would run a random option of an empty vote.
        pub fn simple_winners(&self) -> Vec<String> {
            let top = self.choices.iter().map(|(_, v)| *v).max().unwrap_or(0);
            if top == 0 {
                return Vec::new();
            }
            self.choices
                .iter()
                .filter(|(_, v)| *v == top)
                .map(|(n, _)| n.clone())
                .collect()
        }

        /// `get_random_winner`: `roll` in `1..=total()` picks by weight; 0 votes is no winner.
        pub fn weighted_winner(&self, roll: u32) -> Vec<String> {
            let mut left = roll;
            for (name, votes) in &self.choices {
                if *votes > 0 && left <= *votes {
                    return vec![name.clone()];
                }
                left = left.saturating_sub(*votes);
            }
            Vec::new()
        }

        /// `get_result_text`.
        pub fn result_text(&self, winners: &[String], winner: Option<&str>) -> String {
            let total = self.total();
            if total == 0 {
                return "Итог голосования: не состоялось, голосов нет!".to_owned();
            }
            let method = match self.winner_method {
                WinnerMethod::None => "нет",
                WinnerMethod::WeightedRandom => "взвешенный случай",
                WinnerMethod::Simple => "простое большинство",
            };
            let mut text = format!("Выбор победителя: {method}");
            if self.display_statistics || self.print_results {
                text.push_str("\nРезультаты:");
                for (option, votes) in &self.choices {
                    let share = f64::from(*votes) * 100.0 / f64::from(total);
                    text.push_str(&format!("\n{share:5.1}% | {option}: {votes}"));
                }
            }
            let Some(winner) = winner else {
                return text;
            };
            if winners.len() > 1 {
                text.push_str("\nНичья между:");
                for option in winners {
                    text.push_str(&format!("\n\t{option}"));
                }
            }
            text.push_str(&format!("\nИтог голосования: {winner}"));
            let title = self
                .question
                .clone()
                .unwrap_or_else(|| format!("Голосование: {}", self.kind.name()));
            format!("{title}\n{text}")
        }
    }

    /// `can_vote_start` apart from the "server not initialized" check.
    pub fn can_start(
        now: f64,
        last_vote: f64,
        running: bool,
        forced: bool,
        config: &VoteConfig,
    ) -> Result<(), String> {
        if forced {
            return Ok(());
        }
        let next = last_vote + config.delay_secs;
        if next > now {
            return Err(format!(
                "Голосование начиналось недавно. Новое можно будет начать через {}!",
                display_time_text(next - now)
            ));
        }
        if running {
            return Err("Голосование уже идёт! Дождитесь его окончания.".to_owned());
        }
        Ok(())
    }

    /// The step of a custom vote's dialog waiting for its answer.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum DraftStep {
        CountMethod,
        WinnerMethod,
        Statistics,
        PrintResults,
        Question,
        Option,
    }

    /// `custom_vote/create_vote`'s prompts, in progress.
    #[derive(Clone, Debug)]
    struct CustomDraft {
        client: Entity,
        prompt: Entity,
        step: DraftStep,
        initiator: String,
        vote: Vote,
    }

    /// `SSvote`.
    #[derive(Resource)]
    pub struct Votes {
        pub current: Option<Vote>,
        /// Client tokens that voted in the current vote (`voted`).
        pub voted: HashSet<String>,
        pub last_vote: f64,
        host: Option<Entity>,
        draft: Option<CustomDraft>,
        /// `automatic_crew_transfer_vote_cooldown`; `None` until the round starts.
        pub transfer_at: Option<f64>,
        /// A finished vote closes the windows (`SStgui.close_uis`).
        close_panels: bool,
        /// Last input of each client (`client.inactivity`).
        activity: HashMap<Entity, f64>,
        rng: Rng,
    }

    impl Default for Votes {
        fn default() -> Self {
            Self {
                current: None,
                voted: HashSet::new(),
                last_vote: f64::NEG_INFINITY,
                host: None,
                draft: None,
                transfer_at: None,
                close_panels: false,
                activity: HashMap::new(),
                rng: Rng::new(0x766f_7465),
            }
        }
    }

    /// The window's source, off the map.
    #[derive(Component, Clone, Copy, Debug)]
    pub struct VoteHost;

    /// A client's active admin rights, kept by `ss13_admin` (absent: a plain player).
    #[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct VoteRights {
        /// tg `client.holder`.
        pub holder: bool,
        /// `R_ADMIN`.
        pub admin: bool,
        /// `R_SERVER`.
        pub server: bool,
    }

    /// tg `message_admins`, delivered by `ss13_admin`.
    #[derive(Message, Clone, Debug)]
    pub struct AdminNotice(pub String);

    /// A parsed `ui_act`.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum VoteAct {
        Cancel,
        EndNow,
        ToggleDeadVote,
        ToggleVote(String),
        CallVote(String),
        Single(String),
        Multi(String),
        ResetCooldown,
    }

    pub struct VotesPlugin;

    impl Plugin for VotesPlugin {
        fn build(&self, app: &mut App) {
            app.register_ui::<VotePanelUi>()
                .init_resource::<Votes>()
                .init_resource::<VoteConfig>()
                .add_message::<AdminNotice>()
                .add_systems(Startup, spawn_host)
                .add_systems(Update, (track_activity, fire).chain())
                .add_systems(Update, close_panels.in_set(UiSystems::StatusHook))
                .add_systems(
                    Update,
                    (
                        open_from_verb,
                        trigger_button,
                        apply_acts,
                        draft_answers.after(PromptSystems::Answer),
                    )
                        .in_set(UiSystems::Handle),
                );
        }
    }

    fn spawn_host(mut votes: ResMut<Votes>, mut commands: Commands) {
        let host = commands
            .spawn((Name::new("vote host"), VoteHost, AlwaysState))
            .id();
        votes.host = Some(host);
    }

    /// The connected clients (`GLOB.clients`).
    fn clients(world: &mut World) -> Vec<Entity> {
        let mut q = world.query_filtered::<Entity, With<ClientToken>>();
        q.iter(world).collect()
    }

    fn token(world: &World, client: Entity) -> Option<String> {
        world.get::<ClientToken>(client).map(|t| t.0.clone())
    }

    fn rights(world: &World, client: Entity) -> VoteRights {
        world.get::<VoteRights>(client).copied().unwrap_or_default()
    }

    fn now(world: &World) -> f64 {
        world.resource::<Time>().elapsed_secs_f64()
    }

    fn tell_all(world: &mut World, span: Span, text: impl Into<String>) {
        let clients = clients(world);
        if !clients.is_empty() {
            world.write_message(SendChat {
                clients,
                message: ChatMessage::simple(ChatChannel::System, span, text),
            });
        }
    }

    fn tell(world: &mut World, client: Entity, span: Span, text: impl Into<String>) {
        crate::tools::tell(world, client, span, text);
    }

    fn notify_admins(world: &mut World, text: impl Into<String>) {
        world.write_message(AdminNotice(text.into()));
    }

    /// `EMERGENCY_IDLE_OR_RECALLED`.
    pub fn shuttle_idle(world: &World) -> bool {
        world
            .get_resource::<EmergencyShuttle>()
            .is_some_and(|s| matches!(s.state.mode, ShuttleMode::Idle | ShuttleMode::Recall))
    }

    /// `/client/proc/is_afk`.
    fn is_afk(votes: &Votes, client: Entity, now: f64) -> bool {
        votes
            .activity
            .get(&client)
            .is_none_or(|&last| now - last > INACTIVITY_SECS)
    }

    /// The input timestamps of `is_afk`. A mouse moved over the map is not activity (BYOND counts
    /// verbs and clicks in `client.inactivity`).
    fn track_activity(
        mut connected: MessageReader<ClientConnected>,
        mut disconnected: MessageReader<ClientDisconnected>,
        mut intents: MessageReader<IntentReceived>,
        time: Res<Time>,
        mut votes: ResMut<Votes>,
    ) {
        let now = time.elapsed_secs_f64();
        for c in connected.read() {
            votes.activity.insert(c.client, now);
        }
        for intent in intents.read() {
            if !matches!(intent.intent, ClientIntent::Hover(_)) {
                votes.activity.insert(intent.client, now);
            }
        }
        for d in disconnected.read() {
            votes.activity.remove(&d.client);
        }
    }

    /// `/datum/vote/restart_vote/proc/admins_present`.
    fn admins_present(world: &mut World) -> bool {
        let now = now(world);
        let mut q = world.query::<(Entity, &VoteRights)>();
        let online: Vec<Entity> = q
            .iter(world)
            .filter(|(_, r)| r.server)
            .map(|(e, _)| e)
            .collect();
        let votes = world.resource::<Votes>();
        online.into_iter().any(|e| !is_afk(votes, e, now))
    }

    /// `SSvote.fire` and `SSautomatic_transfer.fire`, once a second.
    fn fire(world: &mut World, mut next: Local<f64>) {
        let now = now(world);
        if now < *next {
            return;
        }
        *next = now + 1.0;
        let period = world.resource::<VoteConfig>().period_secs;
        let mut votes = world.resource_mut::<Votes>();
        let ended = votes.current.as_mut().is_some_and(|v| {
            v.time_remaining = (v.started_at + period - now).round() as i32;
            v.time_remaining < 0
        });
        if ended {
            end_vote(world);
        }
        automatic_transfer(world, now);
    }

    /// `SSautomatic_transfer`: armed when the round starts, then started by its cooldown.
    fn automatic_transfer(world: &mut World, now: f64) {
        let config = world.resource::<VoteConfig>().clone();
        if !config.automatic_transfer || world.resource::<Ticker>().state() != GameState::Playing {
            return;
        }
        let Some(at) = world.resource::<Votes>().transfer_at else {
            world.resource_mut::<Votes>().transfer_at = Some(now + config.transfer_delay_secs);
            return;
        };
        if now < at || !shuttle_idle(world) {
            return;
        }
        initiate_vote(
            world,
            VoteKind::CrewTransfer,
            Some("Автоматическая смена экипажа".to_owned()),
            None,
            true,
        );
        world.resource_mut::<Votes>().transfer_at = Some(now + config.transfer_interval_secs);
    }

    /// `end_vote`.
    pub fn end_vote(world: &mut World) {
        if world.resource::<Votes>().current.is_none() {
            return;
        }
        process_vote_result(world);
        world.resource_mut::<Votes>().close_panels = true;
        reset(world);
    }

    /// `reset`: no vote, no action buttons.
    fn reset(world: &mut World) {
        let mut votes = world.resource_mut::<Votes>();
        votes.current = None;
        votes.voted.clear();
        let mut q = world.query::<(Entity, &ActionButtons)>();
        let with_button: Vec<Entity> = q
            .iter(world)
            .filter(|(_, b)| b.0.iter().any(|(b, _)| b.id == VOTE_BUTTON))
            .map(|(e, _)| e)
            .collect();
        for mob in with_button {
            remove_button(world, mob);
        }
    }

    fn remove_button(world: &mut World, mob: Entity) {
        let Some(buttons) = world.get::<ActionButtons>(mob) else {
            return;
        };
        let kept: Vec<_> = buttons
            .0
            .iter()
            .filter(|(b, _)| b.id != VOTE_BUTTON)
            .cloned()
            .collect();
        set_buttons(world, mob, kept);
    }

    /// tg's `voting_action.Grant(new_voter.mob)`, after the buttons the mob has.
    fn grant_button(world: &mut World, mob: Entity, name: &str) {
        let button = ActionButton {
            id: VOTE_BUTTON.to_owned(),
            name: name.to_owned(),
            icon: icon(world, "icons/hud/actions.dmi", "vote"),
            background: icon(world, "icons/mob/actions/backgrounds.dmi", "bg_default"),
            border: None,
            available: true,
            cooldown: 0.0,
            active: false,
            maptext: None,
        };
        let mut buttons: Vec<(ActionButton, f64)> = world
            .get::<ActionButtons>(mob)
            .map(|b| {
                b.0.iter()
                    .filter(|(b, _)| b.id != VOTE_BUTTON)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        buttons.push((button, 0.0));
        set_buttons(world, mob, buttons);
    }

    fn icon(world: &World, dmi: &str, state: &str) -> Option<ss13_shared::IconStateId> {
        world
            .get_resource::<IconIndex>()
            .and_then(|icons| icons.get(dmi, state))
    }

    /// `process_vote_result`: counts, announces and applies the current vote.
    fn process_vote_result(world: &mut World) {
        let now = now(world);
        let mut q = world.query::<(Entity, &ClientToken)>();
        let mut non_voters = 0u32;
        let online: Vec<(Entity, String)> = q.iter(world).map(|(e, t)| (e, t.0.clone())).collect();
        let default_no_vote = world.resource::<VoteConfig>().default_no_vote;
        let (winners, result, vote) = {
            let mut votes = world.resource_mut::<Votes>();
            let votes = &mut *votes;
            for (client, key) in &online {
                if !votes.voted.contains(key) && !is_afk(votes, *client, now) {
                    non_voters += 1;
                }
            }
            let Some(vote) = votes.current.as_mut() else {
                return;
            };
            // `restart_vote/get_vote_result`: non-voters count for continuing.
            if vote.kind == VoteKind::Restart
                && !default_no_vote
                && let Some(slot) = vote.slot(CHOICE_CONTINUE)
            {
                *slot += non_voters;
            }
            let winners = match vote.winner_method {
                WinnerMethod::None => Vec::new(),
                WinnerMethod::Simple => vote.simple_winners(),
                WinnerMethod::WeightedRandom => {
                    let total = vote.total();
                    if total == 0 {
                        Vec::new()
                    } else {
                        let roll = votes.rng.range(1, i64::from(total)) as u32;
                        vote.weighted_winner(roll)
                    }
                }
            };
            let final_winner = match winners.len() {
                0 => None,
                1 => Some(winners[0].clone()),
                _ => {
                    let i = votes.rng.range(0, winners.len() as i64 - 1) as usize;
                    Some(winners[i].clone())
                }
            };
            let result = vote.result_text(&winners, final_winner.as_deref());
            (final_winner, result, vote.clone())
        };
        tell_all(world, Span::Info, result);
        finalize_vote(world, &vote, winners.as_deref());
    }

    /// `finalize_vote` of the kinds. No winner, no effect.
    fn finalize_vote(world: &mut World, vote: &Vote, winner: Option<&str>) {
        match (vote.kind, winner) {
            (VoteKind::Restart, Some(CHOICE_RESTART)) => {
                if admins_present(world) {
                    tell_all(
                        world,
                        Span::BoldAnnounce,
                        "Внимание: голосование за рестарт не перезапустит сервер автоматически, \
                         потому что на сервере есть активные администраторы.",
                    );
                    notify_admins(
                        world,
                        "Голосование за рестарт прошло, но на сервере есть активные администраторы \
                         с +SERVER, поэтому оно отменено. При необходимости перезапустите сервер.",
                    );
                    return;
                }
                world.resource_mut::<Ticker>().force_ending = true;
                info!("round ended by a successful restart vote");
            }
            (VoteKind::CrewTransfer, Some(CHOICE_TRANSFER)) => initiate_transfer(world),
            _ => {}
        }
    }

    /// `crew_transfer/initiate_tranfer`.
    fn initiate_transfer(world: &mut World) {
        if !shuttle_idle(world) {
            notify_admins(
                world,
                "Шаттл не может быть вызван голосованием за смену экипажа: он уже занят или \
                 отключён.",
            );
            return;
        }
        if let Some(mut shuttle) = world.get_resource_mut::<EmergencyShuttle>() {
            shuttle.state.no_recall = true;
            shuttle.state.mode = ShuttleMode::Idle;
        }
        let coeff = crate::security_level::current(world).shuttle_call_time_mod();
        crate::shuttle::call_shuttle(world, TRANSFER_REASON, None, coeff);
        notify_admins(world, "Шаттл вызван голосованием за смену экипажа.");
    }

    /// `initiate_vote`. A custom vote only opens its dialog here; the vote starts at its end.
    pub fn initiate_vote(
        world: &mut World,
        kind: VoteKind,
        initiator_name: Option<String>,
        initiator: Option<Entity>,
        forced: bool,
    ) -> bool {
        let now = now(world);
        let config = world.resource::<VoteConfig>().clone();
        let votes = world.resource::<Votes>();
        let started = can_start(
            now,
            votes.last_vote,
            votes.current.is_some(),
            forced,
            &config,
        )
        .and_then(|()| {
            can_initiate(
                kind,
                forced,
                &config,
                world.resource::<Ticker>().state(),
                shuttle_idle(world),
            )
        });
        if let Err(reason) = started {
            if let Some(client) = initiator {
                tell(world, client, Span::Warning, reason);
            }
            return false;
        }
        let name = initiator_name.clone();
        if kind == VoteKind::Custom {
            let Some(client) = initiator else {
                return false;
            };
            begin_custom(world, client, name.unwrap_or_default());
            return true;
        }
        if kind == VoteKind::Restart
            && admins_present(world)
            && let Some(client) = initiator
        {
            // `async_alert_about_admins`.
            ask_world(
                world,
                Ask::new(
                    client,
                    client,
                    "Голосование за рестарт",
                    "Внимание: независимо от результата голосования раунд не перезапустится \
                     автоматически, потому что на сервере есть активный администратор.",
                    PromptKind::Alert {
                        buttons: vec!["Ок".to_owned()],
                        autofocus: false,
                    },
                ),
            );
        }
        let vote = Vote::new(kind, now, config.period_secs);
        start(world, vote, initiator_name);
        true
    }

    /// The moment a vote really starts: `reset`, `current_vote`, the announcement, the sound and
    /// the buttons.
    fn start(world: &mut World, vote: Vote, initiator: Option<String>) {
        reset(world);
        let now = now(world);
        let period = world.resource::<VoteConfig>().period_secs;
        let title = vote
            .question
            .clone()
            .unwrap_or_else(|| vote.kind.name().to_owned());
        let starter = initiator.unwrap_or_else(|| "Центральное командование".to_owned());
        let mut line = format!(
            "Голосование «{}» начато: {starter}.",
            capitalize(vote.kind.name())
        );
        if let Some(question) = &vote.question {
            line.push('\n');
            line.push_str(question);
        }
        let mut votes = world.resource_mut::<Votes>();
        votes.current = Some(vote);
        votes.last_vote = now;
        tell_all(
            world,
            Span::Info,
            format!(
                "{line}\nНажмите кнопку голосования или откройте его в разделе OOC. На \
                 голосование даётся {}.",
                display_time_text(period)
            ),
        );
        let targets = clients(world);
        for &client in &targets {
            world.write_message(SendSound {
                client,
                sound: SoundEvent {
                    file: VOTE_SOUND.to_owned(),
                    volume: 100.0,
                    speed: 1.0,
                    offset: None,
                },
            });
        }
        let mut q = world.query_filtered::<Entity, With<PlayerMob>>();
        let mobs: Vec<Entity> = q.iter(world).collect();
        let name = format!("Голосование: {title}");
        for mob in mobs {
            grant_button(world, mob, &name);
        }
        world.resource_mut::<Votes>().close_panels = false;
    }

    fn begin_custom(world: &mut World, client: Entity, initiator: String) {
        if world.resource::<Votes>().draft.is_some() {
            tell(
                world,
                client,
                Span::Warning,
                "Другой администратор уже создаёт особое голосование.",
            );
            return;
        }
        let period = world.resource::<VoteConfig>().period_secs;
        let now = now(world);
        let prompt = ask_list(
            world,
            client,
            "Способ выбора",
            "Один вариант или несколько?",
            &["Один", "Несколько"],
        );
        world.resource_mut::<Votes>().draft = Some(CustomDraft {
            client,
            prompt,
            step: DraftStep::CountMethod,
            initiator,
            vote: Vote::new(VoteKind::Custom, now, period),
        });
    }

    fn ask_list(
        world: &mut World,
        client: Entity,
        title: &str,
        message: &str,
        items: &[&str],
    ) -> Entity {
        ask_world(
            world,
            Ask::new(
                client,
                client,
                title,
                message,
                PromptKind::List {
                    items: items.iter().map(|s| (*s).to_owned()).collect(),
                    default: items.first().map(|s| (*s).to_owned()),
                },
            ),
        )
    }

    fn ask_yes_no(world: &mut World, client: Entity, title: &str, message: &str) -> Entity {
        ask_world(
            world,
            Ask::new(
                client,
                client,
                title,
                message,
                PromptKind::Alert {
                    buttons: vec!["Да".to_owned(), "Нет".to_owned()],
                    autofocus: false,
                },
            ),
        )
    }

    fn ask_text(world: &mut World, client: Entity, title: &str, message: &str, max: u32) -> Entity {
        ask_world(
            world,
            Ask::new(
                client,
                client,
                title,
                message,
                PromptKind::Text {
                    default: String::new(),
                    max_length: max,
                    multiline: false,
                },
            ),
        )
    }

    fn ask_next_option(world: &mut World, client: Entity) -> Entity {
        ask_text(
            world,
            client,
            "Варианты",
            &format!(
                "Введите вариант или нажмите «Отмена», чтобы закончить. Не больше \
                 {MAX_CUSTOM_OPTIONS}."
            ),
            MAX_NAME_LEN,
        )
    }

    /// The dialog's answers. Any cancel before the options ends it (tg `return FALSE`).
    fn draft_answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
        let Some(messages) = world.get_resource::<Messages<PromptAnswered>>() else {
            return;
        };
        let answered: Vec<(Entity, Option<PromptAnswer>)> = cursor
            .read(messages)
            .map(|a| (a.prompt, a.answer.clone()))
            .collect();
        for (prompt, answer) in answered {
            let is_draft = world
                .resource::<Votes>()
                .draft
                .as_ref()
                .is_some_and(|d| d.prompt == prompt);
            if is_draft {
                step_draft(world, answer);
            }
        }
    }

    fn step_draft(world: &mut World, answer: Option<PromptAnswer>) {
        let Some(mut draft) = world.resource_mut::<Votes>().draft.take() else {
            return;
        };
        let client = draft.client;
        let next: Option<Entity> = match (draft.step, answer) {
            (DraftStep::CountMethod, Some(PromptAnswer::Item(i))) => {
                draft.vote.multi = i == 1;
                draft.step = DraftStep::WinnerMethod;
                Some(ask_list(
                    world,
                    client,
                    "Способ определения победителя",
                    "Как определить победителя?",
                    &["Простое большинство", "Взвешенный случай", "Без победителя"],
                ))
            }
            (DraftStep::WinnerMethod, Some(PromptAnswer::Item(i))) => {
                draft.vote.winner_method = match i {
                    0 => WinnerMethod::Simple,
                    1 => WinnerMethod::WeightedRandom,
                    _ => WinnerMethod::None,
                };
                draft.step = DraftStep::Statistics;
                Some(ask_yes_no(
                    world,
                    client,
                    "Показывать статистику?",
                    "Сделать статистику голосования публичной?",
                ))
            }
            (DraftStep::Statistics, Some(PromptAnswer::Button(b))) => {
                draft.vote.display_statistics = b == 0;
                if draft.vote.display_statistics {
                    draft.step = DraftStep::Question;
                    Some(ask_text(
                        world,
                        client,
                        "Особое голосование",
                        "О чём голосование?",
                        MAX_MESSAGE_LEN,
                    ))
                } else {
                    draft.step = DraftStep::PrintResults;
                    Some(ask_yes_no(
                        world,
                        client,
                        "Показать итог?",
                        "Показать итог голосования после окончания?",
                    ))
                }
            }
            (DraftStep::PrintResults, Some(PromptAnswer::Button(b))) => {
                draft.vote.print_results = b == 0;
                draft.step = DraftStep::Question;
                Some(ask_text(
                    world,
                    client,
                    "Особое голосование",
                    "О чём голосование?",
                    MAX_MESSAGE_LEN,
                ))
            }
            (DraftStep::Question, Some(PromptAnswer::Text(text))) if !text.trim().is_empty() => {
                draft.vote.question = Some(text);
                draft.step = DraftStep::Option;
                Some(ask_next_option(world, client))
            }
            (DraftStep::Option, Some(PromptAnswer::Text(text))) if !text.trim().is_empty() => {
                let option = capitalize(text.trim());
                if !draft.vote.choices.iter().any(|(n, _)| *n == option) {
                    draft.vote.choices.push((option, 0));
                }
                if draft.vote.choices.len() < MAX_CUSTOM_OPTIONS {
                    Some(ask_next_option(world, client))
                } else {
                    finish_custom(world, draft);
                    return;
                }
            }
            (DraftStep::Option, _) => {
                finish_custom(world, draft);
                return;
            }
            // A cancel or an empty question: no vote.
            _ => None,
        };
        if let Some(prompt) = next {
            draft.prompt = prompt;
            world.resource_mut::<Votes>().draft = Some(draft);
        }
    }

    /// The end of `create_vote`: the prompts took a while, so the rights are checked again.
    fn finish_custom(world: &mut World, draft: CustomDraft) {
        if draft.vote.choices.is_empty()
            || world.get_entity(draft.client).is_err()
            || !rights(world, draft.client).holder
        {
            return;
        }
        let now = now(world);
        let period = world.resource::<VoteConfig>().period_secs;
        let mut vote = draft.vote;
        vote.started_at = now;
        vote.time_remaining = period.round() as i32;
        start(world, vote, Some(draft.initiator));
    }

    /// `/mob/vote`, the OOC verb: everyone may open the window.
    fn open_from_verb(world: &mut World, mut acts: Local<MessageCursor<UiActed<StatPanelUi>>>) {
        let pending: Vec<(Entity, Entity)> = acts
            .read(world.resource::<Messages<UiActed<StatPanelUi>>>())
            .filter(|a| a.act.tab == OOC_TAB && a.act.name == OOC_VERB_VOTE)
            .map(|a| (a.client, a.source))
            .collect();
        for (client, source) in pending {
            if world.get::<StatPanelHost>(source).map(|h| h.client) == Some(client) {
                open_panel(world, client);
            }
        }
    }

    /// `/datum/action/vote/Trigger`: opens the window and takes the button away.
    fn trigger_button(world: &mut World, mut acts: Local<MessageCursor<UiActed<ActionButtonsUi>>>) {
        let pending: Vec<(Entity, Entity)> = acts
            .read(world.resource::<Messages<UiActed<ActionButtonsUi>>>())
            .filter(|a| a.act == VOTE_BUTTON)
            .map(|a| (a.client, a.source))
            .collect();
        for (client, mob) in pending {
            let has_button = world
                .get::<ActionButtons>(mob)
                .is_some_and(|b| b.0.iter().any(|(b, _)| b.id == VOTE_BUTTON));
            if has_button && world.get::<PlayerMob>(mob).map(|p| p.client) == Some(client) {
                open_panel(world, client);
                remove_button(world, mob);
            }
        }
    }

    /// `SSvote.ui_interact`: the host's window with the client as the actor, so it survives a
    /// change of body.
    pub fn open_panel(world: &mut World, client: Entity) {
        if let Some(host) = world.resource::<Votes>().host {
            world.write_message(OpenUi::new::<VotePanelUi>(
                client,
                client,
                host,
                WINDOW_TITLE,
            ));
        }
    }

    /// Closes the panels of a vote that ended.
    fn close_panels(
        mut votes: ResMut<Votes>,
        mut windows: Query<&mut UiWindow, With<UiKind<VotePanelUi>>>,
    ) {
        if std::mem::take(&mut votes.close_panels) {
            for mut window in &mut windows {
                window.cap_status(UiStatus::Close);
            }
        }
    }

    /// `no_dead_vote` and `voter.stat == DEAD` and not an admin; a ghost and a lobby client
    /// (tg `/mob/dead`) count as dead.
    fn dead_vote_blocked(
        no_dead_vote: bool,
        holder: bool,
        no_mob: bool,
        ghost: bool,
        dead: bool,
    ) -> bool {
        no_dead_vote && !holder && (no_mob || ghost || dead)
    }

    fn voter_blocked(world: &World, client: Entity) -> bool {
        let mob = world.resource::<Players>().mob(client);
        dead_vote_blocked(
            world.resource::<VoteConfig>().no_dead_vote,
            rights(world, client).holder,
            mob.is_none(),
            mob.is_some_and(|m| world.get::<Ghost>(m).is_some()),
            mob.is_some_and(|m| world.get::<Stat>(m) == Some(&Stat::Dead)),
        )
    }

    /// `ui_act`, with every right and the vote's state checked here (the window is untrusted).
    fn apply_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<VotePanelUi>>>) {
        let pending: Vec<(Entity, VoteAct)> = acts
            .read(world.resource::<Messages<UiActed<VotePanelUi>>>())
            .map(|a| (a.client, a.act.clone()))
            .collect();
        for (client, act) in pending {
            let who = token(world, client).unwrap_or_default();
            let r = rights(world, client);
            match act {
                VoteAct::Cancel | VoteAct::EndNow | VoteAct::ResetCooldown if !r.holder => {
                    notify_admins(
                        world,
                        format!(
                            "{who} попытался управлять голосованием без прав администратора: \
                             возможный эксплойт."
                        ),
                    );
                }
                VoteAct::ToggleDeadVote | VoteAct::ToggleVote(_) if !r.admin => {
                    notify_admins(
                        world,
                        format!(
                            "{who} попытался менять настройки голосований без R_ADMIN: возможный \
                             эксплойт."
                        ),
                    );
                }
                VoteAct::Cancel => {
                    notify_admins(world, format!("{who} отменил текущее голосование."));
                    reset(world);
                }
                VoteAct::EndNow => {
                    notify_admins(world, format!("{who} завершил текущее голосование."));
                    end_vote(world);
                }
                VoteAct::ResetCooldown => {
                    world.resource_mut::<Votes>().last_vote = f64::NEG_INFINITY;
                }
                VoteAct::ToggleDeadVote => {
                    let mut config = world.resource_mut::<VoteConfig>();
                    config.no_dead_vote = !config.no_dead_vote;
                    let state = if config.no_dead_vote {
                        "запретил"
                    } else {
                        "разрешил"
                    };
                    notify_admins(world, format!("{who} {state} голосование мёртвых."));
                }
                VoteAct::ToggleVote(name) => {
                    let mut config = world.resource_mut::<VoteConfig>();
                    match VoteKind::by_name(&name) {
                        Some(VoteKind::Restart) => config.allow_restart = !config.allow_restart,
                        Some(VoteKind::CrewTransfer) => {
                            config.allow_crew_transfer = !config.allow_crew_transfer;
                        }
                        _ => {}
                    }
                }
                VoteAct::CallVote(name) => {
                    if let Some(kind) = VoteKind::by_name(&name) {
                        initiate_vote(world, kind, Some(who), Some(client), r.holder);
                    }
                }
                VoteAct::Single(option) => submit(world, client, &who, &option, false),
                VoteAct::Multi(option) => submit(world, client, &who, &option, true),
            }
        }
    }

    /// `submit_single_vote` and `submit_multi_vote`.
    fn submit(world: &mut World, client: Entity, key: &str, option: &str, multi: bool) {
        if key.is_empty() || voter_blocked(world, client) {
            return;
        }
        let mut votes = world.resource_mut::<Votes>();
        let Some(vote) = votes.current.as_mut() else {
            return;
        };
        let known = vote.choices.iter().any(|(n, _)| n == option);
        let accepted = if multi {
            vote.submit_multi(key, option)
        } else {
            vote.submit_single(key, option)
        };
        if accepted {
            votes.voted.insert(key.to_owned());
        } else if !known {
            notify_admins(
                world,
                format!("ВОЗМОЖНЫЙ ЭКСПЛОЙТ: {key} проголосовал за несуществующий вариант."),
            );
        }
    }

    impl ServerUi for VotePanelUi {
        type Source = &'static VoteHost;
        type UserParam = (
            Res<'static, Votes>,
            Res<'static, VoteConfig>,
            Res<'static, Time>,
            Res<'static, Ticker>,
            Option<Res<'static, EmergencyShuttle>>,
            Res<'static, Players>,
            Query<'static, 'static, &'static VoteRights>,
            Query<'static, 'static, &'static ClientToken>,
            Query<'static, 'static, &'static Stat>,
            Query<'static, 'static, (), With<Ghost>>,
        );
        type Act = VoteAct;

        fn static_data(_: &VoteHost) {}

        fn data(
            _: &VoteHost,
            user: Entity,
            params: &SystemParamItem<'_, '_, Self::UserParam>,
        ) -> VotePanelData {
            let (votes, config, time, ticker, shuttle, players, rights, tokens, stats, ghosts) =
                params;
            let r = rights.get(user).copied().unwrap_or_default();
            let mob = players.mob(user);
            let dead = mob.is_some_and(|m| stats.get(m).is_ok_and(|s| *s == Stat::Dead));
            let ghost = mob.is_some_and(|m| ghosts.contains(m));
            let idle = shuttle
                .as_ref()
                .is_some_and(|s| matches!(s.state.mode, ShuttleMode::Idle | ShuttleMode::Recall));
            let state = ticker.state();
            let key = tokens.get(user).ok().map(|t| t.0.clone());
            let selections = votes
                .current
                .as_ref()
                .zip(key.as_ref())
                .and_then(|(v, k)| v.by_key.get(k).cloned())
                .unwrap_or_default();
            let possible_votes = VoteKind::ALL
                .into_iter()
                .map(|kind| {
                    let can = can_initiate(kind, r.holder, config, state, idle);
                    VoteEntry {
                        name: kind.name().to_owned(),
                        can_be_initiated: can.is_ok(),
                        config: kind.config(config),
                        message: can
                            .err()
                            .unwrap_or_else(|| kind.default_message().to_owned()),
                    }
                })
                .collect();
            let current_vote = votes.current.as_ref().map(|v| ActiveVote {
                name: v.kind.name().to_owned(),
                question: v.question.clone(),
                time_remaining: v.time_remaining,
                multi: v.multi,
                options: v
                    .choices
                    .iter()
                    .map(|(name, count)| VoteOption {
                        name: name.clone(),
                        votes: (v.display_statistics || r.holder).then_some(*count),
                    })
                    .collect(),
            });
            let cooldown =
                ((votes.last_vote + config.delay_secs - time.elapsed_secs_f64()).max(0.0)) as u32;
            VotePanelData {
                user: VoteUser {
                    is_ghost: dead_vote_blocked(
                        config.no_dead_vote,
                        r.holder,
                        mob.is_none(),
                        ghost,
                        dead,
                    ),
                    is_lower_admin: r.holder,
                    is_upper_admin: r.admin,
                    selections,
                },
                current_vote,
                possible_votes,
                cooldown,
                no_dead_vote: config.no_dead_vote,
            }
        }

        fn parse_act(action: &str, params: &UiParams) -> Result<VoteAct, UiReject> {
            let name = || params.text(VOTE_NAME);
            let option = || params.text(VOTE_OPTION);
            Ok(match action {
                CANCEL => VoteAct::Cancel,
                END_NOW => VoteAct::EndNow,
                TOGGLE_DEAD_VOTE => VoteAct::ToggleDeadVote,
                RESET_COOLDOWN => VoteAct::ResetCooldown,
                TOGGLE_VOTE => VoteAct::ToggleVote(name()?),
                CALL_VOTE => VoteAct::CallVote(name()?),
                VOTE_SINGLE => VoteAct::Single(option()?),
                VOTE_MULTI => VoteAct::Multi(option()?),
                _ => return Err(UiReject::UnknownAction),
            })
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn vote(multi: bool) -> Vote {
            let mut v = Vote::new(VoteKind::Restart, 0.0, 60.0);
            v.multi = multi;
            v
        }

        #[test]
        fn a_client_has_one_single_vote_and_can_change_it() {
            let mut v = vote(false);
            assert!(v.submit_single("a", CHOICE_RESTART));
            assert!(v.submit_single("a", CHOICE_RESTART));
            assert_eq!(v.choices[0].1, 1);
            assert!(v.submit_single("a", CHOICE_CONTINUE));
            assert_eq!((v.choices[0].1, v.choices[1].1), (0, 1));
            assert!(!v.submit_single("a", "выдуманный вариант"));
            assert_eq!(v.total(), 1);
        }

        #[test]
        fn a_multiple_choice_vote_toggles_each_option_once() {
            let mut v = vote(true);
            assert!(v.submit_multi("a", CHOICE_RESTART));
            assert!(v.submit_multi("a", CHOICE_CONTINUE));
            assert_eq!(v.total(), 2);
            assert!(v.submit_multi("a", CHOICE_RESTART));
            assert_eq!(v.choices[0].1, 0);
            // A window cannot use the other method's act to count twice.
            assert!(!v.submit_single("a", CHOICE_RESTART));
            assert_eq!(v.total(), 1);
        }

        #[test]
        fn no_votes_means_no_winner() {
            let v = vote(false);
            assert!(v.simple_winners().is_empty());
            assert_eq!(v.total(), 0);
            assert!(v.weighted_winner(1).is_empty());
            assert!(v.result_text(&[], None).contains("не состоялось"));
        }

        #[test]
        fn winners_are_the_top_options() {
            let mut v = vote(false);
            v.submit_single("a", CHOICE_RESTART);
            v.submit_single("b", CHOICE_CONTINUE);
            assert_eq!(v.simple_winners().len(), 2);
            v.submit_single("c", CHOICE_RESTART);
            assert_eq!(v.simple_winners(), vec![CHOICE_RESTART.to_owned()]);
            // Weighted: roll 1..=3, the first two rolls land on the two-vote option.
            assert_eq!(v.weighted_winner(2), vec![CHOICE_RESTART.to_owned()]);
            assert_eq!(v.weighted_winner(3), vec![CHOICE_CONTINUE.to_owned()]);
        }

        #[test]
        fn starting_respects_cooldown_and_a_running_vote_unless_forced() {
            let config = VoteConfig::tg();
            assert!(can_start(10.0, 0.0, false, false, &config).is_err());
            assert!(can_start(601.0, 0.0, true, false, &config).is_err());
            assert!(can_start(601.0, 0.0, false, false, &config).is_ok());
            assert!(can_start(10.0, 0.0, true, true, &config).is_ok());
        }

        #[test]
        fn who_may_initiate_what() {
            let config = VoteConfig::tg();
            let state = GameState::Playing;
            // Restart is off by default in the config; only an admin forces it.
            assert!(can_initiate(VoteKind::Restart, false, &config, state, true).is_err());
            assert!(can_initiate(VoteKind::Restart, true, &config, state, true).is_ok());
            assert!(can_initiate(VoteKind::Custom, false, &config, state, true).is_err());
            assert!(can_initiate(VoteKind::Custom, true, &config, state, true).is_ok());
            assert!(can_initiate(VoteKind::CrewTransfer, false, &config, state, true).is_ok());
            assert!(can_initiate(VoteKind::CrewTransfer, false, &config, state, false).is_err());
            assert!(
                can_initiate(
                    VoteKind::CrewTransfer,
                    false,
                    &config,
                    GameState::Pregame,
                    true
                )
                .is_err()
            );
        }

        #[test]
        fn dead_voters_are_blocked_only_with_the_config() {
            assert!(!dead_vote_blocked(false, false, true, true, true));
            assert!(dead_vote_blocked(true, false, false, true, false));
            assert!(dead_vote_blocked(true, false, true, false, false));
            assert!(!dead_vote_blocked(true, true, false, true, false));
            assert!(!dead_vote_blocked(true, false, false, false, false));
        }
    }
}
