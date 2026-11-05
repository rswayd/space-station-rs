//! Jobs, latejoin and character names (server), after tg `SSjob`, `/datum/job`,
//! `/datum/latejoin_menu` and `/mob/dead/new_player/AttemptLateSpawn`.
//!
//! The jobs come from `ss13-mapload convert` ([`JobsData`] in `<assets>/jobs.ron`); without that
//! file the server knows only the assistant with its old fixed kit. A client in the lobby
//! (`crate::round`, tg `new_player`) sends `ClientIntent::LateJoinMenu` ("Join game"); the
//! `JobSelection` window opens on its connection entity, and a `select_job` act, checked when it
//! is applied (the client is still in the lobby, the round is in progress, the job exists, is
//! playable here and has a free slot), gives it a body of that job ([`join`]): at a latejoin
//! landmark (`/obj/effect/landmark/latejoin`, else the general spawn points), wearing the outfit
//! ([`crate::outfit`]), with an ID card of the trim's access. Round start uses [`join`] too, with
//! the job's `/obj/effect/landmark/start`. Slots count living player mobs per job
//! ([`PlayerJob`]); a despawned mob frees its slot.
//!
//! Names: tg's random human name (`first_names_male` + `last_names`, the body is male), unique
//! among players, given when the client enters the lobby and kept per connection. The token stays
//! the account.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::jobs::{
    Department, JOBS_FILE, Job, JobsData, Outfit, OutfitSlot, STAFFING_PRIORITY_NEGLIGIBLE,
};
use ss13_mapload::spawner::Rng;
use ss13_mapload::{MapLoadPlugin, MapSeed, Template, read_manifest};
use ss13_net::server::{IntentReceived, SendNotice};
use ss13_shared::TilePos;
use ss13_shared::protocol::{ClientIntent, ServerNotice};
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::job_selection::{
    DepartmentSlots, DepartmentStatic, JobSelectionData, JobSelectionStatic, JobSelectionUi,
    JobSlots, JobStatic, RANDOM_JOB, SELECT_JOB,
};
use ss13_tgui::interfaces::ru_jobs::job_ru;
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};

use crate::lobby::GameState;

use crate::human::{ASSISTANT_SHOES, ASSISTANT_UNIFORM};
use crate::inventory::ASSISTANT_ACCESS;
use crate::outfit::{OutfitTemplates, equip};
use crate::round::NewPlayer;
use crate::server::{PlayerMob, SpawnPoints};

/// tgui window title (`new(user, src, "JobSelection", "Latejoin Menu")`).
pub const WINDOW_TITLE: &str = "Latejoin Menu";
/// The default job's card when there is no `jobs.ron`: `/datum/job` gives every job this ID.
const FALLBACK_ID: &str = "/obj/item/card/id/advanced";
/// Attempts at a name no player has before accepting a duplicate.
const NAME_ATTEMPTS: usize = 16;

pub struct JobsServerPlugin;

/// Loads jobs and outfit templates, after the map's landmarks are recorded.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct JobsStartup;

impl Plugin for JobsServerPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui::<JobSelectionWindow>()
            .init_resource::<JobSpawns>()
            .add_systems(
                Startup,
                (load_jobs, build_outfit_templates)
                    .chain()
                    .in_set(JobsStartup),
            )
            .add_systems(
                Update,
                (
                    open_menus.before(UiSystems::Open),
                    apply_choices.in_set(UiSystems::Handle),
                ),
            )
            .add_observer(free_slot);
    }
}

/// The jobs of this round and their filled slots (tg `SSjob.joinable_occupations` and each job's
/// `current_positions`).
#[derive(Resource)]
pub struct Jobs {
    pub data: JobsData,
    positions: Vec<u32>,
    /// Index of the overflow role, which every new client gets.
    pub default_job: usize,
    pub(crate) static_data: Arc<JobSelectionStatic>,
    pub(crate) rng: Rng,
}

impl Jobs {
    pub fn new(data: JobsData, seed: u64) -> Self {
        let default_job = data.job("Assistant").unwrap_or(0);
        let static_data = Arc::new(static_data(&data));
        Self {
            positions: vec![0; data.jobs.len()],
            data,
            default_job,
            static_data,
            rng: Rng::new(seed ^ 0x6a6f_6273),
        }
    }

    /// A copy for a dry run of `divide_occupations` (tg `pure = TRUE`), rolling from its own
    /// seed taken from this one.
    pub(crate) fn snapshot(&mut self) -> Self {
        Self {
            data: self.data.clone(),
            positions: self.positions.clone(),
            default_job: self.default_job,
            static_data: self.static_data.clone(),
            rng: Rng::new(self.rng.next_u64()),
        }
    }

    /// Living player mobs with this job.
    pub fn positions(&self, job: usize) -> u32 {
        self.positions.get(job).copied().unwrap_or(0)
    }

    /// One more body holds a slot of `job` (tg `current_positions++`).
    pub(crate) fn add_position(&mut self, job: usize) {
        self.positions[job] += 1;
    }

    /// tg `IsJobUnavailable(latejoin = TRUE)` + `get_job_unavailable_error_message`: `None` if
    /// the job can be taken now.
    pub fn unavailable(&self, job: usize) -> Option<String> {
        let j = &self.data.jobs[job];
        let ru = job_ru(&j.title).unwrap_or(&j.title);
        if !j.unresolved.is_empty() {
            return Some(format!("{ru} недоступна для выбора."));
        }
        let full = j.total_positions >= 0 && self.positions(job) >= j.total_positions as u32;
        full.then(|| format!("Роль {ru} уже заполнена до максимума."))
    }

    fn data(&self, round_secs: f64) -> JobSelectionData {
        let departments = self
            .data
            .departments
            .iter()
            .map(|d| department_slots(self, d))
            .collect();
        JobSelectionData {
            departments,
            round_duration: display_time_text(round_secs),
            shuttle_status: String::new(),
        }
    }

    /// tg `random_unique_name(MALE)`: a name no current player has, if one is found.
    pub(crate) fn random_name(&mut self, taken: &HashSet<String>) -> Option<String> {
        let names = &self.data.names;
        if names.first_male.is_empty() || names.last.is_empty() {
            return None;
        }
        let mut name = String::new();
        for _ in 0..NAME_ATTEMPTS {
            let first =
                &names.first_male[self.rng.range(0, names.first_male.len() as i64 - 1) as usize];
            let last = &names.last[self.rng.range(0, names.last.len() as i64 - 1) as usize];
            name = format!("{first} {last}");
            if !taken.contains(&name) {
                break;
            }
        }
        Some(name)
    }
}

fn static_data(data: &JobsData) -> JobSelectionStatic {
    JobSelectionStatic {
        departments: data
            .departments
            .iter()
            .map(|d| DepartmentStatic {
                name: d.name.clone(),
                color: d.color.clone(),
                jobs: d
                    .jobs
                    .iter()
                    .map(|&j| {
                        let job = &data.jobs[j as usize];
                        JobStatic {
                            title: job.title.clone(),
                            command: job.command,
                            description: job.description.clone(),
                        }
                    })
                    .collect(),
            })
            .collect(),
    }
}

/// `latejoin_menu/ui_data` for one department.
fn department_slots(jobs: &Jobs, department: &Department) -> DepartmentSlots {
    let mut open = Some(0u32);
    let slots = department
        .jobs
        .iter()
        .map(|&j| {
            let j = j as usize;
            let job = &jobs.data.jobs[j];
            let total = u32::try_from(job.total_positions).ok();
            let used = jobs.positions(j);
            match total {
                None => open = None,
                Some(total) => {
                    if let Some(open) = &mut open {
                        *open += total.saturating_sub(used);
                    }
                }
            }
            JobSlots {
                used_slots: used,
                open_slots: total,
                prioritized: false,
                unavailable_reason: jobs.unavailable(j),
            }
        })
        .collect();
    DepartmentSlots {
        open_slots: open,
        jobs: slots,
    }
}

/// tg `declension_ru(num, one, few, many)`.
pub(crate) fn declension_ru(
    n: u64,
    one: &'static str,
    few: &'static str,
    many: &'static str,
) -> &'static str {
    match (n % 10, n % 100) {
        (1, r) if r != 11 => one,
        (2..=4, r) if !(12..=14).contains(&r) => few,
        _ => many,
    }
}

/// BandaStation `DisplayTimeText(time, round_seconds_to = 1)`, in seconds. tg declines hours and
/// days by the minute count; kept.
pub fn display_time_text(secs: f64) -> String {
    let second = secs.max(0.0).floor() as u64;
    if second == 0 {
        return "сейчас же".into();
    }
    let seconds = |s: u64| format!("{s} {}", declension_ru(s, "секунда", "секунды", "секунд"));
    let minutes = |m: u64| format!("{m} {}", declension_ru(m, "минута", "минуты", "минут"));
    if second < 60 {
        return seconds(second);
    }
    let minute = second / 60;
    let second = second % 60;
    let second_t = if second > 0 {
        format!(" и {}", seconds(second))
    } else {
        String::new()
    };
    if minute < 60 {
        return format!("{}{second_t}", minutes(minute));
    }
    let hour = minute / 60;
    let minute = minute % 60;
    let minute_t = if minute > 0 {
        format!(", {}", minutes(minute))
    } else {
        String::new()
    };
    let hours = |h: u64| format!("{h} {}", declension_ru(minute, "час", "часа", "часов"));
    if hour < 24 {
        return format!("{}{minute_t}{second_t}", hours(hour));
    }
    let day = hour / 24;
    let hour = hour % 24;
    let hour_t = if hour > 0 {
        format!(", {}", hours(hour))
    } else {
        String::new()
    };
    let days = declension_ru(minute, "день", "дня", "дней");
    format!("{day} {days}{hour_t}{minute_t}{second_t}")
}

/// Start landmarks per job title (`/obj/effect/landmark/start` whose `name` is the title),
/// recorded at map load by [`crate::server`].
#[derive(Resource, Default, Debug)]
pub struct JobSpawns {
    pub by_title: HashMap<String, Vec<TilePos>>,
    /// `/obj/effect/landmark/start/ai` without `/secondary` (tg `primary_ai`).
    pub primary_ai: Vec<TilePos>,
    next: HashMap<String, usize>,
}

/// On a player mob: its job, an index into [`JobsData::jobs`]. Holds the job's slot.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerJob(pub usize);

/// On a client connection: its character's name, kept across the job choice.
#[derive(Component, Clone, Debug)]
pub struct CharacterName(pub String);

/// On a client connection the menu was opened for (tgui's source): the static half; `ui_data`
/// reads [`Jobs`]. Removed when the client joins, which closes the window.
#[derive(Component, Clone, Debug)]
pub struct JobBoard(Arc<JobSelectionStatic>);

/// The server side of `JobSelection` (same name and data as [`JobSelectionUi`]).
pub struct JobSelectionWindow;

impl UiInterface for JobSelectionWindow {
    const NAME: &'static str = JobSelectionUi::NAME;
    type Data = JobSelectionData;
    type StaticData = JobSelectionStatic;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JobAct {
    /// A title, or [`RANDOM_JOB`].
    Select(String),
}

impl ServerUi for JobSelectionWindow {
    type Source = &'static JobBoard;
    type UserParam = (
        Option<Res<'static, Jobs>>,
        Res<'static, Time>,
        Res<'static, crate::round::Ticker>,
    );
    type Act = JobAct;

    fn static_data(board: &JobBoard) -> JobSelectionStatic {
        (*board.0).clone()
    }

    /// Slots and the round time; tgui sends it when it changed, at most every `REFRESH_SECS`.
    fn data(
        _: &JobBoard,
        _: Entity,
        (jobs, time, ticker): &(Option<Res<Jobs>>, Res<Time>, Res<crate::round::Ticker>),
    ) -> JobSelectionData {
        jobs.as_ref()
            .map(|jobs| jobs.data(ticker.round_duration(time.elapsed_secs_f64())))
            .unwrap_or_default()
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<JobAct, UiReject> {
        match action {
            SELECT_JOB => params
                .get("job")
                .map(|job| JobAct::Select(job.to_owned()))
                .ok_or_else(|| UiReject::BadParams("job: missing".into())),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

/// The default job when no `jobs.ron` exists: the assistant kit the server always had.
pub fn fallback_jobs() -> JobsData {
    let item = |slot, path: &str| (slot, path.to_owned());
    JobsData {
        departments: vec![Department {
            path: "/datum/job_department/undefined".into(),
            name: "No Department".into(),
            color: "#9689db".into(),
            jobs: vec![0],
        }],
        jobs: vec![Job {
            path: "/datum/job/assistant".into(),
            title: "Assistant".into(),
            total_positions: -1,
            spawn_positions: -1,
            outfit: Some(Outfit {
                path: "/datum/outfit/job/assistant".into(),
                items: vec![
                    item(OutfitSlot::Uniform, ASSISTANT_UNIFORM),
                    item(OutfitSlot::Shoes, ASSISTANT_SHOES),
                    item(OutfitSlot::Id, FALLBACK_ID),
                ],
                // `/datum/outfit/job::box`.
                backpack_contents: vec![(ss13_mapload::RUNTIME_CONTAINER_TYPES[1].into(), 1)],
            }),
            trim: Some("/datum/id_trim/job/assistant".into()),
            assignment: "Assistant".into(),
            access: ASSISTANT_ACCESS.map(str::to_owned).to_vec(),
            staffing_priority: STAFFING_PRIORITY_NEGLIGIBLE,
            // `/datum/job/assistant`: `PAYCHECK_LOWER` from `ACCOUNT_CIV`.
            paycheck: 25,
            paycheck_department: "CIV".into(),
            template_access: ["captain", "change_ids", "hop"].map(str::to_owned).to_vec(),
            sechud_icon_state: "hudassistant".into(),
            ..default()
        }],
        names: default(),
    }
}

fn load_jobs(world: &mut World) {
    if world.contains_resource::<Jobs>() {
        return; // inserted by a test
    }
    let path = world
        .get_resource::<MapLoadPlugin>()
        .map(|m| m.assets.join(JOBS_FILE));
    let data = match path.as_deref().map(JobsData::read) {
        Some(Ok(data)) if !data.jobs.is_empty() => data,
        Some(Err(e)) if path.as_deref().is_some_and(std::path::Path::exists) => {
            error!("{e:#}; only the assistant is available");
            fallback_jobs()
        }
        _ => {
            warn!("no {JOBS_FILE} (rerun `ss13-mapload convert`); only the assistant is available");
            fallback_jobs()
        }
    };
    let playable = data.jobs.iter().filter(|j| j.unresolved.is_empty()).count();
    let spawns = world.resource::<JobSpawns>();
    let with_landmark = data
        .jobs
        .iter()
        .filter(|j| spawns.by_title.get(&j.title).is_some_and(|t| !t.is_empty()))
        .count();
    info!(
        "{} jobs ({playable} playable, {with_landmark} with a start landmark on the map)",
        data.jobs.len()
    );
    let seed = world.get_resource::<MapSeed>().map_or(0, |s| s.0);
    world.insert_resource(Jobs::new(data, seed));
}

fn build_outfit_templates(world: &mut World) {
    let items: Vec<String> = {
        let jobs = world.resource::<Jobs>();
        jobs.data
            .outfit_items()
            .into_iter()
            // The items quirks give at spawn (`crate::quirks`).
            .chain(crate::quirks::ITEM_PATHS)
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let Some(assets) = world
        .get_resource::<MapLoadPlugin>()
        .map(|m| m.assets.clone())
    else {
        world.init_resource::<OutfitTemplates>();
        return;
    };
    let manifest = match read_manifest(&assets) {
        Ok(manifest) => manifest,
        Err(e) => {
            error!("outfits: {e:#}");
            world.init_resource::<OutfitTemplates>();
            return;
        }
    };
    let index = manifest.index();
    let protos = world.resource::<ss13_proto::Prototypes>();
    let mut missing = 0;
    let templates: HashMap<String, Template> = items
        .into_iter()
        .filter_map(|path| {
            let proto = protos.get(&path).cloned();
            missing += usize::from(proto.is_none());
            Some((path, Template::new(proto?, &index)))
        })
        .collect();
    if missing > 0 {
        warn!("{missing} outfit items have no prototype (rerun `ss13-mapload convert`)");
    }
    world.insert_resource(OutfitTemplates(templates));
}

/// The client's character name, given once per connection (tg `random_unique_name(MALE)`).
pub fn character_name(world: &mut World, client: Entity) -> String {
    if let Some(name) = world.get::<CharacterName>(client) {
        return name.0.clone();
    }
    let mut named = world.query::<&CharacterName>();
    let mut taken: HashSet<String> = named.iter(world).map(|n| n.0.clone()).collect();
    let mut players = world.query_filtered::<&Name, With<PlayerMob>>();
    taken.extend(players.iter(world).map(|n| n.to_string()));
    let name = world
        .resource_mut::<Jobs>()
        .random_name(&taken)
        .unwrap_or_else(|| format!("Игрок {}", client.index()));
    world.entity_mut(client).insert(CharacterName(name.clone()));
    name
}

/// Gives `client` a body of `job`, replacing its current one (and taking it out of the lobby):
/// tg `create_character` + `transfer_character` + `equip_rank` after `assign_role`. A latejoin
/// spawns on a latejoin landmark (tg `get_latejoin_spawn_point`), round start on the job's start
/// landmark, else a latejoin one (`get_roundstart_spawn_point`); dev spawns win over both.
pub fn join(world: &mut World, client: Entity, job: usize, latejoin: bool) -> Entity {
    let name = character_name(world, client);
    let def = world.resource::<Jobs>().data.jobs[job].clone();
    if let Some(mob) = crate::silicon::join(world, client, job, &def, &name, latejoin) {
        world.entity_mut(client).remove::<(NewPlayer, JobBoard)>();
        return mob;
    }
    let forced =
        crate::server::dev_spawn(world, client).or_else(|| world.resource::<SpawnPoints>().forced);
    // `get_roundstart_spawn_point`: `STATION_TRAIT_LATE_ARRIVALS` sends everyone to latejoin.
    let at_job_spawn =
        !latejoin && !crate::station_traits::has(world, crate::station_traits::LATE_ARRIVALS);
    if latejoin && forced.is_none() {
        crate::arrivals::require_undocked(world);
    }
    let pos = forced
        .or_else(|| at_job_spawn.then(|| job_spawn(world, &def.title)).flatten())
        .unwrap_or_else(|| crate::server::pick_latejoin(world));
    let mob = crate::server::spawn_mob(world, client, pos, name.clone());
    world.resource_mut::<Jobs>().add_position(job);
    world.entity_mut(mob).insert(PlayerJob(job));
    let mut inventory = equip(world, mob, &def, &name);
    // `pre_equip_species_outfit`: what the species needs to live (plasmamen).
    crate::species::equip_species_outfit(world, mob, &mut inventory);
    crate::cargo::economy::open_job_account(world, &def, &name, inventory.id);
    // shortcut: every job gets storage's grey backpack, not the outfit's department `back`
    // (`pre_equip` by backpack preference); spawn that once storage builds templates for it.
    inventory.back = crate::storage::spawn_backpack(world, mob);
    if let (Some(pack), Some(outfit)) = (inventory.back, &def.outfit) {
        crate::outfit::fill_backpack(world, pack, outfit);
    }
    world.entity_mut(mob).insert(inventory);
    crate::modpc::pda::post_equip(world, mob, &name, &def.assignment);
    // tg `equip_characters` / `AttemptLateSpawn`: `SSquirks.AssignQuirks` after the outfit.
    crate::quirks::assign(world, client, mob);
    // `/datum/job/chaplain/after_spawn`: the bible, the religion's names.
    crate::service::chaplain_spawned(world, client, mob);
    crate::records::manifest::inject_joiner(world, mob, job);
    world.entity_mut(client).remove::<(NewPlayer, JobBoard)>();
    info!(
        "{name} ({client}) joined as {} at {pos:?}{}",
        def.title,
        if latejoin { " (latejoin)" } else { "" }
    );
    mob
}

/// The next free start landmark of `title`, round robin (tg takes the first unused one).
pub(crate) fn job_spawn(world: &mut World, title: &str) -> Option<TilePos> {
    let spawns = world.resource::<JobSpawns>();
    let tiles = spawns
        .by_title
        .get(title)
        .filter(|t| !t.is_empty())?
        .clone();
    let next = spawns.next.get(title).copied().unwrap_or(0);
    let (i, pos) = crate::server::pick_free(world, &tiles, next);
    world
        .resource_mut::<JobSpawns>()
        .next
        .insert(title.to_owned(), i + 1);
    Some(pos)
}

fn free_slot(
    remove: On<Remove<PlayerJob>>,
    jobs_of: Query<&PlayerJob>,
    jobs: Option<ResMut<Jobs>>,
) {
    if let (Ok(&PlayerJob(job)), Some(mut jobs)) = (jobs_of.get(remove.entity), jobs)
        && let Some(n) = jobs.positions.get_mut(job)
    {
        *n = n.saturating_sub(1);
    }
}

fn notice(client: Entity, text: impl Into<String>) -> SendNotice {
    SendNotice {
        client,
        notice: ServerNotice { text: text.into() },
    }
}

/// `LateJoinMenu` (the lobby's "Join game", tg `late_join` href) -> the window, for a client
/// still in the lobby.
fn open_menus(
    mut intents: MessageReader<IntentReceived>,
    new_players: Query<(), With<NewPlayer>>,
    ticker: Res<crate::round::Ticker>,
    jobs: Option<Res<Jobs>>,
    mut open: MessageWriter<OpenUi>,
    mut notices: MessageWriter<SendNotice>,
    mut commands: Commands,
) {
    let Some(jobs) = jobs else {
        return;
    };
    for intent in intents.read() {
        if intent.intent != ClientIntent::LateJoinMenu {
            continue;
        }
        let client = intent.client;
        // tg: only a `new_player` has the menu (a body or a ghost does not).
        if !new_players.contains(client) {
            continue;
        }
        if ticker.state() == GameState::Finished {
            notices.write(notice(
                client,
                "Невозможно присоединиться, игра уже окончена!",
            ));
            continue;
        }
        commands
            .entity(client)
            .insert(JobBoard(jobs.static_data.clone()));
        open.write(OpenUi::new::<JobSelectionWindow>(
            client,
            client,
            client,
            WINDOW_TITLE,
        ));
    }
}

/// `select_job`, checked at the moment of effect.
fn apply_choices(world: &mut World, mut acts: Local<MessageCursor<UiActed<JobSelectionWindow>>>) {
    let acted: Vec<(Entity, JobAct)> = acts
        .read(world.resource::<Messages<UiActed<JobSelectionWindow>>>())
        .map(|a| (a.client, a.act.clone()))
        .collect();
    for (client, JobAct::Select(title)) in acted {
        // Joined already (an earlier act this tick, round start), or observing.
        if world.get::<NewPlayer>(client).is_none() {
            continue;
        }
        let verdict = choose(world, &title);
        match verdict {
            Ok(job) => {
                let mob = join(world, client, job, true);
                // `COMSIG_GLOB_JOB_AFTER_LATEJOIN_SPAWN` and `SSdynamic.on_latejoin`.
                crate::antag::revolution::server::on_latejoin(world, mob);
                crate::antag::midround::on_latejoin(world, mob);
            }
            Err(reason) => {
                world.write_message(notice(client, reason));
            }
        }
    }
}

/// The job `title` names, or why it cannot be taken now (tg `select_job` + `AttemptLateSpawn`).
fn choose(world: &mut World, title: &str) -> Result<usize, String> {
    if world.resource::<crate::round::Ticker>().state() != GameState::Playing {
        return Err("The round is either not ready, or has already finished...".into());
    }
    let jobs = world.resource::<Jobs>();
    if title == RANDOM_JOB {
        // tg `get_random_job`: any job the player could pick now.
        let open: Vec<usize> = (0..jobs.data.jobs.len())
            .filter(|&j| jobs.unavailable(j).is_none())
            .collect();
        if open.is_empty() {
            return Err(
                "В данный момент для вас нет случайной профессии. Обратитесь в ahelp за помощью."
                    .into(),
            );
        }
        let mut jobs = world.resource_mut::<Jobs>();
        let pick = jobs.rng.range(0, open.len() as i64 - 1) as usize;
        return Ok(open[pick]);
    }
    let Some(job) = jobs.data.job(title) else {
        return Err(format!("Профессии «{title}» нет."));
    };
    match jobs.unavailable(job) {
        Some(reason) => Err(reason),
        None => Ok(job),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_text_matches_display_time_text() {
        assert_eq!(display_time_text(0.4), "сейчас же");
        assert_eq!(display_time_text(1.0), "1 секунда");
        assert_eq!(display_time_text(22.0), "22 секунды");
        assert_eq!(display_time_text(60.0), "1 минута");
        assert_eq!(display_time_text(125.0), "2 минуты и 5 секунд");
        // tg declines the hour by the minute count.
        assert_eq!(display_time_text(3600.0 + 11.0 * 60.0), "1 часов, 11 минут");
        assert_eq!(display_time_text(3600.0 + 60.0), "1 час, 1 минута");
    }

    fn officer_data(total: i32) -> JobsData {
        let mut data = fallback_jobs();
        let mut officer = data.jobs[0].clone();
        officer.title = "Security Officer".into();
        officer.total_positions = total;
        officer.access = vec!["security".into(), "brig".into()];
        data.jobs.push(officer);
        let mut ai = data.jobs[0].clone();
        ai.title = "AI".into();
        ai.total_positions = 1;
        ai.outfit = None;
        ai.unresolved = vec!["no outfit".into()];
        data.jobs.push(ai);
        data.departments.push(Department {
            name: "Security".into(),
            jobs: vec![1, 2],
            ..default()
        });
        data
    }

    #[test]
    fn slots_and_unavailable_reasons() {
        let mut jobs = Jobs::new(officer_data(1), 1);
        assert_eq!(jobs.default_job, 0);
        assert_eq!(jobs.unavailable(1), None);
        jobs.positions[1] = 1;
        assert_eq!(
            jobs.unavailable(1).as_deref(),
            Some("Роль Офицер уже заполнена до максимума.")
        );
        assert_eq!(
            jobs.unavailable(2).as_deref(),
            Some("ИИ недоступна для выбора.")
        );
        // The overflow role never fills.
        jobs.positions[0] = 1000;
        assert_eq!(jobs.unavailable(0), None);
        let data = jobs.data(0.0);
        assert_eq!(data.departments[0].open_slots, None);
        assert_eq!(data.departments[1].open_slots, Some(1));
        assert_eq!(data.departments[1].jobs[0].used_slots, 1);
        assert_eq!(data.departments[1].jobs[0].open_slots, Some(1));
    }

    #[test]
    fn random_names_avoid_taken_ones() {
        let mut data = fallback_jobs();
        data.names.first_male = vec!["Аарон".into(), "Абель".into()];
        data.names.last = vec!["Адамс".into()];
        let mut jobs = Jobs::new(data, 7);
        let taken = HashSet::from(["Аарон Адамс".to_owned()]);
        for _ in 0..20 {
            assert_eq!(jobs.random_name(&taken).as_deref(), Some("Абель Адамс"));
        }
        assert!(Jobs::new(fallback_jobs(), 0).random_name(&taken).is_none());
    }
}
