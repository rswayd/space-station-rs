//! Separatist nations after tg `code/modules/antagonists/separatist/` (`separatist.dm`,
//! `nation_creation.dm`, `objectives.dm`) and the department names of
//! `code/modules/jobs/departments/departments.dm` (`generate_nation_name`).
//!
//! - [`create_separatist_nation`]: a department (given or random among the not yet independent
//!   ones) becomes a nation ([`Nations`]): its first-department jobs revolt, every player body
//!   with such a job becomes a [`Separatist`], later crew of those jobs joins on arrival
//!   ([`join_latecomers`], `COMSIG_GLOB_CREWMEMBER_JOINED`). No citizens: no nation.
//! - Objectives: silicons keep the peace (`united_nations`); others get a fluff goal and, when
//!   dangerous with another nation around, "no member of the enemy escapes alive" — the enemy
//!   strikes back if it is dangerous too (`war_declared`).
//! - The AI of the silicon nation leaves its law rack and takes the United Nations lawset.
//! - [`roundend_report`]: each nation as a team.
// shortcut: the info panel is the generic one (no nation colour, `AntagInfoSeparatist` is not
// drawn), `onSyndieBase` is not checked by the destruction goal, and the round default lawset
// of new silicons stays the config one; upgrade with a separatist client view.

use bevy::prelude::*;
use ss13_mapload::spawner::Rng;
use ss13_net::chat::server::SendChat;
use ss13_net::server::ClientToken;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_tgui::UiInterface;
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiSystems};

use super::dynamic::DynamicRng;
use super::objectives::{RoundEnd, considered_alive, considered_escaped};
use super::{AntagHudData, AntagHudUi, BrotherInfo, ObjectiveInfo};
use crate::jobs::{Jobs, PlayerJob};
use crate::server::PlayerMob;
use crate::silicon::ai::AiCore;
use crate::tools::tell;

/// `/datum/antagonist/separatist` `name`.
pub const ANTAG_NAME: &str = "Separatists";
/// `/datum/team/nation` `member_name`.
const MEMBER_NAME: &str = "separatist";
pub const SILICON: &str = "/datum/job_department/silicon";
pub const ASSISTANT: &str = "/datum/job_department/assistant";
/// `create_separatist_nation`'s random pool.
const RANDOM_POOL: [&str; 7] = [
    ASSISTANT,
    "/datum/job_department/medical",
    "/datum/job_department/engineering",
    "/datum/job_department/science",
    "/datum/job_department/cargo",
    "/datum/job_department/service",
    "/datum/job_department/security",
];
/// `/datum/dynamic_ruleset/roundstart/nations` `department_types`.
pub const NATIONS_DEPARTMENTS: [&str; 7] = [
    SILICON,
    "/datum/job_department/cargo",
    "/datum/job_department/engineering",
    "/datum/job_department/medical",
    "/datum/job_department/science",
    "/datum/job_department/security",
    "/datum/job_department/service",
];
const NATION_SUFFIXES: [&str; 19] = [
    "stan", "topia", "land", "nia", "ca", "tova", "dor", "ador", "tia", "sia", "ano", "tica",
    "tide", "cis", "marea", "co", "taoide", "slavia", "stotzka",
];
const NOMADIC_NAMES: [&str; 7] = [
    "roving clans",
    "barbaric tribes",
    "tides",
    "bandit kingdom",
    "tribal society",
    "marauder clans",
    "horde",
];
const UNITED_NATIONS_LAWS: &str = "united_nations";

/// A department's `nation_prefixes`.
fn nation_prefixes(department: &str) -> &'static [&'static str] {
    match department.rsplit('/').next().unwrap_or_default() {
        "assistant" => &[
            "Assa", "Mainte", "Tunnel", "Gris", "Grey", "Liath", "Grigio", "Ass", "Assi",
        ],
        "security" => &[
            "Securi",
            "Beepski",
            "Shitcuri",
            "Red",
            "Stunba",
            "Flashbango",
            "Flasha",
            "Stanfordi",
        ],
        "engineering" => &["Atomo", "Engino", "Power", "Teleco"],
        "medical" => &["Mede", "Healtha", "Recova", "Chemi", "Viro", "Psych"],
        "science" => &[
            "Sci", "Griffa", "Geneti", "Explosi", "Mecha", "Xeno", "Nani", "Cyto",
        ],
        "cargo" => &[
            "Cargo", "Guna", "Suppli", "Mule", "Crate", "Ore", "Mini", "Shaf",
        ],
        "service" => &[
            "Honka", "Boozo", "Fatu", "Danka", "Mimi", "Libra", "Jani", "Religi",
        ],
        _ => &[],
    }
}

fn pick<'a>(list: &[&'a str], rng: &mut Rng) -> &'a str {
    if list.is_empty() {
        return "";
    }
    list[rng.range(0, list.len() as i64 - 1) as usize]
}

/// `/datum/job_department/proc/generate_nation_name` and its overrides.
pub fn generate_nation_name(department: &str, rng: &mut Rng) -> String {
    if department == SILICON {
        return "United Nations".to_owned();
    }
    let base = format!(
        "{}{}",
        pick(nation_prefixes(department), rng),
        pick(&NATION_SUFFIXES, rng)
    );
    if department == ASSISTANT {
        return format!("The {} of {base}", pick(&NOMADIC_NAMES, rng));
    }
    base
}

/// A nation's objective (`/datum/objective/destroy_nation`, `separatist_fluff`,
/// `united_nations`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NationObjective {
    /// The index of the enemy nation in [`Nations`].
    Destroy(usize),
    Fluff(String),
    UnitedNations,
}

/// `/datum/team/nation`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Nation {
    pub name: String,
    pub department: String,
    pub department_name: String,
    /// `potential_recruits`: job titles.
    pub jobs: Vec<String>,
    pub objectives: Vec<NationObjective>,
    /// `dangerous_nation`.
    pub dangerous: bool,
}

/// Every nation of the round (`GLOB.antagonist_teams` of nations).
#[derive(Resource, Clone, Debug, Default)]
pub struct Nations(pub Vec<Nation>);

/// On a separatist's body (`/datum/antagonist/separatist`).
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct Separatist {
    pub nation: usize,
    pub key: String,
}

fn fluff_text(nation: &str, rng: &mut Rng) -> String {
    let options = [
        format!("The rest of the station must be taxed for their use of {nation}'s services."),
        format!(
            "Make statues everywhere of your glorious leader of {nation}. If you have nobody, \
             crown one amongst yourselves!"
        ),
        format!("{nation} must be absolutely blinged out."),
        format!(
            "Damage as much of the station as you can, keep it in disrepair. {nation} must be \
             the untouched paragon!"
        ),
        format!("Heavily reinforce {nation} against the dangers of the outside world."),
        format!(
            "Make sure {nation} is fully off the grid, not requiring power or any other \
             services from other departments!"
        ),
        format!(
            "Use a misaligned teleporter to make you and your fellow citizens of {nation} \
             flypeople. Bring toxin medication!"
        ),
        format!(
            "Save the station when it needs you most. {nation} will be remembered as the \
             protectors."
        ),
        format!("Arm up. The citizens of {nation} have a right to bear arms."),
    ];
    options[rng.range(0, options.len() as i64 - 1) as usize].clone()
}

impl Nations {
    pub fn explanation(&self, objective: &NationObjective) -> String {
        match objective {
            NationObjective::Destroy(target) => match self.0.get(*target) {
                Some(t) => format!(
                    "Make sure no member of {} ({}) nation escapes alive!",
                    t.name, t.department_name
                ),
                None => "Free Objective".to_owned(),
            },
            NationObjective::Fluff(text) => text.clone(),
            NationObjective::UnitedNations => "Maintain the peace on the station. Ensure every \
                                               nation has a delegate alive by the end of the \
                                               round."
                .to_owned(),
        }
    }
}

/// `check_completion` of a nation objective.
pub fn check_completion(world: &mut World, objective: &NationObjective, end: RoundEnd) -> bool {
    let members = |world: &mut World, nation: usize| -> Vec<Entity> {
        let mut q = world.query::<(Entity, &Separatist)>();
        q.iter(world)
            .filter(|(_, s)| s.nation == nation)
            .map(|(e, _)| e)
            .collect()
    };
    match objective {
        // "at least one member got away": alive and on CentCom.
        NationObjective::Destroy(target) => !members(world, *target)
            .into_iter()
            .any(|m| considered_alive(world, m) && crate::shuttle::escape_status(world, m).0),
        NationObjective::Fluff(_) => true,
        // Every nation with a member that escaped.
        NationObjective::UnitedNations => {
            let count = world.get_resource::<Nations>().map_or(0, |n| n.0.len());
            (0..count).all(|n| {
                members(world, n)
                    .into_iter()
                    .any(|m| considered_escaped(world, m, end))
            })
        }
    }
}

fn info(world: &World, nation: usize) -> BrotherInfo {
    let Some(nations) = world.get_resource::<Nations>() else {
        return BrotherInfo::default();
    };
    let Some(n) = nations.0.get(nation) else {
        return BrotherInfo::default();
    };
    BrotherInfo {
        antag_name: n.name.clone(),
        objectives: n
            .objectives
            .iter()
            .enumerate()
            .map(|(i, o)| ObjectiveInfo {
                count: i as u32 + 1,
                explanation: nations.explanation(o),
                complete: false,
            })
            .collect(),
    }
}

/// The separatist's info panel (generic, see the module shortcut).
pub struct SeparatistInfoWindow;

impl UiInterface for SeparatistInfoWindow {
    const NAME: &'static str = crate::antag::cult::AntagInfoGenericUi::NAME;
    type Data = ();
    type StaticData = BrotherInfo;
}

/// The panel reads the nation through the world, so the source carries a snapshot.
#[derive(Component, Clone, Debug, Default)]
pub struct SeparatistPanel(pub BrotherInfo);

impl ServerUi for SeparatistInfoWindow {
    type Source = &'static SeparatistPanel;
    type UserParam = ();
    type Act = ();

    fn static_data(panel: &SeparatistPanel) -> BrotherInfo {
        panel.0.clone()
    }

    fn data(_: &SeparatistPanel, _: Entity, _: &()) {}
}

pub struct SeparatistPlugin;

impl Plugin for SeparatistPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui::<SeparatistInfoWindow>().add_systems(
            Update,
            (open_info.in_set(UiSystems::Handle), join_latecomers),
        );
    }
}

fn with_rng<R>(world: &mut World, f: impl FnOnce(&mut World, &mut Rng) -> R) -> R {
    let mut rng = world
        .remove_resource::<DynamicRng>()
        .map_or_else(|| Rng::new(0), |r| r.0);
    let out = f(world, &mut rng);
    world.insert_resource(DynamicRng(rng));
    out
}

/// The client of a body, the AI core's through its eye.
fn client_of(world: &World, mob: Entity) -> Option<Entity> {
    let mob = world.get::<AiCore>(mob).map_or(mob, |c| c.eye);
    world.get::<PlayerMob>(mob).map(|p| p.client)
}

fn job_title(world: &World, mob: Entity) -> Option<String> {
    let job = world.get::<PlayerJob>(mob)?.0;
    Some(
        world
            .get_resource::<Jobs>()?
            .data
            .jobs
            .get(job)?
            .title
            .clone(),
    )
}

/// `/proc/create_separatist_nation`. `department` is a `/datum/job_department` path; `None`
/// picks a random one that is not independent yet. Returns the new nation's index, `None` when
/// nobody holds its jobs (or every department already seceded).
pub fn create_separatist_nation(
    world: &mut World,
    department: Option<&str>,
    announcement: bool,
    dangerous: bool,
) -> Option<usize> {
    let existing = world
        .get_resource::<Nations>()
        .map(|n| n.0.clone())
        .unwrap_or_default();
    let department = match department {
        Some(d) => d.to_owned(),
        None => {
            let pool: Vec<&str> = RANDOM_POOL
                .iter()
                .copied()
                .filter(|d| !existing.iter().any(|n| n.department == *d))
                .collect();
            if pool.is_empty() {
                warn!(
                    "Department Revolt could not create a nation, as all the departments are independent"
                );
                return None;
            }
            with_rng(world, |_, rng| pick(&pool, rng).to_owned())
        }
    };
    let (department_name, jobs) = {
        let data = &world.resource::<Jobs>().data;
        let Some(d) = data.departments.iter().find(|d| d.path == department) else {
            warn!("no department {department} to secede");
            return None;
        };
        // A non-command department lists only the jobs that have it first (command ones are
        // listed in all theirs, and command never secedes).
        let jobs: Vec<String> = d
            .jobs
            .iter()
            .filter_map(|&j| data.jobs.get(usize::from(j)).map(|j| j.title.clone()))
            .collect();
        (d.name.clone(), jobs)
    };
    let index = existing.len();
    let (name, target, fluff) = with_rng(world, |_, rng| {
        let name = generate_nation_name(&department, rng);
        let target =
            (!existing.is_empty()).then(|| rng.range(0, existing.len() as i64 - 1) as usize);
        let fluff = fluff_text(&name, rng);
        (name, target, fluff)
    });
    // `generate_nation_objectives`.
    let mut objectives = Vec::new();
    let mut is_dangerous = true;
    if department == SILICON {
        objectives.push(NationObjective::UnitedNations);
    } else {
        is_dangerous = dangerous;
        if let Some(target) = target.filter(|_| dangerous) {
            objectives.push(NationObjective::Destroy(target));
        }
        objectives.push(NationObjective::Fluff(fluff));
    }
    let citizens: Vec<Entity> = {
        let mut q = world.query_filtered::<Entity, (With<PlayerJob>, Without<Separatist>)>();
        let mut all: Vec<Entity> = q.iter(world).collect();
        all.sort();
        all.into_iter()
            .filter(|&m| client_of(world, m).is_some())
            .filter(|&m| job_title(world, m).is_some_and(|t| jobs.contains(&t)))
            .collect()
    };
    if citizens.is_empty() {
        info!("The nation of {name} did not have enough potential members to be created.");
        return None;
    }
    world.init_resource::<Nations>();
    world.resource_mut::<Nations>().0.push(Nation {
        name: name.clone(),
        department: department.clone(),
        department_name: department_name.clone(),
        jobs: jobs.clone(),
        objectives,
        dangerous: is_dangerous,
    });
    // `target_nation.war_declared(src)`.
    if let Some(target) = target.filter(|_| dangerous && department != SILICON) {
        war_declared(world, target, index);
    }
    for mob in citizens {
        add_separatist(world, mob, index);
        info!("{mob} was made into a separatist, long live {name}!");
    }
    info!(
        "The nation of {name} has been formed. Affected jobs are {}. Any new crewmembers with \
         these jobs will join the secession.",
        jobs.join(", ")
    );
    if announcement {
        let text = if department == ASSISTANT {
            format!(
                "The assistants of the station have risen to form the new independent state of {name}!"
            )
        } else {
            format!(
                "The new independent state of {name} has formed from the ashes of the \
                 {department_name} department!"
            )
        };
        let station = crate::events::station_name(world);
        crate::events::priority_announce(world, &format!("Отделение от {station}"), &text);
    }
    Some(index)
}

/// `/datum/team/nation/proc/war_declared`: a dangerous nation strikes back.
fn war_declared(world: &mut World, nation: usize, attacker: usize) {
    let Some(mut nations) = world.get_resource::<Nations>().cloned() else {
        return;
    };
    let Some(n) = nations.0.get_mut(nation).filter(|n| n.dangerous) else {
        return;
    };
    n.objectives.push(NationObjective::Destroy(attacker));
    let message = format!(
        "The nation of {} has declared the intent to conquer {}! You have new objectives.",
        nations.0[attacker].name, nations.0[nation].name
    );
    world.insert_resource(nations);
    update_all_member_objectives(world, nation, Some(&message));
}

/// `update_all_member_objectives`: the panel snapshot, the message, the objectives.
fn update_all_member_objectives(world: &mut World, nation: usize, message: Option<&str>) {
    let mut q = world.query::<(Entity, &Separatist)>();
    let mut members: Vec<Entity> = q
        .iter(world)
        .filter(|(_, s)| s.nation == nation)
        .map(|(e, _)| e)
        .collect();
    members.sort();
    let panel = info(world, nation);
    for mob in members {
        world.entity_mut(mob).insert(SeparatistPanel(panel.clone()));
        let Some(client) = client_of(world, mob) else {
            continue;
        };
        if let Some(message) = message {
            tell(world, client, Span::Danger, message);
        }
        announce_objectives(world, client, &panel);
    }
}

fn announce_objectives(world: &mut World, client: Entity, panel: &BrotherInfo) {
    tell(world, client, Span::Notice, "Ваши текущие задачи:");
    for objective in &panel.objectives {
        world.write_message(SendChat {
            clients: vec![client],
            message: ChatMessage {
                channel: ChatChannel::System,
                segments: vec![
                    ChatSegment::new(Span::Bold, format!("Задача #{}", objective.count)),
                    ChatSegment::new(Span::Plain, format!(": {}", objective.explanation)),
                ],
            },
        });
    }
}

/// `add_antag_datum(/datum/antagonist/separatist, nation)`: the nation's objectives, the
/// greeting, the panel; an AI takes the United Nations laws off its rack.
pub fn add_separatist(world: &mut World, mob: Entity, nation: usize) -> bool {
    if world.get::<Separatist>(mob).is_some() || world.get_entity(mob).is_err() {
        return false;
    }
    let client = client_of(world, mob);
    let key = client
        .and_then(|c| world.get::<ClientToken>(c))
        .map(|t| t.0.clone())
        .unwrap_or_default();
    let panel = info(world, nation);
    world
        .entity_mut(mob)
        .insert((Separatist { nation, key }, SeparatistPanel(panel.clone())));
    if world.get::<AntagHudData>(mob).is_none() {
        world.entity_mut(mob).insert(AntagHudData::default());
    }
    // `apply_innate_effects`: `no_law_rack_link`, `replace_law_set(united_nations)`.
    if world.get::<AiCore>(mob).is_some() {
        crate::silicon::law_rack::forget_link(world, mob);
        world
            .entity_mut(mob)
            .insert(crate::silicon::law_rack::NoLawRackLink);
        let laws = world
            .resource::<crate::silicon::Silicons>()
            .0
            .lawset(UNITED_NATIONS_LAWS)
            .map(crate::silicon::laws::Laws::from_lawset);
        match laws {
            Some(laws) => {
                world.entity_mut(mob).insert(laws);
                crate::silicon::law_rack::announce_law_change(world, mob);
            }
            None => warn!("no {UNITED_NATIONS_LAWS} lawset"),
        }
    }
    let Some(client) = client else {
        return true;
    };
    // The default `greet`.
    tell(
        world,
        client,
        Span::Bold,
        format!("Вы - {}.", panel.antag_name),
    );
    announce_objectives(world, client, &panel);
    // The panel opens on the body the client controls (the AI's eye watches the core's).
    let viewer = world.get::<AiCore>(mob).map_or(mob, |c| c.eye);
    world.write_message(OpenUi::new::<AntagHudUi>(client, viewer, mob, ""));
    world.write_message(OpenUi::new::<SeparatistInfoWindow>(
        client, viewer, mob, ANTAG_NAME,
    ));
    true
}

/// `new_possible_separatist` (`COMSIG_GLOB_CREWMEMBER_JOINED`): new crew of a seceded job joins.
fn join_latecomers(
    nations: Option<Res<Nations>>,
    joined: Query<Entity, (Added<PlayerJob>, Without<Separatist>)>,
    mut commands: Commands,
) {
    if nations.is_none_or(|n| n.0.is_empty()) {
        return;
    }
    for mob in &joined {
        commands.queue(move |world: &mut World| {
            let Some(title) = job_title(world, mob) else {
                return;
            };
            let nation = world
                .resource::<Nations>()
                .0
                .iter()
                .position(|n| n.jobs.contains(&title));
            if let Some(nation) = nation
                && client_of(world, mob).is_some()
            {
                add_separatist(world, mob, nation);
            }
        });
    }
}

/// `/datum/action/antag_info/Trigger` of a separatist.
fn open_info(
    world: &mut World,
    mut acts: Local<bevy::ecs::message::MessageCursor<UiActed<AntagHudUi>>>,
) {
    let pending: Vec<(Entity, Entity, Entity)> = acts
        .read(world.resource::<Messages<UiActed<AntagHudUi>>>())
        .map(|a| (a.client, a.actor, a.source))
        .collect();
    for (client, actor, source) in pending {
        if world.get::<Separatist>(source).is_some() {
            world.write_message(OpenUi::new::<SeparatistInfoWindow>(
                client, actor, source, ANTAG_NAME,
            ));
        }
    }
}

/// `/datum/team/roundend_report` of every nation.
pub fn roundend_report(world: &mut World, end: RoundEnd) -> Vec<ChatMessage> {
    let Some(nations) = world.get_resource::<Nations>().cloned() else {
        return Vec::new();
    };
    let line = |segments: Vec<ChatSegment>| ChatMessage {
        channel: ChatChannel::System,
        segments,
    };
    let mut out = Vec::new();
    for (index, nation) in nations.0.iter().enumerate() {
        let mut q = world.query::<(Entity, &Separatist)>();
        let mut members: Vec<(Entity, String)> = q
            .iter(world)
            .filter(|(_, s)| s.nation == index)
            .map(|(e, s)| (e, s.key.clone()))
            .collect();
        members.sort();
        out.push(line(vec![ChatSegment::new(
            Span::Bold,
            format!("{}:", nation.name),
        )]));
        out.push(line(vec![ChatSegment::new(
            Span::Plain,
            format!("{MEMBER_NAME} были:"),
        )]));
        for (mob, key) in &members {
            out.push(super::traitor::print_player(world, *mob, key));
        }
        if nation.objectives.is_empty() {
            continue;
        }
        out.push(line(vec![ChatSegment::new(
            Span::Bold,
            "Команда имела следующие цели:",
        )]));
        let mut win = true;
        for (i, objective) in nation.objectives.iter().enumerate() {
            let done = check_completion(world, objective, end);
            win &= done;
            out.push(line(vec![
                ChatSegment::new(Span::Bold, format!("Цель #{}", i + 1)),
                ChatSegment::new(
                    Span::Plain,
                    format!(": {} ", nations.explanation(objective)),
                ),
                if done {
                    ChatSegment::new(Span::GreenText, "Успех!")
                } else {
                    ChatSegment::new(Span::RedText, "Неудача.")
                },
            ]));
        }
        out.push(line(vec![if win {
            ChatSegment::new(
                Span::GreenText,
                format!("{} выполнил свои задачи!", nation.name),
            )
        } else {
            ChatSegment::new(Span::RedText, format!("{} провалился!", nation.name))
        }]));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nation_names_follow_the_department() {
        let mut rng = Rng::new(3);
        assert_eq!(generate_nation_name(SILICON, &mut rng), "United Nations");
        let assistants = generate_nation_name(ASSISTANT, &mut rng);
        assert!(assistants.starts_with("The "), "{assistants}");
        let medical = generate_nation_name("/datum/job_department/medical", &mut rng);
        assert!(
            nation_prefixes("/datum/job_department/medical")
                .iter()
                .any(|p| medical.starts_with(p))
                && NATION_SUFFIXES.iter().any(|s| medical.ends_with(s)),
            "{medical}"
        );
    }

    #[test]
    fn destruction_goal_names_the_enemy() {
        let nations = Nations(vec![Nation {
            name: "Medestan".into(),
            department: "/datum/job_department/medical".into(),
            department_name: "Medical".into(),
            jobs: vec!["Medical Doctor".into()],
            objectives: Vec::new(),
            dangerous: true,
        }]);
        assert_eq!(
            nations.explanation(&NationObjective::Destroy(0)),
            "Make sure no member of Medestan (Medical) nation escapes alive!"
        );
        assert_eq!(
            nations.explanation(&NationObjective::Destroy(5)),
            "Free Objective"
        );
    }

    fn world_with_departments() -> World {
        use ss13_mapload::jobs::{Department, Job};
        let mut data = crate::jobs::fallback_jobs();
        data.jobs.push(Job {
            title: "Medical Doctor".into(),
            ..Default::default()
        });
        data.jobs.push(Job {
            title: "Station Engineer".into(),
            ..Default::default()
        });
        let dept = |path: &str, name: &str, job: u16| Department {
            path: path.into(),
            name: name.into(),
            color: "#65b2bd".into(),
            jobs: vec![job],
        };
        data.departments
            .push(dept("/datum/job_department/medical", "Medical", 1));
        data.departments
            .push(dept("/datum/job_department/engineering", "Engineering", 2));
        let mut world = World::new();
        world.init_resource::<Messages<SendChat>>();
        world.init_resource::<Messages<OpenUi>>();
        world.insert_resource(Jobs::new(data, 0));
        world
    }

    #[test]
    fn departments_secede_and_dangerous_ones_declare_war() {
        let mut world = world_with_departments();
        let client = world.spawn_empty().id();
        let doctor = world.spawn((PlayerJob(1), PlayerMob { client })).id();
        let engineer = world.spawn((PlayerJob(2), PlayerMob { client })).id();
        let assistant = world.spawn((PlayerJob(0), PlayerMob { client })).id();
        // A department nobody works in: no nation.
        assert_eq!(
            create_separatist_nation(
                &mut world,
                Some("/datum/job_department/science"),
                false,
                true
            ),
            None
        );
        let medical = create_separatist_nation(
            &mut world,
            Some("/datum/job_department/medical"),
            false,
            true,
        )
        .unwrap();
        // The first nation has nobody to fight: only its fluff goal.
        let nations = world.resource::<Nations>().0.clone();
        assert!(matches!(
            nations[medical].objectives.as_slice(),
            [NationObjective::Fluff(_)]
        ));
        let engineering = create_separatist_nation(
            &mut world,
            Some("/datum/job_department/engineering"),
            false,
            true,
        )
        .unwrap();
        let nations = world.resource::<Nations>().0.clone();
        assert_eq!(
            nations[engineering].objectives[0],
            NationObjective::Destroy(medical)
        );
        // The dangerous enemy strikes back.
        assert_eq!(
            nations[medical].objectives.last(),
            Some(&NationObjective::Destroy(engineering))
        );
        assert_eq!(world.get::<Separatist>(doctor).unwrap().nation, medical);
        assert_eq!(
            world.get::<Separatist>(engineer).unwrap().nation,
            engineering
        );
        assert!(world.get::<Separatist>(assistant).is_none());
        // The doctor's panel shows the new goal.
        assert_eq!(
            world
                .get::<SeparatistPanel>(doctor)
                .unwrap()
                .0
                .objectives
                .len(),
            2
        );
        // Nobody escaped to CentCom: no enemy got away.
        assert!(check_completion(
            &mut world,
            &NationObjective::Destroy(medical),
            RoundEnd::default()
        ));
        assert!(!roundend_report(&mut world, RoundEnd::default()).is_empty());
    }
}
