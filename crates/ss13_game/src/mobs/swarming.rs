//! tg `/datum/component/swarming`: swarming mobs sharing a tile each shift by their own random
//! pixel offset so the pile reads as a swarm, and shift back once alone on a tile.
//!
//! A member's offset is rolled once at `Initialize` (`rand(-max_x, max_x)`, `rand(-max_y,
//! max_y)`); tg's pairwise `swarm_members` amounts to "another swarming mob on my tile", which
//! is recomputed for both tiles of a move, a spawn, a pick-up (the mob leaves the map) and a
//! despawn.
// shortcut: the offset snaps instead of tg's 2 ds `animate`; animate it when the client gets
// persistent offset tweens.

use bevy::ecs::system::SystemState;
use bevy::prelude::*;
use ss13_mapload::DmAtom;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::mobs::MobDef;
use ss13_shared::{Appearance, TilePos};
use ss13_world::SpatialIndex;

use crate::combat::CombatRng;

/// A swarming mob.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Swarming {
    /// `offset_x`, `offset_y`.
    pub offset: [i16; 2],
    /// `is_swarming`.
    pub active: bool,
    /// The tile it was last seen on, to recount the one it left.
    at: Option<TilePos>,
}

/// The `AddComponent(/datum/component/swarming, max_x, max_y)` calls of basic mob
/// `Initialize`s (24 × 24 by default).
const SWARMERS: [(&str, i16, i16); 13] = [
    // BandaStation: carp swarm (`carp.dm`).
    ("/mob/living/basic/carp", 20, 20),
    ("/mob/living/basic/mouse", 16, 16),
    ("/mob/living/basic/bee", 24, 24),
    ("/mob/living/basic/garden_gnome", 24, 24),
    ("/mob/living/basic/snake/banded", 24, 24),
    ("/mob/living/basic/supermatter_spider", 24, 24),
    ("/mob/living/basic/spider/growing/spiderling", 24, 24),
    ("/mob/living/basic/living_limb_flesh", 8, 8),
    ("/mob/living/basic/mining/legion_brood", 12, 12),
    ("/mob/living/basic/hivelord_brood", 24, 24),
    ("/mob/living/basic/mining/lobstrosity/juvenile", 16, 11),
    ("/mob/living/basic/heretic_summon/fire_shark", 24, 24),
    ("/mob/living/basic/viscerator", 24, 24),
];

/// `max_x`, `max_y` of a swarming type.
pub fn swarm_range(path: &str) -> Option<(i16, i16)> {
    SWARMERS
        .iter()
        .find(|(root, ..)| is_under(path, root))
        .map(|&(_, x, y)| (x, y))
}

pub(super) fn build(app: &mut App) {
    app.add_observer(on_swarmer_removed).add_systems(
        Update,
        follow_tiles.after(ss13_world::WorldSystems::Movement),
    );
}

/// The swarming part of a new basic mob (`Initialize`).
pub(super) fn attach(world: &mut World, mob: Entity, def: &MobDef) {
    let Some((max_x, max_y)) = swarm_range(&def.path) else {
        return;
    };
    let mut rng = CombatRng(world.resource::<CombatRng>().0);
    let mut roll = |max: i16| rng.range(0, 2 * max as u32) as i16 - max;
    let offset = [roll(max_x), roll(max_y)];
    world.resource_mut::<CombatRng>().0 = rng.0;
    world.entity_mut(mob).insert(Swarming {
        offset,
        active: false,
        at: None,
    });
}

/// Recounts the tiles of swarming mobs that moved, appeared or left the map.
#[allow(clippy::type_complexity)]
fn follow_tiles(
    world: &mut World,
    state: &mut SystemState<(
        Query<Entity, (With<Swarming>, Or<(Changed<TilePos>, Added<Swarming>)>)>,
        RemovedComponents<TilePos>,
    )>,
) {
    let mobs: Vec<Entity> = {
        let Ok((changed, mut lost)) = state.get_mut(world) else {
            return;
        };
        changed.iter().chain(lost.read()).collect()
    };
    let mut tiles = Vec::new();
    for mob in mobs {
        let now = world.get::<TilePos>(mob).copied();
        let old = {
            let Some(mut swarm) = world.get_mut::<Swarming>(mob) else {
                continue;
            };
            if swarm.at == now {
                continue;
            }
            std::mem::replace(&mut swarm.at, now)
        };
        tiles.extend(old);
        match now {
            Some(pos) => tiles.push(pos),
            None => set_active(world, mob, false),
        }
    }
    tiles.sort_unstable_by_key(|p| (p.z, p.y, p.x));
    tiles.dedup();
    for pos in tiles {
        recount(world, pos);
    }
}

/// tg `swarming/Destroy`: the others on its tile recount without it.
fn on_swarmer_removed(
    remove: On<Remove<Swarming>>,
    swarms: Query<&Swarming>,
    mut commands: Commands,
) {
    if let Some(pos) = swarms.get(remove.entity).ok().and_then(|s| s.at) {
        commands.queue(move |world: &mut World| recount(world, pos));
    }
}

/// `join_swarm` / `leave_swarm` for one tile: every swarming mob there swarms while another is
/// with it.
pub fn recount(world: &mut World, pos: TilePos) {
    let members: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .filter(|&e| world.get::<Swarming>(e).is_some() && world.get::<TilePos>(e) == Some(&pos))
        .collect();
    let swarm = members.len() >= 2;
    for mob in members {
        set_active(world, mob, swarm);
    }
}

/// `swarm()` / `unswarm()`: the offset on top of the type's own `pixel_x`/`pixel_y`.
fn set_active(world: &mut World, mob: Entity, active: bool) {
    let Some(mut swarm) = world.get_mut::<Swarming>(mob) else {
        return;
    };
    if swarm.active == active {
        return;
    }
    swarm.active = active;
    let offset = if active { swarm.offset } else { [0, 0] };
    let base = world
        .get::<DmAtom>(mob)
        .map_or([0, 0], |a| [a.pixel_x, a.pixel_y]);
    if let Some(mut look) = world.get_mut::<Appearance>(mob) {
        look.pixel_offset = [base[0] + offset[0], base[1] + offset[1]];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swarm_ranges_follow_initialize() {
        assert_eq!(swarm_range("/mob/living/basic/carp/mega"), Some((20, 20)));
        assert_eq!(
            swarm_range("/mob/living/basic/mouse/hamster"),
            Some((16, 16))
        );
        assert_eq!(
            swarm_range("/mob/living/basic/mining/lobstrosity/juvenile/lava"),
            Some((16, 11))
        );
        // Only the juvenile lobstrosity swarms.
        assert_eq!(swarm_range("/mob/living/basic/mining/lobstrosity"), None);
        assert_eq!(swarm_range("/mob/living/basic/pet/dog/corgi"), None);
    }
}
