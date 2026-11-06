//! Icemoon fauna (server), after tg `code/modules/mob/living/basic/icemoon/`: they are
//! `/mob/living/basic/mining` mobs ([`super::MiningMob`]); their trees are in
//! `crate::mobs::ai::trees::lavaland`, the leaves in [`super::ai`].
//!
//! - Polar bear: fights back and attacks whoever comes within 2 tiles; at half health or less
//!   it gets enraged (`ai_behavior/enrage`: its melee cooldown halved, restored once it is
//!   above half again).
//! - White wolf: flees when hurt to half health (`ai_flee_while_injured`), bites otherwise and
//!   howls for its pack every 30 s (`call_reinforcements`: the target goes into the retaliate
//!   list of every mob of its faction with an AI within 15 tiles).
//! - Ice whelp: `fire_breath/ice` (6 s, 7 tiles, 0.6 s forecast holding still, a tile every
//!   0.085 s: 10 burn and frozen solid for 2 s) and `fire_breath/ice/eruption` (6 s, 1 s
//!   forecast, then rings of cold fire out to 3 tiles 0.15 s apart). The ash whelp breathes real
//!   fire: the plain `fire_breath` (3 s, 15 tiles, 20 burn and set alight, no forecast) and a
//!   burning eruption.
//! - Demonic watcher (`ice_demon`): shoots ice blasts (5 burn) from 4-7 tiles; teleports
//!   (`ice_demon_teleport`, 3 s, after 0.5 s) to the tile farthest from a target next to it, or
//!   else next to its target; under 75 health it conjures two afterimages (60 s, at most two
//!   alive: 20 health, 15 s, the damage they take is dealt to the demon, `joint_damage`). It
//!   leaves 3 bluespace crystals and, 5% of the time, a raw bluespace anomaly core.
// shortcut: no wolf taming, riding or pet commands, no whelp sculpting of icy rocks, eating dead
// whelps or burning trees, no ice demon fleeing from lit welders and flares nor its slippery
// floors (the only user of `slippery_ice_floors`), no mobs' crusher trophies or butchering;
// no breath sounds or teleport shrink animation; the ice blast does not cool the body.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_world::SpatialIndex;

use super::abilities::{alive, blocked, capitalize, fleeting_visual, living_on, name, now, tell};
use crate::combat::CombatRng;
use crate::health::{DamageType, Health, Stat};
use crate::mobs::BasicMob;
use crate::mobs::ai::{AiController, Key, Value};
use crate::status_effects::Incap;

pub const POLAR_BEAR: &str = "/mob/living/basic/mining/polarbear";
pub const WOLF: &str = "/mob/living/basic/mining/wolf";
pub const ICE_WHELP: &str = "/mob/living/basic/mining/ice_whelp";
pub const ASH_WHELP: &str = "/mob/living/basic/mining/ice_whelp/ash";
pub const ICE_DEMON: &str = "/mob/living/basic/mining/ice_demon";
pub const AFTERIMAGE: &str = "/mob/living/basic/mining/demon_afterimage";
const AFTERIMAGE_CRUSHER: &str = "/mob/living/basic/mining/demon_afterimage/crusher";
const FIRE_DMI: &str = "icons/effects/fire.dmi";
const BLUESPACE_CRYSTAL: &str = "/obj/item/stack/ore/bluespace_crystal";
const ANOMALY_CORE: &str = "/obj/item/raw_anomaly_core/bluespace";

/// `ai_behavior/enrage` `health_threshold`.
const ENRAGE_THRESHOLD: f32 = 0.5;
/// `call_reinforcements` `reinforcements_range`; the wolf tree's cooldown decorator.
const REINFORCEMENTS_RANGE: u32 = 15;
const REINFORCEMENTS_COOLDOWN: f64 = 30.0;
/// The wolf's `BB_REINFORCEMENTS_EMOTE`.
const HOWL: &str = "издаёт леденящий вой, призывая на помощь!";
/// `fire_breath/ice`: `cooldown_time`, `fire_range`, `fire_damage`, `fire_delay`,
/// `forecast_delay`; the `ice_block_talisman` it applies.
const ICE_COOLDOWN: f64 = 6.0;
const ICE_RANGE: i32 = 7;
const ICE_DAMAGE: f32 = 10.0;
const ICE_DELAY: f64 = 0.085;
const ICE_FORECAST: f64 = 0.6;
const ICE_BLOCK: f32 = 2.0;
/// The plain `fire_breath` of the ash whelp, with `DRAKE_FIRE_TEMP` and `DRAKE_FIRE_EXPOSURE`.
const FIRE_COOLDOWN: f64 = 3.0;
const FIRE_RANGE: i32 = 15;
const FIRE_DAMAGE: f32 = 20.0;
const FIRE_DELAY: f64 = 0.15;
const FIRE_TEMP: f64 = 500.0;
const FIRE_VOLUME: f64 = 50.0;
/// `fire_breath/ice/eruption`: `cooldown_time`, `fire_range`, `forecast_delay`, `fire_delay`
/// (its damage is the ice breath's).
const ERUPTION_COOLDOWN: f64 = 6.0;
pub const ERUPTION_RANGE: i32 = 3;
const ERUPTION_FORECAST: f64 = 1.0;
const ERUPTION_DELAY: f64 = 0.15;
/// `ice_demon_teleport`: `cooldown_time`, `time_delay`; the tree's search ranges.
const TELEPORT_COOLDOWN: f64 = 3.0;
const TELEPORT_DELAY: f64 = 0.5;
pub const ESCAPE_RANGE: u32 = 7;
pub const NEAR_RANGE: u32 = 3;
/// `create_afterimages`: `cooldown_time`, `summon_amount`, `max_summons`; the tree's
/// `pawn_health_below`; the afterimage's `existence_period` (the crusher's 7 s).
const CLONE_COOLDOWN: f64 = 60.0;
const CLONE_AMOUNT: usize = 2;
const MAX_CLONES: usize = 2;
pub const CLONE_BELOW_HEALTH: f32 = 75.0;
const AFTERIMAGE_LIFE: f64 = 15.0;
const AFTERIMAGE_CRUSHER_LIFE: f64 = 7.0;

/// A polar bear's `BB_BASIC_MOB_ENRAGE`: the melee cooldown (deciseconds) before it halved.
#[derive(Component, Clone, Copy, Debug)]
pub struct Enraged(pub f32);

/// A wolf's `BB_BASIC_MOB_REINFORCEMENTS_COOLDOWN`.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Howl {
    pub ready_at: f64,
}

/// A whelp's two breaths and the one under way.
#[derive(Component, Clone, Debug)]
pub struct WhelpBreath {
    pub line_ready: f64,
    pub wide_ready: f64,
    /// The ash whelp: real fire, the plain `fire_breath` for the line.
    ash: bool,
    phase: Breath,
}

#[derive(Clone, Debug, PartialEq)]
enum Breath {
    Idle,
    /// `attack_forecast`: the `do_after` on itself before breathing.
    Forecast {
        until: f64,
        wide: bool,
        toward: TilePos,
    },
    /// `progressive_fire_line`: the tiles still to burn (next last).
    Line {
        next: f64,
        tiles: Vec<TilePos>,
        hit: Vec<Entity>,
    },
    /// `eruption/breath_attack`: ring `ring` burns at `next`.
    Rings {
        next: f64,
        ring: i32,
        center: TilePos,
        hit: Vec<Entity>,
    },
}

/// An ice demon's teleport and afterimages.
#[derive(Component, Clone, Debug, Default)]
pub struct DemonPowers {
    pub teleport_ready: f64,
    pub clone_ready: f64,
    /// `number_of_summons`: the afterimages alive.
    pub clones: Vec<Entity>,
    /// `teleport_to_turf` due.
    jump: Option<(f64, TilePos)>,
}

/// A demon's afterimage: `joint_damage` to `overlord`, `fade_into_nothing` at `fades_at`.
#[derive(Component, Clone, Copy, Debug)]
pub struct Afterimage {
    pub overlord: Option<Entity>,
    last_health: f32,
    fades_at: f64,
}

pub(super) fn build(app: &mut App) {
    app.add_systems(
        Update,
        (breaths, jumps, afterimages).before(crate::health::HealthSystems),
    );
}

pub(super) fn attach(world: &mut World, mob: Entity, path: &str) {
    let now = now(world);
    let under = |root: &str| is_under(path, root);
    let mut e = world.entity_mut(mob);
    if under(WOLF) {
        e.insert(Howl::default());
    } else if under(ICE_WHELP) {
        e.insert(WhelpBreath {
            line_ready: 0.0,
            wide_ready: 0.0,
            ash: under(ASH_WHELP),
            phase: Breath::Idle,
        });
    } else if under(ICE_DEMON) {
        e.insert(DemonPowers::default());
    } else if under(AFTERIMAGE) {
        let life = if under(AFTERIMAGE_CRUSHER) {
            AFTERIMAGE_CRUSHER_LIFE
        } else {
            AFTERIMAGE_LIFE
        };
        let health = e.get::<Health>().map_or(0.0, Health::health);
        e.insert(Afterimage {
            overlord: None,
            last_health: health,
            fades_at: now + life,
        });
    }
}

/// `death_drops` and `ice_demon/death`.
pub(super) fn on_death(world: &mut World, path: &str, pos: TilePos) {
    if !is_under(path, ICE_DEMON) {
        return;
    }
    let mut rng = CombatRng(world.resource::<CombatRng>().0);
    let core = rng.chance(5.0);
    world.resource_mut::<CombatRng>().0 = rng.0;
    if core {
        crate::construction::spawn_obj(world, ANOMALY_CORE, pos);
    }
    for _ in 0..3 {
        crate::construction::spawn_stack(world, BLUESPACE_CRYSTAL, pos, 1);
    }
}

/// A breath forecast holds the whelp still (its `do_after`).
pub(super) fn channeling(world: &World, mob: Entity) -> bool {
    world
        .get::<WhelpBreath>(mob)
        .is_some_and(|b| matches!(b.phase, Breath::Forecast { .. }))
}

// ---------------------------------------------------------------------------------------------
// Polar bear

/// `ai_behavior/enrage/perform`: true when the rage started or ended.
pub fn enrage(world: &mut World, mob: Entity, target: Option<Entity>) -> bool {
    let Some(health) = world.get::<Health>(mob) else {
        return false;
    };
    let low = health.health() / health.max <= ENRAGE_THRESHOLD;
    let enraged = world.get::<Enraged>(mob).copied();
    match (low, enraged) {
        (true, None) => {
            let Some(mut basic) = world.get_mut::<BasicMob>(mob) else {
                return false;
            };
            let def = std::sync::Arc::make_mut(&mut basic.def);
            let previous = def.melee_attack_cooldown;
            def.melee_attack_cooldown = previous / 2.0;
            world.entity_mut(mob).insert(Enraged(previous));
            let user = capitalize(&name(world, mob));
            let text = match target.filter(|&t| world.get_entity(t).is_ok()) {
                Some(t) => format!("{user} в ярости смотрит на {}!", name(world, t)),
                None => format!("{user} приходит в ярость!"),
            };
            if let Some(&at) = world.get::<TilePos>(mob) {
                super::abilities::visible(world, at, Span::Danger, text);
            }
            true
        }
        (false, Some(Enraged(previous))) => {
            if let Some(mut basic) = world.get_mut::<BasicMob>(mob) {
                std::sync::Arc::make_mut(&mut basic.def).melee_attack_cooldown = previous;
            }
            world.entity_mut(mob).remove::<Enraged>();
            true
        }
        _ => false,
    }
}

// ---------------------------------------------------------------------------------------------
// Wolf

/// `call_reinforcements` behind the wolf tree's 30 s cooldown: the howl, and `target` on the
/// retaliate list of every same-faction mob with an AI in view.
pub fn call_reinforcements(world: &mut World, mob: Entity, target: Entity, now: f64) -> bool {
    if world.get::<Howl>(mob).is_none_or(|h| now < h.ready_at)
        || world.get_entity(target).is_err()
        || crate::mobs::pets::allied(world, mob, target)
    {
        return false;
    }
    let Some(&from) = world.get::<TilePos>(mob) else {
        return false;
    };
    if let Some(mut howl) = world.get_mut::<Howl>(mob) {
        howl.ready_at = now + REINFORCEMENTS_COOLDOWN;
    }
    crate::mobs::defense::emote(world, mob, HOWL);
    let mut q = world.query::<(Entity, &TilePos, &AiController)>();
    let others: Vec<(Entity, TilePos)> = q
        .iter(world)
        .filter(|(e, _, _)| *e != mob)
        .map(|(e, p, _)| (e, *p))
        .collect();
    for (other, at) in others {
        let near = from
            .chebyshev(at)
            .is_some_and(|d| d <= REINFORCEMENTS_RANGE);
        if !near
            || !crate::mobs::same_faction(world, mob, other)
            || !crate::mobs::ai::behaviors::can_see(world, from, at, REINFORCEMENTS_RANGE)
        {
            continue;
        }
        let Some(mut controller) = world.get_mut::<AiController>(other) else {
            continue;
        };
        let mut list = match controller.bb.get(Key::RetaliateList) {
            Some(Value::Attackers(list)) => list.clone(),
            _ => Vec::new(),
        };
        list.retain(|(e, _)| *e != target);
        list.push((target, now));
        controller
            .bb
            .set(Key::RetaliateList, Value::Attackers(list));
    }
    true
}

// ---------------------------------------------------------------------------------------------
// Whelps

/// The whelp's breath (`wide`: the eruption) ready.
pub fn breath_ready(world: &World, whelp: Entity, wide: bool, now: f64) -> bool {
    world.get::<WhelpBreath>(whelp).is_some_and(|b| {
        b.phase == Breath::Idle && now >= if wide { b.wide_ready } else { b.line_ready }
    })
}

/// `fire_breath/Activate` at `target`: the cooldown starts, the forecast (or, for the ash
/// whelp's plain line, the fire) begins.
pub fn start_breath(world: &mut World, whelp: Entity, target: Entity, wide: bool) -> bool {
    let now = now(world);
    let Some(&toward) = world.get::<TilePos>(target) else {
        return false;
    };
    if !breath_ready(world, whelp, wide, now) || !alive(world, whelp) {
        return false;
    }
    let ash = world.get::<WhelpBreath>(whelp).is_some_and(|b| b.ash);
    let plain_line = ash && !wide;
    let phase = if plain_line {
        line_phase(world, whelp, toward, FIRE_RANGE, now)
    } else {
        let until = now
            + if wide {
                ERUPTION_FORECAST
            } else {
                ICE_FORECAST
            };
        Breath::Forecast {
            until,
            wide,
            toward,
        }
    };
    if let Some(mut b) = world.get_mut::<WhelpBreath>(whelp) {
        if wide {
            b.wide_ready = now + ERUPTION_COOLDOWN;
        } else {
            b.line_ready = now + if ash { FIRE_COOLDOWN } else { ICE_COOLDOWN };
        }
        b.phase = phase;
    }
    true
}

/// `fire_line`: the turfs from the whelp (excluded) towards `toward`, `range` tiles out.
fn line_phase(world: &World, whelp: Entity, toward: TilePos, range: i32, now: f64) -> Breath {
    let Some(&from) = world.get::<TilePos>(whelp) else {
        return Breath::Idle;
    };
    let index = world.resource::<SpatialIndex>();
    let end = crate::mobs::megafauna::drake::ranged_target_turf(
        |p| index.in_bounds(p),
        from,
        toward,
        range,
        0.0,
    );
    let mut tiles: Vec<TilePos> = crate::mobs::megafauna::drake::get_line(from, end)
        .into_iter()
        .filter(|&p| p != from)
        .collect();
    tiles.reverse();
    Breath::Line {
        next: now,
        tiles,
        hit: vec![whelp],
    }
}

/// The open tiles of an eruption around `center` by ring (`circle_range`, chebyshev rings 1 to
/// `range`).
pub fn eruption_ring(center: TilePos, range: i32, ring: i32) -> Vec<TilePos> {
    // BYOND `circle_range`: dx² + dy² <= r * (r + 0.5).
    let rsq = f64::from(range) * (f64::from(range) + 0.5);
    let mut out = Vec::new();
    for dy in -range..=range {
        for dx in -range..=range {
            if f64::from(dx * dx + dy * dy) > rsq || dx.abs().max(dy.abs()) != ring {
                continue;
            }
            out.push(TilePos::new(center.x + dx, center.y + dy, center.z));
        }
    }
    out
}

/// `is_blocked_turf(exclude_mobs = TRUE)`: out of the map, or a dense turf or object.
fn breath_blocked(world: &World, pos: TilePos) -> bool {
    let index = world.resource::<SpatialIndex>();
    !index.in_bounds(pos)
        || !super::abilities::open_ground(world, pos)
        || index
            .dense_on_tile(pos)
            .iter()
            .any(|&e| world.get::<Health>(e).is_none())
}

/// `burn_turf` + `on_burn_mob`: ice freezes (`ice_block_talisman`), the ash whelp's fire burns
/// the air and sets alight.
fn burn_turf(
    world: &mut World,
    whelp: Entity,
    pos: TilePos,
    hit: &mut Vec<Entity>,
    ash: bool,
    damage: f32,
) {
    fleeting_visual(world, pos, FIRE_DMI, "light", 1.0);
    if ash && let Some(mut atmos) = world.get_resource_mut::<ss13_atmos::Atmos>() {
        atmos.hotspot_expose(pos, FIRE_TEMP, FIRE_VOLUME, true);
    }
    let source = name(world, whelp);
    for victim in living_on(world, pos, None) {
        if hit.contains(&victim) {
            continue;
        }
        hit.push(victim);
        if ash {
            let text = format!("Вас подожгло огненное дыхание {source}!");
            tell(world, victim, Span::Danger, text);
            crate::fire::fire_act(world, victim, FIRE_TEMP);
        } else {
            crate::status_effects::apply(world, victim, Incap::Immobilize, ICE_BLOCK);
            let text = format!("Вы заморожены ледяным дыханием {source}!");
            tell(world, victim, Span::Danger, text);
        }
        super::hurt(world, victim, DamageType::Burn, damage, None);
    }
}

fn breaths(world: &mut World) {
    let now = now(world);
    let due: Vec<(Entity, WhelpBreath)> = world
        .query::<(Entity, &WhelpBreath)>()
        .iter(world)
        .filter(|(_, b)| match b.phase {
            Breath::Idle => false,
            Breath::Forecast { until, .. } => now >= until,
            Breath::Line { next, .. } | Breath::Rings { next, .. } => now >= next,
        })
        .map(|(e, b)| (e, b.clone()))
        .collect();
    for (whelp, breath) in due {
        let next = if !alive(world, whelp) {
            // `SLEEP_CHECK_DEATH` / the dead `do_after`.
            Breath::Idle
        } else {
            step_breath(world, whelp, breath.ash, breath.phase, now)
        };
        if let Some(mut b) = world.get_mut::<WhelpBreath>(whelp) {
            b.phase = next;
        }
    }
}

fn step_breath(world: &mut World, whelp: Entity, ash: bool, phase: Breath, now: f64) -> Breath {
    match phase {
        Breath::Idle => Breath::Idle,
        Breath::Forecast { wide, toward, .. } => {
            if wide {
                let Some(&center) = world.get::<TilePos>(whelp) else {
                    return Breath::Idle;
                };
                Breath::Rings {
                    next: now,
                    ring: 1,
                    center,
                    hit: vec![whelp],
                }
            } else {
                line_phase(world, whelp, toward, ICE_RANGE, now)
            }
        }
        Breath::Line {
            mut tiles, mut hit, ..
        } => {
            let Some(pos) = tiles.pop() else {
                return Breath::Idle;
            };
            if breath_blocked(world, pos) {
                return Breath::Idle;
            }
            // The ash whelp's line is the plain `fire_breath` (20).
            let damage = if ash { FIRE_DAMAGE } else { ICE_DAMAGE };
            burn_turf(world, whelp, pos, &mut hit, ash, damage);
            if tiles.is_empty() {
                return Breath::Idle;
            }
            let delay = if ash { FIRE_DELAY } else { ICE_DELAY };
            Breath::Line {
                next: now + delay,
                tiles,
                hit,
            }
        }
        Breath::Rings {
            ring,
            center,
            mut hit,
            ..
        } => {
            for pos in eruption_ring(center, ERUPTION_RANGE, ring) {
                if !breath_blocked(world, pos) {
                    burn_turf(world, whelp, pos, &mut hit, ash, ICE_DAMAGE);
                }
            }
            if ring >= ERUPTION_RANGE {
                return Breath::Idle;
            }
            Breath::Rings {
                next: now + ERUPTION_DELAY,
                ring: ring + 1,
                center,
                hit,
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Ice demon

pub fn teleport_ready(world: &World, demon: Entity, now: f64) -> bool {
    world
        .get::<DemonPowers>(demon)
        .is_some_and(|d| d.jump.is_none() && now >= d.teleport_ready)
}

/// `find_furthest_turf_from_target` (`escape`, 7) or `find_valid_teleport_location` (3): the
/// open, unblocked tile around `target` in its view to jump to.
pub fn teleport_spot(
    world: &World,
    target: TilePos,
    escape: bool,
    rng: &mut CombatRng,
) -> Option<TilePos> {
    let range = (if escape { ESCAPE_RANGE } else { NEAR_RANGE }) as i32;
    let mut spots = Vec::new();
    for dy in -range..=range {
        for dx in -range..=range {
            let p = TilePos::new(target.x + dx, target.y + dy, target.z);
            if p == target
                || blocked(world, p)
                || !crate::mobs::ai::behaviors::can_see(world, target, p, range as u32)
            {
                continue;
            }
            spots.push(p);
        }
    }
    if escape {
        // The first of the farthest (`best_distance == range` stops the search).
        let mut best: Option<(u32, TilePos)> = None;
        for p in spots {
            let d = target.chebyshev(p).unwrap_or(0);
            if best.is_none_or(|(b, _)| d > b) {
                best = Some((d, p));
            }
        }
        best.map(|(_, p)| p)
    } else if spots.is_empty() {
        None
    } else {
        Some(spots[rng.range(0, spots.len() as u32 - 1) as usize])
    }
}

/// `ice_demon_teleport/Activate` to `to`: the jump lands `time_delay` later.
pub fn start_teleport(world: &mut World, demon: Entity, to: TilePos) -> bool {
    let now = now(world);
    if !teleport_ready(world, demon, now) || !super::abilities::open_ground(world, to) {
        return false;
    }
    if let Some(mut d) = world.get_mut::<DemonPowers>(demon) {
        d.teleport_ready = now + TELEPORT_COOLDOWN;
        d.jump = Some((now + TELEPORT_DELAY, to));
    }
    true
}

/// `teleport_to_turf`: `do_teleport(forced = TRUE)`.
fn jumps(world: &mut World) {
    let now = now(world);
    let due: Vec<(Entity, TilePos)> = world
        .query::<(Entity, &DemonPowers)>()
        .iter(world)
        .filter_map(|(e, d)| d.jump.filter(|(at, _)| now >= *at).map(|(_, to)| (e, to)))
        .collect();
    for (demon, to) in due {
        if let Some(mut d) = world.get_mut::<DemonPowers>(demon) {
            d.jump = None;
        }
        if alive(world, demon) {
            world.entity_mut(demon).insert(to);
        }
    }
}

/// `create_afterimages` castable: under 75 health, off cooldown, fewer than two alive.
pub fn clone_ready(world: &World, demon: Entity, now: f64) -> bool {
    let low = world
        .get::<Health>(demon)
        .is_some_and(|h| h.health() < CLONE_BELOW_HEALTH);
    low && world
        .get::<DemonPowers>(demon)
        .is_some_and(|d| now >= d.clone_ready && live_clones(world, &d.clones) < MAX_CLONES)
}

fn live_clones(world: &World, clones: &[Entity]) -> usize {
    clones
        .iter()
        .filter(|&&c| world.get::<Stat>(c).is_some_and(|s| *s != Stat::Dead))
        .count()
}

/// `conjure/limit_summons/create_afterimages`: two afterimages on free tiles around it, tied
/// to it (`joint_damage`).
pub fn conjure_afterimages(world: &mut World, demon: Entity) -> bool {
    let now = now(world);
    let Some(&at) = world.get::<TilePos>(demon) else {
        return false;
    };
    if !clone_ready(world, demon, now) {
        return false;
    }
    let mut free: Vec<TilePos> = std::iter::once(at)
        .chain(ss13_shared::Dir::DMI_ORDER.into_iter().map(|d| at.step(d)))
        .filter(|&p| super::abilities::open_ground(world, p))
        .collect();
    let mut rng = CombatRng(world.resource::<CombatRng>().0);
    let mut spots = Vec::new();
    for _ in 0..CLONE_AMOUNT {
        if free.is_empty() {
            break;
        }
        spots.push(free.swap_remove(rng.range(0, free.len() as u32 - 1) as usize));
    }
    world.resource_mut::<CombatRng>().0 = rng.0;
    let mut made = Vec::new();
    for spot in spots {
        if let Some(image) = crate::mobs::spawn::spawn_mob(world, AFTERIMAGE, spot) {
            if let Some(mut a) = world.get_mut::<Afterimage>(image) {
                a.overlord = Some(demon);
            }
            made.push(image);
        }
    }
    if let Some(mut d) = world.get_mut::<DemonPowers>(demon) {
        d.clone_ready = now + CLONE_COOLDOWN;
        d.clones.extend(made);
    }
    true
}

/// `joint_damage` (what an afterimage loses, its demon loses as brute) and `fade_into_nothing`.
fn afterimages(world: &mut World) {
    let now = now(world);
    let images: Vec<(Entity, Afterimage, f32, bool)> = world
        .query::<(Entity, &Afterimage, &Health, &Stat)>()
        .iter(world)
        .map(|(e, a, h, s)| (e, *a, h.health(), *s == Stat::Dead))
        .collect();
    for (image, a, health, dead) in images {
        let lost = a.last_health - health;
        if lost > 0.0
            && let Some(overlord) = a.overlord.filter(|&o| alive(world, o))
        {
            super::hurt(world, overlord, DamageType::Brute, lost, None);
        }
        if let Some(mut img) = world.get_mut::<Afterimage>(image) {
            img.last_health = health;
        }
        if dead {
            world.entity_mut(image).remove::<Afterimage>();
        } else if now >= a.fades_at {
            world.entity_mut(image).remove::<Afterimage>();
            if let Some(mut stat) = world.get_mut::<Stat>(image) {
                stat.set_if_neq(Stat::Dead);
            }
        }
    }
    // `delete_copy`: the demons forget gone afterimages.
    let mut q = world.query::<(Entity, &DemonPowers)>();
    let demons: Vec<(Entity, Vec<Entity>)> = q
        .iter(world)
        .filter(|(_, d)| !d.clones.is_empty())
        .map(|(e, d)| (e, d.clones.clone()))
        .collect();
    for (demon, clones) in demons {
        let kept: Vec<Entity> = clones
            .into_iter()
            .filter(|&c| world.get::<Stat>(c).is_some_and(|s| *s != Stat::Dead))
            .collect();
        if let Some(mut d) = world.get_mut::<DemonPowers>(demon) {
            d.clones = kept;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eruption_rings_follow_circle_range() {
        let c = TilePos::new(10, 10, 1);
        // Ring 1: all 8 neighbours.
        assert_eq!(eruption_ring(c, ERUPTION_RANGE, 1).len(), 8);
        // Ring 3 (r² = 10.5): the corners (18) and the knight's moves past them are out.
        let ring3 = eruption_ring(c, ERUPTION_RANGE, 3);
        assert!(ring3.contains(&TilePos::new(13, 10, 1)));
        assert!(ring3.contains(&TilePos::new(13, 11, 1)));
        assert!(!ring3.contains(&TilePos::new(13, 12, 1)));
        assert!(!ring3.contains(&TilePos::new(13, 13, 1)));
        assert_eq!(ring3.len(), 12);
        assert!(eruption_ring(c, ERUPTION_RANGE, 0).is_empty());
    }
}
