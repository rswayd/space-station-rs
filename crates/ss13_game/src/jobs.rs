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

use crate::human::AppearancePrefs;
use crate::human::{ASSISTANT_SHOES, ASSISTANT_UNIFORM};
use crate::inventory::ASSISTANT_ACCESS;
use crate::outfit::{OutfitTemplates, equip};
use crate::round::{JobPreferences, NewPlayer, RoundConfig};
use crate::server::{PlayerMob, SpawnPoints};

pub mod security;

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
            .init_resource::<security::SecurityDistribution>()
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
                    validate_job_restrictions.after(UiSystems::Handle),
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
    /// tg `SSjob.assigned_captain`: someone was made captain or acting captain this shift.
    pub assigned_captain: bool,
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
            assigned_captain: false,
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
            assigned_captain: self.assigned_captain,
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

    /// One slot of `job` opens again (tg `current_positions = max(current_positions - 1, 0)`).
    pub(crate) fn free_position(&mut self, job: usize) {
        if let Some(n) = self.positions.get_mut(job) {
            *n = n.saturating_sub(1);
        }
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
    /// `/obj/effect/landmark/secequipment` (tg `GLOB.secequipment`): where extra officer lockers
    /// go, in map order.
    pub secequipment: Vec<TilePos>,
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
                accessory: None,
                implants: Vec::new(),
                belt_contents: Vec::new(),
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
            .map(str::to_owned)
            // The items quirks give at spawn (`crate::quirks`).
            .chain(crate::quirks::item_paths())
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
    // `/datum/job/security_officer`: the department, and at round start its post (`move_to`).
    let department =
        (def.title == security::SECURITY_OFFICER).then(|| security::pick_department(world));
    let pos = forced
        .or_else(|| {
            department
                .filter(|_| !latejoin)
                .and_then(|d| security::post_tile(world, d))
        })
        .or_else(|| at_job_spawn.then(|| job_spawn(world, &def.title)).flatten())
        .unwrap_or_else(|| crate::server::pick_latejoin(world));
    let mob = crate::server::spawn_mob(world, client, pos, name.clone());
    world.resource_mut::<Jobs>().add_position(job);
    world.entity_mut(mob).insert(PlayerJob(job));
    let mut inventory = equip(world, mob, &def, &name);
    // `pre_equip_species_outfit`: what the species needs to live (plasmamen).
    crate::species::equip_species_outfit(world, mob, &mut inventory);
    crate::cargo::economy::open_job_account(world, &def, &name, inventory.id);
    // The outfit's `back`: the department backpack (`pre_equip`, DBACKPACK); the grey one
    // without it.
    // shortcut: the backpack preference (satchel, duffel, messenger) is not read.
    let outfit_back = def
        .outfit
        .as_ref()
        .and_then(|o| o.item(ss13_mapload::jobs::OutfitSlot::Back))
        .map(str::to_owned);
    inventory.back = outfit_back
        .and_then(|path| crate::outfit::spawn_back(world, mob, &path))
        .or_else(|| crate::storage::spawn_backpack(world, mob));
    if let (Some(pack), Some(outfit)) = (inventory.back, &def.outfit) {
        crate::outfit::fill_backpack(world, pack, outfit);
    }
    // tg `equip`: `belt_contents` into the belt's storage.
    if let (Some(belt), Some(outfit)) = (inventory.belt, &def.outfit) {
        crate::outfit::fill_storage(world, belt, &outfit.belt_contents, &outfit.path);
    }
    world.entity_mut(mob).insert(inventory);
    // `/datum/outfit/job/detective/post_equip`: the cigarette is lit.
    if def
        .outfit
        .as_ref()
        .is_some_and(|o| o.path == security::DETECTIVE_OUTFIT)
    {
        security::light_mask_cigarette(world, mob);
    }
    // `setup_department`: the department's armband, headset and trim (the PDA follows it).
    let assignment = match department {
        Some(d) => security::setup_department(world, client, mob, &def, d),
        None => def.assignment.clone(),
    };
    crate::modpc::pda::post_equip(world, mob, &name, &assignment);
    // tg `equip_characters` / `AttemptLateSpawn`: `SSquirks.AssignQuirks` after the outfit.
    crate::quirks::assign(world, client, mob);
    // `/datum/preference/personality/apply_to_human`.
    crate::personality::assign(world, client, mob);
    // `/datum/outfit/job/clown/post_equip`: the clown is clumsy.
    if def.outfit.as_ref().is_some_and(|o| o.path == CLOWN_OUTFIT) {
        crate::genetics::make_clown_clumsy(world, mob);
    }
    // `/datum/job/chaplain/after_spawn`: the bible, the religion's names.
    crate::service::chaplain_spawned(world, client, mob);
    // `/datum/outfit/job/curator/post_equip`: `grant_all_languages(source = LANGUAGE_CURATOR)`.
    if def.path == "/datum/job/curator"
        && let Some(mut holder) = world.get_mut::<ss13_chat::language::LanguageHolder>(mob)
    {
        holder.grant_all(ss13_chat::language::LanguageSource::Mind);
    }
    crate::records::manifest::inject_joiner(world, mob, job);
    // `/datum/job/prisoner/handle_prisoner_joining`.
    if def.title == security::PRISONER {
        security::prisoner_joined(world, client, &name);
    }
    // `equip_rank`: `job.announce_job(equipping)`.
    announce_head(world, mob, &name, &def.title);
    // `AttemptLateSpawn`: `SSshuttle.arrivals.QueueAnnounce(humanc, rank)`, the captaincy.
    if latejoin {
        // `/datum/job/security_officer/after_latejoin_spawn`.
        if let Some(d) = department {
            security::announce_latejoin(world, &name, d);
        }
        crate::arrivals::queue_announce(world, mob, &def.title);
        latejoin_captaincy(world, mob, &name, &def.title);
    }
    world.entity_mut(client).remove::<(NewPlayer, JobBoard)>();
    info!(
        "{name} ({client}) joined as {} at {pos:?}{}",
        def.title,
        if latejoin { " (latejoin)" } else { "" }
    );
    mob
}

/// `/datum/job/clown`'s `outfit`.
const CLOWN_OUTFIT: &str = "/datum/outfit/job/clown";
/// tg `JOB_CAPTAIN` (`is_captain_job`).
const CAPTAIN: &str = "Captain";
/// The jobs' `head_announce`: the channel announcing a new head of that department.
const HEAD_ANNOUNCE: [(&str, u16); 7] = [
    ("Head of Personnel", ss13_chat::radio::FREQ_SERVICE),
    ("Head of Security", ss13_chat::radio::FREQ_SECURITY),
    ("Chief Engineer", ss13_chat::radio::FREQ_ENGINEERING),
    ("Chief Medical Officer", ss13_chat::radio::FREQ_MEDICAL),
    ("Research Director", ss13_chat::radio::FREQ_SCIENCE),
    ("Quartermaster", ss13_chat::radio::FREQ_SUPPLY),
    // BandaStation `/datum/job/magistrate`.
    ("Magistrate", ss13_chat::radio::FREQ_JUSTICE),
];
/// tg `SSjob.chain_of_command`: who becomes acting captain without one, lowest first.
const CHAIN_OF_COMMAND: [(&str, u8); 7] = [
    (CAPTAIN, 1),
    ("Head of Personnel", 2),
    ("Research Director", 3),
    ("Chief Engineer", 4),
    ("Chief Medical Officer", 5),
    ("Head of Security", 6),
    ("Quartermaster", 7),
];
/// tg `ACCESS_COMMAND`, forced onto a promoted captain's card.
const ACCESS_COMMAND: &str = "command";

fn chain_rank(title: &str) -> Option<u8> {
    CHAIN_OF_COMMAND
        .iter()
        .find(|(t, _)| *t == title)
        .map(|&(_, r)| r)
}

/// `get_captaincy_announcement`: the captain's, else the acting captain's.
pub fn captaincy_announcement(captain_job: bool, name: &str) -> String {
    if captain_job {
        format!("Капитан {name} на борту!")
    } else {
        format!(
            "В связи с острой нехваткой персонала, недавно назначенный исполняющий обязанности \
             капитана {name} на борту!"
        )
    }
}

/// tg `announce_head`: the AAS's `/datum/aas_config_entry/newhead` line on the job's
/// `head_announce` channel. tg delays it a tick past the round start (after the captain's); the
/// radio delivers it after this tick's announcements anyway.
fn announce_head(world: &mut World, mob: Entity, name: &str, title: &str) {
    let Some(&(_, freq)) = HEAD_ANNOUNCE.iter().find(|(t, _)| *t == title) else {
        return;
    };
    if world.get_entity(mob).is_err() {
        return;
    }
    let text = format!("{name}, {title} назначен(а) главой отдела.");
    crate::arrivals::aas_announce_on(world, freq, &text);
}

/// `AttemptLateSpawn`'s captaincy: a captain is announced (the announcement sound); without a
/// captain so far, a latejoining head of the chain of command becomes acting captain.
fn latejoin_captaincy(world: &mut World, mob: Entity, name: &str, title: &str) {
    let Some(full) = latejoin_captain(title, world.resource::<Jobs>().assigned_captain) else {
        return;
    };
    let acting = !full;
    let sound = if full {
        crate::announce::ANNOUNCE_SOUND
    } else {
        crate::security_level::NOTICE
    };
    let text = captaincy_announcement(full, name);
    crate::security_level::minor_announce_with_sound(world, "", &text, sound);
    promote_to_captain(world, mob, acting);
}

/// `AttemptLateSpawn`'s `is_captain`: `Some(true)` for a Captain, `Some(false)` for an acting
/// captain (in the chain of command while nobody was made captain), `None` otherwise.
fn latejoin_captain(title: &str, assigned_captain: bool) -> Option<bool> {
    if title == CAPTAIN {
        Some(true)
    } else {
        (!assigned_captain && chain_rank(title).is_some()).then_some(false)
    }
}

/// tg `SSticker.equip_characters`' spare ID pick: of the spawned crew (client, mob, job), the one
/// highest in the chain of command (a random one of equals) becomes captain, acting unless a
/// Captain; nobody: every spawned crew member hears so.
pub fn roundstart_captaincy(world: &mut World, spawned: &[(Entity, Entity, usize)]) {
    let ranked: Vec<(usize, u8)> = {
        let jobs = world.resource::<Jobs>();
        spawned
            .iter()
            .enumerate()
            .filter_map(|(i, &(_, _, job))| {
                chain_rank(&jobs.data.jobs.get(job)?.title).map(|r| (i, r))
            })
            .collect()
    };
    let Some(best) = ranked.iter().map(|&(_, r)| r).min() else {
        let clients: Vec<Entity> = spawned
            .iter()
            .filter(|&&(_, mob, _)| world.get::<crate::silicon::Silicon>(mob).is_none())
            .map(|&(client, _, _)| client)
            .collect();
        if !clients.is_empty() {
            world.write_message(ss13_net::chat::server::SendChat {
                clients,
                message: ss13_shared::chat::ChatMessage::simple(
                    ss13_shared::chat::ChatChannel::System,
                    ss13_shared::chat::Span::Notice,
                    "Капитанство никому не назначено.",
                ),
            });
        }
        return;
    };
    let candidates: Vec<usize> = ranked
        .iter()
        .filter(|&&(_, r)| r == best)
        .map(|&(i, _)| i)
        .collect();
    let pick = {
        let mut jobs = world.resource_mut::<Jobs>();
        candidates[jobs.rng.range(0, candidates.len() as i64 - 1) as usize]
    };
    let (_, mob, job) = spawned[pick];
    let full = world.resource::<Jobs>().data.jobs[job].title == CAPTAIN;
    promote_to_captain(world, mob, !full);
    let name = world
        .get::<Name>(mob)
        .map(|n| n.to_string())
        .unwrap_or_default();
    crate::security_level::minor_announce(world, "", &captaincy_announcement(full, &name), false);
}

/// tg `SSjob.promote_to_captain`: command access forced onto the ID card, the shift has its
/// captain.
// shortcut: no spare ID safe (`SSid_access.spare_id_safe_code`) here, so no code folder or
// memory: the full captain's message is only about it and is left out, the acting one's note
// part too. Add both with the spare ID safe.
pub fn promote_to_captain(world: &mut World, mob: Entity, acting: bool) {
    world.resource_mut::<Jobs>().assigned_captain = true;
    if let Some(card) = world
        .get::<crate::inventory::Inventory>(mob)
        .and_then(|inv| inv.id)
        .and_then(|item| crate::inventory::get_id(world, item))
        && let Some(mut card) = world.get_mut::<crate::inventory::IdCard>(card)
        && !card.access.iter().any(|a| a == ACCESS_COMMAND)
    {
        card.access.push(ACCESS_COMMAND.to_owned());
    }
    if acting && let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
        world.write_message(ss13_net::chat::server::SendChat {
            clients: vec![client],
            message: ss13_shared::chat::ChatMessage::simple(
                ss13_shared::chat::ChatChannel::System,
                ss13_shared::chat::Span::Notice,
                "Due to your position in the chain of command, you have been promoted to \
                 Acting Captain.",
            ),
        });
    }
}

/// BandaStation `is_job_allowed_for_species_with_string` (inverted): the `job_restrictions`
/// config keeps `title` from every species path outside `allowed_species`; an empty
/// `allowed_species` restricts nobody, and the xenobureaucracy error trait lifts it all.
pub fn species_restricted(
    config: &RoundConfig,
    xeno_error: bool,
    species: &str,
    title: &str,
) -> bool {
    !xeno_error
        && !config.allowed_species.is_empty()
        && config.job_restrictions.iter().any(|t| t == title)
        && !config.allowed_species.iter().any(|s| s == species)
}

/// The jobs of `data` that `species` may not take.
pub fn restricted_indexes(
    config: &RoundConfig,
    xeno_error: bool,
    species: &str,
    data: &JobsData,
) -> Vec<usize> {
    (0..data.jobs.len())
        .filter(|&j| species_restricted(config, xeno_error, species, &data.jobs[j].title))
        .collect()
}

/// The species path of the character `look` describes (tg `read_preference(species)`).
fn species_path(look: &AppearancePrefs) -> Option<&'static str> {
    crate::human::roundstart_species()
        .get(look.species)
        .map(|s| s.def().path)
}

/// The jobs `client`'s chosen species may not take (none without preferences: tg's
/// `if(!species) return TRUE`).
pub fn restricted_jobs(world: &World, client: Entity) -> Vec<usize> {
    let (Some(config), Some(jobs), Some(species)) = (
        world.get_resource::<RoundConfig>(),
        world.get_resource::<Jobs>(),
        world.get::<AppearancePrefs>(client).and_then(species_path),
    ) else {
        return Vec::new();
    };
    let xeno = crate::station_traits::has(world, crate::station_traits::XENOBUREAUCRACY_ERROR);
    restricted_indexes(config, xeno, species, &jobs.data)
}

/// The lobby's "Готов" check (BandaStation `new_player/Topic` `toggle_ready`): a wanted job the
/// species may not take.
pub fn prefs_restricted(world: &World, client: Entity) -> bool {
    let restricted = restricted_jobs(world, client);
    world
        .get::<JobPreferences>(client)
        .is_some_and(|p| p.levels.keys().any(|j| restricted.contains(j)))
}

/// BandaStation's ready-check message.
pub const SPECIES_INCOMPATIBLE: &str =
    "Выбранная раса несовместима с одной или более выбранных профессий.";

/// BandaStation `validate_job_restrictions`: a ready player whose species or wanted jobs
/// changed into a restricted pair is no longer ready (before the round sets up).
fn validate_job_restrictions(
    ticker: Option<Res<crate::round::Ticker>>,
    config: Option<Res<RoundConfig>>,
    jobs: Option<Res<Jobs>>,
    traits: Option<Res<crate::station_traits::StationTraits>>,
    mut players: Query<
        (Entity, &mut NewPlayer, &AppearancePrefs, &JobPreferences),
        Or<(Changed<AppearancePrefs>, Changed<JobPreferences>)>,
    >,
    mut chat: MessageWriter<ss13_net::chat::server::SendChat>,
) {
    let (Some(config), Some(jobs), Some(ticker)) = (config, jobs, ticker) else {
        return;
    };
    if ticker.state() >= GameState::SettingUp {
        return;
    }
    let xeno = traits.is_some_and(|t| t.has(crate::station_traits::XENOBUREAUCRACY_ERROR));
    for (client, mut player, look, prefs) in &mut players {
        let Some(species) = species_path(look).filter(|_| player.ready) else {
            continue;
        };
        let restricted = restricted_indexes(&config, xeno, species, &jobs.data);
        if prefs.levels.keys().any(|j| restricted.contains(j)) {
            player.ready = false;
            chat.write(ss13_net::chat::server::SendChat {
                clients: vec![client],
                message: ss13_shared::chat::ChatMessage::simple(
                    ss13_shared::chat::ChatChannel::System,
                    ss13_shared::chat::Span::Warning,
                    SPECIES_INCOMPATIBLE,
                ),
            });
        }
    }
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
    // A slot `round::reopen_roundstart_suicide_roles` already freed.
    jobs_of: Query<&PlayerJob, Without<crate::round::SlotReopened>>,
    jobs: Option<ResMut<Jobs>>,
) {
    if let (Ok(&PlayerJob(job)), Some(mut jobs)) = (jobs_of.get(remove.entity), jobs) {
        jobs.free_position(job);
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
#[allow(clippy::too_many_arguments)]
fn open_menus(
    mut intents: MessageReader<IntentReceived>,
    new_players: Query<(), With<NewPlayer>>,
    ticker: Res<crate::round::Ticker>,
    jobs: Option<Res<Jobs>>,
    mut open: MessageWriter<OpenUi>,
    mut notices: MessageWriter<SendNotice>,
    mut commands: Commands,
    (config, mut queue, living, mut chat): (
        Res<crate::round::RoundConfig>,
        ResMut<crate::join_queue::JoinQueue>,
        Query<Option<&crate::health::Stat>, (With<PlayerMob>, Without<crate::ghost::Ghost>)>,
        MessageWriter<ss13_net::chat::server::SendChat>,
    ),
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
        // The population cap: the button queues instead (`crate::join_queue`).
        let alive = living
            .iter()
            .filter(|s| s.copied() != Some(crate::health::Stat::Dead))
            .count() as u32;
        let cap = crate::join_queue::relevant_cap(&config);
        if ticker.state() == GameState::Playing
            && let Some(lines) = crate::join_queue::join_button(&mut queue, cap, alive, client)
        {
            for (span, text) in lines {
                chat.write(ss13_net::chat::server::SendChat {
                    clients: vec![client],
                    message: ss13_shared::chat::ChatMessage::simple(
                        ss13_shared::chat::ChatChannel::System,
                        span,
                        text,
                    ),
                });
            }
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

/// Opens the latejoin menu for a lobby client (the join queue's turn).
pub(crate) fn open_latejoin_menu(world: &mut World, client: Entity) {
    if world.get::<NewPlayer>(client).is_none() {
        return;
    }
    let Some(board) = world
        .get_resource::<Jobs>()
        .map(|jobs| JobBoard(jobs.static_data.clone()))
    else {
        return;
    };
    world.entity_mut(client).insert(board);
    world.write_message(OpenUi::new::<JobSelectionWindow>(
        client,
        client,
        client,
        WINDOW_TITLE,
    ));
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
        let verdict = choose(world, client, &title).and_then(|job| {
            // `select_job`'s queue check: only the first in line, below the cap.
            let cap =
                crate::join_queue::relevant_cap(world.resource::<crate::round::RoundConfig>());
            let living = crate::join_queue::living_player_count(world);
            let queue = world.resource::<crate::join_queue::JoinQueue>();
            if crate::join_queue::may_select(queue, cap, living, client) {
                Ok(job)
            } else {
                Err("Сервер заполнен!".to_owned())
            }
        });
        match verdict {
            Ok(job) => {
                // `AttemptLateSpawn`: out of the queue.
                crate::join_queue::joined(world, client);
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
fn choose(world: &mut World, client: Entity, title: &str) -> Result<usize, String> {
    if world.resource::<crate::round::Ticker>().state() != GameState::Playing {
        return Err("The round is either not ready, or has already finished...".into());
    }
    let restricted = restricted_jobs(world, client);
    let jobs = world.resource::<Jobs>();
    if title == RANDOM_JOB {
        // tg `get_random_job`: any job the player could pick now. BandaStation checks the
        // species against the "Random" title only, which lets a random pick land on a
        // restricted job; the restricted ones are left out here.
        let open: Vec<usize> = (0..jobs.data.jobs.len())
            .filter(|&j| jobs.unavailable(j).is_none() && !restricted.contains(&j))
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
    // BandaStation's `select_job` check, before `AttemptLateSpawn`'s.
    if restricted.contains(&job) {
        return Err("Выбранная раса несовместима с выбранной профессией!".into());
    }
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

    fn restricting(titles: &[&str], species: &[&str]) -> RoundConfig {
        RoundConfig {
            job_restrictions: titles.iter().map(|t| (*t).to_owned()).collect(),
            allowed_species: species.iter().map(|t| (*t).to_owned()).collect(),
            ..RoundConfig::tg()
        }
    }

    #[test]
    fn species_restrictions_follow_banda_config() {
        const HUMAN: &str = "/datum/species/human";
        const LIZARD: &str = "/datum/species/lizard";
        let config = restricting(&["Security Officer"], &[HUMAN, "/datum/species/skrell"]);
        assert!(species_restricted(
            &config,
            false,
            LIZARD,
            "Security Officer"
        ));
        assert!(!species_restricted(
            &config,
            false,
            HUMAN,
            "Security Officer"
        ));
        assert!(!species_restricted(&config, false, LIZARD, "Assistant"));
        // Felinids are their own path, not humans.
        assert!(species_restricted(
            &config,
            false,
            "/datum/species/human/felinid",
            "Security Officer"
        ));
        // The xenobureaucracy error lifts it; no allowed species restricts nobody.
        assert!(!species_restricted(
            &config,
            true,
            LIZARD,
            "Security Officer"
        ));
        let open = restricting(&["Security Officer"], &[]);
        assert!(!species_restricted(
            &open,
            false,
            LIZARD,
            "Security Officer"
        ));
        assert_eq!(
            restricted_indexes(&config, false, LIZARD, &officer_data(1)),
            vec![1]
        );
    }

    #[test]
    fn wanted_restricted_job_blocks_ready_and_random_picks() {
        let mut world = World::new();
        world.insert_resource(restricting(
            &["Security Officer"],
            &["/datum/species/human"],
        ));
        world.insert_resource(Jobs::new(officer_data(5), 3));
        let alien = crate::human::roundstart_species()
            .iter()
            .position(|s| s.def().path != "/datum/species/human")
            .expect("a non-human roundstart species");
        let mut look = AppearancePrefs::random(1);
        look.species = alien;
        let mut prefs = JobPreferences::default();
        prefs.set(0, Some(crate::lobby::JobPriority::High), 0);
        let client = world.spawn((look, prefs.clone())).id();
        assert_eq!(restricted_jobs(&world, client), vec![1]);
        assert!(!prefs_restricted(&world, client));
        prefs.set(1, Some(crate::lobby::JobPriority::Low), 0);
        world.entity_mut(client).insert(prefs);
        assert!(prefs_restricted(&world, client));
        // A human may.
        look.species = 0;
        world.entity_mut(client).insert(look);
        assert!(!prefs_restricted(&world, client));
        // Without a character there is nothing to check.
        let bare = world.spawn_empty().id();
        assert!(restricted_jobs(&world, bare).is_empty());
    }

    #[test]
    fn captaincy_follows_the_chain_of_command() {
        assert_eq!(latejoin_captain("Captain", true), Some(true));
        assert_eq!(latejoin_captain("Head of Personnel", false), Some(false));
        // Somebody already holds the spare: no acting captain.
        assert_eq!(latejoin_captain("Head of Personnel", true), None);
        assert_eq!(latejoin_captain("Assistant", false), None);
        // The Magistrate announces as a head but is not in the chain.
        assert_eq!(latejoin_captain("Magistrate", false), None);
        assert!(HEAD_ANNOUNCE.iter().any(|(t, _)| *t == "Magistrate"));
        assert_eq!(chain_rank("Research Director"), Some(3));
        assert_eq!(captaincy_announcement(true, "Боб"), "Капитан Боб на борту!");
        assert!(captaincy_announcement(false, "Боб").ends_with("капитана Боб на борту!"));
    }

    #[test]
    fn promotion_forces_command_access() {
        let mut world = World::new();
        world.init_resource::<Messages<ss13_net::chat::server::SendChat>>();
        world.insert_resource(Jobs::new(fallback_jobs(), 1));
        let card = world
            .spawn(crate::inventory::IdCard {
                access: vec!["hop".into()],
            })
            .id();
        let mob = world
            .spawn(crate::inventory::Inventory {
                id: Some(card),
                ..default()
            })
            .id();
        promote_to_captain(&mut world, mob, true);
        promote_to_captain(&mut world, mob, true);
        assert!(world.resource::<Jobs>().assigned_captain);
        assert_eq!(
            world.get::<crate::inventory::IdCard>(card).unwrap().access,
            vec!["hop".to_owned(), "command".to_owned()]
        );
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
