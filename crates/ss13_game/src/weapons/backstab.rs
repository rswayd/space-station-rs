//! Backstabs (server), after BandaStation `/datum/component/backstabs`
//! (`modular_bandastation/~components/code/backstab.dm`), which the switchblade carries
//! (`AddComponent(/datum/component/backstabs, 2, 2 SECONDS)`, a `BANDASTATION ADDITION` in
//! `knives.dm`).
//!
//! On an item attack (`COMSIG_ITEM_ATTACK`, before blocks and armor) a conscious living target
//! hit from behind (the attacker and the target both face the way from the attacker to the
//! target) takes the item's `force × (multiplier - 1)` of its `damtype` more in the chest,
//! unarmored; then the backstab is on cooldown. Pacifists never get here (`crate::combat`
//! refuses their damaging attacks first).
// shortcut: the extra hit rolls no wound (tg multiplies `wound_bonus`, `exposed_wound_bonus`,
// `sharpness`) and is logged to the server log, not `log_combat`; add both with attack logs and
// a world-level wound roll.

use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_mapload::fidelity::is_under;
use ss13_shared::{Dir, Facing, TilePos};

use crate::health::{DamageType, Health, Stat};
use crate::zone::BodyZone;

pub const SWITCHBLADE: &str = "/obj/item/switchblade";

/// `backstab_multiplier` and `cooldown_time` (seconds) of an item with the component.
pub fn backstabs(path: &str) -> Option<(f32, f64)> {
    is_under(path, SWITCHBLADE).then_some((2.0, 2.0))
}

/// The item's backstab cooldown: ready again at this game time (`COOLDOWN_START`).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct BackstabCooldown(pub f64);

/// `can_backstab`'s facing check: `get_dir(user, target)` shares a bit with both `dir`s.
pub fn from_behind(user_pos: TilePos, user_dir: Dir, target_pos: TilePos, target_dir: Dir) -> bool {
    let (dx, dy) = (
        (target_pos.x - user_pos.x).signum(),
        (target_pos.y - user_pos.y).signum(),
    );
    let Some(dir) = Dir::DMI_ORDER.into_iter().find(|d| d.offset() == (dx, dy)) else {
        return false;
    };
    let bits = dir as u8;
    user_dir as u8 & bits != 0 && target_dir as u8 & bits != 0
}

/// `on_attack`: `user` hits `target` with `item`.
pub fn on_attack(world: &mut World, user: Entity, target: Entity, item: Entity) {
    let Some(dm) = world.get::<DmItem>(item) else {
        return;
    };
    let Some((multiplier, cooldown)) = backstabs(&dm.path) else {
        return;
    };
    let (force, kind) = (
        dm.force,
        DamageType::from_dm(&dm.damtype).unwrap_or(DamageType::Brute),
    );
    let now = world
        .get_resource::<Time>()
        .map_or(0.0, Time::elapsed_secs_f64);
    if world
        .get::<BackstabCooldown>(item)
        .is_some_and(|c| now < c.0)
    {
        return;
    }
    // `isliving(target)`, then no backstabbing someone already down.
    if world.get::<Health>(target).is_none()
        || world
            .get::<Stat>(target)
            .is_none_or(|&s| s != Stat::Conscious)
    {
        return;
    }
    let (Some(&up), Some(&uf), Some(&tp), Some(&tf)) = (
        world.get::<TilePos>(user),
        world.get::<Facing>(user),
        world.get::<TilePos>(target),
        world.get::<Facing>(target),
    ) else {
        return;
    };
    if !from_behind(up, uf.0, tp, tf.0) {
        return;
    }
    let damage = force * (multiplier - 1.0);
    // "Truthy because backstabs can heal".
    if damage != 0.0 {
        crate::bots::hurt_zone_raw(world, target, BodyZone::Chest, kind, damage);
        let (who, whom) = (
            crate::security::name_of(world, user),
            crate::security::name_of(world, target),
        );
        info!("{who} scored a backstab on {whom} ({damage} {kind:?})");
    }
    if cooldown > 0.0 {
        world
            .entity_mut(item)
            .insert(BackstabCooldown(now + cooldown));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_stab_in_the_back_counts() {
        let user = TilePos::new(5, 4, 0);
        let target = TilePos::new(5, 5, 0);
        // Both face north, the attacker south of the target: its back.
        assert!(from_behind(user, Dir::North, target, Dir::North));
        // Face to face.
        assert!(!from_behind(user, Dir::North, target, Dir::South));
        // Turned sideways.
        assert!(!from_behind(user, Dir::North, target, Dir::East));
        // A diagonal approach shares a bit with a cardinal back.
        let corner = TilePos::new(4, 4, 0);
        assert!(from_behind(corner, Dir::NorthEast, target, Dir::North));
        // On the same tile there is no direction.
        assert!(!from_behind(target, Dir::North, target, Dir::North));
    }

    #[test]
    fn backstab_doubles_force_then_cools_down() {
        let mut world = World::new();
        let blade = world
            .spawn(DmItem {
                path: "/obj/item/switchblade/extended".into(),
                force: 20.0,
                damtype: "brute".into(),
                ..default()
            })
            .id();
        let user = world
            .spawn((TilePos::new(5, 4, 0), Facing(Dir::North)))
            .id();
        let target = world
            .spawn((
                TilePos::new(5, 5, 0),
                Facing(Dir::North),
                Health::default(),
                Stat::Conscious,
            ))
            .id();
        on_attack(&mut world, user, target, blade);
        assert_eq!(world.get::<Health>(target).unwrap().brute, 20.0);
        assert_eq!(
            world.get::<BackstabCooldown>(blade),
            Some(&BackstabCooldown(2.0))
        );
        // Cooling down: nothing more.
        on_attack(&mut world, user, target, blade);
        assert_eq!(world.get::<Health>(target).unwrap().brute, 20.0);
    }
}
