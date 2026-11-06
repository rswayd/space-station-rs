//! Legion, after tg `megafauna/legion.dm` and its abilities (`create_legion_skull.dm`,
//! `chase_target.dm`, `create_legion_turrets.dm`):
//!
//! - `OpenFire` (every `ranged_cooldown_time`, 2 s) rolls `rand(4)`: 0-2 a skull (a
//!   `legion_brood` on its tile with its faction, set on the target), 3 the charge, 4 sentinels.
//!   The abilities share the megafauna cooldown and never block melee (`melee_cooldown_time`).
//! - The charge ("charges!"): for 6 s it closes in (`minimum_distance` 0) at full speed; after
//!   2 s it flings itself at the target (7 tiles, 1.1 tiles a decisecond); a mob it slams takes
//!   `22 * size / 2` brute and is knocked 2 tiles back.
//! - Sentinels (`/obj/structure/legionturret`): 2 to `size * 2` on free turfs within 4 tiles;
//!   1.8 s later each aims at a conscious non-mining mob within 9 tiles in sight (else a random
//!   cardinal), 0.8 s later fires a blood pulse (hitscan, 19 burn to everything within 6 tiles
//!   of the line) and crumbles 0.5 s after.
//! - It keeps 5 tiles away; a human out cold next to it is infested by a new skull
//!   (`AttackingTarget`).
//! - Death splits it (large -> 3 medium -> 3 small each); the last one alive drops the staff of
//!   storms, another one a tendril chest one time in five, else bones.
// shortcut: no retreat (it only keeps its distance), spin animation, tracer beam visual or
// necropolis gate; the pulse ignores armour; the charge's knockback is a throw of the victim.
// tg `reset_charge` sets the speed to 2 instead of its own 5 (a permanent speed-up after the
// first charge): the initial speed is restored instead.

use bevy::prelude::*;
use ss13_shared::chat::Span;
use ss13_shared::{Dir, TilePos};
use ss13_world::{Density, Mover, SpatialIndex};

use super::{Kind, Megafauna, Step as MegaStep, living_on, roll, schedule, show, sound};
use crate::health::{DamageType, Health, Stat};
use crate::mobs::defense::hurt;

pub const LEGION: &str = "/mob/living/simple_animal/hostile/megafauna/legion";
pub const MEDIUM: &str = "/mob/living/simple_animal/hostile/megafauna/legion/medium";
pub const MEDIUM_LEFT: &str = "/mob/living/simple_animal/hostile/megafauna/legion/medium/left";
pub const MEDIUM_RIGHT: &str = "/mob/living/simple_animal/hostile/megafauna/legion/medium/right";
pub const MEDIUM_EYE: &str = "/mob/living/simple_animal/hostile/megafauna/legion/medium/eye";
pub const SMALL: &str = "/mob/living/simple_animal/hostile/megafauna/legion/small";
pub const TURRET: &str = "/obj/structure/legionturret";
pub const STORM_STAFF: &str = "/obj/item/storm_staff";
pub const TENDRIL_CHEST: &str = "/obj/structure/closet/crate/necropolis/tendril";
pub const BONES: &str = "/obj/item/stack/sheet/bone";

const ATTACK_SOUND: &str = "sound/effects/magic/demon_attack1.ogg";
const RATTLE: &str = "sound/effects/magic/RATTLEMEBONES.ogg";
const JACKHAMMER: &str = "sound/items/weapons/sonic_jackhammer.ogg";
const METEOR_IMPACT: &str = "sound/effects/meteorimpact.ogg";
const AIRLOCK: &str = "sound/machines/airlock/airlockopen.ogg";
const BIN_CLOSE: &str = "sound/effects/bin/bin_close.ogg";

/// `ranged_cooldown_time`.
const RANGED_COOLDOWN: f64 = 2.0;
/// `create_legion_skull` and `create_legion_turrets` `cooldown_time`.
const SKULL_COOLDOWN: f64 = 2.0;
const TURRET_COOLDOWN: f64 = 2.0;
/// `chase_target`: `cooldown_time`, the throw after, the speed-up's length.
const CHASE_COOLDOWN: f64 = 6.0;
const CHASE_THROW_AFTER: f64 = 2.0;
const CHASE_LENGTH: f64 = 6.0;
/// `throw_at(target, 7, 1.1)`: tiles, seconds a tile.
const THROW_RANGE: u32 = 7;
const THROW_STEP: f64 = 0.1 / 1.1;
/// The charge's speed (`set_varspeed(0)`): seconds a tile of its walk.
// shortcut: speed 0 is one step per 0.1 s, the server tick of tg's `walk_to`.
const CHARGE_MOVE_DELAY: f32 = 0.1;
/// `retreat_distance`/`minimum_distance`.
const KEEP_AWAY: u32 = 5;
/// Turret: `oview(owner, 4)`, `oview(9)`, `initial_firing_time`, `shot_delay`, `QDEL_IN`.
const TURRET_PLACE_RANGE: i32 = 4;
const TURRET_SIGHT: u32 = 9;
const TURRET_AIM_AFTER: f64 = 1.8;
const TURRET_SHOT_DELAY: f64 = 0.8;
const TURRET_CRUMBLE: f64 = 0.5;
/// `/obj/projectile/beam/legion`: damage and range.
const PULSE_DAMAGE: f32 = 19.0;
const PULSE_RANGE: f32 = 6.0;
/// `FACTION_MINING`: the sentinels spare it.
const FACTION_MINING: &str = "mining";

#[derive(Debug, Default)]
pub struct State {
    /// `LEGION_LARGE` 3, `LEGION_MEDIUM` 2, `LEGION_SMALL` 1.
    pub size: u8,
    /// The charge's speed-up lasts until then.
    pub charging_until: f64,
    /// It is flying at its target.
    pub thrown: bool,
}

#[derive(Debug, PartialEq)]
pub enum Step {
    /// `throw_thyself`.
    Throw { target: Entity },
    /// One tile of the throw.
    Fly { dest: TilePos, left: u32 },
    /// `reset_charge`.
    ResetCharge,
}

/// A Legion sentinel.
#[derive(Component, Clone, Copy, Debug)]
pub struct Sentinel {
    /// The next phase runs then.
    pub at: f64,
    /// Aimed (`fire`): the pulse's angle.
    pub angle: Option<f32>,
    /// Fired: it crumbles at `at`.
    pub fired: bool,
}

/// The size of a legion of `path`.
pub fn size_of(path: &str) -> u8 {
    use ss13_mapload::fidelity::is_under;
    if is_under(path, SMALL) {
        1
    } else if is_under(path, MEDIUM) {
        2
    } else {
        3
    }
}

fn state(world: &World, mob: Entity) -> Option<&State> {
    world.get::<Megafauna>(mob).map(|m| &m.legion)
}

/// Flying at its target: the AI neither walks nor strikes.
pub fn flying(world: &World, mob: Entity) -> bool {
    state(world, mob).is_some_and(|s| s.thrown)
}

/// `minimum_distance`: 5, none while charging.
pub(super) fn minimum_distance(world: &World, mob: Entity, now: f64) -> u32 {
    if state(world, mob).is_some_and(|s| now < s.charging_until) {
        1
    } else {
        KEEP_AWAY
    }
}

/// `legion/OpenFire`.
pub(super) fn open_fire(world: &mut World, mob: Entity, target: Entity, now: f64) {
    let Some(m) = world.get::<Megafauna>(mob) else {
        return;
    };
    if !m.ability_available(now) {
        return;
    }
    if let Some(mut m) = world.get_mut::<Megafauna>(mob) {
        m.ranged_ready = now + RANGED_COOLDOWN;
    }
    let cooldown = match roll(world, |rng| rng.range(0, 4)) {
        0..=2 => {
            create_skull(world, mob, Some(target));
            SKULL_COOLDOWN
        }
        3 => {
            chase(world, mob, target, now);
            CHASE_COOLDOWN
        }
        _ => {
            create_turrets(world, mob);
            TURRET_COOLDOWN
        }
    };
    if let Some(mut m) = world.get_mut::<Megafauna>(mob) {
        m.ability_ready = now + cooldown;
    }
}

/// `create_legion_skull/create`: a skull on its tile with its faction, after `target`.
fn create_skull(world: &mut World, mob: Entity, target: Option<Entity>) -> Option<Entity> {
    let pos = *world.get::<TilePos>(mob)?;
    let brood = crate::mobs::spawn::spawn_mob(world, crate::mobs::lavaland::LEGION_BROOD, pos)?;
    if let Some(faction) = world.get::<crate::mobs::Faction>(mob).cloned() {
        world.entity_mut(brood).insert(faction);
    }
    if let (Some(target), Some(mut ai)) = (
        target.filter(|&t| world.get_entity(t).is_ok()),
        world.get_mut::<crate::mobs::ai::AiController>(brood),
    ) {
        ai.bb.set(
            crate::mobs::ai::Key::CurrentTarget,
            crate::mobs::ai::Value::Entity(target),
        );
    }
    Some(brood)
}

/// `chase_target/charge`.
fn chase(world: &mut World, mob: Entity, target: Entity, now: f64) {
    if world.get::<Stat>(target) == Some(&Stat::Dead) {
        return;
    }
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    let name = crate::security::name_of(world, mob);
    show(world, pos, Span::BoldWarning, format!("{name} charges!"));
    if let Some(mut m) = world.get_mut::<Megafauna>(mob) {
        m.legion.charging_until = now + CHASE_LENGTH;
    }
    if let Some(mut mover) = world.get_mut::<Mover>(mob) {
        mover.delay = CHARGE_MOVE_DELAY;
    }
    schedule(
        world,
        mob,
        now + CHASE_LENGTH,
        MegaStep::Legion(Step::ResetCharge),
    );
    schedule(
        world,
        mob,
        now + CHASE_THROW_AFTER,
        MegaStep::Legion(Step::Throw { target }),
    );
}

pub(super) fn run(world: &mut World, mob: Entity, at: f64, step: Step) {
    match step {
        Step::Throw { target } => throw_thyself(world, mob, at, target),
        Step::Fly { dest, left } => fly(world, mob, at, dest, left),
        Step::ResetCharge => {
            let delay = world
                .get::<crate::mobs::BasicMob>(mob)
                .map(|b| b.def.speed.max(0.0) / 10.0);
            if let (Some(delay), Some(mut mover)) = (delay, world.get_mut::<Mover>(mob)) {
                mover.delay = delay;
            }
        }
    }
}

fn throw_thyself(world: &mut World, mob: Entity, at: f64, target: Entity) {
    let (Some(&pos), Some(&dest)) = (world.get::<TilePos>(mob), world.get::<TilePos>(target))
    else {
        return;
    };
    if pos.z != dest.z {
        return;
    }
    sound(world, pos, JACKHAMMER, 50.0, 0.0);
    super::stop_moving(world, mob);
    if let Some(mut m) = world.get_mut::<Megafauna>(mob) {
        m.legion.thrown = true;
    }
    schedule(
        world,
        mob,
        at + THROW_STEP,
        MegaStep::Legion(Step::Fly {
            dest,
            left: THROW_RANGE,
        }),
    );
}

/// One tile of the throw: a living mob in the way is slammed (`throw_impact`), anything dense
/// stops it; it lands with a boom (the throw's callback).
fn fly(world: &mut World, mob: Entity, at: f64, dest: TilePos, left: u32) {
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    let next = (left > 0 && pos != dest)
        .then(|| crate::mobs::ai::movement::dir_towards(pos, dest))
        .flatten()
        .map(|d| pos.step(d))
        .filter(|&p| world.resource::<SpatialIndex>().in_bounds(p));
    let Some(next) = next else {
        land(world, mob, pos);
        return;
    };
    let blockers: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .dense_on_tile(next)
        .iter()
        .copied()
        .filter(|&e| e != mob && world.get::<Density>(e).is_some())
        .collect();
    if !blockers.is_empty() {
        if let Some(victim) = blockers
            .into_iter()
            .find(|&e| world.get::<Health>(e).is_some() && world.get::<Stat>(e).is_some())
        {
            impact(world, mob, pos, victim);
        }
        land(world, mob, pos);
        return;
    }
    world.entity_mut(mob).insert(next);
    schedule(
        world,
        mob,
        at + THROW_STEP,
        MegaStep::Legion(Step::Fly {
            dest,
            left: left - 1,
        }),
    );
}

/// `legion/throw_impact`: `22 * size / 2` damage and a 2-tile knockback.
fn impact(world: &mut World, mob: Entity, from: TilePos, victim: Entity) {
    let size = state(world, mob).map_or(3, |s| s.size);
    let Some(&at) = world.get::<TilePos>(victim) else {
        return;
    };
    sound(world, at, ATTACK_SOUND, 100.0, 0.0);
    hurt(
        world,
        victim,
        DamageType::Brute,
        22.0 * f32::from(size) / 2.0,
        None,
    );
    let dir = crate::pull::dir_between(from, at).unwrap_or(Dir::South);
    let (dx, dy) = dir.offset();
    let to = TilePos::new(at.x + dx * 2, at.y + dy * 2, at.z);
    crate::throw::throw_at(world, victim, to, 2, 1.0);
}

fn land(world: &mut World, mob: Entity, pos: TilePos) {
    let size = state(world, mob).map_or(3, |s| s.size);
    if let Some(mut m) = world.get_mut::<Megafauna>(mob) {
        m.legion.thrown = false;
    }
    sound(world, pos, METEOR_IMPACT, 50.0 * f32::from(size), 2.0);
}

/// `create_legion_turrets/create`: 2 to `size * 2` sentinels on free turfs within 4 tiles.
fn create_turrets(world: &mut World, mob: Entity) {
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    let size = state(world, mob).map_or(3, |s| s.size);
    sound(world, pos, RATTLE, 100.0, 0.0);
    let mut free: Vec<TilePos> = {
        let w: &World = world;
        let index = w.resource::<SpatialIndex>();
        let r = TURRET_PLACE_RANGE;
        (-r..=r)
            .flat_map(|dx| (-r..=r).map(move |dy| (dx, dy)))
            .filter(|&d| d != (0, 0))
            .map(|(dx, dy)| TilePos::new(pos.x + dx, pos.y + dy, pos.z))
            .filter(|&p| index.in_bounds(p) && index.dense_on_tile(p).is_empty())
            .filter(|&p| crate::mobs::ai::behaviors::can_see(w, pos, p, TURRET_PLACE_RANGE as u32))
            .collect()
    };
    let max = u32::from(size) * 2;
    let count = roll(world, |rng| rng.range(2, max.max(2))) as usize;
    let now = super::now(world);
    for _ in 0..count.min(free.len()) {
        let i = roll(world, |rng| rng.range(0, free.len() as u32 - 1)) as usize;
        let tile = free.swap_remove(i);
        if let Some(turret) = crate::construction::spawn_obj(world, TURRET, tile) {
            world.entity_mut(turret).insert(Sentinel {
                at: now + TURRET_AIM_AFTER,
                angle: None,
                fired: false,
            });
        }
    }
}

/// The sentinels' phases: aim (`set_up_shot`), fire (`fire_beam`), crumble.
// Cost: per tick, one pass over the few live sentinels; a sight scan once per sentinel.
pub(super) fn sentinels(world: &mut World) {
    let now = super::now(world);
    let due: Vec<(Entity, Sentinel)> = world
        .query::<(Entity, &Sentinel)>()
        .iter(world)
        .filter(|(_, s)| now >= s.at)
        .map(|(e, s)| (e, *s))
        .collect();
    for (turret, s) in due {
        let Some(&pos) = world.get::<TilePos>(turret) else {
            continue;
        };
        if s.fired {
            world.despawn(turret);
            continue;
        }
        match s.angle {
            None => {
                let angle = match sentinel_target(world, turret, pos) {
                    Some(to) => crate::projectile::angle_between(pos, to),
                    None => roll(world, |rng| {
                        [0.0, 180.0, 90.0, 270.0][rng.range(0, 3) as usize]
                    }),
                };
                sound(world, pos, AIRLOCK, 100.0, 0.0);
                world.entity_mut(turret).insert(Sentinel {
                    at: s.at + TURRET_SHOT_DELAY,
                    angle: Some(angle),
                    fired: false,
                });
            }
            Some(angle) => {
                for tile in pulse_tiles(pos, angle) {
                    for victim in living_on(world, tile) {
                        hurt(world, victim, DamageType::Burn, PULSE_DAMAGE, None);
                    }
                }
                sound(world, pos, BIN_CLOSE, 100.0, 0.0);
                world.entity_mut(turret).insert(Sentinel {
                    at: s.at + TURRET_CRUMBLE,
                    angle: Some(angle),
                    fired: true,
                });
            }
        }
    }
}

/// `set_up_shot`: the first conscious living mob in sight within 9 tiles not of the mining
/// faction.
fn sentinel_target(world: &World, turret: Entity, pos: TilePos) -> Option<TilePos> {
    let index = world.resource::<SpatialIndex>();
    let mut found: Vec<(u64, TilePos)> = index
        .in_radius(pos, TURRET_SIGHT)
        .filter(|&e| e != turret && world.get::<Health>(e).is_some())
        .filter(|&e| !crate::mobs::lavaland::abilities::out_cold(world, e))
        .filter(|&e| {
            !world
                .get::<crate::mobs::Faction>(e)
                .is_some_and(|f| f.0.iter().any(|f| f == FACTION_MINING))
        })
        .filter(|&e| world.get::<crate::ghost::Ghost>(e).is_none())
        .filter_map(|e| {
            let p = *world.get::<TilePos>(e)?;
            (p != pos && crate::mobs::ai::behaviors::can_see(world, pos, p, TURRET_SIGHT))
                .then_some((e.to_bits(), p))
        })
        .collect();
    found.sort_by_key(|&(bits, _)| bits);
    found.first().map(|&(_, p)| p)
}

/// The tiles a piercing hitscan pulse crosses from `from` along `angle` (degrees clockwise
/// from north), up to its range.
pub fn pulse_tiles(from: TilePos, angle: f32) -> Vec<TilePos> {
    let (sin, cos) = angle.to_radians().sin_cos();
    let mut out: Vec<TilePos> = Vec::new();
    let mut t = 0.25;
    while t <= PULSE_RANGE + 1e-3 {
        let tile = TilePos::new(
            from.x + (sin * t).round() as i32,
            from.y + (cos * t).round() as i32,
            from.z,
        );
        if tile != from && !out.contains(&tile) {
            out.push(tile);
        }
        t += 0.25;
    }
    out
}

/// `legion/AttackingTarget` after the bite: a human out cold (alive) is infested.
pub(super) fn after_attack(world: &mut World, mob: Entity, target: Entity) {
    let human = world.get::<crate::medical::wounds::Limbs>(target).is_some();
    let alive = world.get::<Stat>(target).is_some_and(|s| *s != Stat::Dead);
    if !human || !alive || !crate::mobs::lavaland::abilities::out_cold(world, target) {
        return;
    }
    if let Some(brood) = create_skull(world, mob, None) {
        crate::mobs::lavaland::abilities::infest(world, brood, target);
    }
}

/// `legion/death` -> `Split`: three smaller skulls; the last one alive drops the staff of
/// storms, another one a tendril chest one time in five, else bones.
pub(super) fn on_death(world: &mut World, mob: Entity, pos: TilePos) {
    let size = state(world, mob).map_or(3, |s| s.size);
    match size {
        3 => {
            for path in [MEDIUM_LEFT, MEDIUM_RIGHT, MEDIUM_EYE] {
                crate::mobs::spawn::spawn_mob(world, path, pos);
            }
        }
        2 => {
            for _ in 0..3 {
                crate::mobs::spawn::spawn_mob(world, SMALL, pos);
            }
        }
        _ => {}
    }
    let last = !world
        .query::<(Entity, &Megafauna, &Stat)>()
        .iter(world)
        .any(|(e, m, s)| e != mob && m.kind == Kind::Legion && *s != Stat::Dead);
    let loot = if last {
        STORM_STAFF
    } else if roll(world, |rng| rng.chance(20.0)) {
        TENDRIL_CHEST
    } else {
        BONES
    };
    crate::construction::spawn_obj(world, loot, pos);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_by_path() {
        assert_eq!(size_of(LEGION), 3);
        assert_eq!(size_of(MEDIUM_LEFT), 2);
        assert_eq!(size_of(MEDIUM_EYE), 2);
        assert_eq!(size_of(SMALL), 1);
    }

    #[test]
    fn pulse_runs_six_tiles_along_its_angle() {
        let from = TilePos::new(10, 10, 1);
        let east = pulse_tiles(from, 90.0);
        assert_eq!(east.len(), 6);
        assert_eq!(east.first(), Some(&TilePos::new(11, 10, 1)));
        assert_eq!(east.last(), Some(&TilePos::new(16, 10, 1)));
        let north = pulse_tiles(from, 0.0);
        assert!(north.iter().all(|p| p.x == 10 && p.y > 10 && p.y <= 16));
        assert!(!pulse_tiles(from, 45.0).contains(&from));
    }
}
