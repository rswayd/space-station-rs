//! The map initialization pass, tg `SSatoms.InitializeAtoms` over the atoms of one map load.
//!
//! A load (the station at startup, a template mid-round) opens a pass ([`MapInitPass`]); every
//! entity spawned while it is open is [`Mapped`]. [`run_map_init`] then runs [`MapInit`] and
//! [`MapInitLate`], whose systems initialize the [`Mapped`] entities (doors, power, closets,
//! air...), and closes the pass. At startup it runs in `PostStartup`, after the station loaded
//! (`PreStartup`) and the data loaders ran (`Startup`); a mid-round load calls it itself.
//! The same systems so cover both: a template loaded mid-round comes up as alive as the station.

use bevy::ecs::schedule::ScheduleLabel;
use bevy::prelude::*;

/// Initializes [`Mapped`] entities (what `Startup` systems did for the station).
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct MapInit;

/// Runs after [`MapInit`] (what `PostStartup` systems did): air, wires, links between devices.
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct MapInitLate;

/// Spawned during the open [`MapInitPass`]: not initialized yet. Removed when the pass closes.
#[derive(Component, Clone, Copy, Debug, Default)]
#[component(storage = "SparseSet")]
pub struct Mapped;

/// An open map initialization pass: the signal that a load happened. Present from the load
/// until [`run_map_init`] closes it.
#[derive(Resource, Clone, Debug, Default)]
pub struct MapInitPass {
    /// The z-levels the pass loaded.
    pub levels: Vec<u16>,
}

/// Opens a pass (or extends the open one) with `levels`.
pub fn begin_map_init(world: &mut World, levels: impl IntoIterator<Item = u16>) {
    world
        .get_resource_or_insert_with(MapInitPass::default)
        .levels
        .extend(levels);
}

/// Whether an entity spawned now is [`Mapped`].
pub fn map_init_open(world: &World) -> bool {
    world.contains_resource::<MapInitPass>()
}

/// Runs [`MapInit`] then [`MapInitLate`] over the [`Mapped`] entities and closes the pass.
pub fn run_map_init(world: &mut World) {
    // A schedule nobody added systems to does not exist.
    let _ = world.try_run_schedule(MapInit);
    let _ = world.try_run_schedule(MapInitLate);
    world.remove_resource::<MapInitPass>();
    let mut query = world.query_filtered::<Entity, With<Mapped>>();
    let marked: Vec<Entity> = query.iter(world).collect();
    for e in marked {
        world.entity_mut(e).remove::<Mapped>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Component)]
    struct Seen;

    fn see(mut commands: Commands, fresh: Query<Entity, With<Mapped>>) {
        for e in &fresh {
            commands.entity(e).insert(Seen);
        }
    }

    #[test]
    fn a_pass_initializes_only_what_it_spawned() {
        let mut world = World::new();
        world.add_schedule(Schedule::new(MapInit));
        world.schedule_scope(MapInit, |_, s| {
            s.add_systems(see);
        });
        let old = world.spawn_empty().id();
        begin_map_init(&mut world, [3]);
        assert!(map_init_open(&world));
        let new = world.spawn(Mapped).id();
        run_map_init(&mut world);
        assert!(!map_init_open(&world));
        assert!(world.get::<Seen>(new).is_some());
        assert!(world.get::<Seen>(old).is_none());
        assert!(world.get::<Mapped>(new).is_none());
    }
}
