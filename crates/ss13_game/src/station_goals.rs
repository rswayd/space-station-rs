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
//! - An emagged meteor shield ([`emag_act`], one per minute station-wide) shows on GPS and,
//!   while active, doubles the weights of the meteor wave and stray meteor events
//!   ([`update_emagged`]); 3, 6 and 7 at once bring warnings (7: an announcement), 10 the dark
//!   matt-eor. A dark matt-eor reflects the beam and destroys the shield.
//! - The round-end report lists every goal and whether it was met ([`goal_report`]).
//! - The DNA vault (`/datum/station_goal/dna_vault`): samples of animals, plants and humans
//!   gathered with DNA probes into a vault (`crate::genetics::dna_vault`).
//! - [`on_report`]: a goal's supply packs become orderable with the report.
// shortcut: the bluespace artillery is not offered (its machines and parts are not ported); no
// beam visual or explosion visual, no examine lines or multitool
// readout of satellites, no admin goals panel (the toggle is [`set_completed`]).

use crate::construction::is_space;
use std::collections::HashSet;

use bevy::prelude::*;
use ss13_mapload::spawner::Rng;
use ss13_mapload::{DmAtom, DmObj, IconIndex, MapSeed, Opaque};
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_shared::{Appearance, TilePos};
use ss13_world::SpatialIndex;

use crate::construction::Job;
use crate::obj_damage::{Anchored, is_under};
use crate::tools::{Actor, do_after, tell};

pub const STATION_SHIELD: &str = "/datum/station_goal/station_shield";
pub const DNA_VAULT: &str = "/datum/station_goal/dna_vault";
pub const SATELLITE: &str = "/obj/machinery/satellite";
pub const METEOR_SHIELD: &str = "/obj/machinery/satellite/meteor_shield";
/// `/obj/machinery/satellite/meteor_shield` `kill_range`.
pub const KILL_RANGE: i32 = 14;
/// `/datum/station_goal/station_shield` `coverage_goal`.
pub const COVERAGE_GOAL: usize = 500;
/// `toggle`'s `do_after(user, 2 SECONDS)`, deciseconds.
const TOGGLE_DELAY_DS: f32 = 20.0;
/// `METEOR_SHIELD_EMAG_COOLDOWN`, seconds.
pub const EMAG_COOLDOWN_SECS: f64 = 60.0;
/// `EMAGGED_METEOR_SHIELD_THRESHOLD_ONE` .. `_FOUR`.
const EMAG_THRESHOLDS: [u32; 4] = [3, 6, 7, 10];
/// `change_meteor_chance`: the `meteor_wave` controls and the stray meteor.
const METEOR_CHANCE_EVENTS: [&str; 6] = [
    crate::events::meteors::WAVE_NORMAL,
    crate::events::meteors::WAVE_THREATENING,
    crate::events::meteors::WAVE_CATASTROPHIC,
    crate::events::meteors::WAVE_MEATY,
    crate::events::meteors::DUST_STORM,
    crate::events::meteors::STRAY_METEOR,
];

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
pub static GOALS: [GoalDef; 2] = [
    GoalDef {
        path: STATION_SHIELD,
        name: "Станционные щиты",
        weight: 1.0,
        requires_space: true,
        report: "Система противометеорной защиты\n\
             Станция находится в зоне, заполненной космическим мусором.\n\
             У вас есть прототип защитной системы, который необходимо развернуть для снижения \
             количества аварий из-за столкновений.\n\
             Вы можете заказать спутники и системы через отдел снабжения.",
    },
    GoalDef {
        path: DNA_VAULT,
        name: "ДНК хранилище",
        weight: 1.0,
        requires_space: false,
        // Built with the goal's sample counts ([`Goal::report`]).
        report: "",
    },
];

/// The supply packs a goal's `on_report` enables (`ORDER_SPECIAL_ENABLED`).
fn goal_packs(path: &str) -> &'static [&'static str] {
    match path {
        STATION_SHIELD => &[
            "/datum/supply_pack/engineering/shield_sat",
            "/datum/supply_pack/engineering/shield_sat_control",
        ],
        DNA_VAULT => &[
            "/datum/supply_pack/engineering/dna_vault",
            "/datum/supply_pack/engineering/dna_probes",
        ],
        _ => &[],
    }
}

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
    /// `/datum/station_goal/dna_vault`'s `animal_count`, `plant_count`, `human_count`.
    pub vault: Option<crate::genetics::dna_vault::VaultTargets>,
}

impl Goal {
    /// tg `get_report()`.
    pub fn report(&self) -> String {
        match &self.vault {
            Some(t) => crate::genetics::dna_vault::goal_report(t),
            None => self.def.report.to_owned(),
        }
    }
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

/// A meteor shield's `obj_flags & EMAGGED`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EmaggedShield;

/// The meteor shield's statics: `emagged_active_meteor_shields` (here the shields themselves,
/// so a shield counts down only after it counted up), `highest_emagged_threshold_reached`,
/// `shared_emag_cooldown`.
#[derive(Resource, Default, Debug)]
pub struct ShieldEmags {
    pub counted: Vec<Entity>,
    pub highest: u32,
    pub cooldown_until: f64,
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
            .init_resource::<ShieldEmags>()
            .add_systems(Update, (shield_meteors, forget_destroyed_shields).chain());
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
    let picked = pick(budget, false, &mut rng);
    let mut new = Vec::new();
    for def in picked {
        let vault = (def.path == DNA_VAULT)
            .then(|| crate::genetics::dna_vault::roll_targets(world, &mut rng));
        new.push(Goal {
            def,
            completed: false,
            vault,
        });
    }
    let mut goals = world.resource_mut::<StationGoals>();
    for goal in new {
        if !goals.has(goal.def.path) {
            info!("station goal: {}", goal.def.name);
            goals.goals.push(goal);
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
    let reports: Vec<String> = goals.goals.iter().map(Goal::report).collect();
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
        DNA_VAULT => goal
            .vault
            .is_some_and(|t| crate::genetics::dna_vault::goal_met(world, &t)),
        _ => false,
    }
}

/// tg `on_report()` of every goal (`send_roundstart_report`): its supply packs can be ordered.
pub fn on_report(world: &mut World) {
    let paths: Vec<&'static str> = world
        .get_resource::<StationGoals>()
        .map(|g| g.goals.iter().map(|goal| goal.def.path).collect())
        .unwrap_or_default();
    let Some(mut packs) = world.get_resource_mut::<crate::cargo::supply::SupplyPacks>() else {
        return;
    };
    for pack in paths.into_iter().flat_map(goal_packs) {
        if let Some(i) = packs.by_id(pack) {
            packs.packs[i].order_flags |= ss13_mapload::cargo::order_flags::SPECIAL_ENABLED;
        }
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

pub fn is_satellite(world: &World, e: Entity) -> bool {
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
    toggle(world, target, Some(client));
}

/// `/obj/machinery/satellite/toggle(user)`: the satellite control console calls it without a
/// user (no `do_after`, no messages). True when it switched.
pub fn toggle(world: &mut World, target: Entity, client: Option<Entity>) -> bool {
    let Some(&pos) = world.get::<TilePos>(target) else {
        return false;
    };
    if !is_satellite(world, target) {
        return false;
    }
    let on = !active(world, target);
    let name = world
        .get::<ss13_mapload::DmName>(target)
        .map(|n| n.name.trim_start_matches("\\improper ").to_owned())
        .unwrap_or_default();
    if on && !is_space(world, pos) {
        if let Some(client) = client {
            tell(
                world,
                client,
                Span::Warning,
                format!("You can only activate {name} in space."),
            );
        }
        return false;
    }
    if let Some(client) = client {
        let verb = if on { "activate" } else { "deactivate" };
        tell(world, client, Span::Notice, format!("You {verb} {name}."));
    }
    world
        .entity_mut(target)
        .insert((Satellite { active: on }, Anchored(on)));
    if world.get::<EmaggedShield>(target).is_some() {
        update_emagged(world, target, client);
    }
    // `update_icon_state`: "<base_icon_state>_<active|inactive>".
    let state = if on { "sat_active" } else { "sat_inactive" };
    let icon = world.get::<DmAtom>(target).map(|a| a.icon.clone());
    let id = icon.and_then(|icon| world.get_resource::<IconIndex>()?.get(&icon, state));
    if let (Some(id), Some(mut appearance)) = (id, world.get_mut::<Appearance>(target)) {
        appearance.icon_state = id;
    }
    update_coverage(world);
    true
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
/// space only (`space_los`) is destroyed (`shield_defense`), but a dark matt-eor destroys the
/// shield instead.
// Cost: meteors in flight (up to ~100) × active shields per tick, a line of at most 15 tiles
// for those in range.
fn shield_meteors(world: &mut World) {
    let mut meteors =
        world.query_filtered::<(Entity, &TilePos), With<crate::events::meteors::Meteor>>();
    let flying: Vec<(Entity, TilePos)> = meteors.iter(world).map(|(e, p)| (e, *p)).collect();
    if flying.is_empty() {
        return;
    }
    let mut sats = world.query::<(Entity, &Satellite, &DmObj, &TilePos)>();
    let mut shields: Vec<(Entity, TilePos)> = sats
        .iter(world)
        .filter(|(_, s, o, _)| s.active && is_under(&o.path, METEOR_SHIELD))
        .map(|(e, _, _, p)| (e, *p))
        .collect();
    if shields.is_empty() {
        return;
    }
    for (meteor, pos) in flying {
        let in_reach: Vec<(Entity, TilePos)> = shields
            .iter()
            .copied()
            .filter(|&(_, sat)| {
                sat.chebyshev(pos).is_some_and(|d| d <= KILL_RANGE as u32)
                    && crate::radiation::get_line(sat, pos)
                        .iter()
                        .all(|&p| is_space(world, p))
            })
            .collect();
        if in_reach.is_empty() {
            continue;
        }
        if !crate::events::meteors::is_dark_matteor(world, meteor) {
            crate::events::meteors::retarget_cargo(world, meteor, in_reach[0].1);
            world.despawn(meteor);
            continue;
        }
        // `dark_matteor/shield_defense`: the beam is reflected and the shield goes.
        let name = world
            .get::<ss13_mapload::DmName>(meteor)
            .map(|n| n.name.clone())
            .unwrap_or_default();
        for (sat, at) in in_reach {
            let text = format!("{}'s beam is reflected by {name}!", plain_name(world, sat));
            for message in crate::pull::visible_message(
                world,
                at,
                ChatChannel::Local,
                (Span::Danger, text),
                &[],
            ) {
                world.write_message(message);
            }
            world.despawn(sat);
            shields.retain(|&(s, _)| s != sat);
        }
    }
}

fn plain_name(world: &World, e: Entity) -> String {
    world
        .get::<ss13_mapload::DmName>(e)
        .map(|n| crate::vending::plain_name(&n.name).to_owned())
        .unwrap_or_default()
}

/// The meteor shield's family takes emags (`crate::emag`).
pub fn takes_emag(world: &World, target: Entity) -> bool {
    world
        .get::<DmObj>(target)
        .is_some_and(|o| is_under(&o.path, METEOR_SHIELD))
}

/// `/obj/machinery/satellite/meteor_shield/emag_act`: once per shield, one shield a minute
/// station-wide; the shield shows on GPS and, if active, counts.
pub fn emag_act(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if !takes_emag(world, target) {
        return false;
    }
    if world.get::<EmaggedShield>(target).is_some() {
        crate::balloon::balloon_alert(world, mob, target, "уже емагнуто!");
        return false;
    }
    let now = crate::tools::now(world);
    let until = world.resource::<ShieldEmags>().cooldown_until;
    if now < until {
        crate::balloon::balloon_alert(world, mob, target, "на перезарядке!");
        tell(
            world,
            client,
            Span::Warning,
            format!(
                "Последнему емагнутому спутнику требуется {} для перекалибровки. Емаг другого \
                 спутника в слишком короткое время может повредить сеть.",
                crate::jobs::display_time_text(until - now)
            ),
        );
        return false;
    }
    world.resource_mut::<ShieldEmags>().cooldown_until = now + EMAG_COOLDOWN_SECS;
    tell(
        world,
        client,
        Span::Notice,
        "Вы получаете доступ к режиму отладки спутника, и он начинает излучать странный сигнал, \
         увеличивая вероятность метеоритных ударов.",
    );
    world.entity_mut(target).insert((
        EmaggedShield,
        crate::devices::gps::Gps::fixed("Сигнал привлечения повреждённого метеоритного щита"),
    ));
    let text = format!(
        "Перекалибровка... Примерное время: {}.",
        crate::jobs::display_time_text(EMAG_COOLDOWN_SECS)
    );
    crate::atmos_machinery::machine_say(world, target, &text);
    if active(world, target) {
        update_emagged(world, target, Some(client));
    }
    true
}

/// `update_emagged_meteor_sat(user)`: an active emagged shield doubles the meteor chance and
/// counts toward the thresholds; an inactive one that counted halves it back.
pub fn update_emagged(world: &mut World, sat: Entity, client: Option<Entity>) {
    let mob = client.and_then(|c| world.get_resource::<crate::server::Players>()?.mob(c));
    let counted = world.resource::<ShieldEmags>().counted.contains(&sat);
    if !active(world, sat) {
        if !counted {
            return;
        }
        world
            .resource_mut::<ShieldEmags>()
            .counted
            .retain(|&e| e != sat);
        change_meteor_chance(world, false);
        if let Some(mob) = mob {
            crate::balloon::balloon_alert(
                world,
                mob,
                sat,
                "вероятность метеоров уменьшилась вдвое",
            );
        }
        return;
    }
    if counted {
        return;
    }
    change_meteor_chance(world, true);
    let reached = {
        let mut emags = world.resource_mut::<ShieldEmags>();
        emags.counted.push(sat);
        let n = emags.counted.len() as u32;
        (n > emags.highest).then(|| {
            emags.highest = n;
            n
        })
    };
    if let Some(mob) = mob {
        crate::balloon::balloon_alert(world, mob, sat, "вероятность метеоров удвоена");
    }
    if let Some(n) = reached {
        emag_threshold(world, sat, n);
    }
}

/// `handle_new_emagged_shield_threshold`.
fn emag_threshold(world: &mut World, sat: Entity, reached: u32) {
    let say = |world: &mut World, text: &str| crate::atmos_machinery::machine_say(world, sat, text);
    match EMAG_THRESHOLDS.iter().position(|&t| t == reached) {
        Some(0) => say(
            world,
            "Предупреждение. Вероятность метеоритного удара входит в опасный диапазон для более \
             экзотических метеоритов.",
        ),
        Some(1) => say(
            world,
            "Предупреждение. Риск сгущения тёмной материи входит в существующие диапазоны. \
             Дальнейшее вмешательство будет доложено.",
        ),
        Some(2) => {
            say(
                world,
                "Предупреждение. Дальнейшее вмешательство было доложено.",
            );
            crate::events::priority_announce(
                world,
                "Предупреждение о странном метеоритном сигнале",
                "Внимание! Вмешательство в работу метеоритных щитов подвергает станцию риску \
                 необычных и смертоносных столкновений с метеоритами. Пожалуйста, проверьте свои \
                 GPS-устройства на наличие странных сигналов и демонтируйте взломанные \
                 метеоритные щиты.",
            );
        }
        Some(3) => {
            say(
                world,
                "Предупреждение. Предупреждение. Тёмно-материальный метеор на курсе к станции.",
            );
            if let Err(e) =
                crate::events::force_event(world, crate::events::meteors::DARK_MATTEOR, None)
            {
                warn!("tampered meteor satellites: {e}");
            }
        }
        _ => {}
    }
}

/// `change_meteor_chance(mod)`: the meteor events' weights doubled or halved.
fn change_meteor_chance(world: &mut World, double: bool) {
    let Some(mut events) = world.get_resource_mut::<crate::events::Events>() else {
        return;
    };
    for control in events
        .controls
        .iter_mut()
        .filter(|c| METEOR_CHANCE_EVENTS.contains(&c.name))
    {
        control.weight = if double {
            control.weight.saturating_mul(2)
        } else {
            control.weight / 2
        };
    }
}

/// `meteor_shield/Destroy` of an emagged shield: a counted shield that is gone counts down.
fn forget_destroyed_shields(world: &mut World) {
    if world.resource::<ShieldEmags>().counted.is_empty() {
        return;
    }
    let gone: Vec<Entity> = world
        .resource::<ShieldEmags>()
        .counted
        .iter()
        .copied()
        .filter(|&e| world.get_entity(e).is_err())
        .collect();
    for e in gone {
        world
            .resource_mut::<ShieldEmags>()
            .counted
            .retain(|&c| c != e);
        change_meteor_chance(world, false);
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
        assert!(pick(0.0, false, &mut rng).is_empty());
        // More budget than goals: all of them, once.
        assert_eq!(pick(5.0, false, &mut rng).len(), GOALS.len());
        // The shield needs space; the vault does not.
        for _ in 0..8 {
            let planetary = pick(5.0, true, &mut rng);
            assert_eq!(planetary.len(), 1);
            assert_eq!(planetary[0].path, DNA_VAULT);
        }
    }

    #[test]
    fn report_and_admin_completion() {
        let mut world = World::new();
        world.insert_resource(StationGoalsConfig { budget: 1.0 });
        world.init_resource::<StationGoals>();
        assert!(report_section(&world).is_empty());
        generate(&mut world);
        assert_eq!(world.resource::<StationGoals>().goals.len(), 1);
        let goal = world.resource::<StationGoals>().goals[0].clone();
        let section = report_section(&world);
        assert!(section.starts_with("=== Цели на смену ==="), "{section}");
        match goal.def.path {
            STATION_SHIELD => assert!(section.contains("Система противометеорной защиты")),
            _ => {
                assert!(goal.vault.is_some());
                assert!(section.contains("#### ДНК хранилище"), "{section}");
            }
        }
        // A second roll never adds a type already there.
        generate(&mut world);
        let goals = &world.resource::<StationGoals>().goals;
        assert!(goals.iter().filter(|g| g.def.path == goal.def.path).count() == 1);
        set_completed(&mut world, goal.def.path, true);
        let goal = world.resource::<StationGoals>().goals[0].clone();
        assert!(check_completion(&mut world, &goal));
    }
}
