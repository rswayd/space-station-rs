//! Basic mobs that shoot (server), after tg `/datum/component/ranged_attacks` (`RangedAttack` of
//! a basic mob: a casing's projectile at the target, `cooldown_time` between shots) and the
//! `basic_ranged_attack` leaf (`basic_attacking.dm`): in sight within `max_range`, and, with
//! `avoid_friendly_fire`, never through a friend in the way (the shooter steps aside instead).
//!
//! The ported shooters are the hivebots (`/obj/item/ammo_casing/hivebot`: 10 brute) and the
//! stick gunman (`/obj/item/ammo_casing/c9mm`, 5 s).
// shortcut: one projectile per shot (no `burst_shots`, no casings left behind, no firing sound);
// add them with the ranged troopers.

use bevy::prelude::*;
use ss13_mapload::ProjectileData;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::mobs::MobDef;
use ss13_shared::{Dir, TilePos};
use ss13_world::{MoveRequest, SpatialIndex};

use super::ai::behaviors::{DELAY_SUCCEED, FAIL, Perform, Strategy, can_see};
use super::ai::{AiCtx, Key};
use crate::health::Health;

/// `/datum/component/ranged_attacks` of a mob.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct RangedAttacks {
    pub projectile: ProjectileData,
    /// `cooldown_time`, seconds.
    pub cooldown: f64,
    /// When it may shoot again.
    pub next: f64,
}

/// `/obj/projectile/hivebotbullet` with the `/obj/projectile` defaults.
pub fn hivebot_bullet() -> ProjectileData {
    ProjectileData {
        path: "/obj/projectile/hivebotbullet".to_owned(),
        damage: 10.0,
        damage_type: "brute".to_owned(),
        armor_flag: "bullet".to_owned(),
        speed: 1.25,
        range: 50.0,
        pass_flags: crate::projectile::PASSTABLE,
        accurate_range: 100.0,
        accuracy_falloff: 7.0,
        ..default()
    }
}

/// `/obj/projectile/bullet/c9mm` with the `/obj/projectile/bullet` defaults (no embedding).
pub fn c9mm_bullet() -> ProjectileData {
    ProjectileData {
        path: "/obj/projectile/bullet/c9mm".to_owned(),
        damage: 30.0,
        damage_type: "brute".to_owned(),
        armor_flag: "bullet".to_owned(),
        speed: 1.25,
        range: 50.0,
        pass_flags: crate::projectile::PASSTABLE,
        accurate_range: 100.0,
        accuracy_falloff: 7.0,
        wound_falloff_tile: -5.0,
        // `SHARP_POINTY`.
        sharpness: 2,
        ..default()
    }
}

/// The `ranged_attacks` of mob type `path`: hivebots with `ranged_attacker`, the stick gunman.
fn ranged_of(path: &str) -> Option<RangedAttacks> {
    let under = |root: &str| is_under(path, root);
    if under("/mob/living/basic/stickman/ranged") {
        return Some(RangedAttacks {
            projectile: c9mm_bullet(),
            cooldown: 5.0,
            next: 0.0,
        });
    }
    let cooldown = if under("/mob/living/basic/hivebot/rapid") {
        1.5
    } else if under("/mob/living/basic/hivebot/range")
        || under("/mob/living/basic/hivebot/strong")
        || under("/mob/living/basic/hivebot/mechanic")
    {
        3.0
    } else {
        return None;
    };
    Some(RangedAttacks {
        projectile: hivebot_bullet(),
        cooldown,
        next: 0.0,
    })
}

pub(super) fn attach(world: &mut World, mob: Entity, def: &MobDef) {
    if let Some(ranged) = ranged_of(&def.path) {
        world.entity_mut(mob).insert(ranged);
    }
}

/// `basic_ranged_attack` `perform` with the `Basic` targeting strategy.
pub fn attack(ctx: &mut AiCtx, key: Key, max_range: u32, avoid_friendly_fire: bool) -> Perform {
    let Some(target) = ctx
        .bb
        .entity(key)
        .filter(|&t| ctx.world.get_entity(t).is_ok())
    else {
        return FAIL;
    };
    let (Some(from), Some(&to)) = (ctx.pawn_pos(), ctx.world.get::<TilePos>(target)) else {
        return FAIL;
    };
    if !can_see(ctx.world, from, to, max_range) || !Strategy::Basic.valid(ctx, target, 0) {
        return FAIL;
    }
    if avoid_friendly_fire && friendly_in_path(ctx, from, to, target) {
        adjust_position(ctx, from, to);
        return FAIL;
    }
    let Some(ranged) = ctx.world.get::<RangedAttacks>(ctx.pawn).cloned() else {
        return FAIL;
    };
    if ctx.now < ranged.next {
        return FAIL;
    }
    if let Some(mut r) = ctx.world.get_mut::<RangedAttacks>(ctx.pawn) {
        r.next = ctx.now + ranged.cooldown;
    }
    // tg `face_atom` before firing.
    if let Some(dir) = crate::pull::dir_between(from, to)
        && let Some(mut facing) = ctx.world.get_mut::<ss13_shared::Facing>(ctx.pawn)
    {
        facing.set_if_neq(ss13_shared::Facing(dir));
    }
    crate::projectile::fire(
        ctx.world,
        crate::projectile::Shot {
            data: &ranged.projectile,
            firer: ctx.pawn,
            from,
            target: Some(target),
            target_pos: to,
            zone: crate::zone::BodyZone::Chest,
            // `combat_mode = TRUE` shooters hit the prone too.
            hit_prone: true,
            pixel: [0, 0],
            deviation: 0.0,
            fired_from: None,
        },
    );
    DELAY_SUCCEED
}

/// `calculate_trajectory`: the line's tiles, and both cardinal neighbours at each diagonal step,
/// without the ends.
pub fn trajectory(from: TilePos, to: TilePos) -> Vec<TilePos> {
    let (dx, dy) = (to.x - from.x, to.y - from.y);
    let steps = dx.abs().max(dy.abs());
    let mut line: Vec<TilePos> = (0..=steps)
        .map(|i| {
            let t = if steps == 0 {
                0.0
            } else {
                f64::from(i) / f64::from(steps)
            };
            TilePos::new(
                from.x + (f64::from(dx) * t).round() as i32,
                from.y + (f64::from(dy) * t).round() as i32,
                from.z,
            )
        })
        .collect();
    let mut extra = Vec::new();
    for pair in line.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if a.x != b.x && a.y != b.y {
            extra.push(TilePos::new(b.x, a.y, a.z));
            extra.push(TilePos::new(a.x, b.y, a.z));
        }
    }
    line.extend(extra);
    line.retain(|&p| p != from && p != to);
    line
}

/// `check_friendly_in_path`: a living mob in the way the shooter would not shoot.
fn friendly_in_path(ctx: &AiCtx, from: TilePos, to: TilePos, target: Entity) -> bool {
    let world = &*ctx.world;
    let index = world.resource::<SpatialIndex>();
    trajectory(from, to).into_iter().any(|p| {
        index.on_tile(p).iter().any(|&e| {
            e != ctx.pawn
                && e != target
                && world.get::<Health>(e).is_some()
                && !Strategy::Basic.valid(ctx, e, 0)
        })
    })
}

/// `adjust_position`: a step to the free neighbour closest to the target, never farther away.
fn adjust_position(ctx: &mut AiCtx, from: TilePos, to: TilePos) {
    let far = from.chebyshev(to).unwrap_or(0);
    let index = ctx.world.resource::<SpatialIndex>();
    let best = Dir::DMI_ORDER
        .into_iter()
        .filter(|&d| super::ai::movement::can_step(index, ctx.pawn, from, d))
        .filter_map(|d| Some((from.step(d).chebyshev(to)?, d)))
        .filter(|&(dist, _)| dist <= far)
        .min_by_key(|&(dist, d)| (dist, d as u8));
    if let Some((_, dir)) = best {
        ctx.world.write_message(MoveRequest {
            entity: ctx.pawn,
            dir,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trajectory_covers_diagonal_corners() {
        let from = TilePos::new(0, 0, 0);
        let line = trajectory(from, TilePos::new(3, 0, 0));
        assert_eq!(line, vec![TilePos::new(1, 0, 0), TilePos::new(2, 0, 0)]);
        let diag = trajectory(from, TilePos::new(2, 2, 0));
        assert!(diag.contains(&TilePos::new(1, 1, 0)));
        assert!(diag.contains(&TilePos::new(1, 0, 0)));
        assert!(diag.contains(&TilePos::new(0, 1, 0)));
        assert!(!diag.contains(&from));
    }

    #[test]
    fn hivebots_shoot() {
        assert!(ranged_of("/mob/living/basic/hivebot").is_none());
        assert_eq!(
            ranged_of("/mob/living/basic/hivebot/rapid").map(|r| r.cooldown),
            Some(1.5)
        );
        assert_eq!(hivebot_bullet().damage, 10.0);
    }
}
