//! Station goals (server), after tg `/datum/station_goal` (`code/modules/station_goals/`) with
//! BandaStation's generation in `send_roundstart_report` and its admin completion toggle.
//!
//! - [`generate`]: when the round-start report goes out, goals are taken at random until their
//!   weights reach the budget (`STATION_GOAL_BUDGET`, 1: one goal); their reports go into the
//!   report's "Цели на смену" section ([`report_section`]).
//! - The meteor shield (`/datum/station_goal/station_shield`): meteor shield satellites from
//!   cargo, switched on by hand in space after 2 s (`toggle`), anchor and destroy every meteor
//!   that comes within 14 tiles through open space (`HasProximity`, `space_los`). The goal is
//!   met when the active satellites on the station levels see 500 tiles together
//!   (`update_coverage`).
//! - The round-end report lists every goal and whether it was met ([`goal_report`]).
// shortcut: the DNA vault and the bluespace artillery are not offered (their machines, probes
// and parts are not ported); no satellite control console (`sat_control`), no emagged
// satellites (meteor chance, the dark matt-eor), no beam visual, no admin goals panel (the
// toggle is [`set_completed`]).

use std::collections::HashSet;

use bevy::prelude::*;
use ss13_mapload::spawner::Rng;
use ss13_mapload::{DmAtom, DmObj, IconIndex, MapSeed, Opaque};
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_shared::{Appearance, TilePos};
use ss13_world::SpatialIndex;

use crate::construction::{Job, path_of, turf_at};
use crate::obj_damage::{Anchored, is_under};
use crate::tools::{Actor, do_after, tell};

pub const STATION_SHIELD: &str = "/datum/station_goal/station_shield";
pub const SATELLITE: &str = "/obj/machinery/satellite";
pub const METEOR_SHIELD: &str = "/obj/machinery/satellite/meteor_shield";
/// `/obj/machinery/satellite/meteor_shield` `kill_range`.
pub const KILL_RANGE: i32 = 14;
/// `/datum/station_goal/station_shield` `coverage_goal`.
pub const COVERAGE_GOAL: usize = 500;
/// `toggle`'s `do_after(user, 2 SECONDS)`, deciseconds.
const TOGGLE_DELAY_DS: f32 = 20.0;
const SPACE: &str = "/turf/open/space";

/// A `/datum/station_goal` type's vars.
#[derive(Debug, PartialEq)]
pub struct GoalDef {
    pub path: &'static str,
    pub name: &'static str,
    pub weight: f64,
    pub requires_space: bool,
    /// `get_report()`.
    pub report: &'static str,
}

/// The ported goals.
pub static GOALS: [GoalDef; 1] = [GoalDef {
    path: STATION_SHIELD,
    name: "Станционные щиты",
    weight: 1.0,
    requires_space: true,
    report: "Система противометеорной защиты\n\
             Станция находится в зоне, заполненной космическим мусором.\n\
             У вас есть прототип защитной системы, который необходимо развернуть для снижения \
             количества аварий из-за столкновений.\n\
             Вы можете заказать спутники и системы через отдел снабжения.",
}];

/// tg `STATION_GOAL_BUDGET` (BandaStation: 1). Insert before `GameServerPlugin` to override;
/// otherwise `SS13_STATION_GOAL_BUDGET` or 1.
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct StationGoalsConfig {
    pub budget: f64,
}

impl Default for StationGoalsConfig {
    fn default() -> Self {
        Self {
            budget: std::env::var("SS13_STATION_GOAL_BUDGET")
                .ok()
                .and_then(|v| v.trim().parse::<f64>().ok())
                .filter(|v| v.is_finite() && *v >= 0.0)
                .unwrap_or(1.0),
        }
    }
}

/// One active goal.
#[derive(Clone, Debug, PartialEq)]
pub struct Goal {
    pub def: &'static GoalDef,
    /// tg `completed`: set by an admin.
    pub completed: bool,
}

/// tg `SSstation.goals_by_type`.
#[derive(Resource, Default)]
pub struct StationGoals {
    pub goals: Vec<Goal>,
    /// The meteor shield's `cached_coverage_length`.
    pub coverage: usize,
    rng: Option<Rng>,
}

impl StationGoals {
    pub fn has(&self, path: &str) -> bool {
        self.goals.iter().any(|g| g.def.path == path)
    }
}

/// An `/obj/machinery/satellite`'s `active` (absent: inactive).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Satellite {
    pub active: bool,
}

pub struct StationGoalsPlugin;

impl Plugin for StationGoalsPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // A satellite is switched on or off by hand.
        app.add_click_handler(ClickPhase::EmptyHand, 3470, |w, c| {
            crate::station_goals::interact(w, c.client, c.mob, c.target)
        });
        if !app.world().contains_resource::<StationGoalsConfig>() {
            app.init_resource::<StationGoalsConfig>();
        }
        app.init_resource::<StationGoals>()
            .add_systems(Update, shield_meteors);
    }
}

/// tg `generate_station_goals(goal_budget)`: random goals until the weights reach the budget;
/// space-only ones are skipped on planets (every map here is in space).
pub fn generate(world: &mut World) {
    let budget = world
        .get_resource::<StationGoalsConfig>()
        .map_or(1.0, |c| c.budget);
    let seed = world.get_resource::<MapSeed>().map_or(0, |s| s.0);
    let Some(mut goals) = world.get_resource_mut::<StationGoals>() else {
        return;
    };
    let mut rng = goals
        .rng
        .take()
        .unwrap_or_else(|| Rng::new(seed ^ 0x474f_414c));
    for def in pick(budget, false, &mut rng) {
        if !goals.has(def.path) {
            info!("station goal: {}", def.name);
            goals.goals.push(Goal {
                def,
                completed: false,
            });
        }
    }
    goals.rng = Some(rng);
}

/// The goals `generate` takes (`pick_n_take` until the weights reach `budget`).
pub fn pick(budget: f64, planetary: bool, rng: &mut Rng) -> Vec<&'static GoalDef> {
    let mut possible: Vec<&'static GoalDef> = GOALS.iter().collect();
    let mut weights = 0.0;
    let mut chosen = Vec::new();
    while !possible.is_empty() && weights < budget {
        let i = rng.range(0, possible.len() as i64 - 1) as usize;
        let picked = possible.remove(i);
        if picked.requires_space && planetary {
            continue;
        }
        weights += picked.weight;
        chosen.push(picked);
    }
    chosen
}

/// BandaStation's `%STATION_GOALS`: "Цели на смену" with every goal's report; empty without.
pub fn report_section(world: &World) -> String {
    let Some(goals) = world.get_resource::<StationGoals>() else {
        return String::new();
    };
    if goals.goals.is_empty() {
        return String::new();
    }
    let reports: Vec<&str> = goals.goals.iter().map(|g| g.def.report).collect();
    format!("=== Цели на смену ===\n\n{}", reports.join("\n\n---\n\n"))
}

/// BandaStation's `set_completed` (the admin toggle).
pub fn set_completed(world: &mut World, path: &str, completed: bool) {
    if let Some(mut goals) = world.get_resource_mut::<StationGoals>() {
        for goal in goals.goals.iter_mut().filter(|g| g.def.path == path) {
            goal.completed = completed;
        }
    }
}

/// tg `check_completion()`.
pub fn check_completion(world: &mut World, goal: &Goal) -> bool {
    if goal.completed {
        return true;
    }
    match goal.def.path {
        STATION_SHIELD => update_coverage(world) >= COVERAGE_GOAL,
        _ => false,
    }
}

/// tg `goal_report()`: "<name> : Выполнена!/Провалена!" per goal.
pub fn goal_report(world: &mut World) -> Vec<ChatMessage> {
    let goals = world
        .get_resource::<StationGoals>()
        .map(|g| g.goals.clone())
        .unwrap_or_default();
    goals
        .iter()
        .map(|goal| {
            let (span, result) = if check_completion(world, goal) {
                (Span::GreenText, "Выполнена!")
            } else {
                (Span::RedText, "Провалена!")
            };
            ChatMessage {
                channel: ChatChannel::System,
                segments: vec![
                    ChatSegment::new(Span::Plain, format!("{} : ", goal.def.name)),
                    ChatSegment::new(span, result),
                ],
            }
        })
        .collect()
}

fn is_space(world: &World, pos: TilePos) -> bool {
    turf_at(world, pos)
        .and_then(|t| path_of(world, t))
        .is_some_and(|p| is_under(&p, SPACE))
}

fn is_satellite(world: &World, e: Entity) -> bool {
    world
        .get::<DmObj>(e)
        .is_some_and(|o| is_under(&o.path, SATELLITE))
}

fn active(world: &World, e: Entity) -> bool {
    world.get::<Satellite>(e).is_some_and(|s| s.active)
}

/// `/obj/machinery/satellite/interact` -> the meteor shield's `toggle(user)`: 2 s of looking for
/// the switch first.
pub fn interact(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if !is_satellite(world, target) {
        return false;
    }
    let word = if active(world, target) {
        "выключения"
    } else {
        "включения"
    };
    tell(
        world,
        client,
        Span::Notice,
        format!("Ищем кнопку {word}..."),
    );
    let actor = Actor {
        client,
        mob,
        item: None,
    };
    do_after(world, actor, target, TOGGLE_DELAY_DS, 0, Job::Satellite);
    true
}

/// The end of the meteor shield's `toggle`: `/obj/machinery/satellite/toggle` (only in space
/// to switch on), `set_anchored`, the coverage updated.
pub fn complete_toggle(world: &mut World, client: Entity, target: Entity) {
    let Some(&pos) = world.get::<TilePos>(target) else {
        return;
    };
    if !is_satellite(world, target) {
        return;
    }
    let on = !active(world, target);
    let name = world
        .get::<ss13_mapload::DmName>(target)
        .map(|n| n.name.trim_start_matches("\\improper ").to_owned())
        .unwrap_or_default();
    if on && !is_space(world, pos) {
        tell(
            world,
            client,
            Span::Warning,
            format!("You can only activate {name} in space."),
        );
        return;
    }
    let verb = if on { "activate" } else { "deactivate" };
    tell(world, client, Span::Notice, format!("You {verb} {name}."));
    world
        .entity_mut(target)
        .insert((Satellite { active: on }, Anchored(on)));
    // `update_icon_state`: "<base_icon_state>_<active|inactive>".
    let state = if on { "sat_active" } else { "sat_inactive" };
    let icon = world.get::<DmAtom>(target).map(|a| a.icon.clone());
    let id = icon.and_then(|icon| world.get_resource::<IconIndex>()?.get(&icon, state));
    if let (Some(id), Some(mut appearance)) = (id, world.get_mut::<Appearance>(target)) {
        appearance.icon_state = id;
    }
    update_coverage(world);
}

/// The active meteor shields on the station levels with their tiles.
fn active_shields(world: &mut World) -> Vec<TilePos> {
    let levels = crate::events::controller::station_levels(world);
    let mut q = world.query::<(&Satellite, &DmObj, &TilePos)>();
    let mut out: Vec<TilePos> = q
        .iter(world)
        .filter(|(s, o, p)| s.active && is_under(&o.path, METEOR_SHIELD) && levels.contains(&p.z))
        .map(|(_, _, p)| *p)
        .collect();
    out.sort_by_key(|p| (p.z, p.y, p.x));
    out
}

/// tg `update_coverage`: the tiles in `view(kill_range)` of every active shield, counted once.
// shortcut: `view` is the square of `kill_range` without the tiles behind an opaque one on the
// line from the satellite (tg's `view` lighting and corner rules are not reproduced).
pub fn update_coverage(world: &mut World) -> usize {
    let shields = active_shields(world);
    let mut covered: HashSet<TilePos> = HashSet::new();
    {
        let index = world.resource::<SpatialIndex>();
        let opaque = |p: TilePos| {
            index
                .on_tile(p)
                .iter()
                .any(|&e| world.get::<Opaque>(e).is_some())
        };
        for sat in shields {
            let Some((w, h)) = index.level_size(sat.z) else {
                continue;
            };
            for y in (sat.y - KILL_RANGE).max(1)..=(sat.y + KILL_RANGE).min(h as i32) {
                for x in (sat.x - KILL_RANGE).max(1)..=(sat.x + KILL_RANGE).min(w as i32) {
                    let tile = TilePos::new(x, y, sat.z);
                    if covered.contains(&tile) {
                        continue;
                    }
                    let line = crate::radiation::get_line(sat, tile);
                    let seen = line
                        .iter()
                        .skip(1)
                        .take(line.len().saturating_sub(2))
                        .all(|&p| !opaque(p));
                    if seen {
                        covered.insert(tile);
                    }
                }
            }
        }
    }
    let n = covered.len();
    if let Some(mut goals) = world.get_resource_mut::<StationGoals>() {
        goals.coverage = n;
    }
    n
}

/// `HasProximity` of the active meteor shields: a meteor within `kill_range` seen through open
/// space only (`space_los`) is destroyed (`shield_defense`: every ported meteor gives in).
// Cost: meteors in flight (up to ~100) × active shields per tick, a line of at most 15 tiles
// for those in range.
fn shield_meteors(world: &mut World) {
    let mut meteors =
        world.query_filtered::<(Entity, &TilePos), With<crate::events::meteors::Meteor>>();
    let flying: Vec<(Entity, TilePos)> = meteors.iter(world).map(|(e, p)| (e, *p)).collect();
    if flying.is_empty() {
        return;
    }
    let mut sats = world.query::<(&Satellite, &DmObj, &TilePos)>();
    let shields: Vec<TilePos> = sats
        .iter(world)
        .filter(|(s, o, _)| s.active && is_under(&o.path, METEOR_SHIELD))
        .map(|(_, _, p)| *p)
        .collect();
    if shields.is_empty() {
        return;
    }
    for (meteor, pos) in flying {
        let shot = shields.iter().any(|&sat| {
            sat.chebyshev(pos).is_some_and(|d| d <= KILL_RANGE as u32)
                && crate::radiation::get_line(sat, pos)
                    .iter()
                    .all(|&p| is_space(world, p))
        });
        if shot {
            world.despawn(meteor);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_budget_takes_goals_until_reached() {
        let mut rng = Rng::new(3);
        let one = pick(1.0, false, &mut rng);
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].path, STATION_SHIELD);
        assert!(pick(0.0, false, &mut rng).is_empty());
        // More budget than goals: all of them, once.
        assert_eq!(pick(5.0, false, &mut rng).len(), GOALS.len());
        // The shield needs space.
        assert!(pick(1.0, true, &mut rng).is_empty());
    }

    #[test]
    fn report_and_admin_completion() {
        let mut world = World::new();
        world.insert_resource(StationGoalsConfig { budget: 1.0 });
        world.init_resource::<StationGoals>();
        assert!(report_section(&world).is_empty());
        generate(&mut world);
        generate(&mut world);
        assert_eq!(
            world.resource::<StationGoals>().goals.len(),
            1,
            "once per type"
        );
        let section = report_section(&world);
        assert!(section.starts_with("=== Цели на смену ==="), "{section}");
        assert!(section.contains("Система противометеорной защиты"));
        set_completed(&mut world, STATION_SHIELD, true);
        let goal = world.resource::<StationGoals>().goals[0].clone();
        assert!(check_completion(&mut world, &goal));
    }
}
