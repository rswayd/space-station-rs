//! Xenobiology slimes and their extracts (server), after tg `/mob/living/basic/slime`
//! (`code/modules/mob/living/basic/slime/`), `/datum/slime_type`, `/obj/item/slime_extract`
//! and `/obj/item/slimepotion` (`code/modules/research/xenobiology/xenobiology.dm`) and the
//! extract reactions (`code/modules/reagents/chemistry/recipes/slime_extracts.dm`).
//!
//! - A `/mob/living/basic/slime` basic mob gets a [`Slime`]: its colour (one of the 24
//!   `/datum/slime_type`s), life stage, nutrition, growth, power level, cores and mutation
//!   chance. Every `SSmobs` fire (`Life` -> `handle_nutrition`) it gets hungry, grows while fed
//!   over 150 nutrition and charges up; BZ (5% of the air, under 100 C) puts it in stasis.
//! - An AI slime that is hungry or rabid hunts the nearest living thing it can feed on within
//!   7 tiles, latches on (`start_feeding`) and drains it every second (`slime_leech`: brute and
//!   toxin, nutrition 1.8 per point, healing itself) until the meal dies or is no food.
//! - Grown (10 growth) a baby evolves into an adult for 100 nutrition, an adult splits into
//!   four babies (`reproduce`), each mutating by its type's weighted mutations at
//!   `mutation_chance` (rainbow at 100); two other slimes on its tile overcrowd it.
//! - A dying adult splits instead (`death`): it lives on as a baby next to a new rabid baby.
//! - A hemostat or crowbar on a dead slime takes its cores out (the core removal operation,
//!   1.6 s): one extract of its type per core.
//! - Extracts hold 100 units (injectable, drawable). Their reactions are the converted
//!   `/datum/chemical_reaction/slime` types; each one that runs uses the extract up
//!   (`extract_uses`) and does its `on_reaction` ([`extract_reacted`]): slimes, sheets,
//!   potions and items, EMP, gas and explosions (5 s later), enraged slimes. A spent extract is
//!   "used" and reacts no more; one without reagent results is gone.
//! - Potions on slimes: docility, steroid, stabilizer, mutator; the enhancer on an extract.
//! - An adult slime fed 10 extracts of one kind becomes the cross-extract of its colour and
//!   that extract's family (`spawn_corecross`).
//! - The extracts' floor tiles lay `/turf/open/floor/bluespace` (`slowdown = -1`) and
//!   `/turf/open/floor/sepia` (`slowdown = 2`): a mob stepping onto one gets its slowdown
//!   (`/datum/movespeed_modifier/turf_slowdown`, [`TurfSlowdown`]) until it steps off.
//!
//! Cost: every 2 s one pass over the slimes (tens in a pen); hunting slimes scan 7 tiles every
//! 0.5 s; a feeding slime follows its meal every tick.
// shortcut: no slime AI tree (the hunt above stands in for `slime.bt.json`; no pet commands,
// friends, face moods or retaliation), no player slime actions (evolve/reproduce/feed are
// automatic for AI slimes only), no shocks on attack, no cold stun (`freon`), no `slimed`
// colouring after feeding; the gold mob spawns, food/drink borks, foam, timestop and rainbow
// cluster grenades only use the extract (their `on_reaction` effects are not ported); the
// potions other than docility, steroid, stabilizer, mutator and enhancer do nothing yet;
// cross-extracts are items without their effects.

use std::sync::Arc;

use bevy::prelude::*;
use ss13_mapload::mobs::MobDef;
use ss13_mapload::{DmItem, DmMob, DmName};
use ss13_net::chat::server::SendChat;
use ss13_reagents::{Chemistry, Holder, Owner, flags, is_under};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_world::{Density, MoveRequest, SpatialIndex};

use crate::chem::{Reagents, clients_near, line, map_pos, name_of};
use crate::combat::CombatRng;
use crate::health::{DamageType, Health, HealthSystems, Stat, update_stat};
use crate::mobs::BasicMob;
use crate::server::PlayerMob;
use crate::tools::{Actor, DoAfter, do_after, tell};

pub const SLIME: &str = "/mob/living/basic/slime";
const SLIME_RANDOM: &str = "/mob/living/basic/slime/random";
const SLIME_PET: &str = "/mob/living/basic/slime/pet";
const SLIME_HILBERT: &str = "/mob/living/basic/slime/hilbert";
pub const EXTRACT: &str = "/obj/item/slime_extract";
const SLIME_REACTIONS: &str = "/datum/chemical_reaction/slime/";
const SLIME_DMI: &str = "icons/mob/simple/slimes.dmi";

/// `SLIME_MAX_NUTRITION`, `SLIME_STARTING_NUTRITION`, `SLIME_GROW_NUTRITION`,
/// `SLIME_HUNGER_NUTRITION` (starving, below 10, is hungry too).
const MAX_NUTRITION: f32 = 200.0;
const STARTING_NUTRITION: f32 = 100.0;
const GROW_NUTRITION: f32 = 150.0;
const HUNGER_NUTRITION: f32 = 50.0;
/// `SLIME_EVOLUTION_THRESHOLD`, `SLIME_EVOLUTION_COST`, `SLIME_OVERCROWD_AMOUNT`.
const EVOLUTION_THRESHOLD: u32 = 10;
const EVOLUTION_COST: f32 = 100.0;
const OVERCROWD_AMOUNT: usize = 2;
/// `SLIME_MEDIUM_POWER`, `SLIME_MAX_POWER`.
const MEDIUM_POWER: i32 = 5;
const MAX_POWER: i32 = 10;
/// `SLIME_EXTRACT_CROSSING_REQUIRED`.
const CROSSING_REQUIRED: u32 = 10;
/// `SLIME_TRANSPARENCY_ALPHA`.
const TRANSPARENCY_ALPHA: u8 = 180;
/// The `slime_leech` status effect's `tick_interval`.
const LEECH_SECS: f64 = 1.0;
/// How often an AI slime looks for food and steps towards it.
const THINK_SECS: f64 = 0.5;
/// The hunt's view.
const HUNT_RANGE: u32 = 7;
/// The core removal operation's `time`, deciseconds.
const CORE_REMOVAL_DS: f32 = 16.0;
/// `T0C`.
const T0C: f64 = 273.15;
/// Delayed extract effects (`addtimer(..., 5 SECONDS)`) and the extract's deletion after them
/// (`addtimer(delete_extract, 55)`).
const DELAYED_SECS: f64 = 5.0;
const DELAYED_DELETE_SECS: f64 = 5.5;

/// One `/datum/slime_type`.
pub struct SlimeType {
    /// `colour`: also the icon state prefix.
    pub colour: &'static str,
    pub transparent: bool,
    /// `mutations` with their weights.
    pub mutations: &'static [(&'static str, u32)],
    /// The extract's `crossbreed_modification`.
    pub crossbreed: &'static str,
}

const fn t(
    colour: &'static str,
    transparent: bool,
    mutations: &'static [(&'static str, u32)],
    crossbreed: &'static str,
) -> SlimeType {
    SlimeType {
        colour,
        transparent,
        mutations,
        crossbreed,
    }
}

/// Every `/datum/slime_type` in tg's order (`subtypesof`).
pub const SLIME_TYPES: [SlimeType; 22] = [
    t(
        "grey",
        true,
        &[("blue", 1), ("metal", 1), ("orange", 1), ("purple", 1)],
        "reproductive",
    ),
    t(
        "blue",
        true,
        &[("dark-blue", 1), ("pink", 2), ("silver", 1)],
        "stabilized",
    ),
    t(
        "metal",
        false,
        &[("gold", 2), ("silver", 1), ("yellow", 1)],
        "industrial",
    ),
    t(
        "purple",
        true,
        &[("dark-blue", 1), ("dark-purple", 1), ("green", 2)],
        "regenerative",
    ),
    t(
        "orange",
        true,
        &[("dark-purple", 1), ("red", 2), ("yellow", 1)],
        "burning",
    ),
    t(
        "dark-blue",
        true,
        &[("blue", 1), ("cerulean", 2), ("purple", 1)],
        "chilling",
    ),
    t(
        "dark-purple",
        false,
        &[("orange", 1), ("purple", 1), ("sepia", 2)],
        "self-sustaining",
    ),
    t(
        "silver",
        false,
        &[("blue", 1), ("metal", 1), ("pyrite", 2)],
        "consuming",
    ),
    t(
        "yellow",
        true,
        &[("bluespace", 2), ("metal", 1), ("orange", 1)],
        "charged",
    ),
    t("bluespace", false, &[("bluespace", 1)], "warping"),
    t("cerulean", true, &[("cerulean", 1)], "recurring"),
    t("pyrite", false, &[("pyrite", 1)], "prismatic"),
    t("sepia", true, &[("sepia", 1)], "lengthened"),
    t("gold", false, &[("adamantine", 1), ("gold", 1)], "symbiont"),
    t("green", true, &[("black", 1), ("green", 1)], "mutative"),
    t("pink", true, &[("light-pink", 1), ("pink", 1)], "gentle"),
    t("red", true, &[("oil", 1), ("red", 1)], "sanguine"),
    t("adamantine", false, &[("adamantine", 1)], "crystalline"),
    t("black", true, &[("black", 1)], "transformative"),
    t("light-pink", true, &[("light-pink", 1)], "loyal"),
    t("oil", false, &[("oil", 1)], "detonating"),
    t("rainbow", true, &[("rainbow", 1)], "hyperchromatic"),
];

const TYPE_COUNT: usize = SLIME_TYPES.len();

pub fn slime_type(colour: &str) -> &'static SlimeType {
    SLIME_TYPES[..TYPE_COUNT]
        .iter()
        .find(|t| t.colour == colour)
        .unwrap_or(&SLIME_TYPES[0])
}

/// `core_type`: `/obj/item/slime_extract/<colour without dashes>`.
pub fn extract_path(colour: &str) -> String {
    format!("{EXTRACT}/{}", colour.replace('-', ""))
}

/// The colour of an extract type.
fn extract_colour(path: &str) -> Option<&'static str> {
    let name = path.strip_prefix(EXTRACT)?.strip_prefix('/')?;
    SLIME_TYPES[..TYPE_COUNT]
        .iter()
        .find(|t| t.colour.replace('-', "") == name)
        .map(|t| t.colour)
}

/// tg `/mob/living/basic/slime` state.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Slime {
    /// `slime_type.colour`.
    pub colour: &'static str,
    /// `life_stage == SLIME_LIFE_STAGE_ADULT`.
    pub adult: bool,
    pub nutrition: f32,
    pub amount_grown: u32,
    pub powerlevel: i32,
    pub cores: u32,
    pub mutation_chance: f32,
    pub hunger_disabled: bool,
    /// `BB_SLIME_RABID`.
    pub rabid: bool,
    pub mutator_used: bool,
    pub overcrowded: bool,
    /// `/datum/status_effect/grouped/stasis` from BZ.
    pub stasis: bool,
    /// `BB_SLIME_HUNGER_LEVEL` is not `SLIME_HUNGER_NONE`.
    pub hungry: bool,
    /// The mob it is latched onto (`buckled`).
    pub feeding: Option<Entity>,
    /// `crossbreed_modification` and `applied_crossbreed_amount`.
    pub crossbreed: Option<&'static str>,
    pub crossbreed_amount: u32,
    /// Renamed (a docility potion): `update_name` keeps it.
    pub named: bool,
    next_leech: f64,
    next_think: f64,
}

impl Default for Slime {
    fn default() -> Self {
        Self {
            colour: "grey",
            adult: false,
            nutrition: STARTING_NUTRITION,
            amount_grown: 0,
            powerlevel: 0,
            cores: 1,
            mutation_chance: 30.0,
            hunger_disabled: false,
            rabid: false,
            mutator_used: false,
            overcrowded: false,
            stasis: false,
            hungry: false,
            feeding: None,
            crossbreed: None,
            crossbreed_amount: 0,
            named: false,
            next_leech: 0.0,
            next_think: 0.0,
        }
    }
}

impl Slime {
    /// `adjust_nutrition`: between 0 and `SLIME_MAX_NUTRITION`.
    pub fn adjust_nutrition(&mut self, amount: f32) {
        self.nutrition = (self.nutrition + amount).clamp(0.0, MAX_NUTRITION);
    }

    /// `adjust_power_level`.
    pub fn adjust_power(&mut self, amount: i32) {
        self.powerlevel = (self.powerlevel + amount).clamp(0, MAX_POWER);
    }

    fn stage(&self) -> &'static str {
        if self.adult { "adult" } else { "baby" }
    }
}

/// `get_random_mutation()`: rainbow at 100% mutation chance, else a weighted mutation with
/// `mutation_chance` percent, else the same type.
pub fn random_mutation(rng: &mut CombatRng, colour: &'static str, chance: f32) -> &'static str {
    if chance >= 100.0 {
        return "rainbow";
    }
    if !rng.chance(chance) {
        return colour;
    }
    let mutations = slime_type(colour).mutations;
    let total: u32 = mutations.iter().map(|m| m.1).sum();
    let mut roll = rng.range(1, total.max(1));
    for &(m, w) in mutations {
        if roll <= w {
            return slime_type(m).colour;
        }
        roll -= w;
    }
    colour
}

/// `SPT_PROB(percent, seconds)`.
fn spt_prob(rng: &mut CombatRng, percent: f64, seconds: f64) -> bool {
    let p = 100.0 * (1.0 - (1.0 - percent / 100.0).powf(seconds));
    rng.chance(p as f32)
}

/// An extract's `extract_uses`, and a delayed effect of its last reaction.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct SlimeExtract {
    pub uses: u32,
    /// (when, what) of a 5 s `addtimer` effect, and when the spent extract goes after it.
    pub timer: Option<(f64, Delayed)>,
    pub delete_at: Option<f64>,
}

/// What a delayed extract reaction does when its timer runs out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Delayed {
    /// `atmos_spawn_air` on the extract's turf.
    Gas(&'static str),
    /// `explosion(devastation, heavy, light)`.
    Explosion(i32, i32, i32),
    /// Only the extract's use (the effect is not ported).
    Nothing,
}

#[derive(Resource, Default)]
struct SlimeClock(f64);

pub struct XenobioPlugin;

impl Plugin for XenobioPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // Core removal, potions on slimes, the enhancer and crossbreeding extracts.
        app.add_click_handler(ClickPhase::HeldOnTarget, 1777, |w, c| {
            c.held
                .is_some_and(|h| item_interaction(w, c.client, c.mob, h, c.target))
        });
        app.init_resource::<SlimeClock>()
            .add_observer(on_slime_added)
            .add_observer(on_extract_added)
            .add_systems(
                Update,
                (
                    (slime_life, slime_tick)
                        .chain()
                        .after(crate::click::ResolveClicks)
                        .before(HealthSystems),
                    extract_timers,
                    update_turf_slowdown
                        .after(ss13_world::WorldSystems::Movement)
                        .before(HealthSystems),
                ),
            );
    }
}

// --- Spawning -----------------------------------------------------------------------------

fn on_slime_added(add: On<Add<BasicMob>>, mobs: Query<&DmMob>, mut commands: Commands) {
    let Ok(mob) = mobs.get(add.entity) else {
        return;
    };
    if !is_under(&mob.path, SLIME) {
        return;
    }
    let e = add.entity;
    commands.queue(move |world: &mut World| init_slime(world, e));
}

/// tg `/mob/living/basic/slime/Initialize` and its subtypes'.
fn init_slime(world: &mut World, e: Entity) {
    if world.get::<Slime>(e).is_some() || world.get::<BasicMob>(e).is_none() {
        return;
    }
    let path = world
        .get::<DmMob>(e)
        .map(|m| m.path.clone())
        .unwrap_or_default();
    let mut slime = Slime::default();
    let mut rng = CombatRng(world.resource::<CombatRng>().0);
    if is_under(&path, SLIME_RANDOM) {
        slime.colour = SLIME_TYPES[rng.range(0, TYPE_COUNT as u32 - 1) as usize].colour;
        slime.adult = rng.chance(50.0);
    } else if is_under(&path, SLIME_HILBERT) {
        slime.colour = "bluespace";
        slime.rabid = true;
    } else if is_under(&path, SLIME_PET) {
        slime.hunger_disabled = true;
    }
    world.resource_mut::<CombatRng>().0 = rng.0;
    world.entity_mut(e).insert(slime);
    refresh(world, e, true);
}

/// `new /mob/living/basic/slime(loc, type, life_stage)`. `None` without its prototype.
pub fn spawn_slime(
    world: &mut World,
    pos: TilePos,
    colour: &'static str,
    adult: bool,
) -> Option<Entity> {
    let e = crate::mobs::spawn::spawn_mob(world, SLIME, pos)?;
    world.flush();
    if world.get::<BasicMob>(e).is_none() {
        return Some(e);
    }
    init_slime(world, e);
    if let Some(mut s) = world.get_mut::<Slime>(e) {
        s.colour = slime_type(colour).colour;
        s.adult = adult;
    }
    refresh(world, e, true);
    Some(e)
}

/// `set_life_stage`, `update_name` and `regenerate_icons` after the stage or colour changed:
/// the adult's health and attacks (`/ 0.75`, `* 2`), the name, the sprite and its alpha.
pub fn refresh(world: &mut World, e: Entity, rename: bool) {
    let Some(slime) = world.get::<Slime>(e).cloned() else {
        return;
    };
    let path = world
        .get::<DmMob>(e)
        .map(|m| m.path.clone())
        .unwrap_or_default();
    let base = crate::mobs::mob_defs(world).0.get(&path).cloned();
    let Some(base) = base else {
        return;
    };
    let mut def: MobDef = (*base).clone();
    if slime.adult {
        def.max_health = base.max_health / 0.75;
        def.melee_damage_lower *= 2.0;
        def.melee_damage_upper *= 2.0;
        def.obj_damage = 15.0;
    }
    if let Some(mut h) = world.get_mut::<Health>(e)
        && h.max != def.max_health
    {
        // tg scales `health` with `maxHealth`: the damage keeps its share.
        let ratio = def.max_health / h.max.max(1.0);
        let damage = h.brute;
        h.max = def.max_health;
        h.adjust(DamageType::Brute, damage * ratio - damage);
    }
    let living = format!("{}-{}", slime.colour, slime.stage());
    let dead = if slime.cores == 0 {
        format!("{}-cut", slime.colour)
    } else {
        format!("{living}-dead")
    };
    let is_dead = world.get::<Stat>(e).is_some_and(|s| *s == Stat::Dead);
    let state = if is_dead {
        dead.clone()
    } else {
        living.clone()
    };
    world.entity_mut(e).insert(BasicMob {
        def: Arc::new(def),
        icon_living: living,
        icon_dead: dead,
        mouth: false,
    });
    crate::mobs::set_icon_state(world, e, SLIME_DMI, &state);
    let alpha = slime_type(slime.colour).transparent;
    if let Some(mut a) = world.get_mut::<ss13_shared::Appearance>(e) {
        let color = alpha.then_some([255, 255, 255, TRANSPARENCY_ALPHA]);
        if a.color != color {
            a.color = color;
        }
    }
    if rename && !slime.named {
        let id = world.resource_mut::<CombatRng>().range(1, 1000);
        let name = format!("{} {} slime ({id})", slime.colour, slime.stage());
        set_name(world, e, &name);
    }
}

fn set_name(world: &mut World, e: Entity, name: &str) {
    let mut ent = world.entity_mut(e);
    ent.insert(Name::new(name.to_owned()));
    if let Some(mut n) = ent.get_mut::<DmName>() {
        n.name = name.to_owned();
    }
}

pub(crate) fn say_around(world: &mut World, pos: TilePos, span: Span, text: &str) {
    let clients = clients_near(world, pos, None);
    if !clients.is_empty() {
        world.write_message(SendChat {
            clients,
            message: line(span, text),
        });
    }
}

// --- Life ---------------------------------------------------------------------------------

/// `SSmobs` -> `Life()` of slimes: BZ stasis and `handle_nutrition`, then the AI's evolve and
/// reproduce abilities.
fn slime_life(world: &mut World) {
    let delta = world.resource::<Time>().delta_secs_f64();
    let fires = {
        let mut clock = world.resource_mut::<SlimeClock>();
        clock.0 += delta;
        let n = (clock.0 / crate::breath::SSMOBS_WAIT_SECS).floor();
        clock.0 -= n * crate::breath::SSMOBS_WAIT_SECS;
        n as u32
    };
    for _ in 0..fires {
        let slimes: Vec<Entity> = world
            .query_filtered::<Entity, (With<Slime>, With<TilePos>)>()
            .iter(world)
            .collect();
        for e in slimes {
            live(world, e, crate::breath::SSMOBS_WAIT_SECS);
        }
    }
}

fn live(world: &mut World, e: Entity, secs: f64) {
    if world.get::<Stat>(e).is_none_or(|s| *s == Stat::Dead) {
        return;
    }
    let Some(&pos) = world.get::<TilePos>(e) else {
        return;
    };
    // `handle_slime_stasis`.
    let body = world
        .get::<crate::mobs::BodyTemperature>(e)
        .map_or(310.15, |b| b.0);
    let bz = world
        .get_resource::<ss13_atmos::Atmos>()
        .and_then(|a| a.air(pos))
        .map_or(0.0, |air| {
            let total = air.total_moles();
            if total > 0.0 {
                air.get(ss13_atmos::GasId::BZ) / total
            } else {
                0.0
            }
        });
    // `TRAIT_STASIS` from the slime console's storage (`super::xenobio_console`).
    let stored = world.get::<crate::closets::InCloset>(e).is_some();
    let stasis = stored || (bz >= 0.05 && body < T0C + 100.0);
    let mut rng = CombatRng(world.resource::<CombatRng>().0);
    let mut starve_damage = 0.0;
    {
        let Some(mut s) = world.get_mut::<Slime>(e) else {
            return;
        };
        if stasis && !s.stasis {
            s.adjust_power(-MAX_POWER);
            s.rabid = false;
        }
        s.stasis = stasis;
        if !stasis {
            starve_damage = handle_nutrition(&mut s, &mut rng, secs);
        }
    }
    world.resource_mut::<CombatRng>().0 = rng.0;
    if starve_damage > 0.0 {
        damage(world, e, DamageType::Brute, starve_damage);
    }
    if world.get::<PlayerMob>(e).is_none() {
        evolve_or_reproduce(world, e);
    }
}

/// `handle_nutrition(seconds_per_tick)`; the starvation brute damage it deals.
pub fn handle_nutrition(s: &mut Slime, rng: &mut CombatRng, secs: f64) -> f32 {
    if s.hunger_disabled {
        s.nutrition = 100.0;
        s.hungry = false;
        return 0.0;
    }
    if spt_prob(rng, 1.25, secs) {
        let rate = if s.adult { -1.0 } else { -0.5 };
        s.adjust_nutrition(rate * secs as f32);
    }
    s.hungry = s.nutrition < HUNGER_NUTRITION
        || (s.nutrition < GROW_NUTRITION && spt_prob(rng, 25.0, secs));
    if s.nutrition == 0.0 {
        if spt_prob(rng, 50.0, secs) {
            return rng.range(0, 5) as f32;
        }
        return 0.0;
    }
    if s.nutrition >= GROW_NUTRITION {
        if s.amount_grown < EVOLUTION_THRESHOLD {
            s.adjust_nutrition(-2.5 * secs as f32);
            s.amount_grown += 1;
        }
        if s.powerlevel < MAX_POWER && spt_prob(rng, f64::from(30 - s.powerlevel * 2), secs) {
            s.adjust_power(1);
        }
    } else if s.powerlevel < MEDIUM_POWER
        && s.nutrition >= HUNGER_NUTRITION
        && spt_prob(rng, f64::from(25 - s.powerlevel * 5), secs)
    {
        s.adjust_power(1);
    }
    0.0
}

/// Damage to a living mob with tg `update_stat`; the health it lost.
fn damage(world: &mut World, mob: Entity, kind: DamageType, amount: f32) -> f32 {
    let Some(mut stat) = world.get::<Stat>(mob).copied() else {
        return 0.0;
    };
    let Some(mut health) = world.get_mut::<Health>(mob) else {
        return 0.0;
    };
    let before = health.health();
    health.adjust(kind, amount);
    let after = health.health();
    let health = health.clone();
    if update_stat(&health, &mut stat).is_some() {
        *world.get_mut::<Stat>(mob).expect("checked above") = stat;
    }
    before - after
}

/// The slime AI's `BB_SLIME_EVOLVE` and `BB_SLIME_REPRODUCE` abilities when available.
fn evolve_or_reproduce(world: &mut World, e: Entity) {
    let Some(s) = world.get::<Slime>(e).cloned() else {
        return;
    };
    if s.amount_grown < EVOLUTION_THRESHOLD || s.feeding.is_some() {
        return;
    }
    if !s.adult {
        if s.nutrition < EVOLUTION_COST {
            return;
        }
        if let Some(mut s) = world.get_mut::<Slime>(e) {
            s.adjust_nutrition(-EVOLUTION_COST);
            s.adult = true;
            s.amount_grown = 0;
        }
        refresh(world, e, true);
        return;
    }
    reproduce(world, e);
}

/// `reproduce()`: three babies of random mutations next to it, it becomes a baby of one too.
pub fn reproduce(world: &mut World, e: Entity) {
    let (Some(s), Some(&pos)) = (world.get::<Slime>(e).cloned(), world.get::<TilePos>(e)) else {
        return;
    };
    if !s.adult || s.amount_grown < EVOLUTION_THRESHOLD {
        return;
    }
    let others = world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .filter(|&&o| o != e && world.get::<Slime>(o).is_some())
        .count();
    let overcrowded = others >= OVERCROWD_AMOUNT;
    if let Some(mut slime) = world.get_mut::<Slime>(e) {
        slime.overcrowded = overcrowded;
    }
    if overcrowded {
        return;
    }
    let player = world.get::<PlayerMob>(e).is_some();
    let new_nutrition = (s.nutrition * 0.9).floor();
    let new_power = (s.powerlevel as f32 * 0.25).floor() as i32;
    let mut created = vec![e];
    for _ in 0..3 {
        let mut rng = CombatRng(world.resource::<CombatRng>().0);
        let colour = random_mutation(&mut rng, s.colour, s.mutation_chance);
        world.resource_mut::<CombatRng>().0 = rng.0;
        let Some(baby) = spawn_slime(world, pos, colour, false) else {
            continue;
        };
        created.push(baby);
        // `step_away(baby, src)`.
        let dir = world.resource_mut::<CombatRng>().range(0, 7) as usize;
        let dir = ss13_shared::Dir::DMI_ORDER[dir];
        world.write_message(MoveRequest { entity: baby, dir });
    }
    let mut rng = CombatRng(world.resource::<CombatRng>().0);
    let own = random_mutation(&mut rng, s.colour, s.mutation_chance);
    for &baby in &created {
        let chance = if s.mutation_chance > 0.0 {
            (s.mutation_chance + rng.range(0, 10) as f32 - 5.0).clamp(0.0, 100.0)
        } else {
            0.0
        };
        if let Some(mut b) = world.get_mut::<Slime>(baby) {
            if player {
                b.nutrition = new_nutrition;
            }
            b.powerlevel = new_power;
            b.mutation_chance = chance;
        }
    }
    world.resource_mut::<CombatRng>().0 = rng.0;
    if let Some(mut slime) = world.get_mut::<Slime>(e) {
        if !player {
            slime.nutrition = STARTING_NUTRITION;
        }
        slime.adult = false;
        slime.colour = own;
        slime.amount_grown = 0;
        slime.mutator_used = false;
    }
    refresh(world, e, true);
}

// --- Every tick: deaths, feeding, hunting -------------------------------------------------

fn slime_tick(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let slimes: Vec<Entity> = world
        .query_filtered::<Entity, (With<Slime>, With<TilePos>)>()
        .iter(world)
        .collect();
    for e in slimes {
        let Some(s) = world.get::<Slime>(e).cloned() else {
            continue;
        };
        // Stored in a slime console: in stasis.
        if world.get::<crate::closets::InCloset>(e).is_some() {
            continue;
        }
        let dead = world.get::<Stat>(e).is_none_or(|st| *st == Stat::Dead);
        let dying = world.get::<Health>(e).is_some_and(|h| h.health() <= 0.0);
        if s.adult && (dead || dying) {
            split_on_death(world, e);
            continue;
        }
        if dead {
            if s.feeding.is_some() {
                stop_feeding(world, e, true);
            }
            continue;
        }
        if let Some(victim) = s.feeding {
            feed(world, e, victim, now);
            continue;
        }
        if now >= s.next_think && world.get::<PlayerMob>(e).is_none() {
            if let Some(mut slime) = world.get_mut::<Slime>(e) {
                slime.next_think = now + THINK_SECS;
            }
            hunt(world, e, &s);
        }
    }
}

/// `death()` of an adult: it lives on as a healed baby and a new rabid baby of its type
/// appears.
fn split_on_death(world: &mut World, e: Entity) {
    let (Some(s), Some(&pos)) = (world.get::<Slime>(e).cloned(), world.get::<TilePos>(e)) else {
        return;
    };
    if s.feeding.is_some() {
        stop_feeding(world, e, true);
    }
    if let Some(baby) = spawn_slime(world, pos, s.colour, false)
        && let Some(mut b) = world.get_mut::<Slime>(baby)
    {
        b.rabid = true;
    }
    if let Some(mut slime) = world.get_mut::<Slime>(e) {
        slime.adult = false;
    }
    refresh(world, e, true);
    // `revive(HEAL_ALL)`.
    if let Some(mut h) = world.get_mut::<Health>(e) {
        let healed = h.healed();
        *h = healed;
    }
    if let Some(mut st) = world.get_mut::<Stat>(e)
        && *st != Stat::Conscious
    {
        *st = Stat::Conscious;
    }
}

/// `can_feed_on(meal)`.
pub fn can_feed_on(world: &World, slime: Entity, meal: Entity) -> bool {
    if meal == slime
        || world.get::<Slime>(slime).is_none_or(|s| s.hunger_disabled)
        || world.get::<Slime>(meal).is_some()
        || world.get::<crate::silicon::Silicon>(meal).is_some()
        || world.get::<Stat>(meal).is_none_or(|s| *s == Stat::Dead)
        || world.get::<crate::closets::InCloset>(meal).is_some()
    {
        return false;
    }
    let Some(h) = world.get::<Health>(meal) else {
        return false;
    };
    // `GET_PHYSIOLOGY(meal, BRUTE) <= 0 && GET_PHYSIOLOGY(meal, TOX) <= 0`.
    if h.basic.is_some_and(|p| p[0] <= 0.0 && p[2] <= 0.0) {
        return false;
    }
    // `locate(/mob/living/basic/slime) in meal.buckled_mobs`.
    let mut q = world.try_query::<(Entity, &Slime)>();
    !q.as_mut().is_some_and(|q| {
        q.iter(world)
            .any(|(o, s)| o != slime && s.feeding == Some(meal))
    })
}

/// The hunt that stands in for the slime tree's food search: the nearest meal within
/// [`HUNT_RANGE`] while hungry or rabid; latch on next to it, else step closer.
fn hunt(world: &mut World, e: Entity, s: &Slime) {
    if s.stasis || s.hunger_disabled || !(s.hungry || s.rabid) {
        return;
    }
    let Some(&pos) = world.get::<TilePos>(e) else {
        return;
    };
    let mut best: Option<(u32, Entity, TilePos)> = None;
    for m in world.resource::<SpatialIndex>().in_radius(pos, HUNT_RANGE) {
        let Some(&at) = world.get::<TilePos>(m) else {
            continue;
        };
        let Some(d) = pos.chebyshev(at) else {
            continue;
        };
        if best.is_some_and(|(bd, ..)| bd <= d) || !can_feed_on(world, e, m) {
            continue;
        }
        best = Some((d, m, at));
    }
    let Some((dist, meal, at)) = best else {
        return;
    };
    if dist <= 1 {
        // `feed_on_slime_target`: a healthy player gets a moment (20% a try).
        let player = world.get::<PlayerMob>(meal).is_some();
        let healthy = world
            .get::<Health>(meal)
            .is_some_and(|h| h.health() >= 20.0);
        let lying = world
            .get::<crate::health::BodyPosition>(meal)
            .is_some_and(|b| b.lying());
        if lying || !player || !healthy || world.resource_mut::<CombatRng>().chance(20.0) {
            start_feeding(world, e, meal);
        }
        return;
    }
    let dir = crate::mobs::ai::movement::greedy_step(world.resource::<SpatialIndex>(), e, pos, at);
    if let Some(dir) = dir {
        world.write_message(MoveRequest { entity: e, dir });
    }
}

/// `start_feeding(target)`: latched onto the meal's tile, not dense while on it.
pub fn start_feeding(world: &mut World, e: Entity, meal: Entity) {
    let Some(&at) = world.get::<TilePos>(meal) else {
        return;
    };
    let now = world.resource::<Time>().elapsed_secs_f64();
    if let Some(mut s) = world.get_mut::<Slime>(e) {
        s.feeding = Some(meal);
        s.next_leech = now + LEECH_SECS;
    }
    world.entity_mut(e).insert(at).remove::<Density>();
    crate::afflictions::leech_started(world, meal);
    let (slime, victim) = (name_of(world, e), name_of(world, meal));
    say_around(
        world,
        at,
        Span::Danger,
        &format!("{slime} присасывается к {victim}!"),
    );
    crate::chem::tell_mob(
        world,
        e,
        Span::Notice,
        &format!("Я начинаю питаться {victim}..."),
    );
}

/// `stop_feeding(silent)`.
pub fn stop_feeding(world: &mut World, e: Entity, silent: bool) {
    let Some(meal) = world.get_mut::<Slime>(e).and_then(|mut s| s.feeding.take()) else {
        return;
    };
    // `slime_leech/on_buckle_end`: the meal may be slimed.
    crate::afflictions::leech_ended(world, meal);
    let alive = world.get::<Stat>(e).is_some_and(|s| *s != Stat::Dead);
    let dense = world.get::<BasicMob>(e).is_some_and(|b| b.def.density);
    if alive && dense {
        world.entity_mut(e).insert(Density);
    }
    if !silent && let Some(pos) = world.get::<TilePos>(e).copied() {
        let (slime, victim) = (name_of(world, e), name_of(world, meal));
        say_around(
            world,
            pos,
            Span::Warning,
            &format!("{slime} отпускает {victim}!"),
        );
    }
}

/// tg `slime_leech/tick` once a second while latched; the slime follows its meal.
fn feed(world: &mut World, e: Entity, meal: Entity, now: f64) {
    let meal_pos = world.get::<TilePos>(meal).copied();
    let Some(meal_pos) = meal_pos.filter(|_| world.get::<Health>(meal).is_some()) else {
        stop_feeding(world, e, true);
        return;
    };
    if world.get::<TilePos>(e) != Some(&meal_pos) {
        world.entity_mut(e).insert(meal_pos);
    }
    let due = world.get::<Slime>(e).is_some_and(|s| now >= s.next_leech);
    if !due {
        return;
    }
    if let Some(mut s) = world.get_mut::<Slime>(e) {
        s.next_leech = now + LEECH_SECS;
    }
    if world.get::<Stat>(meal).is_some_and(|s| *s == Stat::Dead) {
        crate::chem::tell_mob(
            world,
            e,
            Span::Info,
            "Жизненной энергии этого существа больше не хватает...",
        );
        let human_player = world.get::<PlayerMob>(meal).is_some()
            && world.get::<Health>(meal).is_some_and(|h| h.basic.is_none());
        let rabid = world.get::<Slime>(e).is_some_and(|s| s.rabid);
        if human_player
            && !rabid
            && world.resource_mut::<CombatRng>().chance(60.0)
            && let Some(mut s) = world.get_mut::<Slime>(e)
        {
            s.rabid = true;
        }
        stop_feeding(world, e, false);
        return;
    }
    let (brute, tox) = {
        let mut rng = world.resource_mut::<CombatRng>();
        (
            rng.range(2, 4) as f32 * 0.5 * LEECH_SECS as f32,
            rng.range(1, 2) as f32 * 0.5 * LEECH_SECS as f32,
        )
    };
    let taken =
        damage(world, meal, DamageType::Brute, brute) + damage(world, meal, DamageType::Tox, tox);
    if taken <= 0.0 {
        crate::chem::tell_mob(world, e, Span::Warning, "Это не еда!");
        stop_feeding(world, e, false);
        return;
    }
    if world.get::<PlayerMob>(meal).is_some() && world.resource_mut::<CombatRng>().chance(5.0) {
        const PAIN: [&str; 4] = [
            "Вы чувствуете, как ваше тело слабеет!",
            "Кажется, вы вот-вот умрёте!",
            "Каждая часть вашего тела кричит от боли!",
            "Вы чувствуете себя крайне слабым!",
        ];
        let i = world
            .resource_mut::<CombatRng>()
            .range(0, PAIN.len() as u32 - 1) as usize;
        crate::chem::tell_mob(world, meal, Span::BoldWarning, PAIN[i]);
    }
    if let Some(mut s) = world.get_mut::<Slime>(e) {
        s.adjust_nutrition(1.8 * taken);
    }
    damage(world, e, DamageType::Brute, -1.5 * LEECH_SECS as f32);
}

// --- Extracts -----------------------------------------------------------------------------

/// `/obj/item/slime_extract/Initialize`: `create_reagents(100, INJECTABLE | DRAWABLE |
/// SEALED_CONTAINER)`, one use.
fn on_extract_added(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    let Ok(item) = items.get(add.entity) else {
        return;
    };
    if !is_under(&item.path, EXTRACT) {
        return;
    }
    let owner = Owner {
        path: item.path.clone(),
        living: false,
    };
    commands.entity(add.entity).try_insert((
        Reagents(Holder::new(
            100.0,
            flags::INJECTABLE | flags::DRAWABLE | flags::SEALED_CONTAINER,
            owner,
        )),
        SlimeExtract {
            uses: 1,
            timer: None,
            delete_at: None,
        },
    ));
}

/// The effect of a slime reaction (its `on_reaction` beyond `use_slime_core`).
enum Effect {
    Items(&'static [&'static str]),
    Stack(&'static str, u32),
    Stacks(&'static [(&'static str, u32)]),
    /// One random path of the list.
    Pick(&'static [&'static str]),
    SpawnSlime(&'static str),
    RandomSlime,
    Emp(i32, i32),
    Enrage,
    Later(Delayed),
    Nothing,
}

/// `on_reaction` of `/datum/chemical_reaction/slime/<name>`.
fn effect_of(name: &str) -> Effect {
    use Effect::*;
    match name {
        "slimespawn" => SpawnSlime("grey"),
        "slimemonkey" => Items(&[
            "/obj/item/food/monkeycube",
            "/obj/item/food/monkeycube",
            "/obj/item/food/monkeycube",
        ]),
        "slimemetal" => Stacks(&[
            ("/obj/item/stack/sheet/plasteel", 5),
            ("/obj/item/stack/sheet/iron", 15),
        ]),
        "slimeglass" => Stacks(&[
            ("/obj/item/stack/sheet/rglass", 5),
            ("/obj/item/stack/sheet/glass", 15),
        ]),
        "slimemobspawn"
        | "slimemobspawn/lesser"
        | "slimemobspawn/friendly"
        | "slimemobspawn/spider" => Later(Delayed::Nothing),
        "slimestabilizer" => Items(&["/obj/item/slimepotion/slime/stabilizer"]),
        "slimefreeze" => Later(Delayed::Gas("n2=50;TEMP=2.7")),
        "slimefireproof" => Items(&["/obj/item/slimepotion/fireproof"]),
        "slimefire" => Later(Delayed::Gas("plasma=50;TEMP=1000")),
        "slimeoverload" => Emp(3, 5),
        "slimecell" => Items(&["/obj/item/stock_parts/power_store/cell/emproof/slime"]),
        "slimeglow" => Items(&["/obj/item/flashlight/slime"]),
        "slimepsteroid" => Items(&["/obj/item/slimepotion/slime/steroid"]),
        "slimeplasma" => Stack("/obj/item/stack/sheet/mineral/plasma", 3),
        "slimemutator" => Items(&["/obj/item/slimepotion/slime/mutator"]),
        "slimebloodlust" => Enrage,
        "slimespeed" => Items(&["/obj/item/slimepotion/speed"]),
        "docility" => Items(&["/obj/item/slimepotion/slime/docility"]),
        "gender" => Items(&["/obj/item/slimepotion/genderchange"]),
        "slimeexplosion" => Later(Delayed::Explosion(1, 2, 4)),
        "slimepotion2" => Items(&["/obj/item/slimepotion/sentience"]),
        "renaming" => Items(&["/obj/item/slimepotion/renaming"]),
        "adamantine" => Stack("/obj/item/stack/sheet/mineral/adamantine", 1),
        "slimefloor2" => Stack("/obj/item/stack/tile/bluespace", 25),
        "slimecrystal" => Stack("/obj/item/stack/ore/bluespace_crystal", 1),
        "slimeradio" => Items(&["/obj/item/slimepotion/slimeradio"]),
        "slimepsteroid2" => Items(&["/obj/item/slimepotion/enhancer"]),
        "slime_territory" => Items(&["/obj/item/blueprints/slime"]),
        "slimestop" => Later(Delayed::Nothing),
        "slimecamera" => Items(&["/obj/item/camera", "/obj/item/camera_film"]),
        "slimefloor" => Stack("/obj/item/stack/tile/sepia", 25),
        "slimepaint" => Pick(&[
            "/obj/item/paint/red",
            "/obj/item/paint/green",
            "/obj/item/paint/blue",
            "/obj/item/paint/yellow",
            "/obj/item/paint/violet",
            "/obj/item/paint/black",
            "/obj/item/paint/white",
            "/obj/item/paint/anycolor",
            "/obj/item/paint/paint_remover",
        ]),
        "slimecrayon" => Pick(&[
            "/obj/item/toy/crayon/red",
            "/obj/item/toy/crayon/orange",
            "/obj/item/toy/crayon/yellow",
            "/obj/item/toy/crayon/green",
            "/obj/item/toy/crayon/blue",
            "/obj/item/toy/crayon/purple",
            "/obj/item/toy/crayon/black",
            "/obj/item/toy/crayon/white",
            "/obj/item/toy/crayon/mime",
            "/obj/item/toy/crayon/rainbow",
        ]),
        "slime_rng" => RandomSlime,
        "slime_transfer" => Items(&["/obj/item/slimepotion/transference"]),
        _ => Nothing,
    }
}

/// Instant reactions ran in extract `e` (`crate::chem::refresh_container`): each slime one
/// uses the extract (`instant_react`: `extract_uses--`, "used slime extract" at 0) and does its
/// `on_reaction`; a spent extract stops reacting (`pre_reaction_other_checks`) and, without
/// reagent results, is deleted (`delete_extract`).
pub(crate) fn extract_reacted(
    world: &mut World,
    chem: &Chemistry,
    e: Entity,
    reacted: &[(u32, f32)],
) {
    for &(index, _) in reacted {
        let Some(reaction) = chem.reactions.get(index as usize) else {
            continue;
        };
        let Some(name) = reaction.def.path.strip_prefix(SLIME_REACTIONS) else {
            continue;
        };
        let has_results = !reaction.def.results.is_empty();
        let Some(pos) = map_pos(world, e) else {
            continue;
        };
        let spent = {
            let Some(mut x) = world.get_mut::<SlimeExtract>(e) else {
                return;
            };
            x.uses = x.uses.saturating_sub(1);
            x.uses == 0
        };
        if spent {
            let own = name_of(world, e);
            say_around(
                world,
                pos,
                Span::Notice,
                &format!("Сила {own} поглощается реакцией."),
            );
            set_name(world, e, "used slime extract");
            if let Some(mut n) = world.get_mut::<DmName>(e) {
                n.desc = "This extract has been used up.".to_owned();
            }
            if let Some(mut r) = world.get_mut::<Reagents>(e) {
                r.0.flags |= flags::NO_REACT;
            }
        }
        let mut deletes = true;
        match effect_of(name) {
            Effect::Items(paths) => {
                for p in paths {
                    crate::construction::spawn_obj(world, p, pos);
                }
            }
            Effect::Stack(p, n) => {
                crate::construction::spawn_stack(world, p, pos, n);
            }
            Effect::Stacks(list) => {
                for &(p, n) in list {
                    crate::construction::spawn_stack(world, p, pos, n);
                }
            }
            Effect::Pick(paths) => {
                let i = world
                    .resource_mut::<CombatRng>()
                    .range(0, paths.len() as u32 - 1) as usize;
                crate::construction::spawn_obj(world, paths[i], pos);
            }
            Effect::SpawnSlime(colour) => {
                spawn_slime(world, pos, colour, false);
                say_around(
                    world,
                    pos,
                    Span::Danger,
                    "Напитавшись плазмой, ядро начинает дрожать и расти, и из него появляется \
                     новый малыш-слайм!",
                );
            }
            Effect::RandomSlime => {
                // shortcut: 5+ units make a slime cluster grenade in tg; not ported, a slime
                // instead.
                let (colour, adult) = {
                    let mut rng = world.resource_mut::<CombatRng>();
                    (
                        SLIME_TYPES[rng.range(0, TYPE_COUNT as u32 - 1) as usize].colour,
                        rng.chance(50.0),
                    )
                };
                spawn_slime(world, pos, colour, adult);
            }
            Effect::Emp(heavy, light) => {
                world.write_message(crate::emp::Empulse {
                    epicenter: pos,
                    heavy,
                    light,
                });
            }
            Effect::Enrage => enrage_slimes(world, pos),
            Effect::Later(delayed) => {
                deletes = false;
                let text = match delayed {
                    Delayed::Gas(g) if g.starts_with("n2") => {
                        "Слаймовый экстракт становится очень холодным!"
                    }
                    _ => "Слаймовый экстракт начинает яростно вибрировать!",
                };
                say_around(world, pos, Span::Danger, text);
                let now = world.resource::<Time>().elapsed_secs_f64();
                if let Some(mut x) = world.get_mut::<SlimeExtract>(e) {
                    x.timer = Some((now + DELAYED_SECS, delayed));
                    x.delete_at = Some(now + DELAYED_DELETE_SECS);
                }
            }
            Effect::Nothing => {}
        }
        if deletes && spent && !has_results {
            world.despawn(e);
            return;
        }
    }
}

/// `slimebloodlust/on_reaction`: every slime in view becomes wild (a docile one) or rabid.
fn enrage_slimes(world: &mut World, pos: TilePos) {
    let slimes: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .in_radius(pos, 7)
        .filter(|&s| world.get::<Slime>(s).is_some())
        .collect();
    for e in slimes {
        let name = name_of(world, e);
        let Some(mut s) = world.get_mut::<Slime>(e) else {
            continue;
        };
        let text = if s.hunger_disabled {
            s.hunger_disabled = false;
            s.rabid = false;
            format!("{name} забывает свою дрессировку и вновь дичает!")
        } else {
            s.hunger_disabled = false;
            s.rabid = true;
            format!("{name} впадает в бешенство!")
        };
        if let Some(&at) = world.get::<TilePos>(e) {
            say_around(world, at, Span::Danger, &text);
        }
    }
}

/// The 5 s timers of extract reactions and the deletion of spent extracts after them.
fn extract_timers(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut q = world.query::<(Entity, &SlimeExtract)>();
    let due: Vec<Entity> = q
        .iter(world)
        .filter(|(_, x)| {
            x.timer.is_some_and(|(at, _)| at <= now) || x.delete_at.is_some_and(|at| at <= now)
        })
        .map(|(e, _)| e)
        .collect();
    for e in due {
        let Some(x) = world.get::<SlimeExtract>(e).cloned() else {
            continue;
        };
        let pos = map_pos(world, e);
        if let Some((at, delayed)) = x.timer
            && at <= now
        {
            if let Some(mut x) = world.get_mut::<SlimeExtract>(e) {
                x.timer = None;
            }
            match (delayed, pos) {
                (Delayed::Gas(gas), Some(pos)) => {
                    if let Some(mut atmos) = world.get_resource_mut::<ss13_atmos::Atmos>()
                        && atmos.spawn_air(pos, gas).is_err()
                    {
                        warn!("slime extract gas {gas:?} does not parse");
                    }
                }
                (Delayed::Explosion(d, h, l), Some(pos)) => {
                    crate::explosions::explosion(
                        world,
                        pos,
                        crate::explosions::Explosion::new(d, h, l).cause("slime extract"),
                    );
                }
                _ => {}
            }
        }
        if world.get_entity(e).is_err() {
            continue;
        }
        if let Some(at) = x.delete_at
            && at <= now
        {
            if let Some(mut x) = world.get_mut::<SlimeExtract>(e) {
                x.delete_at = None;
            }
            // `delete_extract`: spent and without reagent results.
            let spent = world.get::<SlimeExtract>(e).is_some_and(|x| x.uses == 0);
            if spent {
                world.despawn(e);
            }
        }
    }
}

// --- Items on slimes and extracts ---------------------------------------------------------

fn item_path(world: &World, e: Entity) -> Option<String> {
    world.get::<DmItem>(e).map(|i| i.path.clone())
}

/// Core removal, potions on slimes, the enhancer on extracts, extracts fed to adult slimes.
/// True when the click was theirs.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let Some(path) = item_path(world, held) else {
        return false;
    };
    if world.get::<Slime>(target).is_some() {
        return on_slime(world, client, mob, held, &path, target);
    }
    if world.get::<SlimeExtract>(target).is_some()
        && is_under(&path, "/obj/item/slimepotion/enhancer")
    {
        enhance(world, client, held, &path, target);
        return true;
    }
    false
}

fn on_slime(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    path: &str,
    target: Entity,
) -> bool {
    let Some(s) = world.get::<Slime>(target).cloned() else {
        return false;
    };
    let dead = world.get::<Stat>(target).is_none_or(|st| *st == Stat::Dead);
    let tool = world
        .get::<ss13_mapload::DmTool>(held)
        .map(|t| t.behaviour.clone())
        .unwrap_or_default();
    if dead && (tool == "hemostat" || tool == "crowbar") {
        if s.cores == 0 {
            tell(
                world,
                client,
                Span::Warning,
                format!("В {} не осталось ни одного ядра!", name_of(world, target)),
            );
            return true;
        }
        let actor = Actor {
            client,
            mob,
            item: Some(held),
        };
        if do_after(
            world,
            actor,
            target,
            CORE_REMOVAL_DS,
            0,
            crate::construction::Job::SlimeCores,
        ) {
            tell(
                world,
                client,
                Span::Notice,
                format!(
                    "Вы начинаете извлекать ядро из {}...",
                    name_of(world, target)
                ),
            );
        }
        return true;
    }
    if is_under(path, EXTRACT) {
        return crossbreed(world, client, held, path, target);
    }
    let potion = |p: &str| is_under(path, &format!("/obj/item/slimepotion/slime/{p}"));
    if !is_under(path, "/obj/item/slimepotion/slime") {
        return false;
    }
    if dead {
        tell(world, client, Span::Warning, "Слайм мёртв!");
        return true;
    }
    let text = if potion("docility") {
        let calmed = s.rabid;
        if let Some(mut slime) = world.get_mut::<Slime>(target) {
            if calmed {
                // Stops being rabid, but does not become truly docile.
                slime.rabid = false;
            } else {
                slime.hunger_disabled = true;
                slime.rabid = false;
                slime.nutrition = STARTING_NUTRITION;
                slime.named = true;
            }
        }
        if calmed {
            "Вы скармливаете слайму зелье, усмиряя его бешенство."
        } else {
            // shortcut: no naming prompt, tg's default name.
            set_name(world, target, "Pet Slime");
            "Вы скармливаете слайму зелье, избавляя его от голода и успокаивая."
        }
    } else if potion("steroid") {
        if s.adult {
            tell(
                world,
                client,
                Span::Warning,
                "Стероид действует только на малышей-слаймов!",
            );
            return true;
        }
        if s.cores >= 5 {
            tell(
                world,
                client,
                Span::Warning,
                "У слайма уже максимум экстракта!",
            );
            return true;
        }
        if let Some(mut slime) = world.get_mut::<Slime>(target) {
            slime.cores += 1;
        }
        "Вы скармливаете слайму стероид. Теперь он даст на один экстракт больше."
    } else if potion("stabilizer") {
        if s.mutation_chance == 0.0 {
            tell(
                world,
                client,
                Span::Warning,
                "У слайма и так нет шанса мутировать!",
            );
            return true;
        }
        if let Some(mut slime) = world.get_mut::<Slime>(target) {
            slime.mutation_chance = (slime.mutation_chance - 15.0).clamp(0.0, 100.0);
        }
        "Вы скармливаете слайму стабилизатор. Теперь он реже мутирует."
    } else if potion("mutator") {
        if s.mutator_used {
            tell(
                world,
                client,
                Span::Warning,
                "Этот слайм уже принял мутатор, ещё один сделает его слишком нестабильным!",
            );
            return true;
        }
        if s.mutation_chance >= 100.0 {
            tell(
                world,
                client,
                Span::Warning,
                "Слайм и так непременно мутирует!",
            );
            return true;
        }
        if let Some(mut slime) = world.get_mut::<Slime>(target) {
            slime.mutation_chance = (slime.mutation_chance + 12.0).clamp(0.0, 100.0);
            slime.mutator_used = true;
        }
        "Вы скармливаете слайму мутатор. Теперь он чаще мутирует."
    } else {
        return false;
    };
    tell(world, client, Span::Notice, text);
    world.despawn(held);
    true
}

/// `/obj/item/slime_extract/item_interaction` with an enhancer: one more use (five with the
/// maximizer), at most five.
fn enhance(world: &mut World, client: Entity, held: Entity, path: &str, target: Entity) {
    let uses = world.get::<SlimeExtract>(target).map_or(0, |x| x.uses);
    if uses >= 5 {
        tell(
            world,
            client,
            Span::Warning,
            "Этот экстракт уже нельзя улучшить!",
        );
        return;
    }
    let max = is_under(path, "/obj/item/slimepotion/enhancer/max");
    if let Some(mut x) = world.get_mut::<SlimeExtract>(target) {
        x.uses = if max { 5 } else { x.uses + 1 };
    }
    if let Some(mut r) = world.get_mut::<Reagents>(target) {
        r.0.flags &= !flags::NO_REACT;
    }
    let text = if max {
        "Вы выливаете максимизатор на слаймовый экстракт. Теперь его можно использовать 5 раз!"
    } else {
        "Вы применяете усилитель к слаймовому экстракту. Его можно использовать ещё один раз."
    };
    tell(world, client, Span::Notice, text);
    world.despawn(held);
}

/// `/obj/item/slime_extract/interact_with_atom` on a slime: core-crossing.
fn crossbreed(world: &mut World, client: Entity, held: Entity, path: &str, target: Entity) -> bool {
    let Some(s) = world.get::<Slime>(target).cloned() else {
        return false;
    };
    if world.get::<Stat>(target).is_none_or(|st| *st == Stat::Dead) {
        tell(world, client, Span::Warning, "Слайм мёртв!");
        return true;
    }
    if !s.adult {
        tell(
            world,
            client,
            Span::Warning,
            "Чтобы скрестить ядро, слайм должен быть взрослым!",
        );
        return true;
    }
    let Some(modification) = extract_colour(path).map(|c| slime_type(c).crossbreed) else {
        return false;
    };
    if s.crossbreed.is_some_and(|m| m != modification) {
        tell(
            world,
            client,
            Span::Warning,
            "Слайм уже скрещивается с другим экстрактом!",
        );
        return true;
    }
    let amount = s.crossbreed_amount + 1;
    if let Some(mut slime) = world.get_mut::<Slime>(target) {
        slime.crossbreed = Some(modification);
        slime.crossbreed_amount = amount;
    }
    world.despawn(held);
    let text = if amount == 1 {
        "Вы скармливаете слайму экстракт, начиная мутацию его ядра."
    } else {
        "Вы скармливаете слайму экстракт, продолжая мутацию его ядра."
    };
    tell(world, client, Span::Notice, text);
    if let Some(pos) = world.get::<TilePos>(target).copied() {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, "sound/effects/blob/attackblob.ogg", 50.0, true),
        );
    }
    if amount >= CROSSING_REQUIRED {
        spawn_corecross(world, target);
    }
    true
}

/// `spawn_corecross()`: the cross-extract of the slime's colour and family, or a puddle.
fn spawn_corecross(world: &mut World, e: Entity) {
    let (Some(s), Some(&pos)) = (world.get::<Slime>(e).cloned(), world.get::<TilePos>(e)) else {
        return;
    };
    let name = name_of(world, e);
    say_around(
        world,
        pos,
        Span::Danger,
        &format!("{name} содрогается: его мутировавшее ядро поглощает остатки тела!"),
    );
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(pos, "sound/effects/magic/smoke.ogg", 50.0, true),
    );
    let family = s.crossbreed.unwrap_or_default().replace('-', "");
    let colour = s.colour.replace('-', "");
    let path = format!("/obj/item/slimecross/{family}/{colour}");
    if crate::construction::spawn_obj(world, &path, pos).is_none() {
        say_around(
            world,
            pos,
            Span::Warning,
            "Мутировавшее ядро содрогается и растекается лужицей, не в силах удержать форму.",
        );
    }
    stop_feeding(world, e, true);
    world.despawn(e);
}

/// The core removal operation's end (`core_removal/on_success` -> `try_extract_cores`).
pub fn complete_core_removal(world: &mut World, action: &DoAfter) {
    let e = action.target;
    let dead = world.get::<Stat>(e).is_some_and(|s| *s == Stat::Dead);
    let (Some(s), Some(&pos)) = (world.get::<Slime>(e).cloned(), world.get::<TilePos>(e)) else {
        return;
    };
    let name = name_of(world, e);
    if !dead || s.cores == 0 {
        tell(
            world,
            action.client,
            Span::Warning,
            format!("В {name} не осталось ни одного ядра!"),
        );
        return;
    }
    let extract = extract_path(s.colour);
    for _ in 0..s.cores {
        crate::construction::spawn_obj(world, &extract, pos);
    }
    if let Some(mut slime) = world.get_mut::<Slime>(e) {
        slime.cores = 0;
    }
    refresh(world, e, false);
    tell(
        world,
        action.client,
        Span::Notice,
        format!("Вы успешно извлекли ядро из {name}."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mutations_follow_the_type_table() {
        let mut rng = CombatRng(7);
        assert_eq!(random_mutation(&mut rng, "grey", 100.0), "rainbow");
        for _ in 0..50 {
            assert_eq!(random_mutation(&mut rng, "grey", 0.0), "grey");
            let m = random_mutation(&mut rng, "grey", 99.9);
            assert!(["blue", "metal", "orange", "purple", "grey"].contains(&m));
        }
        assert_eq!(
            extract_path("dark-purple"),
            "/obj/item/slime_extract/darkpurple"
        );
        assert_eq!(
            extract_colour("/obj/item/slime_extract/lightpink"),
            Some("light-pink")
        );
        // Every mutation names a type of the table.
        for t in &SLIME_TYPES[..TYPE_COUNT] {
            for (m, _) in t.mutations {
                assert_eq!(slime_type(m).colour, *m);
            }
        }
    }

    #[test]
    fn a_fed_slime_grows_and_a_starving_one_is_hungry() {
        let mut rng = CombatRng(1);
        let mut s = Slime {
            nutrition: MAX_NUTRITION,
            ..Slime::default()
        };
        for _ in 0..10 {
            handle_nutrition(&mut s, &mut rng, 2.0);
        }
        assert_eq!(s.amount_grown, EVOLUTION_THRESHOLD);
        assert!(s.nutrition < MAX_NUTRITION);
        assert!(!s.hungry);
        let mut starving = Slime {
            nutrition: 5.0,
            ..Slime::default()
        };
        handle_nutrition(&mut starving, &mut rng, 2.0);
        assert!(starving.hungry);
        assert_eq!(starving.amount_grown, 0);
        let mut pet = Slime {
            hunger_disabled: true,
            nutrition: 3.0,
            ..Slime::default()
        };
        handle_nutrition(&mut pet, &mut rng, 2.0);
        assert_eq!((pet.nutrition, pet.hungry), (100.0, false));
    }
}

// --- Bluespace and sepia floors -------------------------------------------------------------

/// `/turf/open/floor/bluespace`, `/turf/open/floor/sepia` and their `slowdown` (deciseconds).
pub const FLOOR_SLOWDOWNS: [(&str, f32); 2] = [
    ("/turf/open/floor/bluespace", -1.0),
    ("/turf/open/floor/sepia", 2.0),
];

/// On a mob: `/datum/movespeed_modifier/turf_slowdown` of the turf it stands on, deciseconds
/// per tile (negative: faster); `crate::health` adds it to the step delay.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct TurfSlowdown(pub f32);

/// The `slowdown` of the turf at `pos`.
// shortcut: only the two xenobiology floors; other turfs' `slowdown` (lava, snow, water) is not
// carried by the converter yet.
pub fn turf_slowdown(world: &World, pos: TilePos) -> f32 {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .find_map(|&e| {
            world.get::<ss13_mapload::Turf>(e)?;
            let path = &world.get::<ss13_mapload::DmTurfBuild>(e)?.path;
            FLOOR_SLOWDOWNS
                .iter()
                .find(|(floor, _)| is_under(path, floor))
                .map(|&(_, slowdown)| slowdown)
        })
        .unwrap_or(0.0)
}

/// `update_turf_movespeed(loc)` after a living mob moved.
// shortcut: flying and floating mobs are slowed too (tg skips `FLYING`/`FLOATING` movement
// types and `TRAIT_TURF_IGNORE_SLOWDOWN`); a tile laid under a standing mob counts from its
// next step.
// Cost: per tick, one tile lookup per living mob that moved.
fn update_turf_slowdown(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<ss13_world::Moved>>,
) {
    let moved: Vec<ss13_world::Moved> = cursor
        .read(world.resource::<bevy::ecs::message::Messages<ss13_world::Moved>>())
        .copied()
        .collect();
    for m in moved {
        if world.get::<Health>(m.entity).is_none() || world.get::<TilePos>(m.entity) != Some(&m.to)
        {
            continue;
        }
        let slowdown = turf_slowdown(world, m.to);
        let old = world.get::<TurfSlowdown>(m.entity).map_or(0.0, |t| t.0);
        if slowdown != old {
            world.entity_mut(m.entity).insert(TurfSlowdown(slowdown));
        }
    }
}
