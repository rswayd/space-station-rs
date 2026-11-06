//! Lava (server), after tg `/turf/open/lava` (`code/game/turfs/open/lava.dm`): whatever touches
//! lava burns.
//!
//! - A living mob stepping onto lava, and every second while it stays there
//!   (`Entered` -> `burn_stuff`, then the turf's `SSobj` `process`), gets 20 fire stacks, is set
//!   alight and takes 20 burn (`lava_firestacks`, `lava_damage`, times the second).
//! - An object or item lying in lava takes `fire_act(10000)` (20 fire damage) and starts burning
//!   (`/datum/component/burning`, [`crate::fire::set_burning`]); lava makes it flammable and not
//!   fireproof first, and caps an item's fire armor at 50.
//! - Nothing burns that is `LAVA_PROOF`, flies, is thrown or weightless, is lava immune
//!   (`TRAIT_LAVA_IMMUNE`: species such as golems, the mining fauna and megafauna), sits on
//!   something that does not burn itself (`buckled`), wears a `LAVAPROTECT` suit and helmet, or
//!   pilots a mech; a catwalk or stone tile over the lava (`TRAIT_LAVA_STOPPED`) makes it safe.
// shortcut: no plasma lava (`/turf/open/lava/plasma`: freezing instead of burning), no
// `perma_fire_overlay` / `TRAIT_NO_EXTINGUISH` (a mob can roll its fire out on lava), no turf
// slowdown (`slowdown = 2`: turf slowdown is not ported), no immerse visuals, no lava light
// borders (`refresh_light`), no lava fishing, no lighting cigarettes, no heatproof rods
// (`/obj/item/stack/rods/lava`), no elevation (`TRAIT_ELEVATED_TURF`, `TRAIT_MOB_ELEVATED`), no
// closet contents (things inside a closet are not on the tile); `/obj/effect`s are ignored
// (they are all lava proof).

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::{DmClothing, DmItem, DmObj, DmTurfBuild, Turf};
use ss13_shared::TilePos;
use ss13_shared::protocol::InventorySlot;
use ss13_world::{Moved, SpatialIndex, WorldSystems};

use crate::fire::{FIRE_PROOF, FLAMMABLE};
use crate::health::{Health, Stat};
use crate::inventory::Inventory;
use crate::obj_damage::is_under;

/// `/turf/open/lava`.
pub const LAVA: &str = "/turf/open/lava";
/// `/turf/open/lava/plasma`: cold, not ported.
const PLASMA_LAVA: &str = "/turf/open/lava/plasma";
/// `lava_damage`, `lava_firestacks`, `temperature_damage`.
pub const LAVA_DAMAGE: f32 = 20.0;
pub const LAVA_FIRESTACKS: f32 = 20.0;
pub const TEMPERATURE_DAMAGE: f64 = 10000.0;
/// `resistance_flags` `LAVA_PROOF`.
pub const LAVA_PROOF: u32 = 1 << 0;
/// `clothing_flags` `LAVAPROTECT`.
pub const LAVAPROTECT: u32 = 1 << 0;
/// `SSobj` `wait`.
const PROCESS_SECS: f64 = 1.0;
/// `give_turf_traits` with `TRAIT_LAVA_STOPPED`.
const LAVA_STOPPERS: [&str; 2] = [
    "/obj/structure/lattice/catwalk",
    "/obj/structure/stone_tile",
];
const EFFECT: &str = "/obj/effect";
const MECHA: &str = "/obj/vehicle/sealed/mecha";
/// Mobs with `TRAIT_LAVA_IMMUNE` from their type (`weather_immunities`, `add_traits` in
/// `Initialize`): the mining fauna, megafauna and bosses, minebots, the dark wizard, fugu.
pub const LAVA_IMMUNE_MOBS: [&str; 7] = [
    "/mob/living/basic/mining",
    "/mob/living/simple_animal/hostile/megafauna",
    "/mob/living/simple_animal/hostile/mining_mobs",
    "/mob/living/basic/boss",
    "/mob/living/basic/mining_drone",
    "/mob/living/basic/dark_wizard",
    "/mob/living/basic/wumborian_fugu",
];

/// tg `can_burn_stuff` results.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BurnCheck {
    /// `LAVA_BE_IGNORING`.
    Ignore,
    /// `LAVA_BE_PROCESSING`: safe, but the turf keeps watching it.
    Safe,
    /// `LAVA_BE_BURNING`.
    Burn,
}

/// The z-levels with lava (found once the map loaded): the systems skip every other level.
#[derive(Resource, Clone, Debug, Default)]
pub struct LavaLevels(pub Vec<u16>);

#[derive(Resource, Default)]
struct LavaClock(f64);

pub struct LavaPlugin;

impl Plugin for LavaPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LavaLevels>()
            .init_resource::<LavaClock>()
            .add_systems(PostStartup, find_lava_levels)
            .add_systems(
                Update,
                (entered_lava, process_lava)
                    .chain()
                    .after(WorldSystems::Movement)
                    .before(crate::health::HealthSystems),
            );
    }
}

fn find_lava_levels(world: &mut World) {
    let mut q = world.query_filtered::<(&TilePos, &DmTurfBuild), With<Turf>>();
    let mut levels: Vec<u16> = q
        .iter(world)
        .filter(|(_, t)| is_lava_path(&t.path))
        .map(|(p, _)| p.z)
        .collect();
    levels.sort_unstable();
    levels.dedup();
    world.resource_mut::<LavaLevels>().0 = levels;
}

fn is_lava_path(path: &str) -> bool {
    is_under(path, LAVA) && !is_under(path, PLASMA_LAVA)
}

/// `islava(turf) && !is_safe()`: a lava turf on `pos` without a catwalk over it.
pub fn burning_lava_at(world: &World, pos: TilePos) -> bool {
    let index = world.resource::<SpatialIndex>();
    let tile = index.on_tile(pos);
    let lava = tile.iter().any(|&e| {
        world.get::<Turf>(e).is_some()
            && world
                .get::<DmTurfBuild>(e)
                .is_some_and(|t| is_lava_path(&t.path))
    });
    lava && !tile.iter().any(|&e| {
        world
            .get::<DmObj>(e)
            .is_some_and(|o| LAVA_STOPPERS.iter().any(|root| is_under(&o.path, root)))
    })
}

/// `TRAIT_LAVA_IMMUNE` of a mob.
pub fn lava_immune(world: &World, mob: Entity) -> bool {
    crate::species::has_trait(world, mob, crate::species::Trait::LavaImmune)
        || world.get::<crate::mobs::BasicMob>(mob).is_some_and(|m| {
            LAVA_IMMUNE_MOBS
                .iter()
                .any(|root| is_under(&m.def.path, root))
        })
}

/// `LAVAPROTECT` on both the suit and the helmet.
fn lava_protected(world: &World, mob: Entity) -> bool {
    let Some(inventory) = world.get::<Inventory>(mob) else {
        return false;
    };
    [InventorySlot::Suit, InventorySlot::Head]
        .iter()
        .all(|&slot| {
            inventory
                .slot(slot)
                .and_then(|e| world.get::<DmClothing>(e))
                .is_some_and(|c| c.clothing_flags & LAVAPROTECT != 0)
        })
}

/// tg `can_burn_stuff(burn_target)`.
pub fn can_burn(world: &World, thing: Entity) -> BurnCheck {
    let Ok(entity) = world.get_entity(thing) else {
        return BurnCheck::Ignore;
    };
    let Some(&pos) = entity.get::<TilePos>() else {
        return BurnCheck::Ignore;
    };
    if entity.contains::<Turf>()
        || entity.contains::<crate::throw::Thrown>()
        || crate::multiz::flies(world, thing)
        || !crate::multiz::has_gravity(world, pos)
    {
        return BurnCheck::Ignore;
    }
    let resistance = entity
        .get::<DmObj>()
        .map(|o| (o.path.as_str(), o.resistance_flags))
        .or_else(|| {
            entity
                .get::<DmItem>()
                .map(|i| (i.path.as_str(), i.resistance_flags))
        });
    if let Some((path, flags)) = resistance {
        if is_under(path, EFFECT) || is_under(path, MECHA) {
            return BurnCheck::Ignore;
        }
        return if flags & LAVA_PROOF != 0 {
            BurnCheck::Safe
        } else {
            BurnCheck::Burn
        };
    }
    if !entity.contains::<Health>() {
        return BurnCheck::Ignore;
    }
    if lava_immune(world, thing) || entity.contains::<crate::mecha::core::Pilot>() {
        return BurnCheck::Safe;
    }
    if let Some(b) = entity.get::<crate::medical::surgery::Buckled>()
        && can_burn(world, b.0) != BurnCheck::Burn
    {
        return BurnCheck::Safe;
    }
    if lava_protected(world, thing) {
        return BurnCheck::Safe;
    }
    BurnCheck::Burn
}

/// tg `do_burn(burn_target, seconds_per_tick)`.
pub fn do_burn(world: &mut World, thing: Entity, spt: f64) {
    if world.get_entity(thing).is_err() {
        return;
    }
    if world.get::<Health>(thing).is_some() && world.get::<DmObj>(thing).is_none() {
        crate::fire::adjust_fire_stacks(world, thing, LAVA_FIRESTACKS * spt as f32);
        crate::fire::ignite_mob(world, thing);
        if let Ok(mut entity) = world.get_entity_mut(thing)
            && let Ok((mut health, mut stat)) =
                entity.get_components_mut::<(&mut Health, &mut Stat)>()
        {
            crate::health::apply_damage(
                &mut health,
                &mut stat,
                crate::health::DamageType::Burn,
                LAVA_DAMAGE * spt as f32,
            );
        }
        return;
    }
    // An object: already burning, nothing more.
    if world.get::<crate::fire::Burning>(thing).is_some() {
        return;
    }
    // "Even fireproof things burn up in lava"; fire armor over 50 is cut to 50.
    if let Some(mut obj) = world.get_mut::<DmObj>(thing) {
        obj.resistance_flags = (obj.resistance_flags | FLAMMABLE) & !FIRE_PROOF;
    }
    if let Some(mut item) = world.get_mut::<DmItem>(thing) {
        item.resistance_flags = (item.resistance_flags | FLAMMABLE) & !FIRE_PROOF;
    }
    if let Some(mut armor) = world.get_mut::<ss13_mapload::DmArmor>(thing)
        && armor.fire > 50.0
    {
        armor.fire = 50.0;
    }
    // `fire_act(temperature_damage, 1000)`: `clamp(0.02 * 10000, 0, 20)`, then it burns.
    let potential = (0.02 * TEMPERATURE_DAMAGE) as f32;
    let deflection = world
        .get::<DmObj>(thing)
        .map_or(0.0, |o| o.damage_deflection);
    if potential > deflection {
        crate::fire::fire_damage(world, thing, potential.min(20.0));
    }
    crate::fire::set_burning(world, thing);
}

/// `burn_stuff(thing)`: burns it if it can; true while the lava has something to watch.
pub fn burn_stuff(world: &mut World, thing: Entity, spt: f64) -> bool {
    match can_burn(world, thing) {
        BurnCheck::Ignore => false,
        BurnCheck::Safe => true,
        BurnCheck::Burn => {
            do_burn(world, thing, spt);
            true
        }
    }
}

/// `Entered`: a mob walking onto lava burns at once.
fn entered_lava(world: &mut World, mut cursor: Local<MessageCursor<Moved>>) {
    let moved: Vec<Moved> = cursor
        .read(world.resource::<Messages<Moved>>())
        .copied()
        .collect();
    if world.resource::<LavaLevels>().0.is_empty() {
        return;
    }
    for m in moved {
        if world.resource::<LavaLevels>().0.contains(&m.to.z)
            && world.get::<TilePos>(m.entity) == Some(&m.to)
            && burning_lava_at(world, m.to)
        {
            burn_stuff(world, m.entity, 1.0);
        }
    }
}

/// The lava turfs' `process`: every second, everything resting in lava burns (things dropped,
/// thrown or spawned there too, tg `on_atom_inited` / `hitby`).
// Cost: once a second, one pass over the movables of the lava levels (the mining level's few
// thousand), a tile lookup each.
fn process_lava(world: &mut World) {
    let delta = world.resource::<Time>().delta_secs_f64();
    {
        let mut clock = world.resource_mut::<LavaClock>();
        clock.0 += delta;
        if clock.0 < PROCESS_SECS {
            return;
        }
        clock.0 -= PROCESS_SECS;
    }
    let levels = world.resource::<LavaLevels>().0.clone();
    if levels.is_empty() {
        return;
    }
    let mut q = world
        .query_filtered::<(Entity, &TilePos, Has<Health>, Has<DmItem>, Has<DmObj>), Without<Turf>>(
        );
    let candidates: Vec<(Entity, TilePos)> = q
        .iter(world)
        .filter(|&(_, p, living, item, obj)| (living || item || obj) && levels.contains(&p.z))
        .map(|(e, &p, ..)| (e, p))
        .collect();
    let burning: Vec<Entity> = candidates
        .into_iter()
        .filter(|&(_, p)| burning_lava_at(world, p))
        .map(|(e, _)| e)
        .collect();
    for thing in burning {
        burn_stuff(world, thing, PROCESS_SECS);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lava_types() {
        assert!(is_lava_path("/turf/open/lava/smooth/lava_land_surface"));
        assert!(is_lava_path("/turf/open/lava"));
        assert!(!is_lava_path("/turf/open/lava/plasma/ice_moon"));
        assert!(!is_lava_path("/turf/open/lavaland_floor"));
    }
}
