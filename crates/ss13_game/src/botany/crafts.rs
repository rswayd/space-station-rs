//! Produce worked by hand (tg `grown/cotton.dm`, `grass_carpet.dm`, `hedges.dm`):
//!
//! - Cotton and durathread bundles used in hand become `1 + round(potency / 25)` raw cotton or
//!   durathread at the user's feet.
//! - Grass, fairygrass and carpet used in hand become `1 + round(potency × 0.02)` tiles of their
//!   kind, taking every clump of the exact same type on the user's tile along.
//! - A shrub used in hand is planted (8 s) into an opaque hedge on a tile with nothing dense but
//!   mobs on it.
// shortcut: no organ tree harvest (pod organs and limbs), no hedge trimming (`TRAIT_BONSAI`
// comes from a skillchip, none are ported).

use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_reagents::is_under;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_world::SpatialIndex;

use super::BotanyJob;
use super::seeds::{Seed, dm_round};
use crate::chem::{name_of, visible};
use crate::construction::Job;
use crate::inventory::CarriedBy;
use crate::tools::{Actor, DoAfter, do_after, tell};

const COTTON: &str = "/obj/item/grown/cotton";
const DURATHREAD: &str = "/obj/item/grown/cotton/durathread";
const GRASS: &str = "/obj/item/food/grown/grass";
const FAIRYGRASS: &str = "/obj/item/food/grown/grass/fairy";
const CARPET: &str = "/obj/item/food/grown/grass/carpet";
const SHRUB: &str = "/obj/item/grown/shrub";
pub const HEDGE: &str = "/obj/structure/hedge/opaque";
/// `tile_coefficient`.
const TILE_COEFFICIENT: f32 = 0.02;
/// `do_after(user, 8 SECONDS)`, deciseconds.
const PLANT_SHRUB_DS: f32 = 80.0;

/// `cotton_type` of a bundle.
fn cotton_type(path: &str) -> &'static str {
    if is_under(path, DURATHREAD) {
        "/obj/item/stack/sheet/cotton/durathread"
    } else {
        "/obj/item/stack/sheet/cotton"
    }
}

/// `stacktype` of a grass clump.
fn grass_tiles(path: &str) -> &'static str {
    if is_under(path, FAIRYGRASS) {
        "/obj/item/stack/tile/fairygrass"
    } else if is_under(path, CARPET) {
        "/obj/item/stack/tile/carpet"
    } else {
        "/obj/item/stack/tile/grass"
    }
}

/// `1 + round(potency / 25)` (no seed: 1).
pub fn cotton_count(potency: Option<f32>) -> u32 {
    1 + potency.map_or(0.0, |p| dm_round(p / 25.0)) as u32
}

/// `1 + round(potency * tile_coefficient)`.
pub fn grass_count(potency: f32) -> u32 {
    1 + dm_round(potency * TILE_COEFFICIENT) as u32
}

fn path_of(world: &World, item: Entity) -> String {
    world
        .get::<DmItem>(item)
        .map(|i| i.path.clone())
        .unwrap_or_default()
}

fn potency(world: &World, item: Entity) -> Option<f32> {
    world.get::<Seed>(item).map(|s| s.potency)
}

/// `attack_self` of cotton, grass and shrubs. True when it was one of them.
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    let path = path_of(world, item);
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return false;
    };
    if is_under(&path, COTTON) {
        let count = cotton_count(potency(world, item));
        let s = if count == 1 { "" } else { "s" };
        crate::balloon::balloon_alert(world, mob, mob, &format!("pulled {count} piece{s}"));
        world.despawn(item);
        crate::construction::spawn_stack(world, cotton_type(&path), pos, count);
        return true;
    }
    if is_under(&path, GRASS) {
        tell(world, client, Span::Notice, "You prepare the astroturf.");
        let mut count = grass_count(potency(world, item).unwrap_or(0.0));
        let clumps: Vec<Entity> = world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .filter(|&e| e != item && world.get::<DmItem>(e).is_some_and(|i| i.path == path))
            .collect();
        for clump in clumps {
            count += grass_count(potency(world, clump).unwrap_or(0.0));
            world.despawn(clump);
        }
        world.despawn(item);
        crate::construction::spawn_stack(world, grass_tiles(&path), pos, count);
        return true;
    }
    if is_under(&path, SHRUB) {
        if blocked(world, pos) {
            return true;
        }
        let (user, name) = (name_of(world, mob), name_of(world, item));
        visible(
            world,
            mob,
            &format!("{user} begins to plant the {name}..."),
            &format!("{user} begins to plant the {name}..."),
            Span::Danger,
        );
        let actor = Actor {
            client,
            mob,
            item: Some(item),
        };
        do_after(
            world,
            actor,
            mob,
            PLANT_SHRUB_DS,
            0,
            Job::Botany(BotanyJob::PlantShrub),
        );
        return true;
    }
    false
}

/// `is_blocked_turf(exclude_mobs = TRUE)`: something dense but a mob on the tile.
fn blocked(world: &World, pos: TilePos) -> bool {
    world
        .resource::<SpatialIndex>()
        .dense_on_tile(pos)
        .iter()
        .any(|&e| world.get::<crate::health::Health>(e).is_none())
}

/// The shrub planted: still in the planter's hands, the tile still free.
pub fn complete_plant_shrub(world: &mut World, mob: Entity, action: &DoAfter) {
    let Some(shrub) = action.item else {
        return;
    };
    if world.get_entity(shrub).is_err() || world.get::<CarriedBy>(shrub).map(|c| c.0) != Some(mob) {
        return;
    }
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    if blocked(world, pos) {
        return;
    }
    let name = name_of(world, shrub);
    crate::construction::spawn_obj(world, HEDGE, pos);
    tell(
        world,
        action.client,
        Span::Notice,
        format!("You plant the {name}."),
    );
    world.despawn(shrub);
    crate::construction::tile_changed(world, pos);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_follow_the_potency() {
        assert_eq!(cotton_count(None), 1);
        assert_eq!(cotton_count(Some(24.0)), 1);
        assert_eq!(cotton_count(Some(50.0)), 3);
        assert_eq!(cotton_count(Some(100.0)), 5);
        assert_eq!(grass_count(49.0), 1);
        assert_eq!(grass_count(50.0), 2);
        assert_eq!(grass_tiles(CARPET), "/obj/item/stack/tile/carpet");
        assert_eq!(grass_tiles(GRASS), "/obj/item/stack/tile/grass");
        assert_eq!(
            cotton_type(DURATHREAD),
            "/obj/item/stack/sheet/cotton/durathread"
        );
    }
}
