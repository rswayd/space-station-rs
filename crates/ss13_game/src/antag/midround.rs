//! Midround and latejoin antagonists after BandaStation's `SSdynamic` (`dynamic.dm`,
//! `dynamic_ruleset_midround.dm`, `dynamic_ruleset_latejoin.dm`, `config/dynamic.toml`).
//!
//! At round start ([`start`], from [`super::dynamic::prepare_roundstart`]) the round's tier sets
//! how many light midround, heavy midround and latejoin rulesets may spawn (`set_tier`: the same
//! `rand(low, high)` with the high end cut at 25 and 40 players) and when each kind may start
//! (`time_threshold`). Every 5 minutes (`wait`), while the emergency shuttle is idle or recalled
//! and the midround cooldown is over, dynamic tries a light midround, then a heavy one
//! ([`try_spawn_midround`]): `prob(100 - 200 × dead / (alive + dead))`, a ruleset by weight
//! (`get_weight`: tier weight, `min_pop`, repeats lose `repeatable_weight_decrease`, a
//! non-repeatable one or, below the high tier, a second high impact one is out), candidates
//! (living crew, or ghosts, per ruleset), a cooldown of 10-20 minutes. A crew latejoin
//! ([`on_latejoin`]) runs an admin-queued latejoin ruleset first, else rolls one the same way
//! (`+15%` per failed latejoin, a fifth of the chance before light midrounds may start) with the
//! latejoiner as the only candidate.
//!
//! Admins force a midround ruleset ([`force_midround`], `force_run_midround`) or queue a latejoin
//! one ([`Midround::queued_latejoin`]).
//!
//! A ghost ruleset ([`poll_ruleset`], `from_ghosts/collect_candidates`) asks its willing ghosts
//! with a 30-second [`crate::ghost_poll`] poll and is selected from who signed up once it ends;
//! a light one that then fails still tries the heavy one, as tg's `fire()` does after its sleep.
// shortcut: of the rulesets only the midround traitor (sleeper agent), mass traitors, the midround
// nuclear and clown operatives, the latejoin traitor
// (infiltrator), the stowaway changeling, the latejoin revolution (provocateur), the midround
// wizard, the paradox clone (`super::paradox_clone`), the nightmare (`super::nightmare`), the
// morph (`super::morph`), the space dragon (`super::space_dragon`), the revenant
// (`super::revenant`), the slaughter demon (`super::slaughter`), the abductors
// (`super::abductor`), the blob and blob infection (`super::blob`) and the xenomorphs
// (`super::xeno`) spawn anyone;
// the others keep their weights, are logged and fail preparation, as tg's do without candidates.
// There is no admin cancel or reroll window before a midround, no false alarms, no AFK checks,
// no account age or job bans, and `EMERGENCY_PAST_POINT_OF_NO_RETURN` ignores the call's
// restriction point (only docked or later).

use bevy::prelude::*;
use ss13_mapload::spawner::Rng;

use super::dynamic::{AntagPreferences, DynamicConfig, DynamicRng, ROUNDSTART_RULESETS, Tier};
use super::{
    ROLE_OPERATIVE_MIDROUND, ROLE_SLEEPER_AGENT, ROLE_STOWAWAY_CHANGELING,
    ROLE_SYNDICATE_INFILTRATOR,
};
use crate::ghost::Ghost;
use crate::ghost_poll::{self, Candidate, DEFAULT_POLL_SECS, PollSpec};
use crate::health::Stat;
use crate::jobs::{Jobs, PlayerJob};
use crate::server::PlayerMob;

/// `SSdynamic` `wait = 5 MINUTES`.
pub const FIRE_SECS: f64 = 300.0;
/// Every tier's `half_range_pop_threshold` and `full_range_pop_threshold` of the three kinds.
const HALF_RANGE_POP: u32 = 25;
const FULL_RANGE_POP: u32 = 40;
/// Every tier's `execution_cooldown_low` / `_high`, minutes.
const COOLDOWN_MINUTES: (i64, i64) = (10, 20);
/// `failed_latejoins * 15`.
const FAILED_LATEJOIN_BONUS: f32 = 15.0;

/// `LIGHT_MIDROUND`, `HEAVY_MIDROUND`, `LATEJOIN`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Light = 0,
    Heavy = 1,
    Latejoin = 2,
}

/// Per tier (0..=4), per kind (light, heavy, latejoin): `low`, `high`, `time_threshold` minutes.
pub const TIER_RANGES: [[(u32, u32, u32); 3]; 5] = [
    [(0, 0, 30), (0, 0, 60), (0, 0, 0)],
    [(0, 2, 30), (0, 1, 60), (0, 1, 5)],
    [(0, 2, 30), (0, 1, 60), (1, 2, 5)],
    [(1, 2, 30), (1, 2, 60), (1, 3, 5)],
    [(1, 2, 20), (2, 4, 30), (2, 3, 5)],
];

/// Who a ruleset picks from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Candidates {
    /// `from_living`: living crew who are no antagonists.
    Living,
    /// `from_ghosts`: ghosts and observers.
    Ghosts,
    /// The latejoiner.
    Latejoiner,
}

/// `get_antag_cap`: a number, or `ceil(population / denominator) + offset`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cap {
    Fixed(u32),
    Scaled { denominator: u32, offset: u32 },
}

impl Cap {
    pub fn get(self, population: u32) -> u32 {
        match self {
            Cap::Fixed(n) => n,
            Cap::Scaled {
                denominator,
                offset,
            } => population.div_ceil(denominator) + offset,
        }
    }
}

/// A midround or latejoin ruleset's selection values (`config/dynamic.toml` over the code).
#[derive(Clone, Copy, Debug)]
pub struct Ruleset {
    pub config_tag: &'static str,
    pub kind: Kind,
    /// The ruleset type it subtypes (`istype` for the repeat penalty).
    pub parent: Option<&'static str>,
    pub weight: [u32; 4],
    pub min_pop: u32,
    pub repeatable: bool,
    pub repeatable_weight_decrease: u32,
    pub high_impact: bool,
    /// The `pref_flag`, for the implemented ones.
    pub role: Option<&'static str>,
    pub candidates: Candidates,
    /// (`min_antag_cap`, `max_antag_cap || min_antag_cap`).
    pub caps: (Cap, Cap),
    pub blacklisted_roles: &'static [&'static str],
}

pub const MIDROUND_TRAITOR: &str = "Midround Traitor";
pub const MASS_TRAITORS: &str = "Mass Traitors";
pub const MIDROUND_NUKEOPS: &str = "Midround Nukeops";
pub const MIDROUND_CLOWNOPS: &str = "Midround Clownops";
pub const MIDROUND_WIZARD: &str = "Midround Wizard";
pub const LATEJOIN_TRAITOR: &str = "Latejoin Traitor";
pub const LATEJOIN_CHANGELING: &str = "Latejoin Changeling";
pub const LATEJOIN_REVOLUTION: &str = "Latejoin Revolution";
pub const PARADOX_CLONE: &str = "Paradox Clone";
pub const NIGHTMARE: &str = "Nightmare";
pub const MORPH: &str = "Morph";
pub const SPACE_DRAGON: &str = "Space Dragon";
pub const REVENANT: &str = "Revenant";
pub const SLAUGHTER_DEMON: &str = "Slaughter Demon";
pub const ABDUCTORS: &str = "Abductors";
pub const BLOB: &str = "Blob";
pub const BLOB_INFECTION: &str = "Blob Infection";
pub const XENOMORPH: &str = "Xenomorph";

const HOP: &[&str] = &["Head of Personnel"];
const ONE: (Cap, Cap) = (Cap::Fixed(1), Cap::Fixed(1));

const fn other(tag: &'static str, kind: Kind, weight: [u32; 4], min_pop: u32) -> Ruleset {
    Ruleset {
        config_tag: tag,
        kind,
        parent: None,
        weight,
        min_pop,
        repeatable: true,
        repeatable_weight_decrease: 2,
        high_impact: false,
        role: None,
        candidates: Candidates::Ghosts,
        caps: ONE,
        blacklisted_roles: &[],
    }
}

const fn flat(w: u32) -> [u32; 4] {
    [w; 4]
}

const fn non_repeatable(r: Ruleset) -> Ruleset {
    Ruleset {
        repeatable: false,
        ..r
    }
}

const fn decrease(r: Ruleset, by: u32) -> Ruleset {
    Ruleset {
        repeatable_weight_decrease: by,
        ..r
    }
}

const fn high_impact(r: Ruleset) -> Ruleset {
    Ruleset {
        high_impact: true,
        ..r
    }
}

const fn child(r: Ruleset, parent: &'static str) -> Ruleset {
    Ruleset {
        parent: Some(parent),
        ..r
    }
}

const NUKIE_CAP: Cap = Cap::Scaled {
    denominator: 18,
    offset: 1,
};

/// Every midround and latejoin ruleset of BandaStation's code, with its config values.
pub const RULESETS: [Ruleset; 30] = {
    use Kind::{Heavy, Latejoin, Light};
    [
        // Light midrounds.
        other("Light Pirates", Light, flat(3), 15),
        other("Midround Blood Worm", Light, flat(2), 20),
        Ruleset {
            role: Some(super::ROLE_NIGHTMARE),
            ..other(NIGHTMARE, Light, flat(5), 15)
        },
        Ruleset {
            role: Some(super::abductor::ROLE_ABDUCTOR),
            caps: (Cap::Fixed(2), Cap::Fixed(2)),
            ..decrease(other(ABDUCTORS, Light, flat(5), 20), 3)
        },
        Ruleset {
            role: Some(super::ROLE_REVENANT),
            ..non_repeatable(other(REVENANT, Light, flat(5), 10))
        },
        other("Midround Changeling", Light, flat(5), 15),
        Ruleset {
            role: Some(super::paradox_clone::ROLE_PARADOX_CLONE),
            ..other(PARADOX_CLONE, Light, flat(5), 10)
        },
        other("Voidwalker", Light, flat(5), 30),
        non_repeatable(other("Fugitives", Light, flat(3), 20)),
        Ruleset {
            role: Some(super::ROLE_MORPH),
            ..other(MORPH, Light, flat(0), 0)
        },
        Ruleset {
            role: Some(ROLE_SLEEPER_AGENT),
            candidates: Candidates::Living,
            blacklisted_roles: HOP,
            ..other(MIDROUND_TRAITOR, Light, flat(10), 3)
        },
        other("Midround Obsessed", Light, [5, 5, 3, 1], 5),
        // Heavy midrounds.
        other("Spiders", Heavy, [0, 0, 1, 2], 30),
        child(other("Heavy Pirates", Heavy, flat(3), 25), "Light Pirates"),
        Ruleset {
            role: Some(super::wizard::ROLE_WIZARD_MIDROUND),
            ..high_impact(other(MIDROUND_WIZARD, Heavy, [0, 0, 1, 2], 30))
        },
        Ruleset {
            role: Some(ROLE_OPERATIVE_MIDROUND),
            caps: (NUKIE_CAP, NUKIE_CAP),
            ..high_impact(non_repeatable(other(
                MIDROUND_NUKEOPS,
                Heavy,
                [0, 1, 3, 3],
                30,
            )))
        },
        Ruleset {
            role: Some(super::nukeop::ROLE_CLOWN_OPERATIVE_MIDROUND),
            caps: (NUKIE_CAP, NUKIE_CAP),
            ..child(
                high_impact(non_repeatable(other(MIDROUND_CLOWNOPS, Heavy, flat(0), 30))),
                MIDROUND_NUKEOPS,
            )
        },
        Ruleset {
            role: Some(super::ROLE_BLOB),
            ..decrease(other(BLOB, Heavy, [0, 1, 3, 3], 30), 3)
        },
        Ruleset {
            role: Some(super::ROLE_ALIEN),
            ..decrease(other(XENOMORPH, Heavy, [0, 1, 5, 5], 30), 3)
        },
        Ruleset {
            role: Some(super::ROLE_SPACE_DRAGON),
            ..decrease(other(SPACE_DRAGON, Heavy, [0, 3, 5, 5], 30), 3)
        },
        non_repeatable(other("Space Ninja", Heavy, [0, 0, 1, 2], 30)),
        child(
            decrease(other("Mass Changelings", Heavy, [0, 3, 4, 5], 25), 4),
            "Midround Changeling",
        ),
        Ruleset {
            role: Some(super::ROLE_SLAUGHTER_DEMON),
            ..other(SLAUGHTER_DEMON, Heavy, flat(0), 20)
        },
        Ruleset {
            role: Some(ROLE_SLEEPER_AGENT),
            candidates: Candidates::Living,
            caps: (Cap::Fixed(2), Cap::Fixed(4)),
            ..child(
                decrease(other(MASS_TRAITORS, Heavy, [0, 3, 8, 10], 15), 7),
                MIDROUND_TRAITOR,
            )
        },
        high_impact(non_repeatable(other(
            "Midround Malfunctioning AI",
            Heavy,
            [0, 1, 3, 3],
            30,
        ))),
        Ruleset {
            role: Some(super::ROLE_BLOB_INFECTION),
            candidates: Candidates::Living,
            ..decrease(other(BLOB_INFECTION, Heavy, [0, 1, 3, 3], 30), 3)
        },
        // Latejoins.
        Ruleset {
            role: Some(ROLE_SYNDICATE_INFILTRATOR),
            candidates: Candidates::Latejoiner,
            blacklisted_roles: HOP,
            ..other(LATEJOIN_TRAITOR, Latejoin, flat(10), 3)
        },
        Ruleset {
            role: Some(ROLE_STOWAWAY_CHANGELING),
            candidates: Candidates::Latejoiner,
            blacklisted_roles: HOP,
            ..other(LATEJOIN_CHANGELING, Latejoin, flat(3), 15)
        },
        Ruleset {
            role: Some(super::revolution::ROLE_PROVOCATEUR),
            candidates: Candidates::Latejoiner,
            blacklisted_roles: &super::revolution::server::HEAD_JOBS,
            ..high_impact(non_repeatable(other(
                LATEJOIN_REVOLUTION,
                Latejoin,
                flat(1),
                30,
            )))
        },
        // Placeholder for the config's `Latejoin Heretic`: no such ruleset type in the code.
        other("Latejoin Heretic", Latejoin, flat(0), 30),
    ]
};

pub fn ruleset(tag: &str) -> Option<&'static Ruleset> {
    RULESETS.iter().find(|r| r.config_tag == tag)
}

fn high_impact_tag(tag: &str) -> bool {
    ruleset(tag).map(|r| r.high_impact).unwrap_or_else(|| {
        ROUNDSTART_RULESETS
            .iter()
            .any(|r| r.config_tag == tag && r.high_impact)
    })
}

/// `istype(other, type)`.
fn is_type(other: &str, of: &str) -> bool {
    other == of || ruleset(other).and_then(|r| r.parent) == Some(of)
}

/// `get_weight(population, tier)` against the executed rulesets (`can_be_selected` aside).
pub fn get_weight(r: &Ruleset, population: u32, tier: u8, executed: &[&str]) -> u32 {
    if tier == 0 || r.min_pop > population {
        return 0;
    }
    let mut weight = r.weight[usize::from(tier - 1)] as i64;
    for other in executed {
        if tier != 4 && r.high_impact && high_impact_tag(other) {
            return 0;
        }
        if !is_type(other, r.config_tag) {
            continue;
        }
        if !r.repeatable {
            return 0;
        }
        weight -= i64::from(r.repeatable_weight_decrease);
    }
    weight.max(0) as u32
}

/// `set_tier` for one kind: `rand(low, high)`, the high end quartered at or below 25 players and
/// halved at or below 40.
pub fn kind_count(tier: u8, kind: Kind, population: u32, rng: &mut Rng) -> u32 {
    let (low, mut high, _) = TIER_RANGES[usize::from(tier)][kind as usize];
    if population <= HALF_RANGE_POP {
        high = low.max(high.div_ceil(4));
    } else if population <= FULL_RANGE_POP {
        high = low.max(high.div_ceil(2));
    }
    rng.range(i64::from(low), i64::from(high)) as u32
}

/// `get_midround_chance` / `get_latejoin_chance` before the latejoin's own terms: `100 - 200 ×
/// dead / (alive + dead)` (tg's `num_antags < 0` bonus can never apply).
pub fn base_chance(alive: u32, dead: u32) -> f32 {
    if alive + dead == 0 {
        return 0.0;
    }
    100.0 - 200.0 * dead as f32 / (alive + dead) as f32
}

/// `get_latejoin_chance`.
pub fn latejoin_chance(alive: u32, dead: u32, failed: u32, lights_started: bool) -> f32 {
    if alive + dead == 0 {
        return 0.0;
    }
    let chance = base_chance(alive, dead) + failed as f32 * FAILED_LATEJOIN_BONUS;
    if lights_started { chance } else { chance * 0.2 }
}

/// SSdynamic's midround and latejoin state.
#[derive(Resource, Debug)]
pub struct Midround {
    /// Set at round start; nothing fires before.
    pub started: bool,
    pub tier: u8,
    /// `rulesets_to_spawn` of light, heavy, latejoin.
    pub to_spawn: [u32; 3],
    /// `light_ruleset_start`, `heavy_ruleset_start`, `latejoin_ruleset_start`: game time.
    pub start_at: [f64; 3],
    /// `midround_cooldown`, `latejoin_cooldown`: game time they end.
    pub midround_cooldown: f64,
    pub latejoin_cooldown: f64,
    pub failed_latejoins: u32,
    /// The next `fire()`.
    pub next_fire: f64,
    /// `executed_rulesets`, config tags, roundstart ones included.
    pub executed: Vec<&'static str>,
    /// `queued_rulesets` of latejoin rulesets (admins).
    pub queued_latejoin: Vec<&'static str>,
    /// `admin_forcing_next_latejoin`.
    pub force_next_latejoin: bool,
}

impl Default for Midround {
    fn default() -> Self {
        Self {
            started: false,
            tier: 0,
            to_spawn: [0; 3],
            start_at: [0.0; 3],
            midround_cooldown: 0.0,
            latejoin_cooldown: 0.0,
            failed_latejoins: 0,
            next_fire: 0.0,
            executed: Vec::new(),
            queued_latejoin: Vec::new(),
            force_next_latejoin: false,
        }
    }
}

pub struct MidroundPlugin;

impl Plugin for MidroundPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Midround>()
            .add_systems(Update, fire.run_if(|m: Res<Midround>| m.started));
    }
}

use crate::tools::now;

fn with_rng<R>(world: &mut World, f: impl FnOnce(&mut Rng) -> R) -> R {
    if !world.contains_resource::<DynamicRng>() {
        world.insert_resource(DynamicRng(Rng::new(0)));
    }
    f(&mut world.resource_mut::<DynamicRng>().0)
}

/// After the roundstart rulesets: the counts of the tier and the start times.
pub fn start(world: &mut World, tier: &Tier, population: u32, executed: Vec<&'static str>) {
    let t = now(world);
    let to_spawn = with_rng(world, |rng| {
        [Kind::Light, Kind::Heavy, Kind::Latejoin]
            .map(|k| kind_count(tier.tier, k, population, rng))
    });
    let ranges = TIER_RANGES[usize::from(tier.tier)];
    let mut state = world.resource_mut::<Midround>();
    state.started = true;
    state.tier = tier.tier;
    state.to_spawn = to_spawn;
    state.start_at = ranges.map(|(_, _, minutes)| t + f64::from(minutes) * 60.0);
    state.next_fire = t + FIRE_SECS;
    state.executed.extend(executed);
    info!(
        "dynamic: light/heavy midround and latejoin rulesets to spawn: {to_spawn:?}, tier {}",
        tier.tier
    );
}

/// (`get_active_player_count(afk_check)`, `length(GLOB.dead_player_list)`): player bodies and
/// the ghosts of played ones; the dead among them.
pub fn player_counts(world: &mut World) -> (u32, u32) {
    let mut q = world.query_filtered::<(Option<&Ghost>, Option<&Stat>), With<PlayerMob>>();
    let (mut active, mut dead) = (0, 0);
    for (ghost, stat) in q.iter(world) {
        match ghost {
            Some(g) if g.body.is_none() => {}
            Some(_) => {
                active += 1;
                dead += 1;
            }
            None => {
                active += 1;
                dead += u32::from(stat == Some(&Stat::Dead));
            }
        }
    }
    (active, dead)
}

/// `EMERGENCY_IDLE_OR_RECALLED`.
fn shuttle_idle_or_recalled(world: &World) -> bool {
    use crate::shuttle::{EmergencyShuttle, ShuttleMode};
    world
        .get_resource::<EmergencyShuttle>()
        .is_some_and(|s| matches!(s.state.mode, ShuttleMode::Idle | ShuttleMode::Recall))
}

/// `EMERGENCY_AT_LEAST_DOCKED` (see the shortcut above).
pub fn shuttle_past_no_return(world: &World) -> bool {
    use crate::shuttle::{EmergencyShuttle, ShuttleMode};
    world.get_resource::<EmergencyShuttle>().is_some_and(|s| {
        matches!(
            s.state.mode,
            ShuttleMode::Docked | ShuttleMode::Escape | ShuttleMode::Endgame
        )
    })
}

/// `SSdynamic/fire`.
fn fire(world: &mut World) {
    let t = now(world);
    let state = world.resource::<Midround>();
    if t < state.next_fire {
        return;
    }
    world.resource_mut::<Midround>().next_fire = t + FIRE_SECS;
    let state = world.resource::<Midround>();
    if t < state.midround_cooldown || !shuttle_idle_or_recalled(world) {
        return;
    }
    if t >= state.start_at[0] && try_spawn_midround(world, Kind::Light) {
        return;
    }
    if t >= world.resource::<Midround>().start_at[1] {
        try_spawn_midround(world, Kind::Heavy);
    }
}

/// `get_ruleset_cooldown`: 10-20 minutes.
fn cooldown(world: &mut World) -> f64 {
    with_rng(world, |rng| {
        rng.range(COOLDOWN_MINUTES.0, COOLDOWN_MINUTES.1) as f64 * 60.0
    })
}

/// The weighted pool of a kind (`get_midround_rulesets` / `get_latejoin_rulesets`).
fn pool(world: &mut World, kind: Kind, population: u32) -> Vec<(&'static Ruleset, u32)> {
    let state = world.resource::<Midround>();
    let (tier, executed) = (state.tier, state.executed.clone());
    RULESETS
        .iter()
        .filter(|r| r.kind == kind)
        .filter(|r| can_be_selected(world, r))
        .map(|r| (r, get_weight(r, population, tier, &executed)))
        .collect()
}

/// `can_be_selected` of the rulesets that have one here.
fn can_be_selected(world: &mut World, r: &Ruleset) -> bool {
    match r.config_tag {
        // No revolution yet and at least 3 living heads of staff.
        LATEJOIN_REVOLUTION => {
            world
                .get_resource::<super::revolution::server::RevolutionHandler>()
                .is_none()
                && super::revolution::server::active_heads(world)
                    >= super::revolution::server::LATEJOIN_HEADS_NECESSARY
        }
        // Someone to clone and a maintenance spawn.
        PARADOX_CLONE => super::paradox_clone::can_spawn(world),
        // A dark maintenance spawn; any maintenance spawn.
        NIGHTMARE => super::nightmare::can_spawn(world),
        MORPH => super::morph::can_spawn(world),
        // A carp spawn in space.
        SPACE_DRAGON => super::space_dragon::can_spawn(world),
        // More than 10 corpses on the station.
        REVENANT => super::revenant::can_spawn(world),
        SLAUGHTER_DEMON => super::slaughter::can_spawn(world),
        // Fewer than 4 teams, and a ship to put the next one on.
        ABDUCTORS => super::abductor::server::can_spawn(world),
        BLOB => super::blob::can_spawn(world),
        // A vent to crawl out of.
        XENOMORPH => super::xeno::can_spawn(world),
        _ => true,
    }
}

fn pick(pool: &[(&'static Ruleset, u32)], rng: &mut Rng) -> Option<&'static Ruleset> {
    let weights: Vec<u32> = pool.iter().map(|(_, w)| *w).collect();
    super::dynamic::pick_weight(&weights, rng).map(|i| pool[i].0)
}

/// `try_spawn_midround(range)`.
pub fn try_spawn_midround(world: &mut World, kind: Kind) -> bool {
    if world.resource::<Midround>().to_spawn[kind as usize] == 0 {
        return false;
    }
    let (alive, dead) = player_counts(world);
    let chance = base_chance(alive, dead);
    if !with_rng(world, |rng| rng.prob(chance)) {
        info!("dynamic: midround ({kind:?}): ruleset chance failed ({chance}%)");
        return false;
    }
    let pool = pool(world, kind, alive);
    let Some(picked) = with_rng(world, |rng| pick(&pool, rng)) else {
        info!("dynamic: midround ({kind:?}): no rulesets to pick from");
        return false;
    };
    if picked.candidates == Candidates::Ghosts {
        let polling = poll_ruleset(world, picked, alive, AfterPoll::Midround(kind));
        if !polling {
            info!(
                "dynamic: midround ({kind:?}): {} preparation failed",
                picked.config_tag
            );
        }
        return polling;
    }
    if execute(world, picked, alive, None).is_empty() {
        info!(
            "dynamic: midround ({kind:?}): {} preparation failed",
            picked.config_tag
        );
        return false;
    }
    midround_spawned(world, kind);
    true
}

/// A midround of `kind` ran: one fewer to spawn and the cooldown.
fn midround_spawned(world: &mut World, kind: Kind) {
    let cd = cooldown(world);
    let t = now(world);
    let mut state = world.resource_mut::<Midround>();
    state.to_spawn[kind as usize] = state.to_spawn[kind as usize].saturating_sub(1);
    state.midround_cooldown = t + cd;
}

/// `force_run_midround`: the admin's ruleset, now. The selected bodies, or why nobody; a ghost
/// ruleset selects when its poll ends, and `Ok` with no bodies says the poll is running.
pub fn force_midround(world: &mut World, tag: &str) -> Result<Vec<Entity>, String> {
    let r = ruleset(tag)
        .filter(|r| r.kind != Kind::Latejoin)
        .ok_or_else(|| format!("Нет мидраунд-рулсета {tag}."))?;
    if r.role.is_none() {
        return Err(format!("{tag} здесь не реализован."));
    }
    let (alive, _) = player_counts(world);
    if r.candidates == Candidates::Ghosts {
        return if poll_ruleset(world, r, alive, AfterPoll::Forced) {
            Ok(Vec::new())
        } else {
            Err(format!("{tag}: подготовка не удалась (нет кандидатов)."))
        };
    }
    let picked = execute(world, r, alive, None);
    if picked.is_empty() {
        return Err(format!("{tag}: подготовка не удалась (нет кандидатов)."));
    }
    Ok(picked)
}

/// `on_latejoin`: a queued latejoin ruleset first, else a roll. `latejoiner` is a crew body.
pub fn on_latejoin(world: &mut World, latejoiner: Entity) {
    if !world.resource::<Midround>().started
        || shuttle_past_no_return(world)
        || world
            .get::<crate::inventory::Inventory>(latejoiner)
            .is_none()
    {
        return;
    }
    let (alive, dead) = player_counts(world);
    let queued = world.resource::<Midround>().queued_latejoin.clone();
    for (i, tag) in queued.into_iter().enumerate() {
        let Some(r) = ruleset(tag) else {
            continue;
        };
        if !execute(world, r, alive, Some(latejoiner)).is_empty() {
            world.resource_mut::<Midround>().queued_latejoin.remove(i);
            info!("dynamic: latejoin (forced): {latejoiner} selected for {tag}");
            return;
        }
        info!("dynamic: latejoin (forced): queued {tag} failed to prepare, it stays queued");
    }
    let t = now(world);
    let state = world.resource::<Midround>();
    if t < state.start_at[2] || t < state.latejoin_cooldown || state.to_spawn[2] == 0 {
        return;
    }
    let chance = if state.force_next_latejoin {
        100.0
    } else {
        latejoin_chance(alive, dead, state.failed_latejoins, t >= state.start_at[0])
    };
    if !with_rng(world, |rng| rng.prob(chance)) {
        info!("dynamic: latejoin: ruleset chance failed ({chance}%)");
        return;
    }
    let pool = pool(world, Kind::Latejoin, alive);
    let Some(picked) = with_rng(world, |rng| pick(&pool, rng)) else {
        info!("dynamic: latejoin: no rulesets to pick from");
        return;
    };
    if execute(world, picked, alive, Some(latejoiner)).is_empty() {
        info!(
            "dynamic: latejoin: {} failed for {latejoiner}, latejoin chance has increased",
            picked.config_tag
        );
        world.resource_mut::<Midround>().failed_latejoins += 1;
        return;
    }
    let cd = cooldown(world);
    let mut state = world.resource_mut::<Midround>();
    state.failed_latejoins = 0;
    state.force_next_latejoin = false;
    state.to_spawn[2] -= 1;
    state.latejoin_cooldown = t + cd;
}

/// `get_blacklisted_roles`: the config's, the protected and blacklisted jobs, the AI and cyborg.
fn blacklisted(world: &World, r: &Ruleset, title: &str) -> bool {
    let protect = world
        .get_resource::<DynamicConfig>()
        .is_none_or(|c| c.protect_roles);
    r.blacklisted_roles.contains(&title)
        || title == "AI"
        || title == "Cyborg"
        || world
            .get_resource::<Jobs>()
            .and_then(|jobs| jobs.data.jobs.iter().find(|j| j.title == title))
            .is_some_and(|j| j.antag_blacklisted || (protect && j.antag_protected))
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

/// `trim_candidates` + `is_valid_candidate`: the client opted in (lobby toggle and, when the
/// ruleset has a `pref_flag`, `be_special`).
fn wants(world: &World, mob: Entity, r: &Ruleset) -> bool {
    let Some(prefs) = world
        .get::<PlayerMob>(mob)
        .and_then(|p| world.get::<AntagPreferences>(p.client))
    else {
        return false;
    };
    match r
        .role
        .filter(|_| !matches!(r.config_tag, MORPH | SLAUGHTER_DEMON))
    {
        Some(role) => prefs.wants(role),
        None => prefs.antag_roll,
    }
}

/// `collect_candidates` + `trim_candidates` (bodies; ghosts for `from_ghosts`).
fn candidates(world: &mut World, r: &Ruleset, latejoiner: Option<Entity>) -> Vec<Entity> {
    if r.role.is_none() {
        return Vec::new();
    }
    let mut pool: Vec<Entity> = match r.candidates {
        Candidates::Latejoiner => latejoiner.into_iter().collect(),
        Candidates::Living => {
            let mut q =
                world.query_filtered::<(Entity, &Stat), (With<PlayerMob>, With<PlayerJob>)>();
            q.iter(world)
                .filter(|(_, s)| **s != Stat::Dead)
                .map(|(e, _)| e)
                .filter(|&e| world.get::<super::AntagHudData>(e).is_none())
                .collect()
        }
        Candidates::Ghosts => {
            let mut q = world.query_filtered::<Entity, (With<PlayerMob>, With<Ghost>)>();
            q.iter(world).collect()
        }
    };
    pool.sort();
    pool.retain(|&m| {
        wants(world, m, r)
            && (r.candidates == Candidates::Ghosts
                || job_title(world, m).is_some_and(|t| !blacklisted(world, r, &t)))
    });
    pool
}

/// `prepare_execution` + `execute`: the selected bodies (empty: preparation failed).
fn execute(
    world: &mut World,
    r: &'static Ruleset,
    population: u32,
    latejoiner: Option<Entity>,
) -> Vec<Entity> {
    let pool = candidates(world, r, latejoiner);
    let caps = (r.caps.1.get(population), r.caps.0.get(population));
    select_and_run(world, r, &pool, caps, None)
}

/// What follows a ghost ruleset's poll (tg's sleeping callers).
#[derive(Clone, Copy, Debug)]
enum AfterPoll {
    /// `try_spawn_midround(kind)`.
    Midround(Kind),
    /// `force_run_midround`.
    Forced,
}

/// The poll's title (`candidate_role || pref_flag`).
fn poll_role_name(r: &Ruleset) -> &'static str {
    match r.config_tag {
        MIDROUND_NUKEOPS | MIDROUND_CLOWNOPS => "Оперативник",
        MIDROUND_WIZARD => "Волшебник",
        NIGHTMARE => "Кошмар",
        MORPH => "Морф",
        SPACE_DRAGON => "Космический дракон",
        REVENANT => "Ревенант",
        SLAUGHTER_DEMON => "Демон резни",
        ABDUCTORS => "Абдуктор",
        BLOB => "Блоб",
        XENOMORPH => "Ксеноморф",
        tag => tag,
    }
}

/// `from_ghosts/collect_candidates`: polls the ruleset's willing ghosts; [`ghost_ruleset_polled`]
/// goes on when the poll ends. `false`: nobody to ask, preparation failed.
fn poll_ruleset(world: &mut World, r: &'static Ruleset, population: u32, after: AfterPoll) -> bool {
    let group = candidates(world, r, None);
    if group.is_empty() {
        return false;
    }
    let mut caps = (r.caps.1.get(population), r.caps.0.get(population));
    let mut role_name = poll_role_name(r).to_owned();
    // The original is picked first, for the poll to name it (`clone_target_ref`).
    let mut original = None;
    if r.config_tag == PARADOX_CLONE {
        // The paradox clone's `max_antag_cap += prob(bonus_clone_chance)`.
        if with_rng(world, |rng| {
            rng.prob(super::paradox_clone::BONUS_CLONE_CHANCE)
        }) {
            caps.0 += 1;
        }
        let Some(target) = super::paradox_clone::pick_original(world) else {
            return false;
        };
        role_name = format!("клон {}", super::objectives::target_text(world, target));
        original = Some(target);
    }
    // `xenomorph/New`: `max_antag_cap += prob(50)`.
    if r.config_tag == XENOMORPH && with_rng(world, |rng| rng.prob(50.0)) {
        caps.0 += 1;
    }
    let spec = PollSpec {
        question: format!("Ищем добровольцев на роль: {role_name}."),
        role_name,
        secs: DEFAULT_POLL_SECS,
        ..default()
    };
    ghost_poll::poll_candidates(world, spec, group, move |world, signed_up| {
        ghost_ruleset_polled(world, r, caps, original, after, signed_up);
    });
    true
}

/// tg after the sleep of `collect_candidates`: `prepare_execution` (`can_be_selected` again and
/// who is still valid; the poll has trimmed who left), `execute` and the caller's bookkeeping.
fn ghost_ruleset_polled(
    world: &mut World,
    r: &'static Ruleset,
    caps: (u32, u32),
    original: Option<Entity>,
    after: AfterPoll,
    signed_up: Vec<Candidate>,
) {
    let mut chosen = Vec::new();
    if !signed_up.is_empty() && can_be_selected(world, r) {
        let pool: Vec<Entity> = signed_up.iter().map(|c| c.mob).collect();
        let pool: Vec<Entity> = pool.into_iter().filter(|&m| wants(world, m, r)).collect();
        chosen = select_and_run(world, r, &pool, caps, original);
    }
    if chosen.is_empty() {
        info!(
            "dynamic: {} preparation failed: not enough candidates after the poll",
            r.config_tag
        );
        if let AfterPoll::Midround(Kind::Light) = after
            && now(world) >= world.resource::<Midround>().start_at[1]
        {
            try_spawn_midround(world, Kind::Heavy);
        }
        return;
    }
    if let AfterPoll::Midround(kind) = after {
        midround_spawned(world, kind);
    }
}

/// `select_candidates` from `pool` under `caps`, then `execute`: the selected bodies (empty:
/// too few candidates or the role could not be given).
fn select_and_run(
    world: &mut World,
    r: &'static Ruleset,
    pool: &[Entity],
    caps: (u32, u32),
    original: Option<Entity>,
) -> Vec<Entity> {
    let picked = with_rng(world, |rng| {
        let mut indices: Vec<usize> = (0..pool.len()).collect();
        super::dynamic::select_up_to(&mut indices, |_| true, caps, rng)
    });
    let Some(picked) = picked else {
        return Vec::new();
    };
    let chosen: Vec<Entity> = picked.into_iter().map(|i| pool[i]).collect();
    match r.config_tag {
        MIDROUND_NUKEOPS => {
            let clients: Vec<Entity> = chosen
                .iter()
                .filter_map(|&g| world.get::<PlayerMob>(g).map(|p| p.client))
                .collect();
            super::nukeop::server::spawn_operatives(world, &clients);
        }
        MIDROUND_CLOWNOPS => {
            let clients: Vec<Entity> = chosen
                .iter()
                .filter_map(|&g| world.get::<PlayerMob>(g).map(|p| p.client))
                .collect();
            super::nukeop::server::spawn_operatives_as(world, &clients, true);
        }
        MIDROUND_WIZARD => {
            let clients: Vec<Entity> = chosen
                .iter()
                .filter_map(|&g| world.get::<PlayerMob>(g).map(|p| p.client))
                .collect();
            super::wizard::server::spawn_wizards(world, &clients);
        }
        PARADOX_CLONE => {
            let clients: Vec<Entity> = chosen
                .iter()
                .filter_map(|&g| world.get::<PlayerMob>(g).map(|p| p.client))
                .collect();
            if super::paradox_clone::spawn_clones(world, &clients, original).is_empty() {
                return Vec::new();
            }
        }
        NIGHTMARE => {
            let clients: Vec<Entity> = chosen
                .iter()
                .filter_map(|&g| world.get::<PlayerMob>(g).map(|p| p.client))
                .collect();
            if super::nightmare::spawn_nightmares(world, &clients).is_empty() {
                return Vec::new();
            }
        }
        MORPH => {
            let ghosts: Vec<(Entity, Entity)> = chosen
                .iter()
                .filter_map(|&g| world.get::<PlayerMob>(g).map(|p| (p.client, g)))
                .collect();
            if super::morph::spawn_morphs(world, &ghosts).is_empty() {
                return Vec::new();
            }
        }
        BLOB => {
            let ghosts: Vec<(Entity, Entity)> = chosen
                .iter()
                .filter_map(|&g| world.get::<PlayerMob>(g).map(|p| (p.client, g)))
                .collect();
            if super::blob::spawn_blobs(world, &ghosts).is_empty() {
                return Vec::new();
            }
        }
        XENOMORPH => {
            let ghosts: Vec<(Entity, Entity)> = chosen
                .iter()
                .filter_map(|&g| world.get::<PlayerMob>(g).map(|p| (p.client, g)))
                .collect();
            if super::xeno::spawn_xenos(world, &ghosts).is_empty() {
                return Vec::new();
            }
        }
        BLOB_INFECTION => {
            for &m in &chosen {
                super::blob::make_host(world, m);
            }
        }
        SPACE_DRAGON => {
            let ghosts: Vec<(Entity, Entity)> = chosen
                .iter()
                .filter_map(|&g| world.get::<PlayerMob>(g).map(|p| (p.client, g)))
                .collect();
            if super::space_dragon::spawn_dragons(world, &ghosts).is_empty() {
                return Vec::new();
            }
        }
        REVENANT => {
            let ghosts: Vec<(Entity, Entity)> = chosen
                .iter()
                .filter_map(|&g| world.get::<PlayerMob>(g).map(|p| (p.client, g)))
                .collect();
            if super::revenant::spawn_revenants(world, &ghosts).is_empty() {
                return Vec::new();
            }
        }
        SLAUGHTER_DEMON => {
            let ghosts: Vec<(Entity, Entity)> = chosen
                .iter()
                .filter_map(|&g| world.get::<PlayerMob>(g).map(|p| (p.client, g)))
                .collect();
            if super::slaughter::spawn_demons(world, &ghosts).is_empty() {
                return Vec::new();
            }
        }
        ABDUCTORS => {
            let clients: Vec<Entity> = chosen
                .iter()
                .filter_map(|&g| world.get::<PlayerMob>(g).map(|p| p.client))
                .collect();
            if super::abductor::server::spawn_abductors(world, &clients).is_empty() {
                return Vec::new();
            }
        }
        LATEJOIN_CHANGELING => {
            for &m in &chosen {
                super::changeling::server::add_changeling(world, m);
            }
        }
        LATEJOIN_REVOLUTION => {
            for &m in &chosen {
                super::revolution::server::make_dormant(world, m, true);
            }
        }
        _ => {
            for &m in &chosen {
                super::traitor::add_traitor(world, m);
            }
        }
    }
    info!("dynamic: {} executed for {chosen:?}", r.config_tag);
    world.resource_mut::<Midround>().executed.push(r.config_tag);
    chosen
}

/// Lines for the admins' dynamic summary.
pub fn report_lines(world: &World) -> Vec<String> {
    let Some(state) = world.get_resource::<Midround>() else {
        return Vec::new();
    };
    if !state.started {
        return vec!["Мидраунд: раунд не начат".into()];
    }
    let t = now(world);
    let left = |at: f64| {
        let secs = (at - t).max(0.0) as u64;
        format!("{:02}:{:02}", secs / 60, secs % 60)
    };
    let mut lines = vec![
        format!("Тир: {}", state.tier),
        format!(
            "Осталось рулсетов: лёгких {}, тяжёлых {}, латджойн {}",
            state.to_spawn[0], state.to_spawn[1], state.to_spawn[2]
        ),
        format!(
            "Старт: лёгкие через {}, тяжёлые через {}, латджойн через {}",
            left(state.start_at[0]),
            left(state.start_at[1]),
            left(state.start_at[2])
        ),
        format!(
            "Следующий бросок мидраунда через {}, кулдаун мидраунда {}, латджойна {}",
            left(state.next_fire),
            left(state.midround_cooldown),
            left(state.latejoin_cooldown)
        ),
        format!("Неудачных латджойнов: {}", state.failed_latejoins),
        format!("Исполнены: {}", state.executed.join(", ")),
    ];
    if !state.queued_latejoin.is_empty() {
        lines.push(format!(
            "В очереди латджойна: {}",
            state.queued_latejoin.join(", ")
        ));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::antag::dynamic::AntagPreferences;
    use ss13_net::chat::server::SendChat;
    use ss13_tgui::prompt::Prompt;
    use ss13_tgui::server::OpenUi;

    fn poll_world() -> (World, Entity, Entity) {
        let mut world = World::new();
        world.init_resource::<Messages<OpenUi>>();
        world.init_resource::<Messages<SendChat>>();
        world.init_resource::<Midround>();
        world.init_resource::<ghost_poll::Polls>();
        world.insert_resource(Time::<()>::default());
        let client = world
            .spawn(AntagPreferences {
                antag_roll: true,
                be_special: [super::super::ROLE_NIGHTMARE.to_owned()].into(),
            })
            .id();
        let ghost = world
            .spawn((Ghost { body: None }, PlayerMob { client }))
            .id();
        (world, client, ghost)
    }

    #[test]
    fn a_ghost_ruleset_polls_and_a_timeout_fails_it_cleanly() {
        let (mut world, _, ghost) = poll_world();
        let r = ruleset(NIGHTMARE).unwrap();
        assert!(poll_ruleset(&mut world, r, 1, AfterPoll::Forced));
        assert_eq!(world.query::<&Prompt>().iter(&world).count(), 1);
        // Nobody signed up: no role, the ghost stays, no poll left over.
        ghost_poll::finish_all(&mut world);
        assert!(world.resource::<Midround>().executed.is_empty());
        assert!(world.get::<Ghost>(ghost).is_some());
        assert_eq!(world.query::<&Prompt>().iter(&world).count(), 0);
    }

    #[test]
    fn nobody_willing_fails_the_ghost_ruleset_at_once() {
        let (mut world, client, _) = poll_world();
        world.entity_mut(client).insert(AntagPreferences {
            antag_roll: true,
            be_special: Default::default(),
        });
        let r = ruleset(NIGHTMARE).unwrap();
        assert!(!poll_ruleset(&mut world, r, 1, AfterPoll::Forced));
        // The morph has no `pref_flag`: the lobby toggle is enough.
        assert!(poll_ruleset(
            &mut world,
            ruleset(MORPH).unwrap(),
            1,
            AfterPoll::Forced
        ));
    }

    #[test]
    fn a_failed_poll_books_nothing() {
        let (mut world, _, _) = poll_world();
        let r = ruleset(NIGHTMARE).unwrap();
        let mut state = world.resource_mut::<Midround>();
        state.to_spawn = [1, 1, 0];
        // Not time for the heavy midround the failed light one would go on to.
        state.start_at = [0.0, f64::INFINITY, 0.0];
        ghost_ruleset_polled(
            &mut world,
            r,
            (1, 1),
            None,
            AfterPoll::Midround(Kind::Light),
            Vec::new(),
        );
        let state = world.resource::<Midround>();
        assert_eq!(state.to_spawn, [1, 1, 0]);
        assert_eq!(state.midround_cooldown, 0.0);
    }

    #[test]
    fn tier_counts_and_thresholds_follow_the_config() {
        for seed in 0..50 {
            let mut rng = Rng::new(seed);
            // Greenshift spawns nothing midround.
            assert_eq!(kind_count(0, Kind::Light, 60, &mut rng), 0);
            // High chaos heavies: 2..=4 above 40 players, 2..=2 at 40 (halved), 2 at 20.
            let n = kind_count(4, Kind::Heavy, 60, &mut rng);
            assert!((2..=4).contains(&n), "{n}");
            assert_eq!(kind_count(4, Kind::Heavy, 40, &mut rng), 2);
            assert_eq!(kind_count(4, Kind::Heavy, 20, &mut rng), 2);
            // Low chaos lights 0..=2 above 40, 0..=1 at 30.
            assert!(kind_count(1, Kind::Light, 30, &mut rng) <= 1);
        }
        // Light midrounds may start after 30 minutes, heavies after 60, latejoins after 5
        // (high chaos: 20 and 30).
        assert_eq!(TIER_RANGES[2].map(|r| r.2), [30, 60, 5]);
        assert_eq!(TIER_RANGES[4].map(|r| r.2), [20, 30, 5]);
    }

    #[test]
    fn chances_follow_the_dead_share() {
        assert_eq!(base_chance(10, 0), 100.0);
        assert_eq!(base_chance(8, 2), 60.0);
        assert_eq!(base_chance(5, 5), 0.0);
        assert!(base_chance(2, 8) < 0.0);
        assert_eq!(base_chance(0, 0), 0.0);
        // A fifth before light midrounds start, +15% per failed latejoin.
        assert_eq!(latejoin_chance(8, 2, 0, false), 12.0);
        assert_eq!(latejoin_chance(8, 2, 2, true), 90.0);
    }

    #[test]
    fn weights_follow_repeats_and_high_impact() {
        let traitor = ruleset(MIDROUND_TRAITOR).unwrap();
        let mass = ruleset(MASS_TRAITORS).unwrap();
        let nukies = ruleset(MIDROUND_NUKEOPS).unwrap();
        assert_eq!(get_weight(traitor, 10, 2, &[]), 10);
        // `min_pop` 3, tier 0 nothing.
        assert_eq!(get_weight(traitor, 2, 2, &[]), 0);
        assert_eq!(get_weight(traitor, 10, 0, &[]), 0);
        // A previous traitor (or mass traitors, a subtype) costs 2 each.
        assert_eq!(get_weight(traitor, 10, 2, &[MIDROUND_TRAITOR]), 8);
        assert_eq!(
            get_weight(traitor, 10, 2, &[MASS_TRAITORS, MIDROUND_TRAITOR]),
            6
        );
        // Mass traitors are not a midround traitor: no penalty the other way.
        assert_eq!(get_weight(mass, 20, 3, &[MIDROUND_TRAITOR]), 8);
        assert_eq!(get_weight(mass, 20, 3, &[MASS_TRAITORS]), 1);
        // Non-repeatable, and below the high tier another high impact ruleset blocks.
        assert_eq!(get_weight(nukies, 30, 3, &[]), 3);
        assert_eq!(get_weight(nukies, 30, 3, &[MIDROUND_NUKEOPS]), 0);
        assert_eq!(get_weight(nukies, 30, 3, &["Roundstart Revolution"]), 0);
        assert_eq!(get_weight(nukies, 30, 4, &["Roundstart Revolution"]), 3);
        assert_eq!(get_weight(nukies, 30, 3, &["Roundstart Traitor"]), 3);
        // Caps: nukies `ceil(pop / 18) + 1`, mass traitors 2..4.
        assert_eq!(nukies.caps.0.get(30), 3);
        assert_eq!((mass.caps.0.get(30), mass.caps.1.get(30)), (2, 4));
        // Picks at 10 players in a low-medium tier: only the traitor and obsessed lights
        // (Morph weighs 0, the rest need more players); traitors about 2/3 of the time.
        let pool: Vec<(&'static Ruleset, u32)> = RULESETS
            .iter()
            .filter(|r| r.kind == Kind::Light)
            .map(|r| (r, get_weight(r, 10, 2, &[])))
            .filter(|(_, w)| *w > 0)
            .collect();
        let tags: Vec<&str> = pool.iter().map(|(r, _)| r.config_tag).collect();
        assert_eq!(
            tags,
            [
                "Revenant",
                "Paradox Clone",
                MIDROUND_TRAITOR,
                "Midround Obsessed"
            ]
        );
        let mut rng = Rng::new(7);
        let traitors = (0..1000)
            .filter(|_| pick(&pool, &mut rng).unwrap().config_tag == MIDROUND_TRAITOR)
            .count();
        // 10 of 5 + 5 + 10 + 5.
        assert!((330..470).contains(&traitors), "{traitors}");
    }
}
