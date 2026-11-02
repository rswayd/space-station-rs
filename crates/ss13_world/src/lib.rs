//! Authoritative world state. Runs headless; no rendering or networking types.

use bevy::prelude::*;
use ss13_shared::TilePos;

pub mod index;
pub mod movement;

pub use index::{CHUNK_SIZE, ChunkPos, LevelError, SpatialIndex};
pub use movement::{
    Border, Bumped, ClimbWalkable, Density, MoveRequest, Moved, Mover, next_ready_at, resolve_move,
};

/// Ordering points for other plugins: write `MoveRequest` before `Movement`, read `Moved`
/// and fresh positions after it.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum WorldSystems {
    Movement,
}

/// Must be added before any entity with `TilePos` is spawned: the index hooks can only be
/// registered while no archetype contains `TilePos`.
pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SpatialIndex>()
            .add_message::<MoveRequest>()
            .add_message::<Moved>()
            .add_message::<Bumped>()
            .add_systems(
                Update,
                movement::apply_move_requests.in_set(WorldSystems::Movement),
            );
        // Hooks, not observers: one fn-pointer call per change, no observer dispatch, which
        // matters for map load (200k inserts) and every step. Insert + Discard together cover
        // spawn, replace, remove and despawn.
        app.world_mut()
            .register_component_hooks::<TilePos>()
            .try_on_insert(index::on_tile_pos_insert)
            .and_then(|h| h.try_on_discard(index::on_tile_pos_discard))
            .expect("TilePos hooks are owned by ss13_world");
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use bevy::time::TimeUpdateStrategy;
    use ss13_shared::{Dir, Facing};

    use super::*;

    const TICK: Duration = Duration::from_millis(50);

    /// `Moved` seen during the last `update`. Read through a `MessageReader`, because the message
    /// buffers only swap after a fixed-timestep run, so raw buffers can hold older ticks.
    #[derive(Resource, Default)]
    struct MovedLog(Vec<Moved>);

    fn log_moved(mut reader: MessageReader<Moved>, mut log: ResMut<MovedLog>) {
        log.0.extend(reader.read());
    }

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, WorldPlugin))
            .insert_resource(TimeUpdateStrategy::ManualDuration(TICK))
            .init_resource::<MovedLog>()
            .add_systems(Update, log_moved.after(WorldSystems::Movement));
        index(&mut app).create_level(1, 20, 20).unwrap();
        app
    }

    fn index(app: &mut App) -> Mut<'_, SpatialIndex> {
        app.world_mut().resource_mut::<SpatialIndex>()
    }

    fn on_tile(app: &App, x: i32, y: i32) -> Vec<Entity> {
        app.world()
            .resource::<SpatialIndex>()
            .on_tile(TilePos::new(x, y, 1))
            .to_vec()
    }

    fn mover(app: &mut App, x: i32, y: i32, delay: f32) -> Entity {
        app.world_mut()
            .spawn((
                TilePos::new(x, y, 1),
                Mover::new(delay),
                Density,
                Facing::default(),
            ))
            .id()
    }

    fn wall(app: &mut App, x: i32, y: i32) -> Entity {
        app.world_mut().spawn((TilePos::new(x, y, 1), Density)).id()
    }

    /// Sends one request, runs one tick, returns the `Moved` messages of that tick.
    fn request(app: &mut App, entity: Entity, dir: Dir) -> Vec<Moved> {
        app.world_mut().write_message(MoveRequest { entity, dir });
        tick(app)
    }

    fn tick(app: &mut App) -> Vec<Moved> {
        app.world_mut().resource_mut::<MovedLog>().0.clear();
        app.update();
        app.world().resource::<MovedLog>().0.clone()
    }

    fn pos(app: &App, e: Entity) -> TilePos {
        *app.world().get::<TilePos>(e).unwrap()
    }

    fn facing(app: &App, e: Entity) -> Dir {
        app.world().get::<Facing>(e).unwrap().0
    }

    fn window(app: &mut App, x: i32, y: i32, dir: Dir) -> Entity {
        app.world_mut()
            .spawn((Border::facing(dir, false), Density, TilePos::new(x, y, 1)))
            .id()
    }

    fn bumped_since(
        app: &App,
        cursor: &mut bevy::ecs::message::MessageCursor<Bumped>,
    ) -> Vec<Entity> {
        cursor
            .read(app.world().resource::<Messages<Bumped>>())
            .map(|b| b.obstacle)
            .collect()
    }

    #[test]
    fn border_blocks_only_its_edge_entering_and_leaving() {
        let mut app = app();
        let w = window(&mut app, 5, 5, Dir::North);
        let mut bumps = app
            .world_mut()
            .resource_mut::<Messages<Bumped>>()
            .get_cursor();
        // Into its tile from the south and the sides: free.
        let m = mover(&mut app, 5, 4, 0.0);
        assert_eq!(request(&mut app, m, Dir::North).len(), 1);
        assert_eq!(request(&mut app, m, Dir::West).len(), 1);
        assert_eq!(request(&mut app, m, Dir::East).len(), 1);
        assert_eq!(pos(&app, m), TilePos::new(5, 5, 1));
        assert!(bumped_since(&app, &mut bumps).is_empty());
        // Out through the edge it faces: refused, bumped, turned.
        assert!(request(&mut app, m, Dir::North).is_empty());
        assert_eq!(facing(&app, m), Dir::North);
        assert_eq!(bumped_since(&app, &mut bumps), [w]);
        // Into its tile from the north: refused.
        assert_eq!(request(&mut app, m, Dir::South).len(), 1);
        let n = mover(&mut app, 5, 7, 0.0);
        assert_eq!(request(&mut app, n, Dir::South).len(), 1);
        assert!(request(&mut app, n, Dir::South).is_empty());
        assert_eq!(bumped_since(&app, &mut bumps), [w]);
        // Not a full-tile blocker: spawning and dropping see the tile as free.
        let index = app.world().resource::<SpatialIndex>();
        assert!(index.dense_on_tile(TilePos::new(5, 5, 1)).is_empty());
        assert_eq!(index.on_tile(TilePos::new(5, 5, 1)), [w]);
    }

    #[test]
    fn border_follows_density_replace_and_despawn() {
        let mut app = app();
        let w = window(&mut app, 5, 5, Dir::North);
        let m = mover(&mut app, 5, 6, 0.0);
        let border = |app: &App| {
            app.world()
                .resource::<SpatialIndex>()
                .border_on_tile(TilePos::new(5, 5, 1))
                .to_vec()
        };
        assert_eq!(border(&app), [(w, Dir::North as u8)]);
        // An open windoor: not dense, passable from every side.
        app.world_mut().entity_mut(w).remove::<Density>();
        assert!(border(&app).is_empty());
        assert_eq!(request(&mut app, m, Dir::South).len(), 1);
        assert_eq!(request(&mut app, m, Dir::North).len(), 1);
        app.world_mut().entity_mut(w).insert(Density);
        assert_eq!(border(&app), [(w, Dir::North as u8)]);
        assert!(request(&mut app, m, Dir::South).is_empty());
        // Turned to face east: the north edge is free, the east one blocks.
        app.world_mut()
            .entity_mut(w)
            .insert(Border::facing(Dir::East, false));
        assert_eq!(border(&app), [(w, Dir::East as u8)]);
        assert_eq!(request(&mut app, m, Dir::South).len(), 1);
        assert!(request(&mut app, m, Dir::East).is_empty());
        // No longer a border object: a full-tile blocker.
        app.world_mut().entity_mut(w).remove::<Border>();
        assert!(border(&app).is_empty());
        let dense = |app: &App| {
            app.world()
                .resource::<SpatialIndex>()
                .dense_on_tile(TilePos::new(5, 5, 1))
                .to_vec()
        };
        assert!(dense(&app).contains(&w));
        app.world_mut()
            .entity_mut(w)
            .insert(Border::facing(Dir::West, false));
        assert!(!dense(&app).contains(&w));
        app.world_mut().despawn(w);
        assert!(border(&app).is_empty());
        assert_eq!(on_tile(&app, 5, 5), [m]);
        assert_eq!(dense(&app), [m]);
    }

    #[test]
    fn diagonals_step_through_border_edges_like_cardinals() {
        let mut app = app();
        // Leaving north is refused by its own tile's window: goes east, then north.
        window(&mut app, 5, 5, Dir::North);
        let m = mover(&mut app, 5, 5, 0.0);
        let steps: Vec<_> = request(&mut app, m, Dir::NorthEast)
            .iter()
            .map(|e| e.dir)
            .collect();
        assert_eq!(steps, [Dir::East, Dir::North]);
        assert_eq!(pos(&app, m), TilePos::new(6, 6, 1));

        // The target's west edge refuses the second half: slides north and faces north.
        window(&mut app, 11, 11, Dir::West);
        let m = mover(&mut app, 10, 10, 0.0);
        assert_eq!(request(&mut app, m, Dir::NorthEast).len(), 1);
        assert_eq!(pos(&app, m), TilePos::new(10, 11, 1));
        assert_eq!(facing(&app, m), Dir::North);

        // Facing south it is passed from the west: the diagonal is reached.
        window(&mut app, 16, 16, Dir::South);
        let m = mover(&mut app, 15, 15, 0.0);
        assert_eq!(request(&mut app, m, Dir::NorthEast).len(), 2);
        assert_eq!(pos(&app, m), TilePos::new(16, 16, 1));

        // Both halves refused by a corner of two panes on its own tile.
        window(&mut app, 3, 15, Dir::North);
        window(&mut app, 3, 15, Dir::East);
        let m = mover(&mut app, 3, 15, 0.0);
        assert!(request(&mut app, m, Dir::NorthEast).is_empty());
        assert_eq!(pos(&app, m), TilePos::new(3, 15, 1));
        assert_eq!(request(&mut app, m, Dir::SouthWest).len(), 2);
    }

    #[test]
    fn diagonal_border_facing() {
        let railing = Border::facing(Dir::NorthEast, true);
        assert!(railing.blocks(Dir::North) && railing.blocks(Dir::East));
        assert!(!railing.blocks(Dir::South));
        let pane = Border::facing(Dir::NorthEast, false);
        assert!(Dir::CARDINALS.iter().all(|&d| !pane.blocks(d)));
        assert!(Border::facing(Dir::West, false).blocks(Dir::West));
    }

    #[test]
    fn reach_across_border_edges() {
        let mut app = app();
        let w = window(&mut app, 5, 5, Dir::North);
        let reach = |app: &App, from: (i32, i32), to: (i32, i32), ignore: Option<Entity>| {
            app.world().resource::<SpatialIndex>().adjacent(
                TilePos::new(from.0, from.1, 1),
                TilePos::new(to.0, to.1, 1),
                |e| Some(e) == ignore,
            )
        };
        assert!(!reach(&app, (5, 5), (5, 6), None));
        assert!(!reach(&app, (5, 6), (5, 5), None));
        assert!(reach(&app, (5, 6), (5, 5), Some(w)), "the target itself");
        assert!(reach(&app, (5, 5), (6, 5), None));
        assert!(reach(&app, (5, 4), (5, 5), None));
        // Diagonal: around the pane through the east side tile.
        assert!(reach(&app, (5, 5), (6, 6), None));
        window(&mut app, 6, 5, Dir::North);
        assert!(!reach(&app, (5, 5), (6, 6), None));
        assert!(!reach(&app, (6, 6), (5, 5), None));
        assert!(reach(&app, (4, 6), (5, 5), None), "west side tile is free");
    }

    #[test]
    fn index_follows_insert_replace_remove_despawn() {
        let mut app = app();
        let e = app.world_mut().spawn(TilePos::new(2, 3, 1)).id();
        assert_eq!(on_tile(&app, 2, 3), [e]);

        app.world_mut()
            .entity_mut(e)
            .insert(TilePos::new(17, 18, 1));
        assert!(on_tile(&app, 2, 3).is_empty());
        assert_eq!(on_tile(&app, 17, 18), [e]);

        app.world_mut().entity_mut(e).remove::<TilePos>();
        assert!(on_tile(&app, 17, 18).is_empty());

        app.world_mut().entity_mut(e).insert(TilePos::new(4, 4, 1));
        let other = app.world_mut().spawn(TilePos::new(4, 4, 1)).id();
        app.world_mut().despawn(e);
        assert_eq!(on_tile(&app, 4, 4), [other]);

        // Off-map positions are not indexed and do not break later removal.
        let lost = app.world_mut().spawn(TilePos::new(0, 4, 1)).id();
        let no_level = app.world_mut().spawn(TilePos::new(1, 1, 7)).id();
        app.world_mut().despawn(lost);
        app.world_mut().despawn(no_level);
        assert_eq!(on_tile(&app, 4, 4), [other]);
    }

    #[test]
    fn rect_radius_and_chunk_queries() {
        let mut app = app();
        let near = app.world_mut().spawn(TilePos::new(10, 10, 1)).id();
        let edge = app.world_mut().spawn(TilePos::new(19, 1, 1)).id();
        let far = app.world_mut().spawn(TilePos::new(20, 20, 1)).id();
        let index = app.world().resource::<SpatialIndex>();

        let mut got: Vec<_> = index.in_radius(TilePos::new(14, 5, 1), 5).collect();
        got.sort();
        let mut want = vec![near, edge];
        want.sort();
        assert_eq!(got, want);
        assert_eq!(index.in_rect(1, (-5, -5), (100, 100)).count(), 3);
        assert_eq!(index.in_radius(TilePos::new(10, 10, 2), 50).count(), 0);

        // 20x20 level = 2x2 chunks; tile 17 is the first tile of chunk 1.
        assert_eq!(
            index.chunks_in_radius(TilePos::new(16, 16, 1), 0).count(),
            1
        );
        let chunks: Vec<_> = index.chunks_in_radius(TilePos::new(16, 16, 1), 1).collect();
        assert_eq!(chunks.len(), 4);
        let top_right = ChunkPos { x: 1, y: 1, z: 1 };
        assert_eq!(ChunkPos::of(TilePos::new(17, 17, 1)), top_right);
        assert_eq!(index.in_chunk(top_right).collect::<Vec<_>>(), [far]);
        assert_eq!(index.in_chunk(ChunkPos { x: 2, y: 0, z: 1 }).count(), 0);
    }

    #[test]
    fn move_updates_index_and_emits_moved() {
        let mut app = app();
        let m = mover(&mut app, 5, 5, 0.0);
        let events = request(&mut app, m, Dir::East);
        assert_eq!(
            events,
            [Moved {
                entity: m,
                from: TilePos::new(5, 5, 1),
                to: TilePos::new(6, 5, 1),
                dir: Dir::East
            }]
        );
        assert!(on_tile(&app, 5, 5).is_empty());
        assert_eq!(on_tile(&app, 6, 5), [m]);
        assert_eq!(facing(&app, m), Dir::East);
    }

    #[test]
    fn facing_is_written_only_when_it_turns_and_never_added() {
        let mut app = app();
        let m = mover(&mut app, 5, 5, 0.0);
        request(&mut app, m, Dir::East);
        let turned = app.world().entity(m).get_change_ticks::<Facing>().unwrap();
        assert_eq!(request(&mut app, m, Dir::East).len(), 1);
        let same = app.world().entity(m).get_change_ticks::<Facing>().unwrap();
        assert_eq!(turned.changed, same.changed);

        let pushed = mover(&mut app, 10, 10, 0.0);
        app.world_mut().entity_mut(pushed).remove::<Facing>();
        assert_eq!(request(&mut app, pushed, Dir::North).len(), 1);
        assert!(app.world().get::<Facing>(pushed).is_none());
    }

    #[test]
    fn density_blocks_but_turns_the_mover() {
        let mut app = app();
        let m = mover(&mut app, 5, 5, 0.0);
        let w = wall(&mut app, 5, 6);
        let mut bumps = app
            .world_mut()
            .resource_mut::<Messages<Bumped>>()
            .get_cursor();
        assert!(request(&mut app, m, Dir::North).is_empty());
        assert_eq!(pos(&app, m), TilePos::new(5, 5, 1));
        assert_eq!(facing(&app, m), Dir::North);
        let seen: Vec<Bumped> = bumps
            .read(app.world().resource::<Messages<Bumped>>())
            .copied()
            .collect();
        assert_eq!(
            seen,
            [Bumped {
                entity: m,
                obstacle: w
            }]
        );

        // Non-dense things on the tile do not block.
        app.world_mut().spawn(TilePos::new(5, 4, 1));
        assert_eq!(request(&mut app, m, Dir::South).len(), 1);
    }

    #[test]
    fn density_changes_and_despawn_update_blocking() {
        let mut app = app();
        let m = mover(&mut app, 5, 5, 0.0);
        let door = wall(&mut app, 6, 5);
        let dense = |app: &App| {
            app.world()
                .resource::<SpatialIndex>()
                .dense_on_tile(TilePos::new(6, 5, 1))
                .to_vec()
        };
        assert_eq!(dense(&app), [door]);
        app.world_mut().entity_mut(door).insert(Density);
        assert_eq!(dense(&app), [door]);

        app.world_mut().entity_mut(door).remove::<Density>();
        assert!(dense(&app).is_empty());
        assert_eq!(on_tile(&app, 6, 5), [door]);
        app.world_mut().entity_mut(door).insert(Density);
        assert!(request(&mut app, m, Dir::East).is_empty());

        app.world_mut().despawn(door);
        assert!(dense(&app).is_empty());
        assert!(on_tile(&app, 6, 5).is_empty());
        assert_eq!(request(&mut app, m, Dir::East).len(), 1);
    }

    #[test]
    fn dense_movers_cannot_share_a_tile_in_one_tick() {
        let mut app = app();
        let a = mover(&mut app, 4, 5, 0.0);
        let b = mover(&mut app, 6, 5, 0.0);
        app.world_mut().write_message(MoveRequest {
            entity: a,
            dir: Dir::East,
        });
        app.world_mut().write_message(MoveRequest {
            entity: b,
            dir: Dir::West,
        });
        assert_eq!(tick(&mut app).len(), 1);
        assert_eq!(on_tile(&app, 5, 5).len(), 1);
    }

    #[test]
    fn map_edge_blocks_without_turning() {
        let mut app = app();
        let m = mover(&mut app, 1, 20, 0.0);
        assert!(request(&mut app, m, Dir::West).is_empty());
        assert!(request(&mut app, m, Dir::North).is_empty());
        assert!(request(&mut app, m, Dir::NorthWest).is_empty());
        assert_eq!(pos(&app, m), TilePos::new(1, 20, 1));
        assert_eq!(facing(&app, m), Dir::South);
    }

    #[test]
    fn diagonal_free_takes_vertical_then_horizontal() {
        let mut app = app();
        let m = mover(&mut app, 5, 5, 0.0);
        let events = request(&mut app, m, Dir::NorthEast);
        let steps: Vec<_> = events.iter().map(|e| (e.dir, e.to)).collect();
        assert_eq!(
            steps,
            [
                (Dir::North, TilePos::new(5, 6, 1)),
                (Dir::East, TilePos::new(6, 6, 1))
            ]
        );
        assert_eq!(facing(&app, m), Dir::East);
    }

    #[test]
    fn diagonal_goes_around_a_blocked_vertical() {
        let mut app = app();
        let m = mover(&mut app, 5, 5, 0.0);
        wall(&mut app, 5, 6);
        request(&mut app, m, Dir::NorthEast);
        assert_eq!(pos(&app, m), TilePos::new(6, 6, 1));
        assert_eq!(facing(&app, m), Dir::North);
    }

    #[test]
    fn diagonal_slides_when_target_is_blocked() {
        let mut app = app();
        let m = mover(&mut app, 5, 5, 0.0);
        wall(&mut app, 6, 6);
        let events = request(&mut app, m, Dir::NorthEast);
        assert_eq!(events.len(), 1);
        assert_eq!(pos(&app, m), TilePos::new(5, 6, 1));
        assert_eq!(facing(&app, m), Dir::North);
    }

    #[test]
    fn diagonal_fails_when_both_cardinals_are_blocked() {
        let mut app = app();
        let m = mover(&mut app, 5, 5, 0.0);
        wall(&mut app, 5, 6);
        wall(&mut app, 6, 5);
        assert!(request(&mut app, m, Dir::NorthEast).is_empty());
        assert_eq!(pos(&app, m), TilePos::new(5, 5, 1));
    }

    #[test]
    fn cooldown_is_per_second_and_spent_when_blocked() {
        let mut app = app();
        // 0.25 s is exact in f32 and a multiple of the 50 ms tick.
        let m = mover(&mut app, 2, 2, 0.25);
        let mut moves = 0;
        for _ in 0..12 {
            moves += request(&mut app, m, Dir::North).len();
        }
        let elapsed = app.world().resource::<Time>().elapsed_secs_f64();
        // One move per 0.25 s from t = 0, inclusive of the last tick.
        assert_eq!(moves, (elapsed / 0.25).floor() as usize + 1, "{elapsed}");

        let blocked = mover(&mut app, 10, 10, 0.25);
        wall(&mut app, 10, 11);
        request(&mut app, blocked, Dir::North);
        let mover = app.world().get::<Mover>(blocked).unwrap();
        let now = app.world().resource::<Time>().elapsed_secs_f64();
        assert!(mover.ready_at() > now);
    }

    #[test]
    fn diagonal_costs_sqrt2() {
        let mut app = app();
        let m = mover(&mut app, 5, 5, 1.0);
        app.update();
        let start = app.world().resource::<Time>().elapsed_secs_f64();
        request(&mut app, m, Dir::NorthEast);
        let ready = app.world().get::<Mover>(m).unwrap().ready_at();
        let now = app.world().resource::<Time>().elapsed_secs_f64();
        assert!(now > start);
        assert!(
            (ready - now - std::f64::consts::SQRT_2).abs() < 1e-6,
            "{ready} {now}"
        );
    }

    #[test]
    fn requests_for_invalid_entities_are_ignored() {
        let mut app = app();
        let static_thing = app.world_mut().spawn(TilePos::new(3, 3, 1)).id();
        let gone = mover(&mut app, 4, 4, 0.0);
        app.world_mut().despawn(gone);
        assert!(request(&mut app, static_thing, Dir::North).is_empty());
        assert!(request(&mut app, gone, Dir::North).is_empty());
        assert_eq!(pos(&app, static_thing), TilePos::new(3, 3, 1));
    }

    #[test]
    fn diagonal_reach_needs_a_free_side_tile() {
        let mut app = app();
        let from = TilePos::new(5, 5, 1);
        let ne = TilePos::new(6, 6, 1);
        fn index(app: &App) -> &SpatialIndex {
            app.world().resource::<SpatialIndex>()
        }
        assert!(index(&app).adjacent(from, from, |_| false));
        assert!(index(&app).adjacent(from, TilePos::new(5, 6, 1), |_| false));
        assert!(index(&app).adjacent(from, ne, |_| false));
        assert!(!index(&app).adjacent(from, TilePos::new(7, 5, 1), |_| false));
        assert!(!index(&app).adjacent(from, TilePos::new(5, 5, 2), |_| false));
        // A wall corner: one side blocked still reaches, both blocked does not.
        wall(&mut app, 5, 6);
        assert!(index(&app).adjacent(from, ne, |_| false));
        let blocker = app.world_mut().spawn((TilePos::new(6, 5, 1), Density)).id();
        assert!(!index(&app).adjacent(from, ne, |_| false));
        assert!(
            index(&app).adjacent(from, ne, |e| e == blocker),
            "ignored blockers"
        );
        // Orthogonal neighbors are reached even when dense themselves.
        assert!(index(&app).adjacent(from, TilePos::new(5, 6, 1), |_| false));
    }
}
