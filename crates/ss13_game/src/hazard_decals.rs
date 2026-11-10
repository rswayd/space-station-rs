//! Dangerous puddles (`effects/decals/cleanable/fuel.dm`, `mess.dm`).
//!
//! A fuel pool (`crate::janitor::expose_by`: at least 5 u of welding fuel on a turf) burns
//! `burn_amount` times: lit by fire, a hot item on it or entering it, a burning mob entering it,
//! or sparks, it makes a hotspot, lights the pools around it 0.5 s later and, when the hotspot
//! dies, burns again until it is gone. A radioactive goo pulses radiation.
// shortcut: bullets do not light pools (they pass decals here), landing thrown items do not
// either (no `COMSIG_TURF_MOVABLE_THROW_LANDED`); the glow's reagents (`decal_reagent`) are not
// kept, so it cannot be scooped back up.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_atmos::Atmos;
use ss13_atmos::reactions::FIRE_MINIMUM_TEMPERATURE_TO_EXIST;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmCleanable, DmItem};
use ss13_shared::TilePos;
use ss13_world::{Moved, SpatialIndex, WorldSystems};

use crate::click_handlers::{AddClickHandler, ClickPhase};
use crate::construction::spawn_obj;

pub const FUEL_POOL: &str = "/obj/effect/decal/cleanable/fuel_pool";
pub const GREENGLOW: &str = "/obj/effect/decal/cleanable/greenglow";
const GREENGLOW_RADIOACTIVE: &str = "/obj/effect/decal/cleanable/greenglow/radioactive";
/// `burn_amount`: the var's default and tg's limits.
const DEFAULT_BURN: i32 = 3;
const MAX_BURN: i32 = 10;
/// `hotspot` defaults (`volume`, `FIRE_MINIMUM_TEMPERATURE_TO_EXIST`).
const HOTSPOT_VOLUME: f64 = 125.0;
/// `addtimer(ignite_others, 0.5 SECONDS)`.
const SPREAD_SECS: f64 = 0.5;
/// `radioactive_emitter`: `cooldown_time`, `range`, `threshold` (`RAD_MEDIUM_INSULATION`).
const GOO_COOLDOWN: f64 = 5.0;
const GOO_RANGE: f32 = 4.0;
const GOO_THRESHOLD: f32 = 0.7;

/// `/obj/effect/decal/cleanable/fuel_pool`.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct FuelPool {
    pub burn_amount: i32,
    pub burning: bool,
    ignite_others_at: Option<f64>,
}

impl FuelPool {
    fn new(burn_amount: i32) -> Self {
        Self {
            burn_amount: burn_amount.clamp(1, MAX_BURN),
            burning: false,
            ignite_others_at: None,
        }
    }
}

pub struct HazardDecalsPlugin;

impl Plugin for HazardDecalsPlugin {
    fn build(&self, app: &mut App) {
        // `fuel_pool/item_interaction`: `ignition_effect` of a hot tool.
        app.add_click_handler(ClickPhase::HeldOnTarget, 3487, ignite_click);
        app.add_systems(
            Update,
            (
                init_new,
                on_entered.after(WorldSystems::Movement),
                tick_pools.run_if(any_with_component::<FuelPool>),
            ),
        );
    }
}

/// `round(volume / 5)` burns, between 1 and 10.
pub fn burns_for(volume: f32) -> i32 {
    ((volume / 5.0).floor() as i32).clamp(1, MAX_BURN)
}

fn pool_on(world: &World, pos: TilePos, except: Option<Entity>) -> Option<Entity> {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .find(|&e| Some(e) != except && world.get::<FuelPool>(e).is_some())
}

/// A placed pool is the `burn_amount` default, or merges into the one already on the tile; a
/// radioactive goo starts to pulse.
fn init_new(
    mut commands: Commands,
    added: Query<(Entity, &DmCleanable), (Added<DmCleanable>, Without<FuelPool>)>,
) {
    for (e, dm) in &added {
        if is_under(&dm.path, FUEL_POOL) {
            commands.queue(move |world: &mut World| {
                if world.get_entity(e).is_err() {
                    return;
                }
                let duplicate = world
                    .get::<TilePos>(e)
                    .is_some_and(|&pos| pool_on(world, pos, Some(e)).is_some());
                if duplicate {
                    world.despawn(e);
                } else {
                    world.entity_mut(e).insert(FuelPool::new(DEFAULT_BURN));
                }
            });
        } else if is_under(&dm.path, GREENGLOW_RADIOACTIVE) {
            commands
                .entity(e)
                .insert(crate::radiation::RadioactiveEmitter::new(
                    GOO_COOLDOWN,
                    GOO_RANGE,
                    GOO_THRESHOLD,
                    None,
                ));
        }
    }
}

/// `/datum/reagent/fuel/expose_turf`: the pool of `spawn_unique_cleanable`.
pub fn spawn_pool(world: &mut World, pos: TilePos, volume: f32) {
    let burn = burns_for(volume);
    if let Some(existing) = pool_on(world, pos, None) {
        if let Some(mut pool) = world.get_mut::<FuelPool>(existing) {
            pool.burn_amount = burn;
        }
        return;
    }
    if let Some(pool) = spawn_obj(world, FUEL_POOL, pos) {
        world.entity_mut(pool).insert(FuelPool::new(burn));
    }
}

/// `spawn_unique_cleanable(greenglow)`.
pub fn spawn_glow(world: &mut World, pos: TilePos) {
    let exists = world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .any(|&e| {
            world
                .get::<DmCleanable>(e)
                .is_some_and(|d| is_under(&d.path, GREENGLOW))
        });
    if !exists {
        spawn_obj(world, GREENGLOW, pos);
    }
}

/// `ignite`.
pub fn ignite(world: &mut World, pool: Entity) {
    let Some(mut state) = world.get_mut::<FuelPool>(pool) else {
        return;
    };
    if state.burning {
        return;
    }
    state.burning = true;
    burn_process(world, pool);
}

/// `burn_process`: one burn, the pool gone at the last.
fn burn_process(world: &mut World, pool: Entity) {
    let Some(&pos) = world.get::<TilePos>(pool) else {
        return;
    };
    let now = world.resource::<Time>().elapsed_secs_f64();
    if let Some(mut atmos) = world.get_resource_mut::<Atmos>() {
        atmos.new_hotspot(pos, HOTSPOT_VOLUME, FIRE_MINIMUM_TEMPERATURE_TO_EXIST);
    }
    let Some(mut state) = world.get_mut::<FuelPool>(pool) else {
        return;
    };
    state.burn_amount -= 1;
    state.ignite_others_at = Some(now + SPREAD_SECS);
    if state.burn_amount <= 0 {
        world.despawn(pool);
    }
}

/// `fuel_pool/fire_act`. True when `thing` is a pool.
pub fn fire_act(world: &mut World, thing: Entity) -> bool {
    if world.get::<FuelPool>(thing).is_none() {
        return false;
    }
    ignite(world, thing);
    true
}

/// `COMSIG_ATOM_TOUCHED_SPARKS` of the pools on a tile.
pub fn touched_sparks(world: &mut World, pos: TilePos) {
    if let Some(pool) = pool_on(world, pos, None) {
        ignite(world, pool);
    }
}

/// `item_interaction`: a hot tool lights it.
fn ignite_click(world: &mut World, c: &crate::click_handlers::ClickCtx) -> bool {
    let Some(held) = c.held else {
        return false;
    };
    if world.get::<FuelPool>(c.target).is_none() || !crate::fireplace::is_hot(world, held) {
        return false;
    }
    ignite(world, c.target);
    true
}

/// `on_entered`: a hot item or a burning mob sets it off; things thrown over it do not.
// Cost: per tick, one tile lookup per `Moved` that finds a pool only where pools are.
fn on_entered(world: &mut World, mut cursor: Local<MessageCursor<Moved>>) {
    let moved: Vec<Moved> = cursor
        .read(world.resource::<Messages<Moved>>())
        .copied()
        .collect();
    for m in moved {
        let Some(pool) = pool_on(world, m.to, Some(m.entity)) else {
            continue;
        };
        if world.get::<crate::throw::Thrown>(m.entity).is_some() {
            continue;
        }
        let lights = if world.get::<DmItem>(m.entity).is_some() {
            crate::fireplace::is_hot(world, m.entity)
        } else {
            crate::fire::on_fire(world, m.entity)
        };
        if lights {
            ignite(world, pool);
        }
    }
}

/// `ignite_others` 0.5 s after each burn, and the next burn when the hotspot is gone.
fn tick_pools(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let burning: Vec<(Entity, FuelPool, TilePos)> = world
        .query::<(Entity, &FuelPool, &TilePos)>()
        .iter(world)
        .filter(|(_, p, _)| p.burning)
        .map(|(e, p, pos)| (e, *p, *pos))
        .collect();
    for (e, pool, pos) in burning {
        if pool.ignite_others_at.is_some_and(|t| now >= t) {
            if let Some(mut state) = world.get_mut::<FuelPool>(e) {
                state.ignite_others_at = None;
            }
            for dx in -1..=1 {
                for dy in -1..=1 {
                    let tile = TilePos::new(pos.x + dx, pos.y + dy, pos.z);
                    if let Some(other) = pool_on(world, tile, None) {
                        ignite(world, other);
                    }
                }
            }
        }
        let fire_gone = world
            .get_resource::<Atmos>()
            .is_none_or(|a| a.hotspot(pos).is_none());
        if fire_gone && world.get::<FuelPool>(e).is_some() {
            burn_process(world, e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pool_holds_one_burn_per_five_units() {
        assert_eq!(burns_for(5.0), 1);
        assert_eq!(burns_for(14.0), 2);
        assert_eq!(burns_for(500.0), 10);
    }
}
