//! Cone, charged and basic projectile spells (server) after tg `spell_types/cone/`,
//! `spell_types/charged/` and `spell_types/projectile/`.
//!
//! - **Cone of Cold** ([`cone_turfs`], `cone_of_cold`): four levels ahead of the caster, walls
//!   stopping it (`respect_density`), each level 0.05 s after the last: the floor freezes
//!   (permafrost, 45 s), mobs in it freeze (20 s), take 10 brute and 20 burn (antimagic and
//!   the caster spared), objects freeze (20 s).
//! - **Tesla Blast** ([`begin_channel`], `tesla_blast`): a 10 s channel the caster can walk in;
//!   then an arc at a random nearby human, bouncing up to 5 more times, 30 energy less 5 per
//!   bounce (at least 5), `electrocute_act` each.
//! - **Gauntlet Echo** (`gauntlet_echo`): the juggernaut's 30 brute, knocking-down bolt, straight
//!   ahead up to 7 tiles until a wall.
// shortcut: frozen mobs are paralyzed (the freon status effect is not ported) and no
// `TRAIT_RESISTCOLD` spares them; the channel has no overlay; targets are found in range, without
// `view` or `get_path_to` (as `crate::spells::effects`); no beam visuals; the echo's antimagic is
// the default.

use bevy::prelude::*;
use ss13_shared::chat::Span;
use ss13_shared::{Dir, Facing, TilePos};
use ss13_world::{Density, SpatialIndex};

use super::effects::{BoltKind, fire_bolt, hurt, pick, play};
use super::{CastOn, MAGIC_RESISTANCE, Spell, can_block_magic, cast, chat_to, now};
use crate::effects::frozen;
use crate::health::{DamageType, Health, Stat};
use crate::human::HumanLook;
use crate::janitor::{TURF_WET_PERMAFROST, make_slippery};
use crate::status_effects::{Incap, effects_mut};

/// `cone_of_cold` vars.
const CONE_LEVELS: u32 = 4;
const LEVEL_DELAY: f64 = 0.05;
const TURF_FREEZE_SECS: f32 = 45.0;
const MOB_FREEZE_SECS: f32 = 20.0;
const OBJ_FREEZE_SECS: f64 = 20.0;
const FREEZE_BRUTE: f32 = 10.0;
const FREEZE_BURN: f32 = 20.0;
/// `tesla` vars.
const CHANNEL_SECS: f64 = 10.0;
const TESLA_RADIUS: u32 = 7;
const TESLA_BOUNCES: u32 = 5;
const TESLA_ENERGY: f32 = 30.0;
const TESLA_ENERGY_LOST: f32 = 5.0;
const CHARGE_SOUND: &str = "sound/effects/magic/lightning_chargeup.ogg";
const SHOCK_SOUND: &str = "sound/effects/magic/lightningshock.ogg";
/// `juggernaut` range.
const ECHO_RANGE: u32 = 7;

/// A staggered cone on its way (`make_cone`'s timers).
#[derive(Component, Clone, Debug)]
struct PendingCone {
    caster: Entity,
    levels: Vec<Vec<TilePos>>,
    next: usize,
    at: f64,
}

/// An object frozen by the cone until then.
#[derive(Component, Clone, Copy, Debug)]
struct ThawAt(f64);

/// `/datum/action/cooldown/spell/charged` in progress.
#[derive(Component, Clone, Copy, Debug)]
pub struct Channeling {
    pub client: Entity,
    pub spell: Spell,
    pub until: f64,
}

fn turf_dense(world: &World, pos: TilePos) -> bool {
    crate::construction::turf_at(world, pos).is_some_and(|t| world.get::<Density>(t).is_some())
}

fn step(pos: TilePos, dir: Dir) -> TilePos {
    let (dx, dy) = dir.offset();
    TilePos::new(pos.x + dx, pos.y + dy, pos.z)
}

/// `calculate_cone_shape`: 1 (innate) -> 3 -> 5 -> 5 -> 7 -> 7 ...
fn cone_width(level: u32) -> u32 {
    level + (level % 2) + 1
}

/// `get_cone_turfs` with `respect_density`; `dense` tells whether a tile stops it.
pub fn cone_turfs(
    start: TilePos,
    dir: Dir,
    levels: u32,
    dense: impl Fn(TilePos) -> bool,
) -> Vec<Vec<TilePos>> {
    let (left, right) = match dir {
        Dir::North => (Dir::West, Dir::East),
        Dir::South => (Dir::East, Dir::West),
        Dir::East => (Dir::North, Dir::South),
        _ => (Dir::South, Dir::North),
    };
    let mut out = Vec::new();
    let mut center = start;
    for level in 1..=levels {
        center = step(center, dir);
        let mut row = vec![center];
        if level != 1 {
            let width = (cone_width(level) - 1) / 2;
            let mut side = center;
            for _ in 0..width {
                if dense(side) {
                    break;
                }
                side = step(side, left);
                row.push(side);
            }
            side = center;
            for _ in 0..width {
                if dense(side) {
                    break;
                }
                side = step(side, right);
                row.push(side);
            }
        }
        out.push(row);
        if level != levels && dense(center) {
            break;
        }
    }
    out
}

/// `cone/cast` of `cone_of_cold`: the levels are queued for `tick`.
pub(super) fn cone_of_cold(world: &mut World, mob: Entity, pos: TilePos) -> bool {
    let dir = world.get::<Facing>(mob).map_or(Dir::South, |f| f.0);
    let levels = cone_turfs(pos, dir, CONE_LEVELS, |p| turf_dense(world, p));
    let t = now(world);
    world.spawn((
        Name::new("cone of cold"),
        PendingCone {
            caster: mob,
            levels,
            next: 0,
            at: t + LEVEL_DELAY,
        },
    ));
    true
}

/// `do_cone_effects` of a level: the floors, the mobs, the objects.
fn freeze_level(world: &mut World, caster: Entity, turfs: &[TilePos], t: f64) {
    for &pos in turfs {
        if turf_dense(world, pos) {
            continue;
        }
        make_slippery(world, pos, TURF_WET_PERMAFROST, TURF_FREEZE_SECS, 0.0);
        let on_tile: Vec<Entity> = world.resource::<SpatialIndex>().on_tile(pos).to_vec();
        for e in on_tile {
            if world.get::<Health>(e).is_some() {
                freeze_mob(world, caster, e);
            } else if frozen::freeze(world, e) {
                world.entity_mut(e).insert(ThawAt(t + OBJ_FREEZE_SECS));
            }
        }
    }
}

/// `do_mob_cone_effect`.
fn freeze_mob(world: &mut World, caster: Entity, target: Entity) {
    if target == caster || can_block_magic(world, target, MAGIC_RESISTANCE) {
        return;
    }
    if let Some(mut effects) = effects_mut(world, target) {
        effects.apply(Incap::Paralyze, MOB_FREEZE_SECS);
    }
    hurt(world, target, DamageType::Brute, FREEZE_BRUTE);
    hurt(world, target, DamageType::Burn, FREEZE_BURN);
    chat_to(world, target, Span::Danger, "You feel a bitter cold!");
}

/// [`Spell::TeslaBlast`]'s button: `before_cast` of a charged spell.
pub(super) fn begin_channel(world: &mut World, client: Entity, mob: Entity, spell: Spell) {
    if world.get::<Channeling>(mob).is_some() {
        chat_to(
            world,
            mob,
            Span::Warning,
            format!("You're already channeling {}!", spell.def().name),
        );
        return;
    }
    chat_to(world, mob, Span::Notice, "You start gathering power...");
    if let Some(&pos) = world.get::<TilePos>(mob) {
        play(world, pos, CHARGE_SOUND);
    }
    let t = now(world);
    world.entity_mut(mob).insert(Channeling {
        client,
        spell,
        until: t + CHANNEL_SECS,
    });
}

/// `tesla/get_target`: a random human near `center`, not the caster or `center` itself.
fn tesla_target(world: &mut World, mob: Entity, center: TilePos, except: Entity) -> Option<Entity> {
    let mut near: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .in_radius(center, TESLA_RADIUS)
        .filter(|&e| {
            e != mob
                && e != except
                && world.get::<Health>(e).is_some()
                && world.get::<HumanLook>(e).is_some()
        })
        .collect();
    near.sort();
    pick(world, &near)
}

/// `beam/cast` -> `send_beam`.
pub(super) fn tesla_blast(world: &mut World, mob: Entity, pos: TilePos) -> bool {
    let Some(mut target) = tesla_target(world, mob, pos, mob) else {
        chat_to(world, mob, Span::Warning, "no targets nearby!");
        return false;
    };
    let mut energy = TESLA_ENERGY;
    for bounces_left in (0..=TESLA_BOUNCES).rev() {
        let Some(&at) = world.get::<TilePos>(target) else {
            break;
        };
        play(world, at, SHOCK_SOUND);
        if can_block_magic(world, target, MAGIC_RESISTANCE) {
            let name = crate::medical::name_of(world, target);
            crate::medical::visible(
                world,
                target,
                "You absorb the spell, remaining unharmed!",
                &format!("{name} absorbs the spell, remaining unharmed!"),
                Span::Warning,
            );
        } else {
            crate::wires::electrocute_act(world, target, energy, mob, 1.0);
        }
        if bounces_left == 0 {
            break;
        }
        let Some(next) = tesla_target(world, mob, at, target) else {
            break;
        };
        energy = (energy - TESLA_ENERGY_LOST).max(TESLA_ENERGY_LOST);
        target = next;
    }
    true
}

/// `basic_projectile/juggernaut/cast`: as far ahead as the walls allow.
pub(super) fn gauntlet_echo(world: &mut World, mob: Entity, pos: TilePos, level: u8) -> bool {
    let dir = world.get::<Facing>(mob).map_or(Dir::South, |f| f.0);
    let mut at = pos;
    for _ in 1..ECHO_RANGE {
        let next = step(at, dir);
        if turf_dense(world, next) {
            break;
        }
        at = next;
    }
    fire_bolt(world, mob, pos, None, at, BoltKind::GauntletEcho, level);
    true
}

/// The cones' levels, thawing objects and finished channels.
pub(super) fn tick(world: &mut World) {
    let t = now(world);
    let cones: Vec<(Entity, PendingCone)> = world
        .query::<(Entity, &PendingCone)>()
        .iter(world)
        .map(|(e, c)| (e, c.clone()))
        .collect();
    for (e, mut cone) in cones {
        if t < cone.at {
            continue;
        }
        let level = cone.levels.get(cone.next).cloned().unwrap_or_default();
        freeze_level(world, cone.caster, &level, t);
        cone.next += 1;
        cone.at += LEVEL_DELAY;
        if cone.next >= cone.levels.len() {
            world.despawn(e);
        } else {
            world.entity_mut(e).insert(cone);
        }
    }
    let thawing: Vec<Entity> = world
        .query::<(Entity, &ThawAt)>()
        .iter(world)
        .filter(|(_, at)| t >= at.0)
        .map(|(e, _)| e)
        .collect();
    for e in thawing {
        world.entity_mut(e).remove::<ThawAt>();
        frozen::unfreeze(world, e);
    }
    let channels: Vec<(Entity, Channeling)> = world
        .query::<(Entity, &Channeling)>()
        .iter(world)
        .map(|(e, c)| (e, *c))
        .collect();
    for (mob, channel) in channels {
        let awake = world.get::<Stat>(mob) == Some(&Stat::Conscious);
        if awake && t < channel.until {
            continue;
        }
        world.entity_mut(mob).remove::<Channeling>();
        if awake {
            cast(world, channel.client, mob, channel.spell, CastOn::Caster);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cone_widens_like_tg() {
        let open = cone_turfs(TilePos::new(10, 10, 1), Dir::North, 4, |_| false);
        assert_eq!(open.len(), 4);
        assert_eq!(open[0], vec![TilePos::new(10, 11, 1)]);
        assert_eq!(open[1].len(), 3);
        assert_eq!(open[2].len(), 5);
        assert_eq!(open[3].len(), 5);
    }

    #[test]
    fn walls_stop_the_cone() {
        let wall = TilePos::new(10, 12, 1);
        let cone = cone_turfs(TilePos::new(10, 10, 1), Dir::North, 4, |p| p == wall);
        assert_eq!(cone.len(), 2, "stops after the blocked centre");
    }
}
