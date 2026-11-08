//! Tesla balls and lightning (server): tg `code/modules/power/tesla/energy_ball.dm`
//! (`/obj/energy_ball`, `tesla_zap`) and BandaStation's Tesla Rework
//! (`modular_bandastation/tesla_rework/code/tesla_ball.dm`, `/obj/tesla_ball`, what a tesla
//! delamination of the supermatter makes).
//!
//! - [`tesla_zap`]: a bolt jumps to the nearest best target in sight within `range + 2` tiles —
//!   a tesla coil not hit within the last second, then a grounding rod, a living mob, a
//!   machine, a structure — and on from there with what is left (a coil keeps a quarter of a
//!   power zap for its net and passes the rest on, a rod swallows everything, a mob takes
//!   `min(round(power / 600), 90) ± 5` through `electrocute_act` and the bolt goes on at 2/3, a
//!   machine or structure takes `power * 2.5e-4` burn and halves it); 20% of the time it forks
//!   in two halves. It stops under 400 kJ. The air it lands in is electrolyzed.
//! - A ball ([`TeslaBall`]) processes every 2 s: it wanders `4 + 1.5 * mini-balls` random
//!   steps (biased towards what it hit last), never into a containment field, dusting the
//!   carbons it lands on unless an anchored grounding rod stands within 2 tiles, then zaps:
//!   upstream at a fixed 695 MJ, with each mini-ball adding a smaller zap; its mini-balls come
//!   (10 s after) and go as its energy passes doubling thresholds.
//! - The reworked ball starts with 1500 MJ and loses 4% a tick plus what it zaps
//!   (`energy * zap ratio`); the air on its tile sets the ratios (hotter: stronger zaps and
//!   faster decay; plasma scales the zaps and what an emitter hit gives back, 45 MJ base); one
//!   mini-ball per 200 MJ; at 100 MJ it disperses. Its zaps make power.
// shortcut: no orbiting mini-ball sprites (a count), no bolt beams, no bikes, ridden vehicles or
// blobs as targets, no silicon EMP or longer silicon bounce, no buckled-mob shocks, no
// telekinesis touch, no ghost alerts; a tesla shock always stuns (`electrocute_act` has no
// `SHOCK_NOSTUN`).

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_atmos::{Atmos, GasId};
use ss13_mapload::DmObj;
use ss13_mapload::fidelity::is_under;
use ss13_shared::chat::{ChatChannel, Span};
use ss13_shared::{Dir, TilePos};
use ss13_world::{Bumped, Density, SpatialIndex};

use super::EngineRng;
use super::accumulator::{Accumulator, AccumulatorKind, ZAP_GENERATES_POWER};
use super::supermatter::{Shocked, ZAP_MOB_DAMAGE, ZAP_MOB_STUN, ZAP_OBJ_DAMAGE};
use crate::health::{DamageType, Health, Stat};

pub const ENERGY_BALL: &str = "/obj/energy_ball";
pub const TESLA_BALL: &str = "/obj/tesla_ball";
const MJ: f64 = 1e6;
/// Upstream `TESLA_DEFAULT_ENERGY`, `TESLA_MINI_ENERGY`.
const UPSTREAM_ZAP_ENERGY: f64 = 695.304 * MJ;
const UPSTREAM_MINI_ENERGY: f64 = 347.652 * MJ;
/// BandaStation `TESLA_DEFAULT_ENERGY`, `TESLA_MINI_ENERGY`, `TESLA_PASSIVE_DECAY_RATIO`,
/// `TESLA_ZAP_POWER_RATIO`, `TESLA_MIN_ENERGY`, `TESLA_EMITTER_HIT_ENERGY`.
pub const DEFAULT_ENERGY: f64 = 1500.0 * MJ;
pub const MINI_ENERGY: f64 = 200.0 * MJ;
pub const PASSIVE_DECAY_RATIO: f64 = 0.04;
pub const ZAP_POWER_RATIO: f64 = 0.1;
pub const MIN_ENERGY: f64 = 100.0 * MJ;
pub const EMITTER_HIT_ENERGY: f64 = 45.0 * MJ;
/// `ZAP_DEFAULT_FLAGS`, `ZAP_TESLA_FLAGS`.
pub const ZAP_DEFAULT_FLAGS: u32 = ZAP_MOB_STUN | ZAP_MOB_DAMAGE | ZAP_OBJ_DAMAGE;
pub const ZAP_TESLA_FLAGS: u32 = ZAP_DEFAULT_FLAGS | ZAP_GENERATES_POWER;
/// `tesla_zap`'s default `cutoff`, joules.
pub const ZAP_CUTOFF: f64 = 4e5;
/// `SSobj`.
const PROCESS_SECS: f64 = 2.0;
/// `addtimer(new_mini_ball)`: upstream 10 s, reworked 2 s.
const UPSTREAM_MINI_DELAY: f64 = 10.0;
const REWORK_MINI_DELAY: f64 = 2.0;
/// `EMP_LIGHT`.
const EMP_LIGHT: u8 = 2;
/// `GLOB.alldirs`.
const ALLDIRS: [Dir; 8] = [
    Dir::North,
    Dir::South,
    Dir::East,
    Dir::West,
    Dir::NorthEast,
    Dir::NorthWest,
    Dir::SouthEast,
    Dir::SouthWest,
];
/// `things_to_shock` exclusions.
const NOT_SHOCKED: [&str; 15] = [
    "/obj/machinery/atmospherics",
    "/obj/machinery/portable_atmospherics",
    "/obj/machinery/power/emitter",
    "/obj/machinery/field/generator",
    "/obj/machinery/field/containment",
    "/obj/machinery/camera",
    "/obj/machinery/gateway",
    "/obj/structure/disposalpipe",
    "/obj/structure/disposaloutlet",
    "/obj/machinery/disposal/delivery_chute",
    "/obj/structure/sign",
    "/obj/structure/lattice",
    "/obj/structure/grille",
    "/obj/structure/frame/machine",
    "/mob/living/simple_animal",
];
/// Sounds (`ss13_mapload::sound::files::TESLA`).
const LIGHTNINGBOLT: usize = 0;
const CHARGEUP: usize = 1;
const MOVE_SOUNDS: [usize; 3] = [2, 3, 4];
const DESTROY: usize = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BallKind {
    /// `/obj/energy_ball`.
    Upstream,
    /// `/obj/tesla_ball`.
    Reworked,
}

/// tg `/obj/energy_ball`, BandaStation `/obj/tesla_ball`.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct TeslaBall {
    pub kind: BallKind,
    /// Upstream: an abstract charge (50 by default); reworked: joules.
    pub energy: f64,
    /// `orbiting_balls.len`.
    pub miniballs: u32,
    /// `addtimer(new_mini_ball)`: when each coming mini-ball appears.
    pub pending_minis: Vec<f64>,
    /// Upstream `energy_to_raise`, `energy_to_lower`.
    pub energy_to_raise: f64,
    pub energy_to_lower: f64,
    /// What its zaps hit lately (its own momentum).
    pub shocked_things: Vec<Entity>,
    /// Reworked: what an emitter hit gives (`hit_heal`), and the air last read.
    pub hit_heal: f64,
    pub temp: f64,
    pub total_moles: f64,
    pub plasma_moles: f64,
    pub target: Option<Entity>,
    pub next_process: f64,
}

impl TeslaBall {
    pub fn new(kind: BallKind, now: f64) -> Self {
        Self {
            kind,
            energy: match kind {
                BallKind::Upstream => 50.0,
                BallKind::Reworked => DEFAULT_ENERGY,
            },
            miniballs: 0,
            pending_minis: Vec::new(),
            energy_to_raise: 32.0,
            energy_to_lower: -20.0,
            shocked_things: Vec::new(),
            hit_heal: EMITTER_HIT_ENERGY,
            temp: 0.0,
            total_moles: 0.0,
            plasma_moles: 0.0,
            target: None,
            next_process: now + PROCESS_SECS,
        }
    }

    /// Upstream `handle_energy`: true when a mini-ball is coming; a lost threshold drops one.
    pub fn upstream_handle_energy(&mut self) -> bool {
        if self.energy >= self.energy_to_raise {
            self.energy_to_lower = self.energy_to_raise - 20.0;
            self.energy_to_raise *= 1.25;
            return true;
        }
        if self.energy < self.energy_to_lower && self.miniballs > 0 {
            self.energy_to_raise /= 1.25;
            self.energy_to_lower = self.energy_to_raise / 1.25 - 20.0;
            self.miniballs -= 1;
        }
        false
    }

    /// The reworked `handle_energy` after dispersing: one mini-ball per 200 MJ, true when one
    /// more is coming; one too many goes.
    pub fn rework_handle_energy(&mut self) -> bool {
        let wanted = (self.energy / MINI_ENERGY).floor() as u32;
        if self.miniballs < wanted {
            return true;
        }
        if self.miniballs > wanted {
            self.miniballs -= 1;
        }
        false
    }

    /// The reworked `process` up to the zap: the gas ratios, the passive decay; returns the
    /// zap power (`energy * zap ratio`).
    pub fn rework_decay(&mut self) -> f64 {
        let (zap_ratio, decay_ratio, hit_heal) = gas_factors(self.temp, self.plasma_moles);
        self.hit_heal = hit_heal;
        self.energy = (self.energy - self.energy * decay_ratio).max(0.0);
        self.energy * zap_ratio
    }
}

/// BandaStation's gas processing: (zap power ratio, passive decay ratio, emitter hit energy) for
/// the tile's temperature (K) and plasma (moles).
pub fn gas_factors(temp: f64, plasma_moles: f64) -> (f64, f64, f64) {
    let below = temp.min(10_000.0);
    let above = (temp - 10_000.0).max(0.0);
    let zap_ratio = (ZAP_POWER_RATIO + below * 0.000_005 + above * 0.0005) * plasma_moles * 0.0001;
    let decay_ratio = PASSIVE_DECAY_RATIO + above * 0.000_01;
    let temp_bonus = (temp / 100.0 * 0.03).max(0.0);
    let temp_multiplier = (temp / 275.0).clamp(0.85, 1.0) * (1.0 + temp_bonus / EMITTER_HIT_ENERGY);
    let plasma_factor = (plasma_moles * 0.1).max(1.0);
    (
        zap_ratio,
        decay_ratio,
        EMITTER_HIT_ENERGY * temp_multiplier * plasma_factor,
    )
}

pub struct TeslaPlugin;

impl Plugin for TeslaPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (adopt_balls, tesla_tick).chain().after(super::PowerSystems),
        );
    }
}

/// A new ball (map, spawn) gets its state (`Initialize`).
fn adopt_balls(
    mut commands: Commands,
    time: Res<Time>,
    added: Query<(Entity, &DmObj), (Added<DmObj>, Without<TeslaBall>)>,
) {
    let now = time.elapsed_secs_f64();
    for (e, obj) in &added {
        let kind = if is_under(&obj.path, TESLA_BALL) {
            BallKind::Reworked
        } else if is_under(&obj.path, ENERGY_BALL) {
            BallKind::Upstream
        } else {
            continue;
        };
        commands.entity(e).insert(TeslaBall::new(kind, now));
    }
}

fn now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

/// `/datum/sm_delam/proc/effect_tesla`: BandaStation makes a reworked ball.
pub fn effect_tesla(world: &mut World, pos: TilePos) -> Option<Entity> {
    let Some(e) = crate::construction::spawn_obj(world, TESLA_BALL, pos) else {
        warn!("no prototype for {TESLA_BALL}: no tesla at {pos:?}");
        return None;
    };
    let t = now(world);
    world
        .entity_mut(e)
        .insert(TeslaBall::new(BallKind::Reworked, t));
    info!(target: "engine", "reworked tesla created at {pos:?}");
    Some(e)
}

fn tesla_tick(world: &mut World, mut bumps: Local<MessageCursor<Bumped>>) {
    // `Bumped`: what walks into a ball is dusted.
    let bumped: Vec<(Entity, Entity)> = bumps
        .read(world.resource::<Messages<Bumped>>())
        .map(|b| (b.entity, b.obstacle))
        .collect();
    for (mover, obstacle) in bumped {
        if world.get::<TeslaBall>(obstacle).is_some() {
            dust_mobs(world, obstacle, mover);
        }
    }
    let t = now(world);
    let mut q = world.query_filtered::<Entity, With<TeslaBall>>();
    let balls: Vec<Entity> = q.iter(world).collect();
    for e in balls {
        let Some(mut ball) = world.get_mut::<TeslaBall>(e) else {
            continue;
        };
        let before = ball.pending_minis.len();
        ball.pending_minis.retain(|&at| at > t);
        let arrived = (before - ball.pending_minis.len()) as u32;
        ball.miniballs += arrived;
        if t < ball.next_process {
            continue;
        }
        ball.next_process = t + PROCESS_SECS;
        let kind = ball.kind;
        match kind {
            BallKind::Upstream => process_upstream(world, e, t),
            BallKind::Reworked => process_rework(world, e, t),
        }
    }
}

/// `/obj/energy_ball/process`.
fn process_upstream(world: &mut World, e: Entity, t: f64) {
    let Some(mut ball) = world.get_mut::<TeslaBall>(e) else {
        return;
    };
    let coming = ball.upstream_handle_energy();
    if coming {
        ball.pending_minis.push(t + UPSTREAM_MINI_DELAY);
    }
    let moves = 4.0 + f64::from(ball.miniballs) * 1.5;
    let Some(&pos) = world.get::<TilePos>(e) else {
        return;
    };
    if coming {
        sound(world, pos, CHARGEUP, 100.0, 30.0);
    }
    wander(world, e, moves);
    let Some(&pos) = world.get::<TilePos>(e) else {
        return;
    };
    sound(world, pos, LIGHTNINGBOLT, 100.0, 30.0);
    zap_around(
        world,
        e,
        pos,
        UPSTREAM_ZAP_ENERGY,
        ZAP_DEFAULT_FLAGS,
        |range, _| UPSTREAM_MINI_ENERGY / 7.0 * f64::from(range),
    );
}

/// BandaStation `/obj/tesla_ball/process`.
fn process_rework(world: &mut World, e: Entity, t: f64) {
    let Some(&pos) = world.get::<TilePos>(e) else {
        return;
    };
    read_air(world, e, pos);
    let Some(mut ball) = world.get_mut::<TeslaBall>(e) else {
        return;
    };
    let zap_power = ball.rework_decay();
    if ball.energy <= MIN_ENERGY {
        disperse(world, e, pos);
        return;
    }
    let coming = ball.rework_handle_energy();
    if coming {
        ball.pending_minis.push(t + REWORK_MINI_DELAY);
    }
    let moves = 4.0 + f64::from(ball.miniballs) * 1.5;
    if coming {
        sound(world, pos, CHARGEUP, 100.0, 30.0);
    }
    wander(world, e, moves);
    let Some(&pos) = world.get::<TilePos>(e) else {
        return;
    };
    let pick = world.resource_mut::<EngineRng>().0.range(0, 2) as usize;
    sound(world, pos, MOVE_SOUNDS[pick], 100.0, 10.0);
    // The main zap first, the energy it took gone before the mini-balls' share is reckoned.
    let mut shocked = Vec::new();
    tesla_zap(
        world,
        e,
        pos,
        3,
        zap_power,
        ZAP_CUTOFF,
        ZAP_TESLA_FLAGS,
        &mut shocked,
        0,
    );
    sound(world, pos, LIGHTNINGBOLT, 120.0, 30.0);
    let Some(mut ball) = world.get_mut::<TeslaBall>(e) else {
        return;
    };
    ball.energy = (ball.energy - zap_power).max(0.0);
    let energy = ball.energy;
    cut_momentum(&mut ball.shocked_things);
    ball.shocked_things.extend(shocked);
    zap_around(world, e, pos, 0.0, ZAP_TESLA_FLAGS, |range, _| {
        energy / 2.0 / 7.0 * f64::from(range)
    });
}

/// `shocked_things.Cut(1, shocked_things.len / 1.3)`: the oldest hits are forgotten (an end
/// below 1 is 0: the whole list).
fn cut_momentum(list: &mut Vec<Entity>) {
    let end = (list.len() as f64 / 1.3) as usize;
    if end == 0 {
        list.clear();
    } else {
        list.drain(..end - 1);
    }
}

/// The ball's own zap of `main` joules (none at 0), then one per mini-ball of
/// `mini(range, count)` joules at `rand(1, clamp(count, 2, 3))` tiles; the hits become its
/// momentum.
fn zap_around(
    world: &mut World,
    e: Entity,
    pos: TilePos,
    main: f64,
    flags: u32,
    mini: impl Fn(u32, u32) -> f64,
) {
    let mut info = Vec::new();
    if main > 0.0 {
        if let Some(mut ball) = world.get_mut::<TeslaBall>(e) {
            cut_momentum(&mut ball.shocked_things);
        }
        tesla_zap(world, e, pos, 3, main, ZAP_CUTOFF, flags, &mut info, 0);
    }
    let count = world.get::<TeslaBall>(e).map_or(0, |b| b.miniballs);
    for _ in 0..count {
        let range = world
            .resource_mut::<EngineRng>()
            .0
            .range(1, count.clamp(2, 3));
        let mut temp = Vec::new();
        tesla_zap(
            world,
            e,
            pos,
            range,
            mini(range, count),
            ZAP_CUTOFF,
            flags,
            &mut temp,
            0,
        );
        info.extend(temp);
    }
    if let Some(mut ball) = world.get_mut::<TeslaBall>(e) {
        ball.shocked_things.extend(info);
    }
}

/// `process_atmos`: the open tile's moles, temperature and plasma.
fn read_air(world: &mut World, e: Entity, pos: TilePos) {
    let closed =
        crate::construction::turf_at(world, pos).is_some_and(|t| world.get::<Density>(t).is_some());
    if closed {
        return;
    }
    let Some(air) = world
        .get_resource::<Atmos>()
        .and_then(|a| a.air(pos).cloned())
    else {
        return;
    };
    if let Some(mut ball) = world.get_mut::<TeslaBall>(e) {
        ball.total_moles = air.total_moles();
        ball.temp = air.temperature;
        ball.plasma_moles = air.get(GasId::PLASMA);
    }
}

/// `disperse_event`.
fn disperse(world: &mut World, e: Entity, pos: TilePos) {
    let name = crate::security::name_of(world, e);
    crate::security::show(
        world,
        pos,
        crate::health::DEFAULT_MESSAGE_RANGE,
        ChatChannel::Local,
        (
            Span::Warning,
            format!(
                "{} вспыхивает с жутким треском и растворяется, ионизируя воздух вокруг!",
                ss13_shared::capitalize(&name)
            ),
        ),
        &[],
    );
    sound(world, pos, DESTROY, 165.0, 75.0);
    info!(target: "engine", "tesla ball {e} dispersed");
    world.despawn(e);
}

/// `move(move_amount)`: `floor(amount) + 1` random steps, some towards what it hit lately
/// (and 10% of them towards its target), each onto a tile no field blocks.
fn wander(world: &mut World, e: Entity, amount: f64) {
    let Some(ball) = world.get::<TeslaBall>(e).cloned() else {
        return;
    };
    for _ in 0..=(amount.floor() as u32) {
        let Some(&pos) = world.get::<TilePos>(e) else {
            return;
        };
        let mut dirs: Vec<Option<Dir>> = ALLDIRS.into_iter().map(Some).collect();
        if !ball.shocked_things.is_empty() {
            for _ in 0..30 {
                let i = world
                    .resource_mut::<EngineRng>()
                    .0
                    .range(0, ball.shocked_things.len() as u32 - 1)
                    as usize;
                let toward = world
                    .get::<TilePos>(ball.shocked_things[i])
                    .and_then(|&p| crate::mecha::core::dir_between(pos, p));
                dirs.push(toward);
            }
        }
        let mut rng = world.resource_mut::<EngineRng>();
        let mut dir = dirs[rng.0.range(0, dirs.len() as u32 - 1) as usize];
        let chase = rng.0.chance(10.0);
        if let Some(target) = ball.target
            && chase
            && let Some(&tp) = world.get::<TilePos>(target)
        {
            dir = crate::mecha::core::dir_between(pos, tp);
        }
        // `get_step(src, 0)`: its own tile, a move in place.
        let Some(dir) = dir else {
            continue;
        };
        let (dx, dy) = dir.offset();
        let next = TilePos::new(pos.x + dx, pos.y + dy, pos.z);
        if !world.resource::<SpatialIndex>().in_bounds(next)
            || super::singularity::blocks_singularity(world, next)
        {
            continue;
        }
        world.entity_mut(e).insert((next, ss13_shared::Facing(dir)));
        let carbons: Vec<Entity> = crate::explosions::ex_act::tile_contents(world, next)
            .into_iter()
            .filter(|&m| world.get::<Health>(m).is_some())
            .collect();
        for mob in carbons {
            dust_mobs(world, e, mob);
        }
    }
}

/// `dust_mobs`: a carbon is dusted, unless an anchored grounding rod stands within 2 tiles.
fn dust_mobs(world: &mut World, ball: Entity, mob: Entity) {
    let carbon = world.get::<Health>(mob).is_some()
        && world.get::<crate::inventory::Inventory>(mob).is_some()
        && world.get::<crate::silicon::Silicon>(mob).is_none()
        && world.get::<crate::mobs::BasicMob>(mob).is_none();
    let Some(&pos) = world.get::<TilePos>(ball) else {
        return;
    };
    if !carbon {
        return;
    }
    let grounded = world.resource::<SpatialIndex>().in_radius(pos, 2).any(|r| {
        world
            .get::<Accumulator>(r)
            .is_some_and(|a| a.kind == AccumulatorKind::GroundingRod)
            && crate::obj_damage::anchored(world, r)
    });
    if grounded {
        return;
    }
    info!(target: "engine", "{mob} was dusted by an energy ball");
    crate::health::dust(world, mob);
}

/// The reworked ball's `check_hit`: an emitter beam gives it `hit_heal` (true: the beam is
/// spent on a ball).
pub fn emitter_hit(world: &mut World, target: Entity) -> bool {
    let Some(mut ball) = world.get_mut::<TeslaBall>(target) else {
        return false;
    };
    if ball.kind != BallKind::Reworked {
        return true;
    }
    let heal = ball.hit_heal;
    ball.energy += heal;
    let Some(&pos) = world.get::<TilePos>(target) else {
        return true;
    };
    let name = crate::security::name_of(world, target);
    crate::security::show(
        world,
        pos,
        crate::health::DEFAULT_MESSAGE_RANGE,
        ChatChannel::Local,
        (
            Span::Notice,
            format!(
                "{} потрескивает от попадания!",
                ss13_shared::capitalize(&name)
            ),
        ),
        &[],
    );
    true
}

fn sound(world: &mut World, pos: TilePos, which: usize, vol: f32, extrarange: f32) {
    let mut s =
        crate::sound::PlaySound::new(pos, ss13_mapload::sound::files::TESLA[which], vol, true)
            .extrarange(extrarange);
    s.pressure_affected = false;
    crate::sound::playsound(world, s);
}

/// A bolt's target classes, worst first (`STRUCTURE` .. `COIL`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Class {
    Structure,
    Machinery,
    Living,
    Rod,
    Coil,
}

/// `TRAIT_BEING_SHOCKED` (1 s after a hit).
fn being_shocked(world: &World, e: Entity, now: f64) -> bool {
    world
        .get::<Accumulator>(e)
        .map(|a| a.shocked_at)
        .or_else(|| world.resource::<Shocked>().0.get(&e).copied())
        .is_some_and(|at| now - at < 1.0)
}

fn classify(world: &World, e: Entity, now: f64) -> Option<Class> {
    if let Some(acc) = world.get::<Accumulator>(e) {
        return match acc.kind {
            AccumulatorKind::TeslaCoil { .. } => {
                (!being_shocked(world, e, now)).then_some(Class::Coil)
            }
            AccumulatorKind::GroundingRod => Some(Class::Rod),
        };
    }
    if world.get::<Health>(e).is_some() {
        let alive = world.get::<Stat>(e) != Some(&Stat::Dead);
        // `TRAIT_TESLA_SHOCKIMMUNE`: a lightning bolt's caster.
        let immune = crate::spells::knows(world, e, crate::spells::Spell::LightningBolt);
        return (alive && !immune && !being_shocked(world, e, now)).then_some(Class::Living);
    }
    let path = &world.get::<DmObj>(e)?.path;
    if NOT_SHOCKED.iter().any(|r| is_under(path, r)) || being_shocked(world, e, now) {
        return None;
    }
    if is_under(path, "/obj/machinery") {
        Some(Class::Machinery)
    } else if is_under(path, "/obj/structure") {
        Some(Class::Structure)
    } else {
        None
    }
}

/// tg `tesla_zap(source, zap_range, power, cutoff, zap_flags, shocked_targets)`.
#[allow(clippy::too_many_arguments)]
pub fn tesla_zap(
    world: &mut World,
    source: Entity,
    from: TilePos,
    range: u32,
    power: f64,
    cutoff: f64,
    flags: u32,
    shocked: &mut Vec<Entity>,
    depth: u32,
) {
    if !shocked.contains(&source) {
        shocked.push(source);
    }
    if power < cutoff || depth > 32 {
        return;
    }
    let t = now(world);
    // `oview(zap_range + 2)`: the nearest of the best class in sight.
    let reach = range + 2;
    let mut best: Option<(Class, u32, Entity)> = None;
    let candidates: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .in_radius(from, reach)
        .filter(|&c| c != source && !shocked.contains(&c))
        .collect();
    for c in candidates {
        let Some(class) = classify(world, c, t) else {
            continue;
        };
        let Some(&at) = world.get::<TilePos>(c) else {
            continue;
        };
        let Some(dist) = at.chebyshev(from) else {
            continue;
        };
        let better = best.is_none_or(|(bc, bd, be)| {
            class > bc || (class == bc && (dist < bd || (dist == bd && c < be)))
        });
        if better && crate::mobs::ai::behaviors::can_see(world, from, at, reach) {
            best = Some((class, dist, c));
        }
    }
    let Some((class, _, target)) = best else {
        return;
    };
    let Some(&at) = world.get::<TilePos>(target) else {
        return;
    };
    let mut power = power;
    let mut next_range = if class == Class::Coil { 5 } else { 2 };
    if class == Class::Living {
        world.resource_mut::<Shocked>().0.insert(target, t);
        let damage = if flags & ZAP_MOB_DAMAGE != 0 {
            let spread = world.resource_mut::<EngineRng>().0.range(0, 10) as f32 - 5.0;
            (power / 600.0).round().min(90.0) as f32 + spread
        } else {
            0.0
        };
        shock(world, target, damage, source);
        next_range = 5;
        power /= 1.5;
    } else {
        power = zap_act(world, target, class, power, flags, t);
    }
    // Electrolysis.
    if let Some(mut atmos) = world.get_resource_mut::<Atmos>() {
        atmos.modify_air(at, |air| air.electrolyze(power / 200.0, None));
    }
    if world.resource_mut::<EngineRng>().0.chance(20.0) {
        let mut copy = shocked.clone();
        tesla_zap(
            world,
            target,
            at,
            next_range,
            power * 0.5,
            cutoff,
            flags,
            &mut copy,
            depth + 1,
        );
        tesla_zap(
            world,
            target,
            at,
            next_range,
            power * 0.5,
            cutoff,
            flags,
            shocked,
            depth + 1,
        );
        shocked.extend(copy);
    } else {
        tesla_zap(
            world,
            target,
            at,
            next_range,
            power,
            cutoff,
            flags,
            shocked,
            depth + 1,
        );
    }
}

/// `electrocute_act(damage, source, 1, SHOCK_TESLA)` with the human's tesla siemens:
/// insulated gloves take 0.5 off, an insulated suit 0.95 (-1: all), then the gloves multiply.
fn shock(world: &mut World, mob: Entity, damage: f32, source: Entity) {
    let Some(inv) = world.get::<crate::inventory::Inventory>(mob).cloned() else {
        if let Ok(mut entity) = world.get_entity_mut(mob)
            && let Ok((mut health, mut stat)) =
                entity.get_components_mut::<(&mut Health, &mut Stat)>()
        {
            crate::health::apply_damage(&mut health, &mut stat, DamageType::Burn, damage.max(0.0));
        }
        return;
    };
    let siemens_of = |e: Option<Entity>| {
        e.and_then(|e| world.get::<ss13_mapload::DmClothing>(e))
            .map(|c| c.siemens_coefficient)
    };
    let (gloves, suit) = (siemens_of(inv.gloves), siemens_of(inv.suit));
    let mut siemens = 1.0f32;
    if gloves.is_some_and(|g| g <= 0.0) {
        siemens -= 0.5;
    }
    match suit {
        Some(-1.0) => siemens -= 1.0,
        Some(s) if s <= 0.0 => siemens -= 0.95,
        _ => {}
    }
    siemens = siemens.max(0.0) * gloves.unwrap_or(1.0);
    crate::wires::electrocute_act(world, mob, damage, source, siemens);
}

/// `zap_act` of a coil, rod, machine or structure: what the bolt keeps.
fn zap_act(world: &mut World, target: Entity, class: Class, power: f64, flags: u32, t: f64) -> f64 {
    let working = crate::obj_damage::anchored(world, target)
        && !world
            .get::<crate::construction::PanelOpen>(target)
            .is_some_and(|p| p.0);
    if matches!(class, Class::Coil | Class::Rod) && working {
        let wired = world.resource::<super::Grid>().node_net(target).is_some();
        let mut acc = world.get_mut::<Accumulator>(target).expect("classified");
        return acc.zap(power, flags, wired, t);
    }
    let path = world
        .get::<DmObj>(target)
        .map(|o| o.path.clone())
        .unwrap_or_default();
    let mut power = power;
    let mut flags = flags;
    // `door/zap_act` and `light/zap_act` take no damage.
    if is_under(&path, "/obj/machinery/door") || is_under(&path, "/obj/machinery/light") {
        flags &= !ZAP_OBJ_DAMAGE;
    }
    let machine = is_under(&path, "/obj/machinery");
    if flags & ZAP_OBJ_DAMAGE != 0 {
        crate::obj_damage::take_damage(world, target, (power * 2.5e-4) as f32, DamageType::Burn);
        if machine && world.resource_mut::<EngineRng>().0.chance(40.0) {
            crate::emp::emp_act(world, target, EMP_LIGHT);
        }
    }
    // `/obj/structure/zap_act` always takes its share; a machine only with object damage.
    if !machine || flags & ZAP_OBJ_DAMAGE != 0 {
        power -= power * 5e-4;
    }
    if world.get_entity(target).is_err() {
        return 0.0;
    }
    // `/obj/zap_act`.
    let mut shocked = world.resource_mut::<Shocked>();
    shocked.0.retain(|_, at| t - *at < 1.0);
    shocked.0.insert(target, t);
    power / 2.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gas_sets_the_rework_ratios() {
        // No plasma: no zaps; room temperature keeps the base decay and hit energy.
        let (zap, decay, heal) = gas_factors(293.15, 0.0);
        assert_eq!(zap, 0.0);
        assert_eq!(decay, PASSIVE_DECAY_RATIO);
        let bonus = 293.15 / 100.0 * 0.03;
        assert!((heal - EMITTER_HIT_ENERGY * (1.0 + bonus / EMITTER_HIT_ENERGY)).abs() < 1e-6);
        // 100 mol of plasma at 12000 K: ratio (0.1 + 0.05 + 1.0) * 0.01, decay 0.06.
        let (zap, decay, heal) = gas_factors(12_000.0, 100.0);
        assert!((zap - 1.15 * 0.01).abs() < 1e-12);
        assert!((decay - 0.06).abs() < 1e-12);
        assert!(heal > EMITTER_HIT_ENERGY * 10.0);
        // Cold air weakens the hits (down to 85%).
        let (_, _, cold) = gas_factors(100.0, 0.0);
        assert!(
            (cold - EMITTER_HIT_ENERGY * 0.85 * (1.0 + 0.03 / EMITTER_HIT_ENERGY)).abs() < 1e-3
        );
    }

    #[test]
    fn rework_decays_and_counts_miniballs() {
        let mut ball = TeslaBall::new(BallKind::Reworked, 0.0);
        ball.temp = 293.15;
        assert_eq!(ball.rework_decay(), 0.0);
        assert!((ball.energy - DEFAULT_ENERGY * 0.96).abs() < 1.0);
        // 1440 MJ: seven mini-balls wanted, one at a time.
        assert!(ball.rework_handle_energy());
        ball.miniballs = 9;
        assert!(!ball.rework_handle_energy());
        assert_eq!(ball.miniballs, 8);
        ball.miniballs = 7;
        assert!(!ball.rework_handle_energy());
        assert_eq!(ball.miniballs, 7);
    }

    #[test]
    fn upstream_thresholds_give_about_nine_balls_at_200() {
        let mut ball = TeslaBall::new(BallKind::Upstream, 0.0);
        ball.energy = 200.0;
        let mut coming = 0;
        for _ in 0..30 {
            if ball.upstream_handle_energy() {
                coming += 1;
                ball.miniballs += 1;
            }
        }
        assert_eq!(coming, 9);
        // Losing its charge drops them one by one.
        ball.energy = 0.0;
        assert!(!ball.upstream_handle_energy());
        assert_eq!(ball.miniballs, 8);
    }

    #[test]
    fn momentum_forgets_the_oldest() {
        let mut list: Vec<Entity> = (0..10)
            .map(Entity::from_raw_u32)
            .map(Option::unwrap)
            .collect();
        cut_momentum(&mut list);
        // `Cut(1, 7)` drops the first six.
        assert_eq!(list.len(), 4);
        let mut one = vec![Entity::PLACEHOLDER];
        cut_momentum(&mut one);
        assert!(one.is_empty());
    }
}
