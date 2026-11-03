//! Client-side prediction of the own mob's walking, reconciled against the server's
//! [`MoveAck`]s. Headless: needs only `ss13_net::client::NetClientPlugin`.
//!
//! While a direction is held ([`MoveInput`]) and the local copy of the run cooldown allows, the
//! client takes the step at once, by the same rules as `ss13_world` ([`resolve_move`],
//! [`next_ready_at`]), and sends it as `Move { dir, seq }`. Unacked steps stay in a buffer. Each
//! ack carries the server's tile and facing after step `seq`: the buffer drops steps up to `seq`
//! and the rest is replayed from the server's state. The result is written to the own mob's
//! `TilePos`/`Facing`, so drawing, gliding and the camera need nothing special; every other
//! entity keeps the server's `TilePos`.
//!
//! Blocking: the server replicates what blocks walking (walls, closed doors, windows, tables,
//! border objects with their edges; `ss13_net` files them in the client's
//! [`SpatialIndex`]), so a step into them is not taken locally, by the same `exit_blockers` /
//! `enter_blockers` rule as the server's. The step is still sent: a closed door opens on the
//! bump, its density replicates away and the next step passes. Mobs are not replicated as
//! blockers (they are pushed, swap places and walk away): a step into one is predicted and, if
//! the server refuses it, learnt from the ack as blocked until the server moves the mob
//! somewhere else. A ghost or the AI's eye ([`OwnPhasing`]) ignores blockers.

use std::collections::VecDeque;

use bevy::platform::collections::HashSet;
use bevy::prelude::*;
use ss13_net::client::{NetClientState, NetEntityMap, SendIntent};
use ss13_net::move_ack::client::MoveAckReceived;
use ss13_shared::protocol::{ClientIntent, MoveAck, TICK_DURATION};
use ss13_shared::{Dir, Facing, TilePos};
use ss13_world::{ClimbWalkable, SpatialIndex, next_ready_at, resolve_move};

use crate::{MAX_UNACKED_STEPS, RUN_DELAY_SECS};

pub struct PredictionPlugin;

/// The direction the player holds; written by input before [`PredictionSystems`].
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MoveInput(pub Option<Dir>);

/// Seconds per tile of the own mob as the server last reported it (`crate::health_client`; slower
/// while hurt or crawling, one tick for a ghost). Prediction steps at this rate.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct OwnMoveDelay(pub f32);

impl Default for OwnMoveDelay {
    fn default() -> Self {
        Self(RUN_DELAY_SECS)
    }
}

/// The server reported the own mob cannot move (crit, stuns, stamcrit; `crate::health_client`):
/// no steps are predicted or sent until it can.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OwnImmobile(pub bool);

/// The own mob walks through anything (a ghost, the AI's eye; `crate::health_client`).
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OwnPhasing(pub bool);

/// `Update`: reconcile, predict, write the own mob.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PredictionSystems;

impl Plugin for PredictionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MoveInput>()
            .init_resource::<Prediction>()
            .init_resource::<OwnMoveDelay>()
            .init_resource::<OwnImmobile>()
            .init_resource::<OwnPhasing>()
            .add_systems(
                Update,
                (track_control, reconcile, predict_steps, write_own_mob)
                    .chain()
                    .in_set(PredictionSystems),
            );
    }
}

/// A predicted step: where it ended and the cardinal moves (from, direction) it made.
#[derive(Clone, Copy, Debug)]
struct Step {
    seq: u32,
    dir: Dir,
    to: TilePos,
    moves: [Option<(TilePos, Dir)>; 2],
}

#[derive(Resource, Debug, Default)]
pub struct Prediction {
    /// The own mob the state below belongs to.
    entity: Option<Entity>,
    /// The newest server state of the own mob.
    auth: Option<MoveAck>,
    /// `seq` of the last step sent on this connection.
    last_seq: u32,
    pending: VecDeque<Step>,
    pos: Option<TilePos>,
    facing: Option<Dir>,
    ready_at: f64,
    /// Cardinal moves (from, direction) the server refused since its mob last changed tile.
    blocked: HashSet<(TilePos, Dir)>,
    corrections: u32,
}

impl Prediction {
    /// Where the own mob is drawn: the server's state plus the unacked steps.
    pub fn predicted(&self) -> Option<TilePos> {
        self.pos
    }

    /// The newest state the server acknowledged.
    pub fn authoritative(&self) -> Option<MoveAck> {
        self.auth
    }

    /// Steps sent and not acked yet.
    pub fn unacked(&self) -> usize {
        self.pending.len()
    }

    /// Acks that disagreed with the step's prediction (blocked steps, server-side moves).
    pub fn corrections(&self) -> u32 {
        self.corrections
    }

    /// Server state plus the unacked steps; their predicted outcome is refreshed on the way.
    /// `walls`: what the replicated blockers refuse.
    fn replay(&mut self, walls: &dyn Fn(TilePos, Dir) -> bool) {
        let Some(auth) = self.auth else {
            return;
        };
        let (mut pos, mut facing) = (auth.pos, auth.facing);
        let learnt = &self.blocked;
        let blocked = |at: TilePos, d: Dir| learnt.contains(&(at, d)) || walls(at, d);
        for step in &mut self.pending {
            (*step, facing) = predict(pos, facing, step.seq, step.dir, &blocked);
            pos = step.to;
        }
        (self.pos, self.facing) = (Some(pos), facing);
    }
}

/// One step from `pos` by the `ss13_world` rules, with unknown moves taken as free.
fn predict(
    pos: TilePos,
    facing: Option<Dir>,
    seq: u32,
    dir: Dir,
    blocked: &dyn Fn(TilePos, Dir) -> bool,
) -> (Step, Option<Dir>) {
    let (mut at, mut face) = (pos, facing);
    let mut moves = [None; 2];
    // Things without `Facing` never get one (`ss13_world::movement::set_facing`).
    let turn = |d: Dir, face: &mut Option<Dir>| {
        if face.is_some() {
            *face = Some(d);
        }
    };
    let turn_to = resolve_move(dir, |d| {
        turn(d, &mut face);
        if blocked(at, d) {
            return false;
        }
        moves[usize::from(moves[0].is_some())] = Some((at, d));
        at = at.step(d);
        true
    });
    if let Some(d) = turn_to {
        turn(d, &mut face);
    }
    (
        Step {
            seq,
            dir,
            to: at,
            moves,
        },
        face,
    )
}

/// The replicated blockers' verdict on a cardinal move of `mover` from `at` in `d`: `ss13_world`'s
/// `step` (`exit_blockers`, then `enter_blockers`, tables walked across from a table).
fn index_blocks(
    index: &SpatialIndex,
    climbable: &dyn Fn(Entity) -> bool,
    mover: Entity,
    at: TilePos,
    d: Dir,
) -> bool {
    if index.exit_blockers(at, d, mover).next().is_some() {
        return true;
    }
    let on_climbable = index.on_tile(at).iter().any(|&e| climbable(e));
    index
        .enter_blockers(at.step(d), d, mover)
        .any(|e| !(on_climbable && climbable(e)))
}

/// What the systems need to ask [`index_blocks`].
#[derive(bevy::ecs::system::SystemParam)]
struct Walls<'w, 's> {
    index: Option<Res<'w, SpatialIndex>>,
    climbable: Query<'w, 's, (), With<ClimbWalkable>>,
    phasing: Option<Res<'w, OwnPhasing>>,
}

impl Walls<'_, '_> {
    fn check(&self, mover: Option<Entity>) -> impl Fn(TilePos, Dir) -> bool + '_ {
        let index = self
            .index
            .as_deref()
            .filter(|_| !self.phasing.as_deref().is_some_and(|p| p.0));
        let mover = mover.unwrap_or(Entity::PLACEHOLDER);
        move |at, d| {
            index.is_some_and(|i| index_blocks(i, &|e| self.climbable.contains(e), mover, at, d))
        }
    }
}

/// A new own mob (or none) starts from its replicated state; a new connection from scratch.
fn track_control(
    map: Res<NetEntityMap>,
    net: Res<NetClientState>,
    positions: Query<(&TilePos, Option<&Facing>)>,
    mut state: ResMut<Prediction>,
    walls: Walls,
) {
    if net.is_changed() && *net != NetClientState::Connected {
        *state = Prediction::default();
    }
    let controlled = map.controlled();
    if controlled == state.entity {
        return;
    }
    // shortcut: an ack that arrived for a previous mob is kept for a new one; per-mob acks are
    // needed once a client can switch mobs (ghosting, possession).
    let last_seq = state.last_seq;
    let auth = state.auth;
    *state = Prediction {
        entity: controlled,
        last_seq,
        ..default()
    };
    let Some((pos, facing)) = controlled.and_then(|e| positions.get(e).ok()) else {
        return;
    };
    state.auth = Some(auth.unwrap_or(MoveAck {
        seq: last_seq,
        pos: *pos,
        facing: facing.map(|f| f.0),
    }));
    state.replay(&walls.check(controlled));
}

fn reconcile(
    mut acks: MessageReader<MoveAckReceived>,
    mut state: ResMut<Prediction>,
    walls: Walls,
) {
    let mut any = false;
    for &MoveAckReceived(ack) in acks.read() {
        any = true;
        if state.auth.is_none_or(|old| old.pos != ack.pos) {
            state.blocked.clear();
        }
        if let Some(step) = state.pending.iter().find(|s| s.seq == ack.seq).copied()
            && step.to != ack.pos
        {
            state.corrections += 1;
            debug!(
                "prediction: step {} {:?} predicted {:?}, server {:?}",
                step.seq, step.dir, step.to, ack.pos
            );
            for (from, dir) in step.moves.into_iter().flatten() {
                if from.step(dir) != ack.pos {
                    state.blocked.insert((from, dir));
                }
            }
        }
        state.pending.retain(|s| s.seq > ack.seq);
        state.auth = Some(ack);
    }
    if any && state.entity.is_some() {
        let entity = state.entity;
        state.replay(&walls.check(entity));
    }
}

fn predict_steps(
    input: Res<MoveInput>,
    delay: Res<OwnMoveDelay>,
    immobile: Res<OwnImmobile>,
    time: Res<Time>,
    mut state: ResMut<Prediction>,
    mut intents: MessageWriter<SendIntent>,
    walls: Walls,
) {
    let now = time.elapsed_secs_f64();
    let (Some(dir), Some(pos)) = (input.0, state.pos) else {
        return;
    };
    if state.entity.is_none()
        || immobile.0
        || now < state.ready_at
        || state.pending.len() >= MAX_UNACKED_STEPS
    {
        return;
    }
    let seq = state.last_seq + 1;
    let wall = walls.check(state.entity);
    let learnt = &state.blocked;
    let blocked = |at: TilePos, d: Dir| learnt.contains(&(at, d)) || wall(at, d);
    let (step, facing) = predict(pos, state.facing, seq, dir, &blocked);
    let diagonal_reached = !Dir::CARDINALS.contains(&dir) && step.to == pos.step(dir);
    state.ready_at = next_ready_at(
        state.ready_at,
        now,
        TICK_DURATION.as_secs_f64(),
        delay.0,
        diagonal_reached,
    );
    state.last_seq = seq;
    state.pending.push_back(step);
    (state.pos, state.facing) = (Some(step.to), facing);
    intents.write(SendIntent(ClientIntent::Move { dir, seq }));
}

/// Replication also writes the own mob's server `TilePos`/`Facing` (in `PreUpdate`); this puts
/// the prediction back before anything draws it. Writes only on a difference.
fn write_own_mob(
    state: Res<Prediction>,
    mobs: Query<(&TilePos, Option<&Facing>)>,
    mut commands: Commands,
) {
    let (Some(entity), Some(pos)) = (state.entity, state.pos) else {
        return;
    };
    let Ok((current, facing)) = mobs.get(entity) else {
        return;
    };
    let mut mob = commands.entity(entity);
    if *current != pos {
        mob.try_insert(pos);
    }
    if let Some(dir) = state.facing
        && facing.is_none_or(|f| f.0 != dir)
    {
        mob.try_insert(Facing(dir));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const Z: u16 = 1;

    fn at(x: i32, y: i32) -> TilePos {
        TilePos::new(x, y, Z)
    }

    fn state_at(pos: TilePos) -> Prediction {
        let mut state = Prediction {
            entity: Some(Entity::PLACEHOLDER),
            auth: Some(MoveAck {
                seq: 0,
                pos,
                facing: Some(Dir::South),
            }),
            ..default()
        };
        state.replay(&|_, _| false);
        state
    }

    /// Sends a step the way `predict_steps` does, without the cooldown.
    fn step(state: &mut Prediction, dir: Dir) -> u32 {
        let seq = state.last_seq + 1;
        let learnt = &state.blocked;
        let (step, facing) = predict(state.pos.unwrap(), state.facing, seq, dir, &|a, d| {
            learnt.contains(&(a, d))
        });
        state.last_seq = seq;
        state.pending.push_back(step);
        (state.pos, state.facing) = (Some(step.to), facing);
        seq
    }

    fn ack(state: &mut Prediction, seq: u32, pos: TilePos, facing: Dir) {
        let mut app = App::new();
        app.add_message::<MoveAckReceived>()
            .insert_resource(std::mem::take(state))
            .add_systems(Update, reconcile);
        app.world_mut().write_message(MoveAckReceived(MoveAck {
            seq,
            pos,
            facing: Some(facing),
        }));
        app.update();
        *state = app.world_mut().remove_resource::<Prediction>().unwrap();
    }

    #[test]
    fn diagonals_follow_the_world_rules() {
        let blocked: HashSet<(TilePos, Dir)> = HashSet::from_iter([(at(5, 5), Dir::North)]);
        let (s, face) = predict(at(5, 5), Some(Dir::South), 1, Dir::NorthEast, &|a, d| {
            blocked.contains(&(a, d))
        });
        assert_eq!((s.to, face), (at(6, 6), Some(Dir::North)));
        assert_eq!(
            s.moves,
            [Some((at(5, 5), Dir::East)), Some((at(6, 5), Dir::North))]
        );
        let blocked: HashSet<(TilePos, Dir)> = HashSet::from_iter([(at(5, 6), Dir::East)]);
        let (s, face) = predict(at(5, 5), Some(Dir::South), 1, Dir::NorthEast, &|a, d| {
            blocked.contains(&(a, d))
        });
        assert_eq!((s.to, face), (at(5, 6), Some(Dir::North)));
        let (s, face) = predict(at(5, 5), None, 1, Dir::NorthEast, &|_, _| false);
        assert_eq!((s.to, face), (at(6, 6), None));
    }

    /// A reception desk (panes facing south on y = 5, x 3..=7) and a railing facing north-east.
    /// The client predicts each step before the server runs it; they disagree only on the first
    /// contact with a refused edge, never again while it stays known.
    #[test]
    fn prediction_agrees_with_the_server_around_border_objects() {
        use ss13_world::{Border, Density, MoveRequest, Mover, SpatialIndex, WorldPlugin};

        let mut server = App::new();
        server
            .add_plugins((MinimalPlugins, WorldPlugin))
            .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
                TICK_DURATION,
            ));
        server
            .world_mut()
            .resource_mut::<SpatialIndex>()
            .create_level(Z, 20, 20)
            .unwrap();
        for x in 3..=7 {
            server
                .world_mut()
                .spawn((Border::facing(Dir::South, false), Density, at(x, 5)));
        }
        server
            .world_mut()
            .spawn((Border::facing(Dir::NorthEast, true), Density, at(10, 3)));
        let mob = server
            .world_mut()
            .spawn((at(5, 3), Mover::new(0.0), Density, Facing(Dir::South)))
            .id();

        let mut state = state_at(at(5, 3));
        // `false`: the first contact with a refused edge, the one step allowed to disagree.
        let script = [
            // Into the desk, then held against it.
            (Dir::North, true),
            (Dir::North, false),
            (Dir::North, true),
            // Along it (panes do not block sideways), around its west end, onto a pane tile
            // from the north (free) and out through the pane's edge (refused).
            (Dir::West, true),
            (Dir::West, true),
            (Dir::West, true),
            (Dir::North, true),
            (Dir::North, true),
            (Dir::East, true),
            (Dir::South, true),
            (Dir::South, false),
            (Dir::South, true),
            (Dir::North, true),
            // Over to the railing, onto it diagonally from the west (free).
            (Dir::East, true),
            (Dir::East, true),
            (Dir::East, true),
            (Dir::East, true),
            (Dir::East, true),
            (Dir::East, true),
            (Dir::South, true),
            (Dir::South, true),
            (Dir::SouthEast, true),
            // Out through both its edges: the north one is learnt first (the predicted second
            // half started from the tile north of it), the east one on the next try.
            (Dir::NorthEast, false),
            (Dir::NorthEast, false),
            (Dir::NorthEast, true),
            (Dir::North, true),
            (Dir::East, true),
            (Dir::SouthEast, true),
            // Back in through its east edge as a diagonal's second half: slides north.
            (Dir::NorthWest, false),
            (Dir::NorthWest, true),
        ];
        let mut mismatches = 0;
        for (i, (dir, expect_agree)) in script.into_iter().enumerate() {
            let seq = step(&mut state, dir);
            let predicted = state.predicted();
            server
                .world_mut()
                .write_message(MoveRequest { entity: mob, dir });
            server.update();
            let pos = *server.world().get::<TilePos>(mob).unwrap();
            let facing = server.world().get::<Facing>(mob).unwrap().0;
            let agree = predicted == Some(pos);
            mismatches += usize::from(!agree);
            assert_eq!(
                agree, expect_agree,
                "step {i} {dir:?}: predicted {predicted:?}, server {pos:?}"
            );
            ack(&mut state, seq, pos, facing);
            assert_eq!(state.predicted(), Some(pos), "step {i} after the ack");
        }
        assert_eq!(state.corrections(), mismatches as u32);
        assert_eq!(*server.world().get::<TilePos>(mob).unwrap(), at(10, 4));
    }

    #[test]
    fn acks_replay_unacked_steps() {
        let mut state = state_at(at(5, 5));
        for _ in 0..3 {
            step(&mut state, Dir::East);
        }
        assert_eq!(state.predicted(), Some(at(8, 5)));
        ack(&mut state, 1, at(6, 5), Dir::East);
        assert_eq!((state.predicted(), state.unacked()), (Some(at(8, 5)), 2));
        assert_eq!(state.corrections(), 0);
        ack(&mut state, 3, at(8, 5), Dir::East);
        assert_eq!((state.predicted(), state.unacked()), (Some(at(8, 5)), 0));
    }

    #[test]
    fn a_refused_step_is_learnt_until_the_server_moves() {
        let mut state = state_at(at(5, 5));
        // A wall at x = 7: the client walks into it and one step beyond before the ack.
        for _ in 0..3 {
            step(&mut state, Dir::East);
        }
        assert_eq!(state.predicted(), Some(at(8, 5)));
        ack(&mut state, 1, at(6, 5), Dir::East);
        ack(&mut state, 2, at(6, 5), Dir::East);
        assert_eq!(state.predicted(), Some(at(6, 5)), "corrected to the wall");
        assert_eq!(state.corrections(), 1);
        // Held into the wall: no more bounces.
        step(&mut state, Dir::East);
        assert_eq!(state.predicted(), Some(at(6, 5)));
        ack(&mut state, 3, at(6, 5), Dir::East);
        ack(&mut state, 4, at(6, 5), Dir::East);
        assert_eq!(state.corrections(), 1);
        // A door there opened: the server moves through, the client follows and predicts again.
        let s = step(&mut state, Dir::East);
        assert_eq!(state.predicted(), Some(at(6, 5)));
        ack(&mut state, s, at(7, 5), Dir::East);
        step(&mut state, Dir::East);
        assert_eq!(state.predicted(), Some(at(8, 5)));
    }

    #[test]
    fn a_server_move_without_a_step_is_followed() {
        let mut state = state_at(at(5, 5));
        step(&mut state, Dir::North);
        // Teleported, then the step applied from there.
        ack(&mut state, 0, at(20, 20), Dir::South);
        assert_eq!(state.predicted(), Some(at(20, 21)));
        ack(&mut state, 1, at(20, 21), Dir::North);
        assert_eq!((state.predicted(), state.unacked()), (Some(at(20, 21)), 0));
    }
}
