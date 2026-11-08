//! Abductors (server) after tg `code/modules/antagonists/abductor`: `abductor.dm` (the
//! antagonist, its team and objective), `abductee/` (the abducted), `machinery/` (console, pad,
//! experimentation machine, gland storage: [`ship`]), `equipment/gland.dm` and `glands/`
//! ([`glands`]), `equipment/gear/abductor_items.dm`, `abductor_clothing.dm` and
//! `abduction_outfits.dm` ([`gear`]).
//!
//! - **The ship.** tg lazy-loads `_maps/templates/lazy_templates/abductor_ships.dmm`; here it
//!   loads with the station onto its own level (like the wizard's den) when every type on it has
//!   a prototype. Its machines and landmarks carry `team_number` var edits: the four ships.
//! - **The team** ([`spawn_abductors`], the "Abductors" midround ruleset): a new
//!   `/datum/team/abductor_team` ("Mothership <greek letter>", the next team number, the
//!   experiment objective: 6 specimens). The first player is the scientist, the second the agent
//!   (a lone player: the solo scientist with the agent's gear). Each gets a fresh abductor body
//!   on its team's landmark, named "<team> Agent"/"<team> Scientist", the outfit
//!   (`/datum/outfit/abductor/*`), the vest and science tool linked to the team's console, the
//!   HUD icon, the greeting and the info panel.
//! - **Abductees** ([`add_abductee`]): the experimentation machine's subjects get one strange
//!   objective (75%: a line of `strings/abductee_objectives.txt`, else steal, paint, speech,
//!   capture, a seance or the forbidden number), the HUD icon and the info panel.
//! - [`roundend_report`]: each team (won when its machine has 6 points), its members and the
//!   abductees.
// shortcut: no team telepathy (the superlingual matrix tongue is a speech feature), no team HUD
// between the members, no gland HUD, no baton recall spell, no beamplant implant, no
// skin-colour teams, no admin "Equip"/team picker; the scientist's extra alien surgeries are the
// surgery group's.

use std::path::PathBuf;

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::spawner::Rng;
use ss13_mapload::{IconIndex, MapLoadPlugin};
use ss13_net::server::ClientToken;
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_tgui::UiInterface;
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiSystems};

use crate::antag::{AntagHudData, AntagHudUi, BrotherInfo, ObjectiveInfo};
use crate::health::Health;
use crate::server::PlayerMob;
use crate::tools::tell_mob;

pub mod gear;
pub mod glands;
pub mod ship;

pub use gear::*;
pub use glands::*;
pub use ship::*;

#[cfg(test)]
mod tests;

/// `roundend_category` of the abductors and the abductees.
pub const ROUNDEND_CATEGORY: &str = "Абдукторы";
pub const ABDUCTEE_CATEGORY: &str = "Похищенные";
/// `/datum/objective/experiment` `target_amount`.
pub const EXPERIMENT_TARGET: u32 = 6;
/// The ruleset's `can_be_selected`: at most 4 teams (the ship has four).
pub const MAX_TEAMS: usize = 4;
const ANTAG_HUD_DMI: &str = "icons/mob/huds/antag_hud.dmi";
pub const ABDUCTEE_OBJECTIVES: &str = "strings/abductee_objectives.txt";

pub const AGENT_JOB: &str = "/datum/job/abductor_agent";
pub const SCIENTIST_JOB: &str = "/datum/job/abductor_scientist";
pub const SOLO_JOB: &str = "/datum/job/abductor_solo";

/// `GLOB.greek_letters`.
const GREEK_LETTERS: [&str; 24] = [
    "Alpha", "Beta", "Gamma", "Delta", "Epsilon", "Zeta", "Eta", "Theta", "Iota", "Kappa",
    "Lambda", "Mu", "Nu", "Xi", "Omicron", "Pi", "Rho", "Sigma", "Tau", "Upsilon", "Phi", "Chi",
    "Psi", "Omega",
];

/// `/datum/antagonist/abductor/agent`, `/scientist`, `/scientist/onemanteam`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AbductorRole {
    Agent,
    Scientist,
    Solo,
}

impl AbductorRole {
    /// `name`.
    pub fn antag_name(self) -> &'static str {
        match self {
            Self::Agent => "Abductor Agent",
            Self::Scientist => "Abductor Scientist",
            Self::Solo => "Abductor Solo",
        }
    }

    /// `sub_role`: the body's name after the team's.
    pub fn sub_role(self) -> &'static str {
        match self {
            Self::Agent => "Agent",
            Self::Scientist | Self::Solo => "Scientist",
        }
    }

    pub fn job(self) -> &'static str {
        match self {
            Self::Agent => AGENT_JOB,
            Self::Scientist => SCIENTIST_JOB,
            Self::Solo => SOLO_JOB,
        }
    }

    /// `greet_text`.
    pub fn greet_text(self) -> &'static str {
        match self {
            Self::Agent => {
                "Используйте технологии маскировки и снаряжение, чтобы обезвреживать людей, \
                 которых заберёт ваш учёный."
            }
            Self::Scientist | Self::Solo => {
                "Используйте экспериментальную консоль и хирургические инструменты, чтобы \
                 следить за агентом и проводить эксперименты над похищенными людьми."
            }
        }
    }

    /// `TRAIT_ABDUCTOR_SCIENTIST_TRAINING`.
    pub fn scientist(self) -> bool {
        matches!(self, Self::Scientist | Self::Solo)
    }
}

/// `/datum/antagonist/abductor` on its body.
#[derive(Component, Clone, Debug)]
pub struct Abductor {
    /// The player's key, for the round end report.
    pub key: String,
    pub team: u32,
    pub role: AbductorRole,
}

/// `/datum/team/abductor_team`.
#[derive(Clone, Debug, PartialEq)]
pub struct AbductorTeam {
    pub number: u32,
    pub name: String,
    pub members: Vec<Entity>,
    /// `abductees`: the bodies of the abducted.
    pub abductees: Vec<Entity>,
}

/// `GLOB.antagonist_teams` of the abductors.
#[derive(Resource, Clone, Debug, Default)]
pub struct AbductorTeams(pub Vec<AbductorTeam>);

impl AbductorTeams {
    pub fn get(&self, number: u32) -> Option<&AbductorTeam> {
        self.0.iter().find(|t| t.number == number)
    }

    pub fn get_mut(&mut self, number: u32) -> Option<&mut AbductorTeam> {
        self.0.iter_mut().find(|t| t.number == number)
    }
}

/// `/datum/antagonist/abductee` on its body.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct Abductee {
    pub key: String,
    /// The `/datum/objective/abductee` explanation.
    pub objective: String,
}

#[derive(Resource)]
pub struct AbductorRng(pub Rng);

/// The lines of `strings/abductee_objectives.txt`.
#[derive(Resource, Default, Clone)]
pub struct AbducteeLines(pub Vec<String>);

/// Insert before `GameServerPlugin` to say where the ship template comes from.
#[derive(Resource, Clone, Debug, Default)]
pub struct AbductorConfig {
    /// `None`: [`SHIP_MAP`] of the BYOND checkout, with a real station map.
    pub ship: Option<PathBuf>,
}

/// `HAS_MIND_TRAIT(user, TRAIT_ABDUCTOR_TRAINING)`.
pub fn has_training(world: &World, mob: Entity) -> bool {
    world.get::<Abductor>(mob).is_some()
}

/// `TRAIT_ABDUCTOR_SCIENTIST_TRAINING`.
pub fn has_scientist_training(world: &World, mob: Entity) -> bool {
    world
        .get::<Abductor>(mob)
        .is_some_and(|a| a.role.scientist())
}

/// `TRAIT_ABDUCTOR_KNOWLEDGE`: the abductor brain (the species) and the antagonists.
pub fn has_knowledge(world: &World, mob: Entity) -> bool {
    has_training(world, mob) || is_abductor_species(world, mob)
}

pub fn is_abductor_species(world: &World, mob: Entity) -> bool {
    world.get::<crate::species::Species>(mob).map(|s| s.0)
        == Some(crate::species::SpeciesId::Abductor)
}

pub(crate) fn with_rng<R>(world: &mut World, f: impl FnOnce(&mut World, &mut Rng) -> R) -> R {
    let mut rng = world
        .remove_resource::<AbductorRng>()
        .map_or_else(|| Rng::new(0), |r| r.0);
    let out = f(world, &mut rng);
    world.insert_resource(AbductorRng(rng));
    out
}

pub(crate) fn name_of(world: &World, e: Entity) -> String {
    crate::medical::name_of(world, e)
}

fn hud_icon(world: &World, state: &str) -> Option<ss13_shared::IconStateId> {
    world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(ANTAG_HUD_DMI, state))
}

fn key_of(world: &World, mob: Entity) -> String {
    world
        .get::<PlayerMob>(mob)
        .and_then(|p| world.get::<ClientToken>(p.client))
        .map(|t| t.0.clone())
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------------------------
// The team

/// `/datum/objective/experiment` explanation.
pub fn experiment_explanation() -> String {
    format!("Проведите эксперименты над {EXPERIMENT_TARGET} людьми.")
}

/// `/datum/team/abductor_team/New`: the next team number (tg's static `team_count`) and a
/// name. `None` when the ship has no landmarks for it.
pub fn new_team(world: &mut World) -> Option<u32> {
    if !world.contains_resource::<AbductorTeams>() {
        world.init_resource::<AbductorTeams>();
    }
    let number = world.resource::<AbductorTeams>().0.len() as u32 + 1;
    let ship = world.get_resource::<AbductorShip>()?;
    if !ship.landmarks.iter().any(|l| l.team == number) {
        return None;
    }
    let name = with_rng(world, |_, rng| {
        let i = rng.range(0, GREEK_LETTERS.len() as i64 - 1) as usize;
        format!("Mothership {}", GREEK_LETTERS[i])
    });
    world.resource_mut::<AbductorTeams>().0.push(AbductorTeam {
        number,
        name,
        members: Vec::new(),
        abductees: Vec::new(),
    });
    Some(number)
}

/// The ruleset's `can_be_selected`: fewer than [`MAX_TEAMS`] teams and landmarks for the next.
pub fn can_spawn(world: &World) -> bool {
    let teams = world
        .get_resource::<AbductorTeams>()
        .map_or(0, |t| t.0.len());
    teams < MAX_TEAMS
        && world
            .get_resource::<AbductorShip>()
            .is_some_and(|s| s.landmarks.iter().any(|l| l.team == teams as u32 + 1))
}

/// `/datum/objective/experiment/check_completion`: the team's machine has enough points.
pub fn experiment_done(world: &mut World, team: u32) -> bool {
    experiment_of(world, team)
        .and_then(|e| world.get::<Experiment>(e))
        .is_some_and(|e| e.points >= EXPERIMENT_TARGET)
}

/// The ruleset's `assign_role`: a fresh body on its team's landmark each, the first the
/// scientist. The bodies (empty: no ship or team).
pub fn spawn_abductors(world: &mut World, clients: &[Entity]) -> Vec<Entity> {
    if clients.is_empty() {
        return Vec::new();
    }
    let Some(team) = new_team(world) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (i, &client) in clients.iter().enumerate() {
        let role = match (i, clients.len()) {
            (0, 1) => AbductorRole::Solo,
            (0, _) => AbductorRole::Scientist,
            _ => AbductorRole::Agent,
        };
        let Some(pos) = landmark(world, team, role) else {
            continue;
        };
        let mob = crate::server::spawn_mob(world, client, pos, String::new());
        world.entity_mut(client).remove::<crate::round::NewPlayer>();
        if add_abductor(world, mob, team, role) {
            out.push(mob);
        }
    }
    out
}

/// `add_antag_datum(/datum/antagonist/abductor/*, team)`: `on_gain` + `finalize_abductor` +
/// `greet`. False if it already is one, is not a living mob or the team does not exist.
pub fn add_abductor(world: &mut World, mob: Entity, team: u32, role: AbductorRole) -> bool {
    if world.get::<Abductor>(mob).is_some() || world.get::<Health>(mob).is_none() {
        return false;
    }
    let Some(team_name) = world
        .get_resource::<AbductorTeams>()
        .and_then(|t| t.get(team))
        .map(|t| t.name.clone())
    else {
        return false;
    };
    let key = key_of(world, mob);
    crate::noncrew_jobs::set_assigned_role(world, mob, role.job());
    crate::antag::wizard::server::strip(world, mob);
    crate::species::set_species(world, mob, crate::species::SpeciesId::Abductor);
    world
        .entity_mut(mob)
        .insert(Name::new(format!("{team_name} {}", role.sub_role())));
    gear::equip(world, mob, role);
    if let Some(pos) = landmark(world, team, role) {
        world.entity_mut(mob).insert(pos);
    }
    let hud_icon = hud_icon(world, "abductor");
    world.entity_mut(mob).insert((
        Abductor { key, team, role },
        AntagHudData {
            hud_icon,
            ..default()
        },
    ));
    if let Some(t) = world.resource_mut::<AbductorTeams>().get_mut(team) {
        t.members.push(mob);
    }
    gear::link_to_console(world, mob, team);
    greet(world, mob, role);
    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
        world.write_message(OpenUi::new::<AntagHudUi>(client, mob, mob, ""));
        world.write_message(OpenUi::new::<AbductorInfoWindow>(
            client,
            mob,
            mob,
            role.antag_name(),
        ));
    }
    info!("{mob} is an abductor ({role:?}) of team {team}");
    true
}

fn greet(world: &mut World, mob: Entity, role: AbductorRole) {
    tell_mob(
        world,
        mob,
        Span::Bold,
        format!("Вы — {}!", role.antag_name()),
    );
    tell_mob(
        world,
        mob,
        Span::Notice,
        "Вместе с напарником похищайте членов экипажа станции и проводите над ними эксперименты!",
    );
    tell_mob(world, mob, Span::Notice, role.greet_text());
    tell_mob(
        world,
        mob,
        Span::Plain,
        format!("Задача #1: {}", experiment_explanation()),
    );
}

/// The landmark of `role` in `team`'s ship (`landmark_type`, `team_number`).
pub fn landmark(world: &World, team: u32, role: AbductorRole) -> Option<TilePos> {
    let agent = role == AbductorRole::Agent;
    world
        .get_resource::<AbductorShip>()?
        .landmarks
        .iter()
        .find(|l| l.team == team && l.agent == agent)
        .map(|l| l.pos)
}

// ---------------------------------------------------------------------------------------------
// Abductees

/// `/datum/objective/abductee/*/New` (`give_objective`: 75% `random`, else one of the others).
pub fn abductee_objective(
    lines: &[String],
    jobs: &[String],
    dead: Option<&str>,
    rng: &mut Rng,
) -> String {
    fn pick(rng: &mut Rng, list: &[&str]) -> String {
        list[rng.range(0, list.len() as i64 - 1) as usize].to_owned()
    }
    if rng.prob(75.0) && !lines.is_empty() {
        return lines[rng.range(0, lines.len() as i64 - 1) as usize].clone();
    }
    match rng.range(0, 5) {
        0 => format!(
            "Украдите {}.",
            pick(
                rng,
                &[
                    "всех животных",
                    "все лампочки",
                    "всех обезьян",
                    "все фрукты",
                    "все ботинки",
                    "всё мыло",
                    "всё оружие",
                    "всю технику",
                    "все органы",
                ]
            )
        ),
        1 => format!(
            "Эта станция ужасна. Вы должны разукрасить её {}!",
            pick(
                rng,
                &[
                    "красным цветом",
                    "голубым цветом",
                    "зеленым цветом",
                    "желтым цветом",
                    "оранжевым цветом",
                    "розовым цветом",
                    "черным цветом",
                    "разноцветным",
                    "кровью",
                ]
            )
        ),
        2 => format!(
            "Ваш мозг сломлен... Вы можете общаться только {}.",
            pick(
                rng,
                &[
                    "пантомимой",
                    "рифмами",
                    "хайку",
                    "длинными метафорами",
                    "загадками",
                    "буквальными значениями слов",
                    "звуковыми эффектами",
                    "военным жаргоном",
                    "предложениями из трех слов",
                ]
            )
        ),
        3 => {
            if jobs.is_empty() {
                "Захватите кого-нибудь.".to_owned()
            } else {
                format!(
                    "Захватите {}.",
                    jobs[rng.range(0, jobs.len() as i64 - 1) as usize]
                )
            }
        }
        // `/datum/objective/abductee/calling`: without a dead mob tg keeps the empty text.
        4 => match dead {
            Some(name) => format!(
                "Вы знаете, что {name} погиб. Проведите спиритический сеанс, чтобы призвать его \
                 из мира духов."
            ),
            None => "Призовите духа с той стороны.".to_owned(),
        },
        _ => format!(
            "Игнорируйте группы предметов из {} штук, их не существует.",
            rng.range(2, 10)
        ),
    }
}

/// Titles of the occupied crew jobs (`joinable_occupations` with `current_positions`).
fn occupied_jobs(world: &mut World) -> Vec<String> {
    let mut q = world.query::<&crate::jobs::PlayerJob>();
    let indices: Vec<usize> = q.iter(world).map(|j| j.0).collect();
    let Some(jobs) = world.get_resource::<crate::jobs::Jobs>() else {
        return Vec::new();
    };
    let mut out: Vec<String> = indices
        .into_iter()
        .filter_map(|i| jobs.data.jobs.get(i).map(|j| j.title.clone()))
        .collect();
    out.sort();
    out.dedup();
    out
}

/// A random ghost's name (`pick(GLOB.dead_mob_list)`).
fn dead_name(world: &mut World) -> Option<String> {
    let mut q = world.query_filtered::<Entity, With<crate::ghost::Ghost>>();
    let ghosts: Vec<Entity> = q.iter(world).collect();
    if ghosts.is_empty() {
        return None;
    }
    let i = with_rng(world, |_, rng| {
        rng.range(0, ghosts.len() as i64 - 1) as usize
    });
    Some(name_of(world, ghosts[i]))
}

/// `add_antag_datum(/datum/antagonist/abductee)`: the objective, the greeting, the HUD icon
/// and the info panel. False if it already is one.
pub fn add_abductee(world: &mut World, mob: Entity) -> bool {
    if world.get::<Abductee>(mob).is_some() {
        return false;
    }
    let lines = world
        .get_resource::<AbducteeLines>()
        .map(|l| l.0.clone())
        .unwrap_or_default();
    let jobs = occupied_jobs(world);
    let dead = dead_name(world);
    let objective = with_rng(world, |_, rng| {
        abductee_objective(&lines, &jobs, dead.as_deref(), rng)
    });
    let key = key_of(world, mob);
    let hud_icon = hud_icon(world, "abductee");
    // An antagonist keeps its own HUD icon.
    if world.get::<AntagHudData>(mob).is_none() {
        world.entity_mut(mob).insert(AntagHudData {
            hud_icon,
            ..default()
        });
    }
    world.entity_mut(mob).insert(Abductee {
        key,
        objective: objective.clone(),
    });
    tell_mob(world, mob, Span::BoldWarning, "Ваш разум ломается!");
    tell_mob(
        world,
        mob,
        Span::BoldWarning,
        "Вы не можете вспомнить, как здесь оказались...",
    );
    tell_mob(world, mob, Span::Plain, format!("Задача #1: {objective}"));
    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
        world.write_message(OpenUi::new::<AntagHudUi>(client, mob, mob, ""));
        world.write_message(OpenUi::new::<AbducteeInfoWindow>(
            client, mob, mob, "Abductee",
        ));
    }
    true
}

// ---------------------------------------------------------------------------------------------
// Info panels

/// The abductor's info panel (tg's default `AntagInfoGeneric`).
pub struct AbductorInfoWindow;

impl UiInterface for AbductorInfoWindow {
    const NAME: &'static str = crate::antag::cult::AntagInfoGenericUi::NAME;
    type Data = ();
    type StaticData = BrotherInfo;
}

impl ServerUi for AbductorInfoWindow {
    type Source = &'static Abductor;
    type UserParam = ();
    type Act = ();

    fn static_data(abductor: &Abductor) -> BrotherInfo {
        BrotherInfo {
            antag_name: abductor.role.antag_name().to_owned(),
            objectives: vec![ObjectiveInfo {
                count: 1,
                explanation: experiment_explanation(),
                complete: false,
            }],
        }
    }

    fn data(_: &Abductor, _: Entity, _: &()) {}
}

/// The abductee's info panel.
pub struct AbducteeInfoWindow;

impl UiInterface for AbducteeInfoWindow {
    const NAME: &'static str = crate::antag::cult::AntagInfoGenericUi::NAME;
    type Data = ();
    type StaticData = BrotherInfo;
}

impl ServerUi for AbducteeInfoWindow {
    type Source = &'static Abductee;
    type UserParam = ();
    type Act = ();

    fn static_data(abductee: &Abductee) -> BrotherInfo {
        BrotherInfo {
            antag_name: "Abductee".to_owned(),
            objectives: vec![ObjectiveInfo {
                count: 1,
                explanation: abductee.objective.clone(),
                // `/datum/objective/abductee` `completed = 1`.
                complete: true,
            }],
        }
    }

    fn data(_: &Abductee, _: Entity, _: &()) {}
}

/// `/datum/action/antag_info/Trigger` of an abductor or abductee.
fn open_info(world: &mut World, mut acts: Local<MessageCursor<UiActed<AntagHudUi>>>) {
    let pending: Vec<(Entity, Entity, Entity)> = acts
        .read(world.resource::<Messages<UiActed<AntagHudUi>>>())
        .map(|a| (a.client, a.actor, a.source))
        .collect();
    for (client, actor, source) in pending {
        if let Some(role) = world.get::<Abductor>(source).map(|a| a.role) {
            world.write_message(OpenUi::new::<AbductorInfoWindow>(
                client,
                actor,
                source,
                role.antag_name(),
            ));
        } else if world.get::<Abductee>(source).is_some() {
            world.write_message(OpenUi::new::<AbducteeInfoWindow>(
                client, actor, source, "Abductee",
            ));
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Round end

fn line(segments: Vec<ChatSegment>) -> ChatMessage {
    ChatMessage {
        channel: ChatChannel::System,
        segments,
    }
}

/// `/datum/team/abductor_team/roundend_report` of every team, then the abductees.
pub fn roundend_report(
    world: &mut World,
    _end: crate::antag::objectives::RoundEnd,
) -> Vec<ChatMessage> {
    let teams = world
        .get_resource::<AbductorTeams>()
        .map(|t| t.0.clone())
        .unwrap_or_default();
    let mut out = Vec::new();
    for team in teams {
        let won = experiment_done(world, team.number);
        out.push(line(vec![if won {
            ChatSegment::new(
                Span::GreenText,
                format!("Команда {} выполнила свою миссию!", team.name),
            )
        } else {
            ChatSegment::new(
                Span::RedText,
                format!("Команда {} провалила свою миссию.", team.name),
            )
        }]));
        out.push(line(vec![ChatSegment::new(
            Span::Bold,
            format!("Абдукторами {} были:", team.name),
        )]));
        for &mob in &team.members {
            let key = world
                .get::<Abductor>(mob)
                .map(|a| a.key.clone())
                .unwrap_or_default();
            if world.get_entity(mob).is_ok() {
                out.push(crate::antag::traitor::print_player(world, mob, &key));
            }
        }
        out.push(line(vec![
            ChatSegment::new(Span::Bold, "Задача #1"),
            ChatSegment::new(Span::Plain, format!(": {} ", experiment_explanation())),
            if won {
                ChatSegment::new(Span::GreenText, "Успех!")
            } else {
                ChatSegment::new(Span::RedText, "Неудача.")
            },
        ]));
    }
    let mut q = world.query::<(Entity, &Abductee)>();
    let mut abductees: Vec<(Entity, Abductee)> =
        q.iter(world).map(|(e, a)| (e, a.clone())).collect();
    abductees.sort_by_key(|(e, _)| *e);
    if !abductees.is_empty() {
        out.push(line(vec![ChatSegment::new(
            Span::Bold,
            format!("{ABDUCTEE_CATEGORY} были:"),
        )]));
    }
    for (mob, abductee) in abductees {
        out.push(crate::antag::traitor::print_player(
            world,
            mob,
            &abductee.key,
        ));
        out.push(line(vec![
            ChatSegment::new(Span::Bold, "Задача #1"),
            ChatSegment::new(Span::Plain, format!(": {} ", abductee.objective)),
            ChatSegment::new(Span::GreenText, "Успех!"),
        ]));
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Setup

fn load_lines(world: &mut World) {
    let seed = world
        .get_resource::<ss13_mapload::MapSeed>()
        .map_or(0, |s| s.0);
    world.insert_resource(AbductorRng(Rng::new(seed ^ 0x6162_6475_6374)));
    let lines = world
        .get_resource::<MapLoadPlugin>()
        .map(|m| m.source.join(ABDUCTEE_OBJECTIVES))
        .and_then(|p| ss13_data::read(&p).ok())
        .map(|bytes| {
            String::from_utf8_lossy(&bytes)
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    world.insert_resource(AbducteeLines(lines));
}

pub struct AbductorPlugin;

impl Plugin for AbductorPlugin {
    fn build(&self, app: &mut App) {
        if !app.world().contains_resource::<AbductorConfig>() {
            app.init_resource::<AbductorConfig>();
        }
        let config = app.world().resource::<AbductorConfig>().clone();
        let ship = config.ship.or_else(|| {
            let map = app.world().get_resource::<MapLoadPlugin>()?;
            // Only with a real station map: fixture maps name `.dmm` files.
            map.maps.first().filter(|m| !m.ends_with(".dmm"))?;
            let path = map.source.join(SHIP_MAP);
            ss13_data::exists(&path).then_some(path)
        });
        if let Some(ship) = ship {
            app.insert_resource(ShipCandidate(ship));
        }
        app.register_ui::<AbductorInfoWindow>()
            .register_ui::<AbducteeInfoWindow>()
            .init_resource::<AbductorTeams>()
            .init_resource::<AbductorShip>()
            .add_systems(Startup, load_lines)
            .add_systems(Update, open_info.in_set(UiSystems::Handle));
        ship::build_ship(app);
        glands::build_glands(app);
        gear::build_gear(app);
    }

    /// The ship loads with the station only when every type on it has a prototype.
    fn finish(&self, app: &mut App) {
        ship::finish_ship(app);
    }
}
