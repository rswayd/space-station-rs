//! Exosuit weapons (server), after tg `/obj/item/mecha_parts/mecha_equipment/weapon`
//! (`equipment/weapons/weapons.dm`) and the ammo boxes (`weapons/mecha_ammo.dm`,
//! `mecha_defense.dm` `ammo_resupply`).
//!
//! - A pilot's click on a non-adjacent atom with a ranged weapon on the selected arm fires it
//!   (`on_mouseclick`): `action_checks` (and a loaded magazine for ballistic ones), the cooldown
//!   and `energy_drain` once, then `projectiles_per_shot` projectiles from the mech (the firer)
//!   at the clicked pixel, fanned over `variance` or spread at random (`randomspread`), each
//!   `projectile_delay` after the previous one while the mech still has `energy_drain`; the
//!   weapon's `fire_sound` at 50. A pacifist pilot will not fire a `harmful` weapon.
//! - Ballistic weapons spend `projectiles_per_shot` per action from the magazine and reload it
//!   from the reserve (`rearm`). An ammo box used on the mech fills the reserve
//!   (`projectiles_cache` up to `projectiles_cache_max`) or, for `direct_load` boxes, the
//!   magazine of every weapon of its `ammo_type`.
// shortcut: there is no Mecha window, so its "reload" button is replaced by reloading an empty
// magazine on the next trigger pull (that click only reloads); add the button with the window.
// shortcut: launchers (thrown grenades, peels, mousetraps), the thermal guns (casings), the
// honker, the concealed weapon bay, zero-g kickback and the muzzle flash are not ported; the
// ammo box's name, sprite, materials and folding stay as they are when emptied.

use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_mapload::mecha::AmmoDef;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;

use super::core::{Mech, MechClick, MechaDb, has_charge, name_of};
use super::equipment::{action_checks, def, spend};
use crate::combat::{CombatMode, CombatRng};
use crate::health::ZoneSelected;
use crate::projectile::{MAX_AIM_PIXEL, Shot};
use crate::sound::PlaySound;
use crate::tools::{Actor, tell};
use crate::zone::BodyZone;

/// A ballistic weapon's rounds: the magazine (`projectiles`) and the reserve
/// (`projectiles_cache`). Starts from the type's values the first time it is read.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WeaponAmmo {
    pub projectiles: u32,
    pub cache: u32,
}

impl WeaponAmmo {
    /// tg `ballistic/rearm`: the magazine back to `magazine` from the reserve. True when it
    /// changed.
    pub fn rearm(&mut self, magazine: u32) -> bool {
        if self.projectiles >= magazine || self.cache == 0 {
            return false;
        }
        let add = (magazine - self.projectiles).min(self.cache);
        self.projectiles += add;
        self.cache -= add;
        true
    }
}

/// An ammo box's `rounds`, from its type the first time it is read.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct AmmoRounds(pub u32);

/// A shot of a burst still to fire (tg `sleep(projectile_delay)` in `action`).
#[derive(Clone, Copy, Debug)]
struct PendingShot {
    at: f64,
    mech: Entity,
    equip: Entity,
    target: Entity,
    target_pos: TilePos,
    pixel: [i16; 2],
    spread: f32,
    zone: BodyZone,
    hit_prone: bool,
}

#[derive(Resource, Default)]
struct MechBursts(Vec<PendingShot>);

pub struct MechWeaponsPlugin;

impl Plugin for MechWeaponsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MechBursts>().add_systems(
            Update,
            fire_bursts.before(crate::projectile::ProjectileSystems),
        );
    }
}

/// A weapon that fires `/obj/projectile`s.
pub fn is_weapon(world: &World, equip: Entity) -> bool {
    def(world, equip).is_some_and(|d| !d.projectile.path.is_empty())
}

/// The weapon's rounds (ballistic weapons only: a magazine in the type).
pub fn ammo(world: &mut World, equip: Entity) -> Option<WeaponAmmo> {
    let d = def(world, equip)?;
    if d.projectiles == 0 {
        return None;
    }
    if let Some(a) = world.get::<WeaponAmmo>(equip) {
        return Some(*a);
    }
    let a = WeaponAmmo {
        projectiles: d.projectiles,
        cache: d.projectiles_cache,
    };
    world.entity_mut(equip).insert(a);
    Some(a)
}

/// tg `round((projectiles_to_shoot / projectiles_per_shot - 0.5) * variance, 1)`, or with
/// `randomspread` a random offset of up to half the variance either way (`unit` in 0..=1).
// Not tg: tg's random spread uses `rand(0, 1)`, an integer, so every pellet goes exactly
// ±variance/2; the intended uniform spread is ported.
pub fn spread(shot: u32, per_shot: u32, variance: f32, randomspread: bool, unit: f32) -> f32 {
    if variance == 0.0 {
        return 0.0;
    }
    let x = if randomspread {
        (f64::from(unit) - 0.5) * f64::from(variance)
    } else {
        // `(shot / n - 0.5) * variance` as one fraction: exact for tg's halves.
        let n = per_shot.max(1);
        (2.0 * f64::from(shot) - f64::from(n)) * f64::from(variance) / (2.0 * f64::from(n))
    };
    // DM `round(x, 1)`: `floor(x + 0.5)`.
    (x + 0.5).floor() as f32
}

/// tg `weapon/action` of a ranged click. True when the click was the weapon's.
pub fn fire(world: &mut World, click: MechClick, mech: Entity, equip: Entity) -> bool {
    let Some(d) = def(world, equip) else {
        return false;
    };
    if d.projectile.path.is_empty() {
        return false;
    }
    if d.harmful
        && world
            .get::<crate::medical::traumas::Pacifist>(click.pilot)
            .is_some()
    {
        tell(
            world,
            click.client,
            Span::Warning,
            "You don't want to harm other living beings!",
        );
        return true;
    }
    if !action_checks(world, mech, equip) {
        return true;
    }
    if let Some(mut a) = ammo(world, equip) {
        if a.projectiles == 0 {
            // shortcut: the Mecha window's "reload" (see the module docs).
            if a.rearm(d.projectiles) {
                world.entity_mut(equip).insert(a);
            }
            return true;
        }
        // `ballistic/action`: the rounds of the whole action, after it fired.
        a.projectiles = a.projectiles.saturating_sub(d.projectiles_per_shot);
        world.entity_mut(equip).insert(a);
    }
    spend(world, mech, equip, d.equip_cooldown);
    let now = world.resource::<Time>().elapsed_secs_f64();
    let max = MAX_AIM_PIXEL;
    let pixel = click
        .modifiers
        .pixel
        .map_or([0, 0], |p| p.map(|v| i16::from(v).clamp(-max, max)));
    let zone = world
        .get::<ZoneSelected>(click.pilot)
        .copied()
        .unwrap_or_default()
        .0
        .check_zone();
    let hit_prone = world.get::<CombatMode>(click.pilot).is_some_and(|c| c.0);
    let n = d.projectiles_per_shot.max(1);
    let mut first = Vec::new();
    for i in 1..=n {
        let unit = world.resource_mut::<CombatRng>().unit();
        let shot = PendingShot {
            at: now + f64::from(d.projectile_delay) / 10.0 * f64::from(i - 1),
            mech,
            equip,
            target: click.target,
            target_pos: click.target_pos,
            pixel,
            spread: spread(i, n, d.variance, d.randomspread, unit),
            zone,
            hit_prone,
        };
        if i == 1 || d.projectile_delay <= 0.0 {
            first.push(shot);
        } else {
            world.resource_mut::<MechBursts>().0.push(shot);
        }
    }
    for shot in first {
        fire_one(world, shot);
    }
    true
}

/// One projectile of an action, if the weapon is still on the mech and the mech has the
/// energy (tg's per-shot `has_charge` check).
fn fire_one(world: &mut World, shot: PendingShot) {
    let Some(d) = def(world, shot.equip) else {
        return;
    };
    let attached = world
        .get::<Mech>(shot.mech)
        .is_some_and(|m| m.equipment().any(|e| e == shot.equip));
    if !attached || (d.energy_drain > 0.0 && !has_charge(world, shot.mech, d.energy_drain)) {
        return;
    }
    let Some(&from) = world.get::<TilePos>(shot.mech) else {
        return;
    };
    let target = Some(shot.target).filter(|&t| world.get_entity(t).is_ok());
    crate::projectile::fire(
        world,
        Shot {
            data: &d.projectile,
            firer: shot.mech,
            from,
            target,
            target_pos: shot.target_pos,
            zone: shot.zone,
            hit_prone: shot.hit_prone,
            pixel: shot.pixel,
            deviation: shot.spread,
            fired_from: Some(shot.equip),
        },
    );
    if !d.fire_sound.is_empty() {
        crate::sound::playsound(
            world,
            PlaySound::new(from, d.fire_sound.clone(), 50.0, true),
        );
    }
}

fn fire_bursts(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let due: Vec<PendingShot> = {
        let mut bursts = world.resource_mut::<MechBursts>();
        if bursts.0.is_empty() {
            return;
        }
        let (due, wait): (Vec<PendingShot>, Vec<PendingShot>) =
            bursts.0.drain(..).partition(|s| s.at <= now);
        bursts.0 = wait;
        due
    };
    for shot in due {
        fire_one(world, shot);
    }
}

fn ammo_def(world: &World, item: Entity) -> Option<AmmoDef> {
    let path = &world.get::<DmItem>(item)?.path;
    world
        .resource::<MechaDb>()
        .data
        .ammo
        .iter()
        .find(|a| &a.path == path)
        .cloned()
}

/// An `/obj/item/mecha_ammo`.
pub fn is_ammo(world: &World, item: Entity) -> bool {
    ammo_def(world, item).is_some()
}

/// tg `ammo_resupply` of an ammo box used on `mech`. True when rounds went in; false falls
/// through to the mech's other interactions (tg `item_interaction` returning `NONE`).
pub fn ammo_resupply(world: &mut World, actor: Actor, item: Entity, mech: Entity) -> bool {
    let Some(box_def) = ammo_def(world, item) else {
        return false;
    };
    let rounds = match world.get::<AmmoRounds>(item) {
        Some(r) => r.0,
        None => box_def.rounds,
    };
    if rounds == 0 {
        tell(world, actor.client, Span::Warning, "the box is empty!");
        return false;
    }
    let guns: Vec<Entity> = world
        .get::<Mech>(mech)
        .map(|m| m.equipment().collect())
        .unwrap_or_default();
    let mut found_gun = false;
    for gun in guns {
        let Some(d) = def(world, gun) else {
            continue;
        };
        if d.projectiles == 0 || d.ammo_type != box_def.ammo_type {
            continue;
        }
        let Some(mut a) = ammo(world, gun) else {
            continue;
        };
        found_gun = true;
        let needed = if box_def.direct_load {
            d.projectiles.saturating_sub(a.projectiles)
        } else {
            d.projectiles_cache_max.saturating_sub(a.cache)
        };
        if needed == 0 {
            continue;
        }
        let added = needed.min(rounds);
        if box_def.direct_load {
            a.projectiles += added;
        } else {
            a.cache += added;
        }
        world.entity_mut(gun).insert(a);
        if let Some(&pos) = world.get::<TilePos>(actor.mob)
            && !box_def.load_audio.is_empty()
        {
            crate::sound::playsound(
                world,
                PlaySound::new(pos, box_def.load_audio.clone(), 50.0, true),
            );
        }
        let plural = if added > 1 { "s" } else { "" };
        let gun_name = name_of(world, gun);
        tell(
            world,
            actor.client,
            Span::Notice,
            format!(
                "You add {added} {}{plural} to {gun_name}",
                box_def.ammo_type
            ),
        );
        if added == rounds && box_def.qdel_on_empty {
            if let Some(mut inventory) = world.get_mut::<crate::inventory::Inventory>(actor.mob) {
                for hand in &mut inventory.hands {
                    if *hand == Some(item) {
                        *hand = None;
                    }
                }
            }
            world.despawn(item);
        } else {
            world.entity_mut(item).insert(AmmoRounds(rounds - added));
        }
        return true;
    }
    let text = if found_gun {
        "ammo storage is full!"
    } else {
        "can't use this ammo!"
    };
    tell(world, actor.client, Span::Warning, text);
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fanned_spread_follows_tg_rounding() {
        // The disabler: 5 beams over 25 degrees.
        let spreads: Vec<f32> = (1..=5).map(|i| spread(i, 5, 25.0, false, 0.0)).collect();
        assert_eq!(spreads, vec![-7.0, -2.0, 3.0, 8.0, 13.0]);
        assert_eq!(spread(1, 1, 0.0, false, 0.0), 0.0);
    }

    #[test]
    fn random_spread_stays_within_half_the_variance() {
        assert_eq!(spread(1, 3, 6.0, true, 0.0), -3.0);
        assert_eq!(spread(1, 3, 6.0, true, 1.0), 3.0);
        assert_eq!(spread(1, 3, 6.0, true, 0.5), 0.0);
    }

    #[test]
    fn rearm_takes_what_the_reserve_has() {
        let mut a = WeaponAmmo {
            projectiles: 0,
            cache: 300,
        };
        assert!(a.rearm(300));
        assert_eq!(
            a,
            WeaponAmmo {
                projectiles: 300,
                cache: 0
            }
        );
        assert!(!a.rearm(300), "full");
        let mut b = WeaponAmmo {
            projectiles: 2,
            cache: 5,
        };
        assert!(b.rearm(24));
        assert_eq!(
            b,
            WeaponAmmo {
                projectiles: 7,
                cache: 0
            }
        );
        assert!(!b.rearm(24), "no reserve");
    }
}
