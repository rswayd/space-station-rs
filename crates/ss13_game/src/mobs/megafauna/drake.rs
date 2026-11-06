//! The ash drake, after tg `megafauna/drake.dm` and its abilities (`fire_breath.dm`,
//! `meteors.dm`, `lava_swoop.dm`):
//!
//! - fire cone: three lines of fire (-40, 0, 40 degrees off the target, 15 tiles, a tile every
//!   0.15 s, stopped by walls): 20 burn and a hotspot (`fire_act`: the victim catches fire), once
//!   per line per mob; half the time followed by meteors: on 11% of the turfs within 9 of the
//!   target a crosshair, and 0.9 s later a fireball (rock drilled, 10 burn);
//! - lava swoop (15% + anger): it rises (unhittable, not dense, unclickable), flies onto the
//!   target 20 tiles a second while lava pools (30 warnings around the target, 0.8 s apart, 10
//!   burn each after 1.3 s), lands 1 s later: 75 brute and a 3 tile throw to everyone around,
//!   then the fire cone and meteors;
//! - below half health the swoop ends in a lava arena instead: walls of fire 3 tiles out, rock
//!   inside drilled, three rounds of lava on every tile but one safe tile 2 tiles from each
//!   player inside; a player who escaped the arena enrages it (250 health back, double speed,
//!   then mass fire); enraged it also breathes mass fire (10% + anger): 12 lines all around,
//!   three spins 2.5 s apart.
//!
//! Anger is a point per 60 health lost (at most 20). Light explosions do nothing to it. Its
//! corpse stays; its chest holds one of a spectral blade, a lava staff, a sacred flame spellbook
//! or dragon's blood.
// shortcut: the lava is a visual (no lava turfs: who stands there later is not burned), the
// fire walls do not block air, mechs take no swoop, lava or fire damage, the rising and landing
// keep full alpha in between (no alpha animation).

use bevy::prelude::*;
use ss13_mapload::DmTurf;
use ss13_shared::animation::{Animation, AnimationStep, EaseCurve, Easing, TransformAnimation};
use ss13_shared::chat::Span;
use ss13_shared::{Dir, Facing, MouseOpacity, TilePos};
use ss13_world::{Density, SpatialIndex};

use super::{
    ABOVE_GAME_PLANE, BELOW_MOB_LAYER, FLY_LAYER, Kind, Megafauna, Step as MegaStep, Visual,
    kind_of, living_on, roll, schedule, shake, show, sound, spawn_visual, stop_moving, tell,
};
use crate::health::{DamageType, Health};
use crate::mobs::defense::hurt;

pub const CHEST: &str = "/obj/structure/closet/crate/necropolis/dragon";
/// `necropolis/dragon/PopulateContents`: one of four.
const LOOT: [&str; 4] = [
    "/obj/item/melee/ghost_sword",
    "/obj/item/lava_staff",
    "/obj/item/book/granter/action/spell/sacredflame",
    "/obj/item/dragons_blood",
];
const FIREBALL_SOUND: &str = "sound/effects/magic/fireball.ogg";
const WARN_SOUND: &str = "sound/effects/magic/fleshtostone.ogg";
const IMPACT_SOUND: &str = "sound/effects/meteorimpact.ogg";
/// `SFX_EXPLOSION`.
const EXPLOSION_SFX: &str = "explosion";
const EFFECTS_DMI: &str = "icons/effects/effects.dmi";
const FIRE_DMI: &str = "icons/effects/fire.dmi";

/// `fire_breath`: `fire_range`, `fire_delay`, `DRAKE_FIRE_TEMP`, `DRAKE_FIRE_EXPOSURE`,
/// `fire_damage`; the cone's `angles`.
const FIRE_RANGE: i32 = 15;
const FIRE_DELAY: f64 = 0.15;
const FIRE_TEMP: f64 = 500.0;
const FIRE_VOLUME: f64 = 50.0;
const FIRE_DAMAGE: f32 = 20.0;
const CONE: [f32; 3] = [-40.0, 0.0, 40.0];
/// `cooldown_time` of the fire breath, meteors, mass fire and lava swoop.
const BREATH_COOLDOWN: f64 = 3.0;
const METEORS_COOLDOWN: f64 = 3.0;
const MASS_FIRE_COOLDOWN: f64 = 10.5;
const SWOOP_COOLDOWN: f64 = 4.0;
/// `dragon/lesser/Initialize`: `lava_swoop.cooldown_time`.
const LESSER_SWOOP_COOLDOWN: f64 = 20.0;
/// `mass_fire`: `sectors`, `breath_delay`, `total_spins`.
const SECTORS: u32 = 12;
const SPIN_DELAY: f64 = 2.5;
const SPINS: u32 = 3;
/// `create_meteors`: the radius and chance; `temp_visual/target` `duration`, its fire damage.
const METEOR_RANGE: i32 = 9;
const METEOR_CHANCE: f32 = 11.0;
const METEOR_FALL: f64 = 0.9;
const STRIKE_DAMAGE: f32 = 10.0;
/// `temp_visual/lava_warning` `duration`; how long the lava stays (`reset_time`) for pools and
/// the arena.
const LAVA_FALL: f64 = 1.3;
const POOL_RESET: f64 = 6.0;
const ARENA_RESET: f64 = 1.0;
/// `lava_pools`: `amount`, `delay`.
const POOLS: u32 = 30;
const POOL_DELAY: f64 = 0.8;
/// `swoop_attack`: rising (0.3 + 0.7 s), a tile per 0.05 s, `descentTime`, the landing.
const SWOOP_RISE: f64 = 1.0;
const CHASE_STEP: f64 = 0.05;
const DESCENT: f64 = 1.0;
const SWOOP_DAMAGE: f32 = 75.0;
const SWOOP_THROW: i32 = 3;
/// `lava_arena`: rounds, the pause after each, the fire walls' `duration`.
const ARENA_ROUNDS: u32 = 3;
const ARENA_DELAY: f64 = 2.4;
const ARENA_WALL_SECS: f64 = 8.2;
/// `COLOR_DARK_ORANGE`.
const DARK_ORANGE: [u8; 4] = [0xC3, 0x63, 0x0C, 0xFF];
/// `arena_escape_enrage`.
const ESCAPE_HEAL: f32 = 250.0;
const ESCAPE_SECS: f64 = 5.0;
const ESCAPE_MASS_FIRE_COOLDOWN: f64 = 8.0;

#[derive(Debug, Default)]
pub struct State {
    /// `dragon/lesser`: no meteors, no mass fire, never the arena, a 20 s swoop cooldown.
    pub lesser: bool,
    /// `swooping`: in the air (no AI, no melee, no damage).
    pub swooping: bool,
    saved: Option<Saved>,
    /// The arena's fire walls.
    walls: Vec<Entity>,
    /// `move_to_delay` before `arena_escape_enrage` halved it.
    escape_delay: Option<f32>,
}

/// What the swoop changes: `TRAIT_GODMODE` (physiology), `TRAIT_UNDENSE`, the alpha and
/// `mouse_opacity`.
#[derive(Clone, Copy, Debug)]
struct Saved {
    physiology: Option<[f32; 5]>,
    dense: bool,
    color: Option<[u8; 4]>,
    opacity: MouseOpacity,
}

/// One line of fire still burning its way out: the tiles left (the next one last) and who it
/// already burned (`hit_list`).
#[derive(Clone, Debug, PartialEq)]
pub struct FireLine {
    tiles: Vec<TilePos>,
    hit: Vec<Entity>,
}

#[derive(Debug, PartialEq)]
pub enum Step {
    Fire(FireLine),
    /// `fire_spin` number `n` (from 1) towards the turf that was ahead of it.
    Spin {
        toward: TilePos,
        n: u32,
    },
    /// `lava_pools`: `left` more warnings around the target.
    LavaPool {
        target: Entity,
        left: u32,
    },
    /// `swoop_attack`'s flight onto the target's turf.
    Chase {
        target: Entity,
        arena: bool,
    },
    /// One round of `lava_arena`; `seen` the players met inside.
    ArenaRound {
        center: TilePos,
        left: u32,
        seen: Vec<Entity>,
    },
    Descend {
        target: Entity,
        arena: bool,
        failed: bool,
    },
    Impact {
        target: Entity,
        arena: bool,
        failed: bool,
    },
    SwoopEnd {
        target: Entity,
        arena: bool,
        failed: bool,
    },
    /// `arena_escape_enrage` 5 s later.
    EscapeEnd,
}

/// What falls on a turf after a warning: a meteor (`temp_visual/target`) or lava
/// (`temp_visual/lava_warning`, the lava staying `reset` seconds).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StrikeKind {
    Meteor,
    Lava { reset: f64 },
}

/// A pending strike; it lands at `at` even if the drake died.
#[derive(Component, Clone, Copy, Debug)]
pub struct Strike {
    pub at: f64,
    pub pos: TilePos,
    pub kind: StrikeKind,
    pub owner: Entity,
}

/// In the air (`swooping`).
pub fn swooping(world: &World, mob: Entity) -> bool {
    world
        .get::<Megafauna>(mob)
        .is_some_and(|m| m.drake.swooping)
}

/// BYOND `ATAN2`-based `get_ranged_target_turf_direct`: the last turf within `range` along the
/// angle from `from` to `toward` turned by `offset` degrees (counterclockwise), stopping at the
/// map's edge.
pub fn ranged_target_turf(
    in_bounds: impl Fn(TilePos) -> bool,
    from: TilePos,
    toward: TilePos,
    range: i32,
    offset: f32,
) -> TilePos {
    let dx = f64::from(toward.x - from.x);
    let dy = f64::from(toward.y - from.y);
    let angle = dy.atan2(dx).to_degrees() + f64::from(offset);
    let (sin, cos) = angle.to_radians().sin_cos();
    let mut out = from;
    for i in 1..=range {
        let i = f64::from(i);
        let p = TilePos::new(
            (f64::from(from.x) + cos * i) as i32,
            (f64::from(from.y) + sin * i) as i32,
            from.z,
        );
        if !in_bounds(p) {
            break;
        }
        out = p;
    }
    out
}

/// tg `get_line`: Bresenham from `from` to `to`, both included.
pub fn get_line(from: TilePos, to: TilePos) -> Vec<TilePos> {
    let (mut cx, mut cy) = (from.x, from.y);
    let mut line = vec![from];
    let (dx, dy) = (to.x - from.x, to.y - from.y);
    let (ax, ay) = (dx.abs(), dy.abs());
    let (sx, sy) = (dx.signum(), dy.signum());
    let (mut x, mut y) = (ax >> 1, ay >> 1);
    if ax >= ay {
        for _ in 0..ax {
            y += ay;
            if y >= ax {
                y -= ax;
                cy += sy;
            }
            cx += sx;
            line.push(TilePos::new(cx, cy, from.z));
        }
    } else {
        for _ in 0..ay {
            x += ax;
            if x >= ay {
                x -= ay;
                cx += sx;
            }
            cy += sy;
            line.push(TilePos::new(cx, cy, from.z));
        }
    }
    line
}

/// `/mob/living/simple_animal/hostile/megafauna/dragon/OpenFire`.
pub(super) fn open_fire(world: &mut World, mob: Entity, target: Entity, now: f64) {
    if swooping(world, mob) {
        return;
    }
    let Some(health) = world.get::<Health>(mob).cloned() else {
        return;
    };
    let (hp, max) = (health.health(), health.max);
    let anger = ((max - hp) / 60.0).clamp(0.0, 20.0);
    // The lesser drake has neither meteors nor mass fire, and its swoop is never `enraged`.
    let lesser = world.get::<Megafauna>(mob).is_some_and(|m| m.drake.lesser);
    let enraged = hp < max * 0.5 && !lesser;
    let available = world
        .get::<Megafauna>(mob)
        .is_some_and(|m| m.ability_available(now));
    let (swoop, mass, then_meteors) = roll(world, |rng| {
        (
            rng.chance(15.0 + anger),
            rng.chance(10.0 + anger),
            rng.chance(50.0),
        )
    });
    if swoop {
        if enraged {
            // Lava arena.
            if available {
                start_swoop(world, mob, target, true, now);
            }
            return;
        }
        // Lava pools, then the fire cone and meteors when it lands.
        if available {
            start_swoop(world, mob, target, false, now);
            return;
        }
    } else if mass && enraged {
        if available {
            mass_fire(world, mob, now);
        }
        return;
    }
    if available {
        fire_cone(world, mob, target, now);
        if then_meteors && !lesser {
            meteors(world, mob, target, now);
        }
    }
}

/// `fire_breath/cone`: three lines at the target.
fn fire_cone(world: &mut World, mob: Entity, target: Entity, now: f64) {
    let (Some(&pos), Some(&toward)) = (world.get::<TilePos>(mob), world.get::<TilePos>(target))
    else {
        return;
    };
    sound(world, pos, FIREBALL_SOUND, 200.0, 0.0);
    for offset in CONE {
        if let Some(line) = fire_line(world, mob, toward, offset) {
            burn_next(world, mob, now, line);
        }
    }
    if let Some(mut m) = world.get_mut::<Megafauna>(mob) {
        m.cooldown(now, BREATH_COOLDOWN);
    }
}

/// `fire_line`: the turfs from the drake (excluded) towards `toward` turned by `offset`.
fn fire_line(world: &World, mob: Entity, toward: TilePos, offset: f32) -> Option<FireLine> {
    let &from = world.get::<TilePos>(mob)?;
    let index = world.resource::<SpatialIndex>();
    let end = ranged_target_turf(|p| index.in_bounds(p), from, toward, FIRE_RANGE, offset);
    let mut tiles: Vec<TilePos> = get_line(from, end)
        .into_iter()
        .filter(|&p| p != from)
        .collect();
    tiles.reverse();
    Some(FireLine {
        tiles,
        hit: vec![mob],
    })
}

/// `progressive_fire_line`: the next turf burns unless it is blocked (the line ends there),
/// the rest follows `fire_delay` later.
fn burn_next(world: &mut World, mob: Entity, at: f64, mut line: FireLine) {
    let Some(pos) = line.tiles.pop() else {
        return;
    };
    if blocked(world, pos) {
        return;
    }
    burn_turf(world, mob, pos, &mut line.hit);
    if !line.tiles.is_empty() {
        schedule(
            world,
            mob,
            at + FIRE_DELAY,
            MegaStep::Drake(Step::Fire(line)),
        );
    }
}

/// `is_blocked_turf(exclude_mobs = TRUE)`: a dense turf or a dense object.
fn blocked(world: &World, pos: TilePos) -> bool {
    let index = world.resource::<SpatialIndex>();
    !index.in_bounds(pos)
        || index
            .dense_on_tile(pos)
            .iter()
            .any(|&e| world.get::<Health>(e).is_none())
}

/// `burn_turf`: a hotspot, and the breath's burn on whoever it has not hit yet.
fn burn_turf(world: &mut World, mob: Entity, pos: TilePos, hit: &mut Vec<Entity>) {
    hotspot(world, pos);
    let name = crate::security::name_of(world, mob);
    for victim in living_on(world, pos) {
        if hit.contains(&victim) {
            continue;
        }
        hit.push(victim);
        tell(
            world,
            victim,
            format!("Вас подожгло огненное дыхание {name}!"),
        );
        hurt(world, victim, DamageType::Burn, FIRE_DAMAGE, None);
        crate::fire::fire_act(world, victim, FIRE_TEMP);
    }
}

/// `new /obj/effect/hotspot(turf)` + `hotspot_expose`: the fire's look, and a real fire where
/// the air burns.
fn hotspot(world: &mut World, pos: TilePos) {
    spawn_visual(
        world,
        pos,
        Visual {
            layer: FLY_LAYER,
            plane: ABOVE_GAME_PLANE,
            ..Visual::new(FIRE_DMI, "light", 1.0)
        },
    );
    if let Some(mut atmos) = world.get_resource_mut::<ss13_atmos::Atmos>() {
        atmos.hotspot_expose(pos, FIRE_TEMP, FIRE_VOLUME, true);
    }
}

/// `meteors/create_meteors` around the target.
fn meteors(world: &mut World, mob: Entity, target: Entity, now: f64) {
    let Some(&center) = world.get::<TilePos>(target) else {
        return;
    };
    show(
        world,
        center,
        Span::BoldWarning,
        "Fire rains from the sky!".to_owned(),
    );
    let tiles: Vec<TilePos> = roll(world, |rng| {
        let mut out = Vec::new();
        for dy in -METEOR_RANGE..=METEOR_RANGE {
            for dx in -METEOR_RANGE..=METEOR_RANGE {
                if rng.chance(METEOR_CHANCE) {
                    out.push(TilePos::new(center.x + dx, center.y + dy, center.z));
                }
            }
        }
        out
    });
    let index = world.resource::<SpatialIndex>();
    let tiles: Vec<TilePos> = tiles.into_iter().filter(|&p| index.in_bounds(p)).collect();
    for pos in tiles {
        spawn_strike(world, pos, StrikeKind::Meteor, mob);
    }
    if let Some(mut m) = world.get_mut::<Megafauna>(mob) {
        m.cooldown(now, METEORS_COOLDOWN);
    }
}

/// `fire_breath/mass_fire`: three spins at the turf ahead of it.
fn mass_fire(world: &mut World, mob: Entity, now: f64) {
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    let facing = world.get::<Facing>(mob).map_or(Dir::South, |f| f.0);
    let toward = pos.step(facing);
    if let Some(mut m) = world.get_mut::<Megafauna>(mob) {
        for n in 1..=SPINS {
            m.at(
                now + f64::from(n - 1) * SPIN_DELAY,
                MegaStep::Drake(Step::Spin { toward, n }),
            );
        }
        m.cooldown(now, MASS_FIRE_COOLDOWN);
    }
}

/// `fire_spin`: twelve lines, turned half a sector more each spin.
fn spin(world: &mut World, mob: Entity, at: f64, toward: TilePos, n: u32) {
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    sound(world, pos, FIREBALL_SOUND, 200.0, 0.0);
    let increment = 360.0 / SECTORS as f32;
    let additional = n as f32 * increment / 2.0;
    for i in 1..=SECTORS {
        if let Some(line) = fire_line(world, mob, toward, increment * i as f32 + additional) {
            burn_next(world, mob, at, line);
        }
    }
}

/// `lava_swoop/swoop_attack` (with `lava_pools` or `lava_arena`).
fn start_swoop(world: &mut World, mob: Entity, target: Entity, arena: bool, now: f64) {
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    let name = crate::security::name_of(world, mob);
    show(
        world,
        pos,
        Span::BoldWarning,
        format!("{name} swoops up high!"),
    );
    if let Some(icon) = world
        .get::<crate::mobs::BasicMob>(mob)
        .map(|b| b.def.icon.clone())
    {
        crate::mobs::set_icon_state(world, mob, &icon, "dragon_shadow");
    }
    stop_moving(world, mob);
    let saved = Saved {
        physiology: world.get::<Health>(mob).and_then(|h| h.basic),
        dense: world.get::<Density>(mob).is_some(),
        color: world
            .get::<ss13_shared::Appearance>(mob)
            .and_then(|a| a.color),
        opacity: world
            .get::<ss13_shared::Appearance>(mob)
            .map_or(MouseOpacity::Opaque, |a| a.mouse_opacity),
    };
    // `TRAIT_GODMODE`, `TRAIT_UNDENSE`, `mouse_opacity = MOUSE_OPACITY_TRANSPARENT`.
    if let Some(mut health) = world.get_mut::<Health>(mob)
        && health.basic.is_some()
    {
        health.basic = Some([0.0; 5]);
    }
    world.entity_mut(mob).remove::<Density>();
    if let Some(mut look) = world.get_mut::<ss13_shared::Appearance>(mob) {
        look.mouse_opacity = MouseOpacity::Transparent;
    }
    let scale = |s: f32, time_ds: f32, easing: Easing| AnimationStep {
        offset: [0.0, 0.0],
        turn: 0.0,
        scale: [s, s],
        time_ds,
        easing,
    };
    crate::animation::play(
        world,
        mob,
        Animation::Transform(TransformAnimation {
            steps: vec![
                scale(0.9, 3.0, Easing::of(EaseCurve::Bounce)),
                scale(0.7, 7.0, Easing::LINEAR),
                // Held until it lands (`StopTransforms` then).
                scale(0.7, 600.0, Easing::LINEAR),
            ],
            loops: 1,
        }),
    );
    if let Some(mut m) = world.get_mut::<Megafauna>(mob) {
        m.drake.swooping = true;
        m.drake.saved = Some(saved);
        m.busy_until = f64::INFINITY;
        if !arena {
            m.at(
                now,
                MegaStep::Drake(Step::LavaPool {
                    target,
                    left: POOLS,
                }),
            );
        }
        m.at(
            now + SWOOP_RISE,
            MegaStep::Drake(Step::Chase { target, arena }),
        );
    }
}

pub(super) fn run(world: &mut World, mob: Entity, at: f64, step: Step) {
    match step {
        Step::Fire(line) => burn_next(world, mob, at, line),
        Step::Spin { toward, n } => spin(world, mob, at, toward, n),
        Step::LavaPool { target, left } => lava_pool(world, mob, at, target, left),
        Step::Chase { target, arena } => chase(world, mob, at, target, arena),
        Step::ArenaRound { center, left, seen } => arena_round(world, mob, at, center, left, seen),
        Step::Descend {
            target,
            arena,
            failed,
        } => descend(world, mob, at, target, arena, failed),
        Step::Impact {
            target,
            arena,
            failed,
        } => impact(world, mob, at, target, arena, failed),
        Step::SwoopEnd {
            target,
            arena,
            failed,
        } => swoop_end(world, mob, at, target, arena, failed),
        Step::EscapeEnd => escape_end(world, mob, at),
    }
}

/// One warning of `lava_pools` on a turf next to the target.
fn lava_pool(world: &mut World, mob: Entity, at: f64, target: Entity, left: u32) {
    let Some(&center) = world.get::<TilePos>(target) else {
        return;
    };
    if left == POOLS {
        show(
            world,
            center,
            Span::BoldWarning,
            "Lava starts to pool up around you!".to_owned(),
        );
    }
    let (dx, dy) = roll(world, |rng| {
        (rng.range(0, 2) as i32 - 1, rng.range(0, 2) as i32 - 1)
    });
    let pos = TilePos::new(center.x + dx, center.y + dy, center.z);
    if world.resource::<SpatialIndex>().in_bounds(pos) {
        spawn_strike(world, pos, StrikeKind::Lava { reset: POOL_RESET }, mob);
    }
    if left > 1 {
        schedule(
            world,
            mob,
            at + POOL_DELAY,
            MegaStep::Drake(Step::LavaPool {
                target,
                left: left - 1,
            }),
        );
    }
}

/// A step of the flight onto the target's turf (`forceMove`, through anything).
fn chase(world: &mut World, mob: Entity, at: f64, target: Entity, arena: bool) {
    let (Some(&pos), to) = (
        world.get::<TilePos>(mob),
        world.get::<TilePos>(target).copied(),
    ) else {
        return;
    };
    let next = to
        .filter(|t| t.z == pos.z && *t != pos)
        .and_then(|t| crate::mobs::ai::movement::dir_towards(pos, t));
    if let Some(dir) = next {
        world.entity_mut(mob).insert(pos.step(dir));
        schedule(
            world,
            mob,
            at + CHASE_STEP,
            MegaStep::Drake(Step::Chase { target, arena }),
        );
        return;
    }
    if arena {
        arena_start(world, mob, at, target);
    } else {
        schedule(
            world,
            mob,
            at,
            MegaStep::Drake(Step::Descend {
                target,
                arena,
                failed: false,
            }),
        );
    }
}

/// The square of radius `r` around `center`, on the map.
fn square(world: &World, center: TilePos, r: i32) -> Vec<TilePos> {
    let index = world.resource::<SpatialIndex>();
    let mut out = Vec::new();
    for dy in -r..=r {
        for dx in -r..=r {
            let p = TilePos::new(center.x + dx, center.y + dy, center.z);
            if index.in_bounds(p) {
                out.push(p);
            }
        }
    }
    out
}

/// `lava_arena`'s start: fire walls 3 tiles out, the rock inside drilled. A target that is not
/// a living mob here fails the arena (tg returns no success).
fn arena_start(world: &mut World, mob: Entity, at: f64, target: Entity) {
    let (Some(&center), to) = (
        world.get::<TilePos>(mob),
        world.get::<TilePos>(target).copied(),
    ) else {
        return;
    };
    if to.is_none_or(|t| t.z != center.z) || world.get::<Health>(target).is_none() {
        schedule(
            world,
            mob,
            at,
            MegaStep::Drake(Step::Descend {
                target,
                arena: true,
                failed: true,
            }),
        );
        return;
    }
    let name = crate::security::name_of(world, mob);
    tell(
        world,
        target,
        format!("{name} encases you in an arena of fire!"),
    );
    let mut walls = Vec::new();
    for pos in square(world, center, 3) {
        if pos.chebyshev(center) != Some(3) {
            continue;
        }
        let wall = spawn_visual(
            world,
            pos,
            Visual {
                color: Some(DARK_ORANGE),
                ..Visual::new(FIRE_DMI, "light", ARENA_WALL_SECS)
            },
        )
        .unwrap_or_else(|| {
            world
                .spawn((
                    Name::new("Fire Barrier"),
                    pos,
                    super::TempVisual {
                        until: at + ARENA_WALL_SECS,
                    },
                ))
                .id()
        });
        world.entity_mut(wall).insert(Density);
        walls.push(wall);
    }
    for pos in square(world, center, 2) {
        crate::mining::rocks::gets_drilled(world, pos, Some(mob));
    }
    if let Some(mut m) = world.get_mut::<Megafauna>(mob) {
        m.drake.walls = walls;
        m.at(
            at + 1.0,
            MegaStep::Drake(Step::ArenaRound {
                center,
                left: ARENA_ROUNDS,
                seen: Vec::new(),
            }),
        );
    }
}

/// A round of `lava_arena`: lava everywhere but a safe tile 2 tiles from each player inside;
/// nobody inside ends it (failed when a player met inside is still around).
fn arena_round(
    world: &mut World,
    mob: Entity,
    at: f64,
    center: TilePos,
    left: u32,
    mut seen: Vec<Entity>,
) {
    let turfs = square(world, center, 2);
    let mut safe: Vec<TilePos> = Vec::new();
    let mut any = false;
    for &t in &turfs {
        for victim in living_on(world, t) {
            if world.get::<crate::server::PlayerMob>(victim).is_none() {
                continue;
            }
            let options: Vec<TilePos> = turfs
                .iter()
                .copied()
                .filter(|p| p.chebyshev(t) == Some(2) && !safe.contains(p))
                .collect();
            if !options.is_empty() {
                let i = roll(world, |rng| rng.range(0, options.len() as u32 - 1)) as usize;
                safe.push(options[i]);
            }
            any = true;
            if !seen.contains(&victim) {
                seen.push(victim);
            }
        }
    }
    if !any {
        let walls = world
            .get_mut::<Megafauna>(mob)
            .map(|mut m| std::mem::take(&mut m.drake.walls))
            .unwrap_or_default();
        for wall in walls {
            if let Ok(e) = world.get_entity_mut(wall) {
                e.despawn();
            }
        }
        let failed = seen.iter().any(|&p| {
            world
                .get_entity(p)
                .is_ok_and(|e| e.contains::<crate::server::PlayerMob>())
        });
        let target = world
            .get::<Megafauna>(mob)
            .and_then(|m| m.target)
            .unwrap_or(Entity::PLACEHOLDER);
        schedule(
            world,
            mob,
            at,
            MegaStep::Drake(Step::Descend {
                target,
                arena: true,
                failed,
            }),
        );
        return;
    }
    for &t in &turfs {
        if safe.contains(&t) {
            spawn_visual(
                world,
                t,
                Visual {
                    layer: BELOW_MOB_LAYER,
                    ..Visual::new(
                        "icons/obj/service/hand_of_god_structures.dmi",
                        "trap-earth",
                        LAVA_FALL,
                    )
                },
            );
        } else {
            spawn_strike(world, t, StrikeKind::Lava { reset: ARENA_RESET }, mob);
        }
    }
    let target = world
        .get::<Megafauna>(mob)
        .and_then(|m| m.target)
        .unwrap_or(Entity::PLACEHOLDER);
    let next = if left > 1 {
        Step::ArenaRound {
            center,
            left: left - 1,
            seen,
        }
    } else {
        Step::Descend {
            target,
            arena: true,
            failed: false,
        }
    };
    schedule(world, mob, at + ARENA_DELAY, MegaStep::Drake(next));
}

/// The descent: the landing mark, the drake back to size.
fn descend(world: &mut World, mob: Entity, at: f64, target: Entity, arena: bool, failed: bool) {
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    spawn_visual(
        world,
        pos,
        Visual {
            layer: BELOW_MOB_LAYER,
            offset: [-32, -32],
            color: Some([255, 0, 0, 255]),
            ..Visual::new("icons/effects/96x96.dmi", "landing", DESCENT)
        },
    );
    crate::animation::play(world, mob, Animation::StopTransforms);
    crate::animation::play(
        world,
        mob,
        Animation::Transform(TransformAnimation {
            steps: vec![
                AnimationStep {
                    scale: [0.7, 0.7],
                    ..AnimationStep::rest(0.0)
                },
                AnimationStep::rest(DESCENT as f32 * 10.0),
            ],
            loops: 1,
        }),
    );
    schedule(
        world,
        mob,
        at + DESCENT,
        MegaStep::Drake(Step::Impact {
            target,
            arena,
            failed,
        }),
    );
}

/// The landing: 75 brute to everyone within a tile, thrown clear; the screens around shake.
fn impact(world: &mut World, mob: Entity, at: f64, target: Entity, arena: bool, failed: bool) {
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    sound(world, pos, IMPACT_SOUND, 200.0, 0.0);
    end_swoop_state(world, mob);
    // `end_swoop_state` keeps `swooping` until `SwoopEnd`.
    if let Some(mut m) = world.get_mut::<Megafauna>(mob) {
        m.drake.swooping = true;
    }
    let name = crate::security::name_of(world, mob);
    let victims: Vec<Entity> = square(world, pos, 1)
        .into_iter()
        .flat_map(|p| living_on(world, p))
        .filter(|&v| v != mob)
        .collect();
    for victim in victims {
        hurt(world, victim, DamageType::Brute, SWOOP_DAMAGE, None);
        let Some(&at_pos) = world.get::<TilePos>(victim) else {
            continue;
        };
        let dir = match crate::pull::dir_between(pos, at_pos) {
            Some(d) if at_pos != pos => d,
            _ => roll(world, |rng| Dir::DMI_ORDER[rng.range(0, 7) as usize]),
        };
        let (dx, dy) = dir.offset();
        let throw_to = TilePos::new(
            at_pos.x + dx * SWOOP_THROW,
            at_pos.y + dy * SWOOP_THROW,
            at_pos.z,
        );
        crate::throw::throw_at(world, victim, throw_to, SWOOP_THROW as u32, 1.0);
        let victim_name = crate::security::name_of(world, victim);
        show(
            world,
            pos,
            Span::Warning,
            format!("{victim_name} is thrown clear of {name}!"),
        );
    }
    shake(world, pos, 7, 15.0, 1.0);
    schedule(
        world,
        mob,
        at + 0.1,
        MegaStep::Drake(Step::SwoopEnd {
            target,
            arena,
            failed,
        }),
    );
}

/// The swoop is over: the ability's cooldown; after the pools the fire cone and meteors at
/// once (their cooldowns reset), after a failed arena the escape enrage.
fn swoop_end(world: &mut World, mob: Entity, at: f64, target: Entity, arena: bool, failed: bool) {
    let mut lesser = false;
    if let Some(mut m) = world.get_mut::<Megafauna>(mob) {
        m.drake.swooping = false;
        lesser = m.drake.lesser;
        m.cooldown(at, SWOOP_COOLDOWN);
    }
    crate::mobs::set_look(world, mob, false);
    if failed {
        escape_enrage(world, mob, at);
    }
    if !arena && world.get::<TilePos>(target).is_some() {
        fire_cone(world, mob, target, at);
        if !lesser {
            meteors(world, mob, target, at);
        }
    }
    // tg's fire cone right after the landing restarts the shared cooldown at its own 3 s, which
    // cancels the lesser drake's 20 s swoop cooldown; the port keeps the 20 s it sets.
    if lesser && let Some(mut m) = world.get_mut::<Megafauna>(mob) {
        m.cooldown(at, LESSER_SWOOP_COOLDOWN);
    }
}

/// `arena_escape_enrage`: healed, yellow, twice as fast; mass fire 5 s later.
fn escape_enrage(world: &mut World, mob: Entity, at: f64) {
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    let name = crate::security::name_of(world, mob);
    show(
        world,
        pos,
        Span::BoldWarning,
        format!("{name} starts to glow vibrantly as its wounds close up!"),
    );
    if let Some(mut health) = world.get_mut::<Health>(mob) {
        health.adjust(DamageType::Brute, -ESCAPE_HEAL);
    }
    if let Some(mut look) = world.get_mut::<ss13_shared::Appearance>(mob) {
        look.color = Some([255, 255, 0, 255]);
    }
    let delay = world.get::<ss13_world::Mover>(mob).map(|m| m.delay);
    if let (Some(delay), Some(mut mover)) = (delay, world.get_mut::<ss13_world::Mover>(mob)) {
        mover.delay = delay / 2.0;
    }
    if let Some(mut m) = world.get_mut::<Megafauna>(mob) {
        m.drake.escape_delay = m.drake.escape_delay.or(delay);
        m.at(at + ESCAPE_SECS, MegaStep::Drake(Step::EscapeEnd));
    }
}

fn escape_end(world: &mut World, mob: Entity, at: f64) {
    mass_fire(world, mob, at);
    let delay = world.get_mut::<Megafauna>(mob).and_then(|mut m| {
        m.ability_ready = at + ESCAPE_MASS_FIRE_COOLDOWN;
        m.drake.escape_delay.take()
    });
    if let (Some(delay), Some(mut mover)) = (delay, world.get_mut::<ss13_world::Mover>(mob)) {
        mover.delay = delay;
    }
    if let Some(mut look) = world.get_mut::<ss13_shared::Appearance>(mob) {
        look.color = None;
    }
}

/// Puts back what the swoop changed (the landing, or the drake's death mid-air) and drops the
/// arena's walls.
pub(super) fn end_swoop_state(world: &mut World, mob: Entity) {
    let Some((saved, walls, delay)) = world.get_mut::<Megafauna>(mob).map(|mut m| {
        m.drake.swooping = false;
        (
            m.drake.saved.take(),
            std::mem::take(&mut m.drake.walls),
            m.drake.escape_delay.take(),
        )
    }) else {
        return;
    };
    for wall in walls {
        if let Ok(e) = world.get_entity_mut(wall) {
            e.despawn();
        }
    }
    if let (Some(delay), Some(mut mover)) = (delay, world.get_mut::<ss13_world::Mover>(mob)) {
        mover.delay = delay;
    }
    let Some(saved) = saved else {
        return;
    };
    if let Some(mut health) = world.get_mut::<Health>(mob) {
        health.basic = saved.physiology;
    }
    if saved.dense {
        world.entity_mut(mob).insert(Density);
    }
    if let Some(mut look) = world.get_mut::<ss13_shared::Appearance>(mob) {
        look.color = saved.color;
        look.mouse_opacity = saved.opacity;
    }
}

/// A warning on `pos` and what falls there later (`temp_visual/target` with its fireball, or
/// `temp_visual/lava_warning`).
fn spawn_strike(world: &mut World, pos: TilePos, kind: StrikeKind, owner: Entity) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    sound(world, pos, WARN_SOUND, 80.0, 0.0);
    let fall = match kind {
        StrikeKind::Meteor => {
            spawn_visual(
                world,
                pos,
                Visual {
                    layer: BELOW_MOB_LAYER,
                    ..Visual::new(
                        "icons/mob/actions/actions_items.dmi",
                        "sniper_zoom",
                        METEOR_FALL,
                    )
                },
            );
            let fireball = spawn_visual(
                world,
                pos,
                Visual {
                    layer: FLY_LAYER,
                    plane: ABOVE_GAME_PLANE,
                    ..Visual::new("icons/effects/magic.dmi", "fireball", METEOR_FALL)
                },
            );
            // `pixel_z = 270`, `animate(src, pixel_z = 0, time = duration)`.
            if let Some(fireball) = fireball {
                crate::animation::play(
                    world,
                    fireball,
                    Animation::Transform(TransformAnimation {
                        steps: vec![
                            AnimationStep {
                                offset: [0.0, 270.0],
                                ..AnimationStep::rest(0.0)
                            },
                            AnimationStep::rest(METEOR_FALL as f32 * 10.0),
                        ],
                        loops: 1,
                    }),
                );
            }
            METEOR_FALL
        }
        StrikeKind::Lava { .. } => {
            spawn_visual(
                world,
                pos,
                Visual {
                    layer: BELOW_MOB_LAYER,
                    ..Visual::new(EFFECTS_DMI, "lavastaff_warn", LAVA_FALL)
                },
            );
            LAVA_FALL
        }
    };
    world.spawn((
        Name::new("drake strike"),
        Strike {
            at: now + fall,
            pos,
            kind,
            owner,
        },
    ));
}

/// Strikes whose time came: meteors and lava land.
// Cost: one pass over the pending strikes per tick (tens while a drake fights, none otherwise).
pub(super) fn land_strikes(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let due: Vec<(Entity, Strike)> = world
        .query::<(Entity, &Strike)>()
        .iter(world)
        .filter(|(_, s)| now >= s.at)
        .map(|(e, s)| (e, *s))
        .collect();
    for (e, strike) in due {
        world.despawn(e);
        land(world, strike);
    }
}

fn land(world: &mut World, strike: Strike) {
    let pos = strike.pos;
    let closed = crate::construction::turf_at(world, pos)
        .and_then(|t| world.get::<DmTurf>(t))
        .is_some_and(|t| t.closed);
    match strike.kind {
        StrikeKind::Meteor => {
            crate::mining::rocks::gets_drilled(world, pos, None);
            sound(world, pos, EXPLOSION_SFX, 80.0, 0.0);
            hotspot(world, pos);
            for victim in living_on(world, pos) {
                if kind_of(world, victim) == Some(Kind::Drake) {
                    continue;
                }
                hurt(world, victim, DamageType::Burn, STRIKE_DAMAGE, None);
            }
        }
        StrikeKind::Lava { reset } => {
            sound(world, pos, FIREBALL_SOUND, 200.0, 0.0);
            for victim in living_on(world, pos) {
                if kind_of(world, victim) == Some(Kind::Drake) || victim == strike.owner {
                    continue;
                }
                hurt(world, victim, DamageType::Burn, STRIKE_DAMAGE, None);
                let text = if closed {
                    "You are set ablaze by a fireball from above!"
                } else {
                    "You fall directly into the pool of lava!"
                };
                tell(world, victim, text.to_owned());
            }
            let secs = if closed { 1.0 } else { reset };
            spawn_visual(
                world,
                pos,
                Visual {
                    layer: BELOW_MOB_LAYER,
                    color: (!closed).then_some([255, 120, 0, 255]),
                    ..Visual::new(FIRE_DMI, "light", secs)
                },
            );
        }
    }
}

/// `necropolis/dragon/PopulateContents`.
pub(super) fn loot(world: &mut World) -> Vec<&'static str> {
    let i = roll(world, |rng| rng.range(0, LOOT.len() as u32 - 1)) as usize;
    vec![LOOT[i]]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(x: i32, y: i32) -> TilePos {
        TilePos::new(x, y, 1)
    }

    #[test]
    fn get_line_is_tg_bresenham() {
        assert_eq!(
            get_line(t(0, 0), t(3, 0)),
            vec![t(0, 0), t(1, 0), t(2, 0), t(3, 0)]
        );
        assert_eq!(get_line(t(0, 0), t(2, 2)), vec![t(0, 0), t(1, 1), t(2, 2)]);
        let steep = get_line(t(0, 0), t(1, 3));
        assert_eq!(steep.len(), 4);
        assert_eq!(steep[3], t(1, 3));
        assert!(steep.windows(2).all(|w| w[0].chebyshev(w[1]) == Some(1)));
    }

    #[test]
    fn fire_reaches_fifteen_tiles_turned_by_the_offset() {
        let all = |_: TilePos| true;
        let from = t(50, 50);
        assert_eq!(ranged_target_turf(all, from, t(60, 50), 15, 0.0), t(65, 50));
        // 90 degrees counterclockwise of east is north.
        let north = ranged_target_turf(all, from, t(60, 50), 15, 90.0);
        assert!((north.x - 50).abs() <= 1, "{north:?}");
        assert_eq!(north.y, 65);
        // The map's edge stops it.
        let edge = |p: TilePos| p.x <= 55;
        assert_eq!(
            ranged_target_turf(edge, from, t(60, 50), 15, 0.0),
            t(55, 50)
        );
    }
}
