//! Roundstart antagonists after BandaStation's `SSdynamic` (`code/controllers/subsystem/dynamic/`,
//! values from `config/dynamic.toml` with `DYNAMIC_CONFIG_ENABLED`).
//!
//! At `SETTING_UP`, before the jobs are divided ([`prepare_roundstart`]): a dry run of
//! `divide_occupations` finds the ready players who would get a job (the antag candidates and
//! the population); a tier is rolled by weight for that population and gives how many roundstart
//! rulesets to spawn; rulesets are picked by weight (every BandaStation roundstart ruleset, so the
//! implemented ones keep their real share); each implemented ruleset selects up to its antag cap
//! (`ceil(population / 24)` traitors and vampires, `ceil(population / 29)` blood brothers and
//! changelings) of the
//! candidates who opted in (lobby "Антагонисты" toggle and the role's `be_special`) and bars them
//! from the protected and blacklisted jobs and the ruleset's own `blacklisted_roles` (the
//! vampire's chaplain and head of personnel; `SSjob.prevented_occupations`). After everyone spawned
//! ([`execute_roundstart`]) the selected players get their antagonist (`assign_role`).
//!
//! Admins can queue rulesets instead of the roll ([`DynamicConfig::forced_rulesets`]); tests use
//! that to get deterministic antagonists.
// shortcut: only the traitor, blood brothers, the vampire, the changeling, the nuclear
// operatives, the blood cult, the heretics, the revolution, the wizard and the malfunctioning
// AI (forced into the AI job, `crate::silicon::malf`) are implemented; other picked
// rulesets spawn nobody (logged). Midround and latejoin rulesets: [`super::midround`]. BandaStation's weighted
// selection by antag history (`ANTAG_WEIGHTED_SELECTION`) is off in its config and needs a
// database.

use std::collections::BTreeSet;

use bevy::prelude::*;
use ss13_mapload::MapSeed;
use ss13_mapload::jobs::JobsData;
use ss13_mapload::spawner::Rng;

use super::changeling::ROLE_CHANGELING;
use super::cult::ROLE_CULTIST;
use super::heretic::ROLE_HERETIC;
use super::vampire::ROLE_VAMPIRE;
use super::{ROLE_BROTHER, ROLE_TRAITOR};
use crate::jobs::{Jobs, PlayerJob};
use crate::round::{Assignment, JobPreferences, divide_occupations};
use crate::server::Players;

/// On a client connection: the antagonist preferences, kept for the connection (in memory).
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct AntagPreferences {
    /// BandaStation `/datum/preference/toggle/antag_roll` (default on).
    pub antag_roll: bool,
    /// tg `be_special` (default empty).
    pub be_special: BTreeSet<String>,
}

impl Default for AntagPreferences {
    fn default() -> Self {
        Self {
            antag_roll: true,
            be_special: BTreeSet::new(),
        }
    }
}

impl AntagPreferences {
    pub fn wants(&self, role: &str) -> bool {
        self.antag_roll && self.be_special.contains(role)
    }
}

/// Insert before `GameServerPlugin` to override. The default reads `SS13_DEV_TRAITOR` (debug
/// builds): with the lobby skipped, every joining body becomes a traitor (an admin's "Make
/// Traitor"); `unlocked` also unlocks and opens its uplink (screenshots); `brother` makes it a
/// blood brother with a team of its own instead, `vampire` a vampire, `changeling` a changeling,
/// `nukeop` a nuclear operative aboard the infiltrator (the first one leads), `rev` a
/// revolutionary (the first one a head revolutionary), `wizard` a wizard in its den.
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct DynamicConfig {
    /// Rulesets admins queued (config tags): replaces the tier roll and the ruleset picks.
    pub forced_rulesets: Option<Vec<&'static str>>,
    /// `PROTECT_ROLES_FROM_ANTAGONIST` (on in BandaStation's `game_options.txt`).
    pub protect_roles: bool,
    /// Seed of the rolls; `None`: from the map seed.
    pub seed: Option<u64>,
    pub dev_traitor: DevTraitor,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DevTraitor {
    #[default]
    Off,
    On,
    Unlocked,
    Brother,
    Vampire,
    Changeling,
    Nukeop,
    Cult,
    Heretic,
    Revolution,
    Wizard,
}

impl Default for DynamicConfig {
    fn default() -> Self {
        let dev = match std::env::var("SS13_DEV_TRAITOR").as_deref() {
            Ok("1") => DevTraitor::On,
            Ok("unlocked") => DevTraitor::Unlocked,
            Ok("brother") => DevTraitor::Brother,
            Ok("vampire") => DevTraitor::Vampire,
            Ok("changeling") => DevTraitor::Changeling,
            Ok("nukeop") => DevTraitor::Nukeop,
            Ok("cult") => DevTraitor::Cult,
            Ok("heretic") => DevTraitor::Heretic,
            Ok("rev") => DevTraitor::Revolution,
            Ok("wizard") => DevTraitor::Wizard,
            _ => DevTraitor::Off,
        };
        if dev != DevTraitor::Off && !cfg!(debug_assertions) {
            warn!("SS13_DEV_TRAITOR is ignored: it exists in debug builds only");
        }
        Self {
            forced_rulesets: None,
            protect_roles: true,
            seed: None,
            dev_traitor: if cfg!(debug_assertions) {
                dev
            } else {
                DevTraitor::Off
            },
        }
    }
}

/// The rolls of dynamic.
#[derive(Resource)]
pub struct DynamicRng(pub Rng);

pub(crate) fn init(world: &mut World) {
    let seed = world
        .resource::<DynamicConfig>()
        .seed
        .or_else(|| world.get_resource::<MapSeed>().map(|s| s.0))
        .unwrap_or(0);
    world.insert_resource(DynamicRng(Rng::new(seed ^ 0x0064_796e_616d_6963)));
}

/// BYOND `pick_weight`: an index by weight; `None` when every weight is 0.
pub fn pick_weight(weights: &[u32], rng: &mut Rng) -> Option<usize> {
    let total: u64 = weights.iter().map(|&w| u64::from(w)).sum();
    if total == 0 {
        return None;
    }
    let mut r = rng.range(1, total as i64);
    weights.iter().position(|&w| {
        r -= i64::from(w);
        r <= 0
    })
}

/// BYOND `shuffle`.
pub fn shuffle<T>(items: &mut [T], rng: &mut Rng) {
    for i in (1..items.len()).rev() {
        let j = rng.range(0, i as i64) as usize;
        items.swap(i, j);
    }
}

/// A `/datum/dynamic_tier`'s roundstart settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tier {
    pub tier: u8,
    pub name: &'static str,
    pub weight: u32,
    pub min_pop: u32,
    pub low: u32,
    pub high: u32,
    pub half_range_pop_threshold: u32,
    pub full_range_pop_threshold: u32,
}

/// BandaStation `config/dynamic.toml` tiers (`ruleset_type_settings.roundstart`).
pub const TIERS: [Tier; 5] = [
    Tier {
        tier: 0,
        name: "Greenshift",
        weight: 2,
        min_pop: 0,
        low: 0,
        high: 0,
        half_range_pop_threshold: 25,
        full_range_pop_threshold: 50,
    },
    Tier {
        tier: 1,
        name: "Low Chaos",
        weight: 8,
        min_pop: 0,
        low: 1,
        high: 1,
        half_range_pop_threshold: 25,
        full_range_pop_threshold: 40,
    },
    Tier {
        tier: 2,
        name: "Low-Medium Chaos",
        weight: 46,
        min_pop: 0,
        low: 1,
        high: 2,
        half_range_pop_threshold: 25,
        full_range_pop_threshold: 40,
    },
    Tier {
        tier: 3,
        name: "Medium-High Chaos",
        weight: 36,
        min_pop: 0,
        low: 2,
        high: 3,
        half_range_pop_threshold: 25,
        full_range_pop_threshold: 40,
    },
    Tier {
        tier: 4,
        name: "High Chaos",
        weight: 10,
        min_pop: 25,
        low: 3,
        high: 4,
        half_range_pop_threshold: 25,
        full_range_pop_threshold: 40,
    },
];

/// `pick_tier`: by weight among the tiers the population allows.
pub fn pick_tier(population: u32, rng: &mut Rng) -> &'static Tier {
    let weights: Vec<u32> = TIERS
        .iter()
        .map(|t| if population < t.min_pop { 0 } else { t.weight })
        .collect();
    &TIERS[pick_weight(&weights, rng).unwrap_or(0)]
}

/// `set_tier`: `rand(low, high)`, the high end quartered at or below the half range threshold
/// and halved at or below the full range one.
pub fn rulesets_to_spawn(tier: &Tier, population: u32, rng: &mut Rng) -> u32 {
    let mut high = tier.high;
    if population <= tier.half_range_pop_threshold {
        high = tier.low.max((f64::from(high) * 0.25).ceil() as u32);
    } else if population <= tier.full_range_pop_threshold {
        high = tier.low.max((f64::from(high) * 0.5).ceil() as u32);
    }
    rng.range(i64::from(tier.low), i64::from(high)) as u32
}

/// A BandaStation roundstart ruleset's selection values (`config/dynamic.toml`).
#[derive(Clone, Copy, Debug)]
pub struct Ruleset {
    pub config_tag: &'static str,
    /// Per tier 1..=4 (`get_tier_specific_value`; tier 0 never spawns rulesets).
    pub weight: [u32; 4],
    pub min_pop: u32,
    pub repeatable: bool,
    /// `RULESET_HIGH_IMPACT`.
    pub high_impact: bool,
}

pub const TRAITOR_RULESET: &str = "Roundstart Traitor";
pub const BROTHER_RULESET: &str = "Roundstart Blood Brothers";
/// `/datum/dynamic_ruleset/roundstart/traitor` `max_antag_cap = list("denominator" = 24)`.
pub const TRAITOR_CAP_DENOMINATOR: u32 = 24;
/// `/datum/dynamic_ruleset/roundstart/blood_brother` `max_antag_cap = list("denominator" = 29)`.
pub const BROTHER_CAP_DENOMINATOR: u32 = 29;
pub const VAMPIRE_RULESET: &str = "Roundstart Vampire";
/// `/datum/dynamic_ruleset/roundstart/vampire` `max_antag_cap = list("denominator" = 24)`.
pub const VAMPIRE_CAP_DENOMINATOR: u32 = 24;
pub const CHANGELING_RULESET: &str = "Roundstart Changeling";
pub const NUKEOPS_RULESET: &str = "Roundstart Nukeops";
/// `/datum/dynamic_ruleset/roundstart/nukies` `min_antag_cap = list("denominator" = 18,
/// "offset" = 1)` (no `max_antag_cap`: the same).
pub const NUKEOPS_CAP_DENOMINATOR: u32 = 18;
pub const NUKEOPS_CAP_OFFSET: u32 = 1;
/// `/datum/dynamic_ruleset/roundstart/changeling` `max_antag_cap = list("denominator" = 29)`.
pub const CHANGELING_CAP_DENOMINATOR: u32 = 29;

pub const CULT_RULESET: &str = "Roundstart Blood Cult";
/// `/datum/dynamic_ruleset/roundstart/blood_cult`
/// `min_antag_cap = list("denominator" = 20, "offset" = 1)`, no `max_antag_cap`: exactly that
/// many or nothing.
pub const CULT_CAP_DENOMINATOR: u32 = 20;
pub const CULT_CAP_OFFSET: u32 = 1;

pub const HERETIC_RULESET: &str = "Roundstart Heretics";
/// `/datum/dynamic_ruleset/roundstart/wizard`: `max_antag_cap = 1`, an invader (no station job).
pub const WIZARD_RULESET: &str = "Roundstart Wizard";
pub const REVOLUTION_RULESET: &str = "Roundstart Revolution";
/// `/datum/dynamic_ruleset/roundstart/revolution` `min_antag_cap = 1`, `max_antag_cap = 3`.
pub const REVOLUTION_CAPS: (u32, u32) = (3, 1);
/// `/datum/dynamic_ruleset/roundstart/malf_ai`: one, forced into the AI job.
pub const MALF_RULESET: &str = "Roundstart Malfunctioning AI";
/// `/datum/dynamic_ruleset/roundstart/heretic` `max_antag_cap = list("denominator" = 24)`.
pub const HERETIC_CAP_DENOMINATOR: u32 = 24;

/// The `pref_flag` and the cap denominator of an implemented ruleset (`min_antag_cap` is 1 for
/// both); `None` for the rest.
pub fn implemented(config_tag: &str) -> Option<(&'static str, u32)> {
    match config_tag {
        TRAITOR_RULESET => Some((ROLE_TRAITOR, TRAITOR_CAP_DENOMINATOR)),
        BROTHER_RULESET => Some((ROLE_BROTHER, BROTHER_CAP_DENOMINATOR)),
        VAMPIRE_RULESET => Some((ROLE_VAMPIRE, VAMPIRE_CAP_DENOMINATOR)),
        CHANGELING_RULESET => Some((ROLE_CHANGELING, CHANGELING_CAP_DENOMINATOR)),
        CULT_RULESET => Some((ROLE_CULTIST, CULT_CAP_DENOMINATOR)),
        HERETIC_RULESET => Some((ROLE_HERETIC, HERETIC_CAP_DENOMINATOR)),
        // Fixed caps ([`ruleset_caps`]).
        REVOLUTION_RULESET => Some((super::revolution::ROLE_REV_HEAD, 1)),
        _ => None,
    }
}

/// A ruleset's own `blacklisted_roles` (`config/dynamic.toml`), job titles.
pub fn ruleset_blacklisted_roles(config_tag: &str) -> &'static [&'static str] {
    match config_tag {
        VAMPIRE_RULESET => &["Chaplain", "Head of Personnel"],
        // The config's head of personnel and `get_always_blacklisted_roles() | JOB_CHAPLAIN`.
        CULT_RULESET => &["Head of Personnel", "Chaplain"],
        // `get_always_blacklisted_roles`: every head of staff.
        REVOLUTION_RULESET => &super::revolution::server::HEAD_JOBS,
        _ => &[],
    }
}
/// `repeatable_weight_decrease` of every roundstart ruleset.
const REPEATABLE_WEIGHT_DECREASE: u32 = 2;

const fn flat(w: u32) -> [u32; 4] {
    [w; 4]
}

pub const ROUNDSTART_RULESETS: [Ruleset; 12] = [
    Ruleset {
        config_tag: TRAITOR_RULESET,
        weight: flat(10),
        min_pop: 3,
        repeatable: true,
        high_impact: false,
    },
    Ruleset {
        config_tag: VAMPIRE_RULESET,
        weight: flat(10),
        min_pop: 3,
        repeatable: true,
        high_impact: false,
    },
    Ruleset {
        config_tag: MALF_RULESET,
        weight: [0, 1, 3, 3],
        min_pop: 30,
        repeatable: false,
        high_impact: true,
    },
    Ruleset {
        config_tag: BROTHER_RULESET,
        weight: flat(5),
        min_pop: 10,
        repeatable: true,
        high_impact: false,
    },
    Ruleset {
        config_tag: CHANGELING_RULESET,
        weight: flat(3),
        min_pop: 15,
        repeatable: true,
        high_impact: false,
    },
    Ruleset {
        config_tag: "Roundstart Blood Worm",
        weight: flat(1),
        min_pop: 10,
        repeatable: false,
        high_impact: false,
    },
    Ruleset {
        config_tag: HERETIC_RULESET,
        weight: flat(3),
        min_pop: 30,
        repeatable: true,
        high_impact: false,
    },
    Ruleset {
        config_tag: WIZARD_RULESET,
        weight: [0, 0, 1, 2],
        min_pop: 30,
        repeatable: false,
        high_impact: true,
    },
    Ruleset {
        config_tag: CULT_RULESET,
        weight: [0, 1, 3, 3],
        min_pop: 30,
        repeatable: false,
        high_impact: true,
    },
    Ruleset {
        config_tag: NUKEOPS_RULESET,
        weight: [0, 1, 3, 3],
        min_pop: 30,
        repeatable: false,
        high_impact: true,
    },
    Ruleset {
        config_tag: REVOLUTION_RULESET,
        weight: [0, 1, 3, 3],
        min_pop: 30,
        repeatable: false,
        high_impact: true,
    },
    Ruleset {
        config_tag: "Roundstart Spies",
        weight: [0, 1, 3, 3],
        min_pop: 10,
        repeatable: true,
        high_impact: false,
    },
];

/// `pick_roundstart_rulesets`: `count` picks by weight (`get_weight` for the population and
/// tier); a repeatable pick loses `repeatable_weight_decrease`, a non-repeatable one leaves the
/// pool, and below the high tier a high impact pick removes every other high impact ruleset.
pub fn pick_roundstart_rulesets(
    count: u32,
    population: u32,
    tier: u8,
    rng: &mut Rng,
) -> Vec<&'static str> {
    let mut weights: Vec<u32> = ROUNDSTART_RULESETS
        .iter()
        .map(|r| {
            if tier == 0 || r.min_pop > population {
                0
            } else {
                r.weight[usize::from(tier - 1)]
            }
        })
        .collect();
    let mut picked = Vec::new();
    for _ in 0..count {
        let Some(i) = pick_weight(&weights, rng) else {
            break;
        };
        let ruleset = &ROUNDSTART_RULESETS[i];
        picked.push(ruleset.config_tag);
        if tier != 4 && ruleset.high_impact {
            for (w, r) in weights.iter_mut().zip(&ROUNDSTART_RULESETS) {
                if r.high_impact {
                    *w = 0;
                }
            }
        }
        weights[i] = if ruleset.repeatable {
            weights[i].saturating_sub(REPEATABLE_WEIGHT_DECREASE)
        } else {
            0
        };
    }
    picked
}

/// `get_antag_cap` of the ruleset's `max_antag_cap || min_antag_cap` and `min_antag_cap`
/// (`ceil(population / denominator) + offset`): (max, min) selected players.
pub fn ruleset_caps(config_tag: &str, population: u32, denominator: u32) -> (u32, u32) {
    match config_tag {
        CULT_RULESET => {
            let cap = antag_cap(population, denominator) + CULT_CAP_OFFSET;
            (cap, cap)
        }
        REVOLUTION_RULESET => REVOLUTION_CAPS,
        _ => (antag_cap(population, denominator), 1),
    }
}

/// `get_antag_cap` of `list("denominator" = d)`: `ceil(population / d)`.
pub fn antag_cap(population: u32, denominator: u32) -> u32 {
    population.div_ceil(denominator)
}

/// `get_blacklisted_roles`: the jobs a selected antagonist may not get (config protected and
/// blacklisted jobs, and the AI and cyborg always).
pub fn blacklisted_jobs(jobs: &JobsData, protect_roles: bool) -> Vec<usize> {
    jobs.jobs
        .iter()
        .enumerate()
        .filter(|(_, j)| {
            j.antag_blacklisted
                || (protect_roles && j.antag_protected)
                || j.title == "AI"
                || j.title == "Cyborg"
        })
        .map(|(i, _)| i)
        .collect()
}

/// The roundstart rulesets the round will spawn, in order (`queued_rulesets`).
pub fn roll_rulesets(config: &DynamicConfig, population: u32, rng: &mut Rng) -> Vec<&'static str> {
    roll_tier_and_rulesets(config, population, rng).1
}

/// [`roll_rulesets`] with the rolled tier (`None` when admins forced the rulesets: the tier is
/// rolled after the selection then, keeping the forced selection's rolls).
pub fn roll_tier_and_rulesets(
    config: &DynamicConfig,
    population: u32,
    rng: &mut Rng,
) -> (Option<&'static Tier>, Vec<&'static str>) {
    if let Some(forced) = &config.forced_rulesets {
        return (None, forced.clone());
    }
    let tier = pick_tier(population, rng);
    let count = rulesets_to_spawn(tier, population, rng);
    let picked = pick_roundstart_rulesets(count, population, tier.tier, rng);
    info!(
        "dynamic: tier {} ({}), population {population}, {count} roundstart rulesets: {picked:?}",
        tier.tier, tier.name
    );
    (Some(tier), picked)
}

/// `prepare_execution` of one ruleset: up to `ceil(population / denominator)` of the willing
/// `candidates`, shuffled (`select_candidates` without weighted selection). `None` when fewer
/// than `min_antag_cap` (1) are willing. The selected leave `candidates`.
pub fn select_candidates(
    candidates: &mut Vec<usize>,
    willing: impl Fn(usize) -> bool,
    population: u32,
    denominator: u32,
    rng: &mut Rng,
) -> Option<Vec<usize>> {
    let cap = antag_cap(population, denominator);
    select_up_to(candidates, willing, (cap, 1), rng)
}

/// [`select_candidates`] with explicit (max, min) caps ([`ruleset_caps`]): `None` (nobody leaves
/// `candidates`) when fewer than `min` are willing.
pub fn select_up_to(
    candidates: &mut Vec<usize>,
    willing: impl Fn(usize) -> bool,
    (max, min): (u32, u32),
    rng: &mut Rng,
) -> Option<Vec<usize>> {
    let mut valid: Vec<usize> = candidates.iter().copied().filter(|&c| willing(c)).collect();
    shuffle(&mut valid, rng);
    valid.truncate(max as usize);
    if valid.is_empty() || valid.len() < min as usize {
        return None;
    }
    candidates.retain(|c| !valid.contains(c));
    Some(valid)
}

/// The nuclear operatives' cap: `ceil(population / 18) + 1`, both minimum and maximum.
pub fn nukeops_cap(population: u32) -> u32 {
    population.div_ceil(NUKEOPS_CAP_DENOMINATOR) + NUKEOPS_CAP_OFFSET
}

/// tg `select_roundstart_antagonists`, before the real `divide_occupations`: picks the
/// antagonists among `ready` (client, job preferences clone) and bars their preferences from the
/// blacklisted jobs. Returns the selected clients with their ruleset.
pub fn prepare_roundstart(
    world: &mut World,
    ready: &mut [(Entity, JobPreferences)],
) -> Vec<(Entity, &'static str)> {
    // The dry run (`divide_occupations(pure = TRUE)`): who would get a job at all.
    let assigned = {
        let mut jobs = world.resource_mut::<Jobs>();
        let mut dry = jobs.snapshot();
        let prefs: Vec<&JobPreferences> = ready.iter().map(|(_, p)| p).collect();
        divide_occupations(&mut dry, &prefs)
    };
    let mut candidates: Vec<usize> = (0..ready.len())
        .filter(|&i| matches!(assigned[i], Assignment::Job(_)))
        .collect();
    let population = candidates.len() as u32;
    let config = world.resource::<DynamicConfig>().clone();
    let blacklist = blacklisted_jobs(&world.resource::<Jobs>().data, config.protect_roles);
    let mut rng = world
        .remove_resource::<DynamicRng>()
        .map_or(Rng::new(0), |r| r.0);
    let (tier, rulesets) = roll_tier_and_rulesets(&config, population, &mut rng);
    let mut selected: Vec<(usize, &'static str)> = Vec::new();
    for ruleset in rulesets {
        let (role, caps) = if ruleset == NUKEOPS_RULESET {
            let cap = nukeops_cap(population);
            (super::nukeop::ROLE_OPERATIVE, (cap, cap))
        } else if ruleset == WIZARD_RULESET {
            (super::wizard::ROLE_WIZARD, (1, 1))
        } else if ruleset == MALF_RULESET {
            // `ruleset_forced_job_check`: the AI job must exist.
            if world.resource::<Jobs>().data.job("AI").is_none() {
                info!("dynamic: {ruleset} has no AI job, nobody spawns for it");
                continue;
            }
            (crate::silicon::malf::ROLE_MALF, (1, 1))
        } else {
            let Some((role, denominator)) = implemented(ruleset) else {
                info!("dynamic: {ruleset} is not implemented here, nobody spawns for it");
                continue;
            };
            (role, ruleset_caps(ruleset, population, denominator))
        };
        let willing: Vec<bool> = ready
            .iter()
            .map(|(client, _)| {
                world
                    .get::<AntagPreferences>(*client)
                    .is_some_and(|p| p.wants(role))
            })
            .collect();
        match select_up_to(&mut candidates, |c| willing[c], caps, &mut rng) {
            Some(picked) => selected.extend(picked.into_iter().map(|i| (i, ruleset))),
            None => info!(
                "dynamic: {ruleset} preparation failed: not enough eligible candidates (need {})",
                caps.1
            ),
        }
    }
    let tier = tier.unwrap_or_else(|| pick_tier(population, &mut rng));
    world.insert_resource(DynamicRng(rng));
    let mut executed: Vec<&'static str> = selected.iter().map(|&(_, r)| r).collect();
    executed.dedup();
    super::midround::start(world, tier, population, executed);
    for &(i, ruleset) in &selected {
        let data = &world.resource::<Jobs>().data;
        let own = ruleset_blacklisted_roles(ruleset)
            .iter()
            .filter_map(|title| data.job(title));
        ready[i].1.prevented = blacklist.iter().copied().chain(own).collect();
        // `prepare_for_role`: `SSjob.forced_occupations[candidate] = /datum/job/ai`.
        if ruleset == MALF_RULESET
            && let Some(ai) = data.job("AI")
        {
            ready[i].1.levels = std::iter::once((ai, crate::lobby::JobPriority::High)).collect();
            ready[i].1.prevented = (0..data.jobs.len()).filter(|&j| j != ai).collect();
        }
    }
    let clients: Vec<(Entity, &'static str)> =
        selected.iter().map(|&(i, r)| (ready[i].0, r)).collect();
    info!("dynamic: roundstart antagonists: {clients:?}");
    clients
}

/// The rulesets' `execute()` after job assignment: `assign_role` for every selected player who
/// got a body.
pub fn execute_roundstart(world: &mut World, selected: &[(Entity, &'static str)]) {
    let mut cultists = Vec::new();
    for &(client, ruleset) in selected {
        // Spawned aboard the infiltrator or in the wizard's den by the round setup
        // (`RULESET_INVADER`, no job).
        if ruleset == NUKEOPS_RULESET || ruleset == WIZARD_RULESET {
            continue;
        }
        let mob = world.resource::<Players>().mob(client);
        // The AI's client controls its eye; the antagonist is the core.
        if ruleset == MALF_RULESET {
            let core = mob.and_then(|m| world.get::<crate::silicon::ai::AiEye>(m).map(|e| e.ai));
            match core {
                Some(core) => {
                    crate::silicon::malf::add_malf(world, core);
                }
                None => info!("dynamic: selected {ruleset} antagonist {client} is no AI, skipped"),
            }
            continue;
        }
        let Some(mob) = mob.filter(|&m| world.get::<PlayerJob>(m).is_some()) else {
            info!("dynamic: selected {ruleset} antagonist {client} has no body, skipped");
            continue;
        };
        match ruleset {
            BROTHER_RULESET => {
                super::brother::add_brother(world, mob, None);
            }
            VAMPIRE_RULESET => {
                super::vampire::server::add_vampire(world, mob);
            }
            CHANGELING_RULESET => {
                super::changeling::server::add_changeling(world, mob);
            }
            CULT_RULESET => cultists.push(mob),
            HERETIC_RULESET => {
                super::heretic::server::add_heretic(world, mob);
            }
            REVOLUTION_RULESET => super::revolution::server::make_dormant(world, mob, false),
            _ => {
                super::traitor::add_traitor(world, mob);
            }
        }
    }
    // `blood_cult/execute`: one team for all, then `setup_objectives`.
    if !cultists.is_empty() {
        super::cult::server::start_cult(world, &cultists);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traitor_cap_scales_with_population() {
        let cases = [(1, 1), (3, 1), (24, 1), (25, 2), (48, 2), (49, 3), (100, 5)];
        for (population, cap) in cases {
            assert_eq!(
                antag_cap(population, TRAITOR_CAP_DENOMINATOR),
                cap,
                "{population}"
            );
        }
        // One ruleset selects at most the cap, only among the willing.
        let mut rng = Rng::new(1);
        let mut candidates: Vec<usize> = (0..30).collect();
        let picked = select_candidates(
            &mut candidates,
            |c| c % 2 == 0,
            30,
            TRAITOR_CAP_DENOMINATOR,
            &mut rng,
        )
        .unwrap();
        assert_eq!(picked.len(), 2);
        assert!(picked.iter().all(|c| c % 2 == 0));
        assert_eq!(candidates.len(), 28);
        // Nobody willing: the ruleset fails.
        assert_eq!(
            select_candidates(
                &mut candidates,
                |_| false,
                30,
                TRAITOR_CAP_DENOMINATOR,
                &mut rng
            ),
            None
        );
    }

    #[test]
    fn brother_rulesets_spawn_one_per_29_players() {
        assert_eq!(implemented(BROTHER_RULESET), Some((ROLE_BROTHER, 29)));
        assert_eq!(implemented(TRAITOR_RULESET), Some((ROLE_TRAITOR, 24)));
        assert_eq!(implemented("Roundstart Wizard"), None);
        assert_eq!(
            implemented(CHANGELING_RULESET),
            Some((ROLE_CHANGELING, CHANGELING_CAP_DENOMINATOR))
        );
        assert_eq!(
            ruleset_blacklisted_roles(CHANGELING_RULESET),
            &[] as &[&str]
        );
        let cases = [(10, 1), (29, 1), (30, 2), (58, 2), (59, 3)];
        for (population, cap) in cases {
            assert_eq!(
                antag_cap(population, BROTHER_CAP_DENOMINATOR),
                cap,
                "{population}"
            );
        }
        // A traitor and a brother ruleset at 30 players: two traitors, then two brothers, never
        // the same player twice.
        let mut rng = Rng::new(3);
        let mut candidates: Vec<usize> = (0..30).collect();
        let traitors = select_candidates(
            &mut candidates,
            |_| true,
            30,
            TRAITOR_CAP_DENOMINATOR,
            &mut rng,
        )
        .unwrap();
        let brothers = select_candidates(
            &mut candidates,
            |_| true,
            30,
            BROTHER_CAP_DENOMINATOR,
            &mut rng,
        )
        .unwrap();
        assert_eq!((traitors.len(), brothers.len()), (2, 2));
        assert!(brothers.iter().all(|b| !traitors.contains(b)));
        // Forced queue: exactly what the admin queued.
        let config = DynamicConfig {
            forced_rulesets: Some(vec![BROTHER_RULESET]),
            protect_roles: true,
            seed: None,
            dev_traitor: DevTraitor::Off,
        };
        assert_eq!(roll_rulesets(&config, 1, &mut rng), [BROTHER_RULESET]);
    }

    #[test]
    fn blood_cult_ruleset_follows_its_config() {
        assert_eq!(implemented(CULT_RULESET), Some((ROLE_CULTIST, 20)));
        assert_eq!(
            ruleset_blacklisted_roles(CULT_RULESET),
            ["Head of Personnel", "Chaplain"]
        );
        let cult = ROUNDSTART_RULESETS
            .iter()
            .find(|r| r.config_tag == CULT_RULESET)
            .unwrap();
        assert_eq!(cult.weight, [0, 1, 3, 3]);
        assert_eq!(cult.min_pop, 30);
        assert!(cult.high_impact && !cult.repeatable);
        // `ceil(pop / 20) + 1`, and no fewer.
        assert_eq!(ruleset_caps(CULT_RULESET, 4, 20), (2, 2));
        assert_eq!(ruleset_caps(CULT_RULESET, 30, 20), (3, 3));
        assert_eq!(ruleset_caps(CULT_RULESET, 41, 20), (4, 4));
        assert_eq!(ruleset_caps(TRAITOR_RULESET, 30, 24), (2, 1));
        let mut rng = Rng::new(3);
        let mut candidates = vec![0, 1, 2];
        assert_eq!(
            select_up_to(&mut candidates, |c| c == 0, (2, 2), &mut rng),
            None,
            "one willing cultist is not a cult"
        );
        assert_eq!(candidates, [0, 1, 2]);
        let mut picked = select_up_to(&mut candidates, |c| c != 2, (2, 2), &mut rng).unwrap();
        picked.sort();
        assert_eq!(picked, [0, 1]);
        assert_eq!(candidates, [2]);
    }

    #[test]
    fn heretic_ruleset_follows_its_config() {
        // `max_antag_cap.denominator = 24`, `min_pop = 30`, weight 3, repeatable, no blacklist.
        assert_eq!(implemented(HERETIC_RULESET), Some((ROLE_HERETIC, 24)));
        assert_eq!(ruleset_blacklisted_roles(HERETIC_RULESET), &[] as &[&str]);
        let heretics = ROUNDSTART_RULESETS
            .iter()
            .find(|r| r.config_tag == HERETIC_RULESET)
            .unwrap();
        assert_eq!(heretics.weight, flat(3));
        assert_eq!(heretics.min_pop, 30);
        assert!(heretics.repeatable && !heretics.high_impact);
        assert_eq!(ruleset_caps(HERETIC_RULESET, 30, 24), (2, 1));
        assert_eq!(ruleset_caps(HERETIC_RULESET, 49, 24), (3, 1));
    }

    #[test]
    fn vampire_ruleset_follows_its_config() {
        // `max_antag_cap.denominator = 24`, `blacklisted_roles = ["Chaplain", "Head of Personnel"]`.
        assert_eq!(implemented(VAMPIRE_RULESET), Some((ROLE_VAMPIRE, 24)));
        assert_eq!(
            ruleset_blacklisted_roles(VAMPIRE_RULESET),
            ["Chaplain", "Head of Personnel"]
        );
        assert!(ruleset_blacklisted_roles(TRAITOR_RULESET).is_empty());
        let vampire = ROUNDSTART_RULESETS
            .iter()
            .find(|r| r.config_tag == VAMPIRE_RULESET)
            .unwrap();
        assert_eq!((vampire.weight, vampire.min_pop), ([10; 4], 3));
    }

    #[test]
    fn nukeops_need_their_full_cap() {
        // `ceil(population / 18) + 1`, minimum and maximum alike.
        assert_eq!(nukeops_cap(1), 2);
        assert_eq!(nukeops_cap(30), 3);
        assert_eq!(nukeops_cap(36), 3);
        assert_eq!(nukeops_cap(37), 4);
        let nukeops = ROUNDSTART_RULESETS
            .iter()
            .find(|r| r.config_tag == NUKEOPS_RULESET)
            .unwrap();
        assert_eq!((nukeops.weight, nukeops.min_pop), ([0, 1, 3, 3], 30));
        assert!(nukeops.high_impact && !nukeops.repeatable);
        let mut rng = Rng::new(5);
        let mut candidates: Vec<usize> = (0..30).collect();
        // Two willing for a cap of three: the ruleset fails and nobody leaves the pool.
        assert_eq!(
            select_up_to(&mut candidates, |c| c < 2, (3, 3), &mut rng),
            None
        );
        assert_eq!(candidates.len(), 30);
        let picked = select_up_to(&mut candidates, |c| c < 5, (3, 3), &mut rng).unwrap();
        assert_eq!(picked.len(), 3);
        assert_eq!(candidates.len(), 27);
    }

    #[test]
    fn ruleset_count_follows_the_tier_and_population() {
        let low_medium = &TIERS[2];
        let high = &TIERS[4];
        for seed in 0..50 {
            let mut rng = Rng::new(seed);
            // At 25 or fewer the high end is quartered: 1..=max(1, ceil(2 / 4)) = 1.
            assert_eq!(rulesets_to_spawn(low_medium, 20, &mut rng), 1);
            // Up to 40 halved: 1..=1.
            assert_eq!(rulesets_to_spawn(low_medium, 40, &mut rng), 1);
            let n = rulesets_to_spawn(low_medium, 41, &mut rng);
            assert!((1..=2).contains(&n));
            let n = rulesets_to_spawn(high, 30, &mut rng);
            assert!((3..=3).contains(&n), "{n}");
            assert_eq!(rulesets_to_spawn(&TIERS[0], 100, &mut rng), 0);
            // High chaos needs 25 players.
            assert_ne!(pick_tier(10, &mut rng).tier, 4);
        }
    }

    #[test]
    fn ruleset_picks_respect_min_pop_and_weights() {
        let mut traitors = 0;
        for seed in 0..200 {
            let mut rng = Rng::new(seed);
            let picked = pick_roundstart_rulesets(1, 5, 2, &mut rng);
            // At 5 players only traitors and vampires (min_pop 3) can roll, half each.
            assert!(matches!(
                picked[..],
                [TRAITOR_RULESET | "Roundstart Vampire"]
            ));
            traitors += usize::from(picked[0] == TRAITOR_RULESET);
            // The green tier spawns nothing.
            assert!(pick_roundstart_rulesets(3, 50, 0, &mut rng).is_empty());
            // Below the high tier a high impact pick blocks the others.
            let picked = pick_roundstart_rulesets(4, 60, 3, &mut rng);
            let high = picked
                .iter()
                .filter(|p| {
                    ROUNDSTART_RULESETS
                        .iter()
                        .any(|r| r.config_tag == **p && r.high_impact)
                })
                .count();
            assert!(high <= 1, "{picked:?}");
        }
        assert!((70..130).contains(&traitors), "{traitors}");
    }

    #[test]
    fn protected_and_blacklisted_jobs_are_barred() {
        let mut data = crate::jobs::fallback_jobs();
        let base = data.jobs[0].clone();
        let job = |title: &str, protected: bool, blacklisted: bool| {
            let mut j = base.clone();
            j.title = title.into();
            j.antag_protected = protected;
            j.antag_blacklisted = blacklisted;
            j
        };
        data.jobs.push(job("Security Officer", true, false));
        data.jobs.push(job("Captain", true, false));
        data.jobs.push(job("Cargo Gorilla", false, true));
        data.jobs.push(job("AI", false, false));
        data.jobs.push(job("Station Engineer", false, false));
        assert_eq!(blacklisted_jobs(&data, true), [1, 2, 3, 4]);
        // Without `PROTECT_ROLES_FROM_ANTAGONIST` only the blacklisted and mechanical ones.
        assert_eq!(blacklisted_jobs(&data, false), [3, 4]);
    }

    #[test]
    fn forced_rulesets_skip_the_roll() {
        let config = DynamicConfig {
            forced_rulesets: Some(vec![TRAITOR_RULESET, TRAITOR_RULESET]),
            protect_roles: true,
            seed: None,
            dev_traitor: DevTraitor::Off,
        };
        let mut rng = Rng::new(0);
        assert_eq!(
            roll_rulesets(&config, 1, &mut rng),
            [TRAITOR_RULESET, TRAITOR_RULESET]
        );
        let prefs = AntagPreferences {
            antag_roll: false,
            be_special: [ROLE_TRAITOR.to_owned()].into(),
        };
        // The lobby toggle off opts out of every roll.
        assert!(!prefs.wants(ROLE_TRAITOR));
        assert!(
            AntagPreferences {
                antag_roll: true,
                ..prefs
            }
            .wants(ROLE_TRAITOR)
        );
    }
}
